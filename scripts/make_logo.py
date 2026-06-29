"""Generate the Maslow Desktop app logo: a CNC frame with four belts
converging on the central sled (the Maslow's defining geometry)."""
from PIL import Image, ImageDraw

SS = 4          # supersample factor for smooth edges
N = 1024 * SS   # working canvas size


def s(v):       # scale a 1024-space value into the supersampled canvas
    return int(round(v * SS))


def vgradient(size, top, bottom):
    img = Image.new("RGB", (1, size))
    for y in range(size):
        t = y / (size - 1)
        img.putpixel(
            (0, y),
            tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)),
        )
    return img.resize((size, size))


# --- background: rounded tile with a subtle vertical gradient ----------------
bg = vgradient(N, (32, 40, 58), (17, 21, 30)).convert("RGBA")
mask = Image.new("L", (N, N), 0)
ImageDraw.Draw(mask).rounded_rectangle(
    [s(8), s(8), s(1016), s(1016)], radius=s(228), fill=255
)
canvas = Image.new("RGBA", (N, N), (0, 0, 0, 0))
canvas.paste(bg, (0, 0), mask)
d = ImageDraw.Draw(canvas)

# --- geometry ----------------------------------------------------------------
# Four anchors (frame corners) and the sled hanging slightly low-center.
anchors = [(214, 300), (810, 300), (214, 762), (810, 762)]
cx, cy = 512, 548

FRAME = (58, 68, 92)
BELT = (150, 176, 214)
ANCHOR = (170, 194, 228)
RING = (79, 124, 240)
RING_FILL = (15, 19, 27)
BIT = (61, 220, 132)


def line(p0, p1, color, w):
    d.line([s(p0[0]), s(p0[1]), s(p1[0]), s(p1[1])], fill=color, width=s(w))


def dot(c, r, fill, outline=None, ow=0):
    d.ellipse(
        [s(c[0] - r), s(c[1] - r), s(c[0] + r), s(c[1] + r)],
        fill=fill,
        outline=outline,
        width=s(ow) if ow else 0,
    )


# faint frame rectangle through the anchors
d.rounded_rectangle(
    [s(214), s(300), s(810), s(762)], radius=s(26), outline=FRAME, width=s(7)
)

# belts from each anchor to the sled centre (ends hidden under the sled)
for a in anchors:
    line(a, (cx, cy), BELT, 15)

# anchor nubs
for a in anchors:
    dot(a, 22, ANCHOR)

# sled: ring + bit
dot((cx, cy), 96, RING_FILL, outline=RING, ow=20)
dot((cx, cy), 30, BIT)

# --- downscale to final size -------------------------------------------------
out = canvas.resize((1024, 1024), Image.LANCZOS)
out.save("logo-1024.png")
print("wrote logo-1024.png", out.size)
