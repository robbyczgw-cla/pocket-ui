//! Diagnostic-only PipeWire sampling, away from the audio and UI threads.
//!
//! `pw-top` before per-node profiler counters can display the driver's ERR on followers. We accept only a server
//! version with the per-node field (1.2.6 or newer), playback nodes owned by this PID, and follower rows. Identity is
//! checked again after `pw-top` to exclude a recycled node ID. No matching data is unknown, never zero.
//! See PipeWire 1.2.6 src/modules/module-profiler.c (followerBlock includes na->xrun_count) and
//! src/tools/pw-top.c (XRUN_INVALID otherwise falls back to the driver's count). ERR includes errors as well as xruns.

use serde::Serialize;
#[cfg(any(target_os = "linux", test))]
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

const PERIOD: Duration = Duration::from_secs(2);
const FINISH_WAIT: Duration = Duration::from_millis(80);
#[cfg(any(target_os = "linux", test))]
const MAX_NODES: usize = 64;
const PROVENANCE: &str =
    "PipeWire >=1.2.6 per-follower pw-top ERR; PID/client ownership and node serial verified before/after sampling; includes xruns and errors";

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub id: u32,
    pub serial: u64,
    pub server_cookie: u64,
    pub name: String,
    /// Sum of observed increments, including the first lifetime counter and increments following explicit resets.
    pub xruns_and_errors: u64,
    pub raw_counter: u64,
    pub counter_resets: u64,
    pub present: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    /// available, starting, unavailable or partial. Counts cover observed node lifetimes, not an unobserved prefix
    /// (a stream may close before the first match) or the final playback tail.
    pub status: &'static str,
    pub reason: Option<String>,
    pub client_xruns: Option<u64>,
    /// Observed ERR increments, retained even when a sampling gap makes a session-wide claim unavailable.
    pub observed_xruns: u64,
    pub server_version: Option<String>,
    pub pw_top_version: Option<String>,
    pub pw_top_library_version: Option<String>,
    pub remote: String,
    pub provenance: &'static str,
    pub nodes: Vec<Node>,
    pub retired_nodes: u64,
    pub samples: u64,
    pub sampling_gaps: u64,
    pub last_sample_ms: Option<u64>,
    pub age_ms: Option<u64>,
    pub max_sample_gap_ms: u64,
    pub sampling_period_ms: u64,
    pub retry_backoff_ms: u64,
    pub terminal_unavailable: bool,
    pub stop_timed_out: bool,
    /// Sampling ends at the last verified observation; the tail of playback is not silently asserted error-free.
    pub final_sample_age_ms: Option<u64>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: "starting",
            reason: Some("waiting for a verified playback node".into()),
            client_xruns: None,
            observed_xruns: 0,
            server_version: None,
            pw_top_version: None,
            pw_top_library_version: None,
            remote: std::env::var("PIPEWIRE_REMOTE")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "pipewire-0".into()),
            provenance: PROVENANCE,
            nodes: Vec::new(),
            retired_nodes: 0,
            samples: 0,
            sampling_gaps: 0,
            last_sample_ms: None,
            age_ms: None,
            max_sample_gap_ms: 0,
            sampling_period_ms: PERIOD.as_millis() as u64,
            retry_backoff_ms: 0,
            terminal_unavailable: false,
            stop_timed_out: false,
            final_sample_age_ms: None,
        }
    }
}

pub struct Collector {
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    ended: Option<std::sync::mpsc::Receiver<()>>,
    started: Instant,
}

impl Collector {
    /// Starts helpers only on a worker. Android has no PipeWire and returns an explicit unsupported snapshot.
    pub fn start() -> Self {
        let started = Instant::now();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        #[cfg(target_os = "linux")]
        let ended = {
            let (tx, rx) = std::sync::mpsc::channel();
            let (state, stop) = (state.clone(), stop.clone());
            std::thread::spawn(move || {
                if let Err(reason) = tools_available() {
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    s.gap(reason);
                    s.snapshot.terminal_unavailable = true;
                    let _ = tx.send(());
                    return;
                }
                let tool_versions = helper("pw-top-version", &stop).and_then(|v| top_version(&v));
                match tool_versions {
                    Ok((compiled, linked)) => {
                        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                        s.snapshot.pw_top_version = Some(compiled);
                        s.snapshot.pw_top_library_version = Some(linked);
                    }
                    Err(reason) => {
                        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                        s.gap(reason);
                        s.snapshot.terminal_unavailable = true;
                        let _ = tx.send(());
                        return;
                    }
                }
                let pid = std::process::id();
                let mut failures = 0u32;
                while !stop.load(Ordering::Relaxed) {
                    let cycle = Instant::now();
                    let result = sample(pid, &stop);
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    let terminal = match result {
                        Ok(sample) => {
                            s.observe(sample, started.elapsed().as_millis() as u64);
                            failures = 0;
                            false
                        }
                        Err(failure) => {
                            if failure.version.is_some() {
                                s.snapshot.server_version = failure.version;
                            }
                            s.gap(failure.reason);
                            s.snapshot.terminal_unavailable = failure.terminal;
                            failures = failures.saturating_add(1);
                            failure.terminal
                        }
                    };
                    let interval = (PERIOD * (1u32 << failures.saturating_sub(1).min(4))).min(Duration::from_secs(30));
                    s.snapshot.retry_backoff_ms = if failures > 0 { interval.as_millis() as u64 } else { 0 };
                    drop(s);
                    if terminal {
                        break;
                    }
                    while !stop.load(Ordering::Relaxed) && cycle.elapsed() < interval {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                let _ = tx.send(());
            });
            Some(rx)
        };
        #[cfg(not(target_os = "linux"))]
        let ended = {
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            s.snapshot.status = "unavailable";
            s.snapshot.reason = Some("this platform does not use PipeWire".into());
            s.snapshot.terminal_unavailable = true;
            None
        };
        Self { state, stop, ended, started }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner()).snapshot.clone();
        s.age_ms = s.last_sample_ms.map(|last| (self.started.elapsed().as_millis() as u64).saturating_sub(last));
        // A stalled sampler must not keep advertising its old zero as a current measurement.
        if s.age_ms.is_some_and(|age| age > 6_000) && s.status == "available" {
            s.status = "partial";
            s.reason = Some("PipeWire observation is stale".into());
            s.client_xruns = None;
        }
        s
    }

    /// Stops collection and returns cached evidence. At most 80 ms of waiting, no helper runs on this thread.
    /// This reports the unobserved final tail explicitly; it is not an end-of-playback assertion of zero xruns.
    pub fn finish(&mut self) -> Snapshot {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(ended) = self.ended.take() {
            if matches!(ended.recv_timeout(FINISH_WAIT), Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
                let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                s.snapshot.stop_timed_out = true;
                s.snapshot.status = "partial";
                s.snapshot.reason = Some("sampler did not stop within 80 ms".into());
                s.snapshot.client_xruns = None;
            }
        }
        let mut s = self.snapshot();
        s.final_sample_age_ms = s.age_ms;
        if s.samples > 0 {
            s.status = "partial";
            s.reason = Some(
                if s.stop_timed_out {
                    "sampler stop limit reached; final playback tail unobserved"
                } else {
                    "final playback tail after the last verified sample is unobserved"
                }
                .into(),
            );
            s.client_xruns = None;
        } else {
            s.status = "unavailable";
            if s.reason.as_deref() == Some("waiting for a verified playback node") {
                s.reason = Some("collector stopped before any verified playback observation".into());
            }
        }
        self.state.lock().unwrap_or_else(|e| e.into_inner()).snapshot = s.clone();
        s
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Default)]
struct State {
    snapshot: Snapshot,
    #[cfg(any(target_os = "linux", test))]
    retired_xruns: u64,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    id: u32,
    serial: u64,
    cookie: u64,
    name: String,
}
#[cfg(any(target_os = "linux", test))]
struct Observation {
    version: String,
    counts: Vec<(Identity, u64)>,
}

#[cfg(any(target_os = "linux", test))]
impl State {
    fn gap(&mut self, reason: String) {
        self.snapshot.sampling_gaps = self.snapshot.sampling_gaps.saturating_add(1);
        self.snapshot.status = "unavailable";
        self.snapshot.reason = Some(reason);
        self.snapshot.client_xruns = None;
    }

    fn observe(&mut self, sample: Observation, at_ms: u64) {
        let s = &mut self.snapshot;
        if let Some(last) = s.last_sample_ms {
            let gap = at_ms.saturating_sub(last);
            s.max_sample_gap_ms = s.max_sample_gap_ms.max(gap);
            if gap > 6_000 {
                s.sampling_gaps = s.sampling_gaps.saturating_add(1);
            }
        }
        for node in &mut s.nodes {
            node.present = false;
        }
        for (id, raw) in sample.counts {
            if let Some(n) = s
                .nodes
                .iter_mut()
                .find(|n| n.id == id.id && n.serial == id.serial && n.server_cookie == id.cookie)
            {
                let increment = if raw >= n.raw_counter {
                    raw - n.raw_counter
                } else {
                    n.counter_resets = n.counter_resets.saturating_add(1);
                    raw
                };
                n.xruns_and_errors = n.xruns_and_errors.saturating_add(increment);
                n.raw_counter = raw;
                n.present = true;
            } else {
                s.nodes.push(Node {
                    id: id.id,
                    serial: id.serial,
                    server_cookie: id.cookie,
                    name: id.name,
                    xruns_and_errors: raw,
                    raw_counter: raw,
                    counter_resets: 0,
                    present: true,
                });
            }
        }
        // Keep bounded node detail while preserving the cumulative contribution of closed streams.
        while s.nodes.len() > MAX_NODES {
            if let Some(i) = s.nodes.iter().position(|n| !n.present) {
                let retired = s.nodes.remove(i);
                self.retired_xruns = self.retired_xruns.saturating_add(retired.xruns_and_errors);
                s.retired_nodes = s.retired_nodes.saturating_add(1);
            } else {
                s.nodes.truncate(MAX_NODES);
                s.sampling_gaps = s.sampling_gaps.saturating_add(1);
                break;
            }
        }
        s.observed_xruns = s.nodes.iter().fold(self.retired_xruns, |sum, n| sum.saturating_add(n.xruns_and_errors));
        s.samples = s.samples.saturating_add(1);
        s.last_sample_ms = Some(at_ms);
        s.server_version = Some(sample.version);
        if s.sampling_gaps == 0 {
            s.status = "available";
            s.reason = None;
            s.client_xruns = Some(s.observed_xruns);
        } else {
            s.status = "partial";
            s.reason = Some("sampling gaps prevent a complete client-xrun count; observed counters retained".into());
            s.client_xruns = None;
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn number(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
#[cfg(any(target_os = "linux", test))]
fn proven_version(v: &str) -> bool {
    let mut parts = v.split('.');
    let parse = |s: &str| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok();
    match (
        parts.next().and_then(|p| p.parse::<u32>().ok()),
        parts.next().and_then(|p| p.parse::<u32>().ok()),
        parts.next().and_then(parse),
    ) {
        (Some(a), Some(b), Some(c)) => (a, b, c) >= (1, 2, 6),
        _ => false,
    }
}

#[cfg(any(target_os = "linux", test))]
fn top_version(text: &[u8]) -> Result<(String, String), String> {
    let text = std::str::from_utf8(text).map_err(|_| "pw-top version unavailable")?;
    let compiled = text
        .lines()
        .find_map(|l| l.strip_prefix("Compiled with libpipewire "))
        .ok_or("pw-top compiled version unavailable")?;
    let linked = text
        .lines()
        .find_map(|l| l.strip_prefix("Linked with libpipewire "))
        .ok_or("pw-top linked version unavailable")?;
    if !proven_version(compiled) || !proven_version(linked) {
        return Err(format!(
            "unproven pw-top versions compiled={compiled} linked={linked}; client ERR attribution unavailable"
        ));
    }
    Ok((compiled.to_owned(), linked.to_owned()))
}

#[cfg(any(target_os = "linux", test))]
fn owned_nodes(json: &[u8], pid: u32) -> Result<(String, Vec<Identity>), String> {
    let objects: Vec<serde_json::Value> = serde_json::from_slice(json).map_err(|_| "invalid pw-dump JSON")?;
    let core = objects
        .iter()
        .find(|v| v["type"] == "PipeWire:Interface:Core")
        .ok_or("pw-dump has no server identity")?;
    let version = core["info"]["version"].as_str().ok_or("PipeWire server version unavailable")?.to_owned();
    if !proven_version(&version) {
        return Err(format!("unproven PipeWire server version {version}; follower ERR may be a graph counter"));
    }
    let cookie = number(&core["info"]["cookie"]).ok_or("PipeWire server cookie unavailable")?;
    let clients: BTreeMap<u64, u64> = objects
        .iter()
        .filter(|v| v["type"] == "PipeWire:Interface:Client")
        .filter_map(|v| Some((number(&v["id"])?, number(&v["info"]["props"]["application.process.id"])?)))
        .collect();
    let mut nodes = Vec::new();
    for v in objects.iter().filter(|v| v["type"] == "PipeWire:Interface:Node") {
        let p = &v["info"]["props"];
        if p["media.class"] != "Stream/Output/Audio" {
            continue;
        }
        // Explicit node PID takes priority over the transport client (notably pipewire-pulse).
        let owner = number(&p["application.process.id"]).or_else(|| clients.get(&number(&p["client.id"])?).copied());
        if owner != Some(pid as u64) {
            continue;
        }
        let id = number(&v["id"]).and_then(|v| u32::try_from(v).ok()).ok_or("owned node ID unavailable")?;
        let serial = number(&p["object.serial"]).ok_or("owned node serial unavailable")?;
        nodes.push(Identity {
            id,
            serial,
            cookie,
            name: p["node.name"].as_str().unwrap_or("unnamed").chars().take(128).collect(),
        });
    }
    if nodes.is_empty() {
        return Err("no playback node belongs to this process".into());
    }
    if nodes.len() > MAX_NODES {
        return Err("too many owned playback nodes to sample safely".into());
    }
    nodes.sort_by_key(|n| (n.cookie, n.id, n.serial));
    Ok((version, nodes))
}

#[cfg(any(target_os = "linux", test))]
fn follower_counts(text: &str, nodes: &[Identity]) -> Result<Vec<(Identity, u64)>, String> {
    let mut frame = BTreeMap::new();
    let mut header = false;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.starts_with(&["S", "ID", "QUANT", "RATE", "WAIT", "BUSY", "W/Q", "B/Q", "ERR"]) {
            frame.clear();
            header = true;
            continue;
        }
        if !header || fields.len() < 10 {
            continue;
        }
        let Some(id) = fields[1].parse::<u32>().ok() else {
            continue;
        };
        let Some(node) = nodes.iter().find(|n| n.id == id) else {
            continue;
        };
        let row = if !matches!(fields[0], "R" | "I" | "t" | "T") {
            Err("owned playback node is not active")
        } else if !line.ends_with(&format!(" + {}", node.name)) {
            Err("owned playback node is not a profiled follower; driver ERR excluded")
        } else {
            fields[8].parse::<u64>().map_err(|_| "owned follower ERR is unavailable")
        };
        frame.insert(id, row);
    }
    if !header {
        return Err("pw-top header unavailable".into());
    }
    nodes
        .iter()
        .map(|n| {
            let raw = frame
                .get(&n.id)
                .ok_or("owned playback node missing from final pw-top frame")?
                .as_ref()
                .map_err(|e| *e)?;
            Ok((n.clone(), *raw))
        })
        .collect::<Result<_, &str>>()
        .map_err(str::to_owned)
}

#[cfg(target_os = "linux")]
fn tools_available() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<_> = std::env::split_paths(&path).collect();
    let missing: Vec<_> = ["sh", "head", "pw-dump", "pw-top"]
        .into_iter()
        .filter(|name| {
            !dirs
                .iter()
                .any(|dir| std::fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("PipeWire tools unavailable: {}", missing.join(", ")))
    }
}

#[cfg(target_os = "linux")]
struct Failure {
    reason: String,
    version: Option<String>,
    terminal: bool,
}

#[cfg(target_os = "linux")]
impl From<String> for Failure {
    fn from(reason: String) -> Self {
        Self {
            reason,
            version: None,
            terminal: false,
        }
    }
}

#[cfg(target_os = "linux")]
fn helper(command: &'static str, stop: &AtomicBool) -> Result<Vec<u8>, String> {
    // Fixed read-only commands. Head caps stdout BEFORE proc's reader retains it; utilities' stderr is closed.
    // A cap hit is an invalid sample, not truncated evidence. A separate process group is killed on timeout/stop.
    const LIMIT: usize = 1_048_576;
    let script = match command {
        "pw-dump" => "pw-dump -r \"${PIPEWIRE_REMOTE:-pipewire-0}\" 2>/dev/null | head -c 1048577",
        "pw-top" => "LC_ALL=C pw-top -r \"${PIPEWIRE_REMOTE:-pipewire-0}\" -b -n 2 2>/dev/null | head -c 1048577",
        "pw-top-version" => "LC_ALL=C pw-top --version 2>/dev/null | head -c 1048577",
        _ => unreachable!(),
    };
    let result = crate::proc::run_until(std::process::Command::new("sh").args(["-c", script]), Duration::from_secs(4), stop)
        .map_err(|e| format!("{command} unavailable: {}", e.kind()))?;
    if !result.status.success() || result.stdout.is_empty() {
        return Err(format!("{command} returned no valid observation (tool or server unavailable)"));
    }
    if result.stdout.len() > LIMIT {
        return Err(format!("{command} exceeded the diagnostic output limit"));
    }
    Ok(result.stdout)
}

#[cfg(target_os = "linux")]
fn sample(pid: u32, stop: &AtomicBool) -> Result<Observation, Failure> {
    let dump = helper("pw-dump", stop)?;
    let version = serde_json::from_slice::<Vec<serde_json::Value>>(&dump).ok().and_then(|objects| {
        objects.iter().find(|v| v["type"] == "PipeWire:Interface:Core")?["info"]["version"]
            .as_str()
            .map(str::to_owned)
    });
    let (version, before) = owned_nodes(&dump, pid).map_err(|reason| Failure {
        terminal: reason.starts_with("unproven PipeWire server version") || reason == "PipeWire server version unavailable",
        reason,
        version,
    })?;
    let failure = |reason| Failure {
        reason,
        version: Some(version.clone()),
        terminal: false,
    };
    let text = helper("pw-top", stop)?;
    let counts = follower_counts(std::str::from_utf8(&text).map_err(|_| failure("pw-top output is not UTF-8".into()))?, &before).map_err(failure)?;
    let (after_version, after) = owned_nodes(&helper("pw-dump", stop)?, pid).map_err(failure)?;
    if version != after_version || before != after {
        return Err(failure("playback node identity changed while sampling".into()));
    }
    Ok(Observation { version, counts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dump(version: &str) -> Vec<u8> {
        serde_json::to_vec(&json!([
            {"id":0,"type":"PipeWire:Interface:Core","info":{"version":version,"cookie":77}},
            {"id":1,"type":"PipeWire:Interface:Client","info":{"props":{"application.process.id":"42"}}},
            {"id":2,"type":"PipeWire:Interface:Client","info":{"props":{"application.process.id":"99"}}},
            {"id":10,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","client.id":"1","object.serial":"100","node.name":"game"}}},
            {"id":11,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","client.id":"2","object.serial":"101","node.name":"game"}}},
            {"id":12,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","application.process.id":"42","object.serial":"102"}}},
            {"id":13,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","client.id":"1","application.process.id":"99","object.serial":"103"}}},
            {"id":14,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Input/Audio","application.process.id":"42","object.serial":"104"}}}
        ])).unwrap()
    }
    fn identity(id: u32, serial: u64) -> Identity {
        Identity {
            id,
            serial,
            cookie: 77,
            name: "game".into(),
        }
    }
    fn observation(id: u32, serial: u64, raw: u64) -> Observation {
        Observation {
            version: "1.2.6".into(),
            counts: vec![(identity(id, serial), raw)],
        }
    }
    const HEADER: &str = "S ID QUANT RATE WAIT BUSY W/Q B/Q ERR FORMAT NAME\n";

    #[test]
    fn foreign_process_same_name_driver_and_capture_are_excluded() {
        let (_, nodes) = owned_nodes(&dump("1.2.6"), 42).unwrap();
        assert_eq!(nodes, vec![identity(10, 100)]);
        let text = format!("{HEADER}R 50 256 48000 1us 2us 0.1 0.1 910 S16LE 2 48000 driver\nR 11 0 0 1us 2us 0.1 0.1 800 S16LE 2 48000 + game\nR 10 0 0 1us 2us 0.1 0.1 3 S16LE 2 48000 + game\n");
        assert_eq!(follower_counts(&text, &nodes).unwrap()[0].1, 3);
        let driver = format!("{HEADER}R 10 256 48000 1us 2us 0.1 0.1 910 S16LE 2 48000 game\n");
        assert!(follower_counts(&driver, &nodes).unwrap_err().contains("driver ERR excluded"));
        let mut named_plus = identity(10, 100);
        named_plus.name = "+ game".into();
        assert!(
            follower_counts(&text, &[named_plus]).is_err(),
            "a '+' in the name cannot turn a driver into a follower"
        );
    }

    #[test]
    fn pulse_stream_process_identity_overrides_the_transport_server() {
        let mut objects: Vec<serde_json::Value> = serde_json::from_slice(&dump("1.2.6")).unwrap();
        let node = objects.iter_mut().find(|v| v["id"] == 10).unwrap();
        node["info"]["props"]["client.id"] = json!("2"); // foreign pipewire-pulse process
        node["info"]["props"]["application.process.id"] = json!("42"); // actual playback owner
        let (_, nodes) = owned_nodes(&serde_json::to_vec(&objects).unwrap(), 42).unwrap();
        assert_eq!(nodes, vec![identity(10, 100)]);
    }

    #[test]
    fn unknown_old_or_missing_data_never_becomes_zero() {
        for v in ["0.3.80", "1.2.5", "unknown", "1.2", "1foo.2.6"] {
            assert!(owned_nodes(&dump(v), 42).is_err());
        }
        assert!(owned_nodes(&dump("1.2.6"), 888).is_err());
        assert!(owned_nodes(b"[]", 42).is_err());
        let nodes = vec![identity(10, 100)];
        assert!(follower_counts("pw-top: not found", &nodes).is_err());
        assert!(follower_counts(HEADER, &nodes).is_err());
        let mut state = State::default();
        assert_eq!(state.snapshot.client_xruns, None);
        state.gap("tool missing".into());
        assert_eq!(state.snapshot.client_xruns, None);
        state.observe(observation(10, 100, 0), 2000);
        assert_eq!(state.snapshot.client_xruns, None, "a missing interval cannot later be advertised as zero");
        assert_eq!(state.snapshot.status, "partial");
    }

    #[test]
    fn an_old_pw_top_cannot_masquerade_its_driver_count_as_client_data_on_a_new_server() {
        assert!(top_version(b"pw-top\nCompiled with libpipewire 0.3.80\nLinked with libpipewire 1.2.6\n").is_err());
        assert!(top_version(b"pw-top\nCompiled with libpipewire 1.2.6\nLinked with libpipewire 0.3.80\n").is_err());
        assert!(top_version(b"pw-top version unknown").is_err());
        assert_eq!(
            top_version(b"pw-top\nCompiled with libpipewire 1.2.6\nLinked with libpipewire 1.2.6\n").unwrap(),
            ("1.2.6".into(), "1.2.6".into())
        );
    }

    #[test]
    fn increments_resets_and_reopened_nodes_preserve_observed_errors_without_double_counting() {
        let mut state = State::default();
        state.observe(observation(10, 100, 2), 100);
        state.observe(observation(10, 100, 5), 2100);
        state.observe(observation(10, 100, 5), 4100);
        assert_eq!(state.snapshot.client_xruns, Some(5));
        state.observe(observation(10, 100, 1), 6100); // server-side explicit counter reset
        assert_eq!(state.snapshot.client_xruns, Some(6));
        assert_eq!(state.snapshot.nodes[0].counter_resets, 1);
        state.observe(observation(10, 200, 3), 8100); // node ID recycled for a reopened stream
        assert_eq!(state.snapshot.client_xruns, Some(9));
        assert!(!state.snapshot.nodes[0].present);
        assert!(state.snapshot.nodes[1].present);
    }

    #[test]
    fn only_last_complete_top_frame_counts() {
        let text = format!("{HEADER}R 10 0 0 1us 2us 0.1 0.1 99 S16LE 2 48000 + game\n{HEADER}R 10 0 0 1us 2us 0.1 0.1 4 S16LE 2 48000 + game\n");
        assert_eq!(follower_counts(&text, &[identity(10, 100)]).unwrap()[0].1, 4);
        assert!(follower_counts(&(text + HEADER), &[identity(10, 100)]).is_err());
    }

    #[test]
    fn node_detail_is_bounded_but_retired_error_totals_survive() {
        let mut state = State::default();
        for n in 0..100 {
            state.observe(observation(10, n, 1), n * 2000);
        }
        assert_eq!(state.snapshot.nodes.len(), MAX_NODES);
        assert_eq!(state.snapshot.retired_nodes, 36);
        assert_eq!(state.snapshot.client_xruns, Some(100));
    }
}
