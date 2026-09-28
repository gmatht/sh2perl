#!/usr/bin/env python3
"""fbdev-faces.py — check (or regenerate) fbdev.c's FACE_N table.

FACE_N is the per-face OUTWARD NORMAL of the device's cube, used for
back-face culling. It is a second statement about the same geometry
that CUBE_P and CUBE_I already state, so it is the obvious place for a
transcription to go wrong — and one did: the first FACE_N assumed the
faces ran +x, -x, +y, -y, +z, -z, while the cube table's own order is
top, bottom, +z, -z, +x, -x. Culling on the wrong normal removes faces
that are visible: 118 lit pixels across the 112 silhouette cases, with
0 drawn outside the expected hull, which is exactly the signature of
"culled something that should have been there".

So the table is not maintained by hand. This derives it from the
geometry the C file actually uses and compares.

  fbdev-faces.py <fbdev.c>            check; exit 1 on a mismatch
  fbdev-faces.py <fbdev.c> --print    also print the table to paste
"""
import re
import sys

AXES = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]


def tables(src):
    m = re.search(r'CUBE_P\[72\]\s*=\s*\{(.*?)\};', src, re.S)
    if not m:
        sys.exit('fbdev-faces: CUBE_P not found')
    p = [int(x) for x in re.findall(r'-?\d+', m.group(1))]
    m = re.search(r'CUBE_I\[36\]\s*=\s*\{(.*?)\};', src, re.S)
    if not m:
        sys.exit('fbdev-faces: CUBE_I not found')
    idx = [int(x) for x in re.findall(r'-?\d+', m.group(1))]
    if len(p) != 72:
        sys.exit('fbdev-faces: CUBE_P has %d entries, expected 72' % len(p))
    if len(idx) != 36:
        sys.exit('fbdev-faces: CUBE_I has %d entries, expected 36' % len(idx))
    return p, idx


def derive(p, idx):
    """Outward normal of each of the 6 faces, from the geometry itself.

    A face of this cube is axis-aligned, so its three corners share one
    constant coordinate; the axis of that coordinate, and its sign, IS
    the outward normal. Nothing is assumed about the face ORDER — the
    faces are numbered by CUBE_I, which is the numbering draw_cube uses.
    """
    out = []
    for f in range(6):
        c = [0, 0, 0]
        for k in range(3):                     # the first triangle
            vi = idx[f * 6 + k]
            for a in range(3):
                c[a] += p[vi * 3 + a]
        c = [x / 3.0 for x in c]
        # the axis with the largest magnitude is the constant one
        ax = max(range(3), key=lambda a: abs(c[a]))
        if abs(c[ax]) < 1e-9:
            sys.exit('fbdev-faces: face %d is degenerate' % f)
        n = [0, 0, 0]
        n[ax] = 1 if c[ax] > 0 else -1
        out.append(tuple(n))
    return out


def found_table(src):
    m = re.search(r'FACE_N\[6\]\[3\]\s*=\s*\{(.*?)\};', src, re.S)
    if not m:
        return None
    rows = re.findall(r'\{([^}]*)\}', m.group(1))
    return [tuple(int(x) for x in re.findall(r'-?\d+', r)) for r in rows]


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    path = sys.argv[1]
    src = open(path).read()
    p, idx = tables(src)
    want = derive(p, idx)
    if sorted(want) != sorted(AXES):
        print('fbdev-faces: the derived normals are not the 6 axis directions:')
        for f, n in enumerate(want):
            print('  face %d -> %s' % (f, n))
        return 1
    if '--print' in sys.argv:
        print('static const int8_t FACE_N[6][3] = {')
        for n in want:
            print('    { %2d, %2d, %2d},' % n)
        print('};')

    got = found_table(src)
    if got is None:
        print('fbdev-faces: FACE_N is not present in %s' % path)
        return 1
    if len(got) != 6 or any(len(r) != 3 for r in got):
        print('fbdev-faces: FACE_N is not a 6x3 table (%d rows)' % len(got))
        return 1
    if got != want:
        print('fbdev-faces: FACE_N DOES NOT MATCH the cube geometry')
        for f in range(6):
            mark = '' if got[f] == want[f] else '   <-- WRONG'
            print('  face %d: in the file %-12s derived %-12s%s'
                  % (f, got[f], want[f], mark))
        print('')
        print('  Back-face culling on a wrong normal removes VISIBLE faces.')
        print('  Re-run with --print and paste the table above.')
        return 1
    print('fbdev-faces: FACE_N matches the cube geometry (6 faces)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
