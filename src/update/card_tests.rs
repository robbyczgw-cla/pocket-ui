//! The update file on the card: found, checked, installed like a Wi-Fi update and deleted; every file that isn't ours, isn't
//! newer, isn't for this card or isn't whole is explained and never installed; a damaged file never takes pocket-ui down.
use super::*;
use crate::testutil::TestDir;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use std::process::Command;

fn key() -> SigningKey { SigningKey::from_bytes(&[3u8; 32]) }
fn public() -> [u8; 32] { key().verifying_key().to_bytes() }

/// A release tarball whose programs answer `--version` with `version`.
fn tarball(dir: &Path, version: &str) -> Vec<u8> {
    let src = dir.join(format!("src-{version}"));
    std::fs::create_dir_all(&src).unwrap();
    for bin in ["pocket-ui", "pocket-core"] {
        crate::testutil::write_executable(&src.join(bin), format!("#!/bin/sh\necho {bin} {version}\n"));
    }
    let tar = dir.join(format!("t-{version}.tar.gz"));
    assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).args(["pocket-ui", "pocket-core"]).status().unwrap().success());
    std::fs::read(tar).unwrap()
}

/// The channel's list for `app` named as the release kit names it for `target`.
fn list(version: &str, app: &[u8], target: Target) -> Vec<u8> {
    let (name, extra) = match target {
        Target::Rocknix => (format!("pocket-ui-{version}-aarch64.tar.gz"), ""),
        Target::Brick => (format!("pocket-ui-brick-{version}.tar.gz"), ""),
        Target::Darkos => (format!("pocket-ui-darkos-{version}.tar.gz"), r#""target":"darkos-port","#),
    };
    format!(r#"{{"format":1,{extra}"app":{{"version":"{version}","url":"https://github.com/o/pocket-ui/releases/download/v{version}/{name}","sha256":"{}","size":{},"notes":"n"}},"os":null}}"#,
        crate::store::sha256_hex(app), app.len()).into_bytes()
}

fn sign(json: &[u8], with: &SigningKey) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.encode(with.sign(json).to_bytes()).into_bytes()
}

/// A zip with stored entries, as `scripts/make-card-update.py` writes it.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let crc = crate::battery_history::crc32;
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let at = out.len() as u32;
        let head = |sig: &[u8], central: bool| {
            let mut h = sig.to_vec();
            if central { h.extend(20u16.to_le_bytes()); }
            h.extend(20u16.to_le_bytes()); h.extend(0u16.to_le_bytes()); h.extend(0u16.to_le_bytes());
            h.extend(0u32.to_le_bytes()); h.extend(crc(data).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes()); h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes()); h.extend(0u16.to_le_bytes());
            h
        };
        out.extend(head(b"PK\x03\x04", false));
        out.extend(name.as_bytes());
        out.extend(*data);
        let mut c = head(b"PK\x01\x02", true);
        c.extend([0u8; 6]); c.extend((0o100644u32 << 16).to_le_bytes()); c.extend(at.to_le_bytes());
        c.extend(name.as_bytes());
        central.extend(c);
    }
    let at = out.len() as u32;
    out.extend(&central);
    out.extend(b"PK\x05\x06"); out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes()); out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes()); out.extend(at.to_le_bytes()); out.extend(0u16.to_le_bytes());
    out
}

/// A whole update file for `version` and `target`.
fn update_file(dir: &Path, version: &str, target: Target) -> (Vec<u8>, Vec<u8>) {
    let app = tarball(dir, version);
    let json = list(version, &app, target);
    let name = format!("pocket-ui-{version}-aarch64.tar.gz");
    (zip(&[("update.json", &json), ("update.json.sig", &sign(&json, &key())), (&name, &app), ("README.txt", b"Copy me onto the card.")]), app)
}

/// The places of a ROCKNIX card under `root`: the first part, the GAMES drive (or a games card), its roms folder.
fn rocknix(root: &Path) -> Vec<PathBuf> {
    let places: Vec<PathBuf> = ["flash", "storage/games-external", "storage/roms"].iter().map(|p| root.join(p)).collect();
    for p in &places { std::fs::create_dir_all(p).unwrap(); }
    places
}

fn look_here(places: &[PathBuf], current: &str) -> Look {
    look_in(places, current, true, &public(), Target::Rocknix)
}

fn install_app_at(root: &Path) -> impl Fn(&Release, &str, &Cancel, &dyn Fn(u64)) -> Result<(), String> + '_ {
    move |rel, url, cancel, progress| super::super::install_app(rel, url, root, cancel, progress)
}

/// Up to 0.4.x a handheld without Wi-Fi (RG28XX, R36S) had no way to update at all, only a new flash. The file on its GAMES drive
/// is found, checked, offered, installed exactly like a Wi-Fi update (new release, `previous`, `update-pending` for the
/// health check) and deleted; the next look finds nothing, so nothing loops.
#[test]
fn an_update_file_on_the_games_drive_is_found_installed_like_wifi_and_deleted() {
    let dir = TestDir::new("card-update");
    let places = rocknix(&dir);
    let (file, _) = update_file(&dir, "9.1.0", Target::Rocknix);
    let at = places[1].join("pocket-ui-update-9.1.0-rocknix.zip");
    std::fs::write(&at, &file).unwrap();
    let Look::Offer(found) = look_here(&places, "9.0.0") else { panic!("not offered: {:?}", look_here(&places, "9.0.0")) };
    assert_eq!(found.version(), "9.1.0");
    assert_eq!(found.file, at);
    assert!(found.manifest.os.is_none() && found.manifest.app.as_ref().unwrap().notes == "n");

    let root = dir.join("app");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("pocket-ui"), "old").unwrap();
    std::fs::write(root.join("pocket-core"), "old").unwrap();
    let seen = std::sync::atomic::AtomicU64::new(0);
    let rel = found.manifest.app.clone().unwrap();
    install_with(&found.file, &rel, &Cancel::new(), &|n| seen.store(n, std::sync::atomic::Ordering::SeqCst), &install_app_at(&root)).unwrap();
    assert_eq!(std::fs::read_link(root.join("current")).unwrap(), Path::new("releases/9.1.0"));
    assert!(std::fs::read_link(root.join("previous")).is_ok(), "the way back is kept");
    assert_eq!(std::fs::read_to_string(root.join("update-pending")).unwrap(), "9.1.0", "the health check runs as after Wi-Fi");
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), rel.size, "progress as for a download");
    assert!(!at.exists(), "the file is deleted after the install");
    assert!(matches!(look_here(&places, "9.1.0"), Look::Nothing));
    ACTIVE.with(|a| assert!(a.borrow().is_none(), "the next install on this thread asks the server again"));
}

/// A Mac unpacks a downloaded zip by itself: the folder counts the same, in the card's first part too, and goes after
/// the install with only its own files.
#[test]
fn an_unpacked_update_folder_counts_the_same_and_is_removed_after() {
    let dir = TestDir::new("card-update-folder");
    let places = rocknix(&dir);
    let app = tarball(&dir, "9.2.0");
    let json = list("9.2.0", &app, Target::Rocknix);
    let folder = places[0].join("pocket-ui-update-9.2.0-rocknix");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("update.json"), &json).unwrap();
    std::fs::write(folder.join("update.json.sig"), sign(&json, &key())).unwrap();
    std::fs::write(folder.join("pocket-ui-9.2.0-aarch64.tar.gz"), &app).unwrap();
    std::fs::write(folder.join("._update.json"), b"mac junk").unwrap();
    let Look::Offer(found) = look_here(&places, "9.0.0") else { panic!("folder not offered") };
    let root = dir.join("app");
    install_with(&found.file, found.manifest.app.as_ref().unwrap(), &Cancel::new(), &|_| {}, &install_app_at(&root)).unwrap();
    assert_eq!(std::fs::read_link(root.join("current")).unwrap(), Path::new("releases/9.2.0"));
    assert!(!folder.exists());
}

/// Wrong key, older or the same version, another card's list, a development build: each says why, nothing is installed,
/// the file stays where the player put it.
#[test]
fn a_file_not_ours_not_newer_or_for_another_card_is_explained_and_never_offered() {
    let dir = TestDir::new("card-update-refused");
    let places = rocknix(&dir);
    let at = places[1].join("pocket-ui-update-9.1.0-rocknix.zip");
    let refused = |look: Look| match look { Look::Refused { why, .. } => why, other => panic!("not refused: {other:?}") };

    let app = tarball(&dir, "9.1.0");
    let json = list("9.1.0", &app, Target::Rocknix);
    let other = SigningKey::from_bytes(&[4u8; 32]);
    std::fs::write(&at, zip(&[("update.json", &json), ("update.json.sig", &sign(&json, &other)), ("a.tar.gz", &app)])).unwrap();
    assert_eq!(refused(look_here(&places, "9.0.0")), NOT_SIGNED);
    // The list changed after signing (another version written into it).
    let changed = String::from_utf8(json.clone()).unwrap().replace("9.1.0\",\"url", "9.9.0\",\"url").into_bytes();
    std::fs::write(&at, zip(&[("update.json", &changed), ("update.json.sig", &sign(&json, &key())), ("a.tar.gz", &app)])).unwrap();
    assert_eq!(refused(look_here(&places, "9.0.0")), NOT_SIGNED);

    let (file, _) = update_file(&dir, "9.1.0", Target::Rocknix);
    std::fs::write(&at, &file).unwrap();
    for current in ["9.1.0", "9.2.0"] {
        let look = look_here(&places, current);
        assert!(matches!(&look, Look::NotNewer { version } if version == "9.1.0"), "{current}: {look:?}");
        assert!(look.message().contains("(9.1.0) isn't newer"));
    }
    assert_eq!(refused(look_in(&places, "9.0.0", false, &public(), Target::Rocknix)), super::super::DEVELOPMENT_BUILD);

    for (theirs, ours) in [(Target::Brick, Target::Rocknix), (Target::Darkos, Target::Rocknix), (Target::Rocknix, Target::Brick), (Target::Darkos, Target::Brick), (Target::Brick, Target::Darkos)] {
        let json = list("9.1.0", &app, theirs);
        std::fs::write(&at, zip(&[("update.json", &json), ("update.json.sig", &sign(&json, &key())), ("a.tar.gz", &app)])).unwrap();
        assert_eq!(refused(look_in(&places, "9.0.0", true, &public(), ours)), OTHER_TARGET, "{theirs:?} on {ours:?}");
    }
    let mine = |t: Target| { let json = list("9.1.0", &app, t); std::fs::write(&at, zip(&[("update.json", &json), ("update.json.sig", &sign(&json, &key())), ("a.tar.gz", &app)])).unwrap(); };
    for t in [Target::Brick, Target::Darkos] {
        mine(t);
        assert!(matches!(look_in(&places, "9.0.0", true, &public(), t), Look::Offer(_)), "{t:?} takes its own");
    }
    assert!(at.exists(), "a refused file is left alone");
    assert!(!dir.join("app").exists(), "nothing was installed");
}

/// A copy pulled out half-way (the card taken out of the computer too early): every length of it is explained, none is
/// offered, nothing panics. With bytes changed, a file is offered only when the app inside is still the signed one.
#[test]
fn a_half_copied_or_damaged_file_is_explained_and_never_installed() {
    let dir = TestDir::new("card-update-cut");
    let places = rocknix(&dir);
    let (file, app) = update_file(&dir, "9.1.0", Target::Rocknix);
    let at = places[1].join("pocket-ui-update-9.1.0-rocknix.zip");
    let step = (file.len() / 60).max(1);
    for len in (0..file.len()).step_by(step).chain([file.len() - 1]) {
        std::fs::write(&at, &file[..len]).unwrap();
        match look_here(&places, "9.0.0") {
            Look::Refused { why, .. } => assert_eq!(why, INCOMPLETE, "cut at {len}"),
            other => panic!("cut at {len}: {other:?}"),
        }
    }
    // The tarball inside changed (one byte): the signed hash no longer fits.
    let pos = file.windows(app.len()).position(|w| w == app.as_slice()).unwrap() + app.len() / 2;
    let mut bad = file.clone();
    bad[pos] ^= 0x55;
    std::fs::write(&at, &bad).unwrap();
    assert!(matches!(look_here(&places, "9.0.0"), Look::Refused { why, .. } if why == INCOMPLETE));
    // A folder without its tarball, or without the signature.
    std::fs::remove_file(&at).unwrap();
    let folder = places[0].join("pocket-ui-update-9.1.0-rocknix");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("update.json"), list("9.1.0", &app, Target::Rocknix)).unwrap();
    assert!(matches!(look_here(&places, "9.0.0"), Look::Refused { why, .. } if why == INCOMPLETE));
}

/// The file checked at the look and changed before A (copied again, half): the install's own check refuses it, the
/// installed version stays, the file stays.
#[test]
fn a_file_changed_between_the_look_and_the_install_is_refused_by_the_install() {
    let dir = TestDir::new("card-update-changed");
    let places = rocknix(&dir);
    let (file, app) = update_file(&dir, "9.1.0", Target::Rocknix);
    let at = places[1].join("pocket-ui-update-9.1.0-rocknix.zip");
    std::fs::write(&at, &file).unwrap();
    let Look::Offer(found) = look_here(&places, "9.0.0") else { panic!() };
    let pos = file.windows(app.len()).position(|w| w == app.as_slice()).unwrap() + 10;
    let mut bad = file.clone();
    bad[pos] ^= 1;
    std::fs::write(&at, &bad).unwrap();
    let root = dir.join("app");
    let err = install_with(&found.file, found.manifest.app.as_ref().unwrap(), &Cancel::new(), &|_| {}, &install_app_at(&root)).unwrap_err();
    assert_eq!(err, INCOMPLETE);
    assert_ne!(std::fs::read_link(root.join("current")).ok(), Some(PathBuf::from("releases/9.1.0")), "nothing installed");
    assert!(!root.join("update-pending").exists() && at.exists());
}

/// A Brick or dArkOS update whose component package is not in the file: said, never fetched from a server for minutes on a
/// handheld without Wi-Fi. A package that is there is found by its signed hash, whatever its name.
#[test]
fn a_component_the_file_lacks_is_explained_one_it_has_is_found_by_its_hash() {
    let dir = TestDir::new("card-update-needs");
    let (file, app) = update_file(&dir, "9.1.0", Target::Rocknix);
    let at = dir.join("pocket-ui-update-9.1.0-rocknix.zip");
    std::fs::write(&at, &file).unwrap();
    let bundle = Bundle::open(&at).unwrap();
    let app_sha = crate::store::sha256_hex(&app);
    let package = Release { version: "optional".into(), url: "https://h/asset-x.tar.gz".into(), mirrors: vec![], preferred_sources: vec![],
        sha256: "ab".repeat(32), size: 123, notes: String::new() };
    let dest = dir.join("asset");
    assert_eq!(from(&bundle, &app_sha, &package, &dest, &Cancel::new(), &|_| {}).unwrap_err(), NEEDS_MORE);
    let rel = Release { sha256: app_sha.clone(), size: app.len() as u64, ..package };
    from(&bundle, &app_sha, &rel, &dest, &Cancel::new(), &|_| {}).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), app);
    let cancel = Cancel::new();
    cancel.cancel();
    assert!(from(&bundle, &app_sha, &rel, &dest, &cancel, &|_| {}).is_err());
    assert!(!dest.exists(), "a cancelled copy leaves nothing behind");
}

/// The newest good file wins over an older one and over a broken one beside it; the same file seen through two places
/// (the roms folder is a bind of the games drive's) counts once.
#[test]
fn the_newest_good_file_wins_and_one_file_counts_once() {
    let dir = TestDir::new("card-update-many");
    let places = rocknix(&dir);
    std::fs::write(places[0].join("pocket-ui-update-9.1.0-rocknix.zip"), update_file(&dir, "9.1.0", Target::Rocknix).0).unwrap();
    std::fs::write(places[1].join("Pocket-UI-Update-9.3.0-rocknix (1).zip"), update_file(&dir, "9.3.0", Target::Rocknix).0).unwrap();
    std::fs::write(places[1].join("pocket-ui-update-9.4.0-rocknix.zip"), b"PK half").unwrap();
    std::fs::write(places[1].join("pocket-ui-update-notes.txt"), b"not an update file").unwrap();
    let Look::Offer(found) = look_here(&places, "9.0.0") else { panic!() };
    assert_eq!(found.version(), "9.3.0");
    std::fs::remove_dir_all(&places[2]).unwrap();
    std::os::unix::fs::symlink(&places[1], &places[2]).unwrap();
    std::fs::remove_file(places[1].join("pocket-ui-update-9.4.0-rocknix.zip")).unwrap();
    std::fs::remove_file(places[0].join("pocket-ui-update-9.1.0-rocknix.zip")).unwrap();
    assert!(matches!(look_here(&places, "9.3.0"), Look::NotNewer { .. }));
}

/// Where a computer puts the file: the RG28XX's GAMES drive and the R36S's games card are mounted at
/// /storage/games-external (deploy/image/games-mount.sh, games_card::MOUNT), the first part at /flash; on the Brick the
/// card's root; on dArkOS the EASYROMS part over ports/pocket-ui.
#[test]
fn the_places_are_the_parts_of_the_card_a_computer_shows() {
    let rocknix = places_for(Target::Rocknix, None);
    for p in ["/flash", crate::games_card::MOUNT, "/storage/roms"] {
        assert!(rocknix.contains(&PathBuf::from(p)), "{p}");
    }
    assert_eq!(places_for(Target::Brick, Some(Path::new("/mnt/SDCARD/pocket-ui"))), [PathBuf::from("/mnt/SDCARD"), PathBuf::from("/mnt/SDCARD/pocket-ui")]);
    assert_eq!(places_for(Target::Darkos, Some(Path::new("/roms/ports/pocket-ui"))), [PathBuf::from("/roms"), PathBuf::from("/roms/ports/pocket-ui")]);
    // The R36S: no Wi-Fi, a second slot for the games card, and the Updates page says what to do instead.
    let r36s = crate::device::from_override("r36s");
    assert!(!r36s.has_wifi() && r36s.has_second_card_slot());
    assert!(super::super::NO_WIFI_HERE.contains("pocketui.xyz/install"));
    assert_eq!(Target::here(), Target::Rocknix);
    assert_eq!([Target::Rocknix, Target::Brick, Target::Darkos].map(Target::file_part), ["rocknix", "trimui", "darkos"]);
}

/// xorshift64*: the same numbers on every run.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 >> 12; self.0 ^= self.0 << 25; self.0 ^= self.0 >> 27; self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) }
    fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
}

/// The crash sweep's fuzzer for the update file (`scripts/crash-sweep.sh`, `CASE_KNOWS_FUZZ_ROUNDS` multiplies the
/// rounds): bytes changed, cut, repeated, random; directory entries pointing anywhere. The look always answers, never
/// panics, and offers only a file whose app is still the signed one.
#[test]
fn card_update_fuzz_never_panics_and_never_offers_a_changed_app() {
    let rounds = std::env::var("CASE_KNOWS_FUZZ_ROUNDS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1) * 150;
    let dir = TestDir::new("card-update-fuzz");
    let places = rocknix(&dir);
    let (file, app) = update_file(&dir, "9.1.0", Target::Rocknix);
    let at = places[1].join("pocket-ui-update-9.1.0-rocknix.zip");
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut found = Vec::new();
    for round in 0..rounds {
        let mut bytes = file.clone();
        match round % 5 {
            0 => for _ in 0..1 + rng.below(8) { let i = rng.below(bytes.len()); bytes[i] = rng.next() as u8; },
            1 => bytes.truncate(rng.below(bytes.len())),
            2 => { let tail = bytes.len() - 22; for i in tail..bytes.len() { if rng.below(3) == 0 { bytes[i] = rng.next() as u8; } } }
            3 => { let i = rng.below(bytes.len()); let n = rng.below(4096); let piece = bytes[i..(i + n).min(bytes.len())].to_vec(); bytes.splice(i..i, piece); }
            _ => bytes = (0..rng.below(70_000)).map(|_| rng.next() as u8).collect(),
        }
        std::fs::write(&at, &bytes).unwrap();
        let result = std::panic::catch_unwind(|| look_here(&places, "9.0.0"));
        match result {
            Err(_) => found.push(format!("round {round}: panic")),
            Ok(Look::Offer(f)) => {
                let bundle = Bundle::open(&f.file).unwrap();
                let rel = f.manifest.app.unwrap();
                let e = bundle.find(rel.size, &rel.sha256).unwrap();
                let mut copy = Vec::new();
                Bundle::reader(e).unwrap().read_to_end(&mut copy).unwrap();
                if copy != app { found.push(format!("round {round}: offered a changed app")); }
            }
            Ok(_) => {}
        }
        if found.len() > 20 { break; }
    }
    assert!(found.is_empty(), "{found:#?}");
}

/// The release kit's file (`scripts/make-card-update.py`) is what the device reads: made from a channel folder (the list,
/// its signature, the tarball, a component package the tarball names), found, offered and installed; the same input makes
/// the same bytes.
#[test]
fn the_release_kits_update_file_is_what_the_device_takes() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("no python3: skipped");
        return;
    }
    let dir = TestDir::new("card-update-kit");
    let channel = dir.join("updates/stable");
    std::fs::create_dir_all(&channel).unwrap();
    // A tarball that names one component package, as the Brick's and dArkOS' do.
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    for bin in ["pocket-ui", "pocket-core"] {
        crate::testutil::write_executable(&src.join(bin), format!("#!/bin/sh\necho {bin} 9.5.0\n"));
    }
    let package = b"a component package".to_vec();
    let package_sha = crate::store::sha256_hex(&package);
    let assets = dir.join("release");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join(format!("asset-{package_sha}.tar.gz")), &package).unwrap();
    std::fs::write(assets.join("unrelated.tar.gz"), b"not named").unwrap();
    std::fs::write(src.join("asset-packages.json"), serde_json::to_vec(&serde_json::json!({"format": 1, "assets": [{"path": "cores/x.so", "size": 1,
        "sha256": "00", "package": {"version": "9.5.0", "url": format!("https://h/asset-{package_sha}.tar.gz"), "sha256": package_sha, "size": package.len()}}]})).unwrap()).unwrap();
    let tar = channel.join("pocket-ui-9.5.0-aarch64.tar.gz");
    assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).args(["pocket-ui", "pocket-core", "asset-packages.json"]).status().unwrap().success());
    let app = std::fs::read(&tar).unwrap();
    let json = list("9.5.0", &app, Target::Rocknix);
    std::fs::write(channel.join("update.json"), &json).unwrap();
    std::fs::write(channel.join("update.json.sig"), sign(&json, &key())).unwrap();
    let make = |out: &Path| {
        let o = Command::new("python3").arg("-I").arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/make-card-update.py"))
            .arg("--channel").arg(&channel).arg("--assets").arg(&assets).arg("--out").arg(out).arg("--all-assets").output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        std::fs::read(out.join("pocket-ui-update-9.5.0-rocknix.zip")).unwrap()
    };
    let places = rocknix(&dir.join("card"));
    let first = make(&places[1]);
    assert_eq!(first, make(&dir.join("again")), "the same input, the same bytes");
    let file = places[1].join("pocket-ui-update-9.5.0-rocknix.zip");
    let bundle = Bundle::open(&file).unwrap();
    assert!(bundle.find(package.len() as u64, &package_sha).is_some_and(|e| e.name == format!("assets/asset-{package_sha}.tar.gz")), "the package rides along");
    assert!(bundle.entry("assets/unrelated.tar.gz").is_none() && bundle.entry("README.txt").is_some());
    let Look::Offer(found) = look_here(&places, "9.0.0") else { panic!("{:?}", look_here(&places, "9.0.0")) };
    let root = dir.join("app");
    install_with(&found.file, found.manifest.app.as_ref().unwrap(), &Cancel::new(), &|_| {}, &install_app_at(&root)).unwrap();
    assert_eq!(std::fs::read_link(root.join("current")).unwrap(), Path::new("releases/9.5.0"));
    assert!(!file.exists());
}

/// Release check by hand on a real file from the kit (the real key): `POCKETUI_CARD_UPDATE_FILE=<zip>
/// POCKETUI_CARD_UPDATE_FROM=<version before> POCKETUI_CARD_UPDATE_TARGET=rocknix|trimui|darkos cargo test --lib
/// a_real_update_file -- --ignored`.
#[test]
#[ignore = "needs a real update file: POCKETUI_CARD_UPDATE_FILE"]
fn a_real_update_file_passes_every_check_of_the_device() {
    let file = PathBuf::from(std::env::var("POCKETUI_CARD_UPDATE_FILE").expect("POCKETUI_CARD_UPDATE_FILE"));
    let from = std::env::var("POCKETUI_CARD_UPDATE_FROM").unwrap_or_else(|_| "0.0.1".into());
    let target = match std::env::var("POCKETUI_CARD_UPDATE_TARGET").as_deref() { Ok("trimui") => Target::Brick, Ok("darkos") => Target::Darkos, _ => Target::Rocknix };
    let dir = TestDir::new("card-update-real");
    std::fs::copy(&file, dir.join(file.file_name().unwrap())).unwrap();
    match look_in(&[dir.to_path_buf()], &from, true, &crate::store::PUBLIC_KEY, target) {
        Look::Offer(found) => println!("offered: {} from {}", found.version(), found.file.display()),
        other => panic!("{other:?}"),
    }
}
