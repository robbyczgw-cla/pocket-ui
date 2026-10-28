# Updates

Settings → System → **Updates** ("New versions of pocket-ui and the system."). It needs Wi-Fi.

> **No update has been published yet.** Until the first one is, a check ends with "The update server has no update
> list right now. Try again later." Without a network it says "Couldn't reach the update server. Check Wi-Fi.", and
> on a network that wants a sign-in first (a hotel, a train) "Connected, but this network wants a sign-in first. Open
> Settings › Connections › Wi-Fi." ([Hotel and train Wi-Fi](hotel-wifi.md)).

## The Updates page

The first time you open the page, pocket-ui checks for updates by itself. The page shows:

- **pocket-ui**: the installed version, and a newer one if available ("0.2.0, 0.2.1 available").
- **System**: the installed system version, and a newer one if available, or "update waiting for
  restart" when a system update has been downloaded.

Below that are the actions that apply right now:

| Action | What it does |
|---|---|
| **Install pocket-ui …** | Downloads and installs the new pocket-ui. It restarts by itself ("Installed. Restarting…"). |
| **Get system …** | Downloads and checks a new system version. It is installed the next time the system starts. |
| **Remove the waiting system update** | Cancels a system update that was downloaded but not installed yet. |
| **Check again** | Checks for updates again. |

While something downloads, a progress bar shows how far it is, and **A** cancels the download
("Download cancelled."). A download that stalls (under 10 KB/s for 30 seconds) is retried and then
given up with "Download failed. Check Wi-Fi." The release notes of the selected
update appear at the bottom. If everything is current, the page says "Everything is up to date."

## What makes it safe

**Only our updates are accepted.** The list of available updates is signed. If the signature does
not match, pocket-ui stops: "The update list isn't signed by us. Nothing was changed."

**Every download is checked** against the size and checksum in that signed list. A damaged or
changed download is deleted: "The download is damaged or was changed. Nothing was installed." pocket-ui only
downloads over secure connections (https).

**pocket-ui updates have an automatic way back:**

1. The new version is unpacked next to the current one, which stays untouched.
2. Before switching, pocket-ui checks that the new version actually starts on this device. If it
   does not: "… doesn't start on this device. Nothing was changed."
3. Then it switches to the new version in one step and restarts.
4. If the new version crashes, or does not show its first screen within 20 seconds, the previous
   version comes back by itself, and you see: "Version … didn't start properly, so the last version that worked is back."

The previous version is kept on the SD card until the next update replaces it. So is the last version that ran for two
minutes with its screen: if a later version keeps stopping, that one comes back, and if it stops too, pocket-ui opens
in [safe mode](safe-mode.md).

## System updates and the bootloader

The bootloader is the small program in the device's internal storage that starts Android. A
damaged bootloader can stop the device from starting at all.

ROCKNIX's own updates can rewrite this bootloader. pocket-ui does not allow that. Before a system
update is allowed to install, pocket-ui opens it and refuses it if:

- it contains ROCKNIX's tool for rewriting the bootloader,
- its install scripts would write to the bootloader or to any partition,
- it is made for a different device, or
- its version does not match the signed list.

In each case the update is not installed, and the page says why, for example "This system update
would touch the bootloader or a partition". **Standard ROCKNIX updates are therefore always
refused**; system updates come only from our own system build, which leaves the bootloader out.

## System updates that would break pocket-ui

pocket-ui is made separately from the system, so a new system could lack something it needs. Before a system update is
allowed to install, pocket-ui also looks inside it for what pocket-ui and its games use:

- **Refused** if pocket-ui could not start or play with it: an older system library than pocket-ui was made for,
  a library its window or the games' picture needs, sway (which draws the screen), the program that starts the
  other emulators, or EmulationStation (the way back when pocket-ui can't start). The page says what is missing,
  for example "This system update would break pocket-ui (libxkbcommon.so.0 is missing: pocket-ui can't open its
  window). It was not installed."
- **Installed, with a note** if only a part would stop working: Wi-Fi, sound, Bluetooth or time zone tools, or the
  emulator (core) of a system pocket-ui plays itself, for example "Ready. It installs the next time the system
  starts. Note: no gambatte core in the new system, so gb, gbc games would not start." A core you installed from
  the Store is kept and still counts.

The system update itself is installed by the system during the next start, from the SD card.

### The kernel on the SD card

With the dual-boot setup ([install guide](../INSTALL-DUAL-BOOT.md)) the system's core part (the kernel) is
copied to `boot/pocket/Image` on the SD card by `scripts/make-sd-efi.sh`. A system update replaces `KERNEL` and
`SYSTEM` but not that copy, so the boot files have to be made again from the new version. Making them
automatically after an update is planned. If you start the system from a computer with `fastboot boot`, the
Updates page reminds you: "This device starts from a computer: make a new boot image from the update first."

System updates have not been tested on a device yet, because our own system build does not exist
yet.
