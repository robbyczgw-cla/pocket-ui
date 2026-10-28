//! Tailscale on ROCKNIX, driven from Settings > Tailscale (docs/user/remote.md, docs/SECURITY-HARDENING.md).
//!
//! ROCKNIX ships `tailscale` and `tailscaled` in /usr/sbin and a `tailscaled.service` whose state lives under
//! /storage/.cache/tailscale. Its boot script (`autostart/daemons/004-tailscaled`) starts the service when the
//! setting `tailscale.up` is 1 and stops it otherwise; EmulationStation's switch does the same with
//! `tailscale up --timeout=7s`. This module does what that switch does, with a login that works on a handheld
//! (the sign-in address as a QR code) and with every call on a worker thread under a timeout: a hanging
//! `tailscale` or `systemctl` costs a bounded wait on a thread nobody waits for, never the UI (`spawn`).
//!
//! Tailscale only provides the network path. SSH stays what `remote.rs` makes of it: Tailscale's own SSH
//! server (`--ssh`) would answer on port 22 of the tailnet address instead of sshd, so every call says `--ssh=false`.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// --- what `tailscale status --json` says ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Unknown,
    NoState,
    NeedsLogin,
    NeedsMachineAuth,
    Stopped,
    Starting,
    Running,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub backend: Backend,
    /// The sign-in address while the node waits for a login.
    pub auth_url: Option<String>,
    /// This device's Tailscale addresses, IPv4 first.
    pub ips: Vec<String>,
    /// The name this device has in the tailnet (`Self.HostName`).
    pub hostname: String,
    /// The full MagicDNS name without the trailing dot.
    pub dns_name: String,
    /// `CurrentTailnet.Name`: for a personal tailnet the account's address.
    pub tailnet: String,
    /// The account that is logged in on this device.
    pub login: String,
    pub peers_online: usize,
    pub peers_total: usize,
    pub health: Vec<String>,
}

fn text(v: &serde_json::Value, path: &[&str]) -> String {
    let mut cur = v;
    for key in path {
        match cur.get(key) {
            Some(next) => cur = next,
            None => return String::new(),
        }
    }
    cur.as_str().unwrap_or_default().to_string()
}

fn strings(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

impl Status {
    /// Reads the output of `tailscale status --json`. Fields that are missing or null are empty; only text that is
    /// not a JSON object is an error.
    pub fn parse(json: &str) -> Result<Status, String> {
        let v: serde_json::Value = serde_json::from_str(json.trim()).map_err(|e| format!("not JSON: {e}"))?;
        if !v.is_object() {
            return Err("not a JSON object".into());
        }
        let backend = match text(&v, &["BackendState"]).as_str() {
            "NoState" => Backend::NoState,
            "NeedsLogin" => Backend::NeedsLogin,
            "NeedsMachineAuth" => Backend::NeedsMachineAuth,
            "Stopped" => Backend::Stopped,
            "Starting" => Backend::Starting,
            "Running" => Backend::Running,
            _ => Backend::Unknown,
        };
        let mut ips = strings(v.get("TailscaleIPs"));
        if ips.is_empty() {
            ips = strings(v.get("Self").and_then(|s| s.get("TailscaleIPs")));
        }
        ips.sort_by_key(|ip| ip.contains(':'));
        let user_id = v.get("Self").and_then(|s| s.get("UserID")).map(|u| u.to_string()).unwrap_or_default();
        let peers = v.get("Peer").and_then(|p| p.as_object());
        Ok(Status {
            backend,
            auth_url: Some(text(&v, &["AuthURL"])).filter(|u| !u.is_empty()),
            ips,
            hostname: text(&v, &["Self", "HostName"]),
            dns_name: text(&v, &["Self", "DNSName"]).trim_end_matches('.').to_string(),
            tailnet: text(&v, &["CurrentTailnet", "Name"]),
            login: text(&v, &["User", &user_id, "LoginName"]),
            peers_online: peers.map_or(0, |p| p.values().filter(|n| n.get("Online").and_then(|o| o.as_bool()) == Some(true)).count()),
            peers_total: peers.map_or(0, |p| p.len()),
            health: strings(v.get("Health")),
        })
    }

    pub fn ipv4(&self) -> Option<&str> {
        self.ips.iter().map(String::as_str).find(|ip| !ip.contains(':'))
    }
}

/// The addresses `tailscale ip` prints, one per line.
pub fn parse_ips(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| l.parse::<std::net::IpAddr>().is_ok())
        .map(str::to_string)
        .collect()
}

/// The sign-in address in the output of `tailscale up` (it goes to stderr, after "To authenticate, visit:").
/// Any address will do, a self-hosted control server's too; links to the documentation (`/kb/`) are not it.
pub fn login_url(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .filter(|t| t.starts_with("https://") || t.starts_with("http://"))
        .map(|t| t.trim_end_matches(['.', ',', ';', ')', '>', '"', '\'']))
        .find(|u| !u.contains("/kb/") && u.split_once("://").is_some_and(|(_, rest)| rest.contains('.') && rest.len() > 4))
        .map(str::to_string)
}

// --- names ----------------------------------------------------------------------------------------------------

/// The name suggested for the Pocket in the tailnet.
pub const SUGGESTED_HOSTNAME: &str = "pocket-mini";

/// A name Tailscale accepts as a host name: a DNS label.
pub fn valid_hostname(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// What was typed, made into a host name: lower case, spaces and underscores become hyphens, the rest goes.
pub fn clean_hostname(typed: &str) -> String {
    let mut name: String = typed
        .chars()
        .map(|c| if c == ' ' || c == '_' { '-' } else { c.to_ascii_lowercase() })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    name.truncate(63);
    name.trim_matches('-').to_string()
}

// --- what the page shows ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// `/usr/sbin/tailscale` is there.
    pub installed: bool,
    /// `tailscaled` runs.
    pub running: bool,
    /// ROCKNIX starts it at boot (`tailscale.up=1` in system.cfg).
    pub boot: bool,
    pub status: Option<Status>,
    /// Why `status` is missing while the service runs.
    pub error: Option<String>,
    /// No network under Tailscale: Wi-Fi off or not joined, no cable (`network_up`). Tailscale's own answer cannot say
    /// this: `Running` and an address stay for a while after the network is gone, and `Starting` is all it says
    /// before it is back.
    pub offline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    NotInstalled,
    Off,
    Starting,
    LoggedOut,
    WaitingForApproval,
    Paused,
    Connecting,
    Connected,
    /// The service runs but there is no network to reach the tailnet over: not an error, it connects by itself.
    NoNetwork,
}

/// What the page and the Settings row say while there is no network (the switch stays as the user left it).
pub const NO_NETWORK: &str = "No network yet";

impl Snapshot {
    /// What the page shows: what Tailscale says, unless there is no network to say it over.
    pub fn state(&self) -> State {
        match self.backend_state() {
            State::Off | State::NotInstalled => self.backend_state(),
            _ if self.offline => State::NoNetwork,
            s => s,
        }
    }

    /// What Tailscale itself reports (service, sign-in, tailnet), whatever the network under it does.
    pub fn backend_state(&self) -> State {
        if !self.installed {
            return State::NotInstalled;
        }
        if !self.running {
            return State::Off;
        }
        match self.status.as_ref().map(|s| s.backend) {
            Some(Backend::NeedsLogin) => State::LoggedOut,
            Some(Backend::NeedsMachineAuth) => State::WaitingForApproval,
            Some(Backend::Stopped) => State::Paused,
            Some(Backend::Starting) => State::Connecting,
            Some(Backend::Running) => State::Connected,
            Some(Backend::NoState | Backend::Unknown) | None => State::Starting,
        }
    }

    /// "Off", "Logged out", "Connecting", "Connected as alex@example.com".
    pub fn status_line(&self) -> String {
        match self.state() {
            State::NotInstalled => "Not installed".into(),
            State::Off => "Off".into(),
            State::Starting => "Starting".into(),
            State::LoggedOut => "Logged out".into(),
            State::WaitingForApproval => "Waiting for approval".into(),
            State::Paused => "Not connected".into(),
            State::Connecting => "Connecting".into(),
            State::NoNetwork => NO_NETWORK.into(),
            State::Connected => {
                let s = self.status.as_ref();
                match s
                    .map(|s| if s.tailnet.is_empty() { s.login.as_str() } else { s.tailnet.as_str() })
                    .filter(|n| !n.is_empty())
                {
                    Some(name) => format!("Connected as {name}"),
                    None => "Connected".into(),
                }
            }
        }
    }
}

// --- the setting ROCKNIX reads at boot -------------------------------------------------------------------------

/// The value of `key` in the text of system.cfg (`key=value` lines; the last one wins, as in ROCKNIX's `get_setting`).
pub fn setting_in(cfg: &str, key: &str) -> Option<String> {
    cfg.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().to_string())
        .last()
}

/// ROCKNIX starts Tailscale at boot.
pub fn boot_enabled(cfg: &str) -> bool {
    setting_in(cfg, "tailscale.up").as_deref() == Some("1")
}

// --- what pocket-ui keeps ---------------------------------------------------------------------------------------

/// The only choice of ours: the name given to this device. Whether Tailscale is on lives in ROCKNIX's setting.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub hostname: Option<String>,
}

const PREFS_FILE: &str = "tailscale.json";

impl Prefs {
    pub fn load(data_dir: &Path) -> Prefs {
        let mut prefs: Prefs = crate::fsutil::read_json_or_backup(&data_dir.join(PREFS_FILE)).unwrap_or_default();
        // A hand-edited file cannot make `tailscale up --hostname=` fail.
        prefs.hostname = prefs.hostname.filter(|h| valid_hostname(h));
        prefs
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&data_dir.join(PREFS_FILE), self) {
            eprintln!("tailscale: can't save {PREFS_FILE}: {e}");
        }
    }
}

// --- the network under it ---------------------------------------------------------------------------------------

/// Interfaces that are not a way out: loopback, Tailscale's own (its address stays after the network is gone),
/// containers and bridges.
fn is_virtual(name: &str) -> bool {
    name == "lo" || ["tailscale", "docker", "veth", "br-", "virbr", "dummy", "sit", "ip6tnl"].iter().any(|p| name.starts_with(p))
}

/// Some real network is up: Wi-Fi joined to an access point, or a cable (USB Ethernet) with a link. Read from the
/// `operstate` of each interface in `net_dir` (`/sys/class/net`), as the status corner reads Wi-Fi: a few tiny
/// files, no process. It does not look for a sign-in page of the network (`portal.rs` does) or for a route.
pub fn network_up(net_dir: &Path) -> bool {
    let Ok(dir) = std::fs::read_dir(net_dir) else { return false };
    dir.flatten().any(|e| {
        !is_virtual(&e.file_name().to_string_lossy())
            && std::fs::read_to_string(e.path().join("operstate")).is_ok_and(|s| s.trim() == "up")
    })
}

// --- running it ---------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Timeouts {
    /// `tailscale status`, `systemctl is-active`.
    pub status: Duration,
    /// Starting and stopping the service, logout, set.
    pub action: Duration,
    /// How long a login may wait for the user to approve it.
    pub login: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            status: Duration::from_secs(4),
            action: Duration::from_secs(30),
            login: Duration::from_secs(10 * 60),
        }
    }
}

/// The programs and files this module uses; tests point them at stand-ins.
#[derive(Clone, Debug)]
pub struct Tools {
    pub tailscale: PathBuf,
    pub systemctl: String,
    /// ROCKNIX's settings; written under its lock (`system_cfg`).
    pub system_cfg: PathBuf,
    /// The key file of a login with an auth key is made here (a RAM disk on the device).
    pub run_dir: PathBuf,
    /// ROCKNIX's `tailscaled.service` reads this file; it is copied from `defaults_src` when it is missing.
    pub defaults: PathBuf,
    pub defaults_src: PathBuf,
    /// Where the network interfaces are (`network_up`).
    pub net_dir: PathBuf,
    pub timeouts: Timeouts,
}

impl Default for Tools {
    fn default() -> Self {
        Tools {
            tailscale: PathBuf::from("/usr/sbin/tailscale"),
            systemctl: "systemctl".into(),
            system_cfg: PathBuf::from("/storage/.config/system/configs/system.cfg"),
            run_dir: PathBuf::from("/run"),
            defaults: PathBuf::from("/storage/.config/tailscaled.defaults"),
            defaults_src: PathBuf::from("/usr/config/tailscaled.defaults"),
            net_dir: PathBuf::from("/sys/class/net"),
            timeouts: Timeouts::default(),
        }
    }
}

const SERVICE: &str = "tailscaled";
const KEY_FILE: &str = "pocket-ui-tailscale-key";
/// ROCKNIX's own `tailscaled.defaults`, for the day neither copy exists.
const DEFAULTS: &str = "PORT=\"41641\"\nFLAGS=\"\"\n";

/// The arguments of `tailscale up` for a login or a reconnect. Always `--reset` (so earlier flags cannot make it
/// refuse), always `--ssh=false` (Tailscale SSH must not take over port 22), the key by file, never by value.
pub fn up_args(hostname: Option<&str>, key_file: Option<&Path>, timeout: Option<Duration>) -> Vec<String> {
    let mut args: Vec<String> = ["up", "--reset", "--ssh=false"].map(String::from).to_vec();
    if let Some(name) = hostname {
        args.push(format!("--hostname={name}"));
    }
    if let Some(file) = key_file {
        args.push(format!("--auth-key=file:{}", file.display()));
    }
    if let Some(t) = timeout {
        args.push(format!("--timeout={}s", t.as_secs().max(1)));
    }
    args
}

/// Runs a program to its end under `timeout`; its output on success, the last line it complained with otherwise.
fn run(cmd: &mut Command, timeout: Duration, what: &str) -> Result<String, String> {
    match crate::proc::run_with_timeout(cmd, timeout) {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let outp = String::from_utf8_lossy(&out.stdout);
            let last = err.lines().chain(outp.lines()).map(str::trim).filter(|l| !l.is_empty()).last();
            Err(last.map_or_else(|| format!("{what} failed"), str::to_string))
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(format!("{what} did not answer")),
        Err(e) => Err(format!("{what}: {e}")),
    }
}

/// The CLI's words for "the daemon is not there" ("failed to connect to local tailscaled; it doesn't appear to be
/// running (sudo systemctl start tailscaled ?)") are advice for a shell. The page says what the timeout case says;
/// the original goes to the log.
fn plain_error(raw: &str) -> String {
    if raw.contains("failed to connect to local tailscaled") || raw.contains("doesn't appear to be running") {
        eprintln!("tailscale: {raw}");
        return "tailscale did not answer".into();
    }
    raw.to_string()
}

/// What a worker reports when `result` failed: if the network is gone by now, the failure is that and nothing else
/// (the page shows it calmly, `NO_NETWORK`); any other failure as it is. For work that needs Tailscale's servers.
pub fn failed_offline(tools: &Tools, result: Result<(), String>) -> Result<(), String> {
    match result {
        Err(e) if !network_up(&tools.net_dir) => {
            eprintln!("tailscale: failed without a network: {e}");
            Err(NO_NETWORK.into())
        }
        other => other,
    }
}

fn tailscale(tools: &Tools, args: &[String], timeout: Duration) -> Result<String, String> {
    run(Command::new(&tools.tailscale).args(args), timeout, "tailscale")
}

/// ROCKNIX's `set_setting`, done by `system_cfg` under ROCKNIX's lock, in the same file format.
fn set_setting(tools: &Tools, key: &str, value: &str) -> Result<(), String> {
    crate::system_cfg::SystemCfg::at(&tools.system_cfg).set(key, value).map_err(|e| format!("system.cfg: {e}"))
}

fn service(tools: &Tools, action: &str) -> Result<(), String> {
    run(Command::new(&tools.systemctl).args([action, SERVICE]), tools.timeouts.action, "systemctl").map(|_| ())
}

fn service_active(tools: &Tools) -> bool {
    crate::proc::status_with_timeout(Command::new(&tools.systemctl).args(["is-active", "--quiet", SERVICE]), tools.timeouts.status).is_ok_and(|s| s.success())
}

fn status(tools: &Tools) -> Result<Status, String> {
    let out = crate::proc::run_with_timeout(Command::new(&tools.tailscale).args(["status", "--json"]), tools.timeouts.status);
    match out {
        // `status` exits non-zero in some states and still prints the JSON.
        Ok(out) => Status::parse(&String::from_utf8_lossy(&out.stdout)).map_err(|_| {
            let err = String::from_utf8_lossy(&out.stderr);
            let raw = err.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("tailscale did not answer");
            plain_error(raw)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err("tailscale did not answer".into()),
        Err(e) => Err(format!("tailscale: {e}")),
    }
}

/// What `snapshot` reads: service state, the boot setting, `tailscale status --json`. Blocks up to two status timeouts.
pub fn snapshot(tools: &Tools) -> Snapshot {
    let boot = std::fs::read_to_string(&tools.system_cfg).is_ok_and(|cfg| boot_enabled(&cfg));
    let installed = tools.tailscale.exists();
    let offline = !network_up(&tools.net_dir);
    if !installed || !service_active(tools) {
        return Snapshot {
            installed,
            running: false,
            boot,
            status: None,
            error: None,
            offline,
        };
    }
    let (status, error) = match status(tools) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };
    Snapshot {
        installed,
        running: true,
        boot,
        status,
        error,
        offline,
    }
}

fn ensure_defaults(tools: &Tools) -> Result<(), String> {
    if tools.defaults.exists() {
        return Ok(());
    }
    if let Some(dir) = tools.defaults.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if std::fs::copy(&tools.defaults_src, &tools.defaults).is_err() {
        std::fs::write(&tools.defaults, DEFAULTS).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Sets `tailscale.up=1` (ROCKNIX starts it at every boot), starts the service, and reconnects when it was paused.
pub fn enable(tools: &Tools, hostname: Option<&str>) -> Result<(), String> {
    ensure_defaults(tools)?;
    set_setting(tools, "tailscale.up", "1")?;
    service(tools, "start")?;
    // The service answers a moment after `start`; a login that was paused with `tailscale down` is picked up again.
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        match status(tools) {
            Ok(s) if s.backend == Backend::Stopped => {
                let args = up_args(hostname, None, Some(Duration::from_secs(8)));
                // Without a network `up` runs into its own timeout, but it has said "on": Tailscale connects when the
                // network is back. That is no error to show.
                return match tailscale(tools, &args, tools.timeouts.action) {
                    Err(e) if !network_up(&tools.net_dir) => {
                        eprintln!("tailscale: up without a network, it connects later: {e}");
                        Ok(())
                    }
                    other => other.map(|_| ()),
                };
            }
            Ok(s) if !matches!(s.backend, Backend::Starting | Backend::NoState | Backend::Unknown) => {
                // Tailscale SSH would answer on port 22 instead of sshd: whoever set it by hand, it goes off here.
                let _ = tailscale(tools, &["set".to_string(), "--ssh=false".to_string()], tools.timeouts.status);
                return Ok(());
            }
            _ if Instant::now() >= deadline => return Ok(()),
            _ => std::thread::sleep(Duration::from_millis(300)),
        }
    }
}

/// Sets `tailscale.up=0` and stops the service. The login stays: switching on again reconnects.
pub fn disable(tools: &Tools) -> Result<(), String> {
    set_setting(tools, "tailscale.up", "0")?;
    service(tools, "stop")
}

pub struct Login<'a> {
    pub hostname: Option<&'a str>,
    /// An auth key typed by the user (`tskey-auth-...`): no browser needed.
    pub auth_key: Option<&'a str>,
}

/// Removes the key file when the login is over, however it ends.
struct KeyFile(Option<PathBuf>);

impl Drop for KeyFile {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn kill_group(child: &mut Child) {
    // SAFETY: kill(2) with a negative pid signals only the process group `process_group(0)` made for this child.
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let _ = child.kill();
    let _ = child.wait();
}

fn read_lines(pipe: Option<impl std::io::Read + Send + 'static>, tx: mpsc::Sender<String>) {
    let Some(pipe) = pipe else { return };
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
            buf.clear();
        }
    });
}

/// What has been read of `tailscale up` so far.
#[derive(Default)]
struct Seen {
    url: Option<String>,
    /// The last line that says something (the reason, when it fails).
    last_line: String,
}

impl Seen {
    fn feed(&mut self, line: &str, on_url: &mut dyn FnMut(String)) {
        if let Some(url) = login_url(line) {
            if self.url.as_ref() != Some(&url) {
                self.url = Some(url.clone());
                on_url(url);
            }
        } else if !line.trim().is_empty() && !line.trim_start().starts_with("To authenticate") {
            self.last_line = line.trim().to_string();
        }
    }
}

/// `tailscale up`, until it succeeds, fails, `cancel` is set or the login timeout is over. `on_url` gets the sign-in
/// address once it is printed. Output is read as it comes.
pub fn login(tools: &Tools, opts: &Login, cancel: &AtomicBool, on_url: &mut dyn FnMut(String)) -> Result<(), String> {
    let mut key_file = KeyFile(None);
    let mut key_path = None;
    if let Some(key) = opts.auth_key.map(str::trim).filter(|k| !k.is_empty()) {
        let path = tools.run_dir.join(KEY_FILE);
        crate::secret::write_private(&path, key.as_bytes()).map_err(|e| format!("can't keep the key: {e}"))?;
        key_file = KeyFile(Some(path.clone()));
        key_path = Some(path);
    }
    let args = up_args(opts.hostname, key_path.as_deref(), None);
    let mut child = Command::new(&tools.tailscale)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("tailscale: {e}"))?;
    let (tx, rx) = mpsc::channel();
    read_lines(child.stdout.take(), tx.clone());
    read_lines(child.stderr.take(), tx);
    let deadline = Instant::now() + tools.timeouts.login;
    let mut seen = Seen::default();
    let result = loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => seen.feed(&line, on_url),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(50)),
        }
        match child.try_wait() {
            Ok(Some(exit)) => {
                while let Ok(line) = rx.recv_timeout(Duration::from_millis(150)) {
                    seen.feed(&line, on_url);
                }
                break if exit.success() {
                    Ok(())
                } else {
                    Err(if seen.last_line.is_empty() {
                        format!("tailscale up failed ({exit})")
                    } else {
                        seen.last_line.clone()
                    })
                };
            }
            Ok(None) => {}
            Err(e) => break Err(e.to_string()),
        }
        if cancel.load(Ordering::Relaxed) {
            kill_group(&mut child);
            break Err("Cancelled".into());
        }
        if Instant::now() >= deadline {
            kill_group(&mut child);
            break Err("Timed out".into());
        }
    };
    drop(key_file);
    result
}

/// `tailscale logout`: the device leaves the tailnet; the service keeps running, waiting for a login.
pub fn logout(tools: &Tools) -> Result<(), String> {
    tailscale(tools, &["logout".to_string()], tools.timeouts.action).map(|_| ())
}

/// `tailscale set --hostname`: the new name takes effect at once while logged in.
pub fn set_hostname(tools: &Tools, name: &str) -> Result<(), String> {
    if !valid_hostname(name) {
        return Err("Use letters, digits and hyphens".into());
    }
    let args = ["set".to_string(), format!("--hostname={name}"), "--ssh=false".to_string()];
    tailscale(tools, &args, tools.timeouts.action).map(|_| ())
}

/// Runs `work` on its own thread and hands the result to `done` there. Returns at once, whatever `work` does.
pub fn spawn<T: Send + 'static>(name: &str, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || done(work()));
    if let Err(e) = spawned {
        eprintln!("tailscale: can't start a thread for {name}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    const RUNNING: &str = include_str!("fixtures/tailscale-status-running.json");
    const NEEDS_LOGIN: &str = include_str!("fixtures/tailscale-status-needslogin.json");
    const STOPPED: &str = include_str!("fixtures/tailscale-status-stopped.json");
    const UP_OUTPUT: &str = include_str!("fixtures/tailscale-up-output.txt");
    const IP_OUTPUT: &str = include_str!("fixtures/tailscale-ip.txt");

    // --- parsing ---

    #[test]
    fn a_connected_node_reads_as_connected_with_its_names_and_addresses() {
        let s = Status::parse(RUNNING).unwrap();
        assert_eq!(s.backend, Backend::Running);
        assert_eq!(s.ips, ["100.101.102.103", "fd7a:115c:a1e0::5601:6667"]);
        assert_eq!(s.ipv4(), Some("100.101.102.103"));
        assert_eq!(s.hostname, "pocket-mini");
        assert_eq!(s.dns_name, "pocket-mini.tail1a2b3.ts.net", "no trailing dot");
        assert_eq!(s.tailnet, "alex@example.com");
        assert_eq!(s.login, "alex@example.com");
        assert_eq!((s.peers_online, s.peers_total), (2, 3));
        assert_eq!(s.auth_url, None, "an empty AuthURL is no address");
        assert!(s.health.is_empty());
    }

    #[test]
    fn a_logged_out_node_has_a_sign_in_address_and_no_tailnet() {
        let s = Status::parse(NEEDS_LOGIN).unwrap();
        assert_eq!(s.backend, Backend::NeedsLogin);
        assert_eq!(s.auth_url.as_deref(), Some("https://login.tailscale.com/a/1b2c3d4e5f6a7b"));
        assert!(s.ips.is_empty());
        assert_eq!(s.tailnet, "");
        assert_eq!(s.login, "");
        assert_eq!(s.peers_total, 0, "null peers");
        assert_eq!(s.hostname, "pocket-mini");
    }

    #[test]
    fn a_paused_node_keeps_its_account_but_has_no_address() {
        let s = Status::parse(STOPPED).unwrap();
        assert_eq!(s.backend, Backend::Stopped);
        assert_eq!(s.tailnet, "alex@example.com");
        assert_eq!(s.ipv4(), None);
        assert_eq!(s.health, ["Tailscale is stopped."]);
    }

    #[test]
    fn other_states_and_odd_json_do_not_fail() {
        let state = |name: &str| Status::parse(&format!(r#"{{"BackendState":"{name}"}}"#)).unwrap().backend;
        assert_eq!(state("NoState"), Backend::NoState);
        assert_eq!(state("NeedsMachineAuth"), Backend::NeedsMachineAuth);
        assert_eq!(state("Starting"), Backend::Starting);
        assert_eq!(state("SomethingNew"), Backend::Unknown);
        assert_eq!(Status::parse("{}").unwrap().backend, Backend::Unknown);
        assert!(Status::parse("failed to connect to local tailscaled").is_err());
        assert!(Status::parse("").is_err());
        assert!(Status::parse("[1,2]").is_err(), "not an object");
    }

    #[test]
    fn tailscale_ip_output_is_one_address_per_line() {
        assert_eq!(parse_ips(IP_OUTPUT), ["100.101.102.103", "fd7a:115c:a1e0::5601:6667"]);
        assert_eq!(parse_ips("100.1.2.3\n\n  \nnot an address\n"), ["100.1.2.3"]);
        assert!(parse_ips("failed to connect to local tailscaled").is_empty());
    }

    #[test]
    fn the_sign_in_address_comes_out_of_the_output_of_tailscale_up() {
        assert_eq!(login_url(UP_OUTPUT).as_deref(), Some("https://login.tailscale.com/a/1b2c3d4e5f6a7b"));
        // One line at a time, as it is read from the pipe.
        assert_eq!(
            login_url("\thttps://login.tailscale.com/a/abc123").as_deref(),
            Some("https://login.tailscale.com/a/abc123")
        );
        assert_eq!(login_url("To authenticate, visit:"), None);
        assert_eq!(login_url(""), None);
        // A self-hosted control server, trailing punctuation, carriage returns.
        assert_eq!(
            login_url("visit https://hs.example.org/register/nodekey:ab12.\r\n").as_deref(),
            Some("https://hs.example.org/register/nodekey:ab12")
        );
        // Not the documentation links of an error message.
        assert_eq!(login_url("Error: see https://tailscale.com/kb/1080/cli for help"), None);
        assert_eq!(login_url("Success."), None);
    }

    #[test]
    fn host_names_are_dns_labels() {
        for ok in ["pocket-mini", "a", "mini2", "p0cket", &"x".repeat(63)] {
            assert!(valid_hostname(ok), "{ok}");
        }
        for bad in [
            "",
            "Pocket",
            "pocket mini",
            "-pocket",
            "pocket-",
            "pocket_mini",
            "pocket.mini",
            "pöcket",
            &"x".repeat(64),
        ] {
            assert!(!valid_hostname(bad), "{bad}");
        }
        assert!(valid_hostname(SUGGESTED_HOSTNAME));
    }

    #[test]
    fn typed_names_are_cleaned_into_valid_ones() {
        assert_eq!(clean_hostname("Pocket Mini"), "pocket-mini");
        assert_eq!(clean_hostname("  pocket_mini.local "), "pocket-minilocal");
        assert_eq!(clean_hostname("--Pocket--"), "pocket");
        assert_eq!(clean_hostname("RG!"), "rg");
        assert_eq!(clean_hostname("???"), "");
        assert!(valid_hostname(&clean_hostname(&"Ab ".repeat(40))));
    }

    // --- the page's view of it ---

    fn snap(running: bool, status: Option<&str>) -> Snapshot {
        Snapshot {
            installed: true,
            running,
            boot: running,
            status: status.map(|j| Status::parse(j).unwrap()),
            error: None,
            offline: false,
        }
    }

    /// `snap` with the network gone.
    fn snap_offline(running: bool, status: Option<&str>) -> Snapshot {
        Snapshot { offline: true, ..snap(running, status) }
    }

    #[test]
    fn the_state_and_its_words() {
        let line = |s: &Snapshot| (s.state(), s.status_line());
        assert_eq!(line(&Snapshot::default()), (State::NotInstalled, "Not installed".into()));
        assert_eq!(line(&snap(false, None)), (State::Off, "Off".into()));
        assert_eq!(line(&snap(true, None)), (State::Starting, "Starting".into()));
        assert_eq!(line(&snap(true, Some(NEEDS_LOGIN))), (State::LoggedOut, "Logged out".into()));
        assert_eq!(line(&snap(true, Some(STOPPED))), (State::Paused, "Not connected".into()));
        assert_eq!(line(&snap(true, Some(RUNNING))), (State::Connected, "Connected as alex@example.com".into()));
        let waiting = snap(true, Some(r#"{"BackendState":"NeedsMachineAuth"}"#));
        assert_eq!(line(&waiting), (State::WaitingForApproval, "Waiting for approval".into()));
        let starting = snap(true, Some(r#"{"BackendState":"Starting"}"#));
        assert_eq!(line(&starting), (State::Connecting, "Connecting".into()));
        // Running but without a tailnet name yet: the account, else just "Connected".
        let nameless = snap(
            true,
            Some(r#"{"BackendState":"Running","User":{"1":{"LoginName":"a@b.c"}},"Self":{"UserID":1}}"#),
        );
        assert_eq!(nameless.status_line(), "Connected as a@b.c");
        assert_eq!(snap(true, Some(r#"{"BackendState":"Running"}"#)).status_line(), "Connected");
    }

    #[test]
    fn a_service_that_is_off_is_off_whatever_the_last_status_said() {
        let mut s = snap(false, Some(RUNNING));
        assert_eq!(s.state(), State::Off);
        s.running = true;
        assert_eq!(s.state(), State::Connected);
    }


    // --- no network under it ---

    #[test]
    fn running_without_a_network_is_no_network_yet_not_connected_or_connecting() {
        let line = |s: &Snapshot| (s.state(), s.status_line());
        let none = (State::NoNetwork, "No network yet".to_string());
        // Whatever Tailscale still says about itself: connected, starting, logged out, paused, waiting, silent.
        assert_eq!(line(&snap_offline(true, Some(RUNNING))), none);
        assert_eq!(line(&snap_offline(true, Some(r#"{"BackendState":"Starting"}"#))), none);
        assert_eq!(line(&snap_offline(true, Some(NEEDS_LOGIN))), none);
        assert_eq!(line(&snap_offline(true, Some(STOPPED))), none);
        assert_eq!(line(&snap_offline(true, Some(r#"{"BackendState":"NeedsMachineAuth"}"#))), none);
        assert_eq!(line(&snap_offline(true, None)), none, "a daemon that has not answered yet is not an error either");
        // What Tailscale itself says stays available for what depends on the account.
        assert_eq!(snap_offline(true, Some(RUNNING)).backend_state(), State::Connected);
        // A stopped service or a missing install are what they are, online or not.
        assert_eq!(line(&snap_offline(false, None)), (State::Off, "Off".into()));
        assert_eq!(Snapshot { offline: true, ..Snapshot::default() }.state(), State::NotInstalled);
        // And the same node with a network is what it was.
        assert_eq!(snap(true, Some(RUNNING)).state(), State::Connected);
    }

    #[test]
    fn the_network_is_up_when_wifi_is_joined_or_a_cable_has_a_link() {
        let dir = temp("net");
        let iface = |name: &str, operstate: &str| {
            std::fs::create_dir_all(dir.join(name)).unwrap();
            std::fs::write(dir.join(name).join("operstate"), format!("{operstate}\n")).unwrap();
        };
        assert!(!network_up(&dir.join("none")), "no folder: no network");
        assert!(!network_up(&dir), "nothing in it");
        // Loopback, Tailscale's own interface (its address outlives the network), containers: not a way out.
        iface("lo", "unknown");
        iface("tailscale0", "up");
        iface("docker0", "up");
        iface("veth1234", "up");
        assert!(!network_up(&dir), "only virtual interfaces");
        // Wi-Fi off, not joined or still joining.
        for state in ["down", "dormant", "lowerlayerdown", "notpresent", "unknown", ""] {
            iface("wlan0", state);
            assert!(!network_up(&dir), "wlan0 {state:?}");
        }
        iface("wlan0", "up");
        assert!(network_up(&dir), "Wi-Fi joined");
        // A cable counts when Wi-Fi is off.
        iface("wlan0", "down");
        iface("eth0", "up");
        assert!(network_up(&dir), "Ethernet while Wi-Fi is off");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn work_that_fails_when_the_network_is_gone_reports_that_and_nothing_raw() {
        let (t, _dir) = tools("net-failed", "exit 0");
        let raw: Result<(), String> = Err("dial tcp: lookup controlplane.tailscale.com: no such host".into());
        // Online: the failure is the failure.
        assert_eq!(failed_offline(&t, raw.clone()).unwrap_err(), "dial tcp: lookup controlplane.tailscale.com: no such host");
        assert_eq!(failed_offline(&t, Ok(())), Ok(()));
        // The network went in the meantime.
        wifi(&t.net_dir, "down");
        assert_eq!(failed_offline(&t, raw).unwrap_err(), NO_NETWORK);
        assert_eq!(failed_offline(&t, Ok(())), Ok(()), "a success is a success");
    }

    #[test]
    fn the_clis_advice_for_a_shell_becomes_plain_words() {
        assert_eq!(
            plain_error("failed to connect to local tailscaled; it doesn't appear to be running (sudo systemctl start tailscaled ?)"),
            "tailscale did not answer"
        );
        assert_eq!(plain_error("Logged out."), "Logged out.");
    }

    #[test]
    fn the_snapshot_follows_the_network_and_back_again_without_touching_the_switch() {
        // The node still says Running with its address: the network under it is gone.
        let (t, dir) = tools("net-snap", &format!("cat <<'EOF'\n{RUNNING}\nEOF"));
        std::fs::write(dir.join("system.cfg"), "tailscale.up=1\n").unwrap();
        std::fs::write(dir.join("active"), "").unwrap();
        let s = snapshot(&t);
        assert!(!s.offline);
        assert_eq!(s.state(), State::Connected);
        // Wi-Fi off: calm, and the user's choice (running, on at boot) is as it was.
        wifi(&t.net_dir, "down");
        let s = snapshot(&t);
        assert!(s.offline);
        assert_eq!((s.running, s.boot, s.error.as_deref()), (true, true, None));
        assert_eq!(s.state(), State::NoNetwork);
        // Back: the next snapshot is the truth again, nothing was kept.
        wifi(&t.net_dir, "up");
        let s = snapshot(&t);
        assert!(!s.offline);
        assert_eq!(s.state(), State::Connected);
    }

    #[test]
    fn a_daemon_that_does_not_answer_with_no_network_has_no_error_to_show_online_it_has() {
        let (t, dir) = tools("net-noanswer", "echo 'failed to connect to local tailscaled' >&2; exit 1");
        std::fs::write(dir.join("active"), "").unwrap();
        wifi(&t.net_dir, "down");
        let s = snapshot(&t);
        assert_eq!(s.state(), State::NoNetwork);
        wifi(&t.net_dir, "up");
        let s = snapshot(&t);
        assert_eq!(s.state(), State::Starting);
        assert_eq!(s.error.as_deref(), Some("tailscale did not answer"));
    }

    #[test]
    fn switching_on_a_paused_node_without_a_network_is_not_an_error_but_with_one_it_still_is() {
        // `up` runs into its timeout offline: Tailscale has been told to run and connects by itself later.
        let body = "echo \"ts $*\" >> $(dirname $0)/calls\nif [ \"$1\" = up ]; then echo 'timeout waiting for the network' >&2; exit 1; fi\necho '{\"BackendState\":\"Stopped\"}'";
        let (t, dir) = tools("net-enable", body);
        wifi(&t.net_dir, "down");
        enable(&t, None).expect("offline: no error");
        assert!(calls(&dir).contains("ts up --reset"), "still asked to run: {}", calls(&dir));
        assert!(std::fs::read_to_string(&t.system_cfg).unwrap().contains("tailscale.up=1"), "the choice is kept");
        // The same failure with a network is a real one.
        wifi(&t.net_dir, "up");
        assert_eq!(enable(&t, None).unwrap_err(), "timeout waiting for the network");
    }

    // --- the setting ---

    #[test]
    fn the_boot_setting_is_read_from_system_cfg() {
        let cfg = "audio.volume=60\nssh.enabled=0\ntailscale.up=1\nwifi.enabled=1\n";
        assert_eq!(setting_in(cfg, "tailscale.up").as_deref(), Some("1"));
        assert!(boot_enabled(cfg));
        assert!(!boot_enabled("tailscale.up=0\n"));
        assert!(!boot_enabled(""));
        assert!(!boot_enabled("xtailscale.up=1\ntailscale.upx=1\n"), "whole key only");
        assert_eq!(setting_in("a=1\na=2\n", "a").as_deref(), Some("2"), "the last one wins");
        assert_eq!(setting_in("ssh.enabled=1\n", "tailscale.up"), None);
        assert_eq!(setting_in("k=a=b\n", "k").as_deref(), Some("a=b"));
    }

    #[test]
    fn the_chosen_name_is_kept_across_restarts_and_a_bad_file_is_ignored() {
        let dir = temp("prefs");
        assert_eq!(Prefs::load(&dir), Prefs::default(), "nothing chosen yet");
        Prefs {
            hostname: Some("pocket-mini".into()),
        }
        .save(&dir);
        assert_eq!(Prefs::load(&dir).hostname.as_deref(), Some("pocket-mini"));
        std::fs::write(dir.join("tailscale.json"), r#"{"hostname":"Not valid!"}"#).unwrap();
        assert_eq!(Prefs::load(&dir).hostname, None);
        // A broken file falls back to the last good copy (fsutil keeps one), here the name saved first.
        std::fs::write(dir.join("tailscale.json"), "{ broken").unwrap();
        assert_eq!(Prefs::load(&dir).hostname.as_deref(), Some("pocket-mini"));
    }

    // --- stand-ins for the programs ---

    fn temp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("ts-{name}"))
    }

    fn script(path: &Path, body: &str) {
        crate::testutil::write_executable(path, format!("#!/bin/sh\n{body}\n"));
    }

    /// Tools whose `tailscale` runs `ts_body` and whose `systemctl` logs what it is asked; system.cfg is a file in
    /// the test folder (written by `system_cfg`, as on the device).
    fn tools(name: &str, ts_body: &str) -> (Tools, crate::testutil::TestDir) {
        let dir = temp(name);
        script(&dir.join("tailscale"), ts_body);
        script(
            &dir.join("systemctl"),
            &format!(
                "echo \"systemctl $*\" >> {d}/calls\nif [ \"$1\" = is-active ]; then [ -e {d}/active ]; fi",
                d = dir.display()
            ),
        );
        std::fs::write(dir.join("defaults.src"), "PORT=\"41641\"\nFLAGS=\"\"\n").unwrap();
        // A Wi-Fi that is joined, unless a test takes it away (`wifi`).
        wifi(&dir.join("net"), "up");
        let t = Tools {
            tailscale: dir.join("tailscale"),
            systemctl: dir.join("systemctl").display().to_string(),
            system_cfg: dir.join("system.cfg"),
            run_dir: dir.to_path_buf(),
            defaults: dir.join("conf/tailscaled.defaults"),
            defaults_src: dir.join("defaults.src"),
            net_dir: dir.join("net"),
            timeouts: Timeouts {
                status: Duration::from_millis(1500),
                action: Duration::from_secs(5),
                login: Duration::from_secs(5),
            },
        };
        (t, dir)
    }

    /// The `operstate` of `wlan0` under `net`; "down" is Wi-Fi off or not joined.
    fn wifi(net: &Path, operstate: &str) {
        std::fs::create_dir_all(net.join("wlan0")).unwrap();
        std::fs::write(net.join("wlan0/operstate"), format!("{operstate}\n")).unwrap();
    }

    fn calls(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
    }

    #[test]
    fn switching_on_sets_the_boot_setting_and_starts_the_service_and_off_undoes_it() {
        let (t, dir) = tools("switch", "echo '{\"BackendState\":\"NeedsLogin\"}'");
        std::fs::write(dir.join("system.cfg"), "ssh.enabled=0\ntailscale.up=0\n").unwrap();
        enable(&t, None).unwrap();
        let cfg = std::fs::read_to_string(&t.system_cfg).unwrap();
        assert!(boot_enabled(&cfg), "ROCKNIX's autostart will start tailscaled at the next boot: {cfg}");
        assert!(cfg.contains("ssh.enabled=0"), "other settings stay: {cfg}");
        assert_eq!(cfg.matches("tailscale.up").count(), 1, "one line, not two");
        assert!(calls(&dir).contains("systemctl start tailscaled"), "{}", calls(&dir));
        assert!(t.defaults.exists(), "tailscaled.service needs its defaults file or it does not start");
        disable(&t).unwrap();
        let cfg = std::fs::read_to_string(&t.system_cfg).unwrap();
        assert!(!boot_enabled(&cfg), "{cfg}");
        assert_eq!(setting_in(&cfg, "tailscale.up").as_deref(), Some("0"));
        assert!(calls(&dir).contains("systemctl stop tailscaled"), "{}", calls(&dir));
    }

    #[test]
    fn switching_on_turns_tailscale_ssh_off_whatever_it_was() {
        // The node is already running and signed in, maybe with `--ssh` set by hand: no `up`, but `set --ssh=false`.
        let (t, dir) = tools("ssh-off", "echo \"ts $*\" >> $(dirname $0)/calls\necho '{\"BackendState\":\"Running\"}'");
        enable(&t, None).unwrap();
        let c = calls(&dir);
        assert!(c.contains("ts set --ssh=false"), "{c}");
        assert!(!c.contains("ts up"), "a connected node is left alone: {c}");
    }

    #[test]
    fn a_failing_start_is_an_error_and_a_paused_node_is_brought_back() {
        let (t, dir) = tools("start", "echo \"ts $*\" >> $(dirname $0)/calls\necho '{\"BackendState\":\"Stopped\"}'");
        enable(&t, Some("pocket-mini")).unwrap();
        let c = calls(&dir);
        assert!(
            c.contains("ts up --reset --ssh=false --hostname=pocket-mini"),
            "a stopped node is started again: {c}"
        );
        let (mut t, _dir) = tools("start-fails", "echo '{}'");
        script(Path::new(&t.systemctl), "echo 'Failed to start tailscaled.service' >&2; exit 1");
        t.timeouts.action = Duration::from_secs(2);
        assert_eq!(enable(&t, None).unwrap_err(), "Failed to start tailscaled.service");
    }

    #[test]
    fn the_snapshot_reads_the_service_the_setting_and_the_status() {
        let (t, dir) = tools(
            "snap",
            "cat <<'EOF'\n{\"BackendState\":\"NeedsLogin\",\"AuthURL\":\"https://login.tailscale.com/a/xyz\"}\nEOF",
        );
        std::fs::write(dir.join("system.cfg"), "tailscale.up=1\n").unwrap();
        let s = snapshot(&t);
        assert!(s.installed && !s.running && s.boot);
        assert_eq!(s.state(), State::Off, "enabled for the next boot, not running now");
        std::fs::write(dir.join("active"), "").unwrap();
        let s = snapshot(&t);
        assert!(s.running);
        assert_eq!(s.state(), State::LoggedOut);
        assert_eq!(s.status.unwrap().auth_url.as_deref(), Some("https://login.tailscale.com/a/xyz"));
        // Not there at all.
        let mut gone = t.clone();
        gone.tailscale = dir.join("no-such-tailscale");
        assert_eq!(snapshot(&gone).state(), State::NotInstalled);
    }

    #[test]
    fn a_daemon_that_does_not_answer_is_starting_with_the_reason_kept() {
        let (t, dir) = tools(
            "noanswer",
            "echo 'failed to connect to local tailscaled; it does not appear to be running' >&2; exit 1",
        );
        std::fs::write(dir.join("active"), "").unwrap();
        let s = snapshot(&t);
        assert_eq!(s.state(), State::Starting);
        assert_eq!(s.error.as_deref(), Some("tailscale did not answer"));
    }

    // --- never blocks ---

    #[test]
    fn a_hanging_tailscale_or_systemctl_costs_a_bounded_wait() {
        let (t, dir) = tools("hang", "sleep 30");
        std::fs::write(dir.join("active"), "").unwrap();
        let t0 = Instant::now();
        let s = snapshot(&t);
        assert!(t0.elapsed() < Duration::from_secs(5), "waited {:?}", t0.elapsed());
        assert_eq!(s.state(), State::Starting);
        assert!(s.error.is_some());
        // systemctl hanging: also cut off.
        script(Path::new(&t.systemctl), "sleep 30");
        let t0 = Instant::now();
        let _ = snapshot(&t);
        assert!(t0.elapsed() < Duration::from_secs(5), "waited {:?}", t0.elapsed());
        let mut t2 = t.clone();
        t2.timeouts.action = Duration::from_millis(400);
        let t0 = Instant::now();
        assert!(enable(&t2, None).is_err());
        assert!(logout(&t2).is_err());
        assert!(set_hostname(&t2, "x").is_err());
        assert!(disable(&t2).is_err());
        assert!(t0.elapsed() < Duration::from_secs(6), "waited {:?}", t0.elapsed());
    }

    #[test]
    fn spawn_returns_at_once_and_delivers_later() {
        let (t, dir) = tools("spawn", "sleep 30");
        std::fs::write(dir.join("active"), "").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let t0 = Instant::now();
        spawn("test-snapshot", move || snapshot(&t), move |s| tx.send(s).unwrap());
        assert!(t0.elapsed() < Duration::from_millis(100), "the caller waited {:?}", t0.elapsed());
        assert!(rx.try_recv().is_err(), "not done yet");
        let s = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(t0.elapsed() >= Duration::from_millis(1000), "the stand-in hangs; the timeout ended it");
        assert!(s.installed);
    }

    // --- the login ---

    #[test]
    fn up_always_says_ssh_false_and_never_carries_the_key() {
        let args = up_args(Some("pocket-mini"), None, Some(Duration::from_secs(7)));
        assert_eq!(args, ["up", "--reset", "--ssh=false", "--hostname=pocket-mini", "--timeout=7s"]);
        let key = Path::new("/run/pocket-ui-tailscale-key");
        let args = up_args(None, Some(key), None);
        assert_eq!(args, ["up", "--reset", "--ssh=false", "--auth-key=file:/run/pocket-ui-tailscale-key"]);
        assert!(!args.iter().any(|a| a.contains("--ssh=true") || a == "--ssh"));
    }

    #[test]
    fn the_login_reads_the_address_as_it_is_printed_and_ends_when_tailscale_does() {
        // Prints the address on stderr like the real one, waits (the user is on the phone), then succeeds.
        let (t, dir) = tools("login", "echo \"ts $*\" >> $(dirname $0)/calls\nprintf '\\nTo authenticate, visit:\\n\\n\\thttps://login.tailscale.com/a/1b2c3d4e5f6a7b\\n\\n' >&2\nsleep 1\necho Success.");
        let cancel = AtomicBool::new(false);
        let mut urls = Vec::new();
        let t0 = Instant::now();
        let r = login(
            &t,
            &Login {
                hostname: Some("pocket-mini"),
                auth_key: None,
            },
            &cancel,
            &mut |u| urls.push((u, t0.elapsed())),
        );
        let total = t0.elapsed();
        assert_eq!(r, Ok(()));
        assert_eq!(urls.len(), 1, "{urls:?}");
        assert_eq!(urls[0].0, "https://login.tailscale.com/a/1b2c3d4e5f6a7b");
        // The stand-in waits a second after printing: the address was handed over while it still ran, not at its end.
        assert!(
            urls[0].1 + Duration::from_millis(500) <= total,
            "the address came out before tailscale finished: {:?} of {total:?}",
            urls[0].1
        );
        assert!(calls(&dir).contains("ts up --reset --ssh=false --hostname=pocket-mini"), "{}", calls(&dir));
    }

    #[test]
    fn cancelling_a_login_ends_tailscale_up() {
        let (t, _dir) = tools("cancel", "echo 'https://login.tailscale.com/a/abc' >&2\nsleep 30");
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let t0 = Instant::now();
        let r = login(
            &t,
            &Login {
                hostname: None,
                auth_key: None,
            },
            &cancel,
            &mut |_| flag.store(true, Ordering::Relaxed),
        );
        assert_eq!(r, Err("Cancelled".into()));
        assert!(t0.elapsed() < Duration::from_secs(3), "waited {:?}", t0.elapsed());
    }

    #[test]
    fn a_login_nobody_approves_times_out_and_a_failure_says_why() {
        let (mut t, _dir) = tools("timeout", "echo 'https://login.tailscale.com/a/abc' >&2\nsleep 30");
        t.timeouts.login = Duration::from_millis(500);
        let t0 = Instant::now();
        let r = login(
            &t,
            &Login {
                hostname: None,
                auth_key: None,
            },
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert_eq!(r, Err("Timed out".into()));
        assert!(t0.elapsed() < Duration::from_secs(3));
        let (t, _dir) = tools("fails", "echo 'Error: backend error: invalid key' >&2\nexit 1");
        let r = login(
            &t,
            &Login {
                hostname: None,
                auth_key: Some("tskey-auth-bad"),
            },
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert_eq!(r, Err("Error: backend error: invalid key".into()));
    }

    #[test]
    fn an_auth_key_goes_in_a_private_file_that_is_gone_afterwards() {
        // The stand-in copies what it was given, and the file's mode, to see what `tailscale up` would have seen.
        let body = "echo \"ts $*\" >> $(dirname $0)/calls\nfor a in \"$@\"; do case $a in --auth-key=file:*) f=${a#--auth-key=file:};; esac; done\nstat -f %Lp \"$f\" 2>/dev/null > $(dirname $0)/mode || stat -c %a \"$f\" > $(dirname $0)/mode\ncat \"$f\" > $(dirname $0)/seen";
        let (t, dir) = tools("authkey", body);
        let r = login(
            &t,
            &Login {
                hostname: Some("pocket-mini"),
                auth_key: Some("tskey-auth-kABC123CNTRL-secret"),
            },
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert_eq!(r, Ok(()));
        assert_eq!(std::fs::read_to_string(dir.join("seen")).unwrap(), "tskey-auth-kABC123CNTRL-secret");
        assert_eq!(std::fs::read_to_string(dir.join("mode")).unwrap().trim(), "600");
        assert!(!calls(&dir).contains("tskey"), "the key is not on the command line: {}", calls(&dir));
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("key"))
            .collect();
        assert!(left.is_empty(), "key file left behind: {left:?}");
    }

    #[test]
    fn logout_and_set_hostname_call_tailscale_with_the_right_words() {
        let (t, dir) = tools("words", "echo \"ts $*\" >> $(dirname $0)/calls");
        logout(&t).unwrap();
        set_hostname(&t, "pocket-mini").unwrap();
        assert_eq!(calls(&dir), "ts logout\nts set --hostname=pocket-mini --ssh=false\n");
        assert!(set_hostname(&t, "Not Valid").is_err(), "checked before anything runs");
        assert_eq!(calls(&dir).lines().count(), 2);
    }
}
