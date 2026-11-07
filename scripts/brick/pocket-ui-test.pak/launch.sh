#!/bin/sh
# The kit's pak (Tools/tg5040/pocket-ui.pak while we test): written for this project, nothing here comes from NextUI or MinUI. The
# owner presses A on it in NextUI's Tools list; NextUI's own loop starts it and takes the screen back when it ends. Remove it by
# deleting this folder.
# One-shot: if the kit left a stage name in next-stage, that stage runs (and the file is removed first, so it never runs twice);
# without the file the real pocket-ui starts and runs until the player leaves it (SELECT + START held: "Back to NextUI"). A stage ends
# at its limit; a battery under 20 % stops both before they draw.
KIT=${POCKET_UI_KIT:-/mnt/SDCARD/.pocket-ui-test}
NEXT=$KIT/next-stage
stage=
if [ -s "$NEXT" ]; then
  stage=$(head -n 1 "$NEXT")
  rm -f "$NEXT"
  case "$stage" in *[!a-z]*) stage= ;; esac
fi
export POCKET_MIN_BATTERY=20
if [ -n "$stage" ]; then
  exec sh "$KIT/brick-kit.sh" _run "$stage"
fi
exec sh "$KIT/start.sh" nextui
