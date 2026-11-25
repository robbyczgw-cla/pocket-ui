# Storage

Settings → System → **Storage** gives you a single breakdown of everything on your SD card and how much room it takes.
Sizes calculate in the background whenever you open the page ("Counting…" on your first visit; after that, cached numbers
appear immediately with a quiet "Updating…" beside the title while fresh totals load, catching anything copied over File
sharing). Totals refresh again after you play or delete a game. The footer explains what each highlighted row holds and
what pressing A does.

| Row | What it is | A |
|---|---|---|
| **Free on the SD card** | Available space and total capacity ("41.2 GB of 63.9 GB") | nothing |
| **Games** | All game files across every console | opens per-console breakdown, largest first |
| **Save states** | Manual and automatic save states across all games | clears old automatic states (details below) |
| **Earlier states** | Automatic snapshots kept by [Earlier](pause-menu-saves.md#earlier) (part of Save states; capped at 1 GB total and 100 MB per game, oldest dropped first) | nothing (cleaned up with Save states) |
| **Saves** | In-game saves and virtual memory cards | nothing (never deleted here) |
| **Screenshots** | Screenshots taken with HOME + A | nothing (delete in [Settings → Games → Screenshots](playing.md#looking-at-your-screenshots) or from your PC) |
| **Box art and pictures** | Downloaded box art, screenshots and title screens | clears the cache |
| **Logs** | pocket-ui log files | clears logs from earlier boots |
| **Backups** | Contents of the backup folder | clears older pocket-ui settings backups |
| **Everything else** | BIOS files, music, port data and other files | nothing |
| **Hidden games** | Games you hid from the shelf | opens the list; press A on a game to restore it |
| **SD card check** | File system health reported by the OS: "No errors seen", "Errors seen", "Read-only" or "Can't tell" | nothing (read-only status, [details below](#sd-card-check)) |

Unhiding a game restores it immediately to your shelf, search and Settings → Games counts (Add games, From EmulationStation)
without leaving Settings.

Every cleanup asks twice: the first A press shows in red exactly what will be removed and how much space you'll reclaim;
the second A confirms, while B or moving to another row cancels. Here's what each cleanup removes:

- **Save states:** removes *automatic* states for games you haven't played in 6 months (adjust to 3, 6 or 12 months with
  Left/Right). That covers Continue auto-states, "Before load" backups and [Earlier](pause-menu-saves.md#earlier)
  snapshots. Games played within that window keep everything. Your manual save slots and in-game saves are never touched.
  Cleaned-up games simply resume from their last manual slot or in-game save next time.
- **Box art and pictures:** clears the entire image cache, including custom images you placed there. Covers redownload
  automatically whenever your Pocket is online and idle ([Box art & downloads](extras.md)).
- **Logs:** removes logs from previous boots while keeping the current session (pocket-ui only retains the last three
  boots anyway).
- **Backups:** removes older pocket-ui settings backups while keeping the newest one
  ([Backup & reset](settings-system.md#backup--reset)). Your save backups and any other files in the backup folder stay
  untouched; delete those over File sharing if you want them gone.

Need more room? Delete games you're done with from their game card → **Delete from the card** (press SELECT to view card
rows; [Hide or delete a game](library.md#hide-or-delete-a-game)).

## When the SD card is nearly full

Once free space dips below 1 GB, pocket-ui alerts you once on the shelf ("The SD card is nearly full: 812 MB left …") at
boot or after quitting a game, and highlights the Free row in red. The warning stays quiet until you free up at least
1.5 GB again. Why 1 GB? Modern 3D save states are big: a single PS2 state runs 20 to 40 MB and GameCube/Wii states hit
40 to 100 MB, and a session keeps both an exit state and a "Before load" state (up to 200 MB). Below 1 GB free, pocket-ui
also pauses writing new [Earlier](pause-menu-saves.md#earlier) snapshots (which never exceed 1 GB total anyway). Think of
this as an early heads-up before space runs out; it doesn't monitor individual saves.

## SD card check

The bottom row checks three signals the OS already tracks (whether the card was forced **read-only**, the file system's
internal **error count**, and kernel logs for SD read/file-system errors), all read-only in the background whenever you
open Storage. It never modifies or repairs the card.

- **No errors seen:** the card is writable and the OS hasn't logged any errors. This is **not a full surface test**: SD
  cards can still fail unexpectedly, so back up saves you care about.
- **Errors seen** (red): the OS logged file-system errors on this boot or a previous one (the counter persists until you
  repair the card on a computer). Copy your saves and states off over File sharing right away
  (Settings → Games → Add games shows how), then check the card on a computer. Since it uses Linux ext4, a Linux PC is the easiest option without
  extra Windows/macOS drivers.
- **Read-only** (red): after a serious error, the OS locked the card read-only so saves and states can't be written.
  Reboot once; if it stays read-only, back up and check the card as above.
- **Can't tell:** the OS couldn't read those counters (common on a PC or virtual device). This means "unknown", not
  "healthy".
- Shows "…" briefly while checking.
