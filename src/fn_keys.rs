//! Who handles the volume keys. On ROCKNIX `input.service` runs `input_sense`, which listens to every input
//! device with `evtest`:
//! - Vol+ and Vol- alone call `volume up|down` (5 % steps, repeating while held) and send a notification.
//! - With a function modifier held they do something else. On SM8250 modifier A is HOME and modifier B is
//!   START (`quirks/platforms/SM8250/070-modifiers`): HOME + Vol = `brightness up|down`, START + Vol =
//!   `ledcontrol` (LEDs on/off), HOME + START + Vol = `wifictl enable|disable`.
//!
//! pocket-ui reads the same key events (without grabbing them), so it never has to change the volume itself
//! when `input_sense` runs, it only draws the on-screen display. For START and SELECT + Vol we want
//! brightness and colour temperature, which pocket-ui does itself (also while a game runs). To keep
//! `input_sense` out of the way, three `system.cfg` keys are set once, as ROCKNIX's own `set_setting` does (under its lock, `system_cfg`):
//!
//! | key | value | effect |
//! |---|---|---|
//! | `key.function.b` | `BTN_S[TE][AL]*` | the glob matches BTN_START and BTN_SELECT, not BTN_SOUTH: both are modifier B |
//! | `key.function.b.up` | `true` | modifier B + Vol+ does nothing (instead of `ledcontrol`) |
//! | `key.function.b.down` | `true` | modifier B + Vol- does nothing (instead of `ledcontrol poweroff`) |
//!
//! So SELECT + Vol no longer changes the volume, and START + Vol no longer flips LEDs. `input_sense` reads
//! these keys when it starts, so `input.service` is restarted once, after they were written. Keys that are
//! already set are left alone. To undo: delete the three lines from `/storage/.config/system/configs/system.cfg`
//! and `systemctl restart input`.

use std::path::Path;
use std::process::{Command, Stdio};

pub const KEYS: [(&str, &str); 3] = [
    ("key.function.b", "BTN_S[TE][AL]*"),
    ("key.function.b.up", "true"),
    ("key.function.b.down", "true"),
];

/// True if a process named `input_sense` runs (`/proc/<pid>/comm`; `root` is "/" except in tests).
pub fn rocknix_handles_volume(root: &Path) -> bool {
    std::fs::read_dir(root.join("proc")).into_iter().flatten().flatten().any(|e| {
        e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())
            && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "input_sense")
    })
}

/// The keys of `KEYS` whose current value (as returned by `get`) is empty.
pub fn missing(get: impl Fn(&str) -> String) -> Vec<(&'static str, &'static str)> {
    KEYS.iter().copied().filter(|(k, _)| get(k).trim().is_empty()).collect()
}

fn shell(script: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .arg("fn_keys")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Writes the missing keys and restarts `input.service` if anything changed. Returns what was written.
/// Slow (shell and systemctl): call it from a thread. Does nothing unless `input_sense` is running.
pub fn ensure() -> Result<Vec<&'static str>, String> {
    if !rocknix_handles_volume(Path::new("/")) {
        return Ok(Vec::new());
    }
    let get = |key: &str| shell(r#". /etc/profile.d/001-functions; get_setting "$1""#, &[key]).unwrap_or_default();
    // If modifier B is something the owner chose, SELECT + Vol keeps its old meaning; touch nothing.
    let b = get(KEYS[0].0);
    if !b.trim().is_empty() && b.trim() != KEYS[0].1 {
        return Err(format!("key.function.b is {}: left alone", b.trim()));
    }
    let todo = missing(get);
    for (key, value) in &todo {
        // Under ROCKNIX's lock, as its set_setting (`system_cfg`).
        crate::system_cfg::SystemCfg::rocknix().set(key, value).map_err(|e| format!("couldn't write {key}: {e}"))?;
    }
    if !todo.is_empty() {
        let restarted = Command::new("systemctl")
            .args(["restart", "input"])
            .stdin(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if !restarted.success() {
            return Err("wrote the keys, but `systemctl restart input` failed: they apply at the next start".into());
        }
    }
    Ok(todo.into_iter().map(|(k, _)| k).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn the_modifier_glob_matches_start_and_select_but_not_the_face_buttons() {
        // The pattern input_sense builds: '*('${KEYB_MODIFIER}'), value *' used as a case glob.
        fn glob(pattern: &str, text: &str) -> bool {
            // tiny matcher for the pieces used here: literal, [..] sets and *
            fn m(p: &[u8], t: &[u8]) -> bool {
                match p.first() {
                    None => t.is_empty(),
                    Some(b'*') => (0..=t.len()).any(|i| m(&p[1..], &t[i..])),
                    Some(b'[') => {
                        let end = p.iter().position(|c| *c == b']').unwrap();
                        !t.is_empty() && p[1..end].contains(&t[0]) && m(&p[end + 1..], &t[1..])
                    }
                    Some(c) => !t.is_empty() && t[0] == *c && m(&p[1..], &t[1..]),
                }
            }
            m(pattern.as_bytes(), text.as_bytes())
        }
        let pattern = format!("*({}), value *", KEYS[0].1);
        let line = |name: &str| format!("Event: time 1.0, type 1 (EV_KEY), code 1 ({name}), value 1");
        assert!(glob(&pattern, &line("BTN_START")));
        assert!(glob(&pattern, &line("BTN_SELECT")));
        for other in ["BTN_SOUTH", "BTN_EAST", "BTN_MODE", "BTN_TL", "BTN_THUMBL", "KEY_VOLUMEUP"] {
            assert!(!glob(&pattern, &line(other)), "{other}");
        }
    }

    #[test]
    fn only_unset_keys_are_written() {
        assert_eq!(missing(|_| String::new()).len(), 3);
        let set = |k: &str| if k == "key.function.b" { "BTN_S[TE][AL]*".to_string() } else { String::new() };
        assert_eq!(missing(set), [KEYS[1], KEYS[2]]);
        assert!(missing(|k| KEYS.iter().find(|(n, _)| *n == k).unwrap().1.to_string()).is_empty());
    }

    #[test]
    fn input_sense_is_found_by_its_process_name() {
        let root = TestDir::new("fnkeys");
        std::fs::create_dir_all(root.join("proc/123")).unwrap();
        std::fs::write(root.join("proc/123/comm"), "bash\n").unwrap();
        assert!(!rocknix_handles_volume(&root));
        std::fs::create_dir_all(root.join("proc/456")).unwrap();
        std::fs::write(root.join("proc/456/comm"), "input_sense\n").unwrap();
        assert!(rocknix_handles_volume(&root));
        assert!(!rocknix_handles_volume(&root.join("nothing")));
    }
}
