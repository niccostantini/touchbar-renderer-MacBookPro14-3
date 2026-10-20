#!/bin/sh
# Installa t1bridge (solo Touch Bar) su Ubuntu, ricalcando i PKGBUILD Arch
# (packaging/arch/t1bridge e t1bridge-dkms). Non tocca applespi
# (tastiera/trackpad) né la partizione EFI.
# Uso: sudo sh setup/02-install-t1bridge.sh    (dopo 01-build-t1bridge.sh)
set -eu

[ "$(id -u)" = 0 ] || { echo "Serve sudo"; exit 1; }
[ -n "${SUDO_USER:-}" ] || { echo "Lanciare con sudo da un utente normale"; exit 1; }
REPO=$(cd "$(dirname "$0")/.." && pwd)
SRC=$(dirname "$REPO")/t1bridge
VER=$(sed -n 's/^PACKAGE_VERSION="\(.*\)"/\1/p' "$SRC/kernel/dkms/dkms.conf")
cd "$SRC"

echo "== 1/5 programmi"
for b in t1-touchbar-hw t1-touchbar t1-xart-storage t1-ncm-ready t1-keybag-relay t1-touchid-auth; do
	install -Dm755 target/release/$b /usr/lib/t1bridge/$b
done
install -Dm750 target/release/t1-legacy-recovery /usr/lib/t1bridge/t1-legacy-recovery
install -Dm755 target/release/t1-touchid /usr/bin/t1-touchid
install -Dm755 target/release/t1bridge /usr/bin/t1bridge

echo "== 2/5 servizi systemd, udev, utenti"
for u in t1-touchbar-hw.service t1-touchbar-hw.socket t1-xart-storage@.service t1-ncm-ready@.service \
	t1bridge-keybag.service t1-touchid-auth.service t1-touchid-auth.socket \
	t1bridge-fingerprint.socket t1bridge-import.service; do
	install -Dm644 systemd/$u /usr/lib/systemd/system/$u
done
install -Dm644 systemd/user/t1-touchbar.service /usr/lib/systemd/user/t1-touchbar.service
install -Dm644 systemd/80-t1bridge.preset /usr/lib/systemd/system-preset/80-t1bridge.preset
install -Dm644 systemd/user/80-t1bridge.preset /usr/lib/systemd/user-preset/80-t1bridge.preset
install -Dm644 systemd/t1bridge.sysusers /usr/lib/sysusers.d/t1bridge.conf
install -Dm644 systemd/t1bridge.conf /usr/lib/tmpfiles.d/t1bridge.conf
install -Dm644 systemd/50-t1bridge-ncm.link /usr/lib/systemd/network/50-t1bridge-ncm.link
install -Dm644 systemd/99-zz-t1bridge-touchbar.rules /usr/lib/udev/rules.d/99-zz-t1bridge-touchbar.rules
install -Dm644 systemd/90-t1bridge-xart.rules /usr/lib/udev/rules.d/90-t1bridge-xart.rules

echo "== 3/5 moduli kernel (DKMS)"
D=/usr/src/t1bridge-dkms-$VER
rm -rf "$D"
mkdir -p "$D"
cp -r kernel "$D/kernel"
cp kernel/dkms/dkms.conf "$D/dkms.conf"
dkms install "t1bridge-dkms/$VER" -k "$(uname -r)"
install -Dm644 packaging/arch/t1bridge-dkms/t1bridge.modules-load.conf /usr/lib/modules-load.d/t1bridge.conf
install -Dm644 packaging/arch/t1bridge-dkms/t1bridge.modprobe.conf /usr/lib/modprobe.d/t1bridge.conf
# Equivalente Ubuntu di mkinitcpio MODULES+=(t1_cfgsel)
grep -qx t1_cfgsel /etc/initramfs-tools/modules || echo t1_cfgsel >> /etc/initramfs-tools/modules

echo "== 4/5 attivazione (dal setup.md ufficiale)"
systemd-sysusers /usr/lib/sysusers.d/t1bridge.conf
systemd-tmpfiles --create /usr/lib/tmpfiles.d/t1bridge.conf
usermod -aG t1bridge "$SUDO_USER"
udevadm control --reload
systemctl daemon-reload
systemctl enable t1-touchbar-hw.service
systemctl --global enable t1-touchbar.service

echo "== 5/5 initramfs"
update-initramfs -u -k "$(uname -r)"

echo
echo "FATTO. Controllo: applespi (tastiera/trackpad) deve restare disponibile:"
modinfo -n applespi
echo "Ora SPEGNI completamente il Mac, aspetta 20 s e riaccendi."
