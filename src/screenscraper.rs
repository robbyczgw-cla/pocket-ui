//! ScreenScraper.fr as a second source of box art and the pictures the libretro collection has none of: the box's back,
//! its spine, the whole box as one texture, the cartridge or disc label (docs/SCREENSCRAPER.md). Prepared for 0.8; the
//! settings row and the renderer's use of the new pictures are later steps.
//!
//! * **Only with pocket-ui's developer id.** ScreenScraper's API is for free frontends that present themselves and get a
//!   `devid`/`devpassword`. The release build gets them from the release tool's environment (`build.rs`,
//!   `$OUT_DIR/screenscraper_dev.txt`), never from the repository; a build without them has no ScreenScraper at all ([`available`]).
//! * **The player's own account is optional** ([`login`], [`save_login`]): `<data>/screenscraper/login`, readable by its
//!   owner only (`secret::write_private`), never logged, never on a command line. It brings the account's quota.
//! * **A game is asked for by its checksum** (`jeuInfos.php`): the CRC-32 `rom_id` works out (for a zip or 7z the one
//!   its directory states, with the unpacked size), the size, the file name and ScreenScraper's id of the system. Systems
//!   without a checksum (discs) are left out: the API wants one. Pictures come one by one from `mediaJeu.php`, as PNG,
//!   at most [`Face::max_edge`] pixels on a side, in the ROM's region first, then `wor`, `us`, `eu` ([`pick_media`]).
//! * **Its rules, kept** ([`Client`]): one request at a time (one thread, whatever `maxthreads` allows), at most
//!   `maxrequestspermin` a minute, downloads no faster than `maxdownloadspeed`, the day's counts checked before each
//!   request. 429 waits and tries again twice; 430 and 431 end the day ([`Refusal`], kept in `state.json` until
//!   tomorrow in France); 401, 403, 423 and 426 end the run with a line for the player.
//! * **Where the pictures go** ([`path`]): box art, screenshot and title screen where the libretro ones live
//!   (`covers/`, `snaps/`, `titles/`, so the library shows them as it is), the new faces in folders of their own beside
//!   them (`boxbacks/`, `boxsides/`, `boxtextures/`, `supports/`, `supporttextures/`). A picture already there is never
//!   replaced (a player's own cover stays). What ScreenScraper said of each game is kept in
//!   `<data>/screenscraper/<system>.tsv` ([`Index`]): a stopped run goes on from there, a game it does not know is not
//!   asked for again (that costs the day's quota of unknown games) until its file changes.
//!
//! Every function that talks to the server blocks: a worker's job, never the UI thread. The HTTP is `romm_net`'s: the
//! system's curl on Linux (the whole address, with the ids, goes in through stdin), Android's HTTPS on Android.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;

use crate::romm_net::{Answer, Net};

include!("screenscraper_mask.rs");

/// The API's address. Only `https://`.
pub const API: &str = "https://api.screenscraper.fr/api2";
/// The folder under the data folder with the login, the index and the day's state.
pub const DIR: &str = "screenscraper";
const LOGIN_FILE: &str = "login";
/// The line for Settings → About → Credits and the Box art page (the data and the pictures are CC BY-NC-SA 4.0).
pub const CREDIT: &str = "Data and media: ScreenScraper.fr (CC BY-NC-SA 4.0)";

/// `CREDIT` in the interface's language with the licence's name held together by no-break spaces: a line never breaks
/// inside it (0.8 check video on the Brick: "(CC" / "BY-NC-SA 4.0)" beside the hint bar). The tables keep plain spaces
/// (the Italian typography fix takes no-break spaces out of them).
pub fn credit_shown() -> String {
    // French's " :" likewise: a line never starts with the colon.
    crate::i18n::tr(CREDIT).replace("CC BY-NC-SA 4.0", "CC\u{a0}BY-NC-SA\u{a0}4.0").replace(" :", "\u{a0}:")
}
/// The largest picture that is downloaded (a box texture at 2048 pixels is 2 to 6 MB as PNG).
pub const MAX_MEDIA_BYTES: u64 = 12 << 20;
/// Requests without an answer in a row after which a run stops (`covers::GIVE_UP_AFTER`).
pub const GIVE_UP_AFTER: usize = 3;
/// How long a 429 ("too many threads/requests") is waited out before the request is tried again.
pub const BUSY_WAIT: Duration = Duration::from_secs(30);
/// How often a request answered 429 is tried again before the run stops.
const BUSY_RETRIES: usize = 2;
/// The pause between two requests while the account's per-minute limit is not known yet (before `ssuserInfos`).
const DEFAULT_GAP: Duration = Duration::from_millis(1200);

// ---- the developer id -------------------------------------------------------------------------------------------

/// pocket-ui's developer id for the API. Never printed or logged.
#[derive(Clone, PartialEq, Eq)]
pub struct Dev {
    pub id: String,
    pub password: String,
    pub softname: String,
}

impl std::fmt::Debug for Dev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dev").field("softname", &self.softname).finish_non_exhaustive()
    }
}

/// Unpacks what `build.rs` put in `$OUT_DIR/screenscraper_dev.txt` (hex of the masked `id\npassword\nsoftname`).
pub fn unpack_dev(packed: &str) -> Option<Dev> {
    if packed.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..packed.len()).step_by(2).map(|i| u8::from_str_radix(packed.get(i..i + 2)?, 16).ok()).collect();
    let text = String::from_utf8(screenscraper_mask(&bytes?)).ok()?;
    let mut parts = text.splitn(3, '\n');
    let (id, password, softname) = (parts.next()?, parts.next()?, parts.next()?);
    (!id.is_empty() && !password.is_empty() && !softname.is_empty())
        .then(|| Dev { id: id.to_string(), password: password.to_string(), softname: softname.to_string() })
}

/// The id this build was made with; None in every build without it (the feature is not there).
pub fn dev() -> Option<Dev> {
    // `build.rs` writes the packed id (empty without one) into the build's own folder, never into a printed line.
    Some(include_str!(concat!(env!("OUT_DIR"), "/screenscraper_dev.txt")).trim()).filter(|p| !p.is_empty()).and_then(unpack_dev)
}

/// ScreenScraper is in this build: the settings row shows only then.
pub fn available() -> bool {
    dev().is_some()
}

/// The settings row shows (Settings → Games → Box art & downloads → ScreenScraper): in a build with the id, and in a
/// made-up run's snapshots and tests that ask for it (`POCKETUI_SCREENSCRAPER_ROW=1` with `POCKETUI_MOCK`), where nothing is
/// ever sent.
pub fn shown() -> bool {
    if let Some(on) = SHOWN_HERE.with(|s| s.get()) {
        return on;
    }
    available() || (std::env::var_os("POCKETUI_MOCK").is_some() && std::env::var_os("POCKETUI_SCREENSCRAPER_ROW").is_some_and(|v| v == "1"))
}

thread_local! {
    static SHOWN_HERE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Tests: [`shown`] answers `on` on this thread until the guard goes (a build with and one without the id, side by
/// side; nothing is ever sent, `dev` stays as built).
pub fn shown_for_this_thread(on: bool) -> ShownHere {
    let before = SHOWN_HERE.with(|s| s.replace(Some(on)));
    ShownHere { before, _here: std::marker::PhantomData }
}

/// See [`shown_for_this_thread`].
pub struct ShownHere {
    before: Option<bool>,
    /// Dropped on the thread that made it (not `Send`), so it puts back that thread's answer.
    _here: std::marker::PhantomData<*const ()>,
}

impl Drop for ShownHere {
    fn drop(&mut self) {
        SHOWN_HERE.with(|s| s.set(self.before));
    }
}

/// Which of ScreenScraper's pictures the player wants shown (Settings → Games → Box art & downloads, the three rows under
/// ScreenScraper): each one Off shows exactly what pocket-ui draws without it, also for pictures already on the card,
/// and is not downloaded. The box backs are Off by default (owner 26.11.2026): pocket-ui's own back, with how far the
/// player is, has its charm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Show {
    /// The real spines on the shelf (`box-2D-side`).
    pub spines: bool,
    /// The real label on the cartridge (`support-texture`, `support-2D`; `real_label`).
    pub labels: bool,
    /// The box's real back, turned over (`box-2D-back`; `game_box`).
    pub backs: bool,
    /// The game's real manual: Details → Game manual (`manuel`, fetched only when opened; `app_manual`).
    pub manuals: bool,
}

impl Default for Show {
    fn default() -> Show {
        Show { spines: true, labels: true, backs: false, manuals: true }
    }
}

/// The four kinds of ScreenScraper picture, in `Show`'s order, as Settings (the rows under ScreenScraper) and the 0.8.0
/// Real box pictures card name them.
pub const PICTURE_NAMES: [&str; 4] = ["Real spines", "Real labels", "Real box backs", "Real manuals"];

impl Show {
    /// Kind `i` (`PICTURE_NAMES`: 0 spines, 1 labels, 2 box backs, 3 manuals) shown.
    pub fn get(self, i: usize) -> bool {
        match i {
            0 => self.spines,
            1 => self.labels,
            2 => self.backs,
            _ => self.manuals,
        }
    }

    /// Kind `i` switched (`PICTURE_NAMES`).
    pub fn flip(&mut self, i: usize) {
        let flag = match i {
            0 => &mut self.spines,
            1 => &mut self.labels,
            2 => &mut self.backs,
            _ => &mut self.manuals,
        };
        *flag = !*flag;
    }

    /// Whether any of the four kinds is shown.
    pub fn any(self) -> bool {
        (0..PICTURE_NAMES.len()).any(|i| self.get(i))
    }

    /// Whether `face` is shown, and so fetched: what is not shown is not downloaded. Box art always (the library's own).
    pub fn fetches(self, face: Face) -> bool {
        match face {
            // A series' logo is its spine: shown and fetched with the real spines.
            Face::BoxSide | Face::Wheel => self.spines,
            Face::SupportTexture | Face::Support => self.labels,
            Face::BoxBack => self.backs,
            _ => true,
        }
    }
}

/// What the settings row shows besides On/Off: the account signed in, and the last run's line (its progress, what it
/// found, or why it ended).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RowState {
    pub on: bool,
    /// The three rows under it: which pictures are shown (and fetched).
    pub show: Show,
    /// The account's name; None: a guest.
    pub user: Option<String>,
    /// "Looking up 3 of 120 games…", "42 new pictures", a [`Refusal::message`]; None before the first run.
    pub line: Option<String>,
    /// Today's requests and the day's limit, as the last answer said (0 limit: not known).
    pub today: (u32, u32),
}

impl RowState {
    /// The row in a snapshot (`setup:extras-screenscraper=<state>`, docs/dev/tests.md): `off`, `on`, `running`, `signed`, `quota`.
    pub fn demo(state: &str) -> RowState {
        let on = state != "off";
        match state {
            "running" => RowState { on, user: None, line: Some("Looking up 12 of 140 games…".into()), today: (0, 0), show: Show::default() },
            "signed" => RowState { on, user: Some("player".into()), line: Some("42 new pictures.".into()), today: (168, 20000), show: Show::default() },
            "quota" => RowState { on, user: None, line: Some(Refusal::QuotaToday.message().into()), today: (10000, 10000), show: Show::default() },
            _ => RowState { on, ..RowState::default() },
        }
    }
}

// ---- the player's login -----------------------------------------------------------------------------------------

/// The player's ScreenScraper account (`ssid`, `sspassword`). Never printed or logged.
#[derive(Clone, PartialEq, Eq)]
pub struct Login {
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Login").field("user", &self.user).finish_non_exhaustive()
    }
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

/// The login the player gave, if any.
pub fn login(data_dir: &Path) -> Option<Login> {
    let text = crate::secret::load(&dir(data_dir).join(LOGIN_FILE))?;
    let (user, password) = text.split_once('\n')?;
    let (user, password) = (user.trim(), password.trim_end_matches(['\r', '\n']));
    (!user.is_empty() && !password.is_empty()).then(|| Login { user: user.to_string(), password: password.to_string() })
}

/// Keeps the player's login, readable by its owner only.
pub fn save_login(data_dir: &Path, login: &Login) -> std::io::Result<()> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, what.to_string());
    if login.user.trim().is_empty() || login.user.contains('\n') {
        return Err(bad("user name"));
    }
    if login.password.is_empty() || login.password.contains('\n') {
        return Err(bad("password"));
    }
    crate::secret::write_private(&dir(data_dir).join(LOGIN_FILE), format!("{}\n{}\n", login.user.trim(), login.password).as_bytes())
}

/// Signs out: the login goes; the pictures and the index stay.
pub fn forget_login(data_dir: &Path) {
    let _ = std::fs::remove_file(dir(data_dir).join(LOGIN_FILE));
}

// ---- systems ------------------------------------------------------------------------------------------------------

/// ScreenScraper's `systemeid` for a ROCKNIX system id (`systemesListe.php`; the same numbers as ROCKNIX's own
/// EmulationStation scraper). None: a system it has no id for.
pub fn system_id(system: &str) -> Option<u32> {
    Some(match system {
        "megadrive" | "genesis" => 1,
        "mastersystem" => 2,
        "nes" | "famicom" => 3,
        "snes" | "sfc" => 4,
        "gb" => 9,
        "gbc" => 10,
        "virtualboy" => 11,
        "gba" => 12,
        "gamecube" => 13,
        "n64" => 14,
        "nds" => 15,
        "wii" => 16,
        "3ds" | "n3ds" => 17,
        "wiiu" => 18,
        "sega32x" => 19,
        "segacd" | "megacd" => 20,
        "gamegear" => 21,
        "saturn" => 22,
        "dreamcast" => 23,
        "ngp" => 25,
        "atari2600" => 26,
        "atarijaguar" | "jaguar" => 27,
        "atarilynx" => 28,
        "3do" => 29,
        "pcengine" | "tg16" => 31,
        "xbox" => 32,
        "atari5200" => 40,
        "atari7800" => 41,
        "atarist" => 42,
        "atari800" => 43,
        "wonderswan" => 45,
        "wonderswancolor" => 46,
        "colecovision" => 48,
        "psx" => 57,
        "ps2" => 58,
        "ps3" => 59,
        "psp" => 61,
        "psvita" => 62,
        "amiga" => 64,
        "c64" => 66,
        "neocd" => 70,
        "vic20" => 73,
        "zxspectrum" => 76,
        "channelf" => 80,
        "ngpc" => 82,
        "vectrex" => 102,
        "odyssey2" | "videopac" => 104,
        "supergrafx" => 105,
        "fds" => 106,
        "sg-1000" | "sg1000" => 109,
        "msx" => 113,
        "pcenginecd" | "tg16cd" => 114,
        "intellivision" => 115,
        "msx2" => 116,
        "n64dd" => 122,
        "scummvm" => 123,
        "amigacd32" => 130,
        "cdi" => 133,
        "dos" => 135,
        "neogeo" => 142,
        "supervision" => 207,
        "pokemini" => 211,
        "tic-80" | "tic80" => 222,
        _ => return None,
    })
}

// ---- what a game is asked for by ---------------------------------------------------------------------------------

/// What identifies a game file to the API: a CRC-32 and the size it is of, and the file's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub crc: u32,
    pub size: u64,
    /// Without its copier header (NES, SNES, PC Engine, Lynx), or N64 bytes in big-endian order: asked for when the
    /// file as it is was not found.
    pub alt: Option<(u32, u64)>,
    /// The file's name with its extension (`romnom`).
    pub name: String,
}

/// The copier header `rom_id`'s second checksum leaves out, by system (its size).
fn header_size(system: &str) -> u64 {
    match system {
        "nes" | "famicom" | "fds" => 16,
        "snes" | "sfc" | "pcengine" | "tg16" => 512,
        "atarilynx" => 64,
        _ => 0,
    }
}

/// The file's size and modification time as `rom_id`'s cache keys them.
fn stamp(rom: &Path) -> Option<(u64, u64)> {
    let m = crate::background_io::metadata(rom).ok()?;
    let mtime = m.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Some((m.len(), mtime))
}

/// The identity of a game file: from `rom_id`'s cache when it has the file, else worked out here (the same reading).
/// None for what has no checksum (a disc system, a rar, a zip of several games, a file over `rom_id::MAX_HASH`).
pub fn identity(system: &str, rom: &Path, sums: Option<&crate::rom_id::Cache>) -> Option<Identity> {
    let name = rom.file_name()?.to_str()?.to_string();
    let ext = rom.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    crate::rom_id::dat_name(system)?;
    if matches!(ext.as_str(), "zip" | "7z") {
        // The game inside, as the zip's directory states it: its CRC and its unpacked size.
        let (crc, size) = crate::archive::rom_crc(rom, &[])?;
        return Some(Identity { crc, size, alt: None, name });
    }
    let (size, mtime) = stamp(rom)?;
    let known = sums.and_then(|c| c.sums_of(rom, (size, mtime)));
    let found = match known {
        Some(found) => found,
        None => crate::rom_id::checksums(system, rom).ok()?,
    }?;
    let alt = found.alt.map(|crc| (crc, size.saturating_sub(header_size(system))));
    Some(Identity { crc: found.crc, size, alt, name })
}

// ---- regions -----------------------------------------------------------------------------------------------------

/// The regions a file name's No-Intro / GoodTools tags name, in ScreenScraper's short names ("(USA, Europe)" →
/// `us`, `eu`).
pub fn rom_regions(stem: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut add = |r: &'static str| {
        if !out.contains(&r) {
            out.push(r);
        }
    };
    let mut rest = stem;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else { break };
        let group = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        for tag in group.split(',').map(str::trim) {
            let region = match tag.to_ascii_lowercase().as_str() {
                "world" | "w" => "wor",
                "usa" | "u" | "us" => "us",
                "europe" | "e" | "eu" => "eu",
                "japan" | "j" | "jp" => "jp",
                "ju" => {
                    add("jp");
                    "us"
                }
                "ue" => {
                    add("us");
                    "eu"
                }
                "germany" | "g" => "de",
                "france" | "f" => "fr",
                "spain" | "s" => "sp",
                "italy" | "i" => "it",
                "netherlands" => "nl",
                "sweden" => "se",
                "uk" | "united kingdom" => "uk",
                "korea" | "k" => "kr",
                "china" | "c" => "cn",
                "taiwan" => "tw",
                "brazil" | "b" => "br",
                "australia" | "a" => "au",
                "canada" => "ca",
                "asia" => "asi",
                _ => continue,
            };
            add(region);
        }
    }
    out
}

/// The regions a picture is looked for in, best first: the ROM's own (what ScreenScraper says of the ROM, else its
/// file name), then `wor`, `us`, `eu`, then `jp` and ScreenScraper's own (`ss`).
pub fn region_order(rom_said: &[String], stem: &str) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let named: Vec<String> = if rom_said.is_empty() { rom_regions(stem).into_iter().map(str::to_string).collect() } else { rom_said.to_vec() };
    for r in named.into_iter().chain(["wor", "us", "eu", "jp", "ss"].map(str::to_string)) {
        if !order.contains(&r) {
            // Europe's boxes are often under the UK's own code (Mario Kart DS has `uk`, no `eu`; live 26.11.2026).
            let eu = r == "eu";
            order.push(r);
            if eu && !order.iter().any(|o| o == "uk") {
                order.push("uk".to_string());
            }
        }
    }
    order
}

// ---- pictures ----------------------------------------------------------------------------------------------------

/// A picture of a game ScreenScraper may have.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Face {
    /// The box's front (`box-2D`): `covers/`, where the libretro box art lives.
    Cover,
    /// The game running (`ss`): `snaps/`.
    Snap,
    /// The title screen (`sstitle`): `titles/`.
    Title,
    /// The box's back (`box-2D-back`).
    BoxBack,
    /// The box's spine (`box-2D-side`).
    BoxSide,
    /// The whole box unfolded (`box-texture`).
    BoxTexture,
    /// The cartridge or disc as a picture (`support-2D`).
    Support,
    /// The cartridge's or disc's label as a texture (`support-texture`).
    SupportTexture,
    /// The game's logo, see-through round it (`wheel`): a series' spine on the shelf by series (`series_look`), from one
    /// game of the series only (`series_logo_games`).
    Wheel,
}

impl Face {
    pub const ALL: [Face; 9] = [Face::Cover, Face::Snap, Face::Title, Face::BoxBack, Face::BoxSide, Face::BoxTexture, Face::Support, Face::SupportTexture, Face::Wheel];

    /// ScreenScraper's media type.
    pub fn media(self) -> &'static str {
        match self {
            Face::Cover => "box-2D",
            Face::Snap => "ss",
            Face::Title => "sstitle",
            Face::BoxBack => "box-2D-back",
            Face::BoxSide => "box-2D-side",
            Face::BoxTexture => "box-texture",
            Face::Support => "support-2D",
            Face::SupportTexture => "support-texture",
            Face::Wheel => "wheel",
        }
    }

    /// The folder under the data folder.
    pub fn folder(self) -> &'static str {
        match self {
            Face::Cover => "covers",
            Face::Snap => "snaps",
            Face::Title => "titles",
            Face::BoxBack => "boxbacks",
            Face::BoxSide => "boxsides",
            Face::BoxTexture => "boxtextures",
            Face::Support => "supports",
            Face::SupportTexture => "supporttextures",
            Face::Wheel => "wheels",
        }
    }

    pub fn from_media(media: &str) -> Option<Face> {
        Face::ALL.into_iter().find(|f| f.media() == media)
    }

    /// The longest side the picture is asked for at (`maxwidth`/`maxheight`): the Brick's 1024x768 shows a cover at
    /// most about 700 pixels high; a texture is cut into faces, so it gets twice that.
    pub fn max_edge(self) -> u32 {
        match self {
            Face::Snap | Face::Title => 640,
            Face::BoxTexture | Face::SupportTexture => 2048,
            _ => 1024,
        }
    }
}

/// Where a picture of a game is kept, whether or not it is there yet: `<data>/<folder>/<system>/<rom stem>.png`, the
/// same rule as the box art (`covers::cover_path`) and the libretro pictures (`game_pictures::path`).
pub fn path(data_dir: &Path, face: Face, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    data_dir.join(face.folder()).join(system).join(format!("{stem}.png"))
}

/// A picture of the game that is on the card (from here, the libretro collection or the player), for the renderer's
/// later use of the new faces. Box art: the one the library shows (`covers::active_cover_path`).
pub fn existing(data_dir: &Path, face: Face, system: &str, rom: &Path) -> Option<PathBuf> {
    let p = match face {
        Face::Cover => crate::covers::active_cover_path(data_dir, system, rom),
        _ => path(data_dir, face, system, rom),
    };
    crate::background_io::exists(&p).then_some(p)
}

/// One picture `jeuInfos` lists for a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Media {
    pub kind: String,
    pub region: String,
    pub format: String,
}

/// The picture of `face` to fetch from what a game has: the first region of `order` that has one, else any.
pub fn pick_media<'m>(medias: &'m [Media], face: Face, order: &[String]) -> Option<&'m Media> {
    let of_kind: Vec<&Media> = medias.iter().filter(|m| m.kind == face.media()).collect();
    order.iter().find_map(|r| of_kind.iter().find(|m| &m.region == r).copied()).or_else(|| of_kind.first().copied())
}

// ---- answers -----------------------------------------------------------------------------------------------------

/// A number in an answer: ScreenScraper sends most as strings ("20000").
fn num(v: &Value, key: &str) -> u32 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_u64().map_or(0, |n| n.min(u32::MAX as u64) as u32),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// What `ssuser` says of the account's limits and today's use (`ssuserInfos`, and every `jeuInfos`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quota {
    /// The account's name; empty for a guest.
    pub user: String,
    pub max_threads: u32,
    /// KB/s; 0: not said.
    pub max_kbps: u32,
    pub per_min: u32,
    pub per_day: u32,
    pub ko_per_day: u32,
    pub today: u32,
    pub ko_today: u32,
}

impl Quota {
    pub fn from_ssuser(u: &Value) -> Quota {
        // The documentation spells the per-minute limit two ways.
        let per_min = match num(u, "maxrequestspermin") {
            0 => num(u, "maxrequestsperdmin"),
            n => n,
        };
        Quota {
            user: text(u, "id"),
            max_threads: num(u, "maxthreads"),
            max_kbps: num(u, "maxdownloadspeed"),
            per_min,
            per_day: num(u, "maxrequestsperday"),
            ko_per_day: num(u, "maxrequestskoperday"),
            today: num(u, "requeststoday"),
            ko_today: num(u, "requestskotoday"),
        }
    }

    /// The day's limit is reached, before the server says so.
    pub fn day_over(&self) -> Option<Refusal> {
        if self.per_day > 0 && self.today >= self.per_day {
            Some(Refusal::QuotaToday)
        } else if self.ko_per_day > 0 && self.ko_today >= self.ko_per_day {
            Some(Refusal::UnknownToday)
        } else {
            None
        }
    }

    /// The pause between two requests that keeps under the per-minute limit.
    pub fn gap(&self) -> Duration {
        match self.per_min {
            0 => DEFAULT_GAP,
            n => Duration::from_millis(60_000u64.div_ceil(n as u64)),
        }
    }
}

/// The series a game is part of (`familles`) and its place in it (`numeros`), for the series shelf (`series_of`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Series {
    /// The series' names, the main one first (`principale`), each in English (or world-wide) where ScreenScraper has it,
    /// else in its first language (most come only in French).
    pub families: Vec<String>,
    /// Its number in the series, when ScreenScraper gives a plain number.
    pub number: Option<u32>,
}

/// A list of ScreenScraper's named things (`familles`, `numeros`): each one's name, the main ones first.
fn named(list: Option<&Value>) -> Vec<String> {
    let Some(items) = list.and_then(Value::as_array) else { return Vec::new() };
    let mut out: Vec<(bool, String)> = items
        .iter()
        .filter_map(|it| {
            let names = it.get("noms").and_then(Value::as_array)?;
            let pick = |lang: &str| names.iter().find(|n| text(n, "langue") == lang).map(|n| text(n, "text"));
            let name = pick("en").or_else(|| pick("wor")).or_else(|| names.first().map(|n| text(n, "text")))?;
            let name = name.trim().to_string();
            (!name.is_empty() && !name.contains(['\t', '\n', ';'])).then(|| (text(it, "principale") == "1", name))
        })
        .collect();
    out.sort_by_key(|(main, _)| !*main);
    out.into_iter().map(|(_, n)| n).collect()
}

/// What `jeuInfos` says of a game, as far as the pictures need it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Game {
    pub id: u64,
    /// ScreenScraper's system of the game (`jeu.systeme.id`), which `mediaJeu` wants: not always the one asked by (a
    /// Game Boy Color game in the Game Boy folder is under 10, its parent 9; live 26.11.2026). 0: not said.
    pub system: u32,
    /// The regions ScreenScraper gives the ROM (`rom.romregions`, "fr,us").
    pub rom_regions: Vec<String>,
    pub medias: Vec<Media>,
    pub series: Series,
}

/// The game and the account's numbers from a `jeuInfos` answer; None for one that isn't (an error page).
pub fn parse_game(body: &[u8]) -> Option<(Game, Option<Quota>)> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let response = v.get("response")?;
    let jeu = response.get("jeu")?;
    let id: u64 = text(jeu, "id").parse().ok()?;
    let system = jeu.get("systeme").map_or(0, |s| num(s, "id"));
    let rom_regions = jeu
        .get("rom")
        .map(|r| text(r, "romregions"))
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let medias = jeu
        .get("medias")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|m| text(m, "parent").is_empty() || text(m, "parent") == "jeu")
                .map(|m| Media { kind: text(m, "type"), region: text(m, "region").to_ascii_lowercase(), format: text(m, "format") })
                .filter(|m| !m.kind.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let quota = response.get("ssuser").map(Quota::from_ssuser);
    let series = Series { families: named(jeu.get("familles")), number: named(jeu.get("numeros")).iter().find_map(|n| n.parse().ok()) };
    Some((Game { id, system, rom_regions, medias, series }, quota))
}

/// The account's numbers from an `ssuserInfos` answer.
pub fn parse_user(body: &[u8]) -> Option<Quota> {
    let v: Value = serde_json::from_slice(body).ok()?;
    v.get("response")?.get("ssuser").map(Quota::from_ssuser)
}

// ---- refusals ----------------------------------------------------------------------------------------------------

/// Why a run ended before its games did: each has the player's line ([`Refusal::message`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// 401: the server is too busy for guests and accounts that are not active; or 429 that did not pass.
    Busy,
    /// 403 for pocket-ui's developer id.
    DevRefused,
    /// 403 for the player's login.
    LoginRefused,
    /// 423: the API is closed.
    Closed,
    /// 426: this version of pocket-ui is not accepted any more.
    Outdated,
    /// 430, or the day's count reached: that's it for today.
    QuotaToday,
    /// 431, or the day's count of unknown games reached: that's it for today.
    UnknownToday,
    /// pocket-ui's own brake for a guest (`GUEST_GAMES_PER_DAY`, `GUEST_SHARE`): that's it for today on this handheld.
    DayDone,
    /// A picture's 403 that nothing tells the cause of.
    PicturesRefused,
    /// No answer [`GIVE_UP_AFTER`] times in a row.
    Unreachable,
}

impl Refusal {
    /// The player's line (a toast or the row's text).
    pub fn message(self) -> &'static str {
        match self {
            Refusal::Busy => "ScreenScraper is busy right now. Try again later, or sign in with your ScreenScraper account.",
            Refusal::DevRefused => "ScreenScraper doesn't accept this pocket-ui version. An update will fix it.",
            Refusal::LoginRefused => "ScreenScraper didn't accept your name or password. Check them in Box art & downloads.",
            Refusal::Closed => "ScreenScraper is closed for now. Try again later.",
            Refusal::Outdated => "ScreenScraper wants a newer pocket-ui. Update pocket-ui to download from it.",
            Refusal::QuotaToday => "That's all ScreenScraper allows for today. The rest comes tomorrow.",
            Refusal::UnknownToday => "ScreenScraper didn't know many of your games today. The rest comes tomorrow.",
            Refusal::DayDone => "That's all for today. The rest comes tomorrow.",
            Refusal::PicturesRefused => "ScreenScraper didn't hand out pictures just now. Try again later.",
            Refusal::Unreachable => "Couldn't reach ScreenScraper. Try again later.",
        }
    }

    /// It ends the day, not only this run.
    pub fn for_today(self) -> bool {
        matches!(self, Refusal::QuotaToday | Refusal::UnknownToday | Refusal::DayDone)
    }

    fn code(self) -> &'static str {
        match self {
            Refusal::Busy => "busy",
            Refusal::DevRefused => "dev",
            Refusal::LoginRefused => "login",
            Refusal::Closed => "closed",
            Refusal::Outdated => "outdated",
            Refusal::QuotaToday => "quota",
            Refusal::UnknownToday => "unknown",
            Refusal::DayDone => "day",
            Refusal::PicturesRefused => "pictures",
            Refusal::Unreachable => "unreachable",
        }
    }

    fn from_code(code: &str) -> Option<Refusal> {
        [Refusal::Busy, Refusal::DevRefused, Refusal::LoginRefused, Refusal::Closed, Refusal::Outdated, Refusal::QuotaToday, Refusal::UnknownToday, Refusal::DayDone, Refusal::PicturesRefused, Refusal::Unreachable]
            .into_iter()
            .find(|r| r.code() == code)
    }
}

/// What a status means for the run; None: one that is about this request only (200, 400, 404, 5xx).
pub fn refusal_for(status: u16, body: &[u8], with_login: bool) -> Option<Refusal> {
    Some(match status {
        401 => Refusal::Busy,
        403 => {
            // The same status for both ids; the text says which: "Vérifier vos identifiants développeur" or "Vérifier
            // les identifiants utilisateurs" (live, 26.11.2026). A guest never sends a login, so a user line for one
            // is the developer id's fault only when it names it.
            let t = String::from_utf8_lossy(body).to_lowercase();
            let dev = t.contains("veloppeur") || t.contains("developer") || t.contains("devid");
            if with_login && !dev { Refusal::LoginRefused } else { Refusal::DevRefused }
        }
        423 => Refusal::Closed,
        426 => Refusal::Outdated,
        430 => Refusal::QuotaToday,
        431 => Refusal::UnknownToday,
        _ => return None,
    })
}

// ---- the day's state ---------------------------------------------------------------------------------------------

/// Unix seconds when the next day starts in France (ScreenScraper's counts are by French day), taken as UTC+1: in
/// summer that is an hour after the real start, never before it.
pub fn next_french_day(now_unix: u64) -> u64 {
    const DAY: u64 = 86_400;
    const OFFSET: u64 = 3_600;
    ((now_unix + OFFSET) / DAY + 1) * DAY - OFFSET
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The French day `now` (Unix seconds) is in, as [`next_french_day`] counts days (UTC+1).
pub fn french_day(now_unix: u64) -> u64 {
    (now_unix + 3_600) / 86_400
}

/// A guest asks ScreenScraper for at most this many games a day on one handheld (the owner, 26.11.2026); pictures of
/// games already known come on top of it, within [`GUEST_SHARE`].
pub const GUEST_GAMES_PER_DAY: u32 = 1000;
/// A guest uses at most this share (in percent) of the requests a day ScreenScraper allows it, every request counted
/// (`jeuInfos` and each `mediaJeu`): the guests' quota is shared.
pub const GUEST_SHARE: u32 = 50;
/// Requests whose answer says the day's limit (`jeuInfos`, `ssuserInfos`) a quota sends a day while no answer has said
/// it yet: never a run's worth on a guess, and no picture at all (review 26.11.2026). Counted in the day's file.
pub const PROBES: u32 = 5;
/// The day's counts, per quota: `{"guest": {…}, "user:<name>": {…}}`, or `{"broken": <day>}`.
const DAY_FILE: &str = "day.json";

/// What one quota (a guest's, or an account's) has used today on this handheld, kept in the handheld's data folder so a
/// new start, a new run or a second games card the same day goes on counting. Counts only grow within a day: the
/// server's own counts (which move in steps of ten) raise them, never lower them. Changed only through [`book`] and
/// its siblings, under one lock for the whole program, on the file as it is on the card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// [`french_day`] these counts are of.
    pub day: u64,
    /// Requests booked (every `jeuInfos`, `ssuserInfos` and `mediaJeu`, before it goes).
    pub requests: u32,
    /// Of those, the ones that found nothing (404).
    pub ko: u32,
    /// Games asked for (`jeuInfos` for a game not in the index).
    pub games: u32,
    /// Requests booked while the day's limit was not known ([`PROBES`] at most).
    pub probes: u32,
    /// The day's limits as the last answer said them (0: not said yet).
    pub per_day: u32,
    pub ko_per_day: u32,
    /// The day ended (430, 431, a broken file): nothing more is asked until the next day.
    pub over: Option<Refusal>,
}

/// What a request is booked as ([`book`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Booking {
    /// `jeuInfos` for a game not in the index: a request and a game, in one.
    Game,
    /// A request whose answer says the day's limit (`jeuInfos` again, `ssuserInfos`).
    Ask,
    /// A picture (`mediaJeu`): only on a known limit.
    Media,
}

/// Why a request was not booked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotBooked {
    Refused(Refusal),
    /// A guest's games are used up for today: this new game waits for tomorrow; known games' pictures go on.
    GameHeld,
}

/// Whose quota counts: a guest's, or the account's.
pub fn quota_of(login: Option<&Login>) -> String {
    login.map_or_else(|| "guest".to_string(), |l| format!("user:{}", l.user.to_lowercase()))
}

/// The day's file could not be read as one, or not written: no request goes until it can be (review 26.11.2026: a
/// full, read-only or damaged card must not start the day afresh).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unreadable;

/// The one owner of the day's file in the program: every client (spines, labels, a sign-in's check, …) books under it.
static DAY: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn day_file(data_dir: &Path) -> PathBuf {
    dir(data_dir).join(DAY_FILE)
}

/// A quota's entry as the file has it, checked field by field: every count a whole number, the day's end one of the
/// known ones or none. Anything else is a damaged entry (None).
fn entry_from(v: &Value) -> Option<Ledger> {
    let o = v.as_object()?;
    let n = |k: &str| o.get(k)?.as_u64().filter(|&n| n <= u32::MAX as u64).map(|n| n as u32);
    let over = match o.get("over")? {
        Value::Null => None,
        Value::String(code) => Some(Refusal::from_code(code)?),
        _ => return None,
    };
    Some(Ledger { day: o.get("day")?.as_u64()?, requests: n("requests")?, ko: n("ko")?, games: n("games")?, probes: n("probes")?, per_day: n("per_day")?, ko_per_day: n("ko_per_day")?, over })
}

fn entry_to(l: &Ledger) -> Value {
    serde_json::json!({
        "day": l.day, "requests": l.requests, "ko": l.ko, "games": l.games, "probes": l.probes,
        "per_day": l.per_day, "ko_per_day": l.ko_per_day, "over": l.over.map(Refusal::code),
    })
}

/// The day's file as it is: none is a new one; one that can't be read, isn't an object, or has a damaged entry or
/// marker is `Unreadable`. Read directly, never through `background_io`, whose reads a game's start cancels: an
/// empty answer must never pass for an empty file.
fn read_days(data_dir: &Path) -> Result<serde_json::Map<String, Value>, Unreadable> {
    let days = match std::fs::read_to_string(day_file(data_dir)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(serde_json::Map::new()),
        Err(_) => return Err(Unreadable),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => m,
            _ => return Err(Unreadable),
        },
    };
    let fine = days.iter().all(|(k, v)| if k == "broken" { v.as_u64().is_some() } else { entry_from(v).is_some() });
    if fine { Ok(days) } else { Err(Unreadable) }
}

/// Under the program's one lock: the day's file read as it is now, `who`'s counts for the day `now` is in (a new day
/// starts at nought, keeping the limits), `f` on them, and the file written back when they changed. A day marked broken
/// holds every quota (nothing is written then).
fn transact<R>(data_dir: &Path, who: &str, now: u64, f: impl FnOnce(&mut Ledger) -> R) -> Result<(R, Ledger), Unreadable> {
    let _owner = DAY.lock().unwrap_or_else(|e| e.into_inner());
    let mut days = read_days(data_dir)?;
    let today = french_day(now);
    if days.get("broken").and_then(Value::as_u64) == Some(today) {
        let mut l = Ledger { day: today, over: Some(Refusal::DayDone), ..Ledger::default() };
        let r = f(&mut l);
        return Ok((r, Ledger { over: Some(Refusal::DayDone), ..l }));
    }
    let mut l = days.get(who).and_then(entry_from).unwrap_or(Ledger { day: today, ..Ledger::default() });
    // Forward only: an answer booked on the day before (`Client::note`) never turns a newer day back.
    if l.day < today {
        l = l.next_day(now);
    }
    let before = l.clone();
    let r = f(&mut l);
    // Written only when the counts changed: a look (`Ledger::load`, `day_over`) never writes, so a look at another day
    // can't drop today's broken mark.
    if l != before {
        days.remove("broken");
        days.insert(who.to_string(), entry_to(&l));
        let _ = std::fs::create_dir_all(dir(data_dir));
        crate::fsutil::write_atomic(&day_file(data_dir), Value::Object(days).to_string().as_bytes()).map_err(|_| Unreadable)?;
    }
    Ok((r, l))
}

/// Books one request of `login`'s quota at `now`, before it goes: the day turned over first, its limits checked, the
/// request (and for a new game the game, in the same booking) counted, all on the card. The counts after it come back.
pub fn book(data_dir: &Path, login: Option<&Login>, kind: Booking, now: u64) -> Result<Ledger, NotBooked> {
    let guest = login.is_none();
    match transact(data_dir, &quota_of(login), now, |l| l.book(kind, guest)) {
        Ok((Ok(()), l)) => Ok(l),
        Ok((Err(why), _)) => Err(why),
        Err(Unreadable) => Err(NotBooked::Refused(Refusal::DayDone)),
    }
}

/// What an answer said, taken in under the same lock: the limits, and counts only raised (`Ledger::take`); a day's end
/// (`over`); a miss (`ko`). False when the file can't be kept (the next booking then refuses).
pub fn note(data_dir: &Path, login: Option<&Login>, now: u64, f: impl FnOnce(&mut Ledger)) -> bool {
    transact(data_dir, &quota_of(login), now, f).is_ok()
}

impl Ledger {
    /// `who`'s counts for the day `now` is in, as the file has them (`Unreadable`: there and damaged).
    pub fn load(data_dir: &Path, who: &str, now: u64) -> Result<Ledger, Unreadable> {
        transact(data_dir, who, now, |_| ()).map(|((), l)| l)
    }

    /// The same quota on the day `now` is in: nothing used yet, the limits last said kept.
    pub fn next_day(&self, now: u64) -> Ledger {
        Ledger { day: french_day(now), per_day: self.per_day, ko_per_day: self.ko_per_day, ..Ledger::default() }
    }

    /// Puts these counts in place of `who`'s (setting up a day in tests and tools). False when the file can't be read
    /// as one (nothing is written over it then) or written.
    pub fn save(&self, data_dir: &Path, who: &str) -> bool {
        let me = self.clone();
        transact(data_dir, who, self.day * 86_400, move |l| *l = me).is_ok()
    }

    /// The day's file broke: it starts again as `{"broken": <today>}`, which holds every quota back for the rest of the
    /// day (it can't tell what today used); tomorrow every quota starts afresh. False when even that can't be written.
    pub fn reset_broken(data_dir: &Path, now: u64) -> bool {
        let _owner = DAY.lock().unwrap_or_else(|e| e.into_inner());
        if read_days(data_dir).is_ok() {
            return true;
        }
        let _ = std::fs::create_dir_all(dir(data_dir));
        crate::fsutil::write_atomic(&day_file(data_dir), serde_json::json!({ "broken": french_day(now) }).to_string().as_bytes()).is_ok()
    }

    /// The requests a day this quota may use: an account's whole day, a guest's [`GUEST_SHARE`] of its own. None while
    /// no answer has said the day's limit.
    pub fn allowed(&self, guest: bool) -> Option<u32> {
        match self.per_day {
            0 => None,
            n if guest => Some(n * GUEST_SHARE / 100),
            n => Some(n),
        }
    }

    /// What ends the day before the next request (None: it may be sent, `book` has the rest). A limit of nought is
    /// reached, never "no limit".
    pub fn limit(&self, guest: bool) -> Option<Refusal> {
        if self.over.is_some() {
            return self.over;
        }
        if let Some(allowed) = self.allowed(guest) {
            if self.requests >= allowed {
                return Some(if guest { Refusal::DayDone } else { Refusal::QuotaToday });
            }
        }
        if self.ko_per_day > 0 && self.ko >= self.ko_per_day {
            return Some(Refusal::UnknownToday);
        }
        (self.per_day == 0 && self.probes >= PROBES).then_some(Refusal::Unreachable)
    }

    /// A guest may still ask for another game today.
    pub fn game_allowed(&self, guest: bool) -> bool {
        !guest || self.games < GUEST_GAMES_PER_DAY
    }

    /// One booking on these counts (`book`).
    fn book(&mut self, kind: Booking, guest: bool) -> Result<(), NotBooked> {
        if let Some(r) = self.limit(guest) {
            return Err(NotBooked::Refused(r));
        }
        if self.per_day == 0 {
            // Only requests that bring the limit, and only a few.
            if kind == Booking::Media {
                return Err(NotBooked::Refused(Refusal::Unreachable));
            }
            self.probes += 1;
        }
        if kind == Booking::Game {
            if !self.game_allowed(guest) {
                return Err(NotBooked::GameHeld);
            }
            self.games += 1;
        }
        self.requests = self.requests.saturating_add(1);
        Ok(())
    }

    /// What an answer's `ssuser` said: the limits, and today's counts where they are higher than this handheld's.
    pub fn take(&mut self, q: &Quota) {
        if q.per_day > 0 {
            self.per_day = q.per_day;
        }
        if q.ko_per_day > 0 {
            self.ko_per_day = q.ko_per_day;
        }
        self.requests = self.requests.max(q.today);
        self.ko = self.ko.max(q.ko_today);
    }
}

/// The day's end that holds for `login`'s quota at `now` (a guest's day and an account's are separate); a damaged day's
/// file holds every quota back.
pub fn day_over(data_dir: &Path, login: Option<&Login>, now: u64) -> Option<Refusal> {
    match Ledger::load(data_dir, &quota_of(login), now) {
        Ok(l) => l.limit(login.is_none()),
        Err(Unreadable) => Some(Refusal::DayDone),
    }
}

// ---- the index ---------------------------------------------------------------------------------------------------

/// What ScreenScraper said of one game file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Known: its id, its system (0: the one asked by) and the pictures it has (`type(region)`).
    Found { game: u64, system: u32, medias: Vec<Media>, rom_regions: Vec<String>, series: Series },
    /// Not known by this checksum: not asked again until the file changes.
    Missing,
}

/// `<data>/screenscraper/<system>.tsv`: one line per game file, `stem  crc  size  state`, state `-` (not known) or
/// `<game id> <rom regions> <type(region).format>,…`. Valid for a file while its CRC and size are the same.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    files: BTreeMap<String, (u32, u64, Entry)>,
}

fn index_file(data_dir: &Path, system: &str) -> PathBuf {
    dir(data_dir).join(format!("{system}.tsv"))
}

fn media_text(m: &Media) -> String {
    format!("{}({}).{}", m.kind, m.region, m.format)
}

fn media_from(s: &str) -> Option<Media> {
    let (kind, rest) = s.split_once('(')?;
    let (region, format) = rest.split_once(')')?;
    Some(Media { kind: kind.to_string(), region: region.to_string(), format: format.trim_start_matches('.').to_string() })
}

impl Index {
    pub fn load(data_dir: &Path, system: &str) -> Index {
        let text = crate::background_io::read_to_string(index_file(data_dir, system)).unwrap_or_default();
        let files = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                // The series' two columns came later (`series_of`): a line without them has none.
                let (stem, crc, size, state, families, number) = match f[..] {
                    [stem, crc, size, state] => (stem, crc, size, state, "-", "-"),
                    [stem, crc, size, state, families, number] => (stem, crc, size, state, families, number),
                    _ => return None,
                };
                let entry = if state == "-" {
                    Entry::Missing
                } else {
                    let mut parts = state.splitn(3, ' ');
                    // `<game id>` or `<game id>@<system id>` (a game under another system than the folder's).
                    let id = parts.next()?;
                    let (game, system) = match id.split_once('@') {
                        Some((g, s)) => (g.parse().ok()?, s.parse().ok()?),
                        None => (id.parse().ok()?, 0),
                    };
                    let regions = parts.next().unwrap_or("-");
                    let rom_regions = if regions == "-" { Vec::new() } else { regions.split(',').map(str::to_string).collect() };
                    let medias = parts.next().unwrap_or("").split(',').filter_map(media_from).collect();
                    let families = if families == "-" { Vec::new() } else { families.split(';').map(str::to_string).collect() };
                    Entry::Found { game, system, medias, rom_regions, series: Series { families, number: number.parse().ok() } }
                };
                Some((stem.to_string(), (u32::from_str_radix(crc, 16).ok()?, size.parse().ok()?, entry)))
            })
            .collect();
        Index { files }
    }

    pub fn save(&self, data_dir: &Path, system: &str) -> std::io::Result<()> {
        let mut out = String::new();
        for (stem, (crc, size, entry)) in &self.files {
            let (state, families, number) = match entry {
                Entry::Missing => ("-".to_string(), "-".to_string(), "-".to_string()),
                Entry::Found { game, system, medias, rom_regions, series } => {
                    let regions = if rom_regions.is_empty() { "-".to_string() } else { rom_regions.join(",") };
                    let id = if *system == 0 { game.to_string() } else { format!("{game}@{system}") };
                    let families = if series.families.is_empty() { "-".to_string() } else { series.families.join(";") };
                    let number = series.number.map_or_else(|| "-".to_string(), |n| n.to_string());
                    (format!("{id} {regions} {}", medias.iter().map(media_text).collect::<Vec<_>>().join(",")), families, number)
                }
            };
            out.push_str(&format!("{stem}\t{crc:08X}\t{size}\t{state}\t{families}\t{number}\n"));
        }
        std::fs::create_dir_all(dir(data_dir))?;
        crate::fsutil::write_atomic(&index_file(data_dir, system), out.as_bytes())
    }

    /// What is known of the file `stem` with this identity (a changed file is unknown again).
    pub fn get(&self, stem: &str, id: &Identity) -> Option<&Entry> {
        self.files.get(stem).filter(|(crc, size, _)| (*crc, *size) == (id.crc, id.size)).map(|(_, _, e)| e)
    }

    pub fn set(&mut self, stem: &str, id: &Identity, entry: Entry) {
        self.files.insert(stem.to_string(), (id.crc, id.size, entry));
    }

    /// Forgets a file ("Look up again" on the game card).
    pub fn remove(&mut self, stem: &str) {
        self.files.remove(stem);
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// "Look up again": the game's line goes, so the next run asks for it afresh. The pictures stay; the game card's
/// look-up removes the ones it shows itself (`rom_id::forget`).
pub fn forget(data_dir: &Path, system: &str, rom: &Path) {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let mut index = Index::load(data_dir, system);
    if index.files.contains_key(stem) {
        index.remove(stem);
        let _ = index.save(data_dir, system);
    }
}

/// The series of each game ScreenScraper knows in `system`, by ROM stem (`<data>/screenscraper/<system>.tsv`): for
/// the series shelf, read on its own thread, never on the UI's. Games it doesn't know, and games without a series, are
/// not in it.
/// The games whose logo (`wheel`) the shelf by series asks for: at most one per series. `series`: each series' games
/// (system, ROM) in the order its spine takes them (`series_look::ranked`, key games first). None for a series that has
/// a logo on the card already (any of its games'); else its first game the index lists a logo for; else, while none of
/// its games is in the index yet, its first game (looked up once). Reads the index files, never asks the server: run on
/// the run's thread.
pub fn series_logo_games(data_dir: &Path, series: &[Vec<(String, PathBuf)>]) -> Vec<(String, PathBuf)> {
    let mut indexes: HashMap<String, Index> = HashMap::new();
    let mut out = Vec::new();
    for games in series {
        if games.iter().any(|(sys, rom)| existing(data_dir, Face::Wheel, sys, rom).is_some()) {
            continue;
        }
        let mut known = false;
        let mut pick = None;
        for (sys, rom) in games {
            let index = indexes.entry(sys.clone()).or_insert_with(|| Index::load(data_dir, sys));
            let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            match index.files.get(stem).map(|(_, _, e)| e) {
                Some(Entry::Found { medias, .. }) => {
                    known = true;
                    if medias.iter().any(|m| m.kind == Face::Wheel.media()) {
                        pick = Some((sys.clone(), rom.clone()));
                        break;
                    }
                }
                Some(Entry::Missing) => known = true,
                None => {}
            }
        }
        match pick {
            Some(g) => out.push(g),
            None if !known => out.extend(games.first().cloned()),
            None => {}
        }
    }
    out
}

pub fn series_of(data_dir: &Path, system: &str) -> HashMap<String, (Vec<String>, Option<u32>)> {
    Index::load(data_dir, system)
        .files
        .into_iter()
        .filter_map(|(stem, (_, _, e))| match e {
            Entry::Found { series, .. } if !series.families.is_empty() || series.number.is_some() => Some((stem, (series.families, series.number))),
            _ => None,
        })
        .collect()
}

// ---- the client --------------------------------------------------------------------------------------------------

/// Time for the client: the real one, or the tests' (which only counts).
pub trait Clock {
    fn now(&self) -> Duration;
    /// The wall clock's Unix seconds, for the day's counts (tests set it to stand at midnight).
    fn unix(&self) -> u64 {
        now_unix()
    }
    /// Waits `d` unless `stop` says so first; false when it was stopped.
    fn wait(&self, d: Duration, stop: &dyn Fn() -> bool) -> bool;
}

/// The real clock: waits in short steps so a stop (a game starts, B) is seen within a fifth of a second.
pub struct RealClock(Instant);

impl RealClock {
    pub fn new() -> RealClock {
        RealClock(Instant::now())
    }
}

impl Default for RealClock {
    fn default() -> Self {
        RealClock::new()
    }
}

impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn wait(&self, d: Duration, stop: &dyn Fn() -> bool) -> bool {
        let end = Instant::now() + d;
        loop {
            if stop() {
                return false;
            }
            let now = Instant::now();
            if now >= end {
                return true;
            }
            std::thread::sleep((end - now).min(Duration::from_millis(200)));
        }
    }
}

/// The answer to one request, for the request only (what ends the run is a [`Refusal`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Ok(Vec<u8>),
    /// 404: not known.
    NotFound,
    /// 400: the request was wrong for this game (a name ScreenScraper refuses).
    Bad,
    /// No answer, a server error or a page that is no answer: asked again on the next run.
    NoAnswer,
    /// Not sent: a guest's games are used up for today ([`NotBooked::GameHeld`]).
    Held,
}

/// A picture's download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Got {
    /// In place.
    Saved,
    /// ScreenScraper has none (`NOMEDIA`, 404) or sent what pocket-ui can't show.
    Nothing,
    /// No answer: tried again on the next run.
    NoAnswer,
}

/// The way to ScreenScraper: one request at a time, at the account's pace, within its day.
pub struct Client<'a> {
    net: &'a dyn Net,
    clock: &'a dyn Clock,
    base: String,
    dev: Dev,
    login: Option<Login>,
    pub quota: Quota,
    /// When the last request ended (the clock's time).
    last: Option<Duration>,
    /// Requests in a row without an answer.
    pub silent: usize,
    /// The counts of the quota of `login` as the last booking left them (what the run looks at; the file decides),
    /// and the data folder of the day's file ([`Client::keep_day`]).
    pub ledger: Ledger,
    day_dir: Option<PathBuf>,
    /// The day the last request was booked on: its answer is counted on that day (review round 4), even when it
    /// comes after midnight.
    booked_day: u64,
}

/// A part of an answer the day's file still has to take (`UNSAVED`).
#[derive(Clone, Debug)]
enum Unsaved {
    Miss,
    Over(Refusal),
    Quota(Quota),
}


/// One request at a time on the whole handheld, whatever asks (the run, a sign-in's check): ScreenScraper's `maxthreads`
/// is 1 for guests and most accounts.
static LANE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Answers' parts the card refused to take (a miss, a day's end, a limit), for every client in the program: whoever books
/// next writes them first, and nothing goes until they are written (review round 5: they lived in the client that got
/// them, which a run's end dropped). Per data folder, quota and the day the request was booked on.
static UNSAVED: std::sync::Mutex<Vec<(PathBuf, String, u64, Unsaved)>> = std::sync::Mutex::new(Vec::new());

/// Writes one answer's part on `day` of `who`'s counts in `data_dir` (a newer day is left as it is). The counts after
/// it, or Err when the card refused it.
fn write_part(data_dir: &Path, who: &str, day: u64, part: &Unsaved) -> Result<Option<Ledger>, Unreadable> {
    let mut after = None;
    transact(data_dir, who, day * 86_400, |l| {
        if l.day == day {
            match part {
                Unsaved::Miss => l.ko = l.ko.saturating_add(1),
                Unsaved::Over(why) => l.over = Some(*why),
                Unsaved::Quota(q) => l.take(q),
            }
            after = Some(l.clone());
        }
    })?;
    Ok(after)
}

/// Writes the parts still waiting for `data_dir` (every quota's); true when none is left.
fn write_unsaved(data_dir: &Path) -> bool {
    let mut waiting = UNSAVED.lock().unwrap_or_else(|e| e.into_inner());
    waiting.retain(|(dir, who, day, part)| dir != data_dir || write_part(dir, who, *day, part).is_err());
    !waiting.iter().any(|(dir, ..)| dir == data_dir)
}

/// `s` for an address's query.
fn enc(s: &str) -> String {
    crate::covers::percent_encode(s)
}

impl<'a> Client<'a> {
    /// A client of [`API`]. `base` other than an `https://` address is refused (None).
    pub fn new(net: &'a dyn Net, clock: &'a dyn Clock, base: &str, dev: Dev, login: Option<Login>) -> Option<Client<'a>> {
        base.starts_with("https://").then(|| Client {
            net,
            clock,
            base: base.trim_end_matches('/').to_string(),
            dev,
            login,
            quota: Quota::default(),
            last: None,
            silent: 0,
            ledger: Ledger { day: french_day(now_unix()), ..Ledger::default() },
            day_dir: None,
            booked_day: 0,
        })
    }

    /// The player's own account is in the requests (else a guest's).
    pub fn signed_in(&self) -> bool {
        self.login.is_some()
    }

    /// The ids every request carries. Only ever in an address handed to `Net` (stdin for curl), never logged.
    fn auth(&self) -> String {
        let mut q = format!("devid={}&devpassword={}&softname={}&output=json", enc(&self.dev.id), enc(&self.dev.password), enc(&self.dev.softname));
        if let Some(l) = &self.login {
            q.push_str(&format!("&ssid={}&sspassword={}", enc(&l.user), enc(&l.password)));
        }
        q
    }

    fn url(&self, endpoint: &str, params: &[(&str, String)]) -> String {
        let mut url = format!("{}/{endpoint}?{}", self.base, self.auth());
        for (k, v) in params {
            url.push_str(&format!("&{k}={}", enc(v)));
        }
        url
    }

    /// Counts this quota's day in `data_dir` (`Ledger`): what earlier runs, starts and cards used today counts on.
    pub fn keep_day(&mut self, data_dir: &Path) {
        self.day_dir = Some(data_dir.to_path_buf());
        let now = self.clock.unix();
        self.ledger = match Ledger::load(data_dir, &quota_of(self.login.as_ref()), now) {
            Ok(l) => l,
            // A damaged day's file: today is over for every quota (it can't tell what today used).
            Err(Unreadable) => {
                Ledger::reset_broken(data_dir, now);
                Ledger { day: french_day(now), over: Some(Refusal::DayDone), ..Ledger::default() }
            }
        };
    }

    /// What ends this quota's day, as the counts last read say (`book` decides on the file itself). Counts of a day that
    /// has passed say nothing of today's.
    pub fn day_limit(&self) -> Option<Refusal> {
        if self.day_dir.is_none() {
            return Some(Refusal::DayDone);
        }
        if self.ledger.day != french_day(self.clock.unix()) {
            return None;
        }
        self.ledger.limit(self.login.is_none())
    }

    /// The day's limit is known (an answer said it, today or before).
    pub fn limit_known(&self) -> bool {
        self.ledger.per_day > 0
    }

    /// The day's count for the row ("Today: 168 of 5000 lookups"), as the day's file has it now (not the counts this
    /// client saw last: midnight, another client): booked requests and the share this quota may use (0: not known).
    pub fn day_count(&self) -> (u32, u32) {
        let l = match &self.day_dir {
            Some(d) => Ledger::load(d, &quota_of(self.login.as_ref()), self.clock.unix()).unwrap_or_else(|_| self.ledger.clone()),
            None => self.ledger.clone(),
        };
        (l.requests, l.allowed(self.login.is_none()).unwrap_or(0))
    }

    /// Books the request about to go, on the card (`book`): nothing goes without a day's file to count it in, nor before
    /// the answers it couldn't write yet are written.
    fn reserve(&mut self, kind: Booking) -> Result<(), NotBooked> {
        let Some(d) = self.day_dir.clone() else { return Err(NotBooked::Refused(Refusal::DayDone)) };
        if !write_unsaved(&d) {
            return Err(NotBooked::Refused(Refusal::DayDone));
        }
        self.ledger = book(&d, self.login.as_ref(), kind, self.clock.unix())?;
        self.booked_day = self.ledger.day;
        Ok(())
    }

    /// Writes an answer's part on the day its request was booked on (a newer day is left as it is); kept for the next
    /// booking when the card refuses it.
    fn apply(&mut self, day: u64, part: Unsaved) {
        let Some(d) = self.day_dir.clone() else { return };
        let who = quota_of(self.login.as_ref());
        match write_part(&d, &who, day, &part) {
            Ok(Some(l)) if l.day >= self.ledger.day => self.ledger = l,
            Ok(_) => {}
            Err(Unreadable) => UNSAVED.lock().unwrap_or_else(|e| e.into_inner()).push((d, who, day, part)),
        }
    }

    /// The day ended (430, 431): kept for this quota until the next French day.
    pub fn end_day(&mut self, why: Refusal) {
        let day = if self.booked_day == 0 { french_day(self.clock.unix()) } else { self.booked_day };
        self.apply(day, Unsaved::Over(why));
    }

    /// An answer's parts that count for the quota, taken in before the next request may be booked (the caller holds
    /// `LANE`): a miss, the day's end, the `ssuser` limits.
    fn answered(&mut self, status: u16, refusal: Option<Refusal>, body: &[u8]) {
        self.last = Some(self.clock.now());
        let day = self.booked_day;
        if status == 404 {
            self.apply(day, Unsaved::Miss);
        }
        if let Some(why) = refusal.filter(|r| r.for_today()) {
            self.apply(day, Unsaved::Over(why));
        }
        if status == 200 {
            if let Some(q) = serde_json::from_slice::<Value>(body).ok().and_then(|v| v.get("response")?.get("ssuser").map(Quota::from_ssuser)) {
                self.apply(day, Unsaved::Quota(q.clone()));
                self.quota = Quota { max_threads: q.max_threads.max(self.quota.max_threads), ..q };
            }
        }
    }

    /// Waits for the request's turn: the day's count first, then the per-minute pace.
    fn turn(&mut self, stop: &dyn Fn() -> bool) -> Result<bool, Refusal> {
        if let Some(r) = self.day_limit() {
            return Err(r);
        }
        if let Some(last) = self.last {
            let due = last + self.quota.gap();
            let now = self.clock.now();
            if due > now && !self.clock.wait(due - now, stop) {
                return Ok(false);
            }
        }
        Ok(!stop())
    }


    /// One API request at the account's pace, booked as an [`Booking::Ask`]; 429 is waited out [`BUSY_RETRIES`] times.
    /// `Ok(None)`: stopped.
    pub fn get(&mut self, endpoint: &str, params: &[(&str, String)], stop: &(dyn Fn() -> bool + Sync)) -> Result<Option<Reply>, Refusal> {
        self.get_as(Booking::Ask, endpoint, params, stop)
    }

    fn get_as(&mut self, mut kind: Booking, endpoint: &str, params: &[(&str, String)], stop: &(dyn Fn() -> bool + Sync)) -> Result<Option<Reply>, Refusal> {
        let url = self.url(endpoint, params);
        let mut busy = 0;
        loop {
            if !self.turn(stop)? {
                return Ok(None);
            }
            let lane = LANE.lock().unwrap_or_else(|e| e.into_inner());
            match self.reserve(kind) {
                Ok(()) => {}
                Err(NotBooked::Refused(r)) => return Err(r),
                Err(NotBooked::GameHeld) => return Ok(Some(Reply::Held)),
            }
            // A game is counted once; a retry after 429 is one more request of it.
            kind = if kind == Booking::Game { Booking::Ask } else { kind };
            // A stop that came while this waited (Off, a sign-out): checked right before it goes.
            if stop() {
                return Ok(None);
            }
            let Answer { status, body } = self.net.call("GET", &url, None, None);
            // What the answer says of the quota is in the day's file before the next request may be booked.
            let refusal = refusal_for(status, &body, self.login.is_some());
            self.answered(status, refusal, &body);
            drop(lane);
            if let Some(r) = refusal {
                return Err(r);
            }
            let reply = match status {
                200 => Reply::Ok(body),
                404 => Reply::NotFound,
                400 => Reply::Bad,
                429 if busy < BUSY_RETRIES => {
                    busy += 1;
                    if !self.clock.wait(BUSY_WAIT, stop) {
                        return Ok(None);
                    }
                    continue;
                }
                429 => return Err(Refusal::Busy),
                _ => Reply::NoAnswer,
            };
            self.silent = if reply == Reply::NoAnswer { self.silent + 1 } else { 0 };
            if self.silent >= GIVE_UP_AFTER {
                return Err(Refusal::Unreachable);
            }
            return Ok(Some(reply));
        }
    }

    /// The account's limits and today's use (`ssuserInfos`). Only for an account: a guest's is refused (403).
    pub fn user_info(&mut self, stop: &(dyn Fn() -> bool + Sync)) -> Result<Option<Quota>, Refusal> {
        match self.get("ssuserInfos.php", &[], stop)? {
            Some(Reply::Ok(body)) => {
                // Its `ssuser` is in the day's file already, once (`answered`, under the lane): only read here.
                Ok(parse_user(&body))
            }
            _ => Ok(None),
        }
    }

    /// The game a file is, by its checksum (`jeuInfos`). `new_game`: it counts as one of the day's games (a guest's
    /// are limited, [`Reply::Held`] when they are used up); the second try without a copier header doesn't.
    pub fn game(&mut self, system_id: u32, crc: u32, size: u64, name: &str, new_game: bool, stop: &(dyn Fn() -> bool + Sync)) -> Result<Option<Reply>, Refusal> {
        let params = [
            ("systemeid", system_id.to_string()),
            ("romtype", "rom".to_string()),
            ("crc", format!("{crc:08X}")),
            ("romtaille", size.to_string()),
            ("romnom", name.to_string()),
        ];
        self.get_as(if new_game { Booking::Game } else { Booking::Ask }, "jeuInfos.php", &params, stop)
    }

    /// Downloads one picture into `dest` (by way of `<dest>.part`) unless something is there; kept only as a PNG
    /// pocket-ui will decode. Paced like every request, and no faster than the account's download speed. `Ok(None)`:
    /// stopped (Off, a game started), the download given up at once; [`Got::Nothing`] only for ScreenScraper's own
    /// "none" (`NOMEDIA`, 404), never for a stop or a card that refused the file (those are tried again).
    pub fn download(&mut self, system_id: u32, game: u64, face: Face, media: &Media, dest: &Path, stop: &(dyn Fn() -> bool + Sync)) -> Result<Option<Got>, Refusal> {
        if crate::background_io::exists(dest) {
            return Ok(Some(Got::Saved));
        }
        let edge = face.max_edge().to_string();
        let params = [
            ("systemeid", system_id.to_string()),
            ("jeuid", game.to_string()),
            ("media", format!("{}({})", media.kind, media.region)),
            ("outputformat", "png".to_string()),
            ("maxwidth", edge.clone()),
            ("maxheight", edge),
        ];
        let url = self.url("mediaJeu.php", &params);
        if let Some(d) = dest.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let part = dest.with_extension("part");
        let mut busy = 0;
        loop {
            if !self.turn(stop)? {
                return Ok(None);
            }
            let _ = std::fs::remove_file(&part);
            let started = self.clock.now();
            // A stop while it downloads (Off, a game) ends the transfer, not only the run after it.
            let stopping = AtomicBool::new(stop());
            let fetched = {
                let _lane = LANE.lock().unwrap_or_else(|e| e.into_inner());
                match self.reserve(Booking::Media) {
                    Ok(()) => {}
                    Err(NotBooked::Refused(r)) => return Err(r),
                    Err(NotBooked::GameHeld) => return Ok(Some(Got::NoAnswer)),
                }
                if stop() {
                    return Ok(None);
                }
                let fetched = self.net.fetch(&url, "", &part, MAX_MEDIA_BYTES, &stopping, &|_| {
                    if stop() {
                        stopping.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                });
                // A miss or the day's end in the file before the next request may be booked (a picture's answer has
                // no `ssuser`; a 403 is told apart below, without the lane).
                let status = match fetched {
                    Fetched::Status(s) => s,
                    Fetched::Done => 200,
                    _ => 0,
                };
                let refusal = (status != 403).then(|| refusal_for(status, b"", self.login.is_some())).flatten();
                self.answered(if status == 404 { 404 } else { 0 }, refusal, b"");
                fetched
            };
            use crate::romm::Fetched;
            let got = match fetched {
                Fetched::Done => {
                    let bytes = std::fs::metadata(&part).map_or(0, |m| m.len());
                    // The account's download speed: the rest of the time this picture should have taken.
                    if self.quota.max_kbps > 0 {
                        let should = Duration::from_millis(bytes * 1000 / (self.quota.max_kbps as u64 * 1024));
                        let took = self.clock.now().saturating_sub(started);
                        if should > took && !self.clock.wait(should - took, stop) {
                            let _ = std::fs::remove_file(&part);
                            return Ok(None);
                        }
                    }
                    if stop() {
                        let _ = std::fs::remove_file(&part);
                        return Ok(None);
                    }
                    // Decoded whole before it is kept: a PNG whose header is right and whose picture is broken would
                    // otherwise stay on the card as "there" and never show (review 26.11.2026).
                    if crate::image::png_size(&part).is_some() && crate::image::load_png(&part).is_some() {
                        // A card that refuses the name: tried again on the next run, the picture stays listed.
                        if std::fs::rename(&part, dest).is_ok() { Got::Saved } else { Got::NoAnswer }
                    } else if std::fs::read(&part).is_ok_and(|b| b.trim_ascii().starts_with(b"NOMEDIA")) {
                        Got::Nothing
                    } else {
                        // An error page or a broken file: no answer this time.
                        Got::NoAnswer
                    }
                }
                Fetched::Status(429) if busy < BUSY_RETRIES => {
                    busy += 1;
                    if !self.clock.wait(BUSY_WAIT, stop) {
                        return Ok(None);
                    }
                    continue;
                }
                Fetched::Status(429) => {
                    let _ = std::fs::remove_file(&part);
                    return Err(Refusal::Busy);
                }
                Fetched::Status(403) => {
                    let _ = std::fs::remove_file(&part);
                    // The picture's 403 comes without ScreenScraper's line: only a refused login is known for sure (the
                    // account's own answer says so); anything else gets the neutral line, never "update pocket-ui"
                    // on a guess.
                    // A definite answer of the account's (a refused login, the day's end, closed…) is passed on.
                    if self.login.is_some() {
                        match self.user_info(stop) {
                            Err(r) if r != Refusal::Unreachable => return Err(r),
                            _ => {}
                        }
                    }
                    return Err(Refusal::PicturesRefused);
                }
                Fetched::Status(status) => {
                    let _ = std::fs::remove_file(&part);
                    if let Some(r) = refusal_for(status, b"", self.login.is_some()) {
                        return Err(r);
                    }
                    if status == 404 { Got::Nothing } else { Got::NoAnswer }
                }
                Fetched::Stopped => {
                    let _ = std::fs::remove_file(&part);
                    return Ok(None);
                }
                Fetched::NoSpace | Fetched::CantWrite => Got::NoAnswer,
                Fetched::Unreachable | Fetched::Broke => Got::NoAnswer,
            };
            let _ = std::fs::remove_file(&part);
            self.silent = if got == Got::NoAnswer { self.silent + 1 } else { 0 };
            if self.silent >= GIVE_UP_AFTER {
                return Err(Refusal::Unreachable);
            }
            return Ok(Some(got));
        }
    }
}

// ---- the job -----------------------------------------------------------------------------------------------------

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// Every game was looked at.
    Done,
    /// Stopped (a game started, B): the next run goes on from here.
    Stopped,
    Refused(Refusal),
}

/// What a run got through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// Games asked for (`jeuInfos`).
    pub asked: usize,
    /// Of those, the ones ScreenScraper knew.
    pub found: usize,
    /// Pictures saved.
    pub pictures: usize,
    /// Games left out because they have no checksum (discs, rar) or a system ScreenScraper has no id for.
    pub no_checksum: usize,
    /// Games a guest's brake held back for tomorrow (`GUEST_GAMES_PER_DAY`): not looked up today; the pictures of the
    /// games already known still came.
    pub held: usize,
    pub ended: Ended,
}

/// Where a run is, for the row's progress line: games looked at of all, pictures saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub pictures: usize,
}

/// Prefer cartridge titles the local No-Intro list recognised. Keep library order within each group.
pub fn prioritize_recognized(games: &mut [(String, PathBuf)], names: &crate::rom_id::Names) {
    games.sort_by_key(|(system, rom)| !(crate::rom_id::dat_name(system).is_some() && names.contains_key(rom)));
}

/// The card's own front cover only rules out another front cover. The library's cover cache and existing
/// media for each other face are checked by `run_until`.
pub fn games_for_pass<'a>(
    games: &'a [(String, PathBuf)],
    want: &[Face],
    gamelist: &crate::gamelist::Index,
) -> std::borrow::Cow<'a, [(String, PathBuf)]> {
    if want == [Face::Cover] {
        return std::borrow::Cow::Owned(games.iter().filter(|(sys, rom)| !gamelist.has_box_art(sys, rom)).cloned().collect());
    }
    std::borrow::Cow::Borrowed(games)
}

/// Fetches the `want`ed pictures that are missing for `games` (system id, ROM path): `ssuserInfos` once for the
/// account's limits, then for each game its line in the index or one `jeuInfos`, then a `mediaJeu` per missing
/// picture. `stop` is asked before every request and while waiting; `progress` after each game.
pub fn run_until(
    client: &mut Client,
    data_dir: &Path,
    games: &[(String, PathBuf)],
    want: &[Face],
    progress: &dyn Fn(Progress),
    stop: &(dyn Fn() -> bool + Sync),
) -> Run {
    let stop = &|| stop() || crate::background_io::paused();
    let mut run = Run { asked: 0, found: 0, pictures: 0, no_checksum: 0, held: 0, ended: Ended::Done };
    // This quota's day on this handheld (a guest's and an account's apart): what earlier runs used counts.
    client.keep_day(data_dir);
    // A day's end ScreenScraper said is already in the day's file (`Client::answered`, under the lane); one the counts
    // themselves say (the guest's brake, the probes) needs no writing: they say it again.
    let end = |_: &mut Client, run: Run, why: Refusal| -> Run { Run { ended: Ended::Refused(why), ..run } };
    if let Some(why) = client.day_limit() {
        return Run { ended: Ended::Refused(why), ..run };
    }
    // The games that lack a wanted picture at all (nothing is asked for a game that has them).
    let todo: Vec<&(String, PathBuf)> = games
        .iter()
        .filter(|(sys, rom)| want.iter().any(|&f| existing(data_dir, f, sys, rom).is_none()) && !crate::fsutil::is_empty_file(rom))
        .collect();
    if todo.is_empty() {
        return run;
    }
    // `ssuserInfos` once, before the run's first request: a run with nothing to ask costs none.
    let mut account_read = false;
    let mut read_account = |client: &mut Client| -> Result<bool, Refusal> {
        // Only for an account: ScreenScraper answers a guest's `ssuserInfos` with 403 ("Vérifier les identifiants
        // utilisateurs", live 26.11.2026); a guest's limits come with each `jeuInfos` answer instead.
        if !account_read && client.signed_in() {
            account_read = true;
            client.user_info(stop)?;
        }
        Ok(!stop())
    };
    let sums = crate::rom_id::Cache::load(data_dir);
    let mut indexes: HashMap<String, Index> = HashMap::new();
    let total = todo.len();
    for (done, (sys, rom)) in todo.into_iter().enumerate() {
        if stop() {
            run.ended = Ended::Stopped;
            break;
        }
        let (Some(system_id), Some(id)) = (system_id(sys), identity(sys, rom, Some(&sums))) else {
            run.no_checksum += 1;
            progress(Progress { done: done + 1, total, pictures: run.pictures });
            continue;
        };
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let index = indexes.entry(sys.clone()).or_insert_with(|| Index::load(data_dir, sys));
        // While no answer has said the day's limit (a new handheld's first run over an index from a card), a known game
        // is asked for again: its answer says the limit, before any picture goes on a guess.
        let in_index = index.get(&stem, &id).cloned();
        let was_known = in_index.is_some();
        let known = if client.limit_known() { in_index } else { None };
        let entry = match known {
            Some(e) => e,
            None => {
                match read_account(client) {
                    Err(why) => return end(client, run, why),
                    Ok(false) => return Run { ended: Ended::Stopped, ..run },
                    Ok(true) => {}
                }
                let mut asked = None;
                let mut held = false;
                // The file as it is, then without its copier header (asked only when the first is not known). The first
                // is booked as one of the day's games (a guest's brake: only new look-ups wait for tomorrow, the
                // pictures of known games go on under the day's requests); a known game asked again only to learn the
                // day's limit is not a new one.
                for (i, (crc, size)) in std::iter::once((id.crc, id.size)).chain(id.alt).enumerate() {
                    match client.game(system_id, crc, size, &id.name, i == 0 && !was_known, stop) {
                        Ok(Some(Reply::Held)) => {
                            held = true;
                            break;
                        }
                        Err(why) => return end(client, run, why),
                        Ok(None) => return Run { ended: Ended::Stopped, ..run },
                        Ok(Some(Reply::Ok(body))) => match parse_game(&body) {
                            // Its `ssuser` was taken in under the lane (`Client::answered`), once.
                            Some((game, _)) => {
                                // The description, genre, players and dates came with it: kept for the card, no
                                // request of their own (`screenscraper_facts`).
                                crate::screenscraper_facts::save(data_dir, sys, rom, &crate::screenscraper_facts::parse(&body));
                                asked = Some(Entry::Found { game: game.id, system: game.system, medias: game.medias, rom_regions: game.rom_regions, series: game.series });
                                break;
                            }
                            None => break,
                        },
                        Ok(Some(Reply::NotFound)) => asked = Some(Entry::Missing),
                        Ok(Some(Reply::Bad | Reply::NoAnswer)) => break,
                    }
                }
                if held {
                    run.held += 1;
                } else {
                    run.asked += 1;
                }
                let Some(entry) = asked else {
                    progress(Progress { done: done + 1, total, pictures: run.pictures });
                    continue;
                };
                index.set(&stem, &id, entry.clone());
                let _ = index.save(data_dir, sys);
                entry
            }
        };
        if let Entry::Found { game, system: game_system, medias, rom_regions, series } = &entry {
            // `mediaJeu` by the game's own system, which may not be the folder's.
            let media_system = if *game_system == 0 { system_id } else { *game_system };
            run.found += 1;
            let order = region_order(rom_regions, &stem);
            // Pictures `jeuInfos` lists but `mediaJeu` answers with nothing (`NOMEDIA`): left out of the index, so no
            // later run asks for them again.
            let mut nothing: Vec<Media> = Vec::new();
            for &face in want {
                if existing(data_dir, face, sys, rom).is_some() {
                    continue;
                }
                let Some(media) = pick_media(medias, face, &order) else { continue };
                match read_account(client) {
                    Err(why) => return end(client, run, why),
                    Ok(false) => return Run { ended: Ended::Stopped, ..run },
                    Ok(true) => {}
                }
                match client.download(media_system, *game, face, media, &path(data_dir, face, sys, rom), stop) {
                    Err(why) => return end(client, run, why),
                    Ok(None) => return Run { ended: Ended::Stopped, ..run },
                    Ok(Some(Got::Saved)) => run.pictures += 1,
                    Ok(Some(Got::Nothing)) => nothing.push(media.clone()),
                    Ok(Some(Got::NoAnswer)) => {}
                }
            }
            if !nothing.is_empty() {
                let medias = medias.iter().filter(|m| !nothing.contains(m)).cloned().collect();
                index.set(&stem, &id, Entry::Found { game: *game, system: *game_system, medias, rom_regions: rom_regions.clone(), series: series.clone() });
                let _ = index.save(data_dir, sys);
            }
        }
        progress(Progress { done: done + 1, total, pictures: run.pictures });
    }
    run
}

/// A game's real manual (`manuel`): fetched when the player opens it, kept as its pages' pictures.
#[path = "screenscraper_manual.rs"]
pub mod manual;

#[cfg(test)]
#[path = "screenscraper_tests.rs"]
mod tests;
