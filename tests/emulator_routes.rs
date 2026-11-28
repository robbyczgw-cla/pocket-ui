//! The read-only release check rejects an emulator swap even if either emulator could start a game.
#![cfg(target_os = "linux")]

use std::fs;
use std::process::Command;

#[test]
fn release_probe_checks_the_emulator_and_core_of_every_populated_system() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let xml = dir.join("es_systems.cfg");
    let mut systems = String::from("<systemList>");
    for (system, emulator, core) in [
        ("xbox", "xemu", "xemu-sa"),
        ("ps2", "armsx2", "armsx2-sa"),
        ("gamecube", "dolphin", "dolphin-sa-gc"),
        ("wii", "dolphin", "dolphin-sa-wii"),
        ("3ds", "azahar", "azahar-sa"),
        ("psp", "ppsspp", "ppsspp-sa"),
    ] {
        let roms = dir.join(system);
        fs::create_dir(&roms).unwrap();
        fs::write(roms.join("Demo.iso"), b"fixture").unwrap();
        systems.push_str(&format!(
            r#"<system><name>{system}</name><path>{}</path><extension>.iso</extension>
               <command>true</command><emulators><emulator name="{emulator}"><cores><core default="true">{core}</core></cores></emulator></emulators></system>"#,
            roms.display()
        ));
    }
    systems.push_str("</systemList>");
    fs::write(&xml, systems).unwrap();

    let command = || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.env("POCKETUI_ES_SYSTEMS", &xml)
            .env("POCKETUI_DEVICE", "miniv2")
            .env("POCKETUI_HOST_BIN", dir.join("no-pocket-core"))
            .env("POCKETUI_DATA", dir.join("no-settings"));
        cmd
    };
    let output = command().arg("--emulator-routes").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.as_object().unwrap().len(), 6);
    assert_eq!(rows["xbox"][0], "hakux");
    assert_eq!(rows["psp"][1], "ppsspp-sa");

    let expected = dir.join("expected.json");
    fs::write(&expected, &output.stdout).unwrap();
    let run = || command().arg("--check-emulator-routes").arg(&expected).output().unwrap();
    let matched = run();
    assert!(matched.status.success(), "{}", String::from_utf8_lossy(&matched.stderr));
    rows["xbox"][0] = "xemu".into();
    rows["xbox"][1] = "xemu-sa".into();
    rows.as_object_mut().unwrap().remove("3ds");
    fs::write(&expected, serde_json::to_vec(&rows).unwrap()).unwrap();
    let wrong_emulator = run();
    assert_eq!(wrong_emulator.status.code(), Some(1));
    let bad_reference = || assert_eq!(run().status.code(), Some(1), "an unreadable reference must fail");
    for bytes in [b"".as_slice(), b"{", b"\xff", b"[]"] {
        fs::write(&expected, bytes).unwrap();
        bad_reference();
    }
    fs::remove_file(&expected).unwrap();
    bad_reference();
    let reason = String::from_utf8_lossy(&wrong_emulator.stderr);
    assert!(reason.contains("xbox: expected") && reason.contains("hakux") && reason.contains("3ds: expected"), "{reason}");
}
