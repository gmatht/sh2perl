#!/usr/bin/env python3
"""fbdev-silhouette.py — the silhouette oracle for the fb0 device.

The device (sh2perl/runtime/fbdev.c) draws a cube as 12 projected
triangles. The invariant that must hold is simpler and stronger than
"the triangles tile correctly": a solid opaque cube must cover exactly
the CONVEX HULL of its 8 projected corners. This recomputes that hull
from the game's own projection and diffs it against the device's PPMs.

It lives outside the C test on purpose. Three successive C oracles were
written for this check and every one of them was the broken half — they
reported 7, then 486, then 522 device pixels wrong on renders that
visibly fill their own silhouette. A second edge-function rasterizer
shares the device's failure modes; a hand-rolled hull is easy to get
subtly wrong; and a test whose oracle is broken is worse than no test.
This one is short, was checked against a hand-computed hull, and is
easy to read.

Usage: fbdev-silhouette.py <casefile> <ppmdir>
  casefile: one "<cx> <cy> <cz> <camx> <camy> <camz> <w> <h>" per line
  ppmdir:   NNN.ppm, numbered in the same order
Exit 0 when every case matches, 1 otherwise.
"""
import math
import os
import sys

# The cube's 8 corners, in the same order as the device's aPosition
# table (±500 milli = half a block).
CORNERS = [
    (-500, -500, -500), (500, -500, -500), (500, 500, -500), (-500, 500, -500),
    (-500, -500, 500), (500, -500, 500), (500, 500, 500), (-500, 500, 500),
]

# MIMEcroft's background colour, from the test's fb_clear.
BG = (0x20, 0x40, 0x60)


def project(v, centre, cam, w, h):
    """www/examples/mimecroft-vertex.sh, transcribed.

    wx = ap·uScale/1e6 + objPos ; d = p - (camPos + (0, 1/2, 0))
    rel = (dx·cos+dz·sin, dy, -dx·sin+dz·cos) ; w_ = -rel.z
    ndc = (rel.x·0.7, rel.y·0.45)/w_ ; pixel = ((ndc+1)/2)·(W,H)
    """
    ax, ay, az = v
    wx = ax / 1000.0 + centre[0]
    wy = ay / 1000.0 + centre[1]
    wz = az / 1000.0 + centre[2]
    dx = wx - cam[0]
    dy = wy - (cam[1] + 0.5)
    dz = wz - cam[2]
    rad = math.pi                      # the cases all use yaw 180
    c, s = math.cos(rad), math.sin(rad)
    relx = dx * c + dz * s
    relz = -dx * s + dz * c
    depth = -relz
    if depth < 0.0001:                 # the shader's compile-time clamp
        depth = 0.0001
    ndcx = relx * 0.7 / depth
    ndcy = dy * 0.45 / depth
    return ((ndcx * 0.5 + 0.5) * w, (0.5 - ndcy * 0.5) * h)


def hull(points):
    """Convex hull, monotone chain. Returns a counter-clockwise ring."""
    pts = sorted(set(points))
    if len(pts) <= 2:
        return pts

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower = []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    upper = []
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return lower[:-1] + upper[:-1]


def inside(poly, px, py):
    """Even-odd crossing test at the pixel CENTRE (x+0.5, y+0.5)."""
    c = False
    n = len(poly)
    for i in range(n):
        ax, ay = poly[i]
        bx, by = poly[(i + 1) % n]
        if (ay > py) != (by > py):
            xi = ax + (py - ay) * (bx - ax) / (by - ay)
            if xi > px:
                c = not c
    return c


def read_ppm(path):
    with open(path, 'rb') as f:
        data = f.read()
    if not data.startswith(b'P6'):
        raise ValueError('%s: not a P6 PPM' % path)
    # header: P6 <w> <h> <max>\n  (whitespace separated, exactly one
    # whitespace byte after maxval)
    fields, i = [], 2
    while len(fields) < 3:
        while data[i:i + 1].isspace():
            i += 1
        j = i
        while not data[j:j + 1].isspace():
            j += 1
        fields.append(int(data[i:j]))
        i = j
    i += 1                                    # the single separator byte
    w, h, _mx = fields
    return w, h, data[i:]


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    casefile, ppmdir = sys.argv[1], sys.argv[2]
    cases = []
    with open(casefile) as f:
        for line in f:
            line = line.strip()
            if line:
                cases.append([float(x) for x in line.split()])

    bad = 0
    dev_out = ref_out = 0
    for n, c in enumerate(cases):
        cx, cy, cz, camx, camy, camz, w, h = c
        w, h = int(w), int(h)
        path = os.path.join(ppmdir, '%03d.ppm' % n)
        try:
            pw, ph, pix = read_ppm(path)
        except Exception as e:                # noqa: BLE001
            print('  %s: %s' % (path, e))
            bad += 1
            continue
        if (pw, ph) != (w, h):
            print('  case %d: PPM is %dx%d, expected %dx%d' % (n, pw, ph, w, h))
            bad += 1
            continue

        dev = set()
        for y in range(h):
            row = y * w * 3
            for x in range(w):
                o = row + x * 3
                if (pix[o], pix[o + 1], pix[o + 2]) != BG:
                    dev.add((x, y))

        poly = hull([project(v, (cx, cy, cz), (camx, camy, camz), w, h)
                     for v in CORNERS])
        ref = set()
        for y in range(h):
            for x in range(w):
                if inside(poly, x + 0.5, y + 0.5):
                    ref.add((x, y))

        extra = dev - ref
        missing = ref - dev
        dev_out += len(extra)
        ref_out += len(missing)
        if extra or missing:
            bad += 1
            print('  case %d cube(%g,%g,%g) cam(%g,%g,%g): '
                  'device %d px, %d outside hull, %d missing'
                  % (n, cx, cy, cz, camx, camy, camz, len(dev),
                     len(extra), len(missing)))

    print('  silhouette oracle: %d cases, %d device px outside the hull, '
          '%d missing' % (len(cases), dev_out, ref_out))
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
