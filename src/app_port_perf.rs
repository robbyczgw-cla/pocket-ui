//! Part of the pocket-ui binary: the App's side of `pocket_ui::port_perf` and `pocket_ui::port_perf_page`. A PortMaster
//! port's Performance & fan page (START on its card), the card's line about it, and the port's run:
//! - every port's choice and note are in memory (`port_perf::Store`, read on a thread at the start, written by the card
//!   writer), so the card, the page and a port's start never wait for a file; in the moments before it is read the
//!   card says "Loading…", START on the card says so too, and a port started then gets Settings' fan and its own
//!   performance from `port_perf`'s thread (`Wanted::Later`);
//! - before the start, the port's fan choice goes to Smooth fan's plan (`app_perf_fan`, `PortPerfUi::fan`);
//! - once started, `port_perf::start` applies its performance when its own process runs, and looks at how it uses
//!   the processor; when the port ends (`game_exited`), `port_perf::ended` puts the performance back.

use std::path::Path;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::perf_fan_page::{fan_label, performance_label};
use pocket_ui::port_perf::{self, Choice, Note, Wanted};
use pocket_ui::port_perf_page::{Action, PortPerfPage, Row};
use pocket_ui::power::{Fan, Performance};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

pub(crate) struct PortPerfUi {
    page: Option<PortPerfPage>,
    /// Every port's own choice and note, in memory (loaded on a thread at the start, `port_perf::Store`).
    store: port_perf::Shared,
    /// The running port's fan for Smooth fan's plan, and whether it is the port's own choice; None for other games.
    pub(crate) fan: Option<(Fan, bool)>,
    /// The performance the port starting now runs with.
    mode: Option<Wanted>,
}

impl PortPerfUi {
    /// Starts reading the ports' choices and notes on a thread (a few small files): nothing on the UI thread.
    pub(crate) fn new(data_dir: &Path) -> PortPerfUi {
        PortPerfUi {
            page: None,
            store: port_perf::spawn_load(data_dir),
            fan: None,
            mode: None,
        }
    }
}

/// The card's Speed line while the ports' settings are still being read.
pub(crate) const LOADING: &str = "Loading…";

/// "Battery, fan Quiet" for the card.
fn speed_line(c: &Choice) -> String {
    format!("{}, fan {}", performance_label(c.performance), fan_label(c.fan))
}

impl App {
    /// Settings → Performance & fan's Games: performance and Games: fan, what a port without its own choice runs with.
    fn port_defaults(&self) -> (Performance, Fan) {
        let p = self.perf_prefs();
        (p.games_performance, p.games_fan)
    }

    /// The port's choice and note, from memory; None while the store is still being read (never read here).
    pub(crate) fn port_state(&self, path: &Path) -> Option<(Choice, Option<Note>)> {
        let defaults = self.port_defaults();
        port_perf::with(&self.port_perf.store, |s| (s.choice(path, defaults), s.note(path).cloned()))
    }

    /// The card's line for a port: its performance and fan, and whether to show the busy-core hint ("Loading…" in
    /// the moments before the ports' settings are read). None for games.
    pub(crate) fn port_card_line(&self, system: &str, path: &Path) -> Option<(String, bool)> {
        if system != port_perf::SYSTEM {
            return None;
        }
        Some(match self.port_state(path) {
            Some((choice, note)) => (speed_line(&choice), port_perf::card_hint(note.as_ref(), choice.performance)),
            None => (LOADING.to_string(), false),
        })
    }

    /// START on a port's card. Before the ports' settings are read: a word instead of a page of guesses.
    pub(crate) fn open_port_perf_for_card(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (name, path) = (open.name.clone(), open.path.clone());
        let Some((choice, note)) = self.port_state(&path) else {
            self.toast = Some(("Still reading the ports' settings…".into(), std::time::Instant::now()));
            return;
        };
        // The page shows the offer now: the card no longer needs to.
        if port_perf::card_hint(note.as_ref(), choice.performance) {
            port_perf::with(&self.port_perf.store, |s| s.mark_seen(&path));
        }
        let prefs = self.perf_prefs();
        self.port_perf.page = Some(PortPerfPage::new(&name, path, choice, self.port_defaults(), prefs.smooth_fan, note));
    }

    /// `--snapshot`: waits (up to two seconds) for the ports' settings, which a running pocket-ui never waits for.
    pub(crate) fn port_perf_wait_loaded(&self) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while port_perf::with(&self.port_perf.store, |_| ()).is_none() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// The page is open over the card.
    pub(crate) fn port_perf_open(&self) -> bool {
        self.port_perf.page.is_some() && self.running.is_none()
    }

    /// Options in the pause menu of a running port: its Performance & fan, over the paused game. What it sets applies from
    /// the port's next start; a port that is running keeps what it started with.
    pub(crate) fn port_perf_in_menu(&mut self) {
        let Some(run) = self.running.as_ref() else { return };
        let (name, path) = (run.name.clone(), run.path.clone());
        let Some((choice, note)) = self.port_state(&path) else {
            pocket_ui::ui_sound::refuse();
            self.toast = Some(("Still reading the ports' settings…".into(), std::time::Instant::now()));
            return;
        };
        let prefs = self.perf_prefs();
        self.port_perf.page = Some(PortPerfPage::new(&name, path, choice, self.port_defaults(), prefs.smooth_fan, note));
        if let Some(m) = self.menu.as_mut() {
            m.page = super::Page::PortPerf;
        }
    }

    /// A press on the page in the pause menu; B goes back to the menu's Options row.
    pub(crate) fn port_perf_menu_input(&mut self, b: Button) {
        if self.port_perf_press(b) {
            self.port_perf_back_to_menu();
        }
    }

    fn port_perf_back_to_menu(&mut self) {
        if let Some(m) = self.menu.as_mut() {
            m.sel = m.items().iter().position(|i| *i == super::MainItem::Options).unwrap_or(0);
            m.page = super::Page::Main;
        }
    }

    pub(crate) fn port_perf_mark(&self) -> Option<(usize, Choice)> {
        self.port_perf.page.as_ref().map(|p| p.mark())
    }

    pub(crate) fn port_perf_input(&mut self, b: Button) {
        if self.port_perf_press(b) {
            self.port_perf_card_refresh();
        }
    }

    /// One press on the page; true when it closed (the page is gone). A choice is saved in memory at once, on the
    /// card by the card writer (`port_perf::Store::set`).
    fn port_perf_press(&mut self, b: Button) -> bool {
        let Some(page) = self.port_perf.page.as_mut() else { return true };
        let change = match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => return false,
            Action::Close => {
                self.port_perf.page = None;
                return true;
            }
            Action::Set { key, value } => Some((key, value)),
            Action::Defaults => None,
        };
        let rom = page.rom.clone();
        // The page only opens once the store is there.
        port_perf::with(&self.port_perf.store, |s| s.set(&rom, change));
        false
    }

    /// Back on the card: its Speed line and hint as they are now.
    fn port_perf_card_refresh(&mut self) {
        let Some((system, path)) = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone())) else {
            return;
        };
        let line = self.port_card_line(&system, &path);
        if let Some(open) = self.card.as_mut() {
            open.card.data.port = line;
        }
    }

    /// A game is about to start: for a port its choice (from memory), for the fan's plan now and for the performance
    /// once it runs; for any other game nothing of a port's is left over.
    pub(crate) fn port_perf_before_launch(&mut self, system: &str, path: &Path) {
        if system != port_perf::SYSTEM {
            self.port_perf.fan = None;
            self.port_perf.mode = None;
            return;
        }
        let Some((c, _)) = self.port_state(path) else {
            // The first moments after the start (On start → Continue): no waiting for the files here. Settings' fan,
            // and the performance from the port's thread once the store is read.
            let (p, f) = self.port_defaults();
            println!("ports: {} starts before the ports' settings are read: fan {}, performance once read", path.display(), f.value());
            self.port_perf.fan = Some((f, false));
            self.port_perf.mode = Some(Wanted::Later(p, f));
            return;
        };
        println!(
            "ports: {} starts with performance {}, fan {}{}",
            path.display(),
            c.performance.value(),
            c.fan.value(),
            if c.own_fan { " (its own)" } else { "" }
        );
        self.port_perf.fan = Some((c.fan, c.own_fan));
        self.port_perf.mode = Some(Wanted::Mode(c.performance));
    }

    /// The game's process group is there: for a port the rest happens on `port_perf`'s thread.
    pub(crate) fn port_perf_started(&mut self, pgid: Option<i32>, path: &Path) {
        if let Some(wanted) = self.port_perf.mode.take() {
            port_perf::start(pgid, path, wanted, &self.port_perf.store);
        }
    }

    /// Any game ended: a port's performance goes back, and its fan leaves the plan.
    pub(crate) fn port_perf_ended(&mut self) {
        self.port_perf.fan = None;
        self.port_perf.mode = None;
        port_perf::ended();
    }

    pub(crate) fn paint_port_perf(&mut self, c: &mut Canvas) {
        let Some(page) = self.port_perf.page.take() else { return };
        self.paint_port_perf_page(c, &page);
        self.port_perf.page = Some(page);
    }

    fn paint_port_perf_page(&mut self, c: &mut Canvas, page: &PortPerfPage) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, "Performance & fan");
        let sub = self.text.fit(Face::Body, 28, &page.title, c.w - 2 * pad - 2 * NU_BTN_PAD - 330);
        self.text.draw(c, Face::Body, 28, pad + NU_BTN_PAD, pad + 104, pal().dust, &sub);
        let (top, right) = (pad + NU_PILL + 46, c.w - pad);
        let rows = page.rows();
        for (i, r) in rows.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = page.label(*r);
            let selected = i == page.sel;
            if selected {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                // The offer stands out a little, as the card's hint does.
                let color = if *r == Row::UseBattery { pal().amber } else { pal().text };
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, color, name);
            }
            let vx = right - NU_BTN_PAD;
            match page.value(*r) {
                Some(value) => {
                    let shown = if selected { format!("‹ {value} ›") } else { value };
                    self.text.draw_right(c, Face::Body, font, vx, base, pal().text, &shown);
                    if selected {
                        let vw = self.text.width(Face::Body, font, &shown);
                        let aw = self.text.width(Face::Body, font, "‹ ") + 24;
                        self.hits.button(Area::new(vx - vw - 24, y, aw, row), Button::Left);
                        self.hits.button(Area::new(vx - aw + 24, y, aw, row), Button::Right);
                    }
                }
                None => self.text.draw_right(c, Face::Body, font, vx, base, pal().text, ">"),
            }
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);

        // What the selected row is for and what its choice does.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + rows.len() as i32 * row + 44;
        let [what, choice] = page.describe(page.row());
        for (text, color) in [(what.as_str(), pal().dust), (choice.as_str(), pal().text)] {
            for line in self.text.wrap(Face::Body, NU_SMALL, text, width) {
                self.text.draw(c, Face::Body, NU_SMALL, left, y, color, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
            y += 6;
        }
        y += 14;
        let note = "These apply to this port from its next start. What every port starts with is \
in Settings › Playing › Performance & fan.";
        for line in self.text.wrap(Face::Body, 24, note, width) {
            self.text.draw(c, Face::Body, 24, left, y, pal().quiet, &line);
            y += pocket_ui::type_scale::line(24, 8);
        }
        let hints = page.hints();
        self.nu_hints(c, &hints);
    }

    /// `--snapshot port-perf[:<row>][:busy][:own]`: the page of a made-up port (Deltarune), the row selected, seen
    /// keeping a core busy (`busy`) and with a choice of its own (`own`: Battery and Quiet); over the shelf.
    pub(crate) fn port_perf_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("port-perf") else { return false };
        let parts: Vec<&str> = rest.split(':').filter(|p| !p.is_empty()).collect();
        let row: usize = parts.iter().find_map(|p| p.parse().ok()).unwrap_or(0);
        let own = parts.contains(&"own");
        let choice = Choice {
            performance: if own { Performance::Battery } else { Performance::Auto },
            fan: if own { Fan::Quiet } else { Fan::Auto },
            own_performance: own,
            own_fan: own,
        };
        let note = parts.contains(&"busy").then_some(Note {
            busy: true,
            top: 0.98,
            rest: 0.24,
            at: 0,
            seen: false,
        });
        let mut page = PortPerfPage::new(
            "Deltarune",
            "/storage/roms/ports/Deltarune.sh".into(),
            choice,
            (Performance::Auto, Fan::Auto),
            true,
            note,
        );
        page.sel = row.min(page.rows().len() - 1);
        self.port_perf.page = Some(page);
        true
    }
}
