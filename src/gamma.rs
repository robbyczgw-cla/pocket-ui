//! The Gamma backend of the display look (see `display.rs`): a Wayland client of sway's
//! `wlr-gamma-control-unstable-v1`. The compositor keeps the ramps only while this connection lives,
//! so the `Gamma` value has to stay alive as long as the look should stay (pocket-ui lives for the whole
//! session, games included).
//!
//! `connect()` binds the manager and asks every output for a control. A compositor whose display has no
//! gamma LUT (sway headless, probably the Mini V2's msm driver) answers `failed` or a size of 0: then
//! `connect()` returns an error and the Soft backend is used.

use std::os::fd::{AsFd, FromRawFd};
use std::sync::{Arc, Mutex};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_output::WlOutput, wl_registry::WlRegistry};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::gamma_control::v1::client::zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1;
use wayland_protocols_wlr::gamma_control::v1::client::zwlr_gamma_control_v1::{self, ZwlrGammaControlV1};

use crate::display::Look;

struct Ctl {
    proxy: ZwlrGammaControlV1,
    size: u32,
    failed: bool,
    /// Kept so the output outlives its control.
    _output: WlOutput,
}

type Controls = Arc<Mutex<Vec<Ctl>>>;

struct State {
    controls: Controls,
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as wayland_client::Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlOutput, ()> for State {
    fn event(_: &mut Self, _: &WlOutput, _: <WlOutput as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrGammaControlManagerV1,
        _: <ZwlrGammaControlManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, usize> for State {
    fn event(state: &mut Self, _: &ZwlrGammaControlV1, event: zwlr_gamma_control_v1::Event, idx: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let mut controls = state.controls.lock().unwrap();
        let Some(ctl) = controls.get_mut(*idx) else { return };
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => ctl.size = size,
            zwlr_gamma_control_v1::Event::Failed => ctl.failed = true,
            _ => {}
        }
    }
}

pub struct Gamma {
    conn: Connection,
    controls: Controls,
}

impl Gamma {
    /// Binds the gamma controls of all outputs; `Err` says why the compositor can't take gamma ramps.
    pub fn connect() -> Result<Gamma, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland connection: {e}"))?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
        let qh = queue.handle();
        let manager: ZwlrGammaControlManagerV1 = globals.bind(&qh, 1..=1, ()).map_err(|_| "the compositor has no gamma control".to_string())?;
        let controls: Controls = Arc::new(Mutex::new(Vec::new()));
        let mut state = State { controls: controls.clone() };
        let outputs: Vec<(u32, u32)> = globals
            .contents()
            .with_list(|l| l.iter().filter(|g| g.interface == "wl_output").map(|g| (g.name, g.version)).collect());
        if outputs.is_empty() {
            return Err("no outputs".into());
        }
        for (name, version) in outputs {
            let output: WlOutput = globals.registry().bind(name, version.min(1), &qh, ());
            let mut list = controls.lock().unwrap();
            let idx = list.len();
            let proxy = manager.get_gamma_control(&output, &qh, idx);
            list.push(Ctl {
                proxy,
                size: 0,
                failed: false,
                _output: output,
            });
        }
        queue.roundtrip(&mut state).map_err(|e| format!("Wayland: {e}"))?;
        {
            let list = controls.lock().unwrap();
            if list.iter().all(|c| c.failed || c.size == 0) {
                return Err("the display has no gamma table (the compositor answered failed)".into());
            }
        }
        // Events (a control failing later) keep arriving; this thread only drains them.
        std::thread::spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
        Ok(Gamma { conn, controls })
    }

    /// Entries per channel of the smallest usable ramp.
    pub fn size(&self) -> u32 {
        self.controls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| !c.failed && c.size > 0)
            .map(|c| c.size)
            .min()
            .unwrap_or(0)
    }

    /// Uploads the ramps for `look`. False if no output took them.
    pub fn apply(&self, look: &Look) -> bool {
        let mut ok = false;
        for ctl in self.controls.lock().unwrap().iter().filter(|c| !c.failed && c.size > 0) {
            let ramps = look.ramps(ctl.size as usize);
            let bytes: Vec<u8> = ramps.iter().flat_map(|v| v.to_ne_bytes()).collect();
            // SAFETY: memfd_create returns a new descriptor or -1; the File takes ownership of it.
            let fd = unsafe { libc::memfd_create(c"pocket-ui-gamma".as_ptr(), libc::MFD_CLOEXEC) };
            if fd < 0 {
                continue;
            }
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            use std::io::{Seek, Write};
            let mut f = &file;
            if f.write_all(&bytes).is_err() || f.rewind().is_err() {
                continue;
            }
            ctl.proxy.set_gamma(file.as_fd());
            ok = true;
        }
        let _ = self.conn.flush();
        ok
    }
}
