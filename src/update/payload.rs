//! Optional card assets use the updater's checks, resume and Brick write budget.
use super::{Cancel, Release, Tools, WorkerPhase, TOOLS};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_PAYLOAD: u64 = 320 << 20;
static CLI_CANCEL: AtomicBool = AtomicBool::new(false);

/// A pinned optional runtime, checked before it is made available to its installer.
pub fn download(rel: &Release, destination: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    transfer(&TOOLS, rel, destination, cancel, progress)
}

fn transfer(tools: &Tools, rel: &Release, destination: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    if rel.size == 0 || rel.size > MAX_PAYLOAD || rel.sha256.len() != 64 || !rel.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid optional download description.".into());
    }
    if !super::scheme_allowed(tools.protocols, &rel.url) { return Err(super::NOT_HTTPS.into()); }
    if std::fs::metadata(destination).is_ok_and(|m| m.len() == rel.size)
        && crate::store::sha256_file(destination).is_ok_and(|hash| hash == rel.sha256.to_lowercase()) {
        progress(rel.size);
        return Ok(());
    }
    let parent = destination.parent().ok_or("Invalid download destination.")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if crate::starter::free_bytes(parent).is_some_and(|free| free < rel.size + (32 << 20)) {
        return Err("There isn't enough free space on the SD card for PortMaster.".into());
    }
    #[cfg(feature = "brick")]
    {
        let mut ram = super::brick_io::Ram::new(destination)?;
        ram.reserve(rel.size)?;
        let temp = ram.dir.join("payload");
        let saved = std::fs::metadata(destination).map_or(0, |m| m.len());
        let memory = std::fs::metadata(&temp).map_or(0, |m| m.len());
        if saved > rel.size { std::fs::remove_file(destination).map_err(|e| e.to_string())?; }
        else if saved > memory { std::fs::copy(destination, &temp).map_err(|e| e.to_string())?; }
        let result = super::download_checked(tools, rel, &rel.url, &temp, cancel, progress);
        // curl has exited, including on cancellation. Never write the card during Wi-Fi traffic.
        if temp.is_file() {
            cancel.set_phase(WorkerPhase::Preparing);
            let checkpoint = destination.with_extension("checkpoint");
            let _idle = super::brick_io::IdleIo::enter().map_err(|e| e.to_string())?;
            let mut source = std::fs::File::open(&temp).map_err(|e| e.to_string())?;
            let mut target = std::fs::File::create(&checkpoint).map_err(|e| e.to_string())?;
            super::brick_io::Pacer::new().copy(&mut source, &mut target).map_err(|e| e.to_string())?;
            target.sync_all().map_err(|e| e.to_string())?;
            std::fs::rename(&checkpoint, destination).map_err(|e| e.to_string())?;
            crate::fsutil::sync_dir(parent);
        } else if destination.exists() {
            // A full corrupt copy is discarded by download_checked; don't keep its old bad prefix.
            std::fs::remove_file(destination).map_err(|e| e.to_string())?;
        }
        result?;
        if std::fs::metadata(destination).map_or(0, |m| m.len()) != rel.size
            || crate::store::sha256_file(destination).map_err(|e| e.to_string())? != rel.sha256.to_lowercase() {
            let _ = std::fs::remove_file(destination);
            return Err(super::DAMAGED.into());
        }
        Ok(())
    }
    #[cfg(not(feature = "brick"))]
    super::download_checked(tools, rel, &rel.url, destination, cancel, progress)
}

extern "C" fn cancel_signal(_: libc::c_int) { CLI_CANCEL.store(true, Ordering::Relaxed); }

/// External card writers inherit this thread's idle I/O class across exec.
#[cfg(all(target_os = "linux", feature = "brick"))]
pub fn idle_command(args: &[std::ffi::OsString]) -> i32 {
    use std::os::unix::process::CommandExt;
    let Some((program, args)) = args.split_first() else { return 2; };
    let result = super::brick_io::IdleIo::enter().and_then(|_idle| {
        Err::<(), _>(std::process::Command::new(program).args(args).exec())
    });
    if let Err(error) = result { eprintln!("idle command: {error}"); }
    1
}

pub fn write_progress(path: &Path, phase: &str, bytes: u64, total: u64) {
    let temporary = path.with_extension("new");
    if std::fs::write(&temporary, format!("{phase}|{bytes}|{total}\n")).is_ok() {
        let _ = std::fs::rename(temporary, path);
    }
}

/// Early process entry point: `--download-asset URL SHA256 SIZE DEST PROGRESS`.
/// SIGTERM requests cancellation; returning waits until the last partial is durably checkpointed.
pub fn cli(args: &[String]) -> i32 {
    if args.len() != 5 { eprintln!("download-asset needs URL SHA256 SIZE DEST PROGRESS"); return 2; }
    let Ok(size) = args[2].parse() else { eprintln!("Invalid optional download size."); return 2; };
    if !args[0].starts_with("https://") { eprintln!("Optional downloads require HTTPS."); return 2; }
    let rel = Release { version: "optional".into(), url: args[0].clone(), mirrors: vec![], preferred_sources: vec![],
        sha256: args[1].clone(), size, notes: String::new() };
    let cancel = Cancel::default();
    let status = (!args[4].is_empty()).then(|| std::path::PathBuf::from(&args[4]));
    if let Some(path) = status.clone() {
        cancel.observe(move |snapshot| write_progress(&path, match snapshot.phase {
            WorkerPhase::CheckingDownload => "checking", WorkerPhase::Preparing => "saving", _ => "downloading",
        }, snapshot.bytes, rel.size));
    }
    CLI_CANCEL.store(false, Ordering::Relaxed);
    // SAFETY: a signal handler only sets a lock-free flag; all cleanup happens in normal worker code.
    let previous = unsafe {
        (libc::signal(libc::SIGTERM, cancel_signal as *const () as libc::sighandler_t),
         libc::signal(libc::SIGINT, cancel_signal as *const () as libc::sighandler_t))
    };
    let result = download(&rel, Path::new(&args[3]), &cancel, &|_| {
        if CLI_CANCEL.load(Ordering::Relaxed) { cancel.cancel(); }
    });
    // SAFETY: restore the process handlers installed immediately above.
    unsafe { libc::signal(libc::SIGTERM, previous.0); libc::signal(libc::SIGINT, previous.1); }
    if let Some(path) = status {
        write_progress(&path, if result.is_ok() { "ready" } else if cancel.is_cancelled() { "cancelled" } else { "error" },
                       if result.is_ok() { size } else { 0 }, size);
    }
    match result { Ok(()) => 0, Err(error) => { eprintln!("{error}"); 1 } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use crate::update::{Cancel, Release, Tools, TOOLS};

    fn fixture(dir: &std::path::Path) -> (Release, std::path::PathBuf) {
        let source = dir.join("source");
        std::fs::write(&source, b"0123456789").unwrap();
        (Release { version: "optional".into(), url: format!("file://{}", source.display()), mirrors: vec![],
            preferred_sources: vec![], sha256: crate::store::sha256_hex(b"0123456789"), size: 10, notes: String::new() }, dir.join("cache/asset"))
    }

    #[test]
    fn optional_payload_is_checked_and_existing_verified_cache_is_offline() {
        let dir = TestDir::new("payload-verified");
        let (rel, dest) = fixture(&dir);
        transfer(&TOOLS, &rel, &dest, &Cancel::default(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"0123456789");
        let offline = Tools { curl: "/missing-curl", ..TOOLS };
        transfer(&offline, &rel, &dest, &Cancel::default(), &|_| {}).unwrap();
    }

    #[test]
    fn optional_payload_resumes_partial_and_retries_a_bad_prefix() {
        let dir = TestDir::new("payload-resume");
        let (rel, dest) = fixture(&dir);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        for prefix in [b"0123".as_slice(), b"xxxx".as_slice()] {
            std::fs::write(&dest, prefix).unwrap();
            transfer(&TOOLS, &rel, &dest, &Cancel::default(), &|_| {}).unwrap();
            assert_eq!(std::fs::read(&dest).unwrap(), b"0123456789");
        }
    }

    #[test]
    fn optional_payload_bad_hash_never_publishes_a_complete_file() {
        let dir = TestDir::new("payload-hash");
        let (mut rel, dest) = fixture(&dir);
        rel.sha256 = "0".repeat(64);
        assert!(transfer(&TOOLS, &rel, &dest, &Cancel::default(), &|_| {}).is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn optional_payload_invalid_description_preserves_existing_cache() {
        let dir = TestDir::new("payload-invalid");
        let (mut rel, dest) = fixture(&dir);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"previous").unwrap();
        rel.size = 0;
        assert!(transfer(&TOOLS, &rel, &dest, &Cancel::default(), &|_| {}).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"previous");
    }

    #[test]
    fn optional_payload_cancellation_checkpoints_then_retry_continues() {
        let dir = TestDir::new("payload-cancel");
        let (rel, dest) = fixture(&dir);
        let curl = dir.join("fake-curl");
        crate::testutil::write_executable(&curl, "#!/bin/sh\nwhile [ $# -gt 0 ]; do if [ \"$1\" = -o ]; then shift; out=$1; fi; shift; done\nprintf 0123 > \"$out\"\nexec sleep 5\n");
        let tools = Tools { curl: curl.to_str().unwrap(), ..TOOLS };
        let cancel = Cancel::default();
        assert!(transfer(&tools, &rel, &dest, &cancel, &|bytes| { if bytes >= 4 { cancel.cancel(); } }).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"0123");
        transfer(&TOOLS, &rel, &dest, &Cancel::default(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"0123456789");
    }

    #[test]
    fn optional_payload_cli_refuses_plain_http_before_creating_files() {
        let dir = TestDir::new("payload-http");
        let (_, dest) = fixture(&dir);
        assert_eq!(cli(&["http://example.org/runtime".into(), "0".repeat(64), "1".into(),
                         dest.to_string_lossy().into_owned(), String::new()]), 2);
        assert!(!dest.exists());
    }
}
