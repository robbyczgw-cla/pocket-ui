# Options

Pause menu → **Options**: the settings of the running game. This page describes the Options of games on
pocket-ui's own host (see [Systems](systems.md#on-pocket-uis-own-host)). PlayStation 2, GameCube, Wii and 3DS
have a shorter list of their own ([Options for PlayStation 2, GameCube, Wii and 3DS](options-big.md)); for RetroArch and the
other emulators, Options opens [Runs on](#runs-on): which emulator plays the game, and RetroArch's own settings.

| Entry | What it holds |
|---|---|
| **Picture** | Size, sharpness, screen effects, frame and orientation, with a live preview. |
| **Emulator** | Settings of the emulator itself, such as colours and palettes. Shown only when the emulator offers settings. |
| **Controls** | Which button does what, and the left stick. |
| **Cheats** | Cheats from the libretro cheat database. Shown only when it has cheats for the game. |
| **Shortcuts** | HOME + button combinations while you play. |
| **Extras** | Fast-forward speed, rewind, what is shown over the game, rumble, performance, fan and run-ahead. |
| **Runs on** | Which emulator plays this game or this console (see [Runs on](#runs-on)). It has its own Save changes. |
| **Save changes** | Keeps the settings for this console or this game. |

Move with Up/Down (the list wraps around at the ends), change a value with Left/Right (A also steps
to the next value), and go back with B. The line at the bottom explains the selected setting.
**A change applies to the running game at once** and lasts until you quit. To keep it, use
[Save changes](#save-changes).

## Picture

Pause menu → Options → **Picture**.

While this page is open, a **live preview** shows the game's last picture with the settings as they
are now, in two views side by side:

- **Whole screen**: the full screen, made smaller. Shows the size, the position and the frame.
- **Actual size**: a piece from the middle of the screen at its real size. Shows scanlines, the LCD
  grid and the CRT mask exactly as in the game.

The preview is drawn again after every change. If it cannot be drawn, it says "No preview". With Large text the
list shows three or four settings at a time and moves along as you go up and down, so the preview stays large.

### Screen scaling

| Choice | What it looks like |
|---|---|
| **Integer** | Integer scaling: scaled by a whole number, so every game pixel is the same size: the sharpest picture, with black borders. Game Boy 7× (1120 × 1008), Game Boy Advance 5× (1200 × 800), NES and SNES 4× (about 1170 × 896 and 1195 × 896). The whole number is the largest one whose width still fits the screen, in the shape the emulator reports; so a NES picture that reports 240 lines in 4:3 would be 3× (960 × 720), while the 224 lines it normally shows are 4×. |
| **Integer, smaller** | One whole number smaller than Integer, so every pixel is still the same size and a screen frame has room: Game Boy 6× (960 × 864), Game Boy Advance 4× (960 × 640), NES and SNES 3× (about 880 × 672 and 896 × 672). |
| **Aspect** | As large as fits, in the console's own shape. Bigger, but pixels are no longer all the same size. |
| **Fullscreen** | Fills the screen and stretches the pixels. |

### Screen sharpness

| Choice | What it looks like |
|---|---|
| **Sharp** | Hard pixel edges. |
| **Crisp** | Scales up sharply first, then smooths a little. Hides uneven pixel sizes with Aspect scaling; every pixel keeps its weight. With Integer scaling it is as sharp as Sharp. |
| **Soft** | Blurs the edges. |

### Screen effect

| Choice | What it looks like |
|---|---|
| **None** | The plain picture. |
| **LCD grid** | Thin dark gaps between the pixels, like a handheld's screen. They grow with the picture: about a pixel of the screen at Game Boy and Game Boy Advance sizes, fainter on the smaller DS screens. |
| **Scanlines** | The picture's rows drawn like a TV's beam: thin rows with dark gaps in dark parts, rows that close up in bright parts. Colours stay as bright as without the effect. |
| **CRT** | Scanlines, plus a very fine colour mask (too fine to see at arm's length) and a soft glow around bright parts, like an old TV. No curved screen. |

All three stay even at every size, Aspect included: no row or column is thicker than the next. Scanlines need about
three screen pixels per row of the game; on pictures with more rows (some PlayStation menus in 480 lines) they fade
out, as they did on a TV. For 3D consoles that draw sharper than the original (Nintendo 64 at twice its lines),
scanlines follow the console's own lines.

**LCD colours** (Game Boy Advance, shown while LCD grid is chosen): **On** shows the colours of the Game Boy Advance's
own screen, softer and darker, as the games were made for. It is left out while the emulator's own colour
correction is on (Options → Emulator), so the colours are never corrected twice. Game Boy Color games get their
screen's colours from the emulator already.

### Screen frame

| Choice | What it looks like |
|---|---|
| **None** | Black around the picture. |
| **Auto** | A frame around the picture that suits the console (see below). |
| **Ambient** | The space around the picture glows softly in the game's own colours, like a TV with ambient light. It follows the game smoothly as the scene changes. |
| **Shell** | The screen lens in a handheld's body. Pick its colour under **Shell colour** (shown while Shell is chosen): classic grey, indigo, clear purple, atomic purple, black, white or glacier blue. The clear ones are see-through. |
| **Brick** | A brick-shaped handheld: darker glass around the screen with one large rounded corner, two thin coloured lines above the picture and a power light beside it. **Brick colour**: grey, clear, red, yellow, green or black. |
| **Pocket slim** | A slimmer handheld with tighter glass. **Pocket finish**: brushed silver or clear. |
| **Clamshell** | An opened clamshell's lid with the hinge along the bottom. **Clamshell finish**: platinum, cobalt, flame red or graphite. |
| **Wide** | A landscape handheld with rounded sides, shoulder buttons peeking over its top corners and a small speaker grille. |
| **Twin** | Two screens with a hinge between them and speaker slits beside the top screen. Offered for the Nintendo DS only; see below. |
| **Gloss** | A piano-black wide body with oval ends and a soft diagonal sheen. |
| **Swan** | A minimal handheld. **Swan finish**: pastel or a see-through skeleton. |
| **Watch** | A brushed metal faceplate around a small screen window, like an old single-game handheld. **Watch finish**: gold or silver. |
| **Goggles** | A black visor with red trim, for red-on-black pictures. |
| **CRT** | An old TV: the picture behind curved dark glass, in a cabinet of dark plastic or wood (**TV cabinet**, shown while CRT is chosen). |
| **Studio monitor** | A grey professional monitor with flat glass and a row of small knobs and buttons below. |
| **Portable TV** | A small portable TV with a telescopic antenna and a tuning dial. |
| **Woodgrain** | A wood-look front with a ribbed black band on top. |
| **Arcade** | A glossy black arcade bezel with two soft light strips. |
| **Glow** | A thin rounded line around the picture, glowing in the console's colour from the shelf. |

The colour or finish of a frame is a row of its own under **Screen frame**, shown while that frame
is chosen.

A frame needs space around the picture, so it appears only with Integer, Integer smaller and Aspect
scaling. Fullscreen fills the screen and leaves no space for one. The frame sits
behind the picture and never covers any of it; when the picture changes size, the frame follows.
At Integer the space is narrow on some consoles (Game Boy Advance: 20 pixels left and right);
**Integer, smaller** gives every frame space, and details such as speaker grilles, knobs and the
antenna only appear where there is space for them.

**The power light.** Brick, Pocket slim, Clamshell, Wide, Gloss and Swan have a small power light
that shows the battery: green above 20%, red at 20% and below, blinking slowly at 5% and below.
It flashes briefly when you save a state (to a slot, or the automatic one).

**Twin and the Nintendo DS.** Twin fits the DS with its two screens stacked, as games start. The
hinge sits beside the screens where they touch, and exactly in the gap between them when the
emulator leaves one (its screen gap option). With the screens side by side or one screen alone,
Twin shows the plain screen lens instead.

For Auto, pocket-ui picks one of its own frames for the console:

| Console | Auto |
|---|---|
| Game Boy | Brick, grey |
| Game Boy Color | Pocket slim, clear |
| Game Boy Advance | Wide |
| Nintendo DS | Twin |
| PSP | Gloss |
| WonderSwan | Swan, pastel |
| WonderSwan Color | Swan, skeleton |
| Game & Watch | Watch, gold |
| Virtual Boy | Goggles |
| Atari 2600, Atari 7800 | Woodgrain |
| Game Gear, Neo Geo Pocket (Color), Lynx, Pokemon mini | **Handheld**: a dark screen lens around a recessed screen, with a small power light where there is room for it |
| Every other console | **Monitor**: a graphite surround with rounded corners |

A frame you picked yourself stays as it is; only Auto follows this table.

<!-- TODO: frame packs (/storage/.config/pocket-ui/frames/, docs/FRAMES.md) are meant for the
     store. Describe them for players once there is a way to install them. -->

### Screen orientation

Some games were made for a screen that stands upright: vertical shooters, many WonderSwan games, a few Game Boy and
DS games that you hold like a book. On the Pocket's nearly square screen their picture is small, with black on both
sides. Hold the Pocket turned a quarter, and the picture can be much bigger.

| Choice | How you hold the Pocket |
|---|---|
| **Normal** | As usual. |
| **Turned left** | Turned a quarter to the left: the D-pad at the bottom, the face buttons at the top. |
| **Turned right** | Turned a quarter to the right: the D-pad at the top, the face buttons at the bottom. |

- The picture is drawn turned so that it stands upright for you, and scaled again for its turned shape. A vertical
  game of 224 × 288 pixels grows from 648 × 864 to 1195 × 896 at Integer.
- The **D-pad and the sticks turn with it**: up on the D-pad is up in the game. The other buttons stay where they
  are; change them under [Controls](#controls) if a game needs it. A Bluetooth controller is not turned, because it
  does not turn with the Pocket.
- The pause menu, Quick settings and messages stay as usual: turn the Pocket back to use them.
- Touch follows the turned picture.
- Of the frames, only **Ambient** and **Glow** show around a turned picture: the others are drawn for a Pocket held
  as usual and would stand on their side. Your frame comes back with Normal.
- Screenshots (HOME + A) keep the game's picture as the emulator makes it, not turned.
- Some emulators turn the picture themselves: the WonderSwan emulator turns a vertical game upright when you press
  SELECT. pocket-ui shows the picture as the emulator asks, and Screen orientation turns it once more on top.
- Keep the choice for one game with [Save changes](#save-changes) → For this game.

Arcade games run on RetroArch, which has its own way: see [Arcade games](arcade.md#vertical-games).

### Starting values per console

Every console starts with the look that suits it on this screen:

| Consoles | Starts with |
|---|---|
| Disc and 3D consoles: PlayStation, Mega-CD, PC Engine CD, Saturn, Nintendo 64, PSP, Dreamcast | **Aspect**, **Crisp**, None, frame None |
| Nintendo DS (both screens stacked) | **Aspect**, **Crisp**, None, frame None |
| ScummVM (320 × 200 adventures) | **Aspect**, **Crisp**, None, frame None |
| Everything else, handhelds and 2D home consoles included | Integer, Sharp, None, frame None |

Out of the box no console has a screen effect or a frame: a clean picture. Pick an effect or a frame in the pause menu
(Options → Picture, then Save changes) for one game or a whole console.

With **Handhelds** or **Every console** in [Settings → Look & feel → Screen frames](settings.md#screen-frames), the handhelds or every
console start with frame **Auto** instead of None. A frame saved for a console or a game stays.

Only the systems listed under [On pocket-ui's own host](systems.md#on-pocket-uis-own-host) have the
Options menu today. The other starting values apply once more systems move there.

## Emulator

Pause menu → Options → **Emulator**: the settings of the emulator itself, with the names and
explanations the emulator gives them, for example colour palettes or colour correction. pocket-ui
leaves out settings that make no sense on this device, and starts some with values that suit it
better.

- Some emulators sort their settings into groups. A group opens with A or Right; B goes back.
- Left/Right change a value; A steps to the next one. Where a setting has a long list of values,
  L1/R1 skip five at a time; its explanation says so.
- Some settings only take effect when the game starts again. Their explanation says: "Takes effect
  the next time the game starts. Save it, then start the game again."
- A change reaches the game when you continue playing. If it makes other settings appear or
  disappear, the page shows that the next time you open it.

## Controls

Pause menu → Options → **Controls**.

- **Left stick as D-pad** (On / Off): the left stick works as the D-pad. It is on by default, except
  for PlayStation, where both sticks go to the game as analogue sticks. Turn it off for games that use
  an analogue stick.
- Then one line for each button of the Mini V2: A, B, X, Y, L1, R1, L2, R2, L3, R3, Select, Start
  and the four D-pad directions. The value is the console button it presses, by the name the
  emulator uses; the explanation reads, for example, "A presses A." Left/Right choose another
  console button, or **None** ("X does nothing in this game."). L3 and R3 mean pressing the left
  and right stick in.
- **Reset controls** puts every button back to doing what it says.

When you play together ([Players](pause-menu.md#players)), the layout is the same for every player: a connected
controller's buttons do what these lines say, by where the button sits (the right face button is A). These lines name the buttons as printed on the Pocket, even when Settings → Playing → [Button layout](settings.md#button-layout) is Xbox: that setting changes pocket-ui's menus, not games.

HOME, and SELECT + START held to quit, always work on the real buttons, whatever you change here.

## Cheats

Games on pocket-ui's own host get a **Cheats** page in the Options menu when the libretro cheat
database (the one RetroArch uses, already on the console) has cheats for them. Each cheat is On or
Off; A turns it on or off, L1 and R1 skip five lines, "All off" at the end switches them all off. What is on is
remembered for that game, takes effect at once and is back the next time you start the game. The game
is found by its file name, ignoring the bits in brackets (region and so on): "Contra III - The Alien
Wars (USA).sfc" finds "Contra III - The Alien Wars". A game whose name the database doesn't know has no
Cheats page; neither do consoles without cheats in it. Cheats are not part of "Save changes".

Some cheat files, mostly for Atari, PC Engine and a few NES games, do not give codes. They name a place in memory and
a value to write there. pocket-core can't apply those yet. The last line of the page says how many there are ("3 cheats
use a format pocket-core can't apply yet"), so nothing is lost without a word. A game that has only such cheats still
gets the page, with that line alone. Cheats with a code are not affected.

## Shortcuts

Pause menu → Options → **Shortcuts**. On pocket-ui's own host, hold HOME and press another button
while you play:

| Row | Keys | What it does |
|---|---|---|
| **Save state** | HOME + R1 | Save to the selected slot. |
| **Load state** | HOME + L1 | Load the selected slot. |
| **Change slot** | HOME + Left / Right | Choose the previous / next slot (1 to 8). |
| **Fast-forward** | HOME + R2 | Turn fast-forward on or off. The speed is set under [Extras](#extras). |
| **Rewind** | Hold HOME + L2 | Rewind, for as long as you hold it. Rewind itself has to be switched on under [Extras](#extras). |
| **Screenshot** | HOME + A | Save a [screenshot](playing.md#screenshots) of the game. The pause menu does not open. |
| **Over the game** | HOME + Y | Switch to the next level of [Over the game](pause-menu.md#over-the-game): Off, FPS, FPS + sensors, Pixel Path. It lasts until you quit or change **Over the game** under Extras. |

A short message over the picture confirms what happened, for example "Saved to slot 3",
"Loaded slot 3", "Slot 4 (empty)", "Slot 4 is empty" or "Screenshot saved".

While HOME is held, the game gets no buttons, so a shortcut never presses anything in the game.
L2 and R2 count as pressed from half-way down. On this page each shortcut can be switched **Off**.

## Extras

Pause menu → Options → **Extras** ("Settings you rarely need."). Values in bold are the defaults.

| Setting | Choices | What it does |
|---|---|---|
| **Fast-forward speed** | 2x, 3x, **4x**, 8x, As fast as possible | How fast the game runs while fast-forward is on (HOME + R2). Sound is muted meanwhile, and "FF 4x" (or the speed you chose) shows over the picture. |
| **Rewind** | **Off**, On | Remembers the last minute so you can play it backwards: hold HOME + L2. "REWIND" shows over the picture and sound is muted; at the oldest moment the picture stops. Uses some extra power. |
| **Earlier states** | Off, **On** | Keeps a state every 5 minutes of play, the last half hour, and where each session began; the pause menu's **Earlier** loads them ([Earlier](pause-menu.md#earlier)). The game never waits for it. **Off by default for N64, PSP, DS, Dreamcast and Saturn**: their states are 8 to 40 MB. At most 100 MB a game and 1 GB on the SD card. |
| **Over the game** | **Off**, FPS, FPS + sensors, Pixel Path | What is drawn over the game while you play. **FPS** shows frames per second and CPU load in the top left corner, for example "60 FPS   CPU 23%", updated twice a second. **FPS + sensors** adds temperature, fan and battery draw: see [the FPS line and sensors](pause-menu.md#the-fps-line-and-sensors). **Pixel Path** shows one line beside the picture instead, which turns orange when something slips: see [Pixel Path over the game](pause-menu.md#pixel-path-over-the-game). The start value is the one under Settings → Playing → [Over the game](settings.md#over-the-game). |
| **Rumble** | Off, Low, **Medium**, High | How strongly games that rumble shake the console. |
| **Performance** | Battery, **Auto**, Max | **Auto**: the system decides how fast the CPU runs (until October 2026 this was called Balanced). Battery limits the CPU to half speed: enough for Game Boy and NES. Max keeps the CPU at full speed for demanding games. |
| **Fan** | **Quiet**, Auto, Cool | Quiet starts the fan later and keeps it slow. Auto keeps the system's cooling profile. Cool starts it earlier. With [Smooth fan](performance.md#smooth-fan) on, each is a smooth curve. |
| **Run-ahead** | **Off**, 1 frame, 2 frames | The game responds to your buttons 1 or 2 frames sooner (a frame is 1/60 s). Only on NES, Famicom Disk System, SNES, Game Boy, Game Boy Color, Game Boy Advance, Mega Drive, Master System, Game Gear, SG-1000, PC Engine, SuperGrafx, Atari 2600, WonderSwan and Virtual Boy; the row is not shown for other systems. See [Run-ahead](#run-ahead). |

- Rewind is off by default because keeping the history costs processor time, most on SNES.
  Loading a save clears the history. Holding HOME + L2 while Rewind is off shows "Rewind is off.
  Switch it on under Extras."
- Performance and Fan apply only while the game runs. The system's own settings come back when you
  quit, even if the game crashed.
- What a game starts with when you saved nothing for it or its console is set in Settings → Playing →
  [Performance & fan](performance.md) (**Games: performance** and **Games: fan**), or in
  [Quick settings](quick-settings.md#quick-settings) over the paused game. The bold values
  above are the defaults of that page. A value saved here for a console or a game wins over it, and "Restore
  defaults" goes back to it.

### Run-ahead

Many games react to a button a frame or two after you press it, inside the console itself. Run-ahead hides that
delay: every frame the game is run ahead by the frames you chose with the buttons you hold, that picture is shown,
and the game goes back to where it really is. Jumps and shots start sooner; nothing else changes.

- **Which value.** Most games react one or two frames after the press. Start with 1 frame. If the game still feels
  late, try 2 frames. Run ahead further than a game's own delay and the picture jumps a little when you press a
  button: then go back to 1 frame. Suggestions to start from:

  | Console | Start with |
  |---|---|
  | NES, Game Boy, Game Boy Color, Master System, Game Gear, SG-1000 | 1 frame |
  | SNES, Mega Drive, PC Engine | 1 frame; many games here take 2 |
  | Game Boy Advance | 1 frame. 2 frames are too much for the Mini V2 in heavier games (F-Zero: Maximum Velocity) and switch themselves off |
  | Atari 2600, WonderSwan | 1 frame |
  | Famicom Disk System, SuperGrafx, Virtual Boy | 1 frame |

- **Cost.** The console runs two or three frames for every one you see, so the game uses more processor time and
  power (the measurements are in the developer notes, [HOST.md](../HOST.md)).
- **It switches itself off** where it can't work (the first seconds after the start or a load don't count), and says so over the game: "Run-ahead isn't available for this
  emulator", "Run-ahead is off: too slow for this game", "…: saving states takes too long here", "…: this game
  doesn't replay exactly". Choosing another value in the menu tries again.
- Fast-forward and rewind work as before (run-ahead pauses while they run). Save states, the auto-resume state and
  cheats always use the real game, not the frames run ahead.
- It is saved like the other settings: for this console, for this game, or not at all (Save changes).

## Save changes

Options → **Save changes** keeps everything set in Options: Picture, Emulator, Controls, Shortcuts
and Extras. It offers three choices. The line at the bottom tells you which settings the game uses
now: "Using this game's settings.", "Using this console's settings." or "Using the defaults."

| Choice | What it does |
|---|---|
| **For this console** | Every game on this console starts with these settings. If this game had settings of its own, they are removed. |
| **For this game** | Only this game uses these settings. |
| **Restore defaults** | Removes the saved settings for this console and this game. |

A game with its own settings uses only those; later changes for the console do not reach it until
you save it "For this console" or restore the defaults.

## Runs on

**Runs on** chooses the emulator that plays a game, for this game alone or for every game of its console. Open it
with **START** on a game's card or a console page, or in the pause menu: Options → **Runs on** on pocket-ui's own
host, **Options** for RetroArch and the other emulators. (PlayStation 2, GameCube, Wii and 3DS have other Options in
the pause menu; use START on the card for them.)

The first row shows what plays the game now, for example "pocket-core · mGBA", "RetroArch · gpSP" or "Standalone ·
PPSSPP". Left/Right go through pocket-ui's own host (pocket-core, where it plays the system) and every emulator and
core ROCKNIX lists for the system. Those ROCKNIX can't start on this Pocket are left out; one chosen anyway (earlier,
in EmulationStation) shows "Not installed". The line below says what you get:

- **pocket-core**: continues at once where you left off, eight save slots, Options, shortcuts, run-ahead, fast
  switching between games and achievements.
- **RetroArch**: one save, Quit saves the moment. Starts slower; none of pocket-ui's slots, Options, run-ahead, fast
  switching or achievements.
- **Standalone**: ARMSX2, Dolphin and Azahar keep pocket-ui's Save, Load, Options and continuing. The others keep
  their own saves and settings: save in the game before you quit.

Save states belong to one emulator: after a change the game starts from its in-game save.

While RetroArch is chosen, the page also lists its settings, read the next time the game starts: **Aspect ratio**
(Core provided, 4:3, 3:2, 16:9, Square pixels, Full screen), **Integer scaling**, **Smoothing**, **Shader** (None and
the well-known CRT, LCD and sharp-scaling presets ROCKNIX has, such as crt-geom or lcd-grid-v2), **Rewind** (hold
HOME + L2), **Run-ahead** (RetroArch's own, 1 or 2 frames) and **Auto save and load** (with Off, pocket-ui can't
continue the game where you left off). Rewind and run-ahead are not shown where ROCKNIX switches them off (Nintendo 64,
PSP, Dreamcast, Saturn, 32X and a few more). The values shown are the ones in use, ROCKNIX's own until you change
them (3:2 for the Game Boy Advance). HOME + R2 turns RetroArch's fast-forward on and off.

**Save changes** works as in Options: **For this console**, **For this game** (only what differs from the console's
is kept for the game) or **Restore defaults** (pocket-ui's default emulator and ROCKNIX's settings). Nothing is kept
until you save; a choice applies the next time the game starts, even when made in the pause menu. If pocket-core
couldn't start a game (see [Systems](systems.md#when-a-game-does-not-start-on-the-own-host)), the page says so;
choose pocket-core and save to give it another try.

The choice is kept where ROCKNIX's EmulationStation keeps its "Emulator" setting, so a choice made there earlier is
used here and one made here shows up there (pocket-core as AUTO). Not offered: ROCKNIX's full list of shader presets
(one chosen in EmulationStation is kept and shown), video filters, bezels and the settings of the standalone
emulators themselves. RetroArch settings for one game need a file name ROCKNIX's scripts can read; for names with
`[`, `*`, `$` and the like the page asks to save them for the console.

PlayStation 2, GameCube, Wii and 3DS games have their own, shorter Options: see [Options for PlayStation 2, GameCube, Wii and 3DS](options-big.md).
