#!/bin/sh
# Builds theme/dist/gdm/leocamp-gdm.gresource: Ubuntu's Yaru GDM theme with
# the Leo Camp rules appended and the boot scene as background. No sudo.
# Needs theme/build.py to have run (it generates the dark wallpaper).
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
OUT="$HERE/../dist/gdm"
SRC=/usr/share/gnome-shell/theme/Yaru/gnome-shell-theme.gresource
WALL="$HERE/../dist/backgrounds/leocamp-dark.svg"
[ -f "$WALL" ] || { echo "Prima: python3 -I theme/build.py"; exit 1; }

rm -rf "$OUT" && mkdir -p "$OUT/tree"
files=""
for res in $(gresource list "$SRC"); do
	rel=${res#/org/gnome/shell/theme/}
	mkdir -p "$OUT/tree/$(dirname "$rel")"
	gresource extract "$SRC" "$res" >"$OUT/tree/$rel"
	case "$rel" in pad-osd.css) ;; *.css) cat "$HERE/leocamp-gdm.css" >>"$OUT/tree/$rel" ;; esac
	files="$files    <file>$rel</file>
"
done
# The dark desktop wallpaper: its hero sparkle sits right, leaving the
# centre (where GDM puts the login dialog) clear.
python3 -I -c '
import sys, gi
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf
GdkPixbuf.Pixbuf.new_from_file_at_scale(sys.argv[1], 2880, 1800, True).savev(sys.argv[2], "png", [], [])
' "$WALL" "$OUT/tree/leocamp-gdm-bg.png"
{
	echo '<?xml version="1.0" encoding="UTF-8"?>'
	echo '<gresources>'
	echo '  <gresource prefix="/org/gnome/shell/theme">'
	printf '%s' "$files"
	echo '    <file>leocamp-gdm-bg.png</file>'
	echo '  </gresource>'
	echo '</gresources>'
} >"$OUT/tree/leocamp-gdm.gresource.xml"
glib-compile-resources --sourcedir="$OUT/tree" --target="$OUT/leocamp-gdm.gresource" \
	"$OUT/tree/leocamp-gdm.gresource.xml"

# Dry run checks: same resources as Ubuntu's plus the background, every CSS
# extracts with balanced braces and carries the Leo Camp block.
expected=$(($(gresource list "$SRC" | wc -l) + 1))
actual=$(gresource list "$OUT/leocamp-gdm.gresource" | wc -l)
[ "$actual" -eq "$expected" ] || { echo "ERRORE: $actual risorse, attese $expected"; exit 1; }
for res in $(gresource list "$OUT/leocamp-gdm.gresource" | grep "\.css$" | grep -v pad-osd); do
	css=$(gresource extract "$OUT/leocamp-gdm.gresource" "$res")
	open=$(printf '%s' "$css" | tr -cd '{' | wc -c)
	close=$(printf '%s' "$css" | tr -cd '}' | wc -c)
	[ "$open" -eq "$close" ] || { echo "ERRORE: graffe sbilanciate in $res"; exit 1; }
	printf '%s' "$css" | grep -q 'Leo Camp login screen' || { echo "ERRORE: regole mancanti in $res"; exit 1; }
	echo "ok  $res ($open regole)"
done
echo "ok  sfondo: $(gresource list "$OUT/leocamp-gdm.gresource" | grep -c leocamp-gdm-bg.png)"
echo "Costruito e verificato: $OUT/leocamp-gdm.gresource"
