#!/usr/bin/env python3
"""Builds the Leo Camp theme into theme/dist: Shell CSS (light/dark),
GTK 3/4 CSS, wallpapers and Papirus-derived icon themes with magenta folders.
Usage: python3 theme/build.py"""

import math
import os
import random
import shutil
from pathlib import Path

HERE = Path(__file__).resolve().parent
DIST = HERE / "dist"
PAPIRUS = Path("/usr/share/icons")

G1, G2 = "#f2369b", "#ff7b2e"  # magenta -> orange, as on brave.com/leo
SHADOW = G1
LIGHT = dict(
    bg="#fbf3e6", surface="#fffdf8", chip="#f4e7d6", hover="#efdcc6",
    ink="#17111c", line="#17111c", muted="#7a6a80", track="#e5d3bf", accent="#6a2be8",
)
DARK = dict(
    bg="#120d16", surface="#1d1524", chip="#2a2032", hover="#3a2c45",
    ink="#fbf3e6", line="#fbf3e6", muted="#b9a9bf", track="#3a2c45", accent="#9c6bff",
)


SHOW_APPS_ICON = """<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="-128 -128 256 256">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f2369b"/><stop offset="1" stop-color="#ff7b2e"/></linearGradient></defs>
<ellipse cx="0" cy="8" rx="122" ry="38" transform="rotate(-12)" fill="none" stroke="{ink}" stroke-width="6"/>
<path d="M0,-1 C0.09,-0.3 0.3,-0.09 1,0 C0.3,0.09 0.09,0.3 0,1 C-0.09,0.3 -0.3,0.09 -1,0 C-0.3,-0.09 -0.09,-0.3 0,-1Z" transform="translate(9,9) scale(118)" fill="#6a2be8"/>
<path d="M0,-1 C0.09,-0.3 0.3,-0.09 1,0 C0.3,0.09 0.09,0.3 0,1 C-0.09,0.3 -0.3,0.09 -1,0 C-0.3,-0.09 -0.09,-0.3 0,-1Z" transform="scale(118)" fill="url(#g)" stroke="{ink}" stroke-width="7" vector-effect="non-scaling-stroke" stroke-linejoin="round"/>
<g transform="translate(0,10) scale(0.31)" stroke="{ink}" stroke-width="12" stroke-linejoin="round">
<ellipse cx="-62" cy="150" rx="58" ry="26" fill="#ff8a3d"/><ellipse cx="62" cy="150" rx="58" ry="26" fill="#ff8a3d"/>
<path d="M0,-170 C70,-170 95,-110 98,-40 C150,10 150,110 100,145 C60,165 -60,165 -100,145 C-150,110 -150,10 -98,-40 C-95,-110 -70,-170 0,-170Z" fill="#17111c"/>
<path d="M0,-60 C55,-60 82,0 82,60 C82,120 50,148 0,148 C-50,148 -82,120 -82,60 C-82,0 -55,-60 0,-60Z" fill="#fbf3e6"/>
<ellipse cx="-30" cy="-105" rx="22" ry="30" fill="#fbf3e6"/><ellipse cx="30" cy="-105" rx="22" ry="30" fill="#fbf3e6"/>
<circle cx="-24" cy="-100" r="10" fill="#17111c" stroke="none"/><circle cx="24" cy="-100" r="10" fill="#17111c" stroke="none"/>
<path d="M-34,-62 C-20,-78 20,-78 34,-62 C20,-44 -20,-44 -34,-62Z" fill="#ffd23f"/></g>
</svg>
"""


def fill(template, values):
    return (HERE / "templates" / template).read_text() % values


def build_css():
    for name, scheme, palette in (("LeoCamp", "light", LIGHT), ("LeoCamp-Dark", "dark", DARK)):
        out = DIST / "themes" / name / "gnome-shell"
        out.mkdir(parents=True, exist_ok=True)
        yaru = "gnome-shell-dark.css" if scheme == "dark" else "gnome-shell.css"
        values = dict(palette, variant=name, scheme=scheme, yaru=yaru, g1=G1, g2=G2, shadow=SHADOW)
        (out / "gnome-shell.css").write_text(fill("gnome-shell.css", values))
        (out / "leocamp-show-apps.svg").write_text(SHOW_APPS_ICON.replace("{ink}", palette["line"]))
    both = {f"l_{k}": v for k, v in LIGHT.items()}
    both.update({f"d_{k}": v for k, v in DARK.items()})
    both.update(g1=G1, g2=G2, shadow=SHADOW)
    (DIST / "gtk-4.0").mkdir(parents=True, exist_ok=True)
    (DIST / "gtk-4.0" / "gtk.css").write_text(fill("gtk4.css", both))
    (DIST / "gtk-3.0").mkdir(parents=True, exist_ok=True)
    (DIST / "gtk-3.0" / "gtk.css").write_text(fill("gtk3.css", both))


SPARKLE = ("M0,-1 C0.09,-0.3 0.3,-0.09 1,0 C0.3,0.09 0.09,0.3 0,1 "
           "C-0.09,0.3 -0.3,0.09 -1,0 C-0.3,-0.09 -0.09,-0.3 0,-1Z")


def sparkle(x, y, r, fill_ref, stroke, shadow, rotate=0, width=9):
    offset = r * 0.07 + 8
    shape = (f'<path d="{SPARKLE}" transform="translate({{x}},{{y}}) rotate({rotate}) '
             f'scale({r})" vector-effect="non-scaling-stroke"')
    return (
        shape.format(x=x + offset, y=y + offset) + f' fill="{shadow}"/>\n'
        + shape.format(x=x, y=y)
        + f' fill="{fill_ref}" stroke="{stroke}" stroke-width="{width}" '
        'stroke-linejoin="round"/>\n'
    )


def wallpaper(palette, dark):
    w, h = 2880, 1800
    rng = random.Random(14)
    ink, bg = palette["ink"], palette["bg"]
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">',
        "<defs>",
        f'<linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{G1}"/>'
        f'<stop offset="1" stop-color="{G2}"/></linearGradient>',
        '<linearGradient id="v" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#6a2be8"/>'
        '<stop offset="1" stop-color="#b13bd6"/></linearGradient>',
        f'<pattern id="dots" width="44" height="44" patternUnits="userSpaceOnUse">'
        f'<circle cx="4" cy="4" r="2.4" fill="{ink}" opacity="{0.16 if dark else 0.12}"/></pattern>',
        "</defs>",
        f'<rect width="{w}" height="{h}" fill="{bg}"/>',
        f'<rect width="{w}" height="{h}" fill="url(#dots)"/>',
    ]
    # Orbits around the hero sparkle, like the rings on the Leo page.
    cx, cy = 2050, 900
    for i, (rx, ry, rot) in enumerate(((760, 250, -14), (620, 190, 18), (900, 330, -4))):
        dash = ' stroke-dasharray="2 26" stroke-linecap="round"' if i == 2 else ""
        parts.append(
            f'<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" transform="rotate({rot} {cx} {cy})" '
            f'fill="none" stroke="{ink}" stroke-width="{6 if i < 2 else 8}"{dash}/>'
        )
    parts.append(sparkle(cx, cy, 430, "url(#g)", ink, "#6a2be8", rotate=0, width=12))
    parts.append(sparkle(cx + 520, cy - 420, 120, "url(#v)", ink, G1, rotate=12))
    parts.append(sparkle(560, 1340, 210, "url(#g)", ink, "#6a2be8", rotate=-10, width=10))
    parts.append(sparkle(900, 520, 70, bg, ink, G1, rotate=20, width=7))
    for _ in range(14):
        x, y = rng.uniform(150, w - 150), rng.uniform(150, h - 250)
        if math.hypot(x - cx, y - cy) < 600:
            continue
        r = rng.uniform(14, 34)
        parts.append(sparkle(x, y, r, ink, ink, G1 if rng.random() < 0.5 else G2,
                             rotate=rng.uniform(-20, 20), width=2))
    parts.append("</svg>")
    return "\n".join(parts)


def build_wallpapers():
    out = DIST / "backgrounds"
    out.mkdir(parents=True, exist_ok=True)
    (out / "leocamp-light.svg").write_text(wallpaper(LIGHT, dark=False))
    (out / "leocamp-dark.svg").write_text(wallpaper(DARK, dark=True))


def build_icons():
    """Papirus with magenta folders: every places icon that Papirus links to a
    *-blue* variant is re-linked to the matching *-magenta* one."""
    base = PAPIRUS / "Papirus"
    for name, parent in (("LeoCamp-Papirus", "Papirus"), ("LeoCamp-Papirus-Dark", "Papirus-Dark")):
        root = DIST / "icons" / name
        if root.exists():
            shutil.rmtree(root)
        dirs = []
        for places in sorted(base.glob("*/places")):
            target_dir = root / places.relative_to(base)
            count = 0
            for icon in places.iterdir():
                if not icon.is_symlink():
                    continue
                target = os.readlink(icon)
                if "-blue" not in target:
                    continue
                magenta = places / target.replace("-blue", "-magenta")
                if not magenta.exists():
                    continue
                target_dir.mkdir(parents=True, exist_ok=True)
                (target_dir / icon.name).symlink_to(magenta.resolve())
                count += 1
            if count:
                dirs.append(str(places.relative_to(base)))
        sections = parse_sections(base / "index.theme")
        lines = [
            "[Icon Theme]", f"Name={name}", "Comment=Papirus with Leo Camp magenta folders",
            f"Inherits={parent},breeze,hicolor", "Example=folder", "FollowsColorScheme=true",
            f"Directories={','.join(dirs)}", "",
        ]
        for directory in dirs:
            lines.append(f"[{directory}]")
            lines.extend(sections.get(directory, []))
            lines.append("")
        (root / "index.theme").write_text("\n".join(lines))


def parse_sections(path):
    sections, current = {}, None
    for line in path.read_text().splitlines():
        if line.startswith("[") and line.endswith("]"):
            current = line[1:-1]
            sections[current] = []
        elif current and "=" in line:
            sections[current].append(line)
    return sections


if __name__ == "__main__":
    build_css()
    build_wallpapers()
    build_icons()
    print(f"built into {DIST}")
