Let me start by understanding the project and the IR design document.Let me look at the generator code to understand how these patterns are produced.Let me look at more of the generator to understand the full picture.Let me look at the IR module and the overall generator mod.rs for context.Let me run the actual converter to see the current generated output.Now I have a thorough understanding of the code generator and the IR design. Let me write the idiom review.

---

# Idiom Review: Shell-to-Perl Translation of `examples/007_cat_EOF.sh`

## 1. Source and Generated Code

### Original shell script (`examples/007_cat_EOF.sh`)
```bash
cat <<EOF
alpha
beta
gamma ...
EOF

cat <<FISH
oyster
snapper
salmon
FISH

echo "Fin. That is all folks."

echo "exit: $?"
```

### Generated Perl code
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "alpha
beta
gamma ...
";
print "oyster
snapper
salmon
";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```

---

## 2. Non-Idiomatic Patterns

### Pattern A — Multiline string literals with embedded real newlines

**Generated:**
```perl
print "alpha
beta
gamma ...
";
```

**Problem:** Embedding literal newlines inside double-quoted strings in Perl source is syntactically valid but highly non-idiomatic. It breaks regardless of indentation, confuses syntax highlighters, and risks trailing whitespace corruption. Perl programmers write this as:

**Idiomatic Perl:**
```perl
print "alpha\nbeta\ngamma ...\n";
```
or using a Perl heredoc:
```perl
print <<'END';
alpha
beta
gamma ...
END
```

The root cause is in `cat.rs`: function `heredoc_body_to_perl_interp()` escapes `\`, `"`, `\t`, and `\r`, but deliberately **does not** escape `\n`. This produces a `"..."` literal containing bare newline characters. The function comment says it preserves `$` and `@` for interpolation, but it misses the fact that newline characters belong as `\n` escape sequences in idiomatic Perl.

**IR-fixable?** **YES.** If the generator emitted this as:
```rust
IrStmt::Output {
    value: IrExpr::Str(body_string, StrStyle::DoubleQuoted),
    newline: false,
}
```
instead of raw `format!("print {};\n", body_lit)`, then `ir_expr_to_perl()` for `StrStyle::DoubleQuoted` already handles `\n` → `\\n` escaping. The output would automatically become:
```perl
print "alpha\nbeta\ngamma ...\n";
```
The IR node involved is `IrExpr::Str(String, StrStyle::DoubleQuoted)` → its `ir_expr_to_perl()` match arm.

---

### Pattern B — `say` emitted without importing `feature 'say'`

**Generated:**
```perl
say "Fin. That is all folks.";
```

**Problem:** `say` requires `use feature 'say'` or `use 5.010` (or `use v5.10`). Without it, compilation fails with `String found where operator expected`. The echo generator in `simple_commands.rs` uses:
```rust
let ir_stmt = crate::ir::IrStmt::Output {
    value: crate::ir::IrExpr::RawExpr(...),
    newline: true,
    target: None,
};
output.push_str(&crate::ir::stmt_to_perl(&ir_stmt, 0));
```
This calls `stmt_to_perl()` (a piecemeal helper), not `ir_to_perl()` (the full-program formatter that auto-adds `use feature 'say'`). So `say` is emitted without the feature declaration.

**Idiomatic Perl:**
```perl
use feature 'say';
say "Fin. That is all folks.";
```

**IR-fixable?** **YES.** Two approaches:
1. If the echo generator used the full `IrProgram`/`ir_to_perl()` pipeline, the `prog_uses_say()` check in `ir_to_perl()` would detect `Output { newline: true }` nodes and automatically emit `use feature 'say'`. The node is `IrStmt::Output { newline: true }` — the `ir_to_perl()` function already has the logic at lines 321-324 of `ir.rs`.
2. Even with piecemeal `stmt_to_perl()`, the backend could introduce a tracking mechanism that accumulates required imports and emits them at the top.

**Cleaned-up output with IR:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
say "alpha\nbeta\ngamma ...";
say "oyster\nsnapper\nsalmon";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```
(Note: the heredoc output also becomes `say` for consistency — see Pattern C.)

---

### Pattern C — Inconsistent output style: `print` for heredocs vs `say` for `echo`

**Generated:**
```perl
print "alpha\nbeta\ngamma ...\n";    # heredoc path → raw print
say "Fin. That is all folks.";       # echo path → Output IR node → say
```

**Problem:** Two fundamentally similar operations (output a string with a trailing newline) use different Perl idioms depending on which shell command triggered them (`cat` → `print`, `echo` → `say`). This is because the `cat` generator in `cat.rs` emits `print` directly while the `echo` generator in `simple_commands.rs` emits `IrStmt::Output { newline: true }`.

**Idiomatic Perl:** Both should use `say` with `\n` escapes:
```perl
say "alpha\nbeta\ngamma ...";
say "oyster\nsnapper\nsalmon";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```

**IR-fixable?** **YES.** If `cat.rs` emitted `IrStmt::Output { newline: true }` (matching the echo pattern) instead of raw `format!("print {};\n", body_lit)`, the backend would consistently produce `say` for all output-with-newline statements. The IR node is the same `IrStmt::Output { newline: true }`.

---

### Pattern D — Unnecessary `use English` imports

**Generated:**
```perl
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
```

**Problem:** Five English module variables are imported, but **none** of them appear in the generated code body:
- `$ERRNO` — not used
- `$EVAL_ERROR` — not used
- `$INPUT_RECORD_SEPARATOR` — not used (no line-at-a-time I/O happens)
- `$OS_ERROR` — not used
- `$PROGRAM_NAME` — not used

The generated code references `$?` directly (Pattern E), not `$CHILD_ERROR`. The entire `use English` line is dead weight.

**Idiomatic Perl:** Either omit it entirely (since no English variables are used), or import only what's needed:
```perl
use English qw(-no_match_vars $CHILD_ERROR);   # if $CHILD_ERROR were used
```
or simply omit the module if nothing from `English` is referenced.

**IR-fixable?** **YES.** An IR-based optimizer can perform **import minimization**. The IR tracks every variable reference across all statements. If no IR node references `$ERRNO`, `$OS_ERROR`, etc., the backend skips emitting them. The `gen.mod.rs`'s `needs_english_import()` heuristic at line 176 already attempts this, but it returns `true` too eagerly (it likely sees `$CHILD_ERROR` or `$OS_ERROR` somewhere in a RawText fragment). With proper IR nodes, the analysis would be precise:

- No `IrExpr::Var("ERRNO", ...)` → no `$ERRNO` import
- No `IrExpr::Var("OS_ERROR", ...)` → no `$OS_ERROR` import
- etc.

**Cleaned-up output with IR:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
say "alpha\nbeta\ngamma ...";
say "oyster\nsnapper\nsalmon";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```

---

### Pattern E — `($? >> 8)` instead of `$CHILD_ERROR`

**Generated:**
```perl
say "exit: " . ($? >> 8);
```

**Problem:** `$?` in Perl is the 16-bit wait status (exit code << 8 + signal). The expression `$? >> 8` extracts the exit code. This is correct but:
- It's opaque — a reader has to remember the `$?` encoding
- It's inconsistent with the codebase's own `$CHILD_ERROR` variable (declared as `our $CHILD_ERROR` in the generator's header, see `gen.mod.rs` line ~215) which is meant to hold `$? >> 8`
- It doesn't use the English module's alias `$CHILD_ERROR` even though the English module is imported

**Idiomatic Perl:**
```perl
say "exit: " . $CHILD_ERROR;    # with use English
# or equivalently:
say "exit: " . ($? >> 8);       # without English
```

Since the English module is already in the import list, `$CHILD_ERROR` would be more consistent. However, the root semantic issue — that `echo "exit: $?"` references the shell's `$?` — is correctly handled: the generated code should capture the exit status of the *last command* (which is 0 after `echo "Fin. That is all folks."` since both use `say`/`print`), so `$? >> 8` yields 0. But the code doesn't explicitly track `$?` — it falls through to whatever Perl's `$?` happens to be at that point.

Actually, checking the generated code: there is **no preceding external command** that sets `$?`. The `print` and `say` calls are Perl builtins that don't touch `$?`. So `$?` will be whatever it inherited from the environment (likely 0), which happens to be correct here by accident. For a robust translation, the generator should capture `$?` after the `echo "Fin. That is all folks."` and use that captured value in the second `echo`.

**IR-fixable?** **PARTIALLY.** The IR can help with the stylistic choice (`$? >> 8` vs `$CHILD_ERROR`):
- If the generator emits `IrExpr::Var("CHILD_ERROR", Sigil::Scalar)`, the backend prints `$CHILD_ERROR`.
- If the generator emits `IrExpr::BinOp { lhs: IrExpr::Var("?", Sigil::Scalar), op: ShiftR, rhs: IrExpr::Int(8) }`, the backend prints `($? >> 8)`.

However, the **semantic decision** of what captures `$?` at each point is a generator-level concern. The IR nodes represent "what value to emit" but the generator must decide when to snapshot `$?`. Right now the generator doesn't snapshot `$?` at all for `echo "exit: $?"` — it just emits `$? >> 8` which happens to work because no external commands have run. This is fragile.

**NOT IR-fixable** in the sense that the IR can't inject missing exit-code capture logic. The generator would need to emit something like:
```perl
my $exit_code = $? >> 8;   # capture after the previous command
say "exit: " . $exit_code;
```
or the shell `$?` reference needs to be mapped to the correct Perl exit-status expression at code-generation time. The IR can clean up the formatting once the generator makes that decision.

---

### Pattern F — Missing `use feature 'say'` causes compilation failure

This is related to Pattern B but worth calling out separately. The current generated code **does not compile** under `perl -c` because `say` is used without being enabled. Running the generated code produces:
```
String found where operator expected (Do you need to predeclare "say"?)
syntax error at -e line 1, near "say "Fin. That is all folks.""
```

**IR-fixable?** **YES** — as described in Pattern B, the `ir_to_perl()` function already has the auto-detection logic. The fix is to route all echo generation through the full `IrProgram` → `ir_to_perl()` pipeline instead of using piecemeal `stmt_to_perl()`.

---

### Pattern G — `use German` not used but imported

The `use locale;` import is mentioned in the boilerplate (line 180 of `gen.mod.rs`) but is **not present** in this particular output. That's correct — it's only emitted when `needs_locale()` returns true. But the `use English` import is emitted because `needs_english_import()` returns true. Let me verify why.Now I have all the information needed. Here is the full idiom review:

---

# Idiom Review: Shell-to-Perl Translation of `007_cat_EOF.sh`

## 1. Source vs. Generated Code

**Shell script:**
```bash
cat <<EOF
alpha
beta
gamma ...
EOF

cat <<FISH
oyster
snapper
salmon
FISH

echo "Fin. That is all folks."

echo "exit: $?"
```

**Generated Perl:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "alpha
beta
gamma ...
";
print "oyster
snapper
salmon
";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```

---

## 2. Non-Idiomatic Patterns

### Pattern A — Multiline string literals with bare newlines

**Generated:**
```perl
print "alpha
beta
gamma ...
";
```

**Why it's non-idiomatic:** Embedding literal (not escaped) newlines inside a `"..."` string is syntactically valid Perl but violates every Perl style guide. It breaks regardless of source indentation, confuses syntax highlighters, and risks trailing whitespace corruption. Perl programmers write this with `\n` escapes:

**Idiomatic Perl:**
```perl
print "alpha\nbeta\ngamma ...\n";
```

**IR-fixable? YES.** The root cause is in `cat.rs`, function `heredoc_body_to_perl_interp()`. It escapes `\`, `"`, `\t`, `\r` but **deliberately does not escape `\n`**, producing a `"..."` string with real newlines.

If the generator emitted an `IrExpr::Str(body, StrStyle::DoubleQuoted)` node instead of raw `format!("print {};\n", body_lit)`, then the `ir_expr_to_perl()` backend's match arm for `StrStyle::DoubleQuoted` (in `ir.rs` lines 650-670) would automatically convert newlines to `\n` escapes. The IR node is `IrExpr::Str(String, StrStyle::DoubleQuoted)` → `ir_expr_to_perl()`.

**Cleaned-up output with IR:**
```perl
print "alpha\nbeta\ngamma ...\n";
```

---

### Pattern B — `say` emitted without `use feature 'say'`

**Generated:**
```perl
say "Fin. That is all folks.";
```

**Why it's non-idiomatic / broken:** `say` is a Perl 5.10+ feature that requires `use feature 'say'` or `use 5.010`. Without it, compilation fails with `String found where operator expected` at the `say` keyword. The generated code will not run under `perl -c` or any modern Perl with `use strict`.

**IR-fixable? YES.** The echo generator in `simple_commands.rs` does use the IR: it constructs `IrStmt::Output { value: RawExpr(...), newline: true }` and calls `stmt_to_perl()`. But `stmt_to_perl()` is a piecemeal helper — it emits `say` without any import management. The full-program `ir_to_perl()` function already has a `prog_uses_say()` check (lines 321-324 of `ir.rs`) that auto-adds `use feature 'say'` when `Output { newline: true }` nodes are present. The fix is to route generation through the full `IrProgram` → `ir_to_perl()` pipeline so this auto-detection kicks in.

**Cleaned-up output:**
```perl
use feature 'say';
say "Fin. That is all folks.";
```

---

### Pattern C — Inconsistent output style: `print` for heredocs vs `say` for `echo`

**Generated:**
```perl
print "alpha\nbeta\ngamma ...\n";    # cat generator → raw format!("print ...")
say "Fin. That is all folks.";       # echo generator → IrStmt::Output { newline: true }
```

**Why it's non-idiomatic:** Two shell commands that both output a string with a trailing newline (`cat <<EOF` and `echo`) produce different Perl idioms. A reader sees inconsistency: why does one use `print` with `\n` while the other uses `say`? Both should use `say`.

**IR-fixable? YES.** The `cat` generator in `cat.rs` emits raw `format!("print {};\n", body_lit)` rather than an IR node. If it emitted `IrStmt::Output { value: IrExpr::Str(...), newline: true }` (same as echo already does), the backend would consistently produce `say` for all output-with-newline statements. The IR node is `IrStmt::Output { newline: true }` — the same one the echo path already uses.

**Cleaned-up output with IR:**
```perl
say "alpha\nbeta\ngamma ...";
say "oyster\nsnapper\nsalmon";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```

---

### Pattern D — Unnecessary `use English` import bloat

**Generated:**
```perl
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
```

**Why it's non-idiomatic:** Five English module variables are imported but **none** of them appear anywhere in the generated code body:

| Variable | Used in generated code? |
|---|---|
| `$ERRNO` | No |
| `$EVAL_ERROR` | No |
| `$INPUT_RECORD_SEPARATOR` | No |
| `$OS_ERROR` | No |
| `$PROGRAM_NAME` | No |

The code references `$?` directly (Pattern E), not `$CHILD_ERROR`. The entire `use English` line is dead weight. This happens because `needs_english_import()` uses a heuristic: ANY script with a `cat` command triggers the full English import (see `mod.rs` line 2649: `"cat"` is in the command-name list), even when the `cat` is just emitting a heredoc and never uses `$OS_ERROR` or any English variable.

**IR-fixable? YES.** An IR-based backend performs **import minimization** by tracking every variable reference in the IR tree. If no `IrExpr::Var("ERRNO", ...)`, `IrExpr::Var("OS_ERROR", ...)`, etc. exist in any statement, the backend simply omits those from the `use English` line. With the IR, the analysis is precise rather than heuristic.

**Cleaned-up output with IR:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
# (no English import at all — nothing references English variables)
```

---

### Pattern E — `($? >> 8)` hardwired without exit-code capture context

**Generated:**
```perl
say "exit: " . ($? >> 8);
```

**Why it's non-idiomatic:** The expression `$? >> 8` is syntactically correct but semantically fragile. It reads `$?` in a context where no external command has been run (the preceding `say` and `print` are Perl builtins that leave `$?` untouched). The generated code relies on `$?` happening to be 0 from the environment. A robust translation of `echo "exit: $?"` should capture the exit status of the *previous command* at the point of translation. In the shell script, `$?` after the first `echo` is 0, but if a real command preceded it, the shell would correctly reflect its exit code. The current translation loses this semantics.

**Idiomatic Perl** (assuming we want to track `$?` from the previous operation):
```perl
# Option A: use $? directly if English is not needed
say "exit: " . ($? >> 8);

# Option B: use $CHILD_ERROR (requires `use English qw($CHILD_ERROR)`)
say "exit: " . $CHILD_ERROR;
```

**IR-fixable? PARTIALLY.** The IR can *format* the expression cleanly:
- `IrExpr::Var("CHILD_ERROR", ...)` → `$CHILD_ERROR`
- `IrExpr::BinOp { lhs: Var("?"), op: ShiftR, rhs: Int(8) }` → `($? >> 8)`

But the **semantic decision** of what captures `$?` at each program point is a generator-level concern. The IR nodes represent "what value to emit" but cannot inject missing exit-code snapshot logic. The generator must decide, for example, to emit:
```perl
my $exit_code = $? >> 8;   # snapshot after the "Fin." echo
say "exit: " . $exit_code;
```
The IR can then clean up the formatting (e.g., detect `$x = $? >> 8` and `$x` used once → inline it). Without the generator making this decision, the IR is powerless.

**NOT fully IR-fixable** — the generator must insert exit-code capture points. The IR can only beautify what it's given.

---

### Pattern F — `say` without feature guard makes the program uncompilable

This is a consequence of Patterns B and C combined. The current generated code **does not pass `perl -c`** because `say` is emitted in two places without `use feature 'say'`. Running `perl -c` on the output produces:

```
String found where operator expected (Do you need to predeclare "say"?)
syntax error at -e line 1, near "say "Fin. That is all folks.""
Execution of -e aborted due to compilation errors.
```

**IR-fixable? YES.** The `ir_to_perl()` auto-detection of `Output { newline: true }` is the natural fix (see Pattern B). Additionally, the IR backend already strips unnecessary `use feature 'say'` when there are no `say` statements, so it handles both directions correctly.

---

### Pattern G — `IrStmt::Output` used with `RawExpr` instead of `IrExpr::Str`

The echo generator in `simple_commands.rs` currently constructs:
```rust
IrStmt::Output {
    value: IrExpr::RawExpr(args[0].clone()),   // ← RawExpr bridge
    newline: true,
}
```

This passes the entire Perl expression as raw text, bypassing the IR's ability to format it. The `RawExpr` variant exists as a migration bridge (per `docs/ir-design.md`), but it means the backend has no semantic information about the value being printed — it can't tell if it's a string literal, a variable, or a function call.

**IR-fixable? YES** — this is the entire point of the IR migration. Replace `RawExpr` with a proper IR node:
- String literal: `IrExpr::Str(content, StrStyle::DoubleQuoted)`
- Variable: `IrExpr::Var(name, Sigil::Scalar)`
- Concatenation: `IrExpr::BinOp { lhs, op: Concat, rhs }`

This would let the backend apply consistent formatting, escaping, and optimization to all expressions.

**Cleaned-up output with IR:**
```perl
# Instead of: say "exit: " . ($? >> 8);
# The IR could understand:
#   Output {
#     value: BinOp { lhs: Str("exit: "), op: Concat,
#                    rhs: BinOp { lhs: Var("?"), op: ShiftR, rhs: Int(8) } },
#     newline: true
#   }
# Producing: say "exit: " . ($? >> 8);
# (same output, but the IR can now optimize)
```

---

## 3. Unnecessarily Verbose Translations

For this simple script the generated code is relatively concise, but two areas stand out:

### (a) The `use English` line as dead boilerplate

The entire `use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);` line (90 characters) serves no purpose. It's template-level overhead that the heuristic import checker fails to suppress. An IR-based optimizer would eliminate it entirely, shrinking the program by ~20%.

### (b) Heredoc body printed via `print "..."` when `say` would be simpler

The current heredoc path emits:
```perl
print "alpha
beta
gamma ...
";
```
This is a `print` statement with a string that already has the trailing newline embedded. With `say`, this becomes:
```perl
say "alpha\nbeta\ngamma ...";
```
No embedded `\n` at the end, no bare newlines in the source. The IR backend makes this transformation automatic by switching from raw `format!("print {};\n", ...)` to `IrStmt::Output { newline: true }` with `IrExpr::Str(..., StrStyle::DoubleQuoted)`.

---

## 4. Summary Table

| Pattern | Location | IR-fixable? | IR Node Involved |
|---|---|---|---|
| **A** Multiline bare newlines in strings | `cat.rs` heredoc output | YES | `IrExpr::Str(_, StrStyle::DoubleQuoted)` |
| **B** `say` without `use feature 'say'` | `simple_commands.rs` echo path | YES | `IrStmt::Output { newline: true }` → `ir_to_perl()` auto-import |
| **C** Inconsistent `print` vs `say` | `cat.rs` vs `simple_commands.rs` | YES | Both → `IrStmt::Output { newline: true }` |
| **D** Unused `use English` imports | Heuristic in `mod.rs` | YES | Import minimization pass over IR tree |
| **E** `($? >> 8)` without capture | Generator decision | PARTIAL | IR can format; generator must insert capture |
| **F** Uncompilable output | Consequence of B+C | YES | Same as B |
| **G** `RawExpr` bridge used | `simple_commands.rs` | YES | Replace `RawExpr` with proper `IrExpr::Str/Var/BinOp` |

The ideal idiomatic output that an IR-based backend could produce for this script:

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

say "alpha\nbeta\ngamma ...";
say "oyster\nsnapper\nsalmon";
say "Fin. That is all folks.";
say "exit: " . ($? >> 8);
```

This is 7 lines of clean, correct Perl vs. the current 11 lines with a compilation error, dead imports, and inconsistent output style. Every improvement except Pattern E is achievable through the IR backend described in `docs/ir-design.md`.