//! Physical controller layouts, keyed by controller identity, shared by shelf and pocket-core.
use super::{Button, PadEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

pub const FILE: &str = "controllers.json";
pub const BUTTONS: [Button; 16] = [Button::South, Button::East, Button::North, Button::West,
    Button::L1, Button::R1, Button::L2, Button::R2, Button::L3, Button::R3,
    Button::Select, Button::Start, Button::Up, Button::Down, Button::Left, Button::Right];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout { roles: [Button; 16] }

impl Default for Layout {
    fn default() -> Self { Self { roles: BUTTONS } }
}

impl Layout {
    fn valid(&self) -> bool { BUTTONS.iter().all(|b| self.roles.iter().filter(|r| *r == b).count() == 1) }
    pub fn role(&self, physical: Button) -> Button {
        BUTTONS.iter().position(|b| *b == physical).map(|i| self.roles[i]).unwrap_or(physical)
    }
    pub fn physical(&self, role: Button) -> Button {
        BUTTONS.iter().find(|b| self.role(**b) == role).copied().unwrap_or(role)
    }
    /// Assign physical to role, swapping the previous occupant. No duplicate or unreachable button.
    pub fn assign(&mut self, physical: Button, role: Button) -> Option<Button> {
        let p = BUTTONS.iter().position(|b| *b == physical)?;
        let other = self.roles.iter().position(|r| *r == role)?;
        if p == other { return None; }
        self.roles.swap(p, other);
        Some(BUTTONS[other])
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Profiles {
    #[serde(default)]
    pub devices: BTreeMap<String, Layout>,
}

impl Profiles {
    pub fn load(dir: &Path) -> Self {
        let mut p: Self = std::fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        p.devices.retain(|_, l| l.valid());
        p
    }
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        crate::fsutil::write_atomic(&dir.join(FILE), &serde_json::to_vec(self).map_err(std::io::Error::other)?)
    }
    pub fn layout(&self, key: &str) -> Layout { self.devices.get(key).copied().unwrap_or_default() }
}

#[derive(Default)]
struct Store { profiles: Profiles, stamp: Option<std::time::SystemTime>, initialized: bool }
static STORE: OnceLock<RwLock<Store>> = OnceLock::new();
// Workers serialize disk transactions separately. UI readers only wait for the
// in-memory publication, never for an SD-card read or atomic write/fsync.
static DISK: Mutex<()> = Mutex::new(());
fn store() -> &'static RwLock<Store> { STORE.get_or_init(|| RwLock::new(Store::default())) }
fn data_dir() -> PathBuf { std::env::var_os("POCKETUI_DATA").map(PathBuf::from).unwrap_or_else(|| "/storage/.config/pocket-ui".into()) }

/// Supervisor thread only: pick up saves by the UI in both processes, also while no buttons are pressed.
pub fn refresh() {
    let _disk = DISK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = data_dir();
    let stamp = std::fs::metadata(dir.join(FILE)).and_then(|m| m.modified()).ok();
    let reload = {
        let s = store().read().unwrap_or_else(|p| p.into_inner());
        !s.initialized || stamp != s.stamp
    };
    if reload {
        let profiles = Profiles::load(&dir);
        *store().write().unwrap_or_else(|p| p.into_inner()) = Store { profiles, stamp, initialized: true };
    }
}
pub fn layout(key: &str) -> Layout { store().read().unwrap_or_else(|p| p.into_inner()).profiles.layout(key) }

/// Called on a worker: persist before publishing, so a failed save never claims success.
pub fn save_layout(dir: &Path, key: &str, layout: Layout) -> std::io::Result<()> {
    let _disk = DISK.lock().unwrap_or_else(|p| p.into_inner());
    let mut next = Profiles::load(dir);
    next.devices.insert(key.into(), layout);
    next.save(dir)?;
    let stamp = std::fs::metadata(dir.join(FILE)).and_then(|m| m.modified()).ok();
    *store().write().unwrap_or_else(|p| p.into_inner()) = Store { profiles: next, stamp, initialized: true };
    Ok(())
}

/// Remember the role of a held button, even when its profile changes before release.
#[derive(Default)]
pub struct Remap { held: BTreeMap<String, Vec<(usize, Button, Button)>> }
impl Remap {
    pub fn event(&mut self, key: &str, layout: &Layout, ev: PadEvent) -> PadEvent {
        match ev {
            PadEvent::Button { source, button, pressed } => {
                let held = self.held.entry(key.into()).or_default();
                let at = held.iter().position(|(s, b, _)| *s == source && *b == button);
                let role = if pressed {
                    if let Some(i) = at { held[i].2 } else { let role = layout.role(button); held.push((source, button, role)); role }
                } else { at.map(|i| held.remove(i).2).unwrap_or_else(|| layout.role(button)) };
                PadEvent::Button { source, button: role, pressed }
            }
            other => other,
        }
    }
}

/// How long Controller setup waits for a button before it gives up by itself.
pub const CAPTURE_WAIT: Duration = Duration::from_secs(8);

/// Controller setup waiting for a button of the selected controller (`app_controllers`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    /// The action that gets the button.
    pub target: Button,
    /// Without a press by then the capture ends and nothing changes.
    pub until: Instant,
}

/// What a press does to a capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStep {
    /// The capture ends, nothing changes.
    Cancel,
    /// The selected controller's button: it gets the action.
    Take,
    /// Another controller: nothing happens (it can't steal the capture).
    Swallow,
}

impl Capture {
    pub fn new(target: Button, now: Instant) -> Capture {
        Capture { target, until: now + CAPTURE_WAIT }
    }

    /// A press of `button` on controller `key` while `selected` is set up. HOME on any controller cancels (HOME is no
    /// action, so it can never be captured); while another controller is set up, any button of the Pocket's own
    /// controls cancels too, so the menus always stay in reach of the Pocket (06.11.: a Bluetooth pad's capture left
    /// the RG35XX Plus without a way out).
    pub fn press(&self, key: &str, selected: &str, button: Button) -> CaptureStep {
        if button == Button::Mode {
            CaptureStep::Cancel
        } else if key == selected {
            CaptureStep::Take
        } else if key == super::BUILTIN {
            CaptureStep::Cancel
        } else {
            CaptureStep::Swallow
        }
    }

    pub fn expired(&self, now: Instant) -> bool {
        now >= self.until
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assigning_an_occupied_button_swaps_and_keeps_every_action_reachable() {
        let mut l = Layout::default();
        assert_eq!(l.assign(Button::L2, Button::East), Some(Button::East));
        assert_eq!(l.role(Button::L2), Button::East);
        assert_eq!(l.role(Button::East), Button::L2);
        assert!(l.valid());
    }
    #[test]
    fn two_identical_pads_keep_distinct_layouts_across_restart_and_seat_changes() {
        let d = tempfile::tempdir().unwrap();
        let mut p = Profiles::default();
        let mut l = Layout::default(); l.assign(Button::L2, Button::South);
        p.devices.insert("bt:pad-a".into(), l); p.save(d.path()).unwrap();
        let p = Profiles::load(d.path());
        assert_eq!(p.layout("bt:pad-a"), l);
        assert_eq!(p.layout("bt:pad-b"), Layout::default());
        assert_eq!(p.layout(super::super::BUILTIN), Layout::default());
    }
    #[test]
    fn a_mapping_changed_while_held_releases_the_original_action() {
        let mut r = Remap::default(); let mut l = Layout::default();
        let ev = |pressed| PadEvent::Button { source: 3, button: Button::L2, pressed };
        r.event("bt:a", &l, ev(true)); l.assign(Button::L2, Button::South);
        assert!(matches!(r.event("bt:a", &l, ev(false)), PadEvent::Button { button: Button::L2, pressed: false, .. }));
        assert!(matches!(r.event("bt:a", &l, ev(true)), PadEvent::Button { button: Button::South, pressed: true, .. }));
    }
    #[test]
    fn a_capture_always_has_a_way_out() {
        let now = Instant::now();
        let c = Capture::new(Button::L2, now);
        let pad = "bt:e4:17:d8:e7:2b:a1";
        assert_eq!(c.press(pad, pad, Button::South), CaptureStep::Take);
        assert_eq!(c.press("bt:other", pad, Button::South), CaptureStep::Swallow, "another pad can't steal it");
        assert_eq!(c.press("bt:other", pad, Button::Mode), CaptureStep::Cancel, "HOME on any pad");
        assert_eq!(c.press(pad, pad, Button::Mode), CaptureStep::Cancel);
        for b in [Button::East, Button::South, Button::Up, Button::Mode] {
            assert_eq!(c.press(super::super::BUILTIN, pad, b), CaptureStep::Cancel, "the Pocket's {b:?} leaves a pad's capture");
        }
        // Setting up the Pocket's own controls: its buttons are what is captured, HOME still cancels.
        let builtin = super::super::BUILTIN;
        assert_eq!(c.press(builtin, builtin, Button::East), CaptureStep::Take);
        assert_eq!(c.press(builtin, builtin, Button::Mode), CaptureStep::Cancel);
        assert!(!BUTTONS.contains(&Button::Mode), "HOME can never be captured");
        // Nothing pressed: it ends by itself after about 8 s.
        assert!(!c.expired(now + Duration::from_secs(7)));
        assert!(c.expired(now + Duration::from_secs(8)));
    }
}
