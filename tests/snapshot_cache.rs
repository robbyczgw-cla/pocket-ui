//! Real child processes exercise shared pixels/probes, changed inputs, concurrent
//! misses, damaged entries and unsuccessful renders. No renderer is mocked by a
//! cache hit unless its original complete response has actually been produced.
#![cfg(unix)]
use std::{fs, path::Path, process::Command};
#[path = "common/snapshot.rs"]
mod snapshot;
use snapshot::SnapshotCommand;

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("roms")).unwrap();
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("roms/game.gb"), "original ROM").unwrap();
    fs::write(root.join("es_systems.cfg"), format!("<systemList><system><fullname>Game Boy</fullname><path>{}</path></system></systemList>", root.join("roms").display())).unwrap();
    fs::write(root.join("data/settings.json"), format!(r#"{{"label":"alpha","rom":"{}/roms/game.gb"}}"#, root.display())).unwrap();
}
fn command(program: &Path, root: &Path, count: &Path, output: &Path, state: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(["--snapshot", state]).arg(output)
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", "1794139200")
        .env("POCKETUI_DATA", root.join("data")).env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
        .env("POCKETUI_LANG", "en").env("POCKETUI_FONT", "barlow")
        .env("SNAPSHOT_FIXTURE_CALLS", count);
    for (key, suffix) in [("POCKETUI_TEXT_BOXES", "boxes"), ("POCKETUI_I18N_MISSES", "misses"),
        ("POCKETUI_VISIBLE_TEXT", "visible"), ("POCKETUI_NAV_PROBE", "nav"),
        ("POCKETUI_TITLE_BLOCKS", "titles"), ("POCKETUI_SPINE_LINES", "spines")] {
        cmd.env(key, output.with_extension(suffix));
    }
    cmd
}
fn calls(path: &Path) -> usize { fs::read_to_string(path).unwrap_or_default().lines().count() }
struct Env(Option<std::ffi::OsString>);
impl Drop for Env { fn drop(&mut self) {
    if let Some(value) = &self.0 { std::env::set_var("POCKETUI_SNAPSHOT_CACHE_DIR", value); }
    else { std::env::remove_var("POCKETUI_SNAPSHOT_CACHE_DIR"); }
} }

#[test]
fn shared_snapshots_keep_the_actual_pixels_and_probes_and_never_reuse_changed_or_failed_inputs() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cache");
    let _env = Env(std::env::var_os("POCKETUI_SNAPSHOT_CACHE_DIR"));
    std::env::set_var("POCKETUI_SNAPSHOT_CACHE_DIR", &cache);
    let program = tmp.path().join("renderer");
    fs::write(&program, r#"#!/usr/bin/env python3
import hashlib, json, os, pathlib, struct, sys, zlib, xml.etree.ElementTree as E
with open(os.environ['SNAPSHOT_FIXTURE_CALLS'], 'a') as f: f.write('render\n')
i = sys.argv.index('--snapshot')
state, output = sys.argv[i+1:i+3]
cfg = E.parse(os.environ['POCKETUI_ES_SYSTEMS'])
roms = pathlib.Path(cfg.find('.//path').text)
settings = json.loads((pathlib.Path(os.environ['POCKETUI_DATA'])/'settings.json').read_text())
words = '|'.join([state, settings['label'], (roms/'game.gb').read_text(), cfg.find('.//fullname').text,
                  os.environ.get('POCKETUI_LANG', ''), os.environ.get('POCKETUI_FONT', '')])+'\n'
for name in ['POCKETUI_TEXT_BOXES','POCKETUI_I18N_MISSES','POCKETUI_VISIBLE_TEXT','POCKETUI_NAV_PROBE','POCKETUI_TITLE_BLOCKS','POCKETUI_SPINE_LINES']:
    if name in os.environ: pathlib.Path(os.environ[name]).write_text(words)
def chunk(kind, data): return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
png = b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR', struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0'+hashlib.sha256(words.encode()).digest()[:3]))+chunk(b'IEND',b'')
pathlib.Path(output).write_bytes(png)
if state=='mutate': (roms/'game.gb').write_text('unexpected mutation')
if state=='failed': sys.exit(1)
"#).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let (one, two) = (tmp.path().join("one"), tmp.path().join("two"));
    fixture(&one); fixture(&two);
    let count = tmp.path().join("calls");
    let (a, b) = (tmp.path().join("a.png"), tmp.path().join("b.png"));
    let mut first = command(&program, &one, &count, &a, "shelf:gb");
    first.env("POCKETUI_TEXT_BOXES", tmp.path().join("a.box"));
    assert!(first.snapshot_output().unwrap().status.success());
    let mut second = command(&program, &two, &count, &b, "shelf:gb");
    second.env("POCKETUI_VISIBLE_TEXT", tmp.path().join("b.words"));
    assert!(second.snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), 1, "separate fixture roots and record consumers share one real render");
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    assert_eq!(fs::read(tmp.path().join("a.box")).unwrap(), fs::read(tmp.path().join("b.words")).unwrap());
    let prior = calls(&count);
    let mut other_probes = command(&program, &one, &count, &b, "shelf:gb");
    other_probes.env_remove("POCKETUI_SPINE_LINES");
    assert!(other_probes.snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), prior + 1, "another recorder set is a distinct renderer input");
    for (key, value) in [("POCKETUI_LANG", "de"), ("POCKETUI_FONT", "pixel"), ("POCKETUI_TEXT_SIZE", "large"), ("POCKETUI_DEVICE", "rg35xx-plus"), ("POCKETUI_SNAPSHOT_SIZE", "640x480"), ("POCKETUI_RETRO", "cocoa"), ("POCKETUI_MOCK_NOW", "1794225600"), ("POCKETUI_FUTURE_RENDER_KNOB", "different")] {
        let prior = calls(&count);
        let mut changed = command(&program, &one, &count, &b, "shelf:gb");
        changed.env(key, value);
        assert!(changed.snapshot_output().unwrap().status.success());
        assert_eq!(calls(&count), prior + 1, "changed {key} must render");
    }
    let prior = calls(&count);
    for _ in 0..2 {
        let mut uncached = command(&program, &one, &count, &b, "shelf:gb");
        uncached.env("POCKETUI_SNAPSHOT_CACHE_DISABLE", "1");
        assert!(uncached.snapshot_output().unwrap().status.success());
    }
    assert_eq!(calls(&count), prior + 2, "a child-local bypass always executes the renderer");
    fs::write(two.join("data/settings.json"), r#"{"label":"beta"}"#).unwrap();
    let prior = calls(&count);
    assert!(command(&program, &two, &count, &b, "shelf:gb").snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), prior + 1, "changed player settings must render");
    fs::write(two.join("roms/game.gb"), "replacement ROM").unwrap();
    let prior = calls(&count);
    let mut changed = command(&program, &two, &count, &b, "shelf:gb");
    changed.env("POCKETUI_VISIBLE_TEXT", tmp.path().join("new.words"));
    assert!(changed.snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), prior + 1, "changed library bytes must render");
    assert!(fs::read_to_string(tmp.path().join("new.words")).unwrap().contains("replacement ROM"));
    let first_row: serde_json::Value = serde_json::from_str(fs::read_to_string(cache.join("stats.jsonl")).unwrap().lines().next().unwrap()).unwrap();
    let first_entry = cache.join("renders").join(first_row["key"].as_str().unwrap());
    fs::write(first_entry.join("png"), b"truncated").unwrap();
    let prior = calls(&count);
    assert!(command(&program, &one, &count, &b, "shelf:gb").snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), prior + 1, "damaged cached pixels must be rendered again");
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    let mut sidecar = command(&program, &one, &count, &b, "shelf:gb");
    sidecar.env("POCKETUI_VISIBLE_TEXT", tmp.path().join("restored.words"));
    fs::write(first_entry.join("visible"), b"damaged recorder").unwrap();
    let prior = calls(&count);
    assert!(sidecar.snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), prior + 1, "damaged probe data must be rendered again too");
    assert!(!fs::read_to_string(tmp.path().join("restored.words")).unwrap().contains("damaged recorder"));
    let prior = calls(&count);
    for _ in 0..2 { assert!(!command(&program, &one, &count, &b, "failed").snapshot_output().unwrap().status.success()); }
    assert_eq!(calls(&count), prior + 2, "failure may never turn into a cached success");
    let prior = calls(&count);
    std::thread::scope(|scope| {
        for i in 0..4 {
            let (program, one, count, out) = (&program, &one, &count, tmp.path().join(format!("parallel-{i}.png")));
            scope.spawn(move || { assert!(command(program, one, count, &out, "parallel").snapshot_output().unwrap().status.success()); });
        }
    });
    assert_eq!(calls(&count), prior + 1, "concurrent consumers wait for one complete render");
    let old = fs::read(&program).unwrap(); let mut new = old; new.extend_from_slice(b"\n# another executable\n"); fs::write(&program, new).unwrap();
    let prior = calls(&count);
    assert!(command(&program, &one, &count, &b, "shelf:gb").snapshot_output().unwrap().status.success());
    assert_eq!(calls(&count), prior + 1, "another binary invalidates all previous pixels");
    assert!(command(&program, &one, &count, &b, "mutate").snapshot_output().unwrap_err().to_string().contains("modified a shared fixture"));
    real_renderer_keeps_its_pixels_and_each_probe(tmp.path());
}

// Compare the actual renderer against the cached path, not just a synthetic
// producer. All outputs must agree before a hit is allowed to serve another test.
fn real_renderer_keeps_its_pixels_and_each_probe(tmp: &Path) {
    use std::time::Duration;
    struct Stub(std::process::Child);
    impl Drop for Stub { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let socket = tmp.join("wayland");
    let _stub = if cfg!(target_os = "linux") {
        let child = Command::new("python3").arg("-I").arg(source.join("scripts/wayland-stub.py")).arg(&socket).spawn().unwrap();
        let stub = Stub(child);
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !socket.exists() { assert!(std::time::Instant::now() < until); std::thread::sleep(Duration::from_millis(10)); }
        Some(stub)
    } else { None };
    for (n, state) in ["shelf:gb:split", "curator:home", "settings", "setup:wifi-empty"].into_iter().enumerate() {
        let mut outputs: Vec<Vec<Option<Vec<u8>>>> = Vec::new();
        for mode in ["plain", "cached", "hit"] {
            let root = tmp.join(format!("real-{n}-{mode}"));
            assert!(Command::new("python3").arg("-I").arg(source.join("scripts/curator-fake-library.py"))
                .arg(&root).arg("dark").env("POCKETUI_MOCK_NOW", "1794139200").status().unwrap().success());
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
            cmd.args(["--snapshot", state]).arg(root.join("shot.png"))
                .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", "1794139200")
                .env("POCKETUI_DATA", root.join("data")).env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
                .env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_LANG", "de")
                .env("POCKETUI_FONT", if n % 2 == 0 { "barlow" } else { "pixel" });
            if _stub.is_some() { cmd.env("WAYLAND_DISPLAY", &socket); }
            let records = [("POCKETUI_TEXT_BOXES", "boxes"), ("POCKETUI_I18N_MISSES", "misses"),
                ("POCKETUI_VISIBLE_TEXT", "visible"), ("POCKETUI_NAV_PROBE", "nav"),
                ("POCKETUI_TITLE_BLOCKS", "titles"), ("POCKETUI_SPINE_LINES", "spines")];
            for (key, file) in records.iter().take(if n % 2 == 0 { 2 } else { 6 }) { cmd.env(*key, root.join(file)); }
            let out = if mode == "plain" { cmd.output() } else { cmd.snapshot_output() }.unwrap();
            assert!(out.status.success(), "{state} {mode}: {}", String::from_utf8_lossy(&out.stderr));
            let files: Vec<_> = std::iter::once("shot.png").chain(records.iter().map(|(_, file)| *file))
                .map(|file| fs::read(root.join(file)).ok()).collect();
            assert!(files[0].is_some(), "renderer must produce actual pixels");
            if n == 1 {
                assert!(String::from_utf8_lossy(files[3].as_ref().expect("visible text")).contains("07:00"),
                    "the snapshot clock must show fixture noon in the mock hardware zone (New York): {}",
                    String::from_utf8_lossy(files[3].as_ref().unwrap()));
            }
            if let Some(first) = outputs.first() {
                if &files != first {
                    let evidence = std::env::temp_dir().join(format!("snapshot-equivalence-{}", std::process::id()));
                    fs::create_dir_all(&evidence).unwrap();
                    for (i, (expected, actual)) in first.iter().zip(&files).enumerate() {
                        if let Some(bytes) = expected { fs::write(evidence.join(format!("{n}-{i}-plain")), bytes).unwrap(); }
                        if let Some(bytes) = actual { fs::write(evidence.join(format!("{n}-{i}-{mode}")), bytes).unwrap(); }
                        if expected != actual { eprintln!("different recorder {i}, {state} {mode}"); }
                    }
                    panic!("real PNG and every probe must agree: {state} {mode}; evidence {}", evidence.display());
                }
            }
            outputs.push(files);
        }
    }
}
