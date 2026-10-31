#!/bin/bash
# pocket-ui SD image: ROCKNIX settings that differ on this card, set once at the first start. The image puts this file
# in /storage/.pocket-seed/.config/autostart/; deploy/image/mount-storage.sh moves it to /storage/.config/autostart at the
# second boot, and ROCKNIX's autostart runs it after its own setup (which copies system.cfg into place) and before the
# user interface starts. It then deletes itself: a later change of either setting stays the player's.
#   - updates.enabled=0: EmulationStation offers no ROCKNIX updates (the boot hook would refuse them anyway).
#   - system.hostname=pocket-ui: the name on the network, in Bluetooth and Tailscale (ROCKNIX's default is the
#     device family, SM8250). network-base-setup already named the device for this start; the new name applies now too.
. /etc/profile.d/001-functions
set_setting updates.enabled 0
set_setting system.hostname pocket-ui
{ echo pocket-ui >/proc/sys/kernel/hostname; } 2>/dev/null
hostnamectl --transient hostname pocket-ui >/dev/null 2>&1
avahi-set-host-name pocket-ui >/dev/null 2>&1
rm -f "$0"
true
