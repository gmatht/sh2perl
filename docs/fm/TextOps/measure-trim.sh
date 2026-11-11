#!/bin/bash
# measure-trim.sh — measure each engine's TRIM class and its affix-agreement,
# so the tables in ShIR_TextOps_Strings.thy are a transcription of MEASURED
# behaviour rather than of the renderer's comment.
#
# The question is narrow and empirical:
#
#   * which code points does each renderer of `StringTrim` remove from an END?
#       perl    s/^\s+//            (and s/\s+$// for the trailing end)
#       node    String.prototype.trim()
#       python  str.strip()         (the source comment claims a FIXED six-byte
#                                    ASCII set — this prints the real one)
#       rust    str::trim()
#       go      strings.TrimSpace
#       java    String.strip()      (Character.isWhitespace)
#       zig     std.mem.trim(u8, t, " \t\n")  — a LITERAL set in the renderer,
#                                    checked here as a set, not run
#   * whether a trailing `s/\s+$//` is really a trailing RUN strip (`$` also
#     matches before a final newline, which is a classic over-match);
#   * whether the trailing StringAffix arms agree with `endsWith` for the
#     EMPTY pattern (Perl's is `substr($t, -length($p)) eq $p`);
#   * whether AffixStrip strips ONE occurrence (the node's contract) or every
#     occurrence (`trim_start_matches`-style).
#
# Usage: ./measure-trim.sh [outdir]     (default /tmp/trim)
# Re-run after an engine upgrade before trusting a table; the theory cites the
# version strings printed here.
set -u
out=${1:-/tmp/trim}
mkdir -p "$out"

{
  perl -v | sed -n '2p'
  echo "node:   $(node --version)"
  echo "python: $(python3 --version)"
  rustc --version
  command -v go >/dev/null && go version
  java -version 2>&1 | head -1
} > "$out/versions.txt"

# ── perl \s (both string models) is measured by measure-space.sh; here the ──
# ── trim-specific facts: the two-sided run strip, and the empty affix.    ──
perl -e '
  my @cases = ("\x20a\x20", "a  \nb", "a \n\n", "a\n");
  for my $t (@cases) {
    my $l = $t; $l =~ s/^\s+//;
    my $r = $t; $r =~ s/\s+$//;
    printf "perl run strip %-12s -> lead=[%s] trail=[%s]\n",
      unpack("H*", $t), unpack("H*", $l), unpack("H*", $r);
  }
  # trailing StringAffix arm: substr($t, -length($p)) eq $p
  for my $p ("", "c", "abc", "abcd") {
    my $t = "abc";
    my $hit = (substr($t, -length($p)) eq $p) ? 1 : 0;
    printf "perl suffix test t=abc p=[%s] -> %d (endsWith says %d)\n",
      $p, $hit, (($p eq "") || (substr($t, -length($p)) eq $p)) ? 1 : 0;
  }
' > "$out/perl.txt"

# ── ECMAScript String.prototype.trim() ─────────────────────────────────────
node -e '
let out = [];
for (let c = 0; c <= 0x10FFFF; c++) {
  let s;
  try { s = String.fromCodePoint(c); } catch (e) { continue; }
  if (s.trim() === "") out.push(c);
}
console.log(out.join("\n"));
' > "$out/js.txt"

# ── python str.strip() ────────────────────────────────────────────────────
python3 -c '
import sys
w = sys.stdout.write
for c in range(0x110000):
    try: s = chr(c)
    except ValueError: continue
    if s.strip() == "": w("%d\n" % c)
' > "$out/python.txt"

# ── rust str::trim() ──────────────────────────────────────────────────────
cat > "$out/trim.rs" <<'RS'
fn main() {
    for c in 0u32..=0x10FFFF {
        if let Some(ch) = char::from_u32(c) {
            let s = ch.to_string();
            if s.trim().is_empty() { println!("{}", c); }
        }
    }
}
RS
rustc -O -o "$out/trim" "$out/trim.rs" 2>/dev/null && "$out/trim" > "$out/rust.txt"

# ── go strings.TrimSpace ──────────────────────────────────────────────────
if command -v go >/dev/null; then
  mkdir -p "$out/go"
  cat > "$out/go/main.go" <<'GO'
package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
)

func main() {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	for c := rune(0); c <= 0x10FFFF; c++ {
		if strings.TrimSpace(string(c)) == "" {
			fmt.Fprintf(w, "%d\n", c)
		}
	}
}
GO
  ( cd "$out/go" && GO111MODULE=off go run main.go > "$out/go.txt" 2>/dev/null )
fi

# ── java String.strip() (Character.isWhitespace) ──────────────────────────
if command -v java >/dev/null; then
  cat > "$out/Trim.java" <<'JAVA'
public class Trim {
    public static void main(String[] a) {
        StringBuilder sb = new StringBuilder();
        for (int c = 0; c <= 0x10FFFF; c++) {
            if (!Character.isValidCodePoint(c)) continue;
            String s = new String(Character.toChars(c));
            if (s.strip().isEmpty()) sb.append(c).append('\n');
        }
        System.out.print(sb);
    }
}
JAVA
  ( cd "$out" && java Trim.java > "$out/java.txt" 2>/dev/null )
fi

# ── affix-agreement probes (go / python / java): empty pattern + one occurrence
if command -v go >/dev/null; then
  cat > "$out/go/affix.go" <<'GO'
package main

import (
	"fmt"
	"strings"
)

func main() {
	fmt.Println("go TrimSuffix(abc,\"\") =", strings.TrimSuffix("abc", ""))
	fmt.Println("go TrimSuffix(aaa,\"a\") =", strings.TrimSuffix("aaa", "a"))
	fmt.Println("go TrimPrefix(aaa,\"a\") =", strings.TrimPrefix("aaa", "a"))
	fmt.Println("go HasSuffix(abc,\"\") =", strings.HasSuffix("abc", ""))
}
GO
  ( cd "$out/go" && GO111MODULE=off go run affix.go 2>/dev/null ) > "$out/go-affix.txt"
fi

python3 -c '
print("py removesuffix(abc,\"\") =", repr("abc".removesuffix("")))
print("py removesuffix(aaa,\"a\") =", repr("aaa".removesuffix("a")))
print("py removeprefix(aaa,\"a\") =", repr("aaa".removeprefix("a")))
print("py endswith(abc,\"\") =", "abc".endswith(""))
' > "$out/python-affix.txt"

node -e '
console.log("js endsWith(abc,\"\") =", "abc".endsWith(""));
console.log("js trim is two-sided only: trimStart/trimEnd exist, the renderer calls trim()");
' > "$out/js-affix.txt"

# ── report ────────────────────────────────────────────────────────────────
echo "== versions"; cat "$out/versions.txt"
for f in js python rust go java; do
  [ -s "$out/$f.txt" ] || { echo "-- $f: NOT MEASURED"; continue; }
  echo "-- $f: $(wc -l < "$out/$f.txt") code points"
  echo "   ascii(<128): $(awk '$1<128' "$out/$f.txt" | tr '\n' ' ')"
  echo "   above: $(awk '$1>=128 {printf "%d ", $1}' "$out/$f.txt")"
done
echo "-- posix is_ws (model): 9 10 11 12 13 32"
echo "-- zig literal set in the renderer: 32 9 10"
echo; echo "== perl"; cat "$out/perl.txt"
echo; echo "== affix probes"
for f in go-affix python-affix js-affix; do [ -s "$out/$f.txt" ] && { echo "-- $f"; cat "$out/$f.txt"; }; done
