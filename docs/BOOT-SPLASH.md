# The boot screen: a quiet start, pocket-splash, and the fade into the shelf

Status 2026-10-27: the first reboot on the owner's Mini V2 with the quiet start applied and the unit installed has
happened; the boot screen itself has not been seen on a device yet. That boot's journal measured the two things the
first build got wrong — the frame buffer comes about a second after systemd starts the program, and the look for
sway fired on ROCKNIX's `sway-touch.sh` — plus a wrong key name in the unit. All three are fixed and covered by host
tests below (**host tests only** for the fixes, [STATUS.md](STATUS.md)); the next reboot is their device test, at the
end. The research behind it is outside the repository
(`RESEARCH-BOOT-SPLASH-2026-10-26.md`); the boot chain itself is in [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md).

[V] marks something checked (a file, a source, a test). [I] marks something inferred.

## What the player sees

Before: Retroid's logo, then about 20 seconds of small scrolling text (kernel messages, Tux logos, systemd's
"[ OK ] Started …" lines), then the shelf appears at once.

Now:

1. Retroid's logo (the bootloader's; not ours to change).
2. Black. No kernel text: the kernel command line has `quiet loglevel=3 vt.global_cursor_default=0`, and GRUB prints
   nothing ([Quiet start](#quiet-start)).
3. When the display driver is up (3.50 s on the first measured boot; the program itself starts with systemd at
   2.47 s and waits): **pocket-splash**. On true black, pocket-ui's mark fades in (400 ms): the cartridge with the
   "p" on its label (`ui_art::Art::Logo`, the logo pack's pixel art, whole pixels: two screen pixels a pixel at 480
   lines, three at 720, four from 960) over the "pocket-ui" wordmark in Barlow Condensed (Title 50, 67 from 960
   lines, Chalk), in the
   middle of the screen (since 2026-10-30; before, one spine of the shelf with the wordmark). A 6 px bar as wide as
   the wordmark sits under it and holds each console colour for 1.2 s, then crossfades to the next in 400 ms (NES, GB,
   GBC, GBA, SNES, N64, round again). Nothing else moves; there is no progress bar, because there is no honest
   progress to show.
   - The first start after setting up a card (no `/storage/.configured`) says "Getting ready for the first start…"
     in Dust at the bottom left, where the hints go.
   - After a minute without sway it says "Still starting…". It never just goes blank.
   - After about three minutes the bar holds still (nothing more to draw, no more CPU) until the end.
   - Settings → Sound & feel → Motion: Off shows the picture still, with no fades; Reduced keeps the fades (nothing on
     it moves anyway).
4. When sway's unit starts, the picture fades to black (250 ms). sway comes up on black.
5. pocket-ui's first frame fades in from black (300 ms), in the dark and the light look alike.

Snapshots (`pocket-splash --snapshot`, below): the normal picture, the first start's line, the late line at 1920x1080,
and a frame of the fade-out.

## Quiet start

The SD image's `deploy/sd-efi/grub.cfg` has, since 2026-10-26:

```
linux /boot/pocket/Image boot=LABEL=ROCKNIX disk=LABEL=STORAGE rootwait quiet loglevel=3 vt.global_cursor_default=0 console=tty0 video=efifb:off gpt
```

and no `echo "pocket-ui: loading..."` before it.

- `quiet` keeps the kernel's lines at `info` and below off the screen and the Tux logos away (fbcon skips the logo at
  the quiet log level), silences the EFI stub's lines, and makes ROCKNIX's initramfs set `printk` to `1 4 1 7`. [V]
  (kernel and ROCKNIX sources, read for the research)
- `loglevel=3` leaves errors only. `vt.global_cursor_default=0` hides the console's blinking cursor.
- Everything that is no longer on the screen is still in `dmesg` and the journal, and Report a problem collects both.
  To watch a boot again, take the three words out.
- **Never `fbcon=rotate:` or `fbcon=map:`**: ROCKNIX turns sway's output by `/sys/class/graphics/fbcon/rotate`, and
  pocket-ui's touch mapping reads the same file. fbcon without a frame buffer reports 0 there, and both the picture
  and touch would be turned wrong. `scripts/image/check.sh` refuses an image whose `grub.cfg` has `fbcon=` or `echo`.
- ROCKNIX's own boot (its ABL and boot image) has `quiet` already, so a card set up with the install package needs
  nothing.

### The small white version lines that were still there

After the first reboot with the quiet start, the owner still saw very small white version info on black before the
shelf. It is not kernel text (`quiet` held), and it is not ROCKNIX's `show-version.service` either: that unit runs
`/usr/bin/lsb_release` onto the tty unless the command line has ROCKNIX's own `morequiet`, but in the 20261001
build the ROCKNIX project's `lsb_release` script (the project busybox package wins over the global one; the
journal's BusyBox 1.36.1 is the project's version) only reads `/etc/os-release` and prints nothing — so
`morequiet` would change nothing, and no command-line word was added for it. [V] (ROCKNIX sources, tag 20261001)

What prints it is ROCKNIX's initramfs: its `load_splash` writes `/etc/issue` — "… Version: 20261001 (…) / …
Built: …" — and "==> Loading, please wait…" to `/dev/console`, unconditionally (`quiet` there only sets the
kernel's `printk` levels; no command-line word switches these lines off). systemd's own first lines
("systemd 255.8 running in system mode…") land on the same console. All of it is written before the frame buffer
exists, so it sits in the console's text buffer and is painted in the moment the frame buffer console binds
(3.50 s on that boot) — on that boot the splash had already ended, so the lines stayed until sway. The splash now
covers this properly: graphics mode from its start (the buffered text is never painted) and the picture the
moment the frame buffer opens. Stopping the lines at their source instead would mean the initramfs'
`/flash/post-flash.sh` hook on the owner's card (the SD image already carries one) — that is a new OK from the
owner, so it is described here and not done. The sideways "vertical stuff" before all this is Retroid's
bootloader logo (and possibly u-boot's own lines): the research's decision *e*, out of scope.

### On a card set up before the image had it

`deploy/grub-quiet.sh` makes the same change on the card's `/flash/boot/grub/grub.cfg`. From the computer:

```sh
scripts/boot-quiet.sh status     # read-only: quiet or not, backup or not, the kernel line
scripts/boot-quiet.sh apply      # keeps grub.cfg as grub.cfg.before-quiet, writes the quiet one
scripts/boot-quiet.sh undo       # puts grub.cfg.before-quiet back, removes it
```

(or on the device: `sh grub-quiet.sh apply`). It takes effect at the next start.

- It refuses a `grub.cfg` that is not pocket-ui's: no `menuentry "pocket-ui"`, or not exactly one
  `linux /boot/pocket/Image … boot=LABEL=ROCKNIX disk=LABEL=STORAGE` line. Nothing is written then.
- Only the kernel line and the `echo` line change; it checks that every other line is the same before it writes. A
  `loglevel=` or `vt.global_cursor_default=` already there is set to the value above, once. Running it again changes
  nothing.
- `/flash` is mounted read-only by ROCKNIX: it is remounted read-write for the write and read-only again after it.
  The new file is written next to the old one and renamed over it.
- **If the card does not start any more:** Boot mode: Android (Volume Down + Power, Switch boot mode), put the card in
  a computer and copy `boot/grub/grub.cfg.before-quiet` over `boot/grub/grub.cfg` on its first partition (label
  `ROCKNIX`).
- Tests: `deploy/test-grub-quiet.sh` (part of the gate), against a fake `/flash` with the old `grub.cfg` as fixture,
  under dash, bash and BusyBox sh: apply (the kernel line becomes the image's, byte for byte), idempotence, other words
  kept, words already there not doubled, backup and undo byte for byte, undo without a backup, and refusals (another
  system's `grub.cfg`, no or two kernel lines, no `disk=`, an empty or missing file, a foreign backup).

## pocket-splash

`src/bin/pocket_splash.rs` (the program: frame buffer, console, signals) and `src/splash.rs` (the picture, the timing,
the conversion to the frame buffer, the loop; everything the host tests reach). One program of this repository, built
like the others (`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-splash`). It reuses
pocket-ui's drawing (`gfx::Canvas`, `text::Text::builtin()` with the embedded Barlow, the German table): no font file is
opened at boot.

### How it draws

- **Through the frame buffer, never through DRM.** It writes to `/dev/fb0`, the kernel's frame buffer emulation on
  top of the msm DRM driver, and never opens `/dev/dri`: the first program to open the DRM device becomes DRM master,
  and sway could not take the screen. The frame buffer needs no master, so nothing the splash does can hold sway up.
  ROCKNIX's own splash draws the same way on this panel. [V] (research) The research suggested the frame buffer for
  the same reason.
- **The layout from the frame buffer.** Size, bytes a row (`line_length`, never width × 4: that mistake is the
  sheared text of an old u-boot), bits per pixel and the colour offsets come from `FBIOGET_VSCREENINFO` and
  `FBIOGET_FSCREENINFO`; 16-, 24- and 32-bit true colour work, anything else means no picture.
- **The turn from fbcon.** `/sys/class/graphics/fbcon/rotate` (the kernel sets it from the panel's `rotation`; ROCKNIX
  turns sway by it, pocket-ui turns touch by it): 1 on both Minis (the 1080x1240 panel shows a 1240x1080 picture, its
  top left corner at the panel's top right), 3 on the Pocket 5 and Flip 2. The picture is painted landscape and turned
  in software; any size works (the picture sits in the middle; the first-start line 72 px from the left).
- **Only what changes.** Frames are painted only when the picture changes (the fades and crossfades), at most 30 a
  second, and nothing while a colour holds. While only the bar's colour moves, only the bar is repainted. Only
  the frame buffer rows that changed are written, in one write: the bar's columns of the picture are the rows on the
  Mini V2's turned panel (about 190 rows, 0.8 MB, for a colour step).
- **Waiting for the frame buffer.** It is not there when systemd starts the program: the first device boot
  measured the program starting at 2.47 s and the msm display driver binding at 3.50 s, and `/dev/fb0` exists only
  after that. The program looks for it every 25 ms for up to 20 s (`splash::wait_for_fb`), then draws at once — the
  picture's clock, and with it the fade-in, starts when the frame buffer opens, not at the program's start. The
  wait holds nothing up: the unit is `Type=simple`, so systemd went on as soon as the program started, and sway
  starting (or a stop) during the wait ends it just the same. There is no systemd ordering on the frame buffer
  device instead: nothing in that boot's journal shows a `dev-fb0.device`, so there is no device unit to order on.
- **The console in graphics mode** (`KDSETMODE KD_GRAPHICS` on `/dev/tty0`) from the program's start — before the
  wait, not with the first frame — so no kernel or systemd line is drawn over the picture, and so the text already
  in the console's buffer (the version lines, below) is never painted when the frame buffer console binds: it
  would appear in the same moment the frame buffer does, before any frame could cover it. It is left that way at
  the end of a run that showed the picture (sway takes the console over); every way out that shows no picture —
  the wait ending without a frame buffer, a pixel format it can't write, a write that fails — puts the text mode
  back first, so a boot that ends on the console still shows the console, buffered text and all.

### When it ends

- **sway starts.** The program looks at `/proc` five times a second for a process of sway's unit, by its whole
  name (`sway-config`, the unit's ExecStartPre and the first to run, `sway.sh`, `sway`, also under systemd's name
  for a process about to start one). The whole name matters: the first build matched a prefix, and ROCKNIX's
  `sway-touch.sh` — its own unit, wanted by `rocknix.target`, running from 3.21 s of the first device boot while
  sway itself comes much later — counted as sway, so that boot's splash gave up at 3.33 s, a blink before the
  frame buffer appeared at 3.50 s; even with a frame buffer it would have ended the picture at once. `swaymsg`,
  `swaybar` and the like are not sway either. It then fades to black (250 ms) and ends. sway itself needs longer than that to reach the screen [I], and if it
  is quicker, nothing is lost: the fade just isn't seen.
- **sway has the screen** (a safety net): once a second it asks the frame buffer to pan; the kernel refuses that with
  EBUSY while another program is DRM master. Then it ends at once, without another write.
- **Told to stop** (SIGTERM, SIGINT: a shutdown, `systemctl stop pocket-splash`): the same fade, then it ends.
- **Anything wrong** (no frame buffer within 20 s, a pixel format it can't write, a write that fails): no picture, exit
  0, and the console's text mode back. The boot goes on as without it.
- Started while sway runs (by hand): it sees sway and ends without touching the screen.

### The unit, and why not `Conflicts=sway.service`

`deploy/pocket-splash.service`, `Type=simple`, `DefaultDependencies=no`, `Conflicts=` and `Before=` only
`shutdown.target`, `ConditionFileIsExecutable=/storage/pocket-ui/current/pocket-splash`, `TimeoutStopSec=1`, linked from
`sysinit.target.wants/`. It is ordered before nothing of the boot, so it can't hold anything up. (The condition's
key is `ConditionFileIsExecutable`; the first build shipped `ConditionPathIsExecutable`, which is no key systemd
knows — the first device boot's journal says so, and the unit simply ran without its condition.
`splash::tests::the_units_keys_are_ones_systemd_knows` parses the unit in the gate, and `systemd-analyze verify`
(255, the device's major version) passes it with no word.)

The research proposed `Conflicts=sway.service` with `Before=sway.service` for the hand-over. That would stop sway from
starting at all: at boot, `pocket-splash` (pulled in by `sysinit.target`) and `sway.service` (pulled in through
`essway.service`'s `Requires=` from `rocknix.target`) are in the **same** transaction, and when two conflicting jobs
are both only wanted, systemd keeps the start of the unit that has the `Conflicts=` and drops the other one's start
(`delete_one_unmergeable_job()` in systemd's `transaction.c`: "if something is being stopped because it is
conflicted by another unit … we rather remove the start"). [I] from systemd's source and documentation, not tried; the
risk alone rules it out. So the program watches for sway itself.

### Where it lives, and how it comes and goes

The same folder and the same way out as the essway drop-in: nothing of ROCKNIX's is edited.

| | Program | Unit | Removed by |
|---|---|---|---|
| Owner's card (by hand, `dev.sh`) | `scripts/dev.sh deploy` copies `pocket-splash` into `releases/dev` | the same run writes `/storage/.config/system.d/pocket-splash.service` and the link `sysinit.target.wants/pocket-splash.service` when the essway drop-in is there, then `daemon-reload`; never starts it | `scripts/autostart.sh uninstall --apply` (with the drop-in), or deleting the two files |
| `scripts/autostart.sh install --apply` | (from `dev.sh deploy`) | the unit and the link with the drop-in | `autostart.sh uninstall --apply` |
| Install package | `files/pocket-splash` → `releases/<version>/`, checked with `--version` like the other two | `files/pocket-splash.service` → the unit and its link, after systemd took the drop-in | `uninstall.sh` (also `--keep-programs`: the boot screen goes with the start entry); a unit of that name that isn't ours is left alone |
| SD image | `releases/<version>/pocket-splash` on STORAGE | `.pocket-seed/.config/system.d/pocket-splash.service` and the link, put in place with the drop-in at the second boot by `mount-storage.sh` (`cp -a` keeps the link) | as for the package |
| Updates (`release-app.sh`, `release-channel.sh`) | the tarball carries `pocket-splash`; the unit runs `current/pocket-splash`, so it follows the version in use | unchanged | a version without it: no boot screen, nothing else |

`scripts/autostart.sh status` says whether the boot screen is installed.

### Numbers (build computer)

`splash::tests::twenty_seconds_of_the_boot_screen_cost_little_cpu`: 20 s of the boot screen at 1240x1080 turned into
the Mini V2's 1080x1240 frame buffer, painted, turned and copied as on the device, with the first-start line. On the
build computer (2026-10-26, gate profile): 177 frames, 8.7 a second on average (30 a second during a fade or a
crossfade, none while a colour holds), 0.67 ms of work a frame, **0.58 % of one core**; the test's bound is 10 %, and
it prints its numbers with `--nocapture` (these figures are from the spine picture before 2026-10-30; the cartridge's
fade writes about 210 rows, a colour step about 190). Memory: the picture and a copy of the frame buffer, 5.4 MB each. On the device:
not measured yet (its end line in the journal says frames, bytes and CPU seconds).

## The fade into the shelf

`src/boot_fade.rs`: pocket-ui's first frames after the Pocket started come up from black over 300 ms (eased out), so
the boot reads as one: the mark, dip to black, shelf. Only at the first start since the device started: pocket-ui leaves
`/run/pocket-ui-started` (a tmpfs, empty at every start), so Restart pocket-ui, a restart by the launcher or an update
show the first frame as it is, and waking from sleep starts nothing. The fade starts at the first frame painted, not
at the program's start, and adds nothing to the time to the first frame. Motion Off: no fade (the mark is still left).

## Tests

- `cargo test` (gate): `splash::tests` (fades, holds, the late and first-start lines, Motion Off and Reduced, frame
  pacing, what each update repaints, the picture on black at the four screen sizes, the turn for every fbcon value with
  padded rows checked against the definition of each turn, RGB565, the loop ending when told, when sway starts, when the
  screen is taken and when a write fails, sway seen in a fake `/proc` — and `sway-touch.sh` and its kin *not* seen as
  sway, the wait for the frame buffer against a fake one that appears late and one that never appears, the wait ending
  for sway and for a stop, the unit's keys parsed, the Motion setting read leniently, the CPU
  budget), `boot_fade::tests`, and in the program the size and offsets of the kernel's two structs.
- `deploy/test-grub-quiet.sh` (gate), `deploy/package/test-package.sh` (gate: the unit and its link installed,
  never started, kept on a second run, removed by uninstall and `--keep-programs`, a foreign unit left alone).
- `scripts/image/check.sh`: the image's kernel line, no `echo`, no `fbcon=`; the unit and link in `.pocket-seed`, the
  program in the release, an aarch64 program.
- Snapshots: `pocket-splash --snapshot OUT.png [--size WxH] [--at S] [--stop-at S] [--first-start] [--motion off]`
  (the picture landscape, without a screen).

## Device test (for the person at the device)

On the owner's card, after `scripts/dev.sh deploy` and `scripts/boot-quiet.sh apply`:

1. `scripts/boot-quiet.sh status`: "quiet start: on", backup named. `ls /storage/.config/system.d/` shows
   `pocket-splash.service` and `sysinit.target.wants/`; `systemctl is-enabled pocket-splash` is not "masked" (it is
   linked, so "enabled" or "static" [I]).
2. Restart the device and film it from power-on to the shelf. Expected: Retroid logo (sideways, not ours); black —
   in particular *not* the small white version lines of the first reboot (they are still written to the console's
   buffer, but the splash has it in graphics mode from about 2.5 s, before the frame buffer console binds at about
   3.5 s); the cartridge and wordmark fading in (from about 3.5 s, when the frame buffer appears) and the bar under
   them changing colour; a dip to black; the shelf fading in. Check that the picture is the right way up and not
   sheared, and that no text is
   drawn over it. A version line flashing for a blink at the moment the panel lights would mean the frame buffer
   console paints its buffer on bind despite graphics mode on this kernel — the first (black) frame then covers it
   within about a frame; note it on the video either way.
3. Over SSH: `cat /proc/cmdline` (the three words); `journalctl -b -u pocket-splash` (its first line names the frame
   buffer, the turn and the size; its last line why it ended — "sway is starting" expected — with frames, MB and CPU
   seconds); `systemd-analyze critical-chain sway.service` and `journalctl -b -o short-monotonic | grep -E
   'pocket-splash|sway|essway'` for when each started; `dmesg | grep -i -E 'fb|drm.*(timed out|WARN)'` for anything
   the frame buffer writes or pans caused.
4. The touch screen still lands where the finger is, and the picture is turned as before (fbcon was not touched).
5. Restart pocket-ui from the power menu: no fade this time. Sleep and wake: no fade.
6. Undo test (optional): `scripts/boot-quiet.sh undo`, restart: the text is back; `apply` again.

## Open

- The picture on the device (above). Settled by the first reboot's journal: `/dev/fb0` is *not* there when systemd
  starts the program (2.47 s; the display driver binds at 3.50 s), which is why the program now waits up to 20 s.
  Still open: whether the frame buffer shows the writes without a pan, whether the pan check gives EBUSY once
  sway runs ([I] from the kernel's `drm_fb_helper_pan_display`), and whether graphics mode set before the frame
  buffer console binds keeps its buffered text unpainted on this kernel (7.2) — the next reboot's video tells.
- With HDMI connected at boot, ROCKNIX sets fbcon's rotation to 0: the splash then draws unturned on whatever fb0 is.
- Starting it from the initramfs (about a second earlier, image only) was not done; the research's step 3 needs the
  owner's OK.
- On a card with ROCKNIX's own boot, ROCKNIX's logo still shows from the initramfs before the splash (the image's
  `post-flash.sh` replaces it with one line of text).
