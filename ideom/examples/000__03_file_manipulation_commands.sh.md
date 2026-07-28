Now I have a thorough understanding of the generator internals. Let me write the complete idiom review.

---

## Idiom Review: `examples/000__03_file_manipulation_commands.sh`

### Original Shell Script

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

### Generated Perl (abridged to key patterns)

The full generated output is ~660 lines for a 36-line shell script. Below are representative excerpts keyed by pattern.

---

### Non-Idiomatic Pattern 1: Echo/print wrapping

**Generated code** (appears ~12 times in the output):

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

**Preferred idiomatic Perl:**

```perl
print "Copy result: $cp_result\n";
#   or, using say:
say "Copy result: $cp_result";
```

**IR-fixable?** **YES** — this is the exact pattern the IR design doc lists in its "Style rules" table. If the generator emits `IrStmt::Output { value: <expr>, newline: true }` instead of constructing a raw do-block with newline detection, the pretty-printer in `ir_to_perl()` can emit a single `say` or `print "...\n"`.

**IR node involved:** `IrStmt::Output { value: IrExpr::Interpolate([Lit("Copy result: "), Expr(Var("cp_result", Scalar))]), newline: true }`

**Cleaned output:**

```
say "Copy result: $cp_result";
```

The dead-code accumulation into `$output` (which is declared `my $output = q{};` but never read anywhere) would also be eliminated by a dead-assignment elimination pass in the IR optimizer.

---

### Non-Idiomatic Pattern 2: Triple variable declaration

**Generated code** (appears 4 times):

```perl
my $cp_result;
my @cp_result;
my %cp_result;
```

**Preferred idiomatic Perl:**

```perl
my $cp_result;
```

**IR-fixable?** **NO** — the issue is in the generator logic, not the pretty-printer. The generator conservatively declares all three sigils because it doesn't perform type inference to determine that only the scalar sigil is used in this script. To fix this, the generator would need to track which sigils are actually referenced for each variable. If it did that analysis and emitted only `IrStmt::Declare { vars: [Decl { name: "cp_result", sigil: Scalar }] }`, the backend would produce just `my $cp_result;`.

---

### Non-Idiomatic Pattern 3: Backtick `&&` chaining — the `left_result`/`right_result` pipeline

**Generated code** (appears 4 times; here for `cp_result`):

```perl
$cp_result = do {
    my $left_result_0 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Copy qw(copy);
            if ( -e 'test_file.txt' ) {
                if ( -d 'test_file_copy.txt' ) {
                    require File::Copy;
                    File::Copy::copy('test_file.txt',
                        'test_file_copy.txt' . '/' . ('test_file.txt' =~ m|([^/]+)$|)[0]);
                } else {
                    require File::Copy;
                    File::Copy::copy('test_file.txt', 'test_file_copy.txt');
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
    };
};
```

**Preferred idiomatic Perl:**

```perl
# Using qx{} (backtick equivalent):
my $cp_result = qx{cp test_file.txt test_file_copy.txt && echo "Copy successful"};
$CHILD_ERROR = $? >> 8;

# Or, using native Perl modules and error handling:
use File::Copy qw(copy);
copy('test_file.txt', 'test_file_copy.txt')
    or die "cp failed: $ERRNO";
my $cp_result = "Copy successful";
```

**IR-fixable?** **PARTIALLY** — there are two distinct issues layered here:

1. **The `&&` chaining structure** (the outer `do { left_result; if CHILD_ERROR==0 { right_result; left . right } else { q{} } }`) — if the generator emitted an `IrExpr::BinOp { lhs, op: Concat, rhs }` or modeled the `&&` as a sequence, the backend could produce `qx{cp ... && echo "..."}`. The IR doc's `System { capture: Some("out") }` node is designed for exactly this.

2. **The inlining of `cp`/`mv`/`rm`/`touch`** — the generator replaces each shell command with a full native Perl emulation (File::Copy, unlink, utime, etc.). This is a **generator-logic decision** that the IR backend cannot undo.

**So: the outer `&&` structure IS IR-fixable** (the backend could emit `qx{cmd1 && cmd2}` instead of the split-and-concatenate pattern), but **the inlining of cp/mv/rm/touch is NOT IR-fixable** — it requires changing the generator to emit `IrStmt::System { cmd: "cp", args: [...], capture: Some("cp_result") }` instead of emitting inline Perl.

**IR node involved (for the fixable part):** `IrStmt::System { cmd: IrExpr::Str("cp", Command), args: [...], capture: Some("cp_result") }`

**Cleaned output (for the `&&` structure, even with inlined commands):**

```perl
$cp_result = do {
    my $left_result_0 = do {
        $CHILD_ERROR = 0;
        eval {
            ...inline cp emulation...
            1;
        };
        $CHILD_ERROR = $@ ? 256 : 0;
        q{};
    };
    if ( $CHILD_ERROR == 0 ) {
        $left_result_0 . "Copy successful";
    } else {
        q{};
    };
};
```

(Removes the redundant `my $right_result_0 = do { ... }` variable and just uses the literal string.)

But the **real** improvement is if the generator emitted a `System` node:

```perl
my $cp_result = qx{cp test_file.txt test_file_copy.txt && echo "Copy successful"};
$CHILD_ERROR = $? >> 8;
```

---

### Non-Idiomatic Pattern 4: `ls` emulation (~60 lines per invocation)

**Generated code** (abbreviated — actual block is ~60 lines, repeated 4 times):

```perl
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
    if (@ls_files_4) { push @ls_files_1, join("\n", @ls_files_4); }
    for my $ls_dir_8 (@ls_dirs_5) {
        ... opendir/readdir/sort entries ...
    }
    if (@ls_files_1) { print join "\n\n", @ls_files_1; print "\n"; }
    if ( $ls_all_found_2 ) { local $CHILD_ERROR = 0; $ls_success = 1; }
    else { local $CHILD_ERROR = 2; $ls_success = 0; ... }
};
if ( !defined $ls_success || $ls_success == 0 ) {
    print "No test files found\n";
}
```

**Preferred idiomatic Perl** (for this simple use case — checking if files exist):

```perl
# Simple: use glob or just -e checks
my @files = grep { -e } qw(test_file.txt test_file_copy.txt test_file_moved.txt);
if (@files) {
    print "$_\n" for @files;
} else {
    print "No test files found\n";
}

# Or, to match ls -1 behavior:
system("ls", "test_file.txt", "test_file_copy.txt", "test_file_moved.txt");
if ($? != 0) { print "No test files found\n"; }
```

**IR-fixable?** **NO** — the generator chooses to emulate `ls` in pure Perl instead of using `system("ls", ...)` or `glob()`. This is a generator-logic decision. The IR has an `IrStmt::System` node that could represent the `ls` call, but the generator currently doesn't use it for `ls`. An IR optimizer could potentially collapse a `for` loop over `-f`/`-d` checks into a simpler form, but that's a very advanced optimization and not what the IR design describes.

**This is also an **unnecessarily verbose translation**: a single `ls` command with a stderr redirect and `||` becomes ~65 lines of Perl. The complexity of the `ls` emulator is disproportionate to the use case (just listing file existence).

---

### Non-Idiomatic Pattern 5: Echo redirect via STDOUT manipulation

**Generated code:**

```perl
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_file.txt'
      or die "Cannot access file: $OS_ERROR\n";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
```

**Preferred idiomatic Perl:**

```perl
# Use system call:
system("echo", "test content", ">", "test_file.txt");

# Or direct Perl I/O:
open my $fh, '>', 'test_file.txt' or die "$OS_ERROR";
print $fh "test content\n";
close $fh;

# Or use shell via qx{}:
qx{echo "test content" > test_file.txt};
```

**IR-fixable?** **NO** — the generator chooses to implement file redirection by manipulating STDOUT at the Perl level. A more idiomatic approach would use `system()` or a dedicated file write. This requires changing the generator to emit `IrStmt::System` or direct file I/O instead of the STDOUT-save/restore dance.

**This is an **unnecessarily verbose translation**: the shell's simple `echo "text" > file` becomes a 10-line STDOUT redirect block. The `do {}` wrapper, the save/restore pattern, and the four `die` calls on every redirect are overhead that Perl's I/O primitives handle directly.

---

### Non-Idiomatic Pattern 6: `touch` emulation (~16 lines per file)

**Generated code** (for `touch test_dir/file`):

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
# Simple:
system("touch", "test_dir/file");

# Or using Perl's utime (which also creates files):
open my $fh, '>>', "test_dir/file" and close $fh;
utime undef, undef, "test_dir/file";
```

**IR-fixable?** **NO** — the generator inlines the `touch` logic. A `System { cmd: "touch", args: ["test_dir/file"] }` node would be much cleaner. This is a generator-logic choice.

---

### Non-Idiomatic Pattern 7: `rm` emulation (~15 lines per file)

**Generated code** (for `rm test_file.txt test_file_moved.txt` — note this repeats for each file):

```perl
if ( -e "test_file.txt" ) {
    if ( -d "test_file.txt" ) {
        croak "rm: ", "test_file.txt", " is a directory (use -r to remove recursively)\n";
    } else {
        if ( unlink "test_file.txt" ) { }
        else { croak "rm: cannot remove ", "test_file.txt", ": $OS_ERROR\n"; }
    }
} else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_file.txt", ": No such file or directory\n";
}
```

**Preferred idiomatic Perl:**

```perl
unlink "test_file.txt", "test_file_moved.txt" or warn "rm failed: $OS_ERROR";
```

Or for the `-f` (force) version:

```perl
unlink "test_file.txt", "test_file_moved.txt";  # silently ignore missing files
```

**IR-fixable?** **NO** — the generator inlines the `rm` logic with full error checking. A `System` node or a simple `unlink` call would be much more idiomatic. This is a generator-logic decision: the generator prefers faithful emulation (matching `rm`'s exact error messages) over conciseness.

---

### Non-Idiomatic Pattern 8: `stderr` redirect to `/dev/null`

**Generated code** (used inside `ls` blocks):

```perl
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
```

**Preferred idiomatic Perl:**

```perl
# Suppress warnings temporarily:
local $SIG{__WARN__} = sub {};

# Or redirect at the system level:
open(local *STDERR, '>', '/dev/null');
```

**IR-fixable?** **NO** — the `local *STDERR; open STDERR...` pattern is actually not terrible Perl in terms of idiom (it's the standard way to redirect stderr locally in Perl). However, the `or croak` on the redirect is odd — if you're redirecting to `/dev/null`, failing to open it should probably be silent too. Changing this to `open(local *STDERR, '>', '/dev/null');` without the error check is a style choice the backend could make if it had an `IrStmt::StderrRedirect` node, but the IR doesn't currently have one. So the **structure** is a generator choice.

---

### Non-Idiomatic Pattern 9: `||` chaining via `$ls_success` variable

**Generated code:**

```perl
# After the ls emulation block:
if ( !defined $ls_success || $ls_success == 0 ) {
    print "No test files found\n";
}
```

**Preferred idiomatic Perl:**

```perl
# Just use system exit code:
system("ls", "...", "...", "...");
$? == 0 or print "No test files found\n";

# Or use Perl's built-in:
my @files = grep { -e } qw(...);
print "$_\n" for @files;
@files or print "No test files found\n";
```

**IR-fixable?** **PARTIALLY** — the `$ls_success` variable is an artifact of the `ls` emulation. If the generator emitted `IrStmt::System { cmd: "ls", args: [...] }`, the backend could produce `system("ls", ...) or print "No test files found\n"`. The `||` chaining in general could be modeled as `IrStmt::If { cond: IrExpr::Not(System { ... }), then: [Output { value: "No test files found", newline: true }] }` and the backend would format it cleanly.

However, the real issue is that the generator currently uses `$ls_success` as a side-channel for exit-code tracking, rather than using Perl's `$?` or the IR's `System` node. This requires generator changes.

---

### Unnecessarily Verbose Translations Summary

These are the places where the generated code wraps a simple shell operation in a wildly disproportionate amount of Perl infrastructure:

| Shell construct | Lines of generated Perl | Root cause |
|---|---|---|
| `echo "text"` | 7–10 | Newline-check do-block + `$output` var |
| `cmd && echo "msg"` | 30–50 | `left_result`/`right_result` split + eval + CHILD_ERROR |
| `ls files... 2>/dev/null` | ~65 | Full `ls` emulator with opendir/readdir/sort/headers |
| `echo "text" > file` | 10 | STDOUT save/restore pattern |
| `touch file` | 15 | Inline if-exists/utime/open/close |
| `rm file` | 15 | Inline -e/-d/unlink with error messages |
| `rm -rf dir` | 20 | Inline remove_tree with error checking |
| `mv file1 file2` | 25+ | Inline File::Copy::move with directory checks |
| `mkdir dir` | 10 | Inline `mkdir` with error checking |

The **backtick `&&` chaining** (Pattern 3) is the most egregious example: a single line `cp_result=\`cp a b && echo "msg"\`` becomes ~40 lines of nested `do { eval { ... } }` blocks. This is the prime candidate for IR-based simplification.

---

### Summary Table

| # | Pattern | IR-fixable? | Requires generator change? |
|---|---|---|---|
| 1 | Echo/print do-block with newline regex | **YES** — `Output { value, newline: true }` → `say` | No (generator must emit `Output` node) |
| 2 | Triple variable declaration (`$x @x %x`) | **NO** — needs type inference in generator | Yes |
| 3a | Backtick `&&` chaining (outer structure) | **YES** — `System { capture }` → `qx{}` | No (generator must emit `System` node) |
| 3b | Backtick `&&` chaining (inlined cp/mv/rm/etc) | **NO** — generator chooses to inline | Yes |
| 4 | `ls` emulation (~60 lines) | **NO** | Yes |
| 5 | Echo redirect via STDOUT manipulation | **NO** | Yes |
| 6 | `touch` emulation | **NO** | Yes |
| 7 | `rm` emulation | **NO** | Yes |
| 8 | `stderr` redirect | **NO** — no IR node for stderr redirect | Yes |
| 9 | `$ls_success` for `\|\|` chaining | **PARTIAL** — if using `System` nodes | Yes |

**Bottom line:** The patterns that are IR-fixable (1 and 3a) are precisely the ones the IR design doc was created to address — formatting choices for `Output`, `System`, and `Declare` nodes. The remaining 7 patterns require changes to the generator's fundamental approach: instead of emulating shell commands in native Perl, the generator should emit `IrStmt::System` nodes that tell the backend "run this command," letting it decide whether to use `system()`, `qx{}`, or native Perl code. The IR backend can then apply consistent formatting, dead-code elimination (e.g., removing the unused `$output` accumulation), and import minimization across all generated code.