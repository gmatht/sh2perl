/* fbwin-test.c — the window, checked against something that can SEE it.
 *
 * A window that is opened and never verified is exactly the failure
 * this file exists to prevent: the first version of the device drew
 * into RAM and the only proof anything rendered was a PPM nobody
 * looked at. So this checks the window by READING IT BACK with an
 * independent X client and comparing the pixels to what was drawn —
 * the same shape as the silhouette oracle, for the window.
 *
 * Two rules this file follows after both of them cost a debugging
 * session:
 *   * a check must go through the SERVER, not through the device's own
 *     buffer, or it cannot fail in the way that matters;
 *   * the child that does the read-back holds the window OPEN for the
 *     whole capture, and syncs the server, because a window that is
 *     unmapped between the blit and the capture reads back as the
 *     background — which passes a "did it crash" check and fails a
 *     "is the frame there" one.
 */
#define _GNU_SOURCE
#include "fbdev.h"
#include "fbwin.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

static int fails;

static void check(int ok, const char *what) {
    printf("%-52s %s\n", what, ok ? "ok" : "FAIL");
    if (!ok) fails++;
}

/* ── a PPM reader ──────────────────────────────────────────────────
 *
 * The raster starts on the byte AFTER the single whitespace that ends
 * the header, and there is no trailing space: the header is written as
 * "255\n". A reader that skips "the rest of the line" eats the first
 * pixel and every count afterwards is shifted by one, which shows up
 * as a picture whose background is right and whose content is subtly
 * wrong. */
static int ppm_load(const char *path, unsigned char **out, int *w, int *h) {
    FILE *f = fopen(path, "rb");
    char magic[3];
    int maxv, n;
    unsigned char *px;
    if (!f) return 0;
    if (fscanf(f, "%2s %d %d %d", magic, w, h, &maxv) != 4) { fclose(f); return 0; }
    fgetc(f);
    n = *w * *h * 3;
    px = (unsigned char *)malloc((size_t)n);
    if (!px) { fclose(f); return 0; }
    if (fread(px, 1, (size_t)n, f) != (size_t)n) { free(px); fclose(f); return 0; }
    fclose(f);
    *out = px;
    return 1;
}

/* how many pixels are "this colour, in any shade" — a shaded cube is
 * never exactly its base colour, so an exact match is the wrong test */
static int count_tint(const unsigned char *px, int w, int h,
                      int r, int g, int b) {
    int x, y, n = 0;
    for (y = 0; y < h; y++)
        for (x = 0; x < w; x++) {
            const unsigned char *p = px + ((size_t)y * (size_t)w + (size_t)x) * 3;
            /* the wanted channel dominates, and the others are quiet */
            int rr = p[0] - (r ? 0 : p[0]); (void)rr;
            if (r && p[0] > 40 && p[0] > p[1] + 30 && p[0] > p[2] + 30) n++;
            if (g && p[1] > 40 && p[1] > p[0] + 30 && p[1] > p[2] + 30) n++;
            if (b && p[2] > 40 && p[2] > p[0] + 30 && p[2] > p[1] + 30) n++;
        }
    return n;
}

int main(int argc, char **argv) {
    const char *ppm = argc > 1 ? argv[1] : "/tmp/fbwin-test.ppm";
    /* argv[2] is the ORACLE: a separate program that finds this
     * window BY NAME on the X server and reports what is in it. It is
     * passed in as a path because the test shells out to it, and the
     * path is a parameter so the harness can build it once for both
     * toolchains. A window id cannot be: Xlib ids are CLIENT-RELATIVE,
     * so the id a client gets is meaningless in another connection. */
    const char *oracle = argc > 2 ? argv[2] : NULL;
    unsigned char *px = 0;
    int w = 0, h = 0, n, i;

    /* FBWIN=0 must be honoured: no window, no X connection, no crash. */
    setenv("FBWIN", "0", 1);
    check(fbwin_open(64, 48) == 0, "FBWIN=0 opens no window");
    check(fbwin_open_p() == 0, "FBWIN=0 leaves none open");
    check(fbwin_present(0, 64, 48) == 0, "FBWIN=0 present is a no-op");
    fbwin_close();

    unsetenv("FBWIN");
    check(fbwin_open(64, 48) == 1, "a window opens when DISPLAY is set");
    check(fbwin_open_p() == 1, "fbwin_open_p agrees");
    check(fbwin_open(64, 48) == 1, "re-opening the same size is a no-op");

    /* fbdev's own primitives drive the same window */
    {
        /* At yaw 0 the shader's w = -rel.z, so the camera looks toward
         * NEGATIVE z. A first version of this test put the cubes at +z
         * from a camera at z=0 — behind the eye — and the near-plane
         * clip correctly drew nothing. Wrong test geometry reads as a
         * device bug; here the camera at z=+4 sees both cubes. */
        const char *payload =
            "0 0 0 1 1 1 1 0 0 0 0\n"        /* red   */
            "8 0 -8 1 1 1 0 1 0 0 0\n";      /* green */
        check(fb_open(64, 48) == 0, "fb_open takes the RAM surface");
        check(fb_window_open() == 1, "fb_window_open re-maps at 64x48");
        fb_clear(0xFF101010u);
        fb_set_camera(4, 0.9, 4, 0, 0);
        fb_blocks(payload);
        check(fb_present() == 0, "fb_present returns 0 (status)");
        check(fb_window_closed() == 0, "the window is not closed");
        check(fb_dump_ppm(ppm) == 0, "the RAM dump still works");
        if (ppm_load(ppm, &px, &w, &h)) {
            check(w == 64 && h == 48, "the dump is 64x48");
            n = count_tint(px, w, h, 1, 0, 0);
            check(n > 0, "the red cube reached the RAM surface");
            printf("   (%d red-tinted pixels)\n", n);
            n = count_tint(px, w, h, 0, 1, 0);
            check(n > 0, "the green cube reached the RAM surface");
            printf("   (%d green-tinted pixels)\n", n);
        } else {
            check(0, "the dump is readable");
        }
        free(px); px = 0;
    }

    /* THE READ-BACK, through the oracle.
     *
     * This is the check that cannot be satisfied by the device's own
     * bookkeeping: the oracle is a SEPARATE PROGRAM, linked against
     * the real X11 headers, which walks the server's window tree,
     * finds this window BY NAME, and reads its pixels back.
     *
     * It is a separate program for a reason worth recording, because
     * every in-process attempt to do this failed: a WINDOW ID IS
     * CLIENT-RELATIVE, so passing the id across a process boundary
     * addresses a window in the READER's id space, not the writer's,
     * and XGetWindowAttributes answers BadWindow. The tree walk is the
     * only way to name another client's window, and XQueryTree
     * translates the ids it returns into the caller's own space. */
    if (oracle) {
        char cmd[640];
        char out[1024];
        FILE *p;
        check(fbwin_open(64, 48) == 1, "the window is open at 64x48");
        check(fb_open(64, 48) == 0, "fb_open takes the RAM surface again");
        fb_clear(0xFF101010u);
        fb_set_camera(4, 0.9, 4, 0, 0);
        fb_blocks("0 0 0 1 1 1 1 0 0 0 0\n8 0 -8 1 1 1 0 1 0 0 0\n");
        /* SEVERAL PRESENTS, and the reason is worth writing down
         * because a single present read back 0 lit pixels from a window
         * that plainly had the frame on it.
         *
         * XMapWindow is asynchronous. The window is not viewable until
         * the server has processed it, and the Expose that follows is
         * delivered on the same queue. A blit issued in between goes to
         * a window the server has not finished making, and the Expose
         * then paints the window's background over the top of it — the
         * server is telling the client to redraw, and it means it.
         *
         * The device already handles the second half of this: an Expose
         * marks the window dirty and the NEXT present redraws. So a
         * game loop, which presents every frame, converges on the
         * second or third frame and never notices. A one-shot present
         * does not, which is why this presents a few times: it is
         * testing the window, not the loop, and the loop is the thing
         * that makes the window correct. */
        for (i = 0; i < 5; i++) { fb_present(); usleep(200000); }
        fbwin_sync();
        snprintf(cmd, sizeof cmd, "%s 2>&1", oracle);
        p = popen(cmd, "r");
        if (!p) {
            check(0, "the oracle could be run");
        } else {
            size_t n = fread(out, 1, sizeof out - 1, p);
            int rc = pclose(p);
            out[n] = 0;
            check(rc == 0, "the oracle found and read the window");
            if (rc == 0) {
                check(strstr(out, "64x48") != 0,
                      "the window on the server is 64x48");
                /* lit is the count of pixels that are not the dark clear
                 * colour: anything the maze drew. A window that never
                 * got the frame reads 0 lit and 3072 dark, which is
                 * what a window that "opened fine and showed nothing"
                 * looks like from outside. */
                {
                    long lit = -1, dark = -1;
                    const char *l = strstr(out, "lit=");
                    const char *d = strstr(out, " dark=");
                    if (l) lit = strtol(l + 4, 0, 10);
                    if (d) dark = strtol(d + 6, 0, 10);
                    printf("   oracle: %s", out);
                    check(lit > 0, "the frame IS on the server's window");
                    check(dark < lit, "most of the window is drawn, not cleared");
                }
            }
        }
    }

    /* a size change re-maps at the new size */
    check(fbwin_open(96, 64) == 1, "a different size re-maps");

    fbwin_close();
    check(fbwin_open_p() == 0, "close leaves nothing open");
    check(fbwin_open(16, 16) == 1, "re-open after close works");
    fbwin_close();

    printf("%s: %d failure(s)\n", fails ? "fbwin-test" : "fbwin-test", fails);
    return fails ? 1 : 0;
}
