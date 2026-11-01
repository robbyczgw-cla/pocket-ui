# Second card and USB cable

Two ways to copy games without Wi-Fi.

## The ways on Add games

Settings → Games → **Add games** (and the first start's last step) has a row of tiles under the grid, one for each way your
handheld can take games: **File sharing** and **USB file transfer**. Press Down from the last row of systems (or tap), choose with
Left and Right, and press A to switch the one you chose on or off. Each tile says **On** or **Off**; the right side shows
the address and password of file sharing, or how USB works. Without a connection, A on **File sharing** takes you to the Wi-Fi
page. X still switches file sharing from anywhere on the page, and B goes back. A handheld without Wi-Fi has no file sharing
tile, one without USB transfer has no USB tile, and a Steam Deck has neither (its games go in through Desktop Mode). A second card
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
- **Your saves and states are on that card too**, next to its games (`roms/savestates`). Settings stay on the first card.
- **Don't pull the card while a game runs.** Quit first, and switch off to take it out. If it comes out anyway: on pocket-ui's own
  host the game says its progress can't be saved, and asks before you quit; ROCKNIX's emulators (N64, PSP, Dreamcast and DS on
  these handhelds) can't say so. What was played since the last save is lost either way; what was saved before stays on the card.
  The shelf tells you the card is out. Put it back, and it reads the card again.
- The hidden files a Mac or Windows leaves on a card (`._Game.gba`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`, `System Volume
  Information`) are never shown as games.

## With a USB cable

The **USB file transfer** tile on Add games (Settings → Games → Add games) and the row in Settings → Connections are the same switch: whichever you use, both show the same state. It turns the handheld into a drive for your computer (MTP, as an Android phone). It is off
until you turn it on, stays on across restarts until you turn it off, and the row says **Off**, **On** or **Connected**.

1. Turn it on, and connect the handheld to the computer with a USB data cable. On a handheld with two USB ports, use the one marked OTG.
2. - **Windows, Linux:** it appears by itself in Explorer or the file manager.
   - **Mac:** Finder doesn't show such devices. Install the free app [OpenMTP](https://openmtp.ganeshrvel.com) and open it.
3. Open **Roms** and copy your games into the folder of their system. Turn the row off when you're done; the shelf then reads the folders again.

This shows the whole `roms` folder, `bios` and `savestates` included, as Wi-Fi file sharing does. Charging goes on as usual.
