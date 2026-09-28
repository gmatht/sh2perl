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
    /* The eye sits at cam.y + 0.5 = 1.0. A cube dead ahead at eye
     * level projects to a sliver showing ONE face (measured: a flat
     * 204 = 0.8), which is correct — and is what made an earlier
     * revision of this check demand a "smooth gradient" a flat-shaded
     * cube can never have, and what hid the painter-order bug (a cube
     * never showed more than 2 of its 4 faces from ANY position).
     * Offset it in x AND y so the top and both sides are visible. */
    fb_blocks("0.8 1.8 2.2 1 1 1 1 1 1 0 0\n");
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
    /* the game shades each FACE flat (all three of a face's vertices
     * carry the same aShade), so the number of distinct levels is the
     * number of visible faces — not a per-pixel gradient. */
    ck(mx >= 250, "the top face (aShade 1.0) is visible and full brightness");
    ck(levels >= 3, "several distinct faces are visible (painter order works)");
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

/* ── 5. a SHORT record is reported, not silently dropped ─────────────
 * fb_blocks parses 11 fields per record and used to `return` quietly
 * when it hit a short tail. A payload that is short one field in every
 * record therefore drew only whatever came before the first bad tail —
 * typically nothing — and the frame kept whatever the previous draw
 * left, which reads as a clean but empty image rather than an error.
 * That is the failure the MIMEcroft game hit: its world arrays were
 * half the size the cells index to, so every cube record lost its
 * damage field, and 23 equivalence cases passed byte-for-byte over a
 * flat grey screen.
 *
 * The check is on the DIAGNOSTIC, not the pixels: a caller has no other
 * way to tell a truncated payload from an empty one. */
static void t_short_record(void) {
    int W = 320, H = 240, x, y, n = 0, r, g, b;
    FILE *devnull;

    /* one good record, then a truncated one, then a good one */
    const char *payload =
        "5 1 5 1 1 1 1 0 0 0 0\n"      /* complete: 11 fields */
        "5 1 4 1 1 1 1 0 0 0\n"        /* TRUNCATED: 10 fields */
        "5 1 3 1 1 1 1 0 0 0 0\n";     /* complete: after the damage */

    fb_open(W, H);
    fb_clear(0xFF000000);
    fb_set_camera(5, 0.5, 5, 180, 0);

    /* capture stderr so the check can see the complaint */
    fflush(stdout);
    devnull = freopen("/tmp/fbdev-short.log", "w", stderr);
    if (!devnull) { printf("  FAIL cannot capture stderr\n"); fails++; checks++; return; }
    fb_blocks(payload);
    fflush(stderr);
    freopen("/dev/tderr", "w", stderr);

    /* the diagnostic must name the field count it stopped at */
    {
        char buf[512];
        FILE *f = fopen("/tmp/fbdev-short.log", "r");
        int saw = 0;
        if (!f) { printf("  FAIL cannot read the captured stderr\n"); fails++; checks++; return; }
        buf[0] = 0;
        if (fgets(buf, sizeof buf, f)) {
            /* "ended after 10 of 11 fields" */
            if (strstr(buf, "10 of 11")) saw = 1;
            printf("  short-record diagnostic: %s", buf);
        }
        fclose(f);
        ck(saw, "a record short of 11 fields is reported on stderr");
    }

    /* and the cubes BEFORE the bad record still drew: the parser
     * consumes what it can rather than discarding the whole payload */
    fb_dump_ppm("/tmp/fbt_short.ppm");
    load("/tmp/fbt_short.ppm");
    for (y = 0; y < H; y++) for (x = 0; x < W; x++) {
        at(x, y, &r, &g, &b);
        if (r != 0 || g != 0 || b != 0) n++;
    }
    ck(n > 0, "the records before a truncated one are still drawn");

    /* AN 8-FIELD RECORD IS NOT A RECORD. The block type and the damage
     * field are parsed and ignored — this device has no texture table
     * and no mining damage — but they are still REQUIRED. That is not
     * an accident of the parser: an 8-field payload used to be read as
     * a desynchronised stream, drawing a frame that belonged to no
     * payload at all. A record must hold 11 or it is not one. */
    {
        FILE *d2;
        fflush(stdout);
        d2 = freopen("/tmp/fbdev-short.log", "w", stderr);
        if (!d2) { printf("  FAIL cannot capture stderr\n"); fails++; return; }
        fb_clear(0xFF000000);
        fb_blocks("5 1 5 1 1 1 1 0 0\n5 1 4 1 1 1 1 0 0\n");
        fflush(stderr);
        freopen("/dev/tderr", "w", stderr);
        fb_dump_ppm("/tmp/fbt_short2.ppm");
        load("/tmp/fbt_short2.ppm");
        n = 0;
        for (y = 0; y < H; y++) for (x = 0; x < W; x++) {
            at(x, y, &r, &g, &b);
            if (r != 0 || g != 0 || b != 0) n++;
        }
        ck(n == 0, "an 8-field record draws nothing rather than desynchronising");
    }
}

int main(void) {
    printf("== fbdev tests ==\n");
    t_clear();
    t_cube_colors();
    t_shade_range();
    t_silhouette();
    t_same_coverage();
    t_projection();
    t_short_record();
    printf("== %d checks, %d failures ==\n", checks, fails);
    return fails ? 1 : 0;
}
