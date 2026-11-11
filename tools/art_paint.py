"""Textures painted from those fetched (tools/fetch_art.py runs it: the
"painted" recipe in client/godot/art/sources.json): a corpse's skin from a
living one, a lich's dried over its skull, linen wound round a body, a coat
left pale to take a tint, a dark coat lit up; granite over a boulder, marble,
a rug's pile.

Everything is drawn from a seed: a run paints what the last one did. Only
Pillow is needed.
"""
import math
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


def bone(skin, eyes, nose):
    """A lich's skin from a living one: dried tight over the skull, the
    colour of old bone, the eyes black hollows, the cheeks sunken under
    their bones and the nose a hole (`nose`: where it lies in the image)."""
    skin = skin.convert("RGB")
    size = skin.size
    w, h = size
    cloth = linen(skin)
    out = ImageOps.colorize(ImageOps.autocontrast(ImageOps.grayscale(skin), cutoff=1),
                            black="#2e281c", mid="#a49a7e", white="#ece4cc")
    hollows = Image.new("L", size, 0)
    d = ImageDraw.Draw(hollows)
    (ax, ay), (bx, by) = min(eyes), max(eyes)
    for x, y in ((ax - 0.006, ay + 0.052), (bx + 0.006, by + 0.052)):
        d.ellipse([(x - 0.026) * w, (y - 0.018) * h, (x + 0.026) * w, (y + 0.018) * h], fill=210)
    out = Image.composite(Image.new("RGB", size, "#3e3628"), out, hollows.filter(ImageFilter.GaussianBlur(0.008 * w)))
    out = Image.composite(Image.new("RGB", size, "#050403"), out, sockets(size, eyes, 0.036, 0.005))
    nx, ny = nose
    hole = Image.new("L", size, 0)
    ImageDraw.Draw(hole).polygon([((nx - 0.013) * w, (ny + 0.012) * h), ((nx + 0.013) * w, (ny + 0.012) * h),
                                  (nx * w, (ny - 0.02) * h)], fill=255)
    out = Image.composite(Image.new("RGB", size, "#0a0806"), out, hole.filter(ImageFilter.GaussianBlur(0.002 * w)))
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


def lift(coat, light):
    """A dark coat lit up: every colour raised by one curve, so that it is
    about `light` (0..1) on the whole. The dark gains the most: the coat
    greys a little and takes a tint, and what was light in it (a pink nose,
    a tail) stays its colour."""
    coat = coat.convert("RGB")
    grey = ImageOps.grayscale(coat)
    mean = sum(i * n for i, n in enumerate(grey.histogram())) / (grey.width * grey.height)
    power = math.log(light) / math.log(mean / 255)
    return coat.point([round(255 * (v / 255) ** power) for v in range(256)] * 3)


def tiled(image, filt):
    """`filt` applied to `image` as if it went on for ever every way (it
    tiles): run on nine of it and the middle kept."""
    w, h = image.size
    big = Image.new(image.mode, (w * 3, h * 3))
    for i in range(3):
        for j in range(3):
            big.paste(image, (i * w, j * h))
    return filt(big).crop((w, h, w * 2, h * 2))


def normals(height, strength):
    """A normal map (OpenGL's: green up) from a height map that tiles,
    steeper as `strength` grows."""
    scale = max(1, round(8 / strength))
    # the slope across, and down the image (up is the other way in a
    # normal map's green)
    sx = tiled(height, lambda i: i.filter(ImageFilter.Kernel((3, 3), [1, 0, -1, 2, 0, -2, 1, 0, -1], scale, 128)))
    sy = tiled(height, lambda i: i.filter(ImageFilter.Kernel((3, 3), [-1, -2, -1, 0, 0, 0, 1, 2, 1], scale, 128)))
    return Image.merge("RGB", [sx, sy, Image.new("L", height.size, 255)])


def hollows(height, radius, depth):
    """Ambient occlusion from a height map: darker where it lies below
    what is round it within `radius` pixels, by up to `depth` (0..1)."""
    around = tiled(height, lambda i: i.filter(ImageFilter.GaussianBlur(radius)))
    below = ImageChops.subtract(around, height)
    return below.point(lambda v: max(0, round(255 - v * depth * 2)))


def at_every_wrap(size, draw):
    """`draw(dx, dy)` at the image's own place and its eight neighbours',
    so what crosses an edge comes in at the other."""
    for dx in (-size, 0, size):
        for dy in (-size, 0, size):
            draw(dx, dy)


def fur(size=512, seed="fur"):
    """A pale pelt that takes a tint: locks of hair lying down the image,
    each a few strands from a dark root to a light tip, the newer over the
    older; mottled. It tiles. The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    height = Image.new("L", (size, size), 30)
    d = ImageDraw.Draw(height)
    for _ in range(size * 3):
        x0, y0 = rng.random() * size, rng.random() * size
        length = size * rng.uniform(0.06, 0.12)
        lean, curl = rng.uniform(-0.25, 0.25), rng.uniform(-0.2, 0.2)
        for _ in range(rng.randint(4, 8)):
            sx, sy = x0 + rng.uniform(-4, 4), y0 + rng.uniform(-2, 2)
            pts = [(sx + (lean * t + curl * t * t) * length, sy + t * length)
                   for t in (i / 6 for i in range(7))]
            width = rng.choice((1, 1, 2))
            shade = rng.uniform(0.75, 1.0)
            for i in range(6):
                v = round((70 + 170 * (i + 1) / 6) * shade)
                a, b = pts[i], pts[i + 1]
                at_every_wrap(size, lambda dx, dy: d.line(
                    [(a[0] + dx, a[1] + dy), (b[0] + dx, b[1] + dy)], fill=v, width=width))
    height = height.filter(ImageFilter.GaussianBlur(0.6))
    mottle = noise(rng, (size, size), 6)
    light = ImageChops.add(height.point(lambda v: round(v * 0.45)), mottle.point(lambda v: 95 + v * 50 // 255))
    albedo = Image.merge("RGB", [light, light.point(lambda v: round(v * 0.97)), light.point(lambda v: round(v * 0.92))])
    ao = hollows(height, 6, 0.8)
    rough = height.point(lambda v: round(240 - v * 0.25))
    arm = Image.merge("RGB", [ao, rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 2.5), "arm": arm}


def scale_dome(sw, sh, rise_k=1.0):
    """One scale: a dome shaped like a shield, keeled down its middle, its
    root at the top narrower, its free edge round; and its mask."""
    dome = Image.new("L", (sw, sh), 0)
    mask = Image.new("L", (sw, sh), 0)
    px, pm = dome.load(), mask.load()
    for y in range(sh):
        for x in range(sw):
            u = (x + 0.5) / sw * 2 - 1
            t = (y + 0.5) / sh
            half = 0.62 + 0.38 * math.sin(min(1.0, t * 1.25) * math.pi / 2)
            edge = 1 - (max(0.0, t - 0.55) / 0.45) ** 2
            if abs(u) > half * math.sqrt(max(0.0, edge)):
                continue
            r = abs(u) / max(1e-3, half * math.sqrt(max(1e-3, edge)))
            rise = (0.35 + 0.65 * t) * rise_k
            keel = max(0.0, 1 - abs(u) * 6) * 0.3 * t
            px[x, y] = max(1, min(255, round(255 * (rise * (1 - r ** 3) * 0.8 + keel))))
            pm[x, y] = 255
    return dome, mask


def scales(size=512, rows=8, seed="scales"):
    """Overlapping scales that take a tint, in rows offset by half a scale,
    each a rounded shield keeled down its middle, its free edge down the
    image over the root of the one below; dark in the gaps between. No two
    alike: each its own size, lean, height and shade, set a little off its
    place, a few chipped at the edge, the rows swelling and narrowing
    (not a printed hex). Their middles glossier than their edges. It
    tiles. The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    cols = rows
    cw, rh = size / cols, size / rows
    pad = 2 * rh
    # each scale's own (by its row and column, the rows repeating): size,
    # lean, how high it stands, its shade, where it sits, a chip
    own = [[{
        "k": rng.uniform(0.8, 1.22) * (1.0 + 0.12 * math.sin(r / rows * math.tau)),
        "lean": rng.uniform(-11, 11),
        "rise": rng.uniform(0.7, 1.15),
        "tone": rng.uniform(-22, 18),
        "dx": rng.uniform(-0.17, 0.17) * cw,
        "dy": rng.uniform(-0.1, 0.1) * rh,
        "chip": rng.random() < 0.1,
    } for _ in range(cols)] for r in range(rows)]
    tall = Image.new("L", (size, round(size + 2 * pad)), 0)
    tone = Image.new("L", tall.size, 128)
    for r in range(rows + 2, -3, -1):
        for c in range(cols):
            o = own[r % rows][c]
            sw, sh = round(cw * 1.16 * o["k"]), round(rh * 1.7 * o["k"])
            dome, mask = scale_dome(sw, sh, o["rise"])
            if o["chip"]:
                d = ImageDraw.Draw(mask)
                cx = rng.uniform(0.2, 0.8) * sw
                d.ellipse([cx - sw * 0.08, sh * 0.88, cx + sw * 0.08, sh * 1.06], fill=0)
            dome = dome.rotate(o["lean"], resample=Image.BICUBIC)
            mask = mask.rotate(o["lean"], resample=Image.BICUBIC)
            shade = Image.new("L", (sw, sh), max(0, min(255, round(128 + o["tone"]))))
            x = round((c + 0.5 * (r % 2)) * cw - sw / 2 + o["dx"])
            y = round(pad + (r - 0.7) * rh + o["dy"] - (sh - rh * 1.7) * 0.5)
            for dx in (-size, 0, size):
                tall.paste(dome, (x + dx, y), mask)
                tone.paste(shade, (x + dx, y), mask)
    height = tall.crop((0, round(pad), size, round(pad) + size))
    tone = tone.crop((0, round(pad), size, round(pad) + size))
    height = height.filter(ImageFilter.GaussianBlur(0.8))
    # the hide under them swelling and sinking a little
    swell = noise(rng, (size, size), 4)
    height = ImageChops.add(height.point(lambda v: round(v * 0.88)), swell.point(lambda v: v * 30 // 255))
    mottle = noise(rng, (size, size), 7)
    light = ImageChops.add(height.point(lambda v: round(100 + v * 0.5)), mottle.point(lambda v: v * 26 // 255))
    light = ImageChops.add(light, tone, scale=1.0, offset=-128)
    albedo = Image.merge("RGB", [light] * 3)
    ao = hollows(height, 8, 1.0)
    rough = height.point(lambda v: round(215 - v * 0.25))
    arm = Image.merge("RGB", [ao, rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 4.0), "arm": arm}


def membrane(size=512, seed="membrane"):
    """A wing's membrane that takes a tint: thin skin stretched between
    the fingers, veins branching across it (raised, a shade darker), finer
    veinlets between, the skin creased and blotched; less rough than a
    hide, a little glossy where it is stretched. It tiles. The albedo,
    normal and ORM maps."""
    rng = random.Random(seed)
    height = Image.new("L", (size, size), 70)
    d = ImageDraw.Draw(height)

    def vein(x, y, angle, length, width, depth):
        pts = [(x, y)]
        for _ in range(12):
            angle += rng.uniform(-0.28, 0.28)
            step = length / 12
            x, y = x + math.cos(angle) * step, y + math.sin(angle) * step
            pts.append((x, y))
        for i in range(len(pts) - 1):
            a, b = pts[i], pts[i + 1]
            w = max(2, round(width * (1 - i / len(pts) * 0.55)))
            at_every_wrap(size, lambda dx, dy: d.line(
                [(a[0] + dx, a[1] + dy), (b[0] + dx, b[1] + dy)], fill=175, width=w))
        if depth > 0:
            for _ in range(rng.randint(1, 2)):
                k = rng.randint(3, 8)
                bx, by = pts[k]
                vein(bx, by, angle + rng.choice((-1, 1)) * rng.uniform(0.4, 0.9),
                     length * rng.uniform(0.4, 0.55), max(2, round(width * 0.55)), depth - 1)

    # broad veins fanned the same way (the fingers'), and what branches off
    fan = rng.uniform(0, math.tau)
    for i in range(4):
        vein(size * (i + rng.uniform(0.2, 0.8)) / 4, rng.uniform(0, size),
             fan + rng.uniform(-0.35, 0.35), size * rng.uniform(0.7, 0.95), 13, 2)
    height = height.filter(ImageFilter.GaussianBlur(1.6))
    creases = noise(rng, (size, size), 12)
    height = ImageChops.add(height, creases.point(lambda v: v * 22 // 255))
    blotch = noise(rng, (size, size), 6)
    veins = above(height, 120, 60)
    skin = blotch.point(lambda v: 150 + v * 45 // 255)
    light = ImageChops.subtract(skin, veins.point(lambda v: v * 40 // 255))
    albedo = Image.merge("RGB", [light, light.point(lambda v: round(v * 0.94)), light.point(lambda v: round(v * 0.9))])
    ao = hollows(height, 5, 0.6)
    rough = height.point(lambda v: round(165 - v * 0.15))
    arm = Image.merge("RGB", [ao, rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 2.5), "arm": arm}


def weave(size=512, threads=32, seed="weave"):
    """Cloth that takes a tint: a twill of round threads, warp and weft
    each over two and under two, a step along every row (the diagonal
    ribs of a wool cloak), the threads a little uneven; mottled. It tiles.
    The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    cell = size / threads
    # one thread's rounded rise across it (0..1), along a cell's width
    bump = [math.sin(math.pi * (i + 0.5) / cell) ** 0.6 for i in range(int(cell))]
    height = Image.new("L", (size, size), 0)
    px = height.load()
    warp = [rng.uniform(0.85, 1.0) for _ in range(threads)]
    weft = [rng.uniform(0.85, 1.0) for _ in range(threads)]
    for y in range(size):
        j, v = int(y // cell), int(y % cell)
        for x in range(size):
            i, u = int(x // cell), int(x % cell)
            # 2/2 twill: the warp is over where (i + j) mod 4 is 0 or 1
            over = (i + j) % 4 < 2
            if over:
                h = 0.55 + 0.45 * bump[u] * warp[i]
            else:
                h = 0.55 + 0.45 * bump[v] * weft[j]
            # a thread dips at the cell where it goes under
            px[x, y] = round(255 * h * (0.75 + 0.25 * (bump[v] if over else bump[u])))
    height = height.filter(ImageFilter.GaussianBlur(0.7))
    mottle = noise(rng, (size, size), 8)
    light = ImageChops.add(height.point(lambda v: round(120 + v * 0.35)), mottle.point(lambda v: v * 30 // 255))
    albedo = Image.merge("RGB", [light, light.point(lambda v: round(v * 0.98)), light.point(lambda v: round(v * 0.95))])
    ao = hollows(height, 3, 0.7)
    rough = height.point(lambda v: round(235 - v * 0.08))
    arm = Image.merge("RGB", [ao, rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 2.0), "arm": arm}


def floral(size=512, seed="floral"):
    """A Hawaiian shirt's print, in its own colours (tint it white): big
    hibiscus flowers, five round petals round a dark heart, and pointed
    leaves, scattered over a warm red; the cotton a little rough. It tiles.
    The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    albedo = Image.new("RGB", (size, size), (196, 58, 34))
    d = ImageDraw.Draw(albedo)
    def leaf(x, y, r, a):
        pts = []
        for k in range(12):
            t = k / 11
            w = math.sin(math.pi * t) * r * 0.35
            for side in (1, -1) if k in (0, 11) else (1,):
                pass
            pts.append((t, w))
        outline = [(x + math.cos(a) * (t * r) - math.sin(a) * w, y + math.sin(a) * (t * r) + math.cos(a) * w)
                   for t, w in pts]
        outline += [(x + math.cos(a) * (t * r) + math.sin(a) * w, y + math.sin(a) * (t * r) - math.cos(a) * w)
                    for t, w in reversed(pts)]
        at_every_wrap(size, lambda dx, dy: d.polygon([(px + dx, py + dy) for px, py in outline],
                                                     fill=(46, 112, 52)))
    def flower(x, y, r, colour):
        a0 = rng.uniform(0, math.tau)
        for k in range(5):
            a = a0 + k * math.tau / 5
            cx, cy = x + math.cos(a) * r * 0.55, y + math.sin(a) * r * 0.55
            at_every_wrap(size, lambda dx, dy: d.ellipse(
                [cx - r * 0.5 + dx, cy - r * 0.5 + dy, cx + r * 0.5 + dx, cy + r * 0.5 + dy], fill=colour))
        at_every_wrap(size, lambda dx, dy: d.ellipse(
            [x - r * 0.18 + dx, y - r * 0.18 + dy, x + r * 0.18 + dx, y + r * 0.18 + dy], fill=(120, 20, 30)))
    spots = []
    for _ in range(20):
        x, y = rng.random() * size, rng.random() * size
        spots.append((x, y))
        for _ in range(3):
            leaf(x, y, size * rng.uniform(0.09, 0.14), rng.uniform(0, math.tau))
    for x, y in spots:
        flower(x, y, size * rng.uniform(0.05, 0.08),
               rng.choice([(246, 236, 210), (250, 206, 70), (244, 150, 170)]))
    grain = noise(rng, (size, size), 64).point(lambda v: 118 + v * 20 // 255)
    height = grain.filter(ImageFilter.GaussianBlur(0.5))
    ao = Image.new("L", (size, size), 255)
    rough = Image.new("L", (size, size), 222)
    arm = Image.merge("RGB", [ao, rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 1.0), "arm": arm}


def stamp_max(canvas, image, x, y):
    """`image` laid on `canvas` at (x, y), the lighter of the two kept, so
    what crosses an edge comes in at the other (the canvas is square)."""
    size = canvas.size[0]
    w, h = image.size
    for dx in (-size, 0, size):
        for dy in (-size, 0, size):
            x0, y0 = x + dx, y + dy
            if x0 + w <= 0 or y0 + h <= 0 or x0 >= size or y0 >= size:
                continue
            box = (x0, y0, x0 + w, y0 + h)
            canvas.paste(ImageChops.lighter(canvas.crop(box), image), box[:2])


def forged(size=512, seed="forged"):
    """Hammered steel that takes a tint: the faces of a great many
    overlapping blows, the ridges between them, fine scratches across all,
    and patches of dark scale where it was never polished. The rubbed metal
    is satin (roughness about 0.4), the blows' hollows rougher, the scale
    rough and barely metal (about 0.85). It tiles. The albedo, normal and
    ORM maps."""
    rng = random.Random(seed)

    def blow(r):
        # a shallow round hollow, deepest in its middle
        n = 2 * r + 1
        image = Image.new("L", (n, n), 0)
        px = image.load()
        for y in range(n):
            for x in range(n):
                d = math.hypot(x - r, y - r) / r
                if d < 1:
                    px[x, y] = round(255 * (1 - d * d) ** 1.5)
        return image

    blows = [blow(r) for r in range(7, 24, 2)]
    depth = Image.new("L", (size, size), 0)
    for _ in range(size * size // 240):
        b = rng.choice(blows)
        k = rng.uniform(0.3, 0.65)
        r = b.size[0] // 2
        stamp_max(depth, b.point(lambda v, k=k: round(v * k)), rng.randrange(size) - r,
                  rng.randrange(size) - r)
    depth = tiled(depth, lambda i: i.filter(ImageFilter.GaussianBlur(1.0)))
    scratch = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(scratch)
    lean = rng.uniform(0, math.pi)
    for _ in range(170):
        x, y = rng.random() * size, rng.random() * size
        a = lean + rng.gauss(0, 0.5)
        n = size * rng.uniform(0.03, 0.16)
        v = rng.randint(120, 255)
        ex, ey = x + math.cos(a) * n, y + math.sin(a) * n
        at_every_wrap(size, lambda dx, dy: d.line([(x + dx, y + dy), (ex + dx, ey + dy)], fill=v, width=1))
    scratch = tiled(scratch, lambda i: i.filter(ImageFilter.GaussianBlur(0.45)))
    # the scale: soft-edged patches over a fifth or so of it
    scale = above(tiled(noise(rng, (size, size), 10), lambda i: i.filter(ImageFilter.GaussianBlur(3))), 165, 60)
    mottle = noise(rng, (size, size), 28)
    height = ImageChops.subtract(Image.new("L", (size, size), 225), depth)
    height = ImageChops.subtract(height, scratch.point(lambda v: round(v * 0.12)))
    # albedo: bright metal, darker in the blows' bottoms and the scale,
    # the scratches bare and bright
    grey = ImageChops.subtract(Image.new("L", (size, size), 205), depth.point(lambda v: round(v * 0.22)))
    grey = ImageChops.subtract(grey, scale.point(lambda v: round(v * 0.35)))
    grey = ImageChops.add(grey, mottle.point(lambda v: v * 18 // 255))
    grey = ImageChops.add(grey, scratch.point(lambda v: round(v * 0.16)))
    # (the scale a cold blue-black, the metal itself neutral)
    albedo = Image.merge("RGB", [ImageChops.subtract(grey, scale.point(lambda v: round(v * 0.04))),
                                 ImageChops.subtract(grey, scale.point(lambda v: round(v * 0.02))), grey])
    rough = Image.new("L", (size, size), 100)
    rough = ImageChops.add(rough, depth.point(lambda v: round(v * 0.2)))
    rough = ImageChops.add(rough, scale.point(lambda v: round(v * 0.45)))
    rough = ImageChops.add(rough, mottle.point(lambda v: v * 30 // 255))
    rough = ImageChops.subtract(rough, scratch.point(lambda v: round(v * 0.12)))
    metal = ImageChops.subtract(Image.new("L", (size, size), 255), scale.point(lambda v: round(v * 0.35)))
    arm = Image.merge("RGB", [hollows(height, 5, 0.6), rough, metal])
    return {"albedo": albedo, "normal": normals(height, 2.2), "arm": arm}


def gltf_mesh(path, name):
    """A mesh of a .gltf (its .bin beside it), as triangles of (position,
    UV) corners in its rest pose: pure Python, for a bake by the mesh's
    UVs."""
    import json
    import os
    import struct
    doc = json.load(open(path))
    base = os.path.dirname(path)
    bufs = [open(os.path.join(base, b["uri"]), "rb").read() for b in doc["buffers"]]
    kinds = {5121: "B", 5123: "H", 5125: "I", 5126: "f"}
    widths = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}

    def read(i):
        a = doc["accessors"][i]
        view = doc["bufferViews"][a["bufferView"]]
        n, c = widths[a["type"]], kinds[a["componentType"]]
        size = struct.calcsize("<" + c) * n
        stride = view.get("byteStride", size)
        at = view.get("byteOffset", 0) + a.get("byteOffset", 0)
        buf = bufs[view["buffer"]]
        out = [struct.unpack_from("<" + c * n, buf, at + k * stride) for k in range(a["count"])]
        return [v if n > 1 else v[0] for v in out]

    mesh = next(m for m in doc["meshes"] if m["name"] == name)
    tris = []
    for p in mesh["primitives"]:
        pos, uv = read(p["attributes"]["POSITION"]), read(p["attributes"]["TEXCOORD_0"])
        idx = read(p["indices"])
        for t in range(0, len(idx), 3):
            tris.append([(pos[k], uv[k]) for k in idx[t:t + 3]])
    return tris


def bake(tris, size, paint):
    """Each texel inside a triangle of `tris` (by its UVs, v down the
    image) gets `paint(position, density)`: the rest-pose point there and
    texels to a metre; returns {(x, y): what paint gave}."""
    out = {}
    for corners in tris:
        (p0, (u0, v0)), (p1, (u1, v1)), (p2, (u2, v2)) = corners
        x0, y0, x1, y1, x2, y2 = u0 * size, v0 * size, u1 * size, v1 * size, u2 * size, v2 * size
        den = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2)
        if abs(den) < 1e-9:
            continue
        # texels to a metre over this triangle
        e1 = [p1[k] - p0[k] for k in range(3)]
        e2 = [p2[k] - p0[k] for k in range(3)]
        cr = (e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0])
        area = math.sqrt(sum(c * c for c in cr)) / 2
        if area < 1e-12:
            continue
        density = math.sqrt(abs(den) / 2 / area)
        for py in range(max(0, int(min(y0, y1, y2))), min(size - 1, int(max(y0, y1, y2)) + 1) + 1):
            for px in range(max(0, int(min(x0, x1, x2))), min(size - 1, int(max(x0, x1, x2)) + 1) + 1):
                sx, sy = px + 0.5, py + 0.5
                l0 = ((y1 - y2) * (sx - x2) + (x2 - x1) * (sy - y2)) / den
                l1 = ((y2 - y0) * (sx - x2) + (x0 - x2) * (sy - y2)) / den
                l2 = 1 - l0 - l1
                if min(l0, l1, l2) < -0.01:
                    continue
                q = tuple(l0 * p0[k] + l1 * p1[k] + l2 * p2[k] for k in range(3))
                out[(px, py)] = paint(q, density)
    return out


def grown(texels, size, steps):
    """`texels` ({(x, y): value}) grown `steps` texels out over what is
    round them, each new one the value of a neighbour: an atlas's islands
    padded, so that a far mip of them never takes their gaps' colour."""
    texels = dict(texels)
    edge = set(texels)
    for _ in range(steps):
        new = {}
        for (x, y) in edge:
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                q = (x + dx, y + dy)
                if 0 <= q[0] < size and 0 <= q[1] < size and q not in texels and q not in new:
                    new[q] = texels[(x, y)]
        texels.update(new)
        edge = set(new)
    return texels


def lames(path, mesh, size=1024):
    """A samurai's cuirass painted on an outfit's body by its own UVs: from
    the waist up, lacquered lames, each laced with crimson silk braid below
    the black lacquer of its top edge, every lame a step further out than
    the one over it (its top edge catches the light); the lowest
    cross-knotted in ivory; a lacquered plate over the chest, rimmed in
    gilt, a gilt crest on it. The pattern is laid by each texel's place on the body in its
    rest pose, so it runs on across the atlas's seams. In its own colours
    (tint it white); the lacquer glossy, the silk matte. The albedo,
    normal and ORM maps."""
    rng = random.Random("lames")
    tris = gltf_mesh(path, mesh)
    waist, high, plate_top = 0.935, 0.06, 1.455
    pitch, cord = 0.02, 0.0165
    lacquer, wear = (20, 12, 10), (62, 34, 24)
    silk, gap = (92, 15, 12), (14, 8, 7)
    ivory, gilt = (214, 200, 172), (176, 132, 52)
    grain = noise(rng, (size, size), 48).load()

    def paint(q, density):
        x, y, z = q
        # round the body: its arc from the front, a cord's width apart
        s = math.atan2(x, z) * 0.17
        h = 0.0
        if y >= plate_top:
            # the plate over the chest, its gilt rim along its foot, a
            # crest on its front: a ring round three commas
            rim = y < plate_top + 0.007
            dx, dy = x, y - (plate_top + 0.045)
            r = math.hypot(dx, dy)
            spin = math.atan2(dy, dx)
            commas = r < 0.026 and z > 0 and math.cos(3 * spin + r * 90) > 0.35
            crest = z > 0 and (0.028 < r < 0.034 or commas)
            if rim or crest:
                return gilt, 0.35, 1.0, 0.0045, density
            return lacquer, 0.2, 1.0, 0.003, density
        u = (y - waist) / high
        i, f = math.floor(u), u - math.floor(u)
        # each lame further out than the one over it; its top edge rounded
        h = -(i + max(0.0, (f - 0.84) / 0.16) ** 2) * 0.0075
        if f > 0.7 or i < 0:
            # the lacquer above the braid, worn to the undercoat at its edge
            k = max(0.0, (f - 0.9) / 0.1)
            colour = tuple(round(a + (b - a) * k) for a, b in zip(lacquer, wear))
            return colour, 0.18 + 0.3 * k, 1.0, h, density
        # the braid: cords side by side, each a herringbone of threads
        c = (s / pitch) % 1.0
        half = cord / pitch / 2
        d = abs(c - 0.5)
        if d > half:
            return gap, 0.8, 0.45, h - 0.002, density
        across = d / half
        herring = 0.5 + 0.5 * math.sin((y / 0.006 + across * 0.8) * math.tau)
        lift = 0.0016 * math.sqrt(max(0.0, 1 - across * across)) + 0.0004 * herring
        shade = 0.82 + 0.22 * herring - 0.25 * across ** 3
        colour = tuple(min(255, round(v * shade)) for v in silk)
        # the lowest lame cross-knotted
        if i == 0:
            kx = (s / 0.06) % 1.0 - 0.5
            ky = (f - 0.35) / 0.3
            if abs(ky) < 1 and min(abs(kx * 0.06 - ky * 0.014), abs(kx * 0.06 + ky * 0.014)) < 0.005:
                return ivory, 0.6, 1.0, h + 0.0025, density
        return colour, 0.72, 0.85 + 0.15 * shade, h + lift, density

    return baked(grown(bake(tris, size, paint), size, 6), size, lacquer, grain)


def baked(texels, size, ground, grain, metal_ground=0):
    """The albedo, normal and ORM maps of texels a bake painted ({(x, y):
    (colour, roughness, occlusion, height in metres, texels to a metre[,
    metal])}), over `ground` where none is; `grain` (a loaded noise image)
    mottles the colour a little."""
    albedo = Image.new("RGB", (size, size), ground)
    arm = Image.new("RGB", (size, size), (255, 51, metal_ground))
    a, r = albedo.load(), arm.load()
    height = {}
    for (px, py), texel in texels.items():
        colour, rough, ao, h, density = texel[:5]
        metal = texel[5] if len(texel) > 5 else 0.0
        g = 0.92 + 0.16 * grain[px, py] / 255
        a[px, py] = tuple(min(255, round(v * g)) for v in colour)
        r[px, py] = (round(255 * ao), round(255 * rough), round(255 * metal))
        # in texels, so the slope is the body's own
        height[(px, py)] = h * density
    normal = Image.new("RGB", (size, size), (128, 128, 255))
    n = normal.load()
    for (px, py), h in height.items():
        left = height.get((px - 1, py), h)
        right = height.get((px + 1, py), h)
        up = height.get((px, py - 1), h)
        down = height.get((px, py + 1), h)
        nx, ny = (left - right) / 2, (down - up) / 2
        k = 1 / math.sqrt(nx * nx + ny * ny + 1)
        n[px, py] = (round(128 + 127 * nx * k), round(128 + 127 * ny * k), round(128 + 127 * k))
    return {"albedo": albedo, "normal": normal, "arm": arm}


def relief(coat, radius, light):
    """An outfit's painting without its colours: only what is lighter or
    darker than what is round it within `radius` pixels (its seams,
    stitches, folds, worn edges and the shade of its hollows), about
    `light` (0..1) everywhere, so that a dye over it shows as itself on
    cloth and leather alike."""
    grey = ImageOps.grayscale(coat.convert("RGB"))
    around = grey.filter(ImageFilter.GaussianBlur(radius))
    g, a = grey.load(), around.load()
    w, h = grey.size
    out = Image.new("L", grey.size)
    o = out.load()
    for y in range(h):
        for x in range(w):
            o[x, y] = max(0, min(255, round(255 * light * (g[x, y] + 4) / (a[x, y] + 4))))
    return Image.merge("RGB", [out] * 3)


def ringmail(path, meshes, size=1024):
    """Ring mail painted on an outfit's body and sleeves by their own UVs:
    iron rings sewn flat on leather in rows, each row lapping the one below
    it, every other row half a ring along, the leather showing through the
    rings and between them. Laid by each texel's place on the body in its
    rest pose (round the trunk and up it; round each arm and along it), so
    the rings are the same size everywhere and run on across the atlas's
    seams. In its own colours (tint it white); the iron metal and fairly
    glossy, the leather rough. The albedo, normal and ORM maps."""
    rng = random.Random("ringmail")
    grain = noise(rng, (size, size), 64).load()
    rust = noise(rng, (size, size), 24).load()
    pitch, row = 0.017, 0.0125
    outer, inner = 0.0084, 0.0056
    leather, iron = (58, 40, 28), (150, 150, 146)

    def paint(s, t, density, px):
        best = None
        r0 = math.floor(t / row)
        # the higher rows lie over the lower
        for r in (r0 + 2, r0 + 1, r0, r0 - 1):
            cy = r * row
            shift = 0.5 * pitch * (r % 2)
            c = round((s - shift) / pitch)
            cx = c * pitch + shift
            d = math.hypot(s - cx, t - cy)
            if inner < d < outer:
                best = (d, cy)
                break
        if best is None:
            return leather, 0.72, 0.7, 0.0, density
        d, cy = best
        across = (d - (inner + outer) / 2) / ((outer - inner) / 2)
        lift = 0.0016 * math.sqrt(max(0.0, 1 - across * across))
        # each ring tipped, its lower edge over the row below
        tip = 0.0012 * (cy - t) / outer
        dirt = rust[px] / 255
        shade = 0.75 + 0.3 * math.sqrt(max(0.0, 1 - across * across)) - 0.25 * dirt
        colour = tuple(min(255, round(v * shade + 18 * dirt * k)) for v, k in zip(iron, (1.0, 0.6, 0.2)))
        return colour, 0.34 + 0.3 * dirt, 1.0, lift + tip + 0.002, density, 1.0

    texels = {}
    for name in meshes:
        arm = "Arms" in name

        def at(q, density, arm=arm):
            x, y, z = q
            if arm:
                # round the arm (radius about 5 cm) and along it
                return math.atan2(y - 1.45, z) * 0.05, abs(x), density
            return math.atan2(x, z) * 0.17, y, density

        tris = gltf_mesh(path, name)
        texels.update(bake(tris, size, lambda q, density: ("q", at(q, density))))
    painted = {}
    for (px, py), (_, (s, t, density)) in texels.items():
        painted[(px, py)] = paint(s, t, density, (px, py))
    return baked(grown(painted, size, 6), size, leather, grain)


def winingas(size=512, seed="winingas"):
    """Wool bands wound round a leg, each lapping the one under it, held by
    leather thongs crossed over them; pale to take a tint (the thongs
    darker). It tiles. The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    bands = 8
    band = size / bands
    height = Image.new("L", (size, size), 0)
    albedo = Image.new("L", (size, size), 0)
    hp, ap = height.load(), albedo.load()
    tone = [rng.uniform(0.85, 1.0) for _ in range(bands)]
    for y in range(size):
        for x in range(size):
            # a band's top a little wavy, the same at both edges (tiles)
            yy = y + band * 0.12 * math.sin(math.tau * x / size + y / size * 3)
            k = int(yy // band) % bands
            f = (yy % band) / band
            # it rises from where it comes out from under the one over it
            # to its own lapping lower edge
            hp[x, y] = round(60 + 150 * f ** 0.6)
            ap[x, y] = round((150 + 60 * f ** 0.5) * tone[k])
    weave = noise(rng, (size, size), 160).point(lambda v: 205 + v * 50 // 255)
    albedo = ImageChops.multiply(albedo, weave)
    rough = Image.new("L", (size, size), 236)
    metal = Image.new("L", (size, size), 0)
    # the thongs: crossed over the bands, once each way
    thong = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(thong)
    w = max(3, size // 36)
    # (each way round once while it goes twice down the image: two
    # crossings in it, and it tiles)
    for k in range(2):
        for a, b in (((0, k * size), (size, k * size + 2 * size)), ((size, k * size), (0, k * size + 2 * size))):
            for dy in (-2 * size, -size, 0):
                d.line([(a[0], a[1] + dy), (b[0], b[1] + dy)], fill=255, width=w)
    thong = thong.filter(ImageFilter.GaussianBlur(0.8))
    height = Image.composite(Image.new("L", (size, size), 245), height, thong)
    albedo = Image.composite(Image.new("L", (size, size), 62), albedo, thong)
    rough = Image.composite(Image.new("L", (size, size), 150), rough, thong)
    ao = hollows(height, 6, 0.7)
    arm = Image.merge("RGB", [ao, rough, metal])
    rgb = Image.merge("RGB", [albedo, albedo.point(lambda v: round(v * 0.97)), albedo.point(lambda v: round(v * 0.93))])
    return {"albedo": rgb, "normal": normals(height, 2.0), "arm": arm}


def face(skin, eyes, nose, mouth, stubble=False, paint=None):
    """A base character's skin with a face that reads at a distance: the
    eyes sunk in shadowed sockets with a dark lash line, hollows under the
    cheekbones, the nose's bridge lit and its sides shaded, a shadow under
    the nose and the lower lip, the lips a little deeper; on a man, `stubble`
    on the jaw and the lip; and war paint by `paint`: "woad" (a band of blue
    across the eyes, running down the cheeks) or "kohl" (the eyes lined, the
    line drawn out past their corners). Positions are shares of the image."""
    rng = random.Random("face" + (paint or "") + ("s" if stubble else ""))
    skin = skin.convert("RGB")
    size = skin.size
    w, h = size

    def soft(shapes, blur):
        """A soft mask of ellipses ((x, y, rx, ry, level) in shares)."""
        m = Image.new("L", size, 0)
        d = ImageDraw.Draw(m)
        for x, y, rx, ry, level in shapes:
            d.ellipse([(x - rx) * w, (y - ry) * h, (x + rx) * w, (y + ry) * h], fill=level)
        return m.filter(ImageFilter.GaussianBlur(blur * w))

    def shade(image, mask, colour):
        """`image` multiplied toward `colour` where `mask` is."""
        dark = ImageChops.multiply(image, Image.new("RGB", size, colour))
        return Image.composite(dark, image, mask)

    (lx, ly), (rx_, ry_) = sorted(eyes)
    span = rx_ - lx
    out = skin
    # the sockets, deepest above the eye and toward its outer corner
    sockets_ = [(x + side * 0.005, y - 0.005, 0.04, 0.028, 235) for (x, y), side in (((lx, ly), -1), ((rx_, ry_), 1))]
    out = shade(out, soft(sockets_, 0.007), "#7a5450")
    lash = [(x, y, 0.02, 0.012, 245) for x, y in eyes]
    out = shade(out, soft(lash, 0.002), "#4a302c")
    # the brow's ridge: a dark line along each brow
    brows = [(x + side * 0.003, y - 0.026, 0.032, 0.007, 200) for (x, y), side in (((lx, ly), -1), ((rx_, ry_), 1))]
    out = shade(out, soft(brows, 0.003), "#8a6058")
    # the cheekbones' hollows, the nose's sides, under the nose and the lip
    nx, ny = nose
    mx, my = mouth
    hollows_ = [(lx - 0.016, ly + 0.075, 0.034, 0.016, 70), (rx_ + 0.016, ry_ + 0.075, 0.034, 0.016, 70),
                (nx - 0.022, ny - 0.02, 0.008, 0.03, 110), (nx + 0.022, ny - 0.02, 0.008, 0.03, 110),
                (nx, ny + 0.018, 0.016, 0.006, 120), (mx, my + 0.02, 0.022, 0.007, 120)]
    out = shade(out, soft(hollows_, 0.008), "#a88070")
    # the bridge of the nose and the tops of the cheekbones lit
    lit = soft([(nx, (ly + ny) / 2, 0.006, (ny - ly) / 2, 90), (lx - 0.006, ly + 0.035, 0.022, 0.01, 60),
                (rx_ + 0.006, ry_ + 0.035, 0.022, 0.01, 60)], 0.004)
    out = Image.composite(ImageChops.screen(out, Image.new("RGB", size, "#3a2a20")), out, lit)
    # the lips a shade deeper
    out = shade(out, soft([(mx, my, 0.026, 0.01, 110)], 0.003), "#c08080")
    if stubble:
        random.seed(rng.random())
        dots = Image.effect_noise(size, 60).point(lambda v: 255 if v > 175 else 0)
        dots = dots.filter(ImageFilter.GaussianBlur(0.5))
        jaw = soft([(mx, my + 0.03, span * 0.6, 0.05, 255), (mx, (ny + my) / 2 + 0.004, 0.032, 0.008, 255)], 0.01)
        jaw = ImageChops.subtract(jaw, soft([(mx, my, 0.03, 0.012, 255), (nx, ny, 0.03, 0.02, 255)], 0.004))
        out = shade(out, ImageChops.multiply(jaw, dots).point(lambda v: v * 2 // 5), "#6a5a58")
    if paint == "woad":
        band = soft([(nx, ly + 0.003, span * 0.98, 0.026, 255)], 0.002)
        ragged = noise(rng, size, 70).point(lambda v: 255 if v > 30 else 0).filter(ImageFilter.GaussianBlur(0.001 * w))
        drips = Image.new("L", size, 0)
        d = ImageDraw.Draw(drips)
        for x in (lx - 0.012, lx + 0.008, rx_ - 0.008, rx_ + 0.012):
            d.line([(x * w, ly * h), ((x + rng.uniform(-0.003, 0.003)) * w, (ly + 0.06) * h)], fill=255,
                   width=max(2, round(0.007 * w)))
        drips = drips.filter(ImageFilter.GaussianBlur(0.002 * w))
        woad = ImageChops.multiply(ImageChops.lighter(band, drips), ragged).point(lambda v: v * 9 // 10)
        out = Image.composite(Image.new("RGB", size, "#1a2a4e"), out, woad)
    elif paint == "kohl":
        line = Image.new("L", size, 0)
        d = ImageDraw.Draw(line)
        for (x, y), side in (((lx, ly), -1), ((rx_, ry_), 1)):
            d.line([((x - side * 0.014) * w, (y + 0.007) * h), ((x + side * 0.04) * w, (y - 0.005) * h)], fill=255,
                   width=max(3, round(0.007 * w)))
        line = ImageChops.lighter(line.filter(ImageFilter.GaussianBlur(0.0015 * w)), soft(lash, 0.002))
        out = shade(out, line.point(lambda v: v * 4 // 5), "#3a2a2a")
    return out


def warts(coat, normal, orm, seed="warts"):
    """A coarse hide over a creature's own maps (a troll's): warts and
    knobs scattered over it, dark round their feet and lighter on their
    crowns, and creased between; its own relief kept under them. The
    hide is rough in its creases and a little glossy on its knobs. The
    albedo, normal and ORM maps, the size of `coat`."""
    rng = random.Random(seed)
    coat = coat.convert("RGB")
    size = coat.size
    w, h = size
    normal = normal.convert("RGB").resize(size, Image.LANCZOS)
    orm = orm.convert("RGB").resize(size, Image.LANCZOS)
    # the knobs: soft domes of many sizes; the creases: ridged noise
    knobs = Image.new("L", size, 0)
    d = ImageDraw.Draw(knobs)
    for _ in range(w * h // 2600):
        x, y = rng.random() * w, rng.random() * h
        r = rng.choice((2, 3, 3, 4, 5, 7)) * w / 1024
        d.ellipse([x - r, y - r, x + r, y + r], fill=rng.randint(150, 255))
    knobs = knobs.filter(ImageFilter.GaussianBlur(1.6 * w / 1024))
    crease = noise(rng, size, 60)
    crease = crease.point(lambda v: 255 - abs(v - 128) * 2).filter(ImageFilter.GaussianBlur(1.2))
    # (none over what is lightest: eyes, teeth, claws)
    bare = above(ImageOps.grayscale(coat), 205, 20).filter(ImageFilter.GaussianBlur(3))
    keep = ImageChops.invert(bare)
    knobs = ImageChops.multiply(knobs, keep)
    crease = ImageChops.multiply(crease, keep)
    height = ImageChops.add(knobs.point(lambda v: v * 3 // 4), crease.point(lambda v: v // 5))
    # the colour: darker round each knob's foot and in the creases
    feet = ImageChops.subtract(knobs.filter(ImageFilter.GaussianBlur(3 * w / 1024)), knobs)
    shade = ImageChops.subtract(Image.new("L", size, 255), feet.point(lambda v: min(255, v * 3)))
    shade = ImageChops.multiply(shade, crease.point(lambda v: 215 + v // 6))
    shade = ImageChops.add(shade, knobs.point(lambda v: v // 10))
    albedo = ImageChops.multiply(coat, Image.merge("RGB", [shade] * 3))
    # the hide's slopes over its own: tilts added
    bump = normals(height, 2.5).load()
    own = normal.load()
    out = Image.new("RGB", size)
    px = out.load()
    for y in range(h):
        for x in range(w):
            br, bg, _ = bump[x, y]
            r, g, b = own[x, y]
            nx = (r - 128) + (br - 128)
            ny = (g - 128) + (bg - 128)
            nz = max(1, b - 128)
            k = 127 / max(1.0, math.sqrt(nx * nx + ny * ny + nz * nz))
            px[x, y] = (round(128 + nx * k), round(128 + ny * k), round(128 + nz * k))
    o_r, o_g, o_b = orm.split()
    ao = ImageChops.multiply(o_r, hollows(height, 4, 0.8))
    rough = ImageChops.subtract(Image.new("L", size, 230), knobs.point(lambda v: v // 4))
    rough = ImageChops.add(rough.point(lambda v: v * 3 // 4), o_g.point(lambda v: v // 4))
    return {"albedo": albedo, "normal": out, "arm": Image.merge("RGB", [ao, rough, Image.new("L", size, 0)])}


def tile_noise(rng, size, cells):
    """Smooth noise (0..255) that tiles: `noise` drawn from a grid that
    wraps (laid three by three and the middle kept)."""
    grid = Image.new("L", (cells, cells))
    grid.putdata([rng.randrange(256) for _ in range(cells * cells)])
    big = Image.new("L", (cells * 3, cells * 3))
    for i in range(3):
        for j in range(3):
            big.paste(grid, (i * cells, j * cells))
    return big.resize((size * 3, size * 3), Image.BICUBIC).crop((size, size, size * 2, size * 2))


def granite(stone, seed="granite"):
    """Grey granite over a stone's own painting (a boulder's, by its UVs):
    its cracks and hollows kept, softened, in cool greys instead of its
    colours; the crystals of granite speckled through it, black mica and
    white quartz; and chips struck off it here and there, fresh pale faces
    with a dark lip under them. The albedo, the size of `stone`."""
    rng = random.Random(seed)
    stone = stone.convert("RGB")
    size = stone.size
    w, h = size
    k = w / 1024
    # its light and dark, its cracks' contrast held down (they read as a
    # sponge's holes), in granite's greys
    grey = ImageOps.autocontrast(ImageOps.grayscale(stone), cutoff=1)
    grey = grey.filter(ImageFilter.GaussianBlur(1.8 * k))
    grey = grey.point(lambda v: round(128 + (v - 128) * 0.45))
    out = ImageOps.colorize(grey, black="#1d1e21", mid="#4a4b4d", white="#7a7a78")
    # broad weathering: some faces lighter, some darker
    mottle = noise(rng, size, 9).point(lambda v: 205 + v * 50 // 255)
    out = ImageChops.multiply(out, Image.merge("RGB", [mottle] * 3))
    # the crystals, soft-edged
    specks = Image.new("RGB", size, (0, 0, 0))
    where = Image.new("L", size, 0)
    ds, dw = ImageDraw.Draw(specks), ImageDraw.Draw(where)
    for colour, count, sizes in (((22, 22, 25), 0.09, (0.6, 0.9, 1.3)),
                                 ((150, 148, 141), 0.07, (0.6, 0.9, 1.4, 1.9)),
                                 ((100, 99, 95), 0.05, (1.2, 2.0, 2.8))):
        for _ in range(round(w * h * count / 30)):
            x, y = rng.random() * w, rng.random() * h
            r = rng.choice(sizes) * k
            box = [x - r, y - r * rng.uniform(0.6, 1.0), x + r, y + r]
            ds.ellipse(box, fill=colour)
            dw.ellipse(box, fill=rng.randint(110, 200))
    where = where.filter(ImageFilter.GaussianBlur(0.6 * k))
    out = Image.composite(specks, out, where)
    # chips: angular pale faces, a dark lip along their lower edge
    lips = Image.new("L", size, 0)
    faces = Image.new("L", size, 0)
    dl, df = ImageDraw.Draw(lips), ImageDraw.Draw(faces)
    for _ in range(round(40 * k * k)):
        x, y = rng.random() * w, rng.random() * h
        r = rng.uniform(6, 26) * k
        n = rng.randint(3, 7)
        a0 = rng.random() * math.tau
        pts = [(x + math.cos(a0 + i * math.tau / n + rng.uniform(-0.3, 0.3)) * r * rng.uniform(0.55, 1.0),
                y + math.sin(a0 + i * math.tau / n + rng.uniform(-0.3, 0.3)) * r * rng.uniform(0.55, 1.0))
               for i in range(n)]
        dl.polygon([(px, py + 2.0 * k) for px, py in pts], fill=255)
        df.polygon(pts, fill=rng.randint(150, 230))
    lips = ImageChops.subtract(lips, faces.point(lambda v: 255 if v else 0))
    lips = lips.filter(ImageFilter.GaussianBlur(0.7 * k))
    faces = faces.filter(ImageFilter.GaussianBlur(0.5 * k))
    out = Image.composite(Image.new("RGB", size, "#1a1a1c"), out, lips.point(lambda v: v * 7 // 10))
    fresh = ImageChops.add(out, Image.new("RGB", size, (30, 30, 29)))
    return Image.composite(fresh, out, faces)


def _veins(rng, size, freq, warp, width, turns):
    """Marble's veins: where sin(the way across, bent by turbulence)
    crosses nought, a line about `width` wide (thicker and thinner along
    it), and a broad soft streak about it. (veins, streaks), 0..255; they
    tile."""
    layers = [(tile_noise(rng, size, c), a) for c, a in ((3, 1.0), (6, 0.5), (12, 0.25), (24, 0.13), (48, 0.07))]
    px = [l.load() for l, _ in layers]
    thick = tile_noise(rng, size, 7).load()
    veins = Image.new("L", (size, size))
    streaks = Image.new("L", (size, size))
    o, q = veins.load(), streaks.load()
    fx, fy = turns
    tau = math.tau
    total = sum(a for _, a in layers)
    for y in range(size):
        for x in range(size):
            # turbulence: the noise folded at its middle, sharp creases
            n = sum(abs(p[x, y] - 128) * a for p, (_, a) in zip(px, layers)) / (128 * total)
            v = abs(math.sin(tau * (fx * x + fy * y) / size * freq + n * warp))
            w = width * (0.35 + 1.3 * thick[x, y] / 255)
            o[x, y] = round(255 * max(0.0, 1.0 - v / w) ** 0.7)
            q[x, y] = round(255 * max(0.0, 1.0 - v / 0.45) ** 2)
    return veins, streaks


def marble(size=512, seed="marble", slabs=0):
    """Old temple marble: grey-bone, clouded and dimmed by age, its veins
    dark and wandering, fine hairline cracks across it and darker stains
    spreading in it; worn dull, a gloss left only in patches where feet
    polished it. With `slabs`, laid in that many slabs a side, each cut
    from another part of the stone, dirt packed in the joints and their
    edges chipped. Rough (about 0.6), the worn patches glossier (0.3), the
    joints rough. It tiles. The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    main, streak = _veins(rng, size, 2, 4.2, 0.07, (1, 1))
    fine, _ = _veins(rng, size, 3, 6.5, 0.035, (2, -1))
    cracks, _ = _veins(rng, size, 5, 11.0, 0.018, (3, 1))
    cloud = tile_noise(rng, size, 6)
    soft = tile_noise(rng, size, 20)
    stain = above(tile_noise(rng, size, 5), 140, 80)
    worn = above(tile_noise(rng, size, 4), 165, 50)
    if slabs:
        # each slab its own part of the stone: the fields rolled by a step
        # of their own under it
        n = size // slabs
        def cut(field):
            out = Image.new("L", (size, size))
            for i in range(slabs):
                for j in range(slabs):
                    dx, dy = rng.randrange(size), rng.randrange(size)
                    rolled = ImageChops.offset(field, dx, dy)
                    out.paste(rolled.crop((i * n, j * n, (i + 1) * n, (j + 1) * n)), (i * n, j * n))
            return out
        main, fine, cloud, streak, cracks, stain = (cut(main), cut(fine), cut(cloud), cut(streak),
                                                    cut(cracks), cut(stain))
    main = tiled(main, lambda i: i.filter(ImageFilter.GaussianBlur(0.9)))
    fine = tiled(fine, lambda i: i.filter(ImageFilter.GaussianBlur(0.5)))
    cracks = tiled(cracks, lambda i: i.filter(ImageFilter.GaussianBlur(0.4)))
    stain = tiled(stain, lambda i: i.filter(ImageFilter.GaussianBlur(5)))
    # the ground: grey-bone, clouded darker in broad patches
    base = Image.new("RGB", (size, size), "#a39d91")
    shade = cloud.point(lambda v: 196 + v * 59 // 255)
    shade = ImageChops.multiply(shade, soft.point(lambda v: 232 + v * 23 // 255))
    out = ImageChops.multiply(base, Image.merge("RGB", [shade] * 3))
    # the veins: a broad grey streak about them, a soft halo, a dark core
    streak = tiled(streak, lambda i: i.filter(ImageFilter.GaussianBlur(6)))
    out = Image.composite(Image.new("RGB", (size, size), "#7f7a70"), out, streak.point(lambda v: v * 45 // 100))
    halo = tiled(main, lambda i: i.filter(ImageFilter.GaussianBlur(4)))
    out = Image.composite(Image.new("RGB", (size, size), "#6e6960"), out, halo.point(lambda v: v * 6 // 10))
    out = Image.composite(Image.new("RGB", (size, size), "#3a3630"), out, main.point(lambda v: v * 85 // 100))
    out = Image.composite(Image.new("RGB", (size, size), "#5e5a52"), out, fine.point(lambda v: v * 6 // 10))
    # hairline cracks, and stains spread through it (old water, soot)
    out = Image.composite(Image.new("RGB", (size, size), "#2a2722"), out, cracks.point(lambda v: v * 8 // 10))
    out = Image.composite(Image.new("RGB", (size, size), "#4e4636"), out, stain.point(lambda v: v * 55 // 100))
    height = Image.new("L", (size, size), 200)
    height = ImageChops.subtract(height, main.point(lambda v: v // 12))
    height = ImageChops.subtract(height, cracks.point(lambda v: v // 6))
    rough = Image.new("L", (size, size), 160)
    rough = ImageChops.subtract(rough, worn.point(lambda v: v * 80 // 255))
    rough = ImageChops.add(rough, soft.point(lambda v: v * 20 // 255))
    rough = ImageChops.add(rough, cracks.point(lambda v: v * 50 // 255))
    if slabs:
        n = size // slabs
        joints = Image.new("L", (size, size), 0)
        d = ImageDraw.Draw(joints)
        for i in range(slabs + 1):
            for c in range(i * n - 2, i * n + 2):
                d.line([(c, 0), (c, size)], fill=255)
                d.line([(0, c), (size, c)], fill=255)
        # chips out of the slabs' edges along the joints
        for _ in range(slabs * slabs * 10):
            i = rng.randrange(slabs + 1) * n
            t = rng.randrange(size)
            r = rng.uniform(2, 6) * size / 1024
            x, y = (i, t) if rng.random() < 0.5 else (t, i)
            at_every_wrap(size, lambda dx, dy: d.ellipse([x - r + dx, y - r + dy, x + r + dx, y + r + dy], fill=255))
        joints = tiled(joints, lambda i: i.filter(ImageFilter.GaussianBlur(0.8)))
        out = Image.composite(Image.new("RGB", (size, size), "#221f1a"), out, joints.point(lambda v: v * 9 // 10))
        height = ImageChops.subtract(height, joints.point(lambda v: v * 3 // 5))
        rough = ImageChops.add(rough, joints.point(lambda v: v * 3 // 5))
    arm = Image.merge("RGB", [hollows(height, 3, 0.6), rough, Image.new("L", (size, size), 0)])
    return {"albedo": out, "normal": normals(height, 1.2), "arm": arm}


def marble_slabs(size=1024, seed="marble_slabs"):
    """Marble laid in slabs, two a side (`marble`): a polished floor."""
    return marble(size, seed, slabs=2)


def pile(size=512, seed="pile"):
    """An old rug's pile that takes a tint: tufts of wool standing close,
    the rows of its knots showing through; worn flat and paler in patches,
    threadbare in some down to its weave (the warp and weft crossing, the
    knots' stubs between); stained brown and dark in others; darker where
    it is deep. Matt (roughness about 0.95). It tiles. The albedo, normal
    and ORM maps."""
    rng = random.Random(seed)
    # tufts: soft dots on a fine grid, jittered, of many heights
    tufts = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(tufts)
    step = size / 128
    for j in range(128):
        for i in range(128):
            x = (i + 0.5 + rng.uniform(-0.3, 0.3)) * step
            y = (j + 0.5 + rng.uniform(-0.3, 0.3)) * step
            r = step * rng.uniform(0.45, 0.7)
            v = rng.randint(130, 255)
            at_every_wrap(size, lambda dx, dy: d.ellipse([x - r + dx, y - r + dy, x + r + dx, y + r + dy], fill=v))
    tufts = tiled(tufts, lambda i: i.filter(ImageFilter.GaussianBlur(step * 0.35)))
    # the knots' rows across it, faint
    rows = Image.new("L", (size, size), 0)
    dr = ImageDraw.Draw(rows)
    for j in range(64):
        y = (j + 0.5) * size / 64
        dr.line([(0, y), (size, y)], fill=110, width=max(1, size // 512))
    rows = tiled(rows, lambda i: i.filter(ImageFilter.GaussianBlur(1.0)))
    # the weave under it: warp and weft
    weave = Image.new("L", (size, size), 0)
    dw = ImageDraw.Draw(weave)
    for k in range(96):
        c = (k + 0.5) * size / 96
        dw.line([(c, 0), (c, size)], fill=200, width=max(1, size // 400))
        dw.line([(0, c), (size, c)], fill=150, width=max(1, size // 400))
    weave = tiled(weave, lambda i: i.filter(ImageFilter.GaussianBlur(0.6)))
    # worn patches: the pile flattened and paler; threadbare ones down to
    # the weave; deep patches darker; stains
    wear = above(tile_noise(rng, size, 5), 140, 70)
    bare = above(tile_noise(rng, size, 9), 212, 22)
    deep = tile_noise(rng, size, 12)
    stain = above(tile_noise(rng, size, 6), 160, 70)
    blot = tiled(above(tile_noise(rng, size, 26), 215, 30), lambda i: i.filter(ImageFilter.GaussianBlur(2)))
    height = ImageChops.subtract(tufts, rows)
    height = ImageChops.subtract(height, wear.point(lambda v: v * 3 // 5))
    height = Image.composite(weave.point(lambda v: v // 3), height, bare)
    grey = height.point(lambda v: 140 + v * 80 // 255)
    grey = ImageChops.multiply(grey, deep.point(lambda v: 200 + v * 55 // 255))
    grey = ImageChops.add(grey, wear.point(lambda v: v // 8))
    # threadbare: the pale weave and the dark gaps between its threads
    threads = weave.point(lambda v: 110 + v * 100 // 255)
    grey = Image.composite(threads, grey, bare)
    albedo = Image.merge("RGB", [grey] * 3)
    # stains: brown, and darker blots in them
    brown = ImageChops.multiply(albedo, Image.new("RGB", (size, size), (150, 128, 100)))
    albedo = Image.composite(brown, albedo, stain.point(lambda v: v * 7 // 10))
    dark = ImageChops.multiply(albedo, Image.new("RGB", (size, size), (90, 78, 66)))
    albedo = Image.composite(dark, albedo, blot.point(lambda v: v * 55 // 100))
    rough = Image.new("L", (size, size), 240)
    rough = ImageChops.subtract(rough, wear.point(lambda v: v // 10))
    arm = Image.merge("RGB", [hollows(height, 2, 0.9), rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 1.6), "arm": arm}
