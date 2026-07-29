Now I have all the information needed. Let me write the comprehensive idiom review.

---

# Idiom Review: `examples/002_control_flow.sh` → Perl Translation

## 1. Source & Generated Code

**Original shell script (`examples/002_control_flow.sh`):**
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

**Generated Perl (from user prompt):**
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

**Hand-written idiomatic Perl (from `examples.pl/002_control_flow.pl`):**
```perl
#!/usr/bin/env perl

use strict;
use warnings;

# Control flow examples
if (-f "file.txt") {
    print "File exists\n";
} else {
    print "File does not exist\n";
}

for my $i (1..5) {
    print "Number: $i\n";
}

my $i = 1;
while ($i < 10) {
    print "Counter: $i\n";
    $i++;
}

sub greet {
    my ($name) = @_;
    print "Hello, $name!\n";
}

greet("World");
```

---

## 2. Non-Idiomatic Patterns

### Pattern A — Triple-redundant loop-variable scoping

**Generated:**
```perl
my $i;                    # (1) file-scoped declaration

for my $i (1..5) {        # (2) separate lexical $i inside loop
    say "Number: $i";
}
$i = 5;                   # (3) post-loop assignment to outer $i

while ( $i < 10 ) {       # uses outer $i — never set by the for loop
    ...
}
```

**Problem:** In bash, `for i in {1..5}` leaks the final value `5` into the surrounding scope. The generator simulates this by (1) pre-declaring a file-scoped `$i`, (2) using `for my $i` (which creates a *separate lexical variable* shadowing the outer one), and (3) manually assigning `$i = 5` after the loop to the outer `$i`. This is correct but verbose and confusing — any reader would wonder why `$i` is declared twice and why the loop variable doesn't leak naturally.

**Idiomatic Perl that preserves bash semantics:**
```perl
my $i;
for $i (1..5) {       # no "my" — reuses the outer $i
    say "Number: $i";
}
# $i is now 5 automatically, no post-loop assignment needed
while ( $i < 10 ) {
    ...
    $i++;
}
```

Dropping the `my` from the `for` loop makes the loop variable refer to the file-scoped `$i`, so it naturally retains its final value after the loop. The separate `$i = 5` becomes unnecessary.

---

### Pattern B — `eval { int(...) } // ""` as arithmetic increment

**Generated:**
```perl
$i = eval { int($i + 1) } // "";
```

**Problem:** The shell `i=$((i + 1))` is a simple integer increment. The generator wraps it in three layers of paranoia:
- `eval { ... }` — exception-catching block (for division-by-zero safety)
- `int(...)` — explicit integer cast
- `// ""` — defined-or fallback to empty string

This is **>30 characters** to express what should be **4 characters** in idiomatic Perl.

**Idiomatic Perl:**
```perl
$i++;
```
or less idiomatically:
```perl
$i += 1;
$i = $i + 1;
```

This `eval`-wrapper pattern appears because `convert_arithmetic_to_perl_impl` (in `src/generator/words.rs` at line 2998) unconditionally wraps every arithmetic expression in `eval { int(...) } // ""` as its final step (line 3178). It does not distinguish between a standalone assignment like `$i = $i + 1` and a potentially-dangerous expression like `$x / $y`.

---

### Pattern C — Declared but unused subroutine parameter

**Generated:**
```perl
sub greet {
    my ($file) = @_;
    say "Hello, $_[0]!";
    return;
}
```

**Problem:** The parameter `$file` is declared via `my ($file) = @_;` but is **never used** in the body. Instead, the body uses `$_[0]` directly. This is the worst of both worlds: you pay the verbosity of unpacking `@_` but get none of the readability benefit.

The root cause is in `generate_function_impl` (`src/generator/control_flow.rs`, around line 1156). When a function uses positional parameters (`$1`, `$2`) but has no named-parameter mapping from a `name=$1` pattern, the generator:
1. Emits `my ($file) = @_;` with the hardcoded name `file` for the first positional parameter
2. Post-processes the body by replacing `$1` → `$_[0]`, `$2` → `$_[1]`, etc. (line 1214–1221)

So the declared parameter name and the actual usage are deliberately disconnected. This is a generator logic flaw.

**Idiomatic Perl (option A — use the named parameter):**
```perl
sub greet {
    my ($name) = @_;
    say "Hello, $name!";
}
```

**Idiomatic Perl (option B — skip unpacking for simple subs):**
```perl
sub greet {
    say "Hello, $_[0]!";
}
```

**Idiomatic Perl (option C — use a modern signature):**
```perl
use feature 'signatures';
sub greet ($name) {
    say "Hello, $name!";
}
```

---

### Pattern D — Unnecessary trailing `return;`

**Generated:**
```perl
sub greet {
    ...
    return;
}
```

**Problem:** Perl subs automatically return the value of the last expression evaluated. An explicit `return;` at the end is noise. Perl::Critic may require it under certain policies, but it is non-idiomatic for simple value-returning subs.

**Idiomatic Perl:**
```perl
sub greet {
    my ($name) = @_;
    say "Hello, $name!";
}
```

---

### Pattern E — Extra blank line after `use`

**Generated:**
```perl
use feature 'say';


my $i;
```

**Problem:** Two blank lines after the `use` statement. Minor, but looks like a formatting glitch.

---

### Pattern F — Unnecessary `use feature 'say'` (debatable)

**Generated:**
```perl
use feature 'say';
```

While `say` is cleaner than `print "...\n"`, importing it just for two uses in a tiny script is slightly heavyweight. Either use `print "...\n"` everywhere (no import needed) or use `say` consistently. The hand-written example uses `print "...\n"` throughout, which is simpler.

---

### Pattern G — Spacing inconsistency in `while` condition

**Generated:**
```perl
while ( $i < 10 ) {
```

**Problem:** Spaces inside the parens where the `if` condition has none: `if (-f "file.txt")`. Minor inconsistency.

---

## 3. IR-Fixability Analysis

Per the IR design (`docs/ir-design.md`), the Perl IR decouples *what* to generate from *how to format it*. The `ir_to_perl()` backend controls all style decisions. Patterns that can be fixed by changing `ir_to_perl()` alone are **IR-fixable**. Patterns that require changing the generator (the code that builds IR nodes) are **NOT IR-fixable** with the current architecture.

| Pattern | IR-Fixable? | Why |
|---|---|---|
| **A** — `my $i;` + `for my $i` + `$i = 5` | **NO** | The IR's `IrStmt::For` always emits `for my $var`. There is no `for $var` (non-lexical) variant in the IR. The generator would need to emit a different IR node (or a modified `For` with a `lexical: bool` field). The post-loop `$i = 5` assignment is a separate `IrStmt::Assign` that the backend cannot know is redundant without whole-loop analysis. |
| **B** — `eval { int(...) } // ""` | **PARTIALLY** | If the generator were migrated to emit `IrStmt::Assign { targets: ["i"], expr: IrExpr::BinOp { lhs: Var("i"), op: Add, rhs: Int(1) } }` instead of `RawText`, the backend's compound-assignment optimization (already present in `emit_stmt` for `Assign`) would produce `$i += 1`. But the generator currently emits this as part of the for-loop body, which goes through `generate_block_commands` → string → `RawText`. Until the for-loop body is migrated to proper IR statements, this is a generator issue. **Fixable via IR migration of the for-loop body generator.** |
| **C** — Unused param + `$_[0]` | **NO** | The generator builds the sub body as a string, then replaces `$1` → `$_[0]` via post-processing. Even with IR, the generator would need to produce `IrSub { params: ["file"], body: [Output { value: Interpolate([..., Var("file"), ...]) }] }` — i.e., it must use the declared parameter name in the body expressions. The IR backend cannot invent the connection between a declared param and `$_[0]` usage. Generator logic must change. |
| **D** — Trailing `return;` | **YES** | The IR backend controls how `IrStmt::Return(None)` is printed. It could check whether this is the last statement in a sub and omit it. Currently `emit_sub` just emits all body statements verbatim. A simple check in `emit_sub` (or a suppression rule in `optimize_stmts`) could drop a trailing `return;`. **IR node involved:** `IrStmt::Return(None)` in `emit_sub`. |
| **E** — Extra blank line | **YES** | The IR backend controls inter-statement spacing. **IR node involved:** the header emission in `ir_to_perl()`. |
| **F** — `use feature 'say'` | **YES** | The backend controls the `imports` field and header emission. If the backend prefers `print "...\n"` over `say`, it can omit `use feature 'say'` and emit `print` for `Output { newline: true }`. **IR node involved:** `IrProgram::imports` and `IrStmt::Output`. |
| **G** — Spacing in `while` | **YES** | The backend controls expression formatting in `emit_stmt` for `While`. The spacing around `$i < 10` comes from `ir_expr_to_perl` for `BinOp { op: Lt, lhs: Var("i"), rhs: Int(10) }`, which currently renders with spaces around the operator. **IR node involved:** `IrExpr::BinOp` in `ir_expr_to_perl`. |

### Summary of IR-fixable patterns (and what the cleaned-up output would look like)

**Pattern D** — If `ir_to_perl()` suppresses trailing `return;`:
```perl
sub greet {
    my ($file) = @_;
    say "Hello, $_[0]!";
}
# no "return;" at the end
```

**Pattern F** — If the backend prefers `print` over `say`:
```perl
# no "use feature 'say';"
print "File exists\n";
print "Number: $i\n";
```

**Pattern G** — Consistent condition formatting:
```perl
while ($i < 10) {    # no extra spaces inside parens
```

---

## 4. Unnecessarily Verbose Translations

These are the top candidates for IR-based simplification — places where the generated code wraps simple operations in complex scaffolding:

### 🏆 Worst Offender: `$i = eval { int($i + 1) } // ""`

| Aspect | Generated | Preferred |
|---|---|---|
| Characters | ~32 | 4 (`$i++`) |
| Control structures | `eval { }` block + `//` operator | None |
| Function calls | `int(...)` | None |
| Mental model | "catch exception, cast to int, fall back to empty string" | "increment" |

This is the clearest example of line-by-line transliteration. The shell `$((i + 1))` is a built-in arithmetic construct. The generator treats it as a dangerous operation needing exception handling. In reality, `$i + 1` in Perl is already safe — there is nothing that can throw an exception in integer addition. The `eval` and `// ""` are cargo-cult safety from division expressions.

**Root cause in generator:** `convert_arithmetic_to_perl_impl` (line 3178 of `words.rs`) unconditionally wraps: `format!("eval {{ int({}) }} // \"\"", result)`. This treats all arithmetic the same, whether it's `$i + 1` or `$x / $y`.

**IR fix:** If the arithmetic expression were emitted as `IrExpr::BinOp` instead of `RawExpr`, the backend would produce `$i + 1`. And the compound-assignment optimization in `emit_stmt` for `Assign` would further reduce it to `$i += 1`:

```rust
// Already in ir_to_perl()'s Assign handler:
IrExpr::BinOp { lhs: Var("i"), op: Add, rhs: Int(1) }
    → "$i += 1"
```

### 🥈 Runner Up: The `my $i;` / `for my $i` / `$i = 5` Triplet

| Aspect | Generated | Preferred |
|---|---|---|
| Statements | 3 (decl + for-with-my + post-assign) | 1 (`for $i (1..5)`) |
| Scopes | 2 (file scope + loop scope) | 1 (file scope) |
| Cognitive load | "which $i am I reading?" | "one $i throughout" |

The generator's pre-analysis (`analyze_variable_usage` in `mod.rs`) correctly detects that `$i` is used after the for loop and marks it as a `function_level_var`. Then `generate_for_loop_impl` (control_flow.rs) adds the outer `my $i;` and the post-loop `$i = end_num;`. But the `For` IR node always emits `for my $i`, creating a second lexical scope.

**IR fix:** This requires adding a `lexical` flag to `IrStmt::For`:
```rust
For { var: String, iter: IrExpr, body: Vec<IrStmt>, lexical: bool },
```
and changing `emit_stmt` to:
```rust
IrStmt::For { var, iter, body, lexical } => {
    let iter_str = ir_expr_to_perl(iter);
    if *lexical {
        out.push_str(&format!("for my ${} ({}) {{\n", var, iter_str));
    } else {
        out.push_str(&format!("for ${} ({}) {{\n", var, iter_str));
    }
    ...
}
```

With this change, the generator would emit `lexical: false` when it already holds an outer declaration, making the post-loop assignment naturally correct.

### 🥉 Third Place: Subroutine Parameter Dance

| Aspect | Generated | Preferred |
|---|---|---|
| Lines | 3 (decl, body with `$_[0]`, return) | 2 (no unpacking, body with `$_[0]`, or fewer if using param) |
| Unused code | `$file` declared but never referenced | Zero unused code |

The generator expends effort to declare a parameter name, then deliberately doesn't use it. Either commit to the name (replace `$_[0]` with `$file` in the body) or skip the unpacking entirely.

**IR fix:** The generator must change to produce either:
- `IrSub { params: ["name"], body: [Output { value: Interpolate([Lit("Hello, "), Expr(Var("name")), Lit("!")]) }] }` → `say "Hello, $name!"`
- Or `IrSub { params: [], body: [...] }` with bare `$_[0]` usage.

The IR backend already handles both correctly — the problem is upstream in the generator.

---

## 5. Recommendations for the IR-Based Backend

**Priority 1: Migrate arithmetic to `IrExpr::BinOp`.** Convert `convert_arithmetic_to_perl_impl` to produce `IrExpr` nodes instead of text. The backend already handles `BinOp` correctly and has compound-assignment optimization. This single migration eliminates the `eval { int(...) } // ""` verbosity.

**Priority 2: Add `lexical` flag to `IrStmt::For`.** The IR currently forces `for my $var` on all for loops. Adding a `lexical: bool` field lets the generator choose between `for my $var` (new scope) and `for $var` (reuse outer variable). This eliminates the triple-redundancy pattern.

**Priority 3: Suppress trailing `return;` in subs.** A one-line change in `emit_sub` or `optimize_stmts` to drop `IrStmt::Return(None)` when it's the last statement in a sub body.

**Priority 4: Fix parameter name propagation in the generator.** This is not IR-fixable, but it's the most obvious "line-by-line transliteration" smell. The generator should use the declared parameter name consistently instead of falling back to `$_[0]` after unpacking.