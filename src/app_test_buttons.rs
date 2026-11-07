//! Part of the pocket-ui binary: Settings → Help & tools → Test buttons, the App's side of `pocket_ui::button_test`.
//!
//! The page lists every controller (the handheld's own buttons, Bluetooth, USB) and draws the selected one: face buttons,
//! D-pad, shoulders, triggers with their travel and the point they count as pressed, SELECT, START, HOME and both sticks
//! with the dead zone. Each lights up while it is down. Under the drawing: the last button pressed, on which controller,
//! and what it is in the menus and in a game after the player's own layout (Controller setup) and the Button layout.
//!
//! Every pad event is the page's while it is open (`test_buttons_swallows`, ahead of quick settings and the other
//! handlers), so B, START and HOME can be tested too; only the volume and power keys keep their meaning. The way out is
//! a hold: HOME for two seconds, or B and START together (`pocket_ui::button_test`), and a tap on the hint pill for
//! the touch screen. `merge_pad` hands every event over before the controllers are merged (`test_buttons_event`).

use std::time::Instant;

use pocket_ui::button_layout;
use pocket_ui::button_test::{self, ButtonTest, Kind, Mapping, PadView, HOLD, TRIGGER_PRESS};
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::pad::bindings;
use pocket_ui::pad::{Button, Controller, PadEvent, STICK_DEADZONE};
use pocket_ui::palette::*;
use pocket_ui::players::{self, Mode, Seats};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::app_controllers::button_name;
use super::{App, SettingsPage, CANCEL, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

pub(crate) const SETTINGS_NAME: &str = "Test buttons";

/// The drawing is laid out on a 1000 x 600 sheet and scaled to the room it gets.
const SHEET_W: f32 = 1000.0;
const SHEET_H: f32 = 640.0;

/// The last-press panel `w` wide: its left column (the button, who pressed it) and its right one (what it does), 24 apart.
fn last_panel_columns(w: i32) -> (i32, i32) {
    let inner = w - 2 * pocket_ui::kit::PANEL_PAD;
    let left = inner * 40 / 100;
    (left, inner - left - 24)
}

/// What the App keeps while the page is open.
#[derive(Default)]
pub(crate) struct TestButtons {
    test: Option<ButtonTest>,
    /// `players.json`'s order, read when the page opens: who is player 1, 2, 3, 4.
    order: Vec<String>,
}

impl App {
    pub(crate) fn open_test_buttons(&mut self) -> Option<SettingsPage> {
        let now = Instant::now();
        self.test_buttons = TestButtons {
            test: Some(ButtonTest::new(&self.pad.controllers(), now)),
            order: players::Order::load(&self.data_dir).order,
        };
        Some(SettingsPage::TestButtons)
    }

    fn mapping_of(&self, controller: &Controller) -> Mapping {
        Mapping { layout: bindings::layout(&controller.key), menu: button_layout::get() }
    }

    /// One pad event of one device, before the controllers are merged (`merge_pad`): the tester keeps each controller's
    /// own buttons, sticks and triggers. Nothing while the page is closed.
    pub(crate) fn test_buttons_event(&mut self, pe: &PadEvent) {
        if self.test_buttons.test.is_none() {
            return;
        }
        let source = match pe {
            PadEvent::Button { source, .. } | PadEvent::Axis { source, .. } | PadEvent::Raw { source, .. } => *source,
            _ => return,
        };
        let controller = self.pad.controller_of(source);
        let map = self.mapping_of(&controller);
        if let Some(test) = self.test_buttons.test.as_mut() {
            test.feed(&controller, pe, map, Instant::now());
        }
        self.request_redraw();
    }

    /// Controllers came or went (`merge_pad`, after the pad has the event).
    pub(crate) fn test_buttons_roster(&mut self) {
        let controllers = self.pad.controllers();
        if let Some(test) = self.test_buttons.test.as_mut() {
            test.sync(&controllers, Instant::now());
        }
        self.request_redraw();
    }

    /// Whether the page takes this pad event (`user_event_inner` stops there): all of them but the volume and power keys.
    /// A press that did not come from a controller (the touch screen's tap on the hint pill, the command FIFO) leaves
    /// at once: it has no hold to show.
    pub(crate) fn test_buttons_swallows(&mut self, pe: &PadEvent) -> bool {
        if self.settings_page != Some(SettingsPage::TestButtons) {
            return false;
        }
        match *pe {
            PadEvent::Button { button: Button::Power | Button::VolUp | Button::VolDown, .. } => false,
            PadEvent::Button { source: usize::MAX, button: Button::Mode | CANCEL, pressed: true } => {
                self.leave_test_buttons();
                true
            }
            _ => true,
        }
    }

    pub(crate) fn test_buttons_input(&mut self, b: Button) -> Option<SettingsPage> {
        if b == Button::Mode || b == CANCEL {
            self.leave_test_buttons();
            return self.settings_page;
        }
        Some(SettingsPage::TestButtons)
    }

    fn leave_test_buttons(&mut self) {
        self.test_buttons = TestButtons::default();
        self.settings_page = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        self.request_redraw();
    }

    /// Every turn of the loop: the hold to leave has come to its end.
    pub(crate) fn test_buttons_tick(&mut self, now: Instant) {
        if self.settings_page == Some(SettingsPage::TestButtons) && self.test_buttons.test.as_ref().is_some_and(|t| t.leave_due(now)) {
            println!("test buttons: left by the hold");
            self.leave_test_buttons();
        }
    }

    /// The hold's bar moves.
    pub(crate) fn test_buttons_animating(&self) -> bool {
        self.settings_page == Some(SettingsPage::TestButtons) && self.test_buttons.test.as_ref().is_some_and(|t| t.hold(Instant::now()).is_some())
    }

    /// `--snapshot test-buttons[:<case>]`: made-up controllers pressed the way the pad reader would report them.
    /// Cases: `one` (the default), `idle` (the handheld's own buttons, nothing pressed yet), `bt`, `two`, `remap`, `xbox`,
    /// `hold`, `unknown`, `empty`.
    pub(crate) fn test_buttons_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("test-buttons") else { return false };
        let case = rest.trim_start_matches(':');
        let case = if case.is_empty() { "one" } else { case };
        let now = Instant::now();
        let pocket = Controller::builtin();
        let sn30 = Controller { key: "bt:e4:17:d8:00:00:01".into(), name: "8BitDo SN30 Pro".into(), gamepad: true, node: String::new() };
        let pro2 = Controller { key: "bt:e4:17:d8:00:00:02".into(), name: "8BitDo Pro 2".into(), gamepad: true, node: String::new() };
        let pads: Vec<(usize, &Controller)> = match case {
            "empty" => Vec::new(),
            "bt" => vec![(900, &pocket), (902, &pro2)],
            "two" | "remap" | "xbox" | "hold" | "unknown" => vec![(900, &pocket), (901, &sn30)],
            _ => vec![(900, &pocket)],
        };
        for (source, controller) in &pads {
            self.pad.merge(PadEvent::Connected { source: *source, controller: (*controller).clone() });
        }
        self.open_test_buttons();
        self.test_buttons.order = if case == "bt" { vec![pocket.key.clone(), pro2.key.clone()] } else { vec![pocket.key.clone(), sn30.key.clone()] };
        let button = |source: usize, button: Button| PadEvent::Button { source, button, pressed: true };
        let axis = |source: usize, axis: pocket_ui::pad::Axis, value: f32| PadEvent::Axis { source, axis, value };
        use pocket_ui::pad::Axis;
        let mut events: Vec<PadEvent> = Vec::new();
        match case {
            "bt" => {
                // A Pro 2 in D-input: the triggers are axes. Right trigger past the point it counts, left one half way.
                events.extend([axis(902, Axis::L2, 0.4), axis(902, Axis::R2, 0.8), button(902, Button::R2), button(902, Button::South), axis(902, Axis::LeftX, 0.55), axis(902, Axis::LeftY, -0.3)]);
            }
            "two" => {
                events.extend([button(900, Button::East), button(900, Button::L1), axis(900, Axis::LeftX, 0.12), button(901, Button::Up), button(901, Button::West)]);
            }
            "remap" => {
                // The player made the right button act as the bottom one, and L2 as the left one.
                let mut layout = bindings::Layout::default();
                layout.assign(Button::East, Button::South);
                layout.assign(Button::L2, Button::West);
                let _ = bindings::save_layout(&self.data_dir, &sn30.key, layout);
                events.extend([button(901, Button::South)]);
            }
            "xbox" => {
                button_layout::set(button_layout::ButtonLayout::Xbox);
                self.button_layout = button_layout::ButtonLayout::Xbox;
                events.extend([button(900, Button::East)]);
            }
            "hold" => events.push(button(900, Button::L1)),
            "unknown" => events.extend([button(901, Button::South), PadEvent::Raw { source: 901, text: "8BitDo SN30 Pro: BTN_C 1".into() }]),
            "empty" | "idle" => {}
            _ => {
                events.extend([button(900, Button::East), button(900, Button::L1), axis(900, Axis::LeftX, 0.6), axis(900, Axis::LeftY, 0.35), axis(900, Axis::R2, 0.3), axis(900, Axis::RightX, 0.1)]);
            }
        }
        for e in &events {
            self.test_buttons_event(e);
        }
        if case == "hold" {
            // HOME went down a second ago: the bar is half full.
            let map = Mapping { layout: bindings::Layout::default(), menu: button_layout::get() };
            if let (Some(test), Some(ago)) = (self.test_buttons.test.as_mut(), now.checked_sub(HOLD / 2)) {
                test.feed(&pocket, &PadEvent::Button { source: 900, button: Button::Mode, pressed: true }, map, ago);
            }
        }
        self.settings_page = Some(SettingsPage::TestButtons);
        true
    }

    pub(crate) fn paint_test_buttons(&mut self, c: &mut Canvas) {
        let Some(test) = self.test_buttons.test.take() else {
            // Opened some other way (a deep link): there is nothing to show but how to get out.
            c.clear(pal().void);
            self.sheet_header(c, SETTINGS_NAME);
            self.nu_hints(c, &[("B", "Back")]);
            return;
        };
        self.paint_test_page(c, &test);
        self.test_buttons.test = Some(test);
    }

    fn paint_test_page(&mut self, c: &mut Canvas, test: &ButtonTest) {
        c.clear(pal().void);
        self.sheet_header(c, SETTINGS_NAME);
        let now = Instant::now();
        let hints = [("HOME", "Hold to leave")];
        let note = "To leave, hold HOME for 2 seconds, or B and START together.";
        let bottom = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &hints, note, 2, pal().text);
        // A tap on the pill leaves, for the touch screen (a tap on a hint key sends the button, which has no hold).
        let pill = pocket_ui::kit::pill_width(&mut self.text, c.w, &hints);
        self.hits.button(Area::new(c.w - NU_PAD - pill, c.h - NU_PAD - NU_PILL - 8, pill, NU_PILL + 16), Button::Mode);
        let top = NU_PAD + NU_PILL + 12;
        let (left, right) = (NU_PAD, c.w - NU_PAD);
        if test.pads().is_empty() {
            let (x, base) = (left + NU_BTN_PAD, top + 60 + pocket_ui::type_scale::grow(37));
            let line = "Pair one in Settings › Connections › Bluetooth, or plug one into the USB-C port. This handheld's own buttons show up here too.";
            pocket_ui::kit::empty_state(c, &mut self.text, Some(pocket_ui::ui_art::Art::Search), (x, right - NU_BTN_PAD - x), top + 8, base, bottom - 12, "No controller found", line);
            return;
        }
        // Left: the controllers. Right: the drawing of the selected one. Under both: the last button pressed.
        let column = (c.w * 30 / 100).clamp(300, 400);
        let seats = Seats::arrange(&self.test_buttons.order, &test.pads().iter().map(|p| p.controller.clone()).collect::<Vec<_>>(), Mode::Exact);
        let panel_h = self.last_panel_height(right - left);
        let room = bottom - top - 12 - panel_h - 16;
        self.paint_pad_list(c, test, &seats, (left, top, column, room));
        let x = left + column + 24;
        if let Some(pad) = test.selected() {
            let map = self.mapping_of(&pad.controller);
            self.paint_pad_art(c, (x, top, right - x, room.max(120)), pad, map);
        }
        self.paint_last_panel(c, test, &seats, (left, top + room + 16, right - left, panel_h), now);
    }

    /// Height of the panel `w` wide under the list and the drawing: three lines, the button's name and who pressed it on the
    /// left, what it does in menus, in games and the note on the right; more when the hints before the first press need
    /// them (the same height before and after, so the drawing doesn't jump).
    fn last_panel_height(&mut self, w: i32) -> i32 {
        let step = pocket_ui::type_scale::line(NU_SMALL - 2, 8);
        let (_, right_w) = last_panel_columns(w);
        28 + self.idle_hints(right_w).len().max(3) as i32 * step
    }

    /// The two hints on the right before the first press, wrapped to `w`: in Retro's wider letters the second ran past
    /// the panel's frame and off the screen ("…in game", Mini V2, 06.11.2026).
    fn idle_hints(&mut self, w: i32) -> Vec<String> {
        let small = NU_SMALL - 2;
        let mut lines = self.text.wrap_prose(Face::Body, small, "It lights up on the controller drawn here.", w);
        lines.extend(self.text.wrap_prose(Face::Body, small, "Here you see what it does in menus and in games.", w));
        lines
    }

    fn paint_pad_list(&mut self, c: &mut Canvas, test: &ButtonTest, seats: &Seats, (x, y0, w, h): (i32, i32, i32, i32)) {
        let (name_px, sub_px) = (NU_SMALL, NU_SMALL - 4);
        let row = pocket_ui::type_scale::line(name_px, 8) + pocket_ui::type_scale::line(sub_px, 6) + 22;
        let visible = ((h / row).max(1) as usize).min(test.pads().len());
        let selected = test.selected().and_then(|s| test.pads().iter().position(|p| p.controller.key == s.controller.key)).unwrap_or(0);
        let first = super::window_start(selected, visible, test.pads().len());
        for (i, pad) in test.pads().iter().enumerate().skip(first).take(visible) {
            let y = y0 + (i - first) as i32 * row;
            let on = i == selected;
            if on {
                pocket_ui::kit::selection(c, x, y, w, row);
            }
            // A light that burns while something of this controller is down.
            let (cx, cy) = (x + 30, y + row / 2);
            pocket_ui::retro::disc(c, cx, cy, 10, if pad.busy() { pal().amber } else if on { pal().label_dust } else { pal().off });
            let tx = x + 54;
            let (name, sub) = (players::short_name(&pad.controller), self.pad_line(pad, seats));
            let name = self.text.fit(Face::Body, name_px, &name, w - 54 - 16);
            let sub = self.text.fit(Face::Body, sub_px, &sub, w - 54 - 16);
            let step = pocket_ui::type_scale::line(name_px, 8);
            let base = y + 12 + step - pocket_ui::type_scale::line(name_px, 8) / 4;
            self.text.draw(c, Face::Body, name_px, tx, base, if on { pal().ink } else { pal().text }, &name);
            self.text.draw(c, Face::Body, sub_px, tx, base + pocket_ui::type_scale::line(sub_px, 6), if on { pal().label_dust } else { pal().dust }, &sub);
        }
        if visible < test.pads().len() {
            pocket_ui::kit::scrollbar(c, y0, visible as i32 * row, first, visible, test.pads().len());
        }
    }

    /// "Player 2 · Bluetooth".
    fn pad_line(&self, pad: &PadView, seats: &Seats) -> String {
        let kind = Kind::of(&pad.controller).label();
        match seats.player_of(&pad.controller.key) {
            Some(p) => format!("Player {} · {kind}", p + 1),
            None => format!("No player · {kind}"),
        }
    }

    /// Under the list and the drawing: the last button pressed, on any controller. Left: its name and who pressed it; right: what
    /// it does in menus and in games after the player's remaps, and a note.
    fn paint_last_panel(&mut self, c: &mut Canvas, test: &ButtonTest, seats: &Seats, (x, y, w, h): (i32, i32, i32, i32), now: Instant) {
        pocket_ui::kit::panel(c, x, y, w, h);
        if pocket_ui::retro::on() {
            pocket_ui::retro::frame_lines_are_no_text(c, x, y, w, h);
        }
        let (px, inner) = (x + pocket_ui::kit::PANEL_PAD, w - 2 * pocket_ui::kit::PANEL_PAD);
        let (small, step) = (NU_SMALL - 2, pocket_ui::type_scale::line(NU_SMALL - 2, 8));
        let base = |row: i32| y + 12 + step * (row + 1) - 6;
        if let Some(progress) = test.hold(now) {
            self.text.draw(c, Face::Title, 40, px, base(0) + 2, pal().chalk, "Keep holding to leave");
            let bar_h = 22;
            let bar_y = y + h - bar_h - 20;
            pocket_ui::retro::pill(c, px, bar_y, inner, bar_h, bar_h / 2, pal().spine);
            pocket_ui::retro::pill(c, px, bar_y, ((inner as f32 * progress) as i32).max(bar_h), bar_h, bar_h / 2, pal().amber);
            return;
        }
        let (left_w, right_w) = last_panel_columns(w);
        let right_x = px + left_w + 24;
        let Some(last) = test.last() else {
            // On one line in its column, a little smaller where it needs it: German or Large text on 1024x768 ran into
            // the hints beside it.
            let title_px = (24..=40).rev().step_by(2).find(|p| self.text.width(Face::Title, *p, "Press a button") <= left_w).unwrap_or(24);
            let title = self.text.fit(Face::Title, title_px, "Press a button", left_w);
            self.text.draw(c, Face::Title, title_px, px, base(0) + 2, pal().chalk, &title);
            for (i, line) in self.idle_hints(right_w).iter().enumerate() {
                self.text.draw(c, Face::Body, small, right_x, base(i as i32), pal().quiet, line);
            }
            return;
        };
        let head = self.text.fit(Face::Title, 40, button_name(last.physical), left_w);
        self.text.draw(c, Face::Title, 40, px, base(0) + 2, pal().chalk, &head);
        let who = {
            let name = players::short_name(&last.controller);
            seats.player_of(&last.controller.key).map_or(name.clone(), |p| format!("Player {} · {name}", p + 1))
        };
        let who = self.text.fit(Face::Body, small, &who, left_w);
        self.text.draw(c, Face::Body, small, px, base(1), pal().dust, &who);
        let menus = format!("In menus: {}", button_test::menu_name(last.menus));
        let menus = self.text.fit(Face::Body, small, &menus, right_w);
        self.text.draw(c, Face::Body, small, right_x, base(0), pal().text, &menus);
        let games = match button_test::console_name(last.game) {
            Some(name) => format!("In games: the console's {name}"),
            None if last.physical == Button::Mode => "In games: opens the pause menu".to_string(),
            None => "In games: not used".to_string(),
        };
        let games = self.text.fit(Face::Body, small, &games, right_w);
        self.text.draw(c, Face::Body, small, right_x, base(1), pal().text, &games);
        // The player's own layout moved it, or the pad sent something pocket-ui doesn't know.
        let unknown = test.pads().iter().find(|p| p.controller.key == last.controller.key).and_then(|p| p.unknown.clone());
        let (note, color) = if last.remapped() {
            (format!("Remapped to: {}", button_name(last.game)), pal().amber)
        } else if let Some(raw) = unknown {
            (format!("Not recognised: {raw}"), pal().amber)
        } else {
            return;
        };
        let note = self.text.fit(Face::Body, small, &note, right_w);
        self.text.draw(c, Face::Body, small, right_x, base(2), color, &note);
    }

    /// The controller drawn in `area`, scaled to fit, every control lit while it is down.
    fn paint_pad_art(&mut self, c: &mut Canvas, (ax, ay, aw, ah): (i32, i32, i32, i32), pad: &PadView, map: Mapping) {
        let s = (aw as f32 / SHEET_W).min(ah as f32 / SHEET_H);
        let (ox, oy) = (ax + ((aw as f32 - SHEET_W * s) / 2.0) as i32, ay + ((ah as f32 - SHEET_H * s) / 2.0) as i32);
        let u = |v: i32| (v as f32 * s).round() as i32;
        let (px, py) = (|v: i32| ox + u(v), |v: i32| oy + u(v));
        let (lit, base) = (pal().amber, pal().pill);
        let on_lit = pal().pick(0x1a1916, 0xfffdf8);
        let moved = |b: Button| bindings::BUTTONS.contains(&b) && map.game(b) != b;
        let mut badges: Vec<(i32, i32)> = Vec::new();
        let label_px = |design: u32| ((design as f32 * s.max(0.7)) as u32).clamp(18, design + 4);
        // The body, under everything.
        c.round_rect(px(30), py(100), u(940), u(480), u(170), pal().spine);
        // Shoulders and triggers.
        for (left, shoulder, trigger) in [(true, Button::L1, Button::L2), (false, Button::R1, Button::R2)] {
            let x = if left { 70 } else { 660 };
            let (sx, sy, sw, sh) = (px(x), py(60), u(270), u(56));
            let down = pad.is_down(shoulder);
            c.round_rect(sx, sy, sw, sh, sh / 2, if down { lit } else { base });
            let text = if left { "L1" } else { "R1" };
            let tw = self.text.width(Face::Body, label_px(28), text);
            self.text.draw(c, Face::Body, label_px(28), sx + (sw - tw) / 2, sy + sh / 2 + pocket_ui::type_scale::mid(label_px(28)), if down { on_lit } else { pal().text }, text);
            if moved(shoulder) {
                badges.push((sx + sw - sh / 3, sy + sh / 5));
            }
            // The trigger: a bar that fills with the travel; the tick is where it counts as pressed.
            let t = pad.trigger(left);
            let fraction = if t.analog { t.value } else if t.pressed { 1.0 } else { 0.0 };
            let (bx, by, bw, bh) = (px(x), py(0), u(270), u(44));
            c.round_rect(bx, by, bw, bh, bh / 2, base);
            if fraction > 0.01 {
                let fill = if t.pressed { lit } else { mix(base, lit, 150) };
                c.round_rect(bx, by, ((bw as f32 * fraction) as i32).max(bh), bh, bh / 2, fill);
            }
            if t.analog {
                c.rect(bx + (bw as f32 * TRIGGER_PRESS) as i32, by - 3, 3, bh + 6, pal().text);
            }
            let name = if left { "L2" } else { "R2" };
            let covered = fraction * bw as f32 > u(48) as f32;
            self.text.draw(c, Face::Body, label_px(28), bx + bh / 2 - 4, by + bh / 2 + pocket_ui::type_scale::mid(label_px(28)), if covered && t.pressed { on_lit } else { pal().text }, name);
            if t.analog {
                let shown = format!("{}%", button_test::percent(t.value));
                // Beside the bar, on the side towards the middle: inside it the fill would run over the figure.
                let sw = self.text.width(Face::Body, label_px(24), &shown);
                let tx = if left { bx + bw + u(14) } else { bx - u(14) - sw };
                self.text.draw(c, Face::Body, label_px(24), tx, by + bh / 2 + pocket_ui::type_scale::mid(label_px(24)), pal().text, &shown);
            }
            if moved(trigger) {
                badges.push((bx + bw - bh / 4, by + bh / 6));
            }
        }
        // On the body the controls are the page's colour in Light (the pill's would vanish in the body's), the pill's in Dark.
        let base = pal().pick(pal().pill, pal().void);
        // D-pad.
        let (dx, dy, arm, reach) = (230, 300, 56, 90);
        let cross = [
            (Button::Up, (dx - arm / 2, dy - reach, arm, reach - arm / 2 + 2)),
            (Button::Down, (dx - arm / 2, dy + arm / 2 - 2, arm, reach - arm / 2 + 2)),
            (Button::Left, (dx - reach, dy - arm / 2, reach - arm / 2 + 2, arm)),
            (Button::Right, (dx + arm / 2 - 2, dy - arm / 2, reach - arm / 2 + 2, arm)),
        ];
        c.rect(px(dx - arm / 2), py(dy - arm / 2), u(arm), u(arm), base);
        for (b, (x, y, w, h)) in cross {
            c.round_rect(px(x), py(y), u(w), u(h), u(14), if pad.is_down(b) { lit } else { base });
            if moved(b) {
                badges.push((px(x + w) - u(10), py(y) + u(8)));
            }
        }
        // Face buttons: each cap says the letter it has in the menus (the player's layout and the Button layout).
        let (fx, fy, off, r) = (770, 300, 80, 38);
        for (b, cx, cy) in [(Button::North, fx, fy - off), (Button::South, fx, fy + off), (Button::West, fx - off, fy), (Button::East, fx + off, fy)] {
            let down = pad.is_down(b);
            c.disc(px(cx), py(cy), u(r), if down { lit } else { base });
            let letter = button_test::menu_name(map.menus(b));
            let font = label_px(34);
            let tw = self.text.width(Face::Body, font, letter);
            self.text.draw(c, Face::Body, font, px(cx) - tw / 2, py(cy) + pocket_ui::type_scale::mid(font), if down { on_lit } else { pal().text }, letter);
            if moved(b) {
                badges.push((px(cx + r - 6), py(cy - r + 6)));
            }
        }
        // SELECT, START and HOME: chips as wide as their words need.
        for (b, cx, y) in [(Button::Select, 405, 190), (Button::Start, 595, 190), (Button::Mode, 500, 262)] {
            let down = pad.is_down(b);
            let font = label_px(22);
            let tw = self.text.width(Face::Body, font, b.label());
            let (w, h) = (u(130).max(tw + 24), u(44).max(font as i32 + 16));
            let x = px(cx) - w / 2;
            c.round_rect(x, py(y), w, h, h / 2, if down { lit } else { base });
            self.text.draw(c, Face::Body, font, x + (w - tw) / 2, py(y) + h / 2 + pocket_ui::type_scale::mid(font), if down { on_lit } else { pal().text }, b.label());
            if moved(b) {
                badges.push((x + w - h / 4, py(y) + h / 5));
            }
        }
        // The Brick's two extra keys, F1 and F2, as chips between the shoulders: on the built-in pad of a Brick only.
        #[cfg(feature = "brick")]
        if pad.controller.is_builtin() && pocket_ui::device::brick() {
            for (b, cx) in [(Button::F1, 405), (Button::F2, 595)] {
                let down = pad.is_down(b);
                let font = label_px(22);
                let tw = self.text.width(Face::Body, font, b.label());
                let (w, h) = (u(110).max(tw + 24), u(44).max(font as i32 + 16));
                let x = px(cx) - w / 2;
                c.round_rect(x, py(66), w, h, h / 2, if down { lit } else { pal().pill });
                self.text.draw(c, Face::Body, font, x + (w - tw) / 2, py(66) + h / 2 + pocket_ui::type_scale::mid(font), if down { on_lit } else { pal().text }, b.label());
            }
        }
        // The sticks: the dead zone is the small square (the pad ignores what stays inside it), the dot is where the stick is. Not on
        // the built-in pad of a handheld without sticks (the Brick, the RG35XX Plus): two stick readouts there were sticks it hasn't.
        let sticks = !(pad.controller.is_builtin() && !pocket_ui::device::current().has_sticks());
        for (left, click, cx) in [(true, Button::L3, 360), (false, Button::R3, 640)].into_iter().filter(|_| sticks) {
            let (cx, cy, radius) = (px(cx), py(440), u(75));
            let down = pad.is_down(click);
            c.disc(cx, cy, radius, if down { mix(base, lit, 140) } else { base });
            c.ring(cx, cy, radius, 4, if down { lit } else { pal().chip });
            let dot = (radius / 4).max(6);
            let travel = (radius - dot - 4) as f32;
            let zone = (STICK_DEADZONE * travel) as i32;
            c.frame(cx - zone, cy - zone, 2 * zone + 1, 2 * zone + 1, 2, pal().dust);
            let (x, y) = pad.stick(left);
            let (x, y) = (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0));
            c.disc(cx + (x * travel) as i32, cy + (y * travel) as i32, dot, if pad.pushed(left) { lit } else { pal().dust });
            let shown = format!("{} · {}", (x * 100.0).round() as i32, (y * 100.0).round() as i32);
            let font = label_px(22);
            let tw = self.text.width(Face::Body, font, &shown);
            self.text.draw(c, Face::Body, font, cx - tw / 2, cy + radius + u(30), pal().dust, &shown);
            if moved(click) {
                badges.push((cx + radius - u(8), cy - radius + u(8)));
            }
        }
        for (bx, by) in badges {
            c.disc(bx, by, u(11).max(6), pal().void);
            c.disc(bx, by, u(8).max(4), pal().chalk);
        }
        // What the marks mean, only where they are there.
        let font = label_px(22);
        if pad.trigger(true).analog || pad.trigger(false).analog {
            let line = format!("A trigger counts as pressed from {}%", (TRIGGER_PRESS * 100.0) as u32);
            let tw = self.text.width(Face::Body, font, &line);
            self.text.draw(c, Face::Body, font, px(500) - tw / 2, py(618), pal().dust, &line);
        }
    }
}
