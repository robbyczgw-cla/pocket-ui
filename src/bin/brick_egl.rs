//! brick-egl: the Brick's GPU question, on its own (docs/TRIMUI-BRICK.md, "The GPU"). EGL through the system's libEGL with
//! the window the blob wants, GL_VERSION, a `#version 300 es` shader, and frame times.
//!
//!   brick-egl [--window null|fbdev|pbuffer] [--seconds 10] [--size 1024x768]
//!   brick-egl --probe [--window null] [--size 1024x768]                       one black frame and out: exit 0 if GL can draw here
//!   brick-egl --shaders [--frames 120] [--window null] [--size 1024x768]    pocket-core's picture effects, frame times per variant
//!   brick-egl --panel [--seconds 20] [--window null] [--size 1024x768] [--fb /dev/fb0] [--readback file.raw]
//!
//! `null`: no native window (PVR's NULL WSEGL takes that). `fbdev`: `{ u16 width, u16 height }`, what Knulli's SDL "mali"
//! driver hands over. `pbuffer`: off screen, for a computer. Loads everything by dlopen, each step says how it went.
//! `--panel` is the moving test for a person to look at (`panel`); with `--fb` it also reads what is on fb0's visible page.
//! Exit: 0 fine, 3 no GLES 3 but GLES 2 works, 21 the visible page didn't hold the picture, other = where it stopped.
//! Only behind the `brick` feature.

use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use glow::HasContext;
use pocket_ui::brick_gl::{self, Egl, Presenter};

fn fail(step: &str, code: i32, detail: impl std::fmt::Display) -> ! {
    println!("brick-egl: FAILED at {step}: {detail}");
    std::process::exit(code);
}

fn say(line: String) {
    // A name and its value as they are, a step with the program's name. As it comes: a crash still shows how far it got.
    if line.contains(':') && !line.starts_with("no ") {
        println!("{line}");
    } else {
        println!("brick-egl: {line}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str);
    let flag = |name: &str| args.iter().any(|a| a == name);
    let window = arg("--window").unwrap_or("null");
    let seconds: u64 = arg("--seconds").and_then(|s| s.parse().ok()).unwrap_or(if flag("--panel") { 20 } else { 10 });
    let (w, h): (i32, i32) = arg("--size").and_then(|s| s.split_once('x')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))).unwrap_or((1024, 768));
    if flag("--probe") {
        probe(window, (w, h));
        return;
    }
    if flag("--shaders") {
        shaders(window, (w, h), arg("--frames").and_then(|s| s.parse().ok()).unwrap_or(120));
        return;
    }
    if flag("--panel") {
        panel(window, (w, h), seconds, arg("--fb"), arg("--readback"));
        return;
    }
    let egl = Egl::open(window, (w, h), &mut say).unwrap_or_else(|f| fail(&f.step, f.code, &f.detail));
    triangle_run(&egl, (w, h), seconds);
    let es3 = egl.es3;
    drop(egl);
    if !es3 {
        println!("brick-egl: GLES 2 only");
        std::process::exit(3);
    }
    println!("brick-egl: done");
}

/// What the shaders cost on this GPU: the renderer `pocket-fb` runs for games (`brick_picture::Layer`), every sharpness and effect and
/// the screen frames, over a GBA-sized and a SNES-sized picture, each drawn `frames` times with the GPU finished after every draw (no swap:
/// the numbers are the GPU's work for the picture alone, not the screen's rate). One line each: `shader: <scene> <settings> <mean> ms, worst <w> ms`.
/// Run it with nothing else on the screen (`start.sh` stages do); on a computer `--window pbuffer` gives the rasteriser's times, which
/// compare the variants with each other and say nothing about the Brick.
fn shaders(window: &str, screen: (i32, i32), frames: usize) {
    use pocket_ui::brick_picture::{Layer, Request};
    let egl = Egl::open(window, screen, &mut say).unwrap_or_else(|f| fail(&f.step, f.code, &f.detail));
    let mut layer = Layer::new(egl.another_gl()).unwrap_or_else(|e| fail("renderer", 30, e));
    let dir = std::env::temp_dir().join("brick-egl-no-frame-packs");
    let scenes: [(&str, &str, u32, u32, f32); 2] = [("gba", "gba", 240, 160, 1.5), ("snes", "snes", 256, 224, 4.0 / 3.0)];
    let mut cases: Vec<Vec<(&str, &str)>> = Vec::new();
    for sharpness in ["sharp", "crisp", "soft"] {
        for effect in ["none", "grid", "scanlines", "crt"] {
            cases.push(vec![("sharpness", sharpness), ("effect", effect)]);
        }
    }
    cases.push(vec![("effect", "grid"), ("lcd_colour", "on")]);
    for frame in ["auto", "ambient", "shell", "brick", "crt", "arcade", "glow"] {
        cases.push(vec![("scaling", "integer-smaller"), ("frame", frame)]);
    }
    for (name, system, w, h, aspect) in scenes {
        let pixels: Vec<u8> = (0..w * h).flat_map(|i| { let (x, y) = (i % w, i / w); [(x * 255 / w) as u8, (y * 255 / h) as u8, ((x / 8 + y / 8) % 2 * 200) as u8, 0] }).collect();
        for pairs in &cases {
            let values = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            layer.set(Request { system: system.into(), aspect, turns: 0, frames_dir: dir.clone(), values });
            let label = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ");
            // A frame is prepared on a thread: draw until it is there, so its cost is the steady one.
            let begin = Instant::now();
            for _ in 0..600 {
                if layer.draw(&pixels, w, h, w as usize * 4, (screen.0 as u32, screen.1 as u32)).is_err() {
                    fail("draw", 31, &label);
                }
                if !layer.frame_pending() || begin.elapsed() > Duration::from_secs(8) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut times = Vec::with_capacity(frames);
            for _ in 0..frames {
                let t = Instant::now();
                let _ = layer.draw(&pixels, w, h, w as usize * 4, (screen.0 as u32, screen.1 as u32));
                // SAFETY: the context of `egl` is current on this thread.
                unsafe { egl.gl.finish() };
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let mean = times.iter().sum::<f64>() / times.len().max(1) as f64;
            let worst = times.iter().copied().fold(0.0, f64::max);
            println!("shader: {name} {label}: {mean:.2} ms, worst {worst:.2} ms");
        }
    }
    println!("brick-egl: shaders done");
}

/// `start.sh` asks this first, in a process of its own: a blob that crashes or hangs here costs a line in a log, not the screen.
fn probe(window: &str, screen: (i32, i32)) {
    let found = FoundPan::read("/dev/fb0");
    let mut p = Presenter::new(window, screen, &mut say).unwrap_or_else(|f| fail(&f.step, f.code, &f.detail));
    if let Err(e) = p.flood((0, 0, 0)) {
        fail("first frame", 20, e);
    }
    drop(p);
    found.put_back();
    println!("brick-egl: probe ok");
}

/// The page fb0 showed before the GPU flipped it, shown again at the end (2026-11-03: the panel test's last picture stayed on screen
/// because the pan stayed where its last flip left it).
struct FoundPan(Option<(std::fs::File, [u32; 40])>);

const FBIOPAN_DISPLAY: libc::c_ulong = 0x4606;

impl FoundPan {
    fn read(path: &str) -> FoundPan {
        let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(path) else { return FoundPan(None) };
        let mut var = [0u32; 40];
        // SAFETY: FBIOGET_VSCREENINFO writes the 160 bytes of `fb_var_screeninfo`.
        let ok = unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO as _, var.as_mut_ptr()) } == 0;
        FoundPan(ok.then_some((file, var)))
    }

    fn put_back(self) {
        let Some((file, mut var)) = self.0 else { return };
        // SAFETY: the kernel's own struct from FBIOGET_VSCREENINFO, read back by FBIOPAN_DISPLAY.
        if unsafe { libc::ioctl(file.as_raw_fd(), FBIOPAN_DISPLAY as _, var.as_mut_ptr()) } == 0 {
            println!("brick-egl: fb0 shows row {} again, as before", var[5]);
        } else {
            println!("brick-egl: fb0's page {} couldn't be put back: {}", var[5], std::io::Error::last_os_error());
        }
    }
}

fn triangle_run(egl: &Egl, (w, h): (i32, i32), seconds: u64) {
    let gl = &egl.gl;
    // SAFETY: the context of `egl` is current on this thread.
    unsafe {
        let shaders_300 = brick_gl::program(gl, "#version 300 es\nin vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }", "#version 300 es\nprecision mediump float; uniform float t; out vec4 c; void main() { c = vec4(t, 0.3, 1.0 - t, 1.0); }");
        println!("brick-egl: #version 300 es shaders: {}", if shaders_300.is_ok() { "ok" } else { "no" });
        let program = shaders_300.or_else(|_| brick_gl::program(gl, "attribute vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }", "precision mediump float; uniform float t; void main() { gl_FragColor = vec4(t, 0.3, 1.0 - t, 1.0); }"));
        let program = program.unwrap_or_else(|e| fail("shaders", 18, e));
        gl.use_program(Some(program));
        let t = gl.get_uniform_location(program, "t");
        let at = gl.get_attrib_location(program, "p").unwrap_or(0);
        let vbo = gl.create_buffer().unwrap_or_else(|e| fail("buffer", 19, e));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let points: [f32; 6] = [-0.8, -0.8, 0.8, -0.8, 0.0, 0.8];
        let bytes: Vec<u8> = points.iter().flat_map(|f| f.to_le_bytes()).collect();
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(at);
        gl.vertex_attrib_pointer_f32(at, 2, glow::FLOAT, false, 0, 0);
        gl.viewport(0, 0, w, h);

        let start = Instant::now();
        let mut intervals = Vec::new();
        let mut last = Instant::now();
        while start.elapsed() < Duration::from_secs(seconds) {
            let phase = (start.elapsed().as_secs_f32() * 2.0).sin() * 0.5 + 0.5;
            gl.clear_color(0.05, 0.05, 0.1 + 0.2 * phase, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.uniform_1_f32(t.as_ref(), phase);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            if let Err(e) = egl.swap() {
                fail("eglSwapBuffers", 20, e);
            }
            let now = Instant::now();
            intervals.push((now - last).as_secs_f32() * 1000.0);
            last = now;
        }
        report("frames", &mut intervals);
    }
}

// The four corners and the background of the moving test, so a look at the panel and the read-back check the same thing.
const RED: (u8, u8, u8) = (255, 0, 0);
const GREEN: (u8, u8, u8) = (0, 255, 0);
const BLUE: (u8, u8, u8) = (0, 0, 255);
const WHITE: (u8, u8, u8) = (255, 255, 255);
const BACKGROUND: (u8, u8, u8) = (16, 32, 64);
const CORNER: i32 = 96;

/// A rectangle in screen coordinates (top left origin), by scissor: the GPU fills it, nothing on the CPU.
unsafe fn rect(gl: &glow::Context, h: i32, (x, y, rw, rh): (i32, i32, i32, i32), (r, g, b): (u8, u8, u8)) {
    gl.enable(glow::SCISSOR_TEST);
    gl.scissor(x, h - y - rh, rw, rh);
    gl.clear_color(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0);
    gl.clear(glow::COLOR_BUFFER_BIT);
    gl.disable(glow::SCISSOR_TEST);
}

/// fb0 as the kernel says it is, read only: which page shows, and that page's pixels.
struct Fb0 {
    file: std::fs::File,
    stride: usize,
}

const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;

impl Fb0 {
    fn open(path: &str, width: usize) -> Option<Fb0> {
        let file = std::fs::File::open(path).map_err(|e| println!("brick-egl: {path}: {e}")).ok()?;
        let stride = std::fs::read_to_string("/sys/class/graphics/fb0/stride").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(width * 4);
        Some(Fb0 { file, stride })
    }

    /// `fb_var_screeninfo.yoffset`: the visible page's first row.
    fn yoffset(&self) -> Option<u32> {
        // `fb_var_screeninfo` is 160 bytes, all u32 where we read; yoffset is the sixth field.
        let mut var = [0u32; 40];
        // SAFETY: the ioctl writes at most those 160 bytes.
        (unsafe { libc::ioctl(self.file.as_raw_fd(), FBIOGET_VSCREENINFO as _, var.as_mut_ptr()) } == 0).then_some(var[5])
    }

    fn page(&self, height: usize) -> Option<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        let mut bytes = vec![0u8; self.stride * height];
        self.file.read_exact_at(&mut bytes, u64::from(self.yoffset()?) * self.stride as u64).ok()?;
        Some(bytes)
    }
}

/// What the display engine scans out, from the driver (`/sys/class/disp/disp/attr/sys`): the first row of the layer on show, its `crop`
/// (read 2026-11-02: `crop[0,768,...]` with `fb0/pan` at 0,768, the `addr` stays). Beyond the kernel's pan this is the last word short of
/// a pair of eyes. None on a system without it.
fn scanout_row() -> Option<u32> {
    let text = std::fs::read_to_string("/sys/class/disp/disp/attr/sys").ok()?;
    let line = text.lines().find(|l| l.contains("enable") && l.contains("crop["))?;
    line.split("crop[").nth(1)?.split(',').nth(1)?.trim().parse().ok()
}

/// The four corners on a page of fb0 (B, G, R, A as read on the Brick, r16 g8 b0): those that differ, by name.
fn page_faults(page: &[u8], stride: usize, (w, h): (i32, i32)) -> Vec<String> {
    let at = |x: i32, y: i32| {
        let i = y as usize * stride + x as usize * 4;
        (page[i + 2], page[i + 1], page[i])
    };
    let close = |a: (u8, u8, u8), b: (u8, u8, u8)| a.0.abs_diff(b.0) <= 4 && a.1.abs_diff(b.1) <= 4 && a.2.abs_diff(b.2) <= 4;
    let mut faults = Vec::new();
    for (name, x, y, want) in [("top left", 8, 8, RED), ("top right", w - 9, 8, GREEN), ("bottom left", 8, h - 9, BLUE), ("bottom right", w - 9, h - 9, WHITE)] {
        let got = at(x, y);
        if !close(got, want) {
            faults.push(format!("{name}: {got:?}, wanted {want:?}"));
        }
    }
    faults
}

/// The test a person looks at. On the panel, for `seconds`, ending by itself:
/// four coloured corners (red top left, green top right, blue bottom left, white bottom right), a white bar sweeping from left to
/// right every 2 s, an orange triangle turning in the middle, a yellow bar along the bottom that fills up until the end. For the last
/// fifth the dark blue background turns into a moving violet picture, uploaded from the CPU every frame: what `pocket-fb --gl` does.
fn panel(window: &str, (w, h): (i32, i32), seconds: u64, fb: Option<&str>, readback: Option<&str>) {
    let found = fb.map(FoundPan::read);
    let mut p = Presenter::new(window, (w, h), &mut say).unwrap_or_else(|f| {
        fail(&f.step, f.code, &f.detail);
    });
    // SAFETY: the context of `p.egl` is current on this thread until the end of this function.
    let (spin, turn) = unsafe {
        let gl = &p.egl.gl;
        let spin = brick_gl::program(gl, "#version 300 es\nuniform vec2 turn; void main() { vec2 a = vec2(0.0, 0.3), b = vec2(-0.26, -0.15), c = vec2(0.26, -0.15); vec2 q = gl_VertexID == 0 ? a : gl_VertexID == 1 ? b : c; gl_Position = vec4((q.x * turn.y - q.y * turn.x) * 0.75, q.x * turn.x + q.y * turn.y, 0.0, 1.0); }", "#version 300 es\nprecision mediump float; out vec4 c; void main() { c = vec4(1.0, 0.55, 0.0, 1.0); }").unwrap_or_else(|e| fail("shaders", 18, e));
        (spin, gl.get_uniform_location(spin, "turn"))
    };
    println!("brick-egl: on the panel for {seconds} s");
    let fb0 = fb.and_then(|f| Fb0::open(f, w as usize));
    let total = Duration::from_secs(seconds);
    let uploads_from = total.mul_f32(0.8);
    // The second is the moving test, the fourth the uploaded picture; both are also kept as files (`--readback`, `.late`).
    let check_at = [total.mul_f32(0.25), total.mul_f32(0.5), total.mul_f32(0.75), total.mul_f32(0.9)];
    let mut checked = 0;
    let (mut gpu_frames, mut upload_frames) = (Vec::new(), Vec::new());
    let (mut upload_ms, mut draw_ms, mut swap_ms) = (Vec::new(), Vec::new(), Vec::new());
    let mut pans: Vec<u32> = Vec::new();
    let mut pan_changes = 0u32;
    let mut last_pan = None;
    let mut faults = Vec::new();
    let mut scanouts: Vec<(u32, u32)> = Vec::new();
    let mut picture = vec![0u8; w as usize * h as usize * 4];
    let start = Instant::now();
    let mut last = Instant::now();
    while start.elapsed() < total {
        let now = start.elapsed();
        let secs = now.as_secs_f32();
        let uploading = now >= uploads_from;
        let t = Instant::now();
        if uploading {
            // A picture new each frame: rows of violet shifting by the frame.
            let shift = (secs * 240.0) as usize;
            for (y, row) in picture.chunks_exact_mut(w as usize * 4).enumerate() {
                let v = ((y + shift) % 256) as u8;
                for px in row.chunks_exact_mut(4) {
                    px.copy_from_slice(&[0x60u8.saturating_add(v / 3), 0x10, 0x40u8.saturating_add(v / 3), 0]);
                }
            }
            if let Err(e) = p.draw(&picture, w as u32, h as u32, w as usize * 4) {
                fail("upload", 19, e);
            }
            upload_ms.push(t.elapsed().as_secs_f32() * 1000.0);
        }
        let t = Instant::now();
        // SAFETY: as above.
        unsafe {
            let gl = &p.egl.gl;
            if !uploading {
                rect(gl, h, (0, 0, w, h), BACKGROUND);
            }
            let bar = ((secs / 2.0).fract() * (w - 40) as f32) as i32;
            rect(gl, h, (bar, 0, 40, h), WHITE);
            rect(gl, h, (0, 0, CORNER, CORNER), RED);
            rect(gl, h, (w - CORNER, 0, CORNER, CORNER), GREEN);
            rect(gl, h, (0, h - CORNER, CORNER, CORNER), BLUE);
            rect(gl, h, (w - CORNER, h - CORNER, CORNER, CORNER), WHITE);
            let filled = (now.as_secs_f32() / total.as_secs_f32() * (w - 2 * CORNER) as f32) as i32;
            rect(gl, h, (CORNER, h - 48, filled, 32), (255, 220, 0));
            gl.use_program(Some(spin));
            gl.uniform_2_f32(turn.as_ref(), secs.sin(), secs.cos());
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        draw_ms.push(t.elapsed().as_secs_f32() * 1000.0);
        let t = Instant::now();
        if let Err(e) = p.egl.swap() {
            fail("eglSwapBuffers", 20, e);
        }
        swap_ms.push(t.elapsed().as_secs_f32() * 1000.0);
        let done = Instant::now();
        let took = (done - last).as_secs_f32() * 1000.0;
        if uploading {
            upload_frames.push(took);
        } else {
            gpu_frames.push(took);
        }
        last = done;
        if let Some(pan) = fb0.as_ref().and_then(Fb0::yoffset) {
            pan_changes += u32::from(last_pan.is_some_and(|l| l != pan));
            last_pan = Some(pan);
            if !pans.contains(&pan) {
                pans.push(pan);
            }
        }
        if checked < check_at.len() && now >= check_at[checked] {
            checked += 1;
            let t = Instant::now();
            // The visible page of fb0; on a computer, without one, what was drawn.
            let (page, stride, what) = match &fb0 {
                Some(f) => (f.page(h as usize), f.stride, format!("visible page at row {}", f.yoffset().unwrap_or(0))),
                None => (Some(p.read_xrgb()), w as usize * 4, "the surface".to_string()),
            };
            let read = t.elapsed().as_secs_f32() * 1000.0;
            // Pages and scan-out seen together: the engine's row moves with the pan if it shows what the GPU draws.
            if let (Some(a), Some(y)) = (scanout_row(), fb0.as_ref().and_then(Fb0::yoffset)) {
                println!("display engine {checked}: scans out from row {a}, visible page at row {y}");
                scanouts.push((a, y));
            }
            match page {
                Some(page) => {
                    let wrong = page_faults(&page, stride, (w, h));
                    println!("read-back {checked}: {what} read in {read:.1} ms: {}", if wrong.is_empty() { "the corners are right".to_string() } else { wrong.join("; ") });
                    faults.extend(wrong);
                    if let Some(path) = readback {
                        let file = match checked {
                            2 => Some(path.to_string()),
                            4 => Some(format!("{path}.late")),
                            _ => None,
                        };
                        if let Err(e) = file.map_or(Ok(()), |f| std::fs::write(f, &page)) {
                            println!("read-back: {path}: {e}");
                        }
                    }
                }
                None => println!("read-back {checked}: couldn't read the visible page"),
            }
            // The time of the read isn't the GPU's.
            last = Instant::now();
        }
    }
    let ran = start.elapsed().as_secs_f32();
    if !gpu_frames.is_empty() {
        report("frames, GPU only", &mut gpu_frames);
    }
    if !upload_frames.is_empty() {
        report("frames, picture uploaded each frame", &mut upload_frames);
    }
    median("upload + its draw", &mut upload_ms);
    median("draw", &mut draw_ms);
    median("swap (waits for the flip)", &mut swap_ms);
    // GPU to CPU: what a picture read back each frame would cost, the way to a shm window if the flip can't be had.
    let t = Instant::now();
    let _ = p.read_xrgb();
    println!("glReadPixels of the whole screen, with its conversion: {:.1} ms", t.elapsed().as_secs_f32() * 1000.0);
    println!("ran {ran:.1} s");
    if fb0.is_some() {
        println!("fb0 pages seen: {pans:?} (rows), {pan_changes} page changes: {}", if pan_changes > 0 { "a real flip" } else { "no flip seen" });
    }
    match scanouts.first() {
        Some(&(a0, _)) => {
            let same = scanouts.iter().filter(|&&(a, y)| a == y).count();
            let moved = scanouts.iter().any(|&(a, _)| a != a0);
            println!("display engine: {same} of {} samples on the visible page, {}", scanouts.len(), if moved { "the row moved" } else { "the row stayed" });
        }
        None => println!("display engine: no scan-out row readable"),
    }
    drop(p);
    if let Some(f) = found {
        f.put_back();
    }
    if !faults.is_empty() {
        println!("brick-egl: the visible page does not hold the picture");
        std::process::exit(21);
    }
    println!("brick-egl: done");
}

fn median(name: &str, v: &mut [f32]) {
    if v.is_empty() {
        return;
    }
    v.sort_by(f32::total_cmp);
    println!("{name}: median {:.2} ms, p99 {:.2}, max {:.2}", v[v.len() / 2], v[v.len() * 99 / 100], v[v.len() - 1]);
}

fn report(name: &str, v: &mut [f32]) {
    if v.len() < 2 {
        println!("{name}: {}", v.len());
        return;
    }
    // The first frame includes the set-up.
    let v = &mut v[1..];
    v.sort_by(f32::total_cmp);
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    println!("{name} {}  fps {:.2}  frame {:.2} ms  median {:.2}  p99 {:.2}  max {:.2}  late(>25ms) {}", v.len(), 1000.0 / mean, mean, v[v.len() / 2], v[v.len() * 99 / 100], v[v.len() - 1], v.iter().filter(|x| **x > 25.0).count());
}
