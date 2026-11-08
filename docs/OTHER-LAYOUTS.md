# Other systems' cards: folders and box art

Someone who switches to pocket-ui brings an SD card (or a copy of its folders) from another frontend. pocket-ui reads it **where it
is**: no renaming, nothing moved, nothing written into the player's folders (our cache, `covers/`, stays in the data folder).
This page is the research (public documentation of each system, names and folder conventions only; no code taken from any of
them) and what pocket-ui does with it. Code: `src/layouts.rs` (names, picture folders), `library.rs` (`foreign_folders`,
`system_for_folder`), `gamelist.rs` (covers). Tests: `tests/other_layouts.rs`, one fixture tree per layout.

## The layouts

| System | ROM folders | Game names | Box art and where |
|---|---|---|---|
| **EmulationStation** (RetroPie, ArkOS, JELOS, ROCKNIX, AmberELEC) | `roms/<id>/` (`gba`, `snes`, `megadrive`, `psx` …) | `roms/<id>/gamelist.xml` (or `~/.emulationstation/gamelists/<id>/`) | `<image>` (the scraped picture: a screenshot, a mix or the box), `<thumbnail>` (a small box), `<marquee>` (logo), `<video>`; files in `roms/<id>/images/<game>-image.png`, `-thumb.png`, `-marquee.png` |
| **Batocera, KNULLI** | `/userdata/roms/<id>/` (ES ids: `gba`, `megadrive`, `wswan`, `pcengine`, `atari2600`, `lynx` …) | `gamelist.xml` in the folder | `<image>` (screenshot or mix), `<thumbnail>` (**the box**), `<marquee>` (wheel), `<fanart>`, `<titleshot>`, `<video>`; files in `images/` (`<game>-image.png`, `-thumb.png`, `-marquee.png`) |
| **ES-DE** | `ROMs/<id>/` (ES-DE ids: like the above, but `genesis` beside `megadrive`, `sfc` beside `snes`, `tg16`, `megacd` …) | `ES-DE/gamelists/<id>/gamelist.xml` (the folder's own is ignored by ES-DE; most users have one only after moving from another ES) | **No tags**: `ES-DE/downloaded_media/<id>/<type>/<game file name>.png\|jpg`, types `covers`, `3dboxes`, `miximages`, `screenshots`, `titlescreens`, `marquees`, `backcovers`, `fanart`, `physicalmedia`, `videos`; the name must equal the ROM's, a game in a subfolder mirrors the subfolders |
| **Anbernic stock firmware** (RG35XX, RG28XX …) | `Roms/<SHORT>/` in capitals: `GB`, `GBC`, `GBA`, `FC`, `SFC`, `MD`, `PS`, `PCE`, `MS`, `GG`, `NGP`, `ARCADE`, `MAME`, `FBNEO` … | file names only | `Roms/<SHORT>/Imgs/<game file name>.png` (what Skraper writes with its output folder set to `Imgs`) |
| **MinUI / NextUI** (Trimui Brick, Miyoo, RG35XX …) | `Roms/<Name> (<TAG>)/`: `Game Boy Advance (GBA)`, `Super Nintendo Entertainment System (SFC)`, `Sega Genesis (MD)`, `Sony PlayStation (PS)`, `Nintendo Entertainment System (FC)`; extras `(MGBA)`, `(SUPA)`, `(PCE)`, `(NGP)`, `(P8)`, `(FDS)`, `(LYNX)` … Folders may be renamed but keep the tag; two folders with the same name (GBA + MGBA) are one list | file names only (`map.txt` renames, not read) | NextUI only: `Roms/<folder>/.media/<game file name>.png` (a disc in its own folder: `<folder name>.png`); MinUI has no box art |
| **muOS** (Anbernic, Trimui) | `ROMS/<anything>/`: the system is a core assignment, not the folder name; the folders are usually named like the catalogue: `Nintendo Game Boy Advance`, `Sony PlayStation`, `Nintendo SNES-SFC`, `Sega Mega Drive - Genesis` | file names (`MUOS/info/name` has friendly names, not read) | `MUOS/info/catalogue/<System name>/box/<game>.png` (box), `preview/` (screenshot), `text/`; names match without regard to case |

## What pocket-ui does

**Systems.** The device's `es_systems.cfg` names the folders it reads (`/storage/roms/<id>`). Besides them pocket-ui reads every
other folder directly in `roms/` whose **name means one of the device's systems** (`layouts::system_ids`): the tag in brackets
(`Game Boy Advance (GBA)`: the tag decides, so `MGBA`, `SUPA` work too), the short capital names (`GBA`, `FC`, `MD`, `PS`), the
ES-DE/Batocera ids (`genesis`, `wswan`), the long names of muOS and the usual spellings (`Sega Genesis`, `Super Nintendo`). It
needs no setting. A folder that is a system's folder already (a link, or the same folder under another case on exFAT) is not read
twice. A folder nobody knows (`Atari Jaguar` on a device without Jaguar) stays unread and is still named by Settings → Games →
Game check. Games found this way are listed and started like any other; the folder keeps its name.

**Names.** Unchanged rule: a file name that says little (`sf2ce`) takes the gamelist's `<name>`; a file name that already reads as a name
keeps pocket-ui's cleaned one, because tags and regions are what tells two versions apart.

**Box art**, in this order (`gamelist::SystemData::ranked`), the first usable file wins (at most 1 MB):

1. a box: the gamelist's `cover`/`boxart`/`thumbnail`, ES-DE's `covers` then `3dboxes`, a `-thumb`/`-boxart`/`-cover` file in `images/`,
   `.media/` (NextUI), `Imgs/` (Anbernic), muOS's `box/`, folders called `covers`, `boxart`, `box2d` … (also under `media/`);
2. a gamelist's lone `<image>` that sits in no screenshots folder (the scraper's one picture, most often the box);
3. a mix (ES-DE's `miximages`, `-mix`, a bare file in `images/`);
4. a screenshot (`screenshots`, `snaps`, `titlescreens`, `-image` beside a box, muOS's `preview/`).

Marquees, logos, wheels, fan art, cartridges, videos and manuals are never covers. **Download:** a game with a picture from 1 or 2 is
left out of the box art download (it would only fetch a second, equal picture); one with only a mix or a screenshot still gets the
collection's box, which then comes first (pocket-ui's own cover is looked at before the card's). The download waits for the first read of
the card's pictures (`App::covers_wait_for_gamelists`).

Picture folders are listed once, on the gamelist thread after the first frame (`layouts::art_dirs`), so a lookup is a map access, not a
`stat` per candidate. Pictures are read-only: the cache is `covers/` in the data folder.

## Not covered (on purpose)

- **Saves.** They stay next to the game / in pocket-ui's own folders as before; a MinUI/NextUI `Saves/<TAG>/` or a muOS save folder is not read here
  (the Brick card does that itself).
- **Names from `map.txt`, muOS `MUOS/info/name`, MinUI collections, Anbernic stock's `*.db` lists.** Not read.
- **Systems the device has no emulator for** (`Pico-8 (P8)` on a device without PICO-8, `Commodore PET`): the folder is left alone.
- **Recalbox, OnionOS, spruce, Knulli's Batocera `.media` videos:** not researched in detail; Onion keeps `Roms/<SHORT>/Imgs/` like
  Anbernic stock and is read through that rule; spruce uses `Roms/<SHORT>/Imgs/` as well.
- **Cartridge-level matching** (a differently named copy of the same game): the picture must be named like the ROM (any case).
- **ES-DE media in subfolders** (`covers/<subfolder>/<game>.png`): only the top level is read.

## Numbers

Measured 08.11.2026 on a development machine (AMD Ryzen 5 PRO 4650G, gate profile, 5,000 games and 5,000 pictures, `tests/other_layouts.rs`, `timing_…`, run with `--ignored --nocapture`):

| Card | Library read | Pictures listed | "Has a box?" for all 5,000 (once per start / trigger) |
|---|---|---|---|
| The device's own folder, no pictures (what every start pays for) | 19 ms (16.5 ms before this change: +3 ms for the second look at `es_systems.cfg` and `roms/`) | 1.6 ms | 0.01 ms |
| Folder with a tag in brackets, `.media` beside the games | 19 ms | 4 ms | 10 ms |
| Device folder, `gamelist.xml` with 5,000 games, `images/` | 51 ms (20 ms of it the gamelist, parsed once per change) | 6 ms | 12 ms |

The first version of the folder merge compared every game with the list so far and took 866 ms for 5,000 games; the timing test found it.

## A big library: 13,000 ROMs

A switcher with about 13,000 ROMs across systems (a Reddit case). Fixture: `python3 scripts/other-layouts-big-fixture.py /tmp/fx13k`: 13,000 empty fake ROMs
in nine systems and four layouts at once (gba 2,500 with a `gamelist.xml` of 2,500 games and `images/`; snes 2,500 in `Super Nintendo Entertainment System (SFC)` with
`.media`; megadrive 1,800 in `MD` with `Imgs`; psx 800 disc folders in `Sony PlayStation (PS)` with `.media`; nes 2,200, gb 900, gbc 600, n64 700, arcade 1,000 plain).
Library numbers: `POCKETUI_BIG_FIXTURE=/tmp/fx13k cargo test --profile gate --test big_library -- --ignored --nocapture`; the app numbers are `pocket-ui --snapshot main` and
`--snapshot bench-shelf:<look>:<system>` on the same tree (`/usr/bin/time` for memory). Host, warm page cache.

| What | Host (Ryzen 5 PRO 4650G) | Brick (A133P), estimated |
|---|---|---|
| First scan, no cache (library read before the first frame) | 79 ms in the app (main without this change: 49 ms for the 7,900 games it can see); 80–380 ms in the test when the directories were not just used | 0.5–0.7 s of CPU at 6–8× slower, plus the card's reading of 13,000 directory entries cold (not measured) |
| Re-scan (after the first frame, on a thread) | 65–70 ms | about 0.5 s on the thread, not on the UI |
| Pictures: gamelist + picture folders listed (after the first frame) | 11–16 ms | about 0.1 s |
| "Has a box?" for all 13,000 games (UI thread, when the box art download decides) | 16 ms (7,600 with a box) | about 0.1 s, once per start / trigger |
| Library cache (`library-cache.json`) | 1.4 MB; written in 18 ms (thread), read in 6.7 ms | start from the cache: about 50 ms |
| Whole `--snapshot main` (start, scan, first frame) | 0.09 s wall, 37 MB RSS peak | |
| A move on a system of 2,500 games, thread CPU p50 / p95 (limit 6 / 8 ms) | Shelf 1.1 / 1.4 ms, Cards 0.6 / 1.0, Split 0.3 / 0.4, Cabinet 2.7 / 3.1; Home with all systems (Cards) 0.5 / 0.5 | 6–8× the host: Shelf, Cards, Split below one frame (16.7 ms); Cabinet 16–22 ms: the same as with 300 games, it does not grow with the library |
| Memory, `bench-shelf` (peak RSS of the whole process) | Shelf 88 MB, Split 72, Cabinet 93, Cards 173 (the cover cache of the moves) | 1 GB RAM: no problem |

The Brick column is an estimate, not a measurement: no Brick was used. It takes the host numbers times 6–8 (a Cortex-A53 at about 1.5–1.8 GHz against a Zen 2 core at 4 GHz for
this kind of work: parsing, sorting, strings) and has no number for the card's own speed; the cold directory reads and the first
`gamelist.xml` parse (20 ms per 2,500 games here) are the unknowns. The scan is linear: 6 µs per game, the same for folders of other cards as for the device's own.

What would make 13,000 unusable, and what does not:

- **Nothing in these numbers does.** The scan, the pictures, the cache and the moves are linear or flat in the library size. The shelf's move cost does not depend on it (docs/LATENCY.md: 2,000 and 5,000 games alike).
- **The first start without a cache** reads all folders before the first frame (the cache's rule): about 0.1 s on the host, probably 1–3 s on a Brick with a cold card. Later starts show the shelf from the cache.
- **A hitch to watch:** "Has a box?" runs on the UI thread when the box art download decides (start, after a game, Wi-Fi up): 16 ms here, an estimated 0.1 s on a Brick, once, not while scrolling.
  If a Brick test shows it, move the pass onto the gamelist thread.
- **The box art download itself** is as before: 13,000 games without pictures on the card are fetched one by one over Wi-Fi at the lowest priority, which takes hours. Games with a box on the card are left out.
- **My first version** of the folder merge was quadratic (866 ms for 5,000 games); the timing test and an app-level run showed it, and it is fixed (a stale binary of mine briefly showed 3.9 s for 13,000).
