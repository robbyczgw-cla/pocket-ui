//! "34 of 60 achievements · 57%" where "the case knows where you are" has nothing to say: a game whose save pocket-ui
//! can't read (all but `case_knows::Release::verified()`) shows the player's RetroAchievements tally on the back of its
//! box, on its card, beside Continue and on "Where was I?". The line from the game's own save always comes first.
//!
//! The numbers are the game's kept `Summary` (`<data>/achievements/games/<system>/<rom>.json`, written by pocket-core
//! after playing and by `lookup`). Only softcore counts: RetroAchievements' softcore tally, which a Hardcore unlock is
//! part of (`api::all_progress`). Shared by Linux and Android's handheld view; the App asks [`ask`] before it looks a
//! game up, so nothing goes out without Wi-Fi.

use super::snapshot::Summary;

/// The line in English, as the tables translate it ("34 of 60 achievements · 56%"): none for a game without a set, or
/// with nothing unlocked yet (never "0 of 60" for a game not played with achievements).
pub fn english(s: &Summary) -> Option<String> {
    if s.game_id == 0 || s.total == 0 || s.unlocked == 0 {
        return None;
    }
    let unlocked = s.unlocked.min(s.total);
    Some(format!("{unlocked} of {} achievements · {}%", s.total, percent(unlocked, s.total)))
}

/// Whole percent, rounded down, so only a set with every achievement unlocked says 100%.
pub fn percent(unlocked: u32, total: u32) -> u32 {
    (u64::from(unlocked.min(total)) * 100 / u64::from(total.max(1))) as u32
}

/// The line in the interface's language, its two parts joined as the case-knows line joins its parts (" · "), so its
/// fitting and wrapping (`case_knows_read::fit`, `lines`) apply as they are.
pub fn line(s: &Summary) -> Option<String> {
    english(s).map(|e| crate::i18n::tr(&e))
}

/// Whether the App asks RetroAchievements about a game now: signed in, on Wi-Fi (Android: unmetered), and the kept
/// numbers too old (`lookup::wanted`) or the game just ended (`ended`). Never without Wi-Fi, whatever else.
pub fn ask(signed_in: bool, wifi: bool, wanted: bool, ended: bool) -> bool {
    signed_in && wifi && (wanted || ended)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{tr_in, Lang};

    fn tally(unlocked: u32, total: u32) -> Summary {
        Summary { game_id: 7, total, unlocked, ..Default::default() }
    }

    #[test]
    fn the_line_says_how_many_and_how_far() {
        assert_eq!(english(&tally(34, 60)).unwrap(), "34 of 60 achievements · 56%");
        assert!(english(&tally(59, 60)).unwrap().ends_with("· 98%"), "rounded down: only a full set says 100%");
        assert!(english(&tally(60, 60)).unwrap().ends_with("· 100%"));
        assert_eq!(english(&tally(1, 300)).unwrap(), "1 of 300 achievements · 0%");
        // More unlocked than the set has now (a set made smaller): never above the set.
        assert_eq!(english(&tally(70, 60)).unwrap(), "60 of 60 achievements · 100%");
    }

    /// No line for a game with nothing unlocked, without a set, or unknown to RetroAchievements.
    #[test]
    fn no_line_without_an_unlock_or_a_set() {
        assert_eq!(line(&tally(0, 60)), None);
        assert_eq!(line(&tally(0, 0)), None);
        assert_eq!(line(&Summary { game_id: 0, total: 60, unlocked: 5, ..Default::default() }), None);
        assert_eq!(line(&Summary::default()), None);
    }

    #[test]
    fn nothing_is_asked_without_wifi_or_a_sign_in() {
        for (wanted, ended) in [(true, false), (false, true), (true, true)] {
            assert!(!ask(true, false, wanted, ended), "no Wi-Fi: never");
            assert!(!ask(false, true, wanted, ended), "signed out: never");
            assert!(ask(true, true, wanted, ended));
        }
        assert!(!ask(true, true, false, false), "fresh numbers are not asked again");
    }

    /// Every language says it in its own words, the numbers and the two parts kept, with its own way with "%".
    #[test]
    fn the_line_is_translated_in_every_language() {
        let en = english(&tally(1234, 2000)).unwrap();
        for (lang, says) in [
            (Lang::De, "1234 von 2000 Erfolgen · 61\u{a0}%"),
            (Lang::Fr, "1234 sur 2000 succès · 61\u{a0}%"),
            (Lang::Es, ""),
            (Lang::Pt, ""),
            (Lang::It, ""),
        ] {
            let t = tr_in(lang, &en);
            assert_ne!(t, en, "{lang:?}: not translated");
            assert!(!t.contains("achievements") && t.contains(" · ") && t.ends_with('%') && t.contains("61"), "{lang:?}: {t}");
            if !says.is_empty() {
                assert_eq!(t, says, "{lang:?}");
            }
        }
        assert_eq!(tr_in(Lang::De, &english(&tally(34, 60)).unwrap()), "34 von 60 Erfolgen · 56\u{a0}%");
        assert_eq!(tr_in(Lang::En, &en), en);
    }
}
