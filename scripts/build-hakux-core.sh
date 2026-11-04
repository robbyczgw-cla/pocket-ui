#!/bin/sh
# Builds the hakuX libretro core from one exact commit of the hakuX-Linux fork (branch linux-1.3.0: portare-ch's Linux/libretro port,
# hakuX PR #9, on hakuX 1.3.0, plus our fixes) with the fork's own tooling, on an aarch64 Docker host (HAKUX_BUILD_HOST;
# an Apple Silicon Mac with Colima does it), and leaves the package where `make-install-zip.sh` / the image build take it from:
#
#   scripts/build-hakux-core.sh [out-dir]       (out-dir default: ./hakux-core)
#
# Steps (docs/XBOX.md "Shipping the core"):
#  1. a clone of the commit, any patches of ours from deploy/hakux/patches/ applied as commits with `git am` (committer = author,
#     committer date = author date, so the patched commit has the same id on every machine; today none: our fixes, among them the
#     APU relaunch-silence fix, are commits of the fork). linux-package.sh needs a clean git checkout: it records HEAD.
#  2. that clone sent to the build host (~/hakux-core-build, replaced each time); the dependencies the fork pins
#     (tools/native/linux-deps.json: FEX, glslang, ... by commit; linux-deps.py fetches and checks them) stay in ~/hakux-core-deps
#     there, mounted where the fork expects them (build/deps), so a rebuild does not fetch them again.
#  3. the fork's tools/native/linux-build.sh and linux-package.sh inside its image (tools/native/Dockerfile.linux: Debian
#     stable-slim pinned by digest, Clang 19), built from that Dockerfile if the host does not have it. The image has no Vulkan
#     loader, so CTest's libretro-smoke fails there (it needs a device); every other test must pass.
# The package hakux-1.3.0-private-aarch64.tar.gz is unpacked into out-dir (lib/libretro/hakux_libretro.so, .info,
# share/licenses/hakux/, BUILD-INFO). Same commit + same patches + same image = the same core, byte for byte (checked for 7068703
# 2026-10-29: two builds and the core shipped since 2026-10-28 have one sha256, docs/XBOX.md). HAKUX_VERSION is part of the binary:
# the core shipped for 035ec48 was built as 1.3.0-private, so that is kept to reproduce its sha256.
set -eu
COMMIT=035ec48acde326b017a3f6cd4bc888e1633172df   # hakuX-Linux, branch linux-1.3.0 (its head has since moved on by a README commit)
VERSION=1.3.0-private
HOST=${HAKUX_BUILD_HOST:?set HAKUX_BUILD_HOST to an ssh host with aarch64 Docker}
OUT=${1:-./hakux-core}
REPO=${HAKUX_REPO:-https://github.com/robbyczgw-cla/hakuX-Linux.git}   # the fork; or a local clone of it that has the commit (git fetch origin linux-1.3.0)
case $REPO in *://*|*@*:*) URL=$REPO ;; *) URL=file://$REPO
  git -C "$REPO" cat-file -e "$COMMIT^{commit}" || { echo "build-hakux-core: $COMMIT is not in $REPO (git fetch origin linux-1.3.0)" >&2; exit 1; } ;; esac
HERE=$(cd "$(dirname "$0")/.." && pwd)
SRC=$(mktemp -d); trap 'rm -rf "$SRC"' EXIT
# Only the commit itself (depth 1): the repository's history is large and the build does not need it.
git init -q "$SRC/hakux"
git -C "$SRC/hakux" fetch -q --depth 1 --upload-pack='git -c uploadpack.allowAnySHA1InWant=true upload-pack' "$URL" "$COMMIT"
git -C "$SRC/hakux" checkout -q --detach "$COMMIT"
for P in "$HERE"/deploy/hakux/patches/*.patch; do   # (none today: our fixes are in the fork)
  [ -e "$P" ] || break   # our header lines before the first "From " line are not part of the mail
  sed -n '/^From [0-9a-f]\{40\} /,$p' "$P" | GIT_COMMITTER_NAME=pocket-ui GIT_COMMITTER_EMAIL=noreply@pocket-ui.invalid \
    git -C "$SRC/hakux" am -q --committer-date-is-author-date || { echo "build-hakux-core: $P does not apply to $COMMIT" >&2; exit 1; }
done
echo "build-hakux-core: source $(git -C "$SRC/hakux" rev-parse HEAD) ($COMMIT + $(ls "$HERE"/deploy/hakux/patches/*.patch 2>/dev/null | wc -l) patch(es))"
# Docker on a Mac is not on the PATH of a non-interactive ssh shell.
R='export PATH=/opt/homebrew/bin:/usr/local/bin:$PATH;'
tar -cf - -C "$SRC" hakux | ssh "$HOST" "rm -rf ~/hakux-core-build && mkdir -p ~/hakux-core-build ~/hakux-core-deps && tar -xf - -C ~/hakux-core-build --strip-components=1"
# (the image keeps its old name hakux-pr9-linux so the Macs that have it do not rebuild it)
ssh "$HOST" "$R cd ~/hakux-core-build && { docker image inspect hakux-pr9-linux >/dev/null 2>&1 || docker build -q -t hakux-pr9-linux -f tools/native/Dockerfile.linux tools/native; }"
# The fork's two scripts, unchanged. linux-build.sh ends with CTest (set -e): its result is judged here, by the failed list.
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
