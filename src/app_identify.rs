//! Part of the pocket-ui binary: the games recognised by their checksum (`pocket_ui::rom_id`) and the game card's
//! "Wrong pictures? Look up again".
//!
//! The run is a thread of the lowest CPU and I/O priority, started where the automatic covers start (`auto_covers`:
//! after the first frame, a library read again, Wi-Fi up, after a game); covers and descriptions wait for it, so they
//! ask under the names it found. A game that starts stops it after the file it is on; it goes on after the game.
//! Each file is read once (the cache in `<data>/identify/`), so after the first run it takes milliseconds.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::rom_id::{self, Again, Identified, Names};

use super::{App, UserEvent};

/// After the first frame the run waits this long, so the start's own reading of the card comes first.
const START_DELAY: Duration = Duration::from_secs(3);

/// The result line stays this much longer than a plain toast: it can hold a game's whole name.
const RESULT_EXTRA: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(crate) enum IdentifyEvent {
    Done(Identified),
    /// "Look up again" for this game came out so; `name` is what its checksum gave.
    LookedUp { path: PathBuf, name: Option<String>, outcome: Again },
}

#[derive(Default)]
pub(crate) struct Identify {
    running: bool,
    /// Something may have changed since the last run: the start, the library, a run a game cut short.
    due: bool,
    /// Asked for again while it ran: once more when it is through.
    again: bool,
    cancel: Arc<AtomicBool>,
    /// The last run lacked a system's list, and it ran offline: online, it tries again.
    lists_missing_offline: bool,
    /// The names the last run gave, by ROM path.
    pub names: Arc<Names>,
    /// "Look up again" runs for this game.
    looking: Option<PathBuf>,
    /// The games (system id, ROM path) the last run was given: another list is a reason to run.
    last_games: Vec<(String, PathBuf)>,
}

impl Identify {
    pub(crate) fn new() -> Self {
        Identify { due: true, ..Default::default() }
    }

    /// A game starts: a run stops after the file it is on.
    pub(crate) fn game_started(&mut self) {
        if self.running {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// There is something to go on with after a game.
    pub(crate) fn due(&self) -> bool {
        self.due && !self.running
    }
}

impl App {
    /// Starts recognising the library's games by their checksum when something may have changed (`why`); true while
    /// it runs, so the covers and descriptions wait for its names (it calls `auto_covers` again when done).
    pub(crate) fn identify(&mut self, why: &str) -> bool {
        let i = &mut self.identify;
        // "Look up again" writes the same cache: after it.
        if i.running || i.looking.is_some() {
            i.again = true;
            return true;
        }
        // Pictures: Off fetches no lists either, nor does the first start before it is past the step where Pictures is
        // chosen (the lists are only there to find pictures under the right names); the names then come from the files.
        let allowed = !self.art_off && self.setup.as_ref().is_none_or(|s| s.past_wifi());
        let online = pocket_ui::wifi::ip().is_some() && allowed;
        if self.running.is_some() {
            return false;
        }
        let games = self.all_games();
        let i = &mut self.identify;
        if !(i.due || games != i.last_games || (i.lists_missing_offline && online)) {
            return false;
        }
        i.running = true;
        i.due = false;
        i.again = false;
        i.cancel = Arc::new(AtomicBool::new(false));
        let stop = i.cancel.clone();
        let at_start = why == "at start";
        i.last_games = games.clone();
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        println!("identify ({why}): {} games, {}", games.len(), if online { "online" } else { "offline" });
        std::thread::spawn(move || {
            pocket_ui::web::lower_priority();
            if at_start {
                std::thread::sleep(START_DELAY);
            }
            let t = Instant::now();
            let fetch = |dat: &str| online.then(|| rom_id::fetch_dat(dat)).flatten();
            let mut run = rom_id::run_until(&data_dir, &games, &fetch, true, || stop.load(Ordering::Relaxed));
            // Not reached offline: says to try again online (`lists_missing_offline`).
            run.lists_missing &= !online;
            println!("identify: {} named, {} read in {:?}{}", run.names.len(), run.hashed, t.elapsed(), if run.stopped { ", stopped for a game" } else { "" });
            let _ = proxy.send_event(UserEvent::Identify(IdentifyEvent::Done(run)));
        });
        true
    }

    pub(crate) fn identify_event(&mut self, e: IdentifyEvent) {
        match e {
            IdentifyEvent::Done(run) => {
                let i = &mut self.identify;
                i.running = false;
                i.due |= run.stopped;
                i.lists_missing_offline = run.lists_missing;
                let changed = *i.names != run.names;
                i.names = Arc::new(run.names);
                if changed {
                    self.names_changed();
                }
                if std::mem::take(&mut self.identify.again) {
                    self.identify.due = true;
                }
                // The covers and descriptions waited for the names (or this runs once more first). Not after a run
                // a game stopped: the game runs.
                if !run.stopped {
                    self.auto_covers("games recognised");
                }
            }
            IdentifyEvent::LookedUp { path, name, outcome } => {
                self.identify.looking = None;
                let mut names = (*self.identify.names).clone();
                let changed = match &name {
                    Some(n) => names.insert(path.clone(), n.clone()).as_ref() != Some(n),
                    None => names.remove(&path).is_some(),
                };
                self.identify.names = Arc::new(names);
                if changed {
                    self.names_changed();
                }
                // The new pictures show at once, on the card too.
                self.covers.clear();
                self.pictures_changed();
                self.cabinet.covers_changed();
                self.look_previews = None;
                if let Some(open) = self.card.as_ref().filter(|o| o.path == path) {
                    let (item, system, game) = (open.item, open.system.clone(), open.name.clone());
                    self.game_infos.remove(&path);
                    self.show_card(item, system, game, path);
                }
                self.toast = Some((outcome.message(), Instant::now() + RESULT_EXTRA));
                self.request_redraw();
                // A run asked for meanwhile, then the covers.
                if std::mem::take(&mut self.identify.again) {
                    self.identify.due = true;
                    self.auto_covers("after look up again");
                }
            }
        }
    }

    /// The names changed: the card's facts and Curator's catalog follow them.
    fn names_changed(&mut self) {
        self.game_infos.clear();
        self.curator.library_changed();
    }

    /// "Wrong pictures? Look up again" on the card: the game's pictures, `.missing` lines and checksum go and it is
    /// looked up afresh, checksum first, then name (`rom_id::forget`). Online only, and not while a download runs.
    pub(crate) fn card_look_up_again(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (system, path) = (open.system.clone(), open.path.clone());
        let refused = if pocket_ui::wifi::ip().is_none() {
            Some(Again::Offline)
        } else if self.identify.running || self.identify.looking.is_some() {
            Some(Again::Busy)
        } else {
            None
        };
        if let Some(why) = refused {
            self.toast = Some((why.message(), Instant::now()));
            return;
        }
        let Some(claim) = pocket_ui::covers::claim() else {
            self.toast = Some((Again::Busy.message(), Instant::now()));
            return;
        };
        println!("look up again: {}", path.display());
        self.identify.looking = Some(path.clone());
        self.toast = Some((Again::Working.message(), Instant::now()));
        let pictures = self.pictures.wanted();
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        std::thread::spawn(move || {
            let _claim = claim;
            let (name, outcome) = look_up_again(&data_dir, &system, &path, pictures);
            println!("look up again: {} -> {outcome:?}", path.display());
            let _ = proxy.send_event(UserEvent::Identify(IdentifyEvent::LookedUp { path, name, outcome }));
        });
    }

    /// `--snapshot curator:card:<system>:<name>:lookup=<case>`: the card with the line "Look up again" ends with
    /// (`working`, `offline`, `busy`, `found`, `named`, `nothing`, `unreachable`).
    pub(crate) fn look_up_snapshot(&mut self, state: &str) {
        let Some(case) = state.split(':').find_map(|p| p.strip_prefix("lookup=")) else { return };
        let name = Some("Castlevania - Aria of Sorrow (USA) (Virtual Console)".to_string());
        let outcome = match case {
            "working" => Again::Working,
            "offline" => Again::Offline,
            "busy" => Again::Busy,
            "found" => Again::Found { pictures: 3, name: None },
            "named" => Again::Found { pictures: 3, name },
            "nothing" => Again::NothingFound { name },
            _ => Again::Unreachable,
        };
        self.toast = Some((outcome.message(), Instant::now() + RESULT_EXTRA));
    }
}

/// The thread's part of "Look up again": forget, recognise again (the list fetched if the card lacks it), then the box
/// art and, when wanted, the screenshot and title screen.
fn look_up_again(data_dir: &std::path::Path, system: &str, path: &std::path::Path, pictures: bool) -> (Option<String>, Again) {
    use pocket_ui::game_pictures;
    rom_id::forget(data_dir, system, path);
    let games = [(system.to_string(), path.to_path_buf())];
    let name = rom_id::run_until(data_dir, &games, &rom_id::fetch_dat, false, || false).names.remove(path);
    let run = game_pictures::run_until(&game_pictures::Libretro, data_dir, &games, pictures.then_some(&games[..]), |_, _, _| {}, || false);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let outcome = if run.found > 0 {
        Again::Found { pictures: run.found, name: name.clone() }
    } else if pocket_ui::covers::listed_missing(data_dir, system).contains(stem) {
        // The collection answered: it has nothing under either name.
        Again::NothingFound { name: name.clone() }
    } else {
        Again::Unreachable
    };
    (name, outcome)
}
