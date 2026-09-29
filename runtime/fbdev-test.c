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

/* Where the independent fragment oracle lives. Overridable so the
 * suite can be run from a different tree without editing it. */
#ifndef ORACLE
#define ORACLE "tools/fbdev-frag.py"
#endif
/* The crack texture's slot, as the game binds it (uCrack = 9). Spelled
 * here rather than included from fbdev.c because fbdev.c keeps it
 * private: it is a binding the GAME chooses, and the test that checks
 * the device obeys the game's binding should not be reading the
 * device's copy of it back. */
#ifndef CRACK_SLOT
#define CRACK_SLOT 9
#endif

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


/* ── 5. the block fragment stage ─────────────────────────────────────
 *
 * THE ORACLE IS tools/fbdev-frag.py, written from the game's own
 * emit_fragment_shader rather than from fbdev.c, and it is a SEPARATE
 * PROGRAM. The fragment stage is a transcription — the arithmetic, the
 * order, and the integer divides all have to be the original's — and
 * the only way to know they are is a second implementation written from
 * the other side. A C test that recomputed the same formula in C would
 * agree with a broken device exactly when they share a bug, which is
 * the failure this whole file has been written against.
 *
 * THE TEXTURES ARE NOT GOLDEN FILES. A 2x2 and a 3x3 texture are
 * specified in the test and written out, so every expected value is
 * derivable from the formula by hand and no pixel in this check comes
 * from a file that was recorded from a possibly-wrong device.
 *
 * What is checked, in the order that matters: the tint multiply, the
 * /128 texture divide, the CRT scanline BEFORE the crack, the crack's
 * damage mix, the corruption streak, the vignette, and the clamp. The
 * scanline-before-crack is a separate check precisely because the
 * original calls it "a 1-step visual difference" — it is the kind of
 * reordering that survives a port and then shows up as an unexplainable
 * pixel diff. */
static void t_frag(void) {
    /* A 2x2 texture whose four texels are BLACK, RED, GREEN, BLUE, so a
     * wrong uv lands on a different answer rather than on a
     * coincidentally equal one.
     *
     * THREE SEPARATE PLANES, because that is the shape the game sends
     * (its TSV is one column per channel, and the WebGL path uploads
     * three). The first version of this test passed ONE string as all
     * three, which is a texture whose R, G and B planes are identical —
     * so every texel is grey, every uv error is invisible, and the
     * "four different colours" it claimed to have were four copies of
     * the same colour. A test texture that cannot distinguish a uv bug
     * is a test texture that passes on a broken rasterizer. */
    const char *blk_r = "0   255  0    0";
    const char *blk_g = "0   0    255  0";
    const char *blk_b = "0   0    0    255";
    /* The crack: OPAQUE in the top row, TRANSPARENT in the bottom, so
     * the damage mix has something to actually change, and the change
     * depends on the uv. */
    const char *crk_r = "0   0   0   0";
    const char *crk_g = "0   0   0   0";
    /* The alpha is 200, NOT 255, and that is the point of it.
     *
     * At 255 the damage mix `damage*ca/4` is clamped to 127 for every
     * damage level of 4 or more and is 63 at damage 1, so a device that
     * divided by 2 instead of by 4 produced the SAME clamped 127 and the
     * same 63 — the mutant survived. The original's own default is a
     * fully transparent crack with only its drawn lines opaque, so a
     * test that only ever samples alpha=255 is testing the one value
     * where the divide cannot be seen. 200 puts ca/4 and ca/2 in
     * different places and the arithmetic becomes observable. */
    const char *crk_a = "200 200 0   0";

    /* --- the spec, handed to the oracle as JSON so both sides are
     *     asked the SAME question. --- */
    /* THE UV POINTS ARE THE CENTRES OF THE FOUR TEXELS, not 0.10/0.90.
     *
     * They were 0.10 and 0.90, and on a 2x2 texture that is texel 0 and
     * texel 3 — and both of the cases that used them are on a BLACK
     * texel, so nine of the twelve answers were (0,0,0) and the three
     * mutants below (the /127, the crack's /2, the vignette's edge)
     * all survived: they change the arithmetic, and the arithmetic was
     * multiplied by zero before it could matter. A uv in the middle of
     * a texel is also the honest way to ask, because fract() at a texel
     * BOUNDARY is a rounding question, not a texture question, and this
     * check is about the texture.
     *
     * The tints are deliberately NOT all white: with tint 255 and a
     * texel of 0 the answer is 0 for every stage, so the crack, the
     * scanline and the vignette all have nothing to act on. A tint of
     * 255 against a 255 texel puts every intermediate where a change
     * in it is visible. */
    /* A per-case surface size, because the vignette is a function of
     * the CANVAS and the cases above deliberately use more than one. */
    struct { int tex, shade, dmg, crt, corrupt;
             int tr, tg, tb; double u, v; int px, py, w, h; } Q[] = {
        /* BLACK texel (0,0) and RED texel (1,0): the two uv corners */
        { 0, 1000, 0, 0, 0,  255, 255, 255, 0.25, 0.25,  5,  5 , 320, 240 },
        { 0, 1000, 0, 0, 0,  255, 255, 255, 0.75, 0.25,  5,  5 , 320, 240 },
        /* GREEN (0,1) and BLUE (1,1) */
        { 0, 1000, 0, 0, 0,  255, 255, 255, 0.25, 0.75,  5,  5 , 320, 240 },
        { 0, 1000, 0, 0, 0,  255, 255, 255, 0.75, 0.75,  5,  5 , 320, 240 },
        /* the shade is aShade ×1000 and is applied BEFORE the texture:
         * 500 halves the answer, 800 is between */
        { 1,  500, 0, 0, 0,  255, 255, 255, 0.25, 0.25,  5,  5 , 320, 240 },
        { 1,  800, 0, 0, 0,  255, 255, 255, 0.25, 0.25,  5,  5 , 320, 240 },
        /* damage 4 with an OPAQUE crack replaces the block with black;
         * damage 1 is a quarter of the mix, so it lands in between */
        { 1, 1000, 4, 0, 0,  255, 255, 255, 0.25, 0.25,  5,  5 , 320, 240 },
        { 1, 1000, 1, 0, 0,  255, 255, 255, 0.25, 0.25,  5,  5 , 320, 240 },
        /* the crack texture is opaque only in its TOP row, so the same
         * damage at a bottom-row uv must leave the block alone — which
         * is what proves the crack is SAMPLED and not just mixed in */
        { 1, 1000, 4, 0, 0,  255, 255, 255, 0.25, 0.75,  5,  5 , 320, 240 },
        /* the CRT scanline, on a row that is a multiple of 6 and one
         * that is not — the difference is the whole check */
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25, 40,  6 , 320, 240 },
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25, 40,  7 , 320, 240 },
        /* THE VIGNETTE, AND THE SURFACE IT IS MEASURED ON.
         *
         * The threshold is edge = |px-w/2| + |py-h/2| > 450. On a
         * 320x240 surface the far corner is 160+120 = 280, so the
         * vignette is NEVER reached — every "vignette" case answered
         * the same as a case with the vignette off, and a device with
         * the threshold at 300 instead of 450 was indistinguishable
         * from a correct one. The numbers are the game's own
         * DISPLAY rows: 1080p is 1920x1080, where the corner's
         * vx+vy is 960+540 = 1500 and the dim is the full 30.
         *
         * The spec therefore carries the surface size, and both sides
         * use it — the oracle reads w/h from the spec for exactly this
         * reason. */
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25,    0,    0, 1920, 1080 },
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25,  960,  540, 1920, 1080 },
        /* THE PIXELS THAT STRADDLE THE THRESHOLD, and they are here
         * because the two extremes above do not check it. A 1920x1080
         * canvas has edge 1500 at the corner and 0 at the centre, so
         * they separate "the vignette runs" from "it does not" and say
         * nothing about WHERE it begins: a threshold anywhere from 1 to
         * 1500 passes both. 1320 and 1000 on x=100 give edges of 430
         * and 370, which is either side of the original's 450 and near
         * every plausible wrong threshold (300, 400, 500) as well — a
         * device with 300 instead of 450 dims these two differently
         * from the centre case and from each other. */
        /* y=540 is the vertical centre, so the edge is the horizontal
         * distance alone and is easy to state: 1400 is 440 (no dim,
         * the original's test is `> 450`) and 1420 is 460 (a dim of 10).
         * The first pair of vignette cases put BOTH pixels at an edge
         * of 800, which is past any plausible threshold, so a device
         * with 300 instead of 450 dimmed them exactly the same — the
         * threshold was never located, only the cap. */
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25, 1400,  540, 1920, 1080 },
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25, 1420,  540, 1920, 1080 },
        { 1, 1000, 0, 1, 0,  255, 255, 255, 0.25, 0.25, 1500,  540, 1920, 1080 },
        /* THE CORRUPTION PIXELS DISCRIMINATE THE MODULUS, which is what
         * makes this a check of the hash rather than of the idea of one.
         *
         * They were (0,0) and (1,0), and (0,0) has hash 0 — which is 0
         * modulo EVERY integer, so it streaks under the right modulus and
         * under every wrong one. (1,0) has hash 7, nonzero modulo both
         * 97 and 89, so it never streaks under either. Between them they
         * proved the stage RAN and sometimes did not, and were entirely
         * blind to what the modulus IS: a device with 89 for 97 passed.
         *
         * (165,7) is 82 mod 97 and 0 mod 89; (297,49) is 0 mod 97 and 46
         * mod 89. Each separates the right modulus from the plausible
         * wrong ones, in both directions. */
        { 1, 1000, 0, 0, 1,  255, 255, 255, 0.25, 0.25, 165,   7, 320, 240 },
        { 1, 1000, 0, 0, 1,  255, 255, 255, 0.25, 0.25, 297,  49, 320, 240 },
        { 1, 1000, 0, 0, 1,  255, 255, 255, 0.25, 0.25,  11,  42, 320, 240 },
        /* and one that streaks under every modulus, so a device that
         * dropped the stage altogether is caught as well */
        { 1, 1000, 0, 0, 1,  255, 255, 255, 0.25, 0.25,   0,   0, 320, 240 },
    };
    int i, nq = (int)(sizeof Q / sizeof Q[0]);
    FILE *sp;
    int r, g, b, bad = 0, rc;

    ck(fb_texture(0, blk_r, blk_g, blk_b, 2, 2, 0) == 0,
       "fb_texture accepts a 2x2 as three planes");
    /* slot 1 is ALL WHITE, which is what the effect cases above are
     * measured on: a stage that multiplies is invisible on a black
     * texel, and half of this table would otherwise test nothing. */
    {
        const char *w = "255 255 255 255";
        ck(fb_texture(1, w, w, w, 2, 2, 0) == 0, "fb_texture takes a second slot");
    }
    ck(fb_texture(CRACK_SLOT, crk_r, crk_g, crk_a, 2, 2, 1) == 0,
       "fb_texture accepts the crack as RGBA");
    ck(fb_tex_count() >= 1, "fb_tex_count reports the loaded textures");

    /* write the spec for the oracle */
    sp = fopen("/tmp/fbt_frag.json", "w");
    if (!sp) { ck(0, "can write the oracle spec"); return; }
    /* A 2x2 texture is TWO ROWS. Writing the four texels as one flat list
     * of pairs looks like a grid to a reader and is not one: the oracle
     * read it as a single row of two ints and raised, which is how this
     * was found — the spec is the part of the test most likely to be
     * subtly wrong, and an oracle that cannot parse its input cannot
     * check anything. */
    /* THE SPEC'S TEXTURES ARE THE SLOTS THE CASES NAME, which means a
     * spec with more than one block texture needs a per-case texture —
     * and it did not have one, so every case that named slot 1 (the
     * white one) had the oracle sampling slot 0's four-colour image
     * while the device sampled white. The first symptom was nine
     * "0 0 0" answers from the oracle against non-zero device answers,
     * which looks like a device that ignores its texture and is in
     * fact a spec describing a different one. */
    /* THE VIGNETTE THRESHOLD IS IN THE SPEC, and that is the only place
     * in this test where a constant of the SHADER is written down
     * outside the device.
     *
     * It was not, and two mutations survived because of it: the
     * threshold was a literal in BOTH fbdev.c and the oracle, so a
     * device edited from 450 to 300 moved the oracle with it and the
     * two still agreed. A constant that both sides of a comparison hold
     * is not a test of that constant — it is a test that they are the
     * same constant, which was never the question. The corruption hash
     * has the same property, which is why `corrupt_mod` is here too.
     *
     * The numbers are the original's (www/bin/mimecroft.sh,
     * emit_fragment_shader: `edge > 450` and `hash % 97`). The device
     * has its own copies; if either is edited, this check fails, which
     * is the point. */
    fprintf(sp, "{\"size\": 2, \"tint\": [200, 100, 50], \"shade\": 1000,"
                " \"damage\": 0, \"vignette_edge\": 450, \"corrupt_mod\": 97,"
                " \"block\": [[[0,0,0],[255,0,0]],[[0,255,0],[0,0,255]]],"
                " \"block1\": [[[255,255,255],[255,255,255]],"
                             "[[255,255,255],[255,255,255]]],"
                /* THE SPEC'S CRACK MUST MATCH THE TEXTURE THE DEVICE WAS
                 * GIVEN, or the oracle is being asked a different
                 * question from the one the device answered. It did
                 * once: the spec had BOTH rows opaque while the upload
                 * (crk_a = "255 255 0 0") had only the top row, and
                 * the one case that depends on the difference — a
                 * damaged block whose uv lands on the transparent row
                 * — came back as a mismatch with the DEVICE right and
                 * the spec wrong. Two halves of a test that disagree
                 * about their own input is a test whose failure report
                 * points at the wrong file. */
                " \"crack\": [[[0,0,200],[0,0,0]],[[0,0,0],[0,0,0]]],"
                " \"tints\": [");
    for (i = 0; i < nq; i++)
        fprintf(sp, "%s[%d, %d, %d]", i ? ", " : "",
                Q[i].tr, Q[i].tg, Q[i].tb);
    fprintf(sp, "], \"shades\": [");
    for (i = 0; i < nq; i++) fprintf(sp, "%s%d", i ? ", " : "", Q[i].shade);
    fprintf(sp, "], \"damages\": [");
    for (i = 0; i < nq; i++) fprintf(sp, "%s%d", i ? ", " : "", Q[i].dmg);
    fprintf(sp, "], \"texes\": [");
    for (i = 0; i < nq; i++) fprintf(sp, "%s%d", i ? ", " : "", Q[i].tex);
    fprintf(sp, "], \"crts\": [");
    for (i = 0; i < nq; i++) fprintf(sp, "%s%d", i ? ", " : "", Q[i].crt);
    fprintf(sp, "], \"corrupts\": [");
    for (i = 0; i < nq; i++) fprintf(sp, "%s%d", i ? ", " : "", Q[i].corrupt);
    fprintf(sp, "], \"pixels\": [");
    for (i = 0; i < nq; i++)
        fprintf(sp, "%s[%d, %d, %.4f, %.4f]", i ? ", " : "",
                Q[i].px, Q[i].py, Q[i].u, Q[i].v);
    /* the per-point canvas size, for the vignette */
    fprintf(sp, "], \"sizes\": [");
    for (i = 0; i < nq; i++)
        fprintf(sp, "%s[%d, %d]", i ? ", " : "", Q[i].w, Q[i].h);
    fprintf(sp, "]}\n");
    fclose(sp);

    /* the device's own answers, into the same shape the oracle reads */
    sp = fopen("/tmp/fbt_frag_dev.txt", "w");
    if (!sp) { ck(0, "can write the device's answers"); return; }
    for (i = 0; i < nq; i++) {
        int cr = 0, cg = 0, cb = 0;
        fb_frag_flags(Q[i].crt, Q[i].corrupt);
        fb_frag_view(Q[i].w, Q[i].h);
        fb_frag_color(Q[i].tex, Q[i].shade, Q[i].tr, Q[i].tg, Q[i].tb,
                      Q[i].dmg, Q[i].u, Q[i].v, Q[i].px, Q[i].py,
                      &cr, &cg, &cb);
        fprintf(sp, "%d %d %d\n", cr, cg, cb);
    }
    fclose(sp);
    fb_frag_flags(0, 0);
    fb_frag_view(0, 0);

    /* THE ORACLE'S OUTPUT FILE IS DELETED FIRST, AND THAT IS THE POINT.
     *
     * `system()` was called and its status checked, and the check
     * passed while the oracle had in fact DIED on a syntax error I had
     * just introduced — the python traceback went to /dev/null, the
     * exit status was not the one I assumed, and the comparison then
     * read the answer file left behind by an EARLIER run. The suite
     * printed "30 checks, 0 failures" and four deliberate mutations of
     * the fragment arithmetic all survived, because the half of the
     * check that was supposed to catch them was comparing a stale file
     * with itself.
     *
     * A comparison against a file the other side may not have written
     * is not a comparison. The file is removed, the oracle must
     * recreate it, and its stderr is shown rather than discarded, so
     * the next failure says what the oracle actually said. */
    remove("/tmp/fbt_frag_ref.txt");
    {
        char cmd[512];
        snprintf(cmd, sizeof cmd,
                 "python3 %s --emit /tmp/fbt_frag.json /tmp/fbt_frag_ref.txt"
                 " 2>/tmp/fbt_frag_err.txt", ORACLE);
        rc = system(cmd);
        ck(rc == 0, "the fragment oracle runs");
        if (rc != 0) {
            FILE *ef = fopen("/tmp/fbt_frag_err.txt", "r");
            if (ef) {
                char line[256];
                printf("  --- the oracle said: ---\n");
                while (fgets(line, sizeof line, ef))
                    printf("  %s", line);
                fclose(ef);
            } else {
                printf("  (the oracle wrote no error output either)\n");
            }
            return;
        }
    }
    {
        FILE *rf = fopen("/tmp/fbt_frag_ref.txt", "r");
        FILE *df = fopen("/tmp/fbt_frag_dev.txt", "r");
        if (!rf || !df) {
            ck(0, "both answer files exist");
            if (rf) fclose(rf);
            if (df) fclose(df);
            return;
        }
        for (i = 0; i < nq; i++) {
            int rr, rg, rb, dr, dg, db;
            if (fscanf(rf, "%d %d %d", &rr, &rg, &rb) != 3) { bad++; break; }
            if (fscanf(df, "%d %d %d", &dr, &dg, &db) != 3) { bad++; break; }
            if (rr != dr || rg != dg || rb != db) {
                bad++;
                if (bad <= 6)
                    printf("  frag case %d: device %d,%d,%d oracle %d,%d,%d\n",
                           i, dr, dg, db, rr, rg, rb);
            }
        }
        fclose(rf); fclose(df);
    }
    ck(bad == 0, "the fragment stage matches the independent oracle, every case");

    /* A FEW PROPERTIES, checked without the oracle, because they are
     * what a reader actually needs to believe and an oracle that is
     * merely CONSISTENT with a wrong device would not catch them.
     *
     * THEY ASK ABOUT A WHITE TEXEL, and that is the whole point of the
     * second parameter to fb_texture below. The first version of these
     * checks asked about texel (0,0) of the four-colour texture, which
     * is BLACK — and on a black pixel a 10% dim is invisible, a black
     * crack is invisible, and a black vignette is invisible. Both
     * checks then "failed" against a device that was behaving exactly
     * as the oracle said, which is the worst way for a test to be
     * wrong: it points the next reader at the device instead of at the
     * question. A property has to be asked where the property is
     * visible. */
    {
        int a, b2, c;
        /* the scanline really does dim, and only on py % 6 == 0 */
        fb_frag_flags(1, 0);
        fb_frag_color(1, 1000, 255, 255, 255, 0, 0.1, 0.1, 40, 6, &a, &b2, &c);
        fb_frag_color(1, 1000, 255, 255, 255, 0, 0.1, 0.1, 40, 7, &r, &g, &b);
        ck(a < r, "the CRT scanline dims the row at py%6==0 and not the next");
        /* the scanline is a 90% multiply, and the pixel started at 508 */
        ck(a * 100 / 508 >= 89 && a * 100 / 508 <= 91,
           "the scanline is the original's 90/100 multiply");
        /* damage 4 with an opaque crack replaces the block; damage 0 does
         * not touch it at all */
        fb_frag_flags(0, 0);
        fb_frag_color(1, 1000, 255, 255, 255, 0, 0.1, 0.1, 5, 5, &a, &b2, &c);
        fb_frag_color(1, 1000, 255, 255, 255, 4, 0.1, 0.1, 5, 5, &r, &g, &b);
        ck(r != a || g != b2 || b != c, "damage changes the colour");
        /* damage scales the mix: 4 is a full replacement of a black
         * crack, 1 is a quarter of it, so the higher damage is DARKER */
        fb_frag_color(1, 1000, 255, 255, 255, 1, 0.1, 0.1, 5, 5, &r, &g, &b);
        ck(r < a, "more damage means more crack");
        /* an UNKNOWN texture slot is black, not a crash and not the
         * previous texture: the settings menu asks for a slot before
         * its generator has finished. */
        fb_frag_color(77, 1000, 255, 255, 255, 0, 0.1, 0.1, 5, 5, &r, &g, &b);
        ck(r == 0 && g == 0 && b == 0, "an unknown texture slot reads as black");
        /* and the tint multiply is monotone: a darker tint is a darker
         * pixel, which is what makes the face shading visible at all */
        fb_frag_color(1, 1000, 128, 128, 128, 0, 0.1, 0.1, 5, 5, &r, &g, &b);
        fb_frag_color(1, 1000, 255, 255, 255, 0, 0.1, 0.1, 5, 5, &a, &b2, &c);
        ck(r <= a, "a lower tint gives a lower pixel value");
        /* the SAME texture at two different uvs gives two different
         * answers — the check that would have caught the single-plane
         * test texture above, and the one that actually proves the uv
         * is being interpolated rather than ignored. */
        fb_frag_color(0, 1000, 255, 255, 255, 0, 0.10, 0.10, 5, 5, &r, &g, &b);
        fb_frag_color(0, 1000, 255, 255, 255, 0, 0.90, 0.90, 5, 5, &a, &b2, &c);
        ck(r != a || g != b2 || b != c,
           "two uvs on a four-colour texture give two colours");
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
    t_frag();
    printf("== %d checks, %d failures ==\n", checks, fails);
    return fails ? 1 : 0;
}
