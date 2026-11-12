//! What a game shows of its achievements, without a screen of its own: the unlock cards one after the other (each waits
//! a moment for its badge), and the notes ("Achievements: 12 of 40"). The Android player paints them over the game
//! (`android/src/player_achievements.rs`); pocket-core keeps the same line in its HUD (`overlay`).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::session::Notice;
use crate::image::Image;

/// How long an unlock card stays, and how long it waits for its badge before it shows without one.
pub const CARD_FOR: Duration = Duration::from_millis(4000);
pub const BADGE_WAIT: Duration = Duration::from_millis(900);

/// One unlock, waiting for or on the screen.
pub struct Card {
    pub id: u32,
    pub title: String,
    pub points: u32,
    pub badge_name: String,
    pub badge: Option<Image>,
    /// The badge couldn't be had: show without waiting.
    no_badge: bool,
    queued: Instant,
    shown: Option<Instant>,
}

#[derive(Default)]
pub struct Cards {
    line: VecDeque<Card>,
    notes: Vec<String>,
}

impl Cards {
    /// Takes what the session has to say: unlocks join the line, notes wait for [`Cards::take_notes`].
    pub fn feed(&mut self, notices: Vec<Notice>, now: Instant) {
        for n in notices {
            match n {
                Notice::Unlocked { id, title, points, badge } => self.line.push_back(Card {
                    id,
                    title,
                    points,
                    no_badge: badge.is_empty(),
                    badge_name: badge,
                    badge: None,
                    queued: now,
                    shown: None,
                }),
                Notice::Badge { name, image } => {
                    for c in self.line.iter_mut().filter(|c| c.badge_name == name && c.badge.is_none()) {
                        match &image {
                            Some(i) => c.badge = Some(Image { w: i.w, h: i.h, px: i.px.clone() }),
                            None => c.no_badge = true,
                        }
                    }
                }
                Notice::Ready { unlocked, total } if total > 0 => self.notes.push(format!("Achievements: {unlocked} of {total}")),
                Notice::Ready { .. } => {}
                Notice::Completed { .. } => self.notes.push("Every achievement of this game unlocked".into()),
                Notice::SignInAgain => self.notes.push("RetroAchievements: sign in again in Settings".into()),
            }
        }
    }

    /// The notes since the last call, in English as `i18n::tr` takes them (its patterns fill in the numbers).
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// The card on the screen now, if any. A card that has had its time goes, the next comes up when its badge is there.
    pub fn current(&mut self, now: Instant) -> Option<&Card> {
        if self.line.front().is_some_and(|c| c.shown.is_some_and(|t| now.saturating_duration_since(t) > CARD_FOR)) {
            self.line.pop_front();
        }
        let c = self.line.front_mut()?;
        if c.shown.is_none() {
            if c.badge.is_none() && !c.no_badge && now.saturating_duration_since(c.queued) < BADGE_WAIT {
                return None;
            }
            c.shown = Some(now);
        }
        self.line.front()
    }

    /// Cards waiting or showing (the overlay needs a repaint while this changes, and a wake-up for the card's end).
    pub fn pending(&self) -> usize {
        self.line.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlocked(id: u32, badge: &str) -> Notice {
        Notice::Unlocked { id, title: format!("Achievement {id}"), points: 5, badge: badge.into() }
    }

    fn pixel() -> Image {
        Image { w: 1, h: 1, px: vec![0xff00ff] }
    }

    #[test]
    fn cards_come_one_after_the_other_each_for_its_time() {
        let t0 = Instant::now();
        let mut c = Cards::default();
        c.feed(vec![unlocked(1, ""), unlocked(2, "")], t0);
        assert_eq!(c.current(t0).map(|c| c.id), Some(1), "no badge to wait for: at once");
        assert_eq!(c.current(t0 + CARD_FOR - Duration::from_millis(1)).map(|c| c.id), Some(1));
        assert_eq!(c.current(t0 + CARD_FOR + Duration::from_millis(1)).map(|c| c.id), Some(2));
        assert!(c.current(t0 + CARD_FOR * 3).is_none(), "the second has had its time too");
        assert_eq!(c.pending(), 0);
    }

    #[test]
    fn a_card_waits_a_moment_for_its_badge_and_then_shows_without() {
        let t0 = Instant::now();
        let mut c = Cards::default();
        c.feed(vec![unlocked(7, "00234")], t0);
        assert!(c.current(t0).is_none(), "the badge is on its way");
        c.feed(vec![Notice::Badge { name: "00234".into(), image: Some(pixel()) }], t0);
        let card = c.current(t0 + Duration::from_millis(100)).expect("the badge came");
        assert!(card.badge.is_some());

        let mut c = Cards::default();
        c.feed(vec![unlocked(8, "00999")], t0);
        assert!(c.current(t0 + BADGE_WAIT - Duration::from_millis(1)).is_none());
        let card = c.current(t0 + BADGE_WAIT + Duration::from_millis(1)).expect("shown without the badge");
        assert!(card.badge.is_none());

        let mut c = Cards::default();
        c.feed(vec![unlocked(9, "00555"), Notice::Badge { name: "00555".into(), image: None }], t0);
        assert_eq!(c.current(t0).map(|c| c.id), Some(9), "a badge that couldn't be had doesn't hold the card");
    }

    #[test]
    fn notes_say_how_many_and_when_the_game_is_done() {
        let mut c = Cards::default();
        c.feed(
            vec![
                Notice::Ready { unlocked: 12, total: 40 },
                Notice::Ready { unlocked: 0, total: 0 },
                Notice::Completed { title: "Game".into() },
                Notice::SignInAgain,
            ],
            Instant::now(),
        );
        assert_eq!(
            c.take_notes(),
            vec!["Achievements: 12 of 40", "Every achievement of this game unlocked", "RetroAchievements: sign in again in Settings"]
        );
        assert!(c.take_notes().is_empty());
        assert_eq!(crate::i18n::german().translate("Achievements: 12 of 40").as_deref(), Some("Erfolge: 12 von 40"));
    }

    #[test]
    fn the_notes_have_german_and_spanish_lines() {
        for line in ["Achievements: 12 of 40", "Every achievement of this game unlocked", "RetroAchievements: sign in again in Settings"] {
            assert!(crate::i18n::german().translate(line).is_some(), "German: {line}");
            assert!(crate::i18n::Lang::Es.table().and_then(|t| t.translate(line)).is_some(), "Spanish: {line}");
        }
    }
}
