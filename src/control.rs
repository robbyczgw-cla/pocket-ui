//! Talking to the running game: RetroArch network commands and sway window focus.

use std::net::UdpSocket;
use std::process::Command;
use std::time::Duration;

pub const RETROARCH_APP_ID: &str = "com.libretro.RetroArch";
pub const OWN_APP_ID: &str = "pocket-ui";

/// Sends one RetroArch network command (e.g. "SAVE_STATE", "LOAD_STATE", "QUIT") to localhost:55355.
pub fn retroarch(cmd: &str) -> bool {
    UdpSocket::bind("127.0.0.1:0").and_then(|s| s.send_to(cmd.as_bytes(), "127.0.0.1:55355")).is_ok()
}

/// Ends RetroArch with SIGTERM: it quits cleanly and writes the auto state (BusyBox `pkill -x` does not
/// match it). While paused behind the quick menu RetroArch's loop sleeps and acts on neither SIGTERM nor
/// the QUIT command, so it gets focus (not fullscreen, the menu stays visible) and exits within ~250 ms.
pub fn quit_retroarch() -> bool {
    let sent = crate::proc::status_with_timeout(Command::new("pkill").args(["-TERM", "retroarch"]), SWAY_TIMEOUT).is_ok_and(|s| s.success());
    sent && swaymsg(&format!(r#"[app_id="{RETROARCH_APP_ID}"] focus"#))
}

/// pocket-core's command for the pause menu's Quit with Settings > Remember where I quit off: the battery save, no
/// new auto-resume state.
pub const QUIT_WITHOUT_SAVING: &str = "quit nosave";

/// Sends a command line ("save 3", "load 3", "slot 3", "set <key> <value>", "reload", "quit") to the running pocket-core.
pub fn pocket_core(cmd: &str) -> bool {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    // pocket-core holds the FIFO open for as long as it lives, even while frozen. Non-blocking, so
    // a FIFO left behind by a crashed pocket-core fails right away (ENXIO) instead of hanging the UI.
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/tmp/pocket-core.cmd")
        .and_then(|mut f| writeln!(f, "{cmd}"))
        .is_ok()
}

/// After a pocket-core game: asks `bin` to put back the CPU and fan settings that a killed pocket-core
/// left behind. Does nothing when there is nothing to undo.
pub fn restore_system(bin: &'static str) {
    restore_system_then(bin, || {});
}

/// `restore_system`, then `done` on the same thread once it has finished (or couldn't run): pocket-ui's fan outside
/// games waits for it, so it notes the system's trips and not what a killed game left (`perf_fan`).
pub fn restore_system_then(bin: &'static str, done: impl FnOnce() + Send + 'static) {
    // The Steam Deck's system owns CPU and fan: pocket-core changed none of it (docs/STEAMDECK.md).
    if crate::device::current().host_owned() {
        return done();
    }
    std::thread::spawn(move || {
        let _ = Command::new(bin).arg("--restore-system").status();
        done();
    });
}

/// How long a `swaymsg` may take before the UI thread stops waiting for it (it answers in ~10 ms).
pub const SWAY_TIMEOUT: Duration = Duration::from_millis(1500);

/// Opening the pause menu waits for `get_tree`, `grim` and the focus change this long in all; a helper that
/// hangs costs the menu its background picture (or the window id), not a frozen UI. `get_tree` gets at most
/// `MENU_TREE`, and `MENU_SHOW` is kept back for the focus change.
pub const MENU_HELPERS: Duration = Duration::from_millis(400);
pub const MENU_TREE: Duration = Duration::from_millis(250);
pub const MENU_SHOW: Duration = Duration::from_millis(60);
/// How long the pause menu waits for pocket-core to say it has paused before it covers the game anyway
/// (`fast_switch::wait_paused`; it takes at most a frame, a 3D core's slow one included).
pub const MENU_PAUSE: Duration = Duration::from_millis(100);

fn swaymsg(cmd: &str) -> bool {
    swaymsg_within(cmd, SWAY_TIMEOUT)
}

/// One sway command (`output * power on`) through sway's IPC socket, `swaymsg` where there is none; within
/// `SWAY_TIMEOUT`. True when sway says it worked.
pub fn sway_command(cmd: &str) -> bool {
    swaymsg(cmd)
}

fn swaymsg_within(cmd: &str, timeout: Duration) -> bool {
    match ipc::request(ipc::RUN_COMMAND, cmd, timeout) {
        Ok(reply) => ipc::all_succeeded(&reply),
        // No socket to talk to (not under sway, or `POCKETUI_SWAY_IPC=0`): the program, as before.
        Err(e) if ipc::unreachable(&e) => sway_ok(Command::new("swaymsg").arg(cmd), timeout),
        Err(_) => false,
    }
}

/// sway's IPC socket spoken directly (`SWAYSOCK`, which the launcher sets), instead of starting `swaymsg` for every
/// focus change: a process start, two pipe threads and the exit poll cost the UI thread 5 to 15 ms each time on the
/// device, more while a game tears down next to it; the socket answers in well under a millisecond. The protocol
/// is i3's: `i3-ipc`, payload length and message type as native-endian u32, the payload; the answer has the same
/// header and JSON. `POCKETUI_SWAY_IPC=0` goes back to `swaymsg` (to compare on the device).
/// `display_out` reads the outputs and listens to their events through it too.
pub mod ipc {
    use std::io::{self, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    pub const RUN_COMMAND: u32 = 0;
    pub const GET_WORKSPACES: u32 = 1;
    pub const SUBSCRIBE: u32 = 2;
    pub const GET_OUTPUTS: u32 = 3;
    pub const GET_TREE: u32 = 4;
    /// An event's type has this bit set; `EVENT_OUTPUT` is an output appearing, going or changing.
    pub const EVENT_OUTPUT: u32 = 0x8000_0001;
    pub const MAGIC: &[u8; 6] = b"i3-ipc";

    /// Sends one message and returns the answer's payload, all within `timeout`.
    pub fn request(kind: u32, payload: &str, timeout: Duration) -> io::Result<Vec<u8>> {
        let path = socket().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no SWAYSOCK"))?;
        request_at(&path, kind, payload, timeout)
    }

    pub fn socket() -> Option<std::path::PathBuf> {
        if std::env::var_os("POCKETUI_SWAY_IPC").is_some_and(|v| v == "0") {
            return None;
        }
        std::env::var_os("SWAYSOCK").filter(|p| !p.is_empty()).map(Into::into)
    }

    pub fn request_at(path: &std::path::Path, kind: u32, payload: &str, timeout: Duration) -> io::Result<Vec<u8>> {
        if timeout.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no time left"));
        }
        let deadline = Instant::now() + timeout;
        let mut s = UnixStream::connect(path)?;
        let left = || {
            let d = deadline.saturating_duration_since(Instant::now());
            if d.is_zero() {
                Err(io::Error::new(io::ErrorKind::TimedOut, "sway did not answer in time"))
            } else {
                Ok(d)
            }
        };
        s.set_write_timeout(Some(left()?))?;
        let mut msg = Vec::with_capacity(14 + payload.len());
        msg.extend_from_slice(MAGIC);
        msg.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
        msg.extend_from_slice(&kind.to_ne_bytes());
        msg.extend_from_slice(payload.as_bytes());
        s.write_all(&msg)?;
        let mut read_exact = |buf: &mut [u8]| -> io::Result<()> {
            let mut at = 0;
            while at < buf.len() {
                // macOS refuses the option (EINVAL) once the other side has closed; the read then returns at once.
                if let Err(e) = s.set_read_timeout(Some(left()?)) {
                    if e.kind() != io::ErrorKind::InvalidInput {
                        return Err(e);
                    }
                }
                match s.read(&mut buf[at..]) {
                    Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                    Ok(n) => at += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "sway did not answer in time"))
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        };
        let mut head = [0u8; 14];
        read_exact(&mut head)?;
        let len = u32::from_ne_bytes(head[6..10].try_into().unwrap()) as usize;
        if &head[..6] != MAGIC || u32::from_ne_bytes(head[10..14].try_into().unwrap()) != kind || len > 64 << 20 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not an answer to our message"));
        }
        let mut body = vec![0u8; len];
        read_exact(&mut body)?;
        Ok(body)
    }

    /// The socket isn't there to talk to (as opposed to sway being slow): worth trying `swaymsg` instead.
    pub fn unreachable(e: &io::Error) -> bool {
        matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused | io::ErrorKind::PermissionDenied)
    }

    /// `swaymsg`'s exit status for a RUN_COMMAND answer: every command of the line succeeded.
    pub fn all_succeeded(reply: &[u8]) -> bool {
        serde_json::from_slice::<Vec<serde_json::Value>>(reply).is_ok_and(|v| !v.is_empty() && v.iter().all(|r| r["success"].as_bool() == Some(true)))
    }
}

fn sway_ok(cmd: &mut Command, timeout: Duration) -> bool {
    crate::proc::run_with_timeout(cmd, timeout).is_ok_and(|o| o.status.success())
}

/// Makes every other window (the games) open fullscreen and focused. Some emulators, Dolphin's
/// nogui build among them, don't ask for fullscreen and would otherwise be tiled next to pocket-ui.
pub fn games_open_fullscreen() -> bool {
    if crate::desk::active() {
        return true;
    }
    swaymsg(&format!(r#"for_window [app_id="^(?!{OWN_APP_ID}$).*"] fullscreen enable, focus"#))
        && swaymsg(r#"for_window [class=".*"] fullscreen enable, focus"#)
}

/// The `games_open_fullscreen` that `games_open_fullscreen_soon` started and nobody has waited for yet.
static RULES: std::sync::Mutex<Option<std::thread::JoinHandle<bool>>> = std::sync::Mutex::new(None);

/// `games_open_fullscreen` on a thread, so pocket-ui's first frame doesn't wait for two `swaymsg` (instant start).
/// A game must not open before the rules are in place: `launch` calls `rules_in_place` first.
pub fn games_open_fullscreen_soon() {
    let handle = std::thread::spawn(games_open_fullscreen);
    if let Ok(mut r) = RULES.lock() {
        *r = Some(handle);
    }
}

/// Waits for the rules `games_open_fullscreen_soon` is setting (at most two `swaymsg`, `SWAY_TIMEOUT` each).
/// Returns at once when they are set already or were never asked for.
pub fn rules_in_place() {
    let handle = RULES.lock().ok().and_then(|mut r| r.take());
    if let Some(h) = handle {
        let _ = h.join();
    }
}

/// Puts pocket-ui in front of the game. RetroArch pauses itself when it loses focus (pause_nonactive).
pub fn show_ui() -> bool {
    show_ui_within(SWAY_TIMEOUT)
}

pub fn show_ui_within(timeout: Duration) -> bool {
    if crate::desk::active() {
        return crate::desk::show_ui();
    }
    swaymsg_within(&show_ui_cmd(), timeout)
}

/// Gives the screen and focus back to the game, which resumes by itself.
pub fn show_game() -> bool {
    if crate::desk::active() {
        return crate::desk::show_game();
    }
    swaymsg(&show_game_cmd())
}

/// Every window but pocket-ui: the game, whichever emulator it is (Wayland and XWayland). Used when the
/// game's window id is unknown, e.g. sway answered too slowly under load when the menu opened.
fn games() -> [String; 2] {
    [format!(r#"[app_id="^(?!{OWN_APP_ID}$).*"]"#), r#"[class=".*"]"#.to_string()]
}

fn show_ui_cmd() -> String {
    let [wayland, x] = games();
    format!(r#"{wayland} fullscreen disable; {x} fullscreen disable; [app_id="{OWN_APP_ID}"] focus; [app_id="{OWN_APP_ID}"] fullscreen enable"#)
}

fn show_game_cmd() -> String {
    let [wayland, x] = games();
    format!(r#"[app_id="{OWN_APP_ID}"] fullscreen disable; {wayland} focus; {wayland} fullscreen enable; {x} focus; {x} fullscreen enable"#)
}

/// Sway container id of the focused window, if it is not pocket-ui (the running game during play).
/// None as well when sway doesn't answer within `timeout`.
pub fn focused_window(timeout: Duration) -> Option<i64> {
    if crate::desk::active() {
        return crate::desk::game_window();
    }
    match ipc::request(ipc::GET_TREE, "", timeout) {
        Ok(tree) => focused_in(&tree),
        Err(e) if ipc::unreachable(&e) => focused_window_via(Command::new("swaymsg").args(["-t", "get_tree"]), timeout),
        Err(_) => None,
    }
}

fn focused_window_via(cmd: &mut Command, timeout: Duration) -> Option<i64> {
    let out = crate::proc::run_with_timeout(cmd, timeout).ok()?;
    focused_in(&out.stdout)
}

/// The focused window in sway's tree (`get_tree`'s JSON), if it is not pocket-ui.
fn focused_in(json: &[u8]) -> Option<i64> {
    let tree: serde_json::Value = serde_json::from_slice(json).ok()?;
    fn find(node: &serde_json::Value) -> Option<&serde_json::Value> {
        if node["focused"].as_bool() == Some(true) && matches!(node["type"].as_str(), Some("con" | "floating_con")) {
            return Some(node);
        }
        node["nodes"].as_array().into_iter().chain(node["floating_nodes"].as_array()).flatten().find_map(find)
    }
    let node = find(&tree)?;
    (node["app_id"].as_str() != Some(OWN_APP_ID)).then(|| node["id"].as_i64()).flatten()
}

/// Sends `sig` to the game's whole process group (pocket-ui starts every game in its own group).
fn signal_group(pgid: i32, sig: i32) -> bool {
    // SAFETY: kill(2) with a negative pid only signals that process group.
    pgid > 1 && unsafe { libc::kill(-pgid, sig) } == 0
}

/// Pauses every process of the running game; works for any emulator.
pub fn pause_game(pgid: i32) -> bool {
    signal_group(pgid, libc::SIGSTOP)
}

pub fn resume_game(pgid: i32) -> bool {
    signal_group(pgid, libc::SIGCONT)
}

/// Last resort when a game ignores the close request: SIGTERM (`hard`: SIGKILL) to its emulators, after a
/// SIGCONT in case it is frozen. A group led by ROCKNIX's runemu.sh keeps its leader: once its emulator has
/// ended, runemu.sh puts the CPU governor, GPU level and CPU cores back, which a signal to it would cut short.
pub fn terminate_game(pgid: i32, hard: bool) -> bool {
    resume_game(pgid);
    let sig = if hard { libc::SIGKILL } else { libc::SIGTERM };
    #[cfg(target_os = "linux")]
    {
        let spare = std::fs::read_to_string(format!("/proc/{pgid}/comm")).is_ok_and(|c| c.trim_end() == "runemu.sh");
        let mut sent = false;
        for pid in group_members(pgid).into_iter().filter(|p| !(spare && *p == pgid)) {
            // SAFETY: kill(2) on one process of the game's group.
            sent |= unsafe { libc::kill(pid, sig) } == 0;
        }
        sent
    }
    #[cfg(not(target_os = "linux"))]
    signal_group(pgid, sig)
}

/// Whether any live process of group `pgid` is left.
pub fn group_alive(pgid: i32) -> bool {
    #[cfg(target_os = "linux")]
    {
        !group_members(pgid).is_empty()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // SAFETY: kill(2) with signal 0 only asks.
        pgid > 1 && unsafe { libc::kill(-pgid, 0) } == 0
    }
}

/// The live (not zombie) processes of process group `pgid`.
#[cfg(target_os = "linux")]
fn group_members(pgid: i32) -> Vec<i32> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            // pid (comm) state ppid pgrp ...; comm may hold spaces and parentheses.
            let mut rest = stat.get(stat.rfind(')')? + 2..)?.split(' ');
            let state = rest.next()?;
            let group: i32 = rest.nth(1)?.parse().ok()?;
            (group == pgid && state != "Z" && state != "X").then_some(pid)
        })
        .collect()
}

/// Puts pocket-ui in front of the game window `con`.
pub fn show_ui_over(con: i64) -> bool {
    show_ui_over_within(con, SWAY_TIMEOUT)
}

pub fn show_ui_over_within(con: i64, timeout: Duration) -> bool {
    if crate::desk::active() {
        let _ = con;
        return crate::desk::show_ui();
    }
    swaymsg_within(
        &format!(r#"[con_id={con}] fullscreen disable; [app_id="{OWN_APP_ID}"] focus; [app_id="{OWN_APP_ID}"] fullscreen enable"#),
        timeout,
    )
}

/// Gives the screen and focus back to the game window `con`.
pub fn show_window(con: i64) -> bool {
    if crate::desk::active() {
        return crate::desk::show_window(con);
    }
    swaymsg(&format!(r#"[app_id="{OWN_APP_ID}"] fullscreen disable; [con_id={con}] focus; [con_id={con}] fullscreen enable"#))
}

/// Asks the game window to close, as its own close button would; the emulator saves its config and exits.
/// It gets focus first (not fullscreen, the menu stays visible): paused clients like RetroArch only
/// act on the request while focused.
pub fn close_window(con: i64) -> bool {
    if crate::desk::active() {
        return crate::desk::close_window(con);
    }
    swaymsg(&format!("[con_id={con}] focus; [con_id={con}] kill"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    /// Without a window id (sway was slow to answer under game load) the menu and Continue must still
    /// move any game window, not only RetroArch: ARMSX2 stayed tiled next to pocket-ui after Continue.
    #[test]
    fn without_a_window_id_any_game_window_is_shown_and_hidden() {
        for cmd in [show_ui_cmd(), show_game_cmd()] {
            assert!(cmd.contains(&format!(r#"[app_id="^(?!{OWN_APP_ID}$).*"]"#)), "any non-pocket-ui window: {cmd}");
            assert!(cmd.contains(r#"[class=".*"]"#), "XWayland games too: {cmd}");
        }
        assert!(show_game_cmd().contains("fullscreen enable") && show_game_cmd().starts_with(&format!(r#"[app_id="{OWN_APP_ID}"] fullscreen disable"#)));
        assert!(show_ui_cmd().ends_with(&format!(r#"[app_id="{OWN_APP_ID}"] fullscreen enable"#)));
        assert!(MENU_TREE >= Duration::from_millis(250), "get_tree under PS2 load took longer than 100 ms");
    }

    #[test]
    fn a_swaymsg_that_hangs_costs_the_timeout_not_the_ui() {
        // The hung children note it when they get through their sleep: one of them getting there is what "the UI waited
        // for it" looks like, and it is an event, not a duration (the wall clock only says how loaded the machine is:
        // in a gate this took 2 s and more).
        let dir = TestDir::new("sway-hang");
        let marker = dir.join("finished");
        let hung = || sh(&format!("sleep 30; touch {}", marker.display()));
        assert!(!sway_ok(&mut hung(), Duration::from_millis(200)));
        assert!(focused_window_via(&mut hung(), Duration::from_millis(200)).is_none());
        assert!(!marker.exists(), "a hung swaymsg ran to its end: the UI waited for it");
    }

    #[test]
    fn a_spent_budget_does_not_even_start_swaymsg() {
        let dir = TestDir::new("spent-budget");
        let marker = dir.join("marker");
        // This thread's own CPU time, not the clock: a spent budget is over before anything is forked, so it costs
        // nothing at all. The wall clock measured the machine's load instead (over 100 ms in a gate).
        let cpu0 = crate::run_ahead::thread_cpu_ms();
        assert!(!sway_ok(&mut sh(&format!("touch {}", marker.display())), Duration::ZERO));
        let cpu = crate::run_ahead::thread_cpu_ms() - cpu0;
        assert!(!marker.exists(), "the program was started");
        assert!(cpu < 5.0, "{cpu:.2} ms of this thread's CPU for a spent budget");
    }

    /// A stand-in for sway's socket: answers each message with `reply` (or not at all), and hands back what it got.
    fn fake_sway(reply: Option<&'static str>) -> (tempfile::TempDir, std::path::PathBuf, std::thread::JoinHandle<Vec<u8>>) {
        use std::io::{Read, Write};
        // /tmp, not the temp dir: a socket path has room for about 100 bytes (macOS temp dirs are long).
        // The folder (and the socket in it) goes when the test ends.
        let dir = tempfile::Builder::new().prefix("pui-sway-").tempdir_in("/tmp").unwrap();
        let path = dir.path().join("s.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut head = [0u8; 14];
            c.read_exact(&mut head).unwrap();
            let len = u32::from_ne_bytes(head[6..10].try_into().unwrap()) as usize;
            let mut got = head.to_vec();
            let mut body = vec![0u8; len];
            c.read_exact(&mut body).unwrap();
            got.extend_from_slice(&body);
            match reply {
                Some(r) => {
                    let mut out = b"i3-ipc".to_vec();
                    out.extend_from_slice(&(r.len() as u32).to_ne_bytes());
                    out.extend_from_slice(&head[10..14]);
                    out.extend_from_slice(r.as_bytes());
                    c.write_all(&out).unwrap();
                }
                None => std::thread::sleep(Duration::from_millis(600)),
            }
            got
        });
        (dir, path, server)
    }

    #[test]
    fn commands_go_over_sways_socket_in_its_own_format() {
        let (_dir, path, server) = fake_sway(Some(r#"[{"success":true},{"success":true}]"#));
        let reply = ipc::request_at(&path, ipc::RUN_COMMAND, "[con_id=7] focus; [con_id=7] kill", Duration::from_secs(5)).unwrap();
        assert!(ipc::all_succeeded(&reply));
        let got = server.join().unwrap();
        assert_eq!(&got[..6], b"i3-ipc");
        assert_eq!(u32::from_ne_bytes(got[6..10].try_into().unwrap()) as usize, got.len() - 14);
        assert_eq!(u32::from_ne_bytes(got[10..14].try_into().unwrap()), ipc::RUN_COMMAND);
        assert_eq!(&got[14..], b"[con_id=7] focus; [con_id=7] kill");
        let _ = std::fs::remove_file(&path);
        // swaymsg's exit status: one failed command (a criteria that matched nothing) fails the line.
        assert!(!ipc::all_succeeded(br#"[{"success":true},{"success":false,"error":"No matching node."}]"#));
        assert!(!ipc::all_succeeded(b"[]") && !ipc::all_succeeded(b"garbage"));
    }

    #[test]
    fn the_tree_comes_over_the_socket_too() {
        let tree = r#"{"type":"root","nodes":[{"type":"con","focused":false,"id":3,"app_id":"pocket-ui"},
            {"type":"con","focused":true,"id":7,"app_id":"mgba"}],"floating_nodes":[]}"#;
        let (_dir, path, server) = fake_sway(Some(tree));
        let reply = ipc::request_at(&path, ipc::GET_TREE, "", Duration::from_secs(5)).unwrap();
        assert_eq!(focused_in(&reply), Some(7));
        assert_eq!(u32::from_ne_bytes(server.join().unwrap()[10..14].try_into().unwrap()), ipc::GET_TREE);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_sway_that_does_not_answer_costs_the_timeout_and_no_socket_means_swaymsg() {
        let (_dir, path, server) = fake_sway(None);
        // The kind is the event: the stand-in answers only after 600 ms, so a `TimedOut` from the 150 ms limit is the
        // deadline and nothing else (a wall clock here measured the load).
        let e = ipc::request_at(&path, ipc::RUN_COMMAND, "focus", Duration::from_millis(150)).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
        assert!(!ipc::unreachable(&e), "a slow sway is not asked twice");
        server.join().unwrap();
        let _ = std::fs::remove_file(&path);
        let e = ipc::request_at(&path, ipc::RUN_COMMAND, "focus", Duration::from_millis(150)).unwrap_err();
        assert!(ipc::unreachable(&e), "{e:?}");
    }

    #[test]
    fn swaymsg_answers_are_still_read() {
        assert!(sway_ok(&mut sh("exit 0"), Duration::from_secs(5)));
        assert!(!sway_ok(&mut sh("exit 1"), Duration::from_secs(5)));
        let tree = r#"{"type":"root","nodes":[{"type":"con","focused":false,"id":3,"app_id":"pocket-ui"},
            {"type":"con","focused":true,"id":7,"app_id":"mgba"}],"floating_nodes":[]}"#;
        assert_eq!(focused_window_via(&mut sh(&format!("cat <<'EOF'\n{tree}\nEOF")), Duration::from_secs(5)), Some(7));
    }
}

/// Where the launcher (deploy/run-pocket-ui.sh) finds the running game's process group, to stop it if
/// pocket-ui dies while it runs.
const GAME_FILE: &str = "/tmp/pocket-ui.game";

/// Tells the launcher which process group the running game has (`None`: no game any more).
pub fn note_game(pgid: Option<i32>) {
    let _ = match pgid {
        Some(g) => std::fs::write(GAME_FILE, g.to_string()),
        None => std::fs::remove_file(GAME_FILE),
    };
}

/// Makes the process `cmd` starts get SIGTERM when the thread that starts it ends, so a pocket-core never
/// outlives a pocket-ui that crashed. Spawn from the main thread: the kernel ties this to the thread.
#[cfg(target_os = "linux")]
pub fn die_with_parent(cmd: &mut Command) -> &mut Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: getpid and the closure's prctl and getppid are async-signal-safe.
    let parent = unsafe { libc::getpid() };
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // The parent may have died before the prctl: then there is nobody left to play for.
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::os::unix::process::ExitStatusExt;
    use std::time::{Duration, Instant};

    #[test]
    fn child_ends_with_the_thread_that_started_it() {
        let mut child = std::thread::spawn(|| die_with_parent(Command::new("sleep").arg("30")).spawn().unwrap()).join().unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if let Some(status) = child.try_wait().unwrap() {
                assert_eq!(status.signal(), Some(libc::SIGTERM));
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        panic!("the child outlived the thread that started it");
    }

    /// A game as pocket-ui starts it: a script called `runemu.sh` in its own process group runs an
    /// "emulator" in the foreground and notes in `cleanup` that it got to clean up after it, like ROCKNIX's.
    /// `emulator`: the emulator's trap for SIGTERM.
    struct FakeGame {
        dir: crate::testutil::TestDir,
        child: std::process::Child,
        pgid: i32,
    }

    impl FakeGame {
        fn start(leader: &str, emulator_trap: &str) -> FakeGame {
            use std::os::unix::process::CommandExt;
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let dir = TestDir::new(&format!("game-{n}"));
            let d = dir.display();
            let script = format!(
                "#!/bin/sh\nsh -c 'trap \"{emulator_trap}\" TERM; touch {d}/up; while :; do sleep 0.05; done'\necho \"cleanup after exit $?\" >{d}/cleanup\n"
            );
            let path = dir.join(leader);
            crate::testutil::write_executable(&path, script);
            let child = Command::new(&path).process_group(0).spawn().unwrap();
            let pgid = child.id() as i32;
            let end = Instant::now() + Duration::from_secs(3);
            while !dir.join("up").exists() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(dir.join("up").exists(), "the fake emulator didn't start");
            FakeGame { dir, child, pgid }
        }

        /// How the leader ended, if it did within `within`.
        fn wait(&mut self, within: Duration) -> Option<std::process::ExitStatus> {
            let end = Instant::now() + within;
            while Instant::now() < end {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return Some(status);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            None
        }

        fn cleanup(&self) -> Option<String> {
            std::fs::read_to_string(self.dir.join("cleanup")).ok().map(|s| s.trim().to_string())
        }
    }

    impl Drop for FakeGame {
        fn drop(&mut self) {
            unsafe { libc::kill(-self.pgid, libc::SIGKILL) };
            let _ = self.child.wait();
        }
    }

    #[test]
    fn quitting_a_frozen_game_spares_runemu_so_it_cleans_up() {
        let mut game = FakeGame::start("runemu.sh", "exit 0");
        assert!(pause_game(game.pgid));
        assert!(terminate_game(game.pgid, false));
        let status = game.wait(Duration::from_secs(5)).expect("the game didn't end");
        assert_eq!(status.signal(), None, "runemu.sh itself was signalled: {status:?}");
        assert_eq!(game.cleanup().as_deref(), Some("cleanup after exit 0"));
    }

    #[test]
    fn sigkill_reaches_an_emulator_that_ignores_sigterm_but_not_runemu() {
        let mut game = FakeGame::start("runemu.sh", "");
        terminate_game(game.pgid, false);
        assert!(game.wait(Duration::from_millis(500)).is_none(), "the emulator ignores SIGTERM");
        terminate_game(game.pgid, true);
        let status = game.wait(Duration::from_secs(5)).expect("the game didn't end");
        assert_eq!(status.signal(), None, "runemu.sh itself was killed: {status:?}");
        assert_eq!(game.cleanup().as_deref(), Some("cleanup after exit 137"));
    }

    #[test]
    fn a_game_without_runemu_is_signalled_whole() {
        // pocket-core leads its own group: it is the one to end.
        let mut game = FakeGame::start("pocket-core", "exit 0");
        terminate_game(game.pgid, false);
        let status = game.wait(Duration::from_secs(5)).expect("pocket-core didn't end");
        assert_eq!(status.signal(), Some(libc::SIGTERM));
    }
}
