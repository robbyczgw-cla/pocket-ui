# Frozen games

Your last five games with a saved moment stand a little open on the shelf: on Home's recent row and in their system's row,
the game peeks out over the case's top, and when you pick one its cartridge, card or disc comes a little out of the side.
**A** goes straight back to the moment you left, with no question and no waiting. It's the same moment and the same
start as [Continuing where you left off](playing.md#continuing-where-you-left-off): nothing keeps running and nothing extra goes on
your card. In the Minimal, Split, Cards and Cabinet looks the word **Frozen** beside the game says the same.

How fast depends on the emulator. From A until the moment is back on the screen, measured on real games:

| System | Mini V2 | RG35XX Plus |
|---|---|---|
| Game Boy, Super Nintendo, Game Boy Advance | 0.1 to 0.2 s | 0.3 to 0.5 s |
| PlayStation | 0.2 s | 0.7 s |
| Nintendo DS | 0.4 s | |
| PSP | 0.7 s | |
| Nintendo 64 | 0.9 s | |
| PlayStation 2 | about 3 s, then its load | |
| GameCube | about 18 s | |

PlayStation 2, GameCube, Wii and 3DS boot the game first and take as long as their emulator takes: they show a small spinner
until then. A game that can't be frozen stands as a normal case:

- **No saved moment:** Xbox (until its core has save states), Wii U, PS3, Vita, ports, ScummVM, PICO-8, TIC-80 and other
  emulators without one. A game you haven't quit yet stands closed too. PlayStation 2 games stand open from your next Quit.
- **The game now runs on another emulator or core:** its moment belongs to the old one. The start says so and begins fresh; the old moment stays under Load. This holds after a game on another emulator or core you played in between.
- **The game's file changed** (a new copy or a different version, judged by what is in the file, also the tracks of a `.cue` or `.m3u`; copying the card alone changes nothing): a moment of another file isn't loaded. The start says so and
  begins fresh; until your next Quit the moment is still in the pause menu under Load.
- **The moment is gone** (you deleted it, or a card was swapped): the start says so and begins fresh. In-game saves are never
  touched by any of this.
- **The emulator refuses the moment** (after an update of its core): the same, "Couldn't go back to that moment".

Settings → System → **Frozen games** switches it off: every case is closed and starts as before. With
[Remember where I quit](settings.md#remember-where-i-quit) off, a Quit freezes nothing new and the switch's line says so; moments
from before and from sleep still stand open, and A goes back to those. Motion off: the game is simply out, and the spinner's dots
stand still.

In-game saves (the kind a cartridge keeps, "battery saves") on pocket-ui's own host are written
every 10 seconds when they change and again when you quit. They are stored in the same place and
format RetroArch uses, so they work with both.

