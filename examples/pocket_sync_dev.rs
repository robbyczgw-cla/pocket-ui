//! Pocket-to-Pocket sync by hand, for tests between real devices and a computer (docs/SYNC-OWN.md): the library's
//! service and sync (`pocket_ui::pocket_sync::net`) on folders you name, never pocket-ui's own. Not shipped.
//!
//!   pocket_sync_dev serve <roms> <data> <name> [port] [--code] [--busy-file F]   service (+ a pairing code), until killed
//!   pocket_sync_dev pair  <roms> <data> <name> <host:port> <code> [my port]     pair with a Pocket showing <code>
//!   pocket_sync_dev sync  <roms> <data> <name> [peer name or id]                 sync with every (or one) paired Pocket
//!   pocket_sync_dev index <roms> <data> <name>                                   this side's saves, vectors and questions
//!   pocket_sync_dev find                                                         who answers on the Wi-Fi and in the tailnet
//!   pocket_sync_dev choose <roms> <data> <name> <rel> keep|copy                  answer the question about <rel>
//!
//! `POCKET_SYNC_SAVES=<dir>`: the saves in a folder of their own, `<dir>/<system>/` (the Brick, dArkOS), not beside the games.

use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::pocket_sync::net::{self, Me, Pairing, Peers, Service, Store};
use pocket_ui::pocket_sync::{vv_str, Layout};

fn store(a: &[String]) -> Store {
    let roms = PathBuf::from(&a[1]);
    let data = PathBuf::from(&a[2]);
    let me = Me::load_or_new(&data, Some(&a[3])).expect("device.json");
    let layout = match std::env::var_os("POCKET_SYNC_SAVES") {
        Some(saves) => Layout::saves_root(roms, PathBuf::from(saves)),
        None => Layout::beside(roms),
    };
    Store::new(data, layout, me)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let cmd = a.first().map(String::as_str).unwrap_or("");
    let flag = |f: &str| a.iter().position(|x| x == f);
    match cmd {
        "serve" if a.len() >= 4 => {
            let port: u16 = a.get(4).and_then(|p| p.parse().ok()).unwrap_or(net::PORT);
            let busy_file = flag("--busy-file").and_then(|i| a.get(i + 1)).map(PathBuf::from);
            let st = Arc::new(store(&a));
            let listener = TcpListener::bind(("0.0.0.0", port)).expect("bind");
            let busy = Box::new(move || busy_file.as_deref().is_some_and(Path::exists));
            let svc = Arc::new(Service::new(st.clone(), busy, port));
            if flag("--code").is_some() {
                let p = Pairing::new().expect("random");
                println!("PAIR CODE {} for {} \"{}\"", p.code, st.me.id, st.me.name);
                *svc.pairing.lock().unwrap() = Some(p);
            }
            let stop = Arc::new(AtomicBool::new(false));
            if let Ok(u) = UdpSocket::bind(("0.0.0.0", net::DISCOVERY_PORT)) {
                let (s, t) = (svc.clone(), stop.clone());
                std::thread::spawn(move || net::answer_discovery(u, s, t));
            }
            println!("serving {} on port {port}", st.layout.roms.display());
            net::serve(listener, svc, stop);
        }
        "pair" if a.len() >= 6 => {
            let st = store(&a);
            let addr: SocketAddr = a[4].parse().expect("host:port");
            let my_port = a.get(6).and_then(|p| p.parse().ok()).unwrap_or(net::PORT);
            let mut peers = Peers::load(&st.dir);
            match net::pair(&st, &mut peers, addr, &a[5], my_port) {
                Ok(p) => println!("paired with {} \"{}\"", p.id, p.name),
                Err(e) => {
                    println!("not paired: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        "sync" if a.len() >= 4 => {
            let st = store(&a);
            let peers = Peers::load(&st.dir);
            let want = a.get(4);
            for p in peers.0.iter().filter(|p| want.is_none_or(|w| *w == p.name || *w == p.id)) {
                let t = Instant::now();
                match net::sync_with(&st, p, &|| false) {
                    Ok(r) => println!(
                        "{}: sent {:?} fetched {:?} asked {:?} offered {:?} waiting {} same {} failed {:?} no place {} in {} ms",
                        p.name, r.sent, r.fetched, r.asked, r.offered, r.waiting, r.same, r.failed, r.no_place, t.elapsed().as_millis()
                    ),
                    Err(e) => println!("{}: not synced: {e:?} ({} ms)", p.name, t.elapsed().as_millis()),
                }
            }
        }
        "index" if a.len() >= 4 => {
            let st = store(&a);
            let t = Instant::now();
            for e in st.index() {
                let marks: Vec<String> = e.marks.iter().map(vv_str).collect();
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}{}",
                    e.rel,
                    e.crc.map_or("-".into(), |c| format!("{c:08x}")),
                    e.mtime,
                    &e.sha[..12],
                    vv_str(&e.vv),
                    e.asking.as_ref().map_or(String::new(), |q| format!("asking {} ", vv_str(q))),
                    if marks.is_empty() { String::new() } else { format!("in {}", marks.join(" ")) }
                );
            }
            eprintln!("index in {} ms", t.elapsed().as_millis());
        }
        "find" => {
            // The Wi-Fi's broadcast first, then every online device of the tailnet (as the pairing view asks).
            let mut to = vec![SocketAddr::from(([255, 255, 255, 255], net::DISCOVERY_PORT))];
            to.extend(pocket_ui::tailscale::online_peers(&pocket_ui::tailscale::Tools::default()).into_iter().map(|ip| SocketAddr::from((ip, net::DISCOVERY_PORT))));
            eprintln!("asking {} addresses", to.len());
            for f in net::discover_at(&to, Duration::from_secs(2)) {
                println!("{} \"{}\" at {}{}", f.id, f.name, f.addr, if f.pairing { " (shows a code)" } else { "" });
            }
        }
        "choose" if a.len() >= 6 => {
            let st = store(&a);
            let rel = &a[4];
            let save = st.layout.file(rel).expect("a save path with its game here");
            let dir = save.parent().expect("folder").to_path_buf();
            let file = save.file_name().and_then(|n| n.to_str()).expect("name").to_string();
            let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s).to_string();
            let mut copies: Vec<PathBuf> = std::fs::read_dir(&dir)
                .expect("folder")
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("{stem}.sync-conflict-"))))
                .collect();
            copies.sort();
            // Play before the choice counts as play, the choice itself not.
            let _ = st.index();
            let backup = st.backup_dir();
            std::fs::create_dir_all(&backup).expect("backup folder");
            if a[5] == "copy" {
                let c = copies.remove(0);
                std::fs::rename(&save, backup.join(&file)).expect("keep the old save");
                std::fs::rename(&c, &save).expect("put the copy in place");
            }
            for c in copies {
                let _ = std::fs::rename(&c, backup.join(c.file_name().unwrap()));
            }
            st.chose(rel);
            println!("chose {} for {rel}", a[5]);
        }
        _ => {
            eprintln!("see the top of examples/pocket_sync_dev.rs");
            std::process::exit(2);
        }
    }
}
