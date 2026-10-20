#!/bin/sh
# Compila questo renderer e lo seleziona per t1bridge. Non serve sudo.
# t1bridge torna da solo al renderer integrato se questo termina.
# Uso: sh setup/03-install-renderer.sh
# Per tornare al renderer integrato:
#   rm ~/.config/t1bridge/renderer && systemctl --user restart t1-touchbar.service
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO"
cargo test --release
cargo build --release
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/t1bridge"
ln -sfn "$REPO/target/release/touchbar-renderer" "${XDG_CONFIG_HOME:-$HOME/.config}/t1bridge/renderer"
systemctl --user restart t1-touchbar.service
echo "Renderer attivo."
