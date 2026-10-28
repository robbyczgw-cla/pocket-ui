//! The web companion on this computer, with a made-up library, for working on the page and for screenshots:
//!
//! ```text
//! cargo run --release --example web_dev -- [--port 8737] [--playing] [--dir /tmp/pocketui-web-dev]
//! ```
//!
//! It serves `http://127.0.0.1:<port>` as the owner's tailnet device would see it (no pairing; a stand-in for
//! tailscaled says so) and `http://127.0.0.1:<port + 1>` as another user's device on the tailnet would (the pairing
//! page; the code is printed). `--empty` starts with no games at all, as on a new card. The Stats page gets made-up
//! play history over the made-up games (`play_history::demo`); `--no-history` leaves it out, as with Play history off. Edits are written to the made-up data folder. Development only: these sockets are bound by address,
//! without the interface binding and the address checks of the real server (`web::server::Server::start` with a fixed
//! list).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::battery::Sample;
use pocket_ui::battery_history::History;
use pocket_ui::library_meta::{LibraryMeta, Status};
use pocket_ui::web::api::{Ctx, Tailnet};
use pocket_ui::web::net::{Listen, SelfNode, Whois};
use pocket_ui::web::{auth::Pairing, files::Roots, Edit, NowPlaying, Prefs, Shared};

/// The made-up play history, worked out once at the start.
static HISTORY: std::sync::OnceLock<Option<serde_json::Value>> = std::sync::OnceLock::new();

fn dev_history(_data_dir: &Path) -> Option<serde_json::Value> {
    HISTORY.get().cloned().flatten()
}

/// tailscaled as the first port's server sees it (the owner's laptop) and the second's (a friend's phone).
struct Fake {
    owner: bool,
}

impl Tailnet for Fake {
    fn self_node(&self) -> Option<SelfNode> {
        Some(SelfNode { user_id: 1, login: "alex@example.com".into(), tagged: false, hostname: "pocket".into(), dns_name: "pocket.tail1234.ts.net".into() })
    }
    fn whois(&self, _peer: SocketAddr) -> Option<Whois> {
        let (user_id, login, device) = if self.owner { (1, "alex@example.com", "macbook") } else { (2, "friend@example.com", "friends-phone") };
        Some(Whois { user_id, login: login.into(), device: device.into(), node: format!("n{user_id}"), tagged: false })
    }
}

const GAMES: &[(&str, &str, &[&str])] = &[
    ("gba", "Game Boy Advance", &["Advance Wars (USA).gba", "F-Zero - Maximum Velocity (USA, Europe).gba", "Golden Sun (USA, Europe).gba", "Metroid Fusion (USA).gba", "Mother 3 (Japan).gba", "Wario Ware, Inc. - Mega Microgame$! (USA).gba"]),
    ("gb", "Game Boy", &["Tetris (World) (Rev 1).gb", "Super Mario Land (World).gb", "Kirby's Dream Land (USA, Europe).gb", "Link's Awakening (USA).gb"]),
    ("snes", "Super Nintendo", &["Chrono Trigger (USA).sfc", "Super Metroid (Japan, USA).sfc", "Super Mario Kart (USA).sfc", "F-Zero (USA).sfc", "EarthBound (USA).sfc"]),
    ("nes", "Nintendo Entertainment System", &["Nova the Squirrel.nes", "Super Mario Bros. 3 (USA).nes", "Kirby's Adventure (USA).nes"]),
    ("psx", "PlayStation", &["Chrono Cross (USA) (Disc 1).chd", "Vagrant Story (USA).chd"]),
    ("fds", "Famicom Disk System", &[]),
];

fn cover(path: &Path, rgb: u32) {
    let (w, h) = (120u32, 168u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let edge = x < 10 || y < 10 || x >= w - 10 || y >= h - 10;
            let band = (40..70).contains(&y);
            let c = if edge { 0x202020 } else if band { 0xece9e1 } else { rgb };
            let shade = 1.0 - (y as f32 / h as f32) * 0.35;
            px.extend([((c >> 16) & 255) as f32 * shade, ((c >> 8) & 255) as f32 * shade, (c & 255) as f32 * shade].map(|v| v as u8));
        }
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    std::fs::write(path, out).unwrap();
}

fn setup(dir: &Path, empty: bool) -> (PathBuf, PathBuf, PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
    let roms = dir.join("roms");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let mut cfg = String::from("<systemList>");
    let mut meta = LibraryMeta::load(&data);
    let couch = meta.add_sticker("Couch co-op").unwrap();
    let bed = meta.add_sticker("Before bed").unwrap();
    let mut recents = pocket_ui::recents::Recents::load(&data);
    let mut favs = pocket_ui::favorites::Favorites::load(&data);
    let now = pocket_ui::recents::now_secs();
    for (i, (id, name, games)) in GAMES.iter().enumerate() {
        let folder = roms.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        let ext = match *id {
            "gba" => ".gba .zip",
            "gb" => ".gb .zip",
            "snes" => ".sfc .smc .zip",
            "nes" => ".nes .zip",
            "psx" => ".chd .cue .pbp",
            _ => ".fds .zip",
        };
        cfg.push_str(&format!("<system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>{ext}</extension></system>", folder.display()));
        for (g, file) in games.iter().enumerate().filter(|_| !empty) {
            let path = folder.join(file);
            std::fs::write(&path, b"rom").unwrap();
            if (i + g) % 4 != 3 {
                cover(&pocket_ui::covers::cover_path(&data, id, &path), pocket_ui::bezel::system_colour(id));
            }
            let secs = ((i * 7 + g * 13) % 11) as u64 * 1900;
            if secs > 0 {
                for _ in 0..((g % 3) + 1) {
                    meta.note_session(&path, secs / ((g as u64 % 3) + 1));
                }
                let display = pocket_ui::library::display_name(Path::new(file).file_stem().unwrap().to_str().unwrap());
                recents.note(id, &display, &path, 0);
                if let Some(r) = recents.list.first_mut() {
                    r.last_played = now - (i as u64 * 86_400 + g as u64 * 5_000);
                }
                if g == 0 {
                    favs.toggle(id, &display, &path);
                }
            }
            match (i + g) % 5 {
                1 => meta.set_status(&path, Status::Playing).unwrap(),
                2 => meta.set_status(&path, Status::Completed).unwrap(),
                3 => meta.set_status(&path, Status::Paused).unwrap(),
                _ => {}
            }
            if g == 1 {
                meta.toggle_sticker(&path, if i % 2 == 0 { couch } else { bed }).map(|_| ()).unwrap();
            }
        }
    }
    meta.save_sessions();
    recents.save();
    std::fs::create_dir_all(roms.join("bios")).unwrap();
    // A port that waits for its files.
    let sprout = roms.join("ports/sprout");
    std::fs::create_dir_all(sprout.join("assets")).unwrap();
    std::fs::write(
        sprout.join("port.json"),
        r#"{"items":["SPROUT.sh","sprout"],"attr":{"title":"Sprout","rtr":false,"inst":"Purchase the game from Steam and place the game files (data.win and the music folder) in sprout/assets."}}"#,
    )
    .unwrap();
    cfg.push_str("</systemList>");
    let es = dir.join("es_systems.cfg");
    std::fs::write(&es, cfg).unwrap();
    // A smart shelf, as Curator would keep it.
    std::fs::write(data.join("curator.json"), r#"{"shelves":[{"name":"Handhelds I am playing","lens":{"picks":{"system":["gba","gb"],"status":["playing"]}},"pinned":true},{"name":"Super Nintendo","lens":{"picks":{"system":["snes"]}},"pinned":false}]}"#).unwrap_or(());
    // 40 hours of battery: play, a charge, a night off.
    let mut h = History::new();
    let mut t = (now - 40 * 3600) / 60 * 60;
    let mut cap = 92.0f32;
    for minute in 0..(40 * 60) {
        let charging = (14 * 60..16 * 60).contains(&minute);
        let off = (20 * 60..28 * 60).contains(&minute);
        if !off {
            h.push(Sample { t, capacity: cap.round() as u8, charging, full: false, mv: 3900, ma: 400, temp: Some(300), tag: 0 }, None);
        }
        cap = if charging { (cap + 0.6).min(100.0) } else if off { cap - 0.01 } else { (cap - 0.09).max(4.0) };
        t += 60;
    }
    h.save(&data.join(pocket_ui::battery_history::FILE)).unwrap();
    (roms, data, es)
}

/// Writes what the browser asks for into the made-up data folder, as the App would.
fn apply(data: &Path, e: Edit) -> Result<(), String> {
    let mut meta = LibraryMeta::load(data);
    let r = match e {
        Edit::Favorite { system, name, path, on } => {
            let mut f = pocket_ui::favorites::Favorites::load(data);
            if f.contains(&path) != on {
                f.toggle(&system, &name, &path);
            }
            Ok(())
        }
        Edit::Status { path, status } => meta.set_status(&path, status),
        Edit::Note { path, note } => meta.set_note(&path, &note),
        Edit::Rating { path, rating } => meta.set_rating(&path, rating),
        Edit::Sticker { path, id, on } => {
            if meta.get(&path).stickers.contains(&id) != on {
                meta.toggle_sticker(&path, id).map(|_| ())
            } else {
                Ok(())
            }
        }
        Edit::NewSticker { name, path } => meta.add_sticker(&name).and_then(|id| path.map_or(Ok(()), |p| meta.toggle_sticker(&p, id).map(|_| ()))),
        Edit::RenameSticker { id, name } => meta.rename_sticker(id, &name),
        Edit::DeleteSticker { id } => meta.delete_sticker(id),
        Edit::Preferred { work, path } => meta.set_preferred(&work, &path),
        Edit::RenameShelf { .. } | Edit::DeleteShelf { .. } | Edit::LibraryChanged(_) => Ok(()),
    };
    r.map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(8737);
    let dir = PathBuf::from(arg("--dir").unwrap_or_else(|| std::env::temp_dir().join("pocketui-web-dev").to_string_lossy().into_owned()));
    let (roms, data, es) = setup(&dir, args.iter().any(|a| a == "--empty"));
    let shared = Shared::new(data.clone(), es.clone(), Prefs { enabled: true, ..Prefs::default() });
    let edit_data = data.clone();
    *shared.edits.lock().unwrap() = Some(Box::new(move |e, tx| tx.send(apply(&edit_data, e)).is_ok()));
    if args.iter().any(|a| a == "--playing") {
        let path = roms.join("gba/Golden Sun (USA, Europe).gba");
        *shared.now_playing.lock().unwrap() = Some(NowPlaying { system: "gba".into(), name: "Golden Sun".into(), path, since: pocket_ui::recents::now_secs() - 1500 });
        shared.game_running.store(args.iter().any(|a| a == "--paused-lists"), std::sync::atomic::Ordering::Relaxed);
    }
    // Curator's index, as the App builds it, for the smart shelves and the versions.
    let systems = pocket_ui::library::load(&es).unwrap();
    let games: Vec<pocket_ui::curator::LibraryGame> = systems
        .iter()
        .enumerate()
        .flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, game)| pocket_ui::curator::LibraryGame { at: (s, g), system: sys.name.clone(), path: game.path.clone(), name: game.name.clone(), choice: None, identified: None }))
        .collect();
    let names = systems.iter().map(|s| pocket_ui::curator::SystemName { id: s.name.clone(), name: s.fullname.clone(), short: s.name.to_uppercase() }).collect();
    let mut db = pocket_ui::gamedb::GameDb::embedded();
    let catalog = Arc::new(pocket_ui::curator::build_catalog(&games, names, &mut db, &pocket_ui::curator::CuratorDbFeel));
    let player = pocket_ui::curator::Player { meta: LibraryMeta::load(&data).games(), ..Default::default() };
    let user = pocket_ui::curator::user_state(&catalog, &player);
    *shared.curator.lock().unwrap() = Some(Arc::new(pocket_ui::curator::Index::build(catalog, user)));

    let screen = || {
        let (w, h) = (1240u32, 1080u32);
        let px = (0..w * h).map(|i| {
            let (x, y) = (i % w, i / w);
            let g = y * 120 / h;
            if (500..740).contains(&x) && (400..680).contains(&y) { 0xece9e1 } else { (20 + g) << 16 | (30 + g) << 8 | (60 + g / 2) }
        });
        Some(pocket_ui::image::Image { w, h, px: px.collect() })
    };
    // Play history over the made-up games, as `play_history::stats_json` gives it (with full paths: the server drops them).
    let history = (!args.iter().any(|a| a == "--no-history") && !systems.is_empty()).then(|| {
        let games: Vec<pocket_ui::play_log::Game> = systems
            .iter()
            .flat_map(|s| s.games.iter().map(move |g| pocket_ui::play_log::Game { path: g.path.to_string_lossy().into_owned(), name: g.name.clone(), system: s.name.clone() }))
            .collect();
        let now = pocket_ui::recents::now_secs();
        let tz = pocket_ui::appearance::utc_offset_now(now as i64);
        let today = (now as i64 + tz as i64).div_euclid(86_400);
        pocket_ui::play_history::stats_value(&pocket_ui::play_history::demo(&games, today, tz), now, tz)
    });
    let _ = HISTORY.set(history.flatten());
    let mut servers = Vec::new();
    for (owner, at) in [(true, port), (false, port + 1)] {
        let mut ctx = Ctx::new(shared.clone(), Roots::under(&roms), Box::new(Fake { owner }));
        ctx.screenshot = Box::new(screen);
        ctx.history = Some(dev_history);
        let listen = vec![Listen { addr: SocketAddr::from(([127, 0, 0, 1], at)), iface: "lo".into(), index: 0 }];
        servers.push(pocket_ui::web::server::Server::start(ctx, Some(listen)).unwrap());
    }
    println!("web_dev: http://127.0.0.1:{port} (owner), http://127.0.0.1:{} (pairing), data in {}", port + 1, dir.display());
    loop {
        // A fresh code every five minutes for the pairing port.
        let p = Pairing::new().unwrap();
        println!("web_dev: pairing code {} (or open http://127.0.0.1:{}/#pair={})", p.code, port + 1, p.code);
        *shared.pairing.lock().unwrap() = Some(p);
        let until = Instant::now() + Duration::from_secs(290);
        while Instant::now() < until {
            std::thread::sleep(Duration::from_secs(1));
            if shared.pairing.lock().unwrap().is_none() {
                break;
            }
        }
    }
}
