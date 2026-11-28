# Shelves by series

Settings › Look › Layout › **Order: By series** (settings.json `"shelf_order": "series"`; By console when unset): Home's
systems make way for one shelf per game series, and X on one opens its series card in place of the console page. The
player's side is in [user/playing.md](user/playing.md#shelves-by-series).

## Where a game stands (`src/series_shelf.rs`)

Arranged with Connections' index (`connections::Index::build` → `Index::arranged`) on Curator's thread, after the
library is read. Per game, its series candidates in order: the game data's series (Wikidata, `gamedb`), then the
series ScreenScraper's cache gives (`familles`, `screenscraper::series_of`, read with the catalog on Curator's thread,
one file per system, never a request), then the series its title gives where the data has none
(`connections::derive_series`). ScreenScraper's `numeros` is `Entry::number`. Then:

1. The first candidate that is one of `series_facts::ALL` by name or alias (matched as `connections::key` matches
   series): aliases put sub-series on their big series' shelf ("Super Mario Land" on Super Mario's).
2. Else the first candidate that another title of the library shares.
3. Else One of a kind (`series_shelf::ONE_OF_A_KIND`). A group of one title once counted also goes there: one game on
   two systems is one title.

A shelf is ordered by release year, then the number in the series (`Entry::number`, ScreenScraper's `numeros` once its
cache has them), then by name; games without a year last. Shelves by name ("The" does not count), One of a kind last.

The App (`src/app_series.rs`) filters the arrangement in Home's rebuild (`Index::series_shelves`): hidden systems
(Systems on the shelf) and games in Shelf space's crate are off; a series left with one title moves to One of a kind.
`Item::Series(i)` stands where the systems stood (`series_home`), A opens `Screen::Series(i)` (a mixed list like
Favourites: the system is on the spine and label), the cabinet gets `ShelfKey::Series(i)` shelves
(`Cabinet::build_keyed`), each game in its own console's case.

Home is by series only when chosen, the look is Shelf, Split or Cabinet (not Easy, not Kids Mode) and the index is
there: until then Home is by console, so it is never empty or waiting.

## Its cover and its spine

In `src/series_look.rs` (the pictures and choices) and `src/app_series_look.rs` (the App's side).

The Shelf look's series entry (`Item::Series`, `Medium::Keep`) stands as a case like a system's: its spine in the row and
its front large above the shelf (`App::stand_pic`, `stand_front`).

- **Which games:** `App::series_ranked` (`series_look::ranked`): the series' key games (`Facts::key`, matched as the card
  matches them, every edition) first, then the rest in shelf order (release year).
- **The cover** (`App::series_front`): the first four of them (`COLLAGE_MAX`) with box art on the card, one per title
  (`series_look::cover_games`: a title's first edition with box art; a cover whose cut can't be read is passed over;
  `cover_file`: the library's own pictures, ScreenScraper's included; nothing is downloaded for it). Each is read as a
  game's front is (its cover cut and colours on the cabinet's reader thread; the cuts held until all are in,
  `series_labels`) and made into its own case (`media::front`, in its console's case); `series_look::collage` stacks
  them on a thread of its own (`series-cover`), leaning a few degrees and overlapping, the first in front, each with a
  soft shadow. One box art: that game's case alone, straight. None: the series' plain front with its name, as before.
  While the covers are read and stacked the plain front shows (never a frame's wait; not on a turn's first frame).
  Kept per series and size (`CabinetUi::series_fronts`, at most 24); the series' cases are keyed by its name and
  `series_gen`, which new covers, new logos and a changed list of series or of their games raise (`CabinetUi::series_changed`; Home made again with the same series keeps them).
- **The spine** (`App::series_logo`; **off in 0.8**, `series_look::SERIES_LOGOS`: the series' spine shows its name, no
  logo is fetched or drawn; on again in 0.8.1): ScreenScraper's `wheel` (the game's logo, see-through round it) of the first of
  the series' first `LOGO_LOOK` games in logo order (`series_look::logo_order`: the games named as the series itself
  first, "Metroid" before "Metroid Zero Mission"; then titles that start with the series' name and carry no number,
  "Castlevania - Dracula X" before "Super Castlevania IV"; then the ranked ones) that has a readable one on the card (`wheels/<system>/<rom stem>.png`), while Real
  spines is On. Read with its alpha on the reader thread (`Kind::Logo`: `real_label::load_rgba`,
  `series_look::upright_logo`: its empty margin cut, a wide one turned a quarter clockwise to read top to bottom as the
  titles, at most `LOGO_EDGE` long). `series_look::logo_spine` puts it whole into the spine's label, as large as the
  spine's width lets it, never stretched or cut, on a ground it reads on (`logo_ground`: a warm anthracite, apart from
  a black page; light under a very dark logo; a dark logo gets a thin light edge); nothing is written over it. No logo:
  the painted spine with the name, unchanged. The logos are fetched by ScreenScraper's run in a pass of their own right
  after the spines, one game per series, from the same logo order (a run also starts when Home becomes by series, after the running one if one runs) ([SCREENSCRAPER.md](SCREENSCRAPER.md#the-settings-row), "The run").
- **Speed:** a frame only looks the cover and logo up in their caches; the four cases are made once
  (`made("series cover's cases")`), their stack on its own thread. Tests: `series_look::tests` (0, 1 and 4+ cases, the
  logo turned, cut and fitted, its ground), `screenscraper::tests::a_series_logo_comes_from_one_game_only`.

## The series card (`src/series_page.rs`)

`series_page::parts` lists what the card says, top to bottom: the story (`Facts::story`) or what the card is (a
series without facts, One of a kind, One of a kind while no series was found), the series' facts (Since, Main games or
the library's Years, You have "n of m main games", Consoles), the key games with "On your Pocket" on the owned ones,
then the player's games with year and console; "No games yet" for an empty list. B back, A the shelf's games,
Up/Down and L1/R1 scroll. Drawn with the console page's pieces (`console_page::scrolled`, `row`, `heading`).

## The facts (`src/series_facts.rs`, `series_facts_data.rs`)

One `Facts` per series: name, aliases, original maker, first year, main games as the source counts them (0: unknown,
left out), 3 or 4 key games, a story of 2 to 4 sentences. Sources: [SERIES-SOURCES.md](SERIES-SOURCES.md). Stories
are translated through `assets/i18n/*.tsv` like every painted line; names and titles are painted as they are
(`Text::literal`, `i18n::learn_names`). The tests hold every story to 2 to 4 sentences, no dash as punctuation, and no
alias in two series.

## Identity and speed

A series is kept by its name across a new index and a rescan (`app_series::series_make`, `series_follow_library`): the
open series, its card and Home's remembered row stay on the same series wherever it now stands, or go Home when it is
gone. A panel's tiles name each game without the series (`series_shelf::short_title`; a sequel keeps its number). The
Layout page's previews by series ask for the covers they lack from the picture loader's thread and are made once more
when the last is in (`series_previews_wait`, `series_cover_in`), never decoded on the UI thread. The card measures and
paints only the rows in view (`a_card_of_thousands_of_games_paints_only_what_is_in_view`). In Cabinet the
series are shelves, not rows: X on a game opens its card, as for consoles.
