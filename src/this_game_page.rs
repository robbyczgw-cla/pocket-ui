//! The "This game" page: everything that can be set for one game, reachable from its card (START) without starting it.
//! Picture (scaling, sharpness, effect, screen frame and its sub-choices, orientation), over the game, the extras
//! (Performance, Fan, Run-ahead, ...), Controls, "Runs on", and the standalone emulators' own rows (Xbox resolution,
//! PS2/GameCube/Wii/3DS).
//!
//! There is one source of truth: the rows are the pause menu's own. The settings are `options::GROUPS` read and written
//! through `options::GameSettings` (`game-settings/<system>/<game>.json`, the file pocket-core reads at the start), Controls is
//! `emu_menu::EmuMenu` (`ctl.*` in the same file), the standalone rows are `emu_settings::Menu` (system.cfg, the game's INI).
//! A change is written at once as this game's own settings ("Save changes > For this game" of the pause menu), so the
//! game starts with it and the pause menu shows it. Nothing here touches the running game: there is none.
//!
//! Painting is in the binary (`app_this_game.rs`); the page is plain state, so every path is a test.

use std::path::{Path, PathBuf};

use crate::emu_menu::EmuMenu;
use crate::emu_settings::{Files, Menu};
use crate::options::{self, GameSettings, Level, Setting};
use crate::pad::Button;
use crate::standalone::Emu;

/// What the page is about, gathered by the App when it opens.
pub struct Subject {
    /// ROCKNIX's system id ("gba") and its full name.
    pub system: String,
    pub system_name: String,
    pub name: String,
    pub rom: PathBuf,
    /// pocket-core hosts the system: the picture, extras and controls below apply to it.
    pub hosted: bool,
    /// What plays the game now ("mGBA in RetroArch"), and whether that is pocket-core.
    pub runs_on: String,
    pub on_pocket_core: bool,
    /// The standalone emulator that plays it (PS2, GameCube, Wii, 3DS, Xbox): its rows come from `emu_settings`.
    pub emu: Option<Emu>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// An entry of `options::GROUPS`, by key.
    Setting(&'static str),
    /// Row `n` of the standalone emulator's table.
    Standalone(usize),
    Controls,
    RunsOn,
    /// Takes the game off its own settings.
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    List,
    /// The Controls rows, on this one.
    Controls(usize),
}

pub enum Action {
    Nothing,
    Close,
    /// A on "Runs on": the App opens that page over this one.
    RunsOn,
}

/// The pause menu's groups listed, in this order (each whole: an empty list means every setting of the group).
const GROUPS: [(&str, &[&str]); 3] = [("Picture", &[]), ("Extras", &[]), ("Shortcuts", &[])];

/// What a Picture row says besides its own text: there is no running game to show it on.
pub const PICTURE_HINT: &str = "You see it when the game starts. In the pause menu, Options > Picture shows it live.";

pub struct ThisGamePage {
    pub subject: Subject,
    data_dir: PathBuf,
    files: Files,
    settings: Option<GameSettings>,
    controls: Option<EmuMenu>,
    standalone: Option<Menu>,
    pub sel: usize,
    pub mode: Mode,
    /// A line under the list: a change that could not be kept, or what a change does.
    pub note: Option<String>,
    /// Counts changes, so the App repaints and the sounds notice them.
    rev: u32,
}

impl ThisGamePage {
    pub fn new(subject: Subject, data_dir: &Path, files: Files) -> ThisGamePage {
        let settings = subject.hosted.then(|| GameSettings::load(data_dir, &subject.system, &subject.rom));
        let controls = settings.as_ref().map(|s| EmuMenu::offline(&subject.system, s));
        let standalone = subject.emu.and_then(|e| Menu::for_game(e, &subject.system, &subject.rom, None, &files));
        ThisGamePage { subject, data_dir: data_dir.to_path_buf(), files, settings, controls, standalone, sel: 0, mode: Mode::List, note: None, rev: 0 }
    }

    /// The rows of the list: the settings of the groups, Controls, Runs on, the standalone emulator's, Reset.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(s) = self.settings.as_ref() {
            for (name, only) in GROUPS {
                let Some(g) = options::GROUPS.iter().find(|g| g.name == name) else { continue };
                let visible = g.visible(Some(s));
                if only.is_empty() {
                    rows.extend(visible.into_iter().map(|x| Row::Setting(x.key)));
                } else {
                    rows.extend(only.iter().filter_map(|k| visible.iter().find(|x| x.key == *k)).map(|x| Row::Setting(x.key)));
                }
            }
            rows.push(Row::Controls);
        }
        rows.push(Row::RunsOn);
        if let Some(m) = self.standalone.as_ref() {
            rows.extend((0..m.len()).filter(|i| !m.is_more(*i) && !m.is_advanced(*i)).map(Row::Standalone));
        }
        if self.settings.as_ref().is_some_and(|s| s.level == Level::Game) {
            rows.push(Row::Reset);
        }
        rows
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    /// (name, value, explanation) of a row; the value is ">" for one that opens a page, empty for an action.
    pub fn describe(&self, row: Row) -> (String, String, String) {
        match row {
            Row::Setting(key) => {
                let (Some(setting), Some(s)) = (options::setting(key), self.settings.as_ref()) else { return Default::default() };
                // Rewind and Over the game are in Extras and Shortcuts both: the shortcut says so.
                let shortcut = options::GROUPS.iter().any(|g| g.name == "Shortcuts" && g.settings.iter().any(|x| x.key == key));
                let name = if shortcut { format!("Shortcut: {}", setting.name) } else { setting.name.to_string() };
                (name, setting.choices[s.index(setting)].label.to_string(), setting.desc.to_string())
            }
            Row::Standalone(i) => self.standalone.as_ref().and_then(|m| m.rows().into_iter().nth(i)).unwrap_or_default(),
            Row::Controls => ("Controls".into(), ">".into(), "Which button does what, and the left stick. The pause menu shows this console's own button names.".into()),
            Row::RunsOn => ("Runs on".into(), if self.subject.runs_on.is_empty() { "Default".into() } else { self.subject.runs_on.clone() }, "Which emulator plays this game or this console: pocket-core, RetroArch or another.".into()),
            Row::Reset => ("Use the console's settings".into(), String::new(), "Takes this game off its own settings: it starts like the other games on this console.".into()),
        }
    }

    /// Rows of the Controls page.
    pub fn control_rows(&self) -> Vec<crate::emu_menu::Row> {
        self.controls.as_ref().map(|c| c.control_rows()).unwrap_or_default()
    }

    /// Whether the selected row is a picture setting, which the page can't show on a running game (`PICTURE_HINT`).
    pub fn picture_selected(&self) -> bool {
        matches!(self.mode, Mode::List) && matches!(self.row(), Row::Setting(k) if options::GROUPS.iter().any(|g| g.name == "Picture" && g.settings.iter().any(|x| x.key == k)))
    }

    /// Where the settings in use come from.
    pub fn level_line(&self) -> &'static str {
        match self.settings.as_ref().map(|s| s.level) {
            Some(Level::Game) => "This game has its own settings.",
            Some(Level::Console) => "Using this console's settings. Changing a row gives this game its own.",
            Some(Level::Defaults) => "Using the defaults. Changing a row gives this game its own settings.",
            None => "",
        }
    }

    /// The lines under the list that belong to the page, not to a row: what plays the game when that is not what the
    /// picture settings are for.
    pub fn footer(&self) -> Option<&'static str> {
        if self.subject.hosted && !self.subject.on_pocket_core {
            Some("This game plays on another emulator now. These settings apply when it runs on pocket-core.")
        } else if !self.subject.hosted && self.standalone.is_none() {
            Some("This emulator has no picture settings here. Its own menu has them.")
        } else {
            None
        }
    }

    /// Sets a setting in memory only (`--snapshot`), to show a frame's sub-choices without a file.
    #[doc(hidden)]
    pub fn demo_set(&mut self, key: &str, value: &str) {
        if let Some(s) = self.settings.as_mut() {
            s.values.insert(key.to_string(), value.to_string());
        }
    }

    pub fn mark(&self) -> (usize, Mode, u32) {
        (self.sel, self.mode, self.rev)
    }

    /// Whether the page is still on a game that has its own settings after Reset (for tests and the App).
    pub fn level(&self) -> Option<Level> {
        self.settings.as_ref().map(|s| s.level)
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        match self.mode {
            Mode::List => self.press_list(b, confirm, cancel),
            Mode::Controls(sel) => {
                self.press_controls(sel, b, confirm, cancel);
                Action::Nothing
            }
        }
    }

    fn press_list(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.rows().len();
        let step = |b: Button| match b {
            Button::Left => Some(-1),
            Button::Right => Some(1),
            _ => None,
        };
        if b == cancel {
            return Action::Close;
        }
        match b {
            Button::Up => self.sel = (self.sel + n - 1) % n,
            Button::Down => self.sel = (self.sel + 1) % n,
            // Past the Picture rows to the Extras, and back.
            Button::L1 => self.sel = self.sel.saturating_sub(5),
            Button::R1 => self.sel = (self.sel + 5).min(n - 1),
            _ if b == confirm || step(b).is_some() => {
                let delta = step(b).unwrap_or(1);
                return self.activate(delta, b == confirm);
            }
            _ => {}
        }
        Action::Nothing
    }

    /// A or Left/Right on the selected row. `ok`: it was A, which opens a page where Left/Right do nothing.
    fn activate(&mut self, delta: i32, ok: bool) -> Action {
        self.note = None;
        match self.row() {
            Row::Setting(key) => {
                if let (Some(setting), Some(s)) = (options::setting(key), self.settings.as_mut()) {
                    s.step(setting, delta);
                    self.keep();
                }
            }
            Row::Standalone(i) => {
                if let Some(m) = self.standalone.as_mut() {
                    if m.step(i, delta) {
                        // The game is not running: only the file changes, whatever the plan says about applying it.
                        if let Err(e) = m.commit_to(&self.files) {
                            self.note = Some(e);
                        }
                        self.rev += 1;
                    } else {
                        self.note = m.rows().into_iter().nth(i).map(|r| r.2);
                    }
                }
            }
            Row::Controls if ok => self.mode = Mode::Controls(0),
            Row::Controls => {}
            Row::RunsOn if ok => return Action::RunsOn,
            Row::RunsOn => {}
            Row::Reset if ok => {
                if let Some(s) = self.settings.as_mut() {
                    s.forget_game(&self.data_dir, &self.subject.rom);
                }
                self.controls = self.settings.as_ref().map(|s| EmuMenu::offline(&self.subject.system, s));
                self.rev += 1;
                self.note = Some("This game starts like the others on this console now.".into());
                self.sel = self.sel.min(self.rows().len() - 1);
            }
            Row::Reset => {}
        }
        Action::Nothing
    }

    /// Writes the settings as this game's own, where pocket-core reads them at the start.
    fn keep(&mut self) {
        let Some(s) = self.settings.as_mut() else { return };
        if !s.save_for_game() {
            self.note = Some("Couldn't save. Is the card full or read-only?".into());
        }
        self.rev += 1;
        // A row may have come or gone with the change (the frame's colour); the selection stays on a row.
        self.sel = self.sel.min(self.rows().len() - 1);
    }

    fn press_controls(&mut self, sel: usize, b: Button, confirm: Button, cancel: Button) {
        let n = self.controls.as_ref().map_or(1, |c| c.control_count());
        if b == cancel {
            self.mode = Mode::List;
            self.sel = self.rows().iter().position(|r| *r == Row::Controls).unwrap_or(0);
            return;
        }
        let delta = match b {
            Button::Left => -1,
            _ => 1,
        };
        match b {
            Button::Up => self.mode = Mode::Controls((sel + n - 1) % n),
            Button::Down => self.mode = Mode::Controls((sel + 1) % n),
            Button::Left | Button::Right => self.step_control(sel, delta),
            _ if b == confirm => self.step_control(sel, 1),
            _ => {}
        }
    }

    fn step_control(&mut self, sel: usize, delta: i32) {
        if let (Some(c), Some(s)) = (self.controls.as_mut(), self.settings.as_mut()) {
            c.control_step(sel, delta, s);
            self.keep();
        }
    }
}

/// The setting behind a row, for the paint code's choices (None for rows that are no `options` setting).
pub fn setting_of(row: Row) -> Option<&'static Setting> {
    match row {
        Row::Setting(key) => options::setting(key),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    fn subject(system: &str, hosted: bool, on_core: bool) -> Subject {
        Subject {
            system: system.into(),
            system_name: "Game Boy Advance".into(),
            name: "Advance Wars".into(),
            rom: PathBuf::from(format!("/roms/{system}/Advance Wars.gba")),
            hosted,
            runs_on: if on_core { "mGBA on pocket-core".into() } else { "mGBA in RetroArch".into() },
            on_pocket_core: on_core,
            emu: None,
        }
    }

    fn page(dir: &Path, system: &str) -> ThisGamePage {
        ThisGamePage::new(subject(system, true, true), dir, Files { system_cfg: dir.join("system.cfg"), root: dir.to_path_buf() })
    }

    fn select(p: &mut ThisGamePage, key: &'static str) {
        p.sel = p.rows().iter().position(|r| *r == Row::Setting(key)).unwrap_or_else(|| panic!("no row {key}"));
    }

    #[test]
    fn lists_the_pause_menus_picture_rows_first_then_extras_controls_and_runs_on() {
        let dir = TestDir::new("tg-rows");
        let p = page(&dir, "gba");
        let rows = p.rows();
        let names: Vec<String> = rows.iter().map(|r| p.describe(*r).0).collect();
        assert_eq!(&names[..3], ["Screen scaling", "Screen sharpness", "Screen effect"]);
        for want in ["Screen frame", "Screen orientation", "Over the game", "Performance", "Fan", "Run-ahead", "Fast-forward speed", "Rewind", "Earlier states", "Rumble", "Shortcut: Save state", "Shortcut: Rewind", "Controls", "Runs on"] {
            assert!(names.iter().any(|n| n == want), "{want} is missing: {names:?}");
        }
        assert_eq!(rows.last(), Some(&Row::RunsOn), "no Reset before the game has settings of its own");
    }

    #[test]
    fn a_change_is_this_games_file_and_the_pause_menu_reads_it_back() {
        let dir = TestDir::new("tg-sync");
        let mut p = page(&dir, "gba");
        select(&mut p, "frame");
        let frame = options::setting("frame").unwrap();
        let before = p.settings.as_ref().unwrap().index(frame);
        p.press(Button::Right, CONFIRM, CANCEL);
        let want = frame.choices[(before + 1) % frame.choices.len()].value;
        // What pocket-core and the pause menu load: the same file, the same value.
        let loaded = GameSettings::load(&dir, "gba", &p.subject.rom);
        assert_eq!(loaded.level, Level::Game);
        assert_eq!(loaded.effective()["frame"], want);
        assert_eq!(p.level_line(), "This game has its own settings.");
        // ... and the other way: a value saved by the pause menu shows on the page.
        let mut menu = GameSettings::load(&dir, "gba", &p.subject.rom);
        menu.step(options::setting("scaling").unwrap(), 1);
        assert!(menu.save_for_game());
        let again = page(&dir, "gba");
        assert_eq!(again.describe(Row::Setting("scaling")).1, options::setting("scaling").unwrap().choices[1].label);
    }

    #[test]
    fn the_frames_sub_choice_comes_and_goes_with_the_frame() {
        let dir = TestDir::new("tg-sub");
        let mut p = page(&dir, "gba");
        select(&mut p, "frame");
        let frame = options::setting("frame").unwrap();
        let shell = frame.choices.iter().position(|c| c.value == "shell").unwrap();
        while p.settings.as_ref().unwrap().index(frame) != shell {
            p.press(Button::Right, CONFIRM, CANCEL);
        }
        assert!(p.rows().contains(&Row::Setting("frame_colour")), "Shell offers its colour");
        p.press(Button::Right, CONFIRM, CANCEL);
        assert!(!p.rows().contains(&Row::Setting("frame_colour")));
    }

    #[test]
    fn controls_are_kept_as_ctl_values_and_reset_goes_back_to_the_console() {
        let dir = TestDir::new("tg-controls");
        let mut p = page(&dir, "gba");
        p.sel = p.rows().iter().position(|r| *r == Row::Controls).unwrap();
        p.press(CONFIRM, CONFIRM, CANCEL);
        assert_eq!(p.mode, Mode::Controls(0));
        assert_eq!(p.control_rows()[0].name, "Left stick as D-pad");
        // Row 1 is A: one to the right gives it another button.
        p.press(Button::Down, CONFIRM, CANCEL);
        p.press(Button::Right, CONFIRM, CANCEL);
        let saved = GameSettings::load(&dir, "gba", &p.subject.rom);
        assert!(saved.values.keys().any(|k| k.starts_with("ctl.")), "{:?}", saved.values);
        p.press(CANCEL, CONFIRM, CANCEL);
        assert_eq!(p.row(), Row::Controls, "B goes back to the Controls row");
        // Reset: the file goes, the page reads the console's (here the defaults) again.
        p.sel = p.rows().iter().position(|r| *r == Row::Reset).unwrap();
        p.press(CONFIRM, CONFIRM, CANCEL);
        assert_eq!(GameSettings::load(&dir, "gba", &p.subject.rom).level, Level::Defaults);
        assert!(!p.rows().contains(&Row::Reset));
        assert!(p.note.is_some(), "it says what happened");
    }

    #[test]
    fn runs_on_opens_the_emulator_page_and_b_closes() {
        let dir = TestDir::new("tg-runs");
        let mut p = page(&dir, "gba");
        p.sel = p.rows().len() - 1;
        assert_eq!(p.row(), Row::RunsOn);
        assert!(matches!(p.press(CONFIRM, CONFIRM, CANCEL), Action::RunsOn));
        assert!(matches!(p.press(Button::Left, CONFIRM, CANCEL), Action::Nothing), "Left on a page row does nothing");
        assert!(matches!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close));
    }

    #[test]
    fn a_system_pocket_core_does_not_host_shows_runs_on_and_says_why_there_is_no_picture() {
        let dir = TestDir::new("tg-unhosted");
        let p = ThisGamePage::new(subject("psx", false, false), &dir, Files::default());
        assert_eq!(p.rows(), [Row::RunsOn], "never an empty page");
        assert!(p.footer().unwrap().contains("no picture settings"));
        let other = ThisGamePage::new(subject("gba", true, false), &dir, Files::default());
        assert!(other.footer().unwrap().contains("another emulator"), "hosted, but RetroArch plays it now");
    }

    #[test]
    fn xbox_resolution_is_the_emulator_tables_row_and_lands_in_system_cfg() {
        let dir = TestDir::new("tg-xbox");
        let mut s = subject("xbox", false, false);
        s.rom = PathBuf::from("/roms/xbox/Halo.iso");
        s.emu = Some(Emu::Hakux);
        let mut p = ThisGamePage::new(s, &dir, Files { system_cfg: dir.join("system.cfg"), root: dir.to_path_buf() });
        let res = p.rows().into_iter().find(|r| matches!(r, Row::Standalone(_)) && p.describe(*r).0 == "Resolution").expect("Xbox has Resolution");
        let before = p.describe(res).1;
        p.sel = p.rows().iter().position(|r| *r == res).unwrap();
        p.press(Button::Right, CONFIRM, CANCEL);
        assert_ne!(p.describe(res).1, before);
        let cfg = std::fs::read_to_string(dir.join("system.cfg")).unwrap();
        assert!(cfg.contains("xbox[\"Halo.iso\"]"), "{cfg}");
    }
}
