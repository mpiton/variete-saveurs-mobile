#!/usr/bin/env python3
"""Derive the Android launcher icon from the official logo (templates/logo.png).

The logo is never regenerated (DESIGN.md §8): the VS monogram is cut out of it by
colour + connected components, the surrounding pastries are dropped, and the
silhouette is painted Creme Vitrine on a flat Rouge Enseigne background.

Usage: python3 tools/gen-launcher-icon.py
Requires Pillow. Writes android/res/**.
"""

from collections import deque
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROUGE = (0xC0, 0x18, 0x2B, 0xFF)
CREME = (0xF6, 0xF0, 0xE2, 0xFF)

# A strawberry is perched on the S and merges with it. The cut runs along the
# top edge of the letter's upper bowl, read off the logo column by column: the
# stroke starts at y=142 where it leaves the fruit at x=258 and rises to y=136
# at x=322. Above that line in this range is fruit, below is letterform — so the
# bowl, its counter and its terminal curl all survive intact.
ARC_TOP = ((258, 142), (322, 136))

SUPERSAMPLE = 4  # smooths the staircase left by the per-pixel mask
DENSITIES = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
FOREGROUND_DP = 108  # adaptive icon layer
LEGACY_DP = 48  # pre-API-26 launcher icon
FOREGROUND_SCALE = 0.58  # monogram width inside the 108dp layer (safe zone is 66%)
LEGACY_SCALE = 0.74  # monogram width inside the 48dp icon


def is_monogram_red(pixel):
    r, g, b, a = pixel
    # The VS strokes are a saturated near-neutral red; the chocolate cake shares
    # the r > g, r > b ratios but is much warmer, hence the green/blue balance.
    return a > 100 and r > 90 and r > g * 1.7 and r > b * 1.7 and abs(g - b) < 0.22 * r


def components(mask, w, h):
    seen = [[False] * h for _ in range(w)]
    found = []
    for x in range(w):
        for y in range(h):
            if not mask[x][y] or seen[x][y]:
                continue
            queue = deque([(x, y)])
            seen[x][y] = True
            cells = []
            while queue:
                cx, cy = queue.popleft()
                cells.append((cx, cy))
                for dx in (-1, 0, 1):
                    for dy in (-1, 0, 1):
                        nx, ny = cx + dx, cy + dy
                        if 0 <= nx < w and 0 <= ny < h and mask[nx][ny] and not seen[nx][ny]:
                            seen[nx][ny] = True
                            queue.append((nx, ny))
            found.append(cells)
    found.sort(key=len, reverse=True)
    return found


def monogram(logo_path):
    """Return the VS silhouette as a tightly cropped alpha mask."""
    logo = Image.open(logo_path).convert("RGBA")
    w, h = logo.size
    px = logo.load()
    mask = [[is_monogram_red(px[x, y]) for y in range(h)] for x in range(w)]

    # Keep the biggest red blob (VS + strawberry), then shave the fruit off.
    (x0, y0), (x1, y1) = ARC_TOP

    def is_fruit(x, y):
        if x < x0:
            return False
        along = min(1.0, (x - x0) / (x1 - x0))
        return y < y0 + (y1 - y0) * along

    kept = {p for p in components(mask, w, h)[0] if not is_fruit(*p)}
    shaved = [[(x, y) in kept for y in range(h)] for x in range(w)]
    out = Image.new("L", (w, h), 0)
    for x, y in components(shaved, w, h)[0]:
        out.putpixel((x, y), 255)

    # Supersample + blur + threshold to turn the ragged pixel edge into a curve.
    big = out.resize((w * SUPERSAMPLE, h * SUPERSAMPLE), Image.NEAREST)
    big = big.filter(ImageFilter.GaussianBlur(SUPERSAMPLE * 1.2))
    big = big.point(lambda v: 255 if v >= 128 else 0)
    return big.crop(big.getbbox())


def place(glyph, canvas_px, scale, background):
    icon = Image.new("RGBA", (canvas_px, canvas_px), (0, 0, 0, 0))
    if background is not None:
        icon.alpha_composite(background(canvas_px))
    width = round(canvas_px * scale)
    height = max(1, round(width * glyph.height / glyph.width))
    alpha = glyph.resize((width, height), Image.LANCZOS)
    layer = Image.new("RGBA", (width, height), CREME)
    layer.putalpha(alpha)
    icon.alpha_composite(layer, ((canvas_px - width) // 2, (canvas_px - height) // 2))
    return icon


def square(size):
    plate = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    radius = round(size * 0.18)
    ImageDraw.Draw(plate).rounded_rectangle([0, 0, size - 1, size - 1], radius, fill=ROUGE)
    return plate


def circle(size):
    plate = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(plate).ellipse([0, 0, size - 1, size - 1], fill=ROUGE)
    return plate


def main():
    root = Path(__file__).resolve().parent.parent
    res = root / "android" / "res"
    glyph = monogram(root / "templates" / "logo.png")

    for bucket, factor in DENSITIES.items():
        foreground = round(FOREGROUND_DP * factor)
        legacy = round(LEGACY_DP * factor)
        targets = [
            (f"drawable-{bucket}", "ic_launcher_vs_foreground.png",
             place(glyph, foreground, FOREGROUND_SCALE, None)),
            (f"mipmap-{bucket}", "ic_launcher_vs.png",
             place(glyph, legacy, LEGACY_SCALE, square)),
            (f"mipmap-{bucket}", "ic_launcher_vs_round.png",
             place(glyph, legacy, LEGACY_SCALE, circle)),
        ]
        for folder, name, image in targets:
            (res / folder).mkdir(parents=True, exist_ok=True)
            image.save(res / folder / name, optimize=True)
            print(f"{folder}/{name} {image.width}px")


if __name__ == "__main__":
    main()
