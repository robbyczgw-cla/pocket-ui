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
        Some(SelfNode { user_id: 1, login: "alex@example.com".into(), tagged: false, hostname: "pocket".into(), dns_name: "pocket.tail1234.ts.net".into(), #[cfg(feature = "brick")] addrs: Vec::new() })
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
    if !empty {
        demo_saves(&roms, &data, now);
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
    if !empty {
        // Sample album photos (JPEG + album.tsv), game screenshots, and a shelf picture for visual checks.
        let sample_jpeg = |rgb: [u8; 3]| -> Vec<u8> {
            let (w, h) = (240u32, 160u32);
            let mut px = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                for x in 0..w {
                    let stripe = ((x / 16 + y / 16) % 2) as u8 * 25;
                    px.extend([rgb[0].saturating_add(stripe), rgb[1].saturating_add(stripe), rgb[2].saturating_add(stripe)]);
                }
            }
            pocket_ui::album::encode(w, h, &px).unwrap().0
        };
        let gs = roms.join("gba/Golden Sun (USA, Europe).gba");
        let mf = roms.join("gba/Metroid Fusion (USA).gba");
        let ct = roms.join("snes/Chrono Trigger (USA).sfc");
        let _ = pocket_ui::album::add(&pocket_ui::album::dir(&data, "gba", &gs), now as i64 - 7200, &["Badge 3".into(), "4 h 12 min".into()], (240, 160), &sample_jpeg([42, 74, 140]));
        let _ = pocket_ui::album::add(&pocket_ui::album::dir(&data, "gba", &gs), now as i64 - 1800, &["Badge 4".into(), "6 h 05 min".into()], (240, 160), &sample_jpeg([160, 92, 48]));
        let _ = pocket_ui::album::add(&pocket_ui::album::dir(&data, "gba", &mf), now as i64 - 3600, &["40% items".into(), "2 h 18 min".into()], (240, 160), &sample_jpeg([35, 120, 95]));
        std::fs::write(data.join("settings.json"), r#"{"moment_photos":true}"#).unwrap();

        let shots = roms.join("screenshots");
        let shot_png = |rgb: [u8; 3]| -> Vec<u8> {
            let (w, h) = (240u32, 160u32);
            let mut px = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                for x in 0..w {
                    let frame = x < 6 || y < 6 || x >= w - 6 || y >= h - 6;
                    let c = if frame { [236, 233, 225] } else { [rgb[0], rgb[1].saturating_add((y / 3) as u8), rgb[2]] };
                    px.extend(c);
                }
            }
            pocket_ui::screenshot::encode_rgb(w, h, &px).unwrap()
        };
        let s1 = pocket_ui::screenshot::Stamp { year: 2026, month: 11, day: 18, hour: 21, min: 14, sec: 5 };
        let s2 = pocket_ui::screenshot::Stamp { year: 2026, month: 11, day: 19, hour: 19, min: 42, sec: 11 };
        let _ = pocket_ui::screenshot::save_png(&shots, "gba", "Golden Sun (USA, Europe)", &s1, &shot_png([60, 45, 130]));
        let _ = pocket_ui::screenshot::save_png(&shots, "snes", "Chrono Trigger (USA)", &s2, &shot_png([120, 50, 70]));
        let _ = pocket_ui::screenshot::save_png(&shots, pocket_ui::share_shelf::FOLDER, pocket_ui::share_shelf::NAME, &s2, &shot_png([80, 95, 70]));
        let _ = ct;
    }
    (roms, data, es)
}

/// Saves and states of two games, and Pocket Sync with two paired Pockets, a few saves that came and went and one open
/// question (the Saves tab and a game's saves).
fn demo_saves(roms: &Path, data: &Path, now: u64) {
    let gs = "Golden Sun (USA, Europe)";
    std::fs::write(roms.join(format!("gba/{gs}.srm")), vec![0u8; 64 << 10]).unwrap();
    std::fs::write(roms.join(format!("gba/{gs}.rtc")), [0u8; 16]).unwrap();
    let states = roms.join("savestates/gba");
    std::fs::create_dir_all(&states).unwrap();
    for (ending, kb) in [(".pstate.auto", 310), (".pstate", 296), (".pstate2", 301)] {
        std::fs::write(states.join(format!("{gs}{ending}")), vec![1u8; kb << 10]).unwrap();
    }
    let dir = data.join("pocket-sync");
    std::fs::create_dir_all(&dir).unwrap();
    pocket_ui::pocket_sync_page::Prefs { enabled: true }.save(&dir).unwrap();
    let peer = |id: &str, name: &str| pocket_ui::pocket_sync::net::Peer { id: id.into(), name: name.into(), addr: String::new(), key: [7u8; 32] };
    pocket_ui::pocket_sync::net::Peers(vec![peer("BRICK1", "TrimUI Brick"), peer("PLUS1", "RG35XX Plus")]).save(&dir).unwrap();
    use pocket_ui::pocket_sync::log::{Dir, Event, Last, Log, State};
    let mut log = Log::default();
    log.peers.insert("BRICK1".into(), State { last: Last::Synced, ok_at: Some(now - 180), down_since: None });
    log.peers.insert("PLUS1".into(), State { last: Last::Unreachable, ok_at: Some(now - 2 * 86_400), down_since: Some(now - 86_400 - 3600) });
    log.events = vec![
        Event { at: now - 180, peer: "BRICK1".into(), dir: Dir::From, rel: format!("gba/{gs}.srm") },
        Event { at: now - 7200, peer: "BRICK1".into(), dir: Dir::To, rel: "gba/Metroid Fusion (USA).srm".into() },
        Event { at: now - 2 * 86_400, peer: "PLUS1".into(), dir: Dir::From, rel: "gb/Tetris (World) (Rev 1).srm".into() },
    ];
    log.save(&dir).unwrap();
    let mut v = pocket_ui::pocket_sync::Versions::default();
    let vv: pocket_ui::pocket_sync::Vv = [("BRICK1".to_string(), 3u64)].into_iter().collect();
    v.0.insert("gba/Metroid Fusion (USA).srm".into(), pocket_ui::pocket_sync::Rec { sha: "c".repeat(64), vv: vv.clone(), asking: Some(vv), ..Default::default() });
    v.save(&dir.join("versions.json")).unwrap();
}

/// Writes what the browser asks for into the made-up data folder, as the App would.
fn apply(data: &Path, shots: &Path, shared: &Arc<Shared>, e: Edit) -> Result<(), String> {
    let mut meta = LibraryMeta::load(data);
    match e {
        Edit::Favorite { system, name, path, on } => {
            let mut f = pocket_ui::favorites::Favorites::load(data);
            if f.contains(&path) != on {
                f.toggle(&system, &name, &path);
            }
            Ok(())
        }
        Edit::Status { path, status } => meta.set_status(&path, status).map_err(|e| e.to_string()),
        Edit::Note { path, note } => meta.set_note(&path, &note).map_err(|e| e.to_string()),
        Edit::Rating { path, rating } => meta.set_rating(&path, rating).map_err(|e| e.to_string()),
        Edit::Cover { system, rom, png } => pocket_ui::covers::custom::apply(data, &system, &rom, png.as_deref()).map_err(|e| e.to_string()),
        Edit::Preferred { work, path } => meta.set_preferred(&work, &path).map_err(|e| e.to_string()),
        Edit::RenameShelf { .. } | Edit::DeleteShelf { .. } | Edit::LibraryChanged(_) => Ok(()),
        Edit::DeleteAlbumPhoto { system, rom, file } => pocket_ui::web::media::apply_delete_photo(data, &system, &rom, &file).map(|_| ()),
        Edit::DeleteScreenshot { system, game, at } => pocket_ui::web::media::apply_delete_shot(shots, &system, &game, &at),
        // The made-up card's game goes as on the Pocket (files, pictures, gamelist entry); no states folder here.
        Edit::DeleteGame { system, path, with_saves } => {
            let places = pocket_ui::game_files::Places { states: &data.join("states"), data, protected: &[], shared: false, saves_dir: None };
            let plan = pocket_ui::game_files::plan(&system, &path, &places)?;
            pocket_ui::game_files::delete(&plan, with_saves, &[]).map(|_| shared.library_changed())
        }
        Edit::CheckUpdate => {
            let mut u = shared.update.lock().unwrap();
            u.checked = true;
            u.state = pocket_ui::web::updates::UpdateState::Idle;
            Ok(())
        }
        Edit::InstallUpdate { version } => {
            let mut u = shared.update.lock().unwrap();
            u.state = pocket_ui::web::updates::UpdateState::Downloading;
            u.step = Some(1);
            u.percent = Some(42);
            u.message = Some(format!("Installing pocket-ui {version}"));
            Ok(())
        }
    }
}

fn main() {
    // The page's texts in POCKETUI_LANG's language (`/api/texts`), as the App sets the language at its start.
    pocket_ui::i18n::set(pocket_ui::i18n::Lang::En);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(8737);
    let dir = PathBuf::from(arg("--dir").unwrap_or_else(|| std::env::temp_dir().join("pocketui-web-dev").to_string_lossy().into_owned()));
    let (roms, data, es) = setup(&dir, args.iter().any(|a| a == "--empty"));
    let shared = Shared::new(data.clone(), es.clone(), Prefs { enabled: true, ..Prefs::default() });
    let edit_data = data.clone();
    let edit_shots = roms.join("screenshots");
    let edit_shared = shared.clone();
    *shared.edits.lock().unwrap() = Some(Box::new(move |e, tx| tx.send(apply(&edit_data, &edit_shots, &edit_shared, e)).is_ok()));
    if let Some(ver) = arg("--update") {
        let mut u = shared.update.lock().unwrap();
        u.current = arg("--current").unwrap_or_else(|| "0.6.0".into());
        u.available = Some(ver);
        u.notes = Some("Photo album, screenshots ZIP and signed updates in the web companion.".into());
        u.size = Some(18 * 1024 * 1024);
        u.release_build = true;
        u.checked = true;
    } else if args.iter().any(|a| a == "--up-to-date") {
        let mut u = shared.update.lock().unwrap();
        u.current = arg("--current").unwrap_or_else(|| "0.6.0".into());
        u.release_build = true;
        u.checked = true;
    }
    if let Some(lang) = arg("--lang") {
        pocket_ui::i18n::set(pocket_ui::i18n::Lang::from_locale(&lang));
    }
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
        let listen = vec![Listen { addr: SocketAddr::from(([127, 0, 0, 1], at)), iface: "lo".into(), index: 0, lan: !owner }];
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
