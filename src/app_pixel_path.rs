//! Part of the pocket-ui binary: the pause menu's Pixel Path page (`pocket_ui::pixel_path`, `pixel_path_page`).
//!
//! The page asks pocket-core for a report every `pixel_path::REFRESH_MS` from a thread of its own, which also waits for
//! the answer and reads it; the UI thread only gets a new report when something in it changed, and paints. When the
//! page is closed (B, Continue, Quit, the menu going away) the page drops and the thread ends with its next round:
//! nothing is asked, read or drawn for Pixel Path then.
//!
//! A standalone emulator's game (ARMSX2, Dolphin, Azahar) has no report: its stages come from the emulator's settings
//! files (`pocket_ui::pixel_standalone`), read on a thread when the pause menu opens (and once a while after the game
//! started). The menu lists Pixel Path for such a game only when the last reading found something; RetroArch and the
//! other emulators have none to read, so it isn't listed for them (the log says why).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use pocket_ui::control;
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::pad::Button;
use pocket_ui::pixel_path::{self, Grade, Known, Report};
use pocket_ui::pixel_path_page::{Action, PixelPathPage};
use pocket_ui::pixel_standalone::{self, Screen, Setup};
use pocket_ui::standalone;
use pocket_ui::text::Face;

use super::{App, MainItem, Page, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// How long the reader waits for pocket-core's answer before it asks again.
const ANSWER_WITHIN: Duration = Duration::from_millis(400);

/// How much darker the game's picture is behind the Pixel Path page than behind the rest of the pause menu (225):
/// its small grey text over a bright game frame was hard to read. A little, not a black page.
pub(crate) const PAGE_FADE: u8 = 236;

/// The emulator's name as people know it, for the log.
fn emulator_name(id: &str) -> String {
    match id {
        "retroarch" => "RetroArch".into(),
        "armsx2" => "ARMSX2".into(),
        "dolphin" | "dolphin-sa" => "Dolphin".into(),
        "azahar" => "Azahar".into(),
        "ppsspp" | "ppsspp-sa" => "PPSSPP".into(),
        "flycast" | "flycast-sa" => "Flycast".into(),
        "duckstation" | "duckstation-sa" => "DuckStation".into(),
        "melonds" | "melonds-sa" => "melonDS".into(),
        "mupen64plus" | "mupen64plus-sa" => "Mupen64Plus".into(),
        "" => "its emulator".into(),
        other => {
            let mut c = other.trim_end_matches("-sa").chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        }
    }
}

/// Starts the reader: `pixelpath <seq>` on pocket-core's command FIFO, then the answer for this game and number.
fn start_reader(page: &PixelPathPage, proxy: winit::event_loop::EventLoopProxy<UserEvent>, rom: PathBuf) {
    let alive = page.alive.clone();
    let spawned = std::thread::Builder::new().name("pixel-path".into()).spawn(move || {
        let file = Path::new(pixel_path::FILE);
        let mut seq = 0u32;
        let mut last: Option<Report> = None;
        while alive.load(Ordering::Relaxed) {
            let round = Instant::now();
            seq = seq.wrapping_add(1);
            if control::pocket_core(&format!("{} {seq}", pixel_path::COMMAND)) {
                // pocket-core answers between frames, or within 30 ms while paused.
                while alive.load(Ordering::Relaxed) && round.elapsed() < ANSWER_WITHIN {
                    if let Some(r) = Report::read(file, &rom, seq) {
                        // Only what changed reaches the UI thread: a paused game's numbers stay the same.
                        if !last.as_ref().is_some_and(|l| l.same_as(&r)) {
                            last = Some(r.clone());
                            if proxy.send_event(UserEvent::PixelPath(Box::new(r))).is_err() {
                                return;
                            }
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            let until = round + Duration::from_millis(pixel_path::REFRESH_MS);
            while alive.load(Ordering::Relaxed) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("pixel path: can't start the reader: {e}");
    }
}

/// Over the game is Pixel Path for the running game (the menu's values, saved or just changed).
fn over_game(s: &pocket_ui::options::GameSettings) -> bool {
    pocket_ui::options::setting(pocket_ui::over_game::KEY).is_some_and(|set| set.choices[s.index(set)].value == "pixel")
}

impl App {
    /// A on "Pixel Path" in the pause menu.
    pub(crate) fn open_pixel_path(&mut self) {
        let Some(run) = self.running.as_ref() else { return };
        let system_name = self.system_name(&run.system);
        let page = if run.hosted() {
            let p = PixelPathPage::new();
            start_reader(&p, self.proxy.clone(), run.path.clone());
            p
        } else if let Some(Ok(setup)) = pixel_standalone::cached(&run.path) {
            // Read when the menu opened (`pixel_item_at_open`); a newer reading still on its way updates the page.
            PixelPathPage::standalone(setup, &system_name, self.screen())
        } else {
            pocket_ui::ui_sound::refuse();
            return;
        };
        if let Some(menu) = self.menu.as_mut() {
            menu.pixel = Some(page);
            menu.page = Page::PixelPath;
        }
    }

    /// The panel as pocket-ui sees it: its window's size in front of the game, the output's refresh rate.
    fn screen(&self) -> Screen {
        let Some(w) = self.window.as_ref() else { return Screen::default() };
        let size = w.inner_size();
        let hz = w
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .map_or(Screen::default().hz, |mhz| mhz as f64 / 1000.0);
        Screen { w: size.width, h: size.height, hz }
    }

    /// Whether the pause menu that opens now lists Pixel Path: always for pocket-core's games; for a standalone
    /// emulator's game when its settings could be read last time (a new reading starts on a thread now and moves the
    /// item in or out when it comes, `pixel_setup_event`); never for RetroArch and the other emulators.
    pub(crate) fn pixel_item_at_open(&self) -> bool {
        if !self.pixel_path {
            return false;
        }
        let Some(run) = self.running.as_ref() else { return false };
        if run.hosted() {
            return true;
        }
        if standalone::session().is_some_and(|s| s.rom == run.path) {
            let proxy = self.proxy.clone();
            let spawned = std::thread::Builder::new().name("pixel-setup".into()).spawn(move || {
                if let Some((rom, result)) = pixel_standalone::refresh(&Default::default()) {
                    let _ = proxy.send_event(UserEvent::PixelSetup(rom, Box::new(result)));
                }
            });
            if let Err(e) = spawned {
                eprintln!("pixel path: can't start the settings reader: {e}");
            }
            return pixel_standalone::listed(&run.path);
        }
        println!("pixel path: not listed for {}: pocket-ui doesn't read its settings", emulator_name(&run.emulator));
        false
    }

    /// A new reading of the running standalone game's settings: the menu lists Pixel Path or not, as it says, keeping
    /// the selected row; an open page shows the new values.
    pub(crate) fn pixel_setup_event(&mut self, rom: PathBuf, result: Result<Setup, String>) {
        if !self.running.as_ref().is_some_and(|r| r.path == rom) {
            return;
        }
        let listed = self.pixel_path && result.is_ok();
        let Some(menu) = self.menu.as_mut() else { return };
        let mut changed = false;
        if menu.pixel_item != listed && menu.page != Page::PixelPath {
            let item = menu.item();
            menu.pixel_item = listed;
            menu.sel = menu.items().iter().position(|i| *i == item).unwrap_or(0);
            changed = true;
        }
        if let (Some(page), Ok(setup)) = (menu.pixel.as_mut(), result) {
            changed |= page.receive_setup(setup);
        }
        if changed {
            self.request_redraw();
        }
    }

    fn system_name(&self, system: &str) -> String {
        self.systems
            .iter()
            .find(|s| s.name == system)
            .map_or_else(|| system.to_uppercase(), |s| s.fullname.clone())
    }

    pub(crate) fn pixel_path_input(&mut self, b: Button) {
        let Some(menu) = self.menu.as_mut() else { return };
        let Some(p) = menu.pixel.as_mut() else {
            menu.page = Page::Main;
            return;
        };
        // Y: Pixel Path over the game, on or off (pocket-core games only).
        if b == Button::West && !p.about {
            let setting = pocket_ui::options::setting(pocket_ui::over_game::KEY);
            match (menu.settings.as_mut(), setting, p.standalone.is_none()) {
                (Some(s), Some(setting), true) => {
                    let value = p.toggle_over_game(setting.choices[s.index(setting)].value);
                    // As a change in Options: applied at once, kept with Save changes.
                    control::pocket_core(&format!("set {} {value}", pocket_ui::over_game::KEY));
                    s.values.insert(setting.key.to_string(), value);
                }
                _ => pocket_ui::ui_sound::refuse(),
            }
            return;
        }
        // Nothing to explain until pocket-core has answered.
        if b == CONFIRM && p.report.is_none() && p.standalone.is_none() {
            pocket_ui::ui_sound::refuse();
            return;
        }
        if let Action::Close = p.press(b, CONFIRM, CANCEL) {
            menu.pixel = None;
            menu.page = Page::Main;
            menu.sel = menu.items().iter().position(|i| *i == MainItem::PixelPath).unwrap_or(0);
        }
    }

    pub(crate) fn pixel_path_event(&mut self, r: Report) {
        let changed = self
            .menu
            .as_mut()
            .filter(|m| m.page == Page::PixelPath)
            .and_then(|m| m.pixel.as_mut())
            .is_some_and(|p| p.receive(r));
        if changed {
            self.request_redraw();
        }
    }

    /// `--snapshot pixel-path:<gba|snes|patched|n64|runahead|compromised|pal|bluetooth|armsx2|dolphin|azahar|reading>[:<stage>[:about]]`;
    /// armsx2, dolphin and azahar read the sample settings of `testdata/pixel-path` (`pixel_standalone::demo`).
    pub(crate) fn pixel_path_snapshot(&mut self, case: &str, sel: usize, about: bool) -> Option<()> {
        let mut p = match case {
            "armsx2" | "dolphin" | "azahar" => {
                let (setup, console) = pixel_standalone::demo(case)?;
                PixelPathPage::standalone(setup, console, Screen::default())
            }
            "reading" => PixelPathPage::new(),
            _ => {
                let (r, _) = pixel_path::demo(case)?;
                let mut p = PixelPathPage::new();
                p.receive(r);
                p
            }
        };
        p.alive.store(false, Ordering::Relaxed);
        p.sel = sel.min(pixel_path::Kind::ALL.len() - 1);
        p.about = about;
        p.known = pixel_path::demo(case).map(|(_, k)| k);
        if let Some(menu) = self.menu.as_mut() {
            menu.pixel = Some(p);
            menu.page = Page::PixelPath;
        }
        Some(())
    }

    /// `--snapshot menu:ps2[:<row>]:pixel=<armsx2|missing>`: a PS2 game's pause menu with Pixel Path switched on and
    /// ARMSX2's settings read from `testdata/pixel-path/armsx2` (listed, and selected) or from a folder without them
    /// (not listed: the log says why), the way `pixel_item_at_open` decides.
    pub(crate) fn pixel_menu_snapshot(&mut self, state: &str) {
        let Some(case) = state.split(':').find_map(|p| p.strip_prefix("pixel=")) else { return };
        let result = match case {
            "armsx2" => pixel_standalone::demo("armsx2").map(|(s, _)| s).ok_or_else(|| "no sample settings".to_string()),
            _ => {
                let nowhere = std::env::temp_dir().join("pocketui-snapshot-no-armsx2");
                let files = pocket_ui::emu_settings::Files { system_cfg: nowhere.join("system.cfg"), root: nowhere };
                pixel_standalone::read(standalone::Emu::Armsx2, "ps2", Path::new("game.chd"), None, &files)
            }
        };
        if let Err(why) = &result {
            println!("pixel path: not listed for ARMSX2: {why}");
        }
        if let Some(m) = self.menu.as_mut() {
            m.pixel_item = result.is_ok();
            if let Some(i) = m.items().iter().position(|i| *i == MainItem::PixelPath) {
                m.sel = i;
            }
        }
    }

    /// What pocket-ui adds to pocket-core's report: the system's name, the cheats switched on.
    fn pixel_known(&self) -> Known {
        if let Some(k) = self.menu.as_ref().and_then(|m| m.pixel.as_ref()).and_then(|p| p.known.clone()) {
            return k;
        }
        let system_name = self.running.as_ref().map(|r| self.system_name(&r.system)).unwrap_or_default();
        let cheats = self.menu.as_ref().and_then(|m| m.cheats.as_ref()).map_or(0, |c| c.enabled());
        Known { system_name, cheats }
    }

    /// The chain: a dot per stage on a thin line, its name, the facts and a line on what they mean, the grade at the
    /// right; the last seconds of play under it, the legend and the hints at the bottom. A opens a stage's paragraph.
    pub(crate) fn paint_pixel_path(&mut self, c: &mut Canvas) {
        let known = self.pixel_known();
        let Some(page) = self.menu.as_ref().and_then(|m| m.pixel.as_ref()) else {
            return;
        };
        let path = page.path(&known);
        let (sel, about, managed) = (page.sel, page.about, page.standalone.is_some());
        let pad = NU_PAD;
        self.status_pill(c);

        // Heading: "Pixel Path", then the overall grade with its dot.
        let head_base = pad + (NU_PILL - 20) / 2 + pocket_ui::type_scale::mid(NU_SMALL);
        let x = pad + NU_BTN_PAD;
        let w = self.text.draw(c, Face::Body, NU_SMALL, x, head_base, 0xcccccc, "Pixel Path");
        if let Some(p) = &path {
            let gx = x + w + 30;
            c.disc(gx + 8, head_base - 10, 8, p.overall.colour());
            let word = match (p.overall, p.partial) {
                (Grade::Unknown, _) => "Not graded",
                (_, true) => "Partly graded",
                (g, false) => g.name(),
            };
            // The grey of Not graded is a dot's colour; as text on black it would be too faint (2.9:1).
            let word_colour = if p.overall == Grade::Unknown { 0x9a978f } else { p.overall.colour() };
            self.text.draw(c, Face::Body, NU_SMALL, gx + 26, head_base, word_colour, word);
        }

        let top = pad + NU_PILL + 14;
        let right = c.w - pad;
        // Taller rows and a wider column of stage names with larger text (Settings → Text size); on a screen with
        // fewer than 1080 lines (the Mini V1's 960) the seven rows share what is left, so the last seconds of play
        // stay above the hints (docs/DEVICES.md).
        let full = 104 + pocket_ui::type_scale::grow(28);
        let row = full - ((1080 - c.h).max(0) + 6) / 7;
        // The dot, the value and its line move up by half of what a row gave up, and the line closer to the value;
        // with larger text there is no room for that, and only the selected stage keeps its line, where it was.
        let tight = row < full && pocket_ui::type_scale::grow(28) > 0;
        let (lift, line_dy) = if tight { (0, 40) } else { ((full - row) / 2, 40 - (full - row) / 2) };
        let (dot_x, name_x, value_x) = (pad + 34, pad + 74, pad + 74 + pocket_ui::type_scale::wide(162, 28));
        let Some(path) = path else {
            self.text.draw(c, Face::Body, 30, name_x, top + 60, 0x9a978f, "Reading the path…");
            self.nu_hints(c, &[("B", "Back")]);
            return;
        };
        for (i, s) in path.stages.iter().enumerate() {
            let y0 = top + i as i32 * row;
            let cy = y0 + 34 - lift;
            let colour = s.grade.colour();
            if i == sel && !about {
                c.round_rect(pad, y0 + 2, right - pad, row - 6, 24, 0x222225);
            }
            // The line to the next stage, in this stage's colour, quieter than the dots.
            if i + 1 < path.stages.len() {
                c.rect(dot_x - 1, cy + 14, 3, row - 28, mix(colour, 0x000000, 150));
            }
            c.disc(dot_x, cy, 10, colour);
            if i == sel {
                c.ring(dot_x, cy, 17, 2, colour);
            }
            let base = cy + 11;
            self.text.draw(c, Face::Body, 28, name_x, base, 0xaaa79f, s.kind.name());
            // The grade at 26 and 24 px from the value: the longest German values still fit beside "Beeinträchtigt".
            let grade_w = self.text.width(Face::Body, 26, s.grade.name());
            if s.grade != Grade::Unknown {
                self.text.draw_right(c, Face::Body, 26, right - NU_BTN_PAD, base - 2, colour, s.grade.name());
            }
            let value = self.text.fit(Face::Body, 32, &s.value, right - NU_BTN_PAD - grade_w - 24 - value_x);
            self.text.draw(c, Face::Body, 32, value_x, base, 0xffffff, &value);
            if !s.line.is_empty() && (!tight || i == sel) {
                let line = self.text.fit(Face::Body, 28, &s.line, right - NU_BTN_PAD - value_x);
                self.text.draw(c, Face::Body, 28, value_x, base + line_dy, 0x9a978f, &line);
            }
        }

        // The last seconds of play (the game is paused behind the menu).
        let live_base = top + 7 * row + 26 + pocket_ui::type_scale::grow(28);
        let live = path.live.clone().unwrap_or_else(|| match path.overall {
            Grade::Unknown => "Pixel Path follows games that pocket-core runs.".to_string(),
            _ => "Nothing measured yet: play a few seconds first.".to_string(),
        });
        let live = self.text.fit(Face::Body, 28, &live, right - NU_BTN_PAD - name_x);
        self.text.draw(c, Face::Body, 28, name_x, live_base, 0xcccccc, &live);

        let over = self.menu.as_ref().and_then(|m| m.settings.as_ref()).filter(|_| !managed).map(over_game);
        let hints: &[(&str, &str)] = match (about, over) {
            (true, _) => &[("B", "Close")],
            (false, Some(true)) => &[("B", "Back"), ("Y", "Hide from the game"), ("A", "Explain")],
            (false, Some(false)) => &[("B", "Back"), ("Y", "Show over the game"), ("A", "Explain")],
            (false, None) => &[("B", "Back"), ("A", "Explain")],
        };
        let hints_at = self.nu_hints(c, hints);
        // The legend, left of the hints on their line.
        let base = c.h - pad - NU_PILL / 2 + pocket_ui::type_scale::mid(24);
        let mut lx = pad + NU_BTN_PAD;
        for g in Grade::SCALE {
            let w = self.text.width(Face::Body, 24, g.name());
            if lx + 22 + w > hints_at - 24 {
                break;
            }
            c.disc(lx + 7, base - 8, 7, g.colour());
            self.text.draw(c, Face::Body, 24, lx + 22, base, 0xaaa79f, g.name());
            lx += 22 + w + 30;
        }

        if about {
            self.paint_pixel_about(c, &path.stages[sel.min(path.stages.len() - 1)]);
        }
    }

    /// A stage's paragraph, on a sheet over the darkened page.
    fn paint_pixel_about(&mut self, c: &mut Canvas, s: &pixel_path::Stage) {
        c.darken_rect(0, NU_PAD + NU_PILL, c.w, c.h - 2 * (NU_PAD + NU_PILL), 170);
        // For the snapshot checks of Settings → Text size: the page under the sheet is darkened, not read.
        pocket_ui::text::boxes::covered();
        let (w, inner) = (c.w - 2 * (NU_PAD + 40), 48);
        let lines = self.text.wrap(Face::Body, 30, &s.about, w - 2 * inner);
        let lines: Vec<String> = lines.into_iter().take(12).collect();
        // Every line a little lower with larger text: the heading, the value, the paragraph's lines.
        let (g1, g2, g3, step) = (pocket_ui::type_scale::grow(38), pocket_ui::type_scale::grow(28), pocket_ui::type_scale::grow(30), pocket_ui::type_scale::line(30, 12));
        let h = inner + 44 + 20 + 34 + 30 + lines.len() as i32 * step + inner - 10 + g1 + g2 + g3;
        let (x, y) = (NU_PAD + 40, ((c.h - h) / 2).max(NU_PAD + NU_PILL + 10));
        c.round_rect(x, y, w, h, 28, 0x1a1a1c);
        let colour = s.grade.colour();
        let base = y + inner + 30 + g1;
        c.disc(x + inner + 9, base - 11, 9, colour);
        self.text.draw(c, Face::Body, 38, x + inner + 32, base, 0xffffff, s.kind.name());
        if s.grade != Grade::Unknown {
            self.text.draw_right(c, Face::Body, 28, x + w - inner, base, colour, s.grade.name());
        }
        let value = self.text.fit(Face::Body, 28, &s.value, w - 2 * inner);
        self.text.draw(c, Face::Body, 28, x + inner, base + 50 + g2, 0xaaa79f, &value);
        let mut ly = base + 50 + g2 + 64 + g3;
        for l in &lines {
            self.text.draw(c, Face::Body, 30, x + inner, ly, 0xd9d6ce, l);
            ly += step;
        }
    }
}
