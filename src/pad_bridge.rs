//! Standalone SDL emulators receive the same identity-bound layouts as pocket-core through four
//! stable uinput seats. The wrapper lives in the game's process group, never on the UI thread.
use super::{Button, Mux, PadEvent};
#[cfg(test)]
use super::Axis;
use crate::players::{Mode, Order, Seats, MAX};
use evdev::{uinput::VirtualDevice, AbsInfo, AbsoluteAxisCode as Abs, AttributeSet, BusType, InputEvent, InputId, KeyCode, UinputAbsSetup};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};
use std::collections::BTreeMap;
use evdev::{EventSummary, FFEffectCode, FFEffectData, FFEffectKind, UInputCode};

pub const NAME: &str = "pocket-ui Controller";
const VENDOR: u16 = 0x1209;

pub fn wrap(original: &Command) -> std::io::Result<Command> {
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--controller-bridge").arg(original.get_program()).args(original.get_args());
    for (k, v) in original.get_envs() { if let Some(v) = v { cmd.env(k, v); } else { cmd.env_remove(k); } }
    if let Some(dir) = original.get_current_dir() { cmd.current_dir(dir); }
    crate::control::die_with_parent(&mut cmd);
    Ok(cmd)
}

pub fn device(player: usize) -> std::io::Result<VirtualDevice> { device_id(player as u16 + 1) }
fn device_id(product: u16) -> std::io::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for b in Button::ALL { if let Some(k) = key(b) { keys.insert(k); } }
    let mut builder = VirtualDevice::builder()?.name(NAME).input_id(InputId::new(BusType::BUS_USB, VENDOR, product, 1)).with_keys(&keys)?.with_ff(&AttributeSet::from_iter([FFEffectCode::FF_RUMBLE]))?.with_ff_effects_max(16);
    for code in [Abs::ABS_X, Abs::ABS_Y, Abs::ABS_RX, Abs::ABS_RY] {
        builder = builder.with_absolute_axis(&UinputAbsSetup::new(code, AbsInfo::new(0, -32767, 32767, 0, 0, 0)))?;
    }
    for code in [Abs::ABS_Z, Abs::ABS_RZ] {
        builder = builder.with_absolute_axis(&UinputAbsSetup::new(code, AbsInfo::new(0, 0, 32767, 0, 0, 0)))?;
    }
    let device = builder.build()?;
    // Reader and feedback requests share this worker; no blocking read may delay input.
    let fd = device.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 { return Err(std::io::Error::last_os_error()); }
    Ok(device)
}

/// A full state (not edges) also clears held buttons and sticks after a disconnect or a seat change.
pub fn events(buttons: u32, axes: [f32; 6]) -> Vec<InputEvent> {
    events_for(buttons, axes, &super::bindings::Layout::default())
}
fn events_for(buttons: u32, axes: [f32; 6], layout: &super::bindings::Layout) -> Vec<InputEvent> {
    let mut events: Vec<_> = Button::ALL.into_iter().filter_map(|b| key(b).map(|k| InputEvent::new(1, k.0, i32::from(buttons & b.bit() != 0)))).collect();
    for (i, code) in [Abs::ABS_X, Abs::ABS_Y, Abs::ABS_RX, Abs::ABS_RY].iter().enumerate() {
        events.push(InputEvent::new(3, code.0, (axes[i].clamp(-1.0, 1.0) * 32767.0) as i32));
    }
    // Unchanged analogue triggers keep their travel; remapped triggers follow button edges.
    // A digital trigger has no axis report and therefore uses its button as full travel.
    for (i, code, b) in [(4, Abs::ABS_Z, Button::L2), (5, Abs::ABS_RZ, Button::R2)] {
        let digital = if buttons & b.bit() != 0 { 1.0 } else { 0.0 };
        let value = if layout.role(b) == b && axes[i] > 0.0 { axes[i].clamp(0.0, 1.0) } else { digital };
        events.push(InputEvent::new(3, code.0, (value * 32767.0) as i32));
    }
    events
}

fn key(b: Button) -> Option<KeyCode> {
    Some(match b {
        Button::South => KeyCode::BTN_SOUTH, Button::East => KeyCode::BTN_EAST,
        Button::North => KeyCode::BTN_NORTH, Button::West => KeyCode::BTN_WEST,
        Button::L1 => KeyCode::BTN_TL, Button::R1 => KeyCode::BTN_TR,
        Button::L2 => KeyCode::BTN_TL2, Button::R2 => KeyCode::BTN_TR2,
        Button::L3 => KeyCode::BTN_THUMBL, Button::R3 => KeyCode::BTN_THUMBR,
        Button::Select => KeyCode::BTN_SELECT, Button::Start => KeyCode::BTN_START, Button::Mode => KeyCode::BTN_MODE,
        Button::Up => KeyCode::BTN_DPAD_UP, Button::Down => KeyCode::BTN_DPAD_DOWN,
        Button::Left => KeyCode::BTN_DPAD_LEFT, Button::Right => KeyCode::BTN_DPAD_RIGHT,
        _ => return None,
    })
}

fn sdl_mappings(base: u16) -> String {
    (0..MAX).map(|p| {
        let product = base + p as u16;
        format!("0300000009120000{:02x}{:02x}000001000000,{NAME},a:b0,b:b1,x:b3,y:b2,leftshoulder:b4,rightshoulder:b5,lefttrigger:a2,righttrigger:a5,back:b8,start:b9,guide:b10,leftstick:b11,rightstick:b12,dpup:b13,dpdown:b14,dpleft:b15,dpright:b16,leftx:a0,lefty:a1,rightx:a3,righty:a4,platform:Linux,", product & 255, product >> 8)
    }).collect::<Vec<_>>().join("\n")
}

/// A frozen standalone keeps its devices. Another game must get different SDL IDs, so its
/// whitelist cannot accidentally select the frozen game's seats. Locks survive SIGKILL safely.
fn id_group() -> std::io::Result<(u16, std::fs::File)> {
    use std::os::unix::fs::OpenOptionsExt;
    for offset in 0..16383u32 {
        let group = (std::process::id() + offset) % 16383;
        let base = (group * 4 + 1) as u16;
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(format!("/tmp/pocket-controller-id-{base}.lock"))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 { return Ok((base, file)); }
    }
    Err(std::io::Error::other("no free controller bridge IDs"))
}

#[derive(Default)]
struct Feedback { effects: BTreeMap<i16, (FFEffectData, Option<(Instant, Instant)>)> }
impl Feedback {
    fn poll(&mut self, dev: &mut VirtualDevice, player: usize) -> std::io::Result<()> {
        let events: Vec<_> = match dev.fetch_events() {
            Ok(e) => e.collect(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Vec::new(), Err(e) => return Err(e),
        };
        for event in events {
            match event.destructure() {
                EventSummary::UInput(e, UInputCode::UI_FF_UPLOAD, _) => {
                    let mut upload = dev.process_ff_upload(e)?;
                    let data = upload.effect();
                    let old = upload.effect_id();
                    let id = if self.effects.contains_key(&old) { Some(old) } else { (0..16).find(|i| !self.effects.contains_key(i)) };
                    if let (Some(id), FFEffectKind::Rumble { .. }) = (id, data.kind) {
                        let playing = self.effects.get(&id).and_then(|(_, playing)| *playing);
                        self.effects.insert(id, (data, playing)); upload.set_effect_id(id); upload.set_retval(0);
                    } else { upload.set_retval(-libc::EINVAL); }
                }
                EventSummary::UInput(e, UInputCode::UI_FF_ERASE, _) => { let erase = dev.process_ff_erase(e)?; self.effects.remove(&(erase.effect_id() as i16)); }
                EventSummary::ForceFeedback(_, id, count) => {
                    if let Some((effect, playing)) = self.effects.get_mut(&(id.0 as i16)) {
                        let start = Instant::now() + Duration::from_millis(effect.replay.delay as u64);
                        *playing = (count > 0).then(|| (start, start + Duration::from_millis(effect.replay.length.max(1) as u64 * count as u64)));
                    }
                }
                _ => {}
            }
        }
        let now = Instant::now();
        let (mut strong, mut weak) = (0,0);
        for (data, playing) in self.effects.values() {
            if playing.is_some_and(|(start,end)| now >= start && now < end) {
                if let FFEffectKind::Rumble { strong_magnitude, weak_magnitude } = data.kind {
                    strong = strong.max(strong_magnitude); weak = weak.max(weak_magnitude);
                }
            }
        }
        crate::rumble::core_rumble(player, 0, strong);
        crate::rumble::core_rumble(player, 1, weak);
        Ok(())
    }
}

/// The standalone emulators that get the bridge's seats (`wrap`): those that find a pad by its SDL name (Dolphin's
/// `Device = SDL/0/<name>`, which start_dolphin_gc.sh writes from what SDL reports) or its index (ARMSX2's `SDL-0`).
/// Not Azahar: its qt-config.ini binds every button to the built-in pad's SDL GUID, and a bridge pad's GUID is
/// another one, new on every start (`id_group`), so a 3DS game had no buttons at all (review 0.3).
pub fn bridged(emulator: &str) -> bool {
    matches!(emulator, "dolphin" | "armsx2")
}

/// Invoked before the graphical main. Returns the external emulator's status; no real controller is grabbed.
/// When the seats can't be made (no uinput, no event node) the game still starts, with the pads as ROCKNIX set
/// them up: a missing bridge must never keep a GameCube or PS2 game from starting.
pub fn run(program: &std::ffi::OsStr, args: &[std::ffi::OsString], data: &Path) -> std::io::Result<i32> {
    if let (Some(system), Some(rom)) = (args.iter().find_map(|a| a.to_str().and_then(|a| a.strip_prefix("-P"))), args.first()) {
        let settings = crate::options::GameSettings::load(data, system, Path::new(rom));
        crate::rumble::set_strength(crate::extras::Settings::from_values(&settings.values).rumble);
    }
    match seats() {
        Ok((base, ids, devices)) => run_bridged(program, args, data, base, ids, devices),
        Err(e) => {
            eprintln!("controller bridge: {e}; the game starts with its own controller setup");
            let mut cmd = Command::new(program);
            cmd.args(args);
            crate::control::die_with_parent(&mut cmd);
            Ok(cmd.status()?.code().unwrap_or(1))
        }
    }
}

/// The four uinput seats of one game, their id group and its lock (`id_group`), with every event node present.
fn seats() -> std::io::Result<(u16, std::fs::File, Vec<VirtualDevice>)> {
    let (base, ids) = id_group()?;
    eprintln!("controller bridge: ids {base}..{}", base + MAX as u16 - 1);
    let mut devices: Vec<_> = (0..MAX).map(|p| device_id(base + p as u16)).collect::<std::io::Result<_>>()?;
    // Wait for our own nodes only; enumeration also waits for udev permissions before SDL starts.
    for d in &mut devices { for node in d.enumerate_dev_nodes_blocking()? {
        let node = node?;
        for _ in 0..200 { if node.exists() { break; } std::thread::sleep(Duration::from_millis(5)); }
        if !node.exists() { return Err(std::io::Error::other("uinput event node did not appear")); }
    } }
    Ok((base, ids, devices))
}

fn run_bridged(program: &std::ffi::OsStr, args: &[std::ffi::OsString], data: &Path, base: u16, _ids: std::fs::File, mut devices: Vec<VirtualDevice>) -> std::io::Result<i32> {
    let (tx, rx) = mpsc::channel();
    super::linux::supervise(move |e| tx.send(e).is_ok());
    let mut mux = Mux::default();
    while let Ok(e) = rx.try_recv() { mux.feed(e); }
    let order = Order::load(data);
    let mut seats = Seats::arrange(&order.order, &mux.controllers(), Mode::Compact);
    let worker = std::thread::spawn(move || {
        let mut last: [Option<(u32, [f32; 6])>; MAX] = Default::default();
        let mut feedback: [Feedback; MAX] = Default::default();
        loop {
            let event = match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(e) => Some(e), Err(mpsc::RecvTimeoutError::Timeout) => None, Err(_) => return,
            };
            if let Some(e) = event {
            let roster = matches!(e, PadEvent::Connected { .. } | PadEvent::Disconnected { .. });
            mux.feed(e);
            if roster {
                let connected = mux.controllers();
                for c in &connected { seats.connect(c); }
                let gone: Vec<_> = (0..MAX).filter_map(|p| seats.get(p)).filter(|c| !connected.iter().any(|n| n.key == c.key)).map(|c| c.key.clone()).collect();
                for key in gone { seats.disconnect(&key); }
            }
            }
            crate::rumble::set_targets(std::array::from_fn(|p| match seats.get(p) {
                Some(c) if c.is_builtin() => crate::rumble::Target::Motor,
                Some(c) => crate::rumble::Target::Node(c.node.clone()),
                None => crate::rumble::Target::None,
            }));
            for p in 0..MAX {
                if feedback[p].poll(&mut devices[p], p).is_err() { return; }
                let state = seats.get(p).map(|c| (mux.buttons_of(&c.key), mux.axes_of(&c.key))).unwrap_or((0, [0.0; 6]));
                if last[p] != Some(state) {
                    let layout = seats.get(p).map(|c| super::bindings::layout(&c.key)).unwrap_or_default();
                    if devices[p].emit(&events_for(state.0, state.1, &layout)).is_err() { return; }
                    last[p] = Some(state);
                }
            }
        }
    });
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("SDL_JOYSTICK_HIDAPI", "0")
        .env("SDL_GAMECONTROLLERCONFIG", sdl_mappings(base))
        .env("SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT", (0..MAX).map(|p| format!("0x{VENDOR:04x}/0x{:04x}", base + p as u16)).collect::<Vec<_>>().join(","));
    crate::control::die_with_parent(&mut cmd);
    let status = cmd.status()?;
    // This process exits now, closing precisely its own uinput devices and reader threads.
    drop(worker);
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Review 0.3: Azahar binds its buttons to the built-in pad's SDL GUID, a bridge pad's GUID is new on every start
    /// (two starts in the vdev: 0300052109120000 1938… and 3d42…): through the bridge a 3DS game had no buttons.
    #[test]
    fn a_3ds_game_keeps_the_pad_its_emulator_is_bound_to() {
        assert!(!bridged("azahar"), "Azahar's qt-config.ini names the built-in pad's GUID");
        assert!(bridged("dolphin") && bridged("armsx2"), "they find the seat by its SDL name or index");
        assert_ne!(sdl_mappings(1)[..32], sdl_mappings(5)[..32], "another start, another GUID");
    }

    /// Review 0.3: when the seats can't be made (no access to /dev/uinput here, as on a box without the module), the
    /// game must still start: before, the bridge ended with an error and a GameCube or PS2 game never came up.
    #[test]
    fn a_game_starts_even_when_the_bridge_cannot_make_its_seats() {
        if std::fs::OpenOptions::new().write(true).open("/dev/uinput").is_ok() {
            eprintln!("skipped: /dev/uinput is writable here, the seats can be made");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let code = run("sh".as_ref(), &["-c".into(), "exit 7".into()], dir.path()).expect("the game runs without the seats");
        assert_eq!(code, 7, "the emulator's own exit status");
    }

    #[test]
    fn unchanged_triggers_keep_half_travel_for_analogue_games() {
        let mut axes = [0.0; 6]; axes[4] = 0.3;
        assert!(events(0, axes).iter().any(|e| e.event_type().0 == 3 && e.code() == Abs::ABS_Z.0 && (9000..11000).contains(&e.value())));
    }
    #[test]
    fn remapped_trigger_cannot_leak_its_old_analogue_action_to_a_standalone() {
        let mut axes = [0.0; 6]; axes[Axis::L2 as usize] = 1.0;
        let mut layout = super::super::bindings::Layout::default();
        layout.assign(Button::L2, Button::South);
        let e = events_for(Button::South.bit(), axes, &layout);
        assert!(e.iter().any(|e| e.event_type().0 == 1 && e.code() == KeyCode::BTN_SOUTH.0 && e.value() == 1));
        assert!(e.iter().any(|e| e.event_type().0 == 3 && e.code() == Abs::ABS_Z.0 && e.value() == 0));
    }
}
