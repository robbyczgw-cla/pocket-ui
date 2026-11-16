//! The "Runs on" page: which emulator plays a game or a console, and the RetroArch settings of the games RetroArch
//! plays (`emu_choice`). Opened with START on a game's card or a console page, and from the pause menu (Options).
//!
//! It works like the pause menu's Options: the rows show what is in use, Left/Right change them, and nothing is kept
//! until "Save changes" (for this console, for this game, or back to the defaults). Saving gives the App the
//! system.cfg edits to write (`Action::Save`); the page updates itself from them. Painting is in the binary
//! (`app_emu_choice.rs`); nothing here touches a file, so every path is a plain test.

use std::collections::HashMap;

use crate::emu_choice::{self, Cfg, Ctx, EsEmulator, From, Kind, Level, Runner, RA_SETTINGS};
use crate::pad::Button;

/// What the page is about, gathered by the App when it opens.
#[derive(Clone, Debug, Default)]
pub struct Subject {
    /// ROCKNIX's system id ("gba") and its full name.
    pub system: String,
    pub system_name: String,
    /// The game: its name and its file name. None: the console as a whole.
    pub game: Option<(String, String)>,
    pub emulators: Vec<EsEmulator>,
    /// `emu_choice::Ctx` for the console (pocket-core's core, what stands in for the default).
    pub pocket_core: Option<String>,
    pub pocket_cores: &'static [&'static str],
    pub default_override: Option<(String, String)>,
    /// pocket-core takes the game's file; pocket-core couldn't start it before (`fallback.json`).
    pub file_ok: bool,
    pub fell_back: bool,
    /// Android's handheld view: the page speaks of the phone's ways (our player, apps) instead of ROCKNIX's emulators.
    pub phone: Option<PhoneSubject>,
}

/// What the page needs to know about a phone (`play_route`, `app_runs_on_android`).
#[derive(Clone, Debug, Default)]
pub struct PhoneSubject {
    /// The emulator apps that take the system: package and the name the phone shows.
    pub labels: Vec<(String, String)>,
    /// pocket-ui's own player for the system: its core, whether it is on the phone, what the download is. None where the
    /// own player has no core for the system (PS2, GameCube and Wii, 3DS, Xbox).
    pub own: Option<OwnCore>,
}

/// The Download row's line while this version has no download channel: an emulator app on the phone that takes the system is named,
/// with where to choose it; without one the page says plainly that the system can't be played here. Never "an emulator app can
/// play it" on a phone that has none (Odin 2 Mini 13.11.: N64, PSP, DS and Dreamcast said so with no app installed).
pub fn closed_line(labels: &[(String, String)]) -> String {
    match labels.first() {
        Some((_, app)) => format!("This version of pocket-ui can't download this system. {app} on this device plays it: choose it under Runs on."),
        None => "This version of pocket-ui can't play this system, and no emulator app on this device plays it.".to_string(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnCore {
    pub core: String,
    /// The core as the download channel names it ("PSP"), and its size in bytes.
    pub name: String,
    pub size: u64,
    pub present: bool,
    /// The channel has an address; without one a download core reads "Not available here".
    pub channel_open: bool,
}

/// How the Download row stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Download {
    #[default]
    Idle,
    /// Running: the line it shows ("Downloading · 4.1 MB / 18.6 MB").
    Busy(String),
    /// It ended without the core: why, in a sentence.
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    RunsOn,
    /// Android: adds our player's core for this system; only while our player is the way and its core is not there.
    Download,
    /// An entry of `RA_SETTINGS`.
    Ra(usize),
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveChoice {
    Console,
    Game,
    Defaults,
}

impl SaveChoice {
    pub fn label(self) -> &'static str {
        match self {
            SaveChoice::Console => "For this console",
            SaveChoice::Game => "For this game",
            SaveChoice::Defaults => "Restore defaults",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    List,
    /// "Save changes" is open, on this choice.
    Save {
        sel: usize,
    },
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Android: fetch this core (the Download row).
    Download { core: String },
    /// Write these to system.cfg (None removes the key); `clear_fallback`: pocket-core was chosen for a game it
    /// couldn't start before, so it gets another try (`fallback.json`).
    Save {
        edits: Vec<(String, Option<String>)>,
        clear_fallback: bool,
    },
}

/// `EmuChoicePage::mark`: the selection, the mode, the runner and the RetroArch values.
pub type Mark = (usize, Mode, String, Vec<(&'static str, String)>);

pub struct EmuChoicePage {
    pub subject: Subject,
    /// system.cfg as it is now (the page's own copy, kept up to date with its saves), and as ROCKNIX ships it.
    cfg: Cfg,
    shipped: Cfg,
    /// What can be picked, and whether it is installed.
    choices: Vec<(Runner, bool)>,
    /// What runs when nothing is chosen at this page's level (`default_runner`): first in the list, as "Default (…)".
    default: Runner,
    /// The values on screen.
    pub runner: Runner,
    pub ra: HashMap<&'static str, String>,
    pub sel: usize,
    pub mode: Mode,
    /// Shown after a save ("Saved for this game.").
    pub note: Option<String>,
    /// Android: the Download row's state.
    pub download: Download,
}

impl EmuChoicePage {
    /// `installed`: whether ROCKNIX can start a pair here (`emu_choice::installed` with the real file system).
    pub fn new(subject: Subject, cfg: Cfg, shipped: Cfg, installed: impl Fn(&Runner) -> bool) -> EmuChoicePage {
        let mut page = EmuChoicePage {
            subject,
            cfg,
            shipped,
            choices: Vec::new(),
            default: Runner::default(),
            runner: Runner::default(),
            ra: HashMap::new(),
            sel: 0,
            mode: Mode::List,
            note: None,
            download: Download::Idle,
        };
        page.choices = emu_choice::choices(&page.ctx(page.subject.fell_back))
            .into_iter()
            .map(|r| {
                let ok = installed(&r);
                (r, ok)
            })
            .collect();
        page.reload();
        page
    }

    fn ctx(&self, fell_back: bool) -> Ctx<'_> {
        let s = &self.subject;
        Ctx {
            system: &s.system,
            emulators: &s.emulators,
            pocket_core: s.pocket_core.as_deref(),
            pocket_cores: s.pocket_cores,
            file_ok: s.file_ok,
            fell_back,
            default_override: s.default_override.as_ref().map(|(e, c)| (e.as_str(), c.as_str())),
        }
    }

    fn file(&self) -> Option<&str> {
        self.subject.game.as_ref().map(|(_, f)| f.as_str())
    }

    /// The values in use, from system.cfg.
    fn in_use(&self) -> (Runner, HashMap<&'static str, String>) {
        let (sys, file) = (self.subject.system.as_str(), self.file());
        let ra = RA_SETTINGS
            .iter()
            .map(|s| (s.key, emu_choice::effective_value(&self.cfg, sys, file, s)))
            .collect();
        (self.resolved().runner, ra)
    }

    fn reload(&mut self) {
        (self.runner, self.ra) = self.in_use();
        self.default = self.default_runner();
    }

    /// What the game starts with without a choice of its own (the console's choice, else the default), or the console
    /// without one (pocket-core where it hosts the system, else ROCKNIX's default). Picking it and saving removes the
    /// level's emulator lines, so a player can always go back to the default (`edits`). A game pocket-core couldn't
    /// start counts with pocket-core: choosing it gives pocket-core another try.
    fn default_runner(&self) -> Runner {
        let (sys, file) = (self.subject.system.as_str(), self.file());
        let level = if file.is_some() { Level::Game } else { Level::Console };
        let mut cfg = self.cfg.clone();
        cfg.apply(&[(emu_choice::key(sys, file, level, "emulator"), None), (emu_choice::key(sys, file, level, "core"), None)]);
        emu_choice::resolve(&self.ctx(false), &cfg, file).runner
    }

    /// What the game (or the console) starts with now.
    pub fn resolved(&self) -> emu_choice::Resolved {
        emu_choice::resolve(&self.ctx(self.subject.fell_back), &self.cfg, self.file())
    }

    /// The rows top to bottom: the RetroArch settings only while RetroArch is chosen.
    pub fn rows(&self) -> Vec<Row> {
        let mut v = vec![Row::RunsOn];
        if self.download_offered() {
            v.push(Row::Download);
        }
        if self.runner.kind() == Kind::RetroArch {
            v.extend(
                RA_SETTINGS
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.applies(&self.subject.system))
                    .map(|(i, _)| Row::Ra(i)),
            );
        }
        v.push(Row::Save);
        v
    }

    /// Android: our player is what plays now and its core is not on the phone: the page says so in a row of its own.
    pub fn download_offered(&self) -> bool {
        self.runner.is_pocket_core() && self.subject.phone.as_ref().and_then(|p| p.own.as_ref()).is_some_and(|o| !o.present)
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    pub fn save_choices(&self) -> Vec<SaveChoice> {
        if self.subject.game.is_some() {
            vec![SaveChoice::Console, SaveChoice::Game, SaveChoice::Defaults]
        } else {
            vec![SaveChoice::Console, SaveChoice::Defaults]
        }
    }

    pub fn label(&self, row: Row) -> &'static str {
        match row {
            Row::RunsOn => "Runs on",
            Row::Download => "Download",
            Row::Ra(i) => RA_SETTINGS[i].name,
            Row::Save => "Save changes",
        }
    }

    pub fn runner_label(&self, r: &Runner) -> String {
        if let Some(phone) = &self.subject.phone {
            return match phone.labels.iter().find(|(package, _)| *package == r.emulator) {
                _ if r.is_pocket_core() => "pocket-ui player".to_string(),
                Some((_, name)) => name.clone(),
                None => emu_choice::emulator_label(&r.emulator),
            };
        }
        emu_choice::label(r, &self.subject.emulators)
    }

    pub fn value(&self, row: Row) -> String {
        match row {
            Row::RunsOn if self.runner == self.default => format!("Default ({})", self.runner_label(&self.runner)),
            Row::RunsOn => self.runner_label(&self.runner),
            Row::Ra(i) => {
                let s = &RA_SETTINGS[i];
                s.value_label(self.ra.get(s.key).map_or(s.unset, String::as_str))
            }
            Row::Download => self.download_value(),
            Row::Save => ">".to_string(),
        }
    }

    fn download_value(&self) -> String {
        let Some(own) = self.subject.phone.as_ref().and_then(|p| p.own.as_ref()) else { return String::new() };
        match &self.download {
            Download::Busy(line) => line.clone(),
            Download::Failed(why) => why.clone(),
            Download::Idle if !own.channel_open => "Not available here".to_string(),
            Download::Idle => format!("{} · {:.1} MB", own.name, own.size as f64 / 1_000_000.0),
        }
    }

    /// The selected row's explanation, and what its value means.
    pub fn describe(&self, row: Row) -> [String; 2] {
        match row {
            Row::RunsOn if self.subject.phone.is_some() => {
                let what = match self.subject.game {
                    Some(_) => "Which way plays this game: pocket-ui's own player or another emulator app on this device.",
                    None => "Which way plays this console's games: pocket-ui's own player or another emulator app on this device.",
                }
                .to_string();
                let choice = if self.runner.is_pocket_core() {
                    if self.download_offered() {
                        "This system is not installed yet. Choose Download now to add it on Wi-Fi."
                    } else {
                        "pocket-ui's own player: Continue where you left off, Save & quit and the shelf's start and end."
                    }
                } else {
                    "Another app starts the game and brings you back here. Continue and the pause menu are then that app's own."
                };
                [what, choice.to_string()]
            }
            Row::Download => {
                let open = self.subject.phone.as_ref().and_then(|p| p.own.as_ref()).is_some_and(|o| o.channel_open);
                [
                    "Adds this system to pocket-ui's own player. It downloads on unmetered Wi-Fi.".to_string(),
                    if open { "This system is not installed yet. Choose Download now to add it on Wi-Fi.".to_string() } else { closed_line(self.subject.phone.as_ref().map_or(&[][..], |p| &p.labels[..])) },
                ]
            }
            Row::RunsOn => {
                let what = match self.subject.game {
                    Some(_) => "Which emulator plays this game. Save states don't carry over to another one.",
                    None => "Which emulator plays this console's games. Save states don't carry over to another one.",
                }
                .to_string();
                let installed = self.choices.iter().find(|(r, _)| *r == self.runner).is_none_or(|(_, ok)| *ok);
                let mut choice = emu_choice::note(&self.runner, &self.subject.system);
                if !installed {
                    choice = format!("Not installed on this Pocket, so it can't start. {choice}");
                }
                [what, choice]
            }
            Row::Ra(i) => {
                let s = &RA_SETTINGS[i];
                let mut what = s.desc.to_string();
                if s.key == "shaderset" && emu_choice::shader_presets().is_empty() {
                    what.push_str(" No presets found yet.");
                }
                [what, "RetroArch reads it the next time the game starts.".to_string()]
            }
            Row::Save => ["Keeps these choices for this console or this game.".to_string(), self.level_line()],
        }
    }

    /// Which settings are in use, as the pause menu's Options says it.
    pub fn level_line(&self) -> String {
        let r = self.resolved();
        if r.from == From::Fallback {
            return format!(
                "pocket-core couldn't start this game before, so it runs with {}. Choose pocket-core and save to try again.",
                self.runner_label(&r.runner)
            );
        }
        match self.level() {
            Some(Level::Game) => "Using this game's settings.".to_string(),
            Some(Level::Console) => "Using this console's settings.".to_string(),
            None => "Using the defaults.".to_string(),
        }
    }

    /// The deepest level with a setting of the page's: the game's keys, else the console's (an emulator choice or a
    /// RetroArch value other than ROCKNIX's shipped one), else none.
    pub fn level(&self) -> Option<Level> {
        let sys = &self.subject.system;
        let names = || ["emulator", "core"].into_iter().chain(RA_SETTINGS.iter().map(|s| s.key));
        if let Some(f) = self.file() {
            if names().any(|n| self.cfg.get(&emu_choice::key(sys, Some(f), Level::Game, n)).is_some()) {
                return Some(Level::Game);
            }
        }
        let console = names().any(|n| {
            let k = emu_choice::key(sys, None, Level::Console, n);
            match n {
                "emulator" | "core" => self.cfg.get(&k).is_some(),
                _ => self.cfg.get(&k) != self.shipped.get(&k),
            }
        });
        console.then_some(Level::Console)
    }

    /// The line under the page while RetroArch is chosen: its shortcuts on this Pocket.
    pub fn footer(&self) -> Option<&'static str> {
        (self.runner.kind() == Kind::RetroArch).then_some("RetroArch's shortcuts: HOME + R2 fast-forward, hold HOME + L2 to rewind (with Rewind on).")
    }

    /// Changes on screen that aren't saved yet.
    pub fn unsaved(&self) -> bool {
        let (runner, ra) = self.in_use();
        runner != self.runner || (self.runner.kind() == Kind::RetroArch && ra != self.ra)
    }

    /// The values Left/Right step through on a RetroArch row: its choices, the presets found on the Pocket for the
    /// shader, and the value in use when it is none of them (set in ES: kept, not lost).
    fn ra_values(&self, i: usize) -> Vec<String> {
        let s = &RA_SETTINGS[i];
        let mut v: Vec<String> = s.choices.iter().map(|c| c.value.to_string()).collect();
        if s.key == "shaderset" {
            v.extend(emu_choice::shader_presets().iter().cloned());
        }
        if let Some(now) = self.ra.get(s.key) {
            if !v.contains(now) {
                v.push(now.clone());
            }
        }
        v
    }

    /// Left/Right (and A) change `row`: Runs on only when there is another emulator to step to.
    pub fn changes(&self, row: Row) -> bool {
        match row {
            Row::RunsOn => self.runners().len() > 1,
            Row::Download => false,
            _ => true,
        }
    }

    /// The runners Left/Right step through: the default first, then the installed ones and the one in use.
    fn runners(&self) -> Vec<Runner> {
        let mut v = vec![self.default.clone()];
        v.extend(
            self.choices
                .iter()
                .filter(|(r, ok)| (*ok || *r == self.runner) && *r != self.default)
                .map(|(r, _)| r.clone()),
        );
        v
    }

    fn step(&mut self, row: Row, delta: i32) {
        let walk = |list: &[String], now: &str| -> Option<String> {
            let n = list.len() as i32;
            (n > 0).then(|| {
                let i = list.iter().position(|v| v == now).map_or(0, |i| i as i32);
                list[(i + delta).rem_euclid(n) as usize].clone()
            })
        };
        match row {
            // No wrap: Left always ends on "Default (…)", and never jumps from the first core to the last emulator.
            Row::RunsOn => {
                let list = self.runners();
                let i = list.iter().position(|r| *r == self.runner).map_or(0, |i| i as i32);
                self.runner = list[(i + delta).clamp(0, list.len() as i32 - 1) as usize].clone();
            }
            Row::Ra(i) => {
                let key = RA_SETTINGS[i].key;
                let now = self.ra.get(key).cloned().unwrap_or_default();
                if let Some(v) = walk(&self.ra_values(i), &now) {
                    self.ra.insert(key, v);
                }
            }
            Row::Download | Row::Save => {}
        }
        self.note = None;
    }

    /// A button: `confirm`/`cancel` are the App's A and B.
    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        match self.mode {
            Mode::List => {
                let n = self.rows().len();
                let row = self.row();
                match b {
                    Button::Up => self.sel = (self.sel + n - 1) % n,
                    Button::Down => self.sel = (self.sel + 1) % n,
                    Button::Left => self.step(row, -1),
                    Button::Right => self.step(row, 1),
                    b if b == confirm && row == Row::Save => self.mode = Mode::Save { sel: 0 },
                    b if b == confirm && row == Row::Download => return self.download_pressed(),
                    b if b == confirm => self.step(row, 1),
                    b if b == cancel => return Action::Close,
                    _ => {}
                }
                // Rows come and go with the runner; the selection stays on Runs on or Save changes.
                let rows = self.rows();
                if self.sel >= rows.len() {
                    self.sel = rows.len() - 1;
                }
                Action::Nothing
            }
            Mode::Save { sel } => {
                let choices = self.save_choices();
                let n = choices.len();
                match b {
                    Button::Up | Button::Left => self.mode = Mode::Save { sel: (sel + n - 1) % n },
                    Button::Down | Button::Right => self.mode = Mode::Save { sel: (sel + 1) % n },
                    b if b == cancel => self.mode = Mode::List,
                    b if b == confirm => {
                        let choice = choices[sel.min(n - 1)];
                        let (mut edits, clear_fallback, note) = self.edits(choice);
                        // Only what changes goes to the file: no line removed that isn't there, none rewritten as it is.
                        edits.retain(|(k, v)| self.cfg.get(k) != v.as_deref().filter(|v| !v.is_empty()));
                        self.cfg.apply(&edits);
                        if clear_fallback {
                            self.subject.fell_back = false;
                        }
                        self.reload();
                        self.mode = Mode::List;
                        self.note = Some(note);
                        return Action::Save { edits, clear_fallback };
                    }
                    _ => {}
                }
                Action::Nothing
            }
        }
    }

    /// A on the Download row: the host fetches the core, unless it is already busy or the channel isn't open.
    fn download_pressed(&mut self) -> Action {
        let Some(own) = self.subject.phone.as_ref().and_then(|p| p.own.clone()) else { return Action::Nothing };
        if !own.channel_open || self.download_busy() {
            return Action::Nothing;
        }
        self.download = Download::Busy(String::new());
        self.note = None;
        Action::Download { core: own.core }
    }

    /// A on the Download row does something: the channel is open and no download runs.
    pub fn download_ready(&self) -> bool {
        self.subject.phone.as_ref().and_then(|p| p.own.as_ref()).is_some_and(|o| o.channel_open) && !self.download_busy()
    }

    pub fn download_busy(&self) -> bool {
        matches!(self.download, Download::Busy(_))
    }

    /// The host's word on the download: the core is on the phone now (the row goes, our player is ready), it failed, or
    /// how far it is.
    pub fn download_state(&mut self, state: Download, installed: Option<bool>) {
        if installed == Some(true) {
            if let Some(own) = self.subject.phone.as_mut().and_then(|p| p.own.as_mut()) {
                own.present = true;
            }
            for (_, ok) in &mut self.choices {
                *ok = true;
            }
            self.download = Download::Idle;
            self.note = Some("System installed. You can start your game now.".to_string());
            let rows = self.rows().len();
            self.sel = self.sel.min(rows - 1);
        } else {
            self.download = state;
        }
    }

    /// The system.cfg edits of a save, whether the game's fallback ends, and what the page says afterwards.
    pub fn edits(&self, choice: SaveChoice) -> (Vec<(String, Option<String>)>, bool, String) {
        let sys = self.subject.system.as_str();
        let file = self.file();
        let ra_shown = self.runner.kind() == Kind::RetroArch;
        let applicable = || RA_SETTINGS.iter().filter(|s| s.applies(sys));
        let set = |k: String, v: &str| (k, Some(v.to_string()));
        let del = |k: String| (k, None);
        let mut edits: Vec<(String, Option<String>)> = Vec::new();
        let game_keys = |edits: &mut Vec<(String, Option<String>)>| {
            if let Some(f) = file {
                for n in ["emulator", "core"].into_iter().chain(RA_SETTINGS.iter().map(|s| s.key)) {
                    let k = emu_choice::key(sys, Some(f), Level::Game, n);
                    if self.cfg.get(&k).is_some() {
                        edits.push(del(k));
                    }
                }
            }
        };
        let pocket_retry = self.runner.is_pocket_core() && self.subject.fell_back;
        match choice {
            SaveChoice::Console => {
                let (ek, ck) = (
                    emu_choice::key(sys, None, Level::Console, "emulator"),
                    emu_choice::key(sys, None, Level::Console, "core"),
                );
                // What this page shows when nobody chose: pocket-core for the console, ROCKNIX's default for a game
                // pocket-core doesn't take (a 7z). Saving that is no choice: the console's lines go, as ES's AUTO.
                let mut unchosen = self.cfg.clone();
                unchosen.apply(&[(ek.clone(), None), (ck.clone(), None)]);
                if let Some(f) = file {
                    unchosen.apply(&[
                        (emu_choice::key(sys, Some(f), Level::Game, "emulator"), None),
                        (emu_choice::key(sys, Some(f), Level::Game, "core"), None),
                    ]);
                }
                let auto = emu_choice::resolve(&self.ctx(self.subject.fell_back && !pocket_retry), &unchosen, file).runner;
                // A game with a runner of its own by default (Jeanne d'Arc): what its page saves for the console is a
                // choice for the other games too, even when it matches what this one starts with.
                let own_default = file.is_some_and(|f| emu_choice::game_default(&self.ctx(false), f).is_some());
                if self.runner == auto && !own_default {
                    edits.extend([del(ek), del(ck)]);
                } else {
                    edits.push(set(ek, &self.runner.emulator));
                    edits.push(if self.runner.core.is_empty() { del(ck) } else { set(ck, &self.runner.core) });
                }
                if ra_shown {
                    for s in applicable() {
                        let k = emu_choice::key(sys, None, Level::Console, s.key);
                        let below = emu_choice::below_console(&self.cfg, s.key).unwrap_or_else(|| s.unset.to_string());
                        let v = self.ra.get(s.key).cloned().unwrap_or_else(|| s.unset.to_string());
                        edits.push(if v == below { del(k) } else { set(k, &v) });
                    }
                }
                game_keys(&mut edits);
                (edits, pocket_retry, "Saved for this console.".to_string())
            }
            SaveChoice::Game => {
                let Some(f) = file else { return (edits, false, String::new()) };
                let (ek, ck) = (
                    emu_choice::key(sys, Some(f), Level::Game, "emulator"),
                    emu_choice::key(sys, Some(f), Level::Game, "core"),
                );
                // What the game would run with from the console's side (pocket-core given another try if chosen).
                let mut without_game = self.cfg.clone();
                without_game.apply(&[(ek.clone(), None), (ck.clone(), None)]);
                let console = emu_choice::resolve(&self.ctx(self.subject.fell_back && !pocket_retry), &without_game, Some(f)).runner;
                let mut own = false;
                if self.runner == console {
                    edits.extend([del(ek), del(ck)]);
                } else {
                    own = true;
                    edits.push(set(ek, &self.runner.emulator));
                    edits.push(if self.runner.core.is_empty() { del(ck) } else { set(ck, &self.runner.core) });
                }
                let mut unreadable = false;
                if ra_shown {
                    if emu_choice::findable(f) {
                        for s in applicable() {
                            let k = emu_choice::key(sys, Some(f), Level::Game, s.key);
                            let v = self.ra.get(s.key).cloned().unwrap_or_else(|| s.unset.to_string());
                            if v == emu_choice::console_value(&self.cfg, sys, s) {
                                edits.push(del(k));
                            } else {
                                own = true;
                                edits.push(set(k, &v));
                            }
                        }
                    } else {
                        unreadable = self
                            .ra
                            .iter()
                            .any(|(k, v)| RA_SETTINGS.iter().any(|s| s.key == *k && *v != emu_choice::console_value(&self.cfg, sys, s)));
                    }
                }
                let note = if unreadable {
                    "Saved the emulator for this game. ROCKNIX can't read RetroArch settings for this file name; save them for the console instead."
                } else if own {
                    "Saved for this game."
                } else {
                    "Nothing differs from the console's settings: the game uses those."
                };
                (edits, pocket_retry, note.to_string())
            }
            SaveChoice::Defaults => {
                edits.push(del(emu_choice::key(sys, None, Level::Console, "emulator")));
                edits.push(del(emu_choice::key(sys, None, Level::Console, "core")));
                for s in RA_SETTINGS {
                    let k = emu_choice::key(sys, None, Level::Console, s.key);
                    let shipped = self.shipped.get(&k).map(str::to_string);
                    if self.cfg.get(&k).map(str::to_string) != shipped {
                        edits.push((k, shipped));
                    }
                }
                game_keys(&mut edits);
                (edits, false, "Back to the defaults.".to_string())
            }
        }
    }

    /// Where the page stands, for the menu sounds and haptics.
    pub fn mark(&self) -> Mark {
        let mut ra: Vec<(&'static str, String)> = self.ra.iter().map(|(k, v)| (*k, v.clone())).collect();
        ra.sort();
        (self.sel, self.mode, format!("{}/{}", self.runner.emulator, self.runner.core), ra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gba() -> Vec<EsEmulator> {
        let e = |name: &str, cores: &[&str], default: Option<usize>| EsEmulator {
            name: name.into(),
            cores: cores.iter().map(|c| c.to_string()).collect(),
            default_core: default,
        };
        vec![
            e("retroarch", &["mgba", "vbam", "vba_next", "beetle_gba", "skyemu", "gpsp"], Some(0)),
            e("nanoboyadvance", &["nanoboyadvance-sa"], None),
            e("mednafen", &["gba"], None),
            e("skyemu", &["skyemu-sa"], None),
        ]
    }

    fn subject(game: Option<&str>) -> Subject {
        Subject {
            system: "gba".into(),
            system_name: "Game Boy Advance".into(),
            game: game.map(|f| (f.trim_end_matches(".gba").to_string(), f.to_string())),
            emulators: gba(),
            pocket_core: Some("mgba".into()),
            pocket_cores: &[],
            default_override: None,
            file_ok: true,
            fell_back: false,
            phone: None,
        }
    }

    const SHIPPED: &str = "gba.ratio=3/2\nglobal.ratio=core\nglobal.rewind=0\nglobal.runahead=0\nglobal.autosave=1\n";

    fn page(game: Option<&str>, cfg: &str) -> EmuChoicePage {
        EmuChoicePage::new(subject(game), Cfg::parse(cfg), Cfg::parse(SHIPPED), |r| r.core != "gpsp")
    }

    /// Jeanne d'Arc starts in standalone PPSSPP by default; "Save for this console" on its page must still reach the
    /// other PSP games, and pocket-core chosen there must win over the game's own default.
    #[test]
    fn saving_for_the_console_from_a_game_with_its_own_default_applies_to_the_console() {
        let e = |name: &str, cores: &[&str], default: Option<usize>| EsEmulator { name: name.into(), cores: cores.iter().map(|c| c.to_string()).collect(), default_core: default };
        let file = "Jeanne d'Arc (USA).chd";
        let psp = || Subject {
            system: "psp".into(),
            system_name: "PSP".into(),
            game: Some(("Jeanne d'Arc (USA)".into(), file.into())),
            emulators: vec![e("ppsspp", &["ppsspp-sa"], Some(0)), e("retroarch", &["ppsspp"], None)],
            pocket_core: Some("ppsspp".into()),
            pocket_cores: &[],
            default_override: None,
            file_ok: true,
            fell_back: false,
            phone: None,
        };
        let mut p = EmuChoicePage::new(psp(), Cfg::default(), Cfg::default(), |_| true);
        assert_eq!(p.runner, Runner::new("ppsspp", "ppsspp-sa"), "the page shows what the game starts with");
        let edits = save(&mut p, SaveChoice::Console);
        assert!(edits.contains(&("psp.emulator".into(), Some("ppsspp".into()))), "{edits:?}");
        assert!(edits.contains(&("psp.core".into(), Some("ppsspp-sa".into()))), "{edits:?}");
        let mut p = EmuChoicePage::new(psp(), Cfg::default(), Cfg::default(), |_| true);
        p.runner = Runner::new("pocket-core", "ppsspp");
        let edits = save(&mut p, SaveChoice::Console);
        assert!(edits.contains(&("psp.emulator".into(), Some("pocket-core".into()))), "{edits:?}");
        assert_eq!(p.resolved().runner, Runner::new("pocket-core", "ppsspp"), "the console's choice wins over the game default");
    }

    const A: Button = Button::East;
    const B: Button = Button::South;

    /// Saves through the buttons; returns every key the save decides on (`edits`, before the unchanged ones are
    /// left out of what goes to the file).
    fn save(p: &mut EmuChoicePage, choice: SaveChoice) -> Vec<(String, Option<String>)> {
        p.sel = p.rows().iter().position(|r| *r == Row::Save).unwrap();
        assert_eq!(p.press(A, A, B), Action::Nothing);
        let at = p.save_choices().iter().position(|c| *c == choice).unwrap();
        p.mode = Mode::Save { sel: at };
        let (all, _, _) = p.edits(choice);
        let before = p.cfg.clone();
        match p.press(A, A, B) {
            Action::Save { edits, .. } => {
                // What goes to the file is what changes.
                assert!(edits.iter().all(|(k, v)| before.get(k) != v.as_deref()), "{edits:?}");
                assert!(all.iter().filter(|(k, v)| before.get(k) != v.as_deref()).all(|e| edits.contains(e)));
            }
            other => panic!("{other:?}"),
        }
        all
    }

    #[test]
    fn opens_on_what_runs_and_steps_through_installed_choices() {
        let mut p = page(Some("Zelda.gba"), SHIPPED);
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · mGBA)");
        assert_eq!(p.rows(), [Row::RunsOn, Row::Save], "no RetroArch rows for pocket-core");
        let mut seen = Vec::new();
        for _ in 0..12 {
            p.press(Button::Right, A, B);
            seen.push(p.value(Row::RunsOn));
        }
        assert!(seen.contains(&"RetroArch · VBA-M".to_string()));
        assert!(!seen.iter().any(|s| s.contains("gpSP")), "not installed: skipped");
        assert!(seen.contains(&"Standalone · NanoBoyAdvance".to_string()));
        // On RetroArch the RetroArch rows appear, with what setsettings.sh would use now.
        p.runner = Runner::new("retroarch", "mgba");
        let rows = p.rows();
        assert_eq!(rows.len(), 2 + RA_SETTINGS.iter().filter(|s| s.applies("gba")).count(), "phone games' D-pad row is not a GBA's");
        assert!(RA_SETTINGS.iter().any(|s| !s.applies("gba")));
        assert_eq!(p.value(Row::Ra(0)), "3:2", "gba.ratio from the shipped file");
        assert_eq!(p.value(Row::Ra(6)), "On", "global.autosave=1");
        assert!(p.unsaved());
    }

    /// 06.11. on a real Pocket: there was no way back to the default core but naming it, and Left on mGBA jumped to
    /// "Standalone · SkyEmu" (the end of the list) instead of gpSP.
    // ROCKNIX H700 (`pocket_cores`); the Brick names gpSP/mGBA in its own es_systems.cfg (`emu_choice`'s Brick tests).
    #[cfg(not(feature = "brick"))]
    #[test]
    fn default_comes_first_and_left_stops_there() {
        // H700: pocket-core plays GBA with gpSP, mGBA beside it.
        let mut s = subject(Some("Aero.gba"));
        s.pocket_core = Some("gpsp".into());
        s.pocket_cores = crate::h700_gba::CORES;
        let cfg = format!("{SHIPPED}gba[\"Aero.gba\"].emulator=pocket-core\ngba[\"Aero.gba\"].core=mgba\n");
        let mut p = EmuChoicePage::new(s.clone(), Cfg::parse(&cfg), Cfg::parse(SHIPPED), |_| true);
        assert_eq!(p.value(Row::RunsOn), "pocket-core · mGBA");
        assert_eq!(p.level_line(), "Using this game's settings.");
        p.press(Button::Left, A, B);
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · gpSP)");
        p.press(Button::Left, A, B);
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · gpSP)", "the start of the list: no wrap to the end");
        let edits = save(&mut p, SaveChoice::Game);
        assert!(edits.contains(&("gba[\"Aero.gba\"].core".into(), None)), "the game's own choice goes: {edits:?}");
        assert_eq!(p.level_line(), "Using the defaults.");
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · gpSP)");
        // At the end of the list Right stays too.
        for _ in 0..30 {
            p.press(Button::Right, A, B);
        }
        let last = p.value(Row::RunsOn);
        p.press(Button::Right, A, B);
        assert_eq!(p.value(Row::RunsOn), last);
        // mGBA as the default (a Retroid Pocket with gpSP beside it): Left from gpSP goes to the default, never past it.
        s.pocket_core = Some("mgba".into());
        let mut p = EmuChoicePage::new(s, Cfg::parse(SHIPPED), Cfg::parse(SHIPPED), |_| true);
        p.press(Button::Right, A, B);
        assert_eq!(p.value(Row::RunsOn), "pocket-core · gpSP");
        p.press(Button::Left, A, B);
        p.press(Button::Left, A, B);
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · mGBA)");
    }

    /// On a game's page the default is what the console gives it: back to "Default" drops only the game's choice.
    #[test]
    fn a_games_default_is_the_consoles_choice() {
        let cfg = format!("{SHIPPED}gba.emulator=retroarch\ngba.core=vbam\ngba[\"Zelda.gba\"].emulator=retroarch\ngba[\"Zelda.gba\"].core=mgba\n");
        let mut p = page(Some("Zelda.gba"), &cfg);
        assert_eq!(p.value(Row::RunsOn), "RetroArch · mGBA");
        for _ in 0..30 {
            p.press(Button::Left, A, B);
        }
        assert_eq!(p.value(Row::RunsOn), "Default (RetroArch · VBA-M)");
        let edits = save(&mut p, SaveChoice::Game);
        assert!(edits.contains(&("gba[\"Zelda.gba\"].emulator".into(), None)), "{edits:?}");
        assert!(!edits.iter().any(|(k, _)| k.starts_with("gba.")), "the console's choice stays: {edits:?}");
        assert_eq!(p.level_line(), "Using this console's settings.");
    }

    #[test]
    fn an_es_choice_is_shown_and_kept() {
        // Chosen in EmulationStation, gpSP isn't installed: still shown, marked, and kept unless changed.
        let p = page(
            Some("Zelda.gba"),
            &format!("{SHIPPED}gba.emulator=retroarch\ngba.core=gpsp\ngba.shaderset=crt/crt-royale.slangp\n"),
        );
        assert_eq!(p.value(Row::RunsOn), "Default (RetroArch · gpSP)", "the console's choice is the game's default");
        assert!(p.describe(Row::RunsOn)[1].starts_with("Not installed"));
        assert_eq!(p.value(Row::Ra(3)), "crt-royale", "a shader chosen in ES");
        assert_eq!(p.level(), Some(Level::Console));
        assert_eq!(p.level_line(), "Using this console's settings.");
        assert!(!p.unsaved());
    }

    #[test]
    fn saving_for_the_console_writes_rocknix_keys_and_drops_the_games() {
        let mut p = page(
            Some("Zelda.gba"),
            &format!("{SHIPPED}gba[\"Zelda.gba\"].core=vbam\ngba[\"Zelda.gba\"].emulator=retroarch\n"),
        );
        assert_eq!(p.value(Row::RunsOn), "RetroArch · VBA-M");
        assert_eq!(p.level(), Some(Level::Game));
        p.runner = Runner::new("retroarch", "mgba");
        p.ra.insert("rewind", "1".into());
        let edits = save(&mut p, SaveChoice::Console);
        let get = |k: &str| edits.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        assert_eq!(get("gba.emulator"), Some(Some("retroarch".into())));
        assert_eq!(get("gba.core"), Some(Some("mgba".into())));
        assert_eq!(get("gba.rewind"), Some(Some("1".into())));
        assert_eq!(
            get("gba.ratio"),
            Some(Some("3/2".into())),
            "differs from global.ratio, so it stays the console's"
        );
        assert_eq!(get("gba.autosave"), Some(None), "same as global.autosave: no line of its own");
        assert_eq!(get("gba[\"Zelda.gba\"].core"), Some(None), "the game's own choice goes");
        assert_eq!(p.level_line(), "Using this console's settings.");
        assert_eq!(p.note.as_deref(), Some("Saved for this console."));
        // Back to pocket-core, the default: the console's lines go, as ES writes AUTO.
        p.runner = Runner::new("pocket-core", "mgba");
        let edits = save(&mut p, SaveChoice::Console);
        assert!(edits.contains(&("gba.emulator".into(), None)) && edits.contains(&("gba.core".into(), None)));
        assert!(
            !edits.iter().any(|(k, _)| k == "gba.rewind"),
            "RetroArch rows are left alone while pocket-core is chosen"
        );
    }

    #[test]
    fn saving_for_the_game_keeps_only_what_differs() {
        let mut p = page(Some("Zelda.gba"), SHIPPED);
        p.runner = Runner::new("retroarch", "vbam");
        p.ra.insert("integerscale", "1".into());
        let edits = save(&mut p, SaveChoice::Game);
        let get = |k: &str| edits.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        assert_eq!(get("gba[\"Zelda.gba\"].emulator"), Some(Some("retroarch".into())));
        assert_eq!(get("gba[\"Zelda.gba\"].core"), Some(Some("vbam".into())));
        assert_eq!(get("gba[\"Zelda.gba\"].integerscale"), Some(Some("1".into())));
        assert_eq!(get("gba[\"Zelda.gba\"].ratio"), Some(None), "the console's 3:2: nothing of its own");
        assert_eq!(p.level(), Some(Level::Game));
        assert_eq!(p.note.as_deref(), Some("Saved for this game."));
        // The same as the console: nothing of its own.
        let mut q = page(Some("Other.gba"), SHIPPED);
        let edits = save(&mut q, SaveChoice::Game);
        assert!(edits.iter().all(|(_, v)| v.is_none()));
        assert_eq!(q.note.as_deref(), Some("Nothing differs from the console's settings: the game uses those."));
        // A console page has no "For this game".
        assert_eq!(page(None, SHIPPED).save_choices(), [SaveChoice::Console, SaveChoice::Defaults]);
    }

    #[test]
    fn restore_defaults_goes_back_to_the_shipped_file() {
        let cfg = format!("{SHIPPED}gba.emulator=retroarch\ngba.core=vbam\ngba.ratio=16/9\ngba.rewind=1\ngba[\"Zelda.gba\"].core=gpsp\n");
        let mut p = page(Some("Zelda.gba"), &cfg);
        let edits = save(&mut p, SaveChoice::Defaults);
        assert!(edits.contains(&("gba.ratio".into(), Some("3/2".into()))), "{edits:?}");
        assert!(edits.contains(&("gba.rewind".into(), None)));
        assert!(edits.contains(&("gba.emulator".into(), None)));
        assert!(edits.contains(&("gba[\"Zelda.gba\"].core".into(), None)));
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · mGBA)");
        assert_eq!(p.level(), None);
        assert_eq!(p.level_line(), "Using the defaults.");
    }

    #[test]
    fn a_game_pocket_core_couldnt_start_gets_another_try_when_chosen() {
        let mut s = subject(Some("Broken.gba"));
        s.fell_back = true;
        let mut p = EmuChoicePage::new(s, Cfg::parse(SHIPPED), Cfg::parse(SHIPPED), |_| true);
        assert_eq!(p.value(Row::RunsOn), "RetroArch · mGBA");
        assert!(p.level_line().starts_with("pocket-core couldn't start this game before"));
        p.runner = Runner::new("pocket-core", "mgba");
        p.sel = p.rows().len() - 1;
        p.press(A, A, B);
        p.mode = Mode::Save { sel: 1 };
        let Action::Save { edits, clear_fallback } = p.press(A, A, B) else { panic!() };
        assert!(clear_fallback);
        assert!(edits.iter().all(|(_, v)| v.is_none()), "pocket-core is the default: no keys, just the retry");
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-core · mGBA)");
    }

    #[test]
    fn rows_skip_rewind_and_run_ahead_where_rocknix_switches_them_off() {
        let mut s = subject(None);
        s.system = "n64".into();
        s.pocket_core = None;
        s.emulators = vec![EsEmulator {
            name: "retroarch".into(),
            cores: vec!["mupen64plus_next".into()],
            default_core: Some(0),
        }];
        let p = EmuChoicePage::new(s, Cfg::default(), Cfg::default(), |_| true);
        let names: Vec<&str> = p.rows().iter().map(|r| p.label(*r)).collect();
        assert!(!names.contains(&"Rewind") && !names.contains(&"Run-ahead"), "{names:?}");
        assert!(names.contains(&"Shader"));
        assert!(p.footer().is_some_and(|f| f.contains("HOME + R2")));
    }

    #[test]
    fn walking_and_cancelling() {
        let mut p = page(None, SHIPPED);
        p.runner = Runner::new("retroarch", "mgba");
        let n = p.rows().len();
        p.press(Button::Up, A, B);
        assert_eq!(p.sel, n - 1, "wraps to Save changes");
        p.press(A, A, B);
        assert_eq!(p.mode, Mode::Save { sel: 0 });
        p.press(B, A, B);
        assert_eq!(p.mode, Mode::List);
        assert_eq!(p.press(B, A, B), Action::Close);
        // Leaving RetroArch takes its rows away; the selection stays inside the list.
        p.sel = 3;
        p.runner = Runner::new("pocket-core", "mgba");
        p.press(Button::Right, A, B);
        assert!(p.sel < p.rows().len());
    }

    // ---- Android's handheld view (`play_route`, `app_runs_on_android`) ----

    fn app(package: &str) -> EsEmulator {
        EsEmulator { name: package.into(), cores: vec![], default_core: None }
    }

    /// PSP on an Odin: our core to download (18.6 MB), PPSSPP and its gold edition installed.
    fn phone_page(present: bool, channel_open: bool, cfg: &str) -> EmuChoicePage {
        let subject = Subject {
            system: "psp".into(),
            system_name: "PlayStation Portable".into(),
            game: Some(("Lumines".into(), "Lumines.iso".into())),
            emulators: vec![app("org.ppsspp.ppsspp"), app("org.ppsspp.ppssppgold")],
            pocket_core: Some("ppsspp".into()),
            pocket_cores: &[],
            default_override: None,
            file_ok: true,
            fell_back: false,
            phone: Some(PhoneSubject {
                labels: vec![("org.ppsspp.ppsspp".into(), "PPSSPP".into()), ("org.ppsspp.ppssppgold".into(), "PPSSPP Gold".into())],
                own: Some(OwnCore { core: "ppsspp".into(), name: "PSP".into(), size: 18_600_000, present, channel_open }),
            }),
        };
        EmuChoicePage::new(subject, Cfg::parse(cfg), Cfg::default(), |r| !r.is_pocket_core() || present)
    }

    fn press(p: &mut EmuChoicePage, b: Button) -> Action {
        p.press(b, Button::East, Button::South)
    }

    #[test]
    fn phone_every_row_is_reached_with_up_and_down_and_b_closes() {
        let mut p = phone_page(false, true, "");
        assert_eq!(p.rows(), [Row::RunsOn, Row::Download, Row::Save]);
        let mut seen = vec![p.row()];
        for _ in 0..3 {
            press(&mut p, Button::Down);
            seen.push(p.row());
        }
        assert_eq!(seen, [Row::RunsOn, Row::Download, Row::Save, Row::RunsOn], "Down visits every row and wraps");
        press(&mut p, Button::Up);
        assert_eq!(p.row(), Row::Save);
        press(&mut p, Button::Up);
        assert_eq!(p.row(), Row::Download);
        assert_eq!(press(&mut p, Button::South), Action::Close);
    }

    #[test]
    fn phone_the_download_row_downloads_once_and_only_while_the_channel_is_open() {
        let mut p = phone_page(false, true, "");
        press(&mut p, Button::Down);
        assert!(p.download_ready() && p.changes(Row::Download) == false);
        assert_eq!(p.value(Row::Download), "PSP · 18.6 MB");
        // Left/Right do nothing on it; A asks the host, then nothing more while it runs.
        assert_eq!(press(&mut p, Button::Right), Action::Nothing);
        assert_eq!(press(&mut p, Button::East), Action::Download { core: "ppsspp".into() });
        assert!(p.download_busy() && !p.download_ready());
        assert_eq!(press(&mut p, Button::East), Action::Nothing);
        p.download_state(Download::Busy("Downloading · 4.0 MB / 18.6 MB".into()), None);
        assert_eq!(p.value(Row::Download), "Downloading · 4.0 MB / 18.6 MB");
        // Failed: the reason is in the row and A tries again.
        p.download_state(Download::Failed("Downloads wait for unmetered Wi-Fi.".into()), None);
        assert_eq!(p.value(Row::Download), "Downloads wait for unmetered Wi-Fi.");
        assert_eq!(press(&mut p, Button::East), Action::Download { core: "ppsspp".into() });
        // Installed: the row is gone, the selection stays on a row, our player is ready.
        p.download_state(Download::Idle, Some(true));
        assert_eq!(p.rows(), [Row::RunsOn, Row::Save]);
        assert!(p.row() == Row::Save || p.row() == Row::RunsOn);
        assert!(p.note.as_deref().is_some_and(|n| n.starts_with("System installed")));
        // A closed channel: the row says so and A does nothing.
        let mut closed = phone_page(false, false, "");
        press(&mut closed, Button::Down);
        assert_eq!(closed.value(Row::Download), "Not available here");
        assert_eq!((closed.download_ready(), press(&mut closed, Button::East)), (false, Action::Nothing));
    }

    #[test]
    fn phone_the_page_names_the_phones_ways_and_a_chosen_app_takes_the_download_row_away() {
        let p = phone_page(false, true, "");
        assert_eq!(p.value(Row::RunsOn), "Default (pocket-ui player)");
        let words = p.describe(Row::RunsOn).join(" ");
        assert!(words.contains("not installed yet") && !words.contains("ROCKNIX") && !words.contains("Pocket"), "{words}");
        // The console's choice of an app: the game starts there by default, no download, the app's own name.
        let p = phone_page(false, true, "psp.emulator=org.ppsspp.ppssppgold");
        assert_eq!(p.value(Row::RunsOn), "Default (PPSSPP Gold)");
        assert_eq!(p.rows(), [Row::RunsOn, Row::Save]);
        let words = p.describe(Row::RunsOn).join(" ");
        assert!(words.contains("Another app starts the game") && !words.contains("RetroArch"), "{words}");
        // Left/Right step through the ways: the default, then PPSSPP.
        let mut p = phone_page(true, true, "");
        assert_eq!(p.rows(), [Row::RunsOn, Row::Save], "a core on the phone has no Download row");
        press(&mut p, Button::Right);
        assert_eq!(p.value(Row::RunsOn), "PPSSPP");
    }

    #[test]
    fn phone_a_saved_choice_is_a_plain_emulator_key() {
        let mut p = phone_page(true, true, "");
        press(&mut p, Button::Right);
        press(&mut p, Button::Down);
        press(&mut p, Button::East);
        let Action::Save { edits, .. } = press(&mut p, Button::East) else { panic!("saved") };
        assert!(edits.contains(&("psp.emulator".into(), Some("org.ppsspp.ppsspp".into()))), "{edits:?}");
    }
}
