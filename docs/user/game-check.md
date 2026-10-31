# Game check

Settings → Games → **Game check** answers "will my games start?". It goes through the card, on a quiet thread that
leaves the shelf alone, and shows either **All good** with how many games are ready, or a short list:

| Row | Example | What to do |
|---|---|---|
| **BIOS files** | "Sega CD: BIOS missing." with the exact file names; "The BIOS file is in the wrong folder"; "Wrong BIOS: its checksum differs"; "This BIOS file is empty" | Put your own console's file into `roms/bios`, or move or rename it as the line says |
| **Discs missing a file** | "Disc: Game.cue is there, its Game.bin is missing." Also an `.m3u` playlist none of whose discs can start | Put the file next to it, with exactly this name |
| **Empty files** | "This file is empty." | Copy the game again |
| **Zips that won't start** | "There is no game file in Game.zip"; "Game.zip holds 2 game files" | The line says it, as the shelf would at the start |
| **Same game twice** | `Game (1).gba` next to `Game.gba` in the same folder (the same file name in two folders of their own is two games) | A hides this copy from the shelf. The file stays, and Settings → System → Storage brings it back |
| **Wrong folder or file type** | A `.gba` in `roms/snes`, `.sfcc`, `Game.sfc.txt`, a `.rar`, a game two folders down, a folder that is no system's | The line names the folder or the name it should have |
| **Needs a stronger handheld** | PS2 games on an H700 handheld | Nothing: they stay on the card. Only said where it is known for this handheld |
| **Tips** | "PlayStation games start without a BIOS, and a few run better with one." | Your choice. Tips don't take the green away |

A on a row opens its games, A on a game says what is wrong, what to do and where it is. B goes back and stops a check
that is still running; X looks at everything again. Nothing is downloaded and no network is needed; pocket-ui never
deletes a game, never gets a BIOS or a game for you and never touches saves. A BIOS is checked against the checksums
of the known good files (size and checksum, the files of your own console); a game's content is not read, so a damaged
game that has the right size and name is only found when it is started.

After Add games with new games the page offers the check ("12 new games"); when games turn up in another way (with the
SD card in a computer, for example), a short note on Home says where it is.
