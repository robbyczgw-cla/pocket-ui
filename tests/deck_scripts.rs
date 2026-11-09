//! The Steam Deck packaging scripts as a player meets them (`deploy/steamdeck/`, docs/STEAMDECK.md): the library
//! is found in a RetroDECK folder (and an EmuDeck one), an empty home says so instead of writing nonsense, the
//! shipped Steam Input layout is a readable config with the back buttons on it, and every shell script parses.
//! Needs python3 and sh.

use std::path::Path;
use std::process::Command;

include!("common/root.rs");

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn gen(home: &Path, out: &Path) -> (bool, String) {
    let run = Command::new("python3")
        .args(["-I", &format!("{ROOT}/deploy/steamdeck/gen-es-systems.py"), out.to_str().unwrap()])
        .env("HOME", home)
        .env_remove("POCKETUI_ROMS")
        .output()
        .expect("python3 for gen-es-systems.py");
    let cfg = std::fs::read_to_string(out).unwrap_or_default();
    (run.status.success(), cfg)
}

#[test]
fn a_retrodeck_library_is_found_with_dreamcast_and_the_folder_names_translated() {
    let guard = tempfile::tempdir().unwrap();
    let home = guard.path();
    write(&home.join("retrodeck/roms/gba/Fake Game.gba"), b"x");
    write(&home.join("retrodeck/roms/dreamcast/Fake Game.gdi"), b"x");
    write(&home.join("retrodeck/roms/genesis/Fake Game.md"), b"x");
    write(&home.join("retrodeck/roms/gc/Fake Game.iso"), b"x");
    let out = home.join("es_systems.cfg");
    let (ok, cfg) = gen(home, &out);
    assert!(ok, "gen-es-systems.py on a RetroDECK home");
    for want in ["<name>gba</name>", "<name>dreamcast</name>", "flycast", "<name>megadrive</name>", "genesis_plus_gx", "<name>gamecube</name>", "org.DolphinEmu.dolphin-emu", "retrodeck/roms"] {
        assert!(cfg.contains(want), "{want} in the written library:\n{cfg}");
    }
}

#[test]
fn an_emudeck_library_still_wins_and_an_empty_home_fails_loudly() {
    let guard = tempfile::tempdir().unwrap();
    let home = guard.path();
    let out = home.join("es_systems.cfg");
    let (ok, _) = gen(home, &out);
    assert!(!ok, "no ROM folder anywhere: the launcher writes its empty shelf instead");
    write(&home.join("Emulation/roms/snes/Fake Game.sfc"), b"x");
    write(&home.join("retrodeck/roms/gba/Fake Game.gba"), b"x");
    let (ok, cfg) = gen(home, &out);
    assert!(ok);
    assert!(cfg.contains("<name>snes</name>") && cfg.contains("Emulation/roms"), "EmuDeck first, as before:\n{cfg}");
    assert!(!cfg.contains("retrodeck/roms"), "one library root at a time; empty system definitions let the shared reader find other card layouts:\n{cfg}");
}

#[test]
fn the_shipped_steam_input_layout_reads_as_a_config_with_the_back_buttons() {
    let vdf = std::fs::read_to_string(format!("{ROOT}/deploy/steamdeck/controller_neptune.vdf")).unwrap();
    // Balanced quotes and braces outside the comments: the least Steam will ask of the file.
    let mut depth = 0i32;
    for line in vdf.lines().filter(|l| !l.trim_start().starts_with("//")) {
        assert_eq!(line.matches('"').count() % 2, 0, "unbalanced quotes: {line}");
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "a closing brace too many: {line}");
        }
    }
    assert_eq!(depth, 0, "the config closes what it opens");
    for want in ["\"controller_mappings\"", "\"controller_neptune\"", "\"version\"\t\t\"3\"", "\"four_buttons\"", "\"joystick_move\"", "\"mouse\"", "\"scrollwheel\""] {
        assert!(vdf.contains(want), "{want} in the layout");
    }
    assert_eq!(vdf.matches("xinput_button GUIDE").count(), 2, "L5 and R5 are HOME");
    assert!(vdf.contains("button_grip_left_upper\"\t\t\"xinput_button LEFT_BUMPER"), "L4 repeats L1");
    assert!(vdf.contains("button_grip_right_upper\"\t\t\"xinput_button RIGHT_BUMPER"), "R4 repeats R1");
}

#[test]
fn every_deck_shell_script_parses() {
    for script in ["pocket-ui-deck.sh", "install.sh", "build-bundle.sh", "build-flatpak.sh", "fetch-cores.sh", "runemu.sh", "test-flatpak.sh"] {
        let run = Command::new("sh").args(["-n", &format!("{ROOT}/deploy/steamdeck/{script}")]).output().expect("sh");
        assert!(run.status.success(), "{script}: {}", String::from_utf8_lossy(&run.stderr));
    }
}

#[test]
fn the_deck_archive_uses_the_same_version_as_its_binaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let deck = root.join("deploy/steamdeck");
    std::fs::create_dir_all(&deck).unwrap();
    std::fs::copy(format!("{ROOT}/deploy/steamdeck/build-bundle.sh"), deck.join("build-bundle.sh")).unwrap();
    for name in ["pocket-ui-deck.sh", "runemu.sh", "gen-es-systems.py", "add-to-steam.py", "fetch-cores.sh", "install.sh", "controller_neptune.vdf"] {
        write(&deck.join(name), b"fixture");
    }
    write(&root.join("Cargo.toml"), b"version = \"0.2.0\"\n");
    write(&root.join("scripts/release-rustflags.sh"), b"true\n");
    write(&root.join("scripts/check-binary-paths.sh"), b"exit 0\n");
    let home = root.join("home");
    let cargo = home.join(".cargo/bin/cargo");
    write(&cargo, b"#!/bin/sh\nfor d in target/release target/x86_64-unknown-linux-gnu/release; do mkdir -p \"$d\"; printf binary > \"$d/pocket-ui\"; printf binary > \"$d/pocket-core\"; done\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new("sh").arg(deck.join("build-bundle.sh"))
        .current_dir(root).env("HOME", home).env("POCKETUI_VERSION", "0.3.1-deck-qa")
        .env_remove("CARGO_TARGET_DIR").env_remove("DECK_ZIGBUILD").env_remove("CORES_DIR")
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(root.join("target/pocket-ui-deck-0.3.1-deck-qa.tar.gz").exists(), "the archive must name the version compiled into the app");
}

#[test]
fn deck_games_find_the_players_retroarch_cheats_without_overriding_an_explicit_database() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let app = root.join("app");
    for name in ["pocket-ui-deck.sh", "gen-es-systems.py"] {
        std::fs::create_dir_all(&app).unwrap();
        std::fs::copy(format!("{ROOT}/deploy/steamdeck/{name}"), app.join(name)).unwrap();
    }
    let binary = app.join("pocket-ui");
    write(&binary, b"#!/bin/sh\nprintf '%s' \"${POCKET_CHEATS_DB:-}\" > \"$POCKETUI_DATA/cheats-probe\"\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = root.join("home");
    let database = home.join(".var/app/org.libretro.RetroArch/config/retroarch/cheats");
    write(&database.join("Nintendo - Game Boy/Fake.cht"), b"cheats = 1\n");
    let data = root.join("data");
    let mut launch = Command::new("sh");
    launch.arg(app.join("pocket-ui-deck.sh")).env("HOME", &home).env("POCKETUI_APP_DATA", &data).env_remove("POCKET_CHEATS_DB");
    assert!(launch.status().unwrap().success());
    assert_eq!(std::fs::read_to_string(data.join("cheats-probe")).unwrap(), database.to_str().unwrap(), "the installed database must reach both the menu and pocket-core");
    launch.env("POCKET_CHEATS_DB", root.join("my-database"));
    assert!(launch.status().unwrap().success());
    assert_eq!(std::fs::read_to_string(data.join("cheats-probe")).unwrap(), root.join("my-database").to_str().unwrap());
}

#[test]
fn a_foreign_card_folder_keeps_its_system_in_the_decks_catalog() {
    let guard = tempfile::tempdir().unwrap();
    let home = guard.path();
    write(&home.join("Emulation/roms/Game Boy Advance (GBA)/Fake.gba"), b"x");
    let (ok, cfg) = gen(home, &home.join("systems.cfg"));
    assert!(ok);
    assert!(cfg.contains("<name>gba</name>"), "the shared library reader needs a GBA definition before it can recognise this folder: {cfg}");
    assert!(cfg.contains("Emulation/roms/gba</path>"), "retain a canonical path so library::foreign_folders supplies the actual folder");
    assert!(home.join("Emulation/roms/Game Boy Advance (GBA)/Fake.gba").exists(), "the player's folder stays in place");
}
