#!/bin/sh
# Installs the built Flatpak in a container on DECK_BUILD_HOST (an ssh host with docker) and runs it under Gamescope (headless, 1280x800); fetches the screenshots.
#   test-flatpak.sh [demo home with Emulation/roms]   -> target/deck-gamescope/
set -eu
here=$(cd "$(dirname "$0")" && pwd); home=${1:-$HOME/work/deck-demo/home}
host=${DECK_BUILD_HOST:?an ssh host with docker}; work=pocket-deck-test; out="$here/../../target/deck-gamescope"
ssh "$host" "docker run --rm -v \$HOME:/h alpine rm -rf /h/$work; mkdir -p $work/home"
tar -C "$home" -cf - Emulation | ssh "$host" "tar -C $work/home -xf -"
scp -q "$here/../../target/pocket-ui-deck.flatpak" "$host:$work/"; scp -q "$here/flatpak/test-gamescope.sh" "$host:$work/"
ssh "$host" "cd $work && docker run --rm --privileged --security-opt seccomp=unconfined --security-opt apparmor=unconfined -v pocket-flatpak:/var/lib/flatpak -v \$PWD:/work -w /work pocket-flatpak-env sh /work/test-gamescope.sh" 2>&1 | tail -60
mkdir -p "$out"; scp -qr "$host:$work/shots/." "$out/" 2>/dev/null || true; ls "$out"
