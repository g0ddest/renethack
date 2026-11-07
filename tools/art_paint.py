"""Textures painted from those fetched (tools/fetch_art.py runs it: the
"painted" recipe in client/godot/art/sources.json): a corpse's skin from a
living one, a lich's dried over its skull, linen wound round a body, a coat
left pale to take a tint, a dark coat lit up.

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


def scales(size=512, rows=8, seed="scales"):
    """Overlapping scales that take a tint, in rows offset by half a scale,
    each a rounded shield keeled down its middle, its free edge down the
    image over the root of the one below; dark in the gaps between. Their
    middles glossier than their edges (roughness about 0.6 to 0.8). It
    tiles. The albedo, normal and ORM maps."""
    rng = random.Random(seed)
    cols = rows
    cw, rh = size / cols, size / rows
    # one scale: a dome shaped like a shield, its keel, and where it is
    sw, sh = round(cw * 1.16), round(rh * 1.7)
    dome = Image.new("L", (sw, sh), 0)
    mask = Image.new("L", (sw, sh), 0)
    px, pm = dome.load(), mask.load()
    for y in range(sh):
        for x in range(sw):
            u = (x + 0.5) / sw * 2 - 1
            t = (y + 0.5) / sh
            # narrower toward its root (the top), round at its free edge
            half = 0.62 + 0.38 * math.sin(min(1.0, t * 1.25) * math.pi / 2)
            edge = 1 - (max(0.0, t - 0.55) / 0.45) ** 2
            if abs(u) > half * math.sqrt(max(0.0, edge)):
                continue
            r = abs(u) / max(1e-3, half * math.sqrt(max(1e-3, edge)))
            rise = 0.35 + 0.65 * t
            keel = max(0.0, 1 - abs(u) * 6) * 0.25 * t
            px[x, y] = max(1, round(255 * (rise * (1 - r ** 3) * 0.8 + keel)))
            pm[x, y] = 255
    # the lowest row first, each over the one below it, on a taller sheet
    # (two rows more above and below) so the overlaps run on across the
    # seam; the rows repeat (an even number of them)
    pad = 2 * rh
    jitter = [[rng.uniform(-1, 1) for _ in range(cols)] for _ in range(rows)]
    tall = Image.new("L", (size, round(size + 2 * pad)), 0)
    for r in range(rows + 2, -3, -1):
        for c in range(cols):
            x = round((c + 0.5 * (r % 2)) * cw - sw / 2 + jitter[r % rows][c])
            y = round(pad + (r - 0.7) * rh)
            for dx in (-size, 0, size):
                tall.paste(dome, (x + dx, y), mask)
    height = tall.crop((0, round(pad), size, round(pad) + size))
    height = height.filter(ImageFilter.GaussianBlur(0.8))
    mottle = noise(rng, (size, size), 5)
    light = ImageChops.add(height.point(lambda v: round(110 + v * 0.5)), mottle.point(lambda v: v * 30 // 255))
    albedo = Image.merge("RGB", [light] * 3)
    ao = hollows(height, 8, 0.9)
    rough = height.point(lambda v: round(205 - v * 0.2))
    arm = Image.merge("RGB", [ao, rough, Image.new("L", (size, size), 0)])
    return {"albedo": albedo, "normal": normals(height, 3.0), "arm": arm}


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

    texels = grown(bake(tris, size, paint), size, 6)
    albedo = Image.new("RGB", (size, size), lacquer)
    arm = Image.new("RGB", (size, size), (255, 51, 0))
    a, r = albedo.load(), arm.load()
    height = {}
    for (px, py), (colour, rough, ao, h, density) in texels.items():
        g = 0.92 + 0.16 * grain[px, py] / 255
        a[px, py] = tuple(min(255, round(v * g)) for v in colour)
        r[px, py] = (round(255 * ao), round(255 * rough), 0)
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
