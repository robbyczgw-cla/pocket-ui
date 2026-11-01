#!/bin/sh
# The H700 images: scripts/make-image-family.sh for that family.
exec sh "$(dirname "$0")/make-image-family.sh" --family H700 "$@"
