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
 *   6. fb_input_line(kf) never blocks when there is nothing pending
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
#include <signal.h>
#include <poll.h>
#if defined(__linux__)
#include <pty.h>
#include <sys/ioctl.h>
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
    const char *kf = 0;   /* no key file: only a real keyboard */
    unsetenv("FB_KEYS");
    unsetenv("FB_INPUT");
    unsetenv("FB_TTY");
    setenv("FB_NO_TTY", "1", 1);
    ckstr(fb_input_line(kf), "", "no source -> empty key");
    ckstr(fb_input_line(kf), "", "no source -> still empty (no crash)");
    ck(!fb_input_ready(), "no source -> ready() is false");
    setenv("FB_NO_TTY", "0", 1);
}

/* ── 2/3. the key file ───────────────────────────────────────────── */
static void t_keyfile(void) {
    const char *kf;   /* the device is given the queue as a PARAMETER */
    const char *p = "/tmp/fbinput-keys";
    unlink(p);
    write_file(p, "w\na\ns\n\nd\n");
    kf = p;              /* the game passes its key-file var by value */
    setenv("FB_NO_TTY", "1", 1);

    /* keys come back in order, one per call */
    ckstr(fb_input_line(kf), "w", "keyfile 1st key");
    ckstr(fb_input_line(kf), "a", "keyfile 2nd key");
    ckstr(fb_input_line(kf), "s", "keyfile 3rd key");
    /* a blank line is a REAL key: the game treats "" as "no key", so a
     * scripted run paces itself with blank lines. It must not be
     * swallowed (that would silently drop a frame's worth of time). */
    ckstr(fb_input_line(kf), "", "keyfile blank line is a key");
    ckstr(fb_input_line(kf), "d", "keyfile key after a blank line");
    /* end of file is "" forever, not a hang and not a stale repeat */
    ckstr(fb_input_line(kf), "", "keyfile EOF -> empty");
    ckstr(fb_input_line(kf), "", "keyfile EOF -> empty again");

    /* a key appended AFTER the first read must be seen: the file is
     * held open, so a run that queues keys progressively works */
    append_file(p, "q\n");
    ckstr(fb_input_line(kf), "q", "keyfile sees an appended key");

    /* POINTING THE DEVICE AT A DIFFERENT QUEUE. The game passes its
     * key-file variable on every call, so a run that changes the file
     * must be served the NEW file. Caching the descriptor forever
     * would keep handing out keys from the first one — which is
     * exactly what happened when the file came from the environment
     * instead and a test pointed $FB_KEYS at a fresh file.
     *
     * Switching back re-opens the original and therefore RESTARTS it:
     * a queue has one cursor, and a cursor is per (device, file)
     * rather than global. The key is the new file's first line — a
     * stale key from the old queue would be the bug. */
    {
        const char *p2 = "/tmp/fbinput-keys2";
        unlink(p2);
        write_file(p2, "z\ny\n");
        ckstr(fb_input_line(p2), "z", "a different queue is served, not the cached one");
        ckstr(fb_input_line(p2), "y", "the new queue continues in order");
        ckstr(fb_input_line(kf), "w", "switching back restarts that queue from its first key");
        unlink(p2);
    }

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
    const char *kf = 0;   /* the event stream is the source, not a file */
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
    ckstr(fb_input_line(kf), "w", "evdev press+release -> one key 'w'");

    /* the same key again in the SAME batch: the second press is a
     * repeat, and the game's one-key-per-frame rule means a held key
     * must not turn into a burst of movement. */
    put_evt(&evs[0], EV_KEY, KEY_W, 1);
    put_evt(&evs[1], EV_KEY, KEY_W, 0);
    wr_events(wfd, evs, 2);
    ckstr(fb_input_line(kf), "w", "evdev second press in its own batch -> 'w'");

    /* a batch whose FIRST record is a SYN. Reading one record per call
     * would return "" here and drop the keypress entirely. */
    put_evt(&evs[0], EV_SYN, SYN_REPORT, 0);
    put_evt(&evs[1], EV_KEY, KEY_A, 1);
    put_evt(&evs[2], EV_KEY, KEY_A, 0);
    wr_events(wfd, evs, 3);
    ckstr(fb_input_line(kf), "a", "evdev key behind a leading SYN -> 'a'");

    /* an UNMAPPED key (F13) then a mapped one in the same batch: the
     * unmapped press is skipped, and the mapped key behind it still
     * lands — a decoder that returned the first record blindly would
     * hand the game an escape and swallow the turn key. */
    put_evt(&evs[0], EV_KEY, KEY_F13, 1);
    put_evt(&evs[1], EV_KEY, KEY_F13, 0);
    put_evt(&evs[2], EV_KEY, KEY_E, 1);
    put_evt(&evs[3], EV_KEY, KEY_E, 0);
    wr_events(wfd, evs, 4);
    ckstr(fb_input_line(kf), "e", "evdev skips an unmapped key, keeps 'e'");

    /* a batch of only unmapped keys yields no key at all */
    put_evt(&evs[0], EV_KEY, KEY_F13, 1);
    put_evt(&evs[1], EV_KEY, KEY_F13, 0);
    wr_events(wfd, evs, 2);
    ckstr(fb_input_line(kf), "", "evdev a batch of only unmapped keys -> none");

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
    ckstr(fb_input_line(kf), "w", "evdev a 3-key burst yields the FIRST key only");
    ckstr(fb_input_line(kf), "", "evdev the rest of the burst is consumed");

    /* nothing left: "" without blocking */
    ckstr(fb_input_line(kf), "", "evdev drained -> empty, no block");
    ckstr(fb_input_line(kf), "", "evdev still empty");

    ck(fb_input_ready(), "evdev: ready() once a source is open");

    unsetenv("FB_INPUT");
    close(wfd);
    unlink(fifo);
}
#endif /* __linux__ */

/* ── 5/6. the tty source over a pty ───────────────────────────────── */
static void t_tty(void) {
    const char *kf = 0;   /* the terminal is the source, not a file */
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

    ckstr(fb_input_line(kf), "w", "tty delivers the first typed key");
    ckstr(fb_input_line(kf), "a", "tty delivers the second typed key");
    ckstr(fb_input_line(kf), "", "tty with nothing pending -> empty");

    waitpid(pid, &status, 0);
    close(mfd);
    unsetenv("FB_TTY");
    (void)buf;
}

/* ── 6. it never blocks ───────────────────────────────────────────── */
static void t_noblock(void) {
    const char *p = "/tmp/fbinput-empty";
    const char *kf;
    unlink(p);
    write_file(p, "");
    kf = p;
    setenv("FB_NO_TTY", "1", 1);
    {
        struct timespec a, b;
        clock_gettime(CLOCK_MONOTONIC, &a);
        fb_input_line(kf);
        fb_input_line(kf);
        fb_input_line(kf);
        clock_gettime(CLOCK_MONOTONIC, &b);
        {
            double ms = (double)(b.tv_sec - a.tv_sec) * 1000.0
                      + (double)(b.tv_nsec - a.tv_nsec) / 1e6;
            ck(ms < 50.0, "an empty key file answers in well under 50ms");
        }
    }
    unlink(p);
}

/* ── 7. a terminal owned by ANOTHER job is refused ────────────────────
 * THE HANG THIS GUARDS AGAINST. The device used to open /dev/tty and
 * reconfigure whatever terminal the session leader owned, with no
 * regard for who that terminal belonged to. A game run by a test
 * harness, a CI job or a background shell is in a DIFFERENT process
 * group from that terminal, so the kernel answers its tcsetattr with
 * SIGTTOU and its read with SIGTTIN — and the default action for both
 * is to STOP. The process froze on the first frame and never woke: the
 * harness saw no image, no message and no exit, only a hang. Twenty
 * three equivalence cases died that way and looked like transpiler
 * failures.
 *
 * THE JOB-CONTROL SETUP IS THE WHOLE TEST, and getting it wrong makes
 * the check pass for the wrong reason. A terminal only produces
 * SIGTTOU when the calling process is in a background process group
 * OF THAT TERMINAL'S SESSION. So the shape is three levels:
 *
 *   parent                      -> forks the "session" child
 *     session child: setsid()   -> owns the pty as its controlling
 *                                    terminal, adopts it as foreground
 *       fg child:               -> the game, in its OWN process group,
 *                                    therefore BACKGROUND on that pty
 *
 * An earlier version of this test forked once and called setpgid on the
 * child. The child was then in a group that owned no terminal at all,
 * tcgetpgrp failed, and the device — correctly — treated the terminal
 * as unowned and used it. The check passed with the guard compiled
 * OUT, which is the failure mode a regression test exists to prevent.
 * It has to fail when the guard is removed; that is verified by
 * building this file against a guard-less fbinput.c.
 *
 * The parent waits with a deadline, because a STOPped child never
 * writes to the pipe and an alarm does not fire for a stopped
 * process. */
static void t_foreign_tty(void) {
    int mfd, sfd, pipefd[2];
    pid_t sess, kid;
    char link[64], target[128];
    char got = '?';
    ssize_t n;
    int status = 0;

    if (openpty(&mfd, &sfd, NULL, NULL, NULL) != 0) {
        printf("  FAIL openpty (skipping foreign-tty check)\n");
        return;
    }
    snprintf(link, sizeof link, "/proc/self/fd/%d", sfd);
    n = readlink(link, target, sizeof target - 1);
    if (n <= 0) { close(mfd); close(sfd); return; }
    target[n] = 0;
    if (pipe(pipefd) != 0) { close(mfd); close(sfd); return; }

    /* THE DEVICE'S OWN STATE MUST NOT LEAK INTO THE CHILD. T_fd, the
     * key-file descriptor and the refusal flag are process globals,
     * and check 5 above deliberately leaves a tty OPEN. A kid forked
     * from this process inherits all of it, so fb_input_line short-
     * circuits on the earlier pty and never re-decides the terminal
     * under test — the check then passes no matter what the guard
     * says, which is exactly how the first version of this test was
     * found to be worthless. The child below therefore EXECs a fresh
     * copy of this binary rather than forking, so the only state it
     * can have is what it sets up itself. */
    sess = fork();
    if (sess < 0) { close(mfd); close(sfd); return; }
    if (sess == 0) {
        char self[512];
        ssize_t m;
        close(pipefd[0]);
        setsid();                       /* our own session */
        if (ioctl(sfd, TIOCSCTTY, 0) < 0) _exit(3);
        kid = fork();
        if (kid < 0) _exit(4);
        if (kid == 0) {
            /* the game: its own process group, so the terminal it is
             * holding belongs to the session but NOT to it */
            setpgid(0, 0);
            usleep(150000);             /* let the session adopt the pty */
            /* the report channel is a FIXED descriptor: exec keeps
             * fds open across it, and the pipe's number here is not
             * predictable in the new image */
            if (pipefd[1] != 3) {
                if (dup2(pipefd[1], 3) < 0) _exit(7);
                close(pipefd[1]);
            }
            m = readlink("/proc/self/exe", self, sizeof self - 1);
            if (m <= 0) _exit(5);
            self[m] = 0;
            setenv("FB_TTY", target, 1);
            setenv("FB_NO_TTY", "0", 1);
            unsetenv("FB_KEYS");
            unsetenv("FB_INPUT");
            execl(self, self, "--foreign-tty-child", (char *)0);
            _exit(6);
        }
        /* the session: make the pty ours, so the game is background */
        tcsetpgrp(sfd, getpgrp());
        usleep(600000);
        /* the game is now stopped or done: reap and report nothing */
        kill(kid, SIGCONT);             /* in case it was STOPped */
        kill(kid, SIGKILL);
        waitpid(kid, &status, 0);
        (void)!write(pipefd[1], "S", 1);   /* 'S' = the game never spoke */
        _exit(0);
    }
    close(pipefd[1]);
    {
        struct pollfd pfd;
        int r, done = 0;
        double waited = 0;
        pfd.fd = pipefd[0];
        pfd.events = POLLIN;
        pfd.revents = 0;
        for (;;) {
            r = poll(&pfd, 1, 20);
            waited += 20;
            if (r == 1) { done = 1; break; }
            if (waited > 3000) break;
        }
        if (done) {
            ssize_t g = read(pipefd[0], &got, 1);
            ck(g == 1 && got == 'E',
               "a terminal owned by another job -> \"\" and RETURNS");
        } else {
            ck(0, "a terminal owned by another job -> \"\" and RETURNS");
        }
    }
    close(pipefd[0]);
    close(mfd);
    waitpid(sess, &status, 0);
    unsetenv("FB_TTY");
}

/* The re-executed half of check 7. A fresh process whose only source is
 * a pty it is BACKGROUND on: it must answer "" and exit, rather than
 * be stopped by the kernel the first time it touches that terminal. */
static int foreign_tty_child(void) {
    const char *kf = 0;
    const char *k = fb_input_line(kf);
    char r = k[0] ? 'K' : 'E';   /* K = it read a key, E = nothing */
    (void)!write(3, &r, 1);
    return 0;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--foreign-tty-child") == 0)
        return foreign_tty_child();
    printf("fbinput-test\n");
    t_nosource();
    t_keyfile();
#if defined(__linux__)
    t_evdev();
#endif
    t_tty();
    t_noblock();
    t_foreign_tty();
    printf("  %d checks, %d failed\n", checks, fails);
    return fails ? 1 : 0;
}
