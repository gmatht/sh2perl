#!/usr/bin/env python3
"""fbdev-frag.py — an independent ORACLE for the block fragment stage.

Written from www/bin/mimecroft.sh's own emit_fragment_shader, not from
fbdev.c. Two programs that agree because they share a bug is not a
test, and the fragment stage is the one piece of the device that is a
TRANSCRIPTION: the arithmetic, the order, and the integer divides all
have to be the original's, and the only way to know they are is a
second implementation written from the other direction.

The order this implements, and the order is load-bearing:

    r = tint(aShade)            the vertex shader's vcolor
    r = r * tex_r / 128         the block texture through the tint
    r = r * 90 / 100            CRT scanline  (py % 6 == 0)     <- before
    r -= (r - cr_r) * mix / 128 the crack, mixed by damage        <- the crack
    corruption streak
    r -= r * dim / 256          the vignette, multiplicative
    clamp at 0

Every divide is an INTEGER divide, because the sh2glsl generator emits
mediump ints. A float here is smooth and wrong.

It is fed the device's own PPM and answers per pixel, so it checks the
rasterizer, the uv interpolation, the crack mix and the effects at once
— a single wrong pixel anywhere is reported with its coordinates.
"""
import sys


def fract(x):
    # fract() for the 0..1 case the shader samples with, floor-safe for
    # negatives (a uv interpolated past 1 on a clipped face is normal).
    return x - int(x // 1)


class Tex:
    def __init__(self, size, rgba=False):
        self.size = size
        self.rgba = rgba
        self.px = [[(0, 0, 0, 255)] * size for _ in range(size)]

    def set(self, x, y, r, g, b, a=255):
        self.px[y][x] = (r, g, b, a)

    def sample(self, u, v):
        u, v = fract(u), fract(v)
        iu = min(int(u * self.size), self.size - 1)
        iv = min(int(v * self.size), self.size - 1)
        return self.px[iv][iu]


def frag(tex, crack, tr, tg, tb, shade, damage, px, py, u, v,
         crt=False, corrupt=False, vw=320, vh=240, spec_thresh=450,
         corrupt_mod=97):
    """One pixel, transcribed from emit_fragment_shader."""
    # vcolor carries aShade, which the original's vertex shader multiplies
    # in BEFORE the fragment stage runs.
    tr = tr * shade // 1000
    tg = tg * shade // 1000
    tb = tb * shade // 1000
    tr = max(0, min(255, tr))
    tg = max(0, min(255, tg))
    tb = max(0, min(255, tb))

    t = tex.sample(u, v)
    r = tr * t[0] // 128
    g = tg * t[1] // 128
    b = tb * t[2] // 128

    # CRT scanline, BEFORE the crack blend
    if crt and py % 6 == 0:
        r = r * 90 // 100
        g = g * 90 // 100
        b = b * 90 // 100

    if damage > 0 and crack is not None:
        c = crack.sample(u, v)
        mix = damage * c[3] // 4
        if mix > 127:
            mix = 127
        r = r - (r - c[0]) * mix // 128
        g = g - (g - c[1]) * mix // 128
        b = b - (b - c[2]) * mix // 128

    if corrupt:
        # (fx*7 + fy*13) % 97 == 0 is the original's, and the modulus
        # comes from the spec so that the device's copy is what is being
        # checked. It was MISSING from
        # this oracle entirely — the branch had been dropped while the
        # function was edited — and because the cases that set
        # `corrupt: 1` were answered by a function with no corruption in
        # it, the device's corruption output was compared against a
        # stage that did not exist, and a device with a WRONG hash
        # survived. A missing branch in an oracle is invisible: it makes
        # the oracle agree with the device whenever the device is right
        # and disagree for the wrong reason when it is not.
        if (px * 7 + py * 13) % corrupt_mod == 0:
            r, g, b = 255, g // 2, b // 2

    if crt:
        vx, vy = abs(px - vw // 2), abs(py - vh // 2)
        edge = vx + vy
        # The threshold and the cap come from the SPEC, not from a
        # constant in this file. They were literals here, which made
        # this half of the check unfalsifiable for the vignette: a
        # device with the threshold at 300 and one with it at 450 both
        # "agreed" with the oracle, because the oracle had been changed
        # by the same edit. A constant that both sides of a comparison
        # hold is not a test of that constant.
        thresh = spec_thresh
        if edge > thresh:
            dim = min(edge - thresh, 30)
            r -= r * dim // 256
            g -= g * dim // 256
            b -= b * dim // 256

    return max(0, r), max(0, g), max(0, b)


def read_ppm(path):
    d = open(path, 'rb').read()
    # P6\n<w> <h>\n255\n
    i = d.index(b'255\n') + 4
    header = d[:i].split()
    w, h = int(header[1]), int(header[2])
    return w, h, d[i:]


def emit_answers(spec, out_path):
    """Answer the spec's points and write one `r g b` per line.

    This is the mode the C suite drives, and it is deliberately the
    SIMPLEST possible one: the oracle is handed the same (u, v, shade,
    damage, tint, flags) the device was handed and answers with the
    fragment program alone. It knows nothing about the rasterizer, the
    projection, the cube table or the guard band — so it cannot agree
    with a broken device by sharing a bug in any of them, and when the
    two disagree the disagreement is in the fragment arithmetic, which
    is the thing being checked.
    """
    tex = Tex(spec['size'])
    for y, row in enumerate(spec['block']):
        for x, (r, g, b) in enumerate(row):
            tex.set(x, y, r, g, b)
    crack = None
    if 'crack' in spec:
        crack = Tex(spec['size'], rgba=True)
        for y, row in enumerate(spec['crack']):
            for x, (r, g, a) in enumerate(row):
                crack.set(x, y, r, g, 0, a)

    # Per-point overrides, so one spec can ask about several shades and
    # damage levels in one pass. Absent, the spec's single values are
    # used — and a spec that relies only on those is testing a formula
    # with no cases, which is a formula nobody has checked.
    # Per-case block texture: a case names a SLOT, and the oracle must
    # sample the image in THAT slot. With one texture for every case,
    # a case that names slot 1 is compared against slot 0's image — a
    # disagreement that looks like the device ignoring its texture.
    blocks = {'0': Tex(spec['size'])}
    for y, row in enumerate(spec['block']):
        for x, (r, g, b) in enumerate(row):
            blocks['0'].set(x, y, r, g, b)
    if 'block1' in spec:
        t1 = Tex(spec['size'])
        for y, row in enumerate(spec['block1']):
            for x, (r, g, b) in enumerate(row):
                t1.set(x, y, r, g, b)
        blocks['1'] = t1
    texes = spec.get('texes') or [0] * len(spec['pixels'])

    tints = spec.get('tints') or [spec['tint']] * len(spec['pixels'])
    shades = spec.get('shades') or [spec['shade']] * len(spec['pixels'])
    damages = spec.get('damages') or [spec['damage']] * len(spec['pixels'])
    crts = spec.get('crts') or [spec.get('crt', False)] * len(spec['pixels'])
    cors = spec.get('corrupts') or [spec.get('corrupt', False)] * len(spec['pixels'])

    # Per-point canvas size. The vignette's threshold is a function of
    # the SURFACE, not of the shader: on a 320x240 canvas the far
    # corner's |dx|+|dy| is 280, which never reaches the 450 the
    # original uses, so a vignette case on a 320x240 surface asks a
    # question whose answer is "no effect" and cannot tell a correct
    # threshold from a wrong one. The cases that mean to exercise it
    # carry a bigger canvas — the game's own DISPLAY rows.
    sizes = (spec.get('sizes')
             or [[spec.get('w', 320), spec.get('h', 240)]] * len(spec['pixels']))

    out = []
    for i, p in enumerate(spec['pixels']):
        x, y, u, v = p
        t = tints[i] if i < len(tints) else tints[0]
        sz = sizes[i] if i < len(sizes) else sizes[0]
        tx = blocks.get(str(texes[i]), tex) if i < len(texes) else tex
        out.append(frag(tx, crack, t[0], t[1], t[2],
                        shades[i], damages[i], x, y, u, v,
                        crt=crts[i], corrupt=cors[i],
                        vw=sz[0], vh=sz[1],
                        # the vignette threshold, from the spec — the
                        # original bakes it into the generated program
                        # (edge > 450), and a constant held by BOTH
                        # sides of a comparison is not a test of it
                        spec_thresh=spec.get('vignette_edge', 450),
                        corrupt_mod=spec.get('corrupt_mod', 97)))
    with open(out_path, 'w') as f:
        for (r, g, b) in out:
            f.write("%d %d %d\n" % (r, g, b))
    return len(out)


def main():
    if '--emit' in sys.argv:
        i = sys.argv.index('--emit')
        import json
        spec = json.load(open(sys.argv[i + 1]))
        n = emit_answers(spec, sys.argv[i + 2])
        print("oracle: %d points" % n)
        return 0

    """Compare the device's frame against the oracle for a known cube.

    Usage: fbdev-frag.py <frame.ppm> <spec.json>

    The spec names the block texture, the crack texture, the tint, the
    damage and the camera the device drew with, so the oracle and the
    device are asked the SAME question rather than the oracle
    re-guessing the geometry.
    """
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    import json
    w, h, px = read_ppm(sys.argv[1])
    spec = json.load(open(sys.argv[2]))

    tex = Tex(spec['size'])
    for y, row in enumerate(spec['block']):
        for x, (r, g, b) in enumerate(row):
            tex.set(x, y, r, g, b)
    crack = None
    if 'crack' in spec:
        crack = Tex(spec['size'], rgba=True)
        for y, row in enumerate(spec['crack']):
            for x, (r, g, a) in enumerate(row):
                crack.set(x, y, r, g, 0, a)

    # The lit pixels and their colours, from the frame itself; the
    # oracle is asked "what colour should a pixel of this face be?" and
    # answers for every pixel it was told about. Coverage is checked
    # separately by the C suite — this oracle is about COLOUR.
    mism = 0
    checked = 0
    for p in spec['pixels']:
        x, y, u, v = p
        got = px[(y * w + x) * 3:(y * w + x) * 3 + 3]
        want = frag(tex, crack,
                    spec['tint'][0], spec['tint'][1], spec['tint'][2],
                    spec['shade'], spec['damage'], x, y, u, v,
                    crt=spec.get('crt', False),
                    corrupt=spec.get('corrupt', False),
                    vw=w, vh=h)
        checked += 1
        if tuple(got) != want:
            mism += 1
            if mism <= 8:
                print("  MISMATCH at (%d,%d) uv=(%.4f,%.4f): device=%s oracle=%s"
                      % (x, y, u, v, tuple(got), want))
    print("oracle: %d pixels checked, %d mismatch" % (checked, mism))
    return 1 if mism else 0


if __name__ == '__main__':
    sys.exit(main())
