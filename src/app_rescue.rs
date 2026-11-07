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
    // On a screen too short for all of it (1024x768: Leave's four paragraphs and four choices) the dialog
    // tightens its spacing and shows a window of the choices; the text size steps down for it only when even that
    // leaves less than three choices, and goes back after.
    let size = pocket_ui::type_scale::get();
    let plan = plan_dialog(c, text, d, size);
    pocket_ui::type_scale::set(plan.size);
    paint_dialog_planned(c, text, hits, d, &plan);
    pocket_ui::type_scale::set(size);
    // The page's own hints go, as under Curator's sheets: only the dialog's are left to read and to touch.
    c.rect(0, c.h - NU_PAD - NU_PILL - 12, c.w, NU_PAD + NU_PILL + 12, pal().void);
    paint_hints(c, text, hits, &[("A", "Choose"), ("B", "Cancel")]);
}

/// How a dialog is laid out: the spacing (`tight` or the design's), the choices shown, the text size.
struct DialogPlan {
    size: pocket_ui::type_scale::TextSize,
    tight: bool,
    w: i32,
    paragraphs: Vec<Vec<String>>,
    row: i32,
    small: u32,
    line_h: i32,
    /// The sheet's height, with only `visible` choices.
    h: i32,
    visible: usize,
}

/// (top padding, title to body, after a paragraph, body to choices, below the choices), the design's and the tight ones.
const DIALOG_GAPS: [(i32, i32, i32, i32, i32); 2] = [(48, 26, 14, 16, 36), (32, 16, 8, 10, 24)];

fn plan_dialog(c: &Canvas, text: &mut Text, d: &Dialog, current: pocket_ui::type_scale::TextSize) -> DialogPlan {
    use pocket_ui::type_scale::TextSize;
    let avail = c.h - NU_PILL - 2 * NU_PAD;
    let n = d.choices().len();
    let plan = |text: &mut Text, size: TextSize, tight: bool, w: i32| {
        pocket_ui::type_scale::set(size);
        let g = DIALOG_GAPS[tight as usize];
        let (row, small) = if tight { (pocket_ui::type_scale::row(56, 36), 26u32) } else { (pocket_ui::type_scale::row(66, 36), NU_SMALL) };
        let line_h = pocket_ui::type_scale::line(small, if tight { 6 } else { 12 });
        let paragraphs: Vec<Vec<String>> = d.body().iter().map(|p| text.wrap_prose(Face::Body, small, p, w - 2 * 48 - 28)).collect();
        let body_h: i32 = paragraphs.iter().map(|p| p.len() as i32 * line_h + g.2).sum();
        let fixed = g.0 + 58 + g.1 + body_h + g.3 + g.4;
        let visible = (((avail - fixed) / row).max(1) as usize).min(n);
        DialogPlan { size, tight, w, paragraphs, row, small, line_h, h: fixed + visible as i32 * row, visible }
    };
    // 980 wide, or as wide as the screen when its words need more lines than the room above the hints takes (640x480
    // with Large text in a wider font).
    let mut w = 980.min(c.w - 2 * NU_PAD);
    let mut p = plan(text, current, false, w);
    if p.visible < n && c.w - 2 * NU_PAD > w {
        w = c.w - 2 * NU_PAD;
        p = plan(text, current, false, w);
    }
    if p.visible == n {
        return p;
    }
    let sizes: &[TextSize] = match current {
        TextSize::Large => &[TextSize::Large, TextSize::Medium, TextSize::Normal],
        TextSize::Medium => &[TextSize::Medium, TextSize::Normal],
        TextSize::Normal => &[TextSize::Normal],
    };
    let mut best = None;
    for &size in sizes {
        let t = plan(text, size, true, w);
        let ok = t.visible >= n.min(3);
        best = Some(t);
        if ok {
            break;
        }
    }
    pocket_ui::type_scale::set(current);
    best.unwrap_or(p)
}

fn paint_dialog_planned(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, d: &Dialog, p: &DialogPlan) {
    let choices = d.choices();
    let g = DIALOG_GAPS[p.tight as usize];
    let (row, font, small, line_h) = (p.row, 36u32, p.small, p.line_h);
    let w = p.w;
    let x = (c.w - w) / 2;
    let inner = w - 2 * 48;
    let y = ((c.h - NU_PILL - NU_PAD - p.h) / 2).max(NU_PAD);
    // A sheet like Curator's and the game card's pickers (`kit::sheet`).
    pocket_ui::kit::sheet(c, x, y, w, p.h);
    let left = x + 48;
    let mut ty = y + g.0 + 46;
    text.draw(c, Face::Title, 50, left, ty, pal().chalk, d.title());
    ty += g.1;
    let warn = matches!(d.kind, DialogKind::Leave | DialogKind::OpenEs);
    for (i, lines) in p.paragraphs.iter().enumerate() {
        let is_warning = warn && i > 0;
        if is_warning {
            pocket_ui::retro::pill(c, left, ty + 10, 8, lines.len() as i32 * line_h - 4, 4, pal().alert);
        }
        let colour = if i == 0 { pal().quiet } else { pal().text };
        for line in lines {
            ty += line_h;
            text.draw(c, Face::Body, small, left + 28, ty, colour, line);
        }
        ty += g.2;
    }
    ty += g.3;
    // The window of the choices that fit, the selection near its middle (as Quick settings' list).
    let first = if choices.len() <= p.visible { 0 } else { d.sel.saturating_sub(p.visible / 2).min(choices.len() - p.visible) };
    for (k, choice) in choices.iter().enumerate().skip(first).take(p.visible) {
        let ry = ty + (k - first) as i32 * row;
        paint_row(c, text, (left, ry, inner, row), font, choice.label(), k == d.sel);
        hits.row(Area::new(left, ry, inner, row), k);
    }
    pocket_ui::kit::scrollbar(c, ty, p.visible as i32 * row, first, p.visible, choices.len());
    hits.list(d.sel, choices.len(), 0.0, row as f32);
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
    let sub = text.wrap_prose(Face::Body, NU_SMALL, "pocket-ui started with only what this page needs. Games and saves are untouched.", width);
    let sub_step = pocket_ui::type_scale::line(NU_SMALL, 12);
    for (k, l) in sub.iter().take(2).enumerate() {
        text.draw(c, Face::Body, NU_SMALL, left, pad + 58 + 48 + k as i32 * sub_step, pal().quiet, l);
    }
    let sub_extra = (sub.len().clamp(1, 2) as i32 - 1) * sub_step + pocket_ui::type_scale::grow(NU_SMALL);

    // What happened, in a panel.
    let w = &page.what;
    let mut lines: Vec<(String, u32, u32)> = Vec::new();
    for l in text.wrap_prose(Face::Body, NU_SMALL, &w.headline(), width - 2 * 28).into_iter().take(2) {
        lines.push((l, NU_SMALL, pal().text));
    }
    if let Some(crash) = &w.last_crash {
        for l in text.wrap_prose(Face::Body, 24, &format!("Last crash: {crash}"), width - 2 * 28).into_iter().take(2) {
            lines.push((l, 24, pal().quiet));
        }
    }
    let launcher: Vec<String> = w.launcher.iter().map(|l| text.fit(Face::Body, 24, l, width - 2 * 28)).collect();
    // On a short screen (1024x768) the five choices must still end above the hint pill: the launcher's older lines give way
    // first, then the rows get a little less air. With room to spare, the row's explanation keeps two lines below them.
    let head = 46;
    let step = pocket_ui::type_scale::line(NU_SMALL, 12);
    let rows = page.rows();
    let rows_bottom = c.h - pad - NU_PILL - 16;
    let panel_y = pad + 58 + 48 + 30 + sub_extra;
    let base_h: i32 = lines.iter().map(|(_, px, _)| pocket_ui::type_scale::line(*px, 12)).sum();
    let line24 = pocket_ui::type_scale::line(24, 12);
    let n = rows.len() as i32;
    let floor = pocket_ui::type_scale::row(50, font);
    let (mut keep, mut row_h) = (launcher.len(), row);
    'fit: for wanted_note in [2 * step + 44, 0] {
        for dropped in 0..=launcher.len() {
            let panel_h = head + base_h + (launcher.len() - dropped) as i32 * line24 + 20;
            let room = rows_bottom - wanted_note - (panel_y + panel_h + 24);
            let h = (room / n.max(1)).min(row);
            let enough = if wanted_note > 0 { h >= (row - 12).max(floor) } else { h >= floor };
            if enough || (wanted_note == 0 && dropped == launcher.len()) {
                (keep, row_h) = (launcher.len() - dropped, h.max(floor).min(row));
                break 'fit;
            }
        }
    }
    for l in launcher.iter().skip(launcher.len() - keep) {
        lines.push((l.clone(), 24, pal().dust));
    }
    let row = row_h;
    let panel_h = head + lines.iter().map(|(_, px, _)| pocket_ui::type_scale::line(*px, 12)).sum::<i32>() + 20;
    pocket_ui::kit::panel(c, pad, panel_y, c.w - 2 * pad, panel_h);
    text.draw(c, Face::Body, 24, left, panel_y + 36, pal().dust, "What happened");
    let mut y = panel_y + head;
    for (l, px, colour) in &lines {
        y += pocket_ui::type_scale::line(*px, 12);
        text.draw(c, Face::Body, *px, left, y, *colour, l);
    }

    // The choices.
    let top = panel_y + panel_h + 24;
    for (i, r) in rows.iter().enumerate() {
        let ry = top + i as i32 * row;
        paint_row(c, text, (pad, ry, c.w - 2 * pad, row), font, &page.label(*r), i == page.sel);
        hits.row(Area::new(pad, ry, c.w - 2 * pad, row), i);
    }
    hits.list(page.sel, rows.len(), 0.0, row as f32);
    // Above the hints; with larger text a paragraph that reaches them ends with "…".
    let bottom = c.h - pad - NU_PILL - 16;
    let mut y = top + rows.len() as i32 * row + 44;
    // What was just done (the message) keeps its lines; the row's explanation gives way first.
    let message_h = page.message.as_ref().map_or(0, |m| text.wrap_prose(Face::Body, NU_SMALL, m, width).len() as i32 * step + 12);
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
        // Not behind a running game: the pause menu has the pad then (the FIFO's `leave` could ask at any time). Never in
        // Kids Mode: its way out is the PIN, and a keyboard's Escape comes here without passing `kids_pad`.
        if self.running.is_some() || self.kids_live() {
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
        // The power menu opened over it: the buttons are that menu's, and the dialog is over with.
        if self.leave.is_some() && self.sys.power_menu_open() {
            self.leave = None;
            self.request_redraw();
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

/// A crash loop under Kids Mode: the child sees a picture and "ask a grown-up", no words they can't use. A grown-up holds
/// SELECT + START and enters the PIN, as on the child's shelf, for the rescue page. A forgotten PIN: the file on the card (`kids::reset_wanted`).
/// The handheld with a plaster on it: it isn't the child's fault and it can be mended.
pub(crate) fn paint_plaster(c: &mut Canvas, k: pocket_ui::kids::Colours, top: f32) {
    let (w, h) = (c.w, c.h);
    let u = h as f32 / 1080.0;
    let (bw, bh) = ((460.0 * u) as i32, (290.0 * u) as i32);
    let (x, y) = ((w - bw) / 2, (top * u) as i32);
    c.round_rect(x, y, bw, bh, (46.0 * u) as i32, k.on_accent);
    c.round_rect(x + bw / 5, y + (40.0 * u) as i32, bw * 3 / 5, bh - (80.0 * u) as i32, (12.0 * u) as i32, k.spark);
    c.disc(x + bw / 10, y + bh / 2, (22.0 * u) as i32, k.accent);
    c.disc(x + bw - bw / 10, y + bh / 2, (22.0 * u) as i32, k.accent);
    // The plaster, across the screen.
    let (pw, ph) = ((190.0 * u) as i32, (64.0 * u) as i32);
    c.round_rect((w - pw) / 2, y + (bh - ph) / 2, pw, ph, ph / 2, 0xf2b98a);
    c.rect((w - pw / 3) / 2, y + (bh - ph) / 2, pw / 3, ph, 0xe09a68);
}

struct KidsDoor {
    theme: pocket_ui::kids::Theme,
    pin: String,
    select: bool,
    start: bool,
    hold_since: Option<std::time::Instant>,
    gate: Option<pocket_ui::kids::Gate>,
}

impl KidsDoor {
    fn for_data(data_dir: &std::path::Path) -> Option<KidsDoor> {
        let cfg = pocket_ui::kids::Config::load(data_dir);
        let forgotten = pocket_ui::kids::reset_wanted(&pocket_ui::kids::reset_places(), cfg.reset_seen).is_some();
        (cfg.on && !forgotten).then_some(KidsDoor { theme: cfg.theme, pin: cfg.pin, select: false, start: false, hold_since: None, gate: None })
    }

    /// True when the door is open: the rescue page takes over.
    fn button(&mut self, b: Button, pressed: bool) -> bool {
        match b {
            Button::Select => self.select = pressed,
            Button::Start => self.start = pressed,
            _ => {}
        }
        self.hold_since = if self.select && self.start && self.gate.is_none() { self.hold_since.or(Some(std::time::Instant::now())) } else { None };
        let Some(gate) = self.gate.as_mut().filter(|_| pressed) else { return false };
        match gate.press(b, &self.pin, std::time::Instant::now()) {
            pocket_ui::kids::GateOut::Stay => false,
            pocket_ui::kids::GateOut::Back => {
                self.gate = None;
                false
            }
            pocket_ui::kids::GateOut::Open => true,
        }
    }

    /// The time of the hold is up: the PIN.
    fn tick(&mut self) -> bool {
        if self.hold_since.is_some_and(|t| t.elapsed() >= pocket_ui::kids::LEAVE_HOLD) {
            self.hold_since = None;
            self.gate = Some(Default::default());
            return true;
        }
        false
    }

    fn paint(&self, c: &mut Canvas, text: &mut Text) {
        let (w, h) = (c.w, c.h);
        super::app_kids::paint_backdrop(c, 0, 0, w, h, self.theme);
        let k = self.theme.colours();
        let u = h as f32 / 1080.0;
        let centre = |text: &mut Text, c: &mut Canvas, face, px: f32, y: f32, ink, s: &str| {
            let px = (px * u).round() as u32;
            let mut y = (y * u) as i32;
            for l in text.wrap_at_most(face, px, s, w - (60.0 * u) as i32, 2) {
                let tw = text.width(face, px, &l);
                text.draw(c, face, px, (w - tw) / 2, y, ink, &l);
                y += (px as f32 * 1.25) as i32;
            }
        };
        if let Some(gate) = &self.gate {
            centre(text, c, Face::Title, 72.0, 170.0, k.ink, "For grown-ups");
            centre(text, c, Face::Body, 40.0, 240.0, k.ink, "Enter your PIN.");
            let (bw, bh, gap) = ((170.0 * u) as i32, (240.0 * u) as i32, (34.0 * u) as i32);
            let x0 = (w - 4 * bw - 3 * gap) / 2;
            let top = (340.0 * u) as i32;
            for (i, digit) in gate.dials.digits.iter().enumerate() {
                let x = x0 + i as i32 * (bw + gap);
                let at = i == gate.dials.at;
                pocket_ui::retro::pill(c, x, top, bw, bh, (26.0 * u) as i32, if at { k.accent } else { k.top });
                let s = if i < gate.dials.at { "•".to_string() } else { digit.to_string() };
                let px = (150.0 * u).round() as u32;
                let tw = text.width(Face::Title, px, &s);
                text.draw(c, Face::Title, px, x + (bw - tw) / 2, top + bh / 2 + px as i32 * 36 / 100, if at { k.on_accent } else { k.ink }, &s);
            }
            if let Some(wait) = gate.waiting(std::time::Instant::now()) {
                centre(text, c, Face::Body, 38.0, 680.0, k.ink, &format!("Too many tries. Wait {} seconds.", wait.as_secs() + 1));
            } else if gate.wrong > 0 {
                centre(text, c, Face::Body, 38.0, 680.0, k.ink, "That's not it. Try again.");
            }
            if gate.wrong >= pocket_ui::kids::WRONG_BEFORE_HELP {
                centre(text, c, Face::Body, 34.0, 750.0, k.ink, &format!("Forgot it? Put a file called {} on the card and restart.", pocket_ui::kids::RESET_FILE));
            }
            return;
        }
        paint_plaster(c, k, 130.0);
        centre(text, c, Face::Title, 86.0, 590.0, k.ink, "Oops, something broke");
        centre(text, c, Face::Title, 66.0, 700.0, k.accent, "Please ask a grown-up");
        centre(text, c, Face::Body, 32.0, 980.0, k.ink, "For grown-ups: hold SELECT and START for 5 seconds.");
        if let Some(t) = self.hold_since {
            let p = (t.elapsed().as_secs_f32() / pocket_ui::kids::LEAVE_HOLD.as_secs_f32()).min(1.0);
            let bar = (12.0 * u).max(4.0) as i32;
            c.rect(0, 0, w, bar, k.on_accent);
            c.rect(0, 0, (w as f32 * p) as i32, bar, k.accent);
        }
    }
}

/// The safe-mode app: the page, its window and nothing else.
struct Rescue {
    /// Kids Mode is on: its door in front of the page.
    kids: Option<KidsDoor>,
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
            kids: KidsDoor::for_data(&data_dir),
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

    /// A button while Kids Mode's door is shut, a pad's or a keyboard's: the door has it, and the page behind sees nothing.
    /// False: no door, the page's. The right PIN opens it, from the next press on.
    fn door_takes(&mut self, button: Button, pressed: bool) -> bool {
        let Some(door) = self.kids.as_mut() else { return false };
        if door.button(button, pressed) {
            self.kids = None;
        }
        true
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
        if let Some(door) = &self.kids {
            door.paint(c, &mut self.text);
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
                if result.is_ok() {
                    // The switch went back to off with the settings; a share still running stops with it.
                    std::thread::spawn(pocket_ui::wifi::disable_sharing);
                }
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
            UserEvent::Pad(PadEvent::Button { source, button, pressed }) if self.kids.is_some() => {
                let role = if source == usize::MAX { button } else { pocket_ui::device::current().menu_layout().role(button) };
                self.door_takes(role, pressed);
                self.redraw();
            }
            UserEvent::Pad(PadEvent::Button { source, button, pressed: true }) => {
                // The FIFO already speaks roles; only the physical pads need the printed layout.
                self.press(el, if source == usize::MAX { button } else { pocket_ui::device::current().menu_layout().role(button) });
            }
            UserEvent::Touch(_) if self.kids.is_some() => {}
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
                if self.door_takes(button, true) {
                    self.redraw();
                    return;
                }
                self.press(el, button);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // While SELECT + START are held the door's bar fills.
        if let Some(door) = self.kids.as_mut() {
            door.tick();
            if door.hold_since.is_some() {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(std::time::Instant::now() + std::time::Duration::from_millis(100)));
                self.redraw();
                return;
            }
        }
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
    let kids = state.split(':').nth(1).filter(|p| p.starts_with("kids")).map(str::to_string);
    let mut app = Rescue::new(data_dir.clone(), page, wants_light(&data_dir));
    // `rescue:kids`, `rescue:kids-pin`: Kids Mode's door, whatever POCKETUI_DATA says.
    match kids.as_deref() {
        Some(what) => {
            let gate = (what == "kids-pin").then(|| {
                let mut g = pocket_ui::kids::Gate { wrong: pocket_ui::kids::WRONG_BEFORE_HELP, ..Default::default() };
                g.dials.digits = [4, 0, 0, 0];
                g.dials.at = 1;
                g
            });
            app.kids = Some(KidsDoor { theme: pocket_ui::kids::Theme::Blue, pin: "1234".into(), select: false, start: false, hold_since: None, gate });
        }
        None => app.kids = None,
    }
    let (w, h) = super::snapshot_dims();
    let mut buf = vec![0u32; (w * h) as usize];
    app.paint(&mut Canvas::new(&mut buf, w, h));
    super::write_png(&buf, w, h, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The crash-loop door: a keyboard's keys go where the pad's go, so no press reaches the hidden page.
    #[test]
    fn a_keyboard_cannot_reach_the_rescue_page_behind_the_kids_door() {
        let tmp = tempfile::tempdir().unwrap();
        let page = RescuePage::new(rescue::WhatHappened { reason: None, last_crash: None, launcher: Vec::new() }, None, String::new());
        let mut app = Rescue::new(tmp.path().to_path_buf(), page, false);
        app.kids = Some(KidsDoor { theme: pocket_ui::kids::Theme::Blue, pin: "0000".into(), select: false, start: false, hold_since: None, gate: None });
        for b in [Button::Down, Button::Down, Button::Down, CONFIRM, Button::Down, CONFIRM] {
            assert!(app.door_takes(b, true), "{b:?}");
            assert!(app.kids.is_some());
        }
        // The PIN opens it; from then on the page has the keys.
        app.kids.as_mut().unwrap().gate = Some(Default::default());
        for _ in 0..3 {
            assert!(app.door_takes(CONFIRM, true));
            assert!(app.kids.is_some());
        }
        assert!(app.door_takes(CONFIRM, true), "the press that opens it is the door's");
        assert!(app.kids.is_none());
        assert!(!app.door_takes(Button::Down, true));
    }
}
