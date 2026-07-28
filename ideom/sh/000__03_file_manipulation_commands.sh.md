# Idiom Review: `sh/000__03_file_manipulation_commands.sh` → Perl

---

## 1. Original Shell Script vs. Generated Perl

### Source (`sh/000__03_file_manipulation_commands.sh`)

```bash
#!/bin/bash
echo "=== File Manipulation Commands ==="
echo "=== cp command ==="
echo
echo "test content" > test_file.txt
cp_result=`cp test_file.txt test_file_copy.txt && echo "Copy successful"`
echo "Copy result: $cp_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

echo
echo "=== mv command ==="
mv_result=`mv test_file_copy.txt test_file_moved.txt && echo "Move successful"`
echo "Move result: $mv_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

echo
echo "=== rm command ==="
rm_result=`rm test_file.txt test_file_moved.txt && echo "Remove successful"`
echo "Remove result: $rm_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

echo
echo "=== mkdir command ==="
mkdir_result=`mkdir test_dir && echo "Directory created"`
echo "Mkdir result: $mkdir_result"
touch test_dir/file
ls test_dir 2>/dev/null || echo "Directory not found"
rm test_dir/file
rmdir test_dir

echo
echo "=== touch command ==="
touch_result=`touch test_file.txt && echo "File touched"`
echo "Touch result: $touch_result"

echo
rm -f test_file.txt test_file_copy.txt test_file_moved.txt
rm -rf test_dir 2>/dev/null || true
```

### Generated Perl (key excerpts)

The generated Perl is **extremely** verbose — 374 lines vs. 35 lines of shell. The major structural patterns are shown below.

---

## 2. Non-Idiomatic Patterns

### Pattern A — Echo/print wrapping (`do { my $__echo_line ... }`)

**Generated code** (repeats identically for every `echo`):
```perl
do {
    my $__echo_line = "Copy result: $cp_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
```

**Preferred idiomatic Perl**:
```perl
say "Copy result: $cp_result";
```

**IR-fixable?** ✅ **Yes.**  
This is the textbook case from `docs/ir-design.md`. The IR has:
```rust
IrStmt::Output { value: IrExpr::Interpolate(...), newline: true }
```
The pretty-printer `ir_to_perl()` would emit `say ...;` instead of the 8-line wrapper. The node is `IrStmt::Output`.

---

### Pattern B — Triple variable declaration (all sigils)

**Generated code** (repeats for every captured variable):
```perl
my $cp_result;
my @cp_result;
my %cp_result;
```

**Preferred idiomatic Perl**:
```perl
my $cp_result;
```

**IR-fixable?** ✅ **Yes, partially.**  
The IR node `Assign { targets: [AssignTarget { var: "cp_result", sigil: Scalar }] }` carries the sigil. The current generator emits all three because it doesn't know which sigil will be used at declaration time. With IR, the declaration would be `Declare { vars: [Decl { name: "cp_result", sigil: Scalar }] }` and only the needed sigil is emitted.

However, the root cause is that the generator doesn't infer the sigil from the first assignment. The IR design's `Declare` node solves this because the generator would emit `Declare { .. }` with the correct sigil from the start.

---

### Pattern C — Backtick substitution expanded to do/eval/CHILD_ERROR tower

**Generated code** (e.g., for `cp_result=\`cp ... && echo "..."\``):
```perl
$cp_result = do {
    my $left_result_0 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Copy qw(copy);
            if ( -e 'test_file.txt' ) {
                if ( -d 'test_file_copy.txt' ) {
                    require File::Copy; File::Copy::copy('test_file.txt', 'test_file_copy.txt' . '/' . ('test_file.txt' =~ m|([^/]+)$|)[0]);
                } else {
                    require File::Copy; File::Copy::copy('test_file.txt', 'test_file_copy.txt');
                }
            } else {
                croak "cp: cannot stat 'test_file.txt': No such file or directory\n";
            }
            1;
        };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
        }
        q{};
    };
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_0 = do { ("Copy successful") };
        $left_result_0 . $right_result_0;
    } else {
        q{};
    }
};
```

**Preferred idiomatic Perl**:
```perl
use File::Copy qw(copy);
my $cp_result = copy('test_file.txt', 'test_file_copy.txt') ? 'Copy successful' : '';
```

Or, if retaining the native `cp` command:
```perl
my $cp_result = qx{cp test_file.txt test_file_copy.txt && echo "Copy successful"};
```

**IR-fixable?** ✅ **Yes, partially.**  
The IR has `System { cmd: ..., capture: Some("cp_result") }` and `Pipeline { stages: [...] }`. A backtick substitution is a command substitution — it should lower to a single `IrStmt::System` with a capture variable. The current generator's expansion into do/eval blocks is the transliteration of "run, check exit code, conditionally concatenate" from shell semantics. The IR pretty-printer would produce:

```
my $cp_result = do {
    my $left = do { copy(...); q{}; };
    if ($CHILD_ERROR == 0) { $left . "Copy successful" } else { q{} }
};
```

But even that is still verbose. The real fix requires the **generator** to recognize that `\`cp ... && echo "..."\`` is a simple conditional that can be expressed as a ternary with native Perl calls. This is beyond what a pretty-printer can fix — it needs the generator to produce a simpler IR structure (e.g., a single `Assign` with a `Ternary` expression rather than nested `do` blocks).

**However**, the outer `do { ... }` wrapping and the `$CHILD_ERROR = 0; ... eval { ... 1; }; if(!$eval_result) { $CHILD_ERROR = 256; } q{};` is fixable at IR level: the `System` node would absorb the error handling into a `qx{}` or `system()` call. The verbosity of emulating `cp` as native Perl is a separate concern (the `ls`-emulation problem).

---

### Pattern D — `ls` emulation (massive verbosity)

**Generated code** (~50 lines per invocation):
```perl
$CHILD_ERROR = 0;
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    my @ls_files_1 = ();
    my $ls_all_found_2 = 1;
    my @ls_inputs_3 = ();
    push @ls_inputs_3, 'test_file.txt';
    push @ls_inputs_3, 'test_file_copy.txt';
    push @ls_inputs_3, 'test_file_moved.txt';
    my @ls_files_4 = ();
    my @ls_dirs_5 = ();
    my $ls_show_headers_6 = scalar(@ls_inputs_3) > 1;
    for my $ls_item_7 (@ls_inputs_3) {
        if ( -f $ls_item_7 ) { push @ls_files_4, $ls_item_7; }
        elsif ( -d $ls_item_7 ) { push @ls_dirs_5, $ls_item_7; }
        else { $ls_all_found_2 = 0; }
    }
    @ls_files_4 = sort { $a cmp $b } @ls_files_4;
    @ls_dirs_5 = sort { $a cmp $b } @ls_dirs_5;
    if (@ls_files_4) {
        push @ls_files_1, join("\n", @ls_files_4);
    }
    for my $ls_dir_8 (@ls_dirs_5) {
        my @ls_dir_entries_9 = ();
        if ( opendir my $dh, $ls_dir_8 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_dir_entries_9, $file;
            }
            closedir $dh;
            @ls_dir_entries_9 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_9;
            if ( $ls_show_headers_6 ) { ... }
            ...
        }
        else { $ls_all_found_2 = 0; }
    }
    ...
};
```

**Original**:
```bash
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"
```

**Preferred idiomatic Perl**:
```perl
my @files = grep { -e } qw(test_file.txt test_file_copy.txt test_file_moved.txt);
if (@files) { say for @files } else { say "No test files found" }
```

**IR-fixable?** ❌ **No — requires generator logic change.**  
The current generator translates `ls` to a full Perl emulation (iterating over args, checking file types, reading directories). An IR backend can only format what it receives. If the generator emits `ls` as `System { cmd: "ls", args: [...] }`, the pretty-printer can produce `system("ls", ...)` or `qx{ls ...}`. But if the generator chooses to lower `ls` to native Perl code (as it does now), the verbosity is baked into the IR nodes themselves (`For`, `If`, `opendir`, etc.). 

To get clean output, the **generator** must decide: "emit `ls` as a system call" or "emit `ls` as native Perl but use idiomatic Perl (glob, grep)". The IR backend cannot invent a condensed representation if the generator feeds it 20 verbose IR nodes.

---

### Pattern E — `$CHILD_ERROR = 0;` before every statement

**Generated code**:
```perl
$CHILD_ERROR = 0;
print "\n";
$CHILD_ERROR = 0;
print "=== mv command ===\n";
...
$CHILD_ERROR = 0;
do {
    my $__echo_line = ...;
    ...
};
```

**Preferred idiomatic Perl**: No redundant `$CHILD_ERROR` resets.

**IR-fixable?** ✅ **Yes.**  
The IR would not have an explicit `$CHILD_ERROR = 0` node for simple statements. The `System` node would manage `$CHILD_ERROR` internally. The pretty-printer only emits the reset when actually needed (before a system command). Simple `Output` nodes don't touch `$CHILD_ERROR`.

---

### Pattern F — `do { }` wrapping simple expressions

**Generated code**:
```perl
my $right_result_0 = do { ("Copy successful") };
```

**Preferred idiomatic Perl**:
```perl
my $right_result_0 = "Copy successful";
```

**IR-fixable?** ✅ **Yes.**  
The `Assign` node with `IrExpr::Str("Copy successful")` would produce `my $right_result_0 = "Copy successful";` without `do {}` wrapping.

---

### Pattern G — `do { local *STDERR; open STDERR, '>', '/dev/null'; ... }` for stderr suppression

**Generated code** (repeated ~4 times):
```perl
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    ... body ...
};
```

**Preferred idiomatic Perl**:
```perl
{ local $SIG{__WARN__} = sub {}; ... body ... }
```

Or simply use `qx{... 2>/dev/null}` if shelling out, or `no warnings` for native code.

**IR-fixable?** ❌ **No — requires generator logic change.**  
The current generator explicitly models `2>/dev/null` as a stderr redirect in Perl. To fix this, the generator needs to decide that `2>/dev/null` means "suppress errors" which can be done with `local $SIG{__WARN__}` or by using `system()` with redirected stderr. An IR backend can only format the redirect nodes it receives; it cannot decide to translate a redirect into a signal handler.

---

### Pattern H — `rm -f` file-expansion with existence check + directory guard

**Generated code** (repeats for each file):
```perl
if ( -e "test_file.txt" ) {
    if ( -d "test_file.txt" ) {
        carp "rm: carping: ", "test_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_file.txt" ) {
        }
        else {
            carp "rm: carping: could not remove ", "test_file.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
```

**Original**:
```bash
rm -f test_file.txt
```

**Preferred idiomatic Perl**:
```perl
unlink 'test_file.txt';
```

**IR-fixable?** ✅ **Yes, with generator change.**  
The IR would have `System { cmd: "rm", args: ["-f", "test_file.txt"], ... }`. If the generator produces that, the pretty-printer can emit `unlink 'test_file.txt'` if it recognizes `rm -f`. But the current generator produces a cascade of `If` nodes (existence check, directory check, unlink). The IR backend can only format those `If` nodes — it can't collapse them to `unlink`. The fix is in the **generator** producing a simpler IR tree.

---

### Pattern I — `rm -rf` directory-expansion

**Generated code** (~25 lines):
```perl
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("test_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_dir", ": $err->[0]\n";
        }
    }
    else {
        if ( unlink "test_dir" ) { }
        else { carp ... }
    }
}
else {
    local $CHILD_ERROR = 0;
}
};
```

**Original**:
```bash
rm -rf test_dir 2>/dev/null || true
```

**Preferred idiomatic Perl**:
```perl
use File::Path qw(remove_tree);
remove_tree('test_dir');
```

**IR-fixable?** ❌ **No — requires generator logic change.**  
Same analysis as Pattern H. The generator expands `rm -rf` into existence checks, directory/not-directory branches, error handling. An IR backend can't collapse this back down.

---

### Pattern J — `if ($CHILD_ERROR != 0) { 1; }` dangling no-op

**Generated code** at end:
```perl
if ($CHILD_ERROR != 0) {
    1;
}
```

This is from `|| true` at the end of `rm -rf test_dir 2>/dev/null || true`. In shell, `|| true` suppresses the non-zero exit. Here it becomes a no-op with side-effect-free condition.

**Preferred idiomatic Perl**: Omit entirely, or use `eval { remove_tree('test_dir') }`.

**IR-fixable?** ✅ **Yes.**  
The `LogicalOr` or `System` node would understand that `|| true` means "suppress error". The pretty-printer can omit the no-op.

---

### Pattern K — `my $eval_result = eval { ... 1; }; if(!$eval_result) { $CHILD_ERROR = 256; } q{};`

**Generated code** (the error-handling wrapper around every backtick command):
```perl
$CHILD_ERROR = 0;
my $eval_result = eval {
    ... command ...
    1;
};
if ( !$eval_result ) {
    $CHILD_ERROR = 256;
}
q{};
```

This is a shell-to-Perl idiom: the `eval { ... 1; }` catches `croak` (like `set -e`), maps failure to exit code 256 (which maps to 1 in 8-bit). The `q{}` at the end ensures the do-block returns an empty string (stdout of a command that produces no output).

**Preferred idiomatic Perl**: If capturing output, use `qx{}`. If running for side effect, use `system()` or native Perl.

**IR-fixable?** ✅ **Yes.**  
The `System` node with `capture: Some("cp_result")` would produce `my $cp_result = qx{...};`, which handles stderr and exit codes natively.

---

### Pattern L — Inconsistent `use` vs `require`

In the cp block:
```perl
use File::Copy qw(copy);          # at start of eval
...
require File::Copy; File::Copy::copy(...)   # inside condition
```

Same module imported two different ways.

**IR-fixable?** ✅ **Yes.**  
The `IrProgram::imports` field collects all needed imports at the top. The pretty-printer emits `use` statements once. The generator just needs to put `File::Copy` into the imports list and use `copy(...)` as a bare function.

---

## 3. Summary Table: IR-Fixability

| Pattern | IR-Fixable? | IR Node Involved | Root Cause |
|---------|-------------|------------------|------------|
| **A** Echo → say | ✅ Yes | `IrStmt::Output` | Pretty-printer style |
| **B** Triple declaration | ✅ Yes | `IrStmt::Declare` | Generator emits all sigils |
| **C** Backtick do/eval tower | ❌ No | `IrStmt::System`, `IrStmt::Pipeline` | Generator expands `&&` into nested do-blocks + error handling; IR backend can only format what it gets |
| **D** ls emulation (50 lines) | ❌ No | `IrStmt::For`, `IrStmt::If`, `opendir`, etc. | Generator chooses native emulation over system call; ~20 IR nodes cannot collapse to one |
| **E** `$CHILD_ERROR = 0` noise | ✅ Yes | `IrStmt::System` (removes for non-system) | Unnecessary resets between simple prints |
| **F** `do { (expr) }` wrapping | ✅ Yes | `IrStmt::Assign` | Pretty-printer style |
| **G** stderr redirect emulation | ❌ No | `IrStmt::System` with redirects | 2>/dev/null → `local *STDERR; open ...` is a generator lowering choice |
| **H** rm -f expansion | ❌ No | `IrStmt::If` chain | Generator expands to existence check + directory guard + unlink; IR can't collapse |
| **I** rm -rf expansion | ❌ No | `IrStmt::If` chain + `remove_tree` | Same as H |
| **J** `if ($CHILD_ERROR) { 1; }` | ✅ Yes | `IrStmt::If` (dead-code elimination) | Pretty-printer can omit if condition is known |
| **K** eval { ... 1; } / q{} wrapper | ✅ Yes | `IrStmt::System` | Captured command should use `qx{}` |
| **L** Inconsistent import style | ✅ Yes | `IrProgram::imports` | Generator duplicates require logic |

---

## 4. Unnecessarily Verbose Translations — Prime IR Candidates

These are places where the generated code wraps a simple operation in complex control structures that an IR-based backend could simplify dramatically:

### 🏆 #1: Every `echo` statement → `say`

**Current**: 8 lines per echo (`do { my $__echo_line ...; print ...; if(!newline)...; $output .= ...; }`)
**Should be**: 1 line (`say "..."`)
**Frequency**: 14 `echo` statements in this file → **~112 lines → ~14 lines**

IR can fix this with `IrStmt::Output { newline: true }` → `say`.

### 🏆 #2: Every backtick substitution → `qx{}` or native ternary

**Current**: ~15-20 lines per backtick (do/eval/CHILD_ERROR/left_result/right_result)
**Should be**: 1-2 lines (`$var = qx{...};` or `$var = native_call() ? "success" : "";`)
**Frequency**: 6 backtick substitutions → **~100 lines → ~6 lines**

The `do { my $left_result_N = do { ... eval ... 1; ... q{}; }; if(...) { ... right ... } else { q{} } }` structure is the most egregious verbosity. It's a line-by-line transliteration of "run command, capture stdout, check exit code, conditionally run next command".

### 🏆 #3: The `ls` calls → native Perl one-liner

**Current**: ~50 lines each (directory iteration, sorting, header display)
**Should be**: 2-3 lines
**Frequency**: 3 calls + 1 for test_dir → **~200 lines → ~10 lines**

The `ls` emulation generates loops, conditionals, and special-case path handling that Perl's builtin file-test operators and `glob` can handle in one line.

### 🏆 #4: `rm -f` → `unlink`

**Current**: ~10 lines per file (existence check + directory guard + unlink + carp)
**Should be**: 1 line
**Frequency**: Files: test_file.txt, test_file_copy.txt, test_file_moved.txt → **~30 lines → ~3 lines**

### 🏆 #5: `rm -rf` with stderr redirect → `remove_tree`

**Current**: ~25 lines
**Should be**: 1 line

### 🏆 #6: The `touch` emulation → `open` for append

**Current**: 10 lines (existence check with utime branch vs open-for-write branch)
**Should be**: 1 line (`open my $fh, '>>', $file and close $fh`)

---

## 5. Bottom Line

**IR-fixable patterns** (pretty-printer change only, no generator change needed):
- All `echo` → `say` conversion (Pattern A)
- Redundant `$CHILD_ERROR = 0` resets (Pattern E)
- Unnecessary `do { ... }` wrapping (Pattern F)
- eval/q{} wrapper for captured commands (Pattern K)
- Inconsistent import style (Pattern L)
- Dead `if($CHILD_ERROR){1;}` elimination (Pattern J)
- Triple declaration cleanup (Pattern B — if generator passes correct sigil)

**NOT IR-fixable** (require generator logic changes):
- `ls` emulation (Pattern D) — generator must decide to emit `System` instead of native emulation
- `rm -f`/`rm -rf` expansion (Patterns H, I) — generator must produce simpler IR tree
- stderr redirect style (Pattern G) — generator must choose signal-silencing over open-redirect
- Backtick `&&` chaining (Pattern C) — generator must flatten to ternary or `qx{}`

The biggest win from the IR-based approach would be Pattern A (echo→say) and Pattern E/F (noise removal), which are pure style decisions in `ir_to_perl()`. But the truly game-changing simplifications (Patterns C, D, H, I) require the **generator** to produce a fundamentally simpler IR — the backend cannot invent brevity that wasn't in the IR tree.