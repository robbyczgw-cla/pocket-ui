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
# a card that also starts through UEFI and OpenABL's BLS entry (pins.sh SM8550_EFI_MODELS), else 0 (the default); "#if GENERIC"
# with $3: 1 for an RK3566 card on ROCKNIX's Generic U-Boot (pins.sh is_generic), else 0 (the default); "#if DEVTEST" with $4: 1 for
# an RK3326 device-test card (pins.sh is_devtest), else 0. No nesting.
notes_blocks() {
  awk -v g="$1" -v e="${2:-0}" -v n="${3:-0}" -v d="${4:-0}" 'BEGIN { s = 1 } /^#if (GAMES|EFI|GENERIC|DEVTEST)$/ { v = ($2 == "GAMES") ? g : ($2 == "EFI") ? e : ($2 == "GENERIC") ? n : d; s = (v == 1); next }
    /^#else$/ { s = (v != 1); next } /^#endif$/ { s = 1; next } s'
}

# A prominent test warning in the card notes. Incidental mentions of tests or
# experimental settings elsewhere are not a warning about the whole card.
image_test_warning() {
  LC_ALL=C grep -Eq '^[[:space:]]*Experimental(:|[[:space:]]*$)|^[[:space:]]*([Tt]his is a |[Ii]t is a |[Aa] )?[Tt]est image([[:space:][:punct:]]|$)' "$@"
}
