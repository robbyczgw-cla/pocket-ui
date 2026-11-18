//! The Android games: apps the player calls games, on a shelf of their own ("Android"), in both of Android's views. One list,
//! one file: `android_games` in the Mobile view's `launcher.json` (package → its name when it was marked). The Mobile view
//! marks them in its drawer under the shelf (`android/src/apps_page.rs`); the handheld view on Settings › Games › Android games
//! (`app_android_games.rs`), where they stand as the system `android`: one small file per game in a folder of the view's own
//! (`write_folder`), named in a gamelist with the app's icon or banner as its cover. Android's own "game" category is a hint
//! on the row, nothing more: many games don't set it, some apps do. See docs/ANDROID-MVP.md "Android apps".
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::pad::Button;

/// The system the Android games stand under (`media::is_digital`: a case without a cartridge).
pub const SYSTEM_ID: &str = "android";
pub const SYSTEM: &str = "Android";
/// A game key of the Mobile view's Android shelf: never a document identity (those are `authority:documentId`).
pub const KEY_PREFIX: &str = "android-app:";
/// The handheld view's file of one Android game: `<package>.app`, holding the package.
pub const EXTENSION: &str = "app";
/// The handheld view's folder of them, under its data folder.
pub const FOLDER: &str = "android-games";
/// Who plays an Android game: Android itself, by the app's start (`emu_choice::Runner`'s emulator).
pub const EMULATOR: &str = "android-app";
/// What tests (and the Linux build in Android's profile) hand in for Android's list of apps, as `AppDrawer.list` writes it.
pub const APPS_ENV: &str = "POCKETUI_ANDROID_APPS";

/// An app Android lists with a start icon (`AppDrawer.list`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    pub package: String,
    pub label: String,
    /// Android files it under games (`ApplicationInfo.CATEGORY_GAME`): a suggestion, nothing more.
    pub game: bool,
    /// Its icon as `AppDrawer.writeIcon` wrote it (raw ARGB), or "".
    pub icon: String,
}

/// The apps of `AppDrawer.list`'s answer, by name as a home screen orders them, each once.
pub fn parse(json: &str) -> Vec<App> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let mut apps: Vec<App> = items
        .iter()
        .filter_map(|v| {
            let package = v["package"].as_str().filter(|p| !p.is_empty())?.to_string();
            let label = v["label"].as_str().map(str::trim).filter(|l| !l.is_empty()).unwrap_or(&package).to_string();
            Some(App { package, label, game: v["game"].as_bool().unwrap_or(false), icon: v["icon"].as_str().unwrap_or_default().to_string() })
        })
        .collect();
    apps.sort_by_cached_key(|a| (a.label.to_lowercase(), a.package.clone()));
    apps.dedup_by(|a, b| a.package == b.package);
    apps
}

/// The marked apps of `launcher.json` (package → name). A file that is missing or broken marks none.
pub fn marked(launcher_json: &str) -> BTreeMap<String, String> {
    let v: serde_json::Value = serde_json::from_str(launcher_json).unwrap_or_default();
    v["android_games"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(p, l)| Some((p.clone(), l.as_str()?.to_string())).filter(|(p, _)| !p.is_empty()))
        .collect()
}

/// `launcher.json` with `marked` as its Android games; everything else in it stays as it is.
pub fn json_with_marked(launcher_json: &str, marked: &BTreeMap<String, String>) -> String {
    let mut v: serde_json::Value = serde_json::from_str(launcher_json).unwrap_or_default();
    if !v.is_object() {
        v = serde_json::json!({});
    }
    v["android_games"] = serde_json::to_value(marked).unwrap_or_default();
    serde_json::to_string(&v).unwrap_or_default()
}

/// The shelf's games: the marked apps Android still lists, by the name Android gives them now (before its first answer
/// as they were marked); an app that went keeps its mark for a reinstall but leaves the shelf. By name.
pub fn shelf(marked: &BTreeMap<String, String>, listed: Option<&[App]>) -> Vec<(String, String)> {
    let mut games: Vec<(String, String)> = match listed {
        Some(list) => list.iter().filter(|a| marked.contains_key(&a.package)).map(|a| (a.package.clone(), a.label.clone())).collect(),
        None => marked.iter().map(|(p, l)| (p.clone(), l.clone())).collect(),
    };
    games.sort_by_cached_key(|(p, l)| (l.to_lowercase(), p.clone()));
    games
}

/// Names of marked apps follow their app's (Android's list renames them); true when one changed.
pub fn follow_names(marked: &mut BTreeMap<String, String>, listed: &[App]) -> bool {
    let mut changed = false;
    for app in listed {
        if let Some(label) = marked.get_mut(&app.package).filter(|l| **l != app.label) {
            label.clone_from(&app.label);
            changed = true;
        }
    }
    changed
}

/// The app an Android game's file starts (`<package>.app` of `FOLDER`); None for every other file.
pub fn package_of(path: &Path) -> Option<String> {
    if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
        return None;
    }
    path.file_stem().and_then(|s| s.to_str()).filter(|p| !p.is_empty()).map(str::to_string)
}

/// Where an app's cover is in the folder (`covers/<package>.png`, the app's banner or its icon).
pub fn cover_path(dir: &Path, package: &str) -> PathBuf {
    dir.join("covers").join(format!("{package}.png"))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The handheld view's folder for `games` (package, name): a file per game, holding its package, and a gamelist that names
/// them and points at their cover where there is one. Files of games that left the shelf go; the covers stay for a return.
/// True when anything in it changed (the library is read again then).
pub fn write_folder(dir: &Path, games: &[(String, String)]) -> std::io::Result<bool> {
    std::fs::create_dir_all(dir)?;
    let mut changed = false;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if package_of(&path).is_some_and(|p| !games.iter().any(|(g, _)| *g == p)) {
            std::fs::remove_file(&path)?;
            changed = true;
        }
    }
    let mut list = String::from("<?xml version=\"1.0\"?>\n<gameList>\n");
    for (package, name) in games {
        let file = dir.join(format!("{package}.{EXTENSION}"));
        if std::fs::read(&file).ok().as_deref() != Some(package.as_bytes()) {
            crate::fsutil::write_atomic(&file, package.as_bytes())?;
            changed = true;
        }
        list.push_str(&format!("  <game><path>./{}.{EXTENSION}</path><name>{}</name>", xml(package), xml(name)));
        if cover_path(dir, package).is_file() {
            list.push_str(&format!("<image>./covers/{}.png</image>", xml(package)));
        }
        list.push_str("</game>\n");
    }
    list.push_str("</gameList>\n");
    let gamelist = dir.join("gamelist.xml");
    if std::fs::read_to_string(&gamelist).ok().as_deref() != Some(list.as_str()) {
        crate::fsutil::write_atomic(&gamelist, list.as_bytes())?;
        changed = true;
    }
    Ok(changed)
}

/// The system's entry in the handheld view's es_systems.cfg: its folder, our files only. A game's start is the app's
/// (`App::queue_launch` hands it to Android), so no command runs.
pub fn es_entry(dir: &Path) -> String {
    format!(
        "  <system><name>{SYSTEM_ID}</name><fullname>{SYSTEM}</fullname><path>{}</path><extension>.{EXTENSION}</extension><command>true</command></system>\n",
        xml(&dir.to_string_lossy())
    )
}

/// `es_systems` (a whole es_systems.cfg) with the Android games' system in it, once.
pub fn with_es_entry(es_systems: &str, dir: &Path) -> String {
    if es_systems.contains(&format!("<name>{SYSTEM_ID}</name>")) {
        return es_systems.to_string();
    }
    match es_systems.rfind("</systemList>") {
        Some(at) => format!("{}{}{}", &es_systems[..at], es_entry(dir), &es_systems[at..]),
        None => es_systems.to_string(),
    }
}

// ---- the handheld view's page ------------------------------------------------------------------------------------

/// What A did on the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Close,
    /// The app at this row went onto the shelf (true) or off it.
    Marked(usize, bool),
}

/// Settings › Games › Android games: every app Android lists, A puts one on the Android shelf or takes it off.
#[derive(Debug, Default)]
pub struct Page {
    /// None until Android answered (or could not).
    pub apps: Option<Vec<App>>,
    pub sel: usize,
}

impl Page {
    pub fn new(apps: Option<Vec<App>>) -> Page {
        Page { apps, sel: 0 }
    }

    pub fn len(&self) -> usize {
        self.apps.as_ref().map_or(0, Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn selected(&self) -> Option<&App> {
        self.apps.as_ref()?.get(self.sel)
    }

    /// Up/Down move (round the ends), A marks or unmarks the selected app in `marked`, B (`cancel`) closes.
    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button, marked: &mut BTreeMap<String, String>) -> Action {
        let n = self.len();
        match b {
            b if b == cancel => Action::Close,
            Button::Up if n > 0 => {
                self.sel = (self.sel + n - 1) % n;
                Action::Nothing
            }
            Button::Down if n > 0 => {
                self.sel = (self.sel + 1) % n;
                Action::Nothing
            }
            b if b == confirm => {
                let Some(app) = self.selected().cloned() else { return Action::Nothing };
                let on = marked.remove(&app.package).is_none();
                if on {
                    marked.insert(app.package, app.label);
                }
                Action::Marked(self.sel, on)
            }
            _ => Action::Nothing,
        }
    }

    /// The row's value: on the shelf, or that Android files it under games.
    pub fn note(app: &App, marked: &BTreeMap<String, String>) -> &'static str {
        if marked.contains_key(&app.package) {
            "On the shelf"
        } else if app.game {
            "Looks like a game"
        } else {
            ""
        }
    }

    /// The line over the list: how to put games there, and that none is there yet; or why there is no list.
    pub fn line(&self, marked: &BTreeMap<String, String>) -> &'static str {
        match (&self.apps, marked.is_empty()) {
            (None, _) => "Reading your apps…",
            (Some(a), _) if a.is_empty() => "Android lists no other apps with a start icon. Install a game from your app store, then open this page again.",
            (Some(_), true) => "No Android games on the shelf yet. Press A on a game to put it on the Android shelf.",
            (Some(_), false) => "A puts a game on the Android shelf or takes it off. They stand there beside your consoles.",
        }
    }

    /// The hint for A; None when A does nothing.
    pub fn a_hint(&self, marked: &BTreeMap<String, String>) -> Option<&'static str> {
        let app = self.selected()?;
        Some(if marked.contains_key(&app.package) { "Take off the shelf" } else { "Put on the shelf" })
    }
}

/// What the shelf says when Android didn't start a game's app (it went since Android last listed it).
pub fn not_started(name: &str) -> String {
    format!("{name} didn't start. Android couldn't open the app. It may have been removed.")
}

/// The Settings row's value: how many games are on the shelf.
pub fn count(n: usize) -> String {
    match n {
        0 => "None".into(),
        1 => "1 game".into(),
        n => format!("{n} games"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn apps() -> Vec<App> {
        parse(r#"[{"package":"com.b.racer","label":"Racer","game":true,"icon":""},{"package":"org.mail","label":"Mail","game":false,"icon":"/x.argb"},
                  {"package":"com.a.puzzle","label":" ","game":false},{"package":"org.mail","label":"Mail twice"},{"label":"no package"}]"#)
    }

    #[test]
    fn android_s_list_by_name_each_app_once() {
        let a = apps();
        assert_eq!(a.iter().map(|a| a.package.as_str()).collect::<Vec<_>>(), ["com.a.puzzle", "org.mail", "com.b.racer"], "{a:?}");
        assert_eq!(a[0].label, "com.a.puzzle", "no name: the package");
        assert!(a[2].game && !a[1].game);
        assert_eq!(a[1].icon, "/x.argb");
        assert!(parse("not json").is_empty() && parse("{}").is_empty());
    }

    #[test]
    fn the_mark_is_launcher_json_s_and_the_rest_of_it_stays() {
        let json = r#"{"light":true,"plays_with":{"psp":"org.ppsspp.ppsspp"},"android_games":{"com.b.racer":"Racer"}}"#;
        let mut m = marked(json);
        assert_eq!(m.get("com.b.racer").map(String::as_str), Some("Racer"));
        m.insert("com.a.puzzle".into(), "Puzzle".into());
        let out = json_with_marked(json, &m);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["light"], true);
        assert_eq!(v["plays_with"]["psp"], "org.ppsspp.ppsspp");
        assert_eq!(marked(&out).len(), 2);
        assert!(marked("").is_empty() && marked("[1]").is_empty() && marked(r#"{"android_games":7}"#).is_empty(), "broken: none marked");
        assert_eq!(marked(&json_with_marked("broken", &m)).len(), 2, "a broken file is made anew");
    }

    #[test]
    fn the_shelf_has_the_marked_apps_android_still_lists_by_their_names_now() {
        let mut m = BTreeMap::from([("com.b.racer".to_string(), "Old racer".to_string()), ("gone.app".to_string(), "Gone".to_string())]);
        assert_eq!(shelf(&m, None), [("gone.app".to_string(), "Gone".to_string()), ("com.b.racer".to_string(), "Old racer".to_string())], "before Android answered: as marked");
        let a = apps();
        assert_eq!(shelf(&m, Some(&a)), [("com.b.racer".to_string(), "Racer".to_string())]);
        assert!(follow_names(&mut m, &a) && m["com.b.racer"] == "Racer" && m.contains_key("gone.app"), "renamed; the gone app keeps its mark");
        assert!(!follow_names(&mut m, &a));
    }

    #[test]
    fn the_folder_holds_a_file_per_game_and_a_gamelist_naming_them() {
        let dir = TestDir::new("android-games-folder");
        let folder = dir.join(FOLDER);
        let games = vec![("com.b.racer".to_string(), "Racer & Co <3>".to_string()), ("com.a.puzzle".to_string(), "Puzzle".to_string())];
        std::fs::create_dir_all(folder.join("covers")).unwrap();
        std::fs::write(cover_path(&folder, "com.b.racer"), b"png").unwrap();
        assert!(write_folder(&folder, &games).unwrap());
        assert!(!write_folder(&folder, &games).unwrap(), "the same games: nothing written");
        assert_eq!(std::fs::read_to_string(folder.join("com.b.racer.app")).unwrap(), "com.b.racer", "not empty: an empty file is a broken copy");
        let entries = crate::gamelist::parse(&std::fs::read_to_string(folder.join("gamelist.xml")).unwrap(), &folder);
        let racer = entries.iter().find(|e| e.path == folder.join("com.b.racer.app")).expect("named in the gamelist");
        assert_eq!(racer.name, "Racer & Co <3>");
        assert!(write_folder(&folder, &games[1..]).unwrap());
        assert!(!folder.join("com.b.racer.app").exists() && cover_path(&folder, "com.b.racer").exists(), "off the shelf: its file goes, the cover stays");
        assert_eq!(package_of(&folder.join("com.a.puzzle.app")).as_deref(), Some("com.a.puzzle"));
        assert_eq!(package_of(&folder.join("gamelist.xml")), None);
    }

    #[test]
    fn es_systems_gets_the_android_system_once() {
        let es = "<?xml version=\"1.0\"?>\n<systemList>\n  <system><name>gba</name></system>\n</systemList>\n";
        let dir = Path::new("/data/handheld/android-games");
        let with = with_es_entry(es, dir);
        assert!(with.contains("<name>android</name>") && with.contains("<path>/data/handheld/android-games</path>") && with.ends_with("</systemList>\n"));
        assert_eq!(with_es_entry(&with, dir), with);
        let tmp = TestDir::new("android-games-es");
        std::fs::write(tmp.join("es_systems.cfg"), &with).unwrap();
        let systems = crate::library::all_systems(&tmp.join("es_systems.cfg")).unwrap();
        assert!(systems.iter().any(|s| s.id == SYSTEM_ID && s.path == dir), "the library reads it");
    }

    #[test]
    fn a_marks_and_unmarks_b_closes_and_every_state_says_something() {
        let mut m = BTreeMap::new();
        let mut p = Page::new(Some(apps()));
        assert_eq!(p.line(&m), "No Android games on the shelf yet. Press A on a game to put it on the Android shelf.");
        p.press(Button::Up, A, B, &mut m);
        assert_eq!(p.selected().unwrap().package, "com.b.racer", "Up from the first row: the last");
        assert_eq!(Page::note(p.selected().unwrap(), &m), "Looks like a game");
        assert_eq!(p.a_hint(&m), Some("Put on the shelf"));
        assert_eq!(p.press(A, A, B, &mut m), Action::Marked(2, true));
        assert_eq!(m.get("com.b.racer").map(String::as_str), Some("Racer"));
        assert_eq!((Page::note(p.selected().unwrap(), &m), p.a_hint(&m)), ("On the shelf", Some("Take off the shelf")));
        assert_ne!(p.line(&m), Page::new(Some(apps())).line(&BTreeMap::new()));
        assert_eq!(p.press(A, A, B, &mut m), Action::Marked(2, false));
        assert!(m.is_empty());
        assert_eq!(p.press(B, A, B, &mut m), Action::Close);
        let mut none = Page::new(Some(Vec::new()));
        assert_eq!((none.press(A, A, B, &mut m), none.press(Button::Down, A, B, &mut m), none.a_hint(&m)), (Action::Nothing, Action::Nothing, None));
        assert!(none.line(&m).contains("app store"));
        assert!(!Page::new(None).line(&m).is_empty());
        assert_eq!((count(0), count(1), count(3)), ("None".to_string(), "1 game".to_string(), "3 games".to_string()));
    }
}
