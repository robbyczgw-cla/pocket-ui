# Options: Screen frame and Screen orientation

Two entries of [Options → Picture](options.md#picture), on a page of their own.

## Screen frame

| Choice | What it looks like |
|---|---|
| **None** | Black around the picture. |
| **Auto** | A frame around the picture that suits the console (see below). |
| **Ambient** | The space around the picture glows softly in the game's own colours, like a TV with ambient light. It follows the game smoothly as the scene changes. |
| **DMG** | The first Game Boy: darker glass around the screen with one large rounded corner, two thin coloured lines above the picture and a power light beside it. **DMG colour**: grey, clear, red, yellow, green or black. Offered for Game Boy and Game Boy Color games. |
| **GB Pocket** | The slimmer Game Boy Pocket, with tighter glass. **GB Pocket finish**: brushed silver or clear. Offered for Game Boy and Game Boy Color games. |
| **GBC** | The Game Boy Color's shell around the screen. Pick its colour under **GBC colour** (shown while GBC is chosen): classic grey, indigo, clear purple, atomic purple, black, white or glacier blue. The clear ones are see-through. Offered for Game Boy and Game Boy Color games. |
| **GBA** | The Game Boy Advance: a landscape handheld with rounded sides, shoulder buttons peeking over its top corners and a small speaker grille. Offered for Game Boy Advance games. |
| **GBA SP** | The Game Boy Advance SP, opened: the lid with the hinge along the bottom. **GBA SP finish**: platinum, cobalt, flame red or graphite. Offered for Game Boy Advance games. |
| **DS** | The Nintendo DS: two screens with a hinge between them and speaker slits beside the top screen. Offered for the Nintendo DS only; see below. |
| **PSP** | The PSP: a piano-black wide body with oval ends and a soft diagonal sheen. Offered for PSP games only. |
| **CRT** | An old TV: the picture behind curved dark glass, in a cabinet of dark plastic or wood (**TV cabinet**, shown while CRT is chosen). |
| **Studio monitor** | A grey professional monitor with flat glass and a row of small knobs and buttons below. |
| **Portable TV** | A small portable TV with a telescopic antenna and a tuning dial. |
| **Woodgrain** | A wood-look front with a ribbed black band on top. |
| **Arcade** | A glossy black arcade bezel with two soft light strips. |
| **Glow** | A thin rounded line around the picture, glowing in the console's colour from the shelf. |

The colour or finish of a frame is a row of its own under **Screen frame**, shown while that frame
is chosen. A frame shaped like one console is offered for that console's games only, so the list stays short; a
frame saved for another console keeps working.

Swan, Watch and Goggles are no longer in the list; **Auto** still draws them for WonderSwan, Game & Watch and Virtual Boy.

A frame needs space around the picture, so it appears only with Integer, Integer smaller and Aspect
scaling. Fullscreen fills the screen and leaves no space for one. The frame sits
behind the picture and never covers any of it; when the picture changes size, the frame follows.
At Integer the space is narrow on some consoles (Game Boy Advance: 20 pixels left and right);
**Integer, smaller** gives every frame space, and details such as speaker grilles, knobs and the
antenna only appear where there is space for them.

**The power light.** DMG, GB Pocket, GBA SP, GBA, PSP and Auto's WonderSwan frame have a small power light
that shows the battery: green above 20%, red at 20% and below, blinking slowly at 5% and below.
It flashes briefly when you save a state (to a slot, or the automatic one).

**DS and the Nintendo DS.** DS fits the DS with its two screens stacked, as games start. The
hinge sits beside the screens where they touch, and exactly in the gap between them when the
emulator leaves one (its screen gap option). With the screens side by side or one screen alone,
DS shows the plain screen lens instead.

For Auto, pocket-ui picks one of its own frames for the console:

| Console | Auto |
|---|---|
| Game Boy | DMG, grey |
| Game Boy Color | GB Pocket, clear |
| Game Boy Advance | GBA |
| Nintendo DS | DS |
| PSP | PSP |
| WonderSwan | A minimal handheld, pastel |
| WonderSwan Color | A minimal handheld, see-through skeleton |
| Game & Watch | A brushed gold faceplate |
| Virtual Boy | A black visor with red trim |
| Atari 2600, Atari 7800 | Woodgrain |
| Game Gear, Neo Geo Pocket (Color), Lynx, Pokemon mini | **Handheld**: a dark screen lens around a recessed screen, with a small power light where there is room for it |
| Every other console | **Monitor**: a graphite surround with rounded corners |

A frame you picked yourself stays as it is; only Auto follows this table.

<!-- TODO: frame packs (/storage/.config/pocket-ui/frames/, docs/FRAMES.md) are meant for the
     store. Describe them for players once there is a way to install them. -->

## Screen orientation

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
  are; change them under [Controls](options.md#controls) if a game needs it. A Bluetooth controller is not turned, because it
  does not turn with the Pocket.
- The pause menu, Quick settings and messages stay as usual: turn the Pocket back to use them.
- Touch follows the turned picture.
- Of the frames, only **Ambient** and **Glow** show around a turned picture: the others are drawn for a Pocket held
  as usual and would stand on their side. Your frame comes back with Normal.
- Screenshots (HOME + A) keep the game's picture as the emulator makes it, not turned.
- Some emulators turn the picture themselves: the WonderSwan emulator turns a vertical game upright when you press
  SELECT. pocket-ui shows the picture as the emulator asks, and Screen orientation turns it once more on top.
- Keep the choice for one game with [Keep for](options.md#keep-for) → This game.

Arcade games run on RetroArch, which has its own way: see [Arcade games](arcade.md#vertical-games).
