# Options

Pause menu → **Options**: the settings of the running game. This page describes the Options of games on
pocket-ui's own host (see [Systems](systems.md#on-pocket-uis-own-host)). PlayStation 2, GameCube, Wii and 3DS
have a shorter list of their own ([Options for PlayStation 2, GameCube, Wii and 3DS](options-big.md)); for RetroArch and the
other emulators, Options opens [Runs on](#runs-on): which emulator plays the game, and RetroArch's own settings.
The same settings for one game are on its card without starting it: [This game](#this-game).

| Entry | What it holds |
|---|---|
| **Picture** | Size, look, frame and orientation, with a live preview. |
| **Emulator** | Settings of the emulator itself, such as colours and palettes. Shown only when the emulator offers settings. |
| **Controls** | Which button does what, and the left stick. |
| **Cheats** | Cheats from the libretro cheat database. On TrimUI, opening it downloads the database once over Wi-Fi, with progress; afterwards it works offline. Other hosts show it only for matching games. See [Cheats](options-cheats.md) |
| **Shortcuts** | HOME + button combinations while you play. |
| **Speed, rumble, fan** | Fast-forward speed, rewind, what is shown over the game, rumble, performance, fan and run-ahead. |
| **Zoom** | The paused picture larger, to read small text ([Zoom](big-text.md#zoom)). |
| **Runs on** | Which emulator plays this game or this console (see [Runs on](#runs-on)). It has its own Save changes. |
| **Keep for** | Where your changes stay: **This game** or **This console**. **Restore defaults** below it undoes them. |

Move with Up/Down (the list wraps around at the ends), change a value with Left/Right (A also steps
to the next value), and go back with B. A row that opens a page ends in **›** on every row; a row that changes right
there shows its value, the selected one as **‹ value ›**. The line at the bottom explains the selected setting.
**A change applies to the running game at once** and stays: it is kept the moment you make it, as in Settings. Where
it stays is the **Keep for** row ([below](#keep-for)).

## Picture

Pause menu → Options → **Picture**.

While this page is open, a **live preview** shows the game's last picture with the settings as they
are now, in two views side by side:

- **Whole screen**: the full screen, made smaller. Shows the size, the position and the frame.
- **Actual size**: a piece from the middle of the screen at its real size. Shows scanlines, the LCD
  grid, the Old TV mask and Smooth exactly as in the game.

The preview is drawn again after every change, or says "No picture". With Large text the list shows three or four
settings at a time and scrolls, so the preview stays large.

### Screen scaling

| Choice | What it looks like |
|---|---|
| **Integer** | Integer scaling: scaled by a whole number, so every game pixel is the same size: the sharpest picture, with black borders. Game Boy 7× (1120 × 1008), Game Boy Advance 5× (1200 × 800), NES and SNES 4× (about 1170 × 896 and 1195 × 896). The whole number is the largest one whose width still fits the screen, in the shape the emulator reports. |
| **Integer, smaller** | One whole number smaller than Integer, so every pixel is still the same size and a screen frame has room: Game Boy 6× (960 × 864), Game Boy Advance 4× (960 × 640), NES and SNES 3× (about 880 × 672 and 896 × 672). |
| **Aspect** | As large as fits, in the console's own shape. Bigger, but pixels are no longer all the same size. |
| **Fullscreen** | Fills the screen and stretches the pixels. |

### Look

How the picture looks, in one row. Handhelds get **LCD grid**; TV consoles, arcade and computers get **Scanlines** and
**Old TV**. **Custom** has them all and opens the rows below. Scaling and Screen frame are rows of their own.

| Choice | What it looks like |
|---|---|
| **Clean** | Plain pixels, nothing added, all the same size at any scaling. The start of every console. |
| **Smooth** | Sharp pixels, but stair-steps on slanted edges are rounded off. |
| **Soft** | The whole picture slightly blurred, no hard pixel squares. |
| **Bloom** | A clear picture where bright parts glow softly around the edges. |
| **LCD grid** | Handhelds. Thin dark gaps between the pixels, like a handheld's screen. |
| **Scanlines** | TV consoles, arcade, computers. Rows like a TV's beam, closing up in bright parts. |
| **Old TV** | As Scanlines, plus a fine colour mask and a soft glow. Only the picture: the TV around it is a [Screen frame](options-screen.md#screen-frame). |

Choosing a Look sets Sharpness and Effect together; nothing else changes. What you saved keeps working: a combination
that matches a Look shows as that Look, any other (Soft with an effect, an LCD grid on a PlayStation) shows as
**Custom** with its rows open, and the picture is the same.

A good start: **LCD grid** on handhelds; **Old TV** with Integer scaling on NES, SNES, Mega Drive and PC Engine;
**Clean**, as they start, on PlayStation, Saturn, Nintendo 64, PSP, Dreamcast and Nintendo DS.

**LCD colours** (Game Boy Advance, shown with the LCD grid): **On** shows the colours of the Game Boy Advance's own
screen, softer and darker, as the games were made for. Choosing the LCD grid turns it on; turn it off and the Look stays.
It is left out while the emulator's own colour correction is on (Options → Emulator).

### Custom: Sharpness and Effect

Under **Custom** two rows open, a step in. **Sharpness**: **Sharp** hard pixel squares, as the console drew them;
**Crisp** all pixels the same size at any scaling (the same as Sharp at Integer); **Soft** blurs every pixel edge a
little. **Effect**: **None**, **LCD grid**, **Scanlines**, **Old TV**, **Bloom** or **Smooth**, every console offered
all of them.

### Screen frame and Screen orientation

The frame around the picture and the turned screen for upright games: see [Screen frame and Screen orientation](options-screen.md).

### Resolution

Nintendo 64, PSP, Dreamcast and PlayStation games only, on the handhelds whose graphics chip was measured for it (the Retroid Pocket Mini V2
and its Snapdragon 865 siblings, and the Snapdragon 8 Gen 2 handhelds), with pocket-ui OS and with pocket-ui for Android on the same
handhelds (Options → Picture in the game's menu): how sharp the 3D picture is drawn. **Original** is the
console's own resolution (the N64's 320 × 240, the PSP's 480 × 272, the Dreamcast's 640 × 480, the PlayStation's 320 × 240 or so);
**2x**, **3x** and **4x** draw it that many times larger, so edges and far-away things look clean on this screen instead of in steps. It costs the graphics chip
more, not the game's speed: the list offers only what ran at full speed on this handheld.

- It is the same row as the **Resolution** of PlayStation 2, GameCube, Wii and Xbox games ([their Options](options-big.md)), and
  like theirs it **takes effect the next time the game starts**: quit and start the game again to see it.
- It is kept for the console, like the other Picture settings, or for one game with [Keep for](#keep-for) → This game.
- Unless you choose one, each console starts with the sharpest resolution that keeps a margin on this handheld:

| Handheld | Nintendo 64 | PSP | Dreamcast | PlayStation |
|---|---|---|---|---|
| Retroid Pocket Mini V2, Mini, Pocket 5, Flip 2 (Snapdragon 865) | 2x (3x offered) | 2x (3x offered) | 2x | Original (2x offered) |
| Retroid Pocket 6, Nova, AYN Odin 2 family (Snapdragon 8 Gen 2) | 2x (3x offered) | 2x (3x offered) | 2x | Original (2x offered) |

  On Android, a handheld or phone with another chip has no Resolution row; the emulator keeps its own setting on the Emulator page.
  The H700, RK3566 and RK3326 handhelds have no Resolution row: their N64, PSP and Dreamcast games run on ROCKNIX's own
  emulators, which keep their settings, and the PlayStation keeps its Enhanced Resolution under [Emulator](#emulator).
- PlayStation 2x is the emulator's "Enhanced Resolution": the 3D is sharper, but some games lose a picture with it (Crash
  Bandicoot 2's pause screen shows black where the game was), so it is not where a PlayStation game starts.
- The emulator's own resolution option (Emulator page) is left out where this row is there, so there is one place for it.

### Starting values per console

Every console starts with the look that suits it on this screen:

| Consoles | Starts with |
|---|---|
| Disc and 3D consoles: PlayStation, Mega-CD, PC Engine CD, Saturn, Nintendo 64, PSP, Dreamcast | **Aspect**, **Crisp**, None, frame None |
| Nintendo DS (both screens stacked) | **Aspect**, **Crisp**, None, frame None |
| ScummVM (320 × 200 adventures) | **Aspect**, **Crisp**, None, frame None |
| Everything else, handhelds and 2D home consoles included | Integer, Sharp, None, frame None |

Out of the box no console has a screen effect or a frame: a clean picture. Pick an effect or a frame in the pause menu
(Options → Picture, with Keep for set to this game or this console) for one game or a whole console.

With **Handhelds** or **Every console** in [Settings → Playing → Screen frames](settings-playing.md#screen-frames), the handhelds or every
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

Pause menu → Options → **Controls**: what each button presses in the game you are playing, and your own changes.

- **Left stick as D-pad** (On / Off): on by default, except for PlayStation, where both sticks go to the game
  as analogue sticks. Turn it off for games that use an analogue stick.
- **D-pad as left stick** (On / Off, only for systems whose games use a stick: PlayStation, N64, PSP, Dreamcast and
  others): the D-pad moves the left stick, so Mario 64 walks on a handheld without sticks. On by default for N64, PSP
  and Dreamcast on handhelds without sticks (TrimUI Brick, RG35XX Plus, RG35XX SP, RG28XX, RG34XX). A stick you move wins.
- Then one line for each button of the Mini V2: A, B, X, Y, L1, R1, L2, R2, L3, R3, Select, Start
  and the four D-pad directions. The value is the console button it presses, by the name the
  emulator uses ("A presses A."). Left/Right choose another console button, or **None** ("X does
  nothing in this game."). L3 and R3 mean pressing the left and right stick in.
- **Hold to toggle** ([how](big-text.md#hold-to-toggle)) and **Reset controls**, which puts everything back.
- **Controller setup**, the last line: a separate layout for each controller, in menus and games
  ([Controller setup](controls-settings.md#controller-setup)).

When you play together ([Players](pause-menu.md#players)), the layout is the same for every player: a controller's buttons
do what these lines say, by where the button sits (the right face button is A). They name the buttons as printed on the Pocket, even when Settings → Controls → Buttons → [**Button layout**](settings-sound.md#button-layout) is Xbox: that changes pocket-ui's menus, not games.

HOME, and SELECT + START held to quit, always work on the real buttons, whatever you change here.

On the TrimUI Brick, **F1 and F2 are system shortcuts**. Their global actions are chosen in
Settings → Controls → **Lights & buttons**; they cannot be assigned to console buttons here
or in Controller setup. This applies to every hosted core, including mGBA.

## Cheats

Games on pocket-ui's own host get a **Cheats** page when the libretro cheat database has cheats for them: an On/Off list per
game, with a one-time Wi-Fi download on TrimUI. See [Options: Cheats](options-cheats.md).

## Shortcuts

Pause menu → Options → **Shortcuts**. On pocket-ui's own host, hold HOME and press another button
while you play:

| Row | Keys | What it does |
|---|---|---|
| **Save state** | HOME + R1 | Save to the selected slot. |
| **Load state** | HOME + L1 | Load the selected slot. |
| **Change slot** | HOME + Left / Right | Choose the previous / next slot (1 to 8). |
| **Fast-forward** | HOME + R2 | Turn fast-forward on or off. The speed is set under [Speed, rumble, fan](#speed-rumble-fan). |
| **Rewind** | Hold HOME + L2 | Rewind, for as long as you hold it. Rewind itself has to be switched on under [Speed, rumble, fan](#speed-rumble-fan). |
| **Screenshot** | HOME + A | Save a [screenshot](playing.md#screenshots) of the game. The pause menu does not open. |
| **Over the game** | HOME + Y | Switch to the next level of [Over the game](pause-menu.md#over-the-game): Off, FPS, FPS + sensors (and Pixel Path with Pixel Path in the pause menu). It lasts until you quit or change **Over the game** under Speed, rumble, fan. |
| **Zoom** | HOME + X | The paused picture larger ([Zoom](big-text.md#zoom)); B goes back into the game. Off by default. |

A short message over the picture confirms what happened, for example "Saved to slot 3",
"Slot 4 is empty" or "Screenshot saved".

While HOME is held, the game gets no buttons, so a shortcut never presses anything in the game.
L2 and R2 count as pressed from half-way down. On this page each shortcut can be switched **Off**.

## Speed, rumble, fan

Pause menu → Options → **Speed, rumble, fan** ("Settings you rarely need."). Values in bold are the defaults.

| Setting | Choices | What it does |
|---|---|---|
| **Fast-forward speed** | 2x, 3x, **4x**, 8x, As fast as possible | How fast the game runs while fast-forward is on (HOME + R2). Sound is muted meanwhile, and "FF 4x" (or the speed you chose) shows over the picture. |
| **Rewind** | **Off**, On | Remembers the last minute so you can play it backwards: hold HOME + L2. "REWIND" shows over the picture and sound is muted; at the oldest moment the picture stops. Uses some extra power. |
| **Earlier states** | Off, **On** | Keeps a state every 5 minutes of play, the last half hour, and where each session began; the pause menu's **Earlier** loads them ([Earlier](pause-menu-saves.md#earlier)). The game never waits for it. **Off by default for N64, PSP, DS, Dreamcast and Saturn**: their states are 8 to 40 MB. At most 100 MB a game and 1 GB on the SD card. |
| **Over the game** | **Off**, FPS, FPS + sensors, Pixel Path (with Pixel Path in the pause menu) | What is drawn over the game while you play. **FPS** shows frames per second and CPU load in the top left corner, for example "60 FPS   CPU 23%", updated twice a second. **FPS + sensors** adds temperature, fan and battery draw: see [the FPS line and sensors](pause-menu.md#the-fps-line-and-sensors). **Pixel Path** shows one line beside the picture instead, which turns orange when something slips: see [Pixel Path over the game](pause-menu.md#pixel-path-over-the-game). The start value is the one under Settings → Playing → [Over the game](settings-playing.md#over-the-game). |
| **Rumble** | Off, Low, **Medium**, High | How strongly games that rumble shake the console. |
| **Performance** | Battery, **Auto**, Max | **Auto**: the system decides how fast the CPU runs (until October 2026 this was called Balanced). Battery limits the CPU to half speed: enough for Game Boy and NES. Max keeps the CPU at full speed for demanding games. |
| **Fan** | Very quiet, **Quiet**, Auto, Cool | Very quiet keeps the fan off until the Pocket gets warm (60 °C); the Pocket feels a bit warmer. Quiet starts the fan later and keeps it slow. Auto keeps the system's cooling profile. Cool starts it earlier. With [Smooth fan](performance.md#smooth-fan) on, each is a smooth curve. Not shown on a handheld without a fan (the Anbernic H700 ones, the TrimUI Brick). |
| **Run-ahead** | **Off**, 1 frame, 2 frames | The game responds to your buttons 1 or 2 frames sooner (a frame is 1/60 s). Only on NES, Famicom Disk System, SNES, Game Boy, Game Boy Color, Game Boy Advance, Mega Drive, Master System, Game Gear, SG-1000, PC Engine, SuperGrafx, Atari 2600, WonderSwan and Virtual Boy; the row is not shown for other systems. See [Run-ahead](#run-ahead). |

- Rewind is off by default because keeping the history costs processor time, most on SNES.
  Loading a save clears the history. Holding HOME + L2 while Rewind is off shows "Rewind is off.
  Switch it on under Speed, rumble, fan."
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
- It is kept like the other settings: for this console or for this game ([Keep for](#keep-for)).

## Keep for

Every change in Options is kept the moment you make it; there is nothing to save. **Keep for** says where it stays:
**This game** (only this game uses it) or **This console** (every game on this console starts with it). Left, Right or A
moves between the two and keeps the values in use there at once. It starts on **This game**, or on **This console**
when the console already has settings of its own. **Restore defaults** removes the saved settings for this console and
this game; press A twice, any other button takes it back.

A game with its own settings uses only those; later changes for the console do not reach it until
you move **Keep for** to **This console**, or restore the defaults.

## Runs on

**Runs on** chooses the emulator that plays a game, for this game alone or for every game of its console. Open it
with **START** on a console page, with **START** on a game's card and then the **Runs on** row of [This game](#this-game), or in the pause menu: Options → **Runs on** on pocket-ui's own
host, **Options** for RetroArch and the other emulators. (PlayStation 2, GameCube, Wii and 3DS have other Options in
the pause menu; use START on the card for them.)

The first row shows what plays the game now, for example "pocket-core · mGBA" or "RetroArch · gpSP". Its first entry
is **Default (…)**, what plays without a choice of yours (a game: its console's; a console: pocket-core where it plays
the system, else ROCKNIX's); save it to go back. Right goes on through pocket-core and every emulator ROCKNIX lists for
the system that starts on this Pocket (one chosen earlier in EmulationStation shows "Not installed"); the list stops at
both ends. The line below says what you get:

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
them (3:2 for the Game Boy Advance). HOME + R2 turns RetroArch's fast-forward on and off. Phone games have two more rows,
**D-pad** ([their keys](systems.md#phone-games-j2me)) and **Screen size** ([Phone games](systems.md#phone-games-j2me)).
Nintendo 64 and Dreamcast on RetroArch have a **D-pad** row too: **Automatic** (the D-pad moves the left stick on a
handheld without sticks, such as the RG35XX Plus or RG28XX), **Left stick** or **D-pad**. pocket-ui writes it as
RetroArch's remap for the game, never over a remap of your own. PSP on ROCKNIX's PPSSPP gets the D-pad on the analog
nub by itself on a handheld without sticks, unless you changed those controls in PPSSPP.

**Save changes** on this page is the one place that still asks: **For this console**, **For this game** (only what differs from the console's
is kept for the game) or **Restore defaults** (pocket-ui's default emulator and ROCKNIX's settings). Nothing is kept
until you save, because a choice applies the next time the game starts, even when made in the pause menu. If pocket-core
couldn't start a game (see [Systems](systems.md#when-a-game-does-not-start-on-the-own-host)), the page says so;
choose pocket-core and save to give it another try.

The choice is kept where ROCKNIX's EmulationStation keeps its "Emulator" setting, so each sees the other's (pocket-core
as AUTO). Not offered: ROCKNIX's full shader list (one chosen in EmulationStation is kept), video filters, bezels and
the standalone emulators' own settings. RetroArch settings for one game need a file name ROCKNIX's scripts can read; for names with
`[`, `*`, `$` and the like the page asks to save them for the console.

PlayStation 2, GameCube, Wii and 3DS games have their own, shorter Options: see [Options for PlayStation 2, GameCube, Wii and 3DS](options-big.md).

## This game

Press **START** on a game's card (or its box) to set up one game without starting it. It is the pause menu's
Options for that game, reachable from the shelf: so the picture you want is there before you play, not twelve
presses into a running game.

| Row | What it holds |
|---|---|
| **Screen scaling**, **Look** (with Sharpness and Effect under Custom), **Screen frame** (with its colour or finish), **Screen orientation**, **Resolution** (N64, PSP, Dreamcast, PlayStation) | The [Picture](#picture) settings. A frame's sub-choices show up under it, as in the pause menu. |
| **Fast-forward speed**, **Rewind**, **Earlier states**, **Over the game**, **Rumble**, **Performance**, **Fan**, **Run-ahead** | The same rows as [Speed, rumble, fan](#speed-rumble-fan). |
| **Save state**, **Load state**, **Change slot**, **Fast-forward**, **Rewind**, **Screenshot**, **Over the game**, **Zoom** | The [Shortcuts](#shortcuts): which HOME + button does what. |
| **Controls** | Which button does what, and the left stick ([Controls](#controls)). The page names the buttons the standard way; the pause menu shows the console's own names. |
| **Runs on** | Which emulator plays this game ([Runs on](#runs-on)). B from there comes back here. |
| **Resolution** and the other rows of the emulator | For PlayStation 2, GameCube, Wii, 3DS and Xbox games: the rows of [their Options](options-big.md). They take effect the next time the game starts. |
| **Use the console's settings** | Only when the game has settings of its own: takes it off them, so it starts like the other games on its console. |

Left and Right change a value, B goes back to the card. Every change is kept at once as **this game's own** settings,
exactly what **Keep for → This game** keeps in the pause menu, so the game starts with it and the pause menu
shows it. The two places read and write the same file; there is nothing to save twice. Picture settings have no
running game to show them on here: you see them when the game starts, and Options → Picture in the pause menu shows
them live. A game that RetroArch plays shows its own Runs on only, with a line saying why; one that pocket-core could
play but RetroArch does now keeps its picture rows, which apply when it runs on pocket-core.
