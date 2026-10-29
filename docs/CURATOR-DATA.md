# Curator data: facts, feel tags and file-name facts

Reference for the data half of [CURATOR.md](CURATOR.md) (layers A, B and the file-name part of C), as built on
2026-10-23. The engine and screens (`src/curator/`) use it only through the APIs below.

| Layer | What | Built by | Shipped as | Read by |
|---|---|---|---|---|
| A, facts | Wikidata (CC0): genres, series, mechanics, inputs, languages, versions grouped by work, series order | `scripts/build-gamedb.py` | `assets/gamedb.bin` | `src/gamedb.rs` |
| B, feel | Our tags: family, sub-genre, mechanics, pace, controls, mood, multiplayer, setting, length, session, difficulty, text-heavy | `scripts/build-curatordb.py` | `assets/curatordb.bin` (vocabulary inside) | `src/curatordb.rs` |
| C, the file | Regions, languages, fan translation, revision, dump flags from the ROM's name | — | — | `src/romfacts.rs` |

## Reader API (what the engine calls)

### Layer A: `gamedb`

Unchanged calls (`GameDb::embedded`, `lookup`, `get`, `resolve`, `candidates`; `resolve_rom` and `lookup_rom` take the
name a ROM's checksum gave, see [Identification by checksum](#identification-by-checksum)); `GameInfo` has new fields:

| Field | Meaning |
|---|---|
| `work()` | The QID of the work this entry is a version of. **Group versions by this.** The same QID on several systems is one work; an edition or port filed as its own item points at the work it belongs to (Chrono Trigger DS `Q507828` → `Q761815`). A remake, a fan game or a film tie-in has its own work. |
| `work_of: Option<String>` | The raw column: `Some` only when the work is another item. |
| `related: Vec<(Relation, String)>` | Links that are *not* versions: `RemakeOf` / `RemadeAs` (remakes and remasters, for "Related: remake"), `BasedOn` / `SourceOf` (fan games, spin-offs built on a game). The QID may be on none of our systems. |
| `follows`, `followed_by: Vec<String>` | Series order (P155/P156), QIDs. |
| `mechanics: Vec<String>` | P4151, English labels ("multiple endings"). Sparse: 132 games. |
| `inputs: Vec<String>` | P479 ("gamepad", "touchscreen", "Wii Remote", "light gun", "Wii Balance Board"). 5,103 games. |
| `languages: Vec<String>` | P407 as ISO 639-1 codes ("en", "ja"; a name where Wikidata has no code). 3,694 games. |

### Layer B: `curatordb`

```rust
pub fn vocabulary() -> &'static Vocabulary;                 // the shipped vocabulary, parsed on first use
pub fn lookup(system: &str, qid: &str) -> Option<GameTags>; // the shipped tags; None = not tagged
```

- `Vocabulary`
  - `tags: Vec<Tag>`: `Tag { id, axis, label, hint }`. `label` is short English written for reason lines:
    "crash takedowns", "arcade racing", "short sessions".
  - `axes: Vec<Axis>`: `Axis { id, label, single, hint }`.
  - `id("drifting") -> Option<TagId>`, `tag(TagId) -> &Tag`, `axis("mood") -> impl Iterator<Item = TagId>`.
  - `genre_tags("kart racing game") -> &[TagId]`: the family (and sometimes sub-genre or setting) for a
    Wikidata genre. Every genre in gamedb is mapped, so this gives a Family facet for *every* game, tagged
    or not.
- `TagId = u16`: index into `tags`. It is stable within one build only. Store tag *ids* (strings) on disk.
- `GameTags`
  - `qid`
  - `tags: Vec<(TagId, u8)>`: the weight is 1–3. 3 means defining, 2 clearly present, 1 minor (shipped only
    when confirmed by the cross-check). They are in vocabulary order.
  - `length: Option<Length>`: `Short` (< 5 h), `Medium` (5–15 h), `Long` (15–40 h), `Epic` (40 h+) or
    `Endless`.
  - `session: Option<Session>`: `Pickup` (≤ 15 min) or `Sitdown`.
  - `difficulty: Option<Difficulty>`: `Easy`, `Moderate`, `Hard` or `Brutal`.
  - `text_heavy: Option<bool>`
  - `confidence: u8`: how well the labeller knew the game; 3 well, 2 roughly, 1 barely.
  - `cross_checked: bool`
  - `weight(&vocab, "drifting") -> u8`: 0 when the game doesn't have the tag.
  - `facet_tags(&vocab) -> Vec<TagId>`: the tags plus the vocabulary tags for length, session, difficulty
    and text-heavy (`length-short`, `session-pickup`, …). Build one bitset per `TagId`.
- `Length::tag_id()` and the other value types give the vocabulary id for a value, for its label.
- `CuratorDb::parse(bytes)`, `system(id) -> Option<Arc<SystemTags>>` and `lookup` work on any file; tests use
  `curatordb::encode`.

**Threads and speed.** Nothing happens until the first call. Each system is unpacked once (a few ms) and
shared through an `Arc`. The type is `Send + Sync`, so the index can be built on a background thread. A lookup
after that is a hash-map access plus a clone. System ids are ROCKNIX's (`genesis` finds `megadrive`), and
Game Boy / Game Boy Color fall back to each other, as in gamedb.

**Versions and tags.** Tags are per QID. A game on several systems is labelled once, and the tags that
need hardware are kept off systems that lack it:
- touch screen and two screens only on DS/3DS
- needs an analog stick never on stick-less systems
- link play only on handhelds
- couch co-op, couch versus and up to four players only on home consoles

### Layer C: `romfacts`

```rust
pub fn parse(file_name: &str) -> RomFacts;
RomFacts { regions: Vec<&str>, languages: Vec<&str>, translation: Option<Translation>, revision: Option<String>, flags: Vec<Flag> }
RomFacts::languages_or_implied() -> Vec<String>   // patch language, else listed, else the region's ("Japan" -> ja)
RomFacts::readable_in("en") -> Option<bool>       // None when the name doesn't say (Europe, World)
Translation { language: String, current: bool, by: Option<String> }
Flag::{Verified, Beta, Proto, Demo, Unlicensed, Pirate, Hack, BadDump, Alternate, Fixed, Rerelease, Homebrew}
```

It reads these name styles:
- No-Intro: `(USA, Europe)`, `(En,Fr,De)`, `(Rev 1)`, `(Beta)`, `(Virtual Console)`
- GoodTools: `(JU)`, `(U)`, `[!]`, `[h1]`, `[b2]`, `[T+Ger]`, `[T+Eng1.01_Author]`
- Patch tags: `[T-En by Author]`, `(Translated En)`

The title part is matched by `gamedb`/`covers`; this module only reads the groups they drop. No helper
existed for this before. `cheats.rs` has a private region reader used only for picking cheat files.

## Identification by checksum

Layers A and B are keyed by the name a ROM is matched under. Since 2026-10-25 that name is, for cartridge systems,
the **No-Intro name its checksum gives** when the list knows it, and the file name otherwise (`src/rom_id.rs`):

| Step | What | Where |
|---|---|---|
| Checksum | A zip's CRC-32 from its directory (nothing unpacked, `archive::rom_crc`); a loose file up to 64 MB read once, plus the CRC without an iNES/FDS/Lynx/SNES copier header or of an N64 `.v64`/`.n64` in big-endian order. Disc systems, 7z and bigger files: none | `rom_id::checksums` |
| Cache | `<data>/identify/checksums.tsv`: system, size, mtime (ns), CRC, alternative CRC, path. A file is read again only when its size or mtime changes; lines of files no longer in the library are dropped | `rom_id::Cache` |
| List | libretro-database's `metadat/no-intro/<system>.dat` (clrmamepro), fetched once when online and after 180 days, parsed to `<data>/identify/<system>.tsv` (CRC, name). A CRC several entries share keeps the cleanest name (`covers::rank`: no re-release, the usual region order). Not shipped | `rom_id::parse_dat`, `fetch_dat` |
| Names | path → No-Intro name for every game whose CRC is in its system's list; from the card alone (no game file read) | `rom_id::known` |

### PlayStation discs: the serial

A PlayStation 1 or 2 image is too big for a checksum, so the serial on the disc names it (`src/disc_serial.rs`, run
and cached by the same `rom_id::run_until` and given back by the same `rom_id::known`, so the users above need no
special case):

| Step | What | Where |
|---|---|---|
| Serial | The `BOOT`/`BOOT2` line of the ISO 9660 file `SYSTEM.CNF` (`BOOT = cdrom0:\SLES_556.73;1` → `SLES55673`, compared without dashes, dots and case), else the image's volume identifier when that is serial-shaped. A few sector reads: a 37 KB layout probe at the start, the volume descriptor (sector 16), the root directory (up to 64 KB) and that one file (up to 8 KB) — never the image. `.iso`, `.bin` (2048-byte sectors or a MODE2/2352 PlayStation dump) and the `.cue`/`.m3u` that names them; a `.chd`, `.cso` or `.pbp` is not opened (its file name is used) | `disc_serial::serial_of`, `parse_cnf`, `normalize` |
| Cache | `<data>/identify/discs.tsv`: system, size, mtime (ns), serial, path. An image is read again only when its size or mtime changes; lines of images no longer in the library are dropped | `disc_serial::Cache` |
| List | libretro-database's `metadat/redump/<system>.dat` (`Sony - PlayStation`, `Sony - PlayStation 2`; clrmamepro, a `serial` field per game), fetched once when online and after 180 days, parsed to `<data>/identify/<system>.serials.tsv` (serial, name). A serial several entries share keeps the cleanest name (`covers::rank`). Not shipped; the licence is the same as the No-Intro lists' ([NOTICE.md](../NOTICE.md#data-sources)) | `disc_serial::parse_dat`, `fetch_dat` |
| Names | path → Redump name; from the card alone. The shelf shows it without its region tags but with its disc tag ("Final Fantasy VII", "Final Fantasy VII (Disc 2)"); `library::load_with` does that with `disc_serial::shelf_name`, after the gamelist's names. A file name that starts with the serial loses it when a title follows (`disc_serial::clean_stem`) | `rom_id::known`, `disc_serial::known` |

Who uses the name, always before the file name and after the player's own choice:

- `gamedb::GameDb::resolve_rom(system, stem, identified, choice)` and `lookup_rom`: the checksum's name, then the
  file name. Used by the card (`app_card::game_info`, the App's `Identify::names`), Curator's catalog
  (`LibraryGame::identified`, filled on the catalog's thread), the descriptions (`auto_texts::todo`) and the web
  companion's game page.
- `covers::pick(stem, identified, index)`: the box art, screenshot and title screen under that name, then by the
  file name as before (`best_match`).

A game the list does not know (homebrew, a hack, a bad dump, a disc of another console) behaves exactly as before. Offline before
the first download there are no names; the next run online fetches the list and names the games without reading
them again. Tests: `rom_id::tests` (the zip's CRC in `archive::tests`), `covers::tests::the_checksums_name_comes_first…`,
`game_pictures::tests::a_game_recognised_by_its_checksum…`, `curator::tests::a_game_its_checksum_named…`.

## Layer A: how versions are grouped

`resolve_works` in `scripts/build-gamedb.py`:

1. The same QID on several systems is one work.
2. X *edition of* (P629) Y, or Y *has edition* (P747) X: X is a version of Y.
3. X *based on* (P144) Y, or Y *derivative work* (P4969) X, where Y is a video game. Wikidata uses one property
   for ports, remakes and fan works, so the item's class and title decide:
   - **Remake** (it keeps its own work and is linked as `remake-of` / `remade-as`): a class saying remake,
     remaster or reboot, or a title word like 3D, HD, Remastered, Redux or Anniversary.
   - **Fan work** (`based-on` / `source-of`): a class saying ROM hack, fan game or homebrew.
   - **Port** (same work): the same title, or the title plus port words like Advance, DS, DX, Deluxe or
     Portable.
   - **Based-on**: anything else.
4. A film, book or toy the game is based on is never a version: link targets are checked to be video games.
5. Chains are followed to the end. In a loop, the best-known item becomes the work.

Result: 97 items point at another item's work. That is on top of the 3,273 items that are on several systems
under one QID. Chrono Trigger (SNES, PS1, DS, Wii, PSP and the DS edition item) is one work. "Chrono Trigger:
Crimson Echoes" and Ocarina of Time 3D are not. `gamedb::tests::the_shipped_file_groups_versions_by_work`
checks these cases.

## Layer B: vocabulary and labelling

**Vocabulary** (`assets/data/curator-tags.json`, the source of truth): 172 tags on 14 axes.

| Axis | Tags | Notes |
|---|---|---|
| family | 24 | The genre map sends all 394 Wikidata genres of gamedb here (`genre_map`) |
| sub-genre | 64 | |
| mechanics | 22 | |
| pace | 4 | Exactly one per game |
| session | 2 | A `GameTags` field |
| length | 5 | A `GameTags` field |
| difficulty | 4 | A `GameTags` field |
| story | 3 | |
| text | 1 | A `GameTags` field |
| grinding | 1 | |
| controls | 8 | |
| mood | 15 | |
| multiplayer | 5 | |
| setting | 14 | |

There are 172 tags rather than the ~150 the design names. The setting axis (fantasy, sci-fi, …) was added
because reasons like "sci-fi · on-rails shooting" need it. A new genre in gamedb must be added to `genre_map`;
`curatordb::tests::the_vocabulary_maps_every_wikidata_genre` fails until it is.

**Labelling** (`scripts/build-curatordb.py`):

- **Scope:** every game with sitelinks ≥ 3 (9,029 QIDs), and a library's games first. A library list is
  matched to QIDs by `examples/gamedb_match.rs`, which uses the device's own matching rules.
- **Privacy chaff:** 400 lesser-known games are drawn at random, without looking at the library. Below the
  threshold, every game ships only when the labeller knew it (confidence ≥ 2). This way a public binary
  can't show which obscure games someone owns.
- **Labeller:** `codex exec -m gpt-6-luna -c model_reasoning_effort=medium --sandbox read-only`. It runs as
  a pure function on batches of 40 games, 4 in parallel, with strict JSON out.
  - The model sees only the vocabulary and each game's Wikidata facts: title, aliases, systems, year,
    developer, publisher, genres, series, modes, players, mechanics, inputs.
  - Answers are cached per model and prompt version in `~/.cache/pocket-curator/`, so a run resumes where it
    stopped.
- **Second model where Luna is weak:** on WonderSwan, WonderSwan Color, Atari 2600, Game Boy Color, Virtual
  Boy and Sega CD, Luna said "don't know" (confidence 1) for 8 % or more of the games. There, GPT-6 Sol
  (`--model sol`) labelled the games again (631 games), and its labels replace Luna's.
  - Sol is more careful: on those games it gives fewer tags and says "don't know" more often.
  - On the golden set Sol reaches 95.6 % precision and 95.3 % recall, but it costs about 20 times Luna's
    credits.
  - `golden --model sol` measures it into a separate cache, so the golden test keeps measuring what ships.
- **Shipping rule:** a tag ships at weight ≥ 2. A weight-1 tag ships only when the cross-check confirmed it.
  On the golden set, weight-1 tags were right 31 % of the time, weight-2 tags 86 % and weight-3 tags 98 %.
  Pace is the exception: it is one value per game, so its weight carries no doubt (weight-1 pace tags were
  92 % right), and it ships at any weight.
- **Rebuild:** `scripts/build-curatordb.py label --library <list>`, then `write`, then
  `cargo test --release --lib curatordb`.

### Quality

- **Golden set:** `src/testdata/curator-golden.json` holds 121 games labelled by hand across 18 systems and
  all families.
  - Each game has `must` tags (counted for recall), `ok` tags (defensible too) and accepted values for
    length, session, difficulty and text-heavy.
  - `curatordb::tests::the_shipped_file_knows_the_golden_games_and_agrees_with_them` keeps the shipped file
    at ≥ 85 % precision against it.
- **Iterations:**

  | Prompt | Golden revision | Precision | Recall | Change |
  |---|---|---|---|---|
  | v3 | rev 1 | 85.0 % | 88.7 % | All weights, first try |
  | v4 | rev 2 | 85.7 % | 93.2 % | Sharper tag hints. The golden set gained 30 `ok` tags where the model was defensibly right |
  | v5 | rev 3 | **92.6 %** | **91.2 %** | Shipping rule (weight ≥ 2) and hardware filter. Dropped the noisy `dungeons` and `complex-controls`. Turn-based RPGs accept calm pace, as the vocabulary defines it |

- **Golden scalars** (v5): length 69 %, session 95 %, difficulty 88 %, text-heavy 94 % within the accepted
  values.
- **Cross-check:** a second model family labels a 300-game sample: the developer's own library plus a random spread.
  - On an axis the second labeller covered, only tags both gave are kept. Where it said nothing, the first
    labeller's tag stays unconfirmed. Scalars must agree within one step.
  - The planned second family was DeepSeek V4.1 Flash (`pi`, fallback `opencode`). Its OpenCode Go key had
    expired on 2026-10-23, so Claude Opus 5.5 labelled the sample instead, without seeing Luna's answers.
    The DeepSeek path stays in the script (`cross`); `import` adds labels from elsewhere.
  - Second checker (2026-10-23, later the same day): **Muse Wonder Full** (Meta, another family again)
    labelled the same 300 games and the 121-game golden set blind, from the same prompts (`prompt`), into
    the cache as `muse`. On the golden set it scored **96.2 % precision, 94.8 % recall** (length 84 %,
    difficulty 98 %), against Luna's 92.6 / 91.4 (length 69 %).
  - With two checkers a game's tags are decided by majority (`cross_labels`): a Luna tag ships when Claude
    or Muse gave it too, and a tag both checkers gave ships even if Luna missed it. Scalars come from Muse
    first. Agreement on the 300 games: Luna × Muse 68 % of Luna's tags confirmed (Luna × Claude 56 %,
    Claude × Muse 83 %).
  - Numbers: see the build report below (written before Muse; the per-system table is Luna × Claude).

## Licensing

- **Wikidata data is CC0** (public domain): everything in `gamedb.bin` and every fact the labeller sees.
- **The tags in `curatordb.bin` and the vocabulary are our own derived data**, under pocket-ui's licence
  (GPL-2.0-or-later).
- **No Wikipedia text, review text or other copyrighted text is sent, stored or shipped.** The labeller works
  from CC0 facts and its own knowledge, and only tag ids come back.
- **The No-Intro and Redump lists are not shipped.** libretro-database is CC BY-SA 4.0 as a repository; its
  `metadat/no-intro` DATs are a bulk import from No-Intro, whose own terms for the DATs are not stated in a licence
  we could find (UNCERTAIN), and its `metadat/redump` DATs come from Redump. The device downloads a system's DAT
  itself, as RetroArch's updater does, and keeps a CRC → name table (the discs: serial → name) on its own card
  ([NOTICE.md](../NOTICE.md#data-sources)).
- **A library list is private.** It is read at build time, never written to the repository or the shipped
  file, and its lesser-known games are hidden among the chaff (see above). The local build cache
  (`~/.cache/pocket-curator`) keeps answers in the order they were made, library first, so keep that cache
  private too.

## Build report (2026-10-23)

From `scripts/build-curatordb.py report`:
- **Games (gamedb):** the entries per system.
- **In scope:** the entries labelled (sitelinks ≥ 3, the library, the chaff).
- **Tagged:** the entries shipped.
- **Golden precision:** of the shipped tags of the golden games on that system (small numbers on small
  systems).
- **Labellers agree:** the share of Luna's shipped tags that the second labeller confirmed, on the axes it
  labelled.

| System | Games (gamedb) | In scope | Tagged | Cross-checked | Golden precision (tags) | Labellers agree (games) |
|---|---|---|---|---|---|---|
| 3ds | 796 | 410 | 403 | 9 | 75 % (8) | 72 % (9) |
| atari2600 | 411 | 255 | 246 | 1 | 73 % (15) | 100 % (1) |
| atari7800 | 51 | 44 | 44 | 0 | – | – |
| atarilynx | 61 | 32 | 31 | 1 | – | 100 % (1) |
| dreamcast | 498 | 343 | 338 | 12 | 90 % (21) | 82 % (12) |
| fds | 182 | 92 | 91 | 2 | – | 100 % (2) |
| gamecube | 697 | 586 | 585 | 10 | 96 % (48) | 79 % (10) |
| gamegear | 184 | 121 | 121 | 10 | – | 94 % (10) |
| gb | 545 | 337 | 332 | 14 | 96 % (48) | 89 % (14) |
| gba | 1066 | 736 | 725 | 113 | 97 % (108) | 80 % (113) |
| gbc | 469 | 227 | 223 | 10 | 92 % (12) | 76 % (10) |
| mastersystem | 271 | 209 | 207 | 5 | – | 100 % (5) |
| megadrive | 1042 | 846 | 842 | 34 | 94 % (53) | 86 % (35) |
| n64 | 424 | 358 | 357 | 13 | 93 % (85) | 83 % (13) |
| nds | 1716 | 1036 | 1018 | 21 | 93 % (56) | 82 % (21) |
| nes | 1219 | 942 | 936 | 12 | 94 % (49) | 79 % (12) |
| ngp | 5 | 4 | 4 | 0 | – | – |
| ngpc | 33 | 22 | 22 | 0 | – | – |
| pcengine | 355 | 235 | 234 | 8 | 86 % (7) | 78 % (8) |
| pcenginecd | 18 | 5 | 5 | 0 | – | – |
| ps2 | 3090 | 2125 | 2101 | 31 | 95 % (148) | 82 % (31) |
| psp | 1278 | 883 | 877 | 18 | 94 % (31) | 82 % (18) |
| psx | 2237 | 1267 | 1238 | 24 | 87 % (158) | 80 % (24) |
| saturn | 554 | 322 | 317 | 5 | 65 % (17) | 91 % (5) |
| sega32x | 41 | 38 | 38 | 3 | – | 80 % (3) |
| segacd | 161 | 113 | 109 | 4 | – | 91 % (4) |
| sg-1000 | 43 | 34 | 33 | 0 | – | – |
| snes | 1434 | 916 | 904 | 90 | 95 % (146) | 84 % (92) |
| virtualboy | 26 | 22 | 22 | 1 | – | 100 % (1) |
| wii | 1688 | 994 | 982 | 19 | 88 % (25) | 79 % (19) |
| wonderswan | 46 | 20 | 20 | 2 | – | 85 % (2) |
| wonderswancolor | 33 | 7 | 7 | 1 | – | 80 % (1) |
| **All** | 20674 | 13581 | 13412 | 473 | 92.5 % (1035) | 80.0 % (300) |

Since 2026-10-23 `gamedb.bin` also has NAOMI (12 entries), Atomiswave (1) and Pokémon mini (10), for the systems
pocket-core took on (docs/HOST.md "More systems"); the table above is from before, and they have no feel tags until
the next labelling run.
Since 2026-10-29 it also has the **Xbox** (1066 entries, Wikidata platform Q132020), for hakuX (docs/XBOX.md): queried
alone and merged into the file with `scripts/build-gamedb.py --only xbox --merge`, which keeps every other system's section
byte for byte, so its versions are grouped among Xbox games only. Six of its genres were new and are in `genre_map`
now, also in `curatordb.bin`'s vocabulary (`scripts/build-curatordb.py vocab`: that section only, the labels byte for
byte); Xbox games have no feel tags until the next labelling run.

Unique games tagged: 9292; entries: 13412; tags per entry: 6.6
Confidence: {1: 423, 2: 5252, 3: 3617}
Raw agreement, all 300 cross-checked games (`a` = Luna, `b` = Claude): {"games": 300, "jaccard": 0.512, "a_confirmed": 0.556, "b_confirmed": 0.841, "scalars": {"length": 0.528, "session": 0.924, "difficulty": 0.805, "text_heavy": 0.922}, "a_shipped_confirmed_on_b_axes": 0.8}
curatordb.bin: 233 KB

- **Sizes:**
  - `assets/curatordb.bin` is 233 KB.
  - `assets/gamedb.bin` grew from 833 KB to 896 KB.
  - In total about 296 KB was added to the binary. The vocabulary JSON is only compiled into tests.
- **A library list:** of 323 game files in the test library, 247 match a gamedb entry with the device's rules.
  244 of those games are tagged, and 201 of them are cross-checked.

**Reading the agreement.**
- The raw numbers above count every tag. The second labels were written sparsely, with precision first:
  often no pace or mood. So "not given" is often silence, not disagreement.
- The axis-aware share (80 %) counts only axes the second labeller covered. That is what `merge` acts on.
- Length is the weakest value. The two labellers chose the same bucket for 53 % of the games, and the
  golden set accepts Luna's length for 69 %. Treat length as a rough guide (± one bucket).

**Examples** (as shipped; the weight in brackets):

| Game | Tags | Length, session, difficulty, text |
|---|---|---|
| Burnout Revenge (PS2) | racing (3) · arcade racing (3) · crash takedowns (3) · racing through traffic (3) · drifting (2) · score chasing (2) · frantic pace (2) · needs an analog stick (2) · adrenaline rush (3) · competitive (2) | medium, pickup, moderate, not text-heavy |
| Chrono Trigger (SNES; the same on PS1, DS, Wii, PSP) | role-playing (3) · Japanese RPG (3) · turn-based battles (3) · active-time battles (3) · party building (3) · time travel (3) · multiple endings (3) · calm pace (3) · story-driven (3) · cinematic (2) · plays well on a d-pad (3) · epic (3) · fantasy (2) · sci-fi (2) | long, sitdown, moderate, text-heavy |
| Gran Turismo 4 (PS2) | racing (3) · sim racing (3) · car tuning & collecting (3) · fast pace (2) · needs an analog stick (2) · competitive (2) · versus on one screen (2) · real cars, teams and players (3) | epic, sitdown, moderate |
| Mario Kart 64 (N64) | racing (3) · kart racing (3) · drifting (2) · needs an analog stick (2) · cheerful (2) · competitive (2) · versus on one screen (3) · up to four players (3) · cartoon world (2) | endless, pickup, moderate |
| Castlevania: Symphony of the Night (PS1) | action-adventure (3) · role-playing (2) · metroidvania (3) · action RPG (2) · new abilities open the map (3) · lots to collect (2) · measured pace (1) · plays well on a d-pad (2) · dark (2) · stylish (2) · fantasy (3) · horror (2) | long, sitdown, moderate |
| Animal Crossing (GameCube) | life sim (3) · calm pace (3) · cosy (3) · cute (3) · cheerful (2) · cartoon world (2) | endless, pickup, easy |

Known misses of this kind remain:
- Burnout Revenge's two-player split screen is not tagged.
- Phoenix Wright gets "story choices" and "multiple endings".
- Animal Crossing is not marked text-heavy.
