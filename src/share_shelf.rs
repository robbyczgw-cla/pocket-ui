//! Share the shelf as a picture (0.4): one button makes a square picture of the player's own shelf, to post. The real
//! cases (the cover on the front, the stickers on it) stand on boards in the page's colours, bottoms in line as a
//! collector keeps them, with pocket-ui's small mark in the corner. No menu, no hint, no file name, no path: the only
//! words are the games' titles (printed on their cases) and the name "pocket-ui".
//!
//! One picture for every place it is posted: **1:1, [`SIDE`] x [`SIDE`]**. A square is shown whole in a Reddit feed
//! on a phone and on the desktop, in a Discord channel and in a messenger; a wide picture shrinks to a strip on a
//! phone, a tall one is cut in the feed.
//!
//! This is the shared part, linked by the Linux app and the Android launcher: which games are on it ([`pick`]), where
//! each case stands ([`plan`]) and the painting ([`compose`]). Each platform brings the cases it already knows how to
//! make ([`Entry`]: medium, colours, cover, stickers). Nothing here reads a file or the clock.

use std::path::PathBuf;

use crate::cabinet_art::{self as art, Look, Pic};
use crate::case_stickers::{self, CaseStickers};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::media::{self, Medium, Prop};
use crate::palette::pal;
use crate::room_deco::Earned;
use crate::room_props::{self, PropPics};
use crate::system_art::Sprite;
use crate::text::{Face, Text};
use crate::ui_art::{self, Art};

/// Where the pictures are kept inside the screenshots folder, and what their file names start with: the gallery
/// lists them as the game "pocket-ui shelf" of the "system" `shelf`, and the web companion lists the same files.
pub const FOLDER: &str = "shelf";
pub const NAME: &str = "pocket-ui shelf";

/// The picture's side in pixels.
pub const SIDE: i32 = 1440;
/// The most cases on the picture: four shelves of five. A library with more shows the ones that say most about it
/// ([`pick`]); the cases stay large enough to read their titles.
pub const MAX_CASES: usize = 20;

/// Wall left and right of the shelves.
const MARGIN: i32 = 110;
/// Wall above the first shelf, and the band under the last one that holds the mark.
const TOP: i32 = 96;
const FOOT: i32 = 190;
/// The largest a case may be drawn: a template pixel is this many pixels (a lone case does not fill the picture).
const F_MAX: f32 = 6.0;
const F_MIN: f32 = 1.5;
/// The mark's pixels (the cartridge is 50 x 52 pixels art).
const MARK_K: i32 = 2;

/// What decides whether a game makes it onto a picture that has room for [`MAX_CASES`].
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub favourite: bool,
    /// A sticker on its case.
    pub stickers: bool,
    /// Has box art.
    pub cover: bool,
    /// 0 for the game played last.
    pub recent: Option<usize>,
    pub system: String,
    pub title: String,
}

/// The games that go on the picture, as indexes into `all`, in the order they stand: by system, then by title, so
/// the shelves read like the Shelf look's rows. At most `max`; when there are more, the favourites, the stickered,
/// the ones with box art and the recently played come first (a case with a cover tells more than a title card), the
/// rest in the library's order.
pub fn pick(all: &[Candidate], max: usize) -> Vec<usize> {
    let score = |c: &Candidate| -> i32 {
        let recent = c.recent.map_or(0, |r| (6 - r as i32).max(0));
        4 * c.favourite as i32 + 4 * c.stickers as i32 + 3 * c.cover as i32 + recent
    };
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(score(&all[i])), i));
    order.truncate(max);
    order.sort_by(|&a, &b| {
        let key = |i: usize| (all[i].system.to_lowercase(), all[i].title.to_lowercase(), i);
        key(a).cmp(&key(b))
    });
    order
}

/// A case to put on the picture. Nothing in it needs the files read: the box art is read where the picture is made
/// ([`read_covers`], on its thread), so the player's press costs the UI no time.
pub struct Entry {
    pub medium: Medium,
    /// The system's id ("snes"): the plastic of its cartridges.
    pub system: String,
    /// The system's colour (its stripe): the case's band, and its colours when it has no box art.
    pub stripe: u32,
    /// The game's name as it is printed on the case.
    pub title: String,
    /// The system's short name for a case without box art ("SNES").
    pub system_name: String,
    /// The system's console drawing, printed on a plain case.
    pub console: Option<&'static Sprite>,
    /// Where the box art is, if the game has some ([`read_covers`] reads it).
    pub cover_file: Option<PathBuf>,
    /// The box art as read from its file (cut to the case here).
    pub cover: Option<Image>,
    pub stickers: CaseStickers,
    /// Keeps each sticker's tilt and place the same every time (the game's file name hashed).
    pub seed: u64,
}

impl Entry {
    /// The case's colours: from its box art when it has one.
    fn look(&self) -> Look {
        let colors = self.cover.as_ref().and_then(|i| media::cover_colors(&media::trim_system_band(i, self.medium.game()).px));
        media::case_look(&self.system, self.stripe, self.medium, colors)
    }
}

/// Reads the box art of every entry that has a file for it. A file that cannot be read leaves the case plain.
pub fn read_covers(entries: &mut [Entry]) {
    for e in entries.iter_mut().filter(|e| e.cover.is_none()) {
        e.cover = e.cover_file.as_deref().and_then(crate::image::load_image);
    }
}

/// Where one case stands: its front's top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// One shelf board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Board {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where everything stands on the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Pixels a template pixel of the cases.
    pub f: f32,
    pub slots: Vec<Slot>,
    pub boards: Vec<Board>,
}

fn gap_of(f: f32) -> i32 {
    (6.0 * f).round() as i32
}

fn board_h_of(f: f32) -> i32 {
    ((11.0 * f).round() as i32).max(10)
}

/// What stands on a board: a case, or a piece of the room (the bookend, the plant, the trophy, the framed "100 %").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Case(Medium),
    Prop(Prop),
}

impl Item {
    /// Its front's size at `f` pixels a template pixel.
    fn size(self, f: f32) -> (i32, i32) {
        match self {
            Item::Case(m) => media::front_size(m, f / media::shelf_scale()),
            // A prop is drawn at the same pixel size as the cases it stands beside: the picture was made at the shelf's scale.
            Item::Prop(p) => {
                let k = f / media::SHELF_SCALE;
                (((p.pic().w as f32) * k).round() as i32, ((p.pic().h as f32) * k).round() as i32)
            }
        }
    }
}

/// The rows at scale `f`: `(first, count)` of the items in each, filled in order.
fn wrap(sizes: &[(i32, i32)], gap: i32, room: i32) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let (mut start, mut width) = (0usize, 0i32);
    for (i, &(w, _)) in sizes.iter().enumerate() {
        let next = if i == start { w } else { width + gap + w };
        if i > start && next > room {
            rows.push((start, i - start));
            start = i;
            width = w;
        } else {
            width = next;
        }
    }
    if start < sizes.len() {
        rows.push((start, sizes.len() - start));
    }
    rows
}

fn sizes_at(items: &[Item], f: f32) -> Vec<(i32, i32)> {
    items.iter().map(|&i| i.size(f)).collect()
}

/// Lays `items` out on the picture: the largest scale at which they all fit, wrapped into shelves of the same
/// width, every shelf's items centred on it with their bottoms on its board, the whole block centred between the top
/// margin and the band of the mark. Empty for none.
pub fn plan(items: &[Item]) -> Plan {
    if items.is_empty() {
        return Plan { f: 0.0, slots: Vec::new(), boards: Vec::new() };
    }
    let room = SIDE - 2 * MARGIN;
    let height = SIDE - TOP - FOOT;
    let row_gap = |f: f32| (12.0 * f).round() as i32;
    let fits = |f: f32| -> bool {
        let sizes = sizes_at(items, f);
        if sizes.iter().any(|&(w, _)| w > room) {
            return false;
        }
        let rows = wrap(&sizes, gap_of(f), room);
        let used: i32 = rows.iter().map(|&(s, n)| sizes[s..s + n].iter().map(|z| z.1).max().unwrap_or(0) + board_h_of(f)).sum::<i32>() + (rows.len() as i32 - 1) * row_gap(f);
        used <= height
    };
    // From the largest down in small steps: the first scale that fits.
    let mut f = F_MAX;
    while f > F_MIN && !fits(f) {
        f -= 0.05;
    }
    let sizes = sizes_at(items, f);
    let rows = wrap(&sizes, gap_of(f), room);
    let (bh, rg, gap) = (board_h_of(f), row_gap(f), gap_of(f));
    let heights: Vec<i32> = rows.iter().map(|&(s, n)| sizes[s..s + n].iter().map(|z| z.1).max().unwrap_or(0)).collect();
    let used: i32 = heights.iter().map(|h| h + bh).sum::<i32>() + (rows.len() as i32 - 1) * rg;
    let mut y = TOP + ((height - used).max(0)) / 2;
    let (mut slots, mut boards) = (Vec::new(), Vec::new());
    // The boards are as wide as the widest row needs, a little more at each end.
    let widest = rows.iter().map(|&(s, n)| sizes[s..s + n].iter().map(|z| z.0).sum::<i32>() + (n as i32 - 1) * gap).max().unwrap_or(0);
    let pad = (14.0 * f).round() as i32;
    let board_w = (widest + 2 * pad).min(SIDE - 2 * (MARGIN - 40));
    for (r, &(s, n)) in rows.iter().enumerate() {
        let floor = y + heights[r];
        let row_w: i32 = sizes[s..s + n].iter().map(|z| z.0).sum::<i32>() + (n as i32 - 1) * gap;
        let mut x = (SIDE - row_w) / 2;
        for i in s..s + n {
            let (w, h) = sizes[i];
            slots.push(Slot { index: i, x, y: floor - h, w, h });
            x += w + gap;
        }
        boards.push(Board { x: (SIDE - board_w) / 2, y: floor, w: board_w, h: bh });
        y = floor + bh + rg;
    }
    Plan { f, slots, boards }
}

/// The room's pieces after the last case, as many as `deco` allows and the picture has room for: the bookend, then
/// the player's decorations. None of them when they would stand on a shelf of their own, or make the cases smaller
/// than they would be without them: a trophy is not worth a smaller game.
pub fn with_deco(cases: &[Medium], deco: &[Prop]) -> Vec<Item> {
    let bare: Vec<Item> = cases.iter().map(|&m| Item::Case(m)).collect();
    if cases.is_empty() || deco.is_empty() {
        return bare;
    }
    let mut full = bare.clone();
    full.push(Item::Prop(Prop::Bookend));
    full.extend(deco.iter().map(|&p| Item::Prop(p)));
    // The same shelves and the cases no smaller than without: otherwise the cases alone.
    let (with, without) = (plan(&full), plan(&bare));
    // Everything on the last shelf after the cases: the room's pieces alone on a board.
    let lone = with.slots.last().is_some_and(|last| with.slots.iter().filter(|o| o.y + o.h == last.y + last.h).all(|o| o.index >= cases.len()));
    if with.boards.len() > without.boards.len() || with.f + 0.2 < without.f || lone {
        return bare;
    }
    full
}

/// What the page says once the picture is saved.
pub const SAVED: &str = "Saved. It is the newest picture in the list below.";

/// Why there is no picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No game to put on it.
    Empty,
    /// The picture could not be encoded or written (a card that is full or read-only; the log says which).
    Failed,
}

impl Error {
    /// The words for the player; each is a line of `assets/i18n/de.tsv`.
    pub fn words(self) -> &'static str {
        match self {
            Error::Empty => "Your shelf is empty. Add a game first, then share it.",
            Error::Failed => "Could not save the picture",
        }
    }
}

/// The picture: [`SIDE`] x [`SIDE`], 0RGB.
pub struct Shot {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Shot {
    /// Packed RGB, for the PNG encoder.
    pub fn rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 3);
        for &p in &self.px {
            out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
        out
    }

    /// The picture as a PNG.
    pub fn png(&self) -> Option<Vec<u8>> {
        let rgb = self.rgb();
        let mut out = Vec::with_capacity(rgb.len() / 2 + 1024);
        let mut enc = png::Encoder::new(&mut out, self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        // The picture is flat colour and soft shadows: Fast is a few times quicker on a handheld and not much bigger.
        enc.set_compression(png::Compression::Fast);
        enc.write_header().and_then(|mut w| w.write_image_data(&rgb)).ok()?;
        Some(out)
    }
}

/// The wall: the page colour, a little lighter in the middle so the shelves have a room around them.
fn wall(c: &mut Canvas) {
    let void = pal().void;
    let (cx, cy) = (c.w as f32 / 2.0, c.h as f32 * 0.45);
    let reach = c.w as f32 * 0.7;
    // Lighter on the dark page, a touch darker at the corners on the light one: the same strength either way.
    let toward = if void & 0xff > 0x80 { 0x000000 } else { 0xffffff };
    let (w, h) = (c.w, c.h);
    c.per_pixel(0, w, 0, h, |x, y| {
        let d = (((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() / reach).min(1.0);
        let t = if toward == 0xffffff { (1.0 - d) * 0.07 } else { d * d * 0.10 };
        Some(mix(void, toward, (t * 255.0) as u32))
    });
}

/// One case's front, as the Shelf look makes it: the cover cut to the label, the stickers on it.
fn front_of(text: &mut Text, e: &Entry, f: f32) -> Pic {
    let k = f / media::shelf_scale();
    let (_, _, lw, lh) = media::label_rect(e.medium, k);
    let cover = e.cover.as_ref().map(|i| art::cover_fit_case(i, lw, lh, e.medium));
    let title = media::printed_title(&e.title, true);
    let mut pic = media::front(text, e.medium, k, e.look(), &title, cover.as_ref(), if cover.is_some() { None } else { e.console }, &e.system_name);
    case_stickers::on_front_of(text, &mut pic, &e.stickers, e.medium, k, &title, cover.is_some(), if cover.is_some() { None } else { e.console }, e.seed);
    pic
}

/// pocket-ui's mark in the bottom right corner: the cartridge and the name, quiet.
fn mark(c: &mut Canvas, text: &mut Text) {
    let (mw, mh) = ui_art::size(Art::Logo, MARK_K);
    let y = c.h - 56 - mh;
    let x = c.w - MARGIN - mw;
    ui_art::paint(c, Art::Logo, x, y, MARK_K);
    let px = 30;
    let name = "pocket-ui";
    let w = text.width(Face::Title, px, name);
    text.draw(c, Face::Title, px, x - 18 - w, y + mh / 2 + px as i32 / 3, mix(pal().void, pal().chalk, 150), name);
}

/// What the room adds to the shelves: the decorations the player has earned.
pub struct Room<'a> {
    /// The pieces that stand after the last case, most important first (`room_props::deco_for`); none with
    /// Decorations off.
    pub deco: &'a [Prop],
    pub earned: &'a Earned,
}

/// The picture of `entries` (in the order they stand): at most [`MAX_CASES`] of them, then the bookend and the pieces
/// of `room` where they fit.
pub fn compose(text: &mut Text, entries: &[Entry], room: &Room) -> Result<Shot, Error> {
    if entries.is_empty() {
        return Err(Error::Empty);
    }
    let entries = &entries[..entries.len().min(MAX_CASES)];
    // The words on the cases keep the design's size, whatever Large text says: one picture for everybody.
    let _fixed = crate::type_scale::fixed();
    let mediums: Vec<Medium> = entries.iter().map(|e| e.medium).collect();
    let items = with_deco(&mediums, room.deco);
    let plan = plan(&items);
    let mut px = vec![0u32; (SIDE * SIDE) as usize];
    let mut c = Canvas::new(&mut px, SIDE as u32, SIDE as u32);
    wall(&mut c);
    let shadows = pal().void != 0;
    for b in &plan.boards {
        let board = art::top_board(b.w, b.h);
        art::blit(&mut c, &board, b.x, b.y, 256, (0, SIDE));
        if shadows {
            art::soft_shadow(&mut c, b.x, b.y + b.h, b.w, 12, 18, 0.5, (0, SIDE));
        }
    }
    let mut pics = PropPics::new();
    for s in &plan.slots {
        let pic = match items[s.index] {
            Item::Case(_) => front_of(text, &entries[s.index], plan.f),
            Item::Prop(p) => (*room_props::prop_pic(&mut pics, text, p, room.earned, plan.f)).clone(),
        };
        // A case is a box and casts one; a plant or a trophy is pixel art with air round it: its shadow would be a box too.
        if shadows && matches!(items[s.index], Item::Case(_)) {
            // On the wall behind, and on the board under it.
            art::soft_shadow(&mut c, s.x + 6, s.y + 10, pic.w, pic.h, 22, 0.35, (0, SIDE));
        }
        art::blit(&mut c, &pic, s.x, s.y, 256, (0, SIDE));
    }
    mark(&mut c, text);
    Ok(Shot { w: SIDE, h: SIDE, px })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wall painted natively (`n` = 2) is the old picture (the design canvas and the 2:1 filter) bit for bit, on
    /// both pages.
    #[test]
    fn painted_natively_the_wall_is_the_old_picture() {
        let (w, h) = (320u32, 240u32);
        for light in [0, 255] {
            crate::palette::scoped(light, || {
                let mut design = vec![0u32; (w * h * 4) as usize];
                wall(&mut Canvas::new(&mut design, w * 2, h * 2));
                let mut old = vec![0u32; (w * h) as usize];
                crate::scaled_layout::ScaledFrame::default().downscale(&design, (w * 2, h * 2), &mut old, (w, h));
                let mut new = vec![0u32; (w * h) as usize];
                wall(&mut Canvas::screen(&mut new, (w, h), 2));
                assert_eq!(old, new, "light {light}");
                assert_ne!(new[0], new[(h / 2 * w + w / 2) as usize], "lighter or darker in the middle than at the corner");
            });
        }
    }

    fn cases(m: &[Medium]) -> Vec<Item> {
        m.iter().map(|&m| Item::Case(m)).collect()
    }

    fn cand(title: &str, system: &str) -> Candidate {
        Candidate { title: title.into(), system: system.into(), ..Default::default() }
    }

    #[test]
    fn few_games_all_go_on_in_shelf_order() {
        let all = vec![cand("Zelda", "snes"), cand("Tetris", "gb"), cand("Mario", "snes")];
        assert_eq!(pick(&all, MAX_CASES), vec![1, 2, 0], "by system, then title");
    }

    #[test]
    fn a_big_library_keeps_what_says_most() {
        let mut all: Vec<Candidate> = (0..60).map(|i| cand(&format!("Game {i:02}"), "gba")).collect();
        all[41].favourite = true;
        all[7].stickers = true;
        all[55].cover = true;
        all[3].recent = Some(0);
        let shown = pick(&all, MAX_CASES);
        assert_eq!(shown.len(), MAX_CASES);
        for must in [41, 7, 55, 3] {
            assert!(shown.contains(&must), "{must} is on the picture");
        }
    }

    fn assert_fits(p: &Plan) {
        for s in &p.slots {
            assert!(s.x >= 0 && s.x + s.w <= SIDE && s.y >= TOP - 1 && s.y + s.h <= SIDE - FOOT + 1, "case {s:?} inside the picture and above the mark's band");
        }
        for b in &p.boards {
            assert!(b.x >= 0 && b.x + b.w <= SIDE && b.y + b.h <= SIDE - FOOT + 1, "board {b:?} inside");
        }
    }

    #[test]
    fn five_games_stand_large_and_whole_and_a_lone_one_does_not_fill_the_picture() {
        let five = plan(&cases(&[Medium::Snes, Medium::Gba, Medium::Gba, Medium::Gb, Medium::Snes]));
        assert_eq!(five.slots.len(), 5);
        assert_fits(&five);
        assert!(five.f >= 3.0, "five cases are drawn large, not as a thin row: f = {}", five.f);
        let one = plan(&cases(&[Medium::Snes]));
        assert_eq!(one.slots.len(), 1);
        assert!(one.f <= F_MAX + 0.01);
        assert!(one.slots[0].h < SIDE * 3 / 5, "a lone case leaves wall around it");
    }

    #[test]
    fn a_full_shelf_fits_at_a_readable_size() {
        let kinds = [Medium::Snes, Medium::Gba, Medium::Ps1, Medium::Gb, Medium::N64, Medium::GameCube];
        let many: Vec<Medium> = (0..MAX_CASES).map(|i| kinds[i % kinds.len()]).collect();
        let p = plan(&cases(&many));
        assert_eq!(p.slots.len(), MAX_CASES);
        assert_fits(&p);
        assert!(p.f >= 2.0, "twenty cases stay readable: f = {}", p.f);
    }

    #[test]
    fn the_cases_of_a_shelf_stand_on_its_board() {
        let p = plan(&cases(&[Medium::Snes, Medium::Gba, Medium::Ps1, Medium::Gb, Medium::N64, Medium::GameCube, Medium::Snes]));
        for s in &p.slots {
            assert!(p.boards.iter().any(|b| b.y == s.y + s.h), "{s:?} stands on a board");
        }
    }

    fn entry(m: Medium, title: &str) -> Entry {
        Entry {
            medium: m,
            system: "snes".into(),
            stripe: 0x6a5acd,
            title: title.into(),
            system_name: "SNES".into(),
            console: None,
            cover_file: None,
            cover: None,
            stickers: CaseStickers::default(),
            seed: 7,
        }
    }

    fn entries(n: usize) -> Vec<Entry> {
        let kinds = [Medium::Snes, Medium::Gba, Medium::Ps1, Medium::Gb, Medium::N64, Medium::GameCube];
        (0..n).map(|i| entry(kinds[i % kinds.len()], &format!("Game {}", i + 1))).collect()
    }

    fn trophy_room(earned: &Earned) -> Vec<Prop> {
        let mut deco = room_props::deco_for(earned, None, true);
        deco.push(Prop::Plant);
        deco
    }

    /// How many of the pixels in `r` (x, y, w, h) are not the wall's colour at that place.
    fn inked(shot: &Shot, wall: &Shot, (x, y, w, h): (i32, i32, i32, i32)) -> f32 {
        let mut n = 0;
        for yy in y..y + h {
            for xx in x..x + w {
                let i = (yy * SIDE + xx) as usize;
                if shot.px[i] != wall.px[i] {
                    n += 1;
                }
            }
        }
        n as f32 / (w * h) as f32
    }

    /// The same picture with nothing on it: the wall, the boards' places left out.
    fn wall_only() -> Shot {
        let mut px = vec![0u32; (SIDE * SIDE) as usize];
        wall(&mut Canvas::new(&mut px, SIDE as u32, SIDE as u32));
        Shot { w: SIDE, h: SIDE, px }
    }

    #[test]
    fn every_size_of_collection_makes_a_whole_picture_with_every_case_on_it_and_the_mark_in_the_corner() {
        let room = Earned { plant: 2, trophy: true, hundred: true, figures: Vec::new() };
        let deco = trophy_room(&room);
        for n in [1, 2, 5, 9, 14, MAX_CASES] {
            let mut text = Text::new();
            let list = entries(n);
            let shot = compose(&mut text, &list, &Room { deco: &deco, earned: &room }).unwrap_or_else(|_| panic!("{n} games: a picture"));
            assert_eq!((shot.w, shot.h, shot.px.len() as i32), (SIDE, SIDE, SIDE * SIDE), "{n}: square, whole");
            let wall = wall_only();
            // Every case is there: its front is no wall (a plain case prints its title and its system on it).
            let mediums: Vec<Medium> = list.iter().map(|e| e.medium).collect();
            let p = plan(&with_deco(&mediums, &deco));
            for s in p.slots.iter().filter(|s| s.index < n) {
                assert!(inked(&shot, &wall, (s.x, s.y, s.w, s.h)) > 0.6, "{n} games: case {} stands on the picture", s.index);
            }
            // The mark's place in the bottom right is not empty wall.
            assert!(inked(&shot, &wall, (SIDE - MARGIN - 160, SIDE - 140, 160, 100)) > 0.15, "{n} games: the mark");
            // Nothing is cut at the edge: the margin all round is the wall.
            for r in [(0, 0, SIDE, 40), (0, 0, 40, SIDE), (SIDE - 40, 0, 40, SIDE)] {
                assert!(inked(&shot, &wall, r) < 0.01, "{n} games: wall at the edge {r:?}");
            }
        }
    }

    #[test]
    fn a_few_games_are_drawn_large_and_a_lone_one_does_not_fill_the_picture() {
        let mut text = Text::new();
        let wall = wall_only();
        let one = compose(&mut text, &entries(1), &Room { deco: &[], earned: &Earned::default() }).unwrap();
        assert!(inked(&one, &wall, (MARGIN, TOP, SIDE - 2 * MARGIN, SIDE - TOP - FOOT)) < 0.35, "one case is a case on a board, not a wall of colour");
        let five = compose(&mut text, &entries(5), &Room { deco: &[], earned: &Earned::default() }).unwrap();
        assert!(inked(&five, &wall, (MARGIN, TOP, SIDE - 2 * MARGIN, SIDE - TOP - FOOT)) > 0.3, "five games fill the shelves, they are not a thin strip");
    }

    #[test]
    fn the_same_shelf_makes_the_same_picture_every_time() {
        let mut text = Text::new();
        let room = Earned { plant: 3, trophy: true, hundred: false, figures: Vec::new() };
        let deco = trophy_room(&room);
        let a = compose(&mut text, &entries(7), &Room { deco: &deco, earned: &room }).unwrap();
        let b = compose(&mut Text::new(), &entries(7), &Room { deco: &deco, earned: &room }).unwrap();
        assert!(a.px == b.px, "no clock, no randomness: a second press gives the same picture (the stickers keep their places)");
    }

    #[test]
    fn the_light_page_gets_a_light_picture_with_a_readable_mark() {
        let _dark = {
            let mut text = Text::new();
            compose(&mut text, &entries(5), &Room { deco: &[], earned: &Earned::default() }).unwrap()
        };
        crate::palette::set_blend(255);
        let mut text = Text::new();
        let light = compose(&mut text, &entries(5), &Room { deco: &[], earned: &Earned::default() }).unwrap();
        let lum = |p: u32| ((p >> 16) & 255) + ((p >> 8) & 255) + (p & 255);
        assert!(lum(light.px[(20 * SIDE + 20) as usize]) > 3 * 200, "the wall of the light page is light");
        assert!(lum(_dark.px[(20 * SIDE + 20) as usize]) < 3 * 60, "and the dark page's is dark");
        // The mark's name is dark on light: it reads.
        let name_row = (SIDE - 56 - 60..SIDE - 56 - 20).map(|y| (MARGIN + 700..SIDE - MARGIN - 130).map(|x| lum(light.px[(y * SIDE + x) as usize])).min().unwrap_or(765)).min().unwrap();
        assert!(name_row < 3 * 190, "the name \"pocket-ui\" is darker than the wall");
        crate::palette::set_blend(0);
    }

    #[test]
    fn the_rooms_pieces_cast_no_box_of_shadow_on_the_wall() {
        // On the light page the plant stood in a grey rectangle: a shadow the size of its picture, corners and all.
        crate::palette::set_blend(255);
        let room = Earned { plant: 2, trophy: true, hundred: true, figures: Vec::new() };
        let deco = trophy_room(&room);
        let mut text = Text::new();
        let list: Vec<Entry> = [Medium::Snes, Medium::Gba, Medium::Gb].iter().map(|&m| entry(m, "Game")).collect();
        let shot = compose(&mut text, &list, &Room { deco: &deco, earned: &room }).unwrap();
        let wall = wall_only();
        let mediums: Vec<Medium> = list.iter().map(|e| e.medium).collect();
        let p = plan(&with_deco(&mediums, &deco));
        let plant = p.slots.last().expect("the plant is the last thing on the shelf");
        assert!(plant.index >= list.len(), "that is a piece of the room");
        // The picture's top right corner is empty air in the pixel art: the wall shows there.
        let at = ((plant.y + 1) * SIDE + plant.x + plant.w - 2) as usize;
        assert_eq!(shot.px[at], wall.px[at], "no shadow box round the plant");
        crate::palette::set_blend(0);
    }

    #[test]
    fn the_picture_is_a_png_that_decodes_to_what_was_painted() {
        let mut text = Text::new();
        let shot = compose(&mut text, &entries(3), &Room { deco: &[], earned: &Earned::default() }).unwrap();
        let png = shot.png().expect("encodes");
        let mut dec = png::Decoder::new(png.as_slice()).read_info().unwrap();
        let mut buf = vec![0; dec.output_buffer_size()];
        let info = dec.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (SIDE as u32, SIDE as u32));
        assert_eq!(&buf[..info.buffer_size()], shot.rgb().as_slice());
        assert!(png.len() < 3_000_000, "it is posted: {} bytes", png.len());
    }

    #[test]
    fn the_trophy_and_the_plant_stand_after_the_cases_but_never_cost_a_game_its_size_or_stand_alone() {
        let deco = [Prop::Trophy, Prop::Hundred, Prop::Plant];
        let few = [Medium::Snes, Medium::Gba, Medium::Gb];
        let items = with_deco(&few, &deco);
        assert_eq!(items.len(), few.len() + 1 + deco.len(), "bookend, trophy, 100 %, plant: room on the shelf");
        assert_eq!(items[3], Item::Prop(Prop::Bookend));
        assert!(items[..3].iter().all(|i| matches!(i, Item::Case(_))), "the cases come first");
        // Cases that fill their shelves to the edge keep their size: the room's pieces are left out, not a smaller case.
        let wide: Vec<Medium> = (0..MAX_CASES).map(|_| Medium::GameCube).collect();
        let full = with_deco(&wide, &deco);
        let (with, without) = (plan(&full), plan(&cases(&wide)));
        assert!(with.f + 0.2 >= without.f && with.boards.len() == without.boards.len(), "the cases are not smaller for the trophy");
        // Nothing earned: just the cases, and a bookend alone is no decoration.
        assert_eq!(with_deco(&few, &[]), cases(&few));
        assert!(with_deco(&[], &deco).is_empty());
    }

    #[test]
    fn no_game_is_no_picture_and_says_why() {
        let mut text = Text::new();
        assert!(matches!(compose(&mut text, &[], &Room { deco: &[], earned: &Earned::default() }), Err(Error::Empty)));
        assert!(Error::Empty.words().contains("empty"));
    }
}
