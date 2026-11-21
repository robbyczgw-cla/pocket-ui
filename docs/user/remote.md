# Reach your Pocket from anywhere with Tailscale

Settings → Connections → **Tailscale** connects your Pocket to your private [Tailscale](https://tailscale.com) network
("tailnet")—linking your phone, PC and Pocket across any Wi-Fi, hotspot or mobile network without opening router ports.
Tailscale is already built into ROCKNIX; this page lets you toggle it on, sign in and check status.

> **Status:** built and verified on the virtual device with mocked `tailscale` and `systemctl`
> (`scripts/vdev-fake-tailscale.sh`) covering toggle, QR login, auth keys, naming, log out and restarts. Live sign-in on
> a Mini V2 hasn't been run against a real tailnet yet (a Mini V2 on a real tailnet has already hosted the
> [web companion](web-companion.md)); see the checklist in
> [SECURITY-HARDENING.md](../SECURITY-HARDENING.md#tailscale-settings--tailscale). Let us know how it behaves on your
> tailnet.

## What Tailscale does here, and what it does not

- Gives your Pocket a private tailnet IP (`100.x.y.z`) and hostname (`pocket-mini` by default), reachable from any of
  your Tailscale devices anywhere.
- **It's just the network tunnel.** What's reachable over it still depends on your own toggles: SSH requires Settings →
  Connections → **Remote access (SSH)** and its password, while File sharing and Syncthing keep their own passwords.
  Tailscale SSH is always disabled (`--ssh=false`), so port 22 only runs Remote access SSH.
- Access permissions across your tailnet stay under your control in the Tailscale admin console (ACLs), not on the
  Pocket.

## Turn it on and sign in

1. Open Settings → Connections → **Tailscale**, highlight **Tailscale** and press **A**. The service starts and shows
   **Logged out**.
2. Highlight **Sign in** and press **A** to display a QR code.
3. Scan it on your phone (or open the short URL underneath in any browser), sign in to Tailscale and approve your
   Pocket. The page automatically updates to **Connected as** your tailnet, showing the Pocket's Tailscale IP, hostname
   and how many of your devices are online.
4. Press B on the QR code to cancel—nothing changes on your tailnet until you approve.

Tailscale stays enabled across reboots by setting ROCKNIX's `tailscale.up` flag (credentials live in
`/storage/.cache/tailscale`). Switching it off stops the service while keeping your sign-in, so turning it back on
reconnects without scanning a QR code again. Tailscale has no auto-off timer; it stays on until you switch it off.

### With an auth key instead

No phone handy? Use **Sign in with a key**. Generate a key under Keys in the Tailscale admin console and enter it with
the on-screen keyboard (A types, Y switches to symbols, X is Shift, START finishes) to sign in without a browser. The
key is written briefly to a root-only file for `tailscale up` and deleted immediately after—never stored or passed on a
command line. Prefer an expiring, single-use key when possible.

### Name

**Name** sets your Pocket's hostname on your tailnet (letters, digits and hyphens; `pocket-mini` is suggested). It
updates right away if Tailscale is running, or on next sign-in otherwise, without renaming your Pocket anywhere else.

### Log out

**Log out** asks twice (press A, then A again; any other button cancels). Your Pocket signs out of your tailnet so you
can log back into the same or another account. You can also remove the old entry in the Tailscale admin console.

## Reaching the Pocket

From any device on your tailnet with Settings → Connections → Remote access (SSH) turned on:

```
ssh root@pocket-mini
```

(or `ssh root@100.x.y.z` using the IP shown on screen) with the password from Remote access. Remote access stays on
until you turn it off or reboot; Tailscale doesn't change that. This also gives you an easy lifeline if home Wi-Fi drops:
join your phone's hotspot in Settings → Connections → Wi-Fi and your Pocket is right back on your tailnet.

Moonlight works over Tailscale too—just put your PC's Tailscale IP in `moonlight.conf`
([PC streaming](pc-streaming.md)).

The [web companion](web-companion.md) also works over Tailscale and home Wi-Fi—your library, uploads and play history in
any browser, with no pairing code needed from your own Tailscale devices.

## What the status says

| Status | Meaning |
|---|---|
| **Off** | Service is stopped. If *Tailscale* reads On right after boot, ROCKNIX is still starting it |
| **Starting** | Service is running and hasn't answered yet |
| **Logged out** | Running, not signed in yet—select Sign in |
| **Waiting for approval** | Signed in, waiting for an admin to approve the device in your tailnet |
| **No network yet** | Tailscale is On, but no Wi-Fi or Ethernet is connected yet. Not an error (see below) |
| **Connecting** | Signed in and bringing up the tunnel |
| **Not connected** | Signed in, but paused (`tailscale down`). Toggle Off and On to reconnect |
| **Connected as ...** | Connected to your tailnet |
| **Not installed** | Tailscale isn't included in this OS image |

### Without a network

When Wi-Fi is off or disconnected (and no Ethernet cable is plugged in), the page calmly shows **No network yet** in
grey with *Tailscale connects once you're online*—nothing flashes red, even if the daemon briefly reports its old
address. Your toggle stays On and your sign-in stays saved; once you're back online, the page rechecks every few
seconds, Tailscale reconnects automatically and any temporary offline errors clear themselves.

Actions that need Tailscale's servers (**Sign in**, **Sign in with a key**, **Log out**) tell you *Connect to Wi-Fi
first* instead of hanging. You can still turn Tailscale On (it connects once online) and rename your Pocket offline.

Your Pocket counts as online once joined to Wi-Fi or plugged into USB Ethernet. If a hotel Wi-Fi is joined but gated by
a login screen, clear it first via [Wi-Fi sign-in pages](hotel-wifi.md) so Tailscale can connect.

If something fails while online, the reason appears in red (such as *tailscale did not answer*—toggle Tailscale Off and
On). Every Tailscale call has a timeout so menus never freeze.

## Where things are kept

| What | Where |
|---|---|
| Auto-start at boot | `tailscale.up` in ROCKNIX's `system.cfg` |
| Sign-in state | `/storage/.cache/tailscale/tailscaled.state` |
| Tailnet hostname | `tailscale.json` beside other pocket-ui settings |
| Daemon port & flags | `/storage/.config/tailscaled.defaults` (ROCKNIX's file; created by pocket-ui only if missing) |
