# The pause menu (HOME)

**Tap HOME** while playing to pause the game and open the menu. The game stays dimmed in the
background; on pocket-ui's own host the menu is there at once and the dimmed game appears behind it a moment
later. What the menu offers depends on the emulator the system uses (see
[Systems](systems.md)):

| Emulator | Menu, top to bottom |
|---|---|
| pocket-ui's own host | Continue, Save, Load, Earlier, Options, Quick settings, Quit |
| pocket-ui's own host, signed in to RetroAchievements | Continue, Save, Load, Earlier, Options, Achievements, Quick settings, Quit |
| pocket-ui's own host, for ScummVM, PICO-8 and TIC-80 (no save states) | Continue, Options, Quick settings, Quit |
| PlayStation 2 (ARMSX2), GameCube and Wii (Dolphin), 3DS (Azahar) | Continue, Save, Load, Options, Quick settings, Quit |
| Xbox (hakuX in RetroArch, experimental) | Continue, Options, Quick settings, Quit |
| RetroArch | Continue, Save, Load, Options, Quick settings, Quit |
| PortMaster ports | Continue, Options, Quick settings, Quit |
| Other emulators | Continue, Options, Quick settings, Quit |

**Up** from Continue wraps to Quit, and **Down** from Quit to Continue, like every other menu. **START** opens the
[quick settings](quick-settings.md) from any row of the menu (brightness, volume and fan without walking down to them).

At the right of the title the menu says how long the battery lasts **in this game**, for example "About 3 h 20 min
left in this game". See [Battery in this game](#battery-in-this-game).

With **Pixel Path in the pause menu** switched on in [Settings → Games → Curator](settings.md#curator), the menu of
pocket-ui's own host also has **Pixel Path**, right before Quick settings, and so do PS2, GameCube, Wii and 3DS games
whose emulator settings can be read ([below](#pixel-path-for-ps2-gamecube-wii-and-3ds)). With two or more controllers connected (the Pocket counts as one), the
menu of pocket-ui's own host also has **Players**, after Options and Achievements. A game with more than one disc on pocket-ui's
own host also has **Disc**, right before Options ([below](#changing-discs)). **Earlier** is left out where
[Earlier states](#earlier) is off and none are kept: N64, PSP, DS, Dreamcast and Saturn games start that way.

Save and Load are only there when they do something. Where the emulator saves only inside the game (ScummVM, PICO-8,
TIC-80, [other emulators](#other-emulators)), the menu has no Save or Load and says instead, beside Continue, Options
and Quit, where saving is: for example "This emulator keeps its own saves. Save in the game before you quit."

- **Continue** returns to the game. B or another tap on HOME does the same.
- **Save** saves the game's current moment into one of your save slots (1 to 8; see
  [Save slots](#save-slots-auto-and-before-load)).
- **Load** goes back to a saved moment: the one from your last quit ("Auto"), the one a Load just
  replaced ("Before load"), or one of your slots. It is greyed out and cannot be chosen while the
  selected entry holds nothing to load.
- **Earlier** (pocket-ui's own host) goes back to a moment the game kept by itself: one every 5 minutes of play, and
  where a session began. See [Earlier](#earlier).
- **Disc** (pocket-ui's own host, a game with more than one disc) changes the disc in the drive. See
  [Changing discs](#changing-discs).
- **Options** opens the settings for this game (own host: see [Options](options.md); PlayStation 2,
  GameCube, Wii and 3DS: see [Options for PlayStation 2, GameCube, Wii and 3DS](options-big.md)).
  For RetroArch and the other emulators it opens [Runs on](options.md#runs-on): which emulator plays the game,
  and RetroArch's own picture and rewind settings. On the own host, Runs on is an entry of Options.
  Its pages say what the selected row does at the bottom left and which buttons work at the bottom right: B goes
  back, A opens a page or chooses, Left and Right change a value.
- **Achievements** (own host, signed in to RetroAchievements) lists the game's achievements, locked and
  unlocked, with your progress. See [Achievements](achievements.md#the-pause-menu).
- **Players** (pocket-ui's own host, a second controller connected) shows which controller is which player and changes
  it. See [Players](#players).
- **Pixel Path** (switched on in Settings → Games → Curator) shows how the game's picture and sound reach you, stage by stage. See
  [Pixel Path](#pixel-path).
- **Quick settings** opens the panel that HOME opens on the shelf (brightness, volume, Wi-Fi, Bluetooth,
  performance, fan, power) over the paused game, which stays visible behind it, so you can see a brightness change on
  the game itself. Performance and Fan there change what every game runs with and apply to this game at once, unless
  it has its own choice in Options: then the row says "this game: …" and only the default changes. B or HOME goes back
  to the pause menu. See [Quick settings](quick-settings.md#quick-settings).
- **Quit** ends the game and takes you back to the shelf. Your in-game saves (the game's own save feature, memory
  cards) are written on every Quit. Whether Quit also keeps the moment you are at is set in
  [Settings → Playing → **Remember where I quit**](settings.md#remember-where-i-quit):
  - **On**, the default: Quit saves the moment first ("Saving and quitting…"), and the next start continues
    from there.
  - **Off**: Quit doesn't save the moment ("Quitting…"; the row says "Doesn't save where you are. In-game saves are
    kept."). The next start continues from the moment of your last Quit with the switch on, as if this session had
    not happened. If the device slept during the game, the moment saved before sleeping counts as that last Quit.
    On pocket-ui's own host, an in-game save made during this session is newer than that moment; the game then
    starts fresh from the in-game save instead, so the save is never undone. On PlayStation 2, GameCube, Wii and
    3DS the emulator closes straight away, which is also faster; the next start goes back to the moment of your
    last saving Quit even if you saved in the game since. The save itself stays on the memory card or in the
    console's storage; load it from the game's own menu if you need it.

  Without Save and Load in the menu (ScummVM, PICO-8, TIC-80, other emulators), Quit keeps no moment: it says
  "Quitting…", and the line beside it says where saving is.

  RetroArch games always save the moment on Quit, even with the switch off: ROCKNIX sets RetroArch to save on
  every exit, and RetroArch would load an older moment at the next start without checking for a newer in-game
  save. Other emulators save nothing on Quit to begin with. The [game switcher](playing.md#game-switcher) always
  saves the moment of the game it leaves, whatever the switch says, so you can switch back to it.

**SELECT** in the menu opens the [game switcher](playing.md#game-switcher); B there comes back to the menu.

Holding HOME and pressing another button is not a tap. On pocket-ui's own host these combinations
are [shortcuts](options.md#shortcuts); in RetroArch games they stay RetroArch's own shortcuts (see
[Controls](controls.md#in-a-game)).

## Changing discs

Games that come on several discs (on pocket-ui's own host: PlayStation, and other disc consoles whose emulator can
change discs) have a **Disc** row in the menu, for example "Disc 1 of 3". It is there when the game is started from an
`.m3u` playlist or a multi-disc `.pbp` file; how to make an `.m3u` is on the [Systems](systems.md#games-on-several-discs) page. A
game with one disc (a single `.cue` or `.chd`) has no Disc row.

- **Left / Right** choose a disc. The line beside the menu says which disc is in and what A does.
- **A** puts the chosen disc in and goes back to the game. On the disc that is already in, A puts in the next one, so
  when the game asks for disc 2, HOME, Down to Disc and A is all it takes.

The game sees the drive open and close, as on the console. A short note over the game says which disc is in now, for
example "Disc 2 of 3". If the emulator can't tell which disc is in, the line beside the menu only says which one A
puts in. Not tried with a real multi-disc game on a Mini V2 yet.

## Battery in this game

The line at the right of the menu's title is the time the charge left lasts at the rate **this game** used the
battery while you played it. pocket-ui measures every game while it runs on battery, once a minute, and keeps what
it found (even across restarts). It only says what it measured:

| The line | Means |
|---|---|
| "About 3 h 20 min left in this game" | This game has been measured for at least 20 minutes on battery, and the charge dropped by 3% or more in that time |
| "About 4 h left (PS2 games)" | This game hasn't been measured enough yet: the other games of its console stand in |
| "About 5 h left (all games)" | No game of this console has been measured enough either: all the games you played stand in |
| "Battery: not enough play to tell yet" | Not played long enough on battery yet |
| "Charging" | The Pocket is on the charger |

Time asleep, time on the charger and the minute in which a game starts or ends are never counted, and neither is a
sudden jump of the battery's reading. The longer you play a game, the more its newest play counts: after a change (another
[Performance](performance.md) setting, an older battery) the time follows within a few hours of play. The game's own
time left and rate also show on its [card](library.md#what-the-card-shows), and the whole Pocket's in
[Settings → Playing → Battery](settings-system.md#battery). Without a battery reading of the last minutes the line is left out.

## Players

On pocket-ui's own host you can play together, up to four players. Connect a controller (Settings → Connections →
[Bluetooth](bluetooth.md)) and it is the next player: the Pocket's own buttons are Player 1, the first
controller Player 2, and so on. When one connects or goes away during a game, a short message says so, for example
"DualSense connected — Player 2".

**Players** lists Player 1 to 4 and the controller each one has.

- **Up / Down** on any controller selects a player.
- **A on a controller** makes that controller the selected player. Whoever had that player takes the controller's old
  place. Then the next player is selected, so everyone presses A on their own controller in turn: one press each.
- **B** goes back to the pause menu.

Your choice is kept for the next games. To play on a TV with a controller as Player 1: select Player 1 and press A on
that controller; the Pocket's own buttons become Player 2. Select Player 1 and press A on the Pocket to change it back.

- A controller that goes away during a game leaves its player without a controller. When it comes back it is the same
  player again, unless another controller took that place in the meantime.
- With all four players taken, another controller waits; the page lists it under "Waiting for a free player".
- The game decides how many players it has: in a game for one player, Player 2's controller does nothing.
- Every player's buttons do what [Options → Controls](options.md#controls) sets for the game. Rumble goes to each player's own
  controller (the Pocket's motor for its own buttons), as strong as Options → Extras → Rumble says.
- Only games on pocket-ui's own host have this page. RetroArch and the other emulators number the controllers
  themselves, see [Playing together](controls.md#playing-together).

## Pixel Path

Settings → Games → Curator → **Pixel Path in the pause menu** adds it to the pause menu (it is off by default). It shows the path a game's picture and
sound take, from the emulator to the screen and the speaker, one stage per line. Each stage has a coloured dot:

| Colour | Grade | Means |
|---|---|---|
| Purple | Pristine | Pixel-perfect and native: nothing is changed |
| Green | Faithful | Close to the original; something small differs, such as a pace 0.46% faster or sound converted to another rate |
| Blue | Enhanced | More than the original, on purpose: a higher resolution, run-ahead, a screen effect, cheats |
| Orange | Compromised | Something is lost: pictures that came late, gaps in the sound, a stretched picture, blur where whole pixels would fit |

The grade at the top, next to "Pixel Path", is the lowest of the stages.

| Stage | What it shows |
|---|---|
| Core | The emulator and its version |
| Source | The console's picture: its size, how many per second, the shape of its pixels. 3D systems: the resolution the emulator renders at, against the console's own |
| Scale | How large the picture is drawn on the 1240×1080 screen: a whole number of times (5×, every pixel a 5×5 block) or not, where, and how sharp |
| Look | Screen effects, the frame around the picture, colour changes, the emulator's own colour correction |
| Display | The screen's rate and how the game keeps pace with it, and pictures that came too late |
| Latency | An estimate of the time from a button to the screen, and run-ahead |
| Audio | The sound's rate and how it is converted for the speaker, the buffers, gaps in the sound |

The line under the stages is the last 5 seconds of play: pictures per second, the emulator's time per frame, how
busy the processor was, late pictures and sound gaps. The game is paused while the menu is open, so these are the
seconds just before you opened it.

Up/Down moves between the stages, **A** explains the selected one, **Y** shows Pixel Path over the game
([Over the game](#pixel-path-over-the-game)), or hides it, and **B** goes back. The latency is worked out from how pocket-ui runs a game, not measured with a camera,
and the game's own reaction comes on top of that; it is marked "≈".

While the page is closed and Over the game isn't on Pixel Path, it does nothing at all.

### Pixel Path for PS2, GameCube, Wii and 3DS

ARMSX2 (PS2), Dolphin (GameCube, Wii) and Azahar (3DS) draw the picture and play the sound themselves and tell
pocket-ui nothing while they run. For them Pixel Path reads the emulator's own settings files instead, the values the
game runs with (what you changed under [Options](options-big.md) included), and shows them stage
by stage:

| Stage | What it shows |
|---|---|
| Core | The emulator and how it draws (Vulkan, OpenGL) |
| Source | The internal resolution: 1×, 2×, … the console's, and the size that makes for GameCube, Wii (640×528 at 1×) and 3DS (400×240 at 1×); a widescreen patch or hack |
| Scale | The aspect ratio (4:3, 16:9, Auto, Stretch) or the 3DS screen layout, and how the picture is smoothed on its way to the screen |
| Look | What the emulator adds: FXAA, sharpening, a TV shader, anti-aliasing, texture filters such as xBRZ, or "Nothing added" |
| Display | The screen (1240×1080 at 60 Hz) and whether the emulator waits for it (VSync) |
| Latency | "Not measured": pocket-ui can't time a game it doesn't run, so there is no number rather than a guess |
| Audio | The emulator's sound output and its buffer, on its way to the speaker through PipeWire |

Only stages a setting alone decides get a grade: a resolution above the console's is Enhanced, below it Compromised, the
console's own Pristine; a picture stretched to the screen is Compromised, the game's shape kept is Faithful; an effect
added is Enhanced, nothing added Pristine. The others show their values with a grey dot and no grade, and the top
says **Partly graded**. The line under the stages names the files the values came from. **Y** (Pixel Path over the game) is
there only for games on pocket-ui's own host.

If an emulator's settings can't be read (the file is missing or isn't one Pixel Path understands), the menu leaves
Pixel Path out for that game rather than show seven empty stages. RetroArch and the other emulators have no Pixel Path
in their menu.

## Over the game

One setting decides what is drawn over a game while you play, besides the short messages:

| Level | What you see |
|---|---|
| **Off** | Nothing. The default |
| **FPS** | Frames per second and CPU load in the top left corner: [the FPS line](#the-fps-line-and-sensors) |
| **FPS + sensors** | The same line with the temperature, the fan and the battery draw added |
| **Pixel Path** | One quiet line beside the picture, graded in colour, which turns orange when something slips: [Pixel Path over the game](#pixel-path-over-the-game) |

Only one of them shows at a time: Pixel Path's line has the frame rate too.

**Where to set it.**

- Settings → Playing → **Over the game**: what every game starts with (Left/Right or A). See
  [Settings](settings.md#over-the-game).
- Options → Extras → **Over the game**: for the running game, until you quit.
  [Save changes](options.md#save-changes) keeps it for this console or this game, like every option, and then it
  wins over Settings.
- **HOME + Y** while you play switches to the next level at once (Off, FPS, FPS + sensors, Pixel Path, then Off again)
  and says which, for example "Over the game: FPS". It lasts until you quit or change Over the game in Options. The
  shortcut can be switched off under [Shortcuts](options.md#shortcuts).

With **Off** nothing is drawn and nothing is measured for it. Over the game works in the games pocket-core runs. The
emulators that run on their own (PS2, GameCube, Wii, 3DS) draw their own picture and have their own Show FPS in their
[Options](options-big.md); pocket-ui puts nothing over them.

If you used the older settings: Show FPS **On** is now **FPS**, **Sensors** is **FPS + sensors**, and the Curator's **Pixel Path over every game**
is now Settings → Playing → Over the game on **Pixel Path**. Your choices
were carried over.

### The FPS line and sensors

**FPS** shows frames per second and CPU load in the top left corner, for example "60 FPS   CPU 23%", updated twice a
second. 100% is one processor core. While fast-forwarding it adds the frames the emulator runs, for example
"60 FPS   EMU 239   CPU 50%". **FPS + sensors** adds the Pocket's state to that line, for example
"60 FPS   CPU 23%   62 °C   Fan 45%   4.8 W":

- the hottest temperature the Pocket measures, in whole degrees;
- the fan, as its speed in percent, or "Fan off" while it stands still;
- what the battery gives out in watts, to a tenth, or "charging" while it is charging.

In German the words are German ("Lüfter 45 %", "lädt"); the numbers and units stay as they are.
The numbers are read twice a second, beside the game and not in its way. Whatever a device doesn't have, because there is
no fan or no battery gauge, is simply left out of the line.

### Pixel Path over the game

On **Pixel Path**, Over the game shows one quiet line next to the picture, after a coloured dot, for example
"mGBA · 5× sharp · 60.00 fps · ≈34 ms · 48 kHz". It doesn't need Pixel Path in the pause menu: that switch is for the
page above alone.

The dot is the overall grade, in its colour. Then the emulator, how the picture is scaled (a 3D system drawing above
the console's resolution says so: "2× res, crisp"), the pictures per second (a 50 Hz game: the game's own rate), the
latency estimate (with run-ahead: "≈19 ms, 1 ahead") and the sound's rate. The numbers cover the last 5 seconds of
play and are updated twice a second.

The moment something slips, the dot turns orange and the line says what, in orange: "3 late" (pictures that came
too late, a stutter), "2 underruns" (short gaps in the sound), "run-ahead off" (run-ahead switched itself off because
the game couldn't keep up; this stays for 5 seconds). Once the last 5 seconds are clean again, the line calms down.

**Where it is.** Never over the picture when there is space beside it: in the band above the picture (Game Boy
Advance, SNES and most consoles at Integer, 3D systems at Aspect), else below it, else at the side (Nintendo DS).
When the picture fills the screen, or the bands are too narrow for it (Game Boy at 7×), it is small and see-through
in the top left corner. It looks the same in dark and light appearance, like every message over a game.

**Y** on the [Pixel Path](#pixel-path) page puts Over the game on Pixel Path for the running game, or takes it back to
what it was, as a change in Options does.

## When your progress can't be saved

On pocket-ui's own host the game's own save (the in-game save, `<game>.srm`) and, for games with a clock, the clock
(`<game>.rtc`, see [below](#games-with-a-clock)) are written to the SD card every 10 seconds, and again when you quit or
the Pocket goes to sleep. When the SD card is full, has turned read-only or is failing, that can't go through:

- A line stays at the bottom of the game: "Your progress couldn't be saved to the SD card. Free some space; it will try
  again." (or "the SD card can't be written to right now"). You can keep playing: the save is tried again every 10
  seconds, and the line goes away as soon as one gets through.
- The save already on the SD card stays as it was. A write that fails never damages it.
- The pause menu shows the same line under the game's name.
- **Quit** asks first: "Couldn't save. Quit anyway?", and says what is at stake. **Keep playing** (or A on the first
  row) goes back to the game; B goes back to the menu; **Quit anyway** quits, and tries the save once more on the way
  out. What you did since the last save that reached the SD card is lost then. Free some space first if you can
  ("The SD card is full" in [Troubleshooting](troubleshooting.md#questions)).
- Sleep, the power-off at a nearly empty battery and the game switcher don't ask: they try the save once and go on, so
  the Pocket never hangs on a full SD card. If the last save failed, the shelf says so once after the game.
- When Quit or sleep could still save the moment (Auto) although the in-game save failed, the next start continues from
  that moment, and the in-game save in it goes to the SD card as soon as there is space.

### Games with a clock

Some cartridges have a clock: Pokémon Gold, Silver and Crystal, for example. On pocket-ui's own host that clock is kept
in `<game>.rtc` beside the in-game save, the file RetroArch uses too, so the time in the game goes on after you quit and
start again, even without the moment saved by Quit. [Sync](sync.md) shares it with the save. Game Boy Advance games
with a clock (Pokémon Ruby, Sapphire and Emerald) read the Pocket's own time instead; if theirs is off, check the time
zone in [Settings → System → Time](settings-system.md#system).

## Save slots, Auto and Before load

Every game has **eight save slots** (the 3DS has one, see below) and two states of its own that never
touch them:

| | Written by | Where it is |
|---|---|---|
| **Slots 1 to 8** | **Save**, and only Save | Save and Load |
| **Auto** | Quit and sleep (not Quit with **Remember where I quit** off) | Load, first in the list |
| **Before load** | every Load, just before it | Load, after Auto, until you quit |

With Save selected, press **Left/Right** to choose the slot (the number appears next to it, as
"‹ 3 ›"). With Load selected, **Left/Right** walks through what can be loaded: Auto, Before load
(only while there is one), then slots 1 to 8. Load starts on Auto when there is one.

- **Auto** is "Auto · saved when you quit": the moment you left the game, which the next start
  continues from (see [continuing](playing.md#continuing-where-you-left-off)). It is not one of your slots.
  Sleeping writes it too.
- **Before load** keeps what you were playing just before a Load replaced it, so a Load you did
  not mean can be undone: Load, then Load again and pick "Before load". It lasts until you quit.
  A Quit after a Load saves the game as it is then (Auto); Before load is not kept after that.

On the right you see:

- the preview of the entry (the picture at the moment it was saved), or "Empty" for an empty slot
- under it, a slim row of eight small blocks in the plastic of what the game saved to back then (the PlayStation's
  grey card, the PS2's black one, the Dreamcast's white VMU, the GameCube's, a cartridge's own colour): a block with a
  save shows the game's cover, the ring marks the slot Save or Load is on (no ring while Load is on Auto or Before
  load). With Settings → Look → Cozy extras → **Memory card in the pause menu** on (it is off at first), the
  memory card or cartridge itself stands there beside its numbered blocks, as on the game's info page
- above the keys: "Slot 3 · replaces the save from today 14:02" (on Save), "Slot 3 · saved
  yesterday 21:10" (on Load), "Slot 3 · empty", "Auto · saved when you quit · today 14:02" or
  "Before load · today 14:05" (real times)

On pocket-ui's own host the slot you used last is remembered for each game, and the
[shortcuts](options.md#shortcuts) use the same slot.

### Where the states are, for players coming from Android

The slot numbers are the ones the emulators use, so a state saved on the Android version is the
same slot here. All states are in `roms/savestates/<system>/` on the SD card. The file share leaves that folder out;
reach it with [Remote access (SSH)](settings-system.md#remote-access-ssh) or keep it in step with [Sync](sync.md).

| | Slots 1 to 8 | Auto | Before load |
|---|---|---|---|
| pocket-ui's own host | `<game>.pstate` (slot 1), `.pstate1` to `.pstate7` | `<game>.pstate.auto` | `<game>.pstate.undo` |
| PlayStation 2 (ARMSX2) | `<serial> (<CRC>).01.p2s` to `.08.p2s`, ARMSX2's own slots 1 to 8 | `….99.p2s` (slot 99, not in ARMSX2's menus) | `….98.p2s` |
| GameCube and Wii (Dolphin) | `<game file>.s01` to `.s08`, Dolphin's own slots 1 to 8 | `<game file>.pocket-auto` | `<game file>.pocket-undo` |
| 3DS (Azahar) | `<title id>.00.cst`, Azahar's quick slot (**one slot only**) | `<game file>.auto.cst` | `<game file>.undo.cst` |

Each state has a `.png` beside it, its preview. Slots 9 and 10 of ARMSX2 and Dolphin are left alone.

Older versions wrote the state of a Quit into ARMSX2's slot 0 and Dolphin's slot 1 (and Save into
Dolphin's slot 0). The first start after the update moves them: a state that pocket-ui wrote (it has
a preview picture beside it, which the emulators don't write) goes to Auto, Dolphin's old Save
becomes slot 1. A state without such a picture is yours and stays where it is; nothing is overwritten.

### The 3DS has one slot

Azahar has a quick slot and no shortcut for any other, so Save and Load use that one slot ("The
only slot") and the dots are left out. Auto and Before load work as above.

## Earlier

On pocket-ui's own host the game also keeps moments without you saving them. **Earlier**, right after Load, lists
them, newest first:

- **One every 5 minutes of play**, the last six: half an hour back. Only play counts: time in this menu, asleep or with
  the screen off does not.
- **Where a session began**: a game that continues from Auto keeps that moment too, for the last three sessions.

With Earlier selected, **Left/Right** walk through them; the chip says when ("‹ 12 min ›" ago, "‹ 14:02 ›" for a
session start today), the picture shows the moment and the line under it what it is: "Earlier · 12 min ago",
"Earlier · session start · today 14:02". **A** loads it. As with every Load,
what you were playing becomes **Before load** first, so Load → Before load takes you back.

- With none yet the window says "None yet" and the line "Earlier · none yet · one every 5 min of play".
- A state that could not be written whole (the SD card was full) says "damaged, can't be loaded"; A does nothing there.
- A state that was replaced while the menu was open is not loaded in its place: "That earlier state was just
  replaced. Pick it again."

Turn it on or off under Options → Extras → **Earlier states** ([Options](options.md#extras)), for the console or the game. It
starts On, but Off for N64, PSP, DS, Dreamcast and Saturn: their states are 8 to 40 MB, so a handful would fill the space a
game has. Off, the row stays while there are earlier states of the game to load.

**The game never waits for it.** Most consoles' states are taken between two frames, in about a millisecond; the
picture and the file are written on the side. A console whose state takes longer than a quarter of a frame, and
every 3D one, is not interrupted for it: its state is taken a second after you open this menu, or kept from the
moment the Pocket goes to sleep. For those, the states are where you paused, at least 5 minutes of play apart.

**Space**: at most 100 MB a game and 1 GB on the SD card, the oldest go first; while less than 1 GB is free, none is
written. [Storage](storage.md) shows what they take, and its clean-up of old automatic states deletes them too.
[Sync](sync.md) leaves them out. The files are `<game>.pstate.earlier1` to `.earlier6` and `.pstate.started1` to
`.started3`, each with its `.png`, beside the other states.

## Saving with RetroArch

RetroArch has one save per game in the menu. On the right you see its preview and when it was made,
for example "Replaces the save from today 14:02" or "Saved today 14:02". With no save yet, the preview
area says "Empty slot".

On PlayStation 2, GameCube, Wii and 3DS, Save and Load close the menu and let the game run, then the
emulator saves or loads (a slot far up in Dolphin takes a few seconds: its slots are reached with
its own hotkeys). The preview of a save is the picture the menu was opened on.

A PlayStation 2 game has no Save and Load in the first moments after it starts: ARMSX2 names its saves after the
game, and until it has said which game runs there is nothing to save to. The menu says "Save and Load aren't ready
yet. Save in the game before you quit." Open the menu again once the game runs.

## Xbox

Xbox games (`roms/xbox`, `.iso`) run in RetroArch with the hakuX core, which is experimental. The menu has Continue, Options
(resolution and screen scaling, applied at the next start),
Quick settings and Quit, and says "No save states for Xbox games yet: save in the game before you quit." There is no
Save, Load or Earlier: the core cannot save states yet. The Xbox's own saves in the game work, and they stay on the Pocket
(not in `roms/xbox`). Quit closes the game cleanly; if it hangs, pocket-ui stops it after about 20 seconds. The Save and Load rows
appear by themselves when a later core can save states. More in [Systems](systems.md#xbox).

## PortMaster ports

Quit closes only this launch and its helpers, including helpers in another process group. A port started next keeps its controls.

HOME opens the menu over a running port, and **Continue** gives the port its buttons back. A port saves by itself, so
there is no Save or Load: the menu says "This game saves by itself. Save in the game before you quit."

- **Options** is the port's own **Performance & fan** ([PortMaster ports](performance.md#portmaster-ports)). What you
  change there applies from the port's next start.
- **Quit** closes the port's window as its own Exit would, then stops everything the port's start script launched (the
  game, the button translator, helpers), within about five seconds. Anything not saved in the game is lost.

## Other emulators

Other emulators (for example for Xbox) keep their own saves. Their menu has no Save or Load and says, beside
Continue, Options and Quit: "This emulator keeps its own saves. Save in the game before you quit." Quit asks the
emulator to close; if it has not closed after about five seconds, pocket-ui stops it, and anything unsaved is lost.

**Wii U, PlayStation 3 and PlayStation Vita (experimental, AYN Odin 2 family and the other SM8550 handhelds).** Cemu, RPCS3 and Vita3K have no
save states, so their menu is the same: Continue, Options, Quick settings, Quit, with the line above. HOME opens the
menu over the game, and Continue goes back to it. Quit closes the emulator's window, as its own Exit would, and gives
it about eight seconds to finish; if it hangs, pocket-ui stops it (and, after five more seconds, ends it), and
anything not saved in the game is lost. While one of them runs, the pad belongs to the game; only HOME is pocket-ui's.
Performance in Quick settings and Settings → Playing → Performance & fan applies as for the other big systems.

ScummVM, PICO-8 and TIC-80 on pocket-ui's own host have no save states either. Their menu has no Save or Load and
says where their saving is, for example "No save states for ScummVM: START opens its own Save and Load".
