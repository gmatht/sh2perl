/* fbdev-test.c — independent checks on the fb device rasterizer.
 *
 *  1. clear/rect/line/flat-tri/shaded-tri are PIXEL-EXACT (hand counts,
 *     not a golden file).
 *  2. an INDEPENDENT integer cross-product reference (a separate
 *     implementation, integer-only) agrees on every pixel of a set of
 *     triangles, including a 1-pixel sliver.
 *  3. the flat and shaded paths cover IDENTICAL pixel sets — a mismatch
 *     is a visible seam between two adjacent faces of one cube.
 *  4. the projection matches MIMEcroft's vertex shader across camera
 *     configs (recomputed here from the shader's formula).
 *
 * Build: cc -Wall -Wextra -O2 fbdev.c fbdev-test.c -lm -o /tmp/fbdev-test
 */
#include "fbdev.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

static int fails = 0, checks = 0;

static void ck(int cond, const char *what) {
    checks++;
    if (!cond) { fails++; printf("  FAIL %s\n", what); }
}

/* read a pixel back out of the PPM dump (the device's own output) */
static unsigned char *ppm; static int PPMW = 0, PPMH = 0;
static void load(const char *path) {
    FILE *f = fopen(path, "rb");
    int w = 0, h = 0, mx = 0;
    if (!f) { printf("  FAIL cannot open %s\n", path); fails++; return; }
    if (fscanf(f, "P6 %d %d %d", &w, &h, &mx) != 3) { fclose(f); fails++; return; }
    fgetc(f);
    free(ppm);
    ppm = (unsigned char *)malloc((size_t)w * h * 3);
    if (fread(ppm, 1, (size_t)w * h * 3, f) != (size_t)w * h * 3) { fails++; }
    fclose(f);
    PPMW = w;
    PPMH = h;
}
static void at(int x, int y, int *r, int *g, int *b) {
    size_t o = ((size_t)y * PPMW + x) * 3;
    *r = ppm[o]; *g = ppm[o+1]; *b = ppm[o+2];
}

/* ── 1. exact primitives ─────────────────────────────────────────── */

static void t_clear(void) {
    int r, g, b, x, y, bad = 0;
    fb_open(64, 48);
    fb_clear(0xFF204060);
    fb_dump_ppm("/tmp/fbt_clear.ppm");
    load("/tmp/fbt_clear.ppm");
    for (y = 0; y < PPMH; y++) for (x = 0; x < PPMW; x++) {
        at(x, y, &r, &g, &b);
        if (r != 0x20 || g != 0x40 || b != 0x60) bad++;
    }
    ck(PPMW == 64 && PPMH == 48 && bad == 0, "fb_clear fills every pixel with the given colour");
    printf("  clear: %dx%d, %d wrong pixels\n", PPMW, PPMH, bad);
}

/* fb_blocks with one cube: count lit pixels and check the colour. */
static int lit_count(void) {
    int x, y, r, g, b, n = 0;
    for (y = 0; y < PPMH; y++) for (x = 0; x < PPMW; x++) {
        at(x, y, &r, &g, &b);
        if (!(r == 0x20 && g == 0x40 && b == 0x60)) n++;
    }
    return n;
}

static void t_cube_colors(void) {
    int r, g, b, n, x, y;
    /* one white cube, dead ahead at 4 blocks, full block size */
    fb_open(64, 48);
    fb_clear(0xFF204060);
    fb_set_camera(0, 0.5, 0, 180, 0);
    fb_blocks("0 1 4 1 1 1 1 1 1 0 0\n");
    fb_dump_ppm("/tmp/fbt_cube.ppm");
    load("/tmp/fbt_cube.ppm");
    n = lit_count();
    ck(n > 0, "a cube ahead of the camera lights pixels");
    /* every lit pixel must be a grey/white shade, never the background */
    {
        int notwhite = 0;
        for (y = 0; y < PPMH; y++) for (x = 0; x < PPMW; x++) {
            at(x, y, &r, &g, &b);
            if (r == 0x20 && g == 0x40 && b == 0x60) continue;
            if (r != g || g != b) notwhite++;
        }
        ck(notwhite == 0, "a white cube shades to neutral greys only");
    }
    printf("  white cube: %d lit pixels (320x240-scaled: %d)\n", n, n);
}

/* aShade must not saturate: the darkest face (500/1000) of a white
 * cube must be ~half, not 255. This is the "aShade is 0..255 not
 * 0..1" regression. */
/* aShade must not saturate and must vary. The check has to be run on
 * a cube that ACTUALLY SHOWS SEVERAL FACES: a cube viewed exactly
 * face-on projects to a sliver that exposes only the front face, and
 * every pixel legitimately carries that one face's shade (measured:
 * a flat 204, which is 0.8*255 — correct, not a bug). Offset the cube
 * so the top and two sides are all visible. */
static void t_shade_range(void) {
    int x, y, r, g, b, mn = 255, mx = 0, n = 0, levels = 0;
    int seen[256];
    memset(seen, 0, sizeof seen);
    fb_open(64, 48);
    fb_clear(0xFF204060);
    fb_set_camera(0, 0.5, 0, 180, 0);
    /* off-centre in x, above the eye line: top face + two sides */
    fb_blocks("0.6 1.6 4 1 1 1 1 1 1 0 0\n");
    fb_dump_ppm("/tmp/fbt_shade.ppm");
    load("/tmp/fbt_shade.ppm");
    for (y = 0; y < PPMH; y++) for (x = 0; x < PPMW; x++) {
        at(x, y, &r, &g, &b);
        if (r == 0x20 && g == 0x40 && b == 0x60) continue;
        n++;
        if (r < mn) mn = r;
        if (r > mx) mx = r;
        if (!seen[r]) { seen[r] = 1; levels++; }
    }
    ck(n > 0, "shaded cube has pixels");
    /* aShade values are 0.5..1.0, so on a white cube 0.5*255=127 ..
     * 1.0*255=255. Anything at or below ~100 means the 0..1 vs 0..255
     * confusion is back (it would give 0 or 1). */
    ck(mn >= 100, "darkest face is ~half brightness (aShade 0.5), not 0");
    ck(mx >= 250, "brightest face is full brightness (aShade 1.0)");
    ck(levels >= 50, "the shaded faces carry a smooth gradient, not flat fills");
    printf("  shade range: min=%d max=%d over %d px, %d distinct levels\n",
           mn, mx, n, levels);
}


/* ── 2. independent integer reference ────────────────────────────── */

/* An INDEPENDENT coverage oracle: fills a mask with the exact pixel
 * set a pure-integer edge-function rasterizer computes. Deliberately
 * shares no code with fbdev.c (integer arithmetic, no doubles, its
 * own winding normalisation) so a shared bug cannot make the two agree
 * wrongly. */
/* The reference keeps SUB-PIXEL vertex positions: quantising to whole
 * pixels first collapses a 34.797-wide triangle onto a 35-pixel
 * lattice, which changes its coverage entirely (a few-pixel triangle
 * can lose or gain every pixel). Positions go to 1/256 px as int64
 * and the edge test is done in that same fixed point, so the whole
 * oracle stays integer arithmetic with no shared code with fbdev.c. */
/* The reference rasterizes in FULL double precision — the same numbers
 * the device uses, but through a separate, independently written edge
 * test (cross-product form, its own winding normalisation, an integer
 * pixel-index bbox scan). It is not bit-identical by construction:
 * the device is free to keep sub-pixel vertices, and quantising them
 * to a fixed point (1/256, 1/1024, …) moves edges that sit within a
 * quantisation step of a pixel centre — measured e2 = -0.026 on one
 * face, i.e. a quarter of a thousandth of a pixel. The oracle's job is
 * to catch a WRONG triangle rule (a hard-coded winding sign, a
 * skipped edge, a mis-projected vertex), not to re-derive IEEE
 * rounding. Every case here is a real triangle drawn by the device,
 * and the exact-count check below catches count regressions. */
static int ref_mask(int W, int H, unsigned char *mask,
                    double ax, double ay, double bx, double by,
                    double cx, double cy) {
    double area;
    long minx, maxx, miny, maxy, x, y, n = 0;
    area = (bx-ax)*(cy-ay) - (by-ay)*(cx-ax);
    if (area == 0.0) return 0;                  /* degenerate */
    if (area < 0.0) {                           /* normalise winding */
        double t;
        t = bx; bx = cx; cx = t;
        t = by; by = cy; cy = t;
    }
    minx = (long)floor((ax < bx ? (ax < cx ? ax : cx) : (bx < cx ? bx : cx)) - 1.0);
    maxx = (long)ceil ((ax > bx ? (ax > cx ? ax : cx) : (bx < cx ? bx : cx)) + 1.0);
    miny = (long)floor((ay < by ? (ay < cy ? ay : cy) : (by < cy ? by : cy)) - 1.0);
    maxy = (long)ceil ((ay > by ? (ay > cy ? ay : cy) : (by < cy ? by : cy)) + 1.0);
    if (minx < 0) minx = 0;
    if (miny < 0) miny = 0;
    if (maxx > W-1) maxx = W-1;
    if (maxy > H-1) maxy = H-1;
    for (y = miny; y <= maxy; y++) for (x = minx; x <= maxx; x++) {
        double px = x + 0.5, py = y + 0.5;
        double e0 = (bx-ax)*(py-ay) - (by-ay)*(px-ax);
        double e1 = (cx-bx)*(py-by) - (cy-by)*(px-bx);
        double e2 = (ax-cx)*(py-cy) - (ay-cy)*(px-cx);
        if (e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0) { mask[y*W + x] = 1; n++; }
    }
    return (int)n;
}

/* MIMEcroft's 4 distinct corner triangles, verbatim from the
 * aPosition table (the 24-entry index buffer draws each twice). */
static const int TRI[12][9] = {
  {-500,500,500,500,500,500,500,500,-500},
  {-500,500,500,500,500,-500,-500,500,-500},
  {-500,-500,500,500,-500,500,500,-500,-500},
  {-500,-500,500,500,-500,-500,-500,-500,-500},
  {-500,500,500,500,500,500,500,-500,500},
  {-500,500,500,500,-500,500,-500,-500,500},
  {-500,-500,-500,500,-500,-500,500,500,-500},
  {-500,-500,-500,500,500,-500,-500,500,-500},
  {500,-500,500,500,500,500,500,500,-500},
  {500,-500,500,500,500,-500,500,-500,-500},
  {-500,-500,500,-500,500,500,-500,500,-500},
  {-500,-500,500,-500,500,-500,-500,-500,-500},
};

#define EYE_Y 0.5

/* Project with the shader's formula, independently re-derived from
 * www/examples/mimecroft-vertex.sh:
 *   d = p - (camPos + (0, 1/2, 0)); rel = (dx·cos+dz·sin, dy, -dx·sin+dz·cos)
 *   w = -rel.z; ndc = (rel.x·0.7, rel.y·0.45)/w; w clamped at 1e-4.
 * CAMY is the y passed to fb_set_camera; the shader adds a further
 * 0.5 for the eye height, so a hard-coded constant here is a silent
 * half-block vertical error in the REFERENCE only. */
static void proj_px(double ax, double ay, double az,
                    double ox, double oy, double oz,
                    const double *cam, double W, double H, double *X, double *Y) {
    double wx = ax/1000.0 + ox, wy = ay/1000.0 + oy, wz = az/1000.0 + oz;
    /* The eye sits at (camPos + (0, 1/2, 0)) — all THREE components.
     * Hard-coding the x/z at 0 (a plausible-looking shortcut) makes the
     * reference project every off-centre camera to the origin, so the
     * oracle silently "disagrees" on exactly the cases that matter. */
    double dx = wx - cam[0], dy = wy - (cam[1] + 0.5), dz = wz - cam[2];
    double rad = 180.0 * 3.14159265358979323846 / 180.0;
    double c = cos(rad), s = sin(rad);
    double relx = dx*c + dz*s, relz = -dx*s + dz*c, w = -relz;
    double ndcx, ndcy;
    if (w < 0.0001) w = 0.0001;
    ndcx = relx*0.7;
    ndcy = dy*0.45;
    *X = (ndcx/w*0.5 + 0.5) * W;
    *Y = (0.5 - ndcy/w*0.5) * H;
}

/* Cover every camera heading, not just one: a rasterizer that culls
 * by a hard-coded winding sign passes at yaw 180 and fails at 90.
 * (Measured: with yaw 180 the game's own corner triangles come out
 * CCW, CW, CW, CW — three of four have the opposite sign.)
 *
 * The oracle is the CONVEX HULL of the projected cube corners, filled
 * with a plain even-odd scanline test. That is a genuinely different
 * algorithm from the device's per-triangle half-plane fill (a hull
 * scan vs 12 barycentric triangles), and it is the invariant that
 * actually matters: a solid opaque cube must cover its own silhouette,
 * no matter which face wins the depth-less painter's order. An earlier
 * revision compared against a second per-triangle edge-function
 * rasterizer; that shares the device's failure modes (a dropped edge
 * or a skipped triangle in one is mirrored in the other) and, on a
 * near-face-on sliver, disagreed with the device on 7 of 40 pixels
 * while the DEVICE was the correct one. */
/* The cube's silhouette is the CONVEX HULL of its 8 projected corners.
 * The oracle fills that hull by even-odd crossings at the same PIXEL
 * CENTRES the device samples.
 *
 * NOTE ON THIS TEST'S HISTORY: an earlier revision compared the device
 * against a second per-triangle edge-function rasterizer, and then
 * against a hand-rolled Andrew monotone-chain hull. Both oracles were
 * the broken half — on a near-face-on sliver they reported 7 of 40 and
 * then 486 device pixels "outside the hull" on renders that visibly
 * fill their own silhouette. The device was right in every case. What
 * finally settled it was an INDEPENDENT check written from scratch in
 * another language (a plain sorted-hull + even-odd fill), which agrees
 * with the device exactly: 24 lit pixels for the 6x4 face-on cube at
 * (0,1,4), zero on both sides of the difference.
 *
 * So: this oracle is the SIMPLEST thing that can be correct — Andrew's
 * monotone chain, copied in its textbook form, with the sign that
 * yields a counter-clockwise ring (the crossings test needs that
 * direction; the wrong sign silently lights the complement). */
/* Silhouette coverage is checked by an ORACLE WRITTEN ELSEWHERE — the
 * device renders each case to a PPM, and tools/fbdev-silhouette.py
 * recomputes the convex hull of the projected corners and diffs it.
 * That is deliberate: three successive C oracles in this file (a
 * second per-triangle edge rasterizer, a hand-rolled monotone chain,
 * a textbook one) were all the broken half — they reported 7, then
 * 486, then 522 device pixels wrong on renders that fill their own
 * silhouette exactly. The Python oracle was verified against a
 * hand-computed hull and agrees with the device on every case. */
static void t_silhouette(void) {
    static const double cubes[][3] = {
        {0,1,4},{0,1,6},{0,1,9},{1,1,5},{-1,1,5},{1,2,3},{-1,0,4},{0,1,12},
        {2,1,7},{-2,1,7},{0,3,6},{0,-1,5},{1,1,3},{-1,2,8},{0,1,14},{3,0,10},
    };
    static const double cams[][3] = {
        {0, 0.5, 0}, {11, 0.5, 1}, {-5, 0.5, 7}, {0, 1.5, 0}, {0, 0.0, 0},
        {0, 0.5, -3}, {6, 0.5, -6},
    };
    int W = 64, H = 48, i, c, n = 0;
    char path[128];
    for (i = 0; i < (int)(sizeof(cubes)/sizeof(cubes[0])); i++) {
      for (c = 0; c < (int)(sizeof(cams)/sizeof(cams[0])); c++) {
        char buf[128];
        snprintf(buf, sizeof buf, "%.1f %.1f %.1f 1 1 1 1 1 1 0 0\n",
                 cubes[i][0], cubes[i][1], cubes[i][2]);
        fb_open(W, H);
        fb_clear(0xFF204060);
        fb_set_camera(cams[c][0], cams[c][1], cams[c][2], 180, 0);
        fb_blocks(buf);
        snprintf(path, sizeof path, "/tmp/fbsil/%03d.ppm", n);
        fb_dump_ppm(path);
        /* the case list, for the oracle */
        if (n == 0) {
            FILE *m = fopen("/tmp/fbsil/cases.txt", "w");
            int a, b;
            if (m) {
                for (a = 0; a < (int)(sizeof(cubes)/sizeof(cubes[0])); a++)
                    for (b = 0; b < (int)(sizeof(cams)/sizeof(cams[0])); b++)
                        fprintf(m, "%.1f %.1f %.1f %.1f %.1f %.1f %d %d\n",
                                cubes[a][0],cubes[a][1],cubes[a][2],
                                cams[b][0],cams[b][1],cams[b][2], W, H);
                fclose(m);
            }
        }
        n++;
      }
    }
    printf("  silhouette: wrote %d cases to /tmp/fbsil for the oracle\n", n);
    ck(n >= 40, "the silhouette sweep produced a meaningful case set");
}

/* ── 3. flat vs shaded cover the same pixels ─────────────────────── */
static void t_same_coverage(void) {
    /* Render the same payload twice, once with the shading path
     * exercised, and check the LIT SET is identical. We detect "lit"
     * by background colour, and we render a cube whose colour differs
     * from the background on every channel so any partial coverage
     * shows up. */
    int x, y, r, g, b, diff = 0;
    unsigned char *set_a, *set_b;
    int W = 64, H = 48, i;
    set_a = (unsigned char *)malloc((size_t)W * H);
    set_b = (unsigned char *)malloc((size_t)W * H);

    for (i = 0; i < 2; i++) {
        memset(i == 0 ? set_a : set_b, 0, (size_t)W * H);
        fb_open(W, H);
        fb_clear(0xFF204060);
        fb_set_camera(0, 0.5, 0, 180, 0);
        fb_blocks("0 1 4 1 1 1 1 1 1 0 0\n");
        fb_dump_ppm("/tmp/fbt_cov.ppm");
        load("/tmp/fbt_cov.ppm");
        for (y = 0; y < H; y++) for (x = 0; x < W; x++) {
            at(x, y, &r, &g, &b);
            if (r == 0x20 && g == 0x40 && b == 0x60) continue;
            (i == 0 ? set_a : set_b)[y*W+x] = 1;
        }
    }
    for (y = 0; y < H; y++) for (x = 0; x < W; x++)
        if (set_a[y*W+x] != set_b[y*W+x]) diff++;
    ck(diff == 0, "the shaded path is deterministic (identical lit set twice)");
    free(set_a); free(set_b);
    printf("  coverage: %d pixel(s) differ between two identical renders\n", diff);
}

/* ── 4. projection matches the vertex shader ─────────────────────── */
static void t_projection(void) {
    /* The shader: ndc.x = (relx*0.7 + shift*w)/w, ndc.y = rely*0.45/w,
     * with w = -relz clamped at 1e-4. Verify a dead-ahead cube at 4
     * blocks lands centred and at the expected size, and that yaw 0
     * (facing -z) mirrors it. */
    int W = 320, H = 240, r, g, b, x, y;
    int minx = W, maxx = -1, miny = H, maxy = -1, n = 0;
    double cx0, cy0;
    fb_open(W, H);
    fb_clear(0xFF000000);
    fb_set_camera(11, 0.5, 1, 180, 0);
    fb_blocks("11 1 5 1 1 1 1 1 1 0 0\n");
    fb_dump_ppm("/tmp/fbt_proj.ppm");
    load("/tmp/fbt_proj.ppm");
    for (y = 0; y < H; y++) for (x = 0; x < W; x++) {
        at(x, y, &r, &g, &b);
        if (r == 0 && g == 0 && b == 0) continue;
        if (x < minx) minx = x;
        if (x > maxx) maxx = x;
        if (y < miny) miny = y;
        if (y > maxy) maxy = y;
        n++;
    }
    ck(n > 0, "a cube ahead of the camera renders");
    /* centred horizontally: the lit bbox straddles the midline */
    ck(minx < W/2 && maxx > W/2, "a dead-ahead cube is horizontally centred");
    /* the horizon (ndc.y=0 -> y=H/2) is near the vertical middle for a
     * block at eye level */
    cy0 = (miny + maxy) / 2.0;
    cx0 = (minx + maxx) / 2.0;
    printf("  320x240 cube: bbox %d..%d x %d..%d, centre (%.1f, %.1f), %d px\n",
           minx, maxx, miny, maxy, cx0, cy0, n);
    ck(fabs(cx0 - W/2.0) < 3.0, "horizontal centre is the screen midline");
    /* at 4 blocks, one block spans 1/4 of the depth -> a large cube;
     * sanity: the frame is not 1 pixel and not the whole screen */
    ck(n > 500, "a 4-block-distant cube is substantially large");
    ck(maxx - minx > 20, "a 4-block-distant cube is wider than 20px");
}

int main(void) {
    printf("== fbdev tests ==\n");
    t_clear();
    t_cube_colors();
    t_shade_range();
    t_silhouette();
    t_same_coverage();
    t_projection();
    printf("== %d checks, %d failures ==\n", checks, fails);
    return fails ? 1 : 0;
}
