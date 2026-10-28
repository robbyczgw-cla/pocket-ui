//! The picture side of a pulled-out Home row that is not one game (docs/DESIGN.md "Home panels"): Curator,
//! Favorites, a smart shelf or sticker pinned to Home, and Surprise me in the Curator list. Pure: what it shows and
//! where each cover goes is decided here from data the app already holds in memory (the index thread's `Today`, the
//! favourites, a shelf's first games); `app_home_panel.rs` paints it from covers already decoded.
//!
//! - **Curator**: the day's pick (its cover, "Today" under it), then a small fan of the games waiting for you ("4 to
//!   rediscover", "12 never played"). The line under the title names the pick and says why.
//! - **Favorites, a smart shelf, a sticker**: the first games' covers in a row, standing on one line as cases on a
//!   shelf do. With no games, words on how to fill it.
//! - A game without box art is a tile in its system's colour with its title. Before Curator's index is there, faint
//!   tiles stand where the covers will be, and the line says "Getting ready…".

use std::path::PathBuf;

use crate::curator::today::Waiting;

/// One game of a panel: what its cover is looked up by, and its title for a game without one.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub title: String,
    pub system: String,
    pub rom: PathBuf,
}

/// Covers that belong together, with a caption under them (Curator's) or none (a list's).
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub caption: Option<String>,
    pub tiles: Vec<Tile>,
    /// Shown as a fan, each cover a little behind the one before (the waiting games); else side by side.
    pub fan: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    /// A short line over the title, as "Continue" is over the last game's: what the row is.
    pub kicker: Option<&'static str>,
    /// The line under the title.
    pub line: String,
    /// Left to right.
    pub groups: Vec<Group>,
    /// Not ready yet: faint tiles where covers will be.
    pub getting_ready: bool,
}

/// What Curator's index offers today, with the games as tiles (`curator::today::Today` resolved by the app).
#[derive(Clone, Debug, PartialEq)]
pub struct CuratorToday {
    /// The day's pick and its reason.
    pub pick: Option<(Tile, String)>,
    /// Which games wait, how many, and the first few.
    pub waiting: Option<(Waiting, usize, Vec<Tile>)>,
}

/// The row's kicker on Home: what Curator is, in four words.
pub const CURATOR_KICKER: &str = "Your library by feel";
/// Covers a list's panel shows at most.
pub const MAX_COVERS: usize = 5;

/// The Curator row: `None` while the index is being made.
pub fn curator(today: Option<CuratorToday>) -> Panel {
    let Some(today) = today else {
        return Panel { kicker: Some(CURATOR_KICKER), line: "Getting ready…".into(), groups: Vec::new(), getting_ready: true };
    };
    let mut groups = Vec::new();
    let line = match &today.pick {
        Some((tile, reason)) => {
            groups.push(Group { caption: Some("Today".into()), tiles: vec![tile.clone()], fan: false });
            format!("Today: {} · {}", tile.title, reason)
        }
        // No games: Home has no shelf to speak of; the row still says what it is.
        None => "Lens, smart shelves and stickers".into(),
    };
    if let Some((kind, n, tiles)) = today.waiting.filter(|(_, n, tiles)| *n > 0 && !tiles.is_empty()) {
        let caption = match kind {
            Waiting::Rediscover => format!("{n} to rediscover"),
            Waiting::NotStarted => format!("{n} never played"),
        };
        groups.push(Group { caption: Some(caption), tiles, fan: true });
    }
    Panel { kicker: Some(CURATOR_KICKER), line, groups, getting_ready: false }
}

/// Surprise me in the Curator list: the day's pick, the same game A shows first. `None` while the index is made or
/// with no games: the entry's own line stays.
pub fn surprise(pick: Option<(Tile, String)>, otherwise: String) -> Panel {
    match pick {
        Some((tile, reason)) => Panel {
            kicker: None,
            line: format!("Today: {} · {}", tile.title, reason),
            groups: vec![Group { caption: Some("Today".into()), tiles: vec![tile], fan: false }],
            getting_ready: false,
        },
        None => Panel { kicker: None, line: otherwise, groups: Vec::new(), getting_ready: false },
    }
}

/// Which list a panel shows, for what it says when it is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Favorites,
    SmartShelf,
    Sticker,
}

/// The line of Favorites with no game starred; the panel shows a star resting on a cushion beside it (`ui_art`).
pub const FAVORITES_EMPTY: &str = "Press Y on a game to star it, and it shows here";

/// Favorites, a smart shelf or a sticker: its first games' covers. `line`: the row's line when it has games;
/// `ready`: false while Curator's index (which fills shelves and stickers) is being made.
pub fn list(kind: List, games: Vec<Tile>, line: String, ready: bool) -> Panel {
    if !ready {
        return Panel { kicker: None, line: "Getting ready…".into(), groups: Vec::new(), getting_ready: true };
    }
    if games.is_empty() {
        let line = match kind {
            List::Favorites => FAVORITES_EMPTY,
            List::SmartShelf => "No games here right now",
            List::Sticker => "Put it on a game from its card: X, then Stickers",
        };
        return Panel { kicker: None, line: line.into(), groups: Vec::new(), getting_ready: false };
    }
    let tiles = games.into_iter().take(MAX_COVERS).collect();
    Panel { kicker: None, line, groups: vec![Group { caption: None, tiles, fan: false }], getting_ready: false }
}

/// `line` made to fit `max` by leaving out its last " · " parts, as long as two are left: "Today: Aria of Sorrow ·
/// Never played · you like metroidvanias" loses "you like …" before the painter would cut a word. `measure` gives a
/// string's painted width (in the interface's language).
pub fn fit_parts(line: &str, max: i32, measure: &mut dyn FnMut(&str) -> i32) -> String {
    let mut parts: Vec<&str> = line.split(" · ").collect();
    while parts.len() > 2 && measure(&parts.join(" · ")) > max {
        parts.pop();
    }
    parts.join(" · ")
}

/// A title in lines at most `max` wide, at most `lines` of them: whole words, a word too long for a line cut with "…",
/// the last line shown ending with "…" when the title goes on.
pub fn title_lines(title: &str, max: i32, lines: usize, measure: &mut dyn FnMut(&str) -> i32, fit: &mut dyn FnMut(&str, i32) -> String) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let words: Vec<&str> = title.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        let joined = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if measure(&joined) <= max {
            cur = joined;
            continue;
        }
        if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur = if measure(w) <= max { w.to_string() } else { fit(w, max) };
        if out.len() == lines {
            break;
        }
        if i + 1 == words.len() {
            break;
        }
    }
    if !cur.is_empty() && out.len() < lines {
        out.push(cur);
    }
    let all: usize = words.len();
    let shown: usize = out.iter().map(|l| l.split_whitespace().count()).sum();
    if shown < all {
        if let Some(last) = out.last_mut() {
            let s = fit(&format!("{last} …"), max);
            *last = if s.ends_with('…') { s } else { format!("{s}…") };
        }
    }
    out
}

/// Where one cover goes, from the panel's top-left corner; `dim`: behind another in a fan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub group: usize,
    pub tile: usize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub dim: bool,
}

/// A panel placed in a box: its slots (in painting order, back to front), the captions (group, x, width; their line
/// is the box's foot), and the width used. The panel is set against the box's right edge by the painter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub slots: Vec<Slot>,
    pub captions: Vec<(usize, i32, i32)>,
    pub width: i32,
}

/// The design's panel is 176 px high (the label's box art); everything scales with the box's height.
const BASE_H: f32 = 176.0;
const COVER_W: f32 = 112.0;
const COVER_GAP: f32 = 12.0;
const TODAY_W: f32 = 124.0;
const FAN_W: f32 = 100.0;
const FAN_STEP: f32 = 34.0;
const GROUP_GAP: f32 = 28.0;
/// Faint tiles before the index is there.
pub const READY_TILES: usize = 3;

/// Lays a panel out in a box at most `max_w` wide and `h` high; `caption_h`: the room a caption takes under its
/// covers, `measure` its width (a group is as wide as its caption, at least). Groups that don't fit are left out
/// from the right, a list's covers from the end, never cut.
pub fn layout(panel: &Panel, max_w: i32, h: i32, caption_h: i32, measure: &mut dyn FnMut(&str) -> i32) -> Layout {
    let k = h as f32 / BASE_H;
    let px = |v: f32| (v * k).round() as i32;
    let captions = panel.groups.iter().any(|g| g.caption.is_some());
    let cover_h = if captions { h - caption_h } else { h };
    let mut out = Layout::default();
    let mut x = 0;
    if panel.getting_ready {
        for i in 0..READY_TILES {
            let w = px(COVER_W);
            if x + w > max_w {
                break;
            }
            out.slots.push(Slot { group: 0, tile: i, x, y: h - px(150.0), w, h: px(150.0), dim: true });
            x += w + px(COVER_GAP);
        }
        out.width = (x - px(COVER_GAP)).max(0);
        return out;
    }
    for (gi, g) in panel.groups.iter().enumerate() {
        let start = if gi == 0 { 0 } else { x + px(GROUP_GAP) };
        let mut slots = Vec::new();
        let end = if g.fan {
            // The first in front, the others peeking out to its right, each a little lower and dimmer.
            let (w, fh) = (px(FAN_W), cover_h - px(16.0));
            for (i, _) in g.tiles.iter().enumerate() {
                slots.push(Slot { group: gi, tile: i, x: start + i as i32 * px(FAN_STEP), y: cover_h - fh, w, h: fh, dim: i > 0 });
            }
            start + w + (g.tiles.len() as i32 - 1).max(0) * px(FAN_STEP)
        } else if g.caption.is_some() {
            let w = px(TODAY_W);
            for (i, _) in g.tiles.iter().enumerate().take(1) {
                slots.push(Slot { group: gi, tile: i, x: start, y: 0, w, h: cover_h, dim: false });
            }
            start + w
        } else {
            let (w, gap) = (px(COVER_W), px(COVER_GAP));
            let fit = ((max_w - start + gap) / (w + gap)).max(0) as usize;
            for i in 0..g.tiles.len().min(fit) {
                slots.push(Slot { group: gi, tile: i, x: start + i as i32 * (w + gap), y: 0, w, h: cover_h, dim: false });
            }
            start + slots.len() as i32 * (w + gap) - gap
        };
        let end = match &g.caption {
            Some(c) => end.max(start + measure(c)),
            None => end,
        };
        if slots.is_empty() || end > max_w {
            break;
        }
        if g.caption.is_some() {
            out.captions.push((gi, start, end - start));
        }
        // Back to front: a fan's last cover first.
        if g.fan {
            slots.reverse();
        }
        out.slots.extend(slots);
        x = end;
    }
    out.width = x;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten pixels a character, about Body 21.
    fn chars(s: &str) -> i32 {
        s.chars().count() as i32 * 10
    }

    fn tile(name: &str) -> Tile {
        Tile { title: name.into(), system: "snes".into(), rom: PathBuf::from(format!("/roms/snes/{name}.sfc")) }
    }

    fn today(waiting: Option<(Waiting, usize, Vec<Tile>)>) -> CuratorToday {
        CuratorToday { pick: Some((tile("Super Metroid"), "Never played · you gave it 5 stars".into())), waiting }
    }

    #[test]
    fn curator_shows_the_day_s_pick_with_its_reason_then_what_waits() {
        let p = curator(Some(today(Some((Waiting::Rediscover, 4, vec![tile("Chrono Trigger"), tile("EarthBound")])))));
        assert_eq!(p.kicker, Some("Your library by feel"));
        assert_eq!(p.line, "Today: Super Metroid · Never played · you gave it 5 stars");
        assert_eq!(p.groups.len(), 2);
        assert_eq!((p.groups[0].caption.as_deref(), p.groups[0].tiles.len()), (Some("Today"), 1));
        assert_eq!((p.groups[1].caption.as_deref(), p.groups[1].fan), (Some("4 to rediscover"), true));
        let p = curator(Some(today(Some((Waiting::NotStarted, 12, vec![tile("F-Zero")])))));
        assert_eq!(p.groups[1].caption.as_deref(), Some("12 never played"));
        // Nothing waits: the pick alone.
        let p = curator(Some(today(None)));
        assert_eq!(p.groups.len(), 1);
    }

    #[test]
    fn curator_is_never_a_blank_before_the_index_or_without_games() {
        let p = curator(None);
        assert!(p.getting_ready);
        assert_eq!(p.line, "Getting ready…");
        assert_eq!(layout(&p, 480, 176, 30, &mut chars).slots.len(), READY_TILES, "faint tiles where the covers will be");
        let p = curator(Some(CuratorToday { pick: None, waiting: None }));
        assert!(!p.line.is_empty() && p.groups.is_empty() && !p.getting_ready);
        assert_eq!(p.kicker, Some(CURATOR_KICKER), "the row still says what it is");
    }

    #[test]
    fn a_list_shows_its_first_covers_or_says_how_to_fill_it() {
        let five_plus: Vec<Tile> = (0..8).map(|i| tile(&format!("Game {i}"))).collect();
        let p = list(List::Favorites, five_plus, "8 games you starred".into(), true);
        assert_eq!(p.groups[0].tiles.len(), MAX_COVERS);
        assert_eq!(p.line, "8 games you starred");
        let empty = |kind| list(kind, Vec::new(), "0 games".into(), true);
        assert_eq!(empty(List::Favorites).line, "Press Y on a game to star it, and it shows here");
        assert_eq!(empty(List::SmartShelf).line, "No games here right now");
        assert_eq!(empty(List::Sticker).line, "Put it on a game from its card: X, then Stickers");
        assert!(empty(List::Favorites).groups.is_empty());
        let waiting = list(List::SmartShelf, vec![tile("A")], "1 game".into(), false);
        assert!(waiting.getting_ready && waiting.line == "Getting ready…");
    }

    #[test]
    fn a_long_line_loses_its_last_parts_before_a_word_is_cut() {
        let line = "Today: Aria of Sorrow · Never played · you like metroidvanias";
        assert_eq!(fit_parts(line, 1000, &mut chars), line);
        assert_eq!(fit_parts(line, 400, &mut chars), "Today: Aria of Sorrow · Never played");
        assert_eq!(fit_parts(line, 50, &mut chars), "Today: Aria of Sorrow · Never played", "two parts stay; the painter cuts the rest");
        assert_eq!(fit_parts("3 games you starred", 10, &mut chars), "3 games you starred");
    }

    #[test]
    fn a_title_tile_breaks_between_words_only() {
        let mut fit = |s: &str, max: i32| {
            let n = (max / 10).max(1) as usize;
            if s.chars().count() <= n {
                s.to_string()
            } else {
                format!("{}…", s.chars().take(n - 1).collect::<String>())
            }
        };
        assert_eq!(title_lines("Burnout Dominator", 80, 4, &mut chars, &mut fit), vec!["Burnout", "Dominat…"]);
        assert_eq!(title_lines("Burnout 2 - Point of Impact", 100, 4, &mut chars, &mut fit), vec!["Burnout 2", "- Point of", "Impact"]);
        assert_eq!(title_lines("Burnout 2 - Point of Impact", 100, 2, &mut chars, &mut fit), vec!["Burnout 2", "- Point o…"]);
        assert_eq!(title_lines("Tetris", 100, 3, &mut chars, &mut fit), vec!["Tetris"]);
        assert!(title_lines("", 100, 3, &mut chars, &mut fit).is_empty());
    }

    #[test]
    fn surprise_shows_today_s_pick_or_keeps_its_own_line() {
        let p = surprise(Some((tile("EarthBound"), "Never played".into())), "One game picked for you".into());
        assert_eq!(p.line, "Today: EarthBound · Never played");
        assert_eq!(p.groups[0].tiles[0].title, "EarthBound");
        let p = surprise(None, "One game picked for you".into());
        assert_eq!((p.line.as_str(), p.groups.len()), ("One game picked for you", 0));
    }

    #[test]
    fn the_layout_stays_in_its_box_overlaps_only_in_a_fan_and_scales_with_the_height() {
        let p = curator(Some(today(Some((Waiting::Rediscover, 4, vec![tile("A"), tile("B"), tile("C")])))));
        let l = layout(&p, 480, 176, 30, &mut chars);
        assert_eq!(l.slots.len(), 4);
        assert_eq!(l.captions.len(), 2);
        for s in &l.slots {
            assert!(s.x >= 0 && s.x + s.w <= l.width && s.y >= 0 && s.y + s.h <= 176 - 30, "{s:?}");
        }
        // The fan is painted back to front: its first cover last, not dimmed.
        let fan: Vec<&Slot> = l.slots.iter().filter(|s| s.group == 1).collect();
        assert_eq!(fan.iter().map(|s| s.tile).collect::<Vec<_>>(), vec![2, 1, 0]);
        assert!(!fan[2].dim && fan[0].dim);
        // The pick and the fan don't touch.
        let today_end = l.slots.iter().find(|s| s.group == 0).map(|s| s.x + s.w).unwrap();
        assert!(fan.iter().all(|s| s.x >= today_end + 20));
        // A caption wider than its covers widens its group (German runs longer).
        let long = curator(Some(today(Some((Waiting::NotStarted, 120, vec![tile("A")])))));
        let l = layout(&long, 600, 176, 30, &mut chars);
        let (_, x, w) = l.captions[1];
        assert_eq!(w, chars("120 never played"));
        assert_eq!(l.width, x + w);
        // A narrow box leaves the fan out rather than cutting it.
        let narrow = layout(&p, 200, 176, 30, &mut chars);
        assert_eq!(narrow.captions.len(), 1);
        assert!(narrow.width <= 200);
        // A list: as many covers as fit, side by side, never past the box.
        let favs = list(List::Favorites, (0..5).map(|i| tile(&i.to_string())).collect(), "5 games".into(), true);
        let l = layout(&favs, 480, 176, 30, &mut chars);
        assert_eq!(l.slots.len(), 3, "112 px covers 12 apart: three in 480");
        assert!(l.width <= 480 && l.captions.is_empty());
        assert!(l.slots.windows(2).all(|w| w[1].x >= w[0].x + w[0].w), "side by side, no overlap");
        assert_eq!(l.slots[0].h, 176, "no caption: the whole height");
        // Twice as high, twice as large.
        let big = layout(&favs, 2000, 352, 60, &mut chars);
        assert_eq!(big.slots[0].w, 224);
    }
}
