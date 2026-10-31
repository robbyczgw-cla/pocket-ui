//! Part of the pocket-ui binary: which emulator plays a game (`pocket_ui::emu_choice`) and the "Runs on" page
//! (`pocket_ui::emu_choice_page`).
//!
//! * `resolve_runner` is the launch path's choice (`App::route`, main.rs): pocket-core's core for the system, whether
//!   it takes the file, `fallback.json`, then system.cfg's choices from the cache `emu_choice::preload` filled at start.
//!   A map lookup and a few string compares: nothing is read from the card when a game starts.
//! * The page opens with START on a game's card or a console page, and from the pause menu (Options). Opening it reads
//!   system.cfg and ROCKNIX's shipped copy (two small files) and checks which emulators are installed (a stat each);
//!   saving writes on a thread (`emu_choice::write_in_background`) and updates the cache at once.

use std::path::{Path, PathBuf};
use std::time::Instant;

use pocket_ui::emu_choice::{self, Cfg, Resolved, Runner};
use pocket_ui::emu_choice_page::{Action, EmuChoicePage, Mode, Row, Subject};
use pocket_ui::gfx::Canvas;
use pocket_ui::hostsys;
use pocket_ui::library::System;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, MainItem, Page, CANCEL, CONFIRM, host_bin, HOST_CORES, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// Where the page was opened from, so B goes back there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opener {
    /// A game's card or a console page: they stay open under the page.
    Shelf,
    /// The pause menu: back to Options (pocket-core) or its main page.
    Menu,
}

#[derive(Default)]
pub(crate) struct EmuChoiceUi {
    page: Option<(EmuChoicePage, Opener)>,
}

/// What `resolve_runner` and the page need besides system.cfg.
struct Inputs {
    pocket_core: Option<&'static str>,
    file_ok: bool,
    fell_back: bool,
    default_override: Option<(&'static str, &'static str)>,
}

impl App {
    fn runner_inputs(&self, sys: &System, path: Option<&Path>, host_bin: bool) -> Inputs {
        let system = sys.name.as_str();
        let bios = Path::new(hostsys::BIOS_DIR);
        // pocket-core opens zip files itself but not 7z: those games stay with RetroArch.
        let file_ok = path.is_none_or(|path| {
            let is_7z = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("7z"));
            !is_7z && hostsys::hosts_file(system, path)
        });
        let pocket_core = HOST_CORES
            .iter()
            .find(|(s, _)| *s == system)
            .map(|(_, core)| *core)
            .or_else(|| hostsys::route(system, bios))
            .filter(|_| host_bin);
        Inputs {
            pocket_core,
            file_ok,
            fell_back: path.is_some_and(|p| self.fallback.contains(p)),
            // PICO-8 without the official binary: RetroArch's fake08 instead of start_pico8.sh (`hostsys`).
            default_override: hostsys::default_emulator(system, bios),
        }
    }

    /// The emulator and core `sys`'s game at `path` (None: the system as a whole) starts with, and why: the game's
    /// choice, the console's, or the default (`emu_choice::resolve`). `host_bin`: pocket-core is installed.
    pub(crate) fn resolve_runner(&self, sys: &System, path: Option<&Path>, host_bin: bool) -> Resolved {
        let i = self.runner_inputs(sys, path, host_bin);
        let ctx = emu_choice::Ctx {
            system: &sys.name,
            emulators: &sys.emulators,
            pocket_core: i.pocket_core,
            file_ok: i.file_ok,
            fell_back: i.fell_back,
            default_override: i.default_override,
        };
        let file = path.and_then(|p| p.file_name()).and_then(|n| n.to_str());
        emu_choice::resolve(&ctx, &emu_choice::cached(), file)
    }

    /// Whether pocket-core can play `path` of `sys` (its host and core), whatever plays it now: the picture, extras and
    /// controls of "This game" are for it.
    pub(crate) fn emu_hostable(&self, sys: &System, path: &Path) -> bool {
        self.runner_inputs(sys, Some(path), true).pocket_core.is_some()
    }

    fn emu_subject(&self, system: &str, game: Option<(&str, &Path)>, host_bin: bool) -> Option<Subject> {
        let sys = self.systems.iter().find(|s| s.name == system)?;
        let i = self.runner_inputs(sys, game.map(|(_, p)| p), host_bin);
        Some(Subject {
            system: sys.name.clone(),
            system_name: sys.fullname.clone(),
            game: game.and_then(|(name, p)| Some((name.to_string(), p.file_name()?.to_str()?.to_string()))),
            emulators: sys.emulators.clone(),
            pocket_core: i.pocket_core.map(str::to_string),
            default_override: i.default_override.map(|(e, c)| (e.to_string(), c.to_string())),
            file_ok: i.file_ok,
            fell_back: i.fell_back,
        })
    }

    /// Opens the page for a system and maybe one of its games, reading system.cfg now (two small files: well under the
    /// 50 ms a page may take to open; logged).
    pub(crate) fn open_emu_choice(&mut self, system: &str, game: Option<(&str, &Path)>, from: Opener) -> bool {
        let started = Instant::now();
        let host_bin = Path::new(host_bin()).exists();
        let Some(subject) = self.emu_subject(system, game, host_bin) else {
            return false;
        };
        let cfg = Cfg::read(Path::new(emu_choice::SYSTEM_CFG));
        let shipped = Cfg::read(Path::new(emu_choice::SHIPPED_CFG));
        let page = EmuChoicePage::new(subject, cfg, shipped, |r| emu_choice::installed(r, system, |p| p.exists()));
        emu_choice::find_shaders_in_background();
        println!("emulator choice: page for {system} opened in {:?}", started.elapsed());
        self.emu.page = Some((page, from));
        true
    }

    /// The page is open over the shelf (a card or a console page).
    pub(crate) fn emu_choice_on_shelf(&self) -> bool {
        matches!(self.emu.page, Some((_, Opener::Shelf))) && self.running.is_none()
    }

    /// START on a console page.
    pub(crate) fn emu_choice_for_console(&mut self, s: usize) {
        let Some(system) = self.systems.get(s).map(|sys| sys.name.clone()) else {
            return;
        };
        if !self.open_emu_choice(&system, None, Opener::Shelf) {
            pocket_ui::ui_sound::refuse();
        }
    }

    /// Options in the pause menu of the running game: the page, over the paused game.
    pub(crate) fn emu_choice_in_menu(&mut self) {
        let Some(run) = self.running.as_ref() else { return };
        let (system, name, path) = (run.system.clone(), run.name.clone(), run.path.clone());
        if self.open_emu_choice(&system, Some((&name, &path)), Opener::Menu) {
            if let Some(m) = self.menu.as_mut() {
                m.page = Page::EmuChoice;
            }
        } else {
            // A system the library doesn't list (it can't be, for a running game): stay where the menu was.
            pocket_ui::ui_sound::refuse();
            self.emu_choice_back_to_menu();
        }
    }

    pub(crate) fn emu_choice_input(&mut self, b: Button) {
        let Some((page, from)) = self.emu.page.as_mut() else {
            // The pause menu's page without its state: back to the menu rather than stuck on an empty page.
            if self.menu.as_ref().is_some_and(|m| m.page == Page::EmuChoice) {
                self.emu_choice_back_to_menu();
            }
            return;
        };
        let from = *from;
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.emu.page = None;
                if from == Opener::Menu {
                    self.emu_choice_back_to_menu();
                }
            }
            Action::Save { edits, clear_fallback } => {
                println!(
                    "emulator choice: {}",
                    edits
                        .iter()
                        .map(|(k, v)| format!("{k}={}", v.as_deref().unwrap_or("(removed)")))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                // The next start sees it at once; the file is written on a thread, under ROCKNIX's lock.
                emu_choice::apply_to_cache(&edits);
                emu_choice::write_in_background(edits);
                if clear_fallback {
                    if let Some(path) = self.emu_page_path() {
                        self.fallback.remove(&path);
                    }
                }
                self.emu_choice_refresh_console();
            }
        }
    }

    /// The game the page is about, as a path in the library.
    fn emu_page_path(&self) -> Option<PathBuf> {
        let (page, from) = self.emu.page.as_ref()?;
        let file = page.subject.game.as_ref().map(|(_, f)| f.as_str())?;
        let in_card = self.card.as_ref().map(|o| o.path.clone()).filter(|_| *from == Opener::Shelf);
        let running = self.running.as_ref().map(|r| r.path.clone()).filter(|_| *from == Opener::Menu);
        in_card.or(running).filter(|p| p.file_name().and_then(|n| n.to_str()) == Some(file))
    }

    /// The console page under the page says what runs the system now.
    fn emu_choice_refresh_console(&mut self) {
        let Some(s) = self.console.as_ref().map(|o| o.system) else { return };
        let runs_on = self.systems.get(s).map(|sys| {
            let r = self.resolve_runner(sys, None, Path::new(host_bin()).exists()).runner;
            pocket_ui::consoles::runner_text(&r.emulator, &r.core)
        });
        if let (Some(open), Some(runs_on)) = (self.console.as_mut(), runs_on) {
            open.page.data.runs_on = runs_on;
        }
    }

    /// B on the page in the pause menu: pocket-core's Options list (on its Runs on entry), else the main page (on
    /// Options).
    fn emu_choice_back_to_menu(&mut self) {
        let Some(menu) = self.menu.as_mut() else { return };
        menu.page = if menu.hosted {
            Page::Options {
                sel: super::opt_pos(menu.emu.as_ref(), menu.cheats.is_some(), super::OptEntry::RunsOn),
            }
        } else {
            menu.sel = menu.items().iter().position(|i| *i == MainItem::Options).unwrap_or(0);
            Page::Main
        };
    }

    /// The pause menu went away (Continue, Quit, the game ended) while the page was open.
    pub(crate) fn emu_choice_menu_closed(&mut self) {
        if matches!(self.emu.page, Some((_, Opener::Menu))) {
            self.emu.page = None;
        }
    }

    /// Where the page stands, for the menu sounds and haptics.
    pub(crate) fn emu_choice_mark(&self) -> Option<pocket_ui::emu_choice_page::Mark> {
        self.emu.page.as_ref().map(|(p, _)| p.mark())
    }

    /// `--snapshot emu:<system>[:<game file>][:<runner emulator>/<core>][:save][:<row>]`: the page with a made-up
    /// system.cfg (a RetroArch choice made in EmulationStation for `gba`), the presets of a Mini V2.
    pub(crate) fn emu_choice_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("emu:") else { return false };
        let mut parts = rest.split(':');
        let system = parts.next().unwrap_or("gba").to_string();
        let mut game: Option<String> = None;
        let mut runner: Option<Runner> = None;
        let (mut save, mut row) = (false, 0usize);
        for p in parts {
            if p == "save" {
                save = true;
            } else if let Ok(n) = p.parse() {
                row = n;
            } else if let Some((e, c)) = p.split_once('/') {
                runner = Some(Runner::new(e, c));
            } else {
                game = Some(p.to_string());
            }
        }
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else {
            eprintln!("emu:{system}: no such system in the library");
            return false;
        };
        let path = game.as_ref().map(|g| {
            sys.games
                .iter()
                .find(|x| x.path.file_name().and_then(|n| n.to_str()) == Some(g.as_str()))
                .map_or_else(|| PathBuf::from(format!("/storage/roms/{system}/{g}")), |x| x.path.clone())
        });
        let name = path.as_ref().map(|p| p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string());
        let Some(subject) = self.emu_subject(&system, name.as_deref().zip(path.as_deref()), true) else {
            return false;
        };
        let cfg = Cfg::parse(SNAPSHOT_CFG);
        emu_choice::set_cached(Cfg::choices_only(SNAPSHOT_CFG));
        emu_choice::set_shader_presets(
            [
                "crt/zfast-crt.slangp",
                "crt/crt-geom.slangp",
                "crt/crt-easymode.slangp",
                "handheld/lcd-grid-v2.slangp",
                "handheld/gameboy.slangp",
                "interpolation/sharp-bilinear-simple.slangp",
            ]
            .map(String::from)
            .to_vec(),
        );
        let mut page = EmuChoicePage::new(subject, cfg, Cfg::parse(SNAPSHOT_SHIPPED), |r| r.core != "gpsp" && r.core != "drastic-sa");
        if let Some(r) = runner {
            page.runner = r;
        }
        page.sel = row.min(page.rows().len() - 1);
        if save {
            page.sel = page.rows().len() - 1;
            page.mode = Mode::Save {
                sel: 1.min(page.save_choices().len() - 1),
            };
        }
        self.emu.page = Some((page, Opener::Shelf));
        true
    }

    /// `--snapshot runs-on[:<row>]`: the page as Options opens it in a RetroArch game's pause menu.
    pub(crate) fn emu_choice_snapshot_in_menu(&mut self, row: usize) -> bool {
        let game = self
            .systems
            .iter()
            .find(|s| s.name == "gba")
            .and_then(|s| s.games.first())
            .and_then(|g| g.path.file_name()?.to_str().map(str::to_string));
        let state = format!("emu:gba:{}:retroarch/mgba:{row}", game.unwrap_or_else(|| "Game.gba".into()));
        if !self.emu_choice_snapshot(&state) {
            return false;
        }
        if let Some((_, from)) = self.emu.page.as_mut() {
            *from = Opener::Menu;
        }
        true
    }

    pub(crate) fn paint_emu_choice(&mut self, c: &mut Canvas) {
        let Some((page, from)) = self.emu.page.take() else { return };
        // In the pause menu the page is drawn over the dimmed game, in the menu's dark colours (`paint_menu`).
        if from == Opener::Shelf {
            c.clear(pal().void);
        } else {
            // Long explanations over a busy game: darker than the menu's own dimming.
            c.fade(150);
        }
        self.paint_emu_choice_page(c, &page);
        self.emu.page = Some((page, from));
    }

    fn paint_emu_choice_page(&mut self, c: &mut Canvas, page: &EmuChoicePage) {
        let pad = NU_PAD;
        self.status_pill(c);
        let right = c.w - pad;
        let title = page.subject.game.as_ref().map_or(page.subject.system_name.as_str(), |(n, _)| n.as_str());
        let title = self.text.fit(Face::Title, 50, title, c.w - 2 * pad - 2 * NU_BTN_PAD - 330);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, &title);
        let sub = match (&page.subject.game, page.mode) {
            (_, Mode::Save { .. }) => format!("Save changes · {}", page.subject.system_name),
            (Some(_), _) => page.subject.system_name.clone(),
            (None, _) => format!("{}: every game", page.subject.system_name),
        };
        self.text.draw(c, Face::Body, 28, pad + NU_BTN_PAD, pad + 104, pal().dust, &sub);
        let top = pad + NU_PILL + 46;

        // The rows: the page's own, or the three ways to save.
        let lines: Vec<(String, Option<String>)> = match page.mode {
            Mode::List => page
                .rows()
                .iter()
                .map(|r| (page.label(*r).to_string(), (*r != Row::Save).then(|| page.value(*r))))
                .collect(),
            Mode::Save { .. } => page.save_choices().iter().map(|s| (s.label().to_string(), None)).collect(),
        };
        let sel = match page.mode {
            Mode::List => page.sel,
            Mode::Save { sel } => sel,
        };
        let font = if lines.len() > 6 { 34u32 } else { 36u32 };
        let row = pocket_ui::type_scale::row(if lines.len() > 6 { 54 } else { 66 }, font);
        for (i, (name, value)) in lines.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = i == sel;
            if selected {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let vx = right - NU_BTN_PAD;
            match value {
                Some(value) => {
                    let room = right - pad - 2 * NU_BTN_PAD - self.text.width(Face::Body, font, name) - 60;
                    let shown = if selected { format!("‹ {value} ›") } else { value.clone() };
                    let shown = self.text.fit(Face::Body, font, &shown, room);
                    self.text.draw_right(c, Face::Body, font, vx, base, pal().text, &shown);
                    if selected {
                        let vw = self.text.width(Face::Body, font, &shown);
                        let aw = self.text.width(Face::Body, font, "‹ ") + 24;
                        self.hits.button(Area::new(vx - vw - 24, y, aw, row), Button::Left);
                        self.hits.button(Area::new(vx - aw + 24, y, aw, row), Button::Right);
                    }
                }
                None if matches!(page.mode, Mode::List) => self.text.draw_right(c, Face::Body, font, vx, base, pal().text, ">"),
                None => {}
            }
        }
        self.hits.list(sel, lines.len(), 0.0, row as f32);

        // What the selected row is for and what its value means, then where the settings come from.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + lines.len() as i32 * row + 36;
        let [what, choice] = match page.mode {
            Mode::List => page.describe(page.row()),
            Mode::Save { sel } => [
                save_desc(page.save_choices()[sel.min(page.save_choices().len() - 1)]).to_string(),
                page.level_line(),
            ],
        };
        // Everything under the list stays above the hints; with larger text a paragraph that reaches them ends with "…".
        let hints_top = c.h - pad - NU_PILL - 16;
        for (text, color) in [(what.as_str(), pal().dust), (choice.as_str(), pal().text)] {
            y = self.text.paragraph(c, Face::Body, NU_SMALL, (left, y), width, pocket_ui::type_scale::line(NU_SMALL, 12), hints_top, color, text);
            y += 6;
        }
        y += 10;
        let mut notes: Vec<(String, u32)> = Vec::new();
        if let Some(n) = &page.note {
            notes.push((n.clone(), pal().chalk));
        } else if page.unsaved() {
            notes.push(("Not saved yet: Save changes keeps it.".to_string(), pal().amber));
        }
        // The Save changes row and its choices say it in their own text.
        if matches!(page.mode, Mode::List) && page.row() != Row::Save {
            notes.push((page.level_line(), pal().quiet));
        }
        if let Some(f) = page.footer() {
            notes.push((f.to_string(), pal().quiet));
        }
        for (text, color) in notes {
            y = self.text.paragraph(c, Face::Body, 24, (left, y), width, pocket_ui::type_scale::line(24, 8), hints_top, color, &text);
            y += 4;
        }

        let hints: &[(&str, &str)] = match page.mode {
            Mode::List if page.row() == Row::Save => &[("B", "Back"), ("A", "Open")],
            Mode::List => &[("B", "Back"), ("A", "Next"), ("< >", "Change")],
            Mode::Save { .. } => &[("B", "Cancel"), ("A", "Save")],
        };
        self.nu_hints(c, hints);
    }
}

fn save_desc(s: pocket_ui::emu_choice_page::SaveChoice) -> &'static str {
    use pocket_ui::emu_choice_page::SaveChoice::*;
    match s {
        Console => "Every game on this console starts with these. A game's own choices are removed.",
        Game => "Only this game starts with these: what differs from the console's is kept for it.",
        Defaults => "Back to ROCKNIX's settings and pocket-ui's default emulator for this console and this game.",
    }
}

/// The system.cfg of `--snapshot emu:`: a RetroArch choice made in EmulationStation for the GBA, ROCKNIX's defaults.
const SNAPSHOT_CFG: &str =
    "global.ratio=core\nglobal.autosave=1\nglobal.rewind=0\nglobal.runahead=0\ngba.ratio=3/2\ngba.emulator=retroarch\ngba.core=gpsp\nsnes.ratio=4/3\n";
const SNAPSHOT_SHIPPED: &str = "global.ratio=core\nglobal.autosave=1\nglobal.rewind=0\nglobal.runahead=0\ngba.ratio=3/2\nsnes.ratio=4/3\n";
