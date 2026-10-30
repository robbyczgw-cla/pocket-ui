//! First start: welcome, look, "How do you like to play?" (the profiles and the feature cards, `features`), Wi-Fi
//! with the on-screen keyboard, extras (cover art, PortMaster), how to add games, done.
//! Every step can be skipped with START. Slow work (scan, connect, sharing) runs on threads
//! and reports back through `notify`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::covers;
use crate::game_pictures;
use crate::features::{self, Cards, Profile, Switches};
use crate::image::{self, Image};
use crate::port_files::{self, FilesPage, Share};
use crate::ports::{self, Port};
use crate::gfx::Canvas;
use crate::keyboard::{Key, Keyboard, Outcome, Special};
use crate::library::{self, SysInfo};
use crate::systems_grid::{self, Grid, Tile};
use crate::net_check;
use crate::pad::Button;
use crate::palette::*;
use crate::text::{Face, Text};
use crate::remote;
use crate::settings::{Animations, Appearance, Art, Motion};
use crate::system_ctl::SharedHw;
use crate::timezone::{self, Guess};
use crate::net_state::{NetState, Notice, NoticeKind, Radio, Scan, ScanError, Snapshot};
use crate::wifi::{self, Entry, Network, Saved, Saving};

#[derive(Debug)]
pub enum SetupEvent {
    /// The shared picture of the Wi-Fi changed (`net_state`): the radio, the saved networks, the address, a scan, a
    /// switch that did not work. The page reads the snapshot.
    Net,
    Connected { ssid: String, result: Result<(), String> },
    /// What the connected network does with a plain web request (`net_check::probe`, and `net_check::recheck` every
    /// 15 s while the page shows a sign-in), with the sign-in page's address.
    Network(net_check::Check),
    /// What the sign-in helper's request `ticket` found (`portal::Session`), and whether the page's certificate couldn't
    /// be checked.
    Portal { ticket: u64, outcome: crate::portal::Outcome, insecure: bool },
    /// A saved network was connected without its password (`wifi::connect_saved`).
    ConnectedSaved { ssid: String, result: Result<(), String> },
    /// A saved network was forgotten (`wifi::forget`).
    Forgot { ssid: String, result: Result<(), String> },
    /// The Wi-Fi country was set (None: Automatic), or why not.
    CountrySet(Result<Option<String>, String>),
    /// File sharing as a thread found it (`systemctl is-active smbd`) and the share's password: after a switch
    /// (`switched`), or the look when a page with it opened (`Setup::look_sharing`).
    Sharing { on: bool, password: Option<String>, switched: bool },
    /// Remote access as a thread found it (`remote::current`, `systemctl`) and its password; `result`: the switch's,
    /// None for the look when the page opened.
    Remote { mode: remote::Mode, password: Option<String>, result: Option<Result<(), String>> },
    /// The time zone step's first look (`Setup::enter_zone`): the guess and the zone list, read on a thread.
    Zone(ZoneFacts),
    /// UTC offsets of zones the step lists, read on a thread (`Setup::zone_offsets_wanted`).
    ZoneOffsets(Vec<(String, Option<i32>)>),
    /// How many games each system's folder holds, and what in `roms` isn't read, found on a thread when the games
    /// step opened.
    GameCounts(Counted),
    Covers { done: usize, total: usize, found: usize },
    Catalog(Result<Vec<Port>, String>),
    /// `name`: the catalog name of the port, to open its "Copy your game files" page after an install.
    PortDone { name: String, title: String, result: Result<(), String>, installing: bool, updated: bool },
}

/// The row at the top of the Wi-Fi page, when there is one (`Setup::lead_kind`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lead {
    Off,
    Portal,
}

/// The Wi-Fi page's re-check while it shows a sign-in: what asks, and how often (tests: a stand-in and milliseconds).
#[derive(Clone, Copy)]
struct Recheck {
    ask: fn() -> net_check::Check,
    every: std::time::Duration,
}

impl Default for Recheck {
    fn default() -> Self {
        Recheck { ask: net_check::recheck, every: std::time::Duration::from_secs(crate::portal_page::RECHECK_SECS) }
    }
}

/// The sign-in helper's curl, the check's address and its rules (the end-to-end test points them at a stand-in).
#[derive(Clone)]
struct PortalIo {
    curl: String,
    check: String,
    rules: crate::portal::Rules,
}

impl Default for PortalIo {
    fn default() -> Self {
        PortalIo { curl: "curl".into(), check: net_check::PROBE_URL.into(), rules: crate::portal::Rules::default() }
    }
}

/// The time zone step as a thread found it: the guess, the country, the zone list and the offsets the first screen shows.
pub struct ZoneFacts(ZoneStep);

impl std::fmt::Debug for ZoneFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ZoneFacts({:?})", self.0.guess)
    }
}

impl SetupEvent {
    /// True when files the shelf shows (covers, installed ports) have changed.
    pub fn changes_library(&self) -> bool {
        matches!(self, SetupEvent::Covers { done, total, .. } if done == total) || matches!(self, SetupEvent::PortDone { .. })
    }
}

pub type Notify = Arc<dyn Fn(SetupEvent) + Send + Sync>;

/// How often the Wi-Fi page looks at the radio while it is up.
const RADIO_LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(4);

/// How long a network may stay "Connecting…" before the page says it isn't answering (a refused password kept iwd
/// trying for six minutes on a fresh card, 2026-10-27; a good one takes a few seconds, DHCP at most 45).
const CONNECT_STUCK: std::time::Duration = std::time::Duration::from_secs(60);

/// The Wi-Fi of a setup that was not given the panel's (`Setup::with_net`).
#[cfg(not(test))]
fn default_net(hw: &SharedHw) -> Arc<NetState> {
    NetState::new(Arc::new(crate::net_state::HwNet::new(hw.clone())))
}

/// Tests never reach the machine's own Wi-Fi: a radio that is on and sees nobody, run where it is asked.
#[cfg(test)]
fn default_net(_hw: &SharedHw) -> Arc<NetState> {
    NetState::inline(crate::net_state::fake::FakeNet::new(Radio::On))
}

/// Rows of the Extras page: Cover art, Game pictures, PortMaster games.
const EXTRAS_ROWS: usize = 3;

/// Setup steps that Settings can open on their own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Only {
    Look,
    Wifi,
    Extras,
    Games,
    /// Remote access over SSH; only opened from Settings, not part of the first start.
    Remote,
}

enum Step {
    Welcome,
    Look,
    /// "How do you like to play?": Simple, Enthusiast, Choose yourself (first start only).
    Play { sel: usize },
    /// Choose yourself: one card per feature with a switch.
    Cards(Box<Cards>),
    /// Settings > Look only: which systems the shelf shows.
    ShelfSystems {
        sel: usize,
    },
    /// Settings > Look only: colours, type and button symbols (`theme_page`), R2 on the look step.
    Theme {
        sel: usize,
    },
    Wifi,
    /// `hidden`: a network typed by name (Other network), connected to without seeing it first.
    Password { ssid: String, kb: Keyboard, hidden: bool },
    Connecting { ssid: String },
    /// The connected network wants a sign-in: what it wants and the ways on (`portal_page`).
    Portal(Box<crate::portal_page::PortalPage>),
    /// Other network: the hidden network's name.
    HiddenName { kb: Keyboard },
    /// The Wi-Fi country: `list` is (code, name), "Automatic" first with an empty code.
    Country { sel: usize, list: Vec<(String, String)> },
    /// Time zone, right after Wi-Fi: a guess to confirm, or the picker.
    Zone(Box<ZoneStep>),
    Extras { sel: usize },
    Ports,
    /// "Copy your game files" for a port that needs them: X in the browser, and after installing one.
    PortFiles(Box<FilesPage>),
    Games,
    Remote,
    Done,
}

/// The time zone step: what was guessed, and where the user is in the picker.
struct ZoneStep {
    guess: Guess,
    /// "Austria": the country of the guess, for the notes and the list heading.
    country: Option<String>,
    /// Every zone the picker offers (ROCKNIX's list).
    all: Vec<String>,
    view: ZoneView,
    /// Why the system refused a zone.
    error: Option<String>,
    /// A thread is still reading the guess and the zone list (`Setup::enter_zone`): the step says so and waits.
    loading: bool,
    /// UTC offsets of the zones shown, read on threads (`Setup::zone_offsets_wanted`); painting only looks here.
    offsets: HashMap<String, Option<i32>>,
    /// The zones whose offsets were asked for already.
    asked: std::collections::HashSet<String>,
}

enum ZoneView {
    /// The guess, to confirm with one press.
    Card,
    /// "Search…" and the regions; `sel` 0 is "Search…".
    Regions { sel: usize },
    List { heading: String, zones: Vec<String>, sel: usize, from: ListFrom },
    Search(Box<Keyboard>),
}

/// Where a zone list came from, so that B goes back to it.
enum ListFrom {
    /// A country with several zones: the first screen of the step.
    Country,
    Regions,
    Search(String),
}

/// What a press in the time zone step asks for.
enum ZoneNext {
    Stay,
    Back,
    /// Leave the step with this zone, or with the guess when `None`.
    Take(Option<String>),
}

impl ZoneStep {
    /// The step before its thread has answered.
    fn looking() -> ZoneStep {
        ZoneStep {
            guess: Guess::Utc,
            country: None,
            all: Vec::new(),
            view: ZoneView::Card,
            error: None,
            loading: true,
            offsets: HashMap::new(),
            asked: Default::default(),
        }
    }

    /// The zones the screen shows that have no offset yet and were not asked for.
    fn wanted(&self) -> Vec<String> {
        let shown: Vec<&str> = match &self.view {
            ZoneView::Card if !self.loading => self.guess.zone().into_iter().collect(),
            ZoneView::List { zones, .. } => zones.iter().map(String::as_str).collect(),
            _ => Vec::new(),
        };
        shown.into_iter().filter(|z| !self.asked.contains(*z)).map(String::from).collect()
    }

    /// The first screen: the guess, or the country's own list when the guess is a whole country.
    fn start_view(&self) -> ZoneView {
        match &self.guess {
            Guess::Several { zones, .. } => ZoneView::List {
                heading: self.country.clone().unwrap_or_else(|| "Time zone".into()),
                zones: zones.clone(),
                sel: 0,
                from: ListFrom::Country,
            },
            _ => ZoneView::Card,
        }
    }

    /// The row of the region the guess is in (row 0 is "Search…").
    fn region_row(&self) -> usize {
        let zone = match &self.guess {
            Guess::Several { zones, .. } => zones.first().map(String::as_str),
            Guess::Utc => None,
            g => g.zone(),
        };
        zone.and_then(|z| timezone::regions(&self.all).iter().position(|r| r == timezone::region_of(z))).map_or(0, |i| i + 1)
    }
}

pub struct Setup {
    step: Step,
    notify: Notify,
    /// The Wi-Fi as quick settings see it too (`net_state`): the page shows what it finds there and asks it to scan,
    /// switch, connect.
    net: Arc<NetState>,
    /// Keeps the subscription to `net` alive (it sends `SetupEvent::Net`).
    net_sub: Option<Arc<dyn Fn() + Send + Sync>>,
    /// The radio as `net` last said, and the scan's `scan_rev` this page took.
    radio: Radio,
    scan_rev: u64,
    /// A switch is under way (`net`): the state it goes to.
    switching: Option<bool>,
    /// The last scan failed (the radio is on): why. The page says it, never "No networks found".
    scan_err: Option<ScanError>,
    /// A switch of the radio did not work (what was asked for): said under the title until the radio is as asked or the
    /// next try.
    switch_failed: Option<Notice>,
    /// The newest switch failure of `net` already taken.
    seen_notice: u64,
    /// While the page is up, a thread looks at the radio every `radio_every` (it may be switched elsewhere: the boot
    /// hooks, quick settings); ends when this goes (`keep_watch`).
    radio_watch: Option<Arc<()>>,
    radio_every: std::time::Duration,
    nets: Vec<Network>,
    /// A on the connected network was pressed and nothing since: the status line says so, with a tick drawn before it (`confirming`).
    confirm: bool,
    /// How many scans in a row each network of `nets` has been missing from (`keep_seen`).
    missed: HashMap<String, u8>,
    /// Saved networks, the Wi-Fi country, the address (`wifi::saving`), and the page's rows made of both.
    saving: Saving,
    /// zoneinfo's `iso3166.tab`: country names for the Wi-Fi country (read on the scan's thread).
    iso_tab: String,
    entries: Vec<Entry>,
    /// Y was pressed on this saved network once: the next Y forgets it, anything else keeps it.
    forget: Option<String>,
    /// The network NetworkManager is bringing up (`Saved::connecting`) and since when this page sees it so: after
    /// `CONNECT_STUCK` the page says it isn't answering, and A types the password again.
    connecting_since: Option<(String, Instant)>,
    /// The last typed connection was to a hidden network: a refused password asks again the same way.
    connect_hidden: bool,
    sel: usize,
    scanning: bool,
    /// Last Wi-Fi result shown under the title; `true` marks an error.
    note: Option<(String, bool)>,
    /// The connected network answers with a sign-in page (`net_check`, asked on a thread after a scan).
    portal: bool,
    /// Where its check was sent instead (`net_check::Check::portal`).
    portal_at: Option<String>,
    /// While the page shows a sign-in, a thread asks again every `recheck.every` (`keep_watch`); it ends when this
    /// goes (the page closed, the network answered online).
    watch: Option<Arc<()>>,
    recheck: Recheck,
    /// The sign-in helper's session (its cookie jar), while "Sign in…" is open; and what it runs (tests: a stand-in).
    portal_session: Option<Arc<crate::portal::Session>>,
    portal_io: PortalIo,
    sharing: bool,
    sharing_busy: bool,
    /// A thread is looking at file sharing (`look_sharing`): never `systemctl` on the UI thread.
    share_looking: bool,
    /// This device's file sharing password, once it exists.
    share_password: Option<String>,
    remote: remote::Mode,
    remote_busy: bool,
    /// A thread is looking at remote access (`look_remote`).
    remote_looking: bool,
    remote_password: Option<String>,
    remote_error: Option<String>,
    /// Address shown on the games and remote steps, read once when the step opens.
    ip: Option<String>,
    started: Instant,
    /// (name, one-line description) of each theme, and the one currently shown.
    themes: Vec<(&'static str, &'static str)>,
    theme: usize,
    /// Settings > Look > Appearance, changed with Y on the look step.
    appearance: Appearance,
    /// Settings > Look > Motion, changed with R1 on the look step (Settings > Look only).
    motion: Motion,
    /// The language chosen on the welcome step (Left/Right), until the app has kept it (`take_language`).
    language_chosen: Option<crate::i18n::Lang>,
    /// The text size chosen with R1 on the first start's look step, until the app has kept it (`take_text_size`).
    text_size_chosen: Option<crate::type_scale::TextSize>,
    /// Animations (L1 on the look step): the player's choice, None until then (the look's own shows); and whether the
    /// app still has to keep a change (`take_animations`).
    animations: Option<Animations>,
    animations_changed: bool,
    data_dir: PathBuf,
    /// (system id, ROM path) of every game, for the cover art download.
    games: Vec<(String, PathBuf)>,
    /// Cover art progress: (done, total, found); `None` before the first run.
    covers: Option<(usize, usize, usize)>,
    covers_running: bool,
    /// Settings > Games > Extras > Pictures: what is fetched by itself, and whether the last press changed it
    /// (`take_art`).
    art: Art,
    art_changed: bool,
    ports: Vec<Port>,
    /// Snapshots and tests: catalogue names that count as having an update (`with_updates`).
    fake_updates: Option<Vec<String>>,
    /// Which of the browser's two lists is shown, and the selected row in each.
    port_tab: PortTab,
    port_sel: [usize; 2],
    catalog_loading: bool,
    /// Title of the port being installed or removed.
    port_busy: Option<String>,
    port_note: Option<(String, bool)>,
    previews: HashMap<String, Option<Image>>,
    /// Opened from Settings for one step: leaving that step ends setup instead of moving on.
    only: Option<Only>,
    /// The system controls: the time zone step reads and sets the zone through them.
    hw: SharedHw,
    /// Settings > Look > Systems on the shelf: the systems with games, the hidden ones, whether that changed,
    /// and why the last tick was refused.
    shelf: Vec<ShelfSystem>,
    hidden: Vec<String>,
    hidden_changed: bool,
    shelf_note: Option<String>,
    /// "PC streaming" on Home, a row after the systems: None when Moonlight isn't installed (no row), and
    /// whether that tick changed.
    streaming: Option<bool>,
    streaming_changed: bool,
    /// Every system pocket-ui can start (es_systems.cfg), and the grid of them on the games step.
    catalog: Vec<SysInfo>,
    grid: Option<Grid>,
    /// A thread is counting the games of the folders (`count_games`).
    counting: bool,
    /// What that thread found in `roms` that isn't read: folders of no system, disc games missing a file.
    strays: Vec<library::StrayFolder>,
    broken: Vec<library::BrokenDisc>,
    /// Tests: the answers of the threads that `settle` waits for.
    #[cfg(test)]
    inbox: Option<std::sync::mpsc::Receiver<SetupEvent>>,
    /// The switches the profiles and cards set (`features::Switches`), the cards put off, and whether either
    /// changed since the app last took them.
    switches: Switches,
    later: Vec<String>,
    tour_changed: bool,
}

/// The PortMaster browser's two lists: ports that run as they are, and ports that need files from the
/// player's own copy of the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PortTab {
    Ready,
    OwnFiles,
}

impl PortTab {
    fn index(self) -> usize {
        self as usize
    }

    fn label(self) -> &'static str {
        match self {
            PortTab::Ready => "Ready to play",
            PortTab::OwnFiles => "Bring your own game files",
        }
    }

    fn other(self) -> PortTab {
        match self {
            PortTab::Ready => PortTab::OwnFiles,
            PortTab::OwnFiles => PortTab::Ready,
        }
    }
}

/// Where BIOS files go (the share hides this folder; see docs/user/getting-started.md).
const BIOS_DIR: &str = "/storage/roms/bios";

/// A system with games, for Settings > Look > Systems on the shelf.
#[derive(Clone, Debug)]
pub struct ShelfSystem {
    pub id: String,
    pub name: String,
    pub games: usize,
}

impl Setup {
    /// The systems that have games and the ones hidden from the shelf (Settings > Look).
    pub fn with_shelf(mut self, systems: Vec<ShelfSystem>, hidden: Vec<String>) -> Self {
        self.shelf = systems;
        self.hidden = hidden;
        self
    }

    /// "PC streaming" as a tick after the systems, when Moonlight is `installed`; `on`: it is on Home now.
    pub fn with_streaming(mut self, installed: bool, on: bool) -> Self {
        self.streaming = installed.then_some(on);
        self
    }

    /// Whether "PC streaming" is on Home after its tick changed, once; the app saves it.
    pub fn take_streaming(&mut self) -> Option<bool> {
        std::mem::take(&mut self.streaming_changed).then_some(self.streaming).flatten()
    }

    /// Rows on the Systems on the shelf page: the systems, then PC streaming if Moonlight is installed.
    fn shelf_rows(&self) -> usize {
        self.shelf.len() + usize::from(self.streaming.is_some())
    }

    /// Every system pocket-ui can start, for the grid of the games step.
    pub fn with_catalog(mut self, all: Vec<SysInfo>) -> Self {
        self.catalog = all;
        self
    }

    /// The Pictures choice on the Extras page when it opens.
    pub fn with_art(mut self, art: Art) -> Self {
        self.art = art;
        self
    }

    /// The Pictures choice after a press changed it, once; the app keeps it.
    pub fn take_art(&mut self) -> Option<Art> {
        std::mem::take(&mut self.art_changed).then_some(self.art)
    }

    /// The language after Left/Right on the welcome step changed it, once; the app keeps it (Settings → Language).
    pub fn take_language(&mut self) -> Option<crate::i18n::Lang> {
        self.language_chosen.take()
    }

    /// The text size after R1 on the first start's look step changed it, once; the app keeps it (Settings → Look &
    /// feel → Text size).
    pub fn take_text_size(&mut self) -> Option<crate::type_scale::TextSize> {
        self.text_size_chosen.take()
    }

    /// The animations after L1 on the look step changed them, once; the app keeps them (Settings → Look & feel →
    /// Animations).
    pub fn take_animations(&mut self) -> Option<Animations> {
        std::mem::take(&mut self.animations_changed).then(|| self.animations_shown())
    }

    /// Cozy or Quick as the look step shows it: the choice, else the look's own for the look being viewed.
    pub fn animations_shown(&self) -> Animations {
        self.animations.unwrap_or_else(|| Animations::default_for(crate::settings::Theme::ALL.get(self.theme).copied().unwrap_or_default()))
    }

    /// The choice made before (Settings > Look); None follows the look.
    pub fn with_animations(mut self, a: Option<Animations>) -> Self {
        self.animations = a;
        self
    }

    /// The hidden systems after a tick changed them, once; the app saves them.
    pub fn take_hidden(&mut self) -> Option<Vec<String>> {
        std::mem::take(&mut self.hidden_changed).then(|| self.hidden.clone())
    }

    /// Why a tick was refused.
    pub fn shelf_note(&self) -> Option<&str> {
        self.shelf_note.as_deref()
    }

    fn press_shelf(&mut self, b: Button) {
        let Step::ShelfSystems { sel } = &mut self.step else { return };
        let n = self.shelf.len() + usize::from(self.streaming.is_some());
        self.shelf_note = None;
        match b {
            Button::Up => *sel = sel.saturating_sub(1),
            Button::Down => *sel = (*sel + 1).min(n.saturating_sub(1)),
            Button::L1 => *sel = sel.saturating_sub(5),
            Button::R1 => *sel = (*sel + 5).min(n.saturating_sub(1)),
            Button::East if *sel == self.shelf.len() => {
                if let Some(on) = self.streaming.as_mut() {
                    *on = !*on;
                    self.streaming_changed = true;
                }
            }
            Button::East => {
                let Some(sys) = self.shelf.get(*sel) else { return };
                let with_games: Vec<&str> = self.shelf.iter().map(|s| s.id.as_str()).collect();
                match crate::shelf::toggle_hidden(&mut self.hidden, &sys.id, &with_games) {
                    crate::shelf::Toggle::LastOne => self.shelf_note = Some(format!("{} stays: one system is always on the shelf.", sys.name)),
                    _ => self.hidden_changed = true,
                }
            }
            Button::South | Button::Start => self.step = Step::Look,
            _ => {}
        }
    }

    fn paint_shelf_systems(&self, c: &mut Canvas, text: &mut Text, sel: usize) {
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Systems on the shelf");
        let (line, color) = match &self.shelf_note {
            Some(note) => (note.clone(), pal().alert),
            None => ("Untick a system to leave it off the shelf. Its games stay, starred ones in Favourites.".to_string(), pal().dust),
        };
        let line = text.fit(Face::Body, 28, &line, c.w - 2 * MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, 192, color, &line);
        let (top, row) = (232, 76);
        let visible = ((c.h - 130 - top) / row).max(1) as usize;
        let first = sel.saturating_sub(visible / 2).min(self.shelf_rows().saturating_sub(visible));
        let width = c.w - 2 * MARGIN;
        // The systems, then PC streaming (an app with its own tick, no games to count).
        let streaming = self.streaming.map(|on| (ShelfSystem { id: "streaming".into(), name: "PC streaming".into(), games: 0 }, on));
        let rows = self.shelf.iter().map(|s| (s.clone(), !self.hidden.contains(&s.id))).chain(streaming);
        for (k, (sys, shown)) in rows.enumerate().skip(first).take(visible) {
            let y = top + (k - first) as i32 * row;
            let selected = k == sel;
            crate::kit::shelf_row(c, MARGIN, y, width, row - 8, selected);
            let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else if shown { (pal().chalk, pal().dust) } else { (pal().dust, pal().off) };
            // The tick box.
            let (bx, by, bs) = (MARGIN + 28, y + (row - 8 - 34) / 2, 34);
            if shown {
                c.round_rect(bx, by, bs, bs, 6, if selected { pal().ink } else { pal().chalk });
                tick(c, bx, by, bs, if selected { pal().label } else { pal().spine });
            } else {
                c.round_rect(bx, by, bs, bs, 6, fg);
                c.round_rect(bx + 3, by + 3, bs - 6, bs - 6, 4, if selected { pal().label } else { pal().spine });
            }
            // The console, small, between the box and the name.
            let mut x = bx + bs + 24;
            if let Some(art) = crate::system_art::get(&sys.id).map(|a| a.fitted(84, row - 22)) {
                c.blit_sprite(&art, x + (84 - art.w as i32) / 2, y + (row - 8 - art.h as i32) / 2, if shown || selected { 0 } else { 150 });
            }
            x += 84 + 20;
            let count = match sys.games {
                _ if sys.id == "streaming" => "Moonlight".to_string(),
                1 => "1 game".to_string(),
                n => format!("{n} games"),
            };
            let cw = text.width(Face::Body, 24, &count);
            let name = text.fit(Face::Title, 37, &sys.name, MARGIN + width - 28 - cw - 40 - x);
            text.draw(c, Face::Title, 37, x, y + 47, fg, &name);
            text.draw_right(c, Face::Body, 24, MARGIN + width - 28, y + 44, dim, &count);
        }
        let on = match self.shelf.get(sel) {
            Some(s) => !self.hidden.contains(&s.id),
            None => self.streaming == Some(true),
        };
        hints(c, text, &[("A", if on { "Take off the shelf" } else { "Put on the shelf" }), ("B", "Back")]);
    }
}

/// Size of a look's preview in the look step: the 1240x1080 shelf at 50 %.
pub const LOOK_PREVIEW: (i32, i32) = (620, 470);

/// The preview's size in use on a `screen` (w, h): `LOOK_PREVIEW` on the Mini V2's 1240x1080, or 46 % of the shelf
/// with larger text (Settings → Text size), which needs the room under it for the look's name and the two rows of
/// choices. Another screen keeps its own shape: as tall on 1080 lines (1920x1080: 960x540), and on fewer lines that
/// much shorter, so the rows under it still fit (1280x960: 560x420; docs/DEVICES.md).
pub fn look_preview(screen: (i32, i32)) -> (i32, i32) {
    let tall = if crate::type_scale::grow(28) > 0 { 420 } else { LOOK_PREVIEW.1 };
    let (w, h) = (screen.0.max(1), screen.1.max(1));
    let ph = (tall - (1080 - h).max(0)).max(200);
    (ph * w / h, ph)
}

/// Tiles per row in the games step's grid.
const GRID_COLS: usize = 4;
/// The grid's tiles and the gap between them.
const GRID_TILE_H: i32 = 156;
const GRID_GAP: i32 = 14;

impl Setup {
    /// "Add your games": the systems grid, the selected system's file types and BIOS, and file sharing.
    fn paint_games(&self, c: &mut Canvas, text: &mut Text, dots: &str) {
        let width = c.w - 2 * MARGIN;
        // Something in roms isn't read: say so where the subtitle is (two lines move the title up a little).
        let unread = unread_lines(&self.strays, &self.broken, "roms");
        match unread.as_slice() {
            [] => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Add your games");
                let line = text.fit(Face::Body, 28, "One folder per system in roms. Copy games over Wi-Fi with file sharing.", width);
                text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, &line);
            }
            [one] => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Add your games");
                let line = text.fit(Face::Body, 28, one, width);
                text.draw(c, Face::Body, 28, MARGIN, 192, pal().chalk, &line);
            }
            lines => {
                text.draw(c, Face::Title, 67, MARGIN, 120, pal().chalk, "Add your games");
                for (i, l) in lines.iter().take(2).enumerate() {
                    let line = text.fit(Face::Body, 24, l, width);
                    text.draw(c, Face::Body, 24, MARGIN, 166 + i as i32 * 34, pal().chalk, &line);
                }
            }
        }
        // Three rows of tiles on 1080 lines, two on the Mini V1's 960, so the details under them stay above the
        // hints (docs/DEVICES.md).
        let grid_rows = ((c.h - 228 - 336) / (GRID_TILE_H + GRID_GAP)).clamp(1, 3) as usize;
        if let Some(grid) = &self.grid {
            self.paint_grid(c, text, grid, 228, width, grid_rows);
        }
        let top = 228 + grid_rows as i32 * (GRID_TILE_H + GRID_GAP) + 6;
        c.rect(MARGIN, top, width, 2, pal().keycap);
        let col_w = (width - 40) / 2;
        if let Some(grid) = &self.grid {
            self.paint_tile_details(c, text, grid, MARGIN, top, col_w);
        }
        self.paint_share(c, text, MARGIN + col_w + 40, top, col_w, dots);
        // The "All systems" tile selected: whether its list is open.
        let more = match self.grid.as_ref().and_then(Grid::selected) {
            Some(Tile::More) => Some(self.grid.as_ref().is_some_and(|g| g.expanded)),
            _ => None,
        };
        hints(c, text, &games_hints(more, self.sharing, self.ip.is_some()));
    }

    fn paint_grid(&self, c: &mut Canvas, text: &mut Text, grid: &Grid, top: i32, width: i32, rows_shown: usize) {
        const GAP: i32 = GRID_GAP;
        const TILE_H: i32 = GRID_TILE_H;
        let cols = grid.cols;
        let tile_w = (width - GAP * (cols as i32 - 1)) / cols as i32;
        let tiles = grid.tiles();
        let rows = tiles.len().div_ceil(cols);
        let first_row = (grid.sel / cols).saturating_sub(1).min(rows.saturating_sub(rows_shown));
        for (i, tile) in tiles.iter().enumerate().skip(first_row * cols).take(rows_shown * cols) {
            let (r, col) = ((i / cols - first_row) as i32, (i % cols) as i32);
            let (x, y) = (MARGIN + col * (tile_w + GAP), top + r * (TILE_H + GAP));
            let selected = i == grid.sel;
            crate::kit::shelf_row(c, x, y, tile_w, TILE_H, selected);
            let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
            match tile {
                Tile::System(s) => {
                    let sys = &grid.all[*s];
                    match crate::system_art::get(&sys.id).map(|a| a.fitted(tile_w - 48, 76)) {
                        Some(art) => c.blit_sprite(&art, x + (tile_w - art.w as i32) / 2, y + 12 + (76 - art.h as i32) / 2, 0),
                        // No picture: a plain cartridge, quiet, where the console would be.
                        None => cartridge(c, x + tile_w / 2, y + 16, if selected { pal().pick(0xc9c5bb, 0xd9d3c6) } else { pal().pick(0x34343a, 0xcdc7ba) }, if selected { pal().label } else { pal().spine }),
                    }
                    let name = text.fit(Face::Title, 30, &systems_grid::short_name(sys), tile_w - 32);
                    text.draw(c, Face::Title, 30, x + 16, y + 120, fg, &name);
                    let games = grid.games(&sys.id);
                    let mut room = tile_w - 32;
                    if games > 0 {
                        // "2 games" where the folder leaves room for it, else just the number.
                        let words = if games == 1 { "1 game".to_string() } else { format!("{games} games") };
                        let fits = text.width(Face::Body, 21, &sys.folder) + text.width(Face::Body, 21, &words) + 34 + 12 <= room;
                        let count = if fits { words } else { games.to_string() };
                        let cw = text.width(Face::Body, 21, &count);
                        text.draw_right(c, Face::Body, 21, x + tile_w - 16, y + 145, fg, &count);
                        tick(c, x + tile_w - 16 - cw - 26, y + 127, 22, if selected { pal().ink } else { pal().chalk });
                        room -= cw + 34;
                    }
                    let folder = text.fit_path(Face::Body, 21, &sys.folder, room);
                    text.draw(c, Face::Body, 21, x + 16, y + 145, dim, &folder);
                }
                Tile::More => {
                    // A plus in a ring (or a minus while open), then the words.
                    let (cx, cy) = (x + tile_w / 2, y + 52);
                    c.ring(cx, cy, 28, 4, fg);
                    c.rect(cx - 13, cy - 2, 26, 4, fg);
                    if !grid.expanded {
                        c.rect(cx - 2, cy - 13, 4, 26, fg);
                    }
                    let label = if grid.expanded { "Fewer systems" } else { "All systems" };
                    let w = text.width(Face::Title, 30, label);
                    text.draw(c, Face::Title, 30, x + (tile_w - w) / 2, y + 120, fg, label);
                    let more = if grid.expanded { "Back to the short list".to_string() } else { format!("{} in all", grid.all.len()) };
                    let w = text.width(Face::Body, 21, &more);
                    text.draw(c, Face::Body, 21, x + (tile_w - w) / 2, y + 145, dim, &more);
                }
            }
        }
        // A thin scroll bar in the margin once there are more rows than fit.
        if rows > rows_shown {
            let track_h = rows_shown as i32 * (TILE_H + GAP) - GAP;
            let x = MARGIN + width + 24;
            c.rect(x, top, 4, track_h, pal().keycap);
            let thumb = (track_h * rows_shown as i32 / rows as i32).max(24);
            let at = (track_h - thumb) * first_row as i32 / (rows - rows_shown) as i32;
            c.rect(x, top + at, 4, thumb, pal().dust);
        }
    }

    /// Under the grid on the left: the selected system's folder, file types and BIOS.
    fn paint_tile_details(&self, c: &mut Canvas, text: &mut Text, grid: &Grid, x: i32, top: i32, w: i32) {
        let label_w = text.width(Face::Body, 24, "Files") + 20;
        match grid.selected() {
            Some(Tile::System(s)) => {
                let sys = &grid.all[s];
                let name = text.fit(Face::Title, 37, &sys.name, w);
                text.draw(c, Face::Title, 37, x, top + 50, pal().chalk, &name);
                let games = grid.games(&sys.id);
                let count = match games {
                    0 => "no games yet".to_string(),
                    1 => "1 game".to_string(),
                    n => format!("{n} games"),
                };
                let line = text.fit(Face::Body, 24, &format!("{}, {count}", sys.folder), w);
                text.draw(c, Face::Body, 24, x, top + 86, pal().dust, &line);
                let (exts, bios) = systems_grid::details(sys, std::path::Path::new(BIOS_DIR));
                text.draw(c, Face::Body, 24, x, top + 124, pal().dust, "Files");
                let exts = if exts.is_empty() { "any".to_string() } else { exts };
                let exts = text.fit(Face::Body, 24, &exts, w - label_w);
                text.draw(c, Face::Body, 24, x + label_w, top + 124, pal().chalk, &exts);
                text.draw(c, Face::Body, 24, x, top + 160, pal().dust, "BIOS");
                let (line, color) = match bios {
                    systems_grid::Bios::NotNeeded => ("Not needed".to_string(), pal().chalk),
                    systems_grid::Bios::Needed { missing: Some(msg), .. } => (msg, pal().alert),
                    systems_grid::Bios::Needed { missing: None, what } => (format!("In place ({what})"), pal().chalk),
                    systems_grid::Bios::Unknown => ("Not known to pocket-ui. If its emulator needs one, it goes in roms/bios.".to_string(), pal().dust),
                };
                for (i, l) in text.wrap(Face::Body, 24, &line, w - label_w).iter().take(3).enumerate() {
                    text.draw(c, Face::Body, 24, x + label_w, top + 160 + i as i32 * 32, color, l);
                }
            }
            Some(Tile::More) => {
                text.draw(c, Face::Title, 37, x, top + 50, pal().chalk, if grid.expanded { "Fewer systems" } else { "All systems" });
                let about = if grid.expanded {
                    "A closes the list again; the popular systems and the ones with games stay."
                } else {
                    "Every system ROCKNIX can start, each with its own folder in roms."
                };
                for (i, l) in text.wrap(Face::Body, 24, about, w).iter().take(3).enumerate() {
                    text.draw(c, Face::Body, 24, x, top + 86 + i as i32 * 32, pal().dust, l);
                }
            }
            None => {}
        }
    }

    /// Under the grid on the right: file sharing, its address and login.
    fn paint_share(&self, c: &mut Canvas, text: &mut Text, x: i32, top: i32, w: i32, dots: &str) {
        let (status, color) = if self.sharing_busy {
            (format!("Switching file sharing{dots}"), pal().dust)
        } else if self.share_looking {
            (format!("Looking at file sharing{dots}"), pal().dust)
        } else if self.sharing {
            ("File sharing is on".to_string(), pal().chalk)
        } else if self.ip.is_none() {
            ("Connect to Wi-Fi first".to_string(), pal().dust)
        } else {
            ("File sharing is off".to_string(), pal().chalk)
        };
        let status = text.fit(Face::Title, 37, &status, w);
        text.draw(c, Face::Title, 37, x, top + 50, color, &status);
        let Some(addrs) = share_addresses(self.ip.as_deref(), self.sharing) else {
            // No address to show: what the switch is for, not an address with "(not connected)" in it, and not one
            // that leads nowhere while sharing is off (the new-user test, 2026-10-30).
            let about = if self.ip.is_some() { SHARE_OFF } else { SHARE_OFFLINE };
            for (i, l) in text.wrap(Face::Body, 24, about, w).iter().take(4).enumerate() {
                text.draw(c, Face::Body, 24, x, top + 92 + i as i32 * 32, pal().dust, l);
            }
            return;
        };
        for (i, addr) in addrs.iter().enumerate() {
            let addr = text.fit(Face::Title, 30, addr, w);
            text.draw(c, Face::Title, 30, x, top + 88 + i as i32 * 36, pal().chalk, &addr);
        }
        let label_w = text.width(Face::Body, 24, "Password") + 20;
        text.draw(c, Face::Body, 24, x, top + 168, pal().dust, "User");
        text.draw(c, Face::Title, 30, x + label_w, top + 168, pal().chalk, wifi::SHARE_LOGIN);
        text.draw(c, Face::Body, 24, x, top + 204, pal().dust, "Password");
        text.draw(c, Face::Title, 30, x + label_w, top + 204, pal().chalk, self.share_password.as_deref().unwrap_or("(not set yet)"));
    }
}

/// Add games without Wi-Fi, under "Connect to Wi-Fi first": what file sharing will do once there is an address.
const SHARE_OFFLINE: &str = "Then a computer on the same network can copy games straight into these folders.";

/// Add games with Wi-Fi but file sharing off: how to start it, and how long it stays on.
const SHARE_OFF: &str = "Press X to turn it on. Then a computer on the same network can copy games into these folders. It stays on until you turn it off or restart.";

/// File sharing's two addresses (macOS and Linux, Windows) for the Pocket's address; None without one, where the
/// first-day run (2026-10-26) found "smb://(not connected)/games-roms" shown as if it were an address, and None while
/// sharing is off: the address would lead nowhere (new-user test, 2026-10-30).
fn share_addresses(ip: Option<&str>, sharing: bool) -> Option<[String; 2]> {
    let ip = ip.filter(|_| sharing)?;
    Some([format!("smb://{ip}/games-roms"), format!("\\\\{ip}\\games-roms")])
}

/// A generic cartridge 76 wide and 68 high, centred on `cx`, for systems without a picture.
fn cartridge(c: &mut Canvas, cx: i32, top: i32, color: u32, bg: u32) {
    c.round_rect(cx - 38, top, 76, 68, 8, color);
    // The grip ridges along the top, the label, and the contacts at the bottom edge.
    for i in 0..3 {
        c.rect(cx - 26, top + 6 + i * 5, 52, 2, bg);
    }
    c.round_rect(cx - 26, top + 24, 52, 30, 4, bg);
    c.rect(cx - 30, top + 60, 60, 8, bg);
    for i in 0..6 {
        c.rect(cx - 27 + i * 10, top + 60, 5, 8, color);
    }
}

/// A check mark inside the box at (x, y), `s` wide.
pub(crate) fn tick(c: &mut Canvas, x: i32, y: i32, s: i32, color: u32) {
    let t = (s / 9).max(3);
    // Short stroke down to the right, then the long one up to the right.
    let (ax, ay) = (x + s * 2 / 9, y + s / 2);
    let (bx, by) = (x + s * 4 / 10, y + s * 7 / 10);
    let (cx, cy) = (x + s * 7 / 9, y + s * 3 / 10);
    for (x0, y0, x1, y1) in [(ax, ay, bx, by), (bx, by, cx, cy)] {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=steps {
            let px = x0 + (x1 - x0) * i / steps;
            let py = y0 + (y1 - y0) * i / steps;
            c.rect(px - t / 2, py - t / 2, t, t, color);
        }
    }
}

/// Which top-level step a (sub)screen belongs to.
fn stage(step: &Step) -> Option<Only> {
    match step {
        Step::Look | Step::ShelfSystems { .. } | Step::Theme { .. } => Some(Only::Look),
        Step::Wifi | Step::Portal(_) | Step::Password { .. } | Step::Connecting { .. } | Step::HiddenName { .. } | Step::Country { .. } => Some(Only::Wifi),
        Step::Extras { .. } | Step::Ports | Step::PortFiles(_) => Some(Only::Extras),
        Step::Games => Some(Only::Games),
        Step::Remote => Some(Only::Remote),
        // Settings > Time is the way to this one on its own.
        // The profiles and the cards are first-start steps; Settings → Tour shows the cards on its own.
        Step::Welcome | Step::Play { .. } | Step::Cards(_) | Step::Zone(_) | Step::Done => None,
    }
}

/// The steps the first start counts: Welcome, Look, How you like to play, Wi-Fi, Time zone, Extras, Add your games (the
/// last page, "You're all set", is the end, not a step).
const FIRST_STEPS: usize = 7;

/// A first-start step's number for "Step n of 7"; `None` for the pages inside a step (keyboards, lists) and after the
/// last one, which would only crowd their titles.
fn step_number(step: &Step) -> Option<usize> {
    match step {
        Step::Welcome => Some(1),
        Step::Look => Some(2),
        Step::Play { .. } => Some(3),
        Step::Wifi => Some(4),
        Step::Zone(z) if matches!(z.view, ZoneView::Card) => Some(5),
        Step::Extras { .. } => Some(6),
        Step::Games => Some(7),
        _ => None,
    }
}

/// The first start's progress, in the data folder.
const RESUME_FILE: &str = "setup-step";

/// The name `remember_step` writes for a step, `None` where nothing is written (the welcome, which has no choice to
/// lose, and the steps that only exist in Settings or after the last page).
fn resume_name(step: &Step) -> Option<&'static str> {
    match step {
        Step::Look => Some("look"),
        Step::Play { .. } | Step::Cards(_) => Some("play"),
        Step::Wifi | Step::Portal(_) | Step::Password { .. } | Step::Connecting { .. } | Step::HiddenName { .. } | Step::Country { .. } => Some("wifi"),
        Step::Zone(_) => Some("zone"),
        Step::Extras { .. } | Step::Ports | Step::PortFiles(_) => Some("extras"),
        Step::Games => Some("games"),
        Step::Welcome | Step::ShelfSystems { .. } | Step::Theme { .. } | Step::Remote | Step::Done => None,
    }
}

/// The offsets from UTC of `zones` now: a file each.
fn zone_offsets(hw: &dyn crate::system_ctl::Hw, zones: Vec<String>) -> Vec<(String, Option<i32>)> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    zones.into_iter().map(|z| (z.clone(), hw.zone_offset(&z, now))).collect()
}

/// The time zone step's first look: the zone list, the system's zone and the Wi-Fi chip's region (`iw reg get`) make
/// the guess, and the offsets of the zones the first screen shows. Processes and files: never on the UI thread.
fn look_zone(hw: &dyn crate::system_ctl::Hw) -> ZoneStep {
    let all = hw.zones();
    let zone_tab = hw.zoneinfo_file("zone.tab");
    // A zone nobody chose (the image's New York) is no answer: the Wi-Fi region guesses, or the user picks.
    let system_zone = hw.timezone().filter(|_| hw.timezone_chosen());
    let guess = timezone::guess(system_zone.as_deref(), hw.regdomain().as_deref(), zone_tab.as_deref(), &all);
    let country = match &guess {
        Guess::Country { code, .. } | Guess::Several { code, .. } => {
            Some(hw.zoneinfo_file("iso3166.tab").and_then(|t| timezone::country_name(&t, code)).unwrap_or_else(|| code.clone()))
        }
        _ => None,
    };
    let mut z = ZoneStep { guess, country, all, view: ZoneView::Card, error: None, loading: false, offsets: HashMap::new(), asked: Default::default() };
    z.view = z.start_view();
    let wanted = z.wanted();
    z.asked.extend(wanted.iter().cloned());
    z.offsets.extend(zone_offsets(hw, wanted));
    z
}

/// The right-hand line of the Extras step's cover row. While it runs the count names what it counts: with pictures on a
/// game has up to three (box art, screenshot, title screen), so "2 of 12" beside "6 games" read as a contradiction
/// (new-user test 2026-10-30).
fn extras_status(covers: Option<(usize, usize, usize)>, running: bool, pictures: bool, games: usize, dots: &str) -> String {
    match (covers, running) {
        (Some((d, t, _)), true) if pictures => format!("Looking up {d} of {t} pictures{dots}"),
        (Some((d, t, _)), true) => format!("Looking up {d} of {t} covers{dots}"),
        (Some((_, _, 1)), false) if pictures => "Found 1 new picture".into(),
        (Some((_, _, f)), false) if f > 0 && pictures => format!("Found {f} new pictures"),
        (Some((_, _, f)), false) if f > 0 => format!("Found {f} new covers"),
        (Some(_), false) if pictures => "No new pictures found".into(),
        (Some(_), false) => "No new covers found".into(),
        (None, _) => format!("{games} games"),
    }
}

/// What the games step's thread found in the ROM folders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counted {
    /// Games per system id (systems without any are left out).
    pub games: HashMap<String, usize>,
    /// Folders in `roms` that are no system's (first-run test 2026-10-24, finding #10: "Sega Genesis" next to
    /// megadrive was ignored without a word).
    pub strays: Vec<library::StrayFolder>,
    /// Disc games left off the shelf because a file their sheet names is missing (finding #11).
    pub broken: Vec<library::BrokenDisc>,
}

/// How many games each system of `catalog` has in its folder, the folders of `roms` no system reads, and the disc
/// games with a missing file. Reads every ROM folder: on a thread only.
fn count_catalog(catalog: &[SysInfo]) -> Counted {
    let mut found = Counted::default();
    for s in catalog {
        let (n, broken) = library::look_at_folder(&s.id, &s.path, &s.extensions);
        if n > 0 {
            found.games.insert(s.id.clone(), n);
        }
        found.broken.extend(broken);
    }
    found.broken.sort_by(|a, b| a.sheet.cmp(&b.sheet));
    found.broken.dedup();
    if let Some(roms) = library::roms_root(catalog) {
        found.strays = library::stray_folders(&roms, catalog);
    }
    found
}

/// The lines Add games shows instead of its subtitle when something in `roms` isn't read: the stray folders (with
/// the folder their games go in, when it can be guessed), then the disc games missing a file. At most two lines.
pub fn unread_lines(strays: &[library::StrayFolder], broken: &[library::BrokenDisc], roms: &str) -> Vec<String> {
    let mut lines = Vec::new();
    match strays {
        [] => {}
        [one] => lines.push(match &one.meant {
            Some(folder) => format!("Folder without a system: {}. Its games go in {roms}/{folder}.", one.name),
            None => format!("Folder without a system: {}. The tiles below show the folder names.", one.name),
        }),
        many => {
            let names: Vec<&str> = many.iter().map(|s| s.name.as_str()).collect();
            let meant: Vec<String> = many.iter().filter_map(|s| s.meant.as_ref()).map(|f| format!("{roms}/{f}")).collect();
            lines.push(if meant.is_empty() {
                format!("Folders without a system: {}. The tiles below show the folder names.", names.join(", "))
            } else {
                format!("Folders without a system: {}. Their games go in {}.", names.join(", "), meant.join(", "))
            });
        }
    }
    let title = |b: &library::BrokenDisc| display_name_of(&b.sheet);
    match broken {
        [] => {}
        [one] => lines.push(format!("Not on the shelf: {}. Its file {} is missing.", title(one), one.missing)),
        many => {
            let names: Vec<String> = many.iter().map(title).collect();
            lines.push(format!("Not on the shelf, a disc file is missing: {}", names.join(", ")));
        }
    }
    lines
}

/// The name a game file would have on the shelf.
fn display_name_of(path: &std::path::Path) -> String {
    library::shelf_name(path.file_stem().and_then(|s| s.to_str()).unwrap_or("?"))
}

/// When the sign-in helper ends with "needs a full browser", the page it gave up on is saved for the next
/// "Report a problem" (`logs::save_portal_page`, `report`): what the hotel served, with query values that look
/// like a MAC or IP address already redacted. A worker thread's last job; a failing save changes nothing.
fn keep_last_page(outcome: &crate::portal::Outcome, session: &crate::portal::Session, data_dir: &std::path::Path) {
    if outcome.shows_full_browser() {
        if let Some(html) = session.last_page() {
            let _ = crate::logs::save_portal_page(data_dir, &html);
        }
    }
}

impl Setup {
    pub fn new(
        notify: Notify,
        themes: Vec<(&'static str, &'static str)>,
        theme: usize,
        data_dir: PathBuf,
        games: Vec<(String, PathBuf)>,
        hw: SharedHw,
    ) -> Self {
        let net = default_net(&hw);
        Self {
            step: Step::Welcome,
            notify,
            seen_notice: net.snapshot().notice.map_or(0, |n| n.id),
            net,
            net_sub: None,
            radio: Radio::Unknown,
            scan_rev: 0,
            switching: None,
            scan_err: None,
            switch_failed: None,
            radio_watch: None,
            radio_every: RADIO_LOOK_EVERY,
            nets: Vec::new(),
            confirm: false,
            missed: HashMap::new(),
            saving: Saving::default(),
            iso_tab: String::new(),
            entries: Vec::new(),
            forget: None,
            connecting_since: None,
            connect_hidden: false,
            sel: 0,
            scanning: false,
            note: None,
            portal: false,
            portal_at: None,
            watch: None,
            recheck: Recheck::default(),
            portal_session: None,
            portal_io: PortalIo::default(),
            sharing: false,
            sharing_busy: false,
            share_looking: false,
            share_password: None,
            remote: remote::Mode::Off,
            remote_busy: false,
            remote_looking: false,
            remote_password: None,
            remote_error: None,
            ip: None,
            started: Instant::now(),
            themes,
            theme,
            appearance: Appearance::Dark,
            motion: crate::motion::level(),
            language_chosen: None,
            text_size_chosen: None,
            animations: None,
            animations_changed: false,
            data_dir,
            games,
            covers: None,
            covers_running: false,
            art: Art::default(),
            art_changed: false,
            ports: Vec::new(),
            fake_updates: None,
            port_tab: PortTab::Ready,
            port_sel: [0, 0],
            catalog_loading: false,
            port_busy: None,
            port_note: None,
            previews: HashMap::new(),
            only: None,
            hw,
            shelf: Vec::new(),
            hidden: Vec::new(),
            hidden_changed: false,
            shelf_note: None,
            streaming: None,
            streaming_changed: false,
            catalog: Vec::new(),
            grid: None,
            counting: false,
            strays: Vec::new(),
            broken: Vec::new(),
            #[cfg(test)]
            inbox: None,
            switches: Switches::default(),
            later: Vec::new(),
            tour_changed: false,
        }
    }

    /// What the profiles and the cards start from: the App's switches as they are, and the cards put off.
    pub fn with_switches(mut self, switches: Switches, later: Vec<String>) -> Self {
        self.motion = switches.motion;
        self.switches = switches;
        self.later = later;
        self
    }

    /// The switches and the cards put off after a profile or a card changed them, once; the app applies and saves
    /// them as their Settings pages do (`App::adopt_switches`).
    pub fn take_tour(&mut self) -> Option<(Switches, Vec<String>)> {
        std::mem::take(&mut self.tour_changed).then(|| (self.switches.clone(), self.later.clone()))
    }

    /// Opens the first start at "How do you like to play?" (for the snapshots).
    pub fn at_play(mut self) -> Self {
        self.step = Step::Play { sel: 0 };
        self
    }

    /// Opens Choose yourself at the card of `id` (for the snapshots).
    pub fn at_card(mut self, id: &str) -> Self {
        let list = features::choose_list();
        let at = list.iter().position(|i| features::FEATURES[*i].id == id).unwrap_or(0);
        let mut cards = Cards::new(features::Mode::First, list, self.switches.clone(), self.later.clone());
        cards.show(at);
        self.step = Step::Cards(Box::new(cards));
        self
    }

    /// Runs just one step, as opened from Settings.
    pub fn only(mut self, step: Only) -> Self {
        self.only = Some(step);
        match step {
            Only::Look => self.step = Step::Look,
            Only::Wifi => {
                self.step = Step::Wifi;
                self.scan();
            }
            Only::Extras => self.step = Step::Extras { sel: 0 },
            Only::Games => self.go_to_games(),
            Only::Remote => {
                self.look_remote();
                self.ip = wifi::ip();
                self.step = Step::Remote;
            }
        }
        self
    }

    /// The first start is past the step where Pictures is chosen (or this is a page opened from Settings): automatic
    /// cover downloads may start. They wait for that step so that the choice, Off included, comes first; the name is
    /// from when they started right after the Wi-Fi step.
    pub fn past_wifi(&self) -> bool {
        self.only.is_some()
            || !matches!(
                self.step,
                Step::Welcome
                    | Step::Look
                    | Step::Play { .. }
                    | Step::Cards(_)
                    | Step::ShelfSystems { .. }
                    | Step::Theme { .. }
                    | Step::Wifi
                    | Step::Portal(_)
                    | Step::Password { .. }
                    | Step::Connecting { .. }
                    | Step::HiddenName { .. }
                    | Step::Country { .. }
                    | Step::Zone(_)
                    | Step::Extras { .. }
            )
    }

    /// The appearance in use when the setup opens.
    /// The Wi-Fi page with a made-up neighbourhood and saved networks, for `--snapshot setup:wifi`: no scan runs.
    pub fn with_wifi_demo(self) -> Self {
        let nets = wifi::parse_scan("Home:78:WPA2:*\nCafe guest:41::\nNeighbour\\:5G:30:WPA2: \n");
        self.with_net_demo(Snapshot { wanted: Some(true), radio: Radio::On, scan: Scan::Found(nets), scan_rev: 1, ..Snapshot::default() })
    }

    /// A connection failure through the ordinary result handler, for snapshots and render journeys.
    pub fn with_wifi_error_demo(self) -> Self {
        let mut page = self.with_wifi_demo();
        page.take_event(SetupEvent::ConnectedSaved {
            ssid: "Xiaomi 15".into(),
            result: Err("Error: Timeout expired (45 seconds)".into()),
        });
        page
    }

    /// The Wi-Fi page while NetworkManager is still bringing Home up (`--snapshot setup:wifi-connecting`), or has been
    /// for longer than `CONNECT_STUCK` without an address (`setup:wifi-stuck`).
    pub fn with_connecting_demo(mut self, stuck: bool) -> Self {
        self = self.with_wifi_demo();
        let mut snap = self.net.snapshot();
        snap.saving.ip = None;
        for saved in &mut snap.saving.saved {
            (saved.active, saved.connecting) = (false, saved.ssid == "Home");
        }
        self.net = NetState::demo(snap.clone());
        self.apply_snapshot(&snap, false);
        if stuck {
            if let Some((_, since)) = self.connecting_since.as_mut() {
                *since = Instant::now().checked_sub(CONNECT_STUCK).unwrap_or(*since);
            }
        }
        self
    }

    /// A typed password the network refused, through the ordinary result handler: the keyboard again with the reason
    /// (`--snapshot setup:wifi-refused`).
    pub fn with_refused_demo(self) -> Self {
        let mut page = self.with_wifi_demo();
        page.take_event(SetupEvent::Connected {
            ssid: "Neighbour:5G".into(),
            result: Err("Error: Connection activation failed: Secrets were required, but not provided.".into()),
        });
        page
    }

    /// The Wi-Fi page on a made-up picture of the Wi-Fi (`net_state::demo`: the radio off, a switch that failed, a scan
    /// that failed or found nobody), with the saved networks of `with_wifi_demo`; `--snapshot setup:wifi-off` and
    /// the like. The picture stays as it is: nothing is looked at.
    pub fn with_net_demo(mut self, mut snap: Snapshot) -> Self {
        snap.saving = Saving { saved: wifi::demo_saved(), country: None, reg: Some("AT".into()), ip: Some("192.0.2.23".into()), mac: Some("00:00:5E:00:53:7F".into()) };
        // Nothing seen, nothing connected (a failed scan or an empty one in the pictures: the saved networks are out of reach).
        if snap.radio.is_off() || !matches!(&snap.scan, Scan::Found(n) if !n.is_empty()) {
            snap.saving.ip = None;
            snap.saving.saved.iter_mut().for_each(|s| s.active = false);
        }
        snap.iso_tab = self.hw.zoneinfo_file("iso3166.tab").unwrap_or_default();
        self.net = NetState::demo(snap.clone());
        self.net_sub = None;
        self.scanning = false;
        self.apply_snapshot(&snap, false);
        self
    }

    /// `with_net_demo` for the switch on its way (`--snapshot setup:wifi-switching`).
    pub fn with_switching_demo(self) -> Self {
        self.with_net_demo(crate::net_state::demo::switching_on())
    }

    /// The page on the shared picture of the Wi-Fi of the quick settings (`Sys::net`): both show one state.
    pub fn with_net(mut self, net: Arc<NetState>) -> Self {
        self.seen_notice = net.snapshot().notice.map_or(0, |n| n.id);
        self.net = net;
        self.net_sub = None;
        self
    }

    /// `with_wifi_demo` on a network with a sign-in page, for `--snapshot setup:wifi-portal`.
    pub fn with_portal_demo(mut self) -> Self {
        self.portal = true;
        self.portal_at = Some("http://login.hotel-seeblick.example/start".into());
        self
    }

    /// `with_portal_demo` with the sign-in sheet open (`--snapshot setup:wifi-portal-sheet`); `mac`: the Wi-Fi address
    /// as read, None as if it couldn't be.
    pub fn with_portal_sheet(mut self, mac: bool) -> Self {
        self = self.with_portal_demo();
        if !mac {
            self.saving.mac = None;
        }
        self.open_portal();
        self
    }

    /// The sign-in sheet's "Sign in…" with a made-up answer (`portal_form::demo`), for `--snapshot
    /// setup:wifi-portal-signin=<what>`; nothing is loaded.
    pub fn with_portal_signin(mut self, what: &str) -> Self {
        self = self.with_portal_sheet(true);
        if let Step::Portal(page) = &mut self.step {
            page.demo_signin(what);
        }
        self
    }

    pub fn with_appearance(mut self, a: Appearance) -> Self {
        self.appearance = a;
        self
    }

    /// The appearance chosen on the look step (the app shows it at once).
    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    /// The motion chosen on the look step (the app uses it at once).
    pub fn motion(&self) -> Motion {
        self.motion
    }

    /// The theme to preview while the look step is open; the app draws the shelf in it.
    pub fn previewing(&self) -> Option<usize> {
        matches!(self.step, Step::Look | Step::Theme { .. }).then_some(self.theme)
    }

    /// The network being brought up has been "Connecting…" for longer than `CONNECT_STUCK`.
    fn stuck(&self) -> bool {
        self.connecting_since.as_ref().is_some_and(|(_, since)| since.elapsed() >= CONNECT_STUCK)
    }

    /// True while something on screen moves (scan or connect in progress).
    pub fn animating(&self) -> bool {
        self.scanning
            || self.switching.is_some()
            || self.sharing_busy
            || self.remote_busy
            || self.share_looking
            || self.remote_looking
            || self.covers_running
            || self.catalog_loading
            || matches!(&self.step, Step::Zone(z) if z.loading)
            || self.port_busy.is_some()
            || matches!(self.step, Step::Connecting { .. })
            || (matches!(self.step, Step::Wifi) && self.connecting_since.is_some())
            || matches!(&self.step, Step::Portal(p) if p.busy())
    }

    /// Asks the shared picture of the Wi-Fi to look again (`net_state`: the radio, the saved networks first, then the
    /// scan); the answers come as `SetupEvent::Net`. One scan at a time: a press while one runs does nothing.
    fn scan(&mut self) {
        if self.scanning || self.radio == Radio::Absent {
            return;
        }
        self.scanning = true;
        self.watch_net();
        self.net.request_scan();
    }

    /// Takes part in the shared picture: its changes come back as `SetupEvent::Net` (held until the page is gone).
    fn watch_net(&mut self) {
        let notify = self.notify.clone();
        let sub: Arc<dyn Fn() + Send + Sync> = Arc::new(move || notify(SetupEvent::Net));
        self.net.subscribe(&sub);
        self.net_sub = Some(sub);
    }

    /// The Wi-Fi page's "Turn on": the shared picture switches the radio and stores the wish once it really is on.
    fn turn_wifi_on(&mut self) {
        if self.switching.is_some() {
            return;
        }
        self.switching = Some(true);
        self.switch_failed = None;
        self.note = None;
        self.watch_net();
        self.net.set_enabled(true);
    }

    fn connect(&mut self, ssid: String, psk: Option<String>) {
        self.connect_to(ssid, psk, false);
    }

    fn connect_to(&mut self, ssid: String, psk: Option<String>, hidden: bool) {
        let notify = self.notify.clone();
        let target = ssid.clone();
        self.connect_hidden = hidden;
        // After everything asked before it and before anything asked after: not at the same time as a switch of the radio.
        self.watch_net();
        self.net.exclusive("connect", move || {
            let result = wifi::connect_with(&target, psk.as_deref(), hidden);
            notify(SetupEvent::Connected { ssid: target, result });
        });
        self.step = Step::Connecting { ssid };
    }

    fn connect_saved(&mut self, saved: Saved) {
        let notify = self.notify.clone();
        let ssid = saved.ssid.clone();
        self.watch_net();
        self.net.exclusive("connect saved", move || {
            let result = wifi::connect_saved(&saved);
            notify(SetupEvent::ConnectedSaved { ssid: saved.ssid, result });
        });
        self.step = Step::Connecting { ssid };
    }

    fn forget_saved(&mut self, saved: Saved) {
        let notify = self.notify.clone();
        let net = self.net.clone();
        self.watch_net();
        self.net.exclusive("forget", move || {
            let result = wifi::forget(&saved);
            if result.is_ok() {
                net.forgotten(&saved.uuid);
            }
            notify(SetupEvent::Forgot { ssid: saved.ssid, result });
        });
    }

    /// The Wi-Fi page's rows: "This network wants a sign-in" while it does (`lead`), the networks (`entries`), then
    /// Other network, then Wi-Fi country.
    fn wifi_rows(&self) -> usize {
        if self.radio == Radio::Absent {
            return 0;
        }
        self.lead() + self.entries.len() + 2
    }

    /// Joined, but nothing online works before the sign-in: the page says so on a row of its own, first.
    fn portal_shown(&self) -> bool {
        self.portal && self.entries.iter().any(|e| e.active)
    }

    /// The row before the networks, if there is one: "Wi-Fi is off" with its way to turn it on, or "This network wants a
    /// sign-in". Never both: a radio that is off has no network to sign in to.
    fn lead_kind(&self) -> Option<Lead> {
        if self.radio.is_off() {
            Some(Lead::Off)
        } else if self.portal_shown() {
            Some(Lead::Portal)
        } else {
            None
        }
    }

    /// Rows before the networks: 1 with the "Wi-Fi is off" or the sign-in row.
    fn lead(&self) -> usize {
        usize::from(self.lead_kind().is_some())
    }

    /// The sign-in row came or went (`lead` was `before`): the selection stays on its row. It moves onto the new row
    /// from the top, so A there opens it.
    fn relead(&mut self, before: usize) {
        match self.lead().cmp(&before) {
            std::cmp::Ordering::Greater if self.sel > 0 => self.sel += 1,
            std::cmp::Ordering::Less => self.sel = self.sel.saturating_sub(1),
            _ => {}
        }
        self.sel = self.sel.min(self.wifi_rows().saturating_sub(1));
    }

    /// The sign-in sheet over the Wi-Fi page.
    fn open_portal(&mut self) {
        let ssid = self.entries.iter().find(|e| e.active).map(|e| e.ssid.clone()).unwrap_or_default();
        self.step = Step::Portal(Box::new(crate::portal_page::PortalPage::new(&ssid, self.portal_at.clone(), self.saving.mac.clone())));
    }

    /// "Sign in…": a new session loads the sign-in page from `start` on a thread.
    fn portal_load(&mut self, ticket: u64, start: String) {
        let io = self.portal_io.clone();
        let session = match crate::portal::Session::new(&io.curl, &io.check, io.rules) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                let outcome = crate::portal::Outcome::Failed(crate::portal::Failure::Other(e.to_string()));
                self.on_event(SetupEvent::Portal { ticket, outcome, insecure: false });
                return;
            }
        };
        self.portal_session = Some(session.clone());
        let notify = self.notify.clone();
        let data_dir = self.data_dir.clone();
        std::thread::spawn(move || {
            let outcome = session.open(&start);
            keep_last_page(&outcome, &session, &data_dir);
            let insecure = session.insecure.load(std::sync::atomic::Ordering::Relaxed);
            notify(SetupEvent::Portal { ticket, outcome, insecure });
        });
    }

    /// A form the person sent with its button: on a thread, in the page's session; the copy is wiped once sent.
    fn portal_send(&mut self, ticket: u64, mut form: crate::html_form::Form, submitter: Option<usize>, url: String) {
        let Some(session) = self.portal_session.clone() else {
            form.wipe();
            let outcome = crate::portal::Outcome::Failed(crate::portal::Failure::Other("no session".into()));
            self.on_event(SetupEvent::Portal { ticket, outcome, insecure: false });
            return;
        };
        let notify = self.notify.clone();
        let data_dir = self.data_dir.clone();
        std::thread::spawn(move || {
            let outcome = session.submit(&url, &form, submitter);
            form.wipe();
            if matches!(outcome, crate::portal::Outcome::Online) {
                // Updates, the store and PortMaster ask again instead of remembering the sign-in.
                net_check::forget();
            }
            keep_last_page(&outcome, &session, &data_dir);
            let insecure = session.insecure.load(std::sync::atomic::Ordering::Relaxed);
            notify(SetupEvent::Portal { ticket, outcome, insecure });
        });
    }

    /// Starts the re-check while the Wi-Fi page or its sign-in sheet shows a sign-in, and ends it otherwise. One small
    /// request every 15 s on a thread of its own (`net_check::recheck`); it stops with an answer that isn't a sign-in,
    /// and when the page closes (the setup, and with it `watch`, is gone). It never sends anything else.
    fn keep_watch(&mut self) {
        self.keep_radio_watch();
        let on_page = matches!(self.step, Step::Wifi) || matches!(&self.step, Step::Portal(p) if !p.online);
        if !(on_page && self.portal_shown()) {
            self.watch = None;
            return;
        }
        if self.watch.is_some() {
            return;
        }
        let token = Arc::new(());
        let alive = Arc::downgrade(&token);
        self.watch = Some(token);
        let (notify, recheck) = (self.notify.clone(), self.recheck);
        std::thread::spawn(move || loop {
            let until = Instant::now() + recheck.every;
            while Instant::now() < until {
                if alive.strong_count() == 0 {
                    return;
                }
                std::thread::sleep(recheck.every.min(std::time::Duration::from_millis(200)));
            }
            if alive.strong_count() == 0 {
                return;
            }
            let check = (recheck.ask)();
            let again = matches!(check.net, net_check::Net::Portal | net_check::Net::Unknown);
            notify(SetupEvent::Network(check));
            if !again {
                return;
            }
        });
    }

    /// While the Wi-Fi page is up, a thread looks at the radio every `radio_every` (`NetState::poll`): the boot hooks and
    /// the quick settings switch it too, and the page should not wait for a press of X to notice. It ends with the page
    /// (it holds a `Weak` of the token); the answers come as `SetupEvent::Net` only when something changed.
    fn keep_radio_watch(&mut self) {
        if !matches!(self.step, Step::Wifi) {
            self.radio_watch = None;
            return;
        }
        if self.radio_watch.is_some() || self.net_sub.is_none() {
            return;
        }
        let token = Arc::new(());
        let alive = Arc::downgrade(&token);
        self.radio_watch = Some(token);
        let (net, every) = (self.net.clone(), self.radio_every);
        let started = std::thread::Builder::new().name("wifi-page-radio".into()).spawn(move || loop {
            let until = Instant::now() + every;
            while Instant::now() < until {
                if alive.strong_count() == 0 {
                    return;
                }
                std::thread::sleep(every.min(std::time::Duration::from_millis(200)));
            }
            if alive.strong_count() == 0 {
                return;
            }
            net.poll();
        });
        if started.is_err() {
            self.radio_watch = None;
        }
    }

    /// The list of the Wi-Fi country step: Automatic, then every country of zoneinfo by name.
    fn open_country(&mut self) {
        if self.iso_tab.is_empty() {
            // Before the first scan came back: the one file read, on this press only.
            self.iso_tab = self.hw.zoneinfo_file("iso3166.tab").unwrap_or_default();
        }
        let mut list = vec![(String::new(), "Automatic".to_string())];
        list.extend(wifi::countries(&self.iso_tab));
        let sel = self.saving.country.as_ref().and_then(|c| list.iter().position(|(code, _)| code == c)).unwrap_or(0);
        self.step = Step::Country { sel, list };
    }

    /// "Austria" for "AT", from zoneinfo's list (kept in memory); the code when it isn't there.
    fn country_name(&self, code: &str) -> String {
        timezone::country_name(&self.iso_tab, code).unwrap_or_else(|| code.to_string())
    }

    /// What the Wi-Fi country row says: the setting, or Automatic with the country the chip uses.
    fn country_value(&self) -> String {
        match (&self.saving.country, &self.saving.reg) {
            (Some(c), _) => self.country_name(c),
            (None, Some(r)) => format!("Automatic ({r})"),
            (None, None) => "Automatic".into(),
        }
    }

    /// Takes what the shared picture of the Wi-Fi says: the radio, the saved networks, the address, the scan, a switch
    /// that did not work. `probe`: a new list from a network in use asks (on a thread) whether it wants a sign-in.
    fn apply_snapshot(&mut self, snap: &Snapshot, probe: bool) {
        let before = self.lead();
        self.radio = snap.radio;
        self.switching = snap.switching;
        self.scanning = snap.scanning;
        self.saving = snap.saving.clone();
        if !snap.iso_tab.is_empty() {
            self.iso_tab = snap.iso_tab.clone();
        }
        // The address of the network now, not the last one there was (the page used to keep it when none came).
        self.ip = snap.saving.ip.clone();
        if self.ip.is_none() {
            self.portal = false;
        }
        if snap.scan_rev != self.scan_rev {
            self.scan_rev = snap.scan_rev;
            self.scan_err = None;
            match &snap.scan {
                Scan::Found(nets) => self.nets = keep_seen(&self.nets, nets, &mut self.missed),
                // A list from before the failure would offer networks nobody has seen just now.
                Scan::Failed(e) => {
                    self.nets.clear();
                    self.scan_err = Some(e.clone());
                }
                Scan::NotYet => self.nets.clear(),
            }
            if !matches!(snap.scan, Scan::Found(_)) {
                self.missed.clear();
            }
            // A new list answers the old "Connected to …" and "Already connected …"; a failure stays until it is dealt with.
            if matches!(self.note, Some((_, false))) && self.forget.is_none() {
                self.note = None;
            }
            if probe && matches!(snap.scan, Scan::Found(_)) && self.ip.is_some() {
                // Connected: does the network want a sign-in in a browser? (kept for 20 s, one request)
                let notify = self.notify.clone();
                std::thread::spawn(move || notify(SetupEvent::Network(net_check::probe_check())));
            }
        }
        if snap.radio.is_off() && matches!(self.note, Some((_, false))) && self.forget.is_none() {
            self.note = None;
        }
        // A switch that did not work: said once, until the radio is as asked or it is tried again.
        if let Some(n) = snap.notice.as_ref().filter(|n| n.id > self.seen_notice) {
            self.seen_notice = n.id;
            self.switch_failed = Some(n.clone());
        }
        // Gone with the next try, once the radio is as asked, or once a wish that could not be stored has been.
        let reached = |n: &Notice| n.kind == NoticeKind::Switch && ((n.on && snap.radio.is_on()) || (!n.on && snap.radio.is_off()));
        if snap.notice.is_none() || self.switch_failed.as_ref().is_some_and(reached) {
            self.switch_failed = None;
        }
        self.entries = if self.radio.is_off() { Vec::new() } else { wifi::merge(&self.nets, &self.saving.saved) };
        let connecting = self.entries.iter().find(|e| connecting(e)).map(|e| e.ssid.clone());
        self.connecting_since = match (connecting, self.connecting_since.take()) {
            (Some(ssid), Some((was, since))) if was == ssid => Some((ssid, since)),
            (Some(ssid), _) => Some((ssid, Instant::now())),
            (None, _) => None,
        };
        self.relead(before);
    }

    pub fn on_event(&mut self, ev: SetupEvent) {
        self.take_event(ev);
        self.keep_watch();
    }

    fn take_event(&mut self, ev: SetupEvent) {
        match ev {
            SetupEvent::Net => {
                let snap = self.net.snapshot();
                self.apply_snapshot(&snap, true);
            }
            SetupEvent::Network(check) => {
                let (before, was) = (self.lead(), self.portal);
                match check.net {
                    net_check::Net::Portal => {
                        self.portal = true;
                        if check.portal.is_some() {
                            self.portal_at = check.portal.clone();
                        }
                    }
                    // An answer that says nothing (curl missing or odd) leaves the page as it is.
                    net_check::Net::Unknown => {}
                    net => {
                        self.portal = false;
                        if was && net == net_check::Net::Online {
                            self.note = Some(("You're online now. The network lets this Pocket in.".into(), false));
                        }
                    }
                }
                if let Step::Portal(page) = &mut self.step {
                    page.on_check(&check);
                }
                self.relead(before);
            }
            SetupEvent::Portal { ticket, outcome, insecure } => {
                let online = matches!(outcome, crate::portal::Outcome::Online);
                if let Step::Portal(page) = &mut self.step {
                    page.on_outcome(ticket, outcome, insecure);
                }
                if online {
                    let check = net_check::Check { net: net_check::Net::Online, portal: None };
                    self.take_event(SetupEvent::Network(check));
                }
            }
            SetupEvent::ConnectedSaved { ssid, result } => {
                match result {
                    Ok(()) => {
                        net_check::forget();
                        self.portal = false;
                        std::thread::spawn(wifi::rebind_sharing);
                        self.note = Some((format!("Connected to {ssid}"), false));
                        self.step = Step::Wifi;
                    }
                    // The saved password is no longer right (or the profile lost it): ask for it.
                    Err(e) if wifi::wrong_password(&e) => {
                        eprintln!("wifi: connection failed: {e}");
                        self.note = Some(("The saved password didn't work. Type it again.".into(), true));
                        self.step = Step::Password { ssid, kb: Keyboard::default(), hidden: false };
                        return;
                    }
                    Err(e) => {
                        self.note = Some((friendly_error(&e), true));
                        self.step = Step::Wifi;
                    }
                }
                // The scan that follows is `NetState::exclusive`'s own: asking again would run it twice.
            }
            SetupEvent::Forgot { ssid, result } => {
                self.note = Some(match result {
                    Ok(()) => (format!("Forgot {ssid}"), false),
                    Err(e) => {
                        eprintln!("wifi: couldn't forget {ssid}: {e}");
                        ("Couldn't forget that network. Try again.".into(), true)
                    },
                });
                // The scan that follows is `NetState::exclusive`'s own: asking again would run it twice.
            }
            SetupEvent::CountrySet(result) => {
                match result {
                    Ok(code) => {
                        self.note = Some((format!("Wi-Fi country: {}", code.as_deref().map_or("Automatic".to_string(), |c| self.country_name(c))), false));
                        self.saving.country = code;
                    }
                    Err(e) => {
                        eprintln!("wifi: couldn't set the country: {e}");
                        self.note = Some(("Couldn't set the Wi-Fi country. Try again.".into(), true));
                    },
                }
                if matches!(self.step, Step::Country { .. }) {
                    self.step = Step::Wifi;
                }
                // The scan that follows is `NetState::exclusive`'s own: asking again would run it twice.
            }
            SetupEvent::Connected { ssid, result } => {
                if result.is_ok() {
                    net_check::forget();
                    self.portal = false;
                    // A new network usually means a new address; smbd has to follow it.
                    std::thread::spawn(wifi::rebind_sharing);
                }
                let refused = result.as_ref().is_err_and(|e| wifi::wrong_password(e));
                self.note = Some(match result {
                    Ok(()) => (format!("Connected to {ssid}"), false),
                    Err(e) => (friendly_error(&e), true),
                });
                // A refused password: the keyboard again, empty, with the reason above it (`wifi::connect_with` has
                // removed the refused profile, so what is typed now is what is tried).
                self.step = if refused { Step::Password { ssid, kb: Keyboard::default(), hidden: self.connect_hidden } } else { Step::Wifi };
                // The scan that follows is `NetState::exclusive`'s own: asking again would run it twice.
            }
            SetupEvent::Sharing { on, password, switched } => {
                if switched {
                    self.sharing_busy = false;
                    if on {
                        // The page's drop folder exists by now; sharing took it over when it went on.
                        std::thread::spawn(wifi::rebind_sharing);
                    }
                } else {
                    self.share_looking = false;
                }
                self.sharing = on;
                self.share_password = password;
            }
            SetupEvent::Remote { mode, password, result } => {
                match result {
                    Some(result) => {
                        self.remote_busy = false;
                        self.remote_error = result.err();
                    }
                    None => self.remote_looking = false,
                }
                self.remote = mode;
                self.remote_password = password;
            }
            SetupEvent::Zone(facts) => {
                // Only into the step that is still waiting for it (B or START may have left it already).
                if matches!(&self.step, Step::Zone(z) if z.loading) {
                    self.step = Step::Zone(Box::new(facts.0));
                }
            }
            SetupEvent::ZoneOffsets(found) => {
                if let Step::Zone(z) = &mut self.step {
                    z.offsets.extend(found);
                }
            }
            SetupEvent::GameCounts(found) => {
                self.counting = false;
                self.strays = found.strays;
                self.broken = found.broken;
                if let Some(grid) = &mut self.grid {
                    // The tiles may change places (a system got its first game): the selection stays on its tile.
                    let was = grid.selected();
                    grid.counts = found.games;
                    let tiles = grid.tiles();
                    grid.sel = tiles.iter().position(|t| Some(*t) == was).unwrap_or(grid.sel.min(tiles.len().saturating_sub(1)));
                }
            }
            SetupEvent::Covers { done, total, found } => {
                self.covers = Some((done, total, found));
                self.covers_running = done < total;
            }
            SetupEvent::Catalog(result) => {
                self.catalog_loading = false;
                match result {
                    Ok(ports) => {
                        // The shelf's cards compare their ports with this catalogue (`ports::update_waiting`).
                        ports::note_versions(&ports);
                        self.ports = ports;
                    }
                    Err(e) => self.port_note = Some((e, true)),
                }
            }
            SetupEvent::PortDone { name, title, result, installing, updated } => {
                self.port_busy = None;
                let needs_files = self.ports.iter().find(|p| p.name == name).filter(|p| p.needs_files).cloned();
                self.port_note = Some(match &result {
                    Ok(()) if updated => (format!("{title} is up to date."), false),
                    Ok(()) if installing && needs_files.is_some() => (format!("{title} is installed. It needs your game files before it can start."), false),
                    Ok(()) if installing => (format!("{title} is installed. Find it under Ports."), false),
                    Ok(()) => (format!("{title} was removed."), false),
                    Err(e) => (format!("{title}: {e}"), true),
                });
                // Straight on to what to copy where, if the player is still in the browser.
                if let (Ok(()), true, false, Some(port), Step::Ports) = (&result, installing, updated, needs_files, &self.step) {
                    self.open_port_files(&port);
                }
            }
        }
    }

    fn start_covers(&mut self) {
        if self.covers_running {
            return;
        }
        self.covers_running = true;
        self.covers = Some((0, 0, 0));
        // The automatic download is already on it: the app passes its progress on to this page.
        let Some(claim) = covers::claim() else { return };
        let (notify, data_dir, games, pictures) = (self.notify.clone(), self.data_dir.clone(), self.games.clone(), self.art == Art::Pictures);
        std::thread::spawn(move || {
            let _claim = claim;
            let report = |done, total, found| notify(SetupEvent::Covers { done, total, found });
            // Box art, then (unless Box art only) the games' screenshots and title screens.
            let wanted = pictures.then(|| game_pictures::missing(&data_dir, &games, &Default::default()));
            let run = game_pictures::run_until(&game_pictures::Libretro, &data_dir, &games, wanted.as_deref(), report, || false);
            // Nothing to do still has to end the run.
            if games.is_empty() || run.found == 0 {
                report(1, 1, run.found);
            }
        });
    }

    fn load_catalog(&mut self) {
        if self.catalog_loading || !self.ports.is_empty() {
            return;
        }
        self.catalog_loading = true;
        let (notify, data_dir) = (self.notify.clone(), self.data_dir.clone());
        std::thread::spawn(move || notify(SetupEvent::Catalog(ports::catalog(&data_dir))));
    }

    /// The ports of the list on show, as indices into `ports`.
    fn tab_ports(&self) -> Vec<usize> {
        let own = self.port_tab == PortTab::OwnFiles;
        (0..self.ports.len()).filter(|&i| self.ports[i].needs_files == own).collect()
    }

    fn tab_count(&self, tab: PortTab) -> usize {
        self.ports.iter().filter(|p| p.needs_files == (tab == PortTab::OwnFiles)).count()
    }

    /// The selected port of the list on show.
    fn selected_port(&self) -> Option<&Port> {
        let list = self.tab_ports();
        list.get(self.port_sel[self.port_tab.index()]).map(|&i| &self.ports[i])
    }

    /// A on the selected port: install it, or update it when the catalogue has a newer version. With `remove`
    /// (Y on a port that has an update), take it away instead.
    fn toggle_port(&mut self, remove: bool) {
        let Some(port) = self.selected_port().cloned() else { return };
        if self.port_busy.is_some() {
            return;
        }
        let updating = !remove && self.update_for(&port);
        let installing = updating || !ports::installed(&port);
        self.port_busy = Some(port.title.clone());
        self.port_note = None;
        let notify = self.notify.clone();
        std::thread::spawn(move || {
            let result = if installing { ports::install(&port) } else { ports::uninstall(&port) };
            if !installing && result.is_ok() {
                // Its drop folder on the share, if the player never used it.
                let _ = std::fs::remove_dir(std::path::Path::new(port_files::DROP_DIR).join(&port.dir));
            }
            notify(SetupEvent::PortDone { name: port.name, title: port.title, result, installing, updated: updating });
        });
    }

    /// "Copy your game files" for `port`; its drop folder on the share is made once it is installed.
    fn open_port_files(&mut self, port: &Port) {
        let page = FilesPage::new(&port.title, &port.dir, &port.instructions, std::path::Path::new(ports::PORTS_DIR));
        if page.drop.is_some() && ports::installed(port) {
            port_files::make_drop(std::path::Path::new(port_files::DROP_DIR), &port.dir);
        }
        self.look_sharing();
        self.ip = wifi::ip();
        self.step = Step::PortFiles(Box::new(page));
    }

    /// File sharing as the "Copy your game files" page shows it.
    fn files_share(&self) -> Share {
        Share { on: self.sharing, ip: self.ip.clone(), password: self.share_password.clone() }
    }

    /// X on the games step and on "Copy your game files": file sharing on or off, on a thread.
    fn switch_sharing(&mut self) {
        if self.sharing_busy || self.share_looking || !(self.sharing || self.ip.is_some()) {
            return;
        }
        self.sharing_busy = true;
        let (notify, on) = (self.notify.clone(), !self.sharing);
        std::thread::spawn(move || {
            if on {
                wifi::enable_sharing();
            } else {
                wifi::disable_sharing();
            }
            notify(SetupEvent::Sharing { on: wifi::sharing_enabled(), password: wifi::share_password(), switched: true })
        });
    }

    /// File sharing and its password, looked at on a thread (`systemctl is-active smbd` and a file): a page that shows
    /// them opens at once and says "Looking at file sharing…" until the answer (`SetupEvent::Sharing`). A share that
    /// is on is rebound to the current address there too.
    fn look_sharing(&mut self) {
        self.share_looking = true;
        let notify = self.notify.clone();
        std::thread::spawn(move || {
            let on = wifi::sharing_enabled();
            if on {
                wifi::rebind_sharing();
            }
            notify(SetupEvent::Sharing { on, password: wifi::share_password(), switched: false })
        });
    }

    /// `--snapshot`: what `look_sharing` and `look_remote` would find, read here at once (never so in the running app).
    pub fn looked_now(&mut self) {
        if std::mem::take(&mut self.share_looking) {
            self.sharing = wifi::sharing_enabled();
            self.share_password = wifi::share_password();
        }
        if std::mem::take(&mut self.remote_looking) {
            self.remote = remote::current();
            self.remote_password = remote::password();
        }
        if matches!(&self.step, Step::Zone(z) if z.loading) {
            self.step = Step::Zone(Box::new(look_zone(&*self.hw)));
        }
        self.zone_offsets_now();
        if std::mem::take(&mut self.counting) {
            let counts = count_catalog(&self.catalog);
            self.on_event(SetupEvent::GameCounts(counts));
        }
    }

    /// `looked_now` for the zone offsets a screen asked for.
    fn zone_offsets_now(&mut self) {
        let hw = self.hw.clone();
        if let Step::Zone(z) = &mut self.step {
            let missing: Vec<String> = z.asked.iter().filter(|a| !z.offsets.contains_key(*a)).cloned().collect();
            z.offsets.extend(zone_offsets(&*hw, missing));
        }
    }

    /// Remote access and its password, the same way (`remote::current` runs `systemctl`).
    fn look_remote(&mut self) {
        self.remote_looking = true;
        let notify = self.notify.clone();
        std::thread::spawn(move || notify(SetupEvent::Remote { mode: remote::current(), password: remote::password(), result: None }));
    }

    /// The PortMaster browser with this catalog, without loading it (`--snapshot`).
    /// For snapshots and tests: these catalogue names have an update, whatever is installed.
    pub fn with_updates(mut self, names: &[&str]) -> Self {
        self.fake_updates = Some(names.iter().map(|n| n.to_string()).collect());
        self
    }

    /// The catalogue has a newer version of `port` than the one installed.
    fn update_for(&self, port: &Port) -> bool {
        match &self.fake_updates {
            Some(names) => names.contains(&port.name),
            None => ports::update_available(port),
        }
    }

    pub fn show_ports(mut self, ports: Vec<Port>) -> Self {
        self.ports = ports;
        self.step = Step::Ports;
        self
    }

    /// Handles one button press; returns true when setup is finished.
    /// Changes whenever what the setup shows moves (step, selection, typed text, a switch): for the UI sounds.
    pub fn ui_mark(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        match &self.step {
            Step::Welcome => (0u8, crate::i18n::current()).hash(&mut h),
            Step::Look => (1u8, self.appearance, self.motion).hash(&mut h),
            Step::Play { sel } => (15u8, sel).hash(&mut h),
            Step::Cards(c) => (16u8, c.mark()).hash(&mut h),
            Step::Wifi => (2u8, &self.forget).hash(&mut h),
            Step::Password { ssid, kb, .. } => (3u8, ssid, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            Step::HiddenName { kb } => (13u8, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            Step::Country { sel, .. } => (14u8, sel).hash(&mut h),
            Step::Connecting { ssid } => (4u8, ssid).hash(&mut h),
            Step::Portal(p) => (17u8, p.mark()).hash(&mut h),
            Step::Extras { sel } => (5u8, sel).hash(&mut h),
            Step::Ports => (6u8, self.port_tab).hash(&mut h),
            Step::PortFiles(page) => (12u8, page.scroll()).hash(&mut h),
            Step::Games => 7u8.hash(&mut h),
            Step::Remote => 8u8.hash(&mut h),
            Step::Done => 9u8.hash(&mut h),
            Step::ShelfSystems { sel } => (11u8, sel, &self.hidden, self.streaming).hash(&mut h),
            Step::Theme { sel } => (18u8, sel, crate::theme_page::mark()).hash(&mut h),
            Step::Zone(z) => match &z.view {
                ZoneView::Card => (10u8, 0u8).hash(&mut h),
                ZoneView::Regions { sel } => (10u8, 1u8, sel).hash(&mut h),
                ZoneView::List { heading, sel, .. } => (10u8, 2u8, heading, sel).hash(&mut h),
                ZoneView::Search(kb) => (10u8, 3u8, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            },
        }
        (self.sel, self.theme, self.port_sel, self.port_tab, self.sharing, self.remote_busy, self.sharing_busy).hash(&mut h);
        h.finish()
    }

    /// Where the setup is, as a short word; for tests.
    #[cfg(test)]
    fn at(&self) -> String {
        match &self.step {
            Step::Welcome => "welcome",
            Step::Look => "look",
            Step::Play { .. } => "how you like to play",
            Step::Cards(_) => "feature cards",
            Step::Wifi => "wifi",
            Step::Password { .. } => "password",
            Step::Connecting { .. } => "connecting",
            Step::Portal(p) => match p.view {
                crate::portal_page::View::Overview => "wifi: sign-in",
                crate::portal_page::View::OtherWays => "wifi: other ways",
                crate::portal_page::View::SignIn => "wifi: sign-in form",
            },
            Step::HiddenName { .. } => "wifi: hidden",
            Step::Country { .. } => "wifi: country",
            Step::Extras { .. } => "extras",
            Step::Ports => match self.port_tab {
                PortTab::Ready => "ports",
                PortTab::OwnFiles => "ports: own files",
            },
            Step::PortFiles(_) => "port files",
            Step::Games => "games",
            Step::Remote => "remote",
            Step::Done => "done",
            Step::ShelfSystems { .. } => "shelf systems",
            Step::Theme { .. } => "theme",
            Step::Zone(z) => {
                return match &z.view {
                    ZoneView::Card => "zone".into(),
                    ZoneView::Regions { .. } => "zone: regions".into(),
                    ZoneView::List { heading, .. } => format!("zone: {heading}"),
                    ZoneView::Search(_) => "zone: search".into(),
                }
            }
        }
        .into()
    }

    /// Whether a thread this setup started for the time zone step or the games step has not answered yet.
    #[cfg(test)]
    fn waiting(&self) -> bool {
        self.counting || matches!(&self.step, Step::Zone(z) if z.loading || z.asked.iter().any(|a| !z.offsets.contains_key(a)))
    }

    /// Takes the answers of those threads, waiting for them (the tests run steps one after the other).
    #[cfg(test)]
    fn settle(&mut self) {
        let Some(rx) = self.inbox.take() else { return };
        let until = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            while let Ok(e) = rx.try_recv() {
                self.on_event(e);
            }
            if !self.waiting() || Instant::now() > until {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.inbox = Some(rx);
    }

    /// Types into the time zone search field (the keyboard is slow to drive from a test).
    #[cfg(test)]
    fn type_search(&mut self, text: &str) {
        if let Step::Zone(z) = &mut self.step {
            if let ZoneView::Search(kb) = &mut z.view {
                kb.text = text.into();
            }
        }
    }

    /// Opens the time zone step at once, saying it is looking; what the system and the Wi-Fi chip say (`iw reg get`,
    /// the zoneinfo tree and its tables) is read on a thread (`look_zone`) and swapped in by `SetupEvent::Zone`.
    fn enter_zone(&mut self) {
        self.step = Step::Zone(Box::new(ZoneStep::looking()));
        let (hw, notify) = (self.hw.clone(), self.notify.clone());
        std::thread::spawn(move || notify(SetupEvent::Zone(ZoneFacts(look_zone(&*hw)))));
    }

    /// The offsets of the zones the step shows, read on a thread when a screen with new ones opened.
    fn zone_offsets_wanted(&mut self) {
        let Step::Zone(z) = &mut self.step else { return };
        let wanted = z.wanted();
        if wanted.is_empty() {
            return;
        }
        z.asked.extend(wanted.iter().cloned());
        let (hw, notify) = (self.hw.clone(), self.notify.clone());
        std::thread::spawn(move || notify(SetupEvent::ZoneOffsets(zone_offsets(&*hw, wanted))));
    }

    /// Makes `zone` the system's zone (nothing to do when it already is). False, with the reason on screen, when refused.
    fn set_zone(&mut self, zone: &str) -> bool {
        if self.hw.timezone().as_deref() == Some(zone) {
            return true;
        }
        match self.hw.set_timezone(zone) {
            Ok(()) => {
                // The night light's schedule is in local time.
                crate::night_light::poke();
                true
            }
            Err(e) => {
                if let Step::Zone(z) = &mut self.step {
                    z.error = Some(e);
                }
                false
            }
        }
    }

    fn press_zone(&mut self, b: Button) {
        let Step::Zone(z) = &mut self.step else { return };
        if z.loading {
            // Nothing to confirm or pick yet; leaving works.
            match b {
                Button::South => self.step = Step::Wifi,
                Button::Start => self.step = Step::Extras { sel: 0 },
                _ => {}
            }
            return;
        }
        z.error = None;
        let view = std::mem::replace(&mut z.view, ZoneView::Card);
        let (view, next) = match view {
            ZoneView::Card => match b {
                Button::East | Button::Start => (ZoneView::Card, ZoneNext::Take(None)),
                Button::North => (ZoneView::Regions { sel: z.region_row() }, ZoneNext::Stay),
                Button::South => (ZoneView::Card, ZoneNext::Back),
                _ => (ZoneView::Card, ZoneNext::Stay),
            },
            ZoneView::Regions { mut sel } => {
                let regions = timezone::regions(&z.all);
                let n = regions.len() + 1;
                let mut next = ZoneNext::Stay;
                let mut view = None;
                match b {
                    Button::Up => sel = sel.saturating_sub(1),
                    Button::Down => sel = (sel + 1).min(n - 1),
                    Button::L1 => sel = sel.saturating_sub(5),
                    Button::R1 => sel = (sel + 5).min(n - 1),
                    Button::South => view = Some(z.start_view()),
                    Button::Start => next = ZoneNext::Take(None),
                    Button::East if sel == 0 => view = Some(ZoneView::Search(Box::default())),
                    Button::East => {
                        let region = regions[sel - 1].clone();
                        let zones: Vec<String> = timezone::zones_in(&z.all, &region).into_iter().map(String::from).collect();
                        let at = z.guess.zone().and_then(|g| zones.iter().position(|x| x == g)).unwrap_or(0);
                        view = Some(ZoneView::List { heading: region, zones, sel: at, from: ListFrom::Regions });
                    }
                    _ => {}
                }
                (view.unwrap_or(ZoneView::Regions { sel }), next)
            }
            ZoneView::List { heading, zones, mut sel, from } => {
                let n = zones.len().max(1);
                let mut next = ZoneNext::Stay;
                let mut view = None;
                match b {
                    Button::Up => sel = sel.saturating_sub(1),
                    Button::Down => sel = (sel + 1).min(n - 1),
                    Button::L1 => sel = sel.saturating_sub(5),
                    Button::R1 => sel = (sel + 5).min(n - 1),
                    Button::East => {
                        if let Some(zone) = zones.get(sel) {
                            next = ZoneNext::Take(Some(zone.clone()));
                        }
                    }
                    Button::North if matches!(from, ListFrom::Country) => view = Some(ZoneView::Regions { sel: z.region_row() }),
                    Button::Start => next = ZoneNext::Take(None),
                    Button::South => match &from {
                        ListFrom::Country => next = ZoneNext::Back,
                        ListFrom::Regions => {
                            let row = timezone::regions(&z.all).iter().position(|r| *r == heading).map_or(0, |i| i + 1);
                            view = Some(ZoneView::Regions { sel: row });
                        }
                        ListFrom::Search(text) => view = Some(ZoneView::Search(Box::new(Keyboard { text: text.clone(), ..Default::default() }))),
                    },
                    _ => {}
                }
                (view.unwrap_or(ZoneView::List { heading, zones, sel, from }), next)
            }
            ZoneView::Search(mut kb) => match kb.press(b) {
                Outcome::Editing => (ZoneView::Search(kb), ZoneNext::Stay),
                Outcome::Cancelled => (ZoneView::Regions { sel: 0 }, ZoneNext::Stay),
                Outcome::Done(text) => {
                    let zones: Vec<String> = timezone::search(&z.all, &text).into_iter().map(String::from).collect();
                    (ZoneView::List { heading: format!("Search: {text}"), zones, sel: 0, from: ListFrom::Search(text) }, ZoneNext::Stay)
                }
            },
        };
        z.view = view;
        match next {
            ZoneNext::Stay => {}
            ZoneNext::Back => self.step = Step::Wifi,
            ZoneNext::Take(zone) => self.leave_zone(zone),
        }
    }

    /// Ends the time zone step: sets the picked zone, or the guess when nothing was picked. Stays when the system refuses.
    fn leave_zone(&mut self, pick: Option<String>) {
        let Step::Zone(z) = &self.step else { return };
        let want = pick.or_else(|| match &z.guess {
            // No guess to keep: UTC (or a whole country) leaves the zone as it is.
            Guess::Utc | Guess::Several { .. } => None,
            g => g.zone().map(String::from),
        });
        if want.is_none_or(|zone| self.set_zone(&zone)) {
            self.step = Step::Extras { sel: 0 };
        }
    }

    /// The Wi-Fi page: A connects (a saved network without its password), Y forgets a saved one (twice), X scans,
    /// the last two rows are Other network and Wi-Fi country.
    fn press_wifi(&mut self, b: Button) {
        let n = self.wifi_rows();
        // The row among the networks, Other network and Wi-Fi country; None on the sign-in row.
        let row = self.sel.checked_sub(self.lead());
        let entry = row.and_then(|r| self.entries.get(r)).cloned();
        // No Wi-Fi chip: nothing on this page to choose; Start goes on (first start), B goes back.
        if self.radio == Radio::Absent && !matches!(b, Button::Start | Button::South) {
            return;
        }
        // The tick for A on the connected network stays until the next press.
        self.confirm = false;
        // Anything but a second Y keeps the network; B then only takes the question back.
        if b != Button::West && self.forget.take().is_some() {
            self.note = None;
            if b == Button::South {
                return;
            }
        }
        match b {
            Button::Up => self.sel = self.sel.saturating_sub(1),
            Button::Down => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            Button::L1 => self.sel = self.sel.saturating_sub(5),
            Button::R1 => self.sel = (self.sel + 5).min(n.saturating_sub(1)),
            Button::North => self.scan(),
            Button::Start => self.enter_zone(),
            Button::South => self.step = if self.only.is_none() { Step::Play { sel: 0 } } else { Step::Look },
            Button::West => match entry.and_then(|e| e.saved) {
                Some(saved) if self.forget.as_deref() == Some(saved.uuid.as_str()) => {
                    self.forget = None;
                    self.note = Some((format!("Forgetting {}…", saved.ssid), false));
                    self.forget_saved(saved);
                }
                Some(saved) => {
                    self.note = Some((format!("Forget {}? Press Y again. B keeps it.", saved.ssid), true));
                    self.forget = Some(saved.uuid);
                }
                None => {}
            },
            Button::East if row.is_none() && self.lead_kind() == Some(Lead::Off) => self.turn_wifi_on(),
            Button::East if row.is_none() => self.open_portal(),
            Button::East if row == Some(self.entries.len()) && self.radio.is_off() => self.note = Some(("Turn Wi-Fi on first.".into(), false)),
            Button::East if row == Some(self.entries.len()) => self.step = Step::HiddenName { kb: Keyboard::default() },
            Button::East if row == Some(self.entries.len() + 1) => self.open_country(),
            Button::East => match entry {
                // Nothing to connect: said with a tick and what comes next, not a changed subtitle.
                Some(e) if e.active => {
                    self.note = None;
                    self.confirm = true;
                }
                Some(e) if connecting(&e) && !self.stuck() => self.note = Some((format!("Still connecting to {}. One moment.", e.ssid), false)),
                // Not answering: a secured network most likely refuses the stored password, so it is typed again.
                Some(e) if connecting(&e) && e.secure => self.step = Step::Password { ssid: e.ssid, kb: Keyboard::default(), hidden: false },
                Some(Entry { saved: Some(saved), .. }) => self.connect_saved(saved),
                Some(e) if e.secure => self.step = Step::Password { ssid: e.ssid, kb: Keyboard::default(), hidden: false },
                Some(e) => self.connect(e.ssid, None),
                None => {}
            },
            _ => {}
        }
    }

    pub fn press(&mut self, b: Button) -> bool {
        let done = self.press_step(b);
        self.remember_step(done);
        self.keep_watch();
        // "Sign in…" was left: its session (and cookie jar) goes with it.
        if !matches!(&self.step, Step::Portal(p) if p.signin.is_some()) {
            self.portal_session = None;
        }
        match self.only {
            Some(only) => done || stage(&self.step) != Some(only),
            None => done,
        }
    }

    /// First start only: the step reached goes to `setup-step` in the data folder (a few bytes, on a button press), so a
    /// restart or a crash resumes there (`resume`); finishing deletes the file. Only the steps `resume` can open again
    /// are written; the others (Done, the keyboards inside Wi-Fi) leave the last line as it is.
    fn remember_step(&mut self, done: bool) {
        if self.only.is_some() {
            return;
        }
        let file = self.data_dir.join(RESUME_FILE);
        if done {
            let _ = std::fs::remove_file(file);
        } else if let Some(name) = resume_name(&self.step) {
            if std::fs::read_to_string(&file).ok().as_deref() != Some(name) {
                let _ = std::fs::create_dir_all(&self.data_dir);
                let _ = crate::fsutil::write_atomic(&file, name.as_bytes());
            }
        }
    }

    /// The first start after a restart: opens at the step the last run reached (`remember_step`). A missing, unreadable
    /// or unknown line means the beginning; Wi-Fi, the zone and the games are looked at afresh, as when stepping in.
    pub fn resume(mut self) -> Self {
        let Ok(name) = std::fs::read_to_string(self.data_dir.join(RESUME_FILE)) else { return self };
        match name.trim() {
            "look" => {
                self.step = Step::Look;
                self.scan();
            }
            "play" => self.step = Step::Play { sel: 0 },
            "wifi" => {
                self.step = Step::Wifi;
                self.scan();
            }
            "zone" => self.enter_zone(),
            "extras" => self.step = Step::Extras { sel: 0 },
            "games" => self.go_to_games(),
            _ => {}
        }
        self
    }

    fn press_step(&mut self, b: Button) -> bool {
        match &mut self.step {
            Step::Welcome => {
                if b == Button::East || b == Button::Start {
                    self.step = Step::Look;
                }
                // The language, in use from the next frame on (`i18n`).
                if matches!(b, Button::Left | Button::Right) {
                    let lang = crate::i18n::current().next();
                    crate::i18n::set(lang);
                    self.language_chosen = Some(lang);
                }
            }
            Step::Look => match b {
                Button::Left | Button::Up => self.theme = (self.theme + self.themes.len() - 1) % self.themes.len(),
                Button::Right | Button::Down => self.theme = (self.theme + 1) % self.themes.len(),
                Button::West => {
                    let i = Appearance::ALL.iter().position(|a| *a == self.appearance).unwrap_or(0);
                    self.appearance = Appearance::ALL[(i + 1) % Appearance::ALL.len()];
                }
                // Cozy or Quick; from the first press the look no longer decides.
                Button::L1 => {
                    self.animations = Some(self.animations_shown().next());
                    self.animations_changed = true;
                }
                Button::East | Button::Start => {
                    // The scan starts now, so the networks are there when the Wi-Fi step opens.
                    self.step = if self.only.is_none() { Step::Play { sel: 0 } } else { Step::Wifi };
                    self.scan();
                }
                Button::South => self.step = Step::Welcome,
                // Settings > Look only: the first start has no such step.
                Button::North if self.only == Some(Only::Look) && !self.shelf.is_empty() => self.step = Step::ShelfSystems { sel: 0 },
                // Settings > Look only: the Theme page (`theme_page`).
                Button::R2 if self.only == Some(Only::Look) => self.step = Step::Theme { sel: 0 },
                // Settings > Look only, to keep the first start short: Motion, Full / Reduced / Off.
                Button::R1 if self.only == Some(Only::Look) => self.motion = self.motion.next(),
                // The first start: the text size instead (in Settings it has its own row under Look & feel), in use
                // from the next frame on (`type_scale`).
                Button::R1 => {
                    let size = crate::type_scale::get().next();
                    crate::type_scale::set(size);
                    self.text_size_chosen = Some(size);
                }
                _ => {}
            },
            Step::Play { sel } => match b {
                Button::Up => *sel = (*sel + Profile::ALL.len() - 1) % Profile::ALL.len(),
                Button::Down => *sel = (*sel + 1) % Profile::ALL.len(),
                Button::East => match Profile::ALL[*sel] {
                    Profile::Choose => {
                        let cards = Cards::new(features::Mode::First, features::choose_list(), self.switches.clone(), self.later.clone());
                        self.step = Step::Cards(Box::new(cards));
                    }
                    p => {
                        features::apply(p, &mut self.switches);
                        self.motion = self.switches.motion;
                        self.tour_changed = true;
                        self.step = Step::Wifi;
                    }
                },
                // Skipped: every switch stays as it is.
                Button::Start => self.step = Step::Wifi,
                Button::South => self.step = Step::Look,
                _ => {}
            },
            Step::Cards(cards) => {
                let out = cards.press(b);
                if let Some(s) = cards.take_switches() {
                    self.motion = s.motion;
                    self.switches = s;
                    self.tour_changed = true;
                }
                if cards.later != self.later {
                    self.later = cards.later.clone();
                    self.tour_changed = true;
                }
                match out {
                    features::Out::Done => self.step = Step::Wifi,
                    features::Out::Back => self.step = Step::Play { sel: 2 },
                    features::Out::Stay | features::Out::Changed => {}
                }
            }
            Step::ShelfSystems { .. } => self.press_shelf(b),
            Step::Theme { sel } => {
                if crate::theme_page::press(sel, b) {
                    self.step = Step::Look;
                }
            }
            Step::Wifi => self.press_wifi(b),
            Step::Password { ssid, kb, hidden } => match kb.press(b) {
                Outcome::Editing => {}
                Outcome::Cancelled => self.step = Step::Wifi,
                // A hidden network may be open: an empty password connects without one.
                Outcome::Done(psk) if psk.is_empty() && !*hidden => {}
                Outcome::Done(psk) => {
                    let (ssid, hidden) = (ssid.clone(), *hidden);
                    self.connect_to(ssid, (!psk.is_empty()).then_some(psk), hidden);
                }
            },
            Step::HiddenName { kb } => match kb.press(b) {
                Outcome::Editing => {}
                Outcome::Cancelled => self.step = Step::Wifi,
                Outcome::Done(name) if name.trim().is_empty() => {}
                Outcome::Done(name) => self.step = Step::Password { ssid: name, kb: Keyboard::default(), hidden: true },
            },
            Step::Country { sel, list } => match b {
                Button::Up => *sel = (*sel + list.len() - 1) % list.len().max(1),
                Button::Down => *sel = (*sel + 1) % list.len().max(1),
                Button::L1 => *sel = sel.saturating_sub(8),
                Button::R1 => *sel = (*sel + 8).min(list.len().saturating_sub(1)),
                // The letter jump of the game lists: the first country of the next (previous) letter.
                Button::R2 | Button::L2 => *sel = letter_jump(list, *sel, b == Button::R2),
                Button::South => self.step = Step::Wifi,
                Button::East => {
                    let code = list.get(*sel).map(|(c, _)| c.clone()).filter(|c| !c.is_empty());
                    let notify = self.notify.clone();
                    self.watch_net();
                    self.net.exclusive("country", move || {
                        let result = wifi::set_country(code.as_deref()).map(|_| code);
                        notify(SetupEvent::CountrySet(result));
                    });
                    self.step = Step::Wifi;
                    self.note = Some(("Setting the Wi-Fi country…".into(), false));
                }
                _ => {}
            },
            Step::Connecting { .. } => {}
            Step::Portal(page) => match page.press(b) {
                crate::portal_page::Out::Stay => {}
                crate::portal_page::Out::Back => self.step = Step::Wifi,
                crate::portal_page::Out::Load { ticket, start } => self.portal_load(ticket, start),
                crate::portal_page::Out::Send { ticket, form, submitter, url } => self.portal_send(ticket, *form, submitter, url),
            },
            Step::Zone(_) => {
                self.press_zone(b);
                self.zone_offsets_wanted();
            }
            Step::Extras { sel } => match b {
                Button::Up => *sel = (*sel + EXTRAS_ROWS - 1) % EXTRAS_ROWS,
                Button::Down => *sel = (*sel + 1) % EXTRAS_ROWS,
                // The Pictures choice needs no Wi-Fi: it only says what the downloads bring.
                Button::East | Button::Right if *sel == 1 => {
                    self.art = self.art.next();
                    self.art_changed = true;
                }
                Button::Left if *sel == 1 => {
                    self.art = self.art.prev();
                    self.art_changed = true;
                }
                Button::East if self.ip.is_some() || wifi::ip().is_some() => {
                    if *sel == 0 {
                        self.start_covers();
                    } else {
                        self.step = Step::Ports;
                        self.load_catalog();
                    }
                }
                Button::Start => self.go_to_games(),
                Button::South => self.enter_zone(),
                _ => {}
            },
            Step::Ports => {
                let last = self.tab_ports().len().saturating_sub(1);
                let sel = &mut self.port_sel[self.port_tab.index()];
                match b {
                    Button::Up => *sel = sel.saturating_sub(1),
                    Button::Down => *sel = (*sel + 1).min(last),
                    Button::L1 => *sel = sel.saturating_sub(8),
                    Button::R1 => *sel = (*sel + 8).min(last),
                    // The two lists side by side, as tabs.
                    Button::Left | Button::Right => self.port_tab = self.port_tab.other(),
                    Button::East => self.toggle_port(false),
                    // An update is waiting: A updates, Y still removes.
                    Button::West if self.selected_port().is_some_and(|p| self.update_for(p)) => self.toggle_port(true),
                    Button::North => {
                        if let Some(port) = self.selected_port().filter(|p| p.needs_files).cloned() {
                            self.open_port_files(&port);
                        }
                    }
                    Button::South | Button::Start => self.step = Step::Extras { sel: 2 },
                    _ => {}
                }
            }
            // X turns file sharing on or off right here, as on the games step.
            Step::PortFiles(_) if b == Button::North => self.switch_sharing(),
            Step::PortFiles(page) => {
                if page.press(b) || b == Button::Start {
                    self.step = Step::Ports;
                }
            }
            // The grid moves and opens All systems; X switches file sharing.
            Step::Games if self.grid.as_mut().is_some_and(|g| g.press(b)) => {}
            Step::Games => match b {
                Button::North => self.switch_sharing(),
                Button::Start => self.step = Step::Done,
                Button::South => self.step = Step::Extras { sel: 0 },
                _ => {}
            },
            Step::Remote => match b {
                Button::East if !self.remote_busy && !self.remote_looking && matches!(self.remote, remote::Mode::Off | remote::Mode::On | remote::Mode::External) => {
                    if self.remote == remote::Mode::Off && self.ip.is_none() {
                        return false;
                    }
                    self.remote_busy = true;
                    let (notify, on) = (self.notify.clone(), self.remote == remote::Mode::Off);
                    std::thread::spawn(move || {
                        let result = if on { remote::turn_on().map(|_| ()) } else { remote::turn_off() };
                        notify(SetupEvent::Remote { mode: remote::current(), password: remote::password(), result: Some(result) })
                    });
                }
                Button::South | Button::Start => self.step = Step::Done,
                _ => {}
            },
            Step::Done => return b == Button::East || b == Button::Start,
        }
        false
    }

    fn go_to_games(&mut self) {
        // On a thread (`systemctl`): the page opens at once (review 2026-10-24).
        self.look_sharing();
        self.ip = wifi::ip();
        // The page opens with the counts the shelf knows (in memory); the folders are counted again on a thread,
        // because games may have been copied since (review 2026-10-24: every ROM folder was walked here).
        let counts: HashMap<String, usize> = self.shelf.iter().filter(|s| s.games > 0).map(|s| (s.id.clone(), s.games)).collect();
        self.grid = Some(Grid::new(self.catalog.clone(), counts, GRID_COLS));
        self.step = Step::Games;
        self.count_games();
    }

    /// Counts the games in every system's folder on a thread; `SetupEvent::GameCounts` brings the numbers.
    fn count_games(&mut self) {
        self.counting = true;
        let (catalog, notify) = (self.catalog.clone(), self.notify.clone());
        std::thread::spawn(move || notify(SetupEvent::GameCounts(count_catalog(&catalog))));
    }

    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text) {
        if let Step::Cards(cards) = &mut self.step {
            cards.paint(c, text);
            return;
        }
        c.clear(pal().void);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        match &self.step {
            Step::Welcome => {
                // pocket-ui's mark beside the handheld it runs on, above the welcome (`ui_art`; the Mini V2 where the
                // device isn't known).
                let model = crate::device::current().model.unwrap_or(crate::device::Model::MiniV2);
                let row = [crate::ui_art::Art::Logo, crate::ui_art::Art::Device(model)];
                let dy = crate::ui_art::above_words_row(c, &row, MARGIN, false, 60, 300, 67);
                text.draw(c, Face::Title, 67, MARGIN, 300 + dy, pal().chalk, "Welcome to your Pocket");
                let lines = [
                    "A few short steps: a look you like, how you like to play, Wi-Fi, your time zone, your games.",
                    "You can skip any of them and come back later.",
                ];
                let mut below = paragraphs(c, text, &lines, 360 + dy);
                // On one of the Mini V2's siblings nobody on the project has tested: one calm line, once (this step
                // is only on the first start; docs/DEVICES.md).
                if let Some(m) = crate::device::current().model.filter(|m| m.experimental()) {
                    let name = m.name();
                    let line = if name.starts_with(['A', 'E', 'I', 'O', 'U']) {
                        format!("pocket-ui hasn't been tested on an {} yet. Most things should work; if something doesn't, Settings › System › About › Report a problem.", name)
                    } else {
                        format!("pocket-ui hasn't been tested on a {} yet. Most things should work; if something doesn't, Settings › System › About › Report a problem.", name)
                    };
                    below = paragraphs(c, text, &[line.as_str()], below + 56);
                }
                // The languages in their own names; the line after them in the one chosen.
                let on = crate::i18n::Lang::ALL.iter().position(|l| *l == crate::i18n::current()).unwrap_or(0);
                let names: Vec<&str> = crate::i18n::Lang::ALL.iter().map(|l| l.name()).collect();
                pill_row(c, text, below.max(470 - 40) + 40, "Language", &names, on, "You can change it later in Settings.", 0);
                hints(c, text, &[("Left/Right", "Language"), ("A", "Start"), ("SELECT", "Manual")]);
            }
            // The app paints this step itself, with the previews (`paint_look`); without them, the names.
            Step::Look => self.paint_look(c, text, &[], false),
            Step::Play { sel } => {
                features::paint_profiles(c, text, *sel);
            }
            Step::ShelfSystems { sel } => self.paint_shelf_systems(c, text, *sel),
            Step::Theme { sel } => crate::theme_page::paint(c, text, *sel, None),
            Step::Wifi => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Wi-Fi");
                let (line, color) = self.wifi_status(&dots);
                // A on the connected network: a drawn tick before the line (the font has none).
                let tick = if self.confirming().is_some() { 48 } else { 0 };
                let line = text.fit(Face::Body, 28, &line, c.w - 2 * MARGIN - tick);
                if tick > 0 {
                    for i in 0..7 {
                        c.rect(MARGIN + i * 2, 178 + i * 2, 4, 4, color);
                    }
                    for i in 0..14 {
                        c.rect(MARGIN + 12 + i * 2, 190 - i * 2, 4, 4, color);
                    }
                }
                text.draw(c, Face::Body, 28, MARGIN + tick, 192, color, &line);
                self.paint_networks(c, text);
                let row = self.sel.checked_sub(self.lead());
                let saved = row.and_then(|r| self.entries.get(r)).is_some_and(|e| e.saved.is_some());
                let a = match row.map(|r| r.checked_sub(self.entries.len())) {
                    Some(None) => "Connect",
                    None if self.lead_kind() == Some(Lead::Off) => "Turn on",
                    _ => "Open",
                };
                if self.radio == Radio::Absent {
                    let mut h = Vec::new();
                    if self.only.is_none() {
                        h.push(("START", "Next"));
                    }
                    h.push(("B", "Back"));
                    hints(c, text, &h);
                } else {
                    hints(c, text, &wifi_hints(a, saved, self.forget.is_some(), self.only.is_none()));
                }
            }
            Step::Password { ssid, kb, hidden } => {
                let title = text.fit(Face::Title, 50, &format!("Password for {ssid}"), c.w - 2 * MARGIN);
                text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &title);
                paint_keyboard(c, text, kb);
                // Why the password is asked for again ("Wrong password. Try again."), between the field and the keys.
                if let Some((msg, alert)) = &self.note {
                    let msg = text.fit(Face::Body, 24, msg, c.w - 2 * MARGIN);
                    text.draw(c, Face::Body, 24, MARGIN, 272, if *alert { pal().alert } else { pal().dust }, &msg);
                }
                let done = if *hidden { "Connect (empty: open network)" } else { "Connect" };
                hints(c, text, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", done)]);
            }
            Step::HiddenName { kb } => {
                text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, "Name of the hidden network");
                paint_keyboard_with(c, text, kb, "Next");
                hints(c, text, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Next")]);
            }
            Step::Country { sel, list } => self.paint_country(c, text, *sel, list),
            Step::Portal(page) => page.paint(c, text, &dots),
            Step::Connecting { ssid } => {
                text.draw(c, Face::Title, 67, MARGIN, 300, pal().chalk, &format!("Connecting to {ssid}{dots}"));
                text.draw(c, Face::Body, 28, MARGIN, 360, pal().dust, "This can take up to a minute.");
            }
            Step::Zone(_) => self.paint_zone(c, text, &dots),
            // Painted above.
            Step::Cards(_) => {}
            Step::Extras { sel } => {
                let sel = *sel;
                self.paint_extras(c, text, sel, &dots);
            }
            Step::Ports => self.paint_ports(c, text, &dots),
            Step::PortFiles(_) => self.paint_port_files(c, text, &dots),
            Step::Games => self.paint_games(c, text, &dots),
            Step::Remote => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Remote access");
                let lines = [
                    "Log in to this Pocket from a computer on the same Wi-Fi, with SSH.",
                    "It turns itself off after 30 minutes without a connection, and when the Pocket restarts.",
                ];
                for (i, l) in lines.iter().enumerate() {
                    text.draw(c, Face::Body, 28, MARGIN, 210 + i as i32 * 44, pal().dust, l);
                }
                let ip = self.ip.clone().unwrap_or_else(|| "(not connected)".into());
                text.draw(c, Face::Title, 37, MARGIN, 366, pal().chalk, &format!("ssh root@{ip}"));
                let on = matches!(self.remote, remote::Mode::On | remote::Mode::Dev | remote::Mode::External);
                if on {
                    // Dev or external SSH: pocket-ui never set a password, root's own one applies.
                    let pw = self.remote_password.as_deref().or((self.remote != remote::Mode::On).then_some("(unchanged)"));
                    paint_login(c, text, 434, "root", pw);
                }
                let (status, color) = match (&self.remote_error, self.remote) {
                    _ if self.remote_busy => (format!("Switching remote access{dots}"), pal().dust),
                    _ if self.remote_looking => (format!("Looking at remote access{dots}"), pal().dust),
                    (Some(e), _) => (e.clone(), pal().alert),
                    (None, remote::Mode::On) => ("Remote access is on.".into(), pal().chalk),
                    (None, remote::Mode::Dev) => ("Always on: this is a developer device (dev-ssh).".into(), pal().chalk),
                    (None, remote::Mode::External) => ("On, switched on outside pocket-ui. No time limit.".into(), pal().chalk),
                    (None, remote::Mode::Off) if self.ip.is_none() => ("Connect to Wi-Fi first.".into(), pal().dust),
                    (None, remote::Mode::Off) => ("Remote access is off.".into(), pal().dust),
                };
                text.draw(c, Face::Body, 28, MARGIN, 544, color, &status);
                let h: &[(&str, &str)] = match self.remote {
                    remote::Mode::Dev => &[("B", "Back")],
                    // Without an address A does nothing (the status line says "Connect to Wi-Fi first").
                    remote::Mode::Off if self.ip.is_none() => &[("B", "Back")],
                    remote::Mode::Off => &[("A", "Turn on remote access"), ("B", "Back")],
                    _ => &[("A", "Turn off remote access"), ("B", "Back")],
                };
                hints(c, text, h);
            }
            Step::Done => {
                text.draw(c, Face::Title, 67, MARGIN, 300, pal().chalk, "You're all set");
                let games = done_line(self.games.len());
                paragraphs(c, text, &[games.as_str(), "Settings › System › Tour shows every feature again, one card each."], 360);
                hints(c, text, &[("A", "Go to your games"), ("SELECT", "Manual")]);
            }
        }
        self.paint_step_count(c, text);
    }

    /// First start only: "Step 3 of 7" at the top right, so a new player knows how long the setup is. The Look step is
    /// painted by the app (`paint_look`, which calls this too); the cards have their own "n of m".
    fn paint_step_count(&self, c: &mut Canvas, text: &mut Text) {
        if let Some(n) = self.only.is_none().then(|| step_number(&self.step)).flatten() {
            text.draw_right(c, Face::Body, 28, c.w - MARGIN, 72, pal().dust, &format!("Step {n} of {FIRST_STEPS}"));
        }
    }

    fn paint_extras(&mut self, c: &mut Canvas, text: &mut Text, sel: usize, dots: &str) {
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Extras");
        let online = wifi::ip().is_some();
        let sub = if online { "Optional. You can get these any time later." } else { "Connect to Wi-Fi first to use these." };
        text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, sub);
        let pictures = self.art == Art::Pictures;
        let covers = extras_status(self.covers, self.covers_running, pictures, self.games.len(), dots);
        let ports = if self.ports.is_empty() { "Browse".to_string() } else { format!("{} ready to play", self.tab_count(PortTab::Ready)) };
        let rows = [
            ("Cover art", "Box art for your games, from the free libretro collection.", covers),
            // The choices are drawn as pills in place of the line (`pill_track`), the current one on paper.
            ("Pictures", "", String::new()),
            ("PortMaster games", "PC and indie games from PortMaster: ready to play, or with your own game files.", ports),
        ];
        let width = c.w - 2 * MARGIN;
        // A title and a line under it: taller by their growth with Settings → Text size.
        let (g1, g2) = (crate::type_scale::grow(50), crate::type_scale::grow(24));
        let h = 132 + g1 + g2;
        for (i, (title, about, status)) in rows.iter().enumerate() {
            let y = 240 + i as i32 * (h + 18);
            let selected = i == sel;
            crate::kit::shelf_row(c, MARGIN, y, width, h, selected);
            // The switch works without Wi-Fi; the downloads and the browser need it.
            let usable = online || i == 1;
            let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else if usable { (pal().chalk, pal().dust) } else { (pal().off, pal().off) };
            let status_w = text.width(Face::Body, 24, status);
            let title = text.fit(Face::Title, 50, title, width - 56 - status_w - 40);
            text.draw(c, Face::Title, 50, MARGIN + 28, y + 62 + g1, fg, &title);
            if i == 1 {
                let names: Vec<&str> = Art::ALL.iter().map(|a| a.label()).collect();
                let on = Art::ALL.iter().position(|a| *a == self.art).unwrap_or(0);
                pill_track(c, text, MARGIN + 28, y + 80 + g1, &names, on);
                continue;
            }
            let about = text.fit(Face::Body, 24, about, width - 56);
            text.draw(c, Face::Body, 24, MARGIN + 28, y + 104 + g1 + g2, dim, &about);
            text.draw_right(c, Face::Body, 24, MARGIN + width - 28, y + 58 + g1, dim, status);
        }
        // What the downloads are and where from, whatever is chosen: this step is where it is decided.
        let mut y = 240 + EXTRAS_ROWS as i32 * (h + 18) + 26;
        for l in text.wrap(Face::Body, 24, PICTURES_NOTE, width) {
            text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, &l);
            y += crate::type_scale::line(24, 10);
        }
        let action = match sel {
            0 => "Download",
            1 => "Change",
            _ => "Browse",
        };
        hints(c, text, &extras_hints(action, online || sel == 1));
    }

    fn paint_ports(&mut self, c: &mut Canvas, text: &mut Text, dots: &str) {
        text.draw(c, Face::Title, 67, MARGIN, 124, pal().chalk, "PortMaster games");
        // The two lists as tabs: the one on show is paper, like a selected row.
        let mut x = MARGIN;
        for tab in [PortTab::Ready, PortTab::OwnFiles] {
            let label = if self.ports.is_empty() { tab.label().to_string() } else { format!("{}  {}", tab.label(), self.tab_count(tab)) };
            let w = text.width(Face::Body, 24, &label) + 40;
            let on = tab == self.port_tab;
            c.round_rect(x, 152, w, 44, 22, if on { pal().label } else { pal().spine });
            text.draw(c, Face::Body, 24, x + 20, 182, if on { pal().ink } else { pal().dust }, &label);
            x += w + 12;
        }
        let about = match self.port_tab {
            PortTab::Ready => "They run without extra files.",
            PortTab::OwnFiles => "They need files from your own copy of the game. pocket-ui shows where they go.",
        };
        let (line, color) = match (&self.port_busy, &self.port_note, self.catalog_loading) {
            (Some(title), _, _) => (format!("Working on {title}{dots}"), pal().dust),
            (_, Some((msg, err)), _) => (msg.clone(), if *err { pal().alert } else { pal().dust }),
            (_, _, true) => (format!("Loading the catalogue{dots}"), pal().dust),
            _ => (about.to_string(), pal().dust),
        };
        let line = text.fit(Face::Body, 28, &line, c.w - 2 * MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, 240, color, &line);
        let other = ("Left/Right", self.port_tab.other().label());
        let list = self.tab_ports();
        if list.is_empty() {
            if self.ports.is_empty() {
                hints(c, text, &[("B", "Back")]);
            } else {
                hints(c, text, &[other, ("B", "Back")]);
            }
            return;
        }

        // List on the left, preview and description on the right.
        let (top, row, list_w) = (276, 58, 520);
        let visible = ((c.h - 130 - top) / row).max(1) as usize;
        let sel = self.port_sel[self.port_tab.index()].min(list.len() - 1);
        let first = sel.saturating_sub(visible / 2).min(list.len().saturating_sub(visible));
        for (k, &i) in list.iter().enumerate().skip(first).take(visible) {
            let port = &self.ports[i];
            let y = top + (k - first) as i32 * row;
            let selected = k == sel;
            crate::kit::shelf_row(c, MARGIN, y, list_w, row - 6, selected);
            let fg = if selected { pal().ink } else { pal().chalk };
            let side = if self.update_for(port) {
                "Update available".to_string()
            } else if ports::installed(port) {
                "Installed".to_string()
            } else {
                ports::size_label(port.size)
            };
            let side_w = text.width(Face::Body, 21, &side);
            let title = text.fit(Face::Title, 30, &port.title, list_w - side_w - 64);
            text.draw(c, Face::Title, 30, MARGIN + 20, y + 36, fg, &title);
            text.draw_right(c, Face::Body, 21, MARGIN + list_w - 20, y + 34, if selected { pal().label_dust } else { pal().dust }, &side);
        }

        let port = self.ports[list[sel]].clone();
        let (px, pw) = (MARGIN + list_w + 40, c.w - MARGIN - (MARGIN + list_w + 40));
        // Ports that need files keep the picture smaller: the instructions need the room.
        let pic_h = if port.needs_files { pw / 2 } else { pw * 3 / 4 };
        let preview = self.previews.entry(port.name.clone()).or_insert_with(|| {
            ports::preview(&port).and_then(|p| image::load_image(&p)).map(|img| {
                let (w, h) = img.fit(pw, pw * 3 / 4);
                img.resized(w, h)
            })
        });
        let mut y = top;
        if let Some(img) = preview {
            let (w, h) = img.fit(pw, pic_h);
            c.blit_scaled(img, px, y, w, h);
            y += h + 40;
        } else {
            y += 30;
        }
        let title = text.fit(Face::Title, 37, &port.title, pw);
        text.draw(c, Face::Title, 37, px, y, pal().chalk, &title);
        let genres = port.genres.join(", ");
        text.draw(c, Face::Body, 24, px, y + 36, pal().dust, &genres);
        let bottom = c.h - 120;
        let desc_lines = if port.needs_files { 3 } else { 5 };
        let mut ly = y + 76;
        let desc = text.wrap(Face::Body, 24, &port.desc, pw);
        for (i, l) in desc.iter().take(desc_lines).enumerate() {
            let l = if i + 1 == desc_lines && desc.len() > desc_lines { text.fit(Face::Body, 24, &format!("{l} …"), pw) } else { l.clone() };
            text.draw(c, Face::Body, 24, px, ly, pal().chalk, &l);
            ly += 32;
        }
        if port.needs_files {
            // What to copy, in PortMaster's words, as far as it fits; X shows all of it.
            ly += 16;
            text.draw(c, Face::Body, 24, px, ly, pal().dust, "Your own game files");
            ly += 36;
            let inst = if port.instructions.is_empty() { "PortMaster gives no instructions for this game." } else { port.instructions.as_str() };
            let lines: Vec<String> = port_files::inst_lines(text, inst, pw).into_iter().filter(|l| !l.is_empty()).collect();
            let room = ((bottom - ly) / 32 + 1).max(0) as usize;
            for (i, l) in lines.iter().take(room).enumerate() {
                let l = if i + 1 == room && lines.len() > room { text.fit(Face::Body, 24, &format!("{l} …"), pw) } else { l.clone() };
                text.draw(c, Face::Body, 24, px, ly + i as i32 * 32, pal().chalk, &l);
            }
        }
        let update = self.update_for(&port);
        let action = if update {
            "Update"
        } else if ports::installed(&port) {
            "Remove"
        } else {
            "Install"
        };
        let mut items = vec![("A", action)];
        if update {
            items.push(("Y", "Remove"));
        }
        if port.needs_files {
            items.push(("X", "How to copy"));
        }
        // With Y beside A the bar is full in large German text: the other list's long name is left out then (the two
        // tabs above the list show it, and Left/Right still switches).
        if !update {
            items.push(other);
        }
        items.extend([("L1/R1", "Page"), ("B", "Back")]);
        hints(c, text, &items);
    }

    fn paint_port_files(&mut self, c: &mut Canvas, text: &mut Text, dots: &str) {
        let share = self.files_share();
        let toggle = port_files::Toggle::Here { busy: self.sharing_busy, dots: dots.to_string() };
        let Step::PortFiles(page) = &mut self.step else { return };
        page.paint(c, text, &share, &toggle);
        let items = page.hints(&share, &toggle);
        hints(c, text, &items);
    }

    fn paint_zone(&self, c: &mut Canvas, text: &mut Text, dots: &str) {
        let Step::Zone(z) = &self.step else { return };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let width = c.w - 2 * MARGIN;
        if z.loading {
            text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Time zone");
            text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, &format!("Looking at your time zone{dots}"));
            hints(c, text, &[("START", "Next"), ("B", "Back")]);
            return;
        }
        match &z.view {
            ZoneView::Card => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Time zone");
                let country = z.country.as_deref().unwrap_or("");
                let (note, color) = match (&z.error, &z.guess) {
                    (Some(e), _) => (e.clone(), pal().alert),
                    (None, Guess::System(_)) => ("Already set on this Pocket.".to_string(), pal().dust),
                    (None, Guess::Country { .. }) => (format!("A guess from your Wi-Fi region: {country}."), pal().dust),
                    (None, _) => ("Couldn't tell where you are. Press X to choose.".to_string(), pal().dust),
                };
                text.draw(c, Face::Body, 28, MARGIN, 192, color, &note);

                let zone = z.guess.zone().unwrap_or("UTC");
                let (top, h) = (250, 220);
                c.rect(MARGIN, top, width, h, pal().spine);
                let city = text.fit(Face::Title, 67, &timezone::city_of(zone), width - 440);
                text.draw(c, Face::Title, 67, MARGIN + 40, top + 100, pal().chalk, &city);
                if city != zone {
                    text.draw(c, Face::Body, 28, MARGIN + 40, top + 156, pal().dust, zone);
                }
                if let Some(Some(off)) = z.offsets.get(zone).copied() {
                    text.draw_right(c, Face::Title, 67, MARGIN + width - 40, top + 100, pal().chalk, &timezone::clock_at(now, off));
                    text.draw_right(c, Face::Body, 28, MARGIN + width - 40, top + 156, pal().dust, &timezone::fmt_offset(off));
                }
                text.draw(c, Face::Body, 28, MARGIN, top + h + 60, pal().dust, "The clock sets itself from the internet while you're online.");
                text.draw(c, Face::Body, 28, MARGIN, top + h + 104, pal().dust, "Settings › System › Time has the rest.");
                let yes = if matches!(z.guess, Guess::Utc) { "Keep UTC" } else { "Looks right" };
                hints(c, text, &[("A", yes), ("X", "Change"), ("START", "Next"), ("B", "Back")]);
            }
            ZoneView::Regions { sel } => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Time zone");
                text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, "Choose your region, or search for your city.");
                let mut rows = vec!["Search…".to_string()];
                rows.extend(timezone::regions(&z.all));
                self.paint_zone_rows(c, text, &rows.iter().map(|r| (r.clone(), ">".to_string())).collect::<Vec<_>>(), *sel);
                hints(c, text, &[("A", "Open"), ("L1/R1", "Page"), ("START", "Next"), ("B", "Back")]);
            }
            ZoneView::List { heading, zones, sel, from } => {
                let heading = text.fit(Face::Title, 67, heading, width);
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, &heading);
                let sub = match from {
                    ListFrom::Country => "Several time zones here. Choose yours.",
                    _ => "Choose your city.",
                };
                text.draw(c, Face::Body, 28, MARGIN, 192, if z.error.is_some() { pal().alert } else { pal().dust }, z.error.as_deref().unwrap_or(sub));
                if zones.is_empty() {
                    text.draw(c, Face::Body, 28, MARGIN, 260, pal().chalk, "No time zone matches that.");
                    text.draw(c, Face::Body, 28, MARGIN, 304, pal().dust, "Try part of a city name, such as vie or york.");
                } else {
                    let rows: Vec<(String, String)> = zones
                        .iter()
                        .map(|zone| {
                            // Blank until the thread has read the zone's file.
                            let value = z
                                .offsets
                                .get(zone)
                                .copied()
                                .flatten()
                                .map_or(String::new(), |o| format!("{}   {}", timezone::fmt_offset(o), timezone::clock_at(now, o)));
                            (timezone::city_of(zone), value)
                        })
                        .collect();
                    self.paint_zone_rows(c, text, &rows, *sel);
                }
                if matches!(from, ListFrom::Country) {
                    hints(c, text, &[("A", "Set"), ("X", "Other places"), ("START", "Skip"), ("B", "Back")]);
                } else {
                    hints(c, text, &[("A", "Set"), ("L1/R1", "Page"), ("START", "Next"), ("B", "Back")]);
                }
            }
            ZoneView::Search(kb) => {
                text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, "Search time zones");
                paint_keyboard_with(c, text, kb, "Search");
                hints(c, text, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Search")]);
            }
        }
    }

    /// Rows of (name, value) under the heading, the selected one highlighted, scrolled to keep it in view.
    fn paint_zone_rows(&self, c: &mut Canvas, text: &mut Text, rows: &[(String, String)], sel: usize) {
        let (top, row) = (232, crate::type_scale::row(70, 37));
        let visible = ((c.h - 130 - top) / row).max(1) as usize;
        // A short list (the regions) keeps its top, "Search…", until the selection runs off the bottom; a long one centres it.
        let lead = if rows.len() <= 2 * visible { visible - 1 } else { visible / 2 };
        let first = sel.saturating_sub(lead).min(rows.len().saturating_sub(visible));
        let width = c.w - 2 * MARGIN;
        for (k, (name, value)) in rows.iter().enumerate().skip(first).take(visible).map(|(i, r)| (i - first, r)) {
            let i = first + k;
            let y = top + k as i32 * row;
            let selected = i == sel;
            crate::kit::shelf_row(c, MARGIN, y, width, row - 8, selected);
            let fg = if selected { pal().ink } else { pal().chalk };
            let value_w = text.width(Face::Body, 24, value);
            let name = text.fit(Face::Title, 37, name, width - 56 - value_w - 40);
            text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
            text.draw_right(c, Face::Body, 24, MARGIN + width - 28, y + (row - 8) / 2 + crate::type_scale::base(10, 24), if selected { pal().label_dust } else { pal().dust }, value);
        }
    }

    /// The look step: each look's real shelf, scaled down (`previews`, one per theme, `LOOK_PREVIEW` in size,
    /// made by the app), the one shown in the middle and its neighbours at the sides; then its name, one line
    /// about it, and how to switch. `sample`: the previews use the built-in sample games.
    pub fn paint_look(&mut self, c: &mut Canvas, text: &mut Text, previews: &[Image], sample: bool) {
        // The Theme page under it: the shelf in the look in use as its preview.
        if let Step::Theme { sel } = self.step {
            crate::theme_page::paint(c, text, sel, previews.get(self.theme));
            return;
        }
        c.clear(pal().void);
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Choose a look");
        let sub = if sample { "How the shelf looks, with sample games until yours are on the SD card." } else { "Your shelf, in each look." };
        text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, sub);
        self.paint_step_count(c, text);
        let (pw, ph) = look_preview((c.w, c.h));
        let (x, y) = ((c.w - pw) / 2, 226);
        let n = self.themes.len();
        if previews.len() == n {
            // The neighbours, dimmed and cut off by the screen's edges: there is more to either side.
            for (dx, i) in [(-(pw + 36), (self.theme + n - 1) % n), (pw + 36, (self.theme + 1) % n)] {
                c.round_rect(x + dx - 8, y - 8, pw + 16, ph + 16, 16, pal().spine);
                c.blit_scaled(&previews[i], x + dx, y, pw, ph);
                c.shade_rect(x + dx - 8, y - 8, pw + 16, ph + 16, pal().void, 170);
            }
            c.round_rect(x - 8, y - 8, pw + 16, ph + 16, 16, pal().keycap);
            c.blit_scaled(&previews[self.theme], x, y, pw, ph);
        }
        let (name, blurb) = self.themes[self.theme];
        let base = y + ph + 62 + crate::type_scale::grow(50);
        text.draw(c, Face::Title, 50, MARGIN, base, pal().chalk, name);
        let blurb = text.fit(Face::Body, 28, blurb, c.w - 2 * MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, base + 40 + crate::type_scale::grow(28), pal().dust, &blurb);
        // One mark per theme, the shown one filled.
        for i in 0..n as i32 {
            let mx = c.w - MARGIN - (n as i32 - i) * 28 + 12;
            let color = if i as usize == self.theme { pal().chalk } else { pal().keycap };
            c.rect(mx, base - 18, 16, 16, color);
        }
        // Appearance and a second row as two slimmer rows of pills, evenly spaced between the look's description and
        // the hints: Motion in Settings > Look, the text size on the first start.
        let top = base + 63 + crate::type_scale::grow(28);
        let second = top + pill_h() + 15;
        self.paint_appearance(c, text, top);
        if self.only == Some(Only::Look) {
            self.paint_motion(c, text, second);
        } else {
            self.paint_text_size(c, text, second);
        }
        self.paint_animations(c, text, second + pill_h() + 15);
        hints(c, text, &look_hints(self.only == Some(Only::Look), !self.shelf.is_empty()));
    }

    /// The appearance row of the look step: "Appearance", Dark / Light / Automatic with the one in use on a
    /// paper pill, and what it means (battery included) after it. `top`: the row's top edge.
    fn paint_appearance(&self, c: &mut Canvas, text: &mut Text, top: i32) {
        let names: Vec<&str> = Appearance::ALL.iter().map(|a| a.name()).collect();
        let on = Appearance::ALL.iter().position(|a| *a == self.appearance).unwrap_or(0);
        let label_w = self.label_w(text);
        pill_row(c, text, top, "Appearance", &names, on, self.appearance.blurb(), label_w);
    }

    /// The motion row under it (Settings > Look): Full / Reduced / Off.
    fn paint_motion(&self, c: &mut Canvas, text: &mut Text, top: i32) {
        let names: Vec<&str> = Motion::ALL.iter().map(|m| m.name()).collect();
        let on = Motion::ALL.iter().position(|m| *m == self.motion).unwrap_or(0);
        let label_w = self.label_w(text);
        pill_row(c, text, top, "Motion", &names, on, self.motion.blurb(), label_w);
    }

    /// The animations row under them: Cozy / Quick, with the plain line about the one in use.
    fn paint_animations(&self, c: &mut Canvas, text: &mut Text, top: i32) {
        let names: Vec<&str> = Animations::ALL.iter().map(|a| a.name()).collect();
        let shown = self.animations_shown();
        let on = Animations::ALL.iter().position(|a| *a == shown).unwrap_or(0);
        let label_w = self.label_w(text);
        pill_row(c, text, top, "Animations", &names, on, shown.blurb(), label_w);
    }

    /// The text size row under it (the first start): Normal / Medium / Large.
    fn paint_text_size(&self, c: &mut Canvas, text: &mut Text, top: i32) {
        use crate::type_scale::TextSize;
        let names: Vec<&str> = TextSize::ALL.iter().map(|t| t.label()).collect();
        let now = crate::type_scale::get();
        let on = TextSize::ALL.iter().position(|t| *t == now).unwrap_or(0);
        let label_w = self.label_w(text);
        let blurb = match now {
            TextSize::Normal => "You can change it later in Settings.",
            TextSize::Medium => "A little larger text on every screen.",
            TextSize::Large => "Larger text on every screen.",
        };
        pill_row(c, text, top, "Text size", &names, on, blurb, label_w);
    }

    /// The labels of the two pill rows line up.
    fn label_w(&self, text: &mut Text) -> i32 {
        let second = if self.only == Some(Only::Look) { "Motion" } else { "Text size" };
        ["Appearance", second, "Animations"].iter().map(|l| text.width(Face::Body, 24, l)).max().unwrap_or(0)
    }

    /// The line under the Wi-Fi page's title and its colour. What is true now outranks an old note: a switch under way
    /// or one that failed, an error, a radio that is off, are said before anything the page remembers; "No networks
    /// found" only for a scan that worked and saw nobody, "Couldn't search" for one that did not work. `dots`: the
    /// moving ellipsis.
    /// The connected network while its A-press is being answered (`confirm`) and nothing more urgent is on the line.
    fn confirming(&self) -> Option<&Entry> {
        let urgent = self.radio == Radio::Absent
            || self.switch_failed.is_some()
            || self.switching.is_some()
            || matches!(self.note, Some((_, true)))
            || self.radio.is_off();
        self.confirm.then(|| self.entries.iter().find(|e| e.active)).flatten().filter(|_| !urgent)
    }

    pub(crate) fn wifi_status(&self, dots: &str) -> (String, u32) {
        let active = self.entries.iter().find(|e| e.active);
        match (&self.note, self.scanning) {
            _ if self.radio == Radio::Absent => ("This device has no Wi-Fi. Everything else works without it.".into(), pal().dust),
            _ if self.switch_failed.is_some() => (self.switch_failed.as_ref().map_or("", |n| n.line(true)).into(), pal().alert),
            _ if self.switching == Some(true) => (format!("Turning Wi-Fi on{dots}"), pal().dust),
            _ if self.switching == Some(false) => (format!("Turning Wi-Fi off{dots}"), pal().dust),
            (Some((msg, true)), _) => (msg.clone(), pal().alert),
            _ if self.radio.is_off() => ("Wi-Fi is off. Press A to turn it on.".into(), pal().dust),
            _ if self.confirming().is_some() => {
                let e = self.confirming().unwrap();
                let line = if self.only.is_none() {
                    format!("Connected to {}. Press START for the next step.", e.ssid)
                } else {
                    format!("Connected to {}. Press B to go back.", e.ssid)
                };
                (line, pal().chalk)
            }
            (Some((msg, _)), _) => (msg.clone(), pal().dust),
            (None, false) if self.scan_err.is_some() => ("Couldn't search for networks. Try again (X).".into(), pal().alert),
            (None, _) if active.is_some() => (connected_line(active.unwrap(), self.ip.as_deref()), pal().chalk),
            (None, _) if self.connecting_since.is_some() => {
                let ssid = self.connecting_since.as_ref().map_or("", |(s, _)| s.as_str());
                let secure = self.entries.iter().find(|e| e.ssid == ssid).is_none_or(|e| e.secure);
                match (self.stuck(), secure) {
                    (false, _) => (format!("Connecting to {ssid}{dots}"), pal().dust),
                    (true, true) => (format!("{ssid} isn't letting this Pocket in. Press A to type the password again."), pal().alert),
                    (true, false) => (format!("{ssid} isn't answering. Press A to try again."), pal().alert),
                }
            }
            (None, true) if self.nets.is_empty() => (format!("Looking for networks{dots}"), pal().dust),
            (None, true) => (format!("Scanning{dots}"), pal().dust),
            (None, false) if self.nets.is_empty() => ("No networks found. Press X to scan again.".into(), pal().dust),
            (None, false) => ("Choose your network.".into(), pal().dust),
        }
    }

    /// The networks (in reach and saved), then Other network and Wi-Fi country, as spines.
    fn paint_networks(&self, c: &mut Canvas, text: &mut Text) {
        let (top, row) = (232, crate::type_scale::row(70, 37));
        let visible = ((c.h - 130 - top) / row).max(1) as usize;
        let first = self.sel.saturating_sub(visible.saturating_sub(1));
        let width = c.w - 2 * MARGIN;
        let rows = self.wifi_rows();
        let lead = self.lead();
        // The radio off, or a scan that worked and found nobody: a small radio asleep under the rows (`ui_art`), where
        // the networks would be, when there is room for it.
        if self.radio.is_off() || self.radio == Radio::Absent || (self.entries.is_empty() && !self.scanning && self.scan_err.is_none() && self.switching.is_none()) {
            let k = crate::ui_art::empty_scale(c.h);
            let (aw, ah) = crate::ui_art::size(crate::ui_art::Art::Offline, k);
            let y = top + rows.min(visible) as i32 * row + 48;
            if y + ah <= c.h - 130 {
                crate::ui_art::paint(c, crate::ui_art::Art::Offline, (c.w - aw) / 2, y, k);
            }
        }
        for (k, i) in (first..rows).take(visible).enumerate() {
            let y = top + k as i32 * row;
            let selected = i == self.sel;
            crate::kit::shelf_row(c, MARGIN, y, width, row - 8, selected);
            let fg = if selected { pal().ink } else { pal().chalk };
            let quiet = if selected { pal().label_dust } else { pal().dust };
            let Some(i) = i.checked_sub(lead) else {
                // The first row: "Wi-Fi is off" with its way to turn it on, or what the network wants (a sign-in) and
                // where (its host, or "Sign-in page").
                let off = self.lead_kind() == Some(Lead::Off);
                let side = if off { (if self.switching.is_some() { "Switching…" } else { "Turn on" }).to_string() } else { self.portal_at.as_deref().and_then(net_check::host_of).unwrap_or("Sign-in page").to_string() };
                let name = if off { "Wi-Fi is off" } else { "This network wants a sign-in" };
                let name_w = text.width(Face::Title, 37, name).min(width - 260);
                let name = text.fit(Face::Title, 37, name, name_w);
                text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
                // The off row's words are ours (translated); the sign-in row's side is a host name.
                text.set_translate(off);
                let side = text.fit(Face::Body, 24, &side, width - name_w - 100);
                text.draw_right(c, Face::Body, 24, MARGIN + width - 28, y + (row - 8) / 2 + crate::type_scale::base(10, 24), quiet, &side);
                text.set_translate(true);
                continue;
            };
            let (name, side, signal) = match self.entries.get(i) {
                Some(e) => {
                    let side = match (e.active, &e.saved, e.signal) {
                        (true, _, _) => "Connected",
                        (false, Some(s), _) if s.connecting && self.stuck() => "Not answering",
                        (false, Some(s), _) if s.connecting => "Connecting…",
                        (false, Some(_), None) => "Saved, out of range",
                        (false, Some(_), Some(_)) => "Saved",
                        (false, None, _) if e.secure => "Password",
                        // Not "Open", the hint bar's verb: the German table has one line per English word (`i18n`).
                        (false, None, _) => "No password",
                    };
                    (e.ssid.clone(), side.to_string(), e.signal)
                }
                None if i == self.entries.len() => ("Other network…".to_string(), "Hidden, by name".to_string(), None),
                None => ("Wi-Fi country".to_string(), self.country_value(), None),
            };
            let side_w = text.width(Face::Body, 24, &side);
            let name = text.fit(Face::Title, 37, &name, width - side_w - 180);
            text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
            match signal {
                Some(s) => {
                    text.draw_right(c, Face::Body, 24, MARGIN + width - 100, y + (row - 8) / 2 + crate::type_scale::base(10, 24), quiet, &side);
                    signal_bars(c, MARGIN + width - 80, y + (row - 8) / 2 + 15, s, fg);
                }
                None => text.draw_right(c, Face::Body, 24, MARGIN + width - 28, y + (row - 8) / 2 + crate::type_scale::base(10, 24), quiet, &side),
            }
        }
    }

    /// The Wi-Fi country step: a list of spines, "Automatic" first, the one in use marked.
    fn paint_country(&self, c: &mut Canvas, text: &mut Text, sel: usize, list: &[(String, String)]) {
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Wi-Fi country");
        let line = match &self.saving.reg {
            Some(r) => format!("Which channels Wi-Fi may use. Automatic keeps the chip's own country ({r})."),
            None => "Which channels Wi-Fi may use. Automatic keeps the chip's own country.".to_string(),
        };
        let line = text.fit(Face::Body, 28, &line, c.w - 2 * MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, &line);
        let (top, row) = (232, crate::type_scale::row(70, 37));
        let visible = ((c.h - 130 - top) / row).max(1) as usize;
        let first = sel.saturating_sub(visible / 2).min(list.len().saturating_sub(visible));
        let width = c.w - 2 * MARGIN;
        let current = self.saving.country.clone().unwrap_or_default();
        for (k, (code, name)) in list.iter().enumerate().skip(first).take(visible) {
            let y = top + (k - first) as i32 * row;
            let selected = k == sel;
            crate::kit::shelf_row(c, MARGIN, y, width, row - 8, selected);
            let fg = if selected { pal().ink } else { pal().chalk };
            let name = text.fit(Face::Title, 37, name, width - 300);
            text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
            let side = match (*code == current, code.is_empty()) {
                (true, _) => "In use".to_string(),
                (false, true) => self.saving.reg.clone().unwrap_or_default(),
                (false, false) => code.clone(),
            };
            text.draw_right(c, Face::Body, 24, MARGIN + width - 28, y + (row - 8) / 2 + crate::type_scale::base(10, 24), if selected { pal().label_dust } else { pal().dust }, &side);
        }
        hints(c, text, &[("A", "Use this one"), ("B", "Back"), ("L2/R2", "Letter")]);
    }
}

/// The first row of the next (`forward`) or previous letter in a list sorted by name; row 0 (Automatic) has none.
fn letter_jump(list: &[(String, String)], sel: usize, forward: bool) -> usize {
    let letter = |i: usize| list.get(i).and_then(|(code, n)| (!code.is_empty()).then(|| n.chars().next()).flatten()).map(|c| c.to_ascii_uppercase());
    let here = letter(sel);
    if forward {
        (sel + 1..list.len()).find(|&i| letter(i) != here).unwrap_or(sel)
    } else {
        // Back to the start of this letter, or of the previous one when already there.
        let start = |i: usize| (0..=i).rev().take_while(|&j| letter(j) == letter(i) && letter(j).is_some()).last().unwrap_or(i);
        let first = start(sel);
        if first < sel { first } else { first.checked_sub(1).map_or(0, start) }
    }
}

/// "Connected to Home · signal 78 % · 192.0.2.23": the network in use, how strong it is here, the address.
/// The networks of a new scan, with the ones the last scan had and this one missed kept for two more scans: a radio
/// that hears three networks, then five, then three, would make the page flip between lists. A network missing from
/// three scans in a row is gone. `missed` counts that.
fn keep_seen(old: &[Network], new: &[Network], missed: &mut HashMap<String, u8>) -> Vec<Network> {
    let mut out = new.to_vec();
    missed.retain(|ssid, _| old.iter().any(|n| n.ssid == *ssid));
    for n in new {
        missed.remove(&n.ssid);
    }
    for n in old.iter().filter(|n| !new.iter().any(|m| m.ssid == n.ssid)) {
        let count = missed.entry(n.ssid.clone()).or_insert(0);
        *count += 1;
        if *count < 3 {
            out.push(n.clone());
        } else {
            missed.remove(&n.ssid);
        }
    }
    out
}

fn connected_line(e: &Entry, ip: Option<&str>) -> String {
    let mut line = format!("Connected to {}", e.ssid);
    if let Some(s) = e.signal {
        line.push_str(&format!(" · signal {s}%"));
    }
    if let Some(ip) = ip {
        line.push_str(&format!(" · {ip}"));
    }
    line
}

/// NetworkManager is still bringing this network up: not connected yet.
fn connecting(e: &Entry) -> bool {
    !e.active && e.saved.as_ref().is_some_and(|s| s.connecting)
}

fn friendly_error(e: &str) -> String {
    eprintln!("wifi: connection failed: {e}");
    wifi::connection_error(e).into()
}

/// Four rising bars, filled up to the signal strength.
fn signal_bars(c: &mut Canvas, x: i32, base: i32, signal: u8, color: u32) {
    let filled = match signal {
        0..=24 => 1,
        25..=49 => 2,
        50..=74 => 3,
        _ => 4,
    };
    for i in 0..4 {
        let h = 8 + i * 7;
        let shade = if i < filled { color } else { pal().dust };
        c.rect(x + i * 14, base - h, 9, h, shade);
    }
}

/// The look step's hint bar. Settings → Look: Motion on R1, X for the shelf's systems when there are any. The first
/// start: the text size on R1, and B, which goes back to the welcome (the first-day run, 2026-10-26, found it unnamed).
fn look_hints(settings: bool, has_systems: bool) -> Vec<(&'static str, &'static str)> {
    // The first start's bar is full with the Animations key: Left/Right says what it does by being there.
    let mut h = vec![("Left/Right", if settings { "Change" } else { "" }), ("Y", "Appearance")];
    if settings {
        h.extend([("R1", "Motion"), ("A", "Keep this look"), ("R2", "Theme")]);
        if has_systems {
            h.push(("X", "Systems on the shelf"));
        }
    } else {
        h.extend([("R1", "Text size"), ("A", "Keep this look"), ("B", "Back")]);
    }
    // Last: on a narrow bar the labels go last first, and the ones that keep the page moving stay.
    h.push(("L1", "Animations"));
    h
}

/// The last step's first line: the games already on the card, or where they will show up (the first-day run,
/// 2026-10-26, had 16 games on the shelf and was told they would show up once copied).
fn done_line(games: usize) -> String {
    match games {
        0 => "Games show up on the shelf once they're copied.".into(),
        1 => "Your game is on the shelf.".into(),
        n => format!("Your {n} games are on the shelf."),
    }
}

/// Lines of Body 28 in Dust from the baseline `y` on, 40 px apart, each wrapped to the page's width (larger text, a
/// long translation); returns the last baseline.
fn paragraphs(c: &mut Canvas, text: &mut Text, lines: &[&str], mut y: i32) -> i32 {
    let step = crate::type_scale::line(28, 12);
    let mut last = y;
    for line in lines {
        for part in text.wrap(Face::Body, 28, line, c.w - 2 * MARGIN) {
            text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &part);
            last = y;
            y += step;
        }
    }
    last
}

fn paint_keyboard(c: &mut Canvas, text: &mut Text, kb: &Keyboard) {
    paint_keyboard_with(c, text, kb, "Connect");
}

/// The keyboard with its last key named `done` (Connect for a password, Search for a time zone).
pub fn paint_keyboard_with(c: &mut Canvas, text: &mut Text, kb: &Keyboard, done: &str) {
    paint_keyboard_shown(c, text, kb, done, &kb.text);
}

/// `paint_keyboard_with`, its field showing `shown` instead of the text (a sign-in page's password as dots,
/// `portal_form`).
pub fn paint_keyboard_shown(c: &mut Canvas, text: &mut Text, kb: &Keyboard, done: &str, shown: &str) {
    let width = c.w - 2 * MARGIN;
    // Text field with a caret; the Wi-Fi password is shown, it is typed on a handheld nobody else reads.
    c.rect(MARGIN, 150, width, 84, pal().label);
    // What is typed is the person's own: painted as it is, never through the translation table.
    text.set_translate(false);
    let shown = text.fit(Face::Title, 50, shown, width - 80);
    let w = text.draw(c, Face::Title, 50, MARGIN + 28, 210, pal().ink, &shown);
    text.set_translate(true);
    c.rect(MARGIN + 32 + w, 166, 4, 52, pal().ink);

    let (gap, key_h, top) = (10, 82, 290);
    for (r, row) in kb.rows().iter().enumerate() {
        let y = top + r as i32 * (key_h + gap);
        // Character rows share one key width; the special row gets wider keys.
        let widths: Vec<i32> = if r == 4 {
            vec![170, 170, 380, 170, 170]
        } else {
            vec![(width - gap * 9) / 10; row.len()]
        };
        let total: i32 = widths.iter().sum::<i32>() + gap * (widths.len() as i32 - 1);
        let mut x = (c.w - total) / 2;
        for (col, key) in row.iter().enumerate() {
            let selected = r == kb.row && col == kb.col;
            let active = matches!(key, Key::Special(Special::Shift)) && kb.shift
                || matches!(key, Key::Special(Special::Symbols)) && kb.symbols;
            let bg = if selected { pal().label } else if active { pal().pick(0x3a3a40, 0xd2ccbf) } else { pal().spine };
            c.rect(x, y, widths[col], key_h, bg);
            let label = match key {
                Key::Char(' ') => "space".to_string(),
                Key::Char(ch) => ch.to_string(),
                Key::Special(Special::Shift) => "Shift".into(),
                Key::Special(Special::Symbols) => if kb.symbols { "abc".into() } else { "?123".into() },
                Key::Special(Special::Space) => "Space".into(),
                Key::Special(Special::Delete) => "Delete".into(),
                Key::Special(Special::Done) => done.into(),
            };
            let (face, px) = if matches!(key, Key::Char(_)) { (Face::Title, 37) } else { (Face::Body, 24) };
            let lw = text.width(face, px, &label);
            text.draw(c, face, px, x + (widths[col] - lw) / 2, y + key_h / 2 + 12, if selected { pal().ink } else { pal().chalk }, &label);
            x += widths[col] + gap;
        }
    }
}

/// Button hints along the bottom edge, as on the shelf.
/// User and password for a login typed on a computer; file sharing and remote access look the same.
fn paint_login(c: &mut Canvas, text: &mut Text, y: i32, user: &str, password: Option<&str>) {
    let label_w = text.width(Face::Body, 28, "Password") + 24;
    text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, "User");
    text.draw(c, Face::Title, 37, MARGIN + label_w, y, pal().chalk, user);
    text.draw(c, Face::Body, 28, MARGIN, y + 50, pal().dust, "Password");
    text.draw(c, Face::Title, 37, MARGIN + label_w, y + 50, pal().chalk, password.unwrap_or("(not set yet)"));
}

/// A row of the look step: `label` in Dust, the choices on a keycap track with the one in use on a paper pill,
/// and what it means after them. `label_w`: room for the label (0: as wide as it is).
#[allow(clippy::too_many_arguments)]
fn pill_row(c: &mut Canvas, text: &mut Text, top: i32, label: &str, names: &[&str], on: usize, blurb: &str, label_w: i32) {
    // Two rows (the look step) are a little slimmer than the welcome step's one.
    let (h, px) = (if label_w > 0 { pill_h() } else { crate::type_scale::row(44, 24) }, 24u32);
    let base = top + h / 2 + crate::type_scale::mid(px);
    let mut x = MARGIN + text.draw(c, Face::Body, px, MARGIN, base, pal().dust, label).max(label_w) + 20;
    let widths: Vec<i32> = names.iter().map(|n| text.width(Face::Body, px, n) + 40).collect();
    c.round_rect(x, top, widths.iter().sum::<i32>() + 8, h, h / 2, pal().keycap);
    x += 4;
    for (i, (n, w)) in names.iter().zip(&widths).enumerate() {
        if i == on {
            c.round_rect(x, top + 4, *w, h - 8, (h - 8) / 2, pal().label);
        }
        text.draw(c, Face::Body, px, x + 20, base, if i == on { pal().ink } else { pal().chalk }, n);
        x += w;
    }
    let note = text.fit(Face::Body, px, blurb, c.w - MARGIN - x - 28);
    text.draw(c, Face::Body, px, x + 28, base, pal().dust, &note);
}

/// The choices on a keycap track with the one in use on a paper pill, from (`x`, `top`): `pill_row` without a label and a
/// blurb. Returns where the track ends.
fn pill_track(c: &mut Canvas, text: &mut Text, x: i32, top: i32, names: &[&str], on: usize) -> i32 {
    let (h, px) = (pill_h(), 24u32);
    let base = top + h / 2 + crate::type_scale::mid(px);
    let widths: Vec<i32> = names.iter().map(|n| text.width(Face::Body, px, n) + 40).collect();
    let end = x + widths.iter().sum::<i32>() + 8;
    c.round_rect(x, top, end - x, h, h / 2, pal().keycap);
    let mut x = x + 4;
    for (i, (n, w)) in names.iter().zip(&widths).enumerate() {
        if i == on {
            c.round_rect(x, top + 4, *w, h - 8, (h - 8) / 2, pal().label);
        }
        text.draw(c, Face::Body, px, x + 20, base, if i == on { pal().ink } else { pal().chalk }, n);
        x += w;
    }
    end
}

/// The Extras page's line about the picture downloads: where from, what is sent, how much room they take (the sizes of
/// docs/user/extras.md), and what Off does.
const PICTURES_NOTE: &str = "Pictures come from the free libretro collection: no account is needed, and only a game's name is sent. Box art is about 450 KB a game; screenshots and title screens add 20 to 400 KB. Off downloads nothing by itself.";

/// The height of a slim row of pills (the look step's two), 40 px around Body 24.
fn pill_h() -> i32 {
    crate::type_scale::row(40, 24)
}

/// The hint bar of the setup pages; returns where each pill is, for touch (pills of combined keys are left out).
/// The hint bar of the Wi-Fi page. `wizard`: the first start, where START walks on; in Settings, B (and START) leave.
fn wifi_hints(a: &'static str, saved: bool, forgetting: bool, wizard: bool) -> Vec<(&'static str, &'static str)> {
    let mut h = vec![("A", a), ("X", "Scan again")];
    if saved {
        h.push(("Y", if forgetting { "Forget now" } else { "Forget" }));
    }
    if wizard {
        h.push(("START", "Next"));
    }
    h.push(("B", "Back"));
    h
}

/// The hint bar of the Extras step: A only where it does something. The downloads and the browser need Wi-Fi, the
/// pictures switch does not (`usable`).
fn extras_hints(action: &'static str, usable: bool) -> Vec<(&'static str, &'static str)> {
    let mut h = Vec::new();
    if usable {
        h.push(("A", action));
    }
    h.push(("B", "Back"));
    h.push(("START", "Next"));
    h
}

/// The hint bar of Add your games. `more`: the "All systems" tile is selected, and whether its list is open. X (file
/// sharing) is left out without an address and with sharing off: it does nothing then (`switch_sharing`).
fn games_hints(more: Option<bool>, sharing: bool, online: bool) -> Vec<(&'static str, &'static str)> {
    let mut h = Vec::new();
    if let Some(open) = more {
        h.push(("A", if open { "Fewer systems" } else { "All systems" }));
    }
    if sharing || online {
        h.push(("X", if sharing { "Turn off file sharing" } else { "Turn on file sharing" }));
    }
    h.push(("START", "Next"));
    h.push(("B", "Back"));
    h
}

/// The hint keys at the bottom left, the same as on the shelf (`kit::page_hints`).
pub fn hints(c: &mut Canvas, text: &mut Text, items: &[(&str, &str)]) -> Vec<(crate::touch::Area, Button)> {
    crate::kit::page_hints(c, text, items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_ctl::{MockHw, MockState};
    use crate::testutil::TestDir;

    const IW_AT: &str = "global\ncountry 00: DFS-UNSET\n\nphy#0 (self-managed)\ncountry AT: DFS-ETSI\n\t(2402 - 2472 @ 40), (N/A, 20), (N/A)\n";
    const IW_US: &str = "global\ncountry US: DFS-FCC\n\t(2402 - 2472 @ 40), (N/A, 30), (N/A)\n";

    /// A zoneinfo folder with a few zones and the two tables (`testdata/zoneinfo`; empty zone files are enough for the
    /// lists). Read-only, so every test shares it.
    fn zoneinfo() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/zoneinfo")
    }

    /// A setup on a mock device whose zone and Wi-Fi regulatory domain are as given.
    fn setup(name: &str, zone: &str, reg: Option<&str>) -> (Setup, Arc<MockHw>) {
        let hw = Arc::new(mock(name, zone, reg));
        (setup_on(hw.clone(), name), hw)
    }

    #[test]
    fn the_extras_counter_says_what_it_counts() {
        assert_eq!(extras_status(None, false, true, 6, ""), "6 games");
        assert_eq!(extras_status(Some((2, 12, 0)), true, true, 6, "…"), "Looking up 2 of 12 pictures…", "pictures: up to three a game, so not '6 games'");
        assert_eq!(extras_status(Some((2, 6, 0)), true, false, 6, ""), "Looking up 2 of 6 covers");
        assert_eq!(extras_status(Some((6, 6, 4)), false, false, 6, ""), "Found 4 new covers");
    }

    /// The image's default zone nobody chose (New York), and the Wi-Fi region says Austria.
    #[test]
    fn an_unchosen_default_zone_is_never_called_already_set() {
        let hw = MockHw::new(MockState {
            zone: "America/New_York".into(),
            zone_unchosen: true,
            regdomain: Some("global\ncountry AT: DFS-ETSI\n".into()),
            ..MockState::default()
        })
        .with_zoneinfo(zoneinfo());
        let z = look_zone(&hw);
        assert!(!matches!(z.guess, Guess::System(_)), "New York is the default, not a choice: {:?}", z.guess);
        assert_eq!(z.guess.zone(), Some("Europe/Vienna"), "the Wi-Fi region decides");
        // Without a region either, nothing is claimed: no zone is pre-picked from the default.
        let hw = MockHw::new(MockState { zone: "America/New_York".into(), zone_unchosen: true, ..MockState::default() }).with_zoneinfo(zoneinfo());
        assert!(matches!(look_zone(&hw).guess, Guess::Utc));
        // A zone somebody chose stays "already set".
        let hw = MockHw::new(MockState { zone: "Europe/Berlin".into(), ..MockState::default() }).with_zoneinfo(zoneinfo());
        assert!(matches!(look_zone(&hw).guess, Guess::System(_)));
    }

    fn mock(_name: &str, zone: &str, reg: Option<&str>) -> MockHw {
        MockHw::new(MockState { zone: zone.into(), regdomain: reg.map(String::from), ..MockState::default() }).with_zoneinfo(zoneinfo())
    }

    /// A setup on the given system controls.
    fn setup_on(hw: SharedHw, name: &str) -> Setup {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        // Only the answers `Setup::settle` waits for; the rest (file sharing, remote access) is dropped as before.
        let notify: Notify = Arc::new(move |e| {
            if matches!(e, SetupEvent::Zone(_) | SetupEvent::ZoneOffsets(_) | SetupEvent::GameCounts(_) | SetupEvent::Net) {
                let _ = tx.lock().unwrap().send(e);
            }
        });
        let mut s = Setup::new(
            notify,
            vec![("Dusk", "dark"), ("Dawn", "light")],
            0,
            std::env::temp_dir().join(format!("pocketui-setup-data-{name}")),
            Vec::new(),
            hw,
        );
        s.inbox = Some(rx);
        s
    }

    /// Presses the buttons one after the other, taking the threads' answers after each (the step opens at once and
    /// fills in later).
    fn press(s: &mut Setup, buttons: &[Button]) {
        for b in buttons {
            s.press(*b);
            s.settle();
        }
    }

    /// First start up to the Wi-Fi step (the profiles skipped with START).
    fn at_wifi(s: &mut Setup) {
        press(s, &[Button::East, Button::East, Button::Start]);
        assert_eq!(s.at(), "wifi");
    }

    #[test]
    fn the_last_step_counts_the_games_already_there() {
        assert_eq!(done_line(0), "Games show up on the shelf once they're copied.");
        assert_eq!(done_line(1), "Your game is on the shelf.");
        assert_eq!(done_line(16), "Your 16 games are on the shelf.");
        let de = crate::i18n::german();
        assert_eq!(de.translate(&done_line(16)).as_deref(), Some("Deine 16 Spiele sind im Regal."));
        assert_eq!(de.translate(&done_line(1)).as_deref(), Some("Dein Spiel ist im Regal."));
    }

    /// "Step n of 7" counts the first start's pages in order, and only there (Settings pages have no counter).
    fn net(ssid: &str) -> Network {
        Network { ssid: ssid.into(), signal: 50, secure: true, active: false }
    }

    /// A radio that hears three networks, then five, then three must not make the list jump between 3 and 5.
    #[test]
    fn the_network_list_keeps_what_one_scan_missed() {
        let mut missed = HashMap::new();
        let five = vec![net("a"), net("b"), net("c"), net("d"), net("e")];
        let three = vec![net("a"), net("b"), net("c")];
        let names = |l: &[Network]| l.iter().map(|n| n.ssid.clone()).collect::<Vec<_>>().join("");
        let l = keep_seen(&[], &five, &mut missed);
        assert_eq!(names(&l), "abcde");
        let l = keep_seen(&l, &three, &mut missed);
        assert_eq!(names(&l), "abcde", "d and e missed once: still listed");
        let l = keep_seen(&l, &three, &mut missed);
        assert_eq!(names(&l), "abcde", "missed twice");
        let l = keep_seen(&l, &three, &mut missed);
        assert_eq!(names(&l), "abc", "missed three scans in a row: gone");
        // Heard again in between: the count starts over.
        let l = keep_seen(&five, &three, &mut missed);
        let l = keep_seen(&l, &five, &mut missed);
        let l = keep_seen(&l, &three, &mut missed);
        let l = keep_seen(&l, &three, &mut missed);
        assert_eq!(names(&l), "abcde");
    }

    #[test]
    fn a_on_the_connected_network_says_so_on_the_status_line() {
        let mut s = wifi_setup("wifi-confirm");
        let first = s.entries.iter().position(|e| e.active).expect("a connected network in the demo");
        s.sel = s.lead() + first;
        press(&mut s, &[Button::East]);
        let (line, color) = s.wifi_status("");
        assert!(line.starts_with("Connected to "), "{line}");
        assert_eq!(color, pal().chalk);
        // Any other press takes it back.
        press(&mut s, &[Button::Down]);
        assert!(!s.wifi_status("").0.starts_with("Connected to Home. Press"));
        // Opened from Settings, the way on is B.
        let mut s = wifi_setup("wifi-confirm-settings").only(Only::Wifi);
        s.sel = s.lead() + s.entries.iter().position(|e| e.active).unwrap();
        press(&mut s, &[Button::East]);
        assert!(s.wifi_status("").0.ends_with("Press B to go back."));
    }

    #[test]
    fn the_first_start_counts_its_steps() {
        let (mut s, _hw) = setup("count", "UTC", Some(IW_AT));
        let mut seen = vec![(s.at(), step_number(&s.step))];
        press(&mut s, &[Button::East, Button::East]);
        seen.push((s.at(), step_number(&s.step)));
        press(&mut s, &[Button::Start, Button::Start, Button::Start]);
        seen.push((s.at(), step_number(&s.step)));
        press(&mut s, &[Button::Start]);
        seen.push((s.at(), step_number(&s.step)));
        let want = [("welcome", Some(1)), ("how you like to play", Some(3)), ("extras", Some(6)), ("games", Some(7))];
        assert_eq!(seen, want.map(|(a, n)| (a.to_string(), n)));
        assert_eq!(step_number(&Step::Look), Some(2));
        assert_eq!(step_number(&Step::Wifi), Some(4));
        assert_eq!(step_number(&Step::Done), None, "the end is not a step");
        assert!(FIRST_STEPS == 7);
        assert_eq!(shelf_setup("count-settings").only(Only::Wifi).only, Some(Only::Wifi));
    }

    #[test]
    fn the_first_start_resumes_where_a_restart_found_it() {
        let name = "resume";
        let dir = std::env::temp_dir().join(format!("pocketui-setup-data-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut s, hw) = setup(name, "UTC", Some(IW_AT));
        // The welcome step has nothing to lose; a fresh start begins there.
        assert_eq!(s.at(), "welcome");
        assert!(!dir.join(RESUME_FILE).exists());
        at_wifi(&mut s);
        assert_eq!(std::fs::read_to_string(dir.join(RESUME_FILE)).unwrap(), "wifi");
        // The restart: a new setup on the same data folder opens on Wi-Fi, not the welcome.
        let s2 = setup_on(hw.clone(), name).resume();
        assert_eq!(s2.at(), "wifi");
        // Further on: the zone step, then the games step.
        press(&mut s, &[Button::Start]);
        assert_eq!(setup_on(hw.clone(), name).resume().at(), "zone");
        press(&mut s, &[Button::Start, Button::Start]);
        assert_eq!(s.at(), "games");
        assert_eq!(setup_on(hw.clone(), name).resume().at(), "games");
        // Settings pages leave the first start's line alone; a line nobody knows means the beginning.
        let _ = shelf_setup("resume-settings").only(Only::Extras);
        std::fs::write(dir.join(RESUME_FILE), "nonsense").unwrap();
        assert_eq!(setup_on(hw.clone(), name).resume().at(), "welcome");
        std::fs::write(dir.join(RESUME_FILE), "games").unwrap();
        // Finishing deletes it: the next first start (a reset) begins at the welcome.
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "done");
        assert!(s.press(Button::East), "the last page ends the setup");
        assert!(!dir.join(RESUME_FILE).exists());
        assert_eq!(setup_on(hw, name).resume().at(), "welcome");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_first_start_downloads_nothing_before_the_extras_step_is_left() {
        let (mut s, _hw) = setup("art-first", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        assert!(!s.past_wifi(), "Wi-Fi");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone");
        assert!(!s.past_wifi(), "the zone step: the Pictures choice has not been seen yet");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "extras");
        assert!(!s.past_wifi(), "on the Extras step the downloads still wait, so Off can come first");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "games");
        assert!(s.past_wifi(), "left: the downloads may start, as chosen");
        // The page opened from Settings has no such wait: a change there acts at once.
        let s = shelf_setup("art-settings").only(Only::Extras);
        assert!(s.past_wifi());
    }

    #[test]
    fn pictures_is_one_row_of_three_choices_that_works_without_wifi() {
        let mut s = shelf_setup("art-row").only(Only::Extras);
        assert_eq!(s.take_art(), None, "nothing changed yet");
        press(&mut s, &[Button::Down]);
        // Box art · Box art and screenshots (the start setting) · Off, A and Right go on, Left back.
        press(&mut s, &[Button::East]);
        assert_eq!(s.take_art(), Some(Art::Off));
        press(&mut s, &[Button::Right]);
        assert_eq!(s.take_art(), Some(Art::BoxArt), "wraps round");
        press(&mut s, &[Button::Left]);
        assert_eq!(s.take_art(), Some(Art::Off));
        press(&mut s, &[Button::Left]);
        assert_eq!(s.take_art(), Some(Art::Pictures), "one change, once");
        assert_eq!(s.take_art(), None);
        // The saved choice is shown when the page opens.
        let mut s = shelf_setup("art-saved").only(Only::Extras).with_art(Art::Off);
        press(&mut s, &[Button::Down, Button::East]);
        assert_eq!(s.take_art(), Some(Art::BoxArt), "from Off, the next choice");
    }

    #[test]
    fn the_extras_page_shows_the_choice_made_and_a_note_that_fits() {
        let paint = |art: Art| {
            let mut s = shelf_setup("art-paint").only(Only::Extras).with_art(art);
            let mut px = vec![0u32; 1240 * 1080];
            s.paint(&mut Canvas::new(&mut px, 1240, 1080), &mut Text::builtin());
            px
        };
        let (pictures, off) = (paint(Art::Pictures), paint(Art::Off));
        assert!(pictures.iter().any(|p| *p != pictures[0]), "something is painted");
        assert_ne!(pictures, off, "the pill moves with the choice");
        // Where the pictures come from: a few lines under the rows, never more than the page has room for.
        let lines = Text::builtin().wrap(Face::Body, 24, PICTURES_NOTE, 1240 - 2 * MARGIN);
        assert!((1..=4).contains(&lines.len()), "{} lines", lines.len());
        assert!(PICTURES_NOTE.contains("libretro") && PICTURES_NOTE.contains("no account") && PICTURES_NOTE.contains("Off"));
    }

    #[test]
    fn the_time_zone_step_sits_between_wifi_and_extras() {
        let (mut s, _hw) = setup("flow", "UTC", Some(IW_AT));
        assert_eq!(s.at(), "welcome");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "look");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "how you like to play", "after the look");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "wifi");
        assert!(s.take_tour().is_none(), "skipped: nothing changed");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone", "after Wi-Fi");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "extras", "before Extras");
        // Back goes the same way.
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "how you like to play");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "look");
    }

    #[test]
    fn a_profile_sets_the_switches_once_and_goes_on_to_wifi() {
        use crate::features::Switches;
        for (downs, lens, pixel_path) in [(0, true, false), (1, true, true)] {
            let (s, _hw) = setup(&format!("profile{downs}"), "UTC", Some(IW_AT));
            let mut s = s.with_switches(Switches::default(), Vec::new());
            press(&mut s, &[Button::East, Button::East]);
            for _ in 0..downs {
                press(&mut s, &[Button::Down]);
            }
            press(&mut s, &[Button::East]);
            assert_eq!(s.at(), "wifi");
            let (sw, later) = s.take_tour().expect("the profile's switches");
            assert_eq!((sw.curator.lens, sw.curator.pixel_path, later.len()), (lens, pixel_path, 0));
            assert_eq!(s.motion(), sw.motion, "the look step's motion follows");
            assert!(s.take_tour().is_none(), "once");
        }
    }

    #[test]
    fn simple_and_skip_leave_a_new_device_exactly_as_it_starts() {
        use crate::features::Switches;
        let (s, _hw) = setup("simple-is-skip", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new());
        // Simple, the first row: A.
        press(&mut s, &[Button::East, Button::East, Button::East]);
        assert_eq!(s.at(), "wifi");
        let (sw, later) = s.take_tour().expect("a profile was taken");
        assert_eq!(sw, Switches::default(), "Simple sets nothing that differs from the start settings");
        assert!(later.is_empty());
        assert!(sw.curator.lens && sw.curator.stickers && !sw.curator.up_next, "Lens and Stickers on, Up next off");
        // Skip (START) changes nothing at all, so both end in the same place.
        let (s, _hw) = setup("simple-is-skip-2", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new());
        press(&mut s, &[Button::East, Button::East, Button::Start]);
        assert!(s.take_tour().is_none(), "skipped: nothing changed, which is what Simple leaves too");
    }

    #[test]
    fn choose_yourself_opens_the_cards_and_b_on_the_first_goes_back_to_it() {
        use crate::features::Switches;
        let (s, _hw) = setup("choose", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new());
        press(&mut s, &[Button::East, Button::East, Button::Down, Button::Down, Button::East]);
        assert_eq!(s.at(), "feature cards");
        assert!(!s.past_wifi(), "no covers before the Wi-Fi step");
        // The first card (Lens & smart shelves, on): Off, A.
        press(&mut s, &[Button::Down, Button::East]);
        let (sw, _) = s.take_tour().expect("a switch changed");
        assert!(!sw.curator.lens);
        // The second card: Later.
        press(&mut s, &[Button::Down, Button::Down, Button::East]);
        assert_eq!(s.take_tour().map(|(_, l)| l), Some(vec!["echoes".to_string()]));
        // Back to the first card, then out of the cards to the profiles, Choose yourself selected.
        press(&mut s, &[Button::South, Button::South]);
        assert_eq!(s.at(), "feature cards");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "how you like to play");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "feature cards", "Choose yourself again");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "wifi", "START skips the rest");
    }

    #[test]
    fn settings_look_still_ends_on_a_without_the_profiles() {
        let mut s = shelf_setup("look-only").only(Only::Look);
        assert!(s.press(Button::East), "Keep this look closes the page");
    }

    #[test]
    fn a_zone_the_system_already_has_is_left_alone() {
        let (mut s, hw) = setup("system", "America/New_York", Some(IW_AT));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "America/New_York", "the Wi-Fi country does not override a set zone");
    }

    #[test]
    fn one_press_takes_the_guess_and_skipping_keeps_it() {
        // A confirms ...
        let (mut s, hw) = setup("confirm", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start, Button::East]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "Europe/Vienna");
        // ... and so does START.
        let (mut s, hw) = setup("skip", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start, Button::Start]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "Europe/Vienna");
    }

    #[test]
    fn without_a_guess_utc_stays() {
        let (mut s, hw) = setup("none", "UTC", None);
        at_wifi(&mut s);
        press(&mut s, &[Button::Start, Button::East]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "UTC");
    }

    #[test]
    fn a_country_with_several_zones_opens_its_list() {
        let (mut s, hw) = setup("several", "UTC", Some(IW_US));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone: United States", "the picker, filtered to the country");
        // Skipping keeps what there is: nothing was guessed.
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "UTC");
        // Choosing: Chicago is the first by city.
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone: United States");
        press(&mut s, &[Button::Down, Button::Down, Button::East]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "America/New_York");
    }

    #[test]
    fn x_changes_the_guess_by_region() {
        let (mut s, hw) = setup("change", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start, Button::North]);
        assert_eq!(s.at(), "zone: regions");
        // Opens on the guess's region (Europe), and in it on the guessed city (Vienna, after Berlin).
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: Europe");
        press(&mut s, &[Button::Up, Button::East]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "Europe/Berlin");
    }

    #[test]
    fn b_steps_back_out_of_a_list_and_out_of_the_regions() {
        let (mut s, hw) = setup("change-back", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start, Button::North, Button::Up, Button::East]);
        assert_eq!(s.at(), "zone: America");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone: regions");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone");
        press(&mut s, &[Button::East]);
        assert_eq!(hw.state().zone, "Europe/Vienna", "the guess is still the guess");
    }

    #[test]
    fn search_finds_a_city() {
        let (mut s, hw) = setup("search", "UTC", None);
        at_wifi(&mut s);
        press(&mut s, &[Button::Start, Button::North]);
        assert_eq!(s.at(), "zone: regions");
        press(&mut s, &[Button::Up; 10]); // "Search…" is the first row
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: search");
        s.type_search("vie");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone: Search: vie");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "extras");
        assert_eq!(hw.state().zone, "Europe/Vienna");
    }

    fn wifi_setup(name: &str) -> Setup {
        setup(name, "UTC", Some(IW_AT)).0.only(Only::Wifi).with_wifi_demo()
    }

    fn wifi_row(s: &Setup, ssid: &str) -> usize {
        s.entries.iter().position(|e| e.ssid == ssid).unwrap()
    }

    fn check(net: net_check::Net) -> SetupEvent {
        let portal = (net == net_check::Net::Portal).then(|| "http://login.hotel.example/start".to_string());
        SetupEvent::Network(net_check::Check { net, portal })
    }

    #[test]
    fn a_network_with_a_sign_in_page_is_said_and_forgotten_when_the_wifi_goes() {
        let mut s = wifi_setup("wifi-portal");
        assert!(!s.portal, "nothing is said before the probe answered");
        s.on_event(check(net_check::Net::Online));
        assert!(!s.portal);
        s.on_event(check(net_check::Net::Portal));
        assert!(s.portal, "the Wi-Fi page says it");
        assert_eq!(s.portal_at.as_deref(), Some("http://login.hotel.example/start"));
        // An answer that says nothing leaves it.
        s.on_event(check(net_check::Net::Unknown));
        assert!(s.portal);
        // Offline is not a sign-in page: the plain page.
        s.on_event(check(net_check::Net::Unreachable));
        assert!(!s.portal);
        s.on_event(check(net_check::Net::Portal));
        // The address is gone (Wi-Fi off, another network): nothing to sign in to.
        let mut snap = s.net.snapshot();
        snap.saving.ip = None;
        s.apply_snapshot(&snap, false);
        assert!(!s.portal);
    }

    #[test]
    fn the_sign_in_is_the_first_row_and_opens_its_sheet() {
        let mut s = wifi_setup("wifi-portal-row");
        assert_eq!(s.wifi_rows(), 7);
        s.on_event(check(net_check::Net::Portal));
        assert_eq!(s.wifi_rows(), 8, "the sign-in row before the networks");
        assert_eq!(s.sel, 0, "selected: A opens it");
        // The networks keep their rows below it.
        s.sel = 1 + wifi_row(&s, "Cafe guest");
        press(&mut s, &[Button::Up]);
        assert_eq!(s.sel, 1, "the connected network");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi", "A on the connected network stays on the page");
        let (line, _) = s.wifi_status("");
        assert!(line.starts_with("Connected to "), "{line}");
        press(&mut s, &[Button::Up, Button::East]);
        assert_eq!(s.at(), "wifi: sign-in");
        let Step::Portal(page) = &s.step else { unreachable!() };
        assert_eq!(page.where_line(), "Sign-in page: login.hotel.example");
        assert_eq!(page.mac.as_deref(), Some("00:00:5E:00:53:7F"), "the address for the front desk");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi", "B goes back to the list");
        // Online again: the row goes and the selection stays on the network it was on.
        s.sel = 2;
        let on = s.entries[1].ssid.clone();
        s.on_event(check(net_check::Net::Online));
        assert_eq!(s.wifi_rows(), 7);
        assert_eq!(s.entries[s.sel].ssid, on);
        assert!(s.note.as_ref().is_some_and(|(m, _)| m.starts_with("You're online now")), "said, without a press");
    }

    #[test]
    fn a_network_without_its_wifi_address_says_so() {
        let mut s = wifi_setup("wifi-portal-nomac");
        s.saving.mac = None;
        s.on_event(check(net_check::Net::Portal));
        press(&mut s, &[Button::East]);
        let Step::Portal(page) = &s.step else { panic!("{}", s.at()) };
        assert_eq!(page.mac, None, "the sheet says it couldn't be read (painted in red, `portal_page`)");
    }

    #[test]
    fn sign_in_loads_on_a_thread_shows_what_came_and_drops_its_session_when_left() {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let mut s = wifi_setup("wifi-portal-signin");
        s.notify = Arc::new(move |e| {
            if matches!(e, SetupEvent::Portal { .. }) {
                let _ = tx.lock().unwrap().send(e);
            }
        });
        // A curl that isn't there: the page must still end with something to read.
        s.portal_io.curl = "/nonexistent/curl".into();
        s.on_event(check(net_check::Net::Portal));
        press(&mut s, &[Button::East, Button::East]);
        assert_eq!(s.at(), "wifi: sign-in form");
        assert!(s.portal_session.is_some(), "the session lives while the page is open");
        assert!(s.animating(), "loading: the dots move");
        let e = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the thread answers");
        s.on_event(e);
        let Step::Portal(page) = &s.step else { unreachable!() };
        assert!(matches!(page.signin.as_ref().map(|x| &x.state), Some(crate::portal_form::State::Ended(crate::portal_form::End::Failed(_)))));
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi: sign-in");
        assert!(s.portal_session.is_none(), "left: the session and its cookie jar are gone");
    }

    static ASKED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static ONLINE_AFTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);

    fn fake_recheck() -> net_check::Check {
        use std::sync::atomic::Ordering;
        let n = ASKED.fetch_add(1, Ordering::SeqCst) + 1;
        let net = if n >= ONLINE_AFTER.load(Ordering::SeqCst) { net_check::Net::Online } else { net_check::Net::Portal };
        net_check::Check { net, portal: (net == net_check::Net::Portal).then(|| "http://login.hotel.example/".into()) }
    }

    /// A setup whose notify sends the re-check's answers to the test, re-checking every few milliseconds.
    fn watched(name: &str) -> (Setup, std::sync::mpsc::Receiver<SetupEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let notify: Notify = Arc::new(move |e| {
            if matches!(e, SetupEvent::Network(_)) {
                let _ = tx.lock().unwrap().send(e);
            }
        });
        let mut s = wifi_setup(name);
        s.notify = notify;
        s.recheck = Recheck { ask: fake_recheck, every: std::time::Duration::from_millis(5) };
        (s, rx)
    }

    #[test]
    fn the_page_checks_again_by_itself_turns_online_and_stops() {
        use std::sync::atomic::Ordering;
        // One test drives the counter: the second part needs the first's numbers.
        ASKED.store(0, Ordering::SeqCst);
        ONLINE_AFTER.store(3, Ordering::SeqCst);
        let (mut s, rx) = watched("wifi-recheck");
        s.on_event(check(net_check::Net::Portal));
        assert!(s.watch.is_some(), "the page shows a sign-in: it asks again");
        let wait = std::time::Duration::from_secs(5);
        while s.portal {
            let e = rx.recv_timeout(wait).expect("the re-check answers");
            s.on_event(e);
        }
        assert_eq!(ASKED.load(Ordering::SeqCst), 3, "asked until online, without a press");
        assert!(s.watch.is_none(), "online: no more asking");
        assert!(s.note.as_ref().is_some_and(|(m, _)| m.starts_with("You're online now")));
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(ASKED.load(Ordering::SeqCst), 3, "the thread has ended");

        // The page closed (another step, or the setup dropped with Settings): the asking ends with it.
        ONLINE_AFTER.store(usize::MAX, Ordering::SeqCst);
        s.on_event(check(net_check::Net::Portal));
        assert!(s.watch.is_some());
        let _ = rx.recv_timeout(wait).expect("asks again");
        s.step = Step::Remote;
        s.keep_watch();
        assert!(s.watch.is_none());
        std::thread::sleep(std::time::Duration::from_millis(250));
        let after = ASKED.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert_eq!(ASKED.load(Ordering::SeqCst), after, "nothing is asked once the page is gone");
    }

    #[test]
    fn saved_networks_are_listed_and_connect_without_their_password() {
        let mut s = wifi_setup("wifi-saved");
        let names: Vec<&str> = s.entries.iter().map(|e| e.ssid.as_str()).collect();
        assert_eq!(names, ["Home", "Cafe guest", "Neighbour:5G", "Office", "Phone hotspot"]);
        assert_eq!(s.wifi_rows(), 7, "then Other network and Wi-Fi country");
        s.sel = wifi_row(&s, "Office");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "connecting", "a saved network asks for nothing");
        // A password that no longer works brings the keyboard.
        s.on_event(SetupEvent::ConnectedSaved { ssid: "Office".into(), result: Err("Secrets were required, but not provided".into()) });
        assert_eq!(s.at(), "password");
        // A network that was never saved and has a password asks for it.
        let mut s = wifi_setup("wifi-new");
        s.sel = wifi_row(&s, "Neighbour:5G");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "password");
    }

    /// The page on `snap`'s saved networks: Home in reach, NetworkManager still bringing it up (`activating`).
    fn connecting_home(name: &str) -> Setup {
        let mut s = wifi_setup(name);
        let mut snap = s.net.snapshot();
        snap.saving.ip = None;
        for saved in &mut snap.saving.saved {
            saved.active = false;
            saved.connecting = saved.ssid == "Home";
        }
        s.net = NetState::demo(snap.clone());
        s.apply_snapshot(&snap, false);
        s
    }

    #[test]
    fn a_network_still_being_brought_up_is_connecting_not_connected() {
        // Fresh card, 2026-10-27: iwd kept trying a refused password for six minutes while the page said "Connected" and
        // the status corner said offline.
        let mut s = connecting_home("wifi-connecting");
        let home = &s.entries[wifi_row(&s, "Home")];
        assert!(!home.active, "an activation is not a connection");
        assert_eq!(s.wifi_status("…").0, "Connecting to Home…");
        assert!(s.animating(), "the dots move, and the page is painted again when it gets stuck");
        // A asks for patience at first, it does not start a second activation.
        s.sel = s.lead() + wifi_row(&s, "Home");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi");
        assert!(s.note.as_ref().is_some_and(|(m, alert)| m == "Still connecting to Home. One moment." && !alert));
        // A minute without an address: said plainly, and A types the password again.
        s.note = None;
        s.connecting_since.as_mut().unwrap().1 = Instant::now().checked_sub(CONNECT_STUCK).unwrap();
        let (line, color) = s.wifi_status("");
        assert_eq!(line, "Home isn't letting this Pocket in. Press A to type the password again.");
        assert_eq!(color, pal().alert);
        press(&mut s, &[Button::East]);
        assert!(matches!(&s.step, Step::Password { ssid, hidden: false, .. } if ssid == "Home"));
        // Activated in the end: connected, and the clock is gone.
        let mut snap = s.net.snapshot();
        snap.saving.ip = Some("192.0.2.23".into());
        snap.saving.saved.iter_mut().for_each(|saved| (saved.active, saved.connecting) = (saved.ssid == "Home", false));
        s.step = Step::Wifi;
        s.apply_snapshot(&snap, false);
        assert!(s.connecting_since.is_none());
        assert!(s.wifi_status("").0.starts_with("Connected to Home"), "{}", s.wifi_status("").0);
    }

    #[test]
    fn the_clock_of_a_stuck_activation_keeps_running_across_looks_and_restarts_for_another_network() {
        let mut s = connecting_home("wifi-connecting-clock");
        let long_ago = Instant::now().checked_sub(CONNECT_STUCK).unwrap();
        s.connecting_since.as_mut().unwrap().1 = long_ago;
        let snap = s.net.snapshot();
        let mut again = snap.clone();
        again.saving.saved.iter_mut().for_each(|saved| (saved.active, saved.connecting) = (false, saved.ssid == "Home"));
        s.apply_snapshot(&again, false);
        assert_eq!(s.connecting_since.as_ref().map(|(ssid, t)| (ssid.as_str(), *t)), Some(("Home", long_ago)), "the same activation, the same clock");
        again.saving.saved.iter_mut().for_each(|saved| saved.connecting = saved.ssid == "Office");
        s.apply_snapshot(&again, false);
        assert!(s.connecting_since.as_ref().is_some_and(|(ssid, t)| ssid == "Office" && *t > long_ago));
    }

    #[test]
    fn a_refused_typed_password_brings_the_keyboard_back_with_the_reason() {
        let mut s = wifi_setup("wifi-refused");
        s.sel = wifi_row(&s, "Neighbour:5G");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "password");
        s.connect("Neighbour:5G".into(), Some("qqqqqqqq".into()));
        s.on_event(SetupEvent::Connected {
            ssid: "Neighbour:5G".into(),
            result: Err("Error: Connection activation failed: Secrets were required, but not provided.".into()),
        });
        assert!(matches!(&s.step, Step::Password { ssid, kb, hidden: false } if ssid == "Neighbour:5G" && kb.text.is_empty()));
        assert_eq!(s.note, Some(("Wrong password. Try again.".into(), true)));
        // Another failure is not about the password: back to the list with what happened.
        s.on_event(SetupEvent::Connected { ssid: "Neighbour:5G".into(), result: Err("Error: Timeout expired (45 seconds)".into()) });
        assert_eq!(s.at(), "wifi");
        assert_eq!(s.note, Some(("The network took too long to respond. Try again.".into(), true)));
    }

    #[test]
    fn a_forgotten_network_leaves_the_list_before_the_next_scan() {
        let mut s = wifi_setup("wifi-forgotten");
        let office = s.saving.saved.iter().find(|saved| saved.ssid == "Office").cloned().unwrap();
        s.net.forgotten(&office.uuid);
        let snap = s.net.snapshot();
        s.apply_snapshot(&snap, false);
        assert!(s.entries.iter().all(|e| e.ssid != "Office"), "no second Y on a profile that is gone");
        assert_eq!(s.saving.saved.len(), wifi::demo_saved().len() - 1);
    }

    #[test]
    fn a_join_failed_for_missing_secrets_reopens_the_password_prompt() {
        use crate::wifi_activation::{Device, Ops, prepare};
        struct Joining { reads: usize }
        impl Ops for Joining {
            fn device(&mut self) -> Result<Device, String> {
                self.reads += 1;
                Ok(Device { state: if self.reads == 1 { 50 } else { 120 }, ssid: Some("Home".into()),
                    reason: "7 (Secrets were required, but not provided)".into() })
            }
            fn disconnect(&mut self) -> Result<(), String> { panic!("join must not disconnect") }
            fn now(&self) -> std::time::Duration { std::time::Duration::ZERO }
            fn wait(&mut self) {}
        }
        let error = prepare(&mut Joining { reads: 0 }, "Home").unwrap_err();
        assert_eq!(wifi::connection_error(&error), "Wrong password. Try again.");
        let mut s = wifi_setup("wifi-joined-secrets");
        s.on_event(SetupEvent::ConnectedSaved { ssid: "Home".into(), result: Err(error) });
        assert_eq!(s.at(), "password");
        assert!(matches!(&s.step, Step::Password { ssid, .. } if ssid == "Home"));
        assert!(s.note.as_ref().unwrap().0.contains("saved password didn't work"));
    }

    #[test]
    fn forgetting_takes_two_presses_and_b_keeps_the_network() {
        let mut s = wifi_setup("wifi-forget");
        s.sel = wifi_row(&s, "Office");
        press(&mut s, &[Button::West]);
        assert!(s.forget.is_some() && s.note.as_ref().is_some_and(|(n, err)| n.starts_with("Forget Office?") && *err));
        press(&mut s, &[Button::South]);
        assert_eq!((s.forget.as_deref(), s.at().as_str()), (None, "wifi"), "B only takes the question back");
        press(&mut s, &[Button::West, Button::Down]);
        assert_eq!(s.forget, None, "moving away keeps it too");
        s.sel = wifi_row(&s, "Office");
        press(&mut s, &[Button::West, Button::West]);
        assert!(s.note.as_ref().is_some_and(|(n, _)| n.starts_with("Forgetting Office")));
        // A network that isn't saved has nothing to forget.
        s.sel = wifi_row(&s, "Cafe guest");
        press(&mut s, &[Button::West]);
        assert_eq!(s.forget, None);
    }

    #[test]
    fn a_hidden_network_is_typed_by_name_and_may_be_open() {
        let mut s = wifi_setup("wifi-hidden");
        s.sel = s.entries.len();
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi: hidden");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "wifi: hidden", "no name, no next step");
        if let Step::HiddenName { kb } = &mut s.step {
            kb.text = "Attic".into();
        }
        press(&mut s, &[Button::Start]);
        assert!(matches!(&s.step, Step::Password { ssid, hidden: true, .. } if ssid == "Attic"));
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "connecting", "an empty password: an open hidden network");
    }

    use crate::net_state::fake::{failure, nets, FakeNet, Gate};
    use crate::net_state::Cause;

    /// A Wi-Fi page on a scripted system: the page's `NetState` runs where it is asked, its answers are taken at once.
    fn page_on(name: &str, fake: &Arc<FakeNet>) -> Setup {
        let mut s = setup(name, "UTC", Some(IW_AT)).0.with_net(NetState::inline(fake.clone())).only(Only::Wifi);
        s.settle();
        s
    }

    fn status(s: &Setup) -> String {
        s.wifi_status("").0
    }

    /// The RG28XX has no Wi-Fi chip: the page says so in words (never an empty list, "No networks found" or a scan that
    /// fails), offers nothing to choose, and still lets the player leave it.
    #[test]
    fn a_device_without_wifi_says_so_instead_of_an_empty_page() {
        let fake = FakeNet::new(Radio::Absent);
        let mut s = page_on("wifi-absent", &fake);
        assert_eq!(status(&s), "This device has no Wi-Fi. Everything else works without it.");
        assert_eq!(s.wifi_rows(), 0, "no rows to choose from");
        assert_eq!(fake.scans_run(), 0, "nothing was searched");
        // Nothing opens: not Other network, not the country list, not a scan.
        s.scan();
        assert!(!s.scanning);
        press(&mut s, &[Button::East, Button::Down, Button::East, Button::North]);
        assert!(matches!(s.step, Step::Wifi), "A and X do nothing here");
        // The page paints the words and the small radio, at the Mini V2's size and at the H700's.
        for (w, h) in [(1240, 1080), (640, 480)] {
            let mut px = vec![0u32; (w * h) as usize];
            s.paint(&mut Canvas::new(&mut px, w, h), &mut Text::builtin());
            assert!(px.iter().any(|p| *p != px[0]), "{w}x{h} is not blank");
        }
        // The Settings row says it too.
        assert_eq!(crate::settings_values::Wifi::Absent.label(), "None on this device");
    }

    #[test]
    fn a_radio_blocked_at_boot_shows_wifi_is_off_with_a_way_to_turn_it_on_never_no_networks() {
        // The player switched it off (the wish is off): nothing but the player turns it on.
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:*\nCafe guest:41::\n"))]);
        let mut s = page_on("wifi-blocked", &fake);
        assert_eq!(status(&s), "Wi-Fi is off. Press A to turn it on.");
        assert!(!status(&s).contains("No networks"));
        assert_eq!(s.lead_kind(), Some(Lead::Off));
        assert_eq!(s.wifi_rows(), 3, "the off row, Other network, Wi-Fi country");
        assert_eq!(fake.scans_run(), 0, "nothing was searched with a blocked radio");
        assert_eq!(wifi_hints_for(&s)[0], ("A", "Turn on"));
        // The player turns it on: the radio comes up, the wish is stored, the list is there without another press.
        press(&mut s, &[Button::East]);
        assert_eq!(status(&s), "Connected to Home · signal 78%");
        assert_eq!(s.lead_kind(), None);
        assert_eq!(s.entries.iter().map(|e| e.ssid.as_str()).collect::<Vec<_>>(), ["Home", "Cafe guest"]);
        assert_eq!(fake.script().wanted, Some(true));
        assert_eq!(fake.log()[..2], ["set_radio true", "persist true"]);
    }

    #[test]
    fn a_radio_the_boot_blocked_while_the_wish_is_on_is_on_again_when_the_page_opens() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(true);
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:*\nCafe guest:41::\n"))]);
        let s = page_on("wifi-blocked-wish-on", &fake);
        assert_eq!(status(&s), "Connected to Home · signal 78%", "the page opens on a list, not on a radio that is off");
        assert_eq!(s.lead_kind(), None);
        assert_eq!(fake.log().iter().filter(|l| *l == "set_radio true").count(), 1);
        // If it will not come on, the page says so and A tries.
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().switch_result = Err("wifictl enable exited with 1".into());
        let s = page_on("wifi-blocked-wish-on-fails", &fake);
        assert_eq!(status(&s), "Wi-Fi is off. Press A to turn it on.");
        assert_eq!(s.lead_kind(), Some(Lead::Off));
    }

    #[test]
    fn a_switch_that_fails_says_so_calmly_and_the_page_stays_truthful() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().switch_result = Err("wifictl enable exited with 1".into());
        let mut s = page_on("wifi-switch-fails", &fake);
        press(&mut s, &[Button::East]);
        assert_eq!(status(&s), "Couldn't turn Wi-Fi on. Press A to try again.");
        assert!(s.radio.is_off() && s.lead_kind() == Some(Lead::Off), "still off, still offering to turn it on");
        assert_eq!(fake.script().wanted, Some(false), "the wish was not stored");
        // Tried again, it works: the message goes.
        fake.script().switch_result = Ok(());
        press(&mut s, &[Button::East]);
        assert!(s.radio.is_on() && s.switch_failed.is_none());
        assert_eq!(status(&s), "No networks found. Press X to scan again.", "a scan that worked and saw nobody");
    }

    #[test]
    fn a_failed_scan_is_not_no_networks_found_and_x_tries_again() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = std::collections::VecDeque::from([failure(10, "No Wi-Fi device found")]);
        let mut s = page_on("wifi-scan-fails", &fake);
        assert_eq!(status(&s), "Couldn't search for networks. Try again (X).");
        assert!(s.scan_err.is_some() && s.entries.is_empty());
        // X, and this time it works but finds nobody: that is what "No networks found" is for.
        fake.script().scans = std::collections::VecDeque::from([Ok(Vec::new())]);
        press(&mut s, &[Button::North]);
        assert_eq!(status(&s), "No networks found. Press X to scan again.");
        assert!(s.scan_err.is_none());
        // And a list is a list.
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:\n"))]);
        press(&mut s, &[Button::North]);
        assert_eq!(status(&s), "Choose your network.");
    }

    #[test]
    fn a_scan_that_failed_once_after_boot_is_tried_again_by_itself() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = std::collections::VecDeque::from([failure(10, "No Wi-Fi device found"), Ok(nets("Home:78:WPA2:\n"))]);
        let s = page_on("wifi-boot-retry", &fake);
        assert_eq!(fake.scans_run(), 2);
        assert_eq!(status(&s), "Choose your network.", "the player saw no failure to deal with");
    }

    #[test]
    fn the_page_notices_a_radio_that_comes_up_by_itself_and_lists_the_networks() {
        // Opened inside the boot's blocked window; the hooks unblock the radio a few seconds later. (The wish is off here:
        // with it on, pocket-ui switches the radio on itself, see `a_radio_the_boot_blocked_…`.)
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:\n"))]);
        let mut s = page_on("wifi-comes-up", &fake);
        assert_eq!(status(&s), "Wi-Fi is off. Press A to turn it on.");
        fake.script().radio = Radio::On;
        s.net.poll();
        s.settle();
        assert_eq!(status(&s), "Choose your network.");
        assert_eq!(s.entries.len(), 1);
        assert_eq!(fake.script().wanted, Some(false), "nothing in this changed the wish");
    }

    #[test]
    fn a_stale_list_does_not_come_back_after_the_radio_went_off() {
        // A scan under way when Wi-Fi is switched off (quick settings): its answer must not show networks.
        let fake = FakeNet::new(Radio::On);
        let gate = Gate::new();
        {
            let mut f = fake.script();
            f.scan_gate = Some(gate.clone());
            f.scans = std::collections::VecDeque::from([Ok(nets("Old:50:WPA2:\n"))]);
        }
        let net = NetState::new(fake.clone());
        let mut s = setup("wifi-stale", "UTC", Some(IW_AT)).0.with_net(net.clone()).only(Only::Wifi);
        fake.wait_scan_started();
        net.toggle();
        gate.open();
        net.wait_idle();
        s.settle();
        assert_eq!(status(&s), "Wi-Fi is off. Press A to turn it on.");
        assert!(s.entries.is_empty() && s.nets.is_empty(), "{:?}", s.nets);
    }

    #[test]
    fn the_address_of_a_network_that_is_gone_is_not_kept() {
        let fake = FakeNet::new(Radio::On);
        fake.script().saving.ip = Some("192.0.2.9".into());
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:*\n"))]);
        let mut s = page_on("wifi-ip", &fake);
        assert_eq!(s.ip.as_deref(), Some("192.0.2.9"));
        fake.script().saving.ip = None;
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:\n"))]);
        press(&mut s, &[Button::North]);
        assert_eq!(s.ip, None, "the page used to keep the last address when none came");
    }

    #[test]
    fn a_connection_runs_through_the_same_worker_and_exactly_one_scan_follows_it() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:\n"))]);
        let mut s = page_on("wifi-connect-worker", &fake);
        let before = fake.scans_run();
        s.connect("Home".into(), None);
        s.settle();
        assert_eq!(fake.scans_run(), before + 1, "the list is looked at again once the connection is done, and only once");
    }

    #[test]
    fn the_answers_of_connect_forget_and_country_do_not_ask_for_a_second_scan() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:\n"))]);
        let mut s = page_on("wifi-no-double-scan", &fake);
        let before = fake.scans_run();
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        s.on_event(SetupEvent::ConnectedSaved { ssid: "Home".into(), result: Ok(()) });
        s.on_event(SetupEvent::Forgot { ssid: "Home".into(), result: Ok(()) });
        s.on_event(SetupEvent::CountrySet(Ok(None)));
        assert_eq!(fake.scans_run(), before, "the scan after such an operation is the worker's own (`NetState::exclusive`)");
    }

    fn wifi_hints_for(s: &Setup) -> Vec<(&'static str, &'static str)> {
        let row = s.sel.checked_sub(s.lead());
        let a = match row.map(|r| r.checked_sub(s.entries.len())) {
            Some(None) => "Connect",
            None if s.lead_kind() == Some(Lead::Off) => "Turn on",
            _ => "Open",
        };
        wifi_hints(a, false, false, false)
    }

    #[test]
    fn the_wifi_country_list_starts_with_automatic_and_jumps_by_letter() {
        let mut s = wifi_setup("wifi-country");
        s.sel = s.entries.len() + 1;
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi: country");
        let Step::Country { list, sel } = &s.step else { panic!() };
        assert_eq!((list[0].0.as_str(), list[0].1.as_str(), *sel), ("", "Automatic", 0));
        let list = list.clone();
        let jumped = letter_jump(&list, 1, true);
        assert_ne!(list[jumped].1.chars().next(), list[1].1.chars().next());
        assert_eq!(letter_jump(&list, jumped, false), 1, "back to the first of the previous letter");
        assert_eq!(letter_jump(&list, 0, false), 0);
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi");
        s.on_event(SetupEvent::CountrySet(Ok(Some("AT".into()))));
        assert_eq!(s.saving.country.as_deref(), Some("AT"));
        assert_eq!(s.country_value(), "Austria");
    }

    fn shelf_setup(name: &str) -> Setup {
        let systems = [("gba", "Game Boy Advance", 3), ("ports", "Ports", 2), ("snes", "Super Nintendo", 1)]
            .into_iter()
            .map(|(id, name, games)| ShelfSystem { id: id.into(), name: name.into(), games })
            .collect();
        setup(name, "UTC", None).0.with_shelf(systems, Vec::new())
    }

    #[test]
    fn settings_look_has_systems_on_the_shelf_the_first_start_does_not() {
        let mut s = shelf_setup("shelf-first");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "look");
        press(&mut s, &[Button::North]);
        assert_eq!(s.at(), "look", "no selection step in the first start");

        let mut s = shelf_setup("shelf-settings").only(Only::Look);
        assert!(!s.press(Button::North));
        assert_eq!(s.at(), "shelf systems");
        assert!(!s.press(Button::Down));
        assert!(!s.press(Button::East), "a tick is not the end of the step");
        assert_eq!(s.take_hidden(), Some(vec!["ports".to_string()]));
        assert_eq!(s.take_hidden(), None, "reported once");
        assert!(!s.press(Button::East));
        assert_eq!(s.take_hidden(), Some(Vec::new()), "ticked again");
        assert!(!s.press(Button::South), "B goes back to the look, Settings stays open");
        assert_eq!(s.at(), "look");
    }

    #[test]
    fn the_look_step_asks_cozy_or_quick_and_the_look_decides_until_it_is_answered() {
        use crate::settings::Theme;
        let (mut s, _) = setup("look-animations", "UTC", None);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "look");
        // Browsing: Cozy beside Shelf and Cabinet, Quick beside the other looks, nothing chosen yet.
        for _ in 0..Theme::ALL.len() {
            let theme = Theme::ALL[s.theme];
            assert_eq!(s.animations_shown(), Animations::default_for(theme), "{theme:?}");
            assert_eq!(s.take_animations(), None, "nothing to keep before L1");
            press(&mut s, &[Button::Right]);
        }
        // L1 answers it, once, and from then on the look no longer decides.
        let first = s.animations_shown();
        press(&mut s, &[Button::L1]);
        assert_eq!(s.take_animations(), Some(first.next()));
        assert_eq!(s.take_animations(), None, "reported once");
        for _ in 0..Theme::ALL.len() {
            press(&mut s, &[Button::Right]);
            assert_eq!(s.animations_shown(), first.next(), "kept while the looks change");
        }
        press(&mut s, &[Button::L1]);
        assert_eq!(s.take_animations(), Some(first));
        // Settings > Look starts from what was chosen before.
        let s = setup("look-animations-2", "UTC", None).0.only(Only::Look).with_animations(Some(Animations::Quick));
        assert_eq!(s.animations_shown(), Animations::Quick);
    }

    #[test]
    fn the_look_step_switches_the_preview_with_every_direction() {
        let (mut s, _) = setup("look-switch", "UTC", None);
        press(&mut s, &[Button::East]);
        assert_eq!(s.previewing(), Some(0));
        for (b, want) in [(Button::Right, 1), (Button::Right, 0), (Button::Down, 1), (Button::Up, 0), (Button::Left, 1)] {
            s.press(b);
            assert_eq!(s.previewing(), Some(want), "{b:?}");
        }
        press(&mut s, &[Button::East]);
        assert_eq!(s.previewing(), None, "A keeps it and goes on");
    }

    #[test]
    fn y_on_the_look_step_goes_dark_light_automatic_and_keeps_the_look() {
        let (s, _) = setup("look-appearance", "UTC", None);
        let mut s = s.with_appearance(Appearance::Light).only(Only::Look);
        let mark = s.ui_mark();
        s.press(Button::West);
        assert_eq!(s.appearance(), Appearance::Auto);
        assert_ne!(s.ui_mark(), mark, "the row changed: a click sound");
        s.press(Button::West);
        assert_eq!(s.appearance(), Appearance::Dark);
        s.press(Button::West);
        assert_eq!(s.appearance(), Appearance::Light);
        assert_eq!(s.previewing(), Some(0), "the look stays");
        assert!(s.press(Button::East), "A keeps the look and closes the page");
    }

    #[test]
    fn r1_on_settings_look_goes_full_reduced_off_and_the_first_start_has_no_such_row() {
        let (s, _) = setup("look-motion", "UTC", None);
        let mut s = s.only(Only::Look);
        assert_eq!(s.motion(), crate::motion::level());
        let start = s.motion();
        let mark = s.ui_mark();
        s.press(Button::R1);
        assert_eq!(s.motion(), start.next());
        assert_ne!(s.ui_mark(), mark, "the row changed: a click sound");
        s.press(Button::R1);
        s.press(Button::R1);
        assert_eq!(s.motion(), start, "round again");
        assert_eq!(s.previewing(), Some(0), "the look stays");

        let (first, _) = setup("look-motion-first", "UTC", None);
        let mut first = first;
        first.press(Button::East);
        assert!(first.previewing().is_some(), "on the look step");
        first.press(Button::R1);
        assert_eq!(first.motion(), crate::motion::level(), "R1 does nothing in the first start");
    }

    #[test]
    fn the_last_system_on_the_shelf_keeps_its_tick() {
        let mut s = shelf_setup("shelf-last").only(Only::Look);
        press(&mut s, &[Button::North, Button::East, Button::Down, Button::East, Button::Down, Button::East]);
        assert_eq!(s.take_hidden(), Some(vec!["gba".to_string(), "ports".to_string()]));
        assert!(s.shelf_note().is_some(), "says why Super Nintendo stays");
    }

    #[test]
    fn pc_streaming_has_its_own_tick_after_the_systems_when_moonlight_is_installed() {
        let mut s = shelf_setup("shelf-streaming").with_streaming(true, false).only(Only::Look);
        // gba, ports, snes, then PC streaming.
        press(&mut s, &[Button::North, Button::Down, Button::Down, Button::Down, Button::East]);
        assert_eq!(s.take_streaming(), Some(true), "ticked on");
        assert_eq!(s.take_streaming(), None, "reported once");
        assert_eq!(s.take_hidden(), None, "the systems are untouched");
        press(&mut s, &[Button::East]);
        assert_eq!(s.take_streaming(), Some(false));
        // Not a system: hiding the other two leaves the last system's tick alone, streaming or not.
        press(&mut s, &[Button::Up, Button::Up, Button::Up, Button::East, Button::Down, Button::East]);
        assert_eq!(s.take_hidden(), Some(vec!["gba".to_string(), "ports".to_string()]));

        let mut s = shelf_setup("shelf-no-moonlight").with_streaming(false, false).only(Only::Look);
        press(&mut s, &[Button::North, Button::Down, Button::Down, Button::Down, Button::East]);
        assert_eq!(s.take_streaming(), None, "no row without Moonlight: Down stops at Super Nintendo");
        assert_eq!(s.take_hidden(), Some(vec!["snes".to_string()]), "the tick went to Super Nintendo");
    }

    /// A canvas to paint on, and a text.
    fn paint_it(s: &mut Setup) {
        let mut px = vec![0u32; 1280 * 960];
        let mut c = Canvas::new(&mut px, 1280, 960);
        s.paint(&mut c, &mut Text::builtin());
    }

    /// Review 2026-10-24: pressing START on the Wi-Fi step ran `iw reg get`, read the zoneinfo tree and its tables and, in
    /// every painted frame, a zone file per listed city, all on the UI thread. The step now opens at once with a
    /// "Looking at" line; a thread looks. A spy records every look the test's thread makes: none, at any point.
    #[test]
    fn the_zone_step_opens_at_once_and_never_looks_at_the_system_on_the_ui_thread() {
        let spy = Arc::new(crate::system_ctl::spy::Spy::new(mock("spy-zone", "UTC", Some(IW_US))));
        let mut s = setup_on(spy.clone(), "spy-zone");
        at_wifi(&mut s);
        s.press(Button::Start);
        assert_eq!(s.at(), "zone", "open before the thread has answered");
        assert!(s.waiting() && s.animating(), "says it is looking");
        paint_it(&mut s);
        // Leaving while it looks works (B back to Wi-Fi, again START), and the late answer finds nobody waiting.
        s.press(Button::South);
        assert_eq!(s.at(), "wifi");
        s.press(Button::Start);
        s.settle();
        assert_eq!(s.at(), "zone: United States", "the answer is swapped in: the country's list");
        assert!(!s.waiting());
        paint_it(&mut s);
        // Through the lists: the cities' clocks come from the thread's offsets, painting only looks in memory.
        press(&mut s, &[Button::Down, Button::Down]);
        paint_it(&mut s);
        press(&mut s, &[Button::North, Button::East]);
        assert_eq!(s.at(), "zone: America");
        paint_it(&mut s);
        press(&mut s, &[Button::South, Button::South]);
        paint_it(&mut s);
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "extras");
        assert_eq!(spy.take(), Vec::<&str>::new(), "looked at the system on the UI thread");
    }

    #[test]
    fn leaving_the_zone_step_while_it_looks_drops_the_late_answer() {
        let (mut s, hw) = setup("leave-zone", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        s.press(Button::Start);
        s.press(Button::Start);
        assert_eq!(s.at(), "extras", "START skips even before the answer");
        // The first thread's answer arrives after the step is gone.
        let rx = s.inbox.take().unwrap();
        let late = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the thread answers");
        s.on_event(late);
        assert_eq!(s.at(), "extras", "the answer finds the step gone");
        assert_eq!(hw.state().zone, "UTC", "nothing was set");
    }

    fn roms(name: &str, systems: &[(&str, usize)]) -> (crate::testutil::TestDir, Vec<SysInfo>) {
        let dir = TestDir::new(&format!("setup-roms-{name}"));
        let mut catalog = Vec::new();
        for (id, n) in systems {
            let path = dir.join(id);
            std::fs::create_dir_all(&path).unwrap();
            for i in 0..*n {
                std::fs::write(path.join(format!("game{i}.gba")), b"x").unwrap();
            }
            catalog.push(SysInfo { id: id.to_string(), name: id.to_string(), folder: format!("roms/{id}"), path, extensions: vec!["gba".into()] });
        }
        (dir, catalog)
    }

    /// Review 2026-10-24: Settings > Games > Add games walked every ROM folder on the UI thread before it opened. It now
    /// opens with the counts the shelf knows and a thread counts the folders; the numbers swap in, the selection stays.
    #[test]
    fn the_games_step_opens_with_the_known_counts_and_counts_the_folders_on_a_thread() {
        // The shelf knows 1 game of Game Boy Advance; the folder holds 3, and Super Nintendo's 2 are new.
        let (_dir, catalog) = roms("counts", &[("gba", 3), ("snes", 2), ("psx", 0)]);
        let shelf = vec![ShelfSystem { id: "gba".into(), name: "Game Boy Advance".into(), games: 1 }];
        let mut s = setup("counts", "UTC", None).0.with_shelf(shelf, Vec::new()).with_catalog(catalog).only(Only::Games);
        assert_eq!(s.at(), "games");
        let grid = s.grid.as_ref().unwrap();
        assert_eq!((grid.games("gba"), grid.games("snes"), grid.games("psx")), (1, 0, 0), "what the shelf knows; no folder read yet");
        assert!(s.waiting());
        paint_it(&mut s);
        // Down onto Super Nintendo's tile; its games appear while it is selected.
        let sel_before = s.grid.as_ref().unwrap().tiles().iter().position(|t| *t == Tile::System(1)).unwrap();
        s.grid.as_mut().unwrap().sel = sel_before;
        s.settle();
        let grid = s.grid.as_ref().unwrap();
        assert_eq!((grid.games("gba"), grid.games("snes"), grid.games("psx")), (3, 2, 0), "the folders, counted");
        assert_eq!(grid.selected(), Some(Tile::System(1)), "the selection stays on its tile when tiles move");
        assert!(!s.waiting());
        paint_it(&mut s);
    }

    /// First-run test 2026-10-24, findings #10 and #11: a folder of no system and a disc missing its track file were
    /// passed over without a word. The games step's thread finds them; the page says so where its subtitle is, and
    /// without them the page is as before.
    #[test]
    fn the_games_step_names_folders_of_no_system_and_discs_missing_a_file() {
        let (dir, mut catalog) = roms("unread", &[("gba", 1), ("megadrive", 0)]);
        catalog[1].extensions = vec!["md".into()];
        let (mut s, _) = setup("unread", "UTC", None);
        s = s.with_catalog(catalog.clone()).only(Only::Games);
        s.settle();
        assert!(s.strays.is_empty() && s.broken.is_empty());
        assert!(unread_lines(&s.strays, &s.broken, "roms").is_empty(), "nothing unread: the usual subtitle");
        paint_it(&mut s);

        std::fs::create_dir_all(dir.join("Sega Genesis")).unwrap();
        std::fs::write(dir.join("Sega Genesis/Sonic.md"), b"x").unwrap();
        catalog[0].extensions.push("cue".into());
        std::fs::write(dir.join("gba/Lost Bin (Europe).cue"), "FILE \"Lost Bin (Europe).bin\" BINARY\n").unwrap();
        let (mut s, _) = setup("unread-2", "UTC", None);
        s = s.with_catalog(catalog).only(Only::Games);
        s.settle();
        assert_eq!(s.grid.as_ref().unwrap().games("gba"), 1, "the cue without its bin is not counted");
        assert_eq!(
            unread_lines(&s.strays, &s.broken, "roms"),
            [
                "Folder without a system: Sega Genesis. Its games go in roms/megadrive.",
                "Not on the shelf: Lost Bin. Its file Lost Bin (Europe).bin is missing.",
            ]
        );
        paint_it(&mut s);
    }

    #[test]
    fn several_unread_things_are_listed_in_a_line_each_kind() {
        let stray = |name: &str, meant: Option<&str>| library::StrayFolder { name: name.into(), meant: meant.map(str::to_string) };
        let disc = |sheet: &str| library::BrokenDisc { sheet: PathBuf::from(format!("/storage/roms/psx/{sheet}")), missing: "x.bin".into() };
        assert_eq!(unread_lines(&[stray("Stuff", None)], &[], "roms"), ["Folder without a system: Stuff. The tiles below show the folder names."]);
        assert_eq!(
            unread_lines(&[stray("PS1", Some("psx")), stray("Stuff", None)], &[disc("A (USA).cue"), disc("B.cue")], "roms"),
            ["Folders without a system: PS1, Stuff. Their games go in roms/psx.", "Not on the shelf, a disc file is missing: A, B"]
        );
        assert_eq!(
            unread_lines(&[stray("Junk", None), stray("Stuff", None)], &[], "roms"),
            ["Folders without a system: Junk, Stuff. The tiles below show the folder names."]
        );
    }

    #[test]
    fn a_refused_zone_keeps_the_step_open() {
        // The mock refuses names that are not plain zone names; the table never produces one, so ask the step directly.
        let (mut s, hw) = setup("refused", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        press(&mut s, &[Button::Start]);
        assert!(!s.set_zone("not a zone"));
        assert_eq!(s.at(), "zone");
        assert_eq!(hw.state().zone, "UTC");
    }

    /// The PortMaster browser, opened from Settings > Extras, with two ready ports and two that need files.
    fn ports_setup(name: &str) -> Setup {
        let json = br#"{"ports":{
            "2048.zip":{"name":"2048.zip","items":["2048.sh","2048/"],"attr":{"title":"2048","rtr":true,"arch":["aarch64"]},"source":{"size":1}},
            "cceleste.zip":{"name":"cceleste.zip","items":["Celeste Classic.sh","cceleste/"],"attr":{"title":"Celeste Classic","rtr":true,"arch":["aarch64"]},"source":{"size":1}},
            "sprout.zip":{"name":"sprout.zip","items":["SPROUT.sh","sprout"],"attr":{"title":"-SPROUT-","inst":"Place the game files in sprout/assets.","rtr":false,"arch":["aarch64"]},"source":{"size":1}},
            "arco.zip":{"name":"arco.zip","items":["Arco.sh","arco"],"attr":{"title":"Arco","inst":"Copy arco.exe from your Steam download to ports/arco.","rtr":false,"arch":["aarch64"]},"source":{"size":1}}
        }}"#;
        let (s, _) = setup(name, "UTC", None);
        s.only(Only::Extras).show_ports(ports::parse_catalog(json))
    }

    #[test]
    fn the_browser_keeps_ports_that_need_your_files_in_a_list_of_their_own() {
        let mut s = ports_setup("ports-tabs");
        assert_eq!(s.at(), "ports");
        let titles = |s: &Setup| s.tab_ports().iter().map(|&i| s.ports[i].title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&s), ["2048", "Celeste Classic"]);
        press(&mut s, &[Button::Down]);
        assert_eq!(s.selected_port().unwrap().title, "Celeste Classic");
        press(&mut s, &[Button::North]);
        assert_eq!(s.at(), "ports", "X does nothing on a port that runs as it is");

        press(&mut s, &[Button::Right]);
        assert_eq!(s.at(), "ports: own files");
        assert_eq!(titles(&s), ["-SPROUT-", "Arco"]);
        assert_eq!(s.selected_port().unwrap().title, "-SPROUT-", "each list keeps its own selection");
        press(&mut s, &[Button::Down, Button::Down, Button::R1]);
        assert_eq!(s.selected_port().unwrap().title, "Arco", "stops at the end of its list");
        press(&mut s, &[Button::Left]);
        assert_eq!((s.at().as_str(), s.selected_port().unwrap().title.as_str()), ("ports", "Celeste Classic"));
        press(&mut s, &[Button::Right]);

        // X: what to copy where; B (or A) back to the list.
        press(&mut s, &[Button::North]);
        assert_eq!(s.at(), "port files");
        let Step::PortFiles(page) = &s.step else { unreachable!() };
        assert_eq!((page.title.as_str(), page.place.as_path()), ("Arco", std::path::Path::new("/storage/roms/ports/arco")));
        assert!(!s.press(Button::South), "still in Extras");
        assert_eq!(s.at(), "ports: own files");
        press(&mut s, &[Button::North, Button::East]);
        assert_eq!(s.at(), "ports: own files");
        assert!(!s.press(Button::South));
        assert_eq!(s.at(), "extras", "B on the browser goes back to Extras");
    }

    #[test]
    fn installing_a_port_that_needs_files_goes_on_to_where_they_go() {
        let mut s = ports_setup("ports-install");
        s.on_event(SetupEvent::PortDone { name: "2048.zip".into(), title: "2048".into(), result: Ok(()), installing: true, updated: false });
        assert_eq!(s.at(), "ports", "a ready port: just the note");
        assert_eq!(s.port_note.as_ref().unwrap().0, "2048 is installed. Find it under Ports.");

        s.on_event(SetupEvent::PortDone { name: "sprout.zip".into(), title: "-SPROUT-".into(), result: Err("No space left".into()), installing: true, updated: false });
        assert_eq!(s.at(), "ports", "a failed install stays in the list");

        s.on_event(SetupEvent::PortDone { name: "sprout.zip".into(), title: "-SPROUT-".into(), result: Ok(()), installing: true, updated: false });
        assert_eq!(s.at(), "port files");
        let Step::PortFiles(page) = &s.step else { unreachable!() };
        assert_eq!(page.place, std::path::PathBuf::from("/storage/roms/ports/sprout/assets"));
        assert!(!page.blocked, "after an install it explains, it doesn't refuse anything");
        press(&mut s, &[Button::East]);
        assert_eq!(s.port_note.as_ref().unwrap().0, "-SPROUT- is installed. It needs your game files before it can start.");

        s.on_event(SetupEvent::PortDone { name: "sprout.zip".into(), title: "-SPROUT-".into(), result: Ok(()), installing: false, updated: false });
        assert_eq!(s.at(), "ports", "removing one opens nothing");
    }

    #[test]
    fn an_update_is_a_note_in_the_list_and_ends_with_a_word_not_a_page() {
        let mut s = ports_setup("ports-update").with_updates(&["2048.zip"]);
        let (first, second) = (s.ports.iter().find(|p| p.name == "2048.zip").unwrap().clone(), s.ports.iter().find(|p| p.name == "cceleste.zip").unwrap().clone());
        assert!(s.update_for(&first), "the catalogue has a newer 2048");
        assert!(!s.update_for(&second), "and nothing newer for Celeste");
        // Y on a port without an update does nothing (it would not be asked what to remove).
        press(&mut s, &[Button::Down, Button::West]);
        assert!(s.port_busy.is_none() && s.port_note.is_none());
        // Done: one word, the list stays; an update of a port that needs files does not open "Copy your game files".
        s.on_event(SetupEvent::PortDone { name: "2048.zip".into(), title: "2048".into(), result: Ok(()), installing: true, updated: true });
        assert_eq!(s.port_note.as_ref().unwrap(), &("2048 is up to date.".to_string(), false));
        s.on_event(SetupEvent::PortDone { name: "sprout.zip".into(), title: "-SPROUT-".into(), result: Ok(()), installing: true, updated: true });
        assert_eq!(s.at(), "ports", "an update never opens the copy page");
        s.on_event(SetupEvent::PortDone { name: "2048.zip".into(), title: "2048".into(), result: Err("No space left".into()), installing: true, updated: true });
        assert_eq!(s.port_note.as_ref().unwrap(), &("2048: No space left".to_string(), true), "a failed update says so");
    }
}

#[cfg(test)]
mod hint_bars {
    use super::*;

    fn keys(h: &[(&str, &str)]) -> Vec<String> {
        h.iter().map(|(k, _)| k.to_string()).collect()
    }

    #[test]
    fn wifi_page_leads_back_in_settings_and_walks_on_in_the_first_start() {
        assert_eq!(keys(&wifi_hints("Connect", false, false, true)), ["A", "X", "START", "B"]);
        assert_eq!(keys(&wifi_hints("Connect", false, false, false)), ["A", "X", "B"], "Settings: no next step");
        assert_eq!(wifi_hints("Connect", true, true, false)[2], ("Y", "Forget now"));
    }

    #[test]
    fn the_first_starts_look_step_names_b() {
        assert_eq!(keys(&look_hints(false, true)), ["Left/Right", "Y", "R1", "A", "B", "L1"], "B goes back to the welcome");
        assert_eq!(keys(&look_hints(true, true)), ["Left/Right", "Y", "R1", "A", "R2", "X", "L1"]);
        assert_eq!(keys(&look_hints(true, false)), ["Left/Right", "Y", "R1", "A", "R2", "L1"], "no systems: no X");
        assert_eq!(look_hints(false, false).last(), Some(&("L1", "Animations")));
        assert_eq!(look_hints(false, false)[2], ("R1", "Text size"));
    }

    #[test]
    fn file_sharing_shows_no_address_without_one() {
        assert_eq!(share_addresses(None, true), None, "offline: the page says what sharing is for instead");
        assert_eq!(share_addresses(Some("192.0.2.23"), false), None, "sharing off: no address that leads nowhere");
        assert!(crate::i18n::german().translate(SHARE_OFF).is_some(), "SHARE_OFF has a German line");
        assert_eq!(
            share_addresses(Some("192.0.2.23"), true),
            Some(["smb://192.0.2.23/games-roms".to_string(), "\\\\192.0.2.23\\games-roms".to_string()])
        );
        assert!(crate::i18n::german().translate(SHARE_OFFLINE).is_some(), "SHARE_OFFLINE has a German line");
    }

    #[test]
    fn extras_offers_a_only_where_it_works() {
        assert_eq!(keys(&extras_hints("Download", false)), ["B", "START"], "offline: nothing to download");
        assert_eq!(keys(&extras_hints("Download", true)), ["A", "B", "START"]);
    }

    #[test]
    fn add_games_offers_file_sharing_only_with_an_address_or_while_it_is_on() {
        assert_eq!(keys(&games_hints(None, false, false)), ["START", "B"], "offline, sharing off: X would do nothing");
        assert_eq!(games_hints(None, false, true)[0], ("X", "Turn on file sharing"));
        assert_eq!(games_hints(None, true, false)[0], ("X", "Turn off file sharing"), "on: it can be turned off");
        assert_eq!(keys(&games_hints(Some(false), false, false)), ["A", "START", "B"]);
        assert_eq!(games_hints(Some(true), false, true)[0], ("A", "Fewer systems"));
    }
}

#[cfg(test)]
mod friendly {
    use super::friendly_error;

    #[test]
    fn maps_nmcli_errors_to_plain_language() {
        assert_eq!(friendly_error("Error: Connection activation failed: Secrets were required, but not provided."), "Wrong password. Try again.");
        assert_eq!(friendly_error("Error: No network with SSID 'x' found."), "That network is out of range. Scan again.");
        assert_eq!(friendly_error("Error: Timeout expired"), "The network took too long to respond. Try again.");
        assert_eq!(friendly_error("Timeout expired"), "The network took too long to respond. Try again.");
        for raw in ["Error: Timeout expired (45 seconds)", "Error: Timeout expired (60 seconds)"] {
            assert_eq!(friendly_error(raw), "The network took too long to respond. Try again.");
        }
        for raw in ["Connection activation failed: The Wi-Fi network could not be found", "Activation failed: ssid-not-found"] {
            assert_eq!(friendly_error(raw), "That network is out of range. Scan again.");
        }
        let waiting = friendly_error("wlan0 not ready yet");
        assert_eq!(waiting, "Wi-Fi isn't ready yet. Try again in a moment.");
        assert!(crate::i18n::german().translate(&waiting).is_some());
        assert_eq!(friendly_error("Error: Connection activation failed"), "The connection didn't work. Try again.");
        assert_eq!(friendly_error("Error: Activation failed"), "The connection didn't work. Try again.");
        assert_eq!(friendly_error("Error: private diagnostic xyz"), "Couldn't connect to the network. Try again.");
        for raw in [
            "Secrets were required",
            "No network with SSID x",
            "Timeout expired (45 seconds)",
            "Activation failed",
            "unknown diagnostic",
        ] {
            let line = friendly_error(raw);
            assert!(crate::i18n::german().translate(&line).is_some(), "missing German: {line}");
        }
    }
}
