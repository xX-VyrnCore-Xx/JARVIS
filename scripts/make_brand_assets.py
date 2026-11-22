#!/usr/bin/env python3
"""Genera icone e grafiche degli installer di JARVIS (ADR-0036).

Uso: python3 scripts/make_brand_assets.py
Richiede Pillow. Il badge sorgente è logo.png nella radice del repository; i font sono di sistema
(Inter se presente, altrimenti DejaVu o il carattere predefinito di Pillow).
"""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "apps" / "desktop" / "src-tauri"
ICONS = TAURI / "icons"
INST = TAURI / "installer"
ICON_SOURCE = ROOT / "logo.png"
ANDROID_ICONS = ROOT / "apps" / "android" / "app" / "src" / "main" / "res" / "drawable-nodpi"

CYAN = (63, 208, 255)
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


@lru_cache(maxsize=1)
def source_emblem() -> Image.Image:
    """Carica il badge originale fornito dal proprietario e ritaglia solo il margine trasparente."""
    with Image.open(ICON_SOURCE) as source:
        frame = source.convert("RGBA")
    bounds = frame.getchannel("A").point(lambda alpha: 255 if alpha > 16 else 0).getbbox()
    if bounds is None:
        raise ValueError(f"Nessun pixel opaco nell'icona sorgente: {ICON_SOURCE}")
    return frame.crop(bounds)


def brand_emblem(size: int) -> Image.Image:
    source = source_emblem()
    scale = min(size / source.width, size / source.height)
    dimensions = (round(source.width * scale), round(source.height * scale))
    source = source.resize(dimensions, Image.Resampling.LANCZOS)
    emblem = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    emblem.alpha_composite(source, ((size - source.width) // 2, (size - source.height) // 2))
    return emblem


def android_launcher_emblem(size: int = 432) -> Image.Image:
    """Badge centrato nella safe zone dell'icona Android adattiva."""
    badge = brand_emblem(round(size * 0.65))
    foreground = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    foreground.alpha_composite(badge, ((size - badge.width) // 2, (size - badge.height) // 2))
    return foreground


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
    em = brand_emblem(round((S - 2 * M) * 0.93))
    plate.alpha_composite(em, ((S - em.width) // 2, (S - em.height) // 2))
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
    em = brand_emblem(520)
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
    em = brand_emblem(150)
    img.alpha_composite(em, ((w - 150) // 2, 52))
    d = ImageDraw.Draw(img)
    d.text((w // 2, 228), "JARVIS", font=font(22, "Bold"), fill=(227, 244, 255), anchor="mm")
    d.text((w // 2, 252), "assistente personale", font=font(11, "Regular"), fill=(127, 155, 179), anchor="mm")
    d.line((30, 275, w - 30, 275), fill=CYAN + (120,), width=1)
    return img


def nsis_header(w: int = 150, h: int = 57) -> Image.Image:
    img = Image.new("RGBA", (w, h), (255, 255, 255, 255))
    plate = vgradient(70, h, NAVY_TOP, NAVY_BOTTOM).convert("RGBA")
    em = brand_emblem(50)
    plate.alpha_composite(em, (10, 3))
    img.paste(plate, (w - 70, 0))
    return img


def bmp(img: Image.Image, path: Path) -> None:
    img.convert("RGB").save(path, format="BMP")


def main() -> None:
    ICONS.mkdir(parents=True, exist_ok=True)
    INST.mkdir(parents=True, exist_ok=True)
    ANDROID_ICONS.mkdir(parents=True, exist_ok=True)
    mac = app_icon(1024, 100, shadow=True)
    full = app_icon(1024, 24, shadow=False)  # Windows e icona incorporata nella finestra.
    full.save(ICONS / "icon.png")
    mac.save(ICONS / "icon.icns", format="ICNS")
    full.save(ICONS / "icon.ico", format="ICO", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    for name, px in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)):
        full.resize((px, px), Image.LANCZOS).save(ICONS / name)
    android_launcher_emblem().save(ANDROID_ICONS / "ic_launcher_badge.png")
    dmg_background().convert("RGB").save(INST / "dmg-background.png")
    bmp(nsis_sidebar(), INST / "nsis-sidebar.bmp")
    bmp(nsis_header(), INST / "nsis-header.bmp")
    print(
        "risorse generate in",
        ICONS.relative_to(ROOT),
        INST.relative_to(ROOT),
        "e",
        ANDROID_ICONS.relative_to(ROOT),
    )


if __name__ == "__main__":
    main()
