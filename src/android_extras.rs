//! Settings → System → Android extras (Android only; docs/ANDROID-EXTRAS.md, docs/user/android-extras.md): what pocket-ui
//! can do beyond a normal app once the player gives it more. Standard needs none of it; each step is the player's choice.
//!
//! - **ADB grant**: Android's WRITE_SECURE_SETTINGS, given once from a computer, or with A on its row through Shizuku or root
//!   (which also take it back there; nothing is granted behind the player's back, Sol's review 26.11.2026). The switches of
//!   this page: no pop-up notifications, no animations, the screen on while charging, the bars hidden everywhere on Android
//!   10 and older. The pause menu's service is turned on on Android's own accessibility page.
//! - **Shizuku**: Android's shell, for the pause menu over other emulator apps (their save and load keys pressed) and for
//!   the whole system log in a bug report.
//! - **Root** (Magisk): what Shizuku does, and the scaffold of what only root may ([`root`]): the scripts for processor and
//!   graphics clocks per game, a fan where the kernel shows one, and the RG556's next boot slot. Built and tested here,
//!   not offered on the page yet: they need a device to prove them.
//!
//! The Java side is `Extras.java` (status and presses), `ShizukuLink.java`, `ExtrasProvider.java` and `PadWatch.java` (the
//! menu over other apps). This module is the page's state and words; nothing here touches the system, so it is plain tests.

use crate::pad::Button;

/// The Settings row and the page's title.
pub const SETTINGS_NAME: &str = "Android extras";
/// Tests and snapshots: the status JSON `Extras.status` would give (`--snapshot android-extras`).
pub const STATUS_ENV: &str = "POCKETUI_ANDROID_EXTRAS";
/// Shizuku's own download page, as the page's QR code and its A (the store first, when there is one).
pub const SHIZUKU_PAGE: &str = "https://shizuku.rikka.app/download/";
/// The newest Android that still has `policy_control` (hidden bars in every app): Android 10.
pub const BARS_LAST_SDK: u32 = 29;

/// What `Extras.status` says, read from its JSON. A field it lacks is the plain app's (nothing given).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub sdk: u32,
    /// The app's package (a QA package's own), for the ADB line.
    pub package: String,
    pub model: String,
    /// WRITE_SECURE_SETTINGS is granted.
    pub secure: bool,
    /// "missing", "stopped", "ask" or "ready".
    pub shizuku: String,
    /// "yes", "no", or "" before root was checked.
    pub root: String,
    /// The pause menu's accessibility service is on.
    pub watch: bool,
    pub heads_up_off: Switch,
    pub animations_off: Switch,
    pub stay_awake: Switch,
    pub bars_hidden: Switch,
}

/// A switch of Android's settings as Android has it now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Switch {
    On,
    #[default]
    Off,
    /// This Android has no such setting (the bars on Android 11 and newer).
    Missing,
}

impl Switch {
    fn parse(v: Option<&serde_json::Value>) -> Switch {
        match v.and_then(|v| v.as_str()) {
            Some("on") => Switch::On,
            Some("off") => Switch::Off,
            _ => Switch::Missing,
        }
    }
}

/// How far the player went, the page's first row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Standard,
    AdbGrant,
    Shizuku,
    Root,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Standard => "Standard",
            Level::AdbGrant => "ADB grant",
            Level::Shizuku => "Shizuku",
            Level::Root => "Root",
        }
    }
}

impl Status {
    /// `Extras.status`'s JSON; anything unreadable is the plain app.
    pub fn parse(json: &str) -> Status {
        let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let sw = v.get("switches");
        let switch = |k: &str| Switch::parse(sw.and_then(|s| s.get(k)));
        Status {
            sdk: v.get("sdk").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            package: s("package"),
            model: s("model"),
            secure: v.get("secure").and_then(|x| x.as_bool()).unwrap_or(false),
            shizuku: s("shizuku"),
            root: s("root"),
            watch: v.get("watch").and_then(|x| x.as_bool()).unwrap_or(false),
            heads_up_off: switch("heads_up"),
            animations_off: switch("animations"),
            stay_awake: switch("awake"),
            bars_hidden: switch("bars"),
        }
    }

    pub fn level(&self) -> Level {
        if self.root == "yes" {
            Level::Root
        } else if self.shizuku == "ready" {
            Level::Shizuku
        } else if self.secure {
            Level::AdbGrant
        } else {
            Level::Standard
        }
    }

    /// Shizuku or root: Android's shell for pocket-ui (the ADB grant with one A, the whole log).
    pub fn can_press(&self) -> bool {
        self.level() >= Level::Shizuku
    }

    /// The pause menu's Save, Load and Teach rows: Shizuku or root, and Android 13 or newer (`TargetKey.java`,
    /// `PadWatch.canSave`). On 14+ Android binds the key press to the emulator's uid (`injectInputEventToTarget`); Android 13
    /// falls back to "all windows" on a mismatch for the shell and root (RG556 27.11.2026, AOSP 13 InputManagerService), so
    /// there TargetKey checks the input dispatcher's focus right before and after the down (ANDROID-EXTRAS.md: the rest risk).
    /// Older Android has no target uid at all: no rows.
    pub fn can_save(&self) -> bool {
        self.can_press() && self.sdk >= TARGETED_SDK
    }

    /// The app's package, or the release's when Android didn't say.
    pub fn package(&self) -> &str {
        if self.package.is_empty() { "xyz.pocketui.app" } else { &self.package }
    }
}

/// Android 13 (API 33): the first with `injectInputEventToTarget` (bound on 14+, focus-checked on 13; `TargetKey.SDK`).
pub const TARGETED_SDK: u32 = 33;

/// The one line to type on a computer for the ADB grant (shown as it is, never translated).
pub fn adb_line(package: &str) -> String {
    format!("adb shell pm grant {package} android.permission.WRITE_SECURE_SETTINGS")
}

/// A row of the page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Level,
    Shizuku,
    AdbGrant,
    Root,
    PauseMenu,
    NoPopups,
    NoAnimations,
    HideBars,
    StayAwake,
}

/// The rows for this Android: Hide system bars only where Android still has the setting (10 and older).
pub fn rows(status: &Status) -> Vec<Row> {
    let mut rows = vec![Row::Level, Row::Shizuku, Row::AdbGrant, Row::Root, Row::PauseMenu, Row::NoPopups, Row::NoAnimations];
    if status.bars_hidden != Switch::Missing && status.sdk <= BARS_LAST_SDK {
        rows.push(Row::HideBars);
    }
    rows.push(Row::StayAwake);
    rows
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Close,
    /// Read the status again.
    Refresh,
    /// `PocketBase.extrasAction(what, arg)` on a thread, then the status again.
    Bridge(&'static str, String),
    /// Not possible yet: the page's line says what is needed first.
    Refuse,
}

pub struct Page {
    pub sel: usize,
    /// None until the first answer.
    pub status: Option<Status>,
    /// A question to Android is under way: A waits.
    pub busy: bool,
}

impl Page {
    pub fn new(status: Option<Status>) -> Page {
        Page { sel: 0, status, busy: false }
    }

    fn st(&self) -> Status {
        self.status.clone().unwrap_or_default()
    }

    pub fn rows(&self) -> Vec<Row> {
        rows(&self.st())
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    pub fn label(row: Row) -> &'static str {
        match row {
            Row::Level => "Level",
            Row::Shizuku => "Shizuku",
            Row::AdbGrant => "ADB grant",
            Row::Root => "Root (Magisk)",
            Row::PauseMenu => "Pause menu in other emulators",
            Row::NoPopups => "No pop-up notifications",
            Row::NoAnimations => "No animations",
            Row::HideBars => "Hide system bars",
            Row::StayAwake => "Stay awake while charging",
        }
    }

    pub fn value(&self, row: Row) -> &'static str {
        let s = self.st();
        if self.status.is_none() {
            return "…";
        }
        let switch = |sw: Switch| match (s.secure, sw) {
            (false, _) => "Needs the grant",
            (true, Switch::On) => "On",
            (true, _) => "Off",
        };
        match row {
            Row::Level => s.level().label(),
            Row::Shizuku => match s.shizuku.as_str() {
                "ready" => "Ready",
                "ask" => "Allow pocket-ui",
                "stopped" => "Not running",
                _ => "Not installed",
            },
            Row::AdbGrant => if s.secure { "Granted" } else { "Not granted" },
            Row::Root => match s.root.as_str() {
                "yes" => "Ready",
                "no" => "Not found",
                _ => "Not checked",
            },
            Row::PauseMenu => if s.watch { "On" } else { "Off" },
            Row::NoPopups => switch(s.heads_up_off),
            Row::NoAnimations => switch(s.animations_off),
            Row::HideBars => switch(s.bars_hidden),
            Row::StayAwake => switch(s.stay_awake),
        }
    }

    /// What A does on the row now, for the hint bar; None: nothing.
    pub fn a_hint(&self) -> Option<&'static str> {
        let s = self.st();
        self.status.as_ref()?;
        Some(match self.row() {
            Row::Level => "Check again",
            Row::Shizuku => match s.shizuku.as_str() {
                "ready" => return None,
                "ask" => "Allow",
                "stopped" => "Open Shizuku",
                _ => "Get Shizuku",
            },
            Row::AdbGrant => match (s.secure, s.can_press()) {
                (false, true) => "Grant",
                (true, true) => "Take back",
                _ => "Check again",
            },
            Row::Root => if s.root == "yes" { return None } else { "Check root" },
            Row::PauseMenu => "Accessibility page",
            Row::NoPopups | Row::NoAnimations | Row::HideBars | Row::StayAwake => {
                if !s.secure {
                    return None;
                }
                "Change"
            }
        })
    }

    /// The selected row's lines under the list: what it is for, then what to do now (steps where there are steps).
    pub fn describe(&self) -> Vec<&'static str> {
        let s = self.st();
        match self.row() {
            Row::Level => vec![
                "Standard: pocket-ui works as a normal app and needs nothing here.",
                match s.level() {
                    Level::Standard => "Each extra is your choice: the ADB grant, Shizuku or root. They are off until you set them up.",
                    Level::AdbGrant => "ADB grant: pocket-ui may change Android's settings below.",
                    Level::Shizuku if s.can_save() => "Shizuku: keys pressed in other emulators for save and load, and the ADB grant with one A.",
                    Level::Shizuku => "Shizuku: the ADB grant with one A, and the whole system log in a bug report.",
                    Level::Root => "Root: everything Shizuku gives, through Magisk.",
                },
            ],
            Row::Shizuku => match s.shizuku.as_str() {
                "ready" => {
                    // The grant's state is told whatever else Shizuku gives here (Sol's recheck of 38cfee80f: on Android 13+
                    // the Save/Load line hid it, and the line inviting A would take the grant back).
                    let mut v = vec![if s.can_save() {
                        "Shizuku is ready. pocket-ui presses save and load in other emulators and adds the whole system log to a bug report."
                    } else if s.secure {
                        "Shizuku is ready. ADB access is already granted. A bug report includes the whole system log."
                    } else {
                        "Shizuku is ready: A on ADB grant gives the grant, and a bug report gets the whole system log."
                    }];
                    if s.can_save() && s.secure {
                        v.push("ADB access is already granted.");
                    }
                    v.push("After a restart of the device, start Shizuku again in its app (with root it starts by itself).");
                    v
                }
                "ask" => vec!["Shizuku is running. Press A and allow pocket-ui in Shizuku's question."],
                "stopped" => vec![
                    "Shizuku is installed but not running. Press A to open it, then start it there.",
                    "Without a computer: Android's Developer options › Wireless debugging on, then Pairing in Shizuku.",
                ],
                _ => vec![
                    "1. Get Shizuku: press A, or scan the code with your phone.",
                    "2. Android's Developer options: turn on Wireless debugging.",
                    "3. In Shizuku: Pairing with the code, then Start.",
                    "4. Back here, press A and allow pocket-ui.",
                ],
            },
            Row::AdbGrant => match (s.secure, s.can_press()) {
                (true, true) => vec!["Granted. A takes it back; the switches below go back to Android's values first."],
                (true, false) => vec!["Granted. It stays until you take it back (adb shell pm revoke) or uninstall pocket-ui."],
                (false, true) => vec!["Shizuku or root can give it: press A. You can take it back here at any time."],
                (false, false) => vec![
                    "Once, from a computer with adb: USB debugging on, the cable in, then type this line:",
                    "Then press A here. It stays until you take it back or uninstall pocket-ui. With Shizuku or root, A gives it here.",
                ],
            },
            Row::Root => match s.root.as_str() {
                "yes" => vec!["Root is ready: pocket-ui uses Magisk for what Shizuku would do."],
                "no" => vec!["Magisk said no, or this device has no root. Press A to ask again."],
                _ => vec!["Only with Magisk. A asks Magisk once; allow pocket-ui there."],
            },
            Row::PauseMenu => {
                let mut v = vec!["HOME, or SELECT + START, opens pocket-ui's pause menu over games that pocket-ui started in other emulator apps."];
                v.push(if s.can_save() {
                    "With Shizuku or root it saves and loads too: teach the emulator the keys once, from the menu's Teach rows."
                } else if s.can_press() {
                    // Android 12 and older: no key press for one app, so no Save or Load (`can_save`).
                    "It brings back the shelf and opens the emulator's own menu; save and quit there."
                } else {
                    "Without Shizuku or root it brings back the shelf and opens the emulator's own menu."
                });
                v.push(if s.watch {
                    "A opens Android's accessibility page, where you can turn it off."
                } else {
                    "A opens Android's accessibility page: turn on pocket-ui pause menu there. Android 13 and newer may first ask you to allow restricted settings in pocket-ui's app info."
                });
                v
            }
            Row::NoPopups => switch_lines(&s, "Pop-up notifications stay away, all over Android. Off puts Android's setting back."),
            Row::NoAnimations => switch_lines(&s, "Android's window and transition animations off, all over Android: apps open at once."),
            Row::HideBars => switch_lines(&s, "Android's status and navigation bars stay hidden in every app (Android 10 and older)."),
            Row::StayAwake => switch_lines(&s, "The screen stays on while the device charges, as Android's developer option."),
        }
    }

    /// The selected row wants the QR code beside its lines: Shizuku's page, until Shizuku is installed.
    pub fn qr(&self) -> Option<&'static str> {
        (self.row() == Row::Shizuku && self.status.as_ref().is_some_and(|s| s.shizuku.is_empty() || s.shizuku == "missing")).then_some(SHIZUKU_PAGE)
    }

    /// The selected row shows the ADB line (in a panel, as it is).
    pub fn adb(&self) -> Option<String> {
        let s = self.status.as_ref()?;
        // With Shizuku or root, A gives it: no line to type then.
        (self.row() == Row::AdbGrant && !s.secure && !s.can_press()).then(|| adb_line(s.package()))
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.rows().len();
        match b {
            Button::Up => {
                self.sel = (self.sel + n - 1) % n;
                return Action::Nothing;
            }
            Button::Down => {
                self.sel = (self.sel + 1) % n;
                return Action::Nothing;
            }
            b if b == cancel => return Action::Close,
            b if b == confirm => {}
            _ => return Action::Nothing,
        }
        if self.busy || self.status.is_none() {
            return Action::Nothing;
        }
        let s = self.st();
        let flip = |sw: Switch, name: &str| -> Action {
            if !s.secure {
                return Action::Refuse;
            }
            Action::Bridge("switch", format!("{name}={}", if sw == Switch::On { "off" } else { "on" }))
        };
        match self.row() {
            Row::Level => Action::Refresh,
            // Through Shizuku or root only on this A, and back the same way; without them the line to type, then A checks.
            Row::AdbGrant if s.can_press() => Action::Bridge("grant", if s.secure { "off" } else { "on" }.into()),
            Row::AdbGrant => Action::Refresh,
            Row::Shizuku => match s.shizuku.as_str() {
                "ready" => Action::Nothing,
                "ask" => Action::Bridge("shizuku-request", String::new()),
                "stopped" => Action::Bridge("shizuku-open", String::new()),
                _ => Action::Bridge("shizuku-get", String::new()),
            },
            Row::Root => if s.root == "yes" { Action::Nothing } else { Action::Bridge("root-check", String::new()) },
            Row::PauseMenu => Action::Bridge("watch", String::new()),
            Row::NoPopups => flip(s.heads_up_off, "heads_up"),
            Row::NoAnimations => flip(s.animations_off, "animations"),
            Row::HideBars => flip(s.bars_hidden, "bars"),
            Row::StayAwake => flip(s.stay_awake, "awake"),
        }
    }
}

fn switch_lines(s: &Status, what: &'static str) -> Vec<&'static str> {
    if s.secure { vec![what] } else { vec![what, "Needs the ADB grant, Shizuku or root first (the rows above)."] }
}


/// The lines PadWatch paints over another emulator app, in English: `texts_tsv` writes them with their translation.
pub const OVERLAY_LINES: &[&str] = &[
    "pocket-ui pause menu", "Continue", "Save state", "Load state", "Back to the shelf", "Emulator's own menu",
    "Teach the save key (F2)", "Teach the load key (F4)", "A Select", "B Back to the game",
    // What a press did, a moment over the game (PadWatch.say): saved only when the emulator wrote a state file, else what
    // reached it; "not" when the press reached no window of the emulator (it went nowhere else either).
    "Saved", "Save key sent", "Not saved", "Load key sent", "Not loaded", "Key sent", "Key not sent", "Not sent safely",
];

/// `files/extras/texts.tsv` for PadWatch: each overlay line, a tab, `tr` of it (Java has no tables of its own).
pub fn texts_tsv(tr: impl Fn(&str) -> String) -> String {
    OVERLAY_LINES.iter().map(|l| format!("{l}\t{}\n", tr(l).replace(['\t', '\n'], " "))).collect()
}

/// Every line the page can show, over the states the player can be in (`app_settings_groups`'s Android words test).
pub fn every_line() -> Vec<&'static str> {
    let states = [
        r#"{"sdk":34,"shizuku":"missing","switches":{"heads_up":"off","animations":"off","awake":"off","bars":""}}"#,
        r#"{"sdk":29,"secure":true,"shizuku":"stopped","root":"no","watch":true,"switches":{"heads_up":"on","animations":"on","awake":"on","bars":"on"}}"#,
        r#"{"sdk":33,"secure":true,"shizuku":"ask","root":"yes","switches":{"bars":""}}"#,
        r#"{"sdk":34,"shizuku":"ready"}"#,
        r#"{"sdk":34,"secure":true,"shizuku":"ready"}"#,
        r#"{"sdk":32,"secure":true,"shizuku":"ready"}"#,
        r#"{"sdk":31,"shizuku":"ready","watch":true}"#,
        r#"{"sdk":32,"shizuku":"stopped","root":"yes"}"#,
    ];
    let mut out: Vec<&'static str> = vec![SETTINGS_NAME, "…"];
    out.extend(OVERLAY_LINES);
    for json in states {
        let mut p = Page::new(Some(Status::parse(json)));
        for i in 0..p.rows().len() {
            p.sel = i;
            out.push(Page::label(p.row()));
            out.push(p.value(p.row()));
            out.extend(p.a_hint());
            out.extend(p.describe());
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub mod root;

#[cfg(test)]
mod tests {
    use super::*;

    fn status(json: &str) -> Status {
        Status::parse(json)
    }

    const PLAIN: &str = r#"{"sdk":34,"package":"xyz.pocketui.app","secure":false,"shizuku":"missing","root":"","watch":false,"switches":{"heads_up":"off","animations":"off","awake":"off","bars":""}}"#;

    #[test]
    fn the_level_follows_what_the_player_gave() {
        assert_eq!(status(PLAIN).level(), Level::Standard);
        assert_eq!(status("not json").level(), Level::Standard, "a broken answer is the plain app");
        assert_eq!(status(r#"{"secure":true}"#).level(), Level::AdbGrant);
        assert_eq!(status(r#"{"secure":true,"shizuku":"ask"}"#).level(), Level::AdbGrant, "Shizuku counts once allowed");
        assert_eq!(status(r#"{"shizuku":"ready"}"#).level(), Level::Shizuku);
        assert_eq!(status(r#"{"shizuku":"ready","root":"yes"}"#).level(), Level::Root);
        assert!(!status(r#"{"secure":true}"#).can_press(), "the grant alone presses no keys");
        assert!(status(r#"{"root":"yes"}"#).can_press());
    }

    /// Save and Load only where a press is bound to the emulator's window (Android 13+): on older Android the page promises
    /// neither, on the level, the Shizuku row or the pause menu's row (Sol's review of 6aaab4b7c: a stray press can lose a game).
    #[test]
    fn save_and_load_are_promised_only_from_android_13() {
        assert!(status(r#"{"sdk":34,"shizuku":"ready"}"#).can_save());
        // The page and the Java side draw the same line: TargetKey.SDK, which PadWatch uses.
        let key = include_str!("../android/java/xyz/pocketui/app/TargetKey.java");
        let watch = include_str!("../android/java/xyz/pocketui/app/PadWatch.java");
        assert!(key.contains(&format!("static final int SDK = {TARGETED_SDK};")), "TargetKey.SDK is TARGETED_SDK");
        assert!(watch.contains("TARGETED_SDK = TargetKey.SDK;"), "PadWatch uses TargetKey's line");
        assert!(status(r#"{"sdk":33,"shizuku":"ready"}"#).can_save(), "Android 13: with the focus check");
        assert!(status(r#"{"sdk":35,"root":"yes"}"#).can_save());
        assert!(!status(r#"{"sdk":32,"shizuku":"ready","root":"yes"}"#).can_save(), "Android 12L: no target uid");
        assert!(!status(r#"{"sdk":34,"secure":true}"#).can_save(), "the grant alone presses nothing");
        assert!(!status(r#"{"shizuku":"ready"}"#).can_save(), "Java didn't say the version");
        for (sdk, promised) in [(31, false), (32, false), (33, true), (34, true), (35, true)] {
            let mut p = Page::new(Some(status(&format!(r#"{{"sdk":{sdk},"shizuku":"ready","root":"yes","watch":true}}"#))));
            let mut lines = Vec::new();
            for i in 0..p.rows().len() {
                p.sel = i;
                lines.extend(p.describe());
            }
            let says = lines.iter().any(|l| l.contains("save and load") || l.contains("saves and loads"));
            assert_eq!(says, promised, "Android API {sdk}: {lines:?}");
        }
    }

    #[test]
    fn hide_system_bars_only_where_android_has_the_setting() {
        assert!(!rows(&status(PLAIN)).contains(&Row::HideBars), "Android 14: no policy_control");
        assert!(rows(&status(r#"{"sdk":29,"switches":{"bars":"off"}}"#)).contains(&Row::HideBars), "Android 10");
        assert!(!rows(&status(r#"{"sdk":29,"switches":{}}"#)).contains(&Row::HideBars), "Java said it can't");
    }

    #[test]
    fn switches_need_the_grant_and_say_so() {
        let mut p = Page::new(Some(status(PLAIN)));
        p.sel = p.rows().iter().position(|r| *r == Row::NoPopups).unwrap();
        assert_eq!(p.value(Row::NoPopups), "Needs the grant");
        assert_eq!(p.press(Button::South, Button::South, Button::East), Action::Refuse);
        assert!(p.describe().iter().any(|l| l.contains("Needs the ADB grant")), "the line says what first");
        assert_eq!(p.a_hint(), None, "no A where A can't change it");
        let mut p = Page::new(Some(status(r#"{"secure":true,"switches":{"heads_up":"on","animations":"off","awake":"off"}}"#)));
        p.sel = p.rows().iter().position(|r| *r == Row::NoPopups).unwrap();
        assert_eq!(p.value(Row::NoPopups), "On");
        assert_eq!(p.press(Button::South, Button::South, Button::East), Action::Bridge("switch", "heads_up=off".into()));
        p.sel += 1;
        assert_eq!(p.press(Button::South, Button::South, Button::East), Action::Bridge("switch", "animations=on".into()));
    }

    #[test]
    fn shizuku_row_walks_the_setup() {
        let at = |json: &str| {
            let mut p = Page::new(Some(status(json)));
            p.sel = 1;
            assert_eq!(p.row(), Row::Shizuku);
            let a = p.press(Button::South, Button::South, Button::East);
            (p.value(Row::Shizuku), a, p.qr().is_some(), p.describe().len())
        };
        assert_eq!(at(PLAIN), ("Not installed", Action::Bridge("shizuku-get", String::new()), true, 4), "four steps and the QR code");
        assert_eq!(at(r#"{"shizuku":"stopped"}"#).1, Action::Bridge("shizuku-open", String::new()));
        assert_eq!(at(r#"{"shizuku":"ask"}"#).1, Action::Bridge("shizuku-request", String::new()));
        let ready = at(r#"{"shizuku":"ready"}"#);
        assert_eq!((ready.0, ready.1, ready.2), ("Ready", Action::Nothing, false));
    }

    /// The grant's state shows on every Android version, beside Save/Load where that exists (Sol's recheck of 38cfee80f).
    #[test]
    fn shizuku_tells_the_grant_also_where_it_saves_and_loads() {
        for sdk in [32, 33, 34] {
            for secure in [false, true] {
                let mut p = Page::new(Some(Status { shizuku: "ready".into(), secure, sdk, ..Status::default() }));
                p.sel = 1;
                let lines = p.describe();
                let granted = lines.iter().any(|l| l.contains("already granted"));
                let invites = lines.iter().any(|l| l.contains("A on ADB grant gives"));
                let saves = lines.iter().any(|l| l.contains("save and load"));
                assert_eq!(granted, secure, "API {sdk}, grant {secure}: {lines:?}");
                assert_eq!(invites, !secure && sdk < 33, "API {sdk}, grant {secure}: {lines:?}");
                assert_eq!(saves, sdk >= 33, "API {sdk}: {lines:?}");
            }
        }
    }

    #[test]
    fn shizuku_describes_the_current_adb_grant() {
        for secure in [false, true] {
            let mut p = Page::new(Some(Status { shizuku: "ready".into(), secure, ..Status::default() }));
            p.sel = 1;
            let guidance = p.describe();
            assert!(guidance.iter().any(|line| line.contains("whole system log")));
            assert_eq!(guidance.iter().any(|line| line.contains("A on ADB grant gives")), !secure, "{guidance:?}");
            assert_eq!(guidance.iter().any(|line| line.contains("already granted")), secure, "{guidance:?}");
            p.sel = 2;
            assert_eq!(p.a_hint(), Some(if secure { "Take back" } else { "Grant" }));
        }
    }

    #[test]
    fn the_adb_line_names_this_package() {
        let mut p = Page::new(Some(status(r#"{"package":"xyz.pocketui.qa.extras"}"#)));
        p.sel = 2;
        assert_eq!(p.adb().as_deref(), Some("adb shell pm grant xyz.pocketui.qa.extras android.permission.WRITE_SECURE_SETTINGS"));
        assert_eq!(p.press(Button::South, Button::South, Button::East), Action::Refresh, "A checks again after typing it");
        let mut p = Page::new(Some(status(r#"{"secure":true}"#)));
        p.sel = 2;
        assert_eq!(p.adb(), None, "granted: no line");
        assert_eq!(adb_line(Status::default().package()), "adb shell pm grant xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS");
    }

    #[test]
    fn before_the_first_answer_nothing_happens_and_the_page_says_wait() {
        let mut p = Page::new(None);
        assert_eq!(p.value(Row::Level), "…");
        assert_eq!(p.press(Button::South, Button::South, Button::East), Action::Nothing);
        assert_eq!(p.a_hint(), None);
        assert!(!p.describe().is_empty(), "a line even before the answer (no empty page)");
        assert_eq!(p.press(Button::East, Button::South, Button::East), Action::Close);
    }

    #[test]
    fn every_row_has_words_and_up_down_wrap() {
        for json in [PLAIN, r#"{"sdk":28,"secure":true,"shizuku":"ready","root":"yes","watch":true,"switches":{"heads_up":"on","animations":"on","awake":"on","bars":"on"}}"#] {
            let mut p = Page::new(Some(status(json)));
            let n = p.rows().len();
            for i in 0..n {
                p.sel = i;
                assert!(!Page::label(p.row()).is_empty() && !p.value(p.row()).is_empty() && !p.describe().is_empty(), "{:?}", p.row());
            }
            p.sel = 0;
            p.press(Button::Up, Button::South, Button::East);
            assert_eq!(p.sel, n - 1);
            p.press(Button::Down, Button::South, Button::East);
            assert_eq!(p.sel, 0);
        }
    }

    /// Sol's review 26.11.2026: the service is the player's to switch on Android's page, also with the grant.
    #[test]
    fn the_pause_menu_row_opens_android_s_accessibility_page() {
        for json in [PLAIN, r#"{"secure":true,"watch":true}"#, r#"{"secure":true,"shizuku":"ready"}"#] {
            let mut p = Page::new(Some(status(json)));
            p.sel = 4;
            assert_eq!(p.row(), Row::PauseMenu);
            assert_eq!(p.press(Button::South, Button::South, Button::East), Action::Bridge("watch", String::new()), "{json}");
            assert!(p.describe().iter().any(|l| l.contains("accessibility page")), "{json}: where to switch it");
        }
    }

    /// Sol's review 26.11.2026: Shizuku or root give the grant only on A, and take it back the same way.
    #[test]
    fn the_grant_comes_and_goes_only_with_a() {
        let at = |json: &str| {
            let mut p = Page::new(Some(status(json)));
            p.sel = 2;
            assert_eq!(p.row(), Row::AdbGrant);
            (p.press(Button::South, Button::South, Button::East), p.a_hint())
        };
        assert_eq!(at(r#"{"shizuku":"ready"}"#), (Action::Bridge("grant", "on".into()), Some("Grant")));
        assert_eq!(at(r#"{"root":"yes","secure":true}"#), (Action::Bridge("grant", "off".into()), Some("Take back")));
        assert_eq!(at(r#"{"secure":true}"#), (Action::Refresh, Some("Check again")), "no Shizuku or root: only the check");
        assert_eq!(at(PLAIN).0, Action::Refresh);
        assert_eq!(status(r#"{"shizuku":"ready"}"#).secure, false, "Shizuku alone is no grant");
        let mut p = Page::new(Some(status(r#"{"shizuku":"ready"}"#)));
        p.sel = 2;
        assert_eq!(p.adb(), None, "A gives it: no line to type");
    }

    #[test]
    fn the_overlay_lines_go_to_java_one_per_line() {
        let tsv = texts_tsv(|s| format!("[{s}]"));
        assert_eq!(tsv.lines().count(), OVERLAY_LINES.len());
        assert!(tsv.lines().all(|l| l.split('\t').count() == 2));
        assert!(tsv.contains("Continue\t[Continue]\n"));
        assert_eq!(texts_tsv(|_| "a\tb\nc".into()).lines().next(), Some("pocket-ui pause menu\ta b c"));
    }

    /// The Java menu's English lines are the ones handed over: a row added in PadMenu.java without a line here would be
    /// English in every language.
    #[test]
    fn every_line_of_the_java_menu_is_handed_over() {
        let java = include_str!("../android/java/xyz/pocketui/app/PadMenu.java");
        let watch = include_str!("../android/java/xyz/pocketui/app/PadWatch.java");
        for line in java.lines().filter(|l| l.contains("return \"") && l.contains("case ")) {
            let text = line.split("return \"").nth(1).unwrap().split('"').next().unwrap();
            assert!(OVERLAY_LINES.contains(&text), "PadMenu's {text:?} is not in OVERLAY_LINES");
        }
        for text in ["A Select", "B Back to the game", "pocket-ui pause menu"] {
            assert!(watch.contains(&format!("tr(\"{text}\")")), "PadWatch paints {text:?}");
        }
        // The line after a press (`say`): every text PadWatch can hand it is translated, so none stays English.
        let said: Vec<&str> = watch
            .lines()
            .filter(|l| l.contains("say(") || l.contains("String no ="))
            .flat_map(|l| l.split('"').skip(1).step_by(2))
            .filter(|t| t.chars().next().is_some_and(char::is_uppercase))
            .collect();
        for text in ["Saved", "Save key sent", "Not saved", "Load key sent", "Not loaded", "Key sent", "Key not sent", "Not sent safely"] {
            assert!(said.contains(&text), "PadWatch no longer says {text:?}: {said:?}");
        }
        // An F-key meant for an emulator that reaches pocket-ui's own windows does nothing (lead, 27.11.2026: Android 13 can
        // deliver it elsewhere): the player and the shelf drop it before anything else sees it, and the native views map none.
        let player = include_str!("../android/java/xyz/pocketui/app/PlayerActivity.java");
        let pocket = include_str!("../android/java/xyz/pocketui/app/PocketActivity.java");
        for (name, java) in [("PlayerActivity", player), ("PocketActivity", pocket)] {
            let at = java.find("public boolean dispatchKeyEvent(KeyEvent event)").unwrap_or_else(|| panic!("{name} dispatchKeyEvent"));
            let body = &java[at..];
            let guard = body.find("EmulatorKeys.strayEmulatorKey(").unwrap_or_else(|| panic!("{name} drops stray F-keys"));
            let first_use = ["nativeKey(", "backRequested()", "super.dispatchKeyEvent("].iter().filter_map(|w| body.find(w)).min().unwrap();
            assert!(guard < first_use, "{name}: the F-key guard comes before anything acts on the key");
        }
        for code in 131..=142 {
            assert_eq!(crate::pad::android::button(code), None, "Android key {code} (an F-key) is no pad button");
        }
        // Both native views drop F1 to F12 before anything else in their key arm (Sol's recheck of b203531cf: the Mobile
        // view's "any key" steps, skipping a start or the way back and leaving touch mode, ran before any mapping).
        let mobile = include_str!("../android/src/lib.rs");
        let handheld = include_str!("../android/handheld/app_android.rs");
        // The same filter in both views: all twelve F-keys and Android's raw codes 131 to 142, nothing else.
        let filter = |src: &str| -> String {
            let at = src.find("fn emulator_f_key(key: &PhysicalKey) -> bool {").expect("emulator_f_key");
            src[at..at + src[at..].find("\n}\n").unwrap()].to_string()
        };
        assert_eq!(filter(mobile), filter(handheld), "the two views' F-key filters differ");
        let f = filter(mobile);
        for n in 1..=12 {
            assert!(f.contains(&format!("KeyCode::F{n} ")) || f.contains(&format!("KeyCode::F{n}\n")), "F{n} is filtered");
        }
        assert!(f.contains("NativeKeyCode::Android(131..=142)") && !f.contains("F13"), "Android's F-key codes, no more");
        let arm = mobile.find("WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed =>").expect("mobile key arm");
        let guard = mobile[arm..].find("emulator_f_key(&event.physical_key)").expect("mobile drops F-keys");
        for step in ["skip_insert()", "skip_eject()", "touch_mode = false"] {
            assert!(guard < mobile[arm..].find(step).unwrap(), "mobile: the F-key guard comes before {step}");
        }
        let arm = handheld.find("WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, repeat, .. }, .. } =>").expect("handheld key arm");
        let guard = handheld[arm..].find("emulator_f_key(physical_key)").expect("handheld drops F-keys");
        assert!(guard < handheld[arm..].find("let button = match").unwrap(), "handheld: the guard comes first");
        for text in &said {
            assert!(OVERLAY_LINES.contains(text), "PadWatch says {text:?}, which is not in OVERLAY_LINES");
        }
    }
}
