//! Android can deliver a complete touch between two emulation ticks.
#[derive(Default)]
pub struct Pointer {
    held: Option<(i16, i16)>,
    pending: Option<(i16, i16)>,
}
impl Pointer {
    pub fn event(&mut self, point: Option<(i16, i16)>) {
        self.held = point;
        if point.is_some() { self.pending = point; }
    }
    pub fn sample(&mut self) -> Option<(i16, i16)> {
        let point = self.held.or(self.pending);
        self.pending = None;
        point
    }
    pub fn clear(&mut self) { *self = Self::default(); }
}

#[cfg(test)]
mod tests {
    use super::Pointer;
    #[test]
    fn quick_ds_touch_reaches_one_core_tick_and_then_releases() {
        let mut touch = Pointer::default();
        touch.event(Some((1234, 20000)));
        touch.event(None);
        assert_eq!(touch.sample(), Some((1234, 20000)), "a complete tap between video frames vanished");
        assert_eq!(touch.sample(), None, "a quick tap stayed held");
    }
    #[test]
    fn held_ds_touch_moves_and_cancel_does_not_tap_the_next_game_frame() {
        let mut touch = Pointer::default();
        touch.event(Some((100, 200)));
        assert_eq!(touch.sample(), Some((100, 200)));
        assert_eq!(touch.sample(), Some((100, 200)));
        touch.event(Some((300, 400)));
        assert_eq!(touch.sample(), Some((300, 400)));
        touch.event(None);
        assert_eq!(touch.sample(), None);
        touch.event(Some((500, 600)));
        touch.clear();
        assert_eq!(touch.sample(), None, "pause or cancellation left a phantom stylus press");
    }
}
