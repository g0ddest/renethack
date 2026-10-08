"""Textures painted from those fetched (tools/fetch_art.py runs it: the
"painted" recipe in client/godot/art/sources.json): a corpse's skin from a
living one, linen wound round a body, a coat left pale to take a tint.

Everything is drawn from a seed: a run paints what the last one did. Only
Pillow is needed.
"""
import random

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageOps


def noise(rng, size, cells):
    """Smooth noise (0..255): random values on a grid of `cells`, stretched
    over `size`."""
    grid = Image.new("L", (cells, cells))
    grid.putdata([rng.randrange(256) for _ in range(cells * cells)])
    return grid.resize(size, Image.BICUBIC)


def above(image, level, soft):
    """Where `image` is brighter than `level`: a mask, its edge `soft`
    levels wide."""
    return image.point(lambda v: max(0, min(255, round((v - level) * 255 / soft))))


def linen(skin):
    """Where the underwear painted on a base character's skin lies: it is
    far bluer than any skin."""
    r, _, b = skin.split()
    return above(ImageChops.subtract(b, r.point(lambda v: round(v * 0.75))), 0, 1)


def sockets(size, eyes, radius, blur):
    """A soft mask of the eyes' hollows; `eyes` and `radius` in shares of
    the image."""
    w, h = size
    mask = Image.new("L", size, 0)
    d = ImageDraw.Draw(mask)
    for x, y in eyes:
        d.ellipse([(x - radius) * w, (y - radius * 0.8) * h, (x + radius) * w, (y + radius * 0.8) * h],
                  fill=255)
    return mask.filter(ImageFilter.GaussianBlur(blur * w))


def rot(skin, eyes):
    """A corpse's skin from a living one: grey-green and mottled, with
    bruises and dark sores, the eyes sunken, its linen filthy."""
    rng = random.Random("rot")
    skin = skin.convert("RGB")
    size = skin.size
    cloth = linen(skin)
    # the living skin's light and dark, in a corpse's colours
    out = ImageOps.colorize(ImageOps.autocontrast(ImageOps.grayscale(skin), cutoff=1),
                            black="#1a1c14", mid="#6d7658", white="#b9c09c")
    # mottled: lighter and darker in patches
    mottle = noise(rng, size, 20).point(lambda v: 150 + v * 105 // 255)
    out = ImageChops.multiply(out, Image.merge("RGB", [mottle] * 3))
    for cells, level, colour, strength in ((9, 160, "#3b3344", 0.45),    # bruises
                                           (46, 196, "#2a1410", 0.85)):  # sores
        mask = above(noise(rng, size, cells), level, 26).point(lambda v: round(v * strength))
        out = Image.composite(Image.new("RGB", size, colour), out, mask)
    out = Image.composite(Image.new("RGB", size, "#0e0c0a"), out,
                          sockets(size, eyes, 0.03, 0.006).point(lambda v: v * 9 // 10))
    return Image.composite(Image.new("RGB", size, "#2c2a20"), out, cloth)


def wraps(size, eyes=()):
    """Linen wound round a body: strips across the image, each lapping the
    one before, dark where they part; stained, and open over the `eyes`."""
    rng = random.Random("wraps")
    w, h = size
    colour = Image.new("RGB", size, "#1e1912")
    dc = ImageDraw.Draw(colour)
    xs = [w * i / 8 for i in range(9)]

    def strip(y, thick, slope):
        """A strip from edge to edge, sagging a little; lit along its
        upper edge, in shadow along the lower."""
        sag = [rng.uniform(-1, 1) * h * 0.004 for _ in xs]
        top = [(x, y + slope * (x - w / 2) + s) for x, s in zip(xs, sag)]
        shade = rng.uniform(0.68, 1.0)
        tone = tuple(round(c * shade) for c in (188, 168, 126))
        low = [(x, ty + thick) for x, ty in reversed(top)]
        dc.polygon(top + low, fill=tone)
        edge = max(2, h // 200)
        dc.line([(x, ty + thick - edge / 2) for x, ty in top], fill=tuple(c * 4 // 9 for c in tone), width=edge)

    y = -h * 0.03
    while y < h * 1.03:
        thick = h * rng.uniform(0.03, 0.046)
        strip(y, thick, rng.uniform(-0.035, 0.035))
        # most turns lap the last; some leave the dark between them
        y += thick * (rng.uniform(1.05, 1.22) if rng.random() < 0.4 else rng.uniform(0.8, 0.96))
    for _ in range(7):
        # a turn wound aslant over the rest
        strip(rng.uniform(0.1, 0.9) * h, h * rng.uniform(0.028, 0.04), rng.choice((-1, 1)) * rng.uniform(0.15, 0.4))
    # the weave, and what has seeped through
    weave = noise(rng, size, 300).point(lambda v: 222 + v * 33 // 255)
    colour = ImageChops.multiply(colour, Image.merge("RGB", [weave] * 3))
    stains = above(noise(rng, size, 11), 140, 60).point(lambda v: v // 2)
    colour = Image.composite(Image.new("RGB", size, "#5c452a"), colour, stains)
    if eyes:
        # left open from eye to eye
        (ax, ay), (bx, by) = min(eyes), max(eyes)
        slit = Image.new("L", size, 0)
        r = 0.02
        ImageDraw.Draw(slit).rounded_rectangle([(ax - r * 1.3) * w, (ay - r * 0.7) * h, (bx + r * 1.3) * w, (by + r * 0.7) * h],
                                               radius=r * 0.7 * h, fill=255)
        slit = slit.filter(ImageFilter.GaussianBlur(0.003 * w))
        colour = Image.composite(Image.new("RGB", size, "#0a0806"), colour, slit)
    return colour


def pale(coat, light):
    """A coat without its colour, as `light` (0..1) on the whole as asked:
    a tint multiplied in then shows as itself."""
    grey = ImageOps.grayscale(coat.convert("RGB"))
    mean = sum(i * n for i, n in enumerate(grey.histogram())) / (grey.width * grey.height)
    gain = light * 255 / mean
    return Image.merge("RGB", [grey.point(lambda v: min(255, round(v * gain)))] * 3)
