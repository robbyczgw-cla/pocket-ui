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
}

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
#[derive(Clone, Debug, Default)]
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
    pub sessions: u32,
    pub status: Status,
    pub suggestion: Option<Status>,
    /// The player's rating, 1 to 5; 0 while there is none.
    pub rating: u8,
    pub note: String,
    pub more_by: Option<(String, Vec<RelatedGame>)>,
    pub series: Option<(String, Vec<RelatedGame>)>,
    /// RetroAchievements: "12 of 40" when the game has a set (`achievements::snapshot::Summary`).
    pub achievements: Option<String>,
    /// Curator's rows, in order (none when Curator has nothing to show for the game).
    pub rows: Vec<CuratorRow>,
    /// How long the charge lasts in this game: "About 3 h 20 min left, 18 % an hour", "Measuring this game…"
    /// (`battery_games::card_line`); None on a device without a battery.
    pub battery: Option<String>,
    /// A PortMaster port: its performance and fan ("Battery, fan Quiet"; START changes them, `port_perf_page`), and
    /// whether to show that it keeps a core busy (`port_perf::card_hint`).
    pub port: Option<(String, bool)>,
    /// PortMaster has a newer version of this port: a quiet note under the Speed line (A updates it in Extras).
    pub port_update: bool,
    /// Opened by "Surprise me": why this game ("Not played for 3 months · you like platformers"); Right picks again.
    pub surprise: Option<String>,
    /// The game's pictures come from the libretro collection: the status picker offers "Wrong pictures? Look up
    /// again" (`Out::LookUpAgain`).
    pub look_up: bool,
    /// Some of the facts above (year, players, Developer, Publisher, Genre, the rating line) come from the player's
    /// EmulationStation gamelist (`gamelist::fill_card`): the card says so under them.
    pub from_gamelist: bool,
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
    /// The facts that lead to the player's other games: (index into `facts`, where it leads), in the facts' order.
    /// A fact without one is plain text, never a way into an empty list (`app_connections`).
    pub links: Vec<(usize, CardLink)>,
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
    #[cfg(target_os = "linux")]
    pub fn of(system: &str, rom: &std::path::Path, stripe: u32) -> Option<CardSaves> {
        // The handhelds and the two Nintendo home consoles are `HANDHELD_CORES`, not in `SYSTEMS`.
        let handheld = crate::hostsys::HANDHELD_CORES.iter().any(|(s, _)| *s == system);
        if !handheld {
            crate::hostsys::find(system).filter(|h| h.no_states.is_none())?;
        }
        let stem = rom.file_stem()?.to_str()?;
        let base = crate::slots::base(system, stem);
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
    /// "Hide or delete": the last thing on the card, under everything else (`app_storage`).
    Manage,
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
    for k in 0..6 {
        c.shade_rect(x + 4, y + k, w - 4, 1, 0, ((6 - k) as u32 * 6 * strength as u32 / 255) as u8);
    }
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
