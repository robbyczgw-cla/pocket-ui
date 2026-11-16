//! The choices of `play_route` in the language of the "Runs on" page: system.cfg keys. The page (`emu_choice_page`) reads and
//! writes keys like ROCKNIX's (`<system>.emulator=<emulator>`, `<system>["<file>"].emulator`); on Android they are made from
//! launcher.json's `plays_with` and `plays_with_game` and go back into them, so the Mobile view's Plays with and this page
//! are one choice. Only the handheld view has the page, so only its module set has this file.

use crate::emu_choice::{Cfg, POCKET_CORE};
use crate::play_route::{system_key, Prefs, OWN};

/// A game the library has: ROCKNIX's system name, its file name and its key in the player (the Mobile view's `Game::key`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRef {
    pub system: String,
    pub file: String,
    pub key: String,
}

/// The choices as system.cfg keys, the language of the "Runs on" page: `<system>.emulator=<package>` for a system's app,
/// `<system>["<file>"].emulator=<package or pocket-core>` for a game's.
pub fn cfg_from_prefs(prefs: &Prefs, systems: &[&str], games: &[GameRef]) -> Cfg {
    let mut text = String::new();
    for system in systems {
        if let Some(package) = prefs.plays_with.get(system_key(system)) {
            text.push_str(&format!("{system}.emulator={package}\n"));
        }
    }
    for g in games {
        if let Some(chosen) = prefs.plays_with_game.get(&g.key) {
            let emulator = if chosen == OWN { POCKET_CORE } else { chosen };
            text.push_str(&format!("{}.emulator={emulator}\n", crate::emu_choice::game_config_name(&g.system, &g.file)));
        }
    }
    Cfg::parse(&text)
}

/// What the page saved (system.cfg keys, None removes one) as choices: the emulator keys only, `pocket-core` meaning the
/// own player (no entry for a system, `OWN` for a game).
pub fn apply_edits(prefs: &mut Prefs, edits: &[(String, Option<String>)], games: &[GameRef]) {
    for (key, value) in edits {
        let Some((subject, name)) = key.rsplit_once('.') else { continue };
        if name != "emulator" {
            continue;
        }
        let value = value.as_deref().filter(|v| !v.is_empty());
        if subject.contains("[\"") {
            let Some(game) = games.iter().find(|g| crate::emu_choice::game_config_name(&g.system, &g.file) == subject) else { continue };
            match value {
                None => prefs.plays_with_game.remove(&game.key),
                Some(POCKET_CORE) => prefs.plays_with_game.insert(game.key.clone(), OWN.to_string()),
                Some(package) => prefs.plays_with_game.insert(game.key.clone(), package.to_string()),
            };
        } else {
            let id = system_key(subject);
            if id.is_empty() {
                continue;
            }
            match value {
                None | Some(POCKET_CORE) => prefs.plays_with.remove(id),
                Some(package) => prefs.plays_with.insert(id.to_string(), package.to_string()),
            };
        }
    }
}

/// The systems the APK's player has a core for, from the Brick's es_systems.cfg (the same names as ROCKNIX's), with
/// their folders under `roms`, and the systems only another app plays (`EXTRA_SYSTEMS`). A folder that isn't there is a
/// system without games, as on Linux.
pub fn es_systems(roots: &[std::path::PathBuf]) -> String {
    use crate::android_handheld::child_ci;
    let template = include_str!("../deploy/brick/es_systems.cfg");
    let mut out = String::from("<?xml version=\"1.0\"?>\n<systemList>\n");
    // One entry for each ROMs folder (internal storage, each card) that has the system's folder, with its real name: the
    // library joins a system's entries into one (`library::load`), and the game's key needs the real name (`android_handheld`).
    for line in template.lines().filter(|l| l.trim_start().starts_with("<system>")) {
        let name = line.split("<name>").nth(1).and_then(|s| s.split("</name>").next()).unwrap_or_default();
        if crate::play_route::bundled_core(name).is_none() {
            continue;
        }
        let template_path = line.split("<path>").nth(1).and_then(|s| s.split("</path>").next()).unwrap_or_default();
        let folder = std::path::Path::new(template_path).file_name().and_then(|n| n.to_str()).unwrap_or(name);
        // Where no root has it yet, under the first: a folder made later is read when the library is read again.
        let mut dirs: Vec<std::path::PathBuf> = roots.iter().filter_map(|root| child_ci(root, folder)).collect();
        if dirs.is_empty() {
            dirs.extend(roots.first().map(|r| r.join(folder)));
        }
        // The name never says "(experimental)" (the template's Brick note included): on the shelf and the system menu it read as
        // a warning about the whole system (the Odin hand test, 15.11.). What is still being tested is said once, in the manual's
        // "Which systems play where".
        let (full, note) = (line.split("<fullname>").nth(1).and_then(|s| s.split("</fullname>").next()).unwrap_or_default(), " (experimental)");
        let named = full.strip_suffix(note).unwrap_or(full).to_string();
        let line = line.replace(&format!("<fullname>{full}</fullname>"), &format!("<fullname>{named}</fullname>"));
        for dir in dirs {
            out.push_str(&line.replace(template_path, &dir.to_string_lossy()));
            out.push('\n');
        }
    }
    // Always listed where its folder is: a folder of PS2 games with no app installed still shows them, and Play says which app is
    // missing. The folder is the first of its usual names that is there (a library may call GameCube's `gc`).
    for (name, full, types, folders) in EXTRA_SYSTEMS {
        let mut dirs: Vec<std::path::PathBuf> = roots.iter().filter_map(|root| folders.iter().find_map(|f| child_ci(root, f))).collect();
        if dirs.is_empty() {
            dirs.extend(roots.first().map(|r| r.join(folders[0])));
        }
        for dir in dirs {
            out.push_str(&format!(
                "  <system><name>{name}</name><fullname>{full}</fullname><path>{}</path><extension>{types}</extension><command>true</command></system>\n",
                dir.to_string_lossy()
            ));
        }
    }
    out.push_str("</systemList>\n");
    out
}

/// What no core of ours plays: another app does (`play_route::Phone::way`). ROCKNIX's names, the folders under ROMs.
const EXTRA_SYSTEMS: &[(&str, &str, &str, &[&str])] = &[
    ("ps2", "PlayStation 2", ".iso .ISO .chd .CHD .cso .CSO .bin .BIN .m3u .M3U", &["ps2", "playstation2"]),
    ("gamecube", "GameCube", ".rvz .RVZ .iso .ISO .gcm .GCM .gcz .GCZ .ciso .CISO .wbfs .WBFS", &["gamecube", "gc", "ngc"]),
    ("wii", "Wii", ".rvz .RVZ .iso .ISO .wbfs .WBFS .gcz .GCZ .wad .WAD", &["wii"]),
    ("3ds", "Nintendo 3DS", ".3ds .3DS .cci .CCI .cxi .CXI .3dsx .3DSX", &["3ds", "n3ds"]),
    ("xbox", "Xbox", ".iso .ISO .xiso .XISO", &["xbox"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_route::Prefs;

    fn games() -> Vec<GameRef> {
        vec![
            GameRef { system: "psp".into(), file: "Alpha.iso".into(), key: "00000000000000a1".into() },
            GameRef { system: "psp".into(), file: "Be=ta #2.iso".into(), key: "00000000000000b2".into() },
            GameRef { system: "wii".into(), file: "Mario.rvz".into(), key: "00000000000000c3".into() },
        ]
    }

    /// The handheld view's system names carry no "(experimental)": PSP and Dreamcast said it on the shelf and in the system
    /// menu, and it read as a warning about the system (the Odin hand test, 15.11.). DS left the list on 12.11. already.
    #[test]
    fn the_system_names_say_no_experimental() {
        let cfg = es_systems(&[std::path::PathBuf::from("/storage/emulated/0/ROMs")]);
        let full = |id: &str| -> String {
            let line = cfg.lines().find(|l| l.contains(&format!("<name>{id}</name>"))).unwrap_or_else(|| panic!("{id} in {cfg}"));
            line.split("<fullname>").nth(1).and_then(|s| s.split("</fullname>").next()).unwrap().to_string()
        };
        assert_eq!(full("nds"), "Nintendo DS");
        assert_eq!(full("gba"), "Game Boy Advance");
        for id in ["n64", "psp", "saturn", "dreamcast"] {
            assert!(!full(id).to_lowercase().contains("experimental"), "{id}: {}", full(id));
        }
        assert!(!cfg.to_lowercase().contains("experimental"), "{cfg}");
    }

    #[test]
    fn the_runs_on_page_reads_and_writes_the_same_choices_mobile_does() {
        let mut prefs = Prefs::default();
        prefs.plays_with.insert("psp".into(), "org.ppsspp.ppsspp".into());
        prefs.plays_with.insert("gamecube".into(), "org.dolphinemu.dolphinemu".into());
        prefs.plays_with_game.insert("00000000000000a1".into(), OWN.into());
        prefs.plays_with_game.insert("00000000000000b2".into(), "org.ppsspp.ppssppgold".into());
        let cfg = cfg_from_prefs(&prefs, &["psp", "wii", "gba"], &games());
        assert_eq!(cfg.get("psp.emulator"), Some("org.ppsspp.ppsspp"));
        // "wii" is Mobile's "gamecube": one choice for both.
        assert_eq!(cfg.get("wii.emulator"), Some("org.dolphinemu.dolphinemu"));
        assert_eq!(cfg.get("gba.emulator"), None);
        assert_eq!(cfg.get("psp[\"Alpha.iso\"].emulator"), Some(POCKET_CORE));
        // ES's name of a game: `=` and `#` taken out.
        assert_eq!(cfg.get("psp[\"Beta 2.iso\"].emulator"), Some("org.ppsspp.ppssppgold"));

        // What the page saves goes back into the same two maps; `.core` and every other key are not ours.
        let mut saved = Prefs::default();
        apply_edits(
            &mut saved,
            &[
                ("psp.emulator".into(), Some("org.ppsspp.ppsspp".into())),
                ("psp.core".into(), None),
                ("wii.emulator".into(), Some("org.dolphinemu.dolphinemu".into())),
                ("psp[\"Alpha.iso\"].emulator".into(), Some(POCKET_CORE.into())),
                ("psp[\"Beta 2.iso\"].emulator".into(), Some("org.ppsspp.ppssppgold".into())),
                ("psp.ratio".into(), Some("4:3".into())),
            ],
            &games(),
        );
        assert_eq!(saved, prefs);
        // pocket-core for a console and a removed key are both "our own player"; a removed game key is "like the system".
        apply_edits(
            &mut saved,
            &[("psp.emulator".into(), Some(POCKET_CORE.into())), ("wii.emulator".into(), None), ("psp[\"Alpha.iso\"].emulator".into(), None)],
            &games(),
        );
        assert!(saved.plays_with.is_empty());
        assert_eq!(saved.plays_with_game.len(), 1);
        // A game the library doesn't have, or a system nobody knows, changes nothing.
        let before = saved.clone();
        apply_edits(&mut saved, &[("psp[\"Nope.iso\"].emulator".into(), Some("x".into())), ("zzz.emulator".into(), Some("x".into()))], &games());
        assert_eq!(saved, before);
    }

    /// The "Runs on" page resolves with `emu_choice::resolve`; the launch asks `Phone::way`. Two functions, so this test is the
    /// guard: for every system kind, set of apps and choice they name the same runner (our player, or the same package).
    #[test]
    fn the_page_and_the_launch_never_decide_differently() {
        use crate::emu_choice::{self, Ctx, EsEmulator, Runner};
        use crate::play_route::{own_core, Phone, PhoneApp, Way};
        let apps = |names: &[&str]| -> Vec<PhoneApp> { names.iter().map(|p| PhoneApp { package: p.to_string(), label: p.to_string() }).collect() };
        let mut checked = 0;
        // gba: a bundled core; psp: a download core; ps2: no player of ours.
        for (system, file, profile) in [("gba", "A.gba", "retroarch:mgba"), ("psp", "A.iso", "psp"), ("ps2", "A.iso", "ps2"), ("wii", "A.rvz", "dolphin")] {
            for installed in [&[][..], &["x.one"][..], &["x.one", "x.two"][..]] {
                for core_there in [false, true] {
                    for system_choice in [None, Some("x.one"), Some("x.two"), Some("gone.app")] {
                        for game_choice in [None, Some(OWN), Some("x.one"), Some("x.two"), Some("gone.app")] {
                            let mut phone = Phone::default();
                            phone.apps.insert(profile.to_string(), apps(installed));
                            if core_there {
                                phone.cores.extend(own_core(system, false).map(str::to_string));
                            }
                            let mut prefs = Prefs::default();
                            if let Some(c) = system_choice {
                                prefs.plays_with.insert(crate::play_route::system_key(system).to_string(), c.to_string());
                            }
                            let key = "com.android.externalstorage.documents:primary:ROMs/x/A".to_string();
                            if let Some(c) = game_choice {
                                prefs.plays_with_game.insert(key.clone(), c.to_string());
                            }
                            let way = phone.way(system, game_choice, system_choice);
                            let emulators: Vec<EsEmulator> = phone.apps_for(system).iter().map(|a| EsEmulator { name: a.package.clone(), cores: vec![], default_core: None }).collect();
                            let games = [GameRef { system: system.to_string(), file: file.to_string(), key }];
                            let cfg = cfg_from_prefs(&prefs, &[system], &games);
                            let ctx = Ctx { system, emulators: &emulators, pocket_core: own_core(system, false), pocket_cores: &[], file_ok: true, fell_back: false, default_override: None };
                            let page = emu_choice::resolve(&ctx, &cfg, Some(file)).runner;
                            let launch = match &way {
                                Way::Own { core } | Way::NeedsCore { core } => Runner::new(POCKET_CORE, core),
                                Way::App(a) => Runner::new(&a.package, ""),
                                Way::NoApp { .. } => Runner::default(),
                            };
                            assert_eq!(page, launch, "{system} apps {installed:?} core {core_there} system {system_choice:?} game {game_choice:?}");
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 4 * 3 * 2 * 4 * 5);
    }

    /// A card's folders are entries of the shelf as internal storage's are, with their real names (review of 12.11.2026, #3):
    /// one entry per ROMs folder that has the system.
    #[test]
    fn each_roms_folder_s_system_folders_are_listed_with_their_real_names() {
        let dir = crate::testutil::TestDir::new("es-systems-roots");
        for d in ["a/ROMs/GBA", "b/roms/gba", "b/roms/PS2"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let cfg = es_systems(&[dir.join("a/ROMs"), dir.join("b/roms")]);
        for path in ["a/ROMs/GBA", "b/roms/gba", "b/roms/PS2"] {
            assert!(cfg.contains(&format!("<path>{}</path>", dir.join(path).display())), "{path}: {cfg}");
        }
        assert!(!cfg.contains(&format!("<path>{}</path>", dir.join("a/ROMs/ps2").display())), "PS2 is on the card only");
    }

    #[test]
    fn the_library_lists_our_systems_and_the_ones_only_an_app_plays_where_the_folders_are() {
        let dir = crate::testutil::TestDir::new("es-systems");
        std::fs::create_dir_all(dir.join("gc")).unwrap();
        let cfg = es_systems(&[dir.to_path_buf()]);
        let root = dir.display().to_string();
        let systems: Vec<(String, String)> = roxmltree::Document::parse(&cfg).unwrap().descendants().filter(|n| n.has_tag_name("system"))
            .map(|n| (n.children().find(|c| c.has_tag_name("name")).and_then(|c| c.text()).unwrap().to_string(), n.children().find(|c| c.has_tag_name("path")).and_then(|c| c.text()).unwrap().to_string())).collect();
        let at = |name: &str| systems.iter().find(|(n, _)| n == name).map(|(_, p)| p.as_str());
        // A system of our player is there, with the folder under the games folder.
        assert_eq!(at("gba"), Some(format!("{root}/gba").as_str()));
        assert_eq!(at("psp"), Some(format!("{root}/psp").as_str()));
        // The ones only an app plays are always there; GameCube takes the folder the library has (`gc`).
        assert_eq!(at("ps2"), Some(format!("{root}/ps2").as_str()));
        assert_eq!(at("gamecube"), Some(format!("{root}/gc").as_str()));
        assert!(at("wii").is_some() && at("3ds").is_some() && at("xbox").is_some());
        // Nothing of ROCKNIX's that no player of ours or app has a way for (ports, arcade boards), and no Switch, ever.
        for none in ["ports", "cps2", "pc", "scummvm", "switch", "wiiu"] {
            assert!(at(none).is_none(), "{none}");
        }
        assert!(!cfg.contains("/storage/roms"));
    }
}
