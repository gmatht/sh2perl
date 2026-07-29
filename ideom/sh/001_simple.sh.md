Let me look at the source scripts and the generator architecture.Let me examine the generator mod and understand the actual translation pipeline.Let me check a few more details about the generated code.Now I have all the information needed. Let me write the idiom review.Here is the idiom review.

---

## Idiom Review: `sh/001_simple.sh` → Generated Perl

### 1. Source and Generated Code

**Original shell script** (`sh/001_simple.sh`):
```bash
#!/bin/bash
echo "Hello, World!"
if [ -f "test.txt" ]; then
    echo "File exists"
fi
for i in {1..5}; do
    echo $i
done
```

**Generated Perl** (from `debashc -i ../sh/001_simple.sh`):
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

### 2. Non-Idiomatic Patterns

#### Pattern 1: Undefined variable `$MAX_LOOP_5` used instead of literal `5`

```perl
for my $i ( 1 .. $MAX_LOOP_5 ) {
```

`$MAX_LOOP_5` is not a Perl builtin or user-defined variable. The generator's `no_magic_numbers` flag (which defaults to `true`) suppresses the constant declaration (`my $MAX_LOOP_5 = 5;`), yet the for-loop generator still emits `$MAX_LOOP_5` in the range. The resulting Perl will produce `Use of uninitialized value` warnings and treat the range as `1 .. 0` (empty).

The idiomatic Perl is simply:
```perl
for my $i (1 .. 5) {
```

**IR-fixable? Yes.** In the `IrStmt::For` node, the `iter` field would be `IrExpr::BinOp { lhs: IrExpr::Int(1), op: Range, rhs: IrExpr::Int(5) }`. The pretty-printer for `IrStmt::For` would emit `for my $i (1 .. 5)` — no constants, no indirection. The `$MAX_LOOP_5` artifact only exists because the generator builds a string via `format!()` instead of constructing a semantic IR range node.

**Cleaned-up output:**
```perl
for my $i (1 .. 5) {
```

---

#### Pattern 2: Separate `my $i;` preceding `for my $i (...)`

```perl
my $i;
for my $i ( 1 .. $MAX_LOOP_5 ) {
```

The generator emits a standalone `my $i;` declaration before the loop "so the variable persists after the loop ends." But it then immediately uses `for my $i (...)` which re-declares `$i` lexically inside the loop. The outer `my $i;` is dead — it declares `$i` but the loop creates a *different* `$i` scoped to the `for`'s parenthesized iterator.

In the original shell script, `$i` is set to 5 after the loop (bash leaks the last iteration value). The script even comments this is messy and explicitly says `#PERL_MUST_NOT_CONTAIN: $i = 5;`. So the outer `my $i;` is unnecessary and violates the script's own annotation.

Idiomatic Perl:
```perl
for my $i (1 .. 5) {
    say $i;
}
# $i is not accessible here — and that's fine
```

**IR-fixable? Yes.** `IrStmt::For` with the optional `persist_var` flag. When the analysis shows the loop variable is not used after the loop, the pretty-printer omits the outer `my` declaration and emits `for my $i (1 .. 5)` — no separate declaration. When persist is needed, it emits:
```perl
my $i;
for $i (1 .. 5) { ... }   # no 'my' inside the for
```

The IR already has the `For { var, iter, body }` node; the decision on whether to declare with `my` inside or outside belongs in the IR emit phase based on a usage-analysis flag.

---

#### Pattern 3: `if ((-f "test.txt"))` — redundant double parentheses

```perl
if ((-f "test.txt")) {
```

Shell's `[ -f "test.txt" ]` is a command, so the generator perceives it as needing parentheses around the entire test expression. But Perl's `-f` is already a unary operator that takes a filename argument. The outer parentheses are unnecessary and look like a shell-ism transliterated literally.

Idiomatic Perl:
```perl
if (-f "test.txt") {
```

**IR-fixable? Yes.** The `IrStmt::If` node's `cond` would be `IrExpr::Call { func: "-f", args: [IrExpr::Str("test.txt", DoubleQuoted)] }`. The `emit_cond` function in `ir_to_perl()` controls parentheses. When the condition is a single function call or unary operator, no outer parens are needed beyond the mandatory `if (...)` syntax. The clean rule: emit `if (-f "test.txt")` not `if ((-f "test.txt"))`.

---

#### Pattern 4: `my $output = q{};` — unused variable

```perl
my $output         = q{};
```

No statement in this script ever reads or writes `$output`. It is dead code added by the generator's infrastructure (pipeline output capture scaffolding that this script never uses).

**IR-fixable? Yes.** An `IrProgram` with no `Pipeline` or `System` nodes that reference `$output` would not emit the declaration. The IR-based optimizer already has a `optimize_stmts()` pass that eliminates self-assignments; extending it with dead-variable elimination would drop this.

---

#### Pattern 5: `our $CHILD_ERROR;` — unused global variable

```perl
our $CHILD_ERROR;
```

No command substitution (`qx{}`), no `system()`, no pipeline capture occurs in this script. `$CHILD_ERROR` is never set or read.

**IR-fixable? Yes.** The IR program would contain no `IrStmt::SetChildError` nodes. The backend can check whether any statement references `$CHILD_ERROR` before emitting the `our` declaration. Same mechanism as the existing `stmt_refers_to_main_exit()` helper — just extend it for `$CHILD_ERROR`.

---

#### Pattern 6: `use IPC::Open3;` — unused import

```perl
use IPC::Open3;
```

No external command is spawned by this script. `IPC::Open3` is only needed for command substitution or external-process capture.

**IR-fixable? Yes.** The `IrProgram.imports` vec is derived from the IR nodes present. An `IrProgram` with no `System` nodes and no `Backtick` expressions would not include `"IPC::Open3"` in its imports.

---

### 3. Unnecessarily Verbose Translations (Prime IR Candidates)

These are places where a simple operation is wrapped in heavy infrastructure:

| Shell | Generated Perl | Problem | IR Fix |
|-------|---------------|---------|--------|
| `echo "Hello, World!"` | `say "Hello, World!";` | Actually clean — `say` is idiomatic | None needed |
| `echo $i` (inside loop) | `say $i;` | Also clean | None needed |
| `{1..5}` (brace expansion) | `1 .. $MAX_LOOP_5` | Inserts an undefined constant where a literal `5` suffices | `For { iter: Range(1, 5) }` → pretty-print as `1 .. 5` |
| `[ -f "test.txt" ]` | `if ((-f "test.txt"))` | Double-wraps in parens | `If { cond: Call("-f", ["test.txt"]) }` → emit `-f "test.txt"` without extra parens |
| *(no pipelines)* | `my $output = q{};` | Dead variable for pipeline capture that never happens | Omit when no Pipeline/System nodes exist |
| *(no subprocesses)* | `our $CHILD_ERROR;` | Dead variable | Omit when no SetChildError nodes exist |
| *(no backticks)* | `use IPC::Open3;` | Unused import | Omit when no System/Backtick nodes exist |
| `for i in {1..5}` | `my $i;\nfor my $i (...)` | Double declaration | Single `for my $i (1 .. 5)` when variable not used after loop |

---

### 4. Summary

**How many patterns are IR-fixable?** All 6 patterns are fixable by an IR-based backend because they involve decisions at the pretty-printing level:

| Pattern | IR Node(s) Involved | Fix Location |
|---------|-------------------|--------------|
| `$MAX_LOOP_5` | `IrStmt::For.iter` (a `BinOp::Range` node) | `emit_for()` — use literal `5` not a constant |
| Double `my $i` | `IrStmt::For.var` + usage analysis | `emit_for()` — skip outer `my` when var unused after loop |
| Double parens in `if` | `IrStmt::If.cond` | `emit_cond()` — don't add extra parens around unary/filetest ops |
| Unused `$output` | Whole-program analysis | Dead-variable elimination pass before emit |
| Unused `$CHILD_ERROR` | Whole-program analysis | Dead-variable elimination pass before emit |
| Unused `IPC::Open3` | `IrProgram.imports` | Emit imports based on IR nodes actually present |

**None of these require changing the generator logic** (the `generate_*` functions that walk the AST). They all require changes in a single `ir_to_perl()` backend or in optimization passes that run between IR construction and emission.

**The one subtle issue:** The `$MAX_LOOP_5` bug is actually a *generator bug* that the IR alone cannot paper over. The generator's `generate_for_loop_impl` writes `$MAX_LOOP_5` into the format string, and the `analyze_constants_needed` pass adds the constant to `self.constants`. But when `no_magic_numbers=true`, the constant declaration is skipped while the reference remains. In an IR-based world, the generator would produce `IrExpr::BinOp { lhs: Int(1), op: Range, rhs: Int(5) }` and the decision to use a named constant vs. a literal would happen in the pretty-printer, not in the AST walker. So the IR *does* fix this — by moving the formatting decision out of the generator entirely.