//! Part of the pocket-ui binary: the box art of the games the selection is heading for, decoded before they come into
//! view (TORTOS-SPEED.md #3). Without it a fast scroll on a Retro Handheld showed a plain stand-in front (Shelf, Cabinet,
//! Easy) or no cover (Split) on every case it reached first, and made the front twice: once plain, once with the cover.
//!
//! * After every move the next [`AHEAD`] entries in the direction of the move and [`BEHIND`] behind it are asked for, in
//!   the size the selected entry's cover was last drawn at, nearest first. Nothing else; no new cache.
//! * The same threads and caches as the visible covers: the Cabinet's reader for the case fronts' cover cuts
//!   (`app_cabinet`, Shelf, Cabinet and Easy), the picture loader for Split's preview (`app_pictures`). Both take a
//!   prefetch only when nothing visible is waiting, and the next move's window replaces the last one's.
//! * The caches stay bounded as before: a prefetched picture never pushes out one in view (`App::picture_arrived`,
//!   `App::prune_labels`).
//! * Nothing while a game runs or the device sleeps; a move while a game starts can't queue any (`covers::game_on`).

use std::path::PathBuf;

use pocket_ui::picture_loader::Key;

use super::{App, Item, Screen, Theme};

/// Entries ahead of the selection, in the direction it last moved, whose covers are read in advance.
pub(crate) const AHEAD: usize = 4;
/// Entries behind it: a step back is common, and its cover may already have been pushed out.
pub(crate) const BEHIND: usize = 1;
/// Cover cuts of entries this close to the selection are never pushed out of the Cabinet's cache (the ones in view are
/// among them: the selected case, Easy's neighbours, the case going back).
pub(crate) const NEAR: usize = AHEAD + 1;

/// Where the selection was when the window was last asked for, and which way it moved.
#[derive(Default)]
pub(crate) struct Prefetch {
    at: Option<(Screen, Theme, usize, usize)>,
    dir: i8,
}

/// The positions to read ahead from `sel` in a list of `len`, nearest first: `AHEAD` towards `dir`, then `BEHIND` away.
pub(crate) fn window(sel: usize, len: usize, dir: i8) -> Vec<usize> {
    let step = |d: usize, forward: bool| if forward { sel.checked_add(d).filter(|&i| i < len) } else { sel.checked_sub(d) };
    let forward = dir >= 0;
    let ahead = (1..=AHEAD).filter_map(|d| step(d, forward));
    let behind = (1..=BEHIND).filter_map(|d| step(d, !forward));
    ahead.chain(behind).collect()
}

impl App {
    /// After a frame on Home or a list: asks for the window's covers when the selection moved (once per move; a
    /// still selection costs one comparison). Called at the end of `paint`.
    pub(crate) fn cover_prefetch(&mut self) {
        if !enabled() || self.running.is_some() || self.sleep.state() != pocket_ui::sleep::State::Awake || pocket_ui::covers::game_on() {
            self.cover_prefetch_stop();
            return;
        }
        let at = (self.screen, self.theme, self.sel, self.items.len());
        let last = self.prefetch.at.replace(at);
        if last == Some(at) {
            return;
        }
        // The way the last move went on the same list; a new list starts reading forwards.
        self.prefetch.dir = match last {
            Some((screen, theme, sel, len)) if screen == at.0 && theme == at.1 && len == at.3 => match at.2.cmp(&sel) {
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => self.prefetch.dir,
            },
            _ => 1,
        };
        let near: Vec<Item> = (self.sel.saturating_sub(NEAR)..=(self.sel + NEAR).min(self.items.len().saturating_sub(1))).filter_map(|i| self.items.get(i).copied()).collect();
        let ahead: Vec<Item> = window(self.sel, self.items.len(), self.prefetch.dir).into_iter().filter_map(|i| self.items.get(i).copied()).collect();
        let cuts = matches!(self.theme, Theme::Shelf | Theme::Cabinet) || self.big_live() || self.kids_live();
        // Shelf, Cabinet, Easy: the cover cut of each case's front, at the size the selected front had.
        self.cabinet.near = near.iter().filter_map(|&item| self.prefetch_cover_file(item)).collect();
        let mut jobs = Vec::new();
        if cuts {
            for &item in &ahead {
                let Some(file) = self.prefetch_cover_file(item) else { continue };
                if let Some(job) = self.cabinet_prefetch_job(item, file) {
                    jobs.push(job);
                }
            }
        }
        self.cabinet_prefetch(jobs);
        // Split: the preview's box art, at the preview's size.
        let mut keys: Vec<Key> = Vec::new();
        if self.theme == Theme::Split && !self.big_live() {
            if let Some((w, h)) = self.pictures.cover_box {
                for &item in &ahead {
                    let Some(file) = self.prefetch_cover_file(item) else { continue };
                    keys.push((file, w, h));
                }
            }
        }
        self.pictures_prefetch(keys);
    }

    /// No more reading ahead (a game, sleep): the windows not read yet are dropped.
    pub(crate) fn cover_prefetch_stop(&mut self) {
        if self.prefetch.at.take().is_some() {
            self.cabinet_prefetch(Vec::new());
            self.pictures_prefetch(Vec::new());
        }
    }

    /// The box art file of a game entry, looked for once per ROM (`cover_file`); None for the rest.
    fn prefetch_cover_file(&mut self, item: Item) -> Option<PathBuf> {
        let (system, rom) = self.item_rom(item)?;
        self.cover_file(&system, &rom)
    }
}

/// `POCKETUI_COVER_PREFETCH=0` turns reading ahead off (the benches' "before" with the same binary).
fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("POCKETUI_COVER_PREFETCH").map_or(true, |v| v != "0"))
}

/// What `App::bench_prefetch` measured.
pub(crate) struct PrefetchBench {
    pub moves: usize,
    pub frames: usize,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
    /// Moves whose first frame had the selected game's cover (of the moves onto a game with box art).
    pub first_ready: usize,
    pub with_art: usize,
    /// Frames that showed a selected game with box art without it (a stand-in front, or no picture in Split).
    pub stand_in_frames: usize,
    pub art_frames: usize,
}

impl App {
    /// The selected game has box art and it is decoded for the way the look shows it (true for anything without).
    pub(crate) fn selected_cover_ready(&mut self) -> Option<bool> {
        let item = self.items.get(self.sel).copied()?;
        let file = self.prefetch_cover_file(item)?;
        if self.theme == Theme::Split && !self.big_live() {
            let (w, h) = self.pictures.cover_box?;
            return Some(self.covers.get(&(file, w, h)).is_some_and(|c| c.is_some()));
        }
        self.cover_cut_ready(item)
    }

    /// `item`'s cover is decoded at the size the selected entry's is shown at (None: no box art, or no size yet).
    fn cover_ready_for(&mut self, item: Item) -> Option<bool> {
        let file = self.prefetch_cover_file(item)?;
        if self.theme == Theme::Split && !self.big_live() {
            let (w, h) = self.pictures.cover_box?;
            return Some(self.covers.get(&(file, w, h)).is_some_and(|c| c.is_some()));
        }
        self.cover_cut_ready(item)
    }

    /// `--snapshot prefetch-check:<look>:<system>`: reading ahead, checked (tests/cover_prefetch.rs). Covers on their
    /// threads as on the device. Prints
    /// * `prefetch-ahead ready=<n>/<AHEAD> first_frame=<bool>`: after one move and a rest without frames, how many of
    ///   the next covers are decoded, and whether the next move's first frame has its cover;
    /// * `prefetch-keep selected=<bool> cached=<n>`: after the windows of 80 entries far away were read, the selected
    ///   game's cover (in view all along) is still in the cache, and the cache is bounded.
    pub(crate) fn prefetch_check(&mut self, rest: &str, w: u32, h: u32) {
        use std::time::{Duration, Instant};
        let mut parts = rest.split(':');
        let look = parts.next().filter(|l| !l.is_empty()).unwrap_or("shelf");
        let system = parts.next();
        self.shelf_snapshot(&format!(":{look}"));
        if look == "big" {
            self.theme = Theme::Big;
            self.rebuild();
        }
        if let Some(s) = system.filter(|_| look != "big").and_then(|id| self.systems.iter().position(|s| s.name == id)) {
            self.screen = Screen::Games(s);
            self.rebuild();
        }
        self.cabinet.inline = false;
        let answers = self.pictures_bench_loader();
        self.sel = Item::first_row(&self.items);
        self.prev_sel = self.sel;
        let panel = crate::snapshot_panel_dims();
        let direct = pocket_ui::scaled_layout::logical_size(panel) != (w, h);
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        let mut screen = vec![0u32; if direct { (w * h) as usize } else { panel.0 as usize * panel.1 as usize }];
        let next = if look == "big" || (system.is_some() && look == "cabinet") { super::Button::Right } else { super::Button::Down };
        let mut frame = |app: &mut App| {
            for (key, img) in answers.try_iter() {
                app.picture_arrived(key, img);
            }
            app.device_frame(&mut scaler, &mut screen, panel, direct.then_some((w, h)));
        };
        // Waits, without frames, until `done` (the threads answering; the cabinet's answers taken in as a frame would).
        // Without reading ahead nothing is coming: a second is plenty to show that.
        let patience = Duration::from_secs(if enabled() { 10 } else { 1 });
        let wait = |app: &mut App, frame: &mut dyn FnMut(&mut App), paint: bool, done: &mut dyn FnMut(&mut App) -> bool| {
            let until = Instant::now() + patience;
            while Instant::now() < until {
                if paint {
                    frame(app);
                } else {
                    app.cabinet_poll();
                }
                if done(app) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        };
        // One move forwards, its own cover in, then a rest without frames: what is read ahead.
        frame(self);
        self.step(next);
        frame(self);
        wait(self, &mut frame, true, &mut |app| app.selected_cover_ready() == Some(true));
        let ahead: Vec<Item> = window(self.sel, self.items.len(), 1).into_iter().take(AHEAD).filter_map(|i| self.items.get(i).copied()).collect();
        let all_ready = |app: &mut App| ahead.iter().all(|&i| app.cover_ready_for(i) == Some(true));
        // Split's answers come in with a frame (of the selection only); the cuts with `cabinet_poll`.
        let split = self.theme == Theme::Split && !self.big_live();
        wait(self, &mut frame, split, &mut |app| all_ready(app));
        let ready = ahead.iter().filter(|&&i| self.cover_ready_for(i) == Some(true)).count();
        self.step(next);
        frame(self);
        let first = self.selected_cover_ready();
        println!("prefetch-ahead ready={ready}/{} first_frame={}", ahead.len(), first == Some(true));
        // The selected cover in view while the windows of far entries are read and come in.
        wait(self, &mut frame, true, &mut |app| app.selected_cover_ready() == Some(true));
        let (sel, prev) = (self.sel, self.prev_sel);
        let far: Vec<usize> = (sel + 2 * NEAR..self.items.len()).take(80).collect();
        for chunk in far.chunks(AHEAD + BEHIND) {
            // As a move there would: the window asked for, then the frames of the selection, which stays where it is.
            let items: Vec<Item> = chunk.iter().filter_map(|&i| self.items.get(i).copied()).collect();
            let mut jobs = Vec::new();
            let mut keys = Vec::new();
            for &item in &items {
                let Some(file) = self.prefetch_cover_file(item) else { continue };
                if split {
                    if let Some((w, h)) = self.pictures.cover_box {
                        keys.push((file, w, h));
                    }
                } else if let Some(job) = self.cabinet_prefetch_job(item, file) {
                    jobs.push(job);
                }
            }
            self.cabinet_prefetch(jobs);
            self.pictures_prefetch(keys);
            wait(self, &mut frame, true, &mut |app| items.iter().all(|&i| app.cover_ready_for(i) != Some(false)) || app.selected_cover_ready() != Some(true));
            if self.selected_cover_ready() != Some(true) {
                break;
            }
            (self.sel, self.prev_sel) = (sel, prev);
        }
        let cached = if split { self.covers.len() } else { self.cover_cuts() };
        println!("prefetch-keep selected={} cached={cached}", self.selected_cover_ready() == Some(true));
    }

    /// `--snapshot search-start-check:<look>:<system>`: a game started from the search (tests/cover_prefetch.rs). The
    /// search has read the game's cover for its row; the shelf, on another game, has never shown this one. As the start
    /// would, the cover is asked for (`warm_launch_cover`) and the search closes onto the game's shelf; the first frame
    /// of that scene, covers on their threads as on a device, is checked. Prints
    /// `search-start first_frame_cover=<bool> asked_cover=<bool>`: the first frame shows the real cover (not a plain
    /// stand-in front), for a game that has box art.
    pub(crate) fn search_start_check(&mut self, rest: &str, w: u32, h: u32) {
        let mut parts = rest.split(':');
        let look = parts.next().filter(|l| !l.is_empty()).unwrap_or("shelf");
        let system = parts.next().unwrap_or("gbc");
        self.shelf_snapshot(&format!(":{look}"));
        if look == "big" {
            self.theme = Theme::Big;
            self.rebuild();
        }
        let Some(s) = self.systems.iter().position(|sys| sys.name == system) else { return println!("search-start no system {system}") };
        self.screen = Screen::Games(s);
        self.rebuild();
        self.cabinet.inline = false;
        self.sel = Item::first_row(&self.items);
        self.prev_sel = self.sel;
        let panel = crate::snapshot_panel_dims();
        let direct = pocket_ui::scaled_layout::logical_size(panel) != (w, h);
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        let mut screen = vec![0u32; if direct { (w * h) as usize } else { panel.0 as usize * panel.1 as usize }];
        // The shelf has been showing the first game, as before the search was opened.
        self.device_frame(&mut scaler, &mut screen, panel, direct.then_some((w, h)));
        // A game far from the selection (nothing read ahead for it) that has box art.
        let far = (self.sel + 2 * NEAR..self.items.len()).find_map(|i| {
            let item = self.items.get(i).copied()?;
            let (sys, rom) = self.item_rom(item)?;
            self.cover_file(&sys, &rom).map(|_| (item, sys, rom))
        });
        let Some((item, sys, rom)) = far else { return println!("search-start no game with box art") };
        self.find.inline = true;
        self.open_search();
        let row = self.search_thumb(&sys, &rom, 64).is_some();
        // The start: asked for, then the search closes onto the shelf with the game selected.
        self.warm_launch_cover(&sys, &rom);
        self.launch_at = Some(std::time::Instant::now());
        self.search_launching(&rom);
        let on_it = self.items.get(self.sel).copied() == Some(item);
        self.device_frame(&mut scaler, &mut screen, panel, direct.then_some((w, h)));
        let first = self.selected_cover_ready();
        println!("search-start first_frame_cover={} asked_cover={} selected={on_it}", first == Some(true), row);
    }

    /// `--snapshot bench-prefetch:<look>[:<system>]`: a fast scroll through a library with cold caches, in real time, the covers
    /// read on their threads as on the device: bursts of 40 held moves (one every `REPEAT_EVERY`) with 0.6 s rests
    /// between, a frame every 16.7 ms. Thread CPU of every frame, and how often the selected game showed without its
    /// cover. `POCKETUI_COVER_PREFETCH=0` for the same without reading ahead.
    pub(crate) fn bench_prefetch(&mut self, look: &str, w: u32, h: u32) -> PrefetchBench {
        use std::time::{Duration, Instant};
        let mut parts = look.split(':');
        let look = parts.next().filter(|l| !l.is_empty()).unwrap_or("shelf");
        let system = parts.next();
        self.shelf_snapshot(&format!(":{look}"));
        if look == "big" {
            self.theme = Theme::Big;
            self.rebuild();
        }
        // `:<system>`: that system's games (Easy keeps Home, as `bench-shelf`).
        if let Some(s) = system.filter(|_| look != "big").and_then(|id| self.systems.iter().position(|s| s.name == id)) {
            self.screen = Screen::Games(s);
            self.rebuild();
        }
        // Box art the way the app reads it: on the threads, answers taken in before each frame.
        self.cabinet.inline = false;
        let answers = self.pictures_bench_loader();
        self.rebuild();
        self.sel = Item::first_row(&self.items);
        self.prev_sel = self.sel;
        pocket_ui::status_corner::start(|| {});
        self.text.prewarm();
        self.text.finish_prewarm();
        let panel = crate::snapshot_panel_dims();
        let direct = pocket_ui::scaled_layout::logical_size(panel) != (w, h);
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        let mut screen = vec![0u32; if direct { (w * h) as usize } else { panel.0 as usize * panel.1 as usize }];
        let next = if look == "big" || (system.is_some() && look == "cabinet") { super::Button::Right } else { super::Button::Down };
        let cpu = || pocket_ui::run_ahead::thread_cpu_ms() * 1000.0;
        let frame_len = Duration::from_micros(16_667);
        let mut us = Vec::new();
        let mut b = PrefetchBench { moves: 0, frames: 0, p50_us: 0.0, p95_us: 0.0, max_us: 0.0, first_ready: 0, with_art: 0, stand_in_frames: 0, art_frames: 0 };
        let mut paint = |app: &mut App, until: Instant, first: bool, b: &mut PrefetchBench, us: &mut Vec<f64>| {
            let mut first = first;
            loop {
                let at = Instant::now();
                for (key, img) in answers.try_iter() {
                    app.picture_arrived(key, img);
                }
                let t = cpu();
                app.device_frame(&mut scaler, &mut screen, panel, direct.then_some((w, h)));
                us.push(cpu() - t);
                b.frames += 1;
                if let Some(ready) = app.selected_cover_ready() {
                    b.art_frames += 1;
                    b.stand_in_frames += usize::from(!ready);
                    if first {
                        b.with_art += 1;
                        b.first_ready += usize::from(ready);
                    }
                }
                first = false;
                let next_frame = at + frame_len;
                if next_frame >= until {
                    std::thread::sleep(until.saturating_duration_since(Instant::now()));
                    return;
                }
                std::thread::sleep(next_frame.saturating_duration_since(Instant::now()));
            }
        };
        // The first picture of the list, settled.
        paint(self, Instant::now() + Duration::from_millis(600), false, &mut b, &mut Vec::new());
        b = PrefetchBench { moves: 0, frames: 0, p50_us: 0.0, p95_us: 0.0, max_us: 0.0, first_ready: 0, with_art: 0, stand_in_frames: 0, art_frames: 0 };
        for _burst in 0..3 {
            for _ in 0..40 {
                if self.sel + 1 >= self.items.len() {
                    break;
                }
                self.step(next);
                self.moved_at = Instant::now();
                b.moves += 1;
                paint(self, Instant::now() + super::REPEAT_EVERY, true, &mut b, &mut us);
            }
            paint(self, Instant::now() + Duration::from_millis(600), false, &mut b, &mut us);
        }
        us.sort_by(f64::total_cmp);
        let at = |q: f64| us.get(((us.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
        (b.p50_us, b.p95_us, b.max_us) = (at(0.5), at(0.95), at(1.0));
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_reads_ahead_in_the_direction_of_the_move_nearest_first() {
        assert_eq!(window(10, 300, 1), vec![11, 12, 13, 14, 9]);
        assert_eq!(window(10, 300, -1), vec![9, 8, 7, 6, 11]);
        // At the ends of the list: what there is.
        assert_eq!(window(0, 300, -1), vec![1]);
        assert_eq!(window(298, 300, 1), vec![299, 297]);
        assert_eq!(window(0, 1, 1), Vec::<usize>::new());
    }
}
