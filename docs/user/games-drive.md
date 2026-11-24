# The GAMES drive

On the ready-to-flash RG28XX and RG35XX 2024 images (16 GB card or larger), pocket-ui creates a **GAMES** drive on first boot because
these handhelds have no Wi-Fi for file sharing. You won't see **GAMES** on your computer immediately after flashing; it appears after
you boot the card once in your handheld (first-start setup reminds you too), so there's no need to re-flash. No Wi-Fi, no
second card and no extra apps needed:

1. Power off your handheld and plug the microSD card into your Mac or Windows PC. **GAMES** pops up like a USB stick.
2. Drop your games into its `roms` folder, sorted by console (`roms/gba`, `roms/snes`, and so on; the folders are already
   there). BIOS files go in `roms/bios`.
3. Safely eject the drive, slide the card back into slot 1 and power on. Your games appear right on the shelf.

**Ignore the card's other partitions.** Windows may pop up "You need to format the disk...". Always click **Cancel**,
never Format. macOS may say a disk isn't readable; click **Ignore**. Formatting those partitions will wipe pocket-ui OS
from the card. Only **GAMES** is for your files.

Hidden Mac and Windows files (`._Game.gba`, `.Spotlight-V100`, `.Trashes`, `System Volume Information`) are ignored
automatically, so you never have to clean them up.

Good to know:

- Your saves and save states live on **GAMES** alongside your ROMs, so backing up `roms` backs up your saves too. Shut
  down from the menu whenever you can: a sudden power cut can lose your last save on any card, and pocket-ui runs a check
  on **GAMES** next boot if it wasn't shut down cleanly.
- **Games on both cards show together.** While a games card is in slot 2, the shelf lists its games and the games on
  **GAMES** side by side. Pop slot 2 out and only **GAMES** remains.
- Re-flashing the card usually keeps **GAMES** intact since the layout matches, though you should still back up anything
  important first, as flashing tools can wipe the entire card.
- **Offline updates** work the same way: drop the update zip from pocketui.xyz/install onto **GAMES** right next to
  `roms` and power on ([No Wi-Fi? Update from the card](updates.md#no-wi-fi-update-from-the-card)).
- A handful of PortMaster games rely on symlinks, which the exFAT **GAMES** drive doesn't support (true for any exFAT
  games card).

On all other cards (and on smaller RG28XX and RG35XX 2024 cards), the `roms` folder lives on the Linux ext4 `STORAGE` partition. Windows and
macOS can't write to ext4 without extra drivers, so use File sharing over Wi-Fi or a second SD card if your handheld has
two slots ([With a second SD card](copy-games.md#with-a-second-sd-card)).
