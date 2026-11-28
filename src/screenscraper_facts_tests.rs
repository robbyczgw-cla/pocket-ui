use super::*;
use crate::testutil::TestDir;

fn fixture() -> Facts {
    parse(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/screenscraper/jeuInfos-facts.json")).unwrap())
}

#[test]
fn an_answer_gives_the_description_in_our_languages_the_main_genre_and_clean_text() {
    let f = fixture();
    let langs: Vec<&str> = f.synopsis.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(langs, ["en", "de", "fr"], "only the languages pocket-ui speaks");
    assert_eq!(f.text_in("en"), Some("Run the old farm. Plant, harvest and raise a family. The \"best\" part: any way you want!"), "entities decoded, line breaks one space");
    assert_eq!(f.genre_in("de"), Some("Simulation"), "the main genre, not the first one listed");
    assert_eq!(f.genre_in("es"), Some("Simulación"));
    assert_eq!(f.players, "1-4");
    assert_eq!((f.developer.as_str(), f.publisher.as_str()), ("Field Works", "Barn & Co"));
    assert_eq!(f.rating, Some(18));
    assert_eq!(f.released.len(), 4);
}

#[test]
fn the_description_comes_in_the_players_language_else_in_english() {
    let f = fixture();
    assert!(f.text_in("de").unwrap().starts_with("Bewirtschafte"));
    assert!(f.text_in("fr").unwrap().starts_with("Reprends"));
    for lang in ["ko", "ja", "zh", "vi", "it", "pt"] {
        assert!(f.text_in(lang).unwrap().starts_with("Run the old farm"), "{lang}: English when ScreenScraper has none in it");
    }
}

#[test]
fn the_year_is_the_roms_regions_and_differing_years_are_named_per_region() {
    let f = fixture();
    assert_eq!(f.year_for("Farm Days (Europe)"), Some(2004));
    assert_eq!(f.year_for("Farm Days (USA)"), Some(2003));
    assert_eq!(f.year_for("Farm Days (Japan)"), Some(2003));
    assert_eq!(f.year_for("Farm Days"), Some(2003), "no tag: world, then the US");
    assert_eq!(f.released_line().as_deref(), Some("JP/US 2003 · EU/FR 2004"));
    let same = Facts { released: vec![("us".into(), 1991), ("eu".into(), 1991)], ..Facts::default() };
    assert_eq!(same.released_line(), None, "one year everywhere: the Year says it");
}

#[test]
fn an_empty_card_gets_every_fact_and_the_text_with_its_source() {
    let mut d = CardData::default();
    assert!(fill_card(&mut d, &fixture(), "de", "Farm Days (Europe)", &[]));
    assert_eq!(d.year, Some(2004));
    assert_eq!(d.players.as_deref(), Some("1–4 players"));
    let facts: Vec<(&str, &str)> = d.facts.iter().map(|(k, v)| (*k, v.as_str())).collect();
    assert_eq!(facts, [("Released", "JP/US 2003 · EU/FR 2004"), ("Developer", "Field Works"), ("Publisher", "Barn & Co"), ("Genre", "Simulation"), ("Community rating", "18 of 20 on ScreenScraper")]);
    assert_eq!(d.scores, None, "named apart from reviews and from the player's own rating");
    assert!(matches!(&d.summary, Summary::ScreenScraper(t) if t.starts_with("Bewirtschafte")));
    assert_eq!(d.summary.note().as_deref(), Some("Text: ScreenScraper.fr, CC BY-NC-SA 4.0"), "the source is named under the text");
    assert!(d.from_screenscraper, "the card names ScreenScraper under the facts");
    assert_eq!(crate::game_card_model::SCREENSCRAPER_CREDIT, crate::screenscraper::CREDIT, "one credit line, the settings row's");
    assert!(!fill_card(&mut d, &fixture(), "de", "Farm Days (Europe)", &[]), "a second time changes nothing (no fact twice)");
}

#[test]
fn what_the_card_has_stays_and_the_players_own_gamelist_text_too() {
    let mut d = CardData {
        year: Some(2001),
        players: Some("2 players".into()),
        facts: vec![("Developer", "Our Developer".into()), ("Genre", "Farming".into())],
        scores: Some("4 of 5 in EmulationStation".into()),
        summary: Summary::Gamelist("The player's own text.".into()),
        ..CardData::default()
    };
    fill_card(&mut d, &fixture(), "de", "Farm Days (Europe)", &[]);
    assert_eq!(d.year, Some(2001));
    assert_eq!(d.players.as_deref(), Some("2 players"));
    assert_eq!(d.scores.as_deref(), Some("4 of 5 in EmulationStation"));
    assert_eq!(d.summary, Summary::Gamelist("The player's own text.".into()));
    let get = |k: &str| d.facts.iter().find(|(l, _)| *l == k).map(|(_, v)| v.clone());
    assert_eq!(get("Developer").as_deref(), Some("Our Developer"), "never overwritten");
    assert_eq!(get("Genre").as_deref(), Some("Farming"), "never overwritten");
    assert_eq!(get("Publisher").as_deref(), Some("Barn & Co"), "what was missing is added");
    assert_eq!(get("Community rating"), None, "reviews known: no second rating");
}

#[test]
fn the_text_in_the_players_language_comes_before_wikipedias_english_one() {
    for before in [Summary::Wikipedia { text: "English lead.".into(), title: "Farm Days".into() }, Summary::Loading, Summary::Unavailable("offline".into()), Summary::Romm("RomM's.".into()), Summary::None] {
        let mut d = CardData { summary: before.clone(), ..CardData::default() };
        fill_card(&mut d, &fixture(), "de", "Farm Days", &[]);
        assert!(matches!(&d.summary, Summary::ScreenScraper(t) if t.starts_with("Bewirtschafte")), "{before:?}");
    }
    // Without any description from ScreenScraper the card keeps what it has.
    let none = Facts { developer: "Field Works".into(), ..Facts::default() };
    let mut d = CardData { summary: Summary::Wikipedia { text: "English lead.".into(), title: "Farm Days".into() }, ..CardData::default() };
    fill_card(&mut d, &none, "de", "Farm Days", &[]);
    assert!(matches!(d.summary, Summary::Wikipedia { .. }));
}

#[test]
fn facts_are_kept_per_game_and_a_broken_or_missing_file_is_no_facts() {
    let dir = TestDir::new("screenscraper-facts");
    let rom = Path::new("/roms/gba/Farm Days (Europe).gba");
    assert_eq!(load(&dir, "gba", rom), None, "never asked");
    save(&dir, "gba", rom, &Facts::default());
    assert!(!file(&dir, "gba", rom).exists(), "an answer without facts keeps no file");
    save(&dir, "gba", rom, &fixture());
    assert_eq!(load(&dir, "gba", rom), Some(fixture()));
    assert_eq!(load(&dir, "snes", rom), None, "another system's game of the same name");
    std::fs::write(file(&dir, "gba", rom), b"{\"synopsis\": [[\"en\", \"cut off").unwrap();
    assert_eq!(load(&dir, "gba", rom), None, "a broken file is no facts, no panic");
}

#[test]
fn an_error_page_or_a_game_without_facts_gives_none() {
    assert!(parse(b"Erreur : Jeu non trouve !").is_empty());
    assert!(parse(br#"{"response":{"jeu":{"id":"1","joueurs":{"text":"many"},"note":{"text":"0"},"dates":[{"region":"us","text":"19xx"}]}}}"#).is_empty(), "a word for players, a 0 rating and a broken date are no facts");
    assert!(parse(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/screenscraper/jeuInfos-found.json")).unwrap()).is_empty());
}

#[test]
fn a_long_description_ends_after_a_whole_sentence() {
    let long = format!(r#"{{"response":{{"jeu":{{"synopsis":[{{"langue":"en","text":"{}"}}]}}}}}}"#, "A sentence of the game. ".repeat(400));
    let f = parse(long.as_bytes());
    let t = f.text_in("en").unwrap();
    assert!(t.chars().count() <= MAX_TEXT && t.ends_with('.'), "{} chars", t.chars().count());
}

#[test]
fn our_english_genre_reads_in_the_players_language_and_the_players_own_stays() {
    // pocket-ui's offline list says it in English (Wikidata); ScreenScraper has it in German.
    let ours = vec![("Genre", "Simulation game".to_string()), ("Developer", "Field Works".to_string())];
    let mut d = CardData { facts: ours.clone(), ..CardData::default() };
    fill_card(&mut d, &fixture(), "de", "Farm Days", &ours);
    assert!(d.facts.contains(&("Genre", "Simulation".into())), "{:?}", d.facts);
    assert_eq!(d.facts.iter().filter(|(l, _)| *l == "Genre").count(), 1);
    // English, or a language ScreenScraper has no genre in: ours stays.
    for lang in ["en", "ko"] {
        let mut d = CardData { facts: ours.clone(), ..CardData::default() };
        fill_card(&mut d, &fixture(), lang, "Farm Days", &ours);
        assert!(d.facts.contains(&("Genre", "Simulation game".into())), "{lang}");
    }
    // The player's gamelist genre (not in ours) is never replaced.
    let mut d = CardData { facts: vec![("Genre", "Landwirtschaft".into())], ..CardData::default() };
    fill_card(&mut d, &fixture(), "de", "Farm Days", &ours);
    assert!(d.facts.contains(&("Genre", "Landwirtschaft".into())));
}
