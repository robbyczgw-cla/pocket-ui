# pocket-ui on the TrimUI Brick: the one place for the environment (sourced by start.sh and by brick-kit.sh; $ROOT is our
# folder). Written for this project; nothing here comes from NextUI or MinUI.
SD=${SD:-/mnt/SDCARD}
# NextUI's libretro cores on the card, used by path and never copied or changed: fceumm, gambatte, gpsp, pcsx_rearmed, snes9x.
CORES=${CORES:-$SD/.system/tg5040/cores}
export POCKETUI_DEVICE=brick
export POCKETUI_DATA=$ROOT/data
# The CPU copies the core's picture (no GL): the first Brick build does not depend on the GPU library.
export POCKET_CORE_SOFTWARE=1
# The system's own ALSA "default" (a softvol on the audiocodec card); there is no PipeWire on this system.
export POCKET_CORE_ALSA_DEVICE=${POCKET_CORE_ALSA_DEVICE:-default}
export WAYLAND_DISPLAY=/tmp/pocket-wayland
export SWAYSOCK=/tmp/pocket-sway
export XDG_RUNTIME_DIR=/tmp
# The system's libraries first, ours (libwayland and libxkbcommon, which this system lacks) after them.
LD_LIBRARY_PATH=${LD_LIBRARY_PATH:+$LD_LIBRARY_PATH:}$ROOT/lib
export LD_LIBRARY_PATH
# pocket-ui starts a game with pocket-core: NextUI's cores are read from where they lie (POCKETUI_CORES), our own pocket-core from here.
# The view's /storage is our folder, so saves and save states land in it (docs/TRIMUI-BRICK.md), never next to a game of the owner's.
export POCKETUI_CORES=$CORES
export POCKETUI_HOST_BIN=$ROOT/bin/pocket-core
# Saves (battery saves, memory cards) of any game go to <system>/ here, not next to the game: a game on the owner's card is read only.
export POCKET_CORE_SAVE_DIR=$ROOT/storage/saves
# The audio device of the log line "pipewire (alsa)" is the ALSA PCM named above; there is no PipeWire on this system.
# The screen stays on the CPU copy unless POCKET_BRICK_GL=1 is set before start.sh (the kit's glui stage does).
