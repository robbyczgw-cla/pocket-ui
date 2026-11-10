//! pocket-fb on the host, with a plain file as the framebuffer: two windows and the pause menu's focus changes, spoken
//! over real sockets (docs/TRIMUI-BRICK.md). `cargo test --features brick --test brick_fb`.
#![cfg(all(feature = "brick", target_os = "linux"))]

#[path = "common/executable.rs"]
mod fixture_executable;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pocket_ui::control::ipc;

const W: u32 = 64;
const H: u32 = 48;

struct Bridge {
    child: Child,
    dir: tempfile::TempDir,
}

impl Bridge {
    fn start(bits: u32) -> Bridge {
        let dir = tempfile::tempdir().unwrap();
        let bytes = if bits == 16 { 2 } else { 4 };
        let original = vec![0x5au8; (W * H) as usize * bytes];
        std::fs::write(dir.path().join("fb"), &original).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_pocket-fb"))
            .args(["--wayland", dir.path().join("w").to_str().unwrap(), "--ipc", dir.path().join("i").to_str().unwrap()])
            .args(["--fb", dir.path().join("fb").to_str().unwrap(), "--mode", &format!("{W}x{H}x{bits}")])
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let bridge = Bridge { child, dir };
        wait_for("the sockets", || bridge.path("w").exists() && bridge.path("i").exists());
        bridge
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn fb(&self) -> Vec<u8> {
        std::fs::read(self.path("fb")).unwrap()
    }

    fn pixel(&self) -> [u8; 4] {
        self.fb()[..4].try_into().unwrap()
    }

    fn sway(&self, kind: u32, payload: &str) -> String {
        String::from_utf8(ipc::request_at(&self.path("i"), kind, payload, Duration::from_secs(5)).unwrap()).unwrap()
    }

    fn command_ok(&self, line: &str) -> bool {
        ipc::all_succeeded(self.sway(ipc::RUN_COMMAND, line).as_bytes())
    }

    fn stop(&mut self) {
        // SAFETY: a signal to our own child.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        self.child.wait().unwrap();
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < Duration::from_secs(10), "no {what} after 10 s");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A Wayland client written out by hand: enough to show one solid colour.
struct Client {
    s: UnixStream,
    next: u32,
    surface: u32,
    xdg: u32,
    top: u32,
    pool: u32,
    /// The window's memory, through a mapping of the pool's file.
    file: std::fs::File,
    events: Vec<u8>,
}

fn string(s: &str) -> Vec<u8> {
    let mut p = ((s.len() + 1) as u32).to_le_bytes().to_vec();
    p.extend_from_slice(s.as_bytes());
    p.push(0);
    p.resize(p.len().next_multiple_of(4), 0);
    p
}

impl Client {
    fn send(&mut self, obj: u32, op: u16, body: &[u8]) {
        let mut m = obj.to_le_bytes().to_vec();
        m.extend_from_slice(&op.to_le_bytes());
        m.extend_from_slice(&((8 + body.len()) as u16).to_le_bytes());
        m.extend_from_slice(body);
        self.s.write_all(&m).unwrap();
    }

    fn new_id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// Reads what's there, then looks for an event: object and opcode.
    fn has_event(&mut self, obj: u32, op: u16) -> bool {
        self.s.set_nonblocking(true).unwrap();
        let mut buf = [0u8; 8192];
        while let Ok(n) = self.s.read(&mut buf) {
            if n == 0 {
                break;
            }
            self.events.extend_from_slice(&buf[..n]);
        }
        self.s.set_nonblocking(false).unwrap();
        let mut at = 0;
        while at + 8 <= self.events.len() {
            let id = u32::from_le_bytes(self.events[at..at + 4].try_into().unwrap());
            let opcode = u16::from_le_bytes(self.events[at + 4..at + 6].try_into().unwrap());
            let size = u16::from_le_bytes(self.events[at + 6..at + 8].try_into().unwrap()) as usize;
            if id == obj && opcode == op {
                return true;
            }
            at += size;
        }
        false
    }

    fn connect(path: &Path, app_id: &str) -> Client {
        Client::connect_pool(path, app_id, W * H * 4 * 2)
    }

    /// As `connect`, with a pool of `bytes` (a picture larger than the 64 by 48 screen of the other tests).
    fn connect_pool(path: &Path, app_id: &str, bytes: u32) -> Client {
        let s = UnixStream::connect(path).unwrap();
        // SAFETY: memfd_create with a valid name; the descriptor is ours.
        let fd = unsafe { OwnedFd::from_raw_fd(libc::memfd_create(c"pool".as_ptr(), libc::MFD_CLOEXEC)) };
        let file = std::fs::File::from(fd);
        file.set_len(u64::from(bytes)).unwrap();
        let mut c = Client { s, next: 1, surface: 0, xdg: 0, top: 0, pool: 0, file, events: Vec::new() };
        let registry = c.new_id();
        c.send(1, 1, &registry.to_le_bytes());
        let bind = |c: &mut Client, name: u32, iface: &str, version: u32| {
            let id = c.new_id();
            let mut b = name.to_le_bytes().to_vec();
            b.extend_from_slice(&string(iface));
            b.extend_from_slice(&version.to_le_bytes());
            b.extend_from_slice(&id.to_le_bytes());
            c.send(registry, 0, &b);
            id
        };
        let compositor = bind(&mut c, 1, "wl_compositor", 4);
        let shm = bind(&mut c, 2, "wl_shm", 1);
        let wm = bind(&mut c, 3, "xdg_wm_base", 2);
        c.surface = c.new_id();
        c.send(compositor, 0, &c.surface.to_le_bytes());
        c.xdg = c.new_id();
        let mut b = c.xdg.to_le_bytes().to_vec();
        b.extend_from_slice(&c.surface.to_le_bytes());
        c.send(wm, 2, &b);
        c.top = c.new_id();
        c.send(c.xdg, 1, &c.top.to_le_bytes());
        c.send(c.top, 3, &string(app_id));
        c.send(c.surface, 6, &[]);
        // The pool: a request that carries a descriptor.
        c.pool = c.new_id();
        let mut body = c.pool.to_le_bytes().to_vec();
        body.extend_from_slice(&bytes.to_le_bytes());
        let mut m = shm.to_le_bytes().to_vec();
        m.extend_from_slice(&0u16.to_le_bytes());
        m.extend_from_slice(&((8 + body.len()) as u16).to_le_bytes());
        m.extend_from_slice(&body);
        send_with_fd(&c.s, &m, c.file.as_raw_fd());
        wait_for("configure", || c.has_event(c.xdg, 0));
        c
    }

    /// A picture of `w` by `h` in one colour, at the start of the pool; returns its frame callback.
    fn show_size(&mut self, w: u32, h: u32, rgb: u32) -> u32 {
        let pixels: Vec<u8> = rgb.to_le_bytes().iter().copied().cycle().take((w * h * 4) as usize).collect();
        self.show_pixels(w, h, &pixels)
    }

    /// A picture of `w` by `h` from XRGB8888 `pixels`, at the start of the pool (it must be big enough).
    fn show_pixels(&mut self, w: u32, h: u32, pixels: &[u8]) -> u32 {
        use std::os::unix::fs::FileExt;
        self.file.write_all_at(pixels, 0).unwrap();
        let buffer = self.new_id();
        let mut b = buffer.to_le_bytes().to_vec();
        for v in [0, w, h, w * 4, 1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        self.send(self.pool, 0, &b);
        let mut b = buffer.to_le_bytes().to_vec();
        b.extend_from_slice(&[0; 8]);
        self.send(self.surface, 1, &b);
        let cb = self.new_id();
        self.send(self.surface, 3, &cb.to_le_bytes());
        self.send(self.surface, 6, &[]);
        cb
    }

    /// One picture in one colour; returns its frame callback.
    fn show(&mut self, offset_page: u32, rgb: u32) -> u32 {
        use std::os::unix::fs::FileExt;
        let offset = offset_page * W * H * 4;
        let pixels: Vec<u8> = rgb.to_le_bytes().iter().copied().cycle().take((W * H * 4) as usize).collect();
        self.file.write_all_at(&pixels, u64::from(offset)).unwrap();
        let buffer = self.new_id();
        let mut b = buffer.to_le_bytes().to_vec();
        for v in [offset, W, H, W * 4, 1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        self.send(self.pool, 0, &b);
        let mut b = buffer.to_le_bytes().to_vec();
        b.extend_from_slice(&[0; 8]);
        self.send(self.surface, 1, &b);
        let cb = self.new_id();
        self.send(self.surface, 3, &cb.to_le_bytes());
        self.send(self.surface, 6, &[]);
        cb
    }
}

fn send_with_fd(s: &UnixStream, data: &[u8], fd: i32) {
    let mut control = [0u64; 4];
    let mut iov = libc::iovec { iov_base: data.as_ptr() as *mut _, iov_len: data.len() };
    // SAFETY: the control buffer is large enough for one descriptor, all pointers live across the call.
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = libc::CMSG_SPACE(4) as _;
        let c = libc::CMSG_FIRSTHDR(&msg);
        (*c).cmsg_level = libc::SOL_SOCKET;
        (*c).cmsg_type = libc::SCM_RIGHTS;
        (*c).cmsg_len = libc::CMSG_LEN(4) as _;
        libc::CMSG_DATA(c).cast::<i32>().write_unaligned(fd);
        assert_eq!(libc::sendmsg(s.as_raw_fd(), &msg, 0), data.len() as isize);
    }
}

const RED: u32 = 0x00ff_0000;
const GREEN: u32 = 0x0000_ff00;
const BLUE: u32 = 0x0000_00ff;
const RED_ON_FB: [u8; 4] = [0, 0, 0xff, 0xff];
const GREEN_ON_FB: [u8; 4] = [0, 0xff, 0, 0xff];
const BLUE_ON_FB: [u8; 4] = [0xff, 0, 0, 0xff];

const GAMES: &str = r#"[app_id="^(?!pocket-ui$).*"]"#;
const UI: &str = r#"[app_id="pocket-ui"]"#;

#[test]
fn a_native_port_keeps_the_screen_until_the_pause_menu_and_returns_after_quit() {
    let bridge = Bridge::start(32);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the shelf", || bridge.pixel() == GREEN_ON_FB);
    let pid = std::process::id();
    assert!(bridge.command_ok(&format!("pocket_port begin {pid}")));
    let tree: serde_json::Value = serde_json::from_str(&bridge.sway(ipc::GET_TREE, "")).unwrap();
    assert!(tree.to_string().contains("pocket-native-port"), "native ports must be visible to focus and Quit");
    std::fs::write(bridge.path("fb"), vec![0x42; (W * H * 4) as usize]).unwrap();
    ui.show(1, BLUE);
    assert!(bridge.command_ok(&format!("{GAMES} focus")));
    assert_eq!(bridge.pixel(), [0x42; 4], "background UI updates must not overwrite the port");
    assert!(bridge.command_ok(&format!("{UI} focus")));
    wait_for("the pause menu over the native port", || bridge.pixel() == BLUE_ON_FB);
    assert!(bridge.command_ok(&format!("{GAMES} focus")));
    std::fs::write(bridge.path("fb"), vec![0x43; (W * H * 4) as usize]).unwrap();
    // A stale launch cannot unregister the current port.
    assert!(!bridge.command_ok(&format!("pocket_port end {}", pid + 1)));
    assert!(bridge.command_ok(&format!("pocket_port end {pid}")));
    wait_for("the shelf after Quit", || bridge.pixel() == BLUE_ON_FB);
}

#[test]
fn the_pause_menu_goes_over_the_game_and_back() {
    let bridge = Bridge::start(32);
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    let cb = game.show(0, RED);
    wait_for("the game on the screen", || bridge.pixel() == RED_ON_FB);
    wait_for("the game's frame callback", || game.has_event(cb, 0));

    // A new window takes the screen, as `for_window ... focus` has it under sway.
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    let cb_ui = ui.show(0, GREEN);
    wait_for("the UI on the screen", || bridge.pixel() == GREEN_ON_FB);
    wait_for("the UI's frame callback", || ui.has_event(cb_ui, 0));

    // A picture from the window behind stays behind, and its callback waits.
    let cb_hidden = game.show(1, BLUE);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(bridge.pixel(), GREEN_ON_FB);
    assert!(!game.has_event(cb_hidden, 0), "a window out of sight gets no frame callback");

    // `control::show_game`, word for word, and its newest picture comes back with it.
    assert!(bridge.command_ok(&format!(r#"{UI} fullscreen disable; {GAMES} focus; {GAMES} fullscreen enable"#)));
    wait_for("the game back", || bridge.pixel() == BLUE_ON_FB);
    wait_for("the callback it waited for", || game.has_event(cb_hidden, 0));

    // `control::focused_window` reads the game's id out of the tree, the con id it later uses.
    let tree: serde_json::Value = serde_json::from_str(&bridge.sway(ipc::GET_TREE, "")).unwrap();
    let windows = &tree["nodes"][0]["nodes"][0]["nodes"];
    let focused = windows.as_array().unwrap().iter().find(|w| w["focused"] == true).unwrap();
    assert_eq!(focused["app_id"], "pocket-core");
    assert_eq!(focused["type"], "con");
    let con = focused["id"].as_i64().unwrap();

    // `control::show_ui_over`, then `show_window` back by that id.
    assert!(bridge.command_ok(&format!(r#"[con_id={con}] fullscreen disable; {UI} focus; {UI} fullscreen enable"#)));
    wait_for("the UI over the game", || bridge.pixel() == GREEN_ON_FB);
    assert!(bridge.command_ok(&format!(r#"{UI} fullscreen disable; [con_id={con}] focus; [con_id={con}] fullscreen enable"#)));
    wait_for("the game by its id", || bridge.pixel() == BLUE_ON_FB);

    // `control::close_window`: the toplevel hears `close`.
    assert!(bridge.command_ok(&format!("[con_id={con}] focus; [con_id={con}] kill")));
    wait_for("the close request", || game.has_event(game.top, 1));
    assert!(!bridge.command_ok("[con_id=999] focus"), "no such window");
}

#[test]
fn the_screen_falls_back_when_the_window_on_it_goes() {
    let bridge = Bridge::start(32);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || bridge.pixel() == GREEN_ON_FB);
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    game.show(0, RED);
    wait_for("the game", || bridge.pixel() == RED_ON_FB);
    drop(game);
    wait_for("the UI again after the game ended", || bridge.pixel() == GREEN_ON_FB);
}

#[test]
fn a_blank_screen_takes_no_pictures_until_it_is_back() {
    let bridge = Bridge::start(32);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || bridge.pixel() == GREEN_ON_FB);
    assert!(bridge.command_ok("output * power off"));
    ui.show(1, RED);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(bridge.pixel(), GREEN_ON_FB);
    assert!(bridge.command_ok("output * power on"));
    wait_for("the newest picture", || bridge.pixel() == RED_ON_FB);
}

#[test]
fn a_16_bit_framebuffer_gets_rgb565() {
    let bridge = Bridge::start(16);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, RED);
    wait_for("red in 565", || bridge.fb()[..2] == 0xf800u16.to_le_bytes());
}

#[test]
fn the_screen_is_put_back_on_exit() {
    let mut bridge = Bridge::start(32);
    let before = bridge.fb();
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || bridge.pixel() == GREEN_ON_FB);
    bridge.stop();
    assert!(bridge.fb() == before, "the framebuffer holds what it held at the start");
}

#[test]
fn a_garbled_client_does_not_take_the_bridge_down() {
    let bridge = Bridge::start(32);
    let mut bad = UnixStream::connect(bridge.path("w")).unwrap();
    bad.write_all(&[0xff; 64]).unwrap();
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || bridge.pixel() == GREEN_ON_FB);
}

/// The real pocket-ui on the bridge, its usual winit and softbuffer: its first screen reaches the framebuffer.
#[test]
fn the_real_pocket_ui_reaches_the_framebuffer() {
    let dir = tempfile::tempdir().unwrap();
    let size = 1024 * 768 * 4;
    std::fs::write(dir.path().join("fb"), vec![0u8; size]).unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_pocket-fb"))
        .args(["--wayland", dir.path().join("w").to_str().unwrap(), "--ipc", dir.path().join("i").to_str().unwrap()])
        .args(["--fb", dir.path().join("fb").to_str().unwrap(), "--mode", "1024x768x32"])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("the sockets", || dir.path().join("w").exists() && dir.path().join("i").exists());
    // A stand-in pocket-core keeps loading after the launch fade, as a stalled card read does. It is one
    // process, bounded to 11 seconds, and pocket-ui's parent-death signal ends it if the UI exits early.
    let host = dir.path().join("host");
    fixture_executable::publish(&host, format!("#!/usr/bin/env python3\nimport time,pathlib\ntime.sleep(3)\npathlib.Path({:?}).write_text('loading')\ntime.sleep(8)\n", pocket_ui::startup::loading_path())).unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"shelf","motion":"off","art":"off"}"#).unwrap();
    std::fs::create_dir_all(dir.path().join(".pocket-ui-test")).unwrap();
    let rom = dir.path().join(".pocket-ui-test/Game.gb");
    std::fs::write(&rom, b"game").unwrap();
    std::fs::write(dir.path().join("systems.cfg"), format!("<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", rom.parent().unwrap().display())).unwrap();
    let log = dir.path().join("ui.log");
    let visible = dir.path().join("visible.txt");
    let mut ui = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
        .env("POCKETUI_HOST_BIN", &host)
        .env("POCKETUI_ES_SYSTEMS", dir.path().join("systems.cfg"))
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("WAYLAND_DISPLAY", dir.path().join("w"))
        .env("SWAYSOCK", dir.path().join("i"))
        .env("XDG_RUNTIME_DIR", dir.path())
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_DATA", dir.path().join("data"))
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let painted = loop {
        let fb = std::fs::read(dir.path().join("fb")).unwrap();
        let lit = fb.chunks_exact(4).step_by(97).filter(|p| p[..3] != [0, 0, 0]).count();
        if lit > 200 {
            break true;
        }
        if start.elapsed() > Duration::from_secs(60) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let loading = if painted {
        use std::os::unix::fs::OpenOptionsExt;
        // Other real-UI checks can replace the shared FIFO name. Open this UI's descriptor through /proc,
        // so the launch always reaches our own process, including when its FIFO has since been unlinked.
        let mut fifo = None;
        wait_for("our UI's command reader", || {
            fifo = std::fs::read_dir(format!("/proc/{}/fd", ui.id())).ok().into_iter().flatten().flatten()
                .find(|e| std::fs::read_link(e.path()).is_ok_and(|p| p.to_string_lossy().starts_with("/tmp/pocket-ui.in")))
                .map(|e| e.path());
            fifo.is_some()
        });
        let mut input = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(fifo.unwrap()).unwrap();
        writeln!(input, "play gb {}", rom.display()).unwrap();
        let until = Instant::now() + Duration::from_secs(7);
        let mut reached = false;
        while Instant::now() < until {
            if std::fs::read_to_string(&visible).unwrap_or_default().contains("Loading the game…") {
                let fb = std::fs::read(dir.path().join("fb")).unwrap();
                reached = fb.as_chunks::<4>().0.iter().filter(|p| p[..3] != [0, 0, 0]).count() > 200;
                if reached { break; }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        reached
    } else { false };
    let _ = ui.kill();
    let _ = ui.wait();
    let _ = std::fs::remove_file(pocket_ui::startup::loading_path());
    let _ = bridge.kill();
    let _ = bridge.wait();
    assert!(painted, "pocket-ui's first screen never reached the framebuffer");
    assert!(loading, "the loading message never reached pocket-fb after the fade: {}", std::fs::read_to_string(log).unwrap_or_default());
}

/// The bridge with the GPU's way: Mesa's software GPU on a pbuffer when a computer has one, what was drawn read back into the file.
fn gl_bridge(lib: Option<&str>, window: &str, log: &Path, dir: tempfile::TempDir) -> Bridge {
    gl_bridge_sized((W, H), lib, window, log, dir)
}

fn gl_bridge_sized((w, h): (u32, u32), lib: Option<&str>, window: &str, log: &Path, dir: tempfile::TempDir) -> Bridge {
    std::fs::write(dir.path().join("fb"), vec![0x5au8; (w * h * 4) as usize]).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-fb"));
    cmd.args(["--wayland", dir.path().join("w").to_str().unwrap(), "--ipc", dir.path().join("i").to_str().unwrap()])
        .args(["--fb", dir.path().join("fb").to_str().unwrap(), "--mode", &format!("{w}x{h}x32"), "--gl", "--gl-window", window])
        .env("EGL_PLATFORM", "surfaceless")
        .env("LIBGL_ALWAYS_SOFTWARE", "1")
        .stderr(std::fs::File::create(log).unwrap());
    if let Some(l) = lib {
        cmd.env("BRICK_EGL_LIB", l);
    }
    let child = cmd.spawn().unwrap();
    let bridge = Bridge { child, dir };
    wait_for("the sockets", || bridge.path("w").exists() && bridge.path("i").exists());
    bridge
}

/// Row 0 of the file, as `Bridge::fb` reads it: the same bytes as the copy would have written, alpha aside.
fn rgb_of(bridge: &Bridge) -> [u8; 3] {
    bridge.fb()[..3].try_into().unwrap()
}

#[test]
fn the_gpus_way_shows_the_right_colours_and_follows_the_focus() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let bridge = gl_bridge(None, "pbuffer", log.path(), dir);
    let said = std::fs::read_to_string(log.path()).unwrap();
    if said.contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check: {said}");
        return;
    }
    assert!(said.contains("GL on a pbuffer window"), "{said}");
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    game.show(0, RED);
    wait_for("red through the GPU", || rgb_of(&bridge) == RED_ON_FB[..3]);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    let cb = ui.show(0, BLUE);
    wait_for("blue through the GPU", || rgb_of(&bridge) == BLUE_ON_FB[..3]);
    wait_for("the frame callback", || ui.has_event(cb, 0));
    assert!(bridge.command_ok(&format!(r#"{UI} fullscreen disable; {GAMES} focus"#)));
    wait_for("the game back", || rgb_of(&bridge) == RED_ON_FB[..3]);
}

/// What a row looks like in GL is the top of the screen here: a picture with a differently coloured first row, and a stride wider than it.
#[test]
fn the_gpus_way_keeps_the_picture_upright() {
    use pocket_ui::brick_gl::Presenter;
    std::env::set_var("EGL_PLATFORM", "surfaceless");
    let Ok(mut gl) = Presenter::new("pbuffer", (8, 4), &mut |_| {}) else {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    };
    // 8 by 4 at 12 pixels per row: the top row red, the others blue, what's beyond the width must not show.
    let mut pixels = Vec::new();
    for y in 0..4 {
        for x in 0..12 {
            let v: u32 = if x >= 8 { 0x00ff_ffff } else if y == 0 { RED } else { BLUE };
            pixels.extend_from_slice(&v.to_le_bytes());
        }
    }
    gl.show(&pixels, 8, 4, 12 * 4).unwrap();
    let drawn = gl.read_xrgb();
    let at = |x: usize, y: usize| u32::from_le_bytes(drawn[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].try_into().unwrap());
    // read_xrgb reads after the swap: a pbuffer keeps its picture.
    assert_eq!(at(0, 0) & 0xff_ffff, RED);
    assert_eq!(at(7, 0) & 0xff_ffff, RED);
    assert_eq!(at(0, 3) & 0xff_ffff, BLUE);
    assert_eq!(at(7, 3) & 0xff_ffff, BLUE);
    // A picture smaller than the screen: top left, black around it.
    let small: Vec<u8> = RED.to_le_bytes().iter().copied().cycle().take(2 * 2 * 4).collect();
    gl.show(&small, 2, 2, 2 * 4).unwrap();
    let drawn = gl.read_xrgb();
    let at = |x: usize, y: usize| u32::from_le_bytes(drawn[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].try_into().unwrap());
    assert_eq!((at(0, 0) & 0xff_ffff, at(1, 1) & 0xff_ffff, at(2, 0) & 0xff_ffff, at(0, 2) & 0xff_ffff), (RED, RED, 0, 0));
    // A short buffer is an error, not a read past its end.
    assert!(gl.show(&small, 8, 4, 8 * 4).is_err());
}

#[test]
fn without_the_gpu_library_the_copy_runs_and_the_log_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let bridge = gl_bridge(Some("/nonexistent/libEGL.so.1"), "null", log.path(), dir);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI by the copy", || bridge.pixel() == GREEN_ON_FB);
    let said = std::fs::read_to_string(log.path()).unwrap();
    assert!(said.contains("GL failed at libEGL.so.1") && said.contains("using the CPU copy"), "{said}");
}

/// A window kind the system refuses (Mesa has no Null WSI): the same.
#[test]
fn a_window_the_blob_refuses_leaves_the_copy() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let bridge = gl_bridge(None, "fbdev", log.path(), dir);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || bridge.pixel() == GREEN_ON_FB);
    let said = std::fs::read_to_string(log.path()).unwrap();
    assert!(said.contains("using the CPU copy"), "{said}");
}

/// pocket-ui's page transition and a change of window, drawn by the GPU: the bridge says yes to both, ends on the newest picture and
/// writes one line per animation. Without the GPU it says no, so pocket-ui keeps its own.
#[test]
fn the_gpu_draws_page_transitions_and_window_crossfades() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut bridge = gl_bridge(None, "pbuffer", log.path(), dir);
    if std::fs::read_to_string(log.path()).unwrap().contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    }
    assert!(bridge.command_ok("pocket_gpu"));
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the first page", || rgb_of(&bridge) == GREEN_ON_FB[..3]);
    assert!(bridge.command_ok("pocket_transition push 0 112 102030 0 1"));
    ui.show(1, BLUE);
    wait_for("the new page at rest", || rgb_of(&bridge) == BLUE_ON_FB[..3]);
    assert!(!bridge.command_ok("pocket_transition sideways 0 112 102030 0 1"), "a transition that doesn't read is refused");
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    game.show(0, RED);
    wait_for("the game after the crossfade", || rgb_of(&bridge) == RED_ON_FB[..3]);
    std::thread::sleep(Duration::from_millis(300));
    bridge.stop();
    let said = std::fs::read_to_string(log.path()).unwrap();
    assert!(said.contains("gpu page push:") && said.contains("gpu window crossfade:"), "{said}");
}

#[test]
fn without_the_gpu_transitions_stay_with_pocket_ui() {
    let bridge = Bridge::start(32);
    assert!(!bridge.command_ok("pocket_gpu"));
    assert!(!bridge.command_ok("pocket_transition push 0 112 102030 0 1"));
}

/// A window without a picture doesn't take the screen (pocket-core waits out the launch animation's hold before it draws): the UI
/// stays on it and keeps its frame callbacks, and the game comes with its first picture.
#[test]
fn a_window_takes_the_screen_with_its_first_picture() {
    let bridge = Bridge::start(32);
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || bridge.pixel() == GREEN_ON_FB);
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    let cb = ui.show(1, BLUE);
    wait_for("the UI's next picture, its window still on the screen", || bridge.pixel() == BLUE_ON_FB);
    wait_for("the UI's frame callback", || ui.has_event(cb, 0));
    game.show(0, RED);
    wait_for("the game with its first picture", || bridge.pixel() == RED_ON_FB);
}

/// `pocket_picture` as pocket-core sends it: a console, the core's aspect, no turn, a frames folder, and the Picture settings.
fn picture_command(pairs: &[(&str, &str)]) -> String {
    pocket_ui::brick_picture::Request {
        system: "gba".into(),
        aspect: 0.0,
        turns: 0,
        frames_dir: PathBuf::from("/nonexistent/frames"),
        values: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
    .command()
}

/// Shaders and frames for a game: it hands over its picture at its own size, the bridge's renderer scales and draws it by the settings,
/// and a change of settings redraws the picture at once. Mesa's software GPU on a pbuffer.
#[test]
fn a_game_with_picture_settings_is_drawn_by_the_renderer_at_the_screens_size() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let bridge = gl_bridge(None, "pbuffer", log.path(), dir);
    if std::fs::read_to_string(log.path()).unwrap().contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    }
    assert!(bridge.command_ok("pocket_shaders"), "the renderer compiled, so rows that need it may show");
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    let px = |b: &Bridge, x: usize, y: usize| -> [u8; 3] { b.fb()[(y * W as usize + x) * 4..][..3].try_into().unwrap() };
    // Whole-number scale: a 4 by 2 picture at 16x is 64 by 32, centred: black above and below it.
    assert!(bridge.command_ok(&picture_command(&[("scaling", "native"), ("sharpness", "sharp"), ("effect", "none")])));
    game.show_size(4, 2, RED);
    wait_for("the picture at 16x", || px(&bridge, 32, 24) == RED_ON_FB[..3]);
    assert_eq!(px(&bridge, 32, 2), [0, 0, 0], "black above the picture");
    // Fullscreen fills the screen, and the change draws the picture on show again without a new one.
    assert!(bridge.command_ok(&picture_command(&[("scaling", "fullscreen")])));
    wait_for("the picture over the whole screen", || px(&bridge, 2, 2) == RED_ON_FB[..3] && px(&bridge, 61, 45) == RED_ON_FB[..3]);
    // A scanline effect keeps a flat picture's light (the maths of `picture_fx`): still red, not black stripes.
    assert!(bridge.command_ok(&picture_command(&[("scaling", "fullscreen"), ("effect", "scanlines")])));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(px(&bridge, 32, 24)[2], 0xff, "red stays red under the scanlines");
    // The pause menu's window is not drawn by it, even in the same process; and the game comes back as it was.
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the UI", || px(&bridge, 0, 0) == GREEN_ON_FB[..3] && px(&bridge, 63, 47) == GREEN_ON_FB[..3]);
    assert!(bridge.command_ok(&format!(r#"{UI} fullscreen disable; {GAMES} focus"#)));
    // Red again, not green; the scanlines still darken some rows (218 of 255 at the corner), so "red" is a strong red with no green.
    wait_for("the game again", || px(&bridge, 2, 2)[2] > 0xc0 && px(&bridge, 2, 2)[1] == 0);
    // Off: the window is the plain presenter's again (top left, as the CPU path's native picture would be).
    assert!(bridge.command_ok("pocket_picture off"));
    assert!(!bridge.command_ok("pocket_picture nonsense"), "settings that don't read are refused");
}

/// The pause menu's background on the Brick: with the menu on top, a Picture choice made in it is drawn into the menu shot by the same
/// renderer at once (`pocket_picture_shot`), as on ROCKNIX; native and fullscreen give different pictures.
#[test]
fn a_picture_choice_made_in_the_pause_menu_shows_in_its_background_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let shots = tempfile::tempdir().unwrap();
    let bridge = gl_bridge(None, "pbuffer", log.path(), dir);
    if std::fs::read_to_string(log.path()).unwrap().contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    }
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    assert!(!bridge.command_ok("pocket_picture_shot 1 64 48 /nonexistent-dir/x"), "no settings yet: nothing to draw");
    assert!(bridge.command_ok(&picture_command(&[("scaling", "native"), ("sharpness", "sharp"), ("effect", "none")])));
    game.show_size(4, 2, RED);
    std::thread::sleep(Duration::from_millis(300));
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    std::thread::sleep(Duration::from_millis(200));
    let native = shots.path().join("native.bin");
    assert!(bridge.command_ok(&format!("pocket_picture_shot 7 64 48 {}", native.display())));
    assert!(bridge.command_ok(&picture_command(&[("scaling", "fullscreen"), ("sharpness", "sharp"), ("effect", "none")])));
    let full = shots.path().join("full.bin");
    assert!(bridge.command_ok(&format!("pocket_picture_shot 8 64 48 {}", full.display())));
    let (a, b) = (std::fs::read(&native).unwrap(), std::fs::read(&full).unwrap());
    assert!(a.len() > 64 * 48 && b.len() == a.len(), "two menu shots of the same size");
    assert_ne!(a, b, "the choice made with the menu on top shows in the next menu shot");
    assert!(std::fs::read_to_string(log.path()).unwrap().contains("menu shot 8: 64x48 by the renderer"), "the log says so");
    // Zoom (0.2) crops to the game: the shot says where it is, the picture without its black bands; fullscreen is the whole screen.
    let shot = |file: &Path, seq| pocket_ui::preview::wait_for_shot(file, seq, Duration::ZERO).expect("a menu shot");
    let (n, f) = (shot(&native, 7), shot(&full, 8));
    assert_eq!((n.pixels.w, n.pixels.h, f.pixels.w, f.pixels.h), (64, 48, 64, 48), "the picture is the screen's size");
    let (_, _, nw, nh) = n.game.expect("native: the game's rectangle");
    assert!(nw == 64 && nh < 48, "native 4x2 at 16x: the full width, black above and below ({nw}x{nh})");
    assert_eq!(f.game, Some((0, 0, 64, 48)), "fullscreen: the game is the whole screen");
}

/// The owner on update 5 (03.11.): a Picture choice made in the pause menu did nothing, neither behind the menu nor back in the game (update 3,
/// without the menu shot, applied it on the way back). The device's order: the game at native size, the menu on top, a menu shot, a
/// change (settings, then a shot as pocket-core sends them), the Picture page's preview, the menu goes, the game's next picture: drawn the
/// new way.
#[test]
fn a_picture_choice_made_in_the_pause_menu_holds_when_the_game_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let shots = tempfile::tempdir().unwrap();
    let bridge = gl_bridge(None, "pbuffer", log.path(), dir);
    if std::fs::read_to_string(log.path()).unwrap().contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    }
    let px = |b: &Bridge, x: usize, y: usize| -> [u8; 3] { b.fb()[(y * W as usize + x) * 4..][..3].try_into().unwrap() };
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    assert!(bridge.command_ok(&picture_command(&[("scaling", "native"), ("sharpness", "sharp"), ("effect", "none")])));
    game.show_size(4, 2, RED);
    wait_for("the picture at 16x", || px(&bridge, 32, 24) == RED_ON_FB[..3]);
    assert_eq!(px(&bridge, 2, 2), [0, 0, 0], "native: black in the corner");
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, GREEN);
    wait_for("the menu", || px(&bridge, 0, 0) == GREEN_ON_FB[..3]);
    let before = shots.path().join("before.bin");
    assert!(bridge.command_ok(&format!("pocket_picture_shot 1 64 48 {}", before.display())));
    assert!(bridge.command_ok(&picture_command(&[("scaling", "fullscreen"), ("sharpness", "sharp"), ("effect", "none")])));
    let after = shots.path().join("after.bin");
    assert!(bridge.command_ok(&format!("pocket_picture_shot 2 64 48 {}", after.display())));
    assert_ne!(std::fs::read(&before).unwrap(), std::fs::read(&after).unwrap(), "the menu shot shows the choice");
    assert_eq!(px(&bridge, 0, 0), GREEN_ON_FB[..3], "the menu stays on the screen");
    // The Picture page's live preview (`pocket_picture_preview`): the centre at 1:1 and the whole screen, by the same renderer. Fullscreen
    // fills the overview's corner with the picture; native leaves it black.
    let corner = |file: &Path| -> u32 {
        let (_, images) = pocket_ui::preview::decode(&std::fs::read(file).unwrap()).expect("a preview file");
        assert_eq!(images.len(), 2, "the crop and the overview");
        images[1].px[0] & 0xff_ffff
    };
    let full_preview = shots.path().join("full-preview.bin");
    assert!(bridge.command_ok(&format!("pocket_picture_preview 3 64 48 32 24 {}", full_preview.display())));
    assert!(bridge.command_ok(&picture_command(&[("scaling", "native"), ("sharpness", "sharp"), ("effect", "none")])));
    let native_preview = shots.path().join("native-preview.bin");
    assert!(bridge.command_ok(&format!("pocket_picture_preview 4 64 48 32 24 {}", native_preview.display())));
    assert_ne!(corner(&full_preview), 0, "fullscreen: the picture in the corner");
    assert_eq!(corner(&native_preview), 0, "native: black in the corner");
    assert!(std::fs::read_to_string(log.path()).unwrap().contains("pocket-fb: preview 4: "), "the log says so");
    assert!(bridge.command_ok(&picture_command(&[("scaling", "fullscreen"), ("sharpness", "sharp"), ("effect", "none")])));
    assert!(bridge.command_ok(&format!(r#"{UI} fullscreen disable; {GAMES} focus"#)));
    game.show_size(4, 2, RED);
    wait_for("the game back, over the whole screen", || px(&bridge, 2, 2) == RED_ON_FB[..3] && px(&bridge, 61, 45) == RED_ON_FB[..3]);
}

/// Over the game on the Brick (the owner, update 5: the FPS line never showed): pocket-core's overlay texts go to the bridge
/// (`pocket_overlay`), which draws them over the game it draws with the renderer, and takes them away again.
#[test]
fn over_the_game_is_drawn_by_the_bridge_over_the_picture() {
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let bridge = gl_bridge_sized((320, 240), None, "pbuffer", log.path(), dir);
    if std::fs::read_to_string(log.path()).unwrap().contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    }
    // The top of the screen, where the FPS line goes (above a native picture, which leaves it black): its pill and its light text.
    let lit = |b: &Bridge| b.fb().chunks_exact(4).take(320 * 80).filter(|p| p[0] > 0x80 && p[1] > 0x80 && p[2] > 0x80).count();
    let mut game = Client::connect(&bridge.path("w"), "pocket-core");
    assert!(bridge.command_ok(&picture_command(&[("scaling", "native"), ("sharpness", "sharp"), ("effect", "none")])));
    game.show_size(4, 2, RED);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(lit(&bridge), 0, "nothing over the game yet");
    let fps = ["60.0 FPS".to_string(), String::new(), String::new(), String::new()];
    assert!(bridge.command_ok(&pocket_ui::brick_picture::overlay_command(&fps)));
    game.show_size(4, 2, RED);
    wait_for("the FPS line over the game", || lit(&bridge) > 20);
    assert!(bridge.command_ok(&pocket_ui::brick_picture::overlay_command(&Default::default())));
    game.show_size(4, 2, RED);
    wait_for("the line gone", || lit(&bridge) == 0);
    assert!(!bridge.command_ok("pocket_overlay fps=zz"), "texts that don't read are refused");
}

#[test]
fn without_the_gpu_the_bridge_takes_no_picture_settings() {
    let bridge = Bridge::start(32);
    assert!(!bridge.command_ok("pocket_shaders"));
    assert!(!bridge.command_ok(&picture_command(&[("effect", "crt")])));
}

/// What the player sees of every Picture choice in the pause menu, on the Brick's screen (1024 by 768) with a GBA picture (240 by 160):
/// the game frame read back from the screen before and after each change must differ, and the geometry must be what the choice says.
/// Mesa's software GPU on a pbuffer; the same renderer the Brick's GPU runs, so what the device can add is only its speed and colours.
#[test]
fn every_picture_choice_changes_the_game_frame() {
    const SW: usize = 1024;
    const SH: usize = 768;
    let dir = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let bridge = gl_bridge_sized((SW as u32, SH as u32), None, "pbuffer", log.path(), dir);
    if std::fs::read_to_string(log.path()).unwrap().contains("GL failed") {
        eprintln!("no software GPU here, this test has nothing to check");
        return;
    }
    assert!(bridge.command_ok("pocket_shaders"));
    let mut game = Client::connect_pool(&bridge.path("w"), "pocket-core", 240 * 160 * 4);
    // Checkers of 8 pixels in two colours with a ramp in green, no black in it: the bounding box of what is not black is the picture.
    let mut pixels = Vec::with_capacity(240 * 160 * 4);
    for y in 0..160u32 {
        for x in 0..240u32 {
            let checker = ((x / 8) + (y / 8)) % 2 == 0;
            let (r, b) = if checker { (210u8, 70u8) } else { (70, 210) };
            pixels.extend_from_slice(&[b, 40 + (x * 150 / 240) as u8, r, 0]);
        }
    }
    game.show_pixels(240, 160, &pixels);
    let base: Vec<(&str, &str)> = vec![("scaling", "native"), ("sharpness", "sharp"), ("effect", "none"), ("frame", "none"), ("orientation", "normal")];
    let frames = tempfile::tempdir().unwrap();
    let command = |changes: &[(&str, &str)], turns: u32| {
        let mut values: BTreeMap<String, String> = base.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        values.extend(changes.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        pocket_ui::brick_picture::Request { system: "gba".into(), aspect: 1.5, turns, frames_dir: frames.path().to_path_buf(), values }.command()
    };
    // The screen once it stopped changing.
    let screen = |bridge: &Bridge| {
        let mut last = bridge.fb();
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(25));
            let now = bridge.fb();
            if now == last {
                return now;
            }
            last = now;
        }
        last
    };
    let lit = |p: &[u8], x: usize, y: usize| p[(y * SW + x) * 4..][..3].iter().any(|&c| c != 0);
    // The box of everything that is not black: (x0, y0, width, height).
    let bbox = |p: &[u8]| {
        let (mut x0, mut y0, mut x1, mut y1) = (SW, SH, 0, 0);
        for y in 0..SH {
            for x in 0..SW {
                if lit(p, x, y) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        if x1 < x0 { (0, 0, 0, 0) } else { (x0, y0, x1 + 1 - x0, y1 + 1 - y0) }
    };
    let mut seen: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    // A change is applied to the picture on show at once (the pause menu's `set`), then the screen is read back and compared with the base.
    // A frame is prepared on a thread and drawn with the game's next picture, which a running game sends sixty times a second.
    // `base` is what the change is compared with: a frame that is slow to prepare (a loaded machine, four tests at once) gets up to five
    // seconds of pictures to land on the screen before the choice counts as one that left it as it was.
    let apply = |bridge: &Bridge, game: &mut Client, changes: &[(&str, &str)], turns: u32, base: &[u8]| -> Vec<u8> {
        assert!(bridge.command_ok(&command(changes, turns)), "{changes:?} was refused");
        let mut now = Vec::new();
        for round in 0..36 {
            std::thread::sleep(Duration::from_millis(150));
            game.show_pixels(240, 160, &pixels);
            if round >= 2 {
                now = screen(bridge);
                if now != base {
                    break;
                }
            }
        }
        now
    };
    assert!(bridge.command_ok(&command(&[], 0)));
    let before = screen(&bridge);
    assert_eq!(bbox(&before), (32, 64, 960, 640), "Integer: 4x, centred, black around");
    let check = |what: &str, after: &[u8], before: &[u8], seen: &mut Vec<String>, unchanged: &mut Vec<String>| {
        let differing = before.iter().zip(after).filter(|(a, b)| a != b).count();
        seen.push(format!("{what}: {differing} bytes differ, picture box {:?}", bbox(after)));
        if differing == 0 {
            unchanged.push(what.to_string());
        }
    };
    // Scaling: whole steps, aspect, fullscreen.
    for (value, want) in [("integer-smaller", (152, 144, 720, 480)), ("aspect", (0, 42, 1024, 683)), ("fullscreen", (0, 0, 1024, 768))] {
        let after = apply(&bridge, &mut game, &[("scaling", value)], 0, &before);
        check(&format!("scaling {value}"), &after, &before, &mut seen, &mut unchanged);
        // One step down from 4x is 3x (720 by 480), centred.
        assert_eq!(bbox(&after), want, "scaling {value}");
    }
    // Sharpness and effects at 4x, where a pixel is four screen pixels wide: the edges and the rows differ.
    // Smooth pixels leaves these checkers as they are (it only rounds one-texel stair-steps); tests/picture_shaders.rs shows it on the GPU.
    // Crisp is Sharp at a whole-number scale (it scales up sharply first, then smooths what is left over): it shows at Aspect.
    let aspect = apply(&bridge, &mut game, &[("scaling", "aspect")], 0, &before);
    let crisp = apply(&bridge, &mut game, &[("scaling", "aspect"), ("sharpness", "crisp")], 0, &aspect);
    check("sharpness crisp (Aspect)", &crisp, &aspect, &mut seen, &mut unchanged);
    for (key, value) in [("sharpness", "soft"), ("effect", "grid"), ("effect", "scanlines"), ("effect", "crt"), ("effect", "softglow")] {
        let after = apply(&bridge, &mut game, &[(key, value)], 0, &before);
        check(&format!("{key} {value}"), &after, &before, &mut seen, &mut unchanged);
    }
    let grid = apply(&bridge, &mut game, &[("effect", "grid")], 0, &before);
    let lcd = apply(&bridge, &mut game, &[("effect", "grid"), ("lcd_colour", "on")], 0, &grid);
    check("lcd_colour on (under the LCD grid)", &lcd, &grid, &mut seen, &mut unchanged);
    // Frames (the picture leaves room at 4x): each one against no frame.
    let frames = ["auto", "ambient", "shell", "brick", "pocket", "clamshell", "wide", "gloss", "crt", "studio", "portable-tv", "woodgrain", "arcade", "glow"];
    for frame in frames {
        let after = apply(&bridge, &mut game, &[("frame", frame)], 0, &before);
        check(&format!("frame {frame}"), &after, &before, &mut seen, &mut unchanged);
    }
    for (key, values) in [("frame_colour", &["indigo", "clear-purple", "atomic-purple", "black", "white", "glacier"][..]), ("frame_brick", &["clear", "red", "yellow", "green", "black"]), ("frame_pocket", &["clear"]), ("frame_clamshell", &["cobalt", "flame", "graphite"])] {
        let frame = match key { "frame_colour" => "shell", "frame_brick" => "brick", "frame_pocket" => "pocket", _ => "clamshell" };
        let reference = apply(&bridge, &mut game, &[("frame", frame)], 0, &before);
        for v in values {
            let after = apply(&bridge, &mut game, &[("frame", frame), (key, v)], 0, &reference);
            check(&format!("{frame} {key} {v}"), &after, &reference, &mut seen, &mut unchanged);
        }
    }
    let reference = apply(&bridge, &mut game, &[("frame", "crt")], 0, &before);
    let after = apply(&bridge, &mut game, &[("frame", "crt"), ("frame_tv", "wood")], 0, &reference);
    check("crt frame_tv wood", &after, &reference, &mut seen, &mut unchanged);
    // Orientation: the picture stands up (taller than wide); a core's own turn does the same.
    for value in ["left", "right"] {
        let after = apply(&bridge, &mut game, &[("orientation", value)], 0, &before);
        check(&format!("orientation {value}"), &after, &before, &mut seen, &mut unchanged);
        let (_, _, w, h) = bbox(&after);
        assert!(h > w, "orientation {value}: {w} by {h}");
    }
    let after = apply(&bridge, &mut game, &[], 1, &before);
    check("core rotation 1", &after, &before, &mut seen, &mut unchanged);
    for line in &seen {
        eprintln!("PROOF {line}");
    }
    assert!(unchanged.is_empty(), "choices that left the screen as it was: {unchanged:?}");
}

/// docs/TRIMUI-BRICK-MORE.md: the card's `grim` stand-in takes the window on show from pocket-fb as a PNG (what the screenshot key, the pause
/// menu behind a standalone emulator and the web companion's screen view call), on stdout or in a file; nothing to take is exit 1.
#[test]
fn the_grim_stand_in_hands_over_the_window_on_show_as_a_png() {
    let bridge = Bridge::start(32);
    let grim = |args: &[&str]| {
        Command::new("sh")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/deploy/brick/hwroot/usr/bin/grim"))
            .args(args)
            .env("BRICK_FB", env!("CARGO_BIN_EXE_pocket-fb"))
            .env("SWAYSOCK", bridge.path("i"))
            .env("XDG_RUNTIME_DIR", bridge.dir.path())
            .stderr(Stdio::null())
            .output()
            .unwrap()
    };
    assert_eq!(grim(&["-t", "png", "-"]).status.code(), Some(1), "no window yet: nothing to take");
    let mut ui = Client::connect(&bridge.path("w"), "pocket-ui");
    ui.show(0, RED);
    wait_for("the window on the screen", || bridge.pixel() == RED_ON_FB);
    let out = grim(&["-t", "png", "-"]);
    assert!(out.status.success());
    let mut reader = png::Decoder::new(&out.stdout[..]).read_info().expect("a PNG on stdout");
    let mut rgb = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut rgb).unwrap();
    assert_eq!((info.width, info.height), (W, H));
    assert_eq!(&rgb[..3], [0xff, 0, 0], "red stays red (the shm bytes are blue, green, red)");
    let file = bridge.path("shot.png");
    assert!(grim(&[file.to_str().unwrap()]).status.success());
    assert_eq!(std::fs::read(&file).unwrap(), out.stdout, "the same picture in a file");
    let left: Vec<_> = std::fs::read_dir(bridge.dir.path()).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("grim-")).collect();
    assert!(left.is_empty(), "no temporary file is left behind");
}
