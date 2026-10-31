#!/bin/sh
# Runs INSIDE the container of build-flatpak.sh (see test-flatpak.sh): installs the .flatpak and starts it under Gamescope's
# headless backend at 1280x800, the way Game Mode starts a Non-Steam game, with an EmuDeck-style library in $HOME/Emulation.
# Writes /work/shots/*.jpg (the app's window, read from Gamescope's XWayland) and /work/shots/log.txt.
set -x
export HOME=/work/home XDG_RUNTIME_DIR=/tmp/xdg PATH=$PATH:/usr/games
mkdir -p "$XDG_RUNTIME_DIR" /work/shots; chmod 700 "$XDG_RUNTIME_DIR"
flatpak install -y --user --noninteractive /work/pocket-ui-deck.flatpak 2>&1 | tail -1
export XDG_DATA_DIRS=$HOME/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/share
# Gamescope sets DISPLAY (its XWayland) for the command; `flatpak run` passes it through (--socket=fallback-x11).
cat > /tmp/inner.sh <<'I'
sleep 6
echo "DISPLAY=$DISPLAY WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-}"
# cores as a player would get them: the script in the app, into the app's own data folder
flatpak run --command=sh io.github.pocketui.PocketUI -c 'CORES=mgba /app/share/pocket-ui/fetch-cores.sh' > /work/shots/cores.log 2>&1
flatpak run --env=POCKETUI_MOCK=1 io.github.pocketui.PocketUI > /work/shots/app.log 2>&1 &
sleep 12
xwininfo -root -tree | head -20
for w in $(xdotool search --name "" 2>/dev/null); do echo "win $w $(xdotool getwindowname $w 2>/dev/null) $(xdotool getwindowgeometry $w 2>/dev/null | tr '\n' ' ')"; done
id=$(xdotool search --class pocket-ui | head -1)
echo "pocket-ui window: $id"
[ -n "$id" ] && import -window "$id" -resize 1280x -quality 75 /work/shots/gamescope-shelf.jpg
inst=$(flatpak ps --columns=instance | head -1)
shot() { id=$(xdotool search --class pocket-ui | head -1); import -window "$id" -resize 1280x -quality 75 /work/shots/$1.jpg; }
shotgame() { id=$(xdotool search --name pocket-core | head -1); [ -n "$id" ] && import -window "$id" -resize 1280x -quality 75 /work/shots/$1.jpg; }
send() { flatpak enter "$inst" sh -c "echo $1 > /tmp/pocket-ui.in"; sleep "${2:-1.5}"; }
# The first-start steps without Wi-Fi and time zone, then the shelf, the game list and a game whose core is not installed.
send a 2; send a 2; send a 2; shot 02-extras; send start; send start; send a 3; shot 03-shelf
# Settings in categories: the list, then Sound & feel (no LEDs/Haptics row), Playing (Buttons: Xbox on the Deck), Help & tools.
send start 2; shot 07-settings; send down 2; send a 2; shot 08-sound-and-feel; send b 2; send down 2; send a 2; shot 09-playing; send b 2; send down 2; send down 2; send down 2; send down 2; send a 2; shot 10-help-tools; send b 2; send b 2
send a 2; shot 04-gamelist; send a 12; shotgame 05-game-running; send home 4; shot 06-pause-menu; send b 3; send quit 3
ls $HOME/.var/app/io.github.pocketui.PocketUI/data/pocket-ui/ 2>&1 | head -20
cat $HOME/.var/app/io.github.pocketui.PocketUI/data/pocket-ui/logs/library.log 2>&1 | head -3
touch /work/shots/done
I
timeout 120 gamescope --backend headless -W 1280 -H 800 -w 1280 -h 800 -f -- sh /tmp/inner.sh > /work/shots/log.txt 2>&1
tail -40 /work/shots/log.txt
