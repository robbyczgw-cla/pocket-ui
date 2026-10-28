#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (C) 2022-present JELOS (https://github.com/JustEnoughLinuxOS)
# Adapted from ROCKNIX's start_portmaster.sh (same licence) for pocket-ui; see NOTICE.md.
# Prepares PortMaster the way ROCKNIX's start_portmaster.sh does, without opening its GUI,
# so pocket-ui can install ports with harbourmaster. Safe to run again.
. /etc/profile >/dev/null 2>&1

CFG=/storage/.config/PortMaster
PM=/storage/roms/ports/PortMaster

[ -d "$CFG" ] || { mkdir -p "$CFG"; cp -r /usr/config/PortMaster /storage/.config/; }
cp /usr/config/PortMaster/control.txt /usr/config/PortMaster/mapper.txt "$CFG/"
chmod +x "$CFG/control.txt" "$CFG/mapper.txt"
ln -sf /usr/config/SDL-GameControllerDB/gamecontrollerdb.txt "$CFG/gamecontrollerdb.txt"

if [ ! -d "$PM" ] || [ ! -f "$PM/pugwash" ]; then
  rm -rf "$PM"
  unzip -q /usr/config/PortMaster/release/PortMaster.zip -d /storage/roms/ports/
  chmod +x "$PM/PortMaster.sh"
fi
rm -rf "$PM/tasksetter"
cp "$PM/gptokeyb" "$CFG/gptokeyb"
cp "$CFG/control.txt" "$CFG/mapper.txt" "$PM/"
cp -L "$CFG/gamecontrollerdb.txt" "$PM/gamecontrollerdb.txt"
cp /usr/bin/oga_controls* "$PM/"
# PortMaster.sh (its GUI, which pocket-ui never opens) does this on every start; PortMaster.zip itself
# carries no execute bits, so without it gptokeyb can't run and ports that read the keyboard get no controls.
chmod -R +x "$PM"
echo "PortMaster ready in $PM"
