//! A fake sway models the player's lower panel through hand-over, quick launch/quit, output loss and recovery.
#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use pocket_ui::control::ipc;
use pocket_ui::second_screen::{Controller, Power};

#[derive(Default)]
struct Panel {
    connected: bool,
    lit: bool,
    window_visible: bool,
    fail_on: bool,
    commands: Vec<String>,
}

fn wait_for(panel: &Mutex<Panel>, expected: impl Fn(&Panel) -> bool) {
    let until = Instant::now() + test_clock::timeout(Duration::from_secs(8));
    loop {
        {
            let p = panel.lock().unwrap();
            if expected(&p) { return; }
            assert!(Instant::now() < until, "panel never recovered; commands: {:?}", p.commands);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_launch_quit_output_loss_and_failed_power_writes_recover() {
    let tmp = tempfile::tempdir().unwrap();
    // This process has one test and its own environment. Use a Mini profile to prove that it stays inactive
    // until explicitly opted in; no real hardware backlight or compositor is touched.
    std::env::set_var("POCKETUI_DEVICE", "miniv2");
    std::env::remove_var("POCKETUI_SECOND_OUTPUT");
    std::env::remove_var("POCKETUI_DUAL_PANELS");
    assert!(Controller::start(|_| panic!("single-screen device must not start the worker")).is_none());
    std::env::set_var("POCKETUI_SECOND_OUTPUT", "HEADLESS-2");
    std::env::remove_var("POCKETUI_SWAY_IPC");
    let socket = tmp.path().join("sway.sock");
    std::env::set_var("SWAYSOCK", &socket);
    let server = UnixListener::bind(socket).unwrap();
    server.set_nonblocking(true).unwrap();
    let panel = Arc::new(Mutex::new(Panel { connected: true, window_visible: true, ..Default::default() }));
    let (stop_tx, stop_rx) = mpsc::channel();
    let (block_tx, block_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let fake = panel.clone();
    let worker = std::thread::spawn(move || {
        while stop_rx.try_recv().is_err() {
            let (mut socket, _) = match server.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                },
                Err(e) => panic!("{e}"),
            };
            socket.set_read_timeout(Some(test_clock::timeout(Duration::from_secs(3)))).unwrap();
            let mut header = [0; 14];
            socket.read_exact(&mut header).unwrap();
            let len = u32::from_ne_bytes(header[6..10].try_into().unwrap()) as usize;
            let kind = u32::from_ne_bytes(header[10..14].try_into().unwrap());
            let mut payload = vec![0; len];
            socket.read_exact(&mut payload).unwrap();
            let cmd = String::from_utf8(payload).unwrap();
            if cmd == "output HEADLESS-2 power off" && block_rx.try_recv().is_ok() {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(test_clock::timeout(Duration::from_secs(3))).unwrap();
            }
            let reply = {
                let mut p = fake.lock().unwrap();
                if kind == ipc::GET_OUTPUTS {
                    let mut outputs = vec![serde_json::json!({"name":"HEADLESS-1", "active":true, "power":true})];
                    if p.connected { outputs.push(serde_json::json!({"name":"HEADLESS-2", "active":true, "power":p.lit})); }
                    serde_json::to_vec(&outputs).unwrap()
                } else {
                    assert_eq!(kind, ipc::RUN_COMMAND);
                    p.commands.push(cmd.clone());
                    let failed = cmd == "output HEADLESS-2 power on" && p.fail_on;
                    if failed { p.fail_on = false; }
                    else {
                        if cmd == "output HEADLESS-2 power on" { p.lit = true; }
                        if cmd == "output HEADLESS-2 power off" { p.lit = false; }
                        if cmd.starts_with("[app_id=\"pocket-ui-second\"] move scratchpad") { p.window_visible = false; }
                        if cmd.contains("scratchpad show") { p.window_visible = !p.window_visible; }
                    }
                    format!("[{{\"success\":{}}}]", !failed).into_bytes()
                }
            };
            let mut response = ipc::MAGIC.to_vec();
            response.extend_from_slice(&(reply.len() as u32).to_ne_bytes());
            response.extend_from_slice(&kind.to_ne_bytes());
            response.extend_from_slice(&reply);
            socket.write_all(&response).unwrap();
        }
    });
    let (events_tx, events_rx) = mpsc::channel();
    let mut control = Controller::start(move |event| events_tx.send(event).is_ok()).unwrap();
    control.set_power(Power::Lit);
    assert_eq!(events_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap(), Some(("HEADLESS-1".into(), "HEADLESS-2".into())));
    wait_for(&panel, |p| p.lit);
    control.set_power(Power::Emulator);
    wait_for(&panel, |p| p.lit && !p.window_visible);
    control.set_power(Power::Lit);
    wait_for(&panel, |p| p.lit && p.window_visible);

    // Hold an old game's power-off request while the player quits. The final state must be lit.
    let off_before = panel.lock().unwrap().commands.iter().filter(|s| s.as_str() == "output HEADLESS-2 power off").count();
    block_tx.send(()).unwrap();
    control.set_power(Power::Off);
    entered_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap();
    control.set_power(Power::Lit);
    release_tx.send(()).unwrap();
    wait_for(&panel, |p| p.commands.iter().filter(|s| s.as_str() == "output HEADLESS-2 power off").count() > off_before && p.commands.iter().rev().find(|s| s.starts_with("output HEADLESS-2 power ")).is_some_and(|s| s.ends_with(" on")) && p.lit);

    control.set_power(Power::Off);
    wait_for(&panel, |p| !p.lit);
    panel.lock().unwrap().connected = false;
    assert_eq!(events_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap(), None, "release the missing output's window");
    assert!(!panel.lock().unwrap().lit, "losing outputs during sleep must not relight the panel");
    panel.lock().unwrap().connected = true;
    assert!(events_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap().is_some(), "recreate after reconnect");
    control.set_power(Power::Off);
    wait_for(&panel, |p| !p.lit);
    panel.lock().unwrap().fail_on = true;
    control.set_power(Power::Lit);
    wait_for(&panel, |p| p.lit && !p.fail_on);

    // A failed DS hand-over can still hide our window. Quitting immediately must restore Game info,
    // even though the last fully successful state was already Lit.
    panel.lock().unwrap().fail_on = true;
    control.set_power(Power::Emulator);
    wait_for(&panel, |p| !p.fail_on && !p.window_visible);
    control.set_power(Power::Lit);
    wait_for(&panel, |p| p.lit && p.window_visible);

    // The reverse partial change may already have shown our window over a DS game. Returning to DS
    // must hide it again, rather than trust the previous fully successful Emulator state.
    control.set_power(Power::Emulator);
    wait_for(&panel, |p| !p.window_visible);
    panel.lock().unwrap().fail_on = true;
    control.set_power(Power::Lit);
    wait_for(&panel, |p| !p.fail_on && p.window_visible);
    control.set_power(Power::Emulator);
    wait_for(&panel, |p| !p.window_visible);

    control.set_power(Power::Off);
    wait_for(&panel, |p| !p.lit);
    drop(control);
    assert!(panel.lock().unwrap().lit, "quitting must hand the lower panel back lit");
    assert_eq!(panel.lock().unwrap().commands.iter().filter(|s| s.starts_with("for_window [app_id=\"pocket-ui-second\"]")).count(), 1,
        "reconnecting must not accumulate compositor placement rules");
    assert!(!panel.lock().unwrap().commands.iter().any(|s| s == "output HEADLESS-1 power off"), "the game must never darken the upper panel");
    stop_tx.send(()).unwrap();
    worker.join().unwrap();
}

#[path = "../src/test_clock.rs"]
mod test_clock;
