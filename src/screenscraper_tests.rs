//! ScreenScraper against a fake server that answers from `testdata/screenscraper/` (no real request is made): what is
//! asked, in which order, at which pace, where the pictures land, and how every refusal of the API ends a run.

use super::*;
use crate::romm::Fetched;
use crate::testutil::TestDir;
use std::sync::Mutex;

const USER: &str = include_str!("../testdata/screenscraper/ssuserInfos.json");
const FOUND: &str = include_str!("../testdata/screenscraper/jeuInfos-found.json");
const BASE: &str = "https://api.screenscraper.test/api2";

/// One answer of the fake server.
#[derive(Clone, Debug)]
enum Say {
    /// A status and a body (`jeuInfos`, `ssuserInfos`).
    Json(u16, String),
    /// A picture (`mediaJeu`).
    Png,
    /// A text instead of the picture (`NOMEDIA`).
    Text(&'static str),
    /// A PNG cut after its header: its size reads, its picture doesn't.
    BrokenPng,
    /// Only a status, with ScreenScraper's error line as the body.
    Status(u16),
    /// No answer at all.
    Silent,
}

/// What a request asked for.
#[derive(Clone, Debug)]
struct Asked {
    endpoint: String,
    query: HashMap<String, String>,
}

type Answers = dyn Fn(&Asked, usize) -> Say + Send + Sync;

struct Server {
    log: Mutex<Vec<Asked>>,
    answer: Box<Answers>,
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

impl Server {
    fn new(answer: impl Fn(&Asked, usize) -> Say + Send + Sync + 'static) -> Server {
        Server { log: Mutex::new(Vec::new()), answer: Box::new(answer) }
    }

    /// The usual server: the account, the game, every picture.
    fn usual() -> Server {
        Server::new(|a, _| match a.endpoint.as_str() {
            "ssuserInfos.php" => Say::Json(200, USER.into()),
            "jeuInfos.php" => Say::Json(200, FOUND.into()),
            _ => Say::Png,
        })
    }

    fn ask(&self, url: &str) -> Say {
        assert!(url.starts_with(&format!("{BASE}/")), "only the API, only https: {url}");
        let (endpoint, query) = url[BASE.len() + 1..].split_once('?').expect("a query");
        let query: HashMap<String, String> = query.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), decode(v))).collect();
        let asked = Asked { endpoint: endpoint.to_string(), query };
        let mut log = self.log.lock().unwrap();
        let nth = log.iter().filter(|a| a.endpoint == asked.endpoint).count();
        log.push(asked.clone());
        drop(log);
        (self.answer)(&asked, nth)
    }

    fn asked(&self) -> Vec<Asked> {
        self.log.lock().unwrap().clone()
    }

    fn endpoints(&self) -> Vec<String> {
        self.asked().into_iter().map(|a| a.endpoint).collect()
    }

    fn media(&self) -> Vec<String> {
        self.asked().into_iter().filter(|a| a.endpoint == "mediaJeu.php").map(|a| a.query["media"].clone()).collect()
    }
}

fn png_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&[200; 12]).unwrap();
    }
    out
}

impl Net for Server {
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer {
        assert_eq!((method, token, json), ("GET", None, None));
        match self.ask(url) {
            Say::Json(status, body) => Answer { status, body: body.into_bytes() },
            Say::Status(status) => Answer { status, body: b"Erreur de login : V\xc3\xa9rifier vos identifiants d\xc3\xa9veloppeur !".to_vec() },
            Say::Silent => Answer::none(),
            other => panic!("{other:?} for an API request"),
        }
    }

    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, _stop: &AtomicBool, _progress: &(dyn Fn(u64) + Sync)) -> Fetched {
        assert_eq!(token, "", "no Authorization header for ScreenScraper");
        assert_eq!(size, MAX_MEDIA_BYTES);
        match self.ask(url) {
            Say::Png => {
                std::fs::write(part, png_bytes()).unwrap();
                Fetched::Done
            }
            Say::Text(t) => {
                std::fs::write(part, t).unwrap();
                Fetched::Done
            }
            Say::BrokenPng => {
                std::fs::write(part, &png_bytes()[..40]).unwrap();
                Fetched::Done
            }
            Say::Status(s) | Say::Json(s, _) => Fetched::Status(s),
            Say::Silent => Fetched::Unreachable,
        }
    }
}

/// A clock that only counts: waiting moves it on at once.
#[derive(Default)]
struct FakeClock {
    now: Mutex<Duration>,
    waits: Mutex<Vec<Duration>>,
    /// The wall clock for the day's counts (0: the real one); a test's server may move it past midnight.
    unix: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        *self.now.lock().unwrap()
    }

    fn unix(&self) -> u64 {
        match self.unix.load(std::sync::atomic::Ordering::SeqCst) {
            0 => now_unix(),
            u => u,
        }
    }

    fn wait(&self, d: Duration, stop: &dyn Fn() -> bool) -> bool {
        self.waits.lock().unwrap().push(d);
        *self.now.lock().unwrap() += d;
        !stop()
    }
}

fn test_dev() -> Dev {
    Dev { id: "devid-test".into(), password: "devpw&test".into(), softname: "pocket-ui-test".into() }
}

struct Card {
    _dir: TestDir,
    data: PathBuf,
    roms: PathBuf,
}

fn card(label: &str) -> Card {
    let dir = TestDir::new(&format!("screenscraper-{label}"));
    let data = dir.join("data");
    let roms = dir.join("roms");
    std::fs::create_dir_all(&data).unwrap();
    Card { data, roms, _dir: dir }
}

impl Card {
    /// A game file of `system` with made-up bytes (each name its own).
    fn game(&self, system: &str, file: &str, bytes: &[u8]) -> (String, PathBuf) {
        let d = self.roms.join(system);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(file);
        let mut content = file.as_bytes().to_vec();
        content.extend_from_slice(bytes);
        std::fs::write(&p, content).unwrap();
        (system.to_string(), p)
    }
}

/// A run signed in as "player" (`ssuserInfos` first: a guest has none, `a_guest_asks_no_account_and_takes_its_limits_from_the_game`).
fn run(server: &Server, clock: &FakeClock, card: &Card, games: &[(String, PathBuf)], want: &[Face], stop: &(dyn Fn() -> bool + Sync)) -> Run {
    let mut client = Client::new(server, clock, BASE, test_dev(), Some(player())).unwrap();
    run_until(&mut client, &card.data, games, want, &|_| {}, stop)
}

fn player() -> Login {
    Login { user: "player".into(), password: "p&w d".into() }
}

fn never() -> bool {
    false
}

#[test]
fn the_id_is_unpacked_as_build_rs_packs_it_and_absent_builds_have_no_screenscraper() {
    let packed: String = screenscraper_mask(b"1234\npass word\npocket-ui").iter().map(|b| format!("{b:02x}")).collect();
    assert!(!packed.contains("pass"), "not as plain text in the binary");
    let dev = unpack_dev(&packed).unwrap();
    assert_eq!((dev.id.as_str(), dev.password.as_str(), dev.softname.as_str()), ("1234", "pass word", "pocket-ui"));
    assert!(!format!("{dev:?}").contains("pass"), "Debug never shows the password");
    for bad in ["", "0", "zz", &packed[..packed.len() - 1]] {
        assert_eq!(unpack_dev(bad), None, "{bad}");
    }
    // Only a build with the id has the feature (the gate's and every developer's build: none).
    assert_eq!(available(), !include_str!(concat!(env!("OUT_DIR"), "/screenscraper_dev.txt")).trim().is_empty());
    // build.rs never prints it, not even masked: a `cargo:` line stays in the build's output file (review 26.11.2026).
    let script = include_str!("../build.rs");
    let body = &script[script.find("fn screenscraper_dev").unwrap()..script.find("/// Everything the script does").unwrap()];
    assert!(!body.contains("rustc-env") && !body.contains("println!(\"{") && !body.contains("eprintln"), "the id goes into a file, never into a printed line");
}

#[test]
fn the_players_login_is_private_and_can_be_forgotten() {
    use std::os::unix::fs::PermissionsExt;
    let c = card("login");
    assert_eq!(login(&c.data), None);
    let l = Login { user: "player".into(), password: "s3cret pass".into() };
    save_login(&c.data, &l).unwrap();
    assert_eq!(login(&c.data), Some(l.clone()));
    let file = c.data.join("screenscraper/login");
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(!format!("{l:?}").contains("s3cret"));
    assert!(save_login(&c.data, &Login { user: "a\nb".into(), password: "x".into() }).is_err());
    assert!(save_login(&c.data, &Login { user: "a".into(), password: String::new() }).is_err());
    forget_login(&c.data);
    assert_eq!(login(&c.data), None);
}

#[test]
fn box_art_and_pictures_land_where_the_library_reads_them_the_new_faces_beside_them() {
    let d = Path::new("/data");
    let rom = Path::new("/roms/gba/Golden Sun (USA).gba");
    assert_eq!(path(d, Face::Cover, "gba", rom), crate::covers::cover_path(d, "gba", rom));
    assert_eq!(path(d, Face::Snap, "gba", rom), crate::game_pictures::path(d, crate::game_pictures::Kind::Snap, "gba", rom));
    assert_eq!(path(d, Face::Title, "gba", rom), crate::game_pictures::path(d, crate::game_pictures::Kind::Title, "gba", rom));
    assert_eq!(path(d, Face::BoxBack, "gba", rom), Path::new("/data/boxbacks/gba/Golden Sun (USA).png"));
    let folders: std::collections::HashSet<&str> = Face::ALL.iter().map(|f| f.folder()).collect();
    assert_eq!(folders.len(), Face::ALL.len(), "one folder each");
    for f in Face::ALL {
        assert_eq!(Face::from_media(f.media()), Some(f));
        assert!(crate::image::size_ok(f.max_edge(), f.max_edge()), "{f:?} is asked for at a size pocket-ui decodes");
    }
}

#[test]
fn regions_follow_the_rom_then_world_us_europe() {
    assert_eq!(rom_regions("Pokemon (USA, Europe) (Rev 1)"), ["us", "eu"]);
    assert_eq!(rom_regions("Mario (J) [!]"), ["jp"]);
    assert_eq!(rom_regions("Tetris (World)"), ["wor"]);
    assert_eq!(rom_regions("Zelda (Germany)"), ["de"]);
    assert!(rom_regions("homebrew").is_empty());
    let m = |kind: &str, region: &str| Media { kind: kind.into(), region: region.into(), format: "png".into() };
    let medias = [m("box-2D", "jp"), m("box-2D", "us"), m("box-2D", "eu"), m("box-2D-back", "jp")];
    let pick = |rom: &[String], stem: &str, face| pick_media(&medias, face, &region_order(rom, stem)).map(|m| m.region.clone());
    assert_eq!(pick(&[], "Game (Europe)", Face::Cover).as_deref(), Some("eu"), "the ROM's region first");
    assert_eq!(pick(&["jp".into()], "Game (Europe)", Face::Cover).as_deref(), Some("jp"), "what ScreenScraper says of the ROM beats the name");
    assert_eq!(pick(&[], "Game", Face::Cover).as_deref(), Some("us"), "no region: wor, then us");
    assert_eq!(pick(&[], "Game (France)", Face::Cover).as_deref(), Some("us"), "no French box: the fallbacks");
    assert_eq!(pick(&[], "Game (USA)", Face::BoxBack).as_deref(), Some("jp"), "any box back beats none");
    assert_eq!(pick(&[], "Game", Face::SupportTexture), None);
}

#[test]
fn the_accounts_limits_are_read_and_kept() {
    let q = parse_user(USER.as_bytes()).unwrap();
    assert_eq!(q, Quota { user: "player".into(), max_threads: 1, max_kbps: 256, per_min: 30, per_day: 20000, ko_per_day: 2000, today: 12, ko_today: 3 });
    assert_eq!(q.gap(), Duration::from_secs(2));
    assert_eq!(q.day_over(), None);
    assert_eq!(Quota { today: 20000, ..q.clone() }.day_over(), Some(Refusal::QuotaToday));
    assert_eq!(Quota { ko_today: 2000, ..q.clone() }.day_over(), Some(Refusal::UnknownToday));
    assert_eq!(Quota::default().gap(), DEFAULT_GAP, "unknown limit: a careful pace");
    // The documentation's other spelling, and numbers as numbers.
    let v: Value = serde_json::json!({"maxrequestsperdmin": 15, "maxthreads": 2});
    assert_eq!((Quota::from_ssuser(&v).per_min, Quota::from_ssuser(&v).max_threads), (15, 2));
}

#[test]
fn every_status_of_the_api_has_its_meaning() {
    let dev_text = "Erreur de login : Vérifier vos identifiants développeur !".as_bytes();
    assert_eq!(refusal_for(401, b"", false), Some(Refusal::Busy));
    assert_eq!(refusal_for(403, dev_text, false), Some(Refusal::DevRefused));
    assert_eq!(refusal_for(403, dev_text, true), Some(Refusal::DevRefused), "the developer's text wins");
    assert_eq!(refusal_for(403, b"Erreur de login", true), Some(Refusal::LoginRefused));
    assert_eq!(refusal_for(423, b"", false), Some(Refusal::Closed));
    assert_eq!(refusal_for(426, b"", false), Some(Refusal::Outdated));
    assert_eq!(refusal_for(430, b"", false), Some(Refusal::QuotaToday));
    assert_eq!(refusal_for(431, b"", false), Some(Refusal::UnknownToday));
    for status in [200, 400, 404, 429, 500, 0] {
        assert_eq!(refusal_for(status, b"", false), None, "{status} is about one request");
    }
    let all = [Refusal::Busy, Refusal::DevRefused, Refusal::LoginRefused, Refusal::Closed, Refusal::Outdated, Refusal::QuotaToday, Refusal::UnknownToday, Refusal::Unreachable];
    for r in all {
        assert!(r.message().ends_with('.') && r.message().len() < 110, "{r:?}: a short sentence for the player");
        assert_eq!(Refusal::from_code(r.code()), Some(r));
        assert!(!r.message().contains("43"), "no status numbers on screen");
    }
}

#[test]
fn the_day_ends_at_the_french_midnight_or_later_never_before() {
    // 24.11.2026 10:00 UTC → 25.11. 00:00 in France (UTC+1) = 24.11. 23:00 UTC.
    let now = 1_795_514_400; // 2026-11-24T10:00:00Z
    assert_eq!(next_french_day(now), 1_795_561_200);
    // Just before midnight in France, the next day is an hour away.
    assert_eq!(next_french_day(1_795_561_199), 1_795_561_200);
    assert_eq!(next_french_day(1_795_561_200), 1_795_561_200 + 86_400);
}

#[test]
fn the_index_keeps_what_was_found_and_forgets_a_changed_file() {
    let c = card("index");
    let id = Identity { crc: 0x36C0_8112, size: 4, alt: None, name: "a.gba".into() };
    let mut index = Index::default();
    let medias = vec![Media { kind: "box-2D-back".into(), region: "eu".into(), format: "png".into() }];
    index.set("a", &id, Entry::Found { game: 19512, system: 10, medias: medias.clone(), rom_regions: vec!["eu".into()], series: Series { families: vec!["Test Quest".into()], number: Some(2) } });
    index.set("b", &id, Entry::Missing);
    index.save(&c.data, "gba").unwrap();
    let back = Index::load(&c.data, "gba");
    assert_eq!(back, index);
    assert_eq!(back.get("a", &id), Some(&Entry::Found { game: 19512, system: 10, medias, rom_regions: vec!["eu".into()], series: Series { families: vec!["Test Quest".into()], number: Some(2) } }));
    assert_eq!(back.get("a", &Identity { crc: 1, ..id.clone() }), None, "another file under the same name");
    assert_eq!(back.get("b", &id), Some(&Entry::Missing));
    forget(&c.data, "gba", Path::new("/roms/gba/b.gba"));
    assert_eq!(Index::load(&c.data, "gba").get("b", &id), None);
}

#[test]
fn a_game_is_asked_for_by_its_checksum_and_its_pictures_land_on_the_card() {
    let c = card("found");
    let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
    let server = Server::usual();
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &Face::ALL, &never);
    assert_eq!(r, Run { asked: 1, found: 1, pictures: 9, no_checksum: 0, held: 0, ended: Ended::Done });
    let asked = server.asked();
    assert_eq!(server.endpoints()[..2], ["ssuserInfos.php", "jeuInfos.php"]);
    let crc = crate::rom_id::checksums("gba", &game.1).unwrap().unwrap().crc;
    let q = &asked[1].query;
    assert_eq!(q["crc"], format!("{crc:08X}"));
    assert_eq!(q["romtaille"], std::fs::metadata(&game.1).unwrap().len().to_string());
    assert_eq!(q["systemeid"], "12");
    assert_eq!(q["romnom"], "Test Quest (Europe).gba");
    assert_eq!(q["romtype"], "rom");
    for a in &asked {
        assert_eq!((a.query["devid"].as_str(), a.query["devpassword"].as_str(), a.query["softname"].as_str()), ("devid-test", "devpw&test", "pocket-ui-test"));
        assert_eq!(a.query["output"], "json");
        assert_eq!((a.query["ssid"].as_str(), a.query["sspassword"].as_str()), ("player", "p&w d"), "the login with every request");
    }
    // The ROM is European: the European box, back and label; what has no European one, its fallback.
    assert_eq!(server.media(), ["box-2D(eu)", "ss(wor)", "sstitle(wor)", "box-2D-back(eu)", "box-2D-side(us)", "box-texture(jp)", "support-2D(wor)", "support-texture(eu)", "wheel(wor)"]);
    for a in asked.iter().filter(|a| a.endpoint == "mediaJeu.php") {
        assert_eq!((a.query["jeuid"].as_str(), a.query["systemeid"].as_str(), a.query["outputformat"].as_str()), ("19512", "12", "png"));
        assert!(a.query["maxwidth"].parse::<u32>().unwrap() <= 2048);
    }
    for f in Face::ALL {
        let p = path(&c.data, f, "gba", &game.1);
        assert!(crate::image::png_size(&p).is_some(), "{f:?} at {}", p.display());
        assert!(!p.with_extension("part").exists());
    }
    // Shown by the library as it is: the cover is the one the shelf reads.
    assert_eq!(crate::covers::active_cover_path(&c.data, "gba", &game.1), path(&c.data, Face::Cover, "gba", &game.1));
    assert!(matches!(Index::load(&c.data, "gba").get("Test Quest (Europe)", &identity("gba", &game.1, None).unwrap()), Some(Entry::Found { game: 19512, .. })));
}

/// The shelf by series' logos (`series_logo_games`): one game per series, never all of them; a series with its logo on
/// the card asks nothing more.
#[test]
fn a_series_logo_comes_from_one_game_only() {
    let c = card("logos");
    let zelda = vec![c.game("gba", "Zelda One (Europe).gba", &[3; 64]), c.game("gba", "Zelda Two (Europe).gba", &[4; 64])];
    let server = Server::usual();
    let clock = FakeClock::default();
    // Nothing in the index yet: the first game (key games first), looked up once.
    assert_eq!(series_logo_games(&c.data, std::slice::from_ref(&zelda)), vec![zelda[0].clone()]);
    // The spines' pass knew both, each lists a logo: still the first alone.
    run(&server, &clock, &c, &zelda, &[Face::BoxSide], &never);
    let pick = series_logo_games(&c.data, std::slice::from_ref(&zelda));
    assert_eq!(pick, vec![zelda[0].clone()]);
    let r = run(&server, &clock, &c, &pick, &[Face::Wheel], &never);
    assert_eq!(r.pictures, 1);
    assert_eq!(server.media().iter().filter(|m| m.starts_with("wheel")).count(), 1, "{:?}", server.media());
    assert!(path(&c.data, Face::Wheel, "gba", &zelda[0].1).exists());
    // Its logo on the card: nothing more for this series.
    assert!(series_logo_games(&c.data, std::slice::from_ref(&zelda)).is_empty());
    // Real spines Off: no logos either.
    assert!(!Show { spines: false, ..Show::default() }.fetches(Face::Wheel));
}

#[test]
fn one_request_at_a_time_at_the_accounts_pace() {
    let c = card("pace");
    let games = [c.game("gba", "A (USA).gba", &[1; 32]), c.game("gba", "B (USA).gba", &[2; 32])];
    let server = Server::usual();
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, &games, &[Face::Cover, Face::BoxBack], &never);
    assert_eq!((r.found, r.pictures, r.ended), (2, 4, Ended::Done));
    let n = server.asked().len();
    assert_eq!(n, 1 + 2 * 3);
    // 30 a minute: every request after the first waits 2 s (the fake clock does not move by itself).
    assert_eq!(*clock.waits.lock().unwrap(), vec![Duration::from_secs(2); n - 1]);
}

#[test]
fn downloads_keep_to_the_accounts_speed() {
    let c = card("speed");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let user = USER.replace("\"maxdownloadspeed\": \"256\"", "\"maxdownloadspeed\": \"1\"");
    let found = FOUND.replace("\"maxdownloadspeed\": \"256\"", "\"maxdownloadspeed\": \"1\"");
    let server = Server::new(move |a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, user.clone()),
        "jeuInfos.php" => Say::Json(200, found.clone()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    // 1 KB/s: the picture's bytes in milliseconds.
    let bytes = png_bytes().len() as u64;
    assert!(clock.waits.lock().unwrap().contains(&Duration::from_millis(bytes * 1000 / 1024)), "{:?}", clock.waits.lock().unwrap());
}

#[test]
fn a_players_own_cover_and_pictures_already_there_are_never_asked_for() {
    let c = card("kept");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let cover = path(&c.data, Face::Cover, "gba", &game.1);
    std::fs::create_dir_all(cover.parent().unwrap()).unwrap();
    std::fs::write(&cover, b"the libretro cover").unwrap();
    let server = Server::usual();
    let clock = FakeClock::default();
    run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover, Face::BoxBack], &never);
    assert_eq!(server.media(), ["box-2D-back(eu)"], "no US back: the European one");
    assert_eq!(std::fs::read(&cover).unwrap(), b"the libretro cover");
    // All there: not even the account is asked.
    let server = Server::usual();
    run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover, Face::BoxBack], &never);
    assert!(server.asked().is_empty());
}

#[test]
fn an_unknown_game_is_not_asked_for_again_until_its_file_changes() {
    let c = card("missing");
    let game = c.game("gba", "Homebrew.gba", &[3; 32]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        _ => Say::Status(404),
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!((r.asked, r.found, r.ended), (1, 0, Ended::Done));
    let again = Server::new(|_, _| panic!("asked again"));
    let r = run(&again, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!((r.asked, r.ended), (0, Ended::Done));
    assert!(again.asked().is_empty(), "not even the account");
    // A new dump under the same name is asked for.
    std::fs::write(&game.1, b"another dump").unwrap();
    let server = Server::usual();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!((r.asked, r.found, r.pictures), (1, 1, 1));
}

#[test]
fn a_headered_rom_is_asked_for_again_without_its_header() {
    let c = card("header");
    let mut nes = b"NES\x1a".to_vec();
    nes.extend_from_slice(&[0; 12]);
    nes.extend_from_slice(&[9; 4096]);
    let game = c.game("nes", "Game (USA).nes", &[]);
    std::fs::write(&game.1, &nes).unwrap();
    let server = Server::new(|a, nth| match (a.endpoint.as_str(), nth) {
        ("ssuserInfos.php", _) => Say::Json(200, USER.into()),
        ("jeuInfos.php", 0) => Say::Status(404),
        ("jeuInfos.php", _) => Say::Json(200, FOUND.into()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!((r.asked, r.found, r.pictures), (1, 1, 1));
    let sizes: Vec<String> = server.asked().iter().filter(|a| a.endpoint == "jeuInfos.php").map(|a| a.query["romtaille"].clone()).collect();
    assert_eq!(sizes, [nes.len().to_string(), (nes.len() - 16).to_string()]);
    assert_eq!(server.asked().iter().filter(|a| a.endpoint == "jeuInfos.php").nth(1).unwrap().query["systemeid"], "3");
}

#[test]
fn games_without_a_checksum_cost_no_request() {
    let c = card("disc");
    let games = [c.game("psx", "Disc (Europe).chd", &[0; 64]), c.game("gba", "Several.rar", &[0; 16]), c.game("unknownsys", "x.bin", &[1; 8])];
    let server = Server::new(|_, _| panic!("no request for these"));
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, &games, &[Face::Cover], &never);
    assert_eq!((r.no_checksum, r.asked, r.ended), (3, 0, Ended::Done));
}

#[test]
fn quota_and_unknown_games_end_the_day_until_france_has_a_new_one() {
    for (status, why) in [(430, Refusal::QuotaToday), (431, Refusal::UnknownToday)] {
        let c = card("day");
        let games = [c.game("gba", "A (USA).gba", &[1; 32]), c.game("gba", "B (USA).gba", &[2; 32])];
        let server = Server::new(move |a, _| match a.endpoint.as_str() {
            "ssuserInfos.php" => Say::Json(200, USER.into()),
            _ => Say::Status(status),
        });
        let clock = FakeClock::default();
        let r = run(&server, &clock, &c, &games, &[Face::Cover], &never);
        assert_eq!(r.ended, Ended::Refused(why));
        assert_eq!(server.endpoints(), ["ssuserInfos.php", "jeuInfos.php"], "{status}: nothing more today");
        assert!(why.message().contains("tomorrow"), "{}", why.message());
        assert_eq!(day_over(&c.data, Some(&player()), now_unix()), Some(why));
        assert_eq!(day_over(&c.data, Some(&player()), next_french_day(now_unix())), None, "tomorrow it goes on");
        // A new run today asks nothing.
        let quiet = Server::new(|_, _| panic!("asked on a day that is over"));
        assert_eq!(run(&quiet, &clock, &c, &games, &[Face::Cover], &never).ended, Ended::Refused(why));
    }
}

#[test]
fn the_days_count_is_kept_before_the_server_has_to_say_so() {
    let c = card("count");
    let games = [c.game("gba", "A (USA).gba", &[1; 32])];
    let user = USER.replace("\"requeststoday\": \"12\"", "\"requeststoday\": \"20000\"");
    let server = Server::new(move |_, _| Say::Json(200, user.clone()));
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, &games, &[Face::Cover], &never);
    assert_eq!(r.ended, Ended::Refused(Refusal::QuotaToday));
    assert_eq!(server.endpoints(), ["ssuserInfos.php"]);
    // Counted here between the answers (a picture's answer has no counts): the last one allowed goes, nothing after it.
    let c = card("count2");
    let games = [c.game("gba", "A (USA).gba", &[1; 32]), c.game("gba", "B (USA).gba", &[2; 32])];
    let user = USER.replace("\"requeststoday\": \"12\"", "\"requeststoday\": \"19998\"");
    let found = FOUND.replace("\"requeststoday\": \"13\"", "\"requeststoday\": \"19999\"");
    let server = Server::new(move |a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, user.clone()),
        "jeuInfos.php" => Say::Json(200, found.clone()),
        _ => Say::Png,
    });
    let r = run(&server, &clock, &c, &games, &[Face::Cover], &never);
    assert_eq!(r.ended, Ended::Refused(Refusal::QuotaToday));
    // The answer said 19 999: the picture is the 20 000th, the second game's question would be one too many.
    assert_eq!(server.endpoints(), ["ssuserInfos.php", "jeuInfos.php", "mediaJeu.php"]);
}

#[test]
fn too_many_at_once_is_waited_out_twice_then_the_run_ends() {
    let c = card("busy");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let server = Server::new(|a, nth| match (a.endpoint.as_str(), nth) {
        ("ssuserInfos.php", _) => Say::Json(200, USER.into()),
        ("jeuInfos.php", 0 | 1) => Say::Status(429),
        ("jeuInfos.php", _) => Say::Json(200, FOUND.into()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!((r.found, r.pictures, r.ended), (1, 1, Ended::Done));
    assert_eq!(clock.waits.lock().unwrap().iter().filter(|w| **w == BUSY_WAIT).count(), 2);
    let c = card("busy2");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        _ => Say::Status(429),
    });
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!(r.ended, Ended::Refused(Refusal::Busy));
    assert_eq!(server.endpoints().len(), 1 + 1 + BUSY_RETRIES);
    assert_eq!(day_over(&c.data, Some(&player()), now_unix()), None, "busy is not the end of the day");
}

#[test]
fn closed_refused_and_outdated_end_the_run_with_a_line_for_the_player() {
    for (status, on, why) in [
        (401, "ssuserInfos.php", Refusal::Busy),
        (403, "ssuserInfos.php", Refusal::DevRefused),
        (423, "jeuInfos.php", Refusal::Closed),
        (426, "jeuInfos.php", Refusal::Outdated),
        (430, "mediaJeu.php", Refusal::QuotaToday),
    ] {
        let c = card("refused");
        let games = [c.game("gba", "A (USA).gba", &[1; 32]), c.game("gba", "B (USA).gba", &[2; 32])];
        let server = Server::new(move |a, _| match a.endpoint.as_str() {
            e if e == on => Say::Status(status),
            "ssuserInfos.php" => Say::Json(200, USER.into()),
            "jeuInfos.php" => Say::Json(200, FOUND.into()),
            _ => Say::Png,
        });
        let clock = FakeClock::default();
        let r = run(&server, &clock, &c, &games, &[Face::Cover], &never);
        assert_eq!(r.ended, Ended::Refused(why), "{status} on {on}");
        assert_eq!(server.endpoints().last().map(String::as_str), Some(on), "{status}: nothing after it");
        assert!(!c.data.join("covers/gba/A (USA).part").exists());
    }
}

#[test]
fn a_wrong_login_says_so() {
    let c = card("badlogin");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let server = Server::new(|a, _| {
        assert_eq!((a.query["ssid"].as_str(), a.query["sspassword"].as_str()), ("player", "p&w d"));
        Say::Json(403, "Erreur de login : Vérifier vos identifiants !".into())
    });
    let clock = FakeClock::default();
    let mut client = Client::new(&server, &clock, BASE, test_dev(), Some(Login { user: "player".into(), password: "p&w d".into() })).unwrap();
    let r = run_until(&mut client, &c.data, std::slice::from_ref(&game), &[Face::Cover], &|_| {}, &never);
    assert_eq!(r.ended, Ended::Refused(Refusal::LoginRefused));
}

#[test]
fn a_silent_server_ends_the_run_after_three_and_marks_nothing() {
    let c = card("silent");
    let games: Vec<_> = (0..5).map(|i| c.game("gba", &format!("G{i} (USA).gba"), &[i; 32])).collect();
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        _ => Say::Silent,
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, &games, &[Face::Cover], &never);
    assert_eq!(r.ended, Ended::Refused(Refusal::Unreachable));
    assert_eq!(server.endpoints().len(), 1 + GIVE_UP_AFTER);
    assert!(Index::load(&c.data, "gba").is_empty(), "no game marked unknown");
    assert_eq!(day_over(&c.data, Some(&player()), now_unix()), None);
}

#[test]
fn a_stopped_run_goes_on_where_it_stopped() {
    let c = card("resume");
    let games = [c.game("gba", "A (USA).gba", &[1; 32]), c.game("gba", "B (USA).gba", &[2; 32])];
    let server = Server::usual();
    let clock = FakeClock::default();
    // Stop once the first game's back is in: the second game is not asked for.
    let back_a = path(&c.data, Face::BoxBack, "gba", &games[0].1);
    let stop = || back_a.exists();
    let r = run(&server, &clock, &c, &games, &[Face::BoxBack, Face::SupportTexture], &stop);
    assert_eq!(r.ended, Ended::Stopped);
    assert_eq!(server.endpoints(), ["ssuserInfos.php", "jeuInfos.php", "mediaJeu.php"]);
    // The next run: the first game's id is known (no jeuInfos for it), its missing picture comes, then the second game.
    let server = Server::usual();
    let r = run(&server, &clock, &c, &games, &[Face::BoxBack, Face::SupportTexture], &never);
    assert_eq!((r.asked, r.ended), (1, Ended::Done));
    assert_eq!(server.endpoints(), ["ssuserInfos.php", "mediaJeu.php", "jeuInfos.php", "mediaJeu.php", "mediaJeu.php"]);
    for g in &games {
        for f in [Face::BoxBack, Face::SupportTexture] {
            assert!(path(&c.data, f, "gba", &g.1).exists());
        }
    }
}

#[test]
fn a_stop_while_waiting_ends_the_run_at_once() {
    let c = card("stopwait");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let server = Server::usual();
    let clock = FakeClock::default();
    let waited = || !clock.waits.lock().unwrap().is_empty();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &waited);
    assert_eq!(r.ended, Ended::Stopped);
    assert_eq!(server.endpoints(), ["ssuserInfos.php"], "the pause before the next request was cut short");
}

#[test]
fn no_picture_or_a_broken_one_leaves_nothing_behind() {
    let c = card("nomedia");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ if a.query["media"].starts_with("box-2D(") => Say::Text("NOMEDIA"),
        _ => Say::Status(404),
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover, Face::BoxBack], &never);
    assert_eq!((r.found, r.pictures, r.ended), (1, 0, Ended::Done));
    let dir = c.data.join("covers/gba");
    let left: Vec<_> = std::fs::read_dir(&dir).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
    assert!(!c.data.join("boxbacks/gba").read_dir().is_ok_and(|mut d| d.next().is_some()));
}

#[test]
fn an_error_page_instead_of_the_game_is_asked_again_next_time() {
    let c = card("errpage");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        _ => Say::Json(200, "Erreur : Serveur en maintenance".into()),
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::Cover], &never);
    assert_eq!((r.asked, r.found, r.ended), (1, 0, Ended::Done));
    assert!(Index::load(&c.data, "gba").is_empty());
}

#[test]
fn only_https_is_spoken() {
    let server = Server::usual();
    let clock = FakeClock::default();
    assert!(Client::new(&server, &clock, "http://api.screenscraper.fr/api2", test_dev(), None).is_none());
    assert!(Client::new(&server, &clock, API, test_dev(), None).is_some());
    assert!(API.starts_with("https://"));
}

#[test]
fn the_credit_line_names_the_source_and_licence() {
    assert_eq!(CREDIT, "Data and media: ScreenScraper.fr (CC BY-NC-SA 4.0)");
}

/// The files this module reads (the login, a system's index, the day's state) cut, flipped, empty, missing or not UTF-8
/// (`file_faults`): no panic; the valid ones read; a broken login is no login, a broken state ends no day, a broken index
/// line is a game asked for again.
#[test]
fn file_source_rule_screenscraper_login_index_and_state() {
    let c = card("faults");
    let d = c.data.join(DIR);
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    save_login(&c.data, &Login { user: "player".into(), password: "pw".into() }).unwrap();
    run(&Server::usual(), &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxBack], &never);
    let mut day = Ledger::load(&c.data, &quota_of(Some(&player())), now_unix()).unwrap();
    day.over = Some(Refusal::QuotaToday);
    day.save(&c.data, &quota_of(Some(&player())));
    let seeds: Vec<(&str, Vec<u8>)> = ["login", "gba.tsv", "day.json"].iter().map(|f| (*f, std::fs::read(d.join(f)).unwrap())).collect();
    let id = identity("gba", &game.1, None).unwrap();
    for (file, seed) in &seeds {
        for (label, bytes) in crate::file_faults::faults(seed) {
            let _ = std::fs::remove_file(d.join(file));
            if let Some(b) = &bytes {
                std::fs::write(d.join(file), b).unwrap();
            }
            match *file {
                "login" => {
                    let l = login(&c.data);
                    if label == "valid" {
                        assert_eq!(l.as_ref().map(|l| l.user.as_str()), Some("player"));
                    }
                    if matches!(label.as_str(), "missing" | "empty" | "encoding") {
                        assert_eq!(l, None, "{label}");
                    }
                    assert!(l.is_none_or(|l| !l.user.is_empty() && !l.password.is_empty()), "{label}");
                }
                "gba.tsv" => {
                    let index = Index::load(&c.data, "gba");
                    if label == "valid" {
                        assert!(matches!(index.get("A (USA)", &id), Some(Entry::Found { game: 19512, .. })));
                    }
                    if matches!(label.as_str(), "missing" | "empty" | "encoding") {
                        assert!(index.is_empty(), "{label}");
                    }
                    // A run over whatever is there finishes; the picture is on the card already.
                    let _ = std::fs::remove_file(d.join("day.json"));
                    let r = run(&Server::usual(), &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxBack, Face::Support], &never);
                    assert!(matches!(r.ended, Ended::Done), "{label}: {r:?}");
                    let _ = std::fs::remove_file(path(&c.data, Face::Support, "gba", &game.1));
                    std::fs::write(d.join("day.json"), &seeds[2].1).unwrap();
                }
                _ => {
                    // Review 26.11.2026: a day's file that is there and can't be read holds every request back (a broken
                    // card must not start the day afresh); none at all is a new day.
                    let why = day_over(&c.data, Some(&player()), now_unix());
                    match label.as_str() {
                        "valid" => assert_eq!(why, Some(Refusal::QuotaToday)),
                        "missing" => assert_eq!(why, None, "{label}"),
                        "empty" | "encoding" => assert_eq!(why, Some(Refusal::DayDone), "{label}"),
                        _ => {}
                    }
                }
            }
        }
        std::fs::write(d.join(file), seed).unwrap();
    }
}

/// Live run 26.11.2026: ScreenScraper answers a guest's `ssuserInfos` with 403 "Vérifier les identifiants
/// utilisateurs", which ended every guest's run as "doesn't accept this pocket-ui version". A guest asks for the game
/// straight away and takes the day's numbers from its answer.
#[test]
fn a_guest_asks_no_account_and_takes_its_limits_from_the_game() {
    let c = card("guest");
    let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(403, "Erreur de login : Vérifier les identifiants utilisateurs !".into()),
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    let mut client = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
    assert!(!client.signed_in());
    let r = run_until(&mut client, &c.data, std::slice::from_ref(&game), &[Face::BoxSide], &|_| {}, &never);
    assert_eq!(r.ended, Ended::Done);
    assert_eq!(r.pictures, 1);
    assert_eq!(server.endpoints(), ["jeuInfos.php", "mediaJeu.php"], "no ssuserInfos for a guest");
    assert!(server.asked().iter().all(|a| !a.query.contains_key("ssid")), "no login given: none sent");
    assert!(client.quota.per_day > 0, "the day's limit from the game's answer: {:?}", client.quota);
}

/// The two 403 lines of the live API: the developer id's and the player's.
#[test]
fn the_live_403_lines_tell_the_developer_id_from_the_players_login() {
    let dev = "Erreur de login : Vérifier vos identifiants développeur !".as_bytes();
    let user = "Erreur de login : Vérifier les identifiants utilisateurs !".as_bytes();
    assert_eq!(refusal_for(403, dev, true), Some(Refusal::DevRefused));
    assert_eq!(refusal_for(403, user, true), Some(Refusal::LoginRefused));
    assert_eq!(refusal_for(403, dev, false), Some(Refusal::DevRefused));
}

/// Live run 26.11.2026: Link's Awakening DX in the Game Boy folder is ScreenScraper's game under the Game Boy Color
/// (10, parent 9); `mediaJeu` by the folder's system answered NOMEDIA for every picture. The pictures are asked by the
/// game's own system, and the index keeps it for the next run.
#[test]
fn pictures_are_asked_by_the_games_own_system() {
    let c = card("gbc-in-gb");
    let game = c.game("gb", "Zelda DX (USA, Europe).gbc", &[9; 64]);
    let found = FOUND.replace(r#""id": "12", "text": "Game Boy Advance""#, r#""id": "10", "parentid": "9", "text": "Game Boy Color""#);
    assert_ne!(found, FOUND, "the fixture names its system");
    let server = Server::new(move |a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        "jeuInfos.php" => Say::Json(200, found.clone()),
        _ if a.query["systemeid"] == "10" => Say::Png,
        _ => Say::Text("NOMEDIA"),
    });
    let clock = FakeClock::default();
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::BoxSide], &never);
    assert_eq!(r.pictures, 1);
    let asked = server.asked();
    assert_eq!(asked.iter().find(|a| a.endpoint == "jeuInfos.php").unwrap().query["systemeid"], "9", "asked by the folder's system");
    assert_eq!(asked.iter().find(|a| a.endpoint == "mediaJeu.php").unwrap().query["systemeid"], "10", "the picture by the game's");
    let id = identity("gb", &game.1, None).unwrap();
    assert!(matches!(Index::load(&c.data, "gb").get("Zelda DX (USA, Europe)", &id), Some(Entry::Found { system: 10, .. })));
    // A stopped run's next one fetches by the index: the game's system again.
    std::fs::remove_file(path(&c.data, Face::BoxSide, "gb", &game.1)).unwrap();
    let again = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        _ if a.query.get("systemeid").map(String::as_str) == Some("10") => Say::Png,
        _ => Say::Text("NOMEDIA"),
    });
    let r = run(&again, &clock, &c, std::slice::from_ref(&game), &[Face::BoxSide], &never);
    assert_eq!((r.pictures, again.media()), (1, vec!["box-2D-side(us)".to_string()]));
}

/// A picture `jeuInfos` lists that `mediaJeu` doesn't give (NOMEDIA) is not asked for again: each would cost a
/// request on every start.
#[test]
fn a_picture_answered_with_nomedia_is_not_asked_for_again() {
    let c = card("nomedia");
    let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ if a.query["media"].starts_with("box-2D-back") => Say::Text("NOMEDIA"),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    let want = [Face::BoxSide, Face::BoxBack];
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &want, &never);
    assert_eq!(r.pictures, 1);
    assert!(existing(&c.data, Face::BoxBack, "gba", &game.1).is_none(), "NOMEDIA leaves no file");
    let again = Server::usual();
    let r = run(&again, &clock, &c, std::slice::from_ref(&game), &want, &never);
    assert_eq!(r.pictures, 0);
    assert!(again.media().is_empty(), "nothing asked again: {:?}", again.media());
}

/// Europe's boxes are often under `uk` (Mario Kart DS has no `eu`): a European ROM takes those before the US ones.
#[test]
fn a_european_rom_takes_the_uk_box_before_the_us_one() {
    let order = region_order(&["eu".to_string()], "Mario Kart DS (E)");
    let medias = [Media { kind: "box-2D-side".into(), region: "us".into(), format: "png".into() }, Media { kind: "box-2D-side".into(), region: "uk".into(), format: "png".into() }];
    assert_eq!(pick_media(&medias, Face::BoxSide, &order).map(|m| m.region.as_str()), Some("uk"));
}

/// The series shelf reads each known game's series from the index (`series_of`): ScreenScraper's `familles` (the main
/// one first, English or world-wide where it has the name, else its first language) and `numeros` when it is a plain
/// number. Old index lines without the two columns still load, without a series.
#[test]
fn a_games_series_is_kept_in_the_index_for_the_series_shelf() {
    let live = include_str!("../testdata/screenscraper/jeuInfos-series.json");
    let (game, quota) = parse_game(live.as_bytes()).unwrap();
    assert_eq!(game.series, Series { families: vec!["Metroid".into()], number: Some(1) });
    assert_eq!(game.system, 12);
    assert!(quota.is_some_and(|q| q.per_day == 10000), "a guest's day from the live answer");
    let named = r#"{"response": {"jeu": {"id": "1", "familles": [
        {"noms": [{"langue": "fr", "text": "Zelda (série)"}], "principale": "0"},
        {"noms": [{"langue": "fr", "text": "La Légende"}, {"langue": "en", "text": "The Legend of Zelda"}], "principale": "1"}],
        "numeros": [{"noms": [{"langue": "fr", "text": "II"}]}]}}}"#;
    let (g, _) = parse_game(named.as_bytes()).unwrap();
    assert_eq!(g.series, Series { families: vec!["The Legend of Zelda".into(), "Zelda (série)".into()], number: None }, "main first, English where there is one; II is no plain number");

    let c = card("series");
    let files = c.data.join(DIR);
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("gba.tsv"), "Old Line (USA)\t0000ABCD\t64\t19512 us box-2D-side(us).png\nUnknown (USA)\t0000ABCE\t64\t-\n").unwrap();
    let mut index = Index::load(&c.data, "gba");
    assert!(matches!(index.get("Old Line (USA)", &Identity { crc: 0xABCD, size: 64, alt: None, name: String::new() }), Some(Entry::Found { series, .. }) if *series == Series::default()));
    index.set("Metroid - Zero Mission (USA)", &Identity { crc: 1, size: 2, alt: None, name: String::new() }, Entry::Found { game: game.id, system: 0, medias: game.medias.clone(), rom_regions: Vec::new(), series: game.series.clone() });
    index.save(&c.data, "gba").unwrap();
    let series = series_of(&c.data, "gba");
    assert_eq!(series.len(), 1, "only games with a series: {series:?}");
    assert_eq!(series["Metroid - Zero Mission (USA)"], (vec!["Metroid".to_string()], Some(1)));
}

fn guest_run(server: &Server, clock: &FakeClock, card: &Card, games: &[(String, PathBuf)], want: &[Face]) -> Run {
    let mut client = Client::new(server, clock, BASE, test_dev(), None).unwrap();
    run_until(&mut client, &card.data, games, want, &|_| {}, &never)
}

// The earlier media tests ran directly on lists with no card artwork. Exercise the library's pass selection
// with both sources of existing front covers and the same index across a pause and the later passes.
#[test]
fn the_shelf_gets_spines_for_recognized_games_without_replacing_their_fronts() {
    let c = card("library-coverage");
    let hack = c.game("gba", "A Hack.gba", &[1; 32]);
    let card_cover_game = c.game("megadrive", "B Original (Europe).md", &[2; 32]);
    let cached_cover_game = c.game("genesis", "C Original (USA).md", &[3; 32]);
    let no_cover_game = c.game("snes", "D Original (USA).sfc", &[4; 32]);
    let card_cover = c.roms.join("megadrive/.media/B Original (Europe).png");
    let cached_cover = crate::covers::cover_path(&c.data, &cached_cover_game.0, &cached_cover_game.1);
    let mut cover_bytes = png_bytes();
    cover_bytes.extend_from_slice(b"existing front cover");
    for cover in [&card_cover, &cached_cover] {
        std::fs::create_dir_all(cover.parent().unwrap()).unwrap();
        std::fs::write(cover, &cover_bytes).unwrap();
    }
    let gamelist = crate::gamelist::Index::load(&c.data, &crate::gamelist::Locations::none(),
        &[(card_cover_game.0.clone(), c.roms.join(&card_cover_game.0))]);
    assert!(gamelist.has_box_art(&card_cover_game.0, &card_cover_game.1));

    let names = [&card_cover_game, &cached_cover_game, &no_cover_game].into_iter()
        .map(|(_, rom)| (rom.clone(), rom.file_stem().unwrap().to_string_lossy().into_owned())).collect();
    let mut games = vec![hack, card_cover_game.clone(), cached_cover_game.clone(), no_cover_game.clone()];
    prioritize_recognized(&mut games, &names);

    let server = Server::new(|a, nth| match a.endpoint.as_str() {
        "jeuInfos.php" if a.query["romnom"] == "A Hack.gba" => Say::Status(404),
        "jeuInfos.php" => {
            let mut body: Value = serde_json::from_str(FOUND).unwrap();
            body["response"]["jeu"]["systeme"]["id"] = Value::String(a.query["systemeid"].clone());
            body["response"]["jeu"]["id"] = Value::String((nth + 1).to_string());
            Say::Json(200, body.to_string())
        }
        "mediaJeu.php" => Say::Png,
        _ => panic!("unexpected request: {a:?}"),
    });
    let clock = FakeClock::default();
    let mut client = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
    let side = [Face::BoxSide];
    let spine_games = games_for_pass(&games, &side, &gamelist);
    let stopped = AtomicBool::new(false);
    let first = run_until(&mut client, &c.data, &spine_games, &side,
        &|p| stopped.store(p.pictures == 1, std::sync::atomic::Ordering::Relaxed),
        &|| stopped.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!((first.pictures, first.ended), (1, Ended::Stopped));
    let (sys, rom) = &card_cover_game;
    assert_eq!(std::fs::read(path(&c.data, Face::BoxSide, sys, rom)).unwrap(), png_bytes());
    let requests = server.asked();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].query["romnom"], "B Original (Europe).md");
    assert_eq!(requests[0].query["systemeid"], "1");
    assert_eq!(requests[1].query["systemeid"], "1");

    let rest = run_until(&mut client, &c.data, &spine_games, &side, &|_| {}, &never);
    assert_eq!((rest.pictures, rest.ended), (2, Ended::Done));
    for (sys, rom) in [&card_cover_game, &cached_cover_game, &no_cover_game] {
        assert!(index_file(&c.data, sys).is_file(), "{sys} has its own index");
        assert!(path(&c.data, Face::BoxSide, sys, rom).is_file());
    }
    let requests = server.asked();
    let lookups: Vec<_> = requests.iter().filter(|a| a.endpoint == "jeuInfos.php")
        .map(|a| (a.query["romnom"].as_str(), a.query["systemeid"].as_str())).collect();
    assert_eq!(lookups, [("B Original (Europe).md", "1"), ("C Original (USA).md", "1"), ("D Original (USA).sfc", "4"), ("A Hack.gba", "12")]);

    let fronts = [Face::Cover];
    let (sys, rom) = &no_cover_game;
    assert!(!path(&c.data, Face::Cover, sys, rom).exists(), "spines arrive before new fronts");
    for backs in [false, true] {
        let show = Show { backs, ..Show::default() };
        let faces: Vec<_> = [Face::BoxBack, Face::SupportTexture, Face::Support].into_iter()
            .filter(|&face| show.fetches(face)).collect();
        let back_games = games_for_pass(&games, &faces, &gamelist);
        assert_eq!(run_until(&mut client, &c.data, &back_games, &faces, &|_| {}, &never).ended, Ended::Done);
        for (sys, rom) in [&card_cover_game, &cached_cover_game, &no_cover_game] {
            assert_eq!(path(&c.data, Face::BoxBack, sys, rom).is_file(), backs);
            assert!(path(&c.data, Face::SupportTexture, sys, rom).is_file());
        }
    }
    let front_games = games_for_pass(&games, &fronts, &gamelist);
    let front_run = run_until(&mut client, &c.data, &front_games, &fronts, &|_| {}, &never);
    assert_eq!((front_run.pictures, front_run.ended), (1, Ended::Done));
    assert_eq!(std::fs::read(path(&c.data, Face::Cover, sys, rom)).unwrap(), png_bytes());
    for cover in [card_cover, cached_cover] {
        assert_eq!(std::fs::read(cover).unwrap(), cover_bytes);
    }
    assert!(!path(&c.data, Face::Cover, &card_cover_game.0, &card_cover_game.1).exists());

    let pictures = [Face::Title, Face::Snap];
    let picture_games = games_for_pass(&games, &pictures, &gamelist);
    assert_eq!(run_until(&mut client, &c.data, &picture_games, &pictures, &|_| {}, &never).pictures, 6);
    for (sys, rom) in [&card_cover_game, &cached_cover_game, &no_cover_game] {
        for face in pictures {
            assert!(path(&c.data, face, sys, rom).is_file());
        }
    }
    assert_eq!(server.endpoints().iter().filter(|e| e.as_str() == "jeuInfos.php").count(), 4,
        "the later passes use the index, including the hack that was not found");
}

/// Alex 26.11.2026: a guest looks up at most `GUEST_GAMES_PER_DAY` games a day on one handheld, whatever the server
/// would allow, and a new run or a new start the same day goes on counting (review: the count lived in one run). Only
/// new look-ups wait for tomorrow: the pictures of games already known still come (review, second round).
#[test]
fn a_guest_looks_up_at_most_a_thousand_games_a_day_and_known_games_pictures_go_on() {
    let c = card("guest-games");
    let games = [c.game("gba", "A (USA).gba", &[1; 32]), c.game("gba", "B (USA).gba", &[2; 32])];
    let mut day = Ledger::load(&c.data, "guest", now_unix()).unwrap();
    day.games = GUEST_GAMES_PER_DAY - 1;
    assert!(day.save(&c.data, "guest"));
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    let r = guest_run(&server, &clock, &c, &games, &[Face::BoxSide]);
    assert_eq!((r.asked, r.pictures, r.held, r.ended), (1, 1, 1, Ended::Done), "B waits for tomorrow");
    // A new run the same day: B is not looked up; A's missing picture (a known game) still comes.
    std::fs::remove_file(path(&c.data, Face::BoxSide, "gba", &games[0].1)).unwrap();
    let again = Server::new(|a, _| match a.endpoint.as_str() {
        "mediaJeu.php" => Say::Png,
        other => panic!("{other}: no look-up after the day's games"),
    });
    let r = guest_run(&again, &clock, &c, &games, &[Face::BoxSide]);
    assert_eq!((r.asked, r.pictures, r.held), (0, 1, 1));
    assert_eq!(Ledger::load(&c.data, "guest", next_french_day(now_unix())).unwrap().games, 0, "tomorrow B's turn comes");
    // An account's day is its own.
    assert_eq!(day_over(&c.data, Some(&player()), now_unix()), None);
}

/// A guest uses at most half of the requests ScreenScraper allows a guest a day, every request counted, pictures
/// too; the server's counts (in steps of ten) never lower the ones counted here.
#[test]
fn a_guest_uses_at_most_half_of_the_days_requests_and_the_count_only_grows() {
    let c = card("guest-share");
    let games: Vec<_> = (0..4).map(|i| c.game("gba", &format!("G{i} (USA).gba"), &[i + 1; 32])).collect();
    // The live guest's day: 10000. Half of it is 5000; four requests are left.
    let mut day = Ledger::load(&c.data, "guest", now_unix()).unwrap();
    day.requests = 4996;
    day.save(&c.data, "guest");
    // The answer says 4990 used: lower than counted here, so it changes nothing.
    let found = FOUND.replace("\"requeststoday\": \"13\"", "\"requeststoday\": \"4990\"").replace("\"maxrequestsperday\": \"20000\"", "\"maxrequestsperday\": \"10000\"");
    assert_ne!(found, FOUND);
    let server = Server::new(move |a, _| match a.endpoint.as_str() {
        "jeuInfos.php" => Say::Json(200, found.clone()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    let r = guest_run(&server, &clock, &c, &games, &[Face::BoxSide, Face::BoxBack]);
    assert_eq!(r.ended, Ended::Refused(Refusal::DayDone));
    assert_eq!(server.endpoints().len(), 4, "jeuInfos and both pictures of the first, the second's jeuInfos: {:?}", server.endpoints());
    let day = Ledger::load(&c.data, "guest", now_unix()).unwrap();
    assert_eq!((day.requests, day.per_day), (5000, 10000));
    // An account has its whole day (Alex: no 50 % and no game brake with a login).
    let account = Ledger { requests: 5000, per_day: 20000, ..Ledger::default() };
    assert_eq!(account.limit(false), None);
    assert!(account.game_allowed(false) && !Ledger { games: GUEST_GAMES_PER_DAY, ..Ledger::default() }.game_allowed(true));
    assert_eq!(Ledger { requests: 20000, per_day: 20000, ..Ledger::default() }.limit(false), Some(Refusal::QuotaToday));
}

/// Review 26.11.2026: a stop while a picture downloads (Off, a game) ended as "no picture", which took the picture off
/// the game's list for good. A stop gives the download up and keeps the list; only ScreenScraper's own NOMEDIA or 404
/// takes a picture off it, and a 404 counts as a miss of the day.
#[test]
fn a_stop_during_a_download_keeps_the_picture_listed() {
    let c = card("stopdl");
    let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
    let stopping = std::sync::atomic::AtomicBool::new(false);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ => Say::Png,
    });
    let clock = FakeClock::default();
    // Stops as soon as the first picture is asked for: the download's own check sees it.
    let stop = || stopping.load(std::sync::atomic::Ordering::Relaxed) || server.media().len() == 1;
    let r = run(&server, &clock, &c, std::slice::from_ref(&game), &[Face::BoxSide], &stop);
    assert_eq!(r.ended, Ended::Stopped);
    assert!(existing(&c.data, Face::BoxSide, "gba", &game.1).is_none(), "nothing half-made on the card");
    let id = identity("gba", &game.1, None).unwrap();
    let Some(Entry::Found { medias, .. }) = Index::load(&c.data, "gba").get("Test Quest (Europe)", &id).cloned() else { panic!("known") };
    assert!(medias.iter().any(|m| m.kind == "box-2D-side"), "the spine stays listed: {medias:?}");
    // The next run fetches it; a 404 for the back takes the back off the list and counts as a miss.
    let again = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        _ if a.query["media"].starts_with("box-2D-back") => Say::Status(404),
        _ => Say::Png,
    });
    let r = run(&again, &clock, &c, std::slice::from_ref(&game), &[Face::BoxSide, Face::BoxBack], &never);
    assert_eq!((r.pictures, r.ended), (1, Ended::Done));
    let Some(Entry::Found { medias, .. }) = Index::load(&c.data, "gba").get("Test Quest (Europe)", &id).cloned() else { panic!("known") };
    assert!(!medias.iter().any(|m| m.kind == "box-2D-back"), "404: not asked again");
    assert!(Ledger::load(&c.data, &quota_of(Some(&player())), now_unix()).unwrap().ko >= 1, "the 404 counted as a miss");
}

/// A picture's 403 comes without ScreenScraper's line: only a refused login is known for sure (the account's own answer).
#[test]
fn a_pictures_403_asks_the_account_which_id_it_is() {
    // An account that is accepted tells nothing of the picture's 403: the neutral line, never "update pocket-ui" on a guess.
    for (account, why) in [(Say::Json(200, USER.into()), Refusal::PicturesRefused), (Say::Json(403, "Erreur de login : Vérifier les identifiants utilisateurs !".into()), Refusal::LoginRefused)] {
        let c = card("media403");
        let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
        let account = account.clone();
        let server = Server::new(move |a, n| match a.endpoint.as_str() {
            "ssuserInfos.php" if n == 0 => Say::Json(200, USER.into()),
            "ssuserInfos.php" => account.clone(),
            "jeuInfos.php" => Say::Json(200, FOUND.into()),
            _ => Say::Json(403, String::new()),
        });
        let r = run(&server, &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxSide], &never);
        assert_eq!(r.ended, Ended::Refused(why));
    }
}

/// Review 26.11.2026, second round: a day's file that is there and can't be read (a broken card) holds every quota back
/// for the rest of the day and starts tomorrow afresh; nothing is written over it while it can't be read.
#[test]
fn a_broken_days_file_holds_every_request_back_until_tomorrow() {
    let c = card("broken-day");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let file = c.data.join(DIR).join("day.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{\"guest\": {\"day\": ").unwrap();
    assert!(!Ledger::default().save(&c.data, "guest"), "nothing written over a file it can't read");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\"guest\": {\"day\": ");
    let quiet = Server::new(|_, _| panic!("asked with a broken day's file"));
    let r = guest_run(&quiet, &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxSide]);
    assert_eq!(r.ended, Ended::Refused(Refusal::DayDone));
    for login in [None, Some(&player())] {
        assert_eq!(day_over(&c.data, login, now_unix()), Some(Refusal::DayDone), "every quota");
        assert_eq!(day_over(&c.data, login, next_french_day(now_unix())), None, "tomorrow afresh");
    }
}

/// Every request is on the card before it goes (review: one cut off after its answer was lost), and the day turns over
/// at midnight in France for the counts, not only at a run's start.
#[test]
fn a_request_is_counted_on_the_card_before_it_goes_and_the_day_turns_at_midnight() {
    let c = card("reserve");
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let file = c.data.join(DIR).join("day.json");
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let server = Server::new(move |a, _| {
        let l = serde_json::from_str::<Value>(&std::fs::read_to_string(&file).unwrap_or_default()).ok().and_then(|v| v["guest"]["requests"].as_u64());
        seen2.lock().unwrap().push(l.unwrap_or(0));
        match a.endpoint.as_str() {
            "jeuInfos.php" => Say::Json(200, FOUND.into()),
            _ => Say::Png,
        }
    });
    guest_run(&server, &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxSide, Face::BoxBack]);
    // The first on an empty day is booked as 1 when it goes; the answer's own count (13) then raises the day, and each
    // later request is again on the card, one more, before it goes.
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0], 1, "booked before it was sent");
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "{seen:?}");
    let today = Ledger { day: french_day(now_unix()), requests: 4000, games: 900, per_day: 10000, ..Ledger::default() };
    let tomorrow = today.next_day(next_french_day(now_unix()));
    assert_eq!((tomorrow.requests, tomorrow.games, tomorrow.per_day), (0, 0, 10000), "a new day keeps only the limits");
}

/// Review 26.11.2026: a PNG whose header is right and whose picture is broken passed as saved, and the shelf could never
/// show it. It is decoded whole first; a broken one is left for the next run, and the picture stays listed.
#[test]
fn a_broken_png_is_not_kept_and_comes_again_next_run() {
    let c = card("broken-png");
    let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
    let server = Server::new(|a, _| match a.endpoint.as_str() {
        "ssuserInfos.php" => Say::Json(200, USER.into()),
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ => Say::BrokenPng,
    });
    let r = run(&server, &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxSide], &never);
    assert_eq!(r.pictures, 0);
    assert!(existing(&c.data, Face::BoxSide, "gba", &game.1).is_none());
    let r = run(&Server::usual(), &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxSide], &never);
    assert_eq!(r.pictures, 1, "asked again and kept");
}

/// A limit nobody has said yet is never spent on a guess: no picture at all, and at most `PROBES` requests a day that
/// bring the limit, counted in the day's file, so a new start or a new client gets no fresh five (review, round 3). A
/// limit of one leaves a guest nothing (half of it rounds to nought).
#[test]
fn an_unknown_day_limit_is_never_spent_on_a_guess_and_its_probes_are_counted_on_the_card() {
    let c = card("unknown-limit");
    let games: Vec<_> = (0..3).map(|i| c.game("gba", &format!("G{i} (USA).gba"), &[i + 1; 32])).collect();
    let no_ssuser = r#"{"response": {"jeu": {"id": "19512", "medias": [{"type": "box-2D-side", "region": "us", "format": "png"}]}}}"#;
    let server = Server::new(move |a, _| match a.endpoint.as_str() {
        "jeuInfos.php" => Say::Json(200, no_ssuser.into()),
        _ => Say::Png,
    });
    let r = guest_run(&server, &FakeClock::default(), &c, &games, &[Face::BoxSide]);
    assert_eq!(r.ended, Ended::Refused(Refusal::Unreachable));
    assert_eq!(server.endpoints(), ["jeuInfos.php"], "no picture while the limit is unknown");
    // Restarts: each run is a new client; the probes add up in the file, and after PROBES nothing goes at all.
    for _ in 1..PROBES {
        guest_run(&server, &FakeClock::default(), &c, &games, &[Face::BoxSide]);
    }
    assert_eq!(server.endpoints().len() as u32, PROBES, "{:?}", server.endpoints());
    let quiet = Server::new(|_, _| panic!("a sixth probe"));
    assert_eq!(guest_run(&quiet, &FakeClock::default(), &c, &games, &[Face::BoxSide]).ended, Ended::Refused(Refusal::Unreachable));
    assert_eq!(book(&c.data, None, Booking::Ask, now_unix()), Err(NotBooked::Refused(Refusal::Unreachable)));
    assert_eq!(Ledger { per_day: 1, ..Ledger::default() }.limit(true), Some(Refusal::DayDone), "half of one is nought, not no limit");
    assert_eq!(Ledger { per_day: 1, ..Ledger::default() }.limit(false), None, "an account's one request");
}

/// Review, round 3: two clients at once (the spines' run and a sign-in's check, or the labels' run) book on one file. No
/// booking is lost, and one quota's bookings never drop another's.
#[test]
fn clients_booking_at_once_lose_nothing_and_keep_each_others_quota() {
    let c = card("parallel");
    for who in ["guest", "user:player"] {
        assert!(Ledger { day: french_day(now_unix()), per_day: 1_000_000, ..Ledger::default() }.save(&c.data, who));
    }
    let data = c.data.clone();
    std::thread::scope(|s| {
        for t in 0..6 {
            let data = &data;
            s.spawn(move || {
                let login = (t % 3 == 0).then(player);
                for _ in 0..40 {
                    book(data, login.as_ref(), Booking::Media, now_unix()).unwrap();
                }
            });
        }
    });
    let guest = Ledger::load(&c.data, "guest", now_unix()).unwrap();
    let account = Ledger::load(&c.data, "user:player", now_unix()).unwrap();
    assert_eq!((guest.requests, account.requests), (4 * 40, 2 * 40));
    // An answer taken in raises, never lowers (a stale client's 3 doesn't undo the 160 booked).
    assert!(note(&c.data, None, now_unix(), |l| l.take(&Quota { today: 3, per_day: 1_000_000, ..Quota::default() })));
    assert_eq!(Ledger::load(&c.data, "guest", now_unix()).unwrap().requests, 160);
}

/// Review, round 3: midnight in France between two games (or while a request waits for its turn) starts the new day for
/// every count, the game in the same booking as its request: a guest whose games were used up yesterday looks up today.
#[test]
fn midnight_between_two_games_books_the_next_one_on_the_new_day() {
    let c = card("midnight");
    let today = now_unix();
    let tomorrow = next_french_day(today);
    assert!(Ledger { day: french_day(today), games: GUEST_GAMES_PER_DAY, requests: 4000, probes: 2, per_day: 10_000, ..Ledger::default() }.save(&c.data, "guest"));
    assert_eq!(book(&c.data, None, Booking::Game, today), Err(NotBooked::GameHeld), "today's games are used up");
    let l = book(&c.data, None, Booking::Game, tomorrow).unwrap();
    assert_eq!((l.day, l.games, l.requests, l.probes, l.per_day), (french_day(tomorrow), 1, 1, 0, 10_000), "tomorrow: the game and its request, on the new day");
    // A day's end of yesterday is gone today.
    assert!(note(&c.data, None, tomorrow, |l| l.over = Some(Refusal::QuotaToday)));
    assert!(book(&c.data, None, Booking::Media, next_french_day(tomorrow)).is_ok());
}

/// Review, round 3: a day's file that reads as JSON but whose counts are damaged (a number as text, a null) is damaged:
/// it holds every request back, and nothing is written over it but the day's broken mark.
#[test]
fn a_damaged_count_in_the_days_file_holds_every_request_back() {
    let c = card("schema");
    let file = c.data.join(DIR).join("day.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let damaged = format!("{{\"guest\": {{\"day\": {}, \"requests\": \"5000\", \"ko\": 0, \"games\": null, \"probes\": 0, \"per_day\": 10000, \"ko_per_day\": 1000, \"over\": null}}}}", french_day(now_unix()));
    std::fs::write(&file, &damaged).unwrap();
    assert_eq!(Ledger::load(&c.data, "guest", now_unix()), Err(Unreadable));
    assert_eq!(book(&c.data, None, Booking::Ask, now_unix()), Err(NotBooked::Refused(Refusal::DayDone)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), damaged, "a booking writes nothing over it");
    assert_eq!(day_over(&c.data, None, now_unix()), Some(Refusal::DayDone));
    let quiet = Server::new(|_, _| panic!("asked with a damaged day's file"));
    assert_eq!(guest_run(&quiet, &FakeClock::default(), &c, &[c.game("gba", "A (USA).gba", &[1; 32])], &[Face::BoxSide]).ended, Ended::Refused(Refusal::DayDone));
}

/// Review, round 3: a picture's 403 whose diagnosis (the account's `ssuserInfos`) answers with the day's end ends the day,
/// it isn't turned into the neutral line.
#[test]
fn a_pictures_403_whose_diagnosis_ends_the_day_ends_it() {
    let c = card("media403-day");
    let game = c.game("gba", "Test Quest (Europe).gba", &[7; 64]);
    let server = Server::new(|a, n| match a.endpoint.as_str() {
        "ssuserInfos.php" if n == 0 => Say::Json(200, USER.into()),
        "ssuserInfos.php" => Say::Status(430),
        "jeuInfos.php" => Say::Json(200, FOUND.into()),
        _ => Say::Json(403, String::new()),
    });
    let r = run(&server, &FakeClock::default(), &c, std::slice::from_ref(&game), &[Face::BoxSide], &never);
    assert_eq!(r.ended, Ended::Refused(Refusal::QuotaToday));
    assert_eq!(day_over(&c.data, Some(&player()), now_unix()), Some(Refusal::QuotaToday));
}


/// The known game `A (USA)` of `c`'s gba folder in the index (FOUND's pictures), and a guest's day with the limit known.
fn known_game(c: &Card, requests: u32, day_at: u64) -> (String, PathBuf) {
    let game = c.game("gba", "A (USA).gba", &[1; 32]);
    let (g, _) = parse_game(FOUND.as_bytes()).unwrap();
    let mut index = Index::load(&c.data, "gba");
    index.set("A (USA)", &identity("gba", &game.1, None).unwrap(), Entry::Found { game: g.id, system: g.system, medias: g.medias, rom_regions: g.rom_regions, series: g.series });
    index.save(&c.data, "gba").unwrap();
    assert!(Ledger { day: french_day(day_at), requests, per_day: 10_000, ko_per_day: 1000, ..Ledger::default() }.save(&c.data, "guest"));
    game
}

/// Review round 4: the guest's last request of a day is answered after midnight. The next one is the new day's (no
/// "that's all for today" from yesterday's full count, and nothing of today written over with yesterday's end); a miss
/// answered after midnight counts on the day its request was booked on, not the new one.
#[test]
fn an_answer_after_midnight_counts_on_its_requests_day_and_the_next_goes_on_the_new_day() {
    let c = card("midnight-client");
    let before = next_french_day(now_unix()) - 60;
    let after = before + 120;
    let game = known_game(&c, 4999, before);
    let clock = FakeClock::default();
    clock.unix.store(before, std::sync::atomic::Ordering::SeqCst);
    let unix = clock.unix.clone();
    let server = Server::new(move |a, n| {
        // The day's last allowed request goes before midnight; its answer, a miss, comes after.
        unix.store(after, std::sync::atomic::Ordering::SeqCst);
        match (a.endpoint.as_str(), n) {
            ("mediaJeu.php", 0) => Say::Status(404),
            _ => Say::Png,
        }
    });
    let mut client = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
    let r = run_until(&mut client, &c.data, std::slice::from_ref(&game), &[Face::BoxSide, Face::BoxBack], &|_| {}, &never);
    assert_eq!(r.ended, Ended::Done, "the new day goes on: {r:?}");
    assert_eq!((r.pictures, server.media().len()), (1, 2), "the back went on the new day");
    let today = Ledger::load(&c.data, "guest", after).unwrap();
    assert_eq!((today.day, today.requests, today.ko, today.over), (french_day(after), 1, 0, None), "yesterday's miss and count stay yesterday's");
}

/// Review round 4: what an answer says of the quota (here the day's end) is in the day's file before the next request
/// can be booked by anyone (a second sign-in waiting for the lane books on the file, not on what a caller writes later).
#[test]
fn a_days_end_is_on_the_card_before_the_next_request_can_be_booked() {
    let c = card("lane-answer");
    let game = known_game(&c, 10, now_unix());
    let server = Server::new(|_, _| Say::Status(430));
    let clock = FakeClock::default();
    let mut client = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
    client.keep_day(&c.data);
    let r = client.download(12, 19512, Face::BoxSide, &Media { kind: "box-2D-side".into(), region: "us".into(), format: "png".into() }, &path(&c.data, Face::BoxSide, "gba", &game.1), &never);
    assert_eq!(r, Err(Refusal::QuotaToday));
    assert_eq!(day_over(&c.data, None, now_unix()), Some(Refusal::QuotaToday), "written by the request itself");
    assert_eq!(book(&c.data, None, Booking::Ask, now_unix()), Err(NotBooked::Refused(Refusal::QuotaToday)), "the next booking sees it");
}

/// Review round 4: a card that refuses an answer's count (a miss) holds every further request back until it is written,
/// instead of booking again on the older count.
#[test]
fn an_answer_the_card_refuses_holds_further_requests_back() {
    use std::os::unix::fs::PermissionsExt;
    let c = card("unsaved");
    let game = known_game(&c, 10, now_unix());
    let dir = c.data.join(DIR);
    // A card that refuses writes (as root nothing is refused: nothing to test then).
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let refuses = std::fs::write(dir.join("probe"), b"x").is_err();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !refuses {
        eprintln!("skipped: this user writes into a read-only folder");
        return;
    }
    let d2 = dir.clone();
    let server = Server::new(move |_, _| {
        std::fs::set_permissions(&d2, std::fs::Permissions::from_mode(0o555)).unwrap();
        Say::Status(404)
    });
    let clock = FakeClock::default();
    let mut client = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
    client.keep_day(&c.data);
    let media = Media { kind: "box-2D-side".into(), region: "us".into(), format: "png".into() };
    let dest = path(&c.data, Face::BoxSide, "gba", &game.1);
    assert_eq!(client.download(12, 19512, Face::BoxSide, &media, &dest, &never), Ok(Some(Got::Nothing)));
    let second = client.download(12, 19512, Face::BoxBack, &media, &path(&c.data, Face::BoxBack, "gba", &game.1), &never);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(second, Err(Refusal::DayDone), "held back while the miss isn't written");
    assert_eq!(server.media().len(), 1, "nothing more was sent");
}

/// Review round 4: the row's "Today: n of m" is the count the limits use (every booked request, the share this quota
/// may use), not the server's last number.
#[test]
fn the_days_count_shown_is_the_one_the_limits_use() {
    let c = card("day-count");
    known_game(&c, 1234, now_unix());
    let server = Server::new(|_, _| panic!("nothing asked"));
    let clock = FakeClock::default();
    let mut guest = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
    guest.keep_day(&c.data);
    assert_eq!(guest.day_count(), (1234, 5000), "a guest's half of 10 000");
    // Another client books meanwhile: the row shows the file's count, not this client's last look (review round 5).
    for _ in 0..5 {
        book(&c.data, None, Booking::Media, now_unix()).unwrap();
    }
    assert_eq!(guest.day_count(), (1239, 5000));
}

/// Review round 5: a day's end the card refused to take belongs to the whole program, not to the client that got it. A
/// gets 430 while the card refuses writes and its run ends (the client is dropped); once the card takes writes again,
/// the next client writes A's stop first and sends nothing.
#[test]
fn a_days_end_the_card_refused_stops_the_next_client_too() {
    use std::os::unix::fs::PermissionsExt;
    let c = card("unsaved-shared");
    let game = known_game(&c, 10, now_unix());
    let dir = c.data.join(DIR);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let refuses = std::fs::write(dir.join("probe"), b"x").is_err();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !refuses {
        eprintln!("skipped: this user writes into a read-only folder");
        return;
    }
    let media = Media { kind: "box-2D-side".into(), region: "us".into(), format: "png".into() };
    let dest = path(&c.data, Face::BoxSide, "gba", &game.1);
    {
        let d2 = dir.clone();
        let server = Server::new(move |_, _| {
            std::fs::set_permissions(&d2, std::fs::Permissions::from_mode(0o555)).unwrap();
            Say::Status(430)
        });
        let clock = FakeClock::default();
        let mut a = Client::new(&server, &clock, BASE, test_dev(), None).unwrap();
        a.keep_day(&c.data);
        assert_eq!(a.download(12, 19512, Face::BoxSide, &media, &dest, &never), Err(Refusal::QuotaToday));
    }
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(day_over(&c.data, None, now_unix()), None, "the card refused it: not written yet");
    let quiet = Server::new(|_, _| panic!("sent after a day's end"));
    let clock = FakeClock::default();
    let mut b = Client::new(&quiet, &clock, BASE, test_dev(), None).unwrap();
    b.keep_day(&c.data);
    assert_eq!(b.download(12, 19512, Face::BoxSide, &media, &dest, &never), Err(Refusal::QuotaToday));
    assert_eq!(day_over(&c.data, None, now_unix()), Some(Refusal::QuotaToday), "written by the next client before its request");
}

/// Review round 5: an answer's `ssuser` is taken in once, under the lane, in the order the requests went; nothing applies
/// an older answer again later (its limit over a newer, smaller one).
#[test]
fn each_answers_limits_are_taken_once_in_the_order_they_came() {
    let c = card("ssuser-once");
    known_game(&c, 10, now_unix());
    let first = USER.to_string();
    let second = USER.replace("\"maxrequestsperday\": \"20000\"", "\"maxrequestsperday\": \"10000\"");
    assert_ne!(first, second);
    let (sa, sb) = (Server::new(move |_, _| Say::Json(200, first.clone())), Server::new(move |_, _| Say::Json(200, second.clone())));
    let clock = FakeClock::default();
    let mut a = Client::new(&sa, &clock, BASE, test_dev(), Some(player())).unwrap();
    let mut b = Client::new(&sb, &clock, BASE, test_dev(), Some(player())).unwrap();
    a.keep_day(&c.data);
    b.keep_day(&c.data);
    assert_eq!(a.user_info(&never).unwrap().map(|q| q.per_day), Some(20000));
    assert_eq!(b.user_info(&never).unwrap().map(|q| q.per_day), Some(10000));
    // A's run goes on with what it read (its jeuInfos answers are read, not taken in a second time).
    assert_eq!(Ledger::load(&c.data, &quota_of(Some(&player())), now_unix()).unwrap().per_day, 10000, "the newer answer's limit stays");
}
