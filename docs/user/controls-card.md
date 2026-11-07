# The controls card

New to a console, or back after a year: which button of the Pocket is which button of the game? The **controls card**
shows it on a drawing of the Pocket's buttons: the shoulder buttons at the top, the D-pad and the left stick at the left,
the four face buttons and the right stick at the right, SELECT and START in the middle. Next to each button stands what
it is on the console: on a SNES the right button is **A**, on a PlayStation it is **Circle**, on a Nintendo 64 the bottom
button is **A** and the **C buttons** are on the right stick.

![The controls card for a Super Nintendo game: which button is which](pictures/controls-card.webp)

Nothing on it is made up. The names are the ones the emulator itself gives its buttons; the card also shows **your own
changes** from [Options → Controls](options.md#controls): a button you gave another job is drawn in amber,
a button that now does nothing shows a dash. Under the drawing stand the keys of pocket-ui itself: **HOME** opens the menu,
and, when they are switched on under [Options → Shortcuts](options.md#shortcuts), **HOME + R1** saves, **HOME + L1**
loads and **HOME + X** zooms (Zoom is off until you switch it on).

## Where it is

- **The first time you start a console.** Before the first game of a console (not of every game) the card comes up on its
  own. Any button, or a tap, goes on to the game; if you do nothing it goes on by itself after twelve seconds. Continuing a game you already played skips the card. It comes
  once for each console and not again by itself. In [Kids Mode](kids-mode.md) it does not come.
- **On the game's card.** Press X on a game, scroll down: the row **Controls** is after the game's other rows (Screenshots)
  and before Hide or delete. A opens the card, B comes back.
- **In the pause menu.** HOME, Options, **Controls**: the first line, **Controls card**, shows the card of the game you are
  playing, with the console's own button names as the emulator reports them, and your changes as they are right now. B comes
  back to the list.

The pause menu itself has no extra line for it.

## Which consoles

The card names the buttons of the Game Boy, Game Boy Color, Game Boy Advance, NES / Famicom, SNES / Super Famicom,
PlayStation, Nintendo 64 and Sega Saturn from the start. For any other console it shows the names from the first game you
played on it (the emulator tells them once it runs); until then it says that the names show after the first game, and still
shows the HOME keys. Games that run in RetroArch or in another emulator show only HOME as the menu; their buttons are set
in that emulator.

On a **Nintendo 64**, the right stick is the C buttons. Hold R2 and the four face buttons are the C buttons instead.
On a **PlayStation** both sticks go to the game, as on a DualShock; with Options → Controls → **Left stick as D-pad** on, the
left stick is the D-pad again and the card says so.

## When it looks wrong

- A button shows a dash although you expect it to work: it does nothing in this game (the console has no such button, or you
  set it to None in Options → Controls). **Reset controls** there puts everything back.
- The names differ from the box: the card uses the emulator's names (the PlayStation's four shapes are Cross, Circle,
  Square and Triangle; the Game Boy Advance's turbo buttons are on X and Y).
- The card does not come at a first start: it only comes for games that run on pocket-ui's own host, once for each console,
  and not in Kids Mode. The game's card has the row at any time.

On a TrimUI Brick, **MENU** opens the menu. The card and hints use the name printed on that device; other devices use **HOME**.

## On Android

Tap **Controls** on a game's card (R2 with a gamepad), or **Controls** in the pause sheet.
The scrollable card shows Android's printed A/B/X/Y names alongside the actual touch-pad
labels and the console buttons they press. The first game of a console opens it once;
Back or A goes on. Continuing an existing moment skips it. Swipe or use the D-pad to read
all the rows. The N64 touch D-pad is its analog stick, and the small C arrows use its right
stick. Console names come from the running core, or the verified console table before a
first start. External emulator apps own their controls.

## Bluetooth and separate controller layouts

The controls card describes the game's layout after the controller's own layout. **Controller setup**, beside
**Controls card** in Pause → Options → Controls, changes one controller for the shelf, menus and games;
Settings → Playing → Buttons opens the same setup. An occupied button swaps automatically and L2/R2 can be
captured whether the controller reports buttons or analogue axes. See [Controller setup](controls-settings.md#controller-setup).
