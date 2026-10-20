#!/usr/bin/env python3
"""Builds the Leo Camp Plymouth boot theme (dark) into theme/dist/plymouth:
background, progress bar and pre-rendered greeting pieces, combined at random
by leocamp.script. Text is rendered here so the initramfs needs no fonts.
Usage: python3 -I theme/plymouth/build.py"""

import re
import shutil
import sys
from pathlib import Path

import gi

gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf  # noqa: E402

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "dist" / "plymouth" / "leocamp"

BG, INK = "#120d16", "#fbf3e6"
FONT = "font-family:'Noto Serif Display';font-style:italic;font-weight:700;font-size:64px"
TEXT_HEIGHT, BASELINE = 110, 82

# (prefix, suffix). The suffix follows the name with no space.
FORMULAS = [
    ("ci siamo,", "!"), ("sei pazzesca,", "!"), ("fingiamo di lavorare,", "?!"),
    ("sempre regina,", ""), ("fingi di lavorare,", "!"), ("entra in scena,", "!"),
    ("il pubblico ti aspetta,", "!"), ("oggi si brilla,", "!"), ("caffè e drama,", "?"),
    ("mai una gioia,", ""), ("spacca tutto,", "!"), ("tacchi e terminale,", "!"),
    ("che entrata,", "!"), ("lo sapevo che tornavi,", ""), ("apri le danze,", "!"),
    ("ti trovo benissimo,", "!"), ("niente panico,", "!"), ("l'ombretto è a posto,", "?"),
    ("facciamo finta di niente,", "!"), ("sipario su,", "!"),
]
# (name, stretched name or None).
NAMES = [
    ("Melania", "Melaniaaa"), ("Cinzia", "Cinzaaa"), ("Ottavia", "Ottaviaaa"),
    ("Kathrine", None), ("Ludmilla", "Ludmillaaa"), ("Zelda", "Zeldaaa"),
    ("Ermenegilda", "Ermenegildaaa"), ("Gertrude", "Gertrudeee"),
    ("Rosalinda", "Rosalindaaa"), ("Marlène", None), ("Cleopatra", "Cleopatraaa"),
    ("Ornella", "Ornellaaa"), ("Esmeralda", None), ("Wanda", "Wandaaa"),
    ("Brunilde", "Brunildeee"), ("Fiammetta", "Fiammettaaa"), ("Dolores", None),
    ("Teodolinda", "Teodolindaaa"), ("vita", "vitaaa"), ("tesoro", "tesorooo"),
]


def render_svg(svg, path):
    tmp = path.with_suffix(".svg")
    tmp.write_text(svg)
    GdkPixbuf.Pixbuf.new_from_file(str(tmp)).savev(str(path), "png", [], [])
    tmp.unlink()


def text_piece(text, path):
    """Renders `text` on a transparent strip, then trims it horizontally so
    pieces can be laid side by side on a shared baseline."""
    escaped = text.replace("&", "&amp;").replace("<", "&lt;")
    width = 60 * max(len(text), 1) + 80
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{TEXT_HEIGHT}">'
        f'<text x="20" y="{BASELINE}" style="{FONT}" fill="{INK}">{escaped}</text></svg>'
    )
    render_svg(svg, path)
    pixbuf = GdkPixbuf.Pixbuf.new_from_file(str(path))
    if not pixbuf.get_has_alpha():
        return
    pixels, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
    columns = [
        x for x in range(pixbuf.get_width())
        if any(pixels[y * stride + x * channels + 3] for y in range(pixbuf.get_height()))
    ]
    left, right = (min(columns), max(columns)) if columns else (0, 0)
    pixbuf.new_subpixbuf(left, 0, right - left + 1, TEXT_HEIGHT).savev(str(path), "png", [], [])


def background():
    """The mockup scene without text or bar, 10% smaller and raised."""
    scene = (HERE / "scene.svg").read_text()
    render_svg(scene, OUT / "background.png")


def progress_bar():
    width, height = 520, 26
    trough = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width + 10}" height="{height + 10}">'
        f'<rect x="6" y="6" width="{width}" height="{height}" rx="13" fill="#6a2be8"/>'
        f'<rect x="2" y="2" width="{width - 4}" height="{height - 4}" rx="11" fill="{BG}" '
        f'stroke="{INK}" stroke-width="4"/></svg>'
    )
    render_svg(trough, OUT / "bar.png")
    fill = (
        '<svg xmlns="http://www.w3.org/2000/svg" width="508" height="14">'
        '<defs><linearGradient id="p" x1="0" x2="1"><stop offset="0" stop-color="#2b12ff"/>'
        '<stop offset=".5" stop-color="#f2369b"/><stop offset="1" stop-color="#ff7b2e"/>'
        '</linearGradient></defs><rect width="508" height="14" rx="7" fill="url(#p)"/></svg>'
    )
    render_svg(fill, OUT / "fill.png")


def greetings():
    for index, (prefix, suffix) in enumerate(FORMULAS, 1):
        text_piece(prefix, OUT / f"f{index}.png")
        if suffix:
            text_piece(suffix, OUT / f"p{index}.png")
        else:
            # A one-pixel transparent piece keeps the script branch-free.
            GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, True, 8, 1, TEXT_HEIGHT).savev(
                str(OUT / f"p{index}.png"), "png", [], [])
    for index, (name, stretched) in enumerate(NAMES, 1):
        text_piece(name, OUT / f"n{index}.png")
        text_piece(stretched or name, OUT / f"n{index}s.png")


def main():
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    background()
    progress_bar()
    greetings()
    script = (HERE / "leocamp.script").read_text()
    script = re.sub(r"FORMULAS = \d+;", f"FORMULAS = {len(FORMULAS)};", script)
    script = re.sub(r"NAMES = \d+;", f"NAMES = {len(NAMES)};", script)
    (OUT / "leocamp.script").write_text(script)
    shutil.copy(HERE / "leocamp.plymouth", OUT / "leocamp.plymouth")
    print(f"built into {OUT}")


if __name__ == "__main__":
    sys.exit(main())
