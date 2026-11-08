//! pocket-fb: the smallest "compositor" for a screen that is only a framebuffer (the TrimUI Brick under Knulli,
//! docs/TRIMUI-BRICK.md). pocket-ui and pocket-core stay what they are on ROCKNIX, Wayland clients that talk to sway; this
//! answers both in sway's place and copies the focused window's wl_shm buffer to /dev/fb0.
//!
//!   pocket-fb --wayland <socket> --ipc <socket> [--fb /dev/fb0] [--mode 1024x768x32] [--no-vsync] [--bgr] [--gl] [--gl-window null]
//!   pocket-fb --picture <file.png> [--hold 5] [--fb /dev/fb0] [--mode ...] [--bgr]   one picture, nothing else (the first
//!   test of a screen: format, colours, copy time), then the screen as it was
//!
//! `--mode` is for a plain file instead of a framebuffer device (the host test). One window shows at a time: the last one
//! mapped or focused over the IPC socket. Windows out of sight keep their last buffer and get no frame callbacks, as under
//! sway's fullscreen. There is no seat: pocket-ui and pocket-core read their buttons from evdev themselves.
//! `--gl`: the focused window's picture goes through the GPU and a real page flip (`brick_gl`, docs/TRIMUI-BRICK.md "The GPU") instead of
//! the copy. If that can't start, or the visible page doesn't hold what was drawn, or a swap fails later: a line in the log and the copy.
//! Only behind the `brick` feature; no handheld of the ROCKNIX image builds this.

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pocket_ui::brick_gl::{Look, Presenter};
use pocket_ui::brick_picture;
use pocket_ui::motion::Transition;

const GLOBALS: [(u32, &str, u32); 3] = [(1, "wl_compositor", 4), (2, "wl_shm", 1), (3, "xdg_wm_base", 2)];
const WL_SHM_ARGB8888: u32 = 0;
const WL_SHM_XRGB8888: u32 = 1;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

// ---- The framebuffer ----

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

/// Linux's `fb_var_screeninfo`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct VarInfo {
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
    rest: [u32; 20],
}

/// The start of Linux's `fb_fix_screeninfo` up to `line_length`, plus what follows.
#[repr(C)]
#[derive(Clone, Copy)]
struct FixInfo {
    id: [u8; 16],
    smem_start: u64,
    smem_len: u32,
    kind: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    rest: [u8; 32],
}

const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
const FBIOBLANK: libc::c_ulong = 0x4611;
const FBIOPAN_DISPLAY: libc::c_ulong = 0x4606;
const FBIO_WAITFORVSYNC: libc::c_ulong = 0x4004_4620;
const FB_BLANK_UNBLANK: libc::c_ulong = 0;
const FB_BLANK_POWERDOWN: libc::c_ulong = 4;

/// How a pixel lies in the framebuffer. Alpha is set to opaque: the display engine blends fb0's alpha (Knulli's
/// `setalpha` at boot is about that), and the clients leave wl_shm's X byte at whatever it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// 32 bits; R, G, B at these bit offsets and the alpha mask.
    Bits32 { r: u32, g: u32, b: u32, alpha: u32 },
    /// RGB565.
    Rgb565,
}

impl Format {
    fn from_var(v: &VarInfo) -> Option<Format> {
        match v.bits_per_pixel {
            32 => Some(Format::Bits32 {
                r: v.red.offset,
                g: v.green.offset,
                b: v.blue.offset,
                alpha: if v.transp.length > 0 { ((1u64 << v.transp.length) as u32).wrapping_sub(1) << v.transp.offset } else { 0 },
            }),
            16 => Some(Format::Rgb565),
            _ => None,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Format::Bits32 { .. } => 4,
            Format::Rgb565 => 2,
        }
    }
}

/// One row from XRGB8888 (`src`, native-endian u32 as bytes) to the framebuffer's format. No allocation, on the frame path.
pub fn convert_row(fmt: Format, src: &[u8], dst: &mut [u8]) {
    match fmt {
        // The usual layout: only alpha to set, which the compiler turns into NEON.
        Format::Bits32 { r: 16, g: 8, b: 0, alpha } => {
            for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                let v = u32::from_le_bytes([s[0], s[1], s[2], s[3]]) | alpha;
                d.copy_from_slice(&v.to_le_bytes());
            }
        }
        Format::Bits32 { r, g, b, alpha } => {
            for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                let v = (u32::from(s[2]) << r) | (u32::from(s[1]) << g) | (u32::from(s[0]) << b) | alpha;
                d.copy_from_slice(&v.to_le_bytes());
            }
        }
        Format::Rgb565 => {
            for (d, s) in dst.chunks_exact_mut(2).zip(src.chunks_exact(4)) {
                let v = ((u16::from(s[2]) & 0xf8) << 8) | ((u16::from(s[1]) & 0xfc) << 3) | (u16::from(s[0]) >> 3);
                d.copy_from_slice(&v.to_le_bytes());
            }
        }
    }
}

struct Fb {
    file: std::fs::File,
    map: *mut u8,
    map_len: usize,
    width: u32,
    height: u32,
    stride: usize,
    /// Where the visible page starts: a pan left by whoever had the screen before us stays as it is.
    visible: usize,
    format: Format,
    vsync: bool,
    /// The screen as we found it, put back on exit.
    saved: Vec<u8>,
    blanked: bool,
    /// A plain file instead of a framebuffer device (`--mode`).
    plain_file: bool,
    /// The kernel's screen as we found it: the page that showed goes back on show at the end (the GPU's flips move it).
    found: Option<VarInfo>,
}

impl Fb {
    fn open(path: &str, mode: Option<(u32, u32, u32)>, vsync: bool, bgr: bool) -> Result<Fb, String> {
        let file = std::fs::OpenOptions::new().read(true).write(true).open(path).map_err(|e| format!("{path}: {e}"))?;
        let fd = file.as_raw_fd();
        let (width, height, stride, visible, format, map_len, vsync) = match mode {
            Some((w, h, bits)) => {
                let format = if bits == 16 { Format::Rgb565 } else { Format::Bits32 { r: 16, g: 8, b: 0, alpha: 0xff00_0000 } };
                let stride = w as usize * format.bytes();
                (w, h, stride, 0, format, stride * h as usize, false)
            }
            None => {
                let mut var = VarInfo::default();
                // SAFETY: both ioctls only write into the struct given, which is at least as large as the kernel's.
                let mut fix = FixInfo { id: [0; 16], smem_start: 0, smem_len: 0, kind: 0, type_aux: 0, visual: 0, xpanstep: 0, ypanstep: 0, ywrapstep: 0, line_length: 0, rest: [0; 32] };
                if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut var as *mut VarInfo) } != 0 {
                    return Err(format!("{path}: FBIOGET_VSCREENINFO: {}", std::io::Error::last_os_error()));
                }
                if unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO as _, &mut fix as *mut FixInfo) } != 0 {
                    return Err(format!("{path}: FBIOGET_FSCREENINFO: {}", std::io::Error::last_os_error()));
                }
                let format = Format::from_var(&var).ok_or_else(|| format!("{path}: {} bits per pixel", var.bits_per_pixel))?;
                let stride = fix.line_length as usize;
                eprintln!(
                    "pocket-fb: {}x{} virtual {}x{} offset {} {} bits r{} g{} b{} a{}/{} line {} mem {}",
                    var.xres, var.yres, var.xres_virtual, var.yres_virtual, var.yoffset, var.bits_per_pixel, var.red.offset, var.green.offset, var.blue.offset, var.transp.offset, var.transp.length, stride, fix.smem_len
                );
                (var.xres, var.yres, stride, var.yoffset as usize * stride + var.xoffset as usize * format.bytes(), format, fix.smem_len as usize, vsync)
            }
        };
        let format = match (bgr, format) {
            (true, Format::Bits32 { r, g, b, alpha }) => Format::Bits32 { r: b, g, b: r, alpha },
            (_, f) => f,
        };
        let need = visible + stride * height as usize;
        if map_len < need {
            return Err(format!("{path}: {map_len} bytes, the visible page needs {need}"));
        }
        // SAFETY: a shared mapping of the whole framebuffer; `map` is only used within `map_len`, unmapped in Drop.
        let map = unsafe { libc::mmap(std::ptr::null_mut(), map_len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0) };
        if map == libc::MAP_FAILED {
            return Err(format!("{path}: mmap: {}", std::io::Error::last_os_error()));
        }
        let map = map.cast::<u8>();
        // SAFETY: `need <= map_len`, checked above.
        let saved = unsafe { std::slice::from_raw_parts(map.add(visible), need - visible) }.to_vec();
        let mut found = VarInfo::default();
        // SAFETY: the ioctl only writes into the struct given. A plain file has none.
        let found = (mode.is_none() && unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut found as *mut VarInfo) } == 0).then_some(found);
        Ok(Fb { file, map, map_len, width, height, stride, visible, format, vsync, saved, blanked: false, plain_file: mode.is_some(), found })
    }

    /// The page that showed when we came shows again (after the GPU, whose flips leave the pan where the last one was).
    fn restore_pan(&mut self) {
        if let Some(var) = self.found.as_mut() {
            // SAFETY: FBIOPAN_DISPLAY reads the struct given, which is the kernel's own from FBIOGET_VSCREENINFO.
            if unsafe { libc::ioctl(self.file.as_raw_fd(), FBIOPAN_DISPLAY as _, var as *mut VarInfo) } != 0 {
                eprintln!("pocket-fb: the page we found ({}) couldn't be put back: {}", var.yoffset, std::io::Error::last_os_error());
            }
        }
        self.follow_pan();
    }

    /// Where the visible page is now: the GPU's flips move it.
    fn follow_pan(&mut self) {
        if self.plain_file {
            return;
        }
        let mut var = VarInfo::default();
        // SAFETY: the ioctl only writes into the struct given.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), FBIOGET_VSCREENINFO as _, &mut var as *mut VarInfo) } != 0 {
            return;
        }
        let visible = var.yoffset as usize * self.stride + var.xoffset as usize * self.format.bytes();
        if visible + self.stride * self.height as usize <= self.map_len {
            self.visible = visible;
        }
    }

    /// The pixel in the middle of the visible page as (r, g, b); None for a format this doesn't read.
    fn middle_pixel(&mut self) -> Option<(u8, u8, u8)> {
        self.follow_pan();
        let Format::Bits32 { r, g, b, .. } = self.format else { return None };
        let at = self.visible + self.stride * (self.height as usize / 2) + self.width as usize * 2;
        // SAFETY: inside the visible page, which `follow_pan` and `open` keep inside the mapping. Volatile: the GPU writes there.
        let v = unsafe { self.map.add(at).cast::<u32>().read_volatile() };
        Some(((v >> r) as u8, (v >> g) as u8, (v >> b) as u8))
    }

    fn wait_vsync(&mut self) {
        if !self.vsync {
            return;
        }
        let mut crtc: u32 = 0;
        // SAFETY: the ioctl reads one u32.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), FBIO_WAITFORVSYNC as _, &mut crtc as *mut u32) } != 0 {
            eprintln!("pocket-fb: no vsync from this framebuffer: {}", std::io::Error::last_os_error());
            self.vsync = false;
        }
    }

    /// A client's XRGB8888 picture, top left, to the visible page. What is larger than the screen is cut; `height` rows.
    fn show(&mut self, pixels: &[u8], width: u32, height: u32, stride: usize) {
        if self.blanked {
            return;
        }
        self.wait_vsync();
        let rows = height.min(self.height) as usize;
        let columns = width.min(self.width) as usize;
        for y in 0..rows {
            let Some(src) = pixels.get(y * stride..y * stride + columns * 4) else { return };
            // SAFETY: `visible + stride * height <= map_len` (checked in `open`) and `columns <= self.width`.
            let dst = unsafe { std::slice::from_raw_parts_mut(self.map.add(self.visible + y * self.stride), columns * self.format.bytes()) };
            convert_row(self.format, src, dst);
        }
    }

    fn blank(&mut self, off: bool) {
        self.blanked = off;
        let level = if off { FB_BLANK_POWERDOWN } else { FB_BLANK_UNBLANK };
        // SAFETY: FBIOBLANK takes the level by value. A file instead of a device answers with an error, which is fine.
        let _ = unsafe { libc::ioctl(self.file.as_raw_fd(), FBIOBLANK as _, level) };
    }
}

impl Drop for Fb {
    fn drop(&mut self) {
        if self.blanked {
            self.blank(false);
        }
        // SAFETY: `saved` was read from this very range; the mapping lives until the munmap below.
        unsafe {
            std::ptr::copy_nonoverlapping(self.saved.as_ptr(), self.map.add(self.visible), self.saved.len());
            libc::munmap(self.map.cast(), self.map_len);
        }
    }
}

// ---- The Wayland side ----

#[derive(Clone)]
enum Obj {
    Display,
    Registry,
    Compositor,
    Shm,
    Pool { pool: usize },
    Buffer(Buffer),
    Surface(u32),
    Region,
    Callback,
    WmBase,
    Positioner,
    XdgSurface { surface: u32 },
    Toplevel { surface: u32 },
}

#[derive(Clone, Copy)]
struct Buffer {
    pool: usize,
    offset: usize,
    width: u32,
    height: u32,
    stride: usize,
}

struct Pool {
    fd: OwnedFd,
    map: *mut u8,
    len: usize,
}

impl Pool {
    fn map(fd: &OwnedFd, len: usize) -> Option<*mut u8> {
        // SAFETY: a read-only shared mapping of the client's pool; every read checks against `len` (`Pool::bytes`).
        let map = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, fd.as_raw_fd(), 0) };
        (map != libc::MAP_FAILED).then_some(map.cast())
    }

    fn bytes(&self, b: &Buffer) -> Option<&[u8]> {
        let end = b.offset.checked_add(b.stride.checked_mul(b.height as usize)?)?;
        // SAFETY: inside the mapping. The client can write to it while we read, which only ever tears a picture.
        (end <= self.len).then(|| unsafe { std::slice::from_raw_parts(self.map.add(b.offset), end - b.offset) })
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // SAFETY: mapped by `Pool::map` with this length.
        unsafe { libc::munmap(self.map.cast(), self.len) };
    }
}

/// A window (an xdg_toplevel's surface), or a surface without a role yet.
#[derive(Default)]
struct Surface {
    pending: Option<Option<u32>>,
    /// The buffer on show or next up, a client's wl_buffer id.
    current: Option<u32>,
    frames: Vec<u32>,
    toplevel: Option<u32>,
    xdg_surface: Option<u32>,
    app_id: String,
    configured: bool,
    /// Configured but no picture yet: it takes the screen with its first one, as under sway (a window without a buffer isn't
    /// mapped). pocket-core opens its window at once and draws only after the launch animation's hold; taking the screen at the
    /// configure froze that animation, whose frame callbacks stopped (seen on the Brick, 2026-11-03).
    waiting_map: bool,
    /// The sway container id, as in GET_TREE and `[con_id=N]`.
    con: i64,
}

struct Client {
    stream: UnixStream,
    objects: HashMap<u32, Obj>,
    pools: Vec<Option<Pool>>,
    surfaces: HashMap<u32, Surface>,
    input: Vec<u8>,
    fds: Vec<OwnedFd>,
    output: Vec<u8>,
    serial: u32,
    dead: bool,
}

fn word(body: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(body.get(at..at + 4)?.try_into().ok()?))
}

fn wl_string(body: &[u8], at: usize) -> Option<String> {
    let n = word(body, at)? as usize;
    let s = body.get(at + 4..(at + 4 + n).checked_sub(1)?)?;
    Some(String::from_utf8_lossy(s).into_owned())
}

impl Client {
    fn new(stream: UnixStream) -> Client {
        stream.set_nonblocking(true).ok();
        Client { stream, objects: HashMap::from([(1, Obj::Display)]), pools: Vec::new(), surfaces: HashMap::new(), input: Vec::new(), fds: Vec::new(), output: Vec::new(), serial: 0, dead: false }
    }

    fn send(&mut self, obj: u32, opcode: u16, payload: &[u8]) {
        self.output.extend_from_slice(&obj.to_le_bytes());
        self.output.extend_from_slice(&opcode.to_le_bytes());
        self.output.extend_from_slice(&((8 + payload.len()) as u16).to_le_bytes());
        self.output.extend_from_slice(payload);
    }

    fn flush(&mut self) {
        while !self.output.is_empty() {
            match self.stream.write(&self.output) {
                Ok(0) => break,
                Ok(n) => {
                    self.output.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => {
                    self.dead = true;
                    break;
                }
            }
        }
    }

    /// Reads what is there, the file descriptors that come along (wl_shm.create_pool) included.
    fn receive(&mut self) {
        let mut data = [0u8; 16384];
        // Room for 16 descriptors.
        let mut control = [0u64; 12];
        loop {
            let mut iov = libc::iovec { iov_base: data.as_mut_ptr().cast(), iov_len: data.len() };
            // SAFETY: a zeroed msghdr is valid; the pointers set stay valid across the call.
            let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = control.as_mut_ptr().cast();
            msg.msg_controllen = std::mem::size_of_val(&control) as _;
            // SAFETY: as above.
            let n = unsafe { libc::recvmsg(self.stream.as_raw_fd(), &mut msg, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() != ErrorKind::WouldBlock && e.kind() != ErrorKind::Interrupted {
                    self.dead = true;
                }
                return;
            }
            if n == 0 {
                self.dead = true;
                return;
            }
            // SAFETY: walking the control messages recvmsg just filled in, with the libc macros.
            unsafe {
                let mut c = libc::CMSG_FIRSTHDR(&msg);
                while !c.is_null() {
                    if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                        let count = ((*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / 4;
                        let fds = libc::CMSG_DATA(c).cast::<RawFd>();
                        for i in 0..count {
                            self.fds.push(OwnedFd::from_raw_fd(fds.add(i).read_unaligned()));
                        }
                    }
                    c = libc::CMSG_NXTHDR(&msg, c);
                }
            }
            self.input.extend_from_slice(&data[..n as usize]);
        }
    }
}

/// What a client's requests changed on the screen, for the main loop.
#[derive(Default)]
struct Effects {
    /// A surface was mapped: it takes the screen.
    mapped: Option<u32>,
    /// A surface committed a new picture.
    committed: Vec<u32>,
}

struct Server {
    fb: Fb,
    clients: HashMap<u64, Client>,
    /// Client and surface on the screen.
    focus: Option<(u64, u32)>,
    next_con: i64,
    next_client: u64,
    frames: u64,
    copy: Duration,
    worst: Duration,
    /// The GPU's way to the screen (`--gl`); without it, the copy.
    gl: Option<Presenter>,
    /// `--gl-window pbuffer` on a computer: what was drawn also goes to the file, so a test can see it.
    gl_to_file: bool,
    pace: Pace,
    /// What the GPU draws by itself between the clients' pictures.
    anim: Option<Anim>,
    /// A page transition pocket-ui announced (`pocket_transition`): it starts with the window's next picture.
    next_page: Option<(Transition, u32)>,
    /// The window changed: the next picture crossfades from the one on show.
    cross_next: bool,
    /// Shaders and screen frames (`brick_picture`): the renderer of pocket-core, on the GPU's context. None without `--gl` or when it
    /// didn't compile; then `pocket_shaders` and `pocket_picture` answer no and games keep the CPU copy.
    picture: Option<brick_picture::Layer>,
    /// The process that sent `pocket_picture`: its window is drawn by `picture`, the others by the plain presenter.
    picture_pid: Option<i32>,
    /// The picture on show was drawn by `picture`: a change of window then cuts instead of crossfading from a stale texture.
    shown_by_picture: bool,
    /// The peer of the IPC request being served.
    ipc_pid: Option<i32>,
    /// Over the game on the Brick (`pocket_overlay`): pocket-core's overlay (the FPS line, toasts, the volume bar) drawn over a game the
    /// renderer draws; its own GL context, made with the first texts. None: nothing over the game.
    hud: Option<(glow::Context, pocket_ui::overlay::Overlay, pocket_ui::overlay::Hud)>,
}

/// How long the GPU crossfades from one window to another (the pause menu over a game, a game starting or ending).
const CROSSFADE: Duration = Duration::from_millis(150);

enum Moving {
    /// A page of pocket-ui slides and fades in, as `motion::Transition` does it on the CPU, the page colour given.
    Page(Transition, u32),
    Cross(Instant),
}

/// One animation of the GPU's, and how evenly its frames came (for the log: one line each).
struct Anim {
    moving: Moving,
    started: Instant,
    last: Instant,
    frames: u32,
    worst: Duration,
}

impl Anim {
    fn new(moving: Moving) -> Anim {
        let now = Instant::now();
        Anim { moving, started: now, last: now, frames: 0, worst: Duration::ZERO }
    }

    /// A new animation in `slot`; the one it replaces gets its line in the log.
    fn start(slot: &mut Option<Anim>, moving: Moving) {
        if let Some(old) = slot.take() {
            old.report("cut short by the next");
        }
        *slot = Some(Anim::new(moving));
    }

    fn report(&self, how: &str) {
        eprintln!("pocket-fb: gpu {}: {} frames in {:.0} ms, worst gap {:.2} ms, {how}", self.name(), self.frames, self.started.elapsed().as_secs_f64() * 1000.0, self.worst.as_secs_f64() * 1000.0);
    }

    /// The look at `now`; None once over.
    fn look(&self, now: Instant) -> Option<Look> {
        match &self.moving {
            Moving::Page(t, bg) => t.look(now).map(|(dx, dy, fade)| Look { shift: (dx, dy), fade, bg: *bg, mixing: 1.0 }),
            Moving::Cross(at) => {
                let p = (now - *at).as_secs_f32() / CROSSFADE.as_secs_f32();
                // Ease out, as the CPU's fades.
                (p < 1.0).then(|| Look { mixing: 1.0 - (1.0 - p) * (1.0 - p), ..Look::STILL })
            }
        }
    }

    fn name(&self) -> String {
        match &self.moving {
            Moving::Page(t, _) => format!("page {:?}", t.enter).to_lowercase(),
            Moving::Cross(_) => "window crossfade".into(),
        }
    }
}

/// How evenly pictures reached the screen while something moved: the gaps between frames under `MOVING`. A frame's whole cost,
/// whichever way it went, and the number to compare the copy and the GPU by. No allocation, on the frame path.
#[derive(Default)]
struct Pace {
    last: Option<Instant>,
    gaps: u64,
    total: Duration,
    /// Later than one refresh and a bit: a refresh was missed.
    late: u64,
    worst: Duration,
}

const MOVING: Duration = Duration::from_millis(100);
const LATE: Duration = Duration::from_millis(20);

impl Pace {
    fn frame(&mut self) {
        let now = Instant::now();
        if let Some(gap) = self.last.map(|l| now - l).filter(|g| *g < MOVING) {
            self.gaps += 1;
            self.total += gap;
            self.late += u64::from(gap > LATE);
            self.worst = self.worst.max(gap);
        }
        self.last = Some(now);
    }
}

impl Server {
    /// Handles every whole message in the client's buffer.
    fn handle(&mut self, key: u64) {
        let Some(client) = self.clients.get_mut(&key) else { return };
        let mut fx = Effects::default();
        let mut at = 0;
        while let Some(head) = client.input.get(at..at + 8) {
            let id = u32::from_le_bytes(head[..4].try_into().unwrap());
            let opcode = u16::from_le_bytes(head[4..6].try_into().unwrap());
            let size = u16::from_le_bytes(head[6..8].try_into().unwrap()) as usize;
            if size < 8 {
                client.dead = true;
                break;
            }
            if client.input.len() < at + size {
                break;
            }
            let body = client.input[at + 8..at + size].to_vec();
            at += size;
            if request(client, &mut self.next_con, &mut fx, id, opcode, &body).is_none() {
                eprintln!("pocket-fb: a client sent a request that doesn't read: object {id} opcode {opcode}");
                client.dead = true;
                break;
            }
        }
        client.input.drain(..at);
        if let Some(surface) = fx.mapped {
            self.raise(key, surface);
        }
        for surface in fx.committed {
            if self.focus == Some((key, surface)) {
                self.paint();
                self.send_frames(key, surface);
            }
        }
        if let Some(c) = self.clients.get_mut(&key) {
            c.flush();
        }
    }

    /// Puts a window on the screen: its last picture, and the frame callbacks it was kept waiting for.
    fn raise(&mut self, key: u64, surface: u32) {
        // A window without a picture yet (focused over IPC before it drew) takes the screen with its first one.
        if let Some(sf) = self.clients.get_mut(&key).and_then(|c| c.surfaces.get_mut(&surface)) {
            if sf.current.is_none() {
                sf.waiting_map = true;
                return;
            }
        }
        // Another window: on the GPU its first picture crossfades from the one on show.
        self.cross_next = self.gl.is_some() && !self.shown_by_picture && self.focus.is_some_and(|f| f != (key, surface));
        self.focus = Some((key, surface));
        self.paint();
        self.send_frames(key, surface);
    }

    fn send_frames(&mut self, key: u64, surface: u32) {
        let Some(c) = self.clients.get_mut(&key) else { return };
        let Some(s) = c.surfaces.get_mut(&surface) else { return };
        let frames = std::mem::take(&mut s.frames);
        let now = monotonic_ms();
        for cb in frames {
            c.send(cb, 0, &now.to_le_bytes());
            c.send(1, 1, &cb.to_le_bytes());
            c.objects.remove(&cb);
        }
        c.flush();
    }

    fn paint(&mut self) {
        let Some((key, surface)) = self.focus else { return };
        let Some(c) = self.clients.get(&key) else { return };
        let Some(buffer) = c.surfaces.get(&surface).and_then(|s| s.current) else { return };
        let Some(Obj::Buffer(b)) = c.objects.get(&buffer) else { return };
        let Some(pixels) = c.pools.get(b.pool).and_then(Option::as_ref).and_then(|p| p.bytes(b)) else { return };
        let t = Instant::now();
        let mut lost = None;
        let by_picture = self.picture_pid.is_some() && self.picture_pid == peer_pid(&c.stream) && c.surfaces.get(&surface).is_some_and(|s| s.app_id != "pocket-ui") && self.picture.as_ref().is_some_and(brick_picture::Layer::active);
        match self.gl.as_mut() {
            // A game with shaders or a frame: pocket-core's renderer draws its picture at the picture's own size, on the screen.
            Some(gl) if !self.fb.blanked && by_picture => {
                self.cross_next = false;
                self.next_page = None;
                self.anim = None;
                self.shown_by_picture = true;
                let layer = self.picture.as_mut().expect("checked above");
                let drawn = layer.draw(pixels, b.width, b.height, b.stride, (self.fb.width, self.fb.height));
                if let (Ok(()), Some((hgl, overlay, hud))) = (&drawn, self.hud.as_mut()) {
                    overlay.draw(hgl, hud, self.fb.width, self.fb.height);
                }
                let shown = drawn.and_then(|()| gl.egl.swap());
                if let Err(e) = shown {
                    lost = Some(e);
                } else if self.gl_to_file {
                    let drawn = gl.read_xrgb();
                    self.fb.show(&drawn, self.fb.width, self.fb.height, self.fb.width as usize * 4);
                }
            }
            Some(gl) if !self.fb.blanked => {
                self.shown_by_picture = false;
                if std::mem::take(&mut self.cross_next) {
                    gl.keep_previous();
                    Anim::start(&mut self.anim, Moving::Cross(Instant::now()));
                }
                if let Some((mut t, bg)) = self.next_page.take() {
                    t.at = Instant::now();
                    Anim::start(&mut self.anim, Moving::Page(t, bg));
                }
                // While the GPU animates, the loop draws (`animate`); a picture only replaces what it moves.
                let shown = gl.upload(pixels, b.width, b.height, b.stride).and_then(|()| if self.anim.is_some() { Ok(()) } else { gl.compose(Look::STILL).and_then(|()| gl.egl.swap()) });
                if let Err(e) = shown {
                    lost = Some(e);
                } else if self.gl_to_file && self.anim.is_none() {
                    let drawn = gl.read_xrgb();
                    self.fb.show(&drawn, self.fb.width, self.fb.height, self.fb.width as usize * 4);
                }
            }
            Some(_) => {}
            None => show_copy(&mut self.fb, pixels, b.width, b.height, b.stride),
        }
        if let Some(e) = lost {
            // As `lose_gl`, field by field: the picture is still borrowed from its client.
            eprintln!("pocket-fb: GL failed: {e}; back to the CPU copy");
            (self.gl, self.anim, self.picture, self.picture_pid) = (None, None, None, None);
            self.fb.restore_pan();
            show_copy(&mut self.fb, pixels, b.width, b.height, b.stride);
        }
        let took = t.elapsed();
        self.frames += 1;
        self.copy += took;
        self.worst = self.worst.max(took);
        self.pace.frame();
    }

    /// A GPU that failed: the copy from here on, on the page we found.
    fn lose_gl(&mut self, why: &str) {
        eprintln!("pocket-fb: GL failed: {why}; back to the CPU copy");
        self.gl = None;
        self.anim = None;
        self.picture = None;
        self.picture_pid = None;
        self.fb.restore_pan();
    }

    /// One frame of the GPU's own animation, if one runs: waits for the flip, so the loop goes at the screen's rate meanwhile.
    fn animate(&mut self) {
        let Some(a) = self.anim.as_mut() else { return };
        let Some(gl) = self.gl.as_mut() else {
            self.anim = None;
            return;
        };
        if self.fb.blanked {
            return;
        }
        let now = Instant::now();
        let look = a.look(now);
        let gap = now - a.last;
        if a.frames > 0 {
            a.worst = a.worst.max(gap);
        }
        a.frames += 1;
        a.last = now;
        if let Err(e) = gl.compose(look.unwrap_or(Look::STILL)).and_then(|()| gl.egl.swap()) {
            self.lose_gl(&e);
            self.paint();
            return;
        }
        if self.gl_to_file {
            let drawn = gl.read_xrgb();
            self.fb.show(&drawn, self.fb.width, self.fb.height, self.fb.width as usize * 4);
        }
        self.pace.frame();
        if look.is_none() {
            a.report("done");
            self.anim = None;
        }
    }

    /// The window to show when `focus` is gone.
    fn fall_back(&mut self) {
        let next = self.clients.iter().find_map(|(k, c)| c.surfaces.iter().find(|(_, s)| s.configured).map(|(id, _)| (*k, *id)));
        match next {
            Some((k, s)) => self.raise(k, s),
            None => self.focus = None,
        }
    }

    fn drop_dead(&mut self) {
        let dead: Vec<u64> = self.clients.iter().filter(|(_, c)| c.dead).map(|(k, _)| *k).collect();
        for key in dead {
            if let Some(c) = self.clients.remove(&key) {
                // The game that asked for shaders is gone: nothing draws its pictures any more.
                if self.picture_pid.is_some() && self.picture_pid == peer_pid(&c.stream) {
                    self.picture_pid = None;
                    if let Some(layer) = self.picture.as_mut() {
                        layer.clear();
                    }
                }
            }
            if self.focus.is_some_and(|(k, _)| k == key) {
                self.focus = None;
                self.fall_back();
            }
        }
    }

    fn windows(&self) -> Vec<(u64, u32, i64, String)> {
        let mut all: Vec<_> = self.clients.iter().flat_map(|(k, c)| c.surfaces.iter().filter(|(_, s)| s.toplevel.is_some()).map(|(id, s)| (*k, *id, s.con, s.app_id.clone()))).collect();
        all.sort_by_key(|w| w.2);
        all
    }

    /// sway's GET_TREE, as much of it as `control::focused_in` and `standalone` read.
    fn tree(&self) -> String {
        let nodes: Vec<_> = self.windows().into_iter().map(|(k, s, con, app)| {
            let pid = self.clients.get(&k).and_then(|c| peer_pid(&c.stream));
            serde_json::json!({"type": "con", "id": con, "app_id": app, "pid": pid, "focused": self.focus == Some((k, s)), "visible": self.focus == Some((k, s)), "nodes": [], "floating_nodes": []})
        }).collect();
        serde_json::json!({"type": "root", "id": 1, "nodes": [{"type": "output", "name": "FB-0", "nodes": [{"type": "workspace", "name": "1", "nodes": nodes, "floating_nodes": []}], "floating_nodes": []}], "floating_nodes": []}).to_string()
    }

    fn outputs(&self) -> String {
        serde_json::json!([{"name": "FB-0", "active": !self.fb.blanked, "power": !self.fb.blanked, "dpms": !self.fb.blanked, "transform": "normal", "scale": 1.0,
            "rect": {"x": 0, "y": 0, "width": self.fb.width, "height": self.fb.height},
            "current_mode": {"width": self.fb.width, "height": self.fb.height, "refresh": 60000}, "modes": []}])
        .to_string()
    }

    /// One line of sway commands: `;` between commands, `,` between actions on the same windows. What only arranges windows
    /// on a screen of one (fullscreen, `for_window`: a new window takes the screen anyway) succeeds and does nothing.
    /// The focused window's picture as a PNG in `path` (a temporary file, then a rename). False without a picture or when the file can't be written.
    /// The pictures here are wl_shm's ARGB8888 / XRGB8888: little-endian words, so the bytes are blue, green, red, unused.
    fn grab(&self, path: &str) -> bool {
        let Some((key, surface)) = self.focus else { return false };
        let Some(c) = self.clients.get(&key) else { return false };
        let Some(buffer) = c.surfaces.get(&surface).and_then(|s| s.current) else { return false };
        let Some(Obj::Buffer(b)) = c.objects.get(&buffer) else { return false };
        let Some(pixels) = c.pools.get(b.pool).and_then(Option::as_ref).and_then(|p| p.bytes(b)) else { return false };
        let Some(rgb) = pocket_ui::screenshot::xrgb_to_rgb(pixels, b.width, b.height, b.stride) else { return false };
        let Some(png) = pocket_ui::screenshot::encode_rgb(b.width, b.height, &rgb) else { return false };
        !path.is_empty() && pocket_ui::fsutil::write_atomic(std::path::Path::new(path), &png).is_ok()
    }

    fn run_command(&mut self, line: &str) -> String {
        let mut results = Vec::new();
        for command in split_commands(line) {
            let (criteria, actions) = parse_command(&command);
            for action in actions.split(',').map(str::trim) {
                let ok = self.run_action(criteria.as_deref(), action);
                results.push(if ok { serde_json::json!({"success": true}) } else { serde_json::json!({"success": false, "error": "No matching node."}) });
            }
        }
        serde_json::Value::Array(results).to_string()
    }

    fn run_action(&mut self, criteria: Option<&str>, action: &str) -> bool {
        let verb = action.split_whitespace().next().unwrap_or("");
        match (verb, criteria) {
            // The GPU's own animations (`brick_gpu` in pocket-ui): only with `--gl`, so a "no" keeps pocket-ui on the CPU.
            ("pocket_gpu", None) => self.gl.is_some(),
            // Shaders and screen frames (`brick_picture`): yes only with a GPU and a renderer that compiled.
            ("pocket_shaders", None) => self.gl.is_some() && self.picture.is_some(),
            ("pocket_picture_shot", None) => self.picture_shot(action.trim_start_matches("pocket_picture_shot").trim()),
            ("pocket_overlay", None) => self.overlay_command(action.trim_start_matches("pocket_overlay").trim()),
            ("pocket_picture_preview", None) => self.picture_preview(action.trim_start_matches("pocket_picture_preview").trim()),
            ("pocket_picture", None) => self.picture_command(action.trim_start_matches("pocket_picture").trim()),
            ("pocket_transition", None) => match pocket_ui::brick_gpu::read(action.trim_start_matches("pocket_transition")) {
                Some((enter, top, bg, cozy, moves)) if self.gl.is_some() && !self.fb.blanked => {
                    pocket_ui::motion::set_cozy(cozy);
                    pocket_ui::motion::set_level(if moves { pocket_ui::motion::Motion::Full } else { pocket_ui::motion::Motion::Reduced });
                    self.next_page = Transition::new(enter, Instant::now(), top).map(|t| (t, bg));
                    self.next_page.is_some()
                }
                _ => false,
            },
            // The screen as a PNG for the `grim` stand-in (docs/TRIMUI-BRICK-MORE.md): `pocket_grab <file>`.
            ("pocket_grab", None) => self.grab(action.trim_start_matches("pocket_grab").trim()),
            ("output", _) => {
                if action.ends_with("power off") {
                    self.fb.blank(true);
                } else if action.ends_with("power on") {
                    self.fb.blank(false);
                    self.paint();
                }
                true
            }
            ("focus", Some(c)) => match self.matching(c).last().cloned() {
                Some((k, s, ..)) => {
                    self.raise(k, s);
                    true
                }
                None => false,
            },
            ("kill", Some(c)) => {
                let found = self.matching(c);
                for (k, s, ..) in &found {
                    let Some(client) = self.clients.get_mut(k) else { continue };
                    if let Some(top) = client.surfaces.get(s).and_then(|x| x.toplevel) {
                        client.send(top, 1, &[]);
                        client.flush();
                    }
                }
                !found.is_empty()
            }
            _ => criteria.is_none_or(|c| !self.matching(c).is_empty() || c.starts_with("class")),
        }
    }

    /// `pocket_picture_shot <seq> <w> <h>`: the pause menu's background drawn by the renderer (shaders, frame) from the last picture, written
    /// as pocket-core's menu shot, so a Picture choice in the menu shows behind it at once, as on ROCKNIX.
    fn picture_shot(&mut self, words: &str) -> bool {
        let mut words = words.split_whitespace();
        let mut num = || words.next().and_then(|w| w.parse::<u32>().ok());
        let (Some(seq), Some(w), Some(h)) = (num(), num(), num()) else { return false };
        // A fourth word is another file (the tests); pocket-core's is preview::SHOT_FILE.
        let file = words.next().map_or_else(|| std::path::PathBuf::from(pocket_ui::preview::SHOT_FILE), std::path::PathBuf::from);
        if self.gl.is_none() || w == 0 || h == 0 {
            return false;
        }
        let Some(layer) = self.picture.as_mut().filter(|l| l.active()) else { return false };
        let t = Instant::now();
        let Some(rgba) = layer.shot(w, h) else { return false };
        let shot = pocket_ui::preview::shot_from_screen(&rgba, w, h);
        // The game's rectangle first, the picture last, as pocket-core's own shot (Zoom crops to the game).
        let game = layer.game_rect(w, h).map(pocket_ui::preview::game_rect_image);
        let images: Vec<&pocket_ui::preview::Pixels> = game.iter().chain(std::iter::once(&shot)).collect();
        if let Err(e) = pocket_ui::preview::write(&file, seq, &images) {
            eprintln!("pocket-fb: menu shot {seq}: {e}");
            return false;
        }
        eprintln!("pocket-fb: menu shot {seq}: {w}x{h} by the renderer in {:.1} ms", t.elapsed().as_secs_f32() * 1000.0);
        true
    }

    /// `pocket_picture_preview <seq> <screen w> <screen h> <w> <h>`: the Picture page's live preview (`preview::from_screen`: the centre at
    /// 1:1 and the whole screen) drawn by the renderer from the last picture with the settings just sent, written as `preview::FILE`. On
    /// ROCKNIX pocket-core draws it with its own GL; on the Brick pocket-core has none, so without this the page's preview stayed empty.
    fn picture_preview(&mut self, words: &str) -> bool {
        let mut words = words.split_whitespace();
        let mut num = || words.next().and_then(|w| w.parse::<u32>().ok());
        let (Some(seq), Some(sw), Some(sh), Some(w), Some(h)) = (num(), num(), num(), num(), num()) else { return false };
        // A sixth word is another file (the tests).
        let file = words.next().map_or_else(|| std::path::PathBuf::from(pocket_ui::preview::FILE), std::path::PathBuf::from);
        if self.gl.is_none() || sw == 0 || sh == 0 || w == 0 || h == 0 {
            return false;
        }
        let Some(layer) = self.picture.as_mut().filter(|l| l.active()) else { return false };
        let t = Instant::now();
        let Some(rgba) = layer.shot(sw, sh) else { return false };
        let (crop, overview) = pocket_ui::preview::from_screen(&rgba, sw, sh, w, h);
        if let Err(e) = pocket_ui::preview::write(&file, seq, &[&crop, &overview]) {
            eprintln!("pocket-fb: preview {seq}: {e}");
            return false;
        }
        eprintln!("pocket-fb: preview {seq}: {}x{} + {}x{} by the renderer in {:.1} ms", crop.w, crop.h, overview.w, overview.h, t.elapsed().as_secs_f32() * 1000.0);
        true
    }

    /// `pocket_overlay key=hex ...` (`brick_picture::overlay_command`): what pocket-core lays over the game now, drawn over it from its next
    /// picture on. The first one makes the overlay on the GPU's context. False without the GPU path or when a word does not read.
    fn overlay_command(&mut self, words: &str) -> bool {
        let Some(gl) = self.gl.as_ref() else { return false };
        let Some(texts) = brick_picture::read_overlay(words) else { return false };
        if self.hud.is_none() {
            let hgl = gl.egl.another_gl();
            match pocket_ui::overlay::Overlay::new(&hgl) {
                Ok(overlay) => self.hud = Some((hgl, overlay, pocket_ui::overlay::Hud::default())),
                Err(e) => {
                    eprintln!("pocket-fb: over the game off, the overlay didn't compile: {e}");
                    return false;
                }
            }
        }
        if let Some((_, _, hud)) = self.hud.as_mut() {
            hud.show_texts(texts);
        }
        true
    }

    /// `pocket_picture off`, or the settings of the sender's pictures. True when the sender's window is drawn with them from now on.
    fn picture_command(&mut self, words: &str) -> bool {
        let (Some(layer), Some(_)) = (self.picture.as_mut(), self.gl.as_ref()) else { return false };
        if words == "off" {
            if self.picture_pid == self.ipc_pid {
                layer.clear();
                self.picture_pid = None;
            }
            return true;
        }
        let Some(request) = brick_picture::Request::read(words) else { return false };
        // Another game: nothing of the last one's overlay over it.
        if self.picture_pid != self.ipc_pid {
            if let Some((_, _, hud)) = self.hud.as_mut() {
                hud.show_texts(Default::default());
            }
        }
        eprintln!("pocket-fb: picture for {}: {}", request.system, request.values.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "));
        layer.set(request);
        self.picture_pid = self.ipc_pid;
        // The picture on show is drawn again with the new settings at once, as the menu changes them.
        if self.focus.is_some() && !self.fb.blanked {
            self.paint();
        }
        true
    }

    fn matching(&self, criteria: &str) -> Vec<(u64, u32, i64, String)> {
        self.windows().into_iter().filter(|(_, _, con, app)| criteria_match(criteria, *con, app)).collect()
    }
}

/// The three kinds of criteria control.rs uses: `con_id=N`, `app_id="name"`, `app_id="^(?!name$).*"`. `class` never matches, there is no X here.
pub fn criteria_match(criteria: &str, con: i64, app: &str) -> bool {
    if let Some(id) = criteria.strip_prefix("con_id=") {
        return id.parse() == Ok(con);
    }
    let Some(want) = criteria.strip_prefix("app_id=") else { return false };
    let want = want.trim_matches('"');
    match want.strip_prefix("^(?!").and_then(|w| w.strip_suffix("$).*")) {
        Some(not) => app != not,
        None => app == want,
    }
}

/// A command line split at the `;` outside of brackets and quotes.
pub fn split_commands(line: &str) -> Vec<String> {
    let (mut out, mut now, mut quote) = (Vec::new(), String::new(), false);
    for c in line.chars() {
        match c {
            '"' => {
                quote = !quote;
                now.push(c);
            }
            ';' if !quote => out.push(std::mem::take(&mut now)),
            _ => now.push(c),
        }
    }
    out.push(now);
    out.into_iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
}

/// `[criteria] actions` to its two parts. `for_window [...] ...` has no leading bracket, so it reads as one action.
pub fn parse_command(command: &str) -> (Option<String>, String) {
    if let Some(rest) = command.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return (Some(rest[..end].to_string()), rest[end + 1..].trim().to_string());
        }
    }
    (None, command.to_string())
}

/// The CPU copy. A picture smaller than the screen is a game's own (it handed it over at its size for the shaders, and the GPU is
/// gone): placed as `soft_screen` places it, so it shows right either way.
fn show_copy(fb: &mut Fb, pixels: &[u8], width: u32, height: u32, stride: usize) {
    let (sw, sh) = (fb.width, fb.height);
    if (width >= sw && height >= sh) || width == 0 || height == 0 || stride < width as usize * 4 || pixels.len() < stride * (height as usize - 1) + width as usize * 4 {
        fb.show(pixels, width, height, stride);
        return;
    }
    let frame = pocket_ui::libretro::Frame { width, height, pitch: stride, format: pocket_ui::libretro::PIXEL_XRGB8888, data: pixels.to_vec() };
    let mut px = vec![0u32; sw as usize * sh as usize];
    pocket_ui::soft_screen::blit(&frame, &mut px, sw as usize, sh as usize);
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    fb.show(&bytes, sw, sh, sw as usize * 4);
}

fn peer_pid(stream: &UnixStream) -> Option<i32> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `len` bytes into `cred`.
    let r = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) };
    (r == 0).then_some(cred.pid)
}

fn monotonic_ms() -> u32 {
    let mut t: libc::timespec = unsafe { std::mem::zeroed() };
    // SAFETY: writes into `t`.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) };
    (t.tv_sec as u64 * 1000 + t.tv_nsec as u64 / 1_000_000) as u32
}

/// One request. None when it doesn't read (a short message, an unknown object).
fn request(c: &mut Client, next_con: &mut i64, fx: &mut Effects, id: u32, opcode: u16, body: &[u8]) -> Option<()> {
    let obj = c.objects.get(&id)?.clone();
    match (obj, opcode) {
        (Obj::Display, 0) => {
            let cb = word(body, 0)?;
            c.serial += 1;
            let serial = c.serial;
            c.send(cb, 0, &serial.to_le_bytes());
            c.send(1, 1, &cb.to_le_bytes());
        }
        (Obj::Display, 1) => {
            let reg = word(body, 0)?;
            c.objects.insert(reg, Obj::Registry);
            for (name, iface, version) in GLOBALS {
                let mut p = name.to_le_bytes().to_vec();
                push_string(&mut p, iface);
                p.extend_from_slice(&version.to_le_bytes());
                c.send(reg, 0, &p);
            }
        }
        (Obj::Registry, 0) => {
            let iface = wl_string(body, 4)?;
            let n = word(body, 4)? as usize;
            let new_id = word(body, 8 + n.next_multiple_of(4) + 4)?;
            let kind = match iface.as_str() {
                "wl_compositor" => Obj::Compositor,
                "wl_shm" => Obj::Shm,
                "xdg_wm_base" => Obj::WmBase,
                _ => return None,
            };
            c.objects.insert(new_id, kind);
            if iface == "wl_shm" {
                c.send(new_id, 0, &WL_SHM_ARGB8888.to_le_bytes());
                c.send(new_id, 0, &WL_SHM_XRGB8888.to_le_bytes());
            }
        }
        (Obj::Compositor, 0) => {
            let s = word(body, 0)?;
            c.objects.insert(s, Obj::Surface(s));
            c.surfaces.insert(s, Surface::default());
        }
        (Obj::Compositor, 1) => {
            c.objects.insert(word(body, 0)?, Obj::Region);
        }
        (Obj::Shm, 0) => {
            let pool_id = word(body, 0)?;
            let len = word(body, 4)? as usize;
            if c.fds.is_empty() {
                return None;
            }
            let fd = c.fds.remove(0);
            let map = Pool::map(&fd, len)?;
            c.pools.push(Some(Pool { fd, map, len }));
            c.objects.insert(pool_id, Obj::Pool { pool: c.pools.len() - 1 });
        }
        (Obj::Pool { pool }, 0) => {
            let buffer = word(body, 0)?;
            let (offset, width, height, stride) = (word(body, 4)? as usize, word(body, 8)?, word(body, 12)?, word(body, 16)? as usize);
            c.objects.insert(buffer, Obj::Buffer(Buffer { pool, offset, width, height, stride }));
        }
        (Obj::Pool { .. }, 1) => {
            // The mapping stays while buffers use it; it goes with the client.
            c.objects.remove(&id);
            c.send(1, 1, &id.to_le_bytes());
        }
        (Obj::Pool { pool }, 2) => {
            let len = word(body, 0)? as usize;
            let p = c.pools.get_mut(pool)?.as_mut()?;
            let map = Pool::map(&p.fd, len)?;
            // SAFETY: mapped by `Pool::map` with this length, nothing refers to it after this.
            unsafe { libc::munmap(p.map.cast(), p.len) };
            p.map = map;
            p.len = len;
        }
        (Obj::Buffer(_), 0) | (Obj::Region, 0) | (Obj::Positioner, 0) => {
            c.objects.remove(&id);
            c.send(1, 1, &id.to_le_bytes());
        }
        (Obj::Surface(s), 0) => {
            c.surfaces.remove(&s);
            c.objects.remove(&id);
            c.send(1, 1, &id.to_le_bytes());
        }
        (Obj::Surface(s), 1) => {
            let buffer = word(body, 0)?;
            c.surfaces.get_mut(&s)?.pending = Some((buffer != 0).then_some(buffer));
        }
        (Obj::Surface(s), 3) => {
            let cb = word(body, 0)?;
            c.objects.insert(cb, Obj::Callback);
            c.surfaces.get_mut(&s)?.frames.push(cb);
        }
        (Obj::Surface(s), 6) => {
            let surface = c.surfaces.get_mut(&s)?;
            let first_configure = surface.toplevel.is_some() && !surface.configured;
            let mut released = None;
            if let Some(next) = surface.pending.take() {
                released = surface.current.filter(|old| Some(*old) != next);
                surface.current = next;
            }
            let (top, xdg) = (surface.toplevel, surface.xdg_surface);
            if first_configure {
                surface.configured = true;
                *next_con += 1;
                surface.con = *next_con;
                let (top, xdg) = (top?, xdg?);
                let (w, h) = c_screen();
                // Fullscreen (2) and activated (4), as sway tells a focused fullscreen window.
                let mut p = w.to_le_bytes().to_vec();
                p.extend_from_slice(&h.to_le_bytes());
                p.extend_from_slice(&8u32.to_le_bytes());
                p.extend_from_slice(&2u32.to_le_bytes());
                p.extend_from_slice(&4u32.to_le_bytes());
                c.send(top, 0, &p);
                c.serial += 1;
                let serial = c.serial;
                c.send(xdg, 0, &serial.to_le_bytes());
                let surface = c.surfaces.get_mut(&s)?;
                if surface.current.is_some() {
                    fx.mapped = Some(s);
                } else {
                    surface.waiting_map = true;
                }
            } else {
                let surface = c.surfaces.get_mut(&s)?;
                if surface.waiting_map && surface.current.is_some() {
                    surface.waiting_map = false;
                    fx.mapped = Some(s);
                } else {
                    fx.committed.push(s);
                }
            }
            if let Some(old) = released {
                c.send(old, 0, &[]);
            }
        }
        (Obj::Surface(_), _) | (Obj::Region, _) | (Obj::Positioner, _) | (Obj::Buffer(_), _) => {}
        (Obj::WmBase, 0) => {
            c.objects.remove(&id);
            c.send(1, 1, &id.to_le_bytes());
        }
        (Obj::WmBase, 1) => {
            c.objects.insert(word(body, 0)?, Obj::Positioner);
        }
        (Obj::WmBase, 2) => {
            let (x, surface) = (word(body, 0)?, word(body, 4)?);
            c.objects.insert(x, Obj::XdgSurface { surface });
            c.surfaces.get_mut(&surface)?.xdg_surface = Some(x);
        }
        (Obj::XdgSurface { surface }, 1) => {
            let top = word(body, 0)?;
            c.objects.insert(top, Obj::Toplevel { surface });
            c.surfaces.get_mut(&surface)?.toplevel = Some(top);
        }
        (Obj::XdgSurface { .. } | Obj::Toplevel { .. }, 0) => {
            c.objects.remove(&id);
            c.send(1, 1, &id.to_le_bytes());
        }
        (Obj::Toplevel { surface }, 3) => {
            c.surfaces.get_mut(&surface)?.app_id = wl_string(body, 0)?;
        }
        (Obj::Callback | Obj::Compositor | Obj::Shm | Obj::Registry | Obj::Display | Obj::Pool { .. } | Obj::WmBase | Obj::XdgSurface { .. } | Obj::Toplevel { .. }, _) => {}
    }
    Some(())
}

static SCREEN: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();

fn c_screen() -> (u32, u32) {
    SCREEN.get().copied().unwrap_or((1024, 768))
}

fn push_string(p: &mut Vec<u8>, s: &str) {
    let n = s.len() + 1;
    p.extend_from_slice(&(n as u32).to_le_bytes());
    p.extend_from_slice(s.as_bytes());
    p.push(0);
    p.resize(p.len() + (4 - n % 4) % 4, 0);
}

// ---- The sway IPC side ----

const MAGIC: &[u8; 6] = b"i3-ipc";

/// Answers every whole message on this connection. False when it's gone.
fn serve_ipc(server: &mut Server, stream: &mut UnixStream, input: &mut Vec<u8>) -> bool {
    let mut buf = [0u8; 4096];
    let alive = match stream.read(&mut buf) {
        Ok(0) => false,
        Ok(n) => {
            input.extend_from_slice(&buf[..n]);
            true
        }
        Err(e) => matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
    };
    while input.len() >= 14 {
        if &input[..6] != MAGIC {
            return false;
        }
        let len = u32::from_ne_bytes(input[6..10].try_into().unwrap()) as usize;
        let kind = u32::from_ne_bytes(input[10..14].try_into().unwrap());
        if input.len() < 14 + len {
            break;
        }
        let payload = String::from_utf8_lossy(&input[14..14 + len]).into_owned();
        input.drain(..14 + len);
        let answer = match kind {
            0 => {
                server.ipc_pid = peer_pid(stream);
                server.run_command(&payload)
            }
            2 => r#"{"success":true}"#.to_string(),
            3 => server.outputs(),
            4 => server.tree(),
            _ => "[]".to_string(),
        };
        let mut msg = MAGIC.to_vec();
        msg.extend_from_slice(&(answer.len() as u32).to_ne_bytes());
        msg.extend_from_slice(&kind.to_ne_bytes());
        msg.extend_from_slice(answer.as_bytes());
        stream.set_nonblocking(false).ok();
        stream.set_write_timeout(Some(Duration::from_millis(200))).ok();
        let sent = stream.write_all(&msg).is_ok();
        stream.set_nonblocking(true).ok();
        if !sent {
            return false;
        }
    }
    alive
}

// ---- The loop ----

fn listen(path: &str) -> Result<UnixListener, String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(format!("{path}: {e}")),
        _ => {}
    }
    // Bound under another name and renamed, so the path appearing means that connect() works (as in wayland-stub.py).
    let temp = format!("{path}.bind");
    let _ = std::fs::remove_file(&temp);
    let l = UnixListener::bind(&temp).map_err(|e| format!("{temp}: {e}"))?;
    l.set_nonblocking(true).ok();
    std::fs::rename(&temp, path).map_err(|e| format!("{path}: {e}"))?;
    Ok(l)
}

struct Args {
    wayland: String,
    ipc: String,
    fb: String,
    mode: Option<(u32, u32, u32)>,
    vsync: bool,
    bgr: bool,
    picture: Option<String>,
    message: Option<String>,
    ipc_send: Option<(String, String)>,
    hold: u64,
    gl: bool,
    gl_window: String,
    /// `--gl-trust`: the GPU's picture counts as shown without reading it back from fb0. The Smart Pro's display shows the GL layer,
    /// not fb0 (docs/TRIMUI-SMART-PRO.md), so the Brick's proof (fb0's visible page holds what was drawn) would always fail there.
    gl_trust: bool,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args { wayland: String::new(), ipc: String::new(), fb: "/dev/fb0".into(), mode: None, vsync: true, bgr: false, picture: None, message: None, ipc_send: None, hold: 5, gl: false, gl_window: "null".into(), gl_trust: false };
    let mut it = args;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wayland" => out.wayland = it.next().ok_or("--wayland <socket>")?,
            "--ipc" => out.ipc = it.next().ok_or("--ipc <socket>")?,
            "--fb" => out.fb = it.next().ok_or("--fb <path>")?,
            "--no-vsync" => out.vsync = false,
            "--bgr" => out.bgr = true,
            "--gl" => out.gl = true,
            "--gl-trust" => out.gl_trust = true,
            "--gl-window" => out.gl_window = it.next().ok_or("--gl-window <null|pbuffer>")?,
            "--picture" => out.picture = Some(it.next().ok_or("--picture <file.png>")?),
            "--message" => out.message = Some(it.next().ok_or("--message <text, | starts a new line>")?),
            "--ipc-send" => {
                let socket = it.next().ok_or("--ipc-send <socket> <command>")?;
                out.ipc_send = Some((socket, it.next().ok_or("--ipc-send <socket> <command>")?));
            }
            "--hold" => out.hold = it.next().and_then(|v| v.parse().ok()).ok_or("--hold <seconds>")?,
            "--mode" => {
                let m = it.next().ok_or("--mode <w>x<h>x<bits>")?;
                let p: Vec<u32> = m.split('x').filter_map(|v| v.parse().ok()).collect();
                let [w, h, bits] = p[..] else { return Err(format!("--mode {m}")) };
                out.mode = Some((w, h, bits));
            }
            other => return Err(format!("don't know {other}")),
        }
    }
    if out.picture.is_none() && out.message.is_none() && out.ipc_send.is_none() && (out.wayland.is_empty() || out.ipc.is_empty()) {
        return Err("usage: pocket-fb --wayland <socket> --ipc <socket> [--fb /dev/fb0] [--mode 1024x768x32] [--no-vsync] [--bgr] [--gl] [--gl-trust] [--gl-window null]".into());
    }
    Ok(out)
}

/// `--picture`: as XRGB8888, top left, for `hold` seconds or until a signal; Fb's Drop puts the screen back.
fn show_picture(fb: &mut Fb, path: &str, hold: u64, gl: Option<&str>) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("{path}: {e}"))?;
    let mut raw = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut raw).map_err(|e| format!("{path}: {e}"))?;
    let channels = info.color_type.samples();
    if channels < 3 {
        return Err(format!("{path}: {channels} channels, wanted colour"));
    }
    let mut xrgb = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    for row in raw.chunks_exact(info.line_size).take(info.height as usize) {
        for p in row.chunks_exact(channels) {
            xrgb.extend_from_slice(&[p[2], p[1], p[0], 0]);
        }
    }
    // SAFETY: the handler only stores into an atomic.
    unsafe {
        for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(s, on_signal as *const () as libc::sighandler_t);
        }
    }
    let t = Instant::now();
    fb.show(&xrgb, info.width, info.height, info.width as usize * 4);
    println!("pocket-fb: {}x{} on the screen in {:.2} ms, format {:?}", info.width, info.height, t.elapsed().as_secs_f64() * 1000.0, fb.format);
    hold_still(fb, &xrgb, (info.width, info.height), hold, gl);
    Ok(())
}

/// A still picture (`--picture`, `--message`), already on fb0, held for `hold` seconds or until a signal. With `--gl` it goes through
/// the GPU as well: on a display that shows the GL layer and not fb0 (the Smart Pro) fb0 alone is a black screen. A GPU that fails
/// here costs a line; fb0 still has the picture.
fn hold_still(fb: &mut Fb, xrgb: &[u8], (w, h): (u32, u32), hold: u64, gl: Option<&str>) {
    let mut presenter = gl.and_then(|window| match Presenter::new(window, (fb.width as i32, fb.height as i32), &mut |line| eprintln!("pocket-fb: gl: {line}")) {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("pocket-fb: GL failed at {e}; the picture is on fb0 only");
            None
        }
    });
    if let Some(p) = presenter.as_mut() {
        // Twice: whichever of the WSI's buffers is shown holds the picture.
        for _ in 0..2 {
            if let Err(e) = p.show(xrgb, w, h, w as usize * 4) {
                eprintln!("pocket-fb: GL failed: {e}; the picture is on fb0 only");
                break;
            }
        }
    }
    let until = Instant::now() + Duration::from_secs(hold);
    while Instant::now() < until && !STOP.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(50));
    }
    if presenter.take().is_some() {
        fb.restore_pan();
    }
}

/// `--message`: white 8x8 letters at 4x on dark blue, `|` starts a new line, for `hold` seconds. What the launcher says when
/// a library, a game or the framebuffer is missing and pocket-ui cannot run to say it itself.
fn show_message(fb: &mut Fb, text: &str, hold: u64, gl: Option<&str>) -> Result<(), String> {
    use font8x8::legacy::BASIC_LEGACY;
    const SCALE: usize = 4;
    let (w, h) = (fb.width as usize, fb.height as usize);
    let mut xrgb = vec![0u8; w * h * 4];
    for px in xrgb.chunks_exact_mut(4) {
        px.copy_from_slice(&[0x40, 0x20, 0x10, 0]);
    }
    for (row, line) in text.split('|').enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let glyph = BASIC_LEGACY.get(ch as usize).or_else(|| BASIC_LEGACY.get(b'?' as usize)).copied().unwrap_or([0; 8]);
            for (gy, bits) in glyph.iter().enumerate() {
                for gx in 0..8 {
                    if bits >> gx & 1 == 0 {
                        continue;
                    }
                    for (dy, dx) in (0..SCALE).flat_map(|dy| (0..SCALE).map(move |dx| (dy, dx))) {
                        let (x, y) = (32 + (col * 8 + gx) * SCALE + dx, 48 + (row * 10 + gy) * SCALE + dy);
                        if x < w && y < h {
                            xrgb[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&[0xff; 3]);
                        }
                    }
                }
            }
        }
    }
    // SAFETY: the handler only stores into an atomic.
    unsafe {
        for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(s, on_signal as *const () as libc::sighandler_t);
        }
    }
    fb.show(&xrgb, w as u32, h as u32, w * 4);
    hold_still(fb, &xrgb, (w as u32, h as u32), hold, gl);
    Ok(())
}

/// `--ipc-send`: one sway-IPC command to a running bridge (what `control.rs` sends), its reply on stdout. Exit code 1 when the
/// reply does not say success. The kit's pause stage uses it: this system has no python and no `swaymsg`.
fn ipc_send(socket: &str, command: &str) -> Result<(), String> {
    let mut s = UnixStream::connect(socket).map_err(|e| format!("{socket}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
    let mut msg = b"i3-ipc".to_vec();
    msg.extend_from_slice(&(command.len() as u32).to_ne_bytes());
    msg.extend_from_slice(&0u32.to_ne_bytes());
    msg.extend_from_slice(command.as_bytes());
    s.write_all(&msg).map_err(|e| e.to_string())?;
    let mut head = [0u8; 14];
    s.read_exact(&mut head).map_err(|e| e.to_string())?;
    let mut body = vec![0u8; u32::from_ne_bytes([head[6], head[7], head[8], head[9]]) as usize];
    s.read_exact(&mut body).map_err(|e| e.to_string())?;
    let reply = String::from_utf8_lossy(&body).to_string();
    println!("{reply}");
    if reply.contains("\"success\":true") { Ok(()) } else { Err("not successful".into()) }
}

/// A colour for the first frames, nearly black so nobody sees it, and exact in every format the blob could choose.
const CHECK_COLOUR: (u8, u8, u8) = (8, 16, 24);

/// The GPU's way, if it works here. None, said in the log, and the copy goes on: no black screen because of the GPU. A surface the
/// blob accepts is not enough: what was drawn must be on the page the kernel shows, so that is read back after a few swaps.
fn start_gl(fb: &mut Fb, window: &str, trust: bool) -> Option<Presenter> {
    let mut gl = match Presenter::new(window, (fb.width as i32, fb.height as i32), &mut |line| eprintln!("pocket-fb: gl: {line}")) {
        Ok(gl) => gl,
        Err(e) => {
            eprintln!("pocket-fb: GL failed at {e}; using the CPU copy");
            return None;
        }
    };
    if trust && !fb.plain_file {
        if let Err(e) = gl.flood(CHECK_COLOUR) {
            eprintln!("pocket-fb: GL failed: {e}; using the CPU copy");
            drop(gl);
            fb.restore_pan();
            return None;
        }
        eprintln!("pocket-fb: GL draws; shown without a read-back from fb0 (--gl-trust)");
        return Some(gl);
    }
    if !fb.plain_file {
        let mut seen = None;
        for _ in 0..12 {
            if let Err(e) = gl.flood(CHECK_COLOUR) {
                eprintln!("pocket-fb: GL failed: {e}; using the CPU copy");
                drop(gl);
                fb.restore_pan();
                return None;
            }
            seen = fb.middle_pixel();
            if seen.is_none_or(|(r, g, b)| r.abs_diff(CHECK_COLOUR.0) <= 2 && g.abs_diff(CHECK_COLOUR.1) <= 2 && b.abs_diff(CHECK_COLOUR.2) <= 2) {
                eprintln!("pocket-fb: GL on the screen: the visible page holds the picture, offset {}", fb.visible);
                return Some(gl);
            }
        }
        eprintln!("pocket-fb: GL failed: the visible page holds {seen:?}, drawn {CHECK_COLOUR:?}; using the CPU copy");
        drop(gl);
        fb.restore_pan();
        return None;
    }
    eprintln!("pocket-fb: GL on a {window} window, the screen is a file");
    Some(gl)
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args().skip(1))?;
    if let Some((socket, command)) = &args.ipc_send {
        return ipc_send(socket, command);
    }
    let mut fb = Fb::open(&args.fb, args.mode, args.vsync, args.bgr)?;
    if let Some(path) = &args.picture {
        return show_picture(&mut fb, path, args.hold, args.gl.then_some(args.gl_window.as_str()));
    }
    if let Some(text) = &args.message {
        return show_message(&mut fb, text, args.hold, args.gl.then_some(args.gl_window.as_str()));
    }
    let _ = SCREEN.set((fb.width, fb.height));
    let gl = if args.gl { start_gl(&mut fb, &args.gl_window, args.gl_trust) } else { None };
    let gl_to_file = fb.plain_file && gl.is_some();
    let wayland = listen(&args.wayland)?;
    let ipc = listen(&args.ipc)?;
    // SAFETY: the handler only stores into an atomic.
    unsafe {
        for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(s, on_signal as *const () as libc::sighandler_t);
        }
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    let picture = gl.as_ref().and_then(|gl| match brick_picture::Layer::new(gl.egl.another_gl()) {
        Ok(layer) => Some(layer),
        Err(e) => {
            eprintln!("pocket-fb: shaders and frames off, the renderer didn't compile: {e}");
            None
        }
    });
    let mut server = Server { fb, clients: HashMap::new(), focus: None, next_con: 100, next_client: 0, frames: 0, copy: Duration::ZERO, worst: Duration::ZERO, gl, gl_to_file, pace: Pace::default(), anim: None, next_page: None, cross_next: false, picture, picture_pid: None, shown_by_picture: false, ipc_pid: None, hud: None };
    let mut ipcs: Vec<(UnixStream, Vec<u8>)> = Vec::new();
    while !STOP.load(Ordering::Relaxed) {
        let mut polls = vec![libc::pollfd { fd: wayland.as_raw_fd(), events: libc::POLLIN, revents: 0 }, libc::pollfd { fd: ipc.as_raw_fd(), events: libc::POLLIN, revents: 0 }];
        let keys: Vec<u64> = server.clients.keys().copied().collect();
        for k in &keys {
            let c = &server.clients[k];
            let events = libc::POLLIN | if c.output.is_empty() { 0 } else { libc::POLLOUT };
            polls.push(libc::pollfd { fd: c.stream.as_raw_fd(), events, revents: 0 });
        }
        for (s, _) in &ipcs {
            polls.push(libc::pollfd { fd: s.as_raw_fd(), events: libc::POLLIN, revents: 0 });
        }
        // SAFETY: `polls` is a live array of that length.
        // While the GPU animates the loop doesn't wait: the swap in `animate` paces it.
        let n = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as _, if server.anim.is_some() { 0 } else { 1000 }) };
        if n <= 0 {
            server.animate();
            continue;
        }
        if polls[0].revents != 0 {
            while let Ok((s, _)) = wayland.accept() {
                server.next_client += 1;
                server.clients.insert(server.next_client, Client::new(s));
            }
        }
        // Those accepted below have no entry in `polls` yet.
        let polled_ipcs = ipcs.len();
        if polls[1].revents != 0 {
            while let Ok((s, _)) = ipc.accept() {
                s.set_nonblocking(true).ok();
                ipcs.push((s, Vec::new()));
            }
        }
        for (i, k) in keys.iter().enumerate() {
            let re = polls[2 + i].revents;
            if re == 0 {
                continue;
            }
            let Some(c) = server.clients.get_mut(k) else { continue };
            c.flush();
            if re & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                c.receive();
            }
            server.handle(*k);
        }
        let first_ipc = 2 + keys.len();
        let mut gone = Vec::new();
        for i in 0..polled_ipcs {
            if polls[first_ipc + i].revents != 0 {
                let (stream, input) = &mut ipcs[i];
                if !serve_ipc(&mut server, stream, input) {
                    gone.push(i);
                }
            }
        }
        for i in gone.into_iter().rev() {
            ipcs.remove(i);
        }
        server.drop_dead();
        server.animate();
    }
    if server.frames > 0 {
        let way = if server.gl.is_some() { "gl" } else { "copy" };
        eprintln!("pocket-fb: {} frames, {way} {:.2} ms each, worst {:.2} ms", server.frames, server.copy.as_secs_f64() * 1000.0 / server.frames as f64, server.worst.as_secs_f64() * 1000.0);
        let p = &server.pace;
        if p.gaps > 0 {
            eprintln!("pocket-fb: while moving: {} frames, {:.2} ms apart ({:.1} fps), late (over {} ms) {}, worst {:.2} ms", p.gaps, p.total.as_secs_f64() * 1000.0 / p.gaps as f64, p.gaps as f64 / p.total.as_secs_f64(), LATE.as_millis(), p.late, p.worst.as_secs_f64() * 1000.0);
        }
    }
    // The page we found shows again, with the picture as we found it (`Fb`'s drop).
    if server.gl.take().is_some() {
        server.fb.restore_pan();
    }
    let _ = std::fs::remove_file(&args.wayland);
    let _ = std::fs::remove_file(&args.ipc);
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("pocket-fb: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE_ISH: [u8; 4] = [0x10, 0x20, 0x30, 0x00];

    #[test]
    fn the_usual_32_bit_layout_only_gets_opaque_alpha() {
        let mut d = [0u8; 4];
        convert_row(Format::Bits32 { r: 16, g: 8, b: 0, alpha: 0xff00_0000 }, &BLUE_ISH, &mut d);
        assert_eq!(d, [0x10, 0x20, 0x30, 0xff]);
    }

    #[test]
    fn swapped_channels_land_where_the_framebuffer_says() {
        let mut d = [0u8; 4];
        convert_row(Format::Bits32 { r: 0, g: 8, b: 16, alpha: 0xff00_0000 }, &BLUE_ISH, &mut d);
        assert_eq!(d, [0x30, 0x20, 0x10, 0xff]);
    }

    #[test]
    fn sixteen_bits_keep_the_top_bits_of_each_channel() {
        let mut d = [0u8; 2];
        convert_row(Format::Rgb565, &[0xff, 0x00, 0xff, 0x00], &mut d);
        assert_eq!(u16::from_le_bytes(d), 0xf81f);
    }

    #[test]
    fn a_framebuffer_without_alpha_stays_without() {
        let v = VarInfo { bits_per_pixel: 32, red: Bitfield { offset: 16, length: 8, msb_right: 0 }, green: Bitfield { offset: 8, length: 8, msb_right: 0 }, blue: Bitfield { offset: 0, length: 8, msb_right: 0 }, ..VarInfo::default() };
        assert_eq!(Format::from_var(&v), Some(Format::Bits32 { r: 16, g: 8, b: 0, alpha: 0 }));
    }

    #[test]
    fn the_kernel_structs_have_the_kernels_sizes() {
        assert_eq!(std::mem::size_of::<VarInfo>(), 160);
        assert_eq!(std::mem::offset_of!(FixInfo, line_length), 48);
    }

    #[test]
    fn control_rs_commands_split_and_parse() {
        let line = r#"[app_id="^(?!pocket-ui$).*"] fullscreen disable; [class=".*"] fullscreen disable; [app_id="pocket-ui"] focus; [app_id="pocket-ui"] fullscreen enable"#;
        let parts = split_commands(line);
        assert_eq!(parts.len(), 4);
        assert_eq!(parse_command(&parts[2]), (Some(r#"app_id="pocket-ui""#.to_string()), "focus".to_string()));
        let (c, a) = parse_command(r#"for_window [app_id="^(?!pocket-ui$).*"] fullscreen enable, focus"#);
        assert_eq!(c, None);
        assert!(a.starts_with("for_window"));
    }

    #[test]
    fn criteria_as_control_rs_writes_them() {
        assert!(criteria_match(r#"app_id="pocket-ui""#, 7, "pocket-ui"));
        assert!(!criteria_match(r#"app_id="pocket-ui""#, 7, "pocket-core"));
        assert!(criteria_match(r#"app_id="^(?!pocket-ui$).*""#, 7, "pocket-core"));
        assert!(!criteria_match(r#"app_id="^(?!pocket-ui$).*""#, 7, "pocket-ui"));
        assert!(criteria_match("con_id=7", 7, "x"));
        assert!(!criteria_match("con_id=8", 7, "x"));
        assert!(!criteria_match(r#"class=".*""#, 7, "x"));
    }
}
