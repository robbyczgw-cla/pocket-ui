#!/bin/sh
# pocket-ui defaults for the Retroid Pocket Mini V2 on ROCKNIX. Idempotent; keeps one backup of each file.
#
# Already right in stock ROCKNIX, so not touched here:
#   global.autosave=1 -> savestate_auto_save/load (instant resume via .state.auto)
#   savestate_thumbnail_enable=true -> .state.png previews for the quick menu
#   pause_nonactive=true -> RetroArch pauses while pocket-ui's quick menu has focus

RA=/storage/.config/retroarch/retroarch.cfg
[ -f "$RA.pocketui-backup" ] || cp "$RA" "$RA.pocketui-backup"

set_ra() {
  if grep -q "^$1 = " "$RA"; then
    sed -i "s|^$1 = .*|$1 = \"$2\"|" "$RA"
  else
    echo "$1 = \"$2\"" >>"$RA"
  fi
}

# Quick menu: save, load and quit are sent as network commands on localhost:55355.
set_ra network_cmd_enable true
set_ra network_cmd_port 55355

grep -E "^(network_cmd_enable|network_cmd_port|pause_nonactive|savestate_thumbnail_enable) " "$RA"
