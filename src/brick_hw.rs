//! The TrimUI Brick's own hardware, written to by us (docs/TRIMUI-BRICK-HW.md): the rumble motor, the 23 RGB LEDs and the
//! two extra keys. Everything is plain sysfs below a root (`/`, or a fake tree in the tests) and read from the Brick and
//! its firmware's kernel interface, none of NextUI's code.
//!
//! | what | where | read how |
//! |---|---|---|
//! | rumble | `gpio227` (output, 1 = motor on) and `/sys/class/motor/voltage` | the pin is set up by the start script; the voltage file exists on the Brick. **Its range is not known yet** (`MOTOR_VOLTAGE`): until it is, strength is the length of a pulse, and sustained game rumble is on or off |
//! | LEDs | `/sys/class/led_anim`: `max_scale` (0..60), `effect_enable`, and per zone `l r lr m f1 f2`: `effect_<z>` (0 off, 1 linear, 2 breath, 3 sniff, 4 static), `effect_rgb_hex_<z>` (`rrggbb`, each channel 0..60), `effect_cycles_<z>` (-1 = for ever) | the Brick's directory listing (03.11.), the values from Knulli's `knulli-rgb-led` for the same driver on the Smart Pro |
//! | F1, F2, Menu | `TRIMUI Player1` keys 317, 318, 316 | the Brick with Alex (03.11.): `pad.rs` maps 317/318 to `Button::F1/F2` on this pad only |
//! | mute slider | `gpio243/value` | `brick-hw slider-watch` (C, `scripts/brick/brick-hw.c`): which value is "muted" is **not known yet** |

use std::path::{Path, PathBuf};

use crate::battery::Sample;

/// Where the real sysfs is; the tests use a temporary tree.
pub fn system_root() -> PathBuf {
    PathBuf::from(std::env::var_os("POCKETUI_BRICK_SYS").unwrap_or_else(|| "/".into()))
}

fn put(root: &Path, rel: &str, value: &str) -> bool {
    std::fs::write(root.join(rel), format!("{value}\n")).is_ok()
}

fn has(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}

// ---- rumble motor -------------------------------------------------------------------------------------

const GPIO_EXPORT: &str = "sys/class/gpio/export";
const MOTOR_PIN: &str = "sys/class/gpio/gpio227/value";
const MOTOR_DIR: &str = "sys/class/gpio/gpio227/direction";
const MOTOR_VOLTAGE_FILE: &str = "sys/class/motor/voltage";

/// What `/sys/class/motor/voltage` takes, (weakest, strongest). `None` until the range is read on the Brick
/// (`brick-hw motor-info`); `POCKETUI_MOTOR_VOLTAGE=min,max` tries one without a rebuild.
pub fn motor_voltage_range() -> Option<(u32, u32)> {
    let v = std::env::var("POCKETUI_MOTOR_VOLTAGE").ok()?;
    let (a, b) = v.split_once(',')?;
    let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a < b).then_some((a, b))
}

/// The strongest request a core makes counts as "on" from here: below it the motor stays off (a tremor the motor cannot play).
const MOTOR_FLOOR: u16 = 6000;

/// The Brick's rumble motor.
pub struct Motor {
    root: PathBuf,
    range: Option<(u32, u32)>,
    on: bool,
}

impl Motor {
    /// The motor below `root`, if the Brick's pin can be reached (set up here when the start script has not).
    pub fn open(root: &Path, range: Option<(u32, u32)>) -> Option<Motor> {
        if !has(root, MOTOR_PIN) {
            if !has(root, GPIO_EXPORT) {
                return None;
            }
            put(root, GPIO_EXPORT, "227");
        }
        if has(root, MOTOR_DIR) {
            put(root, MOTOR_DIR, "out");
        }
        has(root, MOTOR_PIN).then(|| Motor { root: root.to_path_buf(), range, on: false })
    }

    /// The real one, or None on a computer.
    pub fn system() -> Option<Motor> {
        Self::open(&system_root(), motor_voltage_range())
    }

    /// On at `magnitude` (0..65535 as a core asks, already scaled by the strength setting); 0 is off.
    pub fn on(&mut self, magnitude: u16) -> bool {
        if magnitude < MOTOR_FLOOR {
            self.off();
            return true;
        }
        if let (Some((lo, hi)), true) = (self.range, has(&self.root, MOTOR_VOLTAGE_FILE)) {
            let v = lo as u64 + (hi - lo) as u64 * magnitude as u64 / 65535;
            put(&self.root, MOTOR_VOLTAGE_FILE, &v.to_string());
        }
        self.on = put(&self.root, MOTOR_PIN, "1");
        self.on
    }

    pub fn off(&mut self) {
        // Written even when we think it is off: a motor left on by a crashed game is worth a write.
        put(&self.root, MOTOR_PIN, "0");
        self.on = false;
    }
}

impl Drop for Motor {
    fn drop(&mut self) {
        self.off();
    }
}

/// How long a menu tick or a pulse lasts at a strength (`rumble::Strength` as 1..=3 = low, medium, high). Without a known
/// voltage range the length is the strength.
pub fn pulse_ms(strength_step: u8) -> u16 {
    match strength_step {
        0 => 0,
        1 => 12,
        2 => 22,
        _ => 38,
    }
}

// ---- LEDs ---------------------------------------------------------------------------------------------

const LED_DIR: &str = "sys/class/led_anim";
/// The zones that carry our colour: both grips (`lr`), the middle (`m`), the two extra keys. `l` and `r` are halves of `lr`.
const ZONES: [&str; 4] = ["lr", "m", "f1", "f2"];
/// The five lights of Settings > Lights & buttons > Each light, in the order of `Lights::zones`: the driver's `l` and `r` zones
/// (the grips) and `m`, `f1`, `f2`.
pub const LIGHT_ZONES: [&str; 5] = ["l", "r", "m", "f1", "f2"];
pub const LIGHT_NAMES: [&str; 5] = ["Left grip", "Right grip", "Middle", "F1 key", "F2 key"];
/// The driver's scale: `max_scale` and each colour channel go 0..60 (Knulli's script, same driver).
const LED_MAX: u32 = 60;

/// The driver's raw picture, one "RRGGBB " per LED (23 on the Brick): it stays lit past the effects. On a dark Brick (03.11., update 5)
/// every effect, `effect_enable`, `max_scale` and each `sunxi_led*` brightness read 0 and `frame_hex` still held "373737" for several
/// LEDs, which glowed until it was written black. Off writes it black, and the raw frame animation off.
const LED_COUNT: usize = 23;

/// `frame_hex` all black.
pub fn black_frame() -> String {
    "000000 ".repeat(LED_COUNT)
}

/// A brightness step of the page (25/50/75/100 %) as the share of the driver's full scale the lights get: 50/67/83/100 %. On the real
/// Brick (03.11., with the owner) the firmware's own `max_scale` 14 was too weak to notice and 50 of 60 clearly visible, so the lowest
/// step stays well above 14 (scale 30) and the page's default (50 %) gives 40.
fn visible(pct: u8) -> u32 {
    if pct == 0 { 0 } else { (34 + pct.min(100) as u32 * 66 / 100).min(100) }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    Off,
    Static((u8, u8, u8)),
    /// Slow breathing, for charging and a low battery.
    Breath((u8, u8, u8)),
}

/// What the lights do: an effect at a brightness step (percent 0..=100).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Look {
    pub effect: Effect,
    pub percent: u8,
    /// The player's per-light choices (`Lights`, Settings > Lights & buttons > Each light) apply to this look: the colour looks do,
    /// the battery look does not.
    pub custom: bool,
}

impl Look {
    pub const OFF: Look = Look { effect: Effect::Off, percent: 0, custom: false };

    pub fn of(effect: Effect, percent: u8) -> Look {
        Look { effect, percent, custom: false }
    }

    /// The same look, with the player's per-light choices on top.
    pub fn with_lights(self) -> Look {
        Look { custom: true, ..self }
    }
}

/// The driver is there, on a board whose zones are these (the Brick; the Smart Pro's rings are the same driver, unmapped:
/// `brick_profile::Board::leds`, so nothing of ours lights them).
pub fn leds_present(root: &Path) -> bool {
    has(root, &format!("{LED_DIR}/effect_enable")) && has(root, &format!("{LED_DIR}/effect_lr")) && crate::brick_profile::board().is_none_or(|b| b.leds())
}

fn hex(c: (u8, u8, u8)) -> String {
    let ch = |v: u8| (v as u32 * LED_MAX + 127) / 255;
    format!("{:02x}{:02x}{:02x}", ch(c.0), ch(c.1), ch(c.2))
}

/// The writes a look needs, in order: effect off, colours, cycles, the effect, the scale, then enable.
pub fn led_writes(look: Look) -> Vec<(String, String)> {
    led_writes_with(look, if look.custom { lights() } else { None })
}

/// `led_writes` with the lights' choices given (None: the page's look alone).
pub fn led_writes_with(look: Look, custom: Option<Lights>) -> Vec<(String, String)> {
    let p = |name: &str| format!("{LED_DIR}/{name}");
    let mut w = vec![(p("effect_enable"), "0".to_string())];
    let (mode, colour) = match look.effect {
        Effect::Off => (0, (0, 0, 0)),
        Effect::Static(c) => (4, c),
        Effect::Breath(c) => (2, c),
    };
    let custom = custom.filter(|l| mode != 0 && *l != Lights::default());
    PAGE_PERCENT.store(look.percent, std::sync::atomic::Ordering::SeqCst);
    // With the player's choices every light has its own brightness: the colours carry it and the driver's scale stays at full.
    let scale = if mode == 0 {
        0
    } else if custom.is_some() {
        LED_MAX
    } else {
        (visible(look.percent) * LED_MAX + 50) / 100
    };
    let dim = |c: (u8, u8, u8), pct: u8| {
        let k = |v: u8| (v as u32 * visible(pct) / 100) as u8;
        (k(c.0), k(c.1), k(c.2))
    };
    let zone = |w: &mut Vec<(String, String)>, z: &str, mode: u8, rgb: (u8, u8, u8), ms: u32| {
        w.push((p(&format!("effect_rgb_hex_{z}")), hex(rgb)));
        w.push((p(&format!("effect_duration_{z}")), ms.to_string()));
        w.push((p(&format!("effect_cycles_{z}")), "-1".to_string()));
        w.push((p(&format!("effect_{z}")), mode.to_string()));
    };
    let normal = Speed::Normal.millis();
    for z in ZONES {
        match (&custom, LIGHT_ZONES.iter().position(|n| *n == z)) {
            (Some(l), Some(i)) => {
                let (m, c) = l.zones[i].resolve(mode, colour, look.percent);
                zone(&mut w, z, m, c, l.zones[i].speed.millis())
            }
            (Some(_), None) => zone(&mut w, z, mode, dim(colour, look.percent), normal),
            (None, _) => zone(&mut w, z, mode, colour, normal),
        }
    }
    // The grips are the driver's `l` and `r` zones, written after `lr` so a choice for one grip wins over the pair.
    if let Some(l) = &custom {
        for (i, z) in LIGHT_ZONES.iter().enumerate().take(2) {
            if l.zones[i] != LightPref::default() {
                let (m, c) = l.zones[i].resolve(mode, colour, look.percent);
                zone(&mut w, z, m, c, l.zones[i].speed.millis());
            }
        }
    }
    w.push((p("max_scale"), scale.to_string()));
    // Off is off: the driver's own effects stop too (the firmware's boot breathing, white with green F keys, never came back down).
    w.push((p("effect_enable"), if mode == 0 { "0" } else { "1" }.to_string()));
    // And its raw picture: what stays in `frame_hex` glows on past the effects.
    if mode == 0 {
        w.push((p("anim_frames_enable"), "0".to_string()));
        w.push((p("frame_hex"), black_frame()));
    }
    w
}

/// Puts `look` on the lights below `root`. A file the Brick does not have (a zone of another model) is skipped; false when
/// the driver is not there at all.
pub fn apply_look(root: &Path, look: Look) -> bool {
    if !leds_present(root) {
        return false;
    }
    for (rel, value) in led_writes(look) {
        if has(root, &rel) {
            put(root, &rel, &value);
        }
    }
    true
}

/// At or under this charge, without a charger, the lights breathe red.
pub const LOW_PERCENT: u8 = 15;

/// Settings > LEDs > Battery: dark while all is well; breathing orange while it charges, green once it is full, breathing red
/// when it runs low. `percent` is the brightness step of the page.
pub fn battery_look(s: Option<&Sample>, percent: u8) -> Look {
    let Some(s) = s else { return Look::of(Effect::Off, percent) };
    let effect = if s.full {
        Effect::Static((60, 220, 90))
    } else if s.charging {
        Effect::Breath((255, 140, 30))
    } else if s.capacity <= LOW_PERCENT {
        Effect::Breath((255, 30, 20))
    } else {
        Effect::Off
    };
    Look::of(effect, percent)
}

/// A cover's main colour as a calm light: the colour kept, its darkest and lightest parts lifted into what LEDs show
/// well (a black cover would be no light; a white one a harsh one).
pub fn calm(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    let (r, g, b) = (rgb.0 as f32, rgb.1 as f32, rgb.2 as f32);
    let hi = r.max(g).max(b);
    if hi < 1.0 {
        return (60, 60, 70);
    }
    // Strongest channel to 255 (the page's brightness step sets the real level), then the colour pulled a quarter toward white
    // so a grey cover is a soft white, not a dull glow.
    let k = 255.0 / hi;
    let lift = |v: f32| (v * k * 0.8 + 255.0 * 0.2).min(255.0) as u8;
    (lift(r), lift(g), lift(b))
}

// ---- the F keys ---------------------------------------------------------------------------------------

/// Shown in game Controls and when Controller setup receives a reserved F-key press.
pub const FKEYS_REMAP_NOTE: &str = "F1/F2 are shortcuts, not game buttons.\nSet them in Settings › Sound & feel › Lights & buttons.";

/// What F1 and F2 do (Settings > Buttons; the choice is `<data>/brick-keys.json`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FKey {
    Nothing,
    #[default]
    Save,
    Load,
    Screenshot,
    Recent,
    /// The pause menu of the running game (nothing on the shelf).
    Pause,
}

impl FKey {
    pub const ALL: [FKey; 6] = [FKey::Save, FKey::Load, FKey::Screenshot, FKey::Recent, FKey::Pause, FKey::Nothing];

    pub fn label(self) -> &'static str {
        match self {
            FKey::Nothing => "Nothing",
            FKey::Save => "Quick save",
            FKey::Load => "Quick load",
            FKey::Screenshot => "Screenshot",
            FKey::Recent => "Recent games",
            FKey::Pause => "Pause menu",
        }
    }

    pub fn next(self, delta: i32) -> FKey {
        let at = Self::ALL.iter().position(|k| *k == self).unwrap_or(0) as i32;
        Self::ALL[(at + delta.signum()).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

/// The two keys' choices: F1 quick save, F2 quick load unless the player picked otherwise.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct FKeys {
    #[serde(default, deserialize_with = "lenient_fkey")]
    pub f1: FKey,
    #[serde(default = "load_default", deserialize_with = "lenient_load")]
    pub f2: FKey,
}

fn load_default() -> FKey {
    FKey::Load
}

fn lenient_fkey<'de, D: serde::Deserializer<'de>>(d: D) -> Result<FKey, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

fn lenient_load<'de, D: serde::Deserializer<'de>>(d: D) -> Result<FKey, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or(FKey::Load))
}

impl Default for FKeys {
    fn default() -> Self {
        FKeys { f1: FKey::Save, f2: FKey::Load }
    }
}

const FKEYS_FILE: &str = "brick-keys.json";

impl FKeys {
    pub fn load(data_dir: &Path) -> FKeys {
        crate::fsutil::read_json_or_backup(&data_dir.join(FKEYS_FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&data_dir.join(FKEYS_FILE), self) {
            eprintln!("brick-hw: can't save {FKEYS_FILE}: {e}");
        }
    }

    pub fn of(&self, n: u8) -> FKey {
        if n == 1 { self.f1 } else { self.f2 }
    }
}

// ---- the mute slider ----------------------------------------------------------------------------------

/// What the slider on the side does (Settings > Lights & buttons > Mute slider; `<data>/brick-slider`, read by `brick-hw slider-watch`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Slider {
    #[default]
    Mute,
    Nothing,
}

const SLIDER_FILE: &str = "brick-slider";

impl Slider {
    pub fn label(self) -> &'static str {
        match self {
            Slider::Mute => "Mutes",
            Slider::Nothing => "Does nothing",
        }
    }

    pub fn other(self) -> Slider {
        match self {
            Slider::Mute => Slider::Nothing,
            Slider::Nothing => Slider::Mute,
        }
    }

    pub fn load(data_dir: &Path) -> Slider {
        match std::fs::read_to_string(data_dir.join(SLIDER_FILE)) {
            Ok(t) if t.trim() == "nothing" => Slider::Nothing,
            _ => Slider::Mute,
        }
    }

    pub fn save(self, data_dir: &Path) {
        let text = if self == Slider::Nothing { "nothing\n" } else { "mute\n" };
        if let Err(e) = std::fs::write(data_dir.join(SLIDER_FILE), text) {
            eprintln!("brick-hw: can't save {SLIDER_FILE}: {e}");
        }
    }
}

// ---- each light ---------------------------------------------------------------------------------------

/// What one light does on top of the page's colour look. `Same` follows the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Glow {
    #[default]
    Same,
    Off,
    Steady,
    Breathe,
    /// The driver's other effects (`effect_<zone>` 1 and 3).
    Linear,
    Sniff,
    /// The colour goes round the colour wheel (pocket-ui turns it, `rainbow`), at the light's speed.
    Rainbow,
    /// This light shows the battery and the charger as "Battery & charging" does (the others keep their colours).
    Battery,
}

impl Glow {
    pub const ALL: [Glow; 8] = [Glow::Same, Glow::Steady, Glow::Breathe, Glow::Linear, Glow::Sniff, Glow::Rainbow, Glow::Battery, Glow::Off];

    pub fn label(self) -> &'static str {
        match self {
            Glow::Same => "Like the rest",
            Glow::Off => "Off",
            Glow::Steady => "Steady",
            Glow::Breathe => "Breathing",
            Glow::Linear => "Fading",
            Glow::Sniff => "Pulsing",
            Glow::Rainbow => "Colour cycle",
            Glow::Battery => "Battery & charging",
        }
    }

    /// The effect moves, so its speed means something.
    pub fn animated(self) -> bool {
        matches!(self, Glow::Breathe | Glow::Linear | Glow::Sniff | Glow::Rainbow)
    }

    /// The driver's effect number; None follows the page.
    fn mode(self) -> Option<u8> {
        match self {
            Glow::Same => None,
            Glow::Off => Some(0),
            Glow::Linear => Some(1),
            Glow::Breathe => Some(2),
            Glow::Sniff => Some(3),
            Glow::Steady | Glow::Rainbow => Some(4),
            Glow::Battery => None,
        }
    }
}

/// How fast a light's effect goes: the driver's `effect_duration_<zone>` (milliseconds a breath or a fade takes), and the colour cycle's
/// pace.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    Slow,
    #[default]
    Normal,
    Fast,
}

impl Speed {
    pub const ALL: [Speed; 3] = [Speed::Slow, Speed::Normal, Speed::Fast];

    pub fn label(self) -> &'static str {
        match self {
            Speed::Slow => "Slow",
            Speed::Normal => "Normal",
            Speed::Fast => "Fast",
        }
    }

    /// The driver's duration: 1000 ms is what the firmware uses (03.11. on the real Brick).
    pub fn millis(self) -> u32 {
        match self {
            Speed::Slow => 2000,
            Speed::Normal => 1000,
            Speed::Fast => 500,
        }
    }

    /// Seconds the colour cycle takes once round.
    pub fn cycle_secs(self) -> f32 {
        match self {
            Speed::Slow => 16.0,
            Speed::Normal => 8.0,
            Speed::Fast => 3.0,
        }
    }
}

/// A colour of the player's own: a hue (0..360 degrees) and a saturation (percent), at full value (the brightness is its own row).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Own {
    pub hue: u16,
    pub sat: u8,
}

impl Default for Own {
    fn default() -> Self {
        Own { hue: 200, sat: 100 }
    }
}

/// The steps of the saturation row.
pub const SATURATIONS: [u8; 4] = [25, 50, 75, 100];
/// One step of the hue row, in degrees.
pub const HUE_STEP: u16 = 15;

impl Own {
    pub fn rgb(self) -> (u8, u8, u8) {
        hsv(self.hue as f32, self.sat.min(100) as f32 / 100.0)
    }
}

/// A colour at full value from a hue in degrees and a saturation 0..=1.
pub fn hsv(hue: f32, sat: f32) -> (u8, u8, u8) {
    let h = hue.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let ch = |v: f32| ((1.0 - sat + sat * v) * 255.0).round() as u8;
    (ch(r), ch(g), ch(b))
}

/// One light's choices; the default follows the page in everything.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct LightPref {
    /// None: the page's colour.
    #[serde(default, deserialize_with = "lenient_opt_hue")]
    pub hue: Option<crate::leds::Hue>,
    #[serde(default, deserialize_with = "lenient_glow")]
    pub glow: Glow,
    /// Index into `leds::LEVELS`; None: the page's brightness.
    #[serde(default)]
    pub level: Option<u8>,
    /// The light's own colour (hue and saturation) is used instead of `hue`; `own` is kept when another colour is picked.
    #[serde(default)]
    pub use_own: bool,
    #[serde(default)]
    pub own: Own,
    #[serde(default)]
    pub speed: Speed,
}

fn lenient_opt_hue<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<crate::leds::Hue>, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).ok().flatten())
}

fn lenient_glow<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Glow, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

impl LightPref {
    /// The driver effect and colour (already dimmed to this light's brightness) given the page's look.
    fn resolve(&self, page_mode: u8, page_colour: (u8, u8, u8), page_percent: u8) -> (u8, (u8, u8, u8)) {
        let pct = self.percent(page_percent);
        let k = |v: u8| (v as u32 * visible(pct) / 100) as u8;
        // Battery & charging on this light alone: the battery look of the moment (dark while all is well).
        if self.glow == Glow::Battery {
            let (mode, colour) = match battery_now().map(|l| l.effect) {
                Some(Effect::Static(c)) => (4, c),
                Some(Effect::Breath(c)) => (2, c),
                _ => (0, (0, 0, 0)),
            };
            return (mode, (k(colour.0), k(colour.1), k(colour.2)));
        }
        let colour = if self.glow == Glow::Rainbow { hsv(rainbow_hue(self.speed), 1.0) } else { self.colour(page_colour) };
        let mode = self.glow.mode().unwrap_or(page_mode);
        (mode, if mode == 0 { (0, 0, 0) } else { (k(colour.0), k(colour.1), k(colour.2)) })
    }

    /// The light's colour before brightness: its own, a preset, or the page's.
    pub fn colour(&self, page_colour: (u8, u8, u8)) -> (u8, u8, u8) {
        if self.use_own {
            self.own.rgb()
        } else {
            self.hue.map_or(page_colour, |h| calm(h.rgb()))
        }
    }

    fn percent(&self, page_percent: u8) -> u8 {
        self.level.map_or(page_percent, |n| crate::leds::LEVELS[(n as usize).min(crate::leds::LEVELS.len() - 1)])
    }

    /// "Like the rest", the colours, then "Own colour" (hue and saturation of its own).
    pub fn step_colour(&mut self, delta: i32) {
        let presets: Vec<crate::leds::Hue> = crate::leds::Hue::ALL.iter().copied().filter(|h| *h != crate::leds::Hue::Game).collect();
        // positions: 0 like the rest, 1..=n the presets, n+1 own
        let n = presets.len() as i32;
        let at = if self.use_own { n + 1 } else { self.hue.and_then(|h| presets.iter().position(|p| *p == h)).map_or(0, |i| i as i32 + 1) };
        let next = (at + delta.signum()).rem_euclid(n + 2);
        self.use_own = next == n + 1;
        self.hue = if next >= 1 && next <= n { Some(presets[(next - 1) as usize]) } else { None };
    }

    pub fn step_own_hue(&mut self, delta: i32) {
        self.own.hue = ((self.own.hue as i32 + delta.signum() * HUE_STEP as i32).rem_euclid(360)) as u16;
    }

    pub fn step_own_sat(&mut self, delta: i32) {
        let at = SATURATIONS.iter().position(|s| *s >= self.own.sat).unwrap_or(SATURATIONS.len() - 1) as i32;
        self.own.sat = SATURATIONS[(at + delta.signum()).clamp(0, SATURATIONS.len() as i32 - 1) as usize];
    }

    pub fn step_speed(&mut self, delta: i32) {
        let at = Speed::ALL.iter().position(|s| *s == self.speed).unwrap_or(1) as i32;
        self.speed = Speed::ALL[(at + delta.signum()).clamp(0, Speed::ALL.len() as i32 - 1) as usize];
    }

    /// "Like the rest" first, then the colours (the game's own is the page's choice, not a light's), then the light's own.
    pub fn step_hue(&mut self, delta: i32) {
        self.step_colour(delta);
    }

    pub fn step_glow(&mut self, delta: i32) {
        let at = Glow::ALL.iter().position(|g| *g == self.glow).unwrap_or(0) as i32;
        self.glow = Glow::ALL[(at + delta.signum()).rem_euclid(Glow::ALL.len() as i32) as usize];
    }

    /// None (like the page), then the page's steps; stops at the ends.
    pub fn step_level(&mut self, delta: i32) {
        let n = crate::leds::LEVELS.len() as i32;
        let at = self.level.map_or(-1, |l| l as i32);
        let next = (at + delta.signum()).clamp(-1, n - 1);
        self.level = (next >= 0).then_some(next as u8);
    }
}

/// The five lights' choices (`<data>/brick-lights.json`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Lights {
    #[serde(default)]
    pub zones: [LightPref; 5],
}

const LIGHTS_FILE: &str = "brick-lights.json";

static LIGHTS: std::sync::Mutex<Option<Lights>> = std::sync::Mutex::new(None);

// ---- per-light battery and the colour cycle ------------------------------------------------------------

/// The battery look of the moment for the lights set to Battery & charging (`Glow::Battery`); the App sets it from its battery samples.
static BATTERY_NOW: std::sync::Mutex<Option<Look>> = std::sync::Mutex::new(None);

pub fn set_battery_now(look: Option<Look>) {
    if let Ok(mut g) = BATTERY_NOW.lock() {
        *g = look;
    }
}

fn battery_now() -> Option<Look> {
    BATTERY_NOW.lock().ok().and_then(|g| *g)
}

/// Where the colour cycle is on the wheel (degrees) for a light at `speed`: from one clock, so lights at the same speed go together.
fn rainbow_hue(speed: Speed) -> f32 {
    let t = RAINBOW_EPOCH.get_or_init(std::time::Instant::now).elapsed().as_secs_f32();
    (t / speed.cycle_secs() * 360.0).rem_euclid(360.0)
}

static RAINBOW_EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
static RAINBOW_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAINBOW_THREAD: std::sync::Once = std::sync::Once::new();

/// Turns the colour cycle on or off (the App: a light on Colour cycle, the colour look on, the screen awake). While on, a thread writes
/// the cycling lights' colours ten times a second (a few bytes into sysfs each); off, it sleeps and writes nothing.
pub fn set_rainbow(on: bool, root: &Path) {
    RAINBOW_ON.store(on, std::sync::atomic::Ordering::SeqCst);
    if !on {
        return;
    }
    let root = root.to_path_buf();
    RAINBOW_THREAD.call_once(move || {
        let _ = std::thread::Builder::new().name("brick-rainbow".into()).spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !RAINBOW_ON.load(std::sync::atomic::Ordering::SeqCst) {
                continue;
            }
            for (rel, value) in rainbow_writes(lights().unwrap_or_default()) {
                // Asked again before each write: a dark screen's Off must not get a colour written after it.
                if RAINBOW_ON.load(std::sync::atomic::Ordering::SeqCst) && has(&root, &rel) {
                    put(&root, &rel, &value);
                }
            }
        });
    });
}

/// The page's brightness of the last colour look written, for the lights on Colour cycle that follow it.
static PAGE_PERCENT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(50);

/// The colour writes of the lights on Colour cycle now (their own brightness, or the page's).
fn rainbow_writes(l: Lights) -> Vec<(String, String)> {
    let page = PAGE_PERCENT.load(std::sync::atomic::Ordering::SeqCst);
    LIGHT_ZONES
        .iter()
        .zip(l.zones.iter())
        .filter(|(_, z)| z.glow == Glow::Rainbow)
        .map(|(name, z)| {
            let (_, c) = z.resolve(4, (0, 0, 0), page);
            (format!("{LED_DIR}/effect_rgb_hex_{name}"), hex(c))
        })
        .collect()
}

impl Lights {
    /// A light is on Colour cycle.
    pub fn cycling(&self) -> bool {
        self.zones.iter().any(|z| z.glow == Glow::Rainbow)
    }

    /// A light shows the battery on its own.
    pub fn shows_battery(&self) -> bool {
        self.zones.iter().any(|z| z.glow == Glow::Battery)
    }

    /// The same lights with all but the battery lights off (the screen is dark).
    pub fn battery_only(&self) -> Lights {
        let mut l = *self;
        for z in l.zones.iter_mut().filter(|z| z.glow != Glow::Battery) {
            z.glow = Glow::Off;
        }
        l
    }
}

/// The choices the colour looks use; set from the App when it loads and whenever the player changes one (the write itself runs on a
/// thread of its own, which reads them here).
pub fn set_lights(l: Lights) {
    if let Ok(mut g) = LIGHTS.lock() {
        *g = Some(l);
    }
}

fn lights() -> Option<Lights> {
    LIGHTS.lock().ok().and_then(|g| *g).filter(|l| *l != Lights::default())
}

impl Lights {
    pub fn load(data_dir: &Path) -> Lights {
        crate::fsutil::read_json_or_backup(&data_dir.join(LIGHTS_FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&data_dir.join(LIGHTS_FILE), self) {
            eprintln!("brick-hw: can't save {LIGHTS_FILE}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// The sysfs files of a Brick with everything in it.
    fn brick_tree(name: &str) -> TestDir {
        let d = TestDir::new(name);
        let mk = |rel: &str, v: &str| {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, v).unwrap();
        };
        mk("sys/class/gpio/export", "");
        mk("sys/class/gpio/gpio227/value", "0\n");
        mk("sys/class/gpio/gpio227/direction", "out\n");
        mk("sys/class/motor/voltage", "0\n");
        mk("sys/class/led_anim/effect_enable", "0\n");
        mk("sys/class/led_anim/max_scale", "60\n");
        mk("sys/class/led_anim/frame_hex", "000000 ");
        mk("sys/class/led_anim/anim_frames_enable", "0\n");
        for z in ["l", "r", "lr", "m", "f1", "f2"] {
            mk(&format!("sys/class/led_anim/effect_{z}"), "0\n");
            mk(&format!("sys/class/led_anim/effect_rgb_hex_{z}"), "000000\n");
            mk(&format!("sys/class/led_anim/effect_cycles_{z}"), "0\n");
        }
        d
    }

    fn read(d: &TestDir, rel: &str) -> String {
        std::fs::read_to_string(d.join(rel)).unwrap().trim().to_string()
    }

    fn sample(capacity: u8, charging: bool, full: bool) -> Sample {
        Sample { t: 0, capacity, charging, full, mv: 3900, ma: 0, temp: None, tag: 0 }
    }

    #[test]
    fn the_motor_goes_on_and_off_through_its_pin() {
        let d = brick_tree("brick-motor");
        let mut m = Motor::open(&d, None).unwrap();
        assert!(m.on(40000));
        assert_eq!(read(&d, MOTOR_PIN), "1");
        assert_eq!(read(&d, MOTOR_VOLTAGE_FILE), "0", "no known range: the voltage is left alone");
        m.off();
        assert_eq!(read(&d, MOTOR_PIN), "0");
        assert!(m.on(1000) && read(&d, MOTOR_PIN) == "0", "a tremor under the floor stays off");
        drop(m);
        assert_eq!(read(&d, MOTOR_PIN), "0");
    }

    #[test]
    fn a_known_voltage_range_gives_strength() {
        let d = brick_tree("brick-motor-range");
        let mut m = Motor::open(&d, Some((1000, 3000))).unwrap();
        m.on(65535);
        assert_eq!(read(&d, MOTOR_VOLTAGE_FILE), "3000");
        m.on(32768);
        let mid: u32 = read(&d, MOTOR_VOLTAGE_FILE).parse().unwrap();
        assert!((1990..=2010).contains(&mid), "{mid}");
    }

    #[test]
    fn the_motor_drops_out_without_a_pin_and_drop_switches_it_off() {
        let none = TestDir::new("brick-motor-none");
        assert!(Motor::open(&none, None).is_none(), "a computer has no motor");
        let d = brick_tree("brick-motor-drop");
        {
            let mut m = Motor::open(&d, None).unwrap();
            m.on(65535);
        }
        assert_eq!(read(&d, MOTOR_PIN), "0", "a motor never stays on after its owner is gone");
        // The pin is exported when the start script has not.
        std::fs::remove_dir_all(d.join("sys/class/gpio/gpio227")).unwrap();
        assert!(Motor::open(&d, None).is_none(), "no pin appears in a fake tree: nothing to drive");
        assert_eq!(read(&d, GPIO_EXPORT), "227");
    }

    #[test]
    fn pulses_grow_with_the_strength_step() {
        assert_eq!(pulse_ms(0), 0);
        assert!(pulse_ms(1) < pulse_ms(2) && pulse_ms(2) < pulse_ms(3));
    }

    #[test]
    fn a_static_colour_reaches_every_zone_on_the_drivers_scale() {
        let d = brick_tree("brick-led-static");
        assert!(apply_look(&d, Look::of(Effect::Static((255, 0, 0)), 50)));
        for z in ZONES {
            assert_eq!(read(&d, &format!("sys/class/led_anim/effect_rgb_hex_{z}")), "3c0000", "{z}");
            assert_eq!(read(&d, &format!("sys/class/led_anim/effect_{z}")), "4");
            assert_eq!(read(&d, &format!("sys/class/led_anim/effect_cycles_{z}")), "-1");
        }
        assert_eq!(read(&d, "sys/class/led_anim/max_scale"), "40", "the 50 % step: well above the firmware's 14, which is too weak to see");
        assert_eq!(read(&d, "sys/class/led_anim/effect_enable"), "1");
        // The half zones are left alone.
        assert_eq!(read(&d, "sys/class/led_anim/effect_l"), "0");
    }

    #[test]
    fn every_brightness_step_is_visible_on_the_real_brick() {
        // 03.11. on the device: max_scale 14 could not be seen, 50 clearly; the driver's limit is 60.
        let scales: Vec<u32> = crate::leds::LEVELS.iter().map(|p| (visible(*p) * LED_MAX + 50) / 100).collect();
        assert_eq!(scales, [30, 40, 50, 60]);
        assert_eq!(visible(0), 0, "off stays off");
    }

    #[test]
    fn off_is_dark_and_breath_is_breath() {
        let d = brick_tree("brick-led-off");
        apply_look(&d, Look::of(Effect::Breath((255, 140, 30)), 100));
        assert_eq!(read(&d, "sys/class/led_anim/effect_m"), "2");
        assert_eq!(read(&d, "sys/class/led_anim/max_scale"), "60");
        apply_look(&d, Look::OFF);
        assert_eq!(read(&d, "sys/class/led_anim/max_scale"), "0");
        assert_eq!(read(&d, "sys/class/led_anim/effect_f1"), "0");
        assert_eq!(read(&d, "sys/class/led_anim/effect_enable"), "0", "Off stops the driver's effects as well");
    }

    /// The dark Brick of 03.11. (update 5): effects, `effect_enable` and `max_scale` all 0, and `frame_hex` still "373737" for several
    /// LEDs, which glowed. Off writes the raw picture black too, and the raw frame animation off.
    #[test]
    fn off_also_blacks_out_a_raw_picture_left_in_the_driver() {
        let d = brick_tree("brick-led-frame");
        apply_look(&d, Look::of(Effect::Static((255, 255, 255)), 100));
        std::fs::write(d.join("sys/class/led_anim/frame_hex"), "373737 ".repeat(6) + &"000000 ".repeat(17)).unwrap();
        std::fs::write(d.join("sys/class/led_anim/anim_frames_enable"), "1\n").unwrap();
        apply_look(&d, Look::OFF);
        let frame = read(&d, "sys/class/led_anim/frame_hex");
        assert_eq!(frame.split_whitespace().count(), 23, "one value per LED: {frame}");
        assert!(frame.split_whitespace().all(|v| v == "000000"), "every LED black: {frame}");
        assert_eq!(read(&d, "sys/class/led_anim/anim_frames_enable"), "0");
        // A colour look leaves the raw picture alone (the effects draw over it).
        std::fs::write(d.join("sys/class/led_anim/frame_hex"), "x").unwrap();
        apply_look(&d, Look::of(Effect::Static((0, 255, 0)), 100));
        assert_eq!(read(&d, "sys/class/led_anim/frame_hex"), "x");
    }

    #[test]
    fn a_brick_without_the_second_zones_still_lights_and_a_computer_does_not() {
        let d = brick_tree("brick-led-missing");
        std::fs::remove_file(d.join("sys/class/led_anim/effect_f1")).unwrap();
        assert!(apply_look(&d, Look::of(Effect::Static((0, 255, 0)), 100)));
        assert!(!d.join("sys/class/led_anim/effect_f1").exists(), "a missing file is not created");
        assert_eq!(read(&d, "sys/class/led_anim/effect_lr"), "4");
        let none = TestDir::new("brick-led-none");
        assert!(!apply_look(&none, Look::of(Effect::Static((0, 255, 0)), 100)));
    }

    #[test]
    fn the_battery_look_shows_charging_full_and_low_and_otherwise_rests() {
        let look = |s: Sample| battery_look(Some(&s), 50).effect;
        assert!(matches!(look(sample(40, true, false)), Effect::Breath((255, 140, 30))), "charging");
        assert!(matches!(look(sample(100, false, true)), Effect::Static((60, 220, 90))), "full");
        assert!(matches!(look(sample(12, false, false)), Effect::Breath((255, 30, 20))), "low");
        assert_eq!(look(sample(15, false, false)), Effect::Breath((255, 30, 20)), "at the limit");
        assert_eq!(look(sample(16, false, false)), Effect::Off, "all well: dark");
        assert!(matches!(look(sample(5, true, false)), Effect::Breath((255, 140, 30))), "a charger beats low");
        assert_eq!(battery_look(None, 50).effect, Effect::Off, "no battery reading: dark, not a guess");
    }

    #[test]
    fn a_cover_colour_becomes_a_calm_light() {
        let (r, g, b) = calm((200, 20, 20));
        assert!(r > g + 60 && r > b + 60, "still red: {r},{g},{b}");
        assert_eq!(calm((0, 0, 0)), (60, 60, 70), "a black cover is a dim white, not nothing");
        let (r, g, b) = calm((128, 128, 128));
        assert!(r == g && g == b && r > 200, "grey lifts to soft white");
    }

    #[test]
    fn the_f_keys_default_to_save_and_load_and_are_remembered() {
        let d = TestDir::new("brick-fkeys");
        std::fs::create_dir_all(&*d).unwrap();
        assert_eq!(FKeys::load(&d), FKeys { f1: FKey::Save, f2: FKey::Load }, "nothing chosen yet");
        let k = FKeys { f1: FKey::Recent, f2: FKey::Screenshot };
        k.save(&d);
        assert_eq!(FKeys::load(&d), k);
        assert_eq!((k.of(1), k.of(2)), (FKey::Recent, FKey::Screenshot));
        std::fs::write(d.join(FKEYS_FILE), r#"{"f1": "something-a-later-version-added"}"#).unwrap();
        assert_eq!(FKeys::load(&d), FKeys { f1: FKey::Save, f2: FKey::Load }, "an unknown choice falls back, the file is not lost");
        std::fs::write(d.join(FKEYS_FILE), "junk").unwrap();
        assert_eq!(FKeys::load(&d), k, "a broken file brings the last good copy back");
        let e = TestDir::new("brick-fkeys-empty");
        std::fs::create_dir_all(&*e).unwrap();
        std::fs::write(e.join(FKEYS_FILE), "junk").unwrap();
        assert_eq!(FKeys::load(&e), FKeys::default(), "junk and no backup: the defaults");
    }

    #[test]
    fn the_f_key_choices_go_round_in_both_directions() {
        let mut k = FKey::Save;
        for _ in 0..FKey::ALL.len() {
            k = k.next(1);
        }
        assert_eq!(k, FKey::Save);
        assert_eq!(FKey::Save.next(-1), FKey::Nothing);
        assert_eq!(FKey::Recent.next(1), FKey::Pause);
        assert_eq!(FKey::Nothing.next(1), FKey::Save);
    }

    #[test]
    fn an_own_colour_a_speed_a_colour_cycle_and_the_battery_on_one_light() {
        let get = |w: &[(String, String)], f: &str| w.iter().rev().find(|(k, _)| k.ends_with(f)).map(|(_, v)| v.clone());
        // Own colour: hue and saturation, at full value.
        assert_eq!(hsv(0.0, 1.0), (255, 0, 0));
        assert_eq!(hsv(120.0, 1.0), (0, 255, 0));
        assert_eq!(hsv(240.0, 0.5), (128, 128, 255), "half saturation is pale");
        let mut z = LightPref::default();
        for _ in 0..7 {
            z.step_colour(1);
        }
        assert!(z.use_own && z.hue.is_none(), "after the six colours comes Own colour");
        z.step_colour(1);
        assert!(!z.use_own && z.hue.is_none(), "and round to Like the rest");
        z.use_own = true;
        z.own = Own { hue: 350, sat: 100 };
        z.step_own_hue(1);
        assert_eq!(z.own.hue, 5, "the hue goes round");
        z.step_own_sat(-1);
        assert_eq!(z.own.sat, 75);
        // Speed: the driver's duration for that light.
        z.glow = Glow::Breathe;
        z.step_speed(1);
        assert_eq!(z.speed, Speed::Fast);
        let mut l = Lights::default();
        l.zones[3] = z;
        let look = Look::of(Effect::Static((60, 120, 255)), 100).with_lights();
        let w = led_writes_with(look, Some(l));
        assert_eq!(get(&w, "effect_duration_f1").as_deref(), Some("500"));
        assert_eq!(get(&w, "effect_duration_m").as_deref(), Some("1000"), "the others keep the firmware's 1000 ms");
        assert_eq!(get(&w, "effect_rgb_hex_f1"), Some(hex(hsv(5.0, 0.75))));
        // Colour cycle: static, and the cycle's writes go to that light only.
        let mut l = Lights::default();
        l.zones[4].glow = Glow::Rainbow;
        assert!(l.cycling());
        let w = led_writes_with(look, Some(l));
        assert_eq!(get(&w, "effect_f2").as_deref(), Some("4"));
        let r = rainbow_writes(l);
        assert_eq!(r.len(), 1);
        assert!(r[0].0.ends_with("effect_rgb_hex_f2"));
        // Battery on the F keys, the rest in colour; dark while all is well, orange breathing while it charges.
        let mut l = Lights::default();
        l.zones[3].glow = Glow::Battery;
        l.zones[4].glow = Glow::Battery;
        set_battery_now(Some(Look::of(Effect::Off, 50)));
        let w = led_writes_with(look, Some(l));
        assert_eq!((get(&w, "effect_f1").as_deref(), get(&w, "effect_m").as_deref()), (Some("0"), Some("4")));
        set_battery_now(Some(Look::of(Effect::Breath((255, 140, 30)), 50)));
        let w = led_writes_with(look, Some(l));
        assert_eq!(get(&w, "effect_f2").as_deref(), Some("2"), "charging: the F keys breathe");
        let dark = l.battery_only();
        assert_eq!(dark.zones[0].glow, Glow::Off);
        assert_eq!(dark.zones[3].glow, Glow::Battery, "a dark screen keeps the battery lights");
        set_battery_now(None);
        // A file of the version before reads with the defaults.
        let old: Lights = serde_json::from_str(r#"{"zones":[{"hue":"rose","glow":"breathe","level":0},{},{},{},{}]}"#).unwrap();
        assert_eq!((old.zones[0].speed, old.zones[0].use_own), (Speed::Normal, false));
    }

    #[test]
    fn each_light_can_have_its_own_colour_effect_and_brightness() {
        use crate::leds::Hue;
        let get = |w: &[(String, String)], f: &str| w.iter().rev().find(|(k, _)| k.ends_with(f)).map(|(_, v)| v.clone());
        let mut l = Lights::default();
        l.zones[3] = LightPref { hue: Some(Hue::Rose), glow: Glow::Breathe, level: Some(0), ..Default::default() };
        l.zones[4].glow = Glow::Off;
        l.zones[0].hue = Some(Hue::Green);
        let look = Look::of(Effect::Static((60, 120, 255)), 100).with_lights();
        let w = led_writes_with(look, Some(l));
        assert_eq!(get(&w, "max_scale").as_deref(), Some("60"), "each light carries its own brightness");
        assert_eq!(get(&w, "effect_f1").as_deref(), Some("2"), "F1 breathes");
        assert_eq!(get(&w, "effect_f2").as_deref(), Some("0"), "F2 off");
        assert_eq!(get(&w, "effect_m").as_deref(), Some("4"), "the middle follows the page");
        assert_eq!(get(&w, "effect_rgb_hex_m"), Some(hex((60, 120, 255))), "full brightness leaves the colour as it is");
        assert_ne!(get(&w, "effect_rgb_hex_f1"), get(&w, "effect_rgb_hex_m"));
        assert_eq!(get(&w, "effect_l").as_deref(), Some("4"), "the left grip is written (its own colour)");
        assert!(get(&w, "effect_r").is_none(), "the right grip is not touched when it follows the page");
        let plain = led_writes_with(Look::of(Effect::Static((60, 120, 255)), 100), None);
        assert_eq!(get(&plain, "effect_f1").as_deref(), Some("4"), "a look without the lights' choices ignores them (battery)");
        assert!(get(&plain, "effect_l").is_none());
        let none = led_writes_with(look, Some(Lights::default()));
        assert_eq!(get(&none, "max_scale").as_deref(), Some("60"));
        assert!(get(&none, "effect_l").is_none(), "no choices made: the page's look alone");
    }

    #[test]
    fn the_lights_choices_are_remembered_and_junk_is_forgiven() {
        use crate::leds::Hue;
        let d = TestDir::new("brick-lights");
        std::fs::create_dir_all(&*d).unwrap();
        assert_eq!(Lights::load(&d), Lights::default());
        let mut l = Lights::default();
        l.zones[2] = LightPref { hue: Some(Hue::Teal), glow: Glow::Sniff, level: Some(2), ..Default::default() };
        l.save(&d);
        assert_eq!(Lights::load(&d), l);
        std::fs::write(d.join(LIGHTS_FILE), r#"{"zones":[{"glow":"from-the-future","hue":"nope"},{},{},{},{}]}"#).unwrap();
        assert_eq!(Lights::load(&d), Lights::default(), "unknown values mean: like the rest");
        let mut p = LightPref::default();
        p.step_level(1);
        assert_eq!(p.level, Some(0));
        p.step_level(-1);
        p.step_level(-1);
        assert_eq!(p.level, None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_saved_slider_choice_reaches_the_card_watcher() {
        let d = TestDir::new("brick-slider-launcher");
        Slider::Nothing.save(&d);
        let out = std::process::Command::new("bash")
            .arg(crate::testutil::manifest_dir().join("scripts/test-brick-card.sh"))
            .env("BRICK_SLIDER_TESTS_ONLY", "1")
            .env("BRICK_SLIDER_TEST_CHOICE", d.join(SLIDER_FILE))
            .output().expect("host card launcher probe");
        assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn the_mute_slider_choice_is_a_file() {
        let d = TestDir::new("brick-slider");
        std::fs::create_dir_all(&*d).unwrap();
        assert_eq!(Slider::load(&d), Slider::Mute, "no choice yet: it mutes");
        Slider::Nothing.save(&d);
        assert_eq!(Slider::load(&d), Slider::Nothing);
        assert_eq!(std::fs::read_to_string(d.join(SLIDER_FILE)).unwrap().trim(), "nothing");
        Slider::Mute.save(&d);
        assert_eq!(Slider::load(&d), Slider::Mute);
    }
}
