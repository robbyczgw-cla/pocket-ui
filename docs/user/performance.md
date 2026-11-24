# Performance and fan

Settings → Playing → **Performance & fan** sets your default CPU speed for games and how your fan behaves in menus and
gameplay. Highlight a row with Up/Down and switch with Left/Right (or A); changes save immediately. The two rows you'll
use most also live in [Quick settings](quick-settings.md#quick-settings) (HOME on the shelf or in the pause menu):
**Performance** maps to Games: performance, while **Fan** controls Fan outside games on the shelf and Games: fan during
a game.

| Row | Choices |
|---|---|
| **Games: performance** | Battery, **Auto**, Max |
| **Games: fan** | Very quiet, **Quiet**, Auto, Cool |
| **Fan outside games** | **Auto**, Very quiet, Quiet, Cool |
| **Smooth fan** | **On**, Off |

On fanless handhelds (such as the experimental Anbernic H700 models), the fan rows show "No fan" and only CPU
performance is editable. On handhelds where ROCKNIX manages the fan directly (including the experimental Classic Rev Q),
they show "ROCKNIX". Read-only rows omit change arrows and show only B Back.

- **Games: performance:** sets how fast the CPU runs during games. **Auto** *(called Balanced before October 2026; old
  saved choices still work)* scales speed dynamically, and runs full speed for PS2, GameCube, Wii and 3DS since they need
  it. **Battery** holds the CPU around half speed, plenty for Game Boy and NES while stretching battery life. **Max**
  locks top clocks for heavy titles, running warmer and drawing more battery.
- **Games: fan:** controls how aggressively the fan spins in games. **Quiet** (default) kicks in at 52 °C and stays low
  (in Burnout on PS2 it spun ~20% slower than Auto while holding 60 to 70 °C with zero throttling). **Very quiet** stays
  silent up to 60 °C, spins as gently as possible and stops below 55 °C: warmer in the hand, nearly silent, and still
  ramps to full speed at 78 °C (opt-in so you can try it yourself). **Auto** uses the system's stock cooling curve.
  **Cool** starts at 40 °C and spins faster for cooler hands and more audible airflow. Existing saved fan choices (even
  Auto) are never overwritten; only fresh installs or older configs without this row default to Quiet.
- **Fan outside games:** same four profiles for browsing your shelf, lists and Settings.
- **Smooth fan:** **On** glides fan speed smoothly with temperature; once spinning, it stays at a soft hum instead of
  cycling on and off or jumping suddenly. **Off** uses the OS's stepped fan curve.

Bold values are defaults. Auto always respects the OS profile: untouched with Smooth fan off, or smoothed into a gradual
curve with Smooth fan on.

## Smooth fan

*Tested on a Mini V2 for eight minutes under steady single-core load: the fan spun up once instead of cycling twice on
stock control. Minimum RPM, the 78 °C ceiling and real-world acoustics are still to be checked.*

pocket-ui takes over fan control and ramps speed smoothly over seconds instead of abrupt steps. Very quiet, Quiet, Auto
and Cool map to four curves starting at 60 °C, 52 °C, 46 °C and 40 °C respectively (Auto follows your OS cooling
profile: quiet, moderate or aggressive), all hitting 100% by 78 °C. Once spinning, the fan waits until temps stay clearly
lower for most of a minute before stopping. Hardware safety stays ironclad:

- Hit 78 °C on any CPU or GPU sensor and the fan jumps to full speed immediately, regardless of profile.
- OS thermal throttling and emergency thermal shutdown stay completely untouched.
- During sleep, power off, or if pocket-ui exits or crashes, the OS's own fan control takes back over within seconds.

With Smooth fan enabled, **Games: fan** covers every emulator: pocket-core, RetroArch, PS2, GameCube, Wii, 3DS and
PortMaster ports (unless you set a per-game ROCKNIX cooling profile, which wins for that game).

## Which setting a game uses

Priority order from highest to lowest: your per-game setting in the pause menu → your per-console setting →
**Games: performance** and **Games: fan** here → the OS default. Set your baseline here, then override individual
consoles or games via Pause menu → Options → Speed, rumble, fan (with Keep for); **Restore defaults** falls back to
this page. See [Options](options.md#speed-rumble-fan).

**Games: performance** applies to pocket-ui's own player ([Systems](systems.md#on-pocket-uis-own-host)), PortMaster
ports (below) and PS2/GameCube/Wii/3DS games that don't have a per-game Performance override (Battery stays Battery;
Auto and Max both run Max). RetroArch games keep the OS default. **Games: fan** covers the same games with Smooth fan
off, and every game with Smooth fan on.

**Fan outside games** only applies while awake on the menus. With Smooth fan off, launching a game hands cooling back to
the system. In light or deep sleep, and whenever pocket-ui exits or crashes, stock OS cooling resumes automatically.

## PortMaster ports

Each PortMaster port has its own **Performance & fan** screen: press X on the port under Ports for its
[card](library.md#opening-a-card) and press **START**, or open **HOME → Options** while playing
([pause menu](pause-menu.md#portmaster-ports)).

| Row | Choices |
|---|---|
| **Performance** | Battery, Auto, Max |
| **Fan** | Very quiet, Quiet, Auto, Cool |
| **Use the defaults** | Clears the port's override (shown only when customised) |

- Changes save right away and apply on the port's **next launch**. Any row marked "(default)" inherits
  **Games: performance** or **Games: fan** from this page.
- **Battery** caps CPU speeds around half for cooler, quieter, longer sessions (heavy ports may stutter). **Auto** uses
  the OS default. **Max** locks big CPU cores and GPU at full speed.
- **Fan** requires Smooth fan On and overrides any ROCKNIX per-port cooling profile; with Smooth fan Off, ports use OS
  cooling.
- Everything resets the instant the port closes (even after a crash), so a port's Battery setting never leaks into your
  next game.

**Busy-core detection.** Some ports (including many GameMaker titles like Deltarune) spin one CPU core at 100% just
waiting for the next frame. pocket-ui checks once during a port's first few minutes; if it spots a busy loop, the game
card shows "Busy core? Press START", and Performance & fan offers **Use Battery for this port**: "This port keeps one
core busy all the time. Battery performance keeps it cooler and quieter; if it stutters, go back to Auto." Press A to
apply; pocket-ui never changes it behind your back, and the hint disappears once you've opened the page.

## Now

The live readout at the bottom refreshes every second: peak temperature, fan speed (RPM and drive percentage), who's
controlling it ("smooth, Quiet" / "full speed, hot" from 78 °C / stock start temperature), live clocks for each CPU
cluster (little, big, prime) and GPU, plus battery draw ("On the charger" when plugged in). Read-only status.

On Anbernic H700 handhelds, **Battery** leaves normal game CPU clocks untouched; power saving applies to menus and idle
only so games don't lose speed. Volume pop-ups appear on the shelf and in Quick settings, staying hidden over running
H700 games to protect their frame budget.
