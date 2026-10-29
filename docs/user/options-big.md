# Options for PlayStation 2, GameCube, Wii and 3DS

**Options** in the pause menu shows a short list for the running game. Left/Right (or A) changes a
value; B goes back. Every change is kept for **this game only**.

| Option | PlayStation 2 | GameCube and Wii |
|---|---|---|
| Resolution | 1x to 3x, in steps of 0.25x | 1x, 1.5x, 2x, 2.5x, 3x |
| Renderer | Vulkan, OpenGL | Vulkan, OpenGL |
| Widescreen | widescreen patch on/off (games that have one) | widescreen hack on/off |
| Aspect ratio | 4:3, 16:9, Stretch | Auto, 16:9, 4:3, Stretch |
| Frame skip | – | Off, When slow |
| Show FPS | on/off | on/off |
| Performance | Max, Auto, Battery | Max, Auto, Battery |

The 3DS has its own list:

| Option | 3DS |
|---|---|
| Resolution | 1x (the 3DS's own), 2x, 3x |
| Screens | Stacked (top above bottom), Large top (with a small bottom screen), Top only |
| Renderer | Vulkan, OpenGL |
| Texture filter | Off, Bicubic, ScaleForce, xBRZ |
| Show FPS | on/off (a small counter in the corner) |
| Performance | Max, Auto, Battery |

## Xbox (experimental)

Options of an Xbox game (hakuX in RetroArch). There are no save states yet, so a change **takes effect the next time the game
starts** (the page says so); pocket-ui does not restart the game under you.

| Option | Xbox |
|---|---|
| Resolution | Original (480p, the Xbox's own), 2x (960p: sharper, more GPU). The default is 2x on the AYN Odin 2 and the other Snapdragon 8 Gen 2 handhelds, Original elsewhere |
| Screen scaling | Aspect (4:3 with black bars, the default), Fullscreen (stretches to the whole screen), Integer (whole-number multiples) |
| Performance | Max, Auto, Battery (applies at once) |

Set it for one game here; set it for all Xbox games with `xbox.pocket_resolution` and `xbox.pocket_scaling` in ROCKNIX's
system.cfg. The same names as pocket-core's Options → Picture → Screen scaling.

With **Top only** the bottom screen is hidden; games that need the touch screen need one of the
others.

**Performance** decides how hard the Pocket works while this game runs, and changes at once:

- **Max**: the big processor cores and the graphics chip run at full speed. The smoothest, but the Pocket gets
  warmer and the battery runs down faster.
- **Auto** (called Balanced until October 2026): the system decides, with ROCKNIX's own setting for this system.
  How fast that is depends on it: where ROCKNIX's setting for PS2, GameCube or 3DS is "performance", Auto is full
  speed too, and switching between Auto and Max changes little.
- **Battery**: lower top speeds. For light games; heavy ones slow down.

Without a choice of its own here, a game follows **Games: performance** (Settings → Playing → Performance & fan, or the
Performance row of [Quick settings](quick-settings.md#quick-settings)): Battery there is Battery here; Auto and Max there
are Max here, because these systems need full speed. Changing it in Quick settings over the paused game applies at
once; a game with its own choice here keeps it, and Quick settings say so ("this game: Auto").

When the game ends, the Pocket goes back to what it was before.

The changes apply when you go back to the game (Continue, B or HOME). The line under the list tells
you how:

- "Applies as soon as you go back to the game." The game simply goes on with the new value. On
  PlayStation 2 that is every option. On GameCube, Wii and 3DS it is Performance; on GameCube and
  Wii also Resolution, Show FPS and the switch between Auto and Stretch; Dolphin shows a short
  message, and a GameCube game (or a Wii game played with a GameCube controller) sees a button
  press for a moment (as with Save and Load).
- "Applies when you go back to the game: it restarts where you are." The menu shows
  "Applying…", the game is saved, closed and started again, and continues from that moment, the
  same way as after Quit. On GameCube and Wii this is the renderer, widescreen, frame skip, 16:9
  and 4:3; on 3DS every option but Performance; on PlayStation 2 only an option you had also set in ARMSX2's own
  menu (see below). On Wii with your own controller layout (ROCKNIX's "custom" Wii controller
  profile) every option restarts the game, so no button of yours is pressed by pocket-ui.

If the game can't be saved first, nothing restarts: the menu says "Couldn't save the game, so the
new options apply at its next start." After **Save** or **Load** an option that needs a restart also
waits for the next start.

**Advanced…** (PlayStation 2 only) closes pocket-ui's menu and opens ARMSX2's own menu over the
game, with all of its settings. Close it there (B, or "Resume Game") to play on. If you end the game
in that menu, pocket-ui closes ARMSX2 and you are back on the shelf. Dolphin (GameCube and Wii) and
Azahar (3DS) have no menu of their own on this device.

**More…** (GameCube and Wii) opens a second page with ROCKNIX's own settings for this game. B goes
back to the first page. Everything on it applies by a restart where you are, as above.

| Row | Choices | What it does |
|---|---|---|
| Button layout (GameCube) | A right, A bottom, Custom | A right puts GameCube A on the right button, as printed on the Pocket (the default). A bottom puts it on the bottom button. Custom appears once you have made a layout of your own in Dolphin |
| Shoulders (GameCube) | Triggers, Bumpers | Triggers: L and R on L2/R2, Z on R1. Bumpers: L and R on L1/R1, Z on R2 |
| Controller (Wii) | Sideways, Upright, Nunchuk, Classic, Custom | How the Wii Remote is held, or a Classic Controller |
| Rumble | On, Off | |
| Shaders | Skip until ready, Pause to compile, Hybrid ubershaders, Ubershaders only | Every new effect needs a shader first. Skip until ready (the default): no stutter, an effect can be missing for a moment. Pause to compile: a stutter each time. Ubershaders: no stutter, but much more work for the graphics chip, often too much on the Pocket |
| Shaders at start | On, Off | Compiles the shaders the game used before while it starts: a longer start, fewer stutters |
| VSync | Off, On | No tearing, a little more input lag. Off is what was tested |
| CPU clock | 100%, 125%, 150%, 200%, 75% | Higher can fix a game that slows down by itself, but costs the Pocket's processor; lower can help heavy games, and some then run wrong |
| Sync GPU thread | Off, On | Fixes crashes and glitches in a few games; costs speed |
| EFB reads by CPU, EFB copies, XFB copies, Texture cache | Accurate, Fast | Accuracy against speed. Fast is quicker but breaks some effects in a few games (aiming, maps, videos, wrong textures); Accurate is slower |

GameCube games start with A on the right button. If you had chosen a layout in ROCKNIX's settings
before, that choice stays. Wii games that take a GameCube controller always use ROCKNIX's "A
bottom" layout, as ROCKNIX's Wii start script sets it.
