#!/bin/bash
# make.sh <kind> <step> [args]: the steps that made assets/media/<kind>-front.png / -spine.png and case/<kind>.json.
# Run from a scratch directory holding photos/ (the Commons originals, fetched with fetch.py) and specs.json (copy the
# one beside this script). PY = a Python with Pillow; I2P = assets/media/game/tools/pixellab_i2p.py.
#   make.sh <kind> prep              straighten + paint, prep/<kind>-front.png and -spine.png
#   make.sh <kind> convert           three PixelLab conversions per face into raw/<kind>/ (default, strength 150, 250 + fixer)
#   make.sh <kind> finish F S MEDIA  keep front-F / spine-S: outline, flat label/band, json into MEDIA (assets/media)
set -e
T=$(cd "$(dirname "$0")" && pwd)
PY=${PY:-python3}
k=$1
case $2 in
prep)
  mkdir -p prep
  for p in front spine; do $PY -I "$T/case_prep.py" specs.json "$k" $p prep/$k-$p.png; done ;;
convert)
  mkdir -p raw/$k
  for p in front spine; do
    read W H < <($PY -I -c "import json;print(*json.load(open('specs.json'))['$k']['$p']['size'])")
    $PY -I "$I2P" prep/$k-$p.png raw/$k/$p-1.png $W $H &
    $PY -I "$I2P" prep/$k-$p.png raw/$k/$p-2.png $W $H --strength 150
    wait
    $PY -I "$I2P" prep/$k-$p.png raw/$k/$p-3.png $W $H --strength 250 --fixer
  done ;;
finish)
  M=$5; mkdir -p "$M/case"
  $PY -I "$T/case_finish.py" specs.json "$k" front raw/$k/front-$3.png "$M/$k-front.png"
  $PY -I "$T/case_finish.py" specs.json "$k" spine raw/$k/spine-$4.png "$M/$k-spine.png"
  $PY -I "$T/write_json.py" specs.json "$k" $3 $4 "$M/case/$k.json" ;;
esac
