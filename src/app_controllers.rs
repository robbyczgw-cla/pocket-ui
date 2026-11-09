//! Controller setup uses the individual source before Mux: a second pad cannot steal a captured mapping.
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::{bindings::{self, Capture, CaptureStep, Layout, BUTTONS}, Button, PadEvent};
use std::time::Instant;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;
use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_PAD, NU_PILL};

#[derive(Default)]
pub(crate) struct Setup {
    key: String,
    capture: Option<Capture>,
    pub(crate) note: String,
    saving: bool,
    /// Return to the game's Controls page instead of Settings.
    pause: bool,
    order: Vec<String>,
}

pub(crate) fn button_name(b: Button) -> &'static str {
    match b {
        Button::South => "Bottom button", Button::East => "Right button",
        Button::North => "Top button", Button::West => "Left button",
        Button::Up => "D-pad up", Button::Down => "D-pad down",
        Button::Left => "D-pad left", Button::Right => "D-pad right",
        Button::Select => "Select", Button::Start => "Start",
        other => other.label(),
    }
}

impl App {
    pub(crate) fn controllers_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("controllers:") else { return false };
        for (source, key, name) in [(900, "builtin", "Pocket"), (901, "bt:demo-pad-one", "8BitDo SN30 Pro"), (902, "bt:demo-pad-two", "8BitDo SN30 Pro")] {
            self.pad.merge(PadEvent::Connected { source, controller: pocket_ui::pad::Controller { key:key.into(),name:name.into(),gamepad:true,node:String::new() } });
        }
        self.open_controllers(false);
        self.controller_setup.key = "bt:demo-pad-one".into();
        self.controller_setup.order = vec!["builtin".into(),"bt:demo-pad-one".into(),"bt:demo-pad-two".into()];
        let mut parts = rest.split(':');
        let row = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        match parts.next() {
            #[cfg(feature = "brick")]
            Some(which @ ("brick-f1" | "brick-f2")) => {
                self.controller_setup.key = "builtin".into();
                self.controller_setup.capture = Some(Capture::new(Button::South, Instant::now()));
                // This snapshot represents the verified plain Brick, including its physical identity;
                // the host has no stock MainUI from which the live reader could establish that.
                let mut decode = pocket_ui::pad::decode::Decoder::with_caps_profile((3, 0x045e, 0x028e, 0x0114),
                    "TRIMUI Player1", vec![304,305,307,308,310,311,314,315,316,317,318], vec![], true);
                let code = if which == "brick-f1" { 317 } else { 318 };
                for down in [1, 0] {
                    for e in decode.event(900, evdev::InputEvent::new(1, code, down)) { self.controller_capture(&e); }
                }
                assert_eq!(bindings::Profiles::load(&self.data_dir).layout("builtin"), Layout::default(), "shortcuts must not change game controls");
            }
            Some("capture-proof") => {
                self.controller_setup.capture = Some(Capture::new(Button::South, Instant::now()));
                // The same normalized events the app receives from a Pro 2's brake trigger.
                let mut decode = pocket_ui::pad::decode::Decoder::with_caps(5, 0x2dc8, 0x6006, 0x100, "8BitDo Pro 2", (304..=318).collect(), vec![(0,0,255),(1,0,255),(2,0,255),(5,0,255),(9,0,1023),(10,0,1023),(16,-1,1),(17,-1,1)]);
                for e in decode.event(902, evdev::InputEvent::new(3,10,1023)) { self.controller_capture(&e); }
                assert_eq!(self.controller_setup.capture.map(|c| c.target), Some(Button::South), "another pad cannot steal capture");
                for e in decode.event(901, evdev::InputEvent::new(3,10,0)) { self.controller_capture(&e); }
                for e in decode.event(901, evdev::InputEvent::new(3,10,1023)) { self.controller_capture(&e); }
                assert!(self.controller_setup.capture.is_none(), "the selected pad's analogue trigger was captured");
                for _ in 0..200 {
                    if bindings::Profiles::load(&self.data_dir).layout("bt:demo-pad-one").role(Button::L2) == Button::South { break; }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                let saved = bindings::Profiles::load(&self.data_dir);
                assert_eq!(saved.layout("bt:demo-pad-one").role(Button::L2), Button::South);
                assert_eq!(saved.layout("bt:demo-pad-one").role(Button::South), Button::L2, "occupied action swapped");
                assert_eq!(saved.layout("bt:demo-pad-two"), Layout::default(), "the other identical pad is unchanged");
                self.controller_saved("Buttons swapped. Every action still has a button.".into());
            }
            Some("capture") => self.controller_setup.capture = Some(Capture::new(Button::L2, Instant::now())),
            // The Pocket's own B leaves a Bluetooth pad's capture, and a capture nobody answers ends by itself.
            Some("capture-escape") => {
                let now = Instant::now();
                self.controller_setup.capture = Some(Capture::new(Button::L2, now));
                self.controller_capture(&PadEvent::Button { source: 902, button: Button::South, pressed: true });
                assert!(self.controller_setup.capture.is_some(), "another pad's button doesn't end it");
                self.controller_capture(&PadEvent::Button { source: 900, button: super::CANCEL, pressed: true });
                assert!(self.controller_setup.capture.is_none(), "the Pocket's B ended the capture");
                self.controller_setup.capture = Some(Capture::new(Button::L2, now));
                assert_eq!(self.controller_capture_tick(now), Some(now + bindings::CAPTURE_WAIT), "a wake-up for the timeout");
                assert_eq!(self.controller_capture_tick(now + bindings::CAPTURE_WAIT), None);
                assert!(self.controller_setup.capture.is_none(), "no press within 8 s: the capture ended");
                assert_eq!(bindings::Profiles::load(&self.data_dir).layout("bt:demo-pad-one"), Layout::default(), "nothing changed");
            }
            Some("swapped") => self.controller_setup.note = "Buttons swapped. Every action still has a button.".into(),
            Some("disconnected") => { self.pad.merge(PadEvent::Disconnected { source:901 }); self.controller_setup.note = "Controller disconnected. Reconnect and try again.".into(); }
            _ => {}
        }
        self.settings_page = Some(SettingsPage::Controllers(row));
        true
    }

    pub(crate) fn open_controllers(&mut self, pause: bool) {
        let controllers = self.pad.controllers();
        self.controller_setup = Setup { key: controllers.iter().find(|c| c.gamepad).map(|c| c.key.clone()).unwrap_or_default(), pause, order: pocket_ui::players::Order::load(&self.data_dir).order, ..Default::default() };
        self.settings_page = Some(SettingsPage::Controllers(0));
    }

    pub(crate) fn controller_capture(&mut self, pe: &PadEvent) -> bool {
        if !matches!(self.settings_page, Some(SettingsPage::Controllers(_))) { return false; }
        let Some(target) = self.controller_setup.capture else { return false };
        if let PadEvent::Disconnected { source } = pe {
            if self.pad.controller_of(*source).key == self.controller_setup.key {
                self.controller_setup.capture = None;
                self.controller_setup.note = "Controller disconnected. Reconnect and try again.".into();
                self.request_redraw();
            }
        }
        let PadEvent::Button { source, button, pressed: true } = *pe else { return false };
        let key = self.pad.controller_of(source).key;
        match target.press(&key, &self.controller_setup.key, button) {
            CaptureStep::Cancel => {
                self.controller_setup.capture = None;
                self.controller_setup.note.clear();
                self.request_redraw();
                return true;
            }
            CaptureStep::Swallow => return true,
            CaptureStep::Take => {}
        }
        let target = target.target;
        // Button layout is an involution; the device layout is a permutation. Undo both to capture physical input.
        let mut layout = bindings::layout(&key);
        let physical = layout.physical(self.button_layout.role(button));
        #[cfg(feature = "brick")]
        if pocket_ui::brick_profile::fkeys() && matches!(physical, Button::F1 | Button::F2) {
            // They bypass the game's RetroPad mapping. End capture with the supported setting instead of timing out.
            self.controller_setup.capture = None;
            self.controller_setup.note = pocket_ui::brick_hw::FKEYS_REMAP_NOTE.into();
            self.request_redraw();
            return true;
        }
        if !BUTTONS.contains(&physical) { return true; }
        let swapped = layout.assign(physical, target).is_some();
        self.controller_setup.capture = None;
        self.save_controller_layout(layout, swapped);
        true
    }

    /// Called every turn of the loop: a capture nobody answers ends after `CAPTURE_WAIT`, and one left behind by
    /// leaving the page ends at once. The time of the next look while one waits.
    pub(crate) fn controller_capture_tick(&mut self, now: Instant) -> Option<Instant> {
        let capture = self.controller_setup.capture?;
        if !matches!(self.settings_page, Some(SettingsPage::Controllers(_))) {
            self.controller_setup.capture = None;
            return None;
        }
        if !capture.expired(now) {
            return Some(capture.until);
        }
        self.controller_setup.capture = None;
        self.controller_setup.note = "No button pressed. Nothing changed.".into();
        self.request_redraw();
        None
    }

    fn save_controller_layout(&mut self, layout: Layout, swapped: bool) {
        self.controller_setup.saving = true;
        self.controller_setup.note = "Saving controller layout…".into();
        let (dir, key, proxy) = (self.data_dir.clone(), self.controller_setup.key.clone(), self.proxy.clone());
        std::thread::spawn(move || {
            let note = match bindings::save_layout(&dir, &key, layout) {
                Ok(()) if swapped => "Buttons swapped. Every action still has a button.".to_string(),
                Ok(()) => "Controller layout saved.".to_string(),
                Err(_) => "Could not save the controller layout. Try again.".to_string(),
            };
            let _ = proxy.send_event(UserEvent::ControllerSaved(note));
        });
        self.request_redraw();
    }

    pub(crate) fn controller_saved(&mut self, note: String) {
        self.controller_setup.saving = false;
        self.controller_setup.note = note;
        self.request_redraw();
    }

    pub(crate) fn controllers_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        if self.controller_setup.capture.is_some() { return Some(SettingsPage::Controllers(sel)); }
        // B leaves while a layout is still being written: the worker's note just isn't seen.
        if b == CANCEL {
            return if self.controller_setup.pause { None } else { Some(SettingsPage::Buttons(2)) };
        }
        let n = BUTTONS.len() + 2;
        let sel = sel.min(n - 1);
        match b {
            Button::Up => return Some(SettingsPage::Controllers((sel + n - 1) % n)),
            Button::Down => return Some(SettingsPage::Controllers((sel + 1) % n)),
            _ => {}
        }
        if self.controller_setup.saving { return Some(SettingsPage::Controllers(sel)); }
        if b == CONFIRM || matches!(b, Button::Left | Button::Right) {
            let pads: Vec<_> = self.pad.controllers().into_iter().filter(|c| c.gamepad).collect();
            if sel == 0 && !pads.is_empty() {
                let at = pads.iter().position(|c| c.key == self.controller_setup.key).unwrap_or(0);
                let next = if b == Button::Left { (at + pads.len() - 1) % pads.len() } else { (at + 1) % pads.len() };
                self.controller_setup.key = pads[next].key.clone();
                self.controller_setup.note.clear();
            } else if pads.iter().any(|c| c.key == self.controller_setup.key) {
                if sel == n - 1 { self.save_controller_layout(Layout::default(), false); }
                else { self.controller_setup.capture = Some(Capture::new(BUTTONS[sel - 1], Instant::now())); self.controller_setup.note.clear(); }
            }
        }
        Some(SettingsPage::Controllers(sel))
    }

    pub(crate) fn paint_controllers(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        self.sheet_header(c, "Controller setup");
        let pads = self.pad.controllers();
        let controller = pads.iter().find(|c| c.key == self.controller_setup.key);
        let name = controller.map(|c| c.name.clone()).unwrap_or_else(|| "Controller disconnected".into());
        let seats = pocket_ui::players::Seats::arrange(&self.controller_setup.order, &pads, pocket_ui::players::Mode::Compact);
        let player = seats.player_of(&self.controller_setup.key).map(|p| format!("Player {}", p + 1)).unwrap_or_else(|| "No player".into());
        let layout = bindings::layout(&self.controller_setup.key);
        let note = if let Some(capture) = self.controller_setup.capture {
            format!("{}: {}.\nPress a button on {name} · HOME on any pad cancels.", name, pocket_ui::i18n::tr(button_name(capture.target)))
        } else if !self.controller_setup.note.is_empty() { self.controller_setup.note.clone() }
        else { "Choose a controller, then an action. A captures its button.\nAn occupied button swaps with the previous one.".into() };
        // With one controller its row has nothing to change to: no A there (A and Left/Right would do nothing).
        let one = pads.iter().filter(|c| c.gamepad).count() < 2;
        let hints: &[(&str, &str)] = if sel == 0 && one { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Change")] };
        let bottom = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, hints, &note, 4, pal().text);
        let (pad, top, row, font) = (NU_PAD, NU_PAD + NU_PILL + 12, pocket_ui::type_scale::row(66, 36), 36);
        let n = BUTTONS.len() + 2;
        let visible = ((bottom - top) / row).max(1) as usize;
        let first = super::window_start(sel, visible, n);
        let identity = if controller.is_some_and(|c| c.is_builtin()) { "Pocket".to_string() } else {
            self.controller_setup.key.chars().rev().take(8).collect::<String>().chars().rev().collect()
        };
        for i in first..(first + visible).min(n) {
            let (label, value) = if i == 0 { (format!("{name} · {identity}"), player.clone()) }
                else if i == n - 1 { ("Reset controller layout".into(), String::new()) }
                else { let b = BUTTONS[i - 1]; (button_name(b).into(), button_name(layout.physical(b)).into()) };
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, c.w - 2 * pad, row), i);
            let width = self.text.width(Face::Body, font, &value);
            let label = self.text.fit(Face::Body, font, &label, c.w - 2 * pad - width - 60);
            let base = pocket_ui::kit::row_label(c, &mut self.text, (pad, y, c.w - 2 * pad, row), font, &label, i == sel);
            pocket_ui::kit::row_value(c, &mut self.text, font, c.w - pad - 18, base, &value, i == sel);
        }
        if n > visible { pocket_ui::kit::scrollbar(c, top, bottom - top, first, visible, n); }
        self.hits.list(sel, n, first as f32 * row as f32, row as f32);
    }
}
