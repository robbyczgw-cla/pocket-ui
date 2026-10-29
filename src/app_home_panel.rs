//! Part of the pocket-ui binary: the picture side of Home's rows that are not one game (`pocket_ui::home_panel`,
//! docs/DESIGN.md "Home panels"). Curator's day's pick and the games waiting for you, the covers of Favorites and of
//! the smart shelves and stickers pinned to Home, at the right end of the pulled-out label (Shelf) and in Split's panel.
//!
//! * Everything it shows is in memory already: `curator.today` and the shelves' first games come from the index
//!   thread and `curator_counts`, the favourites from their list. Nothing is worked out here but the tiles.
//! * Covers are painted from the image cache only: a cover not decoded yet is asked of the picture loader's thread
//!   (`App::picture`, as Split's preview does) and a faint tile stands in its place until it arrives. Which file a
//!   game's box art is (a few `stat`s) is looked up once per game and kept (`App::cover_file`, the cabinet's).

use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::home_panel::{self, CuratorToday, List, Panel, Tile};
use pocket_ui::palette::pal;
use pocket_ui::text::Face;

use super::{stripe, App, Item};

/// The colours a panel is painted with: the captions', and the page under it (the label's paper, Split's panel).
pub(crate) struct Ink {
    pub caption: u32,
    pub under: u32,
}

impl App {
    /// A game of the catalog as a panel's tile, while the library still has it.
    pub(crate) fn catalog_tile(&self, game: usize) -> Option<Tile> {
        match self.curator_item(game)? {
            Item::Game(s, g) => self.game_tile(s, g),
            _ => None,
        }
    }

    fn game_tile(&self, s: usize, g: usize) -> Option<Tile> {
        let (sys, game) = (self.systems.get(s)?, self.systems.get(s)?.games.get(g)?);
        Some(Tile { title: game.name.clone(), system: sys.name.clone(), rom: game.path.clone() })
    }

    /// What Curator's index offers today, as tiles; `None` before the index.
    pub(crate) fn curator_today(&self) -> Option<CuratorToday> {
        self.curator.index.as_ref()?;
        let t = &self.curator.today;
        let pick = t.pick.as_ref().and_then(|p| Some((self.catalog_tile(p.game)?, p.reason.clone())));
        let waiting = t.waiting.as_ref().map(|(kind, n, games)| (*kind, *n, games.iter().filter_map(|g| self.catalog_tile(*g)).collect()));
        Some(CuratorToday { pick, waiting })
    }

    /// The panel of a row, for the rows that have one (Curator, Surprise me, Favorites, pinned smart shelves and
    /// stickers); `None` for games, systems and the rest, which show their own picture.
    pub(crate) fn home_panel(&self, item: Item) -> Option<Panel> {
        let ready = self.curator.index.is_some();
        let tiles = |games: Option<&Vec<usize>>| games.map_or_else(Vec::new, |gs| gs.iter().filter_map(|g| self.catalog_tile(*g)).collect());
        match item {
            Item::Curator => Some(home_panel::curator(self.curator_today())),
            Item::Surprise => {
                let otherwise = self.describe(item).3;
                Some(home_panel::surprise(self.curator_today().and_then(|t| t.pick), otherwise))
            }
            Item::Favorites => {
                let games = self.favorite_items().into_iter().filter_map(|i| if let Item::Game(s, g) = i { self.game_tile(s, g) } else { None }).collect();
                Some(home_panel::list(List::Favorites, games, self.describe(item).3, true))
            }
            // Rows of Home and of Curator's list alike: the first games of each are kept (`curator_counts`).
            Item::SmartShelf(i) => {
                Some(home_panel::list(List::SmartShelf, tiles(self.curator.shelf_tops.get(&i)), self.describe(item).3, ready))
            }
            Item::Sticker(id) => {
                Some(home_panel::list(List::Sticker, tiles(self.curator.sticker_tops.get(&id)), self.describe(item).3, ready))
            }
            _ => None,
        }
    }

    /// Paints `panel` in the box (x, top, max_w, h), against its right edge (`align` 1) or in its middle (0.5);
    /// returns the width used (0 for a panel of words only).
    pub(crate) fn paint_home_panel(&mut self, c: &mut Canvas, panel: &Panel, (x, top, max_w, h): (i32, i32, i32, i32), align: f32, ink: &Ink) -> i32 {
        let font = pocket_ui::type_scale::px(21);
        let caption_h = font as i32 + 12;
        let text = &mut self.text;
        let layout = home_panel::layout(panel, max_w, h, caption_h, &mut |s| text.width(Face::Body, 21, s));
        if layout.width == 0 {
            // No game starred yet: a star resting on an empty cushion where the covers will be (`ui_art`).
            if !panel.getting_ready && panel.groups.is_empty() && panel.line == home_panel::FAVORITES_EMPTY {
                let art = pocket_ui::ui_art::Art::NoFavorites;
                let k = (1..=2).rev().find(|k| pocket_ui::ui_art::size(art, *k).1 <= h).unwrap_or(1);
                let (aw, ah) = pocket_ui::ui_art::size(art, k);
                if aw <= max_w && ah <= h {
                    pocket_ui::ui_art::paint(c, art, x + ((max_w - aw) as f32 * align).round() as i32, top + (h - ah) / 2, k);
                    return aw;
                }
            }
            return 0;
        }
        let x0 = x + ((max_w - layout.width) as f32 * align).round() as i32;
        let faint = mix(ink.under, pal().label_dust, 40);
        for slot in &layout.slots {
            let (x, y, w, sh) = (x0 + slot.x, top + slot.y, slot.w, slot.h);
            if panel.getting_ready {
                c.round_rect(x, y, w, sh, 6, faint);
                continue;
            }
            let tile = &panel.groups[slot.group].tiles[slot.tile];
            let file = self.cover_file(&tile.system, &tile.rom);
            match file {
                Some(file) => match self.picture(&file, w, sh) {
                    // Standing on the slot's foot, in its middle, as a case stands on a shelf.
                    Some(img) => {
                        let (iw, ih) = (img.w as i32, img.h as i32);
                        let (ix, iy) = (x + (w - iw) / 2, y + sh - ih);
                        c.blit_scaled(&img, ix, iy, iw, ih);
                        if slot.dim {
                            c.shade_rect(ix, iy, iw, ih, ink.under, 90);
                        }
                    }
                    // Being decoded on the loader's thread: a faint tile until it comes.
                    None => c.round_rect(x, y, w, sh, 6, faint),
                },
                None => self.paint_title_tile(c, tile, (x, y, w, sh), slot.dim.then_some(ink.under)),
            }
        }
        for (gi, x, w) in &layout.captions {
            if let Some(caption) = &panel.groups[*gi].caption {
                let caption = self.text.fit(Face::Body, 21, caption, *w);
                self.text.draw(c, Face::Body, 21, x0 + x, top + h - 6, ink.caption, &caption);
            }
        }
        layout.width
    }

    /// The width `panel` takes in a box `max_w` x `h`.
    fn panel_width(&mut self, panel: &Panel, max_w: i32, h: i32) -> i32 {
        let caption_h = pocket_ui::type_scale::px(21) as i32 + 12;
        let text = &mut self.text;
        home_panel::layout(panel, max_w, h, caption_h, &mut |s| text.width(Face::Body, 21, s)).width
    }

    /// The line comes first: while the line (its first two parts, the pick and why) doesn't fit beside the covers in
    /// `text_w`, the last group gives way (Curator's waiting games, with Large text or in German).
    pub(crate) fn panel_make_room(&mut self, panel: &mut Panel, max_w: i32, h: i32, text_w: i32) {
        while panel.groups.len() > 1 {
            let room = text_w - self.panel_width(panel, max_w, h) - 36;
            let text = &mut self.text;
            let line = home_panel::fit_parts(&panel.line, room, &mut |s| text.width(Face::Body, 28, s));
            if self.text.width(Face::Body, 28, &line) <= room {
                return;
            }
            panel.groups.pop();
        }
    }

    /// A panel's line, fitted to `max` by its parts first (`home_panel::fit_parts`), then cut with "…".
    pub(crate) fn panel_line(&mut self, line: &str, max: i32) -> String {
        let text = &mut self.text;
        let line = home_panel::fit_parts(line, max, &mut |s| text.width(Face::Body, 28, s));
        self.text.fit(Face::Body, 28, &line, max)
    }

    /// A game without box art: a tile in its system's colour with its title, as the cabinet's case without a cover.
    fn paint_title_tile(&mut self, c: &mut Canvas, tile: &Tile, (x, y, w, h): (i32, i32, i32, i32), dim: Option<u32>) {
        // It stands in for a cover, a picture of a fixed size: its type keeps the design's size (as a case's spine).
        let _fixed = pocket_ui::type_scale::fixed();
        let colour = mix(stripe(&tile.system), 0x101010, 70);
        c.round_rect(x, y, w, h, 6, colour);
        // Behind another in a fan only its edge shows: no title to be cut by the one in front.
        if let Some(under) = dim {
            c.shade_rect(x, y, w, h, under, 90);
            return;
        }
        let step = pocket_ui::type_scale::line(21, 4);
        let lines = ((h - 24) / step).max(1) as usize;
        // A game's name, painted and measured as it is: never put through the translation table.
        self.text.set_translate(false);
        let text = std::cell::RefCell::new(&mut self.text);
        let shown = home_panel::title_lines(
            &tile.title,
            w - 16,
            lines,
            &mut |s| text.borrow_mut().width(Face::Body, 21, s),
            &mut |s, max| text.borrow_mut().fit(Face::Body, 21, s, max),
        );
        for (k, line) in shown.iter().enumerate() {
            self.text.draw(c, Face::Body, 21, x + 8, y + 12 + step * (k as i32 + 1) - 4, 0xECE9E1, line);
        }
        self.text.set_translate(true);
    }
}
