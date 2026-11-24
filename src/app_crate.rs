//! Part of the pocket-ui binary: Shelf space's crate in the Shelf look (`pocket_ui::shelf_space`, laid out by
//! `pocket_ui::crate_scene`). The crate is a screen of its own (`Screen::Crate`, "Crate" on Home): the games not on the
//! shelf stand in cardboard boxes on the floor, one box or more per system, the selected one pulled up out of its box
//! and shown large above them as the shelf shows its selection. A takes it out: with a free place on the shelf it goes
//! straight up onto it; with the shelf full the room shows the shelf over the boxes, the player picks the game that goes
//! into the crate for it, and the two swap places (`Swap`), each case carried from where it stood to where it goes.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::cabinet_art::{self as art, Pic};
use pocket_ui::crate_scene::{self, Case, Laid};
use pocket_ui::palette::*;
use pocket_ui::scene_check::{self, Rests};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::app_cabinet::{shelf_layout, show_k, ShelfLayout};
use super::{App, Button, Item, Screen, CANCEL, CONFIRM};

/// Settings → Look's row (`Opens::ShelfSpace`).
pub(crate) const SHELF_SPACE_ROW: &str = "Shelf space";
/// The line under it.
pub(crate) const SHELF_SPACE_LINE: &str = "Only so many games on the shelf, the rest in a crate under it. Against the backlog.";

/// The Shelf look's tallest case (`app_cabinet::TALLEST`).
const TALLEST: f32 = 120.0;

/// The crate's state on screen.
#[derive(Default)]
pub(crate) struct CrateUi {
    /// The crate row's scroll (row pixels at the view's left edge).
    scroll: f32,
    /// Choosing the game that goes into the crate for `Pick::inn` (the shelf is full).
    pub(crate) pick: Option<Pick>,
    /// Two cases on their way.
    pub(crate) swap: Option<Swap>,
    /// Box pictures made for a width and scale: (back, front).
    pics: Vec<((i32, i32), (Rc<Pic>, Rc<Pic>))>,
}

pub(crate) struct Pick {
    /// The game from the crate.
    pub(crate) inn: PathBuf,
    /// The selected game of the shelf (index into `App::crate_shelf`).
    pub(crate) sel: usize,
    pub(crate) moved_at: Instant,
}

pub(crate) struct Swap {
    pub(crate) at: Instant,
    pub(crate) inn: PathBuf,
    /// None: the shelf had a free place.
    pub(crate) out: Option<PathBuf>,
    /// The two rows as they were when it started: the shelf's games, and the crate's.
    shelf_before: Vec<Item>,
    crate_before: Vec<Item>,
}

impl Swap {
    fn len(&self) -> Duration {
        pocket_ui::motion::span(pocket_ui::motion::Kind::Move, if self.out.is_some() { crate_scene::SWAP } else { crate_scene::PUT })
    }
}

/// The box picture, decoded once.
pub(crate) fn crate_art() -> &'static pocket_ui::system_art::Sprite {
    static ART: std::sync::OnceLock<pocket_ui::system_art::Sprite> = std::sync::OnceLock::new();
    ART.get_or_init(|| {
        pocket_ui::system_art::decode(include_bytes!("../assets/media/props/crate.png"))
            .unwrap_or(pocket_ui::system_art::Sprite { w: 1, h: 1, px: vec![0] })
    })
}

/// The crate on Home in the Shelf look stands on the board as the box itself, as high as this part of a case (19.11.2026:
/// it was a plain case among the games, with "Crate" on its spine).
pub(crate) const HOME_BOX_OF_CASE: f32 = 0.5;

/// The box on Home at the row's scale `f`: its width (a multiple of four, as every spine's place) and its picture's scale.
pub(crate) fn home_box(f: f32) -> (i32, f32) {
    let k = pocket_ui::media::Medium::Keep.template_size().h as f32 * f * HOME_BOX_OF_CASE / crate_scene::ART_H as f32;
    let w = ((crate_scene::ART_W as f32 * k).round() as i32 + 3) / 4 * 4;
    (w, k)
}

/// Which column of the box picture is column `x` of a box `w` columns wide: the sides as they are, the tape in the
/// middle, plain cardboard between.
fn art_col(x: i32, w: i32) -> i32 {
    use crate_scene::{ART_SIDE, ART_TAPE, ART_W};
    if x < ART_SIDE {
        return x;
    }
    if x >= w - ART_SIDE {
        return ART_W - (w - x);
    }
    let tape = ART_TAPE.1 - ART_TAPE.0;
    let t0 = (w - tape) / 2;
    if (t0..t0 + tape).contains(&x) {
        return ART_TAPE.0 + x - t0;
    }
    // Plain cardboard: three of its columns in turn so the wall keeps its grain.
    let plain = if x < t0 { ART_SIDE + 2 } else { ART_TAPE.1 + 4 };
    plain + (x * 7 % 3)
}

/// `paths` as the games of `systems` (`Item::Game`), in `paths`' order; a path no system has is left out.
pub(crate) fn items_in_order(systems: &[pocket_ui::library::System], paths: &[PathBuf]) -> Vec<Item> {
    if paths.is_empty() {
        return Vec::new();
    }
    let place: std::collections::HashMap<&std::path::Path, usize> = paths.iter().enumerate().rev().map(|(i, p)| (p.as_path(), i)).collect();
    let mut found: Vec<Option<Item>> = vec![None; paths.len()];
    let mut left = place.len();
    'card: for (s, sys) in systems.iter().enumerate() {
        for (g, game) in sys.games.iter().enumerate() {
            if let Some(&i) = place.get(game.path.as_path()) {
                if found[i].is_none() {
                    found[i] = Some(Item::Game(s, g));
                    left -= 1;
                    if left == 0 {
                        break 'card;
                    }
                }
            }
        }
    }
    found.into_iter().flatten().collect()
}

impl App {
    /// Shelf space is on: the shelf has a limit and the crate is a screen.
    pub(crate) fn shelf_space_on(&self) -> bool {
        self.shelf_space.on()
    }

    /// The games in the crate: every game not on the shelf, by system and name.
    pub(crate) fn crate_items(&self) -> Vec<Item> {
        let mut out = Vec::new();
        for (s, sys) in self.systems.iter().enumerate() {
            for (g, game) in sys.games.iter().enumerate() {
                if !self.shelf_space.on_shelf(&game.path) {
                    out.push(Item::Game(s, g));
                }
            }
        }
        out
    }

    /// How many games are in the crate (its card's line on Home), counted without making the list.
    pub(crate) fn crate_count(&self) -> usize {
        self.systems.iter().map(|sys| sys.games.iter().filter(|g| !self.shelf_space.on_shelf(&g.path)).count()).sum()
    }

    /// The games on the shelf, in its order. One pass over the card's games (the crate room asks on every frame): a search
    /// through every game for each place was 500 × 179,000 path comparisons a frame on the R36S card.
    pub(crate) fn crate_shelf(&self) -> Vec<Item> {
        items_in_order(&self.systems, &self.shelf_space.shelf)
    }

    pub(crate) fn item_of_path(&self, p: &std::path::Path) -> Option<Item> {
        self.systems.iter().enumerate().find_map(|(s, sys)| sys.games.iter().position(|g| g.path == p).map(|g| Item::Game(s, g)))
    }

    fn path_of(&self, item: Item) -> Option<PathBuf> {
        match item {
            Item::Game(s, g) => Some(self.systems[s].games[g].path.clone()),
            _ => None,
        }
    }

    /// Something in the crate still moves.
    pub(crate) fn crate_animating(&self, now: Instant) -> bool {
        self.screen == Screen::Crate && (self.crate_ui.swap.is_some() || self.crate_ui.pick.as_ref().is_some_and(|p| now < p.moved_at + Duration::from_millis(400)))
    }

    /// A button in the crate (the Shelf look); true when it was the crate's.
    pub(crate) fn crate_press(&mut self, b: Button) -> bool {
        if self.screen != Screen::Crate {
            return false;
        }
        if self.crate_ui.swap.is_some() {
            // A button ends the way: both are where they go at once.
            self.crate_finish();
            return true;
        }
        if let Some((sel, inn)) = self.crate_ui.pick.as_ref().map(|p| (p.sel, p.inn.clone())) {
            let n = self.shelf_space.shelf.len();
            let now = pocket_ui::frame_clock::now();
            let to = match b {
                Button::Left | Button::Up if sel > 0 => Some(sel - 1),
                Button::Right | Button::Down if sel + 1 < n => Some(sel + 1),
                b if b == CONFIRM => {
                    let out = self.crate_shelf().get(sel).and_then(|i| self.path_of(*i));
                    self.crate_start(inn, out);
                    None
                }
                b if b == CANCEL => {
                    self.crate_ui.pick = None;
                    None
                }
                _ => None,
            };
            if let (Some(to), Some(p)) = (to, self.crate_ui.pick.as_mut()) {
                p.sel = to;
                p.moved_at = now;
            }
            self.request_redraw();
            return true;
        }
        if b == CONFIRM {
            let Some(inn) = self.items.get(self.sel).and_then(|i| self.path_of(*i)) else { return true };
            if self.shelf_space.free() > 0 {
                self.crate_start(inn, None);
            } else {
                // The shelf is full: which game makes room? The one played longest ago is offered first.
                let sel = self.crate_stalest();
                self.crate_ui.pick = Some(Pick { inn, sel, moved_at: pocket_ui::frame_clock::now() });
            }
            self.request_redraw();
            return true;
        }
        false
    }

    /// The shelf's game played longest ago (or never), the first offered to make room.
    fn crate_stalest(&self) -> usize {
        let last = |p: &PathBuf| self.recents.list.iter().find(|r| &r.path == p).map_or(0, |r| r.last_played);
        (0..self.shelf_space.shelf.len()).min_by_key(|&i| last(&self.shelf_space.shelf[i])).unwrap_or(0)
    }

    pub(crate) fn crate_start(&mut self, inn: PathBuf, out: Option<PathBuf>) {
        self.crate_ui.pick = None;
        self.crate_ui.swap = Some(Swap { at: pocket_ui::frame_clock::now(), inn, out, shelf_before: self.crate_shelf(), crate_before: self.items.clone() });
        if pocket_ui::motion::span(pocket_ui::motion::Kind::Move, crate_scene::SWAP).is_zero() {
            self.crate_finish();
        }
    }

    /// The swap is done: the shelf as it is now, saved, the crate's list again, the game that went in selected.
    pub(crate) fn crate_finish(&mut self) {
        let Some(swap) = self.crate_ui.swap.take() else { return };
        match &swap.out {
            Some(out) => {
                self.shelf_space.swap(out, &swap.inn);
            }
            None => {
                self.shelf_space.put(&swap.inn);
            }
        }
        self.shelf_space.save(&self.data_dir);
        let went = swap.out.clone();
        self.rebuild();
        if let Some(i) = went.and_then(|p| self.item_of_path(&p)).and_then(|it| self.items.iter().position(|x| *x == it)) {
            self.sel = i;
            self.prev_sel = i;
        }
        let name = self.item_of_path(&swap.inn).map(|i| self.crate_title(i)).unwrap_or_default();
        self.toast = Some((format!("{name} is on the shelf now"), Instant::now()));
        self.request_redraw();
    }

    fn crate_title(&self, item: Item) -> String {
        match item {
            Item::Game(s, g) => self.systems[s].games[g].name.clone(),
            _ => String::new(),
        }
    }

    /// The box picture `w` screen pixels wide at scale `k`, cut at its front's top edge: (back, front).
    /// The crate on Home in the Shelf look (`home_box`): the box, its back and front in one picture, with "Crate" in marker
    /// on its front as the crate's boxes have their system's name.
    pub(crate) fn home_box_pic(&mut self, f: f32) -> Rc<Pic> {
        let (w, k) = home_box(f);
        self.box_pic(w, k)
    }

    /// The crate selected on Home, shown large in the room (`stand_front`): the box as wide as a case's front `fw`, on the
    /// bottom of an `fw` x `fh` picture.
    pub(crate) fn home_box_front(&mut self, fw: i32, fh: i32) -> Rc<Pic> {
        let k = (fw as f32 / crate_scene::ART_W as f32).min(fh as f32 / crate_scene::ART_H as f32);
        let w = ((crate_scene::ART_W as f32 * k).round() as i32).min(fw);
        let pic = self.box_pic(w, k);
        let mut out = Pic::new(fw, fh, 0);
        let (x0, y0) = ((fw - pic.w) / 2, fh - pic.h);
        for y in 0..pic.h {
            for x in 0..pic.w {
                if (0..fh).contains(&(y0 + y)) {
                    out.px[((y0 + y) * fw + x0 + x) as usize] = pic.px[(y * pic.w + x) as usize];
                }
            }
        }
        out.changed();
        Rc::new(out)
    }

    /// The box `w` wide at the picture's scale `k`, its back and front in one picture, "Crate" in marker on its front.
    fn box_pic(&mut self, w: i32, k: f32) -> Rc<Pic> {
        let (back, front) = self.crate_pics(w, k);
        let mut pic = Pic::new(w, back.h + front.h, 0);
        pic.px[..back.px.len()].copy_from_slice(&back.px);
        pic.px[back.px.len()..back.px.len() + front.px.len()].copy_from_slice(&front.px);
        let px = ((front.h as f32) * 0.3).clamp(12.0, 34.0) as u32;
        let side = (crate_scene::ART_FRONT.0 as f32 * k) as i32 + 4;
        let _fixed = pocket_ui::type_scale::fixed();
        let word = self.text.fit(Face::Title, px, "Crate", w - 2 * side);
        let tw = self.text.width(Face::Title, px, &word).max(1);
        let th = (px as i32 * 3 / 2).max(1);
        let mut buf = vec![0u32; (tw + 4) as usize * th as usize];
        {
            let mut c = Canvas::new(&mut buf, (tw + 4) as u32, th as u32);
            self.text.draw(&mut c, Face::Title, px, 2, px as i32, 0xffffff, &word);
        }
        let ink = mix(0x3a2616, pal().void, 30);
        let (x0, y0) = ((w - tw) / 2 - 2, back.h + front.h / 2 - px as i32 * 2 / 3);
        for ty in 0..th {
            for tx in 0..tw + 4 {
                let a = buf[(ty * (tw + 4) + tx) as usize] & 0xff;
                if a > 0 {
                    pic.blend(x0 + tx, y0 + ty, ink, a);
                }
            }
        }
        pic.changed();
        Rc::new(pic)
    }

    fn crate_pics(&mut self, w: i32, k: f32) -> (Rc<Pic>, Rc<Pic>) {
        let key = (w, (k * 1000.0) as i32);
        if let Some((_, p)) = self.crate_ui.pics.iter().find(|(kk, _)| *kk == key) {
            return p.clone();
        }
        let art = crate_art();
        let aw = ((w as f32 / k).round() as i32).max(2 * crate_scene::ART_SIDE + 12);
        let h = (crate_scene::ART_H as f32 * k).round() as i32;
        let rim = (crate_scene::ART_RIM as f32 * k).round() as i32;
        let mut back = Pic::new(w, rim, 0);
        let mut front = Pic::new(w, h - rim, 0);
        for y in 0..h {
            let sy = ((y as f32 / k) as i32).min(crate_scene::ART_H - 1);
            for x in 0..w {
                let sx = art_col((x as f32 * aw as f32 / w as f32) as i32, aw).clamp(0, art.w as i32 - 1);
                let px = art.px[(sy * art.w as i32 + sx) as usize];
                if y < rim {
                    back.px[(y * w + x) as usize] = px;
                } else {
                    front.px[((y - rim) * w + x) as usize] = px;
                }
            }
        }
        self.cabinet.room.lit.tint_all(&mut back.px);
        self.cabinet.room.lit.tint_all(&mut front.px);
        let p = (Rc::new(back), Rc::new(front));
        if self.crate_ui.pics.len() > 24 {
            self.crate_ui.pics.clear();
        }
        self.crate_ui.pics.push((key, p.clone()));
        p
    }

    /// The cases of `items` standing, at scale `f`: their pictures and sizes, and how many of each system follow
    /// each other (the boxes' groups) with the system of each group.
    fn crate_cases(&mut self, items: &[Item], f: f32) -> (Vec<Rc<Pic>>, Vec<Case>, Vec<usize>, Vec<usize>) {
        let (mut pics, mut cases, mut groups, mut systems) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for &item in items {
            let m = self.shelf_medium(item);
            let pic = self.stand_pic(item, m, f);
            cases.push(Case { w: pic.w, h: pic.h });
            pics.push(pic);
            let s = match item {
                Item::Game(s, _) => s,
                _ => usize::MAX,
            };
            if systems.last() == Some(&s) {
                *groups.last_mut().unwrap() += 1;
            } else {
                systems.push(s);
                groups.push(1);
            }
        }
        (pics, cases, groups, systems)
    }

    /// The boxes along the floor (`floor`: the floor's top) of a row scrolled to `scroll`, in view between `left` and
    /// `right`: their backs, then `between` (the cases), then their fronts with the system's name on the tape's left.
    #[allow(clippy::too_many_arguments)]
    fn paint_crates(&mut self, c: &mut Canvas, laid: &Laid, systems: &[usize], floor: i32, ox: i32, left: i32, right: i32, between: &mut dyn FnMut(&mut App, &mut Canvas)) {
        let mut fronts = Vec::new();
        // The floor goes on under the row past the view's ends (a box out of view still stands on it).
        scene_check::surface("the floor", floor, 8, (ox - 16).min(left - 16), (ox + laid.len + 16).max(right + 16));
        for (b, cr) in laid.crates.iter().enumerate() {
            let x = ox + cr.x;
            let inner = crate_scene::inner_floor(cr.k);
            scene_check::surface("a box's floor", floor - inner, 1, x + (crate_scene::ART_FRONT.0 as f32 * cr.k) as i32, x + cr.w - ((crate_scene::ART_W - crate_scene::ART_FRONT.1) as f32 * cr.k) as i32);
            if x + cr.w < left - 40 || x > right + 40 {
                // Out of view; still a thing on the floor for the check.
                scene_check::put(format!("box {b}"), x, floor - (crate_scene::ART_H as f32 * cr.k).round() as i32, cr.w, (crate_scene::ART_H as f32 * cr.k).round() as i32, Rests::Settled, false);
                continue;
            }
            let (back, front) = self.crate_pics(cr.w, cr.k);
            let top = floor - back.h - front.h;
            self.blit_between(c, &back, x, top, 256, (0, c.h), left, right);
            scene_check::set_origin(0, 0);
            scene_check::put(format!("box {b}"), x, top, cr.w, back.h + front.h, Rests::Settled, false);
            fronts.push((x, top + back.h, front, systems.get(cr.group).copied(), cr.k));
        }
        between(self, c);
        for (x, y, front, sys, k) in fronts {
            self.blit_between(c, &front, x, y, 256, (0, c.h), left, right);
            // The system's name in marker on the box, left of the tape.
            if let Some(s) = sys.filter(|s| *s < self.systems.len()) {
                let name = super::system_short(&self.systems, &self.systems[s].name);
                let px = ((front.h as f32) * 0.3).clamp(14.0, 34.0) as u32;
                let side = (crate_scene::ART_FRONT.0 as f32 * k) as i32 + 4;
                let ink = mix(0x3a2616, pal().void, 30);
                self.text.literal(|t| {
                    let w = t.width(Face::Title, px, &name);
                    let at = x + (front.w - w) / 2;
                    if w < front.w - 2 * side && at >= left - 2 && at + w <= right + 2 {
                        t.draw(c, Face::Title, px, at, y + front.h / 2 + px as i32 / 3, ink, &name);
                    }
                });
            }
        }
    }

    /// The crate in the Shelf look (`Screen::Crate`).
    pub(crate) fn paint_crate(&mut self, c: &mut Canvas) {
        self.shelf_media_poll();
        self.room_sync();
        let now = pocket_ui::frame_clock::now();
        if self.crate_ui.swap.as_ref().is_some_and(|s| now >= s.at + s.len()) {
            self.crate_finish();
        }
        if self.crate_ui.pick.is_some() || self.crate_ui.swap.is_some() {
            return self.paint_crate_room(c, now);
        }
        let lay = shelf_layout(c.w, c.h, TALLEST);
        // The floor the boxes stand on: the shelf's board along the bottom, as the floorboards' edge.
        let view = lay.right - lay.left;
        let board = self.shelf_board(view + 32, lay.board_h);
        art::blit(c, &board, lay.left - 16, lay.floor, 256, (0, c.h));
        scene_check::surface("the floor", lay.floor, lay.board_h, lay.left - 16, lay.right + 16);
        let items = self.items.clone();
        if items.is_empty() {
            self.paint_crate_empty(c, &lay);
            return;
        }
        let sel = self.sel.min(items.len() - 1);
        let (pics, cases, groups, systems) = self.crate_cases(&items, lay.f);
        let laid = crate_scene::lay(&groups, &cases, lay.f, (28.0 * lay.f) as i32);
        let ctx = view / 5;
        let want = crate_scene::scroll_to(self.crate_ui.scroll, laid.x[sel], laid.x[sel] + cases[sel].w, laid.len + 16, view, ctx);
        self.crate_ui.scroll = want;
        let ox = lay.left + 8 - want as i32;
        // The selected case comes up out of its box, its spine whole over the box's front.
        let open = self.openness(sel);
        let wall = laid.crates[laid.in_crate[sel]].wall;
        let rise_of = |i: usize, open: f32| ((laid.crates[laid.in_crate[i]].wall as f32 + lay.lift as f32) * open).round() as i32;
        let prev = self.prev_sel.min(items.len() - 1);
        let (left, right) = (lay.left, lay.right);
        self.paint_crates(c, &laid, &systems, lay.floor, ox, left, right, &mut |app, c| {
            for (i, pic) in pics.iter().enumerate() {
                let x = ox + laid.x[i];
                let foot = lay.floor - laid.floor_lift(i);
                let rise = if i == sel { rise_of(i, open) } else if i == prev { rise_of(i, app.openness(i)) } else { 0 };
                let what = format!("case {i}");
                scene_check::set_origin(0, 0);
                if x + pic.w < left || x > right {
                    scene_check::put(what, x, foot - pic.h, pic.w, pic.h, Rests::Settled, true);
                    continue;
                }
                app.blit_between(c, pic, x, foot - pic.h - rise, 256, (0, c.h), left, right);
                let rests = if rise > 0 { Rests::Raised("the selected case, up out of its box") } else { Rests::Settled };
                scene_check::put(what, x, foot - pic.h - rise, pic.w, pic.h, rests, true);
                app.hits.row(Area::new(x, foot - pic.h - rise, pic.w, pic.h - wall / 2), i);
            }
        });
        // The selected game, large, turned to its front, and what it is beside it (as the shelf shows its selection).
        let item = items[sel];
        let sm = self.shelf_medium(item);
        // As high as the tallest case of the crate comes up out of its box: the words never run into it.
        let peak = (0..cases.len()).map(|i| cases[i].h + laid.crates[laid.in_crate[i]].wall + lay.lift + laid.floor_lift(i)).max().unwrap_or(0);
        let (sx, sy, sw, _) = lay.show;
        let sh = (lay.floor - peak - 16 - sy).max(40);
        let show = ShelfLayout { show: (sx, sy, sw, sh), ..lay };
        let k = show_k(&show, sm, &[sm]);
        let (_, front) = self.stand_front(item, sm, k, false);
        if let Some(front) = front {
            let (fx, fy) = (sx, sy + (sh - front.h).max(0) * 2 / 3);
            if pal().void != 0 {
                art::soft_shadow(c, fx + 10, fy + 12, front.w, front.h, 24, 0.45, (0, lay.floor));
            }
            art::blit(c, &front, fx, fy, 256, (sy, sy + sh));
            self.hits.row(Area::new(fx, fy, front.w, front.h), sel);
            let tx = fx + front.w + 48;
            self.paint_shelf_words(c, item, tx, fy.max(sy), sx + sw - tx, sy + sh);
        }
        self.hits.list(sel, items.len(), 30.0, 0.0);
        let _ = (pics.len(), Duration::ZERO);
    }

    /// An empty crate: the box on the floor, empty, and a line saying every game stands on the shelf.
    fn paint_crate_empty(&mut self, c: &mut Canvas, lay: &ShelfLayout) {
        let k = lay.f * 1.6;
        let w = (crate_scene::ART_W as f32 * k * 1.4) as i32;
        let laid = Laid { crates: vec![crate_scene::Crate { x: 0, w, wall: ((crate_scene::ART_H - crate_scene::ART_RIM) as f32 * k) as i32, k, cases: 0..0, group: usize::MAX }], x: vec![], in_crate: vec![], len: w };
        let ox = (c.w - w) / 2;
        self.paint_crates(c, &laid, &[], lay.floor, ox, lay.left, lay.right, &mut |_, _| {});
        let (sx, sy, _, _) = lay.show;
        self.text.draw(c, Face::Title, 44, sx, sy + 70, pal().chalk, "The crate is empty");
        self.text.draw(c, Face::Body, 28, sx, sy + 116, pal().dust, "Every game is on the shelf.");
    }

    /// The room: the shelf over the boxes, while the player picks the game that makes room and while the two swap.
    fn paint_crate_room(&mut self, c: &mut Canvas, now: Instant) {
        let lay = shelf_layout(c.w, c.h, TALLEST);
        let f = lay.f;
        let (left, right) = (lay.left, lay.right);
        let view = right - left;
        let shelf_items = match &self.crate_ui.swap {
            Some(s) => s.shelf_before.clone(),
            None => self.crate_shelf(),
        };
        let crate_items = match &self.crate_ui.swap {
            Some(s) => s.crate_before.clone(),
            None => self.items.clone(),
        };
        let inn = self.crate_ui.swap.as_ref().map(|s| s.inn.clone()).or_else(|| self.crate_ui.pick.as_ref().map(|p| p.inn.clone()));
        let out = self.crate_ui.swap.as_ref().and_then(|s| s.out.clone());
        let inn_item = inn.as_ref().and_then(|p| self.item_of_path(p));
        let out_item = out.as_ref().and_then(|p| self.item_of_path(p));
        let t = self.crate_ui.swap.as_ref().map_or(0.0, |s| (now.saturating_duration_since(s.at).as_secs_f32() / s.len().as_secs_f32().max(0.001)).min(1.0));
        let st = crate_scene::steps(t, out.is_some());
        let (m_out, m_in) = (st.out, st.inn);
        let swapping = self.crate_ui.swap.is_some();

        // The crate after: the one going in at its place by system and name, the one coming out gone.
        let mut crate_after: Vec<Item> = crate_items.iter().copied().filter(|i| Some(*i) != inn_item).collect();
        if let Some(o) = out_item {
            crate_after.push(o);
            crate_after.sort_by_key(|i| match i {
                Item::Game(s, g) => (*s, *g),
                _ => (usize::MAX, 0),
            });
        }
        // Both, for boxes that stay where they are all through.
        let mut both = crate_items.clone();
        if let Some(o) = out_item {
            if !both.contains(&o) {
                both.push(o);
            }
        }
        both.sort_by_key(|i| match i {
            Item::Game(s, g) => (*s, *g),
            _ => (usize::MAX, 0),
        });
        let (pics, cases, groups, systems) = self.crate_cases(&both, f);
        let laid = crate_scene::lay(&groups, &cases, f, (28.0 * f) as i32);
        // Each case's place before and after, packed from its box's left side.
        let place = |present: &dyn Fn(Item) -> bool| -> Vec<(u64, i32)> {
            let mut out = Vec::new();
            for cr in &laid.crates {
                let mut x = laid.x[cr.cases.start];
                for i in cr.cases.clone() {
                    if present(both[i]) {
                        out.push((i as u64, x));
                        x += cases[i].w;
                    }
                }
            }
            out
        };
        let before = place(&|i| crate_items.contains(&i));
        let after = place(&|i| crate_after.contains(&i));
        let mid = place(&|i| both.contains(&i));
        let crate_x = if st.crate_close > 0.0 { crate_scene::slide(&mid, &after, st.crate_close) } else { crate_scene::slide(&before, &mid, st.crate_open) };
        let idx_of = |it: Option<Item>| it.and_then(|it| both.iter().position(|x| *x == it));
        let (inn_i, out_i) = (idx_of(inn_item), idx_of(out_item));
        let x_in = |list: &[(u64, i32)], i: Option<usize>| i.and_then(|i| list.iter().find(|(j, _)| *j == i as u64).map(|p| p.1));
        // The crate's scroll: on the case coming out, then on the place of the one going in.
        let ctx = view / 4;
        let from_scroll = x_in(&before, inn_i).map_or(0.0, |x| crate_scene::scroll_to(0.0, x, x + 30, laid.len + 16, view, ctx));
        let to_scroll = x_in(&after, out_i).map_or(from_scroll, |x| crate_scene::scroll_to(from_scroll, x, x + 30, laid.len + 16, view, ctx));
        let back_scroll = x_in(&mid, inn_i).map_or(to_scroll, |x| crate_scene::scroll_to(to_scroll, x, x + 30, laid.len + 16, view, ctx));
        let scroll = from_scroll + (to_scroll - from_scroll) * st.scroll_go + (back_scroll - to_scroll) * st.scroll_back;
        let ox = lay.left + 8 - scroll.round() as i32;

        // The shelf: its board high enough that its tallest case and the lift fit under the title; the boxes on the floor.
        let shelf_pics: Vec<Rc<Pic>> = shelf_items.iter().map(|&i| { let m = self.shelf_medium(i); self.stand_pic(i, m, f) }).collect();
        let inn_pic = inn_item.map(|i| { let m = self.shelf_medium(i); self.stand_pic(i, m, f) });
        let tallest = shelf_pics.iter().chain(inn_pic.iter()).map(|p| p.h).max().unwrap_or(0);
        let top = lay.show.1;
        let board_y = top + tallest + lay.lift + 8;
        let board = self.shelf_board(view + 32, lay.board_h);
        art::blit(c, &board, lay.left - 16, board_y, 256, (0, c.h));
        if pal().void != 0 {
            art::soft_shadow(c, lay.left - 16, board_y + lay.board_h, view + 32, 10, 12, 0.5, (0, c.h));
        }
        let floor_board = self.shelf_board(view + 32, lay.board_h);
        art::blit(c, &floor_board, lay.left - 16, lay.floor, 256, (0, c.h));
        scene_check::set_origin(0, 0);
        scene_check::surface("the shelf's board", board_y, lay.board_h, lay.left - 16, lay.right + 16);
        scene_check::surface("the floor", lay.floor, lay.board_h, lay.left - 16, lay.right + 16);

        // The shelf's row before and after: the new game in the old one's place (or at the end).
        let mut shelf_after = shelf_items.clone();
        match (out_item, inn_item) {
            (Some(o), Some(i)) => {
                if let Some(k) = shelf_after.iter().position(|x| *x == o) {
                    shelf_after[k] = i;
                }
            }
            (None, Some(i)) if swapping => shelf_after.push(i),
            _ => {}
        }
        let lead = 16;
        let shelf_place = |list: &[Item]| -> Vec<(u64, i32)> {
            let mut x = lead;
            list.iter()
                .map(|it| {
                    let w = if Some(*it) == inn_item { inn_pic.as_ref().map_or(0, |p| p.w) } else { shelf_items.iter().position(|s| s == it).map_or(0, |k| shelf_pics[k].w) };
                    let at = (item_id(*it), x);
                    x += w;
                    at
                })
                .collect()
        };
        let (sb, sa) = (shelf_place(&shelf_items), shelf_place(&shelf_after));
        let shelf_x = crate_scene::slide(&sb, &sa, st.shelf_slide);
        let shelf_len = sa.last().map_or(0, |l| l.1) + 60;
        let pick_sel = self.crate_ui.pick.as_ref().map(|p| p.sel);
        let pick_at = self.crate_ui.pick.as_ref().map(|p| p.moved_at);
        let focus = out_item.and_then(|o| shelf_items.iter().position(|x| *x == o)).or(pick_sel).unwrap_or(0);
        let fx = sb.get(focus).map_or(0, |p| p.1);
        let sscroll = crate_scene::scroll_to(0.0, fx, fx + 30, shelf_len, view, view / 4);
        let sox = lay.left - sscroll as i32;

        // The two on their way: their feet where they stood and where they go, at this moment.
        let crate_foot = |x: i32, i: usize| ((ox + x) as f32, (lay.floor - laid.floor_lift(i)) as f32);
        let inn_way = inn_i.zip(inn_pic.clone()).map(|(i, pic)| {
            // Where it stands in its box: before the other one went in, or after (a box it shares with it made room).
            let x = if st.crate_open > 0.0 { x_in(&mid, Some(i)) } else { x_in(&before, Some(i)) };
            let from = crate_foot(x.unwrap_or(0), i);
            let clear_from = (laid.crates[laid.in_crate[i]].wall + lay.lift) as f32;
            let to_x = sa.iter().find(|p| Some(p.0) == inn_item.map(item_id)).map_or(0, |p| p.1);
            let to = ((sox + to_x) as f32, board_y as f32);
            // Waiting for the game that makes room, it is half out of its box.
            let held = if swapping { m_in } else { crate_scene::Moment { up: 0.55 * pocket_ui::motion::progress(pocket_ui::motion::Kind::Move, pick_at.unwrap_or(now), now, Duration::from_millis(300)), across: 0.0, down: 0.0, slide: 0.0 } };
            (crate_scene::way(held, from, clear_from, to, lay.lift as f32, 24.0 * f), pic)
        });
        let out_way = out_i.zip(out_item).map(|(i, o)| {
            let k = shelf_items.iter().position(|x| *x == o).unwrap_or(0);
            let pic = shelf_pics[k].clone();
            let from = ((sox + sb[k].1) as f32, board_y as f32);
            // Where it goes in its box, and where it slides with the others once the gap of the one that came out closes.
            let to = crate_foot(x_in(&crate_x, Some(i)).or(x_in(&mid, Some(i))).unwrap_or(0), i);
            let clear_to = (laid.crates[laid.in_crate[i]].wall + lay.lift) as f32;
            (crate_scene::way(m_out, from, lay.lift as f32, to, clear_to, 24.0 * f), pic)
        });
        // In a box (coming up out of it, or going down into it) a case is behind the box's front.
        let inn_in_box = !swapping || m_in.up < 1.0;
        let out_in_box = swapping && m_out.down > 0.0;

        // The boxes and the cases in them.
        let (lo, ro) = (left, right);
        self.paint_crates(c, &laid, &systems, lay.floor, ox, lo, ro, &mut |app, c| {
            scene_check::set_origin(0, 0);
            for &(id, x) in &crate_x {
                let i = id as usize;
                if Some(i) == inn_i || Some(i) == out_i {
                    continue;
                }
                let (x, foot) = (ox + x, lay.floor - laid.floor_lift(i));
                scene_check::put(format!("case {i}"), x, foot - pics[i].h, pics[i].w, pics[i].h, Rests::Settled, true);
                if x + pics[i].w >= lo && x <= ro {
                    app.blit_between(c, &pics[i], x, foot - pics[i].h, 256, (0, c.h), lo, ro);
                }
            }
            if let (true, Some(((x, y), pic))) = (inn_in_box, inn_way.as_ref()) {
                app.blit_between(c, pic, *x as i32, *y as i32 - pic.h, 256, (0, c.h), lo, ro);
                let why = if swapping && m_in.up >= 1.0 { Rests::Held("in the hand, from the crate to the shelf") } else { Rests::Raised("taken up out of its box, waiting for its place") };
                scene_check::put("the game coming out", *x as i32, *y as i32 - pic.h, pic.w, pic.h, why, true);
            }
            if let (true, Some(((x, y), pic))) = (out_in_box, out_way.as_ref()) {
                app.blit_between(c, pic, *x as i32, *y as i32 - pic.h, 256, (0, c.h), lo, ro);
                let why = if m_out.down >= 1.0 { Rests::Settled } else { Rests::Held("in the hand, from the shelf to the crate") };
                scene_check::put("the game going in", *x as i32, *y as i32 - pic.h, pic.w, pic.h, why, true);
            }
        });

        // The shelf's row.
        for (k, &(id, x)) in shelf_x.iter().enumerate() {
            let Some(j) = shelf_items.iter().position(|it| item_id(*it) == id) else { continue };
            if Some(shelf_items[j]) == out_item {
                continue;
            }
            let pic = &shelf_pics[j];
            let x = sox + x;
            let rise = match (pick_sel, pick_at) {
                (Some(p), Some(at)) if p == j => (2.0 * lay.lift as f32 * pocket_ui::motion::progress(pocket_ui::motion::Kind::Move, at, now, Duration::from_millis(240))).round() as i32,
                _ => 0,
            };
            let rests = if rise > 0 { Rests::Raised("the game picked to make room, up out of the row") } else { Rests::Settled };
            scene_check::put(format!("shelf case {k}"), x, board_y - pic.h - rise, pic.w, pic.h, rests, true);
            if x + pic.w >= left && x <= right {
                self.blit_between(c, pic, x, board_y - pic.h - rise, 256, (0, c.h), left, right);
                self.hits.row(Area::new(x, board_y - pic.h - rise, pic.w, pic.h), j);
            }
        }
        if let (false, Some(((x, y), pic))) = (out_in_box, out_way.as_ref()) {
            self.blit_between(c, pic, *x as i32, *y as i32 - pic.h, 256, (0, c.h), left, right);
            scene_check::put("the game going in", *x as i32, *y as i32 - pic.h, pic.w, pic.h, Rests::Held("in the hand, from the shelf to the crate"), true);
        }
        if let (false, Some(((x, y), pic))) = (inn_in_box, inn_way.as_ref()) {
            let why = if m_in.down >= 1.0 { Rests::Settled } else { Rests::Held("in the hand, from the crate to the shelf") };
            self.blit_between(c, pic, *x as i32, *y as i32 - pic.h, 256, (0, c.h), left, right);
            scene_check::put("the game coming out", *x as i32, *y as i32 - pic.h, pic.w, pic.h, why, true);
        }

        // Between the shelf and the boxes: which two change places.
        if !swapping {
            let name = |app: &App, it: Option<Item>| it.map(|i| app.crate_title(i)).unwrap_or_default();
            let picked = pick_sel.and_then(|p| shelf_items.get(p).copied());
            let line = match picked {
                Some(p) => format!("{} goes into the crate for {}", name(self, Some(p)), name(self, inn_item)),
                None => format!("{} goes onto the shelf", name(self, inn_item)),
            };
            // Two lines at most, inside the shelf's ends: a long pair of names in Spanish, Portuguese or Italian ran past
            // the right edge at Large text on 1024x768 (text-fit, 20.11.).
            let lh = pocket_ui::type_scale::px(26) as i32 * 13 / 10;
            let ly = board_y + lay.board_h + 34;
            for (k, l) in self.text.wrap_prose_at_most(Face::Body, 26, &line, right - left, 2).iter().enumerate() {
                self.text.draw(c, Face::Body, 26, left, ly + k as i32 * lh, pal().chalk, l);
            }
        }
    }
}

impl App {
    /// Shelf space switched on with `n` places (or to `n` from another number), filled as `fill` says: the games played
    /// last or the favourites first, then by name. Saved; Home and the rows follow.
    pub(crate) fn shelf_space_fill(&mut self, n: u32, fill: pocket_ui::shelf_space::Fill) {
        let recent: Vec<PathBuf> = self.recents.list.iter().map(|r| r.path.clone()).collect();
        let favourites: Vec<PathBuf> = self.favorites.list.iter().map(|f| f.path.clone()).collect();
        let mut all: Vec<(String, PathBuf)> = self.systems.iter().flat_map(|s| s.games.iter().map(|g| (g.name.to_lowercase(), g.path.clone()))).collect();
        all.sort();
        let all: Vec<PathBuf> = all.into_iter().map(|a| a.1).collect();
        let on_card: std::collections::HashSet<&PathBuf> = all.iter().collect();
        let order: Vec<PathBuf> = pocket_ui::shelf_space::order(fill, &recent, &favourites, &all).into_iter().filter(|p| on_card.contains(p)).collect();
        self.shelf_space.forget_missing(|p| on_card.contains(&p.to_path_buf()));
        self.shelf_space.switch_on(n, order);
        self.shelf_space.save(&self.data_dir);
        self.shelf_space_changed();
    }

    /// Home's rows, the systems' rows and the crate after the shelf changed.
    fn shelf_space_changed(&mut self) {
        let at = self.items.get(self.sel).copied();
        self.rebuild();
        if let Some(i) = at.and_then(|at| self.items.iter().position(|x| *x == at)) {
            self.sel = i;
            self.prev_sel = i;
        }
        self.request_redraw();
    }

    /// Settings → Games → Shelf space's value: "Off", or the places.
    pub(crate) fn shelf_space_value(&self) -> String {
        self.shelf_space.limit.map_or_else(|| "Off".to_string(), |n| n.to_string())
    }

    /// Left/Right (A: forward) on Settings → Games → Shelf space: Off, 10, 20, 50. Off to a number asks which games go on
    /// the shelf (the dialog, `rescue::DialogKind::ShelfSpace`); a number to another keeps what is on the shelf (as far as
    /// it fits, filling up with the games played last); Off puts every game back on the shelf.
    pub(crate) fn shelf_space_step(&mut self, forward: bool) {
        let steps: Vec<Option<u32>> = std::iter::once(None).chain(pocket_ui::shelf_space::CHOICES.iter().map(|n| Some(*n))).collect();
        let at = steps.iter().position(|s| *s == self.shelf_space.limit).unwrap_or(0);
        let next = steps[if forward { (at + 1) % steps.len() } else { (at + steps.len() - 1) % steps.len() }];
        match (self.shelf_space.limit, next) {
            (_, None) => {
                self.shelf_space.switch_off();
                self.shelf_space.save(&self.data_dir);
                self.shelf_space_changed();
            }
            (None, Some(n)) => {
                self.leave = Some(pocket_ui::rescue::Dialog { sel: 1, ..pocket_ui::rescue::Dialog::new(pocket_ui::rescue::DialogKind::ShelfSpace(n)) });
                self.request_redraw();
            }
            (Some(_), Some(n)) => self.shelf_space_fill(n, pocket_ui::shelf_space::Fill::Recent),
        }
        self.save_settings();
    }

    /// The game card's "Into the crate" row: only while Shelf space is on and the game stands on the shelf.
    pub(crate) fn card_crate_row(&self, path: &std::path::Path) -> Option<pocket_ui::game_card_model::CuratorRow> {
        (self.shelf_space.on() && self.shelf_space.shelf.iter().any(|p| p == path)).then(|| pocket_ui::game_card_model::CuratorRow {
            kind: pocket_ui::game_card_model::RowKind::Crate,
            title: "Into the crate".into(),
            line: "Off the shelf, its place free for another game".into(),
        })
    }

    /// A on "Into the crate": the game goes into the crate, its place on the shelf is free; the card closes.
    pub(crate) fn card_into_crate(&mut self) {
        let Some(path) = self.card.as_ref().map(|o| o.path.clone()) else { return };
        if !self.shelf_space.put_away(&path) {
            return;
        }
        self.shelf_space.save(&self.data_dir);
        let name = self.item_of_path(&path).map(|i| self.crate_title(i)).unwrap_or_default();
        self.card = None;
        self.shelf_space_changed();
        self.toast = Some((format!("{name} is in the crate now"), Instant::now()));
    }

    /// The crate's hints: A takes the game out (or, picking, swaps), B goes back (or stops picking), X its card.
    pub(crate) fn crate_hints(&self, hints: Vec<(&'static str, &'static str)>) -> Vec<(&'static str, &'static str)> {
        if self.crate_ui.swap.is_some() {
            return vec![];
        }
        if self.crate_ui.pick.is_some() {
            return vec![("A", "Swap"), ("B", "Cancel")];
        }
        if self.items.is_empty() {
            return vec![("B", "Back")];
        }
        let take = if self.shelf_space.free() > 0 { "Onto the shelf" } else { "Swap" };
        let mut out = vec![("A", take), ("B", "Back")];
        out.extend(hints.into_iter().filter(|h| h.0 == "X" || h.0 == "START"));
        out
    }

    /// `--snapshot crate:<row>[:pick:<k>|:swap:<k>:<ms>|:home|:ask|:clip:<k>|:rests]`: the crate in the Shelf look on the snapshot's
    /// library, with Shelf space on (`POCKETUI_SNAPSHOT_SHELF_SPACE`, 10 places when not given), game `<row>` of the crate
    /// selected; `pick:<k>`: making room, the shelf's game `<k>` picked; `swap:<k>:<ms>`: `<ms>` into the swap with it
    /// (`<k>` = `free`: onto a free place); `home`: Home with the Crate row selected; `ask`: the question of switching it on; `clip:<k>`: the whole way as 60 frames a second into `<out>.rgb` (raw `bgr0`);
    /// `rests`: the physics check of every selection, every pick and swaps at twenty moments each (`scene_check`).
    pub(crate) fn crate_snapshot(&mut self, rest: &str, w: u32, h: u32, out: &str) {
        self.shelf_snapshot(":shelf");
        // `POCKETUI_SNAPSHOT_SHELF_SPACE`: the places (10 when not given), filled with the games played last.
        let n = std::env::var("POCKETUI_SNAPSHOT_SHELF_SPACE").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
        self.shelf_space_fill(n, pocket_ui::shelf_space::Fill::Recent);
        self.cabinet.inline = true;
        self.screen = super::Screen::Crate;
        self.rebuild();
        let parts: Vec<&str> = rest.split(':').collect();
        let row: usize = parts.first().and_then(|r| r.parse().ok()).unwrap_or(0);
        self.sel = row.min(self.items.len().saturating_sub(1));
        self.prev_sel = self.sel;
        self.moved_at = Instant::now() - Duration::from_secs(2);
        let mut buf = vec![0u32; (w * h) as usize];
        let base = Instant::now();
        let inn = self.items.get(self.sel).and_then(|i| self.path_of(*i));
        let out_of = |app: &App, k: &str| -> Option<PathBuf> { k.parse::<usize>().ok().and_then(|k| app.shelf_space.shelf.get(k).cloned()) };
        match parts.get(1).copied() {
            Some("pick") => {
                let k = parts.get(2).and_then(|k| k.parse().ok()).unwrap_or(0);
                self.crate_ui.pick = inn.map(|inn| Pick { inn, sel: k, moved_at: pocket_ui::frame_clock::now() - Duration::from_secs(2) });
            }
            Some("swap") => {
                let ms: u64 = parts.get(3).and_then(|m| m.parse().ok()).unwrap_or(0);
                let out = parts.get(2).and_then(|k| out_of(self, k));
                if out.is_none() {
                    self.shelf_space.limit = Some(self.shelf_space.shelf.len() as u32 + 1);
                }
                if let Some(inn) = inn {
                    self.crate_start(inn, out);
                }
                if let Some(s) = self.crate_ui.swap.as_mut() {
                    s.at = base;
                }
                pocket_ui::frame_clock::freeze(base + Duration::from_millis(ms));
            }
            // Home with the Crate row selected; the question that comes with switching it on.
            Some("home") => {
                self.screen = super::Screen::Home;
                self.rebuild();
                self.sel = self.items.iter().position(|i| *i == Item::Crate).unwrap_or(0);
                self.prev_sel = self.sel;
            }
            // Home with the crate: every selection checked for what rests where, and the crate's size as recorded.
            Some("home-rests") => {
                self.screen = super::Screen::Home;
                self.rebuild();
                return self.home_rests(w, h);
            }
            Some("ask") => {
                self.leave = Some(pocket_ui::rescue::Dialog { sel: 1, ..pocket_ui::rescue::Dialog::new(pocket_ui::rescue::DialogKind::ShelfSpace(n)) });
            }
            Some("clip") => return self.crate_clip(parts.get(2).copied().unwrap_or("0"), w, h, out),
            Some("rests") => return self.crate_rests(w, h),
            _ => {}
        }
        let mut c = super::snap_canvas(&mut buf, w, h);
        self.paint(&mut c);
        // The question over the page, as the real draw puts it (`paint_leave`).
        self.paint_leave(&mut c);
        super::write_png(&buf, w, h, out);
        pocket_ui::frame_clock::thaw();
    }

    /// The clip: the crate, two moves to the right, A, the shelf's stalest game picked, one to the right, A, the swap,
    /// and a moment after; 60 frames a second on the frame clock, into `<out>.rgb`.
    fn crate_clip(&mut self, k: &str, w: u32, h: u32, out: &str) {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(format!("{out}.rgb")).expect("clip file"));
        let mut buf = vec![0u32; (w * h) as usize];
        let base = Instant::now() + Duration::from_secs(3600);
        let free = k == "free";
        if free {
            self.shelf_space.limit = Some(self.shelf_space.shelf.len() as u32 + 1);
        }
        // (frame, button)
        let script: &[(u32, Button)] = &[(40, Button::Right), (70, Button::Right), (130, CONFIRM), (190, Button::Right), (250, CONFIRM)];
        let end = 250 + (pocket_ui::motion::span(pocket_ui::motion::Kind::Move, crate_scene::SWAP).as_millis() as u32 * 60 / 1000) + 90;
        let mut moved_frame = 0u32;
        for frame in 0..end {
            let t = base + Duration::from_micros(frame as u64 * 16_667);
            pocket_ui::frame_clock::freeze(t);
            for &(at, b) in script {
                if at == frame {
                    if b == Button::Right && self.crate_ui.pick.is_none() && self.crate_ui.swap.is_none() {
                        self.prev_sel = self.sel;
                        self.sel = (self.sel + 1).min(self.items.len().saturating_sub(1));
                        moved_frame = frame;
                    } else {
                        self.crate_press(b);
                    }
                    if let Some(p) = self.crate_ui.pick.as_mut() {
                        if p.moved_at > t || frame == at {
                            p.moved_at = t;
                        }
                    }
                    if let Some(s) = self.crate_ui.swap.as_mut() {
                        if frame == at {
                            s.at = t;
                        }
                    }
                }
            }
            // The lift's clock is the real one (`pull_progress`): the move as long ago as on the frame clock.
            self.moved_at = Instant::now() - Duration::from_micros((frame - moved_frame) as u64 * 16_667) - if moved_frame == 0 { Duration::from_secs(2) } else { Duration::ZERO };
            self.paint(&mut super::snap_canvas(&mut buf, w, h));
            // Painted natively, the panel's picture is at the buffer's front (`snap_canvas`).
            let n = super::snapshot_native(w, h).map_or((w * h) as usize, |p| (p.0 * p.1) as usize);
            let bytes: Vec<u8> = buf[..n].iter().flat_map(|p| p.to_le_bytes()).collect();
            file.write_all(&bytes).expect("clip frame");
        }
        pocket_ui::frame_clock::thaw();
        let (pw, ph) = super::snapshot_native(w, h).unwrap_or((w, h));
        println!("crate clip frames={end} size={pw}x{ph}");
    }

    /// Nothing floats in the crate (`scene_check`, tests/scene_rests.rs): every selection of the crate at eight moments of
    /// its way up; picking each game of the shelf; a swap with the first, the middle and the last game of the shelf and
    /// one onto a free place, twenty moments each. Prints `rests frames=… faults=…`, what was seen and each fault.
    /// `crate:<row>:home-rests`: Home in the Shelf look with Shelf space on, every selection at five moments of its way up,
    /// checked by `scene_check`; and the crate as it stands in the row (`home crate <w>x<h>`): the box, wider than high.
    fn home_rests(&mut self, w: u32, h: u32) {
        let mut buf = vec![0u32; (w * h) as usize];
        scene_check::record(true);
        let (mut frames, mut faults, mut sizes) = (0, Vec::new(), Vec::new());
        let at = self.items.iter().position(|i| *i == Item::Crate);
        for i in 0..self.items.len() {
            self.prev_sel = self.sel;
            self.sel = i;
            for ms in [0u64, 60, 160, 400, 800] {
                self.moved_at = Instant::now() - Duration::from_millis(ms);
                scene_check::take();
                self.paint(&mut super::snap_canvas(&mut buf, w, h));
                frames += 1;
                let (objs, surfaces) = scene_check::take();
                if let Some(o) = at.and_then(|c| objs.iter().find(|o| o.what == format!("case {c}") || o.what == format!("shelf 0 case {c}"))) {
                    sizes.push((o.w, o.h));
                }
                for f in scene_check::check(&objs, &surfaces) {
                    faults.push(format!("home, row {i} selected, {ms} ms: {f}"));
                }
            }
        }
        scene_check::record(false);
        println!("rests frames={frames} faults={}", faults.len());
        if let Some((cw, ch)) = sizes.iter().max_by_key(|s| s.0) {
            println!("home crate {cw}x{ch}");
        }
        for f in faults.iter().take(40) {
            println!("rests fault: {f}");
        }
    }

    fn crate_rests(&mut self, w: u32, h: u32) {
        let mut buf = vec![0u32; (w * h) as usize];
        scene_check::record(true);
        let (mut frames, mut faults) = (0, Vec::new());
        let mut kinds = std::collections::BTreeSet::new();
        let mut check = |app: &mut App, what: String, frames: &mut u32, faults: &mut Vec<String>, buf: &mut Vec<u32>| {
            scene_check::take();
            app.paint(&mut super::snap_canvas(buf, w, h));
            *frames += 1;
            let (objs, surfaces) = scene_check::take();
            kinds.extend(objs.iter().map(|o| o.what.replace(|c: char| c.is_ascii_digit(), "#").replace("##", "#").replace("##", "#")));
            kinds.extend(surfaces.iter().map(|s| format!("surface: {}", s.what)));
            for f in scene_check::check(&objs, &surfaces) {
                faults.push(format!("{what}: {f}"));
            }
        };
        let base = Instant::now();
        // Every selection, eight moments of its way up out of the box.
        for i in 0..self.items.len() {
            self.prev_sel = self.sel;
            self.sel = i;
            for ms in [0u64, 30, 60, 100, 160, 250, 400, 800] {
                self.moved_at = Instant::now() - Duration::from_millis(ms);
                check(self, format!("crate case {i} selected, {ms} ms"), &mut frames, &mut faults, &mut buf);
            }
        }
        self.moved_at = Instant::now() - Duration::from_secs(2);
        let n = self.shelf_space.shelf.len();
        let rows = [0, self.items.len() / 2, self.items.len().saturating_sub(1)];
        for &row in &rows {
            self.sel = row.min(self.items.len().saturating_sub(1));
            let Some(inn) = self.items.get(self.sel).and_then(|i| self.path_of(*i)) else { continue };
            for k in 0..n {
                self.crate_ui.pick = Some(Pick { inn: inn.clone(), sel: k, moved_at: base - Duration::from_secs(2) });
                pocket_ui::frame_clock::freeze(base);
                check(self, format!("crate case {row} out, shelf game {k} picked"), &mut frames, &mut faults, &mut buf);
            }
            self.crate_ui.pick = None;
            let saved = self.shelf_space.clone();
            for k in [Some(0), Some(n / 2), Some(n.saturating_sub(1)), None] {
                let out = k.and_then(|k| self.shelf_space.shelf.get(k).cloned());
                if out.is_none() {
                    self.shelf_space.limit = Some(n as u32 + 1);
                }
                self.crate_start(inn.clone(), out);
                for step in 0..=20u32 {
                    let len = self.crate_ui.swap.as_ref().map_or(Duration::ZERO, |s| s.len());
                    if let Some(s) = self.crate_ui.swap.as_mut() {
                        s.at = base;
                    }
                    // Short of its end: the end itself finishes the swap (the next frame is the crate again).
                    pocket_ui::frame_clock::freeze(base + len.mul_f32(step as f32 / 20.0) - if step == 20 { Duration::from_millis(1) } else { Duration::ZERO });
                    check(self, format!("crate case {row} swapped with shelf game {k:?}, {}0 %", step / 2), &mut frames, &mut faults, &mut buf);
                }
                self.crate_ui.swap = None;
                self.shelf_space = saved.clone();
            }
        }
        pocket_ui::frame_clock::thaw();
        scene_check::record(false);
        println!("rests frames={frames} faults={}", faults.len());
        println!("rests seen: {}", kinds.into_iter().collect::<Vec<_>>().join(" | "));
        for f in faults.iter().take(40) {
            println!("rests fault: {f}");
        }
    }
}

/// A game's id in a row (its system and place).
fn item_id(i: Item) -> u64 {
    match i {
        Item::Game(s, g) => ((s as u64) << 32) | g as u64,
        _ => u64::MAX,
    }
}

use pocket_ui::gfx::{mix, Canvas};

#[cfg(test)]
mod big_card_tests {
    use super::*;
    use pocket_ui::library::{Game, System};

    fn card(systems: &[(&str, usize)]) -> Vec<System> {
        systems
            .iter()
            .map(|&(name, n)| System {
                name: name.into(),
                fullname: name.into(),
                emulator: None,
                core: None,
                emulators: Vec::new(),
                games: (0..n).map(|i| Game { name: format!("{name} {i:06}"), path: format!("/storage/roms/{name}/Game {i:06}.zip").into() }).collect(),
            })
            .collect()
    }

    /// The crate room asks for the shelf's games on every frame (`paint_crate_room`). The R36S card: 179,000 games, a
    /// full shelf of 500. A search through every game per place was 90 million path comparisons a frame; one pass is
    /// cheap. Thread CPU, so a loaded host doesn't fail it.
    #[test]
    fn the_shelf_of_a_big_card_is_found_in_one_pass() {
        let systems = card(&[("zxspectrum", 22_000), ("snes", 13_500), ("gba", 6_000), ("arcade", 137_000)]);
        let paths: Vec<PathBuf> = systems.iter().flat_map(|s| s.games.iter().step_by(357).map(|g| g.path.clone())).take(500).collect();
        assert_eq!(paths.len(), 500);
        let t = crate::app_cabinet::thread_cpu();
        let items = items_in_order(&systems, &paths);
        let took = crate::app_cabinet::thread_cpu() - t;
        assert_eq!(items.len(), 500);
        assert!(took < Duration::from_millis(500), "500 shelf places on 178,500 games took {took:?} of CPU");
    }

    /// In the shelf's order, not the card's; a game gone from the card is left out; the same game in two places is
    /// the first one, as the search by path found it.
    #[test]
    fn the_shelf_keeps_its_own_order_and_leaves_out_what_the_card_lost() {
        let systems = card(&[("gba", 3), ("snes", 2)]);
        let p = |s: &str, i: usize| PathBuf::from(format!("/storage/roms/{s}/Game {i:06}.zip"));
        let shelf = [p("snes", 1), p("gba", 2), p("nes", 0), p("gba", 0)];
        assert_eq!(items_in_order(&systems, &shelf), [Item::Game(1, 1), Item::Game(0, 2), Item::Game(0, 0)]);
        assert!(items_in_order(&systems, &[]).is_empty());
        let mut twice = systems.clone();
        let again = twice[0].games[1].clone();
        twice[1].games.push(again);
        assert_eq!(items_in_order(&twice, &[p("gba", 1)]), [Item::Game(0, 1)]);
    }
}
