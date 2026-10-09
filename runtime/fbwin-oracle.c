/* fbwin-oracle.c — read a game's window back through the X server, from
 * a SEPARATE PROCESS, and print what is in it.
 *
 * WHY THIS IS A SEPARATE PROGRAM AND NOT A CALL INSIDE THE TEST
 *   Two reasons, the first of which cost a long debugging session:
 *
 *   1. A WINDOW ID IS CLIENT-RELATIVE. Xlib hands each client a private
 *      id space; the server translates on every request. So a second
 *      process cannot ask for window 0xa00001 created by the first —
 *      from its own connection that id means one of ITS windows (or
 *      none), and XGetWindowAttributes answers BadWindow. Every
 *      "independent read-back" attempt that passed the id across
 *      process boundaries therefore failed for a reason that has
 *      nothing to do with the device: xwd, XGetImage and a hand-rolled
 *      capture all got BadWindow/BadMatch on a window that was on
 *      screen and correctly drawn.
 *
 *   2. The ids XQueryTree RETURNS are translated into the CALLER's id
 *      space, so a reader that walks the tree from the root and matches
 *      the window by NAME gets a handle that is valid in its own
 *      connection. That is the only way to address another client's
 *      window, and it is what this does.
 *
 * So: find the window by title, read it with XGetImage, and report the
 * pixels. The device gets no say in how this is judged, which is the
 * entire point of an oracle — the device's own fb_dump_ppm is a
 * different code path to the same buffer, and agreeing with itself
 * proves nothing about the blit.
 *
 *   usage: fbwin-oracle <window-name>            (default: MIMEcroft (fb0))
 *          fbwin-oracle -name X  and prints a summary line:
 *            size WxH avg r=.. g=.. b=.. lit=<n> <dominant colour name>
 *   exit 0 = found and read, 2 = no such window, 3 = XGetImage refused
 */
#include <X11/Xlib.h>
#include <X11/Xatom.h>
#include <X11/Xutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static Display *D;
static Window   root;

/* Depth-first, breadth-first doesn't matter: a window is at most a few
 * levels down and the tree is small. Recursion is bounded by the tree
 * depth, and an X server will not hand a client a cyclic tree. */
static Window find_by_name(Window w, const char *want, int depth) {
    char *nm = 0;
    Window r, parent, *kids = 0;
    unsigned int nkids = 0, i;
    Window found = 0;
    if (depth > 8) return 0;
    if (XFetchName(D, w, &nm) && nm) {
        int hit = (strstr(nm, want) != 0);
        XFree(nm);
        if (hit) return w;
    }
    if (!XQueryTree(D, w, &r, &parent, &kids, &nkids)) return 0;
    for (i = 0; i < nkids && !found; i++) found = find_by_name(kids[i], want, depth + 1);
    if (kids) XFree(kids);
    return found;
}

int main(int argc, char **argv) {
    const char *want = (argc > 1 && argv[1][0] != '-') ? argv[1] : "MIMEcroft (fb0)";
    Window w;
    XWindowAttributes wa;
    XImage *im;
    long nr = 0, ng = 0, nb = 0, n = 0, lit = 0, dark = 0;
    double mr = 0, mg = 0, mb = 0;

    D = XOpenDisplay(NULL);
    if (!D) { fprintf(stderr, "fbwin-oracle: no DISPLAY\n"); return 4; }
    root = RootWindow(D, DefaultScreen(D));

    w = find_by_name(root, want, 0);
    if (!w) {
        fprintf(stderr, "fbwin-oracle: no window matching \"%s\"\n", want);
        XCloseDisplay(D);
        return 2;
    }
    if (!XGetWindowAttributes(D, w, &wa)) {
        fprintf(stderr, "fbwin-oracle: XGetWindowAttributes failed on 0x%lx\n",
                (unsigned long)w);
        XCloseDisplay(D);
        return 2;
    }
    /* XGetImage STRAIGHT OFF THE WINDOW FAILS WITH BadMatch on some
     * displays — this one does, for a window this oracle did not
     * create, and for one it did (a plain Xlib client hits it too). It
     * is a property of the SCREEN's visual, not of the window and not
     * of the device, and it is worth recording because the obvious
     * oracle is exactly this call and it reports a healthy window as
     * unreadable.
     *
     * The way round is a PIXMAP: a server-side copy of the window's
     * contents drawn with XCopyArea, which every display serves, and
     * which XGetImage then reads back happily. Same pixels, one extra
     * server-side copy, and no dependence on the window's visual being
     * an image-readable one. */
    {
        Pixmap pm = XCreatePixmap(D, w, (unsigned)wa.width, (unsigned)wa.height,
                                  (unsigned)DefaultDepth(D, DefaultScreen(D)));
        GC g = XCreateGC(D, pm, 0, NULL);
        XCopyArea(D, w, pm, g, 0, 0, (unsigned)wa.width, (unsigned)wa.height, 0, 0);
        XSync(D, False);
        im = XGetImage(D, pm, 0, 0, (unsigned)wa.width, (unsigned)wa.height,
                       AllPlanes, ZPixmap);
        XFreeGC(D, g);
        if (im) XFreePixmap(D, pm);
    }
    if (!im) {
        fprintf(stderr, "fbwin-oracle: could not read 0x%lx (%dx%d)\n",
                (unsigned long)w, wa.width, wa.height);
        XCloseDisplay(D);
        return 3;
    }
    /* XGetPixel returns the pixel in the DRAWABLE's visual, and the
     * red/green/blue SHIFTS come from that visual, not from a 24-bit
     * assumption: a 16-bit or BGR display is common enough that
     * hard-coding shifts turns a correct frame into a wrong report. */
    {
        int nvis = 0;
        XVisualInfo q;
        XVisualInfo *vi;
        memset(&q, 0, sizeof q);
        q.visualid = XVisualIDFromVisual(wa.visual);
        q.screen = DefaultScreen(D);
        vi = XGetVisualInfo(D, VisualIDMask | VisualScreenMask, &q, &nvis);
        int rs = 16, gs = 8, bs = 0;
        if (vi) {
            rs = 0; gs = 0; bs = 0;
            { unsigned long m;
              m = vi->red_mask;   while (!(m & 1UL)) { m >>= 1; rs++; }
              m = vi->green_mask; while (!(m & 1UL)) { m >>= 1; gs++; }
              m = vi->blue_mask;  while (!(m & 1UL)) { m >>= 1; bs++; } }
            XFree(vi);
        }
        for (int y = 0; y < wa.height; y++) {
            for (int x = 0; x < wa.width; x++) {
                unsigned long p = XGetPixel(im, x, y);
                int r = (int)((p >> rs) & 0xFF), g = (int)((p >> gs) & 0xFF), b = (int)((p >> bs) & 0xFF);
                nr += r; ng += g; nb += b; n++;
                /* "lit" = a pixel that is not the dark clear colour.
                 * The game clears to 0xFF000000, so this counts
                 * anything the maze drew. */
                if (r + g + b > 90) lit++;
                if (r + g + b < 30) dark++;
            }
        }
    }
    mr = n ? (double)nr / (double)n : 0;
    mg = n ? (double)ng / (double)n : 0;
    mb = n ? (double)nb / (double)n : 0;
    printf("window 0x%lx %dx%d  avg r=%.1f g=%.1f b=%.1f  lit=%ld dark=%ld of %ld\n",
           (unsigned long)w, wa.width, wa.height, mr, mg, mb, lit, dark, n);
    XDestroyImage(im);
    XCloseDisplay(D);
    return 0;
}
