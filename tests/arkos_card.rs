//! An ArkOS / dArkOS / R36S stock card (EASYROMS, or a second card switched to with "Switch to SD2 for Roms") read by
//! ROCKNIX's systems, as the R36S card shows it (docs/OTHER-LAYOUTS.md, "ArkOS cards"): every system folder ArkOS makes,
//! by its ArkOS name, is a ROCKNIX system's folder or read as one; the folders of systems ROCKNIX has no emulator for stay
//! unread. The lists are the real ones: `testdata/arkos/arkos-rk3326-folders.tsv` from dArkOS's
//! `Emulationstation/es_systems.cfg.rk3326` (folder, name, full name) and `rocknix-rk3326-systems.tsv` from ROCKNIX
//! 20261101's RK3326 image (`usr/config/emulationstation/es_systems.cfg`: id, folder).

use pocket_ui::library;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ARKOS: &str = include_str!("../testdata/arkos/arkos-rk3326-folders.tsv");
const ROCKNIX: &str = include_str!("../testdata/arkos/rocknix-rk3326-systems.tsv");

/// ArkOS folders whose name is not a ROCKNIX folder, and the ROCKNIX system that plays them.
const MAPPED: &[(&str, &str)] = &[
    // Read as PC Engine: the same games and emulator (`layouts::ALIASES` has said so since the other layouts).
    ("turbografx", "pcengine"),
    ("turbografxcd", "pcenginecd"),
    ("pokemonmini", "pokemini"),
    ("neogeocd", "neocd"),
    ("tic80", "tic-80"),
    ("dos", "pc"),
    ("carts", "pico-8"),
    ("vmac", "macintosh"),
    ("thomson", "moto"),
    ("mame2003", "mame"),
    ("atarixegs", "atari800"),
    ("snes-hacks", "snes"),
    ("sgb", "gb"),
    ("msumd", "megadrive"),
];

/// ArkOS folders of systems ROCKNIX's RK3326 image has no emulator for (or no games: `movies`, `cmds`): left unread.
const UNREAD: &[&str] = &[
    "advision", "apple2", "cavestory", "gx4000", "astrocde", "alg", "love2d", "vmu", "pico", "ti99", "wolf", "mv",
    "onscripter", "piece", "puzzlescript", "enterprise", "tvc", "dragon32", "coco3", "gametank", "lowresnx", "solarus",
    "zeebo", "movies", "cmds",
];

fn rows(text: &str) -> Vec<Vec<&str>> {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| l.split('\t').collect()).collect()
}

/// ROCKNIX's es_systems.cfg with its systems' folders in `roms`.
fn rocknix_cfg(dir: &Path, roms: &Path) -> PathBuf {
    let mut xml = String::from("<systemList>");
    for r in rows(ROCKNIX) {
        xml += &format!("<system><name>{}</name><fullname>{}</fullname><path>{}</path><extension>.zip .7z .bin</extension><command>true</command></system>", r[0], r[0], roms.join(r[1]).display());
    }
    xml += "</systemList>";
    let cfg = dir.join("es_systems.cfg");
    std::fs::write(&cfg, xml).unwrap();
    cfg
}

#[test]
fn every_arkos_folder_rocknix_can_play_is_read_and_the_others_stay_unread() {
    let tmp = tempfile::tempdir().unwrap();
    let roms = tmp.path().join("roms");
    // The card: every ArkOS folder with a game in it, and ArkOS's bios folder.
    for r in rows(ARKOS) {
        std::fs::create_dir_all(roms.join(r[0])).unwrap();
        std::fs::write(roms.join(r[0]).join("Game.zip"), b"rom").unwrap();
    }
    std::fs::create_dir_all(roms.join("bios")).unwrap();
    std::fs::write(roms.join("bios/scph1001.bin"), b"bios").unwrap();
    let cfg = rocknix_cfg(tmp.path(), &roms);

    let rocknix: BTreeMap<String, String> = rows(ROCKNIX).into_iter().map(|r| (r[1].to_string(), r[0].to_string())).collect();
    let sources = library::read_sources(&cfg);
    let read: BTreeMap<String, String> = sources.iter().map(|(id, p)| (p.file_name().unwrap().to_string_lossy().into_owned(), id.clone())).collect();

    let mut wrong = Vec::new();
    for r in rows(ARKOS) {
        let folder = r[0];
        // A ROCKNIX folder, a ROCKNIX id (`supergrafx`, whose folder is `sgfx`), or one of `MAPPED`.
        let by_id = rocknix.values().find(|id| id.as_str() == folder).cloned();
        let want = rocknix.get(folder).cloned().or(by_id).or_else(|| MAPPED.iter().find(|(a, _)| *a == folder).map(|(_, id)| id.to_string()));
        let got = read.get(folder).cloned();
        if want != got {
            wrong.push(format!("{folder} ({}): want {want:?}, read as {got:?}", r[2]));
        }
        assert_eq!(want.is_none(), UNREAD.contains(&folder), "{folder}: in UNREAD exactly when ROCKNIX has no system for it");
    }
    assert!(wrong.is_empty(), "ArkOS folders read wrongly:\n{}", wrong.join("\n"));
    assert!(!read.contains_key("bios"), "the bios folder is no system");
    // Every MAPPED target is a system of ROCKNIX's RK3326 image.
    for (_, id) in MAPPED {
        assert!(rocknix.values().any(|v| v == id), "{id} is no ROCKNIX RK3326 system");
    }
}
