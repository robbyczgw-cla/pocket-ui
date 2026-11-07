#!/bin/sh
# pocket-ui/start.sh on the Brick card (docs/TRIMUI-BRICK-REMOTE.md): the one fixed file of the pocket-ui folder. The entries
# (.tmp_update/updater, trimui/app/MainUI) call it; it runs the program folder app/start.sh and keeps the card working across updates:
#   pocket-ui/app/           the program (bin, lib, cores, ...): what an update replaces as a whole
#   pocket-ui/app.previous/  the one before the last update, kept for the way back
#   pocket-ui/app.new/       an update being unpacked; `.ready` in it once it is complete and checked
#   pocket-ui/storage/, logs/, authorized_keys   the player's: settings, Tailscale's login, Remote access; never touched by an update
# 1. A power cut in the middle of a swap (app renamed away, app.new not yet in its place) is finished, or undone, here.
# 2. pocket-ui exits with 75 after it installed an update: app/start.sh ends with 75 and this script starts the new app/.
# 3. While storage/pocket-ui/update-pending names the version in app/, any end of app/start.sh is that version failing to start
#    (pocket-ui confirms itself by removing the file once its first picture is on screen): app.previous goes back in its place.
# Plain sh (BusyBox ash). BRICK_DATA moves the folder for the tests (scripts/test-brick-card.sh).
D=${BRICK_DATA:-$(cd "$(dirname "$0")" && pwd)}
ENTRY=${1:-direct}
PENDING=$D/storage/pocket-ui/update-pending
mkdir -p "$D/logs" 2>/dev/null
say() { echo "$(date '+%H:%M:%S' 2>/dev/null) boot: $*" >>"$D/logs/boot.log" 2>/dev/null; }
release() { cat "$1/RELEASE" 2>/dev/null || echo unknown; }

# A manifest is written into the OLD app before any unchanged asset is renamed.
# Validate every entry before touching either tree. In particular, never follow a
# symlink, overwrite an old file, or silently continue with an asset missing in both.
asset_path() {
  case "$1" in
    cores/*|bios-data/*|bin/brick-egl|bin/curl|bin/dropbearmulti|bin/syncthing|bin/tailscale|bin/tailscaled) ;;
    *) return 1 ;;
  esac
  case "/$1/" in *'\'*|*"$(printf '\r')"*|*'//'*|*'/./'*|*'/../'*) return 1 ;; esac
}
asset_location() { # tree, relative file: absent is OK, links/non-files are not
  tree=$1; rest=$2
  [ ! -L "$tree" ] && { [ ! -e "$tree" ] || [ -d "$tree" ]; } || return 1
  while [ "${rest#*/}" != "$rest" ]; do
    tree=$tree/${rest%%/*}; rest=${rest#*/}
    [ ! -L "$tree" ] && { [ ! -e "$tree" ] || [ -d "$tree" ]; } || return 1
  done
  [ ! -L "$tree/$rest" ] && { [ ! -e "$tree/$rest" ] || [ -f "$tree/$rest" ]; }
}
check_assets() { # old, source; a missing manifest is the legacy copy-based update
  old=$1; source=$2
  [ ! -L "$old" ] && [ -d "$old" ] && [ ! -L "$source" ] &&
    { [ ! -e "$source" ] || [ -d "$source" ]; } || return 1
  [ ! -L "$old/.asset-moves" ] || return 1
  [ -e "$old/.asset-moves" ] || return 0
  [ -f "$old/.asset-moves" ] || return 1
  # ash/dash cannot represent NUL bytes; reject rather than letting read erase them.
  LC_ALL=C tr -d '\000' <"$old/.asset-moves" | cmp -s "$old/.asset-moves" - || return 1
  while :; do
    asset=
    IFS= read -r asset
    read_rc=$?
    if [ "$read_rc" != 0 ]; then [ -z "$asset" ] && break; return 1; fi
    asset_path "$asset" && asset_location "$old" "$asset" && asset_location "$source" "$asset" || return 1
    [ -f "$old/$asset" ] || [ -f "$source/$asset" ] || return 1
  done <"$old/.asset-moves"
}
restore_assets() { # first check the WHOLE manifest, then restore only absent old files
  check_assets "$1" "$2" || return 1
  old=$1; source=$2
  [ -f "$old/.asset-moves" ] || return 0
  while IFS= read -r asset; do
    if [ ! -f "$old/$asset" ]; then
      mkdir -p "${old}/${asset%/*}" && mv "$source/$asset" "$old/$asset" && sync || return 1
    fi
  done <"$old/.asset-moves"
}
blocked() { say "asset recovery blocked: $*; kept program folders for recovery"; exit 1; }
finish_rollback() {
  [ -f "$D/app/.restore-assets" ] && [ ! -L "$D/app/.restore-assets" ] || return 1
  restore_assets "$D/app" "$D/app.failed" || return 1
  mkdir -p "$D/storage/pocket-ui" && cat "$D/app/.restore-assets" >"$D/storage/pocket-ui/rolled-back.tmp" && sync &&
    mv "$D/storage/pocket-ui/rolled-back.tmp" "$D/storage/pocket-ui/rolled-back" &&
    rm -f "$PENDING" "$D/app/.asset-moves" && sync && rm -f "$D/app/.restore-assets" && sync
}
rollback() {
  # The phase marker stays in the old tree until BOTH renames and the health
  # markers are durable. A reboot can resume before/after either directory rename.
  [ ! -L "$D/app.previous" ] && [ -d "$D/app.previous" ] || return 1
  if [ ! -e "$D/app.previous/.restore-assets" ]; then
    check_assets "$D/app.previous" "$D/app" || return 1
    release "$D/app" >"$D/app.previous/.restore-assets.tmp" && sync &&
      mv "$D/app.previous/.restore-assets.tmp" "$D/app.previous/.restore-assets" && sync || return 1
  fi
  [ -f "$D/app.previous/.restore-assets" ] && [ ! -L "$D/app.previous/.restore-assets" ] || return 1
  if [ -d "$D/app" ] && [ ! -L "$D/app" ]; then source=$D/app; else source=$D/app.failed; fi
  restore_assets "$D/app.previous" "$source" || return 1
  if [ -e "$D/app" ] || [ -L "$D/app" ]; then
    [ -d "$D/app" ] && [ ! -L "$D/app" ] || return 1
    rm -rf "$D/app.failed" && mv "$D/app" "$D/app.failed" && sync || return 1
  fi
  mv "$D/app.previous" "$D/app" && sync && finish_rollback
}

# Resume rollback before considering a staged update: a ready app.new left over
# from an older transaction must never win over a rollback already in progress.
if [ -e "$D/app/.restore-assets" ] || [ -L "$D/app/.restore-assets" ]; then
  [ ! -e "$D/app.previous" ] && finish_rollback || blocked "finishing rollback"
elif [ -e "$D/app.previous/.restore-assets" ] || [ -L "$D/app.previous/.restore-assets" ]; then
  rollback || blocked "resuming rollback"
fi
if [ -e "$D/app/.asset-moves" ] || [ -L "$D/app/.asset-moves" ]; then
  check_assets "$D/app" "$D/app.new" || blocked "invalid interrupted asset manifest"
  rm -f "$D/app.new/.ready" && sync && restore_assets "$D/app" "$D/app.new" &&
    rm -f "$D/app/.asset-moves" && sync || blocked "restoring interrupted asset moves"
fi
if [ ! -f "$D/app/start.sh" ]; then
  [ ! -e "$D/app" ] && [ ! -L "$D/app" ] || blocked "existing program folder has no start script"
  if [ -f "$D/app.new/.ready" ] && [ -f "$D/app.new/start.sh" ]; then
    if [ -e "$D/app.previous/.asset-moves" ] || [ -L "$D/app.previous/.asset-moves" ]; then
      check_assets "$D/app.previous" "$D/app.new" || blocked "invalid activation asset manifest"
    fi
    [ ! -L "$D/app.new" ] && mv "$D/app.new" "$D/app" && sync || blocked "finishing update swap"
    say "finished the update to $(release "$D/app") that a power cut interrupted"
  elif [ -f "$D/app.previous/start.sh" ]; then
    restore_assets "$D/app.previous" "$D/app.new" && rm -f "$D/app.previous/.asset-moves" && sync &&
      mv "$D/app.previous" "$D/app" && sync && rm -f "$PENDING" && sync || blocked "restoring previous program"
    say "no program folder: took back $(release "$D/app")"
  fi
fi
if [ -f "$PENDING" ] && [ "$(cat "$PENDING" 2>/dev/null)" != "$(release "$D/app")" ]; then
  say "update-pending names $(cat "$PENDING" 2>/dev/null), not $(release "$D/app"): dropped"
  rm -f "$PENDING"
fi

sh "$D/app/start.sh" "$ENTRY"
rc=$?
if [ "$rc" = 75 ]; then
  say "restart into $(release "$D/app") after an update"
  exec sh "$D/start.sh" "$ENTRY"
fi
if [ -f "$PENDING" ] && [ -f "$D/app.previous/start.sh" ]; then
  bad=$(release "$D/app")
  say "$bad did not start (exit $rc): back to $(release "$D/app.previous")"
  if rollback; then
    exec sh "$D/start.sh" "$ENTRY"
  fi
  blocked "the way back failed"
fi
exit "$rc"
