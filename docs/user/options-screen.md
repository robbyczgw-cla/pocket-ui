# Options: Screen frame and Screen orientation

Two display options from [Options → Picture](options.md#picture), explained in detail.

## Screen frame

| Choice | What it looks like |
|---|---|
| **None** | Plain black border around the game. |
| **Auto** | Picks a matching console or handheld frame automatically (see table below). |
| **Ambient** | Softly bathes the border in the game's live colours—like bias lighting behind a TV—shifting smoothly as scenes change. |
| **DMG** | Classic original Game Boy: dark bezel with one rounded corner, twin accent stripes above the screen and a working power light. **DMG colour**: grey, clear, red, yellow, green or black. Offered for Game Boy and Game Boy Color. |
| **GB Pocket** | Slimmer Game Boy Pocket shell with tighter glass. **GB Pocket finish**: brushed silver or clear. Offered for Game Boy and Game Boy Color. |
| **GBC** | Game Boy Color bezel with **GBC colour** selector underneath: classic grey, indigo, clear purple, atomic purple, black, white or glacier blue (clear finishes are translucent). Offered for Game Boy and Game Boy Color. |
| **GBA** | Original landscape Game Boy Advance with rounded sides, peeking shoulder buttons and a small speaker grille. Offered for Game Boy Advance. |
| **GBA SP** | Open Game Boy Advance SP clamshell with the bottom hinge. **GBA SP finish**: platinum, cobalt, flame red or graphite. Offered for Game Boy Advance. |
| **DS** | Nintendo DS dual-screen frame with central hinge and top-screen speaker slits. Offered for Nintendo DS only (see below). |
| **PSP** | Glossy piano-black PSP shell with curved ends and a soft diagonal sheen. Offered for PSP only. |
| **CRT** | Vintage tube TV behind curved dark glass in dark plastic or wood (**TV cabinet** option appears below). |
| **Studio monitor** | Grey broadcast monitor with flat glass and bottom control knobs. |
| **Portable TV** | Compact portable TV with a telescoping antenna and tuning dial. |
| **Woodgrain** | Classic woodgrain front with a ribbed black top strip. |
| **Arcade** | Glossy black arcade bezel with two subtle light strips. |
| **Glow** | Slim rounded outline glowing softly in the console's shelf accent colour. |

Whenever a frame offers colour or finish choices, that row appears right below **Screen frame**. Console-specific shells
only appear for their matching system so the list stays tidy (though any previously saved choice for another console
still works).

Swan, Watch and Goggles are no longer listed manually, though **Auto** still draws them for WonderSwan, Game & Watch
and Virtual Boy.

Frames need border space around the picture, so they work with Integer, Integer smaller and Aspect scaling—Fullscreen
fills the entire screen with no room left. Frames sit strictly behind the game and never crop or cover pixels, resizing
automatically if the game changes resolution. Because standard Integer scaling leaves slim margins on some systems
(only 20 pixels per side on GBA), **Integer, smaller** gives frames extra breathing room so details like speaker
grilles, knobs and antennas can show.

**The power light.** DMG, GB Pocket, GBA SP, GBA, PSP, Auto's WonderSwan frame and the Game Gear/Lynx lenses feature a
live power LED that behaves like real hardware: Game Boy and Pocket red LEDs dim at Battery low (5%), GBA and SP turn
from green to red, PSP blinks orange and Game Gear flashes red. It also double-blinks briefly whenever you save a state
(manual or auto). Turn off Settings → Look → Cozy extras → **Power lights** if you prefer a steady light.

**DS and the Nintendo DS.** The DS frame fits stacked dual screens (the default), placing its hinge right between them
or inside your emulator's screen gap. Switch to side-by-side or single-screen layouts and it gracefully swaps to a clean
screen lens.

When set to **Auto**, pocket-ui picks the frame for each system automatically:

| Console | Auto |
|---|---|
| Game Boy | DMG, grey |
| Game Boy Color | GB Pocket, clear |
| Game Boy Advance | GBA |
| Nintendo DS | DS |
| PSP | PSP |
| WonderSwan | Minimal pastel handheld |
| WonderSwan Color | Translucent skeleton handheld |
| Game & Watch | Brushed gold faceplate |
| Virtual Boy | Black visor with red trim |
| Atari 2600, Atari 7800 | Woodgrain |
| Game Gear, Neo Geo Pocket (Color), Lynx, Pokemon mini | **Handheld**: recessed dark lens with a small power LED where space allows |
| Every other console | **Monitor**: graphite surround with rounded corners |

Any frame you choose manually stays put—only **Auto** follows this table.

<!-- TODO: frame packs (/storage/.config/pocket-ui/frames/, docs/FRAMES.md) are meant for the
     store. Describe them for players once there is a way to install them. -->

## Screen orientation

Some games were built for vertical screens—TATE shmups, many WonderSwan titles, and book-style Game Boy or DS games.
Held normally, they sit narrow in the middle with wide black bars. Turn your Pocket sideways and fill the screen:

| Choice | How you hold the Pocket |
|---|---|
| **Normal** | Standard landscape orientation. |
| **Turned left** | Rotated 90° left (D-pad at the bottom, face buttons at the top). |
| **Turned right** | Rotated 90° right (D-pad at the top, face buttons at the bottom). |

- The picture rotates upright and rescales for the vertical shape (for example, a 224 × 288 game jumps from 648 × 864 to
  1195 × 896 at Integer scaling).
- **The D-pad and sticks rotate automatically** so Up is always up in-game. Other buttons stay put—remap them under
  [Controls](options.md#controls) if needed. External Bluetooth controllers aren't rotated since you don't turn them.
- The pause menu, Quick settings and overlays stay in standard landscape—just turn the Pocket back when adjusting menus.
- Touch input rotates along with the picture.
- Only **Ambient** and **Glow** frames appear while rotated (console shells would sit sideways); switching back to Normal
  restores your frame.
- Screenshots (HOME + A) save the raw unrotated image straight from the emulator.
- Watch out for built-in emulator rotation: WonderSwan's core flips vertical games on SELECT, so combining both will
  rotate the image twice.
- Save this per game via [Keep for](options.md#keep-for) → This game.

Arcade games run in RetroArch and handle rotation differently: see [Arcade games](arcade.md#vertical-games).
