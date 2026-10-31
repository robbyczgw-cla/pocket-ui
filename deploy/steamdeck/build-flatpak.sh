#!/bin/sh
# Builds the Flatpak in a container and fetches it: target/pocket-ui-deck.flatpak.
#   build-flatpak.sh <bundle dir from build-bundle.sh>
# Where: DECK_BUILD_HOST=local (default; this x86_64 machine's docker) or an ssh host with docker.
# The container keeps its Flathub runtimes in the volume pocket-flatpak (first run downloads ~1 GB). Never in CI.
set -eu
here=$(cd "$(dirname "$0")" && pwd); bundle=$(cd "${1:?bundle dir}" && pwd)
# The Flatpak ships the bundle's binaries as they are: no build path in them, whatever made the bundle.
sh "$here/../../scripts/check-binary-paths.sh" "$bundle/pocket-ui" "$bundle/pocket-core" || { echo "build-flatpak: the bundle's binaries carry build paths (rebuild with build-bundle.sh)" >&2; exit 1; }
host=${DECK_BUILD_HOST:-local}; work=pocket-deck-flatpak; out="$here/../../target/pocket-ui-deck.flatpak"
# rsh CMD runs on the build host (cwd = its $HOME); put SRC... -> host:$work/DEST/; get host file -> local file
if [ "$host" = local ]; then
  rsh() { (cd "$HOME" && sh -c "$1"); }
  put() { d=$1; shift; mkdir -p "$HOME/$work/$d"; cp -r "$@" "$HOME/$work/$d/"; }
  get() { cp "$HOME/$1" "$2"; }
else
  rsh() { ssh "$host" "$1"; }
  put() { d=$1; shift; scp -qr "$@" "$host:$work/$d/"; }
  get() { scp -q "$host:$1" "$2"; }
fi
# the container works as root: its files are removed by a container too
rsh "docker run --rm -v \$HOME:/h alpine rm -rf /h/$work; mkdir -p $work/bundle"
put . "$here"/flatpak/Dockerfile "$here"/flatpak/io.github.pocketui.PocketUI.yml
put bundle "$bundle"/.
rsh "cd $work && docker build -q -t pocket-flatpak-env . >/dev/null && docker run --rm --privileged --security-opt seccomp=unconfined --security-opt apparmor=unconfined -v pocket-flatpak:/var/lib/flatpak -v \$PWD:/work -w /work pocket-flatpak-env sh -c '
  flatpak-builder --system --install-deps-from=flathub --force-clean --disable-rofiles-fuse --repo=repo build io.github.pocketui.PocketUI.yml &&
  flatpak build-bundle repo pocket-ui-deck.flatpak io.github.pocketui.PocketUI'"
get "$work/pocket-ui-deck.flatpak" "$out"
ls -la "$out"
