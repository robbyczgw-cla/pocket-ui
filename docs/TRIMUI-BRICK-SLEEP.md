# Brick sleep (Power button: Sleep)

Settings › System › Sleep & power › **Short press** on the Brick's own card: **Sleep** (the default from 0.4,
`BRICK_POWER_DEFAULT`) or **Screen off**. Screen off is the light sleep it always was: the panel dark, the game paused, pocket-core at its lowest clock,
only the power key wakes it. Sleep is a real suspend to RAM. Idle never suspends (Sleep after stays off on the Brick); only the
short press and the power menu's first entry do. Holding the key still opens the power menu.

The choice is kept in `sleep.json` as `power_button` (`sleep` / `screen_off`), apart from `short_press`: 0.3.x wrote
`short_press: screen_off` for every Brick because there was no choice, so an old file gets the default, not that value
(`Stored::short_press_for`). `Device::screen_off_only` stays true on the Brick; `Sleep::power_sleeps` and
`sleep_settings::power_sleeps` are the one switch that lets the short press, the quick settings' Sleep and the deep-sleep
barrier through.

## The sequence

1. pocket-ui (`sleep.rs`): light sleep (screen off, `pocket-core sleep`), then the deep-sleep worker: `autosave` and the wait
   for a new state version (the resume note too), `sync`, `systemctl suspend`. The Brick skips ROCKNIX's PipeWire volume read
   and kick (its mixer returns with the codec).
2. pocket-core `sleep` pauses the game and **closes its ALSA stream** (`Audio::hold`, answered before the next command, so the
   confirmed save also means nothing is open). Asleep, `/dev/snd/pcmC0D0p` has no user.
3. `systemctl suspend` → `hwroot/usr/bin/brick-suspend`: panel 0; the stock input daemon's `/tmp/system_suspend` marker
   (or `BRICK_INPUTD_SUSPEND`) pauses polling before services stop; our Bluetooth service ends (`brick-bluetooth sleep`: Settings'
   "on" stays, the connected headphones' address is kept); the procd `wpa_supplicant` service is removed and `wlan0` goes down;
   `sync`; one `echo mem`. **No rfkill, ever** (a host test fails if
   the helper writes one): the first helper's Bluetooth rfkill cycles left the XR829 combo chip's Bluetooth answering nothing
   after the wake (HCI timeouts until a reboot), and a Wi-Fi rfkill cycle restarted the kernel on 08.11. 03:16. The kernel's `suspend_stats` success counter decides
   whether it slept (this BSP sometimes reports an error after a good sleep). An armed RTC alarm due within seconds refuses
   the transition; there is no retry and no power-off fallback.
4. Back from `mem` the helper returns at once (panel level and input marker restored); the radios come back **in the
   background** (`wlan0` up, the supplicant, a DHCP renew, Bluetooth and a reconnect of the kept headphones), logged to
   `/tmp/pocket-ui/brick-suspend.log`. That background shell resets HUP/INT/TERM first: started under the helper's own
   `trap ''`, the Bluetooth service inherited TERM ignored, could never be stopped again, and the next sleep failed. The next suspend waits for that (up to 20 s); a lock whose owner died is taken over.
5. pocket-ui: the screen on; pocket-core hears `pause` before `wake` (it never plays a frame in between, "awake (still
   paused)"), opens a new ALSA stream and writes silence until the game goes on; the app opens the pause menu
   (`Sleep::take_menu_after_wake`). Continue plays on.

`start.sh` also sets the kernel's console log level to 3 (see below).

## Sound across the suspend (measured 08.11., game playing, muted)

Muted the valid way: Headphone Switch **on** (only ever switched on), DAC volume 0, speaker amp mute 1; both output switches off
stop the codec's clock and every counter reads 0. pocket-core `audio stats` before and 10 s after each wake (5 s after Continue).

| pocket-core | console log level | per suspend |
|---|---|---|
| 0.3.0.1 (stream open) | 8 (firmware default) | +3, +2 xruns, +4, +3 write errors and recoveries (-86 suspended, then -32) |
| 0.3.0.1 (stream open) | 1 | +3, +2, +4 xruns |
| stream closed | 8 | 4 xruns in 5 cycles, 5,168 frames dropped (108 ms of game sound) |
| stream closed | 1 | **0 in 5 cycles**: 0 xruns, write errors, recoveries, underruns, dropped |
| stream closed | 3 (`start.sh`) | **0 in 5 cycles** |

Two causes. A PCM open across `mem` is suspended by the kernel and its first write after the wake fails. And the firmware boots
with `console=ttyS0,115200 loglevel=8`: every kernel line is written to a serial console at 115200 baud with interrupts off,
and every interrupt, the sound DMA's too, is on CPU 0. The bursts after a wake (the Wi-Fi chip's SDIO reinit at resume, the
eMMC's, the Wi-Fi join some seconds later) held the sound interrupt past the 21 ms ring: each remaining xrun sat on one of
them. With level 3 only critical lines reach that console; `dmesg` and pstore keep everything. This helps outside sleep too
(Wi-Fi joins, the speaker amp's "force mute" line on every volume change), unmeasured there.

The one xrun 59 ms into every game start is older and unrelated (the silent prefill, BRICK-PUBLIC-READY.md); it is not counted.

## Wi-Fi, Bluetooth, kernel

- Wi-Fi: back after every cycle, default route 5–15 s after the wake. The XRadio driver re-initialises its chip at every
  resume (`xradio_core_reinit`, the SDIO card removed and found again): 4–10 `RTO` lines on `sdc1` and two or three LED
  transfer timeouts per wake, with and without the old rfkill. No file-system or MMC data error, no CRC; they are the
  driver's own with the interface down (at log level 3 they no longer reach the serial console).
- Bluetooth: our service is stopped and started around the suspend (as NextUI stops and starts its `bluetoothd`). 5 of 5
  cycles with it on: Powered and Discovering again after every wake. A chip that is attached but answers nothing (as the
  rfkill cycles left the test Brick) is attached once more by `brick-bluetooth` before it starts: that exact
  `hciattach -n ttyS1 xradio` ends, the Bluetooth power (`sunxi-bt` rfkill, never Wi-Fi's) is reset, a new attach.
  Reconnecting kept headphones is host-tested only: no headset was paired on the test Brick.
- About 40 suspends on 08.11. on one Brick: no kernel restart (boot ID unchanged), pstore unchanged.

## Tests

- `python3 scripts/test-brick-suspend.py`: the real helper and `systemctl` under dash and BusyBox ash: success, a kernel false
  negative, a failed suspend, no suspend, radios off before, an existing input marker, a DHCP renew, Bluetooth on and off in
  Settings, an alarm due now, no alarm, the caller not waiting for the radios, a second sleep waiting for them, a stale lock,
  no rfkill file touched in any outcome, TERM and HUP not ignored by the Bluetooth service started after the wake.
- `python3 scripts/test-brick-bluetooth.py`: `sleep` keeps the "on" marker and asks the headphones back; a chip that answers
  nothing is attached once more (never the Wi-Fi rfkill), one that answers is left alone.
- `cargo test --features brick --lib -- audio:: sleep:: sleep_settings`: no stream while held and a new one after
  (`asleep_on_the_brick_no_stream_is_open_and_waking_opens_a_new_one`); the power button's paths
  (`on_the_brick_set_to_sleep_…`, `…_set_to_screen_off_nothing_suspends`, `…_open_pause_menu_…`); old `sleep.json` files.
- `scripts/test-brick-card.sh`: `start.sh` sets the console level.
- On the device: `scripts/brick/journey.py --phase sleep [--cycles N] [--physical] [--bt]`. The installed UI is stopped by its
  PID, a scratch UI and pocket-core from RAM run a free test cartridge, the power key comes through the UI's FIFO (or a
  person's press with `--physical`, which also checks that the PMIC, not the RTC, woke the kernel). **Tests only:** an RTC
  alarm before every press, so a Brick can never stay asleep. Per cycle: confirmed save, "awake (still paused)", the pause
  menu, Continue, sound counters, Wi-Fi route, SDIO lines, Bluetooth Powered/Discovering with `--bt`. Mixer, console level and
  every file restored exactly; the owner's kernel log is never cleared.

The helper's `BRICK_SUSPEND_ROOT`, `BRICK_POWER_STATE`, `BRICK_SUSPEND_RTC`, `BRICK_SUSPEND_RUN`, `BRICK_SUSPEND_STATS`,
`BRICK_SUSPEND_LOG`, `BRICK_RFKILL`, `BRICK_HW`, `BRICK_BT`, `BRICK_UBUS`, `BRICK_WPA_INIT`, `BRICK_PIDOF`, `BRICK_IFCONFIG`,
`BRICK_SUSPEND_SETTLE`, `BRICK_INPUTD_SUSPEND`, `BRICK_SYNC` and `BRICK_SUSPEND_ENTER` move paths or replace commands for host fixtures; on the device none is set
and the helper writes `/sys/power/state` itself.

## Waking by the power button (checked 08.11.)

The AXP2202 power chip carries the power key; its interrupt reaches the SoC through the NMI line (`/proc/interrupts`:
`379 sunxi-8i-nmi … axp2202_irq_chip`, the key as `axp20x-pek-dbf`/`-dbr` on it), and the chip's device is an enabled
wakeup source (`…/i2c-6/6-0034/power/wakeup` = enabled). A real press woke the Brick from `mem`: `/sys/power/pm_wakeup_irq`
read 379 after it (344, the RTC, after the alarm wakes), 2 s in the kernel's sleep against a 120-s safety alarm, then the
pause menu as above. (The wakeup-source counters of `6-0034` stay 0 on this kernel; `pm_wakeup_irq` is the evidence.)

If a Brick ever does not come back: the game was saved before it slept (the suspend waits for the confirmed state). Once
awake, holding the power button for 6 s saves and switches off as always (`brick-hw power-watch`, `brick-power-off`). For a
system that never resumes only the power chip's own long-press cut-off is left; that one was not tried here (it would switch
the test Brick off).

At one wake of about 45 the Wi-Fi chip's re-initialisation logged `mmc2: error -22 whilst initialising SDIO card` and Wi-Fi
was back 7 s later as usual; `mmc2` is the Wi-Fi chip, not the SD card.

## Not measured yet

- Battery drain asleep against screen off: the AXP2202 has no `current_now`/`power_now` and the capacity is whole percent; it
  needs about 20 minutes unplugged per state (USB was online all of 08.11.).

## References

How others do it (read, nothing copied: NextUI is PolyForm Noncommercial, MinUI has no licence):
- [NextUI suspend sequence](https://github.com/LoveRetro/NextUI/blob/main/skeleton/SYSTEM/tg5040/bin/suspend): stops
  `bluetoothd` and `wpa_supplicant`, `mem` up to five times, a long false "failure" counts as slept, services back in the
  background.
- [NextUI failed-sleep WLAN recovery discussion](https://github.com/LoveRetro/NextUI/issues/594)
- [Stock input polling handshake and Brick wake evidence](https://github.com/LoveRetro/NextUI/pull/836)
- [MinUI fake sleep](https://github.com/shauninman/MinUI/blob/main/workspace/all/common/api.c) (screen off only)
- [TrimUI firmware release notes](https://github.com/trimui/firmware_brick/releases)
