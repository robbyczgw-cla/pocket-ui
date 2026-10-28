//! A few games of hand-made feel tags in the vocabulary's ids (tests only): Echoes tests that should not move when
//! `assets/curatordb.bin` is rebuilt use these instead of the shipped tags.

use crate::gamedb::GameInfo;

use super::features::{FeelSource, FeelTag};

/// (QID, (vocabulary id, weight)).
pub const ENTRIES: &[(&str, &[(&str, u8)])] = &[
    // Burnout 3: Takedown, Burnout Revenge
    ("Q2079604", &[("arcade-racing", 3), ("crash-takedowns", 3), ("traffic", 2), ("pace-frantic", 2), ("adrenaline", 3), ("session-pickup", 2)]),
    ("Q2207176", &[("arcade-racing", 3), ("crash-takedowns", 3), ("traffic", 3), ("pace-frantic", 2), ("adrenaline", 3), ("session-pickup", 2)]),
    // Need for Speed: Hot Pursuit 2, Gran Turismo 4
    ("Q866397", &[("arcade-racing", 3), ("police-chases", 3), ("pace-fast", 2), ("adrenaline", 2), ("session-pickup", 2)]),
    ("Q845695", &[("sim-racing", 3), ("car-tuning", 3), ("pace-fast", 2), ("real-world-sport", 3), ("session-sitdown", 2)]),
    // Chrono Trigger, Chrono Cross, Final Fantasy VI
    ("Q761815", &[("jrpg", 3), ("atb", 3), ("time-travel", 3), ("multiple-endings", 3), ("story-driven", 3), ("pace-calm", 3), ("session-sitdown", 2)]),
    ("Q1088142", &[("jrpg", 3), ("multiple-endings", 3), ("story-driven", 3), ("pace-calm", 3), ("melancholic", 2), ("session-sitdown", 2)]),
    ("Q535940", &[("jrpg", 3), ("atb", 3), ("story-driven", 3), ("party-building", 3), ("pace-calm", 3), ("session-sitdown", 2)]),
    // Super Mario Kart
    ("Q1061560", &[("kart-racing", 3), ("couch-versus", 3), ("pace-fast", 2), ("cheerful", 2), ("session-pickup", 2)]),
];

/// The fixture as a [`FeelSource`].
pub struct Fixture;

impl FeelSource for Fixture {
    fn tags(&self, _system: &str, info: &GameInfo) -> Vec<FeelTag> {
        let vocab = crate::curatordb::vocabulary();
        ENTRIES
            .iter()
            .find(|(q, _)| *q == info.qid)
            .map(|(_, tags)| tags.iter().filter_map(|(id, w)| FeelTag::by_id(vocab, id, *w as f32)).collect())
            .unwrap_or_default()
    }
}

#[test]
fn every_fixture_tag_is_in_the_vocabulary() {
    let vocab = crate::curatordb::vocabulary();
    for (q, tags) in ENTRIES {
        for (id, _) in *tags {
            assert!(vocab.id(id).is_some(), "{q}: {id}");
        }
    }
}
