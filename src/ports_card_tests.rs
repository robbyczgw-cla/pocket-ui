use super::*;
use crate::testutil::TestDir;
use std::sync::Mutex;

fn stamp() -> crate::screenshot::Stamp {
    crate::screenshot::Stamp { year: 2026, month: 11, day: 23, hour: 12, min: 0, sec: 0 }
}

/// A zip as PortMaster's are (deflated), with `comment` as its comment (a runtime package's architecture).
fn zip(files: &[(&str, &[u8])], comment: &str) -> Vec<u8> {
    let mut z = crate::report::zip(&files.iter().map(|(n, d)| (n.to_string(), d.to_vec())).collect::<Vec<_>>(), &stamp());
    let n = z.len();
    z[n - 2..].copy_from_slice(&(comment.len() as u16).to_le_bytes());
    z.extend_from_slice(comment.as_bytes());
    z
}

/// A game's zip as PortMaster-New builds them: the script at the top, the folder with port.json beside it.
fn game(title: &str, folder: &str, arch: &[&str], runtime: serde_json::Value) -> Vec<u8> {
    let script = format!("{title}.sh");
    let json = serde_json::json!({
        "version": 4, "name": format!("{folder}.zip"), "items": [script, folder],
        "attr": { "title": title, "rtr": true, "arch": arch, "runtime": runtime, "reqs": [] }
    });
    zip(&[(&script, b"#!/bin/bash\n"), (&format!("{folder}/port.json"), json.to_string().as_bytes()), (&format!("{folder}/game.dat"), &[7u8; 5000])], "")
}

fn squashfs() -> Vec<u8> {
    let mut v = b"hsqs".to_vec();
    v.extend_from_slice(&[1u8; 4000]);
    v
}

struct Card {
    _dir: TestDir,
    drop: PathBuf,
    libs: PathBuf,
    ports: PathBuf,
}

fn card(label: &str) -> Card {
    let dir = TestDir::new(label);
    let drop = dir.join("roms/ports/autoinstall");
    let libs = dir.join("roms/ports/PortMaster/libs");
    std::fs::create_dir_all(&drop).unwrap();
    std::fs::create_dir_all(&libs).unwrap();
    let ports = dir.join("roms/ports");
    Card { drop, libs, ports, _dir: dir }
}

fn find<'a>(items: &'a [Item], file: &str) -> &'a Item {
    items.iter().find(|i| i.file_name() == file).unwrap_or_else(|| panic!("{file} not found in {items:?}"))
}

#[test]
fn the_folders_are_portmasters_autoinstall_in_every_place_the_card_update_looks_each_once() {
    let dir = TestDir::new("ports-card-dirs");
    let (flash, games, ext, pm) = (dir.join("flash"), dir.join("roms"), dir.join("games-external"), dir.join("roms/ports/PortMaster"));
    for d in [flash.join(FOLDER), games.join(FOLDER), ext.join("roms").join(FOLDER), pm.join("autoinstall")] {
        std::fs::create_dir_all(d).unwrap();
    }
    // A place bound into another (the games card shown again under roms) is the same folder: once.
    std::os::unix::fs::symlink(games.clone(), dir.join("again")).unwrap();
    let found = dirs_in(&[flash.clone(), ext.clone(), games.clone(), dir.join("again"), dir.join("missing")], &pm);
    assert_eq!(found, vec![flash.join(FOLDER), ext.join("roms").join(FOLDER), games.join(FOLDER), pm.join("autoinstall")]);
}

#[test]
fn a_game_is_ready_only_when_its_runtime_is_installed_or_on_the_card_too() {
    let c = card("ports-card-runtime");
    std::fs::write(c.drop.join("celeste.zip"), game("Celeste Classic", "celeste", &["aarch64"], serde_json::json!([]))).unwrap();
    std::fs::write(c.drop.join("dome.zip"), game("Dome Romantik", "dome", &["aarch64"], serde_json::json!(["frt_3.5.2"]))).unwrap();
    std::fs::write(c.drop.join("stardew.zip"), game("Stardew Valley", "stardew", &["aarch64"], serde_json::json!("mono-6.12.0.122-aarch64.squashfs"))).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    assert_eq!(find(&items, "celeste.zip").problem, None);
    assert_eq!(find(&items, "dome.zip").problem, Some(Problem::NeedsRuntime(vec!["frt_3.5.2.squashfs".into()])));
    assert_eq!(find(&items, "stardew.zip").problem, Some(Problem::NeedsRuntime(vec!["mono-6.12.0.122-aarch64.squashfs".into()])));
    assert!(find(&items, "dome.zip").detail().contains("frt_3.5.2.squashfs"), "the page names the file to get");
    // Installed already, or lying beside it on the card: ready.
    std::fs::write(c.libs.join("frt_3.5.2.squashfs"), squashfs()).unwrap();
    std::fs::write(c.drop.join("mono-6.12.0.122-aarch64.squashfs"), squashfs()).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    assert!(items.iter().all(Item::ready), "{items:?}");
    assert_eq!(items.iter().map(Item::title).collect::<Vec<_>>(), ["Celeste Classic", "Dome Romantik", "Stardew Valley", "mono-6.12.0.122-aarch64.squashfs"], "games first, by title");
}

#[test]
fn files_for_other_handhelds_damaged_files_and_other_zips_are_named_not_installed() {
    let c = card("ports-card-problems");
    std::fs::write(c.drop.join("pcgame.zip"), game("PC Game", "pcgame", &["x86_64"], serde_json::json!([]))).unwrap();
    let whole = game("Cut Off", "cutoff", &["aarch64"], serde_json::json!([]));
    std::fs::write(c.drop.join("cutoff.zip"), &whole[..whole.len() / 2]).unwrap();
    std::fs::write(c.drop.join("PortMaster.zip"), zip(&[("PortMaster/harbourmaster", b"#!/usr/bin/env python3")], "")).unwrap();
    std::fs::write(c.drop.join("frt_3.5.2.x86_64.squashfs"), squashfs()).unwrap();
    std::fs::write(c.drop.join("half.squashfs"), b"not a squashfs").unwrap();
    std::fs::write(c.drop.join("runtimes.full.x86_64.zip"), zip(&[("godot_4.3.squashfs", &squashfs())], "x86_64")).unwrap();
    std::fs::write(c.drop.join("All Runtimes.zip"), zip(&[("godot_4.3.squashfs", &squashfs()), ("godot_4.3.squashfs.md5", b"x")], "aarch64")).unwrap();
    std::fs::write(c.drop.join("notes.txt"), b"left alone").unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    assert_eq!(items.len(), 7, "{items:?}");
    assert_eq!(find(&items, "pcgame.zip").problem, Some(Problem::OtherHandheld));
    assert_eq!(find(&items, "cutoff.zip").problem, Some(Problem::Damaged));
    assert_eq!(find(&items, "PortMaster.zip").problem, Some(Problem::NotAGame));
    assert_eq!(find(&items, "frt_3.5.2.x86_64.squashfs").problem, Some(Problem::OtherHandheld));
    assert_eq!(find(&items, "half.squashfs").problem, Some(Problem::Damaged));
    assert_eq!(find(&items, "runtimes.full.x86_64.zip").problem, Some(Problem::OtherHandheld));
    let package = find(&items, "All Runtimes.zip");
    assert_eq!((&package.kind, &package.problem), (&Kind::Runtimes(vec!["godot_4.3.squashfs".into()]), &None));
    for item in &items {
        assert!(!item.label().is_empty() && !item.detail().is_empty(), "{item:?} says what it is");
    }
}

#[test]
fn a_game_that_needs_a_runtime_in_a_package_on_the_card_is_ready() {
    let c = card("ports-card-package");
    std::fs::write(c.drop.join("godotgame.zip"), game("Godot Game", "godotgame", &["aarch64"], serde_json::json!(["godot_4.3"]))).unwrap();
    std::fs::write(c.drop.join("runtimes.popular.aarch64.zip"), zip(&[("godot_4.3.squashfs", &squashfs())], "aarch64")).unwrap();
    assert!(scan(std::slice::from_ref(&c.drop), &c.libs).iter().all(Item::ready));
}

/// Stand-ins for PortMaster and the card, recording what was asked of them.
struct Fake {
    calls: Mutex<Vec<String>>,
    /// PortMaster's folder, set up the way `deploy/portmaster-setup.sh` does: without its `pugwash` the whole folder is
    /// replaced. harbourmaster's every run starts with that (`ports::ensure_ready`), as `prepare` does.
    pm: Option<PathBuf>,
    free: u64,
    ports: PathBuf,
    /// harbourmaster answers 0 but puts nothing in place.
    silent: bool,
}

impl Fake {
    fn setup(&self) -> Result<(), String> {
        if let Some(pm) = &self.pm {
            if !pm.join("pugwash").exists() {
                self.calls.lock().unwrap().push("setup".into());
                let _ = std::fs::remove_dir_all(pm);
                std::fs::create_dir_all(pm).unwrap();
                std::fs::write(pm.join("pugwash"), b"").unwrap();
            }
        }
        Ok(())
    }

    fn run(&self, items: &[Item], libs: &Path) -> Vec<Outcome> {
        let prepare = || self.setup();
        let game = |p: &Path| {
            self.setup()?;
            self.calls.lock().unwrap().push(format!("game {}", p.file_name().unwrap().to_string_lossy()));
            if !self.silent {
                let stem = p.file_stem().unwrap().to_string_lossy().to_string();
                let title = match stem.as_str() { "celeste" => "Celeste Classic", "dome" => "Dome Romantik", "turbotabletennis" => "TurboTableTennis", other => other }.to_string();
                std::fs::write(self.ports.join(format!("{title}.sh")), b"").unwrap();
            }
            Ok(())
        };
        let package = |z: &Path, to: &Path| {
            self.calls.lock().unwrap().push(format!("package {} -> {}", z.file_name().unwrap().to_string_lossy(), to.file_name().unwrap().to_string_lossy()));
            // Its files land side by side, as `unzip -j` puts them.
            for (name, _) in crate::archive::listing(z, |_| false).unwrap().unwrap().entries {
                std::fs::write(to.join(name.rsplit('/').next().unwrap()), b"").unwrap();
            }
            Ok(())
        };
        let free = |_: &Path| Some(self.free);
        let remove = |p: &Path| std::fs::remove_file(p).is_ok();
        let progress = |n: usize, total: usize, what: &str| self.calls.lock().unwrap().push(format!("{n}/{total} {what}"));
        install(items, &Installer { prepare: &prepare, libs: libs.to_path_buf(), ports: self.ports.clone(), game: &game, package: &package, free: &free, remove: &remove }, &progress)
    }
}

#[test]
fn installing_puts_runtimes_first_then_the_games_and_takes_only_installed_files_off_the_card() {
    let c = card("ports-card-install");
    std::fs::write(c.drop.join("dome.zip"), game("Dome Romantik", "dome", &["aarch64"], serde_json::json!(["frt_3.5.2"]))).unwrap();
    std::fs::write(c.drop.join("frt_3.5.2.squashfs"), squashfs()).unwrap();
    std::fs::write(c.drop.join("celeste.zip"), game("Celeste Classic", "celeste", &["aarch64"], serde_json::json!([]))).unwrap();
    std::fs::write(c.drop.join("pcgame.zip"), game("PC Game", "pcgame", &["x86_64"], serde_json::json!([]))).unwrap();
    std::fs::write(c.drop.join("runtimes.popular.aarch64.zip"), zip(&[("godot_4.3.squashfs", &squashfs())], "aarch64")).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    let fake = Fake { calls: Mutex::new(vec![]), pm: None, free: 1 << 40, ports: c.ports.clone(), silent: false };
    let outcomes = fake.run(&items, &c.libs);
    assert_eq!(*fake.calls.lock().unwrap(), [
        "0/4 frt_3.5.2.squashfs", "1/4 runtimes.popular.aarch64.zip", "package runtimes.popular.aarch64.zip -> libs",
        "2/4 Celeste Classic", "game celeste.zip", "3/4 Dome Romantik", "game dome.zip", "4/4 ",
    ]);
    assert!(outcomes.iter().all(|o| o.result.is_ok()), "{outcomes:?}");
    assert_eq!(std::fs::read(c.libs.join("frt_3.5.2.squashfs")).unwrap(), squashfs(), "the runtime is in PortMaster's folder");
    assert!(!c.libs.join(".frt_3.5.2.squashfs.part").exists());
    let left: Vec<String> = std::fs::read_dir(&c.drop).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, ["pcgame.zip"], "the file that wasn't installed stays");
    assert_eq!(summary(&outcomes), ("2 games installed. Find them under Ports.".to_string(), false));
}

#[test]
fn a_game_portmaster_did_not_put_in_place_or_with_no_room_is_an_error_and_stays_on_the_card() {
    let c = card("ports-card-fail");
    std::fs::write(c.drop.join("celeste.zip"), game("Celeste Classic", "celeste", &["aarch64"], serde_json::json!([]))).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    let silent = Fake { calls: Mutex::new(vec![]), pm: None, free: 1 << 40, ports: c.ports.clone(), silent: true };
    let outcomes = silent.run(&items, &c.libs);
    assert_eq!(outcomes[0].result, Err("PortMaster finished, but the game is missing.".into()));
    assert!(c.drop.join("celeste.zip").exists());
    assert_eq!(summary(&outcomes), ("Celeste Classic wasn't installed. PortMaster finished, but the game is missing.".to_string(), true));

    let full = Fake { calls: Mutex::new(vec![]), pm: None, free: 1 << 20, ports: c.ports.clone(), silent: false };
    let outcomes = full.run(&items, &c.libs);
    let e = outcomes[0].result.clone().unwrap_err();
    assert!(e.starts_with("Not enough space on the SD card: 1 MiB free"), "{e}");
    assert!(!full.calls.lock().unwrap().iter().any(|c| c.starts_with("game ")), "nothing is unpacked without room");
    assert!(c.drop.join("celeste.zip").exists());
}

#[test]
fn the_summary_names_one_game_runtimes_alone_and_several_failures() {
    let ok = |t: &str, game: bool| Outcome { title: t.into(), game, result: Ok(()) };
    let bad = |t: &str| Outcome { title: t.into(), game: true, result: Err("Damaged.".into()) };
    assert_eq!(summary(&[ok("frt", false), ok("Celeste Classic", true)]).0, "Celeste Classic is installed. Find it under Ports.");
    assert_eq!(summary(&[ok("frt", false)]).0, "Runtimes installed.");
    assert_eq!(summary(&[bad("A"), bad("B")]), ("2 files weren't installed. Damaged.".to_string(), true));
}

/// Reader rule (scripts/reader-inventory.py): every file the card page reads, missing, empty, cut off, with flipped
/// bytes or in another encoding, is named as a problem or left out, never installed and never a panic.
#[test]
fn file_source_rule_card_ports() {
    let c = card("ports-card-faults");
    // Missing: folders that aren't there are no folders to read.
    assert!(scan(&[c.drop.join("missing")], &c.libs).is_empty());
    let good = game("Celeste Classic", "celeste", &["aarch64"], serde_json::json!([]));
    std::fs::write(c.drop.join("empty.zip"), b"").unwrap();
    std::fs::write(c.drop.join("empty.squashfs"), b"").unwrap();
    std::fs::write(c.drop.join("cut.zip"), &good[..good.len() - 30]).unwrap();
    std::fs::write(c.drop.join("cut.squashfs"), b"hs").unwrap();
    // Flipped bytes inside the deflated port.json: its CRC no longer matches.
    let mut flipped = good.clone();
    let at = flipped.windows(9).position(|w| w == b"port.json").unwrap() + 9 + 40;
    flipped[at] ^= 0xff;
    std::fs::write(c.drop.join("flipped.zip"), &flipped).unwrap();
    // Another encoding: a port.json in UTF-16 (with its BOM) is no JSON this reader takes.
    let utf16: Vec<u8> = [0xff, 0xfe].into_iter().chain("{\"attr\":{\"title\":\"X\"}}".encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
    std::fs::write(c.drop.join("utf16.zip"), zip(&[("X.sh", b""), ("x/port.json", &utf16)], "")).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    for (file, problem) in [("empty.zip", Problem::Damaged), ("empty.squashfs", Problem::Damaged), ("cut.zip", Problem::Damaged),
        ("cut.squashfs", Problem::Damaged), ("flipped.zip", Problem::Damaged), ("utf16.zip", Problem::Damaged)] {
        assert_eq!(find(&items, file).problem, Some(problem), "{file}");
    }
    assert!(items.iter().all(|i| !i.ready()));
    let fake = Fake { calls: Mutex::new(vec![]), pm: None, free: 1 << 40, ports: c.ports.clone(), silent: false };
    assert!(fake.run(&items, &c.libs).is_empty(), "nothing damaged is installed");
    assert_eq!(std::fs::read_dir(&c.drop).unwrap().count(), 6, "and all of it stays on the card");
}

/// R36S test.7 (23.11.2026): TurboTableTennis and frt_3.5.2.squashfs installed from the card on a handheld whose
/// PortMaster wasn't set up yet: "2 games installed", but PortMaster's libs folder was empty and the game ended after
/// 7 s. The runtime was copied into a new PortMaster/libs, then the first game's install set PortMaster up, and its setup
/// replaces a PortMaster folder without PortMaster in it. Missed because the stand-in PortMaster had no setup at all.
#[test]
fn portmaster_is_set_up_before_the_runtimes_go_into_it() {
    let c = card("ports-card-setup-first");
    let pm = c.libs.parent().unwrap().to_path_buf();
    std::fs::remove_dir_all(&pm).unwrap();
    std::fs::write(c.drop.join("turbotabletennis.zip"), game("TurboTableTennis", "turbotabletennis", &[], serde_json::json!("frt_3.5.2.squashfs"))).unwrap();
    std::fs::write(c.drop.join("frt_3.5.2.squashfs"), squashfs()).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    assert!(items.iter().all(Item::ready), "{items:?}");
    let fake = Fake { calls: Mutex::new(vec![]), pm: Some(pm.clone()), free: 1 << 40, ports: c.ports.clone(), silent: false };
    let outcomes = fake.run(&items, &c.libs);
    assert_eq!(std::fs::read(c.libs.join("frt_3.5.2.squashfs")).ok(), Some(squashfs()), "the runtime is in PortMaster after it all: {:?}", fake.calls.lock().unwrap());
    assert!(outcomes.iter().all(|o| o.result.is_ok()), "{outcomes:?}");
    assert_eq!(fake.calls.lock().unwrap().iter().filter(|c| *c == "setup").count(), 1);
}

#[test]
fn a_runtime_that_is_gone_from_portmaster_after_the_install_is_said_and_stays_on_the_card() {
    let c = card("ports-card-runtime-gone");
    std::fs::write(c.drop.join("frt_3.5.2.squashfs"), squashfs()).unwrap();
    std::fs::write(c.drop.join("celeste.zip"), game("Celeste Classic", "celeste", &["aarch64"], serde_json::json!([]))).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    // Something empties PortMaster's runtime folder while the games install.
    let libs = c.libs.clone();
    let game = |_: &Path| { let _ = std::fs::remove_dir_all(&libs); Ok(()) };
    let (prepare, package, free) = (|| Ok(()), |_: &Path, _: &Path| Ok(()), |_: &Path| None);
    let remove = |p: &Path| std::fs::remove_file(p).is_ok();
    let how = Installer { prepare: &prepare, libs: c.libs.clone(), ports: c.ports.clone(), game: &game, package: &package, free: &free, remove: &remove };
    let outcomes = install(&items, &how, &|_, _, _| {});
    let runtime = outcomes.iter().find(|o| !o.game).unwrap();
    assert_eq!(runtime.result, Err(NOT_IN_PORTMASTER.to_string()));
    assert!(c.drop.join("frt_3.5.2.squashfs").exists(), "kept on the card for the next try");
    assert!(summary(&outcomes).1);
}

#[test]
fn when_portmaster_cannot_be_set_up_nothing_is_installed_or_taken_off_the_card() {
    let c = card("ports-card-setup-fails");
    std::fs::write(c.drop.join("frt_3.5.2.squashfs"), squashfs()).unwrap();
    let items = scan(std::slice::from_ref(&c.drop), &c.libs);
    let prepare = || Err("Couldn't prepare PortMaster.".to_string());
    let (game, package, free) = (|_: &Path| Ok(()), |_: &Path, _: &Path| Ok(()), |_: &Path| None);
    let remove = |p: &Path| std::fs::remove_file(p).is_ok();
    let how = Installer { prepare: &prepare, libs: c.libs.clone(), ports: c.ports.clone(), game: &game, package: &package, free: &free, remove: &remove };
    let outcomes = install(&items, &how, &|_, _, _| {});
    assert_eq!(outcomes[0].result, Err("Couldn't prepare PortMaster.".to_string()));
    assert!(!c.libs.join("frt_3.5.2.squashfs").exists() && c.drop.join("frt_3.5.2.squashfs").exists());
}
