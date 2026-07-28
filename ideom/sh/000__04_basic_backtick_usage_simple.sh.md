Now let me also check if there's any existing idiom review or similar analysis in the repo for context.Now I have all the context. Here is the idiom review.

---

# Idiom Review: `sh/000__04_basic_backtick_usage_simple.sh`

## Source Shell Script

```bash
#!/bin/bash

# Simple backtick usage test
echo "=== Basic Command Substitution ==="

# Simple command substitution
echo "Current date: `date +%Y`"
echo "Current directory: `pwd`"

# Assigning backtick results to variables
current_date=`date +%Y%m`
current_dir=`pwd`

echo "Stored date: $current_date"
echo "Stored directory: $current_dir"

# Simple function with backticks
get_file_size() {
    local file=$1
    local size=`wc -c < "$file"`
    echo "File $file has $size bytes"
}

get_file_size 000__04_basic_backtick_usage_simple.sh

echo "=== Basic Command Substitution Complete ==="
```

## Generated Perl

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

print "=== Basic Command Substitution ===\n";
say "Current date: " . (do {
require POSIX; POSIX::strftime('%Y', localtime())
});
say "Current directory: " . (do { use Cwd; $CHILD_ERROR = 0; getcwd(); });
my $current_date;
$current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime())
};
my $current_dir;
$current_dir = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
say "Stored date: $current_date";
say "Stored directory: $current_dir";

sub get_file_size {
    my $file = $_[0];
    my $size = do {
    my $wc_file = "$file";
    my $wc_file_opened = 0;
    my $content = do {
        my $result = q{};
        if (open my $fh, '<', $wc_file) {
            $wc_file_opened = 1;
            local $INPUT_RECORD_SEPARATOR = undef;
            $result = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR\n";
        } else {
            warn "Cannot open $wc_file: $OS_ERROR\n";
        }
        $result;
    };
    $wc_file_opened ? do {
        my $wc_bytes = length($content);
        $wc_bytes;
    } : q{};
};
    say "File $file has $size bytes";
    return;
}
get_file_size('000__04_basic_backtick_usage_simple.sh');
print "=== Basic Command Substitution Complete ===\n";
```

## Idiom Issues

| # | Pattern | Generated code (excerpt) | Idiomatic Perl | IR-fixable? |
|---|---------|--------------------------|----------------|-------------|
| 1 | Unused imports | `use IPC::Open3;` `my $output = q{};` `our $CHILD_ERROR;` | *(omit — none of these are used)* | **Yes** |
| 2 | Inconsistent `print`/`say` | `print "=== ... ===\n"` vs `say "Stored date: ..."` | Consistently `say` for all newline-terminated output | **Yes** |
| 3 | `require POSIX` inside `do` block at expression level | `"Current date: " . (do { require POSIX; POSIX::strftime('%Y', localtime()) })` | Top-level `use POSIX qw(strftime);` then `"Current date: " . strftime('%Y', localtime())` | **Yes** |
| 4 | `use Cwd` inside `do` block at expression level | `"Current directory: " . (do { use Cwd; $CHILD_ERROR = 0; getcwd(); })` | Top-level `use Cwd qw(getcwd);` then `"Current directory: " . getcwd()` | **Yes** |
| 5 | Spurious `$CHILD_ERROR = 0` noise | `$CHILD_ERROR = 0;` before `getcwd()` | *(omit — nothing produces child errors here)* | **Yes** |
| 6 | Declaration/assignment split | `my $current_date;` `$current_date = do { ... };` | `my $current_date = strftime('%Y%m', localtime());` | **Yes** |
| 7 | Sub parameter via `$_[0]` | `my $file = $_[0];` | `my ($file) = @_;` | **Yes** |
| 8 | Redundant `return;` at end of sub | `say "...";` `return;` `}` | Say — the sub ends naturally | **Yes** |
| 9 | `$INPUT_RECORD_SEPARATOR` / `$OS_ERROR` (English-module names without `use English`) | `local $INPUT_RECORD_SEPARATOR = undef;` `warn "...: $OS_ERROR\n"` | `local $/;` and `warn "...: $!\n"` | **Yes** |
| 10 | Unnecessary stringification `"$file"` | `my $wc_file = "$file";` | `my $wc_file = $file;` | **Yes** |
| 11 | Redundant `my $result = q{};` then immediate overwrite | `my $result = q{};` `... $result = <$fh>;` | `my $result = do { local $/; <$fh> // '' };` or just `my $result = slurp($fh)` | **Yes** |
| 12 | `wc -c < "$file"` → 21-line manual file-open/slurp/length | The entire `do { my $wc_file = ...; my $wc_file_opened = 0; my $content = do { ... }; $wc_file_opened ? do { my $wc_bytes = length(...); $wc_bytes; } : q{}; }` | `my $size = -s $file;` or `my $size = (stat($file))[7];` | **No** |
| 13 | Ternary-with-`do`-block for simple expression | `$wc_file_opened ? do { my $wc_bytes = length($content); $wc_bytes; } : q{}` | `$wc_file_opened ? length($content) : ''` | **No** |

---

## Unnecessarily Verbose Translations (Prime Candidates for IR Simplification)

### Issue #12 — `wc -c < "$file"` → 21-line manual file read

This is the single most egregious case. The shell writes:

```bash
local size=`wc -c < "$file"`
```

The generator translates this to **21 lines** of Perl (the entire `do { ... }` block inside `sub get_file_size`). It:

1. Assigns `$file` to `$wc_file` (unnecessary copy)
2. Opens the file with error-checking
3. Slurps the entire content with `local $INPUT_RECORD_SEPARATOR = undef`
4. Closes with error-checking
5. Computes `length($content)`
6. Wraps the result in a ternary checking a boolean flag

The idiomatic Perl is **one line**:

```perl
my $size = -s $file;          # file size in bytes, exactly what wc -c outputs
```

Or, if staying in the `qx{}` world (which preserves the original command):

```perl
my $size = qx{wc -c < "$file"};
```

The IR cannot fix this because by the time the generator emits code, the manual file-open/slurp/length has been *lowered into primitive operations* (open, read, close, length, assignment). No IR node preserves the original intent of "get the size of this file." The IR sees a sequence of `Assign`, `If`, `Declare`, `Output` nodes — there is no `FileSize { path }` node to optimize. The fix must happen at the **generator level**: when the shell command can be expressed as a native Perl built-in (file size, readlink, etc.), emit that built-in directly instead of expanding to primitive operations.

### Issue #13 — Ternary-with-do-block for simple length check

```perl
$wc_file_opened ? do {
    my $wc_bytes = length($content);
    $wc_bytes;
} : q{};
```

This creates a temporary variable `$wc_bytes`, assigns `length($content)` to it, then returns it — all inside a `do` block inside a ternary. The whole thing is equivalent to:

```perl
$wc_file_opened ? length($content) : ''
```

This is a **template-expansion artifact**: the generator's internal patterns always produce `do { my $var = EXPR; $var; }` even when the intermediate variable serves no purpose. An IR peephole pass *could* in theory recognize this pattern:
- Look for `do` blocks whose only statement is `Declare { init: Some(e) }` immediately followed by `Return(Some(Var(name)))` where `name` matches.
- Replace with just the expression `e`.

But this is fragile, and the better fix is to change the generator so it doesn't emit the useless scaffolding in the first place.

---

## IR-Fixability Detail

### Issue #1 — Unused imports

**IR node involved:** `IrProgram.imports: Vec<String>`

The IR already tracks required imports. An **optimization pass** (dead-import elimination) would scan all `IrStmt` / `IrExpr` nodes for their use of imported symbols, then drop unused entries from the import list.

**Cleaned-up output:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
# IPC::Open3 removed; $output and $CHILD_ERROR removed
```

### Issue #2 — Inconsistent `print`/`say`

**IR node involved:** `IrStmt::Output { value, newline: true }`

The `ir_to_perl()` backend currently has a single decision point for all `Output` statements. Both `echo "=== ... ==="` and `echo "Stored date: ..."` produce the same IR node. The inconsistency is in the current generator emitting `print "...\n"` for one and `say` for the other — but with IR, both pass through the same `emit_stmt` match arm, so the style is unified.

**Cleaned-up output:**
```perl
say "=== Basic Command Substitution ===";
# ... all other echo statements also consistently use say
```

### Issue #3 — `require POSIX` in do block

**IR node involved:** `IrStmt::Output { value: IrExpr::Interpolate([Lit("Current date: "), Expr(IrExpr::Call { func: "POSIX::strftime", ... })]) }` plus the `imports` list.

If the generator emits a `Call` node (instead of `RawExpr`), the `ir_to_perl()` backend can:
1. Collect `POSIX::strftime` → add `use POSIX qw(strftime);` to the import section.
2. Emit just `strftime(...)` in the expression, not the wrapping `do { require POSIX; POSIX::... }`.

**Cleaned-up output:**
```perl
use POSIX qw(strftime);

say "Current date: " . strftime('%Y', localtime());
```

### Issue #4 — `use Cwd` in do block (same pattern as #3)

**IR node involved:** `IrExpr::Call { func: "getcwd", args: [] }`

Same mechanism: generator emits `Call` instead of `RawExpr`, backend adds `use Cwd qw(getcwd);` to imports and emits bare `getcwd()`.

**Cleaned-up output:**
```perl
use Cwd qw(getcwd);

say "Current directory: " . getcwd();
```

### Issue #5 — Spurious `$CHILD_ERROR = 0`

**IR node involved:** None directly — this is dead code that appears to be a side-effect of how the current generator templates emit pipeline/command infrastructure.

An IR dead-code elimination pass could remove assignments to variables that are never subsequently used. `$CHILD_ERROR` is declared as `our` but never read.

**Cleaned-up output:**
```perl
# $CHILD_ERROR = 0;  — removed entirely
say "Current directory: " . getcwd();
```

### Issue #6 — Declaration/assignment split

**IR node involved:** Currently emitted as separate `IrStmt::Declare { vars: [Scalar("current_date")], init: None }` + `IrStmt::Assign { targets: [Scalar("current_date")], expr: ... }`.

A peephole optimization pass could merge consecutive `Declare` + `Assign` for the same variable into a single `Declare { init: Some(expr) }`. Alternatively, the generator itself should emit a combined node.

**Cleaned-up output:**
```perl
my $current_date = strftime('%Y%m', localtime());
my $current_dir  = getcwd();
```

### Issue #7 — Sub parameter via `$_[0]`

**IR node involved:** `IrSub { params: ["file"], body: [...] }`

The `ir_to_perl()` backend can choose how to emit parameter unpacking. Currently it emits `my $file = $_[0];` but with the `params` list available, it could emit:

```perl
sub get_file_size {
    my ($file) = @_;
    ...
}
```

**Cleaned-up output:**
```perl
sub get_file_size {
    my ($file) = @_;
    ...
}
```

### Issue #8 — Redundant final `return;`

**IR node involved:** `IrStmt::Return(None)` as the last statement in `IrSub.body`.

The `emit_sub()` function in `ir_to_perl()` can check: if the last statement is `Return(None)`, omit it. A sub with no explicit return already returns the last expression's value (or `undef` in void context).

**Cleaned-up output:**
```perl
sub get_file_size {
    my ($file) = @_;
    my $size = ...;
    say "File $file has $size bytes";
}  # no return;
```

### Issue #9 — English-module variable names without `use English`

**IR node involved:** The IR would use conceptual names like `$/` and `$!`. The pretty-printer maps these to their actual Perl representations.

The current generator emits the long English names (`$INPUT_RECORD_SEPARATOR`, `$OS_ERROR`) but never adds `use English;`. This is a bug (the code would fail if `use English` isn't present). With IR, the backend simply emits `$/` and `$!`.

**Cleaned-up output:**
```perl
local $/;           # instead of local $INPUT_RECORD_SEPARATOR
warn "...: $!\n";   # instead of $OS_ERROR
```

### Issue #10 — Unnecessary stringification `"$file"`

**IR node involved:** `IrExpr::Interpolate([Expr(IrExpr::Var("file"))])` — the generator wraps `$file` in string interpolation when it's already a scalar.

An IR simplification pass could detect `Interpolate([Expr(Var("x"))])` and rewrite it to just `Var("x")`. If the calling context requires a string (e.g., `open`), the backend adds the stringification automatically.

**Cleaned-up output:**
```perl
my $wc_file = $file;
# No quotes needed — $file is already a string
```

### Issue #11 — Redundant `my $result = q{};` initialization

**IR node involved:** Sequence of `Declare { init: Some(Str("")) }` immediately followed by `Assign { targets: ["result"], expr: <fh> }`.

A dead-store elimination pass can notice that the initial value `""` is never used before being overwritten by `<$fh>`. Either remove the initialization, or better, combine into a single `Declare { init: <expression that reads the file> }`.

**Cleaned-up output:**
```perl
my $result = do { local $/; <$fh> // '' };
```

---

## Summary

| # | Issue | IR-fixable | Mechanism |
|---|-------|-----------|-----------|
| 1 | Unused imports | **Yes** | Dead-import elimination pass on `IrProgram.imports` |
| 2 | Inconsistent `print`/`say` | **Yes** | Unified `ir_to_perl()` style for `IrStmt::Output` |
| 3 | `require POSIX` in do block | **Yes** | `IrExpr::Call` → backend hoists import, strips wrapper |
| 4 | `use Cwd` in do block | **Yes** | Same as #3 |
| 5 | Spurious `$CHILD_ERROR = 0` | **Yes** | Dead-code elimination for unused variable writes |
| 6 | Declaration/assignment split | **Yes** | Peephole merge of `Declare` + consecutive `Assign` |
| 7 | `$_[0]` parameter style | **Yes** | `IrSub.params` → `ir_to_perl()` chooses `my ($x) = @_` |
| 8 | Redundant final `return;` | **Yes** | `emit_sub` skips trailing `Return(None)` |
| 9 | `$INPUT_RECORD_SEPARATOR` / `$OS_ERROR` | **Yes** | Backend uses Perl-native `$/` and `$!` |
| 10 | `"$file"` stringification | **Yes** | Simplify `Interpolate([Expr(Var)])` → `Var` |
| 11 | Redundant `my $result = q{};` | **Yes** | Dead-store elimination on overwritten declaration |
| 12 | `wc -c` → 21-line manual read | **No** | Generator must choose `-s` or `qx{}`; IR cannot reconstruct intent from primitive ops |
| 13 | Ternary-with-do-block | **No** | Generator emits unnecessary intermediate variable; peephole possible but brittle |

**Bottom line:** 11 of 13 issues are IR-fixable. The two that are not (#12, #13) both stem from the same root cause: the generator's template for "command substitution that can be expressed natively" expands the shell command into primitive Perl operations (open, read, close, length) instead of either (a) using `qx{...}` to preserve the original command, or (b) recognizing the specific operation (`wc -c`) and mapping it to the correct Perl built-in (`-s`). Fixing these at the generator level would produce dramatically shorter code — reducing 21 lines to 1 — and is the highest-value target for generator improvement.