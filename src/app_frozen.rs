//! Part of the pocket-ui binary: Frozen games (`pocket_ui::frozen`, Settings → System → Frozen games). Which recent
//! games stand open is read on a thread after the start and after each game; the start of one is the normal `launch`,
//! only without the Cozy wait, and a game whose moment can't be used says so. The cases' look is in `app_cabinet`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use pocket_ui::cozy_extras::{self, Extra};
use pocket_ui::frozen::{self, AtLaunch, Mark};
use pocket_ui::gfx::Canvas;
use pocket_ui::palette::*;
use pocket_ui::text::Face;

use super::{host_bin, standalone, App, Item, Running, UserEvent};

/// How often the spinner moves while a big emulator loads: enough to see it live, no work to speak of beside the game.
const SPIN_STEP: Duration = Duration::from_millis(125);
/// It gives up with `standalone`'s wait for the emulator.
const SPIN_FOR: Duration = Duration::from_secs(120);

/// On the Frozen games switch and available on this device.
fn wanted() -> bool {
    cozy_extras::on(Extra::FrozenGames) && frozen::available()
}

impl App {
    fn frozen_game(&self, r: &pocket_ui::recents::Recent) -> Option<frozen::Game> {
        let sys = self.systems.iter().find(|s| s.name == r.system)?;
        let (emulator, core) = self.route(sys, Some(&r.path), Path::new(host_bin()).exists());
        Some(frozen::Game { system: r.system.clone(), path: r.path.clone(), emulator, core, mark: r.frozen.clone() })
    }

    /// Reads again which games stand open: on a thread, the answer comes as an event.
    pub(crate) fn frozen_refresh(&mut self) {
        if !wanted() {
            FROZEN_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return self.frozen_got(Vec::new());
        }
        let games: Vec<frozen::Game> = self.recents.list.iter().filter(|r| self.recent_visible(r)).filter_map(|r| self.frozen_game(r)).collect();
        let proxy = self.proxy.clone();
        let gen = FROZEN_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        std::thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::Frozen(gen, frozen::scan(&games)));
        });
    }

    pub(crate) fn frozen_got(&mut self, list: Vec<PathBuf>) {
        if list != self.frozen {
            self.frozen = list;
            self.cabinet_frozen_changed();
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
    pub(crate) fn frozen_at_launch(&mut self, system: &str, path: &Path, emulator: &str, core: &str, env: &mut Vec<(&'static str, String)>) {
        self.frozen_wait = None;
        if !wanted() {
            return;
        }
        let mark = self.recents.list.iter().find(|r| r.path == path).and_then(|r| r.frozen.clone());
        let game = frozen::Game { system: system.into(), path: path.into(), emulator: emulator.into(), core: core.into(), mark };
        let shown = self.is_frozen_path(path);
        let hosted = emulator == "pocket-core";
        let note = match frozen::at_launch(&game, shown) {
            AtLaunch::Normal => {
                // pocket-core is there in a moment; a big emulator takes as long as it takes, and says that it is on it.
                if shown && !hosted && standalone::auto_states(system, emulator, core, path).is_some() {
                    self.frozen_wait = Some(Instant::now());
                }
                return;
            }
            AtLaunch::Note(note) => note,
            AtLaunch::Fresh(note) => {
                if hosted {
                    env.push((frozen::SKIP_RESUME_ENV, "1".into()));
                } else {
                    standalone::skip_next_resume();
                }
                note
            }
        };
        println!("frozen: {}: {note}", path.display());
        if hosted {
            env.push((frozen::NOTE_ENV, note.into()));
        } else {
            self.toast = Some((note.into(), Instant::now()));
        }
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

pub(crate) const FROZEN: &str = "Frozen";

/// Eight dots round a point, one of them lit, the two behind it fading.
fn spinner(c: &mut Canvas, cx: i32, cy: i32, turn: i32) {
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
