//! Portable game-card/box data and status/rating marks; no page, system services or file I/O.
use crate::gfx::Canvas;
use crate::image::Image;
use crate::library_meta::{Status, RATING_MAX};
use crate::palette::pal;
use crate::text::{Face, Text};
use crate::type_scale;

thread_local! {
    static TOUCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Touch is the input (Android phones without a controller): the card's lines don't name keys.
pub fn set_touch(on: bool) {
    TOUCH.with(|t| t.set(on));
}

/// What an unrated game's Rating says: how to give one with a controller, plainly "Not rated" under a finger
/// (rating by touch is not there yet).
pub fn no_rating() -> &'static str {
    if TOUCH.with(|t| t.get()) {
        "Not rated"
    } else {
        "None. Press Y"
    }
}

/// The description under "About".
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Summary {
    /// No Wikipedia article is known for the game.
    #[default]
    None,
    Loading,
    /// Lead text of the English Wikipedia article `title` (CC BY-SA).
    Wikipedia {
        text: String,
        title: String,
    },
    /// Could not be loaded (offline); the reason in a few words.
    Unavailable(String),
    /// The description in the player's EmulationStation gamelist (`gamelist::fill_card`): shown when there is no
    /// Wikipedia text on hand, with its source named.
    Gamelist(String),
    /// RomM's description of a game of the paired server (`romm`), when the gamelist has none: shown as the gamelist's is.
    Romm(String),
    /// ScreenScraper's description in the interface's language, else its English one (`screenscraper_facts::fill_card`):
    /// before Wikipedia's and RomM's, after the player's own gamelist.
    ScreenScraper(String),
}

impl Summary {
    /// The line under (or instead of) the description that says why there is none, or where it is from; None when
    /// the text speaks for itself. A game without an article and a game whose text is only not here yet (no Wi-Fi) read
    /// differently: the one stays without, the other gets it with the next connection.
    pub fn note(&self) -> Option<String> {
        self.note_in(crate::device::no_wifi_chip())
    }

    /// [`note`](Self::note) on a handheld with (`no_chip` false) or without Wi-Fi: there the text never comes "with Wi-Fi"
    /// (tests/no_wifi_walk.rs).
    pub fn note_in(&self, no_chip: bool) -> Option<String> {
        Some(match self {
            Summary::Wikipedia { title, .. } => format!("Text: Wikipedia, “{title}”, CC BY-SA 4.0"),
            Summary::Gamelist(_) => "From your EmulationStation gamelist".into(),
            Summary::Romm(_) => "From your RomM server".into(),
            Summary::ScreenScraper(_) => "Text: ScreenScraper.fr, CC BY-NC-SA 4.0".into(),
            Summary::Loading => "Loading the description from Wikipedia…".into(),
            Summary::None => "No Wikipedia article for this game".into(),
            Summary::Unavailable(why) => match why.as_str() {
                "not cached" => return None,
                "offline" | "no curl" if no_chip => "No Wi-Fi here, so no description from Wikipedia".into(),
                "offline" | "no curl" => "Offline: the description comes with Wi-Fi".into(),
                "no article" | "no text" => "No Wikipedia article for this game".into(),
                why => format!("No description from Wikipedia: {why}"),
            },
        })
    }
}

/// The credit line under facts from ScreenScraper (`screenscraper::CREDIT`, the same line): here so the box and the
/// card name it in every module set, the Mobile view's too.
pub const SCREENSCRAPER_CREDIT: &str = "Data and media: ScreenScraper.fr (CC BY-NC-SA 4.0)";

/// A game of the library shown under "More by" or "Other games in".
#[derive(Clone, Debug, PartialEq)]
pub struct RelatedGame {
    pub name: String,
    pub system: String,
    pub stripe: u32,
    pub year: Option<u16>,
}

/// Where a fact of the card leads (Connections, `crate::connections`): the list's title and the player's games in it,
/// ((system, game) when it was made, file).
#[derive(Clone, Debug, PartialEq)]
pub struct CardLink {
    pub title: String,
    pub games: Vec<((usize, usize), std::path::PathBuf)>,
}

/// Everything the card shows; filled in by the app from the library, `gamedb`, `library_meta` and the
/// EmulationStation gamelist (`gamelist::fill_card`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardData {
    pub title: String,
    /// "Game Boy".
    pub system: String,
    pub stripe: u32,
    /// The game has an entry in the offline game list.
    pub matched: bool,
    /// Wikidata's name when it differs from the file's ("Listed as").
    pub listed_as: Option<String>,
    pub year: Option<u16>,
    pub players: Option<String>,
    /// (label, value): Developer, Publisher, Genre, Series.
    pub facts: Vec<(&'static str, String)>,
    /// "Metacritic 92 · via Wikidata".
    pub scores: Option<String>,
    /// One sentence made of the facts, shown when there is no Wikipedia text.
    pub sentence: Option<String>,
    pub summary: Summary,
    /// Unix seconds of the last session and total seconds played, from the recent games.
    pub played: Option<(u64, u64)>,
    /// Shared play evidence, even when this frontend has measured no session.
    pub ever_played: bool,
    pub sessions: u32,
    pub status: Status,
    pub suggestion: Option<Status>,
    /// The player's rating, 1 to 5; 0 while there is none.
    pub rating: u8,
    pub note: String,
    pub more_by: Option<(String, Vec<RelatedGame>)>,
    pub series: Option<(String, Vec<RelatedGame>)>,
    /// RetroAchievements: "12 of 40 achievements" when the game has a set (`achievements::snapshot::Summary`).
    pub achievements: Option<String>,
    /// Curator's rows, in order (none when Curator has nothing to show for the game).
    pub rows: Vec<CuratorRow>,
    /// How long the charge lasts in this game: "About 3 h 20 min left, 18 % an hour", "Measuring this game…"
    /// (`battery_games::card_line`); None on a device without a battery.
    pub battery: Option<String>,
    /// The last time a save of this game came from or went to a paired Pocket: "With TrimUI Brick at 14:05"
    /// (`pocket_sync_page::card_line`); None with Pocket sync off or before any.
    pub synced: Option<String>,
    /// A PortMaster port: its performance and fan ("Battery, fan Quiet"; START changes them, `port_perf_page`), and
    /// whether to show that it keeps a core busy (`port_perf::card_hint`).
    pub port: Option<(String, bool)>,
    /// PortMaster has a newer version of this port: a quiet note under the Speed line (A updates it in Extras).
    pub port_update: bool,
    /// An Android game (Android's handheld view): its app starts it, so the card has no "This game" (START does nothing).
    pub app_game: bool,
    /// Opened by X on Just Play: why this game ("Not played for 3 months · you like platformers"); Right picks again.
    pub surprise: Option<String>,
    /// The game's pictures come from the libretro collection: the status picker offers "Wrong pictures? Look up
    /// again" (`Out::LookUpAgain`).
    pub look_up: bool,
    /// Some of the facts above (year, players, Developer, Publisher, Genre, the rating line) come from the player's
    /// EmulationStation gamelist (`gamelist::fill_card`): the card says so under them.
    pub from_gamelist: bool,
    /// Some of the facts above come from ScreenScraper (`screenscraper_facts::fill_card`): the card names it under them.
    pub from_screenscraper: bool,
    /// A phone game: which buttons are its keys, one line (`j2me_keys::CARD_LINE`).
    pub keys: Option<&'static str>,
    /// The patch next to the game that pocket-core applies when it starts it ("Game (USA).ips", `softpatch`).
    pub patch: Option<String>,
    /// Why the game can't be played, in a sentence ("its file is empty"); every card surface shows the cause and
    /// remedy in the alert colour, separately from its facts and Connections (`app_card`).
    pub broken: Option<String>,
    /// The game is one of the player's Favourites (the info page says so, `game_info`).
    pub favorite: bool,
    /// Its save slots in pocket-core, shown on their save medium (`save_media`); None where pocket-core doesn't keep
    /// its states (another emulator's game, a system without states).
    pub saves: Option<CardSaves>,
    /// When the moment a start would continue from was written (Unix seconds), under "Save slots": None without a
    /// waiting moment (`frozen::moment_time`).
    pub continue_at: Option<u64>,
    /// RetroAchievements' Hardcore holds for a start of this game: it starts from its in-game save, and the card says
    /// so where "Continue" would be (`achievements::hardcore::CARD_LINE`).
    pub hardcore: bool,
    /// Where the player is, read from the game's own battery save ("8 badges · Charizard Lv. 45 · 31:12 h",
    /// `case_knows`), in the interface's language: the first line under "Your game". Set when painted (`paint_card`);
    /// None for a game not read or a save that says nothing for sure.
    pub progress: Option<String>,
    /// `progress` is the RetroAchievements line ("34 of 60 achievements · 56%", `achievements::case_line`) standing in
    /// for a save that says nothing: the "Unlocked" row under it is left out then (it would say the same).
    pub progress_achievements: bool,
    /// Under it, pictures from the player's own ROM (the team, the badges: `case_knows_art`) or a small measure
    /// (`case_knows::Meter`); set with `progress`.
    pub case_art: Option<std::sync::Arc<crate::case_knows_art::Art>>,
    pub case_meter: Option<crate::case_knows::Meter>,
    /// The facts that lead to the player's other games: (index into `facts`, where it leads), in the facts' order.
    /// A fact without one is plain text, never a way into an empty list (`app_connections`).
    pub links: Vec<(usize, CardLink)>,
    /// Opened from Easy: only the picture, the sentence, year, players and time played, and nothing to change here.
    pub plain: bool,
    /// The photos of big moments in its case (`album`): Down on the box's front opens the case. None where there is
    /// no case to open (a page without a box).
    pub album: Option<crate::album::AlbumData>,
}

impl CardData {
    pub fn play_evidence(&self) -> crate::play_evidence::Evidence {
        crate::play_evidence::Evidence {
            recorded: self.ever_played || self.played.is_some() || self.sessions > 0 || self.status != Status::NotStarted,
            files: self.continue_at.is_some() || self.saves.as_ref().is_some_and(|s| s.newest.is_some() || s.filled.iter().any(|filled| *filled)),
            progress: self.progress.is_some(),
        }
    }

    pub fn was_played(&self) -> bool {
        self.play_evidence().ever_played()
    }

    /// The Played row's word when pocket-ui hasn't timed the game yet: "Not yet", or, when the game's own save says it
    /// has been played (the shared evidence, including another Pocket or emulator), "Not here yet", so it never stands under a game
    /// clock as if the game were new.
    pub fn not_played(&self) -> &'static str {
        if self.was_played() { "Not here yet" } else { "Not yet" }
    }

    /// Leaves out what could only be painted as letters nothing here can draw (`latinise`): a fact in Japanese
    /// ("Developer: 任天堂") would be an empty row, a description in Korean a block of nothing, "Listed as" a line of
    /// "????". Text that has a Latin part or a font for the rest stays, the painter writes the rest in Latin letters.
    /// A card without a description says so (`Summary::note`).
    pub fn leave_out_unsayable(&mut self) {
        self.facts.retain(|(_, v)| crate::latinise::shows(v));
        if self.listed_as.as_deref().is_some_and(|l| !crate::latinise::shows(l)) {
            self.listed_as = None;
        }
        if let Summary::Gamelist(text) | Summary::ScreenScraper(text) = &self.summary {
            if !crate::latinise::readable(text) {
                self.summary = Summary::None;
            }
        }
        if self.sentence.as_deref().is_some_and(|t| !crate::latinise::readable(t)) {
            self.sentence = None;
        }
    }
}

/// A game's save slots as the card shows them, read-only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardSaves {
    /// The system, for its save medium, and its stripe (the card's label).
    pub system: String,
    pub stripe: u32,
    /// One per slot: it holds a save.
    pub filled: Vec<bool>,
    /// When the newest slot was saved (Unix seconds).
    pub newest: Option<u64>,
}

impl CardSaves {
    /// The slots of a game pocket-core runs with save states (`hostsys`): a few file checks, no reading, like the
    /// patch's. None for every other game.
    #[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
    pub fn of(system: &str, rom: &std::path::Path, stripe: u32) -> Option<CardSaves> {
        Self::of_core(system, rom, stripe, "")
    }

    #[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
    pub fn of_core(system: &str, rom: &std::path::Path, stripe: u32, core: &str) -> Option<CardSaves> {
        // The handhelds and the two Nintendo home consoles are `HANDHELD_CORES`, not in `SYSTEMS`.
        let handheld = crate::hostsys::HANDHELD_CORES.iter().any(|(s, _)| *s == system);
        if !handheld {
            crate::hostsys::find(system).filter(|h| h.no_states.is_none())?;
        }
        let stem = rom.file_stem()?.to_str()?;
        let base = crate::slots::base_for_core(system, stem, core);
        let times: Vec<Option<std::time::SystemTime>> = (0..crate::slots::SLOTS).map(|i| crate::slots::saved_at(&base, i)).collect();
        let newest = times.iter().flatten().max().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
        Some(CardSaves { system: system.to_string(), stripe, filled: times.iter().map(Option::is_some).collect(), newest })
    }

    /// The "Save slots" line as every page of the game words it: "2 of 8", or "All empty" (true when a slot holds a save).
    pub fn summary(&self) -> (String, bool) {
        let n = self.filled.iter().filter(|f| **f).count();
        if n > 0 { (format!("{n} of {}", self.filled.len()), true) } else { ("All empty".to_string(), false) }
    }
}

/// Which Curator row it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowKind {
    Echoes,
    Versions,
    Stickers,
    /// Not Curator's: "Two versions of this game's save. Choose one…" (`sync_conflicts`), first of the rows.
    SaveVersions,
    /// "Screenshots (3)": the game's own, after Curator's rows (`app_gallery`).
    Screenshots,
    /// "Manual": the game's real manual from ScreenScraper (`app_manual`), only for a game it lists one for.
    Manual,
    /// "Into the crate": Shelf space's, while the game is on the shelf (`app_crate`), before Delete.
    Crate,
    /// "Setting up Dolphin": Android, a GameCube or Wii game that plays in the Dolphin app (`rescue::DialogKind::Dolphin`).
    Dolphin,
    /// "Choose cover…": Android's handheld view, Android's photo picker for the game's own cover (`app_cover_pick`).
    ChooseCover,
    /// "Reset to automatic": a game with a chosen cover, the usual one again (`app_cover_pick`).
    ResetCover,
    /// "Delete from the card": the last but one row (`app_storage`); A opens the delete sheet. Only where the game's
    /// folder can be written (`game_files::can_delete`): not on dArkOS's read-only view, not on Android.
    Delete,
    /// "Hide from the shelf": the last row (`app_storage`); the file stays, Settings › System › Storage shows it again.
    Hide,
}

/// A Curator row on the card: "Echoes" and what it holds in one line ("Burnout 3 · Need for Speed · R4").
#[derive(Clone, Debug, PartialEq)]
pub struct CuratorRow {
    pub kind: RowKind,
    pub title: String,
    pub line: String,
}
/// A picture on the card besides the box art. The file is known to be there (its header was read), so the
/// place is kept at once; the picture itself arrives from a thread (`picture_loader`) and until then the place shows
/// a plain tile.
#[derive(Clone)]
pub struct CardPicture {
    pub label: &'static str,
    /// The size it is drawn at (`picture_loader::fitted` into the card picture area).
    pub size: (i32, i32),
    pub image: Option<std::rc::Rc<Image>>,
}

pub fn paint_mark(c: &mut Canvas, cx: i32, cy: i32, status: Status, color: u32) {
    match status {
        Status::NotStarted => c.ring(cx, cy, 9, 2, color),
        Status::Playing => c.disc(cx, cy, 8, color),
        Status::Paused => {
            c.rect(cx - 7, cy - 9, 5, 18, color);
            c.rect(cx + 2, cy - 9, 5, 18, color);
        }
        Status::Completed => {
            let pts = [(cx - 9, cy), (cx - 3, cy + 7), (cx + 9, cy - 8)];
            for seg in pts.windows(2) {
                let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
                let n = (x1 - x0).abs().max((y1 - y0).abs());
                for k in 0..=n {
                    c.disc(x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n, 2, color);
                }
            }
        }
        Status::Dropped => c.rect(cx - 9, cy - 2, 18, 5, color),
    }
}

/// A rating as five stars of radius `r` in a row from the centre of the first (cx, cy), 8 px apart: the first `rating`
/// in `on`, the rest in `off`.
pub fn paint_stars(c: &mut Canvas, cx: i32, cy: i32, r: i32, rating: u8, on: u32, off: u32) {
    for k in 0..RATING_MAX as i32 {
        let colour = if k < rating as i32 { on } else { off };
        crate::rating::star(c, cx + k * (2 * r + 8), cy, r, colour);
    }
}

/// Soft lift shared by cards and interface panels.
pub fn lift_shadow(c: &mut Canvas, x: i32, y: i32, w: i32, strength: u8) {
    if strength == 0 {
        return;
    }
    let amounts: Vec<u8> = (0..6).map(|k| ((6 - k) as u32 * 6 * strength as u32 / 255) as u8).collect();
    c.shade_rows(x + 4, y, w - 4, 0, &amounts);
}

/// A persistent card problem, outside the facts so Connections keeps its fact indices and selection. `base` is the
/// heading's baseline; the return value is the next free baseline after the complete cause and remedy.
pub(crate) fn paint_problem(c: &mut Canvas, text: &mut Text, why: &str, (x, base, w): (i32, i32, i32)) -> i32 {
    let step = type_scale::line(28, 8);
    text.draw(c, Face::Body, 28, x, base, pal().alert, "Problem");
    let mut y = base + step;
    for line in text.wrap(Face::Body, 28, why, w) {
        text.draw(c, Face::Body, 28, x, y, pal().alert, &line);
        y += step;
    }
    y + 12
}

#[cfg(all(test, target_os = "linux"))]
mod saves_tests {
    use super::CardSaves;
    use std::path::Path;

    #[test]
    fn summary_words_the_slots_the_same_everywhere() {
        let mut c = CardSaves::of("gba", Path::new("/storage/roms/x/Game.gba"), 0).unwrap();
        assert_eq!(c.summary(), ("All empty".to_string(), false));
        c.filled[1] = true;
        c.filled[4] = true;
        assert_eq!(c.summary(), (format!("2 of {}", c.filled.len()), true));
    }

    #[test]
    fn systems_with_states_get_a_save_row_as_the_shelf_names_them() {
        for s in ["snes", "gba", "gb", "gbc", "nes", "megadrive", "n64", "psp", "nds", "pcengine", "dreamcast", "psx", "genesis", "sfc"] {
            let saves = CardSaves::of(s, Path::new("/storage/roms/x/Game.rom"), 0);
            assert!(saves.is_some_and(|c| !c.filled.is_empty()), "{s} shows its save slots");
        }
        for s in ["scummvm", "pico-8", "tic-80", "nosuchsystem"] {
            assert!(CardSaves::of(s, Path::new("/storage/roms/x/Game.rom"), 0).is_none(), "{s} has no save states");
        }
    }
}

#[cfg(test)]
mod played_tests {
    use super::{CardData, CardSaves};

    #[test]
    fn known_save_slots_or_a_resume_moment_mean_played_without_a_timed_session() {
        let fresh = CardData { saves: Some(CardSaves { filled: vec![false; 8], ..Default::default() }), ..Default::default() };
        assert!(!fresh.was_played());
        assert_eq!(fresh.not_played(), "Not yet");
        let mut slot = fresh.clone();
        slot.saves.as_mut().unwrap().filled[3] = true;
        let mut dated = fresh.clone();
        dated.saves.as_mut().unwrap().newest = Some(1234);
        let resume = CardData { continue_at: Some(1234), ..Default::default() };
        for card in [slot, dated, resume] {
            assert!(card.play_evidence().files);
            assert!(card.was_played());
            assert_eq!(card.not_played(), "Not here yet");
            assert_eq!(card.played, None, "a save does not invent measured play time");
        }
    }
}

#[cfg(test)]
mod no_wifi_tests {
    use super::Summary;

    /// 22.11.2026: on the RG28XX and the R36S (no Wi-Fi chip) a game card whose Wikipedia text was never loaded said
    /// "Offline: the description comes with Wi-Fi". The snapshot walk (tests/no_wifi_walk.rs) paints no card that is
    /// still waiting for its text, so only this caught it.
    #[test]
    fn a_card_without_its_text_names_no_wifi_where_there_is_none() {
        for why in ["offline", "no curl"] {
            let s = Summary::Unavailable(why.into());
            assert_eq!(s.note_in(false).as_deref(), Some("Offline: the description comes with Wi-Fi"));
            let none = s.note_in(true).unwrap();
            assert!(none.starts_with("No Wi-Fi here"), "{none}");
        }
        // Everything else reads the same on both.
        for s in [Summary::None, Summary::Loading, Summary::Unavailable("timeout".into()), Summary::Unavailable("not cached".into())] {
            assert_eq!(s.note_in(true), s.note_in(false));
        }
    }
}
