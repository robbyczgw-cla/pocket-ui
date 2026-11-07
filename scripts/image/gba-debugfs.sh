#!/bin/sh
# Emit the optional versioned core files for image/build.sh's debugfs batch.
# Source was validated by gba-core-files.sh before the image container starts.
set -eu
SRC=$1 REL=$2 EPOCH=$3
echo "mkdir /$REL/cores"
(cd "$SRC" && find . -mindepth 1 -type d | LC_ALL=C sort) | while IFS= read -r p; do
  echo "mkdir /$REL/cores/${p#./}"
done
(cd "$SRC" && find . -type f | LC_ALL=C sort) | while IFS= read -r p; do
  echo "write \"$SRC/${p#./}\" /$REL/cores/${p#./}"
done
(cd "$SRC" && find . | LC_ALL=C sort) | while IFS= read -r p; do
  dst=/$REL/cores
  [ "$p" = . ] || dst=$dst/${p#./}
  for field in uid gid; do echo "set_inode_field $dst $field 0"; done
  for field in atime mtime ctime crtime; do echo "set_inode_field $dst $field @$EPOCH"; done
done
