# Bluetooth on the standalone TrimUI Brick card

Status: controllers and headphones (A2DP) built and host-tested; no hardware pairing or audio test yet.
The alternate card entry and default ROCKNIX build keep their existing behavior.

## Firmware observations

Read-only inspection on 2026-11-03 found the XR829/XRadio transport, not an RTL/AIC transport:
`hciattach -n ttyS1 xradio` was running, `hci0` existed (UART, down), and
`/lib/firmware/fw_xr829_bt.bin` was present. This identifies the active firmware/driver path;
no physical chip marking was inspected. Alternative AIC and RTL init files are present but
are not evidence those chips are fitted. The firmware init service `hciattach` starts the
UART transport through procd at boot. Firmware `runtrimui-original.sh` starts BlueZ;
our card takes over before that menu. Earlier card-entry tests already had system D-Bus and HCI transport,
but neither bluetoothd nor BlueALSA was running on that entry. A stock-firmware boot also starts
the firmware's procd `bluetoothd -nd` service before the card's MainUI. This difference was reproduced
with the public 0.3.0 card: its own service refused to enable Bluetooth with `foreign bluetoothd is running`.

The firmware provides BlueZ 5.54 (`bluetoothd`, `bluetoothctl`, `hciconfig`, `btmgmt`),
`hciattach`, `bluealsa`, `bluealsa-aplay` and both ALSA BlueALSA plugins.
BlueALSA advertises A2DP Source/Sink with SBC. There was no `rtk_hciattach` on PATH.
Vendor firmware init scripts are not shipped. Only the stock-firmware entry may ask the installed
`/etc/bluetooth/bluetoothd` service to stop during a Bluetooth handover, as described below.
The installed `bt_init.sh` has been modified and must not be treated as pristine stock.

## Card service and UI

`deploy/brick/hwroot/usr/bin/brick-bluetooth` supplies start, stop, status and save.
The card's systemctl stand-in dispatches `bluetooth` and `bluetooth.service` to it.
Only our card exposes Settings → Connections → Bluetooth. `RealHw` uses that service;
the existing asynchronous page supplies search, pair/trust/connect, disconnect and forget.
Its list reads the running service instead of ROCKNIX's system.cfg. BlueZ 5.54's rejected
`devices Paired|Connected|Trusted` filters fall back to per-device `info`.

The service reuses HCI when it already exists. Otherwise it starts its own UART1/XRadio
hciattach and bounds the adapter wait. It uses the firmware's system bus and runs its own
foreground bluetoothd plus a persistent bluetoothctl `NoInputNoOutput` default agent.
Unlike separate `agent` and `default-agent` commands, this client remains registered.
PIN-entry keyboards and interactive passkey confirmation are not supported.
The adapter is pairable but not discoverable. No discovery or pair request starts at boot.

Keys are restored from `storage/.config/pocket-ui/bluetooth.tar` to RAM, bound onto
`/var/lib/bluetooth` before BlueZ starts. On the observed firmware `/var` points to `/tmp`.
The service refuses a symlink at its storage target and refuses a foreign bluetoothd, except for
the stock-firmware handover. That entry exports `BRICK_BT_FIRMWARE_INIT=/etc/bluetooth/bluetoothd`.
Every existing daemon must have the expected executable and the exact procd command `bluetoothd -nd`
before that named service is stopped. An unknown command or storage link is refused without stopping it.
The firmware's `/var/lib/bluetooth` symlink to `/etc/bluetooth/keys/` is removed in RAM, never followed;
our own key directory is bound in its place. On stop or a failed start the empty RAM directory is replaced
with the original symlink. Firmware keys are neither read into the card nor changed. The firmware daemon
stays stopped when the player switches Bluetooth off; the next ordinary stock boot starts it as usual.
Host fixtures may override `BRICK_BT_FIRMWARE_KEYS` and `BRICK_BT_FIRMWARE_PROC` for these checks.
It does not use `/etc/bluetooth/keys` or import another UI's pairings. The card's FAT does
not need to preserve Unix file modes. `brick-bluetoothctl` forwards to the firmware client
and atomically archives keys after pair/trust/remove; stopping archives after BlueZ exits.
The last working archive remains if a bind mount or restore fails. The on/off preference is
`storage/.config/pocket-ui/bluetooth-on`. Startup honors it in the background.
Only recorded PIDs whose command line still matches our program are signalled. The boot
HCI transport and system bus remain running. Stopping powers the adapter off and unmounts
our key store. There is no broad process kill or vendor UI. Outside the stock-firmware entry, firmware
services remain untouched.

## Headphones (A2DP, update 7)

Status: built and host-tested; not yet heard on the real Brick.

- `brick-bluetooth` starts the firmware's `bluealsa -p a2dp-source` beside BlueZ (when it is there; without it controllers
  still work and the sound stays on the speaker) and stops it first on the way down.
- Every 3 s, only while the kernel has a link (`hci0:<handle>`), it asks BlueZ for a connected device with an Audio Sink UUID.
  Its address goes into `$BRICK_RUN/bt-audio` (written by rename); none, BlueALSA gone, or the service stopping removes it.
  `brick-bluetooth audio` prints it. A pad is never a route.
- pocket-core and pocket-ui open ALSA by name at every stream (`platform::linux_audio::brick_device`): "pocket_bt:DEV=<address>"
  while the file names a plain address, else "pipewire" (the chip, as before). `pocket_bt` is in the card's `asoundrc`: the
  firmware's BlueALSA PCM (profile a2dp) behind `softvol` ("BT Volume" on card 0). The name changes, not the config: alsa-lib does
  not read a changed `~/.asoundrc` again in a running program (only its top-level files are watched: on the host a second open
  in the same process after a rewrite still went to the old device). The address argument with its colons was checked the same
  way.
- pocket-core: `audio_route::Real` on the Brick reads the route file as its "link" and "default sink", so the keeper sees
  Bluetooth come and go as it does PipeWire's; the writer opens its stream again when it does (`audio::tests::
  the_brick_opens_the_game_stream_again_when_headphones_come_and_go`), retrying every 0.5 s while the headset's stream will not
  open. A stream that fails (the headset switched off) is reopened for up to 12 s on the Brick (5 s elsewhere): long enough for
  the service's next look to point the name back at the speaker. UI sounds close after a quiet moment and open by name again.
- Volume: the `volume` stand-in ramps "BT Volume" (0..255) with every accepted level change (about 2.2–2.4 dB per 5% key step).
  The mute slider's `volume mute` makes it 0; keys while muted leave the saved target alone and `unmute` restores that target.
  The control exists only after the first stream to the headset, so the service runs `volume bt` until it takes, once per connection.
- Limits of this first version: BlueALSA's PCM takes one stream at a time, so with a game paused under the pause menu the menu
  ticks may stay silent on headphones (never on the speaker instead). SBC only. Latency and dropouts are unmeasured on the
  device; the game's stream uses the 32 ms buffer it uses on the speaker. The first stream after connecting can start at the
  full level until the service sets it (a 3 s polling interval, plus command and ramp time; no measured hard latency bound):
  start with the headset turned down. The new volume curve does not close this initial connection window; physical validation
  of silence during connection is still needed.

## Verification

`scripts/test-brick-bluetooth.py` supplies host stubs for D-Bus, BlueZ, mount and rfkill.
It covers service start/status/stop, repeat start, archive save/forget/restore, refusing a
foreign daemon or missing bus, mount failure, stale PIDs and systemctl unit aliases.
The factory regressions also cover enabling Bluetooth with procd already running, preserving stock
pairings, restoring the firmware storage link, repeat start, failed mount, and refusing a different session.
The backend also has a BlueZ 5.54 filter-fallback regression test. Run these after the
release build reservation is lifted; this series has only had syntax/diff review so far.
Compile both default and `--features brick`; run the targeted Bluetooth and settings tests,
then the long-lived branch gate on the permitted build host. New dark/light screen
snapshots also remain pending. Do not merge/deploy based only on this static review.

Hardware must still show controller pairing, trust and reconnect after a cold start,
forget persistence, controller input in menus and games, service stop/restart, and behavior
after sleep. No reboot/sleep or sound test may be done without the owner's authorization.
Bluetooth service lifecycle has NOT been exercised on the real device.

## Licensing

All scripts in this series are original project code under the project's license.
No firmware binary, init script, firmware blob or NextUI/MinUI asset is bundled. BlueZ
(`/usr/bin/bluetoothd`, `/usr/bin/bluetoothctl`, upstream BlueZ) and BlueALSA
(`/usr/bin/bluealsa`, upstream arkq/bluez-alsa) are device-supplied runtime dependencies;
the exact firmware build sources/licenses have not been verified for redistribution. The card
continues to rely on the installed XR829 firmware rather than redistributing it. If any
of these are bundled later, verify their exact source/version and notices first.
