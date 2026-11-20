//! Pocket Sync's pages fit (Settings → Connections → Pocket Sync, `app_pocket_sync.rs`): every state of the page (the
//! list with each Pocket's line, pairing, typing a code, "What Pocket Sync syncs", a Pocket's own page) in every language,
//! in Normal and Large text, on the smallest screen (the RG35XX Plus, 640x480) and the Brick's (1024x768). No line is cut
//! short, runs past the edge or breaks inside a word; only the made-up handhelds' and games' names may end in "…" (a
//! name fits where it ends, `Text::fit`).
use std::{path::PathBuf, process::{Child, Command}};
include!("common/root.rs");

struct Stub(Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The snapshot's made-up names (`App::pocket_sync_snapshot`): a name may be cut where it ends.
const NAMES: [&str; 7] =
    ["RG35XX Plus", "TrimUI Brick", "Retroid Pocket Mini V2", "RG35XX SP", "Golden Sun", "Pokémon Gold Version", "The Legend of Zelda: The Minish Cap"];

const STATES: [&str; 15] = [
    "pockets:off", "pockets:on", "pockets:none", "pockets:pair", "pockets:nobody", "pockets:code", "pockets:what", "pockets:forget",
    "pockets:taken", "pockets:faults", "pockets:faults:7", "pockets:syncing:3", "pockets:pocket", "pockets:pocket-empty", "pockets:on:2",
];

#[test]
fn every_pocket_sync_page_fits_in_every_language_and_text_size() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("wayland");
    let _stub = Stub(Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().unwrap());
    test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub");
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")));
    let mut jobs = Vec::new();
    for (device, size) in [("rg35xx-plus", "640x480"), ("brick", "1024x768")] {
        for lang in ["en", "de", "es", "pt", "fr", "it"] {
            for text in ["normal", "large"] {
                jobs.push((device, size, lang, text));
            }
        }
    }
    let problems = std::sync::Mutex::new(Vec::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..6 {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(device, size, lang, text)) = jobs.get(i) else { return };
                for state in STATES {
                    let name = format!("{device}-{lang}-{text}-{}", state.replace(':', "_"));
                    let misses = tmp.path().join(format!("{name}.misses"));
                    let out = Command::new(&bin)
                        .args(["--snapshot", state, tmp.path().join(format!("{name}.png")).to_str().unwrap()])
                        .env("WAYLAND_DISPLAY", &socket)
                        .env("POCKETUI_MOCK", "1")
                        .env("POCKETUI_DEVICE", device)
                        .env("POCKETUI_SNAPSHOT_SIZE", size)
                        .env("POCKETUI_LANG", lang)
                        .env("POCKETUI_TEXT_SIZE", text)
                        .env("POCKETUI_ES_SYSTEMS", tmp.path().join("none.cfg"))
                        .env("POCKETUI_DATA", tmp.path().join(format!("data-{name}")))
                        .env("POCKETUI_I18N_MISSES", &misses)
                        .output()
                        .unwrap();
                    let mut p = problems.lock().unwrap();
                    if !out.status.success() {
                        p.push(format!("{name}: exit {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr)));
                        continue;
                    }
                    for line in std::fs::read_to_string(&misses).unwrap_or_default().lines() {
                        let bad = match line.split_once('\t') {
                            Some(("C", s)) => !NAMES.contains(&s),
                            Some(("X", _)) | Some(("W", _)) => true,
                            _ => false,
                        };
                        if bad {
                            p.push(format!("{name}: {line}"));
                        }
                    }
                }
            });
        }
    });
    let problems = problems.into_inner().unwrap();
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}
