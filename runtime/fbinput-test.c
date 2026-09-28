/* fbinput-test.c — checks on the keyboard device.
 *
 * The interesting code here is the EVDEV DECODE, and this host has no
 * /dev/input at all — so the decode is exercised against a synthetic
 * event stream written to a FIFO and pointed at by $FB_INPUT, exactly
 * as a real node would be. Without that, "no input device found" would
 * be indistinguishable from "the decode is right", which is the failure
 * mode a test suite must not have.
 *
 * Checks (each is a hand-computed expectation, not a golden file):
 *   1. no source configured  -> "" forever, never blocks, ready()==0
 *   2. key file: one key per line, in order; a blank line is a key;
 *      end of file is "" forever (the game must not spin on it)
 *   3. a key appended AFTER the first read is seen (the file is kept
 *      open, so a scripted run can pace itself)
 *   4. evdev: a press+release pair yields ONE key; a batch of events
 *      whose FIRST record is a SYN yields the key that is in it; an
 *      unmapped key is skipped, and a mapped key after it still lands
 *   5. the tty source, over a pty pair, echoes and delivers one byte
 *   6. fb_input_line() never blocks when there is nothing pending
 */
/* _GNU_SOURCE FIRST, before even the local header: openpty() is a GNU
 * addition, and with only the default feature macros the implicit
 * declaration made fd -1, so the test's "pty" was a closed descriptor
 * and the tty checks silently tested nothing. */
#define _GNU_SOURCE
#include "fbinput.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <sys/stat.h>
#if defined(__linux__)
#include <pty.h>
#include <linux/input.h>
#endif

static int fails = 0, checks = 0;

static void ck(int cond, const char *what) {
    checks++;
    if (!cond) { fails++; printf("  FAIL %s\n", what); }
}

static void ckstr(const char *got, const char *want, const char *what) {
    checks++;
    if (strcmp(got, want) != 0) {
        fails++;
        printf("  FAIL %s: got \"%s\" want \"%s\"\n", what, got, want);
    }
}

static void write_file(const char *path, const char *text) {
    FILE *f = fopen(path, "w");
    if (!f) { printf("  FAIL cannot write %s\n", path); fails++; return; }
    fputs(text, f);
    fclose(f);
}

/* append, without truncating (a running game is reading this file) */
static void append_file(const char *path, const char *text) {
    FILE *f = fopen(path, "a");
    if (!f) { printf("  FAIL cannot append %s\n", path); fails++; return; }
    fputs(text, f);
    fclose(f);
}

/* ── 1. nothing configured ────────────────────────────────────────── */
static void t_nosource(void) {
    unsetenv("FB_KEYS");
    unsetenv("FB_INPUT");
    unsetenv("FB_TTY");
    setenv("FB_NO_TTY", "1", 1);
    ckstr(fb_input_line(), "", "no source -> empty key");
    ckstr(fb_input_line(), "", "no source -> still empty (no crash)");
    ck(!fb_input_ready(), "no source -> ready() is false");
    setenv("FB_NO_TTY", "0", 1);
}

/* ── 2/3. the key file ───────────────────────────────────────────── */
static void t_keyfile(void) {
    const char *p = "/tmp/fbinput-keys";
    unlink(p);
    write_file(p, "w\na\ns\n\nd\n");
    setenv("FB_KEYS", p, 1);
    setenv("FB_NO_TTY", "1", 1);

    /* keys come back in order, one per call */
    ckstr(fb_input_line(), "w", "keyfile 1st key");
    ckstr(fb_input_line(), "a", "keyfile 2nd key");
    ckstr(fb_input_line(), "s", "keyfile 3rd key");
    /* a blank line is a REAL key: the game treats "" as "no key", so a
     * scripted run paces itself with blank lines. It must not be
     * swallowed (that would silently drop a frame's worth of time). */
    ckstr(fb_input_line(), "", "keyfile blank line is a key");
    ckstr(fb_input_line(), "d", "keyfile key after a blank line");
    /* end of file is "" forever, not a hang and not a stale repeat */
    ckstr(fb_input_line(), "", "keyfile EOF -> empty");
    ckstr(fb_input_line(), "", "keyfile EOF -> empty again");

    /* a key appended AFTER the first read must be seen: the file is
     * held open, so a run that queues keys progressively works */
    append_file(p, "q\n");
    ckstr(fb_input_line(), "q", "keyfile sees an appended key");

    unsetenv("FB_KEYS");
    unlink(p);
}

/* ── 4. evdev ────────────────────────────────────────────────────── */
#if defined(__linux__)
static void put_evt(struct input_event *e, unsigned short type,
                    unsigned short code, int value) {
    memset(e, 0, sizeof *e);
    e->type = type;
    e->code = code;
    e->value = value;
}

/* write `n` events to fd */
static void wr_events(int fd, const struct input_event *ev, int n) {
    int i;
    for (i = 0; i < n; i++) {
        if (write(fd, &ev[i], sizeof ev[i]) != (ssize_t)sizeof ev[i]) {
            printf("  FAIL short event write %d\n", i);
            fails++;
            return;
        }
    }
}

static void t_evdev(void) {
    const char *fifo = "/tmp/fbinput-evdev";
    struct input_event evs[8];
    int wfd;

    /* A FIFO stands in for /dev/input/event0. It is a fifo (not a
     * socket) on purpose: open(2) on a socket's /proc/self/fd link
     * fails, so $FB_INPUT could not point at one. A fifo opened
     * O_RDWR never blocks and always has a writer, so the reader never
     * sees a spurious EOF and the TEST writes each batch at the exact
     * moment it wants the game to see it — no sleeps, no races. */
    unlink(fifo);
    if (mkfifo(fifo, 0600) != 0) {
        printf("  FAIL mkfifo %s\n", fifo);
        fails++;
        return;
    }
    wfd = open(fifo, O_RDWR);
    if (wfd < 0) { printf("  FAIL open fifo rw\n"); fails++; unlink(fifo); return; }

    setenv("FB_INPUT", fifo, 1);
    setenv("FB_NO_TTY", "1", 1);
    unsetenv("FB_KEYS");

    /* a press+release pair, then a SYN — the batch a real keyboard
     * sends for one keypress. The decoder must yield exactly ONE key
     * (the press) and not report the release or the SYN. */
    put_evt(&evs[0], EV_KEY, KEY_W, 1);
    put_evt(&evs[1], EV_KEY, KEY_W, 0);
    put_evt(&evs[2], EV_SYN, SYN_REPORT, 0);
    wr_events(wfd, evs, 3);
    ckstr(fb_input_line(), "w", "evdev press+release -> one key 'w'");

    /* the same key again in the SAME batch: the second press is a
     * repeat, and the game's one-key-per-frame rule means a held key
     * must not turn into a burst of movement. */
    put_evt(&evs[0], EV_KEY, KEY_W, 1);
    put_evt(&evs[1], EV_KEY, KEY_W, 0);
    wr_events(wfd, evs, 2);
    ckstr(fb_input_line(), "w", "evdev second press in its own batch -> 'w'");

    /* a batch whose FIRST record is a SYN. Reading one record per call
     * would return "" here and drop the keypress entirely. */
    put_evt(&evs[0], EV_SYN, SYN_REPORT, 0);
    put_evt(&evs[1], EV_KEY, KEY_A, 1);
    put_evt(&evs[2], EV_KEY, KEY_A, 0);
    wr_events(wfd, evs, 3);
    ckstr(fb_input_line(), "a", "evdev key behind a leading SYN -> 'a'");

    /* an UNMAPPED key (F13) then a mapped one in the same batch: the
     * unmapped press is skipped, and the mapped key behind it still
     * lands — a decoder that returned the first record blindly would
     * hand the game an escape and swallow the turn key. */
    put_evt(&evs[0], EV_KEY, KEY_F13, 1);
    put_evt(&evs[1], EV_KEY, KEY_F13, 0);
    put_evt(&evs[2], EV_KEY, KEY_E, 1);
    put_evt(&evs[3], EV_KEY, KEY_E, 0);
    wr_events(wfd, evs, 4);
    ckstr(fb_input_line(), "e", "evdev skips an unmapped key, keeps 'e'");

    /* a batch of only unmapped keys yields no key at all */
    put_evt(&evs[0], EV_KEY, KEY_F13, 1);
    put_evt(&evs[1], EV_KEY, KEY_F13, 0);
    wr_events(wfd, evs, 2);
    ckstr(fb_input_line(), "", "evdev a batch of only unmapped keys -> none");

    /* three keys inside one batch: the frame consumes ONE. A game must
     * not teleport three cells because the keyboard delivered a burst
     * between two frames — this is why the device drains. */
    put_evt(&evs[0], EV_KEY, KEY_W, 1);
    put_evt(&evs[1], EV_KEY, KEY_W, 0);
    put_evt(&evs[2], EV_KEY, KEY_A, 1);
    put_evt(&evs[3], EV_KEY, KEY_A, 0);
    put_evt(&evs[4], EV_KEY, KEY_D, 1);
    put_evt(&evs[5], EV_KEY, KEY_D, 0);
    wr_events(wfd, evs, 6);
    ckstr(fb_input_line(), "w", "evdev a 3-key burst yields the FIRST key only");
    ckstr(fb_input_line(), "", "evdev the rest of the burst is consumed");

    /* nothing left: "" without blocking */
    ckstr(fb_input_line(), "", "evdev drained -> empty, no block");
    ckstr(fb_input_line(), "", "evdev still empty");

    ck(fb_input_ready(), "evdev: ready() once a source is open");

    unsetenv("FB_INPUT");
    close(wfd);
    unlink(fifo);
}
#endif /* __linux__ */

/* ── 5/6. the tty source over a pty ───────────────────────────────── */
static void t_tty(void) {
    int mfd, sfd;
    pid_t pid;
    int status = 0;
    char buf[8];

    /* A pty pair gives a real terminal: isatty() true, the line
     * discipline and echo in force — the same conditions as a user's
     * terminal, which a pipe would not reproduce. */
    if (openpty(&mfd, &sfd, NULL, NULL, NULL) != 0) {
        printf("  FAIL openpty (skipping tty checks)\n");
        return;
    }
    pid = fork();
    if (pid < 0) { printf("  FAIL fork\n"); fails++; return; }
    if (pid == 0) {
        /* the child is the "user typing": it writes to the MASTER, and
         * then holds still until the parent has read. On a pty, what
         * the user types arrives at the master and what the program
         * reads is the SLAVE's input queue — writing to the slave
         * instead is the program TALKING (output), which the reader
         * never sees. The first version of this test did exactly that
         * and so verified nothing.
         *
         * The child must not close its end early: a pty whose last
         * master closes sends the slave a HANGUP, which is why the
         * parent saw no keys at all until the hold was added. */
        usleep(20000);
        if (write(mfd, "w", 1) != 1) _exit(2);
        usleep(20000);
        if (write(mfd, "a", 1) != 1) _exit(2);
        usleep(200000);
        close(mfd);
        _exit(0);
    }
    /* let both keystrokes land before the first read; the device is
     * non-blocking, so this only costs the test a moment, and it makes
     * the two keys deterministic instead of racing the child */
    usleep(120000);
    /* $FB_TTY is a PATH, so it must be the SLAVE's name (/dev/pts/N).
     * Derive it from the slave fd BEFORE closing it — the master-side
     * link is /dev/ptmx, and opening that hands back a brand-new
     * master, which is why the first version of this test saw no keys
     * at all. */
    {
        char link[64], target[128];
        ssize_t n;
        snprintf(link, sizeof link, "/proc/self/fd/%d", sfd);
        n = readlink(link, target, sizeof target - 1);
        if (n <= 0) {
            printf("  FAIL readlink slave fd %d -> %s\n", (int)sfd,
                   strerror(errno));
            fails++;
            fflush(stdout);
            _exit(1);
        }
        target[n] = 0;
        close(sfd);
        setenv("FB_TTY", target, 1);
    }
    unsetenv("FB_KEYS");
    unsetenv("FB_INPUT");
    setenv("FB_NO_TTY", "0", 1);

    ckstr(fb_input_line(), "w", "tty delivers the first typed key");
    ckstr(fb_input_line(), "a", "tty delivers the second typed key");
    ckstr(fb_input_line(), "", "tty with nothing pending -> empty");

    waitpid(pid, &status, 0);
    close(mfd);
    unsetenv("FB_TTY");
    (void)buf;
}

/* ── 6. it never blocks ───────────────────────────────────────────── */
static void t_noblock(void) {
    const char *p = "/tmp/fbinput-empty";
    unlink(p);
    write_file(p, "");
    setenv("FB_KEYS", p, 1);
    setenv("FB_NO_TTY", "1", 1);
    {
        struct timespec a, b;
        clock_gettime(CLOCK_MONOTONIC, &a);
        fb_input_line();
        fb_input_line();
        fb_input_line();
        clock_gettime(CLOCK_MONOTONIC, &b);
        {
            double ms = (double)(b.tv_sec - a.tv_sec) * 1000.0
                      + (double)(b.tv_nsec - a.tv_nsec) / 1e6;
            ck(ms < 50.0, "an empty key file answers in well under 50ms");
        }
    }
    unsetenv("FB_KEYS");
    unlink(p);
}

int main(void) {
    printf("fbinput-test\n");
    t_nosource();
    t_keyfile();
#if defined(__linux__)
    t_evdev();
#endif
    t_tty();
    t_noblock();
    printf("  %d checks, %d failed\n", checks, fails);
    return fails ? 1 : 0;
}
