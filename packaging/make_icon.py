#!/usr/bin/env python3
"""Draws the NotepadRS icon (PNG + SVG) with pycairo.

Usage: python3 packaging/make_icon.py   (writes packaging/notepad-rs.png/.svg)
"""
import math
import os
import sys

import cairo

HERE = os.path.dirname(os.path.abspath(__file__))


def hexc(h, a=1.0):
    h = h.lstrip("#")
    return (int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255, a)


def rounded_rect(cr, x, y, w, h, r):
    cr.new_sub_path()
    cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
    cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
    cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
    cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
    cr.close_path()


def squircle(cr, x, y, s, n=5.0, steps=200):
    """Superellipse like modern app icons."""
    cx, cy, a = x + s / 2, y + s / 2, s / 2
    for i in range(steps + 1):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        px = cx + a * math.copysign(abs(ct) ** (2 / n), ct)
        py = cy + a * math.copysign(abs(st) ** (2 / n), st)
        if i == 0:
            cr.move_to(px, py)
        else:
            cr.line_to(px, py)
    cr.close_path()


def gear(cr, cx, cy, r_out, r_in, teeth):
    for i in range(teeth * 2):
        a0 = math.pi * 2 * i / (teeth * 2)
        a1 = math.pi * 2 * (i + 1) / (teeth * 2)
        r = r_out if i % 2 == 0 else r_in
        # slightly narrower tooth tops
        inset = 0.12 * (a1 - a0) if i % 2 == 0 else 0
        if i == 0:
            cr.move_to(cx + r * math.cos(a0 + inset), cy + r * math.sin(a0 + inset))
        else:
            cr.line_to(cx + r * math.cos(a0 + inset), cy + r * math.sin(a0 + inset))
        cr.line_to(cx + r * math.cos(a1 - inset), cy + r * math.sin(a1 - inset))
    cr.close_path()


def draw(cr, S):
    k = S / 256.0
    cr.scale(k, k)

    # Background: squircle with a rust gradient (accent colour of the UI theme)
    squircle(cr, 8, 8, 240)
    g = cairo.LinearGradient(0, 8, 0, 248)
    g.add_color_stop_rgba(0, *hexc("#f0773a"))
    g.add_color_stop_rgba(1, *hexc("#a8380f"))
    cr.set_source(g)
    cr.fill_preserve()
    cr.set_source_rgba(*hexc("#6b240a", 0.8))
    cr.set_line_width(2)
    cr.stroke()

    # Subtle highlight in the upper half
    cr.save()
    squircle(cr, 8, 8, 240)
    cr.clip()
    hl = cairo.LinearGradient(0, 8, 0, 130)
    hl.add_color_stop_rgba(0, 1, 1, 1, 0.18)
    hl.add_color_stop_rgba(1, 1, 1, 1, 0.0)
    cr.set_source(hl)
    cr.rectangle(0, 0, 256, 130)
    cr.fill()
    cr.restore()

    # Notepad, slightly tilted
    cr.save()
    cr.translate(124, 132)
    cr.rotate(math.radians(-7))
    px, py, pw, ph = -66, -86, 132, 172
    # shadow
    rounded_rect(cr, px + 5, py + 8, pw, ph, 12)
    cr.set_source_rgba(0, 0, 0, 0.28)
    cr.fill()
    # page
    rounded_rect(cr, px, py, pw, ph, 12)
    cr.set_source_rgba(*hexc("#fbf9f6"))
    cr.fill()
    # binding strip
    cr.save()
    rounded_rect(cr, px, py, pw, ph, 12)
    cr.clip()
    cr.rectangle(px, py, pw, 26)
    cr.set_source_rgba(*hexc("#2c2825"))
    cr.fill()
    cr.restore()
    # spiral rings
    for i in range(6):
        x = px + 16 + i * 20
        cr.set_source_rgba(*hexc("#1b1917"))
        cr.arc(x, py + 15, 4.2, 0, 2 * math.pi)
        cr.fill()
        cr.set_line_width(4)
        cr.set_line_cap(cairo.LINE_CAP_ROUND)
        cr.set_source_rgba(*hexc("#e9e2da"))
        cr.move_to(x, py + 15)
        cr.line_to(x, py - 6)
        cr.stroke()
    # code lines in warm syntax colours: (indent, [(length, colour)])
    kw, fn, st, tx, nu, cm = "#b8411b", "#8a5a3c", "#e0912f", "#6b625a", "#9a7b2f", "#b9b0a6"
    lines = [
        (0, [(26, kw), (46, fn)]),
        (14, [(20, kw), (30, st), (14, tx)]),
        (28, [(36, fn), (24, st)]),
        (28, [(22, nu), (34, tx)]),
        (14, [(12, tx)]),
        (0, [(58, cm)]),
        (0, [(30, kw), (40, fn)]),
    ]
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    cr.set_line_width(7)
    y = py + 46
    for indent, parts in lines:
        x = px + 16 + indent
        for length, col in parts:
            cr.set_source_rgba(*hexc(col))
            cr.move_to(x, y)
            cr.line_to(x + length, y)
            cr.stroke()
            x += length + 10
        y += 17
    cr.restore()

    # Dark gear badge with "RS"
    cx, cy = 188, 190
    cr.save()
    gear(cr, cx + 2, cy + 4, 46, 37, 10)
    cr.set_source_rgba(0, 0, 0, 0.3)
    cr.fill()
    gear(cr, cx, cy, 46, 37, 10)
    gg = cairo.LinearGradient(0, cy - 46, 0, cy + 46)
    gg.add_color_stop_rgba(0, *hexc("#3d3631"))
    gg.add_color_stop_rgba(1, *hexc("#1b1917"))
    cr.set_source(gg)
    cr.fill_preserve()
    cr.set_source_rgba(*hexc("#000000"))
    cr.set_line_width(2)
    cr.stroke()
    cr.arc(cx, cy, 30, 0, 2 * math.pi)
    cr.set_source_rgba(*hexc("#fbf9f6"))
    cr.fill()
    # "RS" centred in the gear (glyphs become paths in the SVG)
    cr.set_source_rgba(*hexc("#b8411b"))
    for face in ("DejaVu Sans", "Noto Sans", "Liberation Sans", "Sans"):
        cr.select_font_face(face, cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        if cr.get_scaled_font().get_font_face().get_family().lower().startswith(face.lower()[:5]):
            break
    cr.set_font_size(30)
    ext = cr.text_extents("RS")
    cr.move_to(cx - ext.x_bearing - ext.width / 2, cy - ext.y_bearing - ext.height / 2)
    cr.text_path("RS")
    cr.fill()
    cr.restore()


def main():
    png = os.path.join(HERE, "notepad-rs.png")
    svg = os.path.join(HERE, "notepad-rs.svg")
    out = sys.argv[1] if len(sys.argv) > 1 else None
    surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, 256, 256)
    draw(cairo.Context(surf), 256)
    surf.write_to_png(out or png)
    if not out:
        s = cairo.SVGSurface(svg, 256, 256)
        draw(cairo.Context(s), 256)
        s.finish()
        big = cairo.ImageSurface(cairo.FORMAT_ARGB32, 512, 512)
        draw(cairo.Context(big), 512)
        big.write_to_png(os.path.join(HERE, "notepad-rs-512.png"))


if __name__ == "__main__":
    main()
