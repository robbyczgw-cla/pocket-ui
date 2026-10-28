# Play history

Status: reference. Describes `feat/play-history` as built (2026-10-23), with the fixes of review SOL-2026-10-24
(sleep left out, session numbers, the migration and the file's identity made safe, the flush at exit). The player's guide is
[user/curator.md](user/curator.md#play-history); the buttons are in [user/controls.md](user/controls.md#in-history).

A log of play sessions on the device, Curator → **History** (a week, a month or a year), the **Year in review**
(cards, opened only by the player) and a read API for the web companion. Settings → Playing → Curator → **Play history**
(on by default) switches it off: nothing new is recorded, what was recorded stays.

## Ground rules

- **Nothing new on the UI thread.** Recording is a channel send; writing, reading and counting run on threads of
  their own. Opening History shows its title at once and the numbers a moment later.
- **Never unasked.** The review has no row on Home, no notice, no card that opens by itself: the Curator list's
  **Year in review** row and Y on History are the only ways in. History and the review rows appear in the Curator
  list (and, with the Lens off, History on Home) only once something was played.
- **Honest numbers.** Only what the log recorded is counted (see "What counts" below). What it can't place on a day
  says so ("Not recorded: History counts from …") instead of showing zero.
- **Private.** The log is a file in pocket-ui's data folder. Nothing sends it anywhere; it is not part of a
  backup (Settings → System → Backup & reset copies to the games part of the card, which file sharing can reach). The web
  companion reads it only while Play history is on.

## Files

| File | What it is |
|---|---|
| `src/play_log.rs` | The log: the file format, the writer (`Writer`, on the `play-log` thread), the process-wide recorder (`started`, `ended`, `status`, `paused`, `resumed`, `suspended`, `open`, `settle`), the reader (`read`, `view`, `log_for`), the one migration (`Seed`, `seed_with`) |
| `src/play_history.rs` | Pure functions over a `Log`: days and periods (local, Monday weeks), `summarise` (a week, month or year), `review` (its cards), `all_time`, `secs_since`, the made-up history for snapshots (`demo`), and the read API `stats_json` |
| `src/history_page.rs` | History and the review: what a press does, the words of each card, the painting (chart, games, facts) |
| `src/app_history.rs` | The App's side: the writer after the first frame, the migration, the Curator rows, counting on a thread (`count`), the pages, a game's card from them and back, `--snapshot curator:history…` |
| `src/feature_previews.rs` (`play_history`), `src/features.rs` | The feature card (Settings → System → Tour): a month of made-up play |

Hooks elsewhere, each one line with a comment: `main.rs` calls `play_log::started` after a game's process is
spawned (`App::launch`), `play_log::ended` where the recent games note the session (`App::game_exited`), and
`play_log::settle` before pocket-ui exits; `app_card.rs` calls `play_log::status` when the card's status picker
saves; `sleep.rs` calls `play_log::paused` when light sleep begins, `play_log::resumed` when the screen wakes and
`play_log::suspended` when its watcher sees a suspend pocket-ui didn't start; `app_curator.rs` has the two `Page`
variants, the rows and `Event::History`.

## Recording

A session is recorded where the recent games and `library.json` count one: from `App::launch` to
`App::game_exited`. Its time is the seconds `game_exited` gets (the game's process time on the monotonic clock, which
stands still in a suspend) less the time in light sleep: the pause menu counts, sleep (light sleep and suspends) does
not. The recent games and the game card keep the figure with light sleep in it.

**Sleep.** Light sleep (the screen dark, the game stopped; a suspend of pocket-ui's own always follows it) ends a
stretch of the session: `play_log::paused` sends the wall and monotonic time, and the writer writes a pause frame with
the stretch's seconds of play. Waking (`play_log::resumed`) writes a resume frame with the wall time the next stretch
begins at; the end gives the last stretch the rest. A suspend pocket-ui didn't start (logind, noticed by `sleep.rs`'s
watcher up to two seconds later) ends a stretch where it began and starts the next where it ended, with no time taken
off (the monotonic clock never counted it). Each stretch is counted on the local days it was played
(`Session::parts`): ten minutes on New Year's Eve, a suspend through the night and twenty minutes in the morning are
ten minutes in one year and twenty in the next. A session whose end never came keeps its time unknown, stretches or
not.

The pause menu is counted on purpose: it is the same figure the game card and the
recent games show, and a pause in the menu is part of the evening's play; telling it apart would need every emulator
(pocket-core, the standalone ones, RetroArch) to report when its menu is open. Every game the shelf starts counts, PortMaster ports and PS2/GameCube/Wii/3DS too;
PC streaming and Settings → Games → Tools run as apps and are not recorded. A start whose files are missing (a port) never
reaches the log.

- The UI thread only sends: `play_log::started/ended/status` take a lock, push a command and return.
- Before `play_log::open` (called by `history_tick` once the first frame is up and the first-start setup is
  closed) commands wait in memory, so nothing of the log falls on pocket-ui's start.
- The `play-log` thread (nice 10) works out the local UTC offset (`appearance::utc_offset_now`), appends and
  `fdatasync`s. A game's start is written up to three seconds later (`START_QUIET`), or at once when the next
  command comes (its end, the flush at exit), so the write never meets the game's loading.
- `play_log::settle` (at exit) waits at most two seconds for what was handed over: the writer tries again what a full
  card held back (for up to one and a half seconds) and answers only once it is synced; what could not be written is
  said in the log (`play log: … could not be written before exit`).
- Play history off: commands are dropped; the writer thread, if it runs, stays idle.
- `POCKETUI_NO_HISTORY=1` (set by `scripts/latency.py`, [LATENCY.md](LATENCY.md)): nothing of that run is recorded,
  whatever the setting says; no writer thread, no migration. The log says
  `play log: POCKETUI_NO_HISTORY is set, nothing of this run is recorded` at start.

### The migration

The first time the writer reads a log that has not taken over the totals from before (`migrated`), the App hands
over `library.json`'s seconds and sessions and the recent games' last play (`App::history_seed`, once in a device's
life, after the first frame, never while a game starts or runs). On the UI thread that is a copy of the games
`library.json` counted time for (`LibraryMeta::played`, not the whole file), of the recent games and an `Arc` of
Curator's index; the seed is made on the writer's thread (`seed_of`, `play_log::seed_with`): names from the recent
games, else the index, else the file's name and folder. No library scan; with nothing played before, the seed is
empty and only marks the log as migrated. Each game becomes a **carried** record: it counts in all-time totals, as
"played before" (a game is not "new to you") and as the last play before a comeback, never in a week, month or year.
What the log already counted for a game is taken off first, so nothing counts twice.

A game is carried at most once: a migration a power cut stopped half way (carried records, no `migrated` frame) is
finished at the next start without carrying the games already carried again, and a reader counts only the first
carried record of a game (files the first version wrote twice read right too).

## Other devices' sessions (Library data)

With Settings → Sync → Library data on, the sessions and status changes of the player's other Pockets come in
through `library_sync` ([SYNC-CURATION.md](SYNC-CURATION.md#play-history-a-union)) and are handed to
`play_log::set_extra`. `view` and `log_for` then return the own log with them added (`play_log::combine`: their games
found by path or added, their sessions numbered after the own ones), so every page and the web companion count all
devices. They are never written into `play-log.bin`; `own_log` is the own log alone (what `library_sync` sends on).

## The file: `<data>/play-log.bin`

An 8-byte header (`PPL1`, version 1) and frames: `u8 kind | u16 length | payload | u32 CRC-32` (of kind, length and
payload). Little endian; strings are ids into a name table written as name frames the first time a string is used.

| Kind | Payload |
|---|---|
| 1 name | u32 id, UTF-8 text (paths cut at 4000 bytes) |
| 2 start | u32 unix start, i16 UTC offset (minutes), u32 path, u32 name, u32 system, u32 emulator, u32 session number (since 2026-10-24) |
| 3 end | u32 unix start, i16 UTC offset, u32 seconds played (sleep left out), u32 path, u32 name, u32 system, u32 session number (since 2026-10-24) |
| 4 status | u32 unix time, i16 UTC offset, u32 path, u32 name, u32 system, u8 status (0 not started … 4 dropped) |
| 5 carried | u32 path, u32 name, u32 system, u32 seconds, u32 sessions, u32 last played (0: unknown) |
| 6 migrated | u32 unix time |
| 7 pause (since 2026-10-24) | u32 session number, u32 stretch (0, 1, …), u32 unix time, u32 seconds played in the stretch |
| 8 resume (since 2026-10-24) | u32 session number, u32 stretch it begins, u32 unix time |

A session of a known game is 66 bytes (start and end, 33 each); each sleep in it adds a pause and a resume (42).

**Session numbers.** The writer numbers sessions 1, 2, … (the next after the highest in the file); a start, its
pauses, resumes and end carry the number, and the reader pairs them by it. Two sessions that began at the same clock
time (a clock that was reset) stay two, and a frame written twice (a write tried again) counts once: a second start or
end of a number, or a pause or resume of a stretch that is not the one running, is left out.

**Compatibility.** The new fields are at the end of their payloads and the new kinds are new numbers; the version
stays 1. This reader reads files of the first version as before: starts and ends without a number pair by start time
and file, a session without pauses is one stretch. pocket-ui from before 2026-10-24 reads files of this one with the
numbers ignored, but takes pause and resume frames for damage (it skips them and says "part of the history file could
not be read"; every session still counts, with its time and on the day it began). This reader skips kinds it doesn't
know without calling them damage, so later additions stay readable.

| What goes wrong | What happens |
|---|---|
| Power cut during a write | Only the last frame can be torn: the reader stops there, the writer cuts it off before its next append |
| A frame damaged on the card | It fails its CRC and is skipped; the reader looks for the next frame whose CRC holds. History says "Not counted: part of the history file could not be read" |
| A name frame lost | Its records read as nameless and are skipped (a game without a path); a new name never reuses a referenced id |
| The file is no log at all (zeros, another file) | Kept aside as `play-log.bin.damaged`, a new log begins; while that file is there, the period the new log began in says "Counted since …: an older history file could not be read" |
| The file replaced behind the writer (copied over the network, put back by hand) | The writer notices another file before its next append (device, inode, length, modification and change time, so also one of the same length with its time kept) and reads it again, so names never get mixed up |
| A write fails half way (the card) | What reached the card is cut back off at once (or before the next append, if that failed too), so the retry leaves nothing twice and nothing damaged |
| A game crashed, pocket-ui was ended, or the battery died | Its start has no end: a session (and a day played) whose time is unknown, never "under a minute" (see "Sessions without time") |
| A game that failed at once, never on the screen (a refused zip or patch, a missing BIOS: `fallback::never_started`) | Its start is closed with no time (`play_log::never_started`): no session, no day, not in a period's games or consoles |
| The card is full | The frames stay in memory and go with the next append, or with the flush at exit (`settle`) |
| Power cut during the migration | It runs again at the next start and carries only the games not carried yet |
| The file grows past 4 MiB (decades of play) | Rewritten atomically (`fsutil::write_atomic`) without the oldest sessions, down to about 3 MiB; carried records and statuses stay |

## Stickers

`history_page::card_art` gives a review card its picture (`ui_art`, `assets/ui-art/sticker-*.png`): Completed → a
trophy; the overview with 100 h or more → an hourglass; the most played console with ten games or more → a shelf of
cartridges; a streak of seven days or more → a flame; the game came back to → a birdhouse; an empty period's overview
→ the blank notebook History shows in its empty chart. With box art the sticker is painted over the box's top left
corner (a third of it outside), the words keep clear of it; without, it stands where the box would. Other cards have
none.

## What counts

| Number | Rule |
|---|---|
| Time | The seconds a game was open (its pause menu included; light sleep and suspends not), split at local midnight between the days it ran on (a session from New Year's Eve counts in both years); a session that sleep interrupted counts each stretch on the days it was played |
| Session | A start that ran for a minute or more (`library_meta::MIN_SESSION_SECS`, as on the game card), or one that never ended; shorter starts add time only. A session belongs to the day it started |
| Sessions without time | A session whose end was never recorded counts as a session and a day, its time as unknown: in no `secs`, counted apart as `untimed` (`Summary`, `GameTime`, `SystemTime`, `Bar`, `AllTime`) |
| Day and period | Local, in the zone the session started in; weeks run Monday to Sunday; a year's chart has one bar per week |
| Undated | Sessions from before the device's clock was set (before November 2023), or dated after today (a clock that was ahead, then put right), count in all-time totals only |
| Carried | The totals from before the log: all time, "played before", the last play before a comeback; never a week, month or year |
| Streak | Days in a row with a session; the one "now" runs up to today or yesterday |
| Back after a long break | A session 60 days or more after the game's previous one (carried last play included) |
| Completed | The day the card's status was set to Completed (the log's status record); games marked before the log are not listed |
| New to you | Played for the first time ever in the period (nothing earlier, nothing carried) |
| Genres, "Leaned into" | Curator's family, sub-genre and mechanic tags of the games played, weighted by time; particular tags before families. Merge versions on: two versions of one work are one game, shown as the version played most |
| Consoles | From the sessions as they were played: with Merge versions a game on two consoles is one game, but each console keeps its own time, sessions and games (the review's console card too) |
| Back and forth | Left (L1 on the review) goes back as far as the period the log began in; Right never into the future |

## Counting off the UI thread

`App::count` gathers what a count needs on the UI thread (two `Arc` clones: the log is read on the thread, the
Curator index is shared; three flags) and spawns a thread (`history`) that waits for the writer's copy of the log if
it isn't there yet (at most two seconds, on that thread), runs `summarise` or `review` (plus "You might like next"
from Echoes) and sends `UserEvent::Curator(Event::History(_))`. The page shows the period asked for in its title at
once; until its numbers come it keeps the last period's (or says "Counting…" when it has none), and A waits. A
result for a period the page has moved away from is dropped (`HistoryPage::set`). Five years of eight sessions a
day (14,600 sessions) take under 40 ms for three periods and a review on the development machine (the test
`five_years_of_fake_history_summarise_quickly`); a normal year's log takes well under a millisecond.

The History row's side text ("3 h 10 min this week", "Nothing this week" for a quiet week: never blank, Split
shows it large in its panel) is `played_since` worded by `week_side`, which looks only at this week's sessions from
the newest back.

## Read API (web companion)

`pocket_ui::play_history::stats_json(data_dir: &Path) -> Option<serde_json::Value>` has the signature of the web
companion's `HistorySource` and is meant to be registered once at start:
`web::set_history_source(pocket_ui::play_history::stats_json)`; the companion puts its value under `"history"` in
`GET /api/stats`.

- It uses the writer's copy of the log when the data directory is this process's (no file read), else reads the
  file; the result is cached for that data directory, that log's contents (a hash of every record, well under a
  millisecond) and that day and UTC offset: any other log, change, day or zone is counted anew.
- None when there is no log (or an empty one), when Play history is switched off for that data directory, or when
  the log can't be counted. It never panics (the log's indices are checked first).
- No genres and no merged versions (they need Curator's index): every file is a game.
- Dates are local days as `YYYY-MM-DD`, times are seconds, `start` is unix seconds.

```json
{
  "version": 1,
  "today": "2026-10-23",
  "counting_since": "2026-03-02",
  "damaged": false,
  "all_time": { "secs": 27000, "sessions": 6, "untimed_sessions": 1, "games": 2, "carried_secs": 7200 },
  "week": {
    "label": "This week", "first": "2026-10-19", "last": "2026-10-25",
    "secs": 4200, "sessions": 3, "untimed_sessions": 1, "days_played": 2,
    "games": [ { "name": "Burnout 3", "system": "ps2", "path": "/storage/roms/ps2/Burnout 3.iso",
                 "secs": 3600, "sessions": 1, "untimed_sessions": 0, "days": 1, "new": false } ],
    "systems": [ { "system": "ps2", "secs": 3600, "sessions": 1, "untimed_sessions": 0, "games": 1 } ],
    "bars": [ { "first": "2026-10-19", "days": 1, "secs": 3600, "untimed_sessions": 0 } ],
    "longest_streak": { "days": 2, "from": "2026-10-21", "to": "2026-10-22" },
    "current_streak": 2,
    "came_back": [ { "name": "…", "system": "…", "path": "…", "gap_days": 225, "date": "2026-09-14" } ],
    "completed": [ { "name": "Tetris", "system": "gb", "path": "…", "date": "2026-10-22" } ],
    "new_games": 0,
    "not_recorded": false
  },
  "month": { "…": "the same fields" },
  "year": { "…": "the same fields; one bar per week" },
  "recent": [ { "name": "Tetris", "system": "gb", "path": "…", "start": 1792648800, "date": "2026-10-22",
                "secs": 600, "ended": true, "emulator": "pocket-core/gambatte" } ]
}
```

`games` lists the ten most played of the period (`play_history::TOP`), `systems` ten, `recent` the last twenty
sessions, newest first (`date` is null for a session from before the clock was set). `not_recorded`: the period is
empty because it ended before the log began. `damaged`: part of the log could not be read. `untimed_sessions`
(added 2026-10-23, `version` stays 1: only new fields): of `sessions`, those whose end was never recorded (pocket-ui
or the Pocket stopped while the game ran); their time is unknown and in no `secs`. A figure with `secs` 0 and
`untimed_sessions` > 0 is "time not recorded", not zero; the pages say "1 session, time not recorded" or
"1 h 05 min, 1 session without time". In `recent` such a session has `"ended": false` and `"secs": 0`. The test
`the_read_api_for_the_web_companion` pins the shape.

The web companion registers it in `main()` and shows it on its Stats page ([WEB-COMPANION.md](WEB-COMPANION.md#play-history)).
It strips every `path` before the answer leaves the Pocket (a game in the library gets its hashed id instead), so a
new field that carries a path must be called `path` or start with `/`; anything else would reach the browser as it
is.

Inside pocket-ui the same numbers are `play_history::summarise`, `review` and `all_time`, all `serde::Serialize`.

## Snapshots

With Curator's made-up library (`scripts/curator-fake-library.py`, see [CURATOR.md](CURATOR.md#snapshots)):

| State | Screen |
|---|---|
| `curator:history[:week\|month\|year[:<keys>]]` | History over made-up history ending today (`play_history::demo`), after the keys (u d l r, L R for L1 R1, a y b) |
| `curator:history-empty[:week\|month\|year]` | History with nothing recorded yet |
| `curator:history-cutoff[:…]`, `curator:history-mixed[:…]` | A session today whose end was never recorded, alone or after made-up history (as after a crash) |
| `curator:history-hub[:cutoff\|:quiet[:<look>]]`, `curator:history-home` | The Curator list on its History row (`cutoff`: on the row after it, with only a session without an end, so History's side text shows; `quiet`: nothing played this week, "Nothing this week", in a look such as `split`); Home with the Lens off and its History row |
| `curator:review[:year\|month[:<card>]]` | The Year in review (on 10 December) or last month's, at that card |

`curator:history:year:llL` is January of the year the made-up log began in, before its first day ("Not recorded"). A snapshot counts on the UI
thread (it paints one frame); the app never does.

## Tests

`cargo test --release play_log play_history history_page`: the file survives a restart, a torn last frame, a
damaged frame, a lost name, a file that is no log, a file replaced behind the writer (also by one of the same
length, in place with its time kept or renamed over it), a full card, a write that failed half way, the flush at
exit and compaction; a file of the first version still reads; two sessions with the same start stay two; sleep is
left out and each stretch counts on its own day; `POCKETUI_NO_HISTORY` records nothing; the migration happens once,
counts nothing twice and is finished after a power cut (`seed_of` makes the seed off the UI thread, app tests);
merged versions leave each console its own time; a tap on a tab two steps away opens it; periods, midnight splits, streaks, comebacks, completions,
genres and merged versions, a clock that was unset or ahead; every state of the pages paints something (an empty
log, a period before the log, a damaged file, a game without a name), Left and Right stop where they should; the
read API's shape, its None cases and its cache.

## On the device

What host tests can't show, for the main session:

- Play a game for a few minutes, quit: History (Curator list) shows it on today's bar; the Home row's side text
  counts it. Power the device off while a game runs (hold power): after the restart the session is there without
  time.
- The first start after the update on a device with play time: the log says `play log: taking over the play time
  of N games from before`; History's first period says "Counted since …"; earlier periods say "Not recorded".
- Play a game for a few minutes, let the screen go dark (light sleep) for a minute, wake it, play on, quit: History
  counts the minutes played, not the dark minute. Sleep it with the power key overnight (or across midnight), play
  on in the morning and quit: the time before the sleep is on the evening's bar, the rest on the morning's.
- `scripts/latency.sh` runs leave History unchanged (`POCKETUI_NO_HISTORY`, the line in pocket-ui's log).
- `scripts/latency.sh --compare`: game start, quit and the shelf unchanged (the start's write waits three seconds,
  the end's goes with the shelf's own writes).
- Open History while the device is busy (right after start): "Counting…" at most a moment; the trace line
  `history: … in N ms` in the log.
