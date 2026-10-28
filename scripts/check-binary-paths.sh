#!/bin/sh
# Fails if a built binary carries a build machine path: the home folder of whoever built it (/home/<user>,
# /Users/<user>, $HOME) or the repository path. Usage: scripts/check-binary-paths.sh <binary>...
# Release builds avoid them with scripts/release-rustflags.sh.
[ $# -gt 0 ] || { echo "usage: $0 <binary>..." >&2; exit 2; }
root=$(cd "$(dirname "$0")/.." && pwd -P)
rc=0
# the two home prefixes, kept apart so this file itself reads as no home path to the public export check
h='/home/'
u='/Users/'
for bin in "$@"; do
  [ -f "$bin" ] || { echo "check-binary-paths: $bin: no such file" >&2; rc=2; continue; }
  # -o: strings glues neighbouring literals together, so print only the path-like part of each hit
  hits=$(strings -a "$bin" | grep -a -o -E "($h|$u|$root|${HOME:-/nonexistent}/)[A-Za-z0-9_./-]{0,50}" | sort -u)
  if [ -n "$hits" ]; then
    echo "check-binary-paths: $bin carries build paths ($(echo "$hits" | wc -l) distinct, first ones):" >&2
    echo "$hits" | head -5 | sed 's/^/  /' >&2
    [ $rc -eq 0 ] && rc=1
  else
    echo "check-binary-paths: $bin clean"
  fi
done
exit $rc
