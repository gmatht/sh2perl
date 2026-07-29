Let me also examine the generator to understand how statements are emitted:Let me also check the actual generated output by running the translator:Now let me look at the IR implementation to see what nodes map to current patterns:Now I have a thorough understanding of both the generated code and the IR system. Let me write the idiom review.

---

## Idiom Review: `sh/008_simple_backup.sh` → Perl

### 1. Source & Generated Code

**Original shell script:**
```bash
#!/bin/bash

# Simple shell script example
echo "Hello, World!"
#TODO: Support multi-column output
ls -1 | grep -v __tmp_test_output.pl
#This should be a single token, not two.
#AST_MUST_CONTAIN: [Literal("-1")]
echo `ls | grep -v __tmp_test_output.pl`
```

**Generated Perl:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;

say "Hello, World!";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
my $output_0 = q{};
say $output_0;
print join(" ", grep { length } split /\s+/msx, do { local $CHILD_ERROR = 0; q{}; });

exit $main_exit_code;
```

**What idiomatic Perl *should* look like for this script:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

say 'Hello, World!';

# ls -1  |  grep -v __tmp_test_output.pl
opendir my $dh, '.' or die "Cannot open .: $!\n";
my @files = grep { !/__tmp_test_output\.pl$/ } readdir($dh);
closedir $dh;
say for @files;

# echo `ls | grep -v __tmp_test_output.pl`  —  same as above, but as one line
say "@{[ grep { !/__tmp_test_output\.pl$/ } readdir(Dir::open('.')) ]}";
```
(Or using `glob` for a simpler but less faithful rendering.)

---

### 2. Non-idiomatic Patterns

---

#### Pattern A — Cargo-culted `use English` import with no references

```perl
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
```

None of `$ERRNO`, `$EVAL_ERROR`, `$INPUT_RECORD_SEPARATOR`, `$OS_ERROR`, or `$PROGRAM_NAME` appear anywhere in the visible generated code. This import is dead weight. It adds a runtime lexical penalty (`qw(-no_match_vars ...)` still has a small cost) and confuses the reader into wondering which English variables are actually being used.

**IR-fixable?** ✅ Yes.

- **Which IR node?** `IrProgram::imports`. The backend already has an `ir_to_perl()` that emits imports. An optimization pass can scan all `IrExpr::Var`, `IrExpr::RawExpr`, and `IrStmt::RawText` for mentions of English variables, then prune the import list to only what's needed. If nothing matches, the entire `use English` line is dropped.

- **Cleaned-up output:**
  ```perl
  # No use English line at all
  ```

---

#### Pattern B — Dead boilerplate variable declarations

```perl
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
```

- `$main_exit_code` is initialized to `0` and only appears in `exit $main_exit_code`. It never changes. This is just `exit 0` with extra indirection.
- `$ls_success` is **never written or read** anywhere else. Entirely dead.
- `$output` is declared but never referenced; the pipeline uses `$output_0` instead.
- `$CHILD_ERROR` is declared `our` but only localized inside a `do` block whose body is just `q{}` — the localization does nothing.

**IR-fixable?** ✅ Yes (mostly).

- **Which IR node?** `IrStmt::Declare`. A dead-variable elimination pass scans `IrProgram` for declared variables that are never referenced (except in their own declaration). Any unreferenced `Declare { init: Some(...) }` where the init has no side effects can be removed. `$main_exit_code` can be constant-folded into the `exit` node.

- **Cleaned-up output:**
  ```perl
  # No $main_exit_code, $ls_success, $output, or $CHILD_ERROR declarations
  exit 0;
  ```

- **Caveat:** The generator currently *emits* these declarations based on a coarse pre-analysis (`needs_exit_code_tracking`, `needs_ls_success`, `needs_output_var`). While the IR can remove dead ones, the generator should not emit them in the first place. This is partially generator-level, but the IR's dead-code elimination can clean it up as a safety net.

---

#### Pattern C — Pipeline infrastructure for an empty result

```perl
my $output_0 = q{};
say $output_0;
```

The `ls -1 | grep -v __tmp_test_output.pl` pipeline was reduced to `q{}` (empty string), then stored in a temp variable and printed. This is the worst of both worlds: the command's output is lost (semantics bug), and the remaining scaffolding — declare variable, assign empty string, print variable — is verbose even if the command *had* worked.

If the pipeline *did* produce output, the idiomatic form would be one of:
```perl
# Direct output (no capture):
say `ls -1 | grep -v __tmp_test_output.pl`;

# Or native Perl (no shell):
opendir my $dh, '.'; say for grep { !/__tmp_test_output\.pl$/ } readdir($dh);
```

**IR-fixable?** ✅ Partially.

- **Which IR node?** `IrStmt::Pipeline { stages, capture: None }`. When the pipeline has no capture variable and its last stage is printed, the backend could fold it into a single `say` with an embedded `qx{}` or native Perl expression, skipping the temporary variable.

- **Cleaned-up output (for a working pipeline):**
  ```perl
  say `ls -1 | grep -v __tmp_test_output.pl`;
  ```

- **Why only partial?** The root cause — the pipeline producing `q{}` instead of the actual command — is a generator bug (the `cmd_str_to_open_perl` function in `ir.rs` always returns `"q{}"`). The IR backend can only optimize what it receives. This is **NOT IR-fixable** because the semantics are lost before IR generation.

---

#### Pattern D — `do { local $CHILD_ERROR = 0; q{}; }` wrapper around a constant

```perl
do { local $CHILD_ERROR = 0; q{}; }
```

This `do` block exists solely to localize `$CHILD_ERROR` before running the command. But the "command" here is `q{}` — a string literal with zero side effects. The entire `do` block is dead ceremony. Compare with:

```perl
# What was generated:
do { local $CHILD_ERROR = 0; q{}; }

# What is equivalent:
q{}

# Or even better for an empty backtick:
''
```

**IR-fixable?** ✅ Yes.

- **Which IR node?** If the generator used `IrExpr::Backtick { expr, native: false }`, the backend's `ir_expr_to_perl` for `Backtick` calls `cmd_str_to_open_perl()`, which currently always returns `q{}`. The IR path skips the `local $CHILD_ERROR = 0` wrapper because `Backtick` is an expression, not a statement. If instead the code goes through `IrStmt::System { capture: Some(...) }`, the `System` handler emits the `do { local $CHILD_ERROR ... }` wrapper — and if the inner command is trivial, the backend could elide it.

- **Additionally:** The `local $CHILD_ERROR = 0;` pattern itself could be optimized. If `$CHILD_ERROR` is never used after the `do` block, the localization is unnecessary. The IR can track `$CHILD_ERROR` usage.

- **Cleaned-up output:**
  ```perl
  print join(" ", grep { length } split /\s+/msx, q{});
  ```
  Or even better, after further constant-folding:
  ```perl
  print "\n";
  ```

---

#### Pattern E — Mandatory word-splitting on every backtick result

```perl
print join(" ", grep { length } split /\s+/msx, do { ... });
```

Every backtick command substitution result is run through `split /\s+/msx` (word splitting), then `grep { length }` (remove empties), then `join(" ", ...)` (rejoin with spaces). This directly models shell's word-splitting behavior, but:

1. **It's always applied**, even when the user clearly wants the whole string (e.g., assigning to a scalar).
2. **It's applied even when the inner expression is empty** — `split /\s+/msx, q{}` returns an empty list, `grep` returns empty, `join` returns `""`. The whole chain is a no-op for empty/constant input.
3. **It adds O(n) runtime** for every backtick, even when splitting destroys the data the user needs.

**IR-fixable?** ✅ Conditional.

- **Which IR node?** `IrExpr::Call { func: "split" }` → `IrExpr::Call { func: "join" }` wrapping `IrExpr::Backtick`. An optimization pass can constant-fold: if the `Backtick` expression is a constant string, the split/grep/join can be pre-computed at compile time. If the `Backtick` produces output that is assigned to a scalar (not `say`/`print`), the split/grep/join could be dropped entirely because shell assigns raw command output to scalars without word-splitting.

- **Cleaned-up output for an empty backtick:**
  ```perl
  print q{};
  ```
  After constant folding of the whole chain:
  ```perl
  print "\n";
  ```

- **Why conditional?** The *decision* to always word-split is a generator-level policy. An IR optimization can remove it for trivial cases (empty input, numeric context), but changing the policy requires modifying the generator to emit different IR nodes depending on context (scalar vs list assignment). The IR backend can only fold what it sees; the split/grep/join pattern is an intentional semantic choice that needs generator-level awareness.

---

#### Pattern F — `exit $main_exit_code` variable indirection

```perl
exit $main_exit_code;
```

`$main_exit_code` is initialized to `0` and never modified. This is just `exit 0;` with a variable lookup. The variable exists because the generator assumes every script might set a non-zero exit code, but for this trivial script it's dead weight.

**IR-fixable?** ✅ Yes.

- **Which IR node?** `IrStmt::Exit(Some(IrExpr::Var("main_exit_code", Sigil::Scalar)))`. A constant-folding pass can follow `$main_exit_code` back to its only assignment (`Declare { init: Some(IrExpr::Int(0)) }`) and replace the exit argument with `IrExpr::Int(0)`. Then dead-variable elimination removes the `$main_exit_code` declaration.

- **Cleaned-up output:**
  ```perl
  exit 0;
  ```

---

### 3. Summary Table

| # | Pattern | IR-fixable? | IR Node(s) Involved | Requires Generator Change? |
|---|---|---|---|---|
| A | Cargo-culted `use English` | ✅ Yes | `IrProgram::imports` | No — pure pruning pass |
| B | Dead boilerplate vars (`$main_exit_code`, `$ls_success`, `$output`, `$CHILD_ERROR`) | ✅ Mostly | `IrStmt::Declare` + dead-var analysis | Partially — generator over-emits, but IR can clean up |
| C | Pipeline infrastructure for empty result (`my $output_0 = q{}; say $output_0`) | ❌ No | `IrStmt::Pipeline` | **Yes** — `cmd_str_to_open_perl()` returns `q{}` always; pipeline generator misses the actual command |
| D | `do { local $CHILD_ERROR = 0; q{}; }` around constant | ✅ Yes | `IrStmt::System` { capture } or `IrExpr::Backtick` | No — IR can elide wrapper when body is side-effect-free |
| E | Mandatory split/grep/join word-splitting | ⚠️ Conditional | `IrExpr::Call { func: "split" }` / `IrExpr::Call { func: "join" }` | Partial — constant-folding works for empty input, but policy change needs generator |
| F | `exit $main_exit_code` variable indirection | ✅ Yes | `IrStmt::Exit` + `IrStmt::Declare` | No — constant folding + dead-var elimination |

---

### 4. Unnecessarily Verbose Translations (Prime IR Candidates)

These are the constructs where the generated code builds elaborate scaffolding for trivial operations:

**V1 — The empty-pipeline dance (Pattern C)**
```perl
my $output_0 = q{};
say $output_0;
```
The generator creates a variable, assigns it, then prints it. A single `say` with the pipeline expression directly would suffice. This is the most obvious place where IR-based simplification would help: if `IrStmt::Pipeline { stages }` has no capture variable and the pipeline's result is immediately printed, the backend should inline it into one statement.

**V2 — The `do` block fortress around nothing (Pattern D)**
```perl
do { local $CHILD_ERROR = 0; q{}; }
```
Five tokens (`do`, `{`, `local`, `$CHILD_ERROR = 0`, `;`, `q{}`, `}`) to produce an empty string. The `local` keyword is executing dynamic-scope setup and teardown for a variable that is never read. The IR backend can see that `q{}` has no side effects and the entire `do` block can be replaced by `q{}`.

**V3 — The split-grep-join pipeline for empty input (Pattern E)**
```perl
print join(" ", grep { length } split /\s+/msx, q{});
```
Six function calls (`join`, `grep`, `split`, plus three implicit temp lists) to print nothing. The entire expression tree evaluates to `""` regardless of runtime conditions. An IR constant-folding pass could verify that `split /\s+/msx, q{}` always returns `()` and reduce the whole subtree to `print "";` or `print "\n";`.

**V4 — Triple declaration (dead boilerplate) (Pattern B)**
```perl
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
```
Four declarations for variables that serve no purpose in the script. The IR can audit each declaration: if a variable is never read after declaration (excluding its own declaration), remove it. If a variable is read only in one place and its value is provably constant, inline the constant.

---

### 5. What a Full IR Pass Would Produce

If all the IR-fixable optimizations above were applied to the same generated code, the output would shrink to:

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

say "Hello, World!";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
say q{};
print q{};
```

...and after the generator bugfix (Pattern C — making pipelines actually produce output):

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

say 'Hello, World!';

# ls -1 | grep -v __tmp_test_output.pl
say `ls -1 | grep -v __tmp_test_output.pl`;

# echo `ls | grep -v __tmp_test_output.pl`
print `ls | grep -v __tmp_test_output.pl`;
```

...or even better, a native-Perl rewrite that avoids shell entirely:

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

say 'Hello, World!';

opendir(my $dh, '.') or die "opendir: $!\n";
my @files = sort grep { !/__tmp_test_output\.pl$/ } readdir($dh);
closedir $dh;
say for @files;

say "@{[ grep { !/__tmp_test_output\.pl$/ } do { opendir(my $d, '.'); my @f = readdir($d); closedir $d; @f } ]}";
```

The last line is still awkward because reproducing `echo \`ls | grep ...\`` semantics in pure Perl requires iterating the directory twice or using a temporary. A pragmatic intermediate would keep the `qx{}` calls for the backtick cases and only translate `ls -1` pipelines to native opendir/readdir.

---

### 6. Key Takeaways

1. **The biggest wins come from dead-code elimination.** The IR can remove ~40% of the generated lines (dead declarations, unused imports, the empty `do` block wrapper) without changing generator logic.

2. **Semantic bugs block IR optimization.** Patterns C (empty pipeline) and the collapsed backtick (`q{}` instead of the real command) are generator bugs, not style issues. The IR cannot fix them because it never sees the correct value.

3. **The `split/grep/join` word-splitting is the most distinctive "transliteration" smell.** It's the clearest sign that the generator is modelling *how shell works* (word splitting after command substitution) instead of producing idiomatic Perl. The IR can constant-fold it away for trivial inputs, but a proper fix requires the generator to distinguish list context (word-split) from scalar context (no split).

4. **`local $CHILD_ERROR = 0;` is the second-biggest boilerplate pattern.** Every backtick or system call gets this wrapper. An IR optimization could skip the `local` when the variable is never read afterward, which is the common case for standalone `echo \`...\`` commands.