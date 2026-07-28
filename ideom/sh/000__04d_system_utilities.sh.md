Let me first examine the actual generated code more carefully by running the translator:Now I have a thorough understanding of the codebase. Let me compile the idiom review.

---

# Idiom review: sh/000__04d_system_utilities.sh

## Source

```bash
echo "=== System Utilities ==="
formatted_date=`date '+%Y-%m-%d'`
echo "Formatted date: $formatted_date"
sleep_duration=`echo "1"`
echo "Sleeping for $sleep_duration seconds..."
sleep $sleep_duration
yes_result=`yes "Hello" | head -3`
echo "Yes command result:"
echo "$yes_result"
echo "=== System Utilities Complete ==="
```

## Generated Perl

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

say "=== System Utilities ===";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
say "Formatted date: $formatted_date";
my $sleep_duration = ("1");
say "Sleeping for $sleep_duration seconds...";
require Time::HiRes; Time::HiRes::sleep($sleep_duration);
my $yes_result = do {
    do { my $output_1 = q{};
my $output_printed_1;
my $head_line_count = 0;
while (1) {
    my $line = 'Hello';
    if ($head_line_count < 3) {
    $output_1 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{};
say "Yes command result:";
say $yes_result;
say "=== System Utilities Complete ===";
}
}
}
}
```

## Idiom issues

| # | Pattern | Generated code | Idiomatic Perl | IR-fixable? |
|---|---------|---------------|----------------|-------------|
| 1 | `date` backtick → `do { require POSIX; ... }` | `my $formatted_date = do {\nrequire POSIX; POSIX::strftime(...)\n};` | `use POSIX qw(strftime); my $formatted_date = strftime('%Y-%m-%d', localtime());` | Yes |
| 2 | `echo "1"` backtick → parenthesized literal | `my $sleep_duration = ("1");` | `my $sleep_duration = "1";` | Yes |
| 3 | Inline `require Time::HiRes` + FQ call | `require Time::HiRes; Time::HiRes::sleep($sleep_duration);` | Move `use Time::HiRes;` to file header with other imports; call as `sleep($sleep_duration);` | Yes |
| 4 | `yes \| head -3` pipeline → manual `while` loop with counter | 20+ lines of `while(1) { my $line = 'Hello'; if ($head_line_count < 3) { ... } else { ... } }` | `my $yes_result = join '', ("Hello\n") x 3;` | Yes (but see caveat) |
| 5 | Stale/unused variable declarations | `my $output = q{};` and `my $output_printed_1;` | Neither variable is ever read after assignment. Eliminate them. | Yes |
| 6 | Unused import `IPC::Open3` | `use IPC::Open3;` | Remove import — no `open3` call is emitted. | Yes |
| 7 | Broken `else` branch absorbs rest of script | Inside `head`-limit else: `$line = q{};` followed by remaining `say` statements | The `else` branch should simply `last;` to exit the infinite `yes` loop, and the post-substitution statements should follow outside the `do` block. | No (generator logic bug) |
| 8 | Double-wrapped `do { do { ... } }` | `my $yes_result = do { do { ... } }` | Single `do { ... }` or just the expression directly. | Yes |

---

## IR-fixability details

### Issue 1: `date` backtick → `do { require POSIX; ... }`

**IR-fixable: Yes**

The generator currently emits raw text for the `date` special-case in `generate_pipeline_for_substitution`. This should produce an `IrStmt::Declare` with `IrExpr::Call { func: "POSIX::strftime", args: [...] }`.

**IR node involved:**
- `IrStmt::Declare { vars: [Decl { name: "formatted_date", sigil: Scalar }], init: Some(IrExpr::Call { func: "POSIX::strftime", args: [...] }) }`
- Import tracking via `IrProgram::imports` → `use POSIX qw(strftime);`

**Cleaned-up output:**
```perl
use POSIX qw(strftime);
my $formatted_date = strftime('%Y-%m-%d', localtime());
```

The `ir_to_perl()` backend would:
1. Collect all `Call` nodes and emit their parent module in `use` statements at the top.
2. Strip the redundant `do { }` wrapper because a `Declare` with an `init` expression doesn't need one.
3. Use the short function name when the import explicitly lists it (`strftime` not `POSIX::strftime`).

---

### Issue 2: `echo "1"` backtick → parenthesized literal

**IR-fixable: Yes**

The shell expression `` `echo "1"` `` evaluates to the constant string `"1"`. The generator already inlines this to `("1")` but keeps the parentheses from some formatting template.

**IR node involved:**
- `IrStmt::Declare { vars: [Decl { name: "sleep_duration", sigil: Scalar }], init: Some(IrExpr::Str("1", SingleQuoted)) }`

**Cleaned-up output:**
```perl
my $sleep_duration = "1";
```

The `ir_to_perl()` backend would emit `IrExpr::Str` with `SingleQuoted` style as `'1'` or `"1"` without wrapping parentheses, since `Declare` already provides the `=` syntax.

---

### Issue 3: Inline `require Time::HiRes`

**IR-fixable: Yes**

Inline `require` is a symptom of the generator emitting code statement-by-statement rather than planning imports. The IR already tracks imports in `IrProgram::imports`.

**IR node involved:**
- The `sleep` command maps to `IrStmt::System { cmd: IrExpr::RawExpr("sleep"), ... }` or a dedicated `IrStmt::Sleep { duration: IrExpr }`.
- Imports are accumulated in `IrProgram::imports` and emitted at the top by `ir_to_perl()`.

**Cleaned-up output:**
```perl
use Time::HiRes qw(sleep);
say "Sleeping for $sleep_duration seconds...";
sleep($sleep_duration);
```

The backend would:
1. Scan all statements for required modules (Time::HiRes detected via the `sleep` call or System node).
2. Emit `use Time::HiRes qw(sleep);` in the header block.
3. Emit the bare `sleep(...)` call instead of the fully-qualified form.

---

### Issue 4: `yes | head -3` pipeline → manual `while` loop

**IR-fixable: Partially**

The `yes` command with `head -n N` is semantically equivalent to repeating a string N times. An IR-based optimizer *could* recognize this pattern if the IR preserves enough semantic information.

**Current IR path:**
- The pipeline generates `IrStmt::Pipeline { stages: [...] }` with a `while(1)` + counter in stage 1 and the head-check in stage 2.
- The backend iterates over the raw stages and emits them verbatim — no optimization.

**What would be needed:**
An IR optimization pass that pattern-matches:
```
Pipeline(
  While(Infinite, { Append($output, $line) }),
  Filter(Head(3))
)
```
and rewrites it to:
```
IrExpr::Str("Hello\n" repeated 3 times)
```

**IR node after optimization:**
```rust
IrStmt::Declare {
    vars: [Decl { name: "yes_result", sigil: Scalar }],
    init: Some(IrExpr::Call {
        func: "join",
        args: [
            IrExpr::Str("", DoubleQuoted),
            IrExpr::Call {
                func: "map",
                args: [
                    IrExpr::Str("Hello\n", DoubleQuoted),
                    IrExpr::Range(1, 3),  // hypothetical
                ],
            },
        ],
    }),
}
```

**Cleaned-up output:**
```perl
my $yes_result = join '', map { "Hello\n" } 1..3;
```
or even simpler for this specific case:
```perl
my $yes_result = ("Hello\n") x 3;
```

**Caveat:** This optimization requires a *semantic* IR pass (not just pretty-printing), so it crosses from "IR backend style fix" into "IR optimizer transformation." The backend alone (`ir_to_perl()`) cannot do this — it would need a MIR-like transform layer between the generator and the backend. The IR design doc mentions this as a future goal ("Optimization passes as MIR transforms").

---

### Issue 5: Stale/unused variable `$output` and `$output_printed_1`

**IR-fixable: Yes**

These are emitted unconditionally by the pipeline infrastructure even when no output-consuming code follows.

**IR node involved:** Dead-variable elimination in the IR optimizer. After generating `IrProgram`, a pass would scan all `IrStmt::Assign`, `IrStmt::System { capture: Some(var) }`, etc., and remove any variable that is never referenced in any subsequent statement or expression.

**Cleaned-up output:** The variables simply disappear.

---

### Issue 6: Unused `use IPC::Open3`

**IR-fixable: Yes**

This is emitted unconditionally in the file header. The IR backend tracks actual usage.

**IR node involved:** `IrProgram::imports` — the backend only emits imports that are actually referenced by `IrExpr::Call` nodes or `IrStmt::System` nodes that use `open3`.

**Cleaned-up output:** `use IPC::Open3;` is absent from the output when no `open3()` call is generated.

---

### Issue 7: Broken `else` branch absorbs rest of script

**IR-fixable: No**

This is a **generator logic bug**, not a pretty-printing issue. The generator's `yes`-pipeline special-case in `pipeline_commands.rs` produces an infinite `while(1)` loop, and the `head` line-by-line generator emits code like:

```rust
if ($head_line_count < {num_lines}) {
    $output_1 .= $line . "\n";
    ++$head_line_count;
} else {
    last;  // should be "last;" but generator emits wrong content
}
```

The actual generated output shows `$line = q{};` in the else branch followed by all subsequent script statements, indicating that the generator is not properly scoping the pipeline block and leaking the rest of the script's translation into the `else` body.

This is a **control-flow nesting error** in `generate_pipeline_for_substitution` or its caller in `words.rs`. The IR backend can only format what it receives; if the generator produces an `IrStmt::RawText` containing the broken `else` block, the IR has no way to fix it.

**Fix required in generator logic:** The `CommandSubstitution` handler needs to close the `do` block *before* emitting the statements that follow the backtick assignment in the original script.

---

### Issue 8: Double-wrapped `do { do { ... } }`

**IR-fixable: Yes**

The outer `do { ... }` wrapper is added by the `CommandSubstitution` handler in `words.rs`:
```rust
format!("do {{ local $CHILD_ERROR = 0; {}; }}", pipeline_code)
```

The inner `do { ... }` is produced by the pipeline generator itself. The result is nested `do { do { ... } }`.

**IR node involved:** When the pipeline generator produces an `IrExpr::Backtick` or `IrStmt::Declare` with a capture expression, the outer `CommandSubstitution` handler should recognize that the inner expression is already self-contained and not wrap it again.

**Cleaned-up output:** Just the inner expression:
```perl
my $yes_result = do {
    my $output_1 = q{};
    my $head_line_count = 0;
    while (1) {
        my $line = 'Hello';
        if ($head_line_count < 3) {
            $output_1 .= $line . "\n";
            ++$head_line_count;
        } else {
            last;
        }
    }
    $output_1;
};
```

After issue 4's optimization (pattern-recognition pass), it would simplify to `my $yes_result = ("Hello\n") x 3;` — but even without that, the elimination of the double `do` is purely a backend/IR matter.

---

## Unnecessarily verbose translations

These are patterns where the generated code wraps trivial operations in heavy control structures:

### A. `echo "1"` → 3 lines of pipeline scaffold → `("1")`

The generator already short-circuits this to a literal, but the pattern it replaces is illuminating: a simple `echo` inside backticks would normally go through `generate_shell_command_substitution`, which emits:
```perl
do {
    my ($in, $out, $err, $pid, $result);
    my $pid = open3($in, $out, '>&STDERR', 'bash', '-c', 'echo 1');
    close $in or croak 'Close failed: ...';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
    close $out or croak 'Close failed: ...';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result =~ s/\n+\z//msx;
    $result;
};
```
— that's **15 lines** to get the string `"1"`. The IR-based special-casing (Issue 2) correctly reduces this to `"1"`. The generator already has the special case for `echo` in backticks, but many other trivial commands still go through the full pipeline.

### B. `yes "Hello" | head -3` → 20+ line while-loop with counter (and broken)

Shell: `yes_result=\`yes "Hello" | head -3\``

Generated: A `while(1)` loop with manual line counting, output accumulation, and a broken `else` branch that leaks the rest of the script.

Idiomatic Perl: Either:
- `my $yes_result = ("Hello\n") x 3;` (for this specific case)
- `my $yes_result = join '', map { "Hello\n" } 1..3;` (more general)
- `my $yes_result = qx{yes "Hello" | head -3};` (if falling back to shell is acceptable)

The pipeline infrastructure (`generate_pipeline_for_substitution`) is designed to handle arbitrary pipelines with line-by-line processing. For a simple `yes | head` pattern, it's enormous overkill.

### C. `date '+%Y-%m-%d'` → `do { require POSIX; POSIX::strftime(...) }`

Shell: `formatted_date=\`date '+%Y-%m-%d'\``

Generated: 3 lines with inline `require` and `do` block.

Idiomatic Perl: `use POSIX qw(strftime); my $formatted_date = strftime('%Y-%m-%d', localtime());`

The `do { }` wrapper is redundant because a `Declare` with an initializer already creates a scalar context. The inline `require` should be hoisted to a top-level `use`.

### D. Empty `$output` and `$output_printed_1` declarations

The pipeline generator unconditionally emits:
```perl
my $output = q{};
my $output_printed_1;
```
These are vestigial — the first is a global variable (`$output`) that shadows nothing and is never read; the second is a pipeline tracking variable for a pipeline that never conditionally prints. An IR dead-code pass would eliminate both.

---

## Summary of IR impact

| Issue | IR-fixable | Requires generator change | Priority |
|-------|-----------|--------------------------|----------|
| 1. `date` → `do { require }` | ✅ Backend style (`Call` + import hoisting) | Low (migrate date special-case from `RawText` to IR nodes) | Medium |
| 2. `echo "1"` → `("1")` | ✅ Backend style (`Declare` + `Str`) | Low (already works, just drop parens) | Low |
| 3. Inline `require` | ✅ Backend style (import hoisting) | Low | Medium |
| 4. `yes\|head` → while-loop | ✅🟡 Needs optimizer pass (pattern-matching on pipeline stages) | Medium (add to IR optimizer, not just backend) | High |
| 5. Unused variables | ✅ Dead-code elimination pass | Low | Low |
| 6. Unused import | ✅ Import tracking | Low | Low |
| 7. Broken else branch | ❌ Generator logic bug | High (fix nesting in `CommandSubstitution` handler) | **Critical** |
| 8. Double `do {}` | ✅ Backend dedup | Low | Medium |

The generated Perl has **one critical bug** (issue 7) that makes it non-functional. The rest are style/verbosity issues that an IR-based approach can address systematically.