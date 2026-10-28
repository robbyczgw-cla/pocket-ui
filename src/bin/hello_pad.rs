//! M2 "hello pad": fullscreen Wayland client that shows every button and stick live.
//! Exit by holding SELECT + START for one second (Esc on the desktop preview).

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::gfx::{Canvas, ACCENT, ACTIVE, BG, DIM, FG, PANEL};
use pocket_ui::pad::{Button, PadEvent, PadState};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

const EXIT_HOLD: Duration = Duration::from_secs(1);

struct App {
    started: Instant,
    first_frame: Option<Duration>,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    pad: PadState,
    exit_since: Option<Instant>,
    last_input: Option<Instant>,
    input_to_frame: Option<Duration>,
}

impl App {
    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn update_exit_combo(&mut self) {
        let held = self.pad.is_pressed(Button::Select) && self.pad.is_pressed(Button::Start);
        self.exit_since = match (held, self.exit_since) {
            (true, None) => Some(Instant::now()),
            (true, since) => since,
            (false, _) => None,
        };
    }

    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        surface.resize(w, h).expect("resize surface");
        let mut buffer = surface.buffer_mut().expect("buffer");
        let mut c = Canvas::new(&mut buffer, w.get(), h.get());
        let stats = Stats { first_frame: self.first_frame, input_to_frame: self.input_to_frame, exit_since: self.exit_since };
        draw_screen(&mut c, &self.pad, &stats);
        buffer.present().expect("present");

        let now = Instant::now();
        if self.first_frame.is_none() {
            self.first_frame = Some(now - self.started);
            println!("first frame after {:?}", now - self.started);
        }
        if let Some(t) = self.last_input.take() {
            self.input_to_frame = Some(now - t);
        }
    }
}

struct Stats {
    first_frame: Option<Duration>,
    input_to_frame: Option<Duration>,
    exit_since: Option<Instant>,
}

fn draw_screen(c: &mut Canvas, pad: &PadState, app: &Stats) {
    c.clear(BG);
    // Layout is designed for 1240x1080 and scaled to whatever sway gives us.
    let (cw, ch) = (c.w, c.h);
    let sx = move |x: i32| x * cw / 1240;
    let sy = move |y: i32| y * ch / 1080;
    let on = |b: Button| if pad.is_pressed(b) { ACTIVE } else { PANEL };

    c.text(sx(60), sy(48), 5, ACCENT, "pocket-ui");
    c.text(sx(60) + 9 * 40 + 24, sy(64), 3, DIM, "hello pad");
    let stats = format!(
        "first frame {} ms   input->frame {}   events {}",
        app.first_frame.map_or("-".into(), |d| d.as_millis().to_string()),
        app.input_to_frame.map_or("-".into(), |d| format!("{:.1} ms", d.as_secs_f32() * 1000.0)),
        pad.events,
    );
    c.text(sx(60), sy(110), 2, DIM, &stats);

    // Shoulders and analog triggers.
    let bar = |c: &mut Canvas, x: i32, label: &str, v: f32, b: Button| {
        c.rect(sx(x), sy(170), sx(200), sy(28), PANEL);
        c.rect(sx(x), sy(170), (sx(200) as f32 * v.clamp(0.0, 1.0)) as i32, sy(28), ACTIVE);
        c.frame(sx(x), sy(170), sx(200), sy(28), 2, if pad.is_pressed(b) { ACTIVE } else { DIM });
        c.text(sx(x), sy(206), 2, FG, label);
    };
    bar(c, 60, "L2", pad.l2, Button::L2);
    bar(c, 980, "R2", pad.r2, Button::R2);
    c.rect(sx(60), sy(250), sx(200), sy(50), on(Button::L1));
    c.text_centered(sx(160), sy(266), 2, FG, "L1");
    c.rect(sx(980), sy(250), sx(200), sy(50), on(Button::R1));
    c.text_centered(sx(1080), sy(266), 2, FG, "R1");

    // D-pad.
    let (dx, dy, k) = (sx(250), sy(470), sx(70));
    for (b, ox, oy) in [(Button::Up, 0, -1), (Button::Down, 0, 1), (Button::Left, -1, 0), (Button::Right, 1, 0)] {
        c.rect(dx + ox * k - k / 2, dy + oy * k - k / 2, k, k, on(b));
    }
    c.rect(dx - k / 2, dy - k / 2, k, k, PANEL);

    // Face buttons, labelled by evdev position (the Retroid prints A on EAST, B on SOUTH).
    let (fx, fy, r) = (sx(990), sy(470), sx(46));
    for (b, ox, oy, label) in [
        (Button::North, 0, -1, "X"),
        (Button::South, 0, 1, "B"),
        (Button::West, -1, 0, "Y"),
        (Button::East, 1, 0, "A"),
    ] {
        let (cx, cy) = (fx + ox * sx(84), fy + oy * sy(84));
        c.disc(cx, cy, r, on(b));
        c.text_centered(cx, cy - 12, 3, FG, label);
    }

    // Center buttons.
    for (b, x) in [(Button::Select, 470), (Button::Mode, 620), (Button::Start, 770)] {
        c.rect(sx(x) - sx(60), sy(420), sx(120), sy(44), on(b));
        c.text_centered(sx(x), sy(434), 2, FG, b.label());
    }
    for (b, x) in [(Button::Back, 470), (Button::VolDown, 620), (Button::VolUp, 770)] {
        c.rect(sx(x) - sx(60), sy(490), sx(120), sy(44), on(b));
        c.text_centered(sx(x), sy(504), 2, FG, b.label());
    }

    // Sticks.
    for (center, (ax, ay), click) in [(sx(450), pad.left, Button::L3), (sx(790), pad.right, Button::R3)] {
        let (cy, rr) = (sy(700), sx(90));
        if pad.is_pressed(click) {
            c.disc(center, cy, rr, ACTIVE);
        }
        c.ring(center, cy, rr, 4, DIM);
        c.disc(center + (ax * rr as f32) as i32, cy + (ay * rr as f32) as i32, sx(22), FG);
        c.text_centered(center, cy + rr + 14, 2, DIM, &format!("{:+.2} {:+.2}", ax, ay));
    }

    // Power key is shown but deliberately not acted on.
    c.rect(sx(60), sy(620), sx(200), sy(44), on(Button::Power));
    c.text_centered(sx(160), sy(634), 2, FG, "POWER");

    // Event log and input sources.
    let log_y = sy(840);
    c.text(sx(60), log_y, 2, DIM, "events");
    // Both columns are clipped to their width (16 px per character at scale 2).
    let fit = |s: &str, px: i32| s.chars().take((px / 16).max(0) as usize).collect::<String>();
    for (i, (at, line)) in pad.log.iter().take(7).enumerate() {
        let age = at.elapsed().as_secs_f32();
        let color = if age < 0.5 { FG } else { DIM };
        c.text(sx(60), log_y + 26 + i as i32 * 22, 2, color, &fit(line, sx(620)));
    }
    c.text(sx(700), log_y, 2, DIM, "sources");
    for (i, s) in pad.sources.iter().enumerate().take(7) {
        c.text(sx(700), log_y + 26 + i as i32 * 22, 2, FG, &fit(&format!("{i} {s}"), sx(1240 - 700 - 20)));
    }

    // Exit hint and hold progress.
    let hint = "hold SELECT + START to exit";
    c.text_centered(c.w / 2, c.h - sy(40), 2, DIM, hint);
    if let Some(since) = app.exit_since {
        let p = (since.elapsed().as_secs_f32() / EXIT_HOLD.as_secs_f32()).min(1.0);
        c.rect(0, c.h - 6, (c.w as f32 * p) as i32, 6, ACTIVE);
    }
}

impl ApplicationHandler<PadEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("pocket-ui hello-pad")
            .with_fullscreen(Some(Fullscreen::Borderless(None)));
        let window = Rc::new(el.create_window(attrs).expect("create window"));
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface"));
        self.window = Some(window);
        self.redraw();
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, ev: PadEvent) {
        self.pad.apply(&ev);
        self.last_input.get_or_insert_with(Instant::now);
        self.update_exit_combo();
        self.redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, repeat: false, .. },
                ..
            } => {
                if code == KeyCode::Escape {
                    el.exit();
                    return;
                }
                if let Some(button) = desktop_key(code) {
                    self.user_event(el, PadEvent::Button { source: 0, button, pressed: state == ElementState::Pressed });
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        match self.exit_since {
            Some(since) if since.elapsed() >= EXIT_HOLD => el.exit(),
            Some(_) => {
                self.redraw();
                el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));
            }
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// Keyboard stand-in for the pad on the desktop preview.
fn desktop_key(code: KeyCode) -> Option<Button> {
    Some(match code {
        KeyCode::ArrowUp => Button::Up,
        KeyCode::ArrowDown => Button::Down,
        KeyCode::ArrowLeft => Button::Left,
        KeyCode::ArrowRight => Button::Right,
        KeyCode::KeyK => Button::South,
        KeyCode::KeyL => Button::East,
        KeyCode::KeyI => Button::North,
        KeyCode::KeyJ => Button::West,
        KeyCode::KeyQ => Button::L1,
        KeyCode::KeyE => Button::R1,
        KeyCode::Backspace => Button::Select,
        KeyCode::Enter => Button::Start,
        KeyCode::KeyH => Button::Mode,
        _ => return None,
    })
}

fn main() {
    let started = Instant::now();
    let event_loop = EventLoop::<PadEvent>::with_user_event().build().expect("event loop");

    #[cfg(target_os = "linux")]
    let sources = {
        let devices = pocket_ui::pad::linux::open();
        let names = devices.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        for (i, (_, dev)) in devices.into_iter().enumerate() {
            let proxy = event_loop.create_proxy();
            std::thread::spawn(move || pocket_ui::pad::linux::pump(i, dev, move |e| proxy.send_event(e).is_ok()));
        }
        names
    };
    #[cfg(not(target_os = "linux"))]
    let sources = vec!["keyboard (desktop preview)".to_string()];

    for s in &sources {
        println!("input source: {s}");
    }

    let mut app = App {
        started,
        first_frame: None,
        window: None,
        surface: None,
        pad: PadState::new(sources),
        exit_since: None,
        last_input: None,
        input_to_frame: None,
    };
    event_loop.run_app(&mut app).expect("run app");
}
