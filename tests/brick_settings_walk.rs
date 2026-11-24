//! The TrimUI Brick's own settings as a player meets them (settings walk 06.11.2026, before the public release): rendered with
//! `pocket-ui --snapshot` as the Brick's own card runs it (`POCKETUI_DEVICE=brick`, `POCKETUI_BRICK_STANDALONE=1`, 1024x768) on
//! scripts/curator-fake-library.py's made-up library, in English, German and Spanish. Each test is a spot the walk found confusing:
//! a page that named the Pocket, offered ROCKNIX's menu or Android, called a tested handheld untested, said "System: unknown", ran
//! text into the foot or cut a line short, or drew sticks the Brick hasn't.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/empty_views.rs).
#![cfg(feature = "brick")]

use std::path::PathBuf;
use std::process::Command;

include!("common/root.rs");

struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The made-up library, a Wayland stand-in and a place for the pictures.
struct Card {
    _dir: tempfile::TempDir,
    tmp: PathBuf,
    stub: Option<Stub>,
    /// `POCKETUI_DEVICE` and the screen: the Brick, or the Smart Pro on the same card (`Card::smart_pro`, docs/TRIMUI-SMART-PRO.md).
    device: (&'static str, &'static str),
    /// `POCKETUI_HOST`: the dArkOS port (`Card::darkos`) instead of the Brick's own card.
    host: Option<&'static str>,
    /// `POCKETUI_NET_CLASS` (the port's `/sys/class/net`): with `wlan0` in it, dArkOS's Wi-Fi is there (`darkos::wifi_here`).
    net_class: Option<PathBuf>,
}

impl Card {
    fn new() -> Card {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path().to_path_buf();
        let out = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let stub = (cfg!(target_os = "linux") && !std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty())).then(|| {
            let path = tmp.join("wl");
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
            let stub = Stub { path, child };
            test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            stub
        });
        Card { _dir: dir, tmp, stub, device: ("brick", "1024x768"), host: None, net_class: None }
    }

    /// The same build as the dArkOS port on an RG353M (docs/DARKOS.md): `POCKETUI_HOST=darkos`, 640x480, no Brick card;
    /// `wifi`: the kernel shows `wlan0` (dArkOS's Wi-Fi), else only `lo`.
    fn darkos(wifi: bool) -> Card {
        let card = Card::new();
        let net = card.tmp.join("net");
        std::fs::create_dir_all(net.join("lo")).unwrap();
        if wifi {
            std::fs::create_dir_all(net.join("wlan0")).unwrap();
        }
        Card { device: ("darkos-rg353m", "640x480"), host: Some("darkos"), net_class: Some(net), ..card }
    }

    /// The same card on a TrimUI Smart Pro: 1280x720, sticks, no F keys, no slider, no lights page.
    fn smart_pro() -> Card {
        Card { device: ("smartpro", "1280x720"), ..Card::new() }
    }

    /// What the player reads on `state` in `lang` at `size` (Normal or Large text), and the painter's findings (text running into text
    /// or the hints, cut by an edge).
    fn shot(&self, state: &str, lang: &str, size: &str) -> (String, String) {
        self.shot_style(state, lang, size, "barlow")
    }

    fn shot_style(&self, state: &str, lang: &str, size: &str, font: &str) -> (String, String) {
        let (visible, boxes) = (self.tmp.join("visible.txt"), self.tmp.join("boxes.txt"));
        let _ = std::fs::remove_file(&visible);
        let binary = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
        let mut cmd = Command::new(binary);
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.path);
        }
        match self.host {
            Some(host) => cmd.env("POCKETUI_HOST", host).env_remove("POCKETUI_BRICK_STANDALONE"),
            None => cmd.env("POCKETUI_BRICK_STANDALONE", "1").env_remove("POCKETUI_HOST"),
        };
        match &self.net_class {
            Some(net) => cmd.env("POCKETUI_NET_CLASS", net),
            None => cmd.env_remove("POCKETUI_NET_CLASS"),
        };
        let out = cmd
            .args(["--snapshot", state, self.tmp.join("shot.png").to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_DEVICE", self.device.0)
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"))
            .env("POCKETUI_DATA", self.tmp.join("lib/data"))
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_TEXT_SIZE", size)
            .env("POCKETUI_SNAPSHOT_SIZE", self.device.1)
            .env("POCKETUI_FONT", font)
            .env("POCKETUI_RETRO", if font == "retro" { "cocoa" } else { "off" })
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env_remove("SWAYSOCK")
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        (std::fs::read_to_string(&visible).unwrap(), std::fs::read_to_string(&boxes).unwrap_or_default())
    }
}

const LANGS: [&str; 6] = ["en", "de", "es", "pt", "fr", "it"];

/// The Brick's printed menu key is MENU in the hints, in every language and both appearances, never HOME. (It was checked
/// on the controls card's snapshots until the card went, 15.11.2026.)
#[test]
fn printed_menu_key_is_used_in_the_brick_hints_in_both_appearances() {
    let card = Card::new();
    for appearance in ["dark", "light"] {
        let settings = card.tmp.join("lib/data/settings.json");
        let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        saved["appearance"] = appearance.into();
        std::fs::write(&settings, serde_json::to_vec(&saved).unwrap()).unwrap();
        for lang in LANGS {
            let (visible, _) = card.shot("home", lang, "normal");
            assert!(visible.contains("MENU") && !visible.contains("HOME"), "{appearance} {lang}: {visible}");
        }
    }
}

/// A player pressing F1/F2 during Controller setup got eight seconds of waiting and then "No button pressed".
/// Feed the real evdev codes into capture and check the page explains the shortcut instead.
#[test]
fn fkeys_remapping_explains_where_shortcuts_are_changed() {
    let card = Card::new();
    for (lang, explanation, settings) in [
        ("en", "F1/F2 are shortcuts, not game buttons.", "Lights & buttons"),
        ("de", "F1/F2 sind Kurzbefehle, keine Spieltasten.", "Lichter & Tasten"),
        ("es", "F1/F2 son atajos, no botones de juego.", "Luces y botones"),
    ] {
        for size in ["normal", "large"] {
            // "controls:19": the pause menu's Controls page on its last row, Controller setup (a Game Boy Color game: the stick,
            // sixteen buttons, Hold to toggle and Reset controls before it).
            for state in ["controllers:1:brick-f1", "controllers:1:brick-f2", "controls:19"] {
                let (shown, _) = card.shot(state, lang, size);
                assert!(shown.contains(explanation) && shown.contains(settings), "{lang} {size} {state}: {shown}");
            }
        }
    }
}

/// Lights & buttons changes the global shortcut action; it must not suggest per-game button binding.
#[test]
fn fkeys_lights_help_explains_that_the_assignment_is_global() {
    let card = Card::new();
    for (lang, explanation) in [
        ("en", "Shortcuts for all games; not game buttons."),
        ("de", "Kurzbefehle für alle Spiele; keine Spieltasten."),
        ("es", "Atajos para todos los juegos; no botones de juego."),
    ] {
        for size in ["normal", "large"] {
            for state in ["leds:off:1", "leds:off:2"] {
                let (shown, _) = card.shot(state, lang, size);
                assert!(shown.contains(explanation), "{lang} {size} {state}: {shown}");
            }
        }
    }
}

/// The handheld is a Brick: the Tailscale, Sync, remote access and power pages said "the Pocket", in every language.
#[test]
fn the_bricks_pages_call_it_the_brick() {
    let card = Card::new();
    for lang in LANGS {
        for state in ["tailscale", "sync:off", "sys:power-noboot", "settings:0:3"] {
            let (shown, _) = card.shot(state, lang, "normal");
            // The German and Spanish line under Remote access names no device at all.
            assert!(shown.contains("Brick") || (state == "settings:0:3" && lang != "en"), "{lang} {state}: {shown}");
            // Pocket Sync is a feature's name and keeps its word there (75cd47ef4, `brick_words`); the handheld is a Brick.
            assert!(!shown.replace("Pocket Sync", "").contains("Pocket"), "{lang} {state}: {shown}");
        }
    }
}

/// SELECT + START on the Brick's own card: there is no NextUI on it, and pocket-ui only starts again. The dialog says so.
#[test]
fn leaving_on_the_bricks_own_card_says_it_restarts_pocket_ui() {
    let card = Card::new();
    for (lang, title) in [("en", "Restart pocket-ui?"), ("de", "pocket-ui neu starten?"), ("es", "¿Reiniciar pocket-ui?")] {
        let (shown, _) = card.shot("leave", lang, "normal");
        assert!(shown.lines().any(|l| l == title), "{lang}: {shown}");
        assert!(!shown.contains("NextUI"), "{lang}: {shown}");
    }
}

/// Nothing of ROCKNIX's or Android's on the Brick: About offered "Open EmulationStation…", the power menu how to start Android.
#[test]
fn the_brick_offers_no_emulationstation_and_no_android() {
    let card = Card::new();
    for lang in LANGS {
        let (about, _) = card.shot("about", lang, "normal");
        assert!(!about.contains("EmulationStation"), "{lang}: {about}");
        let (power, _) = card.shot("sys:power-noboot", lang, "normal");
        assert!(!power.contains("Android"), "{lang}: {power}");
    }
}

/// The first start called the Brick untested ("pocket-ui hasn't been tested on a TrimUI Brick yet"), and Updates showed a System line
/// "unknown" for TrimUI's firmware, which nothing on the card updates.
#[test]
fn the_first_start_and_updates_say_nothing_untrue_about_the_brick() {
    let card = Card::new();
    let (first, _) = card.shot("setup:first", "en", "normal");
    assert!(first.contains("Get started"), "{first}");
    assert!(!first.contains("tested"), "{first}");
    for lang in LANGS {
        let (updates, _) = card.shot("updates:note", lang, "normal");
        assert!(updates.contains("pocket-ui"), "{lang}: {updates}");
        assert!(!updates.lines().any(|l| ["System", "Sistema"].contains(&l)), "{lang}: {updates}");
    }
}

/// Sleep & power on the Brick: its note took two lines and the selected row's explanation was cut to one line with "…"; at Large
/// text the explanation ran under the hints.
#[test]
fn sleep_and_power_explains_each_row_in_full_on_the_brick() {
    let card = Card::new();
    for lang in LANGS {
        for font in ["barlow", "retro"] {
            for size in ["normal", "medium", "large"] {
                // sleep:3 is Short press (Sleep, the default), and with Right on it Screen off: both explanations whole (08.11.).
                for state in ["sleep", "sleep:1", "sleep:2", "sleep:3", "sleep:3:keys=r"] {
                    let (shown, boxes) = card.shot_style(state, lang, size, font);
                    assert!(!shown.lines().any(|l| l.ends_with('…') || l.ends_with("...")), "{lang} {font} {size} {state}: {shown}");
                    assert!(boxes.lines().all(|l| !['O', 'E', 'T', 'V'].iter().any(|kind| l.starts_with(*kind))), "{lang} {font} {size} {state}: {boxes}");
                }
            }
        }
    }
}

/// Sync with no Wi-Fi: "Connect to Wi-Fi first" ran into the page's two-line foot at 1024x768, touched the hints under a one-line
/// foot (the Library data row), and in German ran off the screen.
#[test]
fn syncs_wifi_line_stays_clear_of_the_foot() {
    let card = Card::new();
    for lang in LANGS {
        for size in ["normal", "large"] {
            for state in ["sync:nowifi", "sync:nowifi:5"] {
                let (shown, boxes) = card.shot(state, lang, size);
                assert!(shown.contains("Wi-Fi") || shown.contains("WLAN"), "{lang} {size} {state}: {shown}");
                assert!(boxes.lines().all(|l| !l.starts_with('O') && !l.starts_with('E')), "{lang} {size} {state}: {boxes}");
            }
        }
    }
}

/// Test buttons: the Brick's own pad was drawn with two sticks and their readouts; it has none.
#[test]
fn test_buttons_draws_no_sticks_on_the_bricks_own_pad() {
    let card = Card::new();
    let (shown, _) = card.shot("test-buttons", "en", "normal");
    assert!(shown.contains("SELECT") && shown.contains("Player 1"), "{shown}");
    let readout = |l: &str| l.split_once(" · ").is_some_and(|(x, y)| x.trim_start_matches('-').parse::<i32>().is_ok() && y.trim_start_matches('-').parse::<i32>().is_ok());
    assert!(!shown.lines().any(readout), "{shown}");
}

/// The defaults chosen for the walk's open questions (06.11.): About doesn't call the Brick experimental; Performance has no fan rows
/// (the Brick has none); Sync offers no PS2, GameCube & Wii or 3DS folders; Backup names the card's folder as a computer sees it and
/// says nothing about flashing; the sleep line says what happens, without "yet"; Test buttons draws F1 and F2.
#[test]
fn the_bricks_pages_offer_only_what_the_brick_has() {
    let card = Card::new();
    for lang in LANGS {
        let (about, _) = card.shot("about", lang, "normal");
        assert!(about.contains("TrimUI Brick") && !about.contains("experiment") && !about.contains("expériment") && !about.contains("sperimental") && !about.contains("실험"), "{lang}: {about}");
        let (perf, _) = card.shot("perf", lang, "normal");
        // No fan row ("Fan", "Fan outside games" and their translations start their line); the note that names the pause
        // menu's group "Options › Speed, rumble, fan" is that menu's real name (it shows since the flat hint bar left it room).
        let fan_row = |l: &str| ["Fan", "Lüfter", "Ventilador", "Ventoinha", "Ventola", "Ventilateur"].iter().any(|w| l.trim_start().starts_with(w));
        assert!(!perf.lines().any(fan_row), "{lang}: {perf}");
        let (sync, _) = card.shot("sync:off:6", lang, "normal");
        assert!(!["PS2", "GameCube", "3DS"].iter().any(|w| sync.contains(w)), "{lang}: {sync}");
        let (backup, _) = card.shot("backup", lang, "normal");
        assert!(backup.contains("Roms/backup/pocket-ui-saves"), "{lang}: {backup}");
        assert!(!["Flashing", "Aufspielen", "grabarla", "PS2"].iter().any(|w| backup.contains(w)), "{lang}: {backup}");
        let (sleep, _) = card.shot("sleep", lang, "normal");
        assert!(!sleep.contains(" yet") && !sleep.contains("Noch kein") && !sleep.contains("Aún sin"), "{lang}: {sleep}");
    }
    let (pad, _) = card.shot("test-buttons", "en", "normal");
    assert!(pad.lines().any(|l| l == "F1") && pad.lines().any(|l| l == "F2"), "{pad}");
}

/// The manual on the Brick starts with the Brick (its own card, Roms/…, the power key, updates) and leaves out ROCKNIX's pages.
#[test]
fn the_manual_on_the_brick_starts_with_the_brick() {
    let card = Card::new();
    let (first, _) = card.shot("manual", "en", "normal");
    assert!(first.contains("Brick") && !first.contains("ROCKNIX"), "{first}");
}

/// Add games names each system's folder as a computer sees it on the Brick's card ("Roms/gba"), not "roms/gba": the card's es_systems
/// paths are under /storage/roms, which is the card's Roms folder.
#[test]
fn add_games_names_the_cards_roms_folders() {
    let card = Card::new();
    let cfg = card.tmp.join("lib/es_systems.cfg");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let mut out = String::new();
    for part in text.split("<path>") {
        match part.split_once("</path>") {
            Some((path, rest)) if !out.is_empty() => {
                let system = path.rsplit('/').next().unwrap_or_default();
                out += &format!("<path>/storage/roms/{system}</path>{rest}");
            }
            _ => out += part,
        }
    }
    std::fs::write(&cfg, out).unwrap();
    let (shown, _) = card.shot("setup:games", "en", "normal");
    assert!(shown.contains("Roms/gba") && !shown.contains("roms/gba"), "{shown}");
}


/// The Brick's Normal text is a step above the design (a note's 21 → 25 px, Body 28 → 31, the hint pill's 30 → 32: readable at
/// arm's length on a 3.2" panel): Home's one hint bar (the same pill as on every page) shows three whole hints, in English,
/// German and Spanish, and nothing runs into it. A fourth (SELECT, the recent games) does not fit the 1024 px beside them
/// and goes first (`kit::top_hints`); the manual and the tour name it.
#[test]
fn the_bricks_larger_type_keeps_homes_whole_hint_bar() {
    let card = Card::new();
    for (lang, labels) in [
        ("en", ["Open", "Quick settings", "Settings"]),
        ("de", ["Öffnen", "Schnellmenü", "Einstellungen"]),
        ("es", ["Abrir", "Ajustes rápidos", "Ajustes"]),
    ] {
        let (shown, boxes) = card.shot("home", lang, "normal");
        assert!(labels.iter().all(|label| shown.lines().any(|l| l == *label)), "{lang}: the hint bar lost hints: {shown}");
        assert!(!boxes.lines().any(|l| l.starts_with("O\t") || l.starts_with("T\t")), "{lang}: {boxes}");
    }
}

// ---- the same card on a TrimUI Smart Pro (1280x720; docs/TRIMUI-SMART-PRO.md) ----------------------------------------------

/// A Smart Pro owner meets their own handheld's name: About says "TrimUI Smart Pro (experimental)" (nobody on the project has played
/// on one), the pages that called it "the Pocket" say "Smart Pro", never "Brick", and the first start says it is untested.
#[test]
fn the_smart_pro_is_called_by_its_name_and_experimental() {
    let card = Card::smart_pro();
    for lang in LANGS {
        let (about, _) = card.shot("about", lang, "normal");
        assert!(about.contains("TrimUI Smart Pro") && (about.contains("experiment") || about.contains("expériment") || about.contains("sperimental") || about.contains("실험")), "{lang}: {about}");
        for state in ["tailscale", "sync:off", "sys:power-noboot"] {
            let (shown, _) = card.shot(state, lang, "normal");
            assert!(shown.contains("Smart Pro"), "{lang} {state}: {shown}");
            assert!(!shown.contains("Pocket") && !shown.contains("Brick"), "{lang} {state}: {shown}");
        }
    }
    let (first, _) = card.shot("setup:first", "en", "normal");
    assert!(first.contains("Smart Pro") && first.contains("tested"), "{first}");
}

/// The Smart Pro has sticks and none of the Brick's extra keys: Test buttons draws both sticks with their readouts and no F1 or F2;
/// Settings has no Lights & buttons page (its rings are not mapped, and it has no F keys or slider to assign).
#[test]
fn the_smart_pro_shows_its_sticks_and_none_of_the_bricks_keys() {
    let card = Card::smart_pro();
    let (pad, _) = card.shot("test-buttons", "en", "normal");
    let readout = |l: &str| l.split_once(" · ").is_some_and(|(x, y)| x.trim_start_matches('-').parse::<i32>().is_ok() && y.trim_start_matches('-').parse::<i32>().is_ok());
    assert!(pad.lines().filter(|l| readout(l)).count() >= 2, "both sticks' readouts: {pad}");
    assert!(!pad.lines().any(|l| l == "F1" || l == "F2"), "{pad}");
    for group in 0..7 {
        let (shown, _) = card.shot(&format!("settings:{group}"), "en", "normal");
        assert!(!shown.contains("Lights & buttons") && !shown.contains("mute slider") && !shown.contains("F1"), "settings:{group}: {shown}");
    }
}

/// The Smart Pro paints its 1280x720 natively (`scaled_layout::logical_size`; until 13.11.2026 it was laid out at 1707x960 and
/// scaled down, so this test never saw 720 lines): the 1280x960 handhelds' width with 240 lines less, a short screen like the
/// Brick's 768 (`kit::short_screen`). On the pages a player reads first, in three languages, at Normal and Large text, in both
/// styles, it shows no line cut short and no text running into text, the hints or the edge that neither the same page at
/// 1280x960 (the Mini V1's layout, the one every 1280-wide page is made for) nor the Brick's 1024x768 (the other short screen)
/// shows too. What they share is the layout's own (a long Spanish explanation at Large, a long system name in a tile, About's
/// help at Large on a short screen): not the Smart Pro's, and not fixed here.
#[test]
fn text_fits_on_the_smart_pros_1280x720() {
    let card = Card::smart_pro();
    let wide = Card { device: ("mini", "1280x960"), ..Card::new() };
    let brick = Card::new();
    let states = ["home", "setup:first", "setup:first-games", "setup:first-look", "settings", "settings:0", "settings:1", "settings:2",
        "settings:3", "settings:4", "settings:5", "settings:6", "settings:7", "about", "sleep", "sleep:1", "perf", "sync:off", "test-buttons", "updates:note", "leave"];
    let findings = |shown: &str, boxes: &str| -> Vec<String> {
        let cut = shown.lines().filter(|l| l.ends_with('…') || l.ends_with("...")).map(|l| format!("cut: {l}"));
        // A box line names its kind and the strings involved, no places: the same finding reads the same at both sizes.
        let boxed = boxes.lines().filter(|l| ['O', 'E', 'T', 'V'].iter().any(|kind| l.starts_with(*kind))).map(str::to_string);
        cut.chain(boxed).collect()
    };
    let mut bad = Vec::new();
    for lang in LANGS {
        for font in ["barlow", "retro"] {
            for size in ["normal", "large"] {
                for state in states {
                    let (shown, boxes) = card.shot_style(state, lang, size, font);
                    let (wide_shown, wide_boxes) = wide.shot_style(state, lang, size, font);
                    let (brick_shown, brick_boxes) = brick.shot_style(state, lang, size, font);
                    let mut there = findings(&wide_shown, &wide_boxes);
                    there.extend(findings(&brick_shown, &brick_boxes));
                    for f in findings(&shown, &boxes) {
                        // The made-up card's own folder (About's data line) is a test path, cut at any width; a card's is short.
                        // A line cut at both sizes is cut at a different letter: the same line when it starts the same.
                        let start = |l: &str| l.chars().take(20).collect::<String>();
                        let known = there.iter().any(|g| *g == f || f.starts_with("cut: ") && g.starts_with("cut: ") && start(g) == start(&f));
                        if !known && !f.contains("/lib/") {
                            bad.push(format!("{lang} {font} {size} {state}: {f}"));
                        }
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} findings neither the 1280x960 layout nor the Brick's 1024x768 has:\n{}", bad.len(), bad.join("\n"));
}

/// 0.5 candidate check, 15.11.: on the Smart Pro (1280x720) in Large text the Updates page's note was cut in every language. Every
/// Updates state there shows its whole note and step line, nothing cut, nothing running into the actions or the hints; no forgiving
/// a cut the larger layouts share, as `text_fits_on_the_smart_pros_1280x720` does.
#[test]
fn the_updates_page_shows_its_whole_note_on_the_smart_pro_in_large_text() {
    let card = Card::smart_pro();
    let states = ["updates:note", "updates:refused", "updates:ready", "updates:confirm", "updates:confirm-low", "updates:card",
        "updates:card-refused", "updates:no-wifi", "updates:timeout", "updates:unreachable", "updates:connecting", "updates:fallback", "updates:downloading",
        "updates:unpacking", "updates:moving", "updates:restarting"];
    let mut bad = Vec::new();
    for lang in LANGS {
        for font in ["barlow", "retro"] {
            for state in states {
                let (shown, boxes) = card.shot_style(state, lang, "large", font);
                // A cut line ends in " …"; German's "Startet neu …" (Restarting…) is whole.
                bad.extend(shown.lines().filter(|l| (l.ends_with(" …") && !l.ends_with("Startet neu …")) || l.ends_with("...")).map(|l| format!("{lang} {font} {state}: cut: {l}")));
                bad.extend(boxes.lines().filter(|l| ['O', 'E', 'T', 'V'].iter().any(|k| l.starts_with(*k))).map(|l| format!("{lang} {font} {state}: {l}")));
            }
        }
    }
    assert!(bad.is_empty(), "{} findings on the Smart Pro's Updates page:\n{}", bad.len(), bad.join("\n"));
}

/// Brick Pro keeps the Brick framebuffer layout and has the larger panel's type, two sticks and separate F keys.
#[test]
fn brickpro_names_the_experiment_and_shows_only_its_proven_controls() {
    let card = Card { device: ("brickpro", "1024x768"), ..Card::new() };
    for lang in LANGS {
        let (about, _) = card.shot("about", lang, "normal");
        assert!(about.contains("TrimUI Brick Pro") && (about.contains("experiment") || about.contains("expériment") || about.contains("sperimental") || about.contains("실험")), "{lang}: {about}");
    }
    let (keys, _) = card.shot("leds:colour", "en", "normal");
    assert!(keys.contains("F1 button") && keys.contains("F2 button"), "{keys}");
    for absent in ["Mute slider", "Each light", "Brightness", "Left grip", "Colour"] {
        assert!(!keys.lines().any(|l| l == absent), "unproven control {absent}: {keys}");
    }
    let (pad, _) = card.shot("test-buttons", "en", "normal");
    for key in ["F1", "F2"] { assert!(pad.lines().any(|l| l == key), "missing {key}: {pad}"); }
    let readout = |l: &str| l.split_once(" · ").is_some_and(|(x,y)| x.trim_start_matches('-').parse::<i32>().is_ok() && y.trim_start_matches('-').parse::<i32>().is_ok());
    assert!(pad.lines().filter(|l| readout(l)).count() >= 2, "both sticks: {pad}");
}

/// A handheld's start on the Brick (Cozy, Motion Full, Old-TV switch-on on, as the Brick ships): once the game is in, the
/// GBA's power light and then its display light up; 1450 ms after A the glass is brighter than with the switch-on off, a
/// block of the console and nothing else. Night check 14.11.2026 on a real Brick: only the power light came on, because
/// the Brick's build left the display's light out (it once spread over the whole screen for pocket-core to clear).
#[test]
fn a_handhelds_start_lights_its_display_on_the_brick() {
    let card = Card::new();
    let frame = |system: &str, crt: bool| -> Vec<u8> {
        let settings = format!(r#"{{"onboarded": true, "theme": "shelf", "animations": "cozy", "motion": "full", "extras": {{"playful_animations": true, "tv_switch_on": {crt}}}}}"#);
        std::fs::write(card.tmp.join("lib/data/settings.json"), settings).unwrap();
        let png = card.tmp.join(format!("launch-{system}-{crt}.png"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &card.stub {
            cmd.env("WAYLAND_DISPLAY", &s.path);
        }
        let out = cmd
            .args(["--snapshot", &format!("shelf:{system}:shelf:games:0"), png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_DEVICE", "brick")
            .env("POCKETUI_BRICK_STANDALONE", "1")
            .env("POCKETUI_ES_SYSTEMS", card.tmp.join("lib/es_systems.cfg"))
            .env("POCKETUI_DATA", card.tmp.join("lib/data"))
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
            .env("POCKETUI_SNAPSHOT_LAUNCH_MS", "1450")
            .env_remove("SWAYSOCK")
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{system}: {}", String::from_utf8_lossy(&out.stderr));
        let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
        let mut px = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut px).unwrap();
        assert_eq!((info.width, info.height), (1024, 768));
        px
    };
    for system in ["gba", "gb"] {
        let (on, off) = (frame(system, true), frame(system, false));
        let (mut n, mut brighter, mut bb) = (0usize, 0usize, (usize::MAX, usize::MAX, 0usize, 0usize));
        for (i, (a, b)) in on.chunks(3).zip(off.chunks(3)).enumerate() {
            if a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 12) {
                n += 1;
                let (x, y) = (i % 1024, i / 1024);
                bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
                if a.iter().map(|v| u32::from(*v)).sum::<u32>() > b.iter().map(|v| u32::from(*v)).sum::<u32>() {
                    brighter += 1;
                }
            }
        }
        assert!(n * 300 > 1024 * 768, "{system}: its display lights up on the Brick ({n} px changed)");
        assert!(brighter * 10 >= n * 9, "{system}: lit means brighter ({brighter} of {n})");
        assert!((bb.2 - bb.0) * (bb.3 - bb.1) * 4 < 1024 * 768, "{system}: only the glass, {bb:?}");
    }
}

/// The dArkOS port offers no row that does nothing there (dArkOS walk, 20.11.2026: Screen frames, Tailscale, Bluetooth and RomM
/// were listed and did nothing). dArkOS keeps its own Wi-Fi, Bluetooth, SSH and clock, the port brings no Tailscale and no Syncthing,
/// RomM can't write into the read-only games folders, and the port's bridge has no GPU path for frames (`darkos::LEAVES_OUT`,
/// `NEEDS_GPU`). Every category of Settings, at its first and its last row, and a game's own settings.
#[test]
fn the_darkos_port_lists_no_row_that_does_nothing_there() {
    let card = Card::darkos(true);
    let mut lines = std::collections::BTreeSet::new();
    for g in 0..8 {
        for r in [0, 99] {
            let (seen, _) = card.shot(&format!("settings:{g}:{r}"), "en", "normal");
            lines.extend(seen.lines().map(|l| l.trim().to_string()));
        }
    }
    let dead: Vec<&str> = ["Bluetooth", "Tailscale", "Remote access (SSH)", "Sync", "Time", "RomM", "Screen frames"]
        .into_iter().filter(|row| lines.contains(*row)).collect();
    assert!(dead.is_empty(), "rows that do nothing on dArkOS: {dead:?}\n{lines:#?}");
    // What works there stays: the way back, the port's own updates, Pocket Sync, the web companion, Wi-Fi (its own page).
    for kept in ["Wi-Fi", "Back to EmulationStation", "Updates", "Pocket Sync", "Web companion", "Over the game"] {
        assert!(lines.contains(kept), "{kept} is a row on dArkOS: {lines:#?}");
    }
    // A game's own settings: no Picture rows (Screen frame, scaling, effect) the CPU copy ignores.
    let (game, _) = card.shot("this-game:gba", "en", "normal");
    assert!(!game.lines().any(|l| l.trim() == "Screen frame"), "{game}");
}

/// The lead's question of 20.11.2026: the boards' rows say `wifi: false` (pocket-ui has no Wi-Fi page there), so an RG353V on
/// dArkOS's Wi-Fi was told "this handheld has no Wi-Fi" by Pocket Sync and the web companion. With `wlan0` they speak of a
/// network; without one (an RG351MP, no dongle) they still say there is none.
#[test]
fn the_darkos_port_with_wifi_is_not_told_it_has_none() {
    let none = "this handheld has no Wi-Fi";
    let (with, without) = (Card::darkos(true), Card::darkos(false));
    for state in ["web:off", "pockets:what"] {
        let online = joined(&with.shot(state, "en", "normal").0);
        assert!(!online.contains(none), "{state} with wlan0: {online}");
        let offline = joined(&without.shot(state, "en", "normal").0);
        assert!(offline.contains(none), "{state} without a wireless interface: {offline}");
    }
}

/// The starter pack can't add games in the dArkOS port (the games folders are read only there): its page says so at once instead
/// of meeting a write error after A.
#[test]
fn the_darkos_port_says_the_starter_pack_does_not_work_there() {
    let raw = Card::darkos(true).shot("starter", "en", "normal").0;
    let page = joined(&raw);
    // The hint bar offers no A ("Get all"/"Get" as a hint label of its own; the first row is "Get all 9 games").
    assert!(page.contains("isn't available on dArkOS") && !raw.lines().any(|l| matches!(l.trim(), "Get all" | "Get")), "{page}");
    let (brick, _) = Card::new().shot("starter", "en", "normal");
    assert!(!brick.contains("dArkOS"), "{brick}");
    // An empty card (one system, its folder empty, no history): Home offers no starter pack there, and says where games go on dArkOS.
    let empty = Card::darkos(true);
    let roms = empty.tmp.join("empty-roms/gba");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::write(empty.tmp.join("lib/es_systems.cfg"), format!(
        "<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba .zip</extension><command>true</command></system></systemList>", roms.display()
    )).unwrap();
    std::fs::remove_dir_all(empty.tmp.join("lib/data")).unwrap();
    std::fs::create_dir_all(empty.tmp.join("lib/data")).unwrap();
    std::fs::write(empty.tmp.join("lib/data/settings.json"), r#"{"onboarded":true}"#).unwrap();
    let home = joined(&empty.shot("shelf::shelf", "en", "normal").0);
    assert!(home.contains("No games yet") && home.contains("EASYROMS") && !home.contains("Get the starter pack"), "{home}");
}

/// Every row the dArkOS port's Settings show (each category at its first and its last row), one line each.
fn darkos_rows(card: &Card) -> std::collections::BTreeSet<String> {
    let mut lines = std::collections::BTreeSet::new();
    for g in 0..8 {
        for r in [0, 99] {
            lines.extend(card.shot(&format!("settings:{g}:{r}"), "en", "normal").0.lines().map(|l| l.trim().to_string()));
        }
    }
    lines
}

/// The lead's rule of 20.11.2026: Wi-Fi stays a row on dArkOS, with a page of its own that says where Wi-Fi is set up and shows the
/// connection, never "This device has no Wi-Fi"; and every "Settings › … › Row" an offline or network page names on dArkOS is a
/// row that is there (with dArkOS's Wi-Fi and without).
#[test]
fn the_darkos_port_points_only_at_rows_it_has() {
    for wifi in [true, false] {
        let card = Card::darkos(wifi);
        let page = joined(&card.shot("setup:wifi", "en", "normal").0);
        assert!(page.contains("Set up Wi-Fi in dArkOS") && !page.contains("has no Wi-Fi"), "wifi={wifi}: {page}");
        let rows = darkos_rows(&card);
        let states = ["web:off", "web:waiting", "achievements:offline", "pockets:nobody", "pockets:what", "starter-home:wifi-off",
            "starter:3:offline", "setup:extras", "updates:unreachable", "updates:no-wifi", "test-buttons:empty", "pc-streaming:none",
            "pc-streaming:missing", "cheats-offline"];
        let mut dead = Vec::new();
        for state in states {
            let text = joined(&card.shot(state, "en", "normal").0);
            for (i, _) in text.match_indices("Settings › ") {
                // "Settings › Category › Row …": the row is the start of what follows the second "›".
                let Some(rest) = text[i..].splitn(3, " › ").nth(2) else { continue };
                if !rows.iter().any(|r| r.len() >= 3 && rest.starts_with(r.as_str())) {
                    dead.push(format!("{state} (wifi={wifi}): …{}", text[i..].chars().take(60).collect::<String>()));
                }
            }
            if wifi && text.contains("has no Wi-Fi") {
                dead.push(format!("{state} (wifi=true) says there is no Wi-Fi: {text}"));
            }
        }
        assert!(dead.is_empty(), "texts on dArkOS that point where nothing is:\n{}", dead.join("\n"));
    }
}

/// The painted words in one line: a sentence wrapped over two lines reads whole.
fn joined(seen: &str) -> String {
    seen.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}
