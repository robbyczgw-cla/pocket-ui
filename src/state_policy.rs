//! Raw-state size exceptions shared by the Linux and Android players.
/// Only adapters known to bound their reader by the supplied length belong here.
/// PPSSPP's pinned adapter ignores that length; Flycast uses a separate guarded
/// padding path on Linux, and is not bundled in the Android APK.
pub fn core_states_vary(core_id: &str) -> bool {
    matches!(core_id, "mgba" | "nestopia")
}

#[cfg(test)]
mod tests {
    #[test]
    fn short_states_require_a_length_bounded_adapter() {
        for core in ["mgba", "nestopia"] {
            assert!(super::core_states_vary(core));
        }
        for core in ["ppsspp", "flycast", "mupen64plus_next", "melondsds", "unknown", ""] {
            assert!(!super::core_states_vary(core), "{core}");
        }
    }
}
