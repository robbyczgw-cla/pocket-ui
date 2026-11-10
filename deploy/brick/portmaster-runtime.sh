#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Persistent, optional PortMaster runtime. Never runs the upstream firmware updater.
set -eu
BRICK_APP=${BRICK_APP:-$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)}
BRICK_CARD=${BRICK_CARD:-/mnt/SDCARD}
DATA=${BRICK_PORTMASTER_DATA:-$BRICK_CARD/pocket-ui/portmaster}
ASSETS=$BRICK_APP/portmaster
MANIFEST=${BRICK_PORTMASTER_MANIFEST:-$ASSETS/portmaster-manifest.txt}
export BRICK_APP BRICK_CARD
export BRICK_PORTMASTER_HOME=$DATA/current/PortMaster
export BRICK_PORTMASTER_RUNTIME=$DATA/current/runtime
die() { echo "PortMaster: $*" >&2; exit 1; }
progress() {
  [ -n "${BRICK_PORTMASTER_PROGRESS:-}" ] || return 0
  printf '%s|%s|%s\n' "$1" "${2:-0}" "${3:-0}" >"$BRICK_PORTMASTER_PROGRESS.new"
  mv "$BRICK_PORTMASTER_PROGRESS.new" "$BRICK_PORTMASTER_PROGRESS"
}
while [ "$DATA" != / ] && [ "${DATA%/}" != "$DATA" ]; do DATA=${DATA%/}; done
case "$DATA/" in /|*//*|/mnt/|/mnt/SDCARD/|*/../*|*/./*) die 'invalid data folder';; /*) :;; *) die 'invalid data folder';; esac
needed_kib=${BRICK_PORTMASTER_MIN_KB:-716800}
available_space() {
  free_kb=$("${BRICK_PORTMASTER_DF:-df}" -Pk "$BRICK_CARD" | awk 'END {print $4}')
  case "$free_kb" in ''|*[!0-9]*) die 'cannot check free card space';; esac
}
ready() { [ -f "$DATA/current/.ready" ] && [ -s "$BRICK_PORTMASTER_HOME/harbourmaster" ] && [ -s "$BRICK_PORTMASTER_RUNTIME/bin/python3" ] && cmp -s "$MANIFEST" "$DATA/current/INSTALLED.txt"; }
env_runtime() {
  export PYTHONHOME=$BRICK_PORTMASTER_RUNTIME
  export LD_LIBRARY_PATH=$BRICK_PORTMASTER_RUNTIME/lib:$BRICK_APP/lib:${BRICK_FIRMWARE:-/usr/trimui}/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
  export PATH=$BRICK_PORTMASTER_RUNTIME/bin:$BRICK_APP/bin:$PATH
  export HOME=$BRICK_CARD/pocket-ui/storage/home
  export XDG_CONFIG_HOME=$HOME/.config XDG_DATA_HOME=$HOME/.local/share XDG_STATE_HOME=$HOME/.local/state
  mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME"
}
acquire_lock() {
  [ ! -L "$DATA" ] || die 'runtime data folder is a symbolic link'
  mkdir -p "$DATA"
  if ! mkdir "$DATA/.lock" 2>/dev/null; then
    owner=$(cat "$DATA/.lock/pid" 2>/dev/null || echo '')
    case "$owner" in ''|*[!0-9]*) die 'another installation holds the runtime lock';; esac
    if kill -0 "$owner" 2>/dev/null; then die 'another installation is running'; fi
    rm -f "$DATA/.lock/pid"; rmdir "$DATA/.lock" || die 'runtime lock is not empty'
    mkdir "$DATA/.lock" || die 'runtime lock is busy'
  fi
  echo $$ >"$DATA/.lock/pid"
}
release_lock() { rm -f "$DATA/.lock/pid"; rmdir "$DATA/.lock" 2>/dev/null || :; }
# Move on the same card, including hidden entries, without duplicating large engine
# runtimes. A retry can merge a partially completed preservation step safely.
preserve_move() {
  if [ ! -e "$2" ] && [ ! -L "$2" ]; then mv "$1" "$2"; return; fi
  if [ -d "$1" ] && [ ! -L "$1" ] && [ -d "$2" ] && [ ! -L "$2" ]; then
    for entry in "$1"/* "$1"/.[!.]* "$1"/..?*; do
      [ -e "$entry" ] || [ -L "$entry" ] || continue
      preserve_move "$entry" "$2/${entry##*/}" || return 1
    done
    rmdir "$1"
  else
    [ ! -d "$1" ] && [ ! -d "$2" ] || return 1
    mv -f "$1" "$2"
  fi
}
remove_runtime() {
  acquire_lock
  trap release_lock EXIT
  trap 'exit 1' HUP INT TERM
  for folder in current current/PortMaster previous previous/PortMaster preserved; do [ ! -L "$DATA/$folder" ] || die 'runtime data folder is a symbolic link'; done
  mkdir -p "$DATA/preserved"
  for folder in config libs; do
    source=$DATA/current/PortMaster/$folder
    if [ ! -d "$source" ]; then
      # A cancelled removal may already have moved the latest folder here.
      # Never replace that copy with an older rollback generation.
      [ ! -e "$DATA/preserved/$folder" ] && [ ! -L "$DATA/preserved/$folder" ] || continue
      source=$DATA/previous/PortMaster/$folder
    fi
    if [ -d "$source" ]; then
      [ ! -L "$source" ] || die 'player data folder is a symbolic link'
      preserve_move "$source" "$DATA/preserved/$folder" || die 'could not preserve player data; nothing else was removed'
    fi
  done
  sync
  for folder in .stage cache current previous; do rm -rf "$DATA/$folder"; done
  sync
  progress removed
}
install_runtime() {
  acquire_lock
  trap 'rm -rf "$DATA/.stage"; release_lock' EXIT
  download_pid=
  cancel_install() {
    if [ -n "$download_pid" ]; then kill -TERM "$download_pid" 2>/dev/null || :; wait "$download_pid" || :; fi
    progress cancelled
    exit 1
  }
  trap cancel_install HUP INT TERM
  # Waiting on an asynchronous child lets the shell handle TERM immediately.
  # Keep every long-running writer tracked until wait has reaped it, so the
  # cancellation trap cannot remove staging while an extractor still uses it.
  install_child() {
    "$@" &
    download_pid=$!
    if wait "$download_pid"; then
      download_pid=
    else
      child_status=$?
      download_pid=
      return "$child_status"
    fi
  }
  # Recover the rename window after an interrupted activation before doing network work.
  if [ ! -d "$DATA/current" ] && [ -f "$DATA/previous/.ready" ]; then mv "$DATA/previous" "$DATA/current"; fi
  if [ "$1" = ensure ] && ready; then
    [ ! -f "$ASSETS/control.txt" ] || cp "$ASSETS/control.txt" "$BRICK_PORTMASTER_HOME/control.txt"
    progress ready
    return
  fi
  available_space
  if [ "$free_kb" -lt "$needed_kib" ]; then
    printf 'PORTMASTER_SPACE|%s|%s\n' "$needed_kib" "$free_kb" >&2
    die 'at least 700 MiB free card space is needed for installation'
  fi
  rm -rf "$DATA/.stage"
  mkdir -p "$DATA/.stage/runtime" "$DATA/cache"
  stage=$DATA/.stage
  while IFS='|' read -r kind checksum size url; do
    case "$kind" in ''|'#'*) continue;; esac
    case "$checksum" in *[!0-9a-f]*|'') die 'invalid pinned checksum';; esac
    [ ${#checksum} = 64 ] || die 'invalid pinned checksum'
    case "$size" in ''|*[!0-9]*) die 'invalid pinned size';; esac
    [ "$size" -gt 0 ] || die 'invalid pinned size'
    file=$DATA/cache/$checksum
    progress downloading 0 "$size"
    if ! { [ -f "$file" ] && echo "$checksum  $file" | sha256sum -c - >/dev/null 2>&1; }; then
      install_child "$BRICK_APP/bin/pocket-ui" --download-asset "$url" "$checksum" "$size" "$file" "${BRICK_PORTMASTER_PROGRESS:-}" || die 'download stopped; retry continues the partial download'
    fi
    echo "$checksum  $file" | sha256sum -c - >/dev/null || die 'download checksum failed'
    progress unpacking
    case "$kind" in
      python)
        install_child "$BRICK_APP/bin/pocket-ui" --unpack-runtime "$file" "$stage/runtime"
        # FAT/exFAT permissions come from mount options; chmod may return EPERM
        # even when that mount already permits execution. The real Python run
        # below verifies execution instead of trusting Unix permission metadata.
        chmod +x "$stage/runtime/bin/python3" 2>/dev/null || :
        # pip, headers and GUI tools aren't used by the store, keep the optional download small on disk.
        rm -rf "$stage/runtime/include" "$stage/runtime/share" "$stage/runtime/lib/python3.11/site-packages"
        ;;
      portmaster) pm_archive=$file; pm_revision=${url##*/};;
      lib/*) name=${kind#lib/}; case "$name" in */*|*..*) die 'invalid library name';; esac; cp "$file" "$stage/runtime/lib/$name";;
      *) die 'unknown runtime component';;
    esac
    # Finish unzip/copy writeback before starting the next Wi-Fi transfer.
    install_child sync
  done <"$MANIFEST"
  [ -n "${pm_archive:-}" ] || die 'PortMaster archive missing'
  runtime_python() {
    if install_child env PYTHONHOME="$stage/runtime" LD_LIBRARY_PATH="$stage/runtime/lib:${BRICK_FIRMWARE:-/usr/trimui}/lib" "$stage/runtime/bin/python3" "$@"; then
      return 0
    else
      python_status=$?
      [ "$python_status" != 126 ] || die 'The SD card does not allow executable files; check its mount options (noexec).'
      return "$python_status"
    fi
  }
  runtime_python "$ASSETS/portmaster-extract.py" "$pm_archive" "$stage"
  # Generated release metadata is absent from a source archive. Prepare the
  # honest Git version and Unicode fonts without firmware tar/XZ dependencies.
  # runtime_python tracks this writer too, so cancellation waits before cleanup.
  runtime_python "$ASSETS/portmaster-extract.py" --prepare "$stage" "$pm_revision"
  runtime_python -c 'import ssl, ctypes, zipfile, lzma, bz2' || die 'Python runtime dependencies are missing'
  [ -s "$stage/PortMaster/harbourmaster" ] && [ -s "$stage/PortMaster/gptokeyb" ] || die 'incomplete PortMaster archive'
  # Player configuration and engine runtimes survive both app OTA and PM updates.
  for folder in config libs; do
    if [ -d "$DATA/preserved/$folder" ]; then mkdir -p "$stage/PortMaster/$folder"; install_child cp -R "$DATA/preserved/$folder/." "$stage/PortMaster/$folder/"; fi
    if [ -d "$DATA/current/PortMaster/$folder" ]; then mkdir -p "$stage/PortMaster/$folder"; install_child cp -R "$DATA/current/PortMaster/$folder/." "$stage/PortMaster/$folder/"; fi
  done
  [ ! -f "$ASSETS/control.txt" ] || cp "$ASSETS/control.txt" "$stage/PortMaster/control.txt"
  chmod +x "$stage/PortMaster/gptokeyb" "$stage/PortMaster/gptokeyb2" 2>/dev/null || :
  cp "$MANIFEST" "$stage/INSTALLED.txt"
  echo 'pocket-ui PortMaster runtime v1' >"$stage/.ready"
  install_child sync
  progress activating
  rm -rf "$DATA/previous"
  if [ -d "$DATA/current" ]; then mv "$DATA/current" "$DATA/previous"; fi
  if ! mv "$stage" "$DATA/current"; then
    [ ! -d "$DATA/previous" ] || mv "$DATA/previous" "$DATA/current"
    die 'activation failed; previous runtime restored'
  fi
  install_child sync
  rm -rf "$DATA/cache"
  # The newly active generation now owns these exact copies. Failed or cancelled
  # installs leave preserved data intact until this durable activation boundary.
  for folder in config libs; do rm -rf "$DATA/preserved/$folder"; done
  [ ! -d "$DATA/preserved" ] || rmdir "$DATA/preserved" 2>/dev/null || :
  progress ready
}
command=${1:-ensure}; [ $# = 0 ] || shift
case "$command" in
  space) available_space; printf 'needed_kib=%s available_kib=%s\n' "$needed_kib" "$free_kb";;
  remove-runtime) remove_runtime;;
  status)
    if ready; then echo ready; elif [ -d "$DATA/current" ]; then echo outdated; exit 1; else echo missing; exit 1; fi;;
  ensure|update) install_runtime "$command";;
  harbourmaster)
    sh "$BRICK_APP/bin/portmaster" ensure
    env_runtime
    [ ! -f "$ASSETS/device-env.sh" ] || . "$ASSETS/device-env.sh"
    exec "$BRICK_PORTMASTER_RUNTIME/bin/python3" "$ASSETS/portmaster-bridge.py" "$@";;
  run)
    ready || die 'Get PortMaster from the Ports page first'
    env_runtime
    exec sh "$BRICK_APP/bin/portmaster-run" "$@";;
  *) die 'usage: portmaster status|space|ensure|update|remove-runtime|harbourmaster ARGS|run PORT';;
esac
