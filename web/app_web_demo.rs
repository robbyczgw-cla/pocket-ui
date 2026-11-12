//! The browser demo's side of the App (docs/WEB-DEMO.md), compiled only for wasm32-unknown-emscripten: what the page
//! calls. The App runs on its own thread (PROXY_TO_PTHREAD); these functions run on the page's thread and only queue
//! events (the winit stand-in wakes the App) or copy the newest frame out.

use super::{UserEvent, CANCEL, CONFIRM};
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::touch::{Phase, TouchEvent};

/// The page's button numbers, in the order of web/demo/pad.js.
const BUTTONS: [Button; 15] = [
    Button::Up, Button::Down, Button::Left, Button::Right, CONFIRM, CANCEL, Button::North, Button::West,
    Button::L1, Button::R1, Button::L2, Button::R2, Button::Select, Button::Start, Button::Mode,
];

fn send(ev: UserEvent) {
    let _ = winit::web::proxy::<UserEvent>().send_event(ev);
}

/// A button of the keyboard or the on-screen pad went down (`pressed` 1) or up (0). As the command FIFO's: in the
/// roles the App reads (A confirms), not through the Button layout.
#[no_mangle]
pub extern "C" fn pocket_web_button(button: u32, pressed: u32) {
    if let Some(&button) = BUTTONS.get(button as usize) {
        send(UserEvent::Pad(PadEvent::Button { source: 0, button, pressed: pressed != 0 }));
    }
}

/// A finger (or the mouse) on the canvas: `phase` 0 down, 1 moved, 2 up; `x`, `y` fractions of the canvas, as the
/// touch screen sends them (`app_touch`).
#[no_mangle]
pub extern "C" fn pocket_web_touch(phase: u32, x: f32, y: f32) {
    let phase = match phase {
        0 => Phase::Down,
        1 => Phase::Move,
        _ => Phase::Up,
    };
    send(UserEvent::Touch(TouchEvent { phase, x: x.clamp(0.0, 1.0), y: y.clamp(0.0, 1.0) }));
}

/// The canvas's size in pixels: the App paints at it (a supported layout, `scaled_layout`).
#[no_mangle]
pub extern "C" fn pocket_web_resize(w: u32, h: u32) {
    winit::web::set_size(w.max(1), h.max(1));
}

/// The page became visible again: one fresh frame.
#[no_mangle]
pub extern "C" fn pocket_web_redraw() {
    winit::web::request_redraw();
}

/// For the page's measurements: since the last call, frames painted, their mean and their longest painting time in
/// microseconds, written to `out` (three u32).
#[no_mangle]
pub extern "C" fn pocket_web_paint_stats(out: *mut u32) {
    let (n, total, max) = winit::web::take_paint_stats();
    let mean = if n > 0 { total.as_micros() as u32 / n } else { 0 };
    // SAFETY: the page passes three u32 it allocated.
    unsafe { std::ptr::copy_nonoverlapping([n, mean, max.as_micros() as u32].as_ptr(), out, 3) };
}

/// The newest frame when there is one the page hasn't had: a pointer to [width, height, sequence, pixels pointer]
/// (u32 each; RGBA bytes, width * height * 4), else 0. Valid until the next call.
#[no_mangle]
pub extern "C" fn pocket_web_frame(seen: u32) -> *const u32 {
    use std::sync::Mutex;
    static FRONT: Mutex<(Vec<u8>, [u32; 4])> = Mutex::new((Vec::new(), [0; 4]));
    let mut front = FRONT.lock().unwrap_or_else(|p| p.into_inner());
    let (pixels, header) = &mut *front;
    match softbuffer::web::take_frame(seen as u64, pixels) {
        Some((w, h, seq)) => {
            *header = [w, h, seq as u32, pixels.as_ptr() as u32];
            header.as_ptr()
        }
        None => std::ptr::null(),
    }
}

// ---- The stand-in game -------------------------------------------------------------------------------------------
// No game runs in the browser. A start goes through the real launch (the case into the console, the TV coming on); then
// the page shows the card "would start here" (it reads `game=playing` in /tmp/pocket-ui.status). pocket-core's
// side is played by a thread here: the App writes its commands to /tmp/pocket-core.cmd as on a device (a device file the
// page makes, a stream like the FIFO: web/demo/page/pad.js); "shot" and "preview" get a screen saying where the game's
// picture would be, "save" leaves a slot, "quit" ends the game, and the shelf comes back as after a real one.

use std::sync::Mutex;
use std::time::{Duration, Instant};

const CMD: &str = "/tmp/pocket-core.cmd";

/// The "game's picture" (0RGB, the canvas's size): where a handheld shows the game, a plain screen saying so. It is the
/// picture behind the pause menu (`shot`), in Options → Picture's previews (`preview`) and under Zoom.
static PLACEHOLDER: Mutex<Option<pocket_ui::preview::Pixels>> = Mutex::new(None);
/// The card the page shows while the "game runs" (RGBA, the canvas's size), and its size and number.
static CARD: Mutex<(Vec<u8>, [u32; 4])> = Mutex::new((Vec::new(), [0; 4]));
/// The game that "runs": when it started, its system and file name without the extension.
static GAME: Mutex<Option<(Instant, String, String)>> = Mutex::new(None);
/// The card of the game that would run: a pointer to [width, height, number, pixels pointer] (u32 each; RGBA), or 0
/// before the first start. The number changes with each start; the page draws the pixels while `game=playing`.
#[no_mangle]
pub extern "C" fn pocket_web_card() -> *const u32 {
    let card = CARD.lock().unwrap_or_else(|p| p.into_inner());
    if card.0.is_empty() {
        return std::ptr::null();
    }
    // Static storage: the pointer stays valid after the guard is gone (only `spawn_game` writes, on the App's thread).
    card.1.as_ptr()
}

/// A console picture at one and a half times its size, nearest pixel (transparency kept: `blit_sprite` draws it).
fn enlarged(s: &pocket_ui::system_art::Sprite) -> pocket_ui::system_art::Sprite {
    let (w, h) = (s.w * 3 / 2, s.h * 3 / 2);
    let px = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| s.px[((y * 2 / 3) * s.w + x * 2 / 3) as usize]).collect();
    pocket_ui::system_art::Sprite { w, h, px }
}

/// The card in pocket-ui's own look: an empty-state page (`kit::empty_state`) with the page hint for HOME, in the
/// type, colours and appearance in use (Barlow, or Retro's pixel letters with Cocoa or Pea). Painted on the App's
/// thread (the palette's blend between dark and light is that thread's), at the layout's logical size, reduced to the
/// canvas as every frame is (`scaled_layout`). Also keeps the page without its words for the pause menu's background.
fn paint_card(system: &str, path: &std::path::Path) {
    use pocket_ui::gfx::Canvas;
    use pocket_ui::palette::{pal, MARGIN};
    thread_local! { static TEXT: std::cell::RefCell<Option<pocket_ui::text::Text>> = const { std::cell::RefCell::new(None) }; }
    let (w, h) = winit::web::size();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let name = pocket_ui::library::display_name(stem);
    let console = pocket_ui::consoles::for_system(system).map_or_else(|| system.to_string(), |c| c.name.clone());
    let mut scaled = pocket_ui::scaled_layout::ScaledFrame::default();
    let mut plain = vec![0u32; (w * h) as usize];
    let mut words = vec![0u32; (w * h) as usize];
    TEXT.with(|t| {
        let mut t = t.borrow_mut();
        let text = t.get_or_insert_with(pocket_ui::text::Text::new);
        scaled.render(&mut plain, (w, h), &mut |px, lw, lh| {
            use pocket_ui::text::Face;
            let mut c = Canvas::new(px, lw, lh);
            c.clear(pal().spine);
            let (lw, lh) = (lw as i32, lh as i32);
            let (top, bottom) = ("Game picture", "shows here on a handheld");
            let (top_w, bottom_w) = (text.width(Face::Title, 56, top), text.width(Face::Body, 34, bottom));
            text.draw(&mut c, Face::Title, 56, (lw - top_w) / 2, lh / 2 - 10, pal().dust, top);
            text.draw(&mut c, Face::Body, 34, (lw - bottom_w) / 2, lh / 2 + 42, pal().dust, bottom);
            // Once more on one line near the lower edge: Zoom starts there on handhelds (their dialogue boxes, `zoom`).
            let line = "Game picture shows here on a handheld";
            let line_w = text.width(Face::Body, 34, line);
            text.draw(&mut c, Face::Body, 34, (lw - line_w) / 2, lh - 90, pal().dust, line);
        });
        scaled.render(&mut words, (w, h), &mut |px, lw, lh| {
            let mut c = Canvas::new(px, lw, lh);
            c.clear(pal().void);
            let (lw, lh) = (lw as i32, lh as i32);
            // The console on the right, half as big again (the shelf's console art, `system_art`).
            let words_w = match pocket_ui::system_art::get(system) {
                Some(art) => {
                    let big = enlarged(art);
                    let x = lw - MARGIN - big.w as i32;
                    c.blit_sprite(&big, x, (lh - big.h as i32) / 2 - 20, 0);
                    x - MARGIN
                }
                None => lw - 2 * MARGIN,
            };
            text.draw(&mut c, pocket_ui::text::Face::Body, 21, MARGIN, 64, pal().dust, &console);
            let line = "would start here. On a handheld you would be playing now. No games run in the browser.";
            pocket_ui::kit::empty_state(&mut c, text, None, (MARGIN, words_w), lh / 5, lh * 2 / 5, lh - 110, &name, line);
            pocket_ui::kit::page_hints(&mut c, text, &[("HOME", "Pause menu")]);
        });
    });
    *PLACEHOLDER.lock().unwrap_or_else(|p| p.into_inner()) = Some(pocket_ui::preview::Pixels { w, h, px: plain });
    let mut card = CARD.lock().unwrap_or_else(|p| p.into_inner());
    let (rgba, header) = &mut *card;
    rgba.clear();
    rgba.extend(words.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]));
    *header = [w, h, header[2] + 1, rgba.as_ptr() as u32];
}

/// The web build's `spawn_game` (main.rs has the device's): nothing starts; the stand-in answers pocket-core's commands
/// until Quit.
pub(crate) fn spawn_game(proxy: winit::event_loop::EventLoopProxy<UserEvent>, run: &super::Run, _env: &[(&str, String)]) -> Option<i32> {
    let (system, path) = (run.system.as_str(), run.path.as_path());
    println!("web demo: {system} {} would start now on {} (core {})", path.display(), run.player.name(), run.core);
    // What waited in the stream from before (an OSD line between games) is not this game's.
    let _ = std::fs::read_to_string(CMD);
    paint_card(system, path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    *GAME.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), system.to_string(), stem));
    std::thread::spawn(move || stand_in(proxy));
    None
}

/// Apps (PC streaming, tools) don't start in the browser either: they end at once, as a failed start.
pub(crate) fn spawn_app(proxy: winit::event_loop::EventLoopProxy<UserEvent>, _cmd: std::process::Command) -> Option<i32> {
    let _ = proxy.send_event(UserEvent::GameExited { secs: 0, ok: false, pgid: None, start: Default::default(), moments: false });
    None
}

/// How long a "game" may take to end once pocket-ui asked it to by any way pocket-core isn't told (RetroArch's quit,
/// a standalone emulator's, closing its window): after that it has "saved and closed", as a game does.
const QUIT_WITHIN: Duration = Duration::from_millis(700);

/// The stand-in game ends: the App hears it as a game that saved and closed.
fn finish(proxy: &winit::event_loop::EventLoopProxy<UserEvent>) {
    let started = GAME.lock().unwrap_or_else(|p| p.into_inner()).take();
    let secs = started.map_or(0, |g| g.0.elapsed().as_secs());
    let _ = proxy.send_event(UserEvent::GameExited { secs, ok: true, pgid: None, start: Default::default(), moments: false });
}

/// pocket-core's `save <n>`: a slot as the core leaves one, in the page's memory: the state file (one line saying
/// there is no game in it) and its picture, the card, so the pause menu's Load shows the slot as saved.
fn save_slot(n: usize) {
    let Some((_, system, stem)) = GAME.lock().unwrap_or_else(|p| p.into_inner()).clone() else { return };
    let slot = n.saturating_sub(1);
    let base = pocket_ui::slots::base(&system, &stem);
    let state = pocket_ui::slots::state_path(&base, slot);
    if let Some(dir) = state.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&state, "pocket-ui browser demo: no game, so no moment of it is in here.\n");
    let card = CARD.lock().unwrap_or_else(|p| p.into_inner());
    let (rgba, [w, h, ..]) = (&card.0, card.1);
    if !rgba.is_empty() {
        let mut png = Vec::new();
        let mut enc = png::Encoder::new(&mut png, w, h);
        enc.set_color(png::ColorType::Rgba);
        if enc.write_header().and_then(|mut wr| wr.write_image_data(rgba)).is_ok() {
            let _ = std::fs::write(pocket_ui::slots::preview_path(&state), &png);
        }
    }
    pocket_ui::slots::set_current(std::path::Path::new("/storage/.config/pocket-ui"), &system, &stem, slot);
}

fn stand_in(proxy: winit::event_loop::EventLoopProxy<UserEvent>) {
    let mut quitting_since: Option<Instant> = None;
    let mut looked = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(10));
        // Everything written since the last look; reading takes it out of the stream.
        let text = std::fs::read_to_string(CMD).unwrap_or_default();
        for cmd in text.lines().map(str::trim) {
            let mut words = cmd.split_whitespace();
            match words.next().unwrap_or_default() {
                "shot" => {
                    let seq = words.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let (w, h) = winit::web::size();
                    let picture = PLACEHOLDER.lock().unwrap_or_else(|p| p.into_inner()).clone()
                        .unwrap_or_else(|| pocket_ui::preview::Pixels { w, h, px: vec![0x101418; (w * h) as usize] });
                    let _ = pocket_ui::preview::write(std::path::Path::new(pocket_ui::preview::SHOT_FILE), seq, &[&picture]);
                }
                "save" => save_slot(words.next().and_then(|s| s.parse().ok()).unwrap_or(1)),
                // Options → Picture's two previews: the detail at its size and the whole screen (`preview::Live`).
                "preview" => {
                    let n: Vec<u32> = words.filter_map(|w| w.parse().ok()).collect();
                    if let ([dw, dh, seq], Some(p)) = (n.as_slice(), PLACEHOLDER.lock().unwrap_or_else(|p| p.into_inner()).clone()) {
                        let rgba: Vec<u8> = p.px.iter().flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]).collect();
                        let (detail, whole) = pocket_ui::preview::from_screen(&rgba, p.w, p.h, *dw, *dh);
                        let _ = pocket_ui::preview::write(std::path::Path::new(pocket_ui::preview::FILE), *seq, &[&detail, &whole]);
                    }
                }
                w if w.starts_with("quit") => return finish(&proxy),
                // load, restart, resume, pause, set, reload, cheats, osd, slot, …: the game would just go on.
                _ => {}
            }
        }
        // Any other way of ending a game (RetroArch, a standalone emulator, its window closed) shows as `game=quitting`.
        if looked.elapsed() >= Duration::from_millis(100) {
            looked = Instant::now();
            let quitting = std::fs::read_to_string(pocket_ui::ui_status::FILE).is_ok_and(|s| s.contains("game=quitting"));
            match (quitting, quitting_since) {
                (true, None) => quitting_since = Some(Instant::now()),
                (true, Some(t)) if t.elapsed() >= QUIT_WITHIN => return finish(&proxy),
                (false, _) => quitting_since = None,
                _ => {}
            }
        }
    }
}
