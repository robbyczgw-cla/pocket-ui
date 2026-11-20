# Settings: Screen & sound, Controls

The **Screen & sound** and **Controls** categories of [Settings](settings.md), on a page of their own. Until this version
they were one category, Controls & sound (Sound & feel before that); Brightness and Volume were only in the
[quick settings](quick-settings.md), Screen colour and Night light were in Look, and Zoom in the pause menu was here (it
is in [Playing](settings-playing.md#zoom-in-the-pause-menu) now).

## Screen & sound

Settings → **Screen & sound**. What the device puts out: how bright and how loud, the screen's colour, and what you hear
besides the game.

| Entry | What it holds |
|---|---|
| **Brightness** | How bright the screen is: the same value as the [quick settings](quick-settings.md)' Brightness and START + Vol, so they always agree. Left/Right change it, one step at a time; below the lowest backlight the screen gets darker still (extra dim), as in the quick settings |
| **Volume** | How loud games and menus are: the same value as the quick settings' Volume and the volume keys. Left/Right change it, one step at a time |
| **Screen colour** | The screen's white: **Cooler 10** … **Neutral** … **Warmer 10**. Left/Right changes it, you see it at once; A goes back to Neutral. The same setting as [SELECT + Vol](controls-settings.md#volume-and-brightness-keys), so the two always agree |
| **Night light** | Warmer, less blue colours in the evening, in the menus and in games: **Off** (the default), **On** or **Scheduled**, with a strength. See [Night light](#night-light) |
| **Menu sounds** | Soft clicks in the menus, not the game's volume (that is **Volume** above). A turns them on or off, Left/Right set how loud. See [Playing](playing.md#menu-sounds) |
| **Case sounds** | Cases click open, cartridges and discs go in with a sound, the old TV hums on. Shelf and Cabinet looks only. Needs Menu sounds on; the menu sounds are not affected. **On** at the start. A or Left/Right switches it |
| **Photos of big moments** | Off unless you switch it on. On: when your save shows a big step (a new badge, a boss beaten, a level of ten), pocket-ui keeps a small photo of that moment in the game's case, quietly: when you quit it slides into the case. **Also in play** (the third value): the moment a photo is taken you also see it over the game (a soft flash, the shutter, the photo out of the screen's bottom edge, developing); not on the TrimUI Brick and in pocket-ui for Android, which have only Off and On. Off, no photo is taken, nothing is kept and the case has no photos to open (ones taken before stay on the card). See [Photos of big moments](library.md#photos-of-big-moments) |

Case sounds sits beside Menu sounds, which it needs. While Menu sounds is off, the line at the bottom says so.

### Night light

Settings → Screen & sound → **Night light** has its own page: [Night light](night-light.md).


## Controls

Settings → **Controls**. The buttons in one place, then what you feel in your hands and see on the controller.

| Entry | What it holds |
|---|---|
| **Buttons** | Which face button confirms in pocket-ui's menus, and how the buttons look in the hints, with a picture of the four buttons. See [Buttons](#buttons) |
| **Controller setup** | A separate layout for each controller, in menus and games: which of its buttons is which. Controllers are paired in Connections → [Bluetooth](bluetooth.md). See [Controller setup](#controller-setup) |
| **Test buttons** | Every controller's buttons, triggers and sticks light up while you press them, and the page says what each press does in menus and in games. See [Test buttons](#test-buttons) |
| **Haptics** | A very short tick in the menus as the selection moves, on every page where a move also clicks. A turns it on or off. Off by default |
| **LEDs** | The lights around the sticks: off, one colour, or the battery colours. Only on a device with sticks. On the TrimUI Brick the row is called **Lights & buttons** and holds the Brick's own lights, the F1 and F2 keys and the mute slider. See [LEDs](#leds) |

### Buttons

Settings → Controls → **Buttons** has two rows and, under them, a picture of the four face buttons where they sit on
your device, each with the symbol the hints show for it; the one that confirms is named **Confirm** and the one that
goes back **Back**. A line under the picture says which button confirms right now. A or Left/Right switches the
selected row, and the next button you press already works the new way. Both choices are saved at once.

#### Button layout

Which face button plays A, B, X and Y in pocket-ui's own screens.

- **Nintendo**, the default: A on the right, B at the bottom, X on top, Y on the left, as printed on the Mini V2.
- **Xbox**: A at the bottom, B on the right, X on the left, Y on top, as on an Xbox controller.

Menus only: games and safe mode keep the printed buttons. What it changes and what it leaves alone is in
[Controls](controls.md#button-layout).

Not on Android: there the handheld's own setting decides which button confirms (on the Odin its Controller style),
and the Buttons page only has Button symbols ([Android](android-mvp.md)).

On experimental **AYN Odin 2 Mini** and **Odin 2 Portal**, the default menu layout is **Xbox** (A bottom,
B right, X left, Y top), matching the printed letters. The original Odin 2 defaults to Nintendo. A saved choice
wins. Safe mode follows the device’s printed layout. Games keep their positional controls: the screenshot
shortcut is HOME + the right face button (printed B on Mini/Portal), regardless of the menu layout.

#### Button symbols

How the hints show the four face buttons, everywhere a key is shown (the hint bars, dialogs, the pause menu, this
manual's key chips and its controls picture).

- **Nintendo**: plain letters A, B, X and Y.
- **Xbox**: A, B, X and Y on green, red, blue and yellow.
- **PlayStation**: cross, circle, square and triangle. Cross stands for A, circle for B, square for X and triangle
  for Y.

Retroid and AYN handhelds start with Xbox, Anbernic handhelds with Nintendo; your choice wins. It only changes the
pictures of the keys: which button confirms is [Button layout](#button-layout)'s job, and the two can be
combined freely. The explanations under a list still name the keys by their letter.

### Controller setup

Settings → Controls → **Controller setup** gives each controller a layout of its own, kept through reconnects:
choose the controller at the top, then press the button you want for each action. It was the third row of Buttons. The
same page opens from the pause menu, Options → Controls. See [Controller setup](controls-settings.md#controller-setup).

### Test buttons

Settings → Controls → **Test buttons** shows what a controller really sends, for a button that seems dead, a
trigger that feels off, a stick that drifts, or when you want to see what your own layout does.

![Test buttons: the controller drawn, a trigger half pulled, the last button pressed](pictures/test-buttons.webp)

- **Left: the controllers.** The Pocket's own buttons, every Bluetooth controller and every USB controller, each with
  its player number and how it is connected. A small light burns while something of that controller is down. The one
  you press a button on (or push a stick of) is drawn on the right.
- **The drawing.** Face buttons, D-pad, L1 and R1, SELECT, START, HOME and both sticks light up while they are down.
  The letters on the four face buttons are what those buttons are in the menus, after the [Button layout](settings-sound.md#button-layout)
  and your own layout in Controller setup. A small dot on a button means your own layout moved it.
- **Triggers** show how far they are pulled, in percent. The tick on the bar is where they count as pressed (55%); they
  are let go again below 40%. A trigger that is only a button (some controllers) fills at once.
- **Sticks** show where the stick is, as two numbers from -100 to 100. The small square in the middle is the dead zone:
  menus ignore a stick that stays inside it.
- **The last button pressed**, on which controller and player, and what it is **in menus** (A, B, X, Y or its own name)
  and **in games** (the console's button, through your layout). If your layout moved it, the page says **Remapped to**
  and the button it acts as. A press that pocket-ui does not know is shown as **Not recognised**, with the name the
  controller gave it: useful in a bug report.

Every button is a test here, B, START and HOME too, so none of them leaves the page. **Hold HOME for 2 seconds, or B and
START together on one controller**; a bar shows how long to keep holding. A tap on the hint at the bottom right leaves
as well. The volume and power keys keep their meaning. With two controllers, both can be pressed at the same time: each
keeps its own lights, and the page names the one that pressed last.

### LEDs

Settings → Controls → **LEDs** is for the lights around the two sticks. They are **off** unless you turn them on here:
ROCKNIX lights them at full brightness out of the box, and pocket-ui switches them off the first time it starts.

- **Lights** has three choices. **Off** keeps them dark. **Colour** makes both sticks glow in one colour. **Battery**
  uses ROCKNIX's own battery colours: green when charged, orange and then red as it runs low, always at full brightness.
- **Colour** (only with Lights on Colour) picks a calm colour: blue, warm, teal, green, violet or rose. **The game's**
  is warm in the menus and takes the colour of the system you are playing while a game runs.
- **Brightness** (also only with Colour) has four steps: 25, 50, 75 and 100%.

Left/Right change the selected row, A moves on to the next choice. The choice takes effect at once, is kept across restarts,
and is the one ROCKNIX applies at the next boot. The lights also go dark while the device sleeps, as ROCKNIX does it.

On the experimental TrimUI Brick Pro, **Lights & buttons** offers only the separate F1/F2 assignments. L3/R3 are stick clicks; the side switch and light controls stay disabled.
