# Performance and fan

Settings → Playing → **Performance & fan** sets what games start with and what the fan does while you are in the menus. Up/Down
pick a row, Left/Right (or A) change it. A change is saved at once. The two most used are also in
[Quick settings](quick-settings.md#quick-settings) (HOME on the shelf, or the pause menu): **Performance** there is
Games: performance, **Fan** is Fan outside games on the shelf and Games: fan over a game.

| Row | Choices |
|---|---|
| **Games: performance** | Battery, **Auto**, Max |
| **Games: fan** | Very quiet, **Quiet**, Auto, Cool |
| **Fan outside games** | **Auto**, Very quiet, Quiet, Cool |
| **Smooth fan** | **On**, Off |

On a handheld with no fan (the experimental Anbernic RG28XX and RG35XX SP) the fan rows read "No fan" and only the processor speed can be changed.

- **Games: performance** is how fast the processor may run while you play. **Auto**: the system decides; it speeds up
  and slows down as the game needs it. For PlayStation 2, GameCube, Wii and 3DS games that is full speed (they need
  it). Until October 2026 the pause menu called Auto "Balanced"; settings saved then still apply. **Battery** keeps
  the processor at about half speed, which is enough for Game Boy and NES, and the battery lasts longer. **Max** runs
  it at full speed all the time, for demanding games; it uses more battery and gets warmer.
- **Games: fan** is how early the fan starts while you play. **Quiet**, the default, starts the fan at 52 °C and
  keeps it slow: the Pocket gets a little warmer, the fan stays quiet (measured with Burnout on the PS2 emulator: the
  fan ran at about a fifth less than on Auto, the processor stayed at 60–70 °C and never slowed down). **Very quiet**
  keeps the fan off until the Pocket gets warm (60 °C), then runs it as gently as possible and stops it again below
  55 °C: the Pocket feels a bit warmer, and you rarely hear the fan. It still speeds up in time (full speed from
  78 °C), but it is new and not chosen for you; try it and see. **Auto**
  follows the system's own cooling profile. **Cool** starts it at 40 °C and turns it faster: cooler to hold, and you
  hear it more. Where this page was saved before, the fan saved then stays (Auto too); only a new install, or settings
  saved before this row existed, start on Quiet.
- **Fan outside games** has the same fan choices for the shelf, the game lists and Settings.
- **Smooth fan** decides how the fan changes its speed. **On**: gently, along with the temperature. Once it turns, it
  keeps turning slowly instead of stopping and starting again, and it never jumps to a loud speed. **Off**: the
  system's own steps, where the fan switches on, runs for a while and switches off again.

Values in bold are the defaults. Auto is always the system's own: with Smooth fan off it leaves everything as the
system has it, with Smooth fan on it is the system's cooling profile as a smooth curve.

## Smooth fan

*Tested on a Mini V2 for eight minutes with a game that keeps one core busy: the fan started once where the system's
own control started it twice. Its lowest speed, the 78 °C limit and how it sounds are not checked yet.*

pocket-ui drives the fan itself and speeds it up and down over seconds, never in jumps. Very quiet, Quiet, Auto and Cool are
four curves: each starts the fan at its temperature (60, 52, 46 and 40 °C; Auto takes the curve of the system's cooling
profile, quiet, moderate or aggressive) and reaches full speed at 78 °C. It only stops the fan again once the Pocket
has been clearly cooler for most of a minute. Some things never change:
- From 78 °C on any processor or graphics sensor the fan runs at full speed at once, whatever you chose.
- The system's own protection stays as it is: the processor still slows itself down when it gets very hot, and the
  Pocket still switches itself off before it could take damage.
- While the Pocket sleeps or powers off, and whenever pocket-ui is not running (it ended, crashed or got stuck), the
  system's own fan control takes over again within seconds.

With Smooth fan on, **Games: fan** applies to every game: the games on pocket-ui's own host, RetroArch, the
PlayStation 2, GameCube, Wii and 3DS emulators and PortMaster ports. If you gave a game its own cooling profile in
EmulationStation, that one applies to that game.

## Which setting a game uses

A game uses, in this order: what you saved for **this game** in the pause menu, what you saved for **its console**,
then **Games: performance** and **Games: fan** from this page, then the system's own setting. So the page sets the default for every game, and the pause menu (Options → Extras, then Save changes) makes
exceptions for one console or one game. "Restore defaults" in the pause menu goes back to this page's values. See
[Options](options.md#extras).

**Games: performance** applies to the games on pocket-ui's own host (see
[Systems](systems.md#on-pocket-uis-own-host)), to PortMaster ports (below), and to PlayStation 2, GameCube, Wii and
3DS games without a Performance of their own in the pause menu's Options: Battery is Battery there, Auto and Max are
Max. Games in RetroArch keep the system's settings.
**Games: fan** applies to the same games with Smooth fan off, and to every game with it on (above).

**Fan outside games** applies only while no game is running and the Pocket is awake. With Smooth fan off: as soon as
a game starts, the system's cooling is back and the game sets its own. While the Pocket sleeps (light or deep sleep)
the fan is the system's in either case, and the choice comes back when it wakes up. When pocket-ui ends, even after
a crash, the system's cooling is put back.

## PortMaster ports

Ports have no pause menu, so each port has its own **Performance & fan** on its card: select the port under Ports,
press X for its [card](library.md#opening-a-card), then **START**.

| Row | Choices |
|---|---|
| **Performance** | Battery, Auto, Max |
| **Fan** | Very quiet, Quiet, Auto, Cool |
| **Use the defaults** | Forgets the port's own choice (only there when it has one) |

- A change is saved at once and applies from the port's **next start**. A row that shows "(default)" follows
  **Games: performance** or **Games: fan** on this page.
- **Battery** holds the processor's top speeds at about half while the port runs: cooler, quieter, and the battery
  lasts longer; a demanding port may stutter. **Auto** leaves the system's own setting. **Max** runs the big
  processor cores and the graphics at full speed all the time.
- **Fan** needs Smooth fan on; then the port's own choice wins over a cooling profile given to the port in
  EmulationStation. With Smooth fan off, ports keep the system's cooling.
- Everything goes back the moment the port ends, even when it crashes or pocket-ui does, and before the next game
  starts with its own settings, so a port's Battery never sticks to the game after it.

**A port that keeps a core busy.** Some ports, many GameMaker games among them (Deltarune, for example), keep one
processor core busy all the time while they wait for the next picture. In the first minutes of a port pocket-ui
looks at that once. When it finds it, the card shows "Busy core? Press START", and Performance & fan opens on
**Use Battery for this port**: "This port keeps one core busy all the time. Battery performance keeps it cooler and
quieter; if it stutters, go back to Auto." A applies it. pocket-ui never changes it by itself, and the card shows the
hint only until you have opened the page once.

## Now

The bottom of the page shows, refreshed every second: the hottest temperature on the Pocket, the fan speed
(turns per minute and how hard it is driven, in per cent) and who drives it: "smooth, Quiet" (or Auto, Cool)
while Smooth fan does, "full speed, hot" from 78 °C, otherwise the temperature the system's fan starts at with the
profile in use right now; then the clock of each group of processor cores
(little, big and prime) and of the graphics processor, and how much power the battery delivers ("On the charger"
while plugged in). It only reads; nothing on this part can be changed.
