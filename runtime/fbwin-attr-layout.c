/* fbwin-attr-layout.c — is the hand-declared XSetWindowAttributes in
 * fbwin.c the same struct X11 has?
 *
 * WHY THIS IS A SEPARATE PROGRAM AND NOT A COMPILE-TIME ASSERTION:
 * fbwin.c deliberately does NOT include <X11/Xlib.h>, because a
 * framebuffer box has no X11 headers and a device that cannot compile
 * without them is not a fallback. That is the right call, and it has a
 * price: the struct in fbwin.c is maintained by hand, and nothing in an
 * ordinary build can compare it to the real one.
 *
 * The price is worth naming precisely, because the failure is silent.
 * A struct with a field missing still compiles, still passes to
 * XCreateWindow, and still opens a window. The flag this device's whole
 * design rests on — override_redirect, field 14 — simply lands at the
 * wrong offset, and the window that opens is not the window that was
 * asked for. That is not a theory: the first version of the declaration
 * omitted `event_mask` and put override_redirect at byte 80 where X11
 * puts it at 88. Nothing complained. So this program includes the REAL
 * header and compares every field, and the window test runs it.
 *
 * It is also the test that would have caught the dirty-flag gate and the
 * swapped byte order, had they been written as assertions: both are
 * checked in fbwin-test.c, which is where a behavioural check belongs.
 *
 * Build:  cc -Wall -O1 fbwin-attr-layout.c -o fbwin-attr-layout -lX11
 * Run:    ./fbwin-attr-layout        (exit 0 = the struct matches)
 */
#include <X11/Xlib.h>

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

typedef uint64_t XID;

/* …copied VERBATIM from fbwin.c. Keep the two in step; this file is what
 * proves they are in step with X11. */
struct XSetWindowAttributesPublic {
    XID           background_pixmap;
    unsigned long background_pixel;
    XID           border_pixmap;
    unsigned long border_pixel;
    int           bit_gravity;
    int           win_gravity;
    int           backing_store;
    unsigned long backing_planes;
    unsigned long backing_pixel;
    int           save_under;
    long          event_mask;
    long          do_not_propagate_mask;
    int           override_redirect;
    XID           colormap;
    XID           cursor;
};

static int bad;

static void chk(const char *field, size_t real, size_t ours) {
    printf("  %-24s x11=%3zu fbwin=%3zu  %s\n", field, real, ours,
           real == ours ? "ok" : "MISMATCH");
    if (real != ours) bad = 1;
}

int main(void) {
    printf("fbwin's hand-declared XSetWindowAttributes vs <X11/Xlib.h>\n");
    chk("sizeof", sizeof(XSetWindowAttributes),
        sizeof(struct XSetWindowAttributesPublic));
    chk("background_pixmap", offsetof(XSetWindowAttributes, background_pixmap),
        offsetof(struct XSetWindowAttributesPublic, background_pixmap));
    chk("background_pixel", offsetof(XSetWindowAttributes, background_pixel),
        offsetof(struct XSetWindowAttributesPublic, background_pixel));
    chk("border_pixmap", offsetof(XSetWindowAttributes, border_pixmap),
        offsetof(struct XSetWindowAttributesPublic, border_pixmap));
    chk("border_pixel", offsetof(XSetWindowAttributes, border_pixel),
        offsetof(struct XSetWindowAttributesPublic, border_pixel));
    chk("bit_gravity", offsetof(XSetWindowAttributes, bit_gravity),
        offsetof(struct XSetWindowAttributesPublic, bit_gravity));
    chk("win_gravity", offsetof(XSetWindowAttributes, win_gravity),
        offsetof(struct XSetWindowAttributesPublic, win_gravity));
    chk("backing_store", offsetof(XSetWindowAttributes, backing_store),
        offsetof(struct XSetWindowAttributesPublic, backing_store));
    chk("backing_planes", offsetof(XSetWindowAttributes, backing_planes),
        offsetof(struct XSetWindowAttributesPublic, backing_planes));
    chk("backing_pixel", offsetof(XSetWindowAttributes, backing_pixel),
        offsetof(struct XSetWindowAttributesPublic, backing_pixel));
    chk("save_under", offsetof(XSetWindowAttributes, save_under),
        offsetof(struct XSetWindowAttributesPublic, save_under));
    chk("event_mask", offsetof(XSetWindowAttributes, event_mask),
        offsetof(struct XSetWindowAttributesPublic, event_mask));
    chk("do_not_propagate_mask",
        offsetof(XSetWindowAttributes, do_not_propagate_mask),
        offsetof(struct XSetWindowAttributesPublic, do_not_propagate_mask));
    chk("override_redirect", offsetof(XSetWindowAttributes, override_redirect),
        offsetof(struct XSetWindowAttributesPublic, override_redirect));
    chk("colormap", offsetof(XSetWindowAttributes, colormap),
        offsetof(struct XSetWindowAttributesPublic, colormap));
    chk("cursor", offsetof(XSetWindowAttributes, cursor),
        offsetof(struct XSetWindowAttributesPublic, cursor));

    printf("\n%s\n",
           bad ? "MISMATCH — fbwin.c's struct is not X11's struct"
               : "struct matches X11");
    return bad;
}
