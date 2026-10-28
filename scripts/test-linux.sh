#!/bin/sh
# Runs the library's unit tests on Linux (aarch64, in Docker). The tests that need /proc, prctl or real
# process groups (src/control.rs) only exist there; `cargo test` on macOS leaves them out.
#   scripts/test-linux.sh [test binary arguments, e.g. a name filter]
# The default image is debian:trixie-slim plus gcc (the resume tests build a stub core) and curl (download
# tests), built once and cached as pocket-ui-test:trixie (POCKETUI_TEST_IMAGE for another).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
bin=$(cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --lib --tests --message-format=json |
  sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)
[ -n "$bin" ] || { echo "no test binary built" >&2; exit 1; }
image=${POCKETUI_TEST_IMAGE:-pocket-ui-test:trixie}
if [ "$image" = pocket-ui-test:trixie ] && ! docker image inspect "$image" >/dev/null 2>&1; then
  printf 'FROM debian:trixie-slim\nRUN apt-get update && apt-get install -y --no-install-recommends gcc libc6-dev curl ca-certificates && rm -rf /var/lib/apt/lists/*\n' |
    docker build -q -t "$image" - >/dev/null
fi
# The repository at the same path, for tests that read files next to the sources; writable, as with
# `cargo test` on the Mac (fsutil writes a file without a directory into the working directory).
docker run --rm --init -v "$ROOT:$ROOT" -w "$ROOT" "$image" "$bin" "$@"
