//! Coordinate with NetworkManager's own activation, including the iwd auto-connect on wake.
//! Numeric device states avoid depending on translated nmcli descriptions or profile names.
use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct Device {
    pub state: u32,
    pub ssid: Option<String>,
    /// GENERAL.REASON from the same device snapshot; retained when a joined activation fails.
    pub reason: String,
}

impl Device {
    pub fn failure(&self) -> String {
        format!("Activation failed: {}", self.reason)
    }
    pub fn activating(&self) -> bool {
        (40..100).contains(&self.state)
    }
    pub fn settling(&self) -> bool {
        self.state < 30 || self.activating() || self.state == 110
    }
}

pub(crate) trait Ops {
    fn device(&mut self) -> Result<Device, String>;
    fn disconnect(&mut self) -> Result<(), String>;
    fn now(&self) -> Duration;
    fn wait(&mut self);
}

/// True: the requested connection already succeeded. False: safe to start a new activation.
/// A failed or timed-out activation we joined is reported, never replaced with another attempt.
pub(crate) fn prepare(ops: &mut impl Ops, ssid: &str) -> Result<bool, String> {
    prepare_with(ops, ssid, false, false)
}

/// A background request leaves every existing activation alone, including one for another SSID.
/// True also means skipped: the watchdog has no user result to report.
pub(crate) fn prepare_reconnect(ops: &mut impl Ops, ssid: &str) -> Result<bool, String> {
    prepare_with(ops, ssid, true, false)
}

/// An explicitly supplied key replaces an old activation, even for the same SSID.
pub(crate) fn prepare_password(ops: &mut impl Ops, ssid: &str) -> Result<bool, String> {
    prepare_with(ops, ssid, false, true)
}

fn prepare_with(ops: &mut impl Ops, ssid: &str, background: bool, new_password: bool) -> Result<bool, String> {
    let settle_until = ops.now() + Duration::from_secs(10);
    let mut d = ops.device()?;
    if background && !matches!(d.state, 30 | 120) { return Ok(true); }
    while d.state < 30 || d.state == 110 {
        if ops.now() >= settle_until {
            return Err("wlan0 not ready yet".into());
        }
        ops.wait();
        d = ops.device()?;
    }
    // The active UUID may appear just after the device enters prepare. Give NM time to identify it rather than
    // disconnecting what could be the very activation the player selected.
    let identify_until = ops.now() + Duration::from_secs(10);
    while (d.activating() || d.state == 100) && d.ssid.is_none() {
        if ops.now() >= identify_until { return Err("wlan0 not ready yet".into()); }
        ops.wait();
        d = ops.device()?;
    }
    if !new_password && d.ssid.as_deref() == Some(ssid) && (d.activating() || d.state == 100) {
        let until = ops.now() + Duration::from_secs(50);
        loop {
            if d.state == 100 && d.ssid.as_deref() == Some(ssid) {
                return Ok(true);
            }
            if !d.activating() || d.ssid.as_deref() != Some(ssid) {
                return Err(d.failure());
            }
            if ops.now() >= until {
                return Err("Timeout expired while connecting to Wi-Fi".into());
            }
            ops.wait();
            d = ops.device()?;
        }
    }
    if d.activating() || d.state == 100 {
        // The requested SSID differs from the one NM is activating or using.
        ops.disconnect()?;
        let until = ops.now() + Duration::from_secs(10);
        loop {
            d = ops.device()?;
            if d.state == 30 || d.state == 120 {
                break;
            }
            if ops.now() >= until {
                return Err("Timeout expired while leaving the previous network".into());
            }
            ops.wait();
        }
    }
    Ok(false)
}

/// Boot and each resume get a fresh settling deadline. Activations always inhibit the watchdog,
/// even after the ten-second window. Unmanaged/unavailable devices are never reconnect candidates.
pub(crate) struct Settle {
    generation: u64,
    until: Duration,
    settled: bool,
}

impl Settle {
    pub fn new(now: Duration, generation: u64) -> Self {
        Self {
            generation,
            until: now + Duration::from_secs(10),
            settled: false,
        }
    }
    pub fn ready(&mut self, now: Duration, generation: u64, d: &Device) -> bool {
        if generation != self.generation {
            *self = Self::new(now, generation);
        }
        if !d.settling() || now >= self.until {
            self.settled = true;
        }
        self.settled && matches!(d.state, 30 | 120)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Fake {
        states: VecDeque<Device>,
        clock: Duration,
        disconnects: usize,
    }
    fn device(state: u32, ssid: &str) -> Device {
        Device {
            state,
            ssid: (!ssid.is_empty()).then(|| ssid.into()),
            reason: String::new(),
        }
    }
    impl Ops for Fake {
        fn device(&mut self) -> Result<Device, String> {
            if self.states.len() > 1 {
                Ok(self.states.pop_front().unwrap())
            } else {
                Ok(self.states[0].clone())
            }
        }
        fn disconnect(&mut self) -> Result<(), String> {
            self.disconnects += 1;
            Ok(())
        }
        fn now(&self) -> Duration {
            self.clock
        }
        fn wait(&mut self) {
            self.clock += Duration::from_secs(1);
        }
    }
    fn fake(states: Vec<Device>) -> Fake {
        Fake {
            states: states.into(),
            clock: Duration::ZERO,
            disconnects: 0,
        }
    }
    #[test]
    fn same_ssid_activating_joins_one_activation_and_succeeds() {
        let mut f = fake(vec![device(50, "Xiaomi 15"), device(70, "Xiaomi 15"), device(100, "Xiaomi 15")]);
        let mut activations = 1;
        if !prepare(&mut f, "Xiaomi 15").unwrap() {
            activations += 1;
        }
        assert_eq!(activations, 1);
        assert_eq!(f.disconnects, 0);
    }
    #[test]
    fn an_activation_without_its_uuid_yet_is_identified_before_connecting() {
        let mut f = fake(vec![device(40, ""), device(50, "Home"), device(100, "Home")]);
        assert!(prepare(&mut f, "Home").unwrap());
        assert_eq!(f.disconnects, 0);
        let mut f = fake(vec![device(40, "")]);
        assert!(prepare(&mut f, "Home").is_err());
        assert_eq!(f.disconnects, 0);
    }

    #[test]
    fn same_ssid_connected_does_not_reactivate() {
        assert!(prepare(&mut fake(vec![device(100, "Home")]), "Home").unwrap());
    }
    #[test]
    fn joined_activation_failure_and_timeout_do_not_retry() {
        assert!(prepare(&mut fake(vec![device(50, "Home"), device(120, "Home")]), "Home")
            .unwrap_err()
            .contains("Activation failed"));
        let mut f = fake(vec![device(70, "Home")]);
        assert!(prepare(&mut f, "Home").unwrap_err().contains("Timeout expired"));
        assert_eq!(f.clock.as_secs(), 50);
        assert_eq!(f.disconnects, 0);
    }
    #[test]
    fn a_new_password_disconnects_the_old_same_ssid_activation() {
        let mut f = fake(vec![device(50, "Home"), device(120, "Home")]);
        assert!(!prepare_password(&mut f, "Home").unwrap());
        assert_eq!(f.disconnects, 1);
    }

    #[test]
    fn a_join_can_finish_after_networkmanagers_dhcp_timeout() {
        let mut states = vec![device(70, "Home"); 47];
        states.push(device(100, "Home"));
        let mut f = fake(states);
        assert!(prepare(&mut f, "Home").unwrap());
        assert_eq!(f.clock.as_secs(), 47);
    }

    #[test]
    fn different_ssid_switch_disconnects_and_waits_first() {
        for state in [50, 100] {
            let mut f = fake(vec![device(state, "Office"), device(110, "Office"), device(30, "")]);
            assert!(!prepare(&mut f, "Home").unwrap());
            assert_eq!(f.disconnects, 1);
            assert_eq!(f.clock.as_secs(), 1);
        }
    }
    #[test]
    fn boot_and_resume_wait_for_device_to_settle() {
        let mut s = Settle::new(Duration::ZERO, 0);
        assert!(!s.ready(Duration::ZERO, 0, &device(10, "")));
        assert!(!s.ready(Duration::from_secs(1), 0, &device(20, "")));
        assert!(s.ready(Duration::from_secs(2), 0, &device(30, "")));
        assert!(!s.ready(Duration::from_secs(100), 1, &device(20, "")));
        assert!(!s.ready(Duration::from_secs(111), 1, &device(20, "")));
        assert!(s.ready(Duration::from_secs(112), 1, &device(30, "")));
    }
    #[test]
    fn watchdog_during_activation_does_nothing_even_after_settle_deadline() {
        let mut s = Settle::new(Duration::ZERO, 0);
        for state in 40..=110 {
            assert!(!s.ready(Duration::from_secs(60), 0, &device(state, "Home")));
        }
    }
    #[test]
    fn watchdog_guard_never_switches_a_network_that_started_after_its_scan() {
        for state in [10, 20, 40, 50, 70, 90, 100, 110] {
            let mut f = fake(vec![device(state, "Office")]);
            assert!(prepare_reconnect(&mut f, "Home").unwrap());
            assert_eq!(f.disconnects, 0);
            assert_eq!(f.clock, Duration::ZERO);
        }
        assert!(!prepare_reconnect(&mut fake(vec![device(30, "")]), "Home").unwrap());
    }

    #[test]
    fn connect_waits_for_wake_before_joining_nm() {
        let mut f = fake(vec![device(10, ""), device(20, ""), device(70, "Home"), device(100, "Home")]);
        assert!(prepare(&mut f, "Home").unwrap());
        let mut f = fake(vec![device(20, "")]);
        assert!(prepare(&mut f, "Home").is_err());
        assert_eq!(f.clock.as_secs(), 10);
    }
}
