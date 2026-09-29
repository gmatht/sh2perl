/* fbdev.h — the fb0 framebuffer device for bash-O4 renders.
 *
 * A native (no WebGL) sink for a shell script's draw calls. The render
 * path in MIMEcroft.sh builds a batched "x y z sx sy sz r g b tx dam"
 * payload per frame and hands it to the device; here that payload is
 * rasterized with MIMEcroft's own projection into a linear pixel buffer
 * that is either mmap'd from a real /dev/fb0 or kept in RAM and shown
 * in an X11 window (fbwin.h) — and, headless, dumped as a PPM.
 *
 * THREE SURFACES, IN ORDER OF PREFERENCE, and the order is the point:
 *   1. a real /dev/fb0, mmapped. Every draw lands on the display.
 *   2. an X11 window (fbwin.{h,c}) — the VIRTUAL FRAMEBUFFER, and what
 *      a box with a display and no framebuffer gets. fb_present blits
 *      to it every frame, so a game loop is a game loop on screen.
 *   3. RAM only, for a box with neither. fb_dump_ppm is the last
 *      resort, and it is per frame by default in MIMEcroft_fb.sh
 *      because a file nobody opens is not a picture.
 *
 * libX11 is dlopen'd, not linked, so a program that names an fb_*
 * primitive builds on a box with no X11 headers at all — which is
 * every framebuffer box, every container and every CI runner. The
 * window costs one dlopen probe when there is no display and nothing
 * at all on a box that never gets as far as asking.
 *
 * Painter's algorithm, no depth buffer: the caller emits far-to-near,
 * so payload order IS draw order (this is what render_frame already
 * does — see www/bin/MIMEcroft.sh's cell walk).
 */
#ifndef FBDEV_H
#define FBDEV_H

#include <stdint.h>

/* Surface lifecycle. fb_open picks the backend:
 *   FB_BACKEND_AUTO (0) -> /dev/fb0 if present and mmap-able, else RAM
 *   FB_BACKEND_FB0  (1) -> /dev/fb0, fail loudly if absent
 *   FB_BACKEND_RAM  (2) -> RAM + PPM dump
 * An explicit $FB0_DEV overrides the device path. */
int  fb_open(int w, int h);
int  fb_w(void);
int  fb_h(void);
void fb_clear(uint32_t argb);

/* Camera uniforms: fb_set_camera x y z yaw shift.
 * x/y/z and yaw are world units; the device converts yaw to the
 * cos/sin pair internally. shift is the right-shift already in world
 * units (MIMEcroft's cam_shift, normally 0). */
void fb_set_camera(double x, double y, double z, double yaw, double shift);

/* Batched cubes: one "x y z sx sy sz r g b tx dam" record per line of
 * `payload`. Records are drawn in order (painter's algorithm).
 * r/g/b are 0..1; tx is the block-type (texture slot) and dam the
 * damage (the block shader's crack mix). */
void fb_blocks(const char *payload);

/* ── the block textures, and the per-pixel fragment stage ────────────
 *
 * On the WebGL path a block is NOT a flat colour: the fragment shader
 * (www/bin/mimecroft.sh, emit_fragment_shader) samples a block
 * texture with fract(vUv), multiplies the per-face tint by it, mixes a
 * crack texture in by the damage level, and optionally applies CRT
 * scanlines, corruption streaks and a vignette. That is what makes the
 * blocks look like stone rather than like painted boxes, and it is the
 * largest fidelity gap a flat-shaded port has.
 *
 * So the device keeps the textures and runs the same integer
 * arithmetic per covered pixel, in the same ORDER. The order is
 * load-bearing and is not a cosmetic detail: the original dims for the
 * CRT scanline BEFORE the crack blend, so the crack is not dimmed with
 * the texture, and a port that reorders the two is a different game.
 *
 * The arithmetic is transcribed, not reimplemented. Every divide is the
 * original's integer divide (the shader generator's `/` on a mediump
 * int is an integer divide), so a /128 or /256 is a shift or a
 * truncating divide and NOT a float. A float reimplementation is
 * smooth and wrong, and wrong by one unit on a lot of pixels — which
 * is exactly the class of difference a byte-exact comparison against
 * real bash is there to catch. */

/* One texture: `size` × `size` RGB triples, row-major, plus the alpha
 * the crack texture carries (cr_a). Stored as four bytes per texel so
 * the fragment stage reads one row of an RGBA image rather than three
 * interleaved planes. */
int  fb_texture(int idx, const char *r, const char *g, const char *b,
                int n, int size, int rgba);

/* Non-zero once fb_texture has been called at all, so a caller can
 * tell "no textures loaded" from "textures loaded, all zero". */
int  fb_tex_count(void);

/* The fragment program: which optional stages are compiled in, and the
 * canvas size the vignette centres on. Set once per frame from the
 * settings (CRT_ON / CORRUPT_ON) and the surface size; the C path has
 * no shader to recompile, so this is just state. */
void fb_frag_flags(int crt_on, int corrupt_on);

/* The canvas the fragment program's vignette is centred on. The
 * original bakes disp_w/disp_h into the GENERATED program, so it is
 * fixed until the program is re-emitted; 0 (the default) means "the
 * surface", which is correct for a game that never resizes. A caller
 * that changes the DISPLAY row re-emits the shader, and so sets this. */
void fb_frag_view(int w, int h);

/* The colour of ONE point of a block face, through the whole fragment
 * program. `u`/`v` are 0..1 across the face, `shade` is the face's
 * aShade ×1000 (500..1000), and tint is the record's r/g/b ×255.
 *
 * This is not a test-only hook. The settings menu draws a THUMBNAIL of
 * each block texture (draw_settings_menu's sm_tex_thumb_line), and a
 * thumbnail is the fragment program sampled at a handful of points —
 * asking the device for the colour is the only way to get one that
 * matches what the world will actually look like, cracks and all. It is
 * also how the fragment arithmetic is checked against an independent
 * oracle (tools/fbdev-frag.py): a test that has to model the whole
 * rasterizer to ask "what colour is this pixel" is testing the wrong
 * thing, and would agree with a broken device by sharing its bug. */
void fb_frag_color(int texidx, int shade, int tr, int tg, int tb,
                   int damage, double u, double v, int px, int py,
                   int *outr, int *outg, int *outb);

/* Present the frame: copy RAM -> /dev/fb0 when a real device is open
 * (fb_flush does it lazily on close for the RAM path, so this is the
 * explicit "frame done" call). Returns 0 on success. */
/* Present the frame. Copies RAM -> the fbwin window when one is open
 * (that is the display for a box with no /dev/fb0); a no-op on a real
 * framebuffer, where the draws already landed in the mapping. Returns
 * 1 when a blit actually happened, 0 otherwise, so a caller can tell a
 * drawn frame from an undrawn one. */
int  fb_present(void);

/* ── the window, as a game sees it ─────────────────────────────────
 *
 * Three primitives, and each one exists because the alternative was a
 * fact the game had to know about the platform: whether there is a
 * window, what key was pressed IN it, and whether it was closed. A
 * game that has to ask "is there a display?" in shell is a game that
 * cannot be written once for both a framebuffer and a desktop. */

int  fb_window_open(void);
const char *fb_window_key(void);
int  fb_window_closed(void);

/* Dump the RAM surface as a binary P6 PPM (no-op when a real /dev/fb0
 * is open — that surface is the display, not a file). */
int  fb_dump_ppm(const char *path);

#endif /* FBDEV_H */
