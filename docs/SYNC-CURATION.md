# Library data: syncing the curation, not just the saves

Settings → Connections → Sync → **Library data** (off at the start) makes the player's curation the same on every
Pocket they own: favourites, play status, notes, ratings, stickers, hidden games, smart shelves and play history.
The user's side is in [user/sync.md](user/sync.md#library-data). This page is the design: why the files look the way
they do, how two devices that both changed something end up with both changes, and what is deliberately left out.

Code: `src/library_sync.rs` (the files, the merge, applying; pure apart from reading and writing the folder, tests in
`src/library_sync/tests.rs`), `src/app_library_sync.rs` (the thread, applying on the UI thread), `src/sync.rs`
(`Share::Library`, the Syncthing folder), `src/play_log.rs` (`Extra`, `set_extra`: other devices' sessions in the
pages).

## The problem

Syncthing copies whole files. If two Pockets shared `library.json` itself, a status set on one and a note written on
the other before they met would end as one file winning and the other device's change gone (or as a
`.sync-conflict-` copy nobody merges). Backup & reset already knows the right file set, but a backup is a snapshot,
not a merge.

## The layout: one folder per device

The Syncthing folder "Pocket library data" (id `pocket-library`) is `/storage/.config/pocket-ui/library-sync`:

```
library-sync/
  .stignore                 *.tmp and *.bak stay out (our temporary files, the last good copies)
  P7T4NX2-24CSKCT/          one folder per device: the first two groups of its Syncthing device ID
    library.json            this device's whole knowledge of the curation, every value with its time
    history.json            every play session and status change this device knows
  MFZWI3D-BONSGYC/
    library.json
    history.json
```

**A device writes only its own folder** and reads all of them. No file is ever written by two devices, so Syncthing
never has to choose between two versions of one, and nothing is lost to a whole-file copy. The folder name comes from
the Syncthing device ID: a cloned card cannot pair with itself, and a re-flashed card (a new ID) gets a new folder
while the old device's folder stays and is read like any other.

## `library.json`: values with times

```json
{
  "version": 1,
  "device": "P7T4NX2-24CSKCT",
  "games": {
    "gb/Tetris (World) (Rev 1).gb": {
      "info": { "name": "Tetris", "system": "gb", "path": "/storage/roms/gb/Tetris (World) (Rev 1).gb" },
      "f": {
        "favorite":            { "v": true,        "t": 1792929000000, "d": "P7T4NX2-24CSKCT" },
        "status":              { "v": "completed", "t": 1792931000000, "d": "MFZWI3D-BONSGYC" },
        "note":                { "v": null,        "t": 1792932000000, "d": "P7T4NX2-24CSKCT" },
        "sticker:couch co-op": { "v": true,        "t": 1792930000000, "d": "P7T4NX2-24CSKCT" }
      }
    }
  },
  "stickers": { "couch co-op": { "info": { "name": "Couch co-op" }, "f": { "name": { "v": "Couch co-op", … }, "pinned": { … } } } },
  "shelves":  { "now playing": { "info": { "name": "Now playing" }, "f": { "lens": { … }, "pinned": { … }, "order": { … } } } }
}
```

- **Keys, not paths.** A game is keyed by its system folder and file name (`library_sync::game_key`): `gb/Tetris
  (World) (Rev 1).gb`, also for a game in a subfolder (`psx/FF7 (Disc 1).chd`). Paths are per device; the key is the
  same on every Pocket. On applying, the key is found again on this device: the path it already uses for the game,
  else a game of the game list with that key, else `/storage/roms/<key>`, else the other device's path. Two files with
  one key on one device (the same file name in two subfolders of a system) are left out of sync on that device rather
  than mixed up.
- **Fields.** Per game `favorite`, `status`, `note`, `rating`, `hidden` and one `sticker:<name>` per sticker on it.
  Stickers and smart shelves are keyed by their name in lower case (both are unique regardless of case); sticker ids
  are per device and never leave it. A sticker renamed is the old name removed and the new one made.
- **Every value has a time and a device** (`t`: Unix milliseconds, `d`: the device folder). `null` is "removed" or
  "back to the default" (unstarred, note cleared, not rated, shown again, shelf deleted): a **tombstone** that beats an
  older value and loses to a newer one.
- **A device's file holds everything it knows**, its own values and those it took from others (a state-based merge).
  So a change also reaches a third device through the second, and a device that is gone (sold, re-flashed) loses
  nothing that another one already had.

## A pass

`library_sync::pass`, on the `library-sync` thread:

1. Read this device's files as they are (`library.json`, `favorites.json`, `curator.json`) and turn them into fields.
2. Read the own folder's `library.json`, the state written after the last pass. **Every field whose value differs is a
   change made here since**: it gets the value, the time its file was last written (its mtime, never later than now)
   and this device's folder. That is how edits are found without touching every place in pocket-ui that edits: the
   card, the web companion, Curator, Hide or delete all keep writing the files they always wrote.
3. Read every other device's `library.json` and take, **field by field, the newest value** (last writer wins; on the
   same millisecond the larger device folder, then the larger value, so every device picks the same).
4. What differs from this device's files are the **changes** to apply here.

The UI thread then applies the changes (`library_sync::apply`) like any other edit and saves the files once. A change
whose value on this device changed since the pass read it (the player edited meanwhile) is **skipped**: the next pass
finds that edit as a change of this device, newer, and sends it on. Only once the UI has applied the result does the
thread write the own folder (`library_sync::store`). A pass that was never applied (pocket-ui stopped in between) has
written nothing and is simply done again.

A file that is unchanged is not written again, so Syncthing has nothing to send while nothing changes.

### Examples (all in `src/library_sync/tests.rs`)

| Both devices before they met | Result on both |
|---|---|
| A sets Zelda to Playing, B completes Tetris and stars it | Zelda Playing, Tetris Completed and starred |
| A writes a note on Zelda, B rates Zelda 4 | Note and rating |
| A pauses Zelda at 10:00, B completes it at 11:00 | Completed, whichever device syncs first |
| A unstars Zelda at 12:00, B wrote a note at 11:00 | Unstarred, note kept |
| A clears the note at 13:00, B changed it at 12:30 | Note cleared (tombstone newer) |
| B stars Zelda again at 14:00 | Starred (the 12:00 tombstone is older) |
| A renames the sticker "Couch co-op" to "Two players" | The old sticker is gone on B, the new one is on the same games |
| A deletes a sticker, B had put it on another game later | The sticker stays on that game on both |
| A re-flashed card with nothing on it | It takes everything, with the other device's times; it sends nothing back |

## Play history: a union

`history.json` holds sessions (`g` game key, `s` start, `tz`, `secs`, `e` ended, `em` emulator, `p` the stretches
between sleeps) and status changes (`g`, `at`, `tz`, `st`), plus each game's name and system for a game that is not on
this device. A device writes every session it knows: its own play log's, its own earlier file's and the other
devices'. They are **de-duplicated by game and start** (of two copies the one with its end, then the longer one, is
kept); status changes by game, time and status.

The other devices' sessions are **never written into this device's play log** (`play-log.bin` stays this device's
record). They are handed to `play_log::set_extra`, and the log the pages read (`play_log::view`, the web companion's
`log_for`) is the own log with them added (`play_log::combine`): Curator → History, the Year in review and the week
line on Home count every Pocket. The game card's time played and its suggestion still count this
Pocket only (they come from the recent games list).

With Settings → Curator → Play history off, a device reads and writes no history and shows none of the others'.

## When it runs

One thread, `library-sync` (low priority, started after the first frame):

- 15 s after the start, then whenever this device's files or any file of the folder changed (a `stat` of each, once a
  minute; a pass only when one changed, which is also how a change Syncthing brought in is noticed);
- when Settings → Sync opens or a row there is switched;
- after every game, at once and 30 s later (Syncthing has had time to bring what the other devices changed meanwhile);
- **never while a game runs**: a result that arrives while one runs waits and is applied when it ends.

The pass itself (reading and comparing the files) runs on that thread; the UI thread only applies the changes
(nothing at all when there are none) and refreshes what shows them (Curator's index, the lists, the shelves; the game
list is read again on a thread when a game was hidden or shown).

## When something is wrong

- **A file of another device that is missing, cut off, damaged, too big (64 MB) or not JSON of ours** is skipped and
  said once in the log (`library sync: skipped …`). The local data stays exactly as it was; nothing is shown on screen.
- **A newer pocket-ui's values**: a field this version doesn't know (or a value it can't hold, such as a new status) is
  never applied, never overwritten and carried on in this device's file, so a third device of the newer version
  still gets it.
- **The own file damaged**: read from its last good copy (`.bak`, `fsutil::read_json_or_backup`), else as empty, in
  which case this device's current values are taken as its edits at the time their files were written.
- **No Syncthing device ID yet** (Syncthing never started): no pass, tried again later.
- **Switched off**: nothing in the folder is read or written, and the other devices' sessions leave the pages. What
  was merged into this device's files stays. Edits made while off are found at the next pass after switching on,
  with the times their files were written.

## Known limits

- **Clocks.** Last writer wins by the devices' clocks. A device whose clock is far off (after a flat battery, before
  it synced the time) can win or lose wrongly. Both Pockets set their clocks from the network.
- **Time of an edit = time its file was written.** Two edits of the same field on two devices less than a pass apart,
  with another edit of the same file on one of them in between, can be ordered by the later write. In practice edits
  of one game's field on two devices minutes apart are rare.
- **A game kept in a different subfolder on each device** is found by its key; a game added later at another place
  than where the other device's notes were applied keeps them at the first place.
- Not synced: the play counters in `library.json` (`sessions`, `secs`: per device, the history has every session), the
  recent games list, "Wrong game? Choose another", Curator's preferred versions, per-game Options and cheats, and
  the order of stickers and shelves (new ones are added at the end).
- Tombstones are kept for ever. They are small (a key, a field, a time).
