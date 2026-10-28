# Storage

Settings → System → **Storage**. One list of what is on the SD card, each with its size. The sizes are added up in the
background each time the page opens: "Counting…" for a few seconds the first time; after that the numbers of the last
visit show at once, with a quiet "Updating…" next to the title until the new ones are in, so files you copied in over file
sharing are counted too. After a game or a delete they are counted again. Under the list, the selected row says what it holds and what A does there.

| Row | What it is | A |
|---|---|---|
| **Free on the SD card** | Free space and the SD card's size ("41.2 GB of 63.9 GB") | nothing |
| **Games** | Every game file, all systems together | the systems, largest first |
| **Save states** | States of every game, the ones you saved and the automatic ones | deletes old automatic states (below) |
| **Earlier states** | What the pause menu's [Earlier](pause-menu.md#earlier) keeps by itself, part of Save states. Under the list: the most they may take (1 GB in all, 100 MB a game; the oldest go first) | nothing: they go with Save states' clean-up |
| **Saves** | The games' own saves and memory cards | nothing: saves are never deleted here |
| **Screenshots** | The pictures HOME + A took | nothing: delete them in [Settings → Games → Screenshots](playing.md#looking-at-your-screenshots), or from your computer |
| **Box art and pictures** | The box art, screenshots and title screens pocket-ui downloaded | clears them |
| **Logs** | pocket-ui's logs | deletes the logs of earlier starts |
| **Backups** | The backup folder | deletes pocket-ui's older backups |
| **Everything else** | BIOS files, music, the data of ports, anything else | nothing |
| **Hidden games** | The games you hid from the shelf | the list; A on one shows it again |
| **SD card check** | What the system says about the SD card's file system: "No errors seen", "Errors seen", "Read-only" or "Can't tell" | nothing: it only reports ([below](#sd-card-check)) |

A on a hidden game shows it again: it is back on the shelf, in the search and in the counts of Settings → Games (Add
games, From EmulationStation) at once, without leaving Settings first.

Everything that deletes asks twice: the first A says exactly what goes and how much space it frees, in red; the second
A does it, B keeps it. Moving to another row cancels the question. What each one deletes:

- **Save states**: the *automatic* states of games you have not played for 6 months (left/right: 3, 6 or 12):
  the moment Quit kept for "Continue", the state Load kept as "Before load", and the pause menu's
  [Earlier](pause-menu.md#earlier) states. A game played within the months keeps
  them, even when its state files are older. The slots you saved into yourself and your saves are never touched. Such a
  game starts from its last slot or its save next time, not where you quit.
- **Box art and pictures**: the whole picture cache, including pictures you put there yourself. They are downloaded
  again when the Pocket is online and no game runs ([Extras](extras.md)).
- **Logs**: the logs of earlier starts. The ones of this start stay (pocket-ui keeps those of three starts anyway).
- **Backups**: every backup of pocket-ui's settings but the newest ([Backup & reset](settings-system.md#backup--reset)). Other files in
  the backup folder stay.

To free more space, delete games you don't play any more: open a game's card and choose **Hide or delete** at its end
([Hide or delete a game](library.md#hide-or-delete-a-game)).

## When the SD card is nearly full

With less than 1 GB free, pocket-ui says so once on the shelf ("The SD card is nearly
full: 812 MB left …"), at a start or after a game, and the Free row of this page says it in red. It says it again only
after the SD card has had space again (1.5 GB). 1 GB, because what fills an SD card does not grow with its size: a PS2 state
is 20 to 40 MB and a GameCube or Wii state 40 to 100 MB, and a session keeps one when you quit and one more when you
load, so a session of a big game can take 200 MB. Below 1 GB free, no new [Earlier](pause-menu.md#earlier) state is
written either; they never take more than 1 GB in all. The notice is the warning before the SD card is full; it does not
watch the saves themselves.

## SD card check

The last row. Flat batteries, sleep that never woke and power cuts can leave the SD card's file system damaged, and the
first sign is often games or saves that are gone. The page looks, each time it opens, at three things the system
already knows, in the background: whether the SD card is mounted **read-only**, the **error count** the file system keeps
for itself, and the system's log for file system and SD card read errors. It repairs nothing and changes nothing.

- **No errors seen**: the SD card is writable and the system logged no errors on it. This is **not a test of the SD card**:
  an SD card can be failing without having said so yet. Keep a copy of the saves you care about anyway.
- **Errors seen** (red): the system logged errors on the SD card, now or at an earlier start (the count stays until the
  SD card is checked on a computer). It may still work. Copy your saves and states to a computer over file sharing
  (Settings → Games → Add games shows how), then check the SD card on the computer; the file system is ext4, which Windows and macOS don't
  read without extra software, so a Linux computer is the easy way.
- **Read-only** (red): after a serious error the system stopped writing to the SD card, so saves and states can't be
  written. Restart the Pocket once; if it stays that way, do the same as above.
- **Can't tell**: the Pocket could not read these signs (a computer or the virtual device); nothing is reported, which
  is not the same as fine.
- "…" for a moment while it is read.
