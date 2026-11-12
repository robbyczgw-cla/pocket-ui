//! The saves' copies on their worker (src/save_copy.rs decides; `PocketActivity.copy*` moves the bytes into the folder
//! the player chose). One job at a time, never on the UI thread. Never while a game plays: the jobs start from the notes
//! a game leaves when it ends, from the start, and from Settings → Saves.
use super::bridge;
use pocket_ui::save_copy::{self, Report, Store, Trouble};
use pocket_ui::save_transfer::Known;
use std::path::Path;
use std::sync::mpsc;

/// What to do: copy the games in `keys` (None: all that have files), look for copies to bring back.
pub struct Job {
    pub keys: Option<Vec<String>>,
    pub copy: bool,
    pub restore: bool,
}

pub struct Done {
    pub report: Report,
    /// The notes that were taken into this job (removed when the folder worked).
    pub noted: Vec<String>,
    pub copies: super::ui::saves_page::Copies,
    pub seconds: f32,
}

pub struct Run {
    result: mpsc::Receiver<Done>,
}
impl Run {
    pub fn start(known: Vec<Known>, job: Job, noted: Vec<String>) -> Self {
        let (tx, result) = mpsc::channel();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let data = pocket_ui::platform::data_dir();
            let (report, copies) = work(&data, &known, &job);
            let _ = tx.send(Done { report, noted, copies, seconds: started.elapsed().as_secs_f32() });
        });
        Self { result }
    }
    pub fn poll(&self) -> Option<Done> {
        match self.result.try_recv() {
            Ok(done) => Some(done),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Done { report: Report { trouble: Some(Trouble::Failed("the copy stopped".into())), ..Default::default() }, noted: Vec::new(), copies: super::ui::saves_page::Copies::Unknown, seconds: 0.0 }),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub uri: String,
    pub state: String,
}

/// The chosen folders and what pocket-ui may do in each.
pub fn targets() -> Vec<Target> {
    let listed: serde_json::Value = serde_json::from_str(&bridge("copyTargets", &[])).unwrap_or_default();
    listed
        .as_array()
        .map(|a| a.iter().filter_map(|t| Some(Target { uri: t["uri"].as_str()?.to_string(), state: t["state"].as_str()?.to_string() })).collect())
        .unwrap_or_default()
}

/// The page's state from what the folders allow.
pub fn copies_of(targets: &[Target]) -> super::ui::saves_page::Copies {
    use super::ui::saves_page::Copies;
    if targets.iter().any(|t| t.state == "ok") {
        Copies::On
    } else if targets.iter().any(|t| t.state == "gone") {
        Copies::Gone
    } else {
        Copies::Off
    }
}

fn work(data: &Path, known: &[Known], job: &Job) -> (Report, super::ui::saves_page::Copies) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let targets = targets();
    let copies = copies_of(&targets);
    let mut report = Report::default();
    if job.restore {
        for target in targets.iter().filter(|t| t.state == "ok" || t.state == "read-only") {
            let store = AndroidStore { root: target.uri.clone() };
            let found = save_copy::restore(data, known, &store, now);
            // A folder with no copies is no trouble; one that can't be read is said once.
            report.merge(found);
        }
    }
    if job.copy {
        match targets.iter().find(|t| t.state == "ok") {
            Some(target) => report.merge(save_copy::copy_games(data, known, job.keys.as_deref(), &AndroidStore { root: target.uri.clone() }, now)),
            None => {
                report.trouble.get_or_insert(if copies == super::ui::saves_page::Copies::Gone { Trouble::Gone } else { Trouble::NoAccess });
            }
        }
    }
    (report, copies)
}

/// The folder as Java gives it: every call answers "ok[\n…]", "no-access", "gone" or "error\n…".
pub struct AndroidStore {
    pub root: String,
}
impl AndroidStore {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, Trouble> {
        let mut all = vec![self.root.as_str()];
        all.extend_from_slice(args);
        let answer = bridge(name, &all);
        let (state, rest) = answer.split_once('\n').unwrap_or((answer.as_str(), ""));
        match state {
            "ok" => Ok(rest.to_string()),
            "no-access" => Err(Trouble::NoAccess),
            "gone" => Err(Trouble::Gone),
            "error" => Err(Trouble::Failed(rest.to_string())),
            _ => Err(Trouble::Failed(format!("no answer from Android to {name}"))),
        }
    }
}
impl Store for AndroidStore {
    fn games(&self) -> Result<Vec<String>, Trouble> {
        let listed: Vec<String> = serde_json::from_str(&self.call("copyGames", &[])?).map_err(|e| Trouble::Failed(e.to_string()))?;
        Ok(listed)
    }
    fn manifest(&self, dir: &str) -> Result<Option<String>, Trouble> {
        let text = self.call("copyManifest", &[dir])?;
        Ok((!text.is_empty()).then_some(text))
    }
    fn put(&self, dir: &str, rel: &str, from: &Path) -> Result<(), Trouble> {
        self.call("copyPut", &[dir, rel, &from.to_string_lossy()]).map(|_| ())
    }
    fn put_manifest(&self, dir: &str, text: &str) -> Result<(), Trouble> {
        self.call("copyPutManifest", &[dir, text]).map(|_| ())
    }
    fn get(&self, dir: &str, rel: &str, to: &Path) -> Result<(), Trouble> {
        self.call("copyGet", &[dir, rel, &to.to_string_lossy()]).map(|_| ())
    }
}
