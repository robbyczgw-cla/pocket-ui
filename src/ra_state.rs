//! Confirmation for the ROCKNIX RetroArch pause menu's Save and Load commands.

use std::collections::BTreeMap;
use std::io;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::fsutil::{self, Stamp};

const REPLY_WAIT: Duration = Duration::from_millis(750);
const SAVE_WAIT: Duration = Duration::from_secs(4);
const POLL: Duration = Duration::from_millis(150);
const STATE_DIRECTORY: &str = concat!("GET_CONFIG_PARAM ", "savestate_directory");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Save,
    Load,
}

impl Action {
    pub fn unconfirmed(self) -> &'static str {
        match self {
            Self::Save => "RetroArch didn't confirm a new save. Check before quitting.",
            Self::Load => "Couldn't confirm that RetroArch received the load request and had a save to load.",
        }
    }
}

/// Runs off the UI thread after the game regains focus. A save must produce a changed file. For load,
/// the UDP reply confirms reception only. RetroArch shows the result of loading in the game.
pub fn check(action: Action, state: &Path, pgid: i32) -> bool {
    let Ok(socket) = crate::control::retroarch_socket(crate::control::RETROARCH_CMD_PORT) else { return false };
    if socket.set_read_timeout(Some(REPLY_WAIT)).is_err() {
        return false;
    }
    let mut buf = [0; 8192];
    let dir = if socket.send(STATE_DIRECTORY.as_bytes()).is_ok() {
        receive(&socket, &mut buf)
            .and_then(|s| s.strip_prefix(STATE_DIRECTORY))
            .and_then(|s| s.strip_prefix(' '))
            .map(str::trim_end)
            .filter(|s| Path::new(s).is_absolute())
            .map(PathBuf::from)
    } else {
        None
    };
    let Some(dir) = dir.as_deref().or_else(|| state.parent()) else { return false };
    if !crate::control::group_alive(pgid) {
        return false;
    }
    let prefix = state.file_name().and_then(|s| s.to_str());
    let writer = crate::standalone::find_pid(Some(pgid), "retroarch");
    perform(&socket, action, dir, prefix, SAVE_WAIT, writer)
}

fn receive<'a>(socket: &UdpSocket, buf: &'a mut [u8]) -> Option<&'a str> {
    let len = socket.recv(buf).ok()?;
    std::str::from_utf8(&buf[..len]).ok()
}

fn perform(socket: &UdpSocket, action: Action, dir: &Path, prefix: Option<&str>, wait: Duration, writer: Option<i32>) -> bool {
    let before = prefix.and_then(|prefix| state_versions(dir, prefix).ok());
    match action {
        Action::Save => {
            socket.send(b"SAVE_STATE").is_ok()
                && before.zip(prefix).is_some_and(|(before, prefix)| wait_for_save(dir, prefix, &before, wait, writer))
        }
        Action::Load => {
            // RetroArch parses multiple commands in a datagram. A status reply proves that this same
            // datagram reached its listener. LOAD_STATE itself has no completion reply.
            if socket.send(b"LOAD_STATE\nGET_STATUS").is_err() {
                return false;
            }
            let mut buf = [0; 512];
            let received = receive(socket, &mut buf).is_some_and(|s| {
                s.starts_with("GET_STATUS PLAYING ") || s.starts_with("GET_STATUS PAUSED ")
            });
            received && before.is_some_and(|states| !states.is_empty())
        }
    }
}

fn state_versions(dir: &Path, prefix: &str) -> io::Result<BTreeMap<PathBuf, Stamp>> {
    let mut states = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(suffix) = name.to_str().and_then(|name| name.strip_prefix(prefix)) else { continue };
        // Slot zero, numbered slots (including incremental saves), and the automatic slot.
        if suffix != ".auto" && !suffix.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if !entry.metadata().is_ok_and(|m| m.is_file() && m.len() > 0) {
            continue;
        }
        let path = entry.path();
        if let Some(stamp) = fsutil::stamp(&path) {
            states.insert(path, stamp);
        }
    }
    Ok(states)
}

fn wait_for_save(dir: &Path, prefix: &str, before: &BTreeMap<PathBuf, Stamp>, wait: Duration, writer: Option<i32>) -> bool {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if let Ok(states) = state_versions(dir, prefix) {
            for (path, stamp) in states {
                let old = before.get(&path).copied();
                if old != Some(stamp) && fsutil::wait_new_version(&path, old, deadline.saturating_duration_since(Instant::now()), POLL, writer) {
                    return true;
                }
            }
        }
        std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::net::Ipv4Addr;
    use std::sync::mpsc;

    fn listener() -> (UdpSocket, UdpSocket) {
        let listener = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let socket = crate::control::retroarch_socket(listener.local_addr().unwrap().port()).unwrap();
        for s in [&listener, &socket] {
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        }
        (socket, listener)
    }

    #[test]
    fn save_requires_a_finished_new_state_for_this_game() {
        let dir = TestDir::new("ra-save");
        assert!(state_versions(&dir.join("missing"), "Game.state").is_err());
        let old = dir.join("Game.state");
        std::fs::write(&old, b"old").unwrap();
        let (socket, listener) = listener();
        let (changed, wait) = mpsc::channel();
        let new = dir.join("Game.state1");
        let thumb = dir.join("Game.state.png");
        let unrelated = dir.join("Different game.state");
        let worker = std::thread::spawn(move || {
            let mut buf = [0; 64];
            let (len, _) = listener.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..len], b"SAVE_STATE");
            std::fs::write(thumb, b"not a state").unwrap();
            std::fs::write(unrelated, b"other game").unwrap();
            std::fs::write(&new, []).unwrap();
            changed.send(()).unwrap();
            let (len, _) = listener.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..len], b"SAVE_STATE");
            std::fs::write(new, b"new state").unwrap();
        });
        assert!(!perform(&socket, Action::Save, &dir, Some("Game.state"), POLL * 2, None));
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(perform(&socket, Action::Save, &dir, Some("Game.state"), Duration::from_secs(2), None));
        worker.join().unwrap();
    }

    #[test]
    fn load_needs_a_state_and_a_reply_but_does_not_change_the_file() {
        let dir = TestDir::new("ra-load");
        let (socket, listener) = listener();
        socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        let worker = std::thread::spawn(move || {
            let mut buf = [0; 64];
            for reply in [true, true, false] {
                let (len, peer) = listener.recv_from(&mut buf).unwrap();
                assert_eq!(&buf[..len], b"LOAD_STATE\nGET_STATUS");
                if reply {
                    listener.send_to(b"GET_STATUS PLAYING Game Boy,Game,crc32=1234", peer).unwrap();
                }
            }
        });
        let load = || perform(&socket, Action::Load, &dir, Some("Game.state"), Duration::ZERO, None);
        assert!(!load(), "a reply without a save is insufficient");
        let state = dir.join("Game.state");
        std::fs::write(&state, b"saved").unwrap();
        let before = fsutil::stamp(&state);
        assert!(load(), "RetroArch received the request and has a state");
        assert!(!load(), "a state without a reply is insufficient");
        assert_eq!(fsutil::stamp(&state), before);
        worker.join().unwrap();
    }
}
