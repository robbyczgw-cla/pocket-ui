#!/bin/sh
# On the Deck (Desktop Mode, Konsole): unpack the bundle, run this, done.  ./install.sh [--no-steam]
# Copies the bundle to ~/.local/share/pocket-ui-app and adds "pocket-ui" to Steam as a Non-Steam game.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
dest=$HOME/.local/share/pocket-ui-app
mkdir -p "$dest"
cp -a "$here"/. "$dest"/
chmod +x "$dest"/pocket-ui "$dest"/pocket-core "$dest"/*.sh "$dest"/*.py
if [ "${1:-}" != "--no-steam" ]; then
  echo "Closing Steam first is safest (Steam rewrites shortcuts.vdf on exit)."
  python3 -I "$dest/add-to-steam.py" --exe "$dest/pocket-ui-deck.sh" || echo "Could not edit Steam's shortcuts; add $dest/pocket-ui-deck.sh by hand (Steam > Add a Non-Steam Game)."
fi
echo "Installed to $dest. In Game Mode: pocket-ui > Controller settings > Gamepad layout."
