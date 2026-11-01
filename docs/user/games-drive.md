# The GAMES drive

The ready-to-flash RG28XX card, 16 GB or bigger, makes a drive called **GAMES** at its first start, because this handheld
has no Wi-Fi for file sharing. No Wi-Fi, no second card, no app:

1. Switch the handheld off and put the card in your Mac or Windows PC. **GAMES** shows up like a USB stick.
2. Drag your games into its `roms` folder, each into the folder of its system: `roms/gba`, `roms/snes` and so on. The
   folders are already there. BIOS files go in `roms/bios`.
3. Eject the drive properly, put the card back in the first slot and switch on. Your games are on the shelf.

**Your computer sees the card's other parts too. Leave them alone.** Windows may ask to format one ("You need to format
the disk..."): never say yes, press Cancel. A Mac may say a disk isn't readable: press Ignore. Formatting would erase
pocket-ui from the card. Only GAMES is for you.

The hidden files a Mac leaves next to your games (`._Game.gba`, `.Spotlight-V100`, `.Trashes`) and Windows' `System
Volume Information` don't show up anywhere in pocket-ui, so there's nothing to clean up.

Good to know:

- Your saves and states go on GAMES too, next to your games, so copy them off with your games to keep a copy. Switch off
  from the menu when you can: a power cut can cost the last save on any card, and pocket-ui checks GAMES at the next
  start if it wasn't closed properly.
- **A games card in the second slot goes first.** While it's in, the handheld plays from that card and leaves GAMES alone.
  Take it out to go back to GAMES.
- Flashing the card again usually keeps what's on GAMES, because the new card gets the same layout. Still, keep your own
  copy of anything you'd miss: flashing tools can erase the whole card.
- A few PortMaster games use links between files, which GAMES can't hold (it's the same on any exFAT games card).

On all other cards, and on a smaller RG28XX card, the `roms` folder is on the card's STORAGE part, which uses the Linux file
system ext4. Windows and macOS can't write to that without extra software: use file sharing, a second card on a handheld
that has a second slot ([With a second SD card](copy-games.md#with-a-second-sd-card)), or a USB cable
([With a USB cable](copy-games.md#with-a-usb-cable)).
