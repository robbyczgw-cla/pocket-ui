//! An ArkOS card in the second slot, as the image's `games-card.sh` leaves it (docs/OTHER-LAYOUTS.md, "ArkOS cards"): mounted
//! as it is, its top named in the marker (here `POCKETUI_GAMES_CARD_ROOT`), the device's own `roms` beside it. Its games are
//! on the shelf under ROCKNIX's systems, its battery saves are the ones the case reads, ArkOS's ports stay off the shelf, and
//! reading it writes nothing on the card. A test binary of its own: it sets the variable for the whole process.

use pocket_ui::library;
use std::path::{Path, PathBuf};

fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// Every file under `dir` with its bytes and time: the card before and after.
fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.push((p.clone(), Vec::new(), e.metadata().unwrap().modified().unwrap()));
                stack.push(p);
            } else {
                out.push((p.clone(), std::fs::read(&p).unwrap(), e.metadata().unwrap().modified().unwrap()));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn an_arkos_cards_games_are_read_where_they_are_with_their_saves_and_nothing_is_written_on_it() {
    let tmp = tempfile::tempdir().unwrap();
    let roms = tmp.path().join("storage/roms");
    let card = tmp.path().join("games-external");
    // The device: ROCKNIX's folders, one game of its own.
    for id in ["gba", "snes", "psx", "pcengine", "ports", "bios"] {
        std::fs::create_dir_all(roms.join(id)).unwrap();
    }
    put(&roms.join("snes/Own Game.sfc"), b"rom");
    let mut xml = String::from("<systemList>");
    for (id, exts) in [("gba", ".gba .zip"), ("snes", ".sfc .smc .zip"), ("psx", ".cue .chd .m3u"), ("pcengine", ".pce .zip"), ("ports", ".sh")] {
        xml += &format!("<system><name>{id}</name><fullname>{id}</fullname><path>{}</path><extension>{exts}</extension><command>true</command></system>", roms.join(id).display());
    }
    xml += "</systemList>";
    let cfg = tmp.path().join("es_systems.cfg");
    std::fs::write(&cfg, xml).unwrap();
    // The ArkOS card (EASYROMS): folders at its top, saves beside the games, ArkOS's BIOS folder, ports and other folders.
    put(&card.join("gba/Golden Sun (USA).gba"), b"rom");
    put(&card.join("gba/Golden Sun (USA).srm"), b"save of golden sun");
    put(&card.join("gba/Golden Sun (USA).state"), b"retroarch state");
    put(&card.join("snes/Super Metroid (Japan, USA).sfc"), b"rom");
    put(&card.join("psx/Final Fantasy VII (Disc 1).chd"), b"disc");
    put(&card.join("turbografx/Bonk's Adventure (USA).pce"), b"rom");
    put(&card.join("ports/Cave Story.sh"), b"#!/bin/bash\\n/roms/ports/cavestory/run");
    put(&card.join("bios/scph1001.bin"), b"bios");
    put(&card.join("themes/es-theme-nes-box/theme.xml"), b"<theme/>");
    let before = tree(&card);

    std::env::set_var("POCKETUI_GAMES_CARD_ROOT", &card);
    assert_eq!(library::games_card_root().as_deref(), Some(card.as_path()));
    let systems = library::load(&cfg).unwrap();
    let games = |id: &str| -> Vec<PathBuf> { systems.iter().find(|s| s.name == id).map(|s| s.games.iter().map(|g| g.path.clone()).collect()).unwrap_or_default() };

    assert!(games("gba").contains(&card.join("gba/Golden Sun (USA).gba")), "{:?}", games("gba"));
    assert!(games("snes").contains(&card.join("snes/Super Metroid (Japan, USA).sfc")) && games("snes").contains(&roms.join("snes/Own Game.sfc")), "the card's games beside the device's own: {:?}", games("snes"));
    assert!(games("psx").contains(&card.join("psx/Final Fantasy VII (Disc 1).chd")));
    assert!(games("pcengine").contains(&card.join("turbografx/Bonk's Adventure (USA).pce")), "ArkOS's turbografx is PC Engine");
    assert!(!games("ports").iter().any(|p| p.starts_with(&card)), "ArkOS's port scripts stay off the shelf: {:?}", games("ports"));
    assert!(systems.iter().all(|s| s.games.iter().all(|g| !g.path.starts_with(card.join("bios")) && !g.path.starts_with(card.join("themes")))));

    // The battery save beside the game is the one the case reads (and pocket-core loads: it reads `<rom dir>/<stem>.srm`).
    let rom = card.join("gba/Golden Sun (USA).gba");
    let (save, _) = pocket_ui::case_knows_read::save_of("gba", &rom).expect("the card's save is found");
    assert_eq!(save, card.join("gba/Golden Sun (USA).srm"));

    // Reading it changed nothing on the card.
    assert_eq!(tree(&card), before, "nothing on the card was written, moved or touched");

    // The card out (its folder gone): no card, nothing read from it.
    std::env::set_var("POCKETUI_GAMES_CARD_ROOT", tmp.path().join("gone"));
    assert_eq!(library::games_card_root(), None);
    assert!(library::load(&cfg).unwrap().iter().all(|s| s.games.iter().all(|g| !g.path.starts_with(&card))));
}
