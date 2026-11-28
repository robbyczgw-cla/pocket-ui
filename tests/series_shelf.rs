//! Settings › Look › Layout › Order: By series (`app_series`, `pocket_ui::series_shelf`, docs/SERIES-SHELF.md): Home's
//! systems make way for one shelf per series, every console's games of it in release order, and One of a kind for the
//! games of no series; Easy keeps its own order; an empty card still shows the way to games, not an empty Home.
//!
//! On the made-up library of tests/common/shelf_library.rs (40 Super Nintendo games, 36 of them "Made-up Quest NN", and
//! games of six more systems: Super Mario 64, Ocarina of Time, Mario Kart on three consoles, Metroid on three…).
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

/// (name, alone, games as "system/name") per shelf, and Home's rows, from the snapshot's report.
fn report(stdout: &str) -> (Vec<(String, bool, Vec<String>)>, String) {
    let shelves = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("series-shelf "))
        .map(|l| {
            let mut p = l.splitn(3, '|');
            let name = p.next().unwrap().to_string();
            let alone = p.next() == Some("true");
            let games = p.next().unwrap_or("").split(';').filter(|g| !g.is_empty()).map(str::to_string).collect();
            (name, alone, games)
        })
        .collect();
    let rows = stdout.lines().find_map(|l| l.strip_prefix("home-rows ")).unwrap_or_default().to_string();
    (shelves, rows)
}

#[test]
fn home_by_series_has_a_shelf_per_series_from_every_console_in_release_order_and_one_of_a_kind_last() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for look in ["split", "shelf", "cabinet"] {
        let (out, (w, _, px)) = run_in(tmp.path(), &stub, None, look, "1280x720", &format!("series:{look}"));
        assert!(w > 0 && px.iter().any(|&b| b > 0), "{look}: a picture");
        let (shelves, rows) = report(&out);
        assert!(rows.starts_with("systems=0 "), "{look}: no system rows by series: {rows}");
        let series_rows: usize = rows.split_whitespace().find_map(|p| p.strip_prefix("series=")).unwrap().parse().unwrap();
        assert_eq!(series_rows, shelves.len(), "{look}: a row per shelf");
        let find = |name: &str| shelves.iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("{look}: {name} in {shelves:?}"));
        // Three consoles' Mario Kart on one shelf, the SNES one... none here: N64 (1996), GBA (2001), DS (2005).
        let kart = &find("Mario Kart").2;
        assert_eq!(kart.iter().map(|g| g.split('/').next().unwrap()).collect::<Vec<_>>(), ["n64", "gba", "nds"], "{look}: release order: {kart:?}");
        // Zelda: The Ocarina of Time (N64, 1998) before The Wind Waker (GameCube, 2002).
        let zelda = &find("The Legend of Zelda").2;
        assert!(zelda[0].starts_with("n64/") && zelda[1].starts_with("gc/"), "{look}: {zelda:?}");
        // A series of the titles alone (no data, no card): the 36 Made-up Quests.
        assert_eq!(find("Made-up Quest").2.len(), 36, "{look}");
        // Games of no series stand together, last.
        let last = shelves.last().unwrap();
        assert!(last.1 && last.0 == "One of a kind", "{look}: {last:?}");
        assert!(last.2.iter().any(|g| g.contains("EarthBound")), "{look}: {last:?}");
        // Every game of the card on exactly one shelf.
        let all: usize = shelves.iter().map(|s| s.2.len()).sum();
        assert_eq!(all, 40 + 4 + 5 + 4 + 3 + 4 + 3, "{look}: every game once");
    }
}

#[test]
fn the_series_card_and_a_series_shelf_open_at_every_size() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for size in ["640x480", "1024x768", "1240x1080", "1280x720"] {
        for state in ["series:split:the legend:card", "series:shelf:alone:card", "series:cabinet:mario kart:games", "series:split:made-up:card"] {
            let (_, (w, _, px)) = run_in(tmp.path(), &stub, None, "split", size, state);
            assert!(w > 0 && px.iter().any(|&b| b > 0), "{state} at {size}");
        }
    }
}

/// Sol's review 26.11.2026: a new index that sorts a series in before the open one (Home's row, the way back from a game's
/// card and its Connections list kept the old number) must leave the player on the same series.
#[test]
fn a_series_sorted_in_before_the_open_one_keeps_the_way_back_on_the_same_series() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let (out, _) = run_in(tmp.path(), &stub, None, "split", "1280x720", "series:split:the legend:moved");
    let line = out.lines().find_map(|l| l.strip_prefix("series-moved ")).expect("the moved report");
    let z = "The Legend of Zelda";
    assert_eq!(line, format!("before={z} back=true screen={z} home={z} selected={z}"), "{out}");
}

/// Sol's review 26.11.2026: Settings › Look › Layout opened from Easy (no shelves by series of its own) with Order By
/// series made the other looks' previews by console.
#[test]
fn the_layout_previews_are_by_series_from_easy_too() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let (out, _) = run_in(tmp.path(), &stub, None, "split", "1280x720", "series:split::previews");
    let line = out.lines().find(|l| l.starts_with("look previews:")).unwrap_or_else(|| panic!("{out}"));
    assert!(line.contains(", by series"), "{line}");
}

/// ScreenScraper's series (`familles`), read from its cache only (`screenscraper::series_of`, no request): two games of
/// two consoles that the game data puts in no series stand on one shelf by it.
#[test]
fn screenscrapers_series_from_its_cache_join_games_into_a_shelf() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library_in(&tmp.path().join("lib-ss"), None, "split");
    let ss = lib.join("data").join("screenscraper");
    std::fs::create_dir_all(&ss).unwrap();
    // `<stem> <crc> <size> <state> <families> <number>` (`screenscraper::Index`).
    std::fs::write(ss.join("megadrive.tsv"), "Gunstar Heroes (USA)\t0000AA01\t1\t1201 us \tTreasure Hunt\t1\n").unwrap();
    std::fs::write(ss.join("n64.tsv"), "Banjo-Kazooie (USA)\t0000AA02\t1\t1202 us \tTreasure Hunt\t2\n").unwrap();
    let png = tmp.path().join("ss.png");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = &stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", "series:split", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_SNAPSHOT_SIZE", "1280x720")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    let out = String::from_utf8_lossy(&out.stdout).into_owned();
    let (shelves, _) = report(&out);
    let shelf = shelves.iter().find(|s| s.0 == "Treasure Hunt").unwrap_or_else(|| panic!("{shelves:?}"));
    assert_eq!(shelf.2, ["megadrive/Gunstar Heroes", "n64/Banjo-Kazooie"], "both, numbered 1 and 2: {shelf:?}");
}

/// A series' cover and spine (docs/SERIES-SHELF.md "Its cover and its spine"): the cases of its games with box art on
/// the card stacked (at most four), one alone, none the plain front; ScreenScraper's logo of one of its games on its
/// spine while Real spines is On, its name otherwise. Nothing is fetched for either (a snapshot sends nothing, as offline).
#[test]
fn a_series_cover_stacks_its_games_boxes_and_its_spine_carries_a_logo_on_the_card() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library_in(&tmp.path().join("lib-look"), None, "shelf");
    let data = lib.join("data");
    let cover = Path::new(&ROOT).join("assets/media/n64-front.png");
    let put = |folder: &str, system: &str, stem: &str, from: &Path| {
        let d = data.join(folder).join(system);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::copy(from, d.join(format!("{stem}.png"))).unwrap();
    };
    // Zelda: both games with box art. Mario Kart: one of three. Metroid: none.
    put("covers", "n64", "Legend of Zelda, The - Ocarina of Time (USA)", &cover);
    put("covers", "gc", "Legend of Zelda, The - The Wind Waker (USA)", &cover);
    put("covers", "nds", "Mario Kart DS (USA)", &cover);
    // A logo for Zelda's Wind Waker: a see-through PNG, wider than high.
    let logo = tmp.path().join("wheel.png");
    {
        let f = std::fs::File::create(&logo).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), 60, 20);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..60 * 20).flat_map(|i| if (i % 60) > 5 && (i % 60) < 55 { [240, 200, 40, 255] } else { [0, 0, 0, 0] }).collect();
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    put("wheels", "gc", "Legend of Zelda, The - The Wind Waker (USA)", &logo);
    let look = |settings: &str| -> Vec<String> {
        std::fs::write(data.join("settings.json"), settings).unwrap();
        let png_out = tmp.path().join("look.png");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd
            .args(["--snapshot", "series:shelf:the legend", png_out.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
            .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.strip_prefix("series-look ")).map(str::to_string).collect()
    };
    let on = look(r#"{"onboarded": true, "theme": "shelf", "shelf_order": "series"}"#);
    let line = |lines: &[String], name: &str| lines.iter().find(|l| l.starts_with(&format!("{name}|"))).cloned().unwrap_or_else(|| panic!("{name} in {lines:?}"));
    // Logos are off in 0.8 (`series_look::SERIES_LOGOS`): the spine shows the series' name, also with a logo on the card.
    let spine = if pocket_ui::series_look::SERIES_LOGOS { "logo" } else { "name" };
    assert_eq!(line(&on, "The Legend of Zelda"), format!("The Legend of Zelda|cases=2|{spine}"), "{on:?}");
    assert_eq!(line(&on, "Mario Kart"), "Mario Kart|cases=1|name", "one box art: that case alone; no logo: the name");
    assert_eq!(line(&on, "Metroid"), "Metroid|cases=0|name", "no box art: the plain front");
    // Real spines Off: the logo stays on the card, the spine shows the name; the cover is the same.
    let off = look(r#"{"onboarded": true, "theme": "shelf", "shelf_order": "series", "screenscraper_no_spines": true}"#);
    assert_eq!(line(&off, "The Legend of Zelda"), "The Legend of Zelda|cases=2|name", "{off:?}");
}

/// Sol's review 26.11.2026: the cabinet's preview of a real library by series read its covers on the UI thread (`inline`).
/// With box art for every game, the previews read none on the spot: the cabinet's thread does, as on the shelf.
#[test]
fn the_layout_previews_by_series_read_no_cover_on_the_spot() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library_in(&tmp.path().join("lib-covers"), None, "split");
    let cover = Path::new(&ROOT).join("assets/media/n64-front.png");
    for sys in std::fs::read_dir(lib.join("roms")).unwrap().flatten() {
        let id = sys.file_name().to_string_lossy().into_owned();
        let dir = lib.join("data").join("covers").join(&id);
        std::fs::create_dir_all(&dir).unwrap();
        for rom in std::fs::read_dir(sys.path()).unwrap().flatten() {
            let stem = rom.path().file_stem().unwrap().to_string_lossy().into_owned();
            std::fs::copy(&cover, dir.join(format!("{stem}.png"))).unwrap();
        }
    }
    let png = tmp.path().join("covers.png");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = &stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", "series:split::previews", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_SNAPSHOT_SIZE", "1280x720")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    let out = String::from_utf8_lossy(&out.stdout).into_owned();
    let line = out.lines().find(|l| l.starts_with("look previews:")).unwrap_or_else(|| panic!("{out}"));
    assert!(line.ends_with(", by series, 0 read on the spot"), "{line}");
}
