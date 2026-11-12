//! Settings → Licences in the phone layout: pocket-ui's own licence and every emulator core with its licence, where its source
//! is (upstream and commit) and the patches applied to it. The list is `assets/licences/CORES.tsv`, written by the build from
//! the same manifest the source archives come from (`scripts/android/core_licences.py list`), so it names the optional downloads
//! too. When it cannot be read the page says so; it is never an empty sheet.
use super::phone::{MenuRow, Room};
use super::*;

pub const ABOUT: &str = "pocket-ui is free software (GPL version 2 or later). The emulators in it are separate programs with their own licences. Choose one to see its licence, where its source is and what was changed in it.";
pub const OFFER: &str = "The complete source of every emulator, with the patches, is next to the APK on the release page. Ask for it any time: open an issue titled \"Source request\".";
pub const UNREADABLE: &str = "The list of licences could not be read. The texts are in the app's package and next to the APK on the release page.";

/// One row of `CORES.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Core {
    pub id: String,
    pub systems: String,
    pub licence: String,
    pub source: String,
    pub commit: String,
    /// Loaded later over Wi-Fi (true) or inside the APK.
    pub download: bool,
    pub patches: Vec<String>,
}

#[derive(Default)]
pub struct LicencesUi {
    pub cores: Vec<Core>,
    /// The list was read (even if it is empty); false: it could not be.
    pub read: bool,
    /// The core whose details are shown above the rows.
    pub open: Option<usize>,
}

impl LicencesUi {
    /// Reads the build's list: id, systems, licence, upstream, commit, "apk" or "download", patch names.
    pub fn load(&mut self, tsv: &str) {
        self.cores = tsv.lines().filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            (f.len() >= 6 && !f[0].is_empty()).then(|| Core {
                id: f[0].to_string(), systems: f[1].replace(',', ", "), licence: f[2].to_string(), source: f[3].to_string(), commit: f[4].to_string(),
                download: f[5] == "download", patches: f.get(6).map(|p| p.split(',').filter(|p| !p.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
            })
        }).collect();
        self.read = !self.cores.is_empty();
    }
}

impl Core {
    /// What the page says about this core: one line each, already translated.
    pub fn words(&self) -> String {
        let patches = if self.patches.is_empty() { tr("None") } else { self.patches.join(", ") };
        [
            format!("{} · {}", self.id, self.systems),
            format!("{}: {}", tr("Licence"), self.licence),
            format!("{}: {} · {}", tr("Source"), self.source, self.commit.chars().take(12).collect::<String>()),
            format!("{}: {}", tr("Patches"), patches),
            tr(if self.download { "Optional download" } else { "In the app" }),
        ].join("\n")
    }
}

impl Ui {
    pub(super) fn open_licences(&mut self) -> Command {
        self.licences.open = None;
        self.navigate(Screen::Licences);
        Command::None
    }
    pub(super) fn licences_rows(&self) -> Vec<MenuRow> {
        let row = |label: String, value: String| MenuRow { label, value, check: None, stepper: false };
        let mut rows = vec![row("pocket-ui".into(), "GPL-2.0-or-later".into())];
        rows.extend(self.licences.cores.iter().map(|c| row(c.id.clone(), c.licence.clone())));
        rows.push(row(tr("Back"), String::new()));
        rows
    }
    /// The words above the rows: the chosen core's details, else what this page is, else why there is no list.
    pub(super) fn licences_words(&self) -> String {
        match self.licences.open.and_then(|i| self.licences.cores.get(i)) {
            Some(core) => core.words(),
            None if !self.licences.read => format!("{}\n{}", tr(ABOUT), tr(UNREADABLE)),
            None => format!("{}\n{}", tr(ABOUT), tr(OFFER)),
        }
    }
    pub(super) fn licences_press(&mut self, b: Button) -> Command {
        match b {
            Button::Up | Button::Left => self.move_by(-1),
            Button::Down | Button::Right => self.move_by(1),
            Button::South | Button::Start => self.navigate(if b == Button::Start { Screen::Library } else { Screen::Settings }),
            Button::East => {
                let last = self.licences.cores.len() + 1;
                match self.menu {
                    0 => self.licences.open = None,
                    m if m == last => self.navigate(Screen::Settings),
                    m => self.licences.open = Some(m - 1),
                }
            }
            _ => (),
        }
        Command::None
    }
    pub(super) fn paint_licences(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let w = c.w - 2 * room.margin;
        let words = self.licences_words();
        let mut y = room.top + self.d(4.0);
        // The words get the room the rows can spare (`words_limit`).
        let (lines, size, step, most) = self.words_lines(text, &room, &words, w);
        for line in lines.iter().take(most) {
            text.draw(c, Face::Body, size, room.margin, y + size as i32, pal().chalk, line);
            y += step;
        }
        self.cut.extend(lines.iter().skip(most).cloned());
        self.target(Area::new(room.margin, room.top, w, y - room.top), &words.replace('\n', " "), Action::Label);
        let rows = self.licences_rows();
        self.paint_menu_phone(c, text, Room { top: y + self.d(6.0), ..room }, rows);
    }
}
