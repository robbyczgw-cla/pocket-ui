//! Android owns output routing and buffering. The shared mixer never changes the system graph.
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(2);
pub trait System: Send + 'static {
    fn connections(&self) -> Option<Vec<String>>;
    fn default_sink(&self) -> Option<String>;
    fn force(&self, frames: u32) -> bool;
    fn restore(&self);
}
pub struct Real;
impl System for Real {
    fn connections(&self) -> Option<Vec<String>> {
        None
    }
    fn default_sink(&self) -> Option<String> {
        None
    }
    fn force(&self, frames: u32) -> bool {
        crate::audio::force_quantum(frames)
    }
    fn restore(&self) {}
}
#[derive(PartialEq)]
pub enum Output {
    Speaker,
    Bluetooth,
}
pub fn current(_: &impl System) -> Output {
    if crate::platform::is_speaker() {
        Output::Speaker
    } else {
        Output::Bluetooth
    }
}

/// Android selects the route; the shared queue uses the speaker or Bluetooth buffer policy.
pub struct Keeper {
    small: std::sync::Arc<std::sync::atomic::AtomicBool>,
    decided: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Keeper {
    pub fn spawn(_: u32) -> Self {
        Self {
            small: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                crate::platform::is_speaker() && crate::platform::audio_backend::low_latency(),
            )),
            decided: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }
    pub fn small_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.small.clone()
    }
    pub fn decided_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.decided.clone()
    }
    pub fn keep(&self) {}
    pub fn wake(&self) {
        self.small.store(
            crate::platform::is_speaker() && crate::platform::audio_backend::low_latency(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    pub fn bluetooth(&self) -> bool {
        !self.small.load(std::sync::atomic::Ordering::Relaxed)
    }
}
