Now I have a complete picture. Let me write the idiom review for `sh/000__07_find_path_commands.sh`.Let me verify the output was written correctly:The review is written to `ideom/sh/000__07_find_path_commands.sh.md` (561 lines). Here's a summary of the findings:

**7 idiom patterns identified** across the generated code:

| Pattern | Severity | IR-Fixable? |
|---|---|---|
| **A — Dead boilerplate** (`IPC::Open3`, `$output`, `$CHILD_ERROR`) | Medium | ✅ Yes — DCE pass + import minimization |
| **B — Verbose `do { }` block** (9 lines for a simple value) | **High** | ❌ No — generator emits raw format string |
| **C — Trailing-newline dance** (with double-`\n` bug) | **High** | ⚠️ Partial — would vanish if generator used `Backtick` IR node |
| **D — Over-anchored regex** `^.*\.sh$` → `\.sh\z` | Low | ❌ No — `escape_glob_to_regex()` logic |
| **E — `require` inside runtime block** | Medium | ✅ Yes — hoist to `IrProgram.requires` |
| **F — Inconsistent quoting** (`q{.}` for dot) | Low | ✅ Yes — migrate to `IrExpr::Str` |
| **G — Dense one-line callback** | Low | ⚠️ Partial — split format string |

**Key insight**: The root cause of Patterns B, C, E, G, and all three V entries is a single design decision in `src/generator/words.rs:673` — the `find` branch uses `generate_find_for_substitution()` (9-line `do { }` with File::Find scaffolding) instead of `IrExpr::Backtick` (1 expression via `qx{}`) like `head` and `tail` do at lines 682-690. Switching `find` to use `IrExpr::Backtick` would eliminate the majority of the verbosity in one generator change.

**3 unnecessarily verbose translations** identified:
1. **V-1**: The `do` block infrastructure (9 lines → 3 with `qx{}`)
2. **V-2**: `head`/`tail` vs `find` inconsistency — same pattern treated completely differently
3. **V-3**: The `if ($result ne q{})` trailing-newline conditional (buggy and unnecessary)