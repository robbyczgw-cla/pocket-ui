//! The port reuses the Brick transaction, component transport and boot rollback.
use super::*;
use std::os::unix::fs::MetadataExt;

fn port_package(dir: &Path, version: &str, marker: bool) -> Release {
    let (_, archive) = app_tarball(dir, version, Some(version), "#!/bin/sh\nexit 9\n");
    let src = dir.join(format!("app-src-{version}"));
    std::fs::remove_file(src.join("bin/brick-hw")).unwrap();
    std::fs::remove_dir_all(src.join("msg")).unwrap();
    std::fs::remove_dir_all(src.join("cores")).unwrap();
    if marker { std::fs::write(src.join("DARKOS-PORT"), "1\n").unwrap(); }
    std::fs::write(src.join("asset-packages.json"), r#"{"format":1,"assets":[{"path":"port-data/systems.py","size":4,"sha256":"e629fa6598d732768f7c726b4b621285f9c3b8531e7b019ffb80c267b11c0109e","package":{"version":"1","url":"missing-unchanged.tar.gz","size":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}]}"#).unwrap();
    // Correct hash of the fixture; there is deliberately no remote asset file.
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(src.join("asset-packages.json")).unwrap()).unwrap();
    json["assets"][0]["sha256"] = store::sha256_hex(b"same").into();
    std::fs::write(src.join("asset-packages.json"), serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&src).arg(".").status().unwrap().success());
    let bytes = std::fs::read(&archive).unwrap();
    Release { version: version.into(), url: archive.file_name().unwrap().to_str().unwrap().into(),
        size: bytes.len() as u64, sha256: store::sha256_hex(&bytes), mirrors: vec![], preferred_sources: vec![], notes: String::new() }
}

#[test]
fn darkos_reuses_unchanged_parts_without_download_and_the_real_boot_restores_them() {
    let dir = TestDir::new("darkos-components");
    let p = card(&dir);
    std::fs::create_dir_all(p.join("app/port-data")).unwrap();
    std::fs::write(p.join("app/port-data/systems.py"), b"same").unwrap();
    let inode = std::fs::metadata(p.join("app/port-data/systems.py")).unwrap().ino();
    let before = players_files(&p);
    let rel = port_package(&dir, "2", true);
    let url = format!("file://{}/update.json", dir.display());
    super::super::brick::install_folder_with(&TOOLS, &rel, &url, &p, &Cancel::new(), &|_| {}, &plenty,
        &crate::untar::unpack_file_skipping, true).unwrap();
    assert_eq!(release_of(&p, "app"), "2");
    assert_eq!(std::fs::metadata(p.join("app/port-data/systems.py")).unwrap().ino(), inode);
    assert_eq!(players_files(&p), before, "saves, settings and games stay untouched");
    // New start exits before confirming; the exact shared boot script takes version 1 back.
    assert!(Command::new("sh").arg(p.join("start.sh")).env("BRICK_DATA", &p).status().unwrap().success());
    assert_eq!(release_of(&p, "app"), "1");
    assert_eq!(std::fs::metadata(p.join("app/port-data/systems.py")).unwrap().ino(), inode);
    assert_eq!(std::fs::read(p.join("app/port-data/systems.py")).unwrap(), b"same");
}

#[test]
fn darkos_refuses_a_brick_package_before_activation() {
    let dir = TestDir::new("darkos-wrong-package");
    let p = card(&dir);
    std::fs::create_dir_all(p.join("app/port-data")).unwrap();
    std::fs::write(p.join("app/port-data/systems.py"), b"same").unwrap();
    let rel = port_package(&dir, "2", false);
    let url = format!("file://{}/update.json", dir.display());
    let error = super::super::brick::install_folder_with(&TOOLS, &rel, &url, &p, &Cancel::new(), &|_| {}, &plenty,
        &crate::untar::unpack_file_skipping, true).unwrap_err();
    assert!(error.contains("DARKOS-PORT"), "{error}");
    assert_eq!(release_of(&p, "app"), "1");
    assert!(!p.join("storage/pocket-ui/update-pending").exists());
}

#[test]
fn darkos_channel_requires_its_signed_target_and_rejects_firmware() {
    let key = SigningKey::from_bytes(&[79;32]);
    for (target, os, good) in [("darkos-port", "null", true), ("brick", "null", false), ("stable", "null", false),
        ("darkos-port", r#"{"version":"9","url":"os.tar","size":1,"sha256":"00"}"#, false)] {
        let json = format!(r#"{{"format":1,"target":"{target}","os":{os}}}"#);
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(json.as_bytes()).to_bytes());
        let manifest = parse_manifest(json.as_bytes(), &sig, &key.verifying_key().to_bytes()).unwrap();
        assert_eq!(super::super::darkos_manifest(json.as_bytes(), manifest).is_ok(), good);
        let changed = format!("{json} ");
        assert!(parse_manifest(changed.as_bytes(), &sig, &key.verifying_key().to_bytes()).is_err());
    }
}
