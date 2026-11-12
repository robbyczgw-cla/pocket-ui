//! Choosing a scheduling lead from AAudio's presentation estimate and its actual queued frames.
/// The software handoff is separate from AAudio's own counters and must be counted once.
pub(crate) fn ahead_us(timestamp: Option<u64>, counter: u64, handoff: u64, buffer: u64, burst: u64, local_fast: bool) -> u64 {
    // Read counters can omit a few hardware bursts. Allow that, but not half a second on a
    // confirmed speaker/analogue headset Fast track whose actual queue is only a few ms.
    // Unknown, Bluetooth and USB/DSP routes retain their presentation delay. A real backlog
    // confirmed by the read counter is retained too; no device-specific constant is subtracted.
    let tolerance = buffer.saturating_add(burst).saturating_mul(4).max(100_000);
    let plausible = timestamp.filter(|&reported| !local_fast || reported <= counter.max(buffer).saturating_add(tolerance));
    plausible.map_or_else(|| counter.saturating_add(handoff).max(buffer), |reported| reported.saturating_add(handoff))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_valid_timestamp_keeps_the_hardware_delay_when_read_is_coarse() {
        assert_eq!(ahead_us(Some(37_000), 0, 16_000, 9_000, 3_000, true), 53_000);
    }
    #[test]
    fn bluetooth_and_unknown_routes_keep_their_real_presentation_delay() {
        for (buffer, burst) in [(96_000, 32_000), (9_000, 3_000)] {
            let lead = ahead_us(Some(529_000), 8_000, 16_000, buffer, burst, false);
            assert_eq!(700_000u64.saturating_sub(lead) + 545_000, 700_000);
        }
    }
    #[test]
    fn a_backlog_confirmed_by_the_read_counter_is_not_discarded() {
        assert_eq!(ahead_us(Some(529_000), 529_000, 16_000, 9_000, 3_000, true), 545_000);
    }
    #[test]
    fn a_large_software_handoff_is_preserved_once_and_not_mistaken_for_a_driver_fault() {
        assert_eq!(ahead_us(Some(25_000), 9_000, 800_000, 9_000, 3_000, true), 825_000);
    }
    #[test]
    fn before_a_timestamp_exists_the_buffer_or_actual_queue_covers_the_lead() {
        assert_eq!(ahead_us(None, 0, 16_000, 24_000, 3_000, true), 24_000);
        assert_eq!(ahead_us(None, 9_000, 16_000, 9_000, 3_000, true), 25_000);
    }
    #[test]
    fn a_false_local_timestamp_does_not_play_the_scheduled_click_half_a_second_early() {
        let physical_queue = 9_000 + 16_000;
        let due = 700_000u64;
        let lead = ahead_us(Some(529_000), 9_000, 16_000, 9_000, 3_000, true);
        let start_after = due.saturating_sub(lead);
        assert_eq!(start_after + physical_queue, due, "the audible click must land on its visible cue");
    }
}
