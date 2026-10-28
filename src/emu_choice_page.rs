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
    pub default_override: Option<(String, String)>,
    /// pocket-core takes the game's file; pocket-core couldn't start it before (`fallback.json`).
    pub file_ok: bool,
    pub fell_back: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    RunsOn,
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
    /// The values on screen.
    pub runner: Runner,
    pub ra: HashMap<&'static str, String>,
    pub sel: usize,
    pub mode: Mode,
    /// Shown after a save ("Saved for this game.").
    pub note: Option<String>,
}

impl EmuChoicePage {
    /// `installed`: whether ROCKNIX can start a pair here (`emu_choice::installed` with the real file system).
    pub fn new(subject: Subject, cfg: Cfg, shipped: Cfg, installed: impl Fn(&Runner) -> bool) -> EmuChoicePage {
        let mut page = EmuChoicePage {
            subject,
            cfg,
            shipped,
            choices: Vec::new(),
            runner: Runner::default(),
            ra: HashMap::new(),
            sel: 0,
            mode: Mode::List,
            note: None,
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
    }

    /// What the game (or the console) starts with now.
    pub fn resolved(&self) -> emu_choice::Resolved {
        emu_choice::resolve(&self.ctx(self.subject.fell_back), &self.cfg, self.file())
    }

    /// The rows top to bottom: the RetroArch settings only while RetroArch is chosen.
    pub fn rows(&self) -> Vec<Row> {
        let mut v = vec![Row::RunsOn];
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
            Row::Ra(i) => RA_SETTINGS[i].name,
            Row::Save => "Save changes",
        }
    }

    pub fn runner_label(&self, r: &Runner) -> String {
        emu_choice::label(r, &self.subject.emulators)
    }

    pub fn value(&self, row: Row) -> String {
        match row {
            Row::RunsOn => self.runner_label(&self.runner),
            Row::Ra(i) => {
                let s = &RA_SETTINGS[i];
                s.value_label(self.ra.get(s.key).map_or(s.unset, String::as_str))
            }
            Row::Save => ">".to_string(),
        }
    }

    /// The selected row's explanation, and what its value means.
    pub fn describe(&self, row: Row) -> [String; 2] {
        match row {
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

    /// The runners Left/Right step through: the installed ones, and the one in use.
    fn runners(&self) -> Vec<Runner> {
        self.choices.iter().filter(|(r, ok)| *ok || *r == self.runner).map(|(r, _)| r.clone()).collect()
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
            Row::RunsOn => {
                let list = self.runners();
                let n = list.len() as i32;
                if n > 0 {
                    let i = list.iter().position(|r| *r == self.runner).map_or(0, |i| i as i32);
                    self.runner = list[(i + delta).rem_euclid(n) as usize].clone();
                }
            }
            Row::Ra(i) => {
                let key = RA_SETTINGS[i].key;
                let now = self.ra.get(key).cloned().unwrap_or_default();
                if let Some(v) = walk(&self.ra_values(i), &now) {
                    self.ra.insert(key, v);
                }
            }
            Row::Save => {}
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
                if self.runner == auto {
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
            default_override: None,
            file_ok: true,
            fell_back: false,
        }
    }

    const SHIPPED: &str = "gba.ratio=3/2\nglobal.ratio=core\nglobal.rewind=0\nglobal.runahead=0\nglobal.autosave=1\n";

    fn page(game: Option<&str>, cfg: &str) -> EmuChoicePage {
        EmuChoicePage::new(subject(game), Cfg::parse(cfg), Cfg::parse(SHIPPED), |r| r.core != "gpsp")
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
        assert_eq!(p.value(Row::RunsOn), "pocket-core · mGBA");
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
        assert_eq!(rows.len(), 2 + RA_SETTINGS.len());
        assert_eq!(p.value(Row::Ra(0)), "3:2", "gba.ratio from the shipped file");
        assert_eq!(p.value(Row::Ra(6)), "On", "global.autosave=1");
        assert!(p.unsaved());
    }

    #[test]
    fn an_es_choice_is_shown_and_kept() {
        // Chosen in EmulationStation, gpSP isn't installed: still shown, marked, and kept unless changed.
        let p = page(
            Some("Zelda.gba"),
            &format!("{SHIPPED}gba.emulator=retroarch\ngba.core=gpsp\ngba.shaderset=crt/crt-royale.slangp\n"),
        );
        assert_eq!(p.value(Row::RunsOn), "RetroArch · gpSP");
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
        assert_eq!(p.value(Row::RunsOn), "pocket-core · mGBA");
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
        assert_eq!(p.value(Row::RunsOn), "pocket-core · mGBA");
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
}
