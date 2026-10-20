#!/bin/sh
# Annulla 02-install-t1bridge.sh. Non tocca applespi né la partizione EFI.
# Uso: sudo sh setup/uninstall-t1bridge.sh
set -u

[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
systemctl disable t1-touchbar-hw.service 2>/dev/null
systemctl --global disable t1-touchbar.service 2>/dev/null
for v in $(dkms status t1bridge-dkms 2>/dev/null | sed -n 's|^t1bridge-dkms/\([^,]*\),.*|\1|p' | sort -u); do
	dkms remove "t1bridge-dkms/$v" --all
	rm -rf "/usr/src/t1bridge-dkms-$v"
done
rm -rf /usr/lib/t1bridge /usr/bin/t1bridge /usr/bin/t1-touchid
rm -f /usr/lib/systemd/system/t1-* /usr/lib/systemd/system/t1bridge-* \
	/usr/lib/systemd/user/t1-touchbar.service \
	/usr/lib/systemd/system-preset/80-t1bridge.preset /usr/lib/systemd/user-preset/80-t1bridge.preset \
	/usr/lib/sysusers.d/t1bridge.conf /usr/lib/tmpfiles.d/t1bridge.conf \
	/usr/lib/systemd/network/50-t1bridge-ncm.link \
	/usr/lib/udev/rules.d/99-zz-t1bridge-touchbar.rules /usr/lib/udev/rules.d/90-t1bridge-xart.rules \
	/usr/lib/modules-load.d/t1bridge.conf /usr/lib/modprobe.d/t1bridge.conf
sed -i '/^t1_cfgsel$/d' /etc/initramfs-tools/modules
systemctl daemon-reload
update-initramfs -u -k "$(uname -r)"
echo "Rimosso. Riavvia."
