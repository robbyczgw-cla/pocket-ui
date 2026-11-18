//! Earlier states' row (Options → Speed, rumble, fan → Earlier states on Linux, Options → Earlier states in Android's
//! player): its key and where it starts On. One place for both players; Linux's ring itself is `timeline`.

/// The game setting: "on" or "off".
pub const KEY: &str = "timeline";

/// Systems whose states are several MB to tens of MB (docs/HOST.md "Earlier"): Earlier is Off for them until it is
/// switched on for the console or the game. Their states would fill the 100 MB of a game with a handful.
pub const BIG_STATES: &[&str] = &["n64", "psp", "dreamcast", "naomi", "atomiswave", "saturn", "nds"];

/// Whether Earlier starts On for `system`.
pub fn default_on(system: &str) -> bool {
    !BIG_STATES.contains(&system)
}
