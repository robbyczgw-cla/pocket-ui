# Your library: your files, gamelists and discs

How [Your library](library.md) reads your existing gamelists, PlayStation discs, filenames and cards from other
handhelds.

## Your gamelists

If your SD card already has `gamelist.xml` files (from ROCKNIX's own menu or another handheld), they often contain game
titles, descriptions, ratings, release years and favourites from a scraper. pocket-ui reads them in read-only mode (**it
never changes them**, except that deleting a game from the card also removes that game's entry) and uses them to fill in any gaps:

- **Where it looks:** `gamelist.xml` inside each system's ROM folder (`roms/gba/gamelist.xml`), the system's `gamelists`
  folders, and matching `gamelists/<system>/` or `downloaded_media/<system>/` folders on the card. If a game appears in
  more than one file, the first match wins.
- **Names:** cryptic filenames (`sf2ce.zip`, `kof98`, `1943`) pick up the clean title from the gamelist ("Street Fighter
  II' Champion Edition"). Already-readable filenames keep pocket-ui's own cleaned title ("Tetris", "Super Mario Land")
  even if the gamelist includes region or version tags.
- **Game card details:** when pocket-ui's built-in metadata is missing, the card pulls developer, publisher, genre,
  player count, year, rating and **Description** from your gamelist. Downloaded Wikipedia summaries and Wikidata facts
  take priority once present, and the footer always credits where the facts came from.
- **Box art:** if a game has no cover yet, pocket-ui displays the box image (or screenshot) linked in your gamelist
  right from its original path without copying anything. Images larger than 1 MB are skipped so scrolling stays smooth.
  Games with a local box image aren't looked up online; games with only a screenshot still fetch a proper box cover
  automatically, which then takes priority.
- **Favourites and ratings:** nothing is imported automatically. Open Settings → Games → **From EmulationStation** to
  see how many gamelist favourites and ratings were found, then press A to add those favourites or import their ratings
  as **My rating** (0.1 to 1.0 maps to 1 to 5 stars). It only adds: existing favourites and your own star ratings are never
  overwritten. You can run it anytime; if no gamelists exist, it tells you where to place them.

Broken, truncated or huge (over 32 MB) gamelists won't break your library: valid entries before the corruption still
load, alternate encodings (Latin-1, UTF-16) are handled, and unreadable files are skipped cleanly. Parsed results cache
to `/storage/.config/pocket-ui/gamelist-cache.json` for fast startups, auto-refresh when a gamelist changes, and can be
deleted safely anytime.

## PlayStation 1 and 2 discs

PlayStation discs are identified by the **serial number printed inside the disc itself**, not just the filename.
pocket-ui reads a few kilobytes from the disc's boot file (`SYSTEM.CNF`, such as `BOOT = ... SLES_556.73`), never the
whole image, and matches it against the Redump PlayStation 1 and 2 database (via libretro-database), just like cartridges
match by checksum ([Box art & downloads](extras.md#box-art)). A file named `SLES_556.73.Something.iso` or
`SLUS_209.46.iso` shows its real title ("Final Fantasy VII", "Gran Turismo 4") along with matching facts and box art.
The serial list downloads once per console you actually have games for (~4 MB each, when online) and stays cached on
your card.

- Your own names win: custom titles from your [gamelist](#your-gamelists) stay as you set them.
- Unknown serials fall back to the filename, stripping a leading serial when a title follows it (`SLES_556.73.Something`
  becomes `Something`, while `SLUS_209.46` stays as-is).
- Supported formats: `.iso`, `.bin`, and `.cue`/`.m3u` sheets pointing to them (including PlayStation's MODE2/2352
  sectors). Compressed `.chd`, `.cso` and `.pbp` images aren't unpacked for serial lookup and keep their filename.
- Multi-disc games keep "(Disc 2)" after the title so discs stay distinct.
- Scanning takes just milliseconds per disc in the background at lowest priority (never while a game runs), and only
  reruns if a file changes.

Serial matching feeds both the game card and Curator for PlayStation (PS1) and PlayStation 2. Other disc consoles
(Saturn, Dreamcast, PSP, GameCube, …) continue matching by filename.

## Matching your files

To match games with metadata and box art, pocket-ui cleans up filenames automatically: region/revision tags like
"(USA, Europe) (Rev 2)", dump tags in square brackets, scene release numbers ("0902 - ") and trailing versions
("Pipes.v1.02") are ignored; "Legend of Zelda, The" normalises to "The Legend of Zelda"; accents, "&"/"and" and Roman
numerals ("III" = "3") match interchangeably. No-Intro and Redump naming gives the best results. Sequel numbers stay
strict: "Crash Bandicoot 3" will never match "Crash Bandicoot 2".

If two files for the same console would clean down to the exact same title, pocket-ui keeps the distinguishing tag
("Tobu Tobu Girl (Europe)" and "Tobu Tobu Girl (World)"). That only affects display names in lists; saves, box art, play
time, favourites and notes stay tied to each file. Recent games (Continue and SELECT), Play history and the web
companion all use the same cleaned names.

For Lens, smart shelves, Echoes, Versions, play history and Year in review, see [Curator](curator.md).

## A card from another handheld

Already have a ROM card set up for another handheld? Plug it in as-is, with no renaming, moving or re-scraping required.
pocket-ui reads your games right where they sit without altering your folders:

- **Flexible folder names:** alongside standard names (`gba`, `snes`, …), pocket-ui understands descriptive folders like
  `Game Boy Advance (GBA)` (tags in brackets like `(MGBA)` or `(SFC)` work too), `GBA`, `FC`, `MD`, `PS`, `genesis`,
  `Sega Genesis` and `Nintendo Game Boy Advance`. Multiple folders for the same console merge cleanly into one list, and
  any unknown folder shows up in Settings → Games → Game check.
- **Existing artwork comes first:** pocket-ui checks `images`, `Imgs`, `.media`, `covers`, `downloaded_media` and `box`
  folders before downloading anything. Box art takes priority, followed by mixes and screenshots (logos, marquees and
  fan art are skipped). Online box art only downloads when no local box cover is found.
