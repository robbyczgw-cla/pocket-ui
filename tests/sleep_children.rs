//! Exercise the product's logind child-spawn path across teardown and abrupt exits.
#![cfg(target_os = "linux")]
#[allow(unused_imports)]
#[path = "../src/sleep_child.rs"]
mod sleep_child;

use std::{fs, path::PathBuf, process::Command, time::{Duration, Instant}};
use std::os::{fd::AsRawFd, unix::fs::PermissionsExt};

fn until(mut ready: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if ready() { return true; }
        std::thread::sleep(Duration::from_millis(10));
    }
    ready()
}
fn running(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| s.rsplit_once(") ").is_some_and(|(_, fields)| !fields.starts_with('Z')))
}
fn owner(root: PathBuf, mode: String) {
    let lock = fs::File::create(root.join("heavy.lock")).unwrap();
    // Reproduce a shell/heavy descriptor deliberately left open across exec.
    unsafe {
        assert_eq!(libc::fcntl(lock.as_raw_fd(), libc::F_SETFD, 0), 0);
        assert_eq!(libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB), 0);
    }
    std::thread::spawn(move || {
        let child = sleep_child::monitor(false).unwrap();
        fs::write(root.join("pid.next"), child.id().to_string()).unwrap();
        fs::rename(root.join("pid.next"), root.join("pid")).unwrap();
        assert!(until(|| root.join("release").exists()));
        if mode == "drop" { drop(child); }
        else {
            std::mem::forget(child);
            if mode == "exit" { std::process::exit(0); }
        }
    }).join().unwrap();
    // For the thread-death case the process itself stays alive.
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
fn sleep_watchers_release_lock_fds_and_end_on_drop_thread_exit_and_parent_death() {
    if let Some(root) = std::env::var_os("POCKETUI_SLEEP_CHILD_OWNER") {
        owner(root.into(), std::env::var("POCKETUI_SLEEP_CHILD_MODE").unwrap());
        return;
    }
    // Reap our own adopted monitors even in the deliberately broken/red version.
    unsafe { assert_eq!(libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1), 0); }
    let temp = tempfile::tempdir().unwrap();
    let mut errors = Vec::new();
    for mode in ["drop", "thread", "exit", "kill"] {
        let root = temp.path().join(mode);
        fs::create_dir(&root).unwrap();
        let program = root.join("dbus-monitor");
        fs::write(&program, "#!/bin/sh\nexec sleep 60\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "sleep_watchers_release_lock_fds_and_end_on_drop_thread_exit_and_parent_death", "--nocapture"])
            .env("POCKETUI_SLEEP_CHILD_OWNER", &root).env("POCKETUI_SLEEP_CHILD_MODE", mode)
            .env("PATH", format!("{}:/usr/bin:/bin", root.display())).env_remove("FLATPAK_ID")
            .spawn().unwrap();
        assert!(until(|| root.join("pid").exists()), "{mode}: monitor did not start");
        let pid: i32 = fs::read_to_string(root.join("pid")).unwrap().parse().unwrap();
        let lock_path = root.join("heavy.lock");
        let inherited = fs::read_dir(format!("/proc/{pid}/fd")).unwrap().any(|fd| {
            fd.ok().and_then(|fd| fs::read_link(fd.path()).ok()).as_deref() == Some(lock_path.as_path())
        });
        if inherited { errors.push(format!("{mode}: monitor inherited the heavy lock fd")); }
        if mode == "kill" { owner.kill().unwrap(); owner.wait().unwrap(); }
        else { fs::write(root.join("release"), "go").unwrap(); }
        if !until(|| !running(pid)) {
            errors.push(format!("{mode}: monitor survived its owner"));
            // This PID was returned by our own spawn, and is still that live child.
            unsafe { libc::kill(pid, libc::SIGKILL); }
        }
        if owner.try_wait().unwrap().is_none() { owner.kill().unwrap(); }
        owner.wait().unwrap();
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0); }
        let lock = fs::File::open(&lock_path).unwrap();
        assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0, "{mode}: heavy slot stays blocked");
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn the_power_key_inhibitor_stays_held_after_its_startup_thread_hands_it_to_the_ui() {
    use std::{io::{BufRead, BufReader, Write}, process::Stdio};
    let mut child = std::thread::spawn(|| {
        sleep_child::pipe_inhibitor(Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())).unwrap()
    }).join().unwrap();
    // A completed round-trip proves the inhibitor is alive after the handoff,
    // without relying on a sampled scheduling delay.
    let sent = child.stdin.as_mut().unwrap().write_all(b"still held\n");
    let mut echoed = String::new();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let received = output.read_line(&mut echoed);
    drop(child.stdin.take());
    let status = child.wait().unwrap();
    drop(output);
    assert!(sent.is_ok() && received.is_ok() && echoed == "still held\n" && status.success(),
            "the startup thread's exit must not release the power-key inhibitor: sent={sent:?}, received={received:?}, echo={echoed:?}, status={status}");
}
