# Shared by scripts/make-image.sh and scripts/check-image.sh. Sourced, not run.

# Builds the tools container (scripts/image/Dockerfile) once per version of that file; prints its tag.
tools_image() {
  dockerfile=$ROOT/scripts/image/Dockerfile
  tag=pocket-image-tools:$(shasum -a 256 "$dockerfile" | cut -c1-12)
  if ! docker image inspect "$tag" >/dev/null 2>&1; then
    echo "building $tag" >&2
    docker build -q -t "$tag" -f "$dockerfile" "$ROOT/scripts/image" >/dev/null
  fi
  echo "$tag"
}

# The version in a file name like pocket-ui-0.2.0-rp-mini-v2.img.xz.
version_of() {
  basename "$1" | sed -En 's/^pocket-ui-(.*)-(rp-mini-v2|rp5|flip2)\.img(\.xz)?$/\1/p'
}

# The notes for one model, on stdin: keeps what's between "#if GAMES" and "#else" (GAMES = 1) or "#else" and "#endif"
# (GAMES = 0), and drops those lines; "#if EFI" blocks the same way. $1: 1 for a card with the GAMES drive, else 0; $2: 1 for
# a card that also starts through UEFI and OpenABL's BLS entry (pins.sh SM8550_EFI_MODELS), else 0 (the default). No nesting.
notes_blocks() {
  awk -v g="$1" -v e="${2:-0}" 'BEGIN { s = 1 } /^#if (GAMES|EFI)$/ { v = ($2 == "GAMES") ? g : e; s = (v == 1); next }
    /^#else$/ { s = (v != 1); next } /^#endif$/ { s = 1; next } s'
}
