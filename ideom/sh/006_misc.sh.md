Now let me check whether there's any existing generator code I should examine for context:Here is the idiom review.

---

# Idiom Review: `sh/006_misc.sh` → Generated Perl

## 1. Source and Generated Code

**Original shell script** (`sh/006_misc.sh`):
```bash
#!/usr/bin/env bash

echo "== Subshell =="
( echo inside-subshell )

echo "== Simple pipeline =="
echo "alpha beta" | grep beta
```

**Generated Perl** (as shown by the translator):
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
    print "inside-subshell\
";
    q{};
};
print "== Simple pipeline ==\n";
# Original bash: echo "alpha beta" | grep beta
my $output_0 = q{};
print $output_0, "\n";

exit $main_exit_code;
```

**Idiomatic Perl** (what a human would write):
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature qw(say);

say "== Subshell ==";
say "inside-subshell";

say "== Simple pipeline ==";
my $grep_out = `echo "alpha beta" | grep beta`;
print $grep_out;
```

---

## 2. Non-Idiomatic Patterns

### Pattern A — Fat `use English` Import with Unused Variables

```perl
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
our $CHILD_ERROR;
```

Only `$CHILD_ERROR` is declared as `our`, and even it is never referenced anywhere in the generated code. The other imported names (`$ERRNO`, `$EVAL_ERROR`, `$OS_ERROR`, etc.) are also unused. The entire import is dead weight.

**IR-fixable?** ✅ **Yes.**  
The IR program stores imports as `IrProgram { imports: Vec<String> }`.  
If the IR contains no `IrExpr::Var` references to `$CHILD_ERROR`, `$OS_ERROR`, etc., the backend can omit the `use English` line entirely.  
The `our $CHILD_ERROR` would be removed by dead-statement elimination (see Pattern C).  

*IR node:* `IrProgram.imports` — the backend can filter imports against the set of variables actually referenced in `IrExpr::Var` nodes.

**Cleaned-up output:**
```perl
use strict;
use warnings;
```
(No `use English` needed.)

---

### Pattern B — `my $main_exit_code = 0; … exit $main_exit_code;`

```perl
my $main_exit_code = 0;
# ... everything ...
exit $main_exit_code;
```

The variable is assigned a constant `0` once and never mutated.  
Perl exits 0 by default when the script reaches EOF. Even if an explicit `exit` is desired, `exit 0;` is clearer.

**IR-fixable?** ✅ **Yes.**  
The IR backend has a constant-folding pass. If the generator emits:

```rust
IrStmt::Declare { vars: [Decl("main_exit_code", Scalar)], init: Some(IrExpr::Int(0)) }
// ... followed later by ...
IrStmt::Exit { code: Some(IrExpr::Var("main_exit_code", Scalar)) }
```

The backend can inline the constant:  

*IR node:* `IrExpr::Var` with a known constant — the backend can substitute `Int(0)` for `Var("main_exit_code")` via copy-propagation, then emit `exit 0;`.

**Cleaned-up output:**
```perl
exit 0;
```
Or even just dropping the `exit` entirely and letting the script fall through (which exits 0 in Perl).

---

### Pattern C — Dead Variable `my $output = q{};`

```perl
my $output         = q{};
```

This variable is declared at file scope, initialized to empty, and **never referenced again**. It is pure dead code.

**IR-fixable?** ✅ **Yes.**  
The IR backend can perform a liveness pass over `IrStmt` nodes. A `Declare` whose declared variable never appears in any subsequent `IrExpr::Var` or `AssignTarget` is dead and can be dropped.

*IR node:* `IrStmt::Declare` with zero uses of the declared name in the rest of the program. The IR design doc lists "Dead assignment elimination" as a planned optimization.

**Cleaned-up output:** The line is simply omitted.

---

### Pattern D — Subshell Wrapped in `do { local %ENV = %ENV; … q{}; }`

```perl
do {
    local %ENV = %ENV;
    print "inside-subshell\
";
    q{};
};
```

Four problems in one construct:

1. **`local %ENV = %ENV`** — This saves and restores the entire environment. For a subshell that only contains an `echo` (no `export`, no `unset`, no variable writes), the environment isolation is unnecessary overhead. In real-world scripts this is sometimes needed, but here it's conservative boilerplate.

2. **Trailing `q{}`** — The empty-string expression exists solely to give the `do` block a non-numeric return value (so that `my $x = do { … }` doesn't capture something like `$CHILD_ERROR = 0`). When the `do` block is used as a *statement* (as it is here — not captured into a variable), the `q{}` is dead code that does nothing.

3. **`print "inside-subshell\"`** — The newline appears as a literal `\` followed by a real newline in the source, which is a quoting artifact. It should be `"inside-subshell\n"`.

4. **`do { … };` for a single `print`** — The entire block wraps what could be a bare `say` statement.

**IR-fixable?** ⚠️ **Partially.**

- The **trailing `q{}`** IS IR-fixable: if the subshell body is represented as a block of `IrStmt`s (not `RawText`), the backend can see that the last statement is an expression with no side effects and a trivially-known value (`""`), and that the block's result is not captured. It can eliminate the dead expression. This is dead-expression elimination, a standard IR optimization.

- The **weird newline quoting** is IR-fixable if the generator migrates from `RawText` to `IrExpr::Str("inside-subshell\n", DoubleQuoted)`. The backend then formats it as a proper Perl string `"inside-subshell\n"`.

- The **`local %ENV = %ENV`** and the **`do { }` wrapper** are NOT IR-fixable by pretty-printing alone. They are structural decisions made by the generator. The generator currently emits the entire `do { local %ENV …; …; q{}; }` as a single blob of `RawText` (or through `format!()` calls in `generate_subshell_impl`). To drop the wrapper, the generator must either:
  - Detect that the subshell body does not modify the environment and omit the isolation (an analysis problem), or
  - Emit finer-grained IR nodes (e.g., a `Block { env_isolation: bool, stmts: Vec<IrStmt> }` variant) and let the backend choose whether to emit `local %ENV`.

*IR nodes involved:* `IrStmt::Block` (if added) or the current `RawText`. The `q{}` removal involves `IrStmt::Output` or `IrStmt::RawText` with a dead-expression check.

**Cleaned-up output (with generator cooperation):**
```perl
say "inside-subshell";
```

---

### Pattern E — Broken/Empty Pipeline Translation

```perl
# Original bash: echo "alpha beta" | grep beta
my $output_0 = q{};
print $output_0, "\n";
```

This is the most severe issue: the pipeline `echo "alpha beta" | grep beta` does not execute at all. It creates an empty variable and prints a blank line. The pipeline body that would populate `$output_0` is missing entirely — a generator bug.

Even if the pipeline body *were* present, the pipeline infrastructure is extraordinarily heavyweight for two simple commands. Looking at the pipeline generator code in `pipeline_commands.rs`, a typical buffered pipeline would generate dozens of lines involving `open3`, IPC variables, `waitpid`, etc. For `echo "alpha beta" | grep beta`, a single `qx{...}` call suffices.

**IR-fixable?** ❌ **No**, for two separate reasons:

1. **The missing body is a generator bug.** No amount of backend pretty-printing can fix a missing pipeline body. The generator must correctly populate `IrStmt::Pipeline { stages, last_output }`.

2. **Even with a correct body, the verbosity requires generator changes.** The current pipeline generator produces the heavyweight buffered-pipeline machinery (multiple `my ($in, $out)` IPC variables, `open3` calls, `while (<$fh>)` loops) even for trivial pipelines. An IR backend can only shorten what it receives. To emit `qx{echo "alpha beta" | grep beta}`, the generator must choose to produce an `IrStmt::System { capture: Some("output_0") }` with a backtick expression, not a full `Pipeline` node with separate IPC stages.

The IR design doc's "Style rules" table hints at this:

> | Pipeline with 1 stage | wraps in vars, pipes, etc. | just the stage body |
> | Pipeline with N stages | open3 for each | `qx{...}` or native Perl |

But this optimization isn't in the backend yet, and even if it were, the missing-body bug would still need fixing in the generator.

*IR nodes:* `IrStmt::Pipeline` (or missing), `IrStmt::System`.

**Desired idiomatic output:**
```perl
my $output_0 = `echo "alpha beta" | grep beta`;
print $output_0;
```

Or, even more idiomatically, using native Perl grep:
```perl
print "alpha beta\n" if "alpha beta" =~ /beta/;
```

---

### Pattern F — `print X, "\n"` Instead of `say X`

```perl
print "== Subshell ==\n";
print "== Simple pipeline ==\n";
```

Using `print` with an explicit `"\n"` at the end of the string is not wrong, but `say` (from `use feature qw(say)` or Perl 5.10+) is the idiomatic choice when the intent is "print with newline."

The IR design doc specifically lists this as a backend-fixable style issue:

> | `Output { value: Var("x"), newline: true }` | `print $x;\nif (!($x =~ m{\n\z}msx)) { print "\n"; }` | `say $x;` |

**IR-fixable?** ✅ **Yes.**  
When the generator produces `IrStmt::Output { value: IrExpr::Str("== Subshell =="), newline: true }`, the backend simply calls `say` instead of `print "$str\n"`.

*IR node:* `IrStmt::Output { newline: true }` — the backend chooses `say` vs `print`.

**Cleaned-up output:**
```perl
say "== Subshell ==";
say "== Simple pipeline ==";
```

---

### Pattern G — `print $output_0, "\n"` (Extra Comma-separated Argument)

```perl
print $output_0, "\n";
```

`print LIST` separates list items by `$,` (default: empty string), so this prints `$output_0` immediately followed by a newline. It works, but it's non-idiomatic. The standard Perl way to print a variable with a trailing newline is either:

- `print "$output_0\n";` (double-quoted interpolation)
- `print $output_0 . "\n";` (concatenation)
- `say $output_0;` (if you have `say`)

The comma-separated `print $x, "\n"` pattern is a transliteration of how one might think in bash (`echo $x` adds a trailing newline). It reads like a line-by-line translation rather than native Perl.

**IR-fixable?** ✅ **Yes.**  
If the generator produces `IrStmt::Output { value: IrExpr::Var("output_0", Scalar), newline: true }`, the backend can emit `say $output_0;` instead of `print $output_0, "\n";`.

*IR node:* `IrStmt::Output { newline: true }`.

**Cleaned-up output:**
```perl
say $output_0;
```
(Or, with the pipeline fix from Pattern E: just `print $output_0;` since `qx{...}` already produces output with newlines.)

---

## 3. Summary: IR-Fixable vs Generator-Fixable

| Pattern | Category | IR-Fixable? | Fix |
|---|---|---|---|
| **A** `use English` with unused imports | Unnecessary import | ✅ Yes | Import minimization in `IrProgram.imports` |
| **B** `my $main_exit_code = 0; … exit $var;` | Constant indirection | ✅ Yes | Constant propagation → `exit 0;` |
| **C** Dead `my $output = q{};` | Dead code | ✅ Yes | Liveness analysis → drop declaration |
| **D1** Trailing `q{}` in subshell block | Dead expression | ✅ Yes | Dead-expression elimination |
| **D2** Weird newline `\` in string | Quoting artifact | ✅ Yes | Use proper `IrExpr::Str` |
| **D3** `do { local %ENV }` wrapper for simple echo | Unnecessary complexity | ❌ No | Requires generator analysis & IR node change |
| **E** Missing/broken pipeline body | Generator bug + verbosity | ❌ No | Requires generator logic fix |
| **F** `print "…\n"` instead of `say` | Style | ✅ Yes | `IrStmt::Output { newline: true }` → `say` |
| **G** `print $x, "\n"` comma-separated args | Transliteration style | ✅ Yes | `IrStmt::Output { newline: true }` → `say $x` |

---

## 4. Unnecessarily Verbose Translations

The two clearest examples of **unnecessary verbosity** — prime candidates for IR-based simplification — are:

### Subschell (Pattern D)

**Shell:** `( echo inside-subshell )` — one command, no env changes.

**Generated:** 5 lines including `do {`, `local %ENV = %ENV`, `q{}`, `}`.
What should be a single `say` statement is wrapped in a block with environment isolation scaffolding. The `do { … }` block is a heavy construct designed to contain multi-command subshells; here it's overkill.

**Root cause:** The generator always emits the subshell wrapper regardless of content. An IR-based optimizer could collapse `Block { env_isolation: true, stmts: [Output { … }] }` into just `Output { … }` when the output is not captured and the block has no side effects beyond the print. But this requires the IR to *have* a `Block` node with an `env_isolation` flag — currently the generator emits raw text.

### Pipeline (Pattern E)

**Shell:** `echo "alpha beta" | grep beta` — two commands, trivial data flow.

**Generated:** The body is missing (bug), but even a correct buffered pipeline from the current generator would produce ~20 lines of IPC setup (`my ($in_1, $out_1); my $pid_1 = open3(...); close $in_1; while (<$out_1>) { ... } waitpid ...`) for both stages. That's the full `open3` pipeline infrastructure for a simple `grep`.

**What it should be:** `my $out = qx{echo "alpha beta" | grep beta}` — or, in native Perl, `print "alpha beta\n" if "alpha beta" =~ /beta/`.

**Root cause:** The generator takes the generic pipeline path (buffered IPC) for every pipeline, even trivial ones. An IR-based approach could recognize that the pipeline's commands are simple builtins or well-known external commands whose logic could be inlined, or at minimum that the whole pipeline can be passed to `qx{...}` as a single shell command string. The IR doc hints at this with "Pipeline with N stages → `qx{...}`", but it's not implemented.

**Both** of these cases illustrate the same principle: the generator builds general-purpose scaffolding (subshell isolation, IPC pipeline) for every construct, even when the specific content doesn't need it. An IR backend could in theory simplify these patterns, **but only if the generator feeds it fine-grained IR nodes** (like `IrStmt::Output`, `IrStmt::Block`, `IrStmt::System`) instead of opaque `RawText` blobs. The current subshell and pipeline generators produce `RawText`, which the IR backend cannot analyze or simplify.