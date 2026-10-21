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
install -m644 "$HERE/leocamp-whatsapp.svg" "$CANDY/leocamp-whatsapp.svg"
install -m644 "$HERE/leocamp-localsend.svg" "$CANDY/leocamp-localsend.svg"
install -m644 "$HERE/leocamp-vscode.svg" "$CANDY/leocamp-vscode.svg"
install -m644 "$HERE/leocamp-onlyoffice.svg" "$CANDY/leocamp-onlyoffice.svg"
install -m644 "$HERE/leocamp-mailspring.svg" "$CANDY/leocamp-mailspring.svg"
gtk-update-icon-cache -q -f "$DATA/icons/candy-icons" 2>/dev/null || true

mkdir -p "$DATA/applications"
for desktop in /var/lib/snapd/desktop/applications/snap-store_snap-store.desktop; do
	[ -f "$desktop" ] || continue
	sed 's|^Icon=.*|Icon=leocamp-app-center|' "$desktop" >"$DATA/applications/$(basename "$desktop")"
done
for desktop in /var/lib/snapd/desktop/applications/localsend_localsend.desktop; do
	[ -f "$desktop" ] || continue
	sed 's|^Icon=.*|Icon=leocamp-localsend|' "$desktop" >"$DATA/applications/$(basename "$desktop")"
done
for desktop in /var/lib/snapd/desktop/applications/code_code.desktop \
	/var/lib/snapd/desktop/applications/code_code-url-handler.desktop; do
	[ -f "$desktop" ] || continue
	sed 's|^Icon=.*|Icon=leocamp-vscode|' "$desktop" >"$DATA/applications/$(basename "$desktop")"
done
for desktop in /var/lib/snapd/desktop/applications/mailspring_mailspring.desktop; do
	[ -f "$desktop" ] || continue
	sed 's|^Icon=.*|Icon=leocamp-mailspring|' "$desktop" >"$DATA/applications/$(basename "$desktop")"
done
for desktop in /var/lib/snapd/desktop/applications/onlyoffice-desktopeditors_onlyoffice-desktopeditors.desktop; do
	[ -f "$desktop" ] || continue
	sed 's|^Icon=.*|Icon=leocamp-onlyoffice|' "$desktop" >"$DATA/applications/$(basename "$desktop")"
done
# WhatsApp Web is a Brave web app: its launcher already lives in the home.
# Brave may rewrite it when the app is reinstalled; rerun this script then.
for desktop in "$DATA"/applications/brave-*.desktop; do
	grep -q '^Name=WhatsApp' "$desktop" 2>/dev/null || continue
	sed -i 's|^Icon=.*|Icon=leocamp-whatsapp|' "$desktop"
done
update-desktop-database -q "$DATA/applications" 2>/dev/null || true
echo "Icone Terminale e App Center installate in Candy."
echo "Per annullare: rm $DATA/applications/mailspring_mailspring.desktop $CANDY/leocamp-mailspring.svg $DATA/applications/onlyoffice-desktopeditors_*.desktop $CANDY/leocamp-onlyoffice.svg $DATA/applications/snap-store_snap-store.desktop $CANDY/org.gnome.Ptyxis.svg $CANDY/leocamp-app-center.svg $DATA/applications/code_code*.desktop $CANDY/leocamp-vscode.svg"
