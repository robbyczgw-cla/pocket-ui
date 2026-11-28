//! Android extras, root's part (scaffold; docs/ANDROID-EXTRAS.md "Root"): the shell scripts pocket-ui would run through
//! Magisk's `su` (`Extras.action("root-run", script)`) and how it reads their answers. Nothing is offered on the Settings page
//! yet: each needs a device to prove it. What is here is what a test can hold without one.
//!
//! - [`Clocks`]: the processor's cpufreq policies and a graphics devfreq node, read by [`DISCOVER`]; a [`Profile`] per game
//!   becomes the writes of [`Clocks::apply`] and the writes back of [`Clocks::restore`].
//! - The fan: a `hwmon` with `pwm1` whose name says fan, as Linux shows it ([`Clocks::fan`]); Android handhelds that drive
//!   their fan in a vendor service keep it (most do; firmware-dependent).
//! - [`BootControl`]: the RG556's 32-byte `bootloader_control` in `misc` at 2048 (AOSP's boot_control layout, magic "BCAB",
//!   CRC-32 over 28 bytes; pocketui-linux-platforms/platforms/unisoc/ums9620/DUAL-BOOT.md §3): the next start once in the
//!   other slot, with tries left so a failed start falls back to Android.

/// The script that lists what the clocks and the fan are: one `key path value` line per file, nothing else.
pub const DISCOVER: &str = r#"for p in /sys/devices/system/cpu/cpufreq/policy*; do
  [ -d "$p" ] || continue
  for f in scaling_available_frequencies cpuinfo_min_freq cpuinfo_max_freq scaling_min_freq scaling_max_freq scaling_governor; do
    [ -r "$p/$f" ] && echo "cpu $p/$f $(cat "$p/$f")"
  done
done
for d in /sys/class/devfreq/*; do
  case "$(basename "$d") $(cat "$d/name" 2>/dev/null)" in *gpu*|*kgsl*|*mali*|*GPU*)
    for f in available_frequencies min_freq max_freq governor; do [ -r "$d/$f" ] && echo "gpu $d/$f $(cat "$d/$f")"; done;;
  esac
done
for h in /sys/class/hwmon/hwmon*; do
  case "$(cat "$h/name" 2>/dev/null)" in *fan*)
    for f in pwm1 pwm1_enable; do [ -r "$h/$f" ] && echo "fan $h/$f $(cat "$h/$f")"; done;;
  esac
done
"#;

/// One processor cluster (a cpufreq policy) or the graphics node: its folder, the steps it allows, now's limits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Domain {
    pub dir: String,
    /// Kernel kHz (cpufreq) or Hz (devfreq), lowest first.
    pub steps: Vec<u64>,
    pub min: u64,
    pub max: u64,
    pub governor: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clocks {
    pub cpus: Vec<Domain>,
    pub gpu: Option<Domain>,
    /// The fan's hwmon folder (pwm1 0..255), where Linux shows one.
    pub fan: Option<String>,
}

/// What a game runs with. Standard: the device's own; Cool: the highest step at most 70 % of the top; Max: the top only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Standard,
    Cool,
    Max,
}

impl Clocks {
    /// [`DISCOVER`]'s answer; lines it doesn't know are left out.
    pub fn parse(text: &str) -> Clocks {
        let mut c = Clocks::default();
        for line in text.lines() {
            let mut f = line.splitn(3, ' ');
            let (Some(kind), Some(path), value) = (f.next(), f.next(), f.next().unwrap_or("").trim()) else { continue };
            let Some((dir, file)) = path.rsplit_once('/') else { continue };
            if kind == "fan" {
                if file == "pwm1" {
                    c.fan = Some(dir.to_string());
                }
                continue;
            }
            let list = match kind {
                "cpu" => &mut c.cpus,
                "gpu" => {
                    if c.gpu.is_none() {
                        c.gpu = Some(Domain { dir: dir.to_string(), ..Domain::default() });
                    }
                    let g = c.gpu.as_mut().unwrap();
                    if g.dir == dir {
                        set(g, file, value);
                    }
                    continue;
                }
                _ => continue,
            };
            if !list.iter().any(|d| d.dir == dir) {
                list.push(Domain { dir: dir.to_string(), ..Domain::default() });
            }
            let d = list.iter_mut().find(|d| d.dir == dir).unwrap();
            set(d, file, value);
        }
        for d in c.cpus.iter_mut().chain(c.gpu.as_mut()) {
            d.steps.sort_unstable();
            d.steps.dedup();
        }
        c
    }

    /// The writes for `profile` as one script (each file only if it is there), or None for Standard.
    pub fn apply(&self, profile: Profile) -> Option<String> {
        if profile == Profile::Standard {
            return None;
        }
        let mut s = String::new();
        let cpu = |d: &Domain, s: &mut String| {
            let (lo, hi) = limits(d, profile);
            // The max first when it goes down, the min first when it goes up: never min > max between the two writes.
            let (first, second) = if hi < d.max { (("scaling_max_freq", hi), ("scaling_min_freq", lo)) } else { (("scaling_min_freq", lo), ("scaling_max_freq", hi)) };
            for (file, v) in [first, second] {
                s.push_str(&write(&d.dir, file, v));
            }
        };
        for d in &self.cpus {
            cpu(d, &mut s);
        }
        if let Some(g) = &self.gpu {
            let (lo, hi) = limits(g, profile);
            let (first, second) = if hi < g.max { (("max_freq", hi), ("min_freq", lo)) } else { (("min_freq", lo), ("max_freq", hi)) };
            for (file, v) in [first, second] {
                s.push_str(&write(&g.dir, file, v));
            }
        }
        Some(s)
    }

    /// The limits as [`Clocks::parse`] read them, written back (the game ended).
    pub fn restore(&self) -> String {
        let mut s = String::new();
        for d in &self.cpus {
            // Widest first, so neither write is refused for crossing the other.
            s.push_str(&write(&d.dir, "scaling_max_freq", d.steps.last().copied().unwrap_or(d.max)));
            s.push_str(&write(&d.dir, "scaling_min_freq", d.min));
            s.push_str(&write(&d.dir, "scaling_max_freq", d.max));
        }
        if let Some(g) = &self.gpu {
            s.push_str(&write(&g.dir, "max_freq", g.steps.last().copied().unwrap_or(g.max)));
            s.push_str(&write(&g.dir, "min_freq", g.min));
            s.push_str(&write(&g.dir, "max_freq", g.max));
        }
        s
    }

    /// The fan at `pwm` (0..=255) by hand, or None without a fan of Linux's; `None` level gives it back to the kernel (enable 2).
    pub fn fan_script(&self, pwm: Option<u8>) -> Option<String> {
        let dir = self.fan.as_deref()?;
        Some(match pwm {
            Some(v) => format!("{}{}", write(dir, "pwm1_enable", 1), write(dir, "pwm1", u64::from(v))),
            None => write(dir, "pwm1_enable", 2),
        })
    }
}

fn set(d: &mut Domain, file: &str, value: &str) {
    let n = value.parse::<u64>().unwrap_or(0);
    match file {
        "scaling_available_frequencies" | "available_frequencies" => d.steps = value.split_whitespace().filter_map(|v| v.parse().ok()).collect(),
        "scaling_min_freq" | "min_freq" => d.min = n,
        "scaling_max_freq" | "max_freq" => d.max = n,
        // Without a list of steps the hardware's bounds are the two ends.
        "cpuinfo_min_freq" | "cpuinfo_max_freq" => d.steps.push(n),
        "scaling_governor" | "governor" => d.governor = value.to_string(),
        _ => {}
    }
}

fn limits(d: &Domain, profile: Profile) -> (u64, u64) {
    let top = d.steps.last().copied().unwrap_or(d.max);
    let bottom = d.steps.first().copied().unwrap_or(d.min);
    match profile {
        Profile::Standard => (d.min, d.max),
        Profile::Max => (top, top),
        Profile::Cool => {
            let cap = top * 7 / 10;
            let hi = d.steps.iter().copied().filter(|s| *s <= cap).max().unwrap_or(bottom).max(bottom);
            (bottom.min(hi), hi)
        }
    }
}

fn write(dir: &str, file: &str, v: u64) -> String {
    format!("[ -w {dir}/{file} ] && echo {v} > {dir}/{file}\n")
}

/// The RG556 (Anbernic, Unisoc T820), the only device whose next boot slot pocket-ui would switch.
pub fn is_rg556(model: &str) -> bool {
    model.to_ascii_uppercase().replace([' ', '-'], "").contains("RG556")
}

/// Where the block is and how long.
pub const MISC: &str = "/dev/block/by-name/misc";
pub const MISC_OFFSET: u64 = 2048;
const MAGIC: u32 = 0x4241_4342; // "BCAB" little-endian

/// The script that prints the block as hex (`od`, two hex digits per byte).
pub fn misc_read_script() -> String {
    format!("dd if={MISC} bs=1 skip={MISC_OFFSET} count=32 2>/dev/null | od -An -v -tx1")
}

/// The script that writes `block` and prints it back as [`misc_read_script`] does, to be compared.
pub fn misc_write_script(block: &[u8; 32]) -> String {
    let bytes: String = block.iter().map(|b| format!("\\x{b:02x}")).collect();
    format!("printf '{bytes}' | dd of={MISC} bs=1 seek={MISC_OFFSET} count=32 conv=notrunc 2>/dev/null && sync && {}", misc_read_script())
}

/// `od`'s hex back to the 32 bytes; None unless exactly 32.
pub fn parse_hex(text: &str) -> Option<[u8; 32]> {
    let bytes: Vec<u8> = text.split_whitespace().take_while(|w| !w.starts_with("exit=")).map(|w| u8::from_str_radix(w, 16)).collect::<Result<_, _>>().ok()?;
    bytes.try_into().ok()
}

/// AOSP's `bootloader_control`: slot suffix, magic, version, slots, per slot priority (4 bits), tries left (3 bits) and
/// successful (1 bit) in byte 12 + 2 × slot, CRC-32 (little-endian) of bytes 0..28 at 28.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootControl(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub priority: u8,
    pub tries: u8,
    pub successful: bool,
}

impl BootControl {
    /// A block with the right magic, at least two slots and a matching CRC; anything else is not written over.
    pub fn read(block: [u8; 32]) -> Result<BootControl, &'static str> {
        if u32::from_le_bytes([block[4], block[5], block[6], block[7]]) != MAGIC {
            return Err("misc has no boot control block");
        }
        let c = BootControl(block);
        if c.slots() < 2 {
            return Err("fewer than two slots");
        }
        if crc(&block) != u32::from_le_bytes([block[28], block[29], block[30], block[31]]) {
            return Err("boot control CRC does not match");
        }
        Ok(c)
    }

    pub fn slots(&self) -> u8 {
        self.0[9] & 0b111
    }

    pub fn slot(&self, i: usize) -> Slot {
        let b = self.0[12 + 2 * i];
        Slot { priority: b & 0x0f, tries: (b >> 4) & 0b111, successful: b & 0x80 != 0 }
    }

    fn set_slot(&mut self, i: usize, s: Slot) {
        self.0[12 + 2 * i] = (s.priority & 0x0f) | ((s.tries & 0b111) << 4) | if s.successful { 0x80 } else { 0 };
    }

    /// The next start once from slot `to` (0 = a, 1 = b): it gets the top priority and `tries` starts (Unisoc's LK counts a
    /// start with one try left and no success as failed, so a trial start has N + 1, DUAL-BOOT.md); the other keeps its
    /// own, a step lower, so a start that never says "successful" falls back to it.
    pub fn next_start_once(&self, to: usize, tries: u8) -> BootControl {
        let mut c = *self;
        let other = 1 - to;
        let mut o = c.slot(other);
        o.priority = o.priority.clamp(1, 14);
        c.set_slot(other, o);
        c.set_slot(to, Slot { priority: 15, tries: tries.clamp(2, 7), successful: false });
        let sum = crc(&c.0).to_le_bytes();
        c.0[28..32].copy_from_slice(&sum);
        c
    }
}

fn crc(block: &[u8; 32]) -> u32 {
    crc32fast::hash(&block[..28])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SM8250: &str = "cpu /sys/devices/system/cpu/cpufreq/policy0/scaling_available_frequencies 300000 691200 1209600 1612800 1804800
cpu /sys/devices/system/cpu/cpufreq/policy0/scaling_min_freq 300000
cpu /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq 1804800
cpu /sys/devices/system/cpu/cpufreq/policy0/scaling_governor schedutil
cpu /sys/devices/system/cpu/cpufreq/policy4/cpuinfo_min_freq 710400
cpu /sys/devices/system/cpu/cpufreq/policy4/cpuinfo_max_freq 2419200
cpu /sys/devices/system/cpu/cpufreq/policy4/scaling_min_freq 710400
cpu /sys/devices/system/cpu/cpufreq/policy4/scaling_max_freq 2419200
gpu /sys/class/devfreq/3d00000.qcom,kgsl-3d0/available_frequencies 587000000 525000000 441600000 305000000
gpu /sys/class/devfreq/3d00000.qcom,kgsl-3d0/min_freq 305000000
gpu /sys/class/devfreq/3d00000.qcom,kgsl-3d0/max_freq 587000000
fan /sys/class/hwmon/hwmon3/pwm1 128
noise line
exit=0";

    #[test]
    fn the_clocks_are_read_from_the_discovery() {
        let c = Clocks::parse(SM8250);
        assert_eq!(c.cpus.len(), 2);
        assert_eq!(c.cpus[0].steps, vec![300_000, 691_200, 1_209_600, 1_612_800, 1_804_800]);
        assert_eq!(c.cpus[1].steps, vec![710_400, 2_419_200], "no list: the hardware's two ends");
        assert_eq!(c.gpu.as_ref().map(|g| g.steps.first().copied()), Some(Some(305_000_000)), "sorted lowest first");
        assert_eq!(c.fan.as_deref(), Some("/sys/class/hwmon/hwmon3"));
        assert_eq!(Clocks::parse(""), Clocks::default(), "nothing found: nothing to write");
    }

    #[test]
    fn a_profile_writes_its_limits_and_restore_puts_them_back() {
        let c = Clocks::parse(SM8250);
        assert_eq!(c.apply(Profile::Standard), None);
        let cool = c.apply(Profile::Cool).unwrap();
        // 70 % of 1.8048 GHz is 1.263 GHz: the step below it, 1.2096 GHz; the max goes down before the min.
        let p0 = cool.lines().filter(|l| l.contains("policy0")).collect::<Vec<_>>();
        assert!(p0[0].contains("echo 1209600 > /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq"), "{p0:?}");
        assert!(p0[1].contains("echo 300000 >"), "{p0:?}");
        // The GPU: 70 % of 587 MHz is 411 MHz, the step at or under it 305 MHz.
        assert!(cool.contains("echo 305000000 > /sys/class/devfreq/3d00000.qcom,kgsl-3d0/max_freq"), "{cool}");
        let max = c.apply(Profile::Max).unwrap();
        let p4 = max.lines().filter(|l| l.contains("policy4")).collect::<Vec<_>>();
        assert!(p4[0].contains("echo 2419200 > ") && p4[0].contains("scaling_min_freq"), "up: the min first: {p4:?}");
        assert!(max.lines().all(|l| l.starts_with("[ -w ")), "a file that isn't there is left alone");
        let back = c.restore();
        assert!(back.contains("echo 300000 > /sys/devices/system/cpu/cpufreq/policy0/scaling_min_freq"));
        assert!(back.lines().last().unwrap().contains("echo 587000000 > /sys/class/devfreq/3d00000.qcom,kgsl-3d0/max_freq"));
        assert_eq!(c.fan_script(Some(200)).unwrap().lines().count(), 2);
        assert!(c.fan_script(None).unwrap().contains("echo 2 > /sys/class/hwmon/hwmon3/pwm1_enable"), "back to the kernel");
        assert_eq!(Clocks::default().fan_script(Some(1)), None, "no fan, no script");
    }

    /// A block as the RG556's stock firmware would write it: slot a active (priority 15, successful), b 14 with 7 tries.
    fn stock() -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..3].copy_from_slice(b"_a\0");
        b[4..8].copy_from_slice(&MAGIC.to_le_bytes());
        b[8] = 1;
        b[9] = 2 | (7 << 3);
        b[12] = 15 | 0x80;
        b[14] = 14 | (7 << 4);
        let sum = crc(&b).to_le_bytes();
        b[28..32].copy_from_slice(&sum);
        b
    }

    #[test]
    fn the_next_start_goes_once_to_the_other_slot_and_falls_back() {
        let c = BootControl::read(stock()).unwrap();
        assert_eq!(c.slot(0), Slot { priority: 15, tries: 0, successful: true });
        let next = c.next_start_once(1, 3);
        assert_eq!(next.slot(1), Slot { priority: 15, tries: 3, successful: false });
        assert_eq!(next.slot(0), Slot { priority: 14, tries: 0, successful: true }, "Android stays, a step lower");
        assert!(BootControl::read(next.0).is_ok(), "the CRC is written anew");
        assert_eq!(next.0[..12], stock()[..12], "suffix, magic, version and slot count stay");
        assert_eq!(next.0[16..28], stock()[16..28]);
        assert_eq!(c.next_start_once(1, 1).slot(1).tries, 2, "one try would count as failed at once");
    }

    #[test]
    fn a_block_that_is_not_ours_to_change_is_refused() {
        let mut b = stock();
        b[4] = 0;
        assert!(BootControl::read(b).is_err(), "no magic");
        let mut b = stock();
        b[20] ^= 1;
        assert!(BootControl::read(b).is_err(), "CRC");
        let mut b = stock();
        b[9] = 1;
        let sum = crc(&b).to_le_bytes();
        b[28..32].copy_from_slice(&sum);
        assert!(BootControl::read(b).is_err(), "one slot");
    }

    #[test]
    fn the_block_goes_through_od_and_printf_whole() {
        let b = stock();
        let od: String = b.chunks(16).map(|c| c.iter().map(|x| format!(" {x:02x}")).collect::<String>() + "\n").collect();
        assert_eq!(parse_hex(&format!("{od}exit=0")), Some(b));
        assert_eq!(parse_hex(" 01 02\n"), None, "short");
        assert_eq!(parse_hex("zz"), None);
        let w = misc_write_script(&b);
        assert!(w.starts_with("printf '\\x5f\\x61\\x00"), "{w}");
        assert!(w.contains("seek=2048 count=32 conv=notrunc") && w.ends_with(&misc_read_script()));
        assert!(is_rg556("RG556") && is_rg556("Anbernic RG 556") && !is_rg556("Retroid Pocket Mini V2"));
    }
}
