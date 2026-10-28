# Wi-Fi bench

The real Wi-Fi stack of ROCKNIX 20261001 (NetworkManager 1.51.4 with the iwd 3.12 backend, systemd-resolved, wifictl)
and the real pocket-ui, against simulated radios and access points: the Linux `mac80211_hwsim` module and hostapd.
It repeats what the three Wi-Fi incidents of 2026-10-26/27 went through (no networks after a resume, a phone's hotspot
that went away and came back, a fresh card where a wrong password never let go), without a device and as often as
needed. Each scenario says what the player sees on the Wi-Fi page and what NetworkManager and iwd are left with.

```bash
scripts/wifi-bench/run.sh                     # build pocket-ui for aarch64, run every scenario on the Mac, fetch results
scripts/wifi-bench/run.sh --no-build password # one scenario with the binaries already built
scripts/wifi-bench/run.sh --bin ../other/target/aarch64-unknown-linux-gnu/release --label other all
```

A build from another branch (`--bin`) needs the bench's one line in `App::draw` (below, "How close to the device"),
or pocket-ui writes no text and every check of the page fails: `git diff main test/wifi-bench -- src/main.rs | git apply`
in that branch's checkout before building it (the fix branch's first run was made that way).

Exit code 0 when every check passed. Results: `wifi-bench-results/<date>-<time>-<label>/` next to the checkout
(`WIFI_BENCH_RESULTS`), one folder per scenario (below). Where it runs is `WIFI_BENCH_HOST` (an SSH host, or `local`),
from the environment or from `scripts/local.env` (untracked, see `scripts/local.env.example`).

## Where it runs

`mac80211_hwsim` is a kernel module, so the bench needs root on a Linux kernel. It runs in the **colima VM of a Mac**
(Apple silicon, colima's Ubuntu 24.04 with kernel 6.8, the Docker engine the virtual device uses already,
[VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md)): root in that VM is `colima ssh -- sudo`. A build machine without root can't
load the module; `WIFI_BENCH_HOST=local` is for a Linux machine where Docker runs with root.

The module ships in Ubuntu's `linux-modules-extra-<kernel>`, not in the VM's base image. The bench installs it in the
VM the first time and loads it with `radios=0` (`lib.sh`, `ensure_hwsim`); after a colima VM is made anew it does that
again. Nothing on the Mac changes, nor the VM's own network: every radio, address and access point lives in network
namespaces of the bench's two containers. Other virtual devices on the same engine are untouched (they see the radios'
rfkill entries without a device behind them, which pocket-ui and wifictl skip).

```
colima VM (kernel 6.8 + mac80211_hwsim)
├── wb-ap      Debian trixie: hostapd on radios wbap1..6, bridge br0 10.77.0.1/24, dnsmasq (DHCP, DNS: every name is
│              10.77.0.1), internet.py (answers "NetworkManager is online") or scripts/portal-standin.py behind an
│              nft redirect of port 80 (the captive portal)
└── vdev-wb    the virtual device (scripts/vdev.sh, ROCKNIX 20261001 userland) with --network none: only lo and the
               radio wbsta as wlan0; ROCKNIX's systemd-udevd, iwd, NetworkManager, systemd-resolved, wifictl, rfkill,
               and the real pocket-ui on sway, writing what it shows (POCKETUI_VISIBLE_TEXT)
```

All seven radios are made in `wb-ap` (`hwsim.py`: hwsim's generic netlink, no extra tools); radios hear each other only
when they were made in the same namespace, and `iw phy wbsta set netns` keeps that when the station's radio moves into
the device.

## First time

Needs what the virtual device needs on the Mac (colima, the `pocket-vdev-base:20261001` image, `scripts/vdev.sh up`
works there; [VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md)), SSH to it, and `cargo zigbuild` on the build machine. `run.sh` does the rest: it builds
`pocket-ui` and `pocket-core`, stages the few files the bench reads (`scripts/vdev.sh`, `scripts/wifi-bench/`,
`scripts/portal-standin.py`, `testdata/portal/`, `docker/`, `deploy/run-pocket-ui.sh`, the two binaries) into
`~/wifi-bench-run` on the Mac, installs and loads the module if needed, builds the access point image
(`wifi-bench-ap:<hash>`, about 20 s once) and runs `bench.sh` there.

A scenario takes 40 s to 5 min (the hotspot waits for real minutes), all of them about 15 min.

## By hand

On the Mac, in `~/wifi-bench-run` (after one `run.sh`), with `PATH=/opt/homebrew/bin:$PATH`:

```bash
sh scripts/wifi-bench/bench.sh up                                 # both containers, radios, services, pocket-ui
sh scripts/wifi-bench/bench.sh ap start wbap1 Home secret-pass 6  # an access point (SSID, password or -, channel)
sh scripts/wifi-bench/bench.sh ui start down down down a a        # pocket-ui's FIFO: Settings → Connections → Wi-Fi
sh scripts/wifi-bench/bench.sh frame                              # the text on screen now
sh scripts/wifi-bench/bench.sh type secret-pass                   # on the on-screen keyboard (0-9 a-z - _ . @)
sh scripts/wifi-bench/bench.sh ap stop wbap1                      # the access point goes away
sh scripts/wifi-bench/bench.sh ap portal clickthrough             # captive portal instead of the internet (ap internet: back)
sh scripts/wifi-bench/bench.sh ap dhcp off                        # DHCP stops answering (on: again)
sh scripts/wifi-bench/bench.sh sh 'nmcli dev; iwctl station wlan0 show'
sh scripts/wifi-bench/bench.sh down                               # remove both, logs into results/manual
```

`vdev.sh shot`, `log` and `sh` work too with `VDEV_NAME=wb`.

## How close to the device

The same binaries and configuration files as ROCKNIX 20261001: `/etc/NetworkManager/NetworkManager.conf` (iwd backend,
keyfile, internal DHCP, `dns=systemd-resolved`), `/etc/iwd/main.conf` (`EnableNetworkConfiguration=false`,
`ControlPortOverNL80211=true`, `MaximumPeriodicScanInterval=30`), the units' arguments and `STATE_DIRECTORY`, the
profiles in `/storage/.config/NetworkManager/system-connections`, iwd's state in `/storage/.cache/iwd`, ROCKNIX's
`wifictl`, `rfkill` and `system.cfg` (copied from `/usr/config` as the first boot does; fresh: `wifi.enabled=0`).
The files come from the ROCKNIX image the virtual device is made from; the freshly flashed card of the incident was
not reachable when the bench was built, its log shows the same NetworkManager 1.51.4.

What differs, and why:

| Device | Bench |
|---|---|
| systemd starts udevd, iwd, NetworkManager, resolved | `sta-services.sh` starts the same binaries with the units' arguments; NetworkManager with `--debug` instead of `--no-daemon` (the same daemon, logging to stderr: the container has no journal) |
| iwd replaces the chip's interface by its own, named after the phy (`wlan0` on phy0) | `[DriverQuirks] DefaultInterface=mac80211_hwsim` appended to iwd's `main.conf`: for the simulated driver only, iwd keeps the radio's own interface, renamed `wlan0` (the bench's phy has index 20 and more, iwd would call it `wlan23`) |
| ath11k on the board, real air, signal that changes | hwsim: every radio hears every other at full strength; no fading, no interference. Roaming is forced by switching the access point in use off |
| one Wi-Fi rfkill entry | one per radio, all in one kernel: `rfkill block wifi` (wifictl disable) would switch the access points off too. The bench blocks only the station's entry; pocket-ui's unblock (`rfkill unblock wifi`) unblocks all, which is harmless |
| the boot blocks the radio after NetworkManager has started (the fresh card: NetworkManager at 4 s, wlan0 unavailable at 15.6 s) | the same order. A block before NetworkManager starts is lifted by NetworkManager itself (its wish is on without a state file) |
| ROCKNIX's autostart, EmulationStation, sleep and resume | not there: no suspend; "blocked at boot or resume" is an rfkill block at that moment |
| internet | `internet.py` on the gateway answers every web request with "NetworkManager is online"; DNS answers every name with the gateway. Nothing leaves the bench |

What the player sees comes from pocket-ui itself: with `POCKETUI_VISIBLE_TEXT` set, the running pocket-ui writes the
strings painted into each frame it draws (the transcript of `--snapshot` and the journey driver, `text::boxes`; the
one line in `App::draw` is the bench's only change to pocket-ui and does nothing without the variable). The bench
drives pocket-ui through its command FIFO like a player with the D-pad, types passwords on the on-screen keyboard
(`type_text` works out the moves from `src/keyboard.rs`'s layout) and checks the transcript.

## Scenarios

| Scenario | What happens | What it checks | Incident |
|---|---|---|---|
| `mesh` | Six access points, one name (`BenchMesh`), one password, channels 1/6/11; connect, then the one in use is switched off, twice | one row for six access points; connected; on another access point within 60 s each time, with an address; the page still says connected; Connections names the network; one profile | fresh card (its "Wifi" had six) |
| `hotspot` | A phone's hotspot, connected, the player back on the shelf; the hotspot goes away for 90 s (longer than the watchdog's 60 s) and comes back | NetworkManager notices; connected again within 90 s of the return; nothing restarts a connection NetworkManager had already brought back (`new-activation`); still connected 20 s later; the page and Connections say so | hotspot, 2026-10-27 |
| `portal` | An open café network behind the stand-in's click-through portal | the page says connected and shows "This network wants a sign-in" with the portal's host; Connections names the network meanwhile; the sheet, Sign in… loads the café's page at the first try, Send, "You're online", the row goes away, Connections names the network | captive portal (PORTAL-SIGNIN.md) |
| `password` | One access point; the player types a wrong password, then presses A again and types the right one | "Wrong password. Try again."; NetworkManager not left connecting; a network that never connected not shown as Saved; A asks for the password without trying the wrong one again; connected; one profile | fresh card |
| `password-mesh` | The same on six access points with one name | as `password` | fresh card |
| `forget` | Connected with the old password; the router's password changes; while NetworkManager keeps trying the saved profile, the player forgets it (Y, Y); then the new password | no error about forgetting on the page or in the log; no profile and no `BenchHome.psk` left; NetworkManager stops trying; connected with the new password; one profile | fresh card (forget at 299 s while NetworkManager was in config) |
| `rfkill` | A fresh card: `wifi.enabled=0`, the radio blocked at boot | "Wi-Fi is off" and pocket-ui leaves it off; A turns it on; the network appears; `wifi.enabled=1` stored; connected | fresh card's start |
| `rfkill-on` | `wifi.enabled=1`, the radio blocked at boot (a boot or resume hook) | pocket-ui switches it on by itself and says why in its log; the network appears; the page doesn't say off; connected | 2026-10-26 ("no networks at all") |

A check is what the player should see, or what NetworkManager and iwd should be left with. A FAIL is a reproduction,
not necessarily a decision about the fix: the scenario's log holds everything to look at.

## Results

`<results>/<scenario>/`:

| File | What |
|---|---|
| `result.txt` | `PASS`/`FAIL` per check, `NOTE` lines (times, states, what pocket-ui logged) |
| `fails.txt` | the text on screen at each FAIL |
| `shots/NN-name.jpg`, `.txt` | screenshots (JPEG, 1240 px) and the text on screen, in order |
| `logs/pocket-ui.log`, `iwd.log`, `NetworkManager.log`, `resolved.log` | the device's side, with the uptime in front |
| `logs/nm-end-state.txt` | `nmcli dev`, the profiles, iwd's and NetworkManager's files at the end (keys hidden) |
| `logs/ap/` | hostapd per access point, dnsmasq (DHCP and DNS), the portal's requests |
| `run.log`, `../summary.txt` | the scenario's own output; one line per scenario |

`BUILD` in the run's folder names the binary (commit and md5).

## First runs (2026-10-27)

main at 84bb401 and `fix/wifi-wrong-password-iwd` at 7d06e77, each with the one-line transcript hook
(`BUILD`: md5 b87d1b09 and f8ce0173). Results in `wifi-bench-results/` (next to the checkout on the build machine): `20261027-221539-main`
(all), `20261027-224806-main` (password, password-mesh, mesh again), `20261027-222726-fix-wifi-wrong-password-iwd`
(all; its password, password-mesh and mesh are superseded by `20261027-224412-fix-…`, whose scenario follows the fix's
keyboard that stays open), and two earlier main runs (`215453`, `220911`) with the same results apart from the
intermittent checks.

| Scenario | main | fix branch | What failed |
|---|---|---|---|
| mesh | roaming PASS (9–11 s to the next access point, twice, every run); FAIL 3 of 4 runs on the Connections row | the same (2 of 2) | Connections row (below) |
| hotspot | **FAIL** (3 of 3) | **FAIL** (1 of 1) | offline about 95 s after the hotspot is back (below) |
| portal | PASS 1 of 3; FAIL on the Connections row (2 of 3) and on the first Sign in… (2 of 3) | FAIL on the Connections row only | Connections row; first Sign in… (below) |
| password | **FAIL** (4 of 4) | PASS (1 of 1) | a wrong password leaves a "Saved" network that A retries (below) |
| password-mesh | **FAIL** (3 of 3) | PASS (1 of 1) | the same on six access points |
| forget | PASS | PASS | |
| rfkill | PASS | PASS | |
| rfkill-on | PASS | PASS | |

**A wrong password (main).** After "Wrong password. Try again." the network's row says **Saved**: NetworkManager keeps
the profile and iwd a `BenchHome.psk` with the wrong key. A on the row runs `connect saved` with that profile, which
fails ("password for '802-11-wireless-security.psk' not given in 'passwd-file'"); only then does the page ask for the
password, and the right one connects (one profile left). The same with one access point and with six. What the fresh
card did after that (the right password refused again with "Secrets were required", a forget of an unknown profile)
did not come back here. On the fix branch the profile is removed at once ("refused the password; its profile is
removed"), the keyboard stays under the message, the right password connects.

**A phone's hotspot that comes back (main and fix).** Measured in `20261027-215453-main/hotspot` (hostapd's, iwd's and
NetworkManager's times): the hotspot goes at 0 s, NetworkManager drops the link at 5.6 s and iwd tries once
(connect-timeout). At 25.6 s pocket-ui's watchdog connects the saved network while the hotspot is still gone ("offline
for 45 s; connecting to saved network … (try 1)"); iwd's timeout comes back from NetworkManager as `no-secrets` and
pocket-ui logs "Secrets were required, but not provided". The hotspot is back at 95.8 s; iwd stays in
`autoconnect_full` and NetworkManager starts nothing; the Pocket is offline until the watchdog's third try at 191.3 s
("offline for 210 s … (try 3)", "back after 3 reconnect(s)"): **95 s offline with the hotspot back**. The other runs
(main twice more, the fix branch once) show the same lines. Whether iwd's back-off after the failed tries or NetworkManager's
blocked autoconnect after `no-secrets` keeps them still is not settled by these logs. No `new-activation` here: the
2026-10-27 hotspot incident's restart of a link that had just come back did not show.

**Connections says "Not connected" while connected (main and fix, intermittent).** Settings → Connections reads its
facts when the list is painted, at most every 5 s (`settings_values::refresh`), with "connected" as the status
corner last saw wlan0 (`operstate` up; its thread sleeps on pages without the corner, such as the Wi-Fi page). Opened
after a connect, the list shows the old "Not connected"; the corner notices within 3 s and repaints, but that paint
is within 5 s of the first read; the next paint is the clock's minute. So the row is wrong for up to a minute (seen
3 s to 28 s later); a check within 15 s fails unless the minute falls into it.

**The first Sign in… says "No answer" (main, 2 of 3 runs; not on the fix branch's one run, not by hand).** The
helper's DNS lookups of `login.wb-hotel.example` were answered (dnsmasq's log), but no request reached the stand-in;
Try again 5 s later failed too; the page loaded after the next 15-s re-check. A plain `curl` of the same address from
the device works from the first second after connecting, so the network path is not it. Not explained yet.

## The fixes (2026-10-28, `fix/wifi-hotspot-reconnect`)

Results `20261027-235256-fix2-a` … `20261028-005554-fix2-p3`; the last build (e2a0b72) passed every scenario, portal three
times, hotspot five times over the branch's builds (the watchdog unchanged since the first).

| Scenario | before (main 84bb401) | after |
|---|---|---|
| hotspot | FAIL 3 of 3: back 91–95 s after the return, "Secrets were required" in the log | PASS 5 of 5: back 4, 5, 4, 21, 4 s after the return; no try while it was gone |
| portal | first Sign in… "No answer" 2 of 3; Connections row 2 of 3 | PASS 3 of 3 (first Sign in…, Connections) |
| mesh | Connections row FAIL 3 of 4 | PASS 2 of 2 |
| password, password-mesh, forget, rfkill, rfkill-on | (fixed on main since) | PASS |

**Hotspot.** By hand on the bench: a `connection up` of the saved hotspot while iwd still listed the access point that
had gone (iwd scans again only later) ends "Secrets were required" (iwd's connect times out, NetworkManager calls that
`no-secrets`), and after that second failed connect to the same access point iwd's autoconnect leaves it alone well
past its return (still offline 120 s after it; its `AutoConnect` stays yes, so not NetworkManager's block); without the
second try iwd came back by itself 25 s after the return. The watchdog now counts only networks the radio heard since
the drop (`iw dev wlan0 scan dump`'s "last seen"), asks for a scan at each 15-s look for the first 10 min offline (iwd
then finds the hotspot and connects by itself within seconds), and connects at once when a saved network is heard
again. A saved connect the player starts that ends "Secrets were required" while the network isn't heard says "out of
range" instead of asking for the password.

**Connections row.** The settings thread reads wlan0's `operstate` itself instead of the status corner's last look,
and reads again at once when the shared `net_state` names another network or address, or the corner's Wi-Fi changed.

**First Sign in….** The helper logs why a request failed (`portal:` in pocket-ui's log): "wasn't looked up in 5 s".
The café network's DNS answered the A question at once, glibc asked again 5 s later (its own timeout), the helper had
given up at exactly 5 s. Why the first answer never reached glibc (systemd-resolved, just after a burst of AAAA
questions the bench's DNS refuses) is not settled; an IPv4-only lookup failed the same way. The helper now waits 12 s
(`LOOKUP_TIME`), past glibc's retry.

Not reproduced on the bench: "no networks at all" after a resume (2026-10-26): the radio blocked at boot or resume is
switched on once and the scan finds the network (`rfkill-on`), and a fresh card's "Wi-Fi is off" works as designed
(`rfkill`).
