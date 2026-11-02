//! Part of the pocket-ui binary: Kids Mode (docs/KIDS-MODE.md; the model is `pocket_ui::kids`). On top of the
//! Shelf look's parts: the same cases (`stand_front`), the same way into the console (`paint_insert`), the same game
//! start and Quit. Only less of them: one big case, Left, Right, A. The input goes through `kids_pad` first, and while
//! the mode is on nothing behind it sees a button.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::cabinet_art as art;
use pocket_ui::gfx::Canvas;
use pocket_ui::kids::{self, Colours, Config, Flow, FlowOut, Gate, GateOut, Ledger, Pick, Stage, Theme};
use pocket_ui::media;
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::palette::pal;
use pocket_ui::shelf::Item;
use pocket_ui::text::Face;

use super::{App, MainItem, CANCEL, CONFIRM, FADE, HOST_CORES};

/// The rows of the grown-ups' page.
const ADULT: [Row; 8] = [Row::More, Row::NewDay, Row::Games, Row::Theme, Row::Limit, Row::Volume, Row::Pin, Row::TurnOff];

/// The grown-ups' rows: "30 more minutes" and "Start today's time again" mean nothing under "No limit", so they are left out.
fn adult_rows(minutes: Option<u32>) -> &'static [Row] {
    if minutes.is_some() {
        &ADULT
    } else {
        &ADULT[2..]
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    More,
    NewDay,
    Games,
    Theme,
    Limit,
    Volume,
    Pin,
    TurnOff,
}

#[derive(Default)]
pub(crate) enum Page {
    #[default]
    Shelf,
    Pin(Gate),
    Adult(usize),
}

#[derive(Default)]
pub(crate) struct Kids {
    pub(crate) cfg: Config,
    ledger: Ledger,
    /// The mode is in charge of the screen: the saved look and Quit's setting wait in `normal`.
    entered: bool,
    pub(crate) normal: Option<(super::Theme, bool)>,
    page: Page,
    flow: Option<Flow>,
    /// The setup's flow is over the first start, which goes on after it (`Setup::kids_done`).
    over_setup: bool,
    hold_since: Option<Instant>,
    home_tap: bool,
    stop_selected: bool,
    /// When the time last counted, and when it was last written.
    counted: Option<Instant>,
    saved_at: Option<Instant>,
    ticking: bool,
    /// A game wouldn't start: the shelf says so (the technical line is in the log).
    trouble: bool,
    /// A game couldn't start inside the mode and nothing else may take over: the picture that asks for a grown-up, until a press.
    stuck: bool,
    launched_at: Option<Instant>,
    background: Option<(Theme, i32, i32, Rc<Vec<u32>>)>,
    /// The flow waits here while its grown-up is on Add games, and comes back when that closes.
    adding: Option<Flow>,
    /// The pause menu is the time warning: this many minutes are left.
    warning: Option<u32>,
    /// The game that stopped kept its moment; a time-up says so only then.
    kept_moment: bool,
}

impl App {
    pub(crate) fn kids_load(&mut self) {
        self.kids.cfg = Config::load(&self.data_dir);
        if !self.kids.cfg.on {
            // What a mode that ended in a crash may have left closed.
            if self.data_dir.join("kids.json").exists() {
                self.kids_doors(false);
            }
            return;
        }
        // PIN forgotten: the file on the card turns the mode off (docs/KIDS-MODE.md).
        if let Some(t) = kids::reset_wanted(&kids::reset_places(), self.kids.cfg.reset_seen) {
            println!("kids mode: {} found, turning the mode off", kids::RESET_FILE);
            for p in kids::reset_places() {
                let _ = std::fs::remove_file(p);
            }
            self.kids.cfg.reset_seen = Some(t);
            self.kids.cfg.on = false;
            self.kids.cfg.pin.clear();
            self.kids.cfg.save(&self.data_dir);
            self.sys.kids_volume_leave(&self.data_dir);
            self.kids_doors(false);
            return;
        }
        self.kids.ledger = Ledger::load(&self.data_dir);
        if self.setup.is_none() {
            self.kids_enter();
        }
    }

    /// ROCKNIX's and the emulators' own shortcuts: closed for the mode, opened again after it (on a thread: a process, files).
    fn kids_doors(&self, on: bool) {
        let data_dir = self.data_dir.clone();
        let doors = move || {
            if let Err(e) = pocket_ui::fn_keys::kids_keys(on, &data_dir) {
                eprintln!("kids mode: ROCKNIX's keys: {e}");
            }
            if !on {
                pocket_ui::standalone::kids_restore(&data_dir);
            }
        };
        if self.drew_first {
            std::thread::spawn(doors);
        } else {
            pocket_ui::startup::after_first_frame("kids-doors", doors);
        }
    }

    /// In charge of the screen and the buttons.
    pub(crate) fn kids_live(&self) -> bool {
        self.kids.entered
    }

    /// The look and Quit's setting to save while the mode has its own in memory.
    pub(crate) fn kids_saved_settings(&self) -> (super::Theme, bool) {
        self.kids.normal.unwrap_or((self.theme, self.remember_quit))
    }

    fn kids_enter(&mut self) {
        self.leave = None;
        self.kids.entered = true;
        self.kids.normal.get_or_insert((self.theme, self.remember_quit));
        self.theme = super::Theme::Shelf;
        self.remember_quit = true;
        self.screen = super::Screen::Home;
        self.kids.page = Page::Shelf;
        self.rebuild();
        self.sel = 0;
        self.prev_sel = 0;
        self.sys.kids_volume_enter(self.kids.cfg.volume, &self.data_dir);
        self.kids_doors(true);
        if !std::mem::replace(&mut self.kids.ticking, true) {
            let proxy = self.proxy.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(5));
                if proxy.send_event(super::UserEvent::KidsTick).is_err() {
                    return;
                }
            });
        }
    }

    /// Turned off with the PIN: everything as it was.
    fn kids_turn_off(&mut self) {
        self.kids.cfg.on = false;
        self.kids.cfg.save(&self.data_dir);
        self.kids.entered = false;
        if let Some((theme, remember)) = self.kids.normal.take() {
            self.theme = theme;
            self.remember_quit = remember;
        }
        self.kids.page = Page::Shelf;
        self.sys.kids_volume_leave(&self.data_dir);
        self.kids_doors(false);
        self.rebuild();
        self.sel = Item::first_row(&self.items);
        self.prev_sel = self.sel;
        self.cabinet_frozen_changed();
    }

    /// The games a child's shelf shows, by the files the grown-up picked; what is gone from the card is left out.
    pub(crate) fn kids_items(&self) -> Vec<Item> {
        self.kids.cfg.games.iter().filter_map(|path| self.systems.iter().enumerate().find_map(|(s, sys)| sys.games.iter().position(|g| g.path == *path).map(|g| Item::Game(s, g)))).collect()
    }

    /// What can be picked, and how many games on the card can't: games pocket-core plays with its states, and those of the
    /// big emulators whose shortcuts the mode closes (`standalone::kids_keeps_moment`). RetroArch, PortMaster and pocket-core's
    /// systems without states have menus of their own, which it can't (docs/KIDS-MODE.md).
    fn kids_library(&self) -> (Vec<Pick>, usize) {
        let (mut out, mut left_out) = (Vec::new(), 0);
        for sys in self.systems.iter().filter(|s| !self.hidden.contains(&s.name)) {
            for g in &sys.games {
                let (emulator, core) = self.route(sys, Some(&g.path), true);
                let keeps = kids::route_keeps_moment(&sys.name, &emulator, &core, &g.path, kids_hosted(&sys.name));
                match keeps {
                    Some(keeps) => out.push(Pick { system: sys.name.clone(), name: g.name.clone(), path: g.path.clone(), own_saves: !keeps }),
                    None => left_out += 1,
                }
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        (out, left_out)
    }

    /// A flow's list, read afresh (the card's games may have changed under Add games).
    fn kids_with_library(&self, mut flow: Flow) -> Flow {
        (flow.library, flow.left_out) = self.kids_library();
        flow
    }

    /// The mode is on and this route is one it can't keep a child inside: not started, whatever a game's list or a
    /// fallback says now (the choice may have changed since a grown-up picked the game).
    pub(crate) fn kids_route_closed(&self, system: &str, emulator: &str, core: &str, path: &Path) -> bool {
        self.kids_live() && kids::route_keeps_moment(system, emulator, core, path, kids_hosted(system)).is_none()
    }

    /// A game can't start inside the mode. Nothing else starts it, `fallback.json` stays as it is (the grown-up's normal
    /// shelf tries pocket-core as before), and the child sees the picture; the reason is in the log.
    pub(crate) fn kids_stuck(&mut self) {
        self.notice = None;
        self.kids.stuck = true;
        self.request_redraw();
    }

    fn kids_running_keeps_moment(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.emulator == "pocket-core" || pocket_ui::standalone::kids_keeps_moment(&r.system, &r.emulator, &r.core, &r.path) == Some(true))
    }

    /// The starter pack's page is open over Kids Mode's picker (`FlowOut::Starter`).
    pub(crate) fn kids_adding(&self) -> bool {
        self.kids.adding.is_some()
    }

    /// Add games closed: the flow goes on, with what is on the card now.
    fn kids_back_from_adding(&mut self) {
        // Add games is the setup's page, the starter pack Settings' (`FlowOut::Starter`).
        if self.setup.is_none() && self.settings_page.is_none() {
            if let Some(flow) = self.kids.adding.take() {
                self.kids.flow = Some(self.kids_with_library(flow));
            }
        }
    }

    /// The first start is over and the mode takes the screen. Games copied during it wait to be picked first, or the
    /// child's shelf would stay empty. No games at all: the grown-up, who is still holding it, gets the page that adds
    /// them, not the child's "ask a grown-up" (new-player walk 2026-11-02).
    fn kids_enter_after_setup(&mut self) {
        self.kids_enter();
        if self.kids.cfg.games.is_empty() {
            let flow = Flow::single(Stage::Games { sel: 0 }, self.kids.cfg.clone(), Vec::new());
            self.kids.flow = Some(self.kids_with_library(flow));
        }
    }

    /// "For a child" in the first start's profiles, or Settings → Kids mode.
    pub(crate) fn kids_open_flow(&mut self, over_setup: bool) {
        let draft = Config { on: false, ..self.kids.cfg.clone() };
        let mut flow = self.kids_with_library(Flow::new(draft, Vec::new()));
        flow.setup_follows = over_setup && flow.library.is_empty();
        self.kids.flow = Some(flow);
        self.kids.over_setup = over_setup;
    }

    pub(crate) fn kids_time_up(&self) -> bool {
        self.kids.ledger.is_up(self.kids.cfg.minutes)
    }

    fn kids_flow_press(&mut self, b: Button) {
        let Some(flow) = self.kids.flow.as_mut() else { return };
        let out = flow.press(b);
        // A game's pictures come through the case cache; nothing here waits for them.
        match out {
            FlowOut::Stay => {}
            FlowOut::Back => self.kids.flow = None,
            // Over Add games the App's own pages take the buttons and the screen (`kids_pad`, `kids_paint`).
            // The starter pack's page (B comes back, `from_home`), over the flow, which waits in `adding`.
            FlowOut::Starter => {
                self.kids.adding = self.kids.flow.take();
                self.settings_page = self.open_starter(true);
            }
            FlowOut::AddGames => {
                self.kids.adding = self.kids.flow.take();
                self.setup = Some(self.new_setup().only(pocket_ui::setup::Only::Games));
            }
            FlowOut::Done => {
                let flow = self.kids.flow.take().expect("open");
                let was_on = self.kids.entered;
                self.kids.cfg = Config { on: true, ..flow.draft };
                self.kids.cfg.save(&self.data_dir);
                if was_on {
                    self.rebuild();
                    self.sys.kids_volume_enter(self.kids.cfg.volume, &self.data_dir);
                    self.kids.background = None;
                } else if self.kids.over_setup {
                    if let Some(s) = self.setup.as_mut() {
                        s.kids_done();
                    }
                } else {
                    self.settings_page = None;
                    self.kids_enter();
                }
            }
        }
    }

    /// The buttons while the mode is on or its flow is open. True: nothing else sees this event.
    pub(crate) fn kids_pad(&mut self, pe: &PadEvent) -> bool {
        if self.kids.adding.is_some() {
            if self.setup.is_some() || self.settings_page.is_some() {
                return false;
            }
            self.kids_back_from_adding();
        }
        // The setup's first start hands over to the mode here, or a grown-up's flow over it.
        if self.kids.cfg.on && !self.kids.entered && self.setup.is_none() {
            self.kids_enter_after_setup();
        }
        if self.kids.flow.is_some() {
            if let PadEvent::Button { button, pressed: true, .. } = *pe {
                self.kids_flow_press(button);
            }
            return true;
        }
        if !self.kids.entered {
            return false;
        }
        // The power menu, and the volume keys (under the ceiling, `Sys::step_volume`).
        if self.sys.is_open() || matches!(pe, PadEvent::Button { button: Button::VolUp | Button::VolDown | Button::Power, .. }) {
            return false;
        }
        // An analog trigger with HOME is a combination too: no HOME tap after it.
        if matches!(pe, PadEvent::Axis { axis: pocket_ui::pad::Axis::L2 | pocket_ui::pad::Axis::R2, value, .. } if *value > 0.5) {
            self.kids.home_tap = false;
        }
        let PadEvent::Button { button, pressed, .. } = *pe else { return true };
        // The hold to leave, on the shelf and the pause menu; a game gets the buttons, so it stays out of play.
        let can_hold = matches!(self.kids.page, Page::Shelf) && (self.running.is_none() || self.menu.is_some());
        let both = self.pad.is_pressed(Button::Select) && self.pad.is_pressed(Button::Start);
        self.kids.hold_since = if both && can_hold { self.kids.hold_since.or(Some(Instant::now())) } else { None };
        if self.running.is_some() {
            self.kids_in_game(button, pressed);
            return true;
        }
        if pressed {
            self.kids_shelf_press(button);
        }
        true
    }

    fn kids_in_game(&mut self, button: Button, pressed: bool) {
        if !matches!(self.kids.page, Page::Shelf) {
            if pressed {
                self.kids_page_press(button);
            }
            return;
        }
        if self.menu.is_some() {
            if !pressed {
                return;
            }
            // The time warning: any of the buttons that go on.
            if self.kids.warning.is_some() {
                if button == CONFIRM || button == CANCEL || button == Button::Mode {
                    self.kids.warning = None;
                    self.resume_game(None);
                }
                return;
            }
            let quitting = self.menu.as_ref().is_some_and(|m| m.quitting);
            match button {
                _ if quitting => {}
                Button::Up | Button::Down | Button::Left | Button::Right => self.kids.stop_selected = !self.kids.stop_selected,
                Button::Mode => self.resume_game(None),
                b if b == CANCEL => self.resume_game(None),
                b if b == CONFIRM && self.kids.stop_selected => self.kids_stop(),
                b if b == CONFIRM => self.resume_game(None),
                _ => {}
            }
            return;
        }
        // A HOME tap is the pause menu. Every other combination is nothing here, and pocket-core has its shortcuts off.
        let (tap, open_menu) = pocket_ui::hotkeys::home_tap_step(self.kids.home_tap, button, pressed);
        self.kids.home_tap = tap;
        if open_menu {
            self.kids.stop_selected = false;
            self.open_menu();
        }
    }

    /// Stop: the pause menu's own Quit, which keeps the moment.
    fn kids_stop(&mut self) {
        self.kids.warning = None;
        self.kids.kept_moment = self.kids_running_keeps_moment();
        let Some(menu) = self.menu.as_mut() else { return };
        menu.without_saving = false;
        menu.sel = menu.items().iter().position(|i| *i == MainItem::Quit).unwrap_or(menu.sel);
        self.menu_input(CONFIRM);
        // The card refuses saves (`app_save_trouble`): nothing is thrown away on a child's press, the game goes on.
        if let Some(m) = self.menu.as_mut().filter(|m| !matches!(m.page, super::Page::Main)) {
            m.page = super::Page::Main;
            self.kids.trouble = true;
            self.resume_game(None);
        }
    }

    fn kids_shelf_press(&mut self, button: Button) {
        if !matches!(self.kids.page, Page::Shelf) {
            return self.kids_page_press(button);
        }
        if std::mem::take(&mut self.kids.stuck) {
            return;
        }
        if self.kids_time_up() {
            return;
        }
        if std::mem::take(&mut self.kids.trouble) {
            self.notice = None;
            return;
        }
        let n = self.items.len();
        match button {
            Button::Left | Button::Up if n > 0 => {
                self.select((self.sel + n - 1) % n);
            }
            Button::Right | Button::Down if n > 0 => {
                self.select((self.sel + 1) % n);
            }
            b if b == CONFIRM => {
                self.kids.launched_at = Some(Instant::now());
                self.activate();
            }
            _ => {}
        }
    }

    fn kids_page_press(&mut self, b: Button) {
        let now = Instant::now();
        let rows = adult_rows(self.kids.cfg.minutes);
        match &mut self.kids.page {
            Page::Pin(gate) => match gate.press(b, &self.kids.cfg.pin, now) {
                GateOut::Stay => {}
                GateOut::Back => self.kids.page = Page::Shelf,
                GateOut::Open => {
                    self.kids.page = Page::Adult(0);
                    // In a paused game: Stop first, which keeps the moment.
                    if self.menu.is_some() {
                        self.kids_stop();
                    }
                }
            },
            Page::Adult(sel) => match b {
                Button::Up => *sel = (*sel + rows.len() - 1) % rows.len(),
                Button::Down => *sel = (*sel + 1) % rows.len(),
                x if x == CANCEL => self.kids.page = Page::Shelf,
                x if x == CONFIRM => {
                    let row = rows[*sel % rows.len()];
                    self.kids_adult_row(row);
                }
                _ => {}
            },
            Page::Shelf => {}
        }
    }

    fn kids_adult_row(&mut self, row: Row) {
        let draft = self.kids.cfg.clone();
        let single = |stage: Stage, lib: Vec<Pick>| Some(Flow::single(stage, draft.clone(), lib));
        match row {
            Row::More => {
                self.kids.ledger.extra_minutes += kids::MORE_MINUTES;
                self.kids.ledger.save(&self.data_dir);
                self.kids.page = Page::Shelf;
            }
            Row::NewDay => {
                self.kids.ledger.start_over(kids::today());
                self.kids.ledger.save(&self.data_dir);
                self.kids.page = Page::Shelf;
            }
            Row::Games => {
                let flow = Flow::single(Stage::Games { sel: 0 }, draft.clone(), Vec::new());
                self.kids.flow = Some(self.kids_with_library(flow));
            }
            Row::Theme => self.kids.flow = single(Stage::Theme, Vec::new()),
            Row::Limit => self.kids.flow = single(Stage::Limit, Vec::new()),
            Row::Volume => self.kids.flow = single(Stage::Volume, Vec::new()),
            Row::Pin => self.kids.flow = single(Stage::Pin { dials: Default::default(), first: None, mismatch: false }, Vec::new()),
            Row::TurnOff => self.kids_turn_off(),
        }
    }

    /// A touch, on its release: left and right of the case move, the case plays; on the pause menu the upper half goes
    /// on, the lower stops. A game's own touch isn't ours, and a grown-up's pages go by the buttons for now.
    pub(crate) fn kids_touch(&mut self, ev: pocket_ui::touch::TouchEvent) {
        if ev.phase != pocket_ui::touch::Phase::Up || self.kids.flow.is_some() || !matches!(self.kids.page, Page::Shelf) {
            return;
        }
        let (w, h) = super::snapshot_dims();
        let (w, h, x, y) = (w as i32, h as i32, ev.x as i32, ev.y as i32);
        if self.running.is_some() {
            if self.menu.is_some() && !self.menu.as_ref().is_some_and(|m| m.quitting) {
                self.kids.stop_selected = y > h / 2;
                self.kids_in_game(CONFIRM, true);
            }
            return;
        }
        let b = if x < w / 4 {
            Button::Left
        } else if x > w - w / 4 {
            Button::Right
        } else {
            CONFIRM
        };
        self.kids_shelf_press(b);
    }

    /// Five seconds of SELECT + START on the shelf or the pause menu: the PIN.
    pub(crate) fn kids_about_to_wait(&mut self) {
        self.kids_back_from_adding();
        if self.kids.cfg.on && !self.kids.entered && self.setup.is_none() {
            self.kids_enter_after_setup();
        }
        if self.kids.hold_since.is_some_and(|t| t.elapsed() >= kids::LEAVE_HOLD) {
            self.kids.hold_since = None;
            self.kids.page = Page::Pin(Gate::default());
            self.request_redraw();
        }
    }

    /// Every few seconds: the day's time, counted only while a game runs unpaused. Time up: the game is saved first.
    pub(crate) fn kids_tick(&mut self) {
        if !self.kids.entered {
            return;
        }
        let now = Instant::now();
        self.kids.ledger.roll(kids::today());
        let awake = self.sleep.state() == pocket_ui::sleep::State::Awake;
        let playing = kids::time_counts(self.running.as_ref().is_some_and(|r| !r.is_app()), self.menu.is_some(), self.launch_at.is_some_and(|t| t.elapsed() < FADE), awake, self.sleep.dark_game());
        let since = self.kids.counted.replace(now).map(|t| now.duration_since(t));
        // A longer gap than this is a sleep, not play.
        if let Some(d) = since.filter(|d| playing && *d < Duration::from_secs(20)) {
            self.kids.ledger.seconds += d.as_secs_f32().round() as u64;
            if self.kids.saved_at.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(30)) {
                self.kids.saved_at = Some(now);
                self.kids.ledger.save(&self.data_dir);
            }
        }
        if playing && self.kids_time_up() {
            println!("kids mode: time is up, saving the game");
            self.kids.ledger.save(&self.data_dir);
            self.open_menu();
            self.kids_stop();
        } else if playing {
            if let Some(left) = self.kids.ledger.warning_due(self.kids.cfg.minutes) {
                println!("kids mode: {left} minutes left");
                self.kids.ledger.save(&self.data_dir);
                self.kids.warning = Some(left);
                self.open_menu();
            }
        }
        self.request_redraw();
    }

    pub(crate) fn kids_animating(&self) -> bool {
        // Covers still being read: the shelf and the setup's game picker (the grown-up's, `kids.flow`) paint them when they arrive.
        (self.kids.entered || self.kids.flow.is_some()) && self.cabinet.pending > 0
            || self.kids.entered && (self.kids.hold_since.is_some() || self.moved_at.elapsed() < Duration::from_millis(320) || matches!(&self.kids.page, Page::Pin(g) if g.wait_until.is_some()))
    }

    /// At a start with the note of a game that was on when the power went: that game, if it is the child's and there is
    /// time left. The other "On start" choice doesn't apply here: a child starts on the shelf.
    pub(crate) fn kids_resume(&mut self, note: Option<pocket_ui::sleep::Resume>) {
        let Some(r) = note else { return };
        let picked = self.kids.cfg.games.contains(&r.path);
        if picked && !self.kids_time_up() {
            println!("kids mode: resuming {}", r.path.display());
            self.launch(r.system, r.name, r.path);
        }
    }

    // ---- painting -------------------------------------------------------------------------------------------

    /// True when the mode painted the frame.
    pub(crate) fn kids_paint(&mut self, c: &mut Canvas) -> bool {
        // The starter pack's page was left: the picker is back before the next press, with the games it brought.
        if self.kids.adding.is_some() && self.setup.is_none() && self.settings_page.is_none() {
            self.kids_back_from_adding();
        }
        if self.kids.flow.is_some() {
            self.kids_paint_flow(c);
            return true;
        }
        if !self.kids.entered || (self.kids.adding.is_some() && (self.setup.is_some() || self.settings_page.is_some())) {
            return false;
        }
        match self.kids.page {
            Page::Pin(_) => self.kids_paint_pin(c),
            Page::Adult(sel) => self.kids_paint_adult(c, sel),
            Page::Shelf if self.running.is_some() && self.menu.is_some() && self.kids.warning.is_some() => self.kids_paint_warning(c),
            Page::Shelf if self.running.is_some() && self.menu.is_some() => self.kids_paint_pause(c),
            Page::Shelf if self.kids.stuck && self.running.is_none() => self.kids_paint_stuck(c),
            Page::Shelf if self.kids_time_up() && self.cabinet.insert.is_none() => self.kids_paint_time_up(c),
            Page::Shelf => self.kids_paint_shelf(c),
        }
        true
    }

    fn kids_background(&mut self, c: &mut Canvas) {
        let theme = self.kids.cfg.theme;
        let pic = match &self.kids.background {
            Some((t, w, h, pic)) if *t == theme && *w == c.w && *h == c.h => pic.clone(),
            _ => {
                let mut buf = vec![0u32; (c.w * c.h) as usize];
                let mut b = Canvas::new(&mut buf, c.w as u32, c.h as u32);
                let (w, h) = (b.w, b.h);
                paint_backdrop(&mut b, 0, 0, w, h, theme);
                let pic = Rc::new(buf);
                self.kids.background = Some((theme, c.w, c.h, pic.clone()));
                pic
            }
        };
        c.buf.copy_from_slice(&pic);
    }

    fn kids_paint_shelf(&mut self, c: &mut Canvas) {
        // The covers are read on the cabinet's thread; nothing else takes them here, and `kids_animating` waits for them.
        self.cabinet_poll();
        self.kids_background(c);
        let colours = self.kids.cfg.theme.colours();
        let u = c.h as f32 / 1080.0;
        self.kids_corner(c, colours);
        self.cabinet.shelf_lit = None;
        let n = self.items.len();
        if n == 0 {
            // Three empty places on the shelf, over the words.
            let (w, h, gap) = ((150.0 * u) as i32, (200.0 * u) as i32, (34.0 * u) as i32);
            let (x0, y) = ((c.w - 3 * w - 2 * gap) / 2, (150.0 * u) as i32);
            for i in 0..3 {
                c.frame(x0 + i * (w + gap), y, w, h, (8.0 * u).max(3.0) as i32, colours.spark);
            }
            let lines = ["No games here yet", "Ask a grown-up to pick some."];
            self.kids_message(c, colours, lines[0], lines[1]);
            self.kids_grownups_line(c, colours);
            return;
        }
        let sel = self.sel.min(n - 1);
        let item = self.items[sel];
        let m = self.medium(item);
        let now = Instant::now();

        // The neighbours, smaller and behind, slide with the move.
        let (room_w, room_h) = (c.w as f32 * 0.58, c.h as f32 * 0.60);
        let (fw1, fh1) = media::front_size(m, 1.0);
        let k = (room_w / fw1 as f32).min(room_h / fh1 as f32);
        let (fw, fh) = media::front_size(m, k);
        let top = (c.h as f32 * 0.09) as i32;
        let fy = top + (room_h as i32 - fh) / 2;
        let fx = (c.w - fw) / 2;
        let moving = self.moved_at.elapsed() < Duration::from_millis(80);
        for (step, side) in [(n - 1, -1), (1, 1)] {
            if n < 2 || (n == 2 && side < 0) {
                continue;
            }
            let other = self.items[(sel + step) % n];
            let om = self.medium(other);
            let (ow1, oh1) = media::front_size(om, 1.0);
            let ok = (room_w * 0.56 / ow1 as f32).min(room_h * 0.60 / oh1 as f32);
            let (ow, oh) = media::front_size(om, ok);
            let (_, front) = self.stand_front(other, om, ok, moving);
            if let Some(front) = front {
                let cx = if side < 0 { (c.w as f32 * 0.06) as i32 } else { c.w - (c.w as f32 * 0.06) as i32 };
                let ox = cx - ow / 2;
                let oy = top + (room_h as i32 - oh) / 2;
                art::soft_shadow(c, ox + 8, oy + 10, ow, oh, 18, 0.35, (0, c.h));
                art::blit(c, &front, ox, oy, 150, (0, c.h));
            }
        }
        let t = pocket_ui::motion::progress(pocket_ui::motion::Kind::Move, self.moved_at, now, Duration::from_millis(220));
        let rise = ((1.0 - t) * c.h as f32 * 0.03) as i32;
        let (_, front) = self.stand_front(item, m, k, false);
        if let Some(front) = front {
            let inserting = self.cabinet.insert.as_ref().is_some_and(|i| i.item == item);
            if !inserting {
                art::soft_shadow(c, fx + 12, fy + 16 + rise, fw, fh, 28, 0.55, (0, c.h));
                art::blit(c, &front, fx, fy + rise, 256, (0, c.h));
            }
            self.cabinet.shelf_lit = Some((front, fx, fy));
        }
        // The arrows at the screen's edges.
        if n > 1 {
            let (ay, size) = (top + room_h as i32 / 2, (42.0 * u) as i32);
            arrow(c, (18.0 * u) as i32 + size, ay, size, -1, colours);
            arrow(c, c.w - (18.0 * u) as i32 - size, ay, size, 1, colours);
        }
        // The name and what A does, big; while the case goes into its console they are out of its way.
        if self.launch_at.is_none() {
            self.kids_shelf_words(c, item, top + room_h as i32, colours);
        }
        // A notice is for a grown-up (the log has it). Right after a press of A it means that game didn't start.
        if self.notice.take().is_some() && self.kids.launched_at.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            self.kids.trouble = true;
        }
        if self.kids.trouble {
            self.kids_band(c, colours, "This game won't start. Ask a grown-up.");
        }
        self.kids_paint_low_battery(c, colours);
        self.kids_paint_hold(c, colours);

        // The way into the console, as on the Shelf look, and the fade into the game.
        if let Some(t) = self.launch_at {
            let p = (t.elapsed().saturating_sub(self.cabinet.launch_delay()).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
            self.paint_insert(c, Instant::now());
            let p = self.cabinet.launch_fade_at(p);
            c.fade((p * 255.0) as u8);
            if !self.cabinet_inserting() {
                self.shelf_relight(c);
            }
            if let Some(pic) = self.launch_picture.as_ref() {
                pocket_ui::launch_picture::paint(c, pic, (p * 255.0) as u8);
            }
        }
    }

    /// The game's name under the case, never cut: smaller when it is long, two lines when that isn't enough; then A.
    fn kids_shelf_words(&mut self, c: &mut Canvas, item: Item, below: i32, colours: Colours) {
        let u = c.h as f32 / 1080.0;
        let px = |v: f32| (v * u).round() as u32;
        let title = self.case_title(item);
        let width = c.w - (80.0 * u) as i32;
        let one_line = [92.0, 80.0, 70.0].into_iter().map(px).find(|p| self.text.width(Face::Title, *p, &title) <= width);
        let centre = below + (88.0 * u) as i32;
        if let Some(p) = one_line {
            let tw = self.text.width(Face::Title, p, &title);
            self.kids_words(c, Face::Title, p, (c.w - tw) / 2, centre + p as i32 * 34 / 100, colours.ink, colours.top, &title);
        } else {
            let p = px(58.0);
            let lines = self.text.wrap_at_most(Face::Title, p, &title, width, 2);
            let step = (p as f32 * 1.08) as i32;
            let mut y = centre - (lines.len() as i32 - 1) * step / 2 + p as i32 * 34 / 100;
            for l in lines {
                let tw = self.text.width(Face::Title, p, &l);
                self.kids_words(c, Face::Title, p, (c.w - tw) / 2, y, colours.ink, colours.top, &l);
                y += step;
            }
        }
        // A plays: the button, as big as the word.
        let word = if self.recents.list.iter().any(|r| self.case_rom(item).is_some_and(|p| p == r.path)) { "Keep playing" } else { "Play" };
        let (word_px, r) = (px(62.0), (44.0 * u) as i32);
        let ww = self.text.width(Face::Title, word_px, word);
        let row_y = c.h - (88.0 * u) as i32;
        let x0 = (c.w - ww - 2 * r - (20.0 * u) as i32) / 2;
        c.disc(x0 + r, row_y, r, colours.accent);
        let aw = self.text.width(Face::Title, word_px, "A");
        self.text.draw(c, Face::Title, word_px, x0 + r - aw / 2, row_y + (word_px as i32) * 34 / 100, colours.on_accent, "A");
        self.kids_words(c, Face::Title, word_px, x0 + 2 * r + (20.0 * u) as i32, row_y + (word_px as i32) * 34 / 100, colours.ink, colours.top, word);
    }

    fn case_title(&self, item: Item) -> String {
        match item {
            Item::Game(s, g) => self.systems[s].games[g].name.clone(),
            _ => String::new(),
        }
    }

    fn case_rom(&self, item: Item) -> Option<&Path> {
        match item {
            Item::Game(s, g) => Some(self.systems[s].games[g].path.as_path()),
            _ => None,
        }
    }

    /// Big words on a backdrop: with a soft edge of the theme's dark, so they read on its bright parts as well.
    fn kids_words(&mut self, c: &mut Canvas, face: Face, px: u32, x: i32, y: i32, ink: u32, shade: u32, s: &str) {
        let d = (px as i32 / 24).max(1);
        let mark = pocket_ui::text::boxes::mark();
        self.text.draw(c, face, px, x + d, y + d, shade, s);
        pocket_ui::text::boxes::forget_since(mark);
        self.text.draw(c, face, px, x, y, ink, s);
    }

    /// The clock and the battery: what a child's top corner has.
    fn kids_corner(&mut self, c: &mut Canvas, colours: Colours) {
        let u = c.h as f32 / 1080.0;
        let corner = self.status_corner();
        let line: Vec<String> = corner.clock.into_iter().chain(corner.battery).collect();
        let px = (44.0 * u).round() as u32;
        self.text.draw_right(c, Face::Title, px, c.w - (36.0 * u) as i32, (62.0 * u) as i32, colours.ink, &line.join("   "));
    }

    /// A big line and a calmer one, in the middle of the screen.
    fn kids_message(&mut self, c: &mut Canvas, colours: Colours, big: &str, small: &str) {
        let u = c.h as f32 / 1080.0;
        let (big_px, small_px) = ((96.0 * u).round() as u32, (54.0 * u).round() as u32);
        let width = c.w - (120.0 * u) as i32;
        let mut y = c.h * 42 / 100;
        for l in self.text.wrap_at_most(Face::Title, big_px, big, width, 2) {
            let w = self.text.width(Face::Title, big_px, &l);
            self.kids_words(c, Face::Title, big_px, (c.w - w) / 2, y, colours.ink, colours.top, &l);
            y += (big_px as f32 * 1.1) as i32;
        }
        y += (20.0 * u) as i32;
        for l in self.text.wrap_prose_at_most(Face::Body, small_px, small, width, 3) {
            let w = self.text.width(Face::Body, small_px, &l);
            self.kids_words(c, Face::Body, small_px, (c.w - w) / 2, y, colours.ink, colours.top, &l);
            y += (small_px as f32 * 1.3) as i32;
        }
    }

    /// A small line at the foot of the screens that stop the child: how a grown-up gets back in. Not on the playing shelf.
    fn kids_grownups_line(&mut self, c: &mut Canvas, colours: Colours) {
        let u = c.h as f32 / 1080.0;
        let px = (34.0 * u).round().max(14.0) as u32;
        let l = self.text.fit(Face::Body, px, "Grown-ups: hold SELECT and START.", c.w - (60.0 * u) as i32);
        let w = self.text.width(Face::Body, px, &l);
        self.text.draw(c, Face::Body, px, (c.w - w) / 2, c.h - (40.0 * u) as i32, colours.ink, &l);
    }

    fn kids_band(&mut self, c: &mut Canvas, colours: Colours, line: &str) {
        let u = c.h as f32 / 1080.0;
        let px = (50.0 * u).round() as u32;
        let h = (110.0 * u) as i32;
        let y = c.h / 2 - h / 2;
        c.rect(0, y, c.w, h, colours.on_accent);
        let l = self.text.fit(Face::Title, px, line, c.w - (40.0 * u) as i32);
        let w = self.text.width(Face::Title, px, &l);
        self.text.draw(c, Face::Title, px, (c.w - w) / 2, y + h / 2 + px as i32 / 3, colours.accent, &l);
    }

    /// At 15 % or less: a nearly empty battery beside the words, over the shelf.
    fn kids_paint_low_battery(&mut self, c: &mut Canvas, colours: Colours) {
        let percent = self.status_corner().battery.and_then(|b| b.trim_end_matches(['%', ' ']).trim().parse::<u32>().ok());
        if !percent.is_some_and(|p| p <= 15) {
            return;
        }
        let u = c.h as f32 / 1080.0;
        let (px, band) = ((50.0 * u).round() as u32, (130.0 * u) as i32);
        let y = (20.0 * u) as i32;
        let line = "Battery low. Please charge.";
        let tw = self.text.width(Face::Title, px, line);
        let (bw, bh) = ((110.0 * u) as i32, (60.0 * u) as i32);
        let gap = (28.0 * u) as i32;
        let x = (c.w - bw - gap - tw) / 2;
        c.round_rect(x - gap, y, bw + tw + 3 * gap, band, (30.0 * u) as i32, colours.on_accent);
        let by = y + (band - bh) / 2;
        let t = (6.0 * u).max(2.0) as i32;
        c.frame(x, by, bw, bh, t, 0xffffff);
        c.rect(x + bw, by + bh / 4, t * 2, bh / 2, 0xffffff);
        c.rect(x + 2 * t, by + 2 * t, bw / 6, bh - 4 * t, 0xd9302f);
        self.text.draw(c, Face::Title, px, x + bw + gap, y + band / 2 + px as i32 / 3, 0xffffff, line);
    }

    /// While SELECT + START are held: a bar that fills, so a grown-up knows it is working.
    fn kids_paint_hold(&mut self, c: &mut Canvas, colours: Colours) {
        let Some(t) = self.kids.hold_since else { return };
        let p = (t.elapsed().as_secs_f32() / kids::LEAVE_HOLD.as_secs_f32()).min(1.0);
        let h = (12.0 * c.h as f32 / 1080.0).max(4.0) as i32;
        c.rect(0, 0, c.w, h, colours.on_accent);
        c.rect(0, 0, (c.w as f32 * p) as i32, h, colours.accent);
    }

    fn kids_paint_time_up(&mut self, c: &mut Canvas) {
        self.kids_background(c);
        let colours = self.kids.cfg.theme.colours();
        self.kids_corner(c, colours);
        let u = c.h as f32 / 1080.0;
        // A big moon rising over the words.
        let r = (120.0 * u) as i32;
        let (cx, cy) = (c.w / 2, (230.0 * u) as i32);
        soft_disc(c, cx, cy, r, colours.accent, 255, (0, 0, c.w, c.h));
        soft_disc(c, cx + r / 2, cy - r / 4, r * 85 / 100, colours.top, 255, (0, 0, c.w, c.h));
        let line = if self.kids.kept_moment { "Your game is saved. It waits for you tomorrow, right where you left it." } else { "Your game waits for you tomorrow." };
        self.kids_message(c, colours, "That's all for today!", line);
        self.kids_grownups_line(c, colours);
        self.kids_paint_hold(c, colours);
    }

    /// A game that can't start here: the handheld with the plaster, as after a crash, and the way back to the shelf.
    fn kids_paint_stuck(&mut self, c: &mut Canvas) {
        self.kids_background(c);
        let colours = self.kids.cfg.theme.colours();
        super::app_rescue::paint_plaster(c, colours, 70.0);
        self.kids_message(c, colours, "Please ask a grown-up", "This game won't start right now. Press a button to pick another one.");
        self.kids_paint_hold(c, colours);
    }

    /// A few minutes before the time is up: a clock with what is left in colour, over the paused game.
    fn kids_paint_warning(&mut self, c: &mut Canvas) {
        if self.menu.as_ref().is_some_and(|m| m.frame.is_some()) {
            self.paint_game_behind(c, 0, 200);
        } else {
            self.kids_background(c);
            c.fade(90);
        }
        let colours = self.kids.cfg.theme.colours();
        let u = c.h as f32 / 1080.0;
        let left = self.kids.warning.unwrap_or(5);
        let (r, cx, cy) = ((150.0 * u) as i32, c.w / 2, (250.0 * u) as i32);
        c.disc(cx, cy, r + (14.0 * u) as i32, colours.on_accent);
        c.disc(cx, cy, r, 0xffffff);
        // The minutes left, as a slice of the clock from twelve o'clock on.
        let part = left.min(60) as f32 / 60.0 * std::f32::consts::TAU;
        for y in (cy - r)..(cy + r) {
            for x in (cx - r)..(cx + r) {
                let (dx, dy) = ((x - cx) as f32, (y - cy) as f32);
                let angle = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
                if dx * dx + dy * dy < (r * r) as f32 && angle < part {
                    c.blend(x, y, colours.accent, 255);
                }
            }
        }
        let hand = (8.0 * u).max(2.0) as i32;
        c.rect(cx - hand / 2, cy - r * 8 / 10, hand, r * 8 / 10, colours.on_accent);
        c.disc(cx, cy, hand * 2, colours.on_accent);
        let line = if left == 1 { "1 minute left".to_string() } else { format!("{left} minutes left") };
        let big = (96.0 * u).round() as u32;
        let width = self.text.width(Face::Title, big, &line);
        self.kids_words(c, Face::Title, big, (c.w - width) / 2, (560.0 * u) as i32, colours.ink, colours.top, &line);
        let (small, sentence) = ((50.0 * u).round() as u32, "Then your game saves itself.");
        let sentence = if self.kids_running_keeps_moment() { sentence } else { "Then it's time to stop." };
        let width = self.text.width(Face::Body, small, sentence);
        self.kids_words(c, Face::Body, small, (c.w - width) / 2, (650.0 * u) as i32, colours.ink, colours.top, sentence);
        // A goes on, as big as on the shelf.
        let (word_px, disc_r, word) = ((62.0 * u).round() as u32, (44.0 * u) as i32, "Continue");
        let ww = self.text.width(Face::Title, word_px, word);
        let row_y = c.h - (130.0 * u) as i32;
        let x0 = (c.w - ww - 2 * disc_r - (20.0 * u) as i32) / 2;
        c.disc(x0 + disc_r, row_y, disc_r, colours.accent);
        let aw = self.text.width(Face::Title, word_px, "A");
        self.text.draw(c, Face::Title, word_px, x0 + disc_r - aw / 2, row_y + word_px as i32 * 34 / 100, colours.on_accent, "A");
        self.kids_words(c, Face::Title, word_px, x0 + 2 * disc_r + (20.0 * u) as i32, row_y + word_px as i32 * 34 / 100, colours.ink, colours.top, word);
        self.kids_paint_hold(c, colours);
    }

    fn kids_paint_pause(&mut self, c: &mut Canvas) {
        // The paused game's picture; a backdrop when there is none.
        if self.menu.as_ref().is_some_and(|m| m.frame.is_some()) {
            self.paint_game_behind(c, 0, 200);
        } else {
            self.kids_background(c);
            c.fade(90);
        }
        let colours = self.kids.cfg.theme.colours();
        let u = c.h as f32 / 1080.0;
        let px = |v: f32| (v * u).round() as u32;
        let Some(menu) = self.menu.as_ref() else { return };
        if menu.quitting {
            self.kids_message(c, colours, "Saving your game…", "");
            return;
        }
        let name = self.running.as_ref().map(|r| r.name.clone()).unwrap_or_default();
        let name = self.text.fit(Face::Title, px(70.0), &name, c.w - (80.0 * u) as i32);
        let nw = self.text.width(Face::Title, px(70.0), &name);
        self.text.draw(c, Face::Title, px(70.0), (c.w - nw) / 2, (150.0 * u) as i32, colours.ink, &name);
        let (bw, bh) = ((c.w as f32 * 0.66) as i32, (210.0 * u) as i32);
        let stop = self.kids.stop_selected;
        for (i, (word, selected)) in [("Continue", !stop), ("Stop", stop)].into_iter().enumerate() {
            let y = (250.0 * u) as i32 + i as i32 * (bh + (40.0 * u) as i32);
            let x = (c.w - bw) / 2;
            let (bg, fg) = if selected { (colours.accent, colours.on_accent) } else { (colours.top, colours.ink) };
            c.round_rect(x, y, bw, bh, (44.0 * u) as i32, bg);
            let wpx = px(104.0);
            let w = self.text.width(Face::Title, wpx, word);
            let (icon, gap) = ((116.0 * u) as i32, (36.0 * u) as i32);
            let tx = x + (bw - w - icon - gap) / 2;
            let cy = y + bh / 2;
            if i == 0 {
                for row in 0..icon {
                    let half = (row.min(icon - 1 - row)) * 2 / 2;
                    c.rect(tx, cy - icon / 2 + row, half.max(1) * 2, 1, 0x1f9d3a);
                }
            } else {
                c.round_rect(tx, cy - icon / 2 + icon / 8, icon * 3 / 4, icon * 3 / 4, (12.0 * u) as i32, 0xd9302f);
            }
            self.text.draw(c, Face::Title, wpx, tx + icon + gap, cy + wpx as i32 * 34 / 100, fg, word);
        }
        let line = if self.kids_running_keeps_moment() { "It saves by itself. Press A next time to carry on." } else { "Save in the game before you stop." };
        let spx = px(46.0);
        let l = self.text.fit(Face::Body, spx, line, c.w - (60.0 * u) as i32);
        let w = self.text.width(Face::Body, spx, &l);
        self.text.draw(c, Face::Body, spx, (c.w - w) / 2, c.h - (72.0 * u) as i32, colours.ink, &l);
        self.kids_paint_hold(c, colours);
    }

    fn kids_paint_pin(&mut self, c: &mut Canvas) {
        self.kids_background(c);
        c.fade(150);
        let colours = self.kids.cfg.theme.colours();
        let u = c.h as f32 / 1080.0;
        let now = Instant::now();
        let Page::Pin(gate) = &self.kids.page else { return };
        let (dials, wrong, wait) = (gate.dials, gate.wrong, gate.waiting(now));
        self.kids_title(c, colours.ink, "For grown-ups", "Enter your PIN.");
        paint_dials(self, c, dials, (430.0 * u) as i32, Some(colours), true);
        let mut lines: Vec<String> = Vec::new();
        if let Some(w) = wait {
            lines.push(format!("Too many tries. Wait {} seconds.", w.as_secs() + 1));
        } else if wrong > 0 {
            lines.push("That's not it. Try again.".to_string());
        }
        // From the first look: the grown-up who forgot the PIN should not have to guess wrong three times to learn this.
        lines.push(format!("Forgot it? Put a file called {} in the top folder of the card's ROCKNIX drive and restart.", kids::RESET_FILE));
        let (spx, mut y) = ((38.0 * u).round() as u32, (810.0 * u) as i32);
        let width = c.w - (100.0 * u) as i32;
        for line in lines {
            for l in self.text.wrap_prose_at_most(Face::Body, spx, &line, width, 2) {
                let w = self.text.width(Face::Body, spx, &l);
                self.text.draw(c, Face::Body, spx, (c.w - w) / 2, y, colours.ink, &l);
                y += (spx as f32 * 1.3) as i32;
            }
        }
        self.paint_hint_row(c, &[("D-PAD", "Numbers"), ("A", "Next"), ("B", "Back")]);
    }

    /// A grown-up's page: its title and what it is about. Returns where the page's words end, larger with larger text.
    fn kids_title(&mut self, c: &mut Canvas, ink: u32, title: &str, line: &str) -> i32 {
        let u = c.h as f32 / 1080.0;
        let margin = (72.0 * u) as i32;
        self.text.draw(c, Face::Title, (72.0 * u).round() as u32, margin, (130.0 * u) as i32, ink, title);
        let mut y = (196.0 * u) as i32;
        let spx = (34.0 * u).round() as u32;
        for l in self.text.wrap_prose_at_most(Face::Body, spx, line, c.w - 2 * margin, 3) {
            self.text.draw(c, Face::Body, spx, margin, y, ink, &l);
            y += (spx as f32 * 1.35) as i32;
        }
        y
    }

    fn kids_paint_adult(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let u = c.h as f32 / 1080.0;
        // Picked games that were deleted or moved off the card meanwhile: the child's shelf is shorter, and why shows here.
        let gone = self.kids.cfg.games.len().saturating_sub(self.kids_items().len());
        let line = if self.kids.ledger.clock_behind(kids::today()) {
            "The clock is behind the last date it knew, so today's time goes on counting. Start it again here if that's wrong.".to_string()
        } else if gone == 1 {
            "1 of the picked games isn't on the card any more.".to_string()
        } else if gone > 1 {
            format!("{} of the picked games aren't on the card any more.", gone)
        } else {
            "For grown-ups. The child's saves and settings stay as they are.".to_string()
        };
        let words_end = self.kids_title(c, pal().chalk, "Kids mode", &line);
        let cfg = self.kids.cfg.clone();
        let rows = adult_rows(cfg.minutes);
        let sel = sel % rows.len();
        let used = self.kids.ledger.seconds / 60;
        let values = |row: Row| match row {
            Row::More => cfg.minutes.map_or("No limit".to_string(), |m| format!("{used} of {} min today", m + self.kids.ledger.extra_minutes)),
            Row::Games if cfg.games.len() == 1 => "1 game".to_string(),
            Row::Games => format!("{} games", cfg.games.len()),
            Row::Theme => cfg.theme.name().to_string(),
            Row::Limit => limit_label(cfg.minutes),
            Row::Volume => volume_label(cfg.volume),
            Row::NewDay | Row::Pin | Row::TurnOff => String::new(),
        };
        // Under the words however many lines they took, and over the hint bar.
        let rows_y = ((270.0 * u) as i32).max(words_end);
        let row_h = ((78.0 * u) as i32).min((c.h - (90.0 * u) as i32 - rows_y) / rows.len() as i32);
        let (x, w) = ((72.0 * u) as i32, c.w - (144.0 * u) as i32);
        let label_px = (42.0 * u).round() as u32;
        for (i, row) in rows.iter().enumerate() {
            let y = rows_y + i as i32 * row_h;
            let selected = i == sel;
            let (fg, dim) = if selected {
                c.rect(x, y, w, row_h - (8.0 * u) as i32, pal().label);
                (pal().ink, pal().label_dust)
            } else {
                (pal().chalk, pal().dust)
            };
            let label = match row {
                Row::More => "30 more minutes today",
                Row::NewDay => "Start today's time again",
                Row::Games => "Games",
                Row::Theme => "Colours",
                Row::Limit => "Time per day",
                Row::Volume => "Loudest volume",
                Row::Pin => "New PIN",
                Row::TurnOff => "Turn off Kids mode",
            };
            let base = y + row_h / 2 + label_px as i32 / 4;
            self.text.draw(c, Face::Title, label_px, x + (24.0 * u) as i32, base, fg, label);
            let value = values(*row);
            if !value.is_empty() {
                self.text.draw_right(c, Face::Body, (32.0 * u).round() as u32, x + w - (24.0 * u) as i32, base, dim, &value);
            }
        }
        self.paint_hint_row(c, &[("A", "Open"), ("B", "Back to the shelf")]);
    }

    fn kids_paint_flow(&mut self, c: &mut Canvas) {
        c.clear(pal().void);
        let u = c.h as f32 / 1080.0;
        let Some(flow) = self.kids.flow.as_ref() else { return };
        let stage = flow.stage.clone();
        let draft = flow.draft.clone();
        match stage {
            Stage::Welcome => {
                self.kids_title(c, pal().chalk, "Set it up for a child", "It takes two minutes. Your child gets a big, simple shelf with only the games you pick. Settings stay out of reach, behind your PIN.");
                let mut y = (420.0 * u) as i32;
                let lpx = (40.0 * u).round() as u32;
                let margin = (72.0 * u) as i32;
                for line in ["A 4-digit PIN", "5 to 20 games", "Blue or Pink", "A time per day and a loudest volume, if you like"] {
                    c.disc(margin + (10.0 * u) as i32, y - lpx as i32 / 3, (9.0 * u) as i32, pal().label);
                    for l in self.text.wrap_prose_at_most(Face::Body, lpx, line, c.w - 2 * margin - (40.0 * u) as i32, 2) {
                        self.text.draw(c, Face::Body, lpx, margin + (40.0 * u) as i32, y, pal().chalk, &l);
                        y += (lpx as f32 * 1.3) as i32;
                    }
                    y += (72.0 * u) as i32 - (lpx as f32 * 1.3) as i32;
                }
                self.paint_hint_row(c, &[("A", "Start"), ("B", "Back")]);
            }
            Stage::Pin { dials, first, mismatch } => {
                let (t, l) = if first.is_some() { ("Once more", "Enter the same PIN again, so a slip doesn't lock you out.") } else { ("Pick a PIN", "Four digits for you, not for your child. You'll need it to leave Kids mode.") };
                self.kids_title(c, pal().chalk, t, l);
                paint_dials(self, c, dials, (430.0 * u) as i32, None, false);
                if mismatch {
                    let spx = (38.0 * u).round() as u32;
                    let line = "Those two were different. Pick one again.";
                    let w = self.text.width(Face::Body, spx, line);
                    self.text.draw(c, Face::Body, spx, (c.w - w) / 2, (810.0 * u) as i32, pal().chalk, line);
                }
                self.paint_hint_row(c, &[("D-PAD", "Numbers"), ("A", "Next"), ("B", "Back")]);
            }
            Stage::Games { sel } => self.kids_paint_games(c, sel),
            Stage::Theme => {
                self.kids_title(c, pal().chalk, "Blue or Pink?", "Pick the one your child likes. You can change it later.");
                let (gap, top) = ((36.0 * u) as i32, (320.0 * u) as i32);
                let (w, h) = ((c.w - (144.0 * u) as i32 - gap) / 2, (520.0 * u) as i32);
                for (i, theme) in [Theme::Blue, Theme::Pink].into_iter().enumerate() {
                    let x = (72.0 * u) as i32 + i as i32 * (w + gap);
                    paint_backdrop(c, x, top, w, h, theme);
                    let px = (96.0 * u).round() as u32;
                    let tw = self.text.width(Face::Title, px, theme.name());
                    self.text.draw(c, Face::Title, px, x + (w - tw) / 2, top + h / 2 + px as i32 / 3, theme.colours().ink, theme.name());
                    if theme == draft.theme {
                        let t = (10.0 * u).max(3.0) as i32;
                        c.frame(x - t, top - t, w + 2 * t, h + 2 * t, t, pal().chalk);
                    }
                }
                self.paint_hint_row(c, &[("D-PAD", "Choose"), ("A", "Next"), ("B", "Back")]);
            }
            Stage::Limit => {
                self.kids_title(c, pal().chalk, "How long per day?", "When the time is up, the game saves itself and a friendly screen says goodbye until tomorrow.");
                self.kids_value(c, &limit_label(draft.minutes));
                self.paint_hint_row(c, &[("D-PAD", "Choose"), ("A", "Next"), ("B", "Back")]);
            }
            Stage::Volume => {
                self.kids_title(c, pal().chalk, "How loud at most?", "The volume keys stop here. Good for small ears and headphones.");
                self.kids_value(c, &volume_label(draft.volume));
                self.paint_hint_row(c, &[("D-PAD", "Choose"), ("A", "Next"), ("B", "Back")]);
            }
            Stage::Ready => {
                // Nothing on the card yet: there's nothing to hand over until the next steps have brought games.
                let no_games = self.kids.flow.as_ref().is_some_and(|f| f.library.is_empty());
                self.kids_title(c, pal().chalk, "Ready", if no_games { "Next you add games, then pick them. To get back in later:" } else { "Hand it over any time. To get back in:" });
                let mut y = (330.0 * u) as i32;
                let lpx = (44.0 * u).round() as u32;
                let margin = (72.0 * u) as i32;
                let width = c.w - 2 * margin;
                let forgot = if pocket_ui::device::current().has_wifi() {
                    format!("Forgot the PIN? Make a file called {} on the card (a phone with a card reader or the Wi-Fi share will do) and restart. Kids mode goes off, and saves and games stay.", kids::RESET_FILE)
                } else {
                    format!("Forgot the PIN? Make a file called {} on the card (a computer or a phone with a card reader will do) and restart. Kids mode goes off, and saves and games stay.", kids::RESET_FILE)
                };
                for (line, colour, rows) in [("Hold SELECT and START for 5 seconds, then enter your PIN.", pal().chalk, 3), (forgot.as_str(), pal().dust, 6)] {
                    for l in self.text.wrap_prose_at_most(Face::Body, lpx, line, width, rows) {
                        self.text.draw(c, Face::Body, lpx, margin, y, colour, &l);
                        y += (lpx as f32 * 1.35) as i32;
                    }
                    y += (40.0 * u) as i32;
                }
                self.paint_hint_row(c, &[("A", "Done"), ("B", "Back")]);
            }
        }
    }

    fn kids_value(&mut self, c: &mut Canvas, value: &str) {
        let u = c.h as f32 / 1080.0;
        let px = (120.0 * u).round() as u32;
        let w = self.text.width(Face::Title, px, value);
        let (y, size) = ((560.0 * u) as i32, (34.0 * u) as i32);
        self.text.draw(c, Face::Title, px, (c.w - w) / 2, y, pal().chalk, value);
        let colours = Colours { accent: pal().label, on_accent: pal().ink, ink: pal().chalk, ..Theme::Blue.colours() };
        arrow(c, (c.w - w) / 2 - size * 3, y - px as i32 / 3, size, -1, colours);
        arrow(c, (c.w + w) / 2 + size * 3, y - px as i32 / 3, size, 1, colours);
    }

    fn kids_paint_games(&mut self, c: &mut Canvas, sel: usize) {
        let u = c.h as f32 / 1080.0;
        let Some(flow) = self.kids.flow.as_ref() else { return };
        let (library, picked, needed, full, left_out, follows) = (flow.library.clone(), flow.draft.games.clone(), flow.needed(), flow.full, flow.left_out, flow.setup_follows);
        // Why games on the card aren't in the list, for the grown-up, in the grey of what isn't there to choose.
        let left_out_line = match left_out {
            0 => None,
            1 => Some("1 game isn't offered: RetroArch and PortMaster have menus of their own a child could reach.".to_string()),
            n => Some(format!("{n} games aren't offered: RetroArch and PortMaster have menus of their own a child could reach.")),
        };
        if library.is_empty() {
            let starter = flow.offers_starter();
            let (line, hints): (&str, &[(&str, &str)]) = if follows {
                ("The next steps show how to add them. You pick the games for your child right after that.", &[("A", "Go on"), ("B", "Back")])
            } else {
                ("Add some first: you pick the games for your child right after.", if starter { &[("A", "Add games"), ("X", "Starter pack"), ("START", "Later"), ("B", "Back")] } else { &[("A", "Add games"), ("START", "Later"), ("B", "Back")] })
            };
            self.kids_title(c, pal().chalk, "No games to pick yet", line);
            if let Some(l) = left_out_line {
                self.kids_grey_line(c, (420.0 * u) as i32, &l);
            }
            self.paint_hint_row(c, hints);
            return;
        }
        self.kids_title(c, pal().chalk, "Pick the games", &format!("{} of {} picked. At least {needed}.", picked.len(), kids::MAX_GAMES));
        let (x, top, row_h) = ((72.0 * u) as i32, (246.0 * u) as i32, (76.0 * u) as i32);
        let w = (c.w as f32 * 0.62) as i32;
        let below = if left_out_line.is_some() { 210.0 } else { 130.0 };
        let rows = ((c.h - top - (below * u) as i32) / row_h).max(1) as usize;
        let first = crate::window_start(sel, rows, library.len());
        let lpx = (36.0 * u).round() as u32;
        for (i, game) in library.iter().enumerate().skip(first).take(rows) {
            let y = top + (i - first) as i32 * row_h;
            let selected = i == sel;
            let (fg, box_c) = if selected {
                c.rect(x, y, w, row_h - (6.0 * u) as i32, pal().label);
                (pal().ink, pal().ink)
            } else {
                (pal().chalk, pal().dust)
            };
            let size = (30.0 * u) as i32;
            let (bx, by) = (x + (18.0 * u) as i32, y + (row_h - size) / 2 - (3.0 * u) as i32);
            c.frame(bx, by, size, size, (3.0 * u).max(1.0) as i32, box_c);
            if picked.contains(&game.path) {
                c.rect(bx + size / 4, by + size / 4, size / 2, size / 2, box_c);
            }
            let name = self.text.fit(Face::Body, lpx, &game.name, w - size - (60.0 * u) as i32);
            self.text.draw(c, Face::Body, lpx, bx + size + (20.0 * u) as i32, y + row_h / 2 + lpx as i32 / 4, fg, &name);
        }
        if let Some(l) = left_out_line {
            self.kids_grey_line(c, c.h - (178.0 * u) as i32, &l);
        }
        // Beside the list: the case of the game under the cursor, and what its Stop can't do.
        if library[sel].own_saves {
            let spx = (30.0 * u).round() as u32;
            let mut y = c.h - (240.0 * u) as i32;
            for l in self.text.wrap_prose_at_most(Face::Body, spx, "Saves only in the game: Stop doesn't keep the moment.", c.w - x - w - (60.0 * u) as i32, 3) {
                self.text.draw(c, Face::Body, spx, x + w + (30.0 * u) as i32, y, pal().dust, &l);
                y += (spx as f32 * 1.3) as i32;
            }
        }
        if let Some(item) = self.kids_library_item(&library[sel]) {
            // The cover is read on the cabinet's thread; nothing else takes it on this page (`kids_animating` repaints).
            self.cabinet_poll();
            let m = self.medium(item);
            let room_w = c.w - x - w - (60.0 * u) as i32;
            let (fw1, fh1) = media::front_size(m, 1.0);
            let k = (room_w as f32 / fw1 as f32).min(c.h as f32 * 0.4 / fh1 as f32);
            let (_, front) = self.stand_front(item, m, k, false);
            if let Some(front) = front {
                art::blit(c, &front, x + w + (30.0 * u) as i32, top, 256, (0, c.h));
            }
        }
        if full {
            self.kids_band(c, Theme::Blue.colours(), "Twenty is the most. Take one off first.");
        }
        let start = if picked.len() >= needed { "Next" } else { "Pick more first" };
        self.paint_hint_row(c, &[("A", "Pick"), ("START", start), ("B", "Back")]);
    }

    /// One line for the grown-up, in the grey of what isn't there to choose.
    fn kids_grey_line(&mut self, c: &mut Canvas, y: i32, line: &str) {
        let u = c.h as f32 / 1080.0;
        let (x, spx) = ((72.0 * u) as i32, (30.0 * u).round() as u32);
        let mut y = y;
        for l in self.text.wrap_prose_at_most(Face::Body, spx, line, c.w - 2 * x, 2) {
            self.text.draw(c, Face::Body, spx, x, y, pal().dust, &l);
            y += (spx as f32 * 1.3) as i32;
        }
    }

    fn kids_library_item(&self, pick: &Pick) -> Option<Item> {
        let s = self.systems.iter().position(|sys| sys.name == pick.system)?;
        let g = self.systems[s].games.iter().position(|g| g.path == pick.path)?;
        Some(Item::Game(s, g))
    }

    /// "kids-flow:games-thread", "kids-shelf-thread": paints the picker as the paint loop does, until `kids_animating` lets go, and says on
    /// stdout whether the cover of the game under the cursor got there (tests/kids_mode.rs).
    pub(crate) fn kids_cover_settle(&mut self, c: &mut Canvas) {
        let mut frames = 0;
        loop {
            self.paint(c);
            frames += 1;
            if !self.kids_animating() || frames > 300 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let ready = match self.kids.flow.as_ref().and_then(|f| match f.stage {
            Stage::Games { sel } => f.library.get(sel).cloned(),
            _ => None,
        }) {
            Some(pick) => self.kids_library_item(&pick).is_some_and(|item| self.kids_cover_ready(item)),
            None => self.items.get(self.sel).cloned().is_some_and(|item| self.kids_cover_ready(item)),
        };
        println!("kids-cover ready={ready} frames={frames} animating={}", self.kids_animating());
    }

    // ---- snapshots ----------------------------------------------------------------------------------------------

    /// "kids-…": the mode's screens for the pictures in the brief, from POCKETUI_DATA's kids.json.
    pub(crate) fn kids_snapshot(&mut self, state: &str, background: Option<pocket_ui::image::Image>, w: u32, h: u32) -> bool {
        let Some(rest) = state.strip_prefix("kids-") else { return false };
        let mut parts = rest.split(':');
        let what = parts.next().unwrap_or_default();
        let arg = parts.next().unwrap_or_default();
        self.kids.cfg = Config::load(&self.data_dir);
        self.kids.ledger = Ledger::load(&self.data_dir);
        // The covers are read before the picture, not on a thread, except on "games-thread": the way a device reads them.
        self.cabinet.inline = !(what == "shelf-thread" || what == "flow" && arg == "games-thread");
        match what {
            "stuck" => {
                self.kids_enter();
                self.kids.stuck = true;
            }
            "shelf" | "shelf-thread" | "insert" | "hold" | "trouble" => {
                self.kids_enter();
                self.sel = arg.parse().unwrap_or(0).min(self.items.len().saturating_sub(1));
                if what == "hold" {
                    self.kids.hold_since = Some(Instant::now() - Duration::from_millis(2600));
                }
                if what == "trouble" {
                    self.kids.trouble = true;
                }
                if what == "insert" {
                    let mut buf = vec![0u32; (w * h) as usize];
                    let mut c = Canvas::new(&mut buf, w, h);
                    self.moved_at = Instant::now() - Duration::from_secs(5);
                    self.kids_paint(&mut c);
                    self.launch_animation_snapshot(Duration::from_millis(parts.next().and_then(|m| m.parse().ok()).unwrap_or(900)));
                }
            }
            "lowbat" => self.kids_enter(),
            "power" => {
                self.kids_enter();
                self.sys.open_power_in(Instant::now(), false);
            }
            "warning" => {
                self.kids_enter();
                let item = self.items.first().copied();
                let Some(Item::Game(s, g)) = item else { return false };
                let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
                self.running = Some(super::Running { system, name, path, emulator: "pocket-core".into(), core: String::new(), started: Instant::now(), pgid: None, options: Default::default() });
                self.open_menu();
                if let Some(m) = self.menu.as_mut() {
                    m.frame = background;
                }
                self.kids.warning = Some(arg.parse().unwrap_or(5));
            }
            "empty" => {
                self.kids.cfg.games.clear();
                self.kids_enter();
            }
            "timeup" => {
                self.kids.cfg.minutes = Some(30);
                self.kids.ledger.seconds = 3600;
                self.kids_enter();
            }
            "pause" => {
                self.kids_enter();
                let item = self.items.first().copied();
                let Some(Item::Game(s, g)) = item else { return false };
                let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
                self.running = Some(super::Running { system, name, path, emulator: "pocket-core".into(), core: String::new(), started: Instant::now(), pgid: None, options: Default::default() });
                self.open_menu();
                if let Some(m) = self.menu.as_mut() {
                    m.frame = background;
                    m.quitting = arg == "saving";
                }
                self.kids.stop_selected = arg == "stop";
            }
            "pin" => {
                self.kids_enter();
                let mut gate = Gate::default();
                gate.dials.digits = [4, 0, 0, 0];
                gate.dials.at = 1;
                match arg {
                    "wrong" => gate.wrong = 1,
                    "help" => gate.wrong = kids::WRONG_BEFORE_HELP,
                    "wait" => {
                        gate.wrong = kids::WRONG_BEFORE_WAIT;
                        gate.wait_until = Some(Instant::now() + Duration::from_secs(47));
                    }
                    _ => {}
                }
                self.kids.page = Page::Pin(gate);
            }
            "adult" => {
                self.kids_enter();
                self.kids.ledger.seconds = 1320;
                // The clock behind the last date known: `kids-adult:0:behind`; picked games gone from the card: `:gone`.
                match parts.next() {
                    Some("behind") => self.kids.ledger.day = kids::today() + 40,
                    Some("gone") => self.kids.cfg.games.push(PathBuf::from("/nowhere/Gone Game.gba")),
                    _ => {}
                }
                self.kids.page = Page::Adult(arg.parse().unwrap_or(0));
            }
            "flow" => {
                let (library, left_out) = self.kids_library();
                let cfg = self.kids.cfg.clone();
                let dials = kids::Dials { digits: [4, 7, 0, 0], at: 2 };
                let empty = matches!(arg, "games-empty" | "games-first" | "games-none-left");
                let mut flow = Flow::new(Config { on: false, ..cfg }, if empty { Vec::new() } else { library });
                flow.left_out = if matches!(arg, "games-left" | "games-none-left") { 7 } else { left_out };
                flow.setup_follows = arg == "games-first";
                flow.stage = match arg {
                    "pin" => Stage::Pin { dials, first: None, mismatch: false },
                    "pin-again" => Stage::Pin { dials, first: Some("4700".into()), mismatch: false },
                    "pin-differ" => Stage::Pin { dials: Default::default(), first: Some("4700".into()), mismatch: true },
                    "games" | "games-thread" | "games-empty" | "games-first" | "games-left" | "games-none-left" => Stage::Games { sel: 2 },
                    "theme" => Stage::Theme,
                    "limit" => Stage::Limit,
                    "volume" => Stage::Volume,
                    "ready" => Stage::Ready,
                    _ => Stage::Welcome,
                };
                self.kids.flow = Some(flow);
            }
            _ => return false,
        }
        true
    }
}

/// pocket-core has this system with its states.
fn kids_hosted(system: &str) -> bool {
    HOST_CORES.iter().any(|(s, _)| *s == system) || pocket_ui::hostsys::find(system).is_some_and(|h| h.no_states.is_none())
}

fn limit_label(minutes: Option<u32>) -> String {
    match minutes {
        None => "No limit".to_string(),
        Some(m) if m % 60 == 0 => format!("{} hours", m / 60).replace("1 hours", "1 hour"),
        Some(m) if m > 60 => format!("{} min", m),
        Some(m) => format!("{m} min"),
    }
}

fn volume_label(v: Option<u8>) -> String {
    v.map_or("No limit".to_string(), |v| format!("{v} %"))
}

/// The four dials. `hide`: only the one being turned shows its number, for a PIN a child may be watching.
fn paint_dials(app: &mut App, c: &mut Canvas, dials: kids::Dials, top: i32, colours: Option<Colours>, hide: bool) {
    let u = c.h as f32 / 1080.0;
    let (w, h, gap) = ((170.0 * u) as i32, (240.0 * u) as i32, (34.0 * u) as i32);
    let x0 = (c.w - 4 * w - 3 * gap) / 2;
    let px = (150.0 * u).round() as u32;
    let (card, selected, ink, on_selected) = match colours {
        Some(k) => (k.top, k.accent, k.ink, k.on_accent),
        None => (pal().spine, pal().label, pal().chalk, pal().ink),
    };
    for (i, digit) in dials.digits.iter().enumerate() {
        let x = x0 + i as i32 * (w + gap);
        let at = i == dials.at;
        c.round_rect(x, top, w, h, (26.0 * u) as i32, if at { selected } else { card });
        let fg = if at { on_selected } else { ink };
        let s = if hide && !at && i < dials.at { "•".to_string() } else { digit.to_string() };
        let tw = app.text.width(Face::Title, px, &s);
        app.text.draw(c, Face::Title, px, x + (w - tw) / 2, top + h / 2 + px as i32 * 36 / 100, fg, &s);
        if at {
            let size = (22.0 * u) as i32;
            let a = Colours { accent: selected, on_accent: on_selected, ink, ..Theme::Blue.colours() };
            up_arrow(c, x + w / 2, top - (28.0 * u) as i32, size, -1, a);
            up_arrow(c, x + w / 2, top + h + (28.0 * u) as i32, size, 1, a);
        }
    }
}

fn arrow(c: &mut Canvas, cx: i32, cy: i32, size: i32, dir: i32, k: Colours) {
    c.disc(cx, cy, size + size / 3, k.accent);
    let tip = size * 3 / 4;
    for dx in 0..tip {
        let half = tip - dx;
        c.rect(cx + dir * (dx - tip / 3), cy - half, 2, half * 2, k.on_accent);
    }
}

fn up_arrow(c: &mut Canvas, cx: i32, cy: i32, size: i32, dir: i32, k: Colours) {
    for dy in 0..size {
        let half = size - dy;
        c.rect(cx - half, cy + dir * dy, half * 2, 2, k.accent);
    }
}

/// A disc with a smooth edge, at `alpha`, within `clip` (x, y, w, h).
fn soft_disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, colour: u32, alpha: u8, clip: (i32, i32, i32, i32)) {
    let (x0, y0) = ((cx - r - 1).max(clip.0), (cy - r - 1).max(clip.1));
    let (x1, y1) = ((cx + r + 1).min(clip.0 + clip.2), (cy + r + 1).min(clip.1 + clip.3));
    for y in y0..y1 {
        for x in x0..x1 {
            let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
            let edge = (r as f32 - d + 0.5).clamp(0.0, 1.0);
            if edge > 0.0 {
                c.blend(x, y, colour, (edge * alpha as f32) as u8);
            }
        }
    }
}

/// A theme's backdrop on a part of the screen: a gradient and big shapes, all made here (no objects): Blue a night sky with
/// stars and a big planet's curve, Pink a sunset with bubbles and sparkles.
pub(crate) fn paint_backdrop(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, theme: Theme) {
    let k = theme.colours();
    let clip = (x, y, w, h);
    for row in 0..h {
        let t = row as f32 / h.max(1) as f32;
        c.rect(x, y + row, w, 1, pocket_ui::gfx::mix(k.top, k.bottom, (t * t * (3.0 - 2.0 * t) * 255.0) as u32));
    }
    let s = (h as f32 / 1080.0).max(0.1);
    let at = |fx: f32, fy: f32| (x + (w as f32 * fx) as i32, y + (h as f32 * fy) as i32);
    let mut seed = if theme == Theme::Blue { 0x2f6e_2b11u32 } else { 0x9d2c_5680u32 };
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    match theme {
        Theme::Blue => {
            let (cx, cy) = at(0.5, 1.62);
            soft_disc(c, cx, cy, (h as f32 * 0.90) as i32, k.shape, 34, clip);
            soft_disc(c, cx, cy, (h as f32 * 0.84) as i32, k.shape, 40, clip);
            soft_disc(c, cx, cy, (h as f32 * 0.78) as i32, k.top, 90, clip);
            for _ in 0..70 {
                let (r1, r2, r3) = (next(), next(), next());
                let (sx, sy) = at((r1 % 1000) as f32 / 1000.0, (r2 % 340) as f32 / 1000.0);
                let r = ((1 + r3 % 4) as f32 * s).max(1.0) as i32;
                soft_disc(c, sx, sy, r, k.spark, 150 + (r3 % 100) as u8, clip);
            }
            // A shooting star over the top corner.
            let (sx, sy) = at(0.12, 0.09);
            for i in 0..(90.0 * s) as i32 {
                let a = (200 - (i as f32 / s) as i32 * 2).clamp(0, 200) as u8;
                soft_disc(c, sx + i, sy + i / 3, ((4.0 - i as f32 / s / 30.0) * s).max(1.0) as i32, k.spark, a / 3, clip);
            }
        }
        Theme::Pink => {
            let (cx, cy) = at(0.5, 1.5);
            soft_disc(c, cx, cy, (h as f32 * 0.8) as i32, k.spark, 40, clip);
            for _ in 0..26 {
                let (r1, r2, r3) = (next(), next(), next());
                let (sx, sy) = at((r1 % 1000) as f32 / 1000.0, (r2 % 1000) as f32 / 1000.0);
                soft_disc(c, sx, sy, ((30 + r3 % 110) as f32 * s) as i32, k.shape, 34, clip);
            }
            for _ in 0..30 {
                let (r1, r2, r3) = (next(), next(), next());
                let (sx, sy) = at((r1 % 1000) as f32 / 1000.0, (r2 % 300) as f32 / 1000.0);
                let r = ((3 + r3 % 8) as f32 * s).max(2.0) as i32;
                // A four-pointed sparkle.
                for i in 0..r {
                    let t = (r - i).max(1) / 2 + 1;
                    for (dx, dy) in [(i, 0), (-i, 0), (0, i), (0, -i)] {
                        if sx + dx >= x && sx + dx < x + w && sy + dy >= y && sy + dy < y + h {
                            for a in 0..t {
                                c.blend(sx + dx + if dy != 0 { a - t / 2 } else { 0 }, sy + dy + if dx != 0 { a - t / 2 } else { 0 }, k.spark, 220);
                            }
                        }
                    }
                }
            }
        }
    }
}
