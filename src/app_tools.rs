//! Part of the pocket-ui binary: the App's side of `pocket_ui::tools` and `pocket_ui::tools_page`.
//! Settings > Tools: ROCKNIX's tools (the emulators' own menus, RetroArch's menu, the gamepad test and calibration,
//! the file manager, the terminal, …) and the Store's, started without EmulationStation.
//!
//! The list is found on a thread when the page opens (after Store tools left ROCKNIX's folder, `store::migrate_tools_in`).
//! A ROCKNIX tool starts like PC streaming: its own process group, its windows fullscreen (`control::games_open_fullscreen`),
//! an app without pause menu, and the page comes back when it ends. Holding HOME for a second stops it (`tool_home`), for
//! tools that offer no way out with the pad. A Store tool runs on a thread without a window.

use std::path::Path;
use std::time::{Duration, Instant};

use pocket_ui::control;
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::palette::*;
use pocket_ui::quit::Escalation;
use pocket_ui::text::Face;
use pocket_ui::tools::{self, Risk, Tool};
use pocket_ui::tools_page::{Action, Event, ToolsPage};
use pocket_ui::touch::Area;

use super::{App, Running, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Tools";
/// Below the list: what the selected tool is for and how to leave it.
const DETAILS_H: i32 = 280;

/// What the App keeps: the open page, when HOME went down while a tool runs, and the Settings page an app was started
/// from.
#[derive(Default)]
pub(crate) struct ToolsUi {
    page: Option<ToolsPage>,
    home_down: Option<Instant>,
    back_to: Option<SettingsPage>,
}

impl App {
    pub(crate) fn open_tools(&mut self) -> Option<SettingsPage> {
        let page = self.tools.page.take().unwrap_or_default();
        self.tools.page = Some(page);
        self.scan_tools();
        Some(SettingsPage::Tools)
    }

    /// Finds the tools on a thread: a few small files, never on the UI thread.
    fn scan_tools(&mut self) {
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        std::thread::spawn(move || {
            pocket_ui::store::migrate_tools_in(&data_dir);
            let dirs: Vec<&Path> = tools::ROCKNIX_DIRS.iter().map(Path::new).collect();
            let found = tools::scan(&dirs, &data_dir);
            let _ = proxy.send_event(UserEvent::Tools(Event::Scanned(found)));
        });
    }

    pub(crate) fn tools_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.tools.page.as_mut() else { return back };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                // A Store tool still running reports to a page that is gone: that is fine, its result is dropped.
                self.tools.page = None;
                return back;
            }
            Action::Rescan => self.scan_tools(),
            Action::Launch(tool) => {
                self.start_tool(tool);
                // Settings input sets the page from this: none while the tool runs (`settings_aside_for_app`).
                return None;
            }
            Action::RunQuiet(tool) => {
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    println!("tools: running {} ({})", tool.name, tool.script.display());
                    let result = tools::run_quiet(&tool.script);
                    let _ = proxy.send_event(UserEvent::Tools(Event::Ran { name: tool.name, result }));
                });
            }
        }
        Some(SettingsPage::Tools)
    }

    pub(crate) fn tools_event(&mut self, e: Event) {
        if let Some(page) = self.tools.page.as_mut() {
            page.event(e);
        }
    }

    /// Where the page's selection stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn tools_row(&self) -> Option<(usize, Option<usize>, bool)> {
        self.tools.page.as_ref().map(|p| (p.sel, p.confirm, p.busy.is_some()))
    }

    /// A Store tool runs: the dots move.
    pub(crate) fn tools_animating(&self) -> bool {
        self.tools.page.as_ref().is_some_and(|p| p.busy.is_some() || p.tools.is_none())
    }

    /// An app (a tool, a PC game) starts from a Settings page: the page steps aside while it runs, or the buttons
    /// pressed in the app would reach the page (Settings input comes before a running game's), and comes back when it
    /// ends (`settings_after_app`).
    pub(crate) fn settings_aside_for_app(&mut self) {
        self.tools.back_to = self.settings_page.take();
    }

    /// The app has ended: the Settings page it was started from is shown again, with `why` it ended at once on it, and
    /// after a tool the controls are made sure of (`tools::after_tool`). Returns `why` when the app was not started from
    /// a Settings page (Home shows it as a notice).
    pub(crate) fn settings_after_app(&mut self, why: Option<String>) -> Option<String> {
        let Some(p) = self.tools.back_to.take() else { return why };
        self.settings_page = Some(p);
        match p {
            SettingsPage::Tools => {
                tools::after_tool();
                if let (Some(w), Some(page)) = (why, self.tools.page.as_mut()) {
                    page.message = Some((w, true));
                }
            }
            SettingsPage::PcStreaming => {
                if let Some(w) = why {
                    self.pc_streaming_note(w);
                }
            }
            _ => return why,
        }
        None
    }

    /// Starts a ROCKNIX tool the way PC streaming starts: an app with its own process group; the page stays open
    /// behind it and is there again when it ends.
    fn start_tool(&mut self, tool: Tool) {
        println!("tools: starting {} ({})", tool.name, tool.script.display());
        #[cfg(target_os = "linux")]
        control::rules_in_place();
        pocket_ui::ui_sound::stop();
        self.cover_run.game_started();
        self.perf_fan_before_game();
        self.settings_aside_for_app();
        let pgid = super::spawn_app(self.proxy.clone(), tools::command(&tool, Path::new(tools::TERMINAL), &self.data_dir.join("locale")));
        control::note_game(pgid);
        self.running = Some(Running {
            system: "tools".into(),
            name: tool.name,
            path: tool.script,
            emulator: tools::EMULATOR.into(),
            core: String::new(),
            started: Instant::now(),
            pgid,
            options: Default::default(),
        });
        self.launch_at = Some(Instant::now());
        self.held = None;
        self.tools.home_down = None;
    }

    /// The pad while a tool runs (it is an app: the pad belongs to it). Holding HOME for a second and letting go stops
    /// the tool: SIGTERM to its process group, SIGKILL two seconds later (`quit::Escalation`, as Quit does for games).
    pub(crate) fn tool_home(&mut self, pe: &PadEvent) {
        if self.running.as_ref().is_none_or(|r| r.emulator != tools::EMULATOR) {
            return;
        }
        let PadEvent::Button { button: Button::Mode, pressed, .. } = *pe else { return };
        if pressed {
            self.tools.home_down = Some(Instant::now());
        } else if let Some(down) = self.tools.home_down.take() {
            if down.elapsed().as_millis() >= tools::HOLD_TO_STOP_MS && self.quit.is_none() {
                println!("tools: HOME held, stopping {}", self.running.as_ref().map_or("", |r| r.name.as_str()));
                self.quit = Some(Escalation::new(Instant::now(), Duration::ZERO));
            }
        }
    }

    /// `--snapshot tools[:<row>][:confirm|:busy|:done|:empty|:loading]`: the page with the demo tools.
    pub(crate) fn tools_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("tools") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let first = parts.next().unwrap_or("");
        let (row, mode) = match first.parse::<usize>() {
            Ok(r) => (r, parts.next().unwrap_or("")),
            Err(_) => (0, first),
        };
        let mut page = ToolsPage::new();
        if mode != "loading" {
            page.event(Event::Scanned(if mode == "empty" { Vec::new() } else { tools::demo() }));
        }
        page.sel = row.min(page.len().saturating_sub(1));
        let find = |p: &ToolsPage, name: &str| p.tools.as_ref().and_then(|t| t.iter().position(|t| t.name == name)).unwrap_or(0);
        match mode {
            "confirm" => {
                page.sel = find(&page, "Cloud backup");
                page.press(Button::East, Button::East, Button::South);
            }
            "busy" => {
                page.sel = find(&page, "Storage report");
                page.press(Button::East, Button::East, Button::South);
            }
            "done" => {
                page.sel = find(&page, "Storage report");
                page.event(Event::Ran { name: "Storage report".into(), result: Ok("Free: 41.2 GB of 238 GB, 1,204 games".into()) });
            }
            _ => {}
        }
        self.tools.page = Some(page);
        self.settings_page = Some(SettingsPage::Tools);
        true
    }

    pub(crate) fn paint_tools(&mut self, c: &mut Canvas) {
        let Some(page) = self.tools.page.take() else { return };
        self.paint_tools_page(c, &page);
        self.tools.page = Some(page);
    }

    fn paint_tools_page(&mut self, c: &mut Canvas, page: &ToolsPage) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let list_bottom = c.h - pad - NU_PILL - 16 - DETAILS_H;
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        let tools: &[Tool] = page.tools.as_deref().unwrap_or(&[]);
        if tools.is_empty() {
            let (head, line) = if page.tools.is_none() {
                (format!("Looking for tools{dots}"), String::new())
            } else {
                ("No tools found".to_string(), format!("ROCKNIX keeps its tools in {}. X looks again.", pocket_ui::store::ROCKNIX_MODULES))
            };
            // An empty page like the others (`kit::empty_state`): the magnifying glass by an empty box once the folder
            // has been looked at.
            let (left, base) = (pad + NU_BTN_PAD, top + 60 + pocket_ui::type_scale::grow(37));
            let art = page.tools.is_some().then_some(pocket_ui::ui_art::Art::Search);
            pocket_ui::kit::empty_state(c, &mut self.text, art, (left, c.w - 2 * left), top + 8, base, c.h - pad - NU_PILL - 16, &head, &line);
            self.nu_hints(c, &[("B", "Back"), ("X", "Look again")]);
            return;
        }
        let visible = (((list_bottom - top) / row) as usize).max(3);
        let first = super::window_start(page.sel, visible, tools.len());
        if tools.len() > visible {
            let track = visible as i32 * row;
            pocket_ui::kit::scrollbar(c, top, track, first, visible, tools.len());
        }
        for (i, t) in tools.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let value = if page.busy == Some(i) { format!("Running{dots}") } else { t.group.label().to_string() };
            let value_w = self.text.width(Face::Body, font, &value) + 40;
            let name = self.text.fit(Face::Body, font, &t.name, right - pad - 2 * NU_BTN_PAD - value_w);
            let selected = i == page.sel;
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            let quiet = !selected && t.group != tools::Group::Store;
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, if quiet { pal().dust } else { pal().text }, &value);
        }
        self.hits.list(page.sel, tools.len(), 0.0, row as f32);
        if let Some(t) = page.selected() {
            self.paint_tool_details(c, page, t, list_bottom + 14);
        }
        let hints = [("B", "Back"), ("A", page.a_hint()), ("X", "Look again")];
        self.nu_hints(c, &hints);
    }

    /// Below the list: what the tool is for and how to leave it, or its warning, or what a Store tool printed.
    fn paint_tool_details(&mut self, c: &mut Canvas, page: &ToolsPage, t: &Tool, y0: i32) {
        let left = NU_PAD + NU_BTN_PAD;
        let width = c.w - NU_PAD - NU_BTN_PAD - left;
        c.rect(left, y0, width, 2, pal().spine);
        let mut y = y0 + 58;
        let confirming = page.confirm == Some(page.sel);
        let head = if confirming { format!("Start {}?", t.name) } else { t.name.clone() };
        let head = self.text.fit(Face::Title, 44, &head, width);
        self.text.draw(c, Face::Title, 44, left, y, if confirming { pal().alert } else { pal().chalk }, &head);
        y += 50;
        let lines: Vec<(String, u32)> = match (&page.message, &t.risk) {
            _ if confirming => {
                let warn = match &t.risk {
                    Risk::Warn(w) => w.clone(),
                    Risk::Safe => String::new(),
                };
                vec![(warn, pal().text), ("Press A again to start it. B keeps it closed.".into(), pal().dust)]
            }
            (Some((msg, err)), _) if page.busy.is_none() => vec![(msg.clone(), if *err { pal().alert } else { pal().text })],
            (_, risk) => {
                let mut v = vec![(t.about.clone(), pal().text)];
                if let Risk::Warn(_) = risk {
                    v.push(("It asks once more before it starts.".into(), pal().dust));
                }
                v.push((t.leave.to_string(), pal().dust));
                v
            }
        };
        for (text, color) in lines {
            for line in self.text.wrap(Face::Body, NU_SMALL, &text, width).into_iter().take(2) {
                if y > y0 + DETAILS_H - 20 {
                    return;
                }
                self.text.draw(c, Face::Body, NU_SMALL, left, y, color, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
        }
    }
}
