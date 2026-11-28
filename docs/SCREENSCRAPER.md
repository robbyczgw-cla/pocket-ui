# ScreenScraper as a source of box art and the box's other faces

In 0.8: the client, the storage and the index (tested against recorded answers and in live runs, below), the settings
row with its background run (`src/app_screenscraper.rs`) and the real spines on the shelf (below). The box backs and
labels in the game box and the insert animation are read by `media`/`insert_rig`/`game_box` (feat/screenscraper-labels). ScreenScraper ([screenscraper.fr](https://www.screenscraper.fr),
API v2: [webapi2.php](https://www.screenscraper.fr/webapi2.php)) has what the libretro thumbnail collection
([covers.rs](../src/covers.rs)) lacks: the box's back and spine, the whole box as a texture, the cartridge or disc label.

| Module | What it does |
|---|---|
| `src/screenscraper.rs` | The developer id (`dev`, `available`), the player's login (`login`, `save_login`, `forget_login`), systems (`system_id`), what a game is asked for by (`identity`), regions (`rom_regions`, `region_order`, `pick_media`), the pictures and their folders (`Face`, `path`, `existing`), the answers (`parse_user`, `parse_game`, `Quota`), what ends a run (`Refusal`, `refusal_for`, `day_over`), the index (`Index`), the client (`Client`) and the job (`run_until`) |
| `src/screenscraper_facts.rs` | What `jeuInfos` says besides the pictures: the description per language, genre, players, developer, publisher, years per region, rating (`Facts`, `parse`, `save`, `load`), and what of it the game card takes (`fill_card`); tests in `screenscraper_facts_tests.rs`, `tests/screenscraper_card.rs` (below) |
| `src/screenscraper_mask.rs` | How the developer id is kept in the binary (shared with `build.rs`) |
| `src/screenscraper_tests.rs` | Every path against a fake server answering from `testdata/screenscraper/` |
| `src/app_screenscraper.rs` | The App's side: the settings row's state (`RowState`), the background run in passes, sign in and out |
| `examples/screenscraper_live.rs` | A live run from a host build (below): the real job against the real server, a line per request, a table per game |
| `scripts/screenscraper-env.sh` | The release builds' way to the id: reads the build machine's file, exports it for `build.rs` |

The HTTP is RomM's ([ROMM.md](ROMM.md)): `romm_net::Net`, the system's curl on Linux (the address with the ids goes in
through stdin as a curl config, never on a command line), Android's HTTPS (`RommHttp.java`) on Android. Only `https://`.

## The developer id

ScreenScraper's API is for free, distributed frontends that present themselves in its forum and get a `devid` and
`devpassword`; `softname` names the program. pocket-ui's application is under way (25.11.2026). The id is never in the
repository:

1. The build machine keeps it in `~/.config/screenscraper-dev` (mode 600; `SCREENSCRAPER_DEV_FILE` names another file):
   lines `DEVID=…`, `DEVPASSWORD=…` and, optionally, `SOFTNAME=…` (default `pocket-ui`).
2. `scripts/screenscraper-env.sh`, sourced by `scripts/release-app.sh` and `scripts/android/build-apk.sh` (and to be sourced
   by the release kit's ARM build, next to `release-rustflags.sh`), reads the file without running it and exports
   `POCKETUI_SCREENSCRAPER_DEVID`, `POCKETUI_SCREENSCRAPER_DEVPASSWORD`, `POCKETUI_SCREENSCRAPER_SOFTNAME`. It prints only whether the id is in the build.
3. `build.rs` packs them (XOR with a fixed mask, `screenscraper_mask.rs`, so the binary has no plain text of them) into
   `$OUT_DIR/screenscraper_dev.txt`, never into a printed `cargo:` line (cargo keeps those in the build's output file and shows them
   with `-vv`); `screenscraper::dev()` includes and unpacks it. Without both values the file is empty: the build has no ScreenScraper
   (`available()` is false, no row, no request). The gate's and every developer's build are such builds.

The mask is no protection against someone who wants the id; every frontend with a ScreenScraper client ships its id like
this or plainer. Never print, log or commit it.

## The player's login

Optional (`ssid`, `sspassword`): a guest's quota is small, an account's larger (and larger again for contributors).
Kept in `<data>/screenscraper/login` (user and password on two lines, mode 600, `secret::write_private`, as RomM's
token), never logged or put on a command line. `forget_login` signs out; the pictures stay.

## A game's question

`jeuInfos.php` with `crc`, `romtaille`, `romnom`, `romtype=rom`, `systemeid`: the API wants a checksum and the size.

- **The checksum** is `rom_id`'s: for a zip or 7z the CRC-32 and unpacked size its directory states (nothing is unpacked);
  for a loose file the CRC `rom_id` keeps in `identify/checksums.tsv` (worked out here the same way when it isn't there yet).
  A file `rom_id` has a second CRC for (an iNES, SNES copier, PC Engine or Lynx header left out; N64 bytes in big-endian
  order) is asked for a second time with that CRC and the size without the header, only when the first was not known.
- **Left out** (no request, counted as `no_checksum`): systems without a No-Intro list in `rom_id` (discs: PlayStation's
  serial is no checksum the API takes), rar, a zip of several games, files over `rom_id::MAX_HASH`, systems
  ScreenScraper has no id for (`system_id`, the same numbers as ROCKNIX's EmulationStation scraper).
- **The pictures** come from `mediaJeu.php` by `jeuid`, `systemeid` and `media=<type>(<region>)`, as PNG
  (`outputformat=png`) and at most `Face::max_edge` pixels on a side (`maxwidth`, `maxheight`: 1024 for box faces and labels,
  2048 for textures, 640 for screenshots). A file is kept only when its PNG header is one pocket-ui decodes
  (`image::png_size`); a `NOMEDIA` text, an error page or a broken file leaves nothing behind. At most
  `MAX_MEDIA_BYTES` each.

## Regions

`region_order`: the regions ScreenScraper gives the ROM (`rom.romregions`), else the ones its file name's tags name
(`rom_regions`: "(USA, Europe)" → `us`, `eu`; "(J)" → `jp`; "(Germany)" → `de`), then `wor`, `us`, `eu`, then `jp` and
`ss`. `pick_media` takes the first region of that order the game has a picture of, else any of that type (a Japanese
box back beats none).

## Keeping ScreenScraper's rules

The API makes quota handling the client's job.

- **One request at a time.** One thread, whatever `maxthreads` allows, and one request at a time on the whole handheld
  (`LANE`: a sign-in's check waits for the run's request, and the run is stopped first). Accounts with more
  `maxthreads` (contributors) don't load in parallel yet.
- **Its pace.** `ssuserInfos.php` once before a signed-in run's first request (a run with nothing to fetch asks nothing; a
  guest asks none, ScreenScraper answers a guest's with 403 "Vérifier les identifiants utilisateurs", and takes its numbers
  from the `ssuser` of each `jeuInfos` answer); between two
  requests at least `60 s / maxrequestspermin` (1.2 s while it isn't known). A picture's download takes at least its size
  over `maxdownloadspeed`.
- **Its day** (`Ledger`, `<data>/screenscraper/day.json`, one entry per quota: `guest`, `user:<name>`): every request
  (`jeuInfos`, `ssuserInfos`, each `mediaJeu`) and every miss (404) is counted on the card before the next one goes, so a
  new run, a new start or another card the same day goes on counting. The server's `requeststoday` / `requestskotoday`
  only raise the counts, never lower them (they move in steps of ten). Before every request: an account stops at its
  `maxrequestsperday` and `maxrequestskoperday`; a **guest** at half of its `maxrequestsperday` (`GUEST_SHARE`, 5000 of
  the live 10000; the guests' quota is shared) and after `GUEST_GAMES_PER_DAY` (1000) games asked for, the pictures of
  games already known on top (the owner, 26.11.2026): when the games are used up, new look-ups wait for tomorrow
  (`Run::held`, the shelf says "Spines: more tomorrow") and the known games' pictures go on. A guest's day and an account's
  are separate: signing in after the guest's day ended goes on, signing out after the account's did too.
  - **One owner of the day's file** (`DAY`, review round 3): every client in the program (the spines' run, a sign-in's
    check, the labels', the manuals') changes the counts only through `book` (before a request goes) and `note` (what
    an answer said, a miss, a day's end), each one transaction under one lock: the file read as it is on the card,
    checked field by field (`entry_from`: whole numbers, a known day's end; anything else is damaged), the day turned
    over for every count first, the limits checked, the booking made (a new game and its request in one), the file
    written atomically. Answers only raise counts, never lower them. A look (`Ledger::load`, `day_over`) never writes.
  - **An answer is taken in once, under the lane** (`Client::answered`): its miss (404), its day's end (430, 431) and its
    `ssuser` limits go into the file, on the day its request was booked on (an answer after midnight counts on the day
    before), before the next request anywhere in the program may be booked. Callers only read the answer's own data.
    A part the card refuses waits in `UNSAVED`, for every client: whoever books next writes it first, and nothing goes
    until it is written.
  - **The row's "Today"** is read from the file when a run or a sign-in ends (`Client::day_count`), not from a
    client's last look.
  - **Booked before it goes** (`Client::reserve`, under `LANE`); a stop (Off, a sign-out) is checked after the booking,
    right before the request goes, and a request booked but not sent stays counted.
  - **Midnight** (French, UTC+1) turns the day inside every booking, not only at a run's start: a game asked after
    midnight is one of the new day's.
  - **A limit not said yet** (a new handheld, or a day's file without it) is never spent on a guess: only requests whose
    answer says it (`jeuInfos`, `ssuserInfos`), at most `PROBES` (5) a day, counted in the file (a restart gets no fresh
    five); no picture at all until it is known. A known game is asked for again to learn it. A guest's limit of 1 leaves
    it nothing (half rounds to nought), never "no limit".
  - **The day's file** lives in the data folder (`/storage/.config/pocket-ui/screenscraper/day.json` on ROCKNIX, the
    app's own storage on Android): the handheld's own folder, not the games card; a second system card is a second
    installation with its own (a product decision). One that is there and damaged holds every quota back for the rest
    of the day (`{"broken": <day>}`, every quota starts afresh tomorrow); one that can't be written holds every request
    back. It is read directly, never through `background_io`, whose reads a game's start cancels.
  - **Signing in** is booked in the account's day like every request and is not sent when that day is over.
- **A game it doesn't know** (404) is noted in the index and not asked for again until its file changes: unknown games
  have their own, smaller daily quota.

| Status | Meaning (ScreenScraper) | What pocket-ui does |
|---|---|---|
| 200 | the answer | a game's line in the index, or the picture |
| 400 | a field is wrong | this game is left for the next run |
| 401 | closed for guests and inactive members (server busy) | the run ends: "ScreenScraper is busy right now…" |
| 403 | wrong developer id; a wrong login | the run ends: "…doesn't accept this pocket-ui version…" or "…didn't accept your name or password…" (told apart by the answer's text). A picture's 403 has no text: an account's `ssuserInfos` tells a refused login; anything else ends the run with the neutral "ScreenScraper didn't hand out pictures just now…" |
| 404 | game / ROM not found (counted as a miss) | `jeuInfos`: noted as not known; `mediaJeu`: the picture taken off the game's list |
| 423 | API closed | the run ends: "ScreenScraper is closed for now…" |
| 426 | this program is blacklisted or too old | the run ends: "ScreenScraper wants a newer pocket-ui…" |
| 429 | too many threads / requests a minute | wait `BUSY_WAIT`, try again; twice, then the run ends as busy |
| 430 | the day's quota is used | the day ends: "That's all ScreenScraper allows for today…" |
| 431 | too many unknown ROMs today | the day ends: "ScreenScraper didn't know many of your games today…" |
| none, 5xx | no answer | this game is left for the next run; `GIVE_UP_AFTER` in a row end the run |

A day that ended (430, 431, the guest's brake: "That's all for today. The rest comes tomorrow.") is kept in that quota's
entry of `day.json` until the next French midnight (taken as UTC+1, so in summer an hour late, never early); a run
before then asks nothing, and the shelf says "Spines: more tomorrow" where its quiet dot is.

A **stop** (Off, Pictures Off, a game, a sign-in or sign-out) gives up the request it is on, the picture's download too
(its stop flag is set from the download's progress); a stopped picture stays on the game's list. Only ScreenScraper's
own "none" (`NOMEDIA`, 404) takes a picture off it; a broken file (decoded whole before it is kept), an error page or
a card that refused it is tried again next run. A sign-in or sign-out makes the running run's later answers (its
progress, its end, an older sign-in's answer) speak for nobody (`ScreenScraperUi::generation`; Off counts too, so a
sign-in that succeeds after Off is not kept); the login is kept on the UI's side, only from the newest sign-in. A
picture's 403 whose diagnosis (`ssuserInfos`) gives a definite answer (a refused login, the day's end, closed) ends the
run with it; only an ambiguous one gets the neutral line.

**Off** (the owner, 26.11.2026): no request at all, ever; the spines already downloaded stay on the shelf
(`tests/screenscraper_shelf.rs`). Off or offline with nothing downloaded, the shelf is today's, pixel for pixel. On
Android `wifi::ip` can't tell offline: a run there sends its first requests and ends after `GIVE_UP_AFTER` silent
ones, showing nothing on the shelf (the line comes only once a spine did, and the run shows no dot).

## Where the pictures go

| `Face` | ScreenScraper type | Folder under the data folder | Read by |
|---|---|---|---|
| `Cover` | `box-2D` | `covers/<system>/<rom stem>.png` | the library as it is (`covers::active_cover_path`), every layout, labels printed from the cover |
| `Snap` | `ss` | `snaps/<system>/…` | Split's cycle, the game card (`game_pictures`) |
| `Title` | `sstitle` | `titles/<system>/…` | the same |
| `BoxBack` | `box-2D-back` | `boxbacks/<system>/…` | the game box's back, first under its title (`game_box`, `App::real_back`) |
| `BoxSide` | `box-2D-side` | `boxsides/<system>/…` | the spines in Shelf and Cabinet (`media::photo_spine`, below) |
| `BoxTexture` | `box-texture` | `boxtextures/<system>/…` | not yet: several faces of the 3D case |
| `Support` | `support-2D` | `supports/<system>/…` | a disc's face; a cartridge's label cut from it where no `support-texture` is (`real_label`) |
| `SupportTexture` | `support-texture` | `supporttextures/<system>/…` | the cartridge's or card's real label (`real_label`, `media::game_pic_labelled`); a disc's where no `support-2D` is |
| `Wheel` | `wheel` | `wheels/<system>/…` | not in 0.8 (`series_look::SERIES_LOGOS` off: not fetched, not shown; 0.8.1): a series' logo on its spine on the shelves by series ([SERIES-SHELF.md](SERIES-SHELF.md#its-cover-and-its-spine)); fetched for one game per series only |

A picture already there (downloaded from libretro, or the player's own) is never replaced; `existing` is what the
renderer will ask. The index, `<data>/screenscraper/<system>.tsv`, has a line per game file: `stem  CRC  size  state`, the
state `-` (not known) or `<game id>[@<system id>] <ROM regions> <type(region).format>,…` (every picture ScreenScraper has of
it; `@<system id>` when the game is under another system than the folder's, a Game Boy Color game in `gb` is under 10,
and `mediaJeu` wants that one; a picture `mediaJeu` answered with `NOMEDIA` is taken off the list, so it is not asked
for again). It is
valid while the file's CRC and size are the same. A stopped run goes on from it: a known game's missing pictures are
fetched by its id without asking for the game again. `forget` drops a game's line ("Look up again").

## Licence and credit

ScreenScraper's data and media are CC BY-NC-SA 4.0 and come from the sources the API page lists. pocket-ui keeps them only
as the player's local cache on the player's card: never in an image, a pack or a server of ours. Where the feature shows,
the credit line is `screenscraper::CREDIT`: "Data and media: ScreenScraper.fr (CC BY-NC-SA 4.0)".

## The settings row

Settings → Games → **Box art & downloads** ([docs/user/extras.md](user/extras.md#screenscraper)), the row
**ScreenScraper** right under Pictures (`setup.rs`: `SCREENSCRAPER_ROW`; the page's other rows keep their numbers through
`extras_logical` / `extras_at`), only when `shown()`: a build with the id, or `POCKETUI_SCREENSCRAPER_ROW=1` in a made-up run
(`POCKETUI_MOCK`, snapshots `setup:extras-screenscraper=<off|on|running|signed|quota>`).

- Value **Off** (default; `settings.json` `screenscraper`) or **On**; A and Left/Right switch it. Pictures **Off** stops
  it too (nothing downloads by itself).
- **Real spines**, **Real labels**, **Real box backs**, **Real manuals**: four rows right under it (`PICTURE_ROWS`,
  `picture_row`; the page's other rows move down by `screenscraper_rows`), On, On, Off and On unless switched
  (`screenscraper::Show`, `settings.json` `screenscraper_no_spines`, `screenscraper_no_labels`, `screenscraper_backs`,
  `screenscraper_no_manuals`: nothing written while they are as they start). Real manuals Off leaves out the Details
  row Game manual (`app_manual`); manuals on the card stay. A press switches one at once (`App::screenscraper_pictures_switched`: the spines, labels and
  the box's back are made again). What a frame shows asks `App::screenscraper_shows(face)`: that row On, else exactly
  what pocket-ui draws without ScreenScraper, also for pictures already on the card (`spine_photo`, `real_label`,
  `real_back` give nothing). ScreenScraper itself Off only sends nothing: what it brought stays shown. A run fetches
  only what is shown (`Show::fetches`, the passes filtered). Tests: `tests/screenscraper_rows.rs`.
- The explanation: what it brings, the account ("Signed in as …" or how to sign in), the last run's line ("Looking up 3 of
  120 games…", "42 new pictures.", a `Refusal::message`), today's count ("Today: 168 of 20000 lookups.") and the
  credit line.
- **X: Sign in** / **Sign out** (Sign in only while it is On, and online: Off sends nothing). Sign in types the name, then the password (shown as dots) on the on-screen keyboard
  (`Step::ScreenScraperLogin`); `App::screenscraper_sign_in` checks them with `ssuserInfos` on a thread and keeps them only when ScreenScraper
  takes them (`save_login`); a refused or unanswered one shows its line and is not kept.
- **On the shelf**, where the quiet dot by the clock is otherwise: "Getting spines 34 of 120" while the spines come
  (the first pass), "Spines: more tomorrow" when the day ended (the owner, 26.11.2026). Off, Pictures Off, offline or idle: nothing,
  the shelf is exactly as without ScreenScraper (`tests/screenscraper_shelf.rs`).
- **The run** (`App::auto_screenscraper`): where the libretro covers' run ends or finds nothing missing
  (`auto_covers`), when it is switched on and after a sign-in; online, no game, not while another picture job holds
  `covers::claim`. Five passes (`PASSES`): spines, then the series' logos (`wheel`; in 0.8 off, `series_look::SERIES_LOGOS`, the pass has no game and sends nothing; only while Home is by series: for each
  series one game, `series_logo_games`: none when one of its games has a logo on the card, else the first of its ranked
  games the index lists a logo for, else its first game while none is in the index yet; with Real spines), then `box-2D-back` + `support-texture` + `support-2D`, then box art
  for the games still without (libretro's comes first), then missing `sstitle` + `ss` (for the box's back and the card).
  Existing front covers still get spines, backs and labels. Games recognised by the local No-Intro list come first,
  in library order within each group. `box-texture` is not fetched: nothing reads it yet. Starting a game pauses the run
  (`background_io::paused`). The shelf checks for the first spine as soon as it is saved, then groups later refreshes.
  The run refreshes all picture caches when it ends (`cabinet.covers_changed`).

## The spines on the shelf

`media::photo_spine` puts the photo on the case's template: into the printed part (band and label together,
`spine_photo_rect`; on pocket-ui's own cases only the label, their band in the system's colour stays), the case's top
(where a sticker goes) and its cut edge as on a painted spine.

- **Turned:** ScreenScraper keeps the SNES's and the N64's spines lying (680x97, title left to right), the others
  standing (GBA 88x700, MD 96x680, DS 63x458). `upright_spine` turns a lying one a quarter clockwise (its left end on
  top, the letters' tops to the right, as the shelf's titles read) and keeps it at most `SPINE_PHOTO_EDGE` long. Done
  once per file on the cabinet's reader thread (`Kind::Spine`), never in a frame.
- **Whole and in its own colours** (`spine_photo_fit`, the owner, 26.11.2026): never cut, never stretched, never
  tinted (the US N64 spines are green or blue in ScreenScraper's scans, and stay so); the case round it a neutral dark
  grey (`PHOTO_FRAME`), not the cover's colour. As large as fits with its shape kept,
  centred; the rest of the spine in the colour of the photo's own edge (the mean of its outermost pixels). The GBA
  photos are half as thick as pocket-ui's GBA box for their length, so they stand with an edge-coloured band each side.
- **Nothing over it:** the real spine carries its own printed title; ours over it read as text on text. The selected
  game's name stands large above the shelf. Games without a photo keep the painted spine with our title, unchanged.
- **Where:** every spine the app makes goes through `App::spine_or_photo`: the Cabinet's cases (`spine_pic`), the
  Cabinet's case coming out (`shown_pics`), the Shelf look's standing spines (`stand_pic`) and its selected case
  (`stand_front`). `CaseKey::photo` makes a spine again once its photo is read.
- Tests: `media::tests` (`a_lying_spine_photo_stands_up_reading_top_to_bottom`,
  `a_real_spine_shows_whole_never_cut_or_stretched_and_nothing_over_it`, `a_real_spine_has_no_title_written_over_it`,
  `a_picture_that_is_no_spine_stays_off_the_shelf`), `tests/screenscraper_shelf.rs` (Off/offline pixel for pixel
  today's shelf; a real spine beside painted ones; the shelf's line).

## Live runs

`examples/screenscraper_live.rs` runs the real job (`run_until`) from a host build against the real server (an x86-64
build for an x86-64 machine: the copy comes back to `<wt>/target/air/`). The id is read
at run time from `~/.config/screenscraper-dev` (never built in, never printed); `--record <dir>` keeps every answer with
the ids and `commandRequested` cut out.

```sh
tools/air-build.sh -f gate/examples/screenscraper_live <wt> build --profile gate --example screenscraper_live
<wt>/target/air/gate/examples/screenscraper_live /tmp/ss-data games.tsv --record /tmp/ss-rec --want side,back,support,supporttexture
```

26.11.2026, as a guest, from a Linux build machine: 114 game files of the test library (No-Intro sets and some hacks, loose and one zip).

| System | Games | Found | Not known | No checksum | Spine | Back | `support-2D` | `support-texture` | `box-texture` | Spine saved (lying/standing) |
|---|---|---|---|---|---|---|---|---|---|---|
| gb | 10 | 10 | 0 | 0 | 10 | 10 | 10 | 10 | 10 | 10 (0/10) |
| gba | 38 | 36 | 2 | 0 | 36 | 36 | 34 | 34 | 36 | 36 (0/36) |
| gbc | 1 | 1 | 0 | 0 | 1 | 1 | 1 | 1 | 1 | 1 (0/1) |
| megadrive | 2 | 2 | 0 | 0 | 2 | 2 | 2 | 2 | 2 | 2 (0/2) |
| n64 | 15 | 15 | 0 | 0 | 15 | 15 | 15 | 15 | 15 | 15 (15/0) |
| nds | 8 | 7 | 0 | 1 | 7 | 7 | 7 | 7 | 7 | 7 (0/7) |
| nes | 2 | 2 | 0 | 0 | 2 | 2 | 2 | 2 | 2 | 2 (0/2) |
| pcengine | 2 | 2 | 0 | 0 | 2 | 2 | 2 | 2 | 2 | 1 (0/1) |
| snes | 36 | 36 | 0 | 0 | 36 | 36 | 36 | 36 | 36 | 36 (33/3) |
| **all** | 114 | 111 | 2 | 1 | 111 | 111 | 109 | 109 | 111 | 110 (48/62) |

- Not known: two GBA ROM hacks (404, counted in `requestskotoday`). No checksum: a patched DS file over `rom_id::MAX_HASH`.
- Found by the first CRC for every headered SNES file (`.smc` with 512 bytes of copier header) and for the zip (its
  directory's CRC and unpacked size, `romnom` with `.zip`); `alt` was never needed.
- A guest's numbers (`ssuser` in `jeuInfos`): `maxthreads` 1, `maxdownloadspeed` 128 (KB/s), `maxrequestspermin` 3072,
  `maxrequestsperday` 10000, `maxrequestskoperday` 1000. The counts in the answers move in steps of ten.
- Sizes ScreenScraper sends at `maxwidth=1024`: spines up to 680 long, backs 680x497 (SNES, N64) to 800x800 (GBA),
  `support-texture` about 600 on the long side, `box-texture` 1090 to 1500 wide (1-2 MB, not fetched).
- Time: about 13 s a game for spine, back and both label pictures at the guest's 128 KB/s, mostly the backs; the spines
  alone are 20-160 KB. Hence the spines' own first pass.
- One spine per game and region: ScreenScraper lists one `box-2D-side` for each region, never a second side of the same
  box. All 15 N64 spines came as `us`; most US N64 scans are green, GoldenEye's is blue-grey: that is the one it has
  (the owner asked whether a system could pick one look; with one side per region there is nothing to pick).
- No 429, 430 or 431 came; their handling is tested against the fake server (`screenscraper_tests.rs`). The API's debug
  switches were not used.

## Facts and text

`jeuInfos`' answer has more than the pictures need: `synopsis` (per language), `genres`, `joueurs`, `developpeur`,
`editeur`, `dates` (per region), `note` (out of 20). `run_until` keeps them where it reads the answer
(`screenscraper_facts::save`, `<data>/screenscraper/facts/<system>/<rom stem>.json`): no request of their own, nothing of
the quota. Kept are only the languages the interface speaks; the text with its entities decoded, its line breaks one
space, at most 4000 characters, ending after a whole sentence.

When a card opens (`App::show_card`; again after Connections rebuilt its facts), `App::screenscraper_card_facts` reads the game's
file on a thread (snapshots at once) and `fill_card` adds what the card lacks, never replacing a fact:

| On the card | From | Only when |
|---|---|---|
| Year | `dates`: the region of the ROM's file name (`region_order`), else the earliest | the card has none |
| Released | "JP/US 2003 · EU 2004", at most four regions | the regions' years differ |
| Players, Developer, Publisher | `joueurs` ("1-4"), `developpeur`, `editeur` | the card has none |
| Genre | the main genre (`principale`) in the interface's language, else English | the card has none; pocket-ui's own (English, from its offline list) gives way to ScreenScraper's in the interface's language, the player's gamelist genre stays |
| Community rating | "18 of 20 on ScreenScraper", a fact of its own (apart from the player's Rating) | the card has no reviews |
| The description | `synopsis` in the interface's language, else English (`Summary::ScreenScraper`, "Text: ScreenScraper.fr, CC BY-NC-SA 4.0" under it) | the card has none, Wikipedia's (English) or RomM's; the player's EmulationStation gamelist text stays |

On the box's back ScreenScraper's description stops where the screenshot and title screen do (at least six lines; three on a narrow back such as the Brick's),
ends with "…" and "The whole text: SELECT" (the page has it all), and the facts follow across the whole back
(`game_box::COMPACT_LINES`; other descriptions keep the back as it was).

Facts from it put the credit line (`CREDIT`) under the card's facts and, unless its description already names it, under
the back's. What was kept shows with ScreenScraper On or Off, as the spines already downloaded do (decided 26.11.2026:
Off sends nothing, what came stays); a game without a file has exactly today's card (`tests/screenscraper_card.rs`). A
card's late Wikipedia text never replaces ScreenScraper's (`App::card_text`).

## The series

`jeuInfos`' `familles` (the series; the main one first, its English or world-wide name where there is one, else its first
language: most are only in French) and `numeros` (its number, when it is a plain one) are kept in the index's two last
columns (`families` joined with `;`, `number`; `-` for none; lines without them still load). `series_of(data, system)`
gives them by ROM stem to the series shelf, read on its own thread.

## Manuals

A game's real manual (`manuel`, always a PDF, one per region) is in `jeuInfos`' `medias` like the pictures, so the index
knows which games have one without a request (`screenscraper::manual::listed`, `available`). Study: notes/manuals-study.md
in the review folder (26.11.2026: 17 of 17 well-known SNES, GBA, N64, PS1 and PS2 games had one, typically 2.5 to 5 MB,
32 to 72 pages).

- **The row.** Details shows **Game manual** ("The original, page by page") only for a game the index lists a manual
  for, while ScreenScraper is On and Pictures is not Off (`app_manual::card_manual_row`; `settings.json`
  `screenscraper_no_manuals` hides it). The index is read once per system on a thread of its own when a card of it opens; the
  row comes with it. Without a manual the page is exactly as before (`tests/screenscraper_manuals.rs`).
- **Fetched when opened, one request.** A on the row opens the page (`manual_view`); a manual not on the card yet is
  fetched now on a thread (`manual::fetch`: `mediaManuelJeu.php?systemeid&jeuid&media=manuel(<region>)`, paced, counted
  in the day's ledger and refused like every request). Region: the player's language first (`language_regions`: `de`,
  `fr`, `sp` for Spanish, `pt`/`br`, `jp`, `kr`, `cn`/`tw`, `it`, `nl`), then the file's regions (`region_order`), else
  any. Offline: "Needs Wi-Fi: the manual comes from ScreenScraper". While it loads, the page shows the megabytes so far
  and a bar whose bright part moves with every piece that comes (the size is not known ahead). B stops the download.
- **Pages, not a PDF.** ScreenScraper's manuals are scans that Ghostscript put together: each page is exactly one JPEG
  (`DCTDecode`), no fonts. `manual_pdf::pages` reads the page tree (classic table or cross-reference stream with PNG
  predictors, object streams, `/Prev`, inherited resources; a broken table falls back to scanning for objects) and
  `manual::save_pages` writes every page's JPEG unchanged to `<data>/manuals/<system>/<rom stem>/<region>/001.jpg …`,
  then `pages.txt` (`jpg W H` or `other` per page) as the mark that the manual is complete (written in a staging
  folder of its own, `.part-<pid>-<n>`, renamed, removed on any failure). The cache belongs to the region picked: a
  player who switches language gets that manual, and the other one goes once it is there. Pages that together are
  more than the PDF (one picture on many pages) are not written: not readable. One manual is opened at a time from
  download to pages (`manual::open`), the card's list of pages is read on the loading thread too. The PDF is not kept. No PDF renderer, no new library: the same code on Linux and Android. A page that is not
  one picture shows "This page can't be shown here".
- **A file from the network.** `manual_pdf` takes at most 200 000 objects and 2000 pages, inflates no stream past
  16 MB and all of them together past 64 MB (a zip bomb stops there), nests at most 64 deep, follows no loop in the page
  tree, follows an indirect `/Length` at most 4 hops, and gives up after 5 s (read in the lexer's loop too). Everything
  made from the file (parsed values, names, every copy of a value, stream data) comes out of one budget of 32 MB and
  300 000 values, at most 50 000 entries per array or dictionary, names at most 127 bytes (the PDF standard's) checked
  before they are made; dictionaries move, they are not copied. Offsets, lengths and counts from the file are checked,
  never trusted (`MAX_*`). An
  encrypted file, one over a limit, or one without a single page that is a picture is "This manual can't be read here":
  `<data>/manuals/<system>/<rom stem>/<region>.unreadable` marks that region: it is not fetched again, another region
  of the game still is, and with every region marked the row is left out. A page whose picture does not decode (or
  whose header claims more than 8192 pixels a side, checked before decoding) says it can't be shown and is not asked
  for again; at most three pages decode at a time.
- **The page.** One page whole on the screen; **L1/R1** (and Left/Right while whole) turn it; **A** zooms 2x, 3x and
  back to whole, as the pause menu's Zoom; the **D-pad** moves the part; **B** leaves the zoom first (as on the box's real back), then the manual. On a touch screen a swipe to the left or right turns one page,
  and so does a tap on the right or left half (`ManualView::touch`, `app_touch`). "3 / 32" (and "2x") on the hint
  bar's line at the left, never over the page. Pages are decoded on threads (`manual_view::decode`, at most 2400 pixels on the longest side, the
  JPEG's own 1/2 and 1/4 steps), this one first, then the next and the one before; three are kept in memory (about 8 MB
  each at most). `--snapshot manual:<system>:<game>[:<keys>]` paints it (`r`/`l` R1/L1, `a` A, `u` `d` `<` `>`;
  `POCKETUI_SNAPSHOT_MANUAL=loading` as it waits).

## Still open

- Discs (PlayStation, Saturn, …) have no checksum the API takes (`identity`): a look-up by `serialnum` (PlayStation's
  serial from `rom_id`) or by `romnom` + `romtaille` comes after 0.8.
- `box-texture` (the whole box unfolded) is not fetched until something reads it.
- Signing in from the phone (QR code, the web companion): after 0.8; 0.8 signs in with the on-screen keyboard.
- Parallel downloads for accounts with more than one `maxthreads`.
- The release kit's ARM build sources `scripts/screenscraper-env.sh` beside `release-rustflags.sh` (release-kit
  template, the lead's part); `scripts/release-app.sh` and `scripts/android/build-apk.sh` do already.
