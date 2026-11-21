# Big text, Zoom and Hold to toggle

Three comfort features for tiny screens and tired thumbs: bigger text in a single switch, an in-game screen magnifier,
and buttons that stay held down without keeping your finger on them. Nothing gets in your way until you want it.

## Big text

The final card in the Tour—**Big text** (Settings → Help & tools → [Tour](tour.md); pick On or Off with Up/Down, press A
to apply)—flips two settings at once: [**Text size**](settings.md#text-size) to Large and
[**Motion**](settings.md#motion) to Reduced. Menus get bigger, easier-to-read letters, and slides become gentle fades.
Turning it off restores Normal text and Full motion. It isn't a separate lock—both settings stay editable individually
afterwards. Whenever Big text is active (Large text plus non-Full motion), [Zoom](#zoom) also appears right on the first
page of the pause menu. After an update, this card appears under "New in this version".

## Zoom

Available in the [pause menu](pause-menu.md) for games running in pocket-ui's own player, **Zoom** blows up the paused
game screen to **2x** so tiny text (like GBA RPG dialogue) is effortless to read. Only the game picture zooms—never the
black bars or screen frame—and handheld games open focused on the bottom of the screen where dialogue boxes usually sit.

- **D-pad** pans around the zoomed image a quarter-screen at a time up to the edges.
- **A** (or L1 / R1) toggles between **2x** and **3x**. A small badge in the top-left shows the current zoom, and
  pocket-ui remembers your last level until you quit.
- **B** closes Zoom and takes you back to the pause menu—or straight back into the game if you opened it with HOME + X.

**Where to find it.** Always available under Pause menu → Options → **Zoom** (right below Speed, rumble, fan). It also
shows up on the first page of the pause menu right above Quit when Big text is on or when Settings → Playing →
[**Zoom in the pause menu**](settings-playing.md#zoom-in-the-pause-menu) is **On** (default **Off**). Want instant
mid-game zoom? Turn on Options → [Shortcuts](options.md#shortcuts) → **Zoom** to map **HOME + X**—it freezes the game
and zooms immediately without opening the menu (**Off** by default, disabled in Kids Mode and Easy).

Zoom magnifies the frame already captured for the pause screen, so it adds zero overhead to the game. It's unavailable
before the first game frame draws, and isn't supported in RetroArch or standalone emulators because pocket-ui can't grab
their frames the same way.

## Hold to toggle

Found under Pause menu → Options → [Controls](options.md#controls) → **Hold to toggle** (pocket-ui's own player). Pick
any console button a game makes you hold down (like Run)—cycle Off, A, B, X, Y, L1, R1, L2, R2, L3, R3 with Left/Right.
Tap once to lock it down, tap again to release.

- While active, a small "B held" badge sits in the top-right corner (next to "FF") until you release it.
- HOME shortcuts never accidentally toggle your chosen button.
- Held buttons always release automatically on pause, sleep, screen-off, loading a state, restarting, or if a controller
  disconnects or switches player slots.
- Saved per console or per game just like the rest of Controls, works for all players, and **Reset controls** resets it
  to Off.
- Fast-forward doesn't need Hold to toggle: HOME + R2 already toggles it on and off ([Shortcuts](options.md#shortcuts)).

On the TrimUI Brick, small text starts slightly larger for readability. Short console facts step down one size before
dropping a row, narrow game-info columns show unrated games as **None · Y** (press Y to set status or rating), and
headers keep the release year visible by using short console names like **NES**. In German, **Zuletzt** on Home's hint
bar opens Recent games. Credits and password notes also step down a size before truncating, Pixel Path keeps its grades,
descriptions and live stats above the hints, and Sleep & power keeps full explanations readable while Up/Down scrolls
the rows above them.
