//! An app install cut off at every step (`crash_point`: SIGKILL, as a power cut or `kill -9` ends it), then the real
//! launcher (deploy/run-pocket-ui.sh) starting pocket-ui from what is left on the card. The player always gets a
//! version that shows its picture, the old one or the new one, never EmulationStation; a new version that crashes on
//! start is taken back; and the next try of the install finishes.
use super::*;
use crate::testutil::{write_executable, TestDir};
use std::os::unix::process::{CommandExt, ExitStatusExt};

/// `install_app`'s steps in order (its `crash_point`s); "done" lets it finish.
const STEPS: [&str; 9] = ["downloaded", "unpacked", "synced", "renamed", "previous", "pending", "current", "linked", "done"];
/// From this step on `current` names the new version.
const SWITCHED: usize = 6;

const OLD: &str = "1.0.0";
const NEW: &str = "2.0.0";

/// The programs of `version` in `dir`: both answer `--version`; pocket-ui run for real notes `<version> <args>` in
/// `runs`, then shows its picture and asks for EmulationStation (77, which ends the launcher), or with `crashes` dies
/// before its picture as a release that crashes on start does.
fn programs(dir: &Path, version: &str, crashes: bool, runs: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let crash = if crashes { "exit 139\n" } else { "" };
    write_executable(
        &dir.join("pocket-ui"),
        format!(
            "#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocket-ui {version}' && exit 0\n\
             echo \"{version} $*\" >>'{runs}'\n{crash}: >\"$POCKETUI_FRAME_FILE\"\nexit 77\n",
            runs = runs.display()
        ),
    );
    write_executable(&dir.join("pocket-core"), format!("#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocket-core {version}'\nexit 0\n"));
}

/// The new version as the channel offers it: a tarball next to `update.json` in `dir`.
fn offered(dir: &Path, crashes: bool, runs: &Path) -> (Release, String) {
    let src = dir.join("src");
    programs(&src, NEW, crashes, runs);
    let tar = dir.join("app.tar.gz");
    assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).args(["pocket-ui", "pocket-core"]).status().unwrap().success());
    let data = std::fs::read(&tar).unwrap();
    let rel = Release { version: NEW.into(), url: "app.tar.gz".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: store::sha256_hex(&data), size: data.len() as u64, notes: String::new() };
    (rel, format!("file://{}/update.json", dir.display()))
}

/// A card that runs the old version, which has been the known good one for a while.
fn card(root: &Path, runs: &Path) {
    programs(&root.join("releases").join(OLD), OLD, false, runs);
    for (link_name, target) in [("current", format!("releases/{OLD}")), ("known-good", format!("releases/{OLD}")), ("pocket-ui", "current/pocket-ui".into()), ("pocket-core", "current/pocket-core".into())] {
        std::os::unix::fs::symlink(target, root.join(link_name)).unwrap();
    }
}

/// `install_app` in a process of its own, ended at `step` with SIGKILL.
fn install_cut_off_at(step: &str, root: &Path, rel: &Release, url: &str) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "update::crash_tests::install_child", "--ignored", "--nocapture", "--test-threads", "1"])
        .env("POCKETUI_TEST_CRASH_AT", step)
        .env("POCKETUI_TEST_CRASH_SPEC", serde_json::to_string(&(root, url, rel)).unwrap())
        .output()
        .unwrap();
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if step == "done" {
        assert!(out.status.success() && said.contains("1 passed"), "the install finishes: {said}");
    } else {
        assert_eq!(out.status.signal(), Some(libc::SIGKILL), "cut off at {step}: {said}");
    }
}

/// The child of `install_cut_off_at`: the install, nothing else.
#[test]
#[ignore = "runs only as the child of the crash tests (install_cut_off_at)"]
fn install_child() {
    let Ok(spec) = std::env::var("POCKETUI_TEST_CRASH_SPEC") else { return };
    let (root, url, rel): (PathBuf, String, Release) = serde_json::from_str(&spec).unwrap();
    install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).unwrap();
}

/// The launcher on `root` as the device starts it after a power cut, with stand-ins for sway, systemd and ROCKNIX's files
/// in `t`. Returns once it handed over to EmulationStation (the last run asked for it, 77) or failed.
fn boot(t: &Path, root: &Path) {
    let bin = t.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    write_executable(&bin.join("systemctl"), "#!/bin/sh\ncase \"$*\" in *is-active*) exit 1 ;; esac\nexit 0\n");
    for tool in ["swaymsg", "quantum", "start_es.sh"] {
        write_executable(&bin.join(tool), "#!/bin/sh\nexit 0\n");
    }
    // As deploy/test-run-pocket-ui.sh: bash where /bin/sh is dash.
    let sh = if std::fs::read_link("/bin/sh").is_ok_and(|l| l.ends_with("dash")) && Path::new("/bin/bash").exists() { "/bin/bash" } else { "/bin/sh" };
    let f = |name: &str| t.join(name).into_os_string();
    let mut child = Command::new(sh)
        .arg(crate::testutil::manifest_dir().join("deploy/run-pocket-ui.sh"))
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("HOME", t)
        .env("POCKETUI_ROOT", root)
        .env("POCKETUI_LOG", f("log"))
        .env("POCKETUI_BACKLIGHT", f("backlight"))
        .env("POCKETUI_DARK_FILE", f("dark"))
        .env("POCKETUI_GAME_FILE", f("game"))
        .env("POCKETUI_QUANTUM", bin.join("quantum"))
        .env("POCKETUI_RESTART_DELAY", "0")
        .env("POCKETUI_SWAY_WAIT", "5")
        .env("POCKETUI_START_ES", bin.join("start_es.sh"))
        .env("POCKETUI_ES_ONLY_FILE", f("es-only"))
        .env("POCKETUI_FRAME_FILE", f("frame"))
        .env("POCKETUI_UI_PID_FILE", f("ui.pid"))
        .env("POCKETUI_ROCKNIX_PROFILE", f("none"))
        .env("POCKETUI_ROCKNIX_FUNCTIONS", f("none"))
        .env("POCKETUI_ES_SETTINGS", f("none"))
        .env("POCKETUI_SYSTEM_CFG", f("none"))
        .env("POCKETUI_TZ_CACHE", f("cache"))
        .env("POCKETUI_LOCALE_DIR", f("locale"))
        .env("POCKETUI_FAN_ROOT", f("fansys"))
        .env("POCKETUI_FAN_NOTE", f("fan.restore"))
        .env("POCKETUI_FAN_BEAT", f("fan.beat"))
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + crate::testutil::timeout(Duration::from_secs(60));
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() > deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // The launcher's helpers in the background (the black screen guard, the fan watch) go with its process group.
    // SAFETY: a signal to the process group this test made for the launcher.
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let _ = child.wait();
    let log = std::fs::read_to_string(t.join("log")).unwrap_or_default();
    assert!(status.is_some_and(|s| s.success()), "the launcher ended by itself: {status:?}\n{log}");
}

fn runs(file: &Path) -> Vec<String> {
    std::fs::read_to_string(file).unwrap_or_default().lines().map(|l| l.trim().to_string()).collect()
}

#[test]
fn every_step_of_an_install_can_be_cut_off_and_the_player_still_gets_a_working_version() {
    for crashes in [false, true] {
        for (i, step) in STEPS.iter().enumerate() {
            let what = format!("cut off at {step}, the new version {}", if crashes { "crashes on start" } else { "works" });
            let t = TestDir::new("update-crash");
            let (root, channel, runs_file) = (t.join("root"), t.join("channel"), t.join("runs"));
            std::fs::create_dir_all(&root).unwrap();
            card(&root, &runs_file);
            let (rel, url) = offered(&channel, crashes, &runs_file);

            install_cut_off_at(step, &root, &rel, &url);
            boot(&t, &root);

            let ran = runs(&runs_file);
            let back = std::fs::read_to_string(root.join("rolled-back")).ok();
            let expected: Vec<String> = match (i >= SWITCHED, crashes) {
                (false, _) => vec![OLD.into()],
                (true, false) => vec![NEW.into()],
                (true, true) => vec![NEW.into(), OLD.into()],
            };
            assert_eq!(ran, expected, "{what}: runs");
            assert!(!t.join("root/es-fallback").exists(), "{what}: never EmulationStation by itself");
            if i >= SWITCHED && crashes {
                assert_eq!(back.as_deref().map(str::trim), Some(NEW), "{what}: the player is told");
                assert_eq!(link_target(&root.join("current")).as_deref(), Some(format!("releases/{OLD}").as_str()), "{what}");
            } else {
                assert_eq!(back, None, "{what}: nothing to tell");
            }

            // The player tries again (the old version still offers the new one): it finishes from what is there.
            if i < SWITCHED {
                install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).unwrap_or_else(|e| panic!("{what}: the next try: {e}"));
                let version = Command::new(root.join("pocket-ui")).arg("--version").output().unwrap();
                assert!(version.status.success(), "{what}: the retried install starts");
                assert_eq!(String::from_utf8_lossy(&version.stdout).trim(), format!("pocket-ui {NEW}"), "{what}");
                assert_eq!(link_target(&root.join("previous")).as_deref(), Some(format!("releases/{OLD}").as_str()), "{what}: the way back");
                assert_eq!(std::fs::read_to_string(root.join("update-pending")).unwrap(), NEW, "{what}");
                let hidden: Vec<_> = std::fs::read_dir(root.join("releases")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with('.')).map(|e| e.file_name()).collect();
                assert!(hidden.is_empty(), "{what}: left behind {hidden:?}");
            }
        }
    }
}

#[test]
fn reinstall_cut_off_at_any_step_preserves_the_active_and_rollback_programs() {
    for active in [false, true] {
        for step in STEPS {
            let t = TestDir::new("update-reinstall-crash");
            let (root, channel, runs_file) = (t.join("root"), t.join("channel"), t.join("runs"));
            std::fs::create_dir_all(&root).unwrap();
            card(&root, &runs_file);
            // Another working build already has the incoming version's folder name.
            // It is either the active build or the last working rollback target.
            let existing = root.join("releases").join(NEW);
            programs(&existing, NEW, false, &runs_file);
            link(&format!("releases/{NEW}"), &root.join("known-good")).unwrap();
            if active {
                link(&format!("releases/{NEW}"), &root.join("current")).unwrap();
            }
            let before = std::fs::read(existing.join("pocket-ui")).unwrap();
            let (rel, url) = offered(&channel, true, &runs_file);
            install_cut_off_at(step, &root, &rel, &url);
            assert_eq!(std::fs::read(existing.join("pocket-ui")).unwrap(), before,
                "{step}: a reinstall must not overwrite a version the launcher can still need (active={active})");
            boot(&t, &root);
            let ran = runs(&runs_file);
            assert!(!ran.is_empty(), "{step}: the player gets a working version");
            assert!(!root.join("es-fallback").exists(), "{step}: a working previous version is available");
            assert_eq!(ran.last().map(String::as_str), Some(if active { NEW } else { OLD }));
        }
    }
}

#[test]
fn converting_a_flat_install_never_removes_its_boot_path() {
    for step in ["layout-copied-pocket-ui", "layout-copied-pocket-core", "layout-synced", "layout-current", "layout-linked-pocket-ui", "layout-linked-pocket-core"] {
        let t = TestDir::new("update-flat-crash");
        let (root, channel, runs_file) = (t.join("root"), t.join("channel"), t.join("runs"));
        programs(&root, OLD, false, &runs_file);
        let (rel, url) = offered(&channel, false, &runs_file);
        install_cut_off_at(step, &root, &rel, &url);
        for bin in ["pocket-ui", "pocket-core"] {
            let out = Command::new(root.join(bin)).arg("--version").output().unwrap();
            assert!(out.status.success(), "{step}: the original {bin} still starts");
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("{bin} {OLD}"));
        }
        boot(&t, &root);
        assert_eq!(runs(&runs_file), [OLD], "{step}: the launcher still shows the old interface");
    }
}
