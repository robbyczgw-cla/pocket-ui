#!/bin/sh
# Builds the hakuX libretro core from PR #9's exact commit with the PR's own tooling, on an aarch64 Docker host (HAKUX_BUILD_HOST;
# an Apple Silicon Mac with Colima does it), and leaves the package where `make-install-zip.sh` / the image build take it from:
#
#   scripts/build-hakux-core.sh [out-dir]       (out-dir default: ./hakux-core)
#
# Steps (docs/XBOX.md "Shipping the core"):
#  1. a clone of the commit, any patches of ours from deploy/hakux/patches/ applied as commits with `git am` (committer = author,
#     committer date = author date, so the patched commit has the same id on every machine; today none: the APU
#     relaunch-silence fix we submitted is in the PR since 77a5871). linux-package.sh needs a clean git checkout: it records HEAD.
#  2. that clone sent to the build host (~/hakux-core-build, replaced each time); the dependencies the PR pins
#     (tools/native/linux-deps.json: FEX, glslang, ... by commit; linux-deps.py fetches and checks them) stay in ~/hakux-core-deps
#     there, mounted where the PR expects them (build/deps), so a rebuild does not fetch them again.
#  3. the PR's tools/native/linux-build.sh and linux-package.sh inside the PR's image (tools/native/Dockerfile.linux: Debian
#     stable-slim pinned by digest, Clang 19), built from that Dockerfile if the host does not have it. The image has no Vulkan
#     loader, so CTest's libretro-smoke fails there (it needs a device); every other test must pass.
# The package hakux-pr9-2de4e9c-aarch64.tar.gz is unpacked into out-dir (lib/libretro/hakux_libretro.so, .info,
# share/licenses/hakux/, BUILD-INFO). Same commit + same patches + same image = the same core, byte for byte (checked for 7068703
# 2026-10-29: two builds and the core shipped since 2026-10-28 have one sha256, docs/XBOX.md).
set -eu
COMMIT=2de4e9cd86a47e9ffaf928c36e240a995a279713
VERSION=pr9-2de4e9c
HOST=${HAKUX_BUILD_HOST:?set HAKUX_BUILD_HOST to an ssh host with aarch64 Docker}
OUT=${1:-./hakux-core}
REPO=${HAKUX_REPO:-$HOME/pocket-review/hakux-spike}   # a clone of https://github.com/rfandango/hakuX.git with refs/pull/9/head fetched
git -C "$REPO" cat-file -e "$COMMIT^{commit}" || { echo "build-hakux-core: $COMMIT is not in $REPO (git fetch origin pull/9/head)" >&2; exit 1; }
HERE=$(cd "$(dirname "$0")/.." && pwd)
SRC=$(mktemp -d); trap 'rm -rf "$SRC"' EXIT
# Only the commit itself (depth 1): the repository's history is large and the build does not need it.
git init -q "$SRC/hakux"
git -C "$SRC/hakux" fetch -q --depth 1 --upload-pack='git -c uploadpack.allowAnySHA1InWant=true upload-pack' "file://$REPO" "$COMMIT"
git -C "$SRC/hakux" checkout -q --detach "$COMMIT"
for P in "$HERE"/deploy/hakux/patches/*.patch; do   # (none today: the audio fix is in the PR)
  [ -e "$P" ] || break   # our header lines before the first "From " line are not part of the mail
  sed -n '/^From [0-9a-f]\{40\} /,$p' "$P" | GIT_COMMITTER_NAME=pocket-ui GIT_COMMITTER_EMAIL=noreply@pocket-ui.invalid \
    git -C "$SRC/hakux" am -q --committer-date-is-author-date || { echo "build-hakux-core: $P does not apply to $COMMIT" >&2; exit 1; }
done
echo "build-hakux-core: source $(git -C "$SRC/hakux" rev-parse HEAD) ($COMMIT + $(ls "$HERE"/deploy/hakux/patches/*.patch 2>/dev/null | wc -l) patch(es))"
# Docker on a Mac is not on the PATH of a non-interactive ssh shell.
R='export PATH=/opt/homebrew/bin:/usr/local/bin:$PATH;'
tar -cf - -C "$SRC" hakux | ssh "$HOST" "rm -rf ~/hakux-core-build && mkdir -p ~/hakux-core-build ~/hakux-core-deps && tar -xf - -C ~/hakux-core-build --strip-components=1"
ssh "$HOST" "$R cd ~/hakux-core-build && { docker image inspect hakux-pr9-linux >/dev/null 2>&1 || docker build -q -t hakux-pr9-linux -f tools/native/Dockerfile.linux tools/native; }"
# The PR's two scripts, unchanged. linux-build.sh ends with CTest (set -e): its result is judged here, by the failed list.
ssh "$HOST" "$R docker run --rm -v ~/hakux-core-build:/src -v ~/hakux-core-deps:/src/build/deps -e JOBS=\${JOBS:-2} \
  -e HAKUX_VERSION=$VERSION hakux-pr9-linux sh -c 'git config --global --add safe.directory /src; \
  python3 tools/native/linux-deps.py --dest build/deps && sh tools/native/linux-build.sh; f=build/linux/Testing/Temporary/LastTestsFailed.log; \
  [ -s build/linux/hakux_libretro.so ] || exit 1; \
  if [ -s \$f ] && grep -v libretro-smoke \$f | grep -q .; then echo \"build-hakux-core: tests failed:\"; cat \$f; exit 1; fi; \
  STRIP=strip sh tools/native/linux-package.sh $VERSION'"
mkdir -p "$OUT"
ssh "$HOST" "cat ~/hakux-core-build/build/package/hakux-$VERSION-aarch64.tar.gz" >"$OUT/hakux-$VERSION-aarch64.tar.gz"
tar -xzf "$OUT/hakux-$VERSION-aarch64.tar.gz" -C "$OUT" --strip-components=1
echo "build-hakux-core: $(sha256sum "$OUT/lib/libretro/hakux_libretro.so")"
