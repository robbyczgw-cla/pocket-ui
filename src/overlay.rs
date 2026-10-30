//! Small messages drawn over the game by pocket-core: the FPS counter, a status badge (fast-forward,
//! rewind) and short notes ("Saved to slot 3"). Each is a dark pill with text, rasterised on the CPU
//! when its text changes and drawn as one textured quad on top of the game's picture.
//! An achievement unlock gets a small card at the top: its badge, "Achievement unlocked · 5 points" and the title.
//! While the game's saves don't reach the card (`save_trouble`), a calm line of two stays at the bottom in the place of
//! the notes (a note, when one comes, has its moment there first).
//! Pixel Path over the game (`pixel_live`) is one more piece, beside the picture. Which of the FPS counter (with or
//! without the sensors) and the strip shows is one setting, Over the game (`over_game`); Off draws neither.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use glow::HasContext;

use crate::display::Look;
use crate::gfx::Canvas;
use crate::image::Image;
use crate::osd;
use crate::text::{Face, Text};

const FONT: u32 = 34;
const PAD_X: i32 = 20;
const PAD_Y: i32 = 12;
/// The FPS line and the badge keep this far from the screen's corner.
pub const MARGIN: i32 = 24;
const KEY: u32 = crate::pixel_live::KEY; // transparent
const PILL: u32 = 0x1a1a1a;
/// The opacity of the messages and the FPS line.
pub const ALPHA: f32 = 0.82;
const TOAST_FOR: Duration = Duration::from_millis(1600);
/// A sentence of pocket-ui's (`note`, "Switched off after 3 h asleep to save the battery."): long enough to be read.
const NOTE_FOR: Duration = Duration::from_secs(6);
/// The widest a message gets before it goes on to a second line (the save warning, a long German note).
const WRAP_AT: i32 = 1000;
const FPS_EVERY: Duration = Duration::from_millis(500);
/// How long the frame around the game takes to fade in at the start (`Overlay::draw_veil`); pocket-ui's launch fade
/// is as long.
pub const VEIL: Duration = Duration::from_millis(250);

/// How dark the veil is `since` the first frame: 1 (black) at the first frame, 0 after `VEIL`, eased out.
pub fn veil_alpha(since: Duration) -> f32 {
    let t = (since.as_secs_f32() / VEIL.as_secs_f32()).clamp(0.0, 1.0);
    (1.0 - t).powi(2)
}

/// The parts of a `view_w` x `view_h` screen outside `hole` (x, y, w, h, top-left origin), as rectangles of the same
/// form; the whole screen without a hole.
pub fn around(view_w: f32, view_h: f32, hole: Option<(f32, f32, f32, f32)>) -> Vec<(f32, f32, f32, f32)> {
    let Some((x, y, w, h)) = hole else { return vec![(0.0, 0.0, view_w, view_h)] };
    let (x0, y0, x1, y1) = (x.max(0.0), y.max(0.0), (x + w).min(view_w), (y + h).min(view_h));
    [(0.0, 0.0, view_w, y0), (0.0, y1, view_w, view_h - y1), (0.0, y0, x0, y1 - y0), (x1, y0, view_w - x1, y1 - y0)]
        .into_iter()
        .filter(|r| r.2 > 0.0 && r.3 > 0.0)
        .collect()
}

/// How long an unlock card stays, and how long it waits for its badge before it shows without one.
const UNLOCK_FOR: Duration = Duration::from_millis(4000);
const BADGE_WAIT: Duration = Duration::from_millis(900);
/// The unlock card: badge size, padding, the text column's widest line.
const BADGE: i32 = 72;
const CARD_PAD: i32 = 18;
const CARD_TEXT_MAX: i32 = 760;
const CARD_ALPHA: f32 = 0.94;

/// An achievement unlock waiting for, or on, the screen.
pub struct Unlock {
    pub id: u32,
    pub title: String,
    pub points: u32,
    pub badge_name: String,
    pub badge: Option<Image>,
    /// The badge couldn't be had: show without waiting.
    pub no_badge: bool,
    queued: Instant,
    shown: Option<Instant>,
}

/// What to show; the host updates it, `Overlay::draw` paints it.
pub struct Hud {
    pub show_fps: bool,
    /// Over the game → FPS + sensors: the device's state follows the counter. Switched with `set_sensors`; the readings come from
    /// a thread of their own (`osd_sensors`), the counter only formats the last ones.
    sensors: Option<crate::osd_sensors::Watcher>,
    /// Frames the core emulated since the last update; shown when it differs from the screen's (fast-forward).
    pub emulated: u32,
    fps_text: String,
    frames: u32,
    since: Instant,
    cpu_ticks: u64,
    pub badge: String,
    toast: Option<(String, Instant)>,
    /// How long the toast stays: `TOAST_FOR`, or `NOTE_FOR` for a note.
    toast_for: Duration,
    /// The game's saves don't reach the card: its line stays at the bottom until one gets through (pocket-core sets it
    /// at its 10 s flush, `save_trouble`).
    pub save_trouble: Option<crate::save_trouble::Trouble>,
    /// Brightness, volume or colour look, as `osd::Data` wire text (sent by pocket-ui), and since when.
    osd: Option<(String, Instant)>,
    /// Multiplied over the whole picture (the Soft colour temperature, see `display.rs`).
    pub tint: [f32; 3],
    /// Achievement unlocks, one card at a time (`achievement`).
    unlocks: VecDeque<Unlock>,
    /// A card just came up: the host plays the unlock sound (`take_chime`).
    chime: bool,
    /// The display look last set (`set_look`), for Pixel Path's Look stage.
    pub look: Look,
    /// Pixel Path over the game (`pixel_live`), set twice a second while it is on; `strip_seq` counts the changes so
    /// the overlay paints it again only then.
    strip: Option<crate::pixel_live::Strip>,
    strip_seq: u64,
}

impl Default for Hud {
    fn default() -> Self {
        Hud {
            show_fps: false,
            sensors: None,
            emulated: 0,
            fps_text: String::new(),
            frames: 0,
            since: Instant::now(),
            cpu_ticks: process_ticks(),
            badge: String::new(),
            toast: None,
            toast_for: TOAST_FOR,
            save_trouble: None,
            osd: None,
            tint: [1.0; 3],
            unlocks: VecDeque::new(),
            chime: false,
            look: Look::default(),
            strip: None,
            strip_seq: 0,
        }
    }
}

/// utime + stime of this process in clock ticks, from /proc/self/stat.
fn process_ticks() -> u64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else { return 0 };
    parse_ticks(&stat).unwrap_or(0)
}

/// Fields 14 and 15 of a /proc/<pid>/stat line (the name in field 2 may contain spaces and parentheses).
fn parse_ticks(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
}

impl Hud {
    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
        self.toast_for = TOAST_FOR;
    }

    /// A toast that stays long enough to read a sentence (pocket-ui's `note`).
    pub fn note(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
        self.toast_for = NOTE_FOR;
    }

    /// Shows the level bar of `osd::Data::to_wire` text for a moment.
    pub fn osd(&mut self, wire: &str) {
        if osd::Data::from_wire(wire).is_some() {
            self.osd = Some((wire.to_string(), Instant::now()));
        }
    }

    fn osd_text(&mut self) -> &str {
        if self.osd.as_ref().is_some_and(|(_, t)| t.elapsed() > osd::SHOW_FOR) {
            self.osd = None;
        }
        self.osd.as_ref().map_or("", |(m, _)| m.as_str())
    }

    /// The colour temperature of the display look; only the Soft backend draws it here.
    pub fn set_look(&mut self, look: &Look) {
        self.tint = if look.soft { look.gains() } else { [1.0; 3] };
        self.look = *look;
    }

    /// Over the game's FPS + sensors on or off (called every frame with the level; only a change does anything): on starts the
    /// reader thread, off ends it.
    pub fn set_sensors(&mut self, on: bool) {
        if on != self.sensors.is_some() {
            self.sensors = on.then(crate::osd_sensors::Watcher::system);
        }
    }

    /// Takes the given watcher instead of the device's own (tests).
    #[cfg(test)]
    fn use_sensors(&mut self, w: crate::osd_sensors::Watcher) {
        self.sensors = Some(w);
    }

    /// Back from a pause: the next half-second starts now, so the pause counts neither as a slow half-second for the
    /// FPS counter nor in Pixel Path's window.
    pub fn restart(&mut self) {
        self.frames = 0;
        self.emulated = 0;
        self.since = Instant::now();
        self.cpu_ticks = process_ticks();
    }

    /// Counts one frame put on the screen; twice a second it recomputes "60 FPS  CPU 23%" and returns that
    /// half-second (Pixel Path keeps the last few, `pixel_path::Window`).
    pub fn frame(&mut self) -> Option<crate::pixel_path::Tick> {
        self.frames += 1;
        let elapsed = self.since.elapsed();
        if elapsed < FPS_EVERY {
            return None;
        }
        // Read even while the counter is hidden, so that switching it on starts from a fresh reading.
        let ticks = process_ticks();
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        let secs = elapsed.as_secs_f64();
        let tick = crate::pixel_path::Tick { secs, frames: self.frames, emulated: self.emulated, cpu_secs: ticks.saturating_sub(self.cpu_ticks) as f64 / hz };
        if self.show_fps {
            let cpu = tick.cpu_secs / secs * 100.0;
            let (fps, emulated) = (self.frames as f64 / secs, self.emulated as f64 / secs);
            self.fps_text = if (emulated - fps).abs() > fps * 0.1 {
                format!("{fps:.0} FPS   EMU {emulated:.0}   CPU {cpu:.0}%")
            } else {
                format!("{fps:.0} FPS   CPU {cpu:.0}%")
            };
            // FPS + sensors: the last readings of the reader thread; what the device lacks isn't there.
            if let Some(w) = self.sensors.as_ref() {
                let more = w.text();
                if !more.is_empty() {
                    self.fps_text.push_str("   ");
                    self.fps_text.push_str(&more);
                }
            }
        }
        self.emulated = 0;
        self.cpu_ticks = ticks;
        self.frames = 0;
        self.since = Instant::now();
        Some(tick)
    }

    /// An achievement was unlocked: its card comes up once its badge is there (or after a short wait without it).
    pub fn achievement(&mut self, id: u32, title: &str, points: u32, badge_name: &str) {
        self.unlocks.push_back(Unlock {
            id,
            title: title.to_string(),
            points,
            badge_name: badge_name.to_string(),
            badge: None,
            no_badge: badge_name.is_empty(),
            queued: Instant::now(),
            shown: None,
        });
    }

    /// The badge `name` arrived (None: it couldn't be had): the cards waiting for it get it.
    pub fn achievement_badge(&mut self, name: &str, image: Option<&Image>) {
        for u in self.unlocks.iter_mut().filter(|u| u.badge_name == name && u.badge.is_none()) {
            match image {
                Some(i) => u.badge = Some(Image { w: i.w, h: i.h, px: i.px.clone() }),
                None => u.no_badge = true,
            }
        }
    }

    /// True once each time an unlock card comes up: the unlock sound's moment.
    pub fn take_chime(&mut self) -> bool {
        std::mem::take(&mut self.chime)
    }

    /// The card on the screen now, if any. Moves the line on: a card that has had its time goes, the next comes up
    /// when its badge is there.
    pub fn current_unlock(&mut self) -> Option<&Unlock> {
        if self.unlocks.front().is_some_and(|u| u.shown.is_some_and(|t| t.elapsed() > UNLOCK_FOR)) {
            self.unlocks.pop_front();
        }
        let u = self.unlocks.front_mut()?;
        if u.shown.is_none() {
            if u.badge.is_none() && !u.no_badge && u.queued.elapsed() < BADGE_WAIT {
                return None;
            }
            u.shown = Some(Instant::now());
            self.chime = true;
        }
        self.unlocks.front()
    }

    /// Shows Pixel Path's strip (None hides it). Only a different strip is painted again.
    pub fn set_strip(&mut self, strip: Option<crate::pixel_live::Strip>) {
        if self.strip != strip {
            self.strip = strip;
            self.strip_seq += 1;
        }
    }

    /// Pixel Path's strip is on the screen (the FPS counter steps aside).
    pub fn strip_shown(&self) -> bool {
        self.strip.is_some()
    }

    /// The note at the bottom: a toast while it has its time, else the save warning while saves fail.
    fn toast_text(&mut self) -> &str {
        if self.toast.as_ref().is_some_and(|(_, t)| t.elapsed() > self.toast_for) {
            self.toast = None;
        }
        match (&self.toast, self.save_trouble) {
            (Some((m, _)), _) => m.as_str(),
            (None, Some(t)) => t.line(),
            (None, None) => "",
        }
    }
}

#[derive(Clone, Copy)]
enum Anchor {
    TopLeft,
    TopRight,
    BottomCenter,
    TopCenter,
}

struct Piece {
    texture: glow::Texture,
    text: String,
    size: (i32, i32),
    anchor: Anchor,
}

pub struct Overlay {
    program: glow::Program,
    rect: Option<glow::UniformLocation>,
    tex: Option<glow::UniformLocation>,
    alpha: Option<glow::UniformLocation>,
    pieces: [Piece; 4],
    /// The achievement card, and which unlock (and whether with its badge) it holds.
    unlock: Piece,
    /// Pixel Path's strip: the `Hud::strip_seq` it was painted for, where it goes and how opaque it is.
    strip: Piece,
    strip_drawn: u64,
    strip_at: (i32, i32, f32),
    text: Text,
    /// 1x1 texture holding the tint colour, drawn with a multiply blend over the picture.
    tint_tex: glow::Texture,
    tint_set: [f32; 3],
    /// 1x1 black, for the veil over the frame at the start; made with the first veil.
    veil_tex: Option<glow::Texture>,
    /// 1x1 white, for the power-on effect's line and glow (`draw_solid`); made with the first one.
    white_tex: Option<glow::Texture>,
}

pub(crate) const VS: &str = "#version 300 es
uniform vec4 rect;
out vec2 v;
void main() {
    vec2 p = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
    v = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4(mix(rect.xy, rect.zw, p), 0.0, 1.0);
}";

pub(crate) const FS: &str = "#version 300 es
precision mediump float;
uniform sampler2D tex;
uniform float alpha;
in vec2 v;
out vec4 o;
void main() {
    vec4 c = texture(tex, v);
    o = vec4(c.rgb, c.a * alpha);
}";

impl Overlay {
    pub fn new(gl: &glow::Context) -> Result<Overlay, String> {
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
                let sh = gl.create_shader(kind)?;
                // GLSL 3.30 on a desktop OpenGL context (3D cores, see `video::glsl_for`).
                gl.shader_source(sh, &crate::video::glsl_for(gl, src));
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    return Err(gl.get_shader_info_log(sh));
                }
                gl.attach_shader(program, sh);
                shaders.push(sh);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            for sh in shaders {
                gl.detach_shader(program, sh);
                gl.delete_shader(sh);
            }
            let piece = |anchor| -> Result<Piece, String> {
                let texture = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                for (p, v) in [
                    (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                    (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
                }
                Ok(Piece { texture, text: String::new(), size: (0, 0), anchor })
            };
            let tint_tex = piece(Anchor::TopLeft)?.texture;
            let pieces = [piece(Anchor::TopLeft)?, piece(Anchor::TopRight)?, piece(Anchor::BottomCenter)?, piece(Anchor::TopCenter)?];
            let unlock = piece(Anchor::TopCenter)?;
            let strip = piece(Anchor::TopLeft)?;
            Ok(Overlay {
                rect: gl.get_uniform_location(program, "rect"),
                tex: gl.get_uniform_location(program, "tex"),
                alpha: gl.get_uniform_location(program, "alpha"),
                program,
                pieces,
                unlock,
                strip,
                strip_drawn: 0,
                strip_at: (0, 0, 0.0),
                text: Text::new(),
                tint_tex,
                tint_set: [0.0; 3],
                veil_tex: None,
                white_tex: None,
            })
        }
    }

    /// Draws whatever the HUD currently shows. Cheap when nothing does.
    pub fn draw(&mut self, gl: &glow::Context, hud: &mut Hud, view_w: u32, view_h: u32) {
        let toast = hud.toast_text().to_string();
        let fps = if hud.show_fps { hud.fps_text.clone() } else { String::new() };
        let osd_text = hud.osd_text().to_string();
        let texts = [fps, hud.badge.clone(), toast, osd_text];
        let tinted = hud.tint != [1.0; 3];
        // The unlock card: which one, and whether its badge is in (a badge arriving later redraws it).
        let unlock_key = hud.current_unlock().map(|u| format!("{}:{}", u.id, u.badge.is_some()));
        if texts.iter().all(String::is_empty) && !tinted && unlock_key.is_none() && hud.strip.is_none() {
            return;
        }
        unsafe {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(self.tex.as_ref(), 0);
            gl.uniform_1_f32(self.alpha.as_ref(), ALPHA);
        }
        if tinted {
            self.draw_tint(gl, hud.tint);
        }
        // Under the messages: a toast or the volume bar that lands on it stays readable.
        if let Some(s) = hud.strip.as_ref() {
            self.draw_strip(gl, s, hud.strip_seq, view_w, view_h);
        }
        for (i, text) in texts.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            if self.pieces[i].text != *text {
                self.render(gl, i, text);
            }
            let piece = &self.pieces[i];
            let (w, h) = piece.size;
            let (x, y) = match piece.anchor {
                Anchor::TopLeft => (MARGIN, MARGIN),
                Anchor::TopRight => (view_w as i32 - MARGIN - w, MARGIN),
                Anchor::BottomCenter => ((view_w as i32 - w) / 2, view_h as i32 - MARGIN - h),
                Anchor::TopCenter => ((view_w as i32 - w) / 2, osd::TOP),
            };
            let (vw, vh) = (view_w as f32, view_h as f32);
            let ndc = |px: i32, total: f32| px as f32 / total * 2.0 - 1.0;
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(piece.texture));
                gl.uniform_4_f32(self.rect.as_ref(), ndc(x, vw), -ndc(y + h, vh), ndc(x + w, vw), -ndc(y, vh));
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
        }
        if let Some(key) = unlock_key {
            if self.unlock.text != key {
                if let Some(u) = hud.current_unlock() {
                    let (w, h, buf) = paint_unlock(&mut self.text, u);
                    upload(gl, &mut self.unlock, w, h, &buf);
                }
                self.unlock.text = key;
            }
            // Under the volume or brightness bar when that is up too.
            let below_osd = !texts[3].is_empty();
            let (w, h) = self.unlock.size;
            let (x, y) = ((view_w as i32 - w) / 2, if below_osd { osd::TOP + osd::H + 12 } else { MARGIN });
            let (vw, vh) = (view_w as f32, view_h as f32);
            let ndc = |px: i32, total: f32| px as f32 / total * 2.0 - 1.0;
            unsafe {
                gl.uniform_1_f32(self.alpha.as_ref(), CARD_ALPHA);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.unlock.texture));
                gl.uniform_4_f32(self.rect.as_ref(), ndc(x, vw), -ndc(y + h, vh), ndc(x + w, vw), -ndc(y, vh));
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
        }
        unsafe { gl.disable(glow::BLEND) };
    }

    /// Pixel Path's strip where `pixel_live::place` puts it; painted again only when the strip changed (twice a second
    /// at most), drawn as one quad otherwise.
    fn draw_strip(&mut self, gl: &glow::Context, s: &crate::pixel_live::Strip, seq: u64, view_w: u32, view_h: u32) {
        use crate::pixel_live as live;
        if self.strip_drawn != seq {
            let text = &mut self.text;
            let at = live::place(s, |style| live::measure(text, s, style));
            let (w, h, buf) = live::paint(text, s, at.style);
            upload(gl, &mut self.strip, w, h, &buf);
            self.strip_at = (at.x, at.y, at.style.alpha());
            self.strip_drawn = seq;
        }
        let ((x, y, alpha), (w, h)) = (self.strip_at, self.strip.size);
        let (vw, vh) = (view_w as f32, view_h as f32);
        let ndc = |px: i32, total: f32| px as f32 / total * 2.0 - 1.0;
        unsafe {
            gl.uniform_1_f32(self.alpha.as_ref(), alpha);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.strip.texture));
            gl.uniform_4_f32(self.rect.as_ref(), ndc(x, vw), -ndc(y + h, vh), ndc(x + w, vw), -ndc(y, vh));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.uniform_1_f32(self.alpha.as_ref(), ALPHA);
        }
    }

    /// Black at `alpha` (0..=1) over the screen outside `hole` (the game's picture, x, y, w, h in pixels from the top
    /// left): the frame around the game fades in during its first moments, out of the black around the picture that
    /// pocket-ui showed while the game started (`launch_picture`). Without a hole (a turned picture), over everything.
    pub fn draw_veil(&mut self, gl: &glow::Context, view_w: u32, view_h: u32, hole: Option<(f32, f32, f32, f32)>, alpha: f32) {
        if alpha <= 0.0 {
            return;
        }
        let (vw, vh) = (view_w.max(1) as f32, view_h.max(1) as f32);
        unsafe {
            let tex = match self.veil_tex {
                Some(t) => t,
                None => {
                    let Ok(t) = gl.create_texture() else { return };
                    gl.bind_texture(glow::TEXTURE_2D, Some(t));
                    for p in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::NEAREST as i32);
                    }
                    gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&[0, 0, 0, 255]));
                    self.veil_tex = Some(t);
                    t
                }
            };
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.uniform_1_i32(self.tex.as_ref(), 0);
            gl.uniform_1_f32(self.alpha.as_ref(), alpha.min(1.0));
            let ndc = |px: f32, total: f32| px / total * 2.0 - 1.0;
            for (x, y, w, h) in around(vw, vh, hole) {
                gl.uniform_4_f32(self.rect.as_ref(), ndc(x, vw), -ndc(y + h, vh), ndc(x + w, vw), -ndc(y, vh));
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
            gl.disable(glow::BLEND);
        }
    }

    /// Solid black or white rectangles over the screen (`crt_on::plan`): the old-TV power-on over a game's first
    /// moments. Plain quads through the overlay's own program, nothing else is touched.
    pub fn draw_solid(&mut self, gl: &glow::Context, view_w: u32, view_h: u32, quads: &[crate::crt_on::Quad]) {
        if quads.is_empty() {
            return;
        }
        let (vw, vh) = (view_w.max(1) as f32, view_h.max(1) as f32);
        unsafe {
            if self.white_tex.is_none() {
                let Ok(t) = gl.create_texture() else { return };
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                for p in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::NEAREST as i32);
                }
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&[255, 255, 255, 255]));
                self.white_tex = Some(t);
            }
            if self.veil_tex.is_none() {
                let Ok(t) = gl.create_texture() else { return };
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                for p in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::NEAREST as i32);
                }
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&[0, 0, 0, 255]));
                self.veil_tex = Some(t);
            }
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(self.tex.as_ref(), 0);
            let ndc = |px: f32, total: f32| px / total * 2.0 - 1.0;
            for q in quads {
                gl.bind_texture(glow::TEXTURE_2D, if q.white { self.white_tex } else { self.veil_tex });
                gl.uniform_1_f32(self.alpha.as_ref(), q.alpha.clamp(0.0, 1.0));
                let (x, y, w, h) = q.rect;
                gl.uniform_4_f32(self.rect.as_ref(), ndc(x, vw), -ndc(y + h, vh), ndc(x + w, vw), -ndc(y, vh));
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
            gl.uniform_1_f32(self.alpha.as_ref(), ALPHA);
            gl.disable(glow::BLEND);
        }
    }

    /// Multiplies the whole picture with the colour `tint` (all channels 0..=1): the Soft colour temperature.
    fn draw_tint(&mut self, gl: &glow::Context, tint: [f32; 3]) {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tint_tex));
            if self.tint_set != tint {
                let px = [byte(tint[0]), byte(tint[1]), byte(tint[2]), 255];
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&px));
                self.tint_set = tint;
            }
            // destination * texture colour, over the whole viewport
            gl.blend_func(glow::DST_COLOR, glow::ZERO);
            gl.uniform_4_f32(self.rect.as_ref(), -1.0, -1.0, 1.0, 1.0);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }
    }

    /// Rasterises `text` in a pill and uploads it as the texture of piece `i` (piece 3: the level bar of an OSD).
    fn render(&mut self, gl: &glow::Context, i: usize, text: &str) {
        let (w, h, buf) = if i == 3 {
            let Some(data) = osd::Data::from_wire(text) else { return };
            let mut buf = vec![KEY; (osd::W * osd::H) as usize];
            let mut c = Canvas::new(&mut buf, osd::W as u32, osd::H as u32);
            osd::paint(&mut c, &mut self.text, 0, 0, &data, PILL);
            (osd::W, osd::H, buf)
        } else {
            paint_pill(&mut self.text, text)
        };
        upload(gl, &mut self.pieces[i], w, h, &buf);
        self.pieces[i].text = text.to_string();
    }
}

/// A message in its dark pill: one line, or a rounded box of several when it is wider than `WRAP_AT` (the save
/// warning). Returns width, height and pixels (`KEY` is transparent).
pub fn paint_pill(text: &mut Text, msg: &str) -> (i32, i32, Vec<u32>) {
    let lines = if text.width(Face::Body, FONT, msg) > WRAP_AT { text.wrap(Face::Body, FONT, msg, WRAP_AT) } else { vec![msg.to_string()] };
    let line_h = FONT as i32 * 13 / 10;
    let w = lines.iter().map(|l| text.width(Face::Body, FONT, l)).max().unwrap_or(0) + 2 * PAD_X;
    let h = FONT as i32 + (lines.len() as i32 - 1) * line_h + 2 * PAD_Y;
    let mut buf = vec![KEY; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w as u32, h as u32);
    let radius = if lines.len() > 1 { 26 } else { h / 2 };
    c.round_rect(0, 0, w, h, radius, PILL);
    for (i, l) in lines.iter().enumerate() {
        text.draw(&mut c, Face::Body, FONT, PAD_X, PAD_Y + FONT as i32 * 4 / 5 + i as i32 * line_h, 0xffffff, l);
    }
    (w, h, buf)
}

/// Puts a CPU-drawn picture (`KEY` is transparent) into a piece's texture.
fn upload(gl: &glow::Context, piece: &mut Piece, w: i32, h: i32, buf: &[u32]) {
    let rgba: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, if *p == KEY { 0 } else { 255 }]).collect();
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(piece.texture));
        // The video renderer leaves its own row length and alignment set.
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&rgba));
    }
    piece.size = (w, h);
}

/// The unlock card: the badge on the left (a drawn ring in amber when there is none), "Achievement unlocked · 5 points"
/// in grey over the title in white. Dark like every message over a game. Returns width, height and pixels.
pub fn paint_unlock(text: &mut Text, u: &Unlock) -> (i32, i32, Vec<u32>) {
    const SMALL: u32 = 24;
    const TITLE: u32 = 34;
    let head = match u.points {
        0 => "Achievement unlocked".to_string(),
        1 => "Achievement unlocked · 1 point".to_string(),
        p => format!("Achievement unlocked · {p} points"),
    };
    let title = text.fit(Face::Title, TITLE, &u.title, CARD_TEXT_MAX);
    let text_w = text.width(Face::Body, SMALL, &head).max(text.width(Face::Title, TITLE, &title));
    let h = BADGE + 2 * CARD_PAD;
    let w = CARD_PAD + BADGE + 20 + text_w + CARD_PAD + 8;
    let mut buf = vec![KEY; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w as u32, h as u32);
    c.round_rect(0, 0, w, h, 26, PILL);
    let (bx, by) = (CARD_PAD, CARD_PAD);
    match &u.badge {
        Some(img) => c.blit_scaled(img, bx, by, BADGE, BADGE),
        None => {
            c.round_rect(bx, by, BADGE, BADGE, 14, 0x2a2a2e);
            c.ring(bx + BADGE / 2, by + BADGE / 2, 20, 4, 0xd39a2c);
            c.disc(bx + BADGE / 2, by + BADGE / 2, 7, 0xd39a2c);
        }
    }
    let tx = bx + BADGE + 20;
    text.draw(&mut c, Face::Body, SMALL, tx, CARD_PAD + 26, 0xaaa79f, &head);
    text.draw(&mut c, Face::Title, TITLE, tx, CARD_PAD + 26 + 46, 0xffffff, &title);
    (w, h, buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn the_veil_covers_what_is_around_the_picture_and_fades_in_its_time() {
        let parts = around(1240.0, 1080.0, Some((60.0, 36.0, 1120.0, 1008.0)));
        assert_eq!(parts.len(), 4);
        let area: f32 = parts.iter().map(|r| r.2 * r.3).sum();
        assert_eq!(area, 1240.0 * 1080.0 - 1120.0 * 1008.0);
        // A picture as wide as the screen leaves bands above and below only; none leaves the whole screen.
        assert_eq!(around(1240.0, 1080.0, Some((0.0, 75.0, 1240.0, 930.0))).len(), 2);
        assert_eq!(around(1240.0, 1080.0, None), vec![(0.0, 0.0, 1240.0, 1080.0)]);
        assert_eq!(around(1240.0, 1080.0, Some((0.0, 0.0, 1240.0, 1080.0))), vec![]);
        assert_eq!(veil_alpha(Duration::ZERO), 1.0);
        assert!(veil_alpha(VEIL / 2) > 0.0 && veil_alpha(VEIL / 2) < 0.5, "eased out: most of it gone halfway");
        assert_eq!(veil_alpha(VEIL), 0.0);
        assert_eq!(veil_alpha(VEIL * 3), 0.0);
    }

    #[test]
    fn the_strip_is_painted_again_only_when_it_changed() {
        let mut hud = Hud::default();
        assert!(!hud.strip_shown());
        let strip = crate::pixel_live::demo("gba").unwrap();
        hud.set_strip(Some(strip.clone()));
        assert!(hud.strip_shown());
        let seq = hud.strip_seq;
        hud.set_strip(Some(strip.clone()));
        assert_eq!(hud.strip_seq, seq, "the same strip twice a second: nothing to paint");
        hud.set_strip(crate::pixel_live::demo("compromised"));
        assert_eq!(hud.strip_seq, seq + 1);
        hud.set_strip(None);
        assert!(!hud.strip_shown());
    }

    #[test]
    fn a_note_of_pocket_ui_stays_long_enough_to_be_read() {
        let mut hud = Hud::default();
        let text = "Switched off after 3 h asleep to save the battery.";
        hud.note(text);
        hud.toast = Some((text.into(), Instant::now() - TOAST_FOR - Duration::from_millis(1)));
        assert_eq!(hud.toast_text(), text, "still there when a toast would be gone");
        hud.toast = Some((text.into(), Instant::now() - NOTE_FOR - Duration::from_millis(1)));
        assert_eq!(hud.toast_text(), "");
        // A toast after it has a toast's time again.
        hud.toast("Saved to slot 2");
        hud.toast = Some(("Saved to slot 2".into(), Instant::now() - TOAST_FOR - Duration::from_millis(1)));
        assert_eq!(hud.toast_text(), "");
    }

    #[test]
    fn the_save_warning_stays_until_saves_get_through_and_a_note_has_its_moment_first() {
        use crate::save_trouble::Trouble;
        let mut hud = Hud::default();
        assert_eq!(hud.toast_text(), "");
        hud.save_trouble = Some(Trouble::Full);
        assert_eq!(hud.toast_text(), Trouble::Full.line());
        hud.toast("Saved to slot 2");
        assert_eq!(hud.toast_text(), "Saved to slot 2");
        hud.toast = Some(("Saved to slot 2".into(), Instant::now() - TOAST_FOR - Duration::from_millis(1)));
        assert_eq!(hud.toast_text(), Trouble::Full.line(), "back after the note, not gone with it");
        hud.save_trouble = None;
        assert_eq!(hud.toast_text(), "");
        // Two lines inside the screen, not one running off it.
        let mut text = Text::new();
        let (w, h, _) = paint_pill(&mut text, Trouble::Unwritable.line());
        assert!(w <= WRAP_AT + 2 * PAD_X && h > FONT as i32 * 2, "{w}x{h}");
        let (_, h1, _) = paint_pill(&mut text, "Saved to slot 2");
        assert_eq!(h1, FONT as i32 + 2 * PAD_Y, "a short note keeps its one line");
    }

    #[test]
    fn process_ticks_come_from_fields_14_and_15_even_with_odd_names() {
        let line = "123 (pocket (core) x) S 1 123 123 0 -1 4194560 100 0 0 0 250 30 0 0 20 0 5 0 1000 1000 1 18446744073709551615";
        assert_eq!(parse_ticks(line), Some(280));
        assert_eq!(parse_ticks("garbage"), None);
        assert_eq!(parse_ticks("1 (x) S 1"), None);
    }

    #[test]
    fn unlock_cards_wait_for_their_badge_then_come_one_at_a_time() {
        let mut hud = Hud::default();
        assert!(hud.current_unlock().is_none());
        hud.achievement(5501, "First steps", 5, "00234");
        hud.achievement(5502, "Second", 10, "00235");
        assert!(hud.current_unlock().is_none(), "waits for the badge");
        assert!(!hud.take_chime());
        let badge = Image { w: 64, h: 64, px: vec![0x336699; 64 * 64] };
        hud.achievement_badge("00234", Some(&badge));
        assert_eq!(hud.current_unlock().map(|u| (u.id, u.badge.is_some())), Some((5501, true)));
        assert!(hud.take_chime(), "the sound comes with the card");
        assert!(!hud.take_chime(), "once");
        assert_eq!(hud.current_unlock().map(|u| u.id), Some(5501), "stays while it has its time");
        hud.unlocks[0].shown = Some(Instant::now() - UNLOCK_FOR - Duration::from_millis(1));
        // The next one has no badge yet: after the short wait it comes without.
        hud.unlocks[1].queued = Instant::now() - BADGE_WAIT - Duration::from_millis(1);
        assert_eq!(hud.current_unlock().map(|u| (u.id, u.badge.is_some())), Some((5502, false)));
        assert!(hud.take_chime());
        hud.unlocks[0].shown = Some(Instant::now() - UNLOCK_FOR - Duration::from_millis(1));
        assert!(hud.current_unlock().is_none());
        // A badge that can't be had doesn't hold the card up.
        hud.achievement(5503, "Third", 1, "00236");
        hud.achievement_badge("00236", None);
        assert_eq!(hud.current_unlock().map(|u| u.id), Some(5503));
    }

    #[test]
    fn the_unlock_card_fits_long_titles_and_has_its_badge() {
        let mut text = Text::new();
        let mut hud = Hud::default();
        hud.achievement(1, "A very long achievement title that goes on and on and on and would never fit on one line of the card", 25, "");
        let u = hud.current_unlock().unwrap();
        let (w, h, buf) = paint_unlock(&mut text, u);
        assert!(w <= CARD_PAD + BADGE + 20 + CARD_TEXT_MAX + CARD_PAD + 8 && h == BADGE + 2 * CARD_PAD, "{w}x{h}");
        assert_eq!(buf[0], KEY, "rounded corner is see-through");
        assert!(buf.contains(&0xd39a2c), "the drawn mark stands in for a missing badge");
    }

    #[test]
    fn toasts_expire_and_fps_text_waits_for_its_first_interval() {
        let mut hud = Hud::default();
        assert_eq!(hud.toast_text(), "");
        hud.toast("Saved to slot 2");
        assert_eq!(hud.toast_text(), "Saved to slot 2");
        hud.toast = Some(("old".into(), Instant::now() - TOAST_FOR - Duration::from_millis(1)));
        assert_eq!(hud.toast_text(), "");
        hud.show_fps = true;
        hud.frame();
        assert!(hud.fps_text.is_empty(), "less than half a second has passed");
        hud.since = Instant::now() - Duration::from_secs(1);
        let tick = hud.frame().expect("a second has passed");
        assert_eq!((tick.frames, tick.emulated), (2, 0));
        assert!(tick.secs >= 1.0);
        assert!(hud.fps_text.contains("FPS") && hud.fps_text.contains("CPU"), "{}", hud.fps_text);
        assert!(hud.frame().is_none(), "the next half-second has only begun");
        // Back from a pause: the pause's time doesn't count.
        hud.since = Instant::now() - Duration::from_secs(30);
        hud.restart();
        assert!(hud.frame().is_none());
    }

    #[test]
    fn off_formats_nothing_and_starts_no_reader() {
        // Over the game on Off, as pocket-core sets it every frame (`over_game::Level`).
        let off = crate::over_game::Level::Off;
        let mut hud = Hud { show_fps: off.counter(), ..Hud::default() };
        hud.set_sensors(off.sensors());
        for _ in 0..3 {
            hud.since = Instant::now() - Duration::from_secs(1);
            assert!(hud.frame().is_some(), "Pixel Path's window still gets its half-seconds");
        }
        assert!(hud.fps_text.is_empty(), "no line formatted: {}", hud.fps_text);
        assert!(hud.sensors.is_none() && !hud.strip_shown(), "no reader thread, no strip");
        // What `Overlay::draw` checks before it touches GL: nothing to draw.
        assert!(hud.toast_text().is_empty() && hud.badge.is_empty() && hud.osd_text().is_empty() && hud.tint == [1.0; 3]);
        assert!(hud.current_unlock().is_none());
    }

    #[test]
    fn the_counter_takes_the_sensors_from_the_cache_and_leaves_out_what_is_missing() {
        use crate::osd_sensors::Watcher;
        let root = TestDir::new("hud-sensors");
        std::fs::create_dir_all(root.join("sys/class/thermal/thermal_zone0")).unwrap();
        std::fs::write(root.join("sys/class/thermal/thermal_zone0/temp"), "62400\n").unwrap();
        let tick = |hud: &mut Hud| {
            hud.since = Instant::now() - Duration::from_secs(1);
            hud.frame().expect("a second has passed");
        };
        let mut hud = Hud { show_fps: true, ..Hud::default() };
        tick(&mut hud);
        assert!(!hud.fps_text.contains("°C"), "plain On: no sensors, no reader thread: {}", hud.fps_text);
        let w = Watcher::start(root.to_path_buf(), root.join("battery"), Duration::from_millis(5), crate::i18n::Lang::En);
        let until = Instant::now() + Duration::from_secs(5);
        while w.reads() == 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        hud.use_sensors(w);
        tick(&mut hud);
        assert!(hud.fps_text.contains("CPU") && hud.fps_text.ends_with("   62 °C"), "{}", hud.fps_text);
        assert!(!hud.fps_text.contains("Fan") && !hud.fps_text.contains(" W") && !hud.fps_text.contains('?'), "no fan, no gauge: left out: {}", hud.fps_text);
        hud.set_sensors(false);
        tick(&mut hud);
        assert!(!hud.fps_text.contains("°C"), "{}", hud.fps_text);
    }
}
