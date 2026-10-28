# PC streaming (Moonlight)

**PC streaming** on Home plays the games of your own computer on the Pocket, over Wi-Fi, with
[Moonlight](https://moonlight-stream.org). ROCKNIX already contains Moonlight, but the entry is **off until you turn it
on**: Settings → Look & feel → Look → **X** (Systems on the shelf) → **PC streaming**, the last row → **A**. It is an app, not a game:
**no pause menu**, no save slots, and it does not appear in your recent games.

> **Status:** the entry, the lookup and the way back to Home were checked on a Mini V2 without a computer: it looks
> for a PC for up to 12 seconds, says so when none answers, and returns to Home. **Pairing on the Pocket and a real
> stream were not tried** with a real PC, because no outside device may be paired during development; the pairing
> page was checked with a stand-in that prints what Moonlight Embedded 2.7 prints. Please report what you find.

## What you need

- A computer that serves games: **Sunshine** (free, any PC) or NVIDIA **GeForce Experience** with GameStream.
- The Pocket and the computer on the same network (for use away from home see [below](#away-from-home)).
- Wi-Fi switched on on the Pocket (Settings → Connections → Wi-Fi). Without it, A on the page says "Connect to Wi-Fi
  first" instead of opening the keyboard.

## Pair once

Pairing has to be done once per computer, on the Pocket: Settings → Games → **PC streaming**.

1. Start the gaming PC with Sunshine (or GeForce Experience) running.
2. On the Pocket: Settings → Games → **PC streaming** → **Pair a PC** → A. The keyboard asks for the PC's address. Press
   **START** with nothing typed to let Moonlight look for the PC on the network, or type its name or address (for
   example `192.168.1.20`) and press START.
3. A four-digit **PIN** appears in big digits. Enter it on the gaming PC: in Sunshine's web page
   (`https://localhost:47990` on the PC, the **PIN** tab), or in the box GeForce Experience shows.
4. The Pocket says *Paired with …* and gets the PC's games. The PC is now the one PC streaming on Home uses.

B stops a pairing that is waiting for the PIN; after three minutes it stops by itself. If no PC answers, the page says so:
check that it is on, on the same network, and type its address instead of letting Moonlight look.

The page lists every PC paired this way:

- **A on a PC** makes it the one PC streaming on Home uses, and fetches its list of games again.
- **X on a PC**, then X again, unpairs it: the PC forgets this Pocket. If the PC is off, it is taken off the list
  anyway; remove the Pocket in Sunshine's web page too (PIN page, clients).
- **The PC's games** follow the PCs in the list. **A on a game streams that game** instead of Moonlight's default.
  They are also written for ROCKNIX, as its old menu did, into `roms/moonlight` (one start file per game; they are
  not shown on the shelf).

Moonlight keeps its keys in `/storage/.cache/moonlight`. Pairing from a computer over SSH (`moonlight pair <address>`)
still works too.

## Start a stream

Home → **PC streaming** → **A**. The screen says *Looking for your PC*; when the computer answers, Moonlight's own
screen takes over. If the PC is off, not paired, or not on the same network, you are back on Home after at most 12
seconds with a note saying so.

- **What is streamed:** Moonlight's default application (it asks for one called *Steam*). To stream another one
  once, pick it in Settings → Games → PC streaming; to change the default, set `app = Desktop` (or any application name from
  the PC's list) in `/storage/.config/moonlight/moonlight.conf`.
- **Which PC:** the one chosen in Settings → Games → PC streaming (the last one paired). Without one, the one Moonlight finds
  by itself on the network, or `address = <PC address>` in the same file.
- **Picture and sound:** the file also has the resolution (1240 x 1080 on the Mini V2, set at every start of the
  system), the frame rate (60), the bitrate (15 Mbit/s) and the codec (h264). Raise the bitrate for a sharper
  picture on a good network.
- **Controller:** the Pocket's buttons are sent as an Xbox controller. A custom layout can go into
  `/storage/.config/moonlight/gamecontrollerdb.txt`.

## Leaving

There is no HOME menu during a stream: pocket-ui does not react to HOME while Moonlight runs. To stop,
hold **START + SELECT + L1 + R1** (Moonlight's own combination; ROCKNIX's wiki gives L1 + L2 + SELECT + START, so try
that one if the first does not work). pocket-ui is back on Home, and the application on the PC is closed
(`quitappafter` is on).

The power button still puts the Pocket to sleep. The power menu (long press) is not available during a stream:
leave the stream first.

## Away from home

Moonlight works over the internet with a VPN. The easiest is Tailscale, which pocket-ui switches on and signs in
under Settings → Connections → [Tailscale](remote.md); ROCKNIX's wiki also describes WireGuard:
[ROCKNIX: Moonlight](https://rocknix.org/systems/moonlight/). Once the VPN is up, set `address` in
`moonlight.conf` to the PC's VPN address.

## If it does not start

| What you see | Try |
|---|---|
| *Couldn't start the stream*, after 12 seconds | The PC did not answer: is it on, is Sunshine (or GeForce Experience) running, is it on the same network? Set `address` in `moonlight.conf` |
| *Couldn't start the stream*, at once | Moonlight is not installed or cannot run: look in `/var/log/pocket-ui.log` for the line `pocket-ui: no PC answered the lookup` |
| The PC asks for a PIN again | The pairing was lost: pair again in Settings → Games → PC streaming |
| *No PC answered* while pairing | The PC is off, Sunshine isn't running, or the search doesn't reach it: type the PC's address instead |
| The stream stutters | A weaker Wi-Fi band or too high a bitrate: lower `bitrate` in `moonlight.conf` |
