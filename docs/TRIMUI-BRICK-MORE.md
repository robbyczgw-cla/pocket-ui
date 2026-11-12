# TrimUI Brick, own card: Kids mode, Screenshots, Web companion, Over the game, Tour

On top of the Brick's own card ([TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md), [TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)).
All of it is pocket-ui's own code that ROCKNIX already has; the Brick only hid the rows. What changed is behind the `brick` feature and
the standalone card (`POCKETUI_BRICK_STANDALONE`): NextUI's pak and every ROCKNIX build are as they were (the tests below say so).

**Nothing here has run on the Brick.** The Brick was not reachable on 2026-11-03 (ssh refused); everything is proven on the host.

## What is on the card now

| Row / key | What it does on the card | How it works |
|---|---|---|
| Settings > System > Kids mode | the same as on ROCKNIX: PIN, picker of 5 to 20 games, big shelf, play time, volume ceiling, parent menu, SELECT + START held to leave | pure UI (`kids.rs`, `app_kids.rs`); the PIN-forgotten file is `kids-mode-off.txt` in `Roms/` (`/storage/roms` on the card). The standalone-emulator rules for it (`standalone.rs`) never apply: the card has none |
| Settings > Games > Screenshots | the gallery of `Roms/screenshots/<system>/<game> <time>.png` | `gallery.rs`, `app_gallery.rs` |
| HOME + A in a game | pocket-core's own last frame at the core's size | `screenshot.rs`, already in `pocket-core`; the folder is on the card, so a card reader or the file share finds it |
| HOME + A in pocket-ui | the screen as it is, filed under `pocket-ui/Pocket UI <time>.png`, toast "Screenshot saved" and the shutter | `app_screenshot.rs` (Brick only, not in Kids mode); `grim` (below) |
| `grim` on the card | `grim -t png -` and `grim FILE` hand over the window on show | `hwroot/usr/bin/grim` asks pocket-fb (`--ipc-send ... "pocket_grab FILE"`, `Server::grab`); pocket-ui's pause menu behind a standalone emulator and the web companion's screen view use the same call |
| Settings > Connections > Web companion | pocket-ui's pages for the library, notes, uploads, play time, battery and a screen view, in a browser on the tailnet | see below |
| Settings > Playing > Over the game | Off, FPS (frame rate and CPU), FPS + sensors (temperature, **charge in percent**, draw in watts), Pixel Path; HOME + Y goes to the next level | `over_game.rs`, `overlay.rs` (drawn by pocket-core); the Brick has no fan, so no fan figure; the charge figure is the Brick's (`osd_sensors::shows_charge`) |
| Tour | adds Tailscale and Kids mode's profile, leaves out the cards whose page the Brick has no row for (Screen frames, RetroAchievements, Players; Over the game on NextUI's card) | `device::hides_here`, `device::brick_card_hidden`, `features::available` |

Left out on purpose: Screen frames (needs the shaders branch), RetroAchievements, the starter pack. Haptics and stick lights stay out of the Tour
until the Brick's hardware page ([TRIMUI-BRICK-HW.md](TRIMUI-BRICK-HW.md)) is in the Tour (its rows decide: `hides_here` must then stop hiding `haptics` and `leds` on the Brick).

## The web companion on the card

The companion listens only on Tailscale's own interface (`tailscale0`, `SO_BINDTODEVICE`) and recognises the owner's devices with tailscaled's
`whois`. The card's tailscaled may run **in userspace** (the firmware has no `/dev/net/tun`, or the tunnel did not come up): then there
is no `tailscale0`, and tailscaled hands a connection to the node's tailnet address on to `127.0.0.1` by itself (TRIMUI-BRICK-REMOTE.md). So:

- `web::net::plan_here` is `plan_userspace` on the card: with a `tailscale0` it is the usual plan; without one the companion listens on
  **127.0.0.1:80** (the Wi-Fi cannot reach it) and `server.rs` lets loopback peers in on that listener only.
- tailscaled's `whois` sees the loopback peer, so nobody is the owner: **every browser pairs with the six-digit code** on the Brick's screen
  (Settings > Web companion > Pair a browser, with the QR code). The page says so.
- The page's address is the node's name (`http://brick.<tailnet>.ts.net`) once tailscaled reports it; before that it reads "Waiting for
  Tailscale". A browser may also use the node's 100.x address as the Host (`SelfNode::addrs`), not other names (DNS rebinding stays refused).
- tailscaled's socket is the card's: `POCKETUI_TAILSCALED_SOCKET=$RUN/tailscaled.sock` (start.sh), `LocalApi::default` reads it.
- Not the phone page: that one (`phone_upload.rs`) is a separate, short-lived Wi-Fi server with a code in its path;
  it stays as it is, and its upload code is the companion's file code (`web::files`), so a file arrives by the same checks either way.

## Proof (host)

| What | Test |
|---|---|
| the rows are shown on the card, not on NextUI's | `app_settings_groups::brick_standalone_tests::the_standalone_card_shows_kids_screenshots_web_and_over_the_game` |
| the Tour's cards | `device::tests::the_brick_tour_leaves_out_cards_without_a_page` |
| wl_shm picture to RGB | `screenshot::tests::a_shm_picture_becomes_rgb_without_the_stride_padding` |
| `grim` against a real pocket-fb | `cargo test --features brick --test brick_fb the_grim_stand_in` |
| the loopback plan and the node's addresses | `web::tests::the_brick_card_listens_on_loopback_when_tailscale_runs_in_userspace` |
| the charge in the line | `osd_sensors::tests::the_charge_is_in_the_line_when_the_device_has_it` |
| a default build is as it was | the three features are `cfg(feature = "brick")` or card-only; `cargo test` without the feature passes |

## What only the Brick can show

- HOME + A in pocket-ui and in a game: the toast, the shutter, the file on the card, the gallery's list; `grim` against the real pocket-fb with the GPU on.
- The companion over userspace Tailscale: that tailscaled really hands the connection to 127.0.0.1:80 (and that nothing on the card already uses
  port 80), pairing from a phone, the Host check with the name and the 100.x address.
- Kids mode with the Brick's pad (SELECT + START hold) and the power key.
- The sensors line on the real fuel gauge (`axp2202-battery`: does it give `current_now`? Without it the line shows temperature and charge only).
