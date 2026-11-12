//! Compare the real name painters with their literal-language reference. The reference uses
//! exactly the same fitting/truncation, so this checks names rather than prescribing layout.
use pocket_ui::{cabinet_art::Look, game_card::CardData, gamedb::GameDb, gfx::Canvas, i18n::{self, Lang}, media::{self, Medium}, text::{Face, Text}, title_block};
use std::collections::BTreeSet;
static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn corpus() -> BTreeSet<String> {
    let mut names = BTreeSet::from(["Golden Sun".into(), "Game Boy".into(), "Game Boy Advance".into(), "Game Boy Color".into(), "Black: Sun".into(), "Time to Play".into()]);
    let mut db = GameDb::embedded();
    let systems: Vec<_> = db.systems().into_iter().map(str::to_string).collect();
    for system in systems {
        for game in &db.system(&system).unwrap().games {
            names.insert(game.label.clone());
            names.extend(game.aliases.iter().cloned());
        }
    }
    // Read fixture declarations without loading a library: load() teaches names globally,
    // which would hide metadata titles encountered before a library has learned them.
    let out = std::process::Command::new("python3").args(["-I", "tests/name-corpus.py"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for line in String::from_utf8(out.stdout).unwrap().lines() {
        let path = std::path::Path::new(line);
        let rom = path.extension().and_then(|s| s.to_str()).is_some_and(|ext| ["gb", "gbc", "gba", "sfc", "smc", "zip", "z64", "n64", "nds", "3ds", "cia", "nes", "md", "bin", "gen", "chd", "cue", "m3u", "iso", "cso", "rvz", "sh", "ps3", "wua", "wud", "rpx", "xiso", "sms", "gg", "pce", "ngp"].contains(&ext));
        let stem = if rom { path.file_stem().and_then(|s| s.to_str()).unwrap_or(line) } else { line };
        names.insert(pocket_ui::library::display_name(stem));
    }
    for part in include_str!("../deploy/brick/es_systems.cfg").split("<fullname>").skip(1) {
        names.insert(part.split("</fullname>").next().unwrap().into());
    }
    for console in pocket_ui::consoles::all() {
        names.insert(console.name.clone());
        names.extend(console.aka.iter().cloned());
    }
    names.retain(|n| !n.is_empty());
    names
}

fn paint(text: &mut Text, title: &str, path: usize, buf: &mut [u32]) {
    buf.fill(0);
    let mut c = Canvas::new(buf, 240, 240);
    match path {
        0 => pocket_ui::game_info::paint_title_card(&mut c, text, &CardData { title: title.into(), ..Default::default() }, None, (0, 0, 120, 180)),
        1 => {
            let (px, lines) = text.card_name(title, 100);
            for (i, line) in lines.iter().enumerate() {
                let w = text.literal(|text| text.width(Face::Title, px, line));
                text.draw_name(&mut c, Face::Title, px, 120 - w / 2, 50 + i as i32 * 44, 0xffffff, line);
            }
        }
        2 => {
            let p = media::spine(text, Medium::Gb, 0.5, Look { color: 0x555555, band: 0x777777, plastic: 0xaaaaaa }, title);
            for (to, from) in c.buf.iter_mut().zip(p.px) { *to = from; }
        }
        3 => {
            let lines = title_block::name_layout_in(text, Face::Title, 50, title, 160, 170);
            title_block::paint(text, &mut c, Face::Title, &lines, 120, 55, 0xffffff, 0xaaaaaa, true);
        }
        _ => {
            let p = media::front(text, Medium::GenericCart, 0.5, Look { color: 0x555555, band: 0x777777, plastic: 0xaaaaaa }, title, None, None, title);
            for (to, from) in c.buf.iter_mut().zip(p.px) { *to = from; }
        }
    }
}

#[test]
fn every_database_fixture_and_system_name_stays_literal_in_de_and_es() {
    let _language = LANGUAGE.lock().unwrap();
    let names = corpus();
    for name in &names { assert_eq!(media::printed_title(name, true), name.as_str()); }
    eprintln!("name corpus: {} names, 5 painters, DE + ES", names.len());

    for lang in [Lang::De, Lang::Es] {
        i18n::set(lang);
        lang.table().unwrap();
        let mut actual = Text::builtin();
        let mut reference = Text::builtin();
        reference.set_translate(false);
        let mut a = vec![0; 240 * 240];
        let mut b = a.clone();
        for name in ["Black", "Sun", "Golden Sun", "Game Boy Advance", "Black: Sun"].into_iter().chain(names.iter().map(String::as_str)) {
            assert_eq!(actual.fit_name(Face::Title, 37, name, 100), reference.fit_name(Face::Title, 37, name, 100), "compact metadata name: {lang:?}: {name}");
            assert_eq!(actual.wrap_name(Face::Title, 37, name, 100), reference.wrap_name(Face::Title, 37, name, 100), "wrapped metadata name: {lang:?}: {name}");
            for path in 0..5 {
                paint(&mut actual, name, path, &mut a);
                paint(&mut reference, name, path, &mut b);
                assert!(a == b, "translated name: {lang:?} path {path}: {name}");
                assert!(actual.translating(), "name painter changed its caller");
                assert!(!reference.translating(), "literal caller changed");
            }
        }
        let mut ui = i18n::Cache::default();
        assert_ne!(&*ui.get("Settings"), "Settings", "our UI still translates");
        assert_ne!(&*ui.get("Sun"), "Sun", "a title fragment must not teach the UI a weekday name");
    }
}

#[test]
fn wrapped_names_keep_their_language_at_both_fonts_and_text_sizes() {
    use pocket_ui::{font_style::FontStyle, type_scale::{self, TextSize}};
    let _language = LANGUAGE.lock().unwrap();
    for lang in [Lang::De, Lang::Es] {
        i18n::set(lang);
        lang.table().unwrap();
        for font in [FontStyle::Barlow, FontStyle::Retro] {
            for size in [TextSize::Normal, TextSize::Large] {
                type_scale::set(size);
                let mut actual = Text::builtin().pinned_to(font);
                let mut reference = Text::builtin().pinned_to(font);
                reference.set_translate(false);
                let mut a = vec![0; 240 * 240];
                let mut b = a.clone();
                for name in ["Golden Sun", "Game Boy Advance", "Game Boy Color", "HOME Pocket - Black"] {
                    assert_eq!(actual.literal(|text| text.translated(name).to_string()), name);
                    for path in 0..5 {
                        paint(&mut actual, name, path, &mut a);
                        paint(&mut reference, name, path, &mut b);
                        assert!(a == b, "{lang:?} {font:?} {size:?} path {path}: {name}");
                        assert!(actual.translating());
                        assert!(!reference.translating());
                    }
                }
            }
        }
    }
    type_scale::set(TextSize::Normal);
}

#[test]
fn our_labels_on_cases_still_translate_while_names_with_the_same_words_do_not() {
    use pocket_ui::shelf::Item;
    let _language = LANGUAGE.lock().unwrap();
    for item in [Item::Game(0, 0), Item::Recent(0), Item::System(0), Item::SmartShelf(0), Item::Sticker(1)] {
        assert!(item.title_is_name());
    }
    for item in [Item::Favorites, Item::Search, Item::JustPlay, Item::Streaming] {
        assert!(!item.title_is_name());
    }
    let look = Look { color: 0x555555, band: 0x777777, plastic: 0xaaaaaa };
    for (lang, labels) in [
        (Lang::De, [("Search", "Suchen"), ("Favourites", "Favoriten"), ("Just Play", "Einfach spielen")]),
        (Lang::Es, [("Search", "Buscar"), ("Favourites", "Favoritos"), ("Just Play", "Jugar ya")]),
    ] {
        i18n::set(lang);
        lang.table().unwrap();
        let mut text = Text::builtin();
        for (source, translated) in labels {
            // The same word can be a game name or our UI wording. Its origin decides.
            i18n::learn_names([source]);
            assert_eq!(media::printed_title(source, true), source);
            let label = media::printed_title(source, false);
            assert_eq!(label, translated, "our own UI label in {lang:?}");
            let actual = media::spine(&mut text, Medium::Keep, 0.5, look, &label);
            let expected = media::spine(&mut text, Medium::Keep, 0.5, look, translated);
            assert!(actual.px == expected.px, "rendered UI label in {lang:?}: {source}");
            assert!(text.translating());
        }
    }
}

#[test]
fn mixed_card_metadata_and_the_no_match_choice_still_translate_our_words() {
    use pocket_ui::{game_card::{Card, ChoiceRow}, gamedb::Choice};
    let _language = LANGUAGE.lock().unwrap();
    for lang in [Lang::De, Lang::Es] {
        i18n::set(lang);
        lang.table().unwrap();
        let paint = |players: &str, no_match: Option<(&str, &str)>| {
            let mut card = Card::new(CardData {
                title: "Black: Sun".into(), system: "Game Boy".into(), year: Some(2001),
                players: Some(players.into()), ..Default::default()
            });
            if let Some((title, meta)) = no_match {
                card.open_chooser(vec![ChoiceRow { title: title.into(), meta: meta.into(), choice: Choice::NoEntry, current: true }]);
            }
            let mut pixels = vec![0; 1280 * 800];
            card.paint(&mut Canvas::new(&mut pixels, 1280, 800), &mut Text::builtin(), None, None);
            pixels
        };
        let players = i18n::tr("1 player or more");
        assert_ne!(players, "1 player or more", "the player count is UI copy");
        let player_copy = paint("1 player or more", None) == paint(&players, None);
        let no_match = i18n::tr("None of these");
        let no_facts = i18n::tr("No facts for this game");
        assert_ne!(no_match, "None of these");
        assert_ne!(no_facts, "No facts for this game");
        let choice_copy = paint(&players, Some(("None of these", "No facts for this game"))) == paint(&players, Some((&no_match, &no_facts)));
        assert!(player_copy && choice_copy, "{lang:?}: player-count UI translated={player_copy}, no-match UI translated={choice_copy}; only names are literal");
    }
}

#[test]
fn printed_card_lines_fit_at_the_uniform_size_returned_to_the_painter() {
    use pocket_ui::{font_style::FontStyle, type_scale::{self, TextSize}};
    let _language = LANGUAGE.lock().unwrap();
    for font in [FontStyle::Barlow, FontStyle::Retro] {
        for size in [TextSize::Normal, TextSize::Large] {
            type_scale::set(size);
            let mut text = Text::builtin().pinned_to(font);
            for room in [100, 222] {
                for title in ["Need for Speed - Hot Pursuit 2", "007: Licence to Kill", "The Legend of Zelda - Ocarina of Time"] {
                    let (px, lines) = text.card_name(title, room);
                    for line in lines {
                        let width = text.literal(|text| text.width(Face::Title, px, &line));
                        assert!(width <= room, "{font:?} {size:?}: {title}: {line} at {px}px is {width}px, room {room}");
                    }
                }
            }
        }
    }
    type_scale::set(TextSize::Normal);
}
