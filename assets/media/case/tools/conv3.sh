#!/bin/bash
# conv3.sh <kind> <part>: three PixelLab conversions (default, strength 150, strength 250 + fixer), at most 2 at once.
cd "$(dirname "$0")/.."
V=~/pocket-review/scratch/cases-ref/venv/bin/python; P=tools/pixellab_i2p.py
k=$1; p=$2
read W H < <(python3 -c "import json;print(*json.load(open('specs.json'))['$k']['$p']['size'])")
mkdir -p raw/$k
[ -s raw/$k/$p-1.png ] || $V -I $P prep/$k-$p.png raw/$k/$p-1.png $W $H > raw/$k/$p-1.log 2>&1 &
[ -s raw/$k/$p-2.png ] || $V -I $P prep/$k-$p.png raw/$k/$p-2.png $W $H --strength 150 > raw/$k/$p-2.log 2>&1
wait
[ -s raw/$k/$p-3.png ] || $V -I $P prep/$k-$p.png raw/$k/$p-3.png $W $H --strength 250 --fixer > raw/$k/$p-3.log 2>&1
echo done $k $p
