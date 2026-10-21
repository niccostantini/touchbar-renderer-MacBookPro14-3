#!/bin/sh
# Development helper: reinstalls the themes and makes a running Mailspring
# re-read them by switching the light/dark theme names away and back.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
sh "$HERE/install.sh" >/dev/null
CONFIG=${MAILSPRING_DATA:-$HOME/snap/mailspring/common}/config.json
set_names() {
	python3 - "$CONFIG" "$1" "$2" <<'PY'
import json, os, sys
path, light, dark = sys.argv[1:]
config = json.load(open(path))
appearance = config["*"]["core"]["appearance"]
appearance.update(lightThemeName=light, darkThemeName=dark)
# In place: Mailspring watches this file.
with open(path, "w") as out:
    json.dump(config, out, indent=2)
PY
}
set_names ui-light ui-dark
sleep 1.5
set_names ui-leocamp ui-leocamp-dark
