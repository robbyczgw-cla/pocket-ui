# Saving in the pause menu

The [pause menu](pause-menu.md) (HOME) is where you save, load and go back in time. This page has the details: save slots,
Where I quit and Before load, Earlier, where the state files are, saving on the other emulators, and what happens
when the SD card can't take a save. The menu itself, per emulator, is on [The pause menu](pause-menu.md).

## When your progress can't be saved

On pocket-ui's own host the game's own save (the in-game save, `<game>.srm`) and, for games with a clock, the clock
(`<game>.rtc`, see [below](#games-with-a-clock)) are written to the SD card every 10 seconds, and again when you quit or
the Pocket goes to sleep. When the SD card is full, has turned read-only or is failing, that can't go through:

- A line stays at the bottom of the game: "Couldn't save: the SD card is full. Free some space." (or "Couldn't save to
  the SD card. Trying again." when it can't be written to). You can keep playing: the save is tried again every 10
  seconds, and the line goes away as soon as one gets through.
- The save already on the SD card stays as it was. A write that fails never damages it.
- The pause menu shows the same line under the game's name.
- **Quit** asks first: "Couldn't save. Quit anyway?", and says what is at stake. **Keep playing** (or A on the first
  row) goes back to the game; B goes back to the menu; **Quit anyway** quits, and tries the save once more on the way
  out. What you did since the last save that reached the SD card is lost then. Free some space first if you can
  ("The SD card is full" in [Troubleshooting](troubleshooting.md#questions)).
- Sleep, the power-off at a nearly empty battery and switching games from Recent games don't ask: they try the save once and go on, so
  the Pocket never hangs on a full SD card. If the last save failed, the shelf says so once after the game.
- When Quit or sleep could still save the moment (Where I quit) although the in-game save failed, the next start continues from
  that moment, and the in-game save in it goes to the SD card as soon as there is space.
- When the moment itself doesn't fit any more (a big one, a few MB, on a nearly full card), the same line shows and the shelf
  says so after the game: the next start goes back to the moment before.
- A moment set aside because another emulator made it is kept even if the card has no room to put it in a slot.
  A later successful Quit keeps it too. When a slot and enough space are available, the next start puts it under Load.


If the game's save is on the SD card but can't be read when the game starts (a failing card), the game starts without it and
the line says "Couldn't read this game's save. It stays as it was." Nothing is written over that save while you play; what you
do in that session isn't kept. Copy the save off the card ([Troubleshooting](troubleshooting.md#questions)).

### Games with a clock

Some cartridges have a clock: Pokémon Gold, Silver and Crystal, for example. On pocket-ui's own host that clock is kept
in `<game>.rtc` beside the in-game save, the file RetroArch uses too, so the time in the game goes on after you quit and
start again, even without the moment saved by Quit. [Sync](sync.md) shares it with the save. Game Boy Advance games
with a clock (Pokémon Ruby, Sapphire and Emerald) read the Pocket's own time instead; if theirs is off, check the time
zone in [Settings → System → Time](settings-system.md#system). Loading an old gpSP moment follows that time too.
**Start over** keeps the cartridge clock, just as resetting a console does.

## Save slots, Where I quit and Before load

Every game has **eight save slots** (the 3DS has one, see below) and two states of its own that never
touch them:

| | Written by | Where it is |
|---|---|---|
| **Slots 1 to 8** | **Save**, and only Save | Save and Load |
| **Where I quit** | Quit and sleep (not Quit with **Continue where I quit** off; that one leaves the older moment here) | Load, first in the list |
| **Before load** | every Load, just before it | Load, after Where I quit, until you quit |

With Save selected, press **Left/Right** to choose the slot (the number appears next to it, as
"‹ 3 ›"). With Load selected, **Left/Right** walks through what can be loaded: Where I quit, Before load
(only while there is one), then slots 1 to 8, and last **From the start** (pocket-ui's own host). Load starts on Where I quit
when there is one.

- **Where I quit** ("Where I quit · today 14:02" under the picture): the moment you left the game, which the next start
  continues from (not with Continue where I quit off) (see [continuing](playing.md#continuing-where-you-left-off)). It is not one of your slots.
  Sleeping writes it too.
- **Before load** keeps what you were playing just before a Load replaced it, so a Load you did
  not mean can be undone: Load, then Load again and pick "Before load". It lasts until you quit.
  A Quit after a Load saves the game as it is then (Where I quit); Before load is not kept after that.
- **From the start** (pocket-ui's own host) begins the game again and does nothing else. Its own saves stay, and so does the
  moment on the card until the next Save & quit replaces it (the line beside it says so); Before load, Earlier and the other
  slots work as always.

On the right you see:

- the preview of the entry (the picture at the moment it was saved), or "Empty" for an empty slot
- under it, a slim row of eight small blocks in the plastic of what the game saved to back then (the PlayStation's
  grey card, the PS2's black one, the Dreamcast's white VMU, the GameCube's, a cartridge's own colour): a block with a
  save shows the game's cover, the ring marks the slot Save or Load is on (no ring while Load is on Where I quit or Before
  load). The whole memory card or cartridge remains on the game's info page and the box's back
- above the keys: "Slot 3 · replaces the save from today 14:02" (on Save), "Slot 3 · saved
  yesterday 21:10" (on Load), "Slot 3 · empty" (on Save; on Load: "No save in slot 3 yet"), "Where I quit · today 14:02" or
  "Before load · today 14:05" (real times)

On pocket-ui's own host the slot you used last is remembered for each game, and the
[shortcuts](options.md#shortcuts) use the same slot.

A slow shortcut save or Earlier capture is allowed to finish when you open the pause menu or ask to quit meanwhile.

### Where the states are, for players coming from Android

The slot numbers are the ones the emulators use, so a state saved on the Android version is the
same slot here. All states are in `roms/savestates/<system>/` on the SD card. The file share leaves that folder out;
reach it with [Remote access (SSH)](settings-system.md#remote-access-ssh) or keep it in step with [Sync](sync.md).

| | Slots 1 to 8 | Where I quit | Before load |
|---|---|---|---|
| pocket-ui's own host | `<game>.pstate` (slot 1), `.pstate1` to `.pstate7` | `<game>.pstate.auto.z` | `<game>.pstate.undo` |
| PlayStation 2 (ARMSX2) | `<serial> (<CRC>).01.p2s` to `.08.p2s`, ARMSX2's own slots 1 to 8 | `….99.p2s` (slot 99, not in ARMSX2's menus) | `….98.p2s` |
| GameCube and Wii (Dolphin) | `<game file>.s01` to `.s08`, Dolphin's own slots 1 to 8 | `<game file>.pocket-auto` | `<game file>.pocket-undo` |
| 3DS (Azahar) | `<title id>.00.cst`, Azahar's quick slot (**one slot only**) | `<game file>.auto.cst` | `<game file>.undo.cst` |

Each state has a `.png` beside it, its preview. Slots 9 and 10 of ARMSX2 and Dolphin are left alone.

Older versions wrote the state of a Quit into ARMSX2's slot 0 and Dolphin's slot 1 (and Save into
Dolphin's slot 0). The first start after the update moves them: a state that pocket-ui wrote (it has
a preview picture beside it, which the emulators don't write) goes to Where I quit, Dolphin's old Save
becomes slot 1. A state without such a picture is yours and stays where it is; nothing is overwritten.

### The 3DS has one slot

Azahar has a quick slot and no shortcut for any other, so Save and Load use that one slot ("The
only slot") and the dots are left out. Where I quit and Before load work as above. They pass through that slot
for a moment while your own state waits beside it; if the SD card refuses to put it back, the shelf tells you after
the game, and the next start of the game tries again.

## Earlier

On pocket-ui's own host the game also keeps moments without you saving them. **Earlier**, right after Load, lists
them, newest first:

- **One every 5 minutes of play**, the last six: half an hour back. Only play counts: time in this menu, asleep or with
  the screen off does not.
- **Where a session began**: a game that continues from Where I quit keeps that moment too, for the last three sessions.

With Earlier selected, **Left/Right** walk through them; the chip says when ("‹ 12 min ›" ago, "‹ 14:02 ›" for a
session start today), the picture shows the moment and the line under it what it is: "Earlier · 12 min ago",
"Earlier · session start · today 14:02". **A** loads it. As with every Load,
what you were playing becomes **Before load** first, so Load → Before load takes you back.

- With none yet the window says "None yet" and the line "Earlier · none yet · keeps a moment every 5 min of play".
- A state that could not be written whole (the SD card was full) says "damaged, can't be loaded"; A does nothing there.
- A state that was replaced while the menu was open is not loaded in its place: "That earlier state was just
  replaced. Pick it again."

Turn it on or off under Options → Speed, rumble, fan → **Earlier states** ([Options](options.md#speed-rumble-fan)), for the console or the game. It
starts On, but Off for N64, PSP, DS, Dreamcast and Saturn: their states are 8 to 40 MB, so a handful would fill the space a
game has. Off, the row stays while there are earlier states of the game to load.

**The game never waits for it.** Most consoles' states are taken between two frames, in about a millisecond; the
picture and the file are written on the side. A console whose state takes longer than a quarter of a frame, and
every 3D one, is not interrupted for it: its state is taken a second after you open this menu, or kept from the
moment the Pocket goes to sleep. For those, the states are where you paused, at least 5 minutes of play apart.

**Space**: at most 100 MB a game and 1 GB on the SD card, the oldest go first; while less than 1 GB is free, none is
written. [Storage](storage.md) shows what they take, and its clean-up of old automatic states deletes them too.
[Sync](sync.md) leaves them out. The files are `<game>.pstate.earlier1` to `.earlier6` and `.pstate.started1` to
`.started3`, each with its `.png`, beside the other states.

## Saving with RetroArch

RetroArch has one save per game in the menu. On the right you see its preview and when it was made,
for example "Replaces the save from today 14:02" or "Saved today 14:02". With no save yet, the preview
area says "Empty".

On PlayStation 2, GameCube, Wii and 3DS, Save and Load close the menu and let the game run, then the
emulator saves or loads (a slot far up in Dolphin takes a few seconds: its slots are reached with
its own hotkeys). The preview of a save is the picture the menu was opened on.

A PlayStation 2 game has no Save and Load in the first moments after it starts: ARMSX2 names its saves after the
game, and until it has said which game runs there is nothing to save to. The menu says "Save and Load aren't ready
yet. Save in the game before you quit." Open the menu again once the game runs.

Load stays in the normal active text colour when a saved moment is available, even while the focus is on
Continue, Save or another row.
