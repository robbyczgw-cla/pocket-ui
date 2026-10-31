//! Calendar arithmetic on day numbers (days since 1 January 1970), shared by the play history (`play_history`, which
//! re-exports it) and by Rediscover (`connections`) on a phone, which has no play log.

const DAY: i64 = 86_400;

/// The local day (days since 1 January 1970) of a moment, `tz` seconds east of UTC.
pub fn local_day(unix: u64, tz: i32) -> i64 {
    (unix as i64 + tz as i64).div_euclid(DAY)
}

/// (year, month 1-12, day 1-31) of a day number (Howard Hinnant's civil_from_days).
pub fn civil(day: i64) -> (i32, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400 + i64::from(m <= 2)) as i32;
    (y, m, d)
}

/// The day number of a date (days_from_civil).
pub fn day_of(y: i32, m: u32, d: u32) -> i64 {
    let y = y as i64 - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
