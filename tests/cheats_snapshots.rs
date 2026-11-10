//! A missing database, progress and errors must paint readable words on the TrimUI's small screen.
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
include!("common/root.rs");

struct Wayland { path: PathBuf, child: Child }
impl Drop for Wayland {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

#[test]
#[cfg(feature = "brick")]
fn extras_optional_data_states_are_readable_in_all_languages_at_640x480_large() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("wayland");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py")])
        .arg(&socket).stdout(Stdio::null()).spawn().unwrap();
    let wayland = Wayland { path: socket, child };
    test_clock::wait_for(|| wayland.path.exists(), std::time::Duration::from_secs(2), "Wayland snapshot stub");
    let shots = std::env::var_os("POCKETUI_EXTRAS_SHOTS").map(PathBuf::from).unwrap_or_else(|| tmp.path().join("shots"));
    std::fs::create_dir_all(&shots).unwrap();
    for appearance in ["dark", "light"] {
        let library = tmp.path().join(appearance);
        assert!(Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py")])
            .arg(&library).arg(appearance).env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .stdout(Stdio::null()).status().unwrap().success());
        for lang in ["en", "de", "es"] {
            for (family, states) in [("cheats", &[
                ("missing", "Get · 30 MB"), ("downloading", "42%"), ("checking", "Checking…"),
                ("saving", "Saving…"), ("installed", "Installed · 30 MB"), ("installed-offline", "Installed · 30 MB"),
                ("update", "Update · 30 MB"), ("offline", "Needs Wi-Fi"), ("no-space", "Needs 62 MB"),
                ("update-no-space", "Needs 62 MB"),
                ("error", "Retry"), ("remove", "Remove?"),
            ][..]), ("portmaster", &[
                ("missing", "Browse"), ("download", "23%"), ("checking", "Checking…"),
                ("saving", "Preparing…"), ("installed", "Browse"), ("installed-offline", "Browse"),
                ("update", "Browse"), ("offline", "Needs Wi-Fi"), ("no-space", "Needs 700 MiB"),
                ("error", "Retry"), ("remove", "Remove?"), ("removing", "Removing…"),
                ("installed:x", "Remove?"), ("installed:xb", "Browse"),
            ][..]), ("art", &[("error", "Retry")][..])] {
              for &(state, value) in states {
                let kind = state;
                let state = format!("setup:extras-{family}-{state}");
                let file = format!("{appearance}-{lang}-{}", state.replace(':', "-"));
                let png = shots.join(format!("{file}.png"));
                let visible = shots.join(format!("{file}.txt"));
                let misses = shots.join(format!("{file}.misses"));
                let boxes = shots.join(format!("{file}.boxes"));
                let result = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
                    .args(["--snapshot", &state]).arg(&png)
                    .env("WAYLAND_DISPLAY", &wayland.path).env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                    .env("POCKETUI_LANG", lang).env("POCKETUI_DEVICE", "brick").env("POCKETUI_BRICK_STANDALONE", "1")
                    .env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_TEXT_SIZE", "large")
                    .env("POCKETUI_DATA", library.join("data")).env("POCKETUI_ES_SYSTEMS", library.join("es_systems.cfg"))
                    .env("POCKETUI_VISIBLE_TEXT", &visible).env("POCKETUI_I18N_MISSES", &misses)
                    .env("POCKETUI_TEXT_BOXES", &boxes).output().unwrap();
                assert!(result.status.success(), "{file}: {}", String::from_utf8_lossy(&result.stderr));
                let text = std::fs::read_to_string(visible).unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(text.contains("Extras"), "{file}: the page title is visible: {text}");
                let cheats = if lang == "es" { "Trucos" } else { "Cheats" };
                assert!(text.contains(cheats), "{file}: {text}");
                // Scrolling can put the lower row outside the window; every selected row still paints whole.
                assert!(!text.contains("Picture…"), "{file}: row names must stay complete: {text}");
                let layout = std::fs::read_to_string(boxes).unwrap();
                assert!(layout.trim().is_empty(), "{file}: unreadable text: {layout}");
                let value = if family == "portmaster" { match (lang, value) {
                    ("de", "Browse") => "Durchsehen", ("es", "Browse") => "Explorar",
                    ("de", "Checking…") => "Wird geprüft…", ("es", "Checking…") => "Comprobando…",
                    ("de", "Preparing…") => "Vorbereiten…", ("es", "Preparing…") => "Preparando…",
                    ("de", "Needs Wi-Fi") => "Braucht WLAN", ("es", "Needs Wi-Fi") => "Necesita Wi-Fi",
                    ("de", "Needs 700 MiB") => "Braucht 700 MiB", ("es", "Needs 700 MiB") => "Necesita 700 MiB",
                    ("de", "Retry") => "Erneut versuchen", ("es", "Retry") => "Reintentar",
                    ("de", "Remove?") => "Entfernen?", ("es", "Remove?") => "¿Eliminar?",
                    ("de", "Removing…") => "Wird entfernt…", ("es", "Removing…") => "Eliminando…",
                    _ => value,
                }} else { match (lang, kind) {
                    ("de", "missing") => "Holen · 30 MB", ("es", "missing") => "Conseguir · 30 MB",
                    ("de", "checking") => "Wird geprüft…", ("es", "checking") => "Comprobando…",
                    ("de", "saving") => "Wird gespeichert…", ("es", "saving") => "Guardando…",
                    ("de", "installed" | "installed-offline") => "Installiert · 30 MB",
                    ("es", "installed" | "installed-offline") => "Instalado · 30 MB",
                    ("de", "update") => "Update · 30 MB", ("es", "update") => "Actualizar · 30 MB",
                    ("de", "offline") => "Braucht WLAN", ("es", "offline") => "Necesita Wi-Fi",
                    ("de", "no-space" | "update-no-space") => "Braucht 62 MB", ("es", "no-space" | "update-no-space") => "Necesita 62 MB",
                    ("de", "error") => "Erneut versuchen", ("es", "error") => "Reintentar",
                    ("de", "remove") => "Entfernen?", ("es", "remove") => "¿Eliminar?",
                    _ => value,
                }};
                assert!(text.contains(value), "{file}: missing or clipped state {value}: {text}");
                if family == "art" {
                    let reason = match lang {
                        "de" => "Der Bilderserver ist nicht erreichbar. Versuch es später noch einmal.",
                        "es" => "No se pudo conectar con el servidor de imágenes. Inténtalo más tarde.",
                        _ => "Couldn't reach the picture server. Try again later.",
                    };
                    assert!(text.contains(reason), "{file}: Retry explains the picture-server failure: {text}");
                }
                if family == "cheats" && kind == "update" {
                    let preserved = match lang {
                        "de" => "Die installierten Codes funktionieren weiter, bis das Update fertig ist.",
                        "es" => "Los códigos instalados siguen funcionando hasta que termine la actualización.",
                        _ => "The installed codes keep working until the update finishes.",
                    };
                    assert!(text.contains(preserved), "{file}: the update explains that installed codes keep working: {text}");
                }
                if lang != "en" {
                    // The recorder also lists successful translations (O) and their wrapped painted
                    // pieces (M). Brick substitutes its physical MENU label after translation.
                    let recorded = std::fs::read_to_string(misses).unwrap_or_default();
                    let translated: Vec<_> = recorded.lines().filter_map(|line| line.strip_prefix("O\t"))
                        .flat_map(|line| [line.to_owned(), line.replace("HOME", "MENU")]).collect();
                    let missing: Vec<_> = recorded.lines().filter(|line| {
                        if line.starts_with("P\t") { return true; }
                        let Some(piece) = line.strip_prefix("M\t") else { return false };
                        !matches!(piece, "A" | "B" | "X")
                            && piece != value
                            && piece.chars().any(char::is_alphabetic)
                            && !translated.iter().any(|whole| whole.contains(piece.trim_end_matches('…')))
                    }).collect();
                    assert!(missing.is_empty(), "{file}: untranslated text: {missing:?}");
                }
                assert!(std::fs::metadata(png).unwrap().len() > 1000, "{file}: an actual screen was rendered");
            }
        }
      }
    }
}

#[test]
fn optional_cheats_explain_every_state_in_dark_and_light_at_large_text() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("wayland");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py")])
        .arg(&socket).stdout(Stdio::null()).spawn().unwrap();
    let wayland = Wayland { path: socket, child };
    test_clock::wait_for(|| wayland.path.exists(), std::time::Duration::from_secs(2), "Wayland snapshot stub");
    let retained = std::env::var_os("POCKETUI_CHEATS_SHOTS").map(PathBuf::from);
    let shots = retained.unwrap_or_else(|| tmp.path().join("shots"));
    std::fs::create_dir_all(&shots).unwrap();
    for appearance in ["dark", "light"] {
        let library = tmp.path().join(appearance);
        assert!(Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py")])
            .arg(&library).arg(appearance).env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .stdout(Stdio::null()).status().unwrap().success());
        let mut images = Vec::new();
        for (state, caption, explanation) in [
            ("cheats-get", "Get · 30 MB", "Codes for many games, used from the pause menu (HOME › Options › Cheats)."),
            ("cheats-downloading", "50%", "You can go back. The next try continues the download."),
            ("cheats-offline", "Needs Wi-Fi", "Connect to Wi-Fi first to use these."),
            ("cheats-unavailable", "Retry", "Cheats aren't available in this build. Try updating pocket-ui."),
            ("cheats-empty", "No cheats found for this game", "The cheat database has no matching codes. You can keep playing normally."),
        ] {
            let png = shots.join(format!("{appearance}-{state}.png"));
            let visible = shots.join(format!("{appearance}-{state}.txt"));
            let result = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
                .args(["--snapshot", state]).arg(&png)
                .env("WAYLAND_DISPLAY", &wayland.path).env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                .env("POCKETUI_LANG", "en").env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_TEXT_SIZE", "large")
                .env("POCKETUI_DATA", library.join("data")).env("POCKETUI_ES_SYSTEMS", library.join("es_systems.cfg"))
                .env("POCKETUI_VISIBLE_TEXT", &visible).output().unwrap();
            assert!(result.status.success(), "{appearance} {state}: {}", String::from_utf8_lossy(&result.stderr));
            let text = std::fs::read_to_string(visible).unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(text.contains(caption), "{appearance} {state}: caption clipped or missing: {text}");
            assert!(text.contains(explanation), "{appearance} {state}: explanation clipped or missing: {text}");
            if state == "cheats-downloading" {
                assert!(text.contains("50%"), "the player can see download progress: {text}");
            }
            let image = std::fs::read(png).unwrap();
            assert!(!images.contains(&image), "{appearance} {state}: a distinct state must change the screen");
            images.push(image);
        }
    }
}
