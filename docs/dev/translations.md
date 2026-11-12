# Translations

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you add or change UI text, German/Spanish lines or the i18n snapshots.*

## Translations

The interface is in English, German or Spanish (Settings → System → Language, `src/i18n.rs`). Nothing in the code is wrapped for
translation: the text painter (`Text::draw`, `width`, `fit`, `wrap`, `draw_right`, `draw_clipped`) looks every string
up by its exact English wording in the language's table (`assets/i18n/de.tsv`, `es.tsv`) and paints that line instead. A new page needs no code
for it, only lines in the tables. A new language is a `Lang` value, a line of `TRAITS`, and a table with every line (`i18n`'s test
`every_language_has_every_line_with_its_placeholders`). Words and tone: [i18n-glossary.md](../i18n-glossary.md). pocket-core reads the
same setting, so its notes over the game are German too. The one line put together in code is FPS + sensors
over the game: its numbers change twice a second, so `osd_sensors` looks up each part with words ("Fan {n}%") itself,
on its reading thread. The manual (`docs/user/`) stays English: the painter is
switched off while its pages are drawn (`Text::set_translate`).

On the Brick, the snapshot translation check applies the renderer's device wording to recorded translations before
matching wrapped lines (Pocket → Brick, HOME → MENU), retaining raw wording for states that select another device.
It recognizes rejoined pieces of known translations and wrapped tails of fact sentences already kept in English.
Untranslated pattern parts are still checked separately;
status-file page identifiers belong in `skip.txt`, and the literal hostname `pocket-brick` belongs in `keep.txt`.

| File | What it holds |
|---|---|
| `assets/i18n/de.tsv` | `English<TAB>German`, one line each, compiled into both programs. Placeholders for strings with numbers or names: `{n}` `{m}` `{k}` `{n2}`… a number, `{s}` `{s2}`… text kept as it is (a game, a network), `{t}` `{t2}`… text looked up in the table in turn ("Set to {t}" makes "Set to Dark" "Auf Dunkel gestellt"). The German side may put them in any order. `\n` is a line break. A `!` in front of the English (`!{t} and {t2}`) uses a pattern only when every `{t}` is a line of the table, so a short one doesn't take a game's name apart ("Rock and Roll Racing"). A `{s}` never takes in " · " or the end of a sentence; a paragraph of sentences the table has is translated sentence by sentence. Lines that don't parse are left out (and printed); the unit tests refuse them |
| `assets/i18n/es.tsv` | `English<TAB>Spanish`, the same lines as `de.tsv` in the same order, the same placeholders |
| `assets/i18n/keep.txt` | Strings shown in English on purpose: names, brands, addresses; `re:` lines are patterns (`^` `$` `.*`, `\d` digits, `\` for a literal character) |
| `assets/i18n/skip.txt` | Strings the extraction script finds that are never painted on their own: log lines, keys of other programs' files, pieces of longer strings the table translates whole |
| `scripts/i18n-extract.py` | Lists the strings of `src/` that read like words for a person and the ones without a German line (`--stub`: as table lines to fill in; `--summary`: only the numbers; `--check`: exit 1 if one is missing; `--stale`: table lines whose English the source no longer has) |
| `tests/i18n_snapshots.rs`, `tests/i18n-states.txt`, `tests/spanish.rs` | The test (above) and the snapshot states it renders in each language; the Spanish specifics |
| `scripts/i18n-shots.sh` | The same states as contact sheets (`SHOT_LANG=es` for Spanish), dark and light; lists what is cut short or runs past the edge only in German |

### Adding or changing a string

1. Write the English string in the code as usual.
2. Run `scripts/i18n-extract.py`. It prints the numbers and every string without a German line, with its file and
   line; `scripts/i18n-extract.py --stub` prints them as table lines to fill in.
3. Translate in `assets/i18n/de.tsv` and `es.tsv`, in the section of its page, with the words of the
   [glossary](../i18n-glossary.md). A format string comes out of the script with `{s}` for each hole: make it `{n}` for a
   number and `{t}` for a value the table translates, keep `{s}` for a name. Put a name into `keep.txt` and a log
   line or a key into `skip.txt` instead. Every string painted is looked up, game names and descriptions too: a line
   of only `{s}` holes and a short word (`{s} of {s2}`) turns "The Legend of Zelda" into "The Legend von Zelda".
   Anchor it with a number or more words (`{n} {s} of {n2} {s2}` for "41.2 GB of 63.9 GB"); `i18n`'s test
   `the_names_kept_in_english_stay_english` runs `keep.txt`'s names through the table. Game names have a structural
   guard besides: the places that make a game's name (`library::load_with`, `library_cache::read`,
   `library::shelf_name`, the play history) hand it to `i18n::learn_names`, and a learned name is painted as it is,
   whatever the table has for the same words ("Black" is the Look page's word, "Time to Play" fits "{t} to {t2}";
   `tests/game_names.rs`). A name made only to be compared (`library::display_name` for the picture catalogue's
   matching) teaches nothing, and a new place that paints names it makes itself should call `learn_names` or
   `shelf_name`. The price: a word of the interface that is also the name of a game on this Pocket stays English.
4. Run `cargo test --release --test i18n_snapshots`. It fails, naming the strings (and the states), when a string
   of `src/` has no line, or when a German snapshot of `tests/i18n-states.txt` paints a string without one. The
   second catches what the script can't see: strings put together at run time. Add the states of a new page to
   `tests/i18n-states.txt`.
5. Look at the page in German, dark and light: `scripts/i18n-shots.sh` writes contact sheets of every state to
   `screenshots/i18n-de/`, with `cut-de-only.txt` and `overflow-de-only.txt` (strings cut short with "…", or drawn
   past the right edge, in German but not in English). German runs about a third longer than English. The text has
   to fit where the English fits; cut short only where the English is cut short too. Shorten the German before you
   touch the layout. Overlaps (a long label running into its value) only show on the sheets.

A changed English string loses its German line until the table is changed with it; the test says so. After merging
a branch with new pages, do the same: `scripts/i18n-extract.py --stub`, translate, add the new pages' states, run the
test, look at the contact sheets.

### What it costs

English costs one atomic load per painted string (4-5 ns). In German a string painted in the frame before costs one
hash look-up and no allocation: 23-30 ns for a line of the table, 27-32 ns for one a pattern made, 45-50 ns for a
name that isn't in it (build computer, a shared x86 machine, `cargo run --release --example i18n_cost`; measuring the
same string's width takes 360-420 ns). Only the first sight of a string that is not a line of the table runs the
patterns, 6-9 µs, once per painter (`i18n::Cache`). The table is compiled in; when German is chosen it is read on a
thread (1.2-1.3 ms), so the first frame doesn't wait for it. The Mini V2 is slower; measure there before relying on
these numbers.
