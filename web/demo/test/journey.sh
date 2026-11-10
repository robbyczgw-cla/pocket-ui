#!/bin/sh
# The demo's journeys as a visitor makes them, on a computer and a phone (docs/WEB-DEMO.md): shelf, a game list, the
# box, Settings, a start with the card, the pause menu over it (the menu must replace the card: a menu frame painted
# while the card was up stayed hidden once), Quit back to the shelf. Exit 1 when a step never comes.
# Each run is a fresh page: a system's first start shows its controls card, and the second A starts from it.
#   sh web/demo/test/journey.sh <url of the page> [out dir]      (PLAYWRIGHT, CHROME as for drive.mjs)
set -eu
url=${1:?usage: journey.sh <url> [out dir]}
out=${2:-$(mktemp -d)}
here=$(dirname "$0")
node "$here/drive.mjs" "$url" "$out/desktop" desktop wait:1500 until:place=shelf key:Enter until:place=shelf:Games \
  key:ArrowRight wait:2500 key:x wait:1200 shot:box key:Backspace wait:600 key:s until:place=settings key:Backspace \
  until:place=shelf key:Enter until:place=controls wait:800 key:Enter until:game=playing wait:4500 card:yes shot:card \
  key:m until:game=paused card:no shot:pause key:ArrowDown*5 key:Enter until:game=none card:no shot:back
node "$here/drive.mjs" "$url" "$out/phone" phone wait:1500 until:place=shelf btn:a until:place=shelf:Games btn:right \
  wait:2500 btn:a until:place=controls wait:800 btn:a until:game=playing wait:4500 card:yes tap:0.5,0.5 \
  until:game=paused card:no shot:pause btn:down*5 btn:a until:game=none shot:back
# Every system's pause menu, row by row, and Quit back to the shelf (a tester: "saving in the demo hangs"; N64's RetroArch
# Quit waited for an exit nobody sent). Home's spines: GBA is selected, N64, PS1 and SNES follow it.
k=0
for sys in gba n64 psx snes; do
  node "$here/drive.mjs" "$url" "$out/pause-$sys" desktop wait:1500 until:place=shelf key:ArrowRight*$k wait:300 key:Enter \
    until:place=shelf:Games wait:600 key:Enter wait:900 key:Enter until:game=playing wait:4500 key:m until:game=paused \
    walkpause:10 until:game=none
  k=$((k + 1))
done
echo "journeys ok: $out"
