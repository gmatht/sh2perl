/* fbdev.h — the fb0 framebuffer device for bash-O4 renders.
 *
 * A native (no WebGL) sink for a shell script's draw calls. The render
 * path in MIMEcroft.sh builds a batched "x y z sx sy sz r g b tx dam"
 * payload per frame and hands it to the device; here that payload is
 * rasterized with MIMEcroft's own projection into a linear pixel buffer
 * that is either mmap'd from a real /dev/fb0 or kept in RAM and dumped
 * as a PPM (this host has fbcon but no /dev/fb0, so the RAM path is the
 * one that runs).
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
 * damage (ignored by the flat-shaded fb path). */
void fb_blocks(const char *payload);

/* Present the frame: copy RAM -> /dev/fb0 when a real device is open
 * (fb_flush does it lazily on close for the RAM path, so this is the
 * explicit "frame done" call). Returns 0 on success. */
int  fb_present(void);

/* Dump the RAM surface as a binary P6 PPM (no-op when a real /dev/fb0
 * is open — that surface is the display, not a file). */
int  fb_dump_ppm(const char *path);

#endif /* FBDEV_H */
