#!/usr/bin/env python3
"""The game's icon, after NetHack's own (win/X11/nh_icon.xpm and the
Windows icon): a shield with a scalloped top and a blue field, lit from the
left, before two crossed swords, hilts up. Drawn afresh in the game's steel
and gold, with the hero's "@" as the shield's device.

Writes
  client/godot/icon.png            the window's icon (project.godot, config/icon)
  client/godot/icon.icns, .ico     the native icons of macOS and Windows; they
                                   are Steamworks' client icons for both too
  steam/icon/client-linux.zip      Steamworks' client icon for Linux (PNGs)
  steam/icon/community.jpg         and its community icon (184x184)

The sizes of 64 pixels and under are drawn again, bolder and plainer, not
scaled down. Needs Pillow; `make app-icon` runs it, the same bytes every run.
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

GOLD_DIM = (0x7A, 0x5A, 0x24)
GOLD = (0xC2, 0x94, 0x40)
GOLD_BRIGHT = (0xF2, 0xD8, 0x90)
STEEL_BRIGHT = (0xF2, 0xF5, 0xF8)
STEEL = (0xB2, 0xBB, 0xC6)
STEEL_DIM = (0x66, 0x70, 0x7E)
STEEL_DARK = (0x30, 0x36, 0x40)
# NetHack's icon has a blue field going to dark teal on the shaded side
FIELD_LIT = (0x7C, 0xA6, 0xD0)
FIELD = (0x4E, 0x7C, 0xAE)
FIELD_SHADE = (0x2C, 0x56, 0x64)
FIELD_DEEP = (0x18, 0x34, 0x40)
LEATHER = (0x3C, 0x28, 0x1A)
DARK = (0x0E, 0x0B, 0x09)
INK = (0x0B, 0x09, 0x08)


def clamp01(v):
    return 0.0 if v < 0.0 else 1.0 if v > 1.0 else v


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def rgb(c):
    return tuple(int(v + 0.5) for v in c)


class Canvas:
    """A picture drawn `over` times larger than it comes out; shapes are
    given in units of the picture's side, from its top left."""

    def __init__(self, side, over, bold):
        self.side = side
        self.n = side * over
        # bold: the small sizes' wider edges and plainer shapes (1 at 64
        # pixels and under, 2 at 24 and under)
        self.bold = bold
        self.out = Image.new("RGBA", (self.n, self.n), (0, 0, 0, 0))

    def px(self, v):
        return v * self.n

    def mask(self):
        return Image.new("L", (self.n, self.n), 0)

    def polygon(self, points):
        m = self.mask()
        ImageDraw.Draw(m).polygon([(self.px(x), self.px(y)) for x, y in points], fill=255)
        return m

    def disc(self, cx, cy, r):
        m = self.mask()
        box = [self.px(cx - r), self.px(cy - r), self.px(cx + r), self.px(cy + r)]
        ImageDraw.Draw(m).ellipse(box, fill=255)
        return m

    def blur(self, m, r):
        return m.filter(ImageFilter.GaussianBlur(max(0.3, self.px(r))))

    def grown(self, m, r):
        """`m` wider by `r` all round (narrower when negative)."""
        soft = self.blur(m, abs(r) * 0.6)
        if r >= 0:
            return soft.point(lambda v: 255 if v > 26 else 0)
        return soft.point(lambda v: 255 if v > 229 else 0)

    def shifted(self, m, dx, dy):
        out = Image.new(m.mode, m.size, 0)
        out.paste(m, (int(round(self.px(dx))), int(round(self.px(dy)))))
        return out

    def paint(self, m, colour, strength=1.0):
        layer = Image.new("RGBA", (self.n, self.n), rgb(colour) + (0,))
        layer.putalpha(m if strength == 1.0 else m.point(lambda v: int(v * strength + 0.5)))
        self.out.alpha_composite(layer)

    def fill(self, m, picture):
        picture = picture.copy()
        picture.putalpha(m)
        self.out.alpha_composite(picture)

    def ramp(self, a, b, x0, y0, x1, y1):
        """A picture going from colour `a` at (x0, y0) to `b` at (x1, y1)."""
        small = 64
        im = Image.new("RGB", (small, small))
        px = im.load()
        dx, dy = x1 - x0, y1 - y0
        d2 = dx * dx + dy * dy
        for j in range(small):
            for i in range(small):
                u, v = (i + 0.5) / small, (j + 0.5) / small
                t = clamp01(((u - x0) * dx + (v - y0) * dy) / d2)
                px[i, j] = rgb(mix(a, b, t))
        return im.resize((self.n, self.n), Image.BICUBIC).convert("RGBA")

    def relief(self, m, soft, depth, shine, shade, k=1.0):
        """Light on the edges of `m` that face the top left, shade on those
        that face away: the mask rounded by `soft`, lit across `depth`."""
        b = self.blur(m, soft)
        moved = self.shifted(b, depth * 0.6, depth * 0.8)
        lit = ImageChops.multiply(ImageChops.subtract(b, moved), m)
        dim = ImageChops.multiply(ImageChops.subtract(moved, b), m)
        self.paint(dim, shade, 0.9 * k)
        self.paint(lit, shine, 0.95 * k)

    def cast(self, m, dx, dy, soft, strength, onto=None):
        """The shadow `m` throws, on what is drawn already (or on `onto`)."""
        s = self.blur(self.shifted(m, dx, dy), soft)
        if onto is not None:
            s = ImageChops.multiply(s, onto)
        self.paint(s, (0, 0, 0), strength)

    def result(self):
        return self.out.resize((self.side, self.side), Image.LANCZOS)


def shield_outline(cx, top, width, height, steps=48):
    """NetHack's shield: pointed shoulders, a raised middle, two scallops
    between them at the top; straight sides, then round to a point."""
    half = width / 2.0
    pts = []
    # the top, left to right: shoulder, scallop, crest, scallop, shoulder
    dip = height * 0.105
    crest = half * 0.40
    for i in range(steps + 1):
        u = -1.0 + 2.0 * i / steps
        a = abs(u) * half
        if a <= crest:
            # the crest: nearly flat, a little arched
            y = top + dip * 0.10 * (a / crest) ** 2
        else:
            # a scallop from the crest's end down and up to the shoulder
            t = (a - crest) / (half - crest)
            y = top + dip * math.sin(math.pi * t) ** 0.75 * (1.0 - 0.10 * t)
        pts.append((cx + u * half, y))
    # the right side down to the point, then the left side up
    straight = top + height * 0.36
    foot = top + height
    side = []
    for i in range(1, steps + 1):
        t = i / steps
        y = straight + (foot - straight) * t
        w = half * math.cos(t * math.pi / 2.0) ** 0.72
        side.append((w, y))
    pts.append((cx + half, straight))
    pts.extend((cx + w, y) for w, y in side)
    pts.extend((cx - w, y) for w, y in reversed(side[:-1]))
    pts.append((cx - half, straight))
    return pts


def sword(c, hilt, tip):
    """A sword from its pommel at `hilt` to its point at `tip`."""
    (x0, y0), (x1, y1) = hilt, tip
    length = math.hypot(x1 - x0, y1 - y0)
    tx, ty = (x1 - x0) / length, (y1 - y0) / length
    nx, ny = -ty, tx
    bold = c.bold

    def at(along, across):
        return (x0 + tx * along + nx * across, y0 + ty * along + ny * across)

    k = (1.0, 1.3, 1.75)[bold]
    pommel_r = 0.030 * k
    grip_w = 0.032 * k
    guard_at = 0.135
    guard_len = 0.185 * k
    guard_w = 0.034 * k
    blade_w = 0.060 * k
    point_len = 0.085
    edge = (0.0065, 0.011, 0.017)[bold]

    shoulder = length - point_len
    blade_m = c.polygon(
        [
            at(guard_at, blade_w / 2),
            at(shoulder, blade_w * 0.42),
            at(length, 0.0),
            at(shoulder, -blade_w * 0.42),
            at(guard_at, -blade_w / 2),
        ]
    )
    grip_m = c.polygon(
        [at(0.0, grip_w / 2), at(guard_at, grip_w / 2), at(guard_at, -grip_w / 2), at(0.0, -grip_w / 2)]
    )
    guard_m = c.polygon(
        [
            at(guard_at - guard_w / 2, guard_len / 2),
            at(guard_at + guard_w / 2, guard_len / 2),
            at(guard_at + guard_w / 2, -guard_len / 2),
            at(guard_at - guard_w / 2, -guard_len / 2),
        ]
    )
    for s in (1, -1):
        gx, gy = at(guard_at, s * guard_len / 2)
        guard_m = ImageChops.lighter(guard_m, c.disc(gx, gy, guard_w * 0.72))
    pommel_m = c.disc(x0, y0, pommel_r)
    whole = ImageChops.lighter(ImageChops.lighter(blade_m, grip_m), ImageChops.lighter(guard_m, pommel_m))

    c.cast(whole, 0.010, 0.014, 0.010, 0.55)
    c.paint(c.grown(whole, edge), DARK)
    # the blade: two flats, the one toward the light bright
    lit_side = 1 if (nx * -0.6 + ny * -0.8) > 0 else -1
    for s in (1, -1):
        flat = c.polygon(
            [
                at(guard_at, 0.0),
                at(guard_at, s * blade_w / 2),
                at(shoulder, s * blade_w * 0.42),
                at(length, 0.0),
            ]
        )
        if s == lit_side:
            c.fill(flat, c.ramp(STEEL_BRIGHT, STEEL, x0, y0, x1, y1))
        else:
            c.fill(flat, c.ramp(STEEL_DIM, STEEL_DARK, x0, y0, x1, y1))
    if not bold:
        end = length - point_len * 0.4
        ridge = c.polygon([at(guard_at, 0.004), at(end, 0.0015), at(end, -0.0015), at(guard_at, -0.004)])
        c.paint(ridge, STEEL_BRIGHT, 0.75)
    # the grip: leather, wound with wire
    c.paint(grip_m, LEATHER)
    if not bold:
        wires = c.mask()
        d = ImageDraw.Draw(wires)
        a = pommel_r * 1.1
        while a < guard_at - guard_w * 0.6:
            p, q = at(a, grip_w / 2), at(a + 0.006, -grip_w / 2)
            d.line(
                [c.px(p[0]), c.px(p[1]), c.px(q[0]), c.px(q[1])],
                fill=255,
                width=max(1, int(c.px(0.0045))),
            )
            a += 0.016
        c.paint(ImageChops.multiply(wires, grip_m), GOLD, 0.8)
    c.relief(grip_m, 0.004, 0.006, (0x8A, 0x62, 0x3C), (0x12, 0x0A, 0x06))
    # the guard and the pommel: gold
    for part in (guard_m, pommel_m):
        c.fill(part, c.ramp(GOLD_BRIGHT, GOLD_DIM, x0 - 0.05, y0 - 0.08, x0 + 0.12, y0 + 0.14))
        c.relief(part, 0.0045, 0.008, (0xFF, 0xF4, 0xD0), (0x3C, 0x26, 0x08))


def shield(c, device):
    bold = c.bold
    cx = 0.5
    top, width, height = ((0.195, 0.530, 0.705), (0.175, 0.580, 0.745), (0.165, 0.620, 0.770))[bold]
    body = c.polygon(shield_outline(cx, top, width, height))
    rim_w = (0.040, 0.060, 0.082)[bold]
    inner = c.grown(body, -rim_w)
    rim = ImageChops.subtract(body, inner)
    edge = (0.0075, 0.013, 0.020)[bold]

    c.cast(body, 0.014, 0.020, 0.016, 0.6)
    c.paint(c.grown(body, edge), DARK)
    # the rim: steel, round, lit from the top left
    c.fill(rim, c.ramp(STEEL_BRIGHT, STEEL_DIM, 0.22, 0.12, 0.80, 0.86))
    c.relief(rim, rim_w * 0.22, rim_w * 0.55, (0xFF, 0xFF, 0xFF), (0x1E, 0x24, 0x2C))
    # the field: blue toward the light, dark teal away from it
    left, right = cx - width / 2, cx + width / 2
    c.fill(inner, c.ramp(FIELD, FIELD_SHADE, left + width * 0.30, 0.5, right - width * 0.18, 0.5))
    glow = c.blur(c.disc(cx - width * 0.20, top + height * 0.24, width * 0.26), 0.11)
    c.paint(ImageChops.multiply(glow, inner), FIELD_LIT, 0.62)
    dusk = c.blur(c.disc(cx + width * 0.30, top + height * 0.74, width * 0.32), 0.12)
    c.paint(ImageChops.multiply(dusk, inner), FIELD_DEEP, 0.8)
    if not bold:
        # the old icon's dither, as a fine lattice in the paint
        lattice = c.mask()
        d = ImageDraw.Draw(lattice)
        pitch, w = c.px(0.030), max(1, int(c.px(0.0022)))
        k = -c.n
        while k < 2 * c.n:
            d.line([k, 0, k + c.n, c.n], fill=255, width=w)
            d.line([k + c.n, 0, k, c.n], fill=255, width=w)
            k += pitch
        lattice = ImageChops.multiply(lattice, c.grown(inner, -0.012))
        c.paint(c.shifted(lattice, 0.0015, 0.002), (0x0A, 0x1C, 0x26), 0.30)
        c.paint(lattice, (0xC8, 0xE0, 0xF4), 0.13)
    # the rim stands above the field: its shadow inside the lit edges, a
    # line of light inside the far ones
    lip = ImageChops.multiply(ImageChops.subtract(inner, c.shifted(inner, 0.016, 0.022)), inner)
    c.paint(c.blur(lip, 0.006), (0x04, 0x0C, 0x14), 0.75)
    far = ImageChops.multiply(ImageChops.subtract(inner, c.shifted(inner, -0.006, -0.008)), inner)
    c.paint(c.blur(far, 0.002), FIELD_LIT, 0.45)
    c.paint(ImageChops.subtract(c.grown(inner, 0.004), inner), DARK, 0.7)
    if not bold:
        # rivets on the rim
        spots = ((-0.385, 0.17), (0.385, 0.17), (-0.43, 0.42), (0.43, 0.42), (-0.30, 0.66), (0.30, 0.66), (0.0, 0.925))
        for u, v in spots:
            x, y = cx + u * width, top + v * height
            stud = c.disc(x, y, 0.0085)
            c.cast(stud, 0.002, 0.003, 0.002, 0.6, rim)
            c.fill(stud, c.ramp(STEEL_BRIGHT, STEEL_DIM, x - 0.01, y - 0.012, x + 0.01, y + 0.012))
    if device:
        sign(c, cx, top + height * 0.415, (0.30, 0.36, 0.36)[bold], inner)


def sign(c, cx, cy, height, within):
    """The hero's "@" in gold, raised on the field."""
    probe = ImageFont.truetype(str(FONT), 1000)
    l, t, r, b = probe.getbbox(SIGN)
    font = ImageFont.truetype(str(FONT), int(round(1000.0 * height * c.n / (b - t))))
    l, t, r, b = font.getbbox(SIGN)
    at = (c.px(cx) - (l + r) / 2.0 + c.px(0.004), c.px(cy) - (t + b) / 2.0)

    def glyph(stroke=0):
        m = c.mask()
        ImageDraw.Draw(m).text(at, SIGN, font=font, fill=255, stroke_width=stroke, stroke_fill=255)
        return m

    body = glyph()
    edge = glyph(max(1, int(c.px(0.010 if c.bold else 0.0062))))
    c.cast(edge, 0.008, 0.012, 0.008, 0.7, within)
    c.paint(ImageChops.multiply(edge, within), DARK, 0.95)
    top, foot = cy - height / 2, cy + height / 2
    c.fill(body, c.ramp(GOLD_BRIGHT, (0x9A, 0x70, 0x28), 0.5, top, 0.5, foot))
    c.relief(body, 0.0022, 0.0075 if c.bold else 0.0058, (0xFF, 0xF4, 0xCC), (0x4A, 0x30, 0x0E))


def icon(side, fill=1.0, bold=None, shadow=False, device=None):
    """The icon at `side` pixels. `fill`: how much of the side the picture
    takes; `shadow`: a soft shadow under it (macOS draws icons standing out)."""
    if bold is None:
        bold = 2 if side <= 24 else 1 if side <= 64 else 0
    if device is None:
        # under 32 pixels the sign is a smudge: the shield alone
        device = side >= 32
    over = 8 if side <= 128 else 4 if side <= 512 else 2
    c = Canvas(side, over, bold)
    if bold:
        sword(c, (0.105, 0.100), (0.915, 0.915))
        sword(c, (0.895, 0.100), (0.085, 0.915))
    else:
        sword(c, (0.085, 0.085), (0.925, 0.925))
        sword(c, (0.915, 0.085), (0.075, 0.925))
    shield(c, device)
    pic = c.result()
    if fill == 1.0 and not shadow:
        return pic
    inner = max(1, int(round(side * fill)))
    small = pic.resize((inner, inner), Image.LANCZOS) if inner != side else pic
    out = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    at = ((side - inner) // 2, (side - inner) // 2)
    if shadow:
        under = Image.new("L", (side, side), 0)
        under.paste(small.getchannel("A"), (at[0], at[1] + max(1, int(side * 0.012))))
        under = under.filter(ImageFilter.GaussianBlur(max(0.5, side * 0.014)))
        dark = Image.new("RGBA", (side, side), (0, 0, 0, 0))
        dark.putalpha(under.point(lambda v: int(v * 0.5)))
        out.alpha_composite(dark)
    out.alpha_composite(small, at)
    return out


def write_ico(path, sizes=(16, 24, 32, 48, 64, 128, 256)):
    images = [icon(s) for s in sizes]
    images[-1].save(path, format="ICO", sizes=[(s, s) for s in sizes], append_images=images[:-1])


def write_icns(path):
    # macOS keeps an icon's shape inside its grid, with a shadow under it
    sizes = (16, 32, 64, 128, 256, 512, 1024)
    images = [icon(s, fill=0.90, shadow=True) for s in sizes]
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
    # a JPEG has no clear pixels: the icon on the title's ink
    back = Image.new("RGBA", (side, side), INK + (255,))
    back.alpha_composite(icon(side, fill=0.92))
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
