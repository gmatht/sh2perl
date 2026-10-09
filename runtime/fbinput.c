/* fbinput.c — the keyboard side of the native fb0 path (see fbinput.h).
 *
 * One non-blocking device call answers "is there a key right now, and
 * which one". The game script's control code does not change: it still
 * reads a value per frame and branches on w/a/s/d/q/e/. — only the
 * SOURCE of the bytes becomes a device instead of a fork per key.
 *
 * No VLAs, no nested functions, no //-in-preprocessor: this compiles
 * clean under both cc -Wall -Wextra and tcc (bash-O4's toolchain).
 */
#define _GNU_SOURCE
#include "fbinput.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <poll.h>
#include <signal.h>
#include <termios.h>
#include <sys/stat.h>
#include <sys/types.h>

#if defined(__linux__)
#include <sys/ioctl.h>
#include <linux/input.h>
#define FBINPUT_HAVE_EVDEV 1
#endif

/* ── state ────────────────────────────────────────────────────────── */
static int  I_timeout = 0;      /* ms; 0 = never block */
static int  I_last_raw = 0;     /* raw byte, or 0 (see fb_input_last_raw) */
static int  I_count = 0;        /* keys consumed */
static char I_line[64] = "";    /* the returned key (static buffer) */

/* the $FB_KEYS source: a file read with read(2), never a shell.
 * The fd is held open across calls (a run appends keys while the game
 * is going) and a short read is NOT end-of-file — only a read(2)
 * returning 0 is. Latching EOF permanently was the first version's
 * bug, and it had a visible consequence: after a scripted run consumed
 * its queued keys, every LATER append was invisible, so a run that
 * paced itself by appending (the only way to script a game that reacts
 * to what it has already drawn) stopped receiving input. */
static int    KF_fd = -1;
static int    KF_eof = 0;
static char   KF_buf[256];
static size_t KF_len = 0, KF_pos = 0;
static char   KF_path[512] = "";   /* the file KF_fd is reading */

/* the evdev source */
static int    EV_fd = -1;
static int    EV_open_rc = 0;

/* the tty source: the game's own stdin. O_NONBLOCK is set on OUR OWN
 * descriptor, so the terminal's own line discipline is never touched
 * from the read side — see t_next. */
static int    T_fd = -1;
static int    T_open_rc = 0;
static int    T_isatty = 0;

/* The terminal's settings as we FOUND them, and whether we are the ones
 * who changed them. See t_restore_game_mode — a game that leaves the
 * user's shell in raw mode is a game that broke their terminal. */
static struct termios T_orig;
static int           T_saved = 0;
static int    T_raw_set = 0;

/* Defined below t_set_game_mode, which registers it with atexit. */
static void   t_restore_game_mode(void);

/* ATEXIT ALONE IS NOT ENOUGH, and getting that wrong is what left the
 * terminal raw for the second time.
 *
 * atexit handlers run for exit() and for return from main. They do
 * NOT run when a process is killed by a SIGNAL, and for an
 * interactive game the signal is the NORMAL way out: Ctrl-C at the
 * terminal delivers SIGINT to the foreground process group, the game
 * dies, and every atexit handler is discarded. Measured on this host
 * over a pty pair, with a game running unbounded:
 *
 *     child exited, status 2 (signalled: 2)
 *     terminal ICANON now: False  ECHO now: False
 *
 * which is the user's shell left echo-less, needing `stty sane` by
 * hand — the exact complaint that fixing atexit was supposed to end,
 * so a fix that only covers clean exits is a fix that misses the way
 * the program is actually stopped.
 *
 * THE HANDLER RESTORES AND THEN RE-RAISES, rather than calling exit.
 * A handler that returns leaves a killed-by-signal process looking
 * like a clean exit, which loses the exit STATUS a caller checks
 * ($? == 130 is how a shell knows the user asked to stop) and, worse,
 * would swallow a second signal — pressing Ctrl-C twice has to mean
 * "stop waiting and die" even if the terminal restore is wedged. So
 * the default disposition is restored first, and the signal is
 * re-raised with the default handler in place, which kills the
 * process exactly as if this device had never been here.
 *
 * Only the signals a person or a harness actually stops a game with
 * are installed. SIGCHLD, SIGWINCH and friends are left alone: a
 * handler that restores the terminal and then re-raises one of those
 * would kill a game for being resized. */
static struct sigaction  T_old_int, T_old_term, T_old_hup, T_old_quit;
static int  T_sigs_installed = 0;

static void t_signal_restore(int sig) {
    /* Restore the terminal FIRST: it is the one thing the user cannot
     * do for themselves afterwards without a new terminal. */
    t_restore_game_mode();
    /* Then put the default action back and re-raise, so the process
     * dies of the signal it was given instead of pretending to have
     * exited normally. */
    switch (sig) {
        case SIGINT:  sigaction(SIGINT,  &T_old_int,  NULL); break;
        case SIGTERM: sigaction(SIGTERM, &T_old_term, NULL); break;
        case SIGHUP:  sigaction(SIGHUP,  &T_old_hup,  NULL); break;
        case SIGQUIT: sigaction(SIGQUIT, &T_old_quit, NULL); break;
        default: break;
    }
    raise(sig);
    /* Unreachable for a signal with the default action, but not
     * silently: if the signal was blocked or ignored for some reason,
     * leave by signal anyway rather than returning into a game that
     * just handled a quit request. */
    _exit(128 + sig);
}

static void t_install_signal_handlers(void) {
    struct sigaction sa;
    int i;
    static const int sigs[4] = { SIGINT, SIGTERM, SIGHUP, SIGQUIT };
    struct sigaction *olds[4];
    olds[0] = &T_old_int; olds[1] = &T_old_term;
    olds[2] = &T_old_hup;  olds[3] = &T_old_quit;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = t_signal_restore;
    sigemptyset(&sa.sa_mask);
    /* NO SA_RESTART: a signal must interrupt the game's blocking
     * poll(2) on the terminal, or Ctrl-C would sit in the kernel
     * until the next keypress arrived. */
    sa.sa_flags = 0;
    for (i = 0; i < 4; i++) {
        if (sigaction(sigs[i], &sa, olds[i]) != 0) {
            /* One failed install is not fatal; it only means that
             * signal leaves the terminal as it always did. Keeping
             * going lets the other three work. */
        }
    }
    T_sigs_installed = 1;
}

/* Is this terminal SAFE for us to read from and reconfigure? (see
 * t_open for why it is a question at all).
 *
 * The rule is the job-control one, exactly: the kernel raises SIGTTOU
 * (tcsetattr) and SIGTTIN (read) for a member of a BACKGROUND process
 * group of a terminal, and the default action for both is to STOP — a
 * game that touched such a terminal froze on its first frame and never
 * woke, so a harness saw no image, no message and no exit. That is
 * what this check exists to prevent, and a foreground process is the
 * only one that is safe.
 *
 * A terminal with NO session is safe regardless: the signals are
 * raised for a background group OF A SESSION, so a pty pair nobody
 * owns (a test's openpty, a descriptor opened by a process that has no
 * controlling terminal) cannot stop us. Checking that is what keeps
 * the pty test working: a child forked straight from a process with no
 * controlling terminal has tcgetpgrp() == -1, and refusing that would
 * break the tty source in the one place it is legitimately exercised.
 *
 * Note that "same session" is NOT enough — a shell's background job
 * (`cmd &`) shares the session's terminal and is still background on
 * it. Only the foreground process group may touch it. */
static int t_terminal_is_safe(int fd) {
    if (tcgetsid(fd) == (pid_t)-1) return 1;  /* unowned: no one can stop us */
    return tcgetpgrp(fd) == getpgrp();       /* ours and in the foreground */
}

/* An evdev KEY_* name we could not map. The shell gets this (never
 * the device code, which is architecture-specific and 2 bytes wide —
 * a script branching on it would be wrong on any other machine). */
#define UNMAPPED "\x1b"

/* ── $FB_KEYS: the key file, read without forking ─────────────────────
 * The previous head/tail/mv idiom answered "next key" with THREE
 * processes per key. A single read(2) on the same file answers the
 * same question with none, which is the whole point of having a
 * device. The file is opened lazily and kept open: a scripted run
 * appends keys while the game runs, and re-opening per key would both
 * cost a syscall pair and race an append. */
static int kf_open(const char *want) {
    const char *p = (want && *want) ? want : getenv("FB_KEYS");
    if (!p || !*p) return 0;
    if (KF_fd < 0 || strcmp(KF_path, p) != 0) {
        /* a different queue: drop the old descriptor. Without this, a
         * program that changes $FB_KEYS between runs (or a test that
         * points the device at a new file) kept consuming the FIRST
         * file forever, because the fd was cached forever. */
        if (KF_fd >= 0) close(KF_fd);
        KF_fd = -1;
        KF_eof = 0;
        KF_len = 0;
        KF_pos = 0;
    }
    if (KF_fd < 0) {
        /* O_NONBLOCK: a named pipe or an empty regular file must not
         * park the game inside open() waiting for a writer that may
         * never come. */
        KF_fd = open(p, O_RDONLY | O_NONBLOCK);
        if (KF_fd < 0) return 0;
        snprintf(KF_path, sizeof KF_path, "%s", p);
    }
    return 1;
}

/* Next line from the key file, into I_line. Returns 1 when a line was
 * produced, 0 when there is no key waiting. An empty line is a real
 * (blank) key and is returned as such — the game's own code treats ""
 * as "no key", and the file's blank lines are how a scripted run paces
 * itself.
 *
 * A regular file that has reached its end is NOT finished: the writer
 * may still append, and the loop's whole point is to keep drawing
 * while it waits. So end-of-file clears the buffer and lets the next
 * call read again, and a genuine empty read is the "nothing yet"
 * answer, not a latch. */
static int kf_next(char *out, size_t cap) {
    size_t n = 0;
    out[0] = 0;
    for (;;) {
        if (KF_pos >= KF_len) {
            ssize_t r;
            if (KF_eof) {
                /* end reached: look for more, but do not spin on an
                 * unchanging file (one syscall, not a busy loop) */
                struct stat st;
                if (fstat(KF_fd, &st) != 0) { KF_eof = 1; return 0; }
                if ((size_t)st.st_size == KF_len) return 0;
                KF_len = 0;
                KF_pos = 0;
                KF_eof = 0;
            }
            r = read(KF_fd, KF_buf, sizeof KF_buf);
            if (r > 0) {
                KF_len = (size_t)r;
                KF_pos = 0;
            } else if (r == 0) {
                KF_eof = 1;
                return 0;
            } else if (errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR) {
                return 0;
            } else {
                /* no data yet (a writer has not appended): no key */
                out[0] = 0;
                return 0;
            }
        }
        while (KF_pos < KF_len) {
            char c = KF_buf[KF_pos++];
            if (c == '\n') {
                out[n] = 0;
                /* tolerate CRLF key files: a "\r" would not match any
                 * of the game's key tests and the key would vanish */
                if (n > 0 && out[n - 1] == '\r') out[n - 1] = 0;
                return 1;
            }
            if (n + 1 < cap) out[n++] = c;
        }
        /* no newline before EOF: the last partial line is still a key */
        if (KF_eof) {
            out[n] = 0;
            return n > 0;
        }
    }
}

/* ── evdev: /dev/input/eventN ─────────────────────────────────────────
 * This host has no /dev/input at all, so this path is exercised by the
 * test suite through a synthetic device file rather than by a live
 * keyboard — the decode below is the part that can be wrong, and a
 * synthetic event stream is what proves it is not. */
#ifdef FBINPUT_HAVE_EVDEV
/* Is `fd` an evdev node? Decided by ioctl, not by the file name: a
 * named pipe or a regular file standing in for a device (which is
 * exactly what a test uses) must be readable as a raw byte stream
 * when EVIOCGVERSION fails, and a REAL /dev/input/mice node must not
 * be, because its "events" are 3-byte packets that would decode to
 * nonsense. */
static int ev_is_evdev(int fd) {
    unsigned int ver = 0;
    if (ioctl(fd, EVIOCGVERSION, &ver) == 0) return 1;
    {
        struct stat st;
        if (fstat(fd, &st) != 0) return 0;
        if (S_ISCHR(st.st_mode)) {
            /* a character device that is not evdev: refuse rather than
             * decode a foreign packet format as input_event */
            return 0;
        }
    }
    return 1; /* pipe / fifo / regular file: a raw event stream */
}

static int ev_open(void) {
    const char *dev = getenv("FB_INPUT");
    char path[128];
    int i;
    int found = 0;
    if (dev && *dev) {
        EV_fd = open(dev, O_RDONLY | O_NONBLOCK);
        found = EV_fd >= 0;
    } else {
        for (i = 0; i < 8 && !found; i++) {
            snprintf(path, sizeof path, "/dev/input/event%d", i);
            EV_fd = open(path, O_RDONLY | O_NONBLOCK);
            if (EV_fd >= 0) found = 1;
        }
    }
    if (!found) {
        if (EV_fd >= 0) { close(EV_fd); EV_fd = -1; }
        return 0;
    }
    if (!ev_is_evdev(EV_fd)) {
        close(EV_fd);
        EV_fd = -1;
        return 0;
    }
    return 1;
}

/* KEY_* -> the game's own key name. The names are MIMEcroft_fb.sh's:
 * w/a/s/d move, q/e turn, r restart, . quit. Everything else is
 * UNMAPPED — the script then ignores it, which is what a real game
 * does with a key it has no binding for. */
static const char *ev_key_name(unsigned int code) {
    switch (code) {
        case KEY_W: return "w";
        case KEY_A: return "a";
        case KEY_S: return "s";
        case KEY_D: return "d";
        case KEY_Q: return "q";
        case KEY_E: return "e";
        case KEY_R: return "r";
        case KEY_F: return "f";
        case KEY_1: return "1";
        case KEY_2: return "2";
        case KEY_3: return "3";
        case KEY_4: return "4";
        case KEY_5: return "5";
        case KEY_LEFT:  return "h";
        case KEY_RIGHT: return "l";
        case KEY_UP:    return "i";
        case KEY_DOWN:  return "k";
        case KEY_ESC:   return UNMAPPED;
        case KEY_SPACE: return " ";
        case KEY_ENTER: return "\n";
        default: return UNMAPPED;
    }
}

/* Is `nm` the unmapped marker? A POINTER comparison against a string
 * literal is unspecified behaviour (the compiler is free to give each
 * use its own storage), and gcc warns about it for good reason — the
 * check has to be on the CONTENTS. */
static int ev_is_unmapped(const char *nm) {
    return strcmp(nm, UNMAPPED) == 0;
}
#endif /* FBINPUT_HAVE_EVDEV */

/* Drain every event the kernel has buffered, keeping the FIRST key
 * that maps to a name. Returns 1 when a key was produced.
 *
 * WHY EVERY EVENT IS DRAINED, NOT JUST THE FIRST: an X11 evdev node
 * reports a physical key press as press+repeat+release, and only the
 * press carries EV_KEY value 1. Stopping at the first record read
 * would hand the game a SYN_REPORT or a release and drop the keypress
 * entirely — a native port that "sometimes" ignored keys. Draining
 * takes the first press and discards the rest, which is what a
 * keyboard actually delivers: one key per press. */
static int ev_next(char *out, size_t cap) {
#ifdef FBINPUT_HAVE_EVDEV
    struct input_event evs[32];
    if (EV_fd < 0) return 0;
    for (;;) {
        ssize_t r = read(EV_fd, evs, sizeof evs);
        ssize_t i;
        if (r <= 0) return 0;
        for (i = 0; i + (ssize_t)sizeof(struct input_event) <= r;
             i += (ssize_t)sizeof(struct input_event)) {
            struct input_event *e = &evs[i / (ssize_t)sizeof(struct input_event)];
            const char *nm;
            if (e->type != EV_KEY) continue;
            if (e->value != 1) continue;      /* press only, not repeat/release */
            nm = ev_key_name(e->code);
            if (!ev_is_unmapped(nm)) {
                size_t n = strlen(nm);
                if (n + 1 > cap) n = cap - 1;
                memcpy(out, nm, n);
                out[n] = 0;
                I_last_raw = 0;
                return 1;
            }
            /* unmapped press: keep draining, a mapped one may follow */
        }
        if ((size_t)r < sizeof evs) return 0;
    }
#else
    (void)out; (void)cap;
    return 0;
#endif
}

/* ── tty ──────────────────────────────────────────────────────────────
 * Non-canonical, non-echoing, VMIN=0/VTIME=0 — the terminal mode a
 * game needs. See t_set_game_mode for why "leave the terminal in
 * cooked mode and just peek" is not an option.
 */
static int t_set_game_mode(void);

static int t_open(void) {
    const char *t;
    int fd;
    const char *no = getenv("FB_NO_TTY");
    if (no && no[0] == '1') return 0;
    /* The game's own stdin, NOT /dev/tty. /dev/tty names the
     * controlling terminal of the SESSION, which is the whole point
     * and also the trap: a process launched by a test harness, a CI
     * job or a pipeline has no controlling terminal of its own, and
     * /dev/tty resolves to whatever terminal that harness's session
     * leader happens to own. The game then read — and reconfigured —
     * a terminal no human is ever typing into, while its actual
     * input sat unread on stdin. stdin is the descriptor the game was
     * GIVEN, so it is the one that is real for this run. */
    if (getenv("FB_TTY") == NULL) {
        fd = dup(STDIN_FILENO);
        if (fd < 0) return 0;
    } else {
        t = getenv("FB_TTY");
        if (t && *t) {
            fd = open(t, O_RDWR | O_NONBLOCK);
            if (fd < 0) return 0;
        } else {
            fd = dup(STDIN_FILENO);
            if (fd < 0) return 0;
        }
    }
    T_fd = fd;
    T_isatty = isatty(fd);
    if (!T_isatty) {
        /* not a terminal (a pipe, a file, /dev/null): nothing to
         * reconfigure and no user to read a keystroke from */
        close(fd);
        T_fd = -1;
        return 0;
    }
    if (!t_terminal_is_safe(fd)) {
        /* A terminal owned by another job: every call we make to it is
         * answered with SIGTTOU (tcsetattr) or SIGTTIN (read), and
         * the default action for both is to STOP. A test harness that
         * ran a game would hang with no image and no message. Use no
         * keyboard rather than freeze. */
        close(fd);
        T_fd = -1;
        return 0;
    }
    (void)t_set_game_mode();
    return 1;
}
/* Put the terminal into the non-canonical, non-echoing mode a game
 * wants. Done ONCE, at open. The ORIGINAL settings are saved and put
 * back on exit (t_restore_game_mode) — that part was missing, and its
 * absence is why a one-frame run left the user's shell echo-less and
 * line-less, needing `stty sane` by hand every time.
 *
 * Why the mode is still set once and kept for the whole run (rather
 * than toggled around each read): in canonical mode the kernel
 * withholds bytes from a reader until a full LINE has arrived, so a
 * game reading one key per frame saw nothing until the user pressed
 * Enter. Cooked mode is right for a shell (it wants the echo and the
 * line) and useless for a game loop.
 *
 * "THE KERNEL RESTORES IT" IS NOT TRUE, and believing that is exactly
 * how the bug shipped. The claim this comment used to make — that the
 * kernel restores the termios when the session's last descriptor
 * closes — only holds when the pty has no other holder. A terminal
 * session keeps the pty open for the life of the shell, so nothing is
 * left to close when the game exits and the settings simply persist.
 * Measured on this host with pty.openpty(): a child puts the slave
 * into raw mode and exits; the master is still open; the slave is
 * still ICANON-off, ECHO-off afterwards. Restoring is the only thing
 * that works.
 *
 * The change touches only the terminal this process opened, and only
 * while the game runs. Returns 1 when it actually changed something,
 * so the caller can leave the terminal alone when it did not (a
 * non-tty, or an ioctl that failed). */
static int t_set_game_mode(void) {
    struct termios a;
    if (T_fd < 0 || !T_isatty) return 0;
    if (tcgetattr(T_fd, &a) != 0) return 0;
    /* Save BEFORE the first modification, and only once: a re-open
     * must not overwrite the user's settings with raw ones, or the
     * restore would faithfully put raw mode back. */
    if (!T_saved) {
        T_orig = a;
        T_saved = 1;
        atexit(t_restore_game_mode);
        /* The signal handlers go in at the SAME moment as the atexit
         * registration, and for the same reason: the first time this
         * process changes the terminal is the first time it owes the
         * user a way back, on every exit route, not just exit(). */
        if (!T_sigs_installed) t_install_signal_handlers();
    }
    a.c_lflag &= (tcflag_t)~(ICANON | ECHO | ECHOE | ECHOK | ECHONL);
    a.c_cc[VMIN] = 0;
    a.c_cc[VTIME] = 0;
    if (tcsetattr(T_fd, TCSANOW, &a) != 0) return 0;
    T_raw_set = 1;
    return 1;
}

/* Put the terminal back the way we found it. Registered with atexit
 * by t_set_game_mode, and ALSO reached from the signal handler above,
 * and safe to call when we never changed anything.
 *
 * TWO EXIT ROUTES, because there are two, and covering only the first
 * is what left the terminal raw twice. atexit covers exit() and
 * return-from-main; the signal handler covers SIGINT/SIGTERM/SIGHUP/
 * SIGQUIT, which is how an interactive game is actually stopped
 * (Ctrl-C). An explicit call in the game's own cleanup would be a
 * third route, and the worst one: it is the route a program takes
 * least often, because a game that is killed never reaches it.
 *
 * Restoring the SAVED struct rather than re-deriving a "sane" mode
 * matters: it puts back this user's actual settings, including the
 * ones we never touched (flow control, IUTF8, the erase character),
 * where a hand-built tcsetattr would quietly reset them.
 *
 * SAFE TO CALL TWICE, and from a signal handler, which is why it does
 * its own guard: the handler can fire between the tcsetattr and the
 * clearing of T_raw_set on a path that also exits normally, and a
 * second tcsetattr of the same settings is harmless where a use of a
 * cleared flag is not. The handler must not call anything that is not
 * async-signal-safe, so this is deliberately just the flag check and
 * one syscall. */
static void t_restore_game_mode(void) {
    if (T_fd < 0 || !T_saved || !T_raw_set) return;
    /* TCSADRAIN, not TCSANOW: TCSANOW can drop output the game has
     * written but the tty has not yet transmitted, which on a fast
     * final frame is the difference between the last line appearing
     * and being swallowed. The game is exiting, so the wait is short
     * and bounded by what is already in the line discipline. */
    (void)tcsetattr(T_fd, TCSADRAIN, &T_orig);
    T_raw_set = 0;
}

/* Fetch one byte from the tty. `cap` is unused because a tty yields
 * exactly one byte per read. */
static int t_next(char *out, size_t cap) {
    unsigned char c;
    ssize_t r;
    (void)cap;
    if (T_fd < 0) return 0;
    if (I_timeout > 0) {
        struct pollfd pf;
        pf.fd = T_fd;
        pf.events = POLLIN;
        pf.revents = 0;
        if (poll(&pf, 1, I_timeout) <= 0) return 0;
    }
    for (;;) {
        r = read(T_fd, &c, 1);
        if (r == 1) break;
        if (r < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) return 0;
        if (r < 0 && errno == EINTR) continue;
        return 0;  /* EOF or a hard error: the game keeps running */
    }
    I_last_raw = (int)c;
    if (c == '\r' || c == '\n') {
        out[0] = 0;
        return 1;
    }
    out[0] = (char)c;
    out[1] = 0;
    return 1;
}

/* ── the public entry point ────────────────────────────────────────── */
void fb_input_timeout(int ms) {
    if (ms < 0) ms = 0;
    I_timeout = ms;
}

int fb_input_last_raw(void) { return I_last_raw; }
int fb_input_ready(void) { return KF_fd >= 0 || EV_fd >= 0 || T_fd >= 0; }
int fb_input_count(void) { return I_count; }

/* A source that is not there YET is retried on the next call, with no
 * backoff. Resolving a source once meant a game that started before
 * the device existed never saw a key again; backing off by a call
 * count fixed that only after N frames, which is a quarter of a second
 * of a dead keyboard for no reason. A failed open(2) is a path lookup
 * that fails in about a microsecond — at 60 fps that is 0.006% of one
 * core — so "retry every call" is both simpler and the right answer.
 * The cost is paid only while NO source exists; once one is open the
 * retry is skipped entirely. */
static int I_calls = 0;
static int T_refused = 0;   /* a tty exists but must not be touched */

/* Is there a tty to speak of at all? Only used to tell "nothing to
 * read" (retry next call, cheaply) from "there is one but it is not
 * ours" (remember the refusal and stop asking). */
static int tty_exists(void) {
    const char *t = getenv("FB_TTY");
    if (t && *t) return access(t, R_OK) == 0;
    return isatty(STDIN_FILENO);
}

const char *fb_input_line(const char *keyfile) {
    int got = 0;

    I_line[0] = 0;
    I_last_raw = 0;
    I_calls++;

    /* 1. the key file wins: a scripted run must be reproducible even
     *    on a box that also has a live keyboard attached */
    if (kf_open(keyfile)) {
        if (kf_next(I_line, sizeof I_line)) got = 1;
    }
    /* 2. evdev */
    if (!got) {
        if (EV_fd < 0) EV_open_rc = ev_open();
        (void)EV_open_rc;
        if (EV_fd >= 0 && ev_next(I_line, sizeof I_line)) got = 1;
    }
    /* 3. the tty. Re-decided while it is not open, so a test (or a
     * game that sets FB_TTY itself before its first key) can still
     * turn the source on later. It is deliberately NOT retried after
     * a FAILED attempt at a tty that exists: whether this process may
     * touch that terminal is fixed for the whole run, and re-deciding
     * it per frame only re-asks a question whose answer cannot change
     * — while a background process pays a tcgetpgrp syscall for the
     * rest of the game. So the refusal is remembered; "there is no
     * tty here at all" is not. */
    if (!got && T_fd < 0 && !T_refused) {
        T_open_rc = t_open();
        (void)T_open_rc;
        if (T_fd < 0 && tty_exists()) T_refused = 1;
    }
    if (!got && T_fd >= 0 && t_next(I_line, sizeof I_line)) got = 1;

    if (got) I_count++;
    return I_line;
}
