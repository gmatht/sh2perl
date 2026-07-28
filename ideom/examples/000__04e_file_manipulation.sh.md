Let me start by reading the IR design doc and understanding the codebase structure.Let me also look at the generator source for context on how this is produced.Now I have a thorough understanding of the codebase. Let me write the idiom review.

---

## Idiom Review: `examples/000__04e_file_manipulation.sh` → Generated Perl

### 1. Original Shell Script and Generated Perl

**Shell script (30 lines of actual logic):**
```bash
echo "=== File Manipulation Commands ==="
echo "test content" > test_file.txt
cp_result=`cp test_file.txt test_file_copy.txt && echo "Copy successful"`
echo "Copy result: $cp_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"
mv_result=`mv test_file_copy.txt test_file_moved.txt && echo "Move successful"`
echo "Move result: $mv_result"
ls ... || echo "No test files found"
rm_result=`rm test_file.txt test_file_moved.txt && echo "Remove successful"`
echo "Remove result: $rm_result"
ls ... || echo "No test files found"
mkdir_result=`mkdir test_dir && echo "Directory created"`
echo "Mkdir result: $mkdir_result"
touch test_dir/file
ls test_dir 2>/dev/null || echo "Directory not found"
touch_result=`touch test_file.txt && echo "File touched"`
echo "Touch result: $touch_result"
rm -f test_file.txt test_file_copy.txt test_file_moved.txt
rm -rf test_dir 2>/dev/null || true
echo "=== File Manipulation Commands Complete ==="
```

**Generated Perl: ~380 lines.** The full output was shown above. Below I catalog each non-idiomatic pattern.

---

### 2. Non-Idiomatic Patterns

#### Pattern A: `echo "text" > file` → STDOUT-hijacking do-block

**Generated:**
```perl
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_file.txt'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
        say "test content";
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
```

**Problems:**
- STDOUT is global — hijacking it is fragile; `print $tmp` writes the return value of `say` (i.e., `1`) into the file after the content, which is a **bug** (file gets `"test content\n1"` instead of `"test content\n"`).
- `say "test content"` already prints to the redirected STDOUT, then `print $tmp` prints `1` on top of it.

**Preferred idiomatic Perl:**
```perl
open my $fh, '>', 'test_file.txt' or die "Cannot open test_file.txt: $!\n";
print $fh "test content\n";
close $fh;
```

**IR-fixable? NO.** The current IR has no `WriteFile` node and no redirect-on-STDOUT concept. The generator in `command_dispatcher.rs` explicitly emits this STDOUT-save/restore pattern via `format!()` strings. An IR-based backend would need:
1. A new IR node: `WriteFile { path: IrExpr, content: IrExpr, append: bool }` or an `Output` node extended with a redirect target
2. A generator that emits that node instead of STDOUT-hijacking

This requires changing both the IR type definitions and the generator logic. Pretty-printer changes alone cannot fix it.

---

#### Pattern B: Backtick `cmd1 && echo "success"` → eval-wrapped emulation do-block

**Generated (for `cp_result=\`cp ... && echo "Copy successful"\``):**
```perl
my $cp_result = do {
    my $left_result_0 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Copy qw(copy);
            if ( -e 'test_file.txt' ) {
                if ( -d 'test_file_copy.txt' ) {
                    require File::Copy;
                    File::Copy::copy('test_file.txt', 'test_file_copy.txt' . '/' . ('test_file.txt' =~ m|([^/]+)$|)[0]);
                } else {
                    require File::Copy;
                    File::Copy::copy('test_file.txt', 'test_file_copy.txt');
                }
            } else {
                croak "cp: cannot stat 'test_file.txt': No such file or directory\n";
            }
            1;
        };
        if ( !$eval_result ) { $CHILD_ERROR = 256; }
        q{};
    };
    if ($CHILD_ERROR == 0) {
        my $right_result_0 = do { "Copy successful" };
        $left_result_0 . $right_result_0;
    } else {
        q{};
    }
};
```

**Problems:**
- The generator **emulates** `cp`, `mv`, `rm`, `mkdir`, `touch` as native Perl operations instead of shelling out. This is 20–30× more code.
- The `&& echo "success"` pattern produces nested `do { ... . ... }` that concatenates the left command's output (always `""` for cp) with the right string.
- Each command is wrapped in `eval { ... 1; }; if (!$eval_result) { $CHILD_ERROR = 256; }` — this is error-handling infrastructure that should be implicit.
- `$CHILD_ERROR = 0;` before each command.

**Preferred idiomatic Perl:**
```perl
my $cp_result = qx{cp test_file.txt test_file_copy.txt && echo "Copy successful"};
chomp $cp_result;
```

Or even simpler, since cp's output is never meaningful:
```perl
system("cp", "test_file.txt", "test_file_copy.txt") == 0
    or die "cp failed: $?\n";
my $cp_result = "Copy successful";
```

**IR-fixable? NO.** The generator chooses to **emulate** these commands as native Perl. The `File::Copy::copy()` call, the `-e` checks, the `-d` checks, the directory-aware copying — all are emitted by the `cp.rs`, `mv.rs`, `rm.rs`, `touch.rs`, `mkdir.rs` generator modules. Even if the generator emitted `IrStmt::System` nodes instead, the IR pretty-printer could only format them as `system()` or `qx{}`; it could not retroactively decide to emulate vs. shell-out. That decision belongs to the generator logic.

The one sub-pattern that **is** IR-fixable is the `my $left_result_N = do { ... q{} }` / `$left_result_N . $right_result_N` concatenation — but only if the generator emitted a proper concatenation expression in the IR (e.g., `IrExpr::BinOp { op: Concat, ... }`). Currently it emits `RawText` for this whole block.

---

#### Pattern C: Full `ls` emulation (~40 lines per invocation)

**Generated (abbreviated — appears 4× in the output):**
```perl
do {
    local *STDERR;
    open STDERR, '>', '/dev/null' or croak "...";
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
    # ... sorting, directory listing, header printing ...
    if ( $ls_all_found_2 ) { $ls_success = 1; }
    else { $ls_success = 0; }
};
```

**Problems:**
- The generator fully reimplements `ls` in Perl (file-existence checks, directory reading via `opendir`/`readdir`, sorting via Schwartzian transform, header display logic).
- Each of the four `ls` calls generates identical 40-line copies with different variable numbers (`_1`, `_11`, `_21`, `_32`).

**Preferred idiomatic Perl:**
```perl
my @files = qw(test_file.txt test_file_copy.txt test_file_moved.txt);
say for grep { -e } @files;
```

Or if shelling out is acceptable:
```perl
system("ls", "test_file.txt", "test_file_copy.txt", "test_file_moved.txt");
```

**IR-fixable? NO.** The generator's `ls.rs` module emits the full emulation via `format!()` calls. The IR does not have an `Ls` node. An IR-based backend could only fix this if the generator emitted `IrStmt::System { cmd: "ls", args: [...] }`, which the pretty-printer could then format as `system("ls", ...)`. But the generator chooses emulation, not system calls.

---

#### Pattern D: `||` / `&&` exit-status handling → flag variables

**Generated (for `ls ... || echo "No test files found"`):**
```perl
if ( $ls_all_found_22 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
} else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
    $main_exit_code = $CHILD_ERROR;
}
# ... 20 lines later ...
if ( !defined $ls_success || $ls_success == 0 ) {
    say "No test files found";
}
```

**Problems:**
- Introduces a flag variable `$ls_success` that is set and then tested far away.
- The condition `!defined $ls_success || $ls_success == 0` is defensive boilerplate; in practice `$ls_success` is always defined when used.
- `$main_exit_code = $CHILD_ERROR` inside the else branch.

**Preferred idiomatic Perl:**
```perl
# No flag variable needed:
my @existing = grep { -e $_ } qw(test_file.txt test_file_copy.txt test_file_moved.txt);
say for @existing;
say "No test files found" unless @existing;
```

**IR-fixable? PARTIALLY.**
- The `if ( !defined $ls_success || $ls_success == 0 )` pattern: if the generator emitted `IrStmt::If { cond: IrExpr::Not(IrExpr::Var("ls_success")), ... }`, the pretty-printer could format it as `if (!$ls_success)`. But the generator emits the full condition as `RawText` or uses `format!()`.
- The flag variable itself is a generator-logic decision and cannot be eliminated by the pretty-printer.

---

#### Pattern E: `$main_exit_code = 0;` after every section

**Generated (appears after each `ls || echo` block):**
```perl
$main_exit_code = 0;
```

This appears 4 times, always resetting `$main_exit_code` to 0 after the ls-emulation else-branch set it.

**IR-fixable? YES.** If the generator emitted this as `IrStmt::Assign { targets: ["main_exit_code"], expr: IrExpr::Int(0) }`, a dead-assignment elimination pass on the IR could remove assignments that are immediately overwritten before use. The IR design doc explicitly lists "dead assignment elimination" as an optimization pass.

---

#### Pattern F: `2>/dev/null` → `local *STDERR; open STDERR, '>', '/dev/null'`

**Generated (appears before each `ls` emulation):**
```perl
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
```

**Problems:**
- `local *STDERR` is unnecessary; `STDERR` is a global typeglob, and `open STDERR, ...` already affects the global handle. The `local` is cargo-culted from variable-localization patterns.
- `croak` on failure to open `/dev/null` is paradoxical — if stderr is broken, where does the croak go?

**Preferred idiomatic Perl:**
```perl
open STDERR, '>', '/dev/null' or warn "Cannot redirect stderr: $!";
```

Or, simpler:
```perl
close STDERR;
```

**IR-fixable? NO.** The `local *STDERR` is emitted by `redirects.rs`'s `generate_redirect_impl`. The IR does not have a "redirect stderr" node. This is a generator-level pattern.

---

#### Pattern G: Cleanup removes each file individually with if-blocks

**Generated (for `rm -f test_file.txt test_file_copy.txt test_file_moved.txt`):**
```perl
if ( -e "test_file.txt" ) {
    if ( -d "test_file.txt" ) { carp "rm: carping: ..."; }
    else { if ( unlink "test_file.txt" ) { } else { carp "..."; } }
} else { local $CHILD_ERROR = 0; }
if ( -e "test_file_copy.txt" ) { ... identical block ... }
if ( -e "test_file_moved.txt" ) { ... identical block ... }
```

**Problems:**
- Three identical if-blocks instead of a loop.
- `if ( unlink "test_file.txt" ) { }` — empty true branch is a code smell.

**Preferred idiomatic Perl:**
```perl
for my $f (qw(test_file.txt test_file_copy.txt test_file_moved.txt)) {
    unlink $f or carp "rm: could not remove $f: $OS_ERROR\n";
}
```

**IR-fixable? NO.** The `rm.rs` generator processes each file argument separately, emitting individual blocks. This is a generator-logic issue — the IR can't merge three separate `If` nodes into a `For` loop.

---

#### Pattern H: `rm -rf test_dir 2>/dev/null || true` → `remove_tree` + `|| true` boilerplate

**Generated:**
```perl
do {
    local *STDERR;
    open STDERR, '>', '/dev/null' or croak "...";
    if ( -e "test_dir" ) {
        if ( -d "test_dir" ) {
            my $err;
            require File::Path;
            File::Path::remove_tree("test_dir", {error => \$err});
            # ... error handling ...
        } else { unlink "test_dir"; }
    } else { local $CHILD_ERROR = 0; }
};
if ($CHILD_ERROR != 0) { 1; }
```

**Problems:**
- `|| true` produces `if ($CHILD_ERROR != 0) { 1; }` — a statement whose result is discarded.
- `remove_tree` with error checking when `2>/dev/null` discards errors.

**Preferred idiomatic Perl:**
```perl
system("rm", "-rf", "test_dir");
# or
use File::Path qw(remove_tree);
remove_tree("test_dir");
```

**IR-fixable? PARTIALLY.**
- The `if ($CHILD_ERROR != 0) { 1; }` is the `|| true` idiom. If the generator emitted this via IR, a dead-code pass could eliminate it entirely (it's a no-op). But currently it's `RawText`.
- The `remove_tree` choice is generator logic.

---

#### Pattern I: `touch test_dir/file` → native emulation

**Generated (non-backtick `touch`):**
```perl
if ( -e "test_dir/file" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_dir/file";
} else {
    if ( open my $fh, '>', "test_dir/file" ) {
        close $fh or croak "Close failed: $ERRNO";
    } else {
        croak "touch: cannot create ", "test_dir/file", ": $ERRNO\n";
    }
}
```

**Preferred idiomatic Perl:**
```perl
system("touch", "test_dir/file");
# or simply:
open my $fh, '>>', "test_dir/file" and close $fh;
```

**IR-fixable? NO.** Generator logic in `touch.rs` chooses to emulate.

---

#### Pattern J: Stray `$main_exit_code = 0;` assignments

**Generated (4 occurrences):**
```perl
$main_exit_code = 0;
```

These reset `$main_exit_code` after each `ls || echo` block, even though `$main_exit_code` was just assigned inside the block and won't be read before the `exit $main_exit_code` at the end.

**IR-fixable? YES.** Dead-assignment elimination in an IR optimization pass can remove these.

---

#### Pattern K: Excessive numeric suffixes on variables

**Generated:** `@ls_files_1`, `$ls_all_found_2`, `@ls_inputs_3`, `@ls_files_4`, `@ls_dirs_5`, `$ls_show_headers_6`, `$ls_item_7`, `$ls_dir_8`, `@ls_dir_entries_9`...

These are generated by `generator.get_unique_id()`. Each `ls` call gets its own numeric namespace.

**IR-fixable? NO.** Variable naming is a generator concern, not an IR concern. The IR only knows about `IrExpr::Var("ls_files_1", Array)`.

---

### 3. Summary Table

| # | Pattern | Root Cause | IR-Fixable? |
|---|---------|------------|-------------|
| A | `echo > file` → STDOUT hijacking | `command_dispatcher.rs` redirect handler | **No** — needs new IR node |
| B | Backtick `cmd1 && echo x` → eval/do infrastructure | Backtick handler emulates commands | **No** — emulation is generator logic |
| C | `ls` emulation (~40 lines × 4) | `ls.rs` generator | **No** — emulation is generator logic |
| D | `\|\|` → flag variable + far-away check | `ls.rs` exit-status handling | Partial — truthy simplification |
| E | `$main_exit_code = 0;` dead assignments | Pipeline/exit handling | **Yes** — dead-assignment elimination |
| F | `local *STDERR; open STDERR, '>', '/dev/null'` | `redirects.rs` stderr handling | **No** — generator pattern |
| G | Cleanup: individual if-blocks per file | `rm.rs` per-file iteration | **No** — generator logic |
| H | `\|\| true` → `if ($CHILD_ERROR != 0) { 1; }` | Boolean-expression-as-statement | Partial — dead-code elimination |
| I | `touch` emulation | `touch.rs` generator | **No** — emulation is generator logic |
| J | `$main_exit_code = 0` resets | Pipeline/exit handling | **Yes** — dead-assignment elimination |
| K | Numeric suffixes on variables | `get_unique_id()` | **No** — generator naming |

---

### 4. Unnecessarily Verbose Translations (Prime IR Candidates)

These are operations where the generated code uses enormous infrastructure for trivial work:

1. **`echo "test content" > test_file.txt`** → **16 lines** of STDOUT hijacking for a one-line file write. The generator emits a do-block, saves/restores STDOUT, runs the print, and captures the return value. A native Perl one-liner would be `print $fh "test content\n"`. (IR node needed: `WriteFile`)

2. **`cp_result=\`cp ... && echo "success"\``** → **18 lines** of emulation + concatenation for something that in Perl is `qx{...}`. The do-block infrastructure (`$left_result_N`, `$right_result_N`, `$CHILD_ERROR`, `eval`) dwarfs the actual operation. (IR node exists: `System { capture }` — but generator doesn't use it)

3. **Four `ls` calls** → **~160 lines total** (40 each) for what could be 1 line per call. Each emulation reimplements `ls` with opendir/readdir/sort/headers. (IR node needed: none — generator should shell out)

4. **Backtick `touch`** → **22 lines** (do-block + eval + utime/open) for `system "touch", "test_file.txt"`.

5. **Backtick `mkdir`** → **14 lines** (do-block + eval + mkdir + concatenation) for `system "mkdir", "test_dir"` or Perl's `mkdir`.

6. **`rm -f a b c`** → **~60 lines** (three identical if-blocks) for three `unlink` calls.

7. **Each `||` / `&&` exit-status check** → extra `$CHILD_ERROR = 0;` / `local $CHILD_ERROR = N;` / flag variable assignments.

All of these are candidates for an IR-based approach because:
- They involve operations that could be represented as high-level IR nodes (e.g., `System { capture }` for backticks, `WriteFile` for redirects)
- The verbosity comes from the *way* the generator assembles Perl text, not from semantic necessity
- An IR backend could apply optimizations (dead-code elimination, capture-vs-emit decisions) that are impossible when code is already opaque text

However, as the analysis above shows, most of these patterns **cannot be fixed by pretty-printer changes alone** — they require the generator to emit semantic IR nodes instead of RawText. The IR design doc's migration plan (step 3: wrap everything in RawText; step 4: migrate function by function) is precisely the work needed to unlock these improvements.