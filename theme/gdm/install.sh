#!/bin/sh
# Installs the Leo Camp login screen as the GDM theme (Ubuntu's own stays
# installed). Takes effect at the next login screen.
# Uso:        sh theme/gdm/build.sh && sudo sh theme/gdm/install.sh
# Ripristino: sudo sh theme/gdm/install.sh --restore
# Emergenza (schermata di accesso assente): Ctrl+Alt+F3, accedi, poi
#   sudo update-alternatives --auto gdm-theme.gresource && sudo systemctl restart gdm3
set -eu

[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
LINK=/usr/share/gnome-shell/gdm-theme.gresource
DEST=/usr/share/gnome-shell/theme/LeoCamp/gnome-shell-theme.gresource

if [ "${1:-}" = "--restore" ]; then
	update-alternatives --remove gdm-theme.gresource "$DEST" 2>/dev/null || true
	update-alternatives --auto gdm-theme.gresource
	rm -rf /usr/share/gnome-shell/theme/LeoCamp
	echo "Schermata di accesso di Ubuntu ripristinata (dal prossimo accesso)."
	exit 0
fi

SRC="$(cd "$(dirname "$0")/../dist/gdm" 2>/dev/null && pwd)/leocamp-gdm.gresource"
[ -f "$SRC" ] || { echo "Prima: sh theme/gdm/build.sh (senza sudo)"; exit 1; }
gresource list "$SRC" | grep -q '/org/gnome/shell/theme/gdm.css' || { echo "gresource non valido"; exit 1; }

install -Dm644 "$SRC" "$DEST"
update-alternatives --install "$LINK" gdm-theme.gresource "$DEST" 50
update-alternatives --set gdm-theme.gresource "$DEST"
echo "Leo Camp installato come schermata di accesso."
echo "La vedrai al prossimo logout o riavvio."
