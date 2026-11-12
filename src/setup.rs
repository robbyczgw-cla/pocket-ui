//! First start: welcome, look, "How do you like to play?" (the profiles and the feature cards, `features`), Wi-Fi
//! with the on-screen keyboard, extras (cover art, PortMaster), how to add games, done.
//! Every step can be skipped with START. Slow work (scan, connect, sharing) runs on threads
//! and reports back through `notify`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// USB file transfer was switched on Add games (`Setup::switch_usb`): what the controller says now.
    Connected { ssid: String, result: Result<(), String> },
    /// First start: `ADVANCE_AFTER` has passed since a connection made here worked (`Setup::advance`); `gen` says which.
    Advance { gen: u64 },
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
    Sharing { on: bool, read_only: bool, password: Option<String>, switched: bool },
    /// File sharing changed under an open page (`Setup::keep_share_watch`): it came back by itself, the Pocket got an
    /// address, the card turned read-only.
    ShareChanged { now: ShareNow, password: Option<String> },
    /// Remote access as a thread found it (`remote::current`, `systemctl`) and its password; `result`: the switch's,
    /// None for the look when the page opened.
    Remote { mode: remote::Mode, password: Option<String>, result: Option<Result<(), String>> },
    /// The time zone step's first look (`Setup::enter_zone`): the guess and the zone list, read on a thread.
    Zone(ZoneFacts),
    /// The first start's guess of the time zone while the Wi-Fi page is up (`Setup::guess_zone`).
    ZoneGuess(ZoneFacts),
    /// UTC offsets of zones the step lists, read on a thread (`Setup::zone_offsets_wanted`).
    ZoneOffsets(Vec<(String, Option<i32>)>),
    /// How many games each system's folder holds, and what in `roms` isn't read, found on a thread when the games
    /// step opened.
    GameCounts(Counted),
    Covers { done: usize, total: usize, found: usize },
    /// The run that just ended gave up: the picture server didn't answer (it is tried again at the next start).
    CoversUnreachable,
    Catalog(Result<Vec<Port>, String>),
    PortMasterStatus(Result<ports::RuntimeStatus, String>),
    PortMasterSpace(Result<ports::RuntimeSpace, String>),
    PortMasterProgress(ports::DownloadProgress),
    PortMasterDone(Result<(), String>),
    PortMasterRemoved(Result<(), String>),
    PortProgress(ports::DownloadProgress),
    /// `name`: the catalog name of the port, to open its "Copy your game files" page after an install.
    PortDone { name: String, title: String, result: Result<(), String>, installing: bool, updated: bool },
    /// The phone page's server saw something: a phone, a file's bytes, a file in place, it closed by itself.
    Phone,
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
/// How often a page with the file sharing switch looks at the service (`Setup::keep_share_watch`).
const SHARE_LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// What a page with the file sharing switch shows, as the system has it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShareNow {
    pub on: bool,
    pub read_only: bool,
    pub ip: Option<String>,
}

/// `Setup::keep_share_watch`'s look, on its thread.
fn share_now() -> ShareNow {
    ShareNow { on: wifi::sharing_enabled(), read_only: wifi::share_read_only(), ip: wifi::ip() }
}

/// How long a network may stay "Connecting…" before the page says it isn't answering (a refused password kept iwd
/// trying for six minutes on a fresh card, 2026-10-27; a good one takes a few seconds, DHCP at most 45).
const CONNECT_STUCK: std::time::Duration = std::time::Duration::from_secs(60);

/// More games than this, and the look step says Easy is best with fewer.
const HANDFUL: usize = 30;
/// First start: how long "Connected to …" shows before the setup goes on to the time zone by itself.
const ADVANCE_AFTER: std::time::Duration = std::time::Duration::from_millis(1200);

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

fn extras_cheats() -> bool {
    #[cfg(feature = "brick")]
    { ports::trimui_standalone() }
    #[cfg(not(feature = "brick"))]
    { false }
}

/// Box art, Pictures, supported optional data, then Starter pack.
fn extras_rows() -> usize {
    3 + usize::from(ports::offered()) + usize::from(extras_cheats())
}

/// Setup steps that Settings can open on their own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Only {
    Look,
    Wifi,
    Extras,
    Games,
    /// Settings › Help & tools › How you like to play: the first start's question again, without the first start.
    Play,
    /// Remote access over SSH; only opened from Settings, not part of the first start.
    Remote,
}

/// What the small menu of a saved network offers (`Setup::net_menu_rows`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum NetAction {
    Connect,
    /// The first start, on the network it is connected to: on to your games.
    Continue,
    Forget,
}

/// The Wi-Fi page's rows after the networks (`Setup::wifi_tail`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum WifiRow {
    Other,
    Country,
    /// The first start: the time zone is chosen here too.
    Zone,
    /// What X does, as a row (not with the radio off, where it did nothing).
    Scan,
    /// The first start: on to your games, online or not (B goes back a stage).
    Continue,
}

/// The rows of the look step (`Setup::look_rows`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum LookRow {
    Layout,
    Appearance,
    Motion,
    Animations,
    /// The first start: Barlow or Retro (Settings › Look › Style).
    Font,
    /// The first start: the colour set (Retro's own two while it is on; Settings › Look › Colours).
    Colours,
}

/// Whether the welcome offers to set the Pocket up for a child (Kids Mode): not where the host owns buttons a child can
fn kids_offered() -> bool {
    Profile::offered().contains(&Profile::Child)
}

/// Wi-Fi page ("This device has no Wi-Fi") and the Extras page ("downloads aren't available"), both of which have nothing to choose.
fn no_wifi_chip() -> bool {
    let d = crate::device::current();
    !d.has_wifi()
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
    /// The first start's "Your games": how many were found, Continue, Add more…, and the Box art switch. Add more… opens
    /// `Games`, the systems grid and the ways.
    Found { sel: usize },
    Games,
    /// "Send from your phone" (Y on Add games): its upload server runs while this step is shown (`phone_upload`).
    Phone(Box<crate::phone_upload_page::PhonePage>),
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
    /// The first start after Wi-Fi (`Setup::leave_wifi`): a guess it can trust is taken without showing the step; only
    /// a failed guess (no zone, or a country with several) shows it.
    auto: bool,
    /// UTC offsets of the zones shown, read on threads (`Setup::zone_offsets_wanted`); painting only looks here.
    offsets: HashMap<String, Option<i32>>,
    /// The zones whose offsets were asked for already.
    asked: std::collections::HashSet<String>,
    /// The card's focused row: 0 takes the guess, 1 opens the picker (X does too).
    card_row: usize,
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
            auto: false,
            offsets: HashMap::new(),
            asked: Default::default(),
            card_row: 0,
        }
    }

    /// A guess to take without asking: the zone the system has, or a country's one zone, and the system took it.
    fn clear(&self) -> bool {
        !self.loading && self.error.is_none() && matches!(self.guess, Guess::System(_) | Guess::Country { .. })
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
    /// Whether the last page of the first start says what HOME does (`first_game::told_on_last_page`), read once.
    home_line: std::cell::Cell<Option<bool>>,
    /// Add games was left with `kids_wanted` and games on the card: the App opens Kids Mode's steps (`take_kids_asked`).
    kids_asked: bool,
    /// Y on the welcome, "For a child…": only the wish. Kids Mode's own steps (PIN, games, colour, time) take the look
    /// step's place after Add games, when there are games to pick (`leave_games`).
    kids_wanted: bool,
    /// Kids Mode was wanted but there was nothing it could start: the look step says where to set it up later.
    kids_later: bool,
    /// Kids Mode's first page was skipped with B: the look step says where the mode is for later.
    kids_skipped: bool,
    /// The guess of the time zone made while the Wi-Fi page is up (`guess_zone`), for its Time zone row, and whether a
    /// thread is making one.
    zone_guess: Option<Box<ZoneStep>>,
    zone_guessing: bool,
    /// Left/Right on the look step's Colours row (first start): the next (true) or previous set, once; the app steps them.
    colours_step: Option<bool>,
    /// The same on its Style row (Barlow, Retro: `font_style`, `retro`).
    font_step: Option<bool>,
    /// The time zone was opened from the Wi-Fi page's Time zone row: leaving it goes back there, and the zone picked
    /// there is kept (the guess after Wi-Fi is not made again).
    zone_from_wifi: bool,
    zone_picked: Option<String>,
    /// The first start's time zone stage was shown and left (a zone set, Skip, Keep UTC): leaving Wi-Fi again goes
    /// straight on, the Wi-Fi page's Time zone row still opens it (first-start walk 2026-11-06: asked three times).
    zone_asked: bool,
    /// The phone page, or its way on Add games, sent the first start to the Wi-Fi page: leaving it goes back to Add
    /// games, and on to the phone page once there is an address (`back_to_phone`).
    wifi_for_phone: bool,
    /// The look previews (Shelf, Easy, ...) for "How do you like to play?": lent by the app while it paints that step.
    pub play_previews: Vec<Image>,
    profile_look: Option<usize>,
    look_before_profile: Option<usize>,
    /// No Wi-Fi chip (`no_wifi_chip`): the first start has no Wi-Fi and Extras pages. A field so tests can set it.
    no_wifi: bool,
    /// The password last sent to a network: back in the field if it is refused.
    typed_password: String,
    /// Games in the system folders when last counted (`count_games`): `games` is what the shelf knew when this opened,
    /// and a card that was never read, or a game sent from the phone since, isn't in it.
    games_counted: Option<usize>,
    step: Step,
    /// Opened straight at Systems on the shelf (`at_shelf_systems`): B there closes the setup, back to the row, instead
    /// of going to the Look step.
    direct: bool,
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
    /// A on a saved network: its small menu (Connect or Continue, Forget) and the row selected; Y forgets straight.
    net_menu: Option<(String, usize)>,
    /// The network NetworkManager is bringing up (`Saved::connecting`) and since when this page sees it so: after
    /// `CONNECT_STUCK` the page says it isn't answering, and A types the password again.
    connecting_since: Option<(String, Instant)>,
    /// The last typed connection was to a hidden network: a refused password asks again the same way.
    connect_hidden: bool,
    /// First start: a connect was started on this page and has not ended yet. Only such a connect moves the setup on by
    /// itself; coming back to the Wi-Fi step with B never does, even when Wi-Fi is connected.
    advance_armed: bool,
    /// First start: the network just connected here and the time (`ADVANCE_AFTER`) before the step goes on; any press or
    /// leaving the page ends it. `advance_gen` tells the answer of an older wait from this one.
    advance_pending: Option<String>,
    advance_gen: u64,
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
    /// Add games' ways (`Way`): the one last chosen, whether the focus is on that row instead of the grid, USB file
    /// transfer being switched, and its last attempt having failed.
    ways_sel: usize,
    ways_focus: bool,
    /// What the last Add games frame offers to touch (`register_touch`).
    touch: std::cell::RefCell<Vec<(crate::touch::Area, Touch)>>,
    /// A thread is looking at file sharing (`look_sharing`): never `systemctl` on the UI thread.
    share_looking: bool,
    /// This device's file sharing password, once it exists.
    share_password: Option<String>,
    /// The games folder is on a read-only mount: sharing runs, and every copy fails.
    share_read_only: bool,
    /// While a page with the switch is up, a thread looks at file sharing every `share_every` (`keep_share_watch`).
    share_watch: Option<Arc<()>>,
    share_every: std::time::Duration,
    share_ask: fn() -> ShareNow,
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
    /// Settings > Look > Appearance: a row of the look step opened from Settings, and of the first start's.
    appearance: Appearance,
    /// Settings > Sound & feel > Motion: a row of the look step opened from Settings.
    motion: Motion,
    /// The look step's focused row, Up/Down as in every list (`LookRow`): the layouts, then the rows under them. Left/Right
    /// change the focused one (Brick, 2026-11-07: the rows under the layouts were drawn and unreachable).
    look_row: usize,
    /// Settings > Look as it was when the page took its first press: the layout, Appearance, Motion and Animations, which
    /// B puts back (the Brick and the Odin, 2026-11-07: B kept the last layout looked at, though the bar said "A Keep").
    look_opened: Option<(usize, Appearance, Motion, Option<Animations>)>,
    /// Opened again from Settings › Help & tools › First-start setup (`again`): B on the welcome goes back there, and
    /// `backed_out` says so to the app.
    again: bool,
    backed_out: bool,
    /// The welcome's row that Left/Right change: 0 the language, 1 Big text (Up/Down; first-start walk 2026-11-06:
    /// the second row looked like one to choose, and Down then Right changed the language).
    welcome_row: u8,
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
    /// The last run found no answer from the picture server (`SetupEvent::CoversUnreachable`).
    covers_unreachable: bool,
    /// Settings > Games > Extras > Pictures: what is fetched by itself, and whether the last press changed it
    /// (`take_art`).
    art: Art,
    art_changed: bool,
    cheat_data: Option<crate::cheat_data::Page>,
    extras_demo_online: Option<bool>,
    extras_touch: std::cell::RefCell<crate::touch::HitMap>,
    open_starter: bool,
    covers_cancel: crate::update::Cancel,
    /// The time zone step was left without a pick and with nothing to go on: the clock is on UTC (the Done page says so).
    zone_utc: bool,
    /// When the games step's counts were last asked for (a press while the step is open counts again, at most every few seconds).
    counted_at: Instant,
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
    port_progress: Option<ports::DownloadProgress>,
    portmaster: Option<PortMasterState>,
    portmaster_status: ports::RuntimeStatus,
    portmaster_note: Option<String>,
    portmaster_demo_online: Option<bool>,
    portmaster_fetch: fn(&ports::DownloadReporter, &ports::DownloadCancel) -> Result<(), String>,
    portmaster_remove: fn(&ports::DownloadReporter, &ports::DownloadCancel) -> Result<(), String>,
    portmaster_confirm_remove: bool,
    portmaster_removing: bool,
    portmaster_space: Option<ports::RuntimeSpace>,
    port_download_cancel: Option<ports::DownloadCancel>,
    previews: HashMap<String, Option<Image>>,
    /// Opened from Settings for one step: leaving that step ends setup instead of moving on.
    only: Option<Only>,
    /// Games came in from the phone and its page closed: the shelf reads the folders again (`take_phone_added`).
    phone_added: bool,
    /// Add games asked for the Wi-Fi settings (`take_open_wifi`): from which way's tile (true: From your phone,
    /// false: File sharing), and whether from the phone page itself.
    open_wifi: Option<(bool, bool)>,
    /// Settings › Wi-Fi opened from Add games (`back_to_games`): leaving it opens Add games again, on that tile, and on
    /// the phone page when that asked (`take_back_to_games`).
    back_to_games: Option<(bool, bool)>,
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

#[derive(Clone, Debug)]
enum PortMasterState {
    Checking,
    Offer(ports::RuntimeStatus),
    Downloading(ports::DownloadProgress),
    Ready,
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

/// Where BIOS files go (see docs/user/getting-started.md): `crate::hostsys::bios_dir()` (`POCKETUI_BIOS` can override it).

/// A system with games, for Settings > Look > Systems on the shelf.
#[derive(Clone, Debug)]
pub struct ShelfSystem {
    pub id: String,
    pub name: String,
    pub games: usize,
}

impl Drop for Setup {
    fn drop(&mut self) {
        self.covers_cancel.cancel();
        if let Some(cancel) = &self.port_download_cancel { cancel.cancel(); }
    }
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

    /// The animations after L1 on the look step changed them, once; the app keeps them (Settings → Sound & feel →
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
            Button::South | Button::Start => self.step = self.back_to_look(),
            _ => {}
        }
    }

    fn paint_shelf_systems(&self, c: &mut Canvas, text: &mut Text, sel: usize) {
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Systems on the shelf");
        let (line, color) = match &self.shelf_note {
            Some(note) => (note.clone(), pal().alert),
            None => ("Untick a system to leave it off the shelf. Its games stay, starred ones in Favourites.".to_string(), pal().dust),
        };
        // Without systems there is nothing to untick: the line under the list below says why.
        // A line that is too long for the page (German, Spanish, a small screen) takes a second line and the list moves
        // down for it; it used to end in "…" ("…Seine Spiele bleiben, die m…").
        let step = crate::type_scale::line(28, 10);
        let mut top = 232;
        if self.shelf_rows() > 0 {
            for (i, l) in text.wrap_prose_at_most(Face::Body, 28, &line, c.w - 2 * MARGIN, 2).iter().enumerate() {
                text.draw(c, Face::Body, 28, MARGIN, 192 + i as i32 * step, color, l);
                if i > 0 {
                    top += step;
                }
            }
        }
        let row = 76;
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
                crate::retro::pill(c, bx, by, bs, bs, 6, if selected { pal().ink } else { pal().chalk });
                tick(c, bx, by, bs, if selected { pal().label } else { pal().spine });
            } else {
                crate::retro::pill(c, bx, by, bs, bs, 6, fg);
                crate::retro::pill(c, bx + 3, by + 3, bs - 6, bs - 6, 4, if selected { pal().label } else { pal().spine });
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
        // No games on the card yet: nothing to tick, and the page says so instead of staying blank.
        if self.shelf_rows() == 0 {
            let mut y = top + 40;
            for line in text.wrap_prose_at_most(Face::Body, 28, "No systems with games yet. Once games are on the SD card, their systems show up here.", width, 3) {
                text.draw(c, Face::Body, 28, MARGIN, y, pal().chalk, &line);
                y += crate::type_scale::line(28, 10);
            }
            hints(c, text, &[("B", "Back")]);
            return;
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
    let ph = (tall - (1080 - h).max(0)).max(140);
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
        let unread = unread_lines(&self.strays, &self.broken);
        match unread.as_slice() {
            [] => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Add your games");
                let d = crate::device::current();
                let about = if !d.has_wifi() || crate::card_health::games_drive() {
                    // File sharing isn't the way, or not the only one: the right side says which is.
                    "One folder per system in roms."
                } else {
                    "One folder per system in roms. Copy games over Wi-Fi with file sharing."
                };
                // Smaller type before "…": the Spanish line at large text (first-start walk 2026-11-06).
                let (px, line) = text.fit_sizes(Face::Body, &[28, 25, 22], about, width);
                text.draw(c, Face::Body, px, MARGIN, 192, pal().dust, &line);
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
        // The row of ways takes the room of a grid row on the Mini V2 (3 → 2) and the V1 (2 → 1).
        let ways = self.ways();
        let ways_h = if ways.is_empty() { 0 } else { WAY_H + 10 };
        let grid_rows = ((c.h - 228 - 336 - ways_h) / (GRID_TILE_H + GRID_GAP)).clamp(1, 3) as usize;
        self.touch.borrow_mut().clear();
        if let Some(grid) = &self.grid {
            self.paint_grid(c, text, grid, 228, width, grid_rows);
        }
        let mut top = 228 + grid_rows as i32 * (GRID_TILE_H + GRID_GAP) + 6;
        if !ways.is_empty() {
            self.paint_ways(c, text, &ways, top - 6, width, dots);
            top += ways_h;
        }
        c.rect(MARGIN, top, width, 2, pal().keycap);
        let col_w = (width - 40) / 2;
        if let Some(grid) = &self.grid {
            self.paint_tile_details(c, text, grid, MARGIN, top, col_w);
        }
        match ways.get(self.ways_sel).filter(|_| self.ways_focus) {
            _ => self.paint_share(c, text, MARGIN + col_w + 40, top, col_w, dots),
        }
        // The "All systems" tile selected: whether its list is open.
        let more = match self.grid.as_ref().and_then(Grid::selected) {
            Some(Tile::More) => Some(self.grid.as_ref().is_some_and(|g| g.expanded)),
            _ => None,
        };
        let mut bar = games_hints(more, self.sharing, self.ip.is_some());
        // A on the row of ways: what it does to the one selected.
        if let Some(label) = ways.get(self.ways_sel).filter(|_| self.ways_focus).map(|w| self.way_action(*w)) {
            bar.insert(0, ("A", label));
        }
        let areas = hints(c, text, &bar);
        self.touch.borrow_mut().extend(areas.into_iter().map(|(a, b)| (a, Touch::Hint(b))));
    }

    /// The row of ways under the grid: a tile each, the grid's own look; the selected one when the focus is here.
    fn paint_ways(&self, c: &mut Canvas, text: &mut Text, ways: &[Way], y: i32, width: i32, dots: &str) {
        let n = ways.len() as i32;
        // Two columns of the grid wide while there are one or two, else the row shared out.
        let tile_w = if n <= 2 { (width - GRID_GAP * 3) / 4 * 2 + GRID_GAP } else { (width - GRID_GAP * (n - 1)) / n };
        for (i, way) in ways.iter().enumerate() {
            let x = MARGIN + i as i32 * (tile_w + GRID_GAP);
            let selected = self.ways_focus && i == self.ways_sel;
            crate::kit::shelf_row(c, x, y, tile_w, WAY_H, selected);
            let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
            let (title, state, on) = self.way_state(*way, dots);
            let title = text.fit(Face::Title, 30, title, tile_w - 32 - 40);
            text.draw(c, Face::Title, 30, x + 16, y + 38, fg, &title);
            let state = text.fit(Face::Body, 21, &state, tile_w - 32);
            text.draw(c, Face::Body, 21, x + 16, y + 68, dim, &state);
            if on {
                tick(c, x + tile_w - 16 - 22, y + 14, 22, fg);
            }
            self.touch.borrow_mut().push((crate::touch::Area::new(x, y, tile_w, WAY_H), Touch::Way(i)));
        }
    }

    /// A way's name, its state in a few words, and whether it is on.
    fn way_state(&self, way: Way, dots: &str) -> (&'static str, String, bool) {
        match way {
            Way::Sharing if self.sharing_busy => ("File sharing", format!("Switching{dots}"), self.sharing),
            Way::Sharing if self.sharing => ("File sharing", "On".to_string(), true),
            Way::Sharing if self.ip.is_none() => ("File sharing", "Connect to Wi-Fi first".to_string(), false),
            Way::Sharing => ("File sharing", "Off".to_string(), false),
            Way::Phone if self.ip.is_none() => ("From your phone", "Connect to Wi-Fi first".to_string(), false),
            Way::Phone => ("From your phone", "Or a computer's browser, same Wi-Fi".to_string(), false),
        }
    }

    /// What A does on a way, for the hint bar.
    fn way_action(&self, way: Way) -> &'static str {
        match way {
            Way::Sharing if self.sharing => "Turn off file sharing",
            Way::Sharing if self.ip.is_none() => "Connect to Wi-Fi",
            Way::Sharing => "Turn on file sharing",
            Way::Phone if self.ip.is_none() => "Connect to Wi-Fi",
            Way::Phone => "From your phone",
        }
    }


    fn paint_grid(&self, c: &mut Canvas, text: &mut Text, grid: &Grid, top: i32, width: i32, rows_shown: usize) {
        const GAP: i32 = GRID_GAP;
        const TILE_H: i32 = GRID_TILE_H;
        let cols = grid.cols;
        let tile_w = (width - GAP * (cols as i32 - 1)) / cols as i32;
        let tiles = grid.tiles();
        let rows = tiles.len().div_ceil(cols);
        // Keep context above the selection only when there is room for it. A one-row viewport must show
        // the selected row itself (H700's Add games page).
        let first_row = (grid.sel / cols).saturating_sub(rows_shown.saturating_sub(1).min(1)).min(rows.saturating_sub(rows_shown));
        for (i, tile) in tiles.iter().enumerate().skip(first_row * cols).take(rows_shown * cols) {
            let (r, col) = ((i / cols - first_row) as i32, (i % cols) as i32);
            let (x, y) = (MARGIN + col * (tile_w + GAP), top + r * (TILE_H + GAP));
            let selected = i == grid.sel && !self.ways_focus;
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
                let (exts, bios) = systems_grid::details(sys, &crate::hostsys::bios_dir());
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
                // Above the hint bar: on a 768-line screen (the Brick) the third line of an unknown BIOS ran under "From your phone".
                let bar = crate::kit::hint_bar_top(c.h) - 8;
                let room = |y: i32| ((bar - y) / 32 + 1).max(1) as usize;
                let bios_lines = text.wrap_prose_at_most(Face::Body, 24, &line, w - label_w, room(top + 160).min(3));
                for (i, l) in bios_lines.iter().enumerate() {
                    text.draw(c, Face::Body, 24, x + label_w, top + 160 + i as i32 * 32, color, l);
                }
                // One plain caution where the handheld runs the system but not every game of it smoothly.
                if let Some(note) = crate::system_notes::note(&sys.id) {
                    let y = top + 160 + bios_lines.len() as i32 * 32 + 14;
                    if y < bar {
                        for (i, l) in text.wrap_prose_at_most(Face::Body, 24, note, w, room(y).min(2)).iter().enumerate() {
                            text.draw(c, Face::Body, 24, x, y + i as i32 * 32, pal().dust, l);
                        }
                    }
                }
            }
            Some(Tile::More) => {
                text.draw(c, Face::Title, 37, x, top + 50, pal().chalk, if grid.expanded { "Fewer systems" } else { "All systems" });
                let about = if grid.expanded {
                    "A closes the list again; the popular systems and the ones with games stay."
                } else {
                    "Every system ROCKNIX can start, each with its own folder in roms."
                };
                for (i, l) in text.wrap_prose(Face::Body, 24, about, w).iter().take(3).enumerate() {
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
            let d = crate::device::current();
            (if crate::games_card::in_use() { "Games card in" } else { offline_way_here(&d).0 }.to_string(), pal().dust)
        } else {
            ("File sharing is off".to_string(), pal().chalk)
        };
        let status = text.fit(Face::Title, 37, &status, w);
        text.draw(c, Face::Title, 37, x, top + 50, color, &status);
        let Some(addrs) = share_addresses(self.ip.as_deref(), self.sharing) else {
            // No address to show: what the switch is for, not an address with "(not connected)" in it, and not one
            // that leads nowhere while sharing is off (the new-user test, 2026-10-30).
            let d = crate::device::current();
            let about = match self.ip {
                Some(_) => SHARE_OFF,
                None if crate::card_health::image_card() => crate::card_health::IMAGE_CARD_NOTE,
                None if crate::games_card::in_use() => GAMES_CARD_WAY,
                None if self.only.is_none() && crate::card_health::games_drive() => GAMES_DRIVE_FIRST_WAY,
                None => offline_way_here(&d).1,
            };
            // Above the hint bar ("estas carpetas." ran into it at Large text on 768 lines, the Brick).
            let room = (((crate::kit::hint_bar_top(c.h) - 8 - (top + 92)) / 32 + 1).max(1) as usize).min(4);
            for (i, l) in text.wrap_prose_at_most(Face::Body, 24, about, w, room).iter().enumerate() {
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
        let (note, color) = if self.share_read_only { (SHARE_READ_ONLY, pal().alert) } else { (SHARE_BIOS_NOTE, pal().dust) };
        let note = text.fit(Face::Body, 24, note, w);
        text.draw(c, Face::Body, 24, x, top + 244, color, &note);
    }
}


/// Add games without Wi-Fi and no other way, under "Connect to Wi-Fi first": what file sharing will do once there is an address.
const SHARE_OFFLINE: &str = "Then a computer on the same network can copy games straight into these folders.";

/// Add games without a network, on a card with the GAMES drive (docs/SD-IMAGE.md).
const GAMES_DRIVE_WAY: &str = "Put the card in your computer and copy games to the GAMES drive, into its roms folder.";
/// The same in the first start's games step: the drive isn't there until this first start is over (a computer shows no GAMES
/// right after flashing), so it says when to go and get it.
const GAMES_DRIVE_FIRST_WAY: &str = "After this first start, switch off, put the card in a computer and copy games into the drive called GAMES.";
/// Add games on the Brick's own card.
const BRICK_CARD_WAY: &str = "Put the card in your computer and copy games into its Roms folder, one folder per system.";
/// Add games without a network and without that drive, on a handheld with a second slot.
const SECOND_CARD_WAY: &str = "Copy games to a second SD card (over 8 GB) in your computer, into a folder called roms, then put it in the second slot.";
const OR_SECOND_CARD_WAY: &str = "Or copy games to a second SD card (over 8 GB) in your computer, into a folder called roms, then put it in the second slot.";

/// Add games while a card in the second slot is what the games are read from.
const GAMES_CARD_WAY: &str = "Your games are read from your games card. To add more, put that card in your computer and copy them into its roms folder.";

/// What Add games says without a network: a heading and a line, the way that works here. A handheld without Wi-Fi must
/// not end at "Connect to Wi-Fi first", which is a dead end there (RG28XX).
fn offline_way(games_drive: bool, second_slot: bool, wifi: bool) -> (&'static str, &'static str) {
    match (games_drive, second_slot, wifi) {
        (true, _, _) => ("Copy from a computer", GAMES_DRIVE_WAY),
        (false, true, false) => ("Use a second card", SECOND_CARD_WAY),
        (false, true, true) => ("Connect to Wi-Fi first", OR_SECOND_CARD_WAY),
        (false, false, _) => ("Connect to Wi-Fi first", SHARE_OFFLINE),
    }
}

/// [`offline_way`] for this device: the Brick's own card is the one card a computer reads as it is (the Roms folder).
fn offline_way_here(d: &crate::device::Device) -> (&'static str, &'static str) {
    offline_way_for(d.card_reads_in_computer(), crate::card_health::games_drive(), d.has_second_card_slot(), d.has_wifi())
}

fn offline_way_for(card_in_computer: bool, games_drive: bool, second_slot: bool, wifi: bool) -> (&'static str, &'static str) {
    if card_in_computer && !games_drive {
        ("Copy from a computer", BRICK_CARD_WAY)
    } else {
        offline_way(games_drive, second_slot, wifi)
    }
}

/// The height of a tile in the row of ways under the grid.
const WAY_H: i32 = 84;

/// Add games' USB file transfer: its sentence (the same as Settings → Connections) and what it says when it didn't start.

/// A way to get games onto this device that Add games lets you start from its row of tiles. The ways with only a
/// sentence (the second card, the GAMES drive) are not here: they have nothing to switch (`offline_way`).
/// "From your phone" (`Way::Phone`, also Y) opens the upload page; unconnected, it leads to the Wi-Fi page like sharing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Sharing,
    Phone,
}

/// The rows of the first start's "Your games" (`Setup::found_rows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FoundRow {
    Continue,
    More,
    Later,
    BoxArt,
}

/// What a touch on Add games does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Touch {
    Way(usize),
    Hint(Button),
}

/// The ways this device offers: file sharing where there is Wi-Fi (connected or not: unconnected, it leads to the Wi-Fi
fn ways_for(wifi: bool) -> Vec<Way> {
    let mut ways = Vec::new();
    if wifi {
        ways.push(Way::Sharing);
        if crate::phone_upload_page::offered() {
            ways.push(Way::Phone);
        }
    }
    ways
}

/// The first start's "Your games" without games, on a handheld with Wi-Fi and a second card slot. A computer can't read
/// this card's STORAGE part, and Windows offers to format it: that is the warning, not "put them in the roms folder".
const FOUND_WIFI_SLOT_WAY: &str = "Copy them over Wi-Fi from a computer or phone, or use a second SD card. If Windows offers to format a card, press Cancel.";
/// The same on a handheld with Wi-Fi and one slot.
const FOUND_WIFI_WAY: &str = "Copy them over Wi-Fi from a computer or phone. If you put the card in a computer and Windows offers to format it, press Cancel.";

/// What the first start's "Your games" says when there are no games, for the first way that applies. A pure function of the
/// device, so a test can walk every kind of device (K1 of the UX review: "the same folders" was said to cards a computer
/// can't read).
fn no_games_way(image_card: bool, games_card: bool, games_drive: bool, card_in_computer: bool, wifi: bool, second_slot: bool) -> &'static str {
    if image_card {
        crate::card_health::IMAGE_CARD_NOTE
    } else if games_card {
        GAMES_CARD_WAY
    } else if games_drive {
        GAMES_DRIVE_FIRST_WAY
    } else if card_in_computer {
        BRICK_CARD_WAY
    } else if wifi {
        if second_slot { FOUND_WIFI_SLOT_WAY } else { FOUND_WIFI_WAY }
    } else {
        offline_way(false, second_slot, false).1
    }
}

/// The Box art row's line on "Your games".
const BOX_ART_NOTE: &str = "Finds the real box for each game, in the background while you're online. Only a game's name is sent.";

/// Add games with Wi-Fi but file sharing off: how to start it, and how long it stays on.
const SHARE_OFF: &str = "Press X to turn it on. Then a computer on the same network can copy games into these folders. It stays on until you turn it off.";

/// Add games with file sharing on: BIOS files are copied over the share too.
const SHARE_BIOS_NOTE: &str = "BIOS files go in the bios folder";

/// Add games with file sharing on and the games folder on a read-only mount: every copy would fail.
const SHARE_READ_ONLY: &str = "Card is read-only: copying won't work";

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
        Step::Look | Step::ShelfSystems { .. } => Some(Only::Look),
        Step::Wifi | Step::Portal(_) | Step::Password { .. } | Step::Connecting { .. } | Step::HiddenName { .. } | Step::Country { .. } => Some(Only::Wifi),
        Step::Extras { .. } | Step::Ports | Step::PortFiles(_) => Some(Only::Extras),
        Step::Games | Step::Phone(_) | Step::Found { .. } => Some(Only::Games),
        Step::Remote => Some(Only::Remote),
        // Settings > Time is the way to this one on its own.
        // The profiles and the cards are first-start steps; Settings → Tour shows the cards on its own.
        Step::Play { .. } | Step::Cards(_) => Some(Only::Play),
        Step::Welcome | Step::Zone(_) | Step::Done => None,
    }
}

/// How soon after the last count a press on the games step counts again.
const RECOUNT_AFTER: Duration = Duration::from_secs(3);

/// How many stages the first start has, for its dots: the welcome with the language, Wi-Fi (the time zone in its place
/// without a Wi-Fi chip; with Wi-Fi the zone is guessed and only asked inside this stage when the guess is unsure), your
fn stage_count() -> usize {
    4
}

/// Which dot a first-start page lights (from 0); `None` for the pages inside a stage (keyboards, lists, the time zone's
/// picker, Add more…), which keep the screen for their own title.
fn stage_of(step: &Step) -> Option<usize> {
    let games = { 2 };
    match step {
        Step::Welcome => Some(0),
        Step::Wifi => Some(1),
        Step::Zone(z) if matches!(z.view, ZoneView::Card | ZoneView::List { from: ListFrom::Country, .. }) => Some(1),
        Step::Found { .. } => Some(games),
        Step::Look => Some(games + 1),
        _ => None,
    }
}

/// The first start's progress, in the data folder.
const RESUME_FILE: &str = "setup-step";

/// The name `remember_step` writes for a step, `None` where nothing is written (the welcome, which has no choice to
/// lose, and the steps that only exist in Settings or after the last page).
/// "shelf-look" is the look as the last stage: the name "look" was the look as the second step before 0.2 and is read as
/// such (`resume`). The welcome's wish for Kids Mode is kept after the name (`games child`).
fn resume_name(step: &Step) -> Option<&'static str> {
    match step {
        Step::Look => Some("shelf-look"),
        Step::Wifi | Step::Portal(_) | Step::Password { .. } | Step::Connecting { .. } | Step::HiddenName { .. } | Step::Country { .. } => Some("wifi"),
        Step::Zone(_) => Some("zone"),
        Step::Found { .. } | Step::Games | Step::Phone(_) => Some("games"),
        Step::Welcome | Step::Play { .. } | Step::Cards(_) | Step::Extras { .. } | Step::Ports | Step::PortFiles(_) | Step::ShelfSystems { .. } | Step::Remote | Step::Done => None,
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
    let mut z = ZoneStep { guess, country, all, view: ZoneView::Card, error: None, loading: false, auto: false, offsets: HashMap::new(), asked: Default::default(), card_row: 0 };
    z.view = z.start_view();
    let wanted = z.wanted();
    z.asked.extend(wanted.iter().cloned());
    z.offsets.extend(zone_offsets(hw, wanted));
    z
}

/// The Extras step's line under the title. A handheld without a Wi-Fi chip (the RG28XX) can't "connect to Wi-Fi first":
/// it says so instead (first-start test 2026-11-01).
fn extras_sub(online: bool, has_wifi: bool) -> &'static str {
    if online {
        "Optional. You can get these any time later."
    } else if !has_wifi {
        "This device has no Wi-Fi, so the downloads aren't available."
    } else {
        "Connect to Wi-Fi first to use these."
    }
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
        (None, _) if games == 0 => "no games yet".into(),
        (None, _) if games == 1 => "1 game".into(),
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
        if n > 0 && !library::supplied_by_device(&s.id, &s.path) {
            found.games.insert(s.id.clone(), n);
        }
        found.broken.extend(broken);
    }
    found.broken.sort_by(|a, b| a.sheet.cmp(&b.sheet));
    found.broken.dedup();
    if let Some(roms) = library::roms_root(catalog) {
        found.strays = library::stray_folders(&roms, catalog);
        // Folders of another card ("Game Boy Advance (GBA)") are read as the system they are: their games count too.
        for (id, dir) in library::foreign_folders(&roms, catalog) {
            if let Some(s) = catalog.iter().find(|s| s.id == id) {
                let n = library::count_games(&id, &dir, &s.extensions);
                if n > 0 {
                    *found.games.entry(id).or_insert(0) += n;
                }
            }
        }
    }
    found
}

/// The lines Add games shows instead of its subtitle when something in `roms` isn't read: the stray folders, then the
/// disc games missing a file. At most two lines.
pub fn unread_lines(strays: &[library::StrayFolder], broken: &[library::BrokenDisc]) -> Vec<String> {
    let mut lines = Vec::new();
    match strays {
        [] => {}
        [one] => lines.push(format!("Folder without a system: {}. The tiles below show the folder names.", one.name)),
        // More than two don't fit a line, and what to do would be the part cut off: the count and where the rest is said.
        many if many.len() > 2 => lines.push(format!("{} folders without a system. Settings › Games › Game check says where their games go.", many.len())),
        many => {
            let names: Vec<&str> = many.iter().map(|s| s.name.as_str()).collect();
            lines.push(format!("Folders without a system: {}. The tiles below show the folder names.", names.join(", ")));
        }
    }
    let title = |b: &library::BrokenDisc| display_name_of(&b.sheet);
    match broken {
        [] => {}
        [one] => lines.push(format!("Not on the shelf: {}. A file is missing: {}", title(one), one.missing)),
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
            home_line: std::cell::Cell::new(None),
            kids_asked: false,
            kids_wanted: false,
            kids_later: false,
            kids_skipped: false,
            zone_guess: None,
            zone_guessing: false,
            colours_step: None,
            font_step: None,
            zone_from_wifi: false,
            zone_picked: None,
            zone_asked: false,
            wifi_for_phone: false,
            play_previews: Vec::new(),
            profile_look: None,
            look_before_profile: None,
            no_wifi: no_wifi_chip(),
            typed_password: String::new(),
            games_counted: None,
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
            net_menu: None,
            connecting_since: None,
            connect_hidden: false,
            advance_armed: false,
            advance_pending: None,
            advance_gen: 0,
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
            ways_sel: 0,
            ways_focus: false,
            touch: Default::default(),
            share_looking: false,
            share_password: None,
            share_read_only: false,
            share_watch: None,
            share_every: SHARE_LOOK_EVERY,
            share_ask: share_now,
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
            welcome_row: 0,
            look_row: 0,
            look_opened: None,
            again: false,
            backed_out: false,
            language_chosen: None,
            text_size_chosen: None,
            animations: None,
            animations_changed: false,
            data_dir,
            games,
            covers: None,
            covers_running: false,
            covers_unreachable: false,
            art: Art::default(),
            art_changed: false,
            cheat_data: None,
            extras_demo_online: None,
            extras_touch: Default::default(),
            open_starter: false,
            covers_cancel: Default::default(),
            zone_utc: false,
            counted_at: Instant::now(),
            ports: Vec::new(),
            fake_updates: None,
            port_tab: PortTab::Ready,
            port_sel: [0, 0],
            catalog_loading: false,
            port_busy: None,
            port_note: None,
            port_progress: None,
            portmaster: None,
            portmaster_status: ports::RuntimeStatus::Missing,
            portmaster_note: None,
            portmaster_demo_online: None,
            portmaster_fetch: ports::install_runtime,
            portmaster_remove: ports::remove_runtime,
            portmaster_confirm_remove: false,
            portmaster_removing: false,
            portmaster_space: None,
            port_download_cancel: None,
            previews: HashMap::new(),
            only: None,
            open_wifi: None,
            back_to_games: None,
            phone_added: false,
            direct: false,
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
    /// "For a child" was chosen: the App opens Kids Mode's steps (`app_kids`).
    pub fn take_kids_asked(&mut self) -> bool {
        std::mem::take(&mut self.kids_asked)
    }

    /// Kids Mode's steps are done: they were the first start's last step. The app ends the setup (`App::finish_setup`).
    pub fn kids_done(&mut self) {
        self.step = Step::Done;
        self.remember_step(true);
    }

    /// B on Kids Mode's first page: the first start goes on as without the wish, at the look step.
    pub fn kids_cancelled(&mut self) {
        self.kids_wanted = false;
        self.kids_skipped = true;
        self.enter_look();
        self.remember_step(false);
    }

    /// Kids Mode found no game it could start (they all run in emulators with menus of their own): never a locked, empty
    /// shelf. The look step comes and says where to set the mode up later.
    pub fn kids_no_games(&mut self) {
        self.kids_wanted = false;
        self.kids_later = true;
        self.enter_look();
        self.remember_step(false);
    }

    /// The first start's look step asked for the next (true) or previous style on its Style row, or colours on its Colours
    /// row, once; the app steps them as Settings › Look › Style and Colours do.
    pub fn take_font_step(&mut self) -> Option<bool> {
        self.font_step.take()
    }

    pub fn take_colours_step(&mut self) -> Option<bool> {
        self.colours_step.take()
    }

    pub fn take_tour(&mut self) -> Option<(Switches, Vec<String>)> {
        std::mem::take(&mut self.tour_changed).then(|| (self.switches.clone(), self.later.clone()))
    }

    /// Opens the first start at "How do you like to play?" (for the snapshots).
    pub fn at_play(mut self) -> Self {
        self.step = Step::Play { sel: 0 };
        self
    }

    /// Opens the first start at a stage (for the snapshots): "wifi", "games" (your games), "look".
    pub fn at_stage(mut self, stage: &str) -> Self {
        match stage {
            "wifi" => {
                self.step = Step::Wifi;
                self.guess_zone();
            }
            "games" => self.enter_found(),
            "look" => self.enter_look(),
            _ => {}
        }
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

    /// Opens Systems on the shelf straight, as its own row of Settings → Look does: B there goes back to Settings.
    /// After `only(Only::Look)`.
    pub fn at_shelf_systems(mut self) -> Self {
        self.direct = true;
        self.step = Step::ShelfSystems { sel: 0 };
        self
    }

    /// Where B on Systems on the shelf goes: the Look step, or out of the setup when the page was opened straight
    /// (`at_shelf_systems`).
    fn back_to_look(&self) -> Step {
        if self.direct {
            Step::Done
        } else {
            Step::Look
        }
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
            Only::Extras => {
                self.step = Step::Extras { sel: 0 };
                if ports::trimui_standalone() { self.check_portmaster(); }
                self.ip = wifi::ip();
                #[cfg(feature = "brick")]
                if extras_cheats() {
                    if let Some(card) = crate::update::brick_card() {
                        let mut page = crate::cheat_data::Page::new(crate::cheat_data::Source::for_card(&card));
                        page.inspect();
                        self.cheat_data = Some(page);
                    }
                }
            }
            Only::Games => self.go_to_games(),
            Only::Play => {
                let now = Profile::of(&self.switches);
                self.step = Step::Play { sel: self.profiles().iter().position(|p| *p == now).unwrap_or(0) };
            }
            Only::Remote => {
                self.look_remote();
                self.ip = wifi::ip();
                self.step = Step::Remote;
            }
        }
        self
    }

    /// The first start is past the step where box art is switched (Add games), or this is a page opened from Settings:
    /// automatic cover downloads may start. They wait for that step so that the choice, Off included, comes first; the
    /// name is from when they started right after the Wi-Fi step.
    pub fn past_wifi(&self) -> bool {
        self.only.is_some() || matches!(self.step, Step::Look | Step::Done)
    }

    /// The appearance in use when the setup opens.
    /// The Wi-Fi page with a made-up neighbourhood and saved networks, for `--snapshot setup:wifi`: no scan runs.
    pub fn with_wifi_demo(self) -> Self {
        let nets = wifi::parse_scan("Home:78:WPA2:*\nCafe guest:41::\nNeighbour\\:5G:30:WPA2: \n");
        self.with_net_demo(Snapshot { wanted: Some(true), radio: Radio::On, scan: Scan::Found(nets), scan_rev: 1, ..Snapshot::default() })
    }

    /// The Wi-Fi page with one network in reach called `ssid`, for `--snapshot setup:wifi-named=<ssid>`: a name is
    /// painted as it is, never translated.
    pub fn with_named_network_demo(self, ssid: &str) -> Self {
        let nets = wifi::parse_scan(&format!("{}:60:WPA2:\n", ssid.replace(':', "\\:")));
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
        matches!(self.step, Step::Look).then_some(self.theme)
    }

    /// The look an answer to "How do you like to play?" asks for since the last time, once; the app takes it.
    pub fn take_profile_look(&mut self) -> Option<usize> {
        self.profile_look.take()
    }

    /// "How do you like to play?" answered (None: skipped). Easy takes its look; another answer after going back puts the
    /// one from the look step back.
    fn answer_look(&mut self, p: Option<Profile>) {
        let wanted = p.and_then(Profile::look).and_then(|t| crate::settings::Theme::ALL.iter().position(|a| *a == t));
        match (wanted, self.look_before_profile) {
            (Some(i), before) => {
                self.look_before_profile = before.or(Some(self.theme));
                self.theme = i;
                self.profile_look = Some(i);
            }
            (None, Some(before)) => {
                self.look_before_profile = None;
                self.theme = before;
                self.profile_look = Some(before);
            }
            (None, None) => {}
        }
    }

    /// Whether "How do you like to play?" is open: the app lends it the look previews (`play_previews`).
    pub fn at_play_step(&self) -> bool {
        matches!(self.step, Step::Play { .. })
    }

    /// The network being brought up has been "Connecting…" for longer than `CONNECT_STUCK`.
    fn stuck(&self) -> bool {
        self.connecting_since.as_ref().is_some_and(|(_, since)| since.elapsed() >= CONNECT_STUCK)
    }

    /// The Wi-Fi list is still being read. Other pages may intentionally stay
    /// loading while a player can inspect or leave them; that is not scan readiness.
    pub fn wifi_scanning(&self) -> bool {
        matches!(self.step, Step::Wifi) && self.scanning
    }

    /// True while something on screen moves (scan or connect in progress).
    pub fn animating(&self) -> bool {
        self.scanning
            || self.cheat_data.as_ref().is_some_and(|p| p.busy())
            || self.switching.is_some()
            || self.sharing_busy
            || self.remote_busy
            || self.share_looking
            || self.remote_looking
            || self.covers_running
            || self.catalog_loading
            || matches!(&self.step, Step::Zone(z) if z.loading)
            || self.port_busy.is_some()
            || matches!(self.portmaster, Some(PortMasterState::Checking | PortMasterState::Downloading(_)))
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

    /// First start: a connection started here worked. The page shows "Connected to …" and, after `ADVANCE_AFTER`, the
    /// setup goes on to the time zone without a press (`SetupEvent::Advance`). Nothing happens when the player has left
    /// the connecting page, or the connect was not started here.
    fn connected_here(&mut self, ssid: &str, was_connecting: bool) {
        let armed = std::mem::take(&mut self.advance_armed);
        if !(armed && was_connecting && self.only.is_none()) {
            return;
        }
        self.advance_gen += 1;
        self.advance_pending = Some(ssid.to_string());
        let (notify, gen) = (self.notify.clone(), self.advance_gen);
        let started = std::thread::Builder::new().name("wifi-advance".into()).spawn(move || {
            std::thread::sleep(ADVANCE_AFTER);
            notify(SetupEvent::Advance { gen });
        });
        if started.is_err() {
            self.advance_pending = None;
        }
    }

    fn connect(&mut self, ssid: String, psk: Option<String>) {
        self.connect_to(ssid, psk, false);
    }

    fn connect_to(&mut self, ssid: String, psk: Option<String>, hidden: bool) {
        let notify = self.notify.clone();
        let target = ssid.clone();
        self.connect_hidden = hidden;
        self.advance_armed = self.only.is_none();
        self.advance_pending = None;
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
        self.advance_armed = self.only.is_none();
        self.advance_pending = None;
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
    /// `wifi_tail`.
    fn wifi_rows(&self) -> usize {
        if self.radio == Radio::Absent {
            return self.wifi_tail().len();
        }
        self.lead() + self.entries.len() + self.wifi_tail().len()
    }

    /// The rows after the networks: Other network, Wi-Fi country, the first start's Time zone, Scan again (X's, as a
    /// row) and the first start's Continue, so that every way on is a row Down reaches.
    fn wifi_tail(&self) -> Vec<WifiRow> {
        // No Wi-Fi chip after all: nothing to choose, only the way on (the first start).
        if self.radio == Radio::Absent {
            return if self.only.is_none() { vec![WifiRow::Continue] } else { Vec::new() };
        }
        let mut rows = vec![WifiRow::Other, WifiRow::Country];
        if self.zone_row() {
            rows.push(WifiRow::Zone);
        }
        if !self.radio.is_off() {
            rows.push(WifiRow::Scan);
        }
        if self.only.is_none() {
            rows.push(WifiRow::Continue);
        }
        rows
    }

    /// The rows of a saved network's menu: Connect (Continue on the first start's connected one; nothing in Settings
    /// on the connected one, which is connected already), then Forget.
    fn net_menu_rows(&self, ssid: &str) -> Vec<NetAction> {
        let active = self.entries.iter().any(|e| e.ssid == ssid && e.active);
        match (active, self.only.is_none()) {
            (true, true) => vec![NetAction::Continue, NetAction::Forget],
            (true, false) => vec![NetAction::Forget],
            (false, _) => vec![NetAction::Connect, NetAction::Forget],
        }
    }

    /// A saved-network sheet covers its Wi-Fi list; the navigation explorer checks Back at both levels.
    pub fn network_menu_open(&self) -> bool {
        self.net_menu.is_some()
    }

    /// A key on a saved network's menu: Up/Down, A does the row, B closes it.
    fn press_net_menu(&mut self, b: Button) {
        let Some((ssid, sel)) = self.net_menu.clone() else { return };
        let rows = self.net_menu_rows(&ssid);
        match b {
            Button::Up => self.net_menu = Some((ssid, sel.saturating_sub(1))),
            Button::Down => self.net_menu = Some((ssid, (sel + 1).min(rows.len() - 1))),
            Button::South => self.net_menu = None,
            Button::East => {
                self.net_menu = None;
                let saved = self.entries.iter().find(|e| e.ssid == ssid).and_then(|e| e.saved.clone());
                match (rows.get(sel), saved) {
                    (Some(NetAction::Continue), _) => self.leave_wifi(),
                    (Some(NetAction::Connect), Some(saved)) => self.connect_saved(saved),
                    (Some(NetAction::Forget), Some(saved)) => {
                        self.note = Some((format!("Forgetting {}…", saved.ssid), false));
                        self.forget_saved(saved);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// A saved network's menu over the dimmed list: its name, then its rows (`net_menu_rows`), as the other small menus
    /// (`kit::sheet`).
    fn paint_net_menu(&self, c: &mut Canvas, text: &mut Text) {
        let Some((ssid, sel)) = &self.net_menu else { return };
        let rows = self.net_menu_rows(ssid);
        c.shade_rect(0, 0, c.w, c.h, pal().void, 150);
        let row = crate::type_scale::row(70, 37);
        let w = (c.w - 2 * MARGIN).min(720);
        let h = 48 + 60 + rows.len() as i32 * row + 40;
        let (x, y) = ((c.w - w) / 2, ((c.h - h) / 2).max(40));
        crate::kit::sheet(c, x, y, w, h);
        // The network's name is its own: painted as it is, never translated.
        text.set_translate(false);
        let name = text.fit(Face::Title, 50, ssid, w - 96);
        text.draw(c, Face::Title, 50, x + 48, y + 48 + 46, pal().chalk, &name);
        text.set_translate(true);
        for (i, r) in rows.iter().enumerate() {
            let ry = y + 48 + 60 + 20 + i as i32 * row;
            let selected = i == *sel;
            crate::kit::shelf_row(c, x + 24, ry, w - 48, row - 8, selected);
            let label = match r {
                NetAction::Connect => "Connect",
                NetAction::Continue => "Continue",
                NetAction::Forget => "Forget",
            };
            text.draw(c, Face::Title, 37, x + 52, ry + (row - 8) / 2 + crate::type_scale::base(13, 37), if selected { pal().ink } else { pal().chalk }, label);
        }
        let a = match rows.get(*sel) {
            Some(NetAction::Connect) => "Connect",
            Some(NetAction::Continue) => "Continue",
            _ => "Forget",
        };
        hints(c, text, &[("A", a), ("B", "Back")]);
    }

    /// The tail row at the selection, if it is one.
    fn wifi_tail_at(&self, row: Option<usize>) -> Option<WifiRow> {
        row.and_then(|r| r.checked_sub(self.entries.len())).and_then(|t| self.wifi_tail().get(t).copied())
    }

    /// The first start's Wi-Fi page ends with a Time zone row: the zone is guessed after Wi-Fi, and this is the way to
    /// choose it yourself (Settings › System › Time later).
    fn zone_row(&self) -> bool {
        self.only.is_none()
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
        self.keep_share_watch();
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

    /// While a page with the file sharing switch is up, a thread looks at the service every `share_every`: it also comes
    /// back by itself after a restart (`wifi::keep_sharing_on`), and X has to do what the page says. It ends with the
    /// page (it holds a `Weak` of the token); the answers come as `SetupEvent::ShareChanged`, only when something changed.
    fn keep_share_watch(&mut self) {
        if !matches!(self.step, Step::Games | Step::PortFiles(_)) {
            self.share_watch = None;
            return;
        }
        if self.share_watch.is_some() {
            return;
        }
        let token = Arc::new(());
        let alive = Arc::downgrade(&token);
        self.share_watch = Some(token);
        let (notify, every, ask) = (self.notify.clone(), self.share_every, self.share_ask);
        let started = std::thread::Builder::new().name("share-page-look".into()).spawn(move || {
            let mut seen = None;
            loop {
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
                let now = ask();
                if seen.as_ref() != Some(&now) {
                    seen = Some(now.clone());
                    notify(SetupEvent::ShareChanged { now, password: wifi::share_password() });
                }
            }
        });
        if started.is_err() {
            self.share_watch = None;
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
                        let was_connecting = matches!(self.step, Step::Connecting { .. });
                        self.connected_here(&ssid, was_connecting);
                        self.step = Step::Wifi;
                        if !self.zone_guess.as_ref().is_some_and(|z| z.clear()) {
                            self.guess_zone();
                        }
                    }
                    // The saved password is no longer right (or the profile lost it): ask for it.
                    Err(e) if wifi::wrong_password(&e) => {
                        self.advance_armed = false;
                        eprintln!("wifi: connection failed: {e}");
                        self.note = Some(("The saved password didn't work. Type it again.".into(), true));
                        self.step = Step::Password { ssid, kb: Keyboard::default(), hidden: false };
                        return;
                    }
                    Err(e) => {
                        self.advance_armed = false;
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
            SetupEvent::Advance { gen } => {
                // Still the same wait, still on the Wi-Fi page, nothing pressed, and nothing to sign in to (the connect
                // itself worked; the list may not show it yet): on to the time zone, as START would.
                let up = !self.portal_shown() && !self.radio.is_off();
                if self.advance_pending.is_some() && gen == self.advance_gen && matches!(self.step, Step::Wifi) && up {
                    self.advance_pending = None;
                    self.leave_wifi();
                    self.remember_step(false);
                } else if gen == self.advance_gen {
                    self.advance_pending = None;
                }
            }
            SetupEvent::Connected { ssid, result } => {
                if result.is_ok() {
                    net_check::forget();
                    self.portal = false;
                    // A new network usually means a new address; smbd has to follow it.
                    std::thread::spawn(wifi::rebind_sharing);
                }
                let refused = result.as_ref().is_err_and(|e| wifi::wrong_password(e));
                if result.is_ok() {
                    let was_connecting = matches!(self.step, Step::Connecting { .. });
                    self.connected_here(&ssid, was_connecting);
                    // The Wi-Fi chip may know the country now: an unsure time zone is guessed again.
                    if !self.zone_guess.as_ref().is_some_and(|z| z.clear()) {
                        self.guess_zone();
                    }
                } else {
                    self.advance_armed = false;
                }
                self.note = Some(match result {
                    Ok(()) => (format!("Connected to {ssid}"), false),
                    Err(e) => (friendly_error(&e), true),
                });
                // A refused password: the keyboard again with the reason above it (`wifi::connect_with` has removed the
                // refused profile, so what is typed now is what is tried). What was typed stays: one wrong letter of a
                // long password shouldn't cost the whole of it again, with a D-pad (new-player walk 2026-11-02).
                let typed = std::mem::take(&mut self.typed_password);
                self.step = if refused { Step::Password { ssid, kb: Keyboard { text: typed, ..Keyboard::default() }, hidden: self.connect_hidden } } else { Step::Wifi };
                // The scan that follows is `NetState::exclusive`'s own: asking again would run it twice.
            }
            SetupEvent::Sharing { on, read_only, password, switched } => {
                if switched {
                    self.sharing_busy = false;
                } else {
                    self.share_looking = false;
                }
                self.sharing = on;
                self.share_read_only = read_only;
                self.share_password = password;
            }
            SetupEvent::ShareChanged { now, password } => {
                self.sharing = now.on;
                self.share_read_only = now.read_only;
                self.ip = now.ip;
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
                if let Step::Zone(z) = &self.step {
                    if z.loading {
                        let auto = z.auto;
                        self.step = Step::Zone(Box::new(facts.0));
                        // After Wi-Fi: a zone the system has or the country's one zone is taken as it is, unseen.
                        if auto && matches!(&self.step, Step::Zone(z) if matches!(z.guess, Guess::System(_) | Guess::Country { .. })) {
                            self.leave_zone(None);
                            self.remember_step(false);
                        }
                    }
                }
            }
            SetupEvent::ZoneGuess(facts) => {
                self.zone_guessing = false;
                // Picked on the row while the guess was being made: the pick wins (set again, the guess's thread may have
                // set the country's zone after it).
                if let Some(picked) = self.zone_picked.clone() {
                    if facts.0.guess.zone() != Some(picked.as_str()) && matches!(facts.0.guess, Guess::Country { .. }) {
                        let _ = self.hw.set_timezone(&picked);
                    }
                    return;
                }
                self.zone_guess = Some(Box::new(facts.0));
            }
            SetupEvent::ZoneOffsets(found) => {
                if let Step::Zone(z) = &mut self.step {
                    z.offsets.extend(found);
                }
            }
            // Painted from the server's status; the event only asks for a frame.
            SetupEvent::Phone => {}
            SetupEvent::GameCounts(found) => {
                self.counting = false;
                self.games_counted = Some(found.games.values().sum());
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
            SetupEvent::CoversUnreachable => self.covers_unreachable = true,
            SetupEvent::PortMasterStatus(result) => match result {
                Ok(status) => {
                    self.portmaster_status = status;
                    self.portmaster = Some(if status == ports::RuntimeStatus::Ready { PortMasterState::Ready } else { PortMasterState::Offer(status) });
                    if status == ports::RuntimeStatus::Ready && matches!(self.step, Step::Ports) { self.load_catalog(); }
                }
                Err(error) => {
                    self.portmaster = Some(PortMasterState::Offer(ports::RuntimeStatus::Missing));
                    self.portmaster_note = Some(error);
                }
            },
            SetupEvent::PortMasterProgress(progress) => self.portmaster = Some(PortMasterState::Downloading(progress)),
            SetupEvent::PortMasterSpace(space) => self.portmaster_space = space.ok(),
            SetupEvent::PortMasterRemoved(result) => {
                self.port_download_cancel = None;
                self.portmaster_removing = false;
                self.portmaster_confirm_remove = result.is_err();
                if result.is_ok() {
                    self.portmaster_status = ports::RuntimeStatus::Missing;
                    self.portmaster = Some(PortMasterState::Offer(ports::RuntimeStatus::Missing));
                    self.portmaster_note = None;
                } else {
                    self.portmaster = Some(PortMasterState::Ready);
                    self.portmaster_note = result.err();
                }
            }
            SetupEvent::PortMasterDone(result) => {
                self.port_download_cancel = None;
                match result {
                    Ok(()) => {
                        self.portmaster_status = ports::RuntimeStatus::Ready;
                        self.portmaster = Some(PortMasterState::Ready);
                        self.portmaster_note = None;
                        if matches!(self.step, Step::Ports) { self.load_catalog(); }
                    }
                    Err(error) => {
                        self.portmaster = Some(PortMasterState::Offer(self.portmaster_status));
                        self.portmaster_note = Some(error);
                    }
                }
            }
            SetupEvent::PortProgress(progress) => self.port_progress = Some(progress),
            SetupEvent::Catalog(result) => {
                self.port_download_cancel = None;
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
                self.port_download_cancel = None;
                self.port_busy = None;
                self.port_progress = None;
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
        self.covers_unreachable = false;
        self.covers = Some((0, 0, 0));
        self.covers_cancel = Default::default();
        let cancel = self.covers_cancel.clone();
        // The automatic download is already on it: the app passes its progress on to this page.
        let Some(claim) = covers::claim() else { return };
        let (notify, data_dir, games, pictures) = (self.notify.clone(), self.data_dir.clone(), self.games.clone(), self.art == Art::Pictures);
        std::thread::spawn(move || {
            let _claim = claim;
            crate::web::lower_priority();
            let report = |done, total, found| notify(SetupEvent::Covers { done, total, found });
            // Box art, then (unless Box art only) the games' screenshots and title screens.
            let wanted = pictures.then(|| game_pictures::missing(&data_dir, &games, &Default::default()));
            // A game started from the shelf while this first start's run goes on stops it (`covers::game_on`).
            let run = game_pictures::run_until(&game_pictures::Libretro, &data_dir, &games, wanted.as_deref(), report,
                || cancel.is_cancelled() || covers::game_on());
            // Nothing to do still has to end the run.
            if games.is_empty() || run.found == 0 || run.unreachable {
                report(1, 1, run.found);
            }
            if run.unreachable {
                notify(SetupEvent::CoversUnreachable);
            }
        });
    }

    fn check_portmaster(&mut self) {
        self.portmaster = Some(PortMasterState::Checking);
        let notify = self.notify.clone();
        std::thread::spawn(move || {
            notify(SetupEvent::PortMasterStatus(ports::runtime_status()));
            notify(SetupEvent::PortMasterSpace(ports::runtime_space()));
        });
    }

    fn portmaster_can_remove(&self) -> bool {
        ports::trimui_standalone() && self.port_download_cancel.is_none()
            && matches!(self.portmaster, Some(PortMasterState::Ready | PortMasterState::Offer(ports::RuntimeStatus::Outdated)))
    }

    fn remove_portmaster(&mut self) {
        if !self.portmaster_confirm_remove || self.port_download_cancel.is_some() { return; }
        self.portmaster_removing = true;
        self.portmaster_note = None;
        let (notify, remove) = (self.notify.clone(), self.portmaster_remove);
        let cancel = ports::DownloadCancel::new();
        self.port_download_cancel = Some(cancel.clone());
        self.portmaster = Some(PortMasterState::Downloading(ports::DownloadProgress { stage: "removing".into(), ..Default::default() }));
        std::thread::spawn(move || {
            let report = { let notify = notify.clone(); move |progress| notify(SetupEvent::PortMasterProgress(progress)) };
            let result = remove(&report, &cancel);
            let removed = result.is_ok();
            notify(SetupEvent::PortMasterRemoved(result));
            if removed { notify(SetupEvent::PortMasterSpace(ports::runtime_space())); }
        });
    }

    fn portmaster_low_space(&self) -> Option<ports::RuntimeSpace> {
        self.portmaster_space.filter(|s| s.available_kib < s.needed_kib)
    }

    fn portmaster_row(&self, online: bool) -> crate::emu_menu::Row {
        let about = "PC and indie games from PortMaster: ready to play, or with your own game files.";
        let (value, desc) = if self.portmaster_removing {
            ("Removing…".into(), "Your games, game files and PortMaster settings stay on this card.".into())
        } else if self.portmaster_confirm_remove {
            ("Remove?".into(), self.portmaster_note.clone().unwrap_or_else(|| "Remove PortMaster? A again removes it. Your games, game files and settings stay.".into()))
        } else {
            match &self.portmaster {
                Some(PortMasterState::Checking) => ("Checking…".into(), about.into()),
                Some(PortMasterState::Downloading(progress)) => {
                    let value = match progress.stage.as_str() {
                        "checking" | "verify" => "Checking…".into(),
                        "saving" | "preparing" | "installing" | "unpacking" | "ready" => "Preparing…".into(),
                        _ if progress.total > 0 => format!("{}%", progress.done.saturating_mul(100).checked_div(progress.total).unwrap_or(0).min(100)),
                        _ => "Downloading…".into(),
                    };
                    (value, crate::cheat_data::CONTINUE.into())
                }
                _ if !online && !matches!(self.portmaster, Some(PortMasterState::Ready)) => ("Needs Wi-Fi".into(), crate::cheat_data::CONNECT.into()),
                _ if self.portmaster_note.is_some() => ("Retry".into(), self.portmaster_note.clone().unwrap()),
                Some(PortMasterState::Offer(_)) if self.portmaster_low_space().is_some() => {
                    let space = self.portmaster_low_space().unwrap();
                    (format!("Needs {} MiB", space.needed_kib.div_ceil(1024)),
                        format!("Not enough space on the SD card: {} MiB needed, {} MiB free.", space.needed_kib.div_ceil(1024), space.available_kib / 1024))
                }
                Some(PortMasterState::Offer(ports::RuntimeStatus::Outdated)) => ("Browse".into(), "An update for PortMaster is available. A opens the offer; X offers to remove it. Your games and game files stay.".into()),
                _ if self.portmaster_can_remove() => ("Browse".into(), "PC and indie games from PortMaster. X offers to remove PortMaster; your games and game files stay.".into()),
                _ => ("Browse".into(), about.into()),
            }
        };
        crate::emu_menu::Row { name: "PortMaster games".into(), value, desc }
    }

    fn enter_ports(&mut self) {
        self.step = Step::Ports;
        if ports::trimui_standalone() && self.portmaster.is_none() { self.check_portmaster(); }
        if self.portmaster_ready() { self.load_catalog(); }
    }

    fn portmaster_ready(&self) -> bool {
        matches!(self.portmaster, None | Some(PortMasterState::Ready))
    }

    fn portmaster_online(&self) -> bool {
        self.portmaster_demo_online.unwrap_or_else(|| self.ip.is_some() || self.net.snapshot().saving.ip.is_some())
    }

    fn portmaster_action(&self) -> &'static str {
        match self.portmaster {
            Some(PortMasterState::Offer(ports::RuntimeStatus::Outdated)) => "Update PortMaster",
            Some(PortMasterState::Checking) => "Checking PortMaster…",
            Some(PortMasterState::Downloading(ref progress)) if matches!(progress.stage.as_str(), "checking" | "verify") => "Checking PortMaster…",
            Some(PortMasterState::Downloading(ref progress)) if matches!(progress.stage.as_str(), "saving" | "preparing" | "installing" | "unpacking" | "ready") => "Preparing PortMaster…",
            Some(PortMasterState::Downloading(_)) => "Downloading PortMaster…",
            _ => "Get PortMaster (~300 MB, one time)",
        }
    }

    fn portmaster_progress_line(&self) -> String {
        let Some(PortMasterState::Downloading(progress)) = &self.portmaster else { return String::new(); };
        if progress.total > 0 { format!("{} / {} MB", progress.done / (1 << 20), progress.total / (1 << 20)) } else if progress.done > 0 { ports::size_label(progress.done) } else { "Preparing PortMaster…".to_string() }
    }

    fn start_portmaster(&mut self) {
        if !matches!(self.portmaster, Some(PortMasterState::Offer(_))) { return; }
        // Offline the offer already explains Wi-Fi and advertises only B. A must leave that view unchanged.
        if !self.portmaster_online() { return; }
        if let Some(space) = self.portmaster_low_space() {
            self.portmaster_note = Some(format!("Not enough space on the SD card: {} MiB needed, {} MiB free.", space.needed_kib.div_ceil(1024), space.available_kib / 1024));
            return;
        }
        self.portmaster = Some(PortMasterState::Downloading(Default::default()));
        self.portmaster_note = None;
        let (notify, fetch) = (self.notify.clone(), self.portmaster_fetch);
        let cancel = ports::DownloadCancel::new();
        self.port_download_cancel = Some(cancel.clone());
        std::thread::spawn(move || {
            let report = { let notify = notify.clone(); move |progress| notify(SetupEvent::PortMasterProgress(progress)) };
            notify(SetupEvent::PortMasterDone(fetch(&report, &cancel)));
        });
    }

    pub fn with_portmaster_demo(mut self, state: &str) -> Self {
        self.portmaster = Some(match state {
            "checking" => PortMasterState::Checking,
            "download" => PortMasterState::Downloading(ports::DownloadProgress { stage: "download".into(), done: 70 << 20, total: 300 << 20 }),
            "update" => PortMasterState::Offer(ports::RuntimeStatus::Outdated),
            _ => PortMasterState::Offer(ports::RuntimeStatus::Missing),
        });
        self.portmaster_demo_online = Some(state != "offline");
        self.portmaster_note = (state == "error").then(|| "Not enough card space. Free some space and try again.".into());
        self
    }

    pub fn with_extras_portmaster_demo(self, state: &str) -> Self {
        let mut page = self.with_portmaster_demo(state);
        page.step = Step::Extras { sel: 2 };
        page.extras_demo_online = Some(!state.ends_with("offline"));
        page.portmaster_status = match state {
            "installed" | "installed-offline" | "remove" | "removing" => ports::RuntimeStatus::Ready,
            "update" => ports::RuntimeStatus::Outdated,
            _ => ports::RuntimeStatus::Missing,
        };
        if matches!(state, "installed" | "installed-offline" | "remove") { page.portmaster = Some(PortMasterState::Ready); }
        if state == "saving" { page.portmaster = Some(PortMasterState::Downloading(ports::DownloadProgress { stage: "saving".into(), ..Default::default() })); }
        page.portmaster_confirm_remove = state == "remove";
        page.portmaster_removing = state == "removing";
        if state == "no-space" { page.portmaster_space = Some(ports::RuntimeSpace { needed_kib: 716800, available_kib: 421888 }); }
        page
    }

    fn load_catalog(&mut self) {
        if self.catalog_loading || !self.ports.is_empty() {
            return;
        }
        self.catalog_loading = true;
        let (notify, data_dir) = (self.notify.clone(), self.data_dir.clone());
        let cancel = ports::DownloadCancel::new();
        self.port_download_cancel = Some(cancel.clone());
        std::thread::spawn(move || notify(SetupEvent::Catalog(ports::catalog_with_cancel(&data_dir, &cancel))));
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
        self.port_progress = None;
        let notify = self.notify.clone();
        let cancel = ports::DownloadCancel::new();
        self.port_download_cancel = Some(cancel.clone());
        std::thread::spawn(move || {
            let report = { let notify = notify.clone(); move |progress| notify(SetupEvent::PortProgress(progress)) };
            let result = if installing { ports::install_with_cancel(&port, &report, &cancel) } else { ports::uninstall_with_cancel(&port, &cancel) };
            notify(SetupEvent::PortDone { name: port.name, title: port.title, result, installing, updated: updating });
        });
    }

    /// "Copy your game files" for `port`; the folder it names is made once it is installed.
    fn open_port_files(&mut self, port: &Port) {
        let page = FilesPage::new(&port.title, &port.dir, &port.instructions, std::path::Path::new(ports::PORTS_DIR));
        if ports::installed(port) {
            let dir = std::path::Path::new(ports::PORTS_DIR).join(&port.dir);
            port_files::make_place(&dir, &port_files::expect(&port.instructions, &port.dir));
        }
        self.look_sharing();
        self.ip = wifi::ip();
        self.step = Step::PortFiles(Box::new(page));
    }

    /// File sharing as the "Copy your game files" page shows it.
    fn files_share(&self) -> Share {
        Share { on: self.sharing, ip: self.ip.clone(), password: self.share_password.clone() }
    }

    #[cfg(not(feature = "brick"))]
    fn ways(&self) -> Vec<Way> {
        let d = crate::device::current();
        ways_for(d.has_wifi())
    }

    #[cfg(feature = "brick")]
    fn ways(&self) -> Vec<Way> {
        let d = crate::device::current();
        let mut ways = ways_for(d.has_wifi());
        #[cfg(feature = "brick")]
        if matches!(d.model, Some(crate::device::Model::Brick(_))) && !crate::brick_profile::file_sharing() {
            ways.retain(|w| *w != Way::Sharing);
        }
        ways
    }


    /// The grid moves; Down from its last row goes on to the row of ways. True when the button was used.
    fn press_grid(&mut self, b: Button) -> bool {
        let has_ways = !self.ways().is_empty();
        let Some(g) = self.grid.as_mut() else { return false };
        let before = g.sel;
        let used = g.press(b);
        if used && b == Button::Down && g.sel == before && has_ways {
            self.ways_focus = true;
        }
        used
    }

    /// With the focus on the row of ways: Left and Right choose, A does it, Up goes back to the grid. True when used.
    fn press_ways(&mut self, b: Button) -> bool {
        let n = self.ways().len();
        if n == 0 {
            self.ways_focus = false;
            return false;
        }
        self.ways_sel = self.ways_sel.min(n - 1);
        match b {
            Button::Left => self.ways_sel = self.ways_sel.saturating_sub(1),
            Button::Right => self.ways_sel = (self.ways_sel + 1).min(n - 1),
            Button::Up | Button::L1 => self.ways_focus = false,
            Button::East => self.activate_way(),
            // Nothing below the row.
            Button::Down => {}
            _ => return false,
        }
        true
    }

    /// A on a way. File sharing without an address leads to the Wi-Fi page: the first start's step, or from Settings the
    /// Wi-Fi settings (`take_open_wifi`).
    fn activate_way(&mut self) {
        match self.ways().get(self.ways_sel) {
            Some(Way::Sharing) if self.ip.is_none() && !self.sharing => self.to_wifi(),
            Some(Way::Phone) if self.ip.is_none() => {
                self.wifi_for_phone = self.only.is_none();
                self.to_wifi();
            }
            Some(Way::Phone) => self.open_phone(),
            Some(Way::Sharing) => self.switch_sharing(),
            None => {}
        }
    }

    /// The Wi-Fi page: the first start's step, or from Settings the Wi-Fi settings (`take_open_wifi`).
    fn to_wifi(&mut self) {
        if self.only.is_none() {
            // Asked for to connect: the networks first, not the Continue row it may have been left on.
            self.sel = 0;
            self.step = Step::Wifi;
            self.scan();
        } else {
            let page = matches!(self.step, Step::Phone(_));
            let phone = page || self.ways().get(self.ways_sel).filter(|_| self.ways_focus) == Some(&Way::Phone);
            self.open_wifi = Some((phone, page));
            self.step = Step::Done;
        }
    }

    /// The phone page or a way's tile without Wi-Fi asked for the Wi-Fi settings (opened from Settings: the app opens
    /// them): (from the From your phone tile, from the phone page itself).
    pub fn take_open_wifi(&mut self) -> Option<(bool, bool)> {
        self.open_wifi.take()
    }

    /// Settings › Wi-Fi opened from Add games (`take_open_wifi`'s answer): leaving it goes back there.
    pub fn back_to_games(mut self, from: (bool, bool)) -> Self {
        self.back_to_games = Some(from);
        self
    }

    /// Where leaving this Wi-Fi page goes: Add games on the tile, and its phone page, it came from; None: Settings.
    pub fn take_back_to_games(&mut self) -> Option<(bool, bool)> {
        self.back_to_games.take()
    }

    /// Add games back from the Wi-Fi settings one of its ways asked for (`take_open_wifi`): the focus on that way's tile
    /// (From your phone, or File sharing), and the phone page open again when it was the one asking.
    pub fn back_on_way(mut self, (phone, page): (bool, bool)) -> Self {
        let want = if phone { Way::Phone } else { Way::Sharing };
        if let Some(i) = self.ways().iter().position(|w| *w == want).filter(|_| matches!(self.step, Step::Games)) {
            (self.ways_focus, self.ways_sel) = (true, i);
            if page {
                self.open_phone();
            }
        }
        self
    }


    /// A tap on a tile of the row of ways moves the selection there by `n` (0: the one that is, now with the focus).
    pub fn ways_steps(&mut self, n: i32) {
        if let Step::Extras { sel } = &mut self.step {
            *sel = (*sel as i32 + n).clamp(0, extras_rows() as i32 - 1) as usize;
            return;
        }
        let len = self.ways().len();
        if len > 0 {
            self.ways_sel = (self.ways_sel as i32 + n).clamp(0, len as i32 - 1) as usize;
            self.ways_focus = true;
        }
    }

    /// Whether the focus is on the row of ways (a tap on the selected tile then does it).
    pub fn ways_focused(&self) -> bool {
        self.ways_focus || matches!(self.step, Step::Extras { .. })
    }

    /// Add games takes touch: its tiles of ways and its hints.
    pub fn takes_touch(&self) -> bool {
        matches!(self.step, Step::Games | Step::Extras { .. })
    }

    /// The last frame's touch targets, into the app's map: the ways as rows of a list, the hints as buttons.
    pub fn register_touch(&self, hits: &mut crate::touch::HitMap) {
        if let Step::Extras { sel } = self.step {
            for &(area, target) in self.extras_touch.borrow().targets() {
                match target {
                    crate::touch::Target::Row(i) => hits.row(area, i),
                    crate::touch::Target::Button(b) => hits.button(area, b),
                    crate::touch::Target::Presses(b, n) => hits.presses(area, b, n),
                }
            }
            hits.list(sel, extras_rows(), 0.0, 0.0);
            return;
        }
        // Only the step that takes touch: the other steps' frames would keep Add games' tiles in the map.
        if !self.takes_touch() {
            return;
        }
        for (area, t) in self.touch.borrow().iter() {
            match t {
                Touch::Way(i) => hits.row(*area, *i),
                Touch::Hint(b) => hits.button(*area, *b),
            }
        }
        hits.list(self.ways_sel, self.ways().len(), 0.0, 0.0);
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
            notify(SetupEvent::Sharing { on: wifi::sharing_enabled(), read_only: wifi::share_read_only(), password: wifi::share_password(), switched: true })
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
            notify(SetupEvent::Sharing { on, read_only: wifi::share_read_only(), password: wifi::share_password(), switched: false })
        });
    }

    /// `--snapshot`: what `look_sharing` and `look_remote` would find, read here at once (never so in the running app).
    pub fn looked_now(&mut self) {
        if std::mem::take(&mut self.share_looking) {
            self.sharing = wifi::sharing_enabled();
            self.share_read_only = wifi::share_read_only();
            self.share_password = wifi::share_password();
        }
        if std::mem::take(&mut self.remote_looking) {
            self.remote = remote::current();
            self.remote_password = remote::password();
        }
        if matches!(&self.step, Step::Zone(z) if z.loading) {
            self.on_event(SetupEvent::Zone(ZoneFacts(look_zone(&*self.hw))));
        }
        if self.zone_guessing {
            self.on_event(SetupEvent::ZoneGuess(ZoneFacts(look_zone(&*self.hw))));
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
        self.portmaster = None;
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
            Step::Welcome => (0u8, crate::i18n::current(), self.welcome_row, self.kids_wanted, crate::type_scale::get()).hash(&mut h),
            Step::Look => (1u8, self.appearance, self.motion, self.look_row, self.animations_shown(), self.theme).hash(&mut h),
            Step::Play { sel } => (15u8, sel).hash(&mut h),
            Step::Cards(c) => (16u8, c.mark()).hash(&mut h),
            Step::Wifi => (2u8, &self.forget, &self.net_menu).hash(&mut h),
            Step::Password { ssid, kb, .. } => (3u8, ssid, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            Step::HiddenName { kb } => (13u8, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            Step::Country { sel, .. } => (14u8, sel).hash(&mut h),
            Step::Connecting { ssid } => (4u8, ssid).hash(&mut h),
            Step::Portal(p) => (17u8, p.mark()).hash(&mut h),
            Step::Extras { sel } => (5u8, sel).hash(&mut h),
            Step::Ports => (6u8, self.port_tab).hash(&mut h),
            Step::PortFiles(page) => (12u8, page.scroll()).hash(&mut h),
            Step::Games => 7u8.hash(&mut h),
            Step::Found { sel } => (19u8, sel, self.art == Art::Off, self.games_counted).hash(&mut h),
            Step::Phone(_) => 18u8.hash(&mut h),
            Step::Remote => 8u8.hash(&mut h),
            Step::Done => 9u8.hash(&mut h),
            Step::ShelfSystems { sel } => (11u8, sel, &self.hidden, self.streaming).hash(&mut h),
            Step::Zone(z) => match &z.view {
                ZoneView::Card => (10u8, 0u8).hash(&mut h),
                ZoneView::Regions { sel } => (10u8, 1u8, sel).hash(&mut h),
                ZoneView::List { heading, sel, .. } => (10u8, 2u8, heading, sel).hash(&mut h),
                ZoneView::Search(kb) => (10u8, 3u8, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            },
        }
        (self.sel, self.theme, self.port_sel, self.port_tab, self.sharing, self.remote_busy, self.sharing_busy).hash(&mut h);
        (self.share_read_only, &self.ip, &self.share_password).hash(&mut h);
        (self.ways_sel, self.ways_focus).hash(&mut h);
        self.cheat_data.as_ref().map(|p| p.status()).hash(&mut h);
        (self.portmaster_confirm_remove, self.portmaster_removing).hash(&mut h);
        h.finish()
    }

    /// Where the setup is, as a short word; for tests and the navigation explorer (`app_nav_probe`).
    pub fn at(&self) -> String {
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
            Step::Found { .. } => "your games",
            Step::Phone(_) => "phone",
            Step::Remote => "remote",
            Step::Done => "done",
            Step::ShelfSystems { .. } => "shelf systems",
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
        matches!(self.portmaster, Some(PortMasterState::Downloading(_))) || self.counting || self.zone_guessing || matches!(&self.step, Step::Zone(z) if z.loading || z.asked.iter().any(|a| !z.offsets.contains_key(a)))
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

    /// On from the play style (Settings › Help & tools › How you like to play): the answer is all there is to do.
    fn after_play(&mut self) {
        if self.only.is_some() {
            self.step = Step::Done;
        } else {
            self.after_welcome();
        }
    }

    /// On from the welcome: Wi-Fi; the time zone where there is no Wi-Fi chip; your games where the system owns both
    fn after_welcome(&mut self) {
        if self.no_wifi {
            self.enter_zone();
        } else {
            self.step = Step::Wifi;
            self.scan();
            self.guess_zone();
        }
    }

    /// On from Wi-Fi in the first start: a time zone guess that can be trusted was taken already (the Time zone row says
    /// it) and the games come next; an unsure one opens the time zone step, which can be skipped.
    /// Sent here from the phone page: back to it instead (`back_to_phone`). The time zone is asked once only.
    fn leave_wifi(&mut self) {
        if std::mem::take(&mut self.wifi_for_phone) {
            self.back_to_phone();
            return;
        }
        let unsure = self.zone_picked.is_none() && !self.zone_guess.as_ref().is_some_and(|z| z.clear()) && !self.zone_asked;
        if !unsure {
            self.enter_found();
        } else if let Some(z) = self.zone_guess.take() {
            let mut z = *z;
            z.view = z.start_view();
            self.step = Step::Zone(Box::new(z));
            self.zone_offsets_wanted();
        } else {
            // No guess yet (its thread is slow): the step looks itself and takes a clear answer without being seen.
            self.enter_zone_auto();
        }
    }

    /// Back from the time zone: the Wi-Fi page, or the welcome where there is no Wi-Fi page.
    fn zone_back(&mut self) {
        if std::mem::take(&mut self.zone_from_wifi) {
            self.zone_back_to_wifi();
        } else {
            self.step = if self.no_wifi { Step::Welcome } else { Step::Wifi };
        }
    }

    /// Back on the Wi-Fi page from its Time zone row: what the step read (the zones, the offsets) is kept for the row and
    /// the next time it opens.
    fn zone_back_to_wifi(&mut self) {
        if let Step::Zone(z) = std::mem::replace(&mut self.step, Step::Wifi) {
            if !z.loading {
                self.zone_guess = Some(z);
            }
        }
    }

    /// On from the time zone: your games, or back to the Wi-Fi page it was opened from (Settings: the page closes).
    fn after_zone(&mut self) {
        if std::mem::take(&mut self.zone_from_wifi) {
            self.zone_back_to_wifi();
        } else {
            self.zone_asked = true;
            self.enter_found();
        }
    }

    /// Wi-Fi was opened for the phone page: Add games again, and the phone page itself once there is an address (online
    /// it shows its code at once; offline, B on Wi-Fi, the grid with Send from your phone still chosen).
    fn back_to_phone(&mut self) {
        self.go_to_games();
        if self.ip.is_some() {
            self.open_phone();
        }
    }

    /// The first start's "Your games": the folders are counted again on a thread, the page says so meanwhile.
    fn enter_found(&mut self) {
        self.go_to_games();
        self.step = Step::Found { sel: 0 };
    }

    /// Your games is left with Continue (or skipped): Kids Mode's steps where a child was named and there are games,
    /// otherwise the look.
    fn leave_games(&mut self) {
        match (self.kids_wanted, self.games_known() > 0) {
            (true, true) => self.kids_asked = true,
            (true, false) => {
                self.kids_wanted = false;
                self.kids_later = true;
                self.enter_look();
            }
            _ => self.enter_look(),
        }
    }

    /// The look step.
    fn enter_look(&mut self) {
        self.step = Step::Look;
    }

    /// The time zone after Wi-Fi: looked at on a thread like `enter_zone`, and a guess that can be trusted is taken
    /// without the step being seen (`SetupEvent::Zone`).
    fn enter_zone_auto(&mut self) {
        self.enter_zone();
        if let Step::Zone(z) = &mut self.step {
            z.auto = true;
        }
    }

    /// The first start's Wi-Fi page: the time zone guessed on a thread (`look_zone`), again after a connection (the
    /// Wi-Fi chip learns the country from the network). A clear guess is set at once and shown on the Time zone row.
    fn guess_zone(&mut self) {
        if self.only.is_some() || self.zone_guessing || self.zone_picked.is_some() {
            return;
        }
        self.zone_guessing = true;
        let (hw, notify) = (self.hw.clone(), self.notify.clone());
        // A country's one zone is set right there, off the UI thread (the zone the system has needs nothing); a refusal
        // makes it a guess to ask about.
        std::thread::spawn(move || {
            let mut z = look_zone(&*hw);
            if let Guess::Country { zone, .. } = &z.guess {
                if hw.timezone().as_deref() != Some(zone.as_str()) {
                    match hw.set_timezone(zone) {
                        Ok(()) => crate::night_light::poke(),
                        Err(e) => z.error = Some(e),
                    }
                }
            }
            notify(SetupEvent::ZoneGuess(ZoneFacts(z)))
        });
    }

    /// The Wi-Fi page's Time zone row: the picker, coming back to the Wi-Fi page.
    fn open_zone_from_wifi(&mut self) {
        self.zone_from_wifi = true;
        match self.zone_guess.take() {
            Some(z) => {
                let mut z = *z;
                z.view = ZoneView::Regions { sel: z.region_row() };
                self.step = Step::Zone(Box::new(z));
            }
            None => self.enter_zone(),
        }
    }

    /// The Time zone row's value: the zone picked or guessed with its time ("Vienna, 22:41"), "Choose" when unsure.
    fn zone_value(&self) -> String {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let shown = |zone: &str, z: Option<&ZoneStep>| match z.and_then(|z| z.offsets.get(zone).copied().flatten()) {
            Some(off) => format!("{}, {}", timezone::city_here(zone), timezone::clock_at(now, off)),
            None => timezone::city_here(zone),
        };
        match (&self.zone_picked, self.zone_guess.as_deref()) {
            (Some(z), g) => shown(z, g),
            (None, Some(g)) if g.clear() => shown(g.guess.zone().unwrap_or("UTC"), Some(g)),
            (None, Some(_)) => "Choose".into(),
            (None, None) => "…".into(),
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
                Button::South => self.zone_back_or_skip(),
                Button::Start => self.after_zone(),
                _ => {}
            }
            return;
        }
        z.error = None;
        let view = std::mem::replace(&mut z.view, ZoneView::Card);
        let (view, next) = match view {
            // Two rows under the card, as in every list: the guess, or another zone (X opens the picker too).
            ZoneView::Card => match b {
                Button::Up | Button::Down => {
                    z.card_row = usize::from(b == Button::Down);
                    (ZoneView::Card, ZoneNext::Stay)
                }
                Button::East if z.card_row == 1 => (ZoneView::Regions { sel: z.region_row() }, ZoneNext::Stay),
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
                // A country's own list ends with Other places… (X opens it too).
                let other = matches!(from, ListFrom::Country);
                let n = (zones.len() + usize::from(other)).max(1);
                let mut next = ZoneNext::Stay;
                let mut view = None;
                match b {
                    Button::Up => sel = sel.saturating_sub(1),
                    Button::Down => sel = (sel + 1).min(n - 1),
                    Button::L1 => sel = sel.saturating_sub(5),
                    Button::R1 => sel = (sel + 5).min(n - 1),
                    Button::East if other && sel == zones.len() => view = Some(ZoneView::Regions { sel: z.region_row() }),
                    Button::East => {
                        if let Some(zone) = zones.get(sel) {
                            next = ZoneNext::Take(Some(zone.clone()));
                        }
                    }
                    Button::North if other => view = Some(ZoneView::Regions { sel: z.region_row() }),
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
            // L1 empties the field, as in the Wi-Fi password.
            ZoneView::Search(mut kb) if b == Button::L1 => {
                kb.text.clear();
                (ZoneView::Search(kb), ZoneNext::Stay)
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
            ZoneNext::Back => self.zone_back_or_skip(),
            ZoneNext::Take(zone) => self.leave_zone(zone),
        }
    }

    /// The time zone as a stage of the first start (after Wi-Fi, or in its place), not opened from the Wi-Fi page's row.
    fn zone_stage(&self) -> bool {
        self.only.is_none() && !self.zone_from_wifi
    }

    /// B on the time zone's first screen: the first start skips the step (the zone stays as it is, UTC said as
    /// unconfirmed); opened from the Wi-Fi page's row, or from Settings, it goes back.
    fn zone_back_or_skip(&mut self) {
        if self.only.is_some() || self.zone_from_wifi {
            self.zone_back();
            return;
        }
        // Nothing guessed to keep: the clock stays as the system has it, said as unconfirmed when that is UTC.
        self.zone_utc = matches!(&self.step, Step::Zone(z) if matches!(z.guess, Guess::Utc));
        self.after_zone();
    }

    /// Ends the time zone step: sets the picked zone, or the guess when nothing was picked. Stays when the system refuses.
    fn leave_zone(&mut self, pick: Option<String>) {
        let Step::Zone(z) = &self.step else { return };
        let chosen = pick.is_some();
        let want = pick.or_else(|| match &z.guess {
            // No guess to keep: UTC (or a whole country) leaves the zone as it is.
            Guess::Utc | Guess::Several { .. } => None,
            g => g.zone().map(String::from),
        });
        let unconfirmed = want.is_none() && matches!(z.guess, Guess::Utc);
        if want.as_ref().is_none_or(|zone| self.set_zone(zone)) {
            self.zone_utc = unconfirmed;
            // Chosen in a list, or on the Wi-Fi page's row: kept, a guess after a later connection leaves it alone.
            if (chosen || self.zone_from_wifi) && want.is_some() {
                self.zone_picked = want;
            }
            self.after_zone();
        }
    }

    /// The Wi-Fi page: A connects (a saved network without its password), Y forgets a saved one (twice), X scans,
    /// the last two rows are Other network and Wi-Fi country.
    fn press_wifi(&mut self, b: Button) {
        if self.net_menu.is_some() {
            self.press_net_menu(b);
            return;
        }
        let n = self.wifi_rows();
        // The row among the networks and the rows after them; None on the sign-in row.
        let row = self.sel.checked_sub(self.lead());
        let entry = row.and_then(|r| self.entries.get(r)).cloned();
        let tail = self.wifi_tail_at(row);
        // No Wi-Fi chip: nothing on this page to choose but Continue (first start); Start goes on, B goes back.
        if self.radio == Radio::Absent && !matches!(b, Button::Start | Button::South | Button::East) {
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
            Button::North if !self.radio.is_off() => self.scan(),
            Button::Start => self.step = Step::Done,
            // The first start's B went back a stage before it came here (`press_first_start`), but from the phone page's
            // "Connect to Wi-Fi first" it goes back to that page; Settings: B leaves.
            Button::South if self.only.is_none() => self.leave_wifi(),
            Button::South => self.step = Step::Done,
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
            Button::East if tail == Some(WifiRow::Other) && self.radio.is_off() => self.note = Some(("Turn Wi-Fi on first.".into(), false)),
            Button::East if tail == Some(WifiRow::Other) => self.step = Step::HiddenName { kb: Keyboard::default() },
            Button::East if tail == Some(WifiRow::Country) => self.open_country(),
            Button::East if tail == Some(WifiRow::Zone) => self.open_zone_from_wifi(),
            Button::East if tail == Some(WifiRow::Scan) => self.scan(),
            Button::East if tail == Some(WifiRow::Continue) => self.leave_wifi(),
            Button::East => match entry {
                // A saved network: its small menu, Connect (Continue when connected in the first start) and Forget, so
                // that forgetting is never Y alone (2026-11-07). One being connected keeps its own answers below.
                Some(e) if e.saved.is_some() && (e.active || !connecting(&e)) => self.net_menu = Some((e.ssid.clone(), 0)),
                // The first start: connected already, A is Continue.
                Some(e) if e.active && self.only.is_none() => self.leave_wifi(),
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
        let runtime_row = matches!(self.step, Step::Extras { sel: 2 })
            || matches!(self.step, Step::Ports) && !self.portmaster_ready();
        if runtime_row && self.portmaster_confirm_remove {
            match b {
                Button::East => { self.remove_portmaster(); return false; }
                Button::South if !self.portmaster_removing => { self.portmaster_confirm_remove = false; return false; }
                Button::Up | Button::Down | Button::Left | Button::Right => self.portmaster_confirm_remove = false,
                _ => {}
            }
        }
        if runtime_row && b == Button::North && self.portmaster_can_remove() {
            self.portmaster_confirm_remove = true;
            self.portmaster_note = None;
            self.step = Step::Extras { sel: 2 };
            return false;
        }
        if matches!(self.step, Step::Extras { .. }) && matches!(b, Button::South | Button::Start) {
            if let Some(page) = &mut self.cheat_data { page.cancel(); }
            self.covers_cancel.cancel();
        }
        // A press takes the player's hand back: the step no longer goes on by itself.
        self.advance_pending = None;
        if matches!(b, Button::South | Button::Start) && matches!(self.step, Step::Ports | Step::Extras { .. }) {
            if let Some(cancel) = &self.port_download_cancel { cancel.cancel(); }
        }
        let done = self.press_first_start(b).unwrap_or_else(|| self.press_step(b));
        // Games being copied while the page is open: a press reads the folders again (not a read already under way).
        if matches!(self.step, Step::Games | Step::Found { .. }) && !self.counting && self.counted_at.elapsed() >= RECOUNT_AFTER {
            self.count_games();
        }
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

    /// The keys every first-start stage shares (BananaOS's way): START skips the rest of the setup, L2 switches Easy to
    /// read (larger text and calmer menus, at once, on every page of it), B and L1 go back a stage. None: the stage's own
    /// keys decide (B closes a page inside a stage). Each stage's way on is a row or button Down reaches (one scheme for
    /// every page, 2026-11-07: B skipped forward here and went back everywhere else).
    fn press_first_start(&mut self, b: Button) -> Option<bool> {
        if self.only.is_some() {
            return None;
        }
        match b {
            // The Wi-Fi country list jumps by letter with L2/R2.
            Button::L2 if !matches!(self.step, Step::Country { .. }) => {
                self.switch_easy_to_read();
                Some(false)
            }
            Button::Start if stage_of(&self.step).is_some() => Some(true),
            // B and L1 go back to the stage before; lists inside a stage keep L1 for paging. A question the page asked
            // (Forget this network?) is taken back by B first.
            Button::L1 if self.stage_back() => Some(false),
            // Wi-Fi opened from the phone page: B goes back there (`press_wifi`), not a stage.
            Button::South if self.forget.is_none() && self.net_menu.is_none() && !self.wifi_for_phone && self.b_back() => Some(false),
            _ => None,
        }
    }

    /// L1 on a first-start stage: opens the stage before it, as it opens when reached forward (Wi-Fi scans again, your
    /// games are counted afresh); the welcome has none. False where L1 is the page's own (a list inside a stage, the
    /// welcome, the pages that are no stage).
    fn stage_back(&mut self) -> bool {
        self.l1_goes_back() && self.go_back_a_stage()
    }

    /// B on a first-start stage: back a stage as L1 goes, and also from a country's own list of zones, which is the time
    /// zone's stage page too (L1 pages there).
    fn b_back(&mut self) -> bool {
        let zone_list = self.only.is_none() && matches!(&self.step, Step::Zone(z) if matches!(z.view, ZoneView::List { from: ListFrom::Country, .. }));
        (self.l1_goes_back() || zone_list) && self.go_back_a_stage()
    }

    /// The stage before this one, opened as when reached forward.
    fn go_back_a_stage(&mut self) -> bool {
        self.net_menu = None;
        match &self.step {
            // The time zone belongs to the Wi-Fi stage: back to its page (the welcome where it takes Wi-Fi's place).
            // Seen and left with B: asked once, Continue then goes on (the Time zone row still opens it); a country's list
            // has no row to go on without a city.
            Step::Zone(_) if !self.no_wifi => {
                self.zone_from_wifi = false;
                self.zone_asked = true;
                self.zone_back_to_wifi();
                self.scan();
                true
            }
            Step::Wifi | Step::Zone(_) => {
                self.wifi_for_phone = false;
                self.step = Step::Welcome;
                true
            }
            Step::Found { .. } => {
                self.after_welcome();
                true
            }
            Step::Look => {
                self.enter_found();
                true
            }
            _ => false,
        }
    }

    /// Whether L1 goes back a stage here (`stage_back`): the first start's stages after the welcome, not inside a list of
    /// one (the time zone's regions and lists page with L1). B does the same there (`press_first_start`).
    fn l1_goes_back(&self) -> bool {
        if self.only.is_some() {
            return false;
        }
        match &self.step {
            Step::Zone(z) => matches!(z.view, ZoneView::Card),
            Step::Wifi | Step::Found { .. } | Step::Look => true,
            _ => false,
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
            let line = if self.kids_wanted { format!("{name} child") } else { name.to_string() };
            if std::fs::read_to_string(&file).ok().as_deref() != Some(line.as_str()) {
                let _ = std::fs::create_dir_all(&self.data_dir);
                let _ = crate::fsutil::write_atomic(&file, line.as_bytes());
            }
        }
    }

    /// The first start after a restart: opens at the step the last run reached (`remember_step`). A missing, unreadable
    /// or unknown line means the beginning; Wi-Fi, the zone and the games are looked at afresh, as when stepping in.
    /// The names of the first start before 0.2 (look, play, extras: its second, third and sixth steps) open the stage of
    /// today's that comes next by position: the welcome was answered, so Wi-Fi (or what takes its place); Extras is now
    /// the Box art switch on your games. "child" after the name is the welcome's wish for Kids Mode.
    pub fn resume(mut self) -> Self {
        let Ok(line) = std::fs::read_to_string(self.data_dir.join(RESUME_FILE)) else { return self };
        let mut words = line.split_whitespace();
        let name = words.next().unwrap_or("");
        self.kids_wanted = words.next() == Some("child") && kids_offered();
        match name {
            "look" | "play" | "wifi" => self.after_welcome(),
            // Opened again as when Wi-Fi was left: a guess that can be trusted is taken, a failed one shows the step.
            "zone" if self.no_wifi => self.enter_zone(),
            "zone" => self.enter_zone_auto(),
            "extras" | "games" => self.enter_found(),
            "shelf-look" => self.enter_look(),
            _ => {}
        }
        self
    }

    /// The answers "How do you like to play?" offers here.
    fn profiles(&self) -> Vec<Profile> {
        if self.only == Some(Only::Play) { Profile::offered_in_settings() } else { Profile::offered() }
    }

    fn press_step(&mut self, b: Button) -> bool {
        if matches!(self.step, Step::Look) && self.only.is_some() && self.look_opened.is_none() {
            self.look_opened = Some((self.theme, self.appearance, self.motion, self.animations));
        }
        let profiles = self.profiles();
        let found_rows = self.found_rows();
        // Add games: the row of ways under the grid, and the grid's own keys.
        if matches!(self.step, Step::Games) {
            let used = if self.ways_focus { self.press_ways(b) } else { self.press_grid(b) };
            if used {
                return false;
            }
        }
        let portmaster_ready = self.portmaster_ready();
        match &mut self.step {
            Step::Welcome => {
                // A gets started, from any row; B has nothing to go back to on the first page, except Settings when the
                // first start was opened from there.
                if b == Button::East {
                    self.after_welcome();
                }
                if b == Button::South && self.again {
                    self.backed_out = true;
                    return true;
                }
                // "For a child": only the wish, a row of its own (Y switches it too); Kids Mode's own steps come after your
                // games (`leave_games`).
                if b == Button::West && kids_offered() {
                    self.kids_wanted = !self.kids_wanted;
                }
                let rows = 2 + u8::from(kids_offered());
                match b {
                    Button::Up => self.welcome_row = self.welcome_row.saturating_sub(1),
                    Button::Down => self.welcome_row = (self.welcome_row + 1).min(rows - 1),
                    _ => {}
                }
                // The language, in use from the next frame on (`i18n`); on the rows below, Big text as L2 switches it,
                // and the wish for Kids Mode.
                if matches!(b, Button::Left | Button::Right) && self.welcome_row == 2 {
                    self.kids_wanted = !self.kids_wanted;
                } else if matches!(b, Button::Left | Button::Right) && self.welcome_row == 1 {
                    self.switch_easy_to_read();
                } else if matches!(b, Button::Left | Button::Right) {
                    let now = crate::i18n::current();
                    let lang = if b == Button::Left { now.previous() } else { now.next() };
                    crate::i18n::set(lang);
                    self.language_chosen = Some(lang);
                }
            }
            // A list like every other: Up/Down move between the layouts and the rows under them, Left/Right change the
            // focused one, A keeps (the first start: finishes) from any of them.
            Step::Look if self.only.is_some() && b == Button::South => {
                self.cancel_look();
                self.step = Step::Done;
            }
            Step::Look => match b {
                Button::Up => self.look_row = self.look_row.saturating_sub(1),
                Button::Down => self.look_row = (self.look_row + 1).min(self.look_rows().len() - 1),
                Button::Left | Button::Right => self.change_look_row(b == Button::Right),
                // The first start's last stage: Finish, the shelf is next (B went back a stage, `press_first_start`).
                Button::East if self.only.is_none() => return true,
                Button::East | Button::Start => self.step = Step::Done,
                _ => {}
            },
            Step::Found { sel } => {
                let rows = &found_rows;
                *sel = (*sel).min(rows.len().saturating_sub(1));
                match (b, rows.get(*sel).copied()) {
                    (Button::Up, _) => *sel = sel.saturating_sub(1),
                    (Button::Down, _) => *sel = (*sel + 1).min(rows.len().saturating_sub(1)),
                    (Button::East | Button::Left | Button::Right, Some(FoundRow::BoxArt)) => {
                        self.art = if self.art == Art::Off { Art::default() } else { Art::Off };
                        self.art_changed = true;
                    }
                    (Button::East, Some(FoundRow::More)) => self.step = Step::Games,
                    (Button::East, Some(FoundRow::Continue | FoundRow::Later)) => self.leave_games(),
                    _ => {}
                }
            }
            Step::Play { sel } => match b {
                Button::Up => *sel = (*sel + profiles.len() - 1) % profiles.len(),
                Button::Down => *sel = (*sel + 1) % profiles.len(),
                Button::East => match profiles[*sel] {
                    // Kids Mode's steps open over this one (`app_kids`), which `kids_done` leaves.
                    Profile::Child => self.kids_asked = true,
                    Profile::Choose => {
                        self.answer_look(Some(Profile::Choose));
                        let cards = Cards::new(features::Mode::First, features::choose_list(), self.switches.clone(), self.later.clone());
                        self.step = Step::Cards(Box::new(cards));
                    }
                    p => {
                        self.answer_look(Some(p));
                        features::apply(p, &mut self.switches);
                        self.motion = self.switches.motion;
                        self.tour_changed = true;
                        self.after_play();
                    }
                },
                // Skipped: every switch stays as it is.
                Button::Start => {
                    self.answer_look(None);
                    self.after_play();
                }
                Button::South if self.only.is_some() => self.step = Step::Done,
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
                    features::Out::Done => self.after_play(),
                    features::Out::Back => self.step = Step::Play { sel: self.profiles().iter().position(|p| *p == Profile::Choose).unwrap_or(0) },
                    features::Out::Stay | features::Out::Changed => {}
                }
            }
            Step::ShelfSystems { .. } => self.press_shelf(b),
            Step::Wifi => self.press_wifi(b),
            // L1 empties the field (the refused password stays in it, `a_refused_password_stays...`): leaving is then L1, B.
            Step::Password { ssid, kb, hidden } => match if b == Button::L1 { kb.text.clear(); Outcome::Editing } else { kb.press(b) } {
                Outcome::Editing => {}
                Outcome::Cancelled => self.step = Step::Wifi,
                // A hidden network may be open: an empty password connects without one.
                Outcome::Done(psk) if psk.is_empty() && !*hidden => {}
                Outcome::Done(psk) => {
                    let (ssid, hidden) = (ssid.clone(), *hidden);
                    self.typed_password = psk.clone();
                    self.connect_to(ssid, (!psk.is_empty()).then_some(psk), hidden);
                }
            },
            Step::HiddenName { kb } => match if b == Button::L1 { kb.text.clear(); Outcome::Editing } else { kb.press(b) } {
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
                Button::Up => *sel = (*sel + extras_rows() - 1) % extras_rows(),
                Button::Down => *sel = (*sel + 1) % extras_rows(),
                // The Pictures choice needs no Wi-Fi: it only says what the downloads bring.
                Button::East | Button::Right if *sel == 1 => {
                    self.art = self.art.next();
                    self.art_changed = true;
                }
                Button::Left if *sel == 1 => {
                    self.art = self.art.prev();
                    self.art_changed = true;
                }
                Button::East if *sel == extras_rows() - 1 => {
                    if let Some(page) = &mut self.cheat_data { page.cancel(); }
                    self.covers_cancel.cancel();
                    self.open_starter = true;
                }
                Button::East if extras_cheats() && *sel == 3 => {
                    let online = self.extras_demo_online.unwrap_or_else(|| self.ip.is_some() || wifi::ip().is_some());
                    if let Some(page) = &mut self.cheat_data { page.activate(online); }
                }
                Button::North if extras_cheats() && *sel == 3 => {
                    if let Some(page) = &mut self.cheat_data { page.confirm_remove(); }
                }
                Button::East if *sel == 2 && (ports::trimui_standalone() || self.portmaster.is_some()) => self.enter_ports(),
                Button::East if self.ip.is_some() || wifi::ip().is_some() => {
                    if *sel == 0 {
                        self.start_covers();
                    } else {
                        self.enter_ports();
                    }
                }
                Button::Start => self.go_to_games(),
                Button::South => self.enter_zone(),
                _ => {}
            },
            Step::Ports if !portmaster_ready => match b {
                Button::East => self.start_portmaster(),
                Button::South | Button::Start => self.step = Step::Extras { sel: 2 },
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
            // B closes the phone page (and its server) back to the grid, whose counts are read again.
            Step::Phone(page) => match page.press(b) {
                crate::phone_upload_page::Action::Close => {
                    self.phone_added |= page.added() > 0;
                    self.step = Step::Games;
                    self.count_games();
                }
                // No Wi-Fi: the first start goes back to its Wi-Fi step; from Settings the app opens Wi-Fi.
                crate::phone_upload_page::Action::Wifi if self.only.is_none() => {
                    self.wifi_for_phone = true;
                    // To connect: the networks first, not the Continue row Wi-Fi was left on.
                    self.sel = 0;
                    self.step = Step::Wifi;
                    self.scan();
                }
                crate::phone_upload_page::Action::Wifi => {
                    self.open_wifi = Some((true, true));
                    self.step = Step::Done;
                }
                crate::phone_upload_page::Action::None => {}
            },
            // The grid moves and opens All systems; X switches file sharing; Y sends games from a phone.
            Step::Games if self.grid.as_mut().is_some_and(|g| g.press(b)) => {}
            Step::Games => match b {
                Button::North => self.switch_sharing(),
                Button::West if crate::phone_upload_page::offered() => self.open_phone(),
                // The first start: Add more… is a page of "Your games", which counts again.
                // Back on Add more…, the row it was opened from.
                Button::Start | Button::South if self.only.is_none() => {
                    let more = self.found_rows().iter().position(|r| *r == FoundRow::More).unwrap_or(0);
                    self.step = Step::Found { sel: more };
                    self.count_games();
                }
                Button::Start => self.step = Step::Done,
                Button::South => self.step = Step::Done,
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

    /// "Send from your phone": its server starts here, on the Wi-Fi address (none without Wi-Fi: the page says so).
    fn open_phone(&mut self) {
        let notify = self.notify.clone();
        let page = crate::phone_upload_page::PhonePage::open(self.ip.clone(), self.catalog.clone(), Arc::new(move || notify(SetupEvent::Phone)));
        self.step = Step::Phone(Box::new(page));
    }

    /// The phone page is open and its server listens: the Pocket stays awake (no idle sleep, no screen-off timer) until
    /// the page closes itself (`IDLE`, `MAX_SESSION`); the power button still closes it (`phone_upload::sleeping`).
    pub fn phone_open(&self) -> bool {
        matches!(&self.step, Step::Phone(p) if p.listening())
    }

    /// The phone page's server listens (tests: nothing else ever does).
    pub fn phone_listening(&self) -> bool {
        matches!(&self.step, Step::Phone(p) if p.listening())
    }

    /// The phone page closed after files arrived: true once, the app reads the game folders again.
    pub fn take_phone_added(&mut self) -> bool {
        std::mem::take(&mut self.phone_added)
    }

    /// `--snapshot setup:phone=<kind>`: the phone page with a made-up address and status (`PhonePage::demo`).
    pub fn with_phone_demo(mut self, kind: &str) -> Self {
        self.step = Step::Phone(Box::new(crate::phone_upload_page::PhonePage::demo(kind)));
        self
    }

    /// Counts the games in every system's folder on a thread; `SetupEvent::GameCounts` brings the numbers.
    /// How many games are on the card, as well as this page knows.
    fn games_known(&self) -> usize {
        // Only what the player put there: the Odin's Steam entries are not "your games" (first-start walk 2026-11-04).
        self.games_counted.unwrap_or(0).max(self.games.iter().filter(|(_, p)| !library::path_supplied_by_device(p)).count())
    }

    fn count_games(&mut self) {
        self.counting = true;
        self.counted_at = Instant::now();
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
                let lines = [
                    if self.no_wifi {
                        "Next: your time zone, your games, then a look you like."
                    } else {
                        "Next: Wi-Fi, your games, then a look you like."
                    },
                    "You can skip any of them and come back later.",
                ];
                // On a handheld nobody on the project has played on: one calm line, once (this step is only on the
                // first start; docs/DEVICES.md).
                let untested = crate::device::current().model.filter(|m| m.experimental() && !m.played_on()).map(|m| {
                    let name = m.name();
                    if name.starts_with(['A', 'E', 'I', 'O', 'U']) {
                        format!("pocket-ui hasn't been tested on an {} yet. Most things should work; if something doesn't, Settings › Help & tools › Report a problem.", name)
                    } else {
                        format!("pocket-ui hasn't been tested on a {} yet. Most things should work; if something doesn't, Settings › Help & tools › Report a problem.", name)
                    }
                });
                // The rows under the words end above the hint bar: where the untested line and the third row (For a child)
                // would run into it (1024x768), the page moves up by what is missing.
                let step = crate::type_scale::line(28, 12);
                let count = |text: &mut Text, px: u32, l: &str| text.wrap_prose(Face::Body, px, l, c.w - 2 * MARGIN).len().max(1) as i32;
                let rows = 2 + i32::from(kids_offered());
                // How far the page moves up with the untested paragraph in `note_px` type.
                let lift_for = |text: &mut Text, note_px: u32| {
                    let mut words_end = 360 + (lines.iter().map(|l| count(text, 28, l)).sum::<i32>() - 1) * step;
                    if let Some(l) = &untested {
                        words_end += 56 + (count(text, note_px, l) - 1) * crate::type_scale::line(note_px, 12);
                    }
                    let rows_end = words_end.max(430) + 40 + rows * (pill_h() + 15) - 15;
                    (rows_end - (crate::kit::hint_bar_top(c.h) - 12)).max(0)
                };
                // The title stands clear of the status corner (the clock of Big text and the dots) down to here; a
                // longer page sets the paragraph about the device in a smaller type instead (German, Large).
                let (mut note_px, mut lift) = (28, lift_for(text, 28));
                if lift > 160 {
                    (note_px, lift) = (24, lift_for(text, 24));
                }
                // The pictures give way when the lifted title leaves no room for them above it (the untested line, a third
                // row and Large text at 1024x768): pushed back down they would put the rows under the hint bar again.
                let k = crate::ui_art::empty_scale(c.h);
                let caps = 300 - lift - crate::type_scale::px(67) as i32 * 3 / 4;
                let dy = if caps - 28 - crate::ui_art::row_size(&row, k).1 >= 60 {
                    crate::ui_art::above_words_row(c, &row, MARGIN, false, 60, 300 - lift, 67) - lift
                } else {
                    -lift
                };
                let (heading_px, heading) = text.fit_sizes(Face::Title, &[67, 50], "Welcome to your Pocket", c.w - 2 * MARGIN);
                text.draw(c, Face::Title, heading_px, MARGIN, 300 + dy, pal().chalk, &heading);
                let mut below = paragraphs(c, text, &lines, 360 + dy);
                if let Some(line) = &untested {
                    below = paragraphs_at(c, text, note_px, &[line.as_str()], below + 56);
                }
                // The languages in their own names; the line after them in the one chosen.
                let on = crate::i18n::Lang::ALL.iter().position(|l| *l == crate::i18n::current()).unwrap_or(0);
                let names: Vec<&str> = crate::i18n::Lang::ALL.iter().map(|l| l.name()).collect();
                let label_w = ["Language", "Big text", "For a child"].iter().map(|l| text.width(Face::Body, 24, l)).max().unwrap_or(0);
                let top = below.max(470 - 40 + dy.min(0)) + 40;
                pill_row(c, text, top, "Language", &names, on, "You can change it later in Settings.", label_w);
                self.paint_easy_to_read(c, text, top + pill_h() + 15, label_w);
                // Kids Mode: only the wish; its steps come after your games.
                if kids_offered() {
                    let line = if self.kids_wanted { "For a child: Kids Mode is set up after your games." } else { "A simple shelf for a child, set up after your games." };
                    pill_row(c, text, top + 2 * (pill_h() + 15), "For a child", &["Off", "On"], usize::from(self.kids_wanted), line, label_w);
                }
                // The row Left/Right change: a bar in the margin beside it.
                focus_bar(c, top + i32::from(self.welcome_row) * (pill_h() + 15), pill_h());
                let change = match self.welcome_row {
                    0 => "Language",
                    1 => "Big text",
                    _ => "For a child",
                };
                let mut bar = vec![("Left/Right", change), ("A", "Get started")];
                if self.again {
                    bar.push(("B", "Back"));
                }
                hints(c, text, &bar);
            }
            // The app paints this step itself, with the previews (`paint_look`); without them, the names.
            Step::Look => self.paint_look(c, text, &[], false),
            Step::Play { sel } => {
                features::paint_profiles(c, text, *sel, &self.profiles(), self.only == Some(Only::Play), &self.play_previews);
            }
            Step::ShelfSystems { sel } => self.paint_shelf_systems(c, text, *sel),
            Step::Wifi => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Wi-Fi");
                let (line, color) = self.wifi_status(&dots);
                // A on the connected network: a drawn tick before the line (the font has none).
                let tick = if self.confirming().is_some() { 48 } else { 0 };
                // A little smaller before cut short ("Home isn't letting this Brick in. Press A to type the password again.").
                let (line_px, line) = text.fit_sizes(Face::Body, &[28, 24], &line, c.w - 2 * MARGIN - tick);
                if tick > 0 {
                    for i in 0..7 {
                        c.rect(MARGIN + i * 2, 178 + i * 2, 4, 4, color);
                    }
                    for i in 0..14 {
                        c.rect(MARGIN + 12 + i * 2, 190 - i * 2, 4, 4, color);
                    }
                }
                text.draw(c, Face::Body, line_px, MARGIN + tick, 192, color, &line);
                self.paint_networks(c, text);
                let row = self.sel.checked_sub(self.lead());
                let saved = row.and_then(|r| self.entries.get(r)).is_some_and(|e| e.saved.is_some());
                let first = self.only.is_none();
                let entry = row.and_then(|r| self.entries.get(r));
                let a = match (row.map(|r| r.checked_sub(self.entries.len())), self.wifi_tail_at(row)) {
                    // A saved network opens its menu (Connect or Continue, Forget).
                    (Some(None), _) if entry.is_some_and(|e| e.saved.is_some() && (e.active || !connecting(e))) => "Open",
                    (Some(None), _) => "Connect",
                    (None, _) if self.lead_kind() == Some(Lead::Off) => "Turn on",
                    (_, Some(WifiRow::Zone)) => "Change",
                    (_, Some(WifiRow::Scan)) => "Scan again",
                    (_, Some(WifiRow::Continue)) => "Continue",
                    _ => "Open",
                };
                let online = self.entries.iter().any(|e| e.active);
                if self.radio == Radio::Absent && first {
                    hints(c, text, &[("A", "Continue"), ("B", "Back")]);
                } else if self.radio == Radio::Absent {
                    hints(c, text, &[("B", "Back")]);
                } else if self.net_menu.is_some() {
                    // A saved network's menu over the page, with its own hints.
                    self.paint_net_menu(c, text);
                } else {
                    hints(c, text, &wifi_hints(a, saved, self.forget.is_some(), first, online, !self.radio.is_off()));
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
                // With something in the field, L1 empties it at once (then B leaves); in an empty one B leaves.
                let delete = if kb.text.is_empty() { ("B", "Back") } else { ("B", "Delete") };
                let mut bar = vec![("A", "Type"), delete];
                if !kb.text.is_empty() {
                    bar.push(("L1", "Clear"));
                }
                bar.extend([("X", "Shift"), ("Y", "Symbols"), ("START", done)]);
                hints(c, text, &bar);
            }
            Step::HiddenName { kb } => {
                text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, "Name of the hidden network");
                paint_keyboard_with(c, text, kb, "Next");
                let delete = if kb.text.is_empty() { ("B", "Back") } else { ("B", "Delete") };
                let mut bar = vec![("A", "Type"), delete];
                if !kb.text.is_empty() {
                    bar.push(("L1", "Clear"));
                }
                bar.extend([("X", "Shift"), ("Y", "Symbols")]);
                // An empty name goes nowhere: START only once there is one.
                if !kb.text.trim().is_empty() {
                    bar.push(("START", "Next"));
                }
                hints(c, text, &bar);
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
            Step::Found { sel } => {
                let sel = *sel;
                self.paint_found(c, text, sel, &dots);
            }
            Step::Phone(page) => {
                page.paint(c, text);
                hints(c, text, &page.hints());
            }
            Step::Remote => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Remote access");
                let lines = [
                    "Log in to this Pocket from a computer on the same Wi-Fi, with SSH.",
                    "It stays on until you turn it off or the Pocket restarts.",
                ];
                // Wrapped: at Large text the second line ran off the right edge
                // (Mini V2 1240x1080, English; monkey run 2026-10-31, seed 102).
                let mut y = 210;
                for l in lines {
                    for line in text.wrap_prose(Face::Body, 28, l, c.w - 2 * MARGIN) {
                        text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &line);
                        y += 44;
                    }
                }
                let ip = self.ip.clone().unwrap_or_else(|| "(not connected)".into());
                let ssh_y = if y > 322 { y + 24 } else { 366 };
                text.draw(c, Face::Title, 37, MARGIN, ssh_y, pal().chalk, &format!("ssh root@{ip}"));
                // The login and the status move down with a longer text above: four lines of it (German, Retro, 1024 wide: the
                // Brick) put the ssh line over the login.
                let (login_y, mut status_y) = ((ssh_y + 68).max(434), 544);
                let on = matches!(self.remote, remote::Mode::On | remote::Mode::Dev | remote::Mode::External);
                if on {
                    // Dev or external SSH: pocket-ui never set a password, root's own one applies.
                    let pw = self.remote_password.as_deref().or((self.remote != remote::Mode::On).then_some("(unchanged)"));
                    paint_login(c, text, login_y, "root", pw);
                    status_y = status_y.max(login_y + 110);
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
                text.draw(c, Face::Body, 28, MARGIN, status_y, color, &status);
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
                let games = done_line(self.games_known());
                let mut lines = vec![games.as_str()];
                // First start only, and not for a child: a first game in RetroArch or another emulator has no line over it.
                if self.only.is_none() && self.home_line.get().unwrap_or_else(|| { let v = crate::first_game::told_on_last_page(&self.data_dir); self.home_line.set(Some(v)); v }) {
                    lines.push(crate::first_game::LAST_PAGE_LINE);
                }
                lines.push("Settings › Help & tools › Tour shows every feature again, one card each.");
                if self.zone_utc {
                    lines.push("The clock is on UTC. Set your time zone in Settings › System › Time.");
                }
                paragraphs(c, text, &lines, 360);
                hints(c, text, &[("A", "Go to your games"), ("SELECT", "Manual")]);
            }
        }
        self.paint_step_count(c, text);
    }

    /// First start only: "Step 3 of 7" at the top right, so a new player knows how long the setup is. The Look step is
    /// painted by the app (`paint_look`, which calls this too); the cards have their own "n of m".
    fn paint_step_count(&self, c: &mut Canvas, text: &mut Text) {
        let Some(at) = self.only.is_none().then(|| stage_of(&self.step)).flatten() else { return };
        // One dot per stage, the ones done and this one filled: pages turned, not a queue (no "Step 2 of 4").
        let n = stage_count() as i32;
        let (d, gap) = (14, 12);
        let right = c.w - MARGIN;
        let y = 56;
        for i in 0..n {
            let x = right - (n - i) * (d + gap) + gap;
            let color = if i as usize <= at { pal().chalk } else { pal().keycap };
            crate::retro::pill(c, x, y, d, d, d / 2, color);
        }
        crate::text::boxes::area(c, (right - n * (d + gap) + gap, y, n * (d + gap) - gap, d), "‹dots›");
        // Big text, on every stage: its key and "Aa", the letters larger when it is on.
        let on = self.easy_to_read();
        let label = if on { "Aa  On" } else { "Aa" };
        let lw = text.width(Face::Body, 24, label);
        let kh = crate::type_scale::row(36, 21);
        let kw = (text.width(Face::Body, 21, crate::button_prompts::shown("L2")) + 24).max(kh);
        let x = right - n * (d + gap) - 28 - lw - 10 - kw;
        let base = y + d / 2 + crate::type_scale::mid(24);
        crate::kit::key_cap(c, text, (x, y + d / 2 - kh / 2, kw, kh), "L2", 21, y + d / 2 + crate::type_scale::mid(21), (pal().keycap, pal().chalk));
        text.draw(c, Face::Body, 24, x + kw + 10, base, if on { pal().chalk } else { pal().dust }, label);
        crate::nav_probe::hint(c, (x, y + d / 2 - kh / 2, kw + 10 + lw, kh), "L2", label);
    }

    /// "Step n of m" as a number, for the tests: the dot that is lit (from 1) and how many there are.
    #[cfg(test)]
    fn step_of(&self) -> Option<(usize, usize)> {
        let n = self.only.is_none().then(|| stage_of(&self.step)).flatten()?;
        Some((n + 1, stage_count()))
    }

    /// The rows of "Your games": Continue and Add more… with games, Add games… and I'll add games later without; the Box
    /// art switch where pictures can be downloaded (not on a handheld without Wi-Fi).
    fn found_rows(&self) -> Vec<FoundRow> {
        let mut rows = if self.games_known() > 0 { vec![FoundRow::Continue, FoundRow::More] } else { vec![FoundRow::More, FoundRow::Later] };
        if !self.no_wifi {
            rows.push(FoundRow::BoxArt);
        }
        rows
    }

    /// "Your games" without games: the way that works here, in a sentence (Add games… has the details).
    fn found_way(&self) -> &'static str {
        let d = crate::device::current();
        no_games_way(crate::card_health::image_card(), crate::games_card::in_use(), crate::card_health::games_drive(),
            d.card_reads_in_computer(), d.has_wifi(), d.has_second_card_slot())
    }

    /// The first start's "Your games": what was found (or the way to add some), then its rows.
    fn paint_found(&self, c: &mut Canvas, text: &mut Text, sel: usize, dots: &str) {
        let width = c.w - 2 * MARGIN;
        let games = self.games_known();
        let looking = self.counting && self.games_counted.is_none() && games == 0;
        let (title, sub) = if games > 0 || looking {
            ("Your games", "They stand on your shelf in their real boxes.")
        } else {
            ("Add your games", "Pop your games on here and they'll stand on your shelf in their real boxes.")
        };
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, title);
        let mut y = 192;
        for l in text.wrap_prose_at_most(Face::Body, 28, sub, width, 2) {
            text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &l);
            y += crate::type_scale::line(28, 10);
        }
        y += 40;
        let step = crate::type_scale::line(28, 10);
        // The rows from the bottom up, always above the hints (1024x768: Add more… and Box art ran under them), and the
        // line about the selected one over them: both come before the consoles, which give way on a short screen.
        let rows = self.found_rows();
        let row = crate::type_scale::row(70, 37);
        let top = found_rows_top(c.h, rows.len());
        let note = match rows.get(sel) {
            Some(FoundRow::BoxArt) => Some(BOX_ART_NOTE),
            _ if self.kids_wanted && games > 0 => Some("Kids Mode comes next: a PIN, then the games a child may play."),
            _ if self.kids_wanted => Some("Kids Mode needs a few games first. Set it up later in Settings › System › Kids mode."),
            _ => None,
        };
        let note_lines = note.map(|n| text.wrap_prose_at_most(Face::Body, 24, n, width, 2)).unwrap_or_default();
        let note_top = top - 18 - note_lines.len() as i32 * crate::type_scale::line(24, 8);
        if looking {
            text.draw(c, Face::Title, 50, MARGIN, y + 30, pal().chalk, &format!("Looking for your games{dots}"));
            y += 30 + step;
        } else if games > 0 {
            let found = if games == 1 { "Found 1 game".to_string() } else { format!("Found {games} games") };
            text.draw(c, Face::Title, 50, MARGIN, y + 30, pal().chalk, &found);
            y += 30 + step + 4;
            // Which systems, the fullest first: their consoles in a row with the count under each, "+ 4 more" after.
            if let Some(grid) = &self.grid {
                let mut with: Vec<(&SysInfo, usize)> = grid.all.iter().map(|s| (s, grid.games(&s.id))).filter(|(_, n)| *n > 0).collect();
                with.sort_by(|a, b| b.1.cmp(&a.1));
                // The name line under each console must not push the rows below off a short screen (1024x768): the
                // pictures give up that height there, down to 44 px.
                let name_line = crate::type_scale::line(21, 8);
                // The line about the selected row stands over the rows: the pictures leave it room, and where even the
                // smallest would run into it they stay out ("Found 81 games" says it).
                let art_room = note_top - y - 30 - name_line - step;
                let art_h = art_room.clamp(44, 76);
                let (tile, gap) = (150, 18);
                let more_w = 120;
                let fit = ((width - more_w + gap) / (tile + gap)).max(1) as usize;
                let shown = if art_room < 44 { 0 } else if with.len() > fit { fit } else { with.len() };
                for (k, (sys, n)) in with.iter().take(shown).enumerate() {
                    let x = MARGIN + k as i32 * (tile + gap);
                    if let Some(art) = crate::system_art::get(&sys.id).map(|a| a.fitted(tile - 20, art_h)) {
                        c.blit_sprite(&art, x + (tile - art.w as i32) / 2, y + (art_h - art.h as i32), 0);
                    }
                    // The system's name under its console (a Game Gear or a Master System isn't known by sight to
                    // everyone, first-start walk 2026-11-06), how many games under that.
                    // Smaller type before "…": "Game Boy Advance" at 640 px.
                    let (npx, name) = text.fit_sizes(Face::Body, &[21, 18, 16], &systems_grid::short_name(sys), tile);
                    let w = text.width(Face::Body, npx, &name);
                    text.draw(c, Face::Body, npx, x + (tile - w) / 2, y + art_h + 30, pal().chalk, &name);
                    let words = if *n == 1 { "1 game".to_string() } else { format!("{n} games") };
                    let count = text.fit(Face::Body, 21, &words, tile);
                    let w = text.width(Face::Body, 21, &count);
                    text.draw(c, Face::Body, 21, x + (tile - w) / 2, y + art_h + 30 + name_line, pal().dust, &count);
                }
                if with.len() > shown && shown > 0 {
                    let x = MARGIN + shown as i32 * (tile + gap);
                    text.draw(c, Face::Body, 24, x, y + art_h / 2 + 8, pal().dust, &format!("+ {} more", with.len() - shown));
                }
                if shown > 0 {
                    y += art_h + 30 + name_line + step;
                }
            }
        } else {
            for l in text.wrap_prose_at_most(Face::Body, 28, self.found_way(), width, 4) {
                text.draw(c, Face::Body, 28, MARGIN, y, pal().chalk, &l);
                y += step;
            }
        }
        let mut room = top - 18;
        // The note only where it stands clear of what is above it.
        if !note_lines.is_empty() && note_top + crate::type_scale::line(24, 8) >= y {
            let mut ny = note_top + crate::type_scale::line(24, 8);
            room = note_top;
            for l in &note_lines {
                text.draw(c, Face::Body, 24, MARGIN, ny, pal().dust, l);
                ny += crate::type_scale::line(24, 8);
            }
        }
        // What in roms isn't read, in plain words: specific help instead of a quiet "no games"; as many lines as fit
        // above the note.
        for l in unread_lines(&self.strays, &self.broken).iter().take(2) {
            if y + 8 > room {
                break;
            }
            let l = text.fit(Face::Body, 24, l, width);
            text.draw(c, Face::Body, 24, MARGIN, y + 8, pal().alert, &l);
            y += crate::type_scale::line(24, 8);
        }
        for (i, r) in rows.iter().enumerate() {
            let ry = top + i as i32 * row;
            let selected = i == sel;
            crate::kit::shelf_row(c, MARGIN, ry, width, row - 8, selected);
            let fg = if selected { pal().ink } else { pal().chalk };
            let name = match r {
                FoundRow::Continue => "Continue",
                FoundRow::More if games > 0 => "Add more…",
                FoundRow::More => "Add games…",
                FoundRow::Later => "I'll add games later",
                FoundRow::BoxArt => "Box art",
            };
            let base = ry + (row - 8) / 2 + crate::type_scale::base(13, 37);
            text.draw(c, Face::Title, 37, MARGIN + 28, base, fg, name);
            if *r == FoundRow::BoxArt {
                let on = if self.art == Art::Off { "Off" } else { "On" };
                let value = if selected { format!("‹ {on} ›") } else { on.to_string() };
                crate::kit::row_value(c, text, 24, MARGIN + width - 28, ry + (row - 8) / 2 + crate::type_scale::base(10, 24), &value, selected);
            }
        }
        let action = match rows.get(sel) {
            Some(FoundRow::BoxArt) => "Switch",
            Some(FoundRow::More) => "Open",
            _ => "Continue",
        };
        hints(c, text, &[("A", action), ("B", "Back"), ("START", "Skip the rest")]);
    }

    /// The existing page, in the same row and footer style as other Settings pages.
    fn paint_extras(&mut self, c: &mut Canvas, text: &mut Text, sel: usize, dots: &str) {
        if let Some(page) = &mut self.cheat_data { page.poll(); }
        let online = self.extras_demo_online.unwrap_or_else(|| self.ip.is_some() || wifi::ip().is_some());
        let device = crate::device::current();
        let offline = extras_sub(false, device.has_wifi());
        let about = format!("{}\n{}", "The front of each game's box, from a free collection online.", PICTURES_NOTE);
        let covers_error = self.covers_unreachable && !self.covers_running;
        let covers = if !online { "Needs Wi-Fi".into() }
            else if covers_error { "Retry".into() }
            else if self.covers_running || self.covers.is_some() { extras_status(self.covers, self.covers_running, self.art == Art::Pictures, self.games_known(), dots) }
            else { "Download".into() };
        let mut rows = vec![
            crate::emu_menu::Row { name: "Box art".into(), value: covers, desc: if online && covers_error { "Couldn't reach the picture server. Try again later.".into() } else { about } },
            crate::emu_menu::Row { name: "Pictures".into(), value: if sel == 1 { format!("‹ {} ›", self.art.label()) } else { self.art.label().into() },
                desc: PICTURES_NOTE.into() },
        ];
        if ports::offered() {
            rows.push(self.portmaster_row(online));
        }
        if extras_cheats() {
            rows.push(self.cheat_data.as_ref().map_or_else(|| crate::cheat_data::get_row(31_367_177, online, Some(crate::update::cheats::UNAVAILABLE)), |page| page.row(online)));
        }
        rows.push(crate::emu_menu::Row { name: "Starter pack".into(), value: if online { "Browse" } else { "Needs Wi-Fi" }.into(),
            desc: "A few free games to get you started. Pick one or take them all.".into() });
        let cheat = extras_cheats() && sel == 3;
        let runtime = ports::trimui_standalone() && sel == 2;
        let action = if cheat { self.cheat_data.as_ref().filter(|p| p.can_activate(online)).map(|p| p.action(online)) }
            else if runtime && (self.portmaster_removing || matches!(self.portmaster, Some(PortMasterState::Downloading(_) | PortMasterState::Checking))) { None }
            else if runtime && self.portmaster_confirm_remove { Some("Remove") }
            else if runtime && !matches!(self.portmaster, Some(PortMasterState::Downloading(_) | PortMasterState::Checking)) { Some("Browse") }
            else if sel == 1 { Some("Change") } else if sel == rows.len() - 1 { Some("Browse") }
            else if !online { None } else if sel == 0 { Some(if covers_error { "Retry" } else { "Download" }) } else { Some("Browse") };
        let alert = cheat && self.cheat_data.as_ref().is_some_and(|p| p.alert(online))
            || runtime && (self.portmaster_note.is_some() || online && matches!(self.portmaster, Some(PortMasterState::Offer(_))) && self.portmaster_low_space().is_some())
            || sel == 0 && online && covers_error;
        let remove = runtime && !self.portmaster_confirm_remove && self.portmaster_can_remove();
        crate::extras_page::paint(c, text, &mut self.extras_touch.borrow_mut(), &rows, sel, (!online).then_some(offline),
            crate::extras_page::Footer { action, alert, remove });
    }

    pub fn take_open_starter(&mut self) -> bool { std::mem::take(&mut self.open_starter) }
    pub fn at_starter(mut self) -> Self { self.step = Step::Extras { sel: extras_rows() - 1 }; self }
    pub fn at_portmaster(mut self) -> Self { self.step = Step::Extras { sel: 2 }; self }
    pub fn cheat_status(&self) -> Option<(&'static str, u64, u64)> { self.cheat_data.as_ref().map(|p| p.status()) }

    /// Uses the production row mapper, with no worker or filesystem access.
    pub fn with_cheats_demo(mut self, state: &str) -> Self {
        self.cheat_data = Some(crate::cheat_data::Page::snapshot(state));
        self.extras_demo_online = Some(!state.ends_with("offline"));
        self.step = Step::Extras { sel: 3 };
        self
    }

    pub fn with_cover_error_demo(mut self) -> Self {
        self.covers_unreachable = true;
        self.covers_running = false;
        self.extras_demo_online = Some(true);
        self.step = Step::Extras { sel: 0 };
        self
    }

    fn paint_ports(&mut self, c: &mut Canvas, text: &mut Text, dots: &str) {
        text.draw(c, Face::Title, 67, MARGIN, 124, pal().chalk, "PortMaster games");
        if !self.portmaster_ready() { self.paint_portmaster_offer(c, text); return; }
        // The two lists as tabs: the one on show is paper, like a selected row.
        let mut x = MARGIN;
        for tab in [PortTab::Ready, PortTab::OwnFiles] {
            let label = if self.ports.is_empty() { tab.label().to_string() } else { format!("{}  {}", tab.label(), self.tab_count(tab)) };
            let w = text.width(Face::Body, 24, &label) + 40;
            let on = tab == self.port_tab;
            crate::retro::pill(c, x, 152, w, 44, 22, if on { pal().label } else { pal().spine });
            text.draw(c, Face::Body, 24, x + 20, 182, if on { pal().ink } else { pal().dust }, &label);
            x += w + 12;
        }
        let about = match self.port_tab {
            PortTab::Ready => "They run without extra files.",
            PortTab::OwnFiles => "They need files from your own copy of the game. pocket-ui shows where they go.",
        };
        let (line, color) = match (&self.port_busy, &self.port_note, self.catalog_loading) {
            (Some(title), _, _) => (match &self.port_progress {
                Some(p) if p.total > 0 => format!("{title}: {} / {} MB", p.done / (1 << 20), p.total / (1 << 20)),
                Some(p) if p.done > 0 => format!("{title}: {}", ports::size_label(p.done)),
                _ => format!("Working on {title}{dots}"),
            }, pal().dust),
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
        let desc = text.wrap_prose(Face::Body, 24, &port.desc, pw);
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

    fn paint_portmaster_offer(&mut self, c: &mut Canvas, text: &mut Text) {
        let width = c.w - 2 * MARGIN;
        let mut y = 235;
        for line in text.wrap_prose(Face::Body, 28, "Needed once for Ports. It stays on this card; games are downloaded separately.", width) {
            text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &line);
            y += crate::type_scale::line(28, 10);
        }
        y += 32;
        let action = self.portmaster_action();
        let lines = text.wrap_prose(Face::Title, 40, action, width - 64);
        let line_h = crate::type_scale::line(40, 12);
        let row_h = (lines.len() as i32 * line_h + 64).max(132);
        crate::kit::shelf_row(c, MARGIN, y, width, row_h, true);
        for (i, line) in lines.iter().enumerate() {
            text.draw(c, Face::Title, 40, MARGIN + 32, y + 48 + i as i32 * line_h, pal().ink, line);
        }
        y += row_h + 44;
        let note = self.portmaster_note.clone().unwrap_or_else(|| {
            if !self.portmaster_online() && !matches!(self.portmaster, Some(PortMasterState::Downloading(_))) { "Connect to Wi-Fi to get PortMaster.".into() }
            else if matches!(self.portmaster, Some(PortMasterState::Downloading(_))) { self.portmaster_progress_line() }
            else { "No download starts until you press A.".into() }
        });
        for line in text.wrap_prose(Face::Body, 26, &note, width) {
            if y >= c.h - 120 { break; }
            text.draw(c, Face::Body, 26, MARGIN, y, if self.portmaster_note.is_some() { pal().alert } else { pal().dust }, &line);
            y += crate::type_scale::line(26, 10);
        }
        if let Some(PortMasterState::Downloading(progress)) = &self.portmaster {
            if progress.total > 0 && y + 20 < c.h - 100 {
                c.rect(MARGIN, y + 8, width, 10, pal().spine);
                c.rect(MARGIN, y + 8, (width as u64 * progress.done.min(progress.total) / progress.total) as i32, 10, pal().label);
            }
        }
        let mut bar = Vec::new();
        if matches!(self.portmaster, Some(PortMasterState::Offer(_))) && self.portmaster_online() { bar.push(("A", "Download")); }
        bar.push(("B", if matches!(self.portmaster, Some(PortMasterState::Downloading(_))) { "Cancel" } else { "Back" }));
        hints(c, text, &bar);
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
            if self.zone_stage() {
                hints(c, text, &[("B", "Back"), ("START", "Skip the rest")]);
            } else {
                hints(c, text, &[("START", "Next"), ("B", "Back")]);
            }
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
                    (None, _) => ("Couldn't tell where you are. Choose your time zone below.".to_string(), pal().dust),
                };
                text.draw(c, Face::Body, 28, MARGIN, 192, color, &note);

                let zone = z.guess.zone().unwrap_or("UTC");
                // The card gives up height on a short screen (768 lines) for the two rows under it.
                let (top, h) = (250, if c.h < 900 { 180 } else { 220 });
                c.rect(MARGIN, top, width, h, pal().spine);
                let city = text.fit(Face::Title, 67, &timezone::city_here(zone), width - 440);
                text.draw(c, Face::Title, 67, MARGIN + 40, top + 100, pal().chalk, &city);
                if city != zone {
                    text.draw(c, Face::Body, 28, MARGIN + 40, top + 156, pal().dust, zone);
                }
                if let Some(Some(off)) = z.offsets.get(zone).copied() {
                    text.draw_right(c, Face::Title, 67, MARGIN + width - 40, top + 100, pal().chalk, &timezone::clock_at(now, off));
                    text.draw_right(c, Face::Body, 28, MARGIN + width - 40, top + 156, pal().dust, &timezone::fmt_offset(off));
                }
                let yes = if matches!(z.guess, Guess::Utc) { "Keep UTC" } else { "Looks right" };
                // The two ways on as rows, Up/Down between them (X still opens the picker).
                let row = crate::type_scale::row(70, 37);
                let rows_top = top + h + 20;
                for (i, name) in [yes, "Choose another…"].into_iter().enumerate() {
                    let y = rows_top + i as i32 * row;
                    let selected = i == z.card_row;
                    crate::kit::shelf_row(c, MARGIN, y, width, row - 8, selected);
                    text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), if selected { pal().ink } else { pal().chalk }, name);
                }
                // The lines under them as far as the hint bar leaves room.
                let clock = if crate::device::current().has_wifi() { "The clock sets itself from the internet while you're online." } else { "No Wi-Fi here, so the clock doesn't set itself." };
                let last = crate::kit::hint_bar_top(c.h) - 16;
                let mut y = rows_top + 2 * row + 30;
                for l in [clock, "Settings › System › Time has the rest."] {
                    if y > last {
                        break;
                    }
                    let l = text.fit(Face::Body, 24, l, width);
                    text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, &l);
                    y += crate::type_scale::line(24, 10);
                }
                let a = if z.card_row == 1 { "Open" } else { yes };
                // The first start: B goes back a stage, START skips the rest (opened from the Wi-Fi row, B goes back there).
                if self.zone_stage() {
                    hints(c, text, &[("A", a), ("B", "Back"), ("START", "Skip the rest")]);
                } else {
                    hints(c, text, &[("A", a), ("START", "Next"), ("B", "Back")]);
                }
            }
            ZoneView::Regions { sel } => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Time zone");
                text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, "Choose your region, or search for your city.");
                let mut rows = vec!["Search…".to_string()];
                rows.extend(timezone::regions(&z.all));
                self.paint_zone_rows(c, text, &rows.iter().map(|r| (r.clone(), ">".to_string())).collect::<Vec<_>>(), *sel);
                // L1/R1 page only a list longer than the screen.
                let mut bar = vec![("A", "Open"), ("L1/R1", "Page"), ("START", "Next"), ("B", "Back")];
                if rows.len() <= zone_rows_visible(c.h) {
                    bar.remove(1);
                }
                hints(c, text, &bar);
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
                    let mut rows: Vec<(String, String)> = zones
                        .iter()
                        .map(|zone| {
                            // Blank until the thread has read the zone's file.
                            let value = z
                                .offsets
                                .get(zone)
                                .copied()
                                .flatten()
                                .map_or(String::new(), |o| format!("{}   {}", timezone::fmt_offset(o), timezone::clock_at(now, o)));
                            (timezone::city_here(zone), value)
                        })
                        .collect();
                    // A country's own list: the other places last, a row as X's way there.
                    if matches!(from, ListFrom::Country) {
                        rows.push(("Other places…".to_string(), ">".to_string()));
                    }
                    self.paint_zone_rows(c, text, &rows, *sel);
                }
                let mut bar = zone_list_hints(matches!(from, ListFrom::Country), self.zone_stage(), zones.is_empty());
                // L1/R1 page only a list longer than the screen (Arctic, Other: one page).
                if zones.len() + usize::from(matches!(from, ListFrom::Country)) <= zone_rows_visible(c.h) {
                    bar.retain(|(k, _)| *k != "L1/R1");
                }
                if matches!(from, ListFrom::Country) && *sel == zones.len() {
                    bar[0] = ("A", "Open");
                }
                hints(c, text, &bar);
            }
            ZoneView::Search(kb) => {
                text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, "Search time zones");
                paint_keyboard_with(c, text, kb, "Search");
                let mut bar = vec![("A", "Type"), if kb.text.is_empty() { ("B", "Back") } else { ("B", "Delete") }];
                if !kb.text.is_empty() {
                    bar.push(("L1", "Clear"));
                }
                bar.extend([("X", "Shift"), ("Y", "Symbols"), ("START", "Search")]);
                hints(c, text, &bar);
            }
        }
    }

    /// Rows of (name, value) under the heading, the selected one highlighted, scrolled to keep it in view.
    fn paint_zone_rows(&self, c: &mut Canvas, text: &mut Text, rows: &[(String, String)], sel: usize) {
        let (top, row) = (232, crate::type_scale::row(70, 37));
        let visible = zone_rows_visible(c.h);
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

    /// The look step's rows from the top: the layouts, then Appearance, Motion and Animations (Settings > Look; Style and
    /// Colours are rows of Settings › Look itself) or Style, Colours and Appearance (the first start: how every page
    /// looks, shown on the previews as it changes; 2026-11-07: Retro is to be seen, not hidden behind a key).
    fn look_rows(&self) -> &'static [LookRow] {
        if self.only.is_some() {
            &[LookRow::Layout, LookRow::Appearance, LookRow::Motion, LookRow::Animations]
        } else {
            &[LookRow::Layout, LookRow::Font, LookRow::Colours, LookRow::Appearance]
        }
    }

    /// The first start again, from Settings › Help & tools: B on the welcome goes back to Settings.
    pub fn again(mut self) -> Self {
        self.again = true;
        self
    }

    /// The setup ended with B on the welcome of a first start opened from Settings: back to Settings, nothing finished.
    pub fn backed_out(&self) -> bool {
        self.backed_out
    }

    /// B on Settings > Look: everything the page changed goes back to how it was (the app takes it as it takes a
    /// change: the layout through `take_profile_look`, the rest from `appearance`, `motion`, `take_animations`).
    fn cancel_look(&mut self) {
        let Some((theme, appearance, motion, animations)) = self.look_opened.take() else { return };
        if self.theme != theme {
            self.theme = theme;
            self.profile_look = Some(theme);
        }
        (self.appearance, self.motion) = (appearance, motion);
        if self.animations != animations {
            self.animations = animations;
            self.animations_changed = true;
        }
    }

    /// Left/Right on the look step: the focused row's next or previous choice, round at the ends as the Style panel's.
    fn change_look_row(&mut self, forward: bool) {
        fn turn<T: Copy + PartialEq>(all: &[T], now: T, forward: bool) -> T {
            let (i, n) = (all.iter().position(|a| *a == now).unwrap_or(0), all.len());
            all[if forward { (i + 1) % n } else { (i + n - 1) % n }]
        }
        match self.look_rows().get(self.look_row) {
            Some(LookRow::Layout) => {
                let n = self.themes.len();
                self.theme = if forward { (self.theme + 1) % n } else { (self.theme + n - 1) % n };
            }
            Some(LookRow::Appearance) => self.appearance = turn(&Appearance::ALL, self.appearance, forward),
            Some(LookRow::Motion) => self.motion = turn(&Motion::ALL, self.motion, forward),
            Some(LookRow::Animations) => {
                self.animations = Some(turn(&Animations::ALL, self.animations_shown(), forward));
                self.animations_changed = true;
            }
            // The app steps them (`take_font_step`, `take_colours_step`), as Settings › Look's rows do.
            Some(LookRow::Font) => self.font_step = Some(forward),
            Some(LookRow::Colours) => self.colours_step = Some(forward),
            None => {}
        }
    }

    /// The look step: each look's real shelf, scaled down (`previews`, one per theme, `LOOK_PREVIEW` in size,
    /// made by the app), the one shown in the middle and its neighbours at the sides; then its name, one line
    /// about it, and how to switch. `sample`: the previews use the built-in sample games.
    pub fn paint_look(&mut self, c: &mut Canvas, text: &mut Text, previews: &[Image], sample: bool) {
        c.clear(pal().void);
        let first = self.only.is_none();
        let title = if first { "How your games stand" } else { "Choose a look" };
        // Cut to the page: Spanish in the Retro style ran off a 1024 px screen.
        let title = text.fit(Face::Title, 67, title, c.w - 2 * MARGIN);
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, &title);
        let sub = match (first, sample) {
            (true, true) => "Pick how your games line up. Shown with sample games until yours are here.",
            (true, false) => "Pick how your games line up. These are your own games: try each one.",
            (false, true) => "How the shelf looks, with sample games until yours are on the SD card.",
            (false, false) => "Your shelf, in each look.",
        };
        // Smaller type before "…": German and Spanish at large text ended in "Probier j…" (first-start walk 2026-11-06).
        let (sub_px, sub) = text.fit_sizes(Face::Body, &[28, 25, 22], sub, c.w - 2 * MARGIN);
        text.draw(c, Face::Body, sub_px, MARGIN, 192, pal().dust, &sub);
        self.paint_step_count(c, text);
        // The first start has a line under its rows (what HOME does in a game): the pictures give up its height.
        let (pw, ph) = look_preview((c.w, c.h));
        let (pw, ph) = if first { let short = (ph - 3 * crate::type_scale::line(24, 8)).max(44); (pw * short / ph, short) } else { (pw, ph) };
        let (x, y) = ((c.w - pw) / 2, 226);
        let n = self.themes.len();
        if previews.len() == n {
            // The neighbours, dimmed and cut off by the screen's edges: there is more to either side.
            for (dx, i) in [(-(pw + 36), (self.theme + n - 1) % n), (pw + 36, (self.theme + 1) % n)] {
                crate::retro::pill(c, x + dx - 8, y - 8, pw + 16, ph + 16, 16, pal().spine);
                c.blit_scaled(&previews[i], x + dx, y, pw, ph);
                c.shade_rect(x + dx - 8, y - 8, pw + 16, ph + 16, pal().void, 170);
            }
            crate::retro::pill(c, x - 8, y - 8, pw + 16, ph + 16, 16, pal().keycap);
            c.blit_scaled(&previews[self.theme], x, y, pw, ph);
        }
        let (name, blurb) = self.themes[self.theme];
        let base = y + ph + 62 + crate::type_scale::grow(50);
        text.draw(c, Face::Title, 50, MARGIN, base, pal().chalk, name);
        // The layouts focused: the bar beside their name, as beside a row of pills. For the navigation explorer the
        // layouts are one row from the pictures to the name.
        crate::nav_probe::row(c, (MARGIN, y - 8, c.w - 2 * MARGIN, base + 12 - (y - 8)));
        if self.look_row == 0 {
            let cap = crate::type_scale::px(50) as i32 * 3 / 4;
            focus_bar(c, base - cap - 6, cap + 12);
            crate::nav_probe::focus(c, (MARGIN - 28, y - 8, c.w - 2 * MARGIN + 28, base + 12 - (y - 8)));
        }
        // Easy on a card with many games: says so, nothing more.
        let blurb = if crate::settings::Theme::ALL.get(self.theme) == Some(&crate::settings::Theme::Big) && self.games.len() > HANDFUL { "One big game at a time, simple menus. Best with a handful of games." } else { blurb };
        let blurb = text.fit(Face::Body, 28, blurb, c.w - 2 * MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, base + 40 + crate::type_scale::grow(28), pal().dust, &blurb);
        // One mark per theme, the shown one filled.
        for i in 0..n as i32 {
            let mx = c.w - MARGIN - (n as i32 - i) * 28 + 12;
            let color = if i as usize == self.theme { pal().chalk } else { pal().keycap };
            c.rect(mx, base - 18, 16, 16, color);
        }
        // The rows under the layouts, slimmer rows of pills one under the other as `look_rows` has them; the focused
        // one has the bar beside it. Settings > Look: Appearance, Motion, Animations. The first start: Style, Colours,
        // Appearance, the previews above changing with them.
        let top = base + 63 + crate::type_scale::grow(28);
        let step = pill_h() + 15;
        let label_w = self.label_w(text);
        let rows = &self.look_rows()[1..];
        for (i, row) in rows.iter().enumerate() {
            self.paint_look_row(c, text, *row, top + i as i32 * step, label_w);
        }
        if self.look_row > 0 {
            focus_bar(c, top + (self.look_row as i32 - 1) * step, pill_h());
        }
        if first {
            self.paint_look_end(c, text, top + rows.len() as i32 * step + 10 + crate::type_scale::mid(24));
            return;
        }
        hints(c, text, &look_hints(true, self.look_row > 0));
    }

    /// One row of pills under the layouts, from `top`.
    fn paint_look_row(&self, c: &mut Canvas, text: &mut Text, row: LookRow, top: i32, label_w: i32) {
        match row {
            LookRow::Layout => {}
            LookRow::Appearance => {
                let names: Vec<&str> = Appearance::ALL.iter().map(|a| a.name()).collect();
                let on = Appearance::ALL.iter().position(|a| *a == self.appearance).unwrap_or(0);
                pill_row(c, text, top, "Appearance", &names, on, self.appearance.blurb(), label_w);
            }
            LookRow::Motion => {
                let names: Vec<&str> = Motion::ALL.iter().map(|m| m.name()).collect();
                let on = Motion::ALL.iter().position(|m| *m == self.motion).unwrap_or(0);
                pill_row(c, text, top, "Motion", &names, on, self.motion.blurb(), label_w);
            }
            LookRow::Animations => {
                let names: Vec<&str> = Animations::ALL.iter().map(|a| a.name()).collect();
                let shown = self.animations_shown();
                let on = Animations::ALL.iter().position(|a| *a == shown).unwrap_or(0);
                pill_row(c, text, top, "Animations", &names, on, shown.blurb(), label_w);
            }
            // Barlow, Retro: as Settings › Look › Style, and every page in it from the next frame on.
            LookRow::Font => {
                let now = crate::font_style::get();
                let names: Vec<&str> = crate::font_style::FontStyle::ALL.iter().map(|f| f.label()).collect();
                let on = crate::font_style::FontStyle::ALL.iter().position(|f| *f == now).unwrap_or(0);
                pill_row(c, text, top, "Style", &names, on, now.blurb(), label_w);
            }
            // Five sets (Retro: its own two) are too many pills for a small screen: the one in use, Left/Right step on.
            LookRow::Colours => {
                let (name, blurb) = if crate::retro::on() {
                    (crate::retro::colours().label(), crate::retro::colours().blurb())
                } else {
                    (crate::palette::colours().label(), crate::palette::colours().blurb())
                };
                let shown = format!("‹ {name} ›");
                pill_row(c, text, top, "Colours", &[shown.as_str()], 0, blurb, label_w);
            }
        }
    }

    /// Under the first start's rows: the lines beside Finish (what HOME does in a game, where the look changes later),
    /// as many as fit above the hint bar.
    fn paint_look_end(&self, c: &mut Canvas, text: &mut Text, top: i32) {
        let width = c.w - 2 * MARGIN;
        let step = crate::type_scale::line(24, 8);
        let more = text.wrap_prose_at_most(Face::Body, 24, "More to get any time: Settings › Games › Extras.", width, 2);
        let last = crate::kit::hint_bar_top(c.h) - 16 - more.len() as i32 * step;
        let mut lines: Vec<&str> = Vec::new();
        if self.kids_skipped {
            lines.push("Kids Mode is there whenever you want it: Settings › System › Kids mode.");
        }
        if self.kids_later {
            lines.push("Kids Mode needs games it can start. Set it up later in Settings › System › Kids mode.");
        }
        if self.home_line.get().unwrap_or_else(|| {
            let v = crate::first_game::told_on_last_page(&self.data_dir);
            self.home_line.set(Some(v));
            v
        }) {
            lines.push(crate::first_game::LAST_PAGE_LINE);
        }
        lines.push("Change the look any time in Settings › Look.");
        // Whole lines only, and none over the hint bar: the last ones give way on a short screen.
        let mut y = top;
        'lines: for l in lines {
            let wrapped = text.wrap_prose_at_most(Face::Body, 24, l, width, 2);
            if y + (wrapped.len() as i32 - 1) * step > last {
                break 'lines;
            }
            for w in wrapped {
                text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, &w);
                y += step;
            }
        }
        for line in more {
            text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, &line);
            y += step;
        }
        hints(c, text, &look_hints(false, self.look_row > 0));
    }

    /// Big text as the look step shows it: Large text and a Motion that isn't Full (`features::reading_on`).
    fn easy_to_read(&self) -> bool {
        features::reading_on(crate::type_scale::get(), self.motion)
    }

    /// R1 on the first start's look step: Big text on or off, through the Tour card's own switch, so both set Text
    /// size and Motion the same way. The app keeps both (`take_text_size`, `motion`).
    fn switch_easy_to_read(&mut self) {
        let Some(switch) = features::by_id("easy_to_read").and_then(|i| features::FEATURES[i].switch) else { return };
        let mut s = self.switches.clone();
        (s.text_size, s.motion) = (crate::type_scale::get(), self.motion);
        (switch.set)(&mut s, !self.easy_to_read());
        crate::type_scale::set(s.text_size);
        self.text_size_chosen = Some(s.text_size);
        self.motion = s.motion;
    }




    /// The welcome's Big text row under the language: Off / On (L2 on every stage), Large text and calmer menus.
    fn paint_easy_to_read(&self, c: &mut Canvas, text: &mut Text, top: i32, label_w: i32) {
        let on = self.easy_to_read();
        let blurb = if on { "Larger text and calmer menus, on every page." } else { "Larger text and calmer menus. L2 on every page." };
        pill_row(c, text, top, "Big text", &["Off", "On"], usize::from(on), blurb, label_w);
    }

    /// The labels of the pill rows line up.
    fn label_w(&self, text: &mut Text) -> i32 {
        ["Appearance", "Motion", "Animations", "Style", "Colours"].iter().map(|l| text.width(Face::Body, 24, l)).max().unwrap_or(0)
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
            _ if self.advance_pending.is_some() => (format!("Connected to {}", self.advance_pending.as_deref().unwrap_or("")), pal().chalk),
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
                None => match self.wifi_tail_at(Some(i)) {
                    Some(WifiRow::Other) => ("Other network…".to_string(), "Hidden, by name".to_string(), None),
                    Some(WifiRow::Country) => ("Wi-Fi country".to_string(), self.country_value(), None),
                    Some(WifiRow::Zone) => ("Time zone".to_string(), self.zone_value(), None),
                    Some(WifiRow::Scan) => ("Scan again".to_string(), if self.scanning { "Scanning…".to_string() } else { String::new() }, None),
                    Some(WifiRow::Continue) if self.entries.iter().any(|e| e.active) => ("Continue".to_string(), String::new(), None),
                    Some(WifiRow::Continue) | None => ("Continue offline".to_string(), String::new(), None),
                },
            };
            let side_w = text.width(Face::Body, 24, &side);
            // A network's name is its own (a hidden one typed by the player): painted as it is, never translated.
            let ssid = i < self.entries.len();
            text.set_translate(!ssid);
            let name = text.fit(Face::Title, 37, &name, width - side_w - 180);
            text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
            text.set_translate(true);
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

/// Where "Your games" puts the first of its `rows` rows: from the bottom up, clear of the hint bar.
fn found_rows_top(h: i32, rows: usize) -> i32 {
    crate::kit::hint_bar_top(h) - 24 - rows as i32 * crate::type_scale::row(70, 37)
}

/// The look step's hint bar: the keys that work on the focused row (`below`: a row under the layouts). Left/Right change
/// the layout or the row's choice; A keeps (Settings) or finishes (the first start) from any row. B goes back: in Settings
/// with what the page changed put back, so it says Cancel (the Brick, 2026-11-07: no B on the bar, and B kept the layout).
fn look_hints(settings: bool, below: bool) -> Vec<(&'static str, &'static str)> {
    match (settings, below) {
        (true, false) => vec![("Left/Right", "Layout"), ("A", "Keep"), ("B", "Cancel")],
        (true, true) => vec![("Left/Right", "Change"), ("A", "Keep"), ("B", "Cancel")],
        (false, false) => vec![("Left/Right", "Layout"), ("A", "Finish"), ("B", "Back")],
        (false, true) => vec![("Left/Right", "Change"), ("A", "Finish"), ("B", "Back")],
    }
}

/// The focus of a page whose rows are pills, not list rows (the welcome, the look step): a bar in the margin beside the
/// row from `top`, `h` high.
fn focus_bar(c: &mut Canvas, top: i32, h: i32) {
    crate::retro::pill(c, MARGIN - 28, top + 6, 8, h - 12, 4, pal().label);
    // For the navigation explorer the focus is the whole row the bar stands beside.
    crate::nav_probe::focus(c, (MARGIN - 28, top, c.w - 2 * MARGIN + 28, h));
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
fn paragraphs(c: &mut Canvas, text: &mut Text, lines: &[&str], y: i32) -> i32 {
    paragraphs_at(c, text, 28, lines, y)
}

/// `paragraphs` in type of `px`.
fn paragraphs_at(c: &mut Canvas, text: &mut Text, px: u32, lines: &[&str], mut y: i32) -> i32 {
    let step = crate::type_scale::line(px, 12);
    let mut last = y;
    for line in lines {
        for part in text.wrap_prose(Face::Body, px, line, c.w - 2 * MARGIN) {
            text.draw(c, Face::Body, px, MARGIN, y, pal().dust, &part);
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
    paint_keyboard_room(c, text, kb, done, shown, 0);
}

/// `paint_keyboard_with` and a line of explanation under the keys, in the dim colour: at 790 px on a 1080-line screen,
/// right under the keys on a shorter one (1024x768, where the keys give up 40 px for it; at 790 it was off the picture).
pub fn paint_keyboard_noted(c: &mut Canvas, text: &mut Text, kb: &Keyboard, done: &str, note: &str) {
    let short = c.h < 900;
    let reserve = if short { 40 } else { 0 };
    paint_keyboard_room(c, text, kb, done, &kb.text, reserve);
    let (px, note) = text.fit_sizes(Face::Body, &[24, 21], note, c.w - 2 * MARGIN);
    let y = if short { c.h - 100 - reserve + 30 } else { 790 };
    text.draw(c, Face::Body, px, MARGIN, y, pal().dust, &note);
}

/// The keyboard, leaving `reserve` px above the hints free of keys.
fn paint_keyboard_room(c: &mut Canvas, text: &mut Text, kb: &Keyboard, done: &str, shown: &str, reserve: i32) {
    let width = c.w - 2 * MARGIN;
    // Text field with a caret; the Wi-Fi password is shown, it is typed on a handheld nobody else reads.
    c.rect(MARGIN, 150, width, 84, pal().label);
    // What is typed is the person's own: painted as it is, never through the translation table.
    text.set_translate(false);
    let shown = text.fit(Face::Title, 50, shown, width - 80);
    let w = text.draw(c, Face::Title, 50, MARGIN + 28, 210, pal().ink, &shown);
    text.set_translate(true);
    c.rect(MARGIN + 32 + w, 166, 4, 52, pal().ink);

    // 82 px keys on 1080 lines; the five rows share what is left above the hints on a screen of fewer (1024x768: 67).
    let (gap, top) = (10, 290);
    let key_h = ((c.h - 100 - reserve - top - 4 * gap) / 5).clamp(48, 82);
    for (r, row) in kb.rows().iter().enumerate() {
        let y = top + r as i32 * (key_h + gap);
        // Character rows share one key width; the special row gets wider keys.
        let widths: Vec<i32> = if r == 4 {
            // 170/170/380/170/170 shares of the field's width (on 1024 pixels the fixed sizes ran off the edge).
            [170, 170, 380, 170, 170].iter().map(|w| (width - gap * 4) * w / 1060).collect()
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
    crate::retro::pill(c, x, top, widths.iter().sum::<i32>() + 8, h, h / 2, pal().keycap);
    x += 4;
    for (i, (n, w)) in names.iter().zip(&widths).enumerate() {
        if i == on {
            crate::retro::pill(c, x, top + 4, *w, h - 8, (h - 8) / 2, pal().label);
        }
        text.draw(c, Face::Body, px, x + 20, base, if i == on { pal().ink } else { pal().chalk }, n);
        x += w;
    }
    // A row of choices a player expects to reach and change (the explorer, `nav_probe`): its label and its pills.
    crate::nav_probe::row(c, (MARGIN, top, x + 4 - MARGIN, h));
    let room = c.w - MARGIN - x - 28;
    // The welcome step's one row has the page below it: a note that doesn't fit beside the pills (1024x768, the
    // Spanish one) goes under the row, whole, rather than "Settin…"
    // Only where the hint bar stays clear (a page with the "not tested on this device" paragraph is full).
    if label_w == 0 && text.width(Face::Body, px, blurb) > room {
        let lines = text.wrap(Face::Body, px, blurb, c.w - 2 * MARGIN);
        let step = crate::type_scale::line(px, 8);
        let first = base + h / 2 + 34;
        if first + (lines.len() as i32 - 1) * step < c.h - 120 {
            for (i, line) in lines.iter().enumerate() {
                text.draw(c, Face::Body, px, MARGIN, first + i as i32 * step, pal().dust, line);
            }
            return;
        }
    }
    // Smaller type before "…": "Black pages. Uses the least batte…" at 1024 px.
    let (npx, note) = text.fit_sizes(Face::Body, &[px, 22, 20], blurb, room);
    text.draw(c, Face::Body, npx, x + 28, base, pal().dust, &note);
}

/// The Extras page's line about the picture downloads: where from, what is sent, and what Off does (the sizes
/// are in docs/user/extras.md).
const PICTURES_NOTE: &str = "Pictures come from a free collection online: no account is needed, and only a game's name is sent. Off downloads nothing by itself.";

/// The height of a slim row of pills (the look step's two), 40 px around Body 24.
fn pill_h() -> i32 {
    crate::type_scale::row(40, 24)
}

/// The hint bar of the setup pages; returns where each pill is, for touch (pills of combined keys are left out).
/// The hint bar of the Wi-Fi page. `wizard`: the first start, where B goes back a stage like on every stage, the
/// Continue row goes on (online or not) and START skips the rest; Y still forgets there, without a hint (four at most).
/// In Settings, B (and START) leave. `scan`: the radio is on (X with Wi-Fi off did nothing, and said "Scan again");
/// X is a shortcut for the Scan again row, so it is not named on that row itself.
fn wifi_hints(a: &'static str, saved: bool, forgetting: bool, wizard: bool, online: bool, scan: bool) -> Vec<(&'static str, &'static str)> {
    let _ = online;
    let rescan = if scan && a != "Scan again" { vec![("X", "Scan again")] } else { Vec::new() };
    if wizard {
        return [vec![("A", a), ("B", "Back")], rescan, vec![("START", "Skip the rest")]].concat();
    }
    let mut h = [vec![("A", a)], rescan].concat();
    if saved {
        h.push(("Y", if forgetting { "Forget now" } else { "Forget" }));
    }
    h.push(("B", "Back"));
    h
}

/// How many rows a time zone list shows on a screen `h` high.
fn zone_rows_visible(h: i32) -> usize {
    ((h - 130 - 232) / crate::type_scale::row(70, 37)).max(1) as usize
}

/// The hint bar of a time zone list: a country's own (`country`), in the first start's stage (`stage`) or not; a search
/// that found nothing (`empty`) has nothing to set or page through, B goes back to it.
fn zone_list_hints(country: bool, stage: bool, empty: bool) -> Vec<(&'static str, &'static str)> {
    match (country, stage) {
        (true, true) => vec![("A", "Set"), ("B", "Back"), ("START", "Skip the rest")],
        (true, false) => vec![("A", "Set"), ("START", "Skip"), ("B", "Back")],
        _ if empty => vec![("START", "Next"), ("B", "Back")],
        _ => vec![("A", "Set"), ("L1/R1", "Page"), ("START", "Next"), ("B", "Back")],
    }
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
    if crate::phone_upload_page::offered() {
        h.push(("Y", "From your phone"));
    }
    // START did the same as B (back to "Your games" in the first start, out of the page from Settings): B alone.
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
    use crate::system_ctl::{Hw, MockHw, MockState};
    use crate::testutil::TestDir;

    const IW_AT: &str = "global\ncountry 00: DFS-UNSET\n\nphy#0 (self-managed)\ncountry AT: DFS-ETSI\n\t(2402 - 2472 @ 40), (N/A, 20), (N/A)\n";
    const IW_US: &str = "global\ncountry US: DFS-FCC\n\t(2402 - 2472 @ 40), (N/A, 30), (N/A)\n";

    /// A zoneinfo folder with a few zones and the two tables (`testdata/zoneinfo`; empty zone files are enough for the
    /// lists). Read-only, so every test shares it.
    fn zoneinfo() -> std::path::PathBuf {
        crate::testutil::manifest_dir().join("testdata/zoneinfo")
    }

    /// A setup on a mock device whose zone and Wi-Fi regulatory domain are as given.
    fn setup(name: &str, zone: &str, reg: Option<&str>) -> (Setup, Arc<MockHw>) {
        let hw = Arc::new(mock(name, zone, reg));
        (setup_on(hw.clone(), name), hw)
    }


    /// Review 2026-11-01: sharing that came back by itself after the page opened: the page kept saying off, and X
    /// switched on what was on.
    #[test]
    fn the_open_page_follows_file_sharing_as_it_changes() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static ON: AtomicBool = AtomicBool::new(false);
        fn ask() -> ShareNow {
            let on = ON.load(Ordering::SeqCst);
            ShareNow { on, read_only: on, ip: on.then(|| "192.0.2.23".to_string()) }
        }
        let (sink, seen) = std::sync::mpsc::channel();
        let sink = std::sync::Mutex::new(sink);
        let (mut s, _hw) = setup("share-watch", "UTC", None);
        s.notify = Arc::new(move |ev| {
            let _ = sink.lock().unwrap().send(ev);
        });
        s.share_every = std::time::Duration::from_millis(5);
        s.share_ask = ask;
        s.step = Step::Games;
        s.keep_watch();
        let changed = |want: bool| loop {
            let ev = seen.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(10))).expect("the page hears of it");
            if matches!(&ev, SetupEvent::ShareChanged { now, .. } if now.on == want) {
                return ev;
            }
        };
        s.on_event(changed(false));
        assert!(!s.sharing);
        ON.store(true, Ordering::SeqCst);
        s.on_event(changed(true));
        assert!(s.sharing && s.share_read_only, "the page says what the system says");
        assert_eq!(s.ip.as_deref(), Some("192.0.2.23"));
        assert_eq!(games_hints(None, s.sharing, s.ip.is_some())[0], ("X", "Turn off file sharing"), "X does what it says");
        s.step = Step::Done;
        s.keep_watch();
        assert!(s.share_watch.is_none(), "it ends with the page");
    }

    #[test]
    fn extras_without_a_wifi_chip_does_not_ask_to_connect() {
        assert_eq!(extras_sub(false, true), "Connect to Wi-Fi first to use these.");
        assert_eq!(extras_sub(false, false), "This device has no Wi-Fi, so the downloads aren't available.");
        assert_eq!(extras_sub(true, false), "Optional. You can get these any time later.");
    }

    #[test]
    fn the_extras_counter_says_what_it_counts() {
        assert_eq!(extras_status(None, false, true, 6, ""), "6 games");
        assert_eq!(extras_status(None, false, true, 1, ""), "1 game", "first-start test 2026-11-01: was \"1 games\"");
        assert_eq!(extras_status(None, false, true, 0, ""), "no games yet");
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
            if matches!(e, SetupEvent::Zone(_) | SetupEvent::ZoneGuess(_) | SetupEvent::ZoneOffsets(_) | SetupEvent::GameCounts(_) | SetupEvent::Net | SetupEvent::PortMasterStatus(_) | SetupEvent::PortMasterSpace(_) | SetupEvent::PortMasterProgress(_) | SetupEvent::PortMasterDone(_) | SetupEvent::PortMasterRemoved(_)) {
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

    /// First start up to the Wi-Fi step: A on the welcome.
    fn at_wifi(s: &mut Setup) {
        press(s, &[Button::East]);
        assert_eq!(s.at(), "wifi");
    }

    /// The first start's Wi-Fi page left the way a player leaves it without connecting: Down to its last row, Continue
    /// offline, and A (B went on there before 2026-11-07; it goes back now).
    fn wifi_continue(s: &mut Setup) {
        assert_eq!(s.at(), "wifi");
        for _ in 0..s.wifi_rows() {
            press(s, &[Button::Down]);
        }
        assert_eq!(s.wifi_tail_at(s.sel.checked_sub(s.lead())), Some(WifiRow::Continue), "the last row is Continue");
        press(s, &[Button::East]);
    }

    /// "Your games" left the way on: Continue, or I'll add games later on a card without games.
    fn found_on(s: &mut Setup) {
        assert_eq!(s.at(), "your games");
        let rows = s.found_rows();
        let at = rows.iter().position(|r| matches!(r, FoundRow::Continue | FoundRow::Later)).expect("a way on");
        press(s, &vec![Button::Up; rows.len()]);
        press(s, &vec![Button::Down; at]);
        press(s, &[Button::East]);
    }

    /// Selects the Wi-Fi page's row `row` (after the networks).
    fn select_wifi(s: &mut Setup, row: WifiRow) {
        let at = s.wifi_tail().iter().position(|r| *r == row).expect("the row is there");
        s.sel = s.lead() + s.entries.len() + at;
    }

    /// Where the first start is, with its dot: ("wifi", 2 of 4).
    fn where_(s: &Setup) -> (String, Option<(usize, usize)>) {
        (s.at(), s.step_of())
    }

    /// A fresh data folder of its own for a test.
    fn fresh(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pocketui-setup-data-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The first start on a handheld with Wi-Fi: language, Wi-Fi (the time zone set from the Wi-Fi region and shown on its
    /// row), your games, the look; four dots; the way on is a row (Continue) or A, A on the look finishes.
    #[test]
    fn the_first_start_is_language_wifi_games_look() {
        fresh("flow4");
        let (mut s, hw) = setup("flow4", "UTC", Some(IW_AT));
        let mut seen = vec![where_(&s)];
        press(&mut s, &[Button::East]);
        seen.push(where_(&s));
        // Austria has one zone: taken at once, said on the Time zone row, never asked.
        assert_eq!(hw.timezone().as_deref(), Some("Europe/Vienna"));
        assert!(s.zone_value().starts_with("Vienna"), "{}", s.zone_value());
        wifi_continue(&mut s);
        seen.push(where_(&s));
        found_on(&mut s);
        seen.push(where_(&s));
        let want = [("welcome", 1), ("wifi", 2), ("your games", 3), ("look", 4)];
        assert_eq!(seen, want.map(|(a, n)| (a.to_string(), Some((n, 4)))));
        assert!(s.press(Button::East), "Finish ends the setup: the shelf is next, no extra page");
        // Settings pages count nothing.
        assert_eq!(shelf_setup("flow4-settings").only(Only::Wifi).step_of(), None);
    }

    /// No clear guess (no Wi-Fi region, the zone nobody chose): the time zone is asked inside the Wi-Fi stage, and Keep
    /// UTC goes on without setting anything.
    #[test]
    fn an_unsure_time_zone_is_asked_after_wifi_and_can_be_skipped() {
        fresh("zone-unsure");
        let (mut s, hw) = setup("zone-unsure", "UTC", None);
        at_wifi(&mut s);
        assert_eq!(s.zone_value(), "Choose");
        wifi_continue(&mut s);
        assert_eq!(where_(&s), ("zone".to_string(), Some((2, 4))), "the second dot still: part of the Wi-Fi stage");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "your games");
        assert_eq!(hw.timezone().as_deref(), Some("UTC"), "skipped: nothing set");
        assert!(s.zone_utc, "the clock stays on UTC, unconfirmed");
    }

    /// First-start walk 2026-11-06: skipped once, the time zone was asked again every time Wi-Fi was left (L1 back and
    /// on, a connection later). Once is enough; the Wi-Fi page's Time zone row still opens it.
    #[test]
    fn a_skipped_time_zone_is_not_asked_again() {
        fresh("zone-once");
        let (mut s, hw) = setup("zone-once", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "your games");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi");
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games", "skipped once: not asked again");
        assert_eq!(hw.timezone().as_deref(), Some("UTC"));
        press(&mut s, &[Button::L1]);
        select_wifi(&mut s, WifiRow::Zone);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: regions", "the row still opens it");
    }

    /// A zone chosen in the stage's list stays when a later connection guesses one (the guess is for an unset clock).
    #[test]
    fn a_zone_chosen_in_the_stage_is_kept_after_a_later_connection() {
        fresh("zone-chosen-kept");
        let (mut s, hw) = setup("zone-chosen-kept", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
        press(&mut s, &[Button::North, Button::Down, Button::East, Button::East]);
        assert_eq!(s.at(), "your games");
        let chosen = hw.timezone().expect("a zone");
        assert_ne!(chosen, "UTC");
        hw.with(|st| st.regdomain = Some(IW_AT.into()));
        s.step = Step::Wifi;
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        s.settle();
        assert_eq!(hw.timezone(), Some(chosen), "the chosen zone stays");
    }

    /// Wi-Fi opened from the phone page ("Connect to Wi-Fi first", A) and left offline with B: back to Add games, not on
    /// to the time zone or "Your games" (online, `user_journeys` sees the phone page come back).
    #[test]
    fn wifi_from_the_phone_page_goes_back_to_add_games() {
        fresh("phone-wifi-back");
        let (mut s, _hw) = setup("phone-wifi-back", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "your games");
        s.go_to_games();
        s.ip = None;
        press(&mut s, &[Button::West]);
        assert_eq!(s.at(), "phone");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi");
        s.ip = None;
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "games", "back where Wi-Fi was asked for");
        // L1 out of that Wi-Fi page forgets where it came from.
        s.wifi_for_phone = true;
        s.step = Step::Wifi;
        press(&mut s, &[Button::L1]);
        assert!(!s.wifi_for_phone);
    }

    /// The Wi-Fi page's Time zone row: a zone picked there stays (the guess after Wi-Fi doesn't touch it), and B comes
    /// back to the Wi-Fi page.
    #[test]
    fn the_time_zone_row_changes_the_zone_and_comes_back_to_wifi() {
        fresh("zone-row");
        let (mut s, hw) = setup("zone-row", "UTC", None);
        at_wifi(&mut s);
        select_wifi(&mut s, WifiRow::Zone);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: regions", "A on the row opens the picker");
        press(&mut s, &[Button::South, Button::South]);
        assert_eq!(s.at(), "wifi", "B goes back to Wi-Fi, not on");
        select_wifi(&mut s, WifiRow::Zone);
        press(&mut s, &[Button::East, Button::Down, Button::East, Button::East]);
        assert_eq!(s.at(), "wifi", "a pick comes back to Wi-Fi");
        let picked = hw.timezone().expect("a zone");
        assert_ne!(picked, "UTC");
        assert!(s.zone_value().starts_with(&timezone::city_here(&picked)), "the row says the zone picked: {}", s.zone_value());
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games", "picked: not asked again");
        assert_eq!(hw.timezone().as_deref(), Some(picked.as_str()));
    }

    /// RG28XX, RG35XX 2024: no Wi-Fi chip, so the time zone takes the Wi-Fi stage's place; still four dots.
    #[test]
    fn a_handheld_without_wifi_asks_the_time_zone_in_wifis_place() {
        fresh("flow-nowifi");
        let (mut s, _hw) = setup("flow-nowifi", "UTC", None);
        s.no_wifi = true;
        press(&mut s, &[Button::East]);
        assert_eq!(where_(&s), ("zone".to_string(), Some((2, 4))));
        press(&mut s, &[Button::East]);
        assert_eq!(where_(&s), ("your games".to_string(), Some((3, 4))));
        assert!(!s.found_rows().contains(&FoundRow::BoxArt), "no downloads without Wi-Fi: no Box art switch");
        // Painted at the H700's size, the pages are there.
        for (w, h) in [(1240, 1080), (640, 480)] {
            let mut px = vec![0u32; (w * h) as usize];
            s.paint(&mut Canvas::new(&mut px, w, h), &mut Text::builtin());
            assert!(px.iter().any(|p| *p != px[0]), "{w}x{h} is not blank");
        }
        found_on(&mut s);
        assert_eq!(where_(&s), ("look".to_string(), Some((4, 4))));
    }

    /// START on any stage skips the rest of the setup (BananaOS's way); inside a keyboard START is the keyboard's.
    #[test]
    fn start_skips_the_rest_but_not_inside_a_keyboard() {
        fresh("skip-rest");
        let (mut s, _hw) = setup("skip-rest", "UTC", Some(IW_AT));
        assert!(s.press(Button::Start), "on the welcome");
        let (mut s, _hw) = setup("skip-rest", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        assert!(s.press(Button::Start), "on Wi-Fi");
        let mut s = wifi_setup("skip-rest-kb");
        s.only = None;
        s.step = Step::HiddenName { kb: Keyboard::default() };
        assert!(!s.press(Button::Start), "the keyboard's START is its own");
    }

    /// L1 goes back a stage (Alex's review of the first start, 2026-11-06): the look to your games, your games to
    /// Wi-Fi, Wi-Fi to the welcome; on the welcome nothing; B still skips forward; a keyboard's L1 stays its own.
    /// One scheme for every page (2026-11-07): B skipped a first-start stage forward and went back everywhere else, and
    /// some ways on were keys only. Now B (and L1) go back a stage, and every way on is a row Down reaches or A itself.
    #[test]
    fn b_goes_back_a_stage_and_every_way_on_is_a_row() {
        fresh("b-back");
        let (mut s, _hw) = setup("b-back", "UTC", Some(IW_AT));
        s.games = vec![("snes".into(), PathBuf::from("/storage/roms/snes/a.sfc"))];
        assert!(!s.press(Button::South));
        assert!(!s.press(Button::L1));
        assert_eq!(where_(&s).1, Some((1, 4)), "the welcome has nothing before it: B and L1 stay");
        at_wifi(&mut s);
        press(&mut s, &[Button::South]);
        assert_eq!(where_(&s).1, Some((1, 4)), "B: Wi-Fi back to the welcome");
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games", "Continue offline, a row, goes on");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi", "B: your games back to Wi-Fi");
        wifi_continue(&mut s);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "look", "Continue, the first row of your games");
        assert!(!s.press(Button::South), "B on the last stage never ends the setup");
        assert_eq!(s.at(), "your games", "B: the look back to your games");
        press(&mut s, &[Button::East, Button::L1]);
        assert_eq!(s.at(), "your games", "L1 does the same");
        let mut s = wifi_setup("b-back-kb");
        s.only = None;
        s.step = Step::HiddenName { kb: Keyboard::default() };
        press(&mut s, &[Button::L1]);
        assert!(matches!(s.step, Step::HiddenName { .. }), "in a keyboard L1 clears, it doesn't leave");
    }

    /// Settings › Help & tools › First-start setup: B on its welcome goes back to Settings (it went on to Wi-Fi, and there
    /// was no way back); the first start itself has nothing before the welcome.
    #[test]
    fn the_first_start_from_settings_goes_back_there_with_b() {
        fresh("again");
        let (s, _hw) = setup("again", "UTC", None);
        let mut s = s.again();
        assert!(s.press(Button::South), "B ends it");
        assert!(s.backed_out(), "backed out: nothing finished");
        let (mut s, _hw) = setup("again-first", "UTC", None);
        assert!(!s.press(Button::South) && !s.backed_out() && s.at() == "welcome");
    }

    /// The welcome's three rows: the language, Big text, and For a child, which only Y reached before.
    #[test]
    fn the_welcome_reaches_for_a_child_with_the_d_pad() {
        fresh("welcome-rows");
        let (mut s, _hw) = setup("welcome-rows", "UTC", None);
        if !kids_offered() {
            return;
        }
        press(&mut s, &[Button::Down, Button::Down, Button::Right]);
        assert!(s.kids_wanted, "Down, Down, Right: for a child");
        press(&mut s, &[Button::Down, Button::Left]);
        assert!(!s.kids_wanted, "the last row stays the last; Left takes the wish back");
        press(&mut s, &[Button::West]);
        assert!(s.kids_wanted, "Y still does it, as a shortcut");
        assert!(!s.press(Button::South) && s.at() == "welcome", "B has nothing to go back to");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi", "A gets started from any row");
    }

    /// The time zone's card: Looks right and Choose another… are rows (X was the only way to another zone), B goes back
    /// to Wi-Fi.
    #[test]
    fn the_time_zone_card_has_its_ways_as_rows() {
        fresh("zone-rows");
        let (mut s, _hw) = setup("zone-rows", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone", "no guess: the card asks");
        press(&mut s, &[Button::Down, Button::East]);
        assert_eq!(s.at(), "zone: regions", "Choose another…, a row");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi", "B on the card: back to Wi-Fi");
    }

    /// First-start walk 2026-11-06: L1 on the time zone went to the welcome, past the Wi-Fi page it belongs to; and the
    /// top row says L1 where it goes back, not in the zone's lists (L1 pages there) nor on the welcome.
    #[test]
    fn l1_on_the_time_zone_goes_back_to_wifi_and_is_shown_where_it_works() {
        fresh("l1-zone");
        let (mut s, _hw) = setup("l1-zone", "UTC", None);
        assert!(!s.l1_goes_back(), "nothing before the welcome");
        at_wifi(&mut s);
        assert!(s.l1_goes_back());
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone");
        assert!(s.l1_goes_back());
        press(&mut s, &[Button::North]);
        assert_eq!(s.at(), "zone: regions");
        assert!(!s.l1_goes_back(), "the regions page with L1");
        press(&mut s, &[Button::South]);
        press(&mut s, &[Button::L1]);
        assert_eq!(s.at(), "wifi", "the time zone back to its Wi-Fi page");
        // Without a Wi-Fi chip the time zone is the stage after the welcome.
        let (mut s, _hw) = setup("l1-zone-nowifi", "UTC", None);
        s.no_wifi = true;
        press(&mut s, &[Button::East, Button::L1]);
        assert_eq!(s.at(), "welcome");
        assert!(!shelf_setup("l1-zone-settings").only(Only::Wifi).l1_goes_back(), "Settings pages have no stages");
    }

    /// The zone search: no hit offers no "A Set" (A did nothing there), and L1 empties the field as in the password.
    #[test]
    fn the_zone_search_without_hits_offers_no_set_and_l1_clears() {
        fresh("zone-search-none");
        let (mut s, _hw) = setup("zone-search-none", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
        press(&mut s, &[Button::North, Button::East]);
        assert_eq!(s.at(), "zone: search");
        s.type_search("qqq");
        press(&mut s, &[Button::L1]);
        assert!(matches!(&s.step, Step::Zone(z) if matches!(&z.view, ZoneView::Search(kb) if kb.text.is_empty())), "L1 empties the field");
        s.type_search("qqq");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone: Search: qqq");
        assert!(matches!(&s.step, Step::Zone(z) if matches!(&z.view, ZoneView::List { zones, .. } if zones.is_empty())));
        assert_eq!(zone_list_hints(false, true, true).iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["START", "B"], "no A Set without a hit");
        assert_eq!(zone_list_hints(false, true, false)[0].0, "A");
        paint_it(&mut s);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: Search: qqq", "A does nothing, as the bar says");
    }

    /// Wi-Fi off: X ("Scan again") did nothing, and the bar offered it.
    #[test]
    fn wifi_off_offers_no_scan() {
        assert_eq!(wifi_hints("Turn on", false, false, true, false, false).iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["A", "B", "START"]);
        assert_eq!(wifi_hints("Connect", false, false, false, false, false).iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["A", "B"]);
    }

    /// The welcome: Down reaches the Big text row and Left/Right switch it there; the language stays (first-start walk
    /// 2026-11-06: the row looked like one to choose, Down did nothing and Right changed the language).
    #[test]
    fn down_on_the_welcome_reaches_easy_to_read() {
        fresh("welcome-rows");
        let (mut s, _hw) = setup("welcome-rows", "UTC", None);
        let lang = crate::i18n::current();
        let before = s.easy_to_read();
        press(&mut s, &[Button::Down, Button::Right]);
        assert_ne!(s.easy_to_read(), before, "switched on the row below");
        assert_eq!(crate::i18n::current(), lang, "the language stays");
        assert!(s.language_chosen.is_none());
        press(&mut s, &[Button::Left]);
        assert_eq!(s.easy_to_read(), before);
        press(&mut s, &[Button::Up]);
        assert_eq!(s.welcome_row, 0, "Up: the language row again");
        assert_eq!(s.at(), "welcome");
    }

    /// Your games: Continue with games, Add more… opens the grid and B there comes back; without games the ways and
    /// "I'll add games later".
    #[test]
    fn your_games_says_what_was_found_and_goes_on() {
        fresh("found");
        let (mut s, _hw) = setup("found", "UTC", Some(IW_AT));
        s.games = vec![("snes".into(), PathBuf::from("/storage/roms/snes/a.sfc"))];
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.found_rows(), [FoundRow::Continue, FoundRow::More, FoundRow::BoxArt]);
        press(&mut s, &[Button::Down, Button::East]);
        assert_eq!(s.at(), "games", "Add more… opens the systems and the ways");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "your games", "B comes back");
        assert!(matches!(s.step, Step::Found { sel: 1 }), "on Add more…, the row it left from");
        press(&mut s, &[Button::Up, Button::East]);
        assert_eq!(s.at(), "look", "Continue");
        let (mut s, _hw) = setup("found-none", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.found_rows(), [FoundRow::More, FoundRow::Later, FoundRow::BoxArt]);
        press(&mut s, &[Button::Down, Button::East]);
        assert_eq!(s.at(), "look", "I'll add games later");
    }

    /// Box art: one switch on your games, on as it ships; nothing is downloaded before it was seen (Off comes first).
    #[test]
    fn box_art_is_one_switch_and_nothing_downloads_before_it_is_seen() {
        fresh("art-first");
        let (mut s, _hw) = setup("art-first", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        assert!(!s.past_wifi(), "Wi-Fi");
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games");
        assert!(!s.past_wifi(), "the switch is on this page: the downloads still wait");
        assert_ne!(s.art, Art::Off, "on as it ships");
        press(&mut s, &[Button::Down, Button::Down, Button::East]);
        assert_eq!(s.take_art(), Some(Art::Off));
        press(&mut s, &[Button::Right]);
        assert_eq!(s.take_art(), Some(Art::Pictures), "on again");
        found_on(&mut s);
        assert!(s.past_wifi(), "left: the downloads may start, as chosen");
        assert!(shelf_setup("art-settings").only(Only::Extras).past_wifi(), "Settings: at once");
    }

    /// "For a child…" on the welcome is only the wish: Kids Mode's own steps come after your games, in the look's place;
    /// B there goes on to the look; without games no locked, empty shelf.
    #[test]
    fn kids_mode_comes_after_the_games_and_never_without_them() {
        fresh("kids");
        let (mut s, _hw) = setup("kids", "UTC", Some(IW_AT));
        s.games = vec![("snes".into(), PathBuf::from("/storage/roms/snes/a.sfc"))];
        press(&mut s, &[Button::West]);
        assert!(s.kids_wanted && !s.take_kids_asked(), "only the wish, no PIN before the games");
        assert_eq!(s.at(), "welcome");
        press(&mut s, &[Button::East]);
        wifi_continue(&mut s);
        press(&mut s, &[Button::East]);
        assert!(s.take_kids_asked(), "after your games: Kids Mode's steps");
        s.kids_cancelled();
        assert_eq!(s.at(), "look", "cancelled: the look, as without the wish");
        // Done: the setup ends with them.
        let (mut s, _hw) = setup("kids-done", "UTC", Some(IW_AT));
        s.games = vec![("snes".into(), PathBuf::from("/storage/roms/snes/a.sfc"))];
        press(&mut s, &[Button::West, Button::East]);
        wifi_continue(&mut s);
        press(&mut s, &[Button::East]);
        assert!(s.take_kids_asked());
        s.kids_done();
        assert_eq!(s.at(), "done");
        assert!(!s.data_dir.join(RESUME_FILE).exists(), "finished: nothing to resume");
        // No games: the look comes, with where to set the mode up later.
        let (mut s, _hw) = setup("kids-none", "UTC", Some(IW_AT));
        press(&mut s, &[Button::West, Button::East]);
        wifi_continue(&mut s);
        press(&mut s, &[Button::Down, Button::East]);
        assert!(!s.take_kids_asked(), "never an empty locked shelf");
        assert_eq!(s.at(), "look");
        assert!(s.kids_later);
    }

    /// A restart resumes at the stage it reached, the wish for a child included; the names of the first start before
    /// 0.2 open the stage that comes next (look and play: Wi-Fi; extras: your games).
    #[test]
    fn the_first_start_resumes_where_a_restart_found_it() {
        let name = "resume";
        let dir = fresh(name);
        let (mut s, hw) = setup(name, "UTC", None);
        assert!(!dir.join(RESUME_FILE).exists(), "the welcome has nothing to lose");
        press(&mut s, &[Button::West]);
        at_wifi(&mut s);
        assert_eq!(std::fs::read_to_string(dir.join(RESUME_FILE)).unwrap(), "wifi child");
        let s2 = setup_on(hw.clone(), name).resume();
        assert_eq!((s2.at(), s2.kids_wanted), ("wifi".to_string(), true));
        wifi_continue(&mut s);
        assert_eq!(setup_on(hw.clone(), name).resume().at(), "zone", "unsure zone: asked again");
        press(&mut s, &[Button::East]);
        assert_eq!(setup_on(hw.clone(), name).resume().at(), "your games");
        for (old, opens) in [("look", "wifi"), ("play", "wifi"), ("extras", "your games"), ("games", "your games"), ("shelf-look", "look"), ("nonsense", "welcome")] {
            std::fs::write(dir.join(RESUME_FILE), old).unwrap();
            assert_eq!(setup_on(hw.clone(), name).resume().at(), opens, "saved at {old}");
        }
        // Without a Wi-Fi chip Wi-Fi's place is the time zone.
        std::fs::write(dir.join(RESUME_FILE), "wifi").unwrap();
        let mut s3 = setup_on(hw.clone(), name);
        s3.no_wifi = true;
        assert_eq!(s3.resume().at(), "zone");
        // Finishing deletes it.
        let mut s4 = setup_on(hw.clone(), name).resume();
        assert_eq!(s4.at(), "wifi");
        assert!(s4.press(Button::Start));
        s4.remember_step(true);
        assert!(!dir.join(RESUME_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Big text on every stage with L2: larger text and calmer menus at once, kept by the app; and back.
    #[test]
    fn easy_to_read_is_one_key_on_every_stage() {
        use crate::type_scale::TextSize;
        fresh("reading");
        let (mut s, _hw) = setup("reading", "UTC", Some(IW_AT));
        crate::type_scale::set(TextSize::Normal);
        at_wifi(&mut s);
        press(&mut s, &[Button::L2]);
        assert_eq!((crate::type_scale::get(), s.motion()), (TextSize::Large, Motion::Reduced));
        assert_eq!(s.take_text_size(), Some(TextSize::Large), "the app keeps it");
        assert_eq!(s.at(), "wifi", "the page stays");
        press(&mut s, &[Button::South, Button::L2]);
        assert_eq!((crate::type_scale::get(), s.motion()), (TextSize::Normal, Motion::Full));
        crate::type_scale::set(TextSize::Normal);
    }

    /// The look stage: Style, Colours and Appearance are rows under the layouts, Down reaches each, Left/Right change it;
    /// B goes back to your games.
    #[test]
    fn the_look_stage_has_style_and_b_goes_back_to_your_games() {
        fresh("look-end");
        let (s, _hw) = setup("look-end", "UTC", Some(IW_AT));
        let mut s = s.at_stage("look");
        press(&mut s, &[Button::Right]);
        assert_eq!(s.theme, 1);
        press(&mut s, &[Button::Down, Button::Left]);
        assert_eq!((s.take_font_step(), s.take_colours_step()), (Some(false), None), "Style: the app steps it");
        press(&mut s, &[Button::Down, Button::Right]);
        assert_eq!(s.take_colours_step(), Some(true), "Colours: the app steps them");
        press(&mut s, &[Button::Down, Button::Right, Button::Down]);
        assert_eq!(s.appearance(), Appearance::Light, "the last row, Appearance; Down stays on it");
        assert_eq!((s.at(), s.theme), ("look".to_string(), 1), "the rows leave the layout as it was");
        press(&mut s, &[Button::Up, Button::Up, Button::Up]);
        assert_eq!(s.look_row, 0, "Up: back to the layouts");
        paint_it(&mut s);
        // B goes back a stage, never ends the setup (first-start walk 2026-11-06: B, B from Kids Mode's first page was
        // the shelf); back on the look, A finishes with the look picked.
        assert!(!s.press(Button::South), "B on the last stage doesn't end the setup");
        found_on(&mut s);
        assert!(s.press(Button::East), "A finishes");
        assert_eq!(s.theme, 1, "with the look picked");
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
    fn a_on_the_connected_network_opens_its_menu() {
        // A saved network's menu (2026-11-07): the connected one in Settings offers Forget; the status line still says
        // what is connected.
        let mut s = wifi_setup("wifi-confirm");
        let first = s.entries.iter().position(|e| e.active).expect("a connected network in the demo");
        s.sel = s.lead() + first;
        press(&mut s, &[Button::East]);
        assert!(s.net_menu.is_some(), "its menu");
        assert_eq!(s.net_menu_rows(&s.entries[first].ssid.clone()), [NetAction::Forget]);
        let (line, color) = s.wifi_status("");
        assert!(line.starts_with("Connected to "), "{line}");
        assert_eq!(color, pal().chalk);
        press(&mut s, &[Button::South]);
        assert!(s.net_menu.is_none() && s.at() == "wifi", "B closes the menu, the page stays");
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
        let lines = Text::builtin().wrap_prose(Face::Body, 24, PICTURES_NOTE, 1240 - 2 * MARGIN);
        assert!((1..=4).contains(&lines.len()), "{} lines", lines.len());
        assert!(PICTURES_NOTE.contains("free collection") && PICTURES_NOTE.contains("no account") && PICTURES_NOTE.contains("Off"));
    }


    /// Settings › Help & tools › How you like to play: a profile sets its switches once and the page ends (the first
    /// start no longer asks; its switches stay as they ship, `the_first_start_changes_no_switch`).
    #[test]
    fn a_profile_sets_the_switches_once_and_ends_the_page() {
        use crate::features::Switches;
        for (downs, lens, pixel_path) in [(0, false, false), (1, false, false), (2, true, true)] {
            let (s, _hw) = setup(&format!("profile{downs}"), "UTC", Some(IW_AT));
            let mut s = s.with_switches(Switches::default(), Vec::new()).only(Only::Play);
            for _ in 0..downs {
                press(&mut s, &[Button::Down]);
            }
            assert!(s.press(Button::East), "A on a profile ends the page");
            let (sw, later) = s.take_tour().expect("the profile's switches");
            assert_eq!((sw.curator.lens, sw.curator.pixel_path, later.len()), (lens, pixel_path, 0));
            assert_eq!(s.motion(), sw.motion, "the motion follows");
            assert!(s.take_tour().is_none(), "once");
        }
    }

    /// The first start asks no play style: walked through, it changes no switch and puts off no card.
    #[test]
    fn the_first_start_changes_no_switch() {
        use crate::features::Switches;
        fresh("no-switch");
        let (s, _hw) = setup("no-switch", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new());
        press(&mut s, &[Button::East]);
        wifi_continue(&mut s);
        found_on(&mut s);
        assert_eq!(s.at(), "look");
        assert!(s.take_tour().is_none());
    }

    #[test]
    fn the_second_row_is_just_play_and_only_it_changes_the_look() {
        use crate::features::Switches;
        assert_eq!(Profile::offered_in_settings()[..2], [Profile::Simple, Profile::JustPlay]);
        let (s, _hw) = setup("just-play-row", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new()).only(Only::Play);
        press(&mut s, &[Button::Down, Button::East]);
        assert_eq!(s.take_profile_look(), Some(1), "Easy's place among the looks");
        assert_eq!(crate::settings::Theme::ALL[1], crate::settings::Theme::Big);
        assert_eq!(s.take_tour().expect("taken").0, Switches::default(), "the start settings otherwise");
        // The first row alone changes no look.
        let (s, _hw) = setup("shelf-row", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new()).only(Only::Play);
        press(&mut s, &[Button::East]);
        assert_eq!(s.take_profile_look(), None);
    }

    #[test]
    fn simple_leaves_a_new_device_exactly_as_it_starts() {
        use crate::features::Switches;
        let (s, _hw) = setup("simple-is-skip", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new()).only(Only::Play);
        // Simple, the first row: A.
        press(&mut s, &[Button::East]);
        let (sw, later) = s.take_tour().expect("a profile was taken");
        assert_eq!(sw, Switches::default(), "Simple sets nothing that differs from the start settings");
        assert!(later.is_empty());
        assert!(!sw.curator.lens && !sw.curator.echoes && !sw.curator.merge_versions, "Lens, Echoes and Versions are Enthusiast's");
    }

    /// Settings › Help & tools › How you like to play: the first start's question, answered later in one page.
    #[test]
    fn the_play_style_can_be_changed_later_in_one_page_without_the_first_start() {
        use crate::features::{Profile, Switches};
        let open = |name: &str, sw: Switches| {
            let (s, _hw) = setup(name, "UTC", Some(IW_AT));
            s.with_switches(sw, Vec::new()).only(Only::Play)
        };
        // A new device stands at Simple; "For a child" isn't offered (Settings › System › Kids mode is its place).
        let mut s = open("play-later", Switches::default());
        assert_eq!(s.at(), "how you like to play");
        assert_eq!(s.profiles(), [Profile::Simple, Profile::JustPlay, Profile::Enthusiast, Profile::Choose]);
        assert!(matches!(s.step, Step::Play { sel: 0 }));
        // Down, Down, A: Enthusiast is taken and the page closes (true: the setup ends), no Wi-Fi or zone page follows.
        press(&mut s, &[Button::Down, Button::Down]);
        assert!(s.press(Button::East), "A on a profile ends the page");
        let (sw, _) = s.take_tour().expect("the profile's switches");
        assert_eq!(Profile::of(&sw), Profile::Enthusiast);
        // Reopened, it stands on what is set now; B leaves it alone.
        let mut s = open("play-later-2", sw.clone());
        assert!(matches!(s.step, Step::Play { sel: 2 }));
        assert!(s.press(Button::South));
        assert!(s.take_tour().is_none());
        // One switch of its own: "Choose for yourself" is where it stands, and A there opens the cards, B comes back.
        let mut own = Switches::default();
        own.curator.lens = true;
        assert_eq!(Profile::of(&own), Profile::Choose);
        let mut s = open("play-later-3", own);
        assert!(matches!(s.step, Step::Play { sel: 3 }));
        assert!(!s.press(Button::East));
        assert_eq!(s.at(), "feature cards");
        // START skips the rest of the cards and ends the page, nothing further.
        assert!(s.press(Button::Start));
        // Painted at the Mini V2's size and at the H700's: four rows, none cut off the page.
        for (w, h) in [(1240, 1080), (640, 480)] {
            let mut px = vec![0u32; (w * h) as usize];
            let mut s = open("play-later-paint", Switches::default());
            s.paint(&mut Canvas::new(&mut px, w, h), &mut Text::builtin());
            assert!(px.iter().any(|p| *p != px[0]), "{w}x{h} is not blank");
        }
    }

    #[test]
    fn choose_yourself_opens_the_cards_and_b_on_the_first_goes_back_to_it() {
        use crate::features::Switches;
        let (s, _hw) = setup("choose", "UTC", Some(IW_AT));
        let mut s = s.with_switches(Switches::default(), Vec::new()).only(Only::Play);
        press(&mut s, &[Button::Down, Button::Down, Button::Down, Button::East]);
        assert_eq!(s.at(), "feature cards");
        // The first card (Continue where I quit, on): Off, A. A player's first card is one they can judge.
        press(&mut s, &[Button::Down, Button::East]);
        let (sw, _) = s.take_tour().expect("a switch changed");
        assert!(!sw.remember_quit);
        // The second card (off at first, so the cursor starts on Off): Later.
        press(&mut s, &[Button::Down, Button::East]);
        assert_eq!(s.take_tour().map(|(_, l)| l), Some(vec!["last_game".to_string()]));
        // Back to the first card, then out of the cards to the profiles, Choose yourself selected.
        press(&mut s, &[Button::South, Button::South]);
        assert_eq!(s.at(), "feature cards");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "how you like to play");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "feature cards", "Choose yourself again");
        assert!(s.press(Button::Start), "START skips the rest of the cards and ends the page");
    }

    #[test]
    fn settings_look_still_ends_on_a_without_the_profiles() {
        let mut s = shelf_setup("look-only").only(Only::Look);
        assert!(s.press(Button::East), "Keep this look closes the page");
    }

    /// The Wi-Fi page's Time zone row, selected.
    fn on_zone_row(s: &mut Setup) {
        select_wifi(s, WifiRow::Zone);
    }

    #[test]
    fn a_zone_the_system_already_has_is_left_alone() {
        let (mut s, hw) = setup("system", "America/New_York", Some(IW_AT));
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games", "nothing to ask");
        assert_eq!(hw.state().zone, "America/New_York", "the Wi-Fi country does not override a set zone");
    }

    #[test]
    fn a_country_with_one_zone_is_set_without_asking() {
        let (mut s, hw) = setup("confirm", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        assert_eq!(hw.state().zone, "Europe/Vienna", "set while the Wi-Fi page is up, said on its row");
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games");
        assert!(!s.zone_utc, "a zone was set: no UTC fallback");
    }

    #[test]
    fn without_a_guess_utc_stays() {
        let (mut s, hw) = setup("none", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone", "unsure: asked");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "your games");
        assert_eq!(hw.state().zone, "UTC");
        assert!(s.zone_utc, "kept UTC: unconfirmed");
    }

    #[test]
    fn a_country_with_several_zones_opens_its_list() {
        let (mut s, hw) = setup("several", "UTC", Some(IW_US));
        at_wifi(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone: United States", "the picker, filtered to the country");
        // B goes back to Wi-Fi, and Continue then goes on without asking again: nothing was guessed, nothing is set.
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi");
        wifi_continue(&mut s);
        assert_eq!(s.at(), "your games");
        assert_eq!(hw.state().zone, "UTC");
        // Choosing: Chicago is the first by city.
        let (mut s, hw) = setup("several-pick", "UTC", Some(IW_US));
        at_wifi(&mut s);
        wifi_continue(&mut s);
        press(&mut s, &[Button::Down, Button::Down, Button::East]);
        assert_eq!(s.at(), "your games");
        assert_eq!(hw.state().zone, "America/New_York");
    }

    #[test]
    fn the_time_zone_row_changes_the_guess_by_region() {
        let (mut s, hw) = setup("change", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        on_zone_row(&mut s);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: regions");
        // Opens on the guess's region (Europe), and in it on the guessed city (Vienna, after Berlin).
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: Europe");
        press(&mut s, &[Button::Up, Button::East]);
        assert_eq!(s.at(), "wifi", "back on the Wi-Fi page");
        assert_eq!(hw.state().zone, "Europe/Berlin");
        assert!(s.zone_value().starts_with("Berlin"), "{}", s.zone_value());
    }

    #[test]
    fn b_steps_back_out_of_a_list_and_out_of_the_regions() {
        let (mut s, hw) = setup("change-back", "UTC", Some(IW_AT));
        at_wifi(&mut s);
        on_zone_row(&mut s);
        press(&mut s, &[Button::East, Button::Up, Button::East]);
        assert_eq!(s.at(), "zone: America");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone: regions");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "zone");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi");
        assert_eq!(hw.state().zone, "Europe/Vienna", "the guess is still the guess");
    }

    #[test]
    fn search_finds_a_city() {
        let (mut s, hw) = setup("search", "UTC", None);
        at_wifi(&mut s);
        on_zone_row(&mut s);
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: regions");
        press(&mut s, &[Button::Up; 10]); // "Search…" is the first row
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "zone: search");
        s.type_search("vie");
        press(&mut s, &[Button::Start]);
        assert_eq!(s.at(), "zone: Search: vie", "START in the keyboard searches, it doesn't end the setup");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi");
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
        assert_eq!(s.wifi_rows(), 8);
        s.on_event(check(net_check::Net::Portal));
        assert_eq!(s.wifi_rows(), 9, "the sign-in row before the networks");
        assert_eq!(s.sel, 0, "selected: A opens it");
        // The networks keep their rows below it.
        s.sel = 1 + wifi_row(&s, "Cafe guest");
        press(&mut s, &[Button::Up]);
        assert_eq!(s.sel, 1, "the connected network");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi", "A on the connected network stays on the page, with its menu");
        let (line, _) = s.wifi_status("");
        assert!(line.starts_with("Connected to "), "{line}");
        press(&mut s, &[Button::South, Button::Up, Button::East]);
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
        assert_eq!(s.wifi_rows(), 8);
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
        let e = rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(10))).expect("the thread answers");
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
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_the_page_checks_again_by_itself_turns_online_and_stops() {
        use std::sync::atomic::Ordering;
        // One test drives the counter: the second part needs the first's numbers.
        ASKED.store(0, Ordering::SeqCst);
        ONLINE_AFTER.store(3, Ordering::SeqCst);
        let (mut s, rx) = watched("wifi-recheck");
        s.on_event(check(net_check::Net::Portal));
        assert!(s.watch.is_some(), "the page shows a sign-in: it asks again");
        let wait = crate::testutil::timeout(std::time::Duration::from_secs(5));
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
        assert_eq!(s.wifi_rows(), 8, "then Other network, Wi-Fi country and Scan again");
        s.sel = wifi_row(&s, "Office");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "wifi", "A on a saved network opens its menu");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "connecting", "Connect, its first row: a saved network asks for nothing");
        // A password that no longer works brings the keyboard.
        s.on_event(SetupEvent::ConnectedSaved { ssid: "Office".into(), result: Err("Secrets were required, but not provided".into()) });
        assert_eq!(s.at(), "password");
        // A network that was never saved and has a password asks for it.
        let mut s = wifi_setup("wifi-new");
        s.sel = wifi_row(&s, "Neighbour:5G");
        press(&mut s, &[Button::East]);
        assert_eq!(s.at(), "password");
    }

    /// A saved network's menu: Connect and Forget, reached with A and Down; forgetting is never Y alone (2026-11-07). B
    /// closes the menu and changes nothing; the connected one offers Forget only (Continue first in the first start).
    #[test]
    fn a_saved_network_has_a_menu_with_connect_and_forget() {
        let mut s = wifi_setup("wifi-menu");
        s.sel = wifi_row(&s, "Office");
        press(&mut s, &[Button::East]);
        assert_eq!(s.net_menu, Some(("Office".to_string(), 0)));
        paint_it(&mut s);
        press(&mut s, &[Button::South]);
        assert_eq!((s.net_menu.as_ref(), s.at()), (None, "wifi".to_string()), "B closes it, nothing else");
        assert!(s.entries.iter().any(|e| e.ssid == "Office"), "kept");
        press(&mut s, &[Button::East, Button::Down, Button::East]);
        assert!(s.net_menu.is_none());
        assert_eq!(s.note.as_ref().map(|(m, _)| m.as_str()), Some("Forgetting Office…"), "Forget, its second row");
        // The connected one: Forget only in Settings; Continue, then Forget, in the first start.
        assert_eq!(s.net_menu_rows("Home"), [NetAction::Forget]);
        s.only = None;
        assert_eq!(s.net_menu_rows("Home"), [NetAction::Continue, NetAction::Forget]);
    }

    #[test]
    fn rc03_first_start_b_closes_saved_network_menu_before_leaving_the_stage() {
        let mut s = wifi_setup("rc03-first-wifi-menu");
        s.only = None;
        s.sel = wifi_row(&s, "Office");
        let selected = s.sel;
        for _ in 0..2 {
            press(&mut s, &[Button::East]);
            assert_eq!(s.net_menu, Some(("Office".to_string(), 0)), "one A opens the menu");
            press(&mut s, &[Button::South]);
            assert_eq!(s.at(), "wifi", "B closes only the menu, staying on the Wi-Fi list");
            assert!(s.net_menu.is_none());
            assert_eq!(s.sel, selected, "the network stays selected");
        }
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "welcome", "the next B leaves the stage");
        press(&mut s, &[Button::East]);
        assert!(s.net_menu.is_none(), "returning to Wi-Fi must not resurrect a menu");
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

    /// New-player walk 2026-11-02: one wrong letter of a long password cost 67 presses, all of them again.
    #[test]
    fn a_refused_password_stays_in_the_field_to_be_corrected() {
        let mut s = wifi_setup("wifi-refused-kept");
        s.sel = wifi_row(&s, "Neighbour:5G");
        press(&mut s, &[Button::East]);
        if let Step::Password { kb, .. } = &mut s.step {
            kb.text = "sommer-2025".into();
        }
        press(&mut s, &[Button::Start]);
        s.on_event(SetupEvent::Connected {
            ssid: "Neighbour:5G".into(),
            result: Err("Error: Connection activation failed: Secrets were required, but not provided.".into()),
        });
        assert!(matches!(&s.step, Step::Password { kb, .. } if kb.text == "sommer-2025"), "{:?}", s.at());
        assert_eq!(s.note, Some(("Wrong password. Try again.".into(), true)));
        press(&mut s, &[Button::South]);
        assert!(matches!(&s.step, Step::Password { kb, .. } if kb.text == "sommer-202"), "B takes the last letter off");
        // Leaving for the list: L1 empties the field, B leaves (two presses, whatever was typed).
        press(&mut s, &[Button::L1]);
        assert!(matches!(&s.step, Step::Password { kb, .. } if kb.text.is_empty()), "L1 empties the field");
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "wifi", "B on an empty field goes back to the list");
        // The hidden network's name has the same keys.
        let mut s = wifi_setup("wifi-hidden-clear");
        s.step = Step::HiddenName { kb: Keyboard { text: "Cafe".into(), ..Keyboard::default() } };
        press(&mut s, &[Button::L1, Button::South]);
        assert_eq!(s.at(), "wifi");
        // The hint bar offers it while there is something to clear, and says Back on an empty field; painted at both sizes.
        for (w, h) in [(1240, 1080), (640, 480)] {
            let mut s = wifi_setup("wifi-clear-paint");
            s.step = Step::Password { ssid: "Neighbour:5G".into(), kb: Keyboard { text: "sommer-2025".into(), ..Keyboard::default() }, hidden: false };
            let mut px = vec![0u32; (w * h) as usize];
            s.paint(&mut Canvas::new(&mut px, w, h), &mut Text::builtin());
            assert!(px.iter().any(|p| *p != px[0]), "{w}x{h} is not blank");
            s.step = Step::HiddenName { kb: Keyboard { text: "Cafe".into(), ..Keyboard::default() } };
            s.paint(&mut Canvas::new(&mut px, w, h), &mut Text::builtin());
            assert!(px.iter().any(|p| *p != px[0]), "{w}x{h} is not blank");
        }
    }

    /// First-start walk 2026-11-04: the Odin's nine Steam entries were "Your 9 games are on the shelf" on a card with none.
    #[test]
    fn entries_the_device_brings_are_not_your_games() {
        let steam = |n: &str| ("Steam".to_string(), PathBuf::from(format!("/storage/roms/steam/{n}.steam")));
        let mut games: Vec<(String, PathBuf)> = (0..9).map(|i| steam(&format!("g{i}"))).collect();
        games.push(("Ports".into(), PathBuf::from("/storage/roms/ports/PortMaster.sh")));
        let (mut s, _) = setup("games-steam", "UTC", None);
        s.games = games;
        assert_eq!(s.games_known(), 0);
        assert_eq!(done_line(s.games_known()), "Games show up on the shelf once they're copied.");
        s.games.push(("Game Boy".into(), PathBuf::from("/storage/roms/gb/Tetris.gb")));
        assert_eq!(done_line(s.games_known()), "Your game is on the shelf.");
        assert!(library::supplied_by_device("steam", std::path::Path::new("/x/y.steam")) && !library::supplied_by_device("ports", std::path::Path::new("/x/Celeste.sh")));
    }

    /// New-player walk 2026-11-02: what a card that was never read has isn't in the list the shelf gave at the start.
    #[test]
    fn the_games_found_by_counting_are_the_ones_the_pages_speak_of() {
        let (mut s, _) = setup("games-known", "UTC", None);
        assert_eq!(s.games_known(), 0);
        let mut found = Counted::default();
        found.games.insert("gba".into(), 4);
        found.games.insert("snes".into(), 2);
        s.on_event(SetupEvent::GameCounts(found));
        assert_eq!(s.games_known(), 6);
        assert_eq!(done_line(s.games_known()), "Your 6 games are on the shelf.");
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
        assert!(matches!(&s.step, Step::Password { ssid, kb, hidden: false } if ssid == "Neighbour:5G" && kb.text.is_empty()), "nothing was typed in this test");
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

    /// A first-start setup on the Wi-Fi step, with the connect of `ssid` under way as the page's own press would have
    /// left it, and a way to hear the wait's `Advance`.
    fn connecting_in_first_start(name: &str) -> (Setup, std::sync::mpsc::Receiver<SetupEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let mut s = setup(name, "UTC", Some(IW_AT)).0.with_wifi_demo();
        s.no_wifi = false;
        s.notify = Arc::new(move |e| {
            if matches!(e, SetupEvent::Advance { .. }) {
                let _ = tx.lock().unwrap().send(e);
            }
        });
        s.step = Step::Connecting { ssid: "Home".into() };
        s.advance_armed = true;
        (s, rx)
    }

    fn heard_advance(rx: &std::sync::mpsc::Receiver<SetupEvent>) -> SetupEvent {
        rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(10))).expect("the wait ends with an Advance")
    }

    #[test]
    fn first_start_goes_on_to_the_time_zone_by_itself_after_a_connection() {
        let (mut s, rx) = connecting_in_first_start("wifi-advance-ok");
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        assert_eq!(s.at(), "wifi", "the page shows the result first");
        assert_eq!(s.wifi_status("").0, "Connected to Home");
        s.on_event(heard_advance(&rx));
        assert_eq!(s.at(), "zone", "on without a press");
        assert!(s.advance_pending.is_none());
        // A saved network, connected without its password, the same way.
        let (mut s, rx) = connecting_in_first_start("wifi-advance-saved");
        s.on_event(SetupEvent::ConnectedSaved { ssid: "Home".into(), result: Ok(()) });
        s.on_event(heard_advance(&rx));
        assert_eq!(s.at(), "zone");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_first_start_stays_on_wifi_after_a_failed_connection() {
        let (mut s, rx) = connecting_in_first_start("wifi-advance-fail");
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Err("Error: Timeout expired (45 seconds)".into()) });
        assert_eq!(s.at(), "wifi");
        assert!(s.note.as_ref().is_some_and(|(_, bad)| *bad), "the message stays");
        assert!(rx.recv_timeout(std::time::Duration::from_millis(1800)).is_err(), "no wait was started");
        // A refused password: the keyboard again, the typed password kept.
        let (mut s, _rx) = connecting_in_first_start("wifi-advance-refused");
        s.typed_password = "sommer-2025".into();
        s.on_event(SetupEvent::Connected {
            ssid: "Home".into(),
            result: Err("Error: Connection activation failed: Secrets were required, but not provided.".into()),
        });
        assert!(matches!(&s.step, Step::Password { kb, .. } if kb.text == "sommer-2025"));
        // A later Connected that was not started here (a stale answer) does not move on either.
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        assert!(s.advance_pending.is_none());
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_back_from_the_time_zone_does_not_bounce_forward_again() {
        let (mut s, rx) = connecting_in_first_start("wifi-advance-back");
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        s.on_event(heard_advance(&rx));
        assert_eq!(s.at(), "zone");
        s.zone_back();
        assert_eq!(s.at(), "wifi");
        // Connected already, and a list that says so: the player stays in control.
        s.on_event(SetupEvent::Net);
        s.on_event(SetupEvent::Advance { gen: s.advance_gen });
        assert_eq!(s.at(), "wifi");
        assert!(rx.recv_timeout(std::time::Duration::from_millis(1800)).is_err(), "nothing waits to move on");
        // And the Continue row is still the way on (START would skip the rest of the setup; B goes back a stage).
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_leaving_or_pressing_during_the_connected_moment_cancels_the_move() {
        // B within the second: back a stage at once, and the late Advance finds nothing more to do.
        let (mut s, rx) = connecting_in_first_start("wifi-advance-b");
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "welcome");
        s.on_event(heard_advance(&rx));
        assert_eq!(s.at(), "welcome", "not pulled forward after going back");
        // Any other press hands the page back too.
        let (mut s, rx) = connecting_in_first_start("wifi-advance-down");
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        press(&mut s, &[Button::Down]);
        s.on_event(heard_advance(&rx));
        assert_eq!(s.at(), "wifi");
        // Leaving while the connect is still going (the Connecting page has no way out, but the setup may be moved
        // by other means): the result arriving on another step is not a connection made here.
        let (mut s, rx) = connecting_in_first_start("wifi-advance-left");
        s.step = Step::Welcome;
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        assert!(rx.recv_timeout(std::time::Duration::from_millis(1800)).is_err());
    }

    #[test]
    fn a_connection_with_a_sign_in_page_does_not_move_on() {
        let (mut s, rx) = connecting_in_first_start("wifi-advance-portal");
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        s.on_event(check(net_check::Net::Portal));
        s.on_event(heard_advance(&rx));
        assert_eq!(s.at(), "wifi", "the sign-in row is how it goes on");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_settings_wifi_page_never_moves_on_by_itself() {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let mut s = wifi_setup("wifi-advance-settings");
        s.notify = Arc::new(move |e| {
            if matches!(e, SetupEvent::Advance { .. }) {
                let _ = tx.lock().unwrap().send(e);
            }
        });
        s.step = Step::Connecting { ssid: "Home".into() };
        s.advance_armed = true;
        s.on_event(SetupEvent::Connected { ssid: "Home".into(), result: Ok(()) });
        assert_eq!(s.at(), "wifi");
        assert_eq!(s.wifi_status("").0, "Connected to Home", "the note as before");
        assert!(rx.recv_timeout(std::time::Duration::from_millis(1800)).is_err(), "no wait in Settings");
        s.on_event(SetupEvent::Advance { gen: 0 });
        assert_eq!(s.at(), "wifi");
    }

    fn wifi_hints_for(s: &Setup) -> Vec<(&'static str, &'static str)> {
        let row = s.sel.checked_sub(s.lead());
        let a = match row.map(|r| r.checked_sub(s.entries.len())) {
            Some(None) => "Connect",
            None if s.lead_kind() == Some(Lead::Off) => "Turn on",
            _ => "Open",
        };
        wifi_hints(a, false, false, false, false, true)
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
        let mut s = shelf_setup("shelf-first").at_stage("look");
        assert_eq!(s.at(), "look");
        press(&mut s, &[Button::North]);
        assert_eq!(s.at(), "look", "X opens no page in the first start: Style is a row of the look, no systems page");

        // Settings → Look has no X key any more: the Systems on the shelf row opens the page straight.
        let mut s = shelf_setup("shelf-settings").only(Only::Look);
        assert!(!s.press(Button::North));
        assert_eq!(s.at(), "look", "no X key on Settings → Look");
        let mut s = shelf_setup("shelf-settings-row").only(Only::Look).at_shelf_systems();
        assert_eq!(s.at(), "shelf systems");
        assert!(!s.press(Button::Down));
        assert!(!s.press(Button::East), "a tick is not the end of the step");
        assert_eq!(s.take_hidden(), Some(vec!["ports".to_string()]));
        assert_eq!(s.take_hidden(), None, "reported once");
        assert!(!s.press(Button::East));
        assert_eq!(s.take_hidden(), Some(Vec::new()), "ticked again");
        assert!(s.press(Button::South), "B leaves the page that Settings opened straight");
    }

    /// Cozy or Quick follows the look until the player chooses (Settings › Sound & feel › Animations); the first start
    /// doesn't ask.
    #[test]
    fn the_look_decides_the_animations_until_they_are_chosen() {
        use crate::settings::Theme;
        let mut s = setup("look-animations", "UTC", None).0.at_stage("look");
        for _ in 0..s.themes.len() {
            let theme = Theme::ALL[s.theme];
            assert_eq!(s.animations_shown(), Animations::default_for(theme), "{theme:?}");
            press(&mut s, &[Button::Right, Button::L1]);
            assert_eq!(s.take_animations(), None, "nothing chosen on the first start's look");
        }
        // Settings > Look starts from what was chosen before.
        let s = setup("look-animations-2", "UTC", None).0.only(Only::Look).with_animations(Some(Animations::Quick));
        assert_eq!(s.animations_shown(), Animations::Quick);
    }

    #[test]
    fn the_look_step_switches_the_preview_with_every_direction() {
        let mut s = setup("look-switch", "UTC", None).0.at_stage("look");
        assert_eq!(s.previewing(), Some(0));
        for (b, want) in [(Button::Right, 1), (Button::Right, 0), (Button::Left, 1)] {
            s.press(b);
            assert_eq!(s.previewing(), Some(want), "{b:?}");
        }
        // Down is the row under the layouts, Style, as in every list.
        press(&mut s, &[Button::Down, Button::Right]);
        assert_eq!(s.previewing(), Some(1), "Down and Left/Right on the row leave the layout alone");
        assert_eq!(s.take_font_step(), Some(true), "and change the style instead");
        assert!(s.press(Button::East), "Finish keeps it and ends the setup, from any row");
        assert_eq!(s.theme, 1);
    }

    #[test]
    fn settings_look_reaches_every_row_it_draws_with_the_d_pad() {
        // The Brick, 2026-11-07: Settings › Look › Layout drew Appearance, Motion and Animations under the layouts, and
        // Up/Down only turned the layouts: the rows could not be reached, and the hint bar had no B.
        let (s, _) = setup("look-rows", "UTC", None);
        let mut s = s.with_appearance(Appearance::Dark).only(Only::Look);
        let (theme, motion, animations) = (s.theme, s.motion(), s.animations_shown());
        press(&mut s, &[Button::Down, Button::Right]);
        assert_eq!((s.appearance(), s.theme), (Appearance::Light, theme), "Down, Right: the next appearance, the same layout");
        press(&mut s, &[Button::Down, Button::Right]);
        assert_ne!(s.motion(), motion, "the second row: Motion");
        press(&mut s, &[Button::Down, Button::Left]);
        assert_ne!(s.animations_shown(), animations, "the third row: Animations");
        assert_eq!(s.take_animations(), Some(s.animations_shown()), "and the app keeps it");
        press(&mut s, &[Button::Down, Button::Up, Button::Up, Button::Up, Button::Up, Button::Right]);
        assert_eq!(s.previewing(), Some((theme + 1) % s.themes.len()), "Up from the rows: the layouts again");
        assert_eq!(s.appearance(), Appearance::Light, "the rows keep what was chosen");
        for row in [0, 1] {
            let bar = look_hints(true, row == 1);
            assert!(bar.iter().any(|(k, _)| *k == "B"), "B goes back and says so: {bar:?}");
            assert!(bar.iter().any(|(k, _)| *k == "Left/Right"), "{bar:?}");
        }
        assert!(s.press(Button::South), "B leaves the page");
        assert_eq!(s.take_profile_look(), Some(theme), "and puts the layout back");
        assert_eq!((s.appearance(), s.motion(), s.animations_shown()), (Appearance::Dark, motion, animations), "and the rows");
        assert_eq!(s.take_animations(), Some(animations), "the app takes the animations back too");
        // A keeps what is shown.
        let (s, _) = setup("look-keep", "UTC", None);
        let mut s = s.only(Only::Look);
        press(&mut s, &[Button::Right]);
        let shown = s.previewing();
        assert!(s.press(Button::East), "A keeps and leaves");
        assert_eq!((s.take_profile_look(), shown), (None, Some((theme + 1) % s.themes.len())), "nothing put back");
    }

    #[test]
    fn settings_look_has_no_shortcut_keys_and_the_first_start_has_its_style_panel() {
        // Polish 2026-11-04: Appearance, Motion, Animations and Systems on the shelf have rows in Settings; the page
        // keeps Left/Right (the look) and A.
        let (s, _) = setup("look-appearance", "UTC", None);
        let mut s = s.with_appearance(Appearance::Light).only(Only::Look);
        let (motion, animations) = (s.motion(), s.animations_shown());
        for b in [Button::West, Button::R1, Button::L1, Button::North] {
            s.press(b);
            assert_eq!((s.appearance(), s.motion(), s.animations_shown(), s.at()), (Appearance::Light, motion, animations, "look".to_string()), "{b:?}");
        }
        assert_eq!(s.take_animations(), None, "L1 chose nothing");
        assert_eq!(s.previewing(), Some(0), "the look stays");
        assert!(s.press(Button::East), "A keeps the look and closes the page");

        // The first start: no shoulder-button pills; the appearance is a row under the layouts (L1 there goes back
        // a stage: `l1_goes_back_a_stage_and_b_still_skips`).
        let mut first = setup("look-motion-first", "UTC", None).0.at_stage("look");
        let before = first.appearance();
        for b in [Button::West, Button::R1] {
            first.press(b);
        }
        assert_eq!((first.appearance(), first.at()), (before, "look".to_string()), "nothing hidden on keys");
        press(&mut first, &[Button::Down, Button::Down, Button::Down, Button::Right]);
        assert_ne!(first.appearance(), before, "Down to Appearance, Right");
    }

    #[test]
    fn the_last_system_on_the_shelf_keeps_its_tick() {
        let mut s = shelf_setup("shelf-last").only(Only::Look).at_shelf_systems();
        press(&mut s, &[Button::East, Button::Down, Button::East, Button::Down, Button::East]);
        assert_eq!(s.take_hidden(), Some(vec!["gba".to_string(), "ports".to_string()]));
        assert!(s.shelf_note().is_some(), "says why Super Nintendo stays");
    }

    #[test]
    fn pc_streaming_has_its_own_tick_after_the_systems_when_moonlight_is_installed() {
        let mut s = shelf_setup("shelf-streaming").with_streaming(true, false).only(Only::Look).at_shelf_systems();
        // gba, ports, snes, then PC streaming.
        press(&mut s, &[Button::Down, Button::Down, Button::Down, Button::East]);
        assert_eq!(s.take_streaming(), Some(true), "ticked on");
        assert_eq!(s.take_streaming(), None, "reported once");
        assert_eq!(s.take_hidden(), None, "the systems are untouched");
        press(&mut s, &[Button::East]);
        assert_eq!(s.take_streaming(), Some(false));
        // Not a system: hiding the other two leaves the last system's tick alone, streaming or not.
        press(&mut s, &[Button::Up, Button::Up, Button::Up, Button::East, Button::Down, Button::East]);
        assert_eq!(s.take_hidden(), Some(vec!["gba".to_string(), "ports".to_string()]));

        let mut s = shelf_setup("shelf-no-moonlight").with_streaming(false, false).only(Only::Look).at_shelf_systems();
        press(&mut s, &[Button::Down, Button::Down, Button::Down, Button::East]);
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
        // Without a Wi-Fi chip the step opens right after the welcome, before its thread has looked.
        let mut s = setup_on(spy.clone(), "spy-zone");
        s.no_wifi = true;
        s.press(Button::East);
        assert_eq!(s.at(), "zone", "open before the thread has answered");
        assert!(s.waiting() && s.animating(), "says it is looking");
        paint_it(&mut s);
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
        press(&mut s, &[Button::South]);
        assert_eq!(s.at(), "welcome", "B goes back a stage");
        // With Wi-Fi the guess is made while the Wi-Fi page is up, on its thread, the same way.
        let mut s = setup_on(spy.clone(), "spy-zone-wifi");
        at_wifi(&mut s);
        paint_it(&mut s);
        wifi_continue(&mut s);
        assert_eq!(s.at(), "zone: United States", "unsure: the list, already read");
        paint_it(&mut s);
        assert_eq!(spy.take(), Vec::<&str>::new(), "looked at the system on the UI thread");
    }

    #[test]
    fn leaving_the_zone_step_while_it_looks_drops_the_late_answer() {
        let (mut s, hw) = setup("leave-zone", "UTC", Some(IW_AT));
        s.no_wifi = true;
        s.press(Button::East);
        assert_eq!(s.at(), "zone");
        s.press(Button::South);
        assert_eq!(s.at(), "welcome", "B goes back even before the answer");
        // The thread's answer arrives after the step is gone.
        let rx = s.inbox.take().unwrap();
        let late = loop {
            let e = rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(5))).expect("the thread answers");
            if matches!(e, SetupEvent::Zone(_)) {
                break e;
            }
        };
        s.on_event(late);
        assert_eq!(s.at(), "welcome", "the answer finds the step gone");
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

    /// Send from your phone: nothing listens on Add games; Y opens the page (its server only there), B closes it and
    /// the grid counts again; without Wi-Fi from Settings, A asks the app for the Wi-Fi settings.
    #[test]
    fn the_phone_page_listens_only_while_it_is_open() {
        let (_dir, catalog) = roms("phone", &[("gba", 1)]);
        let (s, _) = setup("phone", "UTC", None);
        let mut s = s.with_catalog(catalog).only(Only::Games);
        s.settle();
        assert!(!s.phone_listening(), "Add games alone opens no port");
        assert!(games_hints(None, false, true).contains(&("Y", "From your phone")));
        // No Wi-Fi address on the test machine's interfaces: the page opens and says so, nothing listens.
        s.ip = None;
        assert!(!s.press(Button::West));
        assert_eq!(s.at(), "phone");
        assert!(!s.phone_listening());
        paint_it(&mut s);
        assert!(s.press(Button::East), "from Settings, A on \"Connect to Wi-Fi first\" leaves for the Wi-Fi page");
        assert_eq!((s.take_open_wifi(), s.take_open_wifi()), (Some((true, true)), None));
        // The Wi-Fi settings it asked for: leaving them goes back to Add games, on the phone page (the explorer,
        // 2026-11-07: B landed on the Settings list).
        let mut w = setup("phone-wifi", "UTC", None).0.only(Only::Wifi).back_to_games((true, true));
        assert!(w.press(Button::South), "B leaves the Wi-Fi settings");
        assert_eq!((w.take_back_to_games(), w.take_back_to_games()), (Some((true, true)), None));
        let (_dir2, catalog) = roms("phone-back", &[("gba", 1)]);
        let mut back = setup("phone-back", "UTC", None).0.with_catalog(catalog).only(Only::Games).back_on_way((true, true));
        assert_eq!(back.at(), "phone", "back on the phone page");
        press(&mut back, &[Button::South]);
        assert_eq!(back.at(), "games");
        assert!(back.ways_focus && back.ways().get(back.ways_sel) == Some(&Way::Phone), "and B there: on its tile, where the player came from");
        // A on the tile itself (no Wi-Fi): the Wi-Fi settings, and back on the tile without the page (the explorer).
        let (_dir3, catalog) = roms("phone-tile", &[("gba", 1)]);
        let mut t = setup("phone-tile", "UTC", None).0.with_catalog(catalog).only(Only::Games);
        t.ip = None;
        if let Some(i) = t.ways().iter().position(|w| *w == Way::Phone) {
            (t.ways_focus, t.ways_sel) = (true, i);
            assert!(t.press(Button::East), "the Wi-Fi settings, from Settings");
            assert_eq!(t.take_open_wifi(), Some((true, false)), "from the tile, not the page");
            let (_dir4, catalog) = roms("phone-tile-2", &[("gba", 1)]);
            let back = setup("phone-tile-2", "UTC", None).0.with_catalog(catalog).only(Only::Games).back_on_way((true, false));
            assert_eq!(back.at(), "games");
            assert!(back.ways_focus && back.ways().get(back.ways_sel) == Some(&Way::Phone));
        }
        let (s, _) = setup("phone-2", "UTC", None);
        let mut s = s.only(Only::Games);
        s.ip = Some("203.0.113.9".into());
        s.press(Button::West);
        assert!(!s.phone_listening(), "an address that is not this machine's: the page says it couldn't open");
        paint_it(&mut s);
        assert!(!s.press(Button::South), "B goes back to the grid, not out of Settings");
        assert_eq!(s.at(), "games");
        assert!(!s.phone_listening());
    }

    /// Down from the grid's last row reaches the row of ways, Left/Right choose, Up goes back, and B still leaves.
    #[test]
    fn the_row_of_ways_is_reached_with_down_and_left_alone_with_b() {
        let (_dir, catalog) = roms("ways", &[("gba", 1)]);
        let (s, _) = setup("ways", "UTC", None);
        let mut s = s.with_catalog(catalog).only(Only::Games);
        s.settle();
        let ways = s.ways().len();
        if ways == 0 {
            // A test machine with neither Wi-Fi nor USB: Down stays in the grid.
            s.press(Button::Down);
            assert!(!s.ways_focused());
            return;
        }
        for _ in 0..8 {
            s.press(Button::Down);
        }
        assert!(s.ways_focused(), "Down past the last grid row");
        s.press(Button::Right);
        s.press(Button::Down);
        assert!(s.ways_focused(), "nothing below the row");
        paint_it(&mut s);
        s.press(Button::Up);
        assert!(!s.ways_focused());
        s.ways_steps(0);
        assert!(s.ways_focused(), "a tap on a tile selects it");
        assert!(s.press(Button::South), "B leaves Add games from Settings");
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
        assert!(unread_lines(&s.strays, &s.broken).is_empty(), "nothing unread: the usual subtitle");
        paint_it(&mut s);

        std::fs::create_dir_all(dir.join("Old Stuff")).unwrap();
        std::fs::write(dir.join("Old Stuff/Sonic.md"), b"x").unwrap();
        catalog[0].extensions.push("cue".into());
        std::fs::write(dir.join("gba/Lost Bin (Europe).cue"), "FILE \"Lost Bin (Europe).bin\" BINARY\n").unwrap();
        let (mut s, _) = setup("unread-2", "UTC", None);
        s = s.with_catalog(catalog).only(Only::Games);
        s.settle();
        assert_eq!(s.grid.as_ref().unwrap().games("gba"), 1, "the cue without its bin is not counted");
        assert_eq!(
            unread_lines(&s.strays, &s.broken),
            [
                "Folder without a system: Old Stuff. The tiles below show the folder names.",
                "Not on the shelf: Lost Bin. A file is missing: Lost Bin (Europe).bin",
            ]
        );
        paint_it(&mut s);
    }

    /// Large text cuts a long line with "...": what is wrong must come before the long file name, never after it
    /// (daily-use walk: "Its file Crash Bandicoot (USA) (Track 1).bin is..." lost the word "missing").
    #[test]
    fn a_disc_missing_a_file_says_so_before_the_long_file_name() {
        let disc = library::BrokenDisc { sheet: PathBuf::from("/storage/roms/psx/Crash Bandicoot (USA).cue"), missing: "Crash Bandicoot (USA) (Track 1).bin".into() };
        let line = &unread_lines(&[], &[disc])[0];
        assert!(line.find("missing").unwrap() < line.find("Track 1").unwrap(), "{line}");
    }

    #[test]
    fn several_unread_things_are_listed_in_a_line_each_kind() {
        let stray = |name: &str| library::StrayFolder { name: name.into() };
        let disc = |sheet: &str| library::BrokenDisc { sheet: PathBuf::from(format!("/storage/roms/psx/{sheet}")), missing: "x.bin".into() };
        assert_eq!(unread_lines(&[stray("Stuff")], &[]), ["Folder without a system: Stuff. The tiles below show the folder names."]);
        assert_eq!(
            unread_lines(&[stray("Atari Jaguar"), stray("Stuff")], &[disc("A (USA).cue"), disc("B.cue")]),
            ["Folders without a system: Atari Jaguar, Stuff. The tiles below show the folder names.", "Not on the shelf, a disc file is missing: A, B"]
        );
        assert_eq!(
            unread_lines(&[stray("Junk"), stray("Stuff")], &[]),
            ["Folders without a system: Junk, Stuff. The tiles below show the folder names."]
        );
        // New-player walk 2026-11-02: seven names, cut in the seventh, and what to do cut off with them.
        assert_eq!(
            unread_lines(&[stray("GBA ROMS"), stray("Xbox 360"), stray("Stuff")], &[]),
            ["3 folders without a system. Settings › Games › Game check says where their games go."]
        );
    }

    #[test]
    fn a_refused_zone_keeps_the_step_open() {
        // The mock refuses names that are not plain zone names; the table never produces one, so ask the step directly.
        let (mut s, hw) = setup("refused", "UTC", None);
        at_wifi(&mut s);
        wifi_continue(&mut s);
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
    fn leaving_ports_or_dropping_setup_cancels_the_worker_even_before_it_starts() {
        let mut s = ports_setup("pm-cancel-back");
        let cancel = ports::DownloadCancel::new();
        s.port_download_cancel = Some(cancel.clone());
        s.portmaster = Some(PortMasterState::Downloading(Default::default()));
        s.press(Button::South);
        assert_eq!(s.at(), "extras");
        assert!(cancel.is_cancelled(), "B requests a checkpoint before returning to Extras");
        let cancel = ports::DownloadCancel::new();
        s.port_download_cancel = Some(cancel.clone());
        drop(s);
        assert!(cancel.is_cancelled(), "closing Extras also stops its owned worker");
    }

    #[test]
    fn b_cancels_the_owned_portmaster_worker_and_reopening_can_retry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn fetch(_report: &ports::DownloadReporter, cancel: &ports::DownloadCancel) -> Result<(), String> {
            if CALLS.fetch_add(1, Ordering::SeqCst) > 0 { return Ok(()); }
            let limit = std::time::Instant::now() + Duration::from_secs(2);
            while !cancel.is_cancelled() && std::time::Instant::now() < limit { std::thread::sleep(Duration::from_millis(5)); }
            assert!(cancel.is_cancelled(), "B must reach even a worker scheduled after the page was left");
            Err("Download cancelled.".into())
        }
        let mut s = ports_setup("pm-worker-back");
        s.portmaster = Some(PortMasterState::Offer(ports::RuntimeStatus::Missing));
        s.portmaster_fetch = fetch;
        s.ip = Some("192.0.2.1".into());
        s.press(Button::East);
        s.press(Button::South);
        s.settle();
        assert_eq!(s.at(), "extras");
        assert!(matches!(s.portmaster, Some(PortMasterState::Offer(_))));
        assert_eq!(s.portmaster_note.as_deref(), Some("Download cancelled."));
        s.press(Button::East);
        s.press(Button::East);
        s.settle();
        assert!(matches!(s.portmaster, Some(PortMasterState::Ready)));
        assert_eq!(CALLS.load(Ordering::SeqCst), 2, "retry begins only after the first worker finished");
    }

    #[test]
    fn portmaster_is_an_explicit_one_time_download_and_reopening_does_not_download_again() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static INSTALLS: AtomicUsize = AtomicUsize::new(0);
        fn fetch(report: &(dyn Fn(ports::DownloadProgress) + Send + Sync), _cancel: &ports::DownloadCancel) -> Result<(), String> {
            INSTALLS.fetch_add(1, Ordering::SeqCst);
            report(ports::DownloadProgress { stage: "download".into(), done: 20 << 20, total: 300 << 20 });
            Ok(())
        }
        let mut s = ports_setup("pm-on-demand");
        s.portmaster = Some(PortMasterState::Offer(ports::RuntimeStatus::Missing));
        s.portmaster_fetch = fetch;
        s.ip = Some("192.0.2.1".into());
        assert_eq!(s.portmaster_action(), "Get PortMaster (~300 MB, one time)");
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 0, "opening Ports never starts a large download");
        s.press(Button::East);
        assert!(matches!(s.portmaster, Some(PortMasterState::Downloading(_))));
        s.settle();
        assert!(matches!(s.portmaster, Some(PortMasterState::Ready)));
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 1);
        s.press(Button::South);
        s.press(Button::East);
        assert!(matches!(s.step, Step::Ports));
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 1, "installed runtime stays on the card");
    }

    #[test]
    fn portmaster_offer_stays_reachable_offline_and_keeps_download_errors_with_retry() {
        let mut s = ports_setup("pm-offline");
        s.portmaster = Some(PortMasterState::Offer(ports::RuntimeStatus::Missing));
        s.ip = None;
        s.portmaster_demo_online = Some(false);
        let paint = |s: &mut Setup| {
            let mut pixels = vec![0; 1024 * 768];
            s.paint_portmaster_offer(&mut Canvas::new(&mut pixels, 1024, 768), &mut Text::builtin());
            pixels
        };
        let before = paint(&mut s);
        s.press(Button::East);
        assert!(matches!(s.portmaster, Some(PortMasterState::Offer(_))), "offline A cannot start a download");
        assert!(paint(&mut s) == before, "offline A has no advertised action and leaves the picture unchanged");
        assert!(s.portmaster_note.is_none());
        s.on_event(SetupEvent::PortMasterDone(Err("Not enough card space.".into())));
        assert!(s.portmaster_note.as_deref().is_some_and(|n| n.contains("card space")));
        assert_eq!(s.portmaster_action(), "Get PortMaster (~300 MB, one time)");
        s.press(Button::South);
        assert_eq!(s.at(), "extras", "B always leaves the offer");
    }

    #[test]
    fn portmaster_remove_needs_confirm_works_offline_and_returns_to_the_get_offer() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static REMOVALS: AtomicUsize = AtomicUsize::new(0);
        fn remove(_: &ports::DownloadReporter, _: &ports::DownloadCancel) -> Result<(), String> {
            REMOVALS.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        let mut s = ports_setup("pm-remove");
        s.step = Step::Extras { sel: 2 };
        s.portmaster = Some(PortMasterState::Ready);
        s.portmaster_status = ports::RuntimeStatus::Ready;
        s.portmaster_remove = remove;
        s.ip = None;
        s.remove_portmaster();
        assert_eq!(REMOVALS.load(Ordering::SeqCst), 0, "browsing never removes the runtime");
        s.portmaster_confirm_remove = true;
        assert_eq!(s.portmaster_row(false).value, "Remove?");
        s.press(Button::South);
        assert!(!s.portmaster_confirm_remove, "B puts the offer away without removal");
        s.portmaster_confirm_remove = true;
        s.press(Button::East);
        assert!(s.portmaster_removing);
        s.settle();
        assert_eq!(REMOVALS.load(Ordering::SeqCst), 1);
        assert!(matches!(s.portmaster, Some(PortMasterState::Offer(ports::RuntimeStatus::Missing))));
        assert!(!s.portmaster_confirm_remove && !s.portmaster_removing);
        assert_eq!(s.portmaster_row(false).value, "Needs Wi-Fi", "Get can wait until the player is online again");
    }

    #[test]
    fn portmaster_free_space_refuses_a_download_with_the_actual_card_numbers() {
        let mut s = ports_setup("pm-space").with_extras_portmaster_demo("no-space");
        assert_eq!(s.portmaster_row(true).value, "Needs 700 MiB");
        assert!(s.portmaster_row(true).desc.contains("412 MiB free"));
        s.start_portmaster();
        assert!(matches!(s.portmaster, Some(PortMasterState::Offer(_))));
        assert!(s.port_download_cancel.is_none(), "low space never starts the download");
    }

    #[test]
    fn portmaster_progress_and_pin_refresh_are_visible_and_busy_presses_do_not_start_ports() {
        let mut s = ports_setup("pm-progress");
        s.portmaster = Some(PortMasterState::Offer(ports::RuntimeStatus::Outdated));
        assert_eq!(s.portmaster_action(), "Update PortMaster");
        s.on_event(SetupEvent::PortMasterProgress(ports::DownloadProgress { stage: "verify".into(), done: 300 << 20, total: 300 << 20 }));
        assert!(s.portmaster_progress_line().contains("300 MB"));
        assert_eq!(s.portmaster_action(), "Checking PortMaster…", "a completed transfer says it is being checked");
        for done in [1, 32, 128] {
            s.on_event(SetupEvent::PortMasterProgress(ports::DownloadProgress { stage: "unpacking".into(), done: done << 20, total: 277 << 20 }));
            assert_eq!(s.portmaster_action(), "Preparing PortMaster…");
            assert_eq!(s.portmaster_progress_line(), format!("{done} / 277 MB"));
        }
        s.on_event(SetupEvent::PortMasterProgress(ports::DownloadProgress { stage: "downloading".into(), done: 16 << 20, total: 0 }));
        assert_eq!(s.portmaster_progress_line(), "16 MB", "unknown totals keep real received bytes without a made-up percentage");
        s.press(Button::East);
        assert!(s.port_busy.is_none(), "the runtime's A never installs a game behind it");
        s.press(Button::South);
        assert_eq!(s.at(), "extras", "B leaves after requesting a resumable checkpoint");
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
    fn wifi_page_goes_back_with_b_everywhere_and_on_with_its_continue_row() {
        // One scheme (2026-11-07): B is Back in the first start too; the way on is the Continue row (A on it).
        assert_eq!(wifi_hints("Connect", false, false, true, false, true), [("A", "Connect"), ("B", "Back"), ("X", "Scan again"), ("START", "Skip the rest")]);
        assert_eq!(wifi_hints("Continue", false, false, true, true, true)[..2], [("A", "Continue"), ("B", "Back")]);
        assert_eq!(keys(&wifi_hints("Scan again", false, false, true, true, true)), ["A", "B", "START"], "on the Scan again row X is not named twice");
        assert_eq!(keys(&wifi_hints("Connect", false, false, false, false, true)), ["A", "X", "B"], "Settings: no next step");
        assert_eq!(wifi_hints("Connect", true, true, false, false, true)[2], ("Y", "Forget now"));
    }

    #[test]
    fn the_look_steps_hints_name_the_keys_of_the_focused_row() {
        assert_eq!(look_hints(false, false), [("Left/Right", "Layout"), ("A", "Finish"), ("B", "Back")]);
        assert_eq!(look_hints(false, true), [("Left/Right", "Change"), ("A", "Finish"), ("B", "Back")], "Style, Colours, Appearance");
        for below in [false, true] {
            assert!(look_hints(true, below).iter().all(|(_, label)| !label.is_empty()), "every key on Settings → Look has its word");
        }
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
    fn add_games_without_a_network_shows_the_way_that_works() {
        // The RG28XX: no Wi-Fi at all. Never "Connect to Wi-Fi first" there.
        assert_eq!(offline_way(true, true, false), ("Copy from a computer", GAMES_DRIVE_WAY));
        assert_eq!(offline_way(false, true, false), ("Use a second card", SECOND_CARD_WAY));
        assert_eq!(offline_way(true, true, true).0, "Copy from a computer", "not connected yet: the drive works anyway");
        assert_eq!(offline_way(false, true, true), ("Connect to Wi-Fi first", OR_SECOND_CARD_WAY));
        assert_eq!(offline_way(false, false, true), ("Connect to Wi-Fi first", SHARE_OFFLINE), "one slot, no drive: Wi-Fi is the way");
        let de = crate::i18n::german();
        for line in ["Copy from a computer", "Use a second card", GAMES_DRIVE_WAY, SECOND_CARD_WAY, OR_SECOND_CARD_WAY] {
            assert!(de.translate(line).is_some(), "{line} has a German line");
        }
    }

    /// The first start's games step on a card with the GAMES drive says when the drive is there, in three languages, and
    /// fits the half of the page it stands in at 640x480 (painted at 1280 wide) in Normal and Large text.
    #[test]
    fn the_first_start_says_when_the_games_drive_appears() {
        assert!(GAMES_DRIVE_FIRST_WAY.contains("GAMES") && GAMES_DRIVE_FIRST_WAY.contains("first start"));
        let mut text = Text::new();
        let col_w = (1280 - 2 * MARGIN - 40) / 2;
        for (lang, table) in [("en", None), ("de", Some(crate::i18n::Lang::De)), ("es", Some(crate::i18n::Lang::Es))] {
            let line = match table {
                None => GAMES_DRIVE_FIRST_WAY.to_string(),
                Some(l) => l.table().and_then(|t| t.translate(GAMES_DRIVE_FIRST_WAY)).unwrap_or_else(|| panic!("{lang}: no line")).into_owned(),
            };
            assert!(line.contains("GAMES"), "{lang}: {line}");
            for px in [24, 30] {
                let lines = text.wrap_prose(Face::Body, px, &line, col_w);
                assert!(lines.len() <= 4, "{lang} at {px}: {} lines: {lines:?}", lines.len());
            }
        }
    }

    /// K1 of the UX review: "put them on the SD card in its roms folder" was said on every handheld with Wi-Fi, but a computer
    /// reads only the GAMES drive and the Brick's own card; the ROCKNIX cards' STORAGE part it can't read, and Windows offers to
    /// format it. Each kind of device gets the sentence that is true for it, in three languages, and it fits.
    #[test]
    fn the_first_start_does_not_send_a_rocknix_card_into_the_computer() {
        // (false, image_card, games_card, games_drive, card_in_computer, wifi, second_slot)
        let plus = no_games_way(false, false, false, false, true, true); // RG35XX Plus, SP
        let mini = no_games_way(false, false, false, false, true, false); // Mini V2, Odin
        let rg28 = no_games_way(false, false, false, false, false, true); // no Wi-Fi, second slot
        let drive = no_games_way(false, false, true, false, true, true); // a card with the GAMES drive
        let brick = no_games_way(false, false, false, true, false, false);
        assert_eq!(plus, FOUND_WIFI_SLOT_WAY);
        assert_eq!(mini, FOUND_WIFI_WAY);
        assert_eq!(rg28, SECOND_CARD_WAY, "no Wi-Fi: the second card");
        assert_eq!(drive, GAMES_DRIVE_FIRST_WAY);
        assert_eq!(brick, BRICK_CARD_WAY);
        assert_eq!(no_games_way(false, true, true, false, true, true), GAMES_CARD_WAY, "a games card wins over the rest");
        for (name, line) in [("plus", plus), ("mini", mini)] {
            assert!(!line.contains("roms") && !line.contains("Roms"), "{name}: a ROCKNIX card has no folder a computer can use: {line}");
            assert!(line.contains("Cancel"), "{name}: the format warning: {line}");
        }
        assert!(brick.contains("Roms") && !brick.contains("Cancel"), "{brick}");
        assert!(offline_way_for(true, false, false, false).1.contains("Roms"), "Add games on the Brick names its Roms folder");
        let mut text = Text::new();
        let width = 1024 - 2 * MARGIN; // the narrowest canvas (the 4:3 handhelds paint at 1024 wide)
        for line in [FOUND_WIFI_SLOT_WAY, FOUND_WIFI_WAY, BRICK_CARD_WAY] {
            for (lang, table) in [("en", None), ("de", Some(crate::i18n::Lang::De)), ("es", Some(crate::i18n::Lang::Es))] {
                let shown = match table {
                    None => line.to_string(),
                    Some(l) => l.table().and_then(|t| t.translate(line)).unwrap_or_else(|| panic!("{lang}: no line for {line}")).into_owned(),
                };
                for px in [28, 34] {
                    let lines = text.wrap_prose(Face::Body, px, &shown, width);
                    assert!(lines.len() <= 4, "{lang} at {px}: {} lines: {lines:?}", lines.len());
                }
            }
        }
    }

    #[test]
    fn extras_offers_a_only_where_it_works() {
        assert_eq!(keys(&crate::extras_page::hints(None)), ["B"], "offline: nothing to download");
        assert_eq!(keys(&crate::extras_page::hints(Some("Download"))), ["A", "B"]);
        assert_eq!(keys(&crate::extras_page::hints(Some("Change"))), ["Left/Right", "A", "B"], "Pictures still changes offline");
    }

    #[test]
    fn add_games_offers_the_ways_the_device_has() {
        // Wi-Fi, connected or not (unconnected, file sharing leads to the Wi-Fi page), with and without USB transfer.
        assert_eq!(ways_for(true), [Way::Sharing, Way::Phone], "USB file transfer not available: no tile for it");
        // The RG28XX: no Wi-Fi, a second slot (a sentence, not a way) and maybe the cable.
        assert_eq!(ways_for(false), [], "no Wi-Fi, no USB: the sentence about the card or the GAMES drive alone");
        let de = crate::i18n::german();
        for line in ["File sharing", "Connect to Wi-Fi first", "Connect to Wi-Fi", "From your phone", "Or a computer's browser, same Wi-Fi", "On", "Off", "Switching…"] {
            assert!(de.translate(line).is_some(), "{line} has a German line");
        }
    }

    #[test]
    fn add_games_offers_file_sharing_only_with_an_address_or_while_it_is_on() {
        // Y (Send from your phone) stays: offline its page says "Connect to Wi-Fi first" and A leads there.
        assert_eq!(keys(&games_hints(None, false, false)), ["Y", "B"], "offline, sharing off: X would do nothing");
        assert_eq!(games_hints(None, false, true)[0], ("X", "Turn on file sharing"));
        assert_eq!(games_hints(None, true, false)[0], ("X", "Turn off file sharing"), "on: it can be turned off");
        assert_eq!(keys(&games_hints(Some(false), false, false)), ["A", "Y", "B"]);
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
