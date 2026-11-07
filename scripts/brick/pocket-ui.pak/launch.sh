#!/bin/sh
# The pak: a thin entry into our own folder, which holds everything (start.sh). To remove pocket-ui, delete this folder
# (Tools/tg5040/pocket-ui.pak) and the folder it points to. The standalone card's updater is the same kind of entry.
ROOT=${POCKET_UI_ROOT:-/mnt/SDCARD/.pocket-ui}
exec sh "$ROOT/start.sh" nextui
