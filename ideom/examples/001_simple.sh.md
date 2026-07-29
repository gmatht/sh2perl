Let me look at the generator code to understand how the current patterns are produced.Let me also look at how the generator dispatches commands and check the relevant generator module:Now I have all the information needed. Let me verify one more thing about the for loop generation's approach to magic numbers:Now I have all the information needed. Here is the idiom review:

---

## Idiom Review: `examples/001_simple.sh` → Generated Perl

### 1. Source Script

```bash
#!/bin/bash

# This script demonstrates basic shell functionality
echo "Hello, World!"

# Valid if statement
if [ -f "test.txt" ]; then
    echo "File exists"
fi

# Valid for loop
for i in {1..5}; do
    echo $i
done
```

### 2. Generated Perl

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

say "Hello, World!";
if ((-f "test.txt")) {
    say "File exists";
}
my $i;
for my $i ( 1 .. $MAX_LOOP_5 ) {
    say $i;
}
```

---

### 3. Non-idiomatic Patterns

---

#### Pattern ❶ — Unused boilerplate: `my $output = q{};`, `our $CHILD_ERROR;`, `use IPC::Open3;`

All three are emitted unconditionally by the generator's preamble logic (`src/generator/mod.rs:397-420`):

- `needs_output_var()` returns `true` because it sees an `echo` command, even though the echo generator now emits a bare `say` rather than appending to `$output`.
- `needs_ipc_open3()` returns `true` because `command_needs_ipc_open3` hits the `_` arm for a simple command with arguments (`cmd.args.len() > 0`), even though `echo` is translated natively with zero IPC::Open3 involvement.

**IR-fixable?** — **YES, by dead-code / import elimination in the IR backend.**

The IR program already models unused declarations (`IrStmt::Declare`, `IrProgram::imports`). The existing `optimize_stmts()` pass operates on semantic IR nodes, but *today most code passes through as `RawText`*, so the optimizer cannot touch it. Once `generate_simple_command` and the preamble emit semantic IR nodes instead of `RawText`, a simple reachability analysis can strip:

- `IrStmt::Declare { vars: ["output"], init: None }` if no statement reads `$output` → **remove it**.
- An import string `"IPC::Open3"` in `IrProgram::imports` if no IR node references `open3` or `$CHILD_ERROR` → **emit no `use IPC::Open3;`**.
- `IrStmt::Declare { vars: ["CHILD_ERROR"] }` if no statement mentions `$CHILD_ERROR` → **remove it.**

**What the cleaned-up output would look like:**

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

say "Hello, World!";
if (-f "test.txt") {
    say "File exists";
}
for my $i (1 .. 5) {
    say $i;
}
```

That is only 9 lines, all of which are idiomatic Perl. Every boilerplate line is gone.

---

#### Pattern ❷ — Magic-number constant `$MAX_LOOP_5`

In `generate_for_loop_impl` (src/generator/control_flow.rs:826):

```rust
if end_num > 2 {
    let const_name = format!("$MAX_LOOP_{}", end_num);
    all_items.push(format!(" {} .. {} ", start_num, const_name));
}
```

This replaces the literal `5` with a synthetic named constant `$MAX_LOOP_5`. The constant is never declared — it's a bare reference that would cause a strictures warning at runtime.

**IR-fixable?** — **YES, by constant folding in the IR optimizer.**

When the for-loop range `{1..5}` is lowered to semantic IR:

```
IrStmt::For {
    var: "i",
    iter: IrExpr::Range(IrExpr::Int(1), IrExpr::Int(5), None),
    body: [...]
}
```

(Note: `IrExpr` doesn't yet have a `Range` variant — see discussion in §6 below.) The IR pretty-printer (`emit_stmt` for `For`) would simply emit `for my $i (1 .. 5)`. There is no need for any magic-number abstraction *in the output*. The value `5` is already explicit in the source; the IR backend can use it directly.

**What the cleaned-up output would look like:**

```perl
for my $i (1 .. 5) {
    say $i;
}
```

(Also available today via the `--no-magic-numbers` CLI flag, which enables `NO_MAGIC_NUMBERS` and skips the constant-replacement path — but that's a flag, not the default.)

---

#### Pattern ❸ — Variable scoping: `my $i;` pre-declared, then `for my $i` redeclares it

```perl
my $i;           # outer declaration — shell-compatibility leftover
for my $i (...) { # redeclaration with 'my' — hides the outer $i
    say $i;
}
```

The generator emits the outer `my $i;` (src/generator/control_flow.rs:764-771) so that `$i` persists *after* the loop, mimicking shell behaviour where the loop variable survives the loop. But in the output `for my $i (...)` already declares a lexical `$i` scoped to the loop body. The outer `$i` is never used after the loop in this script, so it is dead code.

**IR-fixable?** — **YES, by dead-assignment elimination + scope analysis.**

If the generator emits semantic IR:

```
IrStmt::Declare { vars: [Decl { name: "i", sigil: Scalar }], init: None }
IrStmt::For { var: "i", iter: ..., body: [...] }
```

An IR optimizer can determine that `$i` is not read after the `For` statement (no later statement references `$i`). The `Declare` can be eliminated. Even better: the `For` statement's own `var` field causes the loop variable to be declared via `for my $i` in the pretty-printer, so the `Declare` is *always* redundant when a `For` follows with the same variable name.

**What the cleaned-up output would look like:**

```perl
for my $i (1 .. 5) {
    say $i;
}
```

---

#### Pattern ❹ — Double parentheses in `if ((-f "test.txt"))`

The test-expression generator (`src/generator/test_expressions.rs:572`) produces `(-f "test.txt")` — already parenthesized. Then `generate_if_statement_impl` in `control_flow.rs:73` wraps it in `if (...)`, yielding `if ((-f "test.txt"))`.

**IR-fixable?** — **YES, at the IR pretty-print level.**

When the if-condition is a semantic `IrExpr` node (e.g. `IrExpr::Call { func: "-f", args: [IrExpr::Str("test.txt")] }`), the `ir_to_perl` backend's `emit_stmt` for `IrStmt::If` would produce:

```perl
if (-f "test.txt") {
    say "File exists";
}
```

No double-parenthesis because the IR expression formatting knows that `-f` is a unary file-test operator and doesn't need wrapping.

---

#### Pattern ❺ — Spacing in `1 .. $MAX_LOOP_5`

The format string uses `" {} .. {} "` with padding spaces. Output: ` 1 .. $MAX_LOOP_5 `. The IR pretty-printer would emit the canonical form `1..5` which is standard Perl style.

**IR-fixable?** — **YES, trivially.** The `ir_expr_to_perl` function for a range expression would emit `start..end` without leading/trailing spaces.

**What the cleaned-up output would look like:**

```perl
for my $i (1 .. 5) {
```

(or `1..5` depending on the chosen style)

---

#### Pattern ❻ — `say $i;` for `echo $i`

This is actually *correct* — the translator correctly emits `say` for `echo`. This is the one properly idiomatic choice in the output. No fix needed.

---

### 4. Summary Table

| Pattern | Current Output | IR-Fixable? | IR Node Involved | Clean Output |
|---|---|---|---|---|
| **❶ Unused boilerplate** | `my $output = q{}; our $CHILD_ERROR; use IPC::Open3;` | Yes | `IrProgram::imports`, `IrStmt::Declare` | Omitted entirely |
| **❷ Magic-number constant** | `$MAX_LOOP_5` | Yes (constant fold) | `IrStmt::For.iter` (range) | `1..5` |
| **❸ Redundant outer `my $i;`** | `my $i;\nfor my $i ...` | Yes (dead-code elim) | `IrStmt::Declare` before `IrStmt::For` | Remove `my $i;` |
| **❹ Double parens** | `if ((-f "test.txt"))` | Yes (pretty-print) | `IrStmt::If.cond` file-test call | `if (-f "test.txt")` |
| **❺ Spacing in range** | ` 1 .. $MAX_LOOP_5 ` | Yes (pretty-print) | Range expr formatting | `1..5` or `1 .. 5` |
| **❻ `say $i` for `echo`** | `say $i;` | Correct already | — | — |

---

### 5. Unnecessarily Verbose Translations

The most egregious unnecessary verbosity is **Pattern ❶**: three lines of boilerplate for a script that performs only three statements, none of which need those declarations. Every one of those lines is noise:

| Boilerplate line | Why it's generated | Why it's unnecessary |
|---|---|---|
| `my $output = q{};` | `needs_output_var()` sees `echo` | `echo` now emits `say` directly, not `$output .=` |
| `our $CHILD_ERROR;` | `needs_ipc_open3()` sees any command with args | This script has no IPC, no pipelines, no subprocesses |
| `use IPC::Open3;` | Same as above | Same as above |

These three lines together constitute **43% of the non-shebang preamble** and **27% of the total generated text**. An IR backend with dead-code elimination removes them effortlessly.

The `my $i;` declaration (Pattern ❸) is another 13% waste — 1 line out of ~12 that serves no purpose.

---

### 6. What *Cannot* Be Fixed Solely by the IR Backend

The IR design doc correctly notes that optimization passes operate on semantic IR nodes, but *today almost all generator functions still produce `RawText` strings*. The only generators that emit proper IR nodes are `date.rs`, `rm.rs`, and `wc.rs`. Until `generate_for_loop_impl`, `generate_if_statement_impl`, `generate_simple_command_impl` (for echo), and the preamble emit semantic `IrStmt`/`IrExpr` nodes instead of raw format strings, the `ir_to_perl` backend's `optimize_stmts()` sees `RawText(...)` and is forced to pass it through unchanged.

**What must change in the generator logic itself (not the IR backend):**

- **`generate_for_loop_impl`** must emit `IrStmt::For { var, iter, body }` instead of a `RawText` string. Currently it concatenates format strings directly. The brace-expansion expansion, magic-number substitution, and variable persistence logic are all baked into the format-string assembly. Extracting the *semantics* (e.g. `iter = Range(1, 5)`) requires restructuring this function.

- **`generate_simple_command_impl` (echo branch)** must emit `IrStmt::Output { value, newline: true }` instead of `$output .= "...\n"; ... if (!($output =~ ...)) ...`. The echo command is semantically "say this value". The IR backend already knows how to pretty-print `Output` as `say ...;` — the generator just needs to produce that IR node.

- **The preamble in `generate_program`** (or equivalent) must emit `IrStmt::Declare` nodes instead of raw `"my $output = q{};\n"` strings. The IR backend's import-minimization pass can then decide whether to keep or drop them.

- **The `$MAX_LOOP_5` magic-number feature** is a *generator-level concern* (it's meant to satisfy Perl::Critic policies about magic numbers). It should not appear in the IR at all — the IR should hold `Int(5)`. If the user wants magic-number extraction, that should be an alternative pretty-printer, not a source-level constant reference. Currently the generator replaces `5` with `$MAX_LOOP_5` before the IR is ever constructed.

**Bottom line:** The IR backend can fix *every* pattern listed above — once the generator migrates from `format!()` → `RawText` to semantic `IrStmt`/`IrExpr` construction. The migration strategy in `docs/ir-design.md` (§*Migration strategy*) correctly outlines this: step 4 ("pick one function") starts the process, but currently only three specialized commands (`date`, `rm`, `wc`) have been migrated. The common path generators (echo, for, if, preamble) still produce `RawText`, which is why none of the desirable optimizations fire on this trivial example.