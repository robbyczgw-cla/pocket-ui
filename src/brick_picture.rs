//! Shaders, screen frames and picture effects for games on the Brick (docs/TRIMUI-BRICK.md, "Shaders and frames"). pocket-core there
//! draws no GL of its own (the Null WSI has one screen, and `pocket-fb` holds it): it puts the core's picture in its window at the
//! core's own size and tells `pocket-fb` how to show it (`pocket_picture`, over the sway IPC, the way pocket-ui asks for transitions).
//! `pocket-fb --gl` then runs the very renderer pocket-core has on ROCKNIX devices (`video::Renderer`: scaling, sharpness, grid,
//! scanlines, CRT, the screen frames, Ambient, orientation) on that picture, in its own context, straight onto the page that flips.
//!
//! Without the GPU (`pocket-fb` without `--gl`, or one that lost it) the bridge answers no, pocket-core keeps the CPU copy, and
//! pocket-ui leaves the rows out that would do nothing (`shaders_available`). Only behind the `brick` feature.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::control::ipc;
use crate::libretro::Frame;
use crate::video::{Renderer, VideoSettings};

const ASK: Duration = Duration::from_millis(300);

/// The settings of the Picture group, which `pocket-fb` acts on (the frame colours and finishes are in it).
pub fn keys() -> Vec<&'static str> {
    crate::options::GROUPS.iter().filter(|g| g.name == "Picture").flat_map(|g| g.settings.iter().map(|s| s.key)).collect()
}

/// The Picture settings in force for this console and game, as the menu keeps them; Kids Mode's picture over them.
pub fn values(data_dir: &Path, system: &str, rom: &Path, kids: bool) -> BTreeMap<String, String> {
    let keys = keys();
    let mut v: BTreeMap<String, String> = crate::options::GameSettings::load(data_dir, system, rom).effective().into_iter().filter(|(k, _)| keys.contains(&k.as_str())).collect();
    if kids {
        v.insert("scaling".into(), "aspect".into());
        v.insert("frame".into(), "none".into());
    }
    v
}

/// Whether the screen's bridge draws pictures with shaders and frames: asked once (`pocket_shaders`), said once in the log. Rows that
/// need it (Settings > Screen frames, the pause menu's Picture rows) show only when this is true.
pub fn shaders_available() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = ipc::request(ipc::RUN_COMMAND, "pocket_shaders", ASK).is_ok_and(|r| ipc::all_succeeded(&r));
        println!("brick: shaders and screen frames {}", if on { "on the GPU (pocket-fb --gl)" } else { "off (no GPU path)" });
        on
    })
}

/// What pocket-core tells the bridge about the picture it hands over.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub system: String,
    /// The core's display aspect (0: the picture's own).
    pub aspect: f32,
    /// SET_ROTATION's quarter turns.
    pub turns: u32,
    pub frames_dir: PathBuf,
    /// The Picture settings by key (`options::GROUPS`), as `VideoSettings::from_values` reads them.
    pub values: BTreeMap<String, String>,
}

pub fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<String> {
    let bytes = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

impl Request {
    /// `pocket_picture <system> <aspect> <turns> <frames dir, hex> key=value ...`; a value has no spaces (the test below checks every choice).
    pub fn command(&self) -> String {
        let mut c = format!("pocket_picture {} {:.5} {} {}", hex(&self.system), self.aspect, self.turns, hex(&self.frames_dir.to_string_lossy()));
        for (k, v) in &self.values {
            c.push_str(&format!(" {k}={v}"));
        }
        c
    }

    /// `command` read back (the words after `pocket_picture`); None when it doesn't read.
    pub fn read(words: &str) -> Option<Request> {
        let mut w = words.split_whitespace();
        let system = unhex(w.next()?)?;
        let aspect: f32 = w.next()?.parse().ok()?;
        let turns: u32 = w.next()?.parse().ok()?;
        let frames_dir = PathBuf::from(unhex(w.next()?)?);
        let values = w.map(|kv| kv.split_once('=').map(|(k, v)| (k.to_string(), v.to_string()))).collect::<Option<BTreeMap<_, _>>>()?;
        Some(Request { system, aspect: if aspect.is_finite() { aspect.max(0.0) } else { 0.0 }, turns: turns & 3, frames_dir, values })
    }

    pub fn settings(&self) -> VideoSettings {
        VideoSettings::from_values(&self.values)
    }
}

/// Tells the bridge how to show the picture. False when it won't (no GPU there): the CPU copy stays.
pub fn send(request: &Request) -> bool {
    ipc::request(ipc::RUN_COMMAND, &request.command(), ASK).is_ok_and(|r| ipc::all_succeeded(&r))
}

/// The pause menu's picture through the bridge's GPU (the shaders and the frame as the game shows them now): the bridge renders its last
/// picture of this game at `w` by `h` and writes it as `preview::SHOT_FILE` with `seq`. False: nothing was written (no GPU path, no picture).
pub fn send_shot(seq: u32, w: u32, h: u32) -> bool {
    ipc::request(ipc::RUN_COMMAND, &format!("pocket_picture_shot {seq} {w} {h}"), ASK).is_ok_and(|r| ipc::all_succeeded(&r))
}

/// The Picture page's live preview through the bridge's GPU (`pocket-fb`'s `pocket_picture_preview`): the last picture drawn at the
/// screen's `sw` by `sh` with the settings sent, cut to the page's `w` by `h` and the overview, written as `preview::FILE` with `seq`.
pub fn send_preview(seq: u32, (sw, sh): (u32, u32), (w, h): (u32, u32)) -> bool {
    ipc::request(ipc::RUN_COMMAND, &format!("pocket_picture_preview {seq} {sw} {sh} {w} {h}"), ASK).is_ok_and(|r| ipc::all_succeeded(&r))
}

/// What pocket-core lays over the game, as one scene for the bridge (`pocket-fb`'s `pocket_overlay`): on ROCKNIX pocket-core draws its
/// overlay stage itself; on the Brick it has no GL, so everything over the picture takes this one way. `key=hex` words; a reader skips keys
/// it does not know, so later parts (achievement cards, Pixel Path's strip, the start veil) come the same way.
pub const OVERLAY_KEYS: [&str; 4] = ["fps", "badge", "toast", "osd"];

/// `pocket_overlay` with the overlay's texts (`Hud::texts`: the FPS line, the badge, the toast or save warning, the volume or brightness bar).
pub fn overlay_command(texts: &[String; 4]) -> String {
    let mut c = String::from("pocket_overlay");
    for (k, t) in OVERLAY_KEYS.iter().zip(texts) {
        if !t.is_empty() {
            c.push_str(&format!(" {k}={}", hex(t)));
        }
    }
    c
}

/// The bridge's side of `overlay_command`: the texts by key, missing ones empty; None when a word does not read.
pub fn read_overlay(words: &str) -> Option<[String; 4]> {
    let mut texts: [String; 4] = Default::default();
    for w in words.split_whitespace() {
        let (k, v) = w.split_once('=')?;
        let v = unhex(v)?;
        if let Some(i) = OVERLAY_KEYS.iter().position(|o| *o == k) {
            texts[i] = v;
        }
    }
    Some(texts)
}

/// Sends `pocket_overlay` lines on a thread of its own (the game's thread never waits for the bridge): the newest one wins.
pub struct HudSender {
    tx: std::sync::mpsc::Sender<String>,
}

impl HudSender {
    pub fn new() -> HudSender {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let _ = std::thread::Builder::new().name("brick-overlay".into()).spawn(move || {
            while let Ok(mut line) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    line = newer;
                }
                if !ipc::request(ipc::RUN_COMMAND, &line, ASK).is_ok_and(|r| ipc::all_succeeded(&r)) {
                    println!("brick picture: the bridge took nothing over the game");
                }
            }
        });
        HudSender { tx }
    }

    pub fn send(&self, texts: &[String; 4]) {
        let _ = self.tx.send(overlay_command(texts));
    }
}

/// The CPU copy is back on (the game ended, or the bridge lost its GPU): nothing to draw from here on.
pub fn send_off() {
    let _ = ipc::request(ipc::RUN_COMMAND, "pocket_picture off", ASK);
}

/// The core's picture as `pocket-fb` receives it in the window's buffer: XRGB8888 at its own size.
pub fn to_xrgb(frame: &Frame, out: &mut Vec<u8>) -> (u32, u32) {
    use crate::libretro::{PIXEL_RGB565, PIXEL_XRGB8888};
    let (w, h) = (frame.width as usize, frame.height as usize);
    out.clear();
    out.resize(w * h * 4, 0);
    let expand = |v: u32, bits: u32| (v << (8 - bits)) | (v >> (2 * bits - 8));
    for y in 0..h {
        let Some(row) = frame.data.get(y * frame.pitch..) else { break };
        let dst = &mut out[y * w * 4..(y + 1) * w * 4];
        match frame.format {
            PIXEL_XRGB8888 if row.len() >= w * 4 => dst.copy_from_slice(&row[..w * 4]),
            PIXEL_XRGB8888 => {}
            f => {
                if row.len() < w * 2 {
                    break;
                }
                for (x, d) in dst.chunks_exact_mut(4).enumerate() {
                    let v = u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]));
                    let (r, g, b) = if f == PIXEL_RGB565 { (expand(v >> 11, 5), expand((v >> 5) & 0x3f, 6), expand(v & 0x1f, 5)) } else { (expand((v >> 10) & 0x1f, 5), expand((v >> 5) & 0x1f, 5), expand(v & 0x1f, 5)) };
                    d.copy_from_slice(&[b as u8, g as u8, r as u8, 0]);
                }
            }
        }
    }
    (frame.width, frame.height)
}

/// The renderer inside `pocket-fb`: pocket-core's `video::Renderer` on the picture a game window holds.
pub struct Layer {
    renderer: Renderer,
    request: Option<Request>,
    settings: VideoSettings,
}

impl Layer {
    /// On the context that is current (`Egl::another_gl`). Compiles the plain variant; an error carries the shader log.
    pub fn new(gl: glow::Context) -> Result<Layer, String> {
        let mut renderer = Renderer::new(gl)?;
        renderer.set_frame_async(true);
        Ok(Layer { renderer, request: None, settings: VideoSettings::default() })
    }

    /// A new `pocket_picture`: from the next picture on, it is drawn this way. The shader variant it needs is compiled now, not in the
    /// middle of a frame.
    pub fn set(&mut self, request: Request) {
        let settings = request.settings();
        if self.request.as_ref().is_none_or(|r| r.system != request.system || r.frames_dir != request.frames_dir) {
            self.renderer.set_frame_source(&request.system, &request.frames_dir);
        }
        self.renderer.set_rotation(request.turns);
        self.renderer.warm(&settings);
        self.settings = settings;
        self.request = Some(request);
    }

    pub fn clear(&mut self) {
        self.request = None;
    }

    pub fn active(&self) -> bool {
        self.request.is_some()
    }

    /// The picture is drawn onto the screen (`screen`), no swap. Err when GL says so.
    pub fn draw(&mut self, pixels: &[u8], width: u32, height: u32, stride: usize, screen: (u32, u32)) -> Result<(), String> {
        let Some(request) = &self.request else { return Err("no picture settings".into()) };
        if width == 0 || height == 0 || stride < width as usize * 4 || pixels.len() < stride * (height as usize - 1) + width as usize * 4 {
            return Err(format!("a picture of {width}x{height}, {stride} bytes per row, {} bytes", pixels.len()));
        }
        let frame = Frame { width, height, pitch: stride, format: crate::libretro::PIXEL_XRGB8888, data: pixels.to_vec() };
        self.renderer.upload(&frame);
        self.renderer.draw(screen.0, screen.1, request.aspect, &self.settings);
        Ok(())
    }

    /// The last picture with the current settings at `w` by `h`, as the screen would show it (RGBA, top row first), for the pause menu's
    /// background. None without settings.
    pub fn shot(&mut self, w: u32, h: u32) -> Option<Vec<u8>> {
        let aspect = self.request.as_ref()?.aspect;
        Some(self.renderer.render_offscreen(w, h, aspect, &self.settings))
    }

    /// Where the game sits in a `shot` of `w` by `h` (without borders or the screen frame), as pocket-core writes it before its own
    /// menu shot, so Zoom crops to the game (`preview::game_rect_image`). None without settings or a picture.
    pub fn game_rect(&self, w: u32, h: u32) -> Option<crate::preview::GameRect> {
        let aspect = self.request.as_ref()?.aspect;
        self.renderer.picture(w, h, aspect, &self.settings).map(|p| (p.x as u32, p.y as u32, p.w as u32, p.h as u32))
    }

    /// The frame around the picture is still being prepared on a thread: the next picture draws it.
    pub fn frame_pending(&self) -> bool {
        self.renderer.frame_pending()
    }

    /// The last picture as it was drawn, for the pause menu's preview and tests (RGBA, top row first).
    pub fn renderer(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    pub fn frames_dir(&self) -> Option<&Path> {
        self.request.as_ref().map(|r| r.frames_dir.as_path())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_overlay_scene_reads_back_and_skips_keys_it_does_not_know() {
        let texts = ["59.9 FPS · 41 °C".to_string(), ">> 2x".to_string(), String::new(), "vol 40".to_string()];
        let c = overlay_command(&texts);
        assert!(c.starts_with("pocket_overlay fps=") && !c.contains("toast="), "{c}");
        assert_eq!(read_overlay(c.trim_start_matches("pocket_overlay").trim()), Some(texts.clone()));
        assert_eq!(read_overlay(&format!("{} card=00", c.trim_start_matches("pocket_overlay").trim())), Some(texts), "a later part is skipped");
        assert_eq!(read_overlay(""), Some(Default::default()), "nothing over the game");
        assert_eq!(read_overlay("fps=zz"), None);
    }
    use super::*;
    use crate::libretro::{PIXEL_0RGB1555, PIXEL_RGB565, PIXEL_XRGB8888};

    fn request() -> Request {
        let values = [("effect", "crt"), ("sharpness", "crisp"), ("frame", "brick"), ("frame_brick", "clear"), ("scaling", "aspect")].map(|(k, v)| (k.to_string(), v.to_string())).into();
        Request { system: "gba".into(), aspect: 1.5, turns: 1, frames_dir: PathBuf::from("/mnt/SD CARD/.pocket-ui/frames"), values }
    }

    #[test]
    fn the_command_reads_back_with_a_path_that_has_a_space() {
        let r = request();
        let c = r.command();
        assert!(c.starts_with("pocket_picture "));
        assert_eq!(Request::read(c.trim_start_matches("pocket_picture")), Some(r));
    }

    #[test]
    fn what_does_not_read_is_none() {
        assert_eq!(Request::read(""), None);
        assert_eq!(Request::read("6762 x 0 00"), None);
        assert_eq!(Request::read("6762 1.5 0 00 effect"), None);
        let r = Request::read("6762 nan 9 00").unwrap();
        assert_eq!((r.aspect, r.turns), (0.0, 1));
    }

    #[test]
    fn the_settings_arrive_as_the_menu_wrote_them() {
        use crate::video::{Effect, FrameMode, Scaling, Sharpness};
        let s = request().settings();
        assert_eq!((s.effect, s.sharpness, s.frame, s.scaling), (Effect::Crt, Sharpness::Crisp, FrameMode::Brick, Scaling::Aspect));
    }

    #[test]
    fn no_choice_of_the_picture_group_has_a_space_in_it() {
        for g in crate::options::GROUPS.iter().filter(|g| g.name == "Picture") {
            for s in g.settings {
                assert!(!s.key.contains(char::is_whitespace) && !s.key.contains('='), "{}", s.key);
                for c in s.choices {
                    assert!(!c.value.contains(char::is_whitespace), "{}={}", s.key, c.value);
                }
            }
        }
        assert!(keys().contains(&"effect") && keys().contains(&"frame"));
    }

    #[test]
    fn a_565_picture_reaches_the_bridge_as_xrgb_at_its_own_size() {
        let mut data = 0xffffu16.to_le_bytes().to_vec();
        data.extend_from_slice(&0xf800u16.to_le_bytes());
        let f = Frame { width: 2, height: 1, pitch: 4, format: PIXEL_RGB565, data };
        let mut out = Vec::new();
        assert_eq!(to_xrgb(&f, &mut out), (2, 1));
        assert_eq!(out, vec![255, 255, 255, 0, 0, 0, 255, 0]);
        let g = Frame { width: 1, height: 1, pitch: 2, format: PIXEL_0RGB1555, data: 0x001fu16.to_le_bytes().to_vec() };
        to_xrgb(&g, &mut out);
        assert_eq!(out, vec![255, 0, 0, 0]);
    }

    #[test]
    fn an_xrgb_picture_with_padding_loses_it_and_a_short_one_does_not_panic() {
        let f = Frame { width: 1, height: 2, pitch: 8, format: PIXEL_XRGB8888, data: vec![1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0, 9, 9, 9, 9] };
        let mut out = Vec::new();
        to_xrgb(&f, &mut out);
        assert_eq!(out, vec![1, 2, 3, 0, 4, 5, 6, 0]);
        let short = Frame { width: 4, height: 4, pitch: 16, format: PIXEL_XRGB8888, data: vec![1; 20] };
        assert_eq!(to_xrgb(&short, &mut out), (4, 4));
    }
}
