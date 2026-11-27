#!/usr/bin/env python3
"""Genera l'icona di JARVIS (ADR-0225): «J» a tubo di neon su squircle scuro, con margine in stile macOS.

Uso: python3 scripts/make_icon.py   (richiede Pillow). Riscrive i file in apps/desktop/src-tauri/icons/.
"""
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "apps/desktop/src-tauri/icons"
S = 1024
SS = 3  # supersampling


def squircle(size, n=4.0):
    """Maschera a superellisse (angoli continui come le icone di macOS)."""
    big = size * SS
    m = Image.new("L", (big, big), 0)
    px = m.load()
    r = big / 2
    for y in range(big):
        dy = abs((y + 0.5 - r) / r)
        if dy > 1:
            continue
        span = (1 - dy**n) ** (1 / n)
        x0, x1 = int(r - r * span), int(r + r * span)
        for x in range(max(x0, 0), min(x1 + 1, big)):
            px[x, y] = 255
    return m.resize((size, size), Image.LANCZOS)


def glyph_path():
    """Punti della «J» (unità 1024): barra superiore, asta, gancio inferiore."""
    pts = [(350 + i * 3.2, 318) for i in range(0, 100)]  # barra 350→667
    pts += [(667, 318 + i * 3.2) for i in range(0, 92)]  # asta 318→612
    cx, cy, r = 507, 612, 160
    pts += [(cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a))) for a in range(0, 181, 2)]
    pts += [(347, 612 - i * 1.2) for i in range(0, 20)]  # risale un poco: terminale tondo
    return pts


def stroke_mask(width, scale=SS):
    m = Image.new("L", (S * scale, S * scale), 0)
    d = ImageDraw.Draw(m)
    rad = width * scale / 2
    for x, y in glyph_path():
        d.ellipse([x * scale - rad, y * scale - rad, x * scale + rad, y * scale + rad], fill=255)
    return m.resize((S, S), Image.LANCZOS)


def vertical_gradient(top, bottom, box):
    g = Image.linear_gradient("L").resize((S, S))
    a = Image.new("RGB", (S, S), top)
    b = Image.new("RGB", (S, S), bottom)
    return Image.composite(b, a, g)


def build():
    body = 824
    off = (S - body) // 2
    mask = Image.new("L", (S, S), 0)
    mask.paste(squircle(body), (off, off))

    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # Ombra sotto il corpo.
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shadow.putalpha(mask.filter(ImageFilter.GaussianBlur(22)).point(lambda v: int(v * 0.45)))
    shadow = ImageChops.offset(shadow, 0, 14)
    canvas = Image.alpha_composite(canvas, shadow)

    # Sfondo: gradiente blu notte e alone chiaro in alto a sinistra.
    bg = vertical_gradient((18, 52, 88), (4, 11, 24), None).convert("RGBA")
    halo = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(halo).ellipse([off - 120, off - 200, off + 560, off + 380], fill=(60, 150, 210, 70))
    halo = halo.filter(ImageFilter.GaussianBlur(90))
    bg = Image.alpha_composite(bg, halo)
    bg.putalpha(mask)
    canvas = Image.alpha_composite(canvas, bg)

    # Bordo sottile e riflesso nel bordo superiore.
    inner = mask.filter(ImageFilter.MinFilter(5))
    rim = ImageChops.subtract(mask, inner)
    rim_img = vertical_gradient((110, 190, 235), (30, 70, 100), None).convert("RGBA")
    rim_img.putalpha(rim.point(lambda v: int(v * 0.55)))
    canvas = Image.alpha_composite(canvas, rim_img)

    # Alone luminoso della «J».
    tube = stroke_mask(84)
    glow = Image.new("RGBA", (S, S), (40, 190, 255, 0))
    glow.putalpha(tube.filter(ImageFilter.GaussianBlur(34)).point(lambda v: int(v * 0.85)))
    canvas = Image.alpha_composite(canvas, glow)
    glow2 = Image.new("RGBA", (S, S), (90, 220, 255, 0))
    glow2.putalpha(tube.filter(ImageFilter.GaussianBlur(12)).point(lambda v: int(v * 0.6)))
    canvas = Image.alpha_composite(canvas, glow2)

    # Tubo: gradiente ciano → azzurro, con anima chiara al centro.
    fill = vertical_gradient((110, 232, 255), (36, 150, 255), None).convert("RGBA")
    fill.putalpha(tube)
    canvas = Image.alpha_composite(canvas, fill)
    core = Image.new("RGBA", (S, S), (236, 252, 255, 0))
    core.putalpha(stroke_mask(26).point(lambda v: int(v * 0.95)))
    canvas = Image.alpha_composite(canvas, core)
    return canvas


def main():
    icon = build()
    OUT.mkdir(parents=True, exist_ok=True)
    icon.save(OUT / "icon.png")
    icon.resize((256, 256), Image.LANCZOS).save(OUT / "128x128@2x.png")
    icon.resize((128, 128), Image.LANCZOS).save(OUT / "128x128.png")
    icon.resize((32, 32), Image.LANCZOS).save(OUT / "32x32.png")
    icon.save(OUT / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    icon.save(OUT / "icon.icns", sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)])
    print("icone scritte in", OUT)


if __name__ == "__main__":
    main()
