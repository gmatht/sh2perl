/* fbinput.h — the /dev/input + tty keyboard device for bash-O4 games.
 *
 * The output half of the native path is fbdev.{h,c} (an fb0 framebuffer
 * the game draws into). This is its other half: where the game's keys
 * come from. A MIMEcroft_fb.sh game loop currently consumes movement
 * from a KEY FILE (head/tail/mv, one key per line) because the shell
 * `read` builtin has nowhere to read from on the native path.
 *
 * fb_input_line() is a one-line, non-blocking read of ONE key, lowered
 * by the C backend as a command substitution:
 *
 *     key=$(fb_input_line)
 *
 * so the game's OWN control code is unchanged — only the source of the
 * bytes changes (a device instead of a fork per key). Every device key
 * is translated here to the game's own name (w/a/s/d/q/e/./...), so the
 * key table in the script needs no translation layer and no
 * architecture-specific keycode knowledge leaks into the shell.
 *
 * WHY ONE call instead of a `read` builtin lowering
 *   bash's `read` is a BLOCKING line read on a tty, and the C backend
 *   lowers a read from a non-tty to a `bash -c` site: a fork+exec per
 *   keypress, which is exactly the cost this device exists to remove.
 *   A game loop wants the opposite of blocking anyway (keep drawing
 *   while nobody is pressing anything), so the natural primitive is
 *   "give me the next key if there is one, else give me the empty
 *   string" — one device call, no fork, never blocks, and a
 *   `$(...)` capture of it is exactly the shape the game already has.
 *
 * SOURCES, in resolution order (first one with input wins):
 *   1. $FB_KEYS      — a key FILE, one key per line (the existing
 *                      head/tail idiom's payload, read without forking;
 *                      this is what makes a headless scripted run
 *                      work on a box with no input at all)
 *   2. $FB_TTY / $FB_INPUT (default /dev/input/event0) — evdev, which
 *      is a real character stream (unlike /dev/input/mice)
 *   3. stdin, when it is a tty — cooked mode, so the terminal ECHOes
 *      the keypress exactly as a normal shell prompt would (a raw-mode
 *      read would eat the echo and the tty would look broken)
 *
 * Both the file and evdev sources are non-blocking; the tty source
 * uses VMIN=0/VTIME=0 via poll, so a call with nothing pending costs
 * one poll and returns "" immediately.
 */
#ifndef FBINPUT_H
#define FBINPUT_H

/* How long fb_input_line blocks waiting for a key, in milliseconds.
 * 0 = never block (return "" when nothing is pending), which is what a
 * game loop wants. The tty is temporarily switched to a non-blocking
 * read for the duration of the call and restored immediately after,
 * so this never changes the terminal's own line discipline. */
void  fb_input_timeout(int ms);

/* Read ONE key, translated to its game name, with the trailing
 * newline stripped (so a line-oriented key file and a keystroke look
 * the same to the script). Returns a pointer to a static buffer,
 * valid until the next call. Returns "" when nothing is pending, and
 * "\x1b" (a lone ESC) for an unrecognised or unmapped device key.
 *
 * `keyfile` is the scripted-input queue (the device's FB_KEYS env var
 * is used when this is NULL, and when neither is set only a real
 * keyboard is read). It is a PARAMETER and not just an env var
 * because a game keeps its key queue in a shell variable, and an
 * `export` in a transpiled program does not reach a C device: the
 * emitted C assigns a variable and never calls setenv, so the device's
 * getenv would always see the pre-run environment and the game would
 * silently never receive a key. An argument keeps the value in the
 * program's own data, where no export plumbing can lose it. */
const char *fb_input_line(const char *keyfile);

/* The byte the last fb_input_line() saw from the tty, or 0 when the
 * key came from a file/evdev mapping (or there was no key). Lets a
 * test tell a real ESC from an unmapped-code ESC, since both render
 * as "\x1b". */
int  fb_input_last_raw(void);

/* Non-zero once a key source has been opened successfully. A game can
 * use this to warn once ("no keyboard found") instead of silently
 * looking frozen. */
int  fb_input_ready(void);

/* Number of keys consumed from $FB_KEYS / stdin since the device was
 * first used. Diagnostics only; the game must not branch on it (a
 * scripted run and a live run are the same run). */
int  fb_input_count(void);

#endif /* FBINPUT_H */
