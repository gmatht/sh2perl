/* uu_run.h — in-process coreutils for the sh2perl backends (UU-FFI.md).
 *
 * A thin C wrapper over libcoreutils_ffi's `uu_run` that adds the one piece
 * the C backend needs that the bare C ABI lacks: **capture with concurrent
 * drain**. The prototype (UU-FFI.md §3) proved a naive run-then-read
 * deadlocks once a command writes more than the pipe buffer (uutils writes
 * in-process into the redirected fd), so every capture helper here drains
 * the output fd on a separate reader thread WHILE uu_run runs.
 *
 * This is the runtime seam. A backend emits calls into these helpers instead
 * of `system("bash -c ...")` / `popen` for the genuinely-external commands
 * this is the fallback seam, not the destination.
 *
 * Usage:
 *   cc prog.c uu_run.c -lcoreutils_ffi -lpthread -Wl,-rpath,...   # link
 *   int rc = sh2_uu_run("sort", argv);                            // no capture
 *   int rc = sh2_uu_capture("sort", argv, buf, cap, NULL);        // capture
 */
#ifndef SH2_UU_RUN_H
#define SH2_UU_RUN_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

int uu_run(const char *util, int argc, char *const argv[]);

/* Run `util` (with full argv incl. argv[0]=util name) as a library call,
 * return the exit code. Stdin/stdout/stderr stay the caller's. Use this for
 * a plain command whose output should go to the caller's own stdout. */
int sh2_uu_run(int argc, char *const argv[]);

/* Run `util` and capture its STDOUT into `buf[0..cap-1]` (NUL-terminated,
 * trailing newlines preserved). *out_len (if nonnull) receives the length.
 * Concurrent-drain so large output cannot deadlock. Returns the exit code.
 * STDERR passes through to the caller's stderr. */
int sh2_uu_capture(int argc, char *const argv[],
                   char *buf, size_t cap, size_t *out_len);

#ifdef __cplusplus
}
#endif

#endif /* SH2_UU_RUN_H */
