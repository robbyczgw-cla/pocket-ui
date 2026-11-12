# Consoles and game data

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you edit console fact sheets, credits, gamedb or Curator data.*

## Console fact sheets

`assets/data/consoles.json` holds one verified fact sheet per system (51 of them: 39 consoles, plus on 2026-11-03 the Amiga, C64, ZX Spectrum, Atari 5200, Atari 8-bit family, PS3, PS Vita and Wii U, and four that are no console: PICO-8 and TIC-80, `kind` "fantasy", ScummVM and MS-DOS (the `pc` folder), `kind` "software", released "WW" for worldwide), compiled into the binary by
`src/consoles.rs` and looked up by ROCKNIX system id; ids that are the same console (`genesis`, `famicom`, `sfc`,
`tg16`, the `*h` hack folders, `wiiware`) are mapped in `consoles::ALIASES`. `megacd` is the Sega CD's other folder. Systems without a sheet (the PC Engine CD, which has no
article or Wikidata facts of its own, the black-and-white Neo Geo Pocket) and later models with other
hardware (WonderSwan Color, New 3DS) are other consoles and get no page.

- **The facts are not ours to edit.** Every value comes from the sources listed with its console (Wikidata,
  Wikipedia, the makers' figures). Don't add, round, correct or "fix" a fact without a source for the change, and
  add that source to the console's `sources`. The code shows the values as they are; it only formats dates
  ("1998-11-27" as "27 November 1998"), names regions ("NA" as "North America") and orders releases by date.
- `lines.*` are ready-made English strings for the shelf's short facts; the other fields feed the console page.
  A field that is missing or `null` is left out of the UI, never shown as "None": keep it that way
  (`consoles::tests::no_row_and_no_page_text_ever_says_none_or_null`).
- The tests check that the file loads, that every console resolves by its id and aliases, and which shelf rows a
  console gets. A new console needs its id in `consoles::tests::IDS` and, ideally, a picture in `assets/systems`.

## Console credits

`assets/data/console-credits.json` holds Credits & links for every console of `consoles.json` (`src/console_credits.rs`
reads it, `src/credits_page.rs` shows it). Unlike `consoles.json` it is **built, not edited**:

```sh
python3 -I scripts/build-console-credits.py review          # fetch (cached), print every candidate fact per console
python3 -I scripts/build-console-credits.py build --strict  # check the chosen facts, write the JSON
```

- **Sources.** Wikidata (CC0) through its SPARQL endpoint, and each console's English Wikipedia article, first section
  only (the infobox and the lead), through the MediaWiki API. Politeness as `build-gamedb.py`: one request at a time,
  2 s apart, a User-Agent naming the tool, Retry-After honoured on 429/503; answers cached in `~/.cache/pocket-consoles`
  (`CONSOLES_CACHE`), so `build` runs offline after the first `review`. `--refresh` fetches again.
- **What ships is chosen by hand, checked by the script.** `scripts/console-credits.in.json` lists per console the
  people, makers, chips (with their maker), family members and best-selling games, each with its `source`:
  `wikidata:<P>` (the console's item has that statement pointing at the fact's `qid`), `wikidata:^<P>` (the fact's item
  points at the console), `wikidata:family` (the fact's item is in the console's family item, not its generation),
  `infobox:<field>` (the field, markup removed, contains the name; for a best-seller, the title and the figure) or
  `lead` (one sentence of the first section has all of the fact's `words`, a few short phrases, and one phrase has the
  name together with what was done, `ROLE_WORDS`: "team led by Hideki Sato", so a sentence that merely mentions
  someone else doesn't pass). A chip's maker must
  be the chip item's P176 (shown as "made by") or P178 ("designed by"). `build` drops every fact that fails its check
  and says why; `--strict` fails instead. A `game_makers` name must be one of the console's checked makers.
- **No chips for programs.** A system whose `kind` is "fantasy" or "software" (PICO-8, TIC-80, ScummVM, DOS) has `chips: []` in the
  input: it exists only as a program, and the test lets it have none.
- **No copyrighted prose ships.** Only the short proof phrases stay in the input file; the output has names, figures, our own
  short role texts ("Led the design team") and our two-sentence `summary` per console. Write those only from facts on
  the page (Hifiteam tone: calm, no marketing), and keep them true without the reader knowing more.
- **People.** Wikidata has almost no "designed by" (P287) or inventor (P61) statements for consoles (only the
  WonderSwan's Gunpei Yokoi, 2026-10-23), so people come from the article's lead with the phrases as proof. The infobox
  fields "designer" don't exist for these articles. Launch titles have no field in either source and are left out.
- **Links.** A family member becomes a link when its `qid` is a ROCKNIX system of its own (`SYSTEM_OF` in the script;
  a model such as the Game Boy Pocket plays its console's games and is no system) or the input names a `system`
  (Triforce). Both are held to the ROCKNIX ids of es_systems.cfg (`ROCKNIX` in the script,
  `console_credits::ROCKNIX_SYSTEMS` in the tests). Consoles with the same chip are found at run time by the chip's
  `qid`: the same Wikidata item, never a similar name (the SG-1000's NEC µPD780C is not the Z80 of the Master System).
  The App shows a link only when the library has games for that system (`console_credits::folder_plays`, aliases such
  as `genesis` and `megacd` included); "By the makers" uses Curator's catalog (the developers from gamedb).
- **Tests** (`console_credits::tests`, `credits_page::tests`): the file parses and covers every console page, every
  fact names a checkable source, links point at ROCKNIX systems and never at the console itself, the same-chip links,
  the game makers, and the page's selection, scrolling and touch rows.

## Game data (gamedb and Curator)

`assets/gamedb.bin` (Wikidata facts) and `assets/curatordb.bin` (Curator's feel tags) are built offline and
committed; the device never fetches them. Rebuild with `scripts/build-gamedb.py` (Wikidata, cached in
`~/.cache/pocket-gamedb`) and `scripts/build-curatordb.py label` then `write` (a batch labeller, cached in
`~/.cache/pocket-curator`). Both files keep one zlib stream per system, so a lookup unpacks only its system.
The vocabulary, the golden set, the quality numbers and the reader APIs are in [CURATOR-DATA.md](../CURATOR-DATA.md).
Never commit a personal library list or file names from it.
