//! The game card: one calm page about a game of the library (X on a game). Box art and the facts from
//! `gamedb` (Wikidata), the player's own status, time and note, a short description, and what else the
//! player owns by the same developer or in the same series. A plays, B goes back, Y sets the status and the
//! player's rating (a row of the same picker), X edits the note, up/down (L1/R1 a page) scroll.
//!
//! Opened by Curator's "Surprise me", the card says why it picked the game in one line over the facts, and Right picks
//! another (`Out::Another`).
//!
//! Curator's rows (Echoes, Versions, Stickers; docs/CURATOR.md) sit under "Your game". Down moves onto them once the
//! first is in view, as onto spines; A on a row asks the app to open it (`Out::Row`), Up from the first goes back to
//! scrolling and A to Play.
//!
//! Drawn like the shelf: the game is a pulled-out paper label with its system's stripe; everything else
//! is Chalk and Dust on black; related games are spines.

use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::keyboard::{Keyboard, Outcome};
use crate::library_meta::{Status, NOTE_MAX, RATING_MAX};
use crate::pad::Button;
use crate::palette::*;
use crate::text::{Face, Text};
use crate::type_scale;

const LABEL_TOP: i32 = 112;
const LABEL_H: i32 = 300;
/// The scrolling part starts under the label and ends above the hints.
const BODY_TOP: i32 = LABEL_TOP + LABEL_H + 28;
const BODY_BOTTOM_GAP: i32 = 100;
const LINE: i32 = 40;
/// The status picker's row after the five statuses: the player's rating, set with left and right.
const RATING_ROW: usize = Status::ALL.len();
/// The picker's row "Wrong game? Choose another".
const CHOOSE_ROW: usize = RATING_ROW + 1;
/// Its last row, for games whose pictures come from the collection: "Wrong pictures? Look up again" (`rom_id`).
const LOOK_UP_ROW: usize = CHOOSE_ROW + 1;
/// One press of up/down scrolls this far.
const STEP: i32 = 2 * LINE;

/// The step from one fact to the next: 44 px at Body 28, more with Settings → Text size (`type_scale`).
fn row() -> i32 {
    type_scale::row(44, 28)
}

/// The step from one line of a text in Body 28 to the next (`LINE` in Normal).
fn line_step() -> i32 {
    type_scale::line(28, 12)
}

/// The step from one line to the next inside a fact or the note (36 px in Normal).
fn tight() -> i32 {
    type_scale::line(28, 8)
}

/// The room "Surprise me"'s line takes above the columns (a row and a little air).
fn surprise_h() -> i32 {
    row() + 14
}

/// A spine of Curator's rows and of "More by …".
fn spine_h() -> i32 {
    type_scale::row(72, 37)
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
    /// The game is one of the player's Favourites (the info page says so, `game_info`).
    pub favorite: bool,
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

pub enum Mode {
    View,
    /// The status picker with the highlighted row, and the stars on its rating row (0: none).
    Status {
        sel: usize,
        stars: u8,
    },
    Note(Box<Keyboard>),
    /// "Wrong game? Choose another": the entries offered and the highlighted one.
    Choose {
        sel: usize,
        rows: Vec<ChoiceRow>,
    },
    /// "Hide or delete": Hide from the shelf (0) or Delete from the card… (1).
    Manage {
        sel: usize,
    },
    /// "Delete from the card?": what it frees, whether saves and states go too, the second A.
    Delete(DeleteSheet),
}

/// What "Delete from the card" would take, as the sheet says it (`game_files::Plan`, worked out on a thread).
#[derive(Clone, Debug, PartialEq)]
pub struct DeleteInfo {
    pub game_bytes: u64,
    /// The game's files (a folder counts as one).
    pub files: usize,
    /// The first few file names.
    pub names: Vec<String>,
    /// Saves and states found: how many, how big.
    pub kept_files: usize,
    pub kept_bytes: u64,
}

/// The delete sheet's state.
#[derive(Clone, Debug, PartialEq)]
pub struct DeleteSheet {
    /// None while it is worked out; Err when the game can't be deleted (gone, not a game).
    pub info: Option<Result<DeleteInfo, String>>,
    /// Saves and states go too (off unless the player turns it on).
    pub with_saves: bool,
    /// 0: Saves and states, 1: Delete.
    pub sel: usize,
    /// A was pressed on Delete once: the next A deletes.
    pub armed: bool,
    /// Sync shares saves or states: deleting them reaches the other devices.
    pub synced: bool,
    /// The delete runs.
    pub busy: bool,
}

/// One entry offered by "Wrong game? Choose another".
#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceRow {
    pub title: String,
    /// "Game Boy · 1992 · Nintendo R&D1".
    pub meta: String,
    pub choice: crate::gamedb::Choice,
    /// The entry the card shows now.
    pub current: bool,
}

/// What a button press on the card asks the app to do.
#[derive(Debug, PartialEq)]
pub enum Out {
    None,
    Play,
    Back,
    SetStatus(Status),
    /// The rating row of the picker: 0 takes the rating away.
    SetRating(u8),
    SetNote(String),
    /// The status picker's "Wrong game? Choose another": the app finds the entries to offer ([`Card::open_chooser`]).
    ChooseGame,
    /// Its "Wrong pictures? Look up again": the game's pictures and checksum are dropped and looked up afresh
    /// (`rom_id::forget`, `app_identify`).
    LookUpAgain,
    SetChoice(crate::gamedb::Choice),
    /// A on a Curator row.
    Row(RowKind),
    /// START: the "This game" page (`this_game_page`), with "Runs on" (`emu_choice_page`) in it.
    Emulator,
    /// Right on a card opened by "Surprise me": pick another game.
    Another,
    /// "Hide from the shelf" (`app_storage`).
    Hide,
    /// "Delete from the card…": the app works out what it takes ([`Card::delete_info`]).
    PlanDelete,
    /// The second A on Delete.
    Delete { with_saves: bool },
    /// SELECT switched between the box and the page: kept for the next card (`App::game_box`).
    Details { boxed: bool },
}

pub struct Card {
    pub data: CardData,
    /// The game's screenshot and title screen under its facts (`app_pictures.rs`); empty when it has none or the
    /// player chose box art only.
    pub pictures: Vec<CardPicture>,
    pub scroll: i32,
    /// How far the body can scroll, known after it was painted once.
    max_scroll: i32,
    pub mode: Mode,
    /// The Curator row the selection is on (None: the card scrolls, A plays).
    pub focus: Option<usize>,
    /// Where each Curator row is in the scrolling body (top, height), and how tall the body's view is; from the
    /// last paint.
    row_at: std::cell::RefCell<Vec<(i32, i32)>>,
    view_h: i32,
    /// Shown as the game's box (`game_box`, SELECT switches); None: the page.
    pub boxed: Option<crate::game_box::BoxView>,
    /// Shown as the one-screen info page (`game_info`; the Minimal, Cards and Split looks); None: the page or the box.
    pub info: Option<crate::game_info::InfoView>,
    /// Opened as the info page: SELECT goes between it and the full page, and the box is never shown.
    pub info_look: bool,
}

/// A picture on the card besides the box art. The file is known to be there (its header was read), so the
/// place is kept at once; the picture itself arrives from a thread (`picture_loader`) and until then the place shows
/// a plain tile.
#[derive(Clone)]
pub struct CardPicture {
    pub label: &'static str,
    /// The size it is drawn at (`picture_loader::fitted` into [`picture_box`]).
    pub size: (i32, i32),
    pub image: Option<std::rc::Rc<Image>>,
}

/// The baseline of the body's first headings ("About the game", "Your game") below `top`: lower by what Large adds to
/// their type, or the marks over a capital ("Über das Spiel") go past the top of the body and are cut off there.
fn heading_baseline(top: i32) -> i32 {
    top + 34 + type_scale::grow(37)
}

/// The room for one of the card's pictures on a screen `screen_w` wide: two side by side with a gap.
pub fn picture_box(screen_w: i32) -> (i32, i32) {
    ((screen_w - 2 * MARGIN - 24) / 2, 220)
}

impl Card {
    pub fn new(data: CardData) -> Self {
        Card {
            data,
            pictures: Vec::new(),
            scroll: 0,
            max_scroll: 0,
            mode: Mode::View,
            focus: None,
            row_at: Default::default(),
            view_h: 0,
            boxed: None,
            info: None,
            info_look: false,
        }
    }

    /// Where Curator's rows are on screen as last painted, (top, height), cut to the card's scrolling body; None
    /// for a row out of view. For taps (`app_curator`).
    pub fn row_areas(&self) -> Vec<Option<(i32, i32)>> {
        let view = (BODY_TOP, BODY_TOP + self.view_h);
        self.row_at
            .borrow()
            .iter()
            .map(|&(top, h)| {
                let (a, b) = ((BODY_TOP + top - self.scroll).max(view.0), (BODY_TOP + top + h - self.scroll).min(view.1));
                (self.view_h > 0 && b > a).then_some((a, b - a))
            })
            .collect()
    }

    /// Puts the selection on Curator row `i` (a tap), scrolled into view.
    pub fn focus_row(&mut self, i: usize) {
        if i < self.data.rows.len() && matches!(self.mode, Mode::View) {
            self.focus = Some(i);
            self.reveal(i);
        }
    }

    /// Whether row `i` is wholly in view at the current scroll.
    fn row_visible(&self, i: usize) -> bool {
        self.row_at.borrow().get(i).is_some_and(|&(top, h)| self.view_h > 0 && top >= self.scroll && top + h <= self.scroll + self.view_h)
    }

    /// Scrolls just enough to show row `i` whole.
    fn reveal(&mut self, i: usize) {
        let Some((top, h)) = self.row_at.borrow().get(i).copied() else { return };
        if top < self.scroll {
            self.scroll = top.max(0);
        } else if top + h > self.scroll + self.view_h {
            // Clear of the fade at the bottom edge where the card goes on (`paint`'s FADE), else to its end.
            self.scroll = (top + h - self.view_h + 36).min(self.max_scroll.max(top + h - self.view_h));
        }
    }

    /// Whether row `j` follows row `i` with nothing to read between them (Curator's rows do; "Hide or delete" sits
    /// under the description, far below them).
    fn next_to(&self, i: usize, j: usize) -> bool {
        let at = self.row_at.borrow();
        match (at.get(i), at.get(j)) {
            (Some(&(ti, hi)), Some(&(tj, hj))) => (tj - (ti + hi)).abs() <= 40 || (ti - (tj + hj)).abs() <= 40,
            _ => true,
        }
    }

    /// Down on the card: onto the next row when it follows on or is in view (or the next one after the focused row),
    /// else a scroll; past the last row in view the focus goes.
    fn down(&mut self) -> Out {
        let n = self.data.rows.len();
        match self.focus {
            Some(i) if i + 1 < n && (self.next_to(i, i + 1) || self.row_visible(i + 1)) => {
                self.focus = Some(i + 1);
                self.reveal(i + 1);
                Out::None
            }
            Some(i) => {
                self.scroll_by(STEP);
                if !self.row_visible(i) {
                    self.focus = None;
                }
                Out::None
            }
            None => match (0..n).find(|&j| self.row_visible(j)) {
                Some(j) => {
                    self.focus = Some(j);
                    Out::None
                }
                None => self.scroll_by(STEP),
            },
        }
    }

    fn up(&mut self) -> Out {
        match self.focus {
            Some(0) => {
                self.focus = None;
                Out::None
            }
            Some(i) if self.next_to(i - 1, i) || self.row_visible(i - 1) => {
                self.focus = Some(i - 1);
                self.reveal(i - 1);
                Out::None
            }
            // From the last row up into the text above it.
            Some(_) => {
                self.focus = None;
                self.scroll_by(-STEP)
            }
            None => self.scroll_by(-STEP),
        }
    }

    /// Where in the card the player is (for the UI sounds): the mode, the picker row, the typed text.
    pub fn mode_mark(&self) -> (u8, usize, usize, usize) {
        match &self.mode {
            Mode::View => (0, 0, 0, 0),
            Mode::Status { sel, stars } => (1, *sel, *stars as usize, 0),
            Mode::Note(kb) => (2, kb.row * 16 + kb.col, kb.text.len(), kb.shift as usize * 2 + kb.symbols as usize),
            Mode::Choose { sel, rows } => (3, *sel, rows.len(), 0),
            Mode::Manage { sel } => (4, *sel, 0, 0),
            Mode::Delete(d) => (5, d.sel, d.with_saves as usize * 2 + d.armed as usize, d.info.is_some() as usize),
        }
    }

    /// What deleting would take arrived (or why it can't be done); `synced`: Sync shares saves or states.
    pub fn delete_info(&mut self, info: Result<DeleteInfo, String>, synced: bool) {
        if let Mode::Delete(d) = &mut self.mode {
            d.info = Some(info);
            d.synced = synced;
        }
    }

    /// The delete failed: the sheet stays, saying why.
    pub fn delete_failed(&mut self, why: String) {
        if let Mode::Delete(d) = &mut self.mode {
            d.busy = false;
            d.armed = false;
            d.info = Some(Err(why));
        }
    }

    /// Shows the entries for "Wrong game? Choose another", the current one highlighted.
    pub fn open_chooser(&mut self, rows: Vec<ChoiceRow>) {
        let sel = rows.iter().position(|r| r.current).unwrap_or(0);
        self.mode = Mode::Choose { sel, rows };
    }

    pub fn scrollable(&self) -> bool {
        self.max_scroll > 0
    }

    /// Opens the card as the info page (`game_info`) instead of the page or the box.
    pub fn show_info(&mut self) {
        self.boxed = None;
        self.info = Some(crate::game_info::InfoView::default());
        self.info_look = true;
    }

    pub fn press(&mut self, b: Button) -> Out {
        // The info page: up and down scroll its description, SELECT shows the full page; the rest as on the page.
        if let (Mode::View, Some(info)) = (&self.mode, self.info.as_mut()) {
            match b {
                Button::Up | Button::Down | Button::L1 | Button::R1 => {
                    let step = if matches!(b, Button::L1 | Button::R1) { 4 * STEP } else { STEP };
                    info.scroll_by(if matches!(b, Button::Up | Button::L1) { -step } else { step });
                    return Out::None;
                }
                Button::Select => {
                    self.info = None;
                    self.scroll = 0;
                    return Out::None;
                }
                Button::Left => return Out::None,
                Button::Right if self.data.surprise.is_none() => return Out::None,
                _ => {}
            }
        }
        // The box: LEFT/RIGHT turn it over, up and down scroll its back, SELECT shows the page; the rest as on the page.
        if let (Mode::View, Some(bx)) = (&self.mode, self.boxed.as_mut()) {
            match b {
                Button::Right if self.data.surprise.is_some() => return Out::Another,
                Button::Left | Button::Right => {
                    bx.flip();
                    return Out::None;
                }
                Button::Up | Button::Down | Button::L1 | Button::R1 => {
                    let step = if matches!(b, Button::L1 | Button::R1) { 4 * STEP } else { STEP };
                    bx.scroll_by(if matches!(b, Button::Up | Button::L1) { -step } else { step });
                    return Out::None;
                }
                Button::Select => {
                    self.boxed = None;
                    return Out::Details { boxed: false };
                }
                _ => {}
            }
        }
        match &mut self.mode {
            Mode::View => match b {
                Button::East => match self.focus.and_then(|i| self.data.rows.get(i)) {
                    Some(row) => Out::Row(row.kind),
                    None => Out::Play,
                },
                Button::South => Out::Back,
                Button::Start => Out::Emulator,
                // SELECT: back to the info page in the looks that open it, the game's box again in the others.
                Button::Select if self.info_look => {
                    self.focus = None;
                    self.info = Some(crate::game_info::InfoView::default());
                    Out::None
                }
                Button::Select => {
                    self.boxed = Some(crate::game_box::BoxView::new());
                    Out::Details { boxed: true }
                }
                Button::Up => self.up(),
                Button::Down => self.down(),
                Button::Right if self.data.surprise.is_some() => Out::Another,
                Button::L1 => {
                    self.focus = None;
                    self.scroll_by(-(4 * STEP))
                }
                Button::R1 => {
                    self.focus = None;
                    self.scroll_by(4 * STEP)
                }
                // Y (the left face button): the status; the suggestion is picked in advance.
                Button::West => {
                    let current = self.data.suggestion.unwrap_or(self.data.status);
                    self.mode = Mode::Status {
                        sel: Status::ALL.iter().position(|s| *s == current).unwrap_or(0),
                        stars: self.data.rating,
                    };
                    Out::None
                }
                // X (the top face button): the note.
                Button::North => {
                    self.mode = Mode::Note(Box::new(Keyboard {
                        text: self.data.note.clone(),
                        ..Default::default()
                    }));
                    Out::None
                }
                _ => Out::None,
            },
            Mode::Status { sel, stars } => match b {
                // On the rating row left and right give fewer or more stars; elsewhere they move like up and down.
                Button::Left if *sel == RATING_ROW => {
                    *stars = stars.saturating_sub(1);
                    Out::None
                }
                Button::Right if *sel == RATING_ROW => {
                    *stars = (*stars + 1).min(RATING_MAX);
                    Out::None
                }
                Button::East if *sel == RATING_ROW => {
                    let rating = *stars;
                    self.mode = Mode::View;
                    self.data.rating = rating;
                    Out::SetRating(rating)
                }
                Button::Up | Button::Left => {
                    *sel = sel.saturating_sub(1);
                    Out::None
                }
                Button::Down | Button::Right => {
                    *sel = (*sel + 1).min(if self.data.look_up { LOOK_UP_ROW } else { CHOOSE_ROW });
                    Out::None
                }
                Button::East if *sel == CHOOSE_ROW => {
                    self.mode = Mode::View;
                    Out::ChooseGame
                }
                Button::East if *sel == LOOK_UP_ROW => {
                    self.mode = Mode::View;
                    Out::LookUpAgain
                }
                Button::East => {
                    let status = Status::ALL[*sel];
                    self.mode = Mode::View;
                    self.data.status = status;
                    self.data.suggestion = None;
                    Out::SetStatus(status)
                }
                Button::South => {
                    self.mode = Mode::View;
                    Out::None
                }
                _ => Out::None,
            },
            Mode::Choose { sel, rows } => match b {
                Button::Up | Button::Left => {
                    *sel = sel.saturating_sub(1);
                    Out::None
                }
                Button::Down | Button::Right => {
                    *sel = (*sel + 1).min(rows.len().saturating_sub(1));
                    Out::None
                }
                Button::East => {
                    let pick = rows.get(*sel).map(|r| r.choice.clone());
                    self.mode = Mode::View;
                    pick.map_or(Out::None, Out::SetChoice)
                }
                Button::South => {
                    self.mode = Mode::View;
                    Out::None
                }
                _ => Out::None,
            },
            Mode::Manage { sel } => match b {
                Button::Up | Button::Left => {
                    *sel = 0;
                    Out::None
                }
                Button::Down | Button::Right => {
                    *sel = 1;
                    Out::None
                }
                Button::East if *sel == 0 => {
                    self.mode = Mode::View;
                    Out::Hide
                }
                Button::East => {
                    self.mode = Mode::Delete(DeleteSheet { info: None, with_saves: false, sel: 1, armed: false, synced: false, busy: false });
                    Out::PlanDelete
                }
                Button::South => {
                    self.mode = Mode::View;
                    Out::None
                }
                _ => Out::None,
            },
            Mode::Delete(d) => {
                if d.busy {
                    return Out::None;
                }
                let ready = matches!(d.info, Some(Ok(_)));
                let has_saves = matches!(&d.info, Some(Ok(i)) if i.kept_files > 0);
                match b {
                    Button::South if d.armed => d.armed = false,
                    Button::South => self.mode = Mode::View,
                    Button::Up if has_saves => {
                        d.sel = 0;
                        d.armed = false;
                    }
                    Button::Down => {
                        d.sel = 1;
                        d.armed = false;
                    }
                    Button::Left | Button::Right if d.sel == 0 && has_saves => {
                        d.with_saves = !d.with_saves;
                        d.armed = false;
                    }
                    Button::East if d.sel == 0 && has_saves => {
                        d.with_saves = !d.with_saves;
                        d.armed = false;
                    }
                    Button::East if d.sel == 1 && ready && !d.armed => d.armed = true,
                    Button::East if d.sel == 1 && ready => {
                        d.busy = true;
                        return Out::Delete { with_saves: d.with_saves };
                    }
                    _ => {}
                }
                Out::None
            }
            Mode::Note(kb) => {
                let outcome = kb.press(b);
                if kb.text.chars().count() > NOTE_MAX {
                    kb.text = kb.text.chars().take(NOTE_MAX).collect();
                }
                match outcome {
                    Outcome::Editing => Out::None,
                    Outcome::Cancelled => {
                        self.mode = Mode::View;
                        Out::None
                    }
                    Outcome::Done(text) => {
                        self.mode = Mode::View;
                        let note = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        self.data.note = note.clone();
                        Out::SetNote(note)
                    }
                }
            }
        }
    }

    fn scroll_by(&mut self, d: i32) -> Out {
        self.scroll = (self.scroll + d).clamp(0, self.max_scroll);
        Out::None
    }

    /// The button hints for the current mode.
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.mode {
            Mode::View if self.boxed.is_some() => {
                // Y (the status) and X (the note) work as on the page; their hints make room for the box's own, so the
                // bar fits a 640 px screen.
                let mut h = vec![("A", "Play"), ("B", "Back"), ("LEFT RIGHT", "Turn over"), ("SELECT", "Page")];
                // START as on the page: the game's settings, a port's performance and fan.
                if self.data.surprise.is_none() {
                    h.push(("START", if self.data.port.is_some() { "Performance" } else { "This game" }));
                }
                if self.data.surprise.is_some() {
                    h[2] = ("LEFT", "Turn over");
                    h.insert(2, ("RIGHT", "Another"));
                }
                if self.boxed.as_ref().is_some_and(|b| b.scrollable()) {
                    h.push(("L1 R1", "Scroll"));
                }
                h
            }
            Mode::View if self.info.is_some() => {
                let start = if self.data.port.is_some() { "Performance & fan" } else { "This game" };
                let mut h = vec![("A", "Play"), ("B", "Back"), ("Y", "Status"), ("START", start), ("SELECT", "Details")];
                if self.data.surprise.is_some() {
                    h.retain(|(k, _)| *k != "START");
                    h.insert(2, ("RIGHT", "Another"));
                }
                if self.info.as_ref().is_some_and(|i| i.scrollable()) {
                    h.push(("L1 R1", "Scroll"));
                }
                h
            }
            Mode::View => {
                let a = if self.focus.is_some() { "Open" } else { "Play" };
                // START: "This game" (picture, performance, controls, "Runs on"); a port has one emulator, so its performance and fan instead.
                let start = if self.data.port.is_some() { "Performance & fan" } else { "This game" };
                let mut h = vec![("A", a), ("B", "Back"), ("Y", "Status"), ("X", "Note"), ("START", start)];
                if self.data.surprise.is_some() {
                    // A card of "Surprise me": another pick is the first thing after Play and Back; the emulator waits.
                    h.retain(|(k, _)| *k != "START");
                    h.insert(2, ("RIGHT", "Another"));
                }
                if self.scrollable() {
                    h.push(("L1 R1", "Scroll"));
                }
                h
            }
            Mode::Status { sel: RATING_ROW, .. } => vec![("LEFT RIGHT", "Stars"), ("A", "Set"), ("B", "Cancel")],
            Mode::Status { .. } => vec![("A", "Set"), ("B", "Cancel")],
            Mode::Note(_) => vec![("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Save")],
            Mode::Choose { .. } => vec![("A", "This one"), ("B", "Cancel")],
            Mode::Manage { .. } => vec![("A", "Choose"), ("B", "Cancel")],
            Mode::Delete(DeleteSheet { busy: true, .. }) => Vec::new(),
            Mode::Delete(DeleteSheet { armed: true, .. }) => vec![("A", "Delete now"), ("B", "Keep it")],
            Mode::Delete(DeleteSheet { sel: 0, .. }) => vec![("A", "Change"), ("B", "Cancel")],
            Mode::Delete(DeleteSheet { info: Some(Ok(_)), .. }) => vec![("A", "Delete"), ("B", "Cancel")],
            Mode::Delete(_) => vec![("B", "Cancel")],
        }
    }

    /// Paints the card; `cover` is the box art fitted into [`cover_box`], `corner` the status corner at the top right.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>, corner: Option<&crate::status_corner::Corner>) {
        if let Mode::Note(kb) = &self.mode {
            c.clear(pal().void);
            let title = text.fit(Face::Title, 50, &format!("Note for {}", self.data.title), c.w - 2 * MARGIN);
            text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &title);
            crate::setup::paint_keyboard_with(c, text, kb, "Save");
            let left = NOTE_MAX.saturating_sub(kb.text.chars().count());
            text.draw_right(c, Face::Body, 28, c.w - MARGIN, 272, pal().dust, &format!("{left} left"));
            crate::setup::hints(c, text, &self.hints());
            return;
        }
        c.clear(pal().void);
        if self.info.is_some() {
            return self.paint_info(c, text, cover, None, corner);
        }
        let left = corner.map_or(c.w - MARGIN, |k| k.paint(c, text, c.w - MARGIN, 72, pal().dust));
        let system = text.fit(Face::Body, 28, &self.data.system, crate::status_corner::room_left_of(left, c.w - MARGIN) - MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, &system);
        self.paint_label(c, text, cover);
        // The body scrolls inside its own strip of the screen; whatever falls outside is cut off there. On a screen
        // too short for the label and the hints (640x480: 440 px top, 380 px bottom) the strip is empty rather than
        // upside down; the card's top still paints, the body simply has no room.
        let top = BODY_TOP.clamp(0, c.h);
        let bottom = (c.h - BODY_BOTTOM_GAP).clamp(top, c.h);
        let view_h = bottom - top;
        let w = c.w;
        {
            let start = (top * w) as usize;
            let end = (bottom * w) as usize;
            let mut body = Canvas::new(&mut c.buf[start..end], w as u32, view_h as u32);
            let height = self.paint_body(&mut body, text, -self.scroll);
            self.view_h = view_h;
            self.max_scroll = (height - view_h).max(0);
            self.scroll = self.scroll.min(self.max_scroll);
            // Text running under an edge fades out there instead of being cut off mid-line.
            const FADE: i32 = 36;
            for k in 0..FADE {
                let amount = (255 - k * 255 / FADE) as u8;
                if self.scroll > 0 {
                    body.shade_rect(0, k, w, 1, pal().void, amount);
                }
                if self.scroll < self.max_scroll {
                    body.shade_rect(0, view_h - 1 - k, w, 1, pal().void, amount);
                }
            }
        }
        if self.max_scroll > 0 && view_h > 0 {
            // A thin bar on the right: where in the page the view is.
            let total = view_h + self.max_scroll;
            let bar_h = (view_h * view_h / total).max(40);
            let bar_y = BODY_TOP + (view_h - bar_h) * self.scroll / self.max_scroll.max(1);
            c.rect(c.w - 28, BODY_TOP, 4, view_h, pal().spine);
            c.rect(c.w - 28, bar_y, 4, bar_h, pal().dust);
        }
        crate::setup::hints(c, text, &self.hints());
        match &self.mode {
            Mode::Status { sel, stars } => self.paint_picker(c, text, *sel, *stars),
            Mode::Choose { sel, rows } => self.paint_chooser(c, text, *sel, rows),
            Mode::Manage { sel } => self.paint_manage(c, text, *sel),
            Mode::Delete(d) => self.paint_delete(c, text, d),
            _ => {}
        }
    }

    /// Paints the card as the info page (`game_info`): the status corner on the top line, the page, the hints and
    /// whatever is open over it.
    pub fn paint_info(&mut self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>, console: Option<&crate::system_art::Sprite>, corner: Option<&crate::status_corner::Corner>) {
        if matches!(self.mode, Mode::Note(_)) {
            return self.paint(c, text, cover, corner);
        }
        c.clear(pal().void);
        if let Some(k) = corner {
            k.paint(c, text, c.w - MARGIN, crate::game_box::top_line(c.h), pal().dust);
        }
        // No Curator rows on this page: none to tap.
        self.row_at.borrow_mut().clear();
        if let Some(info) = self.info.as_mut() {
            info.paint(c, text, &self.data, cover, console);
        }
        crate::setup::hints(c, text, &self.hints());
        match &self.mode {
            Mode::Status { sel, stars } => self.paint_picker(c, text, *sel, *stars),
            Mode::Choose { sel, rows } => self.paint_chooser(c, text, *sel, rows),
            Mode::Manage { sel } => self.paint_manage(c, text, *sel),
            Mode::Delete(d) => self.paint_delete(c, text, d),
            _ => {}
        }
    }

    /// Paints the card as the game's box (`game_box`): the top line, the box, the hints and whatever is open over it.
    pub fn paint_box(&mut self, c: &mut Canvas, text: &mut Text, inputs: &crate::game_box::Inputs, corner: Option<&crate::status_corner::Corner>) {
        if matches!(self.mode, Mode::Note(_)) || self.boxed.is_none() {
            return self.paint(c, text, None, corner);
        }
        c.clear(pal().void);
        let top_y = crate::game_box::top_line(c.h);
        let left = corner.map_or(c.w - MARGIN, |k| k.paint(c, text, c.w - MARGIN, top_y, pal().dust));
        let line = format!("{} · {}", self.data.system, self.data.title);
        let line = text.fit(Face::Body, 28, &line, crate::status_corner::room_left_of(left, c.w - MARGIN) - MARGIN);
        text.draw(c, Face::Body, 28, MARGIN, top_y, pal().dust, &line);
        if let Some(bx) = self.boxed.as_mut() {
            bx.paint(c, text, &self.data, &self.pictures, inputs);
        }
        crate::setup::hints(c, text, &self.hints());
        match &self.mode {
            Mode::Status { sel, stars } => self.paint_picker(c, text, *sel, *stars),
            Mode::Choose { sel, rows } => self.paint_chooser(c, text, *sel, rows),
            Mode::Manage { sel } => self.paint_manage(c, text, *sel),
            Mode::Delete(d) => self.paint_delete(c, text, d),
            _ => {}
        }
    }

    fn paint_label(&self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>) {
        let d = &self.data;
        let (x, y, w) = (MARGIN, LABEL_TOP, c.w - 2 * MARGIN);
        c.rect(x, y, w, LABEL_H, pal().label);
        lift_shadow(c, x, y + LABEL_H, w, pal().t);
        c.rect(x, y, STRIPE, LABEL_H, d.stripe);
        let mut text_w = w - STRIPE - 2 * 36;
        if let Some(img) = cover {
            let (cw, ch) = (img.w as i32, img.h as i32);
            c.blit_scaled(img, x + w - 12 - cw, y + (LABEL_H - ch) / 2, cw, ch);
            text_w -= cw + 12;
        }
        let tx = x + STRIPE + 36;
        // The title as large as it fits: 67, else 50 on up to two lines.
        let mut base = y + 104;
        if text.width(Face::Title, 67, &d.title) <= text_w {
            text.draw(c, Face::Title, 67, tx, base, pal().ink, &d.title);
        } else {
            let lines = text.wrap(Face::Title, 50, &d.title, text_w);
            base = y + 80;
            for (i, line) in lines.iter().take(2).enumerate() {
                let line = if i == 1 && lines.len() > 2 {
                    text.fit(Face::Title, 50, &format!("{line} {}", lines[2..].join(" ")), text_w)
                } else {
                    line.clone()
                };
                text.draw(c, Face::Title, 50, tx, base + i as i32 * type_scale::line(50, 6), pal().ink, &line);
            }
            base += type_scale::line(50, 6) * (lines.len().min(2) as i32 - 1);
        }
        let mut meta = vec![d.system.clone()];
        meta.extend(d.year.map(|y| y.to_string()));
        meta.extend(d.players.clone());
        let meta = text.fit(Face::Body, 28, &meta.join(" · "), text_w);
        text.draw(c, Face::Body, 28, tx, base + 50 + type_scale::grow(28), pal().label_dust, &meta);
        // The player's own line at the foot of the label: status mark, status, time.
        let foot = y + LABEL_H - 40;
        let mut fx = tx;
        if d.status != Status::NotStarted {
            paint_mark(c, fx + 10, foot - 10, d.status, pal().ink);
            fx += 32;
        }
        fx += text.draw(c, Face::Body, 28, fx, foot, pal().ink, d.status.label());
        let time = match d.played {
            Some((last, secs)) => format!("{} played, last {}", crate::recents::duration(secs), crate::recents::ago(last)),
            None => "Not played yet".to_string(),
        };
        let time = text.fit(Face::Body, 28, &time, tx + text_w - fx - 40);
        text.draw(c, Face::Body, 28, fx + 40, foot, pal().label_dust, &time);
    }

    /// The scrolling part, drawn at `y0` (negative when scrolled); returns its full height.
    fn paint_body(&self, c: &mut Canvas, text: &mut Text, y0: i32) -> i32 {
        let d = &self.data;
        let full_w = c.w - 2 * MARGIN;
        let (lx, rx) = (MARGIN, MARGIN + 610);
        let (lw, rw) = (rx - lx - 50, c.w - MARGIN - rx);
        // Label columns as wide as their labels need at the text size in use.
        let key_w = type_scale::wide(170, 28);
        // "Surprise me": why this game, one line across both columns above them.
        let mut top = y0;
        if let Some(why) = &d.surprise {
            let lead = "Surprise me";
            let lw0 = text.draw(c, Face::Body, 28, MARGIN, y0 + 34, pal().dust, lead);
            // Two lines at most: German and Large text don't fit the reason on one.
            let lines = text.wrap(Face::Body, 28, why, full_w - lw0 - 24);
            for (i, line) in lines.iter().take(2).enumerate() {
                let line = if i == 1 && lines.len() > 2 { text.fit(Face::Body, 28, &format!("{line} {}", lines[2..].join(" ")), full_w - lw0 - 24) } else { line.clone() };
                text.draw(c, Face::Body, 28, MARGIN + lw0 + 24, y0 + 34 + i as i32 * tight(), pal().amber, &line);
            }
            top += surprise_h() + (lines.len().min(2) as i32 - 1).max(0) * tight();
        }
        // Left column: the facts.
        let mut ly = heading_baseline(top);
        text.draw(c, Face::Title, 37, lx, ly, pal().dust, "About the game");
        ly += row();
        // Facts the offline list gave, or only the starter pack's author and licence (`app_card`).
        if d.matched || !d.facts.is_empty() {
            let mut rows = d.facts.clone();
            if let Some(s) = &d.scores {
                rows.push(("Reviews", s.clone()));
            }
            if let Some(l) = &d.listed_as {
                rows.push(("Listed as", l.clone()));
            }
            for (k, v) in rows {
                text.draw(c, Face::Body, 28, lx, ly, pal().dust, k);
                for (i, line) in text.wrap(Face::Body, 28, &v, lw - key_w).iter().take(2).enumerate() {
                    text.draw(c, Face::Body, 28, lx + key_w, ly + i as i32 * tight(), pal().chalk, line);
                    if i == 1 {
                        ly += tight();
                    }
                }
                ly += row();
            }
        } else {
            for line in text.wrap(Face::Body, 28, "Not in the offline game list, so there are no facts to show.", lw) {
                text.draw(c, Face::Body, 28, lx, ly, pal().dust, &line);
                ly += tight();
            }
        }
        // Right column: the player's game.
        let mut ry = heading_baseline(top);
        text.draw(c, Face::Title, 37, rx, ry, pal().dust, "Your game");
        ry += row();
        let rkey = type_scale::wide(150, 28);
        text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Status");
        text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, d.status.label());
        ry += row();
        if let Some(s) = d.suggestion {
            let hint = text.fit(Face::Body, 28, &format!("{}? Press Y", s.label()), rw - rkey);
            text.draw(c, Face::Body, 28, rx + rkey, ry - 6, pal().amber, &hint);
            ry += row();
        }
        // The player's rating: five small stars, the given ones in the text colour; none yet says how to give one.
        text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Rating");
        if d.rating == 0 {
            text.draw(c, Face::Body, 28, rx + rkey, ry, pal().dust, "None. Press Y");
        } else {
            // As large as the text beside them: 13 px stars at Body 28, more with Settings → Text size.
            let r = type_scale::wide(13, 28);
            paint_stars(c, rx + rkey + r, ry - type_scale::mid(28), r, d.rating, pal().chalk, mix(pal().dust, pal().void, 150));
        }
        ry += row();
        let sessions = match d.sessions {
            0 => String::new(),
            1 => ", 1 session".to_string(),
            n => format!(", {n} sessions"),
        };
        let played = d
            .played
            .map_or("Not yet".to_string(), |(_, secs)| format!("{}{sessions}", crate::recents::duration(secs)));
        text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Played");
        let played = text.fit(Face::Body, 28, &played, rw - rkey);
        text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, &played);
        ry += row();
        if let Some((last, _)) = d.played {
            text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Last");
            text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, &crate::recents::ago(last));
            ry += row();
        }
        if let Some(b) = &d.battery {
            text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Battery");
            // Too long for one line: the rate ("18 % an hour") or whose measurement it is ("(PS2 games)") goes on a
            // second, quieter line under the time.
            let (first, second) = match battery_split(b) {
                Some((time, rest)) if text.width(Face::Body, 28, b) > rw - rkey => (time, Some(rest)),
                _ => (b.as_str(), None),
            };
            let first = text.fit(Face::Body, 28, first, rw - rkey);
            text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, &first);
            ry += row();
            if let Some(rest) = second {
                let rest = text.fit(Face::Body, 28, rest, rw - rkey);
                text.draw(c, Face::Body, 28, rx + rkey, ry - 6, pal().dust, &rest);
                ry += row();
            }
        }
        if let Some((speed, hint)) = &d.port {
            text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Speed");
            let speed = text.fit(Face::Body, 28, speed, rw - rkey);
            text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, &speed);
            ry += row();
            if *hint {
                // As the status suggestion: one calm line in amber, until the page has shown the offer once.
                let hint = text.fit(Face::Body, 28, "Busy core? Press START", rw - rkey);
                text.draw(c, Face::Body, 28, rx + rkey, ry - 6, pal().amber, &hint);
                ry += row();
            }
            if d.port_update {
                // Quiet: no colour of its own, and where to act on it.
                let note = text.fit(Face::Body, 28, "Update available, in Extras", rw - rkey);
                text.draw(c, Face::Body, 28, rx + rkey, ry - 6, pal().dust, &note);
                ry += row();
            }
        }
        // A translation or hack applied at the start: the player's copy is not the plain game (`softpatch`).
        if let Some(p) = &d.patch {
            text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Patch");
            // A file name can be long: two lines, as the facts beside it, the second cut short if it has to be.
            let lines = text.wrap(Face::Body, 28, p, rw - rkey);
            for (i, line) in lines.iter().take(2).enumerate() {
                let line = if i == 1 { text.fit(Face::Body, 28, &lines[1..].join(" "), rw - rkey) } else { text.fit(Face::Body, 28, line, rw - rkey) };
                text.draw(c, Face::Body, 28, rx + rkey, ry + i as i32 * tight(), pal().chalk, &line);
            }
            ry += row() + (lines.len().min(2) as i32 - 1).max(0) * tight();
        }
        if let Some(a) = &d.achievements {
            text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Unlocked");
            let a = text.fit(Face::Body, 28, &format!("{a} achievements"), rw - rkey);
            text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, &a);
            ry += row();
        }
        text.draw(c, Face::Body, 28, rx, ry, pal().dust, "Note");
        if d.note.is_empty() {
            text.draw(c, Face::Body, 28, rx + rkey, ry, pal().dust, "None. Press X");
            ry += row();
        } else {
            for line in text.wrap(Face::Body, 28, &format!("“{}”", d.note), rw - rkey).iter().take(3) {
                text.draw(c, Face::Body, 28, rx + rkey, ry, pal().chalk, line);
                ry += tight();
            }
            ry += row() - tight();
        }
        let mut y = ly.max(ry) + 30;

        // The game running and its title screen, when the collection has them (Settings > Games > Extras).
        if !self.pictures.is_empty() {
            let bw = picture_box(c.w).0;
            let mut tallest = 0;
            for (i, p) in self.pictures.iter().enumerate() {
                let x = MARGIN + i as i32 * (bw + 24);
                text.draw(c, Face::Body, 28, x, y, pal().dust, p.label);
                match &p.image {
                    Some(img) => c.blit_scaled(img, x, y + 18, img.w as i32, img.h as i32),
                    None => c.rect(x, y + 18, p.size.0, p.size.1, pal().spine),
                }
                tallest = tallest.max(p.image.as_ref().map_or(p.size.1, |i| i.h as i32));
            }
            y += 18 + tallest + 30;
        }

        // Curator's rows: Echoes, Versions, Stickers, each a spine with its one line; the focused one is the label.
        // "Hide or delete" is a row like them, but the last thing on the card (below).
        let mut at = vec![(0, 0); d.rows.len()];
        let mut curator_rows = false;
        for (i, row) in d.rows.iter().enumerate().filter(|(_, r)| r.kind != RowKind::Manage) {
            at[i] = (y - y0, spine_h());
            self.paint_row(c, text, y, row, self.focus == Some(i));
            y += spine_h() + 10;
            curator_rows = true;
        }
        if curator_rows {
            y += 30;
        }

        // About: the Wikipedia lead when there is one, else a sentence made of the facts.
        let about: Option<(String, Option<String>, u32)> = match &d.summary {
            Summary::Wikipedia { text: t, title } => Some((t.clone(), Some(format!("Text: Wikipedia, “{title}”, CC BY-SA 4.0")), pal().chalk)),
            Summary::Loading => Some((
                d.sentence.clone().unwrap_or_default(),
                Some("Loading the description from Wikipedia…".into()),
                pal().chalk,
            )),
            Summary::Unavailable(why) => Some((
                d.sentence.clone().unwrap_or_default(),
                Some(format!("No description from Wikipedia: {why}")),
                pal().chalk,
            )),
            Summary::Gamelist(t) => Some((t.clone(), Some("From your EmulationStation gamelist".into()), pal().chalk)),
            Summary::None => d.sentence.clone().map(|s| (s, None, pal().chalk)),
        };
        if let Some((body, credit, color)) = about {
            // A heading in Title 37 like the console page's; the room for its taller capitals is taken from under it,
            // so the card is as long as with the small headings before.
            y += 8;
            text.draw(c, Face::Title, 37, MARGIN, y, pal().dust, "Description");
            y += row() - 8;
            for line in text.wrap(Face::Body, 28, &body, full_w - 20) {
                text.draw(c, Face::Body, 28, MARGIN, y, color, &line);
                y += line_step();
            }
            if let Some(credit) = credit {
                let credit = text.fit(Face::Body, 21, &credit, full_w);
                text.draw(c, Face::Body, 21, MARGIN, y + 4, pal().dust, &credit);
                y += line_step();
            }
            y += 30;
        }

        // The rest of the player's library by the same developer, and in the same series.
        for (heading, games) in [
            d.more_by.as_ref().map(|(dev, g)| (format!("More by {dev}"), g)),
            d.series.as_ref().map(|(s, g)| (format!("Other games in {s}"), g)),
        ]
        .into_iter()
        .flatten()
        {
            let heading = text.fit(Face::Title, 37, &format!("{heading}, in your library"), full_w);
            y += 12;
            text.draw(c, Face::Title, 37, MARGIN, y, pal().dust, &heading);
            y += 22;
            for g in games {
                c.rect(MARGIN, y, full_w, spine_h(), pal().spine);
                c.rect(MARGIN, y, STRIPE, spine_h(), g.stripe);
                let side = match g.year {
                    Some(year) => format!("{} · {year}", g.system),
                    None => g.system.clone(),
                };
                let side_w = text.width(Face::Body, 28, &side);
                text.draw_right(c, Face::Body, 28, MARGIN + full_w - 28, y + spine_h() / 2 + type_scale::base(10, 28), pal().dust, &side);
                let name = text.fit(Face::Title, 37, &g.name, full_w - STRIPE - 56 - side_w - 32);
                text.draw(c, Face::Title, 37, MARGIN + STRIPE + 28, y + spine_h() / 2 + type_scale::base(13, 37), pal().chalk, &name);
                y += spine_h() + 10;
            }
            y += 22;
        }
        let credit = match (d.matched, d.from_gamelist) {
            (true, true) => Some("Facts: Wikidata, CC0, and your EmulationStation gamelist"),
            (true, false) => Some("Facts: Wikidata, CC0"),
            (false, true) => Some("Facts: your EmulationStation gamelist"),
            (false, false) => None,
        };
        if let Some(credit) = credit {
            let credit = text.fit(Face::Body, 21, credit, full_w);
            text.draw(c, Face::Body, 21, MARGIN, y, pal().dust, &credit);
            y += line_step();
        }
        // "Hide or delete", under everything: found by scrolling to the end, never in the way of reading.
        for (i, row) in d.rows.iter().enumerate().filter(|(_, r)| r.kind == RowKind::Manage) {
            y += 20;
            at[i] = (y - y0, spine_h());
            self.paint_row(c, text, y, row, self.focus == Some(i));
            y += spine_h() + 10;
        }
        *self.row_at.borrow_mut() = at;
        y - y0
    }

    /// A row of the card (Curator's, "Hide or delete"): a spine with its title and one line; the focused one is the label.
    fn paint_row(&self, c: &mut Canvas, text: &mut Text, y: i32, row: &CuratorRow, focused: bool) {
        let d = &self.data;
        let (full_w, h) = (c.w - 2 * MARGIN, spine_h());
        c.rect(MARGIN, y, full_w, h, if focused { pal().label } else { pal().spine });
        if focused {
            lift_shadow(c, MARGIN, y + h, full_w, pal().t);
        }
        c.rect(MARGIN, y, STRIPE, h, if focused { d.stripe } else { mix(d.stripe, pal().spine, 150) });
        let (fg, dim) = if focused { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
        let title_w = text.draw(c, Face::Title, 37, MARGIN + STRIPE + 28, y + h / 2 + type_scale::base(13, 37), fg, &row.title);
        let lx = MARGIN + STRIPE + 28 + title_w.max(170) + 28;
        let line = text.fit(Face::Body, 28, &row.line, MARGIN + full_w - 28 - lx);
        text.draw(c, Face::Body, 28, lx, y + h / 2 + type_scale::base(10, 28), dim, &line);
    }

    /// "Hide or delete": two choices on a sheet over the dimmed card, as the status picker.
    fn paint_manage(&self, c: &mut Canvas, text: &mut Text, sel: usize) {
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        // Two lines in a row: a title and its line under it, each grown with the text size (`type_scale`).
        let (w, gap) = (type_scale::wide(820, 37).min(c.w - 2 * MARGIN - 96), 10);
        let row = 96 + type_scale::grow(37) + type_scale::grow(26);
        let x = (c.w - w) / 2;
        let total = 2 * (row + gap) - gap;
        let top = (c.h - 100 - total) / 2 + 50;
        sheet(c, x - 48, top - 110, w + 96, total + 158);
        let title = text.fit(Face::Title, 50, &self.data.title, w);
        text.draw(c, Face::Title, 50, x, top - 32, pal().chalk, &title);
        let choices = [
            ("Hide from the shelf", "It stays on the SD card. Settings › System › Storage brings it back."),
            ("Delete from the SD card…", "Shows what it frees and asks again."),
        ];
        for (i, (name, meta)) in choices.iter().enumerate() {
            let y = top + i as i32 * (row + gap);
            let selected = i == sel;
            crate::kit::shelf_row(c, x, y, w, row, selected);
            c.rect(x, y, STRIPE, row, if selected { self.data.stripe } else { mix(self.data.stripe, pal().spine, 150) });
            let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
            let name = text.fit(Face::Title, 37, name, w - STRIPE - 56);
            text.draw(c, Face::Title, 37, x + STRIPE + 28, y + 42 + type_scale::grow(37), fg, &name);
            let meta = text.fit(Face::Body, 26, meta, w - STRIPE - 56);
            text.draw(c, Face::Body, 26, x + STRIPE + 28, y + 78 + type_scale::grow(37) + type_scale::grow(26), dim, &meta);
        }
        c.rect(0, c.h - 100, c.w, 100, pal().void);
        crate::setup::hints(c, text, &self.hints());
    }

    /// "Delete from the card?": what goes and what it frees, saves and states kept or not, then Delete (twice).
    fn paint_delete(&self, c: &mut Canvas, text: &mut Text, d: &DeleteSheet) {
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        let (w, row, gap) = (type_scale::wide(900, 28).min(c.w - 2 * MARGIN - 96), type_scale::row(80, 37), 10);
        let step = type_scale::line(28, 12);
        let x = (c.w - w) / 2;
        let mut say: Vec<(String, u32)> = Vec::new();
        match &d.info {
            None => say.push(("Working out what it takes…".into(), pal().chalk)),
            Some(Err(why)) => say.push((why.clone(), pal().alert)),
            Some(Ok(i)) => {
                let freed = i.game_bytes + if d.with_saves { i.kept_bytes } else { 0 };
                let files = i.files + if d.with_saves { i.kept_files } else { 0 };
                say.push((format!("Frees {}: {} files.", crate::storage::size(freed), files), pal().chalk));
                let more = i.files.saturating_sub(i.names.len());
                let names = if more > 0 { format!("{} and {more} more", i.names.join(", ")) } else { i.names.join(", ") };
                say.push((names, pal().dust));
                if d.with_saves && d.synced {
                    say.push(("Sync shares saves and states: they are deleted from your other devices too.".into(), pal().alert));
                }
            }
        }
        // A line too long for the sheet (German, Large text) goes on over a second one, never cut.
        let say: Vec<(String, u32)> = say.into_iter().flat_map(|(line, color)| text.wrap(Face::Body, 28, &line, w).into_iter().take(2).map(move |l| (l, color))).collect();
        // The two rows only once there is something to delete; until then (or when it can't be) only the words.
        let ready = matches!(d.info, Some(Ok(_)));
        let (lines, rows) = (say.len() as i32, if ready { 2 } else { 0 });
        let total = lines * step + if ready { 24 + rows * (row + gap) - gap } else { 0 };
        let top = (c.h - 100 - total) / 2 + 50;
        sheet(c, x - 48, top - 110, w + 96, total + 158);
        let head = text.fit(Face::Title, 50, &format!("Delete {}?", self.data.title), w);
        text.draw(c, Face::Title, 50, x, top - 32, pal().chalk, &head);
        for (k, (line, color)) in say.iter().enumerate() {
            let line = text.fit(Face::Body, 28, line, w);
            text.draw(c, Face::Body, 28, x, top + 20 + type_scale::grow(28) + k as i32 * step, *color, &line);
        }
        if !ready {
            c.rect(0, c.h - 100, c.w, 100, pal().void);
            crate::setup::hints(c, text, &self.hints());
            return;
        }
        let y0 = top + lines * step + 24;
        let saves = match &d.info {
            Some(Ok(i)) if i.kept_files == 0 => "None found".to_string(),
            Some(Ok(i)) if d.with_saves => format!("‹ Delete too, {} ›", crate::storage::size(i.kept_bytes)),
            Some(Ok(i)) => format!("‹ Keep, {} ›", crate::storage::size(i.kept_bytes)),
            _ => String::new(),
        };
        let delete = match &d.info {
            Some(Ok(i)) => format!("Delete {}", crate::storage::size(i.game_bytes + if d.with_saves { i.kept_bytes } else { 0 })),
            _ => "Delete".to_string(),
        };
        for (k, (name, value)) in [("Saves and states", saves), (delete.as_str(), String::new())].iter().enumerate() {
            let y = y0 + k as i32 * (row + gap);
            let selected = d.sel == k;
            let armed = selected && d.armed && k == 1;
            let (bg, fg, dim) = if armed {
                (pal().alert, pal().on_alert, pal().on_alert)
            } else if selected {
                (pal().label, pal().ink, pal().label_dust)
            } else {
                (pal().spine, pal().chalk, pal().dust)
            };
            c.rect(x, y, w, row, bg);
            if !armed {
                c.rect(x, y, STRIPE, row, if selected { self.data.stripe } else { mix(self.data.stripe, pal().spine, 150) });
            }
            let name = if armed { "Press A again to delete. B keeps it." } else if d.busy && k == 1 { "Deleting…" } else { name };
            let (face, size, below) = if armed { (Face::Body, 28, type_scale::base(10, 28)) } else { (Face::Title, 37, type_scale::base(13, 37)) };
            let value_w = if value.is_empty() { 0 } else { text.width(Face::Body, 28, value) + 32 };
            let name = text.fit(face, size, name, w - STRIPE - 56 - value_w);
            text.draw(c, face, size, x + STRIPE + 28, y + row / 2 + below, fg, &name);
            if !value.is_empty() {
                text.draw_right(c, Face::Body, 28, x + w - 28, y + row / 2 + type_scale::base(10, 28), dim, value);
            }
        }
        c.rect(0, c.h - 100, c.w, 100, pal().void);
        crate::setup::hints(c, text, &self.hints());
    }

    /// The status choices over the dimmed card, as a short shelf.
    fn paint_picker(&self, c: &mut Canvas, text: &mut Text, sel: usize, stars: u8) {
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        let (w, row, gap) = (type_scale::wide(620, 37), type_scale::row(72, 37), 8);
        let x = (c.w - w) / 2;
        // The five statuses, the rating a little apart, then quieter, "Wrong game? Choose another" and "Wrong pictures?
        // Look up again".
        let (extra_gap, extra_h) = (18, type_scale::row(60, 28));
        let quiet = if self.data.look_up { 2 } else { 1 };
        let total = (Status::ALL.len() as i32 + 1) * (row + gap) - gap + 2 * extra_gap + quiet * extra_h;
        let top = (c.h - 100 - total) / 2 + 60;
        // A black sheet under the heading and the rows, so nothing of the card runs through them.
        sheet(c, x - 48, top - 120, w + 96, total + 168);
        text.draw(c, Face::Title, 50, x, top - 36, pal().chalk, "Status and rating");
        for (i, s) in Status::ALL.iter().enumerate() {
            let y = top + i as i32 * (row + gap);
            let selected = i == sel;
            crate::kit::shelf_row(c, x, y, w, row, selected);
            c.rect(x, y, STRIPE, row, if selected { self.data.stripe } else { mix(self.data.stripe, pal().spine, 150) });
            let fg = if selected { pal().ink } else { pal().chalk };
            paint_mark(c, x + STRIPE + 40, y + row / 2, *s, fg);
            text.draw(c, Face::Title, 37, x + STRIPE + 76, y + row / 2 + type_scale::base(13, 37), fg, s.label());
            let note = if self.data.suggestion == Some(*s) {
                "suggested"
            } else if self.data.status == *s {
                "now"
            } else {
                ""
            };
            if !note.is_empty() {
                text.draw_right(c, Face::Body, 28, x + w - 28, y + row / 2 + type_scale::base(10, 28), if selected { pal().label_dust } else { pal().dust }, note);
            }
        }
        // The rating: a spine like the statuses, its stars at the right end; left and right change them.
        let y = top + Status::ALL.len() as i32 * (row + gap) - gap + extra_gap;
        let selected = sel == RATING_ROW;
        crate::kit::shelf_row(c, x, y, w, row, selected);
        c.rect(x, y, STRIPE, row, if selected { self.data.stripe } else { mix(self.data.stripe, pal().spine, 150) });
        let (fg, dim, back) = if selected { (pal().ink, pal().label_dust, pal().label) } else { (pal().chalk, pal().dust, pal().spine) };
        // A star where the statuses have their marks.
        crate::switcher::star(c, x + STRIPE + 40, y + row / 2, type_scale::wide(12, 37), fg);
        let label_end = x + STRIPE + 76 + text.draw(c, Face::Title, 37, x + STRIPE + 76, y + row / 2 + type_scale::base(13, 37), fg, "My rating");
        let star_r = type_scale::wide(14, 37);
        let stars_x = x + w - 28 - (RATING_MAX as i32 * 2 * star_r + (RATING_MAX as i32 - 1) * 8) + star_r;
        paint_stars(c, stars_x, y + row / 2, star_r, stars, fg, mix(dim, back, 110));
        // No stars yet: said in words before them, where the words fit beside the name (in German they don't; the dim
        // stars and the hint bar's "Stars" say it there).
        let none_end = stars_x - star_r - 20;
        if stars == 0 && none_end - text.width(Face::Body, 28, "Not rated") > label_end + 24 {
            text.draw_right(c, Face::Body, 28, none_end, y + row / 2 + type_scale::base(10, 28), dim, "Not rated");
        }
        let y = y + row + extra_gap;
        let selected = sel == CHOOSE_ROW;
        if selected {
            c.rect(x, y, w, extra_h, pal().label);
        }
        let fg = if selected { pal().ink } else { pal().dust };
        text.draw(c, Face::Body, 28, x + STRIPE + 64, y + extra_h / 2 + type_scale::base(10, 28), fg, "Wrong game? Choose another");
        if self.data.look_up {
            let y = y + extra_h;
            let selected = sel == LOOK_UP_ROW;
            if selected {
                c.rect(x, y, w, extra_h, pal().label);
            }
            let fg = if selected { pal().ink } else { pal().dust };
            text.draw(c, Face::Body, 28, x + STRIPE + 64, y + extra_h / 2 + type_scale::base(10, 28), fg, "Wrong pictures? Look up again");
        }
        c.rect(0, c.h - 100, c.w, 100, pal().void);
        crate::setup::hints(c, text, &self.hints());
    }

    /// "Which game is this?": the closest entries of the system as a short shelf, then "None of these".
    fn paint_chooser(&self, c: &mut Canvas, text: &mut Text, sel: usize, rows: &[ChoiceRow]) {
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        let (w, row, gap) = (900, type_scale::row(84, 37), 8);
        let x = (c.w - w) / 2;
        let total = rows.len() as i32 * (row + gap) - gap;
        let top = (c.h - 100 - total) / 2 + 50;
        sheet(c, x - 48, top - 110, w + 96, total + 158);
        text.draw(c, Face::Title, 50, x, top - 32, pal().chalk, "Which game is this?");
        for (i, r) in rows.iter().enumerate() {
            let y = top + i as i32 * (row + gap);
            let selected = i == sel;
            crate::kit::shelf_row(c, x, y, w, row, selected);
            c.rect(x, y, STRIPE, row, if selected { self.data.stripe } else { mix(self.data.stripe, pal().spine, 150) });
            let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
            let side = if r.current { "now" } else { "" };
            let side_w = if side.is_empty() { 0 } else { text.width(Face::Body, 28, side) + 32 };
            let tw = w - STRIPE - 56 - side_w;
            if r.meta.is_empty() {
                let title = text.fit(Face::Title, 37, &r.title, tw);
                text.draw(c, Face::Title, 37, x + STRIPE + 28, y + row / 2 + type_scale::base(13, 37), fg, &title);
            } else {
                let title = text.fit(Face::Title, 37, &r.title, tw);
                text.draw(c, Face::Title, 37, x + STRIPE + 28, y + 38 + type_scale::grow(37), fg, &title);
                let meta = text.fit(Face::Body, 26, &r.meta, tw);
                text.draw(c, Face::Body, 26, x + STRIPE + 28, y + 71 + type_scale::grow(37) + type_scale::grow(26), dim, &meta);
            }
            if !side.is_empty() {
                text.draw_right(c, Face::Body, 28, x + w - 28, y + row / 2 + type_scale::base(10, 28), dim, side);
            }
        }
        c.rect(0, c.h - 100, c.w, 100, pal().void);
        crate::setup::hints(c, text, &self.hints());
    }
}

/// Where the card puts the box art: fit the cover into this.
pub fn cover_box() -> (i32, i32) {
    (300, LABEL_H - 24)
}

/// A small sign for a status, centred on (cx, cy), about 20 px: a dot for Playing, two bars for
/// Paused, a tick for Completed, a dash for Dropped, a thin ring for Not started.
/// The sheet under a picker: the page colour; on the light page with a hairline edge and a shadow, since the
/// washed-out card behind it is nearly the same colour.
fn sheet(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    crate::kit::sheet(c, x, y, w, h);
}

/// On the light page, a paper label lifts off the shelf: a soft shadow 6 px deep under its bottom edge (x, y, w).
/// `strength` 0 draws nothing (the dark page, where the label stands out on its own), 255 the full shadow.
pub fn lift_shadow(c: &mut Canvas, x: i32, y: i32, w: i32, strength: u8) {
    if strength == 0 {
        return;
    }
    for k in 0..6 {
        c.shade_rect(x + 4, y + k, w - 4, 1, 0, ((6 - k) as u32 * 6 * strength as u32 / 255) as u8);
    }
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
        crate::switcher::star(c, cx + k * (2 * r + 8), cy, r, colour);
    }
}

/// The sentence the card shows when no description could be loaded: "A 1992 platform game by Nintendo R&D1."
pub fn sentence(info: &crate::gamedb::GameInfo) -> Option<String> {
    let genre = info.genres.first()?;
    let genre = genre.strip_suffix(" video game").unwrap_or(genre);
    let genre = if genre.ends_with("game") {
        genre.to_string()
    } else {
        format!("{genre} game")
    };
    let article = if info.year.is_none() && genre.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "An"
    } else {
        "A"
    };
    let mut s = match info.year {
        Some(y) => format!("{article} {y} {genre}"),
        None => format!("{article} {genre}"),
    };
    if let Some(dev) = info.developers.first() {
        s.push_str(&format!(" by {dev}"));
    }
    if let Some(series) = info.series.first() {
        s.push_str(&format!(", part of the {series} series"));
    }
    s.push('.');
    Some(s)
}

/// The card's Battery line in two: the time left, and the rate or whose measurement it is
/// ("About 3 h 20 min left, 18 % an hour" → "About 3 h 20 min left" + "18 % an hour"). None when it is one piece.
fn battery_split(line: &str) -> Option<(&str, &str)> {
    if let Some((time, rate)) = line.split_once(", ") {
        return Some((time, rate));
    }
    line.find(" (").map(|i| (&line[..i], &line[i + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> CardData {
        CardData {
            title: "Tetris".into(),
            system: "Game Boy".into(),
            stripe: 0x8a9a3b,
            matched: true,
            listed_as: None,
            year: Some(1989),
            players: Some("1–2 players".into()),
            facts: vec![("Developer", "Nintendo R&D1".into())],
            scores: None,
            sentence: None,
            summary: Summary::None,
            played: None,
            sessions: 2,
            status: Status::NotStarted,
            suggestion: Some(Status::Playing),
            rating: 0,
            note: String::new(),
            more_by: None,
            series: None,
            rows: Vec::new(),
            achievements: None,
            battery: None,
            port: None,
            port_update: false,
            surprise: None,
            look_up: false,
            from_gamelist: false,
            patch: None,
            favorite: false,
        }
    }

    #[test]
    fn down_moves_onto_the_curator_rows_and_a_opens_them() {
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        let mut d = data();
        d.rows = vec![
            CuratorRow { kind: RowKind::Echoes, title: "Echoes".into(), line: "Burnout 3 · Need for Speed · R4".into() },
            CuratorRow { kind: RowKind::Stickers, title: "Stickers".into(), line: "Couch co-op".into() },
        ];
        let mut card = Card::new(d);
        assert_eq!(card.press(Button::Down), Out::None, "before a paint nothing is known to be in view: a scroll");
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, None);
        card.press(Button::Down);
        assert_eq!(card.focus, Some(0), "Echoes is in view under Your game");
        assert_eq!(card.hints()[0], ("A", "Open"));
        assert_eq!(card.press(Button::East), Out::Row(RowKind::Echoes));
        card.press(Button::Down);
        assert_eq!(card.press(Button::East), Out::Row(RowKind::Stickers));
        card.press(Button::Up);
        card.press(Button::Up);
        assert_eq!(card.focus, None);
        assert_eq!(card.press(Button::East), Out::Play, "back to Play");
        assert_eq!(card.hints()[0], ("A", "Play"));
        // L1/R1 page through the card and leave the rows.
        card.press(Button::Down);
        card.press(Button::R1);
        assert_eq!(card.focus, None);
    }

    fn manage_row() -> CuratorRow {
        CuratorRow { kind: RowKind::Manage, title: "Hide or delete".into(), line: "Take it off the shelf, or off the SD card".into() }
    }

    /// "Hide or delete" is under the description: Down reads the card to its end first, then lands on it; Up from it
    /// goes back into the text, not straight to Echoes.
    #[test]
    fn the_hide_or_delete_row_is_reached_by_reading_to_the_end() {
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        let mut d = data();
        d.summary = Summary::Wikipedia { text: "A long description. ".repeat(80), title: "Tetris".into() };
        d.rows = vec![CuratorRow { kind: RowKind::Echoes, title: "Echoes".into(), line: "Dr. Mario".into() }, manage_row()];
        let mut card = Card::new(d);
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, None);
        assert!(card.max_scroll > 400, "a long card");
        card.press(Button::Down);
        assert_eq!(card.focus, Some(0), "Echoes first");
        let mut presses = 0;
        while card.focus != Some(1) && presses < 200 {
            card.press(Button::Down);
            card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, None);
            presses += 1;
        }
        assert_eq!(card.focus, Some(1), "Hide or delete at the end");
        assert!(presses > 3, "the description was scrolled through, not jumped over ({presses} presses)");
        assert_eq!(card.press(Button::East), Out::Row(RowKind::Manage));
        card.press(Button::Up);
        assert_eq!(card.focus, None, "up from it scrolls back into the text");
    }

    #[test]
    fn the_info_page_scrolls_its_description_and_select_goes_to_the_full_page_and_back() {
        let (w, h) = (1280, 960);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        let mut d = data();
        d.summary = Summary::Wikipedia { text: "A long description. ".repeat(80), title: "Tetris".into() };
        d.rows = vec![manage_row()];
        let mut card = Card::new(d);
        card.show_info();
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, None);
        assert!(card.info.as_ref().is_some_and(|i| i.scrollable()), "a long description scrolls");
        assert_eq!(card.press(Button::Down), Out::None);
        assert!(card.info.as_ref().is_some_and(|i| i.scroll > 0) && card.focus.is_none(), "down scrolls the text, no row");
        assert_eq!(card.press(Button::Right), Out::None, "nothing to turn over");
        assert_eq!(card.press(Button::Select), Out::None, "the full page, the box preference untouched");
        assert!(card.info.is_none() && card.boxed.is_none());
        assert_eq!(card.press(Button::Select), Out::None);
        assert!(card.info.is_some() && card.boxed.is_none(), "SELECT comes back to the info page, never the box");
        assert_eq!(card.press(Button::East), Out::Play);
        assert_eq!(card.press(Button::South), Out::Back);
        assert!(card.hints().iter().any(|(k, l)| *k == "SELECT" && *l == "Details"));
    }

    #[test]
    fn hide_is_one_choice_and_delete_asks_twice_keeping_saves_unless_changed() {
        let mut card = Card::new(data());
        card.mode = Mode::Manage { sel: 0 };
        assert_eq!(card.press(Button::East), Out::Hide);
        assert!(matches!(card.mode, Mode::View));

        card.mode = Mode::Manage { sel: 0 };
        card.press(Button::Down);
        assert_eq!(card.press(Button::East), Out::PlanDelete);
        assert_eq!(card.press(Button::East), Out::None, "nothing to delete before it is known");
        card.delete_info(Ok(DeleteInfo { game_bytes: 4_000_000_000, files: 3, names: vec!["Racer.cue".into(), "Racer.bin".into()], kept_files: 2, kept_bytes: 9_000 }), true);
        assert_eq!(card.hints()[0], ("A", "Delete"));
        assert_eq!(card.press(Button::East), Out::None, "the first A arms it");
        assert_eq!(card.hints()[0], ("A", "Delete now"));
        assert_eq!(card.press(Button::South), Out::None, "B takes it back");
        assert!(matches!(&card.mode, Mode::Delete(d) if !d.armed));
        card.press(Button::Up);
        card.press(Button::East);
        assert!(matches!(&card.mode, Mode::Delete(d) if d.with_saves), "A on Saves and states changes it");
        card.press(Button::Left);
        card.press(Button::Down);
        card.press(Button::East);
        assert_eq!(card.press(Button::East), Out::Delete { with_saves: false }, "saves kept: the default, and back to it");
        assert_eq!(card.press(Button::South), Out::None, "nothing while it deletes");
        card.delete_failed("Racer.bin could not be deleted (read-only).".into());
        assert!(matches!(&card.mode, Mode::Delete(d) if !d.busy && matches!(d.info, Some(Err(_)))));
        assert_eq!(card.press(Button::East), Out::None, "an error: no delete");
        card.press(Button::South);
        assert!(matches!(card.mode, Mode::View));
    }

    #[test]
    fn a_game_without_saves_or_states_has_no_choice_about_them() {
        let mut card = Card::new(data());
        card.mode = Mode::Delete(DeleteSheet { info: None, with_saves: false, sel: 1, armed: false, synced: false, busy: false });
        card.delete_info(Ok(DeleteInfo { game_bytes: 1, files: 1, names: vec!["A.gba".into()], kept_files: 0, kept_bytes: 0 }), false);
        card.press(Button::Up);
        assert!(matches!(&card.mode, Mode::Delete(d) if d.sel == 1), "stays on Delete");
        card.press(Button::East);
        assert_eq!(card.press(Button::East), Out::Delete { with_saves: false });
    }

    /// VDEV-ROUND-2026-10-25: in Large, "Über das Spiel" at the top of the body lost the dots of its Ü. A capital with
    /// marks, painted where the first headings go, stays inside the body in both sizes.
    #[test]
    fn the_first_headings_keep_their_marks_in_large_text() {
        let mut text = Text::new();
        for size in [type_scale::TextSize::Normal, type_scale::TextSize::Large] {
            type_scale::set(size);
            // How far a capital with marks rises above its baseline, painted with room to spare.
            let (w, h) = (240usize, 160usize);
            let mut px = vec![0u32; w * h];
            text.draw(&mut Canvas::new(&mut px, w as u32, h as u32), Face::Title, 37, 10, 120, 0xffffff, "ÜÄÖ");
            let first = (0..h).find(|&y| px[y * w..(y + 1) * w].iter().any(|&p| p != 0)).expect("ink");
            let rise = 120 - first as i32;
            // The body starts at row 0; painting above it is lost.
            assert!(heading_baseline(0) >= rise, "{size:?}: the marks rise {rise} px over a baseline {} px below the body's top", heading_baseline(0));
        }
        type_scale::set(type_scale::TextSize::Normal);
    }

    /// The card's body is as long as its content: a picture adds its heading and room, no picture adds nothing.
    #[test]
    fn pictures_add_a_row_under_the_facts_and_none_adds_no_gap() {
        let (w, h) = (1240, 1080);
        let mut text = Text::new();
        let scroll_of = |pictures: Vec<CardPicture>, text: &mut Text| {
            let mut px = vec![0u32; (w * h) as usize];
            let mut card = Card::new(data());
            card.pictures = pictures;
            card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), text, None, None);
            (card.max_scroll, px)
        };
        let (plain, plain_px) = scroll_of(Vec::new(), &mut text);
        let tile = |px: &[u32]| px.iter().filter(|p| **p == pal().spine).count();
        let (bw, bh) = picture_box(w);
        let shot = |label, image| CardPicture { label, size: crate::picture_loader::fitted(256, 224, bw, bh), image };
        let (placeholder, tile_px) = scroll_of(vec![shot("In the game", None)], &mut text);
        assert!(tile(&tile_px) > tile(&plain_px) + 20_000, "a plain tile of the picture's size stands in until it is decoded");
        // Decoded, the place is the one the header promised: nothing moves when it arrives.
        let (fw, fh) = crate::picture_loader::fitted(256, 224, bw, bh);
        let img = std::rc::Rc::new(Image { w: fw as u32, h: fh as u32, px: vec![0x336699; (fw * fh) as usize] });
        let (decoded, px) = scroll_of(vec![shot("In the game", Some(img.clone()))], &mut text);
        assert_eq!(decoded, placeholder, "the same height with the picture and without");
        // The picture is on the card (a card of this size shows its first body rows without scrolling).
        assert!(px.contains(&0x336699), "the screenshot is painted");
        let (none, px) = scroll_of(Vec::new(), &mut text);
        assert_eq!(none, plain, "no pictures, no gap");
        assert!(!px.contains(&0x336699));
    }

    #[test]
    fn a_long_battery_line_parts_where_it_reads_well_and_a_port_offers_its_performance() {
        assert_eq!(battery_split("About 3 h 20 min left, 18% an hour"), Some(("About 3 h 20 min left", "18% an hour")));
        assert_eq!(battery_split("About 4 h left (Game Boy Color games)"), Some(("About 4 h left", "(Game Boy Color games)")));
        assert_eq!(battery_split("Measuring this game\u{2026}"), None);
        // START: the emulator for a game, Performance & fan for a port.
        assert!(Card::new(data()).hints().contains(&("START", "This game")));
        let mut d = data();
        d.port = Some(("Auto, fan Auto".into(), true));
        d.battery = Some("About 3 h 20 min left, 18% an hour".into());
        let mut card = Card::new(d);
        assert!(card.hints().contains(&("START", "Performance & fan")));
        // Both lines are painted (and measured) without a fuss.
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut Text::new(), None, None);
    }

    #[test]
    fn the_status_picker_starts_on_the_suggestion_and_reports_the_choice() {
        let mut card = Card::new(data());
        assert_eq!(card.press(Button::West), Out::None);
        assert!(matches!(card.mode, Mode::Status { sel: 1, .. }), "Playing, the suggestion, is highlighted");
        card.press(Button::Down);
        assert_eq!(card.press(Button::East), Out::SetStatus(Status::Paused));
        assert_eq!((card.data.status, card.data.suggestion), (Status::Paused, None));
        // B leaves the picker without a change.
        card.press(Button::West);
        card.press(Button::Up);
        assert_eq!(card.press(Button::South), Out::None);
        assert_eq!(card.data.status, Status::Paused);
        assert_eq!(card.press(Button::East), Out::Play);
        assert_eq!(card.press(Button::South), Out::Back);
    }

    #[test]
    fn the_status_picker_ends_with_wrong_game_and_asks_for_the_choices() {
        let mut card = Card::new(data());
        card.press(Button::West);
        for _ in 0..10 {
            card.press(Button::Down);
        }
        assert!(matches!(card.mode, Mode::Status { sel: 6, .. }), "the last row, after Dropped and the rating");
        assert_eq!(card.press(Button::East), Out::ChooseGame);
        assert_eq!(card.data.status, Status::NotStarted, "choosing a game sets no status");
    }

    #[test]
    fn look_up_again_is_the_last_row_for_games_with_pictures_from_the_collection() {
        let mut card = Card::new(CardData { look_up: true, ..data() });
        card.press(Button::West);
        for _ in 0..10 {
            card.press(Button::Down);
        }
        assert!(matches!(card.mode, Mode::Status { sel: LOOK_UP_ROW, .. }), "after Wrong game");
        assert_eq!(card.press(Button::East), Out::LookUpAgain);
        assert!(matches!(card.mode, Mode::View), "the picker closes; the app says how it went");
        assert_eq!(card.data.status, Status::NotStarted);
        card.press(Button::West);
        for _ in 0..10 {
            card.press(Button::Down);
        }
        card.press(Button::Up);
        assert!(matches!(card.mode, Mode::Status { sel: CHOOSE_ROW, .. }));
        // Both rows are painted, also with Large text.
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut Text::new(), None, None);
    }

    #[test]
    fn the_rating_is_a_row_of_the_status_picker_set_with_left_and_right() {
        let mut card = Card::new(data());
        card.press(Button::West);
        // Down from Playing (the suggestion) past Dropped onto the rating.
        for _ in 0..4 {
            card.press(Button::Down);
        }
        assert!(matches!(card.mode, Mode::Status { sel: RATING_ROW, stars: 0 }), "not rated yet");
        assert_eq!(card.hints()[0], ("LEFT RIGHT", "Stars"));
        for _ in 0..7 {
            card.press(Button::Right);
        }
        card.press(Button::Left);
        assert!(matches!(card.mode, Mode::Status { stars: 4, .. }), "five at most, then one back");
        assert!(matches!(card.mode, Mode::Status { sel: RATING_ROW, .. }), "left and right don't leave the row");
        assert_eq!(card.press(Button::East), Out::SetRating(4));
        assert_eq!((card.data.rating, card.data.status), (4, Status::NotStarted), "the status is left as it was");
        // The picker opens on the rating as given; all stars away is "not rated" again.
        card.press(Button::West);
        assert!(matches!(card.mode, Mode::Status { stars: 4, .. }));
        for _ in 0..4 {
            card.press(Button::Down);
        }
        for _ in 0..5 {
            card.press(Button::Left);
        }
        assert_eq!(card.press(Button::East), Out::SetRating(0));
        // B leaves the stars as they were.
        card.press(Button::West);
        for _ in 0..4 {
            card.press(Button::Down);
        }
        card.press(Button::Right);
        assert_eq!(card.press(Button::South), Out::None);
        assert_eq!(card.data.rating, 0);
    }

    #[test]
    fn the_rating_shows_on_the_card_and_none_says_how_to_give_one() {
        let (w, h) = (1240, 1080);
        let mut text = Text::new();
        let paint = |rating: u8, text: &mut Text| {
            let mut d = data();
            d.rating = rating;
            let mut px = vec![0u32; (w * h) as usize];
            Card::new(d).paint(&mut Canvas::new(&mut px, w as u32, h as u32), text, None, None);
            px
        };
        let count = |px: &[u32], colour: u32| px.iter().filter(|p| **p == colour).count();
        let (none, three, five) = (paint(0, &mut text), paint(3, &mut text), paint(5, &mut text));
        let (n0, n3, n5) = (count(&none, pal().chalk), count(&three, pal().chalk), count(&five, pal().chalk));
        assert!(n3 > n0 + 3 * 100, "three stars in the text colour: {n0} {n3}");
        assert!(n5 > n3 + 2 * 100, "five fill more than three: {n3} {n5}");
        // A card without a rating says so in words ("None. Press Y" in dust), never a blank.
        assert!(count(&none, pal().dust) > count(&three, pal().dust) + 200, "the words where the stars would be");
    }

    #[test]
    fn a_surprise_card_says_why_and_right_picks_another() {
        let mut d = data();
        d.surprise = Some("Not played for 3 months · you like platformer games".into());
        let mut card = Card::new(d);
        let hints = card.hints();
        assert_eq!(&hints[..3], &[("A", "Play"), ("B", "Back"), ("RIGHT", "Another")]);
        assert!(!hints.iter().any(|(k, _)| *k == "START"), "the emulator is not offered here: {hints:?}");
        assert_eq!(card.press(Button::Right), Out::Another);
        assert_eq!(card.press(Button::East), Out::Play);
        // Without a surprise Right does nothing.
        assert_eq!(Card::new(data()).press(Button::Right), Out::None);
        // The line takes a row above the columns: the body is that much longer.
        let (w, h) = (1240, 1080);
        let mut text = Text::new();
        let mut long = data();
        long.summary = Summary::Wikipedia { text: "A long description. ".repeat(40), title: "Tetris".into() };
        let scroll = |surprise: Option<String>, text: &mut Text| {
            let mut d = long.clone();
            d.surprise = surprise;
            let mut c = Card::new(d);
            let mut px = vec![0u32; (w * h) as usize];
            c.paint(&mut Canvas::new(&mut px, w as u32, h as u32), text, None, None);
            (c.max_scroll, px.iter().filter(|p| **p == pal().amber).count())
        };
        let (plain, plain_amber) = scroll(None, &mut text);
        let (with, amber) = scroll(Some("Never played".into()), &mut text);
        assert_eq!(with, plain + surprise_h());
        assert!(amber > plain_amber + 100, "the reason is in amber, as the card's other suggestions");
    }

    #[test]
    fn the_chooser_starts_on_the_current_entry_and_reports_the_pick() {
        use crate::gamedb::Choice;
        let row = |t: &str, choice: Choice, current: bool| ChoiceRow {
            title: t.into(),
            meta: String::new(),
            choice,
            current,
        };
        let rows = vec![
            row("Bomberman PSP", Choice::Entry("Q1".into()), false),
            row("Bomberman Land", Choice::Entry("Q2".into()), true),
            row("None of these", Choice::NoEntry, false),
        ];
        let mut card = Card::new(data());
        card.open_chooser(rows.clone());
        assert!(matches!(card.mode, Mode::Choose { sel: 1, .. }), "on the entry shown now");
        card.press(Button::Down);
        card.press(Button::Down);
        assert_eq!(card.press(Button::East), Out::SetChoice(Choice::NoEntry), "the last row is None of these");
        assert!(matches!(card.mode, Mode::View));
        card.open_chooser(rows);
        card.press(Button::Up);
        assert_eq!(card.press(Button::South), Out::None, "B leaves without a change");
        assert!(matches!(card.mode, Mode::View));
    }

    #[test]
    fn the_note_is_typed_on_the_keyboard_and_saved_with_start() {
        let mut card = Card::new(data());
        card.press(Button::North);
        // Row 1 starts at q: "qw".
        card.press(Button::East);
        card.press(Button::Right);
        card.press(Button::East);
        assert_eq!(card.press(Button::Start), Out::SetNote("qw".into()));
        assert_eq!(card.data.note, "qw");
        // Editing starts from the note; B on an empty field leaves without saving.
        card.press(Button::North);
        card.press(Button::South);
        card.press(Button::South);
        assert_eq!(card.press(Button::South), Out::None);
        assert!(matches!(card.mode, Mode::View));
        assert_eq!(card.data.note, "qw");
    }

    #[test]
    fn the_status_picker_darkens_the_whole_card_behind_it() {
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        let mut d = data();
        d.summary = Summary::Wikipedia {
            text: "A long description. ".repeat(40),
            title: "Tetris".into(),
        };
        let mut card = Card::new(d);
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, None);
        card.press(Button::West);
        card.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, None);
        // Outside the picker's sheet (x 262..978) nothing of the card may stay bright, the description included.
        let luma = |p: u32| ((p >> 16 & 0xff) * 3 + (p >> 8 & 0xff) * 6 + (p & 0xff)) / 10;
        let bright = (BODY_TOP..h - BODY_BOTTOM_GAP)
            .flat_map(|y| (0..250).chain(990..w).map(move |x| (x, y)))
            .filter(|&(x, y)| luma(px[(y * w + x) as usize]) > 0x40)
            .count();
        assert_eq!(bright, 0, "bright pixels of the card under the picker");
    }

    #[test]
    fn a_sentence_is_made_of_the_facts() {
        let info = crate::gamedb::GameInfo {
            year: Some(1992),
            genres: vec!["2D platform game".into()],
            developers: vec!["Nintendo R&D1".into()],
            series: vec!["Super Mario Land".into()],
            ..Default::default()
        };
        assert_eq!(
            sentence(&info).as_deref(),
            Some("A 1992 2D platform game by Nintendo R&D1, part of the Super Mario Land series.")
        );
        let info = crate::gamedb::GameInfo {
            genres: vec!["action role-playing video game".into()],
            ..Default::default()
        };
        assert_eq!(sentence(&info).as_deref(), Some("An action role-playing game."));
        assert_eq!(sentence(&Default::default()), None);
    }
}
