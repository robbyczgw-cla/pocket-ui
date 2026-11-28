//! A live ScreenScraper run from a host build (docs/SCREENSCRAPER.md "Live runs"): the real client, the real job
//! (`screenscraper::run_until`), the real server.
//!   cargo run --release --example screenscraper_live -- <data dir> <list> [--record <dir>] [--want side,back,support,texture]
//! `<list>`: one `system<TAB>path` per line. The developer id is read at run time from
//! `${SCREENSCRAPER_DEV_FILE:-~/.config/screenscraper-dev}` (never from the build, never printed); a player's login comes
//! from `<data dir>/screenscraper/login` as on a device. `--record` keeps every answer's body (ids and passwords
//! replaced by `xxx`, `commandRequested` dropped) for `testdata/screenscraper/`. Prints a line per request (endpoint,
//! status, bytes: never the address) and a table per game at the end: found, which pictures ScreenScraper has, which
//! were saved.
use pocket_ui::romm::Fetched;
use pocket_ui::romm_net::{self, Answer, Net};
use pocket_ui::screenscraper::{self, Entry, Face};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

fn dev_from_file() -> screenscraper::Dev {
    let file = std::env::var("SCREENSCRAPER_DEV_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/screenscraper-dev"));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|_| panic!("no developer id file at {}", file.display()));
    let mut dev = screenscraper::Dev { id: String::new(), password: String::new(), softname: "pocket-ui".into() };
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(v) = line.strip_prefix("DEVID=") {
            dev.id = v.to_string();
        } else if let Some(v) = line.strip_prefix("DEVPASSWORD=") {
            dev.password = v.to_string();
        } else if let Some(v) = line.strip_prefix("SOFTNAME=") {
            dev.softname = v.to_string();
        }
    }
    assert!(!dev.id.is_empty() && !dev.password.is_empty(), "the developer id file lacks DEVID or DEVPASSWORD");
    dev
}

/// The endpoint of an address, without the query (which carries the ids).
fn endpoint(url: &str) -> String {
    let path = url.split('?').next().unwrap_or_default();
    path.rsplit('/').next().unwrap_or_default().to_string()
}

/// The query's fields that carry no secret, for the log.
fn public_params(url: &str) -> String {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
    query
        .split('&')
        .filter(|kv| {
            let k = kv.split('=').next().unwrap_or_default();
            matches!(k, "systemeid" | "crc" | "romtaille" | "jeuid" | "media" | "romnom")
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// `s` as an address's query carries it.
fn percent(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

struct Recorder {
    inner: Box<dyn Net>,
    secrets: Vec<String>,
    record: Option<PathBuf>,
    n: Mutex<usize>,
    pub statuses: Mutex<BTreeMap<String, BTreeMap<u16, usize>>>,
}

impl Recorder {
    fn clean(&self, body: &[u8]) -> Vec<u8> {
        let mut text = String::from_utf8_lossy(body).into_owned();
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(h) = v.get_mut("header").and_then(|h| h.as_object_mut()) {
                h.remove("commandRequested");
            }
            text = serde_json::to_string_pretty(&v).unwrap_or(text);
        }
        // Every secret as it is and as an address carries it (percent-encoded).
        for s in self.secrets.iter().filter(|s| !s.is_empty()) {
            text = text.replace(s.as_str(), "xxx").replace(&percent(s), "xxx");
        }
        text.into_bytes()
    }

    /// One line per request: endpoint, status, size and the fields without a secret, never a body (an error page may
    /// echo the address with the ids). `--record` keeps only answers that are JSON with a 200, cleaned.
    fn note(&self, ep: &str, status: u16, bytes: u64, params: &str, body: Option<&[u8]>) {
        let n = {
            let mut n = self.n.lock().unwrap();
            *n += 1;
            *n
        };
        *self.statuses.lock().unwrap().entry(ep.to_string()).or_default().entry(status).or_default() += 1;
        let nomedia = body.is_some_and(|b| b.trim_ascii().starts_with(b"NOMEDIA"));
        println!("#{n:04} {ep:<18} {status:>3} {bytes:>8} B  {params}{}", if nomedia { "  NOMEDIA" } else { "" });
        if let (Some(dir), Some(b), 200) = (&self.record, body, status) {
            if serde_json::from_slice::<serde_json::Value>(b).is_ok() {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(dir.join(format!("{n:04}-{ep}-{status}.txt")), self.clean(b));
            }
        }
    }
}

impl Net for Recorder {
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer {
        let a = self.inner.call(method, url, token, json);
        self.note(&endpoint(url), a.status, a.body.len() as u64, &public_params(url), Some(&a.body));
        a
    }

    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, stop: &AtomicBool, progress: &(dyn Fn(u64) + Sync)) -> Fetched {
        let f = self.inner.fetch(url, token, part, size, stop, progress);
        let bytes = std::fs::metadata(part).map_or(0, |m| m.len());
        let status = match f {
            Fetched::Done => 200,
            Fetched::Status(s) => s,
            _ => 0,
        };
        // A short answer (NOMEDIA, an error line) is worth keeping; a picture is not.
        let body = (bytes < 400).then(|| std::fs::read(part).unwrap_or_default());
        self.note(&endpoint(url), status, bytes, &public_params(url), body.as_deref());
        f
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(data), Some(list)) = (args.first(), args.get(1)) else {
        eprintln!("usage: screenscraper_live <data dir> <list> [--record <dir>] [--want side,back,support,texture,cover,supporttexture,wheel]");
        std::process::exit(2);
    };
    let data = PathBuf::from(data);
    let mut record = None;
    let mut want = vec![Face::BoxSide, Face::BoxBack, Face::Support, Face::BoxTexture];
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--record" => {
                record = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--want" => {
                want = args
                    .get(i + 1)
                    .map(|w| {
                        w.split(',')
                            .filter_map(|f| match f {
                                "side" => Some(Face::BoxSide),
                                "back" => Some(Face::BoxBack),
                                "support" => Some(Face::Support),
                                "texture" => Some(Face::BoxTexture),
                                "cover" => Some(Face::Cover),
                                "supporttexture" => Some(Face::SupportTexture),
                                "wheel" => Some(Face::Wheel),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let games: Vec<(String, PathBuf)> = std::fs::read_to_string(list)
        .expect("list")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(s, p)| (s.to_string(), PathBuf::from(p)))
        .collect();
    let dev = dev_from_file();
    let login = screenscraper::login(&data);
    let mut secrets = vec![dev.id.clone(), dev.password.clone()];
    if let Some(l) = &login {
        secrets.push(l.user.clone());
        secrets.push(l.password.clone());
    }
    let net = Recorder { inner: romm_net::system(), secrets, record, n: Mutex::new(0), statuses: Mutex::new(BTreeMap::new()) };
    let clock = screenscraper::RealClock::new();
    let mut client = screenscraper::Client::new(&net, &clock, screenscraper::API, dev, login).expect("https");
    let started = std::time::Instant::now();
    let run = screenscraper::run_until(
        &mut client,
        &data,
        &games,
        &want,
        &|p| eprintln!("  {}/{} games, {} pictures", p.done, p.total, p.pictures),
        &|| false,
    );
    let q = &client.quota;
    println!();
    println!(
        "run: asked {} found {} pictures {} no_checksum {} ended {:?} in {:.0} s",
        run.asked,
        run.found,
        run.pictures,
        run.no_checksum,
        run.ended,
        started.elapsed().as_secs_f64()
    );
    println!(
        "quota: guest={} threads {} kbps {} per_min {} today {}/{} ko {}/{}",
        q.user.is_empty(),
        q.max_threads,
        q.max_kbps,
        q.per_min,
        q.today,
        q.per_day,
        q.ko_today,
        q.ko_per_day
    );
    println!("statuses: {:?}", net.statuses.lock().unwrap());
    // Per game: what the index says ScreenScraper has, and what is on the card now.
    let kinds = ["box-2D", "box-2D-side", "box-2D-back", "support-2D", "box-texture", "support-texture"];
    println!();
    println!("system\tgame\tfound\t{}\tsaved", kinds.join("\t"));
    let mut indexes: BTreeMap<String, screenscraper::Index> = BTreeMap::new();
    for (sys, rom) in &games {
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let index = indexes.entry(sys.clone()).or_insert_with(|| screenscraper::Index::load(&data, sys));
        let id = screenscraper::identity(sys, rom, None);
        let entry = id.as_ref().and_then(|id| index.get(&stem, id)).cloned();
        let (found, have): (&str, Vec<String>) = match &entry {
            Some(Entry::Found { medias, .. }) => {
                ("yes", kinds.iter().map(|k| medias.iter().filter(|m| m.kind == *k).map(|m| m.region.clone()).collect::<Vec<_>>().join(",")).collect())
            }
            Some(Entry::Missing) => ("no", kinds.iter().map(|_| String::new()).collect()),
            None if id.is_none() => ("no-crc", kinds.iter().map(|_| String::new()).collect()),
            None => ("?", kinds.iter().map(|_| String::new()).collect()),
        };
        let saved: Vec<&str> = [Face::Cover, Face::BoxSide, Face::BoxBack, Face::Support, Face::BoxTexture, Face::SupportTexture]
            .into_iter()
            .filter(|&f| screenscraper::existing(&data, f, sys, rom).is_some())
            .map(|f| f.folder())
            .collect();
        println!("{sys}\t{stem}\t{found}\t{}\t{}", have.join("\t"), saved.join(","));
    }
}
