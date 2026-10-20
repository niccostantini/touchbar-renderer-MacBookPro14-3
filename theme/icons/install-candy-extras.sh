#!/bin/sh
# Adds the Leo Camp Candy-style icons for Terminal (Ptyxis) and App Center.
# App Center's launcher points at a PNG inside the snap, so a per-user copy
# of its .desktop file is made with the themed icon name. No sudo.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
DATA=${XDG_DATA_HOME:-$HOME/.local/share}
CANDY="$DATA/icons/candy-icons/apps/scalable"
[ -d "$CANDY" ] || { echo "Candy icons non installate in $DATA/icons/candy-icons"; exit 1; }

install -m644 "$HERE/leocamp-terminal.svg" "$CANDY/org.gnome.Ptyxis.svg"
install -m644 "$HERE/leocamp-app-center.svg" "$CANDY/leocamp-app-center.svg"
gtk-update-icon-cache -q -f "$DATA/icons/candy-icons" 2>/dev/null || true

mkdir -p "$DATA/applications"
for desktop in /var/lib/snapd/desktop/applications/snap-store_snap-store.desktop; do
	[ -f "$desktop" ] || continue
	sed 's|^Icon=.*|Icon=leocamp-app-center|' "$desktop" >"$DATA/applications/$(basename "$desktop")"
done
update-desktop-database -q "$DATA/applications" 2>/dev/null || true
echo "Icone Terminale e App Center installate in Candy."
echo "Per annullare: rm $DATA/applications/snap-store_snap-store.desktop $CANDY/org.gnome.Ptyxis.svg $CANDY/leocamp-app-center.svg"
