# Idiom review: sh/000__04_basic_backtick_usage.sh

## Source

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

get_file_size 000__04_basic_backtick_usage.sh

echo "=== Basic Command Substitution Complete ==="
```

## Generated Perl

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;

print "=== Basic Command Substitution ===\n";
say "Current date: " . (do { my $_chomp_temp = do {
require POSIX; POSIX::strftime('%Y', localtime(time())) . "\n"
}; chomp $_chomp_temp; $_chomp_temp; });
$CHILD_ERROR = 0;
say "Current directory: " . (do { my $_chomp_temp = do { use Cwd; $CHILD_ERROR = 0; getcwd(); }; chomp $_chomp_temp; $_chomp_temp; });
$CHILD_ERROR = 0;
my $current_date;
my @current_date;
my %current_date;
$current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time())) . "\n"
};
my $current_dir;
my @current_dir;
my %current_dir;
$current_dir = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
say "Stored date: $current_date";
$CHILD_ERROR = 0;
say "Stored directory: $current_dir";
$CHILD_ERROR = 0;

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
    $CHILD_ERROR = 0;
    return;
}
get_file_size('000__04_basic_backtick_usage.sh');
print "=== Basic Command Substitution Complete ===\n";

exit $main_exit_code;
```

## Idiom issues

| # | Pattern | Generated code | Idiomatic Perl | IR-fixable? |
|---|---------|---------------|----------------|-------------|
| 1 | **Sigil explosion** — all three sigils declared for every variable; only one used | `my $current_date; my @current_date; my %current_date;` | `my $current_date;` | **Yes** |
| 2 | **CPAN boilerplate noise** — modules imported that are never used | `use Carp; use English ...; use locale; use IPC::Open3;` | *(omit unused imports)* | **Yes** |
| 3 | **Dead scaffolding variables** — declared but never read | `$main_exit_code`, `$ls_success`, `$__set_e`, `$output` | *(omit unused variables)* | **Yes** |
| 4 | **CHILD_ERROR reset noise** — `$CHILD_ERROR = 0` after every statement, including `say` and `print` that spawn no child | `$CHILD_ERROR = 0;` after nearly every line | *(omit; only reset after actual external commands)* | **Yes** |
| 5 | **print/say inconsistency** — uses `print "...\n"` where `say` would do, then uses `say` elsewhere | `print "=== ... ===\n"` vs `say "Stored date: ..."` | `say "=== ... ==="` (uniformly `say`) | **Yes** |
| 6 | **Inline `require` / `use` inside expressions** — instead of file-scope imports | `require POSIX;` / `use Cwd;` inside `do { }` | `use POSIX qw(strftime);` / `use Cwd;` at top | **Yes** |
| 7 | **Chomp dance** — appends `"\n"` then immediately chomps it off | `do { ... . "\n" }; chomp $_chomp_temp; $_chomp_temp;` | `qx{date +%Y}` or `strftime(...)` without newline games | **Yes** |
| 8 | **`wc -c < file` expanded to file-read + length** — a 5-character shell command becomes 18 lines of Perl I/O | (see §File-size explosion below) | `my $size = -s $file;` (Perl file test operator) or `qx{wc -c < "$file"}` | **Partly** |
| 9 | **No `use warnings FATAL => ...` granularity** — loads all of `warnings` without scoping | `use warnings;` | `use warnings FATAL => qw(all);` or scoped warnings | **No** (stylistic choice) |
| 10 | **Unnecessary `return;` at sub end** — subs return last expression | `return;` at end of `get_file_size` | *(omit; let last expression be the implicit return)* | **Yes** |
| 11 | **`$_[0]` instead of `shift` or named param** — cryptic | `my $file = $_[0];` | `my $file = shift;` | **No** (stylistic, but IR could choose) |
| 12 | **`exit $main_exit_code;` is dead code** — `$main_exit_code` never modified from `0` | `exit $main_exit_code;` | `exit 0;` or just let the script fall off | **Yes** |

---

## IR-fixability details

### 1. Sigil explosion — IR-fixable: **Yes**

- **IR node involved**: `IrStmt::Declare` (which carries a `sigil: Sigil` field per variable)
- **Problem**: The generator emits three separate `Declare` nodes (one per sigil) for every shell variable, because the shell AST doesn't know which sigil will be used. This is a conservative guess.
- **IR fix**: Add a **liveness analysis** pass over the `IrProgram` before pretty-printing. Any `Declare` for a variable that is never referenced with that sigil is dead code. Remove it.
- **Cleaned output**:
  ```perl
  my $current_date;
  my $current_dir;
  ```

### 2. CPAN boilerplate noise — IR-fixable: **Yes**

- **IR node involved**: `IrProgram.imports`
- **Problem**: The `Imports` list is populated statically from a fixed template, not from actual usage analysis.
- **IR fix**: After the full `IrProgram` is built, scan all `IrExpr` / `IrStmt` nodes for:
  - `use Cwd;` → only needed if `getcwd()` appears.
  - `use POSIX;` → only needed if `POSIX::strftime` appears.
  - `use Carp;` → only needed if `carp`/`croak`/`confess` appears.
  - `use English;` → only needed if `$OS_ERROR`/`$INPUT_RECORD_SEPARATOR` etc. appear.
  - `IPC::Open3` → only needed if pipeline/`system` capture is used.
  Delete unused imports.
- **Cleaned output**:
  ```perl
  use strict;
  use warnings;
  use POSIX qw(strftime);
  use Cwd;
  ```

### 3. Dead scaffolding variables — IR-fixable: **Yes**

- **IR node involved**: `IrStmt::Declare` (for top-level vars)
- **Problem**: Variables like `$main_exit_code`, `$ls_success`, `$__set_e`, `$output` are declared but the `IrProgram` never references them again after initialization.
- **IR fix**: Dead-assignment elimination pass: remove `IrStmt::Declare` (and its optional `init` assignment) for variables that have zero `IrExpr::Var` references in the entire program.
- **Cleaned output**:
  ```perl
  # (all scaffolding variables gone)
  ```

### 4. `$CHILD_ERROR` reset noise — IR-fixable: **Yes**

- **IR node involved**: `IrStmt::System` / `IrStmt::Pipeline` / backtick command substitution
- **Problem**: The generator emits `$CHILD_ERROR = 0;` after *every* statement, even `say`/`print` that touch no external command.
- **IR fix**: Only emit `$CHILD_ERROR = 0;` after nodes that actually run external commands (those producing `IrStmt::System` or backtick `qx{}`). A `say` statement is purely a Perl built-in; no child process is involved. The reset belongs in the `emit_System` handler, not globally.
- **Cleaned output**: No `$CHILD_ERROR = 0;` appears after `say` or `print`.

### 5. print/say inconsistency — IR-fixable: **Yes**

- **IR node involved**: `IrStmt::Output { value, newline }`
- **Problem**: When the shell writes `echo "text"` (which adds a newline), the generator emits either `print "text\n"` or `say "text"` depending on which `generate_*` function handles it. The inconsistency is a symptom of the two code paths not being unified.
- **IR fix**: Once all output paths produce `IrStmt::Output { newline: true }`, `ir_to_perl()` can uniformly emit `say EXPR` for every newline-terminated output.
- **Cleaned output**:
  ```perl
  say "=== Basic Command Substitution ===";
  # ... later ...
  say "=== Basic Command Substitution Complete ===";
  ```

### 6. Inline `require`/`use` inside expressions — IR-fixable: **Yes**

- **IR node involved**: `IrExpr::Call { func: "POSIX::strftime" }` / `IrExpr::Call { func: "getcwd" }`
- **Problem**: The generator inlines the `require POSIX` / `use Cwd` directly into the expression instead of placing them in the file-scope import list.
- **IR fix**: When the generator encounters a function that belongs to a module, it should add the `use Module qw(funcname)` to `IrProgram.imports` (or ensure it's already there) and emit only the function call in the expression. The inline `require` is a fallback when the generator doesn't know the module ahead of time — the IR can hoist it.
- **Cleaned output**:
  ```perl
  use POSIX qw(strftime);
  use Cwd;

  # Then later in the body:
  say "Current date: " . strftime('%Y', localtime);
  say "Current directory: " . getcwd();
  ```

### 7. Chomp dance — IR-fixable: **Yes**

- **IR node involved**: Command-substitution IR node (currently rendered as `RawText` or through ad-hoc `do` blocks)
- **Problem**: The generator models backtick output as: `cmd_output . "\n"` → store in temp → `chomp` → return. This is because it uses a `strftime` expression (which doesn't add a newline), so it appends one to match shell semantics, then chomps to strip it. The dance is backwards: just use `qx{}` directly, which already handles newlines sanely.
- **IR fix**: Introduce an `IrExpr::Backtick(Vec<String>)` node. The pretty-printer would emit:
  ```perl
  qx{date +%Y}
  ```
  or, for known low-risk commands like `date` / `pwd`, the optimizer could inline to `strftime` / `getcwd` without the newline rigmarole.
- **Cleaned output**:
  ```perl
  # Option A — qx (always correct, simple):
  say "Current date: " . qx{date +%Y};
  say "Current directory: " . qx{pwd};

  # Option B — inlined (when the command is known):
  say "Current date: " . strftime('%Y', localtime);
  say "Current directory: " . getcwd();
  ```

### 8. File-size explosion — IR-fixable: **Partly**

This is the most egregious verbosity. The single backtick `` `wc -c < "$file"` `` is expanded into an 18-line `do` block that manually opens the file, reads its entire content, and measures the string length.

- **IR node involved**: Backtick command substitution on `wc -c < "$file"`.
- **Why it's so large**: The generator recognizes `wc -c < file` as a "simple command" and tries to emulate it natively in Perl, rather than just running `wc` via `qx{}`. The expansion logic is:
  1. Open the file
  2. Slurp it into `$content`
  3. Check if open succeeded (`$wc_file_opened`)
  4. Return `length($content)` or empty string
- **IR fix — if we keep native expansion**: Add an `IrExpr::FileSize { path }` node. The generator would detect the `wc -c < "$file"` pattern and emit:
  ```perl
  my $size = -s $file;
  ```
  This is trivially the right Perl idiom (the `-s` file test operator). The IR node `IrExpr::FileSize` would pretty-print to `-s PATH`.

- **IR fix — if we fall back to qx**: Even simpler: don't try to be clever. Emit:
  ```perl
  chomp(my $size = qx{wc -c < "$file"});
  ```
  This preserves shell semantics exactly and is 1 line, not 18.

- **Root cause verdict**: The problem is in the **generator logic** (the expansion of `wc -c` into native Perl code is overly ambitious and produces a terrible result). An IR pass could *detect* the pattern and substitute `-s`, but that requires the generator to emit something the IR can recognize. Currently it emits a `do { ... }` containing raw Perl text (`RawExpr`), which the IR cannot analyze. So **this is NOT fully IR-fixable** without first migrating the generator to emit a semantic IR node instead of raw Perl.

- **What would need to change**: The generator's command-expansion logic for `wc` needs to be taught to emit an `IrExpr::FileSize` or `IrExpr::Backtick` instead of a raw `do { open ... slurp ... length }` text blob. That is a generator-side change, not a pretty-printer change.

### 9. Bare `use warnings` — IR-fixable: **No** (stylistic preference, not correctness)

The choice of `use warnings` vs `use warnings FATAL => qw(all)` is a project convention. The IR could make this configurable (add a `--fatal-warnings` flag to the CLI), but it's not an idiom issue per se.

### 10. Unnecessary `return;` — IR-fixable: **Yes**

- **IR node involved**: `IrStmt::Return(None)`
- **Problem**: The generator explicitly emits `return;` at the end of every sub, even when the sub would naturally end.
- **IR fix**: In `ir_to_perl()`, when the last statement of a sub is `IrStmt::Return(None)`, omit it. Only emit an explicit `return;` if there's a conditional return in the middle of the sub.
- **Cleaned output**:
  ```perl
  sub get_file_size {
      my $file = shift;
      my $size = ...;
      say "File $file has $size bytes";
  }
  ```

### 11. `$_[0]` instead of `shift` — IR-fixable: **No** (stylistic choice)

`my $file = $_[0];` works perfectly well. Some Perl programmers prefer `shift` or `my ($file) = @_;`. The IR *could* normalize this (it has the parameters in `IrSub.params`, and could emit `shift` in the pretty-printer), but it's a minor style point. Not a blocker.

### 12. `exit $main_exit_code;` is dead — IR-fixable: **Yes**

- **IR node involved**: `IrStmt::System` (exit) or a dedicated `IrStmt::Exit`
- **Problem**: `$main_exit_code` is set to 0 and never modified.
- **IR fix**: Constant propagation pass replaces `$main_exit_code` with `0`. Then the IR can emit `exit 0;` or omit it entirely since the script will exit 0 anyway.
- **Cleaned output**:
  ```perl
  exit 0;
  ```
  Or simply nothing (let the script fall off the end with exit code 0).

---

## Unnecessarily verbose translations

These are the most painful expansions — places where a simple shell operation is drowned in Perl scaffolding:

### A. Backtick echo expansion (line 6 of source)

**Shell**: `echo "Current date: `date +%Y`"`

**Generated (97 chars / multi-line do-block)**:
```perl
say "Current date: " . (do { my $_chomp_temp = do {
require POSIX; POSIX::strftime('%Y', localtime(time())) . "\n"
}; chomp $_chomp_temp; $_chomp_temp; });
```

**Preferred** (3 options in increasing order of idiom):
```perl
# 1) Keep the shell command via qx (closest to original semantics)
say "Current date: " . qx{date +%Y};

# 2) Use Perl native time formatting (when the command is known-safe)
use POSIX qw(strftime);
say "Current date: " . strftime('%Y', localtime);

# 3) qx with chomp (explicit about newline handling)
chomp(my $year = qx{date +%Y});
say "Current date: $year";
```

**Verdict**: The chomp-dance (`... . "\n"` then `chomp`) is a code smell. It suggests the generator is working against itself — it generates a value without a newline, adds one, then strips it. The IR should either use `qx{}` (which preserves the newline behavior of the original shell) or inline the call properly without the newline back-and-forth.

### B. Variable assignment with backtick (source line 9)

**Shell**: `current_date=`date +%Y%m``

**Generated** (5 lines):
```perl
my $current_date;
my @current_date;
my %current_date;
$current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time())) . "\n"
};
```

**Preferred**:
```perl
my $current_date = qx{date +%Y%m};
# or
use POSIX qw(strftime);
my $current_date = strftime('%Y%m', localtime);
```

**Verdict**: Three unused sigil declarations + inline `require` + trailing `"\n"` that will be concatenated into `$current_date`. The variable will contain `"202501\n"` (with a trailing newline!), which is almost certainly not what the shell would produce when assigned from backticks (backticks strip trailing newlines in the shell too, but the generated code doesn't chomp it here because there's no chomp wrapper — compare with pattern A where there IS a `chomp`). **This is a bug**: the assignment path doesn't chomp but the echo-interpolation path does.

### C. File-size via `wc -c` (source line 17)

**Shell**: `local size=`wc -c < "$file"``

**Generated** (18 lines of nested `do { }`):
```perl
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
```

**Preferred** (one line):
```perl
my $size = -s $file;
```

Or, keeping the shell command:
```perl
chomp(my $size = qx{wc -c < "$file"});
```

**Verdict**: This is the single worst expansion in the file. The generator tries to "native-ize" `wc -c` by opening the file, reading it, and taking `length`. This is:
- **Wrong for large files** (slurps the entire file into memory just to get its size)
- **Wrong for binary files** (length in characters vs bytes; `-s` gives actual byte count)
- **Wrong semantically** (shell `wc -c` counts bytes, Perl `length` counts characters under `use utf8`)
- **Grotesquely verbose** (18 lines for what should be 3 tokens: `-s $file`)

The generator needs a **pattern-matching pass** that recognizes `wc -c < FILE` → `-s FILE`. This must happen in the generator logic (or as an IR transform pass after the IR builder emits something semantic), not just in the pretty-printer.

---

## Structural comparison: lines of code

| Shell | Generated | Idiomatic Perl |
|-------|-----------|----------------|
| 23 lines | ~55 lines of actual code (plus whitespace) | ~25 lines |

The generated code is **2.4×** the size of the original shell script and **2.2×** the idiomatic Perl version. Roughly half the bloat is from the scaffolding (sigil explosion, `$CHILD_ERROR`, unused imports) and half from the command-expansion verbosity (especially `wc -c`).

## Summary

| Issue | IR-fixable |
|-------|:----------:|
| Sigil explosion | ✅ Declare-node liveness pass |
| Unused imports | ✅ Import usage scan |
| Dead scaffolding vars | ✅ DCE pass |
| `$CHILD_ERROR` noise | ✅ Scope resets to System nodes only |
| print/say inconsistency | ✅ Unify in `emit_Output` |
| Inline `require`/`use` | ✅ Hoist to imports list |
| Chomp dance | ✅ Emit `qx{}` or native call without newline games |
| `wc -c` explosion | ❌ Must fix generator logic to emit `IrExpr::FileSize` or `IrExpr::Backtick` |
| `return;` at sub end | ✅ Omit trailing `Return(None)` |
| `exit $main_exit_code` | ✅ Constant propagation → `exit 0` or omit |

**Of 12 issues, 10 are fixable purely via IR pretty-printing and optimization passes. The one critical fix (the `wc -c` disaster) requires changing how the generator models external commands — it must emit a semantic IR node (like `IrExpr::Backtick` or `IrExpr::FileSize`) instead of raw Perl text.**

The fact that 10/12 issues are IR-fixable validates the IR design's core thesis: style and optimization can be centralized. The `wc -c` case is the counter-example that proves why the generator must also be migrated to emit semantic IR nodes rather than `RawText` strings — as long as the generator can hide behind `RawText`, the IR can't help.