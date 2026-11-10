// Isolated-process fixtures: WANT_HELD and APPLYING must not leak into unrelated sync tests.
use std::process::Command;

struct Service {
    root: PathBuf,
    cut_after_pause: bool,
    refuse_resume: bool,
}

#[cfg(test)]
impl Api for Service {
    fn running(&mut self) -> bool { true }
    fn scan(&mut self, _: &ScanReq) -> Result<(), String> { unreachable!("no scan in ownership fixture") }
    fn peers(&mut self) -> Result<Peers, String> { unreachable!("no peer query in ownership fixture") }
    fn outstanding(&mut self, _: &[String]) -> Result<u64, String> { unreachable!("no transfer in ownership fixture") }
    fn add_folder(&mut self, _: &'static Dir) -> Result<(), String> { unreachable!("no folder creation in ownership fixture") }
    fn set_paused(&mut self, folder: &str, paused: bool) -> Result<(), String> {
        assert_eq!(folder, "pocket-saves", "a manually paused folder must never be changed");
        if !paused && std::mem::take(&mut self.refuse_resume) {
            return Err("service temporarily refused resume".into());
        }
        std::fs::write(self.root.join("service-paused"), if paused { "yes" } else { "no" }).unwrap();
        if paused && self.cut_after_pause {
            // Syncthing has durably accepted the operation, but the client's call has not returned.
            // SAFETY: only the test's own isolated process is terminated.
            unsafe { libc::raise(libc::SIGKILL) };
            unreachable!();
        }
        Ok(())
    }
}

fn child(mode: &str, root: &Path) -> std::process::Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sync::tests::ownership::ownership_child", "--ignored", "--nocapture"])
        .env("POCKETUI_SYNC_OWNERSHIP_FIXTURE", root)
        .env("POCKETUI_SYNC_OWNERSHIP_MODE", mode)
        .output().unwrap()
}

#[test]
#[ignore = "isolated child of sync ownership tests; no device files are read"]
fn ownership_child() {
    let Some(root) = std::env::var_os("POCKETUI_SYNC_OWNERSHIP_FIXTURE").map(PathBuf::from) else { return };
    let mode = std::env::var("POCKETUI_SYNC_OWNERSHIP_MODE").unwrap();
    let held = root.join("held");
    let prefs = Prefs { states: false, ..on() };
    let mut config = with_folders(&[("pocket-saves", false), ("pocket-states", true)]);
    let mut api = Service { root: root.clone(), cut_after_pause: mode == "cut", refuse_resume: mode.starts_with("resume-") };
    want_held(&prefs, Some("gba"));
    if mode == "marker-refused" { std::fs::create_dir(&held).unwrap(); }
    let paused = apply_holds(&mut api, &mut config, &held);
    if mode == "marker-refused" {
        assert!(paused.is_err());
        assert_eq!(std::fs::read_to_string(root.join("service-paused")).unwrap(), "yes");
        assert!(!holds_left(&held), "a refused ownership marker leaves no recoverable hold");
        return;
    }
    paused.unwrap();
    assert_eq!(std::fs::read_to_string(&held).unwrap(), "pocket-saves\n", "unchanged legacy format");
    want_held(&prefs, None);
    if mode.starts_with("resume-") {
        let mut attempts = 0;
        let mut waits = 0;
        recover_leftover_holds(|| {
            attempts += 1;
            assert!(attempts <= 2, "the second resume succeeds; no endless retry");
            assert_eq!(std::fs::read_to_string(&held).unwrap(), "pocket-saves\n", "ownership survives a failed resume");
            Some(apply_holds(&mut api, &mut config, &held))
        }, || {
            waits += 1;
            if mode == "resume-new-game" { want_held(&prefs, Some("gba")); }
        });
        std::fs::write(root.join("attempts"), attempts.to_string()).unwrap();
        std::fs::write(root.join("waits"), waits.to_string()).unwrap();
    } else {
        apply_holds(&mut api, &mut config, &held).unwrap();
    }
    assert!(config.folder("pocket-states").unwrap().paused, "manual pause stays untouched");
}

#[test]
fn legacy_owned_hold_resumes_without_touching_a_manual_pause() {
    let root = TestDir::new("sync-owned-legacy");
    let out = child("normal", &root);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(root.join("service-paused")).unwrap(), "no");
    assert!(!holds_left(&root.join("held")));
}

#[test]
fn refused_ownership_write_reports_error_but_service_has_already_paused() {
    let root = TestDir::new("sync-owned-marker-refused");
    let out = child("marker-refused", &root);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

#[test]
#[ignore = "known ownership ambiguity: recovery design decision required; docs/SYNC-OWNERSHIP.md"]
fn a_crash_after_service_pause_still_leaves_ownership_for_restart() {
    use std::os::unix::process::ExitStatusExt;
    let root = TestDir::new("sync-owned-cut");
    let out = child("cut", &root);
    assert_eq!(out.status.signal(), Some(libc::SIGKILL), "fixture reached API success boundary");
    assert_eq!(std::fs::read_to_string(root.join("service-paused")).unwrap(), "yes");
    let config = with_folders(&[("pocket-saves", true), ("pocket-states", true)]);
    let plan = hold_plan(&[], &read_held(&root.join("held")), &config);
    assert_eq!(plan.resume, ["pocket-saves"], "restart must resume our pause, preserving the manual one");
}

#[test]
fn startup_recovery_retries_a_refused_resume_without_another_restart() {
    let root = TestDir::new("sync-owned-resume-refused");
    let out = child("resume-fails", &root);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(root.join("service-paused")).unwrap(), "no", "temporary failure must not leave saves paused forever");
    assert!(!holds_left(&root.join("held")));
    assert_eq!(std::fs::read_to_string(root.join("attempts")).unwrap(), "2");
    assert_eq!(std::fs::read_to_string(root.join("waits")).unwrap(), "1", "rediscovery waits instead of hammering the service");
}

#[test]
fn startup_recovery_waits_for_discovery_and_finishes_once_ready() {
    let mut attempts = 0;
    let mut waits = 0;
    recover_leftover_holds(|| {
        attempts += 1;
        if attempts < 3 { None } else { Some(Ok(())) }
    }, || waits += 1);
    assert_eq!((attempts, waits), (3, 2), "emulator/config/service discovery still gets its normal wait");
}

#[test]
fn a_game_started_between_resume_attempts_keeps_its_save_folder_paused() {
    let root = TestDir::new("sync-owned-new-game");
    let out = child("resume-new-game", &root);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(root.join("service-paused")).unwrap(), "yes");
    assert_eq!(std::fs::read_to_string(root.join("held")).unwrap(), "pocket-saves\n");
    assert_eq!(std::fs::read_to_string(root.join("attempts")).unwrap(), "2", "the retry uses the new game wish");
}
