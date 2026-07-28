Let me find the actual locations of these files.Let me look at the relevant generator code and the ir.rs file.Let me look at how backtick words are translated to Perl:Now let me check if there's a way to see the full generated output from the pipeline:Now I have a thorough understanding of the codebase. Here is the complete idiom review.

---

# Idiom Review: `sh/000__05_system_utilities.sh` → Generated Perl

## 1. Original Shell Script

```bash
#!/bin/bash
echo "=== System Utilities ==="
formatted_date=`date '+%Y-%m-%d'`
echo "Formatted date: $formatted_date"
yes_result=`yes "Hello" | head -3`
echo "Yes command result:"
echo "$yes_result"
```

## 2. Generated Perl Code

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
my $yes_result = do { my $result_0 = qx{bash -c 'yes Hello | head -3' }; chomp $result_0; $result_0; };
say "Yes command result:";
say $yes_result;
```

## 3. Non-Idiomatic Patterns

---

### Pattern A: Unnecessary `do { }` block wrapping single-expression assignments

**Generated (date):**
```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
```

**Generated (yes_result):**
```perl
my $yes_result = do { my $result_0 = qx{bash -c 'yes Hello | head -3' }; chomp $result_0; $result_0; };
```

**Preferred (date):**
```perl
use POSIX qw(strftime);
my $formatted_date = strftime('%Y-%m-%d', localtime());
```

**Preferred (yes_result):**
```perl
my $yes_result = qx{yes Hello | head -3};
chomp $yes_result;
```

**IR-fixable?** Partially.

The `do { }` wrapper around the `date` expression comes from `src/generator/words.rs` line 1356:
```rust
format!("do {{\n{}\n}}", generate_date_expression(...))
```
This is a hard-coded `format!()` call producing raw text. The date generator then returns `require POSIX; POSIX::strftime(...)` as a string. **Both the `do` wrapper and the inline `require` require generator changes** — they are not IR nodes. The generator would need to emit an `IrStmt::Declare` with an `IrExpr::Call` and push `"POSIX qw(strftime)"` to the import list.

For the `yes_result` pipeline, the `do { my $result_0 = qx{...}; chomp $result_0; $result_0; }` pattern comes from `generate_pipeline_for_substitution()` in `pipeline_commands.rs`. This **is IR-fixable** if the pipeline were emitted as an `IrStmt::System { capture: Some("yes_result") }` with the command string `yes Hello | head -3`. The backend is already designed for this (see the `System` handler in `ir.rs`), which would emit:
```perl
my $yes_result = qx{yes Hello | head -3};
```

---

### Pattern B: Temporary variable for chomping a qx result

**Generated:**
```perl
do { my $result_0 = qx{bash -c 'yes Hello | head -3' }; chomp $result_0; $result_0; };
```

**Preferred:**
```perl
my $yes_result = qx{yes Hello | head -3};
chomp $yes_result;
```

**IR-fixable?** Yes. This is a classic case of the `Backtick` IR node (see `src/ir.rs` line 68-73):

```rust
IrExpr::Backtick {
    expr: Box<IrExpr>,
    native: bool,
}
```

When `native: false`, the backend currently wraps with the `do { my $_r = qx{...}; chomp $_r; $_r; }` pattern (see `ir_expr_to_perl` in `ir.rs`). An improved pretty-printer could instead emit:
```perl
my $var = qx{...};
chomp $var;
```
as two separate statements — an `IrStmt::Declare` with `init: qx{...}` followed by an `IrStmt::Call { func: "chomp", args: [Var("var")] }`. But to use this form, the *caller* (the assignment handler) must produce two statements instead of one expression. The IR `Assign` node would need to collaborate. The simpler fix is to **not use `Backtick` for captured assignments** — use `System { capture: Some("var") }` instead, whose handler already emits clean `qx{}` assignment without the `do {}` temp-var scaffolding.

---

### Pattern C: `use IPC::Open3` (and `our $CHILD_ERROR`, `my $output`) declared but unused

**Generated:**
```perl
use IPC::Open3;
my $output         = q{};
our $CHILD_ERROR;
```

None of these are used in the script. `IPC::Open3` is imported by the generator's boilerplate (`src/generator/mod.rs` line ~364) before checking if it's actually needed. `$output` is emitted when `needs_output_var()` returns true, but no pipeline infrastructure is generated for this script.

**IR-fixable?** Yes. The IR program's `imports: Vec<String>` and top-level `stmts` list would contain only the imports and declarations actually referenced by semantic IR nodes. If no `System { capture: None }` (which uses `system()`) or `Pipeline` (which uses the `$output` variable) nodes are present, the backend would omit `IPC::Open3`, `$output`, and `$CHILD_ERROR`. This is a **dead-code elimination** pass that the IR design explicitly mentions (see `docs/ir-design.md` "Optimization passes" → "Import minimization").

---

### Pattern D: Inline `require POSIX` instead of a compile-time `use` import

**Generated:**
```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
```

**Preferred:**
```perl
use POSIX qw(strftime);
my $formatted_date = strftime('%Y-%m-%d', localtime());
```

**IR-fixable?** No — this requires generator changes. The `require POSIX;` is emitted by `generate_date_expression()` in `src/generator/commands/date.rs` as a hard-coded string prefix:
```rust
format!("require POSIX; {}", expr_to_perl(&ir))
```
The generator would need to:
1. Not emit `require POSIX;` inline.
2. Signal to the framework that `use POSIX qw(strftime);` must be added.
3. Produce an `IrExpr::Call { func: "POSIX::strftime", args: [...] }` instead of a raw string.

The IR program's `imports` list (`IrProgram.imports`) is designed exactly for this, but the date generator doesn't use it yet — it's still in the `RawText` / `format!()` migration phase.

---

### Pattern E: `bash -c '...'` wrapper around simple `qx{}` commands

**Generated:**
```perl
qx{bash -c 'yes Hello | head -3' }
```

**Preferred:**
```perl
qx{yes Hello | head -3}
```

**IR-fixable?** Yes — if the pipeline is represented as `IrStmt::System { capture: ... }` with `cmd_str: "yes Hello | head -3"` directly, without the `bash -c` wrapper. The `bash -c` comes from `generate_pipeline_for_substitution()` in `pipeline_commands.rs` around line 870, where the generic fallback explicitly builds a `bash -c '...'` command string. The IR `System { capture }` handler in `ir.rs` already emits clean `qx{...}` without `bash -c` — but the generator must route through that node rather than format a string.

---

### Pattern F: Pipeline boilerplate for a single-pipeline command substitution

**Generated (conceptually — what the pipeline handler constructs for a multi-stage pipeline):**
The `yes "Hello" | head -3` pipeline triggers the generic 2-command pipeline fallback in `generate_pipeline_for_substitution()`, which — although it produces a single `qx{}` call for this case — surrounds it with `do { my $result_N = qx{...}; chomp $result_N; $result_N; }`. In more complex pipelines (e.g., `ls -l | grep foo | wc -l`), the same function would generate hundreds of lines of Perl pipeline infrastructure (temporary variables, pipe-opening code, etc.) even when the entire pipeline is just a command substitution.

**Preferred:**
```perl
my $yes_result = qx{yes Hello | head -3};
```

**IR-fixable?** Yes. The IR `Pipeline { capture: Some("var"), cmd_str: Some("yes Hello | head -3") }` node in `ir.rs` handles exactly this:
```rust
IrStmt::Pipeline { stages, capture, cmd_str, .. } => {
    if let Some(var) = capture {
        if let Some(cmd) = cmd_str {
            out.push_str(&format!("my ${} = qx{{{}}};\n", var, cmd));
            out.push_str(&format!("chomp ${};\n", var));
            ...
        }
    }
}
```
The generator currently does *not* produce this IR node for pipelines — it produces raw text. Migration to the IR would let the backend emit the simple `qx{}` form automatically.

---

### Pattern G: Uneven indentation in multi-line `do` block

**Generated:**
```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
```

The `require POSIX;` is not indented relative to the surrounding `do` block. This is because `generate_date_expression` returns a string with no leading indentation, and the `do { ... }` wrapper in `word_to_perl` also doesn't indent.

**IR-fixable?** Yes — the `Declare` statement emitter would add proper indentation via `emit_indent()`. The body of a declaration's initializer expression doesn't need inner indentation at all if it's a single expression, which is the case here.

---

### Pattern H: Missing `chomp $formatted_date` — but it happens to be correct

The `date` expression `POSIX::strftime(...)` does not include a trailing newline (strftime doesn't add one), so the value is already correct without chomping. The shell `date` command, however, *does* output a trailing newline. The `chomp` that would have been needed if this were a `qx{}` call is correctly omitted — but only because the native Perl translation happens to produce a newline-free result. The correctness is coincidental; a different `date` format could produce a different behavior.

This is not really a bug, but worth noting: the `date` translation path silently changes the trailing-newline semantics of the shell command (shell backtick strips the trailing newline; `strftime` never had one). They match, but only by accident of the implementation.

---

## 4. Unnecessarily Verbose Translations

### Verdict: The entire `yes_result` translation is unnecessarily verbose

The generated code for `yes_result=` is the standout candidate for IR-based simplification:

**Generated (17 tokens, 4 control-flow constructs):**
```perl
my $yes_result = do { my $result_0 = qx{bash -c 'yes Hello | head -3' }; chomp $result_0; $result_0; };
```

This wraps a simple operation in:
- A `do { }` block (to return a value)
- A temporary variable `$result_0` (used only to chomp and return)
- A `chomp` call (handling trailing-newline stripping from the qx result)

**Clean idiomatic Perl (2 statements, no temporary):**
```perl
my $yes_result = qx{yes Hello | head -3};
chomp $yes_result;
```

Or even, since `qx{}` in scalar context returns the entire output as a single string:
```perl
chomp(my $yes_result = qx{yes Hello | head -3});
```

### The `formatted_date` translation is also unnecessarily verbose

**Generated (do block + inline require):**
```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
```

**Clean (use at top + direct call):**
```perl
use POSIX qw(strftime);
my $formatted_date = strftime('%Y-%m-%d', localtime());
```

---

## 5. Summary Table

| # | Pattern | IR-fixable? | IR Node Involved | Notes |
|---|---------|-------------|------------------|-------|
| A | `do { }` wrapper around single-expression assignment | Partially | `Declare` / `System { capture }` | Date path needs generator change (inline `require`). Pipeline path IS IR-fixable via `System { capture }`. |
| B | Temp variable for chomping qx result | Yes | `System { capture: Some("var") }` | Backend already has a clean handler. |
| C | Unused imports (`IPC::Open3`) and dead variables (`$output`, `$CHILD_ERROR`) | Yes | `IrProgram.imports`, top-level `stmts` | Dead-code elimination / import minimization. |
| D | Inline `require POSIX` instead of compile-time `use` | No | N/A | Generator (date.rs) hard-codes `require POSIX;` string. Must push to `imports` list instead. |
| E | `bash -c '...'` wrapper around qx{} | Yes | `System { cmd_str }` | Backend already emits clean `qx{...}` without `bash -c`. |
| F | Pipeline boilerplate for command-substitution pipelines | Yes | `Pipeline { capture, cmd_str }` | The IR node already handles this cleanly. |
| G | Uneven indentation in multi-line do block | Yes | `emit_stmt()` / `emit_indent()` | Backend formatting concern. |
| H | Coincidental newline correctness (date) | N/A | N/A | Not a bug, but fragile. |

### Unnecessarily verbose (prime IR-simplification candidates):
1. **`yes_result` assignment**: 4 control constructs → 2 simple statements
2. **`formatted_date` assignment**: do-block + inline require → import + direct call