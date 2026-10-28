# Night light

Settings → Look & feel → **Night light** makes the screen warmer and less blue in the evening, in the menus and in
games, the way a phone's night mode does. Blue light is what keeps you awake; the screen turns the colour of a lamp
instead. It is **off** unless you turn it on here. It was built and tested without a real screen in the loop; how the
colours look on the Mini V2's panel has not been checked yet.

| Row | What it does |
|---|---|
| **Night light** | **Off**, the default: the screen keeps its own colours. **On**: warmer colours all the time. **Scheduled**: warmer colours every evening, from one time to another |
| **From** | The time the screen starts to turn warmer. Only with Scheduled. The default is 21:00 |
| **Until** | The time the colours are back to normal. Only with Scheduled. The default is 07:00. A time earlier than From runs over midnight |
| **Strength** | How warm it gets, in five steps: **Slight**, **Gentle**, **Medium** (the default), **Warm** and **Very warm** |

Up and Down move between the rows; Left/Right change the selected one, and **A** moves on to the next choice (on
Strength it goes round from Very warm to Slight). The times move by half an hour. A tap on a row selects it; the
arrows next to the value are buttons for a finger. A line under the rows says what the night light is doing right
now ("On now, until 07:00."). The choice is saved at once and kept across restarts and updates.

## How it behaves

- **It fades.** The colours change over a second or two when the schedule switches on or off, and when you change the
  strength, even in the middle of a game. Nothing jumps. With [Motion](settings.md#motion) on Off, it changes at once.
- **It follows your clock.** The schedule uses the time of your time zone ([Time](settings-system.md#time)); changing the
  zone moves it. There is no sunset or sunrise: the device has no location, only a time zone, so the times are the
  ones you set. The Pocket does not check the clock all night: it wakes once, at the next switch time, and when it
  wakes up from sleep or the clock is set (for example when the network time arrives after a start).
- **It goes with your colour temperature.** [SELECT + Vol](controls-settings.md#volume-and-brightness-keys) still sets the
  colour temperature you like; the night light is added on top of it, so the screen can be warmer than either alone.
  Turn the night light off and your temperature is exactly what it was.
- **In games.** Where the screen can change its colours as a whole (the compositor takes a gamma table), the night
  light covers everything: pocket-ui, pocket-core, RetroArch and the standalone emulators. Otherwise pocket-ui tints
  its own pictures and pocket-core's games, as it does for the colour temperature, and **not** RetroArch,
  PlayStation 2, GameCube, Wii, 3DS and PortMaster games; the Night light page says so when that is the case on your
  Pocket.
- **Screenshots.** With a gamma table the warmth is added by the screen, after the picture is taken, so a screenshot
  (the web companion's picture of the screen, for one) has the true colours. With the tint it is part of the picture
  pocket-ui draws, as the colour temperature is.
- **No cost in games.** It does nothing while a game runs except at the switch times. It adds nothing to the time
  between frames or to the start of a game.

## Ideas that are not in it

No sunset and sunrise, no different strength per hour, and no row in the quick settings (they stay at seven rows).
