#!/bin/sh
# Installs the Leo Camp themes into Mailspring (snap) and selects them in
# "Automatic" mode, so Mailspring follows GNOME's light/dark style like the
# rest of the desktop. No sudo. Undo: sh install.sh --undo
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
DATA=${MAILSPRING_DATA:-$HOME/snap/mailspring/common}
CONFIG="$DATA/config.json"
BACKUP="$CONFIG.leocamp-backup"
[ -d "$DATA" ] || { echo "Mailspring (snap) non trovato in $DATA: avvialo una volta e riprova."; exit 1; }

if [ "${1:-}" = "--undo" ]; then
	rm -rf "$DATA/packages/ui-leocamp" "$DATA/packages/ui-leocamp-dark"
	[ -f "$BACKUP" ] && mv "$BACKUP" "$CONFIG"
	echo "Temi Leo Camp rimossi da Mailspring; configurazione precedente ripristinata."
	exit 0
fi

for theme in ui-leocamp ui-leocamp-dark; do
	rm -rf "$DATA/packages/$theme"
	mkdir -p "$DATA/packages/$theme"
	cp -r "$HERE/$theme/." "$DATA/packages/$theme/"
	cp "$HERE/leocamp.less" "$DATA/packages/$theme/styles/"
done

[ -f "$BACKUP" ] || cp "$CONFIG" "$BACKUP" 2>/dev/null || true
python3 - "$CONFIG" <<'PY'
import json, os, sys
path = sys.argv[1]
config = json.load(open(path)) if os.path.exists(path) else {}
core = config.setdefault("*", {}).setdefault("core", {})
core["theme"] = "ui-automatic"
appearance = core.setdefault("appearance", {})
appearance.update(lightThemeName="ui-leocamp", darkThemeName="ui-leocamp-dark", useSystemAccent=False)
tmp = path + ".tmp"
json.dump(config, open(tmp, "w"), indent=2)
os.replace(tmp, path)
PY
echo "Temi Leo Camp installati in Mailspring (tema: Automatico)."
echo "Per annullare: sh $HERE/install.sh --undo"
