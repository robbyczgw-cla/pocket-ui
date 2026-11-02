# Second card

A way to copy games without Wi-Fi.

## One note on some systems

Pick a system on Add games and it may say under its Files and BIOS lines: *Runs on this handheld, but not every game runs
smoothly.* It appears where the system is heavy for the chip in your handheld (for example PlayStation 2 and GameCube on a
Snapdragon 865, Nintendo 64, PSP, Dreamcast and Nintendo DS on an H700), so a game or two may stutter or need another try. Systems
without the note are ones where games normally run fine; a handheld pocket-ui doesn't know shows no note at all.

## The ways on Add games

Settings → Games → **Add games** (and the first start's last step) has a row of tiles under the grid, one for each way your
handheld can take games: **File sharing** and **From your phone**. Press Down from the last row of systems (or tap), choose with
Left and Right, and press A to switch the one you chose on or off. Each tile says **On** or **Off**; the right side shows
the address and password of file sharing. Without a connection, A on **File sharing** takes you to the Wi-Fi
page. X still switches file sharing from anywhere on the page, and B goes back. A handheld without Wi-Fi has no file sharing
or phone tile, and a Steam Deck has neither (its games go in through Desktop Mode). A second card
and the GAMES drive are not switches: the page says how they work on your handheld.

## With a second SD card

On a handheld with a second card slot (all the Anbernic H700 models), a second card is the simplest way without Wi-Fi:

1. Take a card bigger than 8 GB, formatted exFAT, FAT32 or ext4 (the way cards come from the shop is fine). In your computer,
   make a folder called `roms` with a folder per system in it, for example `roms/gba`, and copy your games there.
2. Put it in the second slot. The handheld reads it by itself, also while it's on: the shelf shows its games within a few
   seconds, and a new card gets the folders of all systems the first time it's in.
3. To add more, switch off, take the card to your computer, copy, and put it back.

Good to know:

- **While the card is in, only its games are on the shelf.** The games on the first card aren't gone; they come back when you take
  the second card out and restart.
- **A card that still holds a system image doesn't work as a games card.** If you flashed it with pocket-ui or ROCKNIX, only its
  small space is used. Home and Add games say so; take it out, or format it for games in your computer.
- **Your saves and states are on that card too**, next to its games (`roms/savestates`). Settings stay on the first card.
- **Don't pull the card while a game runs.** Quit first, and switch off to take it out. If it comes out anyway: on pocket-ui's own
  host the game says its progress can't be saved, and asks before you quit; ROCKNIX's emulators (N64, PSP, Dreamcast and DS on
  these handhelds) can't say so. What was played since the last save is lost either way; what was saved before stays on the card.
  The shelf tells you the card is out. Put it back, and it reads the card again.
- The hidden files a Mac or Windows leaves on a card (`._Game.gba`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`, `System Volume
  Information`) are never shown as games.
