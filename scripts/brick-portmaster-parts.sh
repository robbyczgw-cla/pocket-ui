#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Build on the Air: pinned GNU Bash plus our chroot launcher, glibc <= stock's 2.33.
# Usage: scripts/brick-portmaster-parts.sh OUT [WORK]; pass OUT as BRICK_PORTMASTER_PARTS to the card builder.
set -eu
out=${1:?output}; work=${2:-$out/work}
here=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
mkdir -p "$out/bin" "$out/licences/bash" "$out/sources" "$work"
out=$(CDPATH= cd -- "$out" && pwd); work=$(CDPATH= cd -- "$work" && pwd)
archive=$work/bash-5.2.37.tar.gz
[ -f "$archive" ] || curl -fL --max-time 120 -o "$archive" https://ftp.gnu.org/gnu/bash/bash-5.2.37.tar.gz
actual=$(shasum -a 256 "$archive" | cut -d' ' -f1)
[ "$actual" = 9599b22ecd1d5787ad7d3b7bf0c59f312b3396d1e281175dd1f8a4014da621ff ] || { echo 'Bash source checksum mismatch' >&2; exit 1; }
[ -d "$work/bash-5.2.37" ] || tar -xzf "$archive" -C "$work"
zig=${ZIG:-zig}
"$zig" cc -target aarch64-linux-gnu.2.33 -O2 -s -ffile-prefix-map="$here"=/pocket-ui "$here/deploy/brick/portmaster-exec.c" -o "$out/bin/portmaster-exec"
cd "$work/bash-5.2.37"
if [ ! -f Makefile ]; then
  CC="$zig cc -target aarch64-linux-gnu.2.33" AR="$zig ar" RANLIB="$zig ranlib" CFLAGS="-Os -ffile-prefix-map=$work=/portmaster-build" \
    LDFLAGS=-s bash_cv_dev_fd=standard bash_cv_job_control_missing=present bash_cv_sys_named_pipes=present \
    ./configure --host=aarch64-linux-gnu --build="$(./support/config.guess)" --without-bash-malloc --disable-nls --disable-readline
fi
make -j 2 AR="$zig ar" RANLIB="$zig ranlib" bash
cp bash "$out/bin/bash"
cp COPYING "$out/licences/bash/LICENSE"
cp "$archive" "$out/sources/"
printf '%s\n' 'GNU Bash 5.2.37, GPL-3.0-or-later, unchanged source in sources/bash-5.2.37.tar.gz.' \
 'https://ftp.gnu.org/gnu/bash/bash-5.2.37.tar.gz' >"$out/PORTMASTER-PARTS.txt"
