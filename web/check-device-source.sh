#!/bin/sh
# What the device builds compile is unchanged by the web demo (docs/WEB-DEMO.md, "What the device builds see"): the
# library, pocket-ui and pocket-core, expanded for Linux (cfg and macros, -Zunpretty=expanded), default and
# --features brick, from <ref> and from this working tree, built from the same folder; compared without comments and
# white space. Exit 0 when all six are the same.
#
#   sh web/check-device-source.sh <ref> [scratch dir]     (cargo of the device toolchain on PATH; a check, no codegen)
set -eu
ref=${1:?usage: check-device-source.sh <ref> [scratch dir]}
repo=$(cd "$(dirname "$0")/.." && pwd)
work=${2:-$(mktemp -d)}
mkdir -p "$work"
expand() {
  rm -rf "$work/tree" "$work/$1" && mkdir -p "$work/$1"
  if [ "$1" = ref ]; then git -C "$repo" worktree add --quiet --detach "$work/tree" "$ref"
  else mkdir -p "$work/tree" && (cd "$repo" && git ls-files -co --exclude-standard -z | xargs -0 tar c --no-recursion) | tar x -C "$work/tree"; fi
  for feat in default brick; do
    f=; [ $feat = brick ] && f="--features brick"
    for what in --lib "--bin pocket-ui" "--bin pocket-core"; do
      (cd "$work/tree" && CARGO_TARGET_DIR="$work/target" RUSTC_BOOTSTRAP=1 \
        cargo rustc --locked --profile gate $f $what -- -Zunpretty=expanded 2>/dev/null) \
        | sed -E 's#^[[:space:]]*//.*##; s/(POCKETUI_BUILD_UNIX[^0-9]*)[0-9]+/\1/; s/"[0-9]{10}"/""/g' | tr -d ' \t\n' \
        > "$work/$1/$feat-$(echo "$what" | tr -d ' -')"
    done
  done
}
expand ref
git -C "$repo" worktree remove --force "$work/tree"
expand branch
bad=0
for f in "$work"/ref/*; do
  n=$(basename "$f")
  if [ -s "$f" ] && cmp -s "$f" "$work/branch/$n"; then echo "same: $n"; else echo "DIFFERENT (or empty): $n"; bad=1; fi
done
exit $bad
