//! pocket-ui: the shelf. Home shows the last game to continue, recent games and systems;
//! games launch through ROCKNIX's runemu.sh and the shelf comes back when they exit.
//!
//! Test hook on Linux: write commands (up, down, a, b, l1, r1, quit) into /tmp/pocket-ui.in.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::cheats::CheatMenu;
use pocket_ui::control;
use pocket_ui::hostsys;
use pocket_ui::notice::Notice;
use pocket_ui::emu_menu::{EmuMenu, Row};
use pocket_ui::emu_settings;
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::image::{self, Image};
use pocket_ui::library::{self, System};
use pocket_ui::options::{self, GameSettings, Level};
use pocket_ui::preview;
use pocket_ui::palette::*;
use pocket_ui::quick::{self, Sys};
use pocket_ui::settings::{Appearance, Frames, Settings, Theme};
use pocket_ui::shelf::{self, Item, StartTap};
use pocket_ui::quit::{self, Escalation, How};
use pocket_ui::setup::{Only, Setup, SetupEvent};
use pocket_ui::standalone;
use pocket_ui::system_art::Sprite;
use pocket_ui::pad::{Axis, Button, PadEvent, PadState};
use pocket_ui::recents::{self, Recents};
use pocket_ui::slots;
use pocket_ui::states::{self, StateSet, Target};
use pocket_ui::text::{Face, Text};
use pocket_ui::touch::{self, Area};
use pocket_ui::update::{self, Manifest};
use winit::application::ApplicationHandler;

mod store_page;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

/// Game switcher and favourites: the App's side of `pocket_ui::switcher` and `pocket_ui::favorites`.
mod app_switcher;
/// Switching between pocket-core games without waiting for the old process to end (`pocket_ui::fast_switch`).
mod app_fast_switch;
/// The touch screen: taps and swipes on the shelf, the game lists, Settings and the switcher.
mod app_touch;
/// Playing on a TV through USB-C: the thread that listens to sway's outputs, and what the UI does with it.
mod app_display_out;
// Pages of rows and text in a column on a wide screen (the Pocket 5's and Flip 2's 1920x1080).
mod app_column;
/// The look step's previews of each theme.
mod app_look;
/// The start after a switch-off after sleeping says why, once (`pocket_ui::auto_off`).
mod app_auto_off;
mod app_pictures;
/// The picture side of Home's Curator, Favorites and pinned rows (`pocket_ui::home_panel`).
mod app_home_panel;
/// The game card (X on a game), play status and notes, offline game facts.
mod app_card;
/// Games recognised by their checksum, and the card's "Wrong pictures? Look up again".
mod app_identify;
/// The console pages and the short facts about a system on the shelf.
mod app_console;
/// Settings > Sync, and the save scan after a game.
mod app_sync;
mod app_sync_conflicts;
/// Settings > Sync > Library data: the merge with the other devices on a thread (`pocket_ui::library_sync`).
mod app_library_sync;
/// Settings > Look > Appearance: dark, light or automatic, and the crossfade.
mod app_appearance;
/// The Cabinet look: cases on wooden shelves, pulled out and put into the console.
mod app_cabinet;
/// Settings > LEDs, and the game's colour on the sticks (`leds.rs`).
mod app_leds;
/// Settings > Night light, and its steps on the UI thread (`night_light.rs`).
mod app_night_light;
/// Settings > Tailscale: switch, QR sign-in, status.
mod app_tailscale;
/// Settings > Bluetooth: the switch, a search, pairing controllers.
mod app_bluetooth;
/// Settings > Performance & fan: game defaults, the fan outside games, the live readout.
mod app_perf_fan;
/// Settings → System → Sleep & power: the sleep times and the power button (`pocket_ui::sleep_settings`).
mod app_sleep_power;
// Settings → System → About's rows and Report a problem (`pocket_ui::report`).
mod app_report;
// Settings > Web companion and the edits browsers send (`app_web.rs`, `pocket_ui::web`).
mod app_web;
/// Quick settings' Performance and Fan rows: the game under the panel, a change saved and applied on a thread.
mod app_quick_profiles;
/// PortMaster ports that need the player's own game files: the drop folder and the check before a start.
mod app_ports;
/// A PortMaster port's Performance & fan (START on its card), applied while it runs; the busy-core hint.
mod app_port_perf;
/// Curator (docs/CURATOR.md): the index thread, Home's Curator rows, the Lens, Echoes, Versions, stickers.
mod app_curator;
/// Play history (docs/HISTORY.md): the log's writer and migration, the History and Year in review pages.
mod app_history;
/// RetroAchievements: Settings > RetroAchievements, the card's and console page's tallies, the pause menu's list.
mod app_achievements;
/// Finding games fast: the search (the Search row on Home) and the letter jump of the lists (L2/R2).
mod app_search;
mod app_starter;
/// Pixel Path: the pause menu's graded chain of a game's picture and sound (docs/CURATOR.md).
mod app_pixel_path;
/// The pause menu at once: pocket-core paused before it is covered, its picture after the menu is up.
mod app_menu_open;
mod app_players;
/// The pause menu's Disc row: changing discs of multi-disc games on pocket-ui's own host.
mod app_disc;
mod app_earlier;
/// Game saves that don't reach the card: the pause menu's line and its question before Quit.
mod app_save_trouble;
/// Which emulator plays a game (system.cfg's choices, as EmulationStation keeps them) and the "Runs on" page.
mod app_emu_choice;
mod app_this_game;
/// Speed guard: the frame line of the latency trace (POCKETUI_TRACE, docs/LATENCY.md).
mod app_trace;
/// Settings > Look > Motion: page transitions, list glides, the hint bar's crossfade, the frame log.
mod app_motion;
/// Settings > Tools: ROCKNIX's tools and the Store's, started like an app (feat/tools-wifi).
mod app_tools;
/// Settings > PC streaming: pairing Moonlight with the PIN on screen, the paired PCs, their games (feat/tools-wifi).
mod app_moonlight;
/// Settings > Backup & reset: pocket-ui's settings and library notes to the card and back, reset (feat/tools-wifi).
mod app_backup;
/// Settings > Games > From EmulationStation: the player's gamelists, their favorites and ratings (`pocket_ui::gamelist`).
mod app_gamelist;
/// The screenshot gallery: Settings → Games → Screenshots and the game card's "Screenshots (N)" row (`pocket_ui::gallery`).
mod app_gallery;
/// Settings > System > Storage, the low-space notice, and Hide or delete on the game card (`pocket_ui::storage`).
mod app_clock;
mod app_storage;
// Safe mode (the rescue page) and the dialog before EmulationStation (`pocket_ui::rescue`).
mod app_rescue;
// Settings → Tour and "New in this version": the feature cards outside the first start (`pocket_ui::features`).
mod app_tour;
/// Settings in categories: the category list, a category's rows, the way back (`SETTINGS_GROUPS`).
mod app_settings_groups;
/// The status corner at the top right (time, Wi-Fi, battery): its top line, the minute wake, its thread.
mod app_status_corner;
/// The game list from the cache at start, read again after the first frame (`pocket_ui::library_cache`).
mod app_library;
/// HOME + A in a game pocket-core does not host (the standalone emulators, ports): `grim` on a thread of its own.
mod app_screenshot;
/// Doc-drift tests: the user guide and the developer docs against the code (docs/DEVELOPING.md, "Documentation checks").
#[cfg(test)]
mod doc_checks;

/// The covers of a Curator, Favorites or pinned row beside the Shelf look's front (`app_home_panel`): one size
/// whatever the text size, so the picture cache keeps one copy.
const LABEL_ART_H: i32 = 176;
/// How high the covers are in Split's panel.
const SPLIT_PANEL_H: i32 = 260;

const PULL: Duration = Duration::from_millis(120);
const FADE: Duration = Duration::from_millis(250);
/// What the start puts off until the first frame (`App::after_first_frame`) runs by this time even without one.
const AFTER_FIRST_FRAME_AT_LATEST: Duration = Duration::from_secs(2);
/// Frames go on this long after a fade, so that its last one is drawn at the end (the launch picture at full strength,
/// not left at 95 % over the shelf).
const LAST_FADE_FRAME: Duration = Duration::from_millis(50);
const REPEAT_DELAY: Duration = Duration::from_millis(300);
const REPEAT_EVERY: Duration = Duration::from_millis(70);

/// The shelf's held direction after a button event, whichever page the event goes to (review SOL-08): it ends when it is
/// let go (Left/Right repeat as Up/Down) or when another button is pressed, which may open a page whose input never
/// shows the shelf the release; it used to repeat on behind Settings and after it.
fn repeat_after(held: Option<(Button, Instant)>, button: Button, pressed: bool) -> Option<(Button, Instant)> {
    let as_held = match button {
        Button::Left => Button::Up,
        Button::Right => Button::Down,
        b => b,
    };
    held.filter(|(b, _)| if pressed { *b == as_held } else { *b != button && *b != as_held })
}
/// Pixels a second the manual scrolls with a stick pushed all the way, and with Up/Down held.
const MANUAL_STICK: f32 = 1500.0;
const MANUAL_HELD: f32 = 900.0;
const EXIT_HOLD: Duration = Duration::from_secs(1);
const RECENTS_ON_HOME: usize = 6;
const TOAST: Duration = Duration::from_millis(1200);
/// Animations draw at most this often (60 Hz).
const FRAME: Duration = Duration::from_micros(16_667);

/// The Retroid prints A on the right face button and B on the bottom one.
const CONFIRM: Button = Button::East;
const CANCEL: Button = Button::South;

#[derive(Debug)]
enum UserEvent {
    Pad(PadEvent),
    /// The finger on the touch screen (`pocket_ui::touch`); pocket-core reads it itself while a game runs.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Touch(touch::TouchEvent),
    /// `ok`: the game's process ended with exit code 0. `pgid`: its process group, which tells a game that has
    /// handed over already (`app_fast_switch`) from the one running now.
    GameExited { secs: u64, ok: bool, pgid: Option<i32> },
    /// pocket-core `pgid` has written its exit save and may be replaced (`pocket_ui::fast_switch`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    GameSaved { pgid: i32 },
    /// pocket-core's picture `seq` for behind the pause menu, read (`app_menu_open`).
    MenuShot { seq: u32, image: Option<pocket_ui::preview::Pixels> },
    /// The picture of where the game was left, read and scaled for the launch (`pocket_ui::launch_picture`).
    LaunchPicture(Option<pocket_ui::launch_picture::Prepared>),
    /// Applying PS2, GameCube, Wii or 3DS options by a restart: the resume state was written (or not).
    OptionsSaved(bool),
    Setup(SetupEvent),
    Update(UpdateEvent),
    Store(store_page::Event),
    /// The starter pack's downloads and scans (`app_starter`).
    Starter(app_starter::Event),
    /// The automatic cover download (`pocket_ui::auto_covers`).
    Covers(CoversEvent),
    /// The game card's descriptions loaded ahead (`pocket_ui::auto_texts`, `app_card`).
    Texts(pocket_ui::auto_texts::Event),
    /// Games recognised by their checksum, and "Look up again" (`app_identify`).
    Identify(app_identify::IdentifyEvent),
    /// A screenshot or title screen decoded by the loader thread (`app_pictures`).
    Picture(pocket_ui::picture_loader::Key, Option<Image>),
    /// Wi-Fi came up after boot (or was up already): the automatic covers may start.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Online,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Quit,
    /// A mock-only command FIFO capture (`--test-driver <temporary directory>`).
    TestFrame,
    /// The command FIFO's `leave`: the dialog SELECT + START held opens (`app_rescue`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Leave,
    /// pocket-core's picture preview for the pause menu (None: it didn't answer).
    Preview { seq: u32, images: Option<Vec<preview::Pixels>> },
    /// Sleep now: the quick settings' Sleep (and the power menu's).
    Sleep,
    /// Sleep has news from a thread of its own (back from deep sleep, a power-off that failed, a wake from a suspend it
    /// didn't start, the battery guard); `about_to_wait` runs `Sleep::tick`, which reads it. It used to arrive as `Sleep`,
    /// which put the device to sleep again when the news came while awake.
    SleepNews,
    /// "poweroff" on the command FIFO: save the game and power off (`Sleep::power_off`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    PowerOff,
    /// Power off or restart from the power menu or the quick settings: save the game first (`Sleep::shut_down`).
    Shutdown(pocket_ui::system_ctl::PowerAction),
    /// "powermenu" on the command FIFO: open the power menu as a long press would.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    PowerMenu,
    /// The game card's description from Wikipedia arrived, or could not be loaded (`app_card`).
    CardText { title: String, text: Result<String, String> },
    /// Syncthing: news for Settings > Sync, and the result of the scan after a game (`app_sync.rs`).
    Sync(pocket_ui::sync_page::Event),
    /// Tailscale: news for Settings > Tailscale (`app_tailscale.rs`).
    Tailscale(pocket_ui::tailscale_page::Event),
    /// Bluetooth: news for Settings > Bluetooth (`app_bluetooth.rs`).
    Bluetooth(pocket_ui::bluetooth_page::Event),
    /// Settings > Performance & fan's readout, and the end of a clean-up the fan outside games waited for (`app_perf_fan.rs`).
    PerfFan(pocket_ui::perf_fan_page::Event),
    /// A step of the night light's fade: the white point to show, 0 = off (`app_night_light.rs`).
    Night(u16),
    /// The web companion: an edit from a browser, news for its Settings page (`app_web.rs`).
    Web(app_web::Event),
    /// Settings > Tools: the tools were found, a Store tool ran (`app_tools.rs`).
    Tools(pocket_ui::tools_page::Event),
    /// Settings > PC streaming: pairing, the PCs, their games (`app_moonlight.rs`).
    Moonlight(pocket_ui::moonlight_page::Event),
    /// Settings > Backup & reset: the backups were listed, a job finished (`app_backup.rs`).
    Backup(pocket_ui::backup_page::Event),
    /// EmulationStation's gamelists were read and differ from what the App has (`app_gamelist.rs`).
    Gamelist(std::sync::Arc<pocket_ui::gamelist::Index>),
    /// The screenshots folder was read, or a screenshot was copied to the shelf (`app_gallery.rs`).
    Gallery(app_gallery::Event),
    /// The card was measured or cleaned, the free space looked at, a game's delete worked out or done (`app_storage.rs`).
    Storage(app_storage::Event),
    /// Curator's index thread (`app_curator.rs`).
    Curator(app_curator::Event),
    /// RetroAchievements: a login finished, a game's tally was looked up (`app_achievements.rs`).
    Achievements(pocket_ui::achievements_page::Event),
    /// pocket-core's Pixel Path report changed while its page is open (`app_pixel_path.rs`).
    PixelPath(Box<pocket_ui::pixel_path::Report>),
    /// A standalone game's settings, read for Pixel Path when the pause menu opened (`app_pixel_path`).
    PixelSetup(PathBuf, Box<Result<pocket_ui::pixel_standalone::Setup, String>>),
    /// pocket-core seated the players anew while the Players page is open (`app_players.rs`).
    Players,
    /// A pad event already merged over every controller (`App::merge_pad`), passed on as `Pad`.
    Merged(PadEvent),
    /// The status corner's thread read a change: Wi-Fi connected or gone, the battery (`app_status_corner`).
    StatusCorner,
    /// The values of Settings rows that need a process or a file came in (`app_settings_groups`, `settings_values`).
    SettingsValues,
    /// The folders were read again and the list differs from the one on screen (`app_library`); the cache holds it
    /// for the key.
    LibraryRescanned(Vec<System>, Option<pocket_ui::library_cache::Key>),
    /// The picture went to a TV through USB-C, or back to the panel (`app_display_out`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Display(Option<pocket_ui::display_out::Tv>),
}

#[derive(Debug)]
enum CoversEvent {
    Progress { done: usize, total: usize, found: usize },
    /// `stopped`: a game started and cut the run short. `pictures_looked_up`: the games whose screenshots and title
    /// screens it got to (`pocket_ui::game_pictures`).
    Done { looked_up: Vec<PathBuf>, pictures_looked_up: Vec<PathBuf>, found: usize, total: usize, stopped: bool },
}

/// What a row of the Settings list opens.
#[derive(Clone, Copy)]
enum Opens {
    Setup(Only),
    Store,
    /// Free games to download (`app_starter`).
    Starter,
    /// Time zone and network time (src/quick.rs).
    Time,
    Updates,
    Battery,
    /// The user guide (src/manual.rs).
    Manual,
    /// Writes the bug-report file and shows where it went (`app_report`).
    Report,
    About,
    /// A switch, not a page: menu haptics on or off.
    Haptics,
    /// A switch, not a page: Home or the last game when the Pocket starts (`on_start`).
    OnStart,
    /// A switch, not a page: whether the pause menu's Quit keeps a resume point (`App::remember_quit`).
    RememberQuit,
    /// A switch, not a page: Cozy or Quick animations (`App::animations`).
    Animations,
    /// A switch, not a page: which face button confirms in the menus (`button_layout`).
    ButtonLayout,
    /// A switch, not a page: Auto frames on the handhelds or on every console (`settings::Frames`).
    Frames,
    /// Over the game: Off, FPS, FPS + sensors, Pixel Path; Left/Right step, A goes on (`over_game`).
    OverGame,
    /// A switch, not a page: the interface's language (`pocket_ui::i18n`), used from the next frame on.
    Language,
    /// A switch, not a page: Normal or Large type (`pocket_ui::type_scale`), used from the next frame on.
    TextSize,
    /// The feature cards (`app_tour.rs`).
    Tour,
    /// A switch (A) and a level (left/right): the UI sounds.
    Sounds,
    /// Syncthing: saves and states kept in step with other devices (`app_sync.rs`).
    Sync,
    /// The lights around the sticks (`app_leds.rs`).
    Leds,
    /// Warmer colours in the evening (`app_night_light.rs`).
    NightLight,
    /// Tailscale: the private network to the Pocket's other devices (`app_tailscale.rs`).
    Tailscale,
    /// Bluetooth: pairing controllers (`app_bluetooth.rs`).
    Bluetooth,
    /// The web companion: the library and uploads from a browser (`app_web.rs`).
    Web,
    /// CPU and fan defaults for games, the fan outside games (`app_perf_fan.rs`).
    PerfFan,
    /// Curator's switches (`app_curator.rs`).
    Curator,
    /// The RetroAchievements account (`app_achievements.rs`).
    Achievements,
    /// ROCKNIX's tools and the Store's (`app_tools.rs`).
    Tools,
    /// Pairing PCs for Moonlight, their games (`app_moonlight.rs`).
    PcStreaming,
    /// Backup, restore and reset of pocket-ui's settings (`app_backup.rs`).
    Backup,
    /// What takes the room on the card, the clean-ups, the hidden games (`app_storage.rs`).
    Storage,
    /// The favorites and ratings of EmulationStation's gamelists (`app_gamelist.rs`).
    Gamelist,
    /// The screenshots, newest first (`app_gallery.rs`).
    Screenshots,
    /// Screen off after, Sleep after, Switch off after sleeping, the power button (`app_sleep_power.rs`).
    SleepPower,
}

/// Under Settings → Battery: where the times of sleep went (`app_sleep_power`, 2026-10-26).
const SLEEP_SETTINGS_POINTER: &str = "When the screen goes off and the Pocket sleeps: Settings \u{203a} System \u{203a} Sleep & power.";

/// The highest an empty page's picture (`ui_art::above_words`) goes: under the top line and its status corner.
const EMPTY_ART_TOP: i32 = 110;

/// The Games line while the Store row is hidden (`app_settings_groups::blurb`).
const GAMES_BLURB_WITHOUT_STORE: &str = "Add games, starter pack, extras, screenshots, tools, PC streaming.";

/// A row of Settings: its name, what it opens, the line explaining it.
type SettingsRow = (&'static str, Opens, &'static str);

/// Settings in categories (the flat list of 27 rows was too long, the owner, 2026-10-23): a category's name, the line
/// naming what is in it, its rows in their order. START opens the categories, A a category's list
/// (`app_settings_groups.rs`). A new row is one line in its category; `SETTINGS` follows by itself.
const SETTINGS_GROUPS: [(&str, &str, &[SettingsRow]); 5] = [
    ("Look & feel", "Look, language, text, night light, frames, sounds, haptics, LEDs, animations.", &[
        ("Look", Opens::Setup(Only::Look), "Choose how the shelf looks, dark or light."),
        ("Language", Opens::Language, "Menus and messages in English or German."),
        ("Text size", Opens::TextSize, "Larger text on every screen, fewer rows on a page."),
        (app_night_light::SETTINGS_NAME, Opens::NightLight, "Warmer, less blue colours in the evening."),
        ("Screen frames", Opens::Frames, "Auto frames on the handhelds, or on every console."),
        ("Sounds", Opens::Sounds, "Soft clicks in menus, not in games. Left/right: how loud."),
        ("Haptics", Opens::Haptics, "A soft tick as you move."),
        ("LEDs", Opens::Leds, "The lights around the sticks. Off unless you turn them on."),
        ("Animations", Opens::Animations, "Cozy: watch your game slide into the console. Quick: straight into the game."),
    ]),
    ("Playing", "On start, quit, buttons, over the game, performance, battery, achievements, Curator.", &[
        ("On start", Opens::OnStart, "Home, or straight back into the game you played last."),
        ("Remember where I quit", Opens::RememberQuit, "Quit keeps the moment for the next start."),
        ("Button layout", Opens::ButtonLayout, "A on the right (Nintendo) or bottom (Xbox)."),
        ("Over the game", Opens::OverGame, "FPS, sensors or Pixel Path over every game."),
        (app_perf_fan::SETTINGS_NAME, Opens::PerfFan, "Processor speed and fan, in games and in menus."),
        ("Battery", Opens::Battery, "Battery history, time left, what each game uses."),
        (app_achievements::SETTINGS_NAME, Opens::Achievements, "Earn achievements in your games."),
        (app_curator::SETTINGS_NAME, Opens::Curator, "Lens, smart shelves, stickers, Echoes, versions and play history."),
    ]),
    ("Games", "Add games, starter pack, extras, screenshots, the Store, tools, PC streaming.", &[
        ("Add games", Opens::Setup(Only::Games), "Where each system's games go, and file sharing."),
        (app_gamelist::SETTINGS_NAME, Opens::Gamelist, "Favourites and ratings from your EmulationStation gamelists."),
        (app_starter::SETTINGS_NAME, Opens::Starter, "A few free games to start with, downloaded to your SD card."),
        ("Extras", Opens::Setup(Only::Extras), "Cover art, game pictures and PortMaster games."),
        (app_gallery::SETTINGS_NAME, Opens::Screenshots, "The pictures you took with HOME + A, newest first."),
        ("Store", Opens::Store, "Emulators, frames, themes, tools."),
        (app_tools::SETTINGS_NAME, Opens::Tools, "Emulator menus, RetroArch, gamepad test and calibration, terminal."),
        (app_moonlight::SETTINGS_NAME, Opens::PcStreaming, "Pair a gaming PC for Moonlight, and stream its games."),
    ]),
    ("Connections", "Wi-Fi, Bluetooth, Tailscale, Sync, remote access, web companion.", &[
        ("Wi-Fi", Opens::Setup(Only::Wifi), "Connect to a network."),
        ("Bluetooth", Opens::Bluetooth, "Pair a controller or headphones, see what is connected."),
        ("Tailscale", Opens::Tailscale, "Reach the Pocket from your other devices."),
        ("Sync", Opens::Sync, "Keep saves and states in step with your other devices."),
        ("Remote access (SSH)", Opens::Setup(Only::Remote), "Log in from a computer. Turns itself off after 30 minutes."),
        (app_web::SETTINGS_NAME, Opens::Web, "Your games, play time and uploads in a browser."),
    ]),
    ("System", "Time, sleep & power, updates, storage, backup & reset, Tour, manual, about.", &[
        ("Time", Opens::Time, "Time zone, date and network time."),
        (app_sleep_power::SETTINGS_NAME, Opens::SleepPower, "When the screen goes off and the Pocket sleeps, and the power button."),
        ("Updates", Opens::Updates, "New versions of pocket-ui and the system."),
        (app_storage::SETTINGS_NAME, Opens::Storage, "What takes up space on the SD card, and what can go."),
        (app_backup::SETTINGS_NAME, Opens::Backup, "Save pocket-ui's settings to the SD card, restore them, or start afresh."),
        ("Tour", Opens::Tour, "Every feature on a card, with a picture: switch it on or off."),
        ("Manual", Opens::Manual, "The user guide, on the device."),
        (pocket_ui::report::ROW, Opens::Report, "Saves a file for a bug report, to attach on GitHub."),
        ("About", Opens::About, "Version, storage and network."),
    ]),
];

/// Every row of Settings, category after category: `SettingsPage::List` and `settings_row` count in this order.
const SETTINGS: [SettingsRow; app_settings_groups::count()] = app_settings_groups::flat();

#[derive(Debug)]
enum UpdateEvent {
    Checked(Result<Manifest, String>),
    Progress(u64),
    /// An install finished: `Ok(true)` for the app (restart next), `Ok(false)` for the system.
    Done(Result<bool, String>),
    /// The system update was staged; the note says what the new system lacks that pocket-ui uses (`os_compat`).
    OsStaged(Option<String>),
}

#[derive(Clone, Copy, PartialEq)]
enum UpdateAction {
    Check,
    InstallApp,
    StageOs,
    CancelOs,
}

/// The Updates page: what the channel offers and what is being done about it.
#[derive(Default)]
struct Updates {
    sel: usize,
    manifest: Option<Manifest>,
    /// Running download or install: label and expected size.
    busy: Option<(String, u64)>,
    /// When `busy` was set; a stuck job stops blocking auto-sleep after `update::BUSY_BLOCKS_SLEEP_FOR`.
    busy_since: Option<Instant>,
    /// Stops the running download (not available for the check).
    cancel: Option<update::Cancel>,
    done: u64,
    message: Option<String>,
    /// The new app version is in place; pocket-ui restarts once this moment has passed.
    restart_at: Option<Instant>,
}

impl Updates {
    fn start(&mut self, label: String, total: u64, cancel: Option<update::Cancel>) {
        self.busy = Some((label, total));
        self.busy_since = Some(Instant::now());
        self.cancel = cancel;
    }

    fn finish(&mut self) {
        self.busy = None;
        self.busy_since = None;
        self.cancel = None;
    }

    /// A download is running and has not been told to stop yet.
    fn can_cancel(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| !c.is_cancelled())
    }

    /// Stops the running download; false when there is none.
    fn request_cancel(&mut self) -> bool {
        let Some(cancel) = self.cancel.as_ref().filter(|c| !c.is_cancelled()) else { return false };
        cancel.cancel();
        self.busy = Some(("Cancelling".into(), 0));
        true
    }
}

/// Row of the Settings list called `name`, to come back to from its page.
fn settings_row(name: &str) -> usize {
    SETTINGS.iter().position(|s| s.0 == name).unwrap_or(0)
}

/// First index of a scrolled window of `visible` rows that keeps `sel` inside it.
fn window_start(sel: usize, visible: usize, len: usize) -> usize {
    if len <= visible {
        0
    } else {
        sel.saturating_sub(visible / 2).min(len - visible)
    }
}

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
enum SettingsPage {
    /// The categories (`SETTINGS_GROUPS`), the selected one; what START opens.
    Groups(usize),
    /// A category's list, at this row of `SETTINGS`: the category is the one the row is in.
    List(usize),
    Store,
    /// The starter pack's games (`app_starter`).
    Starter,
    Updates,
    /// Index into `battery::RANGES` for the graph.
    Battery(usize),
    /// The user guide; where the reader is lives in `App::manual`.
    Manual,
    About,
    Sync,
    /// The selected row.
    Leds(usize),
    /// The selected row (`app_night_light.rs`).
    NightLight(usize),
    Tailscale,
    Bluetooth,
    Web,
    PerfFan,
    Curator,
    Achievements,
    Tools,
    PcStreaming,
    Backup,
    Storage,
    Gamelist,
    Screenshots,
    /// Settings → System → Sleep & power; the page itself is `App::sleep_page`.
    SleepPower,
}

#[derive(Clone, Copy, PartialEq, Hash)]
enum Screen {
    Home,
    Games(usize),
    Favorites,
    /// Curator's list (the Lens, smart shelves, stickers), a smart shelf, a sticker, the Lens's games (`app_curator`).
    Curator,
    Shelf(usize),
    Sticker(u32),
    LensResults,
}

struct Running {
    system: String,
    name: String,
    path: PathBuf,
    emulator: String,
    started: Instant,
    /// Process group of the launched game (runemu.sh and everything it starts).
    pgid: Option<i32>,
    /// pocket-core: the Options as the pause menu last left them, saved or not; the menu opens on them again.
    options: options::Session,
}

/// Applying PS2, GameCube, Wii or 3DS options that need a new start (`emu_settings::Restart`), and the game to start again.
struct Reapply {
    restart: emu_settings::Restart,
    game: (String, String, PathBuf),
}

/// Systems played with our own host instead of RetroArch, and the libretro core they use (in the library, so that the
/// OS update check knows them too: `os_compat`).
const HOST_CORES: &[(&str, &str)] = pocket_ui::hostsys::HANDHELD_CORES;
const HOST_BIN: &str = "/storage/pocket-ui/pocket-core";

impl Running {
    fn hosted(&self) -> bool {
        self.emulator == "pocket-core"
    }

    /// An app, not a game (PC streaming, a tool of Settings > Tools): no pause menu, no switcher, no recents, no
    /// saved state.
    fn is_app(&self) -> bool {
        self.emulator == pocket_ui::moonlight::EMULATOR || self.emulator == pocket_ui::tools::EMULATOR
    }

    /// Slot-0 save state for this game: RetroArch's `.state`, or pocket-core's `.pstate`, both in the
    /// system's savestate folder.
    fn state_file(&self) -> PathBuf {
        let stem = self.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let ext = if self.hosted() { "pstate" } else { "state" };
        PathBuf::from(format!("/storage/roms/savestates/{}/{stem}.{ext}", self.system))
    }
}

/// In-game menu opened with a HOME tap while a RetroArch game runs.
struct QuickMenu {
    sel: usize,
    /// The game as it looked when the menu opened, shown dimmed behind the menu.
    frame: Option<Image>,
    /// Preview and age of the state the selected row (Save or Load) is showing, if there is one.
    saved: Option<(Option<Image>, std::time::SystemTime)>,
    /// The states of the game when its emulator has slots (pocket-core, ARMSX2, Dolphin, Azahar; `states.rs`):
    /// Save writes the manual slots, Load lists Auto, Before load and the slots.
    states: Option<StateSet>,
    /// The manual slot Save writes and the hotkeys use (0-based).
    slot: usize,
    /// Which slots hold a state (the dots), and what Load lists with the one it is on.
    filled: Vec<bool>,
    loads: Vec<Target>,
    load_at: usize,
    /// Quit was chosen; the menu stays up until the game has saved and exited.
    quitting: bool,
    /// Quit was chosen and leaves out the resume point (`without_saving`).
    discarding: bool,
    /// Quit leaves out the resume point: Settings > Remember where I quit is off and the emulator can
    /// (`quit::offers_quit_without_saving`; not RetroArch). Fixed when the menu opens.
    without_saving: bool,
    /// The game's sway window, to hand the screen back to.
    window: Option<i64>,
    /// Process group paused while the menu is up (standalone emulators; RetroArch pauses itself).
    frozen: Option<i32>,
    /// RetroArch games (network commands) and pocket-core games (command FIFO) get a save slot;
    /// other emulators only Continue and Quit.
    retroarch: bool,
    hosted: bool,
    page: Page,
    /// Options of a pocket-core game; edits apply to this session until saved.
    settings: Option<GameSettings>,
    /// Short confirmation after Save changes, shown under the options list.
    note: Option<&'static str>,
    /// Live picture preview next to the Picture settings (pocket-core games).
    live: preview::Live,
    /// The core's own options and the console's button names, for the Emulator and Controls pages.
    emu: Option<EmuMenu>,
    /// The game's cheats from the libretro database, if it has any (the Cheats page).
    cheats: Option<CheatMenu>,
    /// PS2, GameCube, Wii and 3DS: the emulator's options for this game (the Options page of a standalone game).
    emu_set: Option<emu_settings::Menu>,
    /// "Advanced…" was chosen: back to the game, then the emulator's own menu.
    want_advanced: bool,
    /// Options are being applied by a restart ("Applying…" instead of "Saving and quitting…").
    applying: bool,
    /// Why the options could not be applied, under the main items.
    emu_note: Option<String>,
    /// RetroAchievements: pocket-core's list for this game, when an account is signed in (the Achievements item).
    achievements: Option<pocket_ui::achievements::snapshot::Snapshot>,
    /// Settings → Curator → Pixel Path is on: the menu lists it (`app_pixel_path.rs`).
    pixel_item: bool,
    /// The Pixel Path page while it is open; its reader thread ends when this drops.
    pixel: Option<pocket_ui::pixel_path_page::PixelPathPage>,
    /// Two or more controllers can play this pocket-core game: the menu lists Players (`app_players.rs`).
    players_item: bool,
    /// The Players page while it is open; its reader thread ends when this drops.
    players: Option<pocket_ui::players_page::PlayersPage>,
    /// What saving can do for this emulator (`pause_saves`): Save and Load only where they do something, else a line.
    saves: pocket_ui::pause_saves::Saves,
    /// A multi-disc game on pocket-core: the Disc row and the disc chosen on it (`app_disc`).
    disc: Option<pocket_ui::disc::DiscRow>,
    /// pocket-core: the Earlier row, its states and the one chosen (`app_earlier`); None: no row.
    earlier: Option<pocket_ui::timeline::Row>,
    /// pocket-core's saves of this game don't reach the card (`app_save_trouble`): a line under the title, and Quit
    /// asks first. Read when the menu opens.
    save_trouble: Option<pocket_ui::save_trouble::Trouble>,
}

/// Which screen of the pause menu is showing.
#[derive(Clone, Copy, PartialEq, Hash)]
enum Page {
    Main,
    /// The list of groups plus "Save changes".
    Options { sel: usize },
    Group { group: usize, sel: usize },
    SaveChanges { sel: usize },
    /// The core's options: `cat` is the category being shown (None: the top level).
    Emulator { cat: Option<usize>, sel: usize },
    Controls { sel: usize },
    Cheats { sel: usize },
    /// A PS2, GameCube, Wii or 3DS game's options (`emu_settings`).
    Standalone { sel: usize },
    /// The running game's achievements (`app_achievements`).
    Achievements { sel: usize },
    /// Pixel Path (`app_pixel_path`); where it stands is in `QuickMenu::pixel`.
    PixelPath,
    /// "Runs on" (`app_emu_choice`): the emulator and RetroArch settings; its state is in `App::emu`.
    EmuChoice,
    /// Players (`app_players`); its state is in `QuickMenu::players`.
    Players,
    /// A running port's Performance & fan (`app_port_perf`); its state is in `App::port_perf`.
    PortPerf,
    /// "Couldn't save. Quit anyway?" before Quit while the game's saves fail (`app_save_trouble`).
    QuitUnsaved { sel: usize },
}

/// Lines of the options list: the groups of `options::GROUPS` with Emulator and Controls after the
/// first one (Picture), and Save changes last.
#[derive(Clone, Copy, PartialEq)]
enum OptEntry {
    Group(usize),
    Emulator,
    Controls,
    Cheats,
    /// Which emulator plays the game (`app_emu_choice`): a page with its own Save changes.
    RunsOn,
    Save,
}

fn opt_entries(emu: Option<&EmuMenu>, cheats: bool) -> Vec<OptEntry> {
    let mut v: Vec<OptEntry> = (0..options::GROUPS.len()).map(OptEntry::Group).collect();
    if let Some(emu) = emu {
        let at = 1.min(v.len());
        v.insert(at, OptEntry::Controls);
        if emu.has_options() {
            v.insert(at, OptEntry::Emulator);
        }
    }
    if cheats {
        // After Controls (or Picture when there is no snapshot).
        let at = v.iter().position(|e| *e == OptEntry::Controls).or_else(|| (!v.is_empty()).then_some(0)).map_or(0, |i| i + 1);
        v.insert(at, OptEntry::Cheats);
    }
    v.push(OptEntry::RunsOn);
    v.push(OptEntry::Save);
    v
}

fn opt_pos(emu: Option<&EmuMenu>, cheats: bool, e: OptEntry) -> usize {
    opt_entries(emu, cheats).iter().position(|x| *x == e).unwrap_or(0)
}

const SAVE_CHANGES: [&str; 3] = ["For this console", "For this game", "Restore defaults"];

impl QuickMenu {
    /// The state the selected row works on: Save's slot, or Load's choice.
    fn shown(&self) -> Option<Target> {
        self.states.as_ref()?;
        match self.items().get(self.sel).copied() {
            Some(MainItem::Load) => self.loads.get(self.load_at).copied(),
            _ => Some(Target::Slot(self.slot as u8 + 1)),
        }
    }

    /// The preview and age of what `shown` names, or of the earlier state chosen on Earlier (`app_earlier`).
    fn refresh_saved(&mut self) {
        if self.item() == MainItem::Earlier {
            self.saved = self.earlier.as_ref().and_then(app_earlier::preview);
            return;
        }
        if let (Some(states), Some(t)) = (self.states.as_ref(), self.shown()) {
            self.saved = states.preview(t);
        }
    }

    fn items(&self) -> Vec<MainItem> {
        let mut items = main_items(self.hosted, self.saves, self.achievements.is_some());
        // Earlier, on pocket-core: only while it has a row (`app_earlier`: switched on, or states of it kept).
        if self.earlier.is_none() {
            items.retain(|i| *i != MainItem::Earlier);
        }
        // Disc, for a game with more than one: after Save and Load, before Options (`app_disc`).
        if self.disc.is_some() {
            let at = items.iter().position(|i| *i == MainItem::Options).unwrap_or(items.len());
            items.insert(at, MainItem::Disc);
        }
        // Pixel Path, when switched on in Settings: before Quick settings, after what belongs to the game.
        if self.pixel_item {
            let at = items.iter().position(|i| *i == MainItem::QuickSettings).unwrap_or(items.len());
            items.insert(at, MainItem::PixelPath);
        }
        // Players, with a second controller: after what belongs to the game, before Pixel Path and Quick settings.
        if self.players_item {
            let at = items.iter().position(|i| matches!(i, MainItem::PixelPath | MainItem::QuickSettings)).unwrap_or(items.len());
            items.insert(at, MainItem::Players);
        }
        items
    }

    /// The selected row of the main page.
    fn item(&self) -> MainItem {
        self.items().get(self.sel).copied().unwrap_or(MainItem::Continue)
    }
}

/// The rows of the pause menu's main page. Actions are matched on these, never on the label text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MainItem {
    Continue,
    Save,
    Load,
    /// pocket-core's rolling states: one every 5 minutes of play and where sessions began (`app_earlier`).
    Earlier,
    /// Which disc is in the drive, for multi-disc games on pocket-core (`app_disc`).
    Disc,
    Options,
    /// The game's RetroAchievements (pocket-core games, signed in).
    Achievements,
    /// The quick settings of the shelf (brightness, volume, radios, power) over the paused game.
    QuickSettings,
    /// The graded chain of the game's picture and sound (`app_pixel_path`), when switched on in Settings.
    PixelPath,
    /// Which controller is which player (`app_players`), pocket-core games with two or more controllers.
    Players,
    /// Ends the game, with or without a new resume state (Settings > Remember where I quit, `without_saving`);
    /// in-game saves are written either way. The one Quit, as in NextUI's menu.
    Quit,
}

impl MainItem {
    fn label(self) -> &'static str {
        match self {
            MainItem::Continue => "Continue",
            MainItem::Save => "Save",
            MainItem::Load => "Load",
            MainItem::Earlier => "Earlier",
            MainItem::Disc => "Disc",
            MainItem::Options => "Options",
            MainItem::Achievements => "Achievements",
            MainItem::QuickSettings => "Quick settings",
            MainItem::PixelPath => "Pixel Path",
            MainItem::Players => "Players",
            MainItem::Quit => "Quit",
        }
    }
}

/// The main page for a pocket-core game (`hosted`) or any other emulator. Save and Load only where they do something
/// (`saves`, `pause_saves`: pocket-core's and PS2/GameCube/Wii/3DS's slots, RetroArch's one save); elsewhere the menu
/// says in a line where saving is. One Quit for all of them; whether it keeps a resume point is a setting, not a row
/// (`QuickMenu::without_saving`).
fn main_items(hosted: bool, saves: pocket_ui::pause_saves::Saves, achievements: bool) -> Vec<MainItem> {
    use MainItem::*;
    let mut items = vec![Continue];
    if saves.rows() {
        items.extend([Save, Load]);
    }
    // pocket-core's own slots have Earlier after Load (`app_earlier`; `QuickMenu::items` leaves it out without a row).
    if hosted && saves == pocket_ui::pause_saves::Saves::Slots {
        items.push(Earlier);
    }
    // Options is there for every emulator: pocket-core's and PS2/GameCube/Wii/3DS's own pages, else "Runs on"
    // (`app_emu_choice`: the emulator, and RetroArch's settings).
    items.push(Options);
    // pocket-core games with an account signed in list their achievements after Options.
    if hosted && achievements {
        items.push(Achievements);
    }
    items.extend([QuickSettings, Quit]);
    items
}

/// Whether the pause menu's Quit leaves out the resume point: Settings > Remember where I quit is off and the
/// emulator can (`quit::offers_quit_without_saving`). RetroArch's Quit saves either way.
fn quit_leaves_out_resume(remember_quit: bool, saver: quit::Saver) -> bool {
    !remember_quit && quit::offers_quit_without_saving(saver)
}

#[derive(Clone, Copy)]
enum SaveAction {
    Save,
    Load(Target),
    /// The state chosen on Earlier (`app_earlier`).
    Earlier,
}

/// NextUI's menu metrics (padding 10, pill 30, button 20 at its scale 1), scaled by 3 for this panel (`kit`).
const NU_PAD: i32 = pocket_ui::kit::PAD;
const NU_PILL: i32 = pocket_ui::kit::PILL;
const NU_BTN_PAD: i32 = pocket_ui::kit::BTN_PAD;
const NU_FONT: u32 = pocket_ui::kit::FONT;
const NU_SMALL: u32 = pocket_ui::kit::SMALL;
/// The hints of a pause-menu page's row with a value (the Picture page's rows: its preview leaves them the room).
const PICTURE_HINTS: &[(&str, &str)] = &[("B", "Back"), ("< >", "Change")];
/// How far the game is darkened behind the quick settings opened from the pause menu (the menu itself: 225).
const QUICK_OVER_GAME_FADE: u8 = 205;

struct App {
    proxy: EventLoopProxy<UserEvent>,
    started: Instant,
    window: Option<Rc<Window>>,
    surface: pocket_ui::frame::Presenter<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    text: Text,
    systems: Vec<System>,
    load_error: Option<String>,
    /// The cache of the game list: the scan after the first frame, a list waiting to be swapped in (`app_library`).
    library_ui: app_library::LibraryUi,
    recents: Recents,
    screen: Screen,
    items: Vec<Item>,
    sel: usize,
    prev_sel: usize,
    moved_at: Instant,
    /// The Home item to return to from a game shelf; found again by identity because recents can shift Home.
    home_item: Option<Item>,
    held: Option<(Button, Instant)>,
    pad: PadState,
    exit_since: Option<Instant>,
    running: Option<Running>,
    launch_at: Option<Instant>,
    drew_first: bool,
    /// The first frames after the Pocket started fade in from black (`boot_fade`, docs/BOOT-SPLASH.md).
    boot_fade: pocket_ui::boot_fade::BootFade,
    /// `after_first_frame` ran.
    after_first_done: bool,
    data_dir: PathBuf,
    theme: Theme,
    /// Settings > Look > Systems on the shelf: system ids the shelf and the switcher leave out.
    hidden: Vec<String>,
    toast: Option<(String, Instant)>,
    /// Settings → System → Sleep & power while it is open (`app_sleep_power`).
    sleep_page: pocket_ui::sleep_settings::Page,
    /// Settings → System → About: its selected row and the report's sheet (`app_report`).
    about: app_report::AboutUi,
    /// "Switched off after 3 h asleep…", to say on the shelf when the game the start resumed ends (`app_auto_off`).
    after_game_note: Option<String>,
    /// The TV the picture is on through USB-C, while it is (`app_display_out`).
    tv: Option<pocket_ui::display_out::Tv>,
    /// Why the last game could not start, from pocket-core.
    notice: Option<Notice>,
    /// Games that run through RetroArch because pocket-core couldn't start them.
    fallback: pocket_ui::fallback::Fallback,
    /// SELECT was pressed alone; releasing it opens the game switcher unless START joined in.
    select_alone: bool,
    /// START pressed and let go on its own opens Settings.
    start_tap: StartTap,
    /// Game switcher (SELECT on the shelf, HOME + SELECT in a game) and what it chose in a game.
    switcher: Option<pocket_ui::switcher::Switcher>,
    switch_to: Option<(String, String, PathBuf)>,
    /// The hand-over between two pocket-core games in the switcher (`app_fast_switch`).
    fast: app_fast_switch::FastSwitch,
    /// The picture for behind the pause menu asked of pocket-core, and its darkened copy (`app_menu_open`).
    menu_shot: app_menu_open::MenuShot,
    /// pocket-ui just put itself in front of a game: frames wait for the full size (`app_menu_open`).
    raise: app_menu_open::Raise,
    /// When the last frame was presented; animations pace themselves from it.
    frame_at: Instant,
    /// HOME is held in a game (HOME + SELECT opens the switcher).
    home_down: bool,
    favorites: pocket_ui::favorites::Favorites,
    /// The selected row's title scrolls when it is too long.
    marquee: pocket_ui::marquee::Marquee,
    menu: Option<QuickMenu>,
    /// After Quit: when to stop the game if it hasn't closed.
    quit: Option<Escalation>,
    /// Options of a PS2, GameCube, Wii or 3DS game being applied by a restart.
    reapply: Option<Reapply>,
    setup: Option<Setup>,
    settings_page: Option<SettingsPage>,
    updates: Updates,
    update_url: Option<String>,
    /// Settings → Store; read after the first frame (`startup::Later`), like the other pages' state below.
    store: pocket_ui::startup::Later<store_page::StorePage>,
    /// Settings → Games → Starter pack and Home's hint on an empty card (`app_starter`).
    starter: app_starter::StarterUi,
    /// Exit code for main once the event loop ends (`update::RESTART_CODE` after an update).
    exit_code: i32,
    onboarded: bool,
    es_systems: PathBuf,
    /// Scaled box art per (file, max width, max height); `None` = no cover for that game.
    covers: std::collections::HashMap<(PathBuf, i32, i32), Option<Rc<Image>>>,
    /// The console picture of a system card, fitted per (system, max width, max height).
    art: std::collections::HashMap<(String, i32, i32), Option<Rc<Sprite>>>,
    /// HOME is down in a game with no other button yet; releasing it opens the quick menu.
    home_tap: bool,
    /// The manual's reading position (Settings > Manual) and a held Up/Down waiting to start scrolling by itself.
    manual: pocket_ui::manual::Reader,
    manual_hold: Option<(Button, Instant)>,
    /// The manual was opened with SELECT from the first-start setup, which B goes back to.
    manual_from_setup: bool,
    /// Box art downloaded in the background while online (`pocket_ui::auto_covers`).
    cover_run: pocket_ui::auto_covers::AutoCovers,
    /// The game card's descriptions loaded ahead while online, under the same rules (`pocket_ui::auto_texts`).
    text_run: pocket_ui::auto_texts::AutoTexts,
    /// The games recognised by their checksum, before the covers and descriptions (`app_identify`).
    identify: app_identify::Identify,
    /// The look step's previews, one per theme (`setup::LOOK_PREVIEW`), and whether they show the built-in
    /// sample games; made when the step opens, dropped when what they show changes.
    look_previews: Option<(Vec<Image>, bool)>,
    /// While the previews are painted: the sample game's ROM path and its box art.
    sample_cover: Option<(PathBuf, Rc<Image>)>,
    /// The power menu paused the running game itself (opened its pause menu): closing the power menu
    /// puts the game back, and Sleep leaves the pause menu first so the game goes on after waking.
    power_paused_game: bool,
    /// Light and deep sleep, the power key, power off (`src/sleep.rs`).
    sleep: pocket_ui::sleep::Sleep,
    /// Volume keys and their combos, the OSD, quick settings (HOME on the shelf), the time page.
    sys: Sys,
    /// What the last frame painted where, for taps (`pocket_ui::touch`), and the swipe in progress.
    hits: touch::HitMap,
    /// The dialog before leaving for EmulationStation (SELECT + START held, Escape, Settings → About), over everything.
    leave: Option<pocket_ui::rescue::Dialog>,
    touch_rec: touch::Recognizer,
    /// Why touches were ignored last, so the log says it once per change (`app_touch`).
    ignore_log: touch::IgnoreLog,
    /// Settings > Sounds.
    sound: pocket_ui::ui_sound::Config,
    /// Settings > LEDs (`app_leds.rs`).
    leds: pocket_ui::leds::Config,
    /// Settings > Night light: the choices as saved, and whether the Pocket was asleep at the last pass (a wake is
    /// a reason to look at the clock again, `app_night_light.rs`).
    night: pocket_ui::night_light::Prefs,
    night_asleep: bool,
    /// The pad event being handled was a volume step (`quick::Out::volume_step`).
    volume_step: bool,
    /// The game card over the shelf (`app_card`).
    card: Option<app_card::OpenCard>,
    /// "Copy your game files", opened instead of starting a port whose files are missing (`app_ports`).
    port_files: Option<(pocket_ui::port_files::FilesPage, pocket_ui::port_files::Share)>,
    /// A port's Performance & fan page, and the running port's choice (`app_port_perf`).
    port_perf: app_port_perf::PortPerfUi,
    /// The pause menu's battery line for the game ("About 3 h 20 min left in this game"), made when it opens
    /// (`battery_games`); None without a battery reading.
    menu_battery: Option<String>,
    /// The console page over the shelf (`app_console`).
    console: Option<app_console::OpenConsole>,
    /// Play status, notes and session counts (`library.json`).
    library: pocket_ui::library_meta::LibraryMeta,
    /// Offline game facts from Wikidata, and what each file matched (None: no entry).
    gamedb: pocket_ui::gamedb::GameDb,
    game_infos: std::collections::HashMap<PathBuf, Option<pocket_ui::gamedb::GameInfo>>,
    /// Settings > Sync and the notice after a game (`app_sync.rs`).
    sync: pocket_ui::startup::Later<app_sync::SyncUi>,
    /// Settings > Tailscale (`app_tailscale.rs`).
    tailscale: pocket_ui::startup::Later<app_tailscale::TailscaleUi>,
    /// Settings > Web companion and its server (`app_web.rs`).
    web: pocket_ui::startup::Later<app_web::WebUi>,
    /// Settings > Bluetooth (`app_bluetooth.rs`).
    bluetooth: pocket_ui::startup::Later<app_bluetooth::BluetoothUi>,
    /// Settings > Performance & fan, and the fan outside games (`app_perf_fan.rs`).
    perf: app_perf_fan::PerfFanUi,
    /// Settings > Tools (`app_tools.rs`).
    tools: app_tools::ToolsUi,
    /// Settings > PC streaming (`app_moonlight.rs`).
    moonlight: app_moonlight::MoonlightUi,
    /// Settings > Backup & reset (`app_backup.rs`).
    backup: app_backup::BackupUi,
    /// Settings > System > Storage, the low-space notice, the card's Hide or delete (`app_storage.rs`).
    storage: app_storage::StorageUi,
    /// EmulationStation's gamelists: what they hold, the covers from them, the page (`app_gamelist.rs`).
    gamelist: app_gamelist::GamelistUi,
    /// The screenshot gallery (`app_gallery.rs`).
    gallery: app_gallery::GalleryUi,
    /// When the wrong-clock look is due (`app_clock`); None once it is done.
    clock_look: Option<Instant>,
    /// The "Runs on" page while it is open (`app_emu_choice.rs`).
    emu: app_emu_choice::EmuChoiceUi,
    /// The "This game" page over a game's card while it is open (`app_this_game.rs`).
    this_game: Option<pocket_ui::this_game_page::ThisGamePage>,
    /// RetroAchievements: the account page and the card lookups (`app_achievements.rs`).
    achievements: pocket_ui::startup::Later<app_achievements::AchievementsUi>,
    /// Moonlight is installed (`pocket_ui::moonlight`); "PC streaming" is on Home only when also ticked in
    /// Settings > Look > Systems on the shelf (`streaming_on`, off by default).
    streaming: bool,
    streaming_on: bool,
    /// Screenshots and title screens beside the box art: the switch, what is known about the files, the cycle of
    /// the Split preview and the thread that decodes them (`app_pictures.rs`).
    pictures: app_pictures::PicturesUi,
    /// Settings > Games > Extras > Pictures is Off: no box art, screenshots or title screens are fetched by themselves
    /// (`auto_covers`, `identify`); the Cover art row still looks up what is asked for.
    art_off: bool,
    /// Settings > Look > Appearance, where the palette is between dark and light, when Automatic last looked
    /// at the clock and whether it waits for a quiet moment to switch (`app_appearance`).
    appearance: Appearance,
    fader: pocket_ui::appearance::Fader,
    appearance_checked: Instant,
    appearance_pending: bool,
    /// The last button press or touch, so Automatic never switches under the player's hands.
    last_input: Instant,
    /// The look previews were painted light.
    look_previews_light: bool,
    /// The language the look previews were painted in.
    look_previews_lang: pocket_ui::i18n::Lang,
    /// And at which text size (R1 on the first start's look step changes it).
    look_previews_size: pocket_ui::type_scale::TextSize,
    /// The column a page of rows and text is painted into on a wide screen, kept between frames (`app_column`).
    column_buf: Vec<u32>,
    /// Settings → Language as chosen; `None` follows the system's (`i18n::Lang::system_default`).
    language: Option<pocket_ui::i18n::Lang>,
    /// The Cabinet look's shelves, animations and pictures (`app_cabinet`).
    cabinet: app_cabinet::CabinetUi,
    /// Curator: the index, its pages, smart shelves (`app_curator`).
    curator: app_curator::CuratorUi,
    /// Settings > On start (`pocket_ui::on_start`).
    on_start: pocket_ui::on_start::OnStart,
    /// Settings > Remember where I quit (settings.json's `forget_where_quit`, the other way round): what the pause
    /// menu's one Quit does where the emulator can leave out the resume point (`QuickMenu::without_saving`).
    remember_quit: bool,
    /// Settings → Look & feel → Animations once chosen; unset follows the look (`App::cozy`).
    animations: Option<pocket_ui::settings::Animations>,
    /// X on a game opens its box (`game_box`), not the page; SELECT on the card switches and is remembered.
    game_box: bool,
    /// Settings > Playing > Button layout (`pocket_ui::button_layout`; the pad threads read it from there).
    button_layout: pocket_ui::button_layout::ButtonLayout,
    /// Settings → Curator → Pixel Path in the pause menu (`Prefs::pixel_path`, off by default), kept in step by the
    /// Curator page; the pause menu reads only this.
    pixel_path: bool,
    /// The picture of where the starting game was left (`pocket_ui::launch_picture`), until the game ends.
    launch_picture: Option<pocket_ui::launch_picture::Prepared>,
    /// The game search and the letter jump (`app_search`).
    find: app_search::FindUi,
    /// Page transitions, list glides and the hint bar's crossfade (`app_motion`).
    motion: app_motion::MotionUi,
    /// Settings → Screen frames (`settings::Frames`); games read it from settings.json (`options::GameSettings`).
    frames: Frames,
    /// Settings → Playing → Over the game (`over_game`); games read it from settings.json (`options::GameSettings`).
    over_game: pocket_ui::over_game::Level,
    /// Settings → Tour and "New in this version" (`app_tour`).
    tour: app_tour::TourUi,
    /// The last frame had the status corner: when its clock turns to the next minute (`app_status_corner`).
    corner_minute: Option<Instant>,
}

impl App {
    /// A recent game Home, the switcher and On start show: its file is there, its system is on the shelf, and the
    /// player did not hide it (Hide or delete on the card, `app_storage`).
    fn recent_visible(&self, r: &recents::Recent) -> bool {
        r.path.exists() && shelf::recent_shown(&r.system, &self.hidden) && !self.library.is_hidden(&r.path)
    }

    /// Home's list: the recent games, Favorites, PC streaming, the systems.
    fn home_items(&self) -> Vec<Item> {
        // Settings > Look > Systems on the shelf: a hidden system's card and its recent games stay off Home.
        let recents = self.recents.list.iter().enumerate().filter(|(_, r)| self.recent_visible(r)).map(|(i, _)| i);
        let systems = shelf::visible_systems(self.systems.iter().map(|s| s.name.as_str()), &self.hidden);
        let home = Item::with_apps(Item::home(recents.take(RECENTS_ON_HOME), !self.favorite_items().is_empty(), systems), self.streaming && self.streaming_on);
        let home = Item::with_curator(home, self.curator_home_rows(), self.theme != Theme::Cabinet);
        // "Search" above Continue in the list looks (`app_search`); the cabinet opens it with Up on its top shelf.
        Item::with_search(home, self.theme != Theme::Cabinet && self.systems.iter().any(|s| !s.games.is_empty()))
    }

    fn rebuild(&mut self) {
        self.items = match self.screen {
            Screen::Home => self.home_items(),
            Screen::Games(s) => (0..self.systems[s].games.len()).map(|g| Item::Game(s, g)).collect(),
            Screen::Favorites => self.favorite_items(),
            screen => self.curator_items(screen),
        };
        self.sel = self.sel.min(self.items.len().saturating_sub(1));
        self.prev_sel = self.sel;
        self.cabinet.dirty = true;
    }

    fn select(&mut self, to: usize) {
        if to != self.sel {
            self.prev_sel = self.sel;
            self.sel = to;
            self.moved_at = Instant::now();
            self.marquee.restart();
        }
    }

    fn step(&mut self, b: Button) {
        // L2/R2: the previous/next first letter (`app_search`), in every look.
        if matches!(b, Button::L2 | Button::R2) {
            return self.letter_jump(b == Button::R2);
        }
        if self.theme == Theme::Cabinet {
            return self.cabinet_step(b);
        }
        let last = self.items.len().saturating_sub(1);
        match b {
            Button::Up => self.select(self.sel.saturating_sub(1)),
            Button::Down => self.select((self.sel + 1).min(last)),
            Button::L1 => self.select(self.sel.saturating_sub(5)),
            Button::R1 => self.select((self.sel + 5).min(last)),
            _ => {}
        }
    }

    fn press(&mut self, b: Button) {
        // Up next after a game (`app_curator`): A plays it, B puts it away, any other button puts it away and goes on.
        if self.curator_up_next_press(b) {
            return;
        }
        // A on Home while the card has no games: the starter pack (`app_starter`).
        if self.starter_home_press(b) {
            return;
        }
        // Up on the cabinet's top shelf opens the search, which the list looks have as a row above Continue.
        if self.theme == Theme::Cabinet && b == Button::Up && self.search_hint().is_some() {
            return self.open_search();
        }
        // The cabinet moves in two directions (`app_cabinet`).
        if self.theme == Theme::Cabinet && self.cabinet_press(b) {
            return;
        }
        // The card carousel runs sideways; left and right move like up and down everywhere.
        let b = match b {
            Button::Left => Button::Up,
            Button::Right => Button::Down,
            other => other,
        };
        match b {
            Button::Select => self.select_alone = true,
            Button::Start => self.select_alone = false,
            Button::Up | Button::Down | Button::L1 | Button::R1 | Button::L2 | Button::R2 => {
                self.step(b);
                self.held = Some((b, Instant::now() + REPEAT_DELAY));
            }
            b if b == CONFIRM => self.activate(),
            // Y (the left face button) stars or unstars a game; on a smart shelf or sticker it pins it to Home.
            Button::West => {
                if !self.curator_y() {
                    self.toggle_favorite();
                }
            }
            // X (the top face button) opens the game's card, or the console page on a system; a shelf's options.
            Button::North => {
                if !self.curator_x() {
                    self.open_details();
                }
            }
            b if b == CANCEL => {
                if self.curator_back() {
                    return;
                }
                if self.screen != Screen::Home {
                    self.screen = Screen::Home;
                    self.rebuild();
                    let back_to = self.home_item.and_then(|item| self.items.iter().position(|i| *i == item));
                    self.sel = back_to.unwrap_or(0);
                    self.prev_sel = self.sel;
                }
            }
            _ => {}
        }
    }

    fn activate(&mut self) {
        match self.items.get(self.sel).copied() {
            Some(Item::System(s)) => {
                self.home_item = Some(Item::System(s));
                self.screen = Screen::Games(s);
                self.sel = 0;
                self.rebuild();
            }
            Some(Item::Game(s, g)) => {
                let (sys, game) = (&self.systems[s], &self.systems[s].games[g]);
                self.launch(sys.name.clone(), game.name.clone(), game.path.clone());
            }
            Some(Item::Recent(i)) => {
                let r = self.recents.list[i].clone();
                self.launch(r.system, r.name, r.path);
            }
            Some(Item::Favorites) => {
                self.home_item = Some(Item::Favorites);
                self.screen = Screen::Favorites;
                self.sel = 0;
                self.rebuild();
            }
            Some(Item::Streaming) => self.start_streaming(),
            Some(item @ (Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_))) => self.curator_activate(item),
            Some(Item::Search) => self.open_search(),
            None => {}
        }
    }

    /// "PC streaming": Moonlight as an app. Same process group, window and return as a game, but it is not a
    /// game: no pause menu, no recents, nothing saved or resumed.
    fn start_streaming(&mut self) {
        self.start_streaming_with(pocket_ui::moonlight::start(), "PC streaming".into());
    }

    /// `start_streaming` with this Moonlight command: one of the PC's games from Settings > PC streaming (`app_moonlight`).
    fn start_streaming_with(&mut self, cmd: std::process::Command, name: String) {
        println!("launch PC streaming (moonlight): {name}");
        #[cfg(target_os = "linux")]
        control::rules_in_place();
        pocket_ui::ui_sound::stop();
        self.cover_run.game_started();
        self.identify.game_started();
        self.perf_fan_before_game();
        let pgid = spawn_app(self.proxy.clone(), cmd);
        control::note_game(pgid);
        let emulator = pocket_ui::moonlight::EMULATOR.to_string();
        self.running = Some(Running { system: emulator.clone(), name, path: PathBuf::new(), emulator, started: Instant::now(), pgid, options: Default::default() });
        self.launch_at = Some(Instant::now());
        self.held = None;
    }

    /// At start: the game of the resume note, or the last game when Settings > On start says so and nothing speaks
    /// against it (`pocket_ui::on_start::decide`).
    #[cfg(target_os = "linux")]
    fn start_game_at_start(&mut self, note: Option<pocket_ui::sleep::Resume>) {
        use pocket_ui::on_start::{self, Decision, Facts};
        let first = on_start::first_start_this_boot(&self.data_dir, pocket_ui::logs::boot_id().as_deref());
        let last = on_start::last_game(&self.data_dir);
        // The first recent game Home shows (`home_items`).
        let recent = self.recents.list.iter().find(|r| self.recent_visible(r)).cloned();
        let facts = Facts {
            setting: self.on_start,
            resume_note: note.is_some(),
            onboarded: self.onboarded,
            first_start_this_boot: first,
            crash_notice: self.notice.is_some(),
            last_game: last.as_ref(),
            recent: recent.as_ref().map(|r| r.path.as_path()),
        };
        match (on_start::decide(&facts), note, recent) {
            (Decision::Resume, Some(r), _) => {
                println!("resuming {} after power off or sleep", r.path.display());
                self.launch(r.system, r.name, r.path);
            }
            (Decision::Continue, _, Some(r)) => {
                println!("on start: continuing {}", r.path.display());
                self.launch(r.system, r.name, r.path);
            }
            (Decision::Home(why), _, _) if self.on_start == on_start::OnStart::Continue => println!("on start: Home, {why}"),
            _ => {}
        }
    }

    fn launch(&mut self, system: String, name: String, path: PathBuf) {
        // The moment the game was asked for; pocket-core logs its first frame against it (`startup`).
        let asked = pocket_ui::startup::clock_ms();
        // A PortMaster port whose game files are surely missing: what to copy where, instead of a start into nothing.
        if system == "ports" && self.port_needs_files(&path) {
            return;
        }
        // A game whose file is gone since the list was read (deleted over the share, a list from the cache that the
        // scan after the start hasn't corrected yet): a clear word instead of an emulator failing on nothing, and the
        // folders are read again (`app_library`).
        if let Some(msg) = pocket_ui::library_cache::missing_game(&name, &path) {
            println!("launch: {} is missing", path.display());
            self.notice = Some(Notice::new(msg));
            self.library_rescan("a game's file was missing");
            return;
        }
        // Started from the search: it closes, and the shelf goes to the game (`app_search`).
        self.search_launching(&path);
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else { return };
        let (emulator, core) = self.route(sys, Some(&path), std::path::Path::new(HOST_BIN).exists());
        // A game still ending after a hand-over (`app_fast_switch`) ends first, unless this one may start next to it.
        self.wait_for_drained(&system, &path, &emulator, &core);
        // The game's window opens fullscreen only once sway has the rules (set on a thread at start).
        #[cfg(target_os = "linux")]
        control::rules_in_place();
        println!("launch {system} {} (emulator {emulator}, core {core})", path.display());
        // The game gets the audio device to itself, and the CPU and network: a cover download stops. A pocket-core game
        // keeps the shelf's quantum (it sets the same one), anything else gets ROCKNIX's.
        pocket_ui::ui_sound::stop_for(emulator != "pocket-core");
        // A port's own performance and fan (`app_port_perf`, from memory), before the fan's plan for the game is made.
        self.port_perf_before_launch(&system, &path);
        // And the system's fan profile, before anything of the game runs (`app_perf_fan.rs`).
        self.perf_fan_before_game();
        self.cover_run.game_started();
        self.text_run.game_started();
        self.identify.game_started();
        // GameCube and Dolphin defaults in system.cfg are in place (put off at start until the first frame).
        #[cfg(target_os = "linux")]
        emu_settings::apply_start_defaults_once();
        standalone::launch(&system, &emulator, &core, &path, &self.data_dir);
        emu_settings::launch(standalone::session().map(|s| s.emu), &system, &path);
        // Pixel Path's first reading of the emulator's settings, on a thread once the start script is done.
        if self.pixel_path {
            pocket_ui::pixel_standalone::prefetch();
        }
        // The game's saves stay out of Syncthing while it runs (`app_sync.rs`).
        self.sync_game_started(&system);
        let mut env = emu_settings::launch_env(&system, &path);
        env.push((pocket_ui::startup::LAUNCHED_AT, format!("{asked:.1}")));
        // The case goes into its console while the game loads (Cabinet and Shelf looks, `app_cabinet`). In Cozy a
        // pocket-core game keeps its window back until the animation has been seen.
        pocket_ui::motion::set_cozy(self.cozy());
        if let Some(hold) = self.launch_animation(&path, emulator == "pocket-core") {
            env.push((pocket_ui::startup::HOLD_UNTIL, format!("{:.1}", asked + hold.as_secs_f64() * 1000.0)));
        }
        // Smooth fan: a pocket-core game tells pocket-ui its fan choice instead of changing the trips (`app_perf_fan`).
        env.extend(self.perf_fan_game_env().filter(|_| emulator == "pocket-core"));
        pocket_ui::mark!("launch", "{system} {}", path.display());
        // pocket-core games get our display (the overlay); RetroArch, the standalone emulators and apps get ROCKNIX's box.
        pocket_ui::mako_osd::set_hidden(emulator == "pocket-core");
        let pgid = spawn_game(self.proxy.clone(), &system, &emulator, &core, &path, &env);
        // A port: its performance once its own process runs, and the busy-core look (`app_port_perf`, on a thread).
        self.port_perf_started(pgid, &path);
        println!("launch: game process started {:.1} ms after it was asked for", pocket_ui::startup::clock_ms() - asked);
        // How it ends decides whether Settings > On start may continue it (`on_start`).
        pocket_ui::on_start::note_started(&self.data_dir, &path);
        // Play history: the session's start, written off this thread (`play_log`).
        pocket_ui::play_log::started(&system, &name, &path, &emulator, &core);
        // Where the game was left, faded in while it starts; read on a thread once the game's process is on its way.
        self.launch_picture = None;
        {
            let (proxy, data_dir, system, path) = (self.proxy.clone(), self.data_dir.clone(), system.clone(), path.clone());
            let hosted = emulator == "pocket-core";
            let (w, h) = self.screen_size();
            std::thread::spawn(move || {
                let picture = hosted.then(|| pocket_ui::video::VideoSettings::load(&data_dir, &system, &path)).map(|v| (v.scaling, v.orientation.turns()));
                let _ = proxy.send_event(UserEvent::LaunchPicture(pocket_ui::launch_picture::prepare(&data_dir, &system, &path, picture, w, h)));
            });
        }
        control::note_game(pgid);
        self.leds_game_started(&system);
        self.sleep.game_started(pocket_ui::sleep::Game::new(&system, &name, &path, &emulator, pgid));
        pocket_ui::battery_history::set_activity(Some(&system));
        // The battery per game: this game's stretches on battery go to its ledger (`battery_games`).
        pocket_ui::battery_history::set_game(Some(pocket_ui::battery_games::Key::new(&system, &path, &emulator)));
        self.running = Some(Running { system, name, path, emulator, started: Instant::now(), pgid, options: Default::default() });
        self.launch_at = Some(Instant::now());
        self.held = None;
    }

    /// The emulator and core that play `sys`'s games: pocket-core for the systems it hosts (when `host_bin`, it is
    /// installed), else es_systems.cfg's default. `path`: for one game, which may stay elsewhere (a 7z archive, a file
    /// pocket-core doesn't take, a game moved to RetroArch after a failed start); None: the system as a whole.
    /// The player's choice (per game, then per console, in system.cfg as EmulationStation keeps it) comes first
    /// (`app_emu_choice::resolve_runner`, `emu_choice::resolve`).
    fn route(&self, sys: &System, path: Option<&std::path::Path>, host_bin: bool) -> (String, String) {
        let r = self.resolve_runner(sys, path, host_bin).runner;
        (r.emulator, r.core)
    }

    fn game_exited(&mut self, secs: u64, ok: bool) {
        // Asked to stop (Quit in the menu, or the grace period ran out) is no failure.
        let quit_requested = self.menu.as_ref().is_some_and(|m| m.quitting) || self.quit.is_some();
        // Options applied by a restart: the emulator closed after writing the resume state.
        let reapply = self
            .reapply
            .take()
            .filter(|_| self.running.is_some())
            .and_then(|mut r| (r.restart.on(emu_settings::Event::Exited) == Some(emu_settings::Cmd::Relaunch)).then_some(r));
        let Some(run) = self.running.take() else { return };
        pocket_ui::fast_switch::mark("game exit handled");
        pocket_ui::mark!("game-exited", "{} {}", run.system, run.path.display());
        pocket_ui::battery_history::set_activity(None);
        pocket_ui::battery_history::set_game(None);
        // A port's performance goes back; its fan leaves the plan (`app_port_perf`).
        self.port_perf_ended();
        self.leds_game_ended();
        if run.is_app() {
            return self.app_exited(run, secs, ok);
        }
        control::note_game(None);
        self.sleep.game_ended();
        // Not after a hand-over to the next game (`app_fast_switch`): it starts now and wants the same quantum. After
        // one to the shelf, the old process leaves it to us.
        if !self.quantum_stays() {
            // pocket-core lowers PipeWire's quantum while playing; make sure it is back even if it crashed, and the
            // shelf's own quantum set again before its first sound, after any game (`ui_sound::game_ended`).
            pocket_ui::ui_sound::game_ended();
        }
        println!("game exited after {secs} s (measured {:?})", run.started.elapsed());
        // An error nobody asked for: Settings > On start doesn't start this game by itself next time.
        pocket_ui::on_start::note_ended(&self.data_dir, &run.path, !ok && !quit_requested);
        // pocket-core's reason for not starting, else a save that didn't reach the card at the end (`save_trouble`).
        self.notice = if run.hosted() { Notice::take_after_game() } else { None };
        // The start resumed this game after a switch-off after sleeping: why it was off, now on the shelf.
        self.switched_off_after_game();
        // A run that failed at once never put the game on the screen (`fallback::never_started`): it is no play
        // session, and when no one else explains it (pocket-core's own reason, the fallback's notice) pocket-ui does.
        // A port's script exits 0 whatever happened, so for ports the exit code doesn't count (`port_never_started`).
        let port = run.system == "ports";
        let never_started = if port { pocket_ui::fallback::port_never_started(secs, quit_requested) } else { pocket_ui::fallback::never_started(ok, secs, quit_requested) };
        if never_started && port {
            self.notice = Some(Notice::new(pocket_ui::fallback::port_not_started_notice(&run.name)));
        } else if never_started && !run.hosted() {
            // A system whose BIOS pocket-ui knows: the files missing are named, not guessed at (`hostsys::missing_bios`).
            let bios = pocket_ui::hostsys::find(&run.system).and_then(|s| s.missing_bios(std::path::Path::new(pocket_ui::hostsys::BIOS_DIR)));
            self.notice = Some(Notice::new(pocket_ui::fallback::not_started_notice(&run.name, bios.as_deref())));
        }
        // pocket-core couldn't start this game: remember it and start it with the system's default emulator instead.
        let failed = run.hosted() && pocket_ui::fallback::should_retry(ok, secs, quit_requested, self.notice.as_ref().map(|n| n.text()));
        // What starts it from now on: the next choice down (`emu_choice::resolve`), as the retry below.
        let default = if failed { self.fallback.add(&run.path); self.systems.iter().find(|s| s.name == run.system).map(|s| self.route(s, Some(&run.path), true).0).unwrap_or_default() } else { String::new() };
        // A system with no other emulator than pocket-core (its core is missing): the next choice is pocket-core again, which
        // failed again, for ever (1,483 starts in two minutes, 2026-10-29). Say what happened instead.
        let retry = failed && default != "pocket-core";
        if retry {
            println!("pocket-core couldn't start {} (exit ok {ok}, {secs} s): {default} from now on", run.path.display());
            self.notice = Some(Notice::new(pocket_ui::fallback::notice(&default)));
        } else if failed {
            self.fallback.remove(&run.path);
            println!("pocket-core couldn't start {} (exit ok {ok}, {secs} s): no other emulator for {}", run.path.display(), run.system);
            if self.notice.is_none() {
                self.notice = Some(Notice::new(pocket_ui::fallback::core_missing_notice(&run.name)));
            }
        }
        self.keep_last_frame(&run);
        // Written to the card at the end, after a game chosen in the switcher has been started (each syncs).
        // Not for a game that never started: it would be "Continue" on the shelf and the first card of the switcher.
        if !never_started {
            self.recents.note(&run.system, &run.name, &run.path, secs);
            // Play history: the session's end, as the recent games count it; written off this thread (`play_log`).
            pocket_ui::play_log::ended(&run.system, &run.name, &run.path, secs);
        } else {
            // Its start is in the play log too: closed with no time, it is no session (left open, it counted as one
            // "without time" in Curator → History).
            pocket_ui::play_log::never_started(&run.system, &run.name, &run.path);
        }
        let session = !never_started && self.library.note_session(&run.path, secs);
        self.curator.touch();
        // Back on the shelf at once; Syncthing gets asked on a thread of its own (`app_sync.rs`). Not when the game
        // starts again to apply its options: its saves stay held until it really ends.
        if !retry && reapply.is_none() {
            self.sync_after_game(&run.path);
        }
        if run.hosted() {
            self.restore_system_then_fan();
        }
        // Back on the shelf (or the next game's start says otherwise right after): our display again.
        pocket_ui::mako_osd::set_hidden(true);
        standalone::exited();
        self.launch_at = None;
        self.end_insert();
        self.launch_picture = None;
        self.menu = None;
        self.home_tap = false;
        self.quit = None;
        // The game ended under an open power menu (it crashed, or is quitting for a restart).
        self.power_paused_game = false;
        if self.sys.power_menu_open() {
            self.sys.close();
        }
        // After a hand-over or pocket-core's own quit pocket-ui is in front already (the pause menu, the switcher):
        // no swaymsg before the shelf's first frame (`app_fast_switch::ui_in_front`).
        if !self.ui_in_front() {
            control::show_ui();
            self.raised();
        }
        // Stick movements were thinned out while the game ran (`idle::StickThrottle`); the controllers stay known.
        self.pad.let_go();
        self.rebuild();
        if self.screen == Screen::Home {
            self.sel = Item::first_row(&self.items);
            self.prev_sel = self.sel;
        }
        if retry {
            self.launch(run.system, run.name, run.path);
        }
        let relaunched = reapply.is_some();
        if let Some(mut r) = reapply {
            let (system, name, path) = r.game.clone();
            println!("options: starting {} again to apply them", path.display());
            self.toast = Some(("Applying options…".into(), Instant::now()));
            self.launch(system, name, path);
            // LoadResume: the start loads the resume state (standalone.rs, as on every start after Quit).
            let _ = r.restart.on(emu_settings::Event::Started);
        }
        self.after_game_for_switcher(retry || relaunched);
        // On the card writer: each list is four fsyncs, which held the shelf's first frame (`fsutil::write_json_soon`).
        self.recents.save_soon();
        if session {
            self.library.save_sessions_soon();
        }
        pocket_ui::trace::step("game exit: shelf ready");
        // HOME + A in the game may have added screenshots (`app_gallery`): a read on a thread.
        self.gallery_refresh("after a game");
        // A cover download (or the checksums before it, `app_identify`) the game stopped goes on.
        if !relaunched && (self.cover_run.resume_due() || self.identify.due()) {
            self.auto_covers("after a game");
        }
        if !relaunched && self.text_run.resume_due() {
            self.auto_texts("after a game");
        }
        // The game wrote saves and states: is the card nearly full now? (a thread, `app_storage`)
        if !relaunched && !retry {
            self.storage_check("after a game");
        }
    }

    /// The fade into PC streaming has ended and its message is yet to be drawn: one more frame after the fade.
    fn stream_message_due(&self) -> bool {
        self.running.as_ref().is_some_and(Running::is_app) && self.launch_at.is_some_and(|t| t.elapsed() < FADE + Duration::from_millis(100))
    }

    /// PC streaming or a tool ended: back to the shelf (or to Settings > Tools, which stayed open behind the tool), with
    /// a word when it never got going.
    fn app_exited(&mut self, run: Running, secs: u64, ok: bool) {
        control::note_game(None);
        self.sleep.game_ended();
        // The shelf's quantum again before its first sound (`ui_sound::game_ended`).
        pocket_ui::ui_sound::game_ended();
        println!("{} exited after {secs} s (ok {ok}, measured {:?})", run.name, run.started.elapsed());
        let tool = run.emulator == pocket_ui::tools::EMULATOR;
        let why = if tool {
            pocket_ui::tools::never_started(&run.name, secs, ok, self.quit.is_some())
        } else {
            pocket_ui::moonlight::never_started(secs, ok, self.quit.is_some()).map(String::from)
        };
        self.launch_at = None;
        self.end_insert();
        self.menu = None;
        self.home_tap = false;
        self.quit = None;
        control::show_ui();
        self.raised();
        // Stick movements were thinned out while the app ran; the controllers stay known.
        self.pad.let_go();
        self.rebuild();
        if let Some(at) = self.items.iter().position(|i| *i == Item::Streaming).filter(|_| !tool) {
            self.select(at);
        }
        // Started from Settings > Tools or PC streaming: that page again, with the word on it; from Home, a notice
        // (drawn over the shelf only) (`app_tools`).
        self.notice = self.settings_after_app(why).map(Notice::new);
        if self.cover_run.resume_due() {
            self.auto_covers(if tool { "after a tool" } else { "after PC streaming" });
        }
    }

    fn in_game_input(&mut self, pe: PadEvent) {
        // An app has no pause menu: the pad belongs to it (a tool can be stopped by holding HOME, `app_tools`).
        if self.running.as_ref().is_some_and(Running::is_app) {
            self.tool_home(&pe);
            return;
        }
        // The Mini V2's triggers are analog: HOME + L2/R2 is a shortcut of pocket-core, not a tap.
        if let PadEvent::Axis { axis: Axis::L2 | Axis::R2, value, .. } = pe {
            if value > 0.5 {
                self.home_tap = false;
            }
            return;
        }
        let PadEvent::Button { button, pressed, source } = pe else { return };
        if self.switcher_in_game_input(button, pressed) {
            return;
        }
        // The Players page needs to know which controller pressed (`app_players`).
        if pressed && self.menu.as_ref().is_some_and(|m| m.page == Page::Players && !m.quitting) {
            self.players_input(button, source);
            return;
        }
        if self.menu.is_some() {
            if pressed {
                self.menu_input(button);
            }
            return;
        }
        // HOME + another button is a hotkey (save, load, quit, the screenshot), not a tap: no menu on release.
        let (tap, open_menu) = pocket_ui::hotkeys::home_tap_step(self.home_tap, button, pressed);
        self.home_tap = tap;
        // HOME + A: a screenshot of the screen, for the games pocket-core does not host (`app_screenshot`). A is the
        // button printed A whatever the Button layout says (`button_layout::is_printed_a`), as in pocket-core.
        if pressed && self.home_down && pocket_ui::button_layout::get().is_printed_a(button) {
            self.screenshot_key();
        }
        if open_menu {
            self.open_menu();
        }
    }

    fn open_menu(&mut self) {
        self.emu_choice_menu_closed();
        let Some(run) = self.running.as_ref().filter(|r| !r.is_app()) else { return };
        // The game is on screen by now: its start picture is done with.
        self.launch_picture = None;
        let retroarch = run.emulator == "retroarch";
        let hosted = run.hosted();
        // grim and sway get ~400 ms in all (B10): if one hangs the menu opens without the paused frame
        // (or over the game without a window id), it doesn't freeze the UI.
        let helpers = pocket_ui::proc::Budget::new(control::MENU_HELPERS);
        let window = control::focused_window(helpers.cap(control::MENU_TREE));
        pocket_ui::trace::step("menu: window known");
        // pocket-core draws the picture behind the menu itself once the menu is up (`app_menu_open`); the others are
        // captured before pocket-ui covers them.
        let frame = if hosted { None } else { image::capture_screen(helpers.left().saturating_sub(control::MENU_SHOW)) };
        pocket_ui::trace::step("menu: screenshot taken");
        // pocket-core games and the standalone emulators have slots (`states.rs`); the standalones name theirs after
        // the disc, which standalone.rs learns once the game runs. RetroArch has its one slot.
        let ra_state = retroarch.then(|| run.state_file());
        let saved = ra_state.as_ref().and_then(|state| std::fs::metadata(state).and_then(|m| m.modified()).ok()).map(|t| {
            let state = ra_state.as_ref().unwrap();
            let mut preview = state.clone().into_os_string();
            preview.push(".png");
            (image::load_png(std::path::Path::new(&preview)), t)
        });
        let stem = run.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let states = if hosted {
            Some(StateSet::pocket_core(&slots::base(&run.system, stem)))
        } else if standalone::active() {
            standalone::states()
        } else {
            None
        };
        // The slot used last is selected for Save (pocket-core remembers it; the others start on 1).
        let slot = if hosted { slots::current(&self.data_dir, &run.system, stem) } else { 0 };
        let (filled, loads) = states.as_ref().map_or((Vec::new(), Vec::new()), |st| (st.filled(), st.load_targets()));
        let slot = slot.min(filled.len().saturating_sub(1));
        // Load starts on the auto state when there is one, so what Quit saved is what a Load finds first.
        let load_at = loads.iter().position(|t| *t == Target::Auto).or_else(|| loads.iter().position(|t| *t == Target::Slot(slot as u8 + 1))).unwrap_or(0);
        // pocket-core pauses itself on request and stays able to draw previews; every other
        // emulator but RetroArch (which pauses when it loses focus) is stopped outright.
        let frozen = run.pgid.filter(|g| !retroarch && !hosted && control::pause_game(*g));
        // pocket-core: paused before it is covered, and asked for the picture behind the menu (`app_menu_open`).
        let shot = hosted.then(|| app_menu_open::pause(run.pgid, self.proxy.clone())).flatten();
        let show = helpers.left().max(control::MENU_SHOW);
        match window {
            Some(con) => control::show_ui_over_within(con, show),
            None => control::show_ui_within(show),
        };
        self.raise.since = Some(Instant::now());
        pocket_ui::trace::step("menu: in front");
        // What the game uses now: the saved Options with this session's changes (`options::Session`).
        let mut settings = hosted.then(|| run.options.open(&self.data_dir, &run.system, &run.path));
        let emu = settings.as_mut().and_then(|s| EmuMenu::load(&self.data_dir, &run.system, &run.path, s));
        let cheats = hosted.then(|| CheatMenu::load(&self.data_dir, &run.system, &run.path)).flatten();
        let achievements = hosted.then(|| Self::menu_achievements(&run.path)).flatten();
        let emu_set = standalone::active().then(|| emu_settings::Menu::for_running(&Default::default())).flatten();
        let no_states = if hosted { hostsys::find(&run.system).and_then(|h| h.no_states) } else { standalone::no_states_line() };
        let saver = if hosted {
            quit::Saver::PocketCore { states: no_states.is_none() }
        } else if retroarch {
            quit::Saver::RetroArch
        } else if standalone::active() {
            quit::Saver::Standalone
        } else if run.system == pocket_ui::port_perf::SYSTEM {
            quit::Saver::Port
        } else {
            quit::Saver::Other
        };
        // Pixel Path, when switched on: a standalone game's only when its settings could be read (`app_pixel_path`).
        let pixel_item = self.pixel_item_at_open();
        // Save and Load only where they do something: a PS2 game whose states have no names yet can't save (`pause_saves`).
        let saves = pocket_ui::pause_saves::Saves::of(saver, states.is_some(), no_states);
        let disc = Self::disc_row_at_open(run);
        // Earlier: a few stats of known names (`app_earlier`).
        let earlier = app_earlier::row_at_open(run, saves, settings.as_ref());
        self.menu = Some(QuickMenu {
            sel: 0,
            frame,
            saved,
            states,
            slot,
            filled,
            loads,
            load_at,
            quitting: false,
            discarding: false,
            without_saving: quit_leaves_out_resume(self.remember_quit, saver),
            window,
            frozen,
            retroarch,
            hosted,
            page: Page::Main,
            settings,
            note: None,
            live: Default::default(),
            emu,
            cheats,
            emu_set,
            want_advanced: false,
            applying: false,
            emu_note: None,
            achievements,
            pixel_item,
            pixel: None,
            players_item: self.players_item(),
            players: None,
            saves,
            disc,
            earlier,
            save_trouble: Self::save_trouble_at_open(hosted),
        });
        self.menu_shot = app_menu_open::MenuShot::asked(shot);
        // The battery in this game, from memory (`battery_games`): microseconds, no file or sysfs read.
        self.menu_battery = self.running.as_ref().and_then(|r| {
            let key = pocket_ui::battery_games::Key::new(&r.system, &r.path, &r.emulator);
            pocket_ui::battery_history::game_left(&key).map(|l| l.line(&system_short(&self.systems, &r.system)))
        });
        pocket_ui::trace::step("menu: built");
    }

    fn menu_input(&mut self, b: Button) {
        let Some(menu) = self.menu.as_mut() else { return };
        if menu.quitting {
            return;
        }
        if menu.page == Page::PixelPath {
            self.pixel_path_input(b);
            return;
        }
        if menu.page == Page::EmuChoice {
            self.emu_choice_input(b);
            return;
        }
        if menu.page == Page::PortPerf {
            self.port_perf_menu_input(b);
            return;
        }
        if matches!(menu.page, Page::QuitUnsaved { .. }) {
            self.quit_question_input(b);
            return;
        }
        if menu.page != Page::Main {
            let before = (menu.page, menu.settings.as_ref().map(|s| s.values.clone()));
            Self::options_input(menu, b);
            // Options → Runs on: the page needs the App (`app_emu_choice`).
            if menu.page == Page::EmuChoice {
                self.emu_choice_in_menu();
                return;
            }
            // "Advanced…" on a PS2 game's options: back to the game, which opens its own menu.
            if menu.want_advanced {
                self.resume_game(None);
                return;
            }
            // Entering Picture or changing one of its settings: show what it looks like now.
            let entered = matches!(menu.page, Page::Group { .. }) && !matches!(before.0, Page::Group { .. });
            if entered || before.1 != menu.settings.as_ref().map(|s| s.values.clone()) {
                self.request_preview();
            }
            return;
        }
        let loadable = menu.saved.is_some();
        match b {
            Button::Up | Button::Down => {
                // Wraps like every other menu: Up from the top reaches Quit in one press.
                menu.sel = pocket_ui::wrap_step(menu.sel, menu.items().len(), b == Button::Up);
                // Save and Load show different states: the slot Save writes, what Load picks.
                menu.refresh_saved();
            }
            // Earlier: Left and Right walk through its states, newest first (`app_earlier`).
            Button::Left | Button::Right if menu.item() == MainItem::Earlier => {
                if let Some(row) = menu.earlier.as_mut() {
                    row.step(if b == Button::Left { -1 } else { 1 });
                }
                menu.refresh_saved();
            }
            // Disc: Left and Right choose the disc A puts in (`app_disc`).
            Button::Left | Button::Right if menu.item() == MainItem::Disc => {
                if let Some(d) = menu.disc.as_mut() {
                    d.step(if b == Button::Left { -1 } else { 1 });
                }
            }
            // Left and Right step through what the row offers: Save the manual slots, Load Auto, Before load and
            // the slots. A slot reached on the way becomes the one the hotkeys use too.
            Button::Left | Button::Right
                if menu.states.is_some()
                    && match menu.item() {
                        MainItem::Load => menu.loads.len() > 1,
                        MainItem::Save => menu.filled.len() > 1,
                        _ => false,
                    } =>
            {
                let delta = if b == Button::Left { -1 } else { 1 };
                if menu.item() == MainItem::Load {
                    menu.load_at = (menu.load_at as i32 + delta).rem_euclid(menu.loads.len().max(1) as i32) as usize;
                    if let Some(Target::Slot(n)) = menu.loads.get(menu.load_at).copied() {
                        menu.slot = n as usize - 1;
                    }
                } else {
                    menu.slot = (menu.slot as i32 + delta).rem_euclid(menu.filled.len().max(1) as i32) as usize;
                    // Load follows the slot Save is on, unless it is on Auto or Before load.
                    if let Some(at) = menu.loads.iter().position(|t| *t == Target::Slot(menu.slot as u8 + 1)) {
                        menu.load_at = at;
                    }
                }
                menu.refresh_saved();
                if let Some(run) = self.running.as_ref().filter(|r| r.hosted()) {
                    slots::set_current(&self.data_dir, &run.system, run.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default(), menu.slot);
                    control::pocket_core(&format!("slot {}", menu.slot + 1));
                }
            }
            b if b == CANCEL || b == Button::Mode => self.resume_game(None),
            // START is the way to the quick settings (brightness, volume, fan) from anywhere in the menu: 1 press
            // instead of 7. Not while SELECT is held: SELECT + START is pocket-core's quit.
            Button::Start if menu.items().contains(&MainItem::QuickSettings) && !self.pad.is_pressed(Button::Select) => self.open_quick_settings_over_game(),
            b if b == CONFIRM => match menu.item() {
                MainItem::Continue => self.resume_game(None),
                MainItem::Save => self.resume_game(Some(SaveAction::Save)),
                MainItem::Load if loadable => {
                    let target = menu.loads.get(menu.load_at).copied().unwrap_or(Target::Slot(menu.slot as u8 + 1));
                    self.resume_game(Some(SaveAction::Load(target)))
                }
                // An empty slot: nothing to load.
                MainItem::Load => pocket_ui::ui_sound::refuse(),
                MainItem::Earlier if menu.earlier.as_ref().is_some_and(pocket_ui::timeline::Row::loadable) => self.resume_game(Some(SaveAction::Earlier)),
                // None yet, or a damaged one.
                MainItem::Earlier => pocket_ui::ui_sound::refuse(),
                MainItem::Disc => self.put_in_disc(),
                MainItem::Options if menu.emu_set.is_some() => {
                    menu.emu_note = None;
                    menu.page = Page::Standalone { sel: 0 };
                }
                MainItem::Options if menu.hosted => menu.page = Page::Options { sel: 0 },
                // A port: its own Performance & fan, where "Runs on" would mean nothing (`app_port_perf`).
                MainItem::Options if menu.saves == pocket_ui::pause_saves::Saves::InGame(pocket_ui::pause_saves::PORT_SAVES) => self.port_perf_in_menu(),
                // RetroArch and the other emulators: "Runs on" (`app_emu_choice`).
                MainItem::Options => self.emu_choice_in_menu(),
                // Read again: pocket-core wrote the list when the menu opened.
                MainItem::Achievements => {
                    if let Some(run) = self.running.as_ref() {
                        menu.achievements = Self::menu_achievements(&run.path).or(menu.achievements.take());
                    }
                    menu.page = Page::Achievements { sel: 0 };
                }
                // Over the paused game; B or HOME there comes back to this menu.
                // With what its Performance and Fan rows need about this game (`app_quick_profiles`).
                MainItem::QuickSettings => self.open_quick_settings_over_game(),
                MainItem::PixelPath => self.open_pixel_path(),
                MainItem::Players => self.open_players(),
                // The game's saves don't reach the card: Quit asks once first (`app_save_trouble`).
                MainItem::Quit if app_save_trouble::asks_before_quit(menu) => menu.page = Page::QuitUnsaved { sel: 0 },
                // RetroArch writes the auto state on quit, so Continue on Home picks up right here.
                MainItem::Quit if menu.retroarch => {
                    menu.quitting = true;
                    control::quit_retroarch();
                    self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::RetroArch)));
                }
                // Cemu, RPCS3 and Vita3K have no states: their window is closed, SIGTERM and SIGKILL if it hangs (`standalone::guest`).
                MainItem::Quit if standalone::guest_active() => {
                    menu.quitting = true;
                    let pgid = menu.frozen.or(self.running.as_ref().and_then(|r| r.pgid));
                    if let Some(g) = menu.frozen {
                        control::resume_game(g);
                    }
                    standalone::guest_quit(pgid);
                    self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::Guest)));
                }
                // ARMSX2, Dolphin and Azahar write a resume state first (not without saving), then shut down by themselves.
                MainItem::Quit if standalone::active() => {
                    let save = !menu.without_saving;
                    // Options changed and not applied yet are kept for the next start.
                    if let Some(Err(e)) = menu.emu_set.as_mut().map(emu_settings::Menu::commit) {
                        println!("options: {e}");
                    }
                    menu.quitting = true;
                    menu.discarding = !save;
                    let pgid = menu.frozen.or(self.running.as_ref().and_then(|r| r.pgid));
                    if let Some(g) = menu.frozen {
                        control::resume_game(g);
                    }
                    standalone::quit(pgid, menu.frame.as_ref().map(|f| Image { w: f.w, h: f.h, px: f.px.clone() }), save);
                    self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::Standalone)));
                }
                // pocket-core without saving: the battery save, no auto state (`quit nosave`). Without the command
                // nothing quits: the window's close request would save.
                MainItem::Quit if menu.hosted && menu.without_saving => {
                    if control::pocket_core(control::QUIT_WITHOUT_SAVING) {
                        menu.quitting = true;
                        menu.discarding = true;
                        self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::Window)));
                    } else {
                        pocket_ui::ui_sound::refuse();
                    }
                }
                MainItem::Quit => {
                    pocket_ui::fast_switch::mark("pause menu: quit");
                    menu.quitting = true;
                    if let Some(g) = menu.frozen {
                        control::resume_game(g);
                    }
                    let (hosted, window) = (menu.hosted, menu.window);
                    // pocket-core: its quit command, a hand-over where the core allows it (`app_fast_switch`): no
                    // swaymsg, no focus change, the shelf once the save is done. Anything else, or a pocket-core that
                    // doesn't take it, closes its window.
                    let sent = hosted && self.quit_to_shelf();
                    if let Some(con) = window.filter(|_| !sent) {
                        control::close_window(con);
                    }
                    self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::Window)));
                }
            },
            _ => {}
        }
    }

    /// Options screens: groups, the settings of one group, and Save changes. B goes one level up.
    fn options_input(menu: &mut QuickMenu, b: Button) {
        let entries = opt_entries(menu.emu.as_ref(), menu.cheats.is_some());
        let last = entries.len() - 1;
        let up = |sel: &mut usize, n: usize| *sel = (*sel + n - 1) % n;
        let down = |sel: &mut usize, n: usize| *sel = (*sel + 1) % n;
        // Left/right step one value; L1/R1 skip five in a long list (palettes).
        let delta = match b {
            Button::Left => -1,
            Button::L1 => -5,
            Button::R1 => 5,
            _ => 1,
        };
        let at = |e: OptEntry| opt_pos(menu.emu.as_ref(), menu.cheats.is_some(), e);
        menu.page = match (menu.page, b) {
            (Page::Options { .. }, b) if b == CANCEL => {
                menu.sel = menu.items().iter().position(|i| *i == MainItem::Options).unwrap_or(0);
                Page::Main
            }
            (Page::Options { mut sel }, Button::Up) => {
                up(&mut sel, entries.len());
                Page::Options { sel }
            }
            (Page::Options { mut sel }, Button::Down) => {
                down(&mut sel, entries.len());
                Page::Options { sel }
            }
            (Page::Options { sel }, b) if b == CONFIRM => {
                menu.note = None;
                match entries[sel] {
                    OptEntry::Group(group) => Page::Group { group, sel: 0 },
                    OptEntry::Emulator => Page::Emulator { cat: None, sel: 0 },
                    OptEntry::Controls => Page::Controls { sel: 0 },
                    OptEntry::Cheats => Page::Cheats { sel: 0 },
                    OptEntry::RunsOn => Page::EmuChoice,
                    OptEntry::Save => Page::SaveChanges { sel: 0 },
                }
            }
            (Page::Group { group, .. }, b) if b == CANCEL => Page::Options { sel: at(OptEntry::Group(group)) },
            (Page::Group { group, mut sel }, Button::Up) => {
                up(&mut sel, options::GROUPS[group].visible(menu.settings.as_ref()).len());
                Page::Group { group, sel }
            }
            (Page::Group { group, mut sel }, Button::Down) => {
                down(&mut sel, options::GROUPS[group].visible(menu.settings.as_ref()).len());
                Page::Group { group, sel }
            }
            (Page::Group { group, sel }, b) if matches!(b, Button::Left | Button::Right) || b == CONFIRM => {
                // Rows come and go with the frame (its colour shows under Shell); the selected one stays.
                let visible = options::GROUPS[group].visible(menu.settings.as_ref());
                let setting = visible[sel.min(visible.len() - 1)];
                if let Some(s) = menu.settings.as_mut() {
                    s.step(setting, if b == Button::Left { -1 } else { 1 });
                    // Applied to the running game right away; saved only through Save changes.
                    let value = setting.choices[s.index(setting)].value;
                    control::pocket_core(&format!("set {} {value}", setting.key));
                }
                Page::Group { group, sel }
            }
            // The core's own options: categories are pages of their own.
            (Page::Emulator { cat, .. }, b) if b == CANCEL => match cat {
                Some(c) => Page::Emulator { cat: None, sel: c },
                None => Page::Options { sel: at(OptEntry::Emulator) },
            },
            (Page::Emulator { cat, mut sel }, Button::Up | Button::Down) => {
                let n = menu.emu.as_ref().map_or(1, |e| e.entries(cat).len().max(1));
                if b == Button::Up {
                    up(&mut sel, n)
                } else {
                    down(&mut sel, n)
                }
                Page::Emulator { cat, sel }
            }
            (Page::Emulator { cat, sel }, b) if matches!(b, Button::Left | Button::Right | Button::L1 | Button::R1) || b == CONFIRM => match (menu.emu.as_mut(), menu.settings.as_mut()) {
                (Some(emu), Some(settings)) => match emu.step(cat, sel, delta, settings) {
                    Some(c) => Page::Emulator { cat: Some(c), sel: 0 },
                    None => Page::Emulator { cat, sel },
                },
                _ => Page::Emulator { cat, sel },
            },
            (Page::Controls { .. }, b) if b == CANCEL => Page::Options { sel: at(OptEntry::Controls) },
            (Page::Controls { mut sel }, Button::Up | Button::Down) => {
                let n = menu.emu.as_ref().map_or(1, |e| e.control_count());
                if b == Button::Up {
                    up(&mut sel, n)
                } else {
                    down(&mut sel, n)
                }
                Page::Controls { sel }
            }
            (Page::Controls { sel }, b) if matches!(b, Button::Left | Button::Right) || b == CONFIRM => {
                if let (Some(emu), Some(settings)) = (menu.emu.as_mut(), menu.settings.as_mut()) {
                    emu.control_step(sel, delta, settings);
                }
                Page::Controls { sel }
            }
            // The game's cheats: A (or Left/Right) switches one, L1/R1 skip five.
            (Page::Cheats { .. }, b) if b == CANCEL => Page::Options { sel: at(OptEntry::Cheats) },
            (Page::Cheats { mut sel }, Button::Up | Button::Down | Button::L1 | Button::R1) => {
                let n = menu.cheats.as_ref().map_or(1, |c| c.count());
                match b {
                    Button::Up => up(&mut sel, n),
                    Button::Down => down(&mut sel, n),
                    _ => sel = (sel as i32 + delta).clamp(0, n as i32 - 1) as usize,
                }
                Page::Cheats { sel }
            }
            (Page::Cheats { sel }, b) if matches!(b, Button::Left | Button::Right) || b == CONFIRM => {
                if let Some(c) = menu.cheats.as_mut() {
                    c.press(sel);
                }
                Page::Cheats { sel }
            }
            // The game's achievements: a list to read; B goes back to the menu.
            (Page::Achievements { .. }, b) if b == CANCEL => {
                menu.sel = menu.items().iter().position(|i| *i == MainItem::Achievements).unwrap_or(0);
                Page::Main
            }
            (Page::Achievements { mut sel }, Button::Up | Button::Down | Button::L1 | Button::R1) => {
                let n = menu.achievements.as_ref().map_or(1, |s| pocket_ui::achievements_page::menu_rows(s).len()).max(1);
                match b {
                    Button::Up => up(&mut sel, n),
                    Button::Down => down(&mut sel, n),
                    _ => sel = (sel as i32 + delta).clamp(0, n as i32 - 1) as usize,
                }
                Page::Achievements { sel }
            }
            // A PS2, GameCube, Wii or 3DS game's options: Left/Right (or A) change a value, A on "Advanced…" opens the
            // emulator's own menu; the values apply when the game goes on.
            // From "More…" back to the first page of the options, from there to the menu.
            (Page::Standalone { .. }, b) if b == CANCEL => match menu.emu_set.as_mut().and_then(emu_settings::Menu::close_more) {
                Some(sel) => Page::Standalone { sel },
                None => {
                    menu.sel = menu.items().iter().position(|i| *i == MainItem::Options).unwrap_or(0);
                    Page::Main
                }
            },
            (Page::Standalone { mut sel }, Button::Up | Button::Down) => {
                let n = menu.emu_set.as_ref().map_or(1, |e| e.len().max(1));
                if b == Button::Up {
                    up(&mut sel, n)
                } else {
                    down(&mut sel, n)
                }
                Page::Standalone { sel }
            }
            (Page::Standalone { mut sel }, b) if matches!(b, Button::Left | Button::Right) || b == CONFIRM => {
                if let Some(set) = menu.emu_set.as_mut() {
                    if set.is_more(sel) {
                        if b == CONFIRM || b == Button::Right {
                            set.open_more();
                            sel = 0;
                        } else {
                            pocket_ui::ui_sound::refuse();
                        }
                    } else if set.is_advanced(sel) {
                        menu.want_advanced = b == CONFIRM;
                    } else if !set.step(sel, if b == Button::Left { -1 } else { 1 }) {
                        pocket_ui::ui_sound::refuse();
                    }
                }
                Page::Standalone { sel }
            }
            (Page::SaveChanges { .. }, b) if b == CANCEL => Page::Options { sel: last },
            (Page::SaveChanges { mut sel }, Button::Up) => {
                up(&mut sel, SAVE_CHANGES.len());
                Page::SaveChanges { sel }
            }
            (Page::SaveChanges { mut sel }, Button::Down) => {
                down(&mut sel, SAVE_CHANGES.len());
                Page::SaveChanges { sel }
            }
            (Page::SaveChanges { sel }, b) if b == CONFIRM => {
                if let Some(s) = menu.settings.as_mut() {
                    menu.note = Some(match sel {
                        0 if s.save_for_console() => "Saved for this console.",
                        1 if s.save_for_game() => "Saved for this game.",
                        2 => {
                            s.restore_defaults();
                            if let Some(emu) = menu.emu.as_mut() {
                                emu.restored();
                            }
                            "Back to defaults."
                        }
                        _ => {
                            pocket_ui::ui_sound::refuse();
                            "Couldn't save the settings."
                        }
                    });
                    control::pocket_core("reload");
                }
                Page::Options { sel: last }
            }
            (page, _) => page,
        };
    }

    fn resume_game(&mut self, action: Option<SaveAction>) {
        // PS2, GameCube, Wii and 3DS options changed in the menu: kept for this game now, and applied live or, when the
        // emulator can't take them while running, by a restart that comes back to this moment. After Save or
        // Load a restart waits for the next start, so the two never write states at the same time.
        let (mut live, mut advanced) = (Vec::new(), None);
        if let Some(menu) = self.menu.as_mut() {
            let want_advanced = std::mem::take(&mut menu.want_advanced);
            if let Some(set) = menu.emu_set.as_mut() {
                match set.commit() {
                    Ok(plan) if plan.restart && action.is_none() => {
                        self.begin_reapply();
                        return;
                    }
                    Ok(plan) => live = plan.actions,
                    Err(e) => {
                        println!("options: {e}");
                        pocket_ui::ui_sound::refuse();
                    }
                }
                advanced = set.advanced().filter(|_| want_advanced);
            }
        }
        let Some(menu) = self.menu.take() else { return };
        // The game goes on with the Options as the menu leaves them; the menu opens on them next time.
        if let (Some(run), Some(s)) = (self.running.as_mut(), menu.settings.as_ref()) {
            run.options.keep(s);
        }
        if let Some(g) = menu.frozen {
            control::resume_game(g);
        }
        if standalone::active() {
            match action {
                Some(SaveAction::Save) => standalone::save(Target::Slot(menu.slot as u8 + 1), menu.frame),
                Some(SaveAction::Load(target)) => standalone::load(target, menu.frame),
                // Earlier is pocket-core's only.
                Some(SaveAction::Earlier) | None => {}
            }
        } else {
            match (action, menu.hosted) {
                (Some(SaveAction::Save), true) => control::pocket_core(&format!("save {}", menu.slot + 1)),
                (Some(SaveAction::Load(target)), true) => control::pocket_core(&format!(
                    "load {}",
                    match target {
                        Target::Auto => "auto".to_string(),
                        Target::Undo => "undo".to_string(),
                        Target::Slot(n) => n.to_string(),
                    }
                )),
                (Some(SaveAction::Earlier), true) => menu.earlier.as_ref().and_then(pocket_ui::timeline::Row::command).is_some_and(|c| control::pocket_core(&c)),
                (Some(SaveAction::Save), false) => control::retroarch("SAVE_STATE"),
                (Some(SaveAction::Load(_)), false) => control::retroarch("LOAD_STATE"),
                (Some(SaveAction::Earlier), false) => true,
                (None, _) => true,
            };
        }
        match menu.window {
            Some(con) => control::show_window(con),
            None => control::show_game(),
        };
        // After the window is back, so no frame of play is spent hidden.
        if menu.hosted {
            control::pocket_core("resume");
        }
        // PINE and Dolphin's hotkeys need the game running and in front.
        if !live.is_empty() || advanced.is_some() {
            let pgid = self.running.as_ref().and_then(|r| r.pgid);
            std::thread::spawn(move || {
                emu_settings::apply_live(&live);
                if let Some(adv) = advanced {
                    emu_settings::open_advanced(adv, pgid);
                }
            });
        }
    }

    /// Options that need a new start: the game runs on behind "Applying…" while the emulator writes the
    /// resume state, then closes; `game_exited` starts it again and the start loads that state.
    fn begin_reapply(&mut self) {
        let (Some(menu), Some(run)) = (self.menu.as_mut(), self.running.as_ref()) else { return };
        menu.quitting = true;
        menu.applying = true;
        let pgid = menu.frozen.or(run.pgid);
        if let Some(g) = menu.frozen {
            control::resume_game(g);
        }
        let (restart, first) = emu_settings::Restart::start();
        debug_assert_eq!(first, emu_settings::Cmd::SaveAndClose);
        self.reapply = Some(Reapply { restart, game: (run.system.clone(), run.name.clone(), run.path.clone()) });
        let proxy = self.proxy.clone();
        let frame = menu.frame.as_ref().map(|f| Image { w: f.w, h: f.h, px: f.px.clone() });
        standalone::save_and_close(pgid, frame, move |saved| {
            let _ = proxy.send_event(UserEvent::OptionsSaved(saved));
        });
        self.request_redraw();
    }

    fn options_saved(&mut self, saved: bool) {
        let Some(r) = self.reapply.as_mut() else { return };
        match r.restart.on(emu_settings::Event::Saved(saved)) {
            // Saved: the emulator is closing; stopped like after Quit if it hangs.
            None => self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::Standalone))),
            Some(emu_settings::Cmd::BackToGame) => {
                self.reapply = None;
                if let Some(menu) = self.menu.as_mut() {
                    menu.quitting = false;
                    menu.applying = false;
                    menu.emu_note = Some("Couldn't save the game, so the new options apply at its next start.".into());
                    if let Some(g) = menu.frozen {
                        control::pause_game(g);
                    }
                }
                pocket_ui::ui_sound::refuse();
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Where the Picture preview goes on a `w`×`h` screen, when the Picture group of a pocket-core
    /// game is showing: between the settings and the longest explanation of the group, so the
    /// images keep their size while the selection moves.
    fn preview_layout(&mut self, w: i32, h: i32) -> Option<preview::Layout> {
        self.picture_page(w, h).map(|(_, layout)| layout)
    }

    /// The Picture page of a pocket-core game: how many rows its list shows, and where the preview goes. Normal text
    /// shows the most rows the group can have, so the preview stays put when a sub-choice appears; Large shows as
    /// many as leave the preview its share of the screen (`preview::rows_above`), at least three, and the list scrolls.
    fn picture_page(&mut self, w: i32, h: i32) -> Option<(usize, preview::Layout)> {
        let menu = self.menu.as_ref()?;
        let Page::Group { group, .. } = menu.page else { return None };
        let g = &options::GROUPS[group];
        if !menu.hosted || g.name != "Picture" {
            return None;
        }
        let max_rows = g.max_rows(menu.settings.as_ref());
        // The explanation stands beside the hint pill, in the column left of it, its last line on the pill's
        // (`kit::footer`); the longest of the group sets the room.
        let pill_x = w - NU_PAD - pocket_ui::kit::pill_width(&mut self.text, w, PICTURE_HINTS);
        let lines = g.settings.iter().map(|s| pocket_ui::kit::footer_lines(&mut self.text, s.desc, w, pill_x, 5).len()).max().unwrap_or(1);
        let foot = h - pocket_ui::kit::footer_baseline(h) + NU_SMALL as i32 - pocket_ui::type_scale::line(NU_SMALL, 10);
        Some(preview::picture_page_over((w, h), NU_PAD, foot, NU_PAD + NU_PILL + 12, max_rows, lines, NU_SMALL))
    }

    /// Asks pocket-core for a new preview of the Picture settings as they are now.
    fn request_preview(&mut self) {
        let (w, h) = self.screen_size();
        let (w, h) = (w as i32, h as i32);
        let Some(layout) = self.preview_layout(w, h) else { return };
        let Some(menu) = self.menu.as_mut() else { return };
        let proxy = self.proxy.clone();
        menu.live.request(control::pocket_core, &layout, move |seq, images| {
            let _ = proxy.send_event(UserEvent::Preview { seq, images });
        });
    }

    /// NextUI-style pause menu: the paused game dimmed behind, the title in a paper pill, the items
    /// as a vertical list with the selected one in a pill, a save preview on the right, button hints
    /// at the bottom right.
    fn paint_menu(&mut self, c: &mut Canvas) {
        // The game's picture, darkened (made once per picture, `app_menu_open`); a little more behind Pixel Path's text.
        let fade = if self.menu.as_ref().is_some_and(|m| m.page == Page::PixelPath) { app_pixel_path::PAGE_FADE } else { 225 };
        self.paint_game_behind(c, VOID, fade);
        let Some(menu) = self.menu.as_ref() else { return };
        if menu.page == Page::PixelPath {
            self.paint_pixel_path(c);
            return;
        }
        if menu.page == Page::Players {
            self.paint_players(c);
            return;
        }
        if menu.page == Page::EmuChoice {
            self.paint_emu_choice(c);
            return;
        }
        if menu.page == Page::PortPerf {
            self.paint_port_perf(c);
            return;
        }
        if menu.page != Page::Main {
            self.paint_options(c);
            return;
        }
        let title = self.running.as_ref().map(|r| r.name.clone()).unwrap_or_default();
        let saved = menu.saved.as_ref().map(|(img, t)| {
            let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            (img.is_some(), recents::ago(secs))
        });
        let (sel, quitting, discarding, without_saving, items) = (menu.sel, menu.quitting, menu.discarding, menu.without_saving, menu.items());
        let item = items.get(sel).copied().unwrap_or(MainItem::Continue);
        let has_states = items.contains(&MainItem::Save);
        let (applying, emu_note) = (menu.applying, menu.emu_note.clone());
        // Where Save and Load can't do anything: the line that says where saving is (`pause_saves`); the Disc row.
        let (saves_line, disc) = (menu.saves.line(), menu.disc);
        // States with slots (`states.rs`): what each row shows, and whether Left/Right step through it.
        let slotted = menu.states.is_some();
        let (slot, filled) = (menu.slot, menu.filled.clone());
        let save_target = Target::Slot(slot as u8 + 1);
        let load_target = menu.loads.get(menu.load_at).copied();
        let (loads_len, slots_len) = (menu.loads.len(), filled.len());
        // Earlier (`app_earlier`): its chip, its line under the picture, whether A loads it.
        let earlier = menu.earlier.as_ref().map(|r| (app_earlier::chip(r), app_earlier::note(r), r.loadable(), r.entries.len()));
        let earlier_len = earlier.as_ref().map_or(0, |e| e.3);
        let anything_to_load = menu.loads.iter().any(|t| !matches!(t, Target::Slot(_))) || filled.iter().any(|f| *f);
        let steps = move |item: MainItem| match item {
            MainItem::Load => loads_len > 1,
            MainItem::Save => slots_len > 1,
            MainItem::Earlier => earlier_len > 1,
            _ => false,
        };

        // Title pill across the top, on the label's paper like every selected row (the menu is dark in both appearances).
        let (pad, pill) = (NU_PAD, NU_PILL);
        let title_w = c.w - 2 * pad;
        c.round_rect(pad, pad, title_w, pill, pill / 2, DARK.select);
        // The battery in this game at the pill's right ("About 3 h 20 min left in this game", `battery_games`); the
        // title gets the rest.
        let mut room = title_w - 2 * NU_BTN_PAD;
        if let Some(line) = self.menu_battery.clone() {
            let line = self.text.fit(Face::Body, NU_SMALL, &line, room * 3 / 5);
            room -= self.text.width(Face::Body, NU_SMALL, &line) + 40;
            self.text.draw_right(c, Face::Body, NU_SMALL, pad + title_w - NU_BTN_PAD, pad + pill / 2 + pocket_ui::type_scale::mid(NU_SMALL), DARK.label_dust, &line);
        }
        let fitted = self.text.fit(Face::Body, NU_FONT, &title, room);
        self.text.draw(c, Face::Body, NU_FONT, pad + NU_BTN_PAD, pad + pill / 2 + pocket_ui::type_scale::mid(NU_FONT), INK, &fitted);
        // Saves that don't reach the card: said under the title on every row (`app_save_trouble`); the rows start below.
        let below_title = match self.menu.as_ref().and_then(|m| m.save_trouble) {
            Some(t) => self.paint_save_trouble_line(c, t),
            None => pad + pill,
        };

        // Items, vertically centred; the selected one sits in a paper pill as wide as its text. A long menu (Players,
        // Achievements and Pixel Path all there) closes up so it stays between the title and the hints.
        let step = pill.min((c.h - pad - pill - below_title) / items.len().max(1) as i32);
        let top = ((c.h - items.len() as i32 * step) / 2).max(below_title);
        for (i, &row) in items.iter().enumerate() {
            let label = row.label();
            let y = top + i as i32 * step;
            let enabled = match row {
                // With Earlier chosen, the preview is Earlier's: Load is there when anything can be loaded.
                MainItem::Load if item == MainItem::Earlier => anything_to_load,
                MainItem::Load => saved.is_some(),
                MainItem::Earlier => earlier.as_ref().is_some_and(|e| e.2),
                _ => true,
            };
            let label_w = self.text.width(Face::Body, NU_FONT, label);
            let baseline = y + step / 2 + pocket_ui::type_scale::mid(NU_FONT);
            // Save and Load show their state: the slot ("3" at rest, "‹ 3 ›" when selected), or on Load also "Auto" and "Undo";
            // Disc the disc chosen ("1 of 3", "‹ 2 of 3 ›").
            let disc_chip = disc.filter(|_| row == MainItem::Disc).map(|d| d.chip());
            let earlier_chip = earlier.as_ref().filter(|_| row == MainItem::Earlier).and_then(|e| e.0.clone());
            let chip = (slotted && matches!(row, MainItem::Save | MainItem::Load))
                .then(|| if row == MainItem::Load { load_target } else { Some(save_target) })
                .flatten()
                .map(states::chip)
                .or(disc_chip)
                .or(earlier_chip)
                .map(|name| {
                    if i == sel && (steps(row) || row == MainItem::Disc) {
                        format!("‹ {name} ›")
                    } else {
                        name
                    }
                });
            let chip_w = chip.as_ref().map_or(0, |t| NU_SMALL as i32 / 2 + self.text.width(Face::Body, NU_FONT, t));
            if i == sel {
                c.round_rect(pad, y, label_w + chip_w + 2 * NU_BTN_PAD, step, step / 2, DARK.select);
                self.text.draw(c, Face::Body, NU_FONT, pad + NU_BTN_PAD, baseline, INK, label);
            } else {
                self.text.draw(c, Face::Body, NU_FONT, pad + NU_BTN_PAD, baseline, if enabled { 0xffffff } else { 0x7f7f7f }, label);
            }
            if let Some(chip) = chip {
                let (x, color) = (pad + NU_BTN_PAD + label_w + NU_SMALL as i32 / 2, if i == sel { DARK.label_dust } else { 0x9a9a9a });
                self.text.draw(c, Face::Body, NU_FONT, x, baseline, color, &chip);
            }
        }

        // Right: a dark window, half the screen, with the save's preview or a message.
        if !quitting && matches!(item, MainItem::Save | MainItem::Load | MainItem::Earlier) {
            // A little lower with larger text (Settings → Text size), so the note under it keeps clear of the hints.
            let (hw, hh) = (c.w / 2, c.h / 2 - 5 * pocket_ui::type_scale::grow(NU_SMALL));
            let r = 12;
            let (ox, oy) = (c.w - pad - hw - 2 * r, (c.h - hh - 2 * r) / 2);
            c.round_rect(ox, oy, hw + 2 * r, hh + 2 * r, r * 2, 0x262626);
            c.rect(ox + r, oy + r, hw, hh, 0x000000);
            let img = self.menu.as_ref().and_then(|m| m.saved.as_ref()).and_then(|(i, _)| i.as_ref());
            match (img, &saved) {
                (Some(img), _) => {
                    // The picture keeps its shape inside the window.
                    let (iw, ih) = img.fit(hw, hh);
                    c.blit_scaled(img, ox + r + (hw - iw) / 2, oy + r + (hh - ih) / 2, iw, ih);
                }
                (None, Some(_)) => self.centered(c, ox + r, oy + r, hw, hh, "No preview"),
                (None, None) if item == MainItem::Earlier => self.centered(c, ox + r, oy + r, hw, hh, "None yet"),
                (None, None) => self.centered(c, ox + r, oy + r, hw, hh, if slotted { "Empty" } else { "Empty slot" }),
            }
            let mut note_y = oy + hh + 2 * r + 48;
            // The slot the row is on (Load may be on Auto or Before load: no slot is ringed then).
            let on_slot = match (item, load_target) {
                (MainItem::Load, Some(Target::Slot(n))) => Some(n as usize - 1),
                (MainItem::Load, _) => None,
                _ => Some(slot),
            };
            // Earlier has no slots: no dots under its picture.
            if slotted && filled.len() > 1 && item != MainItem::Earlier {
                // One dot per slot: filled ones are lit, the one the row is on has a ring.
                let (dot, gap) = (18, 16);
                let count = filled.len() as i32;
                let x0 = ox + (hw + 2 * r - (count * dot + (count - 1) * gap)) / 2;
                for (n, full) in filled.iter().enumerate() {
                    let cx = x0 + n as i32 * (dot + gap) + dot / 2;
                    let cy = oy + hh + 2 * r + 34;
                    c.disc(cx, cy, dot / 2, if *full { 0xffffff } else { 0x4a4a4a });
                    if Some(n) == on_slot {
                        c.ring(cx, cy, dot / 2 + 8, 3, 0xffffff);
                    }
                }
                note_y += 34;
            }
            let ago = saved.as_ref().map(|(_, ago)| ago.as_str());
            let shown = if item == MainItem::Load { load_target } else { Some(save_target) };
            let note = match (slotted, shown, ago, item == MainItem::Save) {
                _ if item == MainItem::Earlier => earlier.as_ref().map(|e| e.1.clone()).unwrap_or_default(),
                // Azahar has its quick slot only: say so instead of offering a picker.
                (true, Some(_), ago, saving) if filled.len() == 1 && (saving || matches!(shown, Some(Target::Slot(_)))) => match (ago, saving) {
                    (Some(ago), true) => format!("The only slot · replaces the save from {ago}"),
                    (Some(ago), false) => format!("The only slot · saved {ago}"),
                    (None, _) => "The only slot · empty".to_string(),
                },
                (true, Some(t), ago, saving) => states::note(t, ago, saving),
                (_, _, Some(ago), true) => format!("Replaces the save from {ago}"),
                (_, _, Some(ago), false) => format!("Saved {ago}"),
                _ => String::new(),
            };
            // Centred under the window, as wide as the screen leaves on both sides of its middle, on a second line
            // if it is wider (larger text).
            let room = 2 * (c.w - 8 - (ox + hw / 2 + r));
            for (k, line) in self.text.wrap(Face::Body, NU_SMALL, &note, room).iter().take(2).enumerate() {
                let w = self.text.width(Face::Body, NU_SMALL, line);
                self.text.draw(c, Face::Body, NU_SMALL, ox + (hw + 2 * r - w) / 2, note_y + k as i32 * pocket_ui::type_scale::line(NU_SMALL, 8), 0xcccccc, line);
            }
        }
        if quitting {
            let msg = if applying {
                "Applying…"
            } else if discarding || !has_states {
                "Quitting…"
            } else {
                "Saving and quitting…"
            };
            self.text.draw_right(c, Face::Body, NU_FONT, c.w - pad - NU_BTN_PAD, c.h / 2, 0xcccccc, msg);
            return;
        }
        // The emulator saves only inside the game (`pause_saves`): said where the player decides, beside Continue,
        // Options and Quit, instead of Save and Load rows that would do nothing.
        let note = match item {
            MainItem::Quit if saves_line.is_some() => saves_line.map(str::to_string),
            MainItem::Quit if without_saving => Some("Doesn't save where you are. In-game saves are kept.".to_string()),
            MainItem::Continue | MainItem::Options => saves_line.map(str::to_string),
            MainItem::Disc => disc.map(|d| d.note()),
            MainItem::QuickSettings => Some("Brightness, volume, Wi-Fi, Bluetooth, performance, fan and power.".to_string()),
            MainItem::PixelPath => Some("How the picture and sound reach you, stage by stage.".to_string()),
            MainItem::Players => Some("Which controller is which player.".to_string()),
            _ => None,
        };
        // Options that couldn't be applied say so instead (not over Save's and Load's preview).
        let note = emu_note.filter(|_| !matches!(item, MainItem::Save | MainItem::Load | MainItem::Earlier)).or(note);
        if let Some(note) = note {
            self.menu_note(c, &note);
        }

        if item == MainItem::Disc {
            self.nu_hints(c, &[("< >", "Disc"), ("SELECT", "Games"), ("B", "Back"), ("A", "OK")]);
        } else if slotted && steps(item) && matches!(item, MainItem::Save | MainItem::Load | MainItem::Earlier) {
            self.nu_hints(c, &[("< >", if item == MainItem::Save { "Slot" } else { "State" }), ("SELECT", "Games"), ("B", "Back"), ("A", "OK")]);
        } else {
            self.nu_hints(c, &[("START", "Quick settings"), ("SELECT", "Games"), ("B", "Back"), ("A", "OK")]);
        }
    }

    /// A line beside the pause menu's rows, right-aligned at the middle of the screen; a long one (German, a long
    /// emulator line) is wrapped into the right half instead of running into the rows.
    fn menu_note(&mut self, c: &mut Canvas, note: &str) {
        let lines = self.text.wrap(Face::Body, NU_SMALL, note, c.w / 2 - NU_PAD);
        let line_h = NU_SMALL as i32 * 4 / 3;
        let top = c.h / 2 - (lines.len() as i32 - 1) * line_h / 2;
        for (i, line) in lines.iter().enumerate() {
            self.text.draw_right(c, Face::Body, NU_SMALL, c.w - NU_PAD - NU_BTN_PAD, top + i as i32 * line_h, 0xcccccc, line);
        }
    }

    /// Options screens in NextUI's layout: a list of rows with the value at the right; the selected
    /// row has its name in a paper pill and a bar in the game's system colour out to the value. The
    /// selected entry's explanation sits at the bottom left beside the hint pill (`kit::footer`), the status pill at
    /// the top right.
    fn paint_options(&mut self, c: &mut Canvas) {
        let Some(menu) = self.menu.as_ref() else { return };
        let accent = self.running.as_ref().map_or(DUST, |r| stripe(&r.system));
        // (name, value or ">" for a submenu, explanation), the selected row, a confirmation replacing the
        // explanation, and a heading for pages below the options list
        let (rows, sel, footer, heading): (Vec<(String, String, String)>, usize, Option<String>, Option<String>) = match menu.page {
            Page::Options { sel } => {
                let level = match menu.settings.as_ref().map(|s| s.level) {
                    Some(Level::Game) => "Using this game's settings.",
                    Some(Level::Console) => "Using this console's settings.",
                    _ => "Using the defaults.",
                };
                let rows = opt_entries(menu.emu.as_ref(), menu.cheats.is_some())
                    .into_iter()
                    .map(|e| match e {
                        OptEntry::Group(g) => (options::GROUPS[g].name.to_string(), ">".to_string(), options::GROUPS[g].desc.to_string()),
                        OptEntry::Emulator => ("Emulator".into(), ">".into(), "Settings of the emulator itself, such as colours and palettes.".into()),
                        OptEntry::Controls => ("Controls".into(), ">".into(), "Which button does what, and the left stick.".into()),
                        OptEntry::Cheats => {
                            let on = menu.cheats.as_ref().map_or(0, |c| c.enabled());
                            ("Cheats".into(), if on > 0 { format!("{on} on") } else { ">".into() }, "Infinite lives and the like, from the libretro cheat database.".into())
                        }
                        OptEntry::RunsOn => ("Runs on".into(), ">".into(), "Which emulator plays this game or this console: pocket-core, RetroArch or another.".into()),
                        OptEntry::Save => ("Save changes".into(), ">".into(), level.into()),
                    })
                    .collect();
                (rows, sel, menu.note.map(str::to_string), None)
            }
            Page::Emulator { cat, sel } => {
                let rows = menu.emu.as_ref().map(|e| e.rows(cat)).unwrap_or_default();
                let rows = rows.into_iter().map(|Row { name, value, desc }| (name, value, desc)).collect();
                let heading = match (cat, menu.emu.as_ref()) {
                    (Some(c), Some(e)) => e.category_label(c).map(|l| format!("Emulator   {l}")),
                    _ => Some("Emulator".to_string()),
                };
                (rows, sel, None, heading)
            }
            Page::Controls { sel } => {
                let rows = menu.emu.as_ref().map(|e| e.control_rows()).unwrap_or_default();
                let rows = rows.into_iter().map(|Row { name, value, desc }| (name, value, desc)).collect();
                (rows, sel, None, Some("Controls".to_string()))
            }
            Page::Cheats { sel } => {
                let rows = menu.cheats.as_ref().map(|c| c.rows()).unwrap_or_default();
                let rows = rows.into_iter().map(|Row { name, value, desc }| (name, value, desc)).collect();
                (rows, sel, None, Some("Cheats".to_string()))
            }
            Page::Group { group, sel } => {
                let g = &options::GROUPS[group];
                let rows = g
                    .visible(menu.settings.as_ref())
                    .into_iter()
                    .map(|s| {
                        let i = menu.settings.as_ref().map_or(s.default, |v| v.index(s));
                        (s.name.to_string(), s.choices[i].label.to_string(), s.desc.to_string())
                    })
                    .collect();
                (rows, sel, None, None)
            }
            Page::SaveChanges { sel } => {
                let descs = [
                    "Every game on this console starts with these settings.",
                    "Only this game uses these settings.",
                    "Removes the saved settings for this console and this game.",
                ];
                (SAVE_CHANGES.iter().zip(descs).map(|(n, d)| (n.to_string(), String::new(), d.to_string())).collect(), sel, None, None)
            }
            Page::Standalone { sel } => {
                let set = menu.emu_set.as_ref();
                let rows = set.map(|e| e.rows()).unwrap_or_default();
                // Once something changed, how it will be applied, under the row's explanation.
                let footer = set.and_then(|e| e.note()).and_then(|n| rows.get(sel).map(|r| format!("{}\n{n}", r.2)));
                (rows, sel, footer, set.map(|e| format!("Options   {}", e.title())))
            }
            Page::Achievements { sel } => {
                let snap = menu.achievements.clone().unwrap_or_default();
                let heading = if snap.state == pocket_ui::achievements::snapshot::State::Ready && snap.total > 0 {
                    format!("Achievements   {}", snap.headline())
                } else {
                    "Achievements".to_string()
                };
                (pocket_ui::achievements_page::menu_rows(&snap), sel, None, Some(heading))
            }
            Page::QuitUnsaved { sel } => {
                let rows = menu.save_trouble.map(Self::quit_question_rows).unwrap_or_default();
                (rows, sel, None, Some(pocket_ui::save_trouble::QUESTION.to_string()))
            }
            Page::Main | Page::PixelPath | Page::EmuChoice | Page::Players | Page::PortPerf => return,
        };

        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        // The heading ("Options   <game>") stops short of the status pill, which is wider with Wi-Fi and "charging".
        let pill_left = self.status_pill(c);
        if let Some(h) = &heading {
            let h = self.text.fit(Face::Body, NU_SMALL, h, pill_left - 24 - (pad + NU_BTN_PAD));
            self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, pad + (NU_PILL - 20) / 2 + pocket_ui::type_scale::mid(NU_SMALL), 0xcccccc, &h);
        }
        let top = pad + NU_PILL + 12;
        let right = c.w - pad;
        // Long lists (a core's options) scroll; the explanation below keeps room for up to five lines over the hint
        // pill. The Picture page with Large text shows fewer rows, so its preview keeps a useful size (`picture_page`).
        let foot = c.h - pocket_ui::kit::footer_baseline(c.h) + 5 * pocket_ui::type_scale::line(NU_SMALL, 10);
        let fit = ((c.h - top - foot) / row).max(3) as usize;
        let fit = self.picture_page(c.w, c.h).map_or(fit, |(rows, _)| rows.min(fit));
        let visible = fit.min(rows.len().max(1));
        let first = window_start(sel, visible, rows.len());
        for (k, (name, value, _)) in rows.iter().enumerate().skip(first).take(visible) {
            let i = k;
            let y = top + (k - first) as i32 * row;
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            // The name gives way to a long value; both stay inside the row.
            let value_w = if value.is_empty() { 0 } else { self.text.width(Face::Body, font, value) + 40 };
            let name = self.text.fit(Face::Body, font, name, right - pad - 2 * NU_BTN_PAD - value_w);
            let name_w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
            if i == sel {
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, accent);
                c.round_rect(pad, y + 4, name_w, row - 8, (row - 8) / 2, DARK.select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, INK, &name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, 0xffffff, &name);
            }
            if !value.is_empty() {
                self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, 0xffffff, value);
            }
        }
        if rows.len() > visible {
            // A thin bar at the right edge shows where the window sits in the list.
            let track = visible as i32 * row;
            pocket_ui::kit::scrollbar(c, top, track, first, visible, rows.len());
        }

        if let Some(layout) = self.preview_layout(c.w, c.h) {
            if let Some(menu) = self.menu.as_ref() {
                menu.live.paint(c, &mut self.text, &layout, 0x9a978f);
            }
        }

        // Explanation of the selected row (or the last confirmation) beside the hints or over them, as on every page
        // of Settings (`kit::footer`); the hints say what B, A and Left/Right do here.
        let desc = footer.unwrap_or_else(|| rows.get(sel).map(|r| r.2.clone()).unwrap_or_default());
        let hints: &[(&str, &str)] = match rows.get(sel).map(|r| r.1.as_str()) {
            Some(">") => &[("B", "Back"), ("A", "Open")],
            Some("") | None => &[("B", "Back"), ("A", "Choose")],
            Some(_) => PICTURE_HINTS,
        };
        pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, hints, &desc, 5, pal().text);
    }

    /// The status corner (time, Wi-Fi while connected, battery) in a pill at the top right, from memory like Home's
    /// (`app_status_corner`); returns the pill's left edge.
    fn status_pill(&mut self, c: &mut Canvas) -> i32 {
        let corner = self.status_corner();
        let pill = pocket_ui::status_corner::Pill { right: c.w - NU_PAD, top: NU_PAD, h: NU_PILL - 20, px: NU_SMALL, pad: NU_BTN_PAD };
        pill.paint(c, &mut self.text, &corner, pal().pill, pal().soft)
    }

    /// The top line of a page of the Settings family (docs/DESIGN.md "Design language"): the status corner in its pill
    /// at the right and the page's title in Title 50 at the left, cut with "…" 24 px short of the pill (a long German
    /// title with Large text). Returns the pill's left edge.
    fn sheet_header(&mut self, c: &mut Canvas, title: &str) -> i32 {
        let pill_left = self.status_pill(c);
        let title = self.text.fit(Face::Title, 50, title, pill_left - 24 - (NU_PAD + NU_BTN_PAD));
        self.text.draw(c, Face::Title, 50, NU_PAD + NU_BTN_PAD, NU_PAD + 58, pal().chalk, &title);
        pill_left
    }

    fn centered(&mut self, c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, msg: &str) {
        let tw = self.text.width(Face::Body, NU_FONT, msg);
        self.text.draw(c, Face::Body, NU_FONT, x + (w - tw) / 2, y + h / 2 + pocket_ui::type_scale::mid(NU_FONT), pal().text, msg);
    }

    /// Button hints in a dark pill at the bottom right: a light circle per button, then its label.
    /// Draws the button hints at the bottom right; returns where their pill starts.
    fn nu_hints(&mut self, c: &mut Canvas, hints: &[(&str, &str)]) -> i32 {
        app_rescue::paint_hints(c, &mut self.text, &mut self.hits, hints)
    }

    /// (system id, ROM path) of every game in the library.
    fn all_games(&self) -> Vec<(String, PathBuf)> {
        self.systems.iter().flat_map(|s| s.games.iter().map(|g| (s.name.clone(), g.path.clone()))).collect()
    }

    fn item_rom(&self, item: Item) -> Option<(String, PathBuf)> {
        match item {
            Item::Recent(i) => self.recents.list.get(i).map(|r| (r.system.clone(), r.path.clone())),
            Item::Game(s, g) => Some((self.systems[s].name.clone(), self.systems[s].games[g].path.clone())),
            Item::System(_) | Item::Favorites | Item::Streaming | Item::Search | Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) => None,
        }
    }

    /// Box art for a game, scaled to fit `max_w` x `max_h`: downloaded cover first, for ports the
    /// image PortMaster ships with the game.
    fn cover(&mut self, item: Item, max_w: i32, max_h: i32) -> Option<Rc<Image>> {
        let (system, rom) = self.item_rom(item)?;
        // The look step's sample game.
        if let Some(img) = self.sample_cover.as_ref().filter(|(path, _)| *path == rom).map(|(_, img)| img.clone()) {
            let (w, h) = img.fit(max_w, max_h);
            return Some(Rc::new(img.resized(w, h)));
        }
        let mut candidates = vec![pocket_ui::covers::cover_path(&self.data_dir, &system, &rom)];
        if system == "ports" {
            let launcher = rom.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            candidates.extend(pocket_ui::ports::launcher_images(&launcher));
        }
        // pocket-ui's own box art first, then the picture in the player's EmulationStation gamelist (read where it is).
        let path = match candidates.into_iter().find(|p| p.exists()) {
            Some(p) => p,
            None => self.gamelist_cover(&system, &rom)?,
        };
        // Like the screenshots, covers are decoded and fitted by the picture worker. A cold cover must
        // not hold up a move or the first box frame. This also bounds the decoded cache while scrolling.
        self.picture(&path, max_w, max_h)
    }

    /// The pixel-art console of a system card (src/system_art.rs), fitted into `max_w` x `max_h`;
    /// `None` for everything that is not a system, and for systems without a picture (plain card).
    fn system_art(&mut self, item: Item, max_w: i32, max_h: i32) -> Option<Rc<Sprite>> {
        let Item::System(s) = item else { return None };
        let name = self.systems.get(s)?.name.clone();
        self.art
            .entry((name.clone(), max_w, max_h))
            .or_insert_with(|| pocket_ui::system_art::get(&name).map(|a| Rc::new(a.fitted(max_w, max_h))))
            .clone()
    }

    fn new_setup(&self) -> Setup {
        let proxy = self.proxy.clone();
        let themes = Theme::ALL.iter().map(|t| (t.name(), t.blurb())).collect();
        let current = Theme::ALL.iter().position(|t| *t == self.theme).unwrap_or(0);
        Setup::new(
            std::sync::Arc::new(move |e| {
                let _ = proxy.send_event(UserEvent::Setup(e));
            }),
            themes,
            current,
            self.data_dir.clone(),
            self.all_games(),
            self.sys.shared_hw(),
        )
        // The Wi-Fi as quick settings see it: one picture, one worker (`net_state`).
        .with_net(self.sys.net())
        .with_shelf(self.shelf_systems(), self.hidden.clone())
        .with_streaming(self.streaming, self.streaming_on)
        .with_art(self.art())
        .with_appearance(self.appearance)
        .with_animations(self.animations)
        .with_catalog(library::all_systems(&self.es_systems).unwrap_or_default())
        .with_switches(self.switches(), self.tour.later.clone())
    }

    /// Every system with games, for Settings > Look > Systems on the shelf.
    fn shelf_systems(&self) -> Vec<pocket_ui::setup::ShelfSystem> {
        self.systems
            .iter()
            .map(|s| pocket_ui::setup::ShelfSystem { id: s.name.clone(), name: s.fullname.clone(), games: s.games.len() })
            .collect()
    }

    fn settings_input(&mut self, b: Button) {
        self.settings_page = match (self.settings_page, b) {
            // The categories, and moving and B in a category's list (`app_settings_groups`).
            (Some(page @ SettingsPage::Groups(_)), b) => app_settings_groups::groups_input(page, b),
            (Some(page @ SettingsPage::List(_)), Button::Up | Button::Down) => app_settings_groups::groups_input(page, b),
            (Some(page @ SettingsPage::List(_)), b) if b == CANCEL => app_settings_groups::groups_input(page, b),
            (Some(SettingsPage::List(sel)), b) if b == CONFIRM => match SETTINGS[sel].1 {
                Opens::Setup(step) => {
                    self.setup = Some(self.new_setup().only(step));
                    Some(SettingsPage::List(sel))
                }
                Opens::Time => {
                    self.sys.open_time(Instant::now());
                    Some(SettingsPage::List(sel))
                }
                Opens::Updates => {
                    self.updates.sel = 0;
                    if self.updates.manifest.is_none() && self.updates.busy.is_none() {
                        self.update_action(UpdateAction::Check);
                    }
                    Some(SettingsPage::Updates)
                }
                Opens::Store => self.open_store(),
                Opens::Starter => self.open_starter(false),
                Opens::Sync => self.open_sync(),
                Opens::Leds => self.open_leds(),
                Opens::NightLight => self.open_night_light(),
                Opens::Tailscale => self.open_tailscale(),
                Opens::Web => self.open_web(),
                Opens::Bluetooth => self.open_bluetooth(),
                Opens::PerfFan => self.open_perf_fan(),
                Opens::SleepPower => self.open_sleep_power(),
                Opens::Curator => self.open_curator_settings(),
                Opens::Achievements => self.open_achievements(),
                Opens::Tools => self.open_tools(),
                Opens::PcStreaming => self.open_pc_streaming(),
                Opens::Backup => self.open_backup(),
                Opens::Storage => self.open_storage(),
                Opens::Gamelist => self.open_gamelist(),
                Opens::Screenshots => self.open_gallery(),
                Opens::Battery => {
                    // A day of graph, or less while the history is young.
                    let now = recents::now_secs();
                    let history = pocket_ui::battery_history::view();
                    let span = history.recent(now, pocket_ui::battery::KEEP).first().map_or(0, |s| now.saturating_sub(s.t));
                    Some(SettingsPage::Battery(pocket_ui::battery::range_for(span)))
                }
                Opens::Manual => Some(SettingsPage::Manual),
                Opens::About => self.open_about(),
                // One press: the file is written and the sheet says where it went (Settings → System → About has the row too).
                Opens::Report => {
                    let page = self.open_about();
                    self.write_report();
                    page
                }
                Opens::Haptics => {
                    pocket_ui::rumble::set_menu_haptics(&self.data_dir, !pocket_ui::rumble::menu_haptics());
                    Some(SettingsPage::List(sel))
                }
                Opens::OnStart => {
                    self.on_start = self.on_start.toggled();
                    self.save_settings();
                    Some(SettingsPage::List(sel))
                }
                Opens::RememberQuit => {
                    self.remember_quit = !self.remember_quit;
                    self.save_settings();
                    Some(SettingsPage::List(sel))
                }
                Opens::Animations => {
                    self.switch_animations();
                    Some(SettingsPage::List(sel))
                }

                Opens::ButtonLayout => {
                    self.switch_button_layout();
                    Some(SettingsPage::List(sel))
                }
                Opens::Frames => {
                    self.frames = self.frames.toggled();
                    self.save_settings();
                    Some(SettingsPage::List(sel))
                }
                Opens::OverGame => {
                    self.over_game = self.over_game.step(1);
                    self.save_settings();
                    Some(SettingsPage::List(sel))
                }
                Opens::Language => {
                    self.set_language(pocket_ui::i18n::current().next());
                    Some(SettingsPage::List(sel))
                }
                Opens::TextSize => {
                    self.set_text_size(pocket_ui::type_scale::get().next());
                    Some(SettingsPage::List(sel))
                }
                Opens::Tour => {
                    self.open_tour();
                    Some(SettingsPage::List(sel))
                }
                Opens::Sounds => {
                    self.sound.on = !self.sound.on;
                    self.sound.save(&self.data_dir);
                    Some(SettingsPage::List(sel))
                }
            },
            // Left and right set how loud; turning it up or down also switches the sounds on.
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Sounds) => {
                self.sound.on = true;
                self.sound.step(if b == Button::Left { -1 } else { 1 });
                self.sound.save(&self.data_dir);
                Some(SettingsPage::List(sel))
            }
            // Two choices: left and right switch between them as A does.
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::OnStart) => {
                self.on_start = self.on_start.toggled();
                self.save_settings();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Animations) => {
                self.switch_animations();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::ButtonLayout) => {
                self.switch_button_layout();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Frames) => {
                self.frames = self.frames.toggled();
                self.save_settings();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::OverGame) => {
                self.over_game = self.over_game.step(if b == Button::Left { -1 } else { 1 });
                self.save_settings();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Language) => {
                self.set_language(pocket_ui::i18n::current().next());
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::TextSize) => {
                let now = pocket_ui::type_scale::get();
                self.set_text_size(if b == Button::Left { now.previous() } else { now.next() });
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::Store), b) => {
                self.store_input(b).or_else(|| Some(SettingsPage::List(SETTINGS.iter().position(|s| matches!(s.1, Opens::Store)).unwrap_or(0))))
            }
            (Some(SettingsPage::Starter), b) => self.starter_input(b),
            (Some(SettingsPage::Sync), b) => self.sync_input(b),
            (Some(SettingsPage::Leds(sel)), b) => self.leds_input(sel, b),
            (Some(SettingsPage::NightLight(sel)), b) => self.night_light_input(sel, b),
            (Some(SettingsPage::Tailscale), b) => self.tailscale_input(b),
            (Some(SettingsPage::Web), b) => self.web_input(b),
            (Some(SettingsPage::Bluetooth), b) => self.bluetooth_input(b),
            (Some(SettingsPage::PerfFan), b) => self.perf_fan_input(b),
            (Some(SettingsPage::SleepPower), b) => self.sleep_power_input(b),
            (Some(SettingsPage::Curator), b) => self.curator_settings_input(b),
            (Some(SettingsPage::Achievements), b) => self.achievements_input(b),
            (Some(SettingsPage::Tools), b) => self.tools_input(b),
            (Some(SettingsPage::PcStreaming), b) => self.pc_streaming_input(b),
            (Some(SettingsPage::Backup), b) => self.backup_input(b),
            (Some(SettingsPage::Storage), b) => self.storage_input(b),
            (Some(SettingsPage::Gamelist), b) => self.gamelist_input(b),
            (Some(SettingsPage::Screenshots), b) => self.gallery_input(b),
            (Some(SettingsPage::Updates), b) if b == CANCEL => Some(SettingsPage::List(settings_row("Updates"))),
            (Some(SettingsPage::Updates), b) => {
                let n = self.update_actions().len();
                match b {
                    Button::Up => self.updates.sel = (self.updates.sel + n - 1) % n,
                    Button::Down => self.updates.sel = (self.updates.sel + 1) % n,
                    // While a download runs, A stops it.
                    b if b == CONFIRM && self.updates.busy.is_some() => {
                        self.updates.request_cancel();
                    }
                    b if b == CONFIRM => {
                        if let Some(action) = self.update_actions().get(self.updates.sel).copied() {
                            self.update_action(action);
                        }
                    }
                    _ => {}
                }
                Some(SettingsPage::Updates)
            }
            (Some(SettingsPage::Battery(_)), b) if b == CANCEL => Some(SettingsPage::List(settings_row("Battery"))),
            (Some(SettingsPage::Battery(range)), b) => {
                let n = pocket_ui::battery::RANGES.len();
                // The times of sleep and switching off moved to Settings → System → Sleep & power (2026-10-26).
                match b {
                    Button::L1 => Some(SettingsPage::Battery((range + n - 1) % n)),
                    Button::R1 => Some(SettingsPage::Battery((range + 1) % n)),
                    _ => Some(SettingsPage::Battery(range)),
                }
            }
            // The manual reads its own input (`manual_input`); this is only B (and the touch's B) closing it.
            (Some(SettingsPage::Manual), b) if b == CANCEL => Some(SettingsPage::List(settings_row("Manual"))),
            // Report a problem and Open EmulationStation… (`app_report`).
            (Some(SettingsPage::About), b) => self.about_input(b),
            (page, _) => page,
        };
    }

    /// Settings list and About page, in the same row style as the pause menu's options.
    fn paint_settings(&mut self, c: &mut Canvas) {
        if self.settings_page == Some(SettingsPage::Store) {
            return self.paint_store(c);
        }
        if self.settings_page == Some(SettingsPage::Starter) {
            return self.paint_starter(c);
        }
        if self.settings_page == Some(SettingsPage::Sync) {
            return self.paint_sync(c);
        }
        if let Some(SettingsPage::Leds(sel)) = self.settings_page {
            return self.paint_leds(c, sel);
        }
        if let Some(SettingsPage::NightLight(sel)) = self.settings_page {
            return self.paint_night_light(c, sel);
        }
        if self.settings_page == Some(SettingsPage::Web) {
            return self.paint_web(c);
        }
        if self.settings_page == Some(SettingsPage::Tailscale) {
            return self.paint_tailscale(c);
        }
        if self.settings_page == Some(SettingsPage::Bluetooth) {
            return self.paint_bluetooth(c);
        }
        if self.settings_page == Some(SettingsPage::PerfFan) {
            return self.paint_perf_fan(c);
        }
        if self.settings_page == Some(SettingsPage::SleepPower) {
            return self.paint_sleep_power(c);
        }
        if self.settings_page == Some(SettingsPage::Curator) {
            return self.paint_curator_settings(c);
        }
        if self.settings_page == Some(SettingsPage::Achievements) {
            return self.paint_achievements(c);
        }
        if self.settings_page == Some(SettingsPage::Tools) {
            return self.paint_tools(c);
        }
        if self.settings_page == Some(SettingsPage::PcStreaming) {
            return self.paint_pc_streaming(c);
        }
        if self.settings_page == Some(SettingsPage::Backup) {
            return self.paint_backup(c);
        }
        if self.settings_page == Some(SettingsPage::Storage) {
            return self.paint_storage(c);
        }
        if self.settings_page == Some(SettingsPage::Gamelist) {
            return self.paint_gamelist(c);
        }
        if self.settings_page == Some(SettingsPage::Screenshots) {
            return self.paint_gallery(c);
        }
        if let Some(SettingsPage::Groups(sel)) = self.settings_page {
            return self.paint_settings_groups(c, sel);
        }
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.status_pill(c);
        let title = match self.settings_page {
            Some(SettingsPage::Manual) => "Manual",
            // A page has its entry's name, as the other pages of a category do.
            Some(SettingsPage::About) => "About",
            Some(SettingsPage::Updates) => "Updates",
            Some(SettingsPage::Battery(_)) => "Battery",
            // A category's list has the category's name (`SETTINGS_GROUPS`).
            Some(SettingsPage::List(sel)) => SETTINGS_GROUPS[app_settings_groups::group_of(sel)].0,
            _ => "Settings",
        };
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, title);
        let top = pad + NU_PILL + 12;
        let right = c.w - pad;
        match self.settings_page {
            Some(SettingsPage::List(sel)) => {
                // Only the rows of the selected row's category, counted from its first (`local`, `at`); their values
                // are worked out for these rows only.
                let rows = app_settings_groups::rows(app_settings_groups::group_of(sel));
                let (at, len) = (rows.iter().position(|i| *i == sel).unwrap_or(0), rows.len());
                // Values that need a process or a file are read on a thread (`app_settings_groups`).
                self.settings_values_refresh(&rows);
                // The hints first: where their pill starts says how much room a description has beside it.
                let hints_at = self.nu_hints(c, &[("B", "Back"), ("A", if matches!(SETTINGS[sel].1, Opens::Haptics | Opens::Sounds | Opens::OnStart | Opens::RememberQuit | Opens::Animations | Opens::ButtonLayout | Opens::Frames | Opens::OverGame | Opens::Language | Opens::TextSize) { "Switch" } else { "Open" })]);
                let desc_x = pad + NU_BTN_PAD;
                let beside = hints_at - 24 - desc_x;
                // A description that doesn't fit beside the hints (German, Large text) takes up to two lines above
                // them. The category keeps the room for it when any of its rows needs it, so the list doesn't shift
                // as the selection moves from one row to the next.
                let desc_line = pocket_ui::type_scale::line(NU_SMALL, 10);
                let needs_room = rows.iter().any(|&i| self.text.width(Face::Body, NU_SMALL, SETTINGS[i].2) > beside);
                let reserved = if needs_room { 2 * desc_line + 14 } else { 0 };
                // More rows than fit above the hints: a window that follows the selection, with a bar at the edge.
                let visible = (((c.h - pad - NU_PILL - 16 - reserved - top) / row) as usize).max(1);
                let first = window_start(at, visible, len);
                if len > visible {
                    let track = visible as i32 * row;
                    pocket_ui::kit::scrollbar(c, top, track, first, visible, len);
                }
                // The band and the pill glide to the selection, the list scrolls under its edges (`app_motion`).
                let now = Instant::now();
                let (first_f, hl) = self.motion.list_at(app_motion::List::Settings, first, at, now);
                let sel_w = self.text.width(Face::Body, font, SETTINGS[sel].0) + 2 * NU_BTN_PAD;
                let pill_w = self.motion.width_at(app_motion::List::Settings, sel_w, now);
                let mut band = c.rows(top, top + visible as i32 * row);
                let hy = ((hl - first_f) * row as f32).round() as i32;
                band.round_rect(pad, hy + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                band.round_rect(pad, hy + 4, pill_w, row - 8, (row - 8) / 2, pal().select);
                for (local, k) in app_motion::rows_in_view(first_f, visible, len) {
                    let i = rows[local];
                    let name = SETTINGS[i].0;
                    let y = (k * row as f32).round() as i32;
                    if (0.0..visible as f32 - 0.5).contains(&k) {
                        self.hits.row(Area::new(pad, top + y, right - pad, row), local);
                        self.motion.row(pad, top + y, right - pad, row);
                    }
                    let base = y + row / 2 + pocket_ui::type_scale::mid(font);
                    let name_w = self.text.draw(&mut band, Face::Body, font, pad + NU_BTN_PAD, base, mix(pal().text, pal().ink, (app_motion::under(local, hl) * 255.0) as u32), name);
                    // The row's value, from memory, cut to the room the name leaves (`app_settings_groups`).
                    let value = self.settings_row_value(i, sel, right - NU_BTN_PAD - (pad + NU_BTN_PAD + name_w + 40));
                    self.text.draw_right(&mut band, Face::Body, font, right - NU_BTN_PAD, base, pal().text, &value);
                }
                self.hits.list(at, len, 0.0, row as f32);
                // Left of the hints, on their line, so a long one doesn't run under them; else over them in the room
                // the category kept (`reserved`), cut short only past two lines.
                if self.text.width(Face::Body, NU_SMALL, SETTINGS[sel].2) <= beside {
                    let desc = self.text.fit(Face::Body, NU_SMALL, SETTINGS[sel].2, beside);
                    self.text.draw(c, Face::Body, NU_SMALL, desc_x, c.h - pad - NU_PILL / 2 + pocket_ui::type_scale::mid(NU_SMALL), pal().text, &desc);
                } else {
                    let lines = self.text.wrap_at_most(Face::Body, NU_SMALL, SETTINGS[sel].2, c.w - 2 * desc_x, 2);
                    let last = c.h - pad - NU_PILL - 24;
                    for (n, line) in lines.iter().enumerate() {
                        let base = last - (lines.len() - 1 - n) as i32 * desc_line;
                        self.text.draw(c, Face::Body, NU_SMALL, desc_x, base, pal().text, line);
                    }
                }
            }
            Some(SettingsPage::Updates) => self.paint_updates(c, top, row, font),
            Some(SettingsPage::Battery(range)) => self.paint_battery(c, range),
            Some(SettingsPage::Manual) => self.paint_manual(c),
            Some(SettingsPage::About) => {
                let row_h = row;
                let games: usize = self.systems.iter().map(|s| s.games.len()).sum();
                // Free space (statvfs on the card) and the address (`ip`, a process): read on a thread every few
                // seconds while the page is drawn, "…" until the first read is in (`ABOUT`).
                let proxy = self.proxy.clone();
                ABOUT.refresh(ABOUT_EVERY, || (free_space("/storage"), pocket_ui::wifi::ip_now()), move || {
                    let _ = proxy.send_event(UserEvent::SettingsValues);
                });
                let (free, network) = match ABOUT.get() {
                    Some((free, ip)) => (free.unwrap_or_else(|| "unknown".into()), ip.map_or("Not connected".into(), |ip| format!("Connected, {ip}"))),
                    None => ("…".to_string(), "…".to_string()),
                };
                let mut lines = vec![
                    ("Version", format!("pocket-ui {}", env!("CARGO_PKG_VERSION"))),
                    ("Games", format!("{games} on {} systems", self.systems.len())),
                    ("Free space", free),
                    ("Network", network),
                    // The status corner's figure (its thread's memory): no sysfs read in every frame of this page.
                    ("Battery", pocket_ui::status_corner::status().battery.map_or_else(|| "unknown".into(), |b| b.label())),
                    // Not shared over the network (only the games are): see the troubleshooting page for SSH.
                    ("Logs", self.data_dir.join(pocket_ui::logs::DIR_NAME).display().to_string()),
                ];
                // On one of the Mini V2's untested siblings: which one, and that it is experimental (docs/DEVICES.md).
                // The Mini V2 and an unknown device show no such row.
                if let Some(device) = pocket_ui::device::current().about_line() {
                    lines.insert(1, ("Device", device));
                }
                // With the Device row on a short screen (the Mini V1's 960 lines, larger text) the facts move closer
                // together, so the two rows under them and two lines of their help stay above the hints; where
                // everything fits (the Mini V2, any text size) they keep the row's height.
                let below = 2 * row + 20 + pocket_ui::type_scale::px(NU_SMALL) as i32 / 2 + pocket_ui::type_scale::line(NU_SMALL, 12);
                let fit_for = |n: usize| (c.h - NU_PAD - NU_PILL - 8 - top - below) / (n as i32 + 1);
                // Where the licence texts are (put there by the install zip and the image: pocket-ui's licence, the
                // credits, the crates' and the fonts' texts). Its row comes only where the facts keep three quarters
                // of the row's height with it; on the shortest page (the Device row, large German text) the help under
                // the rows needs the room, and the manual names the folder.
                if fit_for(lines.len() + 1) >= row * 3 / 4 {
                    lines.push(("Licences", "/storage/pocket-ui/licences".to_string()));
                }
                let fit = fit_for(lines.len());
                let row = row.min(fit).max(row * 3 / 4);
                for (i, (name, value)) in lines.iter().enumerate() {
                    let base = top + i as i32 * row + row / 2 + pocket_ui::type_scale::mid(font);
                    let name_w = self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().dust, name);
                    // A long value (the logs' folder) is cut with "…" before it reaches its name.
                    let value = self.text.fit(Face::Body, font, value, right - NU_BTN_PAD - (pad + NU_BTN_PAD + name_w + 40));
                    self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, &value);
                }
                // Report a problem, and ROCKNIX's own menu behind the warning (`app_report`, `app_rescue`).
                self.paint_about_rows(c, top + (lines.len() as i32 + 1) * row, row_h, font);
            }
            Some(SettingsPage::Groups(_) | SettingsPage::Store | SettingsPage::Starter | SettingsPage::Sync | SettingsPage::Leds(_) | SettingsPage::NightLight(_) | SettingsPage::Tailscale | SettingsPage::Web | SettingsPage::Bluetooth | SettingsPage::PerfFan | SettingsPage::Curator | SettingsPage::Achievements | SettingsPage::Tools | SettingsPage::PcStreaming | SettingsPage::Backup | SettingsPage::Storage | SettingsPage::Gamelist | SettingsPage::Screenshots | SettingsPage::SleepPower) | None => {}
        }
    }

    /// Settings > Battery: the charge now and what it means in time, a graph of the last hours, what the games cost,
    /// and where the times of sleep are set now. Only says what the samples support (`battery::estimate`).
    fn paint_battery(&mut self, c: &mut Canvas, range: usize) {
        use pocket_ui::battery;
        let (pad, font) = (NU_PAD, 36u32);
        let (left, right, top) = (NU_PAD + NU_BTN_PAD, c.w - NU_PAD - NU_BTN_PAD, NU_PAD + NU_PILL + 12);
        let now = recents::now_secs();
        let hours = battery::RANGES[range.min(battery::RANGES.len() - 1)];
        let history = pocket_ui::battery_history::view();
        let samples = history.recent(now, battery::KEEP);
        let current = battery::read(&battery::dir(), now);
        let (headline, sub) = battery::describe(current.as_ref(), pocket_ui::battery_estimate::estimate(samples, now).as_ref());

        let mut x = left;
        if let Some(s) = &current {
            x += self.text.draw(c, Face::Title, 96, left, top + 92, pal().chalk, &format!("{}%", s.capacity)) + 48;
        }
        self.text.draw(c, Face::Body, font, x, top + 44, pal().text, &headline);
        let sub = self.text.fit(Face::Body, NU_SMALL, &sub, right - x);
        self.text.draw(c, Face::Body, NU_SMALL, x, top + 90, pal().quiet, &sub);

        // What the games cost: the game running or played last first, then the menus for comparison.
        let mut draws = pocket_ui::battery_estimate::draws(&history, now);
        let running = self.running.as_ref().map(|r| r.system.clone());
        draws.sort_by_key(|d| (std::cmp::Reverse(d.tag.is_some() && d.tag == running), std::cmp::Reverse(d.tag.is_some()), std::cmp::Reverse(d.last)));
        draws.truncate(2);
        // When the screen goes off and the Pocket sleeps is set elsewhere now: one line says where.
        let pointer = self.text.wrap(Face::Body, NU_SMALL, SLEEP_SETTINGS_POINTER, c.w - 4 * pad);
        let (draw_step, desc_step) = (pocket_ui::type_scale::line(NU_SMALL, 14), pocket_ui::type_scale::line(NU_SMALL, 10));
        // Everything under the graph, from its bottom edge to the pointer's last line, and the hint bar's top: the
        // graph gives up height when Large text needs it (it stays at least `MIN_GRAPH`).
        const MIN_GRAPH: i32 = 140;
        let under = 80 + 12 + draws.len() as i32 * draw_step + 60 + (pointer.len() as i32 - 1).max(0) * desc_step + desc_step / 2;
        let hints_top = c.h - pad - NU_PILL - 8;

        // The graph, with room for the % labels on its left and the hours under it.
        let (gx, gy) = (left + 100, top + 130);
        let (gw, gh) = (right - gx, (hints_top - under - gy).clamp(MIN_GRAPH, 300));
        battery::paint_graph(c, &mut self.text, samples, now, hours, (gx, gy, gw, gh));
        let below = gy + gh + 80;
        if let Some(s) = &current {
            let details = battery::details(s);
            self.text.draw(c, Face::Body, NU_SMALL, gx, below, pal().dust, &details);
        }
        // Read on the sampler's thread: until it is, the page says so instead of "no samples".
        let span = if !pocket_ui::battery_history::loaded() {
            pocket_ui::battery_history::LOADING.to_string()
        } else if samples.is_empty() {
            "No samples yet".to_string()
        } else {
            format!("Last {hours} h")
        };
        self.text.draw_right(c, Face::Body, NU_SMALL, right, below, pal().dust, &span);

        let capacity = current.as_ref().filter(|s| !s.plugged()).map(|s| s.capacity);
        let mut ly = below + 12;
        for d in &draws {
            let name = d.tag.as_deref().map_or("The menus".to_string(), |t| system_short(&self.systems, t));
            let line = self.text.fit(Face::Body, NU_SMALL, &battery::draw_line(&name, d, capacity), right - gx);
            ly += draw_step;
            self.text.draw(c, Face::Body, NU_SMALL, gx, ly, pal().quiet, &line);
        }

        let mut ly = ly + 60;
        for line in &pointer {
            let lw = self.text.width(Face::Body, NU_SMALL, line);
            self.text.draw(c, Face::Body, NU_SMALL, (c.w - lw) / 2, ly, pal().quiet, line);
            ly += desc_step;
        }
        self.nu_hints(c, &[("L1 R1", "Hours"), ("B", "Back")]);
    }

    /// Settings > Manual: one page of the user guide (`pocket_ui::manual`), under the title and over the hints.
    fn paint_manual(&mut self, c: &mut Canvas) {
        use pocket_ui::manual;
        let (pad, left) = (NU_PAD, NU_PAD + NU_BTN_PAD);
        let top = pad + NU_PILL + 12;
        let view_h = c.h - pad - NU_PILL - 16 - top;
        // The guide's pages stay English for now (docs/i18n-glossary.md); the title and hints around them don't.
        self.text.set_translate(false);
        self.manual.paint(c, &mut self.text, left, top, c.w - 2 * left, view_h);
        self.text.set_translate(true);
        let title_w = self.text.width(Face::Title, 50, "Manual");
        let place = format!("{} of {}   {}", self.manual.page + 1, manual::PAGES.len(), manual::title(manual::PAGES[self.manual.page].1));
        let x = left + title_w + 36;
        // Not under the clock and battery pill.
        let place = self.text.fit(Face::Body, NU_SMALL, &place, c.w - pad - 260 - x);
        self.text.draw(c, Face::Body, NU_SMALL, x, pad + 56, pal().dust, &place);
        let (sel, len) = self.manual.touch_rows();
        self.hits.list(sel, len, 0.0, manual::STEP as f32);
        self.nu_hints(c, &[("L1", "Previous"), ("R1", "Next"), ("B", "Back")]);
    }

    /// Input while the manual is open: Up/Down and the stick scroll (held: faster), Left/Right a screen,
    /// L1/R1 the previous and next page, B back to Settings.
    fn manual_input(&mut self, pe: PadEvent) {
        let now = Instant::now();
        match pe {
            PadEvent::Button { button, pressed: true, .. } => match button {
                b if b == CANCEL => {
                    self.manual.set_velocity(0.0, now);
                    self.manual_hold = None;
                    // Back to Settings, or to the setup step it was opened from.
                    self.settings_page = if std::mem::take(&mut self.manual_from_setup) { None } else { Some(SettingsPage::List(settings_row("Manual"))) };
                }
                Button::L1 => self.manual.turn(-1),
                Button::R1 => self.manual.turn(1),
                Button::Left => self.manual.screen(-1),
                Button::Right => self.manual.screen(1),
                Button::Up | Button::Down => {
                    let dir = if button == Button::Up { -1 } else { 1 };
                    self.manual.set_velocity(0.0, now);
                    self.manual.scroll_by(dir * pocket_ui::manual::STEP);
                    // Held on, it goes on by itself after a moment.
                    self.manual_hold = Some((button, now + REPEAT_DELAY));
                }
                _ => {}
            },
            PadEvent::Button { button, pressed: false, .. } if matches!(button, Button::Up | Button::Down) => {
                if self.manual_hold.is_some_and(|(b, _)| b == button) || self.manual.moving() {
                    self.manual_hold = None;
                    self.manual.set_velocity(0.0, now);
                }
            }
            // Either stick: all the way down is `MANUAL_STICK` pixels a second.
            PadEvent::Axis { axis: Axis::LeftY | Axis::RightY, value, .. } => {
                let v = if value.abs() < pocket_ui::pad::STICK_DEADZONE { 0.0 } else { value * MANUAL_STICK };
                self.manual_hold = None;
                self.manual.set_velocity(v, now);
            }
            _ => {}
        }
    }

    fn save_settings(&self) {
        Settings {
            theme: self.theme,
            onboarded: self.onboarded,
            update_url: self.update_url.clone(),
            store_url: self.store.url.clone(),
            hidden_systems: self.hidden.clone(),
            streaming_on_home: self.streaming_on,
            appearance: self.appearance,
            on_start: self.on_start,
            forget_where_quit: !self.remember_quit,
            button_layout: self.button_layout,
            motion: pocket_ui::motion::level(),
            frames: self.frames,
            over_game: self.over_game,
            seen_version: self.tour.seen_version.clone(),
            tour_later: self.tour.later.clone(),
            language: self.language,
            box_art_only: !self.pictures.wanted(),
            art_off: self.art_off,
            text_size: pocket_ui::type_scale::get(),
            animations: self.animations,
            no_launch_animation: false,
            details_page: !self.game_box,
        }
        .save(&self.data_dir);
    }

    /// Settings → Look & feel → Text size: the next frame is painted at `size` (the text painter scales every size,
    /// `pocket_ui::type_scale`), and it is kept. The new size's glyphs are drawn ahead on a thread, as at the start.
    fn set_text_size(&mut self, size: pocket_ui::type_scale::TextSize) {
        pocket_ui::type_scale::set(size);
        self.text.prewarm();
        self.library_prewarm_names();
        self.save_settings();
    }

    /// Settings → Language and the welcome step: the next frame is painted in `lang` (the text painter translates,
    /// `pocket_ui::i18n`), and it is kept.
    fn set_language(&mut self, lang: pocket_ui::i18n::Lang) {
        pocket_ui::i18n::set(lang);
        self.language = Some(lang);
        self.save_settings();
    }

    /// Settings → Playing → Button layout: the other layout, used from the next button on (the press that switched it
    /// is let go as the role it was pressed as, `button_layout::Remap`) and kept.
    fn switch_button_layout(&mut self) {
        self.button_layout = self.button_layout.toggled();
        pocket_ui::button_layout::set(self.button_layout);
        self.save_settings();
    }

    fn manifest_url(&self) -> String {
        self.update_url.clone().unwrap_or_else(|| update::DEFAULT_URL.to_string())
    }

    /// What the Updates page offers right now, in display order.
    fn update_actions(&self) -> Vec<UpdateAction> {
        let m = self.updates.manifest.as_ref();
        let mut actions = Vec::new();
        if m.and_then(|m| m.app.as_ref()).is_some_and(|a| update::newer(&a.version, update::app_version())) {
            actions.push(UpdateAction::InstallApp);
        }
        if update::staged_os().is_some() {
            actions.push(UpdateAction::CancelOs);
        } else if m.and_then(|m| m.os.as_ref()).is_some_and(|o| update::newer(&o.version, &update::os_version().unwrap_or_default())) {
            actions.push(UpdateAction::StageOs);
        }
        actions.push(UpdateAction::Check);
        actions
    }

    /// Starts `action` on a worker thread; results come back as `UserEvent::Update`.
    fn update_action(&mut self, action: UpdateAction) {
        if self.updates.busy.is_some() {
            return;
        }
        let url = self.manifest_url();
        let proxy = self.proxy.clone();
        let send = move |e| {
            let _ = proxy.send_event(UserEvent::Update(e));
        };
        let manifest = self.updates.manifest.clone().unwrap_or_default();
        self.updates.message = None;
        self.updates.done = 0;
        match action {
            UpdateAction::Check => {
                self.updates.start("Checking for updates".into(), 0, None);
                std::thread::spawn(move || send(UpdateEvent::Checked(update::fetch_manifest(&url))));
            }
            UpdateAction::InstallApp => {
                let Some(rel) = manifest.app else { return };
                let cancel = update::Cancel::new();
                self.updates.start(format!("Installing pocket-ui {}", rel.version), rel.size, Some(cancel.clone()));
                std::thread::spawn(move || {
                    let result = update::install_app(&rel, &url, std::path::Path::new(update::APP_ROOT), &cancel, &|n| send(UpdateEvent::Progress(n)));
                    send(UpdateEvent::Done(result.map(|_| true)));
                });
            }
            UpdateAction::StageOs => {
                let Some(rel) = manifest.os else { return };
                let cancel = update::Cancel::new();
                self.updates.start(format!("Getting system {}", rel.version), rel.size, Some(cancel.clone()));
                std::thread::spawn(move || {
                    send(match update::stage_os(&rel, &url, &cancel, &|n| send(UpdateEvent::Progress(n))) {
                        Ok(note) => UpdateEvent::OsStaged(note),
                        Err(e) => UpdateEvent::Done(Err(e)),
                    });
                });
            }
            UpdateAction::CancelOs => {
                update::cancel_os();
                self.updates.message = Some("The system update was removed.".into());
            }
        }
    }

    fn update_event(&mut self, e: UpdateEvent) {
        match e {
            UpdateEvent::Progress(n) => {
                self.updates.done = n;
                return;
            }
            UpdateEvent::Checked(Ok(m)) => {
                self.updates.manifest = Some(m);
                let none = self.update_actions() == [UpdateAction::Check];
                self.updates.message = none.then(|| "Everything is up to date.".to_string());
            }
            UpdateEvent::Checked(Err(e)) | UpdateEvent::Done(Err(e)) => self.updates.message = Some(e),
            UpdateEvent::Done(Ok(true)) => {
                self.updates.message = Some("Installed. Restarting…".into());
                self.updates.restart_at = Some(Instant::now() + Duration::from_millis(1500));
            }
            UpdateEvent::Done(Ok(false)) | UpdateEvent::OsStaged(None) => self.updates.message = Some(os_staged_message(None)),
            UpdateEvent::OsStaged(Some(note)) => self.updates.message = Some(os_staged_message(Some(&note))),
        }
        self.updates.finish();
        self.updates.sel = 0;
    }

    /// Versions, the offered actions, and progress or the last result at the bottom.
    fn paint_updates(&mut self, c: &mut Canvas, top: i32, row: i32, font: u32) {
        let (pad, right) = (NU_PAD, c.w - NU_PAD);
        let m = self.updates.manifest.clone();
        let app_line = match m.as_ref().and_then(|m| m.app.as_ref()) {
            Some(a) if update::newer(&a.version, update::app_version()) => format!("{}, {} available", update::app_version(), a.version),
            _ => update::app_version().to_string(),
        };
        let os = update::os_version().unwrap_or_else(|| "unknown".into());
        let os_line = match (update::staged_os(), m.as_ref().and_then(|m| m.os.as_ref())) {
            (Some(_), _) => format!("{os}, update waiting for restart"),
            (None, Some(o)) if update::newer(&o.version, &os) => format!("{os}, {} available", o.version),
            _ => os,
        };
        for (i, (name, value)) in [("pocket-ui", app_line), ("System", os_line)].iter().enumerate() {
            let base = top + i as i32 * row + row / 2 + pocket_ui::type_scale::mid(font);
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().dust, name);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, value);
        }
        let actions = self.update_actions();
        let busy = self.updates.busy.is_some();
        for (i, action) in actions.iter().enumerate() {
            let label = match action {
                UpdateAction::Check => "Check again".to_string(),
                UpdateAction::InstallApp => format!("Install pocket-ui {}", m.as_ref().and_then(|m| m.app.as_ref()).map_or("", |a| &a.version)),
                UpdateAction::StageOs => format!("Get system {}", m.as_ref().and_then(|m| m.os.as_ref()).map_or("", |o| &o.version)),
                UpdateAction::CancelOs => "Remove the waiting system update".to_string(),
            };
            let y = top + (i as i32 + 2) * row + row / 3;
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            if i == self.updates.sel && !busy {
                let w = self.text.width(Face::Body, font, &label) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, &label);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if busy { pal().dust } else { pal().text }, &label);
            }
        }
        // Bottom: progress while busy, else the last result, else the notes of the selected action.
        let footer = match &self.updates.busy {
            Some((label, total)) if *total > 0 => {
                let (w, y) = (c.w - 2 * pad - 2 * NU_BTN_PAD, c.h - pad - NU_PILL - 90);
                let frac = (self.updates.done as f32 / *total as f32).clamp(0.0, 1.0);
                c.round_rect(pad + NU_BTN_PAD, y, w, 12, 6, pal().keycap);
                c.round_rect(pad + NU_BTN_PAD, y, ((w as f32 * frac) as i32).max(12), 12, 6, pal().band);
                format!("{label}, {}%", (frac * 100.0) as u32)
            }
            Some((label, _)) => format!("{label}…"),
            None => self.updates.message.clone().unwrap_or_else(|| match actions.get(self.updates.sel) {
                Some(UpdateAction::InstallApp) => m.as_ref().and_then(|m| m.app.as_ref()).map(|a| a.notes.clone()).unwrap_or_default(),
                Some(UpdateAction::StageOs) => m.as_ref().and_then(|m| m.os.as_ref()).map(|o| o.notes.clone()).unwrap_or_default(),
                _ => String::new(),
            }),
        };
        // Wrapped to the screen width and kept above the button hints.
        let max_w = c.w - 2 * pad - 2 * NU_BTN_PAD;
        let mut lines: Vec<String> = Vec::new();
        // Translated as a whole before it is broken into lines (`i18n`).
        let footer = self.text.translated(&footer).to_string();
        for word in footer.split_whitespace() {
            match lines.last_mut() {
                Some(line) if self.text.width(Face::Body, NU_SMALL, &format!("{line} {word}")) <= max_w => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_string()),
            }
        }
        let mut y = c.h - pad - NU_PILL - 30 - 40 * (lines.len() as i32 - 1).max(0);
        for line in &lines {
            let w = self.text.width(Face::Body, NU_SMALL, line);
            self.text.draw(c, Face::Body, NU_SMALL, (c.w - w) / 2, y, pal().text, line);
            y += 40;
        }
        let hints: &[(&str, &str)] = if self.updates.can_cancel() {
            &[("B", "Back"), ("A", "Cancel")]
        } else if busy {
            &[("B", "Back")]
        } else {
            // "Choose", not "Select": that is the button's name, and the German table has one line per word (`i18n`).
            &[("B", "Back"), ("A", "Choose")]
        };
        self.nu_hints(c, hints);
    }

    fn finish_setup(&mut self) {
        self.setup = None;
        // The first start saw this version: "New in this version" begins with the next one (`app_tour`).
        if !self.onboarded {
            self.tour.seen_version = Some(update::app_version().to_string());
        }
        self.onboarded = true;
        self.save_settings();
        // Games may have been copied during setup.
        if let Ok(systems) = library::load_shown(&self.es_systems, &self.data_dir) {
            // The next start shows them from the cache (`library_cache`), written on a thread.
            pocket_ui::library_cache::store_soon(&self.data_dir, &self.es_systems, systems.clone());
            self.systems = systems;
            self.recents.follow_library(&self.systems);
            self.curator.library_changed();
            self.find_library_changed();
        }
        // A gamelist copied with the games (`app_gamelist`).
        self.gamelist_refresh("games added");
        self.covers.clear();
        self.pictures_changed();
        self.look_previews = None;
        // From Settings the list stays open; after first-start setup Home starts at the top.
        if self.settings_page.is_none() {
            self.screen = Screen::Home;
            self.sel = 0;
        }
        self.rebuild();
        if self.settings_page.is_none() {
            self.sel = Item::first_row(&self.items);
            self.prev_sel = self.sel;
        }
        // New games, or Wi-Fi just connected: their box art comes by itself.
        self.auto_covers("library read again");
    }

    /// Settings > Games > Extras > Pictures as it is now.
    fn art(&self) -> pocket_ui::settings::Art {
        pocket_ui::settings::Art::from_flags(!self.pictures.wanted(), self.art_off)
    }

    /// Starts the background cover download when it may run (`AutoCovers::decide`): online, no game running,
    /// nothing scraping already, games without covers. Quiet: the shelf shows "Covers 3 of 12" at most.
    fn auto_covers(&mut self, why: &str) {
        use pocket_ui::auto_covers::Decision;
        // The games are recognised by their checksum first (`app_identify`): covers and descriptions are asked for
        // under those names when it is through, which calls this again.
        if self.identify(why) {
            println!("covers ({why}): after the games are recognised");
            return;
        }
        // The game card's descriptions come at the same moments (`app_card`).
        self.auto_texts(why);
        // The first start gets there after its Wi-Fi step, not before.
        if self.setup.as_ref().is_some_and(|s| !s.past_wifi()) {
            return;
        }
        // Pictures: Off. (Everything above was local or the descriptions' own job.)
        if self.art_off {
            println!("covers ({why}): off");
            return;
        }
        let in_game = self.running.is_some();
        let busy = pocket_ui::covers::busy();
        let online = !in_game && !busy && !self.cover_run.running() && pocket_ui::wifi::ip().is_some();
        let games = if online { self.all_games() } else { Vec::new() };
        let missing = if online { self.cover_run.missing(&self.data_dir, &games) } else { Vec::new() };
        // Screenshots and title screens follow the box art, unless Settings > Games > Extras says box art only.
        let pictures = self.cover_run.missing_pictures(&self.data_dir, &games, self.pictures.wanted());
        let decision = self.cover_run.decide(online, in_game, missing.len() + pictures.len(), busy);
        println!("covers ({why}): {decision:?}, {} without a cover, {} without pictures", missing.len(), pictures.len());
        if decision != Decision::Start {
            return;
        }
        let Some(claim) = pocket_ui::covers::claim() else { return };
        let stop = self.cover_run.begin_run(missing.len(), pictures.len());
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        std::thread::spawn(move || {
            let _claim = claim;
            let progress = |done, total, found| {
                let _ = proxy.send_event(UserEvent::Covers(CoversEvent::Progress { done, total, found }));
            };
            let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
            let run = pocket_ui::game_pictures::run_until(&pocket_ui::game_pictures::Libretro, &data_dir, &missing, Some(&pictures), progress, stopped);
            let (total, stopped) = (run.covers_total + run.pictures_total, run.stopped());
            let _ = proxy.send_event(UserEvent::Covers(CoversEvent::Done {
                looked_up: run.covers_looked_up,
                pictures_looked_up: run.pictures_looked_up,
                found: run.found,
                total,
                stopped,
            }));
        });
    }

    fn covers_event(&mut self, e: CoversEvent) {
        // Extras > Cover art shows the run too, while it is open.
        let shown = match e {
            CoversEvent::Progress { done, total, found } => {
                self.cover_run.progress(done, total);
                (done, total, found)
            }
            CoversEvent::Done { looked_up, pictures_looked_up, found, total, stopped } => {
                println!(
                    "covers: {found} found, {} of {total} looked up{}",
                    looked_up.len() + pictures_looked_up.len(),
                    if stopped { ", stopped for a game" } else { "" }
                );
                self.cover_run.finish_run(looked_up, pictures_looked_up, stopped);
                // New box art and pictures show at once.
                self.covers.clear();
                self.pictures_changed();
                self.cabinet.covers_changed();
                self.look_previews = None;
                (total.max(1), total.max(1), found)
            }
        };
        if let Some(setup) = self.setup.as_mut() {
            setup.on_event(SetupEvent::Covers { done: shown.0, total: shown.1, found: shown.2 });
        }
    }

    fn animating(&self) -> bool {
        self.setup.as_ref().is_some_and(|s| s.animating()) || (self.marquee.animating() && (self.running.is_none() || self.switcher.is_some())) || self.switcher.as_ref().is_some_and(|s| s.animating()) ||
        self.pull_moving(Instant::now()) || self.motion_animating(Instant::now()) || self.toast.as_ref().is_some_and(|(_, t)| t.elapsed() < TOAST) || self.sync_note_active() || self.tailscale_animating() || self.bluetooth_animating() || self.tools_animating() || self.pc_streaming_animating() || self.backup_animating() || (self.updates.restart_at.is_some() && self.running.is_none()) || self.notice.as_ref().is_some_and(Notice::active) || self.launch_at.is_some_and(|t| t.elapsed() < FADE + self.cabinet.launch_delay() + LAST_FADE_FRAME) || (self.theme == Theme::Cabinet && self.running.is_none() && self.cabinet.animating(Instant::now())) || (self.theme == Theme::Shelf && self.running.is_none() && self.shelf_media_animating(Instant::now())) || self.stream_message_due() || self.exit_since.is_some() || self.sys.animating(Instant::now()) || (self.manual.moving() && self.settings_page == Some(SettingsPage::Manual)) || self.appearance_animating() || self.card.as_ref().is_some_and(|o| o.card.boxed.as_ref().is_some_and(|b| b.animating(Instant::now()))) || self.find_animating() || self.boot_fade.active()
    }

    fn openness(&self, i: usize) -> f32 {
        let e = self.pull_progress(Instant::now());
        if i == self.sel {
            e
        } else if i == self.prev_sel {
            1.0 - e
        } else {
            0.0
        }
    }

    /// The logical canvas pocket-ui paints: the supported layout for the window's size; before there is one (and in `--snapshot`) the snapshot's or
    /// the device's (`device::Device::screen`; the Mini V2's 1240x1080 when unknown).
    fn screen_size(&self) -> (u32, u32) {
        self.window.as_ref().map_or_else(snapshot_dims, |w| pocket_ui::scaled_layout::logical_size((w.inner_size().width, w.inner_size().height)))
    }

    /// Cozy or Quick: the player's choice, else the look's own (`Animations::default_for`).
    fn animations_in_use(&self) -> pocket_ui::settings::Animations {
        self.animations.unwrap_or_else(|| pocket_ui::settings::Animations::default_for(self.theme))
    }

    fn cozy(&self) -> bool {
        self.animations_in_use() == pocket_ui::settings::Animations::Cozy
    }

    /// The other of Cozy and Quick, kept: from now on the look no longer decides.
    fn switch_animations(&mut self) {
        self.animations = Some(self.animations_in_use().next());
        pocket_ui::motion::set_cozy(self.cozy());
        self.save_settings();
    }

    fn draw(&mut self) {
        let Some(window) = self.window.clone() else { return };
        // Cozy slows every timed animation a little (`motion::COZY_SLOWER`); the look can change it.
        pocket_ui::motion::set_cozy(self.cozy());
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        // Just put in front of a game: not at the old size, which would hold sway's resize (`app_menu_open`).
        if self.wait_for_full_size(w.get(), h.get()) {
            return;
        }
        // Taken out for the frame so painting can borrow the rest of `self`. A surface or buffer the
        // compositor refuses skips this frame and is made anew; it never ends the program (K5).
        let mut surface = std::mem::replace(&mut self.surface, pocket_ui::frame::Presenter::empty());
        let started = Instant::now();
        // A page that has just opened slides in (`app_motion`).
        self.motion_frame_begin(started);
        let drawn = surface.draw_scaled(w, h, &mut |pixels, logical_w, logical_h| {
            // Between dark and light (`app_appearance`) the frame is painted in both and the two pictures are
            // mixed, so text never passes through the grey where it would match its background.
            self.paint_in_appearance(pixels, logical_w, logical_h, &mut |app, c| {
                app.paint(c);
                app.motion_after_paint(c);
                app.sys.paint(c, &mut app.text, app.menu.is_none());
                // The quick settings' pill is the status corner too: its minute and its thread (`app_status_corner`).
                if app.sys.is_open() {
                    app.status_corner();
                }
                app.paint_leave(c);
            });
            // After a boot: the first frames come up from black (`boot_fade`).
            if let Some(dark) = self.boot_fade.darkness(Instant::now()) {
                Canvas::new(pixels, logical_w, logical_h).fade(dark);
            }
            // The Wi-Fi bench reads what the player sees from a running pocket-ui (POCKETUI_VISIBLE_TEXT, as the
            // snapshots and the journey driver write it; docs/WIFI-BENCH.md). Nothing without the variable.
            if pocket_ui::text::boxes::on() {
                pocket_ui::text::boxes::finish(pixels, logical_w as i32, logical_h as i32);
            }
        });
        self.surface = surface;
        if !drawn {
            return;
        }
        self.frame_at = Instant::now();
        // Speed guard: what is on screen and the last input it already shows (docs/LATENCY.md).
        if pocket_ui::trace::on() {
            self.trace_frame(self.frame_at - started, w.get(), h.get());
        }
        self.motion_frame_end(self.frame_at - started);
        if let Some((seq, read, shown)) = self.menu.as_mut().and_then(|m| m.live.shown()) {
            println!("preview {seq}: read after {read:.0} ms, on screen {shown:.0} ms after the change");
        }
        if !self.drew_first {
            self.drew_first = true;
            pocket_ui::startup::mark("drawn");
            println!("first frame after {:?} ({} ms)", self.started.elapsed(), pocket_ui::startup::report(self.started));
            // What the start put off until the first frame is on screen (docs/ARCHITECTURE.md, Start).
            self.after_first_frame();
        }
    }

    fn paint(&mut self, c: &mut Canvas) {
        self.hits.clear();
        // For the snapshot checks of Settings → Text size (`text::boxes`; nothing unless they run).
        pocket_ui::text::boxes::reset();
        // Set again when this frame paints the status corner (`app_status_corner`).
        self.corner_minute = None;
        // Powering off: "Saving…", "Switching off…" over everything.
        if let Some(msg) = self.sleep.overlay() {
            c.clear(pal().void);
            self.centered(c, 0, 0, c.w, c.h, &msg);
            return;
        }
        // PC streaming looks for the PC for up to 12 s before Moonlight's own window comes up over this one; a tool's
        // window takes a moment too.
        if let Some(run) = self.running.as_ref().filter(|r| r.is_app() && self.launch_at.is_some_and(|t| t.elapsed() >= FADE)) {
            let msg = if run.emulator == pocket_ui::tools::EMULATOR { format!("Starting {}", run.name) } else { "Looking for your PC".to_string() };
            c.clear(pal().void);
            self.centered(c, 0, 0, c.w, c.h, &msg);
            return;
        }
        // The manual, also over the first-start setup.
        if self.settings_page == Some(SettingsPage::Manual) {
            self.in_column(c, |app, c| app.paint_settings(c));
            return;
        }
        // The feature cards: Settings → Tour, "New in this version" (`app_tour`).
        if self.tour.is_open() {
            self.paint_tour(c);
            return;
        }
        self.marquee.begin_frame();
        // The look step of setup previews the real shelf in each theme; the one shown is the one in use.
        if let Some(i) = self.setup.as_ref().and_then(|s| s.previewing()) {
            // Painted in the appearance the screen is going to, also while it fades there.
            let light = self.fader.light();
            // And in the interface's language (the welcome step before it can change it).
            let size = pocket_ui::type_scale::get();
            if self.look_previews.is_none() || self.look_previews_light != light || self.look_previews_lang != pocket_ui::i18n::current() || self.look_previews_size != size {
                self.look_previews = Some(pocket_ui::palette::scoped(if light { 255 } else { 0 }, || self.render_look_previews()));
                self.look_previews_light = light;
                self.look_previews_lang = pocket_ui::i18n::current();
                self.look_previews_size = size;
            }
            let (previews, sample) = self.look_previews.take().unwrap_or_default();
            if let Some(setup) = self.setup.as_mut() {
                setup.paint_look(c, &mut self.text, &previews, sample);
            }
            self.look_previews = Some((previews, sample));
            self.theme = Theme::ALL[i];
            return;
        }
        if self.setup.is_some() {
            self.in_column(c, |app, c| {
                if let Some(setup) = app.setup.as_mut() {
                    setup.paint(c, &mut app.text);
                }
            });
            return;
        }
        if self.settings_page.is_some() {
            self.in_column(c, |app, c| app.paint_settings(c));
            return;
        }
        if self.port_files.is_some() && self.running.is_none() {
            self.paint_port_files(c);
            return;
        }
        // Curator's pages, over the card or the shelf (`app_curator`).
        if self.curator.page.is_some() && self.running.is_none() {
            self.paint_curator(c);
            return;
        }
        // "Runs on" (`app_emu_choice`), over the card or the console page it was opened from; over "This game" when that
        // is where it was opened.
        if self.emu_choice_on_shelf() {
            self.paint_emu_choice(c);
            return;
        }
        // "This game" (`app_this_game`), over the card.
        if self.this_game_on_shelf() {
            self.paint_this_game(c);
            return;
        }
        // A port's Performance & fan, over its card (`app_port_perf`).
        if self.port_perf_open() {
            self.paint_port_perf(c);
            return;
        }
        // A game's screenshots, over its card (`app_gallery`).
        if self.gallery_over_card() {
            self.paint_gallery(c);
            return;
        }
        // A save's two versions, over its card (`app_sync_conflicts`).
        if self.conflicts_over_card() {
            self.paint_conflicts(c);
            return;
        }
        if self.card.is_some() && self.running.is_none() {
            self.paint_card(c);
            return;
        }
        if self.console.is_some() && self.running.is_none() {
            self.paint_console(c);
            return;
        }
        // The search (`app_search`); a game's card opened from it is painted over it, above.
        if self.find.is_open() && self.running.is_none() {
            self.in_column(c, |app, c| app.paint_search(c));
            return;
        }
        if self.switcher.is_some() {
            self.paint_switcher(c);
            return;
        }
        if self.menu.is_some() && self.sys.quick_over_game() {
            // The quick settings opened from the pause menu: the paused game behind them, darkened only as far as
            // the rows stay readable, so a brightness change can be judged on the game (`quick_paint` draws the rows).
            self.paint_game_behind(c, VOID, QUICK_OVER_GAME_FADE);
            return;
        }
        if self.menu.is_some() {
            // The pause menu sits over the game picture: dark in both appearances.
            pocket_ui::palette::scoped(0, || self.paint_menu(c));
            return;
        }
        // The Shelf look is painted in the column on a wide screen; the other looks fill it their own way.
        if self.theme == Theme::Shelf {
            self.in_column(c, |app, c| app.paint_home(c, true));
        } else {
            self.paint_home(c, true);
        }
    }

    fn paint_home(&mut self, c: &mut Canvas, with_hints: bool) {
        c.clear(pal().void);
        // Set again by the Shelf look's label: the game card rises out of it (`app_motion`).
        self.motion.sel_y = None;
        // Where we are, left; time, Wi-Fi and battery, right (`app_status_corner`).
        let title = self.screen_title();
        let corner_left = self.paint_top_line(c, title.as_deref(), pal().dust);

        // A card with no games: a calm hint and the one thing to press (`app_starter`), not an empty shelf.
        if self.starter_home() {
            self.paint_starter_home(c, with_hints);
            return;
        }
        if self.items.is_empty() {
            // A Curator list with nothing on it (yet) says so in its own words.
            if let Some((head, line)) = self.curator_empty_text() {
                // Nothing matches (a magnifying glass by an empty box, `ui_art`); "Getting ready…" has no picture.
                let dy = if self.curator.index.is_some() {
                    pocket_ui::ui_art::above_words(c, pocket_ui::ui_art::Art::Search, MARGIN, false, EMPTY_ART_TOP, 300, 50)
                } else {
                    0
                };
                self.text.draw(c, Face::Title, 50, MARGIN, 300 + dy, pal().chalk, head);
                self.text.draw(c, Face::Body, 28, MARGIN, 350 + dy, pal().dust, line);
                if with_hints {
                    self.paint_hint_row(c, &[("B", "Back")]);
                }
                return;
            }
            let msg = self.load_error.clone().unwrap_or_else(|| "No games yet".into());
            let dy = pocket_ui::ui_art::above_words(c, pocket_ui::ui_art::Art::NoGames, MARGIN, false, EMPTY_ART_TOP, 300, 50);
            self.text.draw(c, Face::Title, 50, MARGIN, 300 + dy, pal().chalk, &msg);
            self.text.draw(c, Face::Body, 28, MARGIN, 350 + dy, pal().dust, "Copy games into /storage/roms/<system> and restart.");
            return;
        }

        match self.theme {
            Theme::Shelf => self.paint_shelf(c),
            Theme::Minimal => self.paint_minimal(c),
            Theme::Split => self.paint_split(c),
            Theme::Cards => self.paint_cards(c),
            Theme::Cabinet => self.paint_cabinet(c),
        }

        if with_hints {
            // Up next after a game brings its own hints (`app_curator`).
            if !self.paint_up_next(c) {
                self.paint_hints(c);
            }
            // The letter of an L2/R2 jump (`app_search`).
            self.paint_letter_flash(c);
        }
        // Small, left of the status corner: "Saves synced" after a game, or the cover and description downloads.
        let notes: Vec<String> = self.cover_run.note().into_iter().chain(self.text_run.note()).collect();
        let downloads = Some(notes.join(" · ")).filter(|n| !n.is_empty());
        if let Some(note) = self.sync_note().or(downloads).filter(|_| with_hints) {
            let right = pocket_ui::status_corner::room_left_of(corner_left, c.w - MARGIN);
            self.text.draw_right(c, Face::Body, 24, right, 72, pal().dust, &note);
        }

        if let Some(t) = self.launch_at {
            // With the launch animation the fade waits for the cartridge or disc to go in (`app_cabinet`).
            let p = (t.elapsed().saturating_sub(self.cabinet.launch_delay()).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
            // The Shelf look's case goes into its console over the shelf (the Cabinet paints its own with the cabinet).
            if self.theme == Theme::Shelf {
                self.paint_insert(c, Instant::now());
            }
            c.fade((p * 255.0) as u8);
            // On the shelf the selected case's front stays lit while everything else goes dark (no animation).
            if self.theme == Theme::Shelf && !self.cabinet_inserting() {
                self.shelf_relight(c);
            }
            // And where the game was left fades in over it (`launch_picture`); not behind the quick settings, which
            // can open over the game later.
            if let Some(pic) = self.launch_picture.as_ref().filter(|_| !self.sys.is_open()) {
                pocket_ui::launch_picture::paint(c, pic, (p * 255.0) as u8);
            }
        }

        // A save with two versions found since the last time: told once (`app_sync_conflicts`).
        if with_hints {
            self.sync_conflict_notice();
        }
        if let Some(n) = &self.notice {
            n.paint(c, &mut self.text);
        }
        if let Some((name, at)) = self.toast.clone() {
            if at.elapsed() < TOAST {
                // Comes down into place and fades in and out (`motion::paint_toast`).
                let w = self.text.width(Face::Body, 28, &name) + 48;
                let text = &mut self.text;
                pocket_ui::motion::paint_toast(c, at, Instant::now(), Some(TOAST), (c.w / 2 - w / 2, 36, w, 52), 20.0, |c, dy| {
                    c.round_rect(c.w / 2 - w / 2, 36 + dy, w, 52, 26, KEYCAP);
                    text.draw(c, Face::Body, 28, c.w / 2 - w / 2 + 24, 72 + dy, CHALK, &name);
                });
            }
        }
    }

    /// Minimal: plain white names on black, the selected one inverted in a pill (NextUI style).
    fn paint_minimal(&mut self, c: &mut Canvas) {
        let (top, row) = (124, 74);
        let visible = ((c.h - 120 - top) / row).max(1) as usize;
        let first = window_start(self.sel, visible, self.items.len());
        // A selected system shows its console on the right half; the names keep to the left of it.
        let art_x = c.w / 2 + 20;
        let art = self.items.get(self.sel).copied().and_then(|item| self.system_art(item, c.w - 48 - art_x, 560));
        if let Some(art) = &art {
            let (aw, ah) = (art.w as i32, art.h as i32);
            let ay = top + (c.h - 120 - top - ah) / 2;
            c.blit_sprite(art, art_x + (c.w - 48 - art_x - aw) / 2, ay, 0);
            // A quiet line under the console, as every look names a system's games ("13 games"; Muse's review A13:
            // the console stood there without a word).
            if let Some(item) = self.items.get(self.sel).copied() {
                let count = self.describe(item).2;
                let count = self.text.fit(Face::Body, 28, &count, c.w - 48 - art_x);
                let w = self.text.width(Face::Body, 28, &count);
                let base = (ay + ah + 28 + pocket_ui::type_scale::px(28) as i32).min(c.h - 120);
                self.text.draw(c, Face::Body, 28, art_x + (c.w - 48 - art_x - w) / 2, base, pal().dust, &count);
            }
        }
        let max_w = if art.is_some() { art_x - 72 - 24 } else { c.w - 2 * 48 - 48 };
        // The list scrolls and the pill glides to the selection (`app_motion`); a name turns dark as the pill
        // comes under it. Rows are cut at the list's edges while it moves.
        let now = Instant::now();
        let (first_f, hl) = self.motion.list_at(app_motion::List::Home, first, self.sel, now);
        let sel_title = self.items.get(self.sel).map(|item| self.describe(*item).0).unwrap_or_default();
        let sel_w = self.title_width(Face::Body, 36, &sel_title, self.items.get(self.sel).is_some_and(|i| self.is_favorite(*i))).min(max_w) + 48;
        let pill_w = self.motion.width_at(app_motion::List::Home, sel_w, now);
        let mut band = c.rows(top, top + visible as i32 * row);
        // The selected name on the label's paper in Dark (as every selection), the inverse pill on the light page.
        band.round_rect(48, ((hl - first_f) * row as f32).round() as i32, pill_w, 60, 30, pal().pick(DARK.select, LIGHT.text));
        for (i, k) in app_motion::rows_in_view(first_f, visible, self.items.len()) {
            let y = (k * row as f32).round() as i32;
            if (0.0..visible as f32 - 0.5).contains(&k) {
                self.hits.row(Area::new(48, top + y, band.w - 96, 60), i);
                self.motion.row(48, top + y, max_w + 48, row);
            }
            let (title, _, _, _) = self.describe(self.items[i]);
            let (max, fav) = (max_w, self.is_favorite(self.items[i]));
            let fg = mix(pal().text, pal().void, (app_motion::under(i, hl) * 255.0) as u32);
            self.draw_title(&mut band, Face::Body, 36, 72, y + 43, max, fg, &title, i == self.sel, fav);
        }
        self.hits.list(self.sel, self.items.len(), 0.0, row as f32);
    }

    /// Split: list on the left, a detail panel in the system's colour on the right (classic frontend layout).
    fn paint_split(&mut self, c: &mut Canvas) {
        let bg = pal().split_bg;
        c.clear(bg);
        let title = self.screen_title();
        self.paint_top_line(c, title.as_deref(), pal().dust);
        let (top, row, list_w) = (112, 60, 500);
        let visible = ((c.h - 120 - top) / row).max(1) as usize;
        let first = window_start(self.sel, visible, self.items.len());
        // The bar glides to the selection and takes on its system's colour on the way (`app_motion`).
        let (first_f, hl) = self.motion.list_at(app_motion::List::Home, first, self.sel, Instant::now());
        let stripe_of = |app: &App, i: usize| app.items.get(i).map_or(0, |item| stripe(&app.describe(*item).1));
        let (lo, t) = (hl.floor().max(0.0) as usize, hl - hl.floor());
        let bar = if t > 0.0 { mix(stripe_of(self, lo), stripe_of(self, lo + 1), (t * 255.0) as u32) } else { stripe_of(self, lo) };
        let mut band = c.rows(top, top + visible as i32 * row);
        band.round_rect(48, ((hl - first_f) * row as f32).round() as i32, list_w, row - 6, 10, bar);
        for (i, k) in app_motion::rows_in_view(first_f, visible, self.items.len()) {
            let y = (k * row as f32).round() as i32;
            if (0.0..visible as f32 - 0.5).contains(&k) {
                self.hits.row(Area::new(48, top + y, list_w, row), i);
                self.motion.row(48, top + y, list_w, row);
            }
            let (title, _, _, _) = self.describe(self.items[i]);
            let fav = self.is_favorite(self.items[i]);
            let fg = mix(pal().split_dim, 0xffffff, (app_motion::under(i, hl) * 255.0) as u32);
            self.draw_title(&mut band, Face::Body, 28, 68, y + 38, list_w - 40, fg, &title, i == self.sel, fav);
        }
        self.hits.list(self.sel, self.items.len(), 0.0, row as f32);
        let Some(item) = self.items.get(self.sel).copied() else { return };
        let (title, system, side, meta) = self.describe(item);
        let (px, pw) = (48 + list_w + 40, c.w - (48 + list_w + 40) - 48);
        let color = stripe(&system);
        // On a screen with fewer than 1080 lines (the Mini V1's 960) the panel gives up the difference, so the name and
        // the facts under it keep their room (docs/DEVICES.md); on 1080 lines it is 420 tall.
        let short = (1080 - c.h).max(0);
        c.round_rect(px, top, pw, 420 - short, 18, mix(color, bg, 90));
        // Systems show their game count on the panel, games show their system.
        let big = if split_big_is_side(item) { side } else { system_short(&self.systems, &system) };
        let meta = if meta == big { String::new() } else { meta };
        // Curator, Favorites and the pinned rows: their covers in the middle of the panel (`app_home_panel`), and the
        // panel's line under the name; a panel of words only keeps the big line.
        let panel = self.home_panel(item);
        let ink = app_home_panel::Ink { caption: mix(pal().on_wash, color, 60), under: mix(color, bg, 90) };
        let shown = match &panel {
            Some(p) => self.paint_home_panel(c, p, (px + 24, top + 24 + (372 - short - SPLIT_PANEL_H.min(372 - short)) / 2, pw - 48, SPLIT_PANEL_H.min(372 - short)), 0.5, &ink) > 0,
            None => false,
        };
        // Split has room under the name: a panel's line takes two lines before it is cut.
        let panel_lines = panel.map(|p| self.text.wrap_at_most(Face::Body, 28, &p.line, pw, 2));
        // Box art, then the game running and its title screen fading in turn (`app_pictures`).
        let preview = (px + 24, top + 24, pw - 48, 420 - short - 48);
        if !shown && !self.paint_preview(c, item, preview, mix(color, bg, 90)) {
            // A system card shows its console above the game count.
            if let Some(art) = self.system_art(item, pw - 48, 310 - short) {
                let (aw, ah) = (art.w as i32, art.h as i32);
                c.blit_sprite(&art, px + (pw - aw) / 2, top + 20 + (310 - short - ah) / 2, 0);
            }
            let big = self.text.fit(Face::Title, 67, &big, pw - 64);
            self.text.draw(c, Face::Title, 67, px + 32, top + 380 - short, mix(pal().on_wash, color, 60), &big);
        }
        let title = self.text.fit(Face::Title, 50, &title, pw);
        self.text.draw(c, Face::Title, 50, px, top + 500 - short, pal().text, &title);
        // A system with a fact sheet: its headline and short facts in the room under the name.
        if self.paint_system_facts(c, item, px, top + 548 - short, pw, c.h - 110) {
            return;
        }
        if let Some(lines) = panel_lines {
            for (k, line) in lines.iter().enumerate() {
                self.text.draw(c, Face::Body, 28, px, top + 548 - short + k as i32 * pocket_ui::type_scale::line(28, 12), pal().split_dim, line);
            }
            return;
        }
        let meta = self.text.fit(Face::Body, 28, &meta, pw);
        self.text.draw(c, Face::Body, 28, px, top + 548 - short, pal().split_dim, &meta);
    }

    /// Cards: a horizontal carousel of rounded cards over a wash of the selected system's colour.
    fn paint_cards(&mut self, c: &mut Canvas) {
        let Some(item) = self.items.get(self.sel).copied() else { return };
        let (title, _, _, meta) = self.describe(item);
        // The carousel glides (`app_motion`); the wash behind it turns from one card's colour to the next's.
        let pos = self.motion.cards_at(self.sel, Instant::now());
        let (lo, t) = (pos.floor().max(0.0) as usize, pos - pos.floor());
        let wash_of = |app: &App, i: usize| app.items.get(i).map_or(pal().void, |it| mix(stripe(&app.describe(*it).1), pal().void, 215));
        c.clear(if t > 0.0 { mix(wash_of(self, lo), wash_of(self, lo + 1), (t * 255.0) as u32) } else { wash_of(self, lo) });
        let place = self.screen_title();
        self.paint_top_line(c, place.as_deref(), pal().chalk);
        let center = c.w / 2;
        // Cards within reach of the glide, farthest first so the selected one lies on top. Each takes its size,
        // shade and place from how far it is from the middle; the one in the middle is lifted a little (3 % larger,
        // a soft shadow on paper).
        let mut cards: Vec<(usize, f32)> =
            ((pos.floor() as i32 - 2).max(0)..=(pos.ceil() as i32 + 2).min(self.items.len() as i32 - 1)).map(|i| (i as usize, i as f32 - pos)).collect();
        cards.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        let peek = pocket_ui::text::boxes::mark();
        for (i, d) in cards {
            let near = d.abs().min(1.0);
            let card = self.items[i];
            let (t, sys, side, _) = self.describe(card);
            let lerp = |a: f32, b: f32| (a + (b - a) * near).round() as i32;
            let (w, h) = (lerp(340.0 * 1.03, 270.0), lerp(460.0 * 1.03, 370.0));
            let x = center + (d * 330.0).round() as i32 - w / 2;
            let y = 150 + (460 - h) / 2;
            let color = stripe(&sys);
            self.hits.row(Area::new(x, y, w, h), i);
            pocket_ui::game_card::lift_shadow(c, x + 8, y + h, w - 16, ((1.0 - near) * pal().t as f32) as u8);
            c.round_rect(x, y, w, h, 28, mix(color, pal().void, (near * pal().pick(120, 80) as f32) as u32));
            // Pictures are made in the two resting sizes only (they are cached per size) and scaled on the way.
            let (rw, rh) = if near < 0.5 { ((340.0f32 * 1.03).round() as i32, (460.0f32 * 1.03).round() as i32) } else { (270, 370) };
            let scale = |iw: i32, ih: i32, room_w: i32, room_h: i32, rest_w: i32, rest_h: i32| {
                let k = (room_w as f32 / rest_w.max(1) as f32).min(room_h as f32 / rest_h.max(1) as f32);
                ((iw as f32 * k).round() as i32, (ih as f32 * k).round() as i32)
            };
            // Games with box art show the art on the card; the title is printed below the carousel.
            if let Some(cover) = self.cover(card, rw - 24, rh - 24) {
                let (cw, ch) = scale(cover.w as i32, cover.h as i32, w - 24, h - 24, rw - 24, rh - 24);
                let (cx, cy) = (x + (w - cw) / 2, y + (h - ch) / 2);
                c.blit_scaled(&cover, cx, cy, cw, ch);
                if near > 0.0 {
                    c.shade_rect(cx, cy, cw, ch, pal().void, (near * 120.0) as u8);
                }
                continue;
            }
            // A system card shows its console between the game count and the name. Pixel art is fitted, not scaled,
            // in five sizes between the two resting ones (each is made once and kept).
            let q = (near * 4.0).round() / 4.0;
            let qlerp = |a: f32, b: f32| (a + (b - a) * q).round() as i32;
            let (aw_max, ah_max) = (qlerp(340.0 * 1.03, 270.0) - 40, qlerp(460.0 * 1.03, 370.0) - 64 - 80);
            let art = self.system_art(card, aw_max, ah_max);
            if let Some(art) = &art {
                let (aw, ah) = (art.w as i32, art.h as i32);
                c.blit_sprite_to(art, x + (w - aw) / 2, y + 64 + (h - 64 - 80 - ah) / 2, (near * 120.0) as u8, pal().void);
            }
            let own_line = matches!(card, Item::System(_) | Item::Favorites | Item::Streaming | Item::Search) || card.is_curator();
            if own_line && art.is_none() && !side.is_empty() {
                // No picture (Curator's entries, Favorites, Search, PC streaming): their line ("70 games · 21 filters",
                // "3 shelves") is the card's content, large in its middle as on Split's panel, not a blank body.
                // Too wide for one line: a line per part ("70 games" / "21 filters"), each cut with "…" if need be.
                let lines: Vec<String> = if self.text.width(Face::Title, 44, &side) <= w - 48 {
                    vec![side]
                } else {
                    side.split(" · ").take(3).map(|p| self.text.fit(Face::Title, 44, p, w - 48)).collect()
                };
                let n = lines.len() as i32;
                let first = y + 64 + (h - 64 - 80 - n * 52) / 2 + 40;
                for (k, line) in lines.iter().enumerate() {
                    self.text.draw(c, Face::Title, 44, x + 24, first + k as i32 * 52, 0xffffff, line);
                }
            } else {
                let short = if own_line { side } else { system_short(&self.systems, &sys) };
                let short = self.text.fit(Face::Body, 21, &short, w - 48);
                self.text.draw(c, Face::Body, 21, x + 24, y + 44, 0xffffff, &short);
            }
            let t = self.text.fit(Face::Title, 37, &t, w - 48);
            self.text.draw(c, Face::Title, 37, x + 24, y + h - 32, 0xffffff, &t);
        }
        // For the snapshot checks of Settings → Text size: the neighbours are cut by the screen's edges on purpose.
        pocket_ui::text::boxes::cut_on_purpose(peek);
        // Cards flip sideways; a swipe up or down does the same.
        self.hits.list(self.sel, self.items.len(), 330.0, 150.0);
        let fav = self.is_favorite(item);
        let tw = self.title_width(Face::Title, 67, &title, fav).min(c.w - 2 * MARGIN);
        self.draw_title(c, Face::Title, 67, center - tw / 2, 720, c.w - 2 * MARGIN, pal().text, &title, true, fav);
        let meta = self.text.fit(Face::Body, 28, &meta, c.w - 2 * MARGIN);
        let mw = self.text.width(Face::Body, 28, &meta);
        self.text.draw(c, Face::Body, 28, center - mw / 2, 770, pal().chalk, &meta);
        // A system with a fact sheet: one more line, its headline ("Sega · 1998 · sixth generation").
        if let Some(headline) = self.console_headline(item) {
            let headline = self.text.fit(Face::Body, 28, &headline, c.w - 2 * MARGIN);
            let hw = self.text.width(Face::Body, 28, &headline);
            self.text.draw(c, Face::Body, 28, center - hw / 2, 816, pal().chalk, &headline);
        }
    }

    /// (title, system id for the stripe, text on the spine's right, meta line on the label)
    fn describe(&self, item: Item) -> (String, String, String, String) {
        match item {
            Item::Streaming => ("PC streaming".into(), "streaming".into(), "Moonlight".into(), "Play your PC's games over Wi-Fi with Moonlight".into()),
            Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) => self.curator_describe(item),
            Item::Search => ("Search".into(), "search".into(), "All games".into(), "Find any game by its name, typos and all".into()),
            Item::Favorites => {
                let n = self.favorite_items().len();
                let count = if n == 1 { "1 game".to_string() } else { format!("{n} games") };
                ("Favourites".into(), "favorites".into(), count.clone(), format!("{count} you starred"))
            }
            Item::Recent(i) => {
                let r = &self.recents.list[i];
                let meta = format!("{}, {}, {} played", system_label(&self.systems, &r.system), recents::ago(r.last_played), recents::duration(r.play_secs));
                (r.name.clone(), r.system.clone(), system_short(&self.systems, &r.system), meta)
            }
            Item::System(s) => {
                let sys = &self.systems[s];
                let n = sys.games.len();
                let count = if n == 1 { "1 game".to_string() } else { format!("{n} games") };
                (sys.fullname.clone(), sys.name.clone(), count.clone(), count)
            }
            Item::Game(s, g) => {
                let sys = &self.systems[s];
                let game = &sys.games[g];
                let played = self.recents.list.iter().find(|r| r.path == game.path);
                let meta = played.map_or("Not played yet".to_string(), |r| format!("Last played {}, {} in total", recents::ago(r.last_played), recents::duration(r.play_secs)));
                let meta = match self.library.status(&game.path) {
                    pocket_ui::library_meta::Status::NotStarted => meta,
                    status => format!("{}, {}", status.label(), meta.to_lowercase()),
                };
                // The Favorites shelf mixes systems, and one title can be on several (Blockamok Remix is on the 3DS and the
                // Wii): name the system on the spine and on the label, like the recent games on Home do.
                if matches!(self.screen, Screen::Favorites | Screen::Shelf(_) | Screen::Sticker(_) | Screen::LensResults) {
                    return (game.name.clone(), sys.name.clone(), system_short(&self.systems, &sys.name), format!("{}, {}", sys.fullname, meta.to_lowercase()));
                }
                (game.name.clone(), sys.name.clone(), String::new(), meta)
            }
        }
    }

    fn paint_hints(&mut self, c: &mut Canvas) {
        let selected = self.items.get(self.sel).copied();
        let starred = selected.is_some_and(|item| self.is_favorite(item) || self.curator_pinned(item));
        // The cabinet has no level to go back to: all its shelves are Home (but a Curator list opened from the
        // Curator shelf or the Lens goes back there, `app_curator`).
        let details = self.has_console_page(selected);
        let home = self.screen == Screen::Home || (self.theme == Theme::Cabinet && !self.curator_has_back());
        let mut hints = shelf::hints(selected, home, starred, details);
        // The cabinet's top shelf: Up opens the search (`app_search`).
        hints.extend(self.search_hint());
        self.paint_hint_row(c, &hints);
    }

    /// Button hints along the bottom edge; the ones for a single button can be tapped. When they change, the old
    /// ones fade out as the new ones fade in (`app_motion`).
    fn paint_hint_row(&mut self, c: &mut Canvas, hints: &[(&str, &str)]) {
        if let Some((old, p)) = self.motion.hints_fade(hints, Instant::now()) {
            let old: Vec<(&str, &str)> = old.iter().map(|(k, l)| (k.as_str(), l.as_str())).collect();
            let (y, h) = (c.h - 52 - 46, 64);
            // The old ones go faster than the new ones come, so the two rows hardly overlap where the hints moved.
            pocket_ui::motion::layer(c, 0, y, c.w, h, ((1.0 - p).powi(3) * 255.0) as u8, |c| self.paint_hint_keys(c, &old, false));
            // For the snapshot checks of Settings → Text size: the old hints are on their way out, not read.
            pocket_ui::text::boxes::painted_over(c, (0, y, c.w, h));
            pocket_ui::motion::layer(c, 0, y, c.w, h, (p * 255.0) as u8, |c| self.paint_hint_keys(c, hints, true));
            return;
        }
        self.paint_hint_keys(c, hints, true);
    }

    fn paint_hint_keys(&mut self, c: &mut Canvas, hints: &[(&str, &str)], tappable: bool) {
        // The keys and labels as on every page with this bar (`console_page::paint_hints`).
        for (area, b) in pocket_ui::console_page::paint_hints(c, &mut self.text, hints) {
            if tappable {
                self.hits.button(area, b);
            }
        }
    }
}

/// `POCKETUI_SNAPSHOT_SIZE=<w>x<h>` for `--snapshot`; without it (or when it doesn't read as a size) the device's
/// screen (`device::Device::screen`: 1240 x 1080 on the Mini V2 and anything unknown, `POCKETUI_DEVICE=rp5` 1920 x 1080).
pub(crate) fn snapshot_dims() -> (u32, u32) {
    let panel = snapshot_panel_dims();
    if std::env::var_os("POCKETUI_SNAPSHOT_NATIVE").is_some() { panel } else { pocket_ui::scaled_layout::logical_size(panel) }
}

fn snapshot_panel_dims() -> (u32, u32) {
    snapshot_size(std::env::var("POCKETUI_SNAPSHOT_SIZE").ok().as_deref(), pocket_ui::device::current().screen())
}

fn snapshot_size(env: Option<&str>, screen: (u32, u32)) -> (u32, u32) {
    env.and_then(|s| s.trim().split_once(['x', 'X']))
        .and_then(|(w, h)| Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?)))
        .filter(|&(w, h)| (320..=4096).contains(&w) && (240..=4096).contains(&h))
        .unwrap_or(screen)
}

/// Renders one pause-menu screen to a PNG without a window, for checking layouts on the desktop:
/// `pocket-ui --snapshot <main|options|picture|extras|save|emulator[:sel]|category[:cat[:sel]]|controls[:sel]> <out.png> [background.png] [game.png]`.
/// The Emulator and Controls pages read pocket-core's snapshot from `POCKETUI_DATA/core-options/gbc.json` (copy one from the device).
/// The Cheats page of the snapshots. "cheats-more" has cheats with codes and entries pocket-core can't apply, "cheats-only"
/// only the latter (made-up cht files in the temp folder, so the pictures do not depend on a cheat database); every
/// other state reads the real database, as the page does.
fn snapshot_cheats(state: &str, tmp: &std::path::Path) -> Option<CheatMenu> {
    let rom = std::path::Path::new("/storage/roms/gbc/Pokemon - Crystal Version (USA, Europe).gbc");
    let memory = |n: usize, desc: &str| format!("cheat{n}_desc = \"{desc}\"\ncheat{n}_address = \"{}\"\ncheat{n}_value = \"9\"\ncheat{n}_cheat_type = \"1\"\ncheat{n}_handler = \"1\"\n", 100 + n);
    let body = match state {
        "cheats-more" => format!(
            "cheat0_desc = \"Infinite lives\"\ncheat0_code = \"01FF57D8\"\ncheat1_desc = \"Full energy\"\ncheat1_code = \"01FF5AD8\"\n{}{}",
            memory(2, "Coins to 99"),
            memory(3, "Start with all items")
        ),
        "cheats-only" => memory(0, "Coins to 99"),
        _ => return pocket_ui::cheats::CheatMenu::load(tmp, "gbc", rom),
    };
    let root = tmp.join("cht-demo");
    let folder = root.join(pocket_ui::cheats::folder("gbc").unwrap_or("Nintendo - Game Boy Color"));
    let _ = std::fs::create_dir_all(&folder);
    let _ = std::fs::write(folder.join("Pokemon - Crystal Version (USA, Europe).cht"), body);
    CheatMenu::load_in(tmp, "gbc", rom, &[root], false)
}

/// Six made-up hours of battery history in `data_dir` for the Battery page's snapshot: GBA, the menus, PlayStation, the
/// last 20 minutes on the charger (the battery itself comes from `POCKETUI_BATTERY`).
fn snapshot_battery_samples(data_dir: &std::path::Path) {
    use pocket_ui::battery::Sample;
    let now = recents::now_secs();
    let mut h = pocket_ui::battery_history::History::new();
    let mut capacity = 95.0f32;
    for m in (0..360u64).rev() {
        let (tag, drain, charging) = match m {
            0..=19 => (None, -0.5, true),
            20..=139 => (Some("psx"), 0.12, false),
            140..=199 => (None, 0.06, false),
            _ => (Some("gba"), 0.08, false),
        };
        capacity = (capacity - drain).clamp(1.0, 100.0);
        let s = Sample { t: now - m * 60, capacity: capacity as u8, charging, full: false, mv: 3800, ma: if charging { 900 } else { -600 }, temp: Some(310), tag: 0 };
        h.push(s, tag);
    }
    let _ = h.save(&data_dir.join(pocket_ui::battery_history::FILE));
}

fn snapshot(app: &mut App, args: &[String]) {
    // Snapshots are settled pictures: use the same decoder and scaler synchronously, as before covers
    // moved to the picture worker. Live frames keep drawing while that worker reads an image.
    app.pictures.sync = true;
    // A `:device=` at the end was settled at the start (`device::snapshot_state`); the state is what is before it.
    let state = args.first().map(|s| pocket_ui::device::snapshot_state(s).0.to_string());
    let (Some(state), Some(out)) = (state.as_ref(), args.get(1)) else {
        eprintln!("usage: pocket-ui --snapshot <main|menu:<pocket-core|ps2|retroarch|other>[:row]|game-quick[:row][:own]|options|picture|shortcuts|extras|save|slot-save|slot-load|slot-empty|emulator|category|controls|cheats|console:<system>[:sources|:credits][:<downs>]|shelf:<system>[:<look>]|emu:<system>[:<file>][:<emulator>/<core>][:save][:<row>]> <out.png> [background.png]");
        return;
    };
    // The Mini V2's screen, or another size to see how the layouts take it (docs/NOVA-FEASIBILITY.md).
    let (w, h) = snapshot_dims();
    // In the appearance saved in POCKETUI_DATA's settings.json (Light: `"appearance": "light"`).
    pocket_ui::palette::set_blend(if app.fader.light() { 255 } else { 0 });
    // First-start setup would cover the menu on a machine without settings.
    app.setup = None;
    // A game's card is the page in every state but `box:` (`game_box`), as the snapshot tests have always seen it.
    app.game_box = false;
    if state.starts_with("battery") && state.ends_with(":samples") {
        snapshot_battery_samples(&app.data_dir);
    }
    // What a running pocket-ui reads on threads (shown as "Loading…" until then) is read before the picture.
    pocket_ui::battery_history::load_now(&app.data_dir);
    // The player's EmulationStation gamelists (`app_gamelist`): the card's description and the covers come from them.
    app.gamelist_load_now();
    app.port_perf_wait_loaded();
    // "bench-shelf[:<look>]": a move on Home in numbers, on stdout (`App::bench_shelf`, tests/shelf_moves.rs).
    if let Some(look) = state.strip_prefix("bench-shelf").map(|r| r.trim_start_matches(':')) {
        let b = app.bench_shelf(if look.is_empty() { "split" } else { look }, w, h);
        println!(
            "bench-shelf items={} moves={} p50_us={:.0} p95_us={:.0} max_us={:.0} glyphs={} patterns={} worst={}",
            app.items.len(),
            b.moves,
            b.p50_us,
            b.p95_us,
            b.max_us,
            b.glyphs,
            b.patterns,
            b.worst
        );
        return;
    }
    // "bench-settings": the first frame of Settings in numbers, on stdout (`App::bench_settings`, tests/settings_open.rs).
    if state == "bench-settings" {
        let b = app.bench_settings(w, h);
        println!(
            "bench-settings first_us={:.0} first_glyphs={} first_patterns={} steady_us={:.0} pill_us={:.0} transition_us={:.0}",
            b.first_us, b.first_glyphs, b.first_patterns, b.steady_us, b.pill_us, b.transition_us
        );
        return;
    }
    // "game-save-warning[:unwritable]": pocket-core's line over the game while saves fail (`app_save_trouble`).
    let background = args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)));
    if let Some(buf) = app_save_trouble::game_warning_snapshot(state, background, w, h, &mut app.text) {
        write_png(&buf, w, h, out);
        return;
    }
    // "notice:<several|password|packing|checksum|incomplete|empty|none>": Home with the card a zip pocket-core refused leaves
    // (`archive::message`), "notice:bios" the card for a game that didn't start (`fallback::not_started_notice`; "bios-segacd"
    // for one whose missing BIOS is known),
    // "notice:switched-off" the start after a switch-off after sleeping (`app_auto_off`), "notice:patch-<version|damaged|
    // big|kind>" a patch next to the game that pocket-core refused (`softpatch::message`).
    if let Some(kind) = state.strip_prefix("notice:") {
        let patch = kind.strip_prefix("patch-").and_then(pocket_ui::softpatch::Refusal::named);
        app.notice = Some(Notice::new(match kind {
            _ if patch.is_some() => pocket_ui::softpatch::message(patch.unwrap(), "Seiken Densetsu 3 (Japan).bps"),
            "bios" => pocket_ui::fallback::not_started_notice("Final Quest", None),
            "bios-segacd" => pocket_ui::fallback::not_started_notice(
                "Pocket CD Demo",
                Some("Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin"),
            ),
            "switched-off" => pocket_ui::auto_off::Note { hours: 3, brightness: None }.text(),
            k => pocket_ui::archive::message(k, "Sonic The Hedgehog (USA, Europe).zip"),
        }));
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "leave[:<row>]", "about-es[:<row>]": the dialog before EmulationStation (`app_rescue`).
    if app.leave_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        app.paint_leave(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "tour[:<card id>[:<keys>]]", "tour-new:<id>[,<id>...]": Settings → Tour, "New in this version" (`app_tour`).
    if app.tour_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "setup:<look|wifi|games|first|play|card=<id>>[:keys]": a setup step, then the keys pressed in it (r l u d a b x y s,
    // R for R1).
    if let Some(rest) = state.strip_prefix("setup:") {
        let (step, keys) = rest.split_once(':').unwrap_or((rest, ""));
        let mut setup = app.new_setup();
        setup = match step {
            "look" => setup.only(Only::Look),
            "games" => setup.only(Only::Games),
            // Settings > Games > Extras: Cover art, Game pictures, PortMaster games (keys: d down, u up, a).
            "extras" => setup.only(Only::Extras),
            // "How do you like to play?" (keys: d down, a); "card=<feature id>": Choose yourself at that card (keys d, a, b, r).
            "play" => setup.at_play(),
            _ if step.starts_with("card=") => setup.at_card(&step["card=".len()..]),
            // Settings > Wi-Fi with made-up networks, saved ones among them (keys: d down, a, y forget, x scan).
            "wifi" => setup.only(Only::Wifi).with_wifi_demo(),
            "wifi-timeout" => setup.only(Only::Wifi).with_wifi_error_demo(),
            // Home still being brought up by NetworkManager ("Connecting…"), the same after a minute without an address, and
            // a typed password the network refused (the keyboard again, with the reason).
            "wifi-connecting" => setup.only(Only::Wifi).with_connecting_demo(false),
            "wifi-stuck" => setup.only(Only::Wifi).with_connecting_demo(true),
            "wifi-refused" => setup.only(Only::Wifi).with_refused_demo(),
            // The Wi-Fi page when the radio is off (the boot hooks blocked it; A turns it on), when the switch did not work,
            // when the scan failed, when it worked and found nobody, and while the radio is being switched on.
            "wifi-off" => setup.only(Only::Wifi).with_net_demo(pocket_ui::net_state::demo::off()),
            "wifi-switch-failed" => setup.only(Only::Wifi).with_net_demo(pocket_ui::net_state::demo::switch_failed()),
            "wifi-save-failed" => setup.only(Only::Wifi).with_net_demo(pocket_ui::net_state::demo::save_failed()),
            "wifi-scan-failed" => setup.only(Only::Wifi).with_net_demo(pocket_ui::net_state::demo::scan_failed()),
            "wifi-empty" => setup.only(Only::Wifi).with_net_demo(pocket_ui::net_state::demo::empty()),
            "wifi-switching" => setup.only(Only::Wifi).with_switching_demo(),
            // The same, on a network that wants a sign-in in a browser (`net_check`).
            "wifi-portal" => setup.only(Only::Wifi).with_wifi_demo().with_portal_demo(),
            // Its sign-in sheet (keys: d down, a opens a row, b back); "-nomac" as if the Wi-Fi address couldn't be read.
            "wifi-portal-sheet" => setup.only(Only::Wifi).with_wifi_demo().with_portal_sheet(true),
            "wifi-portal-sheet-nomac" => setup.only(Only::Wifi).with_wifi_demo().with_portal_sheet(false),
            // Its "Sign in…" with a made-up page or answer (`portal_form::demo`: hotel, clickthrough, voucher, email,
            // js, social, elsewhere, still, toomany, failed, online, loading); keys d/u move, a ticks, types or opens.
            _ if step.starts_with("wifi-portal-signin=") => setup.only(Only::Wifi).with_wifi_demo().with_portal_signin(&step["wifi-portal-signin=".len()..]),
            // The PortMaster browser with POCKETUI_DATA's ports.json (or a small demo catalog); "ports-shared":
            // file sharing on (the desktop's stand-in), for the "Copy your game files" page (keys r, x).
            "ports" | "ports-shared" | "ports-update" => {
                #[cfg(not(target_os = "linux"))]
                if step == "ports-shared" {
                    pocket_ui::wifi::enable_sharing();
                }
                let setup = setup.only(Only::Extras).show_ports(app_ports::snapshot_catalog(&app.data_dir));
                // "ports-update": two ports of the list have a newer version in the catalogue (made up).
                if step == "ports-update" { setup.with_updates(&["2048.zip", "cceleste.zip"]) } else { setup }
            }
            _ => setup,
        };
        app.setup = Some(setup);
        // What the setup's threads look at (file sharing, remote access), read at once for the picture.
        if let Some(s) = app.setup.as_mut() {
            s.looked_now();
        }
        for k in keys.chars() {
            let b = match k {
                'r' => Button::Right,
                'l' => Button::Left,
                'u' => Button::Up,
                'd' => Button::Down,
                'a' => CONFIRM,
                'b' => CANCEL,
                'x' => Button::North,
                'y' => Button::West,
                's' => Button::Start,
                'R' => Button::R1,
                'L' => Button::L1,
                _ => continue,
            };
            if let Some(s) = app.setup.as_mut() {
                s.press(b);
                s.looked_now();
            }
        }
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    if state.starts_with("port-files") {
        app.port_files_snapshot(state);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "console:<system>[:sources][:<presses of Down>]": a console page; "shelf:<system>[:<look>]": Home with that system selected.
    if let Some(rest) = state.strip_prefix("console:") {
        app.console_snapshot(rest);
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        app.console_snapshot_scroll(rest);
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "emu:<system>[:<game file>][:<emulator>/<core>][:save][:<row>]": the "Runs on" page (`app_emu_choice`).
    if app.emu_choice_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "this-game[:<system>[:<game file>]][:<row>][:<key>=<value>]": the "This game" page (`app_this_game`).
    if state.starts_with("this-game") && app.this_game_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    if let Some(rest) = state.strip_prefix("shelf:") {
        app.shelf_snapshot(rest);
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        // POCKETUI_SNAPSHOT_LAUNCH_MS: the launch animation that long after A (Shelf look, `app_cabinet`).
        if let Some(ms) = std::env::var("POCKETUI_SNAPSHOT_LAUNCH_MS").ok().and_then(|v| v.parse().ok()) {
            app.launch_animation_snapshot(Duration::from_millis(ms));
            app.paint(&mut Canvas::new(&mut buf, w, h));
        }
        write_png(&buf, w, h, out);
        return;
    }
    // "conflicts:<card|choose|notice|none>[:<system>][:<row>]": a save's two versions (`App::conflicts_snapshot`).
    if app.conflicts_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "box:<system>:<name>[:<keys>]": the game's card as its box (`game_box`): `r` turns it over, `d` scrolls the
    // back; POCKETUI_SNAPSHOT_FLIP_MS paints the last turn that long after it began.
    let boxed = state.strip_prefix("box:").map(|r| format!("card:{r}"));
    if boxed.is_some() {
        app.game_box = true;
        // The case's cover is read at once, not on the thread (as the cabinet's snapshots do).
        app.cabinet.inline = true;
    }
    // "info:<look>:<system>:<name>[:<keys>]": X on the game in that look (`show_card`): the info page (`game_info`) in
    // Minimal, Cards and Split, the box in Shelf and Cabinet.
    let info = state.strip_prefix("info:").and_then(|r| r.split_once(':')).map(|(look, r)| {
        app.theme = match look {
            "minimal" => Theme::Minimal,
            "cards" => Theme::Cards,
            "split" => Theme::Split,
            "cabinet" => Theme::Cabinet,
            _ => Theme::Shelf,
        };
        app.game_box = true;
        app.cabinet.inline = true;
        format!("card:{r}")
    });
    // "curator:<what>": Curator's screens (`App::curator_snapshot`).
    if let Some(rest) = state.strip_prefix("curator:").or(boxed.as_deref()).or(info.as_deref()) {
        app.curator_snapshot(rest);
        // ":battery=<case>" on a card: its Battery line with made-up measurements (`App::battery_snapshot`).
        app.battery_snapshot(rest);
        // ":lookup=<case>" on a card: the line "Look up again" ends with (`App::look_up_snapshot`).
        app.look_up_snapshot(rest);
        // ":patch" on a card: a patch next to its game (`App::patch_snapshot`).
        app.patch_snapshot(rest);
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        app.curator_snapshot_after(rest);
        if let Some(bx) = app.card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
            bx.snapshot_settle();
        }
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "search:<…>" and "letters:<…>": the game search and the letter of a jump (`app_search::find_snapshot`).
    if app.find_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "pictures:<which>[:<system>]": Split's preview at a point of its cycle, or the card (`app_pictures`).
    if app.pictures_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "gallery[:…]": the screenshot gallery with made-up screenshots, or the game card's row for them (`app_gallery`).
    if app.gallery_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        app.gallery_snapshot_after(state);
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    if let Some(rest) = state.strip_prefix("cabinet:") {
        app.cabinet_snapshot(rest);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "card-manage:<system>:<name>[:<mode>]": a card's Hide or delete; "storage-notice": the low-space notice (`app_storage`).
    if app.storage_shelf_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut Canvas::new(&mut buf, w, h));
        app.storage_shelf_snapshot_after(state);
        app.paint(&mut Canvas::new(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    if let Some(demo) = state.strip_prefix("sys:") {
        // Quick settings, the time pages and the OSD, over the shelf.
        app.sys.demo(demo, Instant::now());
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        app.paint(&mut c);
        app.sys.paint(&mut c, &mut app.text, true);
        write_png(&buf, w, h, out);
        return;
    }
    app.running = Some(Running {
        system: "gbc".into(),
        name: "uCity".into(),
        path: PathBuf::from("/storage/roms/gbc/uCity.gbc"),
        emulator: "pocket-core".into(),
        started: Instant::now(),
        pgid: None,
        options: Default::default(),
    });
    let frame = args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)));
    let tmp = std::env::temp_dir().join("pocketui-snapshot");
    // "picture-gba[:<row>]": the Picture page of a GBA game, whose LCD grid has a row for the GBA screen's colours.
    // "picture-snes[:<row>]": a SNES game's.
    let gba = state.starts_with("picture-gba");
    let mut settings = if gba {
        GameSettings::load(&tmp, "gba", std::path::Path::new("Advance Wars.gba"))
    } else if state.starts_with("picture-snes") {
        GameSettings::load(&tmp, "snes", std::path::Path::new("Super Metroid.sfc"))
    } else {
        GameSettings::load(&tmp, "gbc", std::path::Path::new("uCity.gbc"))
    };
    settings.step(options::setting("effect").unwrap(), 1);
    // "picture:<row>:left|right": Screen orientation turned that way (its row is the last one).
    for turned in ["left", "right"] {
        if state.starts_with("picture") && state.split(':').any(|p| p == turned) {
            settings.values.insert("orientation".into(), turned.into());
        }
    }
    // "extras:<row>:<level>": Over the game on that level (`over_game::Level::value`; row 2 selects it).
    if let Some(l) = state.strip_prefix("extras").and_then(|r| r.rsplit(':').next()).and_then(|v| pocket_ui::over_game::Level::ALL.into_iter().find(|l| l.value() == v)) {
        settings.values.insert(pocket_ui::over_game::KEY.into(), l.value().into());
    }
    // "battery:<range>": the Battery page with `battery::RANGES[range]` hours (a day by default).
    // "battery[:<range>][:samples]": the Battery page and its graph's range; `:samples` with six made-up hours of
    // history (`snapshot_battery_samples`).
    let battery_args: Option<Vec<&str>> = state.strip_prefix("battery").map(|r| r.split(':').filter(|p| !p.is_empty()).collect());
    let battery_range = battery_args.as_ref().map(|a| a.first().and_then(|n| n.parse().ok()).unwrap_or(pocket_ui::battery::DEFAULT_RANGE));
    // "manual:<page>:<pixels>": a page of the manual (0 = the first), scrolled down by that much.
    let manual_at = state.strip_prefix("manual").map(|r| r.split(':').filter_map(|n| n.parse::<i32>().ok()).collect::<Vec<_>>());
    if let Some(at) = &manual_at {
        app.manual.page = at.first().copied().unwrap_or(0).clamp(0, pocket_ui::manual::PAGES.len() as i32 - 1) as usize;
        app.settings_page = Some(SettingsPage::Manual);
        // The reader needs one paint to know how long the page is before it can scroll.
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint_manual(&mut Canvas::new(&mut buf, w, h));
        app.manual.scroll_by(at.get(1).copied().unwrap_or(0));
    }
    if manual_at.is_some() || app.store_snapshot(state) || app.starter_snapshot(state) || app.sync_snapshot(state) || app.leds_snapshot(state) || app.night_light_snapshot(state) || app.tailscale_snapshot(state) || app.web_snapshot(state) || app.bluetooth_snapshot(state) || app.perf_fan_snapshot(state) || app.sleep_power_snapshot(state) || app.report_snapshot(state) || app.port_perf_snapshot(state) || app.achievements_snapshot(state) || app.tools_snapshot(state) || app.pc_streaming_snapshot(state) || app.backup_snapshot(state) || app.storage_snapshot(state) || app.gamelist_snapshot(state) {
    } else if let Some(kind) = state.strip_prefix("updates") {
        // The Updates page after "Get system" with a made-up list: "updates:note" staged with a note of what the new
        // system lacks, "updates:refused" refused because pocket-ui would break (`os_compat`).
        let os = update::Release { version: "20261101".into(), url: "x.tar".into(), sha256: String::new(), size: 1, notes: String::new() };
        app.updates.manifest = Some(Manifest { format: 1, os: Some(os), ..Default::default() });
        app.updates.message = Some(if kind == ":refused" {
            "This system update would break pocket-ui (libxkbcommon.so.0 is missing: pocket-ui can't open its window). It was not installed.".into()
        } else {
            os_staged_message(Some("Note: no gambatte core in the new system, so gb, gbc games would not start."))
        });
        app.settings_page = Some(SettingsPage::Updates);
    } else if state == "about" || state.starts_with("settings") || battery_range.is_some() {
        // "settings[:<category>]": the categories with that one selected (the first by default); "settings:<category>:<row>":
        // that category's list (`SETTINGS_GROUPS`) at its row (counted from 0).
        let nums: Vec<usize> = state.split(':').skip(1).filter_map(|n| n.parse().ok()).collect();
        let g = nums.first().copied().unwrap_or(0).min(SETTINGS_GROUPS.len() - 1);
        let rows = app_settings_groups::rows(g);
        app.settings_page = Some(match (state.as_str(), battery_range, nums.get(1)) {
            ("about", _, _) => SettingsPage::About,
            (_, Some(range), _) => SettingsPage::Battery(range),
            (_, _, Some(r)) => SettingsPage::List(rows[(*r).min(rows.len() - 1)]),
            _ => SettingsPage::Groups(g),
        });
        // The values the list reads on a thread, read here at once (`settings_values`).
        let connected = pocket_ui::status_corner::status().wifi;
        pocket_ui::settings_values::set(pocket_ui::settings_values::read(&*app.sys.shared_hw(), &app.sys.net(), &app.data_dir, connected));
        // And the About page's (`ABOUT`) and the Bluetooth row's.
        ABOUT.set((free_space("/storage"), pocket_ui::wifi::ip_now()));
        app.bluetooth_row_now();
    }
    let group = |name: &str| options::GROUPS.iter().position(|g| g.name == name).unwrap_or(0);
    let emu = EmuMenu::load(&app.data_dir, "gbc", std::path::Path::new("/storage/roms/gbc/uCity.gbc"), &mut settings);
    // "name:a:b" selects rows: controls:<row>, emulator:<row>, category:<category>:<row>.
    let mut parts = state.split(':');
    let name = parts.next().unwrap_or_default();
    let nums: Vec<usize> = parts.filter_map(|p| p.parse().ok()).collect();
    let n = |i: usize| nums.get(i).copied().unwrap_or(0);
    let page = match name {
        "options" => Page::Options { sel: n(0) },
        "picture" | "picture-gba" | "picture-snes" => Page::Group { group: group("Picture"), sel: if nums.is_empty() { 2 } else { n(0) } },
        // "shortcuts[:<row>]": Fast-forward unless a row is given (6: Over the game).
        "shortcuts" => Page::Group { group: group("Shortcuts"), sel: nums.first().copied().unwrap_or(3) },
        // "extras:<row>": Rumble unless a row is given (2: Earlier states, 5: Performance, 6: Fan).
        "extras" => Page::Group { group: group("Extras"), sel: nums.first().copied().unwrap_or(4) },
        "save" => Page::SaveChanges { sel: 1 },
        "emulator" => Page::Emulator { cat: None, sel: n(0) },
        "category" => Page::Emulator { cat: Some(n(0)), sel: n(1) },
        "controls" => Page::Controls { sel: n(0) },
        "cheats" | "cheats-more" | "cheats-only" => Page::Cheats { sel: n(0) },
        "achievements-list" => Page::Achievements { sel: n(0) },
        "ps2-options" | "gc-options" | "wii-options" | "3ds-options" | "gc-more" | "wii-more" => Page::Standalone { sel: n(0) },
        _ => Page::Main,
    };
    // Slot pages: the third slot of eight, with slots 1, 3 and 6 filled; "slot-empty" is slot 4. "slot-auto" and
    // "slot-undo" are Load on its first two entries (the auto state, "Before load"); "slot-one" is Azahar's single slot.
    let demo_dir = tmp.join("states");
    let _ = std::fs::remove_dir_all(&demo_dir);
    let _ = std::fs::create_dir_all(&demo_dir);
    let one_slot = state == "slot-one";
    let demo_states = StateSet {
        auto: vec![demo_dir.join("auto")],
        undo: demo_dir.join("undo"),
        slots: (1..=if one_slot { 1 } else { 8 }).map(|n| demo_dir.join(format!("slot{n}"))).collect(),
    };
    let mut fill = vec![demo_states.undo.clone(), demo_states.auto[0].clone()];
    fill.extend(demo_states.slots.iter().enumerate().filter(|(i, _)| [0, 2, 5].contains(i)).map(|(_, p)| p.clone()));
    for f in fill {
        let _ = std::fs::write(&f, b"x");
    }
    let (sel, slot) = match state.as_str() {
        "achievements-menu" => (4, 0),
        "slot-save" => (1, 2),
        "slot-load" | "slot-auto" | "slot-undo" => (2, 2),
        "slot-empty" => (2, 3),
        "slot-one" => (1, 0),
        _ => (if page == Page::Main { 3 } else { 0 }, 0),
    };
    let load_at = match state.as_str() {
        "slot-auto" => 0,
        "slot-undo" => 1,
        _ => slot + 2,
    };
    let shows_state = slot == 2 || matches!(state.as_str(), "slot-auto" | "slot-undo" | "slot-one");
    let preview = shows_state.then(|| args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)))).flatten();
    let minutes = match state.as_str() {
        "slot-auto" => 42,
        "slot-undo" => 1,
        _ => 7,
    };
    let saved = shows_state.then(|| (preview, std::time::SystemTime::now() - Duration::from_secs(minutes * 60)));
    // "menu:<pocket-core|ps2|retroarch|other|scummvm|ps2-starting|xbox>[:<row>]": the main page for that kind of emulator, a
    // row selected (`scummvm`: pocket-core without save states; `ps2-starting`: ARMSX2 before it named the game; `xbox`:
    // hakuX in RetroArch while its core has no save states, with `hakux::NO_STATES`);
    // "game-quick[:<row>][:own]": the quick settings opened from the pause menu of a pocket-core game (`:own`: it has
    // its own Performance in Options).
    let menu_kind = match name {
        "menu" => Some(state.split(':').nth(1).unwrap_or("pocket-core")),
        "game-quick" => Some("pocket-core"),
        _ => None,
    };
    let game_standalone = matches!(menu_kind, Some("ps2" | "ps2-starting" | "xbox"));
    // The guests that close with their window and keep their own saves: Wii U (Cemu), PS3 (RPCS3) and PS Vita (Vita3K) (Xbox: `game_standalone`).
    let guest = match menu_kind {
        Some("wiiu") => Some(("wiiu", "Mario Kart 8", "/storage/roms/wiiu/Mario Kart 8 (USA).wua", "cemu")),
        Some("ps3") => Some(("ps3", "Demon's Souls", "/storage/roms/ps3/Demon's Souls (USA).iso", "rpcs3")),
        // A PortMaster port: its pause menu, and (`port-perf`) Options → Performance & fan over the paused game.
        Some("port" | "port-perf") => Some(("ports", "Celeste Classic", "/storage/roms/ports/Celeste Classic.sh", "sh")),
        Some("psvita") => Some(("psvita", "Tearaway", "/storage/roms/psvita/Tearaway.psvita", "vita3k")),
        _ => None,
    };
    if let Some((system, name, path, emulator)) = guest {
        app.running = Some(Running { system: system.into(), name: name.into(), path: PathBuf::from(path), emulator: emulator.into(), started: Instant::now(), pgid: None, options: Default::default() });
    }
    if game_standalone {
        let xbox = menu_kind == Some("xbox");
        app.running = Some(Running {
            system: if xbox { "xbox" } else { "ps2" }.into(),
            name: if xbox { "Jade Empire" } else { "Okage: Shadow King" }.into(),
            path: PathBuf::from(if xbox { "/storage/roms/xbox/Jade Empire (USA, Europe).iso" } else { "/storage/roms/ps2/Okage - Shadow King (USA).chd" }),
            emulator: if xbox { "hakux" } else { "armsx2" }.into(),
            started: Instant::now(),
            pgid: None,
            options: Default::default(),
        });
    }
    app.menu = Some(QuickMenu {
        sel,
        frame,
        saved,
        states: Some(demo_states.clone()),
        slot,
        filled: demo_states.filled(),
        loads: demo_states.load_targets(),
        load_at,
        quitting: false,
        discarding: false,
        without_saving: false,
        window: None,
        frozen: None,
        retroarch: false,
        hosted: !game_standalone && guest.is_none(),
        page,
        settings: Some(settings),
        note: None,
        live: Default::default(),
        emu,
        cheats: snapshot_cheats(name, &tmp),
        // "ps2-options:<row>[:<step>]": a PS2, GameCube, Wii or 3DS game's options, the row's value stepped by <step>.
        emu_set: match name {
            _ if game_standalone => Some(emu_settings::Menu::demo(standalone::Emu::Armsx2, "ps2")),
            "ps2-options" | "gc-options" | "wii-options" | "3ds-options" | "gc-more" | "wii-more" => {
                let (emu, system) = match name {
                    "ps2-options" => (standalone::Emu::Armsx2, "ps2"),
                    "wii-options" | "wii-more" => (standalone::Emu::Dolphin, "wii"),
                    "3ds-options" => (standalone::Emu::Azahar, "3ds"),
                    _ => (standalone::Emu::Dolphin, "gamecube"),
                };
                let mut m = emu_settings::Menu::demo(emu, system);
                // "gc-more" and "wii-more": the page behind "More…".
                if name.ends_with("-more") {
                    m.open_more();
                }
                if let Some(&d) = nums.get(1) {
                    m.step(n(0), d as i32);
                }
                Some(m)
            }
            _ => None,
        },
        want_advanced: false,
        applying: false,
        emu_note: None,
        // "achievements-menu" (the item selected) and "achievements-list:<row>": a made-up game's list.
        achievements: name.starts_with("achievements-").then(app_achievements::demo_snapshot),
        pixel_item: false,
        pixel: None,
        players_item: false,
        players: None,
        saves: pocket_ui::pause_saves::Saves::Slots,
        disc: None,
        earlier: None,
        save_trouble: None,
    });
    // "pixel-path-menu": the main page with Pixel Path switched on and selected; "pixel-path:<case>[:<stage>[:about]]"
    // its page with a made-up report (`pixel_path::demo`: gba, snes, n64, runahead, compromised, pal, bluetooth; dolphin, reading);
    // "…:over" with Over the game on Pixel Path (the Y hint says Hide).
    if name == "pixel-path-menu" || name == "pixel-path" {
        if let Some(m) = app.menu.as_mut() {
            m.pixel_item = true;
            m.saved = None;
            // "pixel-path-menu:full": with Achievements too, the longest main page there is.
            if state.ends_with(":full") {
                m.achievements = Some(app_achievements::demo_snapshot());
            }
            m.sel = m.items().iter().position(|i| *i == MainItem::PixelPath).unwrap_or(0);
        }
    }
    // "players-menu": the main page with Players listed and selected; "players[:<case>[:<row>]]": its page with
    // made-up controllers (`app_players`: two, promoted, gap, four, five).
    if name == "players-menu" || name == "players" {
        app.players_snapshot(state);
    }
    // "…:save=full", "quit-unsaved[:<row>][:unwritable]": saves that don't reach the card (`app_save_trouble`).
    app.save_trouble_snapshot(state);
    if name == "pixel-path" {
        let mut it = state.split(':').skip(1);
        let case = it.next().unwrap_or("gba");
        let sel = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        if app.pixel_path_snapshot(case, sel, it.next() == Some("about")).is_none() {
            eprintln!("pixel-path: no such case {case}");
        }
        if state.ends_with(":over") {
            if let Some(s) = app.menu.as_mut().and_then(|m| m.settings.as_mut()) {
                s.values.insert(pocket_ui::over_game::KEY.into(), "pixel".into());
            }
        }
    }
    // Picture: a stand-in for pocket-core's preview from a raw game frame (`[game.png]`): the built-in
    // frame and the picture as the CPU draws them, without sharpness or effects.
    if let (Some(pic), Some(layout)) = (args.get(3).and_then(|p| image::load_image(std::path::Path::new(p))), app.preview_layout(w as i32, h as i32)) {
        let values = app.menu.as_ref().and_then(|m| m.settings.as_ref()).map(|s| s.effective()).unwrap_or_default();
        let video = pocket_ui::video::VideoSettings::from_values(&values);
        // Turned as pocket-core turns it (Screen orientation).
        let (pw, ph, px) = pocket_ui::video::turn_pixels(pic.w, pic.h, &pic.px, video.orientation.turns());
        let pic = image::Image { w: pw, h: ph, px };
        let rect = pocket_ui::video::dest_rect(video.scaling, w, h, pic.w, pic.h, 0.0);
        let style = video.fixed_style("gbc").unwrap_or(pocket_ui::bezel::style_for("gbc"));
        let frame = video.shows_frame().then(|| pocket_ui::bezel::generate(style, w, h, rect, rect.h as f32 / pic.h as f32)).flatten();
        let rgba = pocket_ui::bezel::compose(w, h, rect, frame.as_ref(), pic.w, pic.h, &pic.px);
        let (detail, whole) = preview::from_screen(&rgba, w, h, layout.detail.2 as u32, layout.detail.3 as u32);
        let img = |p: preview::Pixels| image::Image { w: p.w, h: p.h, px: p.px };
        if let Some(m) = app.menu.as_mut() {
            m.live.set(img(whole), img(detail));
        }
    }
    // "menu:<kind>:<row>:forget": as with Settings > Remember where I quit off.
    if menu_kind.is_some() && state.ends_with(":forget") {
        app.remember_quit = false;
    }
    let remember_quit = app.remember_quit;
    if let (Some(kind), Some(m)) = (menu_kind, app.menu.as_mut()) {
        let saver = match kind {
            "ps2" | "ps2-starting" | "xbox" => quit::Saver::Standalone,
            "retroarch" => quit::Saver::RetroArch,
            // Wii U, PS3 and PS Vita guests: closed by their window, no states of ours.
            "other" | "wiiu" | "ps3" | "psvita" => quit::Saver::Other,
            "port" | "port-perf" => quit::Saver::Port,
            "scummvm" => quit::Saver::PocketCore { states: false },
            _ => quit::Saver::PocketCore { states: true },
        };
        m.hosted = matches!(saver, quit::Saver::PocketCore { .. });
        m.retroarch = saver == quit::Saver::RetroArch;
        m.without_saving = quit_leaves_out_resume(remember_quit, saver);
        if matches!(saver, quit::Saver::Other | quit::Saver::Port) || kind == "ps2-starting" || kind == "xbox" {
            m.states = None;
        }
        let no_states = match kind {
            "scummvm" => hostsys::find("scummvm").and_then(|h| h.no_states),
            "xbox" => Some(pocket_ui::hakux::NO_STATES),
            _ => None,
        };
        m.saves = pocket_ui::pause_saves::Saves::of(saver, m.states.is_some(), no_states);
        if kind == "scummvm" {
            if let Some(r) = app.running.as_mut() {
                (r.system, r.name, r.path) = ("scummvm".into(), "Beneath a Steel Sky".into(), PathBuf::from("/storage/roms/scummvm/Beneath a Steel Sky.scummvm"));
            }
        }
        if !m.hosted {
            m.settings = None;
        }
        let rows = m.items();
        m.sel = if name == "game-quick" { rows.iter().position(|r| *r == MainItem::QuickSettings).unwrap_or(0) } else { n(0).min(rows.len() - 1) };
        m.saved = None;
    }
    // "menu:pocket-core:<row>:disc=<in>/<count>": a PlayStation game with a Disc row, selected (`app_disc`).
    app.disc_snapshot(state);
    // "slot-earlier[:start|:none|:damaged]": Earlier chosen (`app_earlier`), with `[preview.png]` as its picture.
    app.earlier_snapshot(state, args.get(2).and_then(|p| image::load_image(std::path::Path::new(p))));
    // ":battery=<case>": the pause menu's battery line with made-up measurements (`App::battery_snapshot`).
    app.battery_snapshot(state);
    // ":pixel=<armsx2|missing>": a PS2 game's menu with Pixel Path on, its settings readable or not (`app_pixel_path`).
    app.pixel_menu_snapshot(state);
    // "runs-on[:<row>]": a RetroArch game's pause menu → Options, the "Runs on" page over the dimmed game
    // (`app_emu_choice`; the GBA game of the snapshot library, as for `emu:`).
    if name == "runs-on" && app.emu_choice_snapshot_in_menu(n(0)) {
        if let Some(m) = app.menu.as_mut() {
            (m.hosted, m.retroarch, m.settings, m.page) = (false, true, None, Page::EmuChoice);
        }
    }
    // "menu:port-perf": the port's Options page, Performance & fan, over the paused game (`app_port_perf`).
    if menu_kind == Some("port-perf") {
        app.port_perf_wait_loaded();
        app.port_perf_in_menu();
    }
    let quick_over_game = name == "game-quick";
    if quick_over_game {
        app.sys.demo(&format!("quick-game:{}", n(0)), Instant::now());
        // ":own": the game has its own Performance in Options (`app_quick_profiles`).
        app.quick_profiles_snapshot(state);
    }
    // A port's Performance & fan is a page of the shelf, not of a game (`app_port_perf`).
    if state.starts_with("port-perf") {
        app.menu = None;
        app.running = None;
    }
    // "home": Home with its last row selected; "home:<row>" that row, counted from the end when negative ("home:0" the
    // first, "home:-2" the second to last).
    if state == "home" || state.starts_with("home:") {
        app.menu = None;
        app.running = None;
        let last = app.items.len().saturating_sub(1);
        app.sel = match state.strip_prefix("home:").and_then(|n| n.parse::<i64>().ok()) {
            Some(n) if n < 0 => last.saturating_sub((-n - 1) as usize),
            Some(n) => (n as usize).min(last),
            None => last,
        };
        app.prev_sel = app.sel;
    }
    // "switcher": the game switcher on the shelf, with the recent games of POCKETUI_DATA's recents.json.
    if state == "switcher" {
        app.menu = None;
        app.running = None;
        app.open_switcher(false, false);
    }
    // Home on a card with no games (`app_starter`).
    if state.starts_with("starter-home") {
        app.menu = None;
        app.running = None;
    }
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    app.paint(&mut c);
    if quick_over_game {
        app.sys.paint(&mut c, &mut app.text, false);
    }
    // Repeat the settled page with the live scaler, excluding PNG encoding, startup and compositor waits.
    // Thread CPU time stays meaningful while other worktrees compile on this host.
    if std::env::var_os("POCKETUI_FRAME_BENCH").is_some() {
        let panel = snapshot_panel_dims();
        let mut output = vec![0; panel.0 as usize * panel.1 as usize];
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        let cpu = || pocket_ui::run_ahead::thread_cpu_ms() * 1000.0;
        let mut paint_us = Vec::new();
        let mut frame_us = Vec::new();
        // Quick Settings is a separate overlay. Include it in every measured frame, as in the
        // first snapshot above, rather than timing only the paused page underneath it.
        let paint = |app: &mut App, c: &mut Canvas| {
            app.paint(c);
            if quick_over_game {
                app.sys.paint(c, &mut app.text, false);
            }
        };
        for i in 0..105 {
            let start = cpu();
            paint(app, &mut Canvas::new(&mut buf, w, h));
            let painted = cpu() - start;
            let start = cpu();
            if (w, h) == panel {
                paint(app, &mut Canvas::new(&mut output, w, h));
            } else {
                scaler.render(&mut output, panel, &mut |p, lw, lh| paint(app, &mut Canvas::new(p, lw, lh)));
            }
            let framed = cpu() - start;
            if i >= 4 { paint_us.push(painted); frame_us.push(framed); }
            std::hint::black_box(&output);
        }
        paint_us.sort_by(f64::total_cmp);
        frame_us.sort_by(f64::total_cmp);
        println!("bench-frame state={state} panel={}x{} logical={w}x{h} paint_us={:.0} frame_us={:.0} p95_us={:.0}",
            panel.0, panel.1, paint_us[50], frame_us[50], frame_us[95]);
    }
    write_png(&buf, w, h, out);
}

/// A 0RGB frame as an RGB PNG (`--snapshot`).
fn write_png(buf: &[u32], w: u32, h: u32, out: &str) {
    pocket_ui::text::boxes::finish(buf, w as i32, h as i32);
    let panel = snapshot_panel_dims();
    let mut reduced = Vec::new();
    let (buf, w, h) = if (w, h) != panel {
        reduced.resize(panel.0 as usize * panel.1 as usize, 0);
        pocket_ui::scaled_layout::ScaledFrame::default().downscale(buf, (w, h), &mut reduced, panel);
        (reduced.as_slice(), panel.0, panel.1)
    } else {
        (buf, w, h)
    };
    let rgb: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let file = std::fs::File::create(out).expect("create output");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&rgb)).expect("write png");
    println!("wrote {out}");
}

/// "41.2 GB free" for the file system holding `path`.
/// Settings > About's free space and address, read on a thread (`pocket_ui::kept`); the page's frames only look here.
static ABOUT: std::sync::LazyLock<std::sync::Arc<pocket_ui::kept::Kept<AboutValues>>> = std::sync::LazyLock::new(|| pocket_ui::kept::Kept::new("about-values"));
/// Free space ("12.3 GB free") and the Wi-Fi address; None each when unknown or not connected.
type AboutValues = (Option<String>, Option<String>);
/// How old the About page's values may get while it is shown.
const ABOUT_EVERY: Duration = Duration::from_secs(5);

/// "12.3 GB free" on the file system of `path`; a statvfs, which can wait for the card: never on the UI thread.
fn free_space(path: &str) -> Option<String> {
    let c_path = std::ffi::CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: statvfs only writes into the provided struct.
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let bytes = st.f_bavail as f64 * st.f_frsize as f64;
    Some(format!("{:.1} GB free", bytes / 1e9))
}

fn system_label(systems: &[System], name: &str) -> String {
    systems.iter().find(|s| s.name == name).map_or_else(|| name.to_string(), |s| s.fullname.clone())
}

/// Spine side label: the name people use for the console.
fn system_short(systems: &[System], name: &str) -> String {
    match name {
        "nes" => "NES".into(),
        "snes" => "SNES".into(),
        "gb" => "Game Boy".into(),
        "gbc" => "Game Boy Color".into(),
        "gba" => "GBA".into(),
        "n64" => "N64".into(),
        "psx" => "PS1".into(),
        "psp" => "PSP".into(),
        "nds" => "DS".into(),
        "megadrive" | "genesis" => "Mega Drive".into(),
        "ps2" => "PS2".into(),
        // A system that isn't on the shelf (no games) and has a short id: the id is its name ("ngp", "3do").
        _ if name.len() <= 4 && !systems.iter().any(|s| s.name == name) => name.to_uppercase(),
        _ => system_label(systems, name),
    }
}

/// Edge stripe colour per system; unknown systems get a muted colour derived from the name. Shared
/// with pocket-core, whose Glow frame lights up in it.
fn stripe(system: &str) -> u32 {
    pocket_ui::bezel::system_colour(system)
}

#[cfg(target_os = "linux")]
fn spawn_game(proxy: EventLoopProxy<UserEvent>, system: &str, emulator: &str, core: &str, path: &std::path::Path, env: &[(&str, String)]) -> Option<i32> {
    use std::os::unix::process::CommandExt;
    let started = Instant::now();
    if system == "ports" {
        // A PortMaster unpacked by an older pocket-ui lacks execute bits (gptokeyb): repair it before the port starts.
        if let Err(e) = pocket_ui::ports::ensure_ready() {
            eprintln!("ports: {e}");
        }
    }
    let mut cmd = if let Some(dir) = test_driver_dir() {
        // Same process group and exit notification as a game, with a temporary stand-in executable.
        let mut c = std::process::Command::new(dir.join("run-game"));
        c.arg(path);
        c
    } else if emulator == "pocket-core" {
        let mut c = std::process::Command::new(HOST_BIN);
        c.arg(pocket_ui::store::core_path(core)).arg(path).arg("--system").arg(system);
        // pocket-ui puts ROCKNIX's quantum back when the game ends (`ui_sound::game_ended`), in order with the shelf's own.
        c.env("POCKET_CORE_QUANTUM_OWNER", "pocket-ui");
        // A pocket-ui that crashes takes pocket-core with it (SIGTERM: it saves and quits).
        control::die_with_parent(&mut c);
        c
    } else if emulator == "hakux" {
        // Xbox: ROCKNIX's RetroArch on the hakuX core in pocket-ui's folder, with our own appendconfig (`hakux`,
        // `standalone::launch` has prepared it). Not runemu.sh: that loads cores from /tmp/cores and writes the config itself.
        let mut c = pocket_ui::hakux::command(path);
        c.current_dir("/tmp");
        c
    } else {
        let mut c = std::process::Command::new("/usr/bin/runemu.sh");
        c.arg(path).arg(format!("-P{system}")).arg(format!("--core={core}")).arg(format!("--emulator={emulator}")).arg("--controllers=");
        // A writable working directory: start_armsx2.sh tests `[ "$IRES" > "0" ]`, a redirect to a file "0",
        // which fails in the read-only / and drops the PS2 internal resolution back to 1x.
        c.current_dir("/tmp");
        c
    };
    cmd.envs(env.iter().map(|(k, v)| (*k, v)));
    // pocket-core announces its exit save on a pipe, so a switch needn't wait for the process to end (`fast_switch`).
    let handoff = (emulator == "pocket-core").then(|| pocket_ui::fast_switch::Handoff::attach(&mut cmd)).flatten();
    // Own process group, so pause and quit reach every process the game starts.
    let child = cmd.process_group(0).spawn();
    let pgid = child.as_ref().ok().map(|c| c.id() as i32);
    if test_driver_dir().is_some() {
        if let Some(pid) = pgid { println!("test game pid {pid}"); }
    }
    if let (Some(h), Some(g)) = (handoff, pgid) {
        let proxy = proxy.clone();
        h.watch(g, move || {
            let _ = proxy.send_event(UserEvent::GameSaved { pgid: g });
        });
    }
    std::thread::spawn(move || {
        let ok = child.ok().and_then(|mut c| c.wait().ok()).is_some_and(|s| s.success());
        pocket_ui::fast_switch::mark("game process ended");
        if let Some(g) = pgid {
            pocket_ui::fast_switch::note_ended(g);
        }
        let _ = proxy.send_event(UserEvent::GameExited { secs: started.elapsed().as_secs(), ok, pgid });
    });
    pgid
}

/// Starts an app (PC streaming, a tool) the way a game starts: its own process group, so Quit and the pause reach
/// everything it starts, and a `GameExited` when it ends. sway makes its windows fullscreen (`games_open_fullscreen`).
/// `cmd` runs in `/tmp` unless it names a folder of its own.
#[cfg(target_os = "linux")]
fn spawn_app(proxy: EventLoopProxy<UserEvent>, mut cmd: std::process::Command) -> Option<i32> {
    use std::os::unix::process::CommandExt;
    let started = Instant::now();
    // A pocket-ui that crashes takes the stream or the tool with it.
    control::die_with_parent(&mut cmd);
    if cmd.get_current_dir().is_none() {
        cmd.current_dir("/tmp");
    }
    let child = cmd.process_group(0).spawn();
    let pgid = child.as_ref().ok().map(|c| c.id() as i32);
    std::thread::spawn(move || {
        let ok = child.ok().and_then(|mut c| c.wait().ok()).is_some_and(|s| s.success());
        let _ = proxy.send_event(UserEvent::GameExited { secs: started.elapsed().as_secs(), ok, pgid });
    });
    pgid
}

#[cfg(not(target_os = "linux"))]
fn spawn_app(proxy: EventLoopProxy<UserEvent>, _cmd: std::process::Command) -> Option<i32> {
    // Desktop preview: pretend the stream failed to start after two seconds.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let _ = proxy.send_event(UserEvent::GameExited { secs: 2, ok: false, pgid: None });
    });
    None
}

#[cfg(not(target_os = "linux"))]
fn spawn_game(proxy: EventLoopProxy<UserEvent>, _system: &str, _emulator: &str, _core: &str, _path: &std::path::Path, _env: &[(&str, String)]) -> Option<i32> {
    // Desktop preview: pretend the game ran for two seconds.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let _ = proxy.send_event(UserEvent::GameExited { secs: 2, ok: true, pgid: None });
    });
    None
}

/// Opens the pads and the touch screen on a thread of their own, first thing in `main`. Walking every input device
/// takes 16-25 ms in the virtual device, more on the Mini V2; the shelf doesn't wait for it, and the events wait in the
/// returned `Early` until the event loop can take them (`main` attaches it). Pads that appear later (Bluetooth after a
/// deep sleep, InputPlumber restarted) are picked up by the supervisor; the touch screen is supervised the same way
/// (`touch::linux::supervise`: looked for until it is there, opened again after a read error).
#[cfg(target_os = "linux")]
fn start_input(started: Instant) -> std::sync::Arc<pocket_ui::pad::Early<UserEvent>> {
    let early = std::sync::Arc::new(pocket_ui::pad::Early::default());
    let out = early.clone();
    std::thread::Builder::new()
        .name("input-start".into())
        .spawn(move || {
            let pads = early.clone();
            let found = pocket_ui::pad::linux::supervise(move |e| pads.send(UserEvent::Pad(e)));
            // When the pads could take presses, against the first frame's time in the same log (docs/LATENCY.md).
            println!("pad: {} devices open {:.0} ms after the start", found.len(), started.elapsed().as_secs_f64() * 1000.0);
            pocket_ui::mark!("pad-open", "{}", found.len());
            // The touch screen is kept like the pads: looked for until it is there, opened again after a read error
            // (an incident of 2026-10-26: touch was dead for a while after a boot). Blocks on this thread.
            pocket_ui::touch::linux::supervise(move |t| early.send(UserEvent::Touch(t)));
        })
        .expect("input thread");
    out
}

/// A private headless harness for user journeys. Normal starts never enable it, even with mock hardware.
/// Its path contains the command FIFO, captures and a stand-in game runner; it never borrows a running UI's FIFO.
fn test_driver_dir() -> Option<&'static PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let args: Vec<_> = std::env::args().collect();
        let at = args.iter().position(|a| a == "--test-driver")?;
        assert!(std::env::var_os("POCKETUI_MOCK").is_some(), "--test-driver requires POCKETUI_MOCK");
        Some(PathBuf::from(args.get(at + 1).expect("--test-driver needs a temporary directory")))
    }).as_ref()
}

/// Reads test commands from a FIFO so the UI can be driven over SSH.
#[cfg(target_os = "linux")]
fn spawn_command_fifo(proxy: EventLoopProxy<UserEvent>) {
    use std::io::BufRead;
    let fifo = test_driver_dir().map_or_else(|| PathBuf::from("/tmp/pocket-ui.in"), |d| d.join("ui.in"));
    let _ = std::fs::remove_file(&fifo);
    if !pocket_ui::proc::status_with_timeout(std::process::Command::new("mkfifo").arg(&fifo), pocket_ui::proc::HELPER).is_ok_and(|s| s.success()) {
        return;
    }
    // Opened read+write on purpose: the UI itself then always holds a write end, so the FIFO never reaches EOF between
    // two `echo > fifo` writers. A read-only open saw EOF after each writer closed, dropped the read end and reopened
    // it; a command written in that gap was lost (the journey tests hung) or the writer got "Broken pipe".
    let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(&fifo) else { return };
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            let button = match line.trim() {
                "frame" if test_driver_dir().is_some() => {
                    let _ = proxy.send_event(UserEvent::TestFrame);
                    continue;
                }
                "up" => Button::Up,
                "down" => Button::Down,
                "left" => Button::Left,
                "right" => Button::Right,
                "a" => CONFIRM,
                "b" => CANCEL,
                "l1" => Button::L1,
                "r1" => Button::R1,
                "l2" => Button::L2,
                "r2" => Button::R2,
                "home" => Button::Mode,
                // The roles of the hints (the top button is X, the left one Y), in both Button layouts: the FIFO is
                // not a physical pad (`button_layout`). They were crossed until the Button layout setting.
                "x" => Button::North,
                "y" => Button::West,
                "select" => Button::Select,
                "start" => Button::Start,
                "power" => Button::Power,
                "quit" => {
                    let _ = proxy.send_event(UserEvent::Quit);
                    continue;
                }
                // What SELECT + START held opens: the dialog before EmulationStation (`app_rescue`).
                "leave" => {
                    let _ = proxy.send_event(UserEvent::Leave);
                    continue;
                }
                "poweroff" => {
                    let _ = proxy.send_event(UserEvent::PowerOff);
                    continue;
                }
                // The power button held: "powerdown", wait, "powerup" (a long press opens the menu).
                "powerdown" | "powerup" => {
                    let pressed = line.trim() == "powerdown";
                    let _ = proxy.send_event(UserEvent::Pad(PadEvent::Button { source: usize::MAX, button: Button::Power, pressed }));
                    continue;
                }
                "powermenu" => {
                    let _ = proxy.send_event(UserEvent::PowerMenu);
                    continue;
                }
                _ => continue,
            };
            for pressed in [true, false] {
                let _ = proxy.send_event(UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed }));
            }
        }
    });
}

impl App {
    /// Where the menus stand: a change after a direction button is what a haptic tick answers.
    fn ui_position(&self) -> impl PartialEq {
        (
            self.screen,
            self.sel,
            self.settings_page,
            self.menu.as_ref().map(|m| (m.page, m.sel, m.slot, m.settings.as_ref().map(|s| s.values.clone()), m.cheats.as_ref().map(|c| c.enabled()), m.players.as_ref().map(|p| p.mark()))),
            self.sync_row(),
            self.tailscale_row(),
            self.bluetooth_row(),
            self.perf_fan_row(),
            self.curator_mark(),
            self.achievements_row(),
            self.find_mark(),
            // feat/tools-wifi's pages together: a tuple compares at most twelve fields.
            (self.tools_row(), self.pc_streaming_row(), self.backup_row(), self.emu_choice_mark(), self.tour_mark(), self.web_row(), self.night_light_row(), self.storage_row(), self.gamelist_row(), self.gallery_row(), self.this_game_mark()),
        )
    }

    /// The screen as the UI sounds see it: what covers it, and a fingerprint of everything that moves.
    fn sound_view(&self) -> pocket_ui::ui_sound::View {
        use pocket_ui::ui_sound::{Overlay, View};
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.screen, self.sel, self.settings_page, self.updates.sel, self.theme, self.sync_row(), self.tailscale_row(), self.bluetooth_row(), self.perf_fan_row(), self.achievements_row(), self.find_mark(), self.curator_mark()).hash(&mut h);
        // The pages of feat/tools-wifi, apart: a tuple hashes at most twelve fields.
        (self.tools_row(), self.pc_streaming_row(), self.backup_row(), self.web_row(), self.night_light_row(), self.starter.mark(), self.storage_row(), self.gamelist_row(), self.gallery_row()).hash(&mut h);
        if let Some(m) = &self.menu {
            (m.page, m.sel, m.slot, m.quitting, m.settings.as_ref().map(|s| &s.values), m.cheats.as_ref().map(|c| c.enabled()), m.pixel.as_ref().map(|p| p.mark())).hash(&mut h);
            m.players.as_ref().map(|p| p.mark()).hash(&mut h);
        }
        if let Some(sw) = &self.switcher {
            (sw.sel, sw.entries.len(), sw.entries.iter().filter(|e| e.favorite).count(), sw.switching).hash(&mut h);
        }
        (self.setup.as_ref().map(|s| s.ui_mark()), self.store.ui_mark(), self.sys.ui_mark(), self.tour_mark(), self.frames, self.over_game).hash(&mut h);
        if let Some(open) = &self.card {
            (open.card.scroll, open.card.mode_mark(), open.card.data.status, &open.card.data.note).hash(&mut h);
        }
        if let Some((page, _)) = &self.port_files {
            (&page.dir, page.scroll()).hash(&mut h);
        }
        if let Some(open) = &self.console {
            (open.system, open.page.mark()).hash(&mut h);
        }
        (pocket_ui::rumble::menu_haptics(), self.sound, self.sleep.config, self.toast.as_ref().map(|t| t.1), self.pixel_path).hash(&mut h);
        self.sleep_page.sel.hash(&mut h);
        self.emu_choice_mark().hash(&mut h);
        self.this_game_mark().hash(&mut h);
        self.port_perf_mark().hash(&mut h);
        self.conflicts_mark().hash(&mut h);
        let overlay = if self.sys.power_menu_open() {
            Overlay::PowerMenu
        } else if self.switcher.is_some() {
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

    /// Plays what `input` sounds like, now that it turned `before` into the screen as it is.
    fn sound_after(&mut self, input: pocket_ui::ui_sound::Input, before: pocket_ui::ui_sound::View) {
        use pocket_ui::ui_sound;
        let refused = ui_sound::take_refused();
        if let Some(sound) = ui_sound::sound_for(input, before, self.sound_view(), refused) {
            self.play_sound(sound);
        }
    }

    fn user_event_inner(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        // The dialog before EmulationStation takes the buttons while it is open (`app_rescue`).
        if let UserEvent::Pad(pe) = &ev {
            if self.leave_input(el, pe) {
                return;
            }
        }
        // Volume keys and their combos, and the quick settings and time screens, come first.
        if let UserEvent::Pad(pe) = &ev {
            // START + Vol is brightness, not START; HOME + Vol is ROCKNIX's brightness, and releasing HOME after it
            // is no tap that opens the pause menu (the volume key never reaches `in_game_input`, which would see that).
            if matches!(pe, PadEvent::Button { button: Button::VolUp | Button::VolDown, pressed: true, .. }) {
                self.start_tap.disarm();
                self.home_tap = false;
            }
            let ctx = self.sys_ctx();
            let out = self.sys.pad(pe, &ctx, Instant::now());
            if out.select_used {
                self.select_alone = false;
            }
            self.volume_step |= out.volume_step;
            // B or the power button closed the power menu: a game it paused goes on.
            if out.power_cancelled && std::mem::take(&mut self.power_paused_game) {
                self.resume_game(None);
            }
            if out.consumed {
                self.request_redraw();
                return;
            }
        }
        match ev {
            UserEvent::Quit => el.exit(),
            UserEvent::TestFrame => {
                if let Some(dir) = test_driver_dir() {
                    let (w, h) = snapshot_dims();
                    let mut buf = vec![0; (w * h) as usize];
                    let mut c = Canvas::new(&mut buf, w, h);
                    self.paint(&mut c);
                    self.sys.paint(&mut c, &mut self.text, self.menu.is_none());
                    self.paint_leave(&mut c);
                    write_png(&buf, w, h, dir.join("frame.png").to_str().unwrap());
                }
            }
            UserEvent::Leave => self.open_leave(pocket_ui::rescue::DialogKind::Leave),
            // Handled in `user_event`.
            UserEvent::Touch(_) => {}
            UserEvent::Sleep => {
                // Asked for from the power menu in a game it paused itself: back into the game first, so
                // that it is the game that sleeps and comes back (not a pause menu nobody opened).
                if std::mem::take(&mut self.power_paused_game) {
                    self.resume_game(None);
                }
                let ctx = self.sleep_ctx();
                self.sleep.sleep_now(ctx);
            }
            // Nothing to do here: `about_to_wait` runs `Sleep::tick` after this event, and that reads the news.
            UserEvent::SleepNews => {}
            UserEvent::PowerOff => self.sleep.power_off(),
            UserEvent::Shutdown(action) => {
                self.power_paused_game = false;
                self.sleep.shut_down(action);
            }
            UserEvent::PowerMenu => self.open_power_menu(),
            UserEvent::Preview { seq, images } => {
                if !self.menu.as_mut().is_some_and(|m| m.live.receive(seq, images)) {
                    return;
                }
            }
            UserEvent::GameExited { secs, ok, pgid } => {
                if !self.drained(pgid, ok) {
                    self.game_exited(secs, ok)
                }
            }
            UserEvent::GameSaved { pgid } => self.game_saved(pgid),
            UserEvent::MenuShot { seq, image } => self.menu_shot_arrived(seq, image),
            UserEvent::OptionsSaved(saved) => self.options_saved(saved),
            UserEvent::Sync(e) => self.sync_event(e),
            UserEvent::Tailscale(e) => self.tailscale_event(e),
            UserEvent::Web(e) => self.web_event(e),
            UserEvent::Bluetooth(e) => self.bluetooth_event(e),
            UserEvent::PerfFan(e) => self.perf_fan_event(e),
            UserEvent::Night(level) => self.night_light_event(level),
            UserEvent::Tools(e) => self.tools_event(e),
            UserEvent::Moonlight(e) => self.pc_streaming_event(e),
            UserEvent::Backup(e) => self.backup_event(e),
            UserEvent::Storage(e) => self.storage_event(e),
            UserEvent::Gamelist(index) => self.gamelist_event(index),
            UserEvent::Gallery(e) => self.gallery_event(e),
            UserEvent::Curator(e) => self.curator_event(e),
            UserEvent::Achievements(e) => self.achievements_event(e),
            UserEvent::PixelPath(r) => self.pixel_path_event(*r),
            UserEvent::PixelSetup(rom, result) => self.pixel_setup_event(rom, *result),
            UserEvent::Players => self.players_event(),
            // `merge_pad` turned it into `Pad` already.
            UserEvent::Merged(_) => {}
            UserEvent::Update(e) => self.update_event(e),
            UserEvent::Store(e) => self.store_event(e),
            UserEvent::Starter(e) => self.starter_event(e),
            UserEvent::Covers(e) => self.covers_event(e),
            UserEvent::Texts(e) => self.texts_event(e),
            UserEvent::Identify(e) => self.identify_event(e),
            UserEvent::Picture(key, img) => self.picture_arrived(key, img),
            UserEvent::Online => self.auto_covers("online after boot"),
            UserEvent::LibraryRescanned(systems, key) => self.library_rescanned(systems, key),
            UserEvent::Display(tv) => self.display_changed(tv),
            // A frame only while the corner is on screen (every event below ends in one).
            UserEvent::StatusCorner => {
                if !self.status_corner_shown() {
                    return;
                }
            }
            // A frame only while a category's list or the About page shows them.
            UserEvent::SettingsValues => {
                if !matches!(self.settings_page, Some(SettingsPage::List(_) | SettingsPage::About)) {
                    return;
                }
            }
            // Only for the game still starting: a picture that comes after the game ended or another one started is dropped.
            UserEvent::LaunchPicture(p) => {
                let wanted = self.launch_at.is_some() && self.running.as_ref().is_some_and(|r| p.as_ref().is_some_and(|p| p.rom == r.path));
                if wanted {
                    self.launch_picture = p;
                    self.request_redraw();
                }
            }
            UserEvent::Setup(e) => {
                if e.changes_library() {
                    self.covers.clear();
                    self.pictures_changed();
                    self.look_previews = None;
                }
                if let Some(setup) = self.setup.as_mut() {
                    setup.on_event(e);
                }
            }
            // The manual also opens over the first-start setup (SELECT) and closes back to it; it takes its own input.
            UserEvent::Pad(pe) if self.settings_page == Some(SettingsPage::Manual) => self.manual_input(pe),
            UserEvent::Pad(PadEvent::Button { button: Button::Select, pressed: true, .. }) if self.setup.is_some() => {
                self.manual_from_setup = true;
                self.settings_page = Some(SettingsPage::Manual);
            }
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.setup.is_some() => {
                let before_wifi = self.setup.as_ref().is_some_and(|s| !s.past_wifi());
                let done = self.setup.as_mut().is_some_and(|s| s.press(button));
                // Y on the look step: the new appearance shows at once and is kept.
                if let Some(a) = self.setup.as_ref().map(|s| s.appearance()).filter(|a| *a != self.appearance) {
                    self.set_appearance(a);
                }
                // Left/Right on the welcome step: the language is used at once and kept.
                if let Some(lang) = self.setup.as_mut().and_then(|s| s.take_language()) {
                    self.set_language(lang);
                }
                // R1 on the first start's look step: the text size is used at once (the setup set it) and kept.
                if let Some(size) = self.setup.as_mut().and_then(|s| s.take_text_size()) {
                    self.set_text_size(size);
                }
                // L1 on the look step: Cozy or Quick, kept (the look decides only until then).
                if let Some(a) = self.setup.as_mut().and_then(|s| s.take_animations()) {
                    self.animations = Some(a);
                    pocket_ui::motion::set_cozy(self.cozy());
                    self.save_settings();
                }
                // R1 on Settings > Look: Motion is used at once and kept.
                if let Some(m) = self.setup.as_ref().map(|s| s.motion()).filter(|m| *m != pocket_ui::motion::level()) {
                    pocket_ui::motion::set_level(m);
                    self.night_light_motion_changed();
                    self.save_settings();
                }
                // Past the first start's Wi-Fi step: covers for the games already on the card.
                if before_wifi && self.setup.as_ref().is_some_and(|s| s.past_wifi()) {
                    self.auto_covers("after the Wi-Fi step");
                }
                // A tick in Systems on the shelf is saved at once.
                if let Some(hidden) = self.setup.as_mut().and_then(|s| s.take_hidden()) {
                    self.hidden = hidden;
                    self.look_previews = None;
                    self.save_settings();
                    self.rebuild();
                }
                // Settings > Games > Extras > Pictures: kept at once; a choice that brings more downloads starts them,
                // Off stops the one that runs (after the file it is on).
                if let Some(art) = self.setup.as_mut().and_then(|s| s.take_art()) {
                    self.pictures.set_wanted(art == pocket_ui::settings::Art::Pictures);
                    self.art_off = art == pocket_ui::settings::Art::Off;
                    self.save_settings();
                    if self.art_off {
                        self.cover_run.game_started();
                    } else {
                        self.auto_covers("game pictures switched on");
                    }
                }
                if let Some(on) = self.setup.as_mut().and_then(|s| s.take_streaming()) {
                    self.streaming_on = on;
                    self.save_settings();
                    self.rebuild();
                }
                // A profile or a feature card (`features`): each switch is used at once and saved.
                if let Some((switches, later)) = self.setup.as_mut().and_then(|s| s.take_tour()) {
                    self.tour.later = later;
                    self.adopt_switches(&switches);
                    self.save_settings();
                }
                if done {
                    self.finish_setup();
                }
            }
            UserEvent::Pad(_) if self.setup.is_some() => {}
            // The feature cards, over Settings or the shelf (`app_tour`).
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.tour.is_open() => self.tour_input(button),
            UserEvent::Pad(_) if self.tour.is_open() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.settings_page.is_some() => {
                self.settings_input(button);
            }
            UserEvent::Pad(_) if self.settings_page.is_some() => {}
            // The game switcher on the shelf; in a game it goes through `in_game_input`.
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.switcher.is_some() && self.running.is_none() => {
                self.switcher_input(button);
            }
            UserEvent::Pad(_) if self.switcher.is_some() && self.running.is_none() => {}
            UserEvent::CardText { title, text } => self.card_text(title, text),
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.port_files.is_some() && self.running.is_none() => {
                self.port_files_input(button);
            }
            UserEvent::Pad(_) if self.port_files.is_some() && self.running.is_none() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.curator.page.is_some() && self.running.is_none() => {
                self.curator_input(button);
            }
            UserEvent::Pad(_) if self.curator.page.is_some() && self.running.is_none() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.emu_choice_on_shelf() => {
                self.emu_choice_input(button);
            }
            UserEvent::Pad(_) if self.emu_choice_on_shelf() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.this_game_open() => {
                self.this_game_input(button);
            }
            UserEvent::Pad(_) if self.this_game_open() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.port_perf_open() => {
                self.port_perf_input(button);
            }
            UserEvent::Pad(_) if self.port_perf_open() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.gallery_over_card() => {
                self.gallery_card_input(button);
            }
            UserEvent::Pad(_) if self.gallery_over_card() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.conflicts_over_card() => {
                self.conflicts_input(button);
            }
            UserEvent::Pad(_) if self.conflicts_over_card() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.card.is_some() && self.running.is_none() => {
                self.card_input(button);
            }
            UserEvent::Pad(_) if self.card.is_some() && self.running.is_none() => {}
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.console.is_some() && self.running.is_none() => {
                self.console_input(button);
            }
            UserEvent::Pad(_) if self.console.is_some() && self.running.is_none() => {}
            // The search (`app_search`), under a card opened from it.
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) if self.find.is_open() && self.running.is_none() => {
                self.search_input(button);
            }
            UserEvent::Pad(_) if self.find.is_open() && self.running.is_none() => {}
            // Input belongs to the game while one runs; only a HOME tap and the quick menu are ours.
            UserEvent::Pad(pe) if self.running.is_some() => {
                // Any button skips the launch animation (the game, started already, gets the press as well).
                if matches!(pe, PadEvent::Button { pressed: true, .. }) && self.cabinet.skip_insert(Instant::now()) {
                    self.launch_at = self.launch_at.map(|t| t.min(Instant::now() - FADE));
                    self.request_redraw();
                }
                // The window is behind the game: its input (sticks, buttons, a hundred events a second) draws nothing here.
                let screen_before = self.menu.is_some() || self.switcher.is_some();
                self.in_game_input(pe);
                if !pocket_ui::idle::game_input_needs_redraw(screen_before, self.menu.is_some() || self.switcher.is_some()) {
                    return;
                }
            }
            UserEvent::Pad(pe) => {
                // `self.pad` has this event already (`user_event`: every controller merged).
                if let PadEvent::Button { button, pressed, .. } = pe {
                    if pressed {
                        self.start_tap.pressed(button, self.pad.is_pressed(Button::Select));
                        self.press(button);
                    } else {
                        // START on its own opens Settings, on the shelf and in a system's list alike; B comes back.
                        if self.start_tap.released(button) {
                            self.settings_page = Some(SettingsPage::Groups(0));
                        }
                        if button == Button::Select && std::mem::take(&mut self.select_alone) {
                            self.open_switcher(false, false);
                        }
                    }
                }
                let combo = self.pad.is_pressed(Button::Select) && self.pad.is_pressed(Button::Start);
                self.exit_since = if combo { self.exit_since.or(Some(Instant::now())) } else { None };
            }
        }
        self.request_redraw();
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if test_driver_dir().is_some() { return; }
        if self.window.is_some() {
            return;
        }
        pocket_ui::startup::mark("loop");
        let attrs = Window::default_attributes()
            .with_title("pocket-ui")
            .with_fullscreen(Some(Fullscreen::Borderless(None)));
        // sway addresses the window by app_id when the quick menu takes over from the game.
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs.with_name(control::OWN_APP_ID, control::OWN_APP_ID)
        };
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                // Without a window there is nothing to show; exit non-zero so the launcher starts us again.
                eprintln!("couldn't create the window: {e}");
                self.exit_code = 1;
                el.exit();
                return;
            }
        };
        // Made here and again whenever a frame fails (see `frame`); drawing makes the first one.
        let for_surface = window.clone();
        self.surface.set_factory(move || {
            let context = softbuffer::Context::new(for_surface.clone()).map_err(|e| format!("context: {e}"))?;
            softbuffer::Surface::new(&context, for_surface.clone()).map_err(|e| format!("surface: {e}"))
        });
        self.window = Some(window);
        pocket_ui::startup::mark("window");
        self.request_redraw();
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        // Several controllers (review SOL-09): what goes on is what changed for all of them together (`merge_pad`).
        let Some(ev) = self.merge_pad(el, ev) else { return };
        if matches!(ev, UserEvent::Pad(_) | UserEvent::Touch(_)) {
            self.last_input = Instant::now();
        }
        // Speed guard: numbered so a frame can say which input it shows (docs/LATENCY.md).
        if let UserEvent::Pad(PadEvent::Button { button, pressed, .. }) = &ev {
            pocket_ui::mark!("input", "{} {:?} {}", pocket_ui::trace::input(), button, if *pressed { "down" } else { "up" });
        }
        if matches!(ev, UserEvent::Pad(PadEvent::Button { pressed: true, .. })) {
            self.motion_input();
        }
        if let UserEvent::Touch(t) = ev {
            self.touch_event(el, t);
            return;
        }
        // The shelf's key repeat ends with the release or another press, whatever page they go to (SOL-08).
        if let UserEvent::Pad(PadEvent::Button { button, pressed, .. }) = &ev {
            self.held = repeat_after(self.held, *button, *pressed);
        }
        // Sleep sees every pad event first: waking up, the power key, and input that keeps it awake.
        if let UserEvent::Pad(pe) = &ev {
            if self.sleep.on_pad(pe, self.sleep_ctx()) {
                // A power button released after the threshold the timer had not seen yet.
                if self.sleep.take_menu_request() {
                    self.open_power_menu();
                }
                self.request_redraw();
                return;
            }
        }
        // The Mini V2's L2/R2 are analog: outside games they press like buttons (letter jump, `app_search`).
        let Some(ev) = self.trigger_event(ev) else { return };
        let moved_from = (pocket_ui::rumble::menu_haptics()
            && matches!(&ev, UserEvent::Pad(PadEvent::Button { button: Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1 | Button::L2 | Button::R2, pressed: true, .. })))
        .then(|| self.ui_position());
        let sound_input = match &ev {
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) => Some(pocket_ui::ui_sound::Input::Press(*button)),
            UserEvent::Pad(PadEvent::Button { button, pressed: false, .. }) => Some(pocket_ui::ui_sound::Input::Release(*button)),
            _ => None,
        };
        let sound_from = sound_input.map(|_| self.sound_view());
        self.volume_step = false;
        self.user_event_inner(el, ev);
        if moved_from.is_some_and(|before| before != self.ui_position()) {
            pocket_ui::rumble::tick();
        }
        if let (Some(input), Some(before)) = (sound_input, sound_from) {
            let input = if std::mem::take(&mut self.volume_step) { pocket_ui::ui_sound::Input::VolumeStep } else { input };
            self.sound_after(input, before);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => self.request_redraw(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, repeat: false, .. },
                ..
            } => {
                // Escape asks first, as SELECT + START does; in the dialog it is B.
                if code == KeyCode::Escape && self.leave.is_none() {
                    if state == ElementState::Pressed {
                        self.open_leave(pocket_ui::rescue::DialogKind::Leave);
                    }
                    return;
                }
                let button = match code {
                    KeyCode::ArrowUp => Button::Up,
                    KeyCode::ArrowDown => Button::Down,
                    KeyCode::Enter => CONFIRM,
                    KeyCode::Backspace | KeyCode::Escape => CANCEL,
                    KeyCode::KeyQ => Button::L1,
                    KeyCode::KeyE => Button::R1,
                    _ => return,
                };
                let pressed = state == ElementState::Pressed;
                self.user_event(el, UserEvent::Pad(PadEvent::Button { source: 0, button, pressed }));
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // SELECT + START held: the dialog (Restart pocket-ui, Open EmulationStation), never EmulationStation at once.
        if self.exit_since.is_some_and(|t| t.elapsed() >= EXIT_HOLD) {
            self.open_leave(pocket_ui::rescue::DialogKind::Leave);
        }
        // A frame is on screen and the loop has turned since: a freshly installed version works, and the launcher's
        // black screen guard is told (`rescue::note_first_frame`, once, off this thread).
        if self.drew_first && test_driver_dir().is_none() {
            update::mark_frame_presented();
            pocket_ui::rescue::note_first_frame();
        }
        // The search index after the first frame, and what its threads brought (`app_search`).
        self.find_tick();
        // A start whose first frame doesn't come (a game started at once, sway slow): what waited for it runs anyway.
        if !self.drew_first && !self.after_first_done && self.started.elapsed() >= AFTER_FIRST_FRAME_AT_LATEST {
            self.after_first_frame();
        }
        // A game list read again after the start, swapped in once nothing holds a game by its place (`app_library`).
        self.library_apply_pending();
        // "New in this version", once, when the shelf is free (`app_tour`).
        self.tour_tick();
        let now = Instant::now();
        STICKS.set_quiet(self.running.is_some() && self.menu.is_none() && self.switcher.is_none() && !self.sys.is_open());
        if quit::restart_due(self.updates.restart_at, now, self.running.is_some()) {
            self.exit_code = update::RESTART_CODE;
            el.exit();
            return;
        }

        // A game that ignored Quit: SIGTERM at the deadline, SIGKILL two seconds later.
        if let (Some(q), Some(g)) = (self.quit.as_mut(), self.running.as_ref().and_then(|r| r.pgid)) {
            if let Some(sig) = q.due(now) {
                control::terminate_game(g, sig == quit::Signal::Kill);
            }
        }
        // The same for a game still ending after it handed over (`app_fast_switch`).
        let draining = self.drain_tick(now);
        if let Some((b, next)) = self.held {
            if now >= next {
                let before = self.sound_view();
                self.step(b);
                self.sound_after(pocket_ui::ui_sound::Input::Repeat(b), before);
                // A held L2/R2 goes letter by letter slowly enough to read (`letters::REPEAT`).
                let every = if matches!(b, Button::L2 | Button::R2) { pocket_ui::letters::REPEAT } else { REPEAT_EVERY };
                self.held = Some((b, now + every));
                self.request_redraw();
            }
        }
        // The manual scrolls on while Up/Down is held or the stick is pushed.
        if self.settings_page == Some(SettingsPage::Manual) {
            if let Some((b, _)) = self.manual_hold.filter(|(_, at)| now >= *at) {
                self.manual.set_velocity(if b == Button::Up { -MANUAL_HELD } else { MANUAL_HELD }, now);
                self.manual_hold = None;
            }
            if self.manual.tick(now) {
                self.request_redraw();
            }
        } else if self.manual.moving() {
            self.manual.set_velocity(0.0, now);
        }
        let ctx = self.sys_ctx();
        if self.sys.tick(&ctx, now) {
            self.request_redraw();
        }
        let mut wake = self.held.map(|(_, next)| next).or(self.quit.as_ref().and_then(Escalation::next));
        wake = match (wake, draining) {
            (Some(w), Some(d)) => Some(w.min(d)),
            (w, d) => w.or(d),
        };
        if let Some((_, at)) = self.manual_hold {
            wake = Some(wake.map_or(at, |w| w.min(at)));
        }
        let asleep = self.sleep.tick(self.sleep_ctx());
        // The power menu's Restart pocket-ui: the game is saved and quit; exit 0 is the launcher's "start it again".
        if self.sleep.take_restart() {
            println!("power menu: Restart pocket-ui");
            self.exit_code = 0;
            el.exit();
            return;
        }
        // Awake again: a frame, so the corner's clock and battery are those of now (the minute's wake-up runs on the
        // monotonic clock, which stood still in the suspend). The screen is on already; this is after it.
        if self.sleep.take_woke() {
            self.request_redraw();
        }
        // The fan outside games follows games, sleep and the Settings page.
        self.perf_fan_sync();
        // The night light looks at the clock again after a sleep (`app_night_light`).
        self.night_light_tick();
        // The web companion: starts once the Pocket is up; what it reads follows games and Curator (`app_web.rs`).
        self.web_sync();
        // Curator's index, on its own thread, once the first frame is up and no game runs.
        self.curator_tick();
        // The power button has been held long enough.
        if self.sleep.take_menu_request() {
            self.open_power_menu();
        }
        // The battery reached the warning level (`low_battery`, review B12): a notice on the shelf, and the level
        // pill over a pocket-core game. The critical level is `Sleep`'s own (save, then power off).
        if let Some(percent) = self.sleep.take_battery_warning() {
            if self.running.as_ref().is_some_and(Running::hosted) {
                let pill = pocket_ui::osd::Data::new("Battery low", format!("{percent}%"), percent as f32 / 100.0, false);
                control::pocket_core(&format!("osd {}", pill.to_wire()));
            }
            self.notice = Some(Notice::new(pocket_ui::low_battery::warning_text(percent)));
            self.request_redraw();
        }
        // Is the clock clearly wrong? One notice on the shelf, some time after the start (`app_clock`).
        if let Some(t) = self.clock_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // The Battery page follows the minute samples while it is open.
        let battery_page = matches!(self.settings_page, Some(SettingsPage::Battery(_))).then(|| now + Duration::from_secs(30));
        for t in [asleep, battery_page].into_iter().flatten() {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if battery_page.is_some() {
            self.request_redraw();
        }
        // The pairing code's countdown on Settings > Web companion: a frame a second while it shows.
        if let Some(t) = self.web_wake(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if let Some(t) = self.appearance_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // The status corner's clock: one wake-up as the minute turns, only while it is on screen (`app_status_corner`).
        if let Some(t) = self.status_corner_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if let Some(t) = self.sys.next_wake() {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // Split's preview changes picture now and then (`app_pictures`).
        if let Some(t) = self.pictures_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // Up next: frames while it slides in, then a wake-up when it goes by itself (`app_curator`).
        if let Some(t) = self.curator_up_next_wake(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if self.animating() {
            // One frame per refresh: redrawing as soon as the last frame is out ran animations at thousands
            // of frames a second, which a scrolling title would keep up for as long as it is selected.
            let due = self.frame_at + FRAME;
            if now >= due {
                self.request_redraw();
            }
            let next = if now >= due { now + FRAME } else { due };
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
        // A frame the compositor refused is asked for again after a pause (see `frame`).
        if let Some(t) = self.surface.retry_at() {
            let next = if now >= t { now + FRAME } else { t };
            if now >= t {
                self.request_redraw();
            }
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
        // A frame held back for the window's full size (`app_menu_open`): painted when the wait is over, resized or
        // not. Without this the pause menu stayed black on a screen where sway sends no resize (H700).
        if let Some(t) = self.held_frame_due() {
            let next = if now >= t { now + FRAME } else { t };
            if now >= t {
                self.request_redraw();
            }
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
        el.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

impl App {
    /// What sleep needs to know about the app right now.
    fn sleep_ctx(&self) -> pocket_ui::sleep::Ctx {
        pocket_ui::sleep::Ctx {
            menu_open: self.menu.is_some(),
            busy: self.quit.as_ref().is_some_and(|q| q.busy(Instant::now()))
                || (self.updates.busy.is_some() && update::busy_blocks_sleep(self.updates.busy_since, Instant::now()))
                // A store install or tool (review N5), with the same cap.
                || update::busy_blocks_sleep(self.store.loaded().and_then(|s| s.busy_since()), Instant::now())
                // A starter pack download keeps the Pocket awake the same way (`app_starter`).
                || update::busy_blocks_sleep(self.starter.busy_since(), Instant::now()),
            power_menu: self.sys.power_menu_open(),
        }
    }

    /// The power menu (long press of the power button): Sleep, Restart, Power off, Restart pocket-ui, Restart to boot
    /// menu.
    /// In a game it pauses the game first, the way HOME does (the pause menu opens underneath); B or a
    /// second press of the power button then puts the game back.
    fn open_power_menu(&mut self) {
        if self.sys.power_menu_open() || self.sleep.overlay().is_some() || self.sleep.state() != pocket_ui::sleep::State::Awake {
            return;
        }
        // The power menu would open behind a running app; the power key still puts the device to sleep.
        if self.running.as_ref().is_some_and(Running::is_app) {
            return;
        }
        self.power_paused_game = false;
        if self.running.is_some() && self.menu.is_none() && self.switcher.is_none() {
            self.open_menu();
            self.power_paused_game = self.menu.is_some();
        }
        self.sys.open_power_in(Instant::now(), self.running.is_some());
        pocket_ui::rumble::tick();
        self.play_sound(pocket_ui::ui_sound::Sound::Menu);
        self.request_redraw();
    }

    fn play_sound(&self, sound: pocket_ui::ui_sound::Sound) {
        let ctx = pocket_ui::ui_sound::Ctx {
            config: self.sound,
            in_game: self.running.is_some(),
            pause_menu: self.menu.is_some(),
            asleep: self.sleep.state() != pocket_ui::sleep::State::Awake,
        };
        pocket_ui::ui_sound::play(sound, &ctx);
    }

    /// What the system controls need to know about the screen.
    fn sys_ctx(&self) -> quick::Ctx {
        let game = match &self.running {
            None => quick::Game::None,
            Some(r) if r.hosted() => quick::Game::PocketCore,
            Some(_) => quick::Game::Other,
        };
        let shelf = game == quick::Game::None && self.setup.is_none() && self.settings_page.is_none() && self.menu.is_none() && self.switcher.is_none();
        quick::Ctx { shelf, game }
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The first frame is on screen: what the start put off for it (docs/ARCHITECTURE.md, Start). With a cold page
    /// cache every read before the first frame waits for the card, so only what that frame shows is read before it.
    /// Once; also when no frame came within `AFTER_FIRST_FRAME_AT_LATEST`.
    fn after_first_frame(&mut self) {
        if std::mem::replace(&mut self.after_first_done, true) {
            return;
        }
        // The game list read again (it came from the cache), or the cache written (`app_library`).
        self.library_after_first_frame();
        // The pages' state the first frame doesn't show, each on a thread (`startup::Later`).
        self.store.start();
        self.sync.start();
        // The look for saves with two versions, on a thread that waits a while first (`app_sync_conflicts`).
        self.sync_conflicts_start();
        // Library data merged with the other devices, on a thread that waits a while first (`app_library_sync`).
        self.library_sync_start();
        self.tailscale.start();
        self.web.start();
        self.bluetooth.start();
        self.achievements.start();
        // Is the card nearly full? A statvfs on a thread; a notice on the shelf once (`app_storage`).
        self.storage_check("at start");
        // The games recognised by their checksum, on a thread of the lowest priority that waits a moment first; the
        // covers follow it (`app_identify`).
        self.auto_covers("at start");
        // The letters of the menus, drawn ahead of time on a thread (`Text::prewarm`): a page seen for the first time
        // finds them in the cache. The Japanese letters of the games' names too, from the system's font, if any.
        self.text.prewarm();
        self.library_prewarm_names();
        // Playing on a TV: a thread asleep on sway's output events (`app_display_out`).
        self.display_out_start();
        // The threads and programs the start put off (`startup::after_first_frame`).
        pocket_ui::startup::first_frame_done();
    }
}

/// Told by `about_to_wait` whether a game runs behind a closed menu; the pad reader thins out the sticks then.
static STICKS: pocket_ui::idle::StickThrottle = pocket_ui::idle::StickThrottle::new();

/// The Updates page's message once a system update waits for the next start, with the compatibility note if any.
fn os_staged_message(note: Option<&str>) -> String {
    let base = if update::booted_from_ram() {
        "Ready. It installs the next time the system starts. This device starts from a computer: \
         make a new boot image from the update first."
    } else {
        "Ready. It installs the next time the system starts."
    };
    match note {
        Some(n) => format!("{base} {n}"),
        None => base.to_string(),
    }
}

/// `POCKETUI_TEXT_SIZE=large`, `medium` or `normal`: the text size whatever settings.json says, for the snapshot checks (as
/// `POCKETUI_LANG` is for the language).
fn text_size_override() -> Option<pocket_ui::type_scale::TextSize> {
    match std::env::var("POCKETUI_TEXT_SIZE").as_deref() {
        Ok("large") => Some(pocket_ui::type_scale::TextSize::Large),
        Ok("medium") => Some(pocket_ui::type_scale::TextSize::Medium),
        Ok("normal") => Some(pocket_ui::type_scale::TextSize::Normal),
        _ => None,
    }
}

fn main() {
    use pocket_ui::startup::Later;
    if std::env::args().any(|a| a == "--version") {
        println!("pocket-ui {}", update::app_version());
        return;
    }
    // The OS update check without staging (docs/UPDATES.md): before any window or thread.
    if let Some(i) = std::env::args().position(|a| a == "--check-os-update") {
        std::process::exit(update::check_os_update_cli(std::env::args().nth(i + 1).as_deref()));
    }
    // Safe mode (run-pocket-ui.sh after a crash loop, or POCKETUI_SAFE=1): its own small app, before anything else is
    // read or started (`app_rescue`).
    {
        let args: Vec<String> = std::env::args().collect();
        // `--snapshot <state>:device=rp5`: painted as on that device (`device::snapshot_state`), settled before
        // anything reads the device.
        if let Some(d) = args.iter().position(|a| a == "--snapshot").and_then(|i| args.get(i + 1)).and_then(|s| pocket_ui::device::snapshot_state(s).1) {
            pocket_ui::device::force(d);
        }
        if pocket_ui::rescue::safe_mode_requested(&args, std::env::var(pocket_ui::rescue::SAFE_ENV).ok().as_deref()) {
            let data_dir = PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| "/storage/.config/pocket-ui".into()));
            // Settings → Language (only that field of settings.json is read; a damaged file is the system's).
            pocket_ui::i18n::set(pocket_ui::settings::language(&data_dir));
            // And Settings → Text size, Normal when it can't be read (`POCKETUI_TEXT_SIZE` overrides it, as below).
            pocket_ui::type_scale::set(text_size_override().unwrap_or_else(|| pocket_ui::settings::text_size(&data_dir)));
            if let Some(i) = args.iter().position(|a| a == "--snapshot") {
                return app_rescue::snapshot(data_dir, &args[i + 1..]);
            }
            app_rescue::run(data_dir);
        }
    }
    let started = Instant::now();
    // Speed guard (scripts/latency.sh): process start, off unless POCKETUI_TRACE is set.
    pocket_ui::mark!("ui-start");
    // A freshly installed version is kept once its first frame is on screen. If it hangs before
    // that (this thread runs from the very start), it exits non-zero and the launcher rolls back.
    if !std::env::args().any(|a| a == "--snapshot") {
        std::thread::spawn(move || {
            let root = std::path::Path::new(update::APP_ROOT);
            if update::watch_health(root, started, &update::frame_presented, Duration::from_millis(200), update::WATCHDOG_AFTER) == update::Health::TimedOut {
                eprintln!("update: no frame within {:?}, exiting so that the previous version comes back", update::WATCHDOG_AFTER);
                std::process::exit(update::WATCHDOG_EXIT);
            }
        });
    }
    // The pads and the touch screen first of all, on a thread: a press reaches a device's queue only once the device
    // is open (`pad::Early` has the story). Their events wait in `early_input` until the event loop is there.
    #[cfg(target_os = "linux")]
    let early_input = (!std::env::args().any(|a| a == "--snapshot")).then(|| start_input(started));
    let es_systems = std::env::var("POCKETUI_ES_SYSTEMS").unwrap_or_else(|_| "/storage/.config/emulationstation/es_systems.cfg".into());
    let data_dir = PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| "/storage/.config/pocket-ui".into()));
    // A boot that ended in a freeze or a power cut leaves its report and a marker first (src/logs.rs).
    pocket_ui::logs::begin_boot_now(&data_dir);
    pocket_ui::startup::mark("logs");
    // Release builds abort on a panic; the hook still runs first and leaves crash.log and a marker (see `crash`).
    let crash_notice = pocket_ui::crash::take_marker(&data_dir).map(Notice::new);
    pocket_ui::crash::install("pocket-ui", data_dir.clone(), None);
    pocket_ui::startup::mark("crash");
    // Instant start (docs/DEVELOPING.md, Start time): the type and the game list are read on threads of their own
    // while this one connects to sway and reads the settings; the first frame waits only for the slowest of them.
    let fonts = std::thread::spawn(Text::new);
    // From the cache of the last scan when it is good (`library_cache`; the folders are read again after the first
    // frame, `app_library`), else from the folders as on a first boot.
    let library_load = {
        let (es_systems, data_dir) = (es_systems.clone(), data_dir.clone());
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let loaded = pocket_ui::library_cache::start(&data_dir, std::path::Path::new(&es_systems));
            if let Ok(start) = &loaded {
                let (systems, from) = (start.systems(), if matches!(start, pocket_ui::library_cache::Start::Cached { .. }) { "from the cache" } else { "from the folders" });
                println!("library: {} systems, {} games, read in {:?} ({from})", systems.len(), systems.iter().map(|s| s.games.len()).sum::<usize>(), t0.elapsed());
            }
            loaded
        })
    };
    // The emulator choices in system.cfg (`emu_choice`), on a thread: the first launch finds them in memory.
    pocket_ui::emu_choice::preload();
    pocket_ui::rumble::load_menu_haptics(&data_dir);
    // Renders the UI sounds now, on the mixer thread, not with the first one.
    pocket_ui::ui_sound::start();
    let sound_config = pocket_ui::ui_sound::Config::load(&data_dir);
    pocket_ui::startup::mark("sounds");

    // A measurement aid (`POCKETUI_STARTUP_PROBE=1`): the libraries winit loads, as a step of their own.
    #[cfg(target_os = "linux")]
    if std::env::var_os(pocket_ui::startup::PROBE_ENV).is_some() {
        pocket_ui::startup::preload_display_libs();
        pocket_ui::startup::mark("display libs");
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let proxy = event_loop.create_proxy();
    pocket_ui::startup::mark("event loop");

    // The command FIFO (`mkfifo`, a program) on a thread; the pads and the touch screen were opened at the start.
    #[cfg(target_os = "linux")]
    let sources = {
        let proxy = proxy.clone();
        // Not for a snapshot: it would take the FIFO from the pocket-ui running beside it (`bench-shelf` on a device).
        if !std::env::args().any(|a| a == "--snapshot") {
            // The heap's free pages go back to the system every 20 s (`heap_trim`).
            pocket_ui::heap_trim::spawn();
            std::thread::spawn(move || spawn_command_fifo(proxy));
        }
        Vec::new()
    };
    #[cfg(not(target_os = "linux"))]
    let sources = vec!["keyboard".to_string()];
    pocket_ui::startup::mark("input");

    let mut settings = Settings::load(&data_dir);
    // The Store row is there only when a catalog is set up (`store::catalog_url`; none is public yet).
    app_settings_groups::set_store_offered(pocket_ui::store::catalog_url(settings.store_url.as_deref()).is_some());
    pocket_ui::motion::set_level(settings.motion);
    pocket_ui::motion::set_cozy(settings.animations_or_default() == pocket_ui::settings::Animations::Cozy);
    // The first start since the device started fades in from black after the boot screen (`boot_fade`); the mark is
    // left even with Motion Off, so that turning it on later doesn't fade a restart.
    let boot_fade = !std::env::args().any(|a| a == "--snapshot")
        && pocket_ui::boot_fade::first_since_boot(std::path::Path::new(pocket_ui::boot_fade::MARK))
        && pocket_ui::motion::allowed(pocket_ui::motion::Kind::Fade);
    // Which face button confirms (`button_layout`).
    pocket_ui::button_layout::set(settings.button_layout);
    // From here the input that waited since the start goes to the UI thread, in order, and the rest as it comes.
    // Each button enters as the role the Button layout setting gives it (`button_layout::Remap`), so every screen
    // below sees A as the confirming button: applied here, not when the press came, so a press that waited gets the
    // layout just read. Only the physical pads: touch, the FIFO and safe mode (its own loop) already speak in roles.
    // A stick the game is using is passed on twice a second at most (`STICKS`).
    #[cfg(target_os = "linux")]
    if let Some(early) = &early_input {
        let proxy = proxy.clone();
        let remap = std::sync::Mutex::new(pocket_ui::button_layout::Remap::default());
        early.attach(move |ev| {
            let ev = match ev {
                UserEvent::Pad(e) => {
                    let e = remap.lock().unwrap_or_else(|p| p.into_inner()).event(e);
                    if !STICKS.pass(&e) {
                        return true;
                    }
                    UserEvent::Pad(e)
                }
                ev => ev,
            };
            proxy.send_event(ev).is_ok()
        });
    }
    // Settings → Language, else the system's (`i18n`): before anything is painted.
    pocket_ui::i18n::set(settings.language.unwrap_or_else(pocket_ui::i18n::Lang::system_default));
    // Settings → Look & feel → Text size (`type_scale`): before anything is painted, on the thread that paints.
    // `POCKETUI_TEXT_SIZE=large` overrides it, for the snapshot checks (as `POCKETUI_LANG` the language).
    pocket_ui::type_scale::set(text_size_override().unwrap_or(settings.text_size));
    let hw = std::sync::Arc::new(pocket_ui::system_ctl::AutoHw::detect());
    println!("system controls: mocked {:?}", hw.mocked());
    {
        // `pactl`: a program read from the card; after the first frame (`startup::after_first_frame`).
        let hw = hw.clone();
        pocket_ui::startup::after_first_frame("seed-volume", move || hw.seed_volume());
    }
    pocket_ui::startup::mark("settings+hw");
    // ROCKNIX leaves the GPU overclocked to 925 MHz when nobody chose a speed; heavy PS2 games froze the device.
    #[cfg(target_os = "linux")]
    std::thread::spawn(pocket_ui::system_ctl::cap_gpu_if_unset);
    // GameCube A on the right button and Dolphin's asynchronous shaders, unless the player chose otherwise; and a
    // standalone game's CPU/GPU profile left behind by a crash is undone.
    #[cfg(target_os = "linux")]
    {
        // system.cfg under ROCKNIX's lock: after the first frame, and before any game starts (`launch`).
        pocket_ui::startup::after_first_frame("emu-defaults", emu_settings::apply_start_defaults_once);
        standalone::recover_performance();
        // And a port's (`port_perf`).
        pocket_ui::port_perf::recover();
    }
    // Same for the stick lights: ROCKNIX turns them on at full brightness when nobody chose (`leds.rs`).
    #[cfg(target_os = "linux")]
    {
        let data_dir = data_dir.clone();
        pocket_ui::startup::after_first_frame("leds-first-start", move || pocket_ui::leds::first_start(&data_dir));
    }
    let power_is_mock = !hw.caps.power;
    // What only Settings pages, the Store and the web companion need is read on threads once the first frame is on
    // screen (`startup::Later`, `App::after_first_frame`); a use before that reads it at once, as before.
    let bluetooth_ui = {
        let (hw, service) = (hw.clone(), hw.caps.bluetooth);
        Later::new("bluetooth", move || app_bluetooth::BluetoothUi::load(hw, service))
    };
    let sys = Sys::new(hw, data_dir.clone(), power_is_mock);
    pocket_ui::startup::mark("sys");
    let fallback = pocket_ui::fallback::Fallback::load(&data_dir);
    let store = {
        let (data_dir, url) = (data_dir.clone(), settings.store_url.clone());
        Later::new("store", move || store_page::StorePage::new(&data_dir, url))
    };
    let favorites = pocket_ui::favorites::Favorites::load(&data_dir);
    let library = pocket_ui::library_meta::LibraryMeta::load(&data_dir);
    let sync_ui = {
        let data_dir = data_dir.clone();
        Later::new("sync", move || app_sync::SyncUi::load(&data_dir))
    };
    let leds_config = pocket_ui::leds::Config::load(&data_dir).unwrap_or_default();
    let night_prefs = pocket_ui::night_light::Prefs::load(&data_dir);
    let tailscale_ui = {
        let data_dir = data_dir.clone();
        Later::new("tailscale", move || app_tailscale::TailscaleUi::load(&data_dir))
    };
    let web_ui = {
        let (data_dir, es_systems) = (data_dir.clone(), PathBuf::from(&es_systems));
        Later::new("web", move || app_web::WebUi::load(&data_dir, &es_systems))
    };
    // The web companion's Stats page reads the play history through this (a function pointer stored once; nothing runs now).
    pocket_ui::web::set_history_source(pocket_ui::play_history::stats_json);
    let perf_ui = app_perf_fan::PerfFanUi::load(&data_dir);
    // The ports' own performance and fan, read on a thread (`app_port_perf`).
    let port_perf_ui = app_port_perf::PortPerfUi::new(&data_dir);
    // The catalogue's versions, for the update note on a port's card (read from the cached ports.json, on a thread).
    pocket_ui::ports::load_versions(&data_dir);
    let mut curator_ui = app_curator::CuratorUi::load(&data_dir);
    // Curator's "Pixel Path over every game" became Settings → Playing → Over the game (2026-10-25): moved once.
    pocket_ui::over_game::adopt_curator(&data_dir, &mut settings, &mut curator_ui.prefs);
    // Where the play log lives and whether to record; nothing is read or started before the first frame.
    pocket_ui::play_log::configure(&data_dir, curator_ui.prefs.history);
    pocket_ui::startup::mark("configs");
    let achievements_ui = {
        let data_dir = data_dir.clone();
        Later::new("achievements", move || app_achievements::AchievementsUi::load(&data_dir))
    };
    #[cfg(target_os = "linux")]
    control::games_open_fullscreen_soon();
    pocket_ui::startup::mark("sway rules");
    let sleep = {
        let proxy = proxy.clone();
        pocket_ui::sleep::Sleep::new(data_dir.clone(), std::sync::Arc::new(move || {
            let _ = proxy.send_event(UserEvent::SleepNews);
        }))
    };
    pocket_ui::startup::mark("sleep");
    let text = fonts.join().unwrap_or_default();
    pocket_ui::startup::mark("fonts");
    let (systems, load_error, library_ui) = match library_load.join() {
        Ok(Ok(start)) => {
            let ui = app_library::LibraryUi::from_start(&start, pocket_ui::library_cache::enabled());
            let systems = match start {
                pocket_ui::library_cache::Start::Cached { systems, .. } | pocket_ui::library_cache::Start::Scanned { systems, .. } => systems,
            };
            (systems, None, ui)
        }
        Ok(Err(e)) => (Vec::new(), Some(format!("Cannot read the game list: {e}")), Default::default()),
        Err(_) => (Vec::new(), Some("Cannot read the game list.".to_string()), Default::default()),
    };
    pocket_ui::startup::mark("library");
    let gamedb = pocket_ui::gamedb::GameDb::embedded();
    pocket_ui::startup::mark("gamedb");
    let mut recents = Recents::load(&data_dir);
    // Continue, the recent games and the switcher name the games the way the system list does (from the cached list: no disk).
    recents.follow_library(&systems);
    pocket_ui::startup::mark("recents");
    let mut app = App {
        proxy,
        started,
        window: None,
        surface: pocket_ui::frame::Presenter::empty(),
        text,
        systems,
        load_error,
        library_ui,
        recents,
        screen: Screen::Home,
        items: Vec::new(),
        sel: 0,
        prev_sel: 0,
        moved_at: Instant::now() - PULL,
        home_item: None,
        held: None,
        pad: PadState::new(sources),
        exit_since: None,
        running: None,
        launch_at: None,
        drew_first: false,
        boot_fade: pocket_ui::boot_fade::BootFade::new(boot_fade),
        after_first_done: false,
        theme: settings.theme,
        appearance: settings.appearance,
        fader: pocket_ui::appearance::Fader::at(pocket_ui::appearance::wants_light_now(settings.appearance)),
        appearance_checked: Instant::now(),
        appearance_pending: false,
        last_input: Instant::now(),
        look_previews_light: false,
        look_previews_lang: pocket_ui::i18n::current(),
        look_previews_size: pocket_ui::type_scale::get(),
        column_buf: Vec::new(),
        language: settings.language,
        hidden: settings.hidden_systems.clone(),
        data_dir,
        toast: None,
        sleep_page: pocket_ui::sleep_settings::Page::new(pocket_ui::sleep_settings::Config::default()),
        about: Default::default(),
        after_game_note: None,
        tv: None,
        notice: crash_notice,
        fallback,
        select_alone: false,
        start_tap: StartTap::default(),
        switcher: None,
        switch_to: None,
        fast: Default::default(),
        menu_shot: Default::default(),
        raise: Default::default(),
        frame_at: Instant::now(),
        home_down: false,
        favorites,
        marquee: Default::default(),
        menu: None,
        quit: None,
        reapply: None,
        setup: None,
        settings_page: None,
        manual: pocket_ui::manual::Reader::new(),
        manual_hold: None,
        manual_from_setup: false,
        updates: Updates::default(),
        update_url: settings.update_url.clone(),
        store,
        starter: Default::default(),
        exit_code: 0,
        onboarded: settings.onboarded,
        es_systems: PathBuf::from(&es_systems),
        covers: std::collections::HashMap::new(),
        art: std::collections::HashMap::new(),
        home_tap: false,
        cover_run: Default::default(),
        text_run: Default::default(),
        identify: app_identify::Identify::new(),
        look_previews: None,
        sample_cover: None,
        power_paused_game: false,
        sleep,
        sys,
        hits: Default::default(),
        leave: None,
        touch_rec: Default::default(),
        ignore_log: Default::default(),
        sound: sound_config,
        leds: leds_config,
        night: night_prefs,
        night_asleep: false,
        card: None,
        port_files: None,
        port_perf: port_perf_ui,
        menu_battery: None,
        console: None,
        library,
        gamedb,
        game_infos: Default::default(),
        volume_step: false,
        sync: sync_ui,
        tailscale: tailscale_ui,
        web: web_ui,
        perf: perf_ui,
        tools: Default::default(),
        moonlight: Default::default(),
        backup: Default::default(),
        storage: Default::default(),
        gamelist: Default::default(),
        gallery: Default::default(),
        clock_look: Some(Instant::now() + app_clock::FIRST_LOOK),
        emu: Default::default(),
        this_game: None,
        achievements: achievements_ui,
        bluetooth: bluetooth_ui,
        streaming: pocket_ui::moonlight::installed(),
        streaming_on: settings.streaming_on_home,
        pictures: app_pictures::PicturesUi::new(!settings.box_art_only && !settings.art_off),
        art_off: settings.art_off,
        cabinet: Default::default(),
        // Read before `curator_ui` moves in: the pause menu's Pixel Path switch (`Prefs::pixel_path`).
        pixel_path: curator_ui.prefs.pixel_path,
        curator: curator_ui,
        on_start: settings.on_start,
        remember_quit: !settings.forget_where_quit,
        animations: settings.animations_chosen(),
        game_box: !settings.details_page,
        button_layout: settings.button_layout,
        launch_picture: None,
        find: Default::default(),
        motion: Default::default(),
        frames: settings.frames,
        over_game: settings.over_game,
        tour: app_tour::TourUi::new(settings.tour_later.clone(), settings.seen_version.clone()),
        corner_minute: None,
    };
    // Sleep and Power off in the quick settings go through the sleep module on the UI thread.
    {
        let proxy = std::sync::Mutex::new(app.proxy.clone());
        app.sys.set_sleep_handler(move || {
            let _ = proxy.lock().unwrap().send_event(UserEvent::Sleep);
        });
        let proxy = std::sync::Mutex::new(app.proxy.clone());
        app.sys.set_shutdown_handler(move |action| {
            let _ = proxy.lock().unwrap().send_event(UserEvent::Shutdown(action));
        });
    }
    pocket_ui::startup::mark("app");
    if !app.onboarded {
        app.setup = Some(app.new_setup());
    }
    app.rebuild();
    pocket_ui::startup::mark("rebuild");
    // Home opens on Continue, under the Search row.
    app.sel = Item::first_row(&app.items);
    app.prev_sel = app.sel;
    // A pocket-core that was killed last time may have left the CPU or fan profile changed, and a pocket-ui that
    // crashed its fan outside games; the fan outside games waits for that.
    #[cfg(target_os = "linux")]
    app.restore_system_then_fan();
    pocket_ui::startup::mark("restore");
    if test_driver_dir().is_some() {
        // Keep input, async replies, game exits and menu actions real. Device startup jobs (mounts, services,
        // downloads and rescue markers) belong to an actual device, not a mock-only headless test.
        app.drew_first = true;
        app.after_first_done = true;
        event_loop.run_app(&mut app).expect("run test driver");
        return;
    }
    if let Some(i) = std::env::args().position(|a| a == "--snapshot") {
        let args: Vec<String> = std::env::args().skip(i + 1).collect();
        app_status_corner::snapshot_knobs();
        snapshot(&mut app, &args);
        return;
    }
    // Wi-Fi and the battery for the status corner, read on a thread while the corner is on screen.
    app.status_corner_start();
    // The night light's thread: asleep until the next switch time (`app_night_light`, `night_light.rs`).
    app.night_light_start();
    #[cfg(target_os = "linux")]
    {
        pocket_ui::battery::spawn_sampler(app.data_dir.clone());
        // Temp files of atomic writes whose writer was killed (`fsutil::reap_temp_files`), on a thread, later.
        pocket_ui::fsutil::reap_temp_files_soon(app.data_dir.clone());
        // Powered off from the quick settings, or the battery ran out while asleep: back into that game. Otherwise
        // Settings > On start may continue the last game (`on_start` has the rules).
        app.start_game_at_start(pocket_ui::sleep::take_resume(&app.data_dir));
        // Switched off after sleeping: said once, over that game or on the shelf (`app_auto_off`).
        app.say_switched_off();
        let root = std::path::Path::new(update::APP_ROOT);
        // The launcher handed over to EmulationStation by itself last time (run-pocket-ui.sh, `rescue`).
        if let Some(note) = pocket_ui::rescue::take_es_note(root) {
            if app.notice.is_none() {
                app.notice = Some(Notice::new(note));
            } else {
                app.toast = Some((note, Instant::now()));
            }
        }
        if let Some(version) = update::take_rollback_note(root) {
            // An update that didn't start, or a version that kept crashing (run-pocket-ui.sh, known-good).
            let note = format!("Version {version} didn't start properly, so the last version that worked is back.");
            app.toast = Some((note.clone(), Instant::now()));
            app.updates.message = Some(note);
        }
        // Everything from here to the end of the block starts once the first frame is on screen
        // (`startup::after_first_frame`): with a cold page cache each program and file waits for the card in the same
        // queue as the first frame's reads (docs/ARCHITECTURE.md, Start).
        // START and SELECT + Vol belong to us: ROCKNIX's input_sense gets told to leave them alone (src/fn_keys.rs).
        pocket_ui::startup::after_first_frame("fn-keys", || match pocket_ui::fn_keys::ensure() {
            Ok(written) if !written.is_empty() => println!("function keys: wrote {written:?} and restarted input.service"),
            Ok(_) => {}
            Err(e) => eprintln!("function keys: {e}"),
        });
        // ROCKNIX's own volume box would sit on top of our display: mako hides it while we are in front (src/mako_osd.rs).
        pocket_ui::startup::after_first_frame("mako-osd", || {
            match pocket_ui::mako_osd::ensure() {
                Ok(true) => println!("mako: hiding ROCKNIX's volume box while pocket-ui is in front"),
                Ok(false) => {}
                Err(e) => eprintln!("mako: {e}"),
            }
            pocket_ui::mako_osd::set_hidden(true);
        });
        // Wi-Fi comes back by itself when it drops, and a profile that lost its key is repaired (`wifi::watchdog`).
        let net = app.sys.net();
        pocket_ui::startup::after_first_frame("wifi-watchdog-start", move || pocket_ui::wifi::watchdog(net));
        // ROCKNIX's default Samba configuration shares system folders; only the games may be shared,
        // only with this device's password.
        pocket_ui::startup::after_first_frame("secure-sharing", || {
            if let Err(e) = pocket_ui::wifi::secure_sharing() {
                eprintln!("couldn't restrict file sharing: {e}");
            }
        });
        // Remote access switched on in Settings turns itself off after 30 idle minutes (src/remote.rs).
        pocket_ui::startup::after_first_frame("remote-watch", pocket_ui::remote::watch);
        // Store tools an older version put into ROCKNIX's Tools folder, which ROCKNIX empties at every boot, move to
        // pocket-ui's own folder (`store::migrate_tools_in`; nothing is read when there are none).
        let data_dir = app.data_dir.clone();
        pocket_ui::startup::after_first_frame("migrate-tools", move || pocket_ui::store::migrate_tools_in(&data_dir));
        // Once Wi-Fi is up after boot, games without box art get theirs in the background (src/auto_covers.rs).
        let proxy = app.proxy.clone();
        pocket_ui::startup::after_first_frame("online-wait", move || {
            for _ in 0..36 {
                if pocket_ui::wifi::ip_now().is_some() {
                    let _ = proxy.send_event(UserEvent::Online);
                    return;
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        });
    }
    pocket_ui::startup::mark("threads");
    event_loop.run_app(&mut app).expect("run app");
    // The battery sampler stops after the write it may be in, so no half-written temp file stays (review SOL #12).
    pocket_ui::battery_history::stop();
    // The lists a game's end handed to the card writer are on the card before we go (`fsutil::write_json_soon`).
    pocket_ui::fsutil::settle();
    // And the play log's last frames (at most two seconds).
    pocket_ui::play_log::settle();
    app.perf_fan_exit();
    app.web_exit();
    if app.exit_code == pocket_ui::rescue::OPEN_ES_CODE {
        pocket_ui::rescue::before_es();
    }
    std::process::exit(app.exit_code);
}

#[cfg(test)]
mod start_order_tests {
    /// The first press after a cold start was lost while the pads were opened only after the event loop had been
    /// built (2026-10-26, docs/LATENCY.md "First press after a start"): they come first, the hand-over after the
    /// Button layout is known, and nothing else talks to the proxy for the pads.
    #[test]
    fn the_pads_open_before_anything_else_of_the_start_and_wait_for_the_event_loop() {
        let src = include_str!("main.rs");
        let main = &src[src.find("\nfn main() {").expect("fn main")..];
        let main = &main[..main.find("\n}\n").expect("the end of main")];
        let at = |what: &str| main.find(what).unwrap_or_else(|| panic!("{what} in main"));
        let pads = at("start_input(started)");
        for later in ["begin_boot_now(", "std::thread::spawn(Text::new)", "library_cache::start(", "EventLoop::<UserEvent>::with_user_event()"] {
            assert!(pads < at(later), "the pads are opened before {later}");
        }
        assert!(at("button_layout::set(settings.button_layout)") < at("early.attach("), "the layout is known when the waiting presses go");
        assert!(at("early.attach(") < at("event_loop.run_app("));
        assert!(!main.contains("pad::linux::supervise("), "the pads are opened in start_input only");
    }
}

/// Whether Split's panel shows `item`'s right-hand text (a count: "2 games", "All games") in big letters when it has no
/// picture; else the short name of its system, which a game has. Favorites has no system: without favourites (its
/// covers stand there otherwise) the panel said "favorites", the row's colour key (the first-day run, 2026-10-26).
fn split_big_is_side(item: Item) -> bool {
    matches!(
        item,
        Item::System(_)
            | Item::Streaming
            | Item::Search
            | Item::Favorites
            | Item::Curator
            | Item::Lens
            | Item::Surprise
            | Item::SmartShelf(_)
            | Item::Sticker(_)
            | Item::History
            | Item::Review(_)
    )
}

#[cfg(test)]
mod split_big_tests {
    use super::{split_big_is_side, Item};

    #[test]
    fn only_games_show_their_system_in_splits_panel() {
        assert!(split_big_is_side(Item::Favorites), "Favorites with none starred: \"0 games\", not its colour key");
        assert!(split_big_is_side(Item::System(0)) && split_big_is_side(Item::Search));
        assert!(!split_big_is_side(Item::Recent(0)), "a game shows its system");
    }
}

#[cfg(test)]
mod snapshot_size_tests {
    use super::snapshot_size;

    #[test]
    fn the_snapshot_size_is_the_devices_unless_a_sane_size_is_given() {
        const MINI_V2: (u32, u32) = (1240, 1080);
        assert_eq!(snapshot_size(None, MINI_V2), (1240, 1080));
        assert_eq!(snapshot_size(None, (1920, 1080)), (1920, 1080), "POCKETUI_DEVICE=rp5");
        assert_eq!(snapshot_size(Some("1280x960"), MINI_V2), (1280, 960), "the Retroid Pocket Mini V1");
        assert_eq!(snapshot_size(Some(" 640 X 480 "), MINI_V2), (640, 480));
        for bad in ["", "1280", "1280x", "x960", "big", "0x0", "100000x960", "-1x960"] {
            assert_eq!(snapshot_size(Some(bad), MINI_V2), (1240, 1080), "{bad:?}");
        }
    }
}

#[cfg(test)]
mod repeat_tests {
    use super::{repeat_after, Button, Instant};

    #[test]
    fn the_shelfs_repeat_ends_with_its_release_or_another_press_wherever_they_go() {
        let t = Instant::now();
        let down = Some((Button::Down, t));
        // START opens Settings while Down is held: the repeat ends there, not after Settings closes.
        assert_eq!(repeat_after(down, Button::Start, true), None);
        // The release, which Settings (or any page) keeps to itself, ends it too.
        assert_eq!(repeat_after(down, Button::Down, false), None);
        assert_eq!(repeat_after(down, Button::Right, false), None, "Right repeats as Down on the cards");
        // Its own press again (a repeat from the pad) and other releases leave it.
        assert_eq!(repeat_after(down, Button::Down, true), down);
        assert_eq!(repeat_after(down, Button::Right, true), down);
        assert_eq!(repeat_after(down, Button::Start, false), down);
        assert_eq!(repeat_after(None, Button::Down, true), None);
        let l2 = Some((Button::L2, t));
        assert_eq!(repeat_after(l2, Button::L2, false), None);
        assert_eq!(repeat_after(l2, Button::Up, false), l2);
    }

    /// Two controllers in the menus (review SOL-09): what `user_event` passes on after `PadState::merge`, then the
    /// shelf's repeat as `repeat_after` and `App::press` keep it.
    fn shelf(pad: &mut pocket_ui::pad::PadState, held: &mut Option<(Button, Instant)>, e: pocket_ui::pad::PadEvent) {
        for m in pad.merge(e) {
            if let pocket_ui::pad::PadEvent::Button { button, pressed, .. } = m {
                *held = repeat_after(*held, button, pressed);
                if pressed && matches!(button, Button::Up | Button::Down) {
                    *held = Some((button, Instant::now()));
                }
            }
        }
    }

    #[test]
    fn a_second_controller_does_not_stop_the_first_ones_repeat() {
        use pocket_ui::pad::{Controller, PadEvent, PadState};
        let mut pad = PadState::new(vec![]);
        let mut held = None;
        let b = |source, button, pressed| PadEvent::Button { source, button, pressed };
        let pad2 = Controller { key: "bt:a".into(), name: "DualSense".into(), gamepad: true, node: String::new() };
        shelf(&mut pad, &mut held, PadEvent::Connected { source: 2, controller: pad2 });
        // Player 1 holds Down to scroll; player 2 presses and lets go of Down too.
        shelf(&mut pad, &mut held, b(1, Button::Down, true));
        shelf(&mut pad, &mut held, b(2, Button::Down, true));
        shelf(&mut pad, &mut held, b(2, Button::Down, false));
        assert_eq!(held.map(|h| h.0), Some(Button::Down), "player 1 still holds Down: it keeps repeating");
        // Player 2's controller switches off while holding Down as well.
        shelf(&mut pad, &mut held, b(2, Button::Down, true));
        shelf(&mut pad, &mut held, PadEvent::Disconnected { source: 2 });
        assert_eq!(held.map(|h| h.0), Some(Button::Down));
        // Player 1's own release ends it.
        shelf(&mut pad, &mut held, b(1, Button::Down, false));
        assert_eq!(held, None);
        // Pressing another direction takes over, as with one controller; its release ends that one.
        shelf(&mut pad, &mut held, b(1, Button::Down, true));
        shelf(&mut pad, &mut held, b(3, Button::Up, true));
        assert_eq!(held.map(|h| h.0), Some(Button::Up));
        shelf(&mut pad, &mut held, b(1, Button::Down, false));
        assert_eq!(held.map(|h| h.0), Some(Button::Up), "player 1 letting go of Down leaves player 3's Up");
        shelf(&mut pad, &mut held, b(3, Button::Up, false));
        assert_eq!(held, None);
    }
}

#[cfg(test)]
mod pause_menu_tests {
    use super::{main_items, quit_leaves_out_resume, MainItem};
    use pocket_ui::pause_saves::Saves;
    use pocket_ui::quit::Saver;
    use MainItem::*;

    /// The main page as `QuickMenu::items` builds it for each kind of emulator; `states_known`: a standalone
    /// emulator's state files have names (`standalone::states`).
    fn page(saver: Saver, states_known: bool) -> Vec<MainItem> {
        let hosted = matches!(saver, Saver::PocketCore { .. });
        main_items(hosted, Saves::of(saver, states_known, None), false)
    }

    #[test]
    fn one_quit_for_every_emulator() {
        // pocket-core's Earlier: left out by `QuickMenu::items` when it has no row (`app_earlier`).
        assert_eq!(page(Saver::PocketCore { states: true }, true), [Continue, Save, Load, Earlier, Options, QuickSettings, Quit]);
        assert_eq!(page(Saver::Standalone, true), [Continue, Save, Load, Options, QuickSettings, Quit]);
        assert_eq!(page(Saver::RetroArch, false), [Continue, Save, Load, Options, QuickSettings, Quit]);
        // Options for every emulator: without a page of its own, "Runs on" (`app_emu_choice`).
        assert_eq!(page(Saver::Other, false), [Continue, Options, QuickSettings, Quit]);
    }

    #[test]
    fn save_and_load_are_left_out_where_they_would_do_nothing() {
        // ScummVM, PICO-8, TIC-80 on pocket-core: Save only said "no save states" over the game, Load was greyed out.
        assert_eq!(page(Saver::PocketCore { states: false }, true), [Continue, Options, QuickSettings, Quit]);
        // ARMSX2 before it named the game: Save had no file to write to.
        assert_eq!(page(Saver::Standalone, false), [Continue, Options, QuickSettings, Quit]);
        // Each of them says where saving is instead.
        for saver in [Saver::PocketCore { states: false }, Saver::Other] {
            assert!(Saves::of(saver, true, None).line().is_some(), "{saver:?}");
        }
        assert!(Saves::of(Saver::Standalone, false, None).line().is_some());
    }

    #[test]
    fn remember_where_i_quit_off_leaves_out_the_resume_point_where_the_emulator_can() {
        // On (the start setting): every Quit keeps the moment.
        for saver in [Saver::PocketCore { states: true }, Saver::PocketCore { states: false }, Saver::Standalone, Saver::RetroArch, Saver::Other] {
            assert!(!quit_leaves_out_resume(true, saver), "{saver:?}");
        }
        // Off: pocket-core (`quit nosave`) and PS2/GameCube/Wii/3DS (no save step) leave it out.
        assert!(quit_leaves_out_resume(false, Saver::PocketCore { states: true }));
        assert!(quit_leaves_out_resume(false, Saver::Standalone));
        // RetroArch writes `.state.auto` on every exit; ScummVM and the other emulators have nothing to leave out.
        assert!(!quit_leaves_out_resume(false, Saver::RetroArch));
        assert!(!quit_leaves_out_resume(false, Saver::PocketCore { states: false }));
        assert!(!quit_leaves_out_resume(false, Saver::Other));
    }

    #[test]
    fn achievements_follow_options_for_signed_in_pocket_core_games_only() {
        use MainItem::*;
        let hosted = main_items(true, Saves::Slots, true);
        assert_eq!(hosted.iter().position(|i| *i == Achievements), hosted.iter().position(|i| *i == Options).map(|p| p + 1));
        assert!(!main_items(true, Saves::Slots, false).contains(&Achievements));
        assert!(!main_items(false, Saves::One, true).contains(&Achievements));
        assert!(!main_items(false, Saves::Slots, true).contains(&Achievements));
    }

    #[test]
    fn every_row_has_its_own_label() {
        let all = [Continue, Save, Load, Earlier, Disc, Options, Achievements, QuickSettings, PixelPath, Players, Quit];
        for a in all {
            for b in all {
                assert!(a == b || a.label() != b.label(), "{a:?} and {b:?}");
            }
        }
    }
}
