//! The Brick standalone card's stand-in helpers, run against pocket-ui's own hardware seam (`RealHw` on a fake root, the
//! same one the updater builds in /tmp, docs/TRIMUI-BRICK-STANDALONE.md): volume, mute, brightness, battery, the Wi-Fi switch
//! and power, with a fake `amixer` and fake sysfs files. `cargo test --features brick --test brick_hwroot`.
#![cfg(all(feature = "brick", target_os = "linux"))]

use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use pocket_ui::system_ctl::{Charge, Hw, PowerAction, RealHw};

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The tests that set the process environment (PATH, BRICK_*) take turns.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn the_stand_ins_answer_pocket_uis_hardware_seam() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let hw = d.join("hwroot");
    for sub in ["usr/bin", "sys/class/backlight/panel", "run/systemd/system", "sys/class/rfkill/rfkill1"] {
        std::fs::create_dir_all(hw.join(sub)).unwrap();
    }
    // The card's own stand-ins, linked the way the updater links them.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/brick/hwroot/usr/bin");
    for e in std::fs::read_dir(&src).unwrap().flatten() {
        symlink(e.path(), hw.join("usr/bin").join(e.file_name())).unwrap();
    }
    // The Brick's battery and radio as sysfs shows them (axp2202-battery, rfkill1 = wlan).
    let bat = d.join("axp2202-battery");
    std::fs::create_dir_all(&bat).unwrap();
    std::fs::write(bat.join("capacity"), "63\n").unwrap();
    std::fs::write(bat.join("status"), "Charging\n").unwrap();
    std::fs::create_dir_all(hw.join("sys/class/power_supply")).unwrap();
    symlink(&bat, hw.join("sys/class/power_supply/battery")).unwrap();
    let rfk = d.join("rfkill");
    std::fs::create_dir_all(rfk.join("rfkill1")).unwrap();
    for (f, v) in [("type", "wlan"), ("soft", "0"), ("hard", "0")] {
        std::fs::write(rfk.join("rfkill1").join(f), format!("{v}\n")).unwrap();
    }
    std::fs::remove_dir_all(hw.join("sys/class/rfkill")).unwrap();
    symlink(&rfk, hw.join("sys/class/rfkill")).unwrap();
    // The panel: pocket-ui writes this file, brick-hw (not run here) would follow it.
    std::fs::write(hw.join("sys/class/backlight/panel/max_brightness"), "255\n").unwrap();
    std::fs::write(hw.join("sys/class/backlight/panel/brightness"), "178\n").unwrap();
    // A fake amixer with one control, and the stand-ins' environment (as the updater exports it).
    let mixer = d.join("mixer-value");
    std::fs::write(&mixer, "28\n").unwrap();
    let amixer = d.join("amixer");
    script(
        &amixer,
        &format!(
            r#"case " $* " in *" -s "*) while IFS= read -r line; do eval '"$0" -c 0 -q '"$line" || exit 1; done; exit ;; esac
case "$*" in
  *cget*"BT Volume"*) exit 1 ;;
  *cget*) echo "numid=4,iface=MIXER,name='digital volume'"; echo "  ; type=INTEGER"; echo "  : values=$(cat {m})" ;;
  *cset*"digital volume"*) for a; do v=$a; done; echo "$v" >{m} ;;
esac"#,
            m = mixer.display()
        ),
    );
    let state = d.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let speaker = d.join("mute");
    std::fs::write(&speaker, "0\n").unwrap();
    let power = d.join("power-state");
    let calls = d.join("calls");
    script(&d.join("poweroff"), &format!("echo poweroff >>{}", calls.display()));
    script(&d.join("ifconfig"), &format!("echo ifconfig $* >>{}", calls.display()));
    let hwtool = d.join("brick-hw");
    script(&hwtool, &format!("case \"$1\" in bl-get) echo 178 ;; *) echo brick-hw $* >>{} ;; esac", calls.display()));
    // SAFETY: one test in this binary, nothing else reads the environment meanwhile.
    unsafe {
        std::env::set_var("PATH", format!("{}:{}", hw.join("usr/bin").display(), std::env::var("PATH").unwrap()));
        std::env::set_var("BRICK_AMIXER", &amixer);
        std::env::set_var("BRICK_STATE", &state);
        std::env::set_var("BRICK_SPEAKER", &speaker);
        std::env::set_var("BRICK_RFKILL", &rfk);
        std::env::set_var("BRICK_POWER_STATE", &power);
        std::env::set_var("BRICK_POWEROFF", d.join("poweroff"));
        std::env::set_var("BRICK_IFCONFIG", d.join("ifconfig"));
        std::env::set_var("BRICK_UDHCPC", "true");
        std::env::set_var("BRICK_HW", &hwtool);
    }

    let real = RealHw::with_root(&hw);
    // What the machine has: each control must be found, or pocket-ui would put a mock in its place.
    assert!(real.has_backlight() && real.has_volume() && real.has_wifi() && real.has_battery() && real.systemd());

    // Battery from the AXP chip's own sysfs.
    let b = real.battery().expect("battery");
    assert_eq!(b.percent, 63);
    assert_eq!(b.charge, Charge::Charging);

    // Backlight: 178 of 255 is 70 %; setting 40 % writes 102.
    assert_eq!(real.brightness(), Some(70));
    real.set_brightness(40);
    assert_eq!(std::fs::read_to_string(hw.join("sys/class/backlight/panel/brightness")).unwrap().trim(), "102");
    real.persist_brightness(40);
    assert_eq!(std::fs::read_to_string(state.join("brick-brightness")).unwrap().trim(), "40");

    // With no exact target yet, raw 28 is represented by 19% on the gentler lower curve.
    // Normal restore writes the exact target before the UI starts.
    assert_eq!(real.volume(), Some(19));
    real.set_volume(50);
    assert_eq!(std::fs::read_to_string(&mixer).unwrap().trim(), "20");
    assert_eq!(real.volume(), Some(50), "the remembered percentage is given back");
    real.set_volume(44);
    assert_eq!(real.volume(), Some(44), "show the exact new target, not the curve-2 rollback percentage");
    assert_ne!(std::fs::read_to_string(state.join("brick-volume-curve2")).unwrap().trim(), "44");
    // Polls must not expose intermediate hardware ramp ticks or require a process per held key.
    std::fs::write(&mixer, "60\n").unwrap();
    assert_eq!(real.volume(), Some(44), "the player's target survives an in-flight fade");
    std::fs::write(state.join("brick-muted"), "").unwrap();
    assert_eq!(real.muted(), Some(true), "the slider marker, not unrelated sysfs, owns mute");
    std::fs::remove_file(state.join("brick-muted")).unwrap();
    real.set_volume(0);
    assert_eq!(std::fs::read_to_string(&mixer).unwrap().trim(), "63");
    real.set_volume(100);
    assert_eq!(std::fs::read_to_string(&mixer).unwrap().trim(), "0");
    assert_eq!(real.muted(), Some(false));
    std::fs::write(&speaker, "1\n").unwrap();
    assert_eq!(real.muted(), Some(false), "a muted speaker amplifier must not freeze headphone/Bluetooth keys");
    real.set_volume(95);
    assert_eq!(real.volume(), Some(95));
    assert_eq!(std::fs::read_to_string(&mixer).unwrap().trim(), "2");

    // The Wi-Fi switch: the soft block follows, and the interface.
    real.set_wifi(false).unwrap();
    assert_eq!(std::fs::read_to_string(rfk.join("rfkill1/soft")).unwrap().trim(), "1");
    assert_eq!(real.wifi(), Some(false));
    real.set_wifi(true).unwrap();
    assert_eq!(real.wifi(), Some(true));

    // Power: suspend writes "mem" and puts the screen back; poweroff calls the firmware's.
    real.power(PowerAction::Sleep).unwrap();
    assert_eq!(std::fs::read_to_string(&power).unwrap().trim(), "mem");
    // (poweroff and reboot go through RealHw::power's shutdown log under /storage; the stand-in itself is called here)
    assert!(std::process::Command::new(hw.join("usr/bin/systemctl")).arg("poweroff").status().unwrap().success());
    let log = std::fs::read_to_string(&calls).unwrap();
    assert!(log.contains("brick-hw bl-set 0") && log.contains("brick-hw bl-set 178"), "{log}");
    assert!(log.contains("ifconfig wlan0 down") && log.contains("ifconfig wlan0 up") && log.contains("poweroff"), "{log}");
}

/// The clock page on the card: zone data and the `etc/localtime` link the start script builds in the stand-in root, the
/// zone's name kept on the card, and network time through the two stand-ins (`timedatectl`, `systemctl restart
/// systemd-timesyncd` over a fake ntpd).
#[test]
fn the_cards_clock_answers_the_time_page() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    use pocket_ui::system_ctl::NtpState;
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let hw = d.join("hwroot");
    // The card's zone data, linked into the root the way the start script links it; the start script's own link
    // building is scripts/test-brick-card.sh's job.
    let zones = d.join("zoneinfo");
    for z in ["Europe/Berlin", "Europe/London", "America/New_York", "UTC"] {
        let f = zones.join(z);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"TZif2 fake").unwrap();
    }
    std::fs::create_dir_all(hw.join("usr/share")).unwrap();
    std::fs::create_dir_all(hw.join("etc")).unwrap();
    std::fs::create_dir_all(hw.join("usr/bin")).unwrap();
    symlink(&zones, hw.join("usr/share/zoneinfo")).unwrap();
    symlink("../usr/share/zoneinfo/Europe/London", hw.join("etc/localtime")).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/brick/hwroot/usr/bin");
    for f in ["systemctl", "timedatectl"] {
        symlink(src.join(f), hw.join("usr/bin").join(f)).unwrap();
    }
    let state = d.join("state");
    std::fs::create_dir_all(&state).unwrap();
    script(&d.join("ntpd-ok"), "exit 0");
    // SAFETY: the tests of this binary that read the environment use their own roots; a foreign HW_ROOT never matches them.
    unsafe {
        std::env::set_var("POCKETUI_HW_ROOT", &hw);
        std::env::set_var("PATH", format!("{}:{}", hw.join("usr/bin").display(), std::env::var("PATH").unwrap()));
        std::env::set_var("BRICK_STATE", &state);
        std::env::set_var("BRICK_NTPD", d.join("ntpd-ok"));
    }
    let real = RealHw::with_root(&hw);
    assert_eq!(real.timezone().as_deref(), Some("Europe/London"), "the link the start script built");
    assert!(!real.timezone_chosen(), "nobody chose it yet");
    let listed = real.zones();
    assert!(listed.iter().any(|z| z == "Europe/Berlin") && listed.iter().any(|z| z == "UTC"), "{listed:?}");

    real.set_timezone("Europe/Berlin").expect("the card takes the zone");
    assert_eq!(real.timezone().as_deref(), Some("Europe/Berlin"));
    assert!(real.timezone_chosen());
    assert_eq!(
        std::fs::read_to_string(hw.join("storage/.config/pocket-ui/timezone")).unwrap().trim(),
        "Europe/Berlin",
        "the name is kept on the card (a FAT card takes no symlink); the start script rebuilds the link from it"
    );
    assert!(real.set_timezone("Europe/Nowhere").is_err(), "a zone the card has no file for is refused");
    assert_eq!(real.timezone().as_deref(), Some("Europe/Berlin"), "and the old zone stays");

    // Network time: offered (an ntpd answers), not synced until the one-shot set the clock.
    assert_eq!(real.ntp(), NtpState::Waiting);
    real.sync_time();
    assert_eq!(real.ntp(), NtpState::Synced);
    // Without an ntpd the page must be told plainly, never a made-up sync.
    unsafe { std::env::set_var("BRICK_NTPD", d.join("no-ntpd-here")) }
    std::fs::remove_file(state.join("ntp-synced")).unwrap();
    assert_eq!(real.ntp(), NtpState::Off);
    real.sync_time();
    assert_eq!(real.ntp(), NtpState::Off, "a failed sync writes no note");
    unsafe { std::env::remove_var("POCKETUI_HW_ROOT") }
}

/// What the `nmcli` stand-in prints must be what pocket-ui's own parsers read (`wifi::parse_*`), checked with a fake `wpa_cli`
/// that prints the real one's formats.
#[test]
fn the_nmcli_stand_in_speaks_what_pocket_uis_wifi_parsers_read() {
    use pocket_ui::wifi::{parse_active_ssid, parse_profiles, parse_scan, parse_ssid_hidden};
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("networks"), "0\tCafe\tany\t\n1\tHome Net\tany\t\n").unwrap();
    std::fs::write(d.join("current"), "1\n").unwrap();
    std::fs::write(
        d.join("scan"),
        "aa:01\t2422\t-65\t[WPA2-PSK-CCMP][ESS]\tHome Net\naa:02\t2437\t-85\t[WPA2-PSK-CCMP][ESS]\tCafe\naa:03\t2437\t-50\t[ESS]\tOpen: Net\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.join("rf/rfkill1")).unwrap();
    for (f, v) in [("type", "wlan"), ("soft", "0"), ("hard", "0")] {
        std::fs::write(d.join("rf/rfkill1").join(f), format!("{v}\n")).unwrap();
    }
    let nmcli = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/brick/hwroot/usr/bin/nmcli");
    let run = |args: &[&str]| -> String {
        let out = std::process::Command::new(&nmcli)
            .args(args)
            .env("FAKE_WPA", d)
            .env("BRICK_WPA_CLI", Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/brick-fake-wpa-cli.sh"))
            .env("BRICK_RFKILL", d.join("rf"))
            .env("BRICK_SCAN_WAIT", "0")
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    };
    // The Wi-Fi page's scan (wifi.rs `scan_until`): in-use first, then by signal; the open network is not secure.
    let nets = parse_scan(&run(&["-t", "-f", "SSID,SIGNAL,SECURITY,IN-USE", "dev", "wifi", "list", "--rescan", "yes"]));
    let names: Vec<&str> = nets.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, ["Home Net", "Open: Net", "Cafe"]);
    assert!(nets[0].active && nets[0].secure && nets[0].signal == 70);
    assert!(!nets[1].secure && nets[1].signal == 100);
    assert_eq!(parse_active_ssid(&run(&["-t", "-f", "ACTIVE,SSID", "device", "wifi", "list", "--rescan", "no"])).as_deref(), Some("Home Net"));
    // Saved networks and their real names.
    let saved = parse_profiles(&run(&["-t", "-f", "NAME,UUID,TYPE,ACTIVE,STATE", "connection", "show"]));
    assert_eq!(saved.len(), 2);
    let home = saved.iter().find(|s| s.ssid == "Home Net").unwrap();
    assert!(home.active && !home.connecting && home.uuid == "brick-net-1");
    assert_eq!(parse_ssid_hidden(&run(&["-g", "802-11-wireless.ssid,802-11-wireless.hidden", "connection", "show", "uuid", &home.uuid])), Some(("Home Net".into(), false)));
}
