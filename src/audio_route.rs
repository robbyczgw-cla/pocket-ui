//! Where the sound goes, the Pocket's speaker or a Bluetooth sink (headphones, a speaker), and the PipeWire quantum
//! that suits it (docs/HOST.md "Bluetooth audio").
//!
//! - pocket-core lowers PipeWire's graph quantum from ROCKNIX's 1024 frames to 256 while a game plays, for a short way
//!   to the speaker (`audio::GAME_QUANTUM`). A Bluetooth sink (`bluez_output.*`) is a driver of its own in the
//!   graph (PipeWire's bluez5 `media-sink.c`: `node.driver = true`) and sends A2DP packets whose size has nothing to
//!   do with the quantum ("the quantum and packet size aren't correlated"); its delay is "packet delay + codec
//!   internal delay + transport delay" and "doesn't depend on the quantum". So 256 gains nothing there and only makes
//!   the graph wake four times as often, with crackles when a cycle comes late; on Bluetooth ROCKNIX's quantum stays.
//! - The default sink is what `pw-metadata -n default 0 default.audio.sink` says (about 11 ms on the Mini V2 under
//!   load). It is asked only when the kernel has a Bluetooth connection (`/sys/class/bluetooth/hci0:<handle>`, one per
//!   link, added and removed by the kernel's `hci_conn_add_sysfs`/`hci_conn_del_sysfs`): without one no Bluetooth
//!   sink can exist, and reading a directory costs microseconds. Missing data (no `pw-metadata`, it fails, odd
//!   output) counts as the speaker, which is what pocket-core did before.
//! - [`Keeper`]: a thread per game that sets the quantum for the output, follows a change while the game runs (every
//!   [`CHECK_EVERY`] it looks at the connections; for [`SETTLE`] after one came or went it asks for the default sink,
//!   since the sink appears a few seconds after the link), sets it again after a wake, and puts ROCKNIX's back at the
//!   end unless the next game keeps it (`keep`). Nothing of this runs on the emulation or UI thread, and none of it
//!   before the game's first frame: `Audio::open` only starts the thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Where PipeWire's default sink plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// The Pocket's own speaker (or anything that isn't Bluetooth: a USB or HDMI sink, or nothing known).
    Speaker,
    /// A Bluetooth sink: headphones or a speaker.
    Bluetooth,
}

/// The node name of the default sink in `pw-metadata -n default 0 default.audio.sink`'s output, e.g.
/// `update: id:0 key:'default.audio.sink' value:'{"name":"bluez_output.AA_BB.1"}' type:'Spa:String:JSON'`.
/// None for anything else (no such line, `default.configured.audio.sink` only, no name in the value).
pub fn default_sink_name(out: &str) -> Option<&str> {
    out.lines().find_map(|line| {
        let value = line.split_once("key:'default.audio.sink'")?.1.trim_start().strip_prefix("value:'")?;
        let value = &value[..value.find('\'')?];
        let name = value.split_once("\"name\"")?.1.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
        let name = &name[..name.find('"')?];
        (!name.is_empty()).then_some(name)
    })
}

/// The output a sink plays on: Bluetooth for bluez5's sinks, the speaker for any other or none.
pub fn output_of(sink: Option<&str>) -> Output {
    match sink {
        Some(name) if name.starts_with("bluez_output.") => Output::Bluetooth,
        _ => Output::Speaker,
    }
}

/// Where the sound goes, as Quick settings tells it beside Volume (`sound_of`, read on a worker by `Hw::sound`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sound {
    Speaker,
    /// The wired jack: the codec's `HiFi__Headphones__sink`.
    Headphones,
    Usb,
    Hdmi,
    /// A Bluetooth sink, with the device's name when BlueZ gave one.
    Bluetooth(Option<String>),
    /// PipeWire didn't say (no `pw-metadata` and no `pactl`, or no default sink): the row says so.
    Unknown,
}

/// Where the default sink `sink` plays. The wired jack is a sink of its own in ROCKNIX's UCM profile
/// (`..HiFi__Headphones__sink` next to `..HiFi__Speaker__sink`); it is the default only when something made it so.
pub fn sound_of(sink: Option<&str>) -> Sound {
    let Some(name) = sink else { return Sound::Unknown };
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("bluez_output.") {
        Sound::Bluetooth(None)
    } else if lower.contains("headphone") {
        Sound::Headphones
    } else if lower.contains("hdmi") || lower.contains("displayport") {
        Sound::Hdmi
    } else if lower.contains(".usb-") || lower.starts_with("alsa_output.usb") {
        Sound::Usb
    } else {
        Sound::Speaker
    }
}

/// The device address of a `bluez_output.AC_80_0A_12_34_56.1` node, "AC:80:0A:12:34:56".
pub fn sink_address(sink: &str) -> Option<String> {
    let id = sink.strip_prefix("bluez_output.")?.split('.').next()?;
    let parts: Vec<&str> = id.split('_').collect();
    (parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit())))
        .then(|| parts.join(":").to_ascii_uppercase())
}

/// The name PipeWire gives a node, from `pactl get-default-sink`'s one line.
fn pactl_default_sink(out: &str) -> Option<&str> {
    out.lines().map(str::trim).find(|l| !l.is_empty())
}

/// Where the sound goes now: two small helpers (and `bluetoothctl info` for a Bluetooth sink's name), so call it from
/// a worker, never the UI thread.
pub fn sound_now() -> Sound {
    let sink = Real.default_sink().or_else(|| {
        let out = crate::proc::run_with_timeout(std::process::Command::new("pactl").arg("get-default-sink"), crate::proc::HELPER)
            .ok()
            .filter(|o| o.status.success())?;
        pactl_default_sink(&String::from_utf8_lossy(&out.stdout)).map(str::to_string)
    });
    match sound_of(sink.as_deref()) {
        Sound::Bluetooth(_) => Sound::Bluetooth(sink.as_deref().and_then(sink_address).and_then(|a| bluetooth_name(&a))),
        other => other,
    }
}

fn bluetooth_name(address: &str) -> Option<String> {
    let bt = ["/usr/bin/bluetoothctl", "/bin/bluetoothctl"].into_iter().find(|p| std::path::Path::new(p).exists())?;
    let out = crate::proc::run_with_timeout(std::process::Command::new(bt).args(["info", address]), crate::proc::HELPER).ok()?;
    crate::bluetooth::parse_info(&String::from_utf8_lossy(&out.stdout)).and_then(|d| d.name)
}

/// The quantum a game sets for `output`: `configured` (`POCKET_CORE_QUANTUM`, 256 by default) on the speaker, none on
/// Bluetooth (ROCKNIX's stays) or when it is 0.
pub fn game_quantum(output: Output, configured: u32) -> Option<u32> {
    (output == Output::Speaker && configured > 0).then_some(configured)
}

/// The Bluetooth links among the names in `/sys/class/bluetooth` (`hci0:3585`; the adapters themselves, `hci0`, are
/// not links), sorted.
pub fn connections(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let link = |n: &str| {
        n.split_once(':').is_some_and(|(hci, handle)| {
            hci.strip_prefix("hci").is_some_and(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_digit()))
                && !handle.is_empty()
                && handle.bytes().all(|b| b.is_ascii_digit())
        })
    };
    let mut v: Vec<String> = names.into_iter().filter(|n| link(n)).collect();
    v.sort();
    v
}

/// Whether the default sink has to be asked: when there is a Bluetooth link, or the links couldn't be read (None).
fn may_be_bluetooth(links: &Option<Vec<String>>) -> bool {
    links.as_ref().is_none_or(|l| !l.is_empty())
}

/// How often a running game looks at the Bluetooth links.
pub const CHECK_EVERY: Duration = Duration::from_secs(2);
/// How long after a link came or went the default sink is asked at every check: WirePlumber makes a new Bluetooth
/// sink the default only once its audio profile is connected, a few seconds after the link.
pub const SETTLE: Duration = Duration::from_secs(16);

/// What the keeper reads and changes on the system; the tests give a fake.
pub trait System: Send + 'static {
    /// The names of the links in `/sys/class/bluetooth`, None when it can't be read.
    fn connections(&self) -> Option<Vec<String>>;
    /// The default sink's node name, None when `pw-metadata` is missing, fails or says something else.
    fn default_sink(&self) -> Option<String>;
    /// Forces the quantum; whether it worked.
    fn force(&self, frames: u32) -> bool;
    /// Puts ROCKNIX's quantum back.
    fn restore(&self);
}

/// The real system: sysfs, `pw-metadata`, `audio::force_quantum` and `audio::restore_quantum`.
pub struct Real;

impl System for Real {
    fn connections(&self) -> Option<Vec<String>> {
        let dir = std::fs::read_dir("/sys/class/bluetooth").ok()?;
        Some(connections(dir.filter_map(|e| e.ok()?.file_name().into_string().ok())))
    }

    fn default_sink(&self) -> Option<String> {
        let out = crate::proc::run_with_timeout(
            std::process::Command::new("pw-metadata").args(["-n", "default", "0", "default.audio.sink"]),
            crate::proc::HELPER,
        )
        .ok()
        .filter(|o| o.status.success())?;
        default_sink_name(&String::from_utf8_lossy(&out.stdout)).map(str::to_string)
    }

    fn force(&self, frames: u32) -> bool {
        crate::audio::force_quantum(frames)
    }

    fn restore(&self) {
        crate::audio::restore_quantum()
    }
}

/// The output now, asking PipeWire only when there is a Bluetooth link (UI sounds on the shelf).
pub fn current(sys: &impl System) -> Output {
    if may_be_bluetooth(&sys.connections()) {
        output_of(sys.default_sink().as_deref())
    } else {
        Output::Speaker
    }
}

/// The keeper's state: what it set, and what it last saw. Driven by [`Keeper`]'s thread, and by the tests directly.
/// The largest quantum `audio::Ring::Short` (10 ms, 480 frames, less two writer chunks of margin) is used under.
const SMALL_MAX: u32 = 256;

pub struct Router<S: System> {
    sys: S,
    /// The quantum asked for on the speaker (`POCKET_CORE_QUANTUM`); 0 never sets one (the pulse path, or turned off).
    frames: u32,
    output: Output,
    /// This game changed the quantum and has to put ROCKNIX's back.
    forced: bool,
    /// The next game takes over (`keep`): nothing is changed any more, nothing put back.
    kept: bool,
    /// pocket-ui started this game and puts ROCKNIX's quantum back itself when it ends (`POCKET_CORE_QUANTUM_OWNER`,
    /// `ui_sound::game_ended`), in order with its own and never under a sound; the end of the game leaves it.
    owner_restores: bool,
    links: Option<Vec<String>>,
    /// When the links last changed (or the game started with one), for `SETTLE`.
    changed: Option<Instant>,
    bluetooth: Arc<AtomicBool>,
    /// The graph runs at the small quantum this game forced (the speaker); false while ROCKNIX's (or a Bluetooth
    /// sink's) runs. The writer sizes its ALSA buffer by it (`audio::Ring`).
    small: Arc<AtomicBool>,
}

impl<S: System> Router<S> {
    /// Looks at the output once and sets the quantum for it. `bluetooth` mirrors `output() == Bluetooth`.
    pub fn start(sys: S, frames: u32, bluetooth: Arc<AtomicBool>, small: Arc<AtomicBool>, now: Instant) -> Router<S> {
        let links = sys.connections();
        // A link at the start may be headphones still connecting: watched for `SETTLE` like a new one.
        let changed = links.as_ref().is_some_and(|l| !l.is_empty()).then_some(now);
        let mut r = Router { sys, frames, output: Output::Speaker, forced: false, kept: false, owner_restores: false, links, changed, bluetooth, small };
        let output = r.ask();
        r.apply(output, true);
        r
    }

    pub fn output(&self) -> Output {
        self.output
    }

    /// Asks for the default sink when it can be Bluetooth.
    fn ask(&self) -> Output {
        if may_be_bluetooth(&self.links) {
            output_of(self.sys.default_sink().as_deref())
        } else {
            Output::Speaker
        }
    }

    /// Sets the quantum for `output`: forces it on the speaker (again, when `again`: after a wake ROCKNIX has put 1024
    /// back), puts ROCKNIX's back when it moved to Bluetooth.
    fn apply(&mut self, output: Output, again: bool) {
        if output != self.output {
            println!("audio: output {output:?}");
        }
        self.output = output;
        self.bluetooth.store(output == Output::Bluetooth, Ordering::Relaxed);
        let wanted = game_quantum(output, self.frames);
        // Before the quantum goes up, so the writer's larger buffer is (nearly) there when the first long cycle comes.
        if wanted.is_none() {
            self.small.store(false, Ordering::Relaxed);
        }
        // Whether the small quantum runs now: after a wake ROCKNIX's 1024 is back whatever `forced` remembers, so a
        // force that fails then must not leave `small` set (audit 2026-10-28).
        let mut runs = self.forced;
        match wanted {
            Some(q) if again || !self.forced => {
                runs = self.sys.force(q);
                self.forced |= runs;
            }
            Some(_) => {}
            None if self.forced => {
                self.sys.restore();
                self.forced = false;
            }
            None => {}
        }
        // After it came down (a failed `force` leaves ROCKNIX's). Only a quantum the short buffer holds counts: 10 ms
        // is 480 frames, so POCKET_CORE_QUANTUM=512 gets the cycle's buffer.
        self.small.store(wanted.is_some_and(|q| q <= SMALL_MAX) && runs, Ordering::Relaxed);
    }

    /// Every `CHECK_EVERY`: follows a Bluetooth sink that came or went.
    pub fn tick(&mut self, now: Instant) {
        if self.kept {
            return;
        }
        let links = self.sys.connections();
        if links != self.links {
            self.links = links;
            self.changed = Some(now);
        }
        if self.changed.is_some_and(|t| now.duration_since(t) < SETTLE) {
            let output = self.ask();
            if output != self.output {
                self.apply(output, false);
            }
        }
    }

    /// After a wake: the links and the sink may have changed, and ROCKNIX's wake hook has put its quantum back.
    pub fn wake(&mut self, now: Instant) {
        if self.kept {
            return;
        }
        let links = self.sys.connections();
        if links != self.links {
            self.links = links;
            self.changed = Some(now);
        }
        let output = self.ask();
        self.apply(output, true);
    }

    /// The next game (fast switching) sets its own quantum and must not have this one put back under it.
    pub fn keep(&mut self) {
        self.kept = true;
    }

    /// The game ends: ROCKNIX's quantum back if this game changed it.
    pub fn stop(&mut self) {
        if self.forced && !self.kept && !self.owner_restores {
            self.sys.restore();
            self.forced = false;
        }
    }
}

enum Cmd {
    Wake,
    Keep,
    Stop,
}

/// How long dropping a keeper waits for it to put the quantum back: a `pw-metadata` in flight and the restore each
/// have `proc::HELPER`; normally it is a few ms.
const STOP_WAIT: Duration = Duration::from_secs(7);

/// The thread that keeps the quantum right for the output while a game plays (see the module). Dropping it puts
/// ROCKNIX's quantum back (unless `keep`) and waits for that.
pub struct Keeper {
    tx: mpsc::Sender<Cmd>,
    done: mpsc::Receiver<()>,
    bluetooth: Arc<AtomicBool>,
    small: Arc<AtomicBool>,
    /// The first look is done (`Router::start` returned): `small` means something.
    decided: Arc<AtomicBool>,
}

impl Keeper {
    /// `frames`: the quantum for the speaker, 0 for none (the output is still followed, for Pixel Path).
    pub fn spawn(frames: u32) -> Keeper {
        Self::spawn_owned(Real, frames, CHECK_EVERY, std::env::var("POCKET_CORE_QUANTUM_OWNER").is_ok_and(|v| v == "pocket-ui"))
    }

    pub fn spawn_with<S: System>(sys: S, frames: u32, every: Duration) -> Keeper {
        Self::spawn_owned(sys, frames, every, false)
    }

    /// `owner_restores`: see `Router::owner_restores`.
    fn spawn_owned<S: System>(sys: S, frames: u32, every: Duration, owner_restores: bool) -> Keeper {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        // Reads "speaker" until the first look is done.
        let bluetooth = Arc::new(AtomicBool::new(false));
        let (small, decided) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (flag, small_flag, decided_flag) = (bluetooth.clone(), small.clone(), decided.clone());
        // If the thread can't start, the channels close: nothing is set, and dropping returns at once.
        let spawned = std::thread::Builder::new().name("audio-route".into()).spawn(move || {
            let mut r = Router::start(sys, frames, flag, small_flag, Instant::now());
            r.owner_restores = owner_restores;
            decided_flag.store(true, Ordering::Relaxed);
            loop {
                match rx.recv_timeout(every) {
                    Ok(Cmd::Wake) => r.wake(Instant::now()),
                    Ok(Cmd::Keep) => r.keep(),
                    Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => r.tick(Instant::now()),
                }
            }
            r.stop();
            let _ = done_tx.send(());
        });
        if spawned.is_err() {
            decided.store(true, Ordering::Relaxed);
        }
        Keeper {
            tx,
            done,
            bluetooth,
            small,
            decided,
        }
    }

    /// The sound goes to Bluetooth (as last seen; false until the first look is done).
    pub fn bluetooth(&self) -> bool {
        self.bluetooth.load(Ordering::Relaxed)
    }

    /// Whether the graph runs at the small quantum this game set (see `Router::small`; false until the first look is
    /// done), for the writer thread: it sizes its ALSA buffer by it (`audio::Ring`).
    pub(crate) fn small_flag(&self) -> Arc<AtomicBool> {
        self.small.clone()
    }

    /// Set once the first look at the output is done and `small_flag` means something.
    pub(crate) fn decided_flag(&self) -> Arc<AtomicBool> {
        self.decided.clone()
    }

    /// After a wake (`Audio::reassert_quantum`).
    pub fn wake(&self) {
        let _ = self.tx.send(Cmd::Wake);
    }

    /// See [`Router::keep`].
    pub fn keep(&self) {
        let _ = self.tx.send(Cmd::Keep);
    }
}

impl Drop for Keeper {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        let _ = self.done.recv_timeout(STOP_WAIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const SPEAKER: &str = "update: id:0 key:'default.audio.sink' value:'{\"name\":\"alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Speaker__sink\"}' type:'Spa:String:JSON'";
    const BT: &str = "update: id:0 key:'default.audio.sink' value:'{\"name\":\"bluez_output.AC_80_0A_12_34_56.1\"}' type:'Spa:String:JSON'";

    #[test]
    fn the_default_sink_is_read_from_pw_metadata() {
        assert_eq!(default_sink_name(SPEAKER), Some("alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Speaker__sink"));
        assert_eq!(default_sink_name(BT), Some("bluez_output.AC_80_0A_12_34_56.1"));
        // pw-metadata's first line, and the user's choice next to the sink in use: the one in use counts.
        let both = format!(
            "Found \"default\" metadata 32\nupdate: id:0 key:'default.configured.audio.sink' value:'{{\"name\":\"bluez_output.X.1\"}}' type:'Spa:String:JSON'\n{SPEAKER}\n"
        );
        assert_eq!(default_sink_name(&both), Some("alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Speaker__sink"));
        assert_eq!(default_sink_name("update: id:0 key:'default.audio.sink' value:'{ \"name\" : \"bluez_output.Y.1\" }' type:'Spa:String:JSON'"), Some("bluez_output.Y.1"));
    }

    #[test]
    fn the_sink_name_says_where_the_sound_goes() {
        let sink = |n: &str| sound_of(Some(n));
        assert_eq!(sink("alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Speaker__sink"), Sound::Speaker);
        assert_eq!(sink("alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Headphones__sink"), Sound::Headphones);
        assert_eq!(sink("bluez_output.AC_80_0A_12_34_56.1"), Sound::Bluetooth(None));
        assert_eq!(sink("alsa_output.usb-Generic_USB_Audio-00.analog-stereo"), Sound::Usb);
        assert_eq!(sink("alsa_output.platform-hdmi.hdmi-stereo"), Sound::Hdmi);
        assert_eq!(sound_of(None), Sound::Unknown, "PipeWire said nothing: the row says so, it doesn't guess");
    }

    #[test]
    fn a_bluetooth_sink_names_its_device_by_address() {
        assert_eq!(sink_address("bluez_output.ac_80_0A_12_34_56.1").as_deref(), Some("AC:80:0A:12:34:56"));
        for odd in ["alsa_output.x", "bluez_output.AC_80_0A.1", "bluez_output.ZZ_80_0A_12_34_56.1", "bluez_output."] {
            assert_eq!(sink_address(odd), None, "{odd}");
        }
        assert_eq!(pactl_default_sink("\nbluez_output.X.1\n"), Some("bluez_output.X.1"));
        assert_eq!(pactl_default_sink(""), None);
    }

    #[test]
    fn odd_or_missing_pw_metadata_output_has_no_sink() {
        for out in [
            "",
            "Found \"default\" metadata 32\n",
            "update: id:0 key:'default.configured.audio.sink' value:'{\"name\":\"bluez_output.X.1\"}' type:'Spa:String:JSON'",
            "update: id:0 key:'default.audio.sink' value:'{}' type:'Spa:String:JSON'",
            "update: id:0 key:'default.audio.sink' value:'{\"name\":\"\"}' type:'Spa:String:JSON'",
            "update: id:0 key:'default.audio.sink' value:'{\"name\":\"bluez_out",
            "error: no metadata named default",
        ] {
            assert_eq!(default_sink_name(out), None, "{out:?}");
            assert_eq!(output_of(default_sink_name(out)), Output::Speaker, "{out:?} plays as the speaker did before");
        }
    }

    #[test]
    fn the_speaker_gets_the_small_quantum_bluetooth_keeps_rocknixs() {
        assert_eq!(output_of(default_sink_name(SPEAKER)), Output::Speaker);
        assert_eq!(output_of(default_sink_name(BT)), Output::Bluetooth);
        assert_eq!(output_of(Some("bluez_input.AA.0")), Output::Speaker, "a Bluetooth microphone is no sink");
        assert_eq!(output_of(None), Output::Speaker);
        assert_eq!(game_quantum(Output::Speaker, 256), Some(256));
        assert_eq!(game_quantum(Output::Bluetooth, 256), None);
        assert_eq!(game_quantum(Output::Speaker, 0), None, "POCKET_CORE_QUANTUM=0 keeps ROCKNIX's");
    }

    #[test]
    fn only_links_count_as_connections() {
        let names = ["hci0", "hci0:3585", "hci1:12", "hci0:", "hcix:1", "rfkill0", "hci0:abc"].map(String::from);
        assert_eq!(connections(names), vec!["hci0:3585".to_string(), "hci1:12".to_string()]);
        assert!(!may_be_bluetooth(&Some(vec![])), "no link: no Bluetooth sink, no pw-metadata");
        assert!(may_be_bluetooth(&Some(vec!["hci0:1".into()])));
        assert!(may_be_bluetooth(&None), "sysfs unreadable: ask PipeWire");
    }

    /// A system whose links and default sink the test sets, and which notes what was done to it.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<FakeState>>);

    #[derive(Default)]
    struct FakeState {
        links: Option<Vec<String>>,
        sink: Option<String>,
        force_fails: bool,
        asked: usize,
        log: Vec<String>,
    }

    impl Fake {
        fn new(links: &[&str], sink: Option<&str>) -> Fake {
            let f = Fake::default();
            f.set(links, sink);
            f
        }
        fn set(&self, links: &[&str], sink: Option<&str>) {
            let mut s = self.0.lock().unwrap();
            s.links = Some(links.iter().map(|l| l.to_string()).collect());
            s.sink = sink.map(String::from);
        }
        fn log(&self) -> Vec<String> {
            std::mem::take(&mut self.0.lock().unwrap().log)
        }
        fn asked(&self) -> usize {
            self.0.lock().unwrap().asked
        }
    }

    impl System for Fake {
        fn connections(&self) -> Option<Vec<String>> {
            self.0.lock().unwrap().links.clone()
        }
        fn default_sink(&self) -> Option<String> {
            let mut s = self.0.lock().unwrap();
            s.asked += 1;
            s.sink.clone()
        }
        fn force(&self, frames: u32) -> bool {
            let mut s = self.0.lock().unwrap();
            s.log.push(format!("force {frames}"));
            !s.force_fails
        }
        fn restore(&self) {
            self.0.lock().unwrap().log.push("restore".into());
        }
    }

    const BT_SINK: Option<&str> = Some("bluez_output.AA.1");
    const SPK_SINK: Option<&str> = Some("alsa_output.speaker");

    #[test]
    fn on_the_speaker_a_game_sets_256_and_puts_rocknixs_back_at_the_end() {
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), Instant::now());
        assert_eq!((r.output(), f.log(), f.asked()), (Output::Speaker, vec!["force 256".to_string()], 0), "no link: PipeWire isn't asked");
        r.stop();
        assert_eq!(f.log(), ["restore"]);
    }

    #[test]
    fn on_bluetooth_a_game_leaves_the_quantum_alone() {
        let f = Fake::new(&["hci0:12"], BT_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), Instant::now());
        assert_eq!(r.output(), Output::Bluetooth);
        assert!(r.bluetooth.load(Ordering::Relaxed));
        r.stop();
        assert!(f.log().is_empty(), "nothing set, nothing put back");
    }

    #[test]
    fn a_controller_link_is_still_the_speaker() {
        let f = Fake::new(&["hci0:12"], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), Instant::now());
        assert_eq!((r.output(), f.log()), (Output::Speaker, vec!["force 256".to_string()]));
        r.stop();
        assert_eq!(f.log(), ["restore"]);
    }

    #[test]
    fn missing_data_behaves_as_before_the_speaker() {
        // sysfs unreadable and pw-metadata failing (None): the speaker, 256, put back at the end.
        let f = Fake::default();
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), Instant::now());
        assert_eq!((r.output(), f.log(), f.asked()), (Output::Speaker, vec!["force 256".to_string()], 1));
        r.stop();
        assert_eq!(f.log(), ["restore"]);
        // pw-metadata failing to set the quantum: nothing to put back, as before.
        let f = Fake::new(&[], SPK_SINK);
        f.0.lock().unwrap().force_fails = true;
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), Instant::now());
        r.stop();
        assert_eq!(f.log(), ["force 256"]);
    }

    /// The writer sizes its ALSA buffer by `small` (`audio::Ring`): a buffer shorter than one graph cycle is what made
    /// games on Bluetooth headphones sound broken (2026-10-27). Small only while this game's 256 really runs.
    /// pocket-ui owns the quantum for the games it starts (`POCKET_CORE_QUANTUM_OWNER`): pocket-core's end leaves it, so
    /// the restore comes from pocket-ui's quantum thread, in order and not under a sound.
    #[test]
    fn a_game_pocket_ui_started_leaves_the_restore_to_it() {
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), Instant::now());
        r.owner_restores = true;
        r.stop();
        assert_eq!(f.log(), ["force 256"], "set, not put back");
    }

    /// Audit 2026-10-28: after a wake `forced` was still true while ROCKNIX had put 1024 back; a force that failed
    /// then left `small` set and pocket-core's writer on 10 ms under 1024 cycles. And a quantum above 256 never
    /// gets the short buffer.
    #[test]
    fn small_is_cleared_when_the_force_after_a_wake_fails_and_never_set_above_256() {
        let t0 = Instant::now();
        let small = Arc::new(AtomicBool::new(false));
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), small.clone(), t0);
        assert!(small.load(Ordering::Relaxed));
        f.0.lock().unwrap().force_fails = true;
        r.wake(t0 + CHECK_EVERY);
        assert!(!small.load(Ordering::Relaxed), "ROCKNIX's 1024 runs after the wake");
        let small = Arc::new(AtomicBool::new(true));
        let _r = Router::start(Fake::new(&[], SPK_SINK), 512, Arc::default(), small.clone(), t0);
        assert!(!small.load(Ordering::Relaxed), "512 is more than 10 ms holds");
    }

    #[test]
    fn small_follows_the_quantum_that_really_runs() {
        let t0 = Instant::now();
        let small = Arc::new(AtomicBool::new(false));
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), small.clone(), t0);
        assert!(small.load(Ordering::Relaxed), "speaker: 256 set");
        f.set(&["hci0:12"], BT_SINK);
        r.tick(t0 + CHECK_EVERY);
        assert!(!small.load(Ordering::Relaxed), "headphones: ROCKNIX's 1024 again");
        f.set(&[], SPK_SINK);
        r.tick(t0 + CHECK_EVERY * 2);
        assert!(small.load(Ordering::Relaxed), "back on the speaker: 256");
        // Starting on Bluetooth, a failed force, and the quantum left alone (POCKET_CORE_QUANTUM=0, the pulse path).
        let small = Arc::new(AtomicBool::new(true));
        let _r = Router::start(Fake::new(&["hci0:12"], BT_SINK), 256, Arc::default(), small.clone(), t0);
        assert!(!small.load(Ordering::Relaxed));
        let f = Fake::new(&[], SPK_SINK);
        f.0.lock().unwrap().force_fails = true;
        let small = Arc::new(AtomicBool::new(true));
        let _r = Router::start(f, 256, Arc::default(), small.clone(), t0);
        assert!(!small.load(Ordering::Relaxed), "pw-metadata failed: ROCKNIX's quantum runs");
        let small = Arc::new(AtomicBool::new(true));
        let _r = Router::start(Fake::new(&[], SPK_SINK), 0, Arc::default(), small.clone(), t0);
        assert!(!small.load(Ordering::Relaxed));
    }

    #[test]
    fn headphones_connecting_mid_game_get_rocknixs_quantum_and_leaving_gets_256_again() {
        let t0 = Instant::now();
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), t0);
        f.log();
        r.tick(t0 + CHECK_EVERY);
        assert_eq!(f.asked(), 0, "nothing connected: PipeWire isn't asked");
        // The link comes; the sink becomes the default a few seconds later.
        f.set(&["hci0:12"], SPK_SINK);
        r.tick(t0 + CHECK_EVERY * 2);
        assert_eq!((r.output(), f.asked()), (Output::Speaker, 1));
        f.set(&["hci0:12"], BT_SINK);
        r.tick(t0 + CHECK_EVERY * 4);
        assert_eq!((r.output(), f.log()), (Output::Bluetooth, vec!["restore".to_string()]));
        // Settled: no more asking while nothing changes.
        let asked = f.asked();
        r.tick(t0 + CHECK_EVERY * 2 + SETTLE + Duration::from_secs(1));
        assert_eq!(f.asked(), asked);
        // Switched off: back to the speaker, 256 again; the end puts ROCKNIX's back.
        f.set(&[], SPK_SINK);
        r.tick(t0 + SETTLE * 3);
        assert_eq!((r.output(), f.log()), (Output::Speaker, vec!["force 256".to_string()]));
        r.stop();
        assert_eq!(f.log(), ["restore"]);
    }

    #[test]
    fn a_wake_sets_256_again_on_the_speaker_only() {
        let t0 = Instant::now();
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), t0);
        f.log();
        r.wake(t0 + Duration::from_secs(60));
        assert_eq!(f.log(), ["force 256"], "ROCKNIX's wake hook put 1024 back");
        // Headphones connected while asleep: ROCKNIX's stays, and the end has nothing to put back.
        f.set(&["hci0:7"], BT_SINK);
        r.wake(t0 + Duration::from_secs(120));
        assert_eq!((r.output(), f.log()), (Output::Bluetooth, vec!["restore".to_string()]));
        r.wake(t0 + Duration::from_secs(180));
        r.stop();
        assert!(f.log().is_empty());
    }

    #[test]
    fn a_kept_quantum_is_neither_changed_nor_put_back() {
        let t0 = Instant::now();
        let f = Fake::new(&[], SPK_SINK);
        let mut r = Router::start(f.clone(), 256, Arc::default(), Arc::default(), t0);
        f.log();
        r.keep();
        f.set(&["hci0:1"], BT_SINK);
        r.tick(t0 + CHECK_EVERY);
        r.wake(t0 + CHECK_EVERY * 2);
        r.stop();
        assert!(f.log().is_empty(), "the next game owns the quantum");
    }

    #[test]
    fn no_configured_quantum_never_touches_it_but_still_follows_the_output() {
        let f = Fake::new(&["hci0:1"], BT_SINK);
        let mut r = Router::start(f.clone(), 0, Arc::default(), Arc::default(), Instant::now());
        assert_eq!(r.output(), Output::Bluetooth);
        f.set(&[], SPK_SINK);
        r.wake(Instant::now());
        r.stop();
        assert_eq!((r.output(), f.log()), (Output::Speaker, Vec::<String>::new()));
    }

    #[test]
    fn the_keeper_thread_sets_and_puts_back_in_order() {
        let f = Fake::new(&[], SPK_SINK);
        // Dropped at once: the restore still comes after the force, and the drop waits for it.
        drop(Keeper::spawn_with(f.clone(), 256, Duration::from_millis(5)));
        assert_eq!(f.log(), ["force 256", "restore"]);
        let k = Keeper::spawn_with(f.clone(), 256, Duration::from_millis(5));
        k.keep();
        drop(k);
        assert_eq!(f.log(), ["force 256"], "kept for the next game");
    }
}
