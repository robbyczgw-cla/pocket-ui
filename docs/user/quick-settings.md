# Quick settings, sleep and power

## Quick settings

Tap **HOME** on the shelf, or choose **Quick settings** in a game's pause menu (the game stays paused and
visible behind the panel). Rows, top to bottom:

| Row | Left/Right and A |
|---|---|
| **Brightness** | Left/Right change it |
| **Volume** | Left/Right change it. Muted sound says so: "5%, muted". The small text beside it says where the sound goes: Speaker, Headphones, Bluetooth: and the device's name, USB audio or HDMI ([Where the sound goes](#where-the-sound-goes)) |
| **Wi-Fi** | A switches it on or off, by what the radio is doing right now (not by what the row showed a moment ago). The row says **Off** when the radio is off, however that came about, and the same on Settings → Wi-Fi. If Wi-Fi is on in your settings but the system blocked the radio while starting, pocket-ui switches it on once by itself; a Wi-Fi you switched off stays off. If the switch does not work, a short line says "Couldn't turn Wi-Fi on. Try again." and the row keeps showing what the radio really does. Switching Wi-Fi off ends a remote login |
| **Bluetooth** | A switches it on or off. It takes a few seconds. To pair a controller, see [Bluetooth](bluetooth.md) |
| **Performance** | Left/Right: Battery, Auto, Max; A goes on to the next. What games run with |
| **Fan** | Left/Right: Very quiet, Quiet, Auto, Cool; A goes on to the next. On the shelf the fan in the menus, over a game the fan in games |
| **Power…** | A opens the power menu: Sleep, Restart, Power off, Restart pocket-ui (and Restart to boot menu where the device can) |

The time, the Wi-Fi symbol and the battery level are at the top right, as on Home. B or HOME closes the panel; opened from the pause menu, it
goes back to the pause menu. In a game, a HOME tap opens the pause menu, not this panel. The rows can also be touched:
a row selects it, the selected one is A, and the arrows round a value are Left and Right.

### Where the sound goes

The small text beside the Volume bar is where the system plays sound **right now**, asked when the panel opens and
again every couple of seconds while it is open. It is read in the background, so the panel never waits for it: a
"…" stands there for a moment, and "Output unknown" if the system does not say.

- **Speaker**: the Pocket's own speakers.
- **Headphones**: the wired jack is the system's current output.
- **Bluetooth: name**: Bluetooth headphones or a speaker, with the name they give themselves (long names are cut short;
  the full one is on the [Bluetooth](bluetooth.md) page). Just "Bluetooth" if the name can't be read.
- **USB audio** and **HDMI**: a USB sound device or a screen.

This is a read-only report; pocket-ui does not switch the output. **If you plugged in wired headphones and the row
still says Speaker**, the system has not switched to them. ROCKNIX's own release notes for its Retroid Pocket builds
(20241120) list "no headphone switching" among kernel-level bugs; whether it still applies to your Mini V2 and
ROCKNIX version has not been checked here, so treat it as something that **may** happen. What can help: unplug and plug
the headphones in again, or sleep the Pocket and wake it. If the row says Headphones and you hear nothing, check the
Volume level above it, which is shared.

**Performance and Fan** are the same choices as Settings → Playing → [Performance & fan](performance.md), saved at once:

- **Performance** is always **Games: performance**, what every game runs with unless you chose one for it. Battery:
  about half speed, the battery lasts longer. Auto: the system decides; for PS2, GameCube and 3DS that is full speed.
  Max: full speed all the time. The small text left of the value says what it is for: "for games" on the shelf;
  over a game, a choice of the game's own shows there in orange.
- **Fan** is **Fan outside games** on the shelf ("in menus" beside the value) and **Games: fan** over a game. Auto is
  always the system's own cooling profile.
- Over a paused game a change applies to that game at once: on pocket-ui's own host and for PS2, GameCube, Wii and
  3DS, the processor; the fan for every game while [Smooth fan](performance.md#smooth-fan) is on. A PortMaster port
  takes a new Performance from its next start. RetroArch games keep the system's speed ("this game: the system's").
- A game, its console or a port with its **own** choice keeps it. The row then shows it beside the value, for example
  "this game: Max", with "Set for this game in Options (Max): this changes the default only." under the list; a
  change moves only the default for the other games, and a message repeats what the game keeps.
- On a device without a fan there is no Fan row. Where pocket-ui can't set the processor's speed, Performance says
  "Not available here".

## Sleep and power

> **Status:** waking from deep sleep has been timed on a Mini V2: the screen and the game are back well under a
> second. Deep sleep uses about 5% of the battery an hour (measured over 5½ hours on a Mini V2). **Power off**,
> **Restart**, **Restart pocket-ui**, **Switch off after sleeping** and the power button set to **Screen off** have
> not been tried on a real device yet. Save your game before you put the device away until this note is gone.

Sleep works in two steps, each with its own time in Settings → System → **Sleep & power**
([below](#sleep--power-settings); details for developers: [SLEEP.md](../SLEEP.md)):

- **Light sleep** (the screen off) starts after a while without a button press: 2 minutes by default. The screen
  goes dark and the game pauses. Any button except the volume keys wakes it and the game goes on.
- **Deep sleep** starts 2 minutes into light sleep, or at once when you press the power button, or when you
  choose Sleep in the power menu (also from **Power…** in the quick settings). pocket-ui first saves the game, then the system suspends. Press the power
  button to wake it; the game continues where it was. The screen and the game come back first; the sound can follow
  a moment later. For about half a second after the screen comes on, the shelf and the menus take no button, so a press in
  a bag doesn't start a game; a running game gets your buttons at once.
- **Power off** (in the power menu) saves and closes the running game first. The next start of pocket-ui opens
  that game again.
- **Switch off after sleeping.** Off unless you turn it on: a sleeping Pocket stays asleep until you wake it, as on
  other handhelds. Deep sleep still uses about 5% of the battery an hour, so a full battery would be empty after a day
  in a bag. If you would rather it switched itself off, Settings → System → **Sleep & power** sets the time: 1 h, 3 h
  or 8 h. After that long asleep (3 hours is about 14% of the battery) the Pocket saves the game and switches itself
  off, as Power off does, with the screen dark. When you switch it on again, the game opens where you left it,
  whatever **On start** says, and a line says once "Switched off after 3 h asleep to save the battery." On the charger
  it never switches off; it stays asleep. Waking it before the time is up works as always. With **Off**, no clock
  alarm is ever set for this.
- **Low battery.** At 5% (on battery) a note says so, on the shelf and over a game on pocket-ui's own host. At 2% the Pocket saves the
  running game and switches itself off, as Power off does, so the game is not lost when the battery runs out, even
  when it lies dark in light sleep. On the charger, none of this happens.
- A system sleep block also keeps automatic sleep, the Sleep menu and lid sleep from suspending. The screen
  stays dark; press a button to wake it. The physical power button can still put it to sleep.
- If the system does not manage to suspend, the screen stays dark and it tries again after 4, 8, 16 and then every
  30 minutes.

If the device sleeps while a RetroArch game runs, that game stays in memory; it is not saved first.

On the experimental Pocket 6 and Nova, Power short press turns the screen off; a playing game can keep running.
Any button turns it back on. ROCKNIX sleep is unavailable there. The Sleep & power page offers an optional
Switch off after timer (Off by default), counted while the screen is off on battery.

### Sleep & power settings

Settings → System → **Sleep & power**. Up/Down choose a row, Left/Right change it, A goes on to its next choice; the
line under the rows says what the selected choice does. A change applies at once. The last two rows are under the
heading **Power button**.

| Row | Choices (default in bold) |
|---|---|
| **Screen off after** | Never, 30 s, 1 min, **2 min**, 5 min without a button press before the screen goes dark (light sleep). In a game the game pauses. Never: the screen stays on until you press the power button |
| **Sleep after** | Never, 1 min, **2 min**, 5 min, 15 min with the screen off by itself before the Pocket goes to sleep (deep sleep, the game saved first); a system sleep block keeps it in light sleep. Never: it stays awake with the screen dark until you press a button, which uses the battery; at 2% it still saves and switches off |
| **Switch off after sleeping** | **Off**, 1 h, 3 h, 8 h asleep on battery before the Pocket saves the game and switches off (above) |
| **Short press** | **Sleep**, Screen off: what a short press of the power button does |
| **Hold** | **Power menu**, Sleep: what holding the power button for about a second does |

- **Short press: Screen off.** Only the screen goes dark: a game goes on and you hear it, for a game with music you
  want to keep listening to. Any button except the volume keys turns the screen back on. In games on pocket-ui's own
  host (pocket-core) nothing is drawn while the screen is off, and the buttons don't reach the game, so a press in a
  pocket does nothing in it; the button that turns the screen on doesn't reach the game either. RetroArch and the
  standalone emulators keep drawing and get every press: wake them with the power button. A game left running like
  this never goes to sleep by itself, so it uses the battery until you come back (at 2% it is saved and the Pocket
  switches off). On the shelf, or with the pause menu open, Screen off is the same as the screen going off by itself,
  and Sleep after counts.
- **Hold: Sleep.** Holding the power button also puts the Pocket to sleep, once you let go. The power menu is then still in the quick
  settings, under **Power…**.
- **Retroid Pocket Flip 2** (experimental): closing the lid does what the short press does, Sleep or Screen off;
  opening it wakes the screen.
- Settings from before this page (the times used to be in Settings → Playing → Battery) are kept: 3 minutes there is
  3 min under Screen off after until you change it.
