#!/bin/sh
# Stand-in for ROCKNIX's /usr/bin/runemu.sh on a Steam Deck (POCKETUI_RUNEMU, docs/STEAMDECK.md): pocket-ui calls it as
#   runemu.sh <rom> -P<system> --core=<core> --emulator=<name> --controllers=
# For the big emulators <name> is the emulator's Flatpak id (gen-es-systems.py writes it). Quitting the emulator ends this
# script, and pocket-ui is back on screen.
rom=$1; emu=
for a in "$@"; do case $a in --emulator=*) emu=${a#--emulator=};; esac; done
# Inside our own Flatpak the other Flatpaks are reached through the host (needs --talk-name=org.freedesktop.Flatpak).
if [ -n "${FLATPAK_ID:-}" ]; then fp="flatpak-spawn --host flatpak run"; else fp="flatpak run"; fi
# A Flatpak does not inherit the caller's environment: what routes the sound (a test's null sink) is passed on explicitly.
for v in PULSE_SINK PIPEWIRE_NODE SDL_AUDIODRIVER; do eval "val=\${$v:-}"; [ -n "$val" ] && fp="$fp --env=$v=$val"; done
case $emu in
  net.pcsx2.PCSX2)            exec $fp "$emu" -batch -fullscreen -- "$rom" ;;
  org.DolphinEmu.dolphin-emu) exec $fp "$emu" -b -e "$rom" ;;
  org.ppsspp.PPSSPP)          exec $fp "$emu" --fullscreen "$rom" ;;
  info.cemu.Cemu)             exec $fp "$emu" -f -g "$rom" ;;
  net.rpcs3.RPCS3)            exec $fp "$emu" --no-gui "$rom" ;;
  app.xemu.xemu)              exec $fp "$emu" -full-screen -dvd_path "$rom" ;;
  org.azahar_emu.Azahar)      exec $fp "$emu" -f "$rom" ;;
  io.github.lime3ds.Lime3DS)  exec $fp "$emu" "$rom" ;;
  org.vita3k.Vita3K)          exec $fp "$emu" -F -r "$(basename "$rom" .vpk)" ;;
  "")                         echo "runemu: no --emulator given" >&2; exit 2 ;;
  *)                          exec $fp "$emu" "$rom" ;;
esac
