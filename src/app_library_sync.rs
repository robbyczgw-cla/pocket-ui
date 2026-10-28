//! Part of the pocket-ui binary: the App's side of `pocket_ui::library_sync` (Settings → Sync → Library data). A thread
//! merges this Pocket's curation with the player's other devices' (a while after the start, after each game, when
//! Settings → Sync opens or a row there is switched, and whenever this device's files or the folder changed, looked at
//! every minute; never while a game runs). What it finds is applied here, on the UI thread, like any other edit; only
//! then does the thread write this device's own folder (docs/SYNC-CURATION.md).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use pocket_ui::fsutil::{self, Stamp};
use pocket_ui::library_sync::{self as ls, Outcome};
use pocket_ui::sync::{self, Prefs};
use pocket_ui::sync_page::Event;

use super::{App, UserEvent};

/// The first pass, once the start's own reading is done.
const FIRST_LOOK: Duration = Duration::from_secs(15);
/// How often the thread looks whether anything changed (a few `stat`s; a pass only when something did).
const EVERY: Duration = Duration::from_secs(60);
/// After a game, a second pass once Syncthing had time to bring what the other devices changed meanwhile.
const AFTER_GAME_AGAIN: Duration = Duration::from_secs(30);
/// A result the UI never took (it can't happen, but the thread must not wait for ever): done again after this.
const GIVE_UP: Duration = Duration::from_secs(120);

enum Msg {
    /// Look (again) in `after`, with these choices; `history`: Settings → Curator → Play history is on.
    Look { prefs: Prefs, history: bool, after: Duration },
    /// A game started (true) or ended: no pass while one runs.
    Playing(bool),
    /// The UI applied the result of this pass: its files may be written.
    Applied(u64),
}

static WATCH: OnceLock<Mutex<mpsc::Sender<Msg>>> = OnceLock::new();

fn send(m: Msg) {
    if let Some(tx) = WATCH.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(m);
        }
    }
}

/// What a pass looks at: this device's files and every file of the folder. Unchanged since the last pass: no pass.
fn stamps(data_dir: &Path, root: &Path) -> Vec<(PathBuf, Option<Stamp>)> {
    let mut files: Vec<PathBuf> = ["library.json", "favorites.json", "curator.json", pocket_ui::play_log::FILE]
        .iter()
        .map(|f| data_dir.join(f))
        .collect();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for d in dirs {
        files.push(d.join(ls::STATE_FILE));
        files.push(d.join(ls::HISTORY_FILE));
    }
    files
        .into_iter()
        .map(|f| {
            let s = fsutil::stamp(&f);
            (f, s)
        })
        .collect()
}

/// The library's games by key, from the game list the start keeps (`library_cache`); empty when there is none.
fn library_index(data_dir: &Path, es_systems: &Path) -> std::collections::HashMap<String, Option<PathBuf>> {
    let systems = pocket_ui::library_cache::Key::now(es_systems)
        .and_then(|k| pocket_ui::library_cache::read(data_dir, &k).ok())
        .unwrap_or_default();
    ls::index(systems.iter().flat_map(|s| s.games.iter().map(|g| g.path.as_path())))
}

/// This device's folder: from its Syncthing device ID (None while Syncthing has none to read).
fn me() -> Option<String> {
    sync::Config::read().ok().and_then(|c| ls::device_folder(&c.device_id))
}

/// The thread: a pass when asked and when something changed, never while a game runs; a result with changes waits
/// for the UI before this device's folder is written.
fn watch(rx: mpsc::Receiver<Msg>, data_dir: PathBuf, es_systems: PathBuf, report: impl Fn(Box<Outcome>)) {
    let root = ls::root(&data_dir);
    let mut prefs = Prefs::load(&data_dir);
    let mut history = pocket_ui::curator::prefs::Prefs::load(&data_dir).history;
    let mut due = vec![Instant::now() + FIRST_LOOK];
    let (mut playing, mut shown) = (false, false);
    let mut seen: Option<Vec<(PathBuf, Option<Stamp>)>> = None;
    // The pass whose changes the UI is applying, with when it was handed over.
    let mut waiting: Option<(Outcome, String, Instant)> = None;
    let mut number = 0u64;
    let mut problems: Vec<String> = Vec::new();
    loop {
        let now = Instant::now();
        let asked_at = due.iter().min().copied();
        match rx.recv_timeout(asked_at.unwrap_or(now + EVERY).saturating_duration_since(now).min(EVERY)) {
            Ok(Msg::Look { prefs: p, history: h, after }) => {
                (prefs, history) = (p, h);
                due.push(Instant::now() + after);
                continue;
            }
            Ok(Msg::Playing(on)) => {
                playing = on;
                continue;
            }
            Ok(Msg::Applied(n)) => {
                if let Some((out, me, _)) = waiting.take_if(|(o, _, _)| o.number == n) {
                    if let Err(e) = ls::store(&root, &me, &out) {
                        println!("library sync: {}: {e}", root.join(&me).display());
                    }
                }
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        let asked = asked_at.is_some_and(|a| a <= now);
        due.retain(|d| *d > now);
        if playing {
            continue;
        }
        if !ls::wanted(&prefs) {
            // Switched off: the other devices' sessions leave the pages; nothing in the folder is read or written.
            if shown {
                pocket_ui::play_log::set_extra(None);
                shown = false;
            }
            (seen, waiting) = (None, None);
            continue;
        }
        if waiting.as_ref().is_some_and(|(_, _, at)| at.elapsed() < GIVE_UP) {
            continue;
        }
        let stamps = stamps(&data_dir, &root);
        if !asked && seen.as_ref() == Some(&stamps) {
            continue;
        }
        let Some(me) = me() else {
            println!("library sync: no Syncthing device ID yet; tried again later");
            continue;
        };
        let t = Instant::now();
        let log = if history { pocket_ui::play_log::own_log(&data_dir) } else { None };
        let index = std::cell::OnceCell::new();
        let find = |key: &str| index.get_or_init(|| library_index(&data_dir, &es_systems)).get(key).cloned().flatten();
        let mut out = ls::pass(&ls::Setup {
            data_dir: &data_dir,
            root: &root,
            me: &me,
            now: ls::now_ms(),
            log,
            find: &find,
        });
        seen = Some(stamps);
        // A file another device left damaged is said once in the log, never on screen.
        if out.problems != problems {
            for p in &out.problems {
                println!("library sync: skipped {p}");
            }
            problems = out.problems.clone();
        }
        pocket_ui::play_log::set_extra(out.extra.take());
        shown = true;
        println!(
            "library sync: {} other devices, {} changes, {:?}",
            out.devices.len(),
            out.changes.len(),
            t.elapsed()
        );
        if out.changes.is_empty() {
            if let Err(e) = ls::store(&root, &me, &out) {
                println!("library sync: {}: {e}", root.join(&me).display());
            }
            continue;
        }
        number += 1;
        out.number = number;
        let keep = Outcome {
            number,
            state: out.state.clone(),
            history: out.history.clone(),
            state_changed: out.state_changed,
            history_changed: out.history_changed,
            ..Outcome::default()
        };
        waiting = Some((keep, me, Instant::now()));
        report(Box::new(out));
    }
}

impl App {
    /// After the first frame: the thread that merges Library data (it waits a while before its first pass, and does
    /// nothing while the row is off).
    pub(crate) fn library_sync_start(&mut self) {
        let (tx, rx) = mpsc::channel();
        if WATCH.set(Mutex::new(tx)).is_err() {
            return;
        }
        let (proxy, data_dir, es) = (self.proxy.clone(), self.data_dir.clone(), self.es_systems.clone());
        let report = move |out| {
            let _ = proxy.send_event(UserEvent::Sync(Event::Library(out)));
        };
        let spawned = std::thread::Builder::new().name("library-sync".into()).spawn(move || {
            #[cfg(target_os = "linux")]
            // SAFETY: setpriority on this thread's own id only changes its scheduling.
            unsafe {
                let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
                libc::setpriority(libc::PRIO_PROCESS, tid, 10);
            }
            watch(rx, data_dir, es, report)
        });
        if spawned.is_err() {
            eprintln!("library sync: no thread; Library data is not merged this time");
        }
    }

    /// A pass now (and, after a game, a second one a little later). Returns at once.
    pub(crate) fn library_sync_look(&self, prefs: &Prefs, after_game: bool) {
        let history = self.curator.prefs.history;
        send(Msg::Playing(false));
        send(Msg::Look {
            prefs: prefs.clone(),
            history,
            after: Duration::ZERO,
        });
        if after_game {
            send(Msg::Look {
                prefs: prefs.clone(),
                history,
                after: AFTER_GAME_AGAIN,
            });
        }
    }

    /// A game starts: no pass until it ends.
    pub(crate) fn library_sync_playing(&self) {
        send(Msg::Playing(true));
    }

    /// A pass found changes from the other devices: applied like the Pocket's own edits (kept for after the game when
    /// one runs), then the thread may write this device's folder.
    pub(crate) fn library_merged(&mut self, out: Box<Outcome>) {
        if self.running.is_some() {
            self.sync.library_waiting = Some(out);
            return;
        }
        let (applied, saved) = ls::apply(&out.changes, &mut self.library, &mut self.favorites, &mut self.curator.prefs);
        println!("library sync: {} applied, {} changed here meanwhile", applied.applied, applied.skipped);
        let mut ok = saved.map_err(|e| eprintln!("library sync: not saved: {e}")).is_ok();
        if applied.applied > 0 {
            self.curator.touch();
            if applied.shelves {
                ok &= self.curator_prefs_changed().is_ok();
            }
            if applied.hidden {
                // Hidden or shown on another device: the lists are read again on a thread.
                self.library_read_shown_again();
            }
            self.web_library_changed();
            self.rebuild();
            self.request_redraw();
        }
        // Not saved: this device's folder stays as it was, and the next pass finds the same changes.
        if ok {
            send(Msg::Applied(out.number));
        }
    }

    /// A game ended: a result that came while it ran is applied now.
    pub(crate) fn library_sync_after_game(&mut self) {
        if let Some(out) = self.sync.library_waiting.take() {
            self.library_merged(out);
        }
    }
}
