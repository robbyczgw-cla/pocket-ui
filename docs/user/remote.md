# Reach your Pocket from anywhere with Tailscale

Settings → Connections → **Tailscale** puts the Pocket on your [Tailscale](https://tailscale.com) network (a "tailnet"): a private
network between your phone, your computer and the Pocket that works over any Wi-Fi, a phone hotspot or mobile data,
without opening anything on your router. ROCKNIX already contains Tailscale; this page turns it on, signs it in and
shows its state.

> **Status:** this page was built and tried in the virtual device with stand-ins for `tailscale` and `systemctl`
> (`scripts/vdev-fake-tailscale.sh`): the switch, the QR sign-in, the auth key, the name, the log out and the state
> after a restart of pocket-ui. Its sign-in has **not** been run on a Mini V2 against a real tailnet yet (a Mini V2 on
> a real tailnet did carry the [web companion](web-companion.md)); the checklist is in
> [SECURITY-HARDENING.md](../SECURITY-HARDENING.md#tailscale-settings--tailscale). Please tell us what happens.

## What Tailscale does here, and what it does not

- It gives the Pocket an address on your tailnet (`100.x.y.z`) and a name (`pocket-mini` by default), reachable from
  every device of yours that has Tailscale, wherever they are.
- It is **only the network path**. What you can do over it is decided by the other switches: SSH needs Settings → Connections →
  **Remote access (SSH)** to be on and uses its password; file sharing and Syncthing keep their own passwords.
  Tailscale's own SSH server is **not** used (pocket-ui always starts it with `--ssh=false`), so nothing answers on
  port 22 except the SSH of Remote access.
- Who on your tailnet may reach the Pocket is set in the Tailscale admin console (access controls), not on the Pocket.

## Turn it on and sign in

1. Settings → Connections → **Tailscale**. Select **Tailscale** and press **A**. The service starts; the status says **Logged out**.
2. Select **Sign in** and press **A**. A QR code appears.
3. Scan it with your phone's camera (or type the short address under the code into any browser), sign in to Tailscale
   and approve the Pocket. The page returns to the list by itself and shows **Connected as** your tailnet, the Pocket's
   Tailscale address, its name and how many of your devices are online.
4. B on the QR code cancels. Nothing is changed on your tailnet until you approve.

The switch survives restarts: it sets ROCKNIX's own setting `tailscale.up`, which ROCKNIX reads at every boot, so the
Pocket reconnects by itself (the sign-in is kept in `/storage/.cache/tailscale`). Switching it off stops the service
and sets the setting back; the sign-in stays, so switching on again reconnects without a new QR code.
Tailscale is not tied to a timer: it stays on until you switch it off.

### With an auth key instead

**Sign in with a key** is for when no phone is at hand. Make a key in the Tailscale admin console (under Keys),
type it with the on-screen keyboard (it is long: A types, Y switches to symbols, X is Shift, START is Done) and the
Pocket signs in without a browser. The key is put in a file only root can read for the moment `tailscale up` needs it
and removed afterwards; it is never kept and never on a command line. Use a key that expires, and a single-use one if you can.

### Name

**Name** is what the Pocket is called on your tailnet (letters, digits and hyphens; `pocket-mini` is suggested). It is
applied at once while the service runs and used at the next sign-in otherwise. It does not change the Pocket's name
anywhere else.

### Log out

**Log out** asks twice (A, then A again; any other button cancels it). The Pocket logs out of your
tailnet; you can then sign in again, to the same account or another one. Remove the old entry in the admin
console if you do not want it listed.

## Reaching the Pocket

From a device on your tailnet, with Settings → Connections → Remote access (SSH) switched on:

```
ssh root@pocket-mini
```

(or `ssh root@100.x.y.z` with the address the page shows). The password is the one Remote access shows. Remote access
still turns itself off after 30 minutes without a connection and after a restart; Tailscale does not change that.
This is also the way back in when the Pocket has lost its Wi-Fi: put the phone's hotspot on, connect the Pocket to it
in Settings → Connections → Wi-Fi, and the Pocket and your computer (on Tailscale, wherever it is) can see each other.

Moonlight can use the tailnet too: put the PC's Tailscale address in `moonlight.conf` ([PC streaming](pc-streaming.md)).

The [web companion](web-companion.md) works only over the tailnet: your library, uploads and play time in a browser.

## What the status says

| Status | Meaning |
|---|---|
| **Off** | The service is stopped. If *Tailscale* says On after a restart, ROCKNIX is starting it |
| **Starting** | The service runs and has not answered yet |
| **Logged out** | Running, not signed in. Use Sign in |
| **Waiting for approval** | Signed in, but your tailnet needs an admin to approve new devices |
| **No network yet** | The service is on, but the Pocket has no network: Wi-Fi is off or not joined, and no cable is plugged in. Not an error: see below |
| **Connecting** | Signed in, not yet connected |
| **Not connected** | Signed in, but paused (`tailscale down` was used). Switching it off and on again brings it back |
| **Connected as ...** | On the tailnet |
| **Not installed** | This system image has no Tailscale |

### Without a network

With Wi-Fi off or not joined (and no cable), the page says **No network yet** in calm grey, with *Tailscale connects once
you're online*, and the Settings row says the same. Nothing is red, even though the service still reports *Running*
with its old address for a while: that is not shown as *Connected*. Your choice stays as it was (the switch still says
On, the sign-in is kept), and nothing has to be pressed when the network is back: the page checks again every couple of
seconds, Tailscale reconnects by itself, and anything that failed meanwhile (a timeout, a half-done sign-in) is cleared
as the network goes and as it returns.

What needs Tailscale's servers waits: **Sign in**, **Sign in with a key** and **Log out** say *Connect to Wi-Fi first*
instead of starting and timing out. Switching on works offline (it sets the boot setting and starts the service, which
connects once the network is there), and so does changing the name.

The network counts as there when Wi-Fi is joined to an access point or a cable (a USB Ethernet adapter) has a link. A
Wi-Fi that is joined but has no internet (a hotspot out of data, a sign-in page) still counts as there; the sign-in
page of such a network is handled by [Wi-Fi sign-in pages](hotel-wifi.md) before Tailscale can connect.

If something fails while you are online, the page shows the reason in red (for example *tailscale did not answer*: the service is stuck, or not running properly;
switch it off and on). Every call to Tailscale has a time limit; the menu never waits for one.

## Where things are kept

| What | Where |
|---|---|
| On at boot | `tailscale.up` in ROCKNIX's `system.cfg` |
| The sign-in | `/storage/.cache/tailscale/tailscaled.state` |
| The name you chose | `tailscale.json` next to the other pocket-ui settings |
| Service port and flags | `/storage/.config/tailscaled.defaults` (ROCKNIX's file; pocket-ui only makes it when it is missing) |
