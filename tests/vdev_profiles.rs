//! Check virtual device profiles and userland selection without Docker or touching /sys.
use std::process::Command;

#[test]
fn h700_entrypoint_matches_the_device_profiles() {
    let source = std::fs::read_to_string(format!("{}/docker/vdev/entrypoint.sh", std::env::var("CARGO_MANIFEST_DIR").unwrap())).unwrap();
    let start = source.find("SOC=qcom,sm8250\n").expect("profile defaults");
    let end = source[start..].find("\nexport VDEV_SCREEN=").expect("end of device selection") + start;
    let script = format!(
        "{}\nprintf '%s|%s|%s|%s|%s|%s|%s' \"$SCREEN\" \"$PANEL\" \"$MODEL\" \"$COMPAT\" \"$LID\" \"$SOC\" \"$VDEV_TOUCH\"",
        &source[start..end]
    );
    for (name, panel, lid) in [("rg28xx", "480x640", "0"), ("rg35xx-sp", "640x480", "1")] {
        let profile = pocket_ui::device::from_override(name);
        let out = Command::new("sh").args(["-c", &script]).env("VDEV_DEVICE", name).output().unwrap();
        assert!(out.status.success());
        let line = String::from_utf8(out.stdout).unwrap();
        let fields: Vec<_> = line.split('|').collect();
        assert_eq!(
            fields,
            [
                "640x480",
                panel,
                profile.model.unwrap().name(),
                &format!("anbernic,{name}"),
                lid,
                "allwinner,sun50i-h700",
                "0"
            ]
        );
        assert_eq!(profile.screen(), (640, 480));
        assert!(!profile.has_touch());
        assert_eq!(profile.has_lid(), lid == "1");
        assert_eq!(pocket_ui::scaled_layout::logical_size(profile.screen()), (1280, 960));
    }
}

#[test]
fn selected_rocknix_userland_reaches_the_virtual_device() {
    let out = Command::new("python3")
        .args(["-I", concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/test-vdev-images.py")])
        .output()
        .expect("run the isolated vdev image regression tests");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
