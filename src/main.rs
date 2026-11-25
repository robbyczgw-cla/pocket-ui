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
use pocket_ui::cozy_extras::{on as extra, Extra};
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
mod app_cover_prefetch;
/// The picture side of Home's Curator, Favorites and pinned rows (`pocket_ui::home_panel`).
mod app_home_panel;
/// The game card (X on a game), play status and notes, offline game facts.
mod app_card;
/// Games recognised by their checksum, and the card's "Wrong pictures? Look up again".
mod app_identify;
/// The console pages and the short facts about a system on the shelf.
mod app_console;
mod app_bigcard_bench;
/// Settings > Sync, and the save scan after a game.
mod app_sync;
mod app_sync_conflicts;
/// Settings > Sync > Library data: the merge with the other devices on a thread (`pocket_ui::library_sync`).
mod app_library_sync;
/// Settings > Look > Appearance: dark, light or automatic, and the crossfade.
mod app_appearance;
/// The Cabinet look: cases on wooden shelves, pulled out and put into the console.
mod app_cabinet;
mod app_crate;
mod app_frozen;
mod app_loading;
mod app_launch;
mod app_where_was_i;
mod app_case_knows;
// The case stickers on the Cabinet's, the Shelf look's and the game box's fronts and spines.
mod app_case_stickers;
mod app_shrink_wrap;
// The CRT TV on the Cabinet's top board with the selected game's screenshot.
mod app_cabinet_tv;
mod app_room_extras;
/// Settings > LEDs, and the game's colour on the sticks (`leds.rs`).
mod app_leds;
/// The Brick's motor, lights and F keys: the App's side of `pocket_ui::brick_hw` (private prototype, `--features brick`).
#[cfg(feature = "brick")]
mod app_brick_hw;
#[cfg(feature = "brick")]
mod app_brick_lights;
/// Settings > Night light, and its steps on the UI thread (`night_light.rs`).
mod app_night_light;
mod app_cozy_extras;
mod app_buttons;
mod app_controllers;
/// Settings > Controls > Test buttons: every controller's buttons, triggers and sticks lighting up (`pocket_ui::button_test`).
mod app_test_buttons;
/// Settings > Tailscale: switch, QR sign-in, status.
mod app_tailscale;
/// Settings → Games → RomM and a RomM game's download at its first start (`app_romm.rs`).
mod app_romm;
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
mod app_pocket_sync;
/// Quick settings' Performance and Fan rows: the game under the panel, a change saved and applied on a thread.
mod app_quick_profiles;
/// PortMaster ports that need the player's own game files: the check before a start and its page.
mod app_ports;
/// A PortMaster port's Performance & fan (START on its card), applied while it runs; the busy-core hint.
mod app_port_perf;
/// Curator (docs/CURATOR.md): the index thread, Home's Curator rows, the Lens, Echoes, Versions, stickers.
mod app_curator;
/// Play history (docs/HISTORY.md): the log's writer and migration, the History and Year in review pages.
mod app_history;
/// Connections and Rediscover (docs/CURATOR.md): a card's facts lead to the player's games with the same one.
mod app_connections;
/// RetroAchievements: Settings > RetroAchievements, the card's and console page's tallies, the pause menu's list.
mod app_achievements;
/// Finding games fast: the search (the Search row on Home) and the letter jump of the lists (L2/R2).
mod app_search;

#[cfg(test)]
static READER_GAME_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
mod app_starter;
/// Settings → Connections → USB file transfer, and a games card coming or going.
mod app_games_card;
/// Pixel Path: the pause menu's graded chain of a game's picture and sound (docs/CURATOR.md).
mod app_pixel_path;
/// The pause menu at once: pocket-core paused before it is covered, its picture after the menu is up.
mod app_menu_open;
mod app_players;
mod app_zoom;
/// The pause menu's Disc row: changing discs of multi-disc games on pocket-ui's own host.
mod app_disc;
mod app_earlier;
mod app_save_media;
/// Game saves that don't reach the card: the pause menu's line and its question before Quit.
mod app_save_trouble;
/// Which emulator plays a game (system.cfg's choices, as EmulationStation keeps them) and the "Runs on" page.
mod app_emu_choice;
/// How a game starts on Android's handheld view and its "Runs on" page (`pocket_ui::play_route`).
mod app_runs_on_android;
/// A game's own cover chosen on its card, Android's handheld view (`covers::custom`).
mod app_cover_pick;
mod app_this_game;
/// Speed guard: the frame line of the latency trace (POCKETUI_TRACE, docs/LATENCY.md).
mod app_trace;
/// Settings > Look > Motion: page transitions, list glides, the hint bar's crossfade, the frame log.
mod app_motion;
mod app_messages;
/// Settings > Games > Game check: "will my games start?", on a thread of its own.
mod app_game_check;
/// Settings > Tools: ROCKNIX's tools and the Store's, started like an app (feat/tools-wifi).
mod app_tools;
/// Settings > PC streaming: pairing Moonlight with the PIN on screen, the paired PCs, their games (feat/tools-wifi).
mod app_moonlight;
/// Settings > Backup & reset: pocket-ui's settings and library notes to the card and back, reset (feat/tools-wifi).
mod app_backup;
/// Settings > Games > From EmulationStation: the player's gamelists, their favorites and ratings (`pocket_ui::gamelist`).
mod app_gamelist;
/// Settings > Games > Android games, and the Android shelf of Android's handheld view (`pocket_ui::android_games`).
mod app_android_games;
/// The screenshot gallery: Settings → Games → Screenshots and the game card's "Screenshots (N)" row (`pocket_ui::gallery`).
mod app_gallery;
mod app_album;
/// Settings > System > Storage, the low-space notice, and Hide or delete on the game card (`pocket_ui::storage`).
mod app_clock;
// The quiet "a newer pocket-ui is ready" notice (`pocket_ui::update_hint`).
mod app_update_hint;
// Where the App is and what the frame's list registered, for the navigation explorer (tests/nav_explorer.rs).
mod app_nav_probe;
// Where the App is: the page in front and what covers it, for `@where` and the status file /tmp/pocket-ui.status.
mod app_where;
use app_where::Painter;
mod app_update_status;
mod app_card_update;
mod app_storage;
// Safe mode (the rescue page) and the dialog before EmulationStation (`pocket_ui::rescue`).
mod app_rescue;
// Settings → Tour and "New in this version": the feature cards outside the first start (`pocket_ui::features`).
mod app_tour;
// Kids Mode (`pocket_ui::kids`, docs/KIDS-MODE.md).
mod app_kids;
// The Big shelf (`pocket_ui::big_shelf`, docs/BIG-SHELF.md).
mod app_big_shelf;
/// Settings in categories: the category list, a category's rows, the way back (`SETTINGS_GROUPS`).
mod app_settings_groups;
/// The status corner at the top right (time, Wi-Fi, battery): its top line, the minute wake, its thread.
mod app_status_corner;
/// The game list from the cache at start, read again after the first frame (`pocket_ui::library_cache`).
mod app_library;
mod app_played;
/// HOME + A in a game pocket-core does not host (the standalone emulators, ports): `grim` on a thread of its own.
mod app_screenshot;
mod app_shortcut_list;
mod app_share_shelf;
/// POCKETUI_FRAME_DUMP: the event loop tick by tick on a fixed 60 fps clock (`pocket_ui::frame_dump`).
mod app_frame_dump;
/// Doc-drift tests: the user guide and the developer docs against the code (docs/dev/tests.md, "Documentation checks").
#[cfg(test)]
mod doc_checks;

/// The covers of a Curator, Favorites or pinned row beside the Shelf look's front (`app_home_panel`): one size
/// whatever the text size, so the picture cache keeps one copy.
const LABEL_ART_H: i32 = 176;
/// How high the covers are in Split's panel.
const SPLIT_PANEL_H: i32 = 260;

/// Split's picture panel (box art, the console over its game count): 420 tall on 1080 lines; a screen down to 960 lines
/// (the Mini V1, every H700 panel's 1280x960) gives up the missing lines, so the name and the facts under it keep their
/// room (docs/DEVICES.md). Under 960 lines (the Brick's 1024x768, the Smart Pro's 1280x720, painted natively) it keeps
/// 960's share of the height, as on the RG35XX Plus: giving up 312 of 420 lines left the Brick a 108-line strip, a 60-pixel
/// cover and no room for the console at all.
fn split_panel_h(h: i32) -> i32 {
    if h >= 960 { 420 - (1080 - h).max(0) } else { 300 * h / 960 }
}

thread_local! {
    /// Where the last Split frame put its panel and the console over the game count (x, y, w, h; the console's w 0
    /// when none was painted) on a canvas of w x h: what a `shelf:` snapshot prints for tests/split_panel.rs.
    static SPLIT_SEEN: std::cell::Cell<[i32; 10]> = const { std::cell::Cell::new([0; 10]) };
}

const PULL: Duration = Duration::from_millis(120);
const FADE: Duration = pocket_ui::insert_scene::FADE;
/// What the start puts off until the first frame (`App::after_first_frame`) runs by this time even without one.
const AFTER_FIRST_FRAME_AT_LATEST: Duration = Duration::from_secs(2);
/// Frames go on this long after a fade, so that its last one is drawn at the end (the launch picture at full strength,
/// not left at 95 % over the shelf).
const LAST_FADE_FRAME: Duration = Duration::from_millis(50);
/// A Quit's wait shows a spinner from here on, and from `SAVE_WAIT_SLOW` says it goes on (`paint_saving_wait`).
const SAVE_WAIT_SHOWN: Duration = Duration::from_secs(1);
const SAVE_WAIT_SLOW: Duration = Duration::from_secs(4);
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

/// How long a toast stays: by its length, so a sentence is read to the end (docs/DESIGN.md, "Messages"). A short
/// confirmation ("Added to Favourites", 20 letters) is 2 s, a line of 60 letters 3.4 s, never more than 6 s. What went
/// wrong is not a toast but a notice, which stays until it is closed (`notice`).
pub(crate) fn toast_len(text: &str) -> Duration {
    Duration::from_millis((1000 + 40 * text.chars().count() as u64).clamp(2000, 6000))
}

/// Set by `--snapshot`: a snapshot is a settled picture, so a toast in it has a fixed age.
static SNAPSHOT_RUN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// This run paints `--snapshot`s.
pub(crate) fn snapshot_run() -> bool {
    SNAPSHOT_RUN.load(std::sync::atomic::Ordering::Relaxed)
}

/// This run is the headless test driver's (`--test-driver`).
pub(crate) fn driven() -> bool {
    #[cfg(target_os = "linux")]
    {
        test_driver_dir().is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// How old a toast that appeared at `at` is when it is painted. In a snapshot always the same, settled in and not yet
/// fading out: on a loaded machine the wall clock between putting it up and painting it can pass `TOAST`, and the
/// picture (and the text a test reads from it) lost the toast.
pub(crate) fn toast_age(at: Instant) -> Duration {
    if SNAPSHOT_RUN.load(std::sync::atomic::Ordering::Relaxed) {
        Duration::from_millis(300)
    } else {
        at.elapsed()
    }
}
/// Animations draw at most this often (60 Hz).
const FRAME: Duration = Duration::from_micros(16_667);

/// The Retroid prints A on the right face button and B on the bottom one.
const CONFIRM: Button = Button::East;
const CANCEL: Button = Button::South;

#[derive(Debug)]
enum UserEvent {
    /// Every few seconds while Kids Mode is on: the day's play time (`app_kids`).
    KidsTick,
    /// The Brick's screenshot of pocket-ui itself is in place (None) or why it is not (`app_screenshot`).
    #[cfg(feature = "brick")]
    UiScreenshot(Option<&'static str>),
    /// The Brick wrote the Runs on choice to system.cfg, or why not (`app_emu_choice`).
    #[cfg(feature = "brick")]
    EmuChoiceWritten(Result<(), String>),
    Pad(PadEvent),
    /// The finger on the touch screen (`pocket_ui::touch`); pocket-core reads it itself while a game runs.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Touch(touch::TouchEvent),
    /// `ok`: the game's process ended with exit code 0. `pgid`: its process group, which tells a game that has
    /// handed over already (`app_fast_switch`) from the one running now.
    GameExited { secs: u64, ok: bool, pgid: Option<i32>, start: pocket_ui::fallback::StartOutcome, moments: bool },
    /// pocket-core `pgid` has written its exit save and may be replaced (`pocket_ui::fast_switch`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    GameSaved { pgid: i32 },
    /// pocket-core's picture `seq` for behind the pause menu, read (`app_menu_open`).
    MenuShot { seq: u32, image: Option<pocket_ui::preview::Shot> },
    /// The picture of where the game was left, read and scaled for the launch (`pocket_ui::launch_picture`).
    LaunchPicture(PathBuf, Option<pocket_ui::launch_picture::Prepared>),
    /// The background scan of saves and states belonging to games.
    PlayedFiles(u64, std::collections::HashSet<PathBuf>),
    /// The generation of the read (`frozen_refresh`), what stands open.
    Frozen(u64, Vec<pocket_ui::frozen::Moment>),
    /// Every game with a usable moment, for Easy (`app_big_shelf`).
    BigMoments(u64, Vec<pocket_ui::frozen::Moment>),
    /// Applying PS2, GameCube, Wii or 3DS options by a restart: the resume state was written (or not).
    OptionsSaved(bool),
    /// A standalone load left the game unchanged because Before load could not be kept.
    LoadBlocked(u64),
    /// The result of checking a ROCKNIX RetroArch pause menu command.
    RetroArchStateChecked { started: Instant, action: pocket_ui::ra_state::Action, confirmed: bool },
    Setup(SetupEvent),
    Update(UpdateEvent),
    /// The starter pack's downloads and scans (`app_starter`).
    Starter(app_starter::Event),
    /// The automatic cover download (`pocket_ui::auto_covers`).
    Covers(CoversEvent),
    /// The installed ports metadata worker published another complete snapshot.
    PortIndex,
    /// The game card's descriptions loaded ahead (`pocket_ui::auto_texts`, `app_card`).
    Texts(pocket_ui::auto_texts::Event),
    /// Games recognised by their checksum, and "Look up again" (`app_identify`).
    Identify(app_identify::IdentifyEvent),
    /// A screenshot or title screen decoded by the loader thread (`app_pictures`).
    Picture(pocket_ui::picture_loader::Key, Option<Image>),
    /// Where the player is in a game, read from its battery save (`app_case_knows`): the ROM and the answer.
    CaseKnows(PathBuf, Option<pocket_ui::case_knows::Progress>, Option<std::sync::Arc<pocket_ui::case_knows_art::Art>>),
    /// Wi-Fi came up after boot (or was up already): the automatic covers may start.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Online,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Quit,
    /// A mock-only command FIFO capture (`--test-driver <temporary directory>`); `false`: the navigation explorer's record
    /// only, no picture (`app_nav_probe`).
    TestFrame(bool),
    /// Opens a real page against the test card, without synthetic snapshot rows.
    TestPage(String),
    /// Mock-only presentation for launch progress; never publishes a capture or consumes its feedback.
    TestPresent,
    /// Mock-only: press and release a key, then paint its immediate result in the
    /// same event, before a fast worker's reply can overtake a separate capture.
    TestPressFrame(Button),
    /// The command FIFO's `leave`: the dialog SELECT + START held opens (`app_rescue`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Leave,
    /// The command FIFO's `wake`: the screen on from light sleep, without a button a page could take (`app_where`,
    /// docs/DEVICE-TESTS.md).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Wake,
    /// The command FIFO's `dark`: Screen off without a power-key press or kernel suspend.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    ScreenOff,
    /// FIFO `play <system> <file>`: starts a game the way the shelf does (the Brick smoke test, `scripts/brick-smoke.sh`). Only files below a
    /// `.pocket-ui-test` folder; anything else is ignored.
    #[cfg(feature = "brick")]
    Play(String, PathBuf),
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
    /// dArkOS menu row: save a running game, then return directly to EmulationStation.
    BackToEs,
    /// The game card's description from Wikipedia arrived, or could not be loaded (`app_card`).
    CardText { title: String, text: Result<String, String> },
    /// Android's photo picker answered "Choose cover…" (`app_cover_pick`): picked, cancelled, too-big, no-picker, failed.
    CoverPicked(String),
    /// The chosen picture, checked and scaled on a thread (`covers::custom::from_picture`), for a game (system, file).
    CoverReady { system: String, rom: PathBuf, png: Result<Vec<u8>, pocket_ui::covers::custom::Refused> },
    /// Syncthing: news for Settings > Sync, and the result of the scan after a game (`app_sync.rs`).
    Sync(pocket_ui::sync_page::Event),
    /// Tailscale: news for Settings > Tailscale (`app_tailscale.rs`).
    Tailscale(pocket_ui::tailscale_page::Event),
    /// RomM: pairing, the list, covers (`app_romm.rs`).
    Romm(app_romm::RommNews),
    /// Bluetooth: news for Settings > Bluetooth (`app_bluetooth.rs`).
    Bluetooth(pocket_ui::bluetooth_page::Event),
    /// Settings > Performance & fan's readout, and the end of a clean-up the fan outside games waited for (`app_perf_fan.rs`).
    PerfFan(pocket_ui::perf_fan_page::Event),
    /// A step of the night light's fade: the white point to show, 0 = off (`app_night_light.rs`).
    Night(u16),
    /// The web companion: an edit from a browser, news for its Settings page (`app_web.rs`).
    Web(app_web::Event),
    PocketSync(app_pocket_sync::Event),
    /// Settings > Tools: the tools were found, a Store tool ran (`app_tools.rs`).
    Tools(pocket_ui::tools_page::Event),
    /// Settings > Games > Game check: how far it is, what it found (`app_game_check.rs`).
    /// The generation of the check that sent it (`game_check_start`).
    GameCheck(u64, pocket_ui::game_check_page::Event),
    /// Settings > PC streaming: pairing, the PCs, their games (`app_moonlight.rs`).
    Moonlight(pocket_ui::moonlight_page::Event),
    /// Settings > Backup & reset: the backups were listed, a job finished (`app_backup.rs`).
    Backup(pocket_ui::backup_page::Event),
    /// EmulationStation's gamelists were read and differ from what the App has (`app_gamelist.rs`).
    Gamelist(std::sync::Arc<pocket_ui::gamelist::Index>),
    GamelistDeferred,
    /// Android listed its apps (`app_android_games.rs`), read for this reason.
    AndroidApps(Vec<pocket_ui::android_games::App>, &'static str),
    /// The screenshots folder was read, or a screenshot was copied to the shelf (`app_gallery.rs`).
    Gallery(app_gallery::Event),
    /// A photo of the open case's album, decoded (`app_album`).
    AlbumPicture(PathBuf, Option<pocket_ui::image::Image>),
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
    ControllerSaved(String),
    /// A pad event already merged over every controller (`App::merge_pad`), passed on as `Pad`.
    Merged(PadEvent),
    /// The status corner's thread read a change: Wi-Fi connected or gone, the battery (`app_status_corner`).
    StatusCorner,
    LaunchPrepared,
    /// The values of Settings rows that need a process or a file came in (`app_settings_groups`, `settings_values`).
    SettingsValues,
    /// A games card came or went (`pocket_ui::games_card`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    GamesCard(pocket_ui::games_card::Change),
    /// The folders were read again and the list differs from the one on screen (`app_library`); the cache holds it
    /// for the key.
    LibraryRescanned(Vec<System>, Option<pocket_ui::library_cache::Key>),
    LibraryCacheStored(pocket_ui::library_cache::Key),
    LibraryScanDeferred,
    /// How many things in the ROM folders the shelf leaves unread, counted after a rescan; `rose` when that is news
    /// and no game arrived (`app_library`).
    LibraryUnread { count: usize, rose: bool },
    /// A damaged system entry was skipped, or (`fallback`) the whole list couldn't be read and the shelf runs on the built-in
    /// one; the other games remain on the shelf.
    SystemListUnread { fallback: bool },
    SystemListCheckDeferred,
    /// The picture went to a TV through USB-C, or back to the panel (`app_display_out`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Display(Option<pocket_ui::display_out::Tv>),
}

#[derive(Debug)]
enum CoversEvent {
    Progress { done: usize, total: usize, found: usize },
    /// `stopped`: a game started and cut the run short. `pictures_looked_up`: the games whose screenshots and title
    /// screens it got to (`pocket_ui::game_pictures`).
    Done { looked_up: Vec<PathBuf>, pictures_looked_up: Vec<PathBuf>, found: usize, total: usize, stopped: bool, unreachable: bool },
}

/// What a row of the Settings list opens.
#[derive(Clone, Copy)]
enum Opens {
    Setup(Only),
    /// dArkOS only: the dialog "Back to EmulationStation?" (`darkos::BACK_ROW`).
    BackToEs,
    /// Android only: the handheld view (these pages) or the Mobile view (`android/handheld/app_android.rs`).
    View,
    /// Warmer or cooler screen colour, as SELECT + Vol (`Sys::set_warmth`).
    ScreenColour,
    /// A level, not a page: the quick settings' Brightness, Left/Right one step (`Sys::step_level`).
    Brightness,
    /// A level, not a page: the quick settings' Volume, Left/Right one step (`Sys::step_level`).
    Volume,
    // Starter pack is opened from Extras, Home or Kids Mode, without a separate Games settings row.
    /// "Will my games start?" (`app_game_check`).
    GameCheck,
    /// Every controller's own button layout (`app_controllers.rs`).
    Controllers,
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
    /// A switch, not a page: "Where was I?" after a long break (`App::remind_where`, `pocket_ui::where_was_i`).
    RemindWhere,
    /// A switch, not a page: Zoom on the pause menu's main page (`App::zoom_in_pause`, `app_zoom`).
    ZoomInPause,
    /// A switch, not a page: the Shelf look's rows lived-in (`App::lived_in`, `pocket_ui::lived_in`).
    LivedIn,
    /// A value, not a page: Off, 10, 20 or 50 games on the shelf, the rest in the crate (`pocket_ui::shelf_space`, `app_crate`).
    ShelfSpace,
    /// A switch, not a page: a fresh start runs the console's own boot screen (`App::boot_screen`, `pocket_ui::boot_screen`).
    BootScreen,
    /// Settings → Screen & sound → Photos of big moments (`app_album`): Off, On (into the case) and, where pocket-core draws the game,
    /// Also in play (`album_eject` over the running game); Off by default.
    MomentPhotos,
    /// Kids Mode's setup (`app_kids`).
    Kids,
    /// A switch, not a page: Cozy or Quick animations (`App::animations`).
    Animations,
    /// Button layout and Button symbols on one page (`app_buttons.rs`).
    Buttons,
    /// A switch, not a page: Auto frames on the handhelds or on every console (`settings::Frames`).
    Frames,
    /// Over the game: Off, FPS, FPS + sensors, Pixel Path; Left/Right step, A goes on (`over_game`).
    OverGame,
    /// A switch, not a page: the interface's language (`pocket_ui::i18n`), used from the next frame on.
    Language,
    /// A switch, not a page: Normal or Large type (`pocket_ui::type_scale`), used from the next frame on.
    TextSize,
    /// Help & tools › First-start setup: the setup of the first start again, from the welcome.
    FirstStart,
    /// The feature cards (`app_tour.rs`).
    Tour,
    /// A switch (A) and a level (left/right): the UI sounds.
    Sounds,
    /// Syncthing: saves and states kept in step with other devices (`app_sync.rs`).
    Sync,
    /// Pocket-to-Pocket sync: pairing with a code, the paired Pockets (`app_pocket_sync.rs`).
    Pockets,
    /// The lights around the sticks (`app_leds.rs`).
    Leds,
    /// Warmer colours in the evening (`app_night_light.rs`).
    NightLight,
    /// The little touches, each switched on its own (`app_cozy_extras.rs`).
    CozyExtras,
    /// A switch, not a page: a cozy extra whose home is this row (`cozy_extras::Extra::home`), On or Off.
    Extra(Extra),
    /// Tailscale: the private network to the Pocket's other devices (`app_tailscale.rs`).
    Tailscale,
    /// RomM: the games of the player's own game server on the shelf (`app_romm.rs`).
    Romm,
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
    /// Every controller's buttons, triggers and sticks lighting up while pressed (`app_test_buttons.rs`).
    TestButtons,
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
    /// The apps Android lists, to put on the Android shelf or take off (`app_android_games.rs`; Android only).
    AndroidGames,
    /// Screen off after, Sleep after, Switch off after sleeping, the power button (`app_sleep_power.rs`).
    SleepPower,
    /// A switch, not a page: the colours of every page (`palette::Colours`); the whole screen shows it at once.
    Colours,
    /// A switch, not a page: Barlow or Pixel (`font_style`).
    Font,
    /// A switch, not a page: Dark, Light or Automatic (`settings::Appearance`); Y on the Look page too.
    Appearance,
    /// A switch, not a page: Full, Reduced or Off (`settings::Motion`); R1 on the Look page too.
    Motion,
    /// Which systems are on Home (`setup`'s Systems on the shelf); X on the Look page too.
    ShelfSystems,
}

/// Under Settings → Battery: where the times of sleep went (`app_sleep_power`, 2026-10-26).
const SLEEP_SETTINGS_POINTER: &str = "When the screen goes off and the device sleeps: Settings \u{203a} System \u{203a} Sleep & power.";

/// The highest an empty page's picture (`ui_art::above_words`) goes: under the top line and its status corner.
const EMPTY_ART_TOP: i32 = 110;


/// A row of Settings: its name, what it opens, the line explaining it.
type SettingsRow = (&'static str, Opens, &'static str);

/// Settings in categories (the flat list of 27 rows was too long, Alex 2026-10-23; short ones since 2026-10-30, eight
/// since Settings v2 of 2026-11-16 split Controls & sound into Screen & sound and Controls,
/// at most 9 rows a list, every choice a row of its own, no choice only on a button; the tree of 2026-11-13,
/// SETTINGS-IA-DECISION: Connections first, no page that only gathers more settings): a category's name and its rows
/// in their order. START opens the first category's list, L1/R1 go to the others (`app_settings_groups.rs`, the row of
/// symbols at the top). A new row is one line in its category; `SETTINGS` follows by itself.
const SETTINGS_GROUPS: [(&str, &[SettingsRow]); 8] = [
    // The most used first: START lands here (Wi-Fi, Bluetooth), and every network service is one row of it.
    ("Connections", &[
        ("Wi-Fi", Opens::Setup(Only::Wifi), "Connect to a network."),
        ("Bluetooth", Opens::Bluetooth, "Pair a controller or headphones, see what is connected."),
        ("Tailscale", Opens::Tailscale, "Access this Pocket from other devices."),
        ("Remote access (SSH)", Opens::Setup(Only::Remote), "Log in from a computer. Turns itself off when the Pocket restarts."),
        ("Sync", Opens::Sync, "Keep saves and states in step with your other devices."),
        (app_pocket_sync::SETTINGS_NAME, Opens::Pockets, "Keep saves the same on your other Pockets, without a computer."),
        (app_web::SETTINGS_NAME, Opens::Web, "Browse games, stats and uploads in any browser."),
        (app_moonlight::SETTINGS_NAME, Opens::PcStreaming, "Pair a gaming PC for Moonlight, and stream its games."),
    ]),
    // "Look" with a zero-width mark: the same word as its first row in English, a different one in German (Aussehen,
    // the row Ansicht); the German table looks strings up whole (`i18n`), and the mark is painted as nothing.
    // How pocket-ui looks: the layout first, then the style rows, Text size, what moves (Motion, Game start animation)
    // and the extras you see (Settings v2, 2026-11-16; Zoom in the pause menu went to Playing in 0.5.1).
    (LOOK_CATEGORY, &[
        ("Layout", Opens::Setup(Only::Look), "How your games stand: Shelf, Easy, Split or Cabinet, with a live preview."),
        ("Appearance", Opens::Appearance, "Dark, Light, or Automatic by the time of day."),
        ("Colours", Opens::Colours, "The colours of every page. Left/right: the next set."),
        ("Style", Opens::Font, "Barlow or Pixel type, or Retro: pixel letters, frames and icons everywhere."),
        ("Text size", Opens::TextSize, "Larger text on every screen, fewer rows on a page."),
        ("Motion", Opens::Motion, "Full: selections glide. Reduced: only fades. Off: nothing moves."),
        (GAME_START_ROW, Opens::Animations, "Cozy: watch your game slide into the console. Quick: straight into the game."),
        (app_cozy_extras::SETTINGS_NAME, Opens::CozyExtras, "The cabinet's TV, stickers, playful cases, old-TV switch-on and more: each on or off."),
        // 0.5.2: last, so the rows above keep their places.
        (LIVED_IN_ROW, Opens::LivedIn, LIVED_IN_LINE),
    ]),
    // What the device puts out: the two levels of the quick settings (HOME) as rows of their own, the screen's colour, and
    // what you hear besides the game (Settings v2: "Sounds 70%" looked like the volume, and the volume was nowhere here).
    (SCREEN_SOUND_CATEGORY, &[
        ("Brightness", Opens::Brightness, "How bright the screen is. The same as in the quick settings."),
        ("Volume", Opens::Volume, "How loud games and menus are. The same as in the quick settings."),
        ("Screen colour", Opens::ScreenColour, "Cooler or warmer white, the same as SELECT + Vol. Left/right: change. A: Neutral."),
        (app_night_light::SETTINGS_NAME, Opens::NightLight, "Warmer, less blue colours in the evening."),
        (MENU_SOUNDS_ROW, Opens::Sounds, "Soft clicks in menus, not in games. Left/right: how loud."),
        (Extra::CaseSounds.label(), Opens::Extra(Extra::CaseSounds), Extra::CaseSounds.blurb()),
        // 0.6: the case's photos beside its sounds (Games had nine rows with Shelf space).
        ("Photos of big moments", Opens::MomentPhotos, "Off. On, a photo when your save shows a new badge, a boss beaten or a new level, kept in the game's case."),
    ]),
    // The buttons in one place, and what you feel and see on the controller.
    (CONTROLS_CATEGORY, &[
        (app_buttons::SETTINGS_NAME, Opens::Buttons, "Which button confirms, and how the buttons look in the hints."),
        (app_buttons::CONTROLLERS_NAME, Opens::Controllers, "A separate layout for each controller, in menus and games. Pairing: Connections › Bluetooth."),
        (app_test_buttons::SETTINGS_NAME, Opens::TestButtons, "Press buttons, triggers and sticks of each controller and watch them light up."),
        ("Haptics", Opens::Haptics, "A soft tick as you move."),
        ("LEDs", Opens::Leds, "The lights around the sticks. Off unless you turn them on."),
    ]),
    ("Playing", &[
        ("On start", Opens::OnStart, "Home, or straight back into the game you played last."),
        ("Continue where I quit", Opens::RememberQuit, "Quit keeps the moment for the next start."),
        ("Console boot screen", Opens::BootScreen, "A fresh start shows the console's own boot screen. Needs your BIOS file."),
        ("Remind me where I was", Opens::RemindWhere, "Back after two weeks or more: when you played last, and your note, while the game loads."),
        ("Screen frames", Opens::Frames, "Auto frames on the handhelds, or on every console."),
        ("Over the game", Opens::OverGame, "The frame rate, or the sensors too, over every game."),
        // In Playing since 0.5.1: it acts over a game, and it stood too high in Look, under Text size (a hand walk).
        ("Zoom in the pause menu", Opens::ZoomInPause, "Zoom on the pause menu's first page. It is always under Options."),
        (app_perf_fan::SETTINGS_NAME, Opens::PerfFan, "Processor speed and fan, in games and in menus."),
        (app_achievements::SETTINGS_NAME, Opens::Achievements, "Earn achievements in your games."),
    ]),
    ("Games", &[
        ("Add games", Opens::Setup(Only::Games), "Where each system's games and BIOS files go, and file sharing."),
        (EXTRAS_ROW, Opens::Setup(Only::Extras), "Box art, PortMaster games and starter pack."),
        (app_game_check::SETTINGS_NAME, Opens::GameCheck, "Will your games start? What's missing, in plain words."),
        ("Systems on the shelf", Opens::ShelfSystems, "Which systems are on Home. Their games stay either way."),
        // 0.5.2: beside Systems on the shelf, the other choice of what stands on the shelf (Look had nine rows).
        (app_crate::SHELF_SPACE_ROW, Opens::ShelfSpace, app_crate::SHELF_SPACE_LINE),
        (app_curator::SETTINGS_NAME, Opens::Curator, "Lens, smart shelves, Echoes, versions and play history."),
        (app_gallery::SETTINGS_NAME, Opens::Screenshots, "The pictures you took with HOME + A, newest first."),
        (app_gamelist::SETTINGS_NAME, Opens::Gamelist, "Favourites and ratings from your EmulationStation gamelists."),
        (app_android_games::SETTINGS_NAME, Opens::AndroidGames, "Put the Android games you have on a shelf of their own, beside your consoles."),
        (app_romm::SETTINGS_NAME, Opens::Romm, "The games on your own RomM server, on the shelf. Each downloads the first time you start it."),
    ]),
    ("System", &[
        (pocket_ui::rescue::BACK_ROW, Opens::BackToEs, pocket_ui::rescue::BACK_ROW_HELP),
        ("Updates", Opens::Updates, "New versions of pocket-ui."),
        ("Language", Opens::Language, "The language of menus and messages."),
        ("View", Opens::View, "Handheld: these pages, for the built-in pad. Mobile: touch pages and a folder you pick."),
        (app_sleep_power::SETTINGS_NAME, Opens::SleepPower, "When the screen goes off and the device sleeps, and the power button."),
        ("Battery", Opens::Battery, "Battery history, time left, what each game uses."),
        ("Time", Opens::Time, "Time zone, date and network time."),
        (app_storage::SETTINGS_NAME, Opens::Storage, "What takes up space on the SD card, and what can go."),
        (app_backup::SETTINGS_NAME, Opens::Backup, "Save your saves, states and pocket-ui's settings to the SD card, restore them, or start afresh."),
        ("Kids mode", Opens::Kids, "A big, simple shelf for a child, with a PIN."),
    ]),
    ("Help & tools", &[
        ("Manual", Opens::Manual, "The user guide, on the device."),
        ("Tour", Opens::Tour, "The basics in 7 cards: playing, saving, quick settings, adding games, easier reading."),
        ("How you like to play", Opens::Setup(Only::Play), "The shelf, Easy, Enthusiast or your own choice of the extras."),
        ("First-start setup", Opens::FirstStart, "Language, Wi-Fi, your games and the look again, step by step. Your games and saves stay."),
        (app_tools::SETTINGS_NAME, Opens::Tools, "Emulator menus, RetroArch, gamepad test and calibration, terminal."),
        (pocket_ui::report::ROW, Opens::Report, "Saves a file for a bug report, to attach on GitHub or send by mail."),
        ("About", Opens::About, "Version, storage and network."),
    ]),
];

/// Controls' name: the buttons, the controllers and what you feel (it was "Controls & sound" from the tree of 2026-11-13
/// until Settings v2 gave the sounds to Screen & sound, and "Sound & feel" before).
const CONTROLS_CATEGORY: &str = "Controls";

/// Screen & sound's name: Brightness, Volume, the screen's colour and the menus' sounds (Settings v2, 2026-11-16).
const SCREEN_SOUND_CATEGORY: &str = "Screen & sound";

/// The menus' clicks (it was "Sounds", which looked like the volume).
const MENU_SOUNDS_ROW: &str = "Menu sounds";

/// Cozy or Quick as a game starts (it was "Animations" in Playing, beside Look's Motion, which sounded the same).
const GAME_START_ROW: &str = "Game start animation";

/// The Shelf look's rows as a real shelf: some cases lean, a few lie on a stack (`pocket_ui::lived_in`).
const LIVED_IN_ROW: &str = "Lived-in shelf";

/// The line under Lived-in shelf.
const LIVED_IN_LINE: &str = "Shelf layout: some cases lean, a few lie on a small stack, like a shelf at home.";

/// Games' row of box art, PortMaster games, cheats and the starter pack (it was called "Extras"), as its page's title.
const EXTRAS_ROW: &str = pocket_ui::extras_page::TITLE;

/// The Look category's name: "Look" as painted, a key of its own for the German table.
const LOOK_CATEGORY: &str = "Look\u{200b}";

/// Every row of Settings, category after category: `SettingsPage::List` and `settings_row` count in this order.
const SETTINGS: [SettingsRow; app_settings_groups::count()] = app_settings_groups::flat();

#[derive(Debug)]
enum UpdateEvent {
    Checked(Result<Manifest, String>),
    CheckingCard,
    Progress(u64),
    Worker { job: u64, snapshot: update::WorkerSnapshot },
    Inspected(update::observation::Inspection),
    /// An install finished: `Ok(true)` for the app (restart next), `Ok(false)` for the system.
    Done(Result<bool, String>),
    /// The system update was staged; the note says what the new system lacks that pocket-ui uses (`os_compat`).
    OsStaged(Option<String>),
    /// The quiet look from the shelf for a newer pocket-ui (`app_update_hint`).
    Looked { job_at_start: u64, result: Result<Manifest, String> },
    /// What the card holds (`app_card_update`): at the start, or from "Look on the card".
    CardLooked { at_start: bool, look: update::card::Look },
}

use pocket_ui::update_page::Action as UpdateAction;

#[derive(Default)]
enum UpdatePhase {
    #[default]
    Idle,
    Confirming,
    Running(UpdateJob),
    Finished(UpdateReport),
}

const LOOKING_ON_CARD: &str = "Looking on the card";

enum UpdateTask {
    Check,
    CheckCard,
    App { version: String, total: u64, cancel: update::Cancel },
    System { version: String, total: u64, cancel: update::Cancel },
}

#[derive(Clone)]
enum UpdateTarget { Check, App(String), System(String) }

impl UpdateTarget {
    fn kind(&self) -> &'static str { match self { Self::Check => "check", Self::App(_) => "app", Self::System(_) => "system" } }
    fn version(&self) -> Option<&str> { match self { Self::Check => None, Self::App(v) | Self::System(v) => Some(v) } }
}

impl UpdateTask {
    fn cancel(&self) -> Option<&update::Cancel> {
        match self { Self::Check | Self::CheckCard => None, Self::App { cancel, .. } | Self::System { cancel, .. } => Some(cancel) }
    }
    fn target(&self) -> UpdateTarget {
        match self { Self::Check | Self::CheckCard => UpdateTarget::Check, Self::App { version, .. } => UpdateTarget::App(version.clone()), Self::System { version, .. } => UpdateTarget::System(version.clone()) }
    }
    fn label(&self) -> String {
        match self { Self::Check => "Checking for updates".into(), Self::CheckCard => LOOKING_ON_CARD.into(), Self::App { version, .. } => format!("Installing pocket-ui {version}"), Self::System { version, .. } => format!("Getting system {version}") }
    }
    fn total(&self) -> u64 {
        match self { Self::Check | Self::CheckCard => 0, Self::App { total, .. } | Self::System { total, .. } => *total }
    }
}

struct UpdateJob {
    _card: pocket_ui::background_io::UpdatePause,
    id: u64,
    task: UpdateTask,
    label: String,
    since: Instant,
    worker: update::WorkerSnapshot,
}

#[derive(Clone, Copy)]
enum UpdateOutcome { Checked, Installed, OsStaged, Failed }

impl UpdateOutcome {
    fn code(self) -> &'static str { match self { Self::Checked => "checked", Self::Installed => "installed", Self::OsStaged => "os-staged", Self::Failed => "failed" } }
}

struct UpdateReport {
    id: u64,
    target: UpdateTarget,
    outcome: UpdateOutcome,
    worker: update::WorkerSnapshot,
    cancel_requested: bool,
}

/// The page's typed process-local job. The installer and launcher still own the disk protocol.
#[derive(Default)]
struct Updates {
    /// Snapshot/journey fixtures can stand in for a release; real runs use the compiled release stamp.
    release_build: Option<bool>,
    sel: usize,
    manifest: Option<Manifest>,
    /// When the manifest now on offer was read successfully. Wall time counts sleep between visits.
    manifest_checked_at: Option<std::time::SystemTime>,
    phase: UpdatePhase,
    next_job: u64,
    done: u64,
    message: Option<String>,
    /// Shared with backup restoration, independently of the update job.
    restart_at: Option<Instant>,
    /// The offered app comes from this update file on the card, not from the server (`app_card_update`).
    card: Option<update::card::Found>,
    /// What the start's look on the card found, waiting for a free shelf to say it.
    card_pending: Option<update::card::Look>,
}

impl Updates {
    /// A numbered release: one built by scripts/release-app.sh, or Android's APK (always numbered by build-apk.sh).
    fn release_build(&self) -> bool { self.release_build.unwrap_or_else(|| update::release_build() || app_update_hint::ANDROID_APK) }

    /// One decision shared by the offer, version line, settings value and dot. Against the version the player reads
    /// (Android: the APK's, which Android's list offers by).
    fn app_available(&self) -> Option<&update::Release> {
        pocket_ui::update_page::available(self.manifest.as_ref(), update::shown_version(), self.release_build())
    }

    fn network_offer_is_fresh(&self, now: std::time::SystemTime) -> bool {
        const FRESH_FOR: Duration = Duration::from_secs(5 * 60);
        self.manifest.is_some() && self.manifest_checked_at
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age < FRESH_FOR)
    }

    fn remember_manifest(&mut self, manifest: Manifest, now: std::time::SystemTime) {
        self.manifest = Some(manifest);
        self.manifest_checked_at = Some(now);
    }

    fn start(&mut self, task: UpdateTask) {
        self.next_job = self.next_job.wrapping_add(1).max(1);
        let worker = task.cancel().map_or_else(update::WorkerSnapshot::default, |c| c.snapshot());
        self.phase = UpdatePhase::Running(UpdateJob {
            _card: pocket_ui::background_io::pause_for_update(),
            id: self.next_job, label: task.label(), task, since: Instant::now(), worker,
        });
    }

    fn finish(&mut self, outcome: UpdateOutcome) {
        if let UpdatePhase::Running(job) = std::mem::take(&mut self.phase) {
            let cancel_requested = job.task.cancel().is_some_and(|c| c.is_cancelled());
            self.phase = UpdatePhase::Finished(UpdateReport { id: job.id, target: job.task.target(), outcome, worker: job.worker, cancel_requested });
        }
    }

    fn is_busy(&self) -> bool { matches!(self.phase, UpdatePhase::Running(_)) }
    fn confirming(&self) -> bool { matches!(self.phase, UpdatePhase::Confirming) }
    fn set_confirmation(&mut self, on: bool) {
        if on { self.phase = UpdatePhase::Confirming; } else if self.confirming() { self.phase = UpdatePhase::Idle; }
    }
    fn busy_since(&self) -> Option<Instant> { match &self.phase { UpdatePhase::Running(j) => Some(j.since), _ => None } }
    fn cancel(&self) -> Option<&update::Cancel> { match &self.phase { UpdatePhase::Running(j) => j.task.cancel(), _ => None } }
    fn busy(&self) -> Option<(&str, u64)> {
        match &self.phase {
            UpdatePhase::Running(j) if j.task.cancel().is_some_and(|c| c.is_cancelled()) => Some(("Cancelling", 0)),
            UpdatePhase::Running(j) => Some((&j.label, j.task.total())),
            _ => None,
        }
    }
    fn can_cancel(&self) -> bool { self.cancel().is_some_and(|c| !c.is_cancelled()) }
    /// An app install's step and how far along it is (0 to 1), for the page's bar and step line; None for a check, a system
    /// update or a cancel. Step 4 is the installed app waiting for its restart.
    fn app_step(&self) -> Option<(update::Step, f32)> {
        match &self.phase {
            UpdatePhase::Running(job) => {
                let UpdateTask::App { cancel, .. } = &job.task else { return None };
                if cancel.is_cancelled() { return None; }
                let (w, step) = (job.worker, job.worker.phase.step());
                let frac = match step {
                    // Every file of the download on one bar that only moves forward (`update::Download`).
                    update::Step::Downloading => cancel.download().fraction(),
                    _ if w.work_total > 0 => w.work_done as f32 / w.work_total as f32,
                    _ => 0.0,
                };
                Some((step, frac.clamp(0.0, 1.0)))
            }
            UpdatePhase::Finished(r) if matches!(r.outcome, UpdateOutcome::Installed) && matches!(r.target, UpdateTarget::App(_)) => Some((update::Step::Restarting, 1.0)),
            _ => None,
        }
    }
    fn worker(&self) -> update::WorkerSnapshot { match &self.phase { UpdatePhase::Running(j) => j.worker, _ => Default::default() } }
    fn download(&self) -> update::Download { self.cancel().map(|c| c.download()).unwrap_or_default() }
    fn request_cancel(&mut self) -> bool {
        let Some(cancel) = self.cancel().filter(|c| !c.is_cancelled()) else { return false };
        cancel.cancel();
        true
    }
    fn progress(&mut self, bytes: u64) {
        self.done = bytes;
        if let UpdatePhase::Running(job) = &mut self.phase { job.worker.bytes = bytes; }
    }
}

#[cfg(test)]
mod update_job_characterisation {
    use super::*;

    #[test]
    fn r36s_fix_a_update_work_cancels_shelf_reads_until_it_finishes() {
        let _guard = READER_GAME_TEST.lock().unwrap_or_else(|e| e.into_inner());
        for task in [
            UpdateTask::Check,
            UpdateTask::App { version: "9.8.7".into(), total: 100, cancel: update::Cancel::new() },
        ] {
            let mut updates = Updates::default();
            let mut started = false;
            assert!(pocket_ui::background_io::job(|| {
                started = true;
                updates.start(task);
            }).is_none());
            assert!(started);
            assert!(pocket_ui::background_io::job(|| ()).is_none());
            updates.finish(UpdateOutcome::Failed);
            assert!(pocket_ui::background_io::job(|| ()).is_some());
        }
    }

    #[test]
    fn only_a_recent_successful_network_list_skips_the_check_on_open() {
        let now = std::time::SystemTime::now();
        let mut updates = Updates { manifest: Some(Manifest::default()), ..Updates::default() };
        assert!(!updates.network_offer_is_fresh(now), "an undated list is stale");
        updates.remember_manifest(Manifest::default(), now);
        assert!(updates.network_offer_is_fresh(now + Duration::from_secs(299)));
        assert!(!updates.network_offer_is_fresh(now + Duration::from_secs(300)));
        assert!(!updates.network_offer_is_fresh(now - Duration::from_secs(1)), "the player's clock went backwards");
    }

    #[test]
    fn cancellation_keeps_progress_and_the_original_sleep_timeout() {
        let _guard = READER_GAME_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let mut updates = Updates::default();
        let cancel = update::Cancel::new();
        updates.start(UpdateTask::App { version: "9.8.7".into(), total: 100, cancel: cancel.clone() });
        let since = updates.busy_since();
        updates.done = 42;
        assert!(updates.can_cancel() && updates.request_cancel());
        assert!(cancel.is_cancelled());
        assert!(!updates.can_cancel() && !updates.request_cancel());
        assert_eq!(updates.busy(), Some(("Cancelling", 0)));
        assert_eq!(updates.busy_since(), since, "Cancel must not extend the auto-sleep block");
        assert_eq!(updates.done, 42, "the last received bytes are retained");
        assert!(!update::busy_blocks_sleep(since, since.unwrap() + update::BUSY_BLOCKS_SLEEP_FOR));
        updates.finish(UpdateOutcome::Failed);
        assert!(!updates.is_busy() && updates.busy_since().is_none() && updates.cancel().is_none());
    }

    #[test]
    fn a_manifest_check_has_no_cancel_action_and_does_not_clear_a_backup_restart() {
        let _guard = READER_GAME_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut updates = Updates { restart_at: Some(deadline), ..Updates::default() };
        updates.start(UpdateTask::Check);
        assert!(!updates.can_cancel() && !updates.request_cancel());
        assert!(updates.busy_since().is_some());
        assert_eq!(updates.restart_at, Some(deadline));
        updates.finish(UpdateOutcome::Failed);
        assert_eq!(updates.restart_at, Some(deadline), "backup restoration shares this deadline");
    }
}

/// Row of the Settings list called `name`, to come back to from its page.
fn settings_row(name: &str) -> usize {
    SETTINGS.iter().position(|s| s.0 == name).unwrap_or(0)
}

/// First index of a scrolled window of `visible` rows that keeps `sel` inside it.
fn window_start(sel: usize, visible: usize, len: usize) -> usize {
    pocket_ui::pause_page::window_start(sel, visible, len)
}

/// See `App::settings_page_lets_home_go`.
fn page_lets_home_go(page: Option<SettingsPage>) -> bool {
    use SettingsPage::*;
    matches!(page, Some(List(_) | Tabs(_) | About | Battery(_) | Leds(_) | NightLight(_) | CozyExtras(_) | SleepPower | Curator))
}

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
enum SettingsPage {
    /// A category's list, at this row of `SETTINGS`: the category is the one the row is in. START opens the first
    /// category's (`app_settings_groups::first_row`); L1/R1 go to the others, B closes Settings.
    List(usize),
    /// The same category with the focus on the row of its symbols at the top (`app_settings_groups::groups_input`): Up on
    /// the first row goes there, Left/Right (and L1/R1) switch the category, Down or A goes back into its rows. The row is
    /// the category's first, the list under the symbols shows that category.
    Tabs(usize),
    /// The starter pack's games (`app_starter`).
    Starter,
    /// Will the games start (`app_game_check`).
    GameCheck,
    Updates,
    /// Index into `battery::RANGES` for the graph.
    Battery(usize),
    /// The user guide; where the reader is lives in `App::manual`.
    Manual,
    About,
    Sync,
    Pockets,
    /// The selected row.
    Leds(usize),
    /// The selected row (`app_night_light.rs`).
    NightLight(usize),
    /// The selected row (`app_cozy_extras.rs`).
    CozyExtras(usize),
    /// Settings → Controls → Buttons, the selected row (`app_buttons.rs`).
    Buttons(usize),
    Controllers(usize),
    Tailscale,
    Romm,
    Bluetooth,
    Web,
    PerfFan,
    Curator,
    Achievements,
    Tools,
    /// Settings → Controls → Test buttons; the tester itself is `App::test_buttons` (`app_test_buttons.rs`).
    TestButtons,
    PcStreaming,
    Backup,
    Storage,
    Gamelist,
    Screenshots,
    /// Settings → Games → Android games (`app_android_games.rs`).
    AndroidGames,
    /// Settings → System → Sleep & power; the page itself is `App::sleep_page`.
    SleepPower,
}

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
enum Screen {
    Home,
    Games(usize),
    Favorites,
    /// Curator's list (the Lens, smart shelves, stickers), a smart shelf, a sticker, the Lens's games (`app_curator`).
    Curator,
    Shelf(usize),
    Sticker(u32),
    LensResults,
    /// The games a fact of a card leads to ("More from Criterion Games") and Curator's Rediscover (`app_connections`).
    Connection,
    Rediscover,
    /// Shelf space's crate: the games not on the shelf (`app_crate`).
    Crate,
}

struct Running {
    system: String,
    name: String,
    path: PathBuf,
    emulator: String,
    core: String,
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
const HOST_BIN_DEFAULT: &str = "/storage/pocket-ui/pocket-core";

fn host_bin() -> &'static str {
    static P: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    P.get_or_init(|| std::env::var("POCKETUI_HOST_BIN").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| HOST_BIN_DEFAULT.to_string()))
}

/// Who plays a game, read once from the emulator's name as system.cfg and the routes give it ("Code shape" in
/// docs/DEVELOPING.md): what comes after matches on this, not on the name.
#[derive(Clone, Debug, PartialEq)]
enum Player {
    /// pocket-ui's own host, pocket-core.
    Hosted,
    /// ARMSX2, Dolphin, Azahar or hakuX, which pocket-ui drives itself (`standalone::Emu`); with the emulator's name.
    Standalone(standalone::Emu, String),
    /// RetroArch, a port's script, an app or another program, by its name.
    Other(String),
}

impl Player {
    fn of(system: &str, emulator: &str, core: &str) -> Player {
        match (emulator, standalone::Emu::detect(system, emulator, core)) {
            ("pocket-core", _) => Player::Hosted,
            (_, Some(emu)) => Player::Standalone(emu, emulator.to_string()),
            (_, None) => Player::Other(emulator.to_string()),
        }
    }

    /// The emulator's name, as runemu.sh and the controller bridge take it.
    fn name(&self) -> &str {
        match self {
            Player::Hosted => "pocket-core",
            Player::Standalone(_, name) | Player::Other(name) => name,
        }
    }
}

/// A game to start, as one value: built once in `App::launch` from the route, then handed on (`spawn_game`).
struct Run {
    /// The shelf's resolved title, including gamelist/checksum corrections.
    #[cfg_attr(not(all(target_os = "android", feature = "handheld")), allow(dead_code))]
    name: String,
    system: String,
    player: Player,
    core: String,
    path: PathBuf,
}

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
        pocket_ui::slots::states_dir().join(&self.system).join(format!("{stem}.{ext}"))
    }
}

/// In-game menu opened with a HOME tap while a RetroArch game runs.
struct QuickMenu {
    sel: usize,
    /// The game as it looked when the menu opened, shown dimmed behind the menu.
    frame: Option<Image>,
    /// Where the game is in `frame`, without black borders or a screen frame, when pocket-core said (`app_zoom`).
    game_rect: Option<pocket_ui::preview::GameRect>,
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
    /// pocket-core: Load has a last stop after its states, "From the start".
    restart: bool,
    /// Quit was chosen; the menu stays up until the game has saved and exited.
    quitting: bool,
    /// Quit was chosen and leaves out the resume point (`without_saving`).
    discarding: bool,
    /// Quit leaves out the resume point: Settings > Continue where I quit is off and the emulator can
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
    /// Short confirmation of what Keep for or Restore defaults did, shown under the options list.
    note: Option<&'static str>,
    /// Restore defaults was pressed once: the next A does it (any other button takes it back).
    restore_armed: bool,
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
    /// The Zoom page while it is open (`app_zoom`).
    zoom: Option<pocket_ui::zoom::Zoom>,
    /// Zoom has a row on the main page: Big text is on or Settings → Playing → Zoom in the pause menu is (`app_zoom`).
    zoom_row: bool,
    /// Where B on the Zoom page goes (`app_zoom`).
    zoom_back: app_zoom::Back,
    /// HOME + X opened this menu for Zoom: the page is up (black) while the picture is still on its way (`app_zoom`).
    zoom_pending: bool,
    /// What saving can do for this emulator (`pause_saves`): Save and Load only where they do something, else a line.
    saves: pocket_ui::pause_saves::Saves,
    /// A multi-disc game on pocket-core: the Disc row and the disc chosen on it (`app_disc`).
    disc: Option<pocket_ui::disc::DiscRow>,
    /// pocket-core: the Earlier row, its states and the one chosen (`app_earlier`); None: no row.
    earlier: Option<pocket_ui::timeline::Row>,
    /// pocket-core's saves of this game don't reach the card (`app_save_trouble`): a line under the title, and Quit
    /// asks first. Read when the menu opens.
    save_trouble: Option<pocket_ui::save_trouble::Trouble>,
    /// Save was pressed with Animations on Cozy and Motion on Full: the state is written, the game stays paused while the
    /// saved picture flies into its slot, then it goes on (`save_fill`, `app_save_media`).
    saving: Option<pocket_ui::save_fill::Saving>,
    /// The Achievements page's Hardcore row was pressed once while off: the next A starts the game again in Hardcore
    /// (`app_achievements`); any other button takes it back.
    hardcore_ask: bool,
    /// The lines of Page::ShortcutList, worked out when it opened (`app_shortcut_list`).
    shortcut_list: Vec<pocket_ui::shortcut_list::Line>,
}

/// Which screen of the pause menu is showing.
#[derive(Clone, Copy, PartialEq, Hash, Debug)]
enum Page {
    Main,
    /// A load was refused before mutation because its undo capture failed.
    LoadBlocked,
    RetroArchStateUnconfirmed(pocket_ui::ra_state::Action),
    /// The list of groups plus "Keep for" and "Restore defaults".
    Options { sel: usize },
    Group { group: usize, sel: usize },
    /// The core's options: `cat` is the category being shown (None: the top level).
    Emulator { cat: Option<usize>, sel: usize },
    /// Options > Controls: the stick, what each button presses, Hold to toggle, Reset controls, then Controller setup.
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
    /// Zoom (`app_zoom`): the frozen picture larger; its state is in `QuickMenu::zoom`.
    Zoom,
    /// A running port's Performance & fan (`app_port_perf`); its state is in `App::port_perf`.
    PortPerf,
    /// "Couldn't save. Quit anyway?" before Quit while the game's saves fail (`app_save_trouble`).
    QuitUnsaved { sel: usize },
    /// A RetroArch game's HOME shortcuts, from Runs on (`app_shortcut_list`); the lines are in `QuickMenu::shortcut_list`.
    ShortcutList { sel: usize },
}

/// Lines of the options list (`pause_options::LinuxEntry`, in the library so the comparison with Android's page reads
/// this very list): the groups of `options::GROUPS` with Emulator and Controls after the first one (Picture), and Keep
/// for and Restore defaults last.
use pocket_ui::pause_options::LinuxEntry as OptEntry;

#[cfg(not(feature = "brick"))]
fn opt_entries(emu: Option<&EmuMenu>, cheats: bool) -> Vec<OptEntry> {
    pocket_ui::pause_options::linux_entries(options::GROUPS.len(), emu.map(EmuMenu::has_options), cheats)
}

#[cfg(feature = "brick")]
fn opt_entries(emu: Option<&EmuMenu>, cheats: bool) -> Vec<OptEntry> {
    let mut v: Vec<OptEntry> = (0..options::GROUPS.len()).map(OptEntry::Group).collect();
    // The Brick without the GPU path draws none of the Picture rows (no shaders, frames, scaling modes): they stay out (`brick_profile`).
    #[cfg(feature = "brick")]
    let picture_out = pocket_ui::brick_profile::picture_rows_out();
    #[cfg(not(feature = "brick"))]
    let picture_out = false;
    if picture_out {
        v.retain(|e| !matches!(e, OptEntry::Group(i) if options::GROUPS[*i].name == "Picture"));
    }
    if let Some(emu) = emu {
        let at = if picture_out { 0 } else { 1.min(v.len()) };
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
    // Zoom after what changes the game (`app_zoom`): it stood second, under Picture, and a niche help read as one of the
    // main things (0.5.1, the hand test: the Zoom row in Settings was too prominent too; it went down to Playing).
    v.push(OptEntry::Zoom);
    v.push(OptEntry::RunsOn);
    v.push(OptEntry::Keep);
    v.push(OptEntry::Restore);
    v
}

fn opt_pos(emu: Option<&EmuMenu>, cheats: bool, e: OptEntry) -> usize {
    opt_entries(emu, cheats).iter().position(|x| *x == e).unwrap_or(0)
}

impl QuickMenu {
    /// The Controls page's last row, Controller setup: after the stick, the buttons, Hold to toggle and Reset controls.
    fn controller_setup_row(&self) -> usize {
        self.emu.as_ref().map_or(0, |e| e.control_count())
    }

    /// How many stops Load has: its states, then From the start where the game can.
    fn load_stops(&self) -> usize {
        self.loads.len() + usize::from(self.restart)
    }

    /// Load is on its last stop, From the start.
    fn at_restart(&self) -> bool {
        self.restart && self.load_at == self.loads.len()
    }

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
        if self.item() == MainItem::Load && self.at_restart() {
            // No state, no picture: the window says what the stop does.
            self.saved = None;
            return;
        }
        if let (Some(states), Some(t)) = (self.states.as_ref(), self.shown()) {
            self.saved = states.preview(t);
        }
    }

    fn items(&self) -> Vec<MainItem> {
        let base = main_items(self.hosted, self.saves, self.achievements.is_some());
        arrange_items(base, self.earlier.is_some(), self.hosted && self.zoom_row, self.disc.is_some(), self.pixel_item, self.players_item)
    }

    /// The selected row of the main page.
    fn item(&self) -> MainItem {
        self.items().get(self.sel).copied().unwrap_or(MainItem::Continue)
    }
}

/// Retro's pause menu's sizes and window width: `pause_page` (`RETRO_ROW_END`, `RETRO_GAP`, `retro_list_w`).

/// A pause menu row's picture in the Retro style (`retro`).
fn retro_icon(item: MainItem) -> pocket_ui::retro::Icon {
    use pocket_ui::retro::Icon;
    match item {
        MainItem::Continue => Icon::Continue,
        MainItem::Save => Icon::Save,
        MainItem::Load | MainItem::Earlier => Icon::Load,
        MainItem::Disc => Icon::Games,
        MainItem::Options => Icon::Options,
        MainItem::Achievements | MainItem::Players => Icon::Playing,
        MainItem::PixelPath | MainItem::Zoom => Icon::Look,
        MainItem::Quit => Icon::Quit,
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
    /// The graded chain of the game's picture and sound (`app_pixel_path`), when switched on in Settings.
    PixelPath,
    /// Which controller is which player (`app_players`), pocket-core games with two or more controllers.
    Players,
    /// The frozen picture 2x or 3x as large, moved with the D-pad (`app_zoom`), pocket-core games.
    Zoom,
    /// Ends the game, with or without a new resume state (Settings > Continue where I quit, `without_saving`);
    /// in-game saves are written either way. The one Quit, as in NextUI's menu.
    Quit,
}

impl MainItem {
    /// What the row says on screen: Quit reads "Save & quit" where it keeps the moment (`quit_saves`).
    fn shown(self, quit_saves: bool) -> &'static str {
        if self == MainItem::Quit && quit_saves { "Save & quit" } else { self.label() }
    }

    fn label(self) -> &'static str {
        match self {
            MainItem::Continue => "Continue",
            MainItem::Save => "Save",
            MainItem::Load => "Load",
            MainItem::Earlier => "Earlier",
            MainItem::Disc => "Disc",
            MainItem::Options => "Options",
            MainItem::Achievements => "Achievements",
            MainItem::PixelPath => "Pixel Path",
            MainItem::Players => "Players",
            MainItem::Zoom => "Zoom",
            MainItem::Quit => "Quit",
        }
    }

    /// Whether A opens a page of its own (Options, Achievements, Pixel Path, Players, Zoom): the row ends in the drawn
    /// angle on every row and A's hint says Open, as Settings' page rows (0.5.1, the hand test: which rows open a page
    /// was not clear in the pause menu either). Continue, Save, Load, Earlier, Disc and Quit act right there.
    fn opens_page(self) -> bool {
        matches!(self, MainItem::Options | MainItem::Achievements | MainItem::PixelPath | MainItem::Players | MainItem::Zoom)
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
    // No row for the quick settings: START opens them from any row (the hint says so), as HOME does on the shelf.
    items.push(Quit);
    items
}

/// The main page's rows (`main_items`) with the rows that come and go put in their places: Earlier only while it has a
/// row (`app_earlier`: switched on, or states of it kept); Zoom, for pocket-core, only where the player asked for it (Big
/// text or its own switch, `app_zoom`; otherwise a row of Options), before Quit, after what belongs to the game; Disc for a
/// game with more than one, after Save and Load, before Options (`app_disc`); Pixel Path, when switched on in Settings,
/// before Quit; Players, with a second controller, before Pixel Path and Quit. Android's player keeps the same order
/// (`pause_list::order`, tests `the_main_page_is_linux_s_in_android_s_player`).
fn arrange_items(mut items: Vec<MainItem>, earlier: bool, zoom: bool, disc: bool, pixel: bool, players: bool) -> Vec<MainItem> {
    if !earlier {
        items.retain(|i| *i != MainItem::Earlier);
    }
    if zoom {
        let at = items.iter().position(|i| *i == MainItem::Quit).unwrap_or(items.len());
        items.insert(at, MainItem::Zoom);
    }
    if disc {
        let at = items.iter().position(|i| *i == MainItem::Options).unwrap_or(items.len());
        items.insert(at, MainItem::Disc);
    }
    if pixel {
        let at = items.iter().position(|i| *i == MainItem::Quit).unwrap_or(items.len());
        items.insert(at, MainItem::PixelPath);
    }
    if players {
        let at = items.iter().position(|i| matches!(i, MainItem::PixelPath | MainItem::Quit)).unwrap_or(items.len());
        items.insert(at, MainItem::Players);
    }
    items
}

/// Whether the pause menu's Quit saves the moment (and so says "Save & quit" with a line about next time): Save and Load
/// are there (pocket-core's and the standalone emulators' slots, RetroArch's one save) and Continue where I quit is not off.
fn quit_saves(without_saving: bool, saves: pocket_ui::pause_saves::Saves) -> bool {
    !without_saving && saves.rows()
}

/// Whether the pause menu's Quit leaves out the resume point: Settings > Continue where I quit is off and the
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
    /// Load's last stop: the game from its start, the moment before kept (pocket-core).
    Restart,
}

/// NextUI's menu metrics (padding 10, pill 30, button 20 at its scale 1), scaled by 3 for this panel (`kit`).
const NU_PAD: i32 = pocket_ui::kit::PAD;
const NU_PILL: i32 = pocket_ui::kit::PILL;
const NU_BTN_PAD: i32 = pocket_ui::kit::BTN_PAD;
const NU_FONT: u32 = pocket_ui::kit::FONT;
const NU_SMALL: u32 = pocket_ui::kit::SMALL;
/// The hints of a pause-menu page's row with a value (the Picture page's rows: its preview leaves them the room).
/// How far the game is darkened behind the quick settings opened from the pause menu (the menu itself: 225).
const QUICK_OVER_GAME_FADE: u8 = 205;

/// What a frame is drawn onto: softbuffer's surface; on Android the handheld view's own (`native_surface`).
#[cfg(not(all(target_os = "android", feature = "handheld")))]
type WindowSurface = softbuffer::Surface<Rc<Window>, Rc<Window>>;
#[cfg(all(target_os = "android", feature = "handheld"))]
type WindowSurface = pocket_ui::native_surface::NativeSurface;

struct App {
    proxy: EventLoopProxy<UserEvent>,
    started: Instant,
    window: Option<Rc<Window>>,
    surface: pocket_ui::frame::Presenter<WindowSurface>,
    text: Text,
    systems: Vec<System>,
    load_error: Option<String>,
    /// The system list can't be read at all and the shelf runs on the built-in one (`library_system_list_check`).
    system_list_fallback: bool,
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
    retroarch_state_pending: Option<Instant>,
    launch_at: Option<Instant>,
    pending_launch: Option<app_launch::Pending>,
    /// A phone game's first start loading Java (`j2me_java`): its progress over the start, B stops it.
    java_fetch: Option<std::sync::Arc<pocket_ui::j2me_java::Fetch>>,
    drew_first: bool,
    /// The first frames after the Pocket started fade in from black (`boot_fade`, docs/BOOT-SPLASH.md).
    boot_fade: pocket_ui::boot_fade::BootFade,
    /// `after_first_frame` ran.
    after_first_done: bool,
    data_dir: PathBuf,
    theme: Theme,
    /// Settings > Games > Systems on the shelf: system ids the shelf and the switcher leave out.
    hidden: Vec<String>,
    toast: Option<(String, Instant)>,
    /// The shelf's toast stands over the hint bar this frame (`paint_home`): the hints give way for its moment.
    toast_over_hints: bool,
    /// Settings and lists the card refused to save, told on the shelf once a run (`app_save_trouble`).
    card_refusals: pocket_ui::save_trouble::CardRefusals,
    /// Settings → System → Sleep & power while it is open (`app_sleep_power`).
    sleep_page: pocket_ui::sleep_settings::Page,
    /// Settings → System → About: its selected row and the report's sheet (`app_report`).
    about: app_report::AboutUi,
    /// "Switched off after 3 h asleep…", to say on the shelf when the game the start resumed ends (`app_auto_off`).
    after_game_note: Option<String>,
    /// The TV the picture is on through USB-C, while it is (`app_display_out`).
    tv: Option<pocket_ui::display_out::Tv>,
    /// Why the last game could not start, from pocket-core; and every other message of what went wrong, which stays
    /// until A, B or a tap closes it (`app_messages`).
    notice: Option<Notice>,
    /// This frame painted the notice, so A and B close it (they are never swallowed by one nobody can see).
    notice_painted: bool,
    /// Words of `warn` that wait for the notice that is up to be closed.
    notice_later: Vec<String>,
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
    /// When the last frame was presented; quiet pages time their refreshes from it.
    frame_at: Instant,
    /// The next animation frame's deadline, with painting inside the refresh budget.
    frame_due: Instant,
    /// Whether the frame on screen was painted while something moved: then one more comes (`frame_pace::Settle`).
    settle: pocket_ui::frame_pace::Settle,
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
    /// Retained catalog choice for package downloads; existing settings keep their value.
    store_url: Option<String>,
    /// Settings → Games → Box art & downloads → Starter pack and Home's hint on an empty card (`app_starter`).
    starter: app_starter::StarterUi,
    /// Exit code for main once the event loop ends (`update::RESTART_CODE` after an update).
    exit_code: i32,
    onboarded: bool,
    es_systems: PathBuf,
    /// Scaled box art per (file, max width, max height); `None` = no cover for that game.
    covers: std::collections::HashMap<(PathBuf, i32, i32), Option<Rc<Image>>>,
    /// Box art in its film, each with the cover it was made from (`app_shrink_wrap::sealed_cover`): the last few.
    sealed_covers: Vec<(Rc<Image>, Rc<Image>)>,
    /// The console picture of a system card, fitted per (system, max width, max height).
    art: std::collections::HashMap<(String, i32, i32), Option<Rc<Sprite>>>,
    /// HOME is down in a game with no other button yet; releasing it opens the quick menu.
    home_tap: bool,
    /// Kids Mode (`app_kids`).
    kids: app_kids::Kids,
    /// The Big shelf (`app_big_shelf`).
    big: app_big_shelf::Big,
    /// The manual's reading position (Settings > Manual) and a held Up/Down waiting to start scrolling by itself.
    manual: pocket_ui::manual::Reader,
    manual_hold: Option<(Button, Instant)>,
    /// The manual was opened with SELECT from the first-start setup, which B goes back to.
    manual_from_setup: bool,
    /// Box art downloaded in the background while online (`pocket_ui::auto_covers`).
    cover_run: pocket_ui::auto_covers::AutoCovers,
    card_out_note: bool,
    /// The game card's descriptions loaded ahead while online, under the same rules (`pocket_ui::auto_texts`).
    text_run: pocket_ui::auto_texts::AutoTexts,
    /// The games recognised by their checksum, before the covers and descriptions (`app_identify`).
    identify: app_identify::Identify,
    /// The look step's previews, one per theme (`setup::look_preview`), and whether they show the built-in
    /// sample games; made when the step opens, dropped when what they show changes.
    look_previews: Option<(Vec<Image>, bool)>,
    /// While the previews are painted: the sample game's ROM path and its box art.
    sample_cover: Option<(PathBuf, Rc<Image>)>,
    /// The pause menu's Zoom level used last in this run (`app_zoom`).
    zoom_level: u32,
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
    /// The GameCube or Wii game Dolphin's card stands before (system, name, path): "Start the game" starts it.
    dolphin_launch: Option<(String, String, PathBuf)>,
    /// The game (system, file) Android's photo picker is choosing a cover for (`app_cover_pick`).
    cover_pick: Option<(String, PathBuf)>,
    touch_rec: touch::Recognizer,
    /// where it is in window pixels, whether its left button is down, and the wheel's fraction (`app_touch`).
    cursor: Option<(f64, f64)>,
    mouse_down: bool,
    wheel_carry: f32,
    /// Why touches were ignored last, so the log says it once per change (`app_touch`).
    ignore_log: touch::IgnoreLog,
    /// Settings > Sounds.
    sound: pocket_ui::ui_sound::Config,
    /// Settings > LEDs (`app_leds.rs`).
    leds: pocket_ui::leds::Config,
    /// The Brick's F keys and Battery lights (`app_brick_hw`).
    #[cfg(feature = "brick")]
    brick: app_brick_hw::State,
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
    game_infos: std::collections::HashMap<PathBuf, Option<Box<pocket_ui::gamedb::GameInfo>>>,
    /// `infos_warm_tick`: the next game of the library to look up ahead (system, game), and the lookups made by
    /// `game_info` so far (the card's check counts them).
    infos_warm: (usize, usize),
    infos_resolved: u64,
    /// Settings > Sync and the notice after a game (`app_sync.rs`).
    sync: pocket_ui::startup::Later<app_sync::SyncUi>,
    /// Settings > Tailscale (`app_tailscale.rs`).
    tailscale: pocket_ui::startup::Later<app_tailscale::TailscaleUi>,
    /// Settings > Games > RomM and the RomM games' downloads (`app_romm.rs`).
    romm: pocket_ui::startup::Later<app_romm::RommUi>,
    /// Settings > Web companion and its server (`app_web.rs`).
    web: pocket_ui::startup::Later<app_web::WebUi>,
    /// Pocket-to-Pocket sync (`app_pocket_sync.rs`): its switch is read at the start (one small file), its service starts later.
    pocket_sync: app_pocket_sync::PocketSyncUi,
    /// Settings > Bluetooth (`app_bluetooth.rs`).
    bluetooth: pocket_ui::startup::Later<app_bluetooth::BluetoothUi>,
    /// Settings > Performance & fan, and the fan outside games (`app_perf_fan.rs`).
    perf: app_perf_fan::PerfFanUi,
    /// Settings > Tools (`app_tools.rs`).
    tools: app_tools::ToolsUi,
    /// Settings > Controls > Test buttons (`app_test_buttons.rs`).
    test_buttons: app_test_buttons::TestButtons,
    /// Settings > Games > Game check (`app_game_check.rs`).
    game_check: app_game_check::GameCheckUi,
    /// Settings > PC streaming (`app_moonlight.rs`).
    moonlight: app_moonlight::MoonlightUi,
    /// Settings > Backup & reset (`app_backup.rs`).
    backup: app_backup::BackupUi,
    /// Settings > System > Storage, the low-space notice, the card's Hide or delete (`app_storage.rs`).
    storage: app_storage::StorageUi,
    /// EmulationStation's gamelists: what they hold, the covers from them, the page (`app_gamelist.rs`).
    gamelist: app_gamelist::GamelistUi,
    /// The Android games of Android's handheld view: the marks, Android's list, the page (`app_android_games.rs`).
    android_games: app_android_games::AndroidGamesUi,
    /// The screenshot gallery (`app_gallery.rs`).
    gallery: app_gallery::GalleryUi,
    /// When the wrong-clock look is due (`app_clock`); None once it is done.
    clock_look: Option<Instant>,
    /// The look for a newer pocket-ui and its once-per-version notice (`app_update_hint`).
    update_hint: app_update_hint::UpdateHintUi,
    update_inspection: update::observation::Inspection,
    update_inspector: Option<std::sync::mpsc::Sender<()>>,
    /// The "Runs on" page while it is open (`app_emu_choice.rs`).
    emu: app_emu_choice::EmuChoiceUi,
    /// The "This game" page over a game's card while it is open (`app_this_game.rs`).
    this_game: Option<pocket_ui::this_game_page::ThisGamePage>,
    controller_setup: app_controllers::Setup,
    /// RetroAchievements: the account page and the card lookups (`app_achievements.rs`).
    achievements: pocket_ui::startup::Later<app_achievements::AchievementsUi>,
    /// Moonlight is installed (`pocket_ui::moonlight`); "PC streaming" is on Home only when also ticked in
    /// Settings > Games > Systems on the shelf (`streaming_on`, off by default).
    streaming: bool,
    streaming_on: bool,
    /// Screenshots and title screens beside the box art: the switch, what is known about the files, the cycle of
    /// the Split preview and the thread that decodes them (`app_pictures.rs`).
    pictures: app_pictures::PicturesUi,
    /// Where the selection was when the covers ahead of it were last asked for (`app_cover_prefetch`).
    prefetch: app_cover_prefetch::Prefetch,
    /// Settings > Games > Box art & downloads > Pictures is Off: no box art, screenshots or title screens are fetched by themselves
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
    /// The game whose box front and cartridge are being built ahead, and how many tries it had (`app_card::box_warm_tick`).
    box_warm: Option<(Item, u8)>,
    /// The look previews were painted light.
    look_previews_light: bool,
    /// The colours, font and button symbols the look previews were painted with (`app_look::style_mark`).
    look_previews_theme: u32,
    /// The language the look previews were painted in.
    look_previews_lang: pocket_ui::i18n::Lang,
    /// And at which text size (R1 on the first start's look step changes it).
    look_previews_size: pocket_ui::type_scale::TextSize,
    /// The clock the previews were painted with: a new minute (or a zone chosen meanwhile) paints them again, so the
    /// small clock in each is the real one (new-user test 2026-10-30), not the time the step opened.
    look_previews_clock: Option<String>,
    /// The column a page of rows and text is painted into on a wide screen, kept between frames (`app_column`).
    column_buf: Vec<u32>,
    /// Settings → Language as chosen; `None` follows the system's (`i18n::Lang::system_default`).
    language: Option<pocket_ui::i18n::Lang>,
    /// The Cabinet look's shelves, animations and pictures (`app_cabinet`).
    cabinet: app_cabinet::CabinetUi,
    /// Settings → Games → Shelf space: the games on the shelf; the rest are in the crate (`pocket_ui::shelf_space`).
    shelf_space: pocket_ui::shelf_space::ShelfSpace,
    /// The crate on screen (`app_crate`).
    crate_ui: app_crate::CrateUi,
    /// Curator: the index, its pages, smart shelves (`app_curator`).
    curator: app_curator::CuratorUi,
    /// Settings > On start (`pocket_ui::on_start`).
    on_start: pocket_ui::on_start::OnStart,
    /// Settings > Continue where I quit (settings.json's `forget_where_quit`, the other way round): what the pause
    /// menu's one Quit does where the emulator can leave out the resume point (`QuickMenu::without_saving`).
    remember_quit: bool,
    /// Settings → Playing → Remind me where I was (settings.json's `where_was_i`; Off unless switched on).
    remind_where: bool,
    /// Settings → Playing → Zoom in the pause menu (settings.json's `zoom_in_pause`): Zoom on the pause menu's main page
    /// (Big text shows it too; Zoom is under Options either way, `app_zoom::zoom_row_on`).
    zoom_in_pause: bool,
    /// Settings → Look → Lived-in shelf (settings.json's `lived_in`): the Shelf look's rows of games lean and stack a
    /// little (`pocket_ui::lived_in`, `app_cabinet::shelf_row`). Off unless switched on.
    pub(crate) lived_in: bool,
    /// Settings → Playing → Console boot screen (settings.json's `boot_screen`): pocket-core reads it itself at a game's start;
    /// this is the switch's state (`pocket_ui::boot_screen`).
    boot_screen: bool,
    /// Settings → Screen & sound → Photos of big moments (settings.json's `moment_photos`, Off unless switched on): pocket-core and the
    /// Android player read it themselves at a game's start (`pocket_ui::album`); this is the switch's state.
    moment_photos: bool,
    /// Settings → Screen & sound → Photos of big moments → Also in play (settings.json's `moment_in_play`, Off unless chosen):
    /// pocket-core reads it at a game's start (`album::moment_in_play`).
    moment_in_play: bool,
    /// When the game that just ended began (Unix seconds, a minute early): its photos slide into the case on the way back
    /// (`app_album::album_tucked`).
    album_since: Option<i64>,
    /// "Where was I?": the card of the game being started, shown over its launch picture (`app_where_was_i`).
    where_was_i: Option<pocket_ui::where_was_i::Card>,
    /// "The case knows where you are": lines read from the games' battery saves (`app_case_knows`).
    case_knows: app_case_knows::CaseKnows,
    /// Settings → Look → Game start animation once chosen; unset follows the look (`App::cozy`).
    animations: Option<pocket_ui::settings::Animations>,
    /// X on a game opens its box (`game_box`), not the page; SELECT on the card switches and is remembered.
    game_box: bool,
    /// Settings > Controls > Buttons > Button layout (`pocket_ui::button_layout`; the pad threads read it from there).
    button_layout: pocket_ui::button_layout::ButtonLayout,
    /// Settings → Curator → Pixel Path in the pause menu (`Prefs::pixel_path`, off by default), kept in step by the
    /// Curator page; the pause menu reads only this.
    pixel_path: bool,
    /// The picture of where the starting game was left (`pocket_ui::launch_picture`), until the game ends.
    launch_picture: Option<pocket_ui::launch_picture::Prepared>,
    /// The games that stand open (`app_frozen`), newest first.
    frozen: Vec<PathBuf>,
    /// When each of those moments was written, from the same read (`frozen_got_timed`): the card's "Continue" line.
    moment_reads: pocket_ui::frozen::MomentReads,
    played_files: std::collections::HashSet<PathBuf>,
    played_scan: u64,
    /// The running game's auto state as it was before its session closed (`App::frozen_note_session`).
    frozen_auto: Option<PathBuf>,
    /// A big emulator is loading the moment of a frozen game, since then: the spinner.
    frozen_wait: Option<Instant>,
    /// A slow pocket-core load, including after the launch fade stops drawing.
    loading: app_loading::Loading,
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
    /// Which of `paint`'s painters painted the last frame (`app_where`: `@painted`, which `Place::painter` must name).
    painted: app_where::Painter,
    /// The status file, `/tmp/pocket-ui.status` (`app_where::status_tick`); None where there is no command FIFO.
    status: Option<pocket_ui::ui_status::Writer>,
    /// A write of it failed and the log said so (once).
    status_failed: bool,
    /// The page each button that is down was pressed on (`app_where::note_press_page`): its release, and a held
    /// direction's repeat, count only there.
    press_pages: Vec<(Button, String)>,
    /// What the inputs since the last navigation record asked for: "tick" (the haptic tick, whether or not menu haptics
    /// are on), "sound:Nav" (the sound chosen, whether or not sounds are on). Only under `POCKETUI_NAV_PROBE`
    /// (`app_nav_probe`'s `@feedback`, tests/nav_explorer.rs).
    feedback: Vec<String>,
}

impl App {
    /// A recent game Home, the switcher and On start show: its file is there, its system is on the shelf, and the
    /// player did not hide it (Hide or delete on the card, `app_storage`).
    fn recent_visible(&self, r: &recents::Recent) -> bool {
        r.path.exists() && shelf::recent_shown(&r.system, &self.hidden) && !self.library.is_hidden(&r.path)
    }

    /// Home's list: the recent games, Favorites, PC streaming, the systems.
    fn home_items(&self) -> Vec<Item> {
        // Looking around a card with no games (`app_starter`): nothing to continue, star or search. Only apps such as PC
        // streaming stand on the shelf, when they are on.
        if self.empty_home_around() {
            return Item::with_apps(Vec::new(), self.streaming && self.streaming_on);
        }
        // Settings > Games > Systems on the shelf: a hidden system's card and its recent games stay off Home.
        let recents = self.recents.list.iter().enumerate().filter(|(_, r)| self.recent_visible(r)).map(|(i, _)| i);
        let mut systems = shelf::visible_systems(self.systems.iter().map(|s| s.name.as_str()), &self.hidden);
        // Shelf space: a system shows while one of its games is on the shelf; the rest are in the crate (`app_crate`).
        systems.retain(|&s| self.systems[s].games.iter().any(|g| self.shelf_space.on_shelf(&g.path)));
        let home = Item::with_crate(Item::with_apps(Item::home(recents.take(RECENTS_ON_HOME), !self.favorite_items().is_empty(), systems), self.streaming && self.streaming_on), self.shelf_space_on());
        let home = Item::with_curator(home, self.curator_home_rows(), self.theme != Theme::Cabinet);
        // "Search" above Continue in the list looks (`app_search`); the cabinet opens it with Up on its top shelf.
        let home = Item::with_search(home, self.theme != Theme::Cabinet && self.systems.iter().any(|s| !s.games.is_empty()));
        // "Just Play" right above Continue (`app_curator`: with the Curator's switch, not in Kids Mode).
        Item::with_just_play(home, self.just_play_on())
    }

    fn rebuild(&mut self) {
        let big_was = self.big_before_rebuild();
        // Games arrived: the empty card's hint is over for good (`app_starter`).
        if !self.systems.is_empty() {
            self.starter.looking_around = false;
        }
        self.items = match self.screen {
            Screen::Home if self.kids_live() => self.kids_items(),
            Screen::Home if self.big_on() => self.big_items(),
            Screen::Home => self.home_items(),
            Screen::Games(s) => (0..self.systems[s].games.len()).filter(|&g| self.shelf_space.on_shelf(&self.systems[s].games[g].path)).map(|g| Item::Game(s, g)).collect(),
            Screen::Favorites => self.favorite_items(),
            Screen::Crate => self.crate_items(),
            screen => self.curator.released_items(screen).map_or_else(|| self.curator_items(screen), <[Item]>::to_vec),
        };
        self.big_after_rebuild(big_was);
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
            self.tell_new_names();
        }
    }

    /// The first time the highlight lands on the Curator or on Just Play on Home: one line says what the name means
    /// (once ever each, not in Kids Mode, not while the first start or Settings is open).
    fn tell_new_names(&mut self) {
        if self.screen != Screen::Home || self.running.is_some() || self.setup.is_some() || self.settings_page.is_some() {
            return;
        }
        let note = match self.items.get(self.sel) {
            Some(Item::Curator) if pocket_ui::first_game::tell_curator_once(&self.data_dir) => pocket_ui::first_game::CURATOR_NOTE,
            Some(Item::JustPlay) if pocket_ui::first_game::tell_just_play_once(&self.data_dir) => pocket_ui::first_game::JUST_PLAY_NOTE,
            _ => return,
        };
        self.toast = Some((note.into(), Instant::now()));
        self.request_redraw();
    }

    fn step(&mut self, b: Button) {
        if self.big_live() {
            return self.big_step(b, true);
        }
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
        // A button during the way back from a game ends it and goes on to the shelf (`app_cabinet::skip_eject`).
        self.skip_eject();
        // A button ends a sticker landing on the box's case or lifting off it: it sits (or is gone) at once.
        self.end_sticker_press();
        if self.empty_ports_press(b) { return; }
        if self.big_live() {
            return self.big_press(b);
        }
        // A on Home while the card has no games: the starter pack (`app_starter`).
        if self.starter_home_press(b) {
            return;
        }
        // Shelf space's crate: A takes a game out, the swap's own moves (`app_crate`), in every look.
        if self.crate_press(b) {
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
                if self.connection_back() || self.curator_back() {
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
            Some(item @ (Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover)) => self.curator_activate(item),
            Some(Item::Search) => self.open_search(),
            Some(Item::JustPlay) => self.curator_just_play(),
            Some(Item::Crate) => {
                self.home_item = Some(Item::Crate);
                self.screen = Screen::Crate;
                self.sel = 0;
                self.rebuild();
            }
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
        pocket_ui::covers::set_game_on(true);
        self.perf_fan_before_game();
        let pgid = spawn_app(self.proxy.clone(), cmd);
        control::note_game(pgid);
        let emulator = pocket_ui::moonlight::EMULATOR.to_string();
        self.running = Some(Running { system: emulator.clone(), name, path: PathBuf::new(), emulator, core: String::new(), started: Instant::now(), pgid, options: Default::default() });
        self.launch_at = Some(Instant::now());
        self.loading_start();
        self.held = None;
    }

    /// At start: the game of the resume note, or the last game when Settings > On start says so and nothing speaks
    /// against it (`pocket_ui::on_start::decide`).
    #[cfg(target_os = "linux")]
    fn start_game_at_start(&mut self, note: Option<pocket_ui::sleep::Resume>) {
        use pocket_ui::on_start::{self, Decision, Facts};
        if self.kids_live() {
            return self.kids_resume(note);
        }
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
        self.queue_launch(system, name, path);
    }

    pub(crate) fn launch_prepared(&mut self, pending: app_launch::Pending, at: pocket_ui::frozen::AtLaunch,
        wait: bool, resumable: bool, preview: bool, mut env: Vec<(&'static str, String)>) {
        let app_launch::Pending { run, name, asked, animation, skipped, .. } = pending;
        let (system, path, emulator, core) = (run.system.clone(), run.path.clone(), run.player.name().to_string(), run.core.clone());
        let hosted = run.player == Player::Hosted;
        if pocket_ui::startup::is_r36s_n64(&system) {
            println!("n64 launch: prepared after {:.0} ms", pocket_ui::startup::clock_ms() - asked);
        }
        self.search_launching(&path);
        println!("launch {system} {} (emulator {emulator}, core {core})", path.display());
        self.port_perf_before_launch(&system, &path);
        self.cover_run.game_started();
        self.text_run.game_started();
        self.identify.game_started();
        self.identify_after_playing(&system, &path);
        pocket_ui::covers::set_game_on(true);
        let mut frozen_env = Vec::new();
        let fresh = self.frozen_at_launch(&path, hosted, at, wait, &mut frozen_env);
        self.sync_game_started(&system);
        env.push((pocket_ui::startup::LAUNCHED_AT, format!("{asked:.1}")));
        env.extend(frozen_env);
        // Kids Mode: pocket-core's HOME shortcuts are off (`extras::NO_SHORTCUTS`).
        // Easy: no FPS or CPU numbers over the game, whatever Over the game says (`extras::NO_OVER_GAME`).
        if self.big_on() {
            env.push((pocket_ui::extras::NO_OVER_GAME, "1".into()));
        }
        if self.kids_live() {
            env.push((pocket_ui::extras::NO_SHORTCUTS, "1".into()));
            if system == "3ds" {
                env.push((standalone::KIDS_GPTOKEYB_ENV.0, standalone::KIDS_GPTOKEYB_ENV.1.into()));
            }
        }
        // "Where was I?" after a long break: decided from what is in memory (`app_where_was_i`). On pocket-core the game
        // then stands at its moment while the card is read, until a button (`startup::HOLD_STILL`).
        self.where_was_i_at_launch(&system, &path, fresh, resumable);
        pocket_ui::trace::step("launch reminder ready");
        pocket_ui::trace::step("launch animation ready");
        if skipped { self.where_was_i = None; }
        let reminder = self.where_was_i_hold(hosted, preview);
        if let Some(hold) = animation.max(reminder) {
            env.push((pocket_ui::startup::HOLD_UNTIL, format!("{:.1}", asked + hold.as_secs_f64() * 1000.0)));
            env.push((pocket_ui::startup::HOLD_SKIP, pocket_ui::startup::skip_path().to_string_lossy().into_owned()));
            if reminder.is_some() {
                env.push((pocket_ui::startup::HOLD_STILL, "1".into()));
                env.push((pocket_ui::startup::HOLD_READY, pocket_ui::startup::ready_path().to_string_lossy().into_owned()));
            } else if hosted && resumable && !fresh && self.cabinet.tv.grow.is_none() {
                // An open case on Cozy: its moment stands still behind the scene, nothing of it runs unseen.
                env.push((pocket_ui::startup::HOLD_PAUSE, "1".into()));
            }
        }
        // A on the Cabinet's playing TV (`app_cabinet_tv::Grow`): pocket-core goes on from the TV's state, still until the
        // picture has grown, its sound coming up softly; the TV was on already, so no switch-on and no other picture.
        let grow = self.cabinet.tv.grow.as_ref().map(|g| g.state.clone()).filter(|_| hosted);
        if let Some(state) = &grow {
            env.push((pocket_ui::showcase::STATE_ENV, state.to_string_lossy().into_owned()));
            env.push((pocket_ui::startup::HOLD_PAUSE, "1".into()));
            env.push((pocket_ui::showcase::FADE_IN_ENV, pocket_ui::showcase::FADE_IN.as_millis().to_string()));
        }
        // Keep the UI handover and pocket-core's switch-on on the same launch decision.
        let crt_on = grow.is_none() && animation.is_some() && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full && extra(Extra::CrtOn);
        if crt_on {
            // Cozy's ending: the old-TV power-on over the game's first moments, a handheld's display switching on for a
            // game that had no TV (`eject::Screen::of`, as the way back); its sound goes with Case sounds.
            let screen = match pocket_ui::eject::Screen::of(pocket_ui::cabinet::FormFactor::of(&system)) {
                pocket_ui::eject::Screen::Lcd => pocket_ui::crt_on::Screen::Lcd,
                pocket_ui::eject::Screen::Tv => pocket_ui::crt_on::Screen::Tv,
            };
            env.push((pocket_ui::crt_on::ENV, screen.env_value().into()));
            // A handheld's display switches on as the real one did (Handheld switch-on, `lcd_on`).
            if screen == pocket_ui::crt_on::Screen::Lcd && extra(Extra::HandheldOn) {
                env.push((pocket_ui::lcd_on::ENV, pocket_ui::lcd_on::Panel::of(&system).key().into()));
            }
            if !extra(Extra::CaseSounds) {
                env.push((pocket_ui::crt_on::SILENT_ENV, "1".into()));
            }
        }
        // Power lights off (Cozy extras): the frame's light stays as it is drawn (`frame_led::live`).
        if !extra(Extra::PowerLights) {
            env.push((pocket_ui::frame_led::ENV, "0".into()));
        }
        // Smooth fan: a pocket-core game tells pocket-ui its fan choice instead of changing the trips (`app_perf_fan`).
        env.extend(self.perf_fan_game_env().filter(|_| hosted));
        // RetroArch on ROCKNIX: pocket-core's HOME shortcuts, with this game's switches (`ra_hotkeys`). Not the dArkOS
        // port's or Android's RetroArch, whose configuration is theirs.
        if emulator == "retroarch" && !pocket_ui::device::darkos_port() && !pocket_ui::device::android_profile() {
            let values = options::GameSettings::load(&self.data_dir, &system, &path).effective();
            let on = pocket_ui::ra_hotkeys::enabled(&values, self.kids_live(), self.big_on());
            if let Err(e) = pocket_ui::ra_hotkeys::prepare(&pocket_ui::ra_hotkeys::Paths::device(), on) {
                eprintln!("ra_hotkeys: {e}");
            }
        }
        // The dArkOS port's games in dArkOS's command: the same shortcuts for its RetroArch, in a file of this session that
        // host-launch.py hands over as --appendconfig (dArkOS's retroarch.cfg stays as it is).
        if emulator == "darkos" && pocket_ui::device::darkos_port() {
            let values = options::GameSettings::load(&self.data_dir, &system, &path).effective();
            let on = pocket_ui::ra_hotkeys::enabled(&values, self.kids_live(), self.big_on());
            if let Some(paths) = pocket_ui::ra_hotkeys::DarkosPaths::from_env() {
                if let Err(e) = pocket_ui::ra_hotkeys::prepare_darkos(&paths, on) {
                    eprintln!("ra_hotkeys: dArkOS session: {e}");
                }
            }
        }
        pocket_ui::mark!("launch", "{system} {}", path.display());
        pocket_ui::mako_osd::show_game(hosted);
        pocket_ui::trace::step("launch spawn begin");
        let pgid = spawn_game(self.proxy.clone(), &run, &env);
        pocket_ui::trace::step("launch spawn ready");
        // A port: its performance once its own process runs, and the busy-core look (`app_port_perf`, on a thread).
        self.port_perf_started(pgid, &path);
        println!("launch: game process started {:.1} ms after it was asked for", pocket_ui::startup::clock_ms() - asked);
        pocket_ui::trace::step("launch post spawn");
        // How it ends decides whether Settings > On start may continue it (`on_start`).
        // Play history: the session's start, written off this thread (`play_log`).
        pocket_ui::play_log::started(&system, &name, &path, &emulator, &core);
        // TV-on must open onto the game for the first time: a preview here would flash over the shelf before
        // pocket-core starts dark. The deliberate picture behind a readable reminder still needs to be there.
        self.launch_picture = None;
        if grow.is_none() {
            let (proxy, data_dir, system, path, preview_core) = (self.proxy.clone(), self.data_dir.clone(), system.clone(), path.clone(), core.clone());
            let (w, h) = self.screen_size();
            std::thread::spawn(move || {
                let prepared = if crt_on && reminder.is_none() {
                    None
                } else {
                    let picture = hosted.then(|| pocket_ui::video::VideoSettings::load(&data_dir, &system, &path)).map(|v| (v.scaling, v.orientation.turns()));
                    pocket_ui::launch_picture::prepare_for_core(&data_dir, &system, &path, &preview_core, picture, w, h)
                };
                // Even an omitted picture must clear a reminder that has no image, just as a failed read does.
                let _ = proxy.send_event(UserEvent::LaunchPicture(path, prepared));
            });
        }
        pocket_ui::trace::step("launch picture scheduled");
        control::note_game(pgid);
        #[cfg(feature = "brick")]
        self.leds_game_started(&system, &path);
        #[cfg(not(feature = "brick"))]
        self.leds_game_started(&system);
        self.sleep.game_started(pocket_ui::sleep::Game::with_core(&system, &name, &path, &emulator, &core, pgid));
        pocket_ui::battery_history::set_activity(Some(&system));
        // The battery per game: this game's stretches on battery go to its ledger (`battery_games`).
        pocket_ui::battery_history::set_game(pocket_ui::battery_games::Key::recorded(&system, &path, &emulator));
        pocket_ui::trace::step("launch bookkeeping ready");
        self.running = Some(Running { system, name, path, emulator, core, started: Instant::now(), pgid, options: Default::default() });
        self.loading_start();
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

    fn game_exited(&mut self, secs: u64, ok: bool, start: pocket_ui::fallback::StartOutcome, moments: bool) {
        pocket_ui::trace::step("exit begin");
        pocket_ui::h700_gba::preload();
        // Asked to stop (Quit in the menu, or the grace period ran out) is no failure.
        let quit_requested = self.menu.as_ref().is_some_and(|m| m.quitting) || self.quit.is_some();
        // Options applied by a restart: the emulator closed after writing the resume state.
        let reapply = self
            .reapply
            .take()
            .filter(|_| self.running.is_some())
            .and_then(|mut r| (r.restart.on(emu_settings::Event::Exited) == Some(emu_settings::Cmd::Relaunch)).then_some(r));
        let Some(run) = self.running.take() else { return };
        // The game's CPU profile goes back, and with it the Brick's slow `ondemand` (`power::quick_ondemand`).
        pocket_ui::power::ui_took_over();
        // Its battery save may have changed: the line is read again at the next look (`app_case_knows`).
        self.case_forget(&run.path);
        // How far the launch animation had got when it ended: a start that failed comes back out from there (`begin_failed_start`).
        let ran = self.launch_at.map_or(Duration::from_secs(60), |t| t.elapsed());
        pocket_ui::covers::set_game_on(self.pending_launch.is_some());
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
        pocket_ui::trace::step("exit sound ready");
        self.frozen_note_session(&run);
        // An error nobody asked for: Settings > On start doesn't start this game by itself next time.
        pocket_ui::on_start::note_ended(&self.data_dir, &run.path, !ok && !quit_requested);
        // pocket-core's reason for not starting, else a save that didn't reach the card at the end (`save_trouble`);
        // after a standalone emulator, a parked save state that couldn't go back.
        self.notice = if run.hosted() { Notice::take_after_game() } else { Notice::take_after_standalone() };
        // The start resumed this game after a switch-off after sleeping: why it was off, now on the shelf.
        self.switched_off_after_game();
        // A run that failed at once never put the game on the screen (`fallback::never_started`): it is no play
        // session, and when no one else explains it (pocket-core's own reason, the fallback's notice) pocket-ui does.
        // A port's script exits 0 whatever happened, so for ports the exit code doesn't count (`port_never_started`).
        let port = run.system == "ports";
        let never_started = if run.hosted() { !ok && start == pocket_ui::fallback::StartOutcome::NoFrame && !quit_requested } else if port { pocket_ui::fallback::port_never_started(secs, quit_requested) || pocket_ui::ports::trimui_standalone() && !ok && !quit_requested } else { pocket_ui::fallback::never_started(ok, secs, quit_requested) };
        if never_started && port {
            self.notice = Some(Notice::new(pocket_ui::fallback::port_not_started_notice(&run.name)));
        } else if never_started && run.system == pocket_ui::android_games::SYSTEM_ID {
            // An Android game's app that went: the shelf loses it with Android's next list (`app_android_games`).
            self.notice = Some(Notice::new(pocket_ui::android_games::not_started(&run.name)));
            self.android_games_refresh("an app didn't start");
        } else if never_started && !run.hosted() {
            // A system whose BIOS pocket-ui knows: the files missing are named, not guessed at (`hostsys::missing_bios`).
            let bios = pocket_ui::hostsys::find(&run.system).and_then(|s| s.missing_bios_for(&pocket_ui::hostsys::bios_dir(), &run.path));
            self.notice = Some(Notice::new(pocket_ui::fallback::not_started_notice_on(&run.system, &run.name, bios.as_deref())));
        }
        // A confirmed failed start without pocket-core moments may try the system emulator once.
        // Its core file missing is no fault of the game: no retry with another emulator (there may be none: "emulator , core ", then a
        // message about a damaged file), the emulator is named instead.
        let core_gone = if run.hosted() && !quit_requested { self.notice.as_ref().and_then(|n| pocket_ui::fallback::missing_core_in(n.text())) } else { None };
        let failed = run.hosted() && core_gone.is_none() && !moments && pocket_ui::fallback::should_retry(ok, start, quit_requested, self.notice.as_ref().map(|n| n.text()));
        // Kids Mode has no next choice: RetroArch's menu is no place for a child, and the failure isn't remembered for the normal shelf.
        // An unknown start or a crash after a picture still needs the child's safe stop screen. That is
        // independent of whether the normal shelf would be allowed to retry a confirmed start failure.
        let kids_stopped = run.hosted() && !ok && !quit_requested && self.kids_live();
        let failed = failed && !kids_stopped;
        if kids_stopped {
            println!("kids mode: stopped, pocket-core couldn't start {} (exit ok {ok}, {secs} s), no fallback here", run.path.display());
            self.kids_stuck();
        }
        // The immediate retry uses the next choice down (`emu_choice::resolve`); this choice is temporary.
        let default = if failed { self.fallback.add(&run.path); self.systems.iter().find(|s| s.name == run.system).map(|s| self.route(s, Some(&run.path), true).0).unwrap_or_default() } else { String::new() };
        // A system with no other emulator than pocket-core (its core is missing): the next choice is pocket-core again, which
        // failed again, for ever (1,483 starts in two minutes, 2026-10-29). Say what happened instead.
        let retry = failed && pocket_ui::fallback::retry_with(&default);
        if retry {
            println!("pocket-core couldn't start {} (exit ok {ok}, {secs} s): {default} for this launch", run.path.display());
            self.notice = Some(Notice::new(pocket_ui::fallback::notice(&default)));
        } else if failed {
            self.fallback.remove(&run.path);
            println!("pocket-core couldn't start {} (exit ok {ok}, {secs} s): no other emulator for {}", run.path.display(), run.system);
            if self.notice.is_none() {
                // No other emulator on this system at all (the Brick): not RetroArch's core downloader.
                // pocket-core again (the OS images): its core crashed; a missing one says so itself (`core_gone`).
                self.notice = Some(Notice::new(pocket_ui::fallback::no_retry_notice(&run.system, &run.name, &default)));
            }
        }
        if let Some(core) = core_gone {
            println!("pocket-core couldn't start {}: the {core} core is missing", run.path.display());
            self.notice = Some(Notice::new({ pocket_ui::fallback::core_missing_here(&run.name, &core) }));
        }
        pocket_ui::trace::step("exit notices ready");
        // Cozy's way back (`eject`): Quit in the pause menu, and the picture it showed, before the switcher keeps it.
        // Taken over, not copied: the switcher's keep gets the same picture (`Arc`).
        let cozy = self.cozy();
        let quit_frame = self.menu.as_mut().filter(|m| m.quitting).map(|m| m.frame.take().map(std::sync::Arc::new));
        self.keep_last_frame(&run, quit_frame.as_ref().and_then(|q| q.clone()));
        let eject = quit_frame.filter(|_| cozy);
        // Written to the card at the end, after a game chosen in the switcher has been started (each syncs).
        // Not for a game that never started: it would be "Continue" on the shelf and the first card of the switcher.
        if !never_started {
            self.recents.note(&run.system, &run.name, &run.path, secs);
            self.frozen_session_ended(&run);
            // Play history: the session's end, as the recent games count it; written off this thread (`play_log`).
            pocket_ui::play_log::ended(&run.system, &run.name, &run.path, secs);
        } else {
            // Its start is in the play log too: closed with no time, it is no session (left open, it counted as one
            // "without time" in Curator → History).
            pocket_ui::play_log::never_started(&run.system, &run.name, &run.path);
        }
        pocket_ui::trace::step("exit frozen ready");
        let session = !never_started && self.library.note_session(&run.path, secs);
        self.curator.touch();
        // Back on the shelf at once; Syncthing gets asked on a thread of its own (`app_sync.rs`). Not when the game
        // starts again to apply its options: its saves stay held until it really ends.
        if !retry && reapply.is_none() {
            self.sync_after_game(&run.path);
            // Pocket-to-Pocket sync: the saves go to the paired Pockets (`app_pocket_sync.rs`).
            self.pocket_sync_after_game();
        }
        if run.hosted() {
            self.restore_system_then_fan();
        }
        // Back on the shelf (or the next game's start says otherwise right after): our display again.
        pocket_ui::mako_osd::show_shelf();
        pocket_ui::trace::step("exit shelf state ready");
        standalone::exited();
        self.launch_at = None;
        self.end_insert();
        self.launch_picture = None;
        self.where_was_i = None;
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
        pocket_ui::trace::step("exit focus ready");
        // Stick movements were thinned out while the game ran (`idle::StickThrottle`); the controllers stay known.
        self.pad.let_go();
        // A completed scan can go in before the first picture of the shelf.
        self.library_apply_pending();
        self.rebuild();
        pocket_ui::trace::step("exit rebuild ready");
        // A start that failed stays on the screen until it is read, in Easy.
        if never_started && !retry {
            self.big_note_failure(&run.path, &run.system, &run.name);
        }
        // Kids Mode's shelf stays on the game just played, so A brings the moment back; the Big shelf too (`rebuild`).
        if self.screen == Screen::Home && !self.kids_live() && !self.big_live() {
            self.sel = Item::first_row(&self.items);
            self.prev_sel = self.sel;
        }
        // The shelf is live now; the way back only paints over it.
        if let Some(frame) = eject.filter(|_| !never_started && !retry && reapply.is_none()) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            self.album_since = Some(now - secs as i64 - 60);
            self.begin_eject(&run.path, &run.system, frame);
        }
        // A start that failed for good: the shelf stands on the game again, and in Cozy with Motion Full its medium comes back
        // out and the case goes back on the shelf before the sentence (`begin_failed_start`).
        if never_started && !retry && reapply.is_none() {
            self.begin_failed_start(&run.path, &run.system, &run.name, ran);
        }
        if retry {
            let path = run.path.clone();
            self.launch(run.system, run.name, run.path);
            self.fallback.remove(&path);
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
        #[cfg(feature = "brick")]
        if pressed && matches!(button, Button::F1 | Button::F2) {
            if self.menu.is_none() {
                self.brick_fkey(button, true);
            }
            return;
        }
        if self.switcher_in_game_input(button, pressed) {
            return;
        }
        // The Players page needs to know which controller pressed (`app_players`).
        if pressed && self.menu.as_ref().is_some_and(|m| m.page == Page::Players && !m.quitting) {
            self.players_input(button, source);
            return;
        }
        if self.menu.is_some() {
            if pressed && self.big_pause() {
                self.big_pause_press(button);
            } else if pressed && !self.big_back_from_more(button) {
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
        // HOME + X: Zoom, when Options → Shortcuts → Zoom is on (`app_zoom`).
        if pressed && self.zoom_key(button) {
            return;
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
        self.where_was_i = None;
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
            Some(StateSet::pocket_core(&slots::base_for_core(&run.system, stem, &run.core)))
        } else if standalone::active() {
            standalone::states()
        } else {
            None
        };
        // The slot used last is selected for Save (pocket-core remembers it; the others start on 1).
        let slot = if hosted { slots::current(&self.data_dir, &run.system, stem) } else { 0 };
        let (filled, loads) = states.as_ref().map_or((Vec::new(), Vec::new()), |st| (st.filled(), st.load_targets()));
        let slot = slot.min(filled.len().saturating_sub(1));
        let states_for_restart = states.is_some();
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
        // In Hardcore pocket-core's slots say why they aren't there (`achievements::hardcore`).
        let hardcore = achievements.as_ref().is_some_and(|a| a.hardcore);
        let saves = app_achievements::hardcore_saves(pocket_ui::pause_saves::Saves::of(saver, states.is_some(), no_states), hardcore);
        let cheats = cheats.filter(|_| !hardcore);
        if let Some(s) = settings.as_mut() {
            s.hardcore = hardcore;
        }
        let disc = Self::disc_row_at_open(run);
        // Earlier: a few stats of known names (`app_earlier`).
        let earlier = app_earlier::row_at_open(run, saves, settings.as_ref());
        self.menu = Some(QuickMenu {
            sel: 0,
            frame,
            game_rect: None,
            saved,
            states,
            slot,
            filled,
            loads,
            load_at,
            restart: hosted && states_for_restart,
            quitting: false,
            discarding: false,
            // Hardcore: Quit keeps no moment, since no start would continue from it.
            without_saving: quit_leaves_out_resume(self.remember_quit, saver) || hardcore,
            window,
            frozen,
            retroarch,
            hosted,
            page: Page::Main,
            settings,
            note: None,
            restore_armed: false,
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
            zoom: None,
            zoom_row: self.zoom_row_on(),
            zoom_back: app_zoom::Back::Main,
            zoom_pending: false,
            saves,
            disc,
            earlier,
            save_trouble: Self::save_trouble_at_open(hosted),
            saving: None,
            hardcore_ask: false,
            shortcut_list: Vec::new(),
        });
        self.menu_shot = app_menu_open::MenuShot::asked(shot);
        // The battery in this game, from memory (`battery_games`): microseconds, no file or sysfs read.
        self.menu_battery = self.running.as_ref().and_then(|r| {
            let key = pocket_ui::battery_games::Key::new(&r.system, &r.path, &r.emulator);
            pocket_ui::battery_history::game_left(&key).map(|l| l.line(&system_short(&self.systems, &r.system)))
        });
        self.big_menu_opened();
        pocket_ui::trace::step("menu: built");
    }

    fn menu_input(&mut self, b: Button) {
        // Cozy with Motion Full: Save shows the slot filling before the game goes on (`app_save_media`).
        let fill_on = self.save_fill_on();
        if self.menu.as_ref().is_some_and(|m| m.page == Page::Controls { sel: m.controller_setup_row() }) && (b == CONFIRM || b == Button::Right) {
            self.open_controllers(true);
            return;
        }
        let Some(menu) = self.menu.as_mut() else { return };
        if menu.quitting {
            return;
        }
        if menu.page == Page::LoadBlocked || matches!(menu.page, Page::RetroArchStateUnconfirmed(_)) {
            if b == CONFIRM || b == CANCEL { menu.page = Page::Main; }
            return;
        }
        if menu.page == Page::PixelPath {
            self.pixel_path_input(b);
            return;
        }
        if menu.page == Page::Zoom {
            self.zoom_input(b);
            return;
        }
        if menu.page == Page::EmuChoice {
            self.emu_choice_input(b);
            return;
        }
        if matches!(menu.page, Page::ShortcutList { .. }) {
            self.shortcut_list_input(b);
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
        // Options → Zoom (`app_zoom`): the page needs the App, B comes back to this row.
        if let Page::Options { sel } = menu.page {
            if b == CONFIRM && opt_entries(menu.emu.as_ref(), menu.cheats.is_some()).get(sel) == Some(&OptEntry::Zoom) {
                menu.note = None;
                self.open_zoom(app_zoom::Back::Options { sel });
                return;
            }
        }
        // The Achievements page's Hardcore row (`app_achievements`): A or Left/Right switch it (its hint says Change), on
        // asks first; any other button takes the question back.
        if matches!(menu.page, Page::Achievements { .. }) {
            if matches!(b, Button::Left | Button::Right) || b == CONFIRM {
                if menu.page == (Page::Achievements { sel: 0 }) && app_achievements::hardcore_row(menu).is_some() {
                    self.hardcore_pressed();
                    return;
                }
            }
            menu.hardcore_ask = false;
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
                        MainItem::Load => menu.load_stops() > 1,
                        MainItem::Save => menu.filled.len() > 1,
                        _ => false,
                    } =>
            {
                let delta = if b == Button::Left { -1 } else { 1 };
                if menu.item() == MainItem::Load {
                    menu.load_at = (menu.load_at as i32 + delta).rem_euclid(menu.load_stops().max(1) as i32) as usize;
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
            Button::Start if !self.pad.is_pressed(Button::Select) => self.open_quick_settings_over_game(),
            b if b == CONFIRM => match menu.item() {
                MainItem::Continue => self.resume_game(None),
                MainItem::Save if fill_on && app_save_media::fills(menu) => self.begin_save_fill(),
                MainItem::Save => self.resume_game(Some(SaveAction::Save)),
                // From the start, Load's last stop (pocket-core).
                MainItem::Load if menu.at_restart() => self.resume_game(Some(SaveAction::Restart)),
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
                MainItem::PixelPath => self.open_pixel_path(),
                MainItem::Players => self.open_players(),
                MainItem::Zoom => self.open_zoom(app_zoom::Back::Main),
                // The game's saves don't reach the card: Quit asks once first (`app_save_trouble`).
                MainItem::Quit if app_save_trouble::asks_before_quit(menu) => menu.page = Page::QuitUnsaved { sel: 0 },
                // RetroArch writes the auto state on quit, so Continue on Home picks up right here.
                MainItem::Quit if menu.retroarch && self.running.as_ref().is_some_and(|run| self.retroarch_state_pending == Some(run.started)) => {
                    pocket_ui::ui_sound::refuse();
                }
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

    /// Options screens: groups and the settings of one group. A change applies at once and is kept at once (`Keep for`). B
    /// goes one level up.
    fn options_input(menu: &mut QuickMenu, b: Button) {
        let entries = opt_entries(menu.emu.as_ref(), menu.cheats.is_some());
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
        // Restore defaults asks twice: only an A on it right after the first one does it.
        let was_armed = std::mem::take(&mut menu.restore_armed);
        if was_armed && !(matches!(menu.page, Page::Options { sel } if entries[sel] == OptEntry::Restore) && b == CONFIRM) {
            menu.note = None;
        }
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
            // Keep for: Left/Right or A moves the changes between this game and the whole console.
            (Page::Options { sel }, b) if (b == CONFIRM || matches!(b, Button::Left | Button::Right)) && entries[sel] == OptEntry::Keep => {
                menu.note = Some(match menu.settings.as_mut().map(|s| s.keep_elsewhere()) {
                    Some(Some(Level::Console)) => "Kept for this console.",
                    Some(Some(_)) => "Kept for this game.",
                    _ => {
                        pocket_ui::ui_sound::refuse();
                        "Couldn't save the settings."
                    }
                });
                Page::Options { sel }
            }
            (Page::Options { sel }, b) if b == CONFIRM && entries[sel] == OptEntry::Restore => {
                if !was_armed {
                    menu.restore_armed = true;
                    menu.note = Some(pocket_ui::pause_options::RESTORE_ASK);
                } else if let Some(s) = menu.settings.as_mut() {
                    s.restore_defaults();
                    if let Some(emu) = menu.emu.as_mut() {
                        emu.restored();
                    }
                    control::pocket_core("reload");
                    menu.note = Some(pocket_ui::pause_options::RESTORE_DONE);
                }
                Page::Options { sel }
            }
            (Page::Options { sel }, b) if b == CONFIRM => {
                menu.note = None;
                match entries[sel] {
                    OptEntry::Group(group) => {
                        // The Picture page works the Look out from the values in use each time it opens.
                        if let Some(s) = menu.settings.as_mut() {
                            s.close_custom();
                        }
                        Page::Group { group, sel: 0 }
                    }
                    OptEntry::Emulator => Page::Emulator { cat: None, sel: 0 },
                    OptEntry::Controls => Page::Controls { sel: 0 },
                    // Opened by `menu_input` (it needs the App).
                    OptEntry::Zoom => Page::Options { sel },
                    OptEntry::Cheats => {
                        if let Some(cheats) = &mut menu.cheats { cheats.open(); }
                        Page::Cheats { sel: 0 }
                    },
                    OptEntry::RunsOn => Page::EmuChoice,
                    OptEntry::Keep | OptEntry::Restore => Page::Options { sel },
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
                    // Applied to the running game right away and kept at once (`Keep for`). A Look is its two or
                    // three saved values (no "set look"); Custom changes none.
                    let changed = s.step(setting, if b == Button::Left { -1 } else { 1 });
                    for (key, value) in &changed {
                        control::pocket_core(&format!("set {key} {value}"));
                    }
                    if !changed.is_empty() {
                        s.keep();
                    }
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
                    None => {
                        settings.keep();
                        Page::Emulator { cat, sel }
                    }
                },
                _ => Page::Emulator { cat, sel },
            },
            (Page::Controls { .. }, b) if b == CANCEL => Page::Options { sel: at(OptEntry::Controls) },
            // The page's own rows first (`control_step` counts them from 0), Controller setup last (opened by `menu_input`).
            (Page::Controls { sel }, Button::Left | Button::Right) if sel == menu.controller_setup_row() => Page::Controls { sel },
            (Page::Controls { mut sel }, Button::Up | Button::Down) => {
                let n = menu.controller_setup_row() + 1;
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
                    settings.keep();
                }
                Page::Controls { sel }
            }
            // The game's cheats: A (or Left/Right) switches one, L1/R1 skip five.
            (Page::Cheats { .. }, b) if b == CANCEL => {
                let sel = at(OptEntry::Cheats);
                if let Some(cheats) = &mut menu.cheats { cheats.close(); }
                Page::Options { sel }
            },
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
                    if b == CONFIRM || !c.on_download_row(sel) { c.press(sel); }
                }
                Page::Cheats { sel }
            }
            // The game's achievements: a list to read; B goes back to the menu.
            (Page::Achievements { .. }, b) if b == CANCEL => {
                menu.sel = menu.items().iter().position(|i| *i == MainItem::Achievements).unwrap_or(0);
                Page::Main
            }
            (Page::Achievements { mut sel }, Button::Up | Button::Down | Button::L1 | Button::R1) => {
                let n = app_achievements::menu_page_rows(menu).len().max(1);
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
            (page, _) => page,
        };
    }

    fn resume_game(&mut self, action: Option<SaveAction>) {
        pocket_ui::trace::step("continue begin");
        use pocket_ui::ra_state::Action as RetroArchAction;
        let retroarch_action = match action {
            Some(SaveAction::Save) => Some(RetroArchAction::Save),
            Some(SaveAction::Load(_)) => Some(RetroArchAction::Load),
            _ => None,
        }.filter(|_| self.menu.as_ref().is_some_and(|menu| menu.retroarch)
            && !pocket_ui::device::darkos_port() && !pocket_ui::device::android_profile());
        if retroarch_action.is_some() && self.running.as_ref().is_some_and(|run| self.retroarch_state_pending == Some(run.started)) {
            pocket_ui::ui_sound::refuse();
            return;
        }
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
                Some(SaveAction::Load(target)) => {
                    let proxy = self.proxy.clone();
                    standalone::load(target, menu.frame, move |id| { let _ = proxy.send_event(UserEvent::LoadBlocked(id)); });
                }
                // Earlier is pocket-core's only.
                Some(SaveAction::Earlier | SaveAction::Restart) | None => {}
            }
        } else if retroarch_action.is_none() {
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
                (Some(SaveAction::Restart), true) => control::pocket_core("restart"),
                (Some(SaveAction::Save), false) => control::retroarch("SAVE_STATE"),
                (Some(SaveAction::Load(_)), false) => control::retroarch("LOAD_STATE"),
                (Some(SaveAction::Earlier | SaveAction::Restart), false) => true,
                (None, _) => true,
            };
        }
        match menu.window {
            Some(con) => control::show_window(con),
            None => control::show_game(),
        };
        if let (Some(action), Some(run)) = (retroarch_action, self.running.as_ref()) {
            let (started, pgid, state) = (run.started, run.pgid, run.state_file());
            let proxy = self.proxy.clone();
            self.retroarch_state_pending = Some(started);
            std::thread::spawn(move || {
                let confirmed = pgid.is_some_and(|pgid| pocket_ui::ra_state::check(action, &state, pgid));
                let _ = proxy.send_event(UserEvent::RetroArchStateChecked { started, action, confirmed });
            });
        }
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
        Some(pocket_ui::pause_page::picture_page(&mut self.text, w, h, max_rows, &g.descs()))
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

    /// NextUI-style pause menu: the paused game dimmed behind, the game's name as a plain title, the items
    /// as a vertical list with the selected one in a pill, a save preview on the right, button hints
    /// at the bottom right.
    fn paint_menu(&mut self, c: &mut Canvas) {
        // A wide screen's pages of rows are larger (`type_scale::wide_page`), as Settings' are.
        // Pixel Path is a drawing of fixed rows and dots: its type as on any other screen.
        let pixel_path = self.menu.as_ref().is_some_and(|m| m.page == Page::PixelPath);
        let _wide = pocket_ui::type_scale::panel(if pixel_path { 1.0 } else { pocket_ui::type_scale::wide_page(c.w, c.h) });
        self.paint_menu_page(c);
    }

    fn paint_menu_page(&mut self, c: &mut Canvas) {
        // Zoom fills the canvas: skip the full-screen dimmed background copy on its redraws.
        if self.menu.as_ref().is_some_and(|m| m.page == Page::Zoom) {
            self.paint_zoom(c);
            return;
        }
        // The game's picture, darkened (made once per picture, `app_menu_open`); a little more behind Pixel Path's text.
        let fade = if self.menu.as_ref().is_some_and(|m| m.page == Page::PixelPath) { app_pixel_path::PAGE_FADE } else { pocket_ui::eject::PAUSE_FADE };
        self.paint_game_behind(c, VOID, fade);
        let Some(menu) = self.menu.as_ref() else { return };
        let state_notice = match menu.page {
            Page::LoadBlocked => Some(pocket_ui::save_trouble::UNDO_FAILED),
            Page::RetroArchStateUnconfirmed(action) => Some(action.unconfirmed()),
            _ => None,
        };
        if let Some(message) = state_notice {
            Notice::new(message).paint(c, &mut self.text);
            self.nu_hints(c, &[("A", "Back"), ("B", "Back")]);
            return;
        }
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
        if matches!(menu.page, Page::ShortcutList { .. }) {
            self.paint_shortcut_list(c);
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
            (img.is_some(), recents::when(secs))
        });
        let (sel, quitting, discarding, without_saving, items) = (menu.sel, menu.quitting, menu.discarding, menu.without_saving, menu.items());
        // Save is being shown: the slot holds the save already, so the note says "saved", not "replaces" (`save_fill`).
        let filling = menu.saving.is_some();
        let menu_loads = menu.loads.clone();
        let item = items.get(sel).copied().unwrap_or(MainItem::Continue);
        let has_states = items.contains(&MainItem::Save);
        let quit_saves = quit_saves(menu.without_saving, menu.saves);
        let (applying, emu_note) = (menu.applying, menu.emu_note.clone());
        // Where Save and Load can't do anything: the line that says where saving is (`pause_saves`); the Disc row.
        let (saves_line, disc) = (menu.saves.line(), menu.disc);
        // States with slots (`states.rs`): what each row shows, and whether Left/Right step through it.
        let slotted = menu.states.is_some();
        let (slot, filled) = (menu.slot, menu.filled.clone());
        let save_target = Target::Slot(slot as u8 + 1);
        let load_target = menu.loads.get(menu.load_at).copied();
        let (loads_len, slots_len) = (menu.load_stops(), filled.len());
        let at_restart = menu.at_restart();
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

        // What the page shows, gathered here; painted by `pause_page::paint_main`, the painter Android's player uses too.
        use pocket_ui::pause_page::{MainPage, MainRow, Window};
        let load_chips: Vec<String> = menu_loads.iter().map(|t| states::chip(*t)).collect();
        let rows: Vec<MainRow> = items
            .iter()
            .enumerate()
            .map(|(i, &row)| {
                let enabled = match row {
                    // With Earlier chosen, the preview is Earlier's: Load is there when anything can be loaded.
                    MainItem::Load => if slotted { anything_to_load || at_restart } else { saved.is_some() },
                    MainItem::Earlier => earlier.as_ref().is_some_and(|e| e.2),
                    _ => true,
                };
                // Save and Load show their state: the slot ("3" at rest, "‹ 3 ›" when selected), or on Load also "Auto"
                // and "Undo"; Disc the disc chosen ("1 of 3", "‹ 2 of 3 ›").
                let disc_chip = disc.filter(|_| row == MainItem::Disc).map(|d| d.chip());
                let earlier_chip = earlier.as_ref().filter(|_| row == MainItem::Earlier).and_then(|e| e.0.clone());
                let chip = (slotted && matches!(row, MainItem::Save | MainItem::Load))
                    .then(|| if row == MainItem::Load { load_target } else { Some(save_target) })
                    .flatten()
                    .map(states::chip)
                    .or_else(|| (row == MainItem::Load && at_restart).then(|| "From the start".to_string()))
                    .or(disc_chip)
                    .or(earlier_chip);
                // Retro's rows' window is as wide as the widest value a row can show (Load: the longest of its stops,
                // so it doesn't jump while stepping).
                let widest = match row {
                    MainItem::Save if slotted => Some(states::chip(save_target)),
                    MainItem::Load if slotted => load_chips.iter().max_by_key(|t| t.chars().count()).cloned(),
                    MainItem::Disc => disc.map(|d| d.chip()),
                    MainItem::Earlier => earlier.as_ref().and_then(|e| e.0.clone()),
                    _ => None,
                };
                // Quit says what it does: it saves the moment unless Continue where I quit is off (or there is nothing to save).
                MainRow { label: row.shown(quit_saves).to_string(), icon: retro_icon(row), enabled, opens: row.opens_page(), chip, widest, arrows: i == sel && (steps(row) || row == MainItem::Disc) }
            })
            .collect();
        // Right: a dark window, half the screen, with the save's preview or a message, the slots and a line under it.
        let window_note = (!quitting && matches!(item, MainItem::Save | MainItem::Load | MainItem::Earlier)).then(|| {
            let ago = saved.as_ref().map(|(_, ago)| ago.as_str());
            let shown = if item == MainItem::Load { load_target } else { Some(save_target) };
            match (slotted, shown, ago, item == MainItem::Save && !filling) {
                _ if item == MainItem::Earlier => earlier.as_ref().map(|e| e.1.clone()).unwrap_or_default(),
                _ if item == MainItem::Load && at_restart => "Starts over · Quit replaces it".to_string(),
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
            }
        });
        // The slot the row is on (Load may be on Auto or Before load: no slot is ringed then).
        let on_slot = match (item, load_target) {
            (MainItem::Load, Some(Target::Slot(n))) => Some(n as usize - 1),
            (MainItem::Load, _) => None,
            _ => Some(slot),
        };
        let quit_msg = quitting.then_some(if applying {
            "Applying…"
        } else if discarding || !has_states {
            "Quitting…"
        } else {
            "Saving and quitting…"
        });
        // The emulator saves only inside the game (`pause_saves`): said where the player decides, beside Continue,
        // Options and Quit, instead of Save and Load rows that would do nothing.
        let note = match item {
            MainItem::Quit if saves_line.is_some() => saves_line.map(str::to_string),
            MainItem::Quit if without_saving => Some("Doesn't save where you are. In-game saves are kept.".to_string()),
            MainItem::Quit if quit_saves => Some("Next time, A continues from this moment.".to_string()),
            MainItem::Continue | MainItem::Options => saves_line.map(str::to_string),
            MainItem::Disc => disc.map(|d| d.note()),
            MainItem::PixelPath => Some("How the picture and sound reach you, stage by stage.".to_string()),
            MainItem::Players => Some("Which controller is which player.".to_string()),
            MainItem::Zoom => Some("Zoom in to read tiny text. Pan around with the D-pad.".to_string()),
            _ => None,
        };
        // Options that couldn't be applied say so instead (not over Save's and Load's preview).
        let note = emu_note.filter(|_| !matches!(item, MainItem::Save | MainItem::Load | MainItem::Earlier)).or(note);
        // Continue, Options and Save & quit: the paused game in the window, where Save's stands (`pause_page::shows_game`);
        // the row's own line goes under it, else how long the game was played today.
        let row_kind = match item {
            MainItem::Continue => pocket_ui::pause_page::RowKind::Continue,
            MainItem::Options => pocket_ui::pause_page::RowKind::Options,
            MainItem::Quit => pocket_ui::pause_page::RowKind::Quit,
            _ => pocket_ui::pause_page::RowKind::Other,
        };
        let live = pocket_ui::pause_page::shows_game(row_kind, quitting, self.menu.as_ref().is_some_and(|m| m.frame.is_some()));
        let (window_note, note) = if live {
            let line = note.unwrap_or_else(|| {
                let session = self.running.as_ref().map_or(0, |r| r.started.elapsed().as_secs());
                let today = self.running.as_ref().zip(pocket_ui::play_log::view()).map(|(r, log)| {
                    let now = pocket_ui::play_time::now_secs();
                    pocket_ui::play_history::played_today(&log, &r.path.to_string_lossy(), session, now, pocket_ui::appearance::utc_offset_now(now as i64))
                });
                pocket_ui::pause_page::paused_here(today, session)
            });
            (Some(line), None)
        } else {
            (window_note, note)
        };
        let step = if item == MainItem::Disc {
            Some("Disc")
        } else if slotted && steps(item) && matches!(item, MainItem::Save | MainItem::Load | MainItem::Earlier) {
            Some(if item == MainItem::Save { "Slot" } else { "State" })
        } else {
            None
        };
        // Easy has no game switcher: nothing here names SELECT.
        let hints = pocket_ui::pause_page::main_hints(step, item.opens_page(), !self.big_on(), true);
        // Under the picture the slots (`app_save_media`): the slim row in the dots' 28 px, or with "Memory card in the
        // pause menu" the medium, for which the picture gives up its height.
        let kind = self.running.as_ref().map_or(pocket_ui::save_media::Kind::List, |r| pocket_ui::save_media::Kind::of(&r.system));
        let strip_h = |w: i32| app_save_media::strip_h(kind, w);
        let trouble = self.menu.as_ref().and_then(|m| m.save_trouble).map(|t| t.line());
        let battery = self.menu_battery.clone();
        let empty = match (&saved, item) {
            (Some(_), _) => "No picture",
            (None, MainItem::Earlier) => "None yet",
            (None, MainItem::Load) if at_restart => "From the start",
            (None, _) => "Empty",
        };
        // Earlier has no slots: nothing under its picture.
        let slots = slotted && filled.len() > 1 && item != MainItem::Earlier;
        // The page as painted, with the save's picture from the menu: built again after the slots, which need `self`.
        macro_rules! page {
            () => {
                MainPage {
                    title: &title,
                    battery: battery.as_deref(),
                    trouble,
                    rows: &rows,
                    sel,
                    window: window_note.clone().map(|note| Window {
                        picture: if live { self.menu.as_ref().and_then(|m| m.frame.as_ref()) } else { self.menu.as_ref().and_then(|m| m.saved.as_ref()).and_then(|(i, _)| i.as_ref()) },
                        empty,
                        note,
                        slots: slots && !live,
                        strip_h: &strip_h,
                    }),
                    quitting: quit_msg,
                    note: note.clone(),
                    hints: &hints,
                }
            };
        }
        let out = pocket_ui::pause_page::paint_main(c, &mut self.text, &page!());
        if let Some(at) = out.slots {
            if app_save_media::pause_memory_card() {
                self.paint_save_strip(c, at.strip.0, at.strip.1, at.strip.2, &filled, on_slot, out.picture);
            } else {
                self.paint_save_row(c, at.row.0, at.row.1, &filled, on_slot, out.picture);
            }
        }
        pocket_ui::pause_page::finish_main(c, &mut self.text, &mut self.hits, &page!(), &out);
        if quitting {
            self.paint_saving_wait(c);
        }
    }

    /// Options screens in NextUI's layout: a list of rows with the value at the right; the selected
    /// row is one sheet of paper with the game's system colour as a short stripe at its start. The
    /// selected entry's explanation sits at the bottom left beside the hint pill (`kit::footer`), the status pill at
    /// the top right.
    fn paint_options(&mut self, c: &mut Canvas) {
        let Some(menu) = self.menu.as_ref() else { return };
        let accent = self.running.as_ref().map_or(DUST, |r| stripe(&r.system));
        // (name, value or ">" for a submenu, explanation), the selected row, a confirmation replacing the
        // explanation, and a heading for pages below the options list
        let (rows, sel, footer, heading): (Vec<(String, String, String)>, usize, Option<String>, Option<String>) = match menu.page {
            Page::Options { sel } => {
                let rows = opt_entries(menu.emu.as_ref(), menu.cheats.is_some())
                    .into_iter()
                    .map(|e| match e {
                        OptEntry::Group(g) => (options::GROUPS[g].name.to_string(), ">".to_string(), options::GROUPS[g].desc_here().to_string()),
                        OptEntry::Emulator => ("Emulator".into(), ">".into(), "Core-specific options, like palettes and hardware tweaks.".into()),
                        OptEntry::Controls => ("Controls".into(), ">".into(), "Which button does what, and the left stick.".into()),
                        OptEntry::Zoom => ("Zoom".into(), ">".into(), "Zoom in to read tiny text. Pan around with the D-pad.".into()),
                        OptEntry::Cheats => {
                            let on = menu.cheats.as_ref().map_or(0, |c| c.enabled());
                            // A page's row ends in its angle with a value too ("2 on ›"), as Settings' "Off ›".
                            ("Cheats".into(), if on > 0 { format!("{on} on >") } else { ">".into() }, "Infinite lives and more, from the libretro cheat database.".into())
                        }
                        OptEntry::RunsOn => ("Runs on".into(), ">".into(), "Which emulator plays this game or this console: pocket-core, RetroArch or another.".into()),
                        OptEntry::Keep => {
                            let (value, desc) = match menu.settings.as_ref().map(|s| s.level) {
                                Some(Level::Console) => ("This console", "Changes show at once and stay for every game on this console. Left or right: this game only."),
                                _ => ("This game", "Changes show at once and stay for this game. Left or right: every game on this console."),
                            };
                            ("Keep for".into(), value.into(), desc.into())
                        }
                        OptEntry::Restore => ("Restore defaults".into(), String::new(), pocket_ui::pause_options::RESTORE_DESC.into()),
                    })
                    .collect();
                (rows, sel, menu.note.map(str::to_string), Some("Options".to_string()))
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
                let setup_desc = "A separate layout for each controller, in menus and games.";
                // On the Brick, F1 and F2 are no game buttons: where they are set instead.
                #[cfg(feature = "brick")]
                let setup_desc = if pocket_ui::brick_profile::fkeys() { pocket_ui::brick_hw::FKEYS_REMAP_NOTE } else { setup_desc };
                let mut lines: Vec<(String, String, String)> = rows.into_iter().map(|Row { name, value, desc }| (name, value, desc)).collect();
                lines.push(("Controller setup".into(), ">".into(), setup_desc.into()));
                (lines, sel, None, Some("Controls".to_string()))
            }
            Page::Cheats { sel } => {
                let rows = menu.cheats.as_ref().map(|c| c.rows()).unwrap_or_default();
                let rows = rows.into_iter().map(|Row { name, value, desc }| (name, value, desc)).collect();
                (rows, sel, None, Some("Cheats".to_string()))
            }
            #[cfg(not(feature = "brick"))]
            Page::Group { group, sel } => {
                let g = &options::GROUPS[group];
                let rows = g
                    .visible(menu.settings.as_ref())
                    .into_iter()
                    .map(|s| {
                        let i = menu.settings.as_ref().map_or(s.default, |v| v.index(s));
                        (s.name.to_string(), s.choices[i].label.to_string(), options::describe(s, menu.settings.as_ref()))
                    })
                    .collect();
                (rows, sel, None, Some(format!("Options \u{203a} {}", g.name)))
            }
            #[cfg(feature = "brick")]
            Page::Group { group, sel } => {
                let g = &options::GROUPS[group];
                let rows = g
                    .visible(menu.settings.as_ref())
                    .into_iter()
                    .map(|s| {
                        let i = menu.settings.as_ref().map_or(s.default, |v| v.index(s));
                        let label = pocket_ui::brick_profile::labelled(s.key, s.choices[i].value, s.choices[i].label);
                        (s.name.to_string(), label, options::describe(s, menu.settings.as_ref()))
                    })
                    .collect();
                (rows, sel, None, Some(format!("Options \u{203a} {}", g.name)))
            }
            Page::Standalone { sel } => {
                let set = menu.emu_set.as_ref();
                let rows = set.map(|e| e.rows()).unwrap_or_default();
                // Once something changed, how it will be applied, under the row's explanation.
                let footer = set.and_then(|e| e.note()).and_then(|n| rows.get(sel).map(|r| format!("{}\n{n}", r.2)));
                (rows, sel, footer, set.map(|e| format!("Options   {}", e.title())))
            }
            Page::Achievements { sel } => {
                let heading = "Achievements".to_string();
                (app_achievements::menu_page_rows(menu), sel, None, Some(heading))
            }
            Page::QuitUnsaved { sel } => {
                let rows = menu.save_trouble.map(Self::quit_question_rows).unwrap_or_default();
                (rows, sel, None, Some(pocket_ui::save_trouble::QUESTION.to_string()))
            }
            Page::Main | Page::LoadBlocked | Page::RetroArchStateUnconfirmed(_) | Page::PixelPath | Page::EmuChoice | Page::Players | Page::Zoom | Page::PortPerf | Page::ShortcutList { .. } => return,
        };

        let achievement_counts = if matches!(menu.page, Page::Achievements { .. }) {
            menu.achievements.as_ref().filter(|s| s.state == pocket_ui::achievements::snapshot::State::Ready && s.total > 0).map(|s| s.headline())
        } else {
            None
        };
        // Sharpness and Effect stand a step in under the Look (Custom).
        let indents: Vec<bool> = match (menu.page, menu.settings.as_ref()) {
            (Page::Group { group, .. }, values) => options::GROUPS[group].visible(values).into_iter().map(options::indented).collect(),
            _ => Vec::new(),
        };
        // A cheat's line that only explains has no value to change, and B is its only hint.
        let cheats = matches!(menu.page, Page::Cheats { .. }).then_some(menu.cheats.as_ref());
        let actionable = |i: usize| cheats.is_none_or(|c| c.is_some_and(|c| c.actionable(i)));
        let explanation = cheats.is_some_and(|c| c.is_some_and(|c| !c.actionable(sel)));
        let rows: Vec<pocket_ui::pause_page::OptRow> = rows
            .into_iter()
            .enumerate()
            .map(|(i, (name, value, desc))| pocket_ui::pause_page::OptRow { name, value, desc, indent: indents.get(i).copied().unwrap_or(false), fixed: !actionable(i) })
            .collect();
        // Painted by `pause_page::paint_options`, the painter Android's player uses too; the Picture page with its preview.
        let description_above_hints = matches!(menu.page, Page::Emulator { .. });
        let corner = self.status_corner();
        let picture = self.picture_page(c.w, c.h);
        let page = pocket_ui::pause_page::OptionsPage {
            heading: heading.as_deref(),
            counts: achievement_counts.as_deref(),
            rows: &rows,
            sel,
            footer: footer.as_deref(),
            description_above_hints,
            explanation,
            accent,
            corner: &corner,
            picture: picture.zip(self.menu.as_ref()).map(|((rows, layout), m)| pocket_ui::pause_page::PicturePreview { rows, layout, live: &m.live }),
        };
        pocket_ui::pause_page::paint_options(c, &mut self.text, &mut self.hits, &page);
    }

    /// The status corner (time, Wi-Fi while connected, battery) in a pill at the top right, from memory like Home's
    /// (`app_status_corner`); returns the pill's left edge.
    fn status_pill(&mut self, c: &mut Canvas) -> i32 {
        let corner = self.status_corner();
        pocket_ui::pause_page::status_pill(c, &mut self.text, &corner)
    }

    /// The top line of a page of the Settings family (docs/DESIGN.md "Design language"): the status corner in its pill
    /// at the right and the page's title in Title 50 at the left, cut with "…" 24 px short of the pill (a long German
    /// title with Large text). Returns the pill's left edge.
    fn sheet_header(&mut self, c: &mut Canvas, title: &str) -> i32 {
        let pill_left = self.status_pill(c);
        let title = self.text.fit(Face::Title, 50, title, pill_left - 24 - (NU_PAD + NU_BTN_PAD));
        self.text.draw(c, Face::Title, 50, NU_PAD + NU_BTN_PAD, pocket_ui::kit::header_title(c.h), pal().chalk, &title);
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

    /// `nu_hints` and the selected row's line at the bottom left in `color`: a line that doesn't fit beside the pill
    /// goes on a second line next to it (`kit::footer`), not into a "…".
    fn nu_foot(&mut self, c: &mut Canvas, hints: &[(&str, &str)], desc: &str, color: u32) {
        pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, hints, desc, 2, color);
    }

    /// (system id, ROM path) of every game in the library.
    fn all_games(&self) -> Vec<(String, PathBuf)> {
        self.systems.iter().flat_map(|s| s.games.iter().map(|g| (s.name.clone(), g.path.clone()))).collect()
    }

    fn item_rom(&self, item: Item) -> Option<(String, PathBuf)> {
        match item {
            Item::Recent(i) => self.recents.list.get(i).map(|r| (r.system.clone(), r.path.clone())),
            Item::Game(s, g) => Some((self.systems[s].name.clone(), self.systems[s].games[g].path.clone())),
            Item::System(_) | Item::Favorites | Item::Streaming | Item::Search | Item::JustPlay | Item::Crate | Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover => None,
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
        let mut candidates = vec![pocket_ui::covers::active_cover_path(&self.data_dir, &system, &rom)];
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
        self.setup_with_catalog(library::all_systems(&self.es_systems).unwrap_or_default())
    }

    /// Extras has no systems grid: opening it must not read the card's system catalogue on the UI thread.
    fn new_extras_setup(&self) -> Setup { self.setup_with_catalog(Vec::new()) }

    fn setup_with_catalog(&self, catalog: Vec<library::SysInfo>) -> Setup {
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
        .with_catalog(catalog)
            .with_lived_in(self.lived_in)
        .with_switches(self.switches(), self.tour.later.clone())
    }

    /// Every system with games, for Settings > Games > Systems on the shelf.
    fn shelf_systems(&self) -> Vec<pocket_ui::setup::ShelfSystem> {
        self.systems
            .iter()
            .map(|s| pocket_ui::setup::ShelfSystem { id: s.name.clone(), name: s.fullname.clone(), games: s.games.len() })
            .collect()
    }

    /// Settings pages that hold nothing in progress and nothing unsaved (their switches take effect at once): HOME leaves
    /// them for Home. The others (a download or a sign-in under way, a pairing, a button being remapped, a clean-up) keep
    /// HOME to themselves, as before: B goes back there, and a page that asks first still asks.
    fn settings_page_lets_home_go(&self) -> bool {
        page_lets_home_go(self.settings_page) && self.setup.is_none() && !self.tour.is_open()
    }

    fn settings_input(&mut self, b: Button) {
        // HOME on a Settings page goes to Home, from wherever in Settings you got lost (Quick settings stay on Home and the shelf).
        if b == Button::Mode {
            if self.settings_page_lets_home_go() {
                self.settings_page = None;
                self.sys.cancel_home_tap();
            }
            return;
        }
        self.settings_page = match (self.settings_page, b) {
            // Moving in a category's list, L1/R1 to the next category, B out of Settings (`app_settings_groups`).
            (Some(page @ SettingsPage::List(_)), Button::Up | Button::Down | Button::L1 | Button::R1) => app_settings_groups::groups_input(page, b),
            (Some(page @ SettingsPage::List(_)), b) if b == CANCEL => app_settings_groups::groups_input(page, b),
            // The focus on the categories at the top: every key is theirs (`app_settings_groups`).
            (Some(page @ SettingsPage::Tabs(_)), b) => app_settings_groups::groups_input(page, b),
            (Some(SettingsPage::List(sel)), b) if b == CONFIRM => match SETTINGS[sel].1 {
                Opens::Setup(step) => {
                    self.setup = Some(if step == Only::Extras { self.new_extras_setup() } else { self.new_setup() }.only(step));
                    Some(SettingsPage::List(sel))
                }
                Opens::Colours => {
                    self.step_colours(true);
                    Some(SettingsPage::List(sel))
                }
                Opens::Font => {
                    self.step_font(true);
                    Some(SettingsPage::List(sel))
                }
                Opens::ShelfSystems => {
                    self.setup = Some(self.new_setup().only(Only::Look).at_shelf_systems());
                    Some(SettingsPage::List(sel))
                }
                Opens::Appearance => {
                    self.step_appearance(true);
                    Some(SettingsPage::List(sel))
                }
                Opens::Motion => {
                    self.step_motion(true);
                    Some(SettingsPage::List(sel))
                }
                Opens::Time => {
                    self.sys.open_time(Instant::now());
                    Some(SettingsPage::List(sel))
                }
                Opens::Updates => Some(self.open_updates()),
                Opens::GameCheck => self.open_game_check(),
                Opens::Sync => self.open_sync(),
                Opens::Pockets => self.open_pocket_sync(),
                Opens::Leds => self.open_leds(),
                Opens::NightLight => self.open_night_light(),
                Opens::CozyExtras => self.open_cozy_extras(),
                Opens::Tailscale => self.open_tailscale(),
                Opens::Romm => self.open_romm(),
                Opens::Web => self.open_web(),
                Opens::Bluetooth => self.open_bluetooth(),
                Opens::PerfFan => self.open_perf_fan(),
                Opens::SleepPower => self.open_sleep_power(),
                Opens::Curator => self.open_curator_settings(),
                Opens::Achievements => self.open_achievements(),
                Opens::Tools => self.open_tools(),
                Opens::TestButtons => self.open_test_buttons(),
                Opens::PcStreaming => self.open_pc_streaming(),
                Opens::Controllers => {
                    self.open_controllers(false);
                    self.settings_page
                }
                Opens::Backup => self.open_backup(),
                Opens::Storage => self.open_storage(),
                Opens::Gamelist => self.open_gamelist(),
                Opens::Screenshots => self.open_gallery(),
                Opens::AndroidGames => self.open_android_games(),
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
                Opens::Report => self.open_report(),
                #[cfg(feature = "brick")]
                Opens::Haptics if pocket_ui::device::brick() => {
                    self.brick_haptics_step(1);
                    Some(SettingsPage::List(sel))
                }
                // On or off: A, Left and Right switch it the same way (`flip_settings_switch`).
                Opens::Haptics | Opens::RememberQuit | Opens::RemindWhere | Opens::ZoomInPause | Opens::LivedIn | Opens::BootScreen => {
                    self.flip_settings_switch(SETTINGS[sel].1);
                    Some(SettingsPage::List(sel))
                }
                Opens::MomentPhotos => {
                    self.step_moment_photos(true);
                    Some(SettingsPage::List(sel))
                }
                Opens::Extra(e) => {
                    self.switch_extra(e);
                    Some(SettingsPage::List(sel))
                }
                // Off, 10, 20, 50: the next one (`app_crate::shelf_space_step`).
                Opens::ShelfSpace => {
                    self.shelf_space_step(true);
                    Some(SettingsPage::List(sel))
                }
                Opens::OnStart => {
                    self.on_start = self.on_start.toggled();
                    self.save_settings();
                    Some(SettingsPage::List(sel))
                }
                Opens::Kids => {
                    self.kids_open_flow(false);
                    Some(SettingsPage::List(sel))
                }
                Opens::Animations => {
                    self.switch_animations();
                    Some(SettingsPage::List(sel))
                }

                Opens::Buttons => self.open_buttons(0),
                Opens::Frames => {
                    self.frames = self.frames.toggled();
                    self.save_settings();
                    Some(SettingsPage::List(sel))
                }
                Opens::OverGame => {
                    self.over_game = self.over_game.step(1, self.pixel_path);
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
                // The first start again over Settings; it ends on Home (`finish_setup`).
                Opens::FirstStart => {
                    self.setup = Some(self.new_setup().again());
                    None
                }
                Opens::ScreenColour => {
                    self.set_screen_colour(0);
                    Some(SettingsPage::List(sel))
                }
                Opens::Tour => {
                    self.open_tour();
                    Some(SettingsPage::List(sel))
                }
                Opens::BackToEs => {
                    self.request_back_to_es();
                    Some(SettingsPage::List(sel))
                }
                Opens::View => {
                    #[cfg(all(target_os = "android", feature = "handheld"))]
                    app_android::open_mobile();
                    Some(SettingsPage::List(sel))
                }
                // Levels: Left/Right change them, A has nothing to do (the hints offer no A).
                Opens::Brightness | Opens::Volume => Some(SettingsPage::List(sel)),
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
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Extra(_)) => {
                if let Opens::Extra(e) = SETTINGS[sel].1 {
                    self.switch_extra(e);
                }
                Some(SettingsPage::List(sel))
            }
            #[cfg(feature = "brick")]
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Haptics) && pocket_ui::device::brick() => {
                let delta = if b == Button::Right { 1 } else { -1 };
                self.brick_haptics_step(delta);
                Some(SettingsPage::List(sel))
            }
            // Two choices: left and right switch between them as A does.
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::OnStart) => {
                self.on_start = self.on_start.toggled();
                self.save_settings();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Appearance) => {
                self.step_appearance(b == Button::Right);
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Colours) => {
                self.step_colours(b == Button::Right);
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Font) => {
                self.step_font(b == Button::Right);
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Motion) => {
                self.step_motion(b == Button::Right);
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Animations) => {
                self.switch_animations();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Frames) => {
                self.frames = if b == Button::Left { self.frames.back() } else { self.frames.toggled() };
                self.save_settings();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::OverGame) => {
                self.over_game = self.over_game.step(if b == Button::Left { -1 } else { 1 }, self.pixel_path);
                self.save_settings();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Language) => {
                let now = pocket_ui::i18n::current();
                self.set_language(if b == Button::Left { now.previous() } else { now.next() });
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::TextSize) => {
                let now = pocket_ui::type_scale::get();
                self.set_text_size(if b == Button::Left { now.previous() } else { now.next() });
                Some(SettingsPage::List(sel))
            }
            // Brightness and Volume: the quick settings' own step (`Sys::step_level`), the same value and limits as HOME's rows.
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Brightness | Opens::Volume) => {
                let row = if matches!(SETTINGS[sel].1, Opens::Brightness) { pocket_ui::quick::Row::Brightness } else { pocket_ui::quick::Row::Volume };
                let game = self.sys_ctx().game;
                self.sys.step_level(row, if b == Button::Left { -1 } else { 1 }, game, Instant::now());
                self.request_redraw();
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::ScreenColour) => {
                let now = self.sys.look().warmth;
                self.set_screen_colour(now + if b == Button::Left { -1 } else { 1 });
                Some(SettingsPage::List(sel))
            }
            // The on/off rows: Left and Right switch them as A does (0.5.1: Zoom in the pause menu took only A, a hand
            // walk). Every row that changes in place takes Left/Right (`app_settings_groups::changes_in_place`,
            // tests/user_journeys.rs walks them all).
            (Some(SettingsPage::List(sel)), b @ (Button::Left | Button::Right)) if matches!(SETTINGS[sel].1, Opens::MomentPhotos) => {
                self.step_moment_photos(matches!(b, Button::Right));
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::Haptics | Opens::RememberQuit | Opens::RemindWhere | Opens::ZoomInPause | Opens::LivedIn | Opens::BootScreen) => {
                self.flip_settings_switch(SETTINGS[sel].1);
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::List(sel)), Button::Left | Button::Right) if matches!(SETTINGS[sel].1, Opens::ShelfSpace) => {
                self.shelf_space_step(b == Button::Right);
                Some(SettingsPage::List(sel))
            }
            (Some(SettingsPage::Starter), b) => self.starter_input(b),
            (Some(SettingsPage::Sync), b) => self.sync_input(b),
            (Some(SettingsPage::Leds(sel)), b) => self.leds_input(sel, b),
            (Some(SettingsPage::NightLight(sel)), b) => self.night_light_input(sel, b),
            (Some(SettingsPage::CozyExtras(sel)), b) => self.cozy_extras_input(sel, b),
            (Some(SettingsPage::Buttons(sel)), b) => self.buttons_input(sel, b),
            (Some(SettingsPage::Controllers(sel)), b) => self.controllers_input(sel, b),
            (Some(SettingsPage::Tailscale), b) => self.tailscale_input(b),
            (Some(SettingsPage::Romm), b) => self.romm_input(b),
            (Some(SettingsPage::Web), b) => self.web_input(b),
            (Some(SettingsPage::Pockets), b) => self.pocket_sync_input(b),
            (Some(SettingsPage::Bluetooth), b) => self.bluetooth_input(b),
            (Some(SettingsPage::PerfFan), b) => self.perf_fan_input(b),
            (Some(SettingsPage::SleepPower), b) => self.sleep_power_input(b),
            (Some(SettingsPage::Curator), b) => self.curator_settings_input(b),
            (Some(SettingsPage::Achievements), b) => self.achievements_input(b),
            (Some(SettingsPage::Tools), b) => self.tools_input(b),
            (Some(SettingsPage::TestButtons), b) => self.test_buttons_input(b),
            (Some(SettingsPage::GameCheck), b) => self.game_check_input(b),
            (Some(SettingsPage::PcStreaming), b) => self.pc_streaming_input(b),
            (Some(SettingsPage::Backup), b) => self.backup_input(b),
            (Some(SettingsPage::Storage), b) => self.storage_input(b),
            (Some(SettingsPage::Gamelist), b) => self.gamelist_input(b),
            (Some(SettingsPage::Screenshots), b) => self.gallery_input(b),
            (Some(SettingsPage::AndroidGames), b) => self.android_games_input(b),
            // B on the question only says "Later".
            (Some(SettingsPage::Updates), b) if b == CANCEL && self.updates.confirming() => {
                self.updates.set_confirmation(false);
                self.updates.sel = 0;
                Some(SettingsPage::Updates)
            }
            (Some(SettingsPage::Updates), b) if b == CANCEL => Some(SettingsPage::List(settings_row("Updates"))),
            (Some(SettingsPage::Updates), b) => {
                let n = self.update_actions().len();
                match b {
                    Button::Up => self.updates.sel = (self.updates.sel + n - 1) % n,
                    Button::Down => self.updates.sel = (self.updates.sel + 1) % n,
                    // The switch also turns with Left/Right.
                    Button::Left | Button::Right if self.update_actions().get(self.updates.sel) == Some(&UpdateAction::Auto) => self.update_action(UpdateAction::Auto),
                    // While a download runs, A stops it.
                    b if b == CONFIRM && self.updates.is_busy() => {
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
        if let Some(SettingsPage::Controllers(sel)) = self.settings_page {
            return self.paint_controllers(c, sel);
        }
        if let Some(SettingsPage::Buttons(sel)) = self.settings_page {
            return self.paint_buttons(c, sel);
        }
        if let Some(SettingsPage::CozyExtras(sel)) = self.settings_page {
            return self.paint_cozy_extras(c, sel);
        }
        if self.settings_page == Some(SettingsPage::Web) {
            return self.paint_web(c);
        }
        if self.settings_page == Some(SettingsPage::Pockets) {
            return self.paint_pocket_sync(c);
        }
        if self.settings_page == Some(SettingsPage::Tailscale) {
            return self.paint_tailscale(c);
        }
        if self.settings_page == Some(SettingsPage::Romm) {
            return self.paint_romm(c);
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
        if self.settings_page == Some(SettingsPage::TestButtons) {
            return self.paint_test_buttons(c);
        }
        if self.settings_page == Some(SettingsPage::GameCheck) {
            return self.paint_game_check(c);
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
        if self.settings_page == Some(SettingsPage::AndroidGames) {
            return self.paint_android_games(c);
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
            _ => "Settings",
        };
        // A category's list: the row of the categories' symbols instead of a title, the selected one named
        // (`app_settings_groups::paint_settings_header`); it takes a slim line of its own where it doesn't fit beside the pill.
        let top = match self.settings_page {
            Some(SettingsPage::List(sel)) => self.paint_settings_header(c, app_settings_groups::group_of(sel), false),
            Some(SettingsPage::Tabs(sel)) => self.paint_settings_header(c, app_settings_groups::group_of(sel), true),
            _ => {
                self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pocket_ui::kit::header_title(c.h), pal().chalk, title);
                pocket_ui::kit::header_bottom(c.h) + 12
            }
        };
        let right = c.w - pad;
        match self.settings_page {
            Some(SettingsPage::List(sel) | SettingsPage::Tabs(sel)) => {
                // With the focus on the categories at the top (`Tabs`) the rows show what is in the category under them,
                // with no row selected; a tap on a row goes there (Down, then as many rows on).
                let tabs = matches!(self.settings_page, Some(SettingsPage::Tabs(_)));
                // Only the rows of the selected row's category, counted from its first (`local`, `at`); their values
                // are worked out for these rows only.
                let rows = app_settings_groups::rows(app_settings_groups::group_of(sel));
                let (at, len) = (rows.iter().position(|i| *i == sel).unwrap_or(0), rows.len());
                // Values that need a process or a file are read on a thread (`app_settings_groups`).
                self.settings_values_refresh(&rows);
                // The hints first: where their pill starts says how much room a description has beside it
                // (`settings_row_hints`).
                let hints_at = if tabs { self.nu_hints(c, &[("B", "Back"), ("< >", "Category"), ("A", "Open")]) } else { self.nu_hints(c, self.settings_row_hints(sel)) };
                // The selected row's line at the bottom; the category keeps room over the hints when one of its rows needs
                // it (`app_settings_groups::rows_bottom`).
                let lines: Vec<&str> = rows.iter().map(|&i| self.settings_row_desc(i)).collect();
                let bottom = self.rows_bottom(&rows, c.w, c.h);
                // More rows than fit above the hints: a window that follows the selection, with a bar at the edge.
                let visible = (((bottom - top) / row) as usize).max(1);
                let first = window_start(at, visible, len);
                if len > visible {
                    let track = visible as i32 * row;
                    pocket_ui::kit::scrollbar(c, top, track, first, visible, len);
                }
                // The paper glides to the selection, the list scrolls under its edges (`app_motion`).
                let now = Instant::now();
                let (first_f, hl) = self.motion.list_at(app_motion::List::Settings, first, at, now);
                let mut band = c.rows(top, top + visible as i32 * row);
                let hy = ((hl - first_f) * row as f32).round() as i32;
                if !tabs {
                    pocket_ui::kit::selection(&mut band, pad, hy, right - pad, row);
                }
                for (local, k) in app_motion::rows_in_view(first_f, visible, len) {
                    let i = rows[local];
                    let name = app_settings_groups::row_name(i);
                    let y = (k * row as f32).round() as i32;
                    if (0.0..visible as f32 - 0.5).contains(&k) {
                        if tabs {
                            self.hits.presses(Area::new(pad, top + y, right - pad, row), Button::Down, (local + 1).min(255) as u8);
                        } else {
                            self.hits.row(Area::new(pad, top + y, right - pad, row), local);
                        }
                        self.motion.row(pad, top + y, right - pad, row);
                    }
                    let base = y + row / 2 + pocket_ui::type_scale::mid(font);
                    let under = if tabs { 0 } else { (app_motion::under(local, hl) * 255.0) as u32 };
                    let name_w = self.text.draw(&mut band, Face::Body, font, pad + NU_BTN_PAD, base, mix(pal().text, pal().ink, under), name);
                    if matches!(SETTINGS[i].1, Opens::Updates) && self.update_dot() {
                        Self::paint_update_dot(&mut band, pad + NU_BTN_PAD + name_w, base - pocket_ui::type_scale::mid(font), font);
                    }
                    // The row's value, from memory, cut to the room the name leaves (`app_settings_groups`).
                    // A connected network's name is shown as typed ("Off" or "Unknown" can be a name), in the fit and the paint.
                    let typed = app_settings_groups::value_is_typed(i);
                    let translating = self.text.translating();
                    if typed {
                        self.text.set_translate(false);
                    }
                    let value = self.settings_row_value(i, sel, right - NU_BTN_PAD - (pad + NU_BTN_PAD + name_w + 40));
                    pocket_ui::kit::row_value_in(&mut band, &mut self.text, font, right - NU_BTN_PAD, base, &value, (mix(pal().text, pal().ink, under), mix(pal().dust, pal().ink, under)));
                    self.text.set_translate(translating);
                }
                if !tabs {
                    self.hits.list(at, len, 0.0, row as f32);
                    self.paint_desc(c, lines[at], hints_at);
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
                // Android's handheld view: the app's own volume (Android owns the network, and "/storage" is not a card there).
                let android = pocket_ui::device::android_profile();
                let data_dir = self.data_dir.clone();
                ABOUT.refresh(ABOUT_EVERY, move || {
                    if android { (free_space(&data_dir.to_string_lossy()), None) } else { (free_space("/storage"), pocket_ui::wifi::ip_now()) }
                }, move || {
                    let _ = proxy.send_event(UserEvent::SettingsValues);
                });
                let (free, network) = match ABOUT.get() {
                    Some((free, ip)) => (free.unwrap_or_else(|| "unknown".into()), ip.map_or("Not connected".into(), |ip| format!("Connected, {ip}"))),
                    None => ("…".to_string(), "…".to_string()),
                };
                #[cfg(not(feature = "brick"))]
                let version = format!("pocket-ui {}", pocket_ui::update::app_version());
                #[cfg(feature = "brick")]
                let version = format!("pocket-ui {}", pocket_ui::update::brick_long(pocket_ui::update::app_version()));
                let version = if pocket_ui::device::android_profile() {
                    // The APK's version (Android's app info says the same), never Linux's "-dev (commit)".
                    format!("pocket-ui {}", update::shown_version())
                } else if update::release_build() { version } else {
                    // The commit's first nine characters, as `git log --oneline` shows it: the whole 40 were cut with "…" on
                    // the 4:3 panels, how far depended on the hash's letters (18.11.2026).
                    let commit = option_env!("POCKETUI_COMMIT").unwrap_or("unknown");
                    format!("pocket-ui {}-dev ({})", update::app_version(), commit.get(..9).unwrap_or(commit))
                };
                let mut lines = vec![
                    ("Version", version),
                    ("Games", match self.systems.len() {
                        1 => format!("{games} on 1 system"),
                        n => format!("{games} on {n} systems"),
                    }),
                    ("Free space", free),
                    // The status corner's figure (its thread's memory): no sysfs read in every frame of this page.
                    ("Battery", pocket_ui::status_corner::status().battery.map_or_else(|| "unknown".into(), |b| b.label())),
                ];
                // The Linux card's facts: its address, and its logs' folder (not shared over the network, only the games are; see
                // the troubleshooting page for SSH). Android's handheld view has neither (`about_has`).
                if about_has("Network", android) {
                    lines.insert(3, ("Network", network));
                }
                if about_has("Logs", android) {
                    lines.push(("Logs", self.data_dir.join(pocket_ui::logs::DIR_NAME).display().to_string()));
                }
                // "pocket-ui OS 0.1.7 · built on ROCKNIX 20261001 (SM8250)" on our SD image, "ROCKNIX 20261001 (SM8250)" on a
                // ROCKNIX card with pocket-ui added (`update::os_brand_line`); no row where there is no ROCKNIX.
                if let Some(system) = pocket_ui::update::os_brand() {
                    lines.insert(1, ("System", system.to_string()));
                }
                // On one of the Mini V2's untested siblings: which one, and that it is experimental (docs/DEVICES.md).
                // The Mini V2 and an unknown device show no such row.
                let device = pocket_ui::device::current().about_line();
                // A dev version and commit leave no room for the Brick's model in the Version row.
                // Keep its identity visible in a separate row, as on the experimental devices.
                #[cfg(feature = "brick")]
                let device = device.or_else(|| (!update::release_build()).then(|| {
                    pocket_ui::brick_profile::board().unwrap_or(pocket_ui::brick_profile::Board::Brick).name().to_string()
                }));
                if let Some(device) = device {
                    lines.insert(1, ("Device", device));
                }
                // With the Device row on a short screen (the Mini V1's 960 lines, larger text) the facts move closer
                // together, so the two rows under them and two lines of their help stay above the hints; where
                // everything fits (the Mini V2, any text size) they keep the row's height.
                // Under 768 lines (the Smart Pro's 720) room for two lines of help: one cut "Report a problem"'s explanation short.
                let help_lines = if c.h < 768 { 2 } else { 1 };
                let below = 2 * row + 20 + pocket_ui::type_scale::px(NU_SMALL) as i32 / 2 + help_lines * pocket_ui::type_scale::line(NU_SMALL, 12);
                let fit_for = |n: usize| (pocket_ui::kit::hint_bar_top(c.h) - 8 - top - below) / (n as i32 + 1);
                // Where the licence texts are (put there by the install zip and the image: pocket-ui's licence, the
                // credits, the crates' and the fonts' texts). Its row comes only where the facts keep three quarters
                // of the row's height with it; on the shortest page (the Device row, large German text) the help under
                // the rows needs the room, and the manual names the folder.
                if about_has("Licences", android) && fit_for(lines.len() + 1) >= row * 3 / 4 {
                    lines.push(("Licences", "/storage/pocket-ui/licences".to_string()));
                }
                let fit = fit_for(lines.len());
                // At least three quarters of the row's height, five eighths on 768 lines with the Device row, where the
                // two rows and a line of help under the facts would otherwise lie on the hints.
                // (Nine sixteenths under 768 lines, the Smart Pro's 720, for the second line of help.)
                let row = row.min(fit).max(if c.h < 768 { row * 9 / 16 } else { row * 5 / 8 });
                for (i, (name, value)) in lines.iter().enumerate() {
                    let base = top + i as i32 * row + row / 2 + pocket_ui::type_scale::mid(font);
                    let name_w = self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().dust, name);
                    // A long value (the logs' folder) is cut with "…" before it reaches its name.
                    let value = self.text.fit(Face::Body, font, value, right - NU_BTN_PAD - (pad + NU_BTN_PAD + name_w + 40));
                    self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, &value);
                    // pocket-ui's mark (the cartridge, `ui_art::Art::Logo`) before the version, where the row is tall
                    // enough for it at one pixel a pixel and it clears the row's name.
                    if i == 0 {
                        let (mw, mh) = pocket_ui::ui_art::size(pocket_ui::ui_art::Art::Logo, 1);
                        let vw = self.text.width(Face::Body, font, &value);
                        let mx = right - NU_BTN_PAD - vw - 20 - mw;
                        if mh <= row && mx > pad + NU_BTN_PAD + name_w + 40 {
                            let mid = top + i as i32 * row + row / 2;
                            pocket_ui::ui_art::paint(c, pocket_ui::ui_art::Art::Logo, mx, mid - mh / 2, 1);
                        }
                    }
                }
                // Report a problem, and ROCKNIX's own menu behind the warning (`app_report`, `app_rescue`).
                // Under 768 lines (the Smart Pro's 720) a third of a row between the facts and the rows instead of a whole one.
                let gap = if c.h < 768 { row / 3 } else { row };
                self.paint_about_rows(c, top + lines.len() as i32 * row + gap, row_h, font);
            }
            Some(SettingsPage::Starter | SettingsPage::Sync | SettingsPage::Leds(_) | SettingsPage::NightLight(_) | SettingsPage::CozyExtras(_) | SettingsPage::Buttons(_) | SettingsPage::Controllers(_) | SettingsPage::Tailscale | SettingsPage::Romm | SettingsPage::Web | SettingsPage::Pockets | SettingsPage::Bluetooth | SettingsPage::PerfFan | SettingsPage::Curator | SettingsPage::Achievements | SettingsPage::Tools | SettingsPage::TestButtons | SettingsPage::GameCheck | SettingsPage::PcStreaming | SettingsPage::Backup | SettingsPage::Storage | SettingsPage::Gamelist | SettingsPage::Screenshots | SettingsPage::AndroidGames | SettingsPage::SleepPower) | None => {}
        }
    }

    /// Settings > Battery: the charge now and what it means in time, a graph of the last hours, what the games cost,
    /// and where the times of sleep are set now. Only says what the samples support (`battery::estimate`).
    fn paint_battery(&mut self, c: &mut Canvas, range: usize) {
        use pocket_ui::battery;
        let (pad, font) = (NU_PAD, 36u32);
        let (left, right, top) = (NU_PAD + NU_BTN_PAD, c.w - NU_PAD - NU_BTN_PAD, pocket_ui::kit::header_bottom(c.h) + 12);
        let now = recents::now_secs();
        let hours = battery::RANGES[range.min(battery::RANGES.len() - 1)];
        let history = pocket_ui::battery_history::view();
        let samples = history.recent(now, battery::KEEP);
        let current = battery::read(&battery::dir(), now);
        let curve = pocket_ui::battery_history::curve();
        let (headline, sub) = battery::describe(current.as_ref(), pocket_ui::battery_estimate::estimate_with(samples, now, curve.as_ref()).as_ref());

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
        let capacity = current.as_ref().filter(|s| !s.plugged()).map(|s| s.capacity);
        let mut lines: Vec<String> = draws
            .iter()
            .map(|d| battery::draw_line(&d.tag.as_deref().map_or("The menus".to_string(), |t| system_short(&self.systems, t)), d, capacity))
            .collect();
        // What the last sleep cost (the screen off, the game paused) comes first: no other place says it, while a game's
        // cost is in its pause menu too. Where room runs out (Large German on 1024x768 keeps one line) the systems go.
        if let Some(slept) = pocket_ui::battery_estimate::last_sleep(&history, now) {
            lines.insert(0, battery::sleep_line(&slept));
        }
        // When the screen goes off and the Pocket sleeps is set elsewhere now: one line says where.
        let pointer = { self.text.wrap_prose(Face::Body, NU_SMALL, SLEEP_SETTINGS_POINTER, c.w - 4 * pad) };
        let (draw_step, desc_step) = (pocket_ui::type_scale::line(NU_SMALL, 14), pocket_ui::type_scale::line(NU_SMALL, 10));
        // Everything under the graph, from its bottom edge to the pointer's last line, and the hint bar's top: the
        // graph gives up height when Large text needs it (it stays at least `min_graph`).
        // (100 on a short screen, 768 lines, where a pointer of two lines would otherwise lie on the hints.)
        let min_graph: i32 = if pocket_ui::kit::short_screen(c.h) { 100 } else { 140 };
        let under_for = |n: usize| 80 + 12 + n as i32 * draw_step + 60 + (pointer.len() as i32 - 1).max(0) * desc_step + desc_step / 2;
        let hints_top = pocket_ui::kit::hint_bar_top(c.h) - 8;
        // Where even the shortest graph would leave the lines under it no room (Large German on 1024x768), the games'
        // cost lines go before the pointer to the Sleep settings and the graph, which lie on the hint bar otherwise.
        let top_of_graph = top + 130;
        while !lines.is_empty() && hints_top - under_for(lines.len()) - top_of_graph < min_graph {
            lines.pop();
        }
        let under = under_for(lines.len());

        // The graph, with room for the % labels on its left and the hours under it.
        let (gx, gy) = (left + 100, top + 130);
        let (gw, gh) = (right - gx, (hints_top - under - gy).clamp(min_graph, 300));
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

        let mut ly = below + 12;
        for line in &lines {
            let line = self.text.fit(Face::Body, NU_SMALL, line, right - gx);
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
        let top = pocket_ui::kit::header_bottom(c.h) + 12;
        let view_h = pocket_ui::kit::hint_bar_top(c.h) - 16 - top;
        // The guide's pages stay English for now (docs/i18n-glossary.md); the title and hints around them don't.
        self.text.set_translate(false);
        self.manual.paint(c, &mut self.text, left, top, c.w - 2 * left, view_h);
        self.text.set_translate(true);
        let title_w = self.text.width(Face::Title, 50, "Manual");
        let place = format!("{} of {}   {}", self.manual.page + 1, manual::pages().len(), manual::title(manual::pages()[self.manual.page].1));
        let x = left + title_w + 36;
        // Not under the clock and battery pill.
        let place = self.text.fit(Face::Body, NU_SMALL, &place, c.w - pad - 260 - x);
        self.text.draw(c, Face::Body, NU_SMALL, x, pad + 56, pal().dust, &place);
        let (sel, len) = self.manual.touch_rows();
        self.hits.list(sel, len, 0.0, manual::STEP as f32);
        // X only with another link in view to mark (one link: X would do nothing; the navigation explorer, 2026-11-07).
        if self.manual.links_in_view() > 1 {
            self.nu_hints(c, &[("A", "Open link"), ("X", "Next link"), ("L1", "Previous"), ("R1", "Next"), ("B", "Back")]);
        } else if self.manual.has_links() {
            self.nu_hints(c, &[("A", "Open link"), ("L1", "Previous"), ("R1", "Next"), ("B", "Back")]);
        } else {
            self.nu_hints(c, &[("L1", "Previous"), ("R1", "Next"), ("B", "Back")]);
        }
    }

    /// Input while the manual is open: Up/Down and the stick scroll (held: faster), Left/Right a screen,
    /// L1/R1 the previous and next page, B back to Settings.
    fn manual_input(&mut self, pe: PadEvent) {
        let now = Instant::now();
        match pe {
            PadEvent::Button { button, pressed: true, .. } => match button {
                // A link to another page was followed: B goes back to where it was.
                b if b == CANCEL && self.manual.back_from_link() => self.manual.set_velocity(0.0, now),
                b if b == CONFIRM => {
                    self.manual.open_link();
                }
                Button::North => self.manual.next_link(),
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

    /// Settings → Screen & sound → Photos of big moments one value on (A, Right) or back (Left), and kept: Off, On, Also in play
    /// (only where pocket-core draws the game: `device::hides("moment_in_play")`), round again.
    pub(crate) fn step_moment_photos(&mut self, forward: bool) {
        let values = if pocket_ui::device::hides("moment_in_play", false, false) { 2 } else { 3 };
        let now = match (self.moment_photos, self.moment_in_play && values == 3) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        };
        let next = if forward { (now + 1) % values } else { (now + values - 1) % values };
        self.moment_photos = next > 0;
        self.moment_in_play = next == 2;
        self.save_settings();
    }

    /// One of Settings' on/off rows switched (A, Left or Right on it) and kept.
    fn flip_settings_switch(&mut self, opens: Opens) {
        match opens {
            Opens::Haptics => pocket_ui::rumble::set_menu_haptics(&self.data_dir, !pocket_ui::rumble::menu_haptics()),
            Opens::RememberQuit => self.remember_quit = !self.remember_quit,
            Opens::RemindWhere => self.remind_where = !self.remind_where,
            Opens::ZoomInPause => self.zoom_in_pause = !self.zoom_in_pause,
            Opens::LivedIn => {
                self.lived_in = !self.lived_in;
                self.forget_shelf_row();
            }
            Opens::BootScreen => self.boot_screen = !self.boot_screen,
            _ => return,
        }
        if !matches!(opens, Opens::Haptics) {
            self.save_settings();
        }
    }

    fn save_settings(&self) {
        Settings {
            theme: self.kids_saved_settings().0,
            onboarded: self.onboarded,
            update_url: self.update_url.clone(),
            store_url: self.store_url.clone(),
            hidden_systems: self.hidden.clone(),
            streaming_on_home: self.streaming_on,
            appearance: self.appearance,
            on_start: self.on_start,
            forget_where_quit: !self.kids_saved_settings().1,
            where_was_i: self.remind_where,
            zoom_in_pause: self.zoom_in_pause,
            lived_in: self.lived_in,
            boot_screen: self.boot_screen,
            moment_photos: self.moment_photos,
            moment_in_play: self.moment_in_play,
            big_shelf: false,
            button_layout: self.button_layout,
            prompts: pocket_ui::button_prompts::get(),
            colours: pocket_ui::palette::colours(),
            // The style in use (`font_style`, process wide): the Theme page sets it and the next save keeps it.
            font: pocket_ui::font_style::get(),
            retro_colours: pocket_ui::retro::colours(),
            retro_letters: pocket_ui::font_style::retro_letters(),
            gba_model: pocket_ui::system_art::gba_model(),
            console_colours: pocket_ui::system_art::colours_chosen(),
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
            open_case_told: self.big.open_case_told,
            extras: pocket_ui::cozy_extras::get(),
            retired_look: false,
            damaged: false,
        }
        .save(&self.data_dir);
    }

    /// Settings → Look → Text size: the next frame is painted at `size` (the text painter scales every size,
    /// `pocket_ui::type_scale`), and it is kept. The new size's glyphs are drawn ahead on a thread, as at the start.
    /// Settings → Screen & sound → Screen colour: the same look as SELECT + Vol, applied and saved at once (`Sys::set_warmth`).
    fn set_screen_colour(&mut self, warmth: i8) {
        let game = self.sys_ctx().game;
        if self.sys.set_warmth(warmth, game) {
            self.request_redraw();
        }
    }

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
        self.cabinet.language_changed();
        self.look_previews = None;
        self.request_redraw();
        self.save_settings();
    }

    /// Settings → Controls → Buttons → Button layout: the other layout, used from the next button on (the press that switched it
    /// is let go as the role it was pressed as, `button_layout::Remap`) and kept.
    fn switch_button_layout(&mut self) {
        self.button_layout = self.button_layout.toggled();
        pocket_ui::button_layout::set(self.button_layout);
        self.save_settings();
    }

    #[cfg(not(feature = "brick"))]
    fn manifest_url(&self) -> String {
        self.update_url.clone().unwrap_or_else(|| update::DEFAULT_URL.to_string())
    }

    #[cfg(feature = "brick")]
    fn manifest_url(&self) -> String {
        if pocket_ui::device::darkos_port() { return update::default_url(); }
        self.update_url.clone().unwrap_or_else(update::default_url)
    }

    /// A visit is an explicit check when the remembered list has aged. The automatic switch controls shelf notices.
    fn open_updates(&mut self) -> SettingsPage {
        self.updates.sel = 0;
        self.updates.set_confirmation(false);
        if pocket_ui::device::no_wifi_chip() {
            self.update_action(UpdateAction::Card);
        } else if self.updates.card.is_none() && !self.updates.network_offer_is_fresh(std::time::SystemTime::now()) {
            self.update_action(UpdateAction::Check);
        }
        SettingsPage::Updates
    }

    /// What the Updates page offers right now, in display order. Android's handheld view has no update file on a card.
    fn update_actions(&self) -> Vec<UpdateAction> {
        let (available, confirming) = (self.updates.app_available().is_some(), self.updates.confirming());
        if pocket_ui::device::android_profile() { pocket_ui::update_page::actions(available, confirming) }
        else { pocket_ui::update_page::actions_with_card(available, confirming, !pocket_ui::device::no_wifi_chip()) }
    }

    /// Starts `action` on a worker thread; results come back as `UserEvent::Update`.
    fn update_action(&mut self, action: UpdateAction) {
        let action = match (pocket_ui::device::no_wifi_chip(), action) {
            (true, UpdateAction::Check) => UpdateAction::Card,
            (true, UpdateAction::Auto) => return,
            _ => action,
        };
        // The switch works at any time, even while something downloads.
        if action == UpdateAction::Auto {
            self.set_update_auto(!self.update_auto(), Instant::now());
            return;
        }
        if self.updates.is_busy() {
            return;
        }
        // Android's handheld view reads Android's own list through the app's reader, as its notice does (`app_update_hint`).
        let url = if app_update_hint::ANDROID_APK { self.hint_url() } else { self.manifest_url() };
        let proxy = self.proxy.clone();
        let send = move |e| {
            let _ = proxy.send_event(UserEvent::Update(e));
        };
        self.updates.message = None;
        self.updates.done = 0;
        match action {
            UpdateAction::Check => {
                self.updates.start(UpdateTask::Check);
                let supports_card = !pocket_ui::device::android_profile();
                std::thread::spawn(move || {
                    let result = app_update_hint::fetch(&url);
                    if result.is_err() && supports_card && pocket_ui::wifi::ip_now().is_none() {
                        send(UpdateEvent::CheckingCard);
                        send(UpdateEvent::CardLooked { at_start: false, look: update::card::look() });
                    } else {
                        send(UpdateEvent::Checked(result));
                    }
                });
            }
            UpdateAction::InstallApp => {
                if self.updates.app_available().is_none() { return; }
                self.updates.set_confirmation(true);
                self.updates.sel = 0;
            }
            UpdateAction::Later => {
                self.updates.set_confirmation(false);
                self.updates.sel = 0;
            }
            UpdateAction::Auto => {}
            UpdateAction::Card => {
                self.updates.start(UpdateTask::CheckCard);
                std::thread::spawn(move || send(UpdateEvent::CardLooked { at_start: false, look: update::card::look() }));
            }
            UpdateAction::InstallNow => {
                self.updates.set_confirmation(false);
                // Android: the APK goes to Android's installer; its progress is the card over this page, B stops it.
                if app_update_hint::ANDROID_APK {
                    self.updates.sel = 0;
                    self.android_install(Instant::now());
                    return;
                }
                let Some(rel) = self.updates.app_available().cloned() else { return };
                let cancel = update::Cancel::new();
                self.updates.start(UpdateTask::App { version: rel.version.clone(), total: rel.size, cancel: cancel.clone() });
                self.observe_update_worker(&cancel);
                // From the update file on the card: the same install, its files taken from the file (`update::card`).
                if let Some(found) = self.updates.card.clone().filter(|f| f.manifest.app.as_ref() == Some(&rel)) {
                    std::thread::spawn(move || {
                        let result = update::card::install(&found, &cancel, &|n| send(UpdateEvent::Progress(n)));
                        send(UpdateEvent::Done(result.map(|_| true)));
                    });
                    return;
                }
                std::thread::spawn(move || {
                    #[cfg(feature = "brick")]
                    let result = update::install(&rel, &url, &cancel, &|n| send(UpdateEvent::Progress(n)));
                    #[cfg(not(feature = "brick"))]
                    let result = update::install_app(&rel, &url, std::path::Path::new(update::APP_ROOT), &cancel, &|n| send(UpdateEvent::Progress(n)));
                    send(UpdateEvent::Done(result.map(|_| true)));
                });
            }
        }
    }

    fn observe_update_worker(&self, cancel: &update::Cancel) {
        let UpdatePhase::Running(job) = &self.updates.phase else { return };
        let (job, proxy) = (job.id, self.proxy.clone());
        cancel.observe(move |snapshot| { let _ = proxy.send_event(UserEvent::Update(UpdateEvent::Worker { job, snapshot })); });
    }

    fn update_event(&mut self, e: UpdateEvent) {
        let outcome = match e {
            UpdateEvent::Inspected(inspection) => { self.update_inspection = inspection; return; }
            UpdateEvent::Progress(n) => {
                self.updates.progress(n);
                return;
            }
            UpdateEvent::Worker { job, snapshot } => {
                if let UpdatePhase::Running(active) = &mut self.updates.phase {
                    if active.id == job { active.worker = snapshot; }
                }
                return;
            }
            UpdateEvent::Looked { job_at_start, result } => {
                self.update_hint_looked(result, Instant::now(), job_at_start);
                self.resume_shelf_after_update();
                return;
            }
            UpdateEvent::CheckingCard => {
                if let UpdatePhase::Running(job) = &mut self.updates.phase {
                    if matches!(job.task, UpdateTask::Check) {
                        job.task = UpdateTask::CheckCard;
                        job.label = job.task.label();
                    }
                }
                return;
            }
            UpdateEvent::CardLooked { at_start: true, look } => {
                self.card_update_found_at_start(look);
                self.resume_shelf_after_update();
                return;
            }
            UpdateEvent::CardLooked { at_start: false, look } => self.card_update_looked(look),
            UpdateEvent::Checked(Ok(m)) => {
                self.updates.card = None;
                self.updates.remember_manifest(m, std::time::SystemTime::now());
                let none = !self.update_actions().iter().any(|a| matches!(a, UpdateAction::InstallApp));
                self.updates.message = none.then(|| if self.updates.release_build() { "Everything is up to date." } else { update::DEVELOPMENT_BUILD }.to_string());
                UpdateOutcome::Checked
            }
            UpdateEvent::Checked(Err(e)) | UpdateEvent::Done(Err(e)) => { self.updates.message = Some(e); UpdateOutcome::Failed },
            UpdateEvent::Done(Ok(true)) => {
                self.updates.message = Some("Installed. Restarting…".into());
                self.updates.restart_at = Some(Instant::now() + Duration::from_millis(1500));
                UpdateOutcome::Installed
            }
            UpdateEvent::Done(Ok(false)) | UpdateEvent::OsStaged(None) => { self.updates.message = Some(os_staged_message(None)); UpdateOutcome::OsStaged },
            UpdateEvent::OsStaged(Some(note)) => { self.updates.message = Some(os_staged_message(Some(&note))); UpdateOutcome::OsStaged },
        };
        self.updates.finish(outcome);
        if let Some(inspector) = &self.update_inspector { let _ = inspector.send(()); }
        self.updates.sel = 0;
        self.resume_shelf_after_update();
    }

    fn resume_shelf_after_update(&mut self) {
        if !self.after_first_done || self.updates.restart_at.is_some() || pocket_ui::proc::mock() || pocket_ui::background_io::paused() {
            return;
        }
        if self.identify.due() || self.cover_run.resume_due() {
            self.auto_covers("after an update check");
        } else if self.text_run.resume_due() {
            self.auto_texts("after an update check");
        }
    }

    /// Versions, the offered actions, and progress or the last result at the bottom.
    fn paint_updates(&mut self, c: &mut Canvas, top: i32, row: i32, font: u32) {
        let (pad, right) = (NU_PAD, c.w - NU_PAD);
        let m = self.updates.manifest.clone();
        #[cfg(not(feature = "brick"))]
        let app_line = match self.updates.app_available() {
            Some(a) => format!("{}, {} available", update::shown_version(), a.version),
            _ => update::shown_version().to_string(),
        };
        #[cfg(feature = "brick")]
        let app_line = match self.updates.app_available() {
            Some(a) => format!("{}, {} available", update::brick_short(update::app_version()), update::brick_short(&a.version)),
            _ => update::brick_short(update::app_version()),
        };
        let actions = self.update_actions();
        let busy = self.updates.is_busy();
        // Bottom: progress while busy, else the last result, else the notes of the selected action.
        let bar = |c: &mut Canvas, frac: f32| {
            let (w, y) = (c.w - 2 * pad - 2 * NU_BTN_PAD, pocket_ui::kit::hint_bar_top(c.h) - 90);
            pocket_ui::retro::pill(c, pad + NU_BTN_PAD, y, w, 12, 6, pal().keycap);
            pocket_ui::retro::pill(c, pad + NU_BTN_PAD, y, ((w as f32 * frac) as i32).max(12), 12, 6, pal().band);
        };
        let app_step = self.updates.app_step();
        let footer = match self.updates.busy() {
            // An app install: one bar for every step and one line that names it (Alex, 15.11.: "Preparing update" for half a
            // minute with nothing moving). The short checks between keep their own words.
            Some(_) if app_step.is_some() => {
                let (step, frac) = app_step.unwrap();
                bar(c, frac);
                let phase = self.updates.cancel().and_then(|c| c.phase()).unwrap_or(self.updates.worker().phase);
                match step {
                    // One sentence for every file, the same between the parts (Alex, 20.11.: "Connecting to update server" flashed for
                    // every small part of the Brick's update).
                    update::Step::Downloading => self.updates.download().line(phase),
                    _ => step.line((frac * 100.0) as u32),
                }
            }
            Some((label, total)) if total > 0 => {
                let total = self.updates.cancel().map(|c| c.progress_total()).filter(|n| *n > 0).unwrap_or(total);
                let frac = (self.updates.done as f32 / total as f32).clamp(0.0, 1.0);
                bar(c, frac);
                match self.updates.cancel().and_then(|c| c.phase()) {
                    Some(update::WorkerPhase::Downloading) if self.updates.can_cancel() => format!("Downloading the update… {}%", (frac * 100.0) as u32),
                    Some(phase @ update::WorkerPhase::Fallback) if self.updates.can_cancel() && self.updates.download().retry.is_some() => self.updates.download().line(phase),
                    Some(phase) if self.updates.can_cancel() && phase.label().is_some() => phase.label().unwrap().to_string(),
                    _ => format!("{label}, {}%", (frac * 100.0) as u32),
                }
            }
            Some((LOOKING_ON_CARD, _)) => LOOKING_ON_CARD.into(),
            Some((label, _)) => format!("{label}…"),
            // Installed: the bar stays full and the line says what comes next until the new version is up.
            None if app_step.is_some_and(|(step, _)| step == update::Step::Restarting) => { bar(c, 1.0); update::Step::Restarting.line(100) }
            None if self.updates.confirming() => {
                // Under a fifth and not on the charger: the charger first (a flat battery in the middle of an install is the
                // one way this goes wrong).
                let battery = pocket_ui::status_corner::status().battery;
                match pocket_ui::update_hint::battery_warning(battery) {
                    Some(warning) => warning,
                    // Android's installer closes pocket-ui at its end: said before the press.
                    None if pocket_ui::device::android_profile() => pocket_ui::update_page::ANDROID_CONFIRM.to_string(),
                    None => m.as_ref().and_then(|m| m.app.as_ref()).map(|a| a.notes.clone()).unwrap_or_default(),
                }
            }
            None => self.updates.message.clone().unwrap_or_else(|| match actions.get(self.updates.sel) {
                Some(UpdateAction::InstallApp) => m.as_ref().and_then(|m| m.app.as_ref()).map(|a| a.notes.clone()).unwrap_or_default(),
                _ if !self.updates.release_build() => update::DEVELOPMENT_BUILD.to_string(),
                _ => String::new(),
            }),
        };
        // Wrapped to the screen width and kept above the button hints, never up into the actions. When the whole note doesn't
        // fit (Large text on the Smart Pro's 720 lines: one line of a three-line note, 15.11.), first the rows draw closer, then the
        // note gets a little smaller; only what still doesn't fit ends with "…" (`Text::fit_sizes`: a line a little smaller reads
        // better than one cut short). Under 768 lines the note's lines are closer and without the spare row's margin over them.
        let max_w = c.w - 2 * pad - 2 * NU_BTN_PAD;
        // Translated as a whole before it is broken into lines (`i18n`).
        let footer = self.text.translated(&footer).to_string();
        let tight = c.h < 768;
        let close = (row * 4 / 5).max(pocket_ui::type_scale::px(font) as i32 + 22).min(row);
        let layouts = [(row, NU_SMALL), (close, NU_SMALL), (close, NU_SMALL * 7 / 8), (close, NU_SMALL * 3 / 4)];
        let mut chosen = None;
        for (i, &(r, px)) in layouts.iter().enumerate() {
            let mut lines: Vec<String> = Vec::new();
            // Between words, and between any two hanzi or kana (`text::line_units`).
            for (glue, word) in pocket_ui::text::line_units(&footer) {
                match lines.last_mut() {
                    Some(line) if self.text.width(Face::Body, px, &format!("{line}{glue}{word}")) <= max_w => {
                        line.push_str(glue);
                        line.push_str(word);
                    }
                    _ => lines.push(word.to_string()),
                }
            }
            let actions_end = top + (actions.len() as i32 + 1) * r + r / 3 + if tight { 0 } else { r } + 8;
            let step = (if tight { 36 } else { 40 }) * px as i32 / NU_SMALL as i32;
            let fit = (((pocket_ui::kit::hint_bar_top(c.h) - 30) - actions_end - 30) / step + 1).max(1) as usize;
            if lines.len() <= fit || i + 1 == layouts.len() {
                if lines.len() > fit {
                    lines.truncate(fit);
                    if let Some(last) = lines.last_mut() {
                        *last = self.text.fit(Face::Body, px, &format!("{last} …"), max_w);
                    }
                }
                chosen = Some((r, px, step, lines));
                break;
            }
        }
        let (row, note_px, step, lines) = chosen.expect("the last layout is always taken");
        let versions = [("pocket-ui", app_line)];
        for (i, (name, value)) in versions.iter().enumerate() {
            let base = top + i as i32 * row + row / 2 + pocket_ui::type_scale::mid(font);
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().dust, name);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, value);
        }
        for (i, action) in actions.iter().enumerate() {
            let label = action.label(m.as_ref().and_then(|m| m.app.as_ref()).map(|a| a.version.as_str()));
            let y = top + (i as i32 + 1) * row + row / 3;
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = i == self.updates.sel && !busy;
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
            }
            let ink = if selected { pal().ink } else if busy && *action != UpdateAction::Auto { pal().dust } else { pal().text };
            if *action == UpdateAction::Auto {
                // The switch: its state at the right; the label gives way to it ("Check automatically" where the full one is too long).
                let value = self.text.translated(pocket_ui::settings_values::on_off(self.update_auto())).to_string();
                let room = c.w - 2 * (pad + NU_BTN_PAD) - self.text.width(Face::Body, font, &value) - 24;
                let full = self.text.translated(&label).to_string();
                let label = if self.text.width(Face::Body, font, &full) <= room { full } else { self.text.fit(Face::Body, font, "Check automatically", room) };
                let was = self.text.translating();
                self.text.set_translate(false);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, ink, &label);
                self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, ink, &value);
                self.text.set_translate(was);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, ink, &label);
            }
        }
        let mut y = pocket_ui::kit::hint_bar_top(c.h) - 30 - step * (lines.len() as i32 - 1).max(0);
        for line in &lines {
            let w = self.text.width(Face::Body, note_px, line);
            self.text.draw(c, Face::Body, note_px, (c.w - w) / 2, y, pal().text, line);
            y += step;
        }
        // Cancel only while it still stops something: the Brick's parts move from step 3 on, and after a valid download the other
        // installs finish (`update.cancel` too-late in the status file).
        let cancel_works = matches!(&self.updates.phase, UpdatePhase::Running(job) if job.worker.cancel_effective);
        let hints: &[(&str, &str)] = if self.updates.can_cancel() && cancel_works {
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
            // Games sent during it are no news: no "new games" toast on the first shelf.
            self.game_check_after_first_start();
        }
        self.onboarded = true;
        self.save_settings();
        // Games may have been copied during setup.
        let mut added = 0;
        if let Ok(systems) = library::load_shown(&self.es_systems, &self.data_dir) {
            added = pocket_ui::library_cache::games_added(&self.systems, &systems);
            // The next start shows them from the cache (`library_cache`), written on a thread.
            pocket_ui::library_cache::store_soon(&self.data_dir, &self.es_systems, systems.clone());
            self.library_replace_systems(systems);
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
        self.game_check_games_added(added);
    }

    /// Settings > Games > Box art & downloads > Pictures as it is now.
    fn art(&self) -> pocket_ui::settings::Art {
        pocket_ui::settings::Art::from_flags(!self.pictures.wanted(), self.art_off)
    }

    /// Starts the background cover download when it may run (`AutoCovers::decide`): online, no game running,
    /// nothing scraping already, games without covers. Quiet: the shelf shows "Covers 3 of 12" at most.
    fn auto_covers(&mut self, why: &str) {
        use pocket_ui::auto_covers::Decision;
        if pocket_ui::background_io::paused() { return; }
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
        // A card that came with its own box art (EmulationStation, NextUI, Anbernic …) keeps it: ask for pictures only
        // once the gamelists and picture folders are read (`gamelist_event` asks again), and not for games that have one.
        if online && self.covers_wait_for_gamelists() {
            println!("covers ({why}): waiting for the card's own pictures");
            return;
        }
        let games = if online { self.all_games() } else { Vec::new() };
        let games: Vec<(String, PathBuf)> = games.into_iter().filter(|(sys, rom)| !self.gamelist.index.has_box_art(sys, rom)).collect();
        let missing = if online { self.cover_run.missing(&self.data_dir, &games) } else { Vec::new() };
        // Screenshots and title screens follow the box art, unless Settings > Games > Box art & downloads says box art only.
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
            pocket_ui::web::lower_priority();
            let progress = |done, total, found| {
                let _ = proxy.send_event(UserEvent::Covers(CoversEvent::Progress { done, total, found }));
            };
            let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed) || pocket_ui::background_io::cancelled();
            let run = pocket_ui::background_io::job(|| {
                pocket_ui::game_pictures::run_until(&pocket_ui::game_pictures::Libretro, &data_dir, &missing, Some(&pictures), progress, stopped)
            });
            let event = match run {
                Some(run) => CoversEvent::Done {
                    total: run.covers_total + run.pictures_total,
                    stopped: run.stopped(),
                    unreachable: run.unreachable,
                    looked_up: run.covers_looked_up,
                    pictures_looked_up: run.pictures_looked_up,
                    found: run.found,
                },
                None => CoversEvent::Done {
                    total: missing.len() + pictures.len(),
                    stopped: true,
                    unreachable: false,
                    looked_up: Vec::new(),
                    pictures_looked_up: Vec::new(),
                    found: 0,
                },
            };
            let _ = proxy.send_event(UserEvent::Covers(event));
        });
    }

    /// Covers, pictures or descriptions load in the background: the shelf shows a quiet dot for it, no words.
    pub(crate) fn background_work_sign(&self) -> bool {
        self.cover_run.note().is_some() || self.text_run.note().is_some()
    }

    fn covers_event(&mut self, e: CoversEvent) {
        // Extras > Cover art shows the run too, while it is open.
        let mut gave_up = false;
        let shown = match e {
            CoversEvent::Progress { done, total, found } => {
                self.cover_run.progress(done, total);
                (done, total, found)
            }
            CoversEvent::Done { looked_up, pictures_looked_up, found, total, stopped, unreachable } => {
                println!(
                    "covers: {found} found, {} of {total} looked up{}{}",
                    looked_up.len() + pictures_looked_up.len(),
                    if stopped { ", stopped for a game" } else { "" },
                    if unreachable { ", picture server not answering: will try again at the next start" } else { "" }
                );
                gave_up = unreachable;
                self.cover_run.finish_run(looked_up, pictures_looked_up, stopped);
                // New box art and pictures show at once.
                self.covers.clear();
                self.pictures_changed();
                self.cabinet.covers_changed();
                self.look_previews = None;
                if stopped { self.resume_shelf_after_update(); }
                (total.max(1), total.max(1), found)
            }
        };
        if let Some(setup) = self.setup.as_mut() {
            setup.on_event(SetupEvent::Covers { done: shown.0, total: shown.1, found: shown.2 });
            if gave_up {
                setup.on_event(SetupEvent::CoversUnreachable);
            }
        }
    }

    fn animating(&self) -> bool {
        // The Brick's bridge takes frames on a dark screen as well (no compositor stops them): nothing moves while it is dark.
        #[cfg(feature = "brick")]
        if pocket_ui::device::brick() && self.sleep.state() != pocket_ui::sleep::State::Awake {
            return false;
        }
        // Only a shelf that is drawn moves (`app_where`): not behind a page, Kids Mode or a game.
        let shelf_visible = self.shelf_in_front();
        self.setup.as_ref().is_some_and(|s| s.animating()) || self.switcher.as_ref().is_some_and(|s| s.animating()) ||
        self.pending_launch.is_some() || self.quit.is_some() || self.save_filling() || self.kids_animating() || self.pull_moving(Instant::now()) || self.motion_animating(Instant::now()) || self.sync_note_active() || self.tailscale_animating() || self.romm_animating() || self.bluetooth_animating() || self.tools_animating() || self.test_buttons_animating() || self.game_check_animating() || self.pc_streaming_animating() || self.backup_animating() || (self.updates.restart_at.is_some() && self.running.is_none()) || self.launch_at.is_some_and(|t| t.elapsed() < FADE + self.cabinet.launch_delay() + LAST_FADE_FRAME) || (self.theme == Theme::Cabinet && shelf_visible && self.cabinet.animating(Instant::now())) || (self.theme == Theme::Shelf && shelf_visible && self.shelf_media_animating(Instant::now())) || self.stream_message_due() || self.exit_since.is_some() || (if self.sys.quiet_game_controls(&self.sys_ctx()) { self.sys.animating_on_screen(Instant::now()) } else { self.sys.animating(Instant::now()) }) || (self.manual.moving() && self.settings_page == Some(SettingsPage::Manual)) || self.appearance_animating() || self.card.as_ref().is_some_and(|o| o.card.boxed.as_ref().is_some_and(|b| b.animating(Instant::now()))) || self.find_animating() || self.boot_fade.active() || self.ejecting() || (self.theme == Theme::Cabinet && shelf_visible && self.ajar_sliding(Instant::now()))
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

    /// A Quit that takes longer than a second shows it is working.
    pub(crate) fn paint_saving_wait(&mut self, c: &mut Canvas) {
        let Some(waited) = self.quit.as_ref().map(|q| q.waited(Instant::now())).filter(|w| *w >= SAVE_WAIT_SHOWN) else { return };
        let turn = if pocket_ui::motion::level() == pocket_ui::motion::Motion::Off { 0 } else { (waited.as_millis() / 125) as i32 };
        // pocket-core says when it writes; a game past that is closing. The big emulators don't say: the time does.
        let run = self.running.as_ref();
        let closing = run.is_some_and(|r| r.hosted() && !r.pgid.is_some_and(pocket_ui::fast_switch::saving_now));
        let word = if closing {
            "Closing the game…"
        } else if waited >= SAVE_WAIT_SLOW {
            "This one takes a while. Still saving, don't switch off."
        } else {
            "Still saving…"
        };
        let px = 28;
        let word = self.text.fit(Face::Body, px, word, c.w - 120);
        let w = self.text.width(Face::Body, px, &word);
        let (x, y) = ((c.w - w + 44) / 2, c.h - 40);
        app_frozen::spinner(c, x - 28, y - 10, turn);
        self.text.draw(c, Face::Body, px, x, y, pal().chalk, &word);
    }

    fn cozy(&self) -> bool {
        // Kids Mode: the case always goes into its console.
        self.kids_live() || self.animations_in_use() == pocket_ui::settings::Animations::Cozy
    }

    /// The other of Cozy and Quick, kept: from now on the look no longer decides.
    /// Settings → Look → Layout → Appearance: the next (`forward`) or previous choice, shown and kept at once.
    fn step_appearance(&mut self, forward: bool) {
        use pocket_ui::settings::Appearance;
        let n = Appearance::ALL.len();
        let i = Appearance::ALL.iter().position(|a| *a == self.appearance).unwrap_or(0);
        self.set_appearance(Appearance::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]);
    }

    /// Settings → Look → Motion: the next (`forward`) or previous level, used and kept at
    /// once, as R1 on the Look page does it.
    fn step_motion(&mut self, forward: bool) {
        use pocket_ui::motion::Motion;
        let n = Motion::ALL.len();
        let i = Motion::ALL.iter().position(|m| *m == pocket_ui::motion::level()).unwrap_or(0);
        pocket_ui::motion::set_level(Motion::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]);
        self.night_light_motion_changed();
        self.save_settings();
    }

    /// Settings → Look → Colours: the next (`forward`) or previous set, painted from the next frame on and kept.
    fn step_colours(&mut self, forward: bool) {
        // Retro has palettes of its own: the row steps through them instead.
        if pocket_ui::retro::on() {
            pocket_ui::retro::set_colours(pocket_ui::retro::colours().step(forward));
            self.cases_look_changed();
            self.save_settings();
            return;
        }
        pocket_ui::palette::set_colours(pocket_ui::palette::colours().step(forward));
        self.cases_look_changed();
        self.save_settings();
    }

    /// Settings → Look → Style: the next (`forward`) or previous style, Retro once in each of its letters
    /// (`font_style::ROW`), used from the next frame on and kept.
    fn step_font(&mut self, forward: bool) {
        pocket_ui::font_style::step_row(forward);
        self.cases_look_changed();
        self.save_settings();
    }

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
        // Whether this frame shows something on its way: then the loop asks for one more after it (`about_to_wait`).
        let moving = self.animating();
        // The Cabinet's TV playing a game (prototype, `app_cabinet_tv`): the whole frame is kept, and a frame only the
        // TV asked for paints the TV alone over it. POCKETUI_SHOWCASE_FULL=1 paints whole frames (to compare).
        surface.set_retain(self.cabinet.tv.is_live());
        // On the Brick's GPU only the rows a frame changed go into pocket-fb's texture (`damage`).
        #[cfg(feature = "brick")]
        surface.track_damage(pocket_ui::brick_gpu::upload_rows());
        if self.tv_partial_frame() && std::env::var_os("POCKETUI_SHOWCASE_FULL").is_none() {
            if let Some(drawn) = surface.draw_scaled_partial(w, h, &mut |pixels, lw, lh| self.paint_tv_partial(&mut Canvas::new(pixels, lw, lh))) {
                self.surface = surface;
                if drawn {
                    self.frame_at = Instant::now();
                    self.settle.painted(moving);
                    self.frame_due = pocket_ui::frame_pace::after_paint(started, self.frame_at, FRAME);
                    if pocket_ui::trace::on() {
                        pocket_ui::mark!("tv-partial", "{:.2}", (self.frame_at - started).as_secs_f64() * 1000.0);
                        self.trace_frame(self.frame_at - started, w.get(), h.get());
                    }
                }
                return;
            }
        }
        // A page that has just opened slides in (`app_motion`).
        self.motion_frame_begin(started);
        let drawn = surface.draw_scaled(w, h, &mut |c| {
            // Between dark and light (`app_appearance`) the frame is painted in both and the two pictures are
            // mixed, so text never passes through the grey where it would match its background.
            self.paint_in_appearance(c, &mut |app, c| {
                use pocket_ui::frame::timed;
                timed(0, || app.paint(c));
                timed(1, || app.paint_notice_over_page(c));
                timed(2, || app.motion_after_paint(c));
                timed(3, || {
                    app.sys.paint(c, &mut app.text, app.menu.is_none());
                    // The quick settings' pill is the status corner too: its minute and its thread (`app_status_corner`).
                    if app.sys.is_open() {
                        app.status_corner();
                    }
                });
                timed(4, || app.paint_leave(c));
            });
            // After a boot: the first frames come up from black (`boot_fade`).
            if let Some(dark) = self.boot_fade.darkness(Instant::now()) {
                c.fade(dark);
            }
            // The Wi-Fi bench reads what the player sees from a running pocket-ui (POCKETUI_VISIBLE_TEXT, as the
            // snapshots and the journey driver write it; docs/WIFI-BENCH.md). Nothing without the variable.
            if pocket_ui::text::boxes::on() {
                pocket_ui::text::boxes::finish_canvas(c);
            }
            // Videos of the real device: every drawn frame with its time (POCKETUI_FRAME_DUMP, `frame_dump`). A no-op unset.
            // Painted natively that is the panel's own picture (640x480), else the design canvas as before.
            let (sw, sh) = (c.stride() as u32, (c.pixels().len() / c.stride().max(1)) as u32);
            pocket_ui::frame_dump::capture(c.pixels(), sw, sh);
        });
        self.surface = surface;
        if !drawn {
            return;
        }
        self.frame_at = Instant::now();
        self.settle.painted(moving);
        #[cfg(all(target_os = "android", feature = "handheld"))]
        app_android::paint_took(self.frame_at - started, self.surface.stages(), &format!("{:?}", self.painted));
        self.frame_due = pocket_ui::frame_pace::after_paint(started, self.frame_at, FRAME);
        // Speed guard: what is on screen and the last input it already shows (docs/LATENCY.md).
        if pocket_ui::trace::on() {
            self.trace_frame(self.frame_at - started, w.get(), h.get());
            // Where the frame's time went: the painting (by part) and the rest, mostly the wait for a free buffer (BRICK-60FPS.md).
            let (s, parts) = (self.surface.stages(), pocket_ui::frame::take_sections());
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            let parts: Vec<String> = pocket_ui::frame::SECTIONS.iter().zip(parts).map(|(n, d)| format!("{n}={:.2}", ms(d))).collect();
            pocket_ui::mark!("stages", "paint {:.2} wait {:.2} {} damage={:.2} rows={}", ms(s.paint), ms(s.total.saturating_sub(s.paint + s.damage)), parts.join(" "), ms(s.damage), s.damaged_rows);
        }
        self.launch_response_painted();
        self.launch_poll();
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
            self.frozen_refresh();
        }
    }

    fn paint(&mut self, c: &mut Canvas) {
        // A on the Cabinet's playing TV: from its second frame the grow goes over the shelf kept from its first, not a
        // shelf painted anew (H700: 28 → ~10 ms a frame, `app_cabinet_tv::Grow`).
        if self.launch_at.is_some() && self.paint_grow_on_kept(c) {
            return;
        }
        // Every primary picture replaces the previous title, including the early-return pages
        // (new-version cards, Manual and shutdown). Hidden titles must leave no refresh deadline.
        self.marquee.begin_frame();
        self.pictures.visible.clear();
        self.hits.clear();
        // The shelf's size follows the screen's height (`media::shelf_scale_for`); what was made at another is stale.
        if pocket_ui::media::set_shelf_scale(c.h) {
            self.cabinet_scale_changed();
        }
        // Set again by whatever paints the notice this frame (Home, or `paint_notice_over_page`).
        self.notice_painted = false;
        // The cases' tap areas too: they are the Cabinet's only while this frame paints it, never under Settings.
        self.cabinet_frame_begin();
        // Cozy's way back from a game: where it is (`app_cabinet::eject_tick`).
        self.eject_tick(pocket_ui::frame_clock::now());
        // For the snapshot checks of Settings → Text size (`text::boxes`; nothing unless they run).
        pocket_ui::text::boxes::reset();
        // Set again when this frame paints the status corner (`app_status_corner`).
        self.corner_minute = None;
        // Which painter this frame's page is (`app_where`), set by the branch that paints it.
        self.painted = Painter::Home;
        // Powering off: "Saving…", "Switching off…" over everything.
        if let Some(msg) = self.sleep.overlay() {
            self.painted = Painter::PoweringOff;
            c.clear(pal().void);
            self.centered(c, 0, 0, c.w, c.h, &msg);
            return;
        }
        // Kids Mode paints its own few screens (`app_kids`).
        if self.kids_paint(c) {
            self.paint_eject(c, pocket_ui::frame_clock::now());
            self.painted = Painter::Kids;
            return;
        }
        // PC streaming looks for the PC for up to 12 s before Moonlight's own window comes up over this one; a tool's
        // window takes a moment too.
        if let Some(run) = self.running.as_ref().filter(|r| r.is_app() && self.launch_at.is_some_and(|t| t.elapsed() >= FADE)) {
            let msg = if run.emulator == pocket_ui::tools::EMULATOR { format!("Starting {}", run.name) } else { "Looking for your PC".to_string() };
            self.painted = Painter::AppStarting;
            c.clear(pal().void);
            self.centered(c, 0, 0, c.w, c.h, &msg);
            return;
        }
        // The manual, also over the first-start setup.
        if self.settings_page == Some(SettingsPage::Manual) {
            self.painted = Painter::Manual;
            self.in_page_column(c, |app, c| app.paint_settings(c));
            return;
        }
        // The feature cards: Settings → Tour, "New in this version" (`app_tour`).
        if self.tour.is_open() {
            self.painted = Painter::Tour;
            self.paint_tour(c);
            return;
        }
        // The look step of setup previews the real shelf in each theme; the one shown is the one in use.
        if let Some(i) = self.setup.as_ref().and_then(|s| s.previewing()) {
            self.painted = Painter::Setup;
            // Painted in the appearance the screen is going to, also while it fades there.
            let light = self.fader.light();
            // And in the interface's language (the welcome step before it can change it).
            let size = pocket_ui::type_scale::get();
            if self.look_previews.is_none()
                || self.look_previews_theme != app_look::style_mark()
                || app_look::previews_stale(
                    (self.look_previews_light, self.look_previews_lang, self.look_previews_size, &self.look_previews_clock),
                    (light, pocket_ui::i18n::current(), size, &pocket_ui::status_corner::clock()),
                )
            {
                self.look_previews_clock = pocket_ui::status_corner::clock();
                self.look_previews = Some(pocket_ui::palette::scoped(if light { 255 } else { 0 }, || self.render_look_previews()));
                self.look_previews_light = light;
                self.look_previews_theme = app_look::style_mark();
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
        // "How do you like to play?" shows two of the look previews beside the choices (`features::paint_profiles`).
        let play = self.setup.as_ref().is_some_and(|s| s.at_play_step());
        if play {
            self.painted = Painter::Setup;
            let light = self.fader.light();
            let size = pocket_ui::type_scale::get();
            if self.look_previews.is_none()
                || self.look_previews_theme != app_look::style_mark()
                || app_look::previews_stale(
                    (self.look_previews_light, self.look_previews_lang, self.look_previews_size, &self.look_previews_clock),
                    (light, pocket_ui::i18n::current(), size, &pocket_ui::status_corner::clock()),
                )
            {
                self.look_previews_clock = pocket_ui::status_corner::clock();
                self.look_previews = Some(pocket_ui::palette::scoped(if light { 255 } else { 0 }, || self.render_look_previews()));
                self.look_previews_light = light;
                self.look_previews_theme = app_look::style_mark();
                self.look_previews_lang = pocket_ui::i18n::current();
                self.look_previews_size = size;
            }
            let (previews, sample) = self.look_previews.take().unwrap_or_default();
            if let Some(setup) = self.setup.as_mut() {
                setup.play_previews = previews;
            }
            self.paint_setup_in_column(c);
            if let Some(setup) = self.setup.as_mut() {
                self.look_previews = Some((std::mem::take(&mut setup.play_previews), sample));
            }
            return;
        }
        if self.setup.is_some() {
            self.painted = Painter::Setup;
            self.paint_setup_in_column(c);
            return;
        }
        if self.settings_page.is_some() {
            self.painted = Painter::Settings;
            self.in_page_column(c, |app, c| app.paint_settings(c));
            return;
        }
        if self.port_files.is_some() && self.running.is_none() {
            self.painted = Painter::PortFiles;
            self.paint_port_files(c);
            return;
        }
        // Curator's pages, over the card or the shelf (`app_curator`).
        if self.curator.page.is_some() && self.running.is_none() {
            self.painted = Painter::Curator;
            self.paint_curator(c);
            return;
        }
        // "Runs on" (`app_emu_choice`), over the card or the console page it was opened from; over "This game" when that
        // is where it was opened.
        if self.emu_choice_on_shelf() {
            self.painted = Painter::RunsOn;
            self.paint_emu_choice(c);
            return;
        }
        // "This game" (`app_this_game`), over the card.
        if self.this_game_on_shelf() {
            self.painted = Painter::ThisGame;
            self.paint_this_game(c);
            return;
        }
        // A port's Performance & fan, over its card (`app_port_perf`).
        if self.port_perf_open() {
            self.painted = Painter::PortPerf;
            self.paint_port_perf(c);
            return;
        }
        // A game's screenshots, over its card (`app_gallery`).
        if self.gallery_over_card() {
            self.painted = Painter::Gallery;
            self.paint_gallery(c);
            return;
        }
        // A save's two versions, over its card (`app_sync_conflicts`).
        if self.conflicts_over_card() {
            self.painted = Painter::Conflicts;
            self.paint_conflicts(c);
            return;
        }
        if self.card.is_some() && self.running.is_none() {
            self.painted = Painter::Card;
            self.paint_card(c);
            return;
        }
        if self.console.is_some() && self.running.is_none() {
            self.painted = Painter::Console;
            self.paint_console(c);
            return;
        }
        // The search (`app_search`); a game's card opened from it is painted over it, above.
        if self.find.is_open() && self.running.is_none() {
            self.painted = Painter::Search;
            self.in_column(c, |app, c| app.paint_search(c));
            return;
        }
        if self.switcher.is_some() {
            self.painted = Painter::Switcher;
            self.paint_switcher(c);
            return;
        }
        if self.menu.is_some() && self.sys.quick_over_game() {
            self.painted = Painter::Pause;
            // The quick settings opened from the pause menu: the paused game behind them, darkened only as far as
            // the rows stay readable, so a brightness change can be judged on the game (`quick_paint` draws the rows).
            self.paint_game_behind(c, VOID, QUICK_OVER_GAME_FADE);
            return;
        }
        // The Big shelf's Continue or Stop, also over the game picture.
        if self.big_pause() {
            self.painted = Painter::BigPause;
            pocket_ui::palette::scoped(0, || self.kids_paint_pause(c));
            return;
        }
        if self.menu.is_some() {
            self.painted = Painter::Pause;
            // The pause menu sits over the game picture: dark in both appearances.
            pocket_ui::palette::scoped(0, || self.paint_menu(c));
            return;
        }
        // The Shelf look is painted in the column on a wide Linux screen (`app_column`); the other looks fill it their own way.
        if self.theme == Theme::Shelf {
            self.paint_shelf_look(c);
        } else {
            self.paint_home(c, true);
        }
    }

    /// Home's page up to what goes over it (the way back, the launch): false when it was a page of its own (no games,
    /// the starter) and nothing goes over it.
    fn paint_home_page(&mut self, c: &mut Canvas, with_hints: bool, big: bool) -> bool {
        // The Cabinet clears the page itself and paints its own top line with the shelf's name (`paint_cabinet`): a
        // second clear of the whole page here cost the Brick a millisecond a frame for nothing (docs/TRIMUI-BRICK.md).
        let own_page = !big && self.theme == Theme::Cabinet && !self.items.is_empty() && !self.starter_home();
        if big {
            app_big_shelf::paint_background(c);
        } else if !own_page {
            c.clear(pal().void);
        }
        // Set again by the Shelf look's label: the game card rises out of it (`app_motion`).
        self.motion.sel_y = None;
        // Where we are, left; time, Wi-Fi and battery, right (`app_status_corner`).
        let title = self.screen_title();
        let corner_left = self.paint_top_line(c, title.as_deref(), pal().dust);

        // A card with no games: a calm hint and the one thing to press (`app_starter`), not an empty shelf.
        if self.starter_home() {
            self.paint_starter_home(c, with_hints);
            return false;
        }
        // B on that hint: the look's own shelf, empty on purpose, with a line saying so (`app_starter`).
        if self.empty_home_around() && self.items.is_empty() {
            self.paint_empty_home_around(c, with_hints);
            return false;
        }
        if self.items.is_empty() {
            // Shelf space's crate with nothing in it: its empty box and a line saying every game is on the shelf.
            if self.screen == Screen::Crate {
                self.paint_crate(c);
                if with_hints {
                    self.paint_hints(c);
                }
                return false;
            }
            if self.empty_ports() { self.paint_empty_ports(c, with_hints); return false; }
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
                return false;
            }
            let msg = self.load_error.clone().unwrap_or_else(|| "No games yet".into());
            let dy = pocket_ui::ui_art::above_words(c, pocket_ui::ui_art::Art::NoGames, MARGIN, false, EMPTY_ART_TOP, 300, 50);
            self.text.draw(c, Face::Title, 50, MARGIN, 300 + dy, pal().chalk, &msg);
            // The dArkOS port reads dArkOS's own folders (start.sh's view); /storage/roms is its private view of them.
            let hint = if pocket_ui::card_health::image_card() { pocket_ui::card_health::IMAGE_CARD_NOTE }
                else if pocket_ui::device::darkos_port() { app_starter::DARKOS_OWN_GAMES }
                else { "Copy games into /storage/roms/<system> and restart." };
            for (i, line) in self.text.wrap_prose(Face::Body, 28, hint, c.w - 2 * MARGIN).iter().enumerate() {
                self.text.draw(c, Face::Body, 28, MARGIN, 350 + dy + i as i32 * 38, pal().dust, line);
            }
            return false;
        }

        match self.theme {
            _ if big => self.big_paint(c),
            _ if self.screen == Screen::Crate => self.paint_crate(c),
            Theme::Shelf => self.paint_shelf(c),
            Theme::Split => self.paint_split(c),
            Theme::Cabinet => self.paint_cabinet(c),
            // Off Home (a Curator list opened from Settings): a plain list, the one the Minimal look had until 0.4.
            Theme::Big => self.paint_minimal(c),
        }

        if big {
            // Its hints are its own, and they don't wait for anything else (`big_paint`).
        } else if with_hints {
            self.paint_hints(c);
            // The letter of an L2/R2 jump (`app_search`).
            self.paint_letter_flash(c);
        }
        // Small, left of the status corner: "Saves synced" after a game. While covers and descriptions download in the
        // background only a quiet dot shows, never "Covers 41 of 184" (polish review 12.11.: a count is a technician's text).
        let right = pocket_ui::status_corner::room_left_of(corner_left, c.w - MARGIN);
        if let Some(note) = self.sync_note().filter(|_| with_hints) {
            self.text.draw_right(c, Face::Body, 24, right, 72, pal().dust, &note);
        } else if with_hints && self.background_work_sign() {
            let r = pocket_ui::status_corner::SIGN_RADIUS;
            c.disc(right - r, 72 - 9, r, pal().dust);
        }

        true
    }

    fn paint_home(&mut self, c: &mut Canvas, with_hints: bool) {
        // The toast at the end stands over the hint bar for its moment: the hints give way meanwhile, so its pill never
        // cuts a key in half ("Open means your moment is waiting" over HOME on Android, the crate's "… is on the shelf
        // now", 19.11.2026; tests/hint_bar.rs `:toasted`).
        self.toast_over_hints = self.toast.as_ref().is_some_and(|(m, at)| toast_age(*at) < toast_len(m));
        self.paint_home_inner(c, with_hints);
        self.toast_over_hints = false;
    }

    fn paint_home_inner(&mut self, c: &mut Canvas, with_hints: bool) {
        let big = self.big_live();
        // The Shelf or Cabinet launch: once the page under the scene stands still, copy it (and reuse its dim pass)
        // instead of painting the whole home page every frame (`app_cabinet::LaunchShelf`).
        let (restored, dimmed) = self.launch_shelf_restore(c, big, with_hints);
        if !restored {
            if !self.paint_home_page(c, with_hints, big) {
                return;
            }
            if self.launch_at.is_some() {
                app_cabinet::LAUNCH_PAGES.with(|n| n.set(n.get() + 1));
            }
            self.launch_shelf_keep(c, big, with_hints);
        }
        self.launch_shelf_shadow(c);
        // The covers the selection is heading for, read while it moves or rests (`app_cover_prefetch`); not while a game starts.
        if self.launch_at.is_none() {
            self.cover_prefetch();
        }
        // The way back from a game over the live shelf (`app_cabinet::paint_eject`).
        self.paint_eject(c, pocket_ui::frame_clock::now());
        // Not on the way in or out: a finished scene is let go (`paint_insert`).
        if self.theme == Theme::Cabinet && self.launch_at.is_none() && !self.ejecting() {
            self.paint_insert(c, pocket_ui::frame_clock::now());
        }
        if self.launch_at.is_some() && self.paint_showcase_grow(c) {
            // A on the Cabinet's playing TV: its picture grows into the game instead of the launch fade.
            self.paint_loading(c);
        } else if let Some(t) = self.launch_at {
            // With the launch animation the fade waits for the cartridge or disc to go in (`app_cabinet`).
            let p = (pocket_ui::frame_clock::now().saturating_duration_since(t).saturating_sub(self.cabinet.launch_delay()).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
            // The Shelf look's case goes into its console over the shelf (the Cabinet paints its own with the cabinet).
            if self.theme == Theme::Shelf || self.theme == Theme::Cabinet || big {
                self.paint_insert_with(c, pocket_ui::frame_clock::now(), dimmed);
            }
            let p = self.cabinet.launch_fade_at(p);
            self.cabinet.launch_veil(c, p);
            // On the shelf the selected case's front stays lit while everything else goes dark (no animation).
            if (self.theme == Theme::Shelf || big) && !self.cabinet_inserting() {
                self.shelf_relight(c);
            }
            // And where the game was left fades in over it (`launch_picture`); not behind the quick settings, which
            // can open over the game later.
            if let Some(pic) = self.launch_picture.as_ref().filter(|_| !self.sys.is_open()) {
                pocket_ui::launch_picture::paint(c, pic, (p * 255.0) as u8);
            }
            self.paint_where_was_i(c, p);
            self.paint_frozen_wait(c);
            self.paint_loading(c);
        }

        // A save with two versions found since the last time: told once (`app_sync_conflicts`).
        if with_hints {
            self.sync_conflict_notice();
            // Settings or lists the card refused to save: told once (`app_save_trouble`).
            self.card_refusal_notice();
        }
        let notice_bottom = if let Some(n) = &self.notice { n.paint(c, &mut self.text) } else { 0 };
        self.notice_painted = notice_bottom > 0;
        if let Some((name, at)) = self.toast.clone() {
            if toast_age(at) < toast_len(&name) {
                // Comes down into place and fades in and out (`motion::paint_toast`). At the bottom, over the hint
                // line for its moment: under the top edge it covered the game's name (taste pass 0.2), and a notice
                // card at the top keeps clear of it (both sat at the top and garbled each other, monkey 2026-10-30).
                let top = pocket_ui::kit::footer_baseline(c.h) - 36;
                // A longer line (German) is shortened to the screen, as on the card.
                let name = self.text.fit(Face::Body, 28, &name, c.w - 2 * MARGIN - 48);
                let w = self.text.width(Face::Body, 28, &name) + 48;
                // For the snapshot checks of Settings → Text size: the page under the pill is covered, not run into.
                pocket_ui::text::boxes::covered_since(usize::MAX, c, (c.w / 2 - w / 2, top, w, 52));
                let text = &mut self.text;
                pocket_ui::motion::paint_toast(c, at, at + toast_age(at), Some(toast_len(&name)), (c.w / 2 - w / 2, top, w, 52), 20.0, |c, dy| {
                    pocket_ui::retro::pill(c, c.w / 2 - w / 2, top + dy, w, 52, 26, KEYCAP);
                    text.draw(c, Face::Body, 28, c.w / 2 - w / 2 + 24, top + 36 + dy, CHALK, &name);
                });
            }
        }
    }

    /// The plain list: white names on black, the selected one inverted in a pill. The Minimal look until 0.4; now the
    /// list Easy shows off Home (a Curator list opened from Settings).
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
        pocket_ui::retro::select(&mut band, 48, ((hl - first_f) * row as f32).round() as i32, pill_w, 60, 30, pal().pick(DARK.select, LIGHT.text));
        for (i, k) in app_motion::rows_in_view(first_f, visible, self.items.len()) {
            let y = (k * row as f32).round() as i32;
            if (0.0..visible as f32 - 0.5).contains(&k) {
                self.hits.row(Area::new(48, top + y, band.w - 96, 60), i);
                self.motion.row(48, top + y, max_w + 48, row);
            }
            let (title, _, _, _) = self.describe(self.items[i]);
            let (mut max, fav) = (max_w, self.is_favorite(self.items[i]));
            let fg = mix(pal().text, pal().void, (app_motion::under(i, hl) * 255.0) as u32);
            if self.is_frozen_item(self.items[i]) {
                max -= self.frozen_word_width() + 20;
                self.paint_frozen_word(&mut band, 72 + max_w, y + 41, fg);
            }
            let (px, title) = self.system_name_for_row(self.items[i], Face::Body, 36, 30, title, max);
            self.draw_title(&mut band, Face::Body, px, 72, y + 43, max, fg, &title, i == self.sel, fav);
        }
        self.hits.list(self.sel, self.items.len(), 0.0, row as f32);
    }

    /// Split: list on the left, a detail panel in the system's colour on the right (classic frontend layout).
    fn paint_split(&mut self, c: &mut Canvas) {
        let bg = pal().split_bg;
        c.clear(bg);
        let title = self.screen_title();
        self.paint_top_line(c, title.as_deref(), pal().dust);
        // The list is 500 wide; a canvas under 1280 wide and 960 lines (the Brick's 1024x768) keeps its share of the width,
        // as on the Plus, so the panel beside it is as wide as there (tests/brick_look_proportions.rs).
        let list_w = if c.w < 1280 && c.h < 960 { c.w * 500 / 1280 } else { 500 };
        let (top, row) = (112, 60);
        let visible = ((c.h - 120 - top) / row).max(1) as usize;
        let first = window_start(self.sel, visible, self.items.len());
        // The bar glides to the selection and takes on its system's colour on the way (`app_motion`).
        let (first_f, hl) = self.motion.list_at(app_motion::List::Home, first, self.sel, Instant::now());
        let stripe_of = |app: &App, i: usize| app.items.get(i).map_or(0, |item| stripe(&app.describe(*item).1));
        let (lo, t) = (hl.floor().max(0.0) as usize, hl - hl.floor());
        let bar = if t > 0.0 { mix(stripe_of(self, lo), stripe_of(self, lo + 1), (t * 255.0) as u32) } else { stripe_of(self, lo) };
        let mut band = c.rows(top, top + visible as i32 * row);
        pocket_ui::retro::select(&mut band, 48, ((hl - first_f) * row as f32).round() as i32, list_w, row - 6, 10, bar);
        for (i, k) in app_motion::rows_in_view(first_f, visible, self.items.len()) {
            let y = (k * row as f32).round() as i32;
            if (0.0..visible as f32 - 0.5).contains(&k) {
                self.hits.row(Area::new(48, top + y, list_w, row), i);
                self.motion.row(48, top + y, list_w, row);
            }
            let (title, _, _, _) = self.describe(self.items[i]);
            let fav = self.is_favorite(self.items[i]);
            let fg = mix(pal().split_dim, 0xffffff, (app_motion::under(i, hl) * 255.0) as u32);
            let mut max = list_w - 40;
            if self.is_frozen_item(self.items[i]) {
                max -= self.frozen_word_width() + 16;
                self.paint_frozen_word(&mut band, 68 + list_w - 40, y + 36, fg);
            }
            let (px, title) = self.system_name_for_row(self.items[i], Face::Body, 28, 24, title, max);
            self.draw_title(&mut band, Face::Body, px, 68, y + 38, max, fg, &title, i == self.sel, fav);
        }
        self.hits.list(self.sel, self.items.len(), 0.0, row as f32);
        let Some(item) = self.items.get(self.sel).copied() else { return };
        let (title, system, side, meta) = self.describe(item);
        let (px, pw) = (48 + list_w + 40, c.w - (48 + list_w + 40) - 48);
        let color = stripe(&system);
        // The panel is 420 tall on 1080 lines and gives up what a shorter screen lacks down to 960 (`split_panel_h`);
        // `short` is what it gave up, and the name and the facts under it move up by as much.
        let short = 420 - split_panel_h(c.h);
        pocket_ui::retro::window(c, px, top, pw, 420 - short, 18, mix(color, bg, 90));
        SPLIT_SEEN.set([px, top, pw, 420 - short, 0, 0, 0, 0, c.w, c.h]);
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
        let panel_lines = panel.map(|p| self.text.wrap_prose_at_most(Face::Body, 28, &p.line, pw, 2));
        // Box art, then the game running and its title screen fading in turn (`app_pictures`).
        let preview = (px + 24, top + 24, pw - 48, 420 - short - 48);
        if !shown && !self.paint_preview(c, item, preview, mix(color, bg, 90)) && !self.paint_homemade_preview(c, item, preview) {
            // A system card shows its console above the game count.
            if let Some(art) = self.system_art(item, pw - 48, 310 - short) {
                let (aw, ah) = (art.w as i32, art.h as i32);
                c.blit_sprite(&art, px + (pw - aw) / 2, top + 20 + (310 - short - ah) / 2, 0);
                SPLIT_SEEN.set([px, top, pw, 420 - short, px + (pw - aw) / 2, top + 20 + (310 - short - ah) / 2, aw, ah, c.w, c.h]);
            }
            // A line too wide for the panel (Just Play's "One press" in Retro) goes smaller before it is cut.
            let (big_px, big) = self.text.fit_sizes(Face::Title, &[67, 56, 48, 40], &big, pw - 64);
            self.text.draw(c, Face::Title, big_px, px + 32, top + 380 - short, mix(pal().on_wash, color, 60), &big);
        }
        // A game's name whole, in up to three lines, its subtitle smaller; the meta line stands where the third would
        // end, so nothing moves from one game to the next (`title_block`).
        let mut meta_at = top + 548 - short;
        if matches!(item, Item::Game(..) | Item::Recent(_)) {
            let lines = pocket_ui::title_block::name_layout(&mut self.text, Face::Title, 50, &title, pw, 3);
            pocket_ui::title_block::paint(&mut self.text, c, Face::Title, &lines, px, top + 500 - short, pal().text, pal().split_dim, false);
            meta_at += 2 * pocket_ui::title_block::step(&self.text, 50);
        } else if matches!(item, Item::System(_)) {
            self.text.literal(|text| {
                let title = text.fit(Face::Title, 50, &title, pw);
                text.draw(c, Face::Title, 50, px, top + 500 - short, pal().text, &title);
            });
        } else {
            let title = self.text.fit(Face::Title, 50, &title, pw);
            self.text.draw(c, Face::Title, 50, px, top + 500 - short, pal().text, &title);
        }
        // A system with a fact sheet: its headline and short facts in the room under the name.
        if self.paint_system_facts(c, item, px, top + 548 - short, pw, pocket_ui::kit::content_bottom(c.h)) {
            return;
        }
        if let Some(lines) = panel_lines {
            for (k, line) in lines.iter().enumerate() {
                self.text.draw(c, Face::Body, 28, px, top + 548 - short + k as i32 * pocket_ui::type_scale::line(28, 12), pal().split_dim, line);
            }
            return;
        }
        // Split has room for a second line under a row's line too ("Less browsing. More playing."), not a game's.
        let lines = if matches!(item, Item::Game(..) | Item::Recent(_)) { vec![self.text.fit(Face::Body, 28, &meta, pw)] } else { self.text.wrap_prose_at_most(Face::Body, 28, &meta, pw, 2) };
        for (k, line) in lines.iter().enumerate() {
            self.text.draw(c, Face::Body, 28, px, meta_at + k as i32 * pocket_ui::type_scale::line(28, 12), pal().split_dim, line);
        }
    }

    /// Only system rows use a known short system name when their full name still cannot fit one size smaller.
    /// Game names keep their own title painter and marquee.
    fn system_name_for_row(&mut self, item: Item, face: Face, px: u32, smaller: u32, title: String, room: i32) -> (u32, String) {
        let Item::System(system) = item else { return (px, title) };
        if self.text.width(face, px, &title) <= room { return (px, title); }
        if self.text.width(face, smaller, &title) <= room { return (smaller, title); }
        (px, system_short(&self.systems, &self.systems[system].name))
    }

    /// (title, system id for the stripe, text on the spine's right, meta line on the label)
    fn describe(&self, item: Item) -> (String, String, String, String) {
        match item {
            Item::Streaming => ("PC streaming".into(), "streaming".into(), "Moonlight".into(), "Play your PC's games over Wi-Fi with Moonlight".into()),
            Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover => self.curator_describe(item),
            Item::Search => ("Search".into(), "search".into(), "All games".into(), "Find any game by its name, typos and all".into()),
            Item::JustPlay => (shelf::JUST_PLAY.into(), "curator".into(), shelf::JUST_PLAY_SIDE.into(), shelf::JUST_PLAY_LINE.into()),
            Item::Crate => {
                // Shelf space (`app_crate`): how many are in the crate, and the shelf's free places.
                let n = self.crate_count();
                let count = if n == 1 { "1 game".to_string() } else { format!("{n} games") };
                let line = match self.shelf_space.free() {
                    0 => format!("{count} in the crate, the shelf is full"),
                    1 => format!("{count} in the crate, 1 place free on the shelf"),
                    f => format!("{count} in the crate, {f} places free on the shelf"),
                };
                ("Crate".into(), "crate".into(), count, line)
            }
            Item::Favorites => {
                let n = self.favorite_items().len();
                let count = if n == 1 { "1 game".to_string() } else { format!("{n} games") };
                ("Favourites".into(), "favorites".into(), count.clone(), format!("{count} you starred"))
            }
            Item::Recent(i) => {
                let r = &self.recents.list[i];
                // Where the player is, read from the game's own save, takes the line beside Continue (`app_case_knows`).
                let meta = self.case_or_achievements_line(&r.system, &r.path).unwrap_or_else(|| format!("{}, {}, {} played", system_label(&self.systems, &r.system), recents::ago(r.last_played), recents::duration(r.play_secs)));
                (r.name.clone(), r.system.clone(), system_short(&self.systems, &r.system), meta)
            }
            Item::System(s) => {
                let sys = &self.systems[s];
                // Shelf space: the games on the shelf (the rest are in the crate).
                let n = sys.games.iter().filter(|g| self.shelf_space.on_shelf(&g.path)).count();
                let count = if n == 1 { "1 game".to_string() } else { format!("{n} games") };
                (sys.fullname.clone(), sys.name.clone(), count.clone(), count)
            }
            Item::Game(s, g) => {
                let sys = &self.systems[s];
                let game = &sys.games[g];
                let played = self.recents.list.iter().find(|r| r.path == game.path);
                let meta = played.map_or_else(|| self.play_evidence(&game.path).untimed().to_string(), |r| recents::played_line(Some(r)));
                let meta = meta_with_status(self.library.status(&game.path), &meta);
                // The Favorites shelf mixes systems, and one title can be on several (Blockamok Remix is on the 3DS and the
                // Wii): name the system on the spine and on the label, like the recent games on Home do.
                // Rediscover says why a game is there ("played a year ago today", "last played 412 days ago").
                let why = self.rediscover_why(&game.path);
                // A game whose moment waits (its case stands open, "Continue"): where the player is in it, from its own
                // save, when that is known (`app_case_knows`). Rediscover's reason stays first.
                let line = (why.is_none() && self.is_frozen_item(item)).then(|| self.case_or_achievements_line(&sys.name, &game.path)).flatten();
                let meta = why.unwrap_or(meta);
                if matches!(self.screen, Screen::Favorites | Screen::Shelf(_) | Screen::Sticker(_) | Screen::LensResults | Screen::Connection | Screen::Rediscover) {
                    let meta = line.unwrap_or_else(|| meta_with_system(&sys.fullname, &meta));
                    return (game.name.clone(), sys.name.clone(), system_short(&self.systems, &sys.name), meta);
                }
                let meta = line.unwrap_or(meta);
                (game.name.clone(), sys.name.clone(), String::new(), meta)
            }
        }
    }

    /// The meta line of `describe` in short ("Super Nintendo · Completed · 3 days ago · 6 h"), for the Shelf look when the
    /// whole one does not fit beside the box (`app_cabinet::meta_choice`; 18.11.2026, Brick Favourites: "Super Nintendo,
    /// last played 3 days ago, 6 h …"). The same facts, " · " between them: each part is translated on its own. None where
    /// the line is the save's progress or the RetroAchievements tally (short already) or not a game's.
    fn meta_short(&self, item: Item) -> Option<String> {
        match item {
            Item::Recent(i) => {
                let r = self.recents.list.get(i)?;
                if self.case_or_achievements_line(&r.system, &r.path).is_some() {
                    return None;
                }
                Some([system_label(&self.systems, &r.system), recents::ago(r.last_played), recents::duration_short(r.play_secs)].join(" · "))
            }
            Item::Game(s, g) => {
                let sys = self.systems.get(s)?;
                let game = sys.games.get(g)?;
                let why = self.rediscover_why(&game.path);
                if why.is_none() && self.is_frozen_item(item) && self.case_or_achievements_line(&sys.name, &game.path).is_some() {
                    return None;
                }
                let mut parts = Vec::new();
                if matches!(self.screen, Screen::Favorites | Screen::Shelf(_) | Screen::Sticker(_) | Screen::LensResults | Screen::Connection | Screen::Rediscover) {
                    parts.push(sys.fullname.clone());
                }
                match why {
                    // Rediscover's reason stands for the rest, as in `describe`.
                    Some(why) => parts.push(why),
                    None => {
                        let status = self.library.status(&game.path);
                        if status != pocket_ui::library_meta::Status::NotStarted {
                            parts.push(status.label().to_string());
                        }
                        match self.recents.list.iter().find(|r| r.path == game.path) {
                            Some(r) => {
                                parts.push(recents::ago(r.last_played));
                                parts.push(recents::duration_short(r.play_secs));
                            }
                            None => parts.push(self.play_evidence(&game.path).untimed().to_string()),
                        }
                    }
                }
                Some(parts.join(" · "))
            }
            _ => None,
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
        // The crate: A takes the game out onto the shelf; while one is picked to make room, A swaps (`app_crate`).
        if self.screen == Screen::Crate {
            hints = self.crate_hints(hints);
        }
        // A frozen game goes on where it was (`app_frozen`).
        if selected.is_some_and(|i| self.is_frozen_item(i)) {
            hints.iter_mut().filter(|h| h.1 == "Play").for_each(|h| h.1 = "Continue");
        }
        // The cabinet's top shelf: Up opens the search (`app_search`).
        hints.extend(self.search_hint());
        // Below Home START's Settings gives way to the game's own keys: Y Star / Unstar shows (Settings is on Home's bar).
        let _order = pocket_ui::kit::start_yields(shelf::start_yields(selected, home));
        self.paint_hint_row(c, &hints);
    }

    /// Button hints along the bottom edge; the ones for a single button can be tapped. When they change at rest, the
    /// old ones fade out as the new ones fade in; during a move the new ones come at once (`app_motion`).
    fn paint_hint_row(&mut self, c: &mut Canvas, hints: &[(&str, &str)]) {
        if self.toast_over_hints {
            return;
        }
        let now = Instant::now();
        let moving = self.moving_now(now);
        if let Some((old, p)) = self.motion.hints_fade(hints, now, moving) {
            let old: Vec<(&str, &str)> = old.iter().map(|(k, l)| (k.as_str(), l.as_str())).collect();
            let (y, h) = (pocket_ui::kit::hint_bar_top(c.h) - 8, pocket_ui::kit::hint_h(c.h) + 16);
            // Only from the left end of the longer of the two pills: mixing the whole width cost the Brick 4 ms a frame
            // all through the fade, which starts with nearly every move in the Cabinet (docs/TRIMUI-BRICK.md).
            let left = |app: &mut Self, hints: &[(&str, &str)]| pocket_ui::kit::page_hints_left(&mut app.text, c.w, c.h, hints);
            let x = (left(self, &old).min(left(self, hints)) - 8).max(0);
            // The old ones go faster than the new ones come, so the two rows hardly overlap where the hints moved.
            pocket_ui::motion::layer(c, x, y, c.w - x, h, ((1.0 - p).powi(3) * 255.0) as u8, |c| self.paint_hint_keys(c, &old, false));
            // For the snapshot checks of Settings → Text size: the old hints are on their way out, not read.
            pocket_ui::text::boxes::painted_over(c, (0, y, c.w, h));
            pocket_ui::motion::layer(c, x, y, c.w - x, h, (p * 255.0) as u8, |c| self.paint_hint_keys(c, hints, true));
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

/// A game's meta line with its status in front ("Completed, last played 3 days ago, 6 h 00 min in total"); as it is for a
/// game not started. The tables have the status and the line together ("Completed, {t}"): put together without them the
/// status stayed English ("Nintendo 64, completed, zuletzt gespielt …", 18.11.2026; `meta_words_tests`).
fn meta_with_status(status: pocket_ui::library_meta::Status, meta: &str) -> String {
    match status {
        pocket_ui::library_meta::Status::NotStarted => meta.to_string(),
        status => format!("{}, {}", status.label(), meta.to_lowercase()),
    }
}

/// A game's meta line on a shelf of several systems, its system first ("Super Nintendo, completed, last played …").
fn meta_with_system(system: &str, meta: &str) -> String {
    format!("{system}, {}", meta.to_lowercase())
}

/// `POCKETUI_SNAPSHOT_SIZE=<w>x<h>` for `--snapshot`; without it (or when it doesn't read as a size) the device's
/// screen (`device::Device::screen`: 1240 x 1080 on the Mini V2 and anything unknown, `POCKETUI_DEVICE=rp5` 1920 x 1080).
pub(crate) fn snapshot_dims() -> (u32, u32) {
    let panel = snapshot_panel_dims();
    let logical = if std::env::var_os("POCKETUI_SNAPSHOT_NATIVE").is_some() { panel } else { pocket_ui::scaled_layout::logical_size(panel) };
    // As the live frame does it: a pixel font knows how many canvas pixels one of the screen's is (`scaled_layout::step`).
    pocket_ui::scaled_layout::set_step(logical, panel);
    logical
}

pub(crate) fn snapshot_panel_dims() -> (u32, u32) {
    snapshot_size(std::env::var("POCKETUI_SNAPSHOT_SIZE").ok().as_deref(), pocket_ui::device::current().screen())
}

fn snapshot_size(env: Option<&str>, screen: (u32, u32)) -> (u32, u32) {
    env.and_then(|s| s.trim().split_once(['x', 'X']))
        .and_then(|(w, h)| Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?)))
        .filter(|&(w, h)| (320..=4096).contains(&w) && (240..=4096).contains(&h))
        .unwrap_or(screen)
}

/// Renders one pause-menu screen to a PNG without a window, for checking layouts on the desktop:
/// `pocket-ui --snapshot <main|options|picture|extras|emulator[:sel]|category[:cat[:sel]]|controls[:sel]> <out.png> [background.png] [game.png]`.
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
        "cheats-empty" => return Some(CheatMenu::empty()),
        "cheats-get" | "cheats-downloading" | "cheats-offline" | "cheats-unavailable" => return Some(CheatMenu::download_snapshot(state)),
        _ => return pocket_ui::cheats::CheatMenu::load(tmp, "gbc", rom),
    };
    let root = tmp.join("cht-demo");
    let folder = root.join(pocket_ui::cheats::folder("gbc").unwrap_or("Nintendo - Game Boy Color"));
    let _ = std::fs::create_dir_all(&folder);
    let _ = std::fs::write(folder.join("Pokemon - Crystal Version (USA, Europe).cht"), body);
    CheatMenu::load_in(tmp, "gbc", rom, &[root], false)
}

/// Six made-up hours of battery history in `data_dir` for the Battery page's snapshot: GBA, the menus, PlayStation, the
/// last 20 minutes on the charger (the battery itself comes from `POCKETUI_BATTERY`). `slept`: a night of deep sleep
/// before them, 7 h 40 min that cost 3 points (the Battery page's sleep line).
fn snapshot_battery_samples(data_dir: &std::path::Path, slept: bool) {
    use pocket_ui::battery::Sample;
    let now = recents::now_secs();
    let mut h = pocket_ui::battery_history::History::new();
    if slept {
        for (m, capacity) in [(840u64, 98u8), (380, 95)] {
            let s = Sample { t: now - m * 60, capacity, charging: false, full: false, mv: 3900, ma: -60, temp: Some(250), tag: 0 };
            h.push(s, Some(pocket_ui::battery_history::SLEEP_TAG));
        }
    }
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

/// A snapshot's own folder for the made-up save states, cheat files and game settings some states need: one per
/// process, removed when the snapshot is done. A folder shared by all of them (`pocketui-snapshot`) let snapshots running
/// side by side (`scripts/size-sweep.sh`, JOBS=4) delete each other's states: a page showed slot 4 in one run and 5 in
/// the next, one cheat or two.
fn snapshot_tmp() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("pocketui-snapshot-{}", std::process::id()))
}

fn snapshot(app: &mut App, args: &[String]) {
    // Snapshots are settled pictures: use the same decoder and scaler synchronously, as before covers
    // moved to the picture worker. Live frames keep drawing while that worker reads an image.
    app.pictures.sync = true;
    SNAPSHOT_RUN.store(true, std::sync::atomic::Ordering::Relaxed);
    // POCKETUI_SNAPSHOT_PROGRESS=<case>: every game's line from its save says this (`app_case_knows::snapshot_progress`).
    app.case_knows.made_up = std::env::var("POCKETUI_SNAPSHOT_PROGRESS").ok().and_then(|c| app_case_knows::snapshot_progress(&c));
    // POCKETUI_SNAPSHOT_ART_ROM=<rom>: its pictures for that made-up team (`app_case_knows::snapshot_art`).
    // POCKETUI_SNAPSHOT_RA=<unlocked>/<total>: signed in, every game's RetroAchievements tally (`app_achievements`).
    if let Ok(spec) = std::env::var("POCKETUI_SNAPSHOT_RA") {
        app.achievements.make_up(&spec);
    }
    app.case_knows.made_up_art = std::env::var("POCKETUI_SNAPSHOT_ART_ROM").ok().and_then(|r| app_case_knows::snapshot_art(app.case_knows.made_up.as_ref(), &r));
    pocket_ui::status_corner::freeze_clock();
    // A `:device=` at the end was settled at the start (`device::snapshot_state`); the state is what is before it.
    let state = args.first().map(|s| pocket_ui::device::snapshot_state(s).0.to_string());
    // `<state>:bgrun`: covers, pictures and descriptions load in the background, as on a card that was just filled
    // (the shelf shows a dot for it, never a count: tests/shelf_calm.rs).
    if state.as_deref().is_some_and(|s| s.starts_with("bench-"))
        || ["POCKETUI_FRAME_BENCH", "POCKETUI_BENCH_FRAMES", "POCKETUI_SNAPSHOT_LAUNCH_SERIES"].iter()
            .any(|key| std::env::var(key).is_ok_and(|v| !v.is_empty() && v != "0")) {
        app_shrink_wrap::bench_run();
    }
    // `<state>:toasted`: the shelf's toast stands over the page for its moment, the longest line Home says by itself
    // (tests/hint_bar.rs: no key of the bar half under its pill).
    let toasted = state.as_deref().is_some_and(|s| s.ends_with(":toasted"));
    let state = state.map(|s| s.strip_suffix(":toasted").map(str::to_string).unwrap_or(s));
    if toasted {
        app.toast = Some((pocket_ui::first_game::OPEN_CASE_NOTE.to_string(), Instant::now()));
    }
    let bgrun = state.as_deref().is_some_and(|s| s.ends_with(":bgrun"));
    let state = state.map(|s| s.strip_suffix(":bgrun").map(str::to_string).unwrap_or(s));
    if bgrun {
        app.cover_run.begin_run(184, 12);
        app.cover_run.progress(41, 196);
        app.text_run.begin(198);
        app.text_run.progress(16, 198);
    }
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
    if state.starts_with("battery") && (state.ends_with(":samples") || state.ends_with(":sleep")) {
        snapshot_battery_samples(&app.data_dir, state.ends_with(":sleep"));
    }
    // What a running pocket-ui reads on threads (shown as "Loading…" until then) is read before the picture.
    pocket_ui::battery_history::load_now(&app.data_dir);
    // The player's EmulationStation gamelists (`app_gamelist`): the card's description and the covers come from them.
    app.gamelist_load_now();
    app.port_perf_wait_loaded();
    // "share-shelf[:<n>][:plain]": the picture of the shelf to post (`app_share_shelf`).
    if state == "share-shelf" || state.starts_with("share-shelf:") {
        app.share_shelf_snapshot(state, out);
        return;
    }
    // "big-…": the Big shelf's screens (`app_big_shelf`).
    if state.starts_with("big-") {
        let background = args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)));
        if app.big_snapshot(state, background, w, h) {
            let mut buf = vec![0u32; (w * h) as usize];
            let mut c = snap_canvas(&mut buf, w, h);
            if !state.starts_with("big-jump") && !state.starts_with("big-letter") {
                app.moved_at = Instant::now() - Duration::from_secs(5);
            }
            app.paint(&mut c);
            // Where the big case stands, for the tests that keep it clear of the top line (tests/big_shelf.rs).
            if let Some((front, x, y)) = app.cabinet.shelf_lit.as_ref() {
                println!("big-case x={x} y={y} w={} h={}", front.w, front.h);
            }
            if eject_shot(app, &mut buf, w, h, out) { return; }
            write_png(&buf, w, h, out);
        }
        return;
    }
    // "kids-…": Kids Mode's screens (`app_kids`).
    if state.starts_with("kids-") {
        let background = args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)));
        if app.kids_snapshot(state, background, w, h) {
            let mut buf = vec![0u32; (w * h) as usize];
            let mut c = snap_canvas(&mut buf, w, h);
            app.moved_at = Instant::now() - Duration::from_secs(5);
            if matches!(state.as_str(), "kids-flow:games-thread" | "kids-shelf-thread") {
                app.kids_cover_settle(&mut c);
            } else {
                app.paint(&mut c);
            }
            app.sys.paint(&mut c, &mut app.text, app.menu.is_none());
            if eject_shot(app, &mut buf, w, h, out) { return; }
            write_png(&buf, w, h, out);
        }
        return;
    }
    // "where-was-i[:…]": the card over a moment continued after a long break (`app_where_was_i`).
    if state.starts_with("where-was-i") {
        let picture = args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)));
        app.where_was_i_snapshot(state, picture, w, h);
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // A press through the real start path, then its first painted animation frame. A mock-only slow preparation
    // stands in for a card that stalls; rendering must keep going while it is pending (tests/launch_response.rs).
    // `bench-launch-j2me[:stop]`: A on the first phone game with no Java yet (`j2me_java`, POCKETUI_J2ME_* stand-ins):
    // the start's picture half-way through the load goes to `out`; `:stop` presses B there. Prints what the player saw.
    if let Some(rest) = state.strip_prefix("bench-launch-j2me") {
        app.shelf_snapshot("j2me:split:games:0");
        app.activate();
        let mut buf = vec![0u32; (w * h) as usize];
        let (asked, mut shown, mut saved) = (Instant::now(), false, false);
        while app.pending_launch.is_some() && asked.elapsed() < Duration::from_secs(20) {
            app.loading_wake(Instant::now());
            app.paint(&mut snap_canvas(&mut buf, w, h));
            app.launch_response_painted();
            let half = app.java_fetch.as_ref().is_some_and(|f| f.phase() == pocket_ui::j2me_java::Phase::Downloading && f.fraction() >= 0.5);
            shown |= app.java_fetch.is_some() && app.loading.since.is_some();
            if half && !saved {
                write_png(&buf, w, h, out);
                saved = true;
                if rest == ":stop" {
                    app.java_press(CANCEL);
                }
            }
            std::thread::sleep(Duration::from_millis(16));
            app.launch_poll();
        }
        let notice = app.notice.as_ref().map(|n| n.text().to_string()).unwrap_or_default();
        println!("bench-j2me shown={shown} saved={saved} started={} notice={notice}", app.running.is_some());
        return;
    }
    if state.starts_with("bench-launch-response") {
        app.shelf_snapshot("gba:shelf:games:0");
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        let before = buf.clone();
        let Some(expected) = app.card_game(app.items[app.sel]) else { return };
        if state.ends_with(":missing") { assert!(std::fs::remove_file(&expected.2).is_ok()); }
        if state.ends_with(":empty") { assert!(std::fs::write(&expected.2, []).is_ok()); }
        let asked = Instant::now();
        let cpu = app_cabinet::thread_cpu();
        app.activate();
        // The next display refresh, as on the device; an immediate CPU paint can round subpixel motion to zero.
        std::thread::sleep(FRAME);
        app.paint(&mut snap_canvas(&mut buf, w, h));
        app.launch_response_painted();
        // What the UI thread itself spent on A and its first frame (CPU time: a busy computer doesn't add to it), and
        // whether the preparation was already past its hold when that frame was painted (`TEST_PREPARE_HOLD`): it may
        // only go on after five frames.
        let first_cpu = app_cabinet::thread_cpu().saturating_sub(cpu);
        let prepared = app_launch::TEST_PREPARED.load(std::sync::atomic::Ordering::SeqCst);
        println!("bench-launch-response first_ms={:.3} first_cpu_ms={:.3} prepared_before_frame={prepared} inserting={} changed={}", asked.elapsed().as_secs_f64() * 1000.0, first_cpu.as_secs_f64() * 1000.0, app.cabinet_inserting(), before != buf);
        let mut frames = 0;
        let mut worst = Duration::ZERO;
        while app.pending_launch.is_some() && asked.elapsed() < Duration::from_secs(5) {
            let began = Instant::now();
            app.paint(&mut snap_canvas(&mut buf, w, h));
            worst = worst.max(began.elapsed());
            frames += 1;
            // The preparation goes on once the animation has shown it keeps going (five frames), on any computer.
            if frames == 5 {
                app_launch::TEST_PREPARE_RELEASED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            std::thread::sleep(Duration::from_millis(16));
            app.launch_poll();
        }
        assert!(app.pending_launch.is_none(), "the launch never finished preparing");
        let correct = app.running.as_ref().is_some_and(|r| r.path == expected.2 && r.system == expected.0 && r.name == expected.1);
        println!("bench-launch-result frames={frames} worst_ms={:.3} correct={correct} notice={} elapsed_ms={:.3}", worst.as_secs_f64() * 1000.0, app.notice.is_some(), asked.elapsed().as_secs_f64() * 1000.0);
        write_png(&buf, w, h, out);
        return;
    }
    // "crate:<row>[:pick:<k>|:swap:<k>:<ms>|:home|:ask|:clip:<k>|:rests]": Shelf space's crate (`App::crate_snapshot`).
    if let Some(rest) = state.strip_prefix("crate:") {
        app.crate_snapshot(rest, w, h, out);
        return;
    }
    // "bench-playful:<system>:<launch|pull|lean>": the playful animations' frames in numbers (`App::bench_playful`).
    if let Some(rest) = state.strip_prefix("bench-playful:") {
        app.bench_playful(rest, w, h);
        return;
    }
    // "bench-showcase:<system>:<row>[:<seconds>[:<dir>]]": the Cabinet's TV playing a game, in numbers (prototype,
    // `App::bench_showcase`).
    if let Some(rest) = state.strip_prefix("bench-showcase:") {
        app.bench_showcase(rest, w, h);
        return;
    }
    if state == "test-cabinet-card-swap" && pocket_ui::proc::mock() {
        app.cabinet_card_swap_check();
        return;
    }
    // "infos-warm-check:<system>": the first card's lookups cold and after the idle look-ahead (`App::infos_warm_check`).
    if let Some(rest) = state.strip_prefix("infos-warm-check:") {
        app.infos_warm_check(rest);
        return;
    }
    // "prefetch-check:<look>:<system>": reading covers ahead, checked (`App::prefetch_check`, tests/cover_prefetch.rs).
    if let Some(rest) = state.strip_prefix("prefetch-check:") {
        app.prefetch_check(rest, w, h);
        return;
    }
    // "search-start-check:<look>:<system>": a game started from the search shows its cover in the first frame.
    if let Some(rest) = state.strip_prefix("search-start-check:") {
        app.search_start_check(rest, w, h);
        return;
    }
    // "bench-prefetch:<look>": a fast scroll with cold covers, read on their threads (`App::bench_prefetch`).
    if let Some(look) = state.strip_prefix("bench-prefetch").map(|r| r.trim_start_matches(':')) {
        let b = app.bench_prefetch(look, w, h);
        println!(
            "bench-prefetch look={look} prefetch={} items={} moves={} frames={} p50_us={:.0} p95_us={:.0} max_us={:.0} first_ready={}/{} stand_in_frames={}/{}",
            std::env::var("POCKETUI_COVER_PREFETCH").map_or(true, |v| v != "0"),
            app.items.len(), b.moves, b.frames, b.p50_us, b.p95_us, b.max_us, b.first_ready, b.with_art, b.stand_in_frames, b.art_frames
        );
        return;
    }
    // "bench-bigcard[:<look>]": a huge card's shelf switches and moves, frame by frame (`App::bench_bigcard`).
    if let Some(look) = state.strip_prefix("bench-bigcard").map(|r| r.trim_start_matches(':')) {
        let look = if look.is_empty() { "cabinet" } else { look };
        let samples = app.bench_bigcard(look);
        app_bigcard_bench::print_bigcard(look, &samples);
        return;
    }
    // "bench-shelf[:<look>]": a move on Home in numbers, on stdout (`App::bench_shelf`, tests/shelf_moves.rs).
    if let Some(look) = state.strip_prefix("bench-shelf").map(|r| r.trim_start_matches(':')) {
        let b = app.bench_shelf(if look.is_empty() { "split" } else { look }, w, h);
        println!(
            "bench-shelf items={} moves={} p50_us={:.0} p95_us={:.0} max_us={:.0} glyphs={} patterns={} panel={}x{} logical={w}x{h} paint={:?} design_passes={:.1} sealed={} look={:?} changes={} moment_ms={:.1} worst={} room_rebuilds={} shadows_made={}",
            app.items.len(),
            b.moves,
            b.p50_us,
            b.p95_us,
            b.max_us,
            b.glyphs,
            b.patterns,
            snapshot_panel_dims().0,
            snapshot_panel_dims().1,
            pocket_ui::scaled_layout::native_divisor(snapshot_panel_dims()).map_or(pocket_ui::scaled_layout::Paint::Scaled, |_| pocket_ui::scaled_layout::Paint::Native),
            b.design_passes,
            app.items.iter().filter_map(|item| app.item_rom(*item)).filter(|(_, rom)| app.sealed(rom)).count(),
            app.theme,
            b.changes,
            b.moment_ms,
            b.worst,
            app_cabinet::ROOM_REBUILDS.load(std::sync::atomic::Ordering::Relaxed),
            b.shadows_made
        );
        print_design_sites();
        return;
    }
    // Host regression journey: winit's provisional size settles before launch. Feed the same
    // frame admission used by redraw; a stale full size would leave the pause menu blank.
    if state.ends_with(":startup-resize") {
        app.running = None;
        assert!(!app.wait_for_full_size(800, 600));
        assert!(!app.wait_for_full_size(640, 480));
    }
    // "bench-settings-pages": every category's first page, in list and tab-bar focus, and the strings that ran patterns.
    if state == "bench-settings-pages" {
        let (pages, patterns, found) = app.settings_pages_patterns(w, h);
        println!("bench-settings-pages pages={pages} patterns={patterns} strings={}", found.join(" | "));
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
    // pocket-core draws in the panel's own pixels, not on the shelf's canvas.
    let (gw, gh) = if state == "game-twin-note" { snapshot_panel_dims() } else { (w, h) };
    if let Some(buf) = app_save_trouble::game_warning_snapshot(state, background, gw, gh, &mut app.text) {
        write_png(&buf, gw, gh, out);
        return;
    }
    // "notice:<several|password|packing|checksum|incomplete|empty|none>": Home with the card a zip pocket-core refused leaves
    // (`archive::message`), "notice:bios" the card for a game that didn't start (`fallback::not_started_notice`; "bios-segacd"
    // for one whose missing BIOS is known),
    // "notice:update" the hint that a newer pocket-ui is ready (`app_update_hint`), "notice:card-update" an update file found on the card (`app_card_update`), "notice:twin" RetroArch's start of a game sharing its file name (`frozen::SHARED`), "notice:card-full" / "notice:card-unwritable" settings or lists the card refused to save (`save_trouble::CardRefusals`), "notice:switched-off" the start after a switch-off after sleeping (`app_auto_off`), "notice:patch-<version|damaged|
    // big|kind>" a patch next to the game that pocket-core refused (`softpatch::message`).
    if let Some(kind) = state.strip_prefix("notice:") {
        // A trailing ":toast" also raises the small toast, so its place under the card is rendered.
        let (kind, toast) = match kind.strip_suffix(":toast") {
            Some(k) => (k, true),
            None => (kind, false),
        };
        let patch = kind.strip_prefix("patch-").and_then(pocket_ui::softpatch::Refusal::named);
        app.notice = Some(if kind == "update" { Notice::update_ready("0.3.0") } else if kind == "card-update" { Notice::card_update("0.5.1") } else { Notice::new(match kind {
            _ if patch.is_some() => pocket_ui::softpatch::message(patch.unwrap(), "Seiken Densetsu 3 (Japan).bps"),
            "bios" => pocket_ui::fallback::not_started_notice("Final Quest", None),
            "bios-segacd" => pocket_ui::fallback::not_started_notice(
                "Pocket CD Demo",
                Some("Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin"),
            ),
            #[cfg(feature = "brick")]
            "switched-off" => pocket_ui::auto_off::Note { minutes: 180, hours: 0, brightness: None }.text(),
            #[cfg(not(feature = "brick"))]
            "switched-off" => pocket_ui::auto_off::Note { hours: 3, brightness: None }.text(),
            "twin" => pocket_ui::frozen::SHARED.to_string(),
            "card-full" => pocket_ui::save_trouble::CHANGES_FULL.to_string(),
            "card-unwritable" => pocket_ui::save_trouble::CHANGES_UNWRITABLE.to_string(),
            "save-failed" => pocket_ui::save_trouble::MOMENT_FAILED.to_string(),
            "undo-failed" => pocket_ui::save_trouble::UNDO_FAILED.to_string(),
            "fallback" => pocket_ui::fallback::notice("retroarch"),
            "kept" => pocket_ui::frozen_phone::aside_line_handheld(pocket_ui::frozen_phone::Aside::Kept),
            k => pocket_ui::archive::message(k, "Sonic The Hedgehog (USA, Europe).zip"),
        }) });
        if toast {
            app.toast = Some(("Added to Favourites".to_string(), Instant::now()));
        }
        // The Continue game's cover read at once, not on the thread (as the cabinet's snapshots do).
        app.cabinet.inline = true;
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        // As the real draw: a page that paints no notice of its own (a shelf with nothing on it) gets it over the page.
        app.paint_notice_over_page(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "leave[:<row>]", "about-es[:<row>]": the dialog before EmulationStation (`app_rescue`).
    if app.leave_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        app.paint_leave(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "tour[:<card id>[:<keys>]]", "tour-new:<id>[,<id>...]": Settings → Tour, "New in this version" (`app_tour`).
    if app.tour_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "setup:<look|look-systems|wifi|games|first|play|card=<id>>[:keys]": a setup step, then the keys pressed in it (r l u
    // d a b x y s, R for R1, L for L1, T for R2).
    if let Some(rest) = state.strip_prefix("setup:") {
        let (step, keys) = rest.split_once(':').unwrap_or((rest, ""));
        let mut setup = app.new_setup();
        setup = match step {
            "look" => setup.only(Only::Look),
            // Settings → Remote access (SSH), as the setup step paints it.
            "remote" => setup.only(Only::Remote),
            // Settings → Games → Systems on the shelf, opened from its own row (B goes back to Settings).
            "look-systems" => setup.only(Only::Look).at_shelf_systems(),
            "games" => setup.only(Only::Games),
            // Add games → Y, "Send from your phone", with a made-up address (`PhonePage::demo`: waiting, connected,
            // receiving, done, idle, slept, nowifi).
            _ if step.starts_with("phone=") => setup.only(Only::Games).with_phone_demo(&step["phone=".len()..]),
            // Settings > Games > Box art & downloads: Cover art, Game pictures, PortMaster games (keys: d down, u up, a).
            "extras" => setup.only(Only::Extras),
            _ if step.starts_with("extras-cheats-") => setup.only(Only::Extras).with_cheats_demo(&step["extras-cheats-".len()..]),
            _ if step.starts_with("extras-portmaster-") => setup.only(Only::Extras).with_extras_portmaster_demo(&step["extras-portmaster-".len()..]),
            "extras-art-error" => setup.only(Only::Extras).with_cover_error_demo(),
            // "How do you like to play?" (keys: d down, a); "card=<feature id>": Choose yourself at that card (keys d, a, b, r).
            "play" => setup.at_play(),
            // The first start at a stage: Wi-Fi with made-up networks (its Time zone row from the guess), your games,
            // the look (keys: a, b, x Style…, d, r, s).
            "first-wifi" => setup.at_stage("wifi").with_wifi_demo(),
            "first-wifi-empty" => setup.at_stage("wifi").with_net_demo(pocket_ui::net_state::demo::empty()),
            "first-games" => setup.at_stage("games"),
            "first-look" => setup.at_stage("look"),
            // "How cozy should it be?", after the look (`cozy_level`).
            "first-cozy" => setup.at_stage("cozy"),
            // Settings › Help & tools › How you like to play: without "For a child".
            "play-later" => setup.only(Only::Play),
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
            // One network called <ssid> (a word the German table has, say): its name is painted as it is.
            _ if step.starts_with("wifi-named=") => setup.only(Only::Wifi).with_named_network_demo(&step["wifi-named=".len()..]),
            // Its sign-in sheet (keys: d down, a opens a row, b back); "-nomac" as if the Wi-Fi address couldn't be read.
            "wifi-portal-sheet" => setup.only(Only::Wifi).with_wifi_demo().with_portal_sheet(true),
            "wifi-portal-sheet-nomac" => setup.only(Only::Wifi).with_wifi_demo().with_portal_sheet(false),
            // Its "Sign in…" with a made-up page or answer (`portal_form::demo`: hotel, clickthrough, voucher, email,
            // js, social, elsewhere, still, toomany, failed, online, loading); keys d/u move, a ticks, types or opens.
            _ if step.starts_with("wifi-portal-signin=") => setup.only(Only::Wifi).with_wifi_demo().with_portal_signin(&step["wifi-portal-signin=".len()..]),
            // The PortMaster browser with POCKETUI_DATA's ports.json (or a small demo catalog); "ports-shared":
            // file sharing on (the desktop's stand-in), for the "Copy your game files" page (keys r, x).
            "ports" | "ports-shared" | "ports-update" | "ports-get" | "ports-get-offline" | "ports-get-download" | "ports-get-error" | "ports-get-update" | "ports-get-checking" => {
                #[cfg(not(target_os = "linux"))]
                if step == "ports-shared" {
                    pocket_ui::wifi::enable_sharing();
                }
                let setup = setup.only(Only::Extras).show_ports(app_ports::snapshot_catalog(&app.data_dir));
                // "ports-update": two ports of the list have a newer version in the catalogue (made up).
                if step.starts_with("ports-get") { setup.with_portmaster_demo(step.strip_prefix("ports-get-").unwrap_or("offer")) }
                else if step == "ports-update" { setup.with_updates(&["2048.zip", "cceleste.zip"]) } else { setup }
            }
            // PortMaster games from the card (no Wi-Fi) with made-up files: empty, empty-games (the RG28XX's GAMES drive),
            // found, installing, done, failed (keys: d, u).
            _ if step.starts_with("ports-card-") => setup.only(Only::Extras).with_ports_card_demo(&step["ports-card-".len()..]),
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
                // R2: the look step's Theme page.
                'T' => Button::R2,
                _ => continue,
            };
            if let Some(s) = app.setup.as_mut() {
                s.press(b);
                s.looked_now();
            }
        }
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    if app.empty_ports_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    if state.starts_with("port-files") {
        app.port_files_snapshot(state);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // Cozy extras can return to a warmed hardware view. Finish it before the generic pause-menu
    // fixture below creates a running game/menu over that returned Home view.
    if app.cozy_extras_snapshot(state) {
        app.frozen_snapshot();
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "console:<system>[:sources][:<presses of Down>]": a console page; "shelf:<system>[:<look>]": Home with that system selected.
    if let Some(rest) = state.strip_prefix("console:") {
        app.console_snapshot(rest);
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        app.console_snapshot_scroll(rest);
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "emu:<system>[:<game file>][:<emulator>/<core>][:save][:<row>]": the "Runs on" page (`app_emu_choice`).
    if app.emu_choice_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        // A card that A on the game raised (Dolphin's, `open_dolphin_card`) over everything, as the real draw puts it.
        app.paint_leave(&mut c);
        write_png(&buf, w, h, out);
        return;
    }
    // "this-game[:<system>[:<game file>]][:<row>][:<key>=<value>]": the "This game" page (`app_this_game`).
    if state.starts_with("this-game") && app.this_game_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    if let Some(rest) = state.strip_prefix("shelf:") {
        // A live language change after both shelf/cabinet caches were painted, exercised by sealed_saves.
        let (rest, switch) = rest.split_once(":language=").map_or((rest, None), |(r, l)| (r, Some(l)));
        app.shelf_snapshot(rest);
        app.frozen_snapshot();
        let mut buf = vec![0u32; (w * h) as usize];
        SPLIT_SEEN.set([0; 10]);
        app.paint(&mut snap_canvas(&mut buf, w, h));
        // Split's panel and console, in the canvas' pixels (tests/split_panel.rs).
        let [x, y, pw, ph, ax, ay, aw, ah, cw, ch] = SPLIT_SEEN.get();
        if pw > 0 {
            println!("split-panel x={x} y={y} w={pw} h={ph} canvas={cw}x{ch}");
            println!("split-console x={ax} y={ay} w={aw} h={ah}");
        }
        if let Some(lang) = switch {
            app.set_language(pocket_ui::i18n::Lang::from_locale(lang));
            app.paint(&mut snap_canvas(&mut buf, w, h));
        }
        // Playful animations in the Shelf look (`app_cabinet::shelf_playful_snapshot`): POCKETUI_SNAPSHOT_HELD=<steps>:<ms>
        // Right held for that many steps, that long after the last; POCKETUI_SNAPSHOT_MOVED_MS that long after a move.
        app.shelf_playful_snapshot(&mut snap_canvas(&mut buf, w, h));
        // POCKETUI_SNAPSHOT_LAUNCH_SERIES: every 60 Hz frame of it (`app_cabinet::launch_series_snapshot`).
        if app.launch_series_snapshot(&mut buf, w, h) {
            return;
        }
        // POCKETUI_SNAPSHOT_LAUNCH_MS: the launch animation that long after A (Shelf look, `app_cabinet`).
        if let Some(ms) = std::env::var("POCKETUI_SNAPSHOT_LAUNCH_MS").ok().and_then(|v| v.parse().ok()) {
            app.launch_animation_snapshot(Duration::from_millis(ms));
            // POCKETUI_SNAPSHOT_LAUNCH_PICTURE=<png>: where the game was left, fading in over it (`launch_picture`), fitted.
            if let Some(img) = std::env::var("POCKETUI_SNAPSHOT_LAUNCH_PICTURE").ok().and_then(|p| image::load_png(std::path::Path::new(&p))) {
                let (fw, fh) = img.fit(w as i32, h as i32);
                let img = pocket_ui::launch_picture::scaled(&img, fw as u32, fh as u32);
                app.launch_picture = Some(pocket_ui::launch_picture::Prepared { rom: PathBuf::new(), x: (w as i32 - fw) / 2, y: (h as i32 - fh) / 2, img });
            }
            app.paint(&mut snap_canvas(&mut buf, w, h));
        }
        // POCKETUI_SNAPSHOT_EJECT_MS: Cozy's way back from the selected game that long after it ended
        // (`app_cabinet::eject_snapshot`); POCKETUI_SNAPSHOT_EJECT_BENCH=1 prints its numbers instead.
        if eject_shot(app, &mut buf, w, h, out) {
            return;
        }
        write_png(&buf, w, h, out);
        return;
    }
    // "conflicts:<card|choose|notice|none>[:<system>][:<row>]": a save's two versions (`App::conflicts_snapshot`).
    if app.conflicts_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        app.paint(&mut snap_canvas(&mut buf, w, h));
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
    // Split, the box in Shelf and Cabinet.
    let info = state.strip_prefix("info:").and_then(|r| r.split_once(':')).map(|(look, r)| {
        app.theme = match look {
            "split" => Theme::Split,
            "cabinet" => Theme::Cabinet,
            _ => Theme::Shelf,
        };
        app.game_box = true;
        app.cabinet.inline = true;
        // "info:<look>:stickers:<system>:<name>:<keys>": the box with the card's Stickers sheet open (keys as
        // `curator:stickers:…`; POCKETUI_SNAPSHOT_STICKER_MS holds the clock that long into the sticker's press).
        // "info:<look>:walk:<system>:<name>:<keys>": the card as it opens, then `keys` as the pad sends them, a frame
        // painted after each (`App::walk_key`: S is SELECT).
        if r.starts_with("stickers:") || r.starts_with("walk:") { r.to_string() } else { format!("card:{r}") }
    });
    // "curator:<what>": Curator's screens (`App::curator_snapshot`).
    if let Some(rest) = state.strip_prefix("curator:").or(boxed.as_deref()).or(info.as_deref()) {
        app.curator_snapshot(rest);
        let mut sticker_clock = app.sticker_snapshot_clock();
        // ":battery=<case>" on a card: its Battery line with made-up measurements (`App::battery_snapshot`).
        app.battery_snapshot(rest);
        // ":lookup=<case>" on a card: the line "Look up again" ends with (`App::look_up_snapshot`).
        app.look_up_snapshot(rest);
        // ":patch" on a card: a patch next to its game (`App::patch_snapshot`).
        app.patch_snapshot(rest);
        // ":saves[=none]" on a card: its save slots on their medium (`App::card_saves_snapshot`).
        app.card_saves_snapshot(rest);
        let mut buf = vec![0u32; (w * h) as usize];
        pocket_ui::game_card::BODY_SEEN.set((0, 0));
        app.paint(&mut snap_canvas(&mut buf, w, h));
        let (cw, ch) = { let cv = snap_canvas(&mut buf, w, h); (cv.w, cv.h) };
        print_card_body(cw, ch);
        // A box's back measures its scrolling part when it is painted: keys after its turn (`box:…:rdd`) press on a
        // painted back, as a player's do, so Down scrolls it.
        match boxed.as_deref().and_then(|_| after_turn(rest)) {
            Some((turn, then)) => {
                app.curator_snapshot_after(&turn);
                pocket_ui::frame_clock::hold_settled();
                if let Some(bx) = app.card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
                    bx.snapshot_settle();
                }
                app.paint(&mut snap_canvas(&mut buf, w, h));
                app.curator_snapshot_after(&then);
            }
            None => app.curator_snapshot_after(rest),
        }
        if let Some(keys) = rest.strip_prefix("walk:").and_then(|r| r.splitn(3, ':').nth(2)) {
            let mut chars = keys.chars().peekable();
            while let Some(k) = chars.next() {
                app.walk_key(k);
                // Keep a sticker press started by the final key intact until `sticker_snapshot_clock()` sets its moment:
                // advancing `hold_settled()` by real wall-clock paint time after `walk_key` could expire it under load.
                if chars.peek().is_some() || app.cabinet.press.is_none() {
                    pocket_ui::frame_clock::hold_settled();
                }
                app.paint(&mut snap_canvas(&mut buf, w, h));
            }
            sticker_clock = app.sticker_snapshot_clock();
            let page = app.curator.page.as_ref().map_or("none", app_where::curator_name);
            println!("walk box={} page={page}", app.card.as_ref().is_some_and(|o| o.card.boxed.is_some()));
        }
        // One moment for the turn, the slide and the disc: the clock stands while the frame is set up and painted
        // (a sticker's press holds it at its own moment: `sticker_snapshot_clock`).
        if !sticker_clock {
            pocket_ui::frame_clock::freeze(pocket_ui::frame_clock::now().max(Instant::now()) + pocket_ui::frame_clock::SETTLE);
        }
        if let Some(bx) = app.card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
            bx.snapshot_settle();
        }
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        // POCKETUI_BENCH_FRAMES=<n>: that many frames of the box as it stands (its disc turning, Playful animations),
        // 16 ms apart, their thread CPU time on stdout (docs/LATENCY.md "Playful animations").
        if let Some(n) = std::env::var("POCKETUI_BENCH_FRAMES").ok().and_then(|v| v.parse::<u32>().ok()) {
            let mut times = Vec::new();
            for k in 0..n {
                if let Some(bx) = app.card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
                    bx.age_spin(Duration::from_millis(16 * k as u64));
                }
                let c0 = app_cabinet::thread_cpu();
                app.paint(&mut snap_canvas(&mut buf, w, h));
                times.push(app_cabinet::thread_cpu().saturating_sub(c0));
            }
            let boxed = app.card.as_ref().and_then(|o| o.card.boxed.as_ref());
            println!("{}", app_cabinet::bench_line(&format!("bench-box {rest} playful={} boxed={} sealed={}", pocket_ui::playful::on(), boxed.is_some(), boxed.is_some_and(|b| b.sealed.is_some())), &times));
        }
        return;
    }
    // "search:<…>" and "letters:<…>": the game search and the letter of a jump (`app_search::find_snapshot`).
    if app.find_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "pictures:<which>[:<system>]": Split's preview at a point of its cycle, or the card (`app_pictures`).
    if app.pictures_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "gallery[:…]": the screenshot gallery with made-up screenshots, or the game card's row for them (`app_gallery`).
    if app.gallery_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        app.gallery_snapshot_after(state);
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    // "save-card:<ps1|ps2|gamecube|vmu|xbox|stick|<cartridge system>>": a memory card (or a cartridge) alone, large and at the pause menu's size,
    // on grey, to hold beside a photo of the real one (`app_save_media`).
    if let Some(card) = state.strip_prefix("save-card:") {
        let mut buf = vec![0u32; (w * h) as usize];
        app_save_media::save_card_snapshot(&mut snap_canvas(&mut buf, w, h), &mut app.text, card);
        write_png(&buf, w, h, out);
        return;
    }
    if let Some(rest) = state.strip_prefix("cabinet:") {
        app.cabinet_snapshot(rest);
        app.frozen_snapshot();
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        let page_px = c.pixels().len() as u64;
        pocket_ui::gfx::take_cleared();
        app.paint(&mut c);
        app.cabinet_snapshot_outcome(h as i32, page_px);
        // POCKETUI_SNAPSHOT_EJECT_MS and the rest: the way back from the game the cabinet stands on (`eject_snapshot`).
        if eject_shot(app, &mut buf, w, h, out) {
            return;
        }
        write_png(&buf, w, h, out);
        return;
    }
    // "card-manage:<system>:<name>[:<mode>]": a card's Hide or delete; "storage-notice": the low-space notice (`app_storage`).
    if app.storage_shelf_snapshot(state) {
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint(&mut snap_canvas(&mut buf, w, h));
        app.storage_shelf_snapshot_after(state);
        app.paint(&mut snap_canvas(&mut buf, w, h));
        write_png(&buf, w, h, out);
        return;
    }
    if let Some(demo) = state.strip_prefix("sys:") {
        // Quick settings, the time pages and the OSD, over the shelf.
        app.sys.demo(demo, Instant::now());
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = snap_canvas(&mut buf, w, h);
        app.paint(&mut c);
        app.sys.paint(&mut c, &mut app.text, true);
        write_png(&buf, w, h, out);
        return;
    }
    // POCKETUI_SNAPSHOT_GAME="<system>|<name>|<file>" names the game behind the pause menu (the README's pictures).
    let (g_system, g_name, g_file) = std::env::var("POCKETUI_SNAPSHOT_GAME")
        .ok()
        .and_then(|v| {
            let p: Vec<&str> = v.splitn(3, '|').collect();
            (p.len() == 3).then(|| (p[0].to_string(), p[1].to_string(), p[2].to_string()))
        })
        .unwrap_or_else(|| ("gbc".into(), "uCity".into(), "uCity.gbc".into()));
    app.running = Some(Running {
        system: g_system.clone(),
        name: g_name,
        path: PathBuf::from(format!("/storage/roms/{g_system}/{g_file}")),
        emulator: "pocket-core".into(),
        core: String::new(),
        started: Instant::now(),
        pgid: None,
        options: Default::default(),
    });
    let frame = args.get(2).and_then(|p| image::load_image(std::path::Path::new(p)));
    // The pause menu's "Paused here · played 42 min today" (`pause_page::paused_here`): this morning's 40 minutes of the
    // game in a made-up log, as a device with Play history on (the default) has them; the running session adds its own.
    if state.split(':').next() == Some("menu") && pocket_ui::play_log::view().is_none() {
        let now = pocket_ui::play_time::now_secs();
        let tz = pocket_ui::appearance::utc_offset_now(now as i64);
        let morning = (pocket_ui::civil_date::local_day(now, tz) * 86_400 + 8 * 3600 - tz as i64) as u64;
        let game = pocket_ui::play_log::Game { path: format!("/storage/roms/{g_system}/{g_file}"), name: "uCity".into(), system: g_system.clone() };
        let session = pocket_ui::play_log::Session { game: 0, start: morning, tz, secs: 42 * 60, ended: true, emulator: "pocket-core".into(), ..Default::default() };
        pocket_ui::play_log::show_instead(pocket_ui::play_log::build(vec![game], vec![session], Vec::new(), Vec::new()));
    }
    let tmp = snapshot_tmp();
    // "picture-gba[:<row>]": the Picture page of a GBA game, whose LCD grid has a row for the GBA screen's colours.
    // "picture-snes[:<row>]": a SNES game's.
    // "picture-psx[:<row>]": a PlayStation game's, as it starts (Look Clean). "picture-custom[:<row>]": a PlayStation game
    // with the Look turned to Custom (Crisp, Scanlines: a pair the Look row would call Scanlines), its Sharpness and Effect open.
    let gba = state.starts_with("picture-gba");
    let mut settings = if gba {
        GameSettings::load(&tmp, "gba", std::path::Path::new("Advance Wars.gba"))
    } else if state.starts_with("picture-snes") {
        GameSettings::load(&tmp, "snes", std::path::Path::new("Super Metroid.sfc"))
    } else if state.starts_with("picture-psx") || state.starts_with("picture-custom") {
        GameSettings::load(&tmp, "psx", std::path::Path::new("Some Game (USA).chd"))
    } else if state.starts_with("picture-n64") {
        // With `:device=miniv2` its Resolution row is there (`render_scale`), last.
        GameSettings::load(&tmp, "n64", std::path::Path::new("Super Mario 64 (USA).z64"))
    } else {
        GameSettings::load(&tmp, "gbc", std::path::Path::new("uCity.gbc"))
    };
    if state.starts_with("picture-custom") {
        settings.step(options::setting(options::LOOK).unwrap(), -1);
        settings.values.insert("effect".into(), "scanlines".into());
    } else if !state.starts_with("picture-psx") && !state.starts_with("picture-n64") {
        settings.step(options::setting("effect").unwrap(), 1);
    }
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
    // "battery[:<range>][:samples|:sleep]": the Battery page and its graph's range; `:samples` with six made-up hours of
    // history (`snapshot_battery_samples`), `:sleep` with a night asleep before them.
    let battery_args: Option<Vec<&str>> = state.strip_prefix("battery").map(|r| r.split(':').filter(|p| !p.is_empty()).collect());
    let battery_range = battery_args.as_ref().map(|a| a.first().and_then(|n| n.parse().ok()).unwrap_or(pocket_ui::battery::DEFAULT_RANGE));
    // "manual:<page>:<pixels>": a page of the manual (0 = the first), scrolled down by that much.
    let manual_at = state.strip_prefix("manual").map(|r| r.split(':').filter_map(|n| n.parse::<i32>().ok()).collect::<Vec<_>>());
    if let Some(at) = &manual_at {
        app.manual.page = at.first().copied().unwrap_or(0).clamp(0, pocket_ui::manual::pages().len() as i32 - 1) as usize;
        app.settings_page = Some(SettingsPage::Manual);
        // The reader needs one paint to know how long the page is before it can scroll.
        let mut buf = vec![0u32; (w * h) as usize];
        app.paint_manual(&mut snap_canvas(&mut buf, w, h));
        app.manual.scroll_by(at.get(1).copied().unwrap_or(0));
    }
    if manual_at.is_some() || app.starter_snapshot(state) || app.sync_snapshot(state) || app.leds_snapshot(state) || app.night_light_snapshot(state) || app.buttons_snapshot(state) || app.controllers_snapshot(state) || app.tailscale_snapshot(state) || app.romm_snapshot(state) || app.web_snapshot(state) || app.pocket_sync_snapshot(state) || app.bluetooth_snapshot(state) || app.perf_fan_snapshot(state) || app.sleep_power_snapshot(state) || app.report_snapshot(state) || app.port_perf_snapshot(state) || app.achievements_snapshot(state) || app.tools_snapshot(state) || app.test_buttons_snapshot(state) || app.game_check_snapshot(state) || app.pc_streaming_snapshot(state) || app.backup_snapshot(state) || app.storage_snapshot(state) || app.gamelist_snapshot(state) || app.android_games_snapshot(state) {
    } else if let Some(kind) = state.strip_prefix("updates") {
        // The Updates page after "Get system" with a made-up list: "updates:note" staged with a note of what the new
        // system lacks, "updates:refused" refused because pocket-ui would break (`os_compat`).
        let os = update::Release { version: "20261101".into(), url: "x.tar".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: String::new() };
        app.updates.manifest = Some(Manifest { format: 1, os: Some(os), ..Default::default() });
        app.updates.message = Some(if kind == ":refused" {
            "This system update would break pocket-ui (libxkbcommon.so.0 is missing: pocket-ui can't open its window). It was not installed.".into()
        } else {
            os_staged_message(Some("Note: no gambatte core in the new system, so gb, gbc games would not start."))
        });
        match kind {
            // An app install after its download, as on the Brick's slow card (Alex, 15.11.): unpacking, moving the parts the
            // running version already has, and installed, waiting for the restart.
            ":unpacking" | ":moving" | ":restarting" => {
                let cancel = update::Cancel::new();
                let phase = if kind == ":moving" { update::WorkerPhase::MovingAssets } else { update::WorkerPhase::Unpacking };
                cancel.set_phase(phase);
                app.updates.message = None;
                app.updates.start(UpdateTask::App { version: "0.4.2".into(), total: 499_000, cancel });
                app.updates.progress(499_000);
                let (work_done, work_total) = if kind == ":moving" { (14, 20) } else { (12_600_000, 30_000_000) };
                if let UpdatePhase::Running(job) = &mut app.updates.phase {
                    // As on the Brick: a cancel still works while it unpacks, not once parts move.
                    job.worker = update::WorkerSnapshot { phase, work_done, work_total, cancel_effective: kind == ":unpacking", ..Default::default() };
                }
                if kind == ":restarting" { app.update_event(UpdateEvent::Done(Ok(true))); }
            }
            // "downloading": all parts on one bar with the time left; "fallback": a lost connection and the next try.
            ":connecting" | ":fallback" | ":downloading" => {
                let cancel = update::Cancel::new();
                let phase = match kind {
                    ":connecting" => update::WorkerPhase::Connecting,
                    ":fallback" => update::WorkerPhase::Fallback,
                    _ => update::WorkerPhase::Downloading,
                };
                if kind != ":connecting" { cancel.pretend_download(42_000_000, 100_000_000, 60, (kind == ":fallback").then_some((2, 3))); }
                cancel.set_phase(phase);
                app.updates.start(UpdateTask::App { version: "0.7.0".into(), total: 100_000_000, cancel });
            }
            // The page with a newer pocket-ui on offer: its notes, then the question, with a flat battery or a full one.
            ":ready" | ":confirm" | ":confirm-low" | ":confirm-charging" | ":off" => {
                app.updates.release_build = Some(true);
                let app_release = update::Release { version: "99.0.0".into(), url: "x.tar".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: "Update notice: pocket-ui tells you once when a new version is out. Switch it off in Updates. Fixes long game names on the shelf.".into() };
                app.updates.manifest = Some(Manifest { format: 1, app: Some(app_release), ..Default::default() });
                app.updates.message = None;
                app.updates.set_confirmation(kind.starts_with(":confirm"));
                if kind == ":off" {
                    app.update_hint.state.set_auto(false);
                }
                if kind == ":confirm-low" || kind == ":confirm-charging" {
                    pocket_ui::status_corner::force_battery(pocket_ui::status_corner::Battery { percent: 14, charging: kind == ":confirm-charging" });
                }
            }
            ":development" => {
                app.updates.release_build = Some(false);
                let app_release = update::Release { version: "0.3.2".into(), url: "unused.tar".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: String::new() };
                app.update_event(UpdateEvent::Checked(Ok(Manifest { format: 1, app: Some(app_release), ..Default::default() })));
            }
            ":timeout" => app.updates.message = Some("The download server stopped responding. Nothing was installed. Try again.".into()),
            // An update file on the card (`app_card_update`): found and asked about, refused, none there; and a handheld
            // without Wi-Fi told where updates come from instead.
            ":card" => {
                app.updates.release_build = Some(true);
                let app_release = update::Release { version: "99.0.0".into(), url: "pocket-ui-99.0.0-aarch64.tar.gz".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: "Update notice: pocket-ui tells you once when a new version is out. Switch it off in Updates. Fixes long game names on the shelf.".into() };
                let manifest = Manifest { format: 1, app: Some(app_release), ..Default::default() };
                app.updates.manifest = None;
                app.updates.message = None;
                app.update_event(UpdateEvent::CardLooked { at_start: false, look: update::card::Look::Offer(update::card::Found {
                    file: "/storage/games-external/pocket-ui-update-99.0.0-rocknix.zip".into(), manifest, id: String::new() }) });
            }
            ":card-refused" => app.updates.message = Some(update::card::OTHER_TARGET.into()),
            ":card-none" => app.updates.message = Some(update::card::NOTHING.into()),
            ":card-older" => app.updates.message = Some(update::card::not_newer("0.4.2")),
            ":no-wifi" => app.updates.message = Some(update::NO_WIFI_HERE.into()),
            ":unreachable" => {
                app.updates.manifest = None;
                app.update_event(UpdateEvent::Checked(Err("Couldn't reach the update server. Check Wi-Fi.".into())));
            }
            _ => {}
        }
        app.settings_page = Some(SettingsPage::Updates);
    } else if state == "about" || state.starts_with("settings") || battery_range.is_some() {
        // "settings": what START opens, the first category's list at its first row; "settings:<category>[:<row>]": that
        // category's list (`SETTINGS_GROUPS`) at its row (counted from 0, the first by default). A category this device
        // has no row in (Connections on Android): the first one it has.
        let nums: Vec<usize> = state.split(':').skip(1).filter_map(|n| n.parse().ok()).collect();
        let g = nums.first().copied().unwrap_or_else(app_settings_groups::first_group).min(SETTINGS_GROUPS.len() - 1);
        let rows = app_settings_groups::rows(g);
        let rows = if rows.is_empty() { app_settings_groups::rows(app_settings_groups::first_group()) } else { rows };
        let r = nums.get(1).copied().unwrap_or(0);
        app.settings_page = Some(match (state.as_str(), battery_range) {
            ("about", _) => SettingsPage::About,
            (_, Some(range)) => SettingsPage::Battery(range),
            // "settings-tabs[:<category>]": the focus on the categories at the top, that category's rows under them.
            _ if state.starts_with("settings-tabs") => SettingsPage::Tabs(rows[0]),
            _ => SettingsPage::List(rows[r.min(rows.len() - 1)]),
        });
        // Brightness and Volume (Screen & sound) come from the quick settings' background look: taken here at once.
        app.sys.look_levels_now();
        // The values the list reads on a thread, read here at once (`settings_values`).
        // ":dot": a newer pocket-ui is known (the dot on System and on its Updates row).
        if state.ends_with(":dot") {
            app.updates.release_build = Some(true);
            let release = update::Release { version: "99.0.0".into(), url: "x.tar".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: String::new() };
            app.updates.manifest = Some(Manifest { format: 1, app: Some(release), ..Default::default() });
        }
        let connected = pocket_ui::status_corner::status().wifi;
        pocket_ui::settings_values::set(pocket_ui::settings_values::read(&*app.sys.shared_hw(), &app.sys.net(), &app.data_dir, connected));
        // And the About page's (`ABOUT`) and the Bluetooth row's.
        ABOUT.set((free_space("/storage"), pocket_ui::wifi::ip_now()));
        app.bluetooth_row_now();
    }
    let group = |name: &str| options::GROUPS.iter().position(|g| g.name == name).unwrap_or(0);
    let controls_system = if state.starts_with("controls:nds") { "nds" } else { "gbc" };
    // Reproducible DS assignments for the live Controls page painters.
    if controls_system == "nds" {
        if state.ends_with(":stylus") { settings.values.insert("ctl.l2".into(), "ds_stylus".into()); }
        if state.ends_with(":layout") { settings.values.insert("ctl.r2".into(), "ds_layout".into()); }
    }
    let emu = if controls_system == "nds" {
        Some(EmuMenu::offline(controls_system, &settings))
    } else {
        EmuMenu::load(&app.data_dir, controls_system, std::path::Path::new("/storage/roms/gbc/uCity.gbc"), &mut settings)
            .or_else(|| state.starts_with("controls").then(|| EmuMenu::offline(controls_system, &settings)))
    };
    // "name:a:b" selects rows: controls:<row>, emulator:<row>, category:<category>:<row>.
    let mut parts = state.split(':');
    let name = parts.next().unwrap_or_default();
    let nums: Vec<usize> = parts.filter_map(|p| p.parse().ok()).collect();
    let n = |i: usize| nums.get(i).copied().unwrap_or(0);
    let page = match name {
        "options" => Page::Options { sel: n(0) },
        "picture" | "picture-gba" | "picture-snes" | "picture-psx" | "picture-custom" | "picture-n64" => Page::Group { group: group("Picture"), sel: if nums.is_empty() { 1 } else { n(0) } },
        // "shortcuts[:<row>]": Fast-forward unless a row is given (6: Over the game).
        "shortcuts" => Page::Group { group: group("Shortcuts"), sel: nums.first().copied().unwrap_or(3) },
        // "extras:<row>": Rumble unless a row is given (2: Earlier states, 5: Performance, 6: Fan).
        "extras" => Page::Group { group: group("Speed, rumble, fan"), sel: nums.first().copied().unwrap_or(4) },
        "emulator" => Page::Emulator { cat: None, sel: n(0) },
        "category" => Page::Emulator { cat: Some(n(0)), sel: n(1) },
        "controls" => Page::Controls { sel: n(0) },
        "cheats" | "cheats-more" | "cheats-only" | "cheats-empty" | "cheats-get" | "cheats-downloading" | "cheats-offline" | "cheats-unavailable" => Page::Cheats { sel: n(0) },
        "achievements-list" => Page::Achievements { sel: n(0) },
        "ps2-options" | "gc-options" | "wii-options" | "3ds-options" | "gc-more" | "wii-more" => Page::Standalone { sel: n(0) },
        _ => Page::Main,
    };
    // "slot-<save|load|auto|undo|empty>:<system>[:nocover][:card]": the same slot page for a game of that system, its
    // slim row in the medium's plastic (`app_save_media`; `:card` the full medium beside its blocks, as with Memory card
    // in the pause menu on), with a made-up cover (`:nocover` without, the console's picture in the slots).
    let (slot_state, slot_system) = match state.split_once(':').filter(|(n, _)| matches!(*n, "slot-save" | "slot-load" | "slot-auto" | "slot-undo" | "slot-empty" | "slot-restart")) {
        Some((n, rest)) => (n.to_string(), Some((rest.split(':').next().unwrap_or("gbc").to_string(), !rest.split(':').any(|p| p == "nocover"), rest.split(':').any(|p| p == "card")))),
        None => (state.clone(), None),
    };
    // "slot-save:<system>:fill=<ms>": Save pressed that long ago, its picture on the way into the slot (`save_fill`).
    let fill_ms: Option<u64> = state.split(':').find_map(|p| p.strip_prefix("fill=")?.parse().ok());
    let fill_run: Option<String> = state.split(':').find_map(|p| p.strip_prefix("fill-run=")).map(String::from);
    let state = &slot_state;
    if let Some((system, cover, card)) = &slot_system {
        app.save_media_snapshot(system, *cover, *card);
    }
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
        "slot-load" | "slot-auto" | "slot-undo" | "slot-restart" => (2, 2),
        "slot-empty" => (2, 3),
        "slot-one" => (1, 0),
        _ => (if page == Page::Main { 3 } else { 0 }, 0),
    };
    let load_at = match state.as_str() {
        "slot-auto" => 0,
        "slot-undo" => 1,
        "slot-restart" => demo_states.load_targets().len(),
        _ => slot + 2,
    };
    let shows_state = state != "slot-restart" && slot == 2 || matches!(state.as_str(), "slot-auto" | "slot-undo" | "slot-one");
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
        app.running = Some(Running { system: system.into(), name: name.into(), path: PathBuf::from(path), emulator: emulator.into(), core: String::new(), started: Instant::now(), pgid: None, options: Default::default() });
    }
    if game_standalone {
        let xbox = menu_kind == Some("xbox");
        app.running = Some(Running {
            system: if xbox { "xbox" } else { "ps2" }.into(),
            name: if xbox { "Jade Empire" } else { "Okage: Shadow King" }.into(),
            path: PathBuf::from(if xbox { "/storage/roms/xbox/Jade Empire (USA, Europe).iso" } else { "/storage/roms/ps2/Okage - Shadow King (USA).chd" }),
            emulator: if xbox { "hakux" } else { "armsx2" }.into(),
            core: String::new(),
            started: Instant::now(),
            pgid: None,
            options: Default::default(),
        });
    }
    // "hardcore-menu", or ":hardcore" / ":hardcore-ask" after a state: RetroAchievements' Hardcore holds (`app_achievements`).
    let hard = name == "hardcore-menu" || state.split(':').any(|p| p == "hardcore");
    let hard_ask = state.split(':').any(|p| p == "hardcore-ask");
    settings.hardcore = hard;
    app.menu = Some(QuickMenu {
        sel,
        frame,
        game_rect: None,
        saved,
        states: Some(demo_states.clone()),
        slot,
        filled: demo_states.filled(),
        loads: demo_states.load_targets(),
        load_at,
        restart: !game_standalone && guest.is_none() && !one_slot,
        quitting: false,
        discarding: false,
        without_saving: hard,
        window: None,
        frozen: None,
        retroarch: false,
        hosted: !game_standalone && guest.is_none(),
        page,
        settings: Some(settings),
        note: None,
        restore_armed: false,
        live: Default::default(),
        emu,
        cheats: snapshot_cheats(name, &tmp).filter(|_| !hard),
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
        achievements: (name.starts_with("achievements-") || name == "hardcore-menu").then(|| pocket_ui::achievements::snapshot::Snapshot { hardcore: hard, ..app_achievements::demo_snapshot() }),
        pixel_item: false,
        pixel: None,
        players_item: false,
        players: None,
        zoom: None,
        // "zoom-menu" and a state ending in ":zoomrow" list Zoom on the main page; else as the settings say.
        zoom_row: app.zoom_row_on() || name == "zoom-menu" || state.ends_with(":zoomrow"),
        zoom_back: app_zoom::Back::Main,
        zoom_pending: false,
        saves: app_achievements::hardcore_saves(pocket_ui::pause_saves::Saves::Slots, hard),
        disc: None,
        earlier: None,
        save_trouble: None,
        saving: None,
        hardcore_ask: hard_ask,
        shortcut_list: Vec::new(),
    });
    if let (Some(ms), Some(m)) = (fill_ms, app.menu.as_mut()) {
        let at = Instant::now();
        pocket_ui::frame_clock::freeze(at);
        m.saved = Some((m.frame.as_ref().map(|f| Image { w: f.w, h: f.h, px: f.px.clone() }), std::time::SystemTime::now()));
        m.saving = Some(pocket_ui::save_fill::Saving::new(m.slot, at));
        pocket_ui::frame_clock::step(Duration::from_millis(ms));
    }
    // "slot-save:<system>:fill-run=<run|skip>": A on Save as a player presses it, on the frozen clock, and where the menu and the
    // game stand at each moment (`save-run …` lines): `run` lets the picture play out, `skip` ends it with a button at 300 ms.
    if let Some(how) = fill_run.filter(|_| app.menu.is_some()) {
        let line = |app: &App, step: &str| println!("save-run {step} saving={} menu={}", app.save_filling(), app.menu.is_some());
        let at = Instant::now();
        pocket_ui::frame_clock::freeze(at);
        line(&app, "before");
        app.menu_input(CONFIRM);
        line(&app, "pressed");
        pocket_ui::frame_clock::step(Duration::from_millis(300));
        app.save_fill_tick();
        line(&app, "at-300ms");
        if how == "skip" {
            println!("save-run skip-used-the-press={}", app.skip_save_fill());
        } else {
            pocket_ui::frame_clock::step(Duration::from_millis(260));
            app.save_fill_tick();
        }
        line(&app, "end");
        println!("save-run skip-without-a-fill={}", app.skip_save_fill());
    }
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
    // "zoom-menu": the main page with Zoom selected; "zoom[:<2|3>[:<left|right|top|bottom|step>...]]": its page over the
    // background picture given (`app_zoom`).
    if name == "zoom-menu" {
        if let Some(m) = app.menu.as_mut() {
            m.saved = None;
            m.sel = m.items().iter().position(|i| *i == MainItem::Zoom).unwrap_or(0);
        }
    }
    if name == "zoom" {
        app.zoom_snapshot(state);
    }
    // "…:save=full", "quit-unsaved[:<row>][:unwritable]": saves that don't reach the card (`app_save_trouble`).
    app.save_trouble_snapshot(state);
    if name == "load-blocked" {
        if let Some(menu) = app.menu.as_mut() { menu.page = Page::LoadBlocked; }
        match state.split(':').nth(1) {
            Some("a") => app.menu_input(CONFIRM),
            Some("b") => app.menu_input(CANCEL),
            _ => {}
        }
    }
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
    // "menu:<kind>:<row>:forget": as with Settings > Continue where I quit off.
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
        m.sel = if name == "game-quick" { 0 } else { n(0).min(rows.len() - 1) };
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
    // "shortcut-list[:<row>]": that game's Runs on → Shortcuts, every shortcut on and RetroArch's Rewind too
    // (`app_shortcut_list`).
    if name == "shortcut-list" && app.emu_choice_snapshot_in_menu(0) {
        if let Some(m) = app.menu.as_mut() {
            (m.hosted, m.retroarch, m.settings) = (false, true, None);
        }
        use pocket_ui::shortcut_list::{retroarch, Game};
        app.show_shortcut_list(retroarch(Game { on: Default::default(), rewind: true, kids: false, easy: false }));
        if let Some(m) = app.menu.as_mut() {
            m.page = Page::ShortcutList { sel: n(0) };
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
        // At rest: the front's turn (220 ms) and the lift (120 ms) are over, as in every other look's snapshot.
        app.moved_at = Instant::now() - Duration::from_secs(2);
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
    if state.ends_with(":startup-resize") {
        app.raised();
        assert!(!app.wait_for_full_size(640, 480), "pause must paint without the raise timeout");
        assert!(app.held_frame_due().is_none());
        let before = app.menu.as_ref().expect("pause menu").sel;
        app.menu_input(Button::Down);
        assert_ne!(app.menu.as_ref().expect("pause stays open").sel, before, "pause menu accepts navigation");
    }
    app.frozen_snapshot();
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = snap_canvas(&mut buf, w, h);
    pocket_ui::game_card::BODY_SEEN.set((0, 0));
    app.paint(&mut c);
    app.transition_snapshot(&mut c);
    // What tests/brick_look_proportions.rs measures, in the canvas' pixels: the switcher's selected card (with its touch
    // margin, as far as its bezel reaches), the heading pill's bottom and the game card's scrolling body.
    if let Some(sw) = app.switcher.as_ref() {
        if let Some((x, y, cw, ch, _)) = sw.card_areas.iter().find(|a| a.4 == sw.sel) {
            println!("switcher-card x={x} y={y} w={cw} h={ch} hints={} pill={} canvas={}x{}", pocket_ui::kit::hint_bar_top(c.h), pocket_ui::switcher::heading_bottom(), c.w, c.h);
        }
    }
    print_card_body(c.w, c.h);
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
        let passes = pocket_ui::gfx::design_passes();
        for i in 0..105 {
            let start = cpu();
            paint(app, &mut snap_canvas(&mut buf, w, h));
            let painted = cpu() - start;
            let start = cpu();
            if (w, h) == panel {
                paint(app, &mut Canvas::new(&mut output, w, h));
            } else {
                // The live frame: the design canvas and its reduction, or painted natively (`POCKETUI_PAINT`).
                scaler.render(&mut output, panel, &mut |c| paint(app, c));
            }
            let framed = cpu() - start;
            if i >= 4 { paint_us.push(painted); frame_us.push(framed); }
            std::hint::black_box(&output);
        }
        paint_us.sort_by(f64::total_cmp);
        frame_us.sort_by(f64::total_cmp);
        let how = pocket_ui::scaled_layout::native_divisor(panel).map_or(pocket_ui::scaled_layout::Paint::Scaled, |_| pocket_ui::scaled_layout::Paint::Native);
        let passes = (pocket_ui::gfx::design_passes() - passes) as f64 / 210.0;
        println!("bench-frame state={state} panel={}x{} logical={w}x{h} paint={how:?} paint_us={:.0} frame_us={:.0} p95_us={:.0} design_passes={passes:.1}",
            panel.0, panel.1, paint_us[50], frame_us[50], frame_us[95]);
        print_design_sites();
    }
    write_png(&buf, w, h, out);
}

thread_local! {
    /// The snapshot buffer `snap_canvas` last painted natively (its address; 0: none): `write_png` writes its front as
    /// the panel's picture. Any other buffer is a design canvas, reduced as before.
    static NATIVE_SHOT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The panel a `--snapshot` of the design canvas `w` x `h` is painted natively for (`POCKETUI_PAINT=native`,
/// `scaled_layout::native_divisor`); `None`: painted at design size and reduced, as before.
pub(crate) fn snapshot_native(w: u32, h: u32) -> Option<(u32, u32)> {
    let panel = snapshot_panel_dims();
    (std::env::var_os("POCKETUI_SNAPSHOT_NATIVE").is_none() && pocket_ui::scaled_layout::native_divisor(panel) == Some(2) && (w, h) == (panel.0 * 2, panel.1 * 2)).then_some(panel)
}

/// A box snapshot's `card:<system>:<name>:<keys>` split after the keys' first turn (`r` or `l`) when keys follow it:
/// (up to the turn, the rest), each a `card:` state of its own.
fn after_turn(rest: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = rest.split(':').collect();
    let keys = *parts.get(3)?;
    let at = keys.find(['r', 'l'])? + 1;
    if at == keys.len() || !keys.chars().all(|k| "ydrlfcuab".contains(k)) {
        return None;
    }
    let with = |k: &str| {
        let mut p = parts.clone();
        p[3] = k;
        p.join(":")
    };
    Some((with(&keys[..at]), with(&keys[at..])))
}

/// The canvas a snapshot paints into `buf` (`w` x `h` design pixels, `snapshot_dims`): that design canvas, or painted
/// natively (`snapshot_native`) the panel's pixels at the front of `buf`, which `write_png` then writes as they are.
/// The game card's scrolling body in the last paint, on a canvas `cw` x `ch` (tests/brick_look_proportions.rs).
fn print_card_body(cw: i32, ch: i32) {
    let (top, bottom) = pocket_ui::game_card::BODY_SEEN.get();
    if bottom > 0 {
        println!("card-body top={top} bottom={bottom} hints={} canvas={cw}x{ch}", pocket_ui::kit::hint_bar_top(ch));
    }
}

pub(crate) fn snap_canvas(buf: &mut [u32], w: u32, h: u32) -> Canvas<'_> {
    match snapshot_native(w, h) {
        Some(p) => {
            NATIVE_SHOT.set(buf.as_ptr() as usize);
            Canvas::screen(&mut buf[..(p.0 * p.1) as usize], p, 2)
        }
        None => Canvas::new(buf, w, h),
    }
}

/// The benches' list of code not painted natively yet (`gfx::design_sites`): where, how often, how many screen pixels.
fn print_design_sites() {
    for (file, line, calls, px) in pocket_ui::gfx::design_sites().into_iter().take(12) {
        println!("bench-design-site {file}:{line} calls={calls} screen_px={px}");
    }
}

/// `App::eject_snapshot`, whose frames are painted at design size (as before, whatever `POCKETUI_PAINT` says).
fn eject_shot(app: &mut App, buf: &mut [u32], w: u32, h: u32, out: &str) -> bool {
    let was = NATIVE_SHOT.replace(0);
    let done = app.eject_snapshot(buf, w, h, out);
    if !done {
        NATIVE_SHOT.set(was);
    }
    done
}

/// A 0RGB frame as an RGB PNG (`--snapshot`).
fn write_png(buf: &[u32], w: u32, h: u32, out: &str) {
    pocket_ui::text::boxes::finish(buf, w as i32, h as i32);
    // The pages' main pictures in the canvas' pixels (`POCKETUI_SHARES`, tests/brick_proportions.rs).
    for line in pocket_ui::shares::take(w, h) {
        println!("{line}");
    }
    let panel = snapshot_panel_dims();
    let mut reduced = Vec::new();
    let native = snapshot_native(w, h).is_some() && NATIVE_SHOT.get() == buf.as_ptr() as usize;
    let (buf, w, h) = if native {
        (&buf[..(panel.0 * panel.1) as usize], panel.0, panel.1)
    } else if (w, h) != panel {
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
/// Whether About has a row of that name: the Linux card's address, logs' folder and licences' folder are not on Android's
/// handheld view (Android owns the network; the logs and the texts are not files the player can open there).
fn about_has(row: &str, android: bool) -> bool {
    !(android && matches!(row, "Network" | "Logs" | "Licences"))
}

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
    systems.iter().find(|s| s.name == name).map_or_else(|| pocket_ui::document_library::system_name(name), |s| s.fullname.clone())
}

/// Spine side label: the name people use for the console, the same kind of name for every system
/// (`pocket_ui::system_names`). A system the table doesn't know is its own name without a note in brackets
/// ("Sega Dreamcast (experimental)"), or its id in capitals when it is a short id with no games ("ngp", "3do").
fn system_short(systems: &[System], name: &str) -> String {
    if let Some(s) = pocket_ui::system_names::short(name) {
        return s.to_string();
    }
    // The pictures of the player's own shelf (`app_share_shelf`) are in this folder, not a system's.
    if name == "shelf" && !systems.iter().any(|s| s.name == name) {
        return "Shelf picture".into();
    }
    match systems.iter().find(|s| s.name == name) {
        Some(s) => pocket_ui::system_names::spine_name(name, &s.fullname),
        None if name.len() <= 4 => name.to_uppercase(),
        None => pocket_ui::system_names::spine_name(name, &system_label(systems, name)),
    }
}

/// Edge stripe colour per system; unknown systems get a muted colour derived from the name. Shared
/// with pocket-core, whose Glow frame lights up in it.
fn stripe(system: &str) -> u32 {
    pocket_ui::bezel::system_colour(system)
}

#[cfg(target_os = "linux")]
fn spawn_game(proxy: EventLoopProxy<UserEvent>, run: &Run, env: &[(&str, String)]) -> Option<i32> {
    use std::os::unix::process::CommandExt;
    let (system, path, core) = (run.system.as_str(), run.path.as_path(), run.core.as_str());
    let started = Instant::now();
    if system == "ports" && !pocket_ui::device::darkos_port() && !pocket_ui::ports::trimui_standalone() {
        // A PortMaster unpacked by an older pocket-ui lacks execute bits (gptokeyb): repair it before the port starts.
        if let Err(e) = pocket_ui::ports::ensure_ready() {
            eprintln!("ports: {e}");
        }
    }
    let mut cmd = if let Some(dir) = test_driver_dir().filter(|_| std::env::var_os("POCKETUI_TEST_REAL_CORE").is_none()) {
        // Same process group and exit notification as a game, with a temporary stand-in executable.
        let mut c = std::process::Command::new(dir.join("run-game"));
        c.arg(path);
        // The stand-in never outlives a pocket-ui that was killed (as pocket-core below).
        control::die_with_parent(&mut c);
        c
    } else if let Some(c) = (system == "ports").then(|| pocket_ui::ports::trimui_launch_command(path)).flatten() {
        // Native TrimUI ports prepare their runtime in the child and never pass through pocket-core or runemu.sh.
        c
    } else if run.player == Player::Hosted {
        let mut c = std::process::Command::new(host_bin());
        c.arg(pocket_ui::store::core_path(core)).arg(path).arg("--system").arg(system);
        // pocket-ui puts ROCKNIX's quantum back when the game ends (`ui_sound::game_ended`), in order with the shelf's own.
        c.env("POCKET_CORE_QUANTUM_OWNER", "pocket-ui");
        // A pocket-ui that crashes takes pocket-core with it (SIGTERM: it saves and quits).
        control::die_with_parent(&mut c);
        c
    } else if pocket_ui::device::darkos_port() {
        let mut c = std::process::Command::new("nsenter");
        c.args(["--target", &std::env::var("DARKOS_LAUNCHER_PID").ok()?, "--mount", "--root", "--wd=/tmp", "--"]);
        c.arg("/usr/bin/python3").arg("-I")
            .arg(std::path::Path::new(&std::env::var("DARKOS_ROOT").ok()?).join("port-data/host-launch.py"))
            .arg(system).arg(path);
        control::die_with_parent(&mut c);
        c
    } else if matches!(run.player, Player::Standalone(standalone::Emu::Hakux, _)) {
        // Xbox: ROCKNIX's RetroArch on the hakuX core in pocket-ui's folder, with our own appendconfig (`hakux`,
        // `standalone::launch` has prepared it). Not runemu.sh: that loads cores from /tmp/cores and writes the config itself.
        let mut c = pocket_ui::hakux::command(path);
        c.current_dir("/tmp");
        c
    } else {
        let mut c = std::process::Command::new(std::env::var("POCKETUI_RUNEMU").unwrap_or_else(|_| "/usr/bin/runemu.sh".into()));
        c.arg(path).arg(format!("-P{system}")).arg(format!("--core={core}")).arg(format!("--emulator={}", run.player.name())).arg("--controllers=");
        // A writable working directory: start_armsx2.sh tests `[ "$IRES" > "0" ]`, a redirect to a file "0",
        // which fails in the read-only / and drops the PS2 internal resolution back to 1x.
        c.current_dir("/tmp");
        c
    };
    cmd.envs(env.iter().map(|(k, v)| (*k, v)));
    // Standalone SDL guests receive identity-bound, normalised virtual seats. The helper creates
    // them off the UI thread; its lifetime and process group are exactly the game's.
    if pocket_ui::pad::bridge::bridged(run.player.name()) && test_driver_dir().is_none() {
        match pocket_ui::pad::bridge::wrap(&cmd) {
            Ok(bridged) => cmd = bridged,
            Err(e) => { eprintln!("controller bridge: {e}"); return None; }
        }
    }
    // pocket-core announces its exit save on a pipe, so a switch needn't wait for the process to end (`fast_switch`).
    let handoff = (run.player == Player::Hosted).then(|| pocket_ui::fast_switch::Handoff::attach(&mut cmd)).flatten();
    // Own process group, so pause and quit reach every process the game starts.
    let child = if system == "ports" { standalone::spawn_port(&mut cmd) } else { cmd.process_group(0).spawn() };
    let pgid = child.as_ref().ok().map(|c| c.id() as i32);
    if test_driver_dir().is_some() {
        if let Some(pid) = pgid { println!("test game pid {pid}"); }
    }
    let observation = if let (Some(h), Some(g)) = (handoff, pgid) {
        let proxy = proxy.clone();
        Some(h.watch(g, move || {
            let _ = proxy.send_event(UserEvent::GameSaved { pgid: g });
        }))
    } else { None };
    let hosted = run.player == Player::Hosted;
    let moment_base = pocket_ui::slots::base_for_core(system, path.file_stem().and_then(|s| s.to_str()).unwrap_or_default(), &run.core);
    std::thread::spawn(move || {
        let ok = child.ok().and_then(|mut c| c.wait().ok()).is_some_and(|s| s.success());
        // Drain the launch evidence on the waiter thread, before publishing GameExited. A delayed/broken pipe
        // is Unknown and cannot trigger fallback. A FRAME already in the pipe survives even SIGKILL.
        let start = observation.and_then(|rx| rx.recv_timeout(Duration::from_millis(200)).ok()).unwrap_or_default();
        let moments = hosted && !ok && pocket_ui::fallback::has_moments(&moment_base);
        pocket_ui::fast_switch::mark("game process ended");
        if let Some(g) = pgid {
            pocket_ui::fast_switch::note_ended(g);
        }
        let _ = proxy.send_event(UserEvent::GameExited { secs: started.elapsed().as_secs(), ok, pgid, start, moments });
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
        let _ = proxy.send_event(UserEvent::GameExited { secs: started.elapsed().as_secs(), ok, pgid, start: Default::default(), moments: false });
    });
    pgid
}

#[cfg(not(any(target_os = "linux", target_os = "emscripten", target_os = "android")))]
fn spawn_app(proxy: EventLoopProxy<UserEvent>, _cmd: std::process::Command) -> Option<i32> {
    // Desktop preview: pretend the stream failed to start after two seconds.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let _ = proxy.send_event(UserEvent::GameExited { secs: 2, ok: false, pgid: None, start: Default::default(), moments: false });
    });
    None
}

#[cfg(not(any(target_os = "linux", target_os = "emscripten", target_os = "android")))]
fn spawn_game(proxy: EventLoopProxy<UserEvent>, _run: &Run, _env: &[(&str, String)]) -> Option<i32> {
    // Desktop preview: pretend the game ran for two seconds.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let _ = proxy.send_event(UserEvent::GameExited { secs: 2, ok: true, pgid: None, start: Default::default(), moments: false });
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
            let found = pocket_ui::pad::linux::supervise_ui(move |e| pads.send(UserEvent::Pad(e)));
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

/// A private input and capture harness for user journeys, headless by default. Normal starts never enable it.
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
            // "tap <x> <y>": a finger down and up at that place, in fractions of the screen (0.5 0.5 is the middle),
            // as the touch screen sends it (`app_touch`), for the touch journeys.
            if let Some((x, y)) = line.trim().strip_prefix("tap ").and_then(|r| r.split_once(' ')).and_then(|(x, y)| Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))) {
                use pocket_ui::touch::{Phase, TouchEvent};
                for phase in [Phase::Down, Phase::Up] {
                    let _ = proxy.send_event(UserEvent::Touch(TouchEvent { phase, x, y }));
                }
                continue;
            }
            // "play <system> <file>": the smoke test's way to start its own test game through the same `launch` as the shelf.
            #[cfg(feature = "brick")]
            if let Some((system, file)) = line.trim().strip_prefix("play ").and_then(|r| r.split_once(' ')) {
                let file = PathBuf::from(file.trim());
                if file.is_absolute() && file.components().all(|c| c.as_os_str() != "..") && file.components().any(|c| c.as_os_str() == ".pocket-ui-test") && file.is_file() {
                    let _ = proxy.send_event(UserEvent::Play(system.trim().to_string(), file));
                }
                continue;
            }
            if test_driver_dir().is_some() {
                if let Some(page) = line.trim().strip_prefix("page ") {
                    let _ = proxy.send_event(UserEvent::TestPage(page.to_string()));
                    continue;
                }
            }
            // Test driver only, for the Test buttons journey: controllers of their own, as the pad threads introduce them and
            // send what they press. "tconnect <source> <key> <name…>", "tbtn <source> <up|down|left|right|a|b|x|y|l1|r1|l2|r2|
            // select|start|home> <down|up>" (the roles the App gets), "taxis <source> <l2|r2|lx|ly|rx|ry> <value>".
            if test_driver_dir().is_some() {
                let words: Vec<&str> = line.split_whitespace().collect();
                let event = match words.as_slice() {
                    ["tdisconnect", source] => source.parse().ok().map(|source| PadEvent::Disconnected { source }),
                    ["tconnect", source, key, name @ ..] => source.parse().ok().map(|source| PadEvent::Connected {
                        source,
                        controller: pocket_ui::pad::Controller { key: (*key).into(), name: name.join(" "), gamepad: true, node: String::new() },
                    }),
                    ["tbtn", source, name, state] => {
                        let button = match *name {
                            "up" => Some(Button::Up), "down" => Some(Button::Down), "left" => Some(Button::Left), "right" => Some(Button::Right),
                            "a" => Some(CONFIRM), "b" => Some(CANCEL), "x" => Some(Button::North), "y" => Some(Button::West),
                            "l1" => Some(Button::L1), "r1" => Some(Button::R1), "l2" => Some(Button::L2), "r2" => Some(Button::R2),
                            "select" => Some(Button::Select), "start" => Some(Button::Start), "home" => Some(Button::Mode),
                            _ => None,
                        };
                        source.parse().ok().zip(button).map(|(source, button)| PadEvent::Button { source, button, pressed: *state == "down" })
                    }
                    ["taxis", source, name, value] => {
                        let axis = match *name {
                            "l2" => Some(Axis::L2), "r2" => Some(Axis::R2), "lx" => Some(Axis::LeftX), "ly" => Some(Axis::LeftY),
                            "rx" => Some(Axis::RightX), "ry" => Some(Axis::RightY),
                            _ => None,
                        };
                        source.parse().ok().zip(axis).zip(value.parse().ok()).map(|((source, axis), value)| PadEvent::Axis { source, axis, value })
                    }
                    _ => None,
                };
                if let Some(event) = event {
                    let _ = proxy.send_event(UserEvent::Pad(event));
                    continue;
                }
            }
            let capture_key = test_driver_dir().is_some().then(|| line.trim().strip_prefix("nav-after ")).flatten();
            let button = match capture_key.unwrap_or(line.trim()) {
                "frame" if test_driver_dir().is_some() => {
                    let _ = proxy.send_event(UserEvent::TestFrame(true));
                    continue;
                }
                // The navigation explorer's record of the frame without its picture (`app_nav_probe`, tests/nav_explorer.rs).
                "nav" if test_driver_dir().is_some() => {
                    let _ = proxy.send_event(UserEvent::TestFrame(false));
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
                "r2axis" if test_driver_dir().is_some() => {
                    for value in [0.9, 0.95, 0.0] {
                        let _ = proxy.send_event(UserEvent::Pad(PadEvent::Axis { source: usize::MAX, axis: Axis::R2, value }));
                    }
                    continue;
                }
                "home" => Button::Mode,
                // The roles of the hints (the top button is X, the left one Y), in both Button layouts: the FIFO is
                // not a physical pad (`button_layout`). They were crossed until the Button layout setting.
                "x" => Button::North,
                "y" => Button::West,
                "select" => Button::Select,
                "start" => Button::Start,
                "power" => Button::Power,
                "volup" => Button::VolUp,
                "voldown" => Button::VolDown,
                "quit" => {
                    let _ = proxy.send_event(UserEvent::Quit);
                    continue;
                }
                // What SELECT + START held opens: the dialog before EmulationStation (`app_rescue`).
                "leave" => {
                    let _ = proxy.send_event(UserEvent::Leave);
                    continue;
                }
                // The screen on from light sleep, and no button: a helper's next key then goes to the page it reads in
                // the status file, never to a page a waking B would have left (docs/DEVICE-TESTS.md).
                "wake" => {
                    let _ = proxy.send_event(UserEvent::Wake);
                    continue;
                }
                "dark" => {
                    let _ = proxy.send_event(UserEvent::ScreenOff);
                    continue;
                }
                // SELECT and START held and let go: Kids Mode's way out, for its journey.
                "holddown" | "holdup" if test_driver_dir().is_some() => {
                    let pressed = line.trim() == "holddown";
                    for button in [Button::Select, Button::Start] {
                        let _ = proxy.send_event(UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed }));
                    }
                    continue;
                }
                // HOME held and let go: for HOME + SELECT (the game switcher), in the journeys.
                "homedown" | "homeup" if test_driver_dir().is_some() => {
                    let pressed = line.trim() == "homedown";
                    let _ = proxy.send_event(UserEvent::Pad(PadEvent::Button { source: usize::MAX, button: Button::Mode, pressed }));
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
            if capture_key.is_some() {
                let _ = proxy.send_event(UserEvent::TestPressFrame(button));
            } else {
                for pressed in [true, false] {
                    let _ = proxy.send_event(UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed }));
                }
            }
        }
    });
}

impl App {
    /// Notes what an input asked for, for the navigation explorer's `@feedback` (nothing without `POCKETUI_NAV_PROBE`).
    fn note_feedback(&mut self, what: &str) {
        if pocket_ui::nav_probe::on() {
            self.feedback.push(what.to_string());
        }
    }

    /// The haptic tick for a move: a direction (or a held one stepping again) changed what the page shows
    /// (`feedback_view`, the same mark the UI sounds compare).
    fn tick_after(&mut self, before: pocket_ui::ui_sound::View) {
        if before != self.feedback_view() {
            self.note_feedback("tick");
            pocket_ui::rumble::tick();
        }
    }

    /// Plays what `input` sounds like, now that it turned `before` into the screen as it is.
    fn sound_after(&mut self, input: pocket_ui::ui_sound::Input, before: pocket_ui::ui_sound::View) {
        use pocket_ui::ui_sound;
        let refused = ui_sound::take_refused();
        // The input played its own sound (X opening a case, A starting the launch's): not a second one.
        if ui_sound::take_played() {
            self.note_feedback("sound:own");
            return;
        }
        if let Some(sound) = ui_sound::sound_for(input, before, self.feedback_view(), refused) {
            if pocket_ui::nav_probe::on() {
                self.feedback.push(format!("sound:{sound:?}"));
            }
            self.play_sound(sound);
        }
    }

    fn user_event_inner(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        pocket_ui::mark!("event", "{:?}", std::mem::discriminant(&ev));
        // B while a phone game's first start loads Java: the load stops, the cartridge comes back out (`app_loading`).
        if let UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) = &ev {
            if self.java_press(*button) || self.romm_press(*button) { return; }
        }
        // A start's skip comes before quick settings, notices and volume keys consume their input. HOME and volume
        // never become a game button in pocket-core, so the UI must deliver their skip itself.
        if (self.running.is_some() || self.pending_launch.is_some()) && matches!(ev, UserEvent::Pad(PadEvent::Button { pressed: true, .. })) && self.launch_button() {
            self.home_tap = false;
            self.start_tap.disarm();
            return;
        }
        if self.pending_launch.is_some() && matches!(ev, UserEvent::Pad(_) | UserEvent::Touch(_))
            && !matches!(ev, UserEvent::Pad(PadEvent::Button { button: Button::VolUp | Button::VolDown, .. })) { return; }
        // Test buttons takes every button but the volume and power keys, B, START and HOME included (`app_test_buttons`).
        if let UserEvent::Pad(pe) = &ev {
            if self.test_buttons_swallows(pe) {
                self.request_redraw();
                return;
            }
        }
        // Kids Mode takes the buttons first (`app_kids`).
        if let UserEvent::Pad(pe) = &ev {
            if self.kids_pad(pe) {
                self.request_redraw();
                return;
            }
        }
        // The dialog before EmulationStation takes the buttons while it is open (`app_rescue`).
        if let UserEvent::Pad(pe) = &ev {
            if self.leave_input(el, pe) {
                return;
            }
        }
        // A or B closes the notice that is up (`app_messages`).
        if let UserEvent::Pad(pe) = &ev {
            if self.close_notice_pad(pe) {
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
            let system_visible_before = self.sys.has_visible_content() || self.menu.is_some() || self.switcher.is_some();
            let out = self.sys.pad(pe, &ctx, Instant::now());
            if out.select_used {
                self.select_alone = false;
            }
            self.volume_step |= out.volume_step;
            // dArkOS: direct return from the quick settings or power menu.
            if out.leave {
                self.request_back_to_es();
                self.request_redraw();
            }
            // B or the power button closed the power menu: a game it paused goes on.
            if out.power_cancelled && std::mem::take(&mut self.power_paused_game) {
                self.resume_game(None);
            }
            if out.consumed {
                let system_visible_after = self.sys.has_visible_content() || self.menu.is_some() || self.switcher.is_some();
                if pocket_ui::idle::system_input_needs_redraw(self.sys.quiet_game_controls(&ctx), system_visible_before, system_visible_after) {
                    self.request_redraw();
                }
                return;
            }
        }
        match ev {
            UserEvent::Quit => el.exit(),
            UserEvent::KidsTick => self.kids_tick(),
            UserEvent::LaunchPrepared => self.launch_poll(),
            #[cfg(feature = "brick")]
            UserEvent::UiScreenshot(failed) => self.ui_screenshot_done(failed),
            #[cfg(feature = "brick")]
            UserEvent::EmuChoiceWritten(result) => self.emu_choice_written(result),
            UserEvent::TestPage(page) => {
                if let Some(dir) = test_driver_dir() {
                    self.settings_page = match page.as_str() {
                        "achievements" => self.open_achievements(),
                        "battery" => { pocket_ui::battery_history::load_now(&self.data_dir); Some(SettingsPage::Battery(0)) },
                        "backup" => self.open_backup_at(dir.join("backups"), dir.join("saves-backups")),
                        "sync" => self.open_sync(),
                        "storage" => self.open_storage(),
                        "tools" => self.open_tools(),
                        "history" => {
                            pocket_ui::play_log::open();
                            self.open_history(pocket_ui::play_history::Span::Week);
                            None
                        }
                        _ => panic!("unknown test page: {page}"),
                    };
                    self.request_redraw();
                }
            }
            UserEvent::TestPressFrame(button) => {
                if test_driver_dir().is_some() {
                    for pressed in [true, false] {
                        self.user_event(el, UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed }));
                    }
                    self.user_event_inner(el, UserEvent::TestFrame(false));
                }
            }
            UserEvent::TestPresent => {
                if test_driver_dir().is_some() {
                    let now = Instant::now();
                    let (_, w, h) = self.paint_headless(now);
                    if pocket_ui::trace::on() {
                        self.trace_frame(now.elapsed(), w, h);
                    }
                    self.launch_response_painted();
                    self.launch_poll();
                }
            }
            UserEvent::TestFrame(picture) => {
                if let Some(dir) = test_driver_dir() {
                    let now = Instant::now();
                    let (buf, w, h) = self.paint_headless(now);
                    // The headless driver has no window presentation. Keep its real paint time in the same trace
                    // as window frames, before text diagnostics, scaling and PNG encoding add capture overhead.
                    if pocket_ui::trace::on() {
                        self.trace_frame(now.elapsed(), w, h);
                    }
                    self.launch_response_painted();
                    // The status file says what this frame shows before the frame is announced: it is otherwise written
                    // by the loop's next idle step, so a helper reading it right after the picture got the page before
                    // the press (tests/user_journeys.rs, `the_status_file_already_says_what_the_frame_just_painted`).
                    let _ = self.status_tick(Instant::now());
                    if picture {
                        write_png(&buf, w, h, dir.join("frame.png").to_str().unwrap());
                    } else {
                        pocket_ui::text::boxes::finish(&buf, w as i32, h as i32);
                    }
                    self.nav_probe_done(&buf);
                    self.feedback.clear();
                    self.launch_poll();
                }
            }
            UserEvent::Leave => self.open_leave(pocket_ui::rescue::DialogKind::Leave),
            UserEvent::Wake => {
                if !self.sleep.wake_from_light("the command FIFO's wake") {
                    return;
                }
            }
            UserEvent::ScreenOff => {
                let ctx = self.sleep_ctx();
                self.sleep.screen_off_now(ctx);
            }
            #[cfg(feature = "brick")]
            UserEvent::Play(system, path) => {
                let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.launch(system, name, path);
            }
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
            UserEvent::BackToEs => self.back_to_es(el),
            UserEvent::Preview { seq, images } => {
                if !self.menu.as_mut().is_some_and(|m| m.live.receive(seq, images)) {
                    return;
                }
            }
            UserEvent::PlayedFiles(gen, files) => {
                if gen == self.played_scan {
                    self.played_files.extend(files);
                    self.cases_look_changed();
                    self.curator.touch();
                    self.request_redraw();
                }
            }
            UserEvent::GameExited { secs, ok, pgid, start, moments } => {
                if !self.drained(pgid, ok) {
                    self.game_exited(secs, ok, start, moments)
                }
            }
            UserEvent::GameSaved { pgid } => self.game_saved(pgid),
            UserEvent::MenuShot { seq, image } => self.menu_shot_arrived(seq, image),
            UserEvent::OptionsSaved(saved) => self.options_saved(saved),
            UserEvent::LoadBlocked(id) => {
                if standalone::still(id) {
                    if self.menu.is_none() { self.open_menu(); }
                    if let Some(menu) = self.menu.as_mut() { menu.page = Page::LoadBlocked; }
                    self.request_redraw();
                }
            }
            UserEvent::RetroArchStateChecked { started, action, confirmed } => {
                if self.retroarch_state_pending == Some(started) {
                    self.retroarch_state_pending = None;
                }
                if !confirmed && self.running.as_ref().is_some_and(|run| run.started == started) {
                    if self.menu.is_none() { self.open_menu(); }
                    if let Some(menu) = self.menu.as_mut() { menu.page = Page::RetroArchStateUnconfirmed(action); }
                    self.request_redraw();
                }
            }
            UserEvent::Sync(e) => self.sync_event(e),
            UserEvent::Tailscale(e) => self.tailscale_event(e),
            UserEvent::Romm(n) => self.romm_news(n),
            UserEvent::Web(e) => self.web_event(e),
            UserEvent::PocketSync(e) => self.pocket_sync_event(e),
            UserEvent::Bluetooth(e) => self.bluetooth_event(e),
            UserEvent::PerfFan(e) => self.perf_fan_event(e),
            UserEvent::Night(level) => self.night_light_event(level),
            UserEvent::Tools(e) => self.tools_event(e),
            UserEvent::Moonlight(e) => self.pc_streaming_event(e),
            UserEvent::Backup(e) => self.backup_event(e),
            UserEvent::Storage(e) => self.storage_event(e),
            UserEvent::Gamelist(index) => self.gamelist_event(index),
            UserEvent::GamelistDeferred => self.gamelist_defer(),
            UserEvent::AndroidApps(apps, why) => self.android_apps_event(apps, why),
            UserEvent::Gallery(e) => self.gallery_event(e),
            UserEvent::AlbumPicture(file, img) => self.album_picture(file, img),
            UserEvent::Curator(e) => self.curator_event(e),
            UserEvent::Achievements(e) => self.achievements_event(e),
            UserEvent::PixelPath(r) => self.pixel_path_event(*r),
            UserEvent::PixelSetup(rom, result) => self.pixel_setup_event(rom, *result),
            UserEvent::Players => self.players_event(),
            UserEvent::ControllerSaved(note) => self.controller_saved(note),
            // `merge_pad` turned it into `Pad` already.
            UserEvent::Merged(_) => {}
            UserEvent::Update(e) => self.update_event(e),
            UserEvent::Starter(e) => self.starter_event(e),
            UserEvent::GameCheck(gen, e) => self.game_check_event(gen, e),
            UserEvent::Covers(e) => self.covers_event(e),
            UserEvent::PortIndex => {
                self.cabinet.port_art_changed();
                self.search_covers_changed();
                self.covers.clear();
                self.pictures_changed();
                self.look_previews = None;
                if let Some(open) = self.card.as_mut().filter(|o| o.system == "ports") {
                    open.card.data.port_update = open.path.file_name().and_then(|n| n.to_str()).is_some_and(pocket_ui::ports::update_waiting);
                }
            }
            UserEvent::Texts(e) => self.texts_event(e),
            UserEvent::Identify(e) => self.identify_event(e),
            UserEvent::Picture(key, img) => {
                self.picture_arrived(key, img);
                return;
            },
            // It asks for a frame itself, only when the answer changed.
            UserEvent::CaseKnows(rom, progress, art) => {
                self.case_answer(rom, progress, art);
                return;
            }
            UserEvent::Online => self.auto_covers("online after boot"),
            UserEvent::LibraryRescanned(systems, key) => self.library_rescanned(systems, key),
            UserEvent::LibraryCacheStored(key) => self.library_ui.key = Some(key),
            UserEvent::LibraryScanDeferred => self.library_ui_rescan_after_game(),
            UserEvent::LibraryUnread { count, rose } => self.library_unread(count, rose),
            UserEvent::SystemListCheckDeferred => self.library_ui.system_list_check_due = true,
            UserEvent::SystemListUnread { fallback } => {
                // A rescan that kept the list on screen (`fallback: false`) leaves what the start found.
                self.system_list_fallback |= fallback;
                self.warn(if fallback {
                    app_starter::SYSTEM_LIST_LINE
                } else {
                    "Some systems couldn't be read. Settings › Games › Game check says why."
                });
            }
            UserEvent::GamesCard(change) => self.games_card_changed(change),
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
            UserEvent::BigMoments(gen, list) => self.big_moments_got(gen, list),
            UserEvent::Frozen(gen, list) => {
                if gen == self.frozen_generation() {
                    self.frozen_got_timed(list);
                }
                self.request_redraw();
            }
            UserEvent::LaunchPicture(path, p) => {
                let wanted = self.launch_at.is_some() && self.running.as_ref().is_some_and(|r| path == r.path);
                if wanted {
                    self.where_was_i_picture(p);
                    self.request_redraw();
                }
            }
            UserEvent::Setup(e) => {
                if e.changes_library() {
                    self.covers.clear();
                    self.pictures_changed();
                    self.look_previews = None;
                }
                let before_wifi = self.setup.as_ref().is_some_and(|s| !s.past_wifi());
                if let Some(setup) = self.setup.as_mut() {
                    setup.on_event(e);
                }
                // The first start's Wi-Fi step went on by itself after a connection: same as START there.
                if before_wifi && self.setup.as_ref().is_some_and(|s| s.past_wifi()) {
                    self.auto_covers("after the Wi-Fi step");
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
                // "For a child…" on the welcome, after your games (`app_kids`): its steps take the look step's place. With
                // no game it could start the look step comes instead: never a locked, empty shelf.
                if self.setup.as_mut().is_some_and(|s| s.take_kids_asked()) {
                    self.kids_open_flow(true);
                    if !self.kids_flow_has_games() {
                        self.kids_close_flow();
                        if let Some(s) = self.setup.as_mut() {
                            s.kids_no_games();
                        }
                    }
                }
                // The Style panel's Colours row: the next or previous set, as Settings › Look › Colours.
                if let Some(forward) = self.setup.as_mut().and_then(|s| s.take_colours_step()) {
                    self.step_colours(forward);
                }
                // Its Style row: Barlow or Retro, as Settings › Look › Style.
                if let Some(forward) = self.setup.as_mut().and_then(|s| s.take_font_step()) {
                    self.step_font(forward);
                }
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
                // Settings > Games > Box art & downloads > Pictures: kept at once; a choice that brings more downloads starts them,
                // Off stops the one that runs (after the file it is on).
                // Android's "Your games": a games folder of another app, read from now on; the emulator apps found, kept as
                // Plays with's choices.
                if let Some(folder) = self.setup.as_mut().and_then(|s| s.take_android_folder()) {
                    self.android_folder_added(&folder);
                }
                if let Some(offers) = self.setup.as_mut().and_then(|s| s.take_android_apps()) {
                    self.phone_take_offers(&offers);
                }
                if let Some(url) = self.setup.as_mut().and_then(|s| s.take_android_page()) {
                    self.phone_open_page(url);
                }
                // "How cozy should it be?": its level's switches, used at once and kept.
                if let Some(level) = self.setup.as_mut().and_then(|s| s.take_cozy()) {
                    self.adopt_cozy(level);
                }
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
                // The first start's Easy row (`Profile::look`): at once, the shelf behind the next steps is that one.
                if let Some(i) = self.setup.as_mut().and_then(|s| s.take_profile_look()) {
                    self.theme = Theme::ALL[i];
                    self.save_settings();
                    self.rebuild();
                }
                // A profile or a feature card (`features`): each switch is used at once and saved.
                if let Some((switches, later)) = self.setup.as_mut().and_then(|s| s.take_tour()) {
                    self.tour.later = later;
                    self.adopt_switches(&switches);
                    self.save_settings();
                }
                // The phone page without Wi-Fi, opened from Settings: A leads to the Wi-Fi settings.
                let wifi = self.setup.as_mut().and_then(|s| s.take_open_wifi());
                let starter = self.setup.as_mut().is_some_and(|s| s.take_open_starter());
                // Games came from the phone: the folders are read again now, the shelf swaps the list in when setup is left.
                if self.setup.as_mut().is_some_and(|s| s.take_phone_added()) {
                    self.library_rescan("games added");
                }
                // B on the welcome of the first start opened from Settings: back to its row, nothing finished.
                if starter {
                    self.setup = None;
                    self.settings_page = self.open_starter(false);
                    self.starter.from_extras = true;
                } else if done && self.setup.as_ref().is_some_and(|s| s.backed_out()) {
                    self.setup = None;
                    self.settings_page = Some(SettingsPage::List(settings_row("First-start setup")));
                } else if let Some(from) = done.then(|| self.setup.as_mut().and_then(|s| s.take_back_to_games())).flatten() {
                    // The Wi-Fi settings Add games asked for: leaving them goes back there, on the tile (and the phone page) that asked.
                    let games = self.new_setup().only(Only::Games);
                    self.setup = Some(games.back_on_way(from));
                } else if done && self.settings_page.is_none() && self.empty_ports() {
                    self.setup = None;
                } else if done {
                    self.finish_setup();
                }
                if let Some(from) = wifi {
                    self.setup = Some(self.new_setup().only(Only::Wifi).back_to_games(from));
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
            UserEvent::CoverPicked(outcome) => self.cover_picked(outcome),
            UserEvent::CoverReady { system, rom, png } => self.cover_ready(system, rom, png),
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
                        #[cfg(feature = "brick")]
                        if matches!(button, Button::F1 | Button::F2) {
                            self.brick_fkey(button, false);
                        }
                        self.start_tap.pressed(button, self.pad.is_pressed(Button::Select));
                        self.press(button);
                    } else {
                        // START on its own opens Settings, on the shelf and in a system's list alike; B comes back.
                        if self.start_tap.released(button) {
                            self.settings_page = Some(SettingsPage::List(app_settings_groups::first_row()));
                        }
                        if button == Button::Select && std::mem::take(&mut self.select_alone) && !self.big_on() {
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
        // A framebuffer journey uses the same input fixture as the headless tests, with a real window.
        if test_driver_dir().is_some() && std::env::var_os("POCKETUI_TEST_WINDOW").is_none() {
            return;
        }
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
        #[cfg(not(all(target_os = "android", feature = "handheld")))]
        self.surface.set_factory(move || {
            let context = softbuffer::Context::new(for_surface.clone()).map_err(|e| pocket_ui::frame::FrameError(format!("context: {e}")))?;
            softbuffer::Surface::new(&context, for_surface.clone()).map_err(|e| pocket_ui::frame::FrameError(format!("surface: {e}")))
        });
        #[cfg(all(target_os = "android", feature = "handheld"))]
        self.surface.set_factory(move || pocket_ui::native_surface::NativeSurface::new(&*for_surface));
        self.window = Some(window);
        pocket_ui::startup::mark("window");
        // Android: back in front, after a game too (`app_android`).
        #[cfg(all(target_os = "android", feature = "handheld"))]
        self.android_resumed();
        self.request_redraw();
    }

    // Android takes the window away in the background (`app_android`).
    #[cfg(all(target_os = "android", feature = "handheld"))]
    fn suspended(&mut self, _el: &ActiveEventLoop) {
        self.android_suspended();
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        // Several controllers (review SOL-09): what goes on is what changed for all of them together (`merge_pad`).
        let Some(ev) = self.merge_pad(el, ev) else { return };
        // The same rule as sleep's (`pad::counts_as_input`): a resting or drifting stick is nobody using the device.
        if matches!(ev, UserEvent::Touch(_)) || matches!(&ev, UserEvent::Pad(pe) if pocket_ui::pad::counts_as_input(pe)) {
            self.last_input = Instant::now();
        }
        // Cancel before L2/R2 gestures can consume the press and open another overlay. The press that ends a failed
        // start's way back is used up by it (`eject_input`).
        if self.eject_input(&ev) {
            self.request_redraw();
            return;
        }
        // after sleep has seen it.
        // dismissing pull is swallowed as one press, including subsequent travel until its release.
        let Some(ev) = self.trigger_event(ev) else { return };
        // Speed guard: numbered so a frame can say which input it shows (docs/LATENCY.md).
        if let UserEvent::Pad(PadEvent::Button { button, pressed, .. }) = &ev {
            pocket_ui::mark!("input", "{} {:?} {}", pocket_ui::trace::input(), button, if *pressed { "down" } else { "up" });
            pocket_ui::frame_dump::event(|| format!("input {button:?} {}", if *pressed { "down" } else { "up" }));
        }
        if matches!(ev, UserEvent::Pad(PadEvent::Button { pressed: true, .. })) {
            self.motion_input();
            // Any button ends the way back from a game, and does what it does on the shelf.
            self.skip_eject();
            // Any button ends the Save's picture and the game comes back; the press is that and nothing else (the
            // power key still goes on to the power menu).
            if self.skip_save_fill() && !matches!(ev, UserEvent::Pad(PadEvent::Button { button: Button::Power, .. })) {
                return;
            }
        }
        if let UserEvent::Touch(t) = ev {
            self.skip_eject();
            if self.skip_save_fill() {
                return;
            }
            if self.pending_launch.is_some() {
                self.launch_button();
                return;
            }
            if self.close_notice_touch(&t) {
                return;
            }
            self.touch_event(el, t);
            return;
        }
        // The shelf's key repeat ends with the release or another press, whatever page they go to (SOL-08).
        if let UserEvent::Pad(PadEvent::Button { button, pressed, .. }) = &ev {
            self.held = repeat_after(self.held, *button, *pressed);
        }
        // Raw diagnostics change no menu pixels. The manual's continuous stick scroll is handled below;
        // triggers have already become their button events.
        if matches!(ev, UserEvent::Pad(PadEvent::Raw { .. })) {
            return;
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
        if matches!(ev, UserEvent::Pad(PadEvent::Axis { .. }))
            && self.running.is_none() && self.settings_page != Some(SettingsPage::Manual) {
            return;
        }
        // One rule for releases: a release counts on the page that saw its press (`app_where::press_page`).
        if let UserEvent::Pad(PadEvent::Button { button, pressed, .. }) = &ev {
            self.note_press_page(*button, *pressed);
        }
        let moved_from = ((pocket_ui::rumble::menu_haptics() || pocket_ui::nav_probe::on())
            && matches!(&ev, UserEvent::Pad(PadEvent::Button { button: Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1 | Button::L2 | Button::R2, pressed: true, .. })))
        .then(|| self.feedback_view());
        let sound_input = match &ev {
            UserEvent::Pad(PadEvent::Button { button, pressed: true, .. }) => Some(pocket_ui::ui_sound::Input::Press(*button)),
            UserEvent::Pad(PadEvent::Button { button, pressed: false, .. }) => Some(pocket_ui::ui_sound::Input::Release(*button)),
            _ => None,
        };
        let sound_from = sound_input.map(|_| self.feedback_view());
        // A sound played by hand for an input that never reached `sound_after` doesn't silence this one.
        pocket_ui::ui_sound::take_played();
        self.volume_step = false;
        self.user_event_inner(el, ev);
        if let Some(before) = moved_from {
            self.tick_after(before);
        }
        if let (Some(input), Some(before)) = (sound_input, sound_from) {
            let input = if std::mem::take(&mut self.volume_step) { pocket_ui::ui_sound::Input::VolumeStep } else { input };
            self.sound_after(input, before);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) { return; }
        // Android's pad and touch screen (`app_android`).
        #[cfg(all(target_os = "android", feature = "handheld"))]
        if self.android_window_event(el, &event) { return; }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => self.request_redraw(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, repeat: false, .. },
                ..
            } => {
                if state == ElementState::Pressed && self.running.is_some() && self.launch_button() {
                    return;
                }
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
            // and scrolls like a finger (`app_touch`); a game keeps its own.
            WindowEvent::CursorMoved { position, .. } => {
                let size = self.window.as_ref().map_or((0, 0), |w| (w.inner_size().width, w.inner_size().height));
                self.mouse_moved(el, (position.x, position.y), size);
            }
            WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                let size = self.window.as_ref().map_or((0, 0), |w| (w.inner_size().width, w.inner_size().height));
                self.mouse_button(el, state == ElementState::Pressed, size);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.y / 50.0) as f32,
                };
                self.mouse_wheel(el, lines);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.launch_poll();
        // The core warm-up holds still while a game starts or runs and while the device sleeps (`core_prewarm`).
        pocket_ui::core_prewarm::set_quiet(self.core_prewarm_quiet());
        // Save's picture ends by itself: the game goes on (`app_save_media`).
        self.save_fill_tick();
        self.kids_about_to_wait();
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
        self.games_card_say();
        self.gamelist_resume();
        self.gallery_resume();
        // The Cabinet's TV may have finished behind Settings, a card or another look. Drain its answers here too,
        // so a completed request cannot keep an invisible Cabinet animating forever.
        if self.cabinet.tv.poll() {
            self.request_redraw();
        }
        // "New in this version", once, when the shelf is free (`app_tour`).
        self.tour_tick();
        let now = Instant::now();
        self.test_buttons_tick(now);
        STICKS.set_quiet(self.running.is_some() && self.menu.is_none() && self.switcher.is_none() && !self.sys.is_open());
        if quit::restart_due(self.updates.restart_at, now, self.running.is_some()) {
            self.exit_code = update::RESTART_CODE;
            el.exit();
            return;
        }

        // A game that ignored Quit: SIGTERM at the deadline, SIGKILL two seconds later.
        if let (Some(q), Some(g)) = (self.quit.as_mut(), self.running.as_ref().and_then(|r| r.pgid)) {
            // Never a writer: a killed exit save is a lost moment, and the card can be slow for reasons of its own.
            if pocket_ui::fast_switch::saving_now(g) {
                q.still_saving(now);
            }
            if let Some(sig) = q.due(now) {
                println!("quit: the game didn't end in time; {sig:?} to it");
                control::terminate_game(g, sig == quit::Signal::Kill);
            }
        }
        // The same for a game still ending after it handed over (`app_fast_switch`).
        let draining = self.drain_tick(now);
        // A held direction repeats only on the page its press was on: a page that came up over the shelf without a key (the
        // power menu, the dialog SELECT + START opens) doesn't scroll the shelf behind it.
        if self.held.is_some_and(|(b, _)| !self.on_press_page(b)) {
            self.held = None;
        }
        if let Some((b, next)) = self.held {
            if now >= next {
                let before = self.feedback_view();
                self.step(b);
                if pocket_ui::rumble::menu_haptics() || pocket_ui::nav_probe::on() {
                    self.tick_after(before);
                }
                self.sound_after(pocket_ui::ui_sound::Input::Repeat(b), before);
                // A held L2/R2 goes letter by letter slowly enough to read (`letters::REPEAT`).
                let every = if matches!(b, Button::L2 | Button::R2) || self.big_live() && matches!(b, Button::Up | Button::Down) { pocket_ui::letters::REPEAT } else { REPEAT_EVERY };
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
        let cheats_busy = self.menu.as_ref().and_then(|m| m.cheats.as_ref()).is_some_and(|c| c.downloading());
        if cheats_busy {
            if let Some(cheats) = self.menu.as_mut().and_then(|m| m.cheats.as_mut()) { cheats.poll_download(); }
            self.request_redraw();
        }
        let mut wake = self.held.map(|(_, next)| next).or(self.quit.as_ref().and_then(Escalation::next));
        if cheats_busy {
            let next = now + Duration::from_millis(200);
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
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
            // A dArkOS return to ES uses the same save/quit worker and keeps its requested exit code 77.
            if self.exit_code != pocket_ui::rescue::OPEN_ES_CODE { self.exit_code = 0; }
            el.exit();
            return;
        }
        // Awake again: a frame, so the corner's clock and battery are those of now (the minute's wake-up runs on the
        // monotonic clock, which stood still in the suspend). The screen is on already; this is after it.
        if self.sleep.take_woke() {
            self.request_redraw();
            // Pocket-to-Pocket sync: what the other Pockets saved while this one slept comes now.
            self.pocket_sync_woke();
        }
        // The Brick back from its real sleep: the game waits behind its pause menu (`Sleep::take_menu_after_wake`).
        #[cfg(feature = "brick")]
        if self.sleep.take_menu_after_wake() && self.menu.is_none() {
            println!("sleep: awake on the Brick; the pause menu");
            self.open_menu();
        }
        // Keep sound responsive while browsing (pauses to read included), then release the silent stream so PipeWire can suspend.
        let sound_until = self.last_input + pocket_ui::ui_sound::WARM_LEASE;
        let sound_warm = self.running.is_none() && self.sleep.state() == pocket_ui::sleep::State::Awake
            && self.sound.on && self.sound.volume > 0 && pocket_ui::ui_sound::warm_lease_holds(now.saturating_duration_since(self.last_input));
        pocket_ui::ui_sound::keep_warm(sound_warm);
        if sound_warm { wake = Some(wake.map_or(sound_until, |w| w.min(sound_until))); }
        // The fan outside games follows games, sleep and the Settings page.
        self.perf_fan_sync();
        // The night light looks at the clock again after a sleep (`app_night_light`).
        self.night_light_tick();
        // The web companion: starts once the Pocket is up; what it reads follows games and Curator (`app_web.rs`).
        self.web_sync();
        // Pocket-to-Pocket sync: its service follows the switch and knows when a game runs (`app_pocket_sync.rs`).
        self.pocket_sync_tick();
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
        #[cfg(feature = "brick")]
        if let Some(t) = self.brick_leds_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // An update file found on the card at the start, said once on the free shelf (`app_card_update`).
        self.card_update_tick();
        // A newer pocket-ui, said once on the free shelf (`app_update_hint`).
        if let Some(t) = self.update_hint_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // Controller setup waiting for a button: it gives up by itself (`app_controllers`).
        if let Some(t) = self.controller_capture_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // The Battery page follows the minute samples while it is open.
        let battery_page = matches!(self.settings_page, Some(SettingsPage::Battery(_))).then(|| self.frame_at + Duration::from_secs(30));
        for t in [asleep, battery_page.map(|at| at.max(now + FRAME))].into_iter().flatten() {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if battery_page.is_some_and(|at| now >= at) {
            self.request_redraw();
        }
        // The pairing code's countdown on Settings > Web companion: a frame a second while it shows.
        if let Some(t) = self.web_wake(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if let Some(t) = self.pocket_sync_wake(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if let Some(t) = self.appearance_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // The status corner's clock: one wake-up as the minute turns, only while it is on screen (`app_status_corner`).
        if let Some(t) = self.status_corner_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // Light by time of day and the decorations on the Shelf and the Cabinet: a look once a minute (`app_room_extras`).
        if let Some(t) = self.room_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // Key repeat runs without new input events. The shelf's idle work must wait for release even when
        // the idle delays have expired, so rig, box and spine preparation cannot run between animation frames.
        if self.held.is_none() {
            // The selected game's box front and cartridge, built while the player rests on it (`app_card::box_warm_tick`).
            if let Some(t) = self.box_warm_tick(now) {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
            // The focused game's console rig and cartridge/disc, pre-warmed while resting on a case (`app_cabinet::insert_warm_tick`).
            if let Some(t) = self.insert_warm_tick(now) {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
            // The library's game facts, looked up ahead in idle moments: the first card's "More by" needs all of them (`app_card::infos_warm_tick`).
            if let Some(t) = self.infos_warm_tick(now) {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
            // The Cabinet: the shelves next to the selected one, made ahead after a move (`app_cabinet::cabinet_warm_tick`).
            if let Some(t) = self.cabinet_warm_tick(now) {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
            // Lived-in shelf: the row's leaning and lying spines, made ahead after the row opened (`app_cabinet::lived_warm_tick`).
            if let Some(t) = self.lived_warm_tick(now) {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
        }
        if let Some(t) = self.sys.next_wake() {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // No reading ahead while a game runs or the device sleeps (`app_cover_prefetch`).
        if self.running.is_some() || self.sleep.state() != pocket_ui::sleep::State::Awake {
            self.cover_prefetch_stop();
        }
        // Split's preview changes picture now and then (`app_pictures`).
        if let Some(t) = self.pictures_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        if let Some(t) = self.loading_wake(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // The spinner while a big emulator loads a frozen game's moment (`app_frozen`).
        if let Some(t) = self.frozen_wait_wake(now) {
            if t.saturating_duration_since(now) <= FRAME {
                self.request_redraw();
            }
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        // The open box's disc turning (Playful animations, `game_box::BoxView::spin_wake`).
        if let Some(t) = self.card.as_ref().and_then(|o| o.card.boxed.as_ref()).and_then(|b| b.spin_wake(now)) {
            if now >= t {
                self.request_redraw();
            }
            let next = if now >= t { now + FRAME } else { t };
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
        // Static toast text sleeps between its entrance and its fade-out; the final frame removes it.
        let toast_wake = self.toast.as_ref().map(|(message, at)| {
            let end = *at + toast_len(message);
            if now >= end { return now; }
            let fade = pocket_ui::motion::span(pocket_ui::motion::Kind::Fade, pocket_ui::motion::PAGE);
            if pocket_ui::motion::appearing(*at, now) || now >= end - fade {
                self.frame_due
            } else {
                end - fade
            }
        });
        if self.toast.as_ref().is_some_and(|(message, at)| now >= *at + toast_len(message)) {
            self.toast = None;
        }
        let marquee_wake = (self.sleep.state() == pocket_ui::sleep::State::Awake
            && (self.running.is_none() || self.switcher.is_some()))
            .then(|| self.marquee.next_wake()).flatten();
        for at in [toast_wake, marquee_wake, self.sync_note_wake(now)].into_iter().flatten() {
            if now >= at { self.request_redraw(); }
            let next = at.max(now + FRAME);
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
        // While something moves, and once after the last frame that showed it moving: else a slow screen stays on an
        // in-between frame (the box half turned on the RK3566, `frame_pace::Settle`).
        if self.settle.wants(self.animating()) {
            // One frame per refresh: redrawing as soon as the last frame is out ran animations at thousands
            // of frames a second, which a scrolling title would keep up for as long as it is selected.
            let due = self.frame_due;
            if now >= due {
                self.request_redraw();
            }
            let next = if now >= due { now + FRAME } else { due };
            wake = Some(wake.map_or(next, |w| w.min(next)));
        }
        // The Cabinet's TV playing the selected game (prototype, `app_cabinet_tv::showcase_tick`).
        if let Some(t) = self.showcase_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
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
        // The status file, when what it says changed (`app_where`); again when the quiet after waking ends.
        if let Some(t) = self.status_tick(now) {
            wake = Some(wake.map_or(t, |w| w.min(t)));
        }
        el.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

impl App {
    /// What sleep needs to know about the app right now.
    fn sleep_ctx(&self) -> pocket_ui::sleep::Ctx {
        pocket_ui::sleep::Ctx {
            menu_open: self.menu.is_some(),
            busy: self.pending_launch.is_some() || self.quit.as_ref().is_some_and(|q| q.busy(Instant::now()))
                || (self.updates.is_busy() && update::busy_blocks_sleep(self.updates.busy_since(), Instant::now()))
                // A store install or tool (review N5), with the same cap.
                // A starter pack download keeps the Pocket awake the same way (`app_starter`).
                || update::busy_blocks_sleep(self.starter.busy_since(), Instant::now())
                // A backup or restore of saves copies for as long as the card needs: it ends itself, and says so (`app_backup`).
                || self.backup_working()
                // "Send from your phone" is open (Add games, Y): no idle sleep while it listens or a file arrives; the page
                // ends itself after `IDLE` / `MAX_SESSION`, the power button closes it.
                || self.setup.as_ref().is_some_and(|s| s.phone_open()),
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

    /// [`play_sound`](Self::play_sound) `after` from now, `db` quieter or louder (`ui_sound::play_after`).
    fn play_sound_after(&self, sound: pocket_ui::ui_sound::Sound, after: Duration, db: f32) -> bool {
        let ctx = pocket_ui::ui_sound::Ctx {
            config: self.sound,
            in_game: self.running.is_some(),
            pause_menu: self.menu.is_some(),
            asleep: self.sleep.state() != pocket_ui::sleep::State::Awake,
        };
        pocket_ui::ui_sound::play_after(sound, &ctx, after, db)
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

    fn paint_headless(&mut self, _now: Instant) -> (Vec<u32>, u32, u32) {
        let (w, h) = snapshot_dims();
        let mut buf = vec![0; (w * h) as usize];
        // Match live frames: a saved Light appearance and changes made in Settings must reach the
        // capture too. Calling paint directly left all headless journeys in the default dark palette.
        self.paint_in_appearance(&mut Canvas::new(&mut buf, w, h), &mut |app, c| {
            app.paint(c);
            app.sys.paint(c, &mut app.text, app.menu.is_none());
            app.paint_leave(c);
        });
        (buf, w, h)
    }

    fn request_redraw(&self) {
        app_cabinet_tv::FULL_REDRAW.store(true, std::sync::atomic::Ordering::Relaxed);
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
        // Start the update search before scheduling shelf reads, so they can wait for the card.
        self.card_update_start();
        // The game list read again (it came from the cache), or the cache written (`app_library`).
        self.library_after_first_frame();
        // The pages' state the first frame doesn't show, each on a thread (`startup::Later`).
        self.sync.start();
        // The look for saves with two versions, on a thread that waits a while first (`app_sync_conflicts`).
        self.sync_conflicts_start();
        // Library data merged with the other devices, on a thread that waits a while first (`app_library_sync`).
        self.library_sync_start();
        self.tailscale.start();
        self.romm.start();
        self.romm_after_start();
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
        // A developer device (`dev-ssh`) whose sshd is off, e.g. after a hard power-off: started (`remote::keep_dev_ssh`).
        pocket_ui::startup::after_first_frame("dev-ssh", || {
            if let Some(result) = pocket_ui::remote::keep_dev_ssh() {
                println!("dev-ssh: sshd was not running; started: {result:?}");
            }
        });
        // The core of the game in front read into the page cache, on a thread of the lowest priority
        // (`pocket_ui::core_prewarm`): the next A finds it there instead of on a cold card.
        self.core_prewarm_start();
        // The threads and programs the start put off (`startup::after_first_frame`).
        pocket_ui::startup::first_frame_done();
    }

    /// The game in front (Home's first recent game, the one Continue starts) and the core its launch would load,
    /// resolved on the warm-up's own thread the way `queue_launch` resolves it (`route`: the same `inputs_for` and
    /// `resolve_with`, then `store::core_path` as `spawn_game`): the route may stat files and wait for the GBA moments'
    /// scan, which the UI thread must not. Only after a shelf frame: a start whose game came before any frame skips it.
    fn core_prewarm_start(&self) {
        // In memory only; whether the file is there is asked on the thread.
        let candidates: Vec<(System, PathBuf, bool)> = self.recents.list.iter()
            .filter(|r| shelf::recent_shown(&r.system, &self.hidden) && !self.library.is_hidden(&r.path))
            .filter_map(|r| {
                let sys = self.systems.iter().find(|s| s.name == r.system)?;
                // Without its game list: only the route's fields are needed.
                let sys = System { name: sys.name.clone(), fullname: sys.fullname.clone(), emulator: sys.emulator.clone(),
                    core: sys.core.clone(), emulators: sys.emulators.clone(), games: Vec::new() };
                Some((sys, r.path.clone(), self.fallback.contains(&r.path)))
            })
            .take(8)
            .collect();
        pocket_ui::core_prewarm::set_quiet(self.core_prewarm_quiet());
        pocket_ui::core_prewarm::start(self.drew_first, move || core_prewarm_front(&candidates));
    }

    /// A game starts or runs, or the device isn't awake: the warm-up doesn't begin, or stops at its next piece.
    fn core_prewarm_quiet(&self) -> bool {
        self.running.is_some() || self.pending_launch.is_some() || self.sleep.state() != pocket_ui::sleep::State::Awake
    }
}

/// The first of `candidates` (system, game, failed start before) whose file is there, as its launch would load it.
fn core_prewarm_front(candidates: &[(System, PathBuf, bool)]) -> Option<pocket_ui::core_prewarm::Front> {
    let (sys, path, fell_back) = candidates.iter().find(|(_, p, _)| p.exists())?;
    let host = std::path::Path::new(host_bin()).exists();
    let inputs = app_emu_choice::inputs_for(sys, Some(path), host, *fell_back);
    let r = app_emu_choice::resolve_with(sys, Some(path), inputs).runner;
    Some(pocket_ui::core_prewarm::Front { core_path: pocket_ui::store::core_path(&r.core), emulator: r.emulator, core: r.core })
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

/// `POCKETUI_FONT=barlow`, `pixel` or `retro`: the font style whatever settings.json says, for the snapshots and tests
/// (as `POCKETUI_TEXT_SIZE` the text size).
fn font_override() -> Option<pocket_ui::font_style::FontStyle> {
    std::env::var("POCKETUI_FONT").ok().and_then(|v| pocket_ui::font_style::FontStyle::parse(&v))
}

fn main() {
    #[cfg(all(target_os = "linux", feature = "brick"))]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--brick-idle")) {
        std::process::exit(pocket_ui::update::payload::idle_command(&std::env::args_os().skip(2).collect::<Vec<_>>()));
    }
    // Stock firmware has no unzip applet. This bounded helper runs before UI/input/audio starts.
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--unpack-runtime")) {
        let args: Vec<_> = std::env::args_os().skip(2).collect();
        let status = std::env::var_os("BRICK_PORTMASTER_PROGRESS").filter(|s| !s.is_empty()).map(std::path::PathBuf::from);
        let result = if args.len() == 2 { pocket_ui::archive::unpack_python_runtime_with_progress(std::path::Path::new(&args[0]), std::path::Path::new(&args[1]), &|done, total| {
            if let Some(path) = &status { pocket_ui::update::payload::write_progress(path, "unpacking", done, total); }
        }).map_err(|e| e.to_string()) }
            else { Err("usage: --unpack-runtime ZIP DEST".to_string()) };
        std::process::exit(match result { Ok(()) => 0, Err(error) => { eprintln!("runtime unpack: {error}"); 1 } });
    }
    // Optional card assets use the updater's verified, resumable transport before UI/input/audio starts.
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--download-asset")) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(pocket_ui::update::payload::cli(&args));
    }
    #[cfg(target_os = "linux")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--controller-bridge")) {
        let args: Vec<_> = std::env::args_os().skip(2).collect();
        let data = PathBuf::from(std::env::var_os("POCKETUI_DATA").unwrap_or_else(|| "/storage/.config/pocket-ui".into()));
        let code = args.first().ok_or_else(|| std::io::Error::other("missing emulator command"))
            .and_then(|program| pocket_ui::pad::bridge::run(program, &args[1..], &data));
        std::process::exit(code.unwrap_or_else(|e| { eprintln!("controller bridge: {e}"); 1 }));
    }
    use pocket_ui::startup::Later;
    if std::env::args().any(|a| a == "--version") {
        println!("pocket-ui {}", update::app_version());
        return;
    }
    // The Brick starter seeds changed engine files before starting any UI/input/audio threads.
    #[cfg(feature = "brick")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--brick-seed-data")) {
        let args: Vec<_> = std::env::args_os().skip(2).collect();
        let result = if args.len() == 2 {
            update::seed_brick_data(std::path::Path::new(&args[0]), std::path::Path::new(&args[1]))
        } else if args.len() == 3 {
            match args[2].to_str() {
                Some(release) => update::seed_brick_data_for_release(std::path::Path::new(&args[0]), std::path::Path::new(&args[1]), release),
                None => Err(std::io::Error::other("invalid engine-data release")),
            }
        } else {
            Err(std::io::Error::other("--brick-seed-data needs source, destination and optional release"))
        };
        if let Err(e) = result {
            eprintln!("brick data: {e}");
            std::process::exit(1);
        }
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
        // Settle the device and its Wi-Fi adapter before starting any workers.
        let _ = pocket_ui::device::current();
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
    #[cfg(target_os = "linux")]
    if !std::env::args().any(|a| a == "--snapshot") {
        if let Err(e) = pocket_ui::hakux::seed_running_release(std::path::Path::new(update::APP_ROOT)) {
            eprintln!("Xbox core: {e}");
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
            // The folder of this pocket-ui (releases/<version>/ through the links): its pocket-core is checked before the update is kept.
            let release = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
            use update::observation::{record_health, HealthObservation};
            record_health(HealthObservation::Watching);
            let presented = || {
                let ready = update::frame_presented();
                if ready { record_health(HealthObservation::Confirming); }
                ready
            };
            let health = update::watch_health(root, &release, started, &presented, Duration::from_millis(200), update::WATCHDOG_AFTER);
            record_health(match health {
                update::Health::NotPending => HealthObservation::NotPending,
                update::Health::Confirmed => HealthObservation::Confirmed,
                update::Health::TimedOut => HealthObservation::TimedOut,
                update::Health::Broken => HealthObservation::Broken,
            });
            match health {
                update::Health::TimedOut => {
                    eprintln!("update: no frame within {:?}, exiting so that the previous version comes back", update::WATCHDOG_AFTER);
                    std::process::exit(update::WATCHDOG_EXIT);
                }
                update::Health::Broken => {
                    eprintln!("update: this version's player doesn't start, exiting so that the previous version comes back");
                    std::process::exit(update::WATCHDOG_EXIT);
                }
                update::Health::NotPending | update::Health::Confirmed => {}
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
    // Instant start (docs/dev/start-time.md, Start time): the type and the game list are read on threads of their own
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
            let mut played = loaded.as_ref().map(|s| pocket_ui::play_evidence::scan(s.systems())).unwrap_or_default();
            if pocket_ui::device::android_profile() { played.extend(pocket_ui::android_handheld::mobile_played(&data_dir)); }
            (loaded, played)
        })
    };
    // The emulator choices in system.cfg (`emu_choice`), on a thread: the first launch finds them in memory.
    pocket_ui::emu_choice::preload();
    // A test's or a snapshot's phone (`POCKETUI_PHONE`, `app_runs_on_android`); never set in a normal run.
    app_runs_on_android::init_for_tests(&data_dir);
    // The shelf asks whether Hardcore keeps a game from continuing (`achievements::hardcore::skips_moments`).
    pocket_ui::achievements::hardcore::shelf_data(&data_dir);
    pocket_ui::rumble::load_menu_haptics(&data_dir);
    // The dArkOS port shows Haptics only where something can vibrate (`darkos::no_motor`).
    #[cfg(feature = "brick")]
    if pocket_ui::darkos::host() {
        pocket_ui::rumble::look_for_motor();
    }
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
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    // Android's handheld view: the loop of its Activity (`app_android`).
    #[cfg(all(target_os = "android", feature = "handheld"))]
    app_android::attach(&mut builder);
    let event_loop = builder.build().expect("event loop");
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
    let retired_look = settings.retired_look;
    let settings_damaged = settings.damaged;
    pocket_ui::motion::set_level(settings.motion);
    pocket_ui::cozy_extras::set(settings.extras);
    pocket_ui::system_art::set_gba_model(settings.gba_model);
    pocket_ui::system_art::set_colours_chosen(&settings.console_colours);
    pocket_ui::motion::set_cozy(settings.animations_or_default() == pocket_ui::settings::Animations::Cozy);
    // The first start since the device started fades in from black after the boot screen (`boot_fade`); the mark is
    // left even with Motion Off, so that turning it on later doesn't fade a restart.
    let boot_fade = !std::env::args().any(|a| a == "--snapshot")
        && pocket_ui::boot_fade::first_since_boot(std::path::Path::new(pocket_ui::boot_fade::MARK))
        && pocket_ui::motion::allowed(pocket_ui::motion::Kind::Fade);
    // Which face button confirms (`button_layout`).
    pocket_ui::button_layout::set(settings.button_layout);
    // Settings → Look → Colours, Font: the hints' Button symbols (`POCKETUI_PROMPTS` for the snapshots).
    pocket_ui::button_prompts::set(pocket_ui::button_prompts::PromptStyle::from_env().unwrap_or(settings.prompts));
    // And its Colours, before the first frame (`POCKETUI_COLOURS` for the snapshots).
    pocket_ui::palette::set_colours(pocket_ui::palette::Colours::from_env().unwrap_or(settings.colours));
    // Settings → Look → Style (`font_style`): every `Text` follows it by itself, also after a change on the page.
    pocket_ui::font_style::set(font_override().unwrap_or(settings.font));
    // Settings → Look → Style → Retro's palette (`retro`); `POCKETUI_RETRO` turns Retro on or off for the snapshots.
    pocket_ui::retro::set_colours(settings.retro_colours);
    // Its letters: Departure Mono unless Pixelify is chosen (`POCKETUI_RETRO_LETTERS` for the snapshots).
    pocket_ui::font_style::set_retro_letters(pocket_ui::font_style::RetroLetters::from_env().unwrap_or(settings.retro_letters));
    match pocket_ui::retro::from_env() {
        Some(Some(c)) => {
            pocket_ui::font_style::set(pocket_ui::font_style::FontStyle::Retro);
            pocket_ui::retro::set_colours(c);
        }
        Some(None) if pocket_ui::retro::on() => pocket_ui::font_style::set(pocket_ui::font_style::FontStyle::Barlow),
        _ => {}
    }
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
    // Settings → Look → Text size (`type_scale`): before anything is painted, on the thread that paints.
    // `POCKETUI_TEXT_SIZE=large` overrides it, for the snapshot checks (as `POCKETUI_LANG` the language).
    pocket_ui::type_scale::set(text_size_override().unwrap_or(settings.text_size));
    let hw = std::sync::Arc::new(pocket_ui::system_ctl::AutoHw::detect());
    println!("system controls: mocked {:?}", hw.mocked());
    #[cfg(target_os = "linux")]
    let h700_volume = pocket_ui::h700_volume::Keeper::start();
    {
        // `pactl`: a program read from the card; after the first frame (`startup::after_first_frame`).
        let hw = hw.clone();
        pocket_ui::startup::after_first_frame("seed-volume", move || hw.seed_volume());
    }
    pocket_ui::startup::mark("settings+hw");
    #[cfg(target_os = "linux")]
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
        // The Brick's menus: its CPU speeds up within a few frames of a press, not 2 s later (`power::quick_ondemand`).
        pocket_ui::power::ui_took_over();
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
    let romm_ui = {
        let data_dir = data_dir.clone();
        Later::new("romm", move || app_romm::RommUi::load(&data_dir))
    };
    let web_ui = {
        let (data_dir, es_systems) = (data_dir.clone(), PathBuf::from(&es_systems));
        Later::new("web", move || app_web::WebUi::load(&data_dir, &es_systems))
    };
    // The web companion's Stats page reads the play history through this (a function pointer stored once; nothing runs now).
    pocket_ui::web::set_history_source(pocket_ui::play_history::stats_json);
    let pocket_sync_ui = app_pocket_sync::PocketSyncUi::load(&data_dir);
    let perf_ui = app_perf_fan::PerfFanUi::load(&data_dir);
    // The ports' own performance and fan, read on a thread (`app_port_perf`).
    let port_perf_ui = app_port_perf::PortPerfUi::new(&data_dir);
    // The catalogue's versions, for the update note on a port's card (read from the cached ports.json, on a thread).
    pocket_ui::ports::load_versions(&data_dir);
    {
        let proxy = proxy.clone();
        pocket_ui::ports::start_index(move || { let _ = proxy.send_event(UserEvent::PortIndex); });
    }
    let mut curator_ui = app_curator::CuratorUi::load(&data_dir);
    // Curator's "Pixel Path over every game" became Settings → Playing → Over the game (2026-10-25): moved once.
    pocket_ui::over_game::adopt_curator(&data_dir, &mut settings, &mut curator_ui.prefs);
    // Over the game's Pixel Path level only with the pause menu's Pixel Path (Enthusiast); a saved one is FPS otherwise.
    settings.over_game = settings.over_game.offered(curator_ui.prefs.pixel_path);
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
        let notify: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            let _ = proxy.send_event(UserEvent::SleepNews);
        });
        // A snapshot or bench never touches the real screen, sleep or power (`Sleep::offline`).
        if std::env::args().any(|a| a == "--snapshot") {
            pocket_ui::sleep::Sleep::offline(data_dir.clone(), notify)
        } else {
            pocket_ui::sleep::Sleep::new(data_dir.clone(), notify)
        }
    };
    pocket_ui::startup::mark("sleep");
    let text = fonts.join().unwrap_or_default();
    pocket_ui::startup::mark("fonts");
    let (systems, load_error, library_ui, played_files) = match library_load.join() {
        Ok((Ok(start), played)) => {
            let ui = app_library::LibraryUi::from_start(&start, pocket_ui::library_cache::enabled());
            let systems = match start {
                pocket_ui::library_cache::Start::Cached { systems, .. } | pocket_ui::library_cache::Start::Scanned { systems, .. } => systems,
            };
            (systems, None, ui, played)
        }
        Ok((Err(e), _)) => (Vec::new(), Some(format!("Cannot read the game list: {e}")), Default::default(), Default::default()),
        Err(_) => (Vec::new(), Some("Cannot read the game list.".to_string()), Default::default(), Default::default()),
    };
    pocket_ui::startup::mark("library");
    let gamedb = pocket_ui::gamedb::GameDb::embedded();
    pocket_ui::startup::mark("gamedb");
    let mut recents = Recents::load(&data_dir);
    // Continue, the recent games and the switcher name the games the way the system list does (from the cached list: no disk).
    recents.follow_library(&systems);
    pocket_ui::startup::mark("recents");
    // The Brick's hardware rows read their file before `data_dir` moves into the app.
    #[cfg(feature = "brick")]
    let brick_state = app_brick_hw::State::load(&data_dir);
    let shelf_space = pocket_ui::shelf_space::ShelfSpace::load(&data_dir);
    let update_hint = app_update_hint::UpdateHintUi::new(pocket_ui::update_hint::State::load(&app_update_hint::state_dir(&data_dir)), Instant::now());
    // Inspection stays on its own worker. Mock/snapshot fixtures have no installed layout to inspect.
    let update_inspector = if std::env::var_os("POCKETUI_MOCK").is_some() || std::env::args().any(|a| a == "--snapshot") { None } else {
        let notify = proxy.clone();
        Some(update::observation::watch(move |inspection| {
            let _ = notify.send_event(UserEvent::Update(UpdateEvent::Inspected(inspection)));
        }))
    };
    let mock_update_manifest = app_update_hint::mock_manifest();
    let mock_release_build = (mock_update_manifest.is_some() || app_card_update::mocked().is_some()).then_some(true);
    // With the mock notice on, the fixture stands in for a just-completed automatic look.
    let mock_checked_at = (mock_update_manifest.is_some() && !update_hint.state.off).then(std::time::SystemTime::now);
    let mut app = App {
        proxy,
        started,
        window: None,
        surface: pocket_ui::frame::Presenter::empty(),
        text,
        systems,
        load_error,
        system_list_fallback: false,
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
        retroarch_state_pending: None,
        launch_at: None,
        pending_launch: None,
        java_fetch: None,
        drew_first: false,
        boot_fade: pocket_ui::boot_fade::BootFade::new(boot_fade),
        after_first_done: false,
        theme: settings.theme,
        appearance: settings.appearance,
        fader: pocket_ui::appearance::Fader::at(pocket_ui::appearance::wants_light_now(settings.appearance)),
        appearance_checked: Instant::now(),
        appearance_pending: false,
        last_input: Instant::now(),
        box_warm: None,
        look_previews_light: false,
        look_previews_theme: 0,
        look_previews_lang: pocket_ui::i18n::current(),
        look_previews_size: pocket_ui::type_scale::get(),
        look_previews_clock: None,
        column_buf: Vec::new(),
        language: settings.language,
        hidden: settings.hidden_systems.clone(),
        data_dir,
        toast: None,
        toast_over_hints: false,
        card_refusals: Default::default(),
        sleep_page: pocket_ui::sleep_settings::Page::new(pocket_ui::sleep_settings::Config::default()),
        about: Default::default(),
        after_game_note: None,
        tv: None,
        notice: crash_notice,
        notice_painted: false,
        notice_later: Vec::new(),
        fallback,
        select_alone: false,
        start_tap: StartTap::default(),
        switcher: None,
        switch_to: None,
        fast: Default::default(),
        menu_shot: Default::default(),
        raise: Default::default(),
        frame_at: Instant::now(),
        frame_due: Instant::now(),
        settle: Default::default(),
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
        // A mock run's stand-in list (`POCKETUI_UPDATE_HINT`) also has its pocket-ui on the Updates page, with notes.
        updates: Updates { release_build: mock_release_build, manifest: mock_update_manifest, manifest_checked_at: mock_checked_at, ..Updates::default() },
        update_url: settings.update_url.clone(),
        store_url: settings.store_url.clone(),
        starter: Default::default(),
        exit_code: 0,
        onboarded: settings.onboarded,
        es_systems: PathBuf::from(&es_systems),
        covers: std::collections::HashMap::new(),
        sealed_covers: Vec::new(),
        art: std::collections::HashMap::new(),
        home_tap: false,
        kids: Default::default(),
        big: app_big_shelf::Big::new(settings.open_case_told),
        cover_run: Default::default(),
        card_out_note: false,
        text_run: Default::default(),
        identify: app_identify::Identify::new(),
        look_previews: None,
        sample_cover: None,
        zoom_level: app_zoom::FIRST_LEVEL,
        power_paused_game: false,
        sleep,
        sys,
        hits: Default::default(),
        leave: None,
        dolphin_launch: None,
        cover_pick: None,
        touch_rec: Default::default(),
        cursor: None,
        mouse_down: false,
        wheel_carry: 0.0,
        ignore_log: Default::default(),
        sound: sound_config,
        leds: leds_config,
        #[cfg(feature = "brick")]
        brick: brick_state,
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
        infos_warm: (0, 0),
        infos_resolved: 0,
        volume_step: false,
        sync: sync_ui,
        tailscale: tailscale_ui,
        romm: romm_ui,
        web: web_ui,
        pocket_sync: pocket_sync_ui,
        perf: perf_ui,
        tools: Default::default(),
        test_buttons: Default::default(),
        game_check: Default::default(),
        moonlight: Default::default(),
        backup: Default::default(),
        storage: Default::default(),
        gamelist: Default::default(),
        android_games: Default::default(),
        gallery: Default::default(),
        clock_look: Some(Instant::now() + app_clock::FIRST_LOOK),
        update_hint,
        update_inspection: Default::default(),
        update_inspector,
        emu: Default::default(),
        this_game: None,
        controller_setup: Default::default(),
        achievements: achievements_ui,
        bluetooth: bluetooth_ui,
        streaming: pocket_ui::moonlight::installed(),
        streaming_on: settings.streaming_on_home,
        pictures: app_pictures::PicturesUi::new(!settings.box_art_only && !settings.art_off),
        prefetch: Default::default(),
        art_off: settings.art_off,
        cabinet: Default::default(),
        shelf_space,
        crate_ui: Default::default(),
        // Read before `curator_ui` moves in: the pause menu's Pixel Path switch (`Prefs::pixel_path`).
        pixel_path: curator_ui.prefs.pixel_path,
        curator: curator_ui,
        on_start: settings.on_start,
        remember_quit: !settings.forget_where_quit,
        remind_where: settings.where_was_i,
        zoom_in_pause: settings.zoom_in_pause,
        lived_in: settings.lived_in,
        boot_screen: settings.boot_screen,
        moment_photos: settings.moment_photos,
        moment_in_play: settings.moment_in_play,
        album_since: None,
        where_was_i: None,
        case_knows: Default::default(),
        animations: settings.animations_chosen(),
        game_box: !settings.details_page,
        button_layout: pocket_ui::button_layout::effective(settings.button_layout),
        launch_picture: None,
        frozen: Vec::new(),
        moment_reads: Default::default(),
        played_files,
        played_scan: 0,
        frozen_auto: None,
        frozen_wait: None,
        loading: Default::default(),
        find: Default::default(),
        motion: Default::default(),
        frames: settings.frames,
        over_game: settings.over_game,
        tour: app_tour::TourUi::new(settings.tour_later.clone(), settings.seen_version.clone()),
        corner_minute: None,
        painted: app_where::Painter::Home,
        status: app_where::status_writer(),
        status_failed: false,
        press_pages: Vec::new(),
        feedback: Vec::new(),
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
        // Back at the step the last start reached (a restart or a crash in the first-start setup).
        app.setup = Some(app.new_setup().resume());
    }
    app.rebuild();
    pocket_ui::startup::mark("rebuild");
    // Home opens on Continue, under the Search row.
    app.sel = Item::first_row(&app.items);
    app.prev_sel = app.sel;
    app.kids_load();
    // Easy opens on the game played last.
    app.big_open_on_last();
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
        app.library_system_list_check();
        // The look for an update file on the card, only where a journey names one (`app_card_update`).
        app.card_update_start();
        if settings_damaged {
            app.warn(pocket_ui::settings::DAMAGED_NOTE);
        }
        // As at a real start (below): a look that went with 0.4 is said once.
        if retired_look {
            app.say_retired_look();
        }
        if pocket_ui::frame_dump::on() {
            app_frame_dump::run(event_loop, &mut app);
        } else {
            event_loop.run_app(&mut app).expect("run test driver");
        }
        pocket_ui::fsutil::settle();
        return;
    }
    if let Some(i) = std::env::args().position(|a| a == "--snapshot") {
        let args: Vec<String> = std::env::args().skip(i + 1).collect();
        app_status_corner::snapshot_knobs();
        snapshot(&mut app, &args);
        // The made-up saves must not pile up in /tmp: say so when they stay behind (a state that made none is fine).
        match std::fs::remove_dir_all(snapshot_tmp()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => eprintln!("snapshot: could not remove {}: {e}", snapshot_tmp().display()),
            _ => {}
        }
        // tests/where_states.rs: which painter painted the snapshot and where the App says it is (`app_where`).
        if let Some(record) = std::env::var_os("POCKETUI_WHERE_RECORD") {
            let _ = std::fs::write(record, format!("{}\t{}\n", app.painted.name(), app.where_now()));
        }
        // Snapshot actions can enqueue sticker edits too: keep their files complete before this process exits.
        pocket_ui::fsutil::settle();
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
        // Neither the settings nor their backup could be read: said, not a silent reset.
        if settings_damaged {
            app.warn(pocket_ui::settings::DAMAGED_NOTE);
        }
        // A look that went with 0.4 is Split now (`settings::RETIRED_LOOKS`): said once, and saved at once.
        if retired_look {
            app.say_retired_look();
        }
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
            app.warn(note.clone());
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
            match pocket_ui::mako_osd::start() {
                Ok(true) => println!("mako: hiding ROCKNIX's volume box while pocket-ui is in front"),
                Ok(false) => {}
                Err(e) => eprintln!("mako: {e}"),
            }
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
        // File sharing left on comes back after a restart, once Wi-Fi is up (`wifi::keep_sharing_on`).
        pocket_ui::startup::after_first_frame("keep-sharing", pocket_ui::wifi::keep_sharing_on);
        // A games card coming or going while we run: the shelf reads the folders again, or says the card is out.
        #[cfg(target_os = "linux")]
        {
            let proxy = app.proxy.clone();
            pocket_ui::startup::after_first_frame("games-card", move || {
                pocket_ui::games_card::watch(move |change| {
                    let _ = proxy.send_event(UserEvent::GamesCard(change));
                })
            });
        }
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
    // A frame dump runs the same loop tick by tick on its fixed clock (`app_frame_dump`); without it, as always.
    if pocket_ui::frame_dump::on() {
        app_frame_dump::run(event_loop, &mut app);
    } else {
        event_loop.run_app(&mut app).expect("run app");
    }
    // Stop sampling and flush all buffered history/ledger data before an orderly exit.
    pocket_ui::battery_history::stop();
    // The lists a game's end handed to the card writer are on the card before we go (`fsutil::write_json_soon`).
    pocket_ui::fsutil::settle();
    // And the play log's last frames (at most two seconds).
    pocket_ui::play_log::settle();
    app.perf_fan_exit();
    app.web_exit();
    app.pocket_sync_exit();
    if app.exit_code == pocket_ui::rescue::OPEN_ES_CODE {
        pocket_ui::rescue::before_es();
    }
    // dArkOS starts no logger: leaving on purpose ends pocket-ui's part of the boot, and a power off from EmulationStation
    // later is no unexpected end (`logs::mark_clean`; a start in the same boot takes the mark back).
    #[cfg(feature = "brick")]
    if pocket_ui::darkos::host() && pocket_ui::logs::port_exit_is_clean(app.exit_code) {
        pocket_ui::logs::mark_clean(&app.data_dir, pocket_ui::recents::now_secs());
    }
    #[cfg(target_os = "linux")]
    drop(h700_volume);
    std::process::exit(app.exit_code);
}

#[cfg(test)]
mod start_order_tests {
    /// The player is read once from the route's names, and runemu.sh and the controller bridge still get the same name.
    #[test]
    fn a_game_starts_on_the_player_its_route_names() {
        use super::{standalone::Emu, Player};
        assert_eq!(Player::of("gba", "pocket-core", "mgba"), Player::Hosted);
        assert_eq!(Player::of("xbox", "hakux", "hakux"), Player::Standalone(Emu::Hakux, "hakux".into()));
        assert_eq!(Player::of("ps2", "armsx2", "armsx2"), Player::Standalone(Emu::Armsx2, "armsx2".into()));
        assert_eq!(Player::of("gba", "retroarch", "mgba"), Player::Other("retroarch".into()));
        for (system, emulator, core) in [("gba", "pocket-core", "mgba"), ("3ds", "azahar", "azahar"), ("ports", "ports", ""), ("snes", "retroarch", "snes9x")] {
            assert_eq!(Player::of(system, emulator, core).name(), emulator);
        }
    }

    /// Battery's pointer to Sleep & power is read on every device (Odin, Anbernic): no "the Pocket" in it, and
    /// the German line is the neutral one too.
    #[test]
    fn the_battery_pointer_is_device_neutral_in_both_languages() {
        assert!(!super::SLEEP_SETTINGS_POINTER.contains("Pocket"));
        let de = include_str!("../assets/i18n/de.tsv");
        let line = de.lines().find(|l| l.starts_with(super::SLEEP_SETTINGS_POINTER)).expect("a German line for the pointer");
        assert!(line.contains("das Gerät") && !line.contains("Pocket"), "{line}");
    }

    /// The core warm-up (`core_prewarm`) begins only from `after_first_frame`, with whether a shelf frame was drawn:
    /// read before the first frame, it would wait for the card in the same queue as that frame (TORTOS-SPEED.md #1).
    #[test]
    fn the_core_warm_up_starts_only_after_the_first_frame_and_says_whether_one_was_drawn() {
        let all = include_str!("main.rs");
        // The code, without this test's own words.
        let src = &all[..all.find("fn the_core_warm_up_starts_only_after").unwrap()];
        let calls = src.matches("self.core_prewarm_start();").count();
        assert_eq!(calls, 1, "one call");
        let after = &src[src.find("fn after_first_frame(&mut self)").expect("after_first_frame")..];
        let after = &after[..after.find("\n    }\n").unwrap()];
        assert!(after.contains("self.core_prewarm_start();"), "called from after_first_frame");
        assert_eq!(src.matches("pocket_ui::core_prewarm::start(").count(), 1);
        assert!(src.contains("pocket_ui::core_prewarm::start(self.drew_first,"), "the frame flag is passed, not true");
    }

    #[test]
    fn a_toast_stays_two_seconds_at_the_least_and_longer_with_its_length() {
        assert_eq!(crate::toast_len("Added to Favourites"), std::time::Duration::from_millis(2000));
        assert!(crate::toast_len("Hidden: Game. Settings › System › Storage brings it back.") > std::time::Duration::from_millis(3000));
        assert_eq!(crate::toast_len(&"x".repeat(400)), std::time::Duration::from_millis(6000), "never longer than six seconds");
    }

    #[test]
    fn a_frame_dump_ticks_as_the_ui_paces_its_frames_so_every_tick_gets_one() {
        // POCKETUI_FRAME_DUMP on the Mini V2 (10.11.2026): ticks of exactly 1/60 s (16.6667 ms) came 0.3 µs before
        // `frame_due` (`FRAME`, 16.667 ms), so the window painted only every other tick, 30 fps. The headless driver
        // paints every tick itself, which hid it; this holds the window's rule against the dump's clock.
        let t0 = std::time::Instant::now();
        let mut due = t0;
        for k in 0..600u64 {
            let now = t0 + std::time::Duration::from_nanos(pocket_ui::frame_dump::tick_ns(k));
            assert!(now >= due, "tick {k}: the frame is due {:?} later", due - now);
            due = pocket_ui::frame_pace::after_paint(now, now, crate::FRAME);
        }
    }

    /// The box stayed half turned on the RK3566 (dArkOS test-6, 15.11.2026): the loop asked for frames only while
    /// something moved, so a slow screen's last frame was an in-between one (`game_box` tests simulate it with the
    /// box). Every painted frame tells `frame_pace::Settle` whether it moved, and the loop asks through it.
    #[test]
    fn the_loop_paints_once_more_after_the_last_frame_that_moved() {
        let src = include_str!("main.rs");
        let body = |name: &str| {
            let at = src.find(name).unwrap_or_else(|| panic!("{name}"));
            &src[at..at + src[at..].find("\n    }\n").expect("its end")]
        };
        let wait = body("fn about_to_wait(&mut self, el: &ActiveEventLoop)");
        assert!(wait.contains("self.settle.wants(self.animating())") && !wait.contains("if self.animating() {"), "about_to_wait asks through Settle");
        let draw = body("fn draw(&mut self)");
        assert!(draw.matches("self.settle.painted(moving)").count() == 2, "both kinds of frame (whole and the TV's) tell Settle");
        assert!(draw.find("let moving = self.animating();") < draw.find("surface.draw_scaled"), "moving is read before the frame is painted");
    }

    /// The Brick's `ondemand` comes back from every game with the kernel's 2 s between looks at the load, and the menus'
    /// animations ran at 408 MHz until the next look (15.11.2026, the box turning over in hitches): pocket-ui's start
    /// and every game's end make it quick again (`power::ui_took_over`; its waiting for the governor is tested in
    /// `power`). Apps and failed starts end through `game_exited` too, before it parts ways.
    #[test]
    fn the_menus_make_ondemand_quick_at_the_start_and_after_every_game() {
        let src = include_str!("main.rs");
        let exited = &src[src.find("fn game_exited(").expect("game_exited")..];
        let took = exited.find("pocket_ui::power::ui_took_over();").expect("game_exited makes ondemand quick");
        assert!(took < exited.find("if run.is_app()").expect("the app branch"), "before the first early return after the run is taken");
        let start = &src[src.find("standalone::recover_performance();").expect("the start's CPU recovery")..];
        assert!(start[..start.find("\n    }\n").unwrap()].contains("pocket_ui::power::ui_took_over();"), "the start makes ondemand quick");
    }

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
            | Item::JustPlay
            | Item::Favorites
            | Item::Curator
            | Item::Lens
           
            | Item::SmartShelf(_)
            | Item::Sticker(_)
            | Item::History
            | Item::Review(_)
            | Item::Rediscover
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
mod home_tests {
    use super::{page_lets_home_go, SettingsPage::*};

    #[test]
    fn home_leaves_the_plain_settings_pages_and_never_one_with_something_under_way() {
        for page in [List(3), About, Battery(1), Leds(0), NightLight(0), CozyExtras(2), SleepPower, Curator] {
            assert!(page_lets_home_go(Some(page)), "{page:?}");
        }
        // A download or a check (Updates, the starter pack), a sign-in or pairing, a clean-up or a restore, a button
        // being remapped: HOME keeps out, B goes back as it asks.
        for page in [Updates, Starter, GameCheck, Sync, Tailscale, Bluetooth, Web, PerfFan, Achievements, Tools, PcStreaming, Backup, Storage, Gamelist, Screenshots, Buttons(0), Manual] {
            assert!(!page_lets_home_go(Some(page)), "{page:?}");
        }
        assert!(!page_lets_home_go(None), "not in Settings: HOME is the shelf's (Quick settings)");
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
        assert_eq!(page(Saver::PocketCore { states: true }, true), [Continue, Save, Load, Earlier, Options, Quit]);
        assert_eq!(page(Saver::Standalone, true), [Continue, Save, Load, Options, Quit]);
        assert_eq!(page(Saver::RetroArch, false), [Continue, Save, Load, Options, Quit]);
        // Options for every emulator: without a page of its own, "Runs on" (`app_emu_choice`).
        assert_eq!(page(Saver::Other, false), [Continue, Options, Quit]);
    }

    #[test]
    fn save_and_load_are_left_out_where_they_would_do_nothing() {
        // ScummVM, PICO-8, TIC-80 on pocket-core: Save only said "no save states" over the game, Load was greyed out.
        assert_eq!(page(Saver::PocketCore { states: false }, true), [Continue, Options, Quit]);
        // ARMSX2 before it named the game: Save had no file to write to.
        assert_eq!(page(Saver::Standalone, false), [Continue, Options, Quit]);
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
    fn quit_says_save_and_quit_only_where_it_saves_the_moment() {
        use super::quit_saves;
        assert!(quit_saves(false, Saves::Slots), "pocket-core: the moment is saved");
        assert!(quit_saves(false, Saves::One), "RetroArch writes its state on every exit");
        assert!(!quit_saves(true, Saves::Slots), "Continue where I quit off: plain Quit, with its own line");
        assert!(!quit_saves(false, Saves::InGame("x")), "ScummVM, ports, other emulators: nothing to save");
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
        let all = [Continue, Save, Load, Earlier, Disc, Options, Achievements, PixelPath, Players, Quit];
        for a in all {
            for b in all {
                assert!(a == b || a.label() != b.label(), "{a:?} and {b:?}");
            }
        }
    }
}

#[cfg(test)]
mod test_clock;

#[cfg(test)]
mod system_name_tests {
    use super::*;

    /// "snes, just now" under a game's title (polish review 12.11.): a system missing from the list was named by its folder.
    #[test]
    fn a_system_not_in_the_list_is_named_as_the_player_knows_it() {
        assert_eq!(system_label(&[], "snes"), "Super Nintendo");
        assert_eq!(system_label(&[], "psx"), "PlayStation");
        assert_eq!(system_label(&[], "gba"), "Game Boy Advance");
        for id in pocket_ui::document_library::SYSTEM_IDS {
            assert_ne!(system_label(&[], id), id, "{id}");
        }
        assert_eq!(system_label(&[], "unknownthing"), "unknownthing", "what nothing knows stays as it is");
    }

    #[test]
    fn spine_names_are_short_for_every_system() {
        assert_eq!(system_short(&[], "gbc"), "GBC");
        assert_eq!(system_short(&[], "ngp"), "NGP");
    }
}

// The browser demo: what its page calls (docs/WEB-DEMO.md). At the end, so the device builds keep every line where it was.
#[cfg(target_os = "emscripten")]
#[path = "../web/app_web_demo.rs"]
mod app_web_demo;
#[cfg(target_os = "emscripten")]
use app_web_demo::{spawn_app, spawn_game};

#[cfg(test)]
mod development_update_tests {
    use super::*;

    fn checked(release_build: bool, version: &str) -> Updates {
        let app = update::Release { version: version.into(), url: "unused.tar.gz".into(),
            preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: String::new() };
        Updates { release_build: Some(release_build), manifest: Some(Manifest { format: 1, app: Some(app), ..Default::default() }), ..Default::default() }
    }

    #[test]
    fn development_build_does_not_offer_a_release_over_its_cargo_version() {
        // Main dd5b2e0d contains 47 commits after the 0.3.2 source freeze, but Cargo still says 0.2.0.
        assert!(update::newer("0.3.2", "0.2.0"), "the numeric comparison alone causes the false offer");
        assert!(checked(false, "0.3.2").app_available().is_none(),
            "a dev build has no release ordering: 0.3.2 must not be offered as newer");
        assert!(checked(false, "99.0.0").app_available().is_none(),
            "without a release stamp, even a larger version is not proof of newer code");
    }

    #[test]
    fn normal_runs_take_the_build_type_from_the_compiled_release_stamp() {
        let mut page = checked(false, "99.0.0");
        page.release_build = None;
        assert_eq!(page.app_available().is_some(), update::release_build());
    }

    #[test]
    fn release_build_keeps_the_existing_numeric_update_order() {
        assert!(checked(true, "99.0.0").app_available().is_some());
        assert!(checked(true, update::app_version()).app_available().is_none());
        assert!(checked(true, "0.0.0").app_available().is_none());
    }
}

// Android's handheld view (android/handheld/Cargo.toml): this App as the Activity's native library. At the end, so the
// device builds keep every line where it was.
#[cfg(all(target_os = "android", feature = "handheld"))]
#[path = "../android/handheld/app_android.rs"]
mod app_android;
#[cfg(all(target_os = "android", feature = "handheld"))]
use app_android::{spawn_app, spawn_game};

#[cfg(test)]
mod pause_parity_tests {
    use super::*;

    /// Linux's main-page rows Android's player hasn't, each with why.
    const MAIN_NOT_ON_ANDROID: &[(MainItem, &str)] = &[
        (MainItem::Zoom, "Linux only: a row of its own only with Big text or Settings → Playing → Zoom in the pause menu, settings the player hasn't; Options → Zoom is there"),
        (MainItem::PixelPath, "feature missing on Android: Pixel Path (its own pause-menu page) isn't in the player"),
        (MainItem::Players, "feature missing on Android: the player has no Players page"),
    ];

    fn android_item(i: MainItem) -> Option<pocket_ui::pause_list::Item> {
        use pocket_ui::pause_list::Item;
        Some(match i {
            MainItem::Continue => Item::Continue,
            MainItem::Save => Item::Save,
            MainItem::Load => Item::Load,
            MainItem::Earlier => Item::Earlier,
            MainItem::Disc => Item::Disc,
            MainItem::Options => Item::Options,
            MainItem::Achievements => Item::Achievements,
            MainItem::Quit => Item::Quit,
            MainItem::Zoom | MainItem::PixelPath | MainItem::Players => return None,
        })
    }

    /// The class of main pages, not one: for every game Android's player can have (Earlier with a row or not, one disc or
    /// several, signed in or not) its main page is Linux's for a pocket-core game with slots (`arrange_items`): the same
    /// rows in the same order, with Linux's words, icons and angles, and Linux's hints but for START's and SELECT's, which
    /// lead to what the player hasn't (Android's Home and recent apps are those). Only `MAIN_NOT_ON_ANDROID` differs.
    #[test]
    fn the_main_page_is_linux_s_in_android_s_player() {
        for earlier in [false, true] {
            for disc in [false, true] {
                for achievements in [false, true] {
                    let at = format!("earlier {earlier}, disc {disc}, achievements {achievements}");
                    let linux = arrange_items(main_items(true, pocket_ui::pause_saves::Saves::Slots, achievements), earlier, true, disc, true, true);
                    for (item, why) in MAIN_NOT_ON_ANDROID {
                        assert!(linux.contains(item), "{at}: {item:?} is no Linux row here");
                        assert!(why.starts_with("Linux only") || why.starts_with("feature missing on Android"), "{why}");
                    }
                    let linux: Vec<MainItem> = linux.into_iter().filter(|i| !MAIN_NOT_ON_ANDROID.iter().any(|(o, _)| o == i)).collect();
                    let android = pocket_ui::pause_list::order(earlier, disc, achievements);
                    assert_eq!(linux.iter().map(|i| android_item(*i)).collect::<Vec<_>>(), android.iter().map(|i| Some(*i)).collect::<Vec<_>>(), "{at}");
                    for (l, a) in linux.iter().zip(&android) {
                        assert_eq!((l.shown(true), l.shown(false)), (a.shown(true), a.shown(false)), "{at}");
                        assert_eq!(l.opens_page(), a.opens_page(), "{at}: {l:?}");
                        assert_eq!(retro_icon(*l), a.icon(), "{at}: {l:?}");
                    }
                }
            }
        }
        // The hints: Linux's own list, START's and SELECT's left out.
        for step in [None, Some("Slot"), Some("State"), Some("Disc")] {
            for opens in [false, true] {
                let linux: Vec<_> = pocket_ui::pause_page::main_hints(step, opens, true, true).into_iter().filter(|(k, _)| !matches!(*k, "START" | "SELECT")).collect();
                assert_eq!(pocket_ui::pause_page::main_hints(step, opens, false, false), linux, "{step:?} {opens}");
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/common/data_faults.rs"]
mod file_faults;

#[cfg(test)]
mod meta_words_tests {
    use super::{meta_with_status, meta_with_system};
    use pocket_ui::case_stickers::Kind;
    use pocket_ui::i18n::{tr_in, Lang};
    use pocket_ui::library_meta::Status;

    /// No status or sticker word stays English in a game's meta line, in any language, in either of its shapes (a system's
    /// list: "Completed, last played …"; Favourites and the other mixed shelves: "Nintendo 64, completed, last played …"),
    /// for every time it can say. 18.11.2026: the German line said "Nintendo 64, completed, zuletzt gespielt …". Every
    /// status × every sticker word × every language; the sticker words also on their own.
    #[test]
    fn no_status_or_sticker_word_stays_english_in_the_meta_line() {
        let statuses = [Status::Playing, Status::Paused, Status::Completed, Status::Dropped];
        let words: Vec<&str> = statuses.iter().map(|s| s.label()).chain(Kind::ALL.iter().map(|k| k.label())).collect();
        let lines = [
            "Last played just now, under a minute in total",
            "Last played 5 h ago, 6 h 00 min in total",
            "Last played yesterday, 2 h 00 min in total",
            "Last played 3 days ago, 6 h 00 min in total",
            "Last played 800 days ago, 125 h 00 min in total",
            "Not played yet",
            "Not here yet",
        ];
        let mut wrong = Vec::new();
        for lang in Lang::ALL.into_iter().filter(|l| l.table().is_some() && l.complete()) {
            for word in &words {
                let shown = tr_in(lang, word);
                // Only a word every language writes the same ("100 %") may stay as it is.
                if shown == *word && !word.chars().any(|c| c.is_ascii_digit()) {
                    wrong.push(format!("{lang:?}: the sticker or status word {word:?} has no translation"));
                }
            }
            for status in statuses {
                for line in lines {
                    let own = meta_with_status(status, line);
                    for meta in [own.clone(), meta_with_system("Nintendo 64", &own)] {
                        let shown = tr_in(lang, &meta);
                        let english = words.iter().filter(|w| !w.chars().any(|c| c.is_ascii_digit())).find(|w| {
                            let w = w.to_lowercase();
                            let low = shown.to_lowercase();
                            low.match_indices(&w).any(|(at, _)| {
                                let before = low[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
                                let after = low[at + w.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
                                before && after
                            })
                        });
                        if let Some(w) = english {
                            wrong.push(format!("{lang:?}: {meta:?} shows {shown:?} (\"{w}\" in English)"));
                        }
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{} meta lines with an English word:\n{}", wrong.len(), wrong.join("\n"));
    }
}
