/* fbdev.c — the fb0 framebuffer device for bash-O4 renders.
 *
 * Rasterizes MIMEcroft's batched cube payload with the game's OWN
 * projection (transcribed from www/examples/mimecroft-vertex.sh, the
 * bash-authored vertex shader):
 *
 *   p    = aPosition * uScale + uObjPos          (object -> world)
 *   d    = p - (uCamPos + (0, 0.5, 0))           (eye at the cell centre)
 *   rel  = (d.x·cos + d.z·sin, d.y, -d.x·sin + d.z·cos)
 *   w    = -rel.z
 *   clip = (rel.x·0.7 + shift·w, rel.y·0.45, w²/64, w)
 *   ndc  = clip.xy / clip.w
 *
 * 36 vertices (6 faces x 2 tris) with MIMEcroft's own aPosition corner
 * table and its per-face aShade. NO depth buffer: render_frame already
 * emits far-to-near, so payload order is draw order.
 *
 * Two backends: a real /dev/fb0 (open + FBIOGET_VSCREENINFO +
 * FBIOGET_FSCREENINFO + mmap, honouring bpp and the kernel's channel
 * offsets), or a RAM surface dumped as a P6 PPM. This host has fbcon
 * but no /dev/fb0, so the RAM path is the one that runs here.
 *
 * Builds clean under both cc -Wall -Wextra and tcc (bash-O4's
 * toolchain): no VLAs, no nested functions, no C99 //-in-preprocessor
 * tricks, no anonymous unions.
 */
#define _GNU_SOURCE
#include "fbdev.h"
#include "fbwin.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <math.h>

#if defined(__linux__)
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <linux/fb.h>
#define FBDEV_HAVE_FBIO 1
#endif

/* ── surface state ────────────────────────────────────────────────── */
static int      S_w, S_h;
static uint32_t *S_px;            /* 0xAARRGGBB, row-major, len w*h */
static int      S_opened;

/* real-device state (all zero on the RAM path) */
static int      S_fb_fd = -1;
static uint8_t  *S_fb_map;
static size_t   S_fb_len;
static int      S_fb_bpp;
static int      S_fb_roff, S_fb_goff, S_fb_boff, S_fb_toff;
static int      S_fb_xoff, S_fb_yoff;
static int      S_fb_vw, S_fb_vh, S_fb_xres, S_fb_yres, S_fb_vpan;
static int      S_fb_line;

/* camera (world units) */
static double   C_x, C_y, C_z, C_yaw, C_shift;
static double   C_cos, C_sin;

/* ── the block textures + the per-pixel fragment stage ─────────────
 *
 * The WebGL path's block appearance is a FRAGMENT SHADER, not a vertex
 * colour (www/bin/mimecroft.sh, emit_fragment_shader, line 1312). It
 * samples a block texture with fract(vUv), multiplies the per-face
 * tint by it, mixes a crack texture in by the damage level, then
 * optionally applies a CRT scanline, corruption streaks and a
 * vignette. Reproducing that per pixel is what makes a block look
 * like stone instead of a painted box, and it is the single largest
 * fidelity gap a flat-shaded port has.
 *
 * THE ARITHMETIC IS TRANSCRIBED, NOT REIMPLEMENTED, and every value is
 * an int for the same reason the original's is: the sh2glsl generator
 * emits mediump ints, so `/` is an INTEGER divide there. A float
 * version is smooth and wrong — wrong by one unit on many pixels,
 * which is precisely the class of difference a byte-exact comparison
 * against real bash exists to catch.
 *
 * AND THE ORDER IS LOAD-BEARING. It is, in the original's words, dim
 * first then blend, so the crack does NOT get dimmed with the texture:
 *
 *     r = tint·tex/128          the block texture through the tint
 *     r = r·90/100              CRT scanline  (fy % 6 == 0)   ← BEFORE
 *     r -= (r-cr)·mix/128       the crack, mixed by damage     ← the crack
 *     corruption streaks
 *     r -= r·dim/256            the vignette, multiplicative
 *     clamp to 0
 *
 * The original comments record that moving the scanline after the
 * crack is "a 1-step visual difference on damaged blocks", which is
 * the sort of thing that survives a port and shows up later as a
 * pixel diff nobody can explain. */
#define FB_TEX_MAX   80
#define FB_TEX_MAXSZ 64

typedef struct {
    int   used;
    int   size;
    int   rgba;                 /* the crack texture carries alpha */
    uint8_t *px;                /* size*size*4, row-major RGBA */
} Tex;

static Tex      T_tex[FB_TEX_MAX];
static int      T_n = 0;
static int      T_crt = 0, T_corrupt = 0;

/* The texture slot and damage of the cube being drawn RIGHT NOW.
 * tri_shaded is four call levels below draw_cube and threading two more
 * parameters through the face loop and the guard-band clip for them
 * would mean touching the innermost rasterizer — the one piece of this
 * file that is already right and is covered by a 112-case silhouette
 * oracle. The alternative is a pair of file-scope values set once per
 * cube, which is exactly the "global state is fine" argument except
 * that the writer is a single function and the reader is single-
 * threaded by construction: fb_blocks walks one record at a time and
 * no other thread exists in the device. */
static int      FB_TEX_IDX = -1;
static int      FB_TEX_ACTIVE = 0;

/* The canvas the fragment program was generated for, for the vignette's
 * centre. 0 = "the surface", which is right for a game that never
 * resizes. See frag_vignette for why this is a parameter. */
static int      T_vw = 0, T_vh = 0;

int fb_tex_count(void) { return T_n; }

int fb_texture(int idx, const char *r, const char *g, const char *b,
               int n, int size, int rgba) {
    const char *src[3];
    int i, total;
    Tex *t;
    if (idx < 0 || idx >= FB_TEX_MAX || size <= 0 || size > FB_TEX_MAXSZ)
        return -1;
    if (n <= 0) return -1;
    src[0] = r; src[1] = g; src[2] = b;
    t = &T_tex[idx];
    total = n * n;
    /* The 4th channel of an RGB texture is OPAQUE, and that is not a
     * detail: the crack branch reads the texture's own alpha, so an
     * uninitialised byte there is a random crack on every block. */
    free(t->px);
    /* `rgba` says the caller's three planes are R,G,A rather than
     * R,G,B — the crack texture is a TRANSPARENT overlay, so its "blue"
     * channel is really the crack's coverage. Without this the crack
     * branch reads an opaque alpha and every damaged block comes out
     * fully replaced by the crack colour instead of blended with it. */
    t->px = (uint8_t *)malloc((size_t)total * 4);
    if (!t->px) { t->used = 0; return -1; }
    for (i = 0; i < total; i++) {
        int c;
        for (c = 0; c < 3; c++) {
            char *end;
            long d;
            const char *q = src[c];
            while (*q == ' ' || *q == '\t' || *q == '\n' || *q == '\r') q++;
            d = strtol(q, &end, 10);
            if (end == q) d = 0;
            src[c] = end;              /* this plane's cursor MOVES ON */
            if (d < 0) d = 0;
            if (d > 255) d = 255;
            t->px[i*4 + c] = (uint8_t)d;
        }
        t->px[i*4 + 3] = rgba ? t->px[i*4 + 2] : (uint8_t)255;
        if (rgba) t->px[i*4 + 2] = 0;   /* B is unused on an RGBA texture */
    }
    t->size = size;
    t->rgba = rgba;
    t->used = 1;
    if (idx + 1 > T_n) T_n = idx + 1;
    return 0;
}

void fb_frag_flags(int crt_on, int corrupt_on) {
    T_crt = crt_on ? 1 : 0;
    T_corrupt = corrupt_on ? 1 : 0;
}

void fb_frag_view(int w, int h) { T_vw = w; T_vh = h; }

/* THE CUBE'S PER-VERTEX TEXTURE COORDINATES, transcribed from the
 * game's own /dev/webgl/buffer/aUv write (www/bin/mimecroft.sh, line
 * 1444):
 *
 *   f32 0 0 1 0 1 1 0 1  0 0 1 0 1 1 0 1  0 0 1 0 1 1 0 1
 *       1 1 1 0 1 0  0 1 0 0 1 0 0 1 0  1 1 1 1 1 0 0 0 0 1
 *
 * 24 vertices x (u,v), row-major — 48 doubles, indexed as fuv[vi*2+k]
 * so it lines up with CUBE_P and CUBE_SHADE, which use the same vertex
 * numbering. This is the varying the fragment shader samples with, and
 * it is why the faces do NOT all span the same rectangle: the last two
 * vertices are (0,0) and (0,1), which is the floor/ceiling quad's own
 * orientation. Writing "every face is 0..1" instead — the obvious
 * guess, and what a first pass at this did — puts the same texture on
 * every face at the same scale, which looks plausible and is not what
 * the game draws.
 *
 * TRANSCRIBED, like CUBE_P and CUBE_SHADE, and for the same reason: a
 * hand-built version of a 48-number geometry table is a second source
 * of truth about the same cube, and this file already has one
 * transcription bug on record (see CUBE_P). */
static const double CUBE_UV[48] = {
  0, 0,  1, 0,  1, 1,  0, 1,   0, 0,  1, 0,  1, 1,  0, 1,
  0, 0,  1, 0,  1, 1,  0, 1,   0, 0,  1, 0,  1, 1,  0, 1,
  1, 1,  1, 0,  1, 0,  0, 1,   0, 1,  0, 0,  1, 0,  0, 1,
};

/* THE CRACK TEXTURE IS SLOT 9, not slot 0. The game binds it
 * explicitly: `echo "9" > /dev/webgl/uniform/1i/uCrack`
 * (www/bin/mimecroft.sh, setup_webgl), and slot 0 is the FIRST BLOCK
 * texture, so a device that assumed "the crack is texture 0" would
 * overlay the stone texture on itself: every damaged block would get
 * a faint copy of its own face pattern mixed in, which is invisible in
 * a still frame and obvious in a swing. The number is a binding, not
 * an index into a list this device owns. */
#define CRACK_TEX 9


/* fract(vUv) → a texel index, with the wrap the shader gets free from
 * GL_REPEAT. The generated shader samples with fract(vUv) explicitly,
 * so the device does the same rather than relying on a sampler mode. */
static void frag_sample(const Tex *t, double u, double v,
                        int *r, int *g, int *b, int *a) {
    int iu, iv, o;
    u = u - (double)(long)u;   /* fract() for the 0..1 case, floor-safe */
    if (u < 0.0) u += 1.0;
    v = v - (double)(long)v;
    if (v < 0.0) v += 1.0;
    iu = (int)(u * t->size);
    iv = (int)(v * t->size);
    if (iu >= t->size) iu = t->size - 1;
    if (iv >= t->size) iv = t->size - 1;
    if (iu < 0) iu = 0;
    if (iv < 0) iv = 0;
    o = (iv * t->size + iu) * 4;
    *r = t->px[o]; *g = t->px[o+1]; *b = t->px[o+2]; *a = t->px[o+3];
}

/* The vignette's half-extent comes from the CANVAS, exactly as the
 * original reads disp_w/disp_h back from the device after the resize
 * (emit_fragment_shader bakes disp_w/disp_h into the generated
 * program). So it is a PARAMETER, not the surface size: the settings
 * menu's DISPLAY row resizes the canvas, but the fragment program's
 * centre is fixed at the moment the program was generated, and on a
 * display smaller than ~900x900 the threshold is simply never reached
 * (the original's own numbers: 450 against 1920x1080's 1500 corner).
 *
 * Reading S_w here instead would have made the check for the vignette
 * impossible to write: every 320x240 case is below the threshold, so a
 * device with the wrong threshold and one with the right threshold
 * answer identically on every pixel. */
static void frag_vignette(int px, int py, int vw, int vh,
                          int *r, int *g, int *b) {
    int hw = vw / 2, hh = vh / 2;
    int vx = px - hw, vy = py - hh;
    int edge, dim;
    if (vx < 0) vx = -vx;
    if (vy < 0) vy = -vy;
    edge = vx + vy;
    if (edge > 450) {
        dim = edge - 450;
        if (dim > 30) dim = 30;
        /* multiplicative, so dark pixels never hard-clip to black */
        *r -= *r * dim / 256;
        *g -= *g * dim / 256;
        *b -= *b * dim / 256;
    }
}

/* ONE PIXEL of the block's fragment program.
 *
 * `sh` is the face's shade (the aShade barycentric value 0..1) and
 * `tint` the record's r/g/b × 255, so this is the whole of what the
 * fragment shader receives as vcolor. `u`,`v` are the pixel's position
 * on the face, already fract-wrapped by the caller. */
static void frag_pixel(const Tex *blk, const Tex *crk,
                       int tr, int tg, int tb, int shade,
                       int damage, int px, int py,
                       double u, double v,
                       int vw, int vh,
                       int *outr, int *outg, int *outb) {
    int tex_r, tex_g, tex_b, tex_a;
    int r, g, b;
    frag_sample(blk, u, v, &tex_r, &tex_g, &tex_b, &tex_a);
    /* the per-face shade is the OTHER half of vcolor: the original's
     * vertex shader multiplies aShade into vcolor before the fragment
     * ever runs, so it is applied here, before the texture. */
    tr = tr * shade / 1000;
    tg = tg * shade / 1000;
    tb = tb * shade / 1000;
    if (tr < 0) tr = 0;
    if (tr > 255) tr = 255;
    if (tg < 0) tg = 0;
    if (tg > 255) tg = 255;
    if (tb < 0) tb = 0;
    if (tb > 255) tb = 255;
    r = tr * tex_r / 128;
    g = tg * tex_g / 128;
    b = tb * tex_b / 128;
    if (T_crt) {
        if (py % 6 == 0) {
            r = r * 90 / 100;
            g = g * 90 / 100;
            b = b * 90 / 100;
        }
    }
    if (damage > 0 && crk && crk->used) {
        int cr, cg2, cb, ca, mix;
        frag_sample(crk, u, v, &cr, &cg2, &cb, &ca);
        mix = damage * ca / 4;
        if (mix > 127) mix = 127;
        r = r - (r - cr) * mix / 128;
        g = g - (g - cg2) * mix / 128;
        b = b - (b - cb) * mix / 128;
    }
    if (T_corrupt) {
        int hash = px * 7 + py * 13;
        if (hash % 97 == 0) {
            r = 255;
            g = g / 2;
            b = b / 2;
        }
    }
    if (T_crt) frag_vignette(px, py, vw, vh, &r, &g, &b);
    if (r < 0) r = 0;
    if (g < 0) g = 0;
    if (b < 0) b = 0;
    *outr = r; *outg = g; *outb = b;
}

/* The one-point entry point, AFTER frag_pixel so it can call it without a
 * forward declaration. See fbdev.h for why this is a primitive the game
 * itself wants (the settings menu's texture thumbnails) and not only a
 * test hook. */
void fb_frag_color(int texidx, int shade, int tr, int tg, int tb,
                   int damage, double u, double v, int px, int py,
                   int *outr, int *outg, int *outb) {
    const Tex *blk = 0, *crk = 0;
    *outr = *outg = *outb = 0;
    if (texidx < 0 || texidx >= T_n || !T_tex[texidx].used) return;
    blk = &T_tex[texidx];
    if (CRACK_TEX < T_n && T_tex[CRACK_TEX].used) crk = &T_tex[CRACK_TEX];
    /* The vignette centre is the CANVAS the caller is asking about, and
     * a caller that wants a different one (the settings menu's DISPLAY
     * row) says so with fb_frag_view. It defaults to the surface. */
    frag_pixel(blk, crk, tr, tg, tb, shade, damage, px, py, u, v,
               T_vw > 0 ? T_vw : S_w, T_vh > 0 ? T_vh : S_h,
               outr, outg, outb);
}

/* ── the 2D overlay: the HUD layer ──────────────────────────────────
 *
 * A SEPARATE, PERSISTENT RGBA surface, composited over the world at
 * present. See fbdev.h for the payload grammar and for why it has an
 * alpha channel at all (the erase record sets alpha 0, which is what
 * lets the world show through and what makes the game's per-cell
 * partial redraw possible).
 *
 * NOT A SECOND PIXEL BUFFER OF COLOUR. A layer without alpha is a sheet
 * of paint: the radar's base, the gun and the text would all have to be
 * redrawn every frame, because there is no way to take one back out.
 * The original's rm[] arrays — "the last cell drawn in each radar
 * square" — only make sense against a layer you can erase. */
static uint8_t *O_ov;            /* w*h*4, RGBA, alpha meaningful */
static uint8_t *O_flash;         /* transient overlay, cleared at present */
static int      O_w, O_h;        /* the layer's size, the surface's */
static int      O_have;

static void ov_alloc(int w, int h) {
    size_t n;
    if (O_have && O_w == w && O_h == h) return;
    free(O_ov);
    free(O_flash);
    n = (size_t)w * (size_t)h;
    O_ov    = (uint8_t *)calloc(n, 4);
    O_flash = (uint8_t *)calloc(n, 4);
    O_w = w;
    O_h = h;
    O_have = (O_ov && O_flash);
    /* fb_open RESET THE SURFACE and every overlay went with it. The
     * layer is sized on first use, so a re-open at a different size
     * freed it — which is right — but the suite's re-opens at the SAME
     * size did not, and a layer that survives a re-open is a layer that
     * also survives a game restarting on a new level, carrying the old
     * level's radar with it. fb_open drops it: a new surface is a new
     * HUD. */
}

/* NDC (-1..1, +y up) -> pixel. +y is up in NDC and down in a row. */
static void ndc_to_px(double cx, double cy, int *px, int *py) {
    *px = (int)((cx * 0.5 + 0.5) * S_w);
    *py = (int)((0.5 - cy * 0.5) * S_h);
}

/* The four corners of an overlay record's rectangle, rotated about its
 * centre by `deg` DEGREES. The game writes the gun and the radar's
 * player triangle tilted, and a rotated axis-aligned rectangle is not a
 * rotated one unless the rotation is applied to the corners. */
static void ov_corners(double cx, double cy, double wfrac, double hfrac,
                       double deg, int *out) {
    double hw, hh, rad, ca, sa, dx, dy;
    int i;
    int px, py;
    ndc_to_px(cx, cy, &px, &py);
    /* w and h are the FULL extents as a FRACTION of the canvas, not
     * half-extents. The game's own numbers decide it: the radar is 16
     * cells at CELL_W = 0.040 on a 44-milli pitch, and 16 cells span
     * 16 * 0.044 = 0.704 NDC units = 0.352 of the canvas width — the
     * radar the game draws (x = 13..125 on a 320 canvas). Read as a
     * half-extent the same 16 cells would span twice that and every
     * second cell would swallow its neighbour.
     *
     * NDC -1..1 maps to S_w px, so a fraction f covers f/2 * S_w px
     * across, and the corners are centre +/- half of THAT:
     * hw = f * S_w / 4. The previous revision used f * S_w / 2 — the
     * full width as the half-extent — painting every overlay quad
     * twice as wide AND twice as tall: 13px text cells on a 5px
     * pitch (neighbours overlapped, counters closed, digits bold),
     * a 50px gun barrel against the oracle's 22, a 205px radar
     * against its 112. The oracle's canvas fills (w/2 halves of the
     * full px width) are the ground truth; the old comment's "0.64
     * of the canvas" made the same NDC-vs-fraction error (0.64 NDC
     * units is 0.32 of the canvas, not 0.64). */
    hw = wfrac * S_w * 0.25;
    hh = hfrac * S_h * 0.25;
    rad = deg * 3.14159265358979323846 / 180.0;
    ca = cos(rad);
    sa = sin(rad);
    for (i = 0; i < 4; i++) {
        double sx = (i == 0 || i == 3) ? -1.0 : 1.0;
        double sy = (i < 2) ? -1.0 : 1.0;
        dx = sx * hw;
        dy = sy * hh;
        /* ROUND, DO NOT TRUNCATE, and keep the corners in doubles.
         *
         * Truncating towards zero put the far corner of a 12.8px-wide
         * cell at 6 and the near one at -6 in opposite directions (a
         * C cast truncates toward zero, not down), so the quad was 13
         * px wide instead of 12.8's 12 or 13 and a 16-cell radar did
         * not add up. The truncation is also a half-plane test
         * boundary: a corner moved inward by a whole pixel claims a
         * pixel the shape does not cover. */
        {
            double ex = px + dx * ca - dy * sa;
            double ey = py + dx * sa + dy * ca;
            out[i*2]   = (int)(ex < 0 ? ex - 0.5 : ex + 0.5);
            out[i*2+1] = (int)(ey < 0 ? ey - 0.5 : ey + 0.5);
        }
    }
}

/* Coverage of a pixel by the rotated quad, by the same half-plane test
 * the 3D rasterizer uses, so the two agree on what a pixel centre
 * means. */
static int ov_covers(int ax, int ay, int bx, int by, int px, int py) {
    return (double)(bx - ax) * (py - ay) - (double)(by - ay) * (px - ax) >= 0.0;
}

/* Blit a solid quad into a layer. `a` is 0..255 and is COMBINED with
 * what is already there: a translucent HUD element over a translucent
 * one must not simply replace the first, or the radar's ring and core —
 * drawn as two R records over the same square — would lose the ring. */
static void ov_fill(uint8_t *layer, const int *corners,
                    int r, int g, int b, int a) {
    int minx = corners[0], maxx = corners[0], miny = corners[1], maxy = corners[1];
    int x, y, i;
    for (i = 1; i < 4; i++) {
        if (corners[i*2]   < minx) minx = corners[i*2];
        if (corners[i*2]   > maxx) maxx = corners[i*2];
        if (corners[i*2+1] < miny) miny = corners[i*2+1];
        if (corners[i*2+1] > maxy) maxy = corners[i*2+1];
    }
    if (minx < 0) minx = 0;
    if (miny < 0) miny = 0;
    if (maxx > S_w - 1) maxx = S_w - 1;
    if (maxy > S_h - 1) maxy = S_h - 1;
    if (a <= 0) return;
    for (y = miny; y <= maxy; y++) {
        for (x = minx; x <= maxx; x++) {
            double pxd = x + 0.5, pyd = y + 0.5;
            size_t o;
            if (!ov_covers(corners[0], corners[1], corners[2], corners[3], (int)pxd, (int)pyd)) continue;
            if (!ov_covers(corners[2], corners[3], corners[4], corners[5], (int)pxd, (int)pyd)) continue;
            if (!ov_covers(corners[4], corners[5], corners[6], corners[7], (int)pxd, (int)pyd)) continue;
            if (!ov_covers(corners[6], corners[7], corners[0], corners[1], (int)pxd, (int)pyd)) continue;
            o = ((size_t)y * S_w + x) * 4;
            if (a == 255) {
                layer[o] = (uint8_t)r; layer[o+1] = (uint8_t)g;
                layer[o+2] = (uint8_t)b; layer[o+3] = 255;
            } else {
                /* source-over, in integer, with the divide the compositing
                 * actually needs: out = src*a + dst*(255-a) over 255. */
                int ia = 255 - a;
                layer[o]   = (uint8_t)((r * a + layer[o]   * ia) / 255);
                layer[o+1] = (uint8_t)((g * a + layer[o+1] * ia) / 255);
                layer[o+2] = (uint8_t)((b * a + layer[o+2] * ia) / 255);
                layer[o+3] = (uint8_t)(a + layer[o+3] * ia / 255);
            }
        }
    }
}

/* Set alpha 0 over a rectangle. NOT a background colour: this is what
 * makes the world's pixels visible again underneath. */
/* cx AND cy ARE NDC DOUBLES, and taking them as ints moved the erase
 * to the wrong place. `cy = 0.5` is a legal and used NDC position — it
 * is the TOP HALF of the screen, which is where the game's radar sits —
 * and an int parameter truncated it to 0, putting the erase in the exact
 * centre. The erase then covered empty space, the HUD cell it was
 * supposed to clear stayed, and the device looked like it was ignoring
 * the record.
 *
 * A C compiler will happily convert a double to an int parameter with
 * no diagnostic, and the result is a plausible-looking wrong position
 * rather than a build failure, so this is worth stating: every
 * coordinate in the overlay payload is a double. */
static void ov_erase_rect(uint8_t *layer, double cx, double cy,
                          double wfrac, double hfrac) {
    int corners[8], x, y, minx, maxx, miny, maxy, i;
    ov_corners(cx, cy, wfrac, hfrac, 0.0, corners);
    minx = maxx = corners[0];
    miny = maxy = corners[1];
    for (i = 1; i < 4; i++) {
        if (corners[i*2]   < minx) minx = corners[i*2];
        if (corners[i*2]   > maxx) maxx = corners[i*2];
        if (corners[i*2+1] < miny) miny = corners[i*2+1];
        if (corners[i*2+1] > maxy) maxy = corners[i*2+1];
    }
    if (minx < 0) minx = 0;
    if (miny < 0) miny = 0;
    if (maxx > S_w - 1) maxx = S_w - 1;
    if (maxy > S_h - 1) maxy = S_h - 1;
    for (y = miny; y <= maxy; y++)
        for (x = minx; x <= maxx; x++)
            layer[((size_t)y * S_w + x) * 4 + 3] = 0;
    fprintf(stderr, "DBG ERASED minx=%d maxx=%d miny=%d maxy=%d; now at (160,60) a=%d r=%d\n",
            minx,maxx,miny,maxy, layer[((size_t)60*S_w+160)*4+3], layer[((size_t)60*S_w+160)*4]);
}

/* Blit a TEXTURE into the layer, unrotated (the I record). The game's
 * label textures are name banners; the texidx is its slot in the
 * texture table, which is the same table fb_texture fills. */
static void ov_image(uint8_t *layer, double cx, double cy,
                   double wfrac, double hfrac, int texidx) {
    const Tex *t;
    int corners[8], x, y, minx, maxx, miny, maxy, i;
    if (texidx < 0 || texidx >= T_n || !T_tex[texidx].used) return;
    t = &T_tex[texidx];
    ov_corners(cx, cy, wfrac, hfrac, 0.0, corners);
    minx = maxx = corners[0];
    miny = maxy = corners[1];
    for (i = 1; i < 4; i++) {
        if (corners[i*2]   < minx) minx = corners[i*2];
        if (corners[i*2]   > maxx) maxx = corners[i*2];
        if (corners[i*2+1] < miny) miny = corners[i*2+1];
        if (corners[i*2+1] > maxy) maxy = corners[i*2+1];
    }
    if (minx < 0) minx = 0;
    if (miny < 0) miny = 0;
    if (maxx > S_w - 1) maxx = S_w - 1;
    if (maxy > S_h - 1) maxy = S_h - 1;
    for (y = miny; y <= maxy; y++) {
        for (x = minx; x <= maxx; x++) {
            double pxd = x + 0.5, pyd = y + 0.5;
            size_t o;
            int tu, tv;
            if (!ov_covers(corners[0], corners[1], corners[2], corners[3], (int)pxd, (int)pyd)) continue;
            if (!ov_covers(corners[2], corners[3], corners[4], corners[5], (int)pxd, (int)pyd)) continue;
            if (!ov_covers(corners[4], corners[5], corners[6], corners[7], (int)pxd, (int)pyd)) continue;
            if (!ov_covers(corners[6], corners[7], corners[0], corners[1], (int)pxd, (int)pyd)) continue;
            tu = (x - corners[0]) * t->size / (corners[2] - corners[0] ? corners[2] - corners[0] : 1);
            tv = (y - corners[1]) * t->size / (corners[3] - corners[1] ? corners[3] - corners[1] : 1);
            if (tu < 0) tu = 0;
            if (tv < 0) tv = 0;
            if (tu >= t->size) tu = t->size - 1;
            if (tv >= t->size) tv = t->size - 1;
            o = ((size_t)y * S_w + x) * 4;
            {
                size_t to = ((size_t)tv * t->size + tu) * 4;
                int a = t->px[to+3];
                if (a == 0) continue;          /* transparent texel: draw nothing */
                if (a == 255) {
                    layer[o] = t->px[to]; layer[o+1] = t->px[to+1];
                    layer[o+2] = t->px[to+2]; layer[o+3] = 255;
                } else {
                    int ia = 255 - a;
                    layer[o]   = (uint8_t)((t->px[to]   * a + layer[o]   * ia) / 255);
                    layer[o+1] = (uint8_t)((t->px[to+1] * a + layer[o+1] * ia) / 255);
                    layer[o+2] = (uint8_t)((t->px[to+2] * a + layer[o+2] * ia) / 255);
                    layer[o+3] = (uint8_t)(a + layer[o+3] * ia / 255);
                }
            }
        }
    }
}

/* the overlay layer, composited over the RAM surface at present */
static void ov_composite(void) {
    size_t i, n;
    fprintf(stderr, "DBG COMPOSITE O_ov=%p S_w=%d\n", (void*)O_ov, S_w);
    if (!O_have) return;
    n = (size_t)S_w * (size_t)S_h;
    for (i = 0; i < n; i++) {
        int a = O_ov[i*4+3];
        if (a == 0) continue;
        if (a == 255) {
            S_px[i] = 0xFF000000u | ((uint32_t)O_ov[i*4] << 16)
                   | ((uint32_t)O_ov[i*4+1] << 8) | (uint32_t)O_ov[i*4+2];
        } else {
            uint32_t d = S_px[i] & 0x00FFFFFFu;
            int ia = 255 - a;
            int r = (O_ov[i*4]   * a + (int)((d >> 16) & 0xFF) * ia) / 255;
            int g = (O_ov[i*4+1] * a + (int)((d >>  8) & 0xFF) * ia) / 255;
            int b = (O_ov[i*4+2] * a + (int)( d        & 0xFF) * ia) / 255;
            S_px[i] = 0xFF000000u | ((uint32_t)r << 16) | ((uint32_t)g << 8) | (uint32_t)b;
        }
    }
    /* the flash layer goes over the persistent one and is GONE after
     * this: the game writes it only while the muzzle is lit and simply
     * stops, so a stale flash would hang on a static scene. */
    for (i = 0; i < n; i++) {
        int a = O_flash[i*4+3];
        if (a == 0) continue;
        if (a == 255) {
            S_px[i] = 0xFF000000u | ((uint32_t)O_flash[i*4] << 16)
                   | ((uint32_t)O_flash[i*4+1] << 8) | (uint32_t)O_flash[i*4+2];
        } else {
            uint32_t d = S_px[i] & 0x00FFFFFFu;
            int ia = 255 - a;
            int r = (O_flash[i*4]   * a + (int)((d >> 16) & 0xFF) * ia) / 255;
            int g = (O_flash[i*4+1] * a + (int)((d >>  8) & 0xFF) * ia) / 255;
            int b = (O_flash[i*4+2] * a + (int)( d        & 0xFF) * ia) / 255;
            S_px[i] = 0xFF000000u | ((uint32_t)r << 16) | ((uint32_t)g << 8) | (uint32_t)b;
        }
        O_flash[i*4+3] = 0;
    }
}

/* ── the payload parsers ─────────────────────────────────────────── */

/* Parse and apply one overlay payload into `layer`. The record type is
 * the FIRST CHARACTER and the fields are whitespace separated, so this
 * is a per-line switch rather than a field count: an R record has
 * eight or nine numbers, an I record five and an E record four, and
 * deciding on a count is how a parser ends up desynchronising and
 * drawing a gun where a radar cell should be.
 *
 * A record this device does not know is REPORTED and skipped, not
 * guessed at: a HUD that is silently wrong in an unknown shape is a HUD
 * nobody can debug, and the game's own payload is the only thing that
 * should ever arrive here. */
static void ov_apply(int which, const char *payload) {
    const char *p = payload;
    uint8_t *layer;
    if (!p || !S_px) return;
    ov_alloc(S_w, S_h);
    if (!O_have) return;
    layer = which ? O_flash : O_ov;
    if (!layer) return;
    for (;;) {
        double v[10];
        int n = 0;
        char kind;
        while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') p++;
        if (!*p) return;
        kind = *p++;
        /* A BARE NUMBER IS A RECORD TOO, and this is the payload's one
         * piece of grammar that is not a letter. draw_rect — which is
         * the radar, the digits, the text and every other axis-aligned
         * element — appends `cx cy w h r g b [a]` with NO type letter,
         * because on the WebGL path a payload with no letter IS a quad
         * and the letter is only needed for the shapes that are not.
         *
         * The first version of this parser only knew R/E/I/T and
         * reported every bare record as unknown, which drew the gun and
         * the flash and NOTHING ELSE: an empty radar and no digits,
         * which looks like a game with no HUD rather than like a
         * parser with a gap. Every record type is listed below, and
         * the count of fields is what distinguishes them. */
        if (kind == 'R' || kind == 'E' || kind == 'I' || kind == 'T') {
            int max = (kind == 'R') ? 9 : (kind == 'T' ? 9 : 5);
            while (n < max) {
                char *end;
                double d;
                while (*p == ' ' || *p == '\t') p++;
                if (!*p || *p == '\n' || *p == '\r') break;
                d = strtod(p, &end);
                if (end == p) break;
                v[n++] = d;
                p = end;
            }
        } else if ((kind >= '0' && kind <= '9') || kind == '-' || kind == '+'
                   || kind == '.') {
            /* a bare quad: the first number starts here */
            p--;
            n = 0;
            while (n < 8) {
                char *end;
                double d;
                while (*p == ' ' || *p == '\t') p++;
                if (!*p || *p == '\n' || *p == '\r') break;
                d = strtod(p, &end);
                if (end == p) break;
                v[n++] = d;
                p = end;
            }
            if (n >= 7) {
                int corners[8];
                int a = (n >= 8) ? (int)(v[7] * 255.0 + 0.5) : 255;
                int r = (int)(v[4] * 255.0 + 0.5);
                int g = (int)(v[5] * 255.0 + 0.5);
                int b = (int)(v[6] * 255.0 + 0.5);
                ov_corners(v[0], v[1], v[2], v[3], 0.0, corners);
                ov_fill(layer, corners, r, g, b, a);
            }
        } else {
            fprintf(stderr, "fb_overlay: unknown record type '%c'"
                            " (expected a bare quad, R, E, I or T); skipped\n",
                    kind);
        }
        if (kind == 'R' && n >= 8) {
            int corners[8];
            int a = (n >= 9) ? (int)(v[8] * 255.0 + 0.5) : 255;
            int r = (int)(v[5] * 255.0 + 0.5);
            int g = (int)(v[6] * 255.0 + 0.5);
            int b = (int)(v[7] * 255.0 + 0.5);
            ov_corners(v[0], v[1], v[2], v[3], v[4], corners);
            ov_fill(layer, corners, r, g, b, a);
        } else if (kind == 'E' && n >= 4) {
            fprintf(stderr, "DBG E which=%d layer=%p O_ov=%p S_w=%d\n", which, (void*)layer, (void*)O_ov, S_w);
            ov_erase_rect(layer, v[0], v[1], v[2], v[3]);
        } else if (kind == 'I' && n >= 5) {
            ov_image(layer, v[0], v[1], v[2], v[3], (int)v[4]);
        } else if (kind == 'T' && n >= 9) {
            /* a rotated, tinted, alpha'd TEXTURE: the radar's player
             * triangle, which the game draws from a glyph texture at
             * the player's own heading */
            int corners[8];
            int a = (int)(v[8] * 255.0 + 0.5);
            const Tex *t;
            int texidx = -1, i, minx, maxx, miny, maxy, x, y;
            for (i = 0; i < T_n; i++)
                if (T_tex[i].used) { texidx = i; break; }
            if (texidx < 0) goto next;
            t = &T_tex[texidx];
            ov_corners(v[0], v[1], v[2], v[2] * (double)S_h / (double)(S_w ? S_w : 1),
                       v[7], corners);
            minx = maxx = corners[0];
            miny = maxy = corners[1];
            for (i = 1; i < 4; i++) {
                if (corners[i*2]   < minx) minx = corners[i*2];
                if (corners[i*2]   > maxx) maxx = corners[i*2];
                if (corners[i*2+1] < miny) miny = corners[i*2+1];
                if (corners[i*2+1] > maxy) maxy = corners[i*2+1];
            }
            if (minx < 0) minx = 0;
            if (miny < 0) miny = 0;
            if (maxx > S_w - 1) maxx = S_w - 1;
            if (maxy > S_h - 1) maxy = S_h - 1;
            for (y = miny; y <= maxy; y++) {
                for (x = minx; x <= maxx; x++) {
                    double pxd = x + 0.5, pyd = y + 0.5;
                    size_t o, to;
                    int tu, tv, ia, tr, tg, tb;
                    if (!ov_covers(corners[0], corners[1], corners[2], corners[3], (int)pxd, (int)pyd)) continue;
                    if (!ov_covers(corners[2], corners[3], corners[4], corners[5], (int)pxd, (int)pyd)) continue;
                    if (!ov_covers(corners[4], corners[5], corners[6], corners[7], (int)pxd, (int)pyd)) continue;
                    if (!ov_covers(corners[6], corners[7], corners[0], corners[1], (int)pxd, (int)pyd)) continue;
                    tu = (x - corners[0]) * t->size / (corners[2] - corners[0] ? corners[2] - corners[0] : 1);
                    tv = (y - corners[1]) * t->size / (corners[3] - corners[1] ? corners[3] - corners[1] : 1);
                    if (tu < 0) tu = 0;
                    if (tv < 0) tv = 0;
                    if (tu >= t->size) tu = t->size - 1;
                    if (tv >= t->size) tv = t->size - 1;
                    o = ((size_t)y * S_w + x) * 4;
                    to = ((size_t)tv * t->size + tu) * 4;
                    tr = t->px[to]   * (int)v[3] / 255;
                    tg = t->px[to+1] * (int)v[4] / 255;
                    tb = t->px[to+2] * (int)v[5] / 255;
                    ia = 255 - a;
                    layer[o]   = (uint8_t)((tr * a + layer[o]   * ia) / 255);
                    layer[o+1] = (uint8_t)((tg * a + layer[o+1] * ia) / 255);
                    layer[o+2] = (uint8_t)((tb * a + layer[o+2] * ia) / 255);
                    layer[o+3] = (uint8_t)(a + layer[o+3] * ia / 255);
                }
            }
        }
next:
        /* to the next LINE, so one bad record cannot desynchronise the
         * rest of a HUD frame into nonsense */
        while (*p && *p != '\n') p++;
        if (*p == '\n') p++;
        else return;
    }
}

void fb_overlay(const char *payload) { ov_apply(0, payload); }
void fb_overlay_flash(const char *payload) { ov_apply(1, payload); }

/* The alpha of one layer pixel, as the layer holds it RIGHT NOW.
 *
 * The flash is cleared as it composites, so "is the flash still
 * there?" cannot be answered by looking at a presented FRAME — it is
 * black whether the layer survived or not, and a check that reads the
 * frame passes against a device that kept the layer. This asks the
 * layer itself, which is the thing whose state is in question. It is
 * the same reason a game's own "is it still live?" question is asked of
 * its state and not of its output. */
int fb_overlay_alpha(int x, int y) { return fb_overlay_layer_alpha(0, x, y); }

int fb_overlay_layer_alpha(int which, int x, int y) {
    const uint8_t *L;
    if (!O_have || x < 0 || y < 0 || x >= S_w || y >= S_h) return -1;
    L = which ? O_flash : O_ov;
    if (!L) return -1;
    return L[((size_t)y * S_w + x) * 4 + 3];
}

/* ── MIMEcroft's cube geometry ──────────────────────────────────────
 * Generated VERBATIM from MIMEcroft.sh's own buffer writes:
 *   /dev/webgl/buffer/aPosition  24 verts x (x,y,z), x1000
 *   /dev/webgl/buffer/aShade     24 verts x (r,g,b), x1000 (grey)
 *   /dev/webgl/buffer/cube       36 uint16 indices = 12 triangles
 * Transcribing these by hand is how the first version came out with 12
 * vertices and 12 triangles drawing an open wedge instead of a cube.
 *
 * SIGNED, and int16 not char: aPosition is +/-500, and a signed char
 * wraps +500 to -12, so every vertex lands on garbage. The C backend's
 * emit path uses %hhu for uint8_t — a coordinate must never travel in a
 * narrow type.
 *
 * aShade is 0..1000, NOT 0..1. Clamping a 0..1 range flattens every
 * face to solid white; the real span here is 500..1000 (the 0.5
 * bottom, 0.6/0.8 sides, 1.0 top). */
static const int16_t CUBE_P[72] = {
  -500, 500, 500, 500, 500, 500, 500, 500, -500,
  -500, 500, -500, -500, -500, 500, 500, -500, 500,
  500, -500, -500, -500, -500, -500, -500, 500, 500,
  500, 500, 500, 500, -500, 500, -500, -500, 500,
  -500, -500, -500, 500, -500, -500, 500, 500, -500,
  -500, 500, -500, 500, -500, 500, 500, 500, 500,
  500, 500, -500, 500, -500, -500, -500, -500, 500,
  -500, 500, 500, -500, 500, -500, -500, -500, -500,
};

static const int16_t CUBE_SHADE[24] = {
  1000, 1000, 1000, 1000, 500, 500, 500, 500, 600,
  600, 600, 600, 800, 800, 800, 800, 800, 800,
  800, 800, 600, 600, 600, 600,
};

static const uint8_t CUBE_I[36] = {
  0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7,
  8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15,
  16, 17, 18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
};

/* THE SIX FACE OUTWARD NORMALS, indexed by face number (face f is
 * CUBE_I[6f .. 6f+5], the two triangles of one face).
 *
 * Needed for back-face culling. A cube is CONVEX and CLOSED, so a face
 * whose outward normal points away from the eye is behind the cube's
 * own body and cannot be seen from anywhere outside it. Culling those
 * is exact, not an approximation, and it is the single largest cost
 * saving in the frame: 73% of a 252-cube payload (2221 of 3024
 * triangles) is back-facing at the player's cell, and each was being
 * rasterized over its full projected area only to be painted over.
 *
 * THE ORDER IS TOP, BOTTOM, +z, -z, +x, -x — the cube table's own, not
 * the +x/-x/+y/-y/+z/-z that reads more naturally, and the first table
 * written here WAS that guess. It cost 118 lit pixels across the
 * silhouette oracle's 112 cases, because a face culled as "facing away"
 * was in fact a visible one. A hand-written table of face properties is
 * a second transcription of the same geometry, and this file already
 * has one transcription bug on record (see CUBE_P).
 *
 * So this one is generated: the derivation is the three lines below,
 * and tools/fbdev-faces.py re-derives it from CUBE_P/CUBE_I and diffs
 * the result against this array. Regenerate rather than edit. */
static const int8_t FACE_N[6][3] = {
    { 0,  1,  0},   /* top    */
    { 0, -1,  0},   /* bottom */
    { 0,  0,  1},   /* +z     */
    { 0,  0, -1},   /* -z     */
    { 1,  0,  0},   /* +x     */
    {-1,  0,  0},   /* -x     */
};

/* ── backend open/close ───────────────────────────────────────────── */
static int fb_try_dev(const char *dev, int w, int h) {
#ifdef FBDEV_HAVE_FBIO
    struct fb_var_screeninfo v;
    struct fb_fix_screeninfo f;
    int fd = open(dev, O_RDWR);
    if (fd < 0) return 0;
    if (ioctl(fd, FBIOGET_FSCREENINFO, &f) < 0 ||
        ioctl(fd, FBIOGET_VSCREENINFO, &v)) { close(fd); return 0; }
    if (f.type != FB_TYPE_PACKED_PIXELS ||
        (f.visual != FB_VISUAL_TRUECOLOR && f.visual != FB_VISUAL_DIRECTCOLOR)) {
        close(fd);
        return 0;
    }
    if (f.line_length <= 0 || v.xres <= 0 || v.yres <= 0) { close(fd); return 0; }
    {
        size_t len = (size_t)f.line_length * (size_t)v.yres_virtual;
        void *m = mmap(0, len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        if (m == MAP_FAILED) { close(fd); return 0; }
        S_fb_map = (uint8_t *)m;
        S_fb_len = len;
    }
    S_fb_fd   = fd;
    S_fb_bpp  = v.bits_per_pixel;
    S_fb_roff = v.red.offset;   S_fb_goff = v.green.offset;
    S_fb_boff = v.blue.offset;  S_fb_toff = v.transp.offset;
    S_fb_xoff = v.xoffset;      S_fb_yoff = v.yoffset;
    S_fb_vw   = v.xres;         S_fb_vh  = v.yres;
    S_fb_xres = v.xres;         S_fb_yres = v.yres;
    S_fb_vpan = v.yres_virtual;
    S_fb_line = f.line_length;
    /* The surface is the DEVICE's, not ours: adopt its geometry. A draw
     * sized for the request into a differently-sized buffer shears. */
    S_w = v.xres;
    S_h = v.yres;
    (void)w; (void)h;
    return 1;
#else
    (void)dev; (void)w; (void)h;
    return 0;
#endif
}

int fb_open(int w, int h) {
    const char *dev = getenv("FB0_DEV");
    if (w <= 0) w = 320;
    if (h <= 0) h = 240;

    /* Re-open for a NEW size must actually reallocate. Returning early
     * on an already-open surface leaves S_w/S_h at the OLD size while
     * the buffer stays the old (smaller) size — every draw then writes
     * past the end of the heap block and fb_dump_ppm reads out of
     * bounds. (A one-frame-per-process renderer never hits this; a
     * test suite that opens 64x48 then 320x240 hits it immediately.) */
    if (S_px && (S_w != w || S_h != h)) {
        free(S_px);
        S_px = 0;
        S_opened = 0;
    }
    /* A NEW SURFACE IS A NEW HUD. The overlay layer is dropped on every
     * open, not only on a resize, so a game that restarts on a new
     * level does not carry the previous level's radar, gun and labels
     * into the new one. (The settings menu's C-wipe is the game doing
     * the same thing deliberately, which is what told me the layer
     * outlives a frame.) */
    if (O_have) {
        free(O_ov); free(O_flash);
        O_ov = 0; O_flash = 0;
        O_have = 0; O_w = 0; O_h = 0;
    }
    if (S_opened) return 0;
    S_w = w;
    S_h = h;

    if (fb_try_dev(dev ? dev : "/dev/fb0", w, h)) {
        /* a real device: adopt ITS geometry (fb_try_dev overwrote
         * S_w/S_h), and size the RAM mirror to match so the PPM dump
         * and the device never disagree. */
        S_px = (uint32_t *)calloc((size_t)S_w * (size_t)S_h, sizeof(uint32_t));
        if (!S_px) { close(S_fb_fd); S_fb_fd = -1; return -1; }
        S_opened = 1;
        return 0;
    }
    S_px = (uint32_t *)calloc((size_t)w * (size_t)h, sizeof(uint32_t));
    if (!S_px) return -1;
    S_opened = 1;
    return 0;
}

int fb_w(void) { return S_w; }
int fb_h(void) { return S_h; }

void fb_clear(uint32_t argb) {
    size_t n;
    if (!S_px) return;
    n = (size_t)S_w * (size_t)S_h;
    {
        size_t i;
        for (i = 0; i < n; i++) S_px[i] = argb;
    }
}

void fb_set_camera(double x, double y, double z, double yaw, double shift) {
    double rad;
    C_x = x; C_y = y; C_z = z; C_yaw = yaw; C_shift = shift;
    rad = yaw * 3.14159265358979323846 / 180.0;
    C_cos = cos(rad);
    C_sin = sin(rad);
}

/* ── pixel store ──────────────────────────────────────────────────── */

/* Write to the RAM surface AND (when a device is open) the real fb
 * mapping. The RAM copy is what fb_dump_ppm reads; the device copy is
 * what the display shows. */
static void put_px(int x, int y, uint32_t rgb) {
    if (x < 0 || y < 0 || x >= S_w || y >= S_h) return;
    S_px[(size_t)y * (size_t)S_w + (size_t)x] = 0xFF000000u | rgb;
    if (S_fb_map && S_fb_bpp == 32) {
        uint8_t *p = S_fb_map + (size_t)(y + S_fb_yoff) * (size_t)S_fb_line
                        + (size_t)((x + S_fb_xoff) * 4);
        p[S_fb_roff / 8] = (uint8_t)( rgb        & 0xFF);
        p[S_fb_goff / 8] = (uint8_t)((rgb >>  8) & 0xFF);
        p[S_fb_boff / 8] = (uint8_t)((rgb >> 16) & 0xFF);
        /* a device with no alpha reports transp.offset == 0, which is
         * also where some channel lives — writing it there saturates
         * that channel and flattens every gradient. Guard on length. */
        if (S_fb_toff) p[S_fb_toff / 8] = (uint8_t)((rgb >> 24) & 0xFF);
    }
}

/* ── triangle rasterizer ──────────────────────────────────────────── */

/* Half-plane coverage test, shared by the flat and the shaded paths, so
 * both cover IDENTICAL pixel sets (a mismatch would leave seams between
 * adjacent cube faces). Uses the standard edge-function sign test. */
static int tri_covers(double ax, double ay, double bx, double by,
                      double px, double py) {
    return (bx - ax) * (py - ay) - (by - ay) * (px - ax) >= 0.0;
}

/* Rasterize a triangle with a per-vertex shade (barycentric, so a face
 * gets MIMEcroft's smooth 0.5..1.0 gradient rather than a flat fill).
 *
 * WINDING-INDEPENDENT. A fixed "edge >= 0" half-plane test silently
 * culls every face whose projected winding is clockwise — and a cube's
 * winding FLIPS with the viewing angle, so a single hard-coded sign
 * drops 3 of 4 faces the moment the camera moves. The signed area is
 * computed first and the winding normalised, which is what makes this
 * correct for any camera heading. (Measured: with yaw 180 the game's
 * own 4 corner triangles come out CCW, CW, CW, CW.) */
static void tri_shaded(double x0, double y0, double s0, double u0, double v0,
                       double x1, double y1, double s1, double u1, double v1,
                       double x2, double y2, double s2, double u2, double v2,
                       uint32_t rgb, int damage, int face) {
    int x, y;
    double minx, maxx, miny, maxy, area;
    double cr = (double)((rgb >> 16) & 0xFF);
    double cg = (double)((rgb >>  8) & 0xFF);
    double cb = (double)( rgb        & 0xFF);
    /* The per-pixel path needs the face's four corners in TEXTURE
     * space, which the flat path has no use for. They are carried
     * alongside the shade and interpolated barycentrically by exactly
     * the same weights, so a pixel's (u,v) is the point where the
     * face's own uv rectangle says the pixel is — which is what the
     * fragment shader's varying vUv gives it. */
    const Tex *blk = 0, *crk = 0;
    if (FB_TEX_ACTIVE) {
        int t = FB_TEX_IDX;
        if (t >= 0 && t < T_n && T_tex[t].used) blk = &T_tex[t];
        if (CRACK_TEX < T_n && T_tex[CRACK_TEX].used) crk = &T_tex[CRACK_TEX];
    }
    (void)face;

    area = (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0);
    if (area == 0.0) return;
    if (area < 0.0) {           /* normalise: swap v1 and v2 */
        double tx = x1, ty = y1, ts = s1;
        double tu = u1, tv = v1;
        x1 = x2; y1 = y2; s1 = s2; u1 = u2; v1 = v2;
        x2 = tx; y2 = ty; s2 = ts; u2 = tu; v2 = tv;
    }

    /* whole-triangle off-screen: skip (the near-plane clamp can push
     * faces far outside the viewport and we still pay per pixel) */
    minx = x0 < x1 ? (x0 < x2 ? x0 : x2) : (x1 < x2 ? x1 : x2);
    maxx = x0 > x1 ? (x0 > x2 ? x0 : x2) : (x1 > x2 ? x1 : x2);
    miny = y0 < y1 ? (y0 < y2 ? y0 : y2) : (y1 < y2 ? y1 : y2);
    maxy = y0 > y1 ? (y0 > y2 ? y0 : y2) : (y1 > y2 ? y1 : y2);
    if (maxx < 0 || maxy < 0 || minx >= S_w || miny >= S_h) return;

    if (minx < 0) minx = 0;
    if (miny < 0) miny = 0;
    if (maxx > S_w - 1) maxx = S_w - 1;
    if (maxy > S_h - 1) maxy = S_h - 1;

    for (y = (int)miny; y <= (int)maxy; y++) {
        for (x = (int)minx; x <= (int)maxx; x++) {
            double px = x + 0.5, py = y + 0.5;
            double w0, w1, w2, sh;
            int r, g, b;
            if (!tri_covers(x0, y0, x1, y1, px, py)) continue;
            if (!tri_covers(x1, y1, x2, y2, px, py)) continue;
            if (!tri_covers(x2, y2, x0, y0, px, py)) continue;
            w0 = ((x1 - px) * (y2 - py) - (y1 - py) * (x2 - px)) / area;
            w1 = ((x2 - px) * (y0 - py) - (y2 - py) * (x0 - px)) / area;
            w2 = 1.0 - w0 - w1;
            sh = w0 * s0 + w1 * s1 + w2 * s2;
            if (sh < 0.0) sh = 0.0;
            if (sh > 1.0) sh = 1.0;
            if (blk) {
                /* THE TEXTURED PATH: the fragment program, per pixel.
                 *
                 * The shade is NOT rounded to a float here — it is
                 * carried to frag_pixel as the face's aShade ×1000, so
                 * the multiply that the original's vertex shader does
                 * into vcolor stays an integer multiply. Rounding
                 * through a double first is a smooth-looking way to
                 * be wrong on a different number of pixels every run.
                 *
                 * uv is the barycentric mix of the face's three
                 * texture-space corners, which is the same varying the
                 * shader's vUv carries into the fragment stage. */
                int fr, fg, fb;
                double iu = w0 * u0 + w1 * u1 + w2 * u2;
                double iv = w0 * v0 + w1 * v1 + w2 * v2;
                frag_pixel(blk, crk,
                           (int)cr, (int)cg, (int)cb,
                           (int)(sh * 1000.0 + 0.5),
                           damage, x, y, iu, iv,
                           S_w, S_h,
                           &fr, &fg, &fb);
                if (fr > 255) fr = 255;
                if (fg > 255) fg = 255;
                if (fb > 255) fb = 255;
                put_px(x, y, ((uint32_t)fr << 16) | ((uint32_t)fg << 8) | (uint32_t)fb);
                continue;
            }
            r = (int)(cr * sh + 0.5); if (r > 255) r = 255;
            g = (int)(cg * sh + 0.5); if (g > 255) g = 255;
            b = (int)(cb * sh + 0.5); if (b > 255) b = 255;
            put_px(x, y, ((uint32_t)r << 16) | ((uint32_t)g << 8) | (uint32_t)b);
        }
    }
}

/* ── projection ─────────────────────────────────────────────────── */

/* ONE VERTEX IN VIEW SPACE, before the perspective divide.
 *
 * The device projects per face, not per vertex, because a face can
 * STRADDLE the camera plane and has to be clipped before any of it is
 * divided. `w` is kept alongside the position for exactly that reason:
 * dividing a w <= 0 vertex is what turns a wall the eye is inside into
 * a triangle smeared across the whole screen. */
typedef struct {
    double x, y;          /* view-space position (already rotated) */
    double w;             /* depth = -rel.z, the shader's w */
} Vtx;

/* THE NEAR PLANE, in world units.
 *
 * It has to be a real distance, not an epsilon, because a clipped
 * vertex LANDS ON IT and then gets divided: at w = 1e-4 a point half a
 * block to the side projects 560000 px off a 320-px screen, so the
 * triangle keeps a full-screen bounding box and the clip saves the
 * arithmetic but none of the fill cost. (Measured: 0.0001 gave 23
 * ms/frame for the cube pass, essentially the unclipped cost.)
 *
 * It also has to stay well clear of anything real. In MIMEcroft the
 * eye sits at the PLAYER CELL'S CENTRE and a block spans +/-0.5, so the
 * nearest real surface is 0.5 units away; can_enter refuses any cell
 * that is not air, so the eye is never inside a wall. 0.01 is fifty
 * times closer than that, which is margin enough that no cube the game
 * can draw is ever clipped, while bounding a clipped vertex to ~5600 px
 * off screen. */
#define W_NEAR 0.01

/* object-space vertex -> view space. The world transform (aPosition *
 * uScale + uObjPos) and the yaw rotation are the shader's, verbatim;
 * only the divide is deferred. */
static void to_view(double apx, double apy, double apz,
                    double ox, double oy, double oz,
                    double sx, double sy, double sz, Vtx *v) {
    double wx = apx * sx / 1000000.0 + ox;
    double wy = apy * sy / 1000000.0 + oy;
    double wz = apz * sz / 1000000.0 + oz;
    double dx = wx - C_x, dy = wy - (C_y + 0.5), dz = wz - C_z;
    v->x = dx * C_cos + dz * C_sin;
    v->y = dy;
    v->w = dx * C_sin - dz * C_cos;   /* = -rel.z, the shader's w */
}

/* view-space vertex -> pixel, through the shader's clip and divide.
 *
 * SCALE CONTRACT — this is the shader's arithmetic exactly
 * (www/examples/mimecroft-vertex.sh:45):
 *     wx = ap_x * usc_x / 1000000.0 + uop_x / 1000.0
 * aPosition is MILLI (±500 = half a block) and uScale is MILLI (1000 =
 * a full block), so the product needs 1e6, not 1e3. Dividing by 1e3
 * makes every cube 1000x too big: one block at 4 deep then spans the
 * whole screen and a 320x240 frame comes out 76800 lit pixels.
 *
 * The clamp only guards a degenerate vertex: the near-plane clip has
 * already run, so w >= W_NEAR and the divide is finite. */
static void project(const Vtx *v, double *outx, double *outy) {
    double w = v->w, ndcx, ndcy;
    if (w < W_NEAR) w = W_NEAR;
    ndcx = v->x * 0.7 + C_shift * w;
    ndcy = v->y * 0.45;
    /* NDC -> pixel. +y is up in NDC, down in a framebuffer row. */
    *outx = (ndcx / w * 0.5 + 0.5) * S_w;
    *outy = (0.5 - ndcy / w * 0.5) * S_h;
}

/* Lerp a vertex against the near plane, in view space. Linear in every
 * component: the shader's clip is a straight line in view space (the
 * divide happens after), so a crossing point is the same linear mix
 * whether it is found before or after projection. */
static void lerp_v(const Vtx *a, const Vtx *b, double t, Vtx *out) {
    out->x = a->x + (b->x - a->x) * t;
    out->y = a->y + (b->y - a->y) * t;
    out->w = a->w + (b->w - a->w) * t;
}

/* ── payload parsing ──────────────────────────────────────────────── */

/* Draw one cube record: centre (ox,oy,oz), scale ×1000, colour 0..1. */
static void draw_cube(double ox, double oy, double oz,
                      double sx, double sy, double sz,
                      double r, double g, double b,
                      int texidx, int damage) {
    int i;
    uint32_t rgb;
    int cr = (int)(r * 255.0 + 0.5);
    int cg = (int)(g * 255.0 + 0.5);
    int cb = (int)(b * 255.0 + 0.5);
    if (cr < 0) cr = 0;
    if (cr > 255) cr = 255;
    if (cg < 0) cg = 0;
    if (cg > 255) cg = 255;
    if (cb < 0) cb = 0;
    if (cb > 255) cb = 255;
    rgb = ((uint32_t)cr << 16) | ((uint32_t)cg << 8) | (uint32_t)cb;
    FB_TEX_IDX = texidx;
    FB_TEX_ACTIVE = (T_n > 0);

    /* PAINTER'S ALGORITHM: draw far faces first.
     *
     * The game's /dev/webgl/buffer/cube index order is a fixed
     * face list, NOT a depth order — with a real depth buffer the GPU
     * resolves it per pixel, but this device has none. Emitting the
     * faces in index order therefore lets whichever face happens to
     * come last in the table paint over the nearer ones, and a cube
     * collapses to its 0.6/0.8 side faces no matter which way the
     * camera looks (measured: 2 distinct shades for every camera
     * position tried — the top and bottom faces are always lost).
     *
     * Sorting the 12 triangles by their centroid's view-space depth,
     * far to near, makes the payload order the draw order, which is
     * exactly the contract render_frame already relies on when it
     * emits cubes far-to-near. A stable insertion sort: 12 items. */
    {
        int order[12], a, b;
        double depth[12];
        for (i = 0; i < 12; i++) {
            double sx3 = 0, sy3 = 0, sz3 = 0, dsum = 0;
            int k;
            for (k = 0; k < 3; k++) {
                double vx, vy, vz;
                int vi = CUBE_I[i*3+k];
                vx = CUBE_P[vi*3+0] * sx / 1000000.0 + ox;
                vy = CUBE_P[vi*3+1] * sy / 1000000.0 + oy;
                vz = CUBE_P[vi*3+2] * sz / 1000000.0 + oz;
                sx3 += vx; sy3 += vy; sz3 += vz;
            }
            sx3 /= 3.0; sy3 /= 3.0; sz3 /= 3.0;
            {
                double dx = sx3 - C_x, dz = sz3 - C_z;
                double relz = -dx * C_sin + dz * C_cos;
                dsum = -relz;                     /* the shader's w */
            }
            depth[i] = dsum;
            order[i] = i;
        }
        for (i = 1; i < 12; i++) {              /* descending depth */
            int t = order[i];
            double td = depth[t];
            for (a = i - 1; a >= 0 && depth[order[a]] < td; a--)
                order[a + 1] = order[a];
            order[a + 1] = t;
        }
        (void)b;
        for (b = 0; b < 12; b++) {
            int fi = order[b];
            double px[8], py[8], sh[8];  /* the clipped screen-space polygon */
            double pu[8], pv[8];          /* ... and its texture-space corners */
            const double *fuv = CUBE_UV;  /* the cube's own aUv table */
            double gx0 = -1.0, gy0 = -1.0;
            double gx1 = (double)S_w + 1.0, gy1 = (double)S_h + 1.0;
            int np = 0, k;

            /* NEAR-PLANE CLIP, in view space and then in screen space.
             *
             * A face that straddles the camera plane must be clipped
             * before anything is divided by w, exactly as the GPU clips
             * gl_Position against z_clip = -w. Dividing first and then
             * clamping w to a tiny positive number (what this used to
             * do) sends every w <= 0 vertex to infinity, and the face
             * comes out as a triangle smeared across the entire
             * viewport — a wall the eye is inside paints the whole
             * screen with one face's colour.
             *
             * It was not only wrong, it was the frame's whole cost. At
             * the player cell the nearest wall is 0.5 units away, so
             * most of its faces straddle, and 74 of a 310-cube payload
             * had a projected bounding box larger than the whole
             * 320x240 screen — each rasterizing all 76800 pixels.
             *
             * TWO CLIPS, because one is not enough:
             *
             *   1. view space, w > W_NEAR — Sutherland-Hodgman against
             *      the single near plane. A face wholly in front passes
             *      through untouched, so every pixel of the unclipped
             *      path is bit-identical.
             *   2. screen space, a GUARD BAND around the viewport. The
             *      vertex this clip just created sits ON the near
             *      plane, and dividing by a near distance is exactly
             *      what makes it enormous: at W_NEAR = 0.01 it is still
             *      ~5600 px off a 320-px screen, so the rasterizer
             *      still walks a full-screen bounding box. (At
             *      W_NEAR = 1e-4 it was 560000 px, and the clip bought
             *      nothing at all: 23.0 -> 21.6 ms/frame.) Clipping the
             *      polygon to a small margin of the viewport shrinks
             *      that bounding box to the margin, so a face at the eye
             *      costs a few thousand pixels instead of 76800.
             *
             * The margin is 1 px, and it is there so the bounding-box
             * clamp in tri_shaded is not the thing that decides
             * coverage: the band lies strictly OUTSIDE the viewport, so
             * no pixel inside it changes. A face whose crossing is
             * off-screen loses the sliver it would have contributed
             * there — the same call a GPU makes with a finite
             * rasterizer guard band.
             *
             * A triangle clipped by five planes has at most 3 + 5 = 8
             * corners, so the 8 slots below are the exact bound. */

            /* BACK-FACE CULL, before any of that. See FACE_N: a closed
             * convex cube's rear faces are behind its own body, so this
             * drops 73% of a typical frame's triangles and changes not
             * one pixel. The two triangles of a face always agree,
             * because the test is per face and never per triangle, so a
             * face is never half-drawn.
             *
             * CULLED ONLY WHEN ALL THREE VERTICES FACE AWAY. A test on
             * the face's CENTROID was the obvious version and it is
             * wrong at the silhouette: the centroid of a face seen
             * edge-on sits at dot ~= 0 while the face still owns a
             * one-pixel-wide sliver of the outline, and culling it left
             * a hole — the silhouette oracle reported exactly one
             * missing pixel, at the corner of a cube whose +x and -z
             * faces were being dropped while the top face's own sliver
             * fell a third of a pixel short of covering it. Requiring
             * all three vertices is also what a GPU does, and it is
             * still exact: a face of a convex solid with every vertex
             * facing away is behind the body, full stop. */
            {
                const int8_t *n = FACE_N[fi / 2];
                double nvx = n[0] * C_cos + n[2] * C_sin;
                double nvz = -n[0] * C_sin + n[2] * C_cos;
                int front = 0, e;
                for (e = 0; e < 3; e++) {
                    int vi = CUBE_I[fi*3+e];
                    double fx = CUBE_P[vi*3+0] * sx / 1000000.0 + ox - C_x;
                    double fy = CUBE_P[vi*3+1] * sy / 1000000.0 + oy - (C_y + 0.5);
                    double fz = CUBE_P[vi*3+2] * sz / 1000000.0 + oz - C_z;
                    double dx = fx * C_cos + fz * C_sin;
                    double dz = -fx * C_sin + fz * C_cos;
                    if (nvx * dx + n[1] * fy + nvz * dz >= 0.0) front = 1;
                }
                if (!front) continue;
            }

            for (k = 0; k < 3; k++) {
                int vi = CUBE_I[fi*3+k];
                int vj = CUBE_I[fi*3+(k + 1) % 3];
                Vtx a, b2, iv;
                double s0 = CUBE_SHADE[vi] / 1000.0;
                double s1 = CUBE_SHADE[vj] / 1000.0;
                to_view(CUBE_P[vi*3+0], CUBE_P[vi*3+1], CUBE_P[vi*3+2],
                        ox, oy, oz, sx, sy, sz, &a);
                to_view(CUBE_P[vj*3+0], CUBE_P[vj*3+1], CUBE_P[vj*3+2],
                        ox, oy, oz, sx, sy, sz, &b2);
                if (a.w > W_NEAR && np < 8) {
                    project(&a, &px[np], &py[np]);
                    sh[np] = s0;
                    pu[np] = fuv[vi*2+0];
                    pv[np] = fuv[vi*2+1];
                    np++;
                }
                if ((a.w > W_NEAR) != (b2.w > W_NEAR) && np < 8) {
                    double u = (W_NEAR - a.w) / (b2.w - a.w);
                    lerp_v(&a, &b2, u, &iv);
                    project(&iv, &px[np], &py[np]);
                    sh[np] = s0 + (s1 - s0) * u;
                    pu[np] = fuv[vi*2+0] + (fuv[vj*2+0] - fuv[vi*2+0]) * u;
                    pv[np] = fuv[vi*2+1] + (fuv[vj*2+1] - fuv[vi*2+1]) * u;
                    np++;
                }
            }
            if (np < 3) continue;

            /* The guard band: four Sutherland-Hodgman passes, one per
             * edge, each keeping the half-plane
             *     dot(n, p) + d >= 0
             * where n is the OUTWARD normal of the edge. The `d` sign
             * is what makes it an upper bound: for `x <= gx1` the
             * outward normal is (-1, 0) and the plane passes through
             * (gx1, 0), so d = gx1 — not -gx1, which would be the
             * half-plane `x <= -gx1` and silently discard the entire
             * polygon. (It did: every face vanished, and the silhouette
             * oracle reported 816 missing pixels, 0 outside the hull.)
             *
             * THE OUTPUT GOES IN A SECOND BUFFER, not in place. Doing it
             * in place is the tempting one-line version and it is
             * wrong: an edge can emit a kept vertex AND a crossing, so
             * the write index overtakes the read index, and the write
             * lands on a corner the pass has not read yet. It showed up
             * as one face whose polygon collapsed to four copies of a
             * single point — a fan of degenerate slivers — and the
             * silhouette oracle reported one missing pixel at exactly
             * that face's corner. Clipping into a scratch array and
             * copying back costs one 8-element struct copy per pass. */
            {
                double qx[8], qy[8], qs[8], qu[8], qv[8];
                for (k = 0; k < 4 && np >= 3; k++) {
                    double nx, ny, d;
                    int m = 0, e;
                    switch (k) {
                    case 0: nx =  1.0; ny =  0.0; d = -gx0; break; /* x >= gx0 */
                    case 1: nx = -1.0; ny =  0.0; d =  gx1; break; /* x <= gx1 */
                    case 2: nx =  0.0; ny =  1.0; d = -gy0; break; /* y >= gy0 */
                    default:nx =  0.0; ny = -1.0; d =  gy1; break; /* y <= gy1 */
                    }
                    for (e = 0; e < np; e++) {
                        int e2 = (e + 1) % np;
                        double d0 = px[e] * nx + py[e] * ny + d;
                        double d1 = px[e2] * nx + py[e2] * ny + d;
                        if (d0 >= 0.0 && m < 8) {
                            qx[m] = px[e]; qy[m] = py[e]; qs[m] = sh[e];
                            qu[m] = pu[e]; qv[m] = pv[e]; m++;
                        }
                        if ((d0 >= 0.0) != (d1 >= 0.0) && m < 8) {
                            double u = d0 / (d0 - d1);
                            qx[m] = px[e] + (px[e2] - px[e]) * u;
                            qy[m] = py[e] + (py[e2] - py[e]) * u;
                            qs[m] = sh[e] + (sh[e2] - sh[e]) * u;
                            qu[m] = pu[e] + (pu[e2] - pu[e]) * u;
                            qv[m] = pv[e] + (pv[e2] - pv[e]) * u;
                            m++;
                        }
                    }
                    np = m;
                    for (e = 0; e < np; e++) {
                        px[e] = qx[e]; py[e] = qy[e]; sh[e] = qs[e];
                        pu[e] = qu[e]; pv[e] = qv[e];
                    }
                }
            }
            /* fan-triangulate */
            for (k = 2; k < np; k++)
                tri_shaded(px[0], py[0], sh[0], pu[0], pv[0],
                           px[k - 1], py[k - 1], sh[k - 1], pu[k - 1], pv[k - 1],
                           px[k], py[k], sh[k], pu[k], pv[k],
                           rgb, damage, fi / 2);
        }
    }
}

void fb_blocks(const char *payload) {
    const char *p = payload;
    if (!p || !S_px) return;
    for (;;) {
        double v[11];
        int n = 0;
        int complained = 0;
        /* A RECORD IS A LINE. Parsing with a bare strtod loop over the
         * whole buffer cannot tell a truncated record from a complete
         * one: a record short one field simply borrows the next
         * record's first number as its own, and the parser desynchronises
         * for the rest of the payload without ever seeing a short tail.
         * That is not a hypothetical — it is exactly what happened when
         * MIMEcroft's world arrays were half the size the cells index to,
         * so every cube record lost its damage field. The frame came out
         * a flat grey and 23 equivalence cases passed byte for byte,
         * because both sides replay the same malformed payload through
         * the same device.
         *
         * So the record boundary is the line, and a line that does not
         * hold 11 fields is REPORTED rather than quietly absorbed. The
         * fields are still drawn if there are enough of them, because a
         * caller with one bad record should see the rest of the frame.
         */
        while (n < 11) {
            char *end;
            double d;
            const char *q = p;
            /* the field separator is a space or a TAB, never a newline:
             * letting strtod cross a line break is what let one record
             * eat the next one's first field */
            while (*q == ' ' || *q == '\t') q++;
            if (*q == 0 || *q == '\n' || *q == '\r') break;
            d = strtod(q, &end);
            if (end == q) {
                if (!complained) {
                    fprintf(stderr,
                            "fb_blocks: unparseable field at \"%.16s\";"
                            " the rest of this payload is not drawn\n", q);
                    complained = 1;
                }
                return;
            }
            v[n++] = d;
            p = end;
        }
        if (n == 11) {
            /* v[0..2] centre (world units), v[3..5] scale (world units: 1 =
             * a full block), v[6..8] colour 0..1, v[9] block type, v[10]
             * damage. The scale crosses into project() as MILLI (×1000),
             * matching the shader's usc_* — the one place it converts.
             *
             * v[9] and v[10] are PARSED AND IGNORED, deliberately. The
             * block type selects a texture on the WebGL path, and the
             * damage field accumulates mining damage upstream
             * (www/bin/mimecroft.sh:942, damage_cell). This device
             * shades cubes from the 6 face shades in CUBE_SHADE and has
             * no notion of either, so the colour in v[6..8] is the whole
             * truth it can draw.
             *
             * They are still REQUIRED, and that is the point worth
             * recording: the payload contract is 11 fields, and a caller
             * that emitted only 8 got a silently desynchronised frame for
             * as long as the parser took fields across a newline. Making
             * them required-but-unused is honest — dropping them would
             * change the record's shape and re-open that door. */
            draw_cube(v[0], v[1], v[2], v[3] * 1000.0, v[4] * 1000.0,
                      v[5] * 1000.0, v[6], v[7], v[8],
                      (int)v[9], (int)v[10]);
        } else if (n > 0 && !complained) {
            /* A SHORT LINE, with fields on it. Reported, because from the
             * caller's side a truncated payload and an empty one are the
             * same picture: a frame that never changed. n == 0 is the
             * payload simply ending (a trailing newline is normal) and
             * stays silent. */
            fprintf(stderr,
                    "fb_blocks: record ended after %d of 11 fields"
                    " (at \"%.16s\"); the rest of this payload is"
                    " not drawn\n", n, p);
        }
        /* advance to the next line, so the desync cannot continue */
        while (*p && *p != '\n') p++;
        if (*p == '\n') p++;
        else return;
    }
}

int fb_window_open(void) {
    /* LAZY, AND THE SIZE IS THE SURFACE'S OWN. A real /dev/fb0 is the
     * display already, so there is nothing to open and nothing to
     * report. */
    if (S_fb_map) return 1;
    if (!S_px) return 0;
    return fbwin_open(S_w, S_h);
}

const char *fb_window_key(void) {
    static char k[8];
    k[0] = 0;
    if (S_fb_map) return k;
    fbwin_key(k, (int)sizeof k);
    return k;
}

/* Non-zero when the window manager asked the window to close. Returns
 * ZERO when there is no window at all, so the shell test that reads it
 * is `if fb_window_closed` — "if the window was closed" — and never
 * fires on a headless run.
 *
 * THE POLARITY IS THE WHOLE POINT and it is a measurement, not a
 * preference. Returning 1 for "there is no window" (which is what the
 * name can be read to mean, and what a caller wanting "is there a
 * window" would want) makes every headless game quit on its FIRST
 * frame: the shell test `if fb_window_closed` is true, the game treats
 * it as the quit key, and a run that should have drawn 5 frames drew
 * 1 — the same symptom as a crashed device, and the worst possible
 * place for it. Two predicates, opposite senses, both about a window:
 * fb_window_open asks "is there one", fb_window_closed asks "did it
 * go away". A game that needs the first gets a non-zero only when a
 * window EXISTS. */
int fb_window_closed(void) {
    if (S_fb_map) return 0;
    return fbwin_quit_p();
}

int fb_present(void) {
    /* THE FRAME LEAVES THE DEVICE HERE, or nowhere at all.
     *
     * This used to be `return 0` with a comment saying the display was
     * somebody else's business, and the RAM path's only output was a
     * PPM written at the END of the run. That is right on a box with a
     * real /dev/fb0 and wrong everywhere else: without it there was no
     * frame on screen at ANY point, so a one-frame run drew a maze,
     * wrote a file and exited, and a game loop ran to its budget with
     * nothing to look at. The fbwin window (fbwin.{h,c}) is the
     * virtual framebuffer that presents it, and it opens lazily HERE
     * rather than in fb_open: opening at fb_open would make every
     * program that only ever wanted a PPM connect to the X server,
     * which is a round trip, a lock, and on a busy display a stall,
     * for a window nobody asked for.
     *
     * Returns 1 when a blit happened, so a caller can tell a drawn
     * frame from an undrawn one without keeping its own state. */
    if (S_px) {
        /* THE OVERLAY IS COMPOSITED HERE, and here rather than at the
         * end of every draw call because it is a SEPARATE surface: the
         * world is drawn first, into the RAM buffer, and the HUD goes
         * over it once per presented frame. Compositing per draw call
         * would put the HUD under the next cube, which is behind it in
         * depth and in the payload's order. */
        ov_composite();
        fbwin_present(S_px, S_w, S_h);
    }
    return 0;
}

int fb_dump_ppm(const char *path) {
    FILE *f;
    int x, y;
    if (!S_px) return -1;
    f = fopen(path, "wb");
    if (!f) return -1;
    fprintf(f, "P6\n%d %d\n255\n", S_w, S_h);
    for (y = 0; y < S_h; y++) {
        for (x = 0; x < S_w; x++) {
            uint32_t p = S_px[(size_t)y * (size_t)S_w + (size_t)x];
            unsigned char rgb[3];
            rgb[0] = (unsigned char)((p >> 16) & 0xFF);
            rgb[1] = (unsigned char)((p >>  8) & 0xFF);
            rgb[2] = (unsigned char)( p        & 0xFF);
            fwrite(rgb, 1, 3, f);
        }
    }
    fclose(f);
    return 0;
}
