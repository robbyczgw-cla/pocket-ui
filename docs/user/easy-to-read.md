# Easy to read, Zoom and Hold to toggle

Three helps for small screens and hands that tire: larger text in one switch, a magnifier for a game's picture, and buttons
that stay down without holding them. The magnifier stays out of the way until you want it.

## Easy to read

The Tour's last card **Easy to read** (Settings → Help & tools → [Tour](tour.md); Up/Down choose On or Off, A applies it) sets two settings together:
[**Text size**](settings.md#text-size) to Large and [**Motion**](settings-sound.md#motion) to Reduced. Every screen then has larger
text, and nothing slides: pages and messages only fade. Switching the card off puts Text size back to Normal and Motion to
Full. It is no setting of its own: both stay in their rows, and you can change either one afterwards. While Easy to read is on
(Large text and a Motion that is not Full), the pause menu also lists [Zoom](#zoom) on its first page. After an update the card
is one of the "New in this version" cards.

## Zoom

**Zoom** is in the [pause menu](pause-menu.md) on pocket-ui's own host (see below where). It shows the frozen picture of
the game **2x** larger, for small text such as a Game Boy Advance dialogue. Only the game's own picture is enlarged, not the black
borders or the screen frame around it, and on a handheld's games it opens at the bottom of the picture, where the dialogue boxes are.

- The **D-pad** moves the part you see, a quarter of it at a time, up to the picture's edge.
- **A** (or L1 / R1) switches between **2x** and **3x**. A small pill in the top left corner names the level, and the page
  opens at the one you used last until you quit pocket-ui.
- **B** goes back to where you came from: the menu (the game stays paused), or, after HOME + X, into the game.

**Where to find it.** Pause menu → Options → **Zoom**, next to Picture and Controls, always. It also stands on the menu's first page, right before
Quit, while Easy to read is on or Settings → Sound & feel → [**Zoom in the pause menu**](settings-sound.md#zoom-in-the-pause-menu) is **On**
(**Off** is the default). And for a quick look in the middle of a game, Options → [Shortcuts](options.md#shortcuts) → **Zoom**
switches on **HOME + X**: the picture is enlarged at once, over the frozen game, with no menu in between. The shortcut is **Off**
by default, and not in Kids Mode or Easy.

Zoom enlarges the picture the menu already shows behind it, whatever the screen's size, so the game does nothing extra. Before
that picture is there (a game still starting) the row does not open. Games in RetroArch and the other emulators have no Zoom:
pocket-ui cannot get their picture the same way.

## Hold to toggle

Pause menu → Options → [Controls](options.md#controls) → **Hold to toggle**, on pocket-ui's own host. For a button a game wants
held down, such as run: choose the console button here (Left/Right: Off, then A, B, X, Y, L1, R1, L2, R2, L3, R3 as the console
names them). One press then holds it down for the game, and the next press lets go.

- While a button is held down, a small "B held" badge shows in the top right corner of the game (where "FF" shows too) and goes
  when you let go.
- HOME shortcuts do not toggle the chosen game button.
- Nothing stays held across a pause, a sleep, a dark screen, a loaded state or a restart: you start again with the button up. Disconnecting or moving a controller to another player releases that player’s held button too.
- It is saved per console or per game like the other lines of Controls, works for every player, and **Reset controls** puts it
  back to Off.
- Fast-forward needs no help: HOME + R2 already switches it on and off ([Shortcuts](options.md#shortcuts)).

On the TrimUI Brick the small text starts larger. Short system facts try one smaller size before leaving out a row.
A narrow game-info column can show an unrated game as **None · Y**: Y opens its status and rating choices. The info heading keeps the year visible; a long console name can use its usual short name, such as **NES**.
In German, **Zuletzt** in Home's hints opens Recent games. Credits and password explanations also try a smaller size
before shortening a line. Pixel Path keeps its grades, selected explanation and live readings above the hints.
Sleep & power keeps its explanations complete; Up/Down scrolls the rows above them.
