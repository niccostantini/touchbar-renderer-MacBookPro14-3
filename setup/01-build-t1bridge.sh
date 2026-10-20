#!/bin/sh
# Scarica t1bridge accanto a questo repository, applica la patch locale e
# compila i programmi. Non serve sudo e non installa nulla nel sistema.
# Uso: sh setup/01-build-t1bridge.sh
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
T1BRIDGE=$(dirname "$REPO")/t1bridge
# Versione su cui la patch è stata scritta e provata.
COMMIT=81cbdf81026a16e02f0bea74735c6b029a8ffae2

if [ ! -d "$T1BRIDGE/.git" ]; then
	git clone https://github.com/standardagents/t1bridge "$T1BRIDGE"
fi
cd "$T1BRIDGE"
if git diff --quiet; then
	git checkout --quiet "$COMMIT"
	git apply "$REPO/setup/t1bridge-local.patch"
	echo "patch applicata su $COMMIT"
else
	echo "t1bridge ha già modifiche locali: patch non riapplicata"
fi

build() { cargo build --locked --release "$@"; }
build -p t1-touchbar-hw --bin t1-touchbar-hw --features service
build -p t1-touchbar --bin t1-touchbar
build -p t1-daemons --bin t1-xart-storage
build -p t1-daemons --bin t1-ncm-ready
build -p t1-daemons --bin t1-keybag-relay --features keybag-relay-service
build -p t1-daemons --bin t1-touchid-auth --features auth-broker-service
build -p t1-daemons --bin t1-touchid
build -p t1-daemons --bin t1-legacy-recovery --features legacy-recovery
build -p t1-import --bin t1bridge
echo "compilato in $T1BRIDGE/target/release"
