#!/bin/sh
# Reinstalla solo il servizio hardware t1-touchbar-hw (dopo aver cambiato la
# patch e rilanciato 01-build-t1bridge.sh). Alla prima esecuzione conserva
# l'originale come .orig.
# Uso: sudo sh setup/update-hw-service.sh
set -eu

[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
REPO=$(cd "$(dirname "$0")/.." && pwd)
SRC=$(dirname "$REPO")/t1bridge
cp -n /usr/lib/t1bridge/t1-touchbar-hw /usr/lib/t1bridge/t1-touchbar-hw.orig || true
install -m755 "$SRC/target/release/t1-touchbar-hw" /usr/lib/t1bridge/t1-touchbar-hw
systemctl restart t1-touchbar-hw.service
echo "Fatto. Per tornare all'originale:"
echo "  sudo install -m755 /usr/lib/t1bridge/t1-touchbar-hw.orig /usr/lib/t1bridge/t1-touchbar-hw && sudo systemctl restart t1-touchbar-hw.service"
