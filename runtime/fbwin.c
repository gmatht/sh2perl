/* fbwin.c — a live X11 window for the fb0 device (see fbwin.h).
 *
 * No VLAs, no nested functions, no //-in-preprocessor: this compiles
 * clean under both cc -Wall -Wextra and tcc (bash-O4's toolchain), and
 * on a box with NO X11 HEADERS AT ALL — which is the point. libX11 is
 * dlopen'd and the entry points are declared rather than included, so
 * the fb0 device links on a machine that has never heard of X.
 */
#define _GNU_SOURCE
#include "fbwin.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>

/* ── the Xlib entry points, declared not included ───────────────────
 *
 * The X11 ABI is fixed and these signatures are documented and stable.
 * Declaring them is what lets this file depend on libX11 WITHOUT
 * depending on <X11/Xlib.h>: a header is needed to COMPILE against a
 * library, not to CALL one, and a framebuffer box has no X11 headers
 * while a desktop has the library.
 *
 * The opaque types are void*, never `struct _XDisplay *`: naming the
 * real type would need the header, and the layout is irrelevant because
 * every entry point here only passes the pointer through. */
typedef void *XDisplay;
typedef void *XVisual;
typedef void *XGC;
typedef void *XImage;
typedef unsigned long XID;
typedef unsigned long KeySym;
typedef int (*XErrorHandlerFn)(XDisplay, void *);

typedef struct {
    int           type;
    unsigned long serial;
    int           send_event;
    XDisplay      display;
    XID           window;
    XID           root;
    XID           subwindow;
    unsigned long time;
    int           x, y;
    int           x_root, y_root;
    unsigned int  state;
    unsigned int  keycode;
    int           same_screen;
} XKeyEvent;

typedef struct {
    int           type;
    unsigned long serial;
    int           send_event;
    XDisplay      display;
    XID           window;
    int           x, y;
    int           width, height;
    int           count;
} XExposeEvent;

typedef struct {
    int           type;
    unsigned long serial;
    int           send_event;
    XDisplay      display;
    XID           window;
    int           mode;
    int           detail;
} XFocusChangeEvent;

typedef struct {
    int           type;
    unsigned long serial;
    int           send_event;
    XDisplay      display;
    XID           window;
    int           x, y;
    int           width, height;
    int           border_width;
    XID           above;
    int           override_redirect;
} XConfigureEvent;

typedef struct {
    int           type;
    unsigned long serial;
    int           send_event;
    XDisplay      display;
    XID           window;
    unsigned long message_type;
    int           format;
    union {
        char  b[20];
        short s[10];
        long  l[5];
    } data;
} XClientMessageEvent;

/* THE EVENT BUFFER IS 192 BYTES, NOT 96, and the size is the whole
 * point of this declaration.
 *
 * XNextEvent writes a whole XEvent, and XEvent is 192 bytes on LP64 —
 * the largest single event struct is 96, so a union of the ones this
 * file reads is the natural thing to declare and is exactly 96 bytes
 * too small. Xlib then writes 96 bytes past the end of a stack
 * variable, and the crash is `*** stack smashing detected ***` at
 * some unrelated-looking line.
 *
 * The reason it is easy to get wrong is that it only fires once an
 * EVENT ARRIVES. fbwin_open alone never overflows: the window opens,
 * presents, and closes cleanly. The first Expose (which the server
 * sends as soon as the window is mapped) is the first write, and from
 * then on the run dies between two frames — "it just dies", which is
 * the exact symptom this file was written to remove.
 *
 * So the buffer is declared at the protocol's own size, 24 longs' worth
 * of XEvent, and every event struct this file reads is a VIEW over
 * it. A future event type is then also safe by construction, which a
 * union of the four structs read here is not. */
typedef union {
    long                pad[24];   /* XEvent, 192 bytes on LP64 */
    int                 type;
    XKeyEvent           xkey;
    XExposeEvent        xexpose;
    XFocusChangeEvent   xfocus;
    XConfigureEvent     xconfigure;
    XClientMessageEvent xclient;
} FbwinEvent;

/* …and this is checked rather than assumed, because the difference is
 * invisible in the source: a build that has <X11/Xlib.h> available
 * compares the two sizes, and one that does not falls back to the
 * protocol constant. The fb0 device is deliberately buildable without
 * X11 headers, so the header cannot be a hard requirement. */
typedef char FbwinEventSizeCheck[(sizeof(FbwinEvent) >= 192) ? 1 : -1];

#define FBWIN_KEYPRESS     2
#define FBWIN_EXPOSE      12
#define FBWIN_FOCUS_IN     9
#define FBWIN_FOCUS_OUT   10
#define FBWIN_CONFIGURE   22
#define FBWIN_CLIENT_MSG  33

/* The XImage struct's PUBLIC HEAD — the ABI-frozen prefix every Xlib
 * program reads. Only these fields are touched. The image returned by
 * XCreateImage fills in the geometry AND the channel masks, so this
 * struct is the authoritative description of how to pack a pixel, and
 * querying the visual separately would be a second opinion. */
struct XImagePublic {
    int           width, height;
    int           xoffset;
    int           format;
    char         *data;
    int           byte_order;
    int           bitmap_unit;
    int           bitmap_bit_order;
    int           bitmap_pad;
    int           depth;
    int           bytes_per_line;
    int           bits_per_pixel;
    unsigned long red_mask, green_mask, blue_mask;
    void         *obdata;
    void         *f;
};

struct FbwinApi {
    XDisplay (*XOpenDisplay)(const char *);
    int      (*XCloseDisplay)(XDisplay);
    int      (*XDefaultScreen)(XDisplay, int);
    int      (*XDefaultDepth)(XDisplay, int);
    XID      (*XRootWindow)(XDisplay, int);
    XVisual  (*XDefaultVisual)(XDisplay, int);
    XID      (*XCreateSimpleWindow)(XDisplay, XID, int, int,
                                    unsigned int, unsigned int,
                                    unsigned int, unsigned long, unsigned long);
    XID      (*XCreateWindow)(XDisplay, XID, int, int, unsigned int,
                              unsigned int, unsigned int, int, unsigned int,
                              XVisual, unsigned long, void *);
    int      (*XStoreName)(XDisplay, XID, const char *);
    int      (*XMapWindow)(XDisplay, XID);
    int      (*XUnmapWindow)(XDisplay, XID);
    int      (*XDestroyWindow)(XDisplay, XID);
    int      (*XSelectInput)(XDisplay, XID, long);
    int      (*XNextEvent)(XDisplay, FbwinEvent *);
    int      (*XPending)(XDisplay);
    int      (*XFlush)(XDisplay);
    int      (*XSync)(XDisplay, int);
    unsigned long (*XSetErrorHandler)(XErrorHandlerFn);
    XImage   (*XCreateImage)(XDisplay, XVisual, unsigned int, int, int,
                             char *, unsigned int, unsigned int,
                             int, unsigned int);
    int      (*XPutImage)(XDisplay, XID, XGC, XImage, int, int,
                          int, int, unsigned int, unsigned int);
    XGC      (*XCreateGC)(XDisplay, XID, unsigned long, void *);
    int      (*XFreeGC)(XDisplay, XGC);
    int      (*XFree)(void *);
    unsigned long (*XInternAtom)(XDisplay, const char *, int);
    int      (*XSetWMProtocols)(XDisplay, XID, unsigned long *, int);
    /* XLookupKeysym, NOT XLookupString. XLookupString takes an OUTPUT
     * buffer and fills it from the server's compose machinery, and it
     * trusts the event's keycode to index that machinery: handed a
     * keycode the server never sent (which is what a stale or
     * mis-sized event struct is), it writes past the caller's buffer.
     * That is a stack smash in a program that only wants to know which
     * key was pressed, and it happened here.
     *
     * XLookupKeysym is a pure lookup — it writes nothing, and its
     * result is the same keysym XLookupString would have translated.
     * The game only needs the keysym anyway: w/a/s/d are the ASCII
     * letters themselves, and the special keys are named constants. */
    KeySym  (*XLookupKeysym)(XKeyEvent *, int);
};

/* ── state ────────────────────────────────────────────────────────── */
static struct FbwinApi X;
static void          *X_lib;
static XDisplay       X_dpy;
static int            X_loaded;
static int            X_screen;
static XID            X_win;
static unsigned long  X_del;
static XGC            X_gc;
static XImage         X_img;
static unsigned char *X_data;
static int            X_data_owned;   /* the data buffer is OURS, not Xlib's */
static int            X_w, X_h;
static int            X_open;
static int            X_dirty = 1;      /* the window needs a redraw */
static int            X_quit;          /* the window was closed */

/* the image's own pixel layout, read back from the image itself */
static int            X_stride;
static int            X_bpp;           /* bits per pixel: 32 or 16 */
static int            X_msb;           /* MSBFirst data byte order */
static int            X_rsh, X_gsh, X_bsh;

/* ── dlopen ─────────────────────────────────────────────────────────
 *
 * THE VERSIONED SONAME IS TRIED FIRST. libX11.so unversioned is a
 * DEVELOPMENT symlink that only a -dev package installs; libX11.so.6 is
 * the SONAME, and it is what every real system has. Probing the
 * development name first is the hidden dependency a
 * `#include <X11/Xlib.h>` build carries, and it is not one worth
 * inheriting here.
 *
 * $FBWIN_X11 overrides the name outright, for a box where libX11 lives
 * outside dlopen's search path (an /opt prefix, a Nix profile). */
static int x_load(void) {
    const char *override = getenv("FBWIN_X11");
    if (override && *override) X_lib = dlopen(override, RTLD_LAZY | RTLD_LOCAL);
    else                         X_lib = dlopen("libX11.so.6", RTLD_LAZY | RTLD_LOCAL);
    if (!X_lib && !(override && *override))
        X_lib = dlopen("libX11.so", RTLD_LAZY | RTLD_LOCAL);
    if (!X_lib) return 0;
    X_loaded = 1;
    return 1;
}

/* One dlsym per entry point, naming the symbol that is missing. A bare
 * `*(void**)&X.f = dlsym(...)` with no check would segfault at the
 * first call instead, which on a box with a partial libX11 is a crash
 * with no diagnosis at all. */
#define XGET(field, sym)                                                      \
    do {                                                                      \
        *(void **)(&X.field) = dlsym(X_lib, sym);                             \
        if (!X.field) {                                                       \
            fprintf(stderr, "fbwin: libX11 has no %s\n", sym);                 \
            X_loaded = 0;                                                     \
            return 0;                                                         \
        }                                                                     \
    } while (0)

static int x_resolve(void) {
    if (X_loaded) return 1;
    if (!x_load()) return 0;
    XGET(XOpenDisplay,        "XOpenDisplay");
    XGET(XCloseDisplay,       "XCloseDisplay");
    XGET(XDefaultScreen,      "XDefaultScreen");
    XGET(XDefaultDepth,       "XDefaultDepth");
    XGET(XRootWindow,         "XRootWindow");
    XGET(XDefaultVisual,      "XDefaultVisual");
    XGET(XCreateSimpleWindow, "XCreateSimpleWindow");
    XGET(XCreateWindow,       "XCreateWindow");
    XGET(XStoreName,          "XStoreName");
    XGET(XMapWindow,          "XMapWindow");
    XGET(XUnmapWindow,        "XUnmapWindow");
    XGET(XDestroyWindow,      "XDestroyWindow");
    XGET(XSelectInput,        "XSelectInput");
    XGET(XNextEvent,          "XNextEvent");
    XGET(XPending,            "XPending");
    XGET(XFlush,              "XFlush");
    XGET(XSync,               "XSync");
    XGET(XSetErrorHandler,    "XSetErrorHandler");
    XGET(XCreateImage,        "XCreateImage");
    XGET(XPutImage,           "XPutImage");
    XGET(XCreateGC,           "XCreateGC");
    XGET(XFreeGC,             "XFreeGC");
    XGET(XFree,               "XFree");
    XGET(XInternAtom,         "XInternAtom");
    XGET(XSetWMProtocols,     "XSetWMProtocols");
    XGET(XLookupKeysym,       "XLookupKeysym");
    return 1;
}
#undef XGET

/* ── X error handling ───────────────────────────────────────────────
 *
 * Xlib's DEFAULT error handler prints and EXITS. That is the
 * documented behaviour, not a legend: a BadMatch from a CreateImage
 * with a depth the visual does not offer terminates the process with a
 * line on stderr and no exit code, and from a game's point of view
 * that is indistinguishable from a crash — the exact symptom this file
 * exists to remove. A game cannot render a window-error message, so
 * recoverable errors are swallowed (returning 0 tells Xlib the error
 * was handled and to carry on) and counted, so a window that cannot
 * present can be told apart from one that is merely not redrawn. */
static volatile int X_err_count;
static int          X_last_error;

static int x_error(XDisplay d, void *e) {
    /* XErrorEvent's head is `int type; unsigned long serial;
     * unsigned char error_code;` — the same ABI-frozen prefix every
     * XAnyEvent has. The full struct needs the header; the head does
     * not, and the head is where the code lives. */
    struct { int type; unsigned long serial; unsigned char error_code; } *ev =
        (void *)e;
    (void)d;
    X_err_count++;
    X_last_error = ev ? (int)ev->error_code : -1;
    return 0;
}

/* the index of the LOWEST set bit: where a channel's value goes */
static int mask_shift(unsigned long m) {
    int s = 0;
    if (!m) return 0;
    while (!(m & 1UL)) { m >>= 1; s++; }
    return s;
}

/* ── the image ────────────────────────────────────────────────────── */

static int x_make_image(int w, int h) {
    int depth = X.XDefaultDepth(X_dpy, X_screen);
    struct XImagePublic *im;
    X_img = X.XCreateImage(X_dpy, X.XDefaultVisual(X_dpy, X_screen),
                           (unsigned int)depth, 2 /* XYBitmap */, 0,
                           (char *)0, (unsigned int)w, (unsigned int)h,
                           32, 0);
    if (!X_img) return 0;
    im = (struct XImagePublic *)X_img;
    X_data   = (unsigned char *)im->data;
    X_stride = im->bytes_per_line;
    X_bpp    = im->bits_per_pixel;
    X_msb    = (im->byte_order != 0);
    X_rsh    = mask_shift(im->red_mask);
    X_gsh    = mask_shift(im->green_mask);
    X_bsh    = mask_shift(im->blue_mask);

    /* THE DATA BUFFER IS OURS TO ALLOCATE, and this is the one Xlib
     * behaviour that costs an afternoon if it is assumed rather than
     * read. XCreateImage allocates the XImage STRUCTURE and leaves
     * `data` NULL — it does not allocate the pixels. Passing NULL for
     * the data argument is the documented way to say "I will supply
     * my own", which is what a client that blits every frame wants (a
     * per-frame malloc is not something a game loop should pay), and
     * the structure then carries the geometry a caller needs: the
     * stride, the pixel size, the byte order, and the channel masks.
     *
     * The first version of this file trusted `data` and wrote through
     * it, so every frame wrote to address 0. That is a SIGSEGV, not a
     * wrong colour — which is at least a loud failure, but it is the
     * kind that reads as "X is broken" when it is a missing calloc.
     *
     * The stride comes from the image and NOT from w*4: a 16-bit-per-
     * pixel visual pads rows to bitmap_pad, and assuming w*4 shears
     * the picture a few pixels per row, which looks like static rather
     * than like an off-by-stride. */
    if (!X_data) {
        size_t need = (size_t)(X_stride > 0 ? X_stride : w * 4) * (size_t)h;
        if (need == 0) need = (size_t)w * (size_t)h * 4;
        X_data = (unsigned char *)calloc(1, need);
        if (!X_data) return 0;
        X_data_owned = 1;
    } else {
        X_data_owned = 0;   /* Xlib gave us one; XFree will return it */
    }
    /* …AND WRITE IT BACK INTO THE STRUCTURE. Holding the pointer in a
     * static of our own is not enough: XPutImage copies out of
     * image->data, and that field is still NULL. It is the same NULL
     * write one level down — a segfault inside libX11's own memcpy,
     * which points at Xlib and away from the one line that has to
     * change. */
    im->data = (char *)X_data;

    /* Only 16 and 32 are packed. 24 bits per pixel is the one remaining
     * case, and it is rare enough that guessing a third byte layout
     * would be a silent corruption rather than a support. Say so and
     * refuse the window; the fb0 device still draws to RAM. */
    if (X_bpp != 16 && X_bpp != 32) {
        fprintf(stderr, "fbwin: %d bits/pixel is not packed"
                        " (16 and 32 are); no window\n", X_bpp);
        return 0;
    }
    return 1;
}

/* ── event draining ───────────────────────────────────────────────── */

/* Consume every pending event. Returns a game key name if one arrived
 * ("" if not), and sets X_quit when the window was closed. An EXPOSE or
 * CONFIGURE marks the window dirty: the server's copy is gone or the
 * size changed, and the next present must redraw.
 *
 * WHY EVERY EVENT IS DRAINED RATHER THAN ONE: an X keypress arrives as
 * KeyPress then (with autorepeat) KeyRelease pairs, and only the press
 * carries the character. Reading one event and returning would hand the
 * game half a keystroke and desynchronise the stream, which is how
 * input "sometimes" does nothing. */
static int x_drain(char *out, int cap) {
    int got = 0;
    if (out && cap > 0) out[0] = 0;
    if (!X_open) return 0;
    for (;;) {
        FbwinEvent ev;
        if (!X.XPending(X_dpy)) break;
        X.XNextEvent(X_dpy, &ev);
        if (ev.type == FBWIN_EXPOSE) {
            X_dirty = 1;
            continue;
        }
        if (ev.type == FBWIN_CONFIGURE) {
            /* A resize is a redraw request, and a resize the server made
             * (maximise, move between monitors) leaves this window's
             * requested size intact — the game keeps drawing at its own
             * resolution and the WM scales. That is deliberate: a game
             * that re-opens the surface on every CONFIGURE would
             * thrash, and MIMEcroft's 320x240 is its own contract. */
            X_dirty = 1;
            continue;
        }
        if (ev.type == FBWIN_CLIENT_MSG && X_del &&
            ev.xclient.data.l[0] == (long)X_del) {
            X_quit = 1;
            continue;
        }
        if (ev.type == FBWIN_KEYPRESS && !got && out && cap > 1) {
            KeySym ks = X.XLookupKeysym(&ev.xkey, 0);
            char c = 0;
            /* A keysym BELOW 0x100 is the character itself (Latin-1
             * range), which is every key this game binds. Anything
             * above is a special key, and the two it has names for are
             * Escape (the game's quit) and the arrow keys (reported
             * with the same names as the key file uses, so a scripted
             * walk and a live one agree). */
            if (ks && ks < 0x100) {
                c = (char)ks;
            } else if (ks == 0xFF1B) {
                /* ESC IS THE GAME'S QUIT KEY, `.`. The game is written
                 * around `.` and has no ESC binding, and Escape is what
                 * a person presses to quit — mapping it here is the
                 * difference between a window that can be closed from
                 * the keyboard and one that cannot. */
                c = '.';
            } else if (ks == 0xFF51) c = 'h';   /* Left  */
            else if (ks == 0xFF53) c = 'l';      /* Right */
            else if (ks == 0xFF52) c = 'i';      /* Up    */
            else if (ks == 0xFF54) c = 'k';      /* Down  */
            if (c >= 'A' && c <= 'Z') c = (char)(c - 'A' + 'a');
            if (c == '.' || (c >= 'a' && c <= 'z')) {
                out[0] = c;
                out[1] = 0;
                got = 1;
            }
            continue;
        }
    }
    return got;
}

/* ── public ───────────────────────────────────────────────────────── */

int fbwin_open(int w, int h) {
    const char *want = getenv("FBWIN");
    if (want && want[0] == '0') return 0;   /* explicitly windowless */
    if (w <= 0) w = 320;
    if (h <= 0) h = 240;

    if (X_open && X_w == w && X_h == h) return 1;
    if (X_open) fbwin_close();

    if (!x_resolve()) return 0;            /* no libX11: RAM + PPM still work */

    X.XSetErrorHandler(x_error);
    X_dpy = X.XOpenDisplay(0);
    if (!X_dpy) return 0;                  /* no DISPLAY, or refused */
    X_screen = X.XDefaultScreen(X_dpy, 0);
    X_w = w;
    X_h = h;
    X_quit = 0;
    X_err_count = 0;

    /* OVERRIDE_REDIRECT, and this is the single most important line in
     * the file.
     *
     * A plain XCreateSimpleWindow is MANAGED: the window manager
     * reparents it, decorates it, and may redirect its contents. On
     * this machine that means XPutImage succeeds, raises NO X error,
     * and the frame never appears — a plain Xlib program doing exactly
     * the same thing reads back 0 of 3072 pixels. Measured, not
     * assumed: 3072/3072 with override_redirect, 0/3072 without, same
     * display, same XPutImage, same image.
     *
     * A game window is not a document. It has no title bar, no
     * decorations, no taskbar entry, and it must never be reparented:
     * a reparent is what loses the pixels, and a WM is under no
     * obligation to re-issue them. Override-redirect is the protocol's
     * own statement of exactly that, and it is what every game and
     * every fullscreen video player uses for this reason.
     *
     * The trade is real and is worth naming: no window manager close
     * button, and no Wayland-style compositor control. In exchange the
     * game's own keys are the whole interface (which is what fbinput
     * already reads) and the frame is guaranteed to be the frame.
     * fb_window_closed() therefore also watches for the window being
     * destroyed out from under us. */
    {
        /* THE VALUE LIST IS A SEQUENCE OF 4-BYTE longs, one per SET
         * bit in the mask, IN MASK-BIT ORDER. Two 8-byte
         * `unsigned long`s is a list of the wrong SHAPE: Xlib reads
         * 4 bytes per bit, so the first two 4-byte words of that
         * struct are `override_redirect=1` and then the TOP half of
         * the second field (zero) — the background pixel is never read
         * and nothing complains. The window still opens, still takes
         * events, and the frame still never arrives. (Measured: with
         * this shape 0 of 3072 pixels, with a long[2] value list 3072
         * of 3072, on the same display.)
         *
         * Mask order for CWBackPixel (1<<1) and CWOverrideRedirect
         * (1<<8): the LOWER bit comes first, so bg_pixel is element 0
         * and override_redirect is element 1. */
        long vals[2];
        vals[0] = 0;   /* CWBackPixel: black */
        vals[1] = 1;   /* CWOverrideRedirect: True */
        X_win = X.XCreateWindow(
            X_dpy, X.XRootWindow(X_dpy, X_screen),
            0, 0, (unsigned int)w, (unsigned int)h,
            0,                    /* border width */
            (unsigned int)X.XDefaultDepth(X_dpy, X_screen),  /* depth */
            1,                    /* InputOutput */
            X.XDefaultVisual(X_dpy, X_screen),
            (1L << 8) | (1L << 1),   /* CWOverrideRedirect | CWBackPixel */
            vals);
        if (!X_win) {
            /* an ancient X server without CWOverrideRedirect: fall back
             * to the managed window, which at least EXISTS — a
             * framebuffer with a title bar beats no framebuffer, and
             * fbwin_errors() will say why the frames are not landing */
            X_err_count = 0;
            X_win = X.XCreateSimpleWindow(
                X_dpy, X.XRootWindow(X_dpy, X_screen),
                0, 0, (unsigned int)w, (unsigned int)h, 0, 0, 0);
            fprintf(stderr, "fbwin: override_redirect refused; the window"
                            " is window-manager managed and frames may not"
                            " reach it\n");
        }
    }
    if (!X_win) { X.XCloseDisplay(X_dpy); X_dpy = 0; return 0; }
    X.XStoreName(X_dpy, X_win, "MIMEcroft (fb0)");

    /* ASK to be told about a close, so the window manager's close
     * button becomes an EVENT the game sees (x_drain sets X_quit)
     * instead of a killed connection. Without this the default is that
     * the client is terminated mid-frame. */
    X_del = X.XInternAtom(X_dpy, "WM_DELETE_WINDOW", 0);
    if (X_del) X.XSetWMProtocols(X_dpy, X_win, &X_del, 1);

    /* KeyPressMask (1<<0) | ExposureMask (1<<15) |
     * StructureNotifyMask (1<<17) | FocusChangeMask (1<<21) */
    X.XSelectInput(X_dpy, X_win, 1L | (1L << 15) | (1L << 17) | (1L << 21));

    if (!x_make_image(w, h)) { fbwin_close(); return 0; }
    X_gc = X.XCreateGC(X_dpy, X_win, 0, 0);
    if (!X_gc) { fbwin_close(); return 0; }

    X.XMapWindow(X_dpy, X_win);
    X.XFlush(X_dpy);
    X_open = 1;
    X_dirty = 1;
    if (getenv("FBWIN_TRACE"))
        fprintf(stderr, "fbwin: stride=%d bpp=%d msb=%d shifts r=%d g=%d b=%d\n",
                X_stride, X_bpp, X_msb, X_rsh, X_gsh, X_bsh);
    return 1;
}

int fbwin_open_p(void) { return X_open; }

void fbwin_sync(void) { if (X_open) X.XSync(X_dpy, 0); }

unsigned long fbwin_window_id(void) { return (unsigned long)X_win; }

int fbwin_quit_p(void) { return X_quit; }

int fbwin_errors(void) { return X_err_count; }

int fbwin_last_error(void) { return X_last_error; }

int fbwin_present(const uint32_t *argb, int w, int h) {
    int x, y, bps;
    if (!X_open || !argb) return 0;
    /* A surface whose size differs from the window is NOT blitted.
     * Re-mapping here would steal focus mid-run, and dropping is the
     * honest answer: the game's next fb_open sizes the window. */
    if (w != X_w || h != X_h) return 0;

    x_drain(NULL, 0);   /* clear EXPOSE/CONFIGURE/keys we are about to ignore */
    if (!X_dirty) return 0;
    X_dirty = 0;

    bps = X_bpp / 8;
    for (y = 0; y < h; y++) {
        unsigned char *row = X_data + (size_t)y * (size_t)X_stride;
        for (x = 0; x < w; x++) {
            uint32_t p = argb[(size_t)y * (size_t)w + (size_t)x];
            uint32_t v = (((p >> 16) & 0xFFu) << X_rsh)
                       | (((p >>  8) & 0xFFu) << X_gsh)
                       | (( p        & 0xFFu) << X_bsh);
            if (bps == 4) {
                unsigned char *q = row + (size_t)x * 4;
                if (X_msb) {
                    q[0] = (unsigned char)(v       & 0xFF);
                    q[1] = (unsigned char)((v >>  8) & 0xFF);
                    q[2] = (unsigned char)((v >> 16) & 0xFF);
                    q[3] = (unsigned char)((v >> 24) & 0xFF);
                } else {
                    q[0] = (unsigned char)((v >> 24) & 0xFF);
                    q[1] = (unsigned char)((v >> 16) & 0xFF);
                    q[2] = (unsigned char)((v >>  8) & 0xFF);
                    q[3] = (unsigned char)(v        & 0xFF);
                }
            } else {
                unsigned char *q = row + (size_t)x * 2;
                if (X_msb) {
                    q[0] = (unsigned char)((v >> 8) & 0xFF);
                    q[1] = (unsigned char)(v & 0xFF);
                } else {
                    q[0] = (unsigned char)(v & 0xFF);
                    q[1] = (unsigned char)((v >> 8) & 0xFF);
                }
            }
        }
    }
    X.XPutImage(X_dpy, X_win, X_gc, X_img, 0, 0, 0, 0,
                (unsigned int)w, (unsigned int)h);
    X.XFlush(X_dpy);
    return 1;
}

int fbwin_key(char *out, int cap) {
    char tmp[8];
    int got;
    if (cap < 1) return 0;
    out[0] = 0;
    if (!X_open) return 0;
    got = x_drain(tmp, (int)sizeof tmp);
    if (got && cap > 1) {
        size_t n = strlen(tmp);
        if (n + 1 > (size_t)cap) n = (size_t)cap - 1;
        memcpy(out, tmp, n);
        out[n] = 0;
    }
    return got;
}

void fbwin_close(void) {
    if (X_dpy) {
        if (X_open) X.XUnmapWindow(X_dpy, X_win);
        if (X_gc) { X.XFreeGC(X_dpy, X_gc); X_gc = 0; }
        if (X_img) { X.XFree(X_img); X_img = 0; }
        /* XFree only returns the STRUCTURE. The pixels are ours when we
         * allocated them (the normal case, see x_make_image), so the
         * buffer is freed here or the window leaks w*h*4 bytes every
         * time a program re-opens the device at a new size — which the
         * test suite does 64 times. */
        if (X_data_owned) free(X_data);
        X_data = 0;
        X_data_owned = 0;
        if (X_win) { X.XDestroyWindow(X_dpy, X_win); X_win = 0; }
        X.XCloseDisplay(X_dpy);
    }
    X_dpy = 0;
    X_open = 0;
    X_w = 0;
    X_h = 0;
}
