//! The browser demo's winit (docs/WEB-DEMO.md): only what pocket-ui's App uses of winit 0.30, for one window that is
//! the page's canvas. The App runs on its own thread (emscripten's PROXY_TO_PTHREAD); the page hands it input and the
//! canvas size through `web`, and `EventLoop::run_app` turns the loop as winit does on a device: events, redraws,
//! `about_to_wait`, then a sleep as the control flow says.

use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub mod dpi {
    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    pub struct PhysicalSize<P> {
        pub width: P,
        pub height: P,
    }
    impl<P> PhysicalSize<P> {
        pub const fn new(width: P, height: P) -> Self {
            PhysicalSize { width, height }
        }
    }
    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    pub struct PhysicalPosition<P> {
        pub x: P,
        pub y: P,
    }
    impl<P> PhysicalPosition<P> {
        pub const fn new(x: P, y: P) -> Self {
            PhysicalPosition { x, y }
        }
    }
}

pub mod keyboard {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum KeyCode {
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Enter, Backspace, Escape, Space, Tab,
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS,
        KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum NativeKeyCode {
        Unidentified,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum PhysicalKey {
        Code(KeyCode),
        Unidentified(NativeKeyCode),
    }
}

pub mod event {
    use crate::dpi::{PhysicalPosition, PhysicalSize};
    use crate::keyboard::PhysicalKey;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ElementState {
        Pressed,
        Released,
    }
    impl ElementState {
        pub fn is_pressed(self) -> bool {
            self == ElementState::Pressed
        }
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct DeviceId(pub u32);
    impl DeviceId {
        pub const fn dummy() -> Self {
            DeviceId(0)
        }
    }
    #[derive(Debug, Clone, PartialEq)]
    pub struct KeyEvent {
        pub physical_key: PhysicalKey,
        pub state: ElementState,
        pub repeat: bool,
        pub text: Option<String>,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum MouseButton {
        Left,
        Right,
        Middle,
    }
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum MouseScrollDelta {
        LineDelta(f32, f32),
        PixelDelta(PhysicalPosition<f64>),
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum TouchPhase {
        Started,
        Moved,
        Ended,
        Cancelled,
    }
    #[derive(Debug, Clone, PartialEq)]
    pub enum WindowEvent {
        CloseRequested,
        Destroyed,
        Focused(bool),
        Resized(PhysicalSize<u32>),
        RedrawRequested,
        KeyboardInput { device_id: DeviceId, event: KeyEvent, is_synthetic: bool },
        CursorMoved { device_id: DeviceId, position: PhysicalPosition<f64> },
        CursorLeft { device_id: DeviceId },
        MouseInput { device_id: DeviceId, state: ElementState, button: MouseButton },
        MouseWheel { device_id: DeviceId, delta: MouseScrollDelta, phase: TouchPhase },
    }
}

pub mod window {
    use crate::dpi::PhysicalSize;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct WindowId(pub(crate) u64);
    impl WindowId {
        pub const fn dummy() -> Self {
            WindowId(0)
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Fullscreen {
        Borderless(Option<()>),
    }
    #[derive(Debug, Clone, Default)]
    pub struct WindowAttributes {
        pub title: String,
    }
    impl WindowAttributes {
        pub fn with_title(mut self, title: impl Into<String>) -> Self {
            self.title = title.into();
            self
        }
        pub fn with_fullscreen(self, _: Option<Fullscreen>) -> Self {
            self
        }
        pub fn with_inner_size<S>(self, _: S) -> Self {
            self
        }
        pub fn with_visible(self, _: bool) -> Self {
            self
        }
        pub fn with_decorations(self, _: bool) -> Self {
            self
        }
    }
    /// The page's canvas. Only the first window is shown; another (a second screen) paints into nothing.
    #[derive(Debug)]
    pub struct Window {
        pub(crate) id: WindowId,
    }
    impl Window {
        pub fn default_attributes() -> WindowAttributes {
            WindowAttributes::default()
        }
        pub fn id(&self) -> WindowId {
            self.id
        }
        pub fn inner_size(&self) -> PhysicalSize<u32> {
            let (w, h) = crate::shared().lock().size;
            PhysicalSize::new(w, h)
        }
        pub fn outer_size(&self) -> PhysicalSize<u32> {
            self.inner_size()
        }
        pub fn scale_factor(&self) -> f64 {
            1.0
        }
        pub fn request_redraw(&self) {
            if self.id.0 == 1 {
                crate::shared().lock().redraw = true;
            }
        }
        pub fn set_visible(&self, _: bool) {}
        pub fn set_cursor_visible(&self, _: bool) {}
        pub fn set_fullscreen(&self, _: Option<Fullscreen>) {}
        pub fn set_title(&self, _: &str) {}
        pub fn set_minimized(&self, _: bool) {}
        pub fn focus_window(&self) {}
        pub fn pre_present_notify(&self) {}
        pub fn current_monitor(&self) -> Option<crate::monitor::MonitorHandle> {
            Some(crate::monitor::MonitorHandle)
        }
    }
    impl raw_window_handle::HasWindowHandle for Window {
        fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }
    impl raw_window_handle::HasDisplayHandle for Window {
        fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }
}

pub mod monitor {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MonitorHandle;
    impl MonitorHandle {
        pub fn refresh_rate_millihertz(&self) -> Option<u32> {
            Some(60_000)
        }
        pub fn name(&self) -> Option<String> {
            Some("browser".into())
        }
    }
}

pub mod error {
    #[derive(Debug)]
    pub struct OsError;
    impl std::fmt::Display for OsError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("no window in the browser demo")
        }
    }
    impl std::error::Error for OsError {}
    #[derive(Debug)]
    pub enum EventLoopError {
        NotSupported,
    }
    impl std::fmt::Display for EventLoopError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("event loop not supported")
        }
    }
    impl std::error::Error for EventLoopError {}
}

pub mod application {
    use crate::event::WindowEvent;
    use crate::event_loop::ActiveEventLoop;
    use crate::window::WindowId;

    pub trait ApplicationHandler<T: 'static = ()> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop);
        fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: T) {}
        fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent);
        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}
        fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}
    }
}

pub mod event_loop {
    use super::*;
    use crate::application::ApplicationHandler;
    use crate::error::{EventLoopError, OsError};
    use crate::event::WindowEvent;
    use crate::window::{Window, WindowAttributes, WindowId};
    use std::marker::PhantomData;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ControlFlow {
        Poll,
        #[default]
        Wait,
        WaitUntil(Instant),
    }

    pub struct EventLoopBuilder<T: 'static>(PhantomData<T>);
    impl<T: 'static> EventLoopBuilder<T> {
        pub fn build(&mut self) -> Result<EventLoop<T>, EventLoopError> {
            Ok(EventLoop(PhantomData))
        }
    }

    pub struct EventLoop<T: 'static>(PhantomData<T>);
    impl EventLoop<()> {
        pub fn new() -> Result<EventLoop<()>, EventLoopError> {
            Ok(EventLoop(PhantomData))
        }
    }
    impl<T: 'static> EventLoop<T> {
        pub fn with_user_event() -> EventLoopBuilder<T> {
            EventLoopBuilder(PhantomData)
        }
        pub fn create_proxy(&self) -> EventLoopProxy<T> {
            EventLoopProxy(PhantomData)
        }
        /// Turns until the App exits: what came (the App's own events, then the page's), a frame when one was asked
        /// for (at most one per refresh), `about_to_wait`, and a sleep until the control flow's time or the next event.
        pub fn run_app<A: ApplicationHandler<T>>(self, app: &mut A) -> Result<(), EventLoopError> {
            let el = ActiveEventLoop { _private: () };
            app.resumed(&el);
            let mut last_frame: Option<Instant> = None;
            loop {
                let (users, windows) = {
                    let mut s = shared().lock();
                    (std::mem::take(&mut s.user), std::mem::take(&mut s.window))
                };
                for ev in users {
                    if let Ok(ev) = ev.downcast::<T>() {
                        app.user_event(&el, *ev);
                    }
                }
                for ev in windows {
                    app.window_event(&el, WindowId(1), ev);
                }
                if shared().lock().exit {
                    break;
                }
                let now = Instant::now();
                let frame_due = last_frame.map_or(now, |t| t + REFRESH);
                let redraw = {
                    let mut s = shared().lock();
                    let due = s.redraw && now >= frame_due;
                    if due {
                        s.redraw = false;
                    }
                    due
                };
                if redraw {
                    last_frame = Some(now);
                    app.window_event(&el, WindowId(1), WindowEvent::RedrawRequested);
                    web::note_paint(now.elapsed());
                }
                app.about_to_wait(&el);
                let mut s = shared().lock();
                if s.exit {
                    break;
                }
                if !s.user.is_empty() || !s.window.is_empty() {
                    continue;
                }
                let wake = if s.redraw { Some(last_frame.map_or(now, |t| t + REFRESH)) } else { None };
                let wake = match (s.flow, wake) {
                    (ControlFlow::Poll, w) => Some(w.map_or(Instant::now(), |w| w.min(Instant::now()))),
                    (ControlFlow::Wait, w) => w,
                    (ControlFlow::WaitUntil(t), w) => Some(w.map_or(t, |w| w.min(t))),
                };
                match wake {
                    None => drop(SHARED.get().unwrap().wait(s)),
                    Some(at) => {
                        let left = at.saturating_duration_since(Instant::now());
                        if !left.is_zero() {
                            drop(SHARED.get().unwrap().wait_timeout(s, left));
                        }
                    }
                }
            }
            app.exiting(&el);
            Ok(())
        }
    }

    /// The shortest time between two frames. The App paces its own animations to the refresh (`frame_pace`); this only
    /// keeps a redraw asked for again at once from spinning (a second gate at 16.7 ms beat with the App's and halved it).
    const REFRESH: Duration = Duration::from_millis(4);

    pub struct ActiveEventLoop {
        _private: (),
    }
    impl ActiveEventLoop {
        pub fn create_window(&self, _attrs: WindowAttributes) -> Result<Window, OsError> {
            let mut s = shared().lock();
            s.windows += 1;
            Ok(Window { id: WindowId(s.windows) })
        }
        pub fn exit(&self) {
            shared().lock().exit = true;
        }
        pub fn exiting(&self) -> bool {
            shared().lock().exit
        }
        pub fn set_control_flow(&self, flow: ControlFlow) {
            shared().lock().flow = flow;
        }
        pub fn control_flow(&self) -> ControlFlow {
            shared().lock().flow
        }
        pub fn create_proxy<T: 'static>(&self) -> EventLoopProxy<T> {
            EventLoopProxy(PhantomData)
        }
    }

    pub struct EventLoopProxy<T: 'static>(PhantomData<fn() -> T>);
    impl<T: 'static> Clone for EventLoopProxy<T> {
        fn clone(&self) -> Self {
            EventLoopProxy(PhantomData)
        }
    }
    impl<T: 'static> std::fmt::Debug for EventLoopProxy<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("EventLoopProxy")
        }
    }
    impl<T: 'static> EventLoopProxy<T> {
        pub(crate) fn new() -> Self {
            EventLoopProxy(PhantomData)
        }
    }
    impl<T: Send + 'static> EventLoopProxy<T> {
        pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
            let mut s = shared().lock();
            if s.exit {
                return Err(EventLoopClosed(event));
            }
            s.user.push_back(Box::new(event));
            drop(s);
            SHARED.get().unwrap().cv.notify_all();
            Ok(())
        }
    }
    #[derive(Debug)]
    pub struct EventLoopClosed<T>(pub T);
    impl<T> std::fmt::Display for EventLoopClosed<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("event loop closed")
        }
    }
}

pub(crate) struct State {
    user: VecDeque<Box<dyn Any + Send>>,
    window: VecDeque<event::WindowEvent>,
    redraw: bool,
    exit: bool,
    flow: event_loop::ControlFlow,
    size: (u32, u32),
    windows: u64,
}

pub(crate) struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn wait<'a>(&self, g: std::sync::MutexGuard<'a, State>) -> std::sync::MutexGuard<'a, State> {
        self.cv.wait(g).unwrap_or_else(|p| p.into_inner())
    }
    fn wait_timeout<'a>(&self, g: std::sync::MutexGuard<'a, State>, d: Duration) -> std::sync::MutexGuard<'a, State> {
        self.cv.wait_timeout(g, d).map_or_else(|p| p.into_inner().0, |r| r.0)
    }
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

pub(crate) fn shared() -> &'static Shared {
    SHARED.get_or_init(|| {
        Arc::new(Shared {
            state: Mutex::new(State {
                user: VecDeque::new(),
                window: VecDeque::new(),
                redraw: false,
                exit: false,
                flow: event_loop::ControlFlow::Wait,
                size: (640, 480),
                windows: 0,
            }),
            cv: Condvar::new(),
        })
    })
}

/// The page's side: the canvas size and input as window events. Any thread.
pub mod web {
    use super::*;

    /// The size the App paints at (the canvas's pixels); a resize redraws.
    pub fn set_size(w: u32, h: u32) {
        let mut s = shared().lock();
        if s.size != (w, h) {
            s.size = (w, h);
            s.window.push_back(event::WindowEvent::Resized(dpi::PhysicalSize::new(w, h)));
        }
        drop(s);
        shared().cv.notify_all();
    }

    pub fn size() -> (u32, u32) {
        shared().lock().size
    }

    pub fn push(ev: event::WindowEvent) {
        shared().lock().window.push_back(ev);
        shared().cv.notify_all();
    }

    /// A proxy to the one event loop, for the page's input (`EventLoop::create_proxy` hands out the same).
    pub fn proxy<T: Send + 'static>() -> event_loop::EventLoopProxy<T> {
        event_loop::EventLoopProxy::new()
    }

    static PAINT: Mutex<(u32, Duration, Duration)> = Mutex::new((0, Duration::ZERO, Duration::ZERO));

    pub(crate) fn note_paint(d: Duration) {
        let mut p = PAINT.lock().unwrap_or_else(|p| p.into_inner());
        p.0 += 1;
        p.1 += d;
        p.2 = p.2.max(d);
    }

    /// Frames painted since the last call, their summed and their longest painting time (the App's whole redraw).
    pub fn take_paint_stats() -> (u32, Duration, Duration) {
        std::mem::take(&mut *PAINT.lock().unwrap_or_else(|p| p.into_inner()))
    }

    /// The App wants a frame (the page became visible again, say).
    pub fn request_redraw() {
        shared().lock().redraw = true;
        shared().cv.notify_all();
    }
}
