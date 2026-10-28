#!/bin/sh
# Storage report: a pocket-store test tool, version 1.0.
# Installed into pocket-ui's tools folder (/storage/.config/pocket-ui/tools; ROCKNIX's own Tools folder is
# emptied at every boot); the Store and Settings > Tools run it and show its last line.
games=$(find -L /storage/roms -mindepth 2 -maxdepth 2 -type f ! -name '.*' 2>/dev/null | wc -l | tr -d ' ')
free=$(df -h /storage 2>/dev/null | awk 'NR == 2 { print $4 }')
echo "Storage report 1.0: ${free:-?} free on /storage, ${games} files in the game folders."
