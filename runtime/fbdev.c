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
static void tri_shaded(double x0, double y0, double s0,
                       double x1, double y1, double s1,
                       double x2, double y2, double s2,
                       uint32_t rgb) {
    int x, y;
    double minx, maxx, miny, maxy, area;
    double cr = (double)((rgb >> 16) & 0xFF);
    double cg = (double)((rgb >>  8) & 0xFF);
    double cb = (double)( rgb        & 0xFF);

    area = (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0);
    if (area == 0.0) return;
    if (area < 0.0) {           /* normalise: swap v1 and v2 */
        double tx = x1, ty = y1, ts = s1;
        x1 = x2; y1 = y2; s1 = s2;
        x2 = tx; y2 = ty; s2 = ts;
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
    double x, y, z;        /* view-space position (already rotated) */
    double w;              /* depth = -z after the yaw rotation */
} Vtx;

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
 * The caller has already clipped w > W_NEAR, so the clamp below never
 * fires in the normal path; it is kept so a degenerate vertex cannot
 * produce a NaN coordinate that would poison the bounding box. */
#define W_NEAR 1e-4

static int project(const Vtx *v, double *outx, double *outy) {
    double w = v->w, ndcx, ndcy;
    if (w < W_NEAR) w = W_NEAR;
    ndcx = v->x * 0.7 + C_shift * w;
    ndcy = v->y * 0.45;
    /* NDC -> pixel. +y is up in NDC, down in a framebuffer row. */
    *outx = (ndcx / w * 0.5 + 0.5) * S_w;
    *outy = (0.5 - ndcy / w * 0.5) * S_h;
    return 1;
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
                      double r, double g, double b) {
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
            Vtx vin[3];
            double vshade[3];
            Vtx poly[8];          /* a triangle clipped by one plane
                                   * is at most a quad; 8 is slack */
            double shade[8];
            int np = 0, k;
            for (k = 0; k < 3; k++) {
                int vi = CUBE_I[fi*3+k];
                to_view(CUBE_P[vi*3+0], CUBE_P[vi*3+1], CUBE_P[vi*3+2],
                        ox, oy, oz, sx, sy, sz, &vin[k]);
                vshade[k] = CUBE_SHADE[vi] / 1000.0;
            }
            /* NEAR-PLANE CLIP, against w > W_NEAR.
             *
             * A face that straddles the camera plane has to be clipped
             * before anything is divided by w, exactly as the GPU clips
             * gl_Position against z_clip = -w. Dividing first and then
             * clamping w to a tiny positive number (what this used to
             * do) sends any w <= 0 vertex to infinity, and the face
             * comes out as a triangle smeared across the entire
             * viewport — a wall the eye is inside paints the whole
             * screen with one face's colour.
             *
             * It was not only wrong, it was the frame's whole cost: at
             * the player cell the near wall is 0.5 units away, so most
             * of its faces straddle, and 74 of a 310-cube payload had a
             * projected bounding box larger than the full 320x240
             * screen. Each one rasterized all 76800 pixels. Measured
             * 23 ms/frame for the cube pass, of which almost all was
             * filling triangles that should never have existed.
             *
             * Sutherland–Hodgman against the single plane w = W_NEAR.
             * A triangle wholly in front passes through untouched, so
             * every pixel of the unclipped path is bit-identical. */
            for (k = 0; k < 3; k++) {
                const Vtx *a = &vin[k];
                const Vtx *b2 = &vin[(k + 1) % 3];
                int ain = a->w > W_NEAR, bin = b2->w > W_NEAR;
                if (ain) {
                    if (np < 8) { poly[np] = *a; shade[np] = vshade[k]; np++; }
                }
                if (ain != bin) {
                    double t = (W_NEAR - a->w) / (b2->w - a->w);
                    Vtx iv;
                    if (np < 8) {
                        lerp_v(a, b2, t, &iv);
                        poly[np] = iv;
                        shade[np] = vshade[k] + (vshade[(k + 1) % 3]
                                                 - vshade[k]) * t;
                        np++;
                    }
                }
            }
            /* fan-triangulate the (3- or 4-)gon */
            for (k = 2; k < np; k++) {
                double p0x, p0y, p1x, p1y, p2x, p2y;
                if (!project(&poly[0], &p0x, &p0y)) continue;
                if (!project(&poly[k - 1], &p1x, &p1y)) continue;
                if (!project(&poly[k], &p2x, &p2y)) continue;
                tri_shaded(p0x, p0y, shade[0], p1x, p1y, shade[k - 1],
                           p2x, p2y, shade[k], rgb);
            }
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
                      v[5] * 1000.0, v[6], v[7], v[8]);
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

int fb_present(void) { return 0; }

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
