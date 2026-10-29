# Network hardening: file sharing, SSH, Syncthing, rpcbind, LLMNR

What a Pocket offers on the Wi-Fi, and how pocket-ui narrows it. Measured on ROCKNIX 20261001 (a Mini V2 on the
local network, written `<device-ip>` below, and the extracted image of the virtual device,
`docker/extract-rocknix.sh`).

> **Status:** the file sharing changes and the SSH switch (Settings → Connections → Remote access) are implemented and were checked
> on the device. `scripts/harden-ssh.sh` is prepared and only meant for development devices. The "rollout" steps
> for release devices and the optional rpcbind and LLMNR changes are **planned**. The Syncthing switch
> (Settings → Connections → Sync) is implemented and was checked on the device, see [Syncthing](#syncthing-settings--sync-implemented-checked-on-the-device-22102026). The Tailscale page (Settings → Connections → Tailscale) is implemented and was tried in the virtual device with stand-ins only, see [Tailscale](#tailscale-settings--tailscale); the test device itself is on a real tailnet, and the web companion's check over it is in [WEB-COMPANION.md](WEB-COMPANION.md#device-checks-partly-done). The ledger is [STATUS.md](STATUS.md).

## What ROCKNIX does by default

| Service | ROCKNIX | Port |
|---|---|---|
| sshd | `PermitRootLogin yes`, `StrictModes no`, passwords allowed; root's password is `rocknix` (`root.password` in `system.cfg`, re-applied at every boot by autostart `007-rootpw`). `ssh.enabled=0` by default; our test device has 1 | 22, all addresses |
| Samba | shares `/storage/.update`, `/storage/.config`, `/var/log`, themes and the games to everyone, guest mapped to **root**, no encryption. `samba.enabled=0` by default | 139/445, all addresses |
| Syncthing | web interface and REST API on **all addresses, plain http, login `root` with root's password** (`rocknix` unless changed): `start_syncthing.sh` rewrites this at every start. `syncthing.enabled=0` by default | 8384 tcp, and 22000 tcp/udp for the sync itself |
| rpcbind | `rpcbind.socket` pulled in by a vendor `multi-user.target.wants` link; only needed for NFSv3 mounts | 111 tcp/udp |
| systemd-resolved | LLMNR on (`+LLMNR`) | 5355 tcp/udp |
| avahi | mDNS | 5353 udp |
| pocket-ui's web companion | not ROCKNIX's: off by default (no socket); when switched on, port 80 only on the Tailscale addresses, tied to `tailscale0` ([Web companion](#web-companion-settings--web-companion)) | 80 tcp, tailnet only |
| tailscaled | off unless `tailscale.up=1` (default: unset, so autostart stops it); when on, UDP `41641` (WireGuard, authenticated) and a `tailscale0` interface. Its own SSH server (`--ssh`) is off unless someone asks for it | 41641 udp |

The system image is a read-only squashfs. Everything below changes only files under `/storage`.

Where things live, and what survives a reboot:

- `/etc/ssh/sshd_config` is in the image. sshd starts as `sshd -D $SSH_ARGS` with
  `EnvironmentFile=-/storage/.cache/services/sshd.conf`, but autostart `099-networkservices` deletes and
  re-creates that file empty at every boot, so `SSH_ARGS` there does **not** survive.
- sshd only starts while `/storage/.cache/services/sshd.conf` exists (`ConditionPathExists`), which
  `099-networkservices` creates from `ssh.enabled` at boot.
- `/storage/.config/system.d` is on systemd's unit path, before `/usr/lib/systemd/system` (checked with
  `systemctl show -p UnitPath`). Drop-ins (`sshd.service.d/*.conf`) and masks (a unit name linked to
  `/dev/null`) placed there survive reboots and system updates.
- `/etc/systemd/resolved.conf.d` links to `/storage/.config/resolved.conf.d`.
- `/etc/passwd` is in the image; no user can be added. `/etc/shadow` links to `/storage/.cache/shadow`.
  nsswitch lists `systemd`, but `libnss_systemd` is not in the image, so userdb drop-ins don't work either.

## File sharing (K1, implemented)

`src/wifi.rs` writes `/storage/.config/smb.conf` at every start of pocket-ui (the original is kept as
`smb.conf.rocknix`):

- one share, `games-roms` = `/storage/roms`; no update, config, log or theme shares;
- login required: user `pocket`, a per-device password (`src/secret.rs`, e.g. `word-word-word-123`,
  about 34 bits) kept in `/storage/.config/pocket-ui/share/password` (0600) and shown on the Add games
  screen. pocket-ui writes its own `smbpasswd` (NT hash, one line) and `users.map` (`nobody = pocket`)
  next to it; ROCKNIX's `/storage/.cache/samba/smbpasswd` is not used. `map to guest = Never`,
  `restrict anonymous = 2`, `invalid users = root`, `guest ok = no`, `valid users = nobody`;
- writes happen as `nobody:nogroup` (`force user`), the only unprivileged account no service runs as;
- `veto files = /ports/PortMaster/bios/savestates/`: these folders are invisible and can't be created,
  in any case (clients are case-insensitive with `unix extensions = no`);
- `interfaces = wlan0`, `bind interfaces only = yes`, `smb encrypt = desired`, no symlinks followed;
- the switch on the Add games screen turns sharing on and off **for this session**: ROCKNIX's
  `samba.enabled` is kept at 0, so sharing is off after every restart;
- turning it on hands everything under `/storage/roms` except the vetoed folders to `nobody` (games
  copied earlier as root or rsync'd from a Mac would be read-only over the share otherwise). root,
  and so pocket-ui and the emulators, still read everything;
- smbd binds to the address wlan0 has when it starts and never picks up a new one, not even on SIGHUP
  (tested). pocket-ui restarts it when the Add games screen opens or a Wi-Fi connection succeeds and
  smbd isn't listening on the current address.

Checked on the device (22.10.2026) with `smbclient`:

| Test | Result |
|---|---|
| listens on | `<device-ip>:139`, `<device-ip>:445` only; `127.0.0.1` refused |
| `pocket` + password: list, write into `gba/` | works, file is `nobody:nogroup 0664`, root reads it |
| `cd ports`, `PortMaster`, `bios`, `savestates`; `put evil.sh` into `ports`; create `Ports/` | `NT_STATUS_OBJECT_NAME_NOT_FOUND` / `ACCESS_DENIED`, nothing created |
| anonymous, `guest`, wrong password, `root`/`rocknix` | refused |
| `--client-protection=encrypt` | works |
| switch off | smbd and nmbd stop, nothing on 139/445 |

Clients: macOS Finder (Go → Connect to Server, `smb://<address>/games-roms`, "Registered user") and
Windows (`\\<address>\games-roms`, then the user and password) both speak SMB 3 with NTLMv2, which a
smbpasswd account supports. Windows 11 24H2 refuses guest shares and requires signing; a password-protected
share is what it expects. macOS writes `._*` files for resource forks; harmless.

Trade-off: BIOS files can't be copied over the share any more; they have to go onto the card from a
computer that writes ext4, or over SSH.

## Remote access (SSH, designed, partly implemented)

Decision (22.10.2026): SSH is off by default; a switch turns it on with a per-device password,
the same way as file sharing.

Implemented (`src/remote.rs`, Settings → Connections → Remote access (SSH), `Only::Remote` in `src/setup.rs`):

- **On:** creates the password once (`/storage/.config/pocket-ui/remote/password`, 0600, same generator as
  file sharing), sets it as root's password with ROCKNIX's `setrootpass` (writes `/storage/.cache/shadow`
  and `root.password`, so it also survives the reboot-time `007-rootpw`), marks `/run/pocket-ui-ssh-on`,
  creates `/storage/.cache/services/sshd.conf`, `systemctl start sshd`. The screen shows
  `ssh root@<address>`, user and password, like the Add games screen. `ssh.enabled` stays 0, so SSH is off
  after a restart.
- **Off:** removes the marker, `ssh.enabled` to 0 if it was 1, removes `sshd.conf`, `systemctl stop sshd`.
- **Idle timeout:** a thread checks once a minute; when SSH runs **because of our switch** and no
  connection to port 22 was open for 30 minutes (`/proc/net/tcp{,6}`, `src/sockets.rs`), it turns SSH off.
  Pure logic in `IdleTimer::tick`, unit-tested.
- **Keys:** `/storage/.ssh/authorized_keys` works whenever sshd runs, also during the password window.
- **Developer devices:** `/storage/.config/pocket-ui/dev-ssh` exists → mode `Dev`: the switch is
  disabled, the timeout never fires, `turn_on`/`turn_off` refuse. **Created on the development device.**
- **sshd running for another reason** (ROCKNIX `ssh.enabled=1`, kernel `ssh` argument) → mode `External`:
  the timeout leaves it alone; only an explicit switch press stops it.

Not implemented on purpose (needs the rollout below): nothing in pocket-ui stops SSH at startup.

Decided (2026-10-22): switch off means sshd is off completely, for keys too. Keys work only
while SSH is on. Development devices stay reachable through the `dev-ssh` marker, which keeps sshd on
for good. A key-only sshd that runs while the switch is off is not wanted.

### Rollout order

1. Every development device gets `dev-ssh` (done on the development device; `touch /storage/.config/pocket-ui/dev-ssh`).
2. Deploy a build with this branch. Verified: on a device with `dev-ssh`, nothing changes for SSH
   (sshd still active, no `/run/pocket-ui-ssh-on`, no password file).
3. Release devices: `ssh.enabled=0` (ROCKNIX's default; set explicitly in the image or at first start),
   so sshd is off from the next boot. Until a user switches remote access on, root's password stays
   `rocknix`, but nothing listens on 22. **SD card image (checked 2026-10-28):** ROCKNIX 20261001's
   `/usr/config/system/configs/system.cfg` has `ssh.enabled=0`, `samba.enabled=0`, `syncthing.enabled=0`; SYSTEM has
   no `authorized_keys` and no host keys; the GRUB command line has no `ssh`; STORAGE has no `dev-ssh`, `.ssh` or
   keys. `scripts/check-image.sh` checks all of it. On a fresh card nothing listens on 22, 139/445 or 8384 until the
   player turns Remote access or file sharing on in pocket-ui, which sets a per-device password. Only
   EmulationStation's own SSH switch would start sshd with `rocknix`; the image notes say so.
4. First switch-on per device: password created and set (`setrootpass`), sshd started for the session.
5. 30 idle minutes or a restart: off again.
6. Optional, all devices: rpcbind and LLMNR off (below), without the key-only sshd part on release devices
   (it would lock out the password the switch shows).

## Syncthing (Settings → Sync, implemented, checked on the device 22.10.2026)

User guide: [user/sync.md](user/sync.md). Code: `src/sync.rs`, `src/sync_page.rs`, `src/app_sync.rs`.

### What ROCKNIX does

`syncthing.service` runs `/usr/bin/start_syncthing.sh`, which on **every start**

```sh
syncthing generate --gui-user root --gui-password ${ROOTPASS}      # ROOTPASS = get_setting root.password
xmlstarlet ed --inplace -u "//configuration/gui/address" -v "0.0.0.0:8384" /storage/.config/syncthing/config.xml
```

So with ROCKNIX's own switch (ES → Network → Enable Syncthing, `syncthing.enabled=1`) the web interface listens on
every address over plain http, and the login is `root` with root's password, which is `rocknix` on a Pocket where
nobody changed it (docs above). The interface can add a folder at any path, so this is read and write access to
every file on the Pocket for anyone on the Wi-Fi. A config file produced that way is the fixture
`src/fixtures/syncthing-rocknix.xml`; `Config::problems()` reports it as `RocknixLogin` and `ClearText`.

### What pocket-ui does instead

**Decision (22.10.2026): the web interface stays reachable from the local network, because that is how a computer
pairs with the Pocket, but only with a per-device password, over https, and only while Syncthing is switched on.**
The Pocket's device ID and QR code on the Sync page are enough to add the Pocket on another device; accepting that
device on the Pocket needs the web interface.

With the switch on (`sync::enable`):

- `/storage/.config/pocket-ui/sync/password` (0600) holds a password of the kind file sharing and SSH use
  (`word-word-word-NNN`, `src/secret.rs`), made on first use and shown on the Sync page. It is never root's password.
- `/storage/.config/pocket-ui/sync/start.sh` is the script the service runs. A drop-in
  `/storage/.config/system.d/syncthing.service.d/10-pocket-ui.conf` (`ExecStart=` cleared, then ours) replaces
  ROCKNIX's `ExecStart` without touching the image; it survives reboots and system updates like the SSH drop-ins
  above. The script stops with `exit 1` when the password file is missing: **there is no fallback to an open
  interface.**
- The script sets the login with `syncthing generate --gui-user pocket --gui-password -` (the password on stdin, not
  on the command line), the address to `0.0.0.0:8384` and **`tls="true"`** (Syncthing's own self-signed certificate;
  browsers warn once), usage reports off (`urAccepted=-1`), crash reports off, no browser, and automatic upgrades off
  (the binary is in the read-only image).
- `set_setting syncthing.enabled 1` is what makes ROCKNIX start the service at boot (autostart `007-syncthing` →
  `systemctl start syncthing`), so the switch **persists across restarts**, unlike file sharing and SSH. This is
  what was asked for, and it is why the interface is not tied to a timer: the password and https are the protection.
  Switching off sets it to 0 and stops the service; nothing listens on 8384 or 22000 afterwards.
- The REST API is used by pocket-ui over `https://127.0.0.1:8384` with the API key from `config.xml`; the API key never
  leaves the Pocket and is not shown.
- Every folder pocket-ui creates shares only what it names (2026-10-22: added the PS2 memory cards, GameCube cards,
  Wii and 3DS save folders, see `src/sync.rs`, `Dir`): a `.stignore` written before the folder is first added
  allows the save files with `!` lines and ends in `*`, so no game, no BIOS file next to the GameCube cards, and no
  installed 3DS title is ever hashed or offered (unit-tested against sample paths of each folder). The folders of
  the standalone emulators are paused (`PATCH /rest/config/folders/<id>`) while their emulator runs and resumed when
  it ends; which ones pocket-ui paused is kept in `/storage/.config/pocket-ui/sync/held`, so a folder the user paused
  is never resumed by pocket-ui, and one left paused by a crash is resumed after the next start, once its emulator
  has ended.
- When the page finds Syncthing running with a web interface that is open (no password, ROCKNIX's `root` login, or
  plain http), it says so in red and tells the user to switch it off and on again, which replaces the setup.

Not changed, deliberately: Syncthing's own network behaviour (local discovery, global discovery, relays, NAT
mapping). Devices are identified by their certificate, a peer must be accepted on the Pocket before anything is
shared, and the data is encrypted in transit. A user who wants it to stay on the home network can switch these off in
the web interface.

### Checked on the Mini V2 (22.10.2026) and in the virtual device

| Test | Result |
|---|---|
| listening | `:::8384` and `:::22000` while on; nothing on either after switching off; `syncthing.enabled` 1 then 0 |
| GUI page without login | the sign-in page (200), no data |
| `/rest/system/status`, `/rest/config` without login or key | `403` |
| login `root` / `rocknix` (ROCKNIX's default) | `403` |
| login `pocket` with a wrong password | `403` |
| login `pocket` with the shown password | `204` |
| plain http on 8384 | redirected to https |
| `config.xml` | `tls="true"`, address `0.0.0.0:8384`, user `pocket`, `urAccepted` -1, crash reports off, upgrades 0 |
| folders | `pocket-saves` (`/storage/roms`, only `*.srm`/`*.sav`/… by `.stignore`, 13 files, 39 MB) and `pocket-states` (102 files, 405 MB); no BIOS, no game files |

Not checked: a second device actually syncing with the Pocket (no outside device may be paired during development; in
the virtual device a second Syncthing paired over the loopback synced the `.srm` and the state files, and the
Sync page showed *Up to date*), and a reboot with the switch on (the boot path was read, not run).

## Tailscale (Settings → Tailscale)

User guide: [user/remote.md](user/remote.md). Code: `src/tailscale.rs`, `src/tailscale_page.rs`, `src/app_tailscale.rs`.
Decision (22.10.2026, after a Mini V2 lost its Wi-Fi and was only reachable over a phone hotspot with the Mac on the same
hotspot): Tailscale is a first-class setting, because a private network path that does not depend on the home Wi-Fi is
what makes every other remote switch useful away from home. It only adds the network path; it does not add a way in.

### What ROCKNIX does (read from the 20261001 image, extracted with `docker/extract-rocknix.sh`, and ROCKNIX's `package.mk`, which pins Tailscale 1.98.8)

| Piece | Where |
|---|---|
| binaries | `/usr/sbin/tailscale`, `/usr/sbin/tailscaled` |
| service | `tailscaled.service`: `tailscaled --state=/storage/.cache/tailscale/tailscaled.state --socket=/run/tailscale/tailscaled.sock --port $PORT $FLAGS`, `EnvironmentFile=/storage/.config/tailscaled.defaults` (`PORT="41641"`, `FLAGS=""`, copied from `/usr/config/tailscaled.defaults`), `Type=notify`; ROCKNIX starts and stops it from autostart, see the next row |
| boot | autostart `daemons/004-tailscaled`: `STATE=$(get_setting tailscale.up)`; `099-networkservices` runs `systemctl start tailscaled` when it is `1` and `systemctl stop tailscaled` otherwise |
| EmulationStation | the menu entry *Tailscale VPN* (strings of the `emulationstation` binary; not driven here): on = `systemctl start tailscaled`, `tailscale up --timeout=7s`, the sign-in address shown in a message ("TAILSCALE REAUTHENTICATE"), `tailscale.up=1`; off = `tailscale down`, `systemctl stop tailscaled`, `tailscale.up=0` |
| state | `/storage/.cache/tailscale/` (survives reboots and updates, readable by root only: `StateDirectory` mode 0700) |

### What pocket-ui does

Everything ES does, with a sign-in that works on a handheld, and nothing else:

- **Switch** = `tailscale.up=1|0` in `system.cfg`, written the way ROCKNIX's `set_setting` writes it and under its lock
  (`src/system_cfg.rs`), so the boot script honours it, plus
  `systemctl start|stop tailscaled`. It **persists across restarts**, like Syncthing and unlike SSH: a device that
  was meant to be reachable should come back reachable. The sign-in stays when the switch goes off.
  `/storage/.config/tailscaled.defaults` is copied from `/usr/config` when it is missing, because the unit refuses to
  start without it.
- **Sign in** = `tailscale up --reset --ssh=false [--hostname=…]`, read as it prints: the address after "To authenticate,
  visit:" becomes a QR code and short text; `tailscale up` ends by itself when the user approves, or is killed (B, or
  ten minutes). The poller also reads `AuthURL` from `tailscale status --json` as a second source.
- **Auth key** = the same with `--auth-key=file:/run/pocket-ui-tailscale-key` (mode 0600, on tmpfs, removed afterwards
  in every case, never on a command line and never kept).
- **Name** = `tailscale set --hostname=…` (validated as a DNS label first); a name chosen while the service is off is
  used at the next sign-in. Kept in `tailscale.json`.
- **Log out** = `tailscale logout`, after a second press.
- **Never blocks the UI**: every call runs on its own thread (`tailscale::spawn`) under a time limit
  (`status` and `is-active` 4 s, start/stop/logout/set 30 s, the login 10 minutes), through `proc::run_with_timeout`
  (process group killed at the limit) or, for the login, a streaming reader that is killed on cancel. The poller
  (every 2 s, only while the page is open) is bounded by the same limits. Unit-tested with stand-in programs that
  hang.

### How it interacts with SSH and the other services

| Question | Answer |
|---|---|
| Tailscale SSH instead of our sshd? | **No.** `--ssh` makes tailscaled answer on port 22 of the tailnet address itself, bypassing sshd, its password and `dev-ssh`. Every `tailscale up` and `tailscale set` pocket-ui runs says `--ssh=false` (`--reset` also clears a `--ssh` set by hand earlier; unit-tested). If someone starts it with `--ssh` by hand, the next switch-on or sign-in turns it off again (`tailscale set --ssh=false`) |
| Is SSH reachable over the tailnet? | Only while sshd runs, and then with the rules of Remote access: the shown password, or a key in `/storage/.ssh/authorized_keys`. sshd listens on all addresses, so `tailscale0` is one of them. Nothing is changed in sshd or in root's password |
| The SSH idle timeout | A session over the tailnet counts like any other: `sockets::established` matches local port 22 whatever the address, so an open session keeps SSH on and the 30 minutes run after it ends (to be confirmed on the device) |
| `dev-ssh` devices | sshd is on for good, so with Tailscale on it is reachable from the tailnet at all times. Before relying on that, run `scripts/harden-ssh.sh --apply` on the device (keys only), and limit the Pocket to your own devices in the Tailscale access controls (e.g. tag it and allow only port 22 from your user) |
| File sharing | Samba binds `wlan0` only (`bind interfaces only`), so it is **not** reachable over the tailnet |
| Syncthing | Listens on all addresses (8384 with password and https, 22000): reachable over the tailnet with its own password, as on the Wi-Fi |
| Who may reach the Pocket | The tailnet's access controls, not the Pocket. By default every device of the same user can reach every port that listens. `tailscale up --shields-up` would block all inbound connections and therefore also SSH, so it is not set |
| Exposure to the internet | None added: tailscaled opens one UDP port (41641) for the WireGuard traffic, authenticated by node keys; nothing is forwarded or published (no Funnel, no `serve`) |
| DNS | `tailscale up` keeps Tailscale's default (`--accept-dns=true`): MagicDNS names resolve on the Pocket through systemd-resolved. Not changed from what EmulationStation's switch does |
| Auth key in the menu | The key is typed on a screen anyone looking at the Pocket can read, as the Wi-Fi password is. Use short-lived keys. The sign-in by QR code needs no secret on the Pocket |
| Not covered | Exit nodes, subnet routes, Taildrop, Funnel and `serve` are not offered; they can still be used from a shell on the Pocket |

### Device checks (not done for the page: the device is off-limits to development agents)

| Check | Expected |
|---|---|
| `ls -l /usr/sbin/tailscale /usr/sbin/tailscaled; /usr/sbin/tailscale version` | present; 1.98.x |
| Settings → Connections → Tailscale shows *Off*; `systemctl is-active tailscaled`; `get_setting tailscale.up` | inactive; empty or 0 |
| Switch on | status **Logged out** within a few seconds; `get_setting tailscale.up` is 1; `/storage/.config/tailscaled.defaults` exists; `systemctl is-active tailscaled` active |
| Sign in: scan the QR code with a phone, approve | the page leaves the QR code by itself, shows **Connected as …**, an address `100.x`, a name and the device count; the Pocket is in the admin console |
| `tailscale status --json \| head`, `tailscale debug prefs` (or `tailscale status`) | Running; `RunSSH` false; the hostname as chosen |
| `ss -tlnp \| grep ':22 '` with Remote access **off**, then ping the Pocket's `100.x` from the phone | nothing on 22; ping answers (Tailscale only) |
| Remote access on, `ssh root@pocket-mini` from a tailnet device with the shown password | login works; after closing the session the 30 minutes start (journal or `/run/pocket-ui-ssh-on`) |
| `nmap -p 22,445,8384 <100.x>` from a tailnet device with all three switched on | 22 open (sshd), 445 closed (Samba on wlan0 only), 8384 open (password and https) |
| `tailscale up --ssh` by hand, then switch Tailscale off and on in the menu | `tailscale debug prefs` shows `RunSSH` false again |
| Name: set `pocket-mini`, look in the admin console and `tailscale status` | renamed at once |
| Log out (A, A) | status **Logged out**; the device is gone from the admin console (or expired) |
| Switch off, **restart the Pocket** | Off, nothing running. Switch on, sign in, restart: back **Connected** by itself without a new QR code |
| Wi-Fi gone: connect the phone's hotspot, Pocket joins it | within seconds the Pocket is **Connected** again and reachable by name from the Mac |
| Pull the Wi-Fi while the page is open; kill `tailscaled` (`kill -STOP` it) while the page is open | the page stays responsive; after 4 s it shows *Starting* with the reason; `kill -CONT` it and it recovers |
| Log: `grep -i tailscale /var/log/pocket-ui.log`, `journalctl -u tailscaled -n 50` | no panics; no key or address in the pocket-ui log |

### Checked in the virtual device (22.10.2026)

With the stand-ins of `scripts/vdev-fake-tailscale.sh` (no real tailnet, no network): the page says *Not installed*
before them; switch on writes `tailscale.up=1` into `system.cfg` (then through ROCKNIX's real `set_setting`; since 2026-10-23
through `src/system_cfg.rs`, same format and lock) and "starts" the
service; the QR code carries the address `tailscale up` printed; approving ends the login and the page shows the
connected state (the QR code in the screenshot was read back with macOS's QR detector and is exactly that address); the chosen name reaches `tailscale set` and is kept over a restart of pocket-ui; the sign-in with an auth key puts the key in `/run/pocket-ui-tailscale-key` (0600) for the call and the file is gone afterwards, a rejected key shows the error text; log out and switch off write `tailscale.up=0`; after a restart of pocket-ui the page is still *On* and *Connected*; with the stand-in `tailscale` made to hang, the menu kept moving and the page said *Starting* with *tailscale did not answer* after 4 s. Unit tests
(`cargo test --lib tailscale`) cover the parsing of `status --json`, `tailscale ip` and the sign-in address, the setting
written to a `system.cfg` in the test folder (`system_cfg`, whose own tests run ROCKNIX's `set_setting` beside it), the key file, cancel, timeouts and hanging programs.

## Web companion (Settings → Web companion)

pocket-ui's own HTTP server for a browser on the tailnet: library, edits, uploads, play time, a screenshot. Off by
default; when on it listens on port 80 only on the Tailscale interface's addresses (`SO_BINDTODEVICE`, every peer
checked to be a Tailscale address), never on the Wi-Fi, with no switch for that. The owner's own devices get in by
tailscaled's `whois`, any other browser with a six-digit code from the Pocket's screen (one try a second, five per
code) and then a cookie whose hash only is kept; every write needs a CSRF token and passes an origin check; a strict
Content-Security-Policy; uploads only into game folders, `roms/bios` and the PortMaster drop folders, with cleaned
names, size limits and no-replace renames; no shell. The protocol, every endpoint, the threat model and the
self-review are in [WEB-COMPANION.md](WEB-COMPANION.md); the device checks at its end.

## The sign-in check

pocket-ui can tell a hotel or train network that wants a sign-in in a browser from a network that is simply offline
(`src/net_check.rs`, [user/troubleshooting.md](user/troubleshooting.md)). It is the only connection pocket-ui makes for
this, and it asks **one host**: `http://nmcheck.gnome.org/check_network_status.txt`, NetworkManager's community check
page (plain HTTP on purpose: a sign-in page can only hijack plain HTTP). No Google address is used.

- **When:** after the Wi-Fi page scans while a network is connected (and after joining one), and once when an update,
  the store or the PortMaster catalog has just failed. Not at start, not in a game. The answer is kept for 20
  seconds, so a burst of failures asks once. The one timer: while the Wi-Fi page (or its sign-in sheet) shows a
  sign-in, the same request every 15 seconds on a thread, until the answer is not a sign-in or the page closes
  (`setup::keep_watch`, [PORTAL-SIGNIN.md](PORTAL-SIGNIN.md)).
- **What is sent:** a `GET` by `curl` with curl's own User-Agent and nothing else: no cookie, no identifier, no
  account, no name of the Pocket or the network. The host learns the Pocket's public address, as any web page does.
- **What is read:** the status, the redirect address and the first 4 KB of the body; nothing is stored or passed on.
  The redirect address is kept in memory for the Wi-Fi page (only an http or https address without control
  characters, at most 2 KB), which shows its host.
  A different page, a redirect elsewhere or a very large page counts as a sign-in page; a 404 or 5xx from the check
  page itself counts as unknown.
- **If the page is ever gone or changes its text,** a gone page ends as "unknown" (the generic message stays), but a
  different text is read as a sign-in page, so a connected Wi-Fi would show the sign-in line until `ONLINE_BODY` in
  `src/net_check.rs` is updated. Look at it when this line shows on a network that works.

## The sign-in helper

Settings → Connections → Wi-Fi → the sign-in row → **Sign in…** loads a hotel or train network's sign-in page and sends
its form, only when the person presses its button ([PORTAL-SIGNIN.md](PORTAL-SIGNIN.md)). pocket-ui runs as root, so
the page is treated as hostile: no browser engine and no scripts; a one-pass reader with hard limits (`html_form`);
curl with its request on a pipe, only http and https, no proxy, every host looked up and pinned, never the Pocket's own
addresses, at most 8 redirects, 1 MiB and 10 s a page; a form only to its own site or a private-network address; a
cookie jar in a 0700 folder in RAM that goes with the page; what was typed never saved, logged, put on a command line,
or sent elsewhere, and overwritten when the page closes. The table of rules and the tests that hold them:
[PORTAL-SIGNIN.md](PORTAL-SIGNIN.md#security-rules-and-their-tests).

## `scripts/harden-ssh.sh` (prepared, only run as `--dry-run`)

For devices that are only reached with keys (our development device). Default is a dry run: read-only
checks on the device plus a list of every change. `--apply` makes them; `--revert [--apply]` undoes them.

| Step | What | Persistence |
|---|---|---|
| preflight | key-only login works; our public key (`--pubkey`, default `~/.ssh/id_ed25519.pub`) is in `/storage/.ssh/authorized_keys`; that file isn't group/world-writable; `sshd -t` accepts the new options. Otherwise abort | read-only |
| 1 | drop-in `/storage/.config/system.d/sshd.service.d/10-pocket-ui-keys-only.conf`: `ExecStart=/usr/sbin/sshd -D -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o PermitRootLogin=prohibit-password $SSH_ARGS`, `KillMode=process` | survives reboot and updates |
| 2 | test sshd on port 2222 with the same options while the real one keeps running: key login must work, a password login must be refused (`Permission denied (publickey)`); otherwise the drop-in is removed and the script aborts | — |
| 3 | rollback armed: `systemd-run --on-active=180` removes the drop-in and restarts sshd | — |
| 4 | `daemon-reload`, `restart sshd` (`KillMode=process` keeps the open session) | — |
| 5 | second key login on 22, the running sshd's command line must contain `PasswordAuthentication=no`; then the rollback is disarmed | — |
| 6 | rpcbind masked: `rpcbind.socket` and `rpcbind.service` → `/dev/null` in `/storage/.config/system.d`, stopped | survives reboot |
| 7 | `/storage/.config/resolved.conf.d/10-pocket-ui-no-llmnr.conf`: `LLMNR=no`, `MulticastDNS=no`; resolved restarted | survives reboot |

Dry run on the development device (22.10.2026): all preflight checks passed; today `permitrootlogin yes`,
`passwordauthentication yes`, `kbdinteractiveauthentication yes`; listening on 22, 111 and 5355 (tcp and
udp, IPv4 and IPv6).

### Risks

| Risk | Likelihood | Effect | Guard |
|---|---|---|---|
| Lock-out because the key isn't accepted by the new sshd | low | no SSH until the 3-minute rollback | preflight + test sshd on 2222 before the real one is touched; automatic rollback |
| Lock-out because the rollback can't run (sshd hangs and systemd is wedged) | very low | device needs local access (no keyboard on the Pocket: card in a computer, delete the drop-in) | the drop-in is the only change to sshd; one file under `/storage/.config/system.d` |
| Open session killed on restart | none with the drop-in | — | `KillMode=process`; revert restarts via `systemd-run` from outside the session |
| Remote access switch shows a password that doesn't work | certain on a hardened device | user confusion | only harden dev devices (they have `dev-ssh`, the switch is disabled there) |
| NFS mounts stop working | only if someone uses NFSv3 | mount fails | revert unmasks rpcbind |
| Name lookups of `.local`/single-label names via LLMNR stop | low | `ping pocket` from Windows without DNS fails; IP addresses and mDNS (avahi) still work | revert |
| A ROCKNIX update changes `sshd.service` (e.g. other `ExecStart` arguments) | medium over time | drop-in replaces ExecStart and could drop new arguments | the drop-in keeps `$SSH_ARGS`; check `systemctl cat sshd` after updates |

Not covered: sshd only on wlan0 (needs the address, which changes; `ListenAddress` takes no interface
names), `StrictModes yes`, avahi/mDNS on 5353.
