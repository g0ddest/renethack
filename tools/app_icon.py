#!/usr/bin/env python3
"""The game's icon: the hero's "@" in gold on the medallion that the
achievements wear: a gold rim lit from the top left round a dark field
(the same rim as medallion() in client/rust/renethack-gd/src/achievement_bake.rs).

Writes
  client/godot/icon.png            the window's icon (project.godot, config/icon)
  client/godot/icon.icns, .ico     the native icons of macOS and Windows; they
                                   are Steamworks' client icons for both too
  steam/icon/client-linux.zip      Steamworks' client icon for Linux (PNGs)
  steam/icon/community.jpg         and its community icon (184x184)

The small sizes (48 px and under) are drawn again with a wider rim and a
bigger sign, not scaled down. Needs Pillow; `make app-icon` runs it.
"""

import io
import math
import sys
import zipfile
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "client/godot/fonts/alegreya-sans/AlegreyaSans-Black.ttf"
SIGN = "@"

# the medallion's colours (achievement_bake.rs) and the title's ink
GOLD_DIM = (0x6E, 0x53, 0x28)
GOLD = (0xB8, 0x89, 0x3B)
GOLD_BRIGHT = (0xF0, 0xD4, 0x8C)
FIELD_MID = (0x33, 0x27, 0x1C)
FIELD_EDGE = (0x0B, 0x08, 0x06)
DARK = (0x12, 0x0C, 0x07)
INK = (0x0B, 0x09, 0x08)
# the light comes from the top left
LIGHT = (-0.6, -0.8)

# glyph layers are drawn this many times larger and scaled down
OVER = 3


def clamp01(v):
    return 0.0 if v < 0.0 else 1.0 if v > 1.0 else v


def mix(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)


def medallion(side, radius, rim_share):
    """The rim and the field, `radius` pixels round the middle of a
    `side` x `side` picture; the rim takes `rim_share` of the side.
    Returns the picture and the field's radius."""
    n = float(side)
    c = n / 2.0
    outer = radius
    rim = n * rim_share
    inner = outer - rim
    px = bytearray(side * side * 4)
    for y in range(side):
        fy = y + 0.5
        dy = fy - c
        row = y * side * 4
        for x in range(side):
            dx = x + 0.5 - c
            d = math.sqrt(dx * dx + dy * dy)
            cover = clamp01(outer - d + 0.5)
            if cover <= 0.0:
                continue
            if d > 0.0:
                facing = (dx * LIGHT[0] + dy * LIGHT[1]) / d
            else:
                facing = 0.0
            # the field: darker toward its edge and in the rim's shadow
            t = clamp01(d / inner)
            field = mix(FIELD_MID, FIELD_EDGE, t**1.6)
            shadow = clamp01((d - (inner - n * 0.07)) / (n * 0.07))
            k = 1.0 - 0.55 * shadow * (0.6 - 0.4 * facing)
            field = (field[0] * k, field[1] * k, field[2] * k)
            # the rim: a rounded ridge, its upper left catching the light
            u = clamp01((d - inner) / rim)
            ridge = 1.0 - abs(2.0 * u - 1.0)
            lit = clamp01(0.5 + 0.5 * facing * (1.0 - 2.0 * u))
            metal = mix(GOLD_DIM, GOLD, clamp01(ridge * 1.2))
            metal = mix(metal, GOLD_BRIGHT, lit * ridge * 0.85)
            line = n / 512.0 + 0.7
            lines = max(clamp01(line - abs(d - inner)), clamp01(line - abs(d - outer + 0.6)))
            metal = mix(metal, DARK, lines * 0.85)
            col = mix(field, metal, clamp01(d - inner + 0.5))
            i = row + x * 4
            px[i] = int(col[0] + 0.5)
            px[i + 1] = int(col[1] + 0.5)
            px[i + 2] = int(col[2] + 0.5)
            px[i + 3] = int(cover * 255.0 + 0.5)
    return Image.frombytes("RGBA", (side, side), bytes(px)), inner


def shifted(mask, dx, dy):
    """`mask` moved by (dx, dy), empty where nothing moved in."""
    out = Image.new("L", mask.size, 0)
    out.paste(mask, (int(round(dx)), int(round(dy))))
    return out


def tinted(mask, colour, strength=1.0):
    """A layer of one colour whose alpha is `mask` times `strength`."""
    layer = Image.new("RGBA", mask.size, colour + (0,))
    alpha = mask if strength == 1.0 else mask.point(lambda v: int(v * strength + 0.5))
    layer.putalpha(alpha)
    return layer


def sign_layers(side, field_radius, height_share, small):
    """The sign on a clear picture of `side` pixels: its glow, shadow, dark
    edge, gold body and bevel, kept inside the field."""
    big = side * OVER
    c = big / 2.0
    # the size at which the sign's ink is `height_share` of the side tall
    probe = ImageFont.truetype(str(FONT), 1000)
    l, t, r, b = probe.getbbox(SIGN)
    size = int(round(1000.0 * height_share * big / (b - t)))
    font = ImageFont.truetype(str(FONT), size)
    l, t, r, b = font.getbbox(SIGN)
    # the sign's ink is heavier on the left (its tail opens to the right):
    # a little to the right of the box's middle looks centred
    at = (c - (l + r) / 2.0 + big * 0.004, c - (t + b) / 2.0 - big * 0.006)

    def glyph(stroke=0):
        m = Image.new("L", (big, big), 0)
        ImageDraw.Draw(m).text(at, SIGN, font=font, fill=255, stroke_width=stroke, stroke_fill=255)
        return m

    body = glyph()
    edge = glyph(max(2, int(big * (0.012 if small else 0.0075))))
    out = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    if not small:
        # a room's floor as the game's map draws it: a dot to a square,
        # the squares taller than wide, fading toward the rim
        dots = Image.new("L", (big, big), 0)
        draw = ImageDraw.Draw(dots)
        px, py, r = big * 0.066, big * 0.088, big * 0.006
        reach = field_radius * OVER
        for j in range(-8, 9):
            for i in range(-8, 9):
                x, y = c + i * px, c + j * py
                d = math.hypot(x - c, y - c) / reach
                if d > 0.86:
                    continue
                v = int(255 * (1.0 - d**2.4))
                draw.ellipse([x - r, y - r, x + r, y + r], fill=v)
        # none under the sign or close round it
        near = edge.filter(ImageFilter.GaussianBlur(big * 0.02)).point(lambda v: min(255, v * 3))
        dots = ImageChops.multiply(dots, ImageChops.invert(near))
        out.alpha_composite(tinted(dots, (0xA8, 0x86, 0x4C), 0.68))
        # a fine line of gold inside the rim
        ring = Image.new("L", (big, big), 0)
        rr, w = reach - big * 0.022, max(2, int(big * 0.0042))
        ImageDraw.Draw(ring).ellipse([c - rr, c - rr, c + rr, c + rr], outline=255, width=w)
        out.alpha_composite(tinted(ring, GOLD_DIM, 0.9))
        # torchlight on the field behind the sign
        glow = edge.filter(ImageFilter.GaussianBlur(big * 0.045))
        out.alpha_composite(tinted(glow, (0xD9, 0x8A, 0x2E), 0.34))
    # the shadow the sign casts on the field, away from the light
    cast = shifted(edge, big * 0.012, big * 0.018).filter(ImageFilter.GaussianBlur(big * 0.012))
    out.alpha_composite(tinted(cast, (0, 0, 0), 0.8))
    out.alpha_composite(tinted(edge, DARK, 0.96))
    # the body: bright gold at the top, deep at the foot
    ramp = Image.new("RGBA", (big, big))
    top, foot = at[1] + t, at[1] + b
    draw = ImageDraw.Draw(ramp)
    for y in range(big):
        u = clamp01((y - top) / max(1.0, foot - top))
        if u < 0.45:
            col = mix(GOLD_BRIGHT, (0xD6, 0xAA, 0x52), u / 0.45)
        else:
            col = mix((0xD6, 0xAA, 0x52), (0x96, 0x6C, 0x28), (u - 0.45) / 0.55)
        draw.line([(0, y), (big, y)], fill=tuple(int(v + 0.5) for v in col) + (255,))
    ramp.putalpha(body)
    out.alpha_composite(ramp)
    # the bevel: the edge toward the light shines, the far edge is in shade
    e = big * (0.007 if small else 0.0055)
    soft = ImageFilter.GaussianBlur(big * 0.0022)
    shine = ImageChops.subtract(body, shifted(body, e, e * 1.3)).filter(soft)
    shade = ImageChops.subtract(body, shifted(body, -e, -e * 1.3)).filter(soft)
    shine = ImageChops.multiply(shine, body)
    shade = ImageChops.multiply(shade, body)
    out.alpha_composite(tinted(shade, (0x4A, 0x30, 0x0E), 0.78))
    out.alpha_composite(tinted(shine, (0xFF, 0xF2, 0xC8), 0.82))
    # nothing of the sign lies on the rim
    keep = Image.new("L", (big, big), 0)
    rr = (field_radius - 1.0) * OVER
    ImageDraw.Draw(keep).ellipse([c - rr, c - rr, c + rr, c + rr], fill=255)
    out.putalpha(ImageChops.multiply(out.getchannel("A"), keep))
    return out.resize((side, side), Image.LANCZOS)


def icon(side, fill=0.97, small=None, shadow=False):
    """The icon at `side` pixels; the medallion takes `fill` of the side.
    `shadow`: a soft shadow under it (macOS draws icons standing out)."""
    if small is None:
        small = side <= 48
    # the smaller the picture, the more of it the sign takes
    if side <= 20:
        rim, height = 0.08, 0.64
    elif small:
        rim, height = 0.09, 0.60
    else:
        rim, height = 0.075, 0.50
    radius = side * fill / 2.0 - (0.5 if small else 1.5 * side / 256.0)
    base, field_radius = medallion(side, radius, rim)
    sign = sign_layers(side, field_radius, height * fill, small)
    base.alpha_composite(sign)
    if not shadow:
        return base
    under = Image.new("L", (side, side), 0)
    r = radius
    c = side / 2.0
    ImageDraw.Draw(under).ellipse([c - r, c - r + side * 0.012, c + r, c + r + side * 0.012], fill=255)
    under = under.filter(ImageFilter.GaussianBlur(side * 0.014))
    out = tinted(under, (0, 0, 0), 0.45)
    out.alpha_composite(base)
    return out


def write_ico(path, sizes=(16, 24, 32, 48, 64, 128, 256)):
    images = [icon(s) for s in sizes]
    images[-1].save(path, format="ICO", sizes=[(s, s) for s in sizes], append_images=images[:-1])


def write_icns(path):
    # macOS keeps an icon's shape inside its grid, with a shadow under it
    sizes = (16, 32, 64, 128, 256, 512, 1024)
    images = [icon(s, fill=0.86, shadow=True) for s in sizes]
    images[-1].save(path, format="ICNS", append_images=images[:-1])


def write_linux_zip(path, sizes=(16, 24, 32, 48, 64, 96, 128, 256)):
    # the same bytes on every run: fixed dates, stored order
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for s in sizes:
            data = io.BytesIO()
            icon(s).save(data, format="PNG")
            info = zipfile.ZipInfo(f"renethack_{s}.png", date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, data.getvalue())


def write_community(path, side=184):
    # a JPEG has no clear pixels: the medallion on the title's ink
    back = Image.new("RGBA", (side, side), INK + (255,))
    back.alpha_composite(icon(side, fill=0.94))
    back.convert("RGB").save(path, format="JPEG", quality=92)


def main():
    godot = ROOT / "client/godot"
    steam = ROOT / "steam/icon"
    steam.mkdir(parents=True, exist_ok=True)
    icon(512).save(godot / "icon.png", format="PNG")
    write_ico(godot / "icon.ico")
    write_icns(godot / "icon.icns")
    write_linux_zip(steam / "client-linux.zip")
    write_community(steam / "community.jpg")
    print("app icon: client/godot/icon.{png,icns,ico}, steam/icon/")


if __name__ == "__main__":
    sys.exit(main())
