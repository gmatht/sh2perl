Let me first read the IR design doc to ground the analysis.Now let me look at the current generator to understand how echo is being translated, and also check if `say` is used anywhere.Let me run the actual generator on the test file to see its current output:Now I have a thorough understanding of the codebase. Let me also check the `perl_string_literal_no_interp` function to see why it produces single-quoted strings:Now I have a thorough understanding. Here is the idiom review:

---

## Idiom Review: `004_test_quoted.sh` Shell-to-Perl Translation

### 1. Source and Generated Code

**Original shell script** (`examples/004_test_quoted.sh`):
```bash
echo "Hello, World!"
echo 'Single quoted'
echo "String with \"escaped\" quotes"
echo "String with 'single' quotes"
```

**Generated Perl** (as provided):
```perl
#!/usr/bin/env perl
use strict;
use warnings;
print "Hello, World!\n";
print 'Single quoted', "\n";
print "String with \"escaped\" quotes\n";
print "String with 'single' quotes\n";
```

---

### 2. Non-Idiomatic Patterns

#### Pattern A: `print 'Single quoted', "\n"` — newline appended as separate argument

The single-quoted shell argument `'Single quoted'` becomes `print 'Single quoted', "\n"`. The newline is a separate concatenated argument rather than being embedded directly in the string.

The generator path is:
1. `echo.rs` → `perl_string_literal_no_interp()` produces `'Single quoted'` (a proper single-quoted Perl literal)
2. `simple_commands.rs` → wraps it as `IrStmt::Output { value: IrExpr::RawExpr("'Single quoted'"), newline: true }`
3. `ir.rs` → `emit_stmt` for `Output` sees a non-double-quoted expression (`'Single quoted'` starts with `'`) and falls to the else branch:
   ```rust
   // (not double-quoted, so...)
   out.push_str(&format!("print {}, \"\\n\";\n", expr));
   ```

The result: `print 'Single quoted', "\n"`.

**Truly idiomatic Perl** would be any of:
```perl
say 'Single quoted';          # requires 'use feature "say"'
print "Single quoted\n";       # embed \n in double-quoted string
print 'Single quoted' . "\n";  # concatenation (but still two-arg print is fine)
```

---

#### Pattern B: `print` everywhere instead of `say`

Every echo maps to `print "...\n"` or `print '...', "\n"`. Idiomatic modern Perl (5.10+) uses `say` for newline-terminated output, since the `\n` is implicit.

---

#### Pattern C: `print "Hello, World!\n"` — this line is fine

This one is actually idiomatic. A double-quoted string with `\n` embedded. No complaint.

---

#### Patterns D & E: escaped-double-quotes and single-inside-double — both fine

```perl
print "String with \"escaped\" quotes\n";   # correct escaping
print "String with 'single' quotes\n";      # single quotes inside double — natural
```

---

### 3.–4. IR-Fixable Patterns

| # | Pattern | IR-fixable? | IR Node | Cleaned-up output |
|---|---------|-------------|---------|-------------------|
| **A** | `print 'Single quoted', "\n"` | ✅ Yes | `IrStmt::Output { value: IrExpr::RawExpr("'Single quoted'"), newline: true }` | `say 'Single quoted'` or `print "Single quoted\n"` |
| **B** | `print` instead of `say` | ✅ Yes | `IrStmt::Output { newline: true }` | `say "Hello, World!"` |

**How the IR backend would fix Pattern A:**

The fix lives in `emit_stmt()` in `src/ir.rs`, in the `Output` arm. Currently the code only inlines `\n` for double-quoted strings:

```rust
let is_dq = expr.starts_with('"') && expr.ends_with('"') && expr.len() >= 2;
if is_dq {
    let inner = &expr[1..expr.len()-1];
    out.push_str(&format!("print \"{}\\n\";\n", inner));   // OK for "..." 
} else {
    out.push_str(&format!("print {}, \"\\n\";\n", expr));  // fallback → Pattern A
}
```

A generalized fix would:

1. **Check for single-quoted strings** (`expr.starts_with('\'') && expr.ends_with('\'')`). If the content contains no `$`, `@`, or bare backslashes (which would become interpolation escapes inside double quotes), strip the single quotes and re-emit as double-quoted with `\n`:
   ```perl
   print "Single quoted\n";
   ```

2. **Or, simpler and more robust**: always emit `say` when `newline=true`. The IR node already carries `newline: bool`, so the pretty-printer can unconditionally choose `say` over `print ..., "\n"`. This is what the current codebase actually does (see §7 below).

**How the IR backend would fix Pattern B:**

Again, the `Output` arm of `emit_stmt`. Change:
```rust
// current:
out.push_str(&format!("print ..., \"\\n\";\n"));
// to:
out.push_str(&format!("say ...;\n"));
```
And add `use feature 'say'` (or `use v5.10+`) to the import list. This is a pure style change — no generator modification needed.

---

### 5. Patterns NOT IR-Fixable (Require Generator Changes)

**None for this specific test case.** All four echo lines could produce fully idiomatic output through IR pretty-printer improvements alone.

However, there is a **structural limitation** that blocks the cleanest fix:

The echo generator currently emits `IrExpr::RawExpr("'Single quoted'")` rather than `IrExpr::Str("Single quoted", StrStyle::SingleQuoted)`. Because `RawExpr` is an opaque string, the IR backend cannot safely determine whether the content is a simple safe-to-re-quote literal or something complex containing `$`, `@`, or embedded quotes that would change meaning if the quoting style were altered.

**To get the truly clean output `say 'Single quoted'` via the IR, the generator must be changed to emit proper `IrExpr::Str` instead of `IrExpr::RawExpr`.** Once it does, the IR backend can make optimal quoting decisions.

**Recommended migration path** (matching the strategy in `docs/ir-design.md`):
1. Rewrite the relevant branch in `echo.rs` / `simple_commands.rs` to return `IrExpr::Str("Single quoted", StrStyle::SingleQuoted)` or `IrExpr::Str("Hello, World!", StrStyle::DoubleQuoted)` instead of `IrExpr::RawExpr(...)`.
2. The IR backend's `ir_expr_to_perl` already handles `IrExpr::Str` correctly (producing `'Single quoted'` or `"Hello, World!"`).
3. The `Output` arm then sees a semantic string, recognizes it, and can trivially embed `\n` or switch to `say`.

---

### 6. Unnecessarily Verbose Translations

For this particular test case, the generated output is **commendably lean**. There is no:

- Pipeline infrastructure (no `for` loops, no `open3`, no `$output .= ...`)
- Trailing-newline-check boilerplate (`if (!($x =~ m{\n\z}msx)) { $x .= "\n"; }`)
- Variable assignments or IPC setup
- `do` blocks wrapping simple expressions

Each `echo` maps to exactly one `print` statement. The translator correctly recognized that none of these echo commands involve:
- Multiple arguments (which would require `join " ", @args`)
- `-n` flag (suppress newline)
- `-e` flag (interpret escapes)
- Variable interpolation
- Pipeline context
- Brace expansion
- Command substitution

**The one unnecessary verbosity** is Pattern A: `print 'Single quoted', "\n"` instead of `say 'Single quoted'` or `print "Single quoted\n"`. This is a single-expression-to-two-argument inflation caused entirely by the IR pretty-printer, not by the generator logic.

---

### 7. Note on Current Codebase State

Running the current generator (`target/release/debashc -i examples/004_test_quoted.sh`) actually produces:

```perl
use feature 'say';
...
say "Hello, World!";
say 'Single quoted';
say "String with \"escaped\" quotes";
say "String with 'single' quotes";
```

This is a significant improvement — Pattern A is resolved (single-quoted strings now work with `say`), and Pattern B is resolved (using `say`). This demonstrates the IR-based approach working: the IR backend was updated to emit `say` for `Output { newline: true }` without touching any generator code.

However, note the extra boilerplate (`use IPC::Open3`, `my $output = q{}`, `our $CHILD_ERROR`) that was absent from the user's provided output. These come from the top-level `generate()` method adding common infrastructure unconditionally. That is a separate area for improvement — see the optimization passes section of `docs/ir-design.md` for the planned dead-code elimination that would remove unused imports and variables.

---

### Summary Table

| Pattern | Line | Current Output | Idiomatic Perl | IR-Fixable? | Fix Location |
|---------|------|---------------|----------------|-------------|-------------|
| A | 4 | `print 'Single quoted', "\n"` | `say 'Single quoted'` or `print "Single quoted\n"` | ✅ Yes (IR backend only) | `ir.rs: emit_stmt(Output)` — detect single-quoted strings or unconditionally use `say` |
| B | 3–6 | `print ...` | `say ...` | ✅ Yes (IR backend only) | `ir.rs: emit_stmt(Output)` — change `print ..., "\\n"` to `say ...` |
| C | 5 | `print "String with \"escaped\" quotes\n"` | Already idiomatic | N/A | Already correct |
| D | 6 | `print "String with 'single' quotes\n"` | Already idiomatic | N/A | Already correct |
| RawExpr limitation | 4 | `RawExpr("'Single quoted'")` | `IrExpr::Str("Single quoted", SingleQuoted)` | ❌ No (generator must change) | `echo.rs` / `simple_commands.rs` — emit semantic `IrExpr::Str` instead of `RawExpr` |