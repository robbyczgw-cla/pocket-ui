//! Part of the pocket-ui binary: the controls card (`pocket_ui::controls_card`) on the shelf.
//!
//! Three ways to it (docs/user/controls-card.md): the first start of a console, once (any button goes on to the game),
//! the game card's **Controls** row (`RowKind::Controls`), and Options > Controls in the pause menu (`Page::ControlsCard`,
//! painted by `paint_controls_card_pause`). The card is built from the settings files and the core's snapshot when it
//! opens: a few small files, nothing on the UI thread's hot path.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pocket_ui::controls::Mapping;
use pocket_ui::controls_card::{self, Card, Colors, Input, Shortcuts};
use pocket_ui::core_options::{self, CoreOptionsFile};
use pocket_ui::game_card_model::{CuratorRow, RowKind};
use pocket_ui::gfx::Canvas;
use pocket_ui::options::GameSettings;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL};

/// How long the first-start card waits before the game starts by itself.
const FIRST_START_SECS: u64 = 12;

pub(crate) struct ControlsCardUi {
    card: Card,
    /// The game's name, and the console's, for the heading.
    title: String,
    system_name: String,
    system: String,
    /// The first start of a console: the game that starts when the card goes (system, name, path).
    launch: Option<(String, String, PathBuf)>,
    since: Instant,
}

impl App {
    /// On the Brick the card also says what F1 and F2 do.
    #[cfg(feature = "brick")]
    fn brick_card_keys(&self, card: Card) -> Card {
        if !pocket_ui::brick_profile::fkeys() {
            return card;
        }
        controls_card::with_brick_keys(card, pocket_ui::brick_hw::FKeys::load(&self.data_dir))
    }

    /// The card for a game of `system` as the shelf knows it: the game's settings (remaps, shortcuts), the core's names
    /// from its snapshot when it ran before, whether pocket-core plays it.
    fn controls_card_of(&self, system: &str, path: &Path) -> Option<Card> {
        let sys = self.systems.iter().find(|s| s.name == system)?;
        let settings = GameSettings::load(&self.data_dir, system, path);
        let mapping = Mapping::from_values(system, &settings.values);
        let core_names = CoreOptionsFile::read(&core_options::file_path(&self.data_dir, system), path).map(|f| f.controls.targets).unwrap_or_default();
        let hosted = self.resolve_runner(sys, Some(path), Path::new(super::host_bin()).exists()).runner.emulator == pocket_ui::emu_choice::POCKET_CORE;
        let shortcuts = hosted.then(|| Shortcuts::from_values(&settings.effective()));
        let card = controls_card::build(&Input { system, system_name: &sys.fullname, mapping: &mapping, core_names: &core_names, shortcuts, layout: pocket_ui::device::current().menu_layout(), deck: pocket_ui::device::current().model == Some(pocket_ui::device::Model::SteamDeck) });
        #[cfg(feature = "brick")]
        let card = self.brick_card_keys(card);
        Some(card)
    }

    /// The game card's Controls row: for consoles pocket-core hosts or whose buttons are known.
    pub(crate) fn card_controls_row(&self, system: &str) -> Option<CuratorRow> {
        (pocket_ui::hostsys::find(system).is_some() || controls_card::has_names(system)).then(|| CuratorRow {
            kind: RowKind::Controls,
            title: "Controls".into(),
            line: if pocket_ui::device::current().model == Some(pocket_ui::device::Model::SteamDeck) {
                "Which button of the Deck does what".into()
            } else {
                "Which button of the Pocket does what".into()
            },
        })
    }

    /// A on the game card's Controls row.
    pub(crate) fn open_controls_card_for_card(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (system, name, path) = (open.system.clone(), open.name.clone(), open.path.clone());
        self.open_controls_card(&system, &name, &path, None);
    }

    fn open_controls_card(&mut self, system: &str, name: &str, path: &Path, launch: Option<(String, String, PathBuf)>) {
        let Some(card) = self.controls_card_of(system, path) else {
            pocket_ui::ui_sound::refuse();
            return;
        };
        self.held = None;
        self.home_tap = false;
        self.start_tap.disarm();
        self.controls_card = Some(ControlsCardUi { system_name: card.system_name.clone(), card, title: name.to_string(), system: system.to_string(), launch, since: Instant::now() });
    }

    /// A start the player asked for (A on a game, on the card's Play): the first time a console starts on pocket-core,
    /// its controls card comes first and the game starts when it goes. Everything else starts at once.
    pub(crate) fn launch_asking(&mut self, system: String, name: String, path: PathBuf) {
        if self.first_card_wanted(&system, &path) {
            println!("controls card: first start of {system}");
            self.open_controls_card(&system, &name, &path, Some((system.clone(), name.clone(), path.clone())));
            if self.controls_card.is_some() {
                return;
            }
        }
        self.launch(system, name, path);
    }

    fn first_card_wanted(&self, system: &str, path: &Path) -> bool {
        if self.kids_live() {
            return false;
        }
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else { return false };
        // Only what pocket-core plays, and only when due (unknown consoles still explain HOME): the
        // mark is written here, so a start that fails still counts as told.
        let hosted = self.resolve_runner(sys, Some(path), Path::new(super::host_bin()).exists()).runner.emulator == pocket_ui::emu_choice::POCKET_CORE;
        if !hosted {
            return false;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
        let states = pocket_ui::states::StateSet::pocket_core(&pocket_ui::slots::base(system, stem));
        let resuming = states.path(pocket_ui::states::Target::Auto).is_some();
        controls_card::first_start_due(&self.data_dir, system, false, resuming)
    }

    pub(crate) fn controls_card_open(&self) -> bool {
        self.controls_card.is_some()
    }

    pub(crate) fn controls_card_mark(&self) -> Option<(String, bool)> {
        self.controls_card.as_ref().map(|c| (c.system.clone(), c.launch.is_some()))
    }

    /// Any button: the card goes. At a first start, the game starts.
    pub(crate) fn controls_card_input(&mut self, b: Button) {
        let _ = b;
        self.controls_card_close();
    }

    fn controls_card_close(&mut self) {
        let Some(ui) = self.controls_card.take() else { return };
        if let Some((system, name, path)) = ui.launch {
            self.launch(system, name, path);
        }
    }

    /// Wake even when no other events arrive: the card must never extend its twelve-second wait.
    pub(crate) fn controls_card_wake(&self) -> Option<Instant> {
        self.controls_card.as_ref().filter(|c| c.launch.is_some()).map(|c| c.since + Duration::from_secs(FIRST_START_SECS))
    }

    /// The first-start card does not wait for ever.
    pub(crate) fn controls_card_tick(&mut self) {
        if self.controls_card_wake().is_some_and(|t| Instant::now() >= t) {
            self.controls_card_close();
        }
    }

    fn card_colors(dark: bool) -> Colors {
        // What the player changed is drawn in the palette's amber, the colour of "look here" on every page (a console's own
        // stripe can be nearly the page's colour).
        if dark {
            Colors { bg: VOID, line: 0x6b6963, text: 0xffffff, dim: 0x9a978f, accent: DARK.amber }
        } else {
            Colors { bg: pal().void, line: pal().dust, text: pal().chalk, dim: pal().dust, accent: pal().amber }
        }
    }

    pub(crate) fn paint_controls_card(&mut self, c: &mut Canvas) {
        let Some(ui) = self.controls_card.take() else { return };
        c.clear(pal().void);
        let first = ui.launch.is_some();
        let colors = Self::card_colors(false);
        let hints: &[(&str, &str)] = if first { &[("A", "Play")] } else { &[("B", "Back")] };
        self.paint_card_screen(c, &ui.title, &ui.system_name, &ui.card, colors, hints);
        // A tap anywhere is a press of A, as the hint says; the hint pill's own area lies on top.
        if first {
            self.hits.button(Area::new(0, 0, c.w, c.h), CONFIRM);
        }
        self.controls_card = Some(ui);
    }

    /// The card's page: heading, the card, a line under it and the hints. The shelf and the pause menu share it.
    fn paint_card_screen(&mut self, c: &mut Canvas, title: &str, system_name: &str, card: &Card, colors: Colors, hints: &[(&str, &str)]) {
        let pad = NU_PAD;
        self.status_pill(c);
        let heading = self.text.fit(Face::Title, 50, title, c.w - 2 * pad - 2 * NU_BTN_PAD - 330);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, colors.text, &heading);
        let sub = self.text.fit(Face::Body, 28, &format!("Controls · {system_name}"), c.w - 2 * pad - 2 * NU_BTN_PAD);
        self.text.draw(c, Face::Body, 28, pad + NU_BTN_PAD, pad + 104, colors.dim, &sub);
        let top = pad + NU_PILL + 46;
        let bottom = c.h - pad - NU_PILL - 24;
        let area = (pad + NU_BTN_PAD, top, c.w - 2 * (pad + NU_BTN_PAD), (bottom - top).max(120));
        controls_card::paint(c, &mut self.text, card, area, colors);
        self.nu_hints(c, hints);
    }

    /// Options > Controls > Controls card, in a game: the pause menu's own colours, the live mapping and names.
    pub(crate) fn paint_controls_card_pause(&mut self, c: &mut Canvas) {
        let Some(menu) = self.menu.as_ref() else { return };
        let Some(run) = self.running.as_ref() else { return };
        let (system, name) = (run.system.clone(), run.name.clone());
        let Some(emu) = menu.emu.as_ref() else { return };
        let values = menu.settings.as_ref().map(|s| s.effective());
        let shortcuts = menu.hosted.then(|| values.as_ref().map_or(Shortcuts { save: true, load: true, zoom: false }, Shortcuts::from_values));
        let system_name = self.systems.iter().find(|s| s.name == system).map_or_else(|| system.clone(), |s| s.fullname.clone());
        let card = emu.controls_card(&system_name, shortcuts);
        #[cfg(feature = "brick")]
        let card = self.brick_card_keys(card);
        c.clear(VOID);
        let colors = Self::card_colors(true);
        self.paint_card_screen(c, &name, &system_name, &card, colors, &[("B", "Back")]);
    }

    /// `--snapshot controls-card-row:<system>:<game name part>`: the game's card, scrolled onto its Controls row.
    pub(crate) fn controls_card_row_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("controls-card-row:") else { return false };
        let (sys, name) = rest.split_once(':').unwrap_or((rest, ""));
        let found = self.systems.iter().position(|s| s.name == sys).and_then(|s| {
            let g = self.systems[s].games.iter().position(|g| g.name.to_lowercase().contains(&name.to_lowercase()))?;
            Some((s, g))
        });
        let Some((s, g)) = found else {
            eprintln!("controls-card-row:{rest}: no such game");
            return true;
        };
        let (system, gname, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
        self.show_card(super::Item::Game(s, g), system, gname, path);
        true
    }

    /// After the first paint (which places the card's rows): onto the Controls row.
    pub(crate) fn controls_card_row_snapshot_after(&mut self) {
        if let Some(open) = self.card.as_mut() {
            if let Some(i) = open.card.data.rows.iter().position(|r| r.kind == RowKind::Controls) {
                open.card.focus_row(i);
            }
        }
    }

    /// `--snapshot controls-card:<system>[:remap][:first][:off][:zoom]`: the shelf's card for a game of the snapshot library;
    /// `remap` has the Pocket's X press A and Y nothing (as saved), `first` the first-start hints, `off` the shortcuts
    /// of Options > Shortcuts switched off, `zoom` HOME + X switched on.
    pub(crate) fn controls_card_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("controls-card").filter(|r| r.is_empty() || r.starts_with(':')) else { return false };
        let mut parts = rest.split(':').filter(|p| !p.is_empty());
        let system = parts.next().unwrap_or("snes").to_string();
        let flags: Vec<&str> = parts.collect();
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else {
            eprintln!("controls-card:{system}: no such system in the library");
            return false;
        };
        let (name, path) = match sys.games.first() {
            Some(g) => (g.name.clone(), g.path.clone()),
            None => ("Game".to_string(), PathBuf::from(format!("/storage/roms/{system}/Game.rom"))),
        };
        let mut settings = GameSettings::load(&self.data_dir, &system, &path);
        if flags.contains(&"remap") {
            settings.values.insert("ctl.x".into(), "a".into());
            settings.values.insert("ctl.y".into(), "none".into());
        }
        if flags.contains(&"off") {
            settings.values.insert("hk_save".into(), "off".into());
        }
        if flags.contains(&"zoom") {
            settings.values.insert("hk_zoom".into(), "on".into());
        }
        let mapping = Mapping::from_values(&system, &settings.values);
        let core_names = CoreOptionsFile::read(&core_options::file_path(&self.data_dir, &system), &path).map(|f| f.controls.targets).unwrap_or_default();
        let shortcuts = Some(Shortcuts::from_values(&settings.effective()));
        let card = controls_card::build(&Input { system: &system, system_name: &sys.fullname, mapping: &mapping, core_names: &core_names, shortcuts, layout: pocket_ui::device::current().menu_layout(), deck: pocket_ui::device::current().model == Some(pocket_ui::device::Model::SteamDeck) });
        #[cfg(feature = "brick")]
        let card = self.brick_card_keys(card);
        let launch = flags.contains(&"first").then(|| (system.clone(), name.clone(), path.clone()));
        self.controls_card = Some(ControlsCardUi { system_name: card.system_name.clone(), card, title: name, system, launch, since: Instant::now() });
        true
    }
}
