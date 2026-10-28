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
  basename "$1" | sed -n 's/^pocket-ui-\(.*\)-rp-mini-v2\.img\(\.xz\)\{0,1\}$/\1/p'
}
