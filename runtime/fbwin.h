/* fbwin.h — a real, live X11 window for the fb0 device.
 *
 * THE PROBLEM THIS SOLVES
 *   fbdev's RAM backend was a framebuffer in name only: it filled a
 *   linear pixel buffer and, at the end, wrote a PPM file. A game loop
 *   drew into RAM that nothing displayed, so the run was a maze you
 *   could only see AFTER it stopped — and a one-frame run wrote a file
 *   and exited, which is indistinguishable from a crash. The device had
 *   no notion of a window at all.
 *
 *   This is that window. It is the "virtual framebuffer" a headless box
 *   can actually put a frame on: a real top-level X11 window that the
 *   game's pixels are blitted into, every frame, at the frame's own
 *   resolution, with no scaling, no compositor and no image file.
 *
 * WHY IT IS NOT JUST -lX11 ON THE LINK LINE
 *   Two reasons, and the first is a hard blocker:
 *
 *   1. Xlib needs -lX11, and the link line is derived from the RENDER
 *      by a substring scan (bash-o4/src/tcc.rs, link_needs). fbdev.c
 *      names <X11/Xlib.h> only to CALL into it, and if that header is
 *      absent on a box with no X11 (every framebuffer box, every
 *      container, every CI runner) the C backend still emits
 *      `#include <fbdev.h>` unchanged — the include is keyed on the
 *      script naming an fb_* primitive, not on headers existing — and
 *      the render would fail to COMPILE. A device that has to be
 *      present to be linkable is not a fallback.
 *
 *   2. Even with the header present, -lX11 would make the DEVICE
 *      itself require X11, so the plain fb0 (mmap a real /dev/fb0)
 *      and the headless PPM path would each carry a dependency they do
 *      not use.
 *
 *   So libX11 is opened with dlopen/dlsym AT RUNTIME, and this file
 *   declares the handful of Xlib entry points it uses itself instead of
 *   including <X11/Xlib.h>. The X protocol is fixed and these
 *   signatures are ABI-stable and documented; declaring them is the
 *   only way to depend on a library without being able to compile
 *   against it. Consequence: fbwin.c compiles on a box with no X11
 *   headers at all (verified: cc and tcc, no /usr/include/X11 needed),
 *   and the fb0 device links unchanged.
 *
 * WHERE IT SITS
 *   fbdev.c owns the pixels. This file owns the window: it is handed
 *   the RAM surface and pushes it to X on every present. fbdev.c has no
 *   idea X exists, and this file has no idea what a cube is.
 */
#ifndef FBWIN_H
#define FBWIN_H

#include <stdint.h>

/* Open a window of w x h pixels and show it. Returns 1 on success.
 *
 * Idempotent: a second call with the same size is a no-op (and does not
 * steal focus), a call with a different size re-maps the window at the
 * new size. Returns 0 when there is no X display, when libX11 is not
 * installed, or when the window cannot be created — every one of which
 * is an ordinary outcome, not an error to report, because the fb0
 * device does not need a window to draw.
 *
 * $FBWIN is honoured when set: 0 forces "no window" (so a test, or a
 * run that only wants the PPM, never opens anything), 1 asks for one.
 * Unset, the answer is "yes if there is an X display to ask". */
int  fbwin_open(int w, int h);

/* Blit the w x h 0xAARRGGBB surface to the window and flush. Returns 1
 * when a blit happened, 0 when the window was not open or the frame
 * needed no redraw.
 *
 * The blit is SKIPPED when nothing has changed since the last one: an
 * EXPOSE, a CONFIGURE and a present all mark the window dirty, and an
 * unchanged frame costs nothing. The device is double-buffered in the
 * sense that matters here — fbdev keeps its own surface and this only
 * touches the window's — so a present never disturbs the pixels the
 * game is still drawing into.
 *
 * A surface of a size other than the window's is not blitted: a
 * re-map here would steal focus mid-run, and the game's next fb_open
 * is the thing that sizes the window. */
int  fbwin_present(const uint32_t *argb, int w, int h);

/* One non-blocking X event, translated to the game's own key name
 * (w/a/s/d/q/e/r/./...) through the SERVER's own keycode->character
 * table, so the keys follow the keyboard's layout. Returns 1 when a key
 * was produced, filling `out`; returns 0 when nothing is pending.
 *
 * The names are the game's, not keycodes and not keysyms: a script that
 * branched on a keycode would be wrong on any other machine, and one
 * that branched on a keysym would need its own copy of the table. */
int  fbwin_key(char *out, int cap);

/* Non-zero once the window manager's CLOSE has been asked for (the
 * game pressed it, or the WM sent WM_DELETE_WINDOW). A game should end
 * its loop on this exactly as it ends on the `.` key, so closing the
 * window quits the game instead of leaving a frozen last frame. */
int  fbwin_quit_p(void);

/* X error count and the last error code, for a caller that wants to say
 * "the window could not present" instead of silently drawing into
 * nothing. Zero is the healthy value. */
int  fbwin_errors(void);
int  fbwin_last_error(void);

/* Close the window. Safe when no window was ever opened. */
void fbwin_close(void);

/* Round-trip the server, so everything queued (the window map, the
 * last blit) has been processed when this returns. A TEST that reads
 * the window back with another X client needs it: XMapWindow and
 * XPutImage are both asynchronous, so a capture issued straight after
 * them can be served before the server has done either, and the frame
 * comes back empty — a failure indistinguishable from a blit that did
 * nothing. A game does not need it (XFlush at the end of every present
 * is enough for the display to stay in step). */
void fbwin_sync(void);

/* The window's XID, or 0 when no window is open. Exposed so a TEST
 * can hand it to an independent X client (xwd -id) and read the frame
 * back through the server: that is the only check that cannot be
 * satisfied by our own bookkeeping. It is a window id, which is
 * meaningless off this display, and a game has no use for it. */
unsigned long fbwin_window_id(void);

/* Non-zero when a window is open. A game can use this to pick a
 * frame's pacing (sleep when there is no window to vsync against)
 * without having to know anything about X. */
int  fbwin_open_p(void);

#endif /* FBWIN_H */
