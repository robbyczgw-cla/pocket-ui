//! "Send from your phone"'s server on its own, for checking it on a device with curl (docs/PHONE-UPLOAD.md):
//!
//! ```text
//! phone_upload_dev --roms <dir> [--es-systems <es_systems.cfg>] [--ip <address>] [--seconds <n>]
//! ```
//!
//! The real server (`Server::on_wifi`: the Wi-Fi interface, `SO_BINDTODEVICE`, a fresh code) on `--ip` or the first
//! address of a `wlan*` interface, writing into `--roms` (a test folder: the systems of es_systems.cfg, each pointed
//! at its folder name under it). Prints the address, then every two seconds the status and the process's RSS, and
//! ends when the server closes by itself (idle, sleep, guessing) or after `--seconds`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::library::SysInfo;
use pocket_ui::phone_upload::{Config, Server, IDLE};
use pocket_ui::web::files::Roots;

fn rss_kb() -> String {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| s.lines().find(|l| l.starts_with(k)).map(|l| l[k.len()..].trim().to_string()).unwrap_or_default();
    format!("VmRSS {} VmHWM {} Threads {}", get("VmRSS:"), get("VmHWM:"), get("Threads:"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let roms = PathBuf::from(arg("--roms").expect("--roms <dir>"));
    std::fs::create_dir_all(&roms).expect("roms folder");
    let es = arg("--es-systems").unwrap_or_else(|| "/storage/.config/emulationstation/es_systems.cfg".into());
    let mut systems: Vec<SysInfo> = pocket_ui::library::all_systems(std::path::Path::new(&es)).unwrap_or_default();
    if systems.is_empty() {
        let s = |id: &str, name: &str, exts: &[&str]| SysInfo { id: id.into(), name: name.into(), folder: format!("roms/{id}"), path: PathBuf::new(), extensions: exts.iter().map(|e| e.to_string()).collect() };
        systems = vec![s("gb", "Game Boy", &["gb", "zip"]), s("gba", "Game Boy Advance", &["gba", "zip"]), s("psx", "PlayStation", &["cue", "bin", "chd", "iso", "m3u"])];
    }
    for s in &mut systems {
        let folder = s.path.file_name().map(|f| f.to_os_string()).unwrap_or_else(|| s.id.clone().into());
        s.path = roms.join(folder);
    }
    let ip = arg("--ip").unwrap_or_else(|| {
        pocket_ui::web::net::interfaces()
            .into_iter()
            .find(|i| i.name.starts_with("wlan") && i.addr.is_ipv4())
            .map(|i| i.addr.to_string())
            .expect("no wlan address; give --ip")
    });
    let cfg = Config { roots: Roots::under(&roms), systems, idle: IDLE, notify: Arc::new(|| {}) };
    let server = Server::on_wifi(&ip, cfg).expect("server");
    println!("URL {}", server.url());
    println!("start {}", rss_kb());
    let limit = arg("--seconds").and_then(|s| s.parse().ok()).map(Duration::from_secs);
    let t0 = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let st = server.status();
        let files: Vec<String> = st.files.iter().map(|f| format!("{} {}/{} {:?}", f.name, f.done, f.total, f.state)).collect();
        println!("t={}s connected={} added={} closed={:?} {} | {}", t0.elapsed().as_secs(), st.connected, st.added, st.closed, rss_kb(), files.join("; "));
        if st.closed.is_some() || limit.is_some_and(|l| t0.elapsed() >= l) {
            break;
        }
    }
    drop(server);
    println!("stopped {}", rss_kb());
}
