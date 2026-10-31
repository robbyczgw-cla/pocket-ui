//! The idle slideshow (docs/SLEEP.md, "Idle slideshow"): when the shelf has seen no input for a while and before
//! the screen would go off, pocket-ui shows the player's games instead of a static shelf: covers, scraped
//! screenshots and title screens, and the player's own screenshots and save-state pictures, each with a slow pan and
//! zoom (Ken Burns), fading into the next, the game's title and system small at the bottom, the clock and battery in
//! the corner. Any button goes back to the shelf exactly as it was (`app_slideshow.rs`).
//!
//! This file is the part that needs no window: when it starts ([`start_after`]), which pictures ([`collect`], on a
//! thread), how a picture is prepared once at its size ([`prepare`], on the picture thread), the timing of one
//! slide ([`fade_mix`], [`progress`], [`title_alpha`], [`view`]) and the paint of a prepared picture at a view
//! ([`paint`]: a bilinear window, so a slow zoom does not shimmer). Nothing here touches the sleep timers: the show
//! only reads how long the pad has been quiet and ends when the screen would go off.

#[cfg(target_os = "linux")]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::gfx::Canvas;
use crate::image::{self, Image};

/// Without input this long on the shelf, the show begins (kept short of the screen-off time, see [`start_after`]).
pub const IDLE_AFTER: Duration = Duration::from_secs(60);
/// The show needs at least this long before the screen goes off; with less, there is none.
pub const MIN_RUN: Duration = Duration::from_secs(8);
/// The show ends after this long (Screen off after "Never": it does not play for ever).
pub const MAX_RUN: Duration = Duration::from_secs(10 * 60);
/// A slide starts this often.
pub const HOLD: Duration = Duration::from_secs(10);
/// One picture fades into the next over this long (the first one fades in over the shelf).
pub const FADE: Duration = Duration::from_millis(1600);
/// A picture moves for its own time plus the fade into the next one.
pub const LIFE: Duration = Duration::from_millis(11_600);
/// While something moves, a frame this often (the pan is under a pixel per frame: 15 a second looks smooth and costs
/// a quarter of what 60 would).
pub const FRAME: Duration = Duration::from_millis(66);
/// A prepared picture is this much larger than the screen's fit, which is how far the zoom goes.
pub const ZOOM_MAX: f32 = 1.12;
/// Pictures smaller than this (a side, in pixels) are left out: a blown-up thumbnail is no memory.
pub const MIN_SIDE: u32 = 64;
/// The most pictures one show cycles through, and games looked at to find them.
pub const MAX_SLIDES: usize = 36;
pub const MAX_GAMES_LOOKED_AT: usize = 600;

/// When the show begins, after this long without input: a minute, but never later than half the screen-off time, so
/// that it has time to be seen (Screen off after 30 s: it begins at 15 s). `None`: no room for it (the screen goes
/// off within [`MIN_RUN`] x 2). Screen off "never": a minute.
pub fn start_after(screen_off_secs: Option<u32>) -> Option<Duration> {
    let Some(off) = screen_off_secs else { return Some(IDLE_AFTER) };
    let off = Duration::from_secs(off as u64);
    let after = IDLE_AFTER.min(off / 2);
    (off.saturating_sub(after) >= MIN_RUN).then_some(after)
}

/// Where a picture comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Cover,
    Snap,
    Title,
    /// The player's own screenshot (HOME + A).
    Screenshot,
    /// The picture of one of the player's save states.
    Save,
}

impl Source {
    /// The player's own pictures ("memories").
    pub fn is_memory(self) -> bool {
        matches!(self, Source::Screenshot | Source::Save)
    }

    /// The small line under a memory's title; `None` for the game's own box art and scraped pictures.
    pub fn caption(self) -> Option<&'static str> {
        match self {
            Source::Screenshot => Some("Your screenshot"),
            Source::Save => Some("From your save"),
            _ => None,
        }
    }
}

/// One picture of the show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub file: PathBuf,
    pub title: String,
    pub system: String,
    pub source: Source,
}

#[cfg(target_os = "linux")]
/// A game of the library, as the show needs it.
#[derive(Clone, Debug)]
pub struct GameRef {
    /// The short id ("gbc"), the folder name of its pictures.
    pub system: String,
    /// "Game Boy Color".
    pub system_name: String,
    pub name: String,
    pub path: PathBuf,
}

#[cfg(target_os = "linux")]
/// The folders pictures are looked for in.
#[derive(Clone, Debug)]
pub struct Dirs {
    pub data: PathBuf,
    pub screenshots: PathBuf,
    pub states: PathBuf,
    /// Settings → Games → Extras → Game pictures: with "Box art only" the in-game screenshots and title screens that are
    /// on the card are left out, the box art stays.
    pub scraped_pictures: bool,
}

/// Small, fast, good enough to shuffle a few hundred games.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

#[cfg(target_os = "linux")]
fn usable(file: &Path) -> bool {
    image::png_size(file).is_some_and(|(w, h)| w >= MIN_SIDE && h >= MIN_SIDE)
}

#[cfg(target_os = "linux")]
/// The pictures the show cycles through: of a shuffle of the games, the first picture each has (box art, in-game
/// screenshot or title screen, in an order that varies), and the player's own screenshots and save-state pictures,
/// one in three. Files are looked at by their headers only. Empty when there is nothing real to show.
pub fn collect(games: &[GameRef], dirs: &Dirs, seed: u64) -> Vec<Slide> {
    use crate::game_pictures::{self, Kind};
    let mut rng = Rng(seed | 1);
    let mut order: Vec<usize> = (0..games.len()).collect();
    rng.shuffle(&mut order);
    let want_scraped = MAX_SLIDES * 2 / 3;
    let mut scraped = Vec::new();
    for &i in order.iter().take(MAX_GAMES_LOOKED_AT) {
        if scraped.len() >= want_scraped {
            break;
        }
        let g = &games[i];
        let mut tries = vec![(Source::Cover, crate::covers::cover_path(&dirs.data, &g.system, &g.path))];
        if dirs.scraped_pictures {
            tries.push((Source::Snap, game_pictures::path(&dirs.data, Kind::Snap, &g.system, &g.path)));
            tries.push((Source::Title, game_pictures::path(&dirs.data, Kind::Title, &g.system, &g.path)));
        }
        let turn = rng.below(tries.len());
        tries.rotate_left(turn);
        if let Some((source, file)) = tries.into_iter().find(|(_, f)| usable(f)) {
            scraped.push(Slide { file, title: g.name.clone(), system: g.system_name.clone(), source });
        }
    }
    let mut memories = memories(games, dirs, &mut rng);
    rng.shuffle(&mut memories);
    // One memory in three where there are enough of them; with few games, the memories fill the rest.
    let want_memories = (MAX_SLIDES / 3).max(MAX_SLIDES.saturating_sub(scraped.len()));
    memories.truncate(want_memories);
    let mut out = Vec::new();
    let (mut s, mut m) = (scraped.into_iter(), memories.into_iter());
    while out.len() < MAX_SLIDES {
        let before = out.len();
        out.extend(s.next());
        out.extend(s.next());
        out.extend(m.next());
        if out.len() == before {
            break;
        }
    }
    out.truncate(MAX_SLIDES);
    out
}

#[cfg(target_os = "linux")]
/// A screenshot file's game part: "Game Name 2026-10-24 18-05-09.png" gives "Game Name".
pub fn screenshot_title(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".png")?;
    stem.char_indices().filter(|(_, c)| *c == ' ').map(|(i, _)| i).rev().find_map(|i| crate::screenshot::valid_at(&stem[i + 1..]).then(|| stem[..i].to_string())).filter(|t| !t.is_empty())
}

#[cfg(target_os = "linux")]
/// A save-state picture's game part: "Game.pstate2.png" and "Game.pstate.auto.png" give "Game"; the undo state's picture
/// ("Before load") is no memory.
pub fn state_title(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".png")?;
    if stem.ends_with(".undo") {
        return None;
    }
    let at = stem.rfind(".pstate")?;
    let rest = &stem[at + ".pstate".len()..];
    let slot_ok = rest.is_empty() || rest == ".auto" || rest.bytes().all(|b| b.is_ascii_digit());
    (slot_ok && at > 0).then(|| stem[..at].to_string())
}

#[cfg(target_os = "linux")]
fn memories(games: &[GameRef], dirs: &Dirs, rng: &mut Rng) -> Vec<Slide> {
    let mut systems: Vec<(&str, &str)> = games.iter().map(|g| (g.system.as_str(), g.system_name.as_str())).collect();
    systems.sort_unstable();
    systems.dedup();
    // The name a library game goes by, found from its file name or its own name (a save state is named after the file).
    let mut names: HashMap<(&str, String), &str> = HashMap::new();
    for g in games {
        names.insert((&g.system, g.name.to_lowercase()), &g.name);
        if let Some(stem) = g.path.file_stem().and_then(|s| s.to_str()) {
            names.entry((&g.system, stem.to_lowercase())).or_insert(&g.name);
        }
    }
    let mut found = Vec::new();
    for (system, system_name) in systems {
        for (root, source, title_of) in [(&dirs.screenshots, Source::Screenshot, screenshot_title as fn(&str) -> Option<String>), (&dirs.states, Source::Save, state_title)] {
            let Ok(entries) = std::fs::read_dir(root.join(system)) else { continue };
            // A few of a system's files, picked at random: a big screenshots folder is not read whole into the list.
            let mut files: Vec<_> = entries.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_file())).collect();
            rng.shuffle(&mut files);
            for e in files.into_iter().take(12) {
                let name = e.file_name().to_string_lossy().into_owned();
                let Some(title) = title_of(&name) else { continue };
                let file = e.path();
                if !usable(&file) {
                    continue;
                }
                // A game that is no longer in the library goes by its file's name, tags left off as on the shelf.
                let title = names.get(&(system, title.to_lowercase())).map_or_else(|| crate::library::display_name(&title), |n| (*n).to_string());
                found.push(Slide { file, title, system: system_name.to_string(), source });
            }
        }
    }
    found
}

/// A picture ready to be shown: prepared at the size the screen shows it, a little larger for the zoom.
#[derive(Debug)]
pub struct Scene {
    pub img: Image,
    /// The picture's size on the screen at zoom 1.
    pub base_w: i32,
    pub base_h: i32,
    /// Behind a picture that does not fill the screen: its average colour, dark.
    pub backdrop: u32,
}

/// Reads `file` and prepares it for a screen of `w` x `h`: a picture whose shape is close to the screen's fills it
/// (the edges are cropped), any other (a portrait box, a wide frame) is shown whole on a dark backdrop of its own
/// colour. `None` for a file that cannot be read.
pub fn prepare(file: &Path, w: i32, h: i32) -> Option<Scene> {
    let src = image::load_image(file)?;
    scene_from(&src, w, h)
}

pub fn scene_from(src: &Image, w: i32, h: i32) -> Option<Scene> {
    if src.w == 0 || src.h == 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (sw, sh) = (w as f32, h as f32);
    let (iw, ih) = (src.w as f32, src.h as f32);
    let contain = (sw / iw).min(sh / ih);
    let cover = (sw / iw).max(sh / ih);
    let scale = if cover / contain <= 1.3 { cover } else { contain };
    let (base_w, base_h) = (((iw * scale).round() as i32).max(1), ((ih * scale).round() as i32).max(1));
    let img = src.resized((base_w as f32 * ZOOM_MAX).round() as i32, (base_h as f32 * ZOOM_MAX).round() as i32);
    let backdrop = average_dark(&img);
    Some(Scene { img, base_w, base_h, backdrop })
}

fn average_dark(img: &Image) -> u32 {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for p in img.px.iter().step_by(37) {
        r += (*p >> 16 & 0xff) as u64;
        g += (*p >> 8 & 0xff) as u64;
        b += (*p & 0xff) as u64;
        n += 1;
    }
    let n = n.max(1);
    let dim = |v: u64| (v / n * 45 / 100) as u32;
    dim(r) << 16 | dim(g) << 8 | dim(b)
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a slide shows, `local` after it began: it fades in over [`FADE`].
pub fn fade_mix(local: Duration) -> f32 {
    smooth(local.as_secs_f32() / FADE.as_secs_f32())
}

/// How far a slide's movement has come, 0-1, `local` after it began (it goes on moving while the next one fades in).
pub fn progress(local: Duration) -> f32 {
    (local.as_secs_f32() / LIFE.as_secs_f32()).clamp(0.0, 1.0)
}

/// How visible the title is, `local` after its slide began: in once the picture is mostly there, out before the next
/// slide starts to fade in.
pub fn title_alpha(local: Duration) -> f32 {
    let t = local.as_secs_f32();
    let (fade, hold) = (FADE.as_secs_f32(), HOLD.as_secs_f32());
    smooth((t - fade * 0.6) / 0.8).min(smooth((hold - 0.4 - t) / 0.6))
}

/// Where the camera is: `zoom` 1 to [`ZOOM_MAX`], `pan_x`/`pan_y` -1 to 1 (the share of the room the picture has to
/// move in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

/// The movement of the slide with number `n`, `p` (0-1) of the way: it alternates between zooming in and out, and
/// drifts in one of eight directions. `still`: Motion is Off, the picture stays at its first position.
pub fn view(n: usize, p: f32, still: bool) -> View {
    if still {
        return View { zoom: 1.0, pan_x: 0.0, pan_y: 0.0 };
    }
    let e = smooth(p);
    let (from, to) = if n & 1 == 0 { (1.0, ZOOM_MAX) } else { (ZOOM_MAX, 1.0) };
    const DIRS: [(f32, f32); 8] = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (-0.7, -0.7), (0.7, -0.7)];
    let (dx, dy) = DIRS[(n / 2 * 3 + n) % DIRS.len()];
    let along = e * 2.0 - 1.0;
    View { zoom: from + (to - from) * e, pan_x: dx * along, pan_y: dy * along }
}

/// `a` towards `b` by `t` of 256, both channels at once.
#[inline]
fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let rb = ((a & 0xff00ff) * (256 - t) + (b & 0xff00ff) * t) >> 8 & 0xff00ff;
    let g = ((a & 0xff00) * (256 - t) + (b & 0xff00) * t) >> 8 & 0xff00;
    rb | g
}

/// Where a scene lies on a screen of `w` x `h` at `view`: the top-left of the scaled picture and its size. The pan
/// never takes a filling picture's edge into the screen; a picture shown whole drifts a little within its backdrop.
pub fn place(scene: &Scene, w: i32, h: i32, v: View) -> (i32, i32, i32, i32) {
    let (pw, ph) = ((scene.base_w as f32 * v.zoom).round() as i32, (scene.base_h as f32 * v.zoom).round() as i32);
    let room = |pic: i32, screen: i32| {
        let over = ((pic - screen) / 2).max(0) as f32;
        if pic >= screen {
            over
        } else {
            screen as f32 * 0.02
        }
    };
    let ox = (w - pw) / 2 + (v.pan_x * room(pw, w)).round() as i32;
    let oy = (h - ph) / 2 + (v.pan_y * room(ph, h)).round() as i32;
    (ox, oy, pw, ph)
}

/// The scene at `v` over the canvas, `alpha` (0-255) of it: bilinear from the prepared picture, backdrop where the
/// picture does not reach.
pub fn paint(c: &mut Canvas, scene: &Scene, v: View, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let (w, h) = (c.w, c.h);
    let (ox, oy, pw, ph) = place(scene, w, h, v);
    let (x0, x1) = (ox.max(0), (ox + pw).min(w));
    let (y0, y1) = (oy.max(0), (oy + ph).min(h));
    let a = alpha as u32 + (alpha as u32 >> 7);
    let put = |dst: &mut u32, px: u32| *dst = if a >= 256 { px } else { lerp(*dst, px, a) };
    for y in 0..h {
        let row = &mut c.buf[(y * w) as usize..((y + 1) * w) as usize];
        if y < y0 || y >= y1 || x0 >= x1 {
            row.iter_mut().for_each(|d| put(d, scene.backdrop));
            continue;
        }
        row[..x0 as usize].iter_mut().for_each(|d| put(d, scene.backdrop));
        row[x1 as usize..].iter_mut().for_each(|d| put(d, scene.backdrop));
    }
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    // Source pixels per screen pixel, in 16.16.
    let (sw, sh) = (scene.img.w as i64, scene.img.h as i64);
    let step_x = (sw << 16) / pw.max(1) as i64;
    let step_y = (sh << 16) / ph.max(1) as i64;
    let cols: Vec<(usize, u32)> = (x0..x1)
        .map(|x| {
            let u = ((x - ox) as i64 * step_x + (step_x >> 1) - (1 << 15)).clamp(0, (sw - 1) << 16);
            (((u >> 16) as usize).min(sw as usize - 1), ((u >> 8) & 0xff) as u32)
        })
        .collect();
    let iw = sw as usize;
    for y in y0..y1 {
        let v = ((y - oy) as i64 * step_y + (step_y >> 1) - (1 << 15)).clamp(0, (sh - 1) << 16);
        let (sy, fy) = (((v >> 16) as usize).min(sh as usize - 1), ((v >> 8) & 0xff) as u32);
        let sy2 = (sy + 1).min(sh as usize - 1);
        let (top, bottom) = (&scene.img.px[sy * iw..(sy + 1) * iw], &scene.img.px[sy2 * iw..(sy2 + 1) * iw]);
        let row = &mut c.buf[(y * w + x0) as usize..(y * w + x1) as usize];
        for (d, &(sx, fx)) in row.iter_mut().zip(&cols) {
            let sx2 = (sx + 1).min(iw - 1);
            let t = lerp(top[sx], top[sx2], fx);
            let b = lerp(bottom[sx], bottom[sx2], fx);
            put(d, lerp(t, b, fy));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, colour: u32) -> Image {
        Image { w, h, px: vec![colour; (w * h) as usize] }
    }

    fn write_png(path: &Path, w: u32, h: u32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut enc = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&vec![120u8; (w * h * 3) as usize]).unwrap();
    }

    #[test]
    fn the_show_begins_after_a_minute_but_always_with_time_to_be_seen() {
        assert_eq!(start_after(Some(120)), Some(Duration::from_secs(60)), "the default: a minute of the two");
        assert_eq!(start_after(Some(300)), Some(Duration::from_secs(60)));
        assert_eq!(start_after(None), Some(Duration::from_secs(60)), "screen off never");
        assert_eq!(start_after(Some(60)), Some(Duration::from_secs(30)), "half of it");
        assert_eq!(start_after(Some(30)), Some(Duration::from_secs(15)));
        assert_eq!(start_after(Some(10)), None, "no room before the screen goes off");
        for off in [20, 30, 60, 120, 300] {
            let after = start_after(Some(off)).unwrap();
            assert!(after + MIN_RUN <= Duration::from_secs(off as u64), "{off}: the show is over when the screen goes off");
        }
    }

    #[test]
    fn a_slide_fades_in_moves_on_and_its_title_comes_and_goes_inside_its_own_time() {
        assert_eq!(fade_mix(Duration::ZERO), 0.0);
        assert_eq!(fade_mix(FADE), 1.0);
        assert!(fade_mix(FADE / 2) > 0.4 && fade_mix(FADE / 2) < 0.6);
        assert_eq!(progress(Duration::ZERO), 0.0);
        assert_eq!(progress(LIFE * 2), 1.0);
        assert_eq!(title_alpha(Duration::ZERO), 0.0, "no title while the picture fades in");
        assert_eq!(title_alpha(Duration::from_secs(5)), 1.0);
        assert_eq!(title_alpha(HOLD), 0.0, "gone when the next slide starts to fade in");
        assert_eq!(LIFE, HOLD + FADE);
    }

    #[test]
    fn the_camera_alternates_between_zooming_in_and_out_and_stays_inside_its_range() {
        let still = view(3, 0.5, true);
        assert_eq!((still.zoom, still.pan_x, still.pan_y), (1.0, 0.0, 0.0), "Motion off: not moving");
        assert_eq!(view(0, 0.0, false).zoom, 1.0);
        assert!((view(0, 1.0, false).zoom - ZOOM_MAX).abs() < 1e-5);
        assert!((view(1, 0.0, false).zoom - ZOOM_MAX).abs() < 1e-5);
        assert!((view(1, 1.0, false).zoom - 1.0).abs() < 1e-5);
        for n in 0..40 {
            for i in 0..=10 {
                let v = view(n, i as f32 / 10.0, false);
                assert!((1.0..=ZOOM_MAX + 1e-5).contains(&v.zoom) && v.pan_x.abs() <= 1.0 && v.pan_y.abs() <= 1.0, "{n} {i}: {v:?}");
            }
        }
    }

    #[test]
    fn a_picture_the_shape_of_the_screen_fills_it_and_another_is_shown_whole() {
        let wide = scene_from(&solid(320, 240, 0x336699), 1240, 1080).unwrap();
        assert!(wide.base_w >= 1240 && wide.base_h >= 1080, "4:3 on a 1240 x 1080 screen fills it: {}x{}", wide.base_w, wide.base_h);
        let portrait = scene_from(&solid(300, 420, 0x336699), 1240, 1080).unwrap();
        assert!(portrait.base_h <= 1080 && portrait.base_w < 1240, "a box front is shown whole: {}x{}", portrait.base_w, portrait.base_h);
        assert_eq!(portrait.img.w as f32, (portrait.base_w as f32 * ZOOM_MAX).round());
        assert_ne!(portrait.backdrop, 0, "the backdrop has the picture's colour");
        assert!(portrait.backdrop < 0x336699, "and is darker than it");
        assert!(scene_from(&Image { w: 0, h: 0, px: vec![] }, 100, 100).is_none());
    }

    #[test]
    fn painting_fills_the_screen_with_the_picture_and_the_backdrop_and_never_leaves_it() {
        let portrait = scene_from(&solid(300, 420, 0xff0000), 400, 300).unwrap();
        for v in [view(0, 0.0, false), view(0, 1.0, false), view(1, 0.5, false), view(5, 0.9, false)] {
            let mut buf = vec![0x00ff00u32; 400 * 300];
            paint(&mut Canvas::new(&mut buf, 400, 300), &portrait, v, 255);
            assert!(buf.iter().all(|p| *p == 0xff0000 || *p == portrait.backdrop), "{v:?}: only the picture and its backdrop");
            assert!(buf.contains(&0xff0000) && buf.contains(&portrait.backdrop));
        }
        let filling = scene_from(&solid(400, 300, 0x112233), 400, 300).unwrap();
        for v in [view(0, 0.0, false), view(0, 1.0, false), view(2, 0.5, false), view(7, 1.0, false)] {
            let mut buf = vec![0u32; 400 * 300];
            paint(&mut Canvas::new(&mut buf, 400, 300), &filling, v, 255);
            assert!(buf.iter().all(|p| *p == 0x112233), "{v:?}: a filling picture never shows its edge");
        }
    }

    #[test]
    fn alpha_mixes_the_picture_into_what_is_there() {
        let s = scene_from(&solid(400, 300, 0xffffff), 400, 300).unwrap();
        let mut none = vec![0x102030u32; 400 * 300];
        paint(&mut Canvas::new(&mut none, 400, 300), &s, view(0, 0.5, false), 0);
        assert!(none.iter().all(|p| *p == 0x102030), "alpha 0 changes nothing");
        let mut half = vec![0u32; 400 * 300];
        paint(&mut Canvas::new(&mut half, 400, 300), &s, view(0, 0.5, false), 128);
        let p = half[150 * 400 + 200];
        assert!((0x7e..=0x82).contains(&(p & 0xff)) && (0x7e..=0x82).contains(&(p >> 16 & 0xff)), "half way: {p:06x}");
    }

    #[test]
    fn screenshot_and_state_names_give_the_game_back() {
        assert_eq!(screenshot_title("Super Mario Land 2026-10-24 18-05-09.png").as_deref(), Some("Super Mario Land"));
        assert_eq!(screenshot_title("Game 2 2026-10-24 18-05-09 (2).png").as_deref(), Some("Game 2"));
        assert_eq!(screenshot_title("holiday.png"), None);
        assert_eq!(screenshot_title("2026-10-24 18-05-09.png"), None, "no game part");
        assert_eq!(state_title("Tetris.pstate.png").as_deref(), Some("Tetris"));
        assert_eq!(state_title("Tetris.pstate3.png").as_deref(), Some("Tetris"));
        assert_eq!(state_title("Tetris.pstate.auto.png").as_deref(), Some("Tetris"));
        assert_eq!(state_title("Tetris.pstate.undo.png"), None, "Before load is no memory");
        assert_eq!(state_title("Tetris.png"), None);
        assert_eq!(state_title("Tetris.pstate.auto"), None);
    }

    fn lib(dir: &Path) -> (Vec<GameRef>, Dirs) {
        let games = vec![
            GameRef { system: "gbc".into(), system_name: "Game Boy Color".into(), name: "Alpha".into(), path: dir.join("roms/gbc/Alpha.gbc") },
            GameRef { system: "gbc".into(), system_name: "Game Boy Color".into(), name: "Beta".into(), path: dir.join("roms/gbc/Beta.gbc") },
            GameRef { system: "snes".into(), system_name: "Super Nintendo".into(), name: "Gamma".into(), path: dir.join("roms/snes/Gamma.sfc") },
        ];
        let dirs = Dirs { data: dir.join("data"), screenshots: dir.join("shots"), states: dir.join("states"), scraped_pictures: true };
        (games, dirs)
    }

    /// `cargo test --release --lib paint_cost -- --ignored --nocapture`: what a frame costs on this machine (not a gate: it
    /// depends on the host; the figures in docs/SLEEP.md are from it).
    #[test]
    #[ignore]
    fn paint_cost() {
        for (w, h) in [(1240i32, 1080i32), (640, 480)] {
            let scene = scene_from(&solid(480, 320, 0x336699), w, h).unwrap();
            let mut buf = vec![0u32; (w * h) as usize];
            let mut c = Canvas::new(&mut buf, w as u32, h as u32);
            let start = std::time::Instant::now();
            for i in 0..40 {
                paint(&mut c, &scene, view(0, i as f32 / 40.0, false), 255);
            }
            let each = start.elapsed() / 40;
            let fade_start = std::time::Instant::now();
            for i in 0..40 {
                paint(&mut c, &scene, view(1, i as f32 / 40.0, false), 128);
            }
            println!("{w}x{h}: {each:?} a frame, {:?} blended", fade_start.elapsed() / 40);
        }
    }

    #[test]
    fn the_words_of_the_show_are_in_german_too() {
        for caption in [Source::Screenshot, Source::Save].into_iter().filter_map(Source::caption) {
            assert!(crate::i18n::german().exact(caption).is_some(), "{caption} has no German line");
        }
    }

    #[test]
    fn a_library_without_pictures_has_no_show() {
        let d = tempfile::tempdir().unwrap();
        let (games, dirs) = lib(d.path());
        assert!(collect(&games, &dirs, 7).is_empty());
        assert!(collect(&[], &dirs, 7).is_empty());
    }

    #[test]
    fn the_show_takes_real_pictures_only_and_mixes_in_the_players_own() {
        let d = tempfile::tempdir().unwrap();
        let (games, dirs) = lib(d.path());
        write_png(&dirs.data.join("covers/gbc/Alpha.png"), 120, 160);
        write_png(&dirs.data.join("snaps/snes/Gamma.png"), 256, 224);
        // Too small, a thumbnail: not used.
        write_png(&dirs.data.join("covers/gbc/Beta.png"), 16, 16);
        // A file that is not a picture.
        std::fs::create_dir_all(dirs.data.join("titles/gbc")).unwrap();
        std::fs::write(dirs.data.join("titles/gbc/Beta.png"), b"not a png").unwrap();
        write_png(&dirs.screenshots.join("gbc/Beta 2026-10-24 18-05-09.png"), 160, 144);
        write_png(&dirs.states.join("snes/Gamma.pstate2.png"), 256, 224);
        write_png(&dirs.states.join("snes/Gamma.pstate.undo.png"), 256, 224);
        // A game that is not (any more) in the library: its file name as the shelf would say it.
        write_png(&dirs.states.join("gbc/Delta (USA) (Rev 1).pstate.auto.png"), 160, 144);
        let slides = collect(&games, &dirs, 42);
        let titles: Vec<_> = slides.iter().map(|s| (s.title.as_str(), s.source)).collect();
        assert!(titles.contains(&("Alpha", Source::Cover)), "{titles:?}");
        assert!(titles.contains(&("Gamma", Source::Snap)), "{titles:?}");
        assert!(titles.contains(&("Beta", Source::Screenshot)), "the player's screenshot, under the library's name: {titles:?}");
        assert!(titles.contains(&("Gamma", Source::Save)), "{titles:?}");
        assert!(titles.contains(&("Delta", Source::Save)), "a game that left the library goes by its cleaned file name: {titles:?}");
        assert_eq!(slides.len(), 5, "the thumbnail, the broken file and the undo state are left out: {titles:?}");
        assert!(slides.iter().all(|s| usable(&s.file)));
        let beta = slides.iter().find(|s| s.title == "Beta").unwrap();
        assert_eq!(beta.system, "Game Boy Color");
        assert!(Source::Screenshot.caption().is_some() && Source::Cover.caption().is_none());
    }

    #[test]
    fn each_slide_names_the_system_of_the_game_it_belongs_to_also_for_two_games_of_one_title() {
        let d = tempfile::tempdir().unwrap();
        let games = vec![
            GameRef { system: "snes".into(), system_name: "Super Nintendo".into(), name: "Twin".into(), path: d.path().join("roms/snes/Twin (USA).sfc") },
            GameRef { system: "psx".into(), system_name: "PlayStation".into(), name: "Twin".into(), path: d.path().join("roms/psx/Twin (Japan).chd") },
        ];
        let dirs = Dirs { data: d.path().join("data"), screenshots: d.path().join("shots"), states: d.path().join("states"), scraped_pictures: true };
        // Each has its own box art; the player's screenshot is of the SNES game, the save picture of the PlayStation one.
        write_png(&crate::covers::cover_path(&dirs.data, "snes", &games[0].path), 120, 160);
        write_png(&crate::covers::cover_path(&dirs.data, "psx", &games[1].path), 130, 130);
        write_png(&dirs.screenshots.join("snes/Twin 2026-10-24 18-05-09.png"), 256, 224);
        write_png(&dirs.states.join("psx/Twin (Japan).pstate.png"), 320, 240);
        for seed in 1..30 {
            let slides = collect(&games, &dirs, seed);
            assert_eq!(slides.len(), 4, "seed {seed}: {slides:?}");
            for s in &slides {
                assert_eq!(s.title, "Twin");
                let expected = match s.source {
                    Source::Cover if s.file.starts_with(dirs.data.join("covers/snes")) => "Super Nintendo",
                    Source::Cover => "PlayStation",
                    Source::Screenshot => "Super Nintendo",
                    Source::Save => "PlayStation",
                    other => panic!("{other:?}"),
                };
                assert_eq!(s.system, expected, "{s:?}");
            }
        }
    }

    #[test]
    fn box_art_only_leaves_the_scraped_screenshots_out() {
        let d = tempfile::tempdir().unwrap();
        let (games, mut dirs) = lib(d.path());
        write_png(&dirs.data.join("snaps/snes/Gamma.png"), 256, 224);
        write_png(&dirs.data.join("covers/gbc/Alpha.png"), 120, 160);
        assert_eq!(collect(&games, &dirs, 3).len(), 2);
        dirs.scraped_pictures = false;
        let slides = collect(&games, &dirs, 3);
        assert_eq!(slides.len(), 1, "{slides:?}");
        assert_eq!(slides[0].source, Source::Cover);
    }

    #[test]
    fn a_big_library_is_cut_to_the_most_slides_with_memories_among_them() {
        let d = tempfile::tempdir().unwrap();
        let games: Vec<GameRef> = (0..80)
            .map(|i| GameRef { system: "gba".into(), system_name: "Game Boy Advance".into(), name: format!("G{i}"), path: d.path().join(format!("roms/G{i}.gba")) })
            .collect();
        let dirs = Dirs { data: d.path().join("data"), screenshots: d.path().join("shots"), states: d.path().join("states"), scraped_pictures: true };
        for g in &games {
            write_png(&crate::covers::cover_path(&dirs.data, "gba", &g.path), 100, 100);
        }
        for i in 0..20 {
            write_png(&dirs.screenshots.join(format!("gba/G{i} 2026-10-24 18-05-{i:02}.png")), 240, 160);
        }
        let slides = collect(&games, &dirs, 9);
        assert_eq!(slides.len(), MAX_SLIDES);
        let memories = slides.iter().filter(|s| s.source.is_memory()).count();
        assert!((8..=14).contains(&memories), "about a third are the player's own: {memories}");
        let again = collect(&games, &dirs, 10);
        assert_ne!(slides, again, "another start shuffles");
    }
}
