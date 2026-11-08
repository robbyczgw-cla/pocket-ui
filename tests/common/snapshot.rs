//! Shared static layout probes, never interactive journeys or timing checks.
//! Enabled only by the gate's fresh run directory. Fixture contents, arguments,
//! binary SHA256 and rendering environment form the key. Random fixture roots
//! become one canonical layout fixture; names/content inside it stay unchanged.
#![allow(dead_code)]
use std::{collections::BTreeMap, fs, io, path::{Path, PathBuf}, process::{Command, Output}, sync::{Mutex, OnceLock}};
use sha2::{Digest, Sha256};

const RECORDS: &[(&str, &str)] = &[
    ("POCKETUI_TEXT_BOXES", "boxes"), ("POCKETUI_I18N_MISSES", "misses"),
    ("POCKETUI_VISIBLE_TEXT", "visible"), ("POCKETUI_NAV_PROBE", "nav"),
    ("POCKETUI_TITLE_BLOCKS", "titles"), ("POCKETUI_SPINE_LINES", "spines"),
];
const ROOT_TOKEN: &str = "@snapshot-fixture@";
const DATA_TOKEN: &str = "@snapshot-data@";
type Files = BTreeMap<String, Option<Vec<u8>>>;

pub trait SnapshotCommand {
    fn snapshot_output(&mut self) -> io::Result<Output>;
}
impl SnapshotCommand for Command {
    fn snapshot_output(&mut self) -> io::Result<Output> {
        #[cfg(unix)]
        if let Some(plan) = Plan::of(self) { return plan.run(self); }
        if let Some(dir) = std::env::var_os("POCKETUI_SNAPSHOT_CACHE_DIR").filter(|_| self.get_args().any(|a| a == "--snapshot")) {
            use std::io::Write;
            fs::create_dir_all(&dir)?;
            fs::OpenOptions::new().create(true).append(true).open(Path::new(&dir).join("stats.jsonl"))?.write_all(b"{\"bypass\":true}\n")?;
        }
        self.output()
    }
}
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn normalize(bytes: &[u8], root: &str, data: &str) -> Vec<u8> {
    match std::str::from_utf8(bytes) {
        Ok(s) if s == data => DATA_TOKEN.as_bytes().to_vec(),
        Ok(s) if s == root => ROOT_TOKEN.as_bytes().to_vec(),
        Ok(s) => s.replace(&(data.to_string() + "/"), &(DATA_TOKEN.to_string() + "/"))
            .replace(&(data.to_string() + "\""), &(DATA_TOKEN.to_string() + "\""))
            .replace(&(root.to_string() + "/"), &(ROOT_TOKEN.to_string() + "/"))
            .replace(&(root.to_string() + "\""), &(ROOT_TOKEN.to_string() + "\""))
            .into_bytes(),
        Err(_) => bytes.to_vec(),
    }
}
fn materialize(bytes: &[u8], root: &Path, data: &Path) -> Vec<u8> {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.replace(DATA_TOKEN, &data.to_string_lossy()).replace(ROOT_TOKEN, &root.to_string_lossy()).into_bytes(),
        Err(_) => bytes.to_vec(),
    }
}
fn collect(path: &Path, base: &Path, files: &mut Files, root: &str, data: &str) -> Option<()> {
    let rel = path.strip_prefix(base).ok()?.to_str()?.to_string();
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.is_symlink() { return None; }
    if meta.is_dir() {
        files.insert(rel, None);
        for entry in fs::read_dir(path).ok()? { collect(&entry.ok()?.path(), base, files, root, data)?; }
    } else if meta.is_file() && meta.len() <= 32 * 1024 * 1024 {
        let bytes = fs::read(path).ok()?;
        if std::str::from_utf8(&bytes).is_ok_and(|s| s.contains(ROOT_TOKEN) || s.contains(DATA_TOKEN)) { return None; }
        files.insert(rel, Some(normalize(&bytes, root, data)));
    } else { return None; }
    if files.len() > 4096 { return None; }
    Some(())
}
fn write_files(base: &Path, files: &Files, fixture: &Path, data: &Path, now: u64) -> io::Result<()> {
    for (name, bytes) in files {
        let path = base.join(name);
        if let Some(bytes) = bytes {
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, materialize(bytes, fixture, data))?;
            // These are static layout fixtures. Their clock is already explicit;
            // don't introduce the test runner's copy time as another visual input.
            let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(now);
            fs::File::options().write(true).open(path)?.set_times(fs::FileTimes::new().set_modified(at))?;
        } else { fs::create_dir_all(path)?; }
    }
    Ok(())
}
fn original_cwd(cmd: &Command) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    Some(cmd.get_current_dir().map_or(cwd.clone(), |p| cwd.join(p)))
}
fn file_hash(path: &Path) -> Option<String> {
    type Fingerprints = BTreeMap<PathBuf, (u64, std::time::SystemTime, (i64, i64, u64), String)>;
    static HASHES: OnceLock<Mutex<Fingerprints>> = OnceLock::new();
    let meta = fs::metadata(path).ok()?;
    let changed = meta.modified().ok()?;
    #[cfg(unix)]
    let inode = { use std::os::unix::fs::MetadataExt; (meta.ctime(), meta.ctime_nsec(), meta.ino()) };
    #[cfg(not(unix))]
    let inode = (0, 0, 0);
    let mut hashes = HASHES.get_or_init(Default::default).lock().unwrap();
    if let Some((len, at, old_inode, hash)) = hashes.get(path) {
        if *len == meta.len() && *at == changed && *old_inode == inode { return Some(hash.clone()); }
    }
    let hash = digest(&fs::read(path).ok()?);
    hashes.insert(path.to_path_buf(), (meta.len(), changed, inode, hash.clone()));
    Some(hash)
}
#[cfg(unix)]
struct Lock(fs::File);
#[cfg(unix)]
impl Lock {
    fn new(path: &Path) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let file = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 { return Err(io::Error::last_os_error()); }
        Ok(Self(file))
    }
}
struct Plan {
    cache: PathBuf, key: String, fixture_key: String, fixture: Files, data: Files,
    env: BTreeMap<String, String>, args: Vec<String>, output: PathBuf,
    records: BTreeMap<String, PathBuf>, now: u64,
}
impl Plan {
    fn of(cmd: &Command) -> Option<Self> {
        let cache = PathBuf::from(std::env::var_os("POCKETUI_SNAPSHOT_CACHE_DIR")?);
        let mut env: BTreeMap<String, String> = std::env::vars_os().map(|(k, v)| Some((k.to_str()?.to_string(), v.to_str()?.to_string()))).collect::<Option<_>>()?;
        for (k, v) in cmd.get_envs() {
            let k = k.to_str()?;
            if let Some(v) = v { env.insert(k.into(), v.to_str()?.into()); } else { env.remove(k); }
        }
        if env.get("POCKETUI_MOCK")? != "1" { return None; }
        let now: u64 = env.get("POCKETUI_MOCK_NOW")?.parse().ok()?;
        let args: Vec<String> = cmd.get_args().map(|v| v.to_str().map(str::to_string)).collect::<Option<_>>()?;
        let at = args.iter().position(|v| v == "--snapshot")?;
        if args[..at].iter().any(|a| a != "--safe") || args.iter().filter(|a| *a == "--snapshot").count() != 1 || args.len() > at + 5 { return None; }
        let output = PathBuf::from(args.get(at + 2)?);
        let data_path = PathBuf::from(env.get("POCKETUI_DATA")?);
        if !data_path.is_dir() { return None; }
        let cfg = PathBuf::from(env.get("POCKETUI_ES_SYSTEMS")?);
        let root = cfg.parent()?;
        if !root.is_absolute() || !data_path.is_absolute() || root == Path::new("/") { return None; }
        let (root_s, data_s) = (root.to_str()?, data_path.to_str()?);
        // PATH helpers and special files are behavioral inputs, not static probes.
        if env.get("PATH").is_some_and(|p| p.contains(root_s)) { return None; }
        let xml = fs::read_to_string(&cfg).ok()?;
        let doc = roxmltree::Document::parse(&xml).ok()?;
        let mut fixture = Files::new();
        collect(&cfg, root, &mut fixture, root_s, data_s)?;
        for node in doc.descendants().filter(|n| n.has_tag_name("path")) {
            let path = PathBuf::from(node.text()?);
            if !path.starts_with(root) { return None; }
            // The XML itself records a missing directory; preserve that absence.
            if path.exists() { collect(&path, root, &mut fixture, root_s, data_s)?; }
        }
        for arg in args.iter().skip(at + 3) {
            let path = Path::new(arg);
            if path.is_file() {
                if !path.starts_with(root) { return None; }
                collect(path, root, &mut fixture, root_s, data_s)?;
            }
        }
        if fixture.values().flatten().any(|b| std::str::from_utf8(b).is_ok_and(|s| s.contains(DATA_TOKEN))) { return None; }
        let mut records = BTreeMap::new();
        for (key, name) in RECORDS {
            if let Some(path) = env.get(*key) { records.insert(name.to_string(), PathBuf::from(path)); }
        }
        let mut key_env = BTreeMap::new();
        for (key, value) in &env {
            // Recorder destinations and runner identities aren't visual inputs.
            // Everything else stays in the key, including unknown future knobs.
            if RECORDS.iter().any(|(k, _)| key.as_str() == *k)
                || key.starts_with("POCKETUI_SNAPSHOT_CACHE_") || key.starts_with("NEXTEST_")
                || key.starts_with("CARGO_") || matches!(key.as_str(), "WAYLAND_DISPLAY" | "DISPLAY" | "XDG_RUNTIME_DIR") { continue; }
            let path = Path::new(value);
            if key.starts_with("POCKETUI_") && path.exists() && key != "POCKETUI_DATA" && key != "POCKETUI_ES_SYSTEMS" {
                if path.starts_with(&data_path) { /* already fingerprinted below */ }
                else if path.starts_with(root) { collect(path, root, &mut fixture, root_s, data_s)?; }
                else if path.is_file() { key_env.insert(format!("{key}:file_sha256"), file_hash(path)?); }
                else { return None; }
            }
            key_env.insert(key.clone(), String::from_utf8(normalize(value.as_bytes(), root_s, data_s)).ok()?);
        }
        let mut data = Files::new();
        collect(&data_path, &data_path, &mut data, root_s, data_s)?;
        if fixture.values().flatten().any(|b| std::str::from_utf8(b).is_ok_and(|s| s.contains(DATA_TOKEN))) { return None; }
        for name in fixture.keys().cloned().collect::<Vec<_>>() {
            let mut path = Path::new(&name).parent();
            while let Some(p) = path.filter(|p| !p.as_os_str().is_empty()) {
                fixture.entry(p.to_str()?.into()).or_insert(None);
                path = p.parent();
            }
        }
        let fixture_key = digest(&serde_json::to_vec(&(&fixture, now)).ok()?);
        let mut key_args: Vec<String> = args.iter().map(|s| String::from_utf8(normalize(s.as_bytes(), root_s, data_s)).ok()).collect::<Option<_>>()?;
        key_args[at + 2] = "@output@".into();
        let cwd = original_cwd(cmd)?;
        let key = digest(&serde_json::to_vec(&(4, cwd, file_hash(Path::new(cmd.get_program()))?, &key_args, &key_env, &fixture_key, &data, records.keys().collect::<Vec<_>>())).ok()?);
        Some(Self { cache, key, fixture_key, fixture, data, env, args: key_args, output, records, now })
    }
    #[cfg(unix)]
    fn run(self, original: &Command) -> io::Result<Output> {
        use std::os::unix::process::ExitStatusExt;
        fs::create_dir_all(self.cache.join("fixtures"))?;
        fs::create_dir_all(self.cache.join("renders"))?;
        let fixture = self.cache.join("fixtures").join(&self.fixture_key);
        {
            let _lock = Lock::new(&self.cache.join("fixtures").join(format!("{}.lock", self.fixture_key)))?;
            if fixture.join(".complete").is_file() {
                let mut existing = Files::new();
                collect(&fixture, &fixture, &mut existing, fixture.to_str().unwrap(), "@unused-data-path@")
                    .ok_or_else(|| io::Error::other("snapshot fixture became unreadable"))?;
                existing.remove(""); existing.remove(".complete");
                if existing != self.fixture { return Err(io::Error::other("static snapshot modified a shared fixture input")); }
            } else {
                write_files(&fixture, &self.fixture, &fixture, Path::new("unused"), self.now)?;
                fs::write(fixture.join(".complete"), b"complete")?;
            }
        }
        let entry = self.cache.join("renders").join(&self.key);
        fs::create_dir_all(&entry)?;
        let _lock = Lock::new(&entry.join("lock"))?;
        let manifest = entry.join("complete.json");
        let disabled = self.env.contains_key("POCKETUI_SNAPSHOT_CACHE_DISABLE");
        if !disabled {
            if let Ok(bytes) = fs::read(&manifest) {
                if let Ok(saved) = serde_json::from_slice::<Saved>(&bytes) {
                    if saved.files.contains_key("png") && saved.files.iter().all(|(p, hash)| fs::read(entry.join(p)).is_ok_and(|b| digest(&b) == *hash)) {
                        self.restore(&entry, &saved)?;
                        self.note(true)?;
                        return Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: saved.stdout, stderr: saved.stderr });
                    }
                }
            }
        }
        let _ = fs::remove_file(&manifest);
        let data = entry.join("data");
        if data.exists() { fs::remove_dir_all(&data)?; }
        write_files(&data, &self.data, &fixture, &data, self.now)?;
        let expand = |s: &str| s.replace(DATA_TOKEN, &data.to_string_lossy()).replace(ROOT_TOKEN, &fixture.to_string_lossy());
        let args: Vec<String> = self.args.iter().map(|s| if s == "@output@" { entry.join("png").to_string_lossy().into_owned() } else { expand(s) }).collect();
        let mut cmd = Command::new(original.get_program());
        cmd.args(args).env_clear();
        let old_root = Path::new(self.env.get("POCKETUI_ES_SYSTEMS").unwrap()).parent().unwrap().to_str().unwrap();
        let old_data = self.env.get("POCKETUI_DATA").unwrap();
        for (key, value) in &self.env {
            let value = if key == "POCKETUI_DATA" { data.to_string_lossy().into_owned() }
                else if key == "POCKETUI_ES_SYSTEMS" { fixture.join(Path::new(value).file_name().unwrap()).to_string_lossy().into_owned() }
                else if key == "WAYLAND_DISPLAY" || key == "DISPLAY" || key == "PATH" || key == "XDG_RUNTIME_DIR" { value.clone() }
                else { expand(std::str::from_utf8(&normalize(value.as_bytes(), old_root, old_data)).unwrap()) };
            cmd.env(key, value);
        }
        if let Some(dir) = original.get_current_dir() { cmd.current_dir(dir); }
        for (key, name) in RECORDS {
            if self.records.contains_key(*name) {
                let path = entry.join(name); let _ = fs::remove_file(&path); cmd.env(key, path);
            }
        }
        let _ = fs::remove_file(entry.join("png"));
        let out = cmd.output()?;
        // A static picture may edit its private data copy, never the shared ROM
        // fixture. Fail visibly if a future snapshot gains such a side effect.
        let mut after = Files::new();
        collect(&fixture, &fixture, &mut after, fixture.to_str().unwrap(), "@unused-data-path@").ok_or_else(|| io::Error::other("snapshot fixture became unreadable"))?;
        after.remove(""); after.remove(".complete");
        if after != self.fixture { return Err(io::Error::other("static snapshot modified a shared fixture input")); }
        let mut files = BTreeMap::new();
        for name in std::iter::once("png").chain(self.records.keys().map(String::as_str)) {
            if let Ok(bytes) = fs::read(entry.join(name)) { files.insert(name.to_string(), digest(&bytes)); }
        }
        let saved = Saved { files, stdout: out.stdout.clone(), stderr: out.stderr.clone() };
        self.restore(&entry, &saved)?;
        if out.status.success() && saved.files.contains_key("png") {
            fs::write(entry.join("complete.part"), serde_json::to_vec(&saved)?)?;
            fs::rename(entry.join("complete.part"), manifest)?;
        }
        self.note(false)?;
        Ok(out)
    }
    fn restore(&self, entry: &Path, saved: &Saved) -> io::Result<()> {
        for (name, target) in std::iter::once(("png", &self.output)).chain(self.records.iter().map(|(n, p)| (n.as_str(), p))) {
            if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
            if saved.files.contains_key(name) { fs::copy(entry.join(name), target)?; }
            else { let _ = fs::remove_file(target); }
        }
        Ok(())
    }
    fn note(&self, hit: bool) -> io::Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().create(true).append(true).open(self.cache.join("stats.jsonl"))?;
        let mut line = serde_json::to_vec(&serde_json::json!({"key":self.key,"hit":hit}))?;
        line.push(b'\n');
        file.write_all(&line)
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Saved { files: BTreeMap<String, String>, stdout: Vec<u8>, stderr: Vec<u8> }
