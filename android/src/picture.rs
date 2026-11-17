//! Android player picture policy. Geometry is independent of the density-scaled menu canvas; with a controller it is
//! Linux's (`Settings::place`).
use pocket_ui::{bezel::{self, Cabinet, Placed, Style}, core_options::{FileOption, FileValue}, touch::Area, video::{dest_rect, Rect, Scaling}};
use std::path::{Path, PathBuf};

pub const ROWS: usize = 4;

/// The pixels the game's picture is placed in: `game` is in `size` units, laid onto `view` (physical: left, bottom, width,
/// height, as GL's viewport). The crt switch-on, the frame and the picture itself are all drawn in this space
/// (`Graphics::present`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Space {
    pub size: (i32, i32),
    pub view: (i32, i32, i32, i32),
    pub game: Area,
}
impl Space {
    /// The picture on a window `window_h` high, physical pixels from the top-left (x, y, width, height): the very
    /// arithmetic of `Graphics::present`'s viewport.
    pub fn on_window(&self, window_h: i32) -> (i32, i32, i32, i32) {
        let (vw, vh) = (self.view.2 as i64, self.view.3 as i64);
        let (lw, lh) = (self.size.0.max(1) as i64, self.size.1.max(1) as i64);
        let g = self.game;
        let (w, h) = ((g.w as i64 * vw / lw) as i32, (g.h as i64 * vh / lh) as i32);
        let bottom = self.view.1 + self.view.3 - ((g.y + g.h) as i64 * vh / lh) as i32;
        (self.view.0 + (g.x as i64 * vw / lw) as i32, window_h - bottom - h, w, h)
    }
    /// The picture in the overlay's pixels (`logical` laid onto `view`), for touches on it and the way back's picture.
    pub fn in_overlay(&self, window_h: i32, view: (i32, i32, i32, i32), logical: (i32, i32)) -> Area {
        if self.size == logical && self.view == view { return self.game; }
        let (x, y, w, h) = self.on_window(window_h);
        let top = window_h - view.1 - view.3;
        let (kx, ky) = (logical.0 as f64 / view.2.max(1) as f64, logical.1 as f64 / view.3.max(1) as f64);
        let (x0, y0) = (((x - view.0) as f64 * kx).round() as i32, ((y - top) as f64 * ky).round() as i32);
        let (x1, y1) = (((x + w - view.0) as f64 * kx).round() as i32, ((y + h - top) as f64 * ky).round() as i32);
        Area::new(x0, y0, x1 - x0, y1 - y0)
    }
}
#[derive(Clone)]
pub struct Settings {
    pub scaling: usize,
    pub frame: usize,
    pub effect: usize,
    /// Over the game: FPS in the top left corner (Options → Over the game, Linux's Speed, rumble, fan row), kept with the
    /// picture's choices for this console or this game.
    pub over_game: bool,
    game: bool,
    system_path: PathBuf,
    game_path: PathBuf,
}
impl Settings {
    pub fn load(data: &Path, system: &str, key: &str) -> Self {
        let system_path = data.join("picture/systems").join(format!("{system}.json"));
        let game_path = data.join("picture/games").join(format!("{key}.json"));
        let read = |p: &Path| pocket_ui::fsutil::read_json_or_backup::<serde_json::Value>(p);
        let per_game = read(&game_path);
        let game = per_game.is_some();
        let value = per_game.or_else(|| read(&system_path)).unwrap_or_default();
        let get = |key: &str, max: usize| value[key].as_u64().unwrap_or(0).min(max as u64) as usize;
        Self { scaling: get("scaling", 2), frame: get("frame", 2), effect: get("effect", 3), over_game: get("over_game", 1) == 1, game, system_path, game_path }
    }
    /// The handheld view's choice for this game (`launchPath`'s settings, `picture`: Options → Picture there): what the page
    /// there shows is what is drawn. Not saved here; a change in the pause menu is saved as it always is.
    pub fn follow_view(&mut self, view: &serde_json::Value) {
        let get = |key: &str, max: usize| view[key].as_u64().map(|v| v.min(max as u64) as usize);
        self.scaling = get("scaling", 2).unwrap_or(self.scaling);
        self.frame = get("frame", 2).unwrap_or(self.frame);
        self.effect = get("effect", 3).unwrap_or(self.effect);
        self.over_game = get("over_game", 1).map_or(self.over_game, |v| v == 1);
    }
    /// Keep for: this game (true) or every game of the console.
    pub fn for_game(&self) -> bool {
        self.game
    }
    /// Over the game on or off, kept as the picture's choices are.
    pub fn set_over_game(&mut self, on: bool) -> Result<(), String> {
        self.over_game = on;
        self.save()
    }
    pub fn options(&self) -> Vec<FileOption> {
        [("Scaling", &["Fit / aspect", "Integer", "Stretch / fullscreen"][..], self.scaling),
         ("Screen frame", &["None", "Console", "CRT TV"][..], self.frame),
         ("Picture effect", &["Sharp", "Smooth", "CRT", "Scanlines"][..], self.effect),
         ("Save settings for", &["This system", "This game"][..], usize::from(self.game))]
        .into_iter().map(|(label, values, current)| FileOption {
            key: format!("picture.{label}"), label: label.into(),
            current: values[current].into(), default: values[0].into(),
            values: values.iter().map(|s| FileValue { value: (*s).into(), label: (*s).into() }).collect(),
            ..FileOption::default()
        }).collect()
    }
    pub fn change(&mut self, row: usize, delta: i32) -> Result<(), String> {
        let (at, count) = match row {
            0 => (&mut self.scaling, 3), 1 => (&mut self.frame, 3),
            2 => (&mut self.effect, 4), 3 => { self.game = !self.game; return self.save(); },
            _ => return Ok(()),
        };
        *at = (*at as i32 + delta).rem_euclid(count) as usize;
        self.save()
    }
    fn save(&self) -> Result<(), String> {
        let path = if self.game { &self.game_path } else { &self.system_path };
        let bytes = serde_json::to_vec(&serde_json::json!({"scaling":self.scaling,"frame":self.frame,"effect":self.effect,"over_game":u8::from(self.over_game)})).map_err(|e| e.to_string())?;
        pocket_ui::fsutil::write_atomic(path, &bytes).map_err(|e| e.to_string())?;
        // Selecting the system scope explicitly returns this game to the system policy.
        if !self.game && self.game_path.exists() {
            std::fs::remove_file(&self.game_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// The frame these settings ask for: None draws none; Console is the console's own, as Linux's Auto picks it
    /// (`bezel::style_for`: the handheld's shell, the monitor surround for the others); CRT TV is Linux's CRT frame in its
    /// start cabinet. The same pictures Linux draws, by the same code.
    pub fn frame_style(&self, system: &str) -> Option<Style> {
        match self.frame {
            1 => Some(bezel::style_for(system)),
            2 => Some(Style::Crt(Cabinet::default())),
            _ => None,
        }
    }
    /// How much of the player's height an upright phone keeps for the touch pad: the picture and its frame stand above it.
    fn room_h(size: (i32, i32), touch: bool) -> i32 {
        let (w, h) = size;
        if w < h && touch { (h - 510).max(h / 2) } else { h }
    }
    /// The space around the picture a frame needs, on every side: 8 % of the room's shorter side (the overlay is laid out
    /// at about 2.2 px per dp, so a phone's 411 dp are about 900), enough for a handheld's body to show, little enough that the
    /// picture keeps about four fifths of its size.
    fn frame_margin(size: (i32, i32), touch: bool) -> i32 {
        ((size.0.min(Self::room_h(size, touch)) as f32 * 0.08) as i32).max(24)
    }
    pub fn rect(&self, size: (i32, i32), physical: (i32, i32), native: (u32, u32), reported: f64, touch: bool) -> Area {
        let (w, h) = size;
        let portrait = w < h;
        let room_h = Self::room_h(size, touch);
        let margin = if self.frame == 0 { 0 } else { Self::frame_margin(size, touch) };
        let (rw, rh) = ((w - margin * 2).max(1), (room_h - margin * 2).max(1));
        let aspect = if reported.is_finite() && (0.05..20.0).contains(&reported) { reported } else { native.0 as f64 / native.1.max(1) as f64 };
        let (iw, ih) = if self.scaling == 2 { (rw, rh) } else {
            let mut ih = (rh as f64).min(rw as f64 / aspect);
            if self.scaling == 1 {
                // Quantize on physical scanlines, not density-independent menu pixels. Retain
                // display aspect for non-square-pixel cores; small panels fall back to Fit.
                let py = physical.1 as f64 / h.max(1) as f64;
                let step = native.1.max(1) as f64;
                let multiple = (ih * py / step).floor();
                if multiple >= 1.0 { ih = multiple * step / py; }
            }
            ((ih * aspect).round().max(1.0) as i32, ih.round().max(1.0) as i32)
        };
        // A frame stands even above and below the picture; without one an upright phone keeps it at the top.
        let y = if portrait && self.frame == 0 { margin } else { (room_h - ih) / 2 };
        Area::new((w - iw) / 2, y, iw, ih)
    }
    /// Where the game's picture goes on a `window` (physical pixels) whose safe part is `view` (physical: left, bottom,
    /// width, height, as GL's viewport) with the overlay laid out on `logical`.
    ///
    /// With the touch pad on the screen the picture keeps to the safe area, where the pad is (`rect`). With a controller
    /// (built in or attached, no touch pad) it is placed on the whole screen exactly as pocket-core places it on Linux
    /// (`video::dest_rect`, physical pixels): the insets are gesture zones and bars a picture needs no taps in, and an
    /// integer scale counts the screen's own lines (Odin 2 Mini, 1920x1080: GBA 6x = 1440x960, not 5x in the 1782x951
    /// the safe area leaves). As on Linux a frame fills what the picture leaves and never makes it smaller.
    pub fn place(&self, window: (i32, i32), view: (i32, i32, i32, i32), logical: (i32, i32), native: (u32, u32), reported: f64, touch: bool) -> Space {
        if touch || window.0 <= 0 || window.1 <= 0 {
            return Space { size: logical, view, game: self.rect(logical, (view.2, view.3), native, reported, touch) };
        }
        let scaling = match self.scaling { 0 => Scaling::Aspect, 2 => Scaling::Fullscreen, _ => Scaling::Native };
        let aspect = if reported.is_finite() && (0.05..20.0).contains(&reported) { reported as f32 } else { 0.0 };
        let r = dest_rect(scaling, window.0 as u32, window.1 as u32, native.0, native.1, aspect);
        Space { size: window, view: (0, 0, window.0, window.1), game: Area::new(r.x as i32, r.y as i32, r.w as i32, r.h as i32) }
    }
    /// The part of the player the frame may fill (logical pixels): above the pad on an upright phone, between the pad's two
    /// sides across, the whole screen with no pad. `pad` is where the pad reaches (`touch_pad_extent`: the inset from each side and
    /// the top of its highest button). Never smaller than the picture: where the pad reaches over the picture it lies over the
    /// frame too, as it does over the picture.
    pub fn frame_room(size: (i32, i32), touch: bool, game: Area, pad: (i32, i32)) -> Area {
        let (w, h) = size;
        if !touch { return Area::new(0, 0, w, h); }
        if w < h {
            let bottom = Self::room_h(size, true).min(pad.1 - 8).max(game.y + game.h);
            return Area::new(0, 0, w, bottom);
        }
        let x = pad.0.min(game.x).max(0);
        Area::new(x, 0, w - 2 * x, h)
    }
    /// The frame for a picture at `game` in `room` (both logical, in the `size` the player's overlay is laid out on),
    /// placed on the viewport `view` = (left, bottom, width, height) the player draws in: the picture's pixels are the very
    /// ones `Graphics::present` gives the picture, the room is only inside the viewport, so a system bar, a notch or a
    /// gesture zone (the viewport's insets) is never under the frame.
    pub fn frame_plan(&self, system: &str, size: (i32, i32), view: (i32, i32, i32, i32), room: Area, game: Area, native_w: u32) -> Option<Placed> {
        let style = self.frame_style(system)?;
        let (vw, vh) = (view.2.max(1) as i64, view.3.max(1) as i64);
        let (lw, lh) = (size.0.max(1) as i64, size.1.max(1) as i64);
        // The same arithmetic as `Graphics::present`'s viewports: x, y from the bottom, width, height.
        let phys = |a: Area| ((a.x as i64 * vw / lw) as i32, (vh - (a.y + a.h) as i64 * vh / lh) as i32, (a.w as i64 * vw / lw) as i32, (a.h as i64 * vh / lh) as i32);
        let (gx, gy, gw, gh) = phys(game);
        let (rx, ry, rw, rh) = phys(room);
        // Rounding must never leave the picture outside its room.
        let (x0, y0) = (rx.min(gx).max(0), ry.min(gy).max(0));
        let (x1, y1) = ((rx + rw).max(gx + gw).min(view.2), (ry + rh).max(gy + gh).min(view.3));
        let (w, h) = ((x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32);
        let hole = Rect { x: (gx - x0).max(0) as u32, y: (y1 - gy - gh).max(0) as u32, w: gw.max(1) as u32, h: gh.max(1) as u32 };
        Some(Placed { style, room: (x0, y0, w, h), hole, px: gw.max(1) as f32 / native_w.max(1) as f32 })
    }
}
