//! scripts/release-channel.sh with made-up artefacts and a throwaway key (docs/UPDATE-CHANNEL.md): the signed list it
//! writes, what it refuses, and `--check` against the files it names. The device side of the same list (the GitHub
//! redirect, resume, hash) is tested with a real curl and a small server in `update.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use pocket_ui::update::{parse_manifest, Manifest};

const STORE_TOOL: &str = env!("CARGO_BIN_EXE_store-tool");
const VERSION: &str = "9.9.9";

/// A scratch folder (`.0`), removed with everything in it when the `Tmp` is dropped, also when the test panics.
struct Tmp(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl Tmp {
    fn new(name: &str) -> Tmp {
        let guard = tempfile::Builder::new().prefix(&format!("pocketui-release-channel-{name}-")).tempdir().unwrap();
        Tmp(guard.path().to_path_buf(), guard)
    }
}

/// The start of a 64-bit little-endian ELF program for `machine` (183: aarch64, 62: x86-64), then `text`.
fn fake_program(machine: u16, text: &str) -> Vec<u8> {
    let mut b = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
    b.resize(16, 0);
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&machine.to_le_bytes());
    b.resize(64, 0);
    b.extend_from_slice(text.as_bytes());
    b
}

/// A release-out/ folder as the build leaves it: the two programs, the install package, the image and its notes.
fn release_out(dir: &Path) -> PathBuf {
    let out = dir.join("release-out");
    std::fs::create_dir_all(out.join("bin")).unwrap();
    std::fs::write(out.join("bin/pocket-ui"), fake_program(183, &format!("pocket-ui {VERSION}"))).unwrap();
    std::fs::write(out.join("bin/pocket-core"), fake_program(183, &format!("pocket-core {VERSION}"))).unwrap();
    std::fs::write(out.join(format!("pocket-ui-{VERSION}-rocknix-install.zip")), b"PK fake install package").unwrap();
    std::fs::write(out.join(format!("pocket-ui-{VERSION}-rp-mini-v2.img.xz")), vec![7u8; 50_000]).unwrap();
    std::fs::write(out.join(format!("pocket-ui-{VERSION}-rp-mini-v2.txt")), b"notes of the image").unwrap();
    std::fs::write(out.join("NOTES.txt"), b"Faster menus.").unwrap();
    out
}

/// A new key pair: the private key file (in the test's folder under /tmp) and its public half.
fn throwaway_key(dir: &Path) -> (PathBuf, [u8; 32]) {
    let key = dir.join("throwaway.key");
    let out = Command::new(STORE_TOOL).arg("keygen").arg(&key).output().unwrap();
    assert!(out.status.success());
    // "pub const PUBLIC_KEY: [u8; 32] = [0x77, 0x3e, ...];"
    let text = String::from_utf8(out.stdout).unwrap();
    let bytes: Vec<u8> = text.split("0x").skip(1).map(|h| u8::from_str_radix(&h[..2], 16).unwrap()).collect();
    (key, bytes.try_into().unwrap())
}

fn run(args: &[&str]) -> Output {
    let root = env!("CARGO_MANIFEST_DIR");
    Command::new("sh")
        .arg(Path::new(root).join("scripts/release-channel.sh"))
        .args(args)
        .env("STORE_TOOL", STORE_TOOL)
        .env("SOURCE_DATE_EPOCH", "1761436800")
        .env_remove("POCKETUI_SIGNING_KEY")
        .current_dir(root)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Runs the script on `inp` with the throwaway key, into `<dir>/site`.
fn build(dir: &Path, inp: &Path, key: &Path, extra: &[&str]) -> (Output, PathBuf) {
    let site = dir.join("site");
    let mut args = vec!["--repo", "owner/pocket-ui", "--in", s(inp), "--out", s(&site), "--key", s(key), "--allow-foreign-key"];
    args.extend_from_slice(extra);
    (run(&args), site)
}

fn signed_list(site: &Path, public: &[u8; 32]) -> Manifest {
    let list = site.join("updates/stable/update.json");
    let json = std::fs::read(&list).unwrap();
    let sig = std::fs::read_to_string(list.with_extension("json.sig")).unwrap();
    parse_manifest(&json, &sig, public).expect("the list verifies with the key it was signed with")
}

fn sha(path: &Path) -> String {
    pocket_ui::store::sha256_file(path).unwrap()
}

#[test]
fn builds_a_signed_list_that_names_the_files_of_the_github_release() {
    let tmp = Tmp::new("build");
    let inp = release_out(&tmp.0);
    let (key, public) = throwaway_key(&tmp.0);
    let (out, site) = build(&tmp.0, &inp, &key, &[]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("WARNING: signed with a key pocket-ui does not have built in"), "{}", text(&out));

    let m = signed_list(&site, &public);
    let base = format!("https://github.com/owner/pocket-ui/releases/download/v{VERSION}");
    let app = m.app.expect("the app update");
    assert_eq!(app.version, VERSION);
    assert_eq!(app.url, format!("{base}/pocket-ui-{VERSION}-aarch64.tar.gz"));
    assert_eq!(app.notes, "Faster menus.", "NOTES.txt");
    let tarball = site.join(format!("github/pocket-ui-{VERSION}-aarch64.tar.gz"));
    assert_eq!(app.sha256, sha(&tarball));
    assert_eq!(app.size, std::fs::metadata(&tarball).unwrap().len());
    // What the device unpacks: exactly the three files of a release (update::install_app).
    let listing = Command::new("tar").arg("-tzf").arg(&tarball).output().unwrap();
    let mut names: Vec<String> = String::from_utf8_lossy(&listing.stdout).lines().map(str::to_string).collect();
    names.sort();
    assert_eq!(names, ["pocket-core", "pocket-ui", "portmaster-setup.sh"]);
    // The device resolves the URL as it stands: absolute, on https.
    assert_eq!(pocket_ui::update::resolve(pocket_ui::update::DEFAULT_URL, &app.url), app.url);
    assert!(m.os.is_none(), "no system update without --os-tar");

    let image = m.sd_image.expect("the SD image");
    assert_eq!(image.url, format!("{base}/pocket-ui-{VERSION}-rp-mini-v2.img.xz"));
    assert_eq!(image.sha256, sha(&inp.join(format!("pocket-ui-{VERSION}-rp-mini-v2.img.xz"))));
    let zip = m.install_zip.expect("the install package");
    assert_eq!(zip.sha256, sha(&inp.join(format!("pocket-ui-{VERSION}-rocknix-install.zip"))));

    // Everything the GitHub release needs, listed for `gh release create`, with its checksums.
    let assets = std::fs::read_to_string(site.join("github/assets.txt")).unwrap();
    for name in ["-aarch64.tar.gz", "-rocknix-install.zip", "-rp-mini-v2.img.xz", "-rp-mini-v2.txt", "SHA256SUMS"] {
        assert!(assets.lines().any(|l| l.ends_with(name) && Path::new(l).is_file()), "{name} in {assets}");
    }
    let sums = std::fs::read_to_string(site.join("github/SHA256SUMS")).unwrap();
    assert!(sums.contains(&format!("{}  pocket-ui-{VERSION}-aarch64.tar.gz", app.sha256)), "{sums}");
    let next = std::fs::read_to_string(site.join("NEXT-STEPS.txt")).unwrap();
    assert!(next.contains(&format!("gh release create v{VERSION} --repo owner/pocket-ui")), "{next}");
    assert!(next.contains("--check"), "{next}");

    // The headers that keep the list from being cached stale, as JSON Vercel takes.
    let headers: serde_json::Value = serde_json::from_slice(&std::fs::read(site.join("vercel-headers.json")).unwrap()).unwrap();
    let rule = &headers["headers"][0];
    assert_eq!(rule["source"], "/updates/(.*)");
    assert!(rule["headers"].as_array().unwrap().iter().any(|h| h["key"] == "Cache-Control" && h["value"].as_str().unwrap().contains("no-cache")));

    // The private key never lands in the folder that is deployed or uploaded.
    let secret = std::fs::read(&key).unwrap();
    let mut stack = vec![site.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let data = std::fs::read(&p).unwrap();
                assert!(!data.windows(32).any(|w| w == secret.as_slice()), "{} holds the private key", p.display());
            }
        }
    }

    // The same artefacts give the same tarball, so a rebuilt list does not change its hash.
    let first = app.sha256.clone();
    let (again, site) = build(&tmp.0, &inp, &key, &[]);
    assert!(again.status.success(), "{}", text(&again));
    assert_eq!(signed_list(&site, &public).app.unwrap().sha256, first);
}

#[test]
fn refuses_a_key_whose_public_half_pocket_ui_does_not_have() {
    let tmp = Tmp::new("foreign");
    let inp = release_out(&tmp.0);
    let (key, _) = throwaway_key(&tmp.0);
    let site = tmp.0.join("site");
    let out = run(&["--repo", "owner/pocket-ui", "--in", s(&inp), "--out", s(&site), "--key", s(&key)]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("PUBLIC_KEY"), "{}", text(&out));
    assert!(!site.exists(), "nothing written");

    let out = run(&["--repo", "owner/pocket-ui", "--in", s(&inp), "--out", s(&site), "--key", s(&tmp.0.join("none.key"))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("no signing key"), "{}", text(&out));
    let out = run(&["--in", s(&inp), "--out", s(&site), "--key", s(&key)]);
    assert_eq!(out.status.code(), Some(2), "no --repo: {}", text(&out));
}

#[test]
fn refuses_programs_of_another_architecture_or_version() {
    let tmp = Tmp::new("programs");
    let inp = release_out(&tmp.0);
    let (key, _) = throwaway_key(&tmp.0);
    std::fs::write(inp.join("bin/pocket-core"), fake_program(62, &format!("pocket-core {VERSION}"))).unwrap();
    let (out, _) = build(&tmp.0, &inp, &key, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("not an aarch64 program"), "{}", text(&out));

    std::fs::write(inp.join("bin/pocket-core"), fake_program(183, "pocket-core 9.9.8")).unwrap();
    let (out, _) = build(&tmp.0, &inp, &key, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("does not carry version 9.9.9"), "{}", text(&out));

    std::fs::write(inp.join("bin/pocket-core"), b"#!/bin/sh\necho pocket-core 9.9.9\n").unwrap();
    let (out, _) = build(&tmp.0, &inp, &key, &[]);
    assert!(text(&out).contains("not a 64-bit little-endian ELF"), "{}", text(&out));
}

#[test]
fn refuses_a_file_github_releases_would_not_take_and_plain_http() {
    let tmp = Tmp::new("limits");
    let inp = release_out(&tmp.0);
    let (key, _) = throwaway_key(&tmp.0);
    // 2 GiB without writing them: a sparse file, refused before anything reads it.
    let img = std::fs::OpenOptions::new().write(true).open(inp.join(format!("pocket-ui-{VERSION}-rp-mini-v2.img.xz"))).unwrap();
    img.set_len(2 * 1024 * 1024 * 1024).unwrap();
    let (out, _) = build(&tmp.0, &inp, &key, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("under 2 GiB"), "{}", text(&out));

    let (out, _) = build(&tmp.0, &inp, &key, &["--base-url", "http://mirror.example/v9.9.9"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("must be on https"), "{}", text(&out));
}

#[test]
fn carries_the_system_update_of_the_live_list_over() {
    let tmp = Tmp::new("previous");
    let inp = release_out(&tmp.0);
    let (key, public) = throwaway_key(&tmp.0);
    let live = tmp.0.join("live/update.json");
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    let os_tar = tmp.0.join("live/SYSTEM-20261101.tar");
    std::fs::write(&os_tar, b"a system update").unwrap();
    let url = "https://github.com/owner/pocket-ui/releases/download/v9.9.8/SYSTEM-20261101.tar";
    let made = Command::new(STORE_TOOL).args(["update", s(&key), s(&live), "os", "20261101", s(&os_tar), url]).output().unwrap();
    assert!(made.status.success());

    let (out, site) = build(&tmp.0, &inp, &key, &["--previous", s(&live)]);
    assert!(out.status.success(), "{}", text(&out));
    let m = signed_list(&site, &public);
    assert_eq!(m.os.expect("carried over").url, url);
    assert_eq!(m.app.unwrap().version, VERSION, "the app entry is the new one");

    // A live list that was changed after signing is not built upon.
    let mut json = std::fs::read(&live).unwrap();
    json.extend_from_slice(b" ");
    std::fs::write(&live, json).unwrap();
    let (out, _) = build(&tmp.0, &inp, &key, &["--previous", s(&live)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("not signed by the expected key"), "{}", text(&out));
}

#[test]
fn check_downloads_every_file_and_finds_a_missing_or_changed_one() {
    let tmp = Tmp::new("check");
    let inp = release_out(&tmp.0);
    let (key, _) = throwaway_key(&tmp.0);
    // The "release": a folder the list points to with file://, which --check fetches through curl like a device.
    let release = tmp.0.join("release");
    std::fs::create_dir_all(&release).unwrap();
    let base = format!("file://{}", release.display());
    let (out, site) = build(&tmp.0, &inp, &key, &["--base-url", &base]);
    assert!(out.status.success(), "{}", text(&out));
    for line in std::fs::read_to_string(site.join("github/assets.txt")).unwrap().lines() {
        let p = Path::new(line);
        std::fs::copy(p, release.join(p.file_name().unwrap())).unwrap();
    }
    let list = site.join("updates/stable/update.json");
    let out = run(&["--check", s(&list)]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(text(&out).matches("  ok ").count(), 3, "app, image, install package: {}", text(&out));

    std::fs::write(release.join(format!("pocket-ui-{VERSION}-rp-mini-v2.img.xz")), vec![8u8; 50_000]).unwrap();
    std::fs::remove_file(release.join(format!("pocket-ui-{VERSION}-rocknix-install.zip"))).unwrap();
    let out = run(&["--check", s(&list)]);
    assert_eq!(out.status.code(), Some(1));
    let t = text(&out);
    assert!(t.contains("CHANGED  sd_image"), "{t}");
    assert!(t.contains("MISSING  install_zip"), "{t}");
    assert!(t.contains("do not deploy"), "{t}");
}
