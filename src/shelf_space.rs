//! Settings → Games → **Shelf space** (off unless switched on): only so many games stand on the shelf, the rest lie in
//! the **crate**, a few cardboard boxes under it (`crate_scene`, painted by `app_crate`). A game comes out of the crate
//! by swapping it for one on the shelf: one out, one in, the new one takes the old one's place. Against the backlog
//! (r/SBCGaming 17.11.2026: "it would help me keep my number of games reduced").
//!
//! The model is the list of games on the shelf, by ROM path, in the shelf's own order (`shelf_space.json` in the data
//! folder). A game the list doesn't know is in the crate: games copied onto the card later go into the crate, and the
//! shelf keeps what the player put on it. A game gone from the card leaves a free place.

use std::path::{Path, PathBuf};

const FILE: &str = "shelf_space.json";

/// The places on the shelf a player can choose; any other number from 1 to [`MOST`] is "own number".
pub const CHOICES: [u32; 3] = [10, 20, 50];
/// The most places an own number may have.
pub const MOST: u32 = 500;

/// Which games go on the shelf when shelf space is switched on (the first start asks, `Fill`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fill {
    /// The games played last, the last first; then the favourites, then by name.
    Recent,
    /// The favourites, then the games played last, then by name.
    Favourites,
    /// None: the shelf starts empty and the player takes games out of the crate one by one.
    Pick,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShelfSpace {
    /// How many places the shelf has; None: shelf space is off and every game stands on the shelf.
    pub limit: Option<u32>,
    /// The games on the shelf, in the shelf's order.
    pub shelf: Vec<PathBuf>,
}

impl ShelfSpace {
    pub fn load(data_dir: &Path) -> ShelfSpace {
        let Ok(text) = std::fs::read_to_string(data_dir.join(FILE)) else { return ShelfSpace::default() };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return ShelfSpace::default() };
        let limit = v["limit"].as_u64().map(|n| n.clamp(1, MOST as u64) as u32);
        let shelf = v["shelf"].as_array().map(|a| a.iter().filter_map(|p| p.as_str()).map(PathBuf::from).collect()).unwrap_or_default();
        ShelfSpace { limit, shelf }
    }

    pub fn save(&self, data_dir: &Path) {
        let v = serde_json::json!({
            "limit": self.limit,
            "shelf": self.shelf.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        });
        let _ = crate::fsutil::write_atomic(&data_dir.join(FILE), v.to_string().as_bytes());
    }

    pub fn on(&self) -> bool {
        self.limit.is_some()
    }

    /// Whether `rom` stands on the shelf (every game does while shelf space is off).
    pub fn on_shelf(&self, rom: &Path) -> bool {
        self.limit.is_none() || self.shelf.iter().any(|p| p == rom)
    }

    /// How many places are free.
    pub fn free(&self) -> u32 {
        self.limit.map_or(0, |l| l.saturating_sub(self.shelf.len() as u32))
    }

    /// Switched on with `limit` places, filled from `candidates` in their order (`order` makes them for a `Fill`).
    /// Switched on again later the shelf keeps what was on it, as far as it fits.
    pub fn switch_on(&mut self, limit: u32, candidates: impl IntoIterator<Item = PathBuf>) {
        let limit = limit.clamp(1, MOST);
        self.limit = Some(limit);
        self.shelf.truncate(limit as usize);
        for p in candidates {
            if self.shelf.len() >= limit as usize {
                break;
            }
            if !self.shelf.contains(&p) {
                self.shelf.push(p);
            }
        }
    }

    /// Switched off: every game is back on the shelf. What stood on it is kept for switching on again.
    pub fn switch_off(&mut self) {
        self.limit = None;
    }

    /// One out, one in: `inn` takes `out`'s place. False when `out` is not on the shelf or `inn` already is.
    pub fn swap(&mut self, out: &Path, inn: &Path) -> bool {
        if self.shelf.iter().any(|p| p == inn) {
            return false;
        }
        match self.shelf.iter().position(|p| p == out) {
            Some(i) => {
                self.shelf[i] = inn.to_path_buf();
                true
            }
            None => false,
        }
    }

    /// `inn` onto a free place, at the shelf's end. False when there is none or it is there already.
    pub fn put(&mut self, inn: &Path) -> bool {
        if self.free() == 0 || self.shelf.iter().any(|p| p == inn) {
            return false;
        }
        self.shelf.push(inn.to_path_buf());
        true
    }

    /// `out` into the crate, its place left free (a game played to the end, "Into the crate" on its card).
    pub fn put_away(&mut self, out: &Path) -> bool {
        let n = self.shelf.len();
        self.shelf.retain(|p| p != out);
        self.shelf.len() != n
    }

    /// Games no longer on the card leave their place free.
    pub fn forget_missing(&mut self, exists: impl Fn(&Path) -> bool) -> bool {
        let n = self.shelf.len();
        self.shelf.retain(|p| exists(p));
        self.shelf.len() != n
    }
}

/// The games in the order a `Fill` puts them on the shelf: `recent` (last played first), `favourites`, then `all` by
/// name; each game once.
pub fn order(fill: Fill, recent: &[PathBuf], favourites: &[PathBuf], all: &[PathBuf]) -> Vec<PathBuf> {
    let firsts: [&[PathBuf]; 2] = match fill {
        Fill::Recent => [recent, favourites],
        Fill::Favourites => [favourites, recent],
        Fill::Pick => return Vec::new(),
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for p in firsts.into_iter().flatten().chain(all) {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(format!("/roms/{s}"))
    }

    #[test]
    fn off_every_game_is_on_the_shelf_and_on_only_the_chosen_ones() {
        let mut s = ShelfSpace::default();
        assert!(s.on_shelf(&p("a")));
        s.switch_on(2, [p("a"), p("b"), p("c")]);
        assert_eq!(s.shelf, [p("a"), p("b")]);
        assert!(s.on_shelf(&p("a")) && !s.on_shelf(&p("c")));
        assert_eq!(s.free(), 0);
        s.switch_off();
        assert!(s.on_shelf(&p("c")), "off: everything back on the shelf");
        s.switch_on(3, [p("c"), p("d")]);
        assert_eq!(s.shelf, [p("a"), p("b"), p("c")], "on again: the old shelf, then the new choice");
    }

    #[test]
    fn a_swap_puts_the_new_game_in_the_old_ones_place() {
        let mut s = ShelfSpace::default();
        s.switch_on(3, [p("a"), p("b"), p("c")]);
        assert!(s.swap(&p("b"), &p("x")));
        assert_eq!(s.shelf, [p("a"), p("x"), p("c")]);
        assert!(!s.swap(&p("b"), &p("y")), "b is in the crate now");
        assert!(!s.swap(&p("a"), &p("c")), "c is on the shelf already");
        assert!(!s.put(&p("y")), "no free place");
        assert!(s.put_away(&p("a")));
        assert_eq!(s.free(), 1);
        assert!(s.put(&p("y")));
        assert_eq!(s.shelf, [p("x"), p("c"), p("y")]);
    }

    #[test]
    fn fewer_games_than_places_leaves_places_free_and_a_lost_game_frees_its_place() {
        let mut s = ShelfSpace::default();
        s.switch_on(10, [p("a"), p("b")]);
        assert_eq!((s.shelf.len(), s.free()), (2, 8));
        assert!(s.forget_missing(|r| r != p("a")));
        assert_eq!((s.shelf.clone(), s.free()), (vec![p("b")], 9));
    }

    #[test]
    fn the_fill_orders_put_recent_or_favourite_games_first_and_each_once() {
        let all = [p("a"), p("b"), p("c"), p("d")];
        assert_eq!(order(Fill::Recent, &[p("c")], &[p("d"), p("c")], &all), [p("c"), p("d"), p("a"), p("b")]);
        assert_eq!(order(Fill::Favourites, &[p("c")], &[p("d")], &all), [p("d"), p("c"), p("a"), p("b")]);
        assert!(order(Fill::Pick, &[p("c")], &[], &all).is_empty());
    }

    /// The read of shelf_space.json against every damaged form of a valid file (`file_faults`: missing, empty, cut short,
    /// bits flipped, another encoding): it never panics, and what it reads keeps to its rules (a limit in 1..=MOST).
    #[test]
    fn file_source_rule_shelf_space() {
        let d = crate::testutil::TestDir::new("shelf-space-faults");
        let mut s = ShelfSpace::default();
        s.switch_on(10, [p("a"), p("b")]);
        s.save(&d);
        let seed = std::fs::read(d.join(FILE)).unwrap();
        for (label, bytes) in crate::file_faults::faults(&seed) {
            let _ = std::fs::remove_file(d.join(FILE));
            if let Some(bytes) = bytes {
                std::fs::write(d.join(FILE), bytes).unwrap();
            }
            let got = ShelfSpace::load(&d);
            assert!(got.limit.is_none_or(|n| (1..=MOST).contains(&n)), "{label}: {got:?}");
            if label == "valid" {
                assert_eq!(got, s);
            }
        }
    }

    #[test]
    fn it_survives_a_restart_and_a_broken_file_means_off() {
        let d = crate::testutil::TestDir::new("shelf-space");
        let mut s = ShelfSpace::default();
        s.switch_on(20, [p("a")]);
        s.save(&d);
        assert_eq!(ShelfSpace::load(&d), s);
        std::fs::write(d.join(FILE), b"{oops").unwrap();
        assert_eq!(ShelfSpace::load(&d), ShelfSpace::default());
    }
}
