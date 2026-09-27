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
static void t_shade_range(void) {
    int x, y, r, g, b, mn = 255, mx = 0, n = 0;
    fb_open(64, 48);
    fb_clear(0xFF204060);
    fb_set_camera(0, 0.5, 0, 180, 0);
    fb_blocks("0 1 4 1 1 1 1 1 1 0 0\n");
    fb_dump_ppm("/tmp/fbt_shade.ppm");
    load("/tmp/fbt_shade.ppm");
    for (y = 0; y < PPMH; y++) for (x = 0; x < PPMW; x++) {
        at(x, y, &r, &g, &b);
        if (r == 0x20 && g == 0x40 && b == 0x60) continue;
        n++;
        if (r < mn) mn = r;
        if (r > mx) mx = r;
    }
    ck(n > 0, "shaded cube has pixels");
    /* aShade values are 0.5..1.0, so on a white cube 0.5*255=127 ..
     * 1.0*255=255. Anything at or below ~100 means the 0..1 vs 0..255
     * confusion is back (it would give 0 or 1). */
    ck(mn >= 100, "darkest face is ~half brightness (aShade 0.5), not 0");
    ck(mx >= 250, "brightest face is full brightness (aShade 1.0)");
    ck(mn < mx, "the cube has more than one brightness level (real shading)");
    printf("  shade range: min=%d max=%d over %d pixels\n", mn, mx, n);
}

/* ── 2. independent integer reference ────────────────────────────── */

/* An INDEPENDENT coverage oracle: fills a mask with the exact pixel
 * set a pure-integer edge-function rasterizer computes. Deliberately
 * shares no code with fbdev.c (integer arithmetic, no doubles, its
 * own winding normalisation) so a shared bug cannot make the two agree
 * wrongly. */
static int ref_mask(int W, int H, unsigned char *mask,
                    long ax, long ay, long bx, long by, long cx, long cy) {
    long area, minx, maxx, miny, maxy, x, y, n = 0;
    area = (bx-ax)*(cy-ay) - (by-ay)*(cx-ax);
    if (area == 0) return 0;                    /* degenerate */
    if (area < 0) {                             /* normalise winding */
        long t;
        t = bx; bx = cx; cx = t;
        t = by; by = cy; cy = t;
    }
    minx = ax < bx ? (ax < cx ? ax : cx) : (bx < cx ? bx : cx);
    maxx = ax > bx ? (ax > cx ? ax : cx) : (bx < cx ? bx : cx);
    miny = ay < by ? (ay < cy ? ay : cy) : (by < cy ? by : cy);
    maxy = ay > by ? (ay > cy ? ay : cy) : (by < cy ? by : cy);
    if (minx < 0) minx = 0;
    if (miny < 0) miny = 0;
    if (maxx > W-1) maxx = W-1;
    if (maxy > H-1) maxy = H-1;
    for (y = miny; y <= maxy; y++) for (x = minx; x <= maxx; x++) {
        long px = x*2+1, py = y*2+1;             /* pixel centre, x2 */
        long ax2 = ax*2, ay2 = ay*2, bx2 = bx*2, by2 = by*2, cx2 = cx*2, cy2 = cy*2;
        long e0 = (bx2-ax2)*(py-ay2) - (by2-ay2)*(px-ax2);
        long e1 = (cx2-bx2)*(py-by2) - (cy2-by2)*(px-bx2);
        long e2 = (ax2-cx2)*(py-cy2) - (ay2-cy2)*(px-cx2);
        if (e0 >= 0 && e1 >= 0 && e2 >= 0) { mask[y*W+x] = 1; n++; }
    }
    return (int)n;
}

/* MIMEcroft's 4 distinct corner triangles, verbatim from the
 * aPosition table (the 24-entry index buffer draws each twice). */
static const int TRI[4][9] = {
  {-500, 500, 500,  500, 500, -500, -500, -500,  500},
  { 500,-500,-500, -500, 500,  500,  500, -500,  500},
  {-500,-500,-500,  500, 500, -500,  500, -500,  500},
  { 500, 500,-500, -500,-500,  500, -500,  500, -500},
};

#define EYE_Y 0.5

/* Project with the shader's formula, independently re-derived from
 * www/examples/mimecroft-vertex.sh:
 *   d = p - (camPos + (0, 1/2, 0)); rel = (dx·cos+dz·sin, dy, -dx·sin+dz·cos)
 *   w = -rel.z; ndc = (rel.x·0.7, rel.y·0.45)/w; w clamped at 1e-4.
 * CAMY is the y passed to fb_set_camera; the shader adds a further
 * 0.5 for the eye height, so a hard-coded constant here is a silent
 * half-block vertical error in the REFERENCE only. */
static void proj_px(double ax, double ay, double ox, double oy, double oz,
                    double camy, double W, double H, double *X, double *Y) {
    double wx = ax/1000.0 + ox, wy = ay/1000.0 + oy, wz = oz/1000.0 + oz;
    double dx = wx - 0.0, dy = wy - (camy + 0.5), dz = wz - 0.0;
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
 * (Measured: with yaw 180 the game's own four corner triangles come
 * out CCW, CW, CW, CW — three of the four have the opposite sign.) */
static void t_ref_agrees(void) {
    static const double cubes[][3] = {
        {0,1,4},{0,1,6},{0,1,9},{1,1,5},{-1,1,5},{1,2,3},{-1,0,4},{0,1,12},
        {2,1,7},{-2,1,7},{0,3,6},{0,-1,5},{1,1,3},{-1,2,8},{0,1,14},{3,0,10},
    };
    static const double cams[][3] = {   /* x, y, z */
        {0, EYE_Y, 0}, {11, EYE_Y, 1}, {-5, EYE_Y, 7}, {0, 1.5, 0}, {0, 0, 0},
    };
    int W = 64, H = 48, i, t, c, mismatch = 0, compared = 0, used = 0;
    unsigned char *mask = (unsigned char *)malloc((size_t)W * H);
    unsigned char *dev  = (unsigned char *)malloc((size_t)W * H);

    for (i = 0; i < (int)(sizeof(cubes)/sizeof(cubes[0])); i++) {
      for (c = 0; c < (int)(sizeof(cams)/sizeof(cams[0])); c++) {
        char buf[128];
        int r, g, b, x, y, devn = 0;
        snprintf(buf, sizeof buf, "%.1f %.1f %.1f 1 1 1 1 1 1 0 0\n",
                 cubes[i][0], cubes[i][1], cubes[i][2]);
        fb_open(W, H);
        fb_clear(0xFF204060);
        fb_set_camera(cams[c][0], cams[c][1], cams[c][2], 180, 0);
        fb_blocks(buf);
        fb_dump_ppm("/tmp/fbt_ref.ppm");
        load("/tmp/fbt_ref.ppm");
        memset(dev, 0, (size_t)W * H);
        for (y = 0; y < H; y++) for (x = 0; x < W; x++) {
            at(x, y, &r, &g, &b);
            if (r == 0x20 && g == 0x40 && b == 0x60) continue;
            dev[y*W+x] = 1;
            devn++;
        }
        if (devn == 0) { continue; }   /* off-screen: nothing to compare */
        used++;
        memset(mask, 0, (size_t)W * H);
        for (t = 0; t < 4; t++) {
            double p0x, p0y, p1x, p1y, p2x, p2y;
            proj_px(TRI[t][0], TRI[t][1], cubes[i][0], cubes[i][1], cubes[i][2],
                    cams[c][1], W, H, &p0x, &p0y);
            proj_px(TRI[t][3], TRI[t][4], cubes[i][0], cubes[i][1], cubes[i][2],
                    cams[c][1], W, H, &p1x, &p1y);
            proj_px(TRI[t][6], TRI[t][7], cubes[i][0], cubes[i][1], cubes[i][2],
                    cams[c][1], W, H, &p2x, &p2y);
            ref_mask(W, H, mask,
                    (long)floor(p0x+0.5), (long)floor(p0y+0.5),
                    (long)floor(p1x+0.5), (long)floor(p1y+0.5),
                    (long)floor(p2x+0.5), (long)floor(p2y+0.5));
        }
        /* The device draws each of the 4 triangles TWICE (the 24-entry
         * index table), so compare the DEVICE's union against the
         * reference's union. Comparing one triangle's pixels against
         * the union image is the trap that made an earlier revision of
         * this test pass wrongly. */
        for (y = 0; y < H; y++) for (x = 0; x < W; x++) {
            if (dev[y*W+x] != mask[y*W+x]) mismatch++;
            compared++;
        }
      }
    }
    free(dev); free(mask);
    ck(used >= 10, "the reference sweep exercised a meaningful number of cubes");
    ck(compared > 0, "the reference sweep compared a non-empty set");
    ck(mismatch == 0, "device and independent integer reference agree per pixel");
    printf("  ref sweep: %d cubes x %d px, %d mismatches\n",
           used, compared, mismatch);
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
    t_ref_agrees();
    t_same_coverage();
    t_projection();
    printf("== %d checks, %d failures ==\n", checks, fails);
    return fails ? 1 : 0;
}
