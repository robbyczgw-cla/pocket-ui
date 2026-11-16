//! A Brick app update writes only what changed (docs/UPDATE-CHANNEL.md, "Unchanged parts are moved"): an unchanged folder or big file
//! of the running `app/` is renamed into `app.new/` instead of being unpacked again, and the way back puts it where it was.
use super::*;
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;

/// Deterministic, not compressible to nothing.
fn blob(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed).wrapping_add((i >> 8) as u8)).collect()
}

/// What version `v` of the program folder holds besides the programs: `changed` rewrites two files.
fn extras(changed: bool) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("sources/a.zip".into(), blob(1, 80_000)),
        ("sources/b.zip".into(), blob(2, 90_000)),
        ("sources/README".into(), b"sources\n".to_vec()),
        ("zoneinfo/UTC".into(), b"utc".to_vec()),
        ("zoneinfo/Europe/Berlin".into(), b"berlin".to_vec()),
        ("zoneinfo/America/New_York".into(), b"ny".to_vec()),
        ("msg/big.raw".into(), blob(3, 100_000)),
        ("licences/cores/gambatte.txt".into(), b"gpl".to_vec()),
        ("licences/cores/mgba.txt".into(), b"mpl".to_vec()),
        ("licences/NOTICE".into(), if changed { b"notice 2".to_vec() } else { b"notice 1".to_vec() }),
        ("docs/big.bin".into(), blob(4, 70_000)),
        ("docs/small.txt".into(), if changed { b"small 2".to_vec() } else { b"small 1".to_vec() }),
    ];
    files.sort();
    files
}

/// The tarball of `version` with `files` besides the programs (the base fixture of this module's parent), and its list entry.
fn tarball_with(dir: &Path, version: &str, files: &[(String, Vec<u8>)]) -> Release {
    let (rel, tar) = app_tarball(dir, version, Some(version), START);
    let src = dir.join(format!("app-src-{version}"));
    for (path, data) in files {
        std::fs::create_dir_all(src.join(path).parent().unwrap()).unwrap();
        std::fs::write(src.join(path), data).unwrap();
    }
    assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).arg(".").status().unwrap().success());
    Release { sha256: store::sha256_file(&tar).unwrap(), size: std::fs::metadata(&tar).unwrap().len(), ..rel }
}

/// Every file under `root` (not the update's own marks): path -> bytes.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                walk(&e.path(), base, out);
            } else {
                out.insert(e.path().strip_prefix(base).unwrap().display().to_string(), std::fs::read(e.path()).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out.retain(|k, _| k != ".ready" && k != ".asset-moves");
    out
}

fn inodes(root: &Path) -> BTreeMap<String, u64> {
    tree(root).keys().map(|k| (k.clone(), std::fs::metadata(root.join(k)).unwrap().ino())).collect()
}

/// A card that runs version 2 (everything unpacked) and offers version 3, which changes two small files and the programs.
fn card_at_2_offering_3(dir: &Path) -> (PathBuf, Release) {
    let p = card(dir);
    install(&tarball_with(dir, "2", &extras(false)), dir, &p).unwrap();
    assert_eq!(release_of(&p, "app"), "2");
    let rel3 = tarball_with(dir, "3", &extras(true));
    (p, rel3)
}

#[test]
fn an_update_moves_unchanged_folders_and_big_files_and_unpacks_only_what_changed() {
    let dir = TestDir::new("brick-delta-moves");
    let (p, rel3) = card_at_2_offering_3(&dir);
    let (before, before_inodes) = (tree(&p.join("app")), inodes(&p.join("app")));
    let before_players = players_files(&p);
    install(&rel3, &dir, &p).unwrap();
    assert_eq!(release_of(&p, "app"), "3");
    // The new program folder is exactly what the tarball holds.
    let mut want = tree(&dir.join("app-src-3"));
    want.retain(|k, _| k != "asset-packages.json");
    assert_eq!(tree(&p.join("app")), want);
    let after = inodes(&p.join("app"));
    let same = |path: &str| before_inodes[path] == after[path];
    // Whole unchanged folders and unchanged big files are the very same files, moved.
    for path in ["sources/a.zip", "sources/b.zip", "sources/README", "zoneinfo/UTC", "zoneinfo/Europe/Berlin", "zoneinfo/America/New_York",
        "licences/cores/gambatte.txt", "licences/cores/mgba.txt", "msg/big.raw", "docs/big.bin"] {
        assert!(same(path), "{path} is moved, not written again");
    }
    // What changed (and the programs, and the tiny files beside a changed one) is written.
    for path in ["licences/NOTICE", "docs/small.txt", "bin/pocket-ui", "RELEASE", "msg/starting.png"] {
        assert!(!same(path), "{path} is new");
    }
    // The way back lacks only what moved, and says so.
    let manifest = std::fs::read_to_string(p.join("app.previous/.asset-moves")).unwrap();
    for line in ["move:sources", "move:zoneinfo", "move:licences/cores", "move:msg/big.raw", "move:docs/big.bin", "move:cores"] {
        assert!(manifest.lines().any(|l| l == line), "{line} in {manifest:?}");
    }
    assert!(!manifest.contains("small.txt") && !manifest.contains("NOTICE") && !manifest.contains("bin/pocket-ui"), "{manifest:?}");
    assert!(!p.join("app.previous/sources").exists() && p.join("app.previous/docs/small.txt").exists());
    assert_eq!(players_files(&p), before_players);
    // sources 3, zoneinfo 3, licences/cores 2, cores 1 (the fixture's core), msg/big.raw, docs/big.bin
    assert_eq!(before.len(), tree(&p.join("app.previous")).len() + 11, "eleven files travelled between the two folders");
}

/// 15.11., Alex's Brick update 0.4.1-test042.2 → 0.4.2-test050.1: a 499 KB download, then 25 s of "unpacking" and 10 s of
/// "moving-assets" on its slow card with nothing moving on the page. Both steps now count their work for the bar.
#[test]
fn an_update_reports_real_progress_while_it_unpacks_and_while_it_moves_parts() {
    let dir = TestDir::new("brick-delta-progress");
    let (p, rel3) = card_at_2_offering_3(&dir);
    let cancel = Cancel::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let records = std::sync::Arc::clone(&seen);
    cancel.observe(move |s| records.lock().unwrap().push(s));
    let url = format!("file://{}/update.json", dir.display());
    install_brick_with(&TOOLS, &rel3, &url, &p, &cancel, &|_| {}, &plenty, &crate::untar::unpack_file_skipping).unwrap();
    assert_eq!(release_of(&p, "app"), "3");
    let seen = seen.lock().unwrap();
    crate::update::runtime::assert_step_progress(&seen, crate::update::Step::Unpacking, WorkerPhase::Unpacking);
    crate::update::runtime::assert_step_progress(&seen, crate::update::Step::Installing, WorkerPhase::MovingAssets);
}

#[test]
fn a_new_version_that_does_not_start_gets_the_moved_parts_back_with_the_old_one() {
    let dir = TestDir::new("brick-delta-rollback");
    let (p, rel3) = card_at_2_offering_3(&dir);
    let (before, before_inodes) = (tree(&p.join("app")), inodes(&p.join("app")));
    install(&rel3, &dir, &p).unwrap();
    // The test build never confirms its first picture: the real fixed boot script takes version 2 back, parts and all.
    assert!(Command::new("sh").arg(p.join("start.sh")).env("BRICK_DATA", &p).status().unwrap().success());
    assert_eq!(release_of(&p, "app"), "2");
    assert_eq!(tree(&p.join("app")), before);
    assert_eq!(inodes(&p.join("app")), before_inodes, "the very same files, not copies");
    assert!(!p.join("app/.asset-moves").exists() && !p.join("storage/pocket-ui/update-pending").exists());
}

#[test]
fn a_failure_after_the_parts_moved_puts_them_back_and_leaves_the_running_version_whole() {
    let dir = TestDir::new("brick-delta-undo");
    let (p, rel3) = card_at_2_offering_3(&dir);
    let (before, before_inodes) = (tree(&p.join("app")), inodes(&p.join("app")));
    let fail_ready = |a: &Path, into: &Path, l: u64, skip: &dyn Fn(&str) -> bool, read: &dyn Fn(u64)| {
        let entries = crate::untar::unpack_file_skipping(a, into, l, skip, read)?;
        std::fs::create_dir(into.join(".ready"))?;
        Ok(entries)
    };
    let url = format!("file://{}/update.json", dir.display());
    assert!(install_brick_with(&TOOLS, &rel3, &url, &p, &Cancel::new(), &|_| {}, &plenty, &fail_ready).is_err());
    assert_eq!(release_of(&p, "app"), "2");
    assert_eq!(tree(&p.join("app")), before);
    assert_eq!(inodes(&p.join("app")), before_inodes);
    assert!(!p.join("app.new").exists() && !p.join("app/.asset-moves").exists());
    // The next try works.
    install(&rel3, &dir, &p).unwrap();
    assert_eq!(release_of(&p, "app"), "3");
}

#[test]
fn a_folder_that_differs_from_the_tarball_in_any_way_is_not_moved_whole() {
    for case in ["extra_file", "extra_folder", "link", "changed_file", "missing_file"] {
        let dir = TestDir::new("brick-delta-differs");
        let (p, rel3) = card_at_2_offering_3(&dir);
        let s = p.join("app/sources");
        match case {
            "extra_file" => std::fs::write(s.join("extra"), b"mine").unwrap(),
            "extra_folder" => std::fs::create_dir(s.join("empty-dir")).unwrap(),
            "link" => std::os::unix::fs::symlink("a.zip", s.join("link")).unwrap(),
            "changed_file" => std::fs::write(s.join("README"), b"edited on the card").unwrap(),
            _ => std::fs::remove_file(s.join("README")).unwrap(),
        }
        let before = inodes(&p.join("app"));
        install(&rel3, &dir, &p).unwrap();
        let mut want = tree(&dir.join("app-src-3"));
        want.retain(|k, _| k != "asset-packages.json");
        assert_eq!(tree(&p.join("app")), want, "{case}: the new folder is exactly the tarball");
        let manifest = std::fs::read_to_string(p.join("app.previous/.asset-moves")).unwrap_or_default();
        assert!(!manifest.lines().any(|l| l == "move:sources"), "{case}: {manifest:?}");
        assert!(manifest.lines().any(|l| l == "move:zoneinfo"), "{case}: the other folders still move");
        if case != "link" {
            // Big files inside that are still the same go one by one.
            assert!(manifest.lines().any(|l| l == "move:sources/a.zip"), "{case}: {manifest:?}");
            assert_eq!(before["sources/a.zip"], inodes(&p.join("app"))["sources/a.zip"], "{case}");
        }
        // What stayed behind is still in the way back.
        match case {
            "extra_file" => assert_eq!(std::fs::read(p.join("app.previous/sources/extra")).unwrap(), b"mine"),
            "link" => assert!(p.join("app.previous/sources/link").symlink_metadata().is_ok()),
            _ => {}
        }
    }
}

#[test]
fn what_the_start_check_needs_is_always_unpacked_and_a_card_without_changes_still_updates() {
    let dir = TestDir::new("brick-delta-required");
    let p = card(&dir);
    // A big starting picture, the same in both versions: it is checked before the switch, so it is written, not moved.
    let files = vec![("msg/starting.png".to_string(), blob(9, 120_000)), ("msg/other.raw".to_string(), blob(8, 120_000))];
    install(&tarball_with(&dir, "2", &files), &dir, &p).unwrap();
    let before = inodes(&p.join("app"));
    install(&tarball_with(&dir, "3", &files), &dir, &p).unwrap();
    let after = inodes(&p.join("app"));
    assert_ne!(before["msg/starting.png"], after["msg/starting.png"]);
    assert_eq!(before["msg/other.raw"], after["msg/other.raw"]);
    assert_eq!(release_of(&p, "app"), "3");
}

/// Not a test: the bytes an update writes, measured on a real (sync-mounted) FAT card by the caller, who reads the block device's
/// write counters around each run (scripts/brick-update-bench.sh). `BENCH_CARD` is the card's pocket-ui folder, `BENCH_TAR` the app
/// tarball (a real release whose two programs are stand-ins of the same size), `BENCH_VERSION_FILE` names a file holding its version.
#[test]
#[ignore = "measurement on a FAT card: scripts/brick-update-bench.sh"]
fn bench_install() {
    let (Ok(card), Ok(tar)) = (std::env::var("BENCH_CARD"), std::env::var("BENCH_TAR")) else { return };
    let (card, tar) = (PathBuf::from(card), PathBuf::from(tar));
    let version = std::fs::read_to_string(std::env::var("BENCH_VERSION_FILE").unwrap()).unwrap().trim().to_string();
    let data = std::fs::read(&tar).unwrap();
    let rel = Release { version, url: format!("file://{}", tar.display()), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: store::sha256_hex(&data), size: data.len() as u64, notes: String::new() };
    let started = Instant::now();
    install_brick_with(&TOOLS, &rel, "file:///unused/update.json", &card, &Cancel::new(), &|_| {}, &plenty, &crate::untar::unpack_file_skipping).unwrap();
    eprintln!("bench: installed {} in {:.1} s", rel.version, started.elapsed().as_secs_f64());
}
