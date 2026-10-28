//! One request, from its head to its answer: the Host check, who is asking ([`Principal`]), the CSRF and origin checks
//! of writes, the pause while a game runs, and the JSON API the page uses. The routes are listed in
//! docs/user/web-companion.md's developer notes (docs/ARCHITECTURE.md, "Web companion").
//!
//! Reads come from the files the App writes (library.json, favorites.json, recents.json, curator.json), the library as
//! `library::load` reads it (cached for five minutes, read again after an upload), gamedb, and Curator's index as the
//! App last built it. Writes go to the App (`Shared::edit`), never to those files directly, so the shelf and the
//! files never disagree.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::auth::{self, Browser, Tried};
use super::files::{self, Refused, Roots};
use super::http::{self, BadRequest, Body, Request, Response};
use super::net::{self, LocalApi, SelfNode, Whois};
use super::{assets, Edit, Shared};
use crate::library::{self, SysInfo};
use crate::library_meta::{LibraryMeta, Status};

/// The library is read again after this long (and after every upload).
const LIBRARY_FRESH: Duration = Duration::from_secs(5 * 60);
/// What tailscaled said about a peer or about this node is kept this long.
const WHOIS_FRESH: Duration = Duration::from_secs(60);
/// One new screenshot at most this often; in between, the last one is served again.
pub const SHOT_EVERY: Duration = Duration::from_secs(10);
/// Games in one list answer at most (a system of more pages through `offset`).
const PAGE: usize = 600;
/// What a pairing try that did not pair says, whatever the reason.
pub const NOT_PAIRED: &str = "That code is not right, or no code is showing. On the Pocket: Settings › Connections › Web companion › Pair a browser.";
/// A cover larger than this is not served (nor read).
const COVER_MAX: u64 = 8 << 20;
/// A screenshot is a PNG of the screen at most (1240 x 1080); this is far above one.
const SHOT_MAX: u64 = 16 << 20;
/// A game's page lists this many screenshots, the newest.
const SHOTS_MAX: usize = 60;
/// Pairing tries are answered at most this often, whatever they were.
pub const PAIR_SPACING: Duration = Duration::from_secs(1);
/// The request line and headers must have arrived within this long (a client that sends a byte every few seconds
/// would otherwise keep a worker).
const HEAD_DEADLINE: Duration = Duration::from_secs(15);

/// What the companion asks tailscaled; tests stand in for it.
pub trait Tailnet: Send + Sync {
    fn self_node(&self) -> Option<SelfNode>;
    fn whois(&self, peer: SocketAddr) -> Option<Whois>;
}

impl Tailnet for LocalApi {
    fn self_node(&self) -> Option<SelfNode> {
        LocalApi::self_node(self).ok()
    }
    fn whois(&self, peer: SocketAddr) -> Option<Whois> {
        LocalApi::whois(self, peer).ok()
    }
}

/// The connection a request came on.
#[derive(Clone, Copy, Debug)]
pub struct Conn {
    pub peer: SocketAddr,
    pub local: SocketAddr,
    /// When the supervisor accepted it: the head's deadline counts from here, so a connection that waited in the
    /// queue has had its time (review F2).
    pub accepted: Instant,
}

/// Who is asking.
#[derive(Clone, Debug, PartialEq)]
pub enum Principal {
    Browser(Browser),
    /// One of the owner's tailnet devices.
    Owner(Whois),
}

impl Principal {
    fn id(&self) -> String {
        match self {
            Principal::Browser(b) => format!("b:{}", b.id),
            Principal::Owner(w) => format!("t:{}", w.node),
        }
    }
}

/// The library as the companion lists it.
pub struct LibCache {
    pub systems: Vec<library::System>,
    /// Every game system of es_systems.cfg, also those without games: where uploads may go.
    pub info: Vec<SysInfo>,
    by_id: HashMap<String, (usize, usize)>,
    by_path: HashMap<PathBuf, (usize, usize)>,
    search: crate::search::Index,
    at: Instant,
    /// `Shared::library_epoch` when it was read.
    epoch: u64,
}

/// A game's id in the API: the first 16 hex digits of the SHA-256 of its path. Paths never travel.
pub fn game_id(path: &Path) -> String {
    use sha2::Digest;
    super::hex(&sha2::Sha256::digest(path.to_string_lossy().as_bytes())[..8])
}

impl LibCache {
    pub fn build(systems: Vec<library::System>, info: Vec<SysInfo>) -> LibCache {
        let mut by_id = HashMap::new();
        let mut by_path = HashMap::new();
        for (s, sys) in systems.iter().enumerate() {
            for (g, game) in sys.games.iter().enumerate() {
                by_id.insert(game_id(&game.path), (s, g));
                by_path.insert(game.path.clone(), (s, g));
            }
        }
        let search = crate::search::Index::build(systems.iter().enumerate().flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, game)| (s, g, game.name.as_str()))));
        LibCache { systems, info, by_id, by_path, search, at: Instant::now(), epoch: 0 }
    }

    fn game(&self, id: &str) -> Option<(usize, usize)> {
        self.by_id.get(id).copied()
    }
}

thread_local! {
    /// gamedb unpacks one system at a time and keeps it; each worker keeps its own (it is not shared across threads).
    static GAMEDB: RefCell<Option<crate::gamedb::GameDb>> = const { RefCell::new(None) };
}

/// Everything a worker needs to answer.
pub struct Ctx {
    pub shared: Arc<Shared>,
    pub roots: Roots,
    pub tailnet: Box<dyn Tailnet>,
    /// The screen as it is now (`image::capture_screen`); tests stand in for it.
    pub screenshot: Box<dyn Fn() -> Option<crate::image::Image> + Send + Sync>,
    cache: Mutex<Option<Arc<LibCache>>>,
    stale: AtomicBool,
    whois: Mutex<HashMap<IpAddr, (Instant, Option<Whois>)>>,
    me: Mutex<Option<(Instant, Option<SelfNode>)>>,
    shot: Mutex<Option<(Instant, Arc<Vec<u8>>)>>,
    /// The play history's read function for this server; None: the one registered with `web::set_history_source`
    /// (tests stand in for it here).
    pub history: Option<super::HistorySource>,
    /// When the last pairing try came. Held while a code is checked, so tries from several connections queue up one
    /// behind the other, a second apart, whether a code is showing or not.
    wrong_code: Mutex<Option<Instant>>,
    /// The server was switched off: nothing new starts, an upload stops at its next chunk (`Server::start` puts its
    /// own flag here).
    pub stopped: Arc<AtomicBool>,
    /// The request line and headers must be in this long after the connection was accepted; a body other than an
    /// upload's this long after the head ([`HEAD_DEADLINE`]; tests make it short).
    pub deadline: Duration,
    /// `Shared::trust_epoch` when the whois answers were kept: a change of "Your Tailscale devices" drops them.
    whois_epoch: AtomicU64,
}

impl Ctx {
    pub fn new(shared: Arc<Shared>, roots: Roots, tailnet: Box<dyn Tailnet>) -> Ctx {
        Ctx {
            shared,
            roots,
            tailnet,
            screenshot: Box::new(|| crate::image::capture_screen(Duration::from_secs(3))),
            cache: Mutex::new(None),
            stale: AtomicBool::new(false),
            whois: Mutex::new(HashMap::new()),
            me: Mutex::new(None),
            shot: Mutex::new(None),
            history: None,
            wrong_code: Mutex::new(None),
            stopped: Arc::new(AtomicBool::new(false)),
            deadline: HEAD_DEADLINE,
            whois_epoch: AtomicU64::new(0),
        }
    }

    /// The library, read now if it is older than five minutes or an upload changed it. One reader at a time.
    pub fn library(&self) -> Arc<LibCache> {
        let mut guard = self.cache.lock().unwrap();
        // A game hidden, shown again or deleted on the Pocket (`Shared::library_changed`) makes it stale at once.
        let epoch = self.shared.library_epoch.load(Ordering::Relaxed);
        let fresh = guard.as_ref().is_some_and(|c| c.at.elapsed() < LIBRARY_FRESH && c.epoch == epoch) && !self.stale.load(Ordering::Relaxed);
        if !fresh {
            self.stale.store(false, Ordering::Relaxed);
            let es = &self.shared.es_systems;
            // Without the games hidden on the Pocket, as the shelf (`library::load_shown`).
            let systems = library::load_shown(es, &self.shared.data_dir).unwrap_or_default();
            let info = library::all_systems(es).unwrap_or_default();
            let mut lib = LibCache::build(systems, info);
            lib.epoch = epoch;
            *guard = Some(Arc::new(lib));
        }
        guard.as_ref().unwrap().clone()
    }

    /// Uses a library built elsewhere (tests).
    pub fn set_library(&self, lib: LibCache) {
        *self.cache.lock().unwrap() = Some(Arc::new(lib));
    }

    /// This node, from tailscaled (kept a minute).
    pub fn me(&self) -> Option<SelfNode> {
        let mut guard = self.me.lock().unwrap();
        if let Some((at, me)) = guard.as_ref() {
            if at.elapsed() < WHOIS_FRESH {
                return me.clone();
            }
        }
        let me = self.tailnet.self_node();
        if let Some(n) = &me {
            let short = n.dns_name.split('.').next().filter(|s| !s.is_empty()).unwrap_or(&n.hostname).to_string();
            *self.shared.tailnet_name.lock().unwrap() = Some(short).filter(|s| !s.is_empty());
        }
        *guard = Some((Instant::now(), me.clone()));
        me
    }

    fn whois(&self, peer: SocketAddr) -> Option<Whois> {
        let mut guard = self.whois.lock().unwrap();
        let epoch = self.shared.trust_epoch.load(Ordering::Relaxed);
        if self.whois_epoch.swap(epoch, Ordering::Relaxed) != epoch {
            guard.clear();
        }
        if let Some((at, w)) = guard.get(&peer.ip()) {
            if at.elapsed() < WHOIS_FRESH {
                return w.clone();
            }
        }
        if guard.len() > 256 {
            guard.clear();
        }
        let w = self.tailnet.whois(peer);
        guard.insert(peer.ip(), (Instant::now(), w.clone()));
        w
    }

    /// Who is asking: a paired browser by its cookie, else (when the player allows it) the owner's tailnet device.
    pub fn principal(&self, conn: &Conn, req: &Request) -> Option<Principal> {
        // Every cookie of that name is tried: one set for a wider domain by another site can't hide the real one
        // (review F16).
        for token in req.cookies(auth::COOKIE) {
            if let Some(b) = self.shared.sessions.lock().unwrap().check(token) {
                return Some(Principal::Browser(b));
            }
        }
        if self.shared.prefs().trust_tailnet {
            let me = self.me()?;
            let peer = self.whois(conn.peer)?;
            if net::owner_allows(&me, &peer) {
                return Some(Principal::Owner(peer));
            }
        }
        None
    }

    fn host_ok(&self, req: &Request) -> bool {
        let Some(host) = req.host() else { return false };
        let listening = self.shared.listening.lock().unwrap().clone();
        let names = self.me().map(|m| m.names()).unwrap_or_default();
        net::host_allowed(&host, &names, &listening)
    }
}

/// The socket as the request is read from it: until the head is in, every read must end before `until` (a client
/// that trickles a byte every few seconds is cut off); then reads may wait a minute each (a phone on a weak network
/// pauses in the middle of an upload).
struct Timed {
    stream: TcpStream,
    until: Option<Instant>,
}

impl Read for Timed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let wait = match self.until {
            Some(until) => until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or(std::io::ErrorKind::TimedOut)?,
            None => Duration::from_secs(60),
        };
        self.stream.set_read_timeout(Some(wait))?;
        self.stream.read(buf)
    }
}

/// Answers one connection: reads the request, answers it, closes.
pub fn handle(ctx: &Ctx, conn: &Conn, stream: TcpStream) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(read_half) = stream.try_clone() else { return };
    // From the moment it was accepted: time spent waiting in the queue counts (review F2).
    let mut reader = BufReader::with_capacity(16 * 1024, Timed { stream: read_half, until: Some(conn.accepted + ctx.deadline) });
    let mut writer = stream;
    let t0 = Instant::now();
    let (resp, head_only, line) = match http::read_head(&mut reader) {
        Ok(req) => {
            // An upload's body may take hours (a minute per read at most); any other body is at most 64 KB and gets
            // the same short deadline as the head, so a client trickling a pairing body can't keep a worker.
            reader.get_mut().until = if req.method == "POST" && req.path == "/api/upload" { None } else { Some(Instant::now() + ctx.deadline) };
            let resp = respond(ctx, conn, &req, &mut reader);
            (resp, req.method == "HEAD", Some(format!("{} {}", req.method, req.path)))
        }
        Err(BadRequest::Closed) => return,
        Err(e) => (Response::error(e.status(), "Bad request"), false, None),
    };
    ctx.shared.requests.fetch_add(1, Ordering::Relaxed);
    // The log names the route and the outcome, never a cookie, a code, a query or an address.
    if resp.status >= 400 || line.as_deref().is_some_and(|l| l.contains("/api/") && !l.ends_with("/api/status")) {
        println!("web: {} {} ({} ms)", line.as_deref().unwrap_or("?"), resp.status, t0.elapsed().as_millis());
    }
    if http::write_response(&mut writer, &resp, head_only).is_ok() {
        // A refused upload's body may still be on its way: read a little of it, so that closing does not reset the
        // connection before the browser has read why (it would only say the connection broke off).
        let _ = writer.shutdown(std::net::Shutdown::Write);
        reader.get_mut().until = Some(Instant::now() + Duration::from_secs(1));
        let _ = std::io::copy(&mut (&mut reader).take(1 << 20), &mut std::io::sink());
    }
}

/// Requests that are answered while a game runs: they cost next to nothing (the screenshot is rate-limited).
fn small(path: &str) -> bool {
    matches!(path, "/api/session" | "/api/status" | "/api/screenshot" | "/api/pair")
}

/// A write from this page, not from another site: `Sec-Fetch-Site` same-origin (or absent, older browsers) and an
/// `Origin`, when sent, of the host asked for.
fn same_origin(req: &Request) -> bool {
    if let Some(site) = req.header("sec-fetch-site") {
        if site != "same-origin" {
            return false;
        }
    }
    match req.header("origin") {
        None => true,
        Some(origin) => {
            let host = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")).unwrap_or("");
            let probe = Request { headers: vec![("host".into(), host.to_string())], ..Request::default() };
            probe.host().is_some() && probe.host() == req.host()
        }
    }
}

pub fn respond(ctx: &Ctx, conn: &Conn, req: &Request, body: &mut impl BufRead) -> Response {
    if !ctx.host_ok(req) {
        return Response::error(421, "Open the companion with the Pocket's own name or address.");
    }
    let (m, path) = (req.method.as_str(), req.path.as_str());
    if !matches!(m, "GET" | "HEAD" | "POST") {
        return Response::error(405, "Not allowed.");
    }
    if m != "POST" {
        if let Some((ty, bytes, keep)) = assets::find(path) {
            let mut r = if path == "/" || path == "/index.html" {
                Response::new(200, ty, assets::INDEX_HTML.replace("{{v}}", crate::update::app_version()).into_bytes())
            } else {
                Response { status: 200, content_type: ty, headers: Vec::new(), body: Body::Static(bytes) }
            };
            if keep {
                r = r.with("Cache-Control", "public, max-age=31536000, immutable");
            }
            return r;
        }
    }
    // The API answers this page only: a write, and any API request a modern browser marks as coming from another
    // site, is refused (another site's page can't read the answers anyway, but must not cause a screenshot either).
    if (m == "POST" || path.starts_with("/api/")) && !same_origin(req) {
        return Response::error(403, "Refused: the request did not come from this page.");
    }
    let who = ctx.principal(conn, req);
    match (m, path) {
        ("POST", "/api/pair") => return pair(ctx, conn, req, body),
        (_, "/api/session") => return session(ctx, who.as_ref()),
        _ => {}
    }
    let Some(who) = who else {
        return Response::error(401, "Pair this browser first.");
    };
    if m == "POST" && !ctx.shared.sessions.lock().unwrap().csrf_ok(&who.id(), req.header("x-csrf-token")) {
        return Response::error(403, "Refused: the page is out of date. Reload it.");
    }
    if ctx.shared.playing() && !small(path) {
        return Response::json(503, &json!({ "error": "A game is running. The companion waits until it ends.", "playing": true })).with("Retry-After", "10");
    }
    let r = match (m, path) {
        (_, "/api/status") => Ok(status(ctx)),
        (_, "/api/library") => Ok(library_overview(ctx)),
        (_, "/api/games") => games(ctx, req),
        (_, "/api/game") => game(ctx, req),
        (_, "/api/cover") => cover(ctx, req),
        (_, "/api/shot") => shot(ctx, req),
        (_, "/api/stats") => Ok(stats(ctx)),
        (_, "/api/battery") => Ok(battery(ctx)),
        (_, "/api/screenshot") => Ok(screenshot(ctx)),
        (_, "/api/uploads") => Ok(uploads(ctx)),
        ("POST", "/api/upload/check") => Ok(upload_check(ctx, req)),
        ("POST", "/api/upload") => Ok(upload(ctx, req, body)),
        ("POST", "/api/logout") => Ok(logout(ctx, &who)),
        ("POST", p) if p.starts_with("/api/") => edit(ctx, p, req, body),
        _ => Err(Response::error(404, "Not found.")),
    };
    r.unwrap_or_else(|e| e)
}

// --- session and pairing -----------------------------------------------------------------------------------------

fn session(ctx: &Ctx, who: Option<&Principal>) -> Response {
    // Before pairing, only what the page needs to greet: not whether a code is on the Pocket's screen now.
    let base = json!({ "version": crate::update::app_version(), "playing": ctx.shared.playing() });
    let Some(who) = who else {
        return Response::json(200, &json!({ "signedIn": false, "info": base }));
    };
    let csrf = ctx.shared.sessions.lock().unwrap().csrf(&who.id());
    let (via, name) = match who {
        Principal::Browser(b) => ("paired", b.name.clone()),
        Principal::Owner(w) => ("tailscale", if w.login.is_empty() { w.device.clone() } else { w.login.clone() }),
    };
    Response::json(200, &json!({ "signedIn": true, "via": via, "name": name, "csrf": csrf, "info": base }))
}

fn pair(ctx: &Ctx, conn: &Conn, req: &Request, body: &mut impl Read) -> Response {
    let v = match json_body(req, body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    // A second after any try, for everybody: tries queue up behind this lock (also from several connections at once),
    // so at most one a second is answered, whether a code is showing or not; five wrong ones end the code anyway.
    let mut gate = ctx.wrong_code.lock().unwrap();
    if let Some(at) = *gate {
        let since = at.elapsed();
        if since < PAIR_SPACING {
            std::thread::sleep(PAIR_SPACING - since);
        }
    }
    let code = v.get("code").and_then(|c| c.as_str()).unwrap_or("");
    let tried = auth::try_code(&mut ctx.shared.pairing.lock().unwrap(), code, Instant::now());
    *gate = Some(Instant::now());
    drop(gate);
    match tried {
        Tried::Paired => {
            let mut name = auth::browser_name(req.header("user-agent").unwrap_or(""));
            if let Some(w) = ctx.whois(conn.peer).filter(|w| !w.device.is_empty()) {
                name = format!("{name} ({})", w.device);
            }
            let token = ctx.shared.sessions.lock().unwrap().pair(&name);
            ctx.shared.notify();
            match token {
                Ok(token) => {
                    println!("web: a browser was paired ({name})");
                    Response::json(200, &json!({ "ok": true, "name": name })).with("Set-Cookie", &format!("{}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}", auth::COOKIE, auth::SESSION_LIFETIME.as_secs()))
                }
                Err(_) => Response::error(500, "The Pocket could not save the pairing. Try again."),
            }
        }
        // The same answer whether the code was wrong or none is showing, and no count of tries: nobody learns from it
        // when a code appears (review F5).
        Tried::Wrong => {
            ctx.shared.notify();
            Response::error(403, NOT_PAIRED)
        }
        Tried::NoCode => Response::error(403, NOT_PAIRED),
    }
}

fn logout(ctx: &Ctx, who: &Principal) -> Response {
    if let Principal::Browser(b) = who {
        ctx.shared.sessions.lock().unwrap().revoke(&b.id);
        ctx.shared.notify();
    }
    Response::json(200, &json!({ "ok": true })).with("Set-Cookie", &format!("{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0", auth::COOKIE))
}

// --- reading ---------------------------------------------------------------------------------------------------

fn json_body(req: &Request, body: &mut impl Read) -> Result<Value, Response> {
    let bytes = http::read_body(body, req, http::MAX_JSON).map_err(|s| Response::error(s, if s == 413 { "That request is too large." } else { "The request was cut off." }))?;
    serde_json::from_slice(&bytes).map_err(|_| Response::error(400, "Bad request."))
}

fn status(ctx: &Ctx) -> Response {
    let now = ctx.shared.now_playing.lock().unwrap().clone();
    // The system's full name from the library when it was read already: status never reads it (it answers in games).
    let cached = ctx.cache.lock().unwrap().clone();
    let playing = now.map(|n| {
        let full = cached.as_ref().and_then(|c| c.systems.iter().find(|s| s.name == n.system).map(|s| s.fullname.clone())).unwrap_or_else(|| n.system.clone());
        json!({ "id": game_id(&n.path), "name": n.name, "system": n.system, "systemName": full, "stripe": stripe(&n.system), "since": n.since })
    });
    Response::json(200, &json!({ "playing": playing, "gameRunning": ctx.shared.playing() }))
}

/// What the player said about games, read from the App's files.
struct Player {
    meta: LibraryMeta,
    favorites: std::collections::HashSet<PathBuf>,
    recents: HashMap<PathBuf, (u64, u64)>,
}

fn player(data_dir: &Path) -> Player {
    Player {
        meta: LibraryMeta::load(data_dir),
        favorites: crate::favorites::Favorites::load(data_dir).list.into_iter().map(|f| f.path).collect(),
        recents: crate::recents::Recents::load(data_dir).list.into_iter().map(|r| (r.path, (r.last_played, r.play_secs))).collect(),
    }
}

fn status_key(s: Status) -> &'static str {
    match s {
        Status::NotStarted => "not_started",
        Status::Playing => "playing",
        Status::Paused => "paused",
        Status::Completed => "completed",
        Status::Dropped => "dropped",
    }
}

fn parse_status(s: &str) -> Option<Status> {
    Status::ALL.into_iter().find(|st| status_key(*st) == s)
}

fn row(ctx: &Ctx, lib: &LibCache, p: &Player, s: usize, g: usize) -> Value {
    let sys = &lib.systems[s];
    let game = &sys.games[g];
    let m = p.meta.get(&game.path);
    let (last, recent_secs) = p.recents.get(&game.path).copied().unwrap_or((0, 0));
    json!({
        "id": game_id(&game.path),
        "name": game.name,
        "system": sys.name,
        "systemName": sys.fullname,
        "stripe": stripe(&sys.name),
        "cover": crate::covers::cover_path(&ctx.shared.data_dir, &sys.name, &game.path).is_file(),
        "favorite": p.favorites.contains(&game.path),
        "status": status_key(m.status),
        "sessions": m.sessions,
        "secs": m.secs.max(recent_secs),
        "lastPlayed": last,
        "stickers": m.stickers,
        "rating": m.rating,
    })
}

/// The system's edge stripe on the shelf, as CSS ("#c8102e").
fn stripe(system: &str) -> String {
    format!("#{:06x}", crate::bezel::system_colour(system) & 0xff_ffff)
}

fn library_overview(ctx: &Ctx) -> Response {
    let lib = ctx.library();
    let p = player(&ctx.shared.data_dir);
    let systems: Vec<Value> = lib.systems.iter().map(|s| json!({ "id": s.name, "name": s.fullname, "games": s.games.len(), "stripe": stripe(&s.name) })).collect();
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for (path, m) in p.meta.games() {
        if lib.by_path.contains_key(Path::new(&path)) {
            for id in m.stickers {
                *counts.entry(id).or_default() += 1;
            }
        }
    }
    let stickers: Vec<Value> = p.meta.stickers().iter().map(|s| json!({ "id": s.id, "name": s.name, "pinned": s.pinned, "games": counts.get(&s.id).copied().unwrap_or(0) })).collect();
    let prefs = crate::curator::prefs::Prefs::load(&ctx.shared.data_dir);
    let have_index = ctx.shared.curator.lock().unwrap().is_some();
    let shelves: Vec<Value> = prefs.shelves.iter().enumerate().map(|(i, s)| json!({ "index": i, "name": s.name, "pinned": s.pinned })).collect();
    let favorites = lib.by_path.keys().filter(|p2| p.favorites.contains(*p2)).count();
    Response::json(
        200,
        &json!({
            "systems": systems,
            "favorites": favorites,
            "stickers": stickers,
            "shelves": shelves,
            "shelvesReady": have_index,
            "stickerMax": crate::library_meta::STICKER_MAX,
            "noteMax": crate::library_meta::NOTE_MAX,
            "shelfMax": crate::curator::prefs::SHELF_NAME_MAX,
        }),
    )
}

fn games(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let lib = ctx.library();
    let p = player(&ctx.shared.data_dir);
    let offset: usize = req.query("offset").and_then(|o| o.parse().ok()).unwrap_or(0);
    let (title, list): (String, Vec<(usize, usize)>) = if let Some(id) = req.query("system") {
        let s = lib.systems.iter().position(|s| s.name == id).ok_or_else(|| Response::error(404, "No such system."))?;
        (lib.systems[s].fullname.clone(), (0..lib.systems[s].games.len()).map(|g| (s, g)).collect())
    } else if req.query("favorites").is_some() {
        let mut l: Vec<(usize, usize)> = p.favorites.iter().filter_map(|f| lib.by_path.get(f).copied()).collect();
        l.sort_by_key(|&(s, g)| lib.systems[s].games[g].name.to_lowercase());
        ("Favourites".into(), l)
    } else if let Some(id) = req.query("sticker").and_then(|s| s.parse::<u32>().ok()) {
        let name = p.meta.stickers().iter().find(|s| s.id == id).map(|s| s.name.clone()).ok_or_else(|| Response::error(404, "No such sticker."))?;
        let mut l: Vec<(usize, usize)> = p.meta.games().iter().filter(|(_, m)| m.stickers.contains(&id)).filter_map(|(path, _)| lib.by_path.get(Path::new(path)).copied()).collect();
        l.sort_by_key(|&(s, g)| lib.systems[s].games[g].name.to_lowercase());
        (name, l)
    } else if let Some(i) = req.query("shelf").and_then(|s| s.parse::<usize>().ok()) {
        let prefs = crate::curator::prefs::Prefs::load(&ctx.shared.data_dir);
        let shelf = prefs.shelves.get(i).ok_or_else(|| Response::error(404, "No such shelf."))?;
        let ix = ctx.shared.curator.lock().unwrap().clone().ok_or_else(|| Response::error(503, "Curator is still reading the library. Try again in a moment."))?;
        let bits = ix.matches(&shelf.lens);
        // In the shelf's own order, as on the Pocket.
        let l = ix.sorted(&bits, prefs.merge_versions, shelf.order).into_iter().filter_map(|g| lib.by_path.get(&ix.game(g).path).copied()).collect();
        (shelf.name.clone(), l)
    } else if let Some(q) = req.query("q") {
        let q: String = q.chars().take(80).collect();
        let hits = lib.search.search(&q, &|_| true);
        (format!("“{q}”"), hits.into_iter().take(200).map(|h| (h.system, h.game)).collect())
    } else {
        return Err(Response::error(400, "Which list?"));
    };
    let total = list.len();
    let rows: Vec<Value> = list.into_iter().skip(offset).take(PAGE).map(|(s, g)| row(ctx, &lib, &p, s, g)).collect();
    Ok(Response::json(200, &json!({ "title": title, "total": total, "offset": offset, "games": rows })))
}

fn game(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let lib = ctx.library();
    let (s, g) = req.query("id").and_then(|id| lib.game(id)).ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?;
    let p = player(&ctx.shared.data_dir);
    let sys = &lib.systems[s];
    let gm = &sys.games[g];
    let mut v = row(ctx, &lib, &p, s, g);
    let meta = p.meta.get(&gm.path);
    v["note"] = json!(meta.note);
    v["file"] = json!(gm.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let stem = gm.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    // The name the game's checksum gave (`rom_id`), as the card matches it.
    let identified = crate::rom_id::known(&ctx.shared.data_dir, &[(sys.name.clone(), gm.path.clone())]).remove(&gm.path);
    let info = GAMEDB.with(|db| {
        let mut db = db.borrow_mut();
        db.get_or_insert_with(crate::gamedb::GameDb::embedded).resolve_rom(&sys.name, &stem, identified.as_deref(), meta.choice.as_ref())
    });
    if let Some(i) = info {
        v["facts"] = json!({
            "title": i.label,
            "year": i.year,
            "developers": i.developers,
            "publishers": i.publishers,
            "genres": i.genres,
            "series": i.series,
            "players": i.players,
            "modes": i.modes,
        });
    }
    // The game's screenshots (HOME + A in a game), newest first: only the time of each, never a path (`/api/shot`).
    // The collection's pictures of it, when there are some and they are wanted: which kinds (`/api/cover?kind=`).
    if crate::settings::game_pictures(&ctx.shared.data_dir) {
        let kinds: Vec<&str> = [(crate::game_pictures::Kind::Snap, "snap"), (crate::game_pictures::Kind::Title, "title")]
            .into_iter()
            .filter(|(k, _)| crate::game_pictures::path(&ctx.shared.data_dir, *k, &sys.name, &gm.path).is_file())
            .map(|(_, name)| name)
            .collect();
        v["pictures"] = json!(kinds);
    }
    let shots = crate::screenshot::list(&ctx.roots.screenshots, &sys.name, &stem);
    v["shots"] = json!(shots.into_iter().take(SHOTS_MAX).map(|e| e.at).collect::<Vec<_>>());
    let ix = ctx.shared.curator.lock().unwrap().clone();
    if let Some(ix) = ix {
        if let Some(at) = ix.find(&gm.path) {
            let versions = ix.versions(at);
            if versions.len() > 1 {
                let preferred = ix.preferred(at);
                let list: Vec<Value> = versions
                    .iter()
                    .map(|&j| {
                        let f = ix.game(j as usize);
                        json!({ "id": game_id(&f.path), "name": f.name, "system": f.system, "preferred": j as usize == preferred })
                    })
                    .collect();
                v["versions"] = json!(list);
                v["work"] = json!(ix.game(at).work);
            }
        }
    }
    Ok(Response::json(200, &v))
}

/// The pictures besides the box art that a query names (`kind=snap` or `title`); None for the box art itself.
fn picture_kind(q: Option<&str>) -> Result<Option<crate::game_pictures::Kind>, Response> {
    use crate::game_pictures::Kind;
    match q {
        None | Some("") | Some("box") => Ok(None),
        Some("snap") => Ok(Some(Kind::Snap)),
        Some("title") => Ok(Some(Kind::Title)),
        Some(_) => Err(Response::error(404, "No cover.")),
    }
}

/// `/api/cover?id=`: the box art; `&kind=snap` or `title`: the game running or its title screen (Settings > Games >
/// Extras > Game pictures, `game_pictures`), read the same way and 404 when the player chose box art only.
fn cover(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let lib = ctx.library();
    let (s, g) = req.query("id").and_then(|id| lib.game(id)).ok_or_else(|| Response::error(404, "No cover."))?;
    let (system, rom) = (&lib.systems[s].name, &lib.systems[s].games[g].path);
    let path = match picture_kind(req.query("kind"))? {
        None => crate::covers::cover_path(&ctx.shared.data_dir, system, rom),
        Some(_) if !crate::settings::game_pictures(&ctx.shared.data_dir) => return Err(Response::error(404, "No cover.")),
        Some(kind) => crate::game_pictures::path(&ctx.shared.data_dir, kind, system, rom),
    };
    // The size first: a huge file in the covers folder is never read into memory (review F8).
    let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= COVER_MAX);
    let mut bytes = Vec::new();
    if !small || std::fs::File::open(&path).and_then(|f| f.take(COVER_MAX).read_to_end(&mut bytes)).is_err() {
        return Err(Response::error(404, "No cover."));
    }
    if !bytes.starts_with(b"\x89PNG") {
        return Err(Response::error(404, "No cover."));
    }
    Ok(Response::new(200, "image/png", bytes).with("Cache-Control", "private, max-age=86400"))
}

/// A screenshot of a game (`id`, and `at`: the time part of its name, `screenshot::Entry::at`), as the PNG it is. Read
/// only; `at` is checked to be exactly such a time, so nothing else on the card can be asked for.
fn shot(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let none = || Response::error(404, "No screenshot.");
    let lib = ctx.library();
    let (s, g) = req.query("id").and_then(|id| lib.game(id)).ok_or_else(none)?;
    let at = req.query("at").ok_or_else(none)?;
    let game = &lib.systems[s].games[g];
    let stem = game.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let path = crate::screenshot::find(&ctx.roots.screenshots, &lib.systems[s].name, stem, at).ok_or_else(none)?;
    // The size first, like the covers: a huge file in that folder is never read into memory.
    let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= SHOT_MAX);
    let mut bytes = Vec::new();
    if !small || std::fs::File::open(&path).and_then(|f| f.take(SHOT_MAX).read_to_end(&mut bytes)).is_err() || !bytes.starts_with(b"\x89PNG") {
        return Err(none());
    }
    // The name says when it was taken and never changes: the browser may keep it.
    Ok(Response::new(200, "image/png", bytes).with("Cache-Control", "private, max-age=86400"))
}

fn stats(ctx: &Ctx) -> Response {
    let lib = ctx.library();
    let p = player(&ctx.shared.data_dir);
    let mut per_system: Vec<(u64, u32, usize)> = vec![(0, 0, 0); lib.systems.len()];
    let mut games: Vec<(u64, usize, usize)> = Vec::new();
    for (s, sys) in lib.systems.iter().enumerate() {
        for (g, game) in sys.games.iter().enumerate() {
            let m = p.meta.get(&game.path);
            let secs = m.secs.max(p.recents.get(&game.path).map_or(0, |r| r.1));
            if secs > 0 {
                per_system[s].0 += secs;
                per_system[s].1 += m.sessions;
                per_system[s].2 += 1;
                games.push((secs, s, g));
            }
        }
    }
    games.sort_by_key(|g| std::cmp::Reverse(g.0));
    let top: Vec<Value> = games.iter().take(30).map(|&(_, s, g)| row(ctx, &lib, &p, s, g)).collect();
    let mut systems: Vec<Value> = lib
        .systems
        .iter()
        .zip(&per_system)
        .filter(|(_, t)| t.0 > 0)
        .map(|(s, t)| json!({ "id": s.name, "name": s.fullname, "stripe": stripe(&s.name), "secs": t.0, "sessions": t.1, "played": t.2, "games": s.games.len() }))
        .collect();
    systems.sort_by_key(|v| std::cmp::Reverse(v["secs"].as_u64().unwrap_or(0)));
    let total: u64 = per_system.iter().map(|t| t.0).sum();
    let sessions: u32 = per_system.iter().map(|t| t.1).sum();
    let mut history = ctx.history.or_else(super::history_source).and_then(|f| f(&ctx.shared.data_dir));
    if let Some(h) = history.as_mut() {
        let private = [ctx.shared.data_dir.to_string_lossy().into_owned(), ctx.roots.roms.to_string_lossy().into_owned()];
        without_paths(h, &lib, &private);
    }
    Response::json(200, &json!({ "totalSecs": total, "sessions": sessions, "played": games.len(), "systems": systems, "games": top, "history": history }))
}

/// The play history as the browser gets it: no path leaves the Pocket. Every `path` field goes; a game that is in the
/// library gets its id instead (for a link to its page). Beyond that, whatever looks like a path goes too (review
/// F15): an entry whose key has a slash in it, and any text that starts with a slash, mentions `/storage/` or `roms/`,
/// or contains one of `private` (the data folder, the ROMs folder) becomes null.
pub fn without_paths(v: &mut Value, lib: &LibCache, private: &[String]) {
    match v {
        Value::Object(map) => {
            if let Some(p) = map.remove("path") {
                if let Some((path, &(s, g))) = p.as_str().map(Path::new).and_then(|p| lib.by_path.get(p).map(|at| (p, at))) {
                    map.insert("id".into(), json!(game_id(path)));
                    // The name the game list shows (two versions told apart), not the one the log was written with.
                    if let Some(game) = lib.systems.get(s).and_then(|s| s.games.get(g)).filter(|_| map.get("name").is_some_and(Value::is_string)) {
                        map.insert("name".into(), json!(game.name));
                    }
                }
            }
            map.retain(|k, _| !k.contains(['/', '\\']));
            for (_, x) in map.iter_mut() {
                without_paths(x, lib, private);
            }
        }
        Value::Array(list) => list.iter_mut().for_each(|x| without_paths(x, lib, private)),
        Value::String(s) if looks_like_path(s, private) => *v = Value::Null,
        _ => {}
    }
}

fn looks_like_path(s: &str, private: &[String]) -> bool {
    s.starts_with(['/', '\\']) || s.contains("/storage/") || s.contains("roms/") || s.contains("roms\\") || private.iter().any(|p| !p.is_empty() && s.contains(p.as_str()))
}

fn battery(ctx: &Ctx) -> Response {
    // The companion's own thread may read the file when the sampler hasn't yet (the UI never does).
    crate::battery_history::load_now(&ctx.shared.data_dir);
    let h = crate::battery_history::view();
    let now = crate::recents::now_secs();
    let samples: Vec<Value> = h.recent(now, crate::battery_history::KEEP).iter().map(|s| json!([s.t, s.capacity, s.charging as u8])).collect();
    let last = h.samples().last().map(|s| json!({ "t": s.t, "percent": s.capacity, "charging": s.charging, "full": s.full }));
    Response::json(200, &json!({ "now": now, "gap": crate::battery_history::GAP, "keep": crate::battery_history::KEEP, "last": last, "samples": samples }))
}

/// The screen, at most every [`SHOT_EVERY`] (the last one again in between), half size, as PNG.
fn screenshot(ctx: &Ctx) -> Response {
    // Settings and the first-start steps can show the pairing code, passwords and keys: never captured (review F4).
    let private = || ctx.shared.screen_private.load(Ordering::Relaxed);
    if private() {
        return hidden_screen();
    }
    let mut guard = ctx.shot.lock().unwrap();
    if let Some((at, png)) = guard.as_ref() {
        if at.elapsed() < SHOT_EVERY {
            let age = at.elapsed().as_secs().to_string();
            return Response::new(200, "image/png", png.to_vec()).with("X-Shot-Age", &age);
        }
    }
    let Some(img) = (ctx.screenshot)() else {
        return Response::error(503, "No screenshot: the screen could not be read.");
    };
    // Settings opened while the picture was taken: it is thrown away.
    if private() {
        return hidden_screen();
    }
    let Some(png) = shrink_png(&img) else {
        return Response::error(500, "No screenshot.");
    };
    let png = Arc::new(png);
    *guard = Some((Instant::now(), png.clone()));
    Response::new(200, "image/png", png.to_vec()).with("X-Shot-Age", "0")
}

/// What the screenshot shows while the Pocket shows something private: a plain grey picture, marked so the page says
/// why.
fn hidden_screen() -> Response {
    let img = crate::image::Image { w: 248, h: 216, px: vec![0x00_2a_2a_2e; 248 * 216] };
    let png = shrink_png(&img).unwrap_or_default();
    Response::new(200, "image/png", png).with("X-Screen-Hidden", "1").with("X-Shot-Age", "0")
}

/// The picture at half size (each output pixel the mean of two by two), as PNG.
pub fn shrink_png(img: &crate::image::Image) -> Option<Vec<u8>> {
    // A picture whose pixels don't match its size is refused, never indexed past its end (a panic ends pocket-ui).
    if img.w == 0 || img.h == 0 || img.px.len() != (img.w as usize) * (img.h as usize) {
        return None;
    }
    let (w, h) = ((img.w / 2).max(1), (img.h / 2).max(1));
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = ((x * 2 + dx).min(img.w - 1), (y * 2 + dy).min(img.h - 1));
                let p = img.px[(sy * img.w + sx) as usize];
                acc[0] += (p >> 16) & 0xff;
                acc[1] += (p >> 8) & 0xff;
                acc[2] += p & 0xff;
            }
            rgb.extend(acc.iter().map(|c| (c / 4) as u8));
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
    }
    Some(out)
}

// --- uploads ---------------------------------------------------------------------------------------------------

fn uploads(ctx: &Ctx) -> Response {
    let lib = ctx.library();
    let systems: Vec<Value> = lib
        .info
        .iter()
        .filter(|s| files::system_folder_ok(&ctx.roots, s))
        .map(|s| json!({ "id": s.id, "name": s.name, "folder": s.folder, "extensions": s.extensions }))
        .collect();
    let bios: Vec<Value> = files::bios_report(&ctx.roots.bios)
        .into_iter()
        .map(|n| json!({ "label": n.label, "files": n.files, "found": n.found, "missing": n.missing_line() }))
        .collect();
    let ports = files::ports_needing_files(&ctx.roots);
    let free = files::free_space(&ctx.roots.roms);
    Response::json(
        200,
        &json!({
            "systems": systems,
            "bios": bios,
            "ports": ports,
            "free": free,
            "reserve": files::RESERVE,
            "max": { "rom": files::MAX_ROM, "bios": files::MAX_BIOS, "port": files::MAX_PORT },
        }),
    )
}

/// Where an upload described by the query goes, and its limit.
fn target(ctx: &Ctx, req: &Request) -> Result<(PathBuf, u64), Refused> {
    let name = req.query("name").unwrap_or("");
    match req.query("kind") {
        Some("rom") => {
            let lib = ctx.library();
            let id = req.query("system").unwrap_or("");
            let sys = lib.info.iter().find(|s| s.id == id).ok_or_else(|| Refused::NotAllowed("Choose a system for this file.".into()))?;
            Ok((files::rom_target(&ctx.roots, sys, name)?, files::MAX_ROM))
        }
        Some("bios") => Ok((files::bios_target(&ctx.roots, name, req.query("sub"))?, files::MAX_BIOS)),
        Some("port") => {
            let rel = req.query("rel").filter(|r| !r.is_empty()).unwrap_or(name);
            Ok((files::port_target(&ctx.roots, req.query("dir").unwrap_or(""), rel)?, files::MAX_PORT))
        }
        _ => Err(Refused::NotAllowed("Unknown kind of upload.".into())),
    }
}

/// The path as the player knows it: "roms/gba/Game.gba".
fn shown(ctx: &Ctx, path: &Path) -> String {
    let base = ctx.roots.roms.parent().unwrap_or(Path::new("/"));
    // Targets are written through the resolved folders (`files::inside`): the ROMs folder resolved, too.
    let real = ctx.roots.roms.canonicalize().ok().and_then(|r| r.parent().map(Path::to_path_buf));
    let rel = path.strip_prefix(base).ok().or_else(|| real.as_deref().and_then(|r| path.strip_prefix(r).ok()));
    rel.unwrap_or(path).to_string_lossy().into_owned()
}

fn upload_check(ctx: &Ctx, req: &Request) -> Response {
    let size: u64 = req.query("size").and_then(|s| s.parse().ok()).unwrap_or(0);
    match target(ctx, req).and_then(|(t, max)| files::fits(&t, size, max).map(|()| t)) {
        Ok(t) => Response::json(
            200,
            &json!({
                "ok": true,
                "name": t.file_name().map(|n| n.to_string_lossy().into_owned()),
                "path": shown(ctx, &t),
                "exists": std::fs::symlink_metadata(&t).is_ok(),
            }),
        ),
        Err(e) => Response::json(e.status(), &json!({ "ok": false, "error": e.message() })),
    }
}

fn upload(ctx: &Ctx, req: &Request, body: &mut impl BufRead) -> Response {
    let Some(len) = req.content_length else {
        return Response::error(411, "The browser did not say how large the file is.");
    };
    let overwrite = req.query("overwrite") == Some("1");
    let (t, max) = match target(ctx, req) {
        Ok(t) => t,
        Err(e) => return Response::json(e.status(), &json!({ "ok": false, "error": e.message() })),
    };
    // Room for it, held until it ends: uploads at the same time can't all take the same free space (review F9).
    let _room = match files::reserve(&t, len, max) {
        Ok(r) => r,
        Err(e) => return Response::json(e.status(), &json!({ "ok": false, "error": e.message() })),
    };
    let (wait, abort) = (|| ctx.shared.playing(), || ctx.stopped.load(Ordering::Relaxed));
    let gate = files::Gate { wait: &wait, abort: &abort, no_programs: req.query("kind") == Some("port") };
    match files::receive(body, &t, len, overwrite, &gate) {
        Ok(w) => {
            let kind = req.query("kind").unwrap_or("");
            let mut v = json!({ "ok": true, "path": shown(ctx, &t), "bytes": w.bytes, "replaced": w.replaced });
            if kind == "rom" {
                // Read here, on this low-priority thread: the App only swaps the list in (`app_web.rs`).
                ctx.stale.store(true, Ordering::Relaxed);
                let lib = ctx.library();
                let _ = ctx.shared.edit(Edit::LibraryChanged(super::Systems(Arc::new(lib.systems.clone()))));
            }
            if kind == "bios" {
                let rel = t.strip_prefix(&ctx.roots.bios).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                v["bios"] = json!(files::bios_for(&rel));
            }
            println!("web: uploaded {} ({} bytes)", shown(ctx, &t), w.bytes);
            Response::json(200, &v)
        }
        Err(e) => Response::json(e.status(), &json!({ "ok": false, "error": e.message() })),
    }
}

// --- edits -----------------------------------------------------------------------------------------------------

fn text_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(|t| t.as_str()).unwrap_or_default().to_string()
}

fn edit(ctx: &Ctx, path: &str, req: &Request, body: &mut impl Read) -> Result<Response, Response> {
    let v = json_body(req, body)?;
    let lib = ctx.library();
    let game = || -> Result<(String, String, PathBuf), Response> {
        let (s, g) = v.get("id").and_then(|i| i.as_str()).and_then(|id| lib.game(id)).ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?;
        let (sys, gm) = (&lib.systems[s], &lib.systems[s].games[g]);
        Ok((sys.name.clone(), gm.name.clone(), gm.path.clone()))
    };
    // Numbers that don't fit are refused, never cut down to another sticker's or shelf's (review F18).
    let sticker = || v.get("sticker").and_then(|n| n.as_u64()).and_then(|n| u32::try_from(n).ok()).ok_or_else(|| Response::error(400, "Which sticker?"));
    let shelf = || v.get("index").and_then(|n| n.as_u64()).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| Response::error(400, "Which shelf?"));
    let e = match path {
        "/api/game/favorite" => {
            let (system, name, path) = game()?;
            Edit::Favorite { system, name, path, on: v.get("on").and_then(|b| b.as_bool()).unwrap_or(true) }
        }
        "/api/game/status" => {
            let status = parse_status(&text_of(&v, "status")).ok_or_else(|| Response::error(400, "Unknown status."))?;
            Edit::Status { path: game()?.2, status }
        }
        "/api/game/note" => Edit::Note { path: game()?.2, note: text_of(&v, "note") },
        "/api/game/rating" => {
            // 0 takes the rating away; anything but 0 to 5 is refused, never cut down to another rating.
            let rating = v.get("rating").and_then(|n| n.as_u64()).filter(|n| *n <= crate::library_meta::RATING_MAX as u64).ok_or_else(|| Response::error(400, "A rating is 0 to 5."))?;
            Edit::Rating { path: game()?.2, rating: rating as u8 }
        }
        "/api/game/sticker" => {
            let id = sticker()?;
            Edit::Sticker { path: game()?.2, id, on: v.get("on").and_then(|b| b.as_bool()).unwrap_or(true) }
        }
        "/api/game/preferred" => {
            let path = game()?.2;
            let ix = ctx.shared.curator.lock().unwrap().clone().ok_or_else(|| Response::error(503, "Curator is still reading the library."))?;
            let work = ix.find(&path).and_then(|g| ix.game(g).work.clone()).ok_or_else(|| Response::error(409, "This game has no other versions."))?;
            Edit::Preferred { work, path }
        }
        "/api/stickers/new" => {
            let path = if v.get("id").is_some() { Some(game()?.2) } else { None };
            Edit::NewSticker { name: text_of(&v, "name"), path }
        }
        "/api/stickers/rename" => Edit::RenameSticker { id: sticker()?, name: text_of(&v, "name") },
        "/api/stickers/delete" => Edit::DeleteSticker { id: sticker()? },
        "/api/shelves/rename" => Edit::RenameShelf { index: shelf()?, was: text_of(&v, "was"), name: text_of(&v, "name") },
        "/api/shelves/delete" => Edit::DeleteShelf { index: shelf()?, was: text_of(&v, "was") },
        _ => return Err(Response::error(404, "Not found.")),
    };
    match ctx.shared.edit(e) {
        Ok(()) => Ok(Response::json(200, &json!({ "ok": true }))),
        Err(msg) => Err(Response::error(409, &msg)),
    }
}
