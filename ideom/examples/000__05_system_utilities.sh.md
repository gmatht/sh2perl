Now let me look at more of the generated code context and the original file:Now let me check if there's a more complete generated output or any other test fixtures:Now I have enough context. Here is the full idiom review.

---

# Idiom Review: `examples/000__05_system_utilities.sh` → Perl

## 1. Source Shell Script

```bash
#!/bin/bash
echo "=== System Utilities ==="

formatted_date=`date '+%Y-%m-%d'`
echo "Formatted date: $formatted_date"

yes_result=`yes "Hello" | head -3`
echo "Yes command result:"
echo "$yes_result"
```

Only four active statements (the rest are comments/commented-out).

## 2. Generated Perl

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

---

## 3. Non-idiomatic Patterns

### Pattern A — Dead/Unused Boilerplate

```perl
use IPC::Open3;
my $output         = q{};
our $CHILD_ERROR;
```

**Why it's not idiomatic:** These three items are never used anywhere in the generated code. `IPC::Open3` is a heavy module for interactive subprocess communication — importing it just for `qx{}` is wasteful. `$output` and `$CHILD_ERROR` are declared but never referenced.

**IR-fixable?** Yes. The IR program knows exactly which imports and statements are reachable. A dead-code elimination pass over `IrProgram` can remove unused `use` statements and unused variable declarations before pretty-printing.

- **IR nodes involved:** `IrProgram::imports`, `IrStmt::Declare { vars: [Decl("output", Scalar)], init: Some(...) }`, and the unused `our $CHILD_ERROR` (which would be a `Declare` with package sigil).
- **Cleaned output:** Those three lines simply vanish. No functional change.

### Pattern B — `do { require POSIX; POSIX::strftime(...) }` for a date backtick

```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
```

**Why it's not idiomatic:**
- The `do { ... }` wrapper creates an unnecessary scope block around a single expression.
- A native Perl programmer would write either:

```perl
# Option A — concise, one-liner
my $formatted_date = POSIX::strftime('%Y-%m-%d', localtime);

# Option B — with separate require
require POSIX;
my $formatted_date = POSIX::strftime('%Y-%m-%d', localtime);
```

The `do` block adds visual nesting and syntactic noise for zero benefit. The shell `date` backtick is being correctly replaced by a native Perl call (good!), but the wrapping is clumsy.

**IR-fixable?** Yes, two different ways:

1. **In the IR:** If the assignment is represented as `Assign { targets: ["formatted_date"], expr: Call { func: "POSIX::strftime", args: [...] } }` with `require POSIX` tracked as a needed import in `IrProgram::imports`, then the pretty-printer can emit the `require` at the top of the file and the assignment as a simple `my $formatted_date = POSIX::strftime(...);` — no `do` block.

2. **Alternative:** Even if the `require` is kept inline, the pretty-printer can still avoid the `do { ... }` by emitting the `require` as a separate statement before the assignment.

- **IR nodes involved:** `IrStmt::Assign { targets: [...], expr: IrExpr::Call { func: "POSIX::strftime", ... } }` plus an `IrStmt::Require` (or just tracking the module in `imports`).
- **Cleaned output:**
```perl
use POSIX qw/strftime/;
# ...
my $formatted_date = strftime('%Y-%m-%d', localtime);
```
or at minimum:
```perl
my $formatted_date = POSIX::strftime('%Y-%m-%d', localtime);
```
without the `do` block.

### Pattern C — `do { my $tmp = qx{...}; chomp $tmp; $tmp; }` for a captured command

```perl
my $yes_result = do { my $result_0 = qx{bash -c 'yes Hello | head -3' }; chomp $result_0; $result_0; };
```

**Why it's not idiomatic:**
- The `do { my $var = ...; chomp $var; $var; }` pattern is the **cardinal sign** of a line-by-line transliteration. Shell's `var=$(cmd)` trims trailing newlines by default, so the translator inserts `chomp` — but then wraps everything in a `do` block with a temporary variable so the chomped value is returned.
- An idiomatic Perl programmer would write either:

```perl
# Option A — explicit, clear
my $yes_result = `yes Hello | head -3`;
chomp $yes_result;

# Option B — if chomp-in-assignment is desired
my $yes_result = ( `yes Hello | head -3` =~ s/\n\z//r );
```

The `do { my $tmp = ...; chomp $tmp; $tmp; }` pattern is a construct you see in auto-generated code, almost never in hand-written Perl. It's a dead giveaway that the generator is mapping shell constructs one-to-one rather than thinking in Perl idioms.

**IR-fixable?** Yes, and the IR design doc explicitly calls this out.

- **IR nodes involved:** `IrStmt::System { cmd: "yes", args: ["Hello"], capture: Some("yes_result") }` (or a `Pipeline` stage). The `chomp` is semantically equivalent to "strip trailing newline from captured output." The pretty-printer for `System { capture: Some(var) }` can emit:

```perl
my $yes_result = `yes Hello | head -3`;
chomp $yes_result;
```

as two clean statements, without the `do` wrapper. Or, if the generator's semantic model knows shell strips trailing newlines, it could emit just `my $yes_result = `yes Hello | head -3`;` because the `chomp` is already implicit in Perl's `qx{}` behavior... actually no, `qx{}` does NOT strip trailing newline. So the `chomp` is needed. But the wrapping `do` block is not.

- **Cleaned output:**
```perl
my $yes_result = `yes Hello | head -3`;
chomp $yes_result;
```

### Pattern D — `bash -c '...'` wrapper around simple commands

```perl
qx{bash -c 'yes Hello | head -3' }
```

**Why it's not idiomatic:**
- Shell pipelines are already natively supported by Perl's `qx{}` / backtick operator. There is no need to wrap in `bash -c`. The string inside `qx{}` is already executed by `/bin/sh -c` by default.
- The generated code should emit:

```perl
qx{yes Hello | head -3}
```

The `bash -c` wrapper is defensive (perhaps to force bash instead of sh), but it adds an extra layer of quoting complexity and makes the code look machine-generated.

**IR-fixable?** Yes, at the pretty-print level. If the IR represents a pipeline capture as `Pipeline { stages: [["yes", "Hello"], ["head", "-3"]], capture: Some("yes_result") }`, then the backend knows this is a simple pipeline and can emit the native Perl form without the `bash -c` wrapper. The extra wrapper is a quirk of the current string-based generator, not a semantic requirement.

- **IR nodes involved:** `IrStmt::Pipeline { stages: [...], last_output: Some("yes_result") }` or a nested `System` with pipe.
- **Cleaned output:**
```perl
my $yes_result = `yes Hello | head -3`;
chomp $yes_result;
```

### Pattern E — Variable naming `$result_0` in a `do` block

```perl
my $result_0 = qx{...}
```

**Why it's not idiomatic:**
- The counter-based name `$result_0` is a compiler-generated temporary. In hand-written Perl, you'd just reuse the destination variable name or use a meaningful name.
- If the `do { ... }` block is eliminated (Pattern C), this problem goes away entirely because there's no intermediate variable.

**IR-fixable?** Yes, same as Pattern C — removing the `do` block removes the need for a synthetic temp variable.

### Pattern F — Use of `say` for all output (minor)

Not really a problem — `say` is idiomatic modern Perl. The `say "Yes command result:";` and `say $yes_result;` are perfectly fine.

---

## 4. Summary Table

| Pattern | Description | IR-fixable? | IR node(s) | Automatically cleanable? |
|---------|-------------|-------------|------------|--------------------------|
| **A** | Unused boilerplate (`IPC::Open3`, `$output`, `$CHILD_ERROR`) | ✅ Yes | `IrProgram::imports`, `IrStmt::Declare` (dead elimination) | Yes — dead code elimination pass |
| **B** | `do { require POSIX; func() }` wrapper | ✅ Yes | `IrStmt::Assign` + `IrExpr::Call` + `imports` | Yes — promote `require` to file-level import, drop `do` block |
| **C** | `do { my $tmp = qx{...}; chomp $tmp; $tmp; }` | ✅ Yes | `IrStmt::System { capture: Some(var) }` | Yes — emit two clean statements without `do` wrapper |
| **D** | `bash -c '...'` wrapper around pipeline | ✅ Yes | `IrStmt::Pipeline` or `System` | Yes — backend knows native `qx{}` supports pipes |
| **E** | Compiler-generated temp `$result_0` | ✅ Yes | (same as C) | Goes away when Pattern C is fixed |
| **F** | `say` vs `print` | ✅ Style choice | `IrStmt::Output { newline: true }` | Trivially configurable in `ir_to_perl()` |

**All five problems are IR-fixable.** None require changing the generator logic itself — they are all matters of how the IR nodes are assembled into text. This is exactly the kind of improvement the IR design doc promises.

---

## 5. Unnecessarily Verbose Translations

The **most flagrant** case is the `yes` command capture. Let's break down the verbosity cost:

**Generated code (67 characters of statement):**
```perl
my $yes_result = do { my $result_0 = qx{bash -c 'yes Hello | head -3' }; chomp $result_0; $result_0; };
```

**Idiomatic Perl (38 characters):**
```perl
my $yes_result = `yes Hello | head -3`;
chomp $yes_result;
```

The generated version uses **1.76×** the character count. The overhead comes from:

| Component | Characters | Reason |
|-----------|-----------|--------|
| `do { ... }` | +8 | Creates unnecessary scope |
| `my $result_0 = ` | +17 | Intermediate temp variable |
| `bash -c '...'` | +11 | Defensive shell wrapper |
| `; $result_0;` | +12 | Explicit return from `do` block |

The `date` case is less extreme but still wasteful:

**Generated (54 chars):**
```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
```

**Idiomatic (41 chars):**
```perl
require POSIX;
my $formatted_date = POSIX::strftime('%Y-%m-%d', localtime);
```

The `do { ... }` adds 8 characters and 2 lines of nesting for no benefit.

**Both are prime candidates for IR-based simplification.** The IR design doc specifically lists `System { capture: Some("out") }` → `my $out = do { ... qx{...} ... };` → `my $out = qx{...};` as the poster-child transformation. These examples are textbook cases of exactly that pattern.

---

## 6. Verdict

The generated Perl is **semantically correct** but **stylistically immature**. Every non-idiomatic pattern is a pretty-printing issue rather than a generator logic issue. The IR-based backend described in `docs/ir-design.md` is the right solution:

1. The generator would produce `IrProgram` nodes like `System { capture: Some("yes_result"), cmd: "yes", args: ["Hello"], pipe_to: Some("head -3") }` and `Assign { targets: ["formatted_date"], expr: Call { func: "POSIX::strftime", ... } }`.
2. The `ir_to_perl()` backend would apply a set of style rules and dead-code elimination passes to produce minimal, idiomatic output.
3. If in the future the team wants to change `say` to `print` or `qx{}` to `readpipe`, it's a one-line change in the backend — not a hunt through 80+ generator functions.

The only item missing from the IR design that this review suggests: the concept of **`require` vs `use` at file scope** (for module imports from backtick substitutions) and how to track them without emitting `do { require ...; ... }` blocks. Currently the IR has `imports: Vec<String>` for `use` statements but no equivalent for `require`. A `requires: Vec<String>` field in `IrProgram` would let the `date` case emit a file-level `require POSIX;` instead of an inline `do` block.