Now let me read the rest of the IR design document and check the generator structure:Now I have a thorough understanding of both the generated code, the IR design, and the current generator structure. Let me compose the review.

---

## Idiom Review: `examples/000__04a_basic_command_substitution.sh`

### 1. Original Shell Script

```bash
#!/bin/bash
echo "=== Basic Command Substitution ==="
echo "Current date: `date +%Y`"
echo "Current directory: `basename $(pwd)`"

current_date=`date +%Y%m`
current_dir=`basename $(pwd)`

echo "Stored date: $current_date"
echo "Stored directory: $current_dir"
echo "=== Basic Command Substitution Complete ==="
```

### 2. Generated Perl Code

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
say "Current directory: " . (do { my $_chomp_temp = do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $_chomp_temp; $_chomp_temp; });
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
$current_dir = do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
};
say "Stored date: $current_date";
$CHILD_ERROR = 0;
say "Stored directory: $current_dir";
$CHILD_ERROR = 0;
print "=== Basic Command Substitution Complete ===\n";

exit $main_exit_code;
```

---

### 3. Non-Idiomatic Patterns

---

#### Pattern A: The chomp-dance (`do { my $_chomp_temp = do { ... . "\n" }; chomp $_chomp_temp; $_chomp_temp; }`)

**Generated:**
```perl
say "Current date: " . (do { my $_chomp_temp = do {
require POSIX; POSIX::strftime('%Y', localtime(time())) . "\n"
}; chomp $_chomp_temp; $_chomp_temp; });
```

**Problem:** This is the most pervasive non-idiomatic pattern. Every command substitution (including those translated to native Perl calls) is wrapped in a two-layer `do` block: the inner `do` appends `"\n"` to the result (simulating backtick output), the outer `do` chomps it off and returns. For native Perl calls like `POSIX::strftime`, this is especially absurd — the output never had a newline, so we are appending `"\n"` only to immediately remove it.

**Also a bug:** `POSIX::strftime('%Y', localtime(time()))` does **not** produce a trailing newline, so appending `"\n"` then chomping is a pointless round-trip. The chomp is a no-op; the `"\n"` append is wasted work. The generator is treating all command substitutions uniformly as if they produce newline-terminated output, which is only true for actual backtick/qx results, not for native Perl expressions.

**Preferred idiomatic Perl:**
```perl
say "Current date: " . POSIX::strftime('%Y', localtime());
```
Or with import:
```perl
use POSIX qw(strftime);
say "Current date: " . strftime('%Y', localtime());
```

**IR-fixable?** **Yes**, but requires two things to align:

1. The generator must produce an `IrExpr::Call { func: "POSIX::strftime", args: [...] }` (or `IrExpr::Call { func: "strftime", ... }` with imports in the program header), **not** a `RawExpr` containing the whole `do { ... }` mess.
2. The IR backend's pretty-printer for `Call` would then emit `POSIX::strftime(...)` directly — clean, no chomp wrapper.

The chomp wrapper is an artifact of the text-based generator treating every backtick substitution as an opaque string. Once the generator recognizes that `date +%Y` can be translated to a native Perl call, it should emit a semantic IR node (`Call`), and the backend will format it without the chomp infrastructure.

**IR node involved:** `IrStmt::Output` → `IrExpr::Call` (or, if embedded in an interpolated string, `IrExpr::Interpolate` containing the `Call`).

---

#### Pattern B: The basename inline (6 lines of do/regex/chomp for a simple operation)

**Generated:**
```perl
say "Current directory: " . (do { my $_chomp_temp = do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $_chomp_temp; $_chomp_temp; });
```

**Problem:** `basename $(pwd)` is translated into:
- A `do` block wrapping `use Cwd` at runtime + `getcwd()`
- A regex substitution to strip the directory portion
- A chomp
- An outer chomp-dance wrapper

That's ~8 lines of Perl for something that could be 1–2 lines.

**Preferred idiomatic Perl:**
```perl
use File::Basename qw(basename);
use Cwd qw(getcwd);
say "Current directory: " . basename(getcwd());
```
Or even simpler (no module needed):
```perl
say "Current directory: " . (split '/', getcwd())[-1];
```
Or using the `Cwd` module:
```perl
say "Current directory: " . (split '/', Cwd::getcwd())[-1];
```

**IR-fixable?** **Partially.** The structural issue (do-blocks, $CHILD_ERROR, chomp wrapper) is IR-fixable — if the generator produces a semantic IR node like `Call { func: "basename", args: [Call { func: "getcwd", args: [] }] }`, the backend can emit clean code. The `$CHILD_ERROR = 0` inside the expression is dead-store noise that an optimization pass could eliminate.

**However**, the decision to inline the basename logic as a regex rather than calling `File::Basename::basename` is a **generator-level choice**, not an IR formatting decision. The IR doesn't know that `$basename_path =~ s{.*/}{}msx` is a basename operation. If the generator produces a `RawExpr` containing the inline regex, the backend just passes it through. To get clean output, the generator must produce a `Call` to `File::Basename::basename` (or a simple `split` expression). This requires changing the generator logic.

**IR node involved:** If generator is fixed to use `Call`, then `IrExpr::Call { func: "File::Basename::basename", args: [...] }`. The backend's `emit_expr` for `Call` would format it cleanly.

---

#### Pattern C: Triple declaration (`my $v; my @v; my %v;`) for every variable

**Generated:**
```perl
my $current_date;
my @current_date;
my %current_date;
my $current_dir;
my @current_dir;
my %current_dir;
```

**Problem:** Every scalar variable gets all three sigil forms declared — scalar, array, and hash. Only the scalar form is ever used. This bloats the code by 3× for variable declarations and confuses the reader (it suggests @current_date might be used as an array, but it never is).

**Preferred idiomatic Perl:**
```perl
my $current_date;
my $current_dir;
```

**IR-fixable?** **Yes.** The generator currently emits these as raw text. If it produced `IrStmt::Declare { vars: [Decl { name: "current_date", sigil: Scalar }] }`, the IR backend would emit `my $current_date;` — one line, one sigil. Dead declaration elimination (an IR optimization pass) could also remove unreferenced `@var` / `%var` declarations if the generator insists on emitting them, but the cleanest fix is for the generator to emit only the needed sigil declarations.

**IR node involved:** `IrStmt::Declare` with a single `Decl`.

---

#### Pattern D: `print` / `say` inconsistency

**Generated:**
```perl
print "=== Basic Command Substitution ===\n";
say "Current date: " . ...;
say "Current directory: " . ...;
say "Stored date: $current_date";
say "Stored directory: $current_dir";
print "=== Basic Command Substitution Complete ===\n";
```

**Problem:** The first and last lines use `print "...\n"` while the middle lines use `say`. Both are semantically equivalent, but inconsistent style. `say` is generally preferred for output that always ends with a newline (Perl 5.10+).

**Preferred idiomatic Perl:**
```perl
say "=== Basic Command Substitution ===";
...
say "=== Basic Command Substitution Complete ===";
```

**IR-fixable?** **Yes.** The IR has `Output { value, newline: true }`. The backend already checks `newline` and emits `say` when true. The inconsistency exists because the generator currently emits raw text for these lines (not through the IR Output node). Once the generator produces `IrStmt::Output { value: Str("=== ... ==="), newline: true }`, the backend will consistently emit `say`.

**IR node involved:** `IrStmt::Output { newline: true }`.

---

#### Pattern E: `require POSIX` inside a `do` block instead of top-level `use`

**Generated:**
```perl
$current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time())) . "\n"
};
```

**Problem:** The `POSIX` module is loaded at runtime via `require` inside the expression, every time the assignment executes. This is wasteful — it should be loaded once at compile time with `use POSIX;` at the top of the program, or better yet `use POSIX qw(strftime);`.

**Preferred idiomatic Perl:**
```perl
use POSIX qw(strftime);
...
$current_date = strftime('%Y%m', localtime());
```

**IR-fixable?** **Yes.** The IR program model has `imports: Vec<String>` that are emitted at the top of the program. If the generator produces a `Call { func: "POSIX::strftime", ... }` and adds `"POSIX qw(strftime)"` to the imports list (or the IR backend infers the import from the function name), the backend emits clean code with a proper `use` statement at the top and just the function call inline.

**IR node involved:** `IrProgram::imports` + `IrExpr::Call`.

---

#### Pattern F: `use Cwd` inside `do` block / runtime `require` for Cwd

**Generated:**
```perl
my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
```

**Problem:** Same as Pattern E — `use Cwd` inside a `do` block causes a compile-time check embedded in a runtime expression. It works, but is stylistically poor. It also interleaves `$CHILD_ERROR = 0` in the middle of fetching the current directory.

**Preferred idiomatic Perl:**
```perl
use Cwd qw(getcwd);
...
my $basename_path = getcwd();
```

**IR-fixable?** **Yes.** Same reasoning as Pattern E: if the generator produces `Call { func: "getcwd" }` with `"Cwd qw(getcwd)"` in imports, the backend emits clean code.

**IR node involved:** `IrProgram::imports` + `IrExpr::Call`.

---

#### Pattern G: `$CHILD_ERROR = 0` after every statement

**Generated (5 occurrences):**
```perl
$CHILD_ERROR = 0;
```
appears after `say "Current date:..."`, after `say "Current directory:..."`, after `say "Stored date:..."`, after `say "Stored directory:..."`.

**Problem:** `$CHILD_ERROR` is reset to 0 after every statement that **doesn't** involve an external command. This is cargo-cult code — it's only meaningful after actual pipeline or system call execution, not after `say` or `print`. It adds visual noise and serves no purpose.

**Preferred idiomatic Perl:** Omit entirely for non-command statements. Only set `$CHILD_ERROR` after actual external command execution (where it's set from `$?`).

**IR-fixable?** **Yes, via dead store elimination.** An IR optimization pass can analyze that `$CHILD_ERROR` is assigned but never read before the next assignment (or program exit), and remove the store. However, the root cause is the generator emitting these unconditionally. If the generator produced an `IrStmt::System` (which includes the `$CHILD_ERROR = $? >> 8;` internally), the backend would only emit the reset for actual commands. For non-command statements, the generator should produce `IrStmt::Output` or `IrStmt::Assign` — neither of which includes `$CHILD_ERROR` reset.

**IR node involved:** A dead-assignment elimination pass on `IrStmt::Assign { targets: [AssignTarget { var: "CHILD_ERROR" }] }` would remove these.

---

#### Pattern H: Unused imports (`Carp`, `English`, `locale`, `IPC::Open3`)

**Generated:**
```perl
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
```

**Problem:** These four imports are present in every generated file, regardless of whether they're used. None are referenced anywhere in this script. `Carp` (for stack traces), `English` (for verbose variable names), `locale` (for locale handling), and `IPC::Open3` (for IPC) are all irrelevant to a script that just prints dates and directory names.

**Preferred idiomatic Perl:** Omit completely. Only import what is actually used.

**IR-fixable?** **Yes, via import minimization.** An IR optimization pass can scan all IR nodes for referenced functions/variables and remove unused imports from `IrProgram::imports`. The generator currently adds these unconditionally as raw text at the top. If the generator instead populated `IrProgram::imports` based on analysis of what IR nodes are present, unused imports would never be added.

**IR node involved:** `IrProgram::imports` — a pass can filter to only imports that are actually referenced.

---

#### Pattern I: Unused variables (`$main_exit_code`, `$ls_success`, `$__set_e`, `$output`)

**Generated:**
```perl
my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
```

**Problem:** These four "infrastructure" variables are declared in every generated program, but none are used in this script. They bloat the output and confuse the reader.

**Preferred idiomatic Perl:** Omit entirely.

**IR-fixable?** **Yes, via dead code elimination.** An IR optimization pass can remove `IrStmt::Declare` for variables that are never referenced. The generator currently emits these as raw text unconditionally in `mod.rs`. If they were represented as `IrStmt::Declare` nodes, a DCE pass could remove them.

**IR node involved:** `IrStmt::Declare` — can be removed if the variable never appears in any other IR node.

---

#### Pattern J: `exit $main_exit_code` when it's always 0

**Generated:**
```perl
exit $main_exit_code;
```

**Problem:** `$main_exit_code` is initialized to 0 and never modified. The exit is equivalent to `exit 0;` which is also the implicit default. This line is dead code.

**Preferred idiomatic Perl:** Omit entirely (Perl exits with 0 by default), or use `exit 0;` if explicit.

**IR-fixable?** **Yes.** The `ir_to_perl()` function in `ir.rs` unconditionally appends `exit $main_exit_code;`. An IR optimization pass could recognize that the variable is always 0 and emit `exit 0;` or omit it. Better: the generator should not emit `IrStmt::Declare { var: "main_exit_code" }` unless it's actually used.

**IR node involved:** `IrProgram::stmts` — the final exit statement is hardcoded in `ir_to_perl()`, not an IR node. It could be removed or made conditional.

---

#### Pattern K: String concatenation vs comma in `say`

**Generated:**
```perl
say "Current date: " . (do { ... });
```

**Problem:** The `say` builtin accepts a list, so concatenating with `.` is unnecessary. Using a comma is cleaner and avoids constructing an intermediate string.

**Preferred idiomatic Perl:**
```perl
say "Current date: ", POSIX::strftime('%Y', localtime());
```

**IR-fixable?** **Yes.** The IR backend controls how `Output { value, newline: true }` is formatted. If the value is an `Interpolate` consisting of a literal part followed by an expression, the backend could emit comma-separated arguments to `say` instead of concatenation. This is a style choice in the pretty-printer.

**IR node involved:** `IrStmt::Output` — the backend decides to concatenate with `.` or use comma-separated args.

---

### 4. Unnecessarily Verbose Translations

These are places where the generated code wraps simple operations in complex control structures — prime candidates for IR-based simplification.

| Location | What it does | Generated lines | What it should be |
|---|---|---|---|
| `date +%Y` in echo | Get year | 5 lines: outer `do { my $_chomp_temp = do { require POSIX; POSIX::strftime + "\n" }; chomp; }` | `POSIX::strftime('%Y', localtime())` (1 line) |
| `basename $(pwd)` | Get dir name | 9 lines: outer chomp-dance, inner do with use Cwd, getcwd, regex, chomp | `basename(getcwd())` or `(split '/', getcwd())[-1]` (1 line) |
| `current_date=...` | Assign date | 7 lines: 3 declarations + do-block with require POSIX + "\n" | `my $current_date = strftime('%Y%m', localtime())` (1 line) |
| `current_dir=...` | Assign dir | 10 lines: 3 declarations + do-block with use Cwd, regex, chomp | `my $current_dir = basename(getcwd())` (1 line) |
| `$CHILD_ERROR = 0` | Reset error | 5 lines scattered through script | 0 lines — not needed after non-command statements |
| Infrastructure vars | Setup | 5 lines: `$main_exit_code`, `$ls_success`, `$__set_e`, `$output`, `our $CHILD_ERROR` | 0 lines — none used |
| Unused imports | Boilerplate | 4 lines: Carp, English, locale, IPC::Open3 | 0 lines — not needed |

The chomp-dance alone accounts for **~15 lines** of unnecessary wrapping in this 12-line shell script. The triple declarations add another **6 lines** of bloat. Unused infrastructure adds **9 lines**. In total, the generated code is **40 lines** of Perl for what should be **~13 lines** of clean, idiomatic Perl — a 3× bloat factor.

---

### 5. Summary

| Pattern | IR-fixable? | How |
|---|---|---|
| A. Chomp-dance wrapper | Yes | Generator produces `Call` IR node instead of `RawExpr`; backend emits clean call |
| B. Inline basename regex | Partial | Do/chomp wrapper is IR-fixable; switching to `File::Basename` requires generator change |
| C. Triple declaration | Yes | Generator produces `Declare { sigil: Scalar }`; backend emits one `my $var;` |
| D. print/say inconsistency | Yes | `Output { newline: true }` always formats as `say` in backend |
| E. `require POSIX` inline | Yes | Generator adds import to `IrProgram::imports`; backend emits `use POSIX;` at top |
| F. `use Cwd` inside do | Yes | Same as E — lift to top-level imports |
| G. `$CHILD_ERROR = 0` spam | Yes | Dead store elimination in IR optimization pass |
| H. Unused imports | Yes | Import minimization pass on `IrProgram::imports` |
| I. Unused variables | Yes | Dead code elimination on `IrStmt::Declare` |
| J. `exit $main_exit_code` | Yes | Make final exit conditional in `ir_to_perl()` |
| K. `.` vs `,` in say | Yes | Style choice in backend's `emit_stmt` for `Output` |

**Patterns that require generator logic changes (not just IR pretty-printing):**

- **B (basename inlining):** The generator chooses to emit inline regex instead of calling `File::Basename::basename`. This is a semantic translation decision, not a formatting decision. The IR backend cannot know that `s{.*/}{}msx` is a basename operation.
- **E/F (dynamic require vs static use):** The generator chooses to emit `require POSIX;` and `use Cwd;` inside `do` blocks rather than adding them to the top-level imports. If the generator populated `IrProgram::imports` instead, the backend would emit clean top-level `use` statements.
- **The fundamental cause of pattern A (chomp-dance):** The generator uses a uniform template for all command substitutions: "append `"\n"`, capture, chomp, return." When the command is translated to a native Perl call (like `date` → `POSIX::strftime`), this template is still applied, producing the absurd `append("\n"); chomp()` round-trip. The generator needs to distinguish between "native Perl expression" (no newline) and "qx{} capture" (has trailing newline) and omit the chomp infrastructure for the former.

**Most impactful IR optimization pass:** A **chomp-dance simplifier** that recognizes the pattern:
```
do { my $_chomp_temp = do { EXPR . "\n" }; chomp $_chomp_temp; $_chomp_temp; }
```
and transforms it to just `EXPR`. This alone would cut ~50% of the verbosity in this example.