Now I have a thorough understanding of both the generator internals and the IR design. Let me write the idiom review.

---

## Idiom Review: `sh/000__04e_file_manipulation.sh` → Generated Perl

### 1. Original Shell Script and Generated Perl

**Original shell script** (`/nvme/ai/sh2loop/sh/000__04e_file_manipulation.sh`):

```bash
echo "test content" > test_file.txt
cp_result=`cp test_file.txt test_file_copy.txt && echo "Copy successful"`
echo "Copy result: $cp_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

mv_result=`mv test_file_copy.txt test_file_moved.txt && echo "Move successful"`
echo "Move result: $mv_result"
ls ... || echo "..."  # (repeated pattern)

rm_result=`rm test_file.txt test_file_moved.txt && echo "Remove successful"`
echo "Remove result: $rm_result"
ls ... || echo "..."  # (repeated pattern)

mkdir_result=`mkdir test_dir && echo "Directory created"`
echo "Mkdir result: $mkdir_result"
touch test_dir/file
ls test_dir 2>/dev/null || echo "Directory not found"

touch_result=`touch test_file.txt && echo "File touched"`
echo "Touch result: $touch_result"

rm -f test_file.txt test_file_copy.txt test_file_moved.txt
rm -rf test_dir 2>/dev/null || true
```

**Generated Perl** (truncated to key regions for review):

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

say "=== File Manipulation Commands ===";
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
my $cp_result = do {
    my $left_result_0 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Copy qw(copy);
            if ( -e 'test_file.txt' ) { ... copy(...) ... }
            1;
        };
        if ( !$eval_result ) { $CHILD_ERROR = 256; }
        q{};
    };
    if ($CHILD_ERROR == 0) {
        my $right_result_0 = do { "Copy successful" };
        $left_result_0 . $right_result_0;
    } else { q{}; }
};
# ... repeated for mv, rm, mkdir, touch ...
```

---

### 2. Non-idiomatic Patterns

#### Pattern A — Echo-with-redirect expanded to `do { open STDOUT ... }` block (1 → 16 lines)

**Generated code:**
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

**Idiomatic Perl:**
```perl
say "test content" > 'test_file.txt';   # or simply:
# ...or since Perl can't redirect say output to a file:
use File::Slurp qw(write_file);
write_file('test_file.txt', "test content\n");
# ...or for an exact match:
{
    open my $fh, '>', 'test_file.txt' or die "open: $!";
    print $fh "test content\n";
    close $fh;
}
```

The current code creates a temporary buffer `$tmp`, captures the output of `say`, then prints `$tmp` to the redirected handle. This is unnecessary — Perl can open the file and write directly.

**IR-fixable?** **YES.** An `Output` IR node (with a file redirection hint) should emit a direct `print FH` call. The current approach is a "save/redirect/restore stdout" pattern that belongs in the generator logic, but the IR backend could recognize that the Output's redirection target is a file path (not another command) and emit `open my $fh, '>', $path; print $fh ...; close $fh;` instead of the STDOUT-capture dance.

**Involved IR node:** `IrStmt::Output { value, newline }` combined with a redirection target on the statement. If the IR had a `Redirect` field on statements (or a `Pipeline { stages }` where a redirect turns into a file write), the backend could choose the right pattern.

**Cleaned-up output through IR:**
```perl
{
    open my $fh, '>', 'test_file.txt' or die "open: $OS_ERROR";
    say $fh "test content";
    close $fh;
}
```
or even simpler if the IR backend has a `WriteFile` shorthand.

---

#### Pattern B — Backtick command substitution → `do { ... eval { ... } ... $CHILD_ERROR ... }` monster

**Generated code:**
```perl
my $cp_result = do {
    my $left_result_0 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Copy qw(copy);
            if ( -e 'test_file.txt' ) {
                if ( -d 'test_file_copy.txt' ) {
                    require File::Copy; File::Copy::copy(...);
                } else {
                    require File::Copy; File::Copy::copy(...);
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
    } else { q{}; }
};
```

**Idiomatic Perl (for a backtick that runs `cp ... && echo "Copy successful"`):**
```perl
my $cp_result;
if (system('cp', 'test_file.txt', 'test_file_copy.txt') == 0) {
    $cp_result = "Copy successful\n";
} else {
    $cp_result = '';
}
chomp $cp_result;   # strip trailing newline like backticks do
```

Or using `qx{}` if you really want the shell:
```perl
my $cp_result = qx{cp test_file.txt test_file_copy.txt && echo "Copy successful"};
$CHILD_ERROR = $? >> 8;
```

The core problem: the generator replaces the `cp` command with an emulated Perl-native `copy` call, then wraps the `&& echo "Copy successful"` logic in a complex `$left_result_N . $right_result_N` concatenation inside a `do` block. This turns a simple two-command pipeline (`cmd && msg`) into ~30 lines of nested `do` blocks, `eval`, and `$CHILD_ERROR` checking.

**IR-fixable?** **PARTIALLY.** The IR `System { cmd, args, capture }` node could represent this. The issue is that the *generator logic* already decided to inline `cp` as `File::Copy::copy()` and then tries to stitch together the `&&` pipeline by hand. The IR backend could clean up the PRETTY-PRINTING of the result — e.g., if it saw `System { capture: Some("cp_result") }` it could emit `my $cp_result = qx{...}` directly. But the "split into two halves and concatenate" pattern (the `$left_result_N . $right_result_N` structure) is baked into how the generator handles `&&` inside backticks. That's a **generator logic** decision, not a pretty-printing decision.

However, if the upstream generator produced an IR node like:
```
Pipeline { stages: [
    [System { cmd: "cp", args: [...], capture: None }],
    [Output { value: Str("Copy successful", DoubleQuoted), newline: false }]
], capture: Some("cp_result") }
```
Then the IR backend could recognize this as a simple two-stage pipeline and emit:
```perl
my $cp_result = do {
    system('cp', 'test_file.txt', 'test_file_copy.txt') == 0
        ? "Copy successful"
        : ''
};
```

But the current generator bypasses IR for this — it emits everything as `RawText`. So while the IR *backend* is capable of better output, the *generator* must first be changed to emit semantic IR nodes instead of pre-assembled text.

**Verdict:** NOT IR-fixable in the current architecture. The generator's `generate_command_substitution` logic would need to emit `IrStmt::System` and `IrStmt::Pipeline` nodes rather than raw text. The IR backend can then pretty-print cleanly.

---

#### Pattern C — `ls` emulation (1 line → ~55 lines)

**Generated code** (one of four identical copies):
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
        my @ls_dir_entries_9 = ();
        if ( opendir my $dh, $ls_dir_8 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                push @ls_dir_entries_9, $file;
            }
            closedir $dh;
            @ls_dir_entries_9 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_9;
            # ... header logic ...
        }
    }
    if (@ls_files_1) { print join "\n\n", @ls_files_1; print "\n"; }
    # ... $ls_success / $main_exit_code tracking ...
};
if ( !defined $ls_success || $ls_success == 0 ) {
    say "No test files found";
}
$main_exit_code = 0;
```

**Idiomatic Perl:**
```perl
# For "ls files 2>/dev/null || echo msg" — just try to stat:
my @files = qw(test_file.txt test_file_copy.txt test_file_moved.txt);
my @existing = grep { -e $_ } @files;
if (@existing) {
    say for @existing;
} else {
    say "No test files found";
}
```

Or if you genuinely want to emulate `ls` directory listing:
```perl
use File::Slurp qw(read_dir);
my @entries = grep { !/^\./ } read_dir('test_dir');
say for @entries;
```

The current emulation is a full reimplementation of `ls`'s behavior (sort, headers, directory recursion, exit codes, stderr suppression). The script only uses `ls` to check whether files exist (it's a proxy for "are these files present?"). An idiomatically translated script would use the simplest Perl operation that achieves the same observable behavior.

**IR-fixable?** **NO.** This is a generator logic problem. The generator has a special `generate_ls_command` that produces a complete `ls` emulation. The IR backend only sees the resulting `RawText` block. To get idiomatic output, the generator needs to recognize that the script's `ls` usage is a simple existence check and emit `-e` checks instead of a full `ls` emulation. Or, at minimum, the generator should produce an `IrStmt::System { cmd: "ls", ... }` node and let the IR backend decide whether to emulate or shell out.

---

#### Pattern D — `rm` backtick expansion → eval-based block with per-file error checking

**Generated code** (for `rm_result=`rm test_file.txt test_file_moved.txt && echo "Remove successful"``):

```perl
my $rm_result = do {
    my $left_result_20 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_file.txt" ) {
                if ( -d "test_file.txt" ) {
                    croak "rm: ... is a directory...\n";
                } else {
                    if ( unlink "test_file.txt" ) { }
                    else { croak "rm: cannot remove ..."; }
                }
            } else { ... }
            if ( -e "test_file_moved.txt" ) { ... same pattern ... }
            1;
        };
        if ( !$eval_result ) { $CHILD_ERROR = 256; }
        q{};
    };
    if ($CHILD_ERROR == 0) {
        my $right_result_20 = do { "Remove successful" };
        $left_result_20 . $right_result_20;
    } else { q{}; }
};
```

**Idiomatic Perl:**
```perl
my $rm_result = do {
    my $ok = 1;
    for my $f (qw(test_file.txt test_file_moved.txt)) {
        if (-e $f) {
            unlink $f or $ok = 0;
        } else {
            $ok = 0;
        }
    }
    $ok ? "Remove successful" : '';
};
# or simpler:
my $rm_result = '';
if (unlink qw(test_file.txt test_file_moved.txt)) {
    $rm_result = "Remove successful\n";
}
```

Same issue as Pattern B: the `&&` pipeline is decomposed into a manual `$left_result . $right_result` pattern. The per-file error checking is also extremely defensive — shell `rm` without `-f` would fail on first error, but the generated code checks each file independently and reports errors via `croak`.

**IR-fixable?** Same as Pattern B. **PARTIALLY.** The IR backend could prettify the `System { capture }` node, but the generator's choice to inline `rm → unlink` with per-file error handling is a logic decision.

---

#### Pattern E — `touch` backtick expansion → native Perl implementation

**Generated code:**
```perl
my $touch_result = do {
    my $left_result_41 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_file.txt" ) {
                my $current_time = time;
                utime $current_time, $current_time, "test_file.txt";
            } else {
                if ( open my $fh, '>', "test_file.txt" ) {
                    close $fh or croak "Close failed: $ERRNO";
                } else {
                    croak "touch: cannot create ...";
                }
            }
            $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) { $CHILD_ERROR = 256; }
        q{};
    };
    if ($CHILD_ERROR == 0) {
        my $right_result_41 = do { "File touched" };
        $left_result_41 . $right_result_41;
    } else { q{}; }
};
```

**Idiomatic Perl:**
```perl
my $touch_result = do {
    { utime time, time, 'test_file.txt' or
      open my $fh, '>>', 'test_file.txt' and close $fh or die "touch: $!" }
    ? "File touched"
    : ''
};
```

Or use `File::Touch`:
```perl
use File::Touch;
my $touch_result = (touch('test_file.txt') ? "File touched" : '');
```

**IR-fixable?** Same as B/D — the nested `do { do { eval { ... } } }` wrapping is the problem. If the generator emitted `System { cmd: "touch", args: [...], capture: Some("touch_result") }`, the IR backend could emit a clean `qx{}` or `system()` call. But the inlining of `touch → utime/open` is a generator choice.

---

#### Pattern F — `mkdir` backtick expansion with Perl-native `mkdir`

**Generated code:**
```perl
my $mkdir_result = do {
    my $left_result_30 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Path qw(make_path);
            if ( mkdir 'test_dir' ) { }   # <-- empty if body!
            else { croak "mkdir: cannot create directory ..."; }
            $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) { $CHILD_ERROR = 256; }
        q{};
    };
    if ($CHILD_ERROR == 0) {
        my $right_result_30 = do { "Directory created" };
        $left_result_30 . $right_result_30;
    } else { q{}; }
};
```

Note the empty `if ( mkdir 'test_dir' ) { }` block — a dead giveaway of line-by-line transliteration. Bash would silently discard the command's stdout, so the Perl translation has a no-op block.

**Idiomatic Perl:**
```perl
my $mkdir_result = mkdir('test_dir') ? "Directory created\n" : '';
```

**IR-fixable?** The empty-if-body is an IR backend formatting issue — the `If` node's `then` branch is an empty vec, which the backend formats as `if (cond) {\n}`. The backend could suppress the braces for empty bodies, or the generator could avoid producing an `If` when the body is empty. **YES, IR-fixable** — the IR backend should omit the empty `if` block entirely and just evaluate the condition for its side effect. But the `do` nesting (Pattern B) still requires generator changes.

---

#### Pattern G — `rm -f file` expanded to per-file if/else blocks with carp

**Generated code:**
```perl
if ( -e "test_file.txt" ) {
    if ( -d "test_file.txt" ) {
        carp "rm: carping: ... is a directory...\n";
    } else {
        if ( unlink "test_file.txt" ) { } else {
            carp "rm: carping: could not remove ...";
        }
    }
} else {
    local $CHILD_ERROR = 0;
}
# ... repeated for each file ...
```

**Idiomatic Perl:**
```perl
unlink 'test_file.txt', 'test_file_copy.txt', 'test_file_moved.txt';
```

Or with `-f` semantics (ignore errors):
```perl
for my $f (qw(test_file.txt test_file_copy.txt test_file_moved.txt)) {
    unlink $f;
}
```

The current code unconditionally emits `carp` warnings for `cp`, `mv`, `rm` operations even though the original script deliberately suppresses errors with `2>/dev/null` and `|| true`. The use of `carp` (a warning, not an error) is an odd choice — it neither matches shell semantics (which would either fail silently with `-f` or print to stderr) nor Perl best practices.

**IR-fixable?** **PARTIALLY.** The IR backend could clean up the `if (unlink ...) { }` empty-body pattern (same empty-if issue as Pattern F). But the `-f` semantics (suppress errors) and the `carp` vs `croak` decision are generator logic choices.

---

#### Pattern H — `$main_exit_code = 0;` repeatedly reset

**Generated code:**
```perl
$main_exit_code = 0;
# ...
$main_exit_code = 0;
# ...
$main_exit_code = 0;
# emitted after every ls-clause
```

Each `$ls_success` block resets `$main_exit_code = 0` after deciding whether to set it to `$CHILD_ERROR`. Since the script never uses non-zero exit codes meaningfully (every `||` handler resets it), this is dead assignment.

**Idiomatic Perl:** No `$main_exit_code` at all, or a single assignment at the end.

**IR-fixable?** **YES, but requires an optimization pass.** The IR backend, given an `Assign { targets: [var: "main_exit_code"], expr: ... }` node, could run dead-assignment elimination (a MIR pass). If it sees three consecutive `$main_exit_code = 0` assignments before the final `exit $main_exit_code`, it could eliminate the redundant ones. The IR design doc mentions "Dead assignment elimination" as a benefit. This would require converting the generator to emit `IrStmt::Assign` nodes instead of raw text.

---

#### Pattern I — `touch test_dir/file` emulated as if/else with utime/open

**Generated code:**
```perl
if ( -e "test_dir/file" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_dir/file";
} else {
    if ( open my $fh, '>', "test_dir/file" ) {
        close $fh or croak "Close failed: $ERRNO";
    } else {
        croak "touch: cannot create ...";
    }
}
```

**Idiomatic Perl:**
```perl
# touch-like behavior in one statement:
open my $fh, '>>', "test_dir/file" and close $fh or die "touch: $!";
```

The `>>` open mode creates the file if it doesn't exist AND updates its mtime if it does — exactly what `touch` does.

**IR-fixable?** **NO.** This requires generator logic to recognize `touch` as a special command and emit the correct Perl idiom. The current generator does recognize `touch` and produces the `if (-e) { utime } else { open }` pattern, but it's overly defensive.

---

#### Pattern J — `rm -rf test_dir 2>/dev/null || true` expanded to do block

**Generated code:**
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
            carp "rm: carping: could not remove ...";
        } else { }
    } else {
        if ( unlink "test_dir" ) { } else { carp ... }
    }
} else { local $CHILD_ERROR = 0; }
};
if ($CHILD_ERROR != 0) { 1; }
```

**Idiomatic Perl:**
```perl
use File::Path qw(remove_tree);
remove_tree('test_dir');
```

Or with error suppression:
```perl
eval { remove_tree('test_dir') };
```

The current code re-stderr-suppresses (in a do block), checks if the path exists, checks if it's a directory or file, handles errors from `remove_tree`, and then has an extraneous `if ($CHILD_ERROR != 0) { 1; }` to implement `|| true`.

**IR-fixable?** The `do { local *STDERR; open STDERR... }` stderr redirection is boilerplate that the generator adds for ALL commands with `2>/dev/null`. This could be an IR optimization: if a statement has a "suppress stderr" flag, the IR backend could either wrap in `local *STDERR` or just prefix with `eval { ... }` for silent error handling. **PARTIALLY IR-fixable** — the stderr suppression pattern could be abstracted into an IR node attribute.

---

### 3. Summary: Verbose vs Idiomatic Translation

| Shell line | Generated lines | Idiomatic lines | Ratio |
|---|---|---|---|
| `echo "..." > file` | 16 (do { open... }) | 1–3 | 5–16× |
| `cmd_result= \`cmd && echo msg\`` | ~30 per backtick | 1–3 | 10–30× |
| `ls ... 2>/dev/null` | ~55 per `ls` call | 3–5 | 11–18× |
| `rm -f file1 file2` | ~18 (per-file if/else) | 1 | 18× |
| `touch file` (inline) | ~12 (if/else utime/open) | 1–2 | 6–12× |
| `rm -rf dir 2>/dev/null \|\| true` | ~20 (do/if/remove_tree) | 1 | 20× |
| **Total** | **~290 lines** | **~15–25 lines** | **~12–19×** |

---

### 4. Which Problems Are IR-Fixable?

| # | Pattern | IR-fixable? | IR Node Involved | Clean Output |
|---|---|---|---|---|
| **A** | Echo redirect → STDOUT save/restore | **Yes** | `Output` + redirect attribute | `{ open $fh, '>', $path; say $fh "msg"; close $fh }` |
| **B** | Backtick → nested `do { eval { ... } }` | **No** (generator logic) | Would need `System { capture }` | `$r = qx{...}` |
| **C** | `ls` emulation (55 lines) | **No** (generator logic) | Would need `System` or simpler `glob` | `@e = grep -e, @files; say for @e or say "No..."` |
| **D** | `rm` backtick → per-file unlink | **No** (generator logic) | Same as B | `$r = qx{rm ... && echo ...}` |
| **E** | `touch` backtick → if/else utime | **No** (generator logic) | Same as B | `$r = qx{touch ... && echo ...}` |
| **F** | Empty `if (mkdir) { }` body | **Yes** | `If { then: [] }` | Omit the `if` entirely, just `mkdir(...)` |
| **G** | `rm -f` → per-file carp blocks | **Partial** (empty-if fixable via IR) | `If { cond, then: [...] }` | Empty if-bodies removed; but logic stays |
| **H** | Repeated `$main_exit_code = 0` | **Yes** (MIR dead-assign elim) | `Assign` | Remove redundant assignments |
| **I** | `touch` inline → if/else utime/open | **No** (generator logic) | Would need `touch` recognized | `open $fh, '>>', $path and close $fh` |
| **J** | `rm -rf 2>/dev/null \|\| true` | **Partial** (stderr suppression pattern) | `System` + stderr-suppress flag | `eval { remove_tree($path) }` |

**Total IR-fixable:** 4 out of 10 patterns fully (A, F, H, plus the empty-body aspect of G). The other 6 require changes to generator logic because they involve:
- The decision to inline shell commands (`cp`, `mv`, `rm`, `touch`, `mkdir`) into native Perl calls
- The `&&`-inside-backtick concatenation pattern (`$left_result_N . $right_result_N`)
- The full `ls` emulation
- The `touch` emulation strategy

---

### 5. What the Generator Should Change

The single biggest improvement would be: **For backtick command substitution, emit `qx{...}` or a simple `system()` call instead of expanding each command into native Perl.** The script's cp/mv/rm/mkdir/touch inside backticks are being "native-ized" by the generator (converted to `File::Copy::copy`, `unlink`, `mkdir`, etc.) and then the `&&` pipe is manually reconstructed. If the generator instead recognized that these are simple commands whose output is being captured, it could emit:

```perl
my $cp_result = qx{cp test_file.txt test_file_copy.txt && echo "Copy successful"};
$CHILD_ERROR = $? >> 8;
```

This would reduce the ~140 lines for the 5 backtick commands to ~10 lines, and it's more faithful to the original (which runs the actual cp/mv/rm commands via the shell).

---

### 6. Unnecessarily Verbose Translations (Prime Candidates for IR Simplification)

| Rank | Pattern | Shell Original | Generated Lines | Preferred Output |
|---|---|---|---|---|
| **1** | `echo "..." > file` | 1 line | 16 lines | `{ open $fh, '>', $file; say $fh "..."; close $fh }` |
| **2** | `ls files 2>/dev/null \|\| echo msg` | 1 line × 4 = 4 lines | ~55 lines × 4 = ~220 lines | `grep { -e } @files` check |
| **3** | `cmd_result=\`cp ... && echo msg\`` | 1 line | ~30 lines | `$r = qx{...}` |
| **4** | `rm -f f1 f2` | 1 line | ~18 lines | `unlink qw(f1 f2)` |
| **5** | `touch result=\`touch f && echo msg\`` | 1 line | ~25 lines | `$r = qx{touch f && echo ...}` |
| **6** | `mkdir result=\`mkdir d && echo msg\`` | 1 line | ~22 lines | `$r = qx{mkdir d && echo ...}` |
| **7** | `rm -rf d 2>/dev/null \|\| true` | 1 line | ~20 lines | `eval { remove_tree('d') }` |
| **8** | `rm_result=\`rm ... && echo msg\`` | 1 line | ~30 lines | `$r = qx{rm ... && echo ...}` |
| **9** | `touch test_dir/file` (inline) | 1 line | ~12 lines | `open $fh, '>>', $path and close $fh` |
| **10** | `cp_result=`cp ... && echo msg`` | 1 line | ~35 lines | `$r = qx{cp ... && echo ...}` |

The top 2 alone account for ~80% of the bloat (236 of ~290 lines). They are a single echo with a redirect and four `ls 2>/dev/null || echo` calls. The `ls` emulation is by far the biggest contributor — removing it would cut the generated code from ~290 lines to ~70 lines.

---

### 7. Conclusion

The generated Perl reads like a **structure-preserving transliteration** rather than native Perl because:

1. **Every shell command is replaced by a native Perl equivalent** (cp→File::Copy::copy, ls→opendir/readdir, rm→unlink, touch→utime/open) even when the original script was just running external commands. This is the translator's design philosophy ("native Perl"), but it results in code that is 10–20× longer than the original.

2. **The `&&` chain is manually decomposed** into `$left_result_N . $right_result_N` concatenation inside nested `do` blocks, instead of using Perl's `&&` operators or `qx{}` for command capture.

3. **Every statement is made "safe" with eval, $CHILD_ERROR, and error checking** even when the original script explicitly suppressed errors (`2>/dev/null`, `|| true`).

4. **The `ls` emulation is a full reimplementation** of a command that the script uses purely as an existence check.

**What the IR can fix:** The IR backend (once all generator functions emit semantic nodes instead of raw text) can:
- Remove empty if-bodies (Pattern F/G)
- Eliminate redundant assignments (Pattern H)
- Simplify stderr-suppression wrapping (Pattern J)
- Clean up `Output` with redirects into direct file writes (Pattern A)
- Apply the style rules from `docs/ir-design.md` (say vs print, parenthesis style, etc.)

**What requires generator changes:** The IR backend cannot fix the fundamental decision to inline shell commands as native Perl, the `&&`-decomposition pattern, or the full `ls` emulation. Those are generator logic choices that produce `RawText` nodes which the IR backend passes through verbatim. The migration strategy in the IR design doc acknowledges this — you must first rewrite each generator function to emit IR nodes, then the backend can apply consistent style.