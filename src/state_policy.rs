//! Raw-state size exceptions shared by the Linux and Android players.
/// Only adapters known to bound their reader by the supplied length belong here.
/// PPSSPP's pinned adapter ignores that length, so it never gets a shorter file as it is: it goes through the padding
/// path below instead.
pub fn core_states_vary(core_id: &str) -> bool {
    matches!(core_id, "mgba" | "nestopia")
}

/// The shortest state that is handed over padded (`Core::pad_short_states`): real states of these cores are tens of MB.
pub const PADDED_STATE_MIN: usize = 1 << 20;

/// Cores whose `retro_serialize_size` is not the exact length of the state they write, so a state saved earlier is
/// shorter than the size asked at the next start. Such a state is handed over padded with zeros to the size now
/// (`Core::pad_short_states`): the core still reads inside the buffer, which a bare shorter file would not guarantee.
/// - Flycast: the size moves while a game runs (53217183 bytes at the start, 36439543 at quit).
/// - PPSSPP: before its renderer is ready (the first frames after the start) the size is a fixed 134217728 bytes and
///   `retro_serialize` refuses; later it is the measured state rounded up to 8 MiB (41943040 for Bomberman). A state
///   written at quit is the later size, the start asks the earlier one (libretro/libretro.cpp, `retro_serialize_size`).
pub fn core_pads_short_states(core_id: &str) -> bool {
    matches!(core_id, "flycast" | "ppsspp")
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

    #[test]
    fn cores_with_an_upper_bound_size_get_padded_states_and_the_rest_do_not() {
        for core in ["ppsspp", "flycast"] {
            assert!(super::core_pads_short_states(core), "{core}");
        }
        for core in ["mgba", "nestopia", "mupen64plus_next", "melondsds", "pcsx_rearmed", "unknown", ""] {
            assert!(!super::core_pads_short_states(core), "{core}");
        }
    }
}
