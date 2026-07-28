Now let me examine the generator source to confirm the root causes of each pattern.Now I have enough context to write the review. Let me compose it.

---

## Idiom Review: `sh/000__04f_output_formatting.sh` → Generated Perl

### 1. Original Shell Script and Generated Perl

**Original shell script (`sh/000__04f_output_formatting.sh`):**
```bash
echo_result=`echo "Hello from backticks"`
echo "Echo result: $echo_result"

printf_result=`printf "Number: %d, String: %s\n" 42 "test"`
echo "Printf result: $printf_result"

tee_result=`echo "test output" | tee test_tee.txt`
echo "Tee result: $tee_result"

rm -f test_tee.txt

echo "=== Output and Formatting Commands Complete ==="
```

**Generated Perl:**
```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

say "=== Output and Formatting Commands ===";
my $echo_result = "Hello from backticks";
say "Echo result: $echo_result";
my $printf_result = sprintf("Number: %d, String: %s\n", '42', "test");
say "Printf result: $printf_result";
my $tee_result = do {
    do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'test output' . "\n";
    if ( !($output_0 =~ m{\n\z}) ) { $output_0 .= "\n"; }
    if ($CHILD_ERROR != 0) { $pipeline_success_0 = 0; }
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        carp "tee: Cannot open 'test_tee.txt': $ERRNO";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0 =~ s/\n+\z//msx;
    $output_0;
}; };
};
say "Tee result: $tee_result";
if ( -e "test_tee.txt" ) {
    if ( -d "test_tee.txt" ) {
        carp "rm: carping: ", "test_tee.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_tee.txt" ) { }
        else {
            carp "rm: carping: could not remove ", "test_tee.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
say "=== Output and Formatting Commands Complete ===";
```

---

### 2. Non-Idiomatic Patterns

---

#### **Pattern ① — Dead imports (`IPC::Open3`, `File::Path`)**

```perl
use IPC::Open3;
use File::Path qw(make_path remove_tree);
```

Neither module is used anywhere in the generated code. They are emitted unconditionally by the generator's prologue (`src/generator/mod.rs` or similar).

**Idiomatic Perl:** Omit them entirely:
```perl
use strict;
use warnings;
use feature 'say';
```

**IR-fixable?** **Yes.** The IR's `IrProgram { imports: Vec<String> }` collects all used imports. An import-minimisation pass (one of the optimisations listed in the IR design doc) would compare the used imports against the declared ones and remove unused ones. The IR backend would then emit only `use strict; use warnings; use feature 'say';`.

---

#### **Pattern ② — Dead variable `$output`**

```perl
my $output = q{};
```

`$output` is declared but never read or written anywhere in the generated code (the echo lines use `say`, not `$output`). It is a vestigial variable from the generator's template.

**Idiomatic Perl:** Omit it.

**IR-fixable?** **Yes.** A dead-code elimination pass on the IR would detect that `$output` is never used after its declaration and remove both the `Declare` node and any associated stores. The IR design doc explicitly lists "Dead assignment elimination" as a planned optimization.

---

#### **Pattern ③ — Unnecessary `our $CHILD_ERROR`**

```perl
our $CHILD_ERROR;
```

`$CHILD_ERROR` is used only as `local $CHILD_ERROR = 0;` in one place (the `rm` else-branch). That `local` does not require a prior `our` declaration — `local` works on any global variable. Moreover, `$CHILD_ERROR` is never the target of a `qx{}` or `system()` call in this code, so it never gets set to anything meaningful.

**Idiomatic Perl:** Omit it.

**IR-fixable?** **Yes.** If no `System { capture }` or `System { }` node references `$CHILD_ERROR`, the variable is dead. A live-variable analysis on the IR would remove the `Declare { var: "CHILD_ERROR", sigil: Scalar }` node.

---

#### **Pattern ④ — `'42'` string literal instead of integer `42`**

```perl
sprintf("Number: %d, String: %s\n", '42', "test")
```

The shell source has `42` (no quotes), which is an integer. The translator quotes it as a string. Perl will coerce it, but it's sloppy — `%d` expects an integer.

**Idiomatic Perl:**
```perl
sprintf("Number: %d, String: %s\n", 42, "test")
```

**IR-fixable?** **Yes.** If the generator parsed the `42` token and produced `IrExpr::Int(42)` instead of `IrExpr::Str("42", DoubleQuoted)`, the IR backend's `ir_to_perl()` would emit `42` (no quotes). The fix is in the generator's word-to-expression lowering: when a word parses as an integer literal, emit `IrExpr::Int(i64)`. This is a **generator change**, but once it produces the correct IR node, the backend automatically outputs the clean form.

---

#### **Pattern ⑤ — Triple-nested `do { do { do { ... } }; }; }` blocks**

```perl
my $tee_result = do {
    do { do {
        # actual logic
    }; };
};
```

The two outer `do` blocks serve no purpose. They are artifacts of how the generator wraps pipeline output: the pipeline generator (`pipeline_commands.rs`) adds one `do { ... }` for scope isolation, the backtick-substitution wrapper adds another, and the tee command's own output handler may add a third. The result is a semantic no-op nesting.

**Idiomatic Perl:** A single `do` block or no `do` block at all:
```perl
my $tee_result = do { ... };
```

**IR-fixable?** **Partially, but only in concert with Pattern ⑥.** If the generator emitted `IrStmt::System { capture: Some("tee_result"), cmd: ... }`, the IR backend would produce `my $tee_result = qx{...};` — no `do` blocks at all. If the generator instead emitted a single `IrStmt::Assign` with a `do { ... }` block expression, the backend would emit exactly one `do`. The triple-nesting arises because the generator concatenates opaque `RawText` strings from three layers, and the IR backend cannot collapse opaque strings. Fixing this requires the generator to produce a single semantic IR node instead of nested `RawText`.

---

#### **Pattern ⑥ — Full pipeline simulation for `echo "test output" | tee test_tee.txt`** ★ *Most egregious*

```perl
my $tee_result = do {
    do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'test output' . "\n";
    if ( !($output_0 =~ m{\n\z}) ) { $output_0 .= "\n"; }
    if ($CHILD_ERROR != 0) { $pipeline_success_0 = 0; }
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        carp "tee: Cannot open 'test_tee.txt': $ERRNO";
    }
    $output_0 = $output_0;          # ← no-op self-assignment
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0 =~ s/\n+\z//msx;
    $output_0;
}; };
};
```

**23 lines** of Perl for a shell command that is:

```bash
tee_result=`echo "test output" | tee test_tee.txt`
```

What does the shell do? It runs `echo "test output"`, pipes it through `tee test_tee.txt` (which writes the data to the file and also passes it through to stdout), and captures the stdout into `$tee_result`. That's it.

**Idiomatic Perl** (three reasonable options):

*Option A — use `qx{}` (closest to shell semantics):*
```perl
my $tee_result = `echo "test output" | tee test_tee.txt`;
chomp $tee_result;
```

*Option B — native Perl, no shell:*
```perl
my $tee_result = "test output\n";
open my $fh, '>', 'test_tee.txt' or croak "tee: test_tee.txt: $!";
print $fh $tee_result;
close $fh;
chomp $tee_result;
```

*Option C — half-native (if the capture is what matters more than the file write):*
```perl
my $tee_result = "test output\n";
use File::Slurper 'write_file';
write_file('test_tee.txt', $tee_result);
chomp $tee_result;
```

Option A is 1 line plus a `chomp`. The generated version is 23 lines.

**IR-fixable?** **No, this requires a generator redesign.** The root cause is architectural: the generator's `generate_shell_command_substitution` function in `words.rs` (or the equivalent pipeline handler in `pipeline_commands.rs`) does not emit a high-level "run this command and capture its output" IR node. Instead, it decomposes every pipeline into individual commands, wraps each in a Perl-native implementation (the `tee` handler writes to a file), and simulates `$CHILD_ERROR` / `$pipeline_success` / `$main_exit_code` bookkeeping.

To fix this, the generator needs to:
1. Recognise that the backtick body is a simple pipeline with no shell semantics that require native Perl emulation.
2. Emit `IrStmt::System { cmd: ..., capture: Some("tee_result") }` — or even simpler, emit the whole pipeline as a single `qx{...}` string.
3. Let the IR backend decide the prettiest rendering.

The IR design doc's style table already anticipates this:

> | `System { capture: Some("out") }` | `my $out = do { ... qx{...} ... };` | `my $out = qx{...};` |

But **today**, the generator produces `RawText` for the entire block. The IR backend sees an opaque string and passes it through unchanged. Until the generator produces `IrStmt::System` instead of `IrStmt::RawText`, the IR backend has no leverage.

**Sub-patterns within this block** that are individually IR-fixable but collectively caused by the pipeline-simulation approach:

| Sub-pattern | Generated | IR-fixable? | IR node |
|---|---|---|---|
| ⑥a. Inline `use Carp` mid-block | `use Carp qw(carp croak);` | **Yes** | `IrProgram::imports` — collects all `use` at file top |
| ⑥b. Self-assignment | `$output_0 = $output_0;` | **Yes** | Dead-assignment elimination on `IrStmt::Assign` |
| ⑥c. Regex chomp | `$output_0 =~ s/\n+\z//msx;` | **Yes** (if semantic) | `IrExpr::Call { func: "chomp", args: [...] }` |
| ⑥d. Pipeline temp vars | `$output_0`, `$output_printed_0`, `$pipeline_success_0` | **No** — same root cause as ⑥ | These only exist because the generator does not emit `System` |
| ⑥e. `$main_exit_code` mutation | `if (!$pipeline_success_0) { $main_exit_code = 1; }` | **No** — same root cause as ⑥ | Would vanish with `System { capture }` |

---

#### **Pattern ⑦ — `rm -f` expanded to 15-line conditional tree**

```perl
if ( -e "test_tee.txt" ) {
    if ( -d "test_tee.txt" ) {
        carp "rm: carping: ", "test_tee.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_tee.txt" ) { }
        else {
            carp "rm: carping: could not remove ", "test_tee.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
```

Shell `rm -f file` means: silently delete the file if it exists; ignore if it doesn't; suppress diagnostics. The generated code does the opposite: it explicitly checks for existence, distinguishes files from directories, emits `carp` warnings on error, and manages `$CHILD_ERROR`.

**Idiomatic Perl:**
```perl
unlink 'test_tee.txt';    # -f semantics: returns false for missing, but we ignore
```

Or with minimal error handling befitting `-f`:
```perl
unlink 'test_tee.txt' or carp "rm: test_tee.txt: $!" if -e _;
```

**IR-fixable?** **No.** The verbosity is baked into `generate_rm_command` in `src/generator/commands/rm.rs`. The function does not produce a semantic IR node; it emits conditional Perl text as `String` (which becomes `RawText` in the IR). The function itself contains ~250 lines of nested `format!()` calls that generate existence checks, directory guards, recursive-removal logic, and error handling — all for what should be a simple `unlink`.

To fix this, the generator would need to:
1. Recognise `rm -f` as a distinct pattern (force + no recursion).
2. Emit `IrStmt::System { cmd: "unlink", args: ["test_tee.txt"], opts: { force: true } }`.
3. Let the IR backend decide the prettiest rendering — e.g. `unlink 'test_tee.txt';` with `or carp` only if `force` is false.

Alternatively, a simpler approach: for unrecognised option combinations, fall back to `system 'rm', '-f', 'test_tee.txt'`. The current approach tries to "nativize" every external command, which works well for simple cases (echo, printf) but produces bloated code for commands with even modest option sets like `rm`.

---

#### **Pattern ⑧ — `$main_exit_code` dead-end mutation**

```perl
if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
```

`$main_exit_code` is set to 1 if the pipeline fails, but it is never read or used after this point. The script exits normally (no `exit $main_exit_code` in this generated output).

**Idiomatic Perl:** Omit entirely.

**IR-fixable?** **Yes, with caveats.** The `$main_exit_code` variable is set inside the `do` block (which is `RawText`), so the IR backend cannot currently see it. If the generator were migrated to emit `IrStmt::Assign { targets: [AssignTarget("main_exit_code")], expr: IrExpr::Int(1) }`, a dead-store elimination pass could remove it since `$main_exit_code` is never read afterwards. The IR design doc's optimisation list includes exactly this.

---

#### **Pattern ⑨ — Empty `if` branch**

```perl
if ( unlink "test_tee.txt" ) { }
else { carp ... }
```

The empty `if ( ... ) { }` block is a code smell. It means "do nothing on success, only act on failure," which is more cleanly expressed with a statement modifier:

```perl
unlink 'test_tee.txt' or carp "rm: test_tee.txt: $!";
```

**IR-fixable?** **Partially.** If the generator emitted `IrStmt::System { cmd: "unlink", ... }`, the IR backend could choose the `or carp` form. But the generator currently emits the empty-if pattern as raw Perl text. A simpler IR-level fix: if the generator produced `IrStmt::If { cond: IrExpr::Call("unlink", ...), then: [], else_: [carp(...)] }`, the IR backend's pretty-printer could detect the empty `then` block and emit a statement modifier instead. However, this requires the generator to emit semantic `If` and `Call` nodes instead of `RawText`.

---

#### **Pattern ⑩ — `$CHILD_ERROR = 0` in unreachable else-branch**

```perl
else {
    local $CHILD_ERROR = 0;
}
```

This is the else-branch of `if ( -e "test_tee.txt" )` — the file does not exist. The `local $CHILD_ERROR = 0` mirrors the shell's behaviour of setting `$? = 0` when `rm -f` skips a missing file. But `$CHILD_ERROR` is never read afterwards, so the `local` is dead code.

**Idiomatic Perl:** Omit.

**IR-fixable?** **Yes** — dead-code elimination on the IR would remove this. The `local` assignment to `$CHILD_ERROR` would be a `Declare { local: true }` or `Assign` node with no subsequent reads of `$CHILD_ERROR` before the next assignment or block exit.

---

### 3. Summary Table

| # | Pattern | Line count (generated) | IR-fixable? | IR Node (if fixable) | Key Condition |
|---|---|---|---|---|---|
| ① | Unused imports `IPC::Open3`, `File::Path` | 2 | **Yes** | `IrProgram::imports` | Import-minimisation pass |
| ② | Dead variable `$output` | 1 | **Yes** | `IrStmt::Declare` → DCE | No read after write |
| ③ | Unnecessary `our $CHILD_ERROR` | 1 | **Yes** | `IrStmt::Declare` → DCE | No `qx`/`system` node references it |
| ④ | `'42'` instead of `42` | inline | **Yes** | `IrExpr::Int(42)` | Generator must parse int literals |
| ⑤ | Triple `do { do { do { } } }` | 3 (wrapper) | **No** (root cause = ⑥) | — | Generator must emit single `System` node |
| ⑥ | Pipeline simulation for `\| tee` | **23** | **No** — generator architecture | — | Generator must emit `System { capture }` |
| ⑥a | Inline `use Carp` | 1 | **Yes** | `IrProgram::imports` | Move to import list |
| ⑥b | Self-assignment `$x = $x` | 1 | **Yes** | Dead-assignment elim | DCE pass |
| ⑥c | Regex chomp | 1 | **Yes** (if semantic) | `IrExpr::Call("chomp")` | Requires semantic `Call` node |
| ⑥d | Temp vars `$output_0`, `$pipeline_success_0` | 3 | **No** (root cause = ⑥) | — | Only `System` removes them |
| ⑥e | `$main_exit_code` side-effect | 1 | **No** (root cause = ⑥) | — | Only `System` removes it |
| ⑦ | `rm -f` → 15-line conditional tree | **15** | **No** — generator policy | — | `rm.rs` emits `RawText`; needs semantic `System` node |
| ⑧ | `$main_exit_code = 1` dead store | 1 | **Yes** (but currently in RawText) | Dead-store elim on `Assign` | Must escape `RawText` first |
| ⑨ | Empty `if (unlink) { }` | 2 | **Partially** | `IrStmt::If { then: [] }` | Backend could emit statement modifier |
| ⑩ | `local $CHILD_ERROR = 0` dead | 1 | **Yes** | Dead-store elim | DCE pass |

**Total wasted lines (patterns ①–⑩):** ~55 lines of code that are either dead, overly defensive, or could be collapsed into idiomatic Perl. The entire generated file is ~50 lines; the idiomatic equivalent is ~10 lines.

---

### 4. Unnecessarily Verbose Translations — Prime Candidates for IR Simplification

| Shell line | Generated lines | Idiomatic lines | Expansion ratio |
|---|---|---|---|
| `` tee_result=`echo "test output" \| tee test_tee.txt` `` | 23 lines | 2 lines (`qx{}` + `chomp`) | **11.5×** |
| `rm -f test_tee.txt` | 15 lines | 1 line (`unlink '...'`) | **15×** |
| Prologue (imports + vars) | 6 lines | 3 lines (`use strict; use warnings; use feature 'say';`) | **2×** |

The `tee_result` pipeline and the `rm -f` translation together account for **38 of the ~45 non-trivial lines** in the generated Perl. They are verbose not because the shell semantics are complex, but because the generator decomposes every command into primitive Perl operations (open, read, write, unlink, status-check) rather than treating it as a high-level operation.

These are the **prime candidates** for IR-based simplification because:

1. **They have a single obvious semantic equivalent** in Perl: `qx{...}` and `unlink`.
2. **The current output is all `RawText`** that the IR backend passes through verbatim.
3. **The fix requires only that the generator emit `IrStmt::System`** for these commands instead of decomposing them into Perl primitives.
4. **The IR backend then automatically produces clean output** — `my $out = qx{...};` and `unlink '...';` — with no additional generator changes for style.

The IR design doc's migration strategy addresses exactly this problem: "Style fixes go in one place (ir_to_perl()), not in 80+ generator functions." The key prerequisite is that each generator function must first be migrated from producing `String` (→ `RawText`) to producing semantic IR nodes (→ `System`, `Output`, `Assign`, etc.). The `tee_result` and `rm` patterns are the highest-value targets for that migration.