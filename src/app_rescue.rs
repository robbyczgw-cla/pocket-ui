//! Part of the pocket-ui binary: safe mode and the dialog before EmulationStation. The logic (exit codes, what the rows
//! do, the reset, the warning) is `pocket_ui::rescue`; this paints it and drives it.
//!
//! Safe mode (`pocket-ui --safe`, started by run-pocket-ui.sh after a crash loop) is its own small event loop with its
//! own state, not the App with parts switched off: it reads no game list, decodes no picture (covers, frames, previews),
//! opens no Curator index, starts no achievements, sync, store, Wi-Fi watchdog, cover or search threads, uses the
//! built-in type and no theme, and reads only the appearance from settings.json (defaults if that fails). What it shares
//! with the normal start is the window, the type, the pads and the touch screen: the least a page needs.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::palette::*;
use pocket_ui::rescue::{self, Action, Dialog, DialogKind, DialogOut, RescuePage, WhatHappened};
use pocket_ui::text::{Face, Text};
use pocket_ui::touch::{self, Area, Gesture, HitMap, TapAction};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use super::{App, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

// ---- painting, shared by safe mode and the App ------------------------------------------------------------------

/// Button hints in a dark pill at the bottom right (the App's `nu_hints`, and safe mode's); returns where the pill
/// starts (`kit::pill_hints`).
pub(crate) fn paint_hints(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, hints: &[(&str, &str)]) -> i32 {
    pocket_ui::kit::pill_hints(c, text, hits, hints)
}

/// A list row in the Settings style: the band across, the name on a pill when selected. Returns its text baseline
/// (`kit::row_label`).
pub(crate) fn paint_row(c: &mut Canvas, text: &mut Text, rect: (i32, i32, i32, i32), font: u32, label: &str, selected: bool) -> i32 {
    pocket_ui::kit::row_label(c, text, rect, font, label, selected)
}

/// A dialog on a sheet over the dimmed page: the title, what to know (warnings with a mark in the alert colour), the
/// choices as rows (Cancel first), and its hints. Only the dialog can be touched while it is open.
pub(crate) fn paint_dialog(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, d: &Dialog) {
    hits.clear();
    c.shade_rect(0, 0, c.w, c.h, pal().void, 215);
    // For the snapshot checks of Settings → Text size: the page under the dialog is dimmed, not read.
    pocket_ui::text::boxes::covered();
    let (w, row, font, small) = (980, pocket_ui::type_scale::row(66, 36), 36u32, NU_SMALL);
    let x = (c.w - w) / 2;
    let inner = w - 2 * 48;
    // Lay out first, to centre the sheet.
    let paragraphs: Vec<Vec<String>> = d.body().iter().map(|p| text.wrap(Face::Body, small, p, inner - 28)).collect();
    let line_h = pocket_ui::type_scale::line(small, 12);
    let body_h: i32 = paragraphs.iter().map(|p| p.len() as i32 * line_h + 14).sum();
    let choices = d.choices();
    let h = 48 + 58 + 26 + body_h + 16 + choices.len() as i32 * row + 36;
    let y = ((c.h - NU_PILL - NU_PAD - h) / 2).max(NU_PAD);
    // A sheet like Curator's and the game card's pickers (`kit::sheet`).
    pocket_ui::kit::sheet(c, x, y, w, h);
    let left = x + 48;
    let mut ty = y + 48 + 46;
    text.draw(c, Face::Title, 50, left, ty, pal().chalk, d.title());
    ty += 26;
    let warn = matches!(d.kind, DialogKind::Leave | DialogKind::OpenEs);
    for (i, lines) in paragraphs.iter().enumerate() {
        let is_warning = warn && i > 0;
        if is_warning {
            c.round_rect(left, ty + 10, 8, lines.len() as i32 * line_h - 4, 4, pal().alert);
        }
        let colour = if i == 0 { pal().quiet } else { pal().text };
        for line in lines {
            ty += line_h;
            text.draw(c, Face::Body, small, left + 28, ty, colour, line);
        }
        ty += 14;
    }
    ty += 16;
    for (i, choice) in choices.iter().enumerate() {
        let ry = ty + i as i32 * row;
        paint_row(c, text, (left, ry, inner, row), font, choice.label(), i == d.sel);
        hits.row(Area::new(left, ry, inner, row), i);
    }
    hits.list(d.sel, choices.len(), 0.0, row as f32);
    // The page's own hints go, as under Curator's sheets: only the dialog's are left to read and to touch.
    c.rect(0, c.h - NU_PAD - NU_PILL - 12, c.w, NU_PAD + NU_PILL + 12, pal().void);
    paint_hints(c, text, hits, &[("A", "Choose"), ("B", "Cancel")]);
}

/// The rescue page.
fn paint_page(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, page: &RescuePage) {
    hits.clear();
    c.clear(pal().void);
    let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
    let (left, right) = (pad + NU_BTN_PAD, c.w - pad - NU_BTN_PAD);
    let width = right - left;
    text.draw(c, Face::Title, 50, left, pad + 58, pal().chalk, "Safe mode");
    // On two lines when it is wider than the page (German, larger text); the panel moves down with it.
    let sub = text.wrap(Face::Body, NU_SMALL, "pocket-ui started with only what this page needs. Games and saves are untouched.", width);
    let sub_step = pocket_ui::type_scale::line(NU_SMALL, 12);
    for (k, l) in sub.iter().take(2).enumerate() {
        text.draw(c, Face::Body, NU_SMALL, left, pad + 58 + 48 + k as i32 * sub_step, pal().quiet, l);
    }
    let sub_extra = (sub.len().clamp(1, 2) as i32 - 1) * sub_step + pocket_ui::type_scale::grow(NU_SMALL);

    // What happened, in a panel.
    let w = &page.what;
    let mut lines: Vec<(String, u32, u32)> = Vec::new();
    for l in text.wrap(Face::Body, NU_SMALL, &w.headline(), width - 2 * 28).into_iter().take(2) {
        lines.push((l, NU_SMALL, pal().text));
    }
    if let Some(crash) = &w.last_crash {
        for l in text.wrap(Face::Body, 24, &format!("Last crash: {crash}"), width - 2 * 28).into_iter().take(2) {
            lines.push((l, 24, pal().quiet));
        }
    }
    for l in &w.launcher {
        lines.push((text.fit(Face::Body, 24, l, width - 2 * 28), 24, pal().dust));
    }
    let head = 46;
    let panel_h = head + lines.iter().map(|(_, px, _)| pocket_ui::type_scale::line(*px, 12)).sum::<i32>() + 20;
    let panel_y = pad + 58 + 48 + 30 + sub_extra;
    pocket_ui::kit::panel(c, pad, panel_y, c.w - 2 * pad, panel_h);
    text.draw(c, Face::Body, 24, left, panel_y + 36, pal().dust, "What happened");
    let mut y = panel_y + head;
    for (l, px, colour) in &lines {
        y += pocket_ui::type_scale::line(*px, 12);
        text.draw(c, Face::Body, *px, left, y, *colour, l);
    }

    // The choices.
    let top = panel_y + panel_h + 24;
    let rows = page.rows();
    for (i, r) in rows.iter().enumerate() {
        let ry = top + i as i32 * row;
        paint_row(c, text, (pad, ry, c.w - 2 * pad, row), font, &page.label(*r), i == page.sel);
        hits.row(Area::new(pad, ry, c.w - 2 * pad, row), i);
    }
    hits.list(page.sel, rows.len(), 0.0, row as f32);
    // Above the hints; with larger text a paragraph that reaches them ends with "…".
    let (step, bottom) = (pocket_ui::type_scale::line(NU_SMALL, 12), c.h - pad - NU_PILL - 16);
    let mut y = top + rows.len() as i32 * row + 44;
    // What was just done (the message) keeps its lines; the row's explanation gives way first.
    let message_h = page.message.as_ref().map_or(0, |m| text.wrap(Face::Body, NU_SMALL, m, width).len() as i32 * step + 12);
    y = text.paragraph(c, Face::Body, NU_SMALL, (left, y), width, step, bottom - message_h, pal().quiet, page.describe(page.row()));
    if let Some(m) = &page.message {
        text.paragraph(c, Face::Body, NU_SMALL, (left, y + 12), width, step, bottom, pal().text, m);
    }
    paint_hints(c, text, hits, &[("A", "Choose")]);
    if let Some(d) = &page.dialog {
        paint_dialog(c, text, hits, d);
    }
}

// ---- the App: the dialog before EmulationStation ----------------------------------------------------------------

impl App {
    /// SELECT + START held, Escape, Settings → About: the dialog instead of leaving at once.
    pub(crate) fn open_leave(&mut self, kind: DialogKind) {
        self.exit_since = None;
        // Not behind a running game: the pause menu has the pad then (the FIFO's `leave` could ask at any time).
        if self.running.is_some() {
            return;
        }
        self.select_alone = false;
        self.start_tap.disarm();
        self.leave = Some(Dialog::new(kind));
        self.request_redraw();
    }

    /// The open dialog takes every press (and the releases of what was held to open it). True when it did.
    pub(crate) fn leave_input(&mut self, el: &ActiveEventLoop, pe: &PadEvent) -> bool {
        // The volume keys and the power button keep their meaning.
        if matches!(pe, PadEvent::Button { button: Button::VolUp | Button::VolDown | Button::Power, .. }) {
            return false;
        }
        let Some(d) = self.leave.as_mut() else { return false };
        // `self.pad` has the event already (`user_event`).
        let PadEvent::Button { button, pressed: true, .. } = *pe else { return true };
        match d.press(button, CONFIRM, CANCEL) {
            DialogOut::Stay => {}
            DialogOut::Close => self.leave = None,
            DialogOut::Chose(c) => {
                self.leave = None;
                // Report a problem: Settings → System → About, where the sheet says where the file went.
                if c == rescue::Choice::Report {
                    self.settings_page = self.open_about();
                    self.write_report();
                    self.request_redraw();
                    return true;
                }
                if let Some(code) = rescue::exit_code(c) {
                    println!("rescue: leaving pocket-ui ({c:?}, exit {code})");
                    self.exit_code = code;
                    el.exit();
                }
            }
        }
        self.request_redraw();
        true
    }

    /// Over everything else, last.
    pub(crate) fn paint_leave(&mut self, c: &mut Canvas) {
        if let Some(d) = self.leave.clone() {
            paint_dialog(c, &mut self.text, &mut self.hits, &d);
        }
    }

    /// `--snapshot leave[:<row>]` (Home with the dialog of SELECT + START), `about-es[:<row>]` (Settings → About with
    /// the dialog of its row).
    pub(crate) fn leave_snapshot(&mut self, state: &str) -> bool {
        let (name, row) = state.split_once(':').unwrap_or((state, "0"));
        let kind = match name {
            "leave" => DialogKind::Leave,
            "about-es" => {
                self.settings_page = Some(super::SettingsPage::About);
                DialogKind::OpenEs
            }
            _ => return false,
        };
        let mut d = Dialog::new(kind);
        d.sel = row.parse::<usize>().unwrap_or(0).min(d.choices().len() - 1);
        self.leave = Some(d);
        true
    }
}

// ---- safe mode --------------------------------------------------------------------------------------------------

/// The safe-mode app: the page, its window and nothing else.
struct Rescue {
    window: Option<Rc<Window>>,
    surface: pocket_ui::frame::Presenter<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    text: Text,
    hits: HitMap,
    touch_rec: touch::Recognizer,
    page: RescuePage,
    light: bool,
    data_dir: PathBuf,
    exit_code: i32,
    /// "Switching off…" is on screen; input does nothing more.
    switching_off: bool,
    drew_first: bool,
}

impl Rescue {
    fn new(data_dir: PathBuf, page: RescuePage, light: bool) -> Rescue {
        Rescue {
            window: None,
            surface: pocket_ui::frame::Presenter::empty(),
            text: Text::builtin(),
            hits: HitMap::default(),
            touch_rec: Default::default(),
            page,
            light,
            data_dir,
            exit_code: rescue::RESTART_CODE,
            switching_off: false,
            drew_first: false,
        }
    }

    fn paint(&mut self, c: &mut Canvas) {
        pocket_ui::palette::set_blend(if self.light { 255 } else { 0 });
        if self.switching_off {
            c.clear(pal().void);
            let msg = "Switching off\u{2026}";
            let w = self.text.width(Face::Body, 46, msg);
            self.text.draw(c, Face::Body, 46, (c.w - w) / 2, c.h / 2, pal().text, msg);
            return;
        }
        paint_page(c, &mut self.text, &mut self.hits, &self.page);
    }

    fn press(&mut self, el: &ActiveEventLoop, b: Button) {
        if self.switching_off {
            return;
        }
        match self.page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Exit(code) => {
                println!("rescue: safe mode ends with exit {code}");
                self.exit_code = code;
                el.exit();
            }
            Action::ResetSettings => {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let result = rescue::reset_settings(&self.data_dir, &rescue::stamp(now)).map_err(|e| e.to_string());
                println!("rescue: reset pocket-ui settings: {result:?}");
                self.page.reset_finished(result);
            }
            Action::PowerOff => {
                self.switching_off = true;
                let cmd = std::env::var("POCKETUI_POWEROFF_CMD").unwrap_or_else(|_| "systemctl poweroff".into());
                println!("rescue: power off ({cmd})");
                // Like every other way to power off: no wake alarm left armed (`auto_off::disarm_before_power_off`).
                let rtc = pocket_ui::auto_off::Rtc::from_env();
                pocket_ui::auto_off::disarm_before_power_off(&rtc, &self.data_dir, "rescue PowerOff");
                pocket_ui::shutdown_log::event(&self.data_dir, &format!("rescue PowerOff: running `{cmd}`"));
                if let Err(e) = std::process::Command::new("sh").args(["-c", &cmd]).spawn() {
                    rtc.reopen();
                    eprintln!("rescue: couldn't power off: {e}");
                    self.switching_off = false;
                    self.page.message = Some(format!("Couldn't switch off: {e}"));
                }
            }
        }
        self.redraw();
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(window) = self.window.clone() else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        let mut surface = std::mem::replace(&mut self.surface, pocket_ui::frame::Presenter::empty());
        let drawn = surface.draw_scaled(w, h, &mut |pixels, lw, lh| self.paint(&mut Canvas::new(pixels, lw, lh)));
        self.surface = surface;
        if drawn && !self.drew_first {
            self.drew_first = true;
            // The launcher's black screen guard: this start shows a picture.
            rescue::note_first_frame();
            println!("rescue: safe mode is on screen");
        }
    }
}

impl ApplicationHandler<UserEvent> for Rescue {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("pocket-ui (safe mode)").with_fullscreen(Some(Fullscreen::Borderless(None)));
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs.with_name(pocket_ui::control::OWN_APP_ID, pocket_ui::control::OWN_APP_ID)
        };
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("rescue: couldn't create the window: {e}");
                self.exit_code = 1;
                el.exit();
                return;
            }
        };
        let for_surface = window.clone();
        self.surface.set_factory(move || {
            let context = softbuffer::Context::new(for_surface.clone()).map_err(|e| format!("context: {e}"))?;
            softbuffer::Surface::new(&context, for_surface.clone()).map_err(|e| format!("surface: {e}"))
        });
        self.window = Some(window);
        self.redraw();
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        match ev {
            UserEvent::Pad(PadEvent::Button { source, button, pressed: true }) => {
                // The FIFO already speaks roles; only the physical pads need the printed layout.
                self.press(el, if source == usize::MAX { button } else { pocket_ui::device::current().menu_layout().role(button) });
            }
            UserEvent::Touch(t) => {
                let Some(window) = &self.window else { return };
                let size = window.inner_size();
                let (w, h) = pocket_ui::scaled_layout::logical_size((size.width, size.height));
                let (x, y) = touch::to_px((t.x, t.y), w, h);
                match self.touch_rec.feed(t.phase, x, y, std::time::Instant::now(), self.hits.pitch()) {
                    Some(Gesture::Tap { x, y }) => match self.hits.tap(x, y) {
                        TapAction::Nothing => {}
                        TapAction::Activate => self.press(el, CONFIRM),
                        TapAction::Press(b) => self.press(el, b),
                        TapAction::Presses(b, n) => (0..n).for_each(|_| self.press(el, b)),
                        TapAction::Steps(n) => {
                            for _ in 0..n.unsigned_abs() {
                                self.press(el, if n < 0 { Button::Up } else { Button::Down });
                            }
                        }
                    },
                    Some(Gesture::Scroll { .. }) | None => {}
                }
            }
            // The command FIFO's quit: a restart, as in the App.
            UserEvent::Quit => el.exit(),
            _ => {}
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ElementState::Pressed, repeat: false, .. }, .. } => {
                let button = match code {
                    KeyCode::ArrowUp => Button::Up,
                    KeyCode::ArrowDown => Button::Down,
                    KeyCode::ArrowLeft => Button::Left,
                    KeyCode::ArrowRight => Button::Right,
                    KeyCode::Enter => CONFIRM,
                    KeyCode::Backspace | KeyCode::Escape => CANCEL,
                    _ => return,
                };
                self.press(el, button);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // A frame that failed is tried again (`frame::Presenter`); otherwise the page waits for input.
        match self.surface.retry_at() {
            Some(at) => {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(at));
                self.redraw();
            }
            None => el.set_control_flow(winit::event_loop::ControlFlow::Wait),
        }
    }
}

/// The rescue page as it opens for `data_dir`, with the launcher's reason.
fn page_for(data_dir: &Path, reason: Option<String>) -> RescuePage {
    let log = std::env::var_os("POCKETUI_LOG").map_or_else(|| PathBuf::from("/var/log/pocket-ui.log"), PathBuf::from);
    let what = WhatHappened::read(reason, data_dir, &log);
    let root = Path::new(pocket_ui::update::APP_ROOT);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let backup = rescue::backup_dir(data_dir, &rescue::stamp(now)).display().to_string();
    RescuePage::new(what, rescue::other_version(root), backup)
}

/// Light or dark as settings.json says (Automatic follows the clock); dark when it can't be read.
fn wants_light(data_dir: &Path) -> bool {
    pocket_ui::appearance::wants_light_now(pocket_ui::settings::Settings::load(data_dir).appearance)
}

/// `pocket-ui --safe`: the rescue page until a choice ends it; the exit code tells the launcher what next.
pub(crate) fn run(data_dir: PathBuf) -> ! {
    println!("rescue: safe mode ({})", std::env::var(rescue::REASON_ENV).unwrap_or_else(|_| "started by hand".into()));
    pocket_ui::crash::install("pocket-ui", data_dir.clone(), None);
    let page = page_for(&data_dir, std::env::var(rescue::REASON_ENV).ok());
    let light = wants_light(&data_dir);
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("rescue: no event loop: {e}");
            std::process::exit(1);
        }
    };
    #[cfg(target_os = "linux")]
    {
        let proxy: winit::event_loop::EventLoopProxy<UserEvent> = event_loop.create_proxy();
        std::thread::spawn(move || {
            {
                let proxy = proxy.clone();
                pocket_ui::pad::linux::supervise(move |e| !matches!(e, PadEvent::Button { .. }) || proxy.send_event(UserEvent::Pad(e)).is_ok());
            }
            super::spawn_command_fifo(proxy.clone());
            if let Some((_, dev)) = pocket_ui::touch::linux::open() {
                pocket_ui::touch::linux::pump(dev, true, move |t| proxy.send_event(UserEvent::Touch(t)).is_ok());
            }
        });
    }
    let mut app = Rescue::new(data_dir, page, light);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("rescue: {e}");
        std::process::exit(1);
    }
    if app.exit_code == rescue::OPEN_ES_CODE {
        rescue::before_es();
    }
    std::process::exit(app.exit_code);
}

/// `pocket-ui --safe --snapshot rescue[:<row>][:<reset|es>[:<choice>]][:done] out.png`: the rescue page with a
/// made-up reason, crash and launcher lines (and a previous version), the row selected, a dialog open.
pub(crate) fn snapshot(data_dir: PathBuf, args: &[String]) {
    let (Some(state), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("usage: pocket-ui --safe --snapshot rescue[:<row>][:<reset|es>[:<choice>]][:done] <out.png>");
        return;
    };
    let mut parts = state.split(':').skip(1);
    let what = WhatHappened {
        reason: Some("pocket-ui stopped 4 times in a row (exit 139).".into()),
        last_crash: Some("2026-10-23 07:41:12 UTC pocket-ui panicked at src/image.rs:212:9: attempt to multiply with overflow".into()),
        launcher: vec![
            "pocket-ui crashed (exit 134), restart 3 of 3 in 4 s".into(),
            "pocket-ui failed 4 times in a row (exit 134): starting it in safe mode".into(),
        ],
    };
    let mut page = RescuePage::new(what, Some("0.9.4".into()), rescue::backup_dir(&data_dir, "20261023-074130").display().to_string());
    page.sel = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0).min(page.rows().len() - 1);
    match parts.next() {
        Some("reset") => {
            let mut d = Dialog::new(DialogKind::Reset);
            d.backup = Some(page.backup.clone());
            d.sel = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0).min(1);
            page.dialog = Some(d);
        }
        Some("es") => {
            let mut d = Dialog::new(DialogKind::OpenEs);
            d.sel = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0).min(1);
            page.dialog = Some(d);
        }
        Some("done") => page.reset_finished(Ok(PathBuf::from(&page.backup))),
        _ => {}
    }
    let mut app = Rescue::new(data_dir.clone(), page, wants_light(&data_dir));
    let (w, h) = super::snapshot_dims();
    let mut buf = vec![0u32; (w * h) as usize];
    app.paint(&mut Canvas::new(&mut buf, w, h));
    super::write_png(&buf, w, h, out);
}
