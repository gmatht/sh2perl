Let me examine the relevant source files to understand the translation patterns.Let me run the current tool to see its actual output:Now I have a thorough understanding of the codebase. Let me write the review.

---

## Idiom Review: `sh/005_args.sh` → Perl Translation

### 1. Original Shell Script vs Generated Perl

**Original shell script** (`examples/005_args.sh`):
```bash
#!/usr/bin/env bash

# Demonstrates reading command-line arguments
# This example is intentionally simple so it parses cleanly

echo "== Argument count =="
echo "$#"

echo "== Arguments =="
for a in "$@"; do
  echo "Arg: $a"
done
```

**Generated Perl** (current `file --perl` output):
```perl
#!/usr/bin/env perl
use strict;
use warnings;
print "== Argument count ==\n";
print scalar(@ARGV), "\n";
print "== Arguments ==\n";
for my $a (@ARGV) {
    print "Arg: $a\n";
}
```

**Hand-written idiomatic Perl** (for comparison):
```perl
#!/usr/bin/env perl
use strict;
use warnings;

print "== Argument count ==\n";
print scalar(@ARGV) . "\n";

print "== Arguments ==\n";
for my $arg (@ARGV) {
    print "Arg: $arg\n";
}
```

---

### 2. Non-Idiomatic Patterns

Three patterns stand out. I discuss each in detail below.

---

### Pattern A: `print scalar(@ARGV), "\n"` — list-argument-style print

**What the generator emits:**
```perl
print scalar(@ARGV), "\n";
```

**What idiomatic Perl would look like:**
```perl
print scalar(@ARGV) . "\n";
```

**Why it's non-idiomatic.** The generator passes the value and the newline as *two separate arguments* to `print`, separated by a comma. This directly mirrors shell's `echo "$#"`, where `echo` receives the count as one word and adds the newline automatically. In Perl, `print` with a comma-separated list still works — each argument is evaluated in list context and printed — but it reads like a word-by-word transliteration. Native Perl style prefers either:

- String concatenation: `print scalar(@ARGV) . "\n";` — a single expression, one semantic unit
- String interpolation: `print "$#ARGV\n";` — but `$#ARGV` is the last index, not the count, so this would be off by one

**IR-fixable?** YES. This is a pure style choice in the IR backend.

**IR node involved.** The statement is produced by `IrStmt::Output { value, newline: true }` in `ir.rs`. The current `emit_stmt` handler for `Output { newline: true }` has two branches:

1. If the value is a double-quoted string literal → embeds `\n` directly: `print "content\n";` ✓ (this path already produces clean code)
2. Otherwise → emits `print EXPR, "\n";` — this is the problematic path

The fix is in the `else` branch of `IrStmt::Output` handling in `src/ir.rs` (around line 180-190):

```rust
// Current (non-idiomatic):
print {}, "\\n";

// Fixed (idiomatic):
print {} . "\\n";
```

**Cleaned-up output with the fix:**
```perl
print "== Argument count ==\n";
print scalar(@ARGV) . "\n";
print "== Arguments ==\n";
for my $a (@ARGV) {
    print "Arg: $a\n";
}
```

The other `print` statements (the ones with string literals) already take the first branch and embed `\n` directly, so they are already clean. Only the `scalar(@ARGV)` case triggers the problematic branch.

**Why concatenation instead of `say`.** The IR backend intentionally avoids `use feature 'say'` (comment in ir.rs: "This avoids the dependency on `use feature 'say'`"). Concatenation with `.` is the natural choice that avoids both the `say` dependency and the comma-separated list style.

---

### Pattern B: `$a` as the loop variable

**What the generator emits:**
```perl
for my $a (@ARGV) {
```

**What idiomatic Perl would look like:**
```perl
for my $arg (@ARGV) {
```

**Why it's non-idiomatic.** In Perl, `$a` and `$b` are special package variables used by the `sort` builtin as the two comparison operands. Using them as ordinary loop variables is legal (they are re-declared as lexical by `for my $a`) but is considered poor style. Every Perl::Critic policy (at least up to level 3) and most style guides advise against it. The shell script uses `a` as its loop variable; the translator maps it mechanically to `$a` without checking for the `sort` conflict.

**IR-fixable?** NO. The variable name originates in the shell AST's `ForLoop { variable: "a" }` node. The Perl IR models it as `IrStmt::For { var: "a", ... }` — it is a semantic variable name, not a formatting artifact. Changing it requires a *name-mangling pass* in the generator (or in a future ShIR layer) that detects names colliding with Perl special variables and renames them. This is generator logic, not pretty-printing.

The check is: if the loop variable is `a` or `b`, rename it to `arg` or `item` (or append a suffix). A simple mapping in `generate_for_loop_impl` in `src/generator/control_flow.rs` would suffice. Alternatively, the IR optimizer pass in `ir.rs` could detect the collision and rename, but that would be semantically surprising — the safe place is in the generator when the IR is first constructed.

---

### Pattern C: Lost comments

**What the generator emits:**
```perl
print "== Argument count ==\n";
```

The shell script's comments (`# Demonstrates reading command-line arguments`, `# This example is intentionally simple so it parses cleanly`) are absent.

**IR-fixable?** NO. The shell parser/lexer (`src/lexer.rs`, `src/parser/mod.rs`) does not store comments in the AST. The AST has no `Comment` node. The generator works from the AST only and has no access to comment text. This requires parser-level changes:

- The lexer would need to preserve comment tokens
- The AST would need a `Comment` variant (or a side-channel for attached comments)
- The generator would emit them as `#` prefixed lines or `=pod`/`=cut` blocks

This is a data-model change, not a code-generation style change.

---

### 3. Unnecessarily Verbose Translations

For this particular script (three `echo` commands and one `for` loop), the generated output is remarkably clean. The current generator avoids the worst boilerplate that plagued earlier versions. However, I can still identify one pattern that is unnecessarily verbose:

**The `print EXPR, "\n"` pattern (Pattern A above) as a general style.**

Consider a hypothetical line that echoes a variable:
```
echo "Value: $var"
```

The IR backend would produce:
```perl
print "Value: $var\n";
```
✓ This takes the double-quoted-string branch, so it's clean.

But for any non-string value (count, arithmetic, function return):
```
echo $((x + y))
```

The current generator would produce:
```perl
print eval { int(x + y) } // "", "\n";
```

With the concatenation fix, it would produce:
```perl
print eval { int(x + y) } // "" . "\n";
```

Better, but still not ideal. A further improvement (beyond this review's scope) would be to special-case simple scalars and embed them in a double-quoted string:
```perl
print "x + y\n";
```

But the concatenation fix alone solves the primary non-idiomatic pattern.

---

### 4. Summary Table

| Pattern | Location in generated code | IR-fixable? | IR node involved | Preferred output |
|---|---|---|---|---|
| `print EXPR, "\n"` instead of concatenation | `print scalar(@ARGV), "\n";` | **Yes** | `IrStmt::Output { value, newline: true }` | `print scalar(@ARGV) . "\n";` |
| `$a` as loop variable (collides with `sort`) | `for my $a (@ARGV)` | **No** (needs generator name-mangling) | `IrStmt::For { var: "a", ... }` | `for my $arg (@ARGV)` |
| Lost comments | Entire file | **No** (needs parser/AST changes) | — | `# Demonstrates reading command-line arguments` |

### 5. Bottom Line

The generated Perl for `005_args.sh` is already quite good. The IR backend's `Output` handler produces clean `print "literal\n"` for string literals, and the overall structure (`for`, `@ARGV`, `use strict/warnings`) is idiomatic.

The two actionable items are:

1. **Change one line in `ir.rs`** — switch the non-string-literal branch of `Output { newline: true }` from `print {}, "\\n"` to `print {} . "\\n"`. This eliminates the transliteration-sounding comma-separated print style for all non-string outputs (not just this script).

2. **Add a name-mangling check in the for-loop generator** (or in a variable-analysis pass) — if a loop variable is `a` or `b`, rename it to `arg` or `item` to avoid colliding with `sort`'s special variables.

These two changes, neither one difficult, would make the generated Perl indistinguishable from hand-written code for this class of simple scripts.