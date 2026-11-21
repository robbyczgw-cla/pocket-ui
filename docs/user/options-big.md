# Options for PlayStation 2, GameCube, Wii and 3DS

Opening **Options** in the pause menu shows a focused list for the game you're playing. Adjust values with Left/Right
(or A) and press B to go back. Every tweak saves for **this game only**.

| Option | PlayStation 2 | GameCube and Wii |
|---|---|---|
| Resolution | 1x to 3x, in 0.25x steps | 1x, 1.5x, 2x, 2.5x, 3x |
| Renderer | Vulkan, OpenGL | Vulkan, OpenGL |
| Widescreen | Widescreen patch On/Off (where supported) | Widescreen hack On/Off |
| Aspect ratio | 4:3, 16:9, Stretch | Auto, 16:9, 4:3, Stretch |
| Frame skip | - | Off, When slow |
| Show FPS | On/Off | On/Off |
| Performance | Max, Auto, Battery | Max, Auto, Battery |

Nintendo 3DS gets its own options:

| Option | 3DS |
|---|---|
| Resolution | 1x (native 3DS), 2x, 3x |
| Screens | Stacked (top above bottom), Large top (with small bottom screen), Top only |
| Renderer | Vulkan, OpenGL |
| Texture filter | Off, Bicubic, ScaleForce, xBRZ |
| Show FPS | On/Off (small corner counter) |
| Performance | Max, Auto, Battery |

## Xbox (experimental)

Options for Xbox games (running on hakuX in RetroArch). Since save states aren't available yet, changes **take effect
next time you start the game** (the menu reminds you) rather than restarting mid-session:

| Option | Xbox |
|---|---|
| Resolution | Original (native 480p), 2x (960p: sharper, heavier on GPU). Defaults to 2x on AYN Odin 2 and other Snapdragon 8 Gen 2 handhelds; Original elsewhere |
| Screen scaling | Aspect (4:3 with black bars, default), Fullscreen (fills screen), Integer (exact pixel multiples) |
| Performance | Max, Auto, Battery (applies immediately) |

Options set here apply to the current game; to set defaults for all Xbox games, use `xbox.pocket_resolution` and
`xbox.pocket_scaling` in ROCKNIX's `system.cfg` (matching pocket-core's Options → Picture → Screen scaling names).

On 3DS, **Top only** hides the bottom screen; games that need touch controls require one of the two-screen layouts.

**Performance** controls how hard your handheld pushes while playing, taking effect right away:

- **Max:** runs the big CPU cores and GPU at top clock speeds for the smoothest frame rates, warmer device and shorter battery
  life.
- **Auto** *(called Balanced before October 2026)*: follows ROCKNIX's system profile for that console. Where ROCKNIX
  already sets PS2, GameCube or 3DS to "performance", Auto runs at full speed too, so Auto and Max feel identical.
- **Battery:** caps top speeds lower, which is fine for lighter titles, though demanding 3D games will slow down.

If you haven't set a per-game choice here, the game follows **Games: performance** (Settings → Playing → Performance &
fan, or Quick settings' **Performance** row; see [Quick settings](quick-settings.md#quick-settings)): Battery stays
Battery, while Auto and Max both run at Max since these heavy systems need full power. Changing Performance in Quick
settings updates immediately unless the game has its own override here (Quick settings notes "this game: Auto"). When
you exit the game, clocks return to normal.

Changes take effect as soon as you return to the game (Continue, B or HOME). The footer tells you whether a restart is
needed:

- **"Applies as soon as you go back to the game."** Takes effect live without interrupting play. That covers every PS2
  option, Performance on GameCube/Wii/3DS, plus Resolution, Show FPS and Auto/Stretch on GameCube and Wii (Dolphin
  flashes a brief message, and GameCube games or Wii games using a GameCube pad see a momentary button tap just like
  Save/Load).
- **"Applies when you go back to the game: it restarts where you are."** pocket-ui shows "Applying…", saves your spot,
  restarts the emulator and drops you right back where you were. Used for Renderer, Widescreen, Frame skip and 16:9/4:3
  on GameCube/Wii, every non-Performance option on 3DS, and any PS2 option you previously overrode inside ARMSX2's own
  menu. On Wii with ROCKNIX's "custom" Wii controller profile, every option restarts cleanly so pocket-ui never injects
  a button press.

If saving fails before a restart, pocket-ui keeps your session alive and tells you: "Couldn't save the game, so the new
options apply at its next start." Changes made right after **Save** or **Load** that need a restart also wait for the
next launch.

**Advanced…** *(PlayStation 2 only)* opens ARMSX2's full native menu over the game. Press B or pick "Resume Game" to
return, or exit there to return cleanly to the shelf. Dolphin (GameCube/Wii) and Azahar (3DS) don't expose an in-game
menu on this device.

**More…** *(GameCube and Wii)* opens a second page of ROCKNIX's Dolphin tweaks for this game (press B to go back; all
changes apply via quick save-and-restart):

| Row | Choices | What it does |
|---|---|---|
| Button layout (GameCube) | A right, A bottom, Custom | A right (default) matches the Pocket's printed right button; A bottom moves GameCube A to the bottom button. Custom appears if you created a Dolphin profile |
| Shoulders (GameCube) | Triggers, Bumpers | Triggers maps L/R to L2/R2 and Z to R1; Bumpers maps L/R to L1/R1 and Z to R2 |
| Controller (Wii) | Sideways, Upright, Nunchuk, Classic, Custom | Sets Wii Remote orientation or Classic Controller mode |
| Rumble | On, Off | Toggles controller vibration |
| Shaders | Skip until ready, Pause to compile, Hybrid ubershaders, Ubershaders only | Controls shader stutter. Skip until ready (default) avoids stutters with occasional brief pop-in; Pause to compile stutters on new effects; Ubershaders prevent stutter but are heavy on handheld GPUs |
| Shaders at start | On, Off | Pre-compiles known shaders on launch: slightly longer load, smoother gameplay |
| VSync | Off, On | Eliminates screen tearing at the cost of a little input lag (tested with Off) |
| CPU clock | 100%, 125%, 150%, 200%, 75% | Overclocking fixes in-game slowdown at higher CPU cost; underclocking can help heavy titles but may cause glitches |
| Sync GPU thread | Off, On | Fixes crashes or graphical bugs in a handful of games at a speed cost |
| EFB reads by CPU, EFB copies, XFB copies, Texture cache | Accurate, Fast | Speed vs accuracy trade-off. Fast runs quicker but can break maps, aiming, videos or textures in some games |

GameCube games default to A on the right button (preserving any layout you already set in ROCKNIX). Wii games using a
GameCube controller always use ROCKNIX's "A bottom" layout per ROCKNIX's Wii launch script.
