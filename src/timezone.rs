//! Time zones: the list ROCKNIX offers, grouping by region, search, and the UTC offset a zone has right
//! now (read from its TZif file, so the picker can show "UTC+02:00" and the local time there).
//!
//! ROCKNIX ships "slim" zoneinfo: only transitions up to 1980 or so, then a POSIX rule in the file's
//! footer (`CET-1CEST,M3.5.0,M10.5.0/3`). So the footer is evaluated here too.

use std::path::Path;

extern "C" {
    #[link_name = "tzset"]
    fn c_tzset();
}

/// Makes libc re-read /etc/localtime (and TZ), so a changed zone shows in `localtime_r` at once.
pub fn tzset() {
    // SAFETY: tzset takes no arguments and only updates libc's own time zone state.
    unsafe { c_tzset() }
}

static ZONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Held while the process's `TZ` is written (`system_ctl::apply_process_zone`) and while another thread reads the
/// local time (the night light's): setenv and getenv at the same moment is not safe.
pub fn zone_lock() -> std::sync::MutexGuard<'static, ()> {
    ZONE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Zones with no `/` (UTC, GMT) go under this heading.
pub const OTHER: &str = "Other";

/// Only names made of these characters are ever handed to a system tool.
pub fn is_valid(tz: &str) -> bool {
    !tz.is_empty() && !tz.starts_with('/') && !tz.contains("..") && tz.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'))
}

/// All zone names, sorted: ROCKNIX's own list (`/usr/config/system/configs/tz`, the one EmulationStation
/// offers) when it exists, otherwise the region folders of the zoneinfo directory.
pub fn list(zoneinfo: &Path, rocknix_list: &Path) -> Vec<String> {
    let mut zones: Vec<String> = std::fs::read_to_string(rocknix_list)
        .map(|s| s.lines().map(|l| l.trim().to_string()).collect())
        .unwrap_or_default();
    if zones.is_empty() {
        const REGIONS: [&str; 10] = [
            "Africa",
            "America",
            "Antarctica",
            "Arctic",
            "Asia",
            "Atlantic",
            "Australia",
            "Europe",
            "Indian",
            "Pacific",
        ];
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let path = e.path();
                if path.is_dir() {
                    walk(&path, &format!("{prefix}{name}/"), out);
                } else if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    out.push(format!("{prefix}{name}"));
                }
            }
        }
        for r in REGIONS {
            walk(&zoneinfo.join(r), &format!("{r}/"), &mut zones);
        }
        if zoneinfo.join("UTC").is_file() {
            zones.push("UTC".into());
        }
    }
    zones.retain(|z| is_valid(z) && !z.starts_with("Etc/"));
    zones.sort();
    zones.dedup();
    zones
}

pub fn region_of(zone: &str) -> &str {
    zone.split_once('/').map_or(OTHER, |(r, _)| r)
}

/// "America/Argentina/Buenos_Aires" -> "Buenos Aires, Argentina"; "Europe/Vienna" -> "Vienna".
pub fn city_of(zone: &str) -> String {
    let rest = zone.split_once('/').map_or(zone, |(_, c)| c);
    let rest = rest.replace('_', " ");
    match rest.split_once('/') {
        Some((sub, city)) => format!("{city}, {sub}"),
        None => rest,
    }
}

/// Region headings in the order they are listed.
pub fn regions(zones: &[String]) -> Vec<String> {
    let mut v: Vec<String> = zones.iter().map(|z| region_of(z).to_string()).collect();
    v.sort();
    v.dedup();
    v
}

/// The zones of a region, ordered by the city name shown (not by the path, which puts "Argentina/..." first).
pub fn zones_in<'a>(zones: &'a [String], region: &str) -> Vec<&'a str> {
    let mut v: Vec<&str> = zones.iter().map(String::as_str).filter(|z| region_of(z) == region).collect();
    v.sort_by_key(|z| city_of(z).to_lowercase());
    v
}

/// Zones whose region or city contains every word of `query` (case, spaces and underscores ignored).
pub fn search<'a>(zones: &'a [String], query: &str) -> Vec<&'a str> {
    let norm = |s: &str| s.to_lowercase().replace(['_', '/'], " ");
    let words: Vec<String> = norm(query).split_whitespace().map(str::to_string).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut v: Vec<&str> = zones
        .iter()
        .map(String::as_str)
        .filter(|z| {
            let hay = norm(z);
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    v.sort_by_key(|z| city_of(z).to_lowercase());
    v
}

/// "UTC+02:00", "UTC-04:00", "UTC+05:30".
pub fn fmt_offset(secs: i32) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let a = secs.abs();
    format!("UTC{sign}{:02}:{:02}", a / 3600, a % 3600 / 60)
}

// ---------------------------------------------------------------------------------------------------
// The first start's guess. Nothing leaves the device: the system's own zone, the country of the Wi-Fi chip's
// regulatory domain (`iw reg get`), and zoneinfo's own `zone.tab`. No IP lookup.

/// "17:42" in a zone `offset` seconds east of UTC, at the Unix time `unix`.
pub fn clock_at(unix: i64, offset: i32) -> String {
    let s = (unix + offset as i64).rem_euclid(86400);
    format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
}

/// UTC, GMT and their aliases: what a device that was never set up reports.
pub fn is_utc(zone: &str) -> bool {
    matches!(
        zone.strip_prefix("Etc/").unwrap_or(zone),
        "UTC" | "UCT" | "GMT" | "GMT0" | "GMT+0" | "GMT-0" | "Greenwich" | "Universal" | "Zulu"
    )
}

/// "AT" but not "00", "98", "99" (world, intersection, custom) or "EU".
fn is_country_code(s: &str) -> bool {
    s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase()) && s != "EU"
}

/// The country code from `iw reg get`: a real one (two capitals, not "00" or "99"); the Wi-Fi chip's own
/// ("phy#0 (self-managed)") before the global one, which is "00" until a network has told the system.
pub fn parse_reg_country(iw_reg_get: &str) -> Option<String> {
    let (mut global, mut phy) = (None, None);
    let mut in_phy = false;
    for line in iw_reg_get.lines() {
        let line = line.trim_end();
        if line.starts_with("phy#") {
            in_phy = true;
        } else if line == "global" {
            in_phy = false;
        } else if let Some(rest) = line.strip_prefix("country ") {
            let code = rest.split([':', ' ']).next().unwrap_or("");
            if is_country_code(code) {
                let slot = if in_phy { &mut phy } else { &mut global };
                slot.get_or_insert_with(|| code.to_string());
            }
        }
    }
    phy.or(global)
}

/// What the first start offers.
#[derive(Debug, Clone, PartialEq)]
pub enum Guess {
    /// The system already has a zone other than UTC.
    System(String),
    /// A country with one main zone.
    Country { code: String, zone: String },
    /// A country with several zones: the picker opens on these.
    Several { code: String, zones: Vec<String> },
    /// Nothing to go on.
    Utc,
}

impl Guess {
    /// The zone to pre-select, if there is one.
    pub fn zone(&self) -> Option<&str> {
        match self {
            Guess::System(z) | Guess::Country { zone: z, .. } => Some(z),
            Guess::Several { .. } => None,
            Guess::Utc => Some("UTC"),
        }
    }
}

/// Country code -> main zone, for countries that have one or whose people nearly all live in one. The others
/// (US, CA, MX, BR, AU, RU, ID, KZ, ...) come from `zone.tab` and the picker.
const COUNTRY_ZONES: &[(&str, &str)] = &[
    // Europe
    ("AD", "Europe/Andorra"),
    ("AL", "Europe/Tirane"),
    ("AT", "Europe/Vienna"),
    ("BA", "Europe/Sarajevo"),
    ("BE", "Europe/Brussels"),
    ("BG", "Europe/Sofia"),
    ("BY", "Europe/Minsk"),
    ("CH", "Europe/Zurich"),
    ("CY", "Asia/Nicosia"),
    ("CZ", "Europe/Prague"),
    ("DE", "Europe/Berlin"),
    ("DK", "Europe/Copenhagen"),
    ("EE", "Europe/Tallinn"),
    ("ES", "Europe/Madrid"),
    ("FI", "Europe/Helsinki"),
    ("FR", "Europe/Paris"),
    ("GB", "Europe/London"),
    ("GR", "Europe/Athens"),
    ("HR", "Europe/Zagreb"),
    ("HU", "Europe/Budapest"),
    ("IE", "Europe/Dublin"),
    ("IS", "Atlantic/Reykjavik"),
    ("IT", "Europe/Rome"),
    ("LI", "Europe/Vaduz"),
    ("LT", "Europe/Vilnius"),
    ("LU", "Europe/Luxembourg"),
    ("LV", "Europe/Riga"),
    ("MC", "Europe/Monaco"),
    ("MD", "Europe/Chisinau"),
    ("ME", "Europe/Podgorica"),
    ("MK", "Europe/Skopje"),
    ("MT", "Europe/Malta"),
    ("NL", "Europe/Amsterdam"),
    ("NO", "Europe/Oslo"),
    ("PL", "Europe/Warsaw"),
    ("PT", "Europe/Lisbon"),
    ("RO", "Europe/Bucharest"),
    ("RS", "Europe/Belgrade"),
    ("SE", "Europe/Stockholm"),
    ("SI", "Europe/Ljubljana"),
    ("SK", "Europe/Bratislava"),
    ("SM", "Europe/San_Marino"),
    ("TR", "Europe/Istanbul"),
    ("UA", "Europe/Kyiv"),
    ("VA", "Europe/Vatican"),
    // Asia
    ("AE", "Asia/Dubai"),
    ("BD", "Asia/Dhaka"),
    ("CN", "Asia/Shanghai"),
    ("HK", "Asia/Hong_Kong"),
    ("IL", "Asia/Jerusalem"),
    ("IN", "Asia/Kolkata"),
    ("IQ", "Asia/Baghdad"),
    ("IR", "Asia/Tehran"),
    ("JO", "Asia/Amman"),
    ("JP", "Asia/Tokyo"),
    ("KR", "Asia/Seoul"),
    ("KW", "Asia/Kuwait"),
    ("LB", "Asia/Beirut"),
    ("LK", "Asia/Colombo"),
    ("MY", "Asia/Kuala_Lumpur"),
    ("NP", "Asia/Kathmandu"),
    ("PH", "Asia/Manila"),
    ("PK", "Asia/Karachi"),
    ("QA", "Asia/Qatar"),
    ("SA", "Asia/Riyadh"),
    ("SG", "Asia/Singapore"),
    ("TH", "Asia/Bangkok"),
    ("TW", "Asia/Taipei"),
    ("VN", "Asia/Ho_Chi_Minh"),
    // Americas
    ("AR", "America/Argentina/Buenos_Aires"),
    ("CL", "America/Santiago"),
    ("CO", "America/Bogota"),
    ("CR", "America/Costa_Rica"),
    ("CU", "America/Havana"),
    ("DO", "America/Santo_Domingo"),
    ("EC", "America/Guayaquil"),
    ("JM", "America/Jamaica"),
    ("PA", "America/Panama"),
    ("PE", "America/Lima"),
    ("PR", "America/Puerto_Rico"),
    ("UY", "America/Montevideo"),
    ("VE", "America/Caracas"),
    // Africa and Oceania
    ("DZ", "Africa/Algiers"),
    ("EG", "Africa/Cairo"),
    ("ET", "Africa/Addis_Ababa"),
    ("GH", "Africa/Accra"),
    ("KE", "Africa/Nairobi"),
    ("MA", "Africa/Casablanca"),
    ("NG", "Africa/Lagos"),
    ("NZ", "Pacific/Auckland"),
    ("TN", "Africa/Tunis"),
    ("ZA", "Africa/Johannesburg"),
];

/// The main zone of a country that has one (or one that holds nearly everybody).
pub fn main_zone(code: &str) -> Option<&'static str> {
    COUNTRY_ZONES.iter().find(|(c, _)| *c == code).map(|(_, z)| *z)
}

/// The zones `zone.tab` lists for a country (`code<TAB>coordinates<TAB>zone<TAB>comment`), in file order.
pub fn zone_tab_zones(zone_tab: &str, code: &str) -> Vec<String> {
    zone_tab
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut f = l.split('\t');
            let (c, _, zone) = (f.next()?, f.next()?, f.next()?);
            (c == code && is_valid(zone)).then(|| zone.to_string())
        })
        .collect()
}

/// "AT" -> "Austria", from `iso3166.tab`.
pub fn country_name(iso_tab: &str, code: &str) -> Option<String> {
    iso_tab
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| l.split_once('\t').filter(|(c, _)| *c == code).map(|(_, n)| n.trim().to_string()))
}

/// Names tzdata renamed; ROCKNIX's list still has the old one.
const RENAMED: &[(&str, &str)] = &[("Europe/Kyiv", "Europe/Kiev")];

/// `zone` as the picker's list spells it (`available` empty: no list to check against), or `None` when it isn't on it.
fn offered(zone: &str, available: &[String]) -> Option<String> {
    if available.is_empty() || available.iter().any(|a| a == zone) {
        return Some(zone.to_string());
    }
    let old = RENAMED.iter().find(|(new, _)| *new == zone)?.1;
    available.iter().any(|a| a == old).then(|| old.to_string())
}

/// The guess, in this order: the system's zone unless it is UTC; the country of the regulatory domain (its
/// main zone, or its list when it has several); UTC. `available` is the list the picker offers.
pub fn guess(system: Option<&str>, reg_output: Option<&str>, zone_tab: Option<&str>, available: &[String]) -> Guess {
    if let Some(z) = system.filter(|z| is_valid(z) && !is_utc(z)) {
        return Guess::System(z.to_string());
    }
    let Some(code) = reg_output.and_then(parse_reg_country) else {
        return Guess::Utc;
    };
    // Only zones the picker offers: the answer must be something the user could have chosen.
    if let Some(zone) = main_zone(&code).and_then(|z| offered(z, available)) {
        return Guess::Country { code, zone };
    }
    let mut zones: Vec<String> = zone_tab.map(|t| zone_tab_zones(t, &code)).unwrap_or_default();
    zones = zones.iter().filter_map(|z| offered(z, available)).collect();
    zones.sort_by_key(|z| city_of(z).to_lowercase());
    zones.dedup();
    match zones.len() {
        0 => Guess::Utc,
        1 => Guess::Country { zone: zones.remove(0), code },
        _ => Guess::Several { code, zones },
    }
}

/// A zone's rules.
#[derive(Debug, Default, PartialEq)]
pub struct Tz {
    /// (UTC second, index into `types`)
    transitions: Vec<(i64, usize)>,
    /// (offset east of UTC in seconds, is DST)
    types: Vec<(i32, bool)>,
    footer: Option<Posix>,
}

impl Tz {
    pub fn load(zoneinfo: &Path, zone: &str) -> Option<Tz> {
        if !is_valid(zone) {
            return None;
        }
        Tz::parse(&std::fs::read(zoneinfo.join(zone)).ok()?)
    }

    pub fn parse(data: &[u8]) -> Option<Tz> {
        let be32 = |b: &[u8], i: usize| -> Option<i32> { Some(i32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
        if data.get(..4)? != b"TZif" {
            return None;
        }
        let version = *data.get(4)?;
        let counts = |at: usize| -> Option<[usize; 6]> {
            let mut c = [0usize; 6];
            for (i, v) in c.iter_mut().enumerate() {
                *v = be32(data, at + 20 + i * 4)? as usize;
            }
            Some(c) // isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt
        };
        let first = counts(0)?;
        let v1_len = first[3] * 4 + first[3] + first[4] * 6 + first[5] + first[2] * 8 + first[1] + first[0];
        // Version 1 files only have 32-bit times; version 2 and up repeat everything with 64-bit ones.
        let (base, c, wide) = if version >= b'2' {
            (44 + v1_len, counts(44 + v1_len)?, true)
        } else {
            (0, first, false)
        };
        let [isut, isstd, leap, timecnt, typecnt, charcnt] = c;
        let t_size = if wide { 8 } else { 4 };
        let mut at = base + 44;
        let times_at = at;
        at += timecnt * t_size;
        let idx_at = at;
        at += timecnt;
        let types_at = at;
        at += typecnt * 6;
        at += charcnt + leap * (t_size + 4) + isstd + isut;
        let mut types = Vec::new();
        for i in 0..typecnt {
            types.push((be32(data, types_at + i * 6)?, *data.get(types_at + i * 6 + 4)? != 0));
        }
        let mut transitions = Vec::new();
        for i in 0..timecnt {
            let t = if wide {
                i64::from_be_bytes(data.get(times_at + i * 8..times_at + i * 8 + 8)?.try_into().ok()?)
            } else {
                be32(data, times_at + i * 4)? as i64
            };
            transitions.push((t, *data.get(idx_at + i)? as usize));
        }
        // Footer: "\n<POSIX TZ>\n"
        let footer = if wide && data.get(at) == Some(&b'\n') {
            let rest = &data[at + 1..];
            let end = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
            Posix::parse(std::str::from_utf8(&rest[..end]).ok()?)
        } else {
            None
        };
        if types.is_empty() {
            return None;
        }
        Some(Tz { transitions, types, footer })
    }

    /// Seconds east of UTC at the UTC second `t`.
    pub fn offset_at(&self, t: i64) -> i32 {
        if let Some(last) = self.transitions.last() {
            if t >= last.0 {
                if let Some(rule) = &self.footer {
                    return rule.offset_at(t);
                }
            }
        } else if let Some(rule) = &self.footer {
            return rule.offset_at(t);
        }
        let i = self.transitions.partition_point(|(when, _)| *when <= t);
        match i {
            // Before the first transition: the first standard-time type.
            0 => self.types.iter().find(|(_, dst)| !dst).or(self.types.first()).map_or(0, |t| t.0),
            i => self.types.get(self.transitions[i - 1].1).map_or(0, |t| t.0),
        }
    }
}

/// `std offset [dst [offset] [,start[/time],end[/time]]]`
#[derive(Debug, PartialEq)]
struct Posix {
    std: i32,
    dst: Option<(i32, Rule, Rule)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Day {
    /// Mm.w.d: month, week (5 = last), weekday (0 = Sunday)
    Month(u32, u32, u32),
    /// Jn: day 1..=365, February 29 is never counted
    Julian(u32),
    /// n: day 0..=365, February 29 counted in leap years
    Zero(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rule {
    day: Day,
    /// Local seconds after midnight, may be negative or over a day.
    time: i32,
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn year_of(t: i64) -> i64 {
    let z = t.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + if m <= 2 { 1 } else { 0 }
}

impl Rule {
    /// UTC second at which the rule fires in `year`, given the offset in force just before it.
    fn at(&self, year: i64, offset_before: i32) -> i64 {
        let day = match self.day {
            Day::Month(m, w, d) => {
                let first = days_from_civil(year, m, 1);
                let first_wd = (first + 4).rem_euclid(7) as u32; // 1970-01-01 was a Thursday
                let mut day = first + ((d + 7 - first_wd) % 7) as i64 + (w as i64 - 1) * 7;
                let next_month = if m == 12 {
                    days_from_civil(year + 1, 1, 1)
                } else {
                    days_from_civil(year, m + 1, 1)
                };
                while day >= next_month {
                    day -= 7;
                }
                day
            }
            Day::Julian(n) => {
                let leap_shift = (is_leap(year) && n >= 60) as i64;
                days_from_civil(year, 1, 1) + n as i64 - 1 + leap_shift
            }
            Day::Zero(n) => days_from_civil(year, 1, 1) + n as i64,
        };
        day * 86400 + self.time as i64 - offset_before as i64
    }
}

impl Posix {
    fn parse(s: &str) -> Option<Posix> {
        let mut p = Cursor { s: s.as_bytes(), i: 0 };
        p.name()?;
        let std = -p.offset()?;
        if p.done() {
            return Some(Posix { std, dst: None });
        }
        p.name()?;
        let dst_off = if p.peek().is_some_and(|c| c != b',') { -p.offset()? } else { std + 3600 };
        // Without rules POSIX falls back to the US rules; none of the zone files do that.
        if p.next() != Some(b',') {
            return None;
        }
        let start = p.rule()?;
        if p.next() != Some(b',') {
            return None;
        }
        let end = p.rule()?;
        Some(Posix {
            std,
            dst: Some((dst_off, start, end)),
        })
    }

    fn offset_at(&self, t: i64) -> i32 {
        let Some((dst, start, end)) = &self.dst else { return self.std };
        let year = year_of(t + self.std as i64);
        let (s, e) = (start.at(year, self.std), end.at(year, *dst));
        let in_dst = if s < e { t >= s && t < e } else { !(t >= e && t < s) };
        if in_dst {
            *dst
        } else {
            self.std
        }
    }
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn name(&mut self) -> Option<()> {
        if self.peek()? == b'<' {
            while self.next()? != b'>' {}
        } else {
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                self.i += 1;
            }
            if self.i - start < 3 {
                return None;
            }
        }
        Some(())
    }
    fn number(&mut self) -> Option<i32> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
    /// `[+-]hh[:mm[:ss]]` in seconds.
    fn offset(&mut self) -> Option<i32> {
        let sign = match self.peek()? {
            b'-' => {
                self.i += 1;
                -1
            }
            b'+' => {
                self.i += 1;
                1
            }
            _ => 1,
        };
        let mut secs = self.number()? * 3600;
        if self.peek() == Some(b':') {
            self.i += 1;
            secs += self.number()? * 60;
            if self.peek() == Some(b':') {
                self.i += 1;
                secs += self.number()?;
            }
        }
        Some(sign * secs)
    }
    fn rule(&mut self) -> Option<Rule> {
        let day = match self.peek()? {
            b'M' => {
                self.i += 1;
                let m = self.number()? as u32;
                (self.next()? == b'.').then_some(())?;
                let w = self.number()? as u32;
                (self.next()? == b'.').then_some(())?;
                let d = self.number()? as u32;
                Day::Month(m, w, d)
            }
            b'J' => {
                self.i += 1;
                Day::Julian(self.number()? as u32)
            }
            _ => Day::Zero(self.number()? as u32),
        };
        let time = if self.peek() == Some(b'/') {
            self.i += 1;
            self.offset()?
        } else {
            7200
        };
        Some(Rule { day, time })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-21 12:00 UTC, 2026-12-21 12:00 UTC and 2026-07-01 12:00 UTC.
    const OCT: i64 = 1_792_584_000;
    const DEC: i64 = 1_797_854_400;
    const JUL: i64 = 1_782_907_200;

    #[test]
    fn dates_are_what_the_calendar_says() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 10, 21) * 86400 + 12 * 3600, OCT);
        assert_eq!(year_of(OCT), 2026);
        assert_eq!(year_of(-1), 1969);
    }

    #[test]
    fn central_europe_switches_on_the_last_sundays() {
        let p = Posix::parse("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        assert_eq!(p.offset_at(OCT), 7200, "Oct 21: summer time until Sunday Oct 25");
        assert_eq!(p.offset_at(DEC), 3600);
        assert_eq!(p.offset_at(JUL), 7200);
        // 2026-10-25 01:00 UTC is the moment 03:00 CEST turns into 02:00 CET.
        let switch = days_from_civil(2026, 10, 25) * 86400 + 3600;
        assert_eq!(p.offset_at(switch - 1), 7200);
        assert_eq!(p.offset_at(switch), 3600);
    }

    #[test]
    fn new_york_south_america_and_whole_hours_without_dst() {
        let ny = Posix::parse("EST5EDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(ny.offset_at(OCT), -4 * 3600);
        assert_eq!(ny.offset_at(DEC), -5 * 3600);
        // Southern hemisphere: DST spans the new year.
        let sydney = Posix::parse("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(sydney.offset_at(OCT), 11 * 3600);
        assert_eq!(sydney.offset_at(JUL), 10 * 3600);
        let tokyo = Posix::parse("JST-9").unwrap();
        assert_eq!(tokyo.offset_at(OCT), 9 * 3600);
        let india = Posix::parse("IST-5:30").unwrap();
        assert_eq!(india.offset_at(OCT), 19800);
        let named = Posix::parse("<+0330>-3:30").unwrap();
        assert_eq!(named.offset_at(OCT), 12600);
        assert!(Posix::parse("garbage").is_none());
    }

    /// A slim TZif v2 file for Europe/Vienna (what ROCKNIX ships): one transition and the footer.
    fn slim_vienna() -> Vec<u8> {
        let mut v = Vec::new();
        let header = |v: &mut Vec<u8>, counts: [u32; 6]| {
            v.extend(b"TZif2");
            v.extend([0u8; 15]);
            for c in counts {
                v.extend(c.to_be_bytes());
            }
        };
        // v1 block (empty types) then v2 block
        header(&mut v, [0, 0, 0, 0, 1, 4]);
        v.extend(3600i32.to_be_bytes());
        v.extend([0, 0]);
        v.extend(b"CET\0");
        header(&mut v, [0, 0, 0, 1, 2, 9]);
        v.extend((-2_000_000_000i64).to_be_bytes()); // a transition long ago
        v.push(1);
        v.extend(3600i32.to_be_bytes());
        v.extend([0, 0]);
        v.extend(7200i32.to_be_bytes());
        v.extend([1, 4]);
        v.extend(b"CET\0CEST\0");
        v.extend(b"\nCET-1CEST,M3.5.0,M10.5.0/3\n");
        v
    }

    #[test]
    fn reads_a_slim_tzif_file_with_its_footer() {
        let tz = Tz::parse(&slim_vienna()).expect("parses");
        assert_eq!(tz.offset_at(OCT), 7200);
        assert_eq!(tz.offset_at(DEC), 3600);
        assert_eq!(tz.offset_at(-2_100_000_000), 3600, "before the first transition: standard time");
        assert!(Tz::parse(b"nope").is_none());
    }

    #[test]
    fn names_regions_and_search() {
        let zones: Vec<String> = ["Europe/Vienna", "Europe/Berlin", "America/New_York", "America/Argentina/Buenos_Aires", "UTC"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(regions(&zones), ["America", "Europe", "Other"]);
        assert_eq!(zones_in(&zones, "Europe"), ["Europe/Berlin", "Europe/Vienna"], "by city");
        assert_eq!(city_of("America/New_York"), "New York");
        assert_eq!(city_of("America/Argentina/Buenos_Aires"), "Buenos Aires, Argentina");
        assert_eq!(city_of("UTC"), "UTC");
        assert_eq!(search(&zones, "vie"), ["Europe/Vienna"]);
        assert_eq!(search(&zones, "new york"), ["America/New_York"]);
        assert_eq!(search(&zones, "america bue"), ["America/Argentina/Buenos_Aires"]);
        assert!(search(&zones, "  ").is_empty());
        assert_eq!(fmt_offset(7200), "UTC+02:00");
        assert_eq!(fmt_offset(-4 * 3600), "UTC-04:00");
        assert_eq!(fmt_offset(19800), "UTC+05:30");
    }

    #[test]
    fn only_plain_zone_names_pass() {
        assert!(is_valid("Europe/Vienna") && is_valid("America/Port-au-Prince") && is_valid("Etc/GMT+5"));
        for bad in ["", "/etc/passwd", "../x", "Europe/Vienna; reboot", "a b", "$(x)"] {
            assert!(!is_valid(bad), "{bad}");
        }
    }

    // ---- the first start's guess ----

    /// `iw reg get` on the Mini V2 (2026-10-22): the global domain is still "00", the Wi-Fi chip reports AT itself.
    const IW_DEVICE: &str = "global
country 00: DFS-UNSET
\t(755 - 928 @ 2), (N/A, 20), (N/A), PASSIVE-SCAN
\t(2402 - 2472 @ 40), (N/A, 20), (N/A)

phy#0 (self-managed)
country AT: DFS-ETSI
\t(2402 - 2472 @ 40), (N/A, 20), (N/A)
\t(5170 - 5250 @ 80), (N/A, 23), (N/A), NO-OUTDOOR, AUTO-BW
";
    const IW_GLOBAL_US: &str = "global\ncountry US: DFS-FCC\n\t(2402 - 2472 @ 40), (N/A, 30), (N/A)\n";
    const IW_UNSET: &str = "global\ncountry 00: DFS-UNSET\n\t(2402 - 2472 @ 40), (N/A, 20), (N/A)\n";

    const ZONE_TAB: &str = "# comment\nAT\t+4813+01620\tEurope/Vienna\nUS\t+404251-0740023\tAmerica/New_York\tEastern (most areas)\n\
US\t+415100-0873900\tAmerica/Chicago\tCentral (most areas)\nUS\t+211825-1575130\tPacific/Honolulu\tHawaii\n\
BG\t+4241+02319\tEurope/Sofia\nAU\t-3345+15112\tAustralia/Sydney\tNew South Wales\nAU\t-3749+14458\tAustralia/Melbourne\tVictoria\n\
XX\t+0000+00000\tAtlantic/Elsewhere\n";

    fn list(zones: &[&str]) -> Vec<String> {
        zones.iter().map(|z| z.to_string()).collect()
    }

    #[test]
    fn utc_and_its_aliases_count_as_not_set() {
        for z in ["UTC", "Etc/UTC", "GMT", "Etc/GMT", "UCT", "Zulu", "Universal", "Etc/Universal"] {
            assert!(is_utc(z), "{z}");
        }
        for z in ["Europe/London", "Europe/Vienna", "Africa/Abidjan", "Etc/GMT+5"] {
            assert!(!is_utc(z), "{z}");
        }
    }

    #[test]
    fn iw_reg_get_gives_the_country_of_the_wifi_chip() {
        // The chip's own domain wins over the global "00" that comes first.
        assert_eq!(parse_reg_country(IW_DEVICE).as_deref(), Some("AT"));
        assert_eq!(parse_reg_country(IW_GLOBAL_US).as_deref(), Some("US"));
        // "00" (world), "99" and "98" are no country; nor are an empty answer or an error.
        assert_eq!(parse_reg_country(IW_UNSET), None);
        assert_eq!(parse_reg_country("global\ncountry 99: DFS-UNSET\n"), None);
        assert_eq!(parse_reg_country("global\ncountry 98: DFS-UNSET\n"), None);
        assert_eq!(parse_reg_country(""), None);
        assert_eq!(parse_reg_country("command failed: Operation not supported (-95)"), None);
        // The chip's domain beats a different global one.
        assert_eq!(
            parse_reg_country("global\ncountry DE: DFS-ETSI\n\nphy#1\ncountry JP: DFS-JP\n").as_deref(),
            Some("JP")
        );
        assert_eq!(
            parse_reg_country("global\ncountry DE: DFS-ETSI\n\nphy#1\ncountry 00: DFS-UNSET\n").as_deref(),
            Some("DE")
        );
    }

    #[test]
    fn countries_with_one_zone_have_it_and_those_with_several_do_not() {
        assert_eq!(main_zone("AT"), Some("Europe/Vienna"));
        assert_eq!(main_zone("DE"), Some("Europe/Berlin"));
        assert_eq!(main_zone("JP"), Some("Asia/Tokyo"));
        for multi in ["US", "CA", "AU", "BR", "RU", "MX", "ID", "KZ"] {
            assert_eq!(main_zone(multi), None, "{multi} has several zones");
        }
        assert_eq!(zone_tab_zones(ZONE_TAB, "US"), ["America/New_York", "America/Chicago", "Pacific/Honolulu"]);
        assert_eq!(zone_tab_zones(ZONE_TAB, "BG"), ["Europe/Sofia"]);
        assert!(zone_tab_zones(ZONE_TAB, "ZZ").is_empty());
        assert_eq!(country_name("# x\nAT\tAustria\nUS\tUnited States\n", "US").as_deref(), Some("United States"));
        assert_eq!(country_name("AT\tAustria\n", "FR"), None);
    }

    #[test]
    fn the_guess_goes_system_zone_then_country_then_utc() {
        let available = list(&[
            "Europe/Vienna",
            "Europe/Berlin",
            "Europe/Sofia",
            "America/New_York",
            "America/Chicago",
            "Pacific/Honolulu",
            "UTC",
        ]);
        let g = |system, reg, tab| guess(system, reg, tab, &available);
        let at = Guess::Country {
            code: "AT".into(),
            zone: "Europe/Vienna".into(),
        };

        // 1. A zone the system already has wins, even over a Wi-Fi country that says otherwise.
        assert_eq!(g(Some("Europe/Berlin"), Some(IW_DEVICE), Some(ZONE_TAB)), Guess::System("Europe/Berlin".into()));
        // ... unless it is UTC (a device nobody has set up).
        for utc in ["UTC", "Etc/UTC", "GMT"] {
            assert_eq!(g(Some(utc), Some(IW_DEVICE), Some(ZONE_TAB)), at, "{utc}");
        }
        // 2. No system zone at all: the same.
        assert_eq!(g(None, Some(IW_DEVICE), Some(ZONE_TAB)), at);
        // 2b. A country with several zones opens the picker on its own list.
        assert_eq!(
            g(Some("UTC"), Some(IW_GLOBAL_US), Some(ZONE_TAB)),
            Guess::Several {
                code: "US".into(),
                zones: list(&["America/Chicago", "Pacific/Honolulu", "America/New_York"])
            }
        );
        // 2c. A country that is not in the table takes its only zone from zone.tab.
        assert_eq!(
            g(Some("UTC"), Some("global\ncountry BG: DFS-ETSI\n"), Some(ZONE_TAB)),
            Guess::Country {
                code: "BG".into(),
                zone: "Europe/Sofia".into()
            }
        );
        // 3. Nothing known: UTC.
        assert_eq!(g(Some("UTC"), Some(IW_UNSET), Some(ZONE_TAB)), Guess::Utc);
        assert_eq!(g(None, None, None), Guess::Utc);
        assert_eq!(
            g(Some("UTC"), Some("global\ncountry ZZ: DFS-UNSET\n"), Some(ZONE_TAB)),
            Guess::Utc,
            "a country nobody knows"
        );
        // A country whose zones are not on the picker's list is no guess either.
        assert_eq!(g(Some("UTC"), Some("global\ncountry AU: DFS-ETSI\n"), Some(ZONE_TAB)), Guess::Utc);
    }

    #[test]
    fn the_table_is_only_used_for_zones_the_picker_offers() {
        // ROCKNIX's list lacks Europe/Vienna here: the table's answer would be a zone nobody can pick.
        let available = list(&["Europe/Berlin", "UTC"]);
        assert_eq!(guess(Some("UTC"), Some(IW_DEVICE), Some(ZONE_TAB), &available), Guess::Utc);
        // Without a list to check against, the table is trusted.
        assert_eq!(
            guess(Some("UTC"), Some(IW_DEVICE), None, &[]),
            Guess::Country {
                code: "AT".into(),
                zone: "Europe/Vienna".into()
            }
        );
    }

    #[test]
    fn a_renamed_zone_is_found_under_the_name_the_list_has() {
        // ROCKNIX's list (2026-10-22) still says Europe/Kiev.
        let available = list(&["Europe/Kiev", "UTC"]);
        assert_eq!(
            guess(Some("UTC"), Some("global\ncountry UA: DFS-ETSI\n"), None, &available),
            Guess::Country {
                code: "UA".into(),
                zone: "Europe/Kiev".into()
            }
        );
    }

    #[test]
    fn every_table_zone_is_a_plain_region_zone() {
        for (code, zone) in COUNTRY_ZONES {
            assert!(code.len() == 2 && code.chars().all(|c| c.is_ascii_uppercase()), "{code}");
            assert!(is_valid(zone) && zone.contains('/') && !zone.starts_with("Etc/"), "{zone}");
        }
        let mut codes: Vec<_> = COUNTRY_ZONES.iter().map(|(c, _)| *c).collect();
        codes.sort();
        let n = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), n, "a country twice");
    }

    #[test]
    fn clock_in_another_zone() {
        assert_eq!(clock_at(OCT, 7200), "14:00");
        assert_eq!(clock_at(0, -3600), "23:00");
    }
}
