#!/bin/sh
# Runs hello-pad in sway in place of EmulationStation, then brings EmulationStation back.
# Start detached from SSH with: systemd-run --unit=pocketui-hello /storage/pocket-ui/run-hello-pad.sh

export XDG_RUNTIME_DIR=/var/run/0-runtime-dir
export WAYLAND_DISPLAY=wayland-1

systemctl stop essway
/storage/pocket-ui/hello-pad >/var/log/hello-pad.log 2>&1
echo "exit code $?" >>/var/log/hello-pad.log
systemctl start essway
