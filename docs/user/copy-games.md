# Second card

How to copy games without Wi-Fi.

## One note on some systems

When you highlight a demanding console on Add games, you may see: *Runs on this handheld, but not every game runs
smoothly.* This appears whenever a console pushes your handheld's chip (for example PS2 and GameCube on a Snapdragon
865, or N64, PSP, Dreamcast and DS on an H700), so expect some titles to stutter or need tweaks. Systems without that
note generally run full speed; unknown hardware shows no note.

## The ways on Add games

Settings → Games → **Add games** (and first-start setup) lists transfer options under the system grid: **File sharing**
and **From your phone**. Move Down past the last system row (or tap), pick with Left/Right, and press A to toggle. Each
tile shows **On** or **Off**, with your File sharing address and password on the right. If you're offline, pressing A on
**File sharing** jumps straight to Wi-Fi settings. Pressing X toggles File sharing from anywhere on the page, and B goes
back. Non-Wi-Fi handhelds hide those tiles and explain how a second card or the GAMES drive works instead.

## With a second SD card

On handhelds with a second SD slot (all Anbernic H700 models), a second card is the easiest way to copy games offline:

1. Grab a microSD card larger than 8 GB formatted as exFAT, FAT32 or ext4 (factory formatting works fine). On your
   computer, create a `roms` folder with one subfolder per console (like `roms/gba`) and copy your games in.
2. Insert it into slot 2 (TF2). Your handheld detects it automatically (even while powered on), and your games appear on
   the shelf within seconds. A blank card automatically gets all console folders created on first insert.
3. To add more games later, power off, pop the card into your computer, copy, and slide it back in.

Good to know:

- **Games on both cards show together.** While the second card is in, the shelf lists its games and the games on your
  first card side by side. A game with the same folder and file name on both cards shows once. Two versions of a game
  (another region, a zip beside the unpacked file) both show; [Game check](game-check.md) offers to hide the extra one.
- **Don't use a card that still holds an OS image.** If it was flashed with pocket-ui OS or ROCKNIX, only a small
  partition is visible. Home and Add games warn you if that happens: remove it or reformat it on a computer first.
- **Saves and save states live on the second card too**, next to your games (`roms/savestates`). Settings remain on card
  1.
- **Never eject the card while a game is running.** Quit the game and power off first. If a card is pulled mid-game,
  pocket-ui's own player warns that progress can't be saved and asks before quitting (ROCKNIX's standalone emulators for
  N64, PSP, Dreamcast and DS on H700 cannot warn you). Unsaved progress since the last save is lost, while earlier saves
  stay intact. The shelf lets you know the card was removed and reloads it when reinserted.
- Hidden metadata files left by macOS or Windows (`._Game.gba`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`, `System
  Volume Information`) are ignored automatically.
