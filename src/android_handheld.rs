//! Android's handheld view (android/handheld/app_android.rs): where the games are on an Android handheld (the es_systems.cfg
//! made from these folders is `play_route_cfg::es_systems`) and the name the
//! Mobile view knows each one by. Plain functions over paths, so the Linux tests check them (the App's side is Android-only).
//!
//! - The games are in a `ROMs` folder (any case: Android's shared storage ignores it) on internal storage and on each card
//!   (`/storage/XXXX-XXXX/ROMs`), one folder per system, as ROCKNIX names them.
//! - A game's key is the Mobile view's (`android/src/ui.rs`, `game_key`): the storage provider's document id, `authority:id`,
//!   with the folders' real names, hashed; Gambatte folded in for Game Boy when it plays them. The same key is the same
//!   saves and moments in the player, whichever view started it.
use std::path::{Path, PathBuf};

/// Android's external storage provider: the authority of every document id of shared storage and cards.
pub const STORAGE_AUTHORITY: &str = "com.android.externalstorage.documents";

/// Carry the shelf's resolved title into the player; a filename is only a fallback for an unnamed launch.
pub fn player_title(name: &str, path: &Path) -> String {
    if name.trim().is_empty() {
        crate::library::display_name(path.file_stem().and_then(|s| s.to_str()).unwrap_or_default())
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod player_title_tests {
    #[test]
    fn pause_menu_keeps_the_shelf_title_for_every_kind_of_name() {
        for (file, title) in [
            ("snes/Donkey Kong 1 - Country.sfc", "Donkey Kong Country"),
            ("psx/SCES_003.44.bin", "Crash Bandicoot"),
            ("gba/Supar Mario.gba", "Super Mario Advance"),
            ("gb/Tetris (World).gb", "Mein Tetris — 日本語"),
            ("megadrive/game.zip", "Streets of Rage 2"),
        ] {
            assert_eq!(super::player_title(title, std::path::Path::new(file)), title, "{file}");
        }
        assert_eq!(super::player_title("", std::path::Path::new("Tetris (World).gb")), "Tetris");
    }
}

/// The child of `dir` named `name` in any case, with its real name ("Roms", "GBA"); None when there is none.
pub fn child_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .or_else(|| exact.is_dir().then_some(exact))
}

/// Whether a volume under /storage is a card (`1234-ABCD`: FAT and exFAT cards). A long UUID is a Linux partition another
/// system on the handheld owns (ROCKNIX's games beside Android on an Odin or Retroid): its games are that system's, often
/// copies of the ones here, and are left to it.
pub fn is_card(volume: &str) -> bool {
    let b = volume.as_bytes();
    b.len() == 9 && b[4] == b'-' && b.iter().enumerate().all(|(i, c)| i == 4 || c.is_ascii_hexdigit())
}

/// The `ROMs` folders under `storage` (`/storage`): internal storage's first, then each card's in name order.
pub fn roms_roots(storage: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = child_ci(&storage.join("emulated/0"), "roms").into_iter().collect();
    let mut cards: Vec<PathBuf> = std::fs::read_dir(storage)
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    cards.retain(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(is_card));
    cards.sort();
    roots.extend(cards.iter().filter_map(|card| child_ci(card, "roms")));
    roots
}

/// The storage provider's document id of a file on shared storage or a card ("primary:ROMs/gba/x.gba",
/// "1234-ABCD:ROMs/gba/x.gba"); None elsewhere.
pub fn document_id(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    if let Some(rest) = path.strip_prefix("/storage/emulated/0/") {
        return Some(format!("primary:{rest}"));
    }
    let (volume, inner) = path.strip_prefix("/storage/")?.split_once('/')?;
    (volume != "emulated" && volume != "self").then(|| format!("{volume}:{inner}"))
}

/// The Mobile view's key of the game at `path` (`ui::game_key`): FNV-1a of `authority:document id`, and Gambatte folded in
/// for a Game Boy or Game Boy Color game when Gambatte plays them (`gambatte`).
pub fn game_key(path: &Path, system: &str, gambatte: bool) -> Option<String> {
    let key = format!("{STORAGE_AUTHORITY}:{}", document_id(path)?);
    let mut hash = fnv(0xcbf29ce484222325, &key);
    if gambatte && matches!(system, "gb" | "gbc") {
        hash = fnv(hash, "gambatte");
    }
    Some(format!("{hash:016x}"))
}

fn fnv(start: u64, s: &str) -> u64 {
    s.bytes().fold(start, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}


/// A game's moments in the player: `states/<key>/<stem>` under the app's data (`frozen_phone::base`). The Linux pages ask
/// for `<states>/<system>/<stem>` (`slots::base`); this answers with the player's folder of that game, by the games the
/// shelf has, so the open case, Continue and the card's slots read what the player wrote, and the other way round.
#[derive(Debug, Default)]
pub struct StateKeys {
    gambatte: bool,
    keys: std::collections::HashMap<(String, String), String>,
}

impl StateKeys {
    /// The keys of `games` (system id, file); the first of two files with one name in a system keeps it, as the shelf's
    /// states have always been by that name.
    pub fn of(games: &[(String, PathBuf)], gambatte: bool) -> Self {
        let mut keys = std::collections::HashMap::new();
        for (system, path) in games {
            let (Some(stem), Some(key)) = (path.file_stem().and_then(|s| s.to_str()), game_key(path, system, gambatte)) else { continue };
            keys.entry((system.clone(), stem.to_string())).or_insert(key);
        }
        StateKeys { gambatte, keys }
    }

    /// Whether Gambatte's keys were used (Settings → Game Boy core): the keys are made again when that changes.
    pub fn gambatte(&self) -> bool {
        self.gambatte
    }

    /// `states/<key>/<stem>` under the player's data, None for a game this list doesn't have.
    pub fn base(&self, player_data: &Path, system: &str, stem: &str) -> Option<PathBuf> {
        let key = self.keys.get(&(system.to_string(), stem.to_string()))?;
        Some(player_data.join("states").join(key).join(stem))
    }
}

/// What the player is handed for a game the handheld view starts (`launchPath`'s `settings`): the Mobile view's file
/// (`launcher.json`, which also holds what only the Mobile view has) with the handheld view's own choices over it. What
/// the handheld view's Settings say is what the game shows; a look the handheld view never changed is the start look.
pub fn launch_settings(launcher: &str, view: &serde_json::Value, picture: Option<GamePicture>, cheat_text: &str, moment: bool) -> String {
    let mut prefs = serde_json::from_str::<serde_json::Value>(launcher).ok().filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}));
    for key in VIEW_KEYS {
        match view.get(key) {
            Some(v) => prefs[key] = v.clone(),
            None => {
                prefs.as_object_mut().map(|o| o.remove(key));
            }
        }
    }
    if view.get("language").is_none() {
        // Follows the phone, as the handheld view does when no language was chosen (the older spelling too).
        prefs.as_object_mut().map(|o| o.remove("german"));
    }
    match view.get("animations").and_then(|a| a.as_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("cozy") => prefs["cozy"] = true.into(),
        Some("quick") => prefs["cozy"] = false.into(),
        _ => {
            prefs.as_object_mut().map(|o| o.remove("cozy"));
        }
    }
    // The game's settings as this view keeps them (`options::GameSettings`): their effective values for what no file has,
    // and where its files are, so the player's pause menu reads and writes them as pocket-core's does on Linux.
    for key in ["picture", "settings_dir", "settings_rom"] {
        prefs.as_object_mut().map(|o| o.remove(key));
    }
    if let Some(p) = picture {
        prefs["picture"] = serde_json::to_value(&p.values).unwrap_or_default();
        prefs["settings_dir"] = p.dir.to_string_lossy().into_owned().into();
        prefs["settings_rom"] = p.rom.to_string_lossy().into_owned().into();
    }
    // The player shows the handhelds' pause menu for a game from this view (`pause_list`), not the Mobile view's sheet.
    prefs["pause_menu"] = "handheld".into();
    prefs["cheat_text"] = cheat_text.into();
    prefs["frozen_shown"] = moment.into();
    prefs.to_string()
}

/// The handheld view's settings (`settings.json`) that the player shows too: its language and the look of its menus. The
/// same names and shapes as the Mobile view's file (`Preferences`), so a value copies as it is.
const VIEW_KEYS: [&str; 9] = ["language", "text_size", "colours", "font", "retro_colours", "prompts", "motion", "extras", "gba_model"];

/// A game's settings for the player (`launch_settings`): their effective values (`options::GameSettings::effective`) and
/// the folder and ROM whose `game_settings` files hold them.
pub struct GamePicture {
    pub values: std::collections::BTreeMap<String, String>,
    pub dir: PathBuf,
    pub rom: PathBuf,
}

impl GamePicture {
    pub fn load(data: &Path, system: &str, rom: &Path) -> Self {
        GamePicture { values: crate::options::GameSettings::load(data, system, rom).effective(), dir: data.to_path_buf(), rom: rom.to_path_buf() }
    }
}

/// A same-name `.cht` beside the game (Mobile's sidecar, up to 256 KiB): its text, or "".
pub fn sidecar_cheats(rom: &Path) -> String {
    let (Some(dir), Some(stem)) = (rom.parent(), rom.file_stem()) else { return String::new() };
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_stem() == Some(stem) && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("cht")))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= 262_144))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

/// The handheld view keeps RetroAchievements' tallies and badges (`<data>/achievements`) in its own data folder; the player
/// keeps them in the app's. One folder for both, so the player scores into the tallies the shelf reads. Whatever the handheld
/// view had already is moved over, none of it is lost. (The sign-in is a file of its own: `achievements::account::share_in`.)
pub fn share_account(view_data: &Path, player_data: &Path) -> std::io::Result<()> {
    let (own, shared) = (view_data.join("achievements"), player_data.join("achievements"));
    std::fs::create_dir_all(&shared)?;
    match std::fs::symlink_metadata(&own) {
        Ok(m) if m.file_type().is_symlink() => return Ok(()),
        Ok(_) => {
            // What the view had moves over; a file the player has too stays the player's (its sign-in is the newer one).
            for e in std::fs::read_dir(&own)?.filter_map(|e| e.ok()) {
                let to = shared.join(e.file_name());
                if !to.exists() {
                    std::fs::rename(e.path(), to)?;
                }
            }
            std::fs::remove_dir_all(&own)?;
        }
        Err(_) => std::fs::create_dir_all(view_data)?,
    }
    std::os::unix::fs::symlink(&shared, &own)
}

/// The Mobile view's play history (`launcher.json`'s `played`, when each game was last started, and `play_secs`, by the
/// storage provider's document) in this view's recent games, for the games in this view's folders (`es_systems`, the
/// es_systems.cfg made here): a game played in Mobile is no "Not played yet" here, and its time goes on from Mobile's.
/// What was taken is noted in `mobile-history.json` in `view_data`, so the next start takes only what Mobile added since:
/// time played on both sides adds up, and the newer last start wins. A missing or broken file, another app's folder or a
/// game that isn't here takes nothing. Returns how many games got something; the caller saves the list.
pub fn take_mobile_history(recents: &mut crate::recents::Recents, view_data: &Path, launcher: &str, es_systems: &str) -> usize {
    let Ok(prefs) = serde_json::from_str::<serde_json::Value>(launcher) else { return 0 };
    let Some(played) = prefs["played"].as_object() else { return 0 };
    let note = view_data.join("mobile-history.json");
    let mut taken: std::collections::BTreeMap<String, (u64, u64)> = crate::fsutil::read_json_or_backup(&note).unwrap_or_default();
    let systems = system_dirs(es_systems);
    let mut changed = 0;
    for (key, at) in played {
        let Some(at) = at.as_u64() else { continue };
        let secs = prefs["play_secs"][key].as_u64().unwrap_or(0);
        let (was_at, was_secs) = taken.get(key).copied().unwrap_or_default();
        let add = secs.saturating_sub(was_secs);
        if at <= was_at && add == 0 {
            continue;
        }
        let Some(path) = key.strip_prefix(STORAGE_AUTHORITY).and_then(|k| k.strip_prefix(':')).and_then(document_path) else { continue };
        // The system whose folder holds the file (the nearest, for a game in a folder of its own).
        let Some((system, _)) = systems.iter().filter(|(_, dir)| path.starts_with(dir)).max_by_key(|(_, dir)| dir.as_os_str().len()) else { continue };
        let name = crate::library::display_name(path.file_stem().and_then(|s| s.to_str()).unwrap_or_default());
        recents.take_over(system, &name, &path, at, add);
        taken.insert(key.clone(), (at.max(was_at), secs.max(was_secs)));
        changed += 1;
    }
    if changed > 0 {
        let _ = crate::fsutil::write_json(&note, &taken);
    }
    changed
}

/// Every game the Mobile view has played, as files: the keys of `mobile-history.json` that [`take_mobile_history`] keeps
/// (all of them, while the recent games keep only 20). Sealed new games asks it, so a game played in the Mobile view is
/// never in its film on this view's shelf. Missing or broken note: none.
pub fn mobile_played(view_data: &Path) -> std::collections::HashSet<PathBuf> {
    let taken: std::collections::BTreeMap<String, (u64, u64)> = crate::fsutil::read_json_or_backup(&view_data.join("mobile-history.json")).unwrap_or_default();
    taken.keys().filter_map(|key| key.strip_prefix(STORAGE_AUTHORITY).and_then(|k| k.strip_prefix(':')).and_then(document_path)).collect()
}

/// The file of a document id of shared storage or a card ([`document_id`] the other way round).
fn document_path(id: &str) -> Option<PathBuf> {
    let (volume, inner) = id.split_once(':')?;
    if inner.is_empty() || volume.is_empty() || volume.contains('/') {
        return None;
    }
    Some(if volume == "primary" { Path::new("/storage/emulated/0").join(inner) } else { Path::new("/storage").join(volume).join(inner) })
}

/// The systems' names and folders in an es_systems.cfg.
fn system_dirs(es_systems: &str) -> Vec<(String, PathBuf)> {
    let tag = |s: &str, t: &str| s.split(&format!("<{t}>")).nth(1).and_then(|r| r.split(&format!("</{t}>")).next()).map(str::to_string);
    es_systems.split("<system>").skip(1).filter_map(|s| Some((tag(s, "name")?, PathBuf::from(tag(s, "path")?)))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dirs(root: &Path, list: &[&str]) {
        for d in list {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
    }

    /// A card's games were not read at all (only /storage/emulated/0/ROMs, review of 12.11.2026, #3): every card with a ROMs
    /// folder is a root, after internal storage.
    #[test]
    fn the_roms_folders_on_internal_storage_and_on_every_card_are_roots() {
        let s = TestDir::new("android-roots");
        dirs(&s, &["emulated/0/ROMs/gba", "1234-ABCD/roms/snes", "5678-EF01/Music", "self/primary", "416aa52f-5e6c-43bf-a961-0131731c1bef/roms/gba"]);
        let roots = roms_roots(&s);
        assert_eq!(roots, vec![s.join("emulated/0/ROMs"), s.join("1234-ABCD/roms")], "a Linux partition's roms (a long UUID) is another system's");
    }

    const MANIFEST: &str = include_str!("../android/AndroidManifest.xml");
    const POCKET: &str = include_str!("../android/java/xyz/pocketui/app/PocketActivity.java");
    const HANDHELD: &str = include_str!("../android/java/xyz/pocketui/app/HandheldActivity.java");

    fn activity_tag(name: &str) -> &'static str {
        let at = MANIFEST.find(&format!("android:name=\".{name}\"")).unwrap_or_else(|| panic!("{name} in the manifest"));
        let end = MANIFEST[at..].find('>').unwrap() + at;
        &MANIFEST[at..end]
    }

    /// Android can't take a view down and make it again in its process (its native loop never ends): a pad paired, a new
    /// language or keyboard is followed by the view itself (review of 12.11.2026, #2: an ANR when a pad was paired).
    #[test]
    fn both_views_follow_a_paired_pad_and_a_new_language_themselves() {
        for name in ["PocketActivity", "HandheldActivity"] {
            let tag = activity_tag(name);
            for change in ["keyboard|", "navigation", "locale", "layoutDirection", "orientation", "screenSize"] {
                assert!(tag.contains(change), "{name} configChanges lacks {change}: {tag}");
            }
        }
    }

    /// The first start speaks the device's language (a German Odin 2 Mini welcomed in English, 13.11.): the view's start sets
    /// `LANG` from the Activity's `language()` (Android's locale) before anything reads settings, and only when none is set, so
    /// `i18n::system_default` takes it while settings.json has no language and a choice made in Settings still wins.
    #[test]
    fn the_handheld_view_starts_in_the_device_s_language() {
        const MAIN: &str = include_str!("../android/handheld/app_android.rs");
        let start = &MAIN[MAIN.find("fn android_main(").expect("android_main")..];
        let lang = start.find(r#"bridge("language", &[])"#).expect("the view asks Android for its language");
        let set = start.find(r#"std::env::set_var("LANG", lang)"#).expect("and puts it in LANG");
        let data = start.find(r#"bridge("dataPath""#).expect("the data path");
        assert!(lang < set && set < data, "LANG is set before the data (and settings) are read");
        assert!(start[..lang].contains(r#"std::env::var_os("LANG").is_none_or"#), "only where no LANG is set");
        assert!(POCKET.contains("public String language()"), "the Activity's base answers language()");
        assert_eq!(crate::i18n::Lang::from_locale("de"), crate::i18n::Lang::De);
        assert_eq!(crate::i18n::Lang::from_locale("es"), crate::i18n::Lang::Es);
    }

    /// The files dialog (13.11.2026 rollout): it says what the app writes (a copy of the saves in "pocket-ui saves" when the Mobile
    /// view has a games folder, as `save_copy` does), in every language, and a pad has the main button focused from the start.
    #[test]
    fn the_files_dialog_tells_the_truth_about_saves_and_focuses_its_main_button() {
        let at = HANDHELD.find("private void askForFiles()").expect("the dialog");
        let dialog = &HANDHELD[at..];
        let dialog = &dialog[..dialog.find("onRequestPermissionsResult").unwrap()];
        assert_eq!(dialog.matches("pocket-ui saves").count(), 6, "English, German, Spanish, Portuguese, French and Italian name the copy's folder");
        for lie in ["saves stay in the app's own folder", "Spielstände bleiben im App-Ordner", "quedan en la carpeta de la app"] {
            assert!(!dialog.contains(lie), "{lie}");
        }
        let show = dialog.find(".show();").expect("the dialog is shown");
        let focus = dialog.find("getButton(AlertDialog.BUTTON_POSITIVE)").expect("the main button is looked up");
        assert!(focus > show && dialog[focus..].contains("requestFocus()"), "the main button takes the focus after show()");
    }

    /// A view that ends ends its process before NativeActivity's onDestroy waits for the native loop (review #1 and #2: the
    /// Mobile view after a switch, the handheld view taken down by Android).
    #[test]
    fn both_views_end_their_process_before_native_activity_s_on_destroy() {
        for (name, java) in [("PocketActivity", POCKET), ("HandheldActivity", HANDHELD)] {
            let start = java.find("protected void onDestroy()").unwrap_or_else(|| panic!("{name} has an onDestroy"));
            let body = &java[start..];
            let body = &body[..body.find("super.onDestroy()").unwrap_or_else(|| panic!("{name} calls super.onDestroy"))];
            assert!(body.contains("Views.endProcess(this)"), "{name} ends its process first: {body}");
        }
        assert!(!HANDHELD[HANDHELD.find("protected void onDestroy()").unwrap()..].split("super.onDestroy()").next().unwrap().contains("isFinishing()"),
            "the handheld view ends its process whether it was finished or taken down");
    }

    /// The launcher's icon or Home while a game of the handheld view is paused brings that task back as it was (review #5:
    /// starting the singleTask view cleared the player away).
    #[test]
    fn the_launcher_entry_brings_the_handheld_view_s_task_back_instead_of_starting_it() {
        let create = &POCKET[POCKET.find("protected void onCreate(Bundle saved)").unwrap()..];
        let create = &create[..create.find("super.onCreate").unwrap()];
        assert!(create.contains("Views.show(this, Views.HANDHELD)") && !create.contains("startActivity("), "{create}");
        let again = &POCKET[POCKET.find("protected void onNewIntent").unwrap()..];
        assert!(again[..again.find('}').unwrap()].contains("Views.show"), "onNewIntent too");
    }

    #[test]
    fn document_ids_of_internal_storage_and_cards() {
        assert_eq!(document_id(Path::new("/storage/emulated/0/Roms/GBA/x.gba")).as_deref(), Some("primary:Roms/GBA/x.gba"));
        assert_eq!(document_id(Path::new("/storage/1234-ABCD/ROMs/gba/x.gba")).as_deref(), Some("1234-ABCD:ROMs/gba/x.gba"));
        assert_eq!(document_id(Path::new("/data/x.gba")), None);
    }

    fn games() -> Vec<(String, PathBuf)> {
        vec![
            ("gba".into(), PathBuf::from("/storage/emulated/0/ROMs/gba/Ace Combat Advance.gba")),
            ("gb".into(), PathBuf::from("/storage/1234-ABCD/roms/GB/Tetris.gb")),
            ("snes".into(), PathBuf::from("/storage/emulated/0/ROMs/snes/ActRaiser.sfc")),
        ]
    }

    /// Review #8: the shelf looked for moments in `/storage/roms/savestates/<system>/`, the player writes them in
    /// `states/<key>/`: every case stayed closed and the card had no slots, Continue only worked by the player's own key.
    /// What the player wrote is what the Linux pages read (the open case, Continue, the card's slots), and what they write
    /// is where the player looks.
    #[test]
    fn the_shelf_reads_the_moments_where_the_player_wrote_them() {
        let data = TestDir::new("handheld-states");
        let keys = StateKeys::of(&games(), false);
        let key = game_key(Path::new("/storage/emulated/0/ROMs/gba/Ace Combat Advance.gba"), "gba", false).unwrap();
        let base = keys.base(&data, "gba", "Ace Combat Advance").expect("a game of the shelf has its folder");
        assert_eq!(base, crate::frozen_phone::base(&data, &key, "Ace Combat Advance.gba"), "the player's own layout, states/<key>/<stem>");
        // The player quit: its auto moment and a slot it saved are on the card.
        std::fs::create_dir_all(base.parent().unwrap()).unwrap();
        std::fs::write(crate::slots::auto_path(&base), b"moment").unwrap();
        std::fs::write(crate::slots::state_path(&base, 2), b"slot").unwrap();
        let resolver = |system: &str, stem: &str| keys.base(&data, system, stem);
        let seen = crate::slots::base_with(Some(&resolver), "gba", "Ace Combat Advance");
        assert!(crate::slots::auto_candidates(&crate::slots::auto_path(&seen), false).iter().any(|f| f.is_file()), "the open case: the moment is found");
        let set = crate::states::StateSet::pocket_core(&seen);
        assert!(set.path(crate::states::Target::Auto).is_some(), "Continue and Load find the auto moment");
        assert!(set.filled()[2], "the card's third slot is filled");
        // A game the shelf doesn't have keeps Linux's folder.
        let other = crate::slots::base_with(Some(&resolver), "gba", "Unknown");
        assert_eq!(other, crate::slots::base("gba", "Unknown"));
    }

    /// Game Boy's key follows Settings → Game Boy core, as the player's does (review #4): the moments move with it.
    #[test]
    fn game_boy_moments_follow_the_gambatte_choice() {
        let data = TestDir::new("handheld-states-gb");
        let (plain, gambatte) = (StateKeys::of(&games(), false), StateKeys::of(&games(), true));
        assert!(!plain.gambatte() && gambatte.gambatte());
        assert_ne!(plain.base(&data, "gb", "Tetris"), gambatte.base(&data, "gb", "Tetris"));
        assert!(plain.base(&data, "gb", "Tetris").is_some() && gambatte.base(&data, "gba", "Ace Combat Advance") == plain.base(&data, "gba", "Ace Combat Advance"));
    }

    fn view(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }

    /// The game shows what the handheld view's Settings say, not what the Mobile view's file holds: language, colours and
    /// text size are the handheld view's, a look it never changed is the start look; the Mobile view's own rows stay.
    #[test]
    fn the_player_gets_the_handheld_view_s_settings_over_the_mobile_view_s() {
        let launcher = r#"{"language":"de","colours":"Cool","text_size":"large","gameboy_gambatte":true,"muted":true,"cozy":false}"#;
        let out: serde_json::Value = serde_json::from_str(&launch_settings(
            launcher,
            &view(r#"{"language":"es","font":"Pixel","animations":"Cozy"}"#),
            Some(GamePicture { values: [("scaling".to_string(), "native".to_string()), ("effect".to_string(), "crt".to_string())].into_iter().collect(), dir: "/data/files/handheld".into(), rom: "/storage/emulated/0/ROMs/gba/Golden Sun.gba".into() }),
            "cheat0_desc = \"x\"",
            true,
        ))
        .unwrap();
        assert_eq!(out["language"], "es", "the handheld view's language");
        assert_eq!(out["font"], "Pixel");
        assert!(out.get("colours").is_none(), "colours the handheld view never changed are the start colours");
        assert!(out.get("text_size").is_none());
        assert_eq!(out["cozy"], true, "Animations: Cozy");
        assert_eq!(out["gameboy_gambatte"], true, "the Mobile view's own rows stay");
        assert_eq!(out["muted"], true);
        // Linux's values and where the view keeps them: the player reads and writes those very files (`game_settings`).
        assert_eq!(out["picture"], serde_json::json!({"scaling": "native", "effect": "crt"}));
        assert_eq!((out["settings_dir"].as_str(), out["settings_rom"].as_str()), (Some("/data/files/handheld"), Some("/storage/emulated/0/ROMs/gba/Golden Sun.gba")));
        assert_eq!(out["cheat_text"], "cheat0_desc = \"x\"");
        assert_eq!(out["frozen_shown"], true, "the shelf showed the case open: no old-TV switch-on in the player");
        // A launcher file nobody wrote yet (a first start in this view) is still a start of the game.
        let bare: serde_json::Value = serde_json::from_str(&launch_settings("", &view("{}"), None, "", false)).unwrap();
        assert_eq!(bare["frozen_shown"], false);
        assert!(bare.get("picture").is_none() && bare.get("settings_dir").is_none());
    }

    /// Cheats: Mobile reads a same-name `.cht` beside the game; so does this view (the card on Linux has no database here).
    #[test]
    fn a_cht_beside_the_game_is_its_cheats() {
        let d = TestDir::new("handheld-cht");
        let rom = d.join("Game.gba");
        std::fs::write(&rom, b"rom").unwrap();
        assert_eq!(sidecar_cheats(&rom), "", "no sidecar, no cheats");
        std::fs::write(d.join("Game.CHT"), "cheats = 1").unwrap();
        assert_eq!(sidecar_cheats(&rom), "cheats = 1", "any case of the extension");
        std::fs::write(d.join("Game.CHT"), vec![b'x'; 262_145]).unwrap();
        assert_eq!(sidecar_cheats(&rom), "", "more than 256 KiB is not a cheat list");
    }

    /// RetroAchievements' tallies: the handheld view and the player used two folders, so the shelf never saw what the player
    /// scored (review #8). One folder; what the view had is moved over.
    #[test]
    fn the_tallies_are_one_folder_for_the_view_and_the_player() {
        let d = TestDir::new("handheld-ra");
        let (view_data, player) = (d.join("files/handheld"), d.join("files"));
        std::fs::create_dir_all(view_data.join("achievements")).unwrap();
        std::fs::write(view_data.join("achievements/account.json"), b"{\"username\":\"A\"}").unwrap();
        std::fs::create_dir_all(player.join("achievements")).unwrap();
        std::fs::write(player.join("achievements/tally.json"), b"{}").unwrap();
        share_account(&view_data, &player).unwrap();
        assert_eq!(std::fs::read(player.join("achievements/account.json")).unwrap(), b"{\"username\":\"A\"}", "the view's account is the player's");
        assert!(player.join("achievements/tally.json").is_file(), "the player's own files stay");
        std::fs::write(player.join("achievements/new.json"), b"1").unwrap();
        assert!(view_data.join("achievements/new.json").is_file(), "the same folder from the view's side");
        share_account(&view_data, &player).unwrap();
        assert!(view_data.join("achievements/new.json").is_file(), "again is the same");
        // The player's account wins when both have one: the sign-in made in the Mobile view isn't lost to an older one.
        let e = TestDir::new("handheld-ra-both");
        let (vd, pl) = (e.join("h"), e.join("p"));
        for (dir, name) in [(&vd, "view"), (&pl, "player")] {
            std::fs::create_dir_all(dir.join("achievements")).unwrap();
            std::fs::write(dir.join("achievements/account.json"), name).unwrap();
        }
        share_account(&vd, &pl).unwrap();
        assert_eq!(std::fs::read(vd.join("achievements/account.json")).unwrap(), b"player");
    }

    /// Review #8: the sign-in and the lookups went through curl, which a handheld may not have; the Mobile view and the player
    /// use Android's HTTPS. The page asks for the platform's way, never for curl by name.
    #[test]
    fn the_achievements_page_reaches_the_server_the_platform_s_way() {
        let page = include_str!("app_achievements.rs");
        assert!(!page.contains("Curl::new"), "curl by name");
        assert!(page.matches("http::system(").count() >= 2, "sign-in and lookup");
    }

    /// Signing in on the handheld view wrote `retroachievements.json` into the view's folder, and the player looked in the
    /// app's: "not signed in, nothing runs" (found on the Odin, 13.11.). The account is one file for both.
    #[test]
    fn the_sign_in_is_one_file_for_the_view_and_the_player() {
        use crate::achievements::account::{path_with, Account};
        let d = TestDir::new("handheld-account");
        let (view_data, player) = (d.join("files/handheld"), d.join("files"));
        assert_eq!(path_with(None, &view_data), view_data.join("retroachievements.json"), "elsewhere: the page's own folder");
        assert_eq!(path_with(Some(&player), &view_data), player.join("retroachievements.json"), "shared: the app's");
        // The page saves with its own folder, the player loads with the app's: the same file.
        let account = Account { username: "qa".into(), token: "t".into(), display_name: String::new(), hardcore: false };
        account.save(&path_with(Some(&player), &view_data).parent().unwrap().to_path_buf()).unwrap();
        assert_eq!(Account::load(&player), Some(account));
    }

    const ACTRAISER: &str = "com.android.externalstorage.documents:primary:ROMs/snes/ActRaiser (USA).sfc";
    const DAY: u64 = 86_400;

    fn es() -> String {
        crate::play_route_cfg::es_systems(&[PathBuf::from("/storage/emulated/0/ROMs")])
    }

    fn launcher(played: &[(&str, u64)], secs: &[(&str, u64)]) -> String {
        let map = |l: &[(&str, u64)]| l.iter().map(|(k, v)| (k.to_string(), serde_json::Value::from(*v))).collect::<serde_json::Map<_, _>>();
        serde_json::json!({"played": map(played), "play_secs": map(secs), "light": true}).to_string()
    }

    fn line(r: &crate::recents::Recents, path: &Path) -> String {
        crate::recents::played_line(r.list.iter().find(|e| e.path == path))
    }

    /// Odin 2 Mini, 13.11.2026: ActRaiser, played in the Mobile view, said "Not played yet" on the handheld view's shelf
    /// although its moment from Mobile continued, and its play time began at 0. The shelf shows Mobile's time, and the card
    /// offers "Playing now? Press Y" as for a game played here; later play on either side adds up, nothing counted twice.
    #[test]
    fn a_game_played_in_the_mobile_view_shows_its_history_in_the_handheld_view() {
        let view = TestDir::new("mobile-history");
        let rom = Path::new("/storage/emulated/0/ROMs/snes/ActRaiser (USA).sfc");
        let two_days_ago = crate::recents::now_secs() - 2 * DAY;
        let mut r = crate::recents::Recents::load(&view);
        assert_eq!(line(&r, rom), "Not played yet");
        let mobile = launcher(&[(ACTRAISER, two_days_ago)], &[(ACTRAISER, 90 * 60)]);
        assert_eq!(take_mobile_history(&mut r, &view, &mobile, &es()), 1);
        r.save();
        let mut r = crate::recents::Recents::load(&view);
        assert_eq!(line(&r, rom), "Last played 2 days ago, 1 h 30 min in total");
        assert_eq!(r.list[0].system, "snes");
        let meta = crate::library_meta::LibraryMeta::load(&view);
        assert_eq!(meta.suggestion(rom, r.list[0].play_secs), Some(crate::library_meta::Status::Playing), "the card: \"Playing now? Press Y\"");
        // The view starts again with the same Mobile file: nothing is counted twice.
        assert_eq!(take_mobile_history(&mut r, &view, &mobile, &es()), 0);
        assert_eq!(line(&r, rom), "Last played 2 days ago, 1 h 30 min in total");
        // 10 min here, then 20 min more in Mobile: both count.
        r.note("snes", "ActRaiser", rom, 10 * 60);
        let mobile = launcher(&[(ACTRAISER, crate::recents::now_secs())], &[(ACTRAISER, 110 * 60)]);
        assert_eq!(take_mobile_history(&mut r, &view, &mobile, &es()), 1);
        assert_eq!(line(&r, rom), "Last played just now, 2 h 00 min in total");
        assert_eq!(r.list.len(), 1, "one game, one entry");
    }

    /// Sealed new games on Android (14.11.): the recent games keep 20, so the 21st game played in the Mobile view would
    /// stand in its film again on this view's shelf. Every game Mobile played is known as played, whatever the list keeps.
    #[test]
    fn every_game_the_mobile_view_played_counts_as_played_beyond_the_recent_twenty() {
        let view = TestDir::new("mobile-played");
        assert!(mobile_played(&view).is_empty(), "no note: none");
        let keys: Vec<String> = (0..25).map(|i| format!("{STORAGE_AUTHORITY}:primary:ROMs/gba/Game {i}.gba")).collect();
        let played: Vec<(&str, u64)> = keys.iter().enumerate().map(|(i, k)| (k.as_str(), 1_700_000_000 + i as u64)).collect();
        let mut r = crate::recents::Recents::load(&view);
        assert_eq!(take_mobile_history(&mut r, &view, &launcher(&played, &[]), &es()), 25);
        assert_eq!(r.list.len(), 20, "the recent games keep 20");
        let known = mobile_played(&view);
        assert_eq!(known.len(), 25);
        assert!(known.contains(Path::new("/storage/emulated/0/ROMs/gba/Game 0.gba")), "the oldest, off the recent list");
        std::fs::write(view.join("mobile-history.json"), "{broken").unwrap();
        let _ = mobile_played(&view);
    }

    /// No Mobile file, a broken one, a game of another app's folder or of another provider, a value that is no number: the
    /// shelf says "Not played yet" for what it can't place, takes what it can, and a newer game played here stays newer.
    #[test]
    fn missing_or_broken_mobile_history_leaves_the_shelf_as_it_is() {
        let view = TestDir::new("mobile-history-missing");
        let rom = Path::new("/storage/emulated/0/ROMs/snes/ActRaiser (USA).sfc");
        let mut r = crate::recents::Recents::load(&view);
        for broken in ["", "{not json", "[]", r#"{"played": 5}"#, r#"{"played": {"x": "y"}}"#] {
            assert_eq!(take_mobile_history(&mut r, &view, broken, &es()), 0, "{broken:?}");
        }
        assert!(r.list.is_empty());
        assert_eq!(line(&r, rom), "Not played yet");
        let now = crate::recents::now_secs();
        let gba = "com.android.externalstorage.documents:1234-ABCD:ROMs/gba/Golden Sun.gba";
        let unknown = [
            "com.android.providers.downloads.documents:document/42",
            "com.android.externalstorage.documents:primary:Download/ActRaiser (USA).sfc",
            "com.android.externalstorage.documents:primary:ROMs/no-such-system/x.bin",
        ];
        let mut played: Vec<(&str, u64)> = unknown.iter().map(|k| (*k, now - DAY)).collect();
        played.push((gba, now - 3 * DAY));
        played.push((ACTRAISER, now - 5 * DAY));
        // ActRaiser was played here yesterday, after its last Mobile start.
        r.note("snes", "ActRaiser", rom, 60);
        r.list[0].last_played = now - DAY;
        // The card's gba folder as es_systems.cfg lists it on the device, where the folder is there.
        let es = es() + "  <system><name>gba</name><fullname>Game Boy Advance</fullname><path>/storage/1234-ABCD/ROMs/gba</path></system>\n";
        assert_eq!(take_mobile_history(&mut r, &view, &launcher(&played, &[(ACTRAISER, 300)]), &es), 2);
        assert_eq!(line(&r, rom), "Last played yesterday, 6 min in total", "the newer time from here stays, the time adds up");
        let golden = Path::new("/storage/1234-ABCD/ROMs/gba/Golden Sun.gba");
        assert_eq!(line(&r, golden), "Last played 3 days ago, under a minute in total", "a start without time: started, no time yet");
        assert_eq!(r.list.iter().map(|e| e.path.as_path()).collect::<Vec<_>>(), [rom, golden], "newest first, nothing else");
    }
}
