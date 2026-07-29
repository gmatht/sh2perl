Now let me also check the actual generated output more carefully:Let me check the arithmetic conversion and test expression generation to understand the `eval { int(...) }` wrapper:Now I have a comprehensive understanding of both the generator code and the IR infrastructure. Here's the full idiom review.

---

# Idiom Review: `sh/002_control_flow.sh` → Perl Translation

## 1. Original Shell Script

```bash
#!/bin/bash

# Control flow examples
if [ -f "file.txt" ]; then
    echo "File exists"
else
    echo "File does not exist"
fi

for i in {1..5}; do
    echo "Number: $i"
done

while [ $i -lt 10 ]; do
    echo "Counter: $i"
    i=$((i + 1))
done

function greet() {
    echo "Hello, $1!"
}

greet "World"
```

## 2. Generated Perl Code

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';


my $i;

if (-f "file.txt") {
    say "File exists";
}
else {
    say "File does not exist";
}
for my $i (1..5) {
    say "Number: $i";
}
$i = 5;
while ( $i < 10 ) {
    say "Counter: $i";
    $i = eval { int($i + 1) } // "";
}

sub greet {
    my ($file) = @_;
    say "Hello, $_[0]!";
    return;
}
greet("World");
```

---

## 3. Non-Idiomatic Patterns

### Pattern A: `$i = eval { int($i + 1) } // "";` — arithmetic wrapped in defensive eval

**Generated:**
```perl
$i = eval { int($i + 1) } // "";
```

**Preferred:**
```perl
$i += 1;
```
or at minimum:
```perl
$i = $i + 1;
```

**Analysis:** The shell `i=$((i + 1))` is a simple arithmetic increment. The generator wraps it in `eval { int(EXPR) } // ""`, an artifact of a generic arithmetic-to-Perl converter designed to catch division-by-zero errors. For a trivial addition, this is absurdly defensive — it introduces a runtime eval block and a defined-or fallback for an operation that cannot fail.

**IR-fixable? YES.** 

The IR already has the machinery to fix this. The generator currently emits this as raw text (bypassing the IR). If instead it produced:
```rust
IrStmt::Assign {
    targets: [AssignTarget { var: "i", sigil: Sigil::Scalar, indices: [] }],
    expr: IrExpr::BinOp {
        lhs: Box::new(IrExpr::Var("i", Sigil::Scalar)),
        op: BinOpKind::Add,
        rhs: Box::new(IrExpr::Int(1)),
    },
}
```

Then the existing compound-assignment detection in `emit_stmt` (in `src/ir.rs` lines ~227–253) would automatically emit `$i += 1;`. The `eval { int(...) } // ""` wrapper is eliminated entirely because the IR backend has no knowledge of it — it just formats the semantic IR node.

**IR node involved:** `IrStmt::Assign` with `IrExpr::BinOp` containing `BinOpKind::Add`.

**Which generator function needs migration:** The `convert_arithmetic_to_perl` function in `src/generator/words.rs` currently returns a `String` (wrapped in eval + int). It needs to return an `IrExpr` instead, so the calling context can embed it in an `IrStmt::Assign`.

---

### Pattern B: Named parameter declared but `$_[0]` used instead

**Generated:**
```perl
sub greet {
    my ($file) = @_;
    say "Hello, $_[0]!";
    return;
}
```

**Analysis:** The function declares `$file` as a named parameter but then ignores it, using `$_[0]` instead. This is contradictory to a human reader — is the parameter `$file` or is the function indexing into `@_` directly? It looks like the generator recognizes the function has a `$1` reference in its body, unpacks `@_` into `$file`, but then also does a blanket `$N → $_[N-1]` substitution that replaces the original `$1` with `$_[0]`. The result is two different parameter-access mechanisms in the same function.

**IR-fixable? PARTIALLY (needs generator logic change).**

The issue originates in `generate_function_impl` (`src/generator/control_flow.rs`). When the function uses positional params like `$1`, the generator does two things:
1. Emits `my ($file) = @_;` (from hard-coded fallback in the "uses positional params && no param map" branch)
2. Post-processes the body replacing `$1` → `$_[0]`, `$2` → `$_[1]`, etc.

Step 2 happens *after* step 1's output, so the body sees `$_[0]` instead of `$file`. The generator should emit the body using the named parameter `$file` when it has declared it. Specifically, when the body is being generated, variable references to positional parameters (like `$1`) should map to the named parameter (like `$file`), not to `$_[0]`.

This is **not** something a pure IR backend can fix, because the IR backend sees whatever strings or expression nodes it's given. If the generator passes `IrExpr::RawExpr("$_[0]")`, the backend prints `$_[0]`. If the generator passes `IrExpr::Var("file", Sigil::Scalar)`, the backend prints `$file`. The fix must happen in the generator: it should emit `IrExpr::Var("file")` when it knows `$1` maps to `$file`.

That said, an IR optimization pass *could* detect this pattern: if a subroutine has parameters `["file"]` and the body references `$_[0]`, a pass could rewrite `$_[0]` → `$file`. But this is fragile (what if the function actually needs `$_[0]` for other purposes?). The clean fix is in the generator.

---

### Pattern C: `return;` at end of every subroutine

**Generated:**
```perl
sub greet {
    ...
    return;
}
```

**Analysis:** In Perl, a subroutine automatically returns the value of the last evaluated expression. An explicit `return;` at the end is redundant. Worse, `return;` without an argument returns `undef` in scalar context and `()` in list context, which may silently change the subroutine's return behavior. The `say` statement already returns `1` on success, which is a reasonable implicit return value.

The generator emits this because of a comment: "Add final return statement to satisfy Perl::Critic." This is cargo-culting — Perl::Critic's `RequireExplicitReturn` policy only requires `return` statements *to return values from a sub*, not an empty `return;` at the end. And many Perl style guides (including `perlstyle`) explicitly say the final statement's value is the return value.

**IR-fixable? YES.**

The IR backend's `emit_sub` function can simply omit `IrStmt::Return(None)` when it's the last statement in the body. A simple optimization pass or conditional in `emit_sub`:

```rust
// In emit_sub, before emitting the last statement:
if let Some(IrStmt::Return(None)) = body.last() {
    // Omit trailing empty return — it's redundant
    body.pop();
}
```

**IR node involved:** `IrStmt::Return(None)` — the backend can strip this during emission.

**Cleaned-up output:**
```perl
sub greet {
    my ($file) = @_;
    say "Hello, $_[0]!";
}
```

(Note: the `$_[0]` issue is separate, but even with `$file` fixed, the `return;` removal is clean.)

---

### Pattern D: Extra blank line after `use feature 'say';`

**Generated:**
```perl
use feature 'say';


my $i;
```

**Analysis:** Two blank lines between the last `use` statement and the first declaration. This is cosmetic but sloppy.

**IR-fixable? YES.**

The `ir_to_perl()` function controls spacing. The current code adds a blank line after imports, but the top-level variable declarations that follow create another gap. The backend should collapse consecutive blank lines.

**IR node involved:** None — this is purely in the `ir_to_perl()` output formatting.

**Cleaned-up output:**
```perl
use feature 'say';

my $i;
```

---

### Pattern E: Spaces inside while-condition parentheses

**Generated:**
```perl
while ( $i < 10 ) {
```

**Preferred:**
```perl
while ($i < 10) {
```

**Analysis:** Extra spaces after `(` and before `)` are non-idiomatic in Perl. The old generator hard-codes this in `generate_while_loop_impl`:
```rust
output.push_str(&format!("{} ( ", loop_keyword));
...
output.push_str(" ) {\n");
```

**IR-fixable? YES.** 

The IR backend's `emit_stmt` for `IrStmt::While` already produces clean output without these spaces:
```rust
out.push_str(&format!("while ({}) {{\n", cond_str));
```

Once the while-loop generator is migrated to emit `IrStmt::While` instead of raw text, this is automatically fixed.

**IR node involved:** `IrStmt::While` — whose `emit_stmt` handler already uses no extra spaces.

---

### Pattern F: `else` on its own line instead of `} else {`

**Generated:**
```perl
}
else {
```

**Preferred (per `perlstyle`):**
```perl
} else {
```

**Analysis:** Perl community style overwhelmingly prefers `} else {` on one line. The current generator emits `}\nelse {` which reads like a line-by-line transliteration of shell's `else` keyword placement.

**IR-fixable? YES.** 

The IR backend's `emit_stmt` for `IrStmt::If` already emits `} else {\n` on one line (see `src/ir.rs`):
```rust
emit_indent(out, indent);
out.push_str("} else {\n");
```

Wait — actually this uses `emit_indent` before `}` which would put the `}` at the indent level. But the `}` is closing the `if` block, so it should be at the `if`'s indent level, not indented further. Let me re-check...

Looking at the IR code more carefully:
```rust
IrStmt::If { cond, then, elsifs, else_ } => {
    let cond_str = ir_expr_to_perl(cond);
    emit_indent(out, indent);
    out.push_str(&format!("if ({}) {{\n", cond_str));
    for s in then {
        emit_stmt(out, s, indent + 1);
    }
    for (econd, ebody) in elsifs {
        let estr = ir_expr_to_perl(econd);
        emit_indent(out, indent);
        out.push_str(&format!("}} elsif ({}) {{\n", estr));
        for s in ebody {
            emit_stmt(out, s, indent + 1);
        }
    }
    if !else_.is_empty() {
        emit_indent(out, indent);
        out.push_str("} else {\n");
        for s in else_ {
            emit_stmt(out, s, indent + 1);
        }
    }
    emit_indent(out, indent);
    out.push_str("}\n");
}
```

So the IR backend does: `emit_indent(out, indent)` + `"} else {\n"`. If `indent` is 0, this produces `} else {\n` (since `emit_indent` at indent 0 emits nothing). That's the correct closing brace at the outer level. ✓

Actually, I realize this IR `} else {\n` code would emit `} else {` on a SINGLE line, not `}\nelse {`. So once the if-statement generator uses the IR, the `else` placement is fixed. Good.

---

### Pattern G: `for my $i (1..5)` combined with `my $i;` and `$i = 5;` — triple variable management

**Generated:**
```perl
my $i;

for my $i (1..5) {
    say "Number: $i";
}
$i = 5;
```

**Analysis:** The loop variable `$i` is introduced in three different ways:
1. `my $i;` — declared at top level (outer scope)
2. `for my $i (1..5)` — redeclared as a new lexical inside the loop header (inner scope)
3. `$i = 5;` — assigned to the outer `$i` after the loop

This replicates shell semantics where `$i` persists after the loop with the last value. But the Perl scoping is confusing: the `for my $i` creates a *new* lexical `$i` scoped to the loop body, so the post-loop assignment `$i = 5` sets the outer `$i`, while the loop body used the inner `$i`. It works correctly, but it's confusing to read — the post-loop `$i` is a different variable than the loop iteration variable.

**IR-fixable? NO (semantic requirement).**

Shell semantics require loop-variable persistence. The three-part declaration is a correct but verbose way to achieve it. This is a fundamental mismatch between shell's flat variable namespace and Perl's lexical scoping.

However, the IR could produce a **cleaner equivalent** if the generator chose a different scoping strategy. Instead of `my $i;` + `for my $i (1..5)` + `$i = 5`, it could use:

```perl
my $i;
for $i (1..5) {  # no 'my' — aliases the outer $i
    say "Number: $i";
}
# $i is already 5 here — no post-loop assignment needed
```

This uses `for $i` (without `my`) which iterates over the outer `$i` directly, and `$i` naturally retains the last value. This is both cleaner and more faithful to shell semantics. The IR `IrStmt::For` currently always emits `for my ${var}`, but it could be extended to optionally omit the `my` when the variable is already in scope.

**IR node involved:** `IrStmt::For` — the `var` field is present, but the backend always inserts `my`. Adding a `declare: bool` field (or checking scope) would allow `for $i (...)` without `my`.

**Cleaned-up output (with IR scope tracking):**
```perl
my $i;
for $i (1..5) {
    say "Number: $i";
}
```

---

## 4. Unnecessarily Verbose Translations

These are places where the generated code wraps simple operations in elaborate control structures, making the Perl read like a line-by-line transliteration rather than native Perl.

### Verbosity #1: `eval { int($i + 1) } // ""` for a simple increment (Pattern A above)

**Why it's verbose:** The entire `eval { int(EXPR) } // ""` infrastructure is designed to protect against division-by-zero in arbitrary arithmetic expressions. For `$i + 1`, there is no division, no string manipulation, no edge case. The defensive wrapper contributes runtime overhead (eval block, defined-or check) and visual noise for zero benefit.

The shell `$((i + 1))` is a simple integer addition. The Perl equivalent is `$i + 1`, or better `++$i` or `$i += 1`.

**IR simplification:** When the generator emits `IrExpr::BinOp { lhs: Var("i"), op: Add, rhs: Int(1) }`, the IR backend prints `$i + 1`. When it's inside an `IrStmt::Assign` where target equals the LHS variable, the compound-assignment optimizer emits `$i += 1`. The `eval { int(...) }` disappears entirely.

### Verbosity #2: `my ($file) = @_;` + `say "Hello, $_[0]!";` — unused parameter + raw @_ access (Pattern B above)

**Why it's verbose:** Two different parameter-access mechanisms in the same function. The `my ($file) = @_;` declaration introduces a name that is never used. Then `$_[0]` is used directly, which is the same value. This is like saying "let x = y; print y" — why introduce the name at all?

**IR simplification:** If the generator emits `IrExpr::Var("file", Scalar)` instead of `$_[0]`, the body becomes `say "Hello, $file!";` and the parameter unpacking is coherent.

### Verbosity #3: `for my $i (1..5)` with separate `my $i;` and `$i = 5;` (Pattern G above)

**Why it's verbose:** Three separate mechanisms to manage one variable. The shell simply has `for i in {1..5}; do ... done` and `$i` is `5` after the loop. The generated Perl uses (a) a top-level `my`, (b) a re-declaring `for my`, and (c) a post-loop assignment. This is the single most confusing pattern in the output — it looks like the code doesn't know which `$i` it's working with.

**IR simplification:** The IR `IrStmt::For` currently always emits `for my $var`. If the generator tracked whether `$var` is already declared in the enclosing scope, it could emit `for $var` (without `my`), which aliases the outer variable. This eliminates both the top-level `my $i;` (if nothing else uses it) and the post-loop assignment (since `$i` naturally retains its last iteration value).

### Verbosity #4: `return;` at end of sub (Pattern C above)

**Why it's verbose:** Perl doesn't require `return;` at the end. The `say` statement returns `1`, which is a perfectly fine implicit return value. Adding `return;` turns a successful `1` into `undef`/`()`, which could surprise callers. It's defensive boilerplate that harms rather than helps.

**IR simplification:** The IR backend can simply omit trailing `IrStmt::Return(None)` during emission.

---

## 5. Summary Table

| # | Non-Idiomatic Pattern | IR-Fixable? | IR Node | Cleaned Output |
|---|---|---|---|---|
| A | `eval { int($i + 1) } // ""` for simple addition | YES — emitter sees semantic BinOp | `IrStmt::Assign` + `IrExpr::BinOp(Add)` | `$i += 1;` |
| B | `$_[0]` used despite named param `$file` | PARTIAL — generator must emit `Var("file")` | `IrExpr::Var("file")` | `say "Hello, $file!";` |
| C | Trailing `return;` in sub | YES — backend can omit final `Return(None)` | `IrStmt::Return(None)` | (removed) |
| D | Extra blank lines | YES — `ir_to_perl()` controls spacing | N/A (formatting) | Single blank line |
| E | Spaces inside `( $i < 10 )` | YES — IR `While` emitter has no padding | `IrStmt::While` | `while ($i < 10) {` |
| F | `}\nelse {` instead of `} else {` | YES — IR `If` emitter does one line | `IrStmt::If` | `} else {` |
| G | Triple `$i` management (my + for my + =5) | NO — needs generator to choose scoping | `IrStmt::For` (add `declare: bool`) | `for $i (1..5) {` (no post-assign) |

**Verbosity ranking** (most impactful to fix first):

1. **`eval { int(...) } // ""`** — The single biggest readability problem. Makes every arithmetic operation look like a fragile operation that might fail.
2. **`return;` at end of sub** — 50+ character noise in every function, with actual semantic side effects (changes return value from truthy to undef).
3. **Named param declared but `$_[0]` used** — Confuses code reviewers and suggests the generator doesn't trust its own output.
4. **Triple `$i` management** — The most architecturally verbose pattern; fixing it requires the deepest change but also yields the cleanest result.
5. **Spacing and formatting** — Cosmetic, but collectively make the code look machine-generated rather than human-written.

---

## 6. Recommended Fix Order (for a generator engineer)

1. **Migrate `convert_arithmetic_to_perl` to return `IrExpr`** instead of `String`. This eliminates the `eval { int(...) } // ""` wrapper for all arithmetic expressions. The IR backend's compound-assignment optimization then naturally produces `$i += 1` from `$i = $i + 1`. This is the highest-value change.

2. **Fix the `return;` emission** by either (a) stopping generator from emitting it, or (b) having the IR backend strip trailing `Return(None)`. Option (b) is cleaner because it centralizes the style decision.

3. **Fix the parameter naming** by changing `generate_function_impl` to emit `$file` instead of `$_[0]` when a named parameter exists. The post-processing step that does blanket `$N → $_[N-1]` replacement should instead do `$N → ${param_name}` when the param map is available.

4. **Add a `declare: bool` field to `IrStmt::For`** to allow `for $i (...)` without `my` when the variable is already declared in scope. This eliminates the post-loop persistence assignment and makes the scoping transparent.

5. **Fix spacing/formatting** in the IR backend's `ir_to_perl()` — these are one-line changes to `emit_stmt` for `While`, `If`, and the blank-line logic in `ir_to_perl()`.