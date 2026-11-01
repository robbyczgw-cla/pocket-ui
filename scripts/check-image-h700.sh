#!/bin/sh
# The H700 images' check: scripts/check-image-family.sh, which finds the family from the image's name.
exec sh "$(dirname "$0")/check-image-family.sh" "$@"
