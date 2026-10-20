#!/bin/sh
# Installs the Leo Camp boot splash and makes it the default Plymouth theme.
# Uso: python3 -I theme/plymouth/build.py && sudo sh theme/plymouth/install.sh
# Ripristino: sudo sh theme/plymouth/install.sh --restore
set -eu

[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
DEST=/usr/share/plymouth/themes/leocamp
SRC=$(cd "$(dirname "$0")/../dist/plymouth/leocamp" 2>/dev/null && pwd) || true

if [ "${1:-}" = "--restore" ]; then
	update-alternatives --auto default.plymouth
	update-alternatives --remove default.plymouth "$DEST/leocamp.plymouth" 2>/dev/null || true
	rm -rf "$DEST"
	update-initramfs -u
	echo "Tornato al tema di avvio di Ubuntu."
	exit 0
fi

[ -n "$SRC" ] && [ -f "$SRC/leocamp.plymouth" ] || {
	echo "Prima: python3 -I theme/plymouth/build.py (senza sudo)"
	exit 1
}
rm -rf "$DEST"
install -d "$DEST"
install -m644 "$SRC"/* "$DEST/"
update-alternatives --install /usr/share/plymouth/themes/default.plymouth default.plymouth \
	"$DEST/leocamp.plymouth" 200
update-alternatives --set default.plymouth "$DEST/leocamp.plymouth"
update-initramfs -u
echo "Leo Camp installato come schermo di avvio. Riavvia per vederlo."
