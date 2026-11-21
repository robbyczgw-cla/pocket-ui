# PC streaming (Moonlight)

**PC streaming** lets you stream games straight from your gaming PC to your Pocket over Wi-Fi using
[Moonlight](https://moonlight-stream.org). ROCKNIX ships Moonlight built in, and the shelf entry stays **off until you
enable it**: Settings → Games → **Systems on the shelf** → **PC streaming** (last row) → **A**. Because it's a
streaming app rather than an emulator, there's **no pause menu**, no save slots and no recent-games entry.

> **Status:** the shelf tile, PC lookup and return to Home were verified on a Mini V2 without a host PC (searches up to
> 12 seconds, reports if none responds and returns to Home). **Live pairing and streaming haven't been tested against a
> real PC yet** (pairing was verified with a mock matching Moonlight Embedded 2.7). Let us know how it runs on your
> setup.

## What you need

- A host PC running **Sunshine** (free, works on any GPU) or NVIDIA **GeForce Experience** with GameStream.
- Your Pocket and PC on the same network (for streaming away from home, see [Away from home](#away-from-home)).
- Wi-Fi connected on your Pocket (Settings → Connections → Wi-Fi). If Wi-Fi is off, pressing A prompts "Connect to
  Wi-Fi first".

## Pair once

Pair each gaming PC once under Settings → Connections → **PC streaming**:

1. Make sure your PC is awake with Sunshine (or GeForce Experience) running.
2. On your Pocket, open Settings → Connections → **PC streaming** → **Pair a PC** → A. Leave the field blank and press
   **START** to auto-discover your PC on the network, or type its hostname/IP (such as `192.168.1.20`) and press START.
3. A large four-digit **PIN** appears on screen. Enter it on your PC, in Sunshine's web UI (`https://localhost:47990`,
   **PIN** tab) or the GeForce Experience prompt.
4. Your Pocket confirms *Paired with …*, pulls your PC's app list and sets it as the default host for Home's PC
   streaming tile.

Press B to cancel while waiting for a PIN (pairing also times out after three minutes). If auto-discovery misses your
PC, double-check it's on the same network and enter its IP address directly.

All paired PCs appear right on this page:

- **A on a PC:** makes it the active PC for Home's PC streaming tile and refreshes its games list.
- **X on a PC** (press X again to confirm): unpairs it. If the PC is offline it's still removed locally; just clear the
  Pocket in Sunshine's PIN/clients tab later too.
- **Your PC's games** appear below the PCs. Press **A on any game to stream it directly** instead of Moonlight's default
  app. Entries also sync to `roms/moonlight` for ROCKNIX (kept hidden from your shelf).

Moonlight stores pairing keys in `/storage/.cache/moonlight`. Pairing over SSH (`moonlight pair <address>`) still works
too.

## Start a stream

Select **PC streaming** on Home and press **A**. You'll see *Looking for your PC*, then Moonlight takes over as soon as
your PC answers. If the PC is asleep, unpaired or unreachable, you return to Home within 12 seconds with a clear note.

- **Default app:** Moonlight launches *Steam* by default. Launch another app once from
  Settings → Connections → PC streaming, or set `app = Desktop` (or any app name from your PC) in `/storage/.config/moonlight/moonlight.conf`.
- **Host PC:** uses whichever PC is selected in Settings → Connections → PC streaming (most recently paired by default),
  auto-discovery, or `address = <PC address>` in `moonlight.conf`.
- **Video settings:** `moonlight.conf` also sets resolution (1240 × 1080 on the Mini V2, updated each boot), frame rate
  (60), bitrate (15 Mbit/s) and codec (`h264`). Bump `bitrate` up on fast Wi-Fi for a crisper stream.
- **Controls:** your Pocket maps as an Xbox controller. Need custom binds? Drop a `gamecontrollerdb.txt` in
  `/storage/.config/moonlight/`.

## Leaving

pocket-ui ignores HOME while Moonlight is streaming, so there's no pause menu. To end a stream, hold
**START + SELECT + L1 + R1** (Moonlight's exit combo; if that doesn't trigger, try ROCKNIX's wiki combo
**L1 + L2 + SELECT + START**). You return straight to Home and the streamed app closes on your PC (`quitappafter` is
enabled).

A short press on Power still puts your Pocket to sleep. The long-press power menu is disabled mid-stream; exit the stream
first.

## Away from home

You can stream over the internet through a VPN. The easiest setup is Tailscale under Settings → Connections →
[Tailscale](remote.md) (ROCKNIX's wiki also covers WireGuard:
[ROCKNIX: Moonlight](https://rocknix.org/systems/moonlight/)). Once connected, put your PC's VPN address under `address`
in `moonlight.conf`.

## If it does not start

| What you see | Try |
|---|---|
| *Couldn't start the stream*, after 12 seconds | Your PC didn't answer: verify it's awake, Sunshine (or GeForce Experience) is running and both are on the same network, or set `address` in `moonlight.conf` |
| *Couldn't start the stream*, at once | Moonlight isn't installed or failed to launch: check `/var/log/pocket-ui.log` for `pocket-ui: no PC answered the lookup` |
| The PC asks for a PIN again | Pairing expired or reset: re-pair in Settings → Connections → PC streaming |
| *No PC answered* while pairing | PC is off, Sunshine isn't running or discovery is blocked: type the PC's IP address directly |
| The stream stutters | Weak Wi-Fi signal or bitrate too high: lower `bitrate` in `moonlight.conf` |
