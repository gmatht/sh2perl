#!/bin/bash
# measure-space.sh — measure each engine's whitespace class, so the tables in
# ShIR_TextOps_Space.thy are a transcription of MEASURED behaviour rather than
# of the documentation.
#
# The question the theory asks is narrow and empirical: for which code points c
# does `c` count as whitespace to the engine that renders WordCount / Split?
#    perl    \s   (both string models: byte semantics and unicode_strings)
#    node    /\s/ (the Estree backend's regex, ECMAScript class)
#    python  str.isspace()      (Backend::Python's str.split())
#    rust    char::is_whitespace (Backend::Rust's split_whitespace)
#    go      unicode.IsSpace     (Backend::Go's strings.Fields)
# and, for contrast, POSIX [[:space:]] as this repo models it (is_ws in
# ShIR_TextOps_Base.thy: 9 10 11 12 13 32).
#
# Usage: ./measure-space.sh [outdir]     (default /tmp/space)
# Re-run it after an engine upgrade before trusting a table; the theory cites
# the version strings printed here.
set -u
out=${1:-/tmp/space}
mkdir -p "$out"
MAX=1114111   # U+10FFFF

perl -v | sed -n '2p'  > "$out/versions.txt"
echo "node:   $(node --version)" >> "$out/versions.txt"
echo "python: $(python3 --version)" >> "$out/versions.txt"
rustc --version >> "$out/versions.txt" 2>&1

# ── perl, default (byte) string model ───────────────────────────────────────
perl -e 'for my $c (0..1114111) { my $s; eval { $s = chr($c) }; next unless defined $s; print "$c\n" if $s =~ /\s/ }' \
  > "$out/perl-default.txt"

# ── perl, unicode_strings (what `use v5.12`+ and utf8-flagged data give) ─────
perl -e 'use feature "unicode_strings"; for my $c (0..1114111) { my $s; eval { $s = chr($c) }; next unless defined $s; print "$c\n" if $s =~ /\s/ }' \
  > "$out/perl-unicode.txt"

# ── ECMAScript /\s/ ────────────────────────────────────────────────────────
node -e '
let out = [];
for (let c = 0; c <= 0x10FFFF; c++) {
  let s;
  try { s = String.fromCodePoint(c); } catch (e) { continue; }
  if (/\s/.test(s)) out.push(c);
}
process.stdout.write(out.join("\n") + "\n");
' > "$out/js.txt"

# ── python str.isspace() ───────────────────────────────────────────────────
python3 -c '
import sys
w = sys.stdout.write
for c in range(0x110000):
    try: s = chr(c)
    except ValueError: continue
    if s.isspace(): w("%d\n" % c)
' > "$out/python.txt"

# ── rust char::is_whitespace ───────────────────────────────────────────────
cat > "$out/ws.rs" <<'RS'
fn main() {
    for c in 0u32..=0x10FFFF {
        if let Some(ch) = char::from_u32(c) {
            if ch.is_whitespace() { println!("{}", c); }
        }
    }
}
RS
rustc -O -o "$out/ws" "$out/ws.rs" 2>/dev/null && "$out/ws" > "$out/rust.txt"

# ── go unicode.IsSpace ─────────────────────────────────────────────────────
if command -v go >/dev/null; then
  mkdir -p "$out/go"
  cat > "$out/go/main.go" <<'GO'
package main

import (
	"bufio"
	"fmt"
	"os"
	"unicode"
)

func main() {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	for c := rune(0); c <= 0x10FFFF; c++ {
		if unicode.IsSpace(c) {
			fmt.Fprintf(w, "%d\n", c)
		}
	}
}
GO
  ( cd "$out/go" && GO111MODULE=off go run main.go > "$out/go.txt" 2>/dev/null )
fi

# ── report ─────────────────────────────────────────────────────────────────
echo "== versions"; cat "$out/versions.txt"
for f in perl-default perl-unicode js python rust go; do
  [ -f "$out/$f.txt" ] || { echo "-- $f: NOT MEASURED"; continue; }
  echo "-- $f: $(wc -l < "$out/$f.txt") code points"
  echo "   ascii(<128): $(awk '$1<128' "$out/$f.txt" | tr '\n' ' ')"
  echo "   above: $(awk '$1>=128 {printf "%d ", $1}' "$out/$f.txt")"
done
echo "-- posix is_ws (model): 9 10 11 12 13 32"
