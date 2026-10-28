//! pocket-splash: the boot screen from systemd's early start until sway takes the screen (docs/BOOT-SPLASH.md).
//! Started by `pocket-splash.service` (deploy/), linked from sysinit.target.wants in /storage/.config/system.d.
//!
//!   pocket-splash              draws on /dev/fb0 until sway starts or SIGTERM/SIGINT, then fades to black and ends
//!   pocket-splash --version
//!   pocket-splash --snapshot OUT.png [--size WxH] [--at SECONDS] [--stop-at SECONDS] [--first-start]
//!                [--motion full|reduced|off]
//!                            one frame as a PNG, without a screen (the size is the picture's, landscape)
//!
//! It draws through the frame buffer (`/dev/fb0`, the kernel's emulation on top of the msm DRM driver), never through
//! `/dev/dri`: opening the DRM device first would make this program DRM master and sway could not start. The frame
//! buffer needs no master, so nothing here can hold up sway. Its layout comes from the frame buffer itself (size, row
//! length, pixel format), the quarter turn from `/sys/class/graphics/fbcon/rotate`. The console is put into graphics
//! mode, so no kernel or systemd line is drawn over the picture, and left there at the end (sway takes it over).
//! Anything that goes wrong ends it without a picture: the boot goes on as without it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pocket_ui::settings::Motion;
use pocket_ui::splash::{self, Scene, Timeline};

const DATA_DIR: &str = "/storage/.config/pocket-ui";

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| DATA_DIR.into()))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str);
    if args.iter().any(|a| a == "--version") {
        println!("pocket-splash {}", pocket_ui::update::app_version());
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("pocket-splash [--version | --snapshot OUT.png [--size WxH] [--at S] [--stop-at S] [--first-start] [--motion full|reduced|off]]");
        return;
    }
    let dir = data_dir();
    pocket_ui::i18n::set(pocket_ui::settings::language(&dir));
    if let Some(out) = arg("--snapshot") {
        let (w, h) = arg("--size").and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).unwrap_or((1240, 1080));
        let secs = |name| arg(name).and_then(|s| s.parse::<f32>().ok()).map(Duration::from_secs_f32);
        let motion = match arg("--motion") {
            Some("off") => Motion::Off,
            Some("reduced") => Motion::Reduced,
            _ => Motion::Full,
        };
        let mut timeline = Timeline::new(motion, args.iter().any(|a| a == "--first-start"));
        if let Some(s) = secs("--stop-at") {
            timeline.stop(s);
        }
        let look = timeline.look(secs("--at").unwrap_or(Duration::from_secs(2)));
        let mut buf = vec![0u32; (w * h) as usize];
        Scene::new(w, h).paint(&mut pocket_ui::gfx::Canvas::new(&mut buf, w, h), &look);
        let rgb: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        let png = pocket_ui::screenshot::encode_rgb(w, h, &rgb).expect("encode");
        std::fs::write(out, png).expect("write the snapshot");
        println!("wrote {out}");
        return;
    }
    #[cfg(target_os = "linux")]
    std::process::exit(fb::show(&dir));
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("pocket-splash: draws on a Linux frame buffer only (--snapshot works here)");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
mod fb {
    use super::*;
    use pocket_ui::splash::{Clock, Ended, Format, Screen, Turn, WaitEnded, Watch};
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{FileExt, OpenOptionsExt};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    const FB: &str = "/dev/fb0";
    const TTY: &str = "/dev/tty0";
    const FBCON_ROTATE: &str = "/sys/class/graphics/fbcon/rotate";
    /// ROCKNIX's mark of a card that has been set up (its initramfs says "Initializing…" without it).
    const CONFIGURED: &str = "/storage/.configured";
    /// Waited for the frame buffer this long, then no picture. It is not there when systemd starts this
    /// program: on the Mini V2 the msm driver binds about a second later (the first device boot measured
    /// the program starting at 2.47 s and the display driver at 3.50 s), and a driver having a bad day
    /// gets the rest. The unit is Type=simple, so the wait holds nothing up.
    const WAIT_FOR_FB: Duration = Duration::from_secs(20);
    /// How often the frame buffer is looked for while waiting: often enough that the text the console
    /// buffered before it (the version lines) is covered in the same blink it could first be shown.
    const FB_POLL: Duration = Duration::from_millis(25);

    // linux/fb.h and linux/kd.h
    const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
    const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
    const FBIOPAN_DISPLAY: libc::c_ulong = 0x4606;
    const FB_VISUAL_TRUECOLOR: u32 = 2;
    const KDSETMODE: libc::c_ulong = 0x4B3A;
    const KDGETMODE: libc::c_ulong = 0x4B3B;
    const KD_TEXT: libc::c_int = 0;
    const KD_GRAPHICS: libc::c_int = 1;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Bitfield {
        offset: u32,
        length: u32,
        msb_right: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct VarScreeninfo {
        xres: u32,
        yres: u32,
        xres_virtual: u32,
        yres_virtual: u32,
        xoffset: u32,
        yoffset: u32,
        bits_per_pixel: u32,
        grayscale: u32,
        red: Bitfield,
        green: Bitfield,
        blue: Bitfield,
        transp: Bitfield,
        nonstd: u32,
        activate: u32,
        height: u32,
        width: u32,
        accel_flags: u32,
        pixclock: u32,
        left_margin: u32,
        right_margin: u32,
        upper_margin: u32,
        lower_margin: u32,
        hsync_len: u32,
        vsync_len: u32,
        sync: u32,
        vmode: u32,
        rotate: u32,
        colorspace: u32,
        reserved: [u32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FixScreeninfo {
        id: [u8; 16],
        smem_start: libc::c_ulong,
        smem_len: u32,
        type_: u32,
        type_aux: u32,
        visual: u32,
        xpanstep: u16,
        ypanstep: u16,
        ywrapstep: u16,
        line_length: u32,
        mmio_start: libc::c_ulong,
        mmio_len: u32,
        accel: u32,
        capabilities: u16,
        reserved: [u16; 2],
    }

    static STOP: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_signal(_: libc::c_int) {
        STOP.store(true, Ordering::SeqCst);
    }

    /// SIGTERM and SIGINT set `STOP`, without SA_RESTART: a sleep ends at once.
    fn catch_signals() {
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_signal as *const () as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            for sig in [libc::SIGTERM, libc::SIGINT] {
                libc::sigaction(sig, &sa, std::ptr::null_mut());
            }
        }
    }

    struct Fb {
        file: File,
        var: VarScreeninfo,
        /// Where the visible picture starts in the device, and how far it may be written.
        base: u64,
        limit: u64,
        /// The pan check (`taken`) is skipped from the first answer that isn't "busy" or "done".
        can_pan: bool,
        checks: u32,
    }

    impl Screen for Fb {
        fn write_rows(&mut self, frame: &[u8], y0: usize, y1: usize) -> std::io::Result<()> {
            let line = frame.len() / self.var.yres as usize;
            let (from, to) = (y0 * line, (y1 * line).min(frame.len()));
            let at = self.base + from as u64;
            let n = (to - from).min(self.limit.saturating_sub(at) as usize);
            self.file.write_all_at(&frame[from..from + n], at)
        }

        /// The frame buffer refuses to pan (EBUSY) while another program is DRM master: sway shows its own picture
        /// and nothing written here is seen any more. Asked once a second (each pan is a commit of the same picture).
        fn taken(&mut self) -> bool {
            self.checks += 1;
            if !self.can_pan || self.checks % 5 != 1 {
                return false;
            }
            let mut var = self.var;
            let r = unsafe { libc::ioctl(self.file.as_raw_fd(), FBIOPAN_DISPLAY, &mut var) };
            if r == 0 {
                return false;
            }
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EBUSY) {
                return true;
            }
            eprintln!("pocket-splash: the frame buffer cannot pan ({e}); not checked for sway's take-over");
            self.can_pan = false;
            false
        }
    }

    struct RealClock(Instant);

    impl Clock for RealClock {
        fn now(&self) -> Duration {
            self.0.elapsed()
        }
        fn sleep(&mut self, d: Duration) {
            // One nanosleep, not std's, which sleeps on after a signal: SIGTERM is seen at once.
            let ts = libc::timespec { tv_sec: d.as_secs() as libc::time_t, tv_nsec: d.subsec_nanos() as libc::c_long };
            unsafe { libc::nanosleep(&ts, std::ptr::null_mut()) };
        }
    }

    struct RealWatch;

    impl Watch for RealWatch {
        fn told_to_stop(&mut self) -> bool {
            STOP.load(Ordering::SeqCst)
        }
        fn sway_starting(&mut self) -> bool {
            splash::sway_starting(Path::new("/proc"))
        }
    }

    /// The console in graphics mode while the picture shows; back to text only when the picture could not be shown.
    struct Console {
        file: Option<File>,
        was: libc::c_int,
    }

    impl Console {
        fn graphics() -> Console {
            let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_NOCTTY | libc::O_CLOEXEC).open(TTY).ok();
            let mut was = KD_TEXT;
            if let Some(f) = &file {
                unsafe {
                    libc::ioctl(f.as_raw_fd(), KDGETMODE, &mut was);
                    libc::ioctl(f.as_raw_fd(), KDSETMODE, KD_GRAPHICS);
                }
            }
            Console { file, was }
        }

        fn back_to_text(&self) {
            if let (Some(f), KD_TEXT) = (&self.file, self.was) {
                unsafe { libc::ioctl(f.as_raw_fd(), KDSETMODE, KD_TEXT) };
            }
        }
    }

    fn open_fb() -> Result<(Fb, Format), String> {
        let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_CLOEXEC).open(FB).map_err(|e| format!("{FB}: {e}"))?;
        let fd = file.as_raw_fd();
        let mut var = VarScreeninfo::default();
        let mut fix = FixScreeninfo::default();
        if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut var) } != 0 || unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut fix) } != 0 {
            return Err(format!("{FB}: cannot read its layout ({})", std::io::Error::last_os_error()));
        }
        if fix.visual != FB_VISUAL_TRUECOLOR {
            return Err(format!("{FB}: not a true colour frame buffer (visual {})", fix.visual));
        }
        let bytes = var.bits_per_pixel.div_ceil(8);
        let base = var.yoffset as u64 * fix.line_length as u64 + var.xoffset as u64 * bytes as u64;
        let limit = if fix.smem_len > 0 { fix.smem_len as u64 } else { u64::MAX };
        let fmt = Format {
            xres: var.xres,
            yres: var.yres,
            line_length: fix.line_length,
            bytes,
            red: (var.red.offset, var.red.length),
            green: (var.green.offset, var.green.length),
            blue: (var.blue.offset, var.blue.length),
        };
        Ok((Fb { file, var, base, limit, can_pan: true, checks: 0 }, fmt))
    }

    /// The whole run; the exit code (0 also when there was nothing to show).
    pub fn show(dir: &Path) -> i32 {
        let started = Instant::now();
        catch_signals();
        let mut watch = RealWatch;
        if watch.sway_starting() {
            println!("pocket-splash: sway is starting or running already; nothing to show");
            return 0;
        }
        // The console goes to graphics mode before the wait, not with the first frame: everything written
        // to it so far (ROCKNIX's version lines from its initramfs, systemd's first lines) sits in its
        // buffer and would be painted the moment the frame buffer console binds — before any frame of
        // ours could cover it. In graphics mode that text is never painted. Every way out below that
        // shows no picture puts the text mode back first, so a boot that ends on the console still shows it.
        let console = Console::graphics();
        let mut wait_clock = RealClock(started);
        let (mut fb, fmt) = match splash::wait_for_fb(WAIT_FOR_FB, FB_POLL, &mut wait_clock, &mut watch, &mut open_fb) {
            Ok(found) => found,
            Err(WaitEnded::TimedOut(e)) => {
                console.back_to_text();
                eprintln!("pocket-splash: no picture: {e} (still missing after {} s)", WAIT_FOR_FB.as_secs());
                return 0;
            }
            Err(WaitEnded::Sway) => {
                console.back_to_text();
                println!("pocket-splash: sway is starting; nothing to show");
                return 0;
            }
            Err(WaitEnded::Stopped) => {
                console.back_to_text();
                println!("pocket-splash: told to stop while waiting for the frame buffer; nothing to show");
                return 0;
            }
        };
        if let Err(e) = fmt.usable() {
            console.back_to_text();
            eprintln!("pocket-splash: no picture: {e}");
            return 0;
        }
        let turn = Turn::from_fbcon(&std::fs::read_to_string(FBCON_ROTATE).unwrap_or_default());
        let (w, h) = turn.picture_size(fmt.xres, fmt.yres);
        let motion = splash::motion(dir);
        let first_start = !Path::new(CONFIGURED).exists();
        let mut scene = Scene::new(w, h);
        println!(
            "pocket-splash: {}x{} frame buffer ({} bytes a row, {} bit), turned {turn:?}, picture {w}x{h}, motion {motion:?}{}",
            fmt.xres,
            fmt.yres,
            fmt.line_length,
            fb.var.bits_per_pixel,
            if first_start { ", first start" } else { "" }
        );
        // The picture's time starts now, not at the program's start: the frame buffer may have kept it waiting.
        let mut clock = RealClock(Instant::now());
        let report = splash::run(&mut scene, &mut fb, &fmt, turn, Timeline::new(motion, first_start), &mut clock, &mut watch);
        let why = match &report.ended {
            Ended::Stopped => "told to stop".to_string(),
            Ended::Sway => "sway is starting".to_string(),
            Ended::Taken => "sway has the screen".to_string(),
            Ended::Failed(e) => {
                console.back_to_text();
                format!("cannot write to {FB}: {e}")
            }
        };
        println!(
            "pocket-splash: ended after {:.1} s ({why}): {} frames, {:.1} MB written, {:.2} s of CPU",
            started.elapsed().as_secs_f32(),
            report.frames,
            report.bytes as f32 / 1e6,
            cpu_seconds()
        );
        0
    }

    /// This process's CPU time (user and system), for the end line.
    fn cpu_seconds() -> f32 {
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
        let t = |tv: libc::timeval| tv.tv_sec as f32 + tv.tv_usec as f32 / 1e6;
        t(ru.ru_utime) + t(ru.ru_stime)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The kernel's structs as linux/fb.h lays them out (160 and 80 bytes on 64-bit).
        #[test]
        fn the_ioctl_structs_have_the_kernels_size() {
            assert_eq!(std::mem::size_of::<VarScreeninfo>(), 160);
            #[cfg(target_pointer_width = "64")]
            {
                assert_eq!(std::mem::size_of::<FixScreeninfo>(), 80);
                assert_eq!(std::mem::offset_of!(FixScreeninfo, line_length), 48);
                assert_eq!(std::mem::offset_of!(VarScreeninfo, red), 32);
                assert_eq!(std::mem::offset_of!(VarScreeninfo, rotate), 136);
            }
        }
    }
}
