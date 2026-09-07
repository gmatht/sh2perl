/* uu_run.c — implementation of the uu-ffi runtime wrapper (UU-FFI.md).
 *
 * sh2_uu_capture redirects fd 1 to a pipe and drains it on a reader thread
 * while uu_run runs in-process. This is the exact pattern the prototype
 * validated: bare run-then-read deadlocks on >pipe-buffer output, concurrent
 * drain does not (UU-FFI.md §3, §6).
 *
 * Threading note: a POSIX thread is required. Targets that cannot spawn one
 * (bare WASI/browser) should use the temp-file-sink fallback instead
 * (UU-FFI.md §7.2) — not this file.
 */
#include "uu_run.h"

#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* ------------------------------------------------------------------ */
/* plain run: no capture, caller's fds pass through                    */
/* ------------------------------------------------------------------ */
int sh2_uu_run(int argc, char *const argv[]) {
    if (argc < 1 || !argv[0]) return 127;
    const char *util = argv[0];
    const char *base = strrchr(util, '/');
    util = base ? base + 1 : util;
    return uu_run(util, argc, argv);
}

/* ------------------------------------------------------------------ */
/* capture: pipe fd 1, drain on a reader thread while uu_run runs      */
/* ------------------------------------------------------------------ */

typedef struct {
    char *buf;
    size_t cap;
    size_t len;
    int rfd;
} drain_arg;

static void *drain_thread(void *arg) {
    drain_arg *d = (drain_arg *)arg;
    size_t n = 0;
    while (n < d->cap) {
        ssize_t r = read(d->rfd, d->buf + n, d->cap - n - 1);
        if (r < 0) {
            if (errno == EINTR) continue;
            break;
        }
        if (r == 0) break;
        n += (size_t)r;
    }
    d->len = n;
    d->buf[n] = 0;
    return NULL;
}

int sh2_uu_capture(int argc, char *const argv[],
                   char *buf, size_t cap, size_t *out_len) {
    if (argc < 1 || !argv[0]) return 127;
    if (!buf || cap < 1) return 127;
    buf[0] = 0;

    const char *util = argv[0];
    const char *base = strrchr(util, '/');
    util = base ? base + 1 : util;

    int pfd[2];
    if (pipe(pfd) != 0) return 127;

    /* save + redirect stdout to the pipe, then close the write side here
     * so the ONLY writer is the in-process uu_run writing to fd 1. */
    int saved = dup(1);
    if (saved < 0) { close(pfd[0]); close(pfd[1]); return 127; }
    fflush(stdout);
    if (dup2(pfd[1], 1) < 0) {
        close(saved); close(pfd[0]); close(pfd[1]); return 127;
    }
    close(pfd[1]);

    /* reader thread drains while uu_run runs — prevents the large-output
     * deadlock. */
    pthread_t th;
    drain_arg d = { .buf = buf, .cap = cap, .len = 0, .rfd = pfd[0] };
    if (pthread_create(&th, NULL, drain_thread, &d) != 0) {
        close(pfd[0]); dup2(saved, 1); close(saved); return 127;
    }

    int code = uu_run(util, argc, argv);

    /* restore stdout (this closes the sole write-end fd 1 → the reader will
     * see EOF), join the drain thread, THEN release the read end. */
    dup2(saved, 1);
    close(saved);

    pthread_join(th, NULL);
    close(pfd[0]);

    if (out_len) *out_len = d.len;
    return code;
}
