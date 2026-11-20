# Pocket-to-Pocket sync

pocket-ui's own sync between Pockets: battery saves first, then Continue states and library data. It runs over the home Wi-Fi or
Tailscale, pairs with a six-digit code, and needs no account, no cloud and no server. It is the same code on every
system (ROCKNIX first, then the Brick, dArkOS and Android). The Syncthing switch (docs/user/sync.md) stays for syncing
with a computer and gets no new features.

Status: **the core, the connection, the page and the automatic syncs** (`src/pocket_sync.rs`,
`src/pocket_sync/net.rs`, `src/pocket_sync_page.rs`, `src/app_pocket_sync.rs`; tests in `src/pocket_sync/tests.rs`,
`tests/sim.rs` and `net_tests.rs` below it). Settings → Connections → Pocket Sync switches the service on, pairs with a
code and syncs a paired Pocket on A. After every game this Pocket syncs with every paired Pocket (it gives what it
saved), and when its service starts or it wakes from sleep it does the same (it fetches what others saved); one line
on the shelf tells what came and, after a game, where it went, with the game's name (`pocket_sync_page::shelf_line`).
How each Pocket stands and the saves that came and went are kept in `<data>/pocket-sync/log.json`
(`src/pocket_sync/log.rs`): `sync_with` writes each sync's end there, the service each save another Pocket sent. The
page shows a tick or cross and "Synced · 3 min ago" / "Not reachable since 14:05" per Pocket (`pocket_sync_page::status`),
A opens a Pocket's page with its last saves (game, direction, when), the game card has a quiet line ("Pocket Sync · With TrimUI
Brick at 14:05"), and a problem (pair again, a wrong answer, saves that did not go) is a shelf line, never quiet. A
lost log is an empty one: it decides nothing. Still to come: giving
before deep sleep, the hint on the case when another Pocket may have a newer save, the questions with device and
picture, Continue states and library data (plan below). `examples/pocket_sync_dev.rs` runs the service and a sync by
hand on folders, for tests between devices: 18.11.2026 between an RG35XX Plus and an instance on a computer over the
home Wi-Fi (pairing, both directions, a question asked once, a choice, nothing written while a game runs).

## Why not Syncthing

Syncthing exists only on ROCKNIX (the Brick, dArkOS and Android would need a 25 MB Go program or a second app),
pairing takes a 56-character device ID, folders and a web interface, and it runs all the time. A conflict is only
seen after the fact, and saves match only by the same path. Our own code is small, runs everywhere, matches games by
content, and only works when a game ends, when the Pocket wakes and on the Sync page.

## Any number of Pockets, no server: version vectors

Every save carries a version vector (`Vv`): per device, how many changes of that device the version contains.

- A device that sees its own save changed (a new SHA-256 since its record) adds one to its own count (`bookkeep`).
- A save that arrives brings its vector (`received`).
- Between two Pockets (`decide`): same bytes → nothing (the vectors are merged); one vector contains the other →
  that version has all the other's play and more, it goes over; neither → played independently: a conflict.

An old save can never walk back: its vector is contained in the newer one's on every device the newer one reaches.
A question is asked only for really independent play. No clock decides anything (the RG35XX Plus has no working
clock); times are only shown.

**One question per save at a time.** The device that finds a conflict puts the other version beside the save as
`<name>.sync-conflict-<date>-<time>-<peer>.<ext>` (`conflict_name`: Syncthing's form, so `sync_conflicts` and the
"Two versions" page already show it) and records the question (`Rec::asking`: the union of the versions it shows
besides its own). The other version gets a mark (`Rec::marks`: the question's whole union); marks travel with their
version and spread to every Pocket that has the same bytes (`same`). A mark lasts only while its version is inside
the question: playing on, or the answer arriving, ends it.

- Meeting a version that is in the same open question (shown by the asking Pocket, or both marked by it): wait
  (`Action::Waiting`). A marked version meeting one that question doesn't show is asked about, not waited for: a
  question whose Pocket lost its record would otherwise wait for ever (found by the random run, 19.11.2026).
- Meeting the asking Pocket with new independent play: that play joins its question (`Action::Offer`), or, met from
  the asking side, the question grows (`Action::Conflict` with a question already open).
- The choice (`chose`, called when the page applies it) gets exactly the union of every version shown, nothing
  more, and replaces them everywhere. Two choices on two Pockets that agree (one kept what the other's choice
  contains) settle without a new question. A question whose versions arrive inside a newer save is answered
  elsewhere and closes by itself (`answered`: the copies go to the backup folder).
- Only two Pockets that each asked before hearing of the other's question can ask about the same saves twice. If
  the player answers both ways, the two saves have the same vector; one answer stands on every Pocket (the larger
  hash) without a third question, and the other save goes to the backup folder like every replaced save.

**What the caller must do** (the transport and the page, merges 2 to 5): look at the saves (`index`/`bookkeep`)
after every game and before a choice is applied, so play is counted before a choice can hide it; apply a choice with
`chose`; record arrivals with `received`, equal saves with `same`, a question found here with `conflict_found` and
`mark` (on the peer), an offer with `offered` and `mark`; move the copies to the backup folder when `answered`;
never put a second copy of bytes that are beside the save already.

`pocket_sync/tests/sim.rs` checks all of this with four (and two, three) simulated Pockets in thousands of random
runs of play, syncs between any two and choices, using exactly these functions: a save is only replaced by one that
contains all its play or by the player's choice; no question between versions where one contains the other (except
two contradicting answers); after the last play, every Pocket ends with the same save and nothing open; and asking
about the same two saves again stays rare (17.11.2026: 163 of 47,559 questions in 3000 four-Pocket runs, far more
parallel play than a player does). Each rule above came from a run that broke the previous version of the rules.

## Matching saves

A save belongs to the game file beside it (same name, another ending; `.png`, `.txt` and the like are not games).
Saves pair across devices (`pair`, each save once, never two whose game files have known, different CRCs):

1. the same path (`gba/RPG/Game.srm` with `gba/RPG/Game.srm`);
2. the CRC32 of the game file (up to 64 MB; disc images have none), where it is one save on each side;
3. system and file name without the folder, case-insensitive, where that is one save on each side.

What stays ambiguous stays unpaired, and an unpaired save only ever goes to a free place (Sol review 19.11.2026:
`gba/A/Game.srm` was paired with `gba/B/Game.srm` although `gba/B/Game.srm` was there too). A save new on this device
lands under this device's own name for the game (`target`, the system folder and one subfolder, like the scan; none
when two games fit), and never on a file the card has in other letters (`collides`: one file on FAT and exFAT). The
save and the cartridge clock (`.srm`, `.rtc`) are separate saves of one game. The endings are those of the Syncthing
saves folder (`SAVE_EXTS`, a test keeps them equal).

On ROCKNIX the saves are in `/storage/roms/<system>/` and one subfolder level below (`scan`), without `savestates`,
`sync-backups`, `backup`, `bios` and Azahar's data. The CRCs are cached by path, size and time (`CrcCache`).

## The record

`<data>/pocket-sync/versions.json`: per save path, the hash, the vector, an open question, the marks, and whether
copies no record knows are beside it; plus the id this device's changes are counted under (`Versions`). Written whole
(`fsutil::write_atomic`) and its copy `versions.json.bak` with it.

A record must never be guessed (Sol review 19.11.2026 #3: a lost record started this device's count at 1 again, so
a peer's older save with a higher count looked newer and would have replaced the newer one here):

- damaged or missing, with a whole copy: the copy is used;
- not given by the card (a read error other than "not there") or written by a newer pocket-ui: no sync this time;
- damaged or missing and no whole copy (or a first start): a **new history** under a new id (the device's id and a
  few random letters, `Versions::open`). Every save here then looks independent of what the peers know: different
  bytes become a question, never a winner; equal bytes simply match. Copies left beside a save by a question the
  lost record no longer shows are marked (`Rec::orphan`), and a choice over them counts as news (`chose`).

The random run loses records now and then; nothing is lost and everything settles. Questions about a save made after
a lost record are allowed and counted.

## Where the saves are (`Layout`, `SavesAt`)

The path in the index is always `<system>/<sub>/<stem>.<ext>`, as if the save were beside its game; only `Layout::file`
(and `dir_of`, `rel_of`) knows the file. `scan`, `target`, `collides` and every file the service reads or writes go
through it.

- `SavesAt::BesideGames` (ROCKNIX): `<roms>/<system>/<sub>/<stem>.<ext>`, backups in `<roms>/sync-backups`.
- `SavesAt::Root(saves)` (for the Brick's and dArkOS's cores): `<saves>/<system>/<sub>/<stem>.<ext>`, the game for the CRC
  at the same place in `<roms>`, backups in `<saves>/sync-backups`.
- `SavesAt::Android` (pocket-ui for Android, Handheld view, since 0.6.0): the games in every ROMs folder the view reads
  (`android_handheld::roms_roots`, internal storage first, then the cards, then the folders taken at the first start),
  the saves where the player keeps them, `<files>/saves/<key>/<stem>.<ext>` with the player's key of the game
  (`android_handheld::game_key`, Gambatte folded in for Game Boy as Settings says). A save there whose game is in no
  ROMs folder is not in the index. A peer's save for a game this device lacks has no place: it is skipped (`Report::no_place`,
  `PUT` answers 404), not failed, and comes once the game does. Backups and answered copies go to `<files>/sync-backups`;
  the "Two versions" page looks through `<files>/saves` (`sync_conflicts::android`, `owner_by_key`).

On Android Pocket Sync runs only while the Handheld view is in front: the service stops when the view goes to the
background (Home, a game, the screen off) and starts again a second after it is back, then fetches; after a game the
sync with every Pocket starts at once (the client needs no service of its own). No background service, no new
permission (INTERNET was there). The Mobile view's games are documents of Android's folder picker, not files: it has no
Pocket Sync.

## The connection (`pocket_sync::net`)

- **Pairing**: the Pocket that shows the code (`Pairing`) accepts one Pocket per code, five tries, five minutes, a
  second's wait after a wrong try (the web companion's rules). Both keep a random 32-byte key in
  `<data>/pocket-sync/peers.json` (root only). This Pocket's id is made once (`device.json`). The service looks at
  the card again for a Pocket it doesn't know, so a pairing this Pocket made by asking another one counts at once;
  forgetting a Pocket is written to the card (`Service::forget`).
- **Signed, not encrypted**: every request and every answer carries an HMAC-SHA256 under the pair's key (method,
  path, body hash; the answer over the request's signature, status and body hash). No clock is checked; a replayed
  write only lands on the exact version it was made for. Connections only from private, link-local, loopback and
  Tailscale addresses.
- **The service** answers `GET /index`, `GET /file`, `PUT /file`, `POST /put-many`, `POST /get-many` (bundles of up
  to 64 saves, about 8 MB), `POST /offer`, `POST /mark`, `POST /same`. While a game runs (the caller's `busy`) it
  writes nothing (423) and the index says so.
- **A sync** (`sync_with`) plans with the core and applies each step with the core's record functions, under one
  lock with the service: pushes and pulls in bundles, questions, offers and equal saves one by one. A replaced save
  goes to `<roms>/sync-backups/<date>/` first; every write is only over the version planned with.
- **Finding Pockets**: one UDP broadcast on port 47812 asks who is there (`discover`, `answer_discovery`); a Pocket
  showing a code says so. No broadcast crosses Tailscale, so the pairing view also asks every online device of the
  tailnet by itself (`discover_at`, the addresses from `tailscale status --json`, `tailscale::online_peers`), the
  broadcast first. Android's Tailscale app tells apps nothing about the tailnet: an Android Pocket shows its code and the
  other Pocket finds it. Paired Pockets are tried where they answered last.

Measured 18.11.2026 (RG35XX Plus ↔ computer, Wi-Fi): a first sync of 4 saves 0.8 s, one changed save 0.3–0.4 s,
50 new saves 1.9 s in bundles (10.6 s one by one before), nothing to do with 54 saves 0.16 s; the waiting service
uses 2.6 MB.

## Plan (one merge each)

1. **Core** (this): vectors, matching, the record, the ROCKNIX scan, the random four-Pocket test.
2. **Transport**: a service thread in pocket-ui reusing the web companion's HTTP parts, HMAC-SHA256 on every request
   and answer, pairing with a six-digit code (the web companion's rules), finding Pockets (saved address, a UDP call
   on the Wi-Fi, Tailscale), several files per request, nothing written while a game runs, writes via temporary file
   and with the old save in a dated backup first.
3. **Sync page and pairing** (Settings → Connections → Sync), Syncthing under "Sync with a computer".
4. **Automatic**: give after a game, fetch on wake and start, give before deep sleep; the one-line notices.
5. **Two saves**: the existing page with device, moment picture, play time and, where readable, progress.
6. **Continue states and library data**: states only between the same core and version; library data in its
   existing per-device format over the new transport.
7. **Real test** on two ROCKNIX Pockets, then the Brick as the third.
