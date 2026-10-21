#!/bin/sh
# Installa gpu-pref e il suo watchdog. Non cambia la GPU: per quello serve
# "sudo gpu-pref set igpu --apply".
# Uso:        sudo sh desktop/igpu/install.sh
# Ripristino: sudo sh desktop/igpu/install.sh --restore
set -eu
[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
HERE=$(cd "$(dirname "$0")" && pwd)

if [ "${1:-}" = "--restore" ]; then
	systemctl disable gpu-pref-watchdog.service 2>/dev/null || true
	rm -f /etc/systemd/system/gpu-pref-watchdog.service /usr/local/sbin/gpu-pref
	systemctl daemon-reload
	echo "gpu-pref rimosso (la variabile EFI e /var/lib/gpu-pref restano come sono)."
	exit 0
fi

install -Dm755 "$HERE/gpu-pref" /usr/local/sbin/gpu-pref
install -Dm644 "$HERE/gpu-pref-watchdog.service" /etc/systemd/system/gpu-pref-watchdog.service
systemctl daemon-reload
systemctl enable gpu-pref-watchdog.service
echo "Installato. Il watchdog parte solo se /var/lib/gpu-pref/armed esiste."
gpu-pref status
