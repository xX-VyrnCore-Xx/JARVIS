#!/usr/bin/env python3
"""Genera icone e grafiche degli installer di JARVIS (ADR-0036).

Uso: python3 scripts/make_brand_assets.py
Richiede Pillow. I file generati sono versionati in apps/desktop/src-tauri/{icons,installer}: lo script serve
solo a rigenerarli dopo una modifica al disegno. Nessuna risorsa esterna: forme disegnate e caratteri di sistema
(Inter se presente, altrimenti DejaVu o il carattere predefinito di Pillow).
"""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "apps" / "desktop" / "src-tauri"
ICONS = TAURI / "icons"
INST = TAURI / "installer"

CYAN = (63, 208, 255)
VIOLET = (123, 97, 255)
NAVY_TOP = (14, 44, 78)
NAVY_BOTTOM = (3, 9, 20)


def font(size: int, weight: str = "SemiBold") -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for path in (
        f"/usr/share/fonts/opentype/inter/Inter-{weight}.otf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if weight in ("Bold", "SemiBold") else
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
    ):
        if Path(path).is_file():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def vgradient(w: int, h: int, top: tuple, bottom: tuple) -> Image.Image:
    col = Image.new("RGB", (1, h))
    for y in range(h):
        t = y / max(h - 1, 1)
        col.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    return col.resize((w, h))


def radial(size: int, inner: tuple, outer: tuple, power: float = 1.0) -> Image.Image:
    """Gradiente radiale RGBA (inner al centro, outer al bordo)."""
    small = 256
    img = Image.new("RGBA", (small, small))
    c = (small - 1) / 2
    for y in range(small):
        for x in range(small):
            d = min(math.hypot(x - c, y - c) / c, 1.0) ** power
            img.putpixel((x, y), tuple(round(a + (b - a) * d) for a, b in zip(inner, outer)))
    return img.resize((size, size), Image.LANCZOS)


def reactor(size: int) -> Image.Image:
    """Emblema: anelli concentrici, corona a segmenti e nucleo luminoso (RGBA, sfondo trasparente)."""
    s = size
    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    c = s / 2

    def ring(r: float, width: float, color: tuple) -> None:
        d.ellipse((c - r, c - r, c + r, c + r), outline=color, width=max(1, round(width)))

    ring(s * 0.46, s * 0.012, CYAN + (110,))
    # corona semplificata: 16 segmenti ampi restano leggibili anche nella tray icon
    r_out, r_in = s * 0.42, s * 0.355
    for i in range(20):
        if i % 5 == 4:
            continue
        a0 = math.radians(i * 18 + 2.5)
        a1 = math.radians(i * 18 + 15.5)
        pts = [
            (c + r_out * math.cos(a0), c + r_out * math.sin(a0)),
            (c + r_out * math.cos(a1), c + r_out * math.sin(a1)),
            (c + r_in * math.cos(a1), c + r_in * math.sin(a1)),
            (c + r_in * math.cos(a0), c + r_in * math.sin(a0)),
        ]
        d.polygon(pts, fill=CYAN + (235,))
    # arco viola parziale e anello sottile interno
    rr = s * 0.31
    d.arc((c - rr, c - rr, c + rr, c + rr), 200, 340, fill=VIOLET + (255,), width=round(s * 0.022))
    d.arc((c - rr, c - rr, c + rr, c + rr), 20, 160, fill=CYAN + (200,), width=round(s * 0.022))
    ring(s * 0.25, s * 0.008, (200, 240, 255, 150))

    glow = layer.filter(ImageFilter.GaussianBlur(s * 0.02))
    out = Image.alpha_composite(Image.new("RGBA", (s, s), (0, 0, 0, 0)), glow)
    out = Image.alpha_composite(out, layer)

    # nucleo: alone e sfera
    halo = radial(round(s * 0.62), CYAN + (190,), CYAN + (0,), 0.7)
    out.alpha_composite(halo, (round(c - halo.width / 2), round(c - halo.height / 2)))
    core_d = round(s * 0.34)
    core = radial(core_d, (245, 253, 255, 255), (20, 120, 200, 255), 0.85)
    mask = Image.new("L", (core_d, core_d), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, core_d - 1, core_d - 1), fill=255)
    out.paste(core, (round(c - core_d / 2), round(c - core_d / 2)), mask)
    return out


def overlay(img: Image.Image, paint) -> None:
    """Disegna con trasparenza: ImageDraw sostituisce i pixel, quindi si dipinge su un livello e si compone."""
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    paint(ImageDraw.Draw(layer))
    img.alpha_composite(layer)


def grid(img: Image.Image, x0: int, y0: int, x1: int, y1: int, step: int, alpha: int, width: int = 1) -> None:
    def paint(g: ImageDraw.ImageDraw) -> None:
        for x in range(x0 + step, x1, step):
            g.line((x, y0, x, y1), fill=CYAN + (alpha,), width=width)
        for y in range(y0 + step, y1, step):
            g.line((x0, y, x1, y), fill=CYAN + (alpha,), width=width)
    overlay(img, paint)


def squircle_mask(size: int, box: tuple[int, int, int, int], radius: int) -> Image.Image:
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=radius, fill=255)
    return m


def app_icon(size: int = 1024, margin: int = 100, shadow: bool = True) -> Image.Image:
    """Icona in stile macOS: lastra arrotondata scura con emblema; `margin` segue la griglia di Apple."""
    k = 2  # sovracampionamento
    S, M = size * k, margin * k
    box = (M, M, S - M, S - M)
    radius = round((S - 2 * M) * 0.225)
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    if shadow:
        sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        ImageDraw.Draw(sh).rounded_rectangle((box[0], box[1] + 14 * k, box[2], box[3] + 14 * k), radius=radius, fill=(0, 0, 0, 120))
        canvas = Image.alpha_composite(canvas, sh.filter(ImageFilter.GaussianBlur(18 * k)))
    plate = vgradient(S, S, NAVY_TOP, NAVY_BOTTOM).convert("RGBA")
    # griglia leggera e riflesso in alto
    grid(plate, M, M, S - M, S - M, (S - 2 * M) // 8, 12, k)
    em = reactor(round((S - 2 * M) * 0.86))
    plate.alpha_composite(em, ((S - em.width) // 2, (S - em.height) // 2))
    shine = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shine).ellipse((M - S * 0.2, M - S * 0.55, S - M + S * 0.2, S * 0.42), fill=(255, 255, 255, 22))
    plate = Image.alpha_composite(plate, shine)
    mask = squircle_mask(S, box, radius)
    canvas.paste(plate, (0, 0), mask)
    # bordo interno sottile
    edge = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle(box, radius=radius, outline=CYAN + (90,), width=3 * k)
    canvas = Image.alpha_composite(canvas, edge)
    return canvas.resize((size, size), Image.LANCZOS)


def glass_card(img: Image.Image, box: tuple[int, int, int, int]) -> None:
    """Lastra di vetro chiaro dietro le icone: le etichette di Finder (nere in modalità chiara, bianche in
    modalità scura) restano leggibili in entrambi i casi."""
    w, h = box[2] - box[0], box[3] - box[1]
    card = vgradient(w, h, (138, 168, 196), (104, 134, 164)).convert("RGBA")
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, w - 1, h - 1), radius=22, fill=255)
    sh = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle((box[0], box[1] + 8, box[2], box[3] + 8), radius=22, fill=(0, 0, 0, 140))
    img.alpha_composite(sh.filter(ImageFilter.GaussianBlur(14)))
    img.paste(card, box[:2], mask)
    overlay(img, lambda d: d.rounded_rectangle(box, radius=22, outline=CYAN + (150,), width=1))


def dmg_background(w: int = 660, h: int = 400) -> Image.Image:
    img = vgradient(w, h, (12, 34, 62), (4, 10, 22)).convert("RGBA")
    grid(img, 0, 0, w, h, 30, 16)
    em = reactor(520)
    em.putalpha(ImageChops.multiply(em.getchannel("A"), Image.new("L", em.size, 34)))
    img.alpha_composite(em, (w - 300, -230))
    d = ImageDraw.Draw(img)
    d.text((40, 26), "JARVIS", font=font(28, "Bold"), fill=(227, 244, 255))
    d.text((41, 62), "Assistente personale privato e locale", font=font(13, "Regular"), fill=(127, 155, 179))
    # icone di Finder centrate in (170, 200) e (490, 200), etichette sotto
    glass_card(img, (50, 104, 610, 300))

    def arrow(g: ImageDraw.ImageDraw) -> None:
        y = 196
        for i in range(12):
            x0 = 258 + i * 10
            g.ellipse((x0, y - 3, x0 + 6, y + 3), fill=(8, 28, 52, 90 + i * 13))
        g.polygon([(384, y - 12), (404, y), (384, y + 12)], fill=(8, 28, 52, 255))
    overlay(img, arrow)
    d = ImageDraw.Draw(img)
    d.text((w // 2, 330), "Trascina JARVIS nella cartella Applicazioni", font=font(15, "SemiBold"), fill=(227, 244, 255), anchor="mm")
    d.text((w // 2, 354), "Primo avvio: Impostazioni di Sistema › Privacy e sicurezza › «Apri comunque»",
           font=font(11, "Regular"), fill=(150, 176, 200), anchor="mm")
    return img


def nsis_sidebar(w: int = 164, h: int = 314) -> Image.Image:
    img = vgradient(w, h, NAVY_TOP, NAVY_BOTTOM).convert("RGBA")
    em = reactor(150)
    img.alpha_composite(em, ((w - 150) // 2, 52))
    d = ImageDraw.Draw(img)
    d.text((w // 2, 228), "JARVIS", font=font(22, "Bold"), fill=(227, 244, 255), anchor="mm")
    d.text((w // 2, 252), "assistente personale", font=font(11, "Regular"), fill=(127, 155, 179), anchor="mm")
    d.line((30, 275, w - 30, 275), fill=CYAN + (120,), width=1)
    return img


def nsis_header(w: int = 150, h: int = 57) -> Image.Image:
    img = Image.new("RGBA", (w, h), (255, 255, 255, 255))
    plate = vgradient(70, h, NAVY_TOP, NAVY_BOTTOM).convert("RGBA")
    em = reactor(50)
    plate.alpha_composite(em, (10, 3))
    img.paste(plate, (w - 70, 0))
    return img


def bmp(img: Image.Image, path: Path) -> None:
    img.convert("RGB").save(path, format="BMP")


def main() -> None:
    ICONS.mkdir(parents=True, exist_ok=True)
    INST.mkdir(parents=True, exist_ok=True)
    mac = app_icon(1024, 100, shadow=True)
    full = app_icon(1024, 24, shadow=False)  # Windows e Linux: la lastra occupa quasi tutto il riquadro
    full.save(ICONS / "icon.png")
    mac.save(ICONS / "icon.icns", format="ICNS")
    full.save(ICONS / "icon.ico", format="ICO", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    for name, px in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)):
        full.resize((px, px), Image.LANCZOS).save(ICONS / name)
    dmg_background().convert("RGB").save(INST / "dmg-background.png")
    bmp(nsis_sidebar(), INST / "nsis-sidebar.bmp")
    bmp(nsis_header(), INST / "nsis-header.bmp")
    print("risorse generate in", ICONS.relative_to(ROOT), "e", INST.relative_to(ROOT))


if __name__ == "__main__":
    main()
