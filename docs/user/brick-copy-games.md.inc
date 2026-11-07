# Games on the card

pocket-ui on the Brick runs from its own SD card, and your games live on it too. Without Wi-Fi: switch off, take the card to your
computer, copy the games into the card's `Roms/<system>` folders (for example `Roms/gba`), put it back and switch on. Saves and
states stay on the card (`Saves/savestates` for the states). Don't pull the card while a game runs: quit first and switch off.

## Which folder

Each system has its folder in `Roms`: `gb`, `gbc`, `gba`, `nes`, `snes`, `psx`, `megadrive`, `mastersystem` and `gamegear`,
and also `ngp` / `ngpc` for Neo Geo Pocket/Color,
`wonderswan` / `wonderswancolor`, `pcengine`, `atarilynx`, `atari2600`, and `atari7800`.
Arcade games go in `fbn`, `neogeo`, `cps1`, `cps2`, or `cps3`: keep their ZIP archives intact and
use ROM sets matching FBNeo `f3b7749`. Neo Geo also needs `neogeo.zip` alongside its games or in
`Roms/bios`. The card includes no games or BIOS. N64, PSP and Dreamcast games go in `n64`, `psp` and `dreamcast`; Dreamcast BIOS files (`dc_boot.bin`, `dc_flash.bin`, optional) go in `bios/dc`.

Additional folders: `fds`, `sg-1000`, `segacd`, `sega32x`, `pcenginecd`, `supergrafx`, `pokemini`,
`virtualboy`, `pico-8`, `tic-80`, `nds`, `pc`, `scummvm`, and `saturn`.
Put Sega CD discs in `segacd` (CUE plus its tracks, CHD or M3U), with the matching region's
`bios_CD_U.bin`, `bios_CD_E.bin` or `bios_CD_J.bin` in `Roms/bios`. PC Engine CD needs exactly
`Roms/bios/syscard3.pce`. FDS needs `Roms/bios/disksys.rom`. Missing required BIOS files produce
an error naming the file; the card does not supply them.

PICO-8 uses the free FAKE-08 emulator: copy `.p8` or `.p8.png` carts, not the official runtime.
TIC-80 accepts Lua `.tic` carts. DOS games go in `pc` (ZIP/DOSZ or individual EXE/COM/CONF).
ScummVM uses supported game folders or `.scummvm` launch files and the card's bundled engine data;
copy all original game files, not just the launcher.

**DS, Saturn and Dreamcast are experimental.** DS uses melonDS DS's CPU renderer; Saturn uses Yabause's
software renderer and is expected to be too slow for many games. Dreamcast runs on the GPU, but not at
full speed: Virtua Tennis 2 measured 37 of 60 frames per second. DS and Saturn speed on the Brick has not
been measured. The built-in pad has no sticks or touchscreen: games requiring a mouse, DS touch or the
Virtual Boy's second D-pad may need controls the current mapping cannot provide. Gamepad-friendly
DOS/ScummVM games are the practical starting point.

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
or phone tile, and a Steam Deck has neither (its games go in through Desktop Mode).

On the Brick, phone and web transfers write at a limited speed to keep large wireless copies stable.
For a large game collection, copying to the card from your computer is faster.
