//! Part of the pocket-ui binary: Frozen games (`pocket_ui::frozen`, Settings → System → Frozen games). Which recent
//! games stand open is read on a thread after the start and after each game; the start of one is the normal `launch`,
//! only without the Cozy wait, and a game whose moment can't be used says so. The cases' look is in `app_cabinet`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use pocket_ui::frozen::{self, AtLaunch, Mark};
use pocket_ui::gfx::Canvas;
use pocket_ui::palette::*;
use pocket_ui::text::Face;

use super::{host_bin, standalone, App, Item, Running, UserEvent};

/// How often the spinner moves while a big emulator loads: enough to see it live, no work to speak of beside the game.
const SPIN_STEP: Duration = Duration::from_millis(125);
/// It gives up with `standalone`'s wait for the emulator.
const SPIN_FOR: Duration = Duration::from_secs(120);

/// Available on this device (there is no switch: an open case always means a moment is waiting).
fn wanted() -> bool {
    frozen::available()
}

impl App {
    fn frozen_game(&self, r: &pocket_ui::recents::Recent) -> Option<frozen::Game> {
        let sys = self.systems.iter().find(|s| s.name == r.system)?;
        let (emulator, core) = self.route(sys, Some(&r.path), Path::new(host_bin()).exists());
        Some(frozen::Game { system: r.system.clone(), path: r.path.clone(), emulator, core, mark: r.frozen.clone() })
    }

    /// The other installed games of this system with its file name, hidden ones too: they share its states.
    pub(crate) fn same_name_games(&self, system: &str, path: &Path) -> Vec<PathBuf> {
        let shown = self.systems.iter().filter(|s| s.name == system).flat_map(|s| s.games.iter()).map(|g| g.path.as_path());
        // Asked only for a hidden game of this name; a system that can't be told counts as this one, the safe side.
        let in_system = |p: &Path| {
            let all = pocket_ui::library::all_systems(&self.es_systems).unwrap_or_default();
            all.iter().find(|s| s.id == system).is_none_or(|s| p.starts_with(&s.path))
        };
        frozen::twins(path, shown, self.library.hidden_games(), in_system)
    }

    /// Another game shares this one's states, and the moment on the card can't be shown to be this one's. Only the emulators
    /// that name their states after the file.
    pub(crate) fn shared_moment_not_own(&self, system: &str, path: &Path, emulator: &str) -> bool {
        let others = self.same_name_games(system, path);
        if others.is_empty() || !matches!(emulator, "pocket-core" | "retroarch") {
            return false;
        }
        let recent = |p: &Path| self.recents.list.iter().find(|r| r.path == p);
        let own = recent(path).map(|r| (r.last_played, r.frozen.is_some()));
        !frozen::shared_moment_is_own(own, others.iter().map(|p| recent(p).map(|r| r.last_played)))
    }

    /// When the moment a start of this game would continue from was written, if the shelf's own read found one: the open
    /// case's (`frozen_refresh`), or Easy's and the big shelf's (`big_moments_refresh`) while that look is on. Nothing is
    /// read here: a game that read doesn't list, or hasn't listed yet, has no line.
    pub(crate) fn frozen_moment_time(&self, path: &Path) -> Option<u64> {
        self.moment_reads.time(self.big_on(), path)
    }

    /// Reads again which games stand open: on a thread, the answer comes as an event.
    pub(crate) fn frozen_refresh(&mut self) {
        self.big_moments_refresh();
        if !wanted() {
            FROZEN_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.moment_reads.shelf_started(false);
            return self.frozen_got(Vec::new());
        }
        self.moment_reads.shelf_started(true);
        let games: Vec<frozen::Game> = self.recents.list.iter().filter(|r| self.recent_visible(r)).filter_map(|r| self.frozen_game(r)).filter(|g| !self.shared_moment_not_own(&g.system, &g.path, &g.emulator)).collect();
        let proxy = self.proxy.clone();
        let gen = FROZEN_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        std::thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::Frozen(gen, frozen::scan_timed(&games)));
        });
    }

    /// The answer of a read: the open games and the times of their moments.
    pub(crate) fn frozen_got_timed(&mut self, list: Vec<frozen::Moment>) {
        let paths = list.iter().map(|m| m.0.clone()).collect();
        self.moment_reads.shelf_got(list.clone());
        self.frozen_got(paths);
    }

    pub(crate) fn frozen_got(&mut self, list: Vec<PathBuf>) {
        if list != self.frozen {
            let first = self.frozen.is_empty() && !list.is_empty();
            self.frozen = list;
            self.cabinet_frozen_changed();
            // The first time an open case is on screen: one line says what it means (once ever, not in Kids Mode).
            if first && self.running.is_none() && self.setup.is_none() && self.settings_page.is_none() && pocket_ui::first_game::tell_open_case_once(&self.data_dir) {
                self.toast = Some((pocket_ui::first_game::OPEN_CASE_NOTE.into(), Instant::now()));
                self.request_redraw();
            }
        }
    }

    pub(crate) fn is_frozen_path(&self, path: &Path) -> bool {
        self.frozen.iter().any(|p| p == path)
    }

    /// Whether this entry of the shelf is a frozen game: on Home's recent row, and where the same game is on its system's.
    pub(crate) fn is_frozen_item(&self, item: Item) -> bool {
        if self.frozen.is_empty() {
            return false;
        }
        match item {
            Item::Recent(i) => self.recents.list.get(i).is_some_and(|r| self.is_frozen_path(&r.path)),
            Item::Game(s, g) => self.systems.get(s).and_then(|s| s.games.get(g)).is_some_and(|g| self.is_frozen_path(&g.path)),
            _ => false,
        }
    }

    /// A session ended (before the recent games take it, `run` is its entry): what its state was written with.
    pub(crate) fn frozen_session_ended(&mut self, run: &Running) {
        let auto = self.frozen_auto.take();
        let Some(entry) = self.recents.list.first_mut().filter(|r| r.path == run.path) else { return };
        let started = SystemTime::now() - run.started.elapsed();
        let (emulator, core) = (run.emulator.clone(), run.core.clone());
        // ARMSX2's names are known only while it runs: `standalone::states` was asked before the session closed.
        let mut game = frozen::Game { system: run.system.clone(), path: run.path.clone(), emulator: emulator.clone(), core: core.clone(), mark: None };
        game.mark = auto.as_ref().and_then(|a| Mark::new(&emulator, &core, &run.path, Some(a.clone())));
        let written = frozen::resume_files(&game).iter().any(|f| std::fs::metadata(f).and_then(|m| m.modified()).is_ok_and(|t| t >= started));
        let previous = entry.frozen.take();
        entry.frozen = frozen::next_mark(previous, written, || Mark::new(&emulator, &core, &run.path, auto));
        self.frozen_refresh();
    }

    /// Before the session closes: the running game's auto state, when it has a name of its own only now (ARMSX2).
    pub(crate) fn frozen_note_session(&mut self, run: &Running) {
        self.frozen_auto = if run.emulator == "pocket-core" { None } else { standalone::states().and_then(|s| s.auto.first().cloned()) };
    }

    /// `launch`: what the start does about the game's moment. Goes on the start's environment for pocket-core, on the
    /// shelf for an emulator that takes longer to come up.
    /// True when the moment is left out: the game starts fresh ("Where was I?" shows nothing then, `where_was_i`).
    pub(crate) fn frozen_at_launch(&mut self, path: &Path, hosted: bool, at: AtLaunch, wait: bool, env: &mut Vec<(&'static str, String)>) -> bool {
        self.frozen_wait = None;
        let (note, fresh) = match at {
            AtLaunch::Normal => {
                // pocket-core is there in a moment; a big emulator takes as long as it takes, and says that it is on it.
                if wait {
                    self.frozen_wait = Some(Instant::now());
                }
                return false;
            }
            AtLaunch::Note(note) => (note, false),
            AtLaunch::Fresh(note) => {
                if hosted {
                    env.push((frozen::SKIP_RESUME_ENV, "1".into()));
                }
                (note, true)
            }
        };
        println!("frozen: {}: {note}", path.display());
        if hosted {
            env.push((frozen::NOTE_ENV, note.into()));
        } else {
            // A notice, not a toast: these are sentences, and a toast is one line for a moment.
            self.notice = Some(super::Notice::new(note));
        }
        fresh
    }

    /// The next spinner step while a big emulator loads the moment, and until when.
    pub(crate) fn frozen_wait_wake(&mut self, now: Instant) -> Option<Instant> {
        let since = self.frozen_wait?;
        if self.running.is_none() || !standalone::resuming() || now.duration_since(since) > SPIN_FOR {
            self.frozen_wait = None;
            return None;
        }
        let steps = now.duration_since(since).as_millis() / SPIN_STEP.as_millis() + 1;
        Some(since + SPIN_STEP * steps as u32)
    }

    /// While a big emulator loads the moment: a small spinner and what it is doing, over the launch's picture. With
    /// Motion off the dots stand still.
    pub(crate) fn paint_frozen_wait(&mut self, c: &mut Canvas) {
        let Some(since) = self.frozen_wait else { return };
        let turn = if pocket_ui::motion::level() == pocket_ui::motion::Motion::Off { 0 } else { (since.elapsed().as_millis() / SPIN_STEP.as_millis()) as i32 };
        let word = "Going back to your moment…";
        let w = self.text.width(Face::Body, 28, word);
        let (x, y) = ((c.w - w + 44) / 2, c.h - 72);
        spinner(c, x - 28, y - 10, turn);
        self.text.draw(c, Face::Body, 28, x, y, pal().chalk, word);
    }

    /// The little word that stands for the open case in the looks without cases, its right end at `right`.
    pub(crate) fn paint_frozen_word(&mut self, c: &mut Canvas, right: i32, base: i32, ink: u32) {
        self.text.draw_right(c, Face::Body, 21, right, base, ink, FROZEN);
    }

    pub(crate) fn frozen_word_width(&mut self) -> i32 {
        self.text.width(Face::Body, 21, FROZEN)
    }

    /// `--snapshot` with POCKETUI_SNAPSHOT_FROZEN=<n>: the first n recent games stand open, whatever is on the card here.
    pub(crate) fn frozen_snapshot(&mut self) {
        if let Some(n) = std::env::var("POCKETUI_SNAPSHOT_FROZEN").ok().and_then(|n| n.parse::<usize>().ok()) {
            let list = if wanted() { self.recents.list.iter().take(n.min(frozen::MOST)).map(|r| r.path.clone()).collect() } else { Vec::new() };
            self.frozen_got(list);
            self.moved_at = Instant::now() - Duration::from_secs(5);
        }
    }
}

/// The word itself, one place for the looks that show it.
/// The newest read of which games stand open; the answer of an older one that comes late is dropped.
static FROZEN_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl App {
    pub(crate) fn frozen_generation(&self) -> u64 {
        FROZEN_GEN.load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub(crate) const FROZEN: &str = "Continue";

/// Eight dots round a point, one of them lit, the two behind it fading.
pub(crate) fn spinner(c: &mut Canvas, cx: i32, cy: i32, turn: i32) {
    const DOTS: [(i32, i32); 8] = [(0, -12), (8, -8), (12, 0), (8, 8), (0, 12), (-8, 8), (-12, 0), (-8, -8)];
    for (i, (dx, dy)) in DOTS.iter().enumerate() {
        let behind = (turn - i as i32).rem_euclid(8);
        let ink = pocket_ui::gfx::mix(pal().chalk, pal().void, match behind {
            0 => 0,
            1 => 90,
            2 => 150,
            _ => 210,
        });
        c.round_rect(cx + dx - 2, cy + dy - 2, 5, 5, 2, ink);
    }
}
