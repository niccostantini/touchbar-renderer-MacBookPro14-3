#!/bin/sh
# Webcam FaceTime HD (T1) per tutte le app, via v4l2-relayd.
# Prima: sudo apt install v4l2-relayd
# Uso:        sudo sh desktop/webcam/install.sh
# Ripristino: sudo sh desktop/webcam/install.sh --restore
set -eu
[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
[ -n "${SUDO_USER:-}" ] || { echo "Lanciare con sudo da un utente normale"; exit 1; }
HERE=$(cd "$(dirname "$0")" && pwd)
USER_HOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
WP="$USER_HOME/.config/wireplumber/wireplumber.conf.d/51-hide-ibridge-h264.conf"

if [ "${1:-}" = "--restore" ]; then
	rm -f /etc/v4l2-relayd.d/facetime.conf /etc/modprobe.d/zz-facetime-loopback.conf "$WP"
	rm -rf /etc/systemd/system/v4l2-relayd@facetime.service.d
	systemctl daemon-reload
	systemctl stop v4l2-relayd@facetime.service 2>/dev/null || true
	modprobe -r v4l2loopback 2>/dev/null || true
	echo "Webcam virtuale rimossa (v4l2-relayd resta installato)."
	exit 0
fi

command -v v4l2-relayd >/dev/null || { echo "Prima: sudo apt install v4l2-relayd"; exit 1; }
install -Dm644 "$HERE/facetime.conf" /etc/v4l2-relayd.d/facetime.conf
install -Dm644 "$HERE/zz-facetime-loopback.conf" /etc/modprobe.d/zz-facetime-loopback.conf
install -Dm644 "$HERE/override.conf" /etc/systemd/system/v4l2-relayd@facetime.service.d/override.conf
install -Dm644 -o "$SUDO_USER" -g "$SUDO_USER" "$HERE/51-hide-ibridge-h264.conf" "$WP"
chown -R "$SUDO_USER:$SUDO_USER" "$USER_HOME/.config/wireplumber"

modprobe -r v4l2loopback 2>/dev/null || true
modprobe v4l2loopback
systemctl daemon-reload
systemctl restart v4l2-relayd.service
systemctl restart v4l2-relayd@facetime.service
sleep 1
systemctl is-active v4l2-relayd@facetime.service
grep -H . /sys/devices/virtual/video4linux/*/name
echo "Fatto. Riavvia WirePlumber come utente: systemctl --user restart wireplumber"
