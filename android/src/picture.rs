//! Android player picture policy. Geometry is independent of the density-scaled menu canvas; with a controller it is
//! Linux's (`Settings::place`).
use pocket_ui::{bezel::{self, Placed, Style}, core_options::{FileOption, FileValue}, touch::Area, video::{dest_rect, Rect, Scaling}};
use std::collections::BTreeMap;
use std::path::Path;
use pocket_ui::pause_options;

/// The Mobile sheet's Picture rows (Linux's settings), then Keep for: `ROWS` in all.
pub const SHEET: [&str; 3] = ["scaling", "frame", "look"];
pub const ROWS: usize = SHEET.len() + 1;

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
/// The player's Picture choices and the other per-game rows it keeps (Over the game, Resolution, Earlier states), as Linux
/// keeps them: Linux's keys and values in the game's or the console's file of `game_settings` (the Handheld view's files for
/// a game it started, so its This game page and Linux's pause menu read the same; the player's own for the Mobile view).
/// What neither file has comes from what the view handed over (`handed`: its effective values, Settings' globals included),
/// else Linux's defaults. The indexes the player's rows and geometry use (`scaling`, `frame`, `effect`) are worked out from
/// the values (`derive`), never saved.
#[derive(Clone)]
pub struct Settings {
    /// How the touch layout places the picture: Aspect 0, Integer 1, Fullscreen 2 (from `video.scaling`).
    pub scaling: usize,
    /// Whether a frame is drawn, for the room around the picture: none 0, the CRT TV 2, any other 1 (from `video.frame`;
    /// the frame drawn is `frame_style`'s).
    pub frame: usize,
    /// Over the game: FPS in the top left corner (Options → Over the game, Linux's Speed, rumble, fan row).
    pub over_game: bool,
    /// Options → Picture → Resolution (`render_scale`): the factor chosen (1 = Original … 4), None while nobody chose one
    /// (the handheld's default). Read when the game starts, as pocket-core reads Linux's.
    pub resolution: Option<u8>,
    /// Options → Earlier states: None while nobody chose (`timeline_policy::default_on`).
    pub earlier: Option<bool>,
    /// Speed, rumble, fan › Fast-forward speed and Rewind (`play_speed`): Linux's 4x and Off to begin with.
    pub ff_speed: pocket_ui::play_speed::FastForward,
    pub rewind: bool,
    /// The Picture values as Linux reads them (`video_settings`): the frame and its finish, the shader variant.
    pub video: pocket_ui::video::VideoSettings,
    files: pocket_ui::game_settings::Files,
    level: pocket_ui::game_settings::Level,
    /// This game's or its console's own values (`level` says whose), Linux's keys.
    own: BTreeMap<String, String>,
    handed: BTreeMap<String, String>,
}

/// The player's Picture files from before 18.11.2026 (`picture/games/<key>.json`, `picture/systems/<system>.json`: indexes
/// scaling Fit/Integer/Stretch, frame None/Console/CRT TV, effect Sharp/Smooth/CRT/Scanlines, over_game, resolution,
/// earlier) taken over once into the player's `game-settings` as Linux's values; the old file goes once its values are
/// written. A file `game-settings` already has is left as it is (and the old one goes).
pub fn migrate_old(data: &Path, system: &str, key: &str) {
    let files = pocket_ui::game_settings::Files::new(data, system, Path::new(key));
    for (old, new) in [(data.join("picture/games").join(format!("{key}.json")), files.game_file()), (data.join("picture/systems").join(format!("{system}.json")), files.console_file())] {
        let Some(v) = pocket_ui::fsutil::read_json_or_backup::<serde_json::Value>(&old) else { continue };
        if !new.exists() {
            let values = old_values(&v);
            if pocket_ui::fsutil::write_json(&new, &values).is_err() {
                continue;
            }
        }
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::remove_file(old.with_extension("json.bak"));
    }
}

/// An old file's indexes as Linux's values.
pub fn old_values(v: &serde_json::Value) -> BTreeMap<String, String> {
    let index = |k: &str, n: u64| v[k].as_u64().map(|i| i.min(n - 1) as usize);
    let mut out = BTreeMap::new();
    if let Some(i) = index("scaling", 3) {
        out.insert("scaling".into(), ["aspect", "native", "fullscreen"][i].to_string());
    }
    if let Some(i) = index("frame", 3) {
        out.insert("frame".into(), ["none", "auto", "crt"][i].to_string());
    }
    if let Some(i) = index("effect", 4) {
        // Sharp, Smooth, CRT, Scanlines: Linux's Looks Clean, Smooth, Old TV, Scanlines (`look::Look::canonical`).
        let effect = ["none", "smooth", "crt", "scanlines"][i];
        out.insert("sharpness".into(), "crisp".into());
        out.insert("effect".into(), effect.into());
    }
    if let Some(on) = v["over_game"].as_u64() {
        out.insert(pause_options::OVER_GAME_KEY.into(), pause_options::OVER_GAME_VALUES[usize::from(on == 1)].into());
    }
    if let Some(f) = v["resolution"].as_u64().filter(|f| (1..=4).contains(f)) {
        out.insert(pocket_ui::render_scale::KEY.into(), f.to_string());
    }
    if let Some(on) = v["earlier"].as_u64() {
        out.insert(pocket_ui::timeline_policy::KEY.into(), if on == 1 { "on" } else { "off" }.into());
    }
    out
}

/// Where a game from the Mobile view starts when nothing is saved: as large as fits (the touch pad needs the room).
pub const MOBILE_START: [(&str, &str); 1] = [("scaling", "aspect")];

/// `MOBILE_START` as the values a Mobile game is handed.
pub fn mobile_start() -> BTreeMap<String, String> {
    MOBILE_START.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

impl Settings {
    /// The game's settings in `data` (the folder holding `game-settings`): `game` names its file (the Handheld view's ROM,
    /// whose stem Linux uses; the player's stable key for a Mobile game). `handed`: what the starting view says the game
    /// uses where no file of these has a value.
    pub fn load(data: &Path, system: &str, game: &Path, handed: BTreeMap<String, String>) -> Self {
        let files = pocket_ui::game_settings::Files::new(data, system, game);
        let (own, level) = files.read();
        let mut s = Self { scaling: 0, frame: 0, over_game: false, resolution: None, earlier: None, ff_speed: Default::default(), rewind: false, video: Default::default(), files, level, own, handed };
        s.derive();
        s
    }
    /// The value in use for `key`: the file's, else the view's, else None (Linux's default).
    pub fn value(&self, key: &str) -> Option<&str> {
        self.own.get(key).or_else(|| self.handed.get(key)).map(String::as_str)
    }
    /// Linux's values in use (`effective`), for the Picture page's rows (`pause_options::picture_settings`).
    pub fn values(&self) -> BTreeMap<String, String> {
        self.effective()
    }
    /// Linux's `pairs` from a row of the Picture page, kept as Keep for says.
    pub fn set_values(&mut self, pairs: &[(&'static str, String)]) -> Result<(), String> {
        self.set(pairs)
    }
    fn effective(&self) -> BTreeMap<String, String> {
        let mut v = self.handed.clone();
        v.extend(self.own.iter().map(|(k, v)| (k.clone(), v.clone())));
        v
    }
    /// The indexes and the drawing values from the values in use.
    fn derive(&mut self) {
        self.video = pocket_ui::video::VideoSettings::from_values(&self.effective());
        use pocket_ui::video::{FrameMode, Scaling};
        self.scaling = match self.video.scaling {
            Scaling::Aspect => 0,
            Scaling::Fullscreen => 2,
            Scaling::Native | Scaling::IntegerSmaller => 1,
        };
        self.frame = match self.video.frame {
            FrameMode::None | FrameMode::Ambient => 0,
            FrameMode::Crt => 2,
            _ => 1,
        };
        self.over_game = self.value(pause_options::OVER_GAME_KEY).is_some_and(|v| v != "off");
        self.resolution = self.value(pocket_ui::render_scale::KEY).and_then(|v| v.parse::<u8>().ok()).filter(|f| (1..=4).contains(f));
        self.earlier = self.value(pocket_ui::timeline_policy::KEY).map(|v| v == "on");
        self.ff_speed = self.value(pocket_ui::play_speed::FF_KEY).map(pocket_ui::play_speed::FastForward::parse).unwrap_or_default();
        self.rewind = self.value(pocket_ui::play_speed::REWIND_KEY) == Some("on");
    }
    /// Options › Controls' choices (`ctl.*`, `controls::ControlsChange`), in the same file as the picture's, kept as Keep
    /// for says, as Linux's pause menu keeps them (`emu_menu::control_step`): `set` entries where they differ from the
    /// console's own, `remove` the ones back to it; `reset` takes every `ctl.*` out (Reset controls).
    pub fn set_controls(&mut self, set: &[(String, String)], remove: &[String], reset: bool) -> Result<(), String> {
        if reset {
            self.own.retain(|k, _| !k.starts_with(pocket_ui::controls::KEY_PREFIX));
        }
        for k in remove {
            self.own.remove(k);
        }
        for (k, v) in set {
            self.own.insert(k.clone(), v.clone());
        }
        self.keep_own()
    }
    /// Sets Linux's `pairs` and keeps them where Keep for says, as pocket-core's pause menu does (`GameSettings::keep`).
    fn set(&mut self, pairs: &[(&'static str, String)]) -> Result<(), String> {
        for (k, v) in pairs {
            self.own.insert(k.to_string(), v.clone());
        }
        self.keep_own()
    }
    /// The values of this game or its console kept where Keep for says.
    fn keep_own(&mut self) -> Result<(), String> {
        // Written now, not by the card writer (`Files::keep`): the player is a process of its own that may end with the
        // menu, and a refused write says so in the menu (`SETTING_FAILED`).
        let console = self.level == pocket_ui::game_settings::Level::Console;
        let ok = if console { self.files.save_for_console(&self.own) } else { self.files.save_for_game(&self.own) };
        if !console {
            self.level = pocket_ui::game_settings::Level::Game;
        }
        self.derive();
        if ok { Ok(()) } else { Err(pocket_ui::save_trouble::SETTING_FAILED.into()) }
    }
    /// Keep for: this game (true; also while nothing is saved, as Linux's) or every game of the console.
    pub fn for_game(&self) -> bool {
        self.level != pocket_ui::game_settings::Level::Console
    }
    /// The shader variant the picture is drawn with: the Look's, the GBA screen's colours with the LCD grid.
    pub fn variant(&self, system: &str) -> pocket_ui::video_program::Variant {
        let v = &self.video;
        let lcd = v.lcd_colour && v.effect == pocket_ui::video::Effect::Grid && system == "gba";
        (v.sharpness, v.effect, lcd)
    }
    /// Over the game on or off, kept as the picture's choices are.
    pub fn set_over_game(&mut self, on: bool) -> Result<(), String> {
        self.set(&[(pause_options::OVER_GAME_KEY, pause_options::OVER_GAME_VALUES[usize::from(on)].to_string())])
    }
    /// Options → Picture → Resolution as the core gets it at this start (`render_scale::for_start`, what pocket-core gives
    /// it on Linux): `range` is this handheld's for the console (`render_scale::here`), `core` the core's library.
    pub fn start_resolution(&self, range: Option<pocket_ui::render_scale::Range>, core: &Path) -> Option<pocket_ui::render_scale::StartOption> {
        let chosen = self.resolution.map(|f| f.to_string());
        pocket_ui::render_scale::for_start(range, &pocket_ui::core_options::core_id(core), chosen.as_deref())
    }
    /// Resolution chosen (a factor, 1 = Original); the core gets it at the next start.
    pub fn set_resolution(&mut self, factor: u8) -> Result<(), String> {
        self.set(&[(pocket_ui::render_scale::KEY, factor.clamp(1, 4).to_string())])
    }
    /// Earlier states on or off.
    pub fn set_earlier(&mut self, on: bool) -> Result<(), String> {
        self.set(&[(pocket_ui::timeline_policy::KEY, if on { "on" } else { "off" }.to_string())])
    }
    /// Fast-forward speed chosen, kept as the other rows are.
    pub fn set_ff_speed(&mut self, speed: pocket_ui::play_speed::FastForward) -> Result<(), String> {
        self.set(&[(pocket_ui::play_speed::FF_KEY, speed.value())])
    }
    /// Rewind on or off.
    pub fn set_rewind(&mut self, on: bool) -> Result<(), String> {
        self.set(&[(pocket_ui::play_speed::REWIND_KEY, if on { "on" } else { "off" }.to_string())])
    }
    /// Whether Earlier keeps its moments for `system`: the choice, else Linux's starting value for the console.
    pub fn earlier_on(&self, system: &str) -> bool {
        self.earlier.unwrap_or_else(|| pocket_ui::timeline_policy::default_on(system))
    }
    /// Restore defaults: the console's and the game's files go, every row is back to where it starts (Linux's
    /// `GameSettings::restore_defaults`).
    pub fn restore_defaults(&mut self) -> Result<(), String> {
        self.files.restore_defaults();
        self.own.clear();
        self.level = pocket_ui::game_settings::Level::Defaults;
        self.derive();
        Ok(())
    }
    /// The Mobile view's sheet rows (`ROWS`): Linux's Screen scaling, Screen frame and Look with Linux's values and words for
    /// the console (`pause_options::setting_choices`), Keep for last.
    pub fn options(&self) -> Vec<FileOption> {
        let system = self.files.system();
        let values = self.effective();
        let mut out: Vec<FileOption> = SHEET.iter().map(|key| {
            let setting = pocket_ui::option_table::setting(key).expect("a Picture setting");
            let choices = pause_options::setting_choices(key, system, None);
            let current = pause_options::setting_value_of(key, system, None, &values);
            let label = |v: &str| pocket_ui::option_table::label_in(setting, v, true).to_string();
            FileOption {
                key: format!("picture.{key}"), label: setting.name.into(),
                current: label(current), default: choices.first().map_or(String::new(), |c| label(c.value)),
                values: choices.iter().map(|c| FileValue { value: label(c.value), label: label(c.value) }).collect(),
                ..FileOption::default()
            }
        }).collect();
        let keep = [pause_options::keep_label(false), pause_options::keep_label(true)];
        out.push(FileOption {
            key: "picture.keep".into(), label: "Keep for".into(), current: keep[usize::from(self.for_game())].into(), default: keep[0].into(),
            values: keep.iter().map(|s| FileValue { value: (*s).into(), label: (*s).into() }).collect(), ..FileOption::default()
        });
        out
    }
    /// A row of the Mobile sheet one step on (`SHEET`'s rows, then 3 Keep for): Linux's next value of the row.
    pub fn change(&mut self, row: usize, delta: i32) -> Result<(), String> {
        if let Some(key) = SHEET.get(row) {
            let pairs = pause_options::setting_step(key, self.files.system(), None, &self.effective(), delta);
            return self.set(&pairs);
        }
        if row == SHEET.len() {
            // Keep for: the values in use move to the other side (`GameSettings::keep_elsewhere`).
            let ok = if self.for_game() { self.files.save_for_console(&self.own) } else { self.files.save_for_game(&self.own) };
            if !ok {
                return Err(pocket_ui::save_trouble::SETTING_FAILED.into());
            }
            self.level = if self.for_game() { pocket_ui::game_settings::Level::Console } else { pocket_ui::game_settings::Level::Game };
        }
        Ok(())
    }
    /// The frame these settings ask for, as pocket-core picks it (`VideoSettings::fixed_style`, Auto the console's own by
    /// `bezel::style_for`): with its finish (`frame_pocket`, `frame_clamshell`, `frame_brick`) and the TV's cabinet
    /// (`frame_tv`). Ambient (drawn live from the picture on Linux) is none here. The same pictures Linux draws, by the same code.
    pub fn frame_style(&self, system: &str) -> Option<Style> {
        use pocket_ui::video::FrameMode;
        match self.video.frame {
            FrameMode::None | FrameMode::Ambient => None,
            FrameMode::Auto => Some(bezel::style_for(system)),
            _ => self.video.fixed_style(system),
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
