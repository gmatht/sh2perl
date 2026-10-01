#!/usr/bin/env python3
r"""zsh_traps.py — find render lines that break under `setopt shwordsplit`.

The trap, precisely. bash's `echo A $v` prints ONE line: echo joins its
arguments with a single space. The backend renders it as

    printf '%s\n' A $v

and this backend runs `setopt shwordsplit` to emulate bash's splitting, so an
UNQUOTED `$v` expands into one argv element PER SPLIT WORD. printf has exactly
one %s, so it cycles the format once per element and prints one word per LINE:

    printf '%s\n' "  stdout  : " "not" "a" "tty"
      stdout  : not
      a
      tty

The fix in the renderer is `print -r --`, which joins argv unconditionally and
therefore needs no arity. This script finds the lines still carrying the trap.

Counting argv elements means simulating the shell's quoting rules:

  * `"..."`          one element, NO splitting inside (shwordsplit is off
                     inside double quotes for $var)
  * `'...'`          one element, verbatim
  * bare `$v`        one element PER split word of $v  -> run-dependent
  * `"lit"$v"lit"`   concatenation: the quoted parts contribute one element,
                     the bare $v contributes one per split word -> if the
                     expansion splits into N words the line has N elements
  * `\$`             escaped, literal

So a line is a trap when its printf %s count is less than the number of
elements its argument text can expand to at run time -- i.e. whenever an
unquoted expansion appears anywhere in the argv.

Usage: zsh_traps.py [render-file ...]
       (no args: render the whole corpus through $RENDER)
"""
import os
import re
import subprocess
import sys

ROOT = "/home/llm/sh2loop"
CORE = f"{ROOT}/otranspilerl/target/debug/otranspilerl-cli"
WT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RENDER = os.environ.get("RENDER", f"{WT}/target/debug/shir_render")

# `printf '%s\n' ARGS` or `printf '%s' ARGS` (any leading flags, any spacing)
PRINTF = re.compile(r"""printf\s+(['"])(?P<fmt>(?:[^'"]|\\.)*?)\1(?P<rest>.*)$""")
UNQUOTED_EXPANSION = re.compile(r'(?<!\\)(?<!")\$(?:\{|[A-Za-z_0-9@*#?$!-])')


def shell_words(line: str):
    """Split a line into tokens that are OUTSIDE single and double quotes.

    A `printf` that appears inside an awk program (`awk '{printf "%d", $1}'`)
    or inside a quoted string is not the shell's printf and cannot be hit by
    argv splitting, so it must not be reported.
    """
    toks, cur, in_s, in_d = [], [], False, False
    i = 0
    while i < len(line):
        c = line[i]
        if c == "\\" and not in_s:
            cur.append(c)
            i += 1
            if i < len(line):
                cur.append(line[i])
                i += 1
            continue
        if c == "'" and not in_d:
            in_s = not in_s
        elif c == '"' and not in_s:
            in_d = not in_d
        elif c.isspace() and not in_s and not in_d:
            if cur:
                toks.append("".join(cur))
                cur = []
        else:
            cur.append(c)
        i += 1
    if cur:
        toks.append("".join(cur))
    return toks


def fmt_slot_count(fmt: str) -> int:
    """Number of value slots the format string consumes."""
    return fmt.count("%s") + fmt.count("%b") + fmt.count("%d")


def line_is_trap(line: str) -> bool:
    """True when a printf line's argv can expand past its format's slots."""
    body = line.strip()
    toks = shell_words(body)
    if "printf" not in toks:
        return False
    idx = toks.index("printf")
    # skip flags like -n / --, and stop at a pipe/redirection
    j = idx + 1
    while j < len(toks) and toks[j].startswith("-"):
        j += 1
    if j >= len(toks):
        return False
    fmt_tok = toks[j]
    if not (len(fmt_tok) >= 2 and fmt_tok[0] in "'\"" and fmt_tok[-1] == fmt_tok[0]):
        return False
    fmt = fmt_tok[1:-1]
    if fmt_slot_count(fmt) == 0:
        return False
    argv_toks = toks[j + 1:]
    # an unquoted expansion anywhere in the argv means the element count is
    # run-dependent. `"$v"` is safe (no split inside double quotes);
    # `"lit"$v` is not, because the bare `$v` is outside the quotes.
    return any(UNQUOTED_EXPANSION.search(t) for t in argv_toks)


def scan(text: str):
    return [l for l in text.splitlines() if line_is_trap(l)]


def main() -> int:
    if len(sys.argv) > 1:
        for path in sys.argv[1:]:
            with open(path, encoding="utf-8", errors="replace") as fh:
                hits = scan(fh.read())
            print(f"== {path}: {len(hits)} trap lines")
            for h in hits[:20]:
                print("   ", h.strip())
        return 0

    env = dict(os.environ, OTRANSPILER_ROOT=f"{ROOT}/otranspilerl")
    corpus = []
    for pat in (f"{ROOT}/sh2perl/examples/*.sh",
                f"{ROOT}/sh2perl/frontends/*/testdata/*.sh"):
        import glob
        corpus += sorted(glob.glob(pat))

    total = 0
    for path in corpus:
        try:
            shir = subprocess.run([CORE, "--target", "shir", path],
                                  capture_output=True, env=env, timeout=60).stdout
        except subprocess.TimeoutExpired:
            continue
        if not shir.strip():
            continue
        try:
            out = subprocess.run([RENDER, "--target=zsh", "-"], input=shir,
                                 capture_output=True, env=env, timeout=60).stdout
        except subprocess.TimeoutExpired:
            continue
        hits = scan(out.decode("utf-8", "replace"))
        if hits:
            total += len(hits)
            print(f"{len(hits):4d}  {os.path.basename(path)}")
    print(f"---- total trap lines: {total}")
    return 0


if __name__ == "__main__":
    sys.exit(main())