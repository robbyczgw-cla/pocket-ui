//! Part of the pocket-ui binary: where the App is, the page in front and what covers it, derived from the App's fields
//! in the order `App::paint` paints them. It has no state of its own: `App::where_now` reads the same fields the painter
//! reads, so it can't drift from what is on screen (`painted`, set by `paint`, says which painter ran; `@painted` and
//! `@painter` in the navigation explorer's record, `tests/where_states.rs` over every snapshot state, compare the two).
//!
//! Three readers: the navigation explorer's `@where` (`app_nav_probe`), the status file `/tmp/pocket-ui.status`
//! (`pocket_ui::ui_status`, written by `status_tick` when it changes) and the logs. A name is a machine code, the same in
//! all three: "shelf:Home", "settings:List Look", "settings:Controllers (pause)", "pause:Options", "setup:wifi
//! (settings)", "card:Delete", "kids:pin", with what covers it after " / " ("notice", "quick", "power"…).
//! docs/DEVICE-TESTS.md "Where the UI is" lists them.

use std::fmt;
use std::time::Instant;

use pocket_ui::game_card::Mode as CardMode;
use pocket_ui::ui_status::Line;

use super::app_curator::Page as CuratorPage;
use super::app_kids::KidsView;

use super::{app_settings_groups, App, Page, Screen, SettingsPage, FADE, SETTINGS_GROUPS};

/// The page in front: what `App::paint` paints.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Place {
    /// "Saving…", "Switching off…" over everything (`Sleep::overlay`).
    PoweringOff,
    /// Kids Mode's own screens (`app_kids::KidsView`).
    Kids(&'static str),
    /// A tool or PC streaming is starting: its window is not up yet.
    AppStarting,
    /// A Settings page; the Manual also over the first start (`over_setup`), Controllers also over the pause menu
    /// (`over_pause`): B goes back there.
    Settings { page: SettingsPage, over_setup: bool, over_pause: bool },
    /// The feature cards (Settings → Tour, "New in this version").
    Tour,
    /// A step of the first start, or of a Settings row that opens it (`from_settings`).
    Setup { at: String, from_settings: bool, network_menu: bool },
    PortFiles,
    ControlsCard,
    /// A Curator page over the shelf or the card.
    Curator(&'static str),
    /// "Runs on", over the card or the console page.
    RunsOn,
    ThisGame,
    PortPerf,
    /// A game's screenshots, over its card.
    Gallery,
    /// A save's two versions, over its card.
    Conflicts,
    /// The game card, with its mode when it is more than shown ("Delete").
    Card(Option<&'static str>),
    /// The box, open over the card (X on a game, `game_box`).
    Box,
    Console,
    Search,
    /// Recent games, on the shelf or over a game.
    Switcher { over_game: bool },
    /// The pause menu's page (the Big shelf's own is "Big").
    Pause(Option<Page>),
    /// A game in front, pocket-ui behind it.
    Game,
    Shelf(Screen),
}

/// What is painted over the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cover {
    WhereWasI,
    Notice,
    Quick,
    /// The Date and time page, by its view (`Sys::time_view`).
    Time(&'static str),
    Power,
    Leave,
    Slideshow,
}

impl Cover {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Cover::WhereWasI => "where was I",
            Cover::Notice => "notice",
            Cover::Quick => "quick",
            Cover::Time(view) => view,
            Cover::Power => "power",
            Cover::Leave => "leave",
            Cover::Slideshow => "slideshow",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Where {
    pub place: Place,
    pub covers: Vec<Cover>,
}

/// A Settings page by name (without its selected row: `Leds(3)` is "Leds"; a category's list is one page per category).
fn settings_name(page: SettingsPage) -> String {
    use SettingsPage::*;
    match page {
        Groups(_) => "Groups",
        List(row) => return format!("List {}", SETTINGS_GROUPS[app_settings_groups::group_of(row)].0.trim_end_matches('\u{200b}')),
        Store => "Store",
        Starter => "Starter",
        GameCheck => "GameCheck",
        Updates => "Updates",
        Battery(_) => "Battery",
        Manual => "Manual",
        About => "About",
        Sync => "Sync",
        Leds(_) => "Leds",
        NightLight(_) => "NightLight",
        CozyExtras(_) => "CozyExtras",
        Buttons(_) => "Buttons",
        Controllers(_) => "Controllers",
        Tailscale => "Tailscale",
        Bluetooth => "Bluetooth",
        Web => "Web",
        PerfFan => "PerfFan",
        Curator => "Curator",
        Achievements => "Achievements",
        Tools => "Tools",
        TestButtons => "TestButtons",
        PcStreaming => "PcStreaming",
        Backup => "Backup",
        Storage => "Storage",
        Gamelist => "Gamelist",
        Screenshots => "Screenshots",
        SleepPower => "SleepPower",
    }
    .into()
}

/// Kids Mode's screen by name ("kids:pin").
fn kids_name(view: KidsView) -> &'static str {
    match view {
        KidsView::Flow => "setup",
        KidsView::Pin => "pin",
        KidsView::Adult(_) => "grown-up",
        KidsView::Warning => "time warning",
        KidsView::Pause => "pause",
        KidsView::Stuck => "ask a grown-up",
        KidsView::TimeUp => "time up",
        KidsView::Shelf => "shelf",
    }
}

/// A Curator page by name ("curator:Lens").
fn curator_name(page: &CuratorPage) -> &'static str {
    match page {
        CuratorPage::Lens(_) => "Lens",
        CuratorPage::Echoes(_) => "Echoes",
        CuratorPage::Versions(_) => "Versions",
        CuratorPage::Stickers { .. } => "Stickers",
        CuratorPage::Options { .. } => "Options",
        CuratorPage::Order { .. } => "Order",
        CuratorPage::Name(_) => "Name",
        CuratorPage::History(_) => "History",
        CuratorPage::Review(_) => "Review",
    }
}

/// The game card's mode by name ("card:Delete"); None while the card is only shown.
fn card_mode(mode: &CardMode) -> Option<&'static str> {
    match mode {
        CardMode::View => None,
        CardMode::Status { .. } => Some("Status"),
        CardMode::Note(_) => Some("Note"),
        CardMode::Choose { .. } => Some("Choose"),
        CardMode::Manage { .. } => Some("Manage"),
        CardMode::Delete(_) => Some("Delete"),
    }
}

/// A pause menu page by name, without its selected row (which group or category is a page of its own).
fn pause_name(page: Page) -> String {
    match page {
        Page::Main => "Main".into(),
        Page::LoadBlocked => "LoadBlocked".into(),
        Page::Options { .. } => "Options".into(),
        Page::Group { group, .. } => format!("Group {{ group: {group} }}"),
        Page::Emulator { cat, .. } => format!("Emulator {{ cat: {cat:?} }}"),
        Page::Controls { .. } => "Controls".into(),
        Page::ControlsCard => "ControlsCard".into(),
        Page::Cheats { .. } => "Cheats".into(),
        Page::Standalone { .. } => "Standalone".into(),
        Page::Achievements { .. } => "Achievements".into(),
        Page::PixelPath => "PixelPath".into(),
        Page::EmuChoice => "EmuChoice".into(),
        Page::Players => "Players".into(),
        Page::Zoom => "Zoom".into(),
        Page::PortPerf => "PortPerf".into(),
        Page::QuitUnsaved { .. } => "QuitUnsaved".into(),
    }
}

/// A shelf by name (which system's list is a shelf of its own: "Games(3)").
fn shelf_name(screen: Screen) -> String {
    match screen {
        Screen::Home => "Home".into(),
        Screen::Games(s) => format!("Games({s})"),
        Screen::Favorites => "Favorites".into(),
        Screen::Curator => "Curator".into(),
        Screen::Shelf(s) => format!("Shelf({s})"),
        Screen::Sticker(s) => format!("Sticker({s})"),
        Screen::LensResults => "LensResults".into(),
        Screen::Connection => "Connection".into(),
        Screen::Rediscover => "Rediscover".into(),
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::PoweringOff => f.write_str("powering off"),
            Place::Kids(view) => write!(f, "kids:{view}"),
            Place::AppStarting => f.write_str("app"),
            Place::Settings { page, over_setup, over_pause } => {
                write!(f, "settings:{}", settings_name(*page))?;
                if *over_setup {
                    f.write_str(" (setup)")?;
                }
                if *over_pause {
                    f.write_str(" (pause)")?;
                }
                Ok(())
            }
            Place::Tour => f.write_str("tour"),
            Place::Setup { at, from_settings, network_menu } => {
                write!(f, "setup:{at}")?;
                if *from_settings {
                    f.write_str(" (settings)")?;
                }
                if *network_menu {
                    f.write_str(" / network menu")?;
                }
                Ok(())
            }
            Place::PortFiles => f.write_str("port files"),
            Place::ControlsCard => f.write_str("controls card"),
            Place::Curator(page) => write!(f, "curator:{page}"),
            Place::RunsOn => f.write_str("runs on"),
            Place::ThisGame => f.write_str("this game"),
            Place::PortPerf => f.write_str("port performance"),
            Place::Gallery => f.write_str("screenshots"),
            Place::Conflicts => f.write_str("save versions"),
            Place::Card(None) => f.write_str("card"),
            Place::Card(Some(mode)) => write!(f, "card:{mode}"),
            Place::Box => f.write_str("box"),
            Place::Console => f.write_str("console"),
            Place::Search => f.write_str("search"),
            Place::Switcher { over_game: false } => f.write_str("recent games"),
            Place::Switcher { over_game: true } => f.write_str("recent games (game)"),
            Place::Pause(None) => f.write_str("pause:Big"),
            Place::Pause(Some(page)) => write!(f, "pause:{}", pause_name(*page)),
            Place::Game => f.write_str("game"),
            Place::Shelf(screen) => write!(f, "shelf:{}", shelf_name(*screen)),
        }
    }
}

impl fmt::Display for Where {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.place)?;
        for c in &self.covers {
            write!(f, " / {}", c.name())?;
        }
        Ok(())
    }
}

/// Which of `App::paint`'s painters painted a frame (`App::painted`), and the one a `Place` belongs to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Painter {
    PoweringOff,
    Kids,
    AppStarting,
    Manual,
    Tour,
    Setup,
    Settings,
    PortFiles,
    ControlsCard,
    Curator,
    RunsOn,
    ThisGame,
    PortPerf,
    Gallery,
    Conflicts,
    Card,
    Console,
    Search,
    Switcher,
    Pause,
    BigPause,
    Home,
}

impl Painter {
    /// By name, for the explorer's `@painted`/`@painter` and tests/where_states.rs.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Painter::PoweringOff => "powering off",
            Painter::Kids => "kids",
            Painter::AppStarting => "app starting",
            Painter::Manual => "manual",
            Painter::Tour => "tour",
            Painter::Setup => "setup",
            Painter::Settings => "settings",
            Painter::PortFiles => "port files",
            Painter::ControlsCard => "controls card",
            Painter::Curator => "curator",
            Painter::RunsOn => "runs on",
            Painter::ThisGame => "this game",
            Painter::PortPerf => "port perf",
            Painter::Gallery => "gallery",
            Painter::Conflicts => "conflicts",
            Painter::Card => "card",
            Painter::Console => "console",
            Painter::Search => "search",
            Painter::Switcher => "switcher",
            Painter::Pause => "pause",
            Painter::BigPause => "big pause",
            Painter::Home => "home",
        }
    }
}

impl Place {
    /// Which of `App::paint`'s painters paints this place (the tag `paint` leaves in `App::painted`).
    pub(crate) fn painter(&self) -> Painter {
        match self {
            Place::PoweringOff => Painter::PoweringOff,
            Place::Kids(_) => Painter::Kids,
            Place::AppStarting => Painter::AppStarting,
            Place::Settings { page: SettingsPage::Manual, .. } => Painter::Manual,
            Place::Settings { .. } => Painter::Settings,
            Place::Tour => Painter::Tour,
            Place::Setup { .. } => Painter::Setup,
            Place::PortFiles => Painter::PortFiles,
            Place::ControlsCard => Painter::ControlsCard,
            Place::Curator(_) => Painter::Curator,
            Place::RunsOn => Painter::RunsOn,
            Place::ThisGame => Painter::ThisGame,
            Place::PortPerf => Painter::PortPerf,
            Place::Gallery => Painter::Gallery,
            Place::Conflicts => Painter::Conflicts,
            Place::Card(_) | Place::Box => Painter::Card,
            Place::Console => Painter::Console,
            Place::Search => Painter::Search,
            Place::Switcher { .. } => Painter::Switcher,
            Place::Pause(None) => Painter::BigPause,
            Place::Pause(Some(_)) => Painter::Pause,
            Place::Game | Place::Shelf(_) => Painter::Home,
        }
    }
}

impl App {
    /// The page `paint` paints, by the same tests in the same order (keep the two in step: `@painted` = `@painter`).
    fn place(&self) -> Place {
        if self.sleep.overlay().is_some() {
            return Place::PoweringOff;
        }
        if let Some(view) = self.kids_view() {
            return Place::Kids(kids_name(view));
        }
        if self.running.as_ref().is_some_and(|r| r.is_app()) && self.launch_at.is_some_and(|t| t.elapsed() >= FADE) {
            return Place::AppStarting;
        }
        let over_pause = self.menu.is_some();
        if self.settings_page == Some(SettingsPage::Manual) {
            return Place::Settings { page: SettingsPage::Manual, over_setup: self.setup.is_some(), over_pause };
        }
        if self.tour.is_open() {
            return Place::Tour;
        }
        if let Some(setup) = &self.setup {
            return Place::Setup { at: setup.at(), from_settings: self.settings_page.is_some(), network_menu: setup.network_menu_open() };
        }
        if let Some(page) = self.settings_page {
            return Place::Settings { page, over_setup: false, over_pause };
        }
        let shelf = self.running.is_none();
        if shelf && self.port_files.is_some() {
            return Place::PortFiles;
        }
        if shelf && self.controls_card_open() {
            return Place::ControlsCard;
        }
        if let Some(page) = self.curator.page.as_ref().filter(|_| shelf) {
            return Place::Curator(curator_name(page));
        }
        if self.emu_choice_on_shelf() {
            return Place::RunsOn;
        }
        if self.this_game_on_shelf() {
            return Place::ThisGame;
        }
        if self.port_perf_open() {
            return Place::PortPerf;
        }
        if self.gallery_over_card() {
            return Place::Gallery;
        }
        if self.conflicts_over_card() {
            return Place::Conflicts;
        }
        if let Some(open) = self.card.as_ref().filter(|_| shelf) {
            return if open.card.boxed.is_some() { Place::Box } else { Place::Card(card_mode(&open.card.mode)) };
        }
        if shelf && self.console.is_some() {
            return Place::Console;
        }
        if shelf && self.find.is_open() {
            return Place::Search;
        }
        if self.switcher.is_some() {
            return Place::Switcher { over_game: !shelf };
        }
        if let Some(m) = &self.menu {
            // The quick settings over the paused game are the pause menu's cover (`paint_game_behind`).
            if self.sys.quick_over_game() {
                return Place::Pause(Some(m.page));
            }
            if self.big_pause() {
                return Place::Pause(None);
            }
            return Place::Pause(Some(m.page));
        }
        if self.running.is_some() {
            Place::Game
        } else {
            Place::Shelf(self.screen)
        }
    }

    /// Where the App is: the page in front and what is painted over it (`draw`'s order).
    pub(crate) fn where_now(&self) -> Where {
        let place = self.place();
        let mut covers = Vec::new();
        if self.slideshow_visible() {
            covers.push(Cover::Slideshow);
            return Where { place, covers };
        }
        if self.where_was_i.is_some() {
            covers.push(Cover::WhereWasI);
        }
        // `paint_notice_over_page`, and Home's own: not over a game, nor over "Switching off…".
        if self.notice.is_some() && self.running.is_none() && place != Place::PoweringOff {
            covers.push(Cover::Notice);
        }
        if self.sys.power_menu_open() {
            covers.push(Cover::Power);
        } else if self.sys.quick_open() {
            covers.push(Cover::Quick);
        } else if let Some(view) = self.sys.time_view() {
            covers.push(Cover::Time(view));
        }
        if self.leave.is_some() {
            covers.push(Cover::Leave);
        }
        Where { place, covers }
    }

    /// The game as far as the UI knows: none, starting (its start picture or "Where was I?" up), playing, paused (the
    /// pause menu), quitting (saving on its way out), or an app (a tool, PC streaming).
    fn game_code(&self) -> &'static str {
        let Some(run) = &self.running else { return "none" };
        if run.is_app() {
            return "app";
        }
        if let Some(m) = &self.menu {
            return if m.quitting { "quitting" } else { "paused" };
        }
        if self.where_was_i.is_some() || self.launch_at.is_some_and(|t| t.elapsed() < FADE) {
            return "starting";
        }
        "playing"
    }

    /// The screen's sleep: awake, waking (on again, presses still swallowed for a moment), light (dark, a press wakes
    /// it), deep (suspended), powering-off.
    fn sleep_code(&self, now: Instant) -> &'static str {
        use pocket_ui::sleep::State;
        match self.sleep.state() {
            State::Awake if self.sleep.quiet_until(now).is_some() => "waking",
            State::Awake => "awake",
            State::Light => "light",
            State::Deep => "deep",
            State::PoweringOff => "powering-off",
        }
    }

    /// The status file's fields (`pocket_ui::ui_status`, docs/DEVICE-TESTS.md "Where the UI is").
    pub(crate) fn status_line(&self, now: Instant) -> Line {
        let w = self.where_now();
        let mut line = Line::new();
        line.field("place", w.place.to_string());
        let covers: Vec<&str> = w.covers.iter().map(|c| c.name()).collect();
        line.field("cover", if covers.is_empty() { "none".to_string() } else { covers.join(",") });
        line.field("sleep", self.sleep_code(now));
        line.field("game", self.game_code());
        self.update_status(&mut line, now);
        line
    }

    /// Writes the status file when what it says changed; the moment it changes by itself (the quiet after waking ends)
    /// for the loop's next wake.
    pub(crate) fn status_tick(&mut self, now: Instant) -> Option<Instant> {
        let line = self.status_line(now);
        let writer = self.status.as_mut()?;
        if let Err(e) = writer.offer(&line) {
            if !std::mem::replace(&mut self.status_failed, true) {
                eprintln!("status: couldn't write {}: {e}", writer.path().display());
            }
        }
        self.sleep.quiet_until(now)
    }
}

impl App {
    /// The shelf is the page in front (a notice, the quick settings or a dialog may be over it): what is drawn of it moves.
    pub(crate) fn shelf_in_front(&self) -> bool {
        matches!(self.place(), Place::Shelf(_))
    }

    /// The shelf with nothing over it: the moment for a notice of its own ("New in this version", low space, a newer
    /// pocket-ui, the wrong clock), and the only time a tap belongs to its cases.
    pub(crate) fn shelf_alone(&self) -> bool {
        let w = self.where_now();
        matches!(w.place, Place::Shelf(_)) && w.covers.is_empty()
    }

    /// What the UI sounds and the haptic tick compare before and after an input: what covers the page (for the sounds of a
    /// menu opening and closing) and one mark of where the UI is and what its page shows. The mark is the page's name, the
    /// state of the page in front (`page_mark`, one arm per `Place`: a new page must say what moves on it), its covers',
    /// and the values rows show anywhere. A direction that changes the mark gets the tick and a sound, everywhere.
    pub(crate) fn feedback_view(&self) -> pocket_ui::ui_sound::View {
        use pocket_ui::ui_sound::{Overlay, View};
        use std::hash::{Hash, Hasher};
        let w = self.where_now();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        w.to_string().hash(&mut h);
        self.page_mark(&w.place, &mut h);
        for c in &w.covers {
            match c {
                Cover::Quick | Cover::Time(_) | Cover::Power => self.sys.ui_mark().hash(&mut h),
                Cover::Leave => self.leave.as_ref().map(|d| d.sel).hash(&mut h),
                Cover::WhereWasI | Cover::Notice | Cover::Slideshow => {}
            }
        }
        // Values the rows show (Settings, the quick settings, a card's facts), and the toast an input may say.
        (self.theme, self.frames, self.over_game, self.pixel_path, pocket_ui::rumble::menu_haptics(), self.sound, self.sleep.config).hash(&mut h);
        (app_settings_groups::Switches::now(self.appearance, self.button_layout), self.cozy_extras_mark(), self.toast.as_ref().map(|t| t.1)).hash(&mut h);
        let overlay = if self.sys.power_menu_open() {
            Overlay::PowerMenu
        } else if matches!(w.place, Place::Switcher { .. }) {
            Overlay::Switcher
        } else if self.sys.quick_open() {
            // Also over the pause menu it was opened from.
            Overlay::QuickSettings
        } else if self.menu.is_some() {
            Overlay::PauseMenu
        } else {
            Overlay::None
        };
        View { overlay, at: h.finish() }
    }

    /// What can move on the page in front.
    fn page_mark(&self, place: &Place, h: &mut std::collections::hash_map::DefaultHasher) {
        use std::hash::Hash;
        match place {
            Place::PoweringOff | Place::AppStarting | Place::Game => {}
            Place::Kids(_) => (self.kids_view(), self.sel, self.kids.stop_selected).hash(h),
            Place::Settings { page, .. } => {
                page.hash(h);
                if *page == SettingsPage::Manual {
                    self.manual.mark().hash(h);
                }
                // Each Settings page's own rows (one tuple hashes at most twelve fields).
                (self.updates.sel, self.sleep_page.sel, self.about_mark(), self.sync_row(), self.tailscale_row(), self.bluetooth_row(), self.perf_fan_row(), self.achievements_row(), self.store.ui_mark(), self.starter.mark()).hash(h);
                (self.tools_row(), self.pc_streaming_row(), self.backup_row(), self.web_row(), self.night_light_row(), self.storage_row(), self.gamelist_row(), self.gallery_row(), self.game_check_mark(), self.conflicts_mark(), self.curator_mark()).hash(h);
            }
            Place::Tour => self.tour_mark().hash(h),
            Place::Setup { .. } => self.setup.as_ref().map(|s| s.ui_mark()).hash(h),
            Place::PortFiles => self.port_files.as_ref().map(|(page, _)| (&page.dir, page.scroll())).hash(h),
            Place::ControlsCard => self.controls_card_mark().hash(h),
            Place::Curator(_) => self.curator_mark().hash(h),
            Place::RunsOn => self.emu_choice_mark().hash(h),
            Place::ThisGame => self.this_game_mark().hash(h),
            Place::PortPerf => self.port_perf_mark().hash(h),
            Place::Gallery => self.gallery_row().hash(h),
            Place::Conflicts => self.conflicts_mark().hash(h),
            Place::Card(_) | Place::Box => {
                if let Some(open) = &self.card {
                    (open.card.scroll, open.card.mode_mark(), open.card.data.status, &open.card.data.note).hash(h);
                }
                self.curator_mark().hash(h);
            }
            Place::Console => self.console.as_ref().map(|o| (o.system, o.page.mark())).hash(h),
            Place::Search => self.find_mark().hash(h),
            Place::Switcher { .. } => {
                if let Some(sw) = &self.switcher {
                    (sw.sel, sw.entries.len(), sw.entries.iter().filter(|e| e.favorite).count(), sw.switching).hash(h);
                }
            }
            Place::Pause(_) => {
                if let Some(m) = &self.menu {
                    (m.page, m.sel, m.slot, m.quitting, m.settings.as_ref().map(|s| &s.values), m.cheats.as_ref().map(|c| c.enabled()), m.pixel.as_ref().map(|p| p.mark())).hash(h);
                    m.players.as_ref().map(|p| p.mark()).hash(h);
                }
                // The Big shelf's and Kids Mode's three-row pause (Continue, Stop, More).
                (self.emu_choice_mark(), self.port_perf_mark(), self.kids.stop_selected, self.big_more_selected()).hash(h);
            }
            Place::Shelf(screen) => (screen, self.sel).hash(h),
        }
    }
}

impl App {
    /// The page a press belongs to, for its release and its repeat: the page in front and what covers it, as `@where` names
    /// them ("shelf:Home / power", "settings:List Look", "pause:Main / quick"); a tool starting and running is one page.
    fn press_page(&self) -> String {
        let mut w = self.where_now();
        if w.place == Place::AppStarting {
            w.place = Place::Game;
        }
        w.to_string()
    }

    /// The buttons whose release or repeat does something of its own: HOME, START and SELECT (their taps), and the
    /// directions that repeat on the shelf. A game's buttons are the game's: nothing is kept for them.
    fn press_matters(&self, button: pocket_ui::pad::Button) -> bool {
        use pocket_ui::pad::Button::*;
        matches!(button, Mode | Start | Select) || (self.running.is_none() && matches!(button, Up | Down | Left | Right | L1 | R1 | L2 | R2))
    }

    /// One rule for releases (audit #4, 2026-11-09): a button's release counts on the page that saw its press. A press
    /// notes its page; a release on another page (the press opened it, or a page came up by itself meanwhile: the power
    /// menu, a game that ended) is still delivered, for what holds and modifiers track, but its tap is no tap there:
    /// HOME's (the quick settings on the shelf, the pause menu in a game, Kids Mode's), START's (Settings) and SELECT's
    /// (Recent games). Until then each such case had a patch of its own.
    pub(crate) fn note_press_page(&mut self, button: pocket_ui::pad::Button, pressed: bool) {
        if !self.press_matters(button) && !self.press_pages.iter().any(|(b, _)| *b == button) {
            return;
        }
        let here = self.press_page();
        let before = self.press_pages.iter().position(|(b, _)| *b == button).map(|i| self.press_pages.remove(i).1);
        if pressed {
            self.press_pages.push((button, here));
        } else if before.as_deref() != Some(here.as_str()) {
            self.release_elsewhere(button);
        }
    }

    /// The held direction `button` still on the page its press was on (a direction's repeat goes on only then). Left and
    /// Right repeat as Up and Down (`repeat_after`).
    pub(crate) fn on_press_page(&self, button: pocket_ui::pad::Button) -> bool {
        use pocket_ui::pad::Button::*;
        let here = self.press_page();
        let pressed = |b: pocket_ui::pad::Button| b == button || matches!((button, b), (Up, Left) | (Down, Right));
        self.press_pages.iter().any(|(b, at)| pressed(*b) && *at == here)
    }

    /// `button` was let go on another page than its press's: what its release would start there is disarmed.
    fn release_elsewhere(&mut self, button: pocket_ui::pad::Button) {
        use pocket_ui::pad::Button::*;
        match button {
            Mode => {
                self.home_tap = false;
                self.sys.cancel_home_tap();
                self.kids_cancel_home_tap();
            }
            Start => self.start_tap.disarm(),
            Select => self.select_alone = false,
            _ => {}
        }
        if self.held.is_some_and(|(b, _)| b == button) {
            self.held = None;
        }
    }
}

/// Where the status file goes: beside the command FIFO (the test driver's folder, else /tmp); None where there is no FIFO.
pub(crate) fn status_writer() -> Option<pocket_ui::ui_status::Writer> {
    #[cfg(any(target_os = "linux", target_os = "emscripten"))]
    {
        let path = super::test_driver_dir().map_or_else(|| std::path::PathBuf::from(pocket_ui::ui_status::FILE), |d| d.join("ui.status"));
        Some(pocket_ui::ui_status::Writer::new(path))
    }
    #[cfg(not(any(target_os = "linux", target_os = "emscripten")))]
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_leave_out_the_selected_row_and_keep_which_page_it_is() {
        assert_eq!(pause_name(Page::Options { sel: 2 }), "Options");
        assert_eq!(pause_name(Page::Group { group: 1, sel: 0 }), "Group { group: 1 }");
        assert_eq!(pause_name(Page::Emulator { cat: Some(2), sel: 4 }), "Emulator { cat: Some(2) }");
        assert_eq!(pause_name(Page::Emulator { cat: None, sel: 4 }), "Emulator { cat: None }");
        assert_eq!(settings_name(SettingsPage::Leds(3)), "Leds");
        assert_eq!(settings_name(SettingsPage::Manual), "Manual");
        assert_eq!(shelf_name(Screen::Games(3)), "Games(3)");
        let w = Where { place: Place::Settings { page: SettingsPage::Controllers(1), over_setup: false, over_pause: true }, covers: vec![Cover::Notice, Cover::Quick] };
        assert_eq!(w.to_string(), "settings:Controllers (pause) / notice / quick");
        assert_eq!(Place::Card(Some("Delete")).to_string(), "card:Delete");
        assert_eq!(Place::Card(None).to_string(), "card");
    }
}
