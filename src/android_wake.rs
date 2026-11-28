//! Waking an Android handheld in a game (the Android player, android/src/player.rs): as on pocket-ui OS, a game whose screen
//! went off comes back going on, and a pause menu that was open stays open (docs/SLEEP.md, "Wake"). Android pauses the
//! player for anything that covers it (Home, the notification shade, another app): those come back paused, as before.
//! Pure, so a host test covers it; the Activity says whether the screen went off (`PowerManager.isInteractive`).

/// What the player knows when it comes to the front again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Back {
    /// The screen went off while the game was in front (the power key, the screen timeout).
    pub screen_was_off: bool,
    /// The pause menu (or any page of it) was open when the player went to the background.
    pub menu_was_open: bool,
    /// A state that didn't load waits for the player's answer in the menu (`recovery_required`).
    pub recovery: bool,
}

/// Whether the game goes on by itself (no pause menu) now that the player is in front again.
pub fn game_goes_on(back: Back) -> bool {
    back.screen_was_off && !back.menu_was_open && !back.recovery
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_whose_screen_went_off_goes_on_after_waking_as_on_pocket_ui_os() {
        assert!(game_goes_on(Back { screen_was_off: true, ..Back::default() }));
    }

    #[test]
    fn an_open_pause_menu_stays_open_and_home_or_another_app_still_pause() {
        assert!(!game_goes_on(Back { screen_was_off: true, menu_was_open: true, recovery: false }), "the menu was open");
        assert!(!game_goes_on(Back { screen_was_off: false, ..Back::default() }), "Home, the shade, another app");
        assert!(!game_goes_on(Back { screen_was_off: true, menu_was_open: false, recovery: true }), "a state to answer for");
    }
}
