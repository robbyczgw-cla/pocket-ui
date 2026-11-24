//! Settings → Games → **Shelf space** (off unless switched on): only so many games stand on the shelf, the rest lie in
//! the **crate**, a few cardboard boxes under it (`crate_scene`, painted by `app_crate`). A game comes out of the crate
//! by swapping it for one on the shelf: one out, one in, the new one takes the old one's place. Against the backlog
//! (r/SBCGaming 17.11.2026: "it would help me keep my number of games reduced").
//!
//! The model is the list of games on the shelf, by ROM path, in the shelf's own order (`shelf_space.json` in the data
//! folder). A game the list doesn't know is in the crate: games copied onto the card later go into the crate, and the
//! shelf keeps what the player put on it. A game gone from the card leaves a free place.

use std::collections::HashSet;
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

#[derive(Clone, Debug, Default)]
pub struct ShelfSpace {
    /// How many places the shelf has; None: shelf space is off and every game stands on the shelf.
    pub limit: Option<u32>,
    /// The games on the shelf, in the shelf's order. Changed only through the methods here, which keep `index`.
    pub shelf: Vec<PathBuf>,
    /// `shelf` as a set: `on_shelf` runs for every game of the card on every rebuild and in the crate's count
    /// (179,000 games × 500 places on the R36S card was 90 million path comparisons).
    index: HashSet<PathBuf>,
}

impl PartialEq for ShelfSpace {
    fn eq(&self, o: &Self) -> bool {
        self.limit == o.limit && self.shelf == o.shelf
    }
}

impl ShelfSpace {
    pub fn load(data_dir: &Path) -> ShelfSpace {
        let Ok(text) = std::fs::read_to_string(data_dir.join(FILE)) else { return ShelfSpace::default() };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return ShelfSpace::default() };
        let limit = v["limit"].as_u64().map(|n| n.clamp(1, MOST as u64) as u32);
        let shelf: Vec<PathBuf> = v["shelf"].as_array().map(|a| a.iter().filter_map(|p| p.as_str()).map(PathBuf::from).collect()).unwrap_or_default();
        let mut s = ShelfSpace { limit, shelf, index: HashSet::new() };
        // A hand-edited file may name a game twice: keep the first, as `switch_on` would.
        let mut seen = HashSet::new();
        s.shelf.retain(|p| seen.insert(p.clone()));
        s.index = seen;
        s
    }

    fn reindex(&mut self) {
        self.index = self.shelf.iter().cloned().collect();
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
        if self.limit.is_none() {
            return true;
        }
        // The set matches the list unless someone changed `shelf` past the methods: then the list decides.
        if self.index.len() == self.shelf.len() {
            self.index.contains(rom)
        } else {
            self.shelf.iter().any(|p| p == rom)
        }
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
        self.reindex();
        for p in candidates {
            if self.shelf.len() >= limit as usize {
                break;
            }
            if self.index.insert(p.clone()) {
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
                self.reindex();
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
        self.index.insert(inn.to_path_buf());
        true
    }

    /// `out` into the crate, its place left free (a game played to the end, "Into the crate" on its card).
    pub fn put_away(&mut self, out: &Path) -> bool {
        let n = self.shelf.len();
        self.shelf.retain(|p| p != out);
        self.index.remove(out);
        self.shelf.len() != n
    }

    /// Games no longer on the card leave their place free.
    pub fn forget_missing(&mut self, exists: impl Fn(&Path) -> bool) -> bool {
        let n = self.shelf.len();
        self.shelf.retain(|p| exists(p));
        self.reindex();
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
    // Each game once, by a set: `out.contains` against the whole list was quadratic, 178 s on a desktop for the R36S
    // card's 63,000 games (15–30 minutes of a still picture on the handheld).
    let mut seen: std::collections::HashSet<&PathBuf> = std::collections::HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    for p in firsts.into_iter().flatten().chain(all) {
        if seen.insert(p) {
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

    fn thread_cpu() -> std::time::Duration {
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) }, 0);
        std::time::Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
    }

    /// A big card (the R36S's 179,000 files, 63,000 games): Settings → Games → Shelf space → "My favourites" orders
    /// every game on the main thread. With `out.contains` that took 178 s on a desktop; it has to stay
    /// far below a frame's worth per thousand games. Thread CPU, so a loaded host doesn't fail it.
    #[test]
    fn ordering_a_big_card_is_linear_not_quadratic() {
        let all: Vec<PathBuf> = (0..60_000).map(|i| PathBuf::from(format!("/storage/roms/snes/Game {i:05} (USA).zip"))).collect();
        let favourites: Vec<PathBuf> = all.iter().rev().take(300).cloned().collect();
        let t = thread_cpu();
        let out = order(Fill::Favourites, &all[..200], &favourites, &all);
        let took = thread_cpu() - t;
        assert_eq!(out.len(), all.len(), "each game once");
        assert_eq!(out[0], all[all.len() - 1], "the favourites first");
        assert!(took < std::time::Duration::from_millis(1500), "ordering 60,000 games took {took:?} of CPU");
    }

    /// Home, a system's row and the crate ask `on_shelf` for every game on every rebuild (and the crate's card counts
    /// them while it is drawn): 180,000 games against a full shelf of 500 places was 90 million path comparisons.
    #[test]
    fn asking_a_full_shelf_about_every_game_of_a_big_card_is_cheap() {
        let all: Vec<PathBuf> = (0..180_000).map(|i| PathBuf::from(format!("/storage/roms/zxspectrum/Game {i:06}.tzx"))).collect();
        let mut s = ShelfSpace::default();
        s.switch_on(MOST, all.iter().step_by(360).cloned());
        assert_eq!(s.shelf.len(), MOST as usize);
        let t = thread_cpu();
        let on = all.iter().filter(|g| s.on_shelf(g)).count();
        let took = thread_cpu() - t;
        assert_eq!(on, MOST as usize);
        assert!(took < std::time::Duration::from_millis(800), "180,000 on_shelf questions took {took:?} of CPU");
    }

    /// The set behind `on_shelf` follows every change of the shelf, and a file naming a game twice keeps it once.
    #[test]
    fn on_shelf_follows_every_change() {
        let mut s = ShelfSpace::default();
        s.switch_on(3, [p("a"), p("b"), p("a"), p("c"), p("d")]);
        assert_eq!(s.shelf, [p("a"), p("b"), p("c")]);
        assert!(s.on_shelf(&p("c")) && !s.on_shelf(&p("d")));
        assert!(s.swap(&p("c"), &p("d")));
        assert!(s.on_shelf(&p("d")) && !s.on_shelf(&p("c")));
        assert!(s.put_away(&p("a")));
        assert!(!s.on_shelf(&p("a")));
        assert!(s.put(&p("x")));
        assert!(s.on_shelf(&p("x")));
        assert!(s.forget_missing(|r| r != p("b")));
        assert!(!s.on_shelf(&p("b")) && s.on_shelf(&p("d")) && s.on_shelf(&p("x")));
        s.switch_on(1, []);
        assert_eq!(s.shelf, [p("d")]);
        assert!(!s.on_shelf(&p("x")), "cut to one place: x went into the crate");

        let dir = crate::testutil::TestDir::new("shelf-space-twice");
        std::fs::write(dir.join(FILE), r#"{"limit":5,"shelf":["/roms/a","/roms/b","/roms/a"]}"#).unwrap();
        let read = ShelfSpace::load(&dir);
        assert_eq!(read.shelf, [p("a"), p("b")]);
        assert!(read.on_shelf(&p("b")) && !read.on_shelf(&p("c")));
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
