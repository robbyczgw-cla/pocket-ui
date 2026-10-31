#!/bin/sh
# Builds the Steam Deck bundle on an x86_64 Linux machine: target/deck-bundle/ and pocket-ui-deck-<version>.tar.gz next to it.
# Everything runs on our machines; there is no CI. Cores: CORES_DIR=<dir with *_libretro.so> copies them into cores/
# (licences: docs/STEAMDECK.md, "Cores").
set -eu
cd "$(dirname "$0")/../.."
export PATH=$HOME/.cargo/bin:$PATH
# Build paths out of the binaries (the home folder of whoever builds): scripts/release-rustflags.sh, checked below.
REMAP_ROOT=$(pwd -P) . scripts/release-rustflags.sh
if [ "$(uname -s)-$(uname -m)" = Linux-x86_64 ]; then
  nice cargo build --release --features x11 --bin pocket-ui --bin pocket-core
  bin=target/release
else # a Mac: cross-build (rustup target add x86_64-unknown-linux-gnu; cargo-zigbuild + zig)
  nice cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.38 --features x11 --bin pocket-ui --bin pocket-core
  bin=target/x86_64-unknown-linux-gnu/release
fi
sh scripts/check-binary-paths.sh "$bin/pocket-ui" "$bin/pocket-core" || { echo "build-bundle: the binaries carry build paths" >&2; exit 1; }
b=target/deck-bundle
rm -rf "$b"; mkdir -p "$b"
cp "$bin/pocket-ui" "$bin/pocket-core" "$b/"
cp deploy/steamdeck/pocket-ui-deck.sh deploy/steamdeck/runemu.sh deploy/steamdeck/gen-es-systems.py deploy/steamdeck/add-to-steam.py deploy/steamdeck/fetch-cores.sh deploy/steamdeck/install.sh "$b/"
if [ -n "${CORES_DIR:-}" ]; then mkdir -p "$b/cores"; cp "$CORES_DIR"/*_libretro.so "$b/cores/"; fi
v=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tar -C target -czf "target/pocket-ui-deck-$v.tar.gz" deck-bundle
ls -la "target/pocket-ui-deck-$v.tar.gz"
