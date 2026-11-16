//! A console in another colour (docs/user "Console pages"): Left and Right on a console page choose it, the choice is
//! saved, and every place that shows the console shows that colour: the shelf card, the console page, the insert scene
//! (with the TV on the cabinet). A console with one colour only does nothing; a saved colour this version does not know
//! shows the original, never a gap. On the made-up library (scripts/curator-fake-library.py), as tests/cozy_extras.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child, PathBuf);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    lib
}

/// The picture of `state` below the status corner (it shows the clock); what it saves stays in `data`.
fn shot(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, state: &str, lang: &str) -> Vec<u8> {
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!("shelf");
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let png = tmp.join(format!("{lang}-{}.png", state.replace([':', ' ', ',', '='], "_")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    let rendered = if let Some(s) = state.strip_suffix(":insert") {
        cmd.env("POCKETUI_SNAPSHOT_LAUNCH_MS", "900");
        s
    } else {
        state
    };
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", rendered, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_ROOM_TIME", "07:00")
        .env("POCKETUI_SNAPSHOT_SIZE", "1240x1080")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf[(info.width * 110 * 3) as usize..info.buffer_size()].to_vec()
}

fn saved(data: &Path) -> serde_json::Value {
    let s: serde_json::Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).unwrap()).unwrap();
    s["console_colours"].clone()
}

fn changed(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(a, b)| a != b).count()
}

/// Where each place that shows a console is looked at: the shelf card, the Split look's card, the console page, the start
/// (the cartridge or disc goes into the console; the cabinet's TV stands beside it).
fn views_of(system: &str) -> Vec<String> {
    ["shelf:{}:shelf", "shelf:{}:split", "console:{}", "shelf:{}:cabinet:games:0:insert"].iter().map(|v| v.replace("{}", system)).collect()
}

/// The Advance's.
static VIEWS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| views_of("gba"));

fn views(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>) -> Vec<Vec<u8>> {
    views_in(tmp, lib, data, stub, &VIEWS)
}

fn views_in(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, of: &[String]) -> Vec<Vec<u8>> {
    of.iter().map(|v| shot(tmp, lib, data, stub, v, "en")).collect()
}

/// The start is caught while the cartridge moves: a few pixels of it differ from run to run, a colour is far more.
fn minimum(view: &str) -> usize {
    if view.ends_with(":insert") { 15_000 } else { 300 }
}

#[test]
fn right_and_left_on_the_console_page_choose_the_colour_and_every_view_shows_it() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let original = views(tmp.path(), &lib, &data, &stub);
    assert!(saved(&data).is_null(), "as it starts: nothing saved");
    // Right: the first colour; saved at once, and every place that shows the console shows it.
    shot(tmp.path(), &lib, &data, &stub, "console:gba:keys=r", "en");
    assert_eq!(saved(&data), serde_json::json!({"gba": "milky"}));
    let milky = views(tmp.path(), &lib, &data, &stub);
    for ((view, was), now) in VIEWS.iter().zip(&original).zip(&milky) {
        assert!(changed(was, now) > minimum(view), "{view}: Milky Blue replaces the original ({} bytes changed)", changed(was, now));
    }
    // Right again: the second colour, another picture than the first.
    shot(tmp.path(), &lib, &data, &stub, "console:gba:keys=r", "en");
    assert_eq!(saved(&data), serde_json::json!({"gba": "pink"}));
    let pink = views(tmp.path(), &lib, &data, &stub);
    for ((view, was), now) in VIEWS.iter().zip(&milky).zip(&pink) {
        assert!(changed(was, now) > minimum(view), "{view}: Pink is not Milky Blue ({} bytes changed)", changed(was, now));
    }
    // Right once more: round to the original, which is saved as nothing and looks as it did.
    shot(tmp.path(), &lib, &data, &stub, "console:gba:keys=r", "en");
    assert!(saved(&data).as_object().is_none_or(|m| !m.contains_key("gba")), "the original is not written down: {}", saved(&data));
    for ((view, was), now) in VIEWS.iter().zip(&original).zip(views(tmp.path(), &lib, &data, &stub)) {
        assert!(changed(was, &now) <= minimum(view) / 10, "{view}: back to the original ({} bytes differ)", changed(was, &now));
    }
    // Left from the original goes the other way round: to the last colour.
    shot(tmp.path(), &lib, &data, &stub, "console:gba:keys=l", "en");
    assert_eq!(saved(&data), serde_json::json!({"gba": "pink"}));
}

#[test]
fn colours_are_per_console_and_a_console_with_one_colour_goes_to_it_and_back() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let gba = shot(tmp.path(), &lib, &data, &stub, "shelf:gba:shelf", "en");
    let gbc = shot(tmp.path(), &lib, &data, &stub, "shelf:gbc:shelf", "en");
    shot(tmp.path(), &lib, &data, &stub, "console:gbc:keys=r", "en");
    assert_eq!(saved(&data), serde_json::json!({"gbc": "dandelion"}));
    assert!(changed(&gbc, &shot(tmp.path(), &lib, &data, &stub, "shelf:gbc:shelf", "en")) > 300, "the Game Boy Color is yellow now");
    assert_eq!(changed(&gba, &shot(tmp.path(), &lib, &data, &stub, "shelf:gba:shelf", "en")), 0, "the Advance is not touched");
    shot(tmp.path(), &lib, &data, &stub, "console:gbc:keys=r", "en");
    assert!(saved(&data).as_object().is_none_or(|m| m.is_empty()), "Right again is the original: {}", saved(&data));
}

#[test]
fn a_console_that_has_no_other_colour_ignores_left_and_right() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let page = shot(tmp.path(), &lib, &data, &stub, "console:snes", "en");
    let after = shot(tmp.path(), &lib, &data, &stub, "console:snes:keys=rl", "en");
    assert_eq!(changed(&page, &after), 0, "the page is the same");
    assert!(saved(&data).is_null(), "and nothing is saved: {}", saved(&data));
}

#[test]
fn a_saved_colour_this_version_does_not_know_shows_the_original() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let original = views(tmp.path(), &lib, &data, &stub);
    // A newer version saved a colour this one does not have, and a console it does not know.
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).unwrap_or_else(|_| b"{}".to_vec())).unwrap();
    json["console_colours"] = serde_json::json!({"gba": "neon", "zzz": "pink"});
    std::fs::write(data.join("settings.json"), json.to_string()).unwrap();
    for ((view, was), now) in VIEWS.iter().zip(&original).zip(views(tmp.path(), &lib, &data, &stub)) {
        assert!(changed(was, &now) <= minimum(view) / 10, "{view}: the original, not a gap ({} bytes differ)", changed(was, &now));
    }
    // Right starts from the original: the first colour.
    shot(tmp.path(), &lib, &data, &stub, "console:gba:keys=r", "en");
    assert_eq!(saved(&data), serde_json::json!({"gba": "milky"}));
}

/// Every colour a console has, one after the other with Right: each is saved, shows in all four places that show the console,
/// differs from the colour before it, and the round ends on the original, which looks as it did and is not written down.
fn walk_the_colours(system: &str, colours: &[&str]) {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let of = views_of(system);
    let original = views_in(tmp.path(), &lib, &data, &stub, &of);
    let mut before = original.clone();
    for colour in colours {
        shot(tmp.path(), &lib, &data, &stub, &format!("console:{system}:keys=r"), "en");
        assert_eq!(saved(&data), serde_json::json!({ system: colour }), "{system}: Right chose {colour}");
        let now = views_in(tmp.path(), &lib, &data, &stub, &of);
        for ((view, was), now) in of.iter().zip(&before).zip(&now) {
            assert!(changed(was, now) > minimum(view), "{view}: {colour} replaces what was there ({} bytes changed)", changed(was, now));
        }
        before = now;
    }
    shot(tmp.path(), &lib, &data, &stub, &format!("console:{system}:keys=r"), "en");
    assert!(saved(&data).as_object().is_none_or(|m| !m.contains_key(system)), "{system}: the original is not written down: {}", saved(&data));
    for ((view, was), now) in of.iter().zip(&original).zip(views_in(tmp.path(), &lib, &data, &stub, &of)) {
        assert!(changed(was, &now) <= minimum(view) / 10, "{system} {view}: back to the original ({} bytes differ)", changed(was, &now));
    }
    // Left from the original goes to the last colour.
    shot(tmp.path(), &lib, &data, &stub, &format!("console:{system}:keys=l"), "en");
    assert_eq!(saved(&data), serde_json::json!({ system: colours.last().unwrap() }), "{system}: Left from the original");
}

#[test]
fn the_nintendo_64_comes_in_ice_blue_and_jungle_green_in_every_view() {
    walk_the_colours("n64", &["iceblue", "jungle"]);
}

#[test]
fn the_gamecube_comes_in_jet_black_in_every_view() {
    walk_the_colours("gamecube", &["jetblack"]);
}
