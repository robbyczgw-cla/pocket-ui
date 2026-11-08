//! Portable display of accumulated playing time.

/// One wall clock for recorded history and relative display; test overrides require hardware mocks.
pub fn now_secs() -> u64 {
    // Snapshot fixtures and their relative history share one explicit clock. Cache the optional
    // override once; a real device never accepts it without the existing hardware mock switch.
    static MOCK_NOW: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    if let Some(now) = *MOCK_NOW.get_or_init(|| {
        std::env::var_os("POCKETUI_MOCK").and_then(|_| {
            std::env::var("POCKETUI_MOCK_NOW").ok().map(|s| s.parse().expect("POCKETUI_MOCK_NOW must be Unix seconds"))
        })
    }) {
        return now;
    }
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "under a minute", "12 min", "1 h 05 min".
pub fn duration(secs: u64) -> String {
    let m = secs / 60;
    match m {
        0 => "under a minute".into(),
        1..=59 => format!("{m} min"),
        _ => format!("{} h {:02} min", m / 60, m % 60),
    }
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
pub fn ago(then: u64) -> String {
    ago_from(then, now_secs())
}

/// `ago` as seen at `now` (Unix seconds).
pub fn ago_from(then: u64, now: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        _ if d < 2 * 86_400 => "yesterday".into(),
        _ => format!("{} days ago", d / 86_400),
    }
}
