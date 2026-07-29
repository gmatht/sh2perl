Let me also look at the project structure and the actual generator code for subshells and pipelines to understand how they produce the output.Now let me look at the IR module to see which IR node types are currently defined.Now I have all the information I need. Here is the idiom review.

---

## Idiom Review: `examples/006_misc.sh` → Generated Perl

### 1. Source and Generated Code

**Original shell script (`examples/006_misc.sh`):**
```bash
#!/usr/bin/env bash

echo "== Subshell =="
( echo inside-subshell )

echo "== Simple pipeline =="
echo "alpha beta" | grep beta



echo "exit: $?"
```

**Generated Perl (from current generator):**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

print "== Subshell ==\n";
do {
    local %ENV = %ENV;
    print "inside-subshell\n";
    q{};
};
print "== Simple pipeline ==\n";
# Original bash: echo "alpha beta" | grep beta
my $output_0 = q{};
print $output_0, "\n";
print "exit: ${\($? >> 8)}\n";

exit $main_exit_code;
```

---

### 2. Non-idiomatic Patterns

**Pattern A — Boilerplate variable declarations that are never used**

```perl
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;
```

- `$main_exit_code` is set to `0`, never modified, and only used in `exit $main_exit_code`. Since Perl exits with status 0 by default, both the variable and the `exit` are dead code.
- `$output` is declared, initialized to `q{}`, and **never referenced again**.
- `our $CHILD_ERROR` is declared but never used; the code references `$?` directly (line 14: `${\($? >> 8)}`).

**IR-fixable? Yes.** If the generator emitted `Declare` IR nodes for these variables, `optimize_stmts()` already performs dead-declaration elimination: when a `Declare` has no initializer and none of its variables appear in `collect_referenced_vars()`, it is dropped.  The variable `$main_exit_code` would survive if referenced in an `exit` statement, but the backend could also recognise that `exit $main_exit_code` where `$main_exit_code` is never reassigned is equivalent to just letting Perl fall off the end.  The IR node involved is `IrStmt::Declare` (and `IrStmt::Return` / bare `exit` for the termination).

**Cleaned-up output (dead declarations removed, implicit exit):**
```perl
#!/usr/bin/env perl
use strict;
use warnings;

print "== Subshell ==\n";
do {
    local %ENV = %ENV;
    print "inside-subshell\n";
    q{};
};
print "== Simple pipeline ==\n";
# Original bash: echo "alpha beta" | grep beta
my $output_0 = q{};
print $output_0, "\n";
print "exit: ${\($? >> 8)}\n";
```

*(The fix would also drop the blank line after `use warnings;` when there are no imports.)*

---

**Pattern B — `use English` with unused imported names**

```perl
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
```

Only `$?` is ever consulted (line 14: `${\($? >> 8)}`).  The variables `$ERRNO`, `$EVAL_ERROR`, `$INPUT_RECORD_SEPARATOR`, `$OS_ERROR`, `$PROGRAM_NAME` are all dead imports.  Moreover, `$?` is a built-in Perl variable that does not require `use English` at all — `$CHILD_ERROR` (its `English` alias) is never used.

**IR-fixable? Yes.**  The `IrProgram` struct carries an `imports: Vec<String>` field that, per the IR design, is *"auto-derived from constructs used"*.  A semantic scan of the IR tree would reveal that only the built-in `$?` is referenced, so `use English` can be omitted entirely.  The IR node involved is the top-level `IrProgram::imports`.

**Cleaned-up output (no `use English`):**
```perl
#!/usr/bin/env perl
use strict;
use warnings;

print "== Subshell ==\n";
...
```

---

**Pattern C — Subshell wrapped in `do { local %ENV = %ENV; ... q{}; }`**

```perl
do {
    local %ENV = %ENV;
    print "inside-subshell\n";
    q{};
};
```

This is a poor fit for the original `( echo inside-subshell )`:

1. `local %ENV = %ENV` snapshots `%ENV` even though nothing inside the subshell touches the environment.  This is pure overhead.
2. The `do { ... }` block exists solely to provide localisation scope, but a subshell that neither assigns variables nor modifies `%ENV` nor `cd`s needs no scoping at all — the body can be emitted inline.
3. `q{};` is a no-op expression that ensures the `do` block returns an empty string (the comment in `subshell_commands.rs` explains this is to avoid spurious numeric output from `$CHILD_ERROR = 0`).  Since the return value of the `do` block is never captured, this is dead code.

**IR-fixable? Partially.**  The current IR has no `Subshell` node.  To fix this properly, a new node such as:

```rust
IrStmt::Subshell {
    body: Vec<IrStmt>,
    localize_env: bool,     // true only if body modifies %ENV
    localize_vars: Vec<String>,  // variables that need `my` shadowing
}
```

would let the backend decide how much scaffolding is needed.  When `localize_env` is false and `localize_vars` is empty, the backend can emit just the body statements without any `do` wrapper.  Determining `localize_env` requires a semantic analysis pass *before* IR construction (i.e., in the generator or a ShIR pass), not in the IR backend itself.  However, the trailing `q{};` could be eliminated by DCE on the IR if it were represented as a proper statement rather than `RawText`.

**Cleaned-up output (trivial subshell flattened):**
```perl
print "== Subshell ==\n";
print "inside-subshell\n";
print "== Simple pipeline ==\n";
...
```

---

**Pattern D — Pipeline replaced by empty stub**

```perl
# Original bash: echo "alpha beta" | grep beta
my $output_0 = q{};
print $output_0, "\n";
```

The pipeline never executes.  `$output_0` is initialised to the empty string, printed, and the script continues.  The original pipeline `echo "alpha beta" | grep beta` would output `alpha beta` (since `grep beta` matches); the generated code outputs a blank line instead.

**IR-fixable? No.**  This is a generator logic bug: `pipeline_commands.rs` fails to emit any execution code for this particular `echo | grep` pipeline.  The generator must be fixed to either:

- Recognise `echo | grep` as a special case and inline the logic (`print "alpha beta" if "alpha beta" =~ /beta/`), or
- Emit proper pipeline IR nodes (`IrStmt::Pipeline { stages: [...], capture: None }`), or
- Fall back to `qx{...}` execution.

Whichever approach is taken, the fix must happen in the generator (or in a semantic analysis pass that feeds the IR), not in the pretty-printer.  The IR backend can only format what it receives.

---

**Pattern E — Transliteration comment in output**

```perl
# Original bash: echo "alpha beta" | grep beta
```

This comment is a debugging aid that (a) reveals the transliteration nature of the code and (b) documents a pipeline that does not actually execute.  Clean generated Perl should not carry "original bash" annotations.

**IR-fixable? Partially.**  If comments were stored as metadata on `IrStmt::Pipeline` (a new field like `source_comment: Option<String>`), the backend could choose to emit them or not.  As it stands, the comment is embedded in `RawText` and is invisible to the IR.

---

**Pattern F — `${\($? >> 8)}` interpolation trick**

```perl
print "exit: ${\($? >> 8)}\n";
```

This uses Perl's `${\(EXPR)}` escape to embed arbitrary Perl expressions inside double-quoted strings.  While valid, it is obscure and rarely seen in hand-written Perl.  The pattern evolved from the need to embed `$? >> 8` in a string, but there are clearer alternatives.

**IR-fixable? Yes.**  This expression would be represented in the IR as:

```rust
IrExpr::Interpolate(vec![
    InterpPart::Lit("exit: ".to_string()),
    InterpPart::Expr(IrExpr::BinOp {
        lhs: Box::new(IrExpr::Var("?".to_string(), Sigil::Scalar)),
        op: BinOpKind::ShiftR,
        rhs: Box::new(IrExpr::Int(8)),
    }),
])
```

The `ir_expr_to_perl()` function handles `Interpolate` by iterating over parts.  When it encounters `InterpPart::Expr` that is not a simple `Var`, it currently wraps it as `${\(...)}`.  A style rule could be added to choose between:

| Style | Output |
|---|---|
| **Current (interpolation)** | `"exit: ${\($? >> 8)}"` |
| **Concatenation** | `"exit: " . ($? >> 8)` |
| **`printf`** | `printf "exit: %d\n", $? >> 8` |

The `printf` form is the most idiomatic for this specific case (format string with one integer).  The backend could detect the pattern `Interpolate([Lit, Expr])` with a trailing `Lit("\n")` inside an `Output { newline: false }` and emit `printf` instead.

**Cleaned-up output (printf style):**
```perl
printf "exit: %d\n", $? >> 8;
```

---

**Pattern G — `print "..."\n` instead of `say`**

```perl
print "== Subshell ==\n";
print "inside-subshell\n";
print "== Simple pipeline ==\n";
```

Each of these emits a string with an explicit `\n` via `print`.  Perl 5.10+ provides `say`, which appends `\n` automatically.

**IR-fixable? Yes.**  The IR design document explicitly calls this out.  The `IrStmt::Output { value: IrExpr, newline: bool, target: ... }` node controls this.  When `newline` is `true`, the backend currently emits `print EXPR, "\n";` or `print "EXPR\n";`.  Changing the backend to emit `say EXPR;` when `newline == true` is a one-line change in `emit_stmt()`.

**Cleaned-up output:**
```perl
say "== Subshell ==";
say "inside-subshell";
say "== Simple pipeline ==";
```

(Note: requires adding `use feature 'say';` or `use v5.10;` to the imports.  The `imports` field in `IrProgram` makes this straightforward.)

---

**Pattern H — Spurious blank line from broken pipeline**

```perl
my $output_0 = q{};
print $output_0, "\n";
```

Because `$output_0` is empty, this prints a blank line that does not correspond to anything in the original shell script.  This is a downstream consequence of Pattern D (the missing pipeline execution) and cannot be fixed independently.

**NOT IR-fixable** — requires fixing the generator logic for pipelines.

---

### 3. Unnecessarily Verbose Translations

These are places where the generated code wraps a simple operation in far more scaffolding than needed.

**V1 — Subshell of a single `echo`**

| Aspect | Original | Generated |
|---|---|---|
| Lines of code | 1 (`( echo ... )`) | 6 (`do { local %ENV = %ENV; print "...\n"; q{}; };`) |
| Control structures | implicit fork (shell) | `do` block + environment localisation + no-op return |
| What's actually needed | `print "inside-subshell\n"` (3 words) | 6 lines with 4 levels of semantic wrapping |

The generator treats every subshell as if it might modify the environment, fork background processes, or capture output.  For the common case of a simple grouping `( cmd )`, this is enormous over-engineering.  An IR-based optimisation could flatten the subshell when analysis shows no side effects requiring isolation.

**V2 — Pipeline infrastructure for a two-command pipe** (when correctly implemented)

Even when pipelines *do* work, the current generator infrastructure (visible in `pipeline_commands.rs`) produces reams of code: unique IPC variables, `open3` calls, `while` read loops, `waitpid`, `$CHILD_ERROR` tracking — easily 20+ lines for a simple `echo | grep`.  The IR design acknowledges this problem in its "Style rules" table:

> *Pipeline with 1 stage* → wraps in vars, pipes, etc. → just the stage body

For a two-command pipeline where the output is immediately printed, the entire pipeline could be replaced by a single `qx{...}` or, even better, inlined Perl:

```perl
# Instead of the full open3 scaffold:
print "alpha beta\n" if "alpha beta" =~ /beta/;
```

The `IrStmt::Pipeline` node already has a `capture: Option<String>` and `cmd_str: Option<String>` field, which the IR design intended for exactly this optimisation: when the pipeline is used in a print-on-stdout context, emit a single `qx{...}` call instead of simulating pipes in Perl.

**V3 — `$main_exit_code` variable for a script that never fails**

The entire pattern:
```perl
my $main_exit_code = 0;
...
exit $main_exit_code;
```

is for a script where no command ever assigns to `$main_exit_code`.  The generator always emits this scaffolding regardless of whether the script has any conditional logic.  An IR optimisation could eliminate it when `$main_exit_code` is never written to, leaving Perl's default implicit `exit(0)`.

**V4 — Unused `$output` variable that is never read**

`my $output = q{};` is always emitted but never assigned or read.  This appears to be leftover from an earlier design where all output was accumulated into `$output`.  It is pure noise.

---

### 4. Summary Table

| Pattern | IR-fixable? | IR Node(s) Involved | Requires Generator Logic Change? |
|---|---|---|---|
| **A** Dead variable declarations | Yes | `Declare`, `optimize_stmts` DCE | No (already in `optimize_stmts`) |
| **B** Unused `use English` imports | Yes | `IrProgram::imports` | No (auto-derive imports) |
| **C** Subshell scaffolding | Partially | New `Subshell` node + semantic analysis | Yes — must determine `localize_env` flag before IR construction |
| **D** Missing pipeline execution | **No** | N/A | Yes — generator must emit pipeline code |
| **E** Transliteration comment | Partially | Metadata on `Pipeline` node | Yes — generator should store comment as metadata |
| **F** `${\(...)}` interpolation | Yes | `Interpolate` / `InterpPart::Expr` | No (style choice in `ir_expr_to_perl()`) |
| **G** `print` vs `say` | Yes | `Output { newline: true }` | No (one-line backend change) |
| **H** Spurious blank line | **No** (depends on D) | N/A | Yes — requires pipeline fix |

**Unnecessarily verbose translations:**
- **V1** — Subshell wrapping (6 lines → 1 line)
- **V2** — Pipeline scaffolding (20+ lines → 1 line `qx{...}` or inlined Perl)
- **V3** — `$main_exit_code` boilerplate for trivial scripts
- **V4** — Unused `$output` variable

The three patterns that most strongly signal "transliteration, not native Perl" are **C** (subshell scaffolding), **F** (`${\(...)}` trick), and **V2** (pipeline over-engineering).  All three would be automatically resolved once the generator emits semantic IR nodes instead of `RawText`, because the IR backend can then apply style rules and optimisation passes uniformly.  Patterns **D** and **H** (broken pipeline) are the most serious issues — they produce incorrect output, not just unidiomatic code — and require generator logic changes regardless of the IR.