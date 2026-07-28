Now let me read the IR design doc more carefully and check some other relevant files:Let me examine the specific generators for the problematic commands:Now I have a thorough understanding of the generator mechanics. Let me compile the review.Now I have a complete understanding of the codebase. Here is the comprehensive idiom review:

---

# Idiom Review: `000__06_text_processing_commands.sh` → Generated Perl

## 1. Original Shell Script

```bash
#!/bin/bash
echo "=== Text Processing Commands ==="

# cat + head in backticks
file_content=`cat src/main.rs | head -5`

# grep -n in backticks
grep_result=`grep -n "fn" src/main.rs`

# sed in backticks
sed_result=`echo "Hello World" | sed 's/World/Universe/'`

# awk in backticks
awk_result=`echo "1 2 3 4 5" | awk '{print $1 + $2}'`

# sort in backticks
sort_result=`echo -e "zebra\napple\nbanana" | sort`

# uniq in backticks
uniq_result=`echo -e "apple\napple\nbanana\nbanana\ncherry" | uniq`

# wc in backticks
word_count=`echo "Hello World" | wc -w`
line_count=`echo -e "line1\nline2\nline3" | wc -l`

# head / tail / cut in backticks (seq 1 10 | head -3, etc.)
head_result=`seq 1 10 | head -3`
tail_result=`seq 1 10 | tail -3`
cut_result=`echo "apple:banana:cherry" | cut -d: -f2`

# echo + paste + sed, redirects for temp files
echo -e "1\n2\n3" > temp1.txt
echo -e "a\nb\nc" > temp2.txt
paste_result=`paste temp1.txt temp2.txt | sed 's/\t/ /g'`

# comm / diff with temp files
echo -e "apple\nbanana\ncherry" > file1.txt
echo -e "banana\ncherry\ndate" > file2.txt
comm_result=`comm -12 file1.txt file2.txt`
diff_result=`diff file1.txt file2.txt`

# tr / xargs in backticks
tr_result=`echo "HELLO WORLD" | tr 'A-Z' 'a-z'`
xargs_result=`echo "1 2 3" | xargs -n1 echo "Number:"`

rm -f file1.txt file2.txt
```

## 2. Generated Perl Code

```perl
use strict; use warnings; use feature 'say'; use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $output = q{}; our $CHILD_ERROR;

say "=== Text Processing Commands ===";

# Pattern A: do { chomp(qx{...}); $result_N; }  (x13 occurrences)
my $file_content = do { chomp(my $result_0 = qx{cat src/main.rs | head -5}); $result_0; };
say $file_content;

# Pattern C: Full grep reimplementation (~25 lines)
my $grep_result = do { my $grep_result_1;
    my @grep_lines_1 = (); my @grep_filenames_1 = ();
    if (-e "src/main.rs") { open my $fh, '<', "src/main.rs" or croak "...";
        while (my $line = <$fh>) { chomp $line; push @grep_lines_1, $line;
            push @grep_filenames_1, "src/main.rs"; } close $fh or croak "..."; }
    else { print {*STDERR} "grep: src/main.rs: No such file or directory\n"; }
    my @grep_filtered_1 = grep { /fn/msx } @grep_lines_1;
    my @grep_numbered_1;
    for my $i (0 .. @grep_lines_1 - 1) {
        if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
            push @grep_numbered_1, sprintf "%d:%s", $i+1, $grep_lines_1[$i]; } }
    $grep_result_1 = join "\n", @grep_numbered_1;
    $CHILD_ERROR = scalar @grep_filtered_1 > 0 ? 0 : 1;
    $grep_result_1; };

# Pattern A again (sed, awk, sort, uniq, wc, head, tail, cut, tr, xargs)
my $sed_result = do { chomp(my $result_2 = qx{echo 'Hello World' | sed s/World/Universe/}); $result_2; };
my $awk_result = do { chomp(my $result_3 = qx{echo '1 2 3 4 5' | awk '{print $1 + $2}'}); $result_3; };
my $sort_result = do { chomp(my $result_4 = qx{echo -e "zebra\\napple\\nbanana" | sort}); $result_4; };
my $uniq_result = do { chomp(my $result_5 = qx{echo -e "apple\\n...\\ncherry" | uniq}); $result_5; };
my $word_count = do { chomp(my $result_6 = qx{echo 'Hello World' | wc -w}); $result_6; };
my $line_count = do { chomp(my $result_7 = qx{echo -e "line1\\nline2\\nline3" | wc -l}); $result_7; };
my $head_result = do { chomp(my $result_8 = qx{seq 1 10 | head -3}); $result_8; };
my $tail_result = do { chomp(my $result_9 = qx{seq 1 10 | tail -3}); $result_9; };
my $cut_result = do { chomp(my $result_10 = qx{echo apple:banana:cherry | cut -d : -f 2}); $result_10; };

# Pattern B: STDOUT redirection for echo > file (x4, ~15 lines each)
do {
    open my $original_stdout, '>&', STDOUT or die "...";
    open STDOUT, '>', 'temp1.txt' or die "...";
    my $tmp = do { say "1\n2\n3"; };
    print $tmp;                     # ← BUG: prints "1" (say's return value), not the content
    open STDOUT, '>&', $original_stdout or die "...";
    close $original_stdout or die "...";
};
# ... same pattern for temp2.txt, file1.txt, file2.txt ...

# Pattern A (paste_result)
my $paste_result = do { chomp(my $result_11 = qx{paste temp1.txt temp2.txt | sed "s/\\t/ /g"}); $result_11; };

# Pattern D: Full comm reimplementation (~20 lines)
my $comm_result = do {
    my @file1_lines; my @file2_lines;
    if (open my $fh1, '<', 'file1.txt') { ... }
    if (open my $fh2, '<', 'file2.txt') { ... }
    my %file1_set = map { $_ => 1 } @file1_lines;
    my %file2_set = map { $_ => 1 } @file2_lines;
    my @common_lines;
    foreach my $line (@file1_lines) { if (exists $file2_set{$line}) { push @common_lines, $line; } }
    my $comm_output = q{};
    foreach my $line (@common_lines) { $comm_output .= $line . "\n"; }
    $comm_output =~ s/\n$//msx;
    $comm_output
};

# Pattern E: Pipe-open for diff instead of qx{}
my $diff_result = do { my $diff_output = q{};
    { my $diff_cmd = 'diff'; my @diff_args = ('file1.txt', 'file2.txt');
      my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
      if ($diff_pid) { local $INPUT_RECORD_SEPARATOR = undef; $diff_output = <$diff_fh>;
          close $diff_fh; $CHILD_ERROR = $? >> 8; }
      else { carp "Cannot execute diff command: $OS_ERROR";
          $diff_output = q{}; $CHILD_ERROR = 1; } }
    $diff_output; };

# Pattern A (tr_result, xargs_result)
my $tr_result = do { chomp(my $result_12 = qx{echo 'HELLO WORLD' | tr A-Z a-z}); $result_12; };
my $xargs_result = do { chomp(my $result_13 = qx{echo '1 2 3' | xargs -n 1 echo Number:}); $result_13; };

unlink('file1.txt'); unlink('file2.txt');
```

---

## 3. Non-Idiomatic Patterns

### Pattern A — The `do { chomp(qx{…}); $result }` Boilerplate

**Location:** Every backtick substitution except `grep`, `comm`, `diff` (13 occurrences).

**Generated:**
```perl
my $file_content = do { chomp(my $result_0 = qx{cat src/main.rs | head -5}); $result_0; };
```

**Idiomatic Perl:**
```perl
chomp(my $file_content = qx{cat src/main.rs | head -5});
```
or, if you prefer separate statements:
```perl
my $file_content = qx{cat src/main.rs | head -5};
chomp $file_content;
```

**Why it's non-idiomatic:**
- The `do { }` block is cargo‑culted around every `qx{}` call, even though it serves no purpose — the expression inside already evaluates to the captured value.
- The intermediate variable `$result_N` is declared, assigned, then immediately returned. It is dead code as soon as the assignment to the real variable completes.
- `my` inside `chomp()` is valid Perl but unusual — `chomp(my $x = expr)` sets `$x` to the chomped value and returns the number of characters removed. Readers expect `chomp($x)` or `chomp(my $x = expr)` without a wrapper block.

**IR-fixable?** **Yes.** The `IrStmt::System { cmd, args, capture: Some("file_content") }` node is explicitly listed in the IR design doc's style‑rules table (see `ir-design.md`):

| Pattern in IR | Current output (ugly) | Future output (clean) |
|---|---|---|
| `System { capture: Some("out") }` | `my $out = do { ... qx{...} ... };` | `my $out = qx{...};` |

**IR node involved:** `IrStmt::System { capture: Some(var_name) }`

**Cleaned-up output:**
```perl
chomp(my $file_content = qx{cat src/main.rs | head -5});
chomp(my $sed_result     = qx{echo 'Hello World' | sed s/World/Universe/});
chomp(my $awk_result     = qx{echo '1 2 3 4 5' | awk '{print $1 + $2}'});
# ... etc.
```

**Unnecessarily verbose?** **Yes** — 13× a `do { }` block that wraps a single `qx{}` call, each with a dead intermediate variable.

---

### Pattern B — STDOUT Redirection for Simple File Writes

**Location:** The four `echo "…" > file` statements.

**Generated:**
```perl
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
        say "1\n2\n3";
    };
    print $tmp;  # ← BUG: prints "1" (say's return value), not the file content
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
```

**Idiomatic Perl:**
```perl
open my $fh, '>', 'temp1.txt' or die "Cannot write temp1.txt: $!";
print $fh "1\n2\n3";
close $fh;
```

**Why it's non-idiomatic:**
- The generator redirects the global `STDOUT` filehandle instead of opening its own filehandle. Perl is perfectly capable of writing to a named filehandle (`print $fh ...`); there is no need to hijack `STDOUT`.
- The code saves, redirects, and restores `STDOUT` — 15 lines for what should be 3 lines.
- **Bug:** `say` returns `1` (true) on success, so `$tmp` is `1`. `print $tmp` then writes `"1"` to the file *after* the intended content. The file `temp1.txt` ends up containing:
  ```
  1
  2
  3
  1
  ```
  The correct content should just be `1\n2\n3\n`.

**IR-fixable?** **Partially.** If the generator emitted proper `IrStmt::Open { handle, file, mode }` + `IrStmt::Print { handle, value }` nodes instead of the STDOUT‑duping `RawText`, the backend's `ir_to_perl()` could trivially render them as:
```perl
open my $fh, '>', 'temp1.txt' or die "...";
say $fh "1\n2\n3";
close $fh;
```
However, the current generator *chooses* the STDOUT‑redirect approach at the logic level. This is baked into `generate_redirect_impl` and `command_dispatcher.rs`. The IR can only restyle what it's given — if it receives the STDOUT‑redirect as a `RawText` node, it cannot fix it. If the generator were converted to produce semantic IR nodes, the backend could emit clean code. **This requires changing the generator logic** to emit file‑handle‑based I/O instead of STDOUT redirection.

**Unnecessarily verbose?** **Extremely.** 15 lines for a single `echo > file` — the prime example of "wrapping simple operations in complex control structures."

---

### Pattern C — Full `grep -n` Reimplementation in Perl

**Location:** The `grep_result` assignment.

**Generated:**
```perl
my $grep_result = do { my $grep_result_1;
    my @grep_lines_1 = ();
    my @grep_filenames_1 = ();             # ← never read (dead code)
    if (-e "src/main.rs") {
        open my $fh, '<', "src/main.rs" or croak "...";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_1, $line;
            push @grep_filenames_1, "src/main.rs";  # ← dead write
        }
        close $fh or croak "...";
    } else {
        print {*STDERR} "grep: src/main.rs: No such file or directory\n";
    }
    my @grep_filtered_1 = grep { /fn/msx } @grep_lines_1;

    my @grep_numbered_1;
    for my $i (0 .. @grep_lines_1 - 1) {
        if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
            #                     ^— O(n²) membership test
            push @grep_numbered_1, sprintf "%d:%s", $i + 1, $grep_lines_1[$i];
        }
    }
    $grep_result_1 = join "\n", @grep_numbered_1;
    $CHILD_ERROR = scalar @grep_filtered_1 > 0 ? 0 : 1;
    $grep_result_1;
};
```

**Idiomatic Perl:**
```perl
my $grep_result;
open my $fh, '<', 'src/main.rs' or carp "grep: src/main.rs: No such file or directory";
while (<$fh>) { $grep_result .= "$.:$_" if /fn/ }
close $fh;
```
Or, if shelling out is acceptable:
```perl
chomp(my $grep_result = qx{grep -n "fn" src/main.rs});
```

**Why it's non-idiomatic:**
- **Massive over-engineering** — a single shell command becomes ~25 lines of Perl with file reading, line buffering, filtering, numbered‑line reconstruction, and exit‑status tracking.
- **Unused variable `@grep_filenames_1`** is populated but never read. Dead code.
- **O(n²) membership test:** `scalar grep { $_ eq ... } @grep_filtered_1` inside a `for` loop over all lines. For each line position `$i`, it scans the entire `@grep_filtered_1` array to see if that line matched. Since `@grep_filtered_1` is a subset of `@grep_lines_1`, this is O(n²) where a hash lookup would be O(1).
- **Unnecessary `$CHILD_ERROR` tracking** for a non‑zero exit code that is never checked.
- **Extra regex flags `/fn/msx`** — `m`, `s`, and `x` modifiers are irrelevant for matching a bare identifier like `fn`.

**IR-fixable?** **No** — this is a generator‑level decision. The shell‑AST lowering in `generate_grep_command()` chose to expand the `grep` command into native Perl file I/O and filtering rather than delegating to `qx{}`. By the time the code reaches the IR layer, it is a series of imperative statements (file open, loop, grep, join) that the IR backend can only pretty‑print, not collapse back into a simpler construct. A future IR optimization pass (what `ir-design.md` calls "MIR transforms") could theoretically recognize this pattern and fold it, but that is far beyond the scope of pretty‑printing.

**Unnecessarily verbose?** **Yes** — ~25 lines for what could be 3–5 lines of idiomatic Perl. The outer `do { }` block, the unused array, the `for` loop with pipeline infrastructure for numbering, and the exit‑status boilerplate all add complexity without benefit.

---

### Pattern D — Full `comm -12` Reimplementation

**Location:** The `comm_result` assignment.

**Generated:**
```perl
my $comm_result = do {
    my @file1_lines; my @file2_lines;
    if (open my $fh1, '<', 'file1.txt') {
        while (my $line = <$fh1>) { chomp $line; push @file1_lines, $line; }
        close $fh1 or croak "Close failed: $OS_ERROR";
    }
    if (open my $fh2, '<', 'file2.txt') {
        while (my $line = <$fh2>) { chomp $line; push @file2_lines, $line; }
        close $fh2 or croak "Close failed: $OS_ERROR";
    }
    my %file1_set = map { $_ => 1 } @file1_lines;
    my %file2_set = map { $_ => 1 } @file2_lines;
    my @common_lines;
    foreach my $line (@file1_lines) {
        if (exists $file2_set{$line}) { push @common_lines, $line; }
    }
    my $comm_output = q{};
    foreach my $line (@common_lines) { $comm_output .= $line . "\n"; }
    $comm_output =~ s/\n$//msx;
    $comm_output
};
```

**Idiomatic Perl:**
```perl
chomp(my $comm_result = qx{comm -12 file1.txt file2.txt});
```
Or, using a hash intersection idiom:
```perl
open my $f1, '<', 'file1.txt' or die; my %s1 = map { chomp; $_ => 1 } <$f1>;
open my $f2, '<', 'file2.txt' or die; my @c = grep { $s1{$_} } map { chomp; $_ } <$f2>;
my $comm_result = join "\n", @c;
```

**Why it's non-idiomatic:**
- The generator reimplements `comm -12` (show only common lines) as a hash‑based intersection. While correct, this is ~20 lines of procedural code for what `comm` does natively.
- The reimplementation ignores `comm`'s requirement that input files be sorted. If the files weren't already sorted (they are, in this example), the Perl code would produce different results than `comm`.
- The `do { }` wrapper and the three‑step process (read → hash → join) adds visual noise.

**IR-fixable?** **No** — same reason as Pattern C. The generator `generate_comm_command()` consciously emits a native Perl implementation. The IR receives the expanded logic and can only restyle its presentation, not undo the expansion.

**Unnecessarily verbose?** **Yes** — but arguably less so than grep since the Perl logic is actually clearer than shelling out for such a simple operation. The verbosity comes from the `do { }` wrapper and the manual `foreach` loop for building the output string (could be `join "\n", @common_lines`).

---

### Pattern E — Pipe‑Open for `diff` Instead of `qx{}`

**Location:** The `diff_result` assignment.

**Generated:**
```perl
my $diff_result = do { my $diff_output = q{};
    {
        my $diff_cmd = 'diff';
        my @diff_args = ('file1.txt', 'file2.txt');
        my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
        if ($diff_pid) {
            local $INPUT_RECORD_SEPARATOR = undef;
            $diff_output = <$diff_fh>;
            close $diff_fh;
            $CHILD_ERROR = $? >> 8;
        } else {
            carp "Cannot execute diff command: $OS_ERROR";
            $diff_output = q{};
            $CHILD_ERROR = 1;
        }
    }
    $diff_output;
};
```

**Idiomatic Perl:**
```perl
my $diff_result = qx{diff file1.txt file2.txt};
```

**Why it's non-idiomatic:**
- Every other backtick command in the script uses `qx{}`, but `diff` uses a low‑level `open(my $fh, '-|', ...)` pipe with manual `$?`‑to‑`$CHILD_ERROR` conversion, `$INPUT_RECORD_SEPARATOR` manipulation, and error handling. This is ~15 lines for what should be a one‑liner.
- The outer `do { my $x = q{}; { … } $x; }` wrapping layers a `do` block around a bare block — doubly unnecessary.
- The `carp` on failure prints the message but execution continues, which matches shell semantics but adds complexity.

**IR-fixable?** **Yes** — if the generator emitted the `diff` command as an `IrStmt::System { cmd: "diff", capture: Some("diff_result") }` node, the backend's pretty‑printer would render it as:
```perl
my $diff_result = qx{diff file1.txt file2.txt};
```
The current generator chooses to emit the pipe‑open pattern (see `generate_diff_command()`), but since `diff` is already shelling out, there is no reason to use `open3`‑style piping instead of `qx{}`. If the IR receives a `System` node instead of `RawText`, the clean output is automatic.

**Unnecessarily verbose?** **Yes** — 15 lines of pipe‑open boilerplate where `qx{}` suffices.

---

### Pattern F — Unnecessary `do { }` Wrapper Everywhere

**Location:** Every assignment (file_content, grep_result, sed_result, …, comm_result, diff_result, …).

**Generated pattern:**
```perl
my $x = do { … };
```

**Idiomatic:**
```perl
my $x = …;
```

**Why it's non-idiomatic:**
The `do { }` block is only needed when you need to combine multiple statements into a single expression (e.g., for a return value from within a `map` or a ternary branch). Here, every value‑producing block is wrapped in `do { }`, even when the block contains a single expression. This is cargo‑cult verbosity.

**IR-fixable?** **Yes.** In the IR, a statement `Declare { targets: ["x"], init: Some(expr) }` should render as `my $x = EXPR;` without an enclosing `do`. The pretty‑printer only wraps in `do { }` when there are multiple statements to execute before producing a value. Since the `IrStmt::System { capture }` node already names the result variable, `do { }` is never needed for simple captures.

**Cleaned-up output:**
```perl
chomp(my $file_content = qx{cat src/main.rs | head -5});
chomp(my $sed_result   = qx{echo 'Hello World' | sed s/World/Universe/});
# ... no do {} in sight
```

---

### Pattern G — `$ERRNO` / `$OS_ERROR` Instead of `$!`

**Location:** Every `croak`/`die` call.

**Generated:**
```perl
or croak "Cannot access file: $ERRNO";
or croak "Close failed: $OS_ERROR";
```

**Idiomatic Perl:**
```perl
or croak "Cannot access file: $!";
or croak "Close failed: $!";
```

**Why it's non-idiomatic:**
- `$!` is the standard Perl error variable for OS errors (errno). `$ERRNO` is not a built‑in Perl variable. `$OS_ERROR` is an alias from the `English` module, but the generated code does not `use English`.
- Using a non‑standard name is confusing to readers and may not actually contain the error value.

**IR-fixable?** **Yes** — in the `IrStmt::Die` or `IrExpr::Var` node, variable names are determined by the backend. The pretty‑printer can map any OS‑error reference to `$!` regardless of what the generator calls it. (Though ideally the generator would emit the correct name to begin with.)

---

### Pattern H — Unnecessary Regex Flags `/fn/msx`

**Location:** The `grep` filter.

**Generated:**
```perl
my @grep_filtered_1 = grep { /fn/msx } @grep_lines_1;
```

**Idiomatic:**
```perl
my @grep_filtered_1 = grep { /fn/ } @grep_lines_1;
```

**Why it's non-idiomatic:**
- `/m` (multi‑line) makes `^` and `$` match line boundaries. Not relevant for a bare pattern match.
- `/s` (single‑line) makes `.` match newlines. Not needed here.
- `/x` (extended) allows whitespace in the pattern. Not needed.
- These flags are noise — they were probably added as a blanket default.

**IR-fixable?** **Yes.** The `IrExpr::BinOp { op: Match, lhs: ..., rhs: IrExpr::Regex { pattern: "fn", flags: "msx" } }` node's pretty‑printer can omit default or irrelevant flags. The backend would emit `{ /fn/ }` instead of `{ /fn/msx }`.

---

### Pattern I — Unused `@grep_filenames_1`

**Location:** In the `grep_result` generation.

**Generated:**
```perl
my @grep_filenames_1 = ();
...
push @grep_filenames_1, "src/main.rs";   # ← written
# $grep_filenames_1 or @grep_filenames_1 is never read later
```

**Idiomatic:** Don't declare or populate a variable that is never used.

**IR-fixable?** **Yes, with an optimization pass.** A dead‑code elimination pass on the IR would remove `Declare { vars: ["grep_filenames_1"], sigil: Array }` when the variable is never referenced. However, this is an **IR transform** (optimization), not a pretty‑printing change. The pretty‑printer alone cannot know which variables are dead.

---

### Pattern J — O(n²) Membership Test in Grep Numbering

**Location:** In the numbering loop of `grep_result`.

**Generated:**
```perl
for my $i (0..@grep_lines_1-1) {
    if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
        push @grep_numbered_1, sprintf "%d:%s", $i + 1, $grep_lines_1[$i];
    }
}
```

**Idiomatic Perl:**
```perl
my %matched = map { $_ => 1 } @grep_filtered_1;
for my $i (0..$#grep_lines_1) {
    if ($matched{$grep_lines_1[$i]}) {
        push @grep_numbered_1, sprintf "%d:%s", $i+1, $grep_lines_1[$i];
    }
}
```
Or, more idiomatically, avoid the index loop entirely:
```perl
my $i = 0;
for my $line (@grep_lines_1) {
    $i++;
    push @grep_numbered_1, "$i:$line" if $matched{$line};
}
```

**Why it's non-idiomatic:**
- `scalar grep { $_ eq $x } @array` is a linear scan of `@array` for each element of `@grep_lines_1`. This is O(n·m) where both arrays could be the same size — making it O(n²). Perl programmers reach for a hash for membership tests.
- Using an index loop instead of iterating over elements directly is more common in C than in Perl.

**IR-fixable?** **No** — this is an algorithmic choice in the generator logic (`generate_grep_command()`). The IR receives the loop and the `scalar grep` call as opaque statements; the pretty‑printer cannot change the algorithm to use a hash. A sophisticated IR optimizer could detect this pattern, but that's far beyond pretty‑printing.

---

### Pattern K — `$CHILD_ERROR` Exit Status Infrastructure

**Location:** After grep and diff.

**Generated:**
```perl
$CHILD_ERROR = scalar @grep_filtered_1 > 0 ? 0 : 1;
# ...
$CHILD_ERROR = $? >> 8;
```

**Idiomatic:** Omit if the exit status is never checked. If it must be tracked, use a local variable or `$?` directly.

**Why it's non-idiomatic:**
- `$CHILD_ERROR` is a custom package variable (declared as `our $CHILD_ERROR` at the top). This is infrastructure from shell's `$?` that is rarely needed in translated Perl scripts.
- Setting `$CHILD_ERROR` after every command adds noise when it's never read.

**IR-fixable?** **Partially** — if the IR tracks exit‑status as an optional attribute on `System` nodes, the pretty‑printer could suppress the `$CHILD_ERROR = ...` line when the value is never used downstream (dead‑code elimination again).

---

### Pattern L — `do { my $x = q{}; { … } $x; }` Double Nesting

**Location:** The `diff_result` block.

**Generated:**
```perl
my $diff_result = do { my $diff_output = q{};
    { ... }        # bare block inside do block
    $diff_output;
};
```

**Idiomatic:**
```perl
my $diff_result = do {
    my $diff_output = q{};
    ...;
    $diff_output;
};
```
Or simply:
```perl
my $diff_result = qx{diff file1.txt file2.txt};
```

**Why it's non-idiomatic:**
- A `do { }` block containing a bare `{ }` block is redundant. Bare blocks (`{ ... }`) create a new scope but don't otherwise change semantics. Having both is confusing nesting.

**IR-fixable?** **Yes** — if the statements inside the bare block are `IrStmt` nodes, the backend's `emit_stmt` can flatten the scope. The pretty‑printer would emit all statements at the same indentation inside the `do { }` without the extra `{ }`.

---

## 4. Unnecessarily Verbose Translations — Summary

These are the top candidates for IR‑based simplification:

| Rank | Pattern | Lines of generated code | Could be | Saving |
|---|---|---|---|---|
| 1 | **STDOUT redirection** (echo > file) | ~15 per file × 4 = ~60 | `open my $fh, '>', 'file'; print $fh "..."; close $fh;` (3 lines) | ~48 lines |
| 2 | **`do { chomp(qx{…}); $result }`** wrapper | ~3 per occurrence × 13 = ~39 | `chomp(my $v = qx{…})` (1 line) | ~26 lines |
| 3 | **Full grep reimplementation** | ~25 lines | `while (<$fh>) { $r .= "$.:$_" if /fn/ }` (3 lines) or `qx{…}` (1 line) | ~22 lines |
| 4 | **Pipe‑open for diff** | ~15 lines | `qx{diff file1.txt file2.txt}` (1 line) | ~14 lines |
| 5 | **Full comm reimplementation** | ~20 lines | `qx{comm -12 file1.txt file2.txt}` (1 line) or 5 lines of native Perl | ~15 lines |
| 6 | **`do { }` wrapping** (every assignment) | 1 line each × 15 | Remove `do { … }` | ~15 lines of nesting removed |

---

## 5. IR‑Fixability Grid

| Pattern | IR-fixable by pretty-printing? | IR node involved | Requires generator change? |
|---|---|---|---|
| A. `do { chomp(qx{…}); $result` wrapper | **Yes** | `IrStmt::System { capture }` | No — just emit the node instead of `RawText` |
| B. STDOUT redirection for file writes | **No** (pretty‑printing alone) | Would need `IrStmt::Open` + `IrStmt::Print` | **Yes** — generator must emit file‑handle IR instead of STDOUT‑duping |
| C. Full grep reimplementation | **No** | N/A — pattern is expanded into many IR nodes | **Yes** — generator chose native‑Perl over `qx{}` |
| D. Full comm reimplementation | **No** | N/A — same as grep | **Yes** — generator chose native‑Perl over `qx{}` |
| E. Pipe‑open for diff | **Yes** | `IrStmt::System { cmd: "diff", capture }` | No — if generator emits `System` node, pretty‑printer uses `qx{}` |
| F. Unnecessary `do { }` wrapper | **Yes** | `IrStmt::Declare { init }` | No — style rule in `ir_to_perl()` |
| G. `$ERRNO` / `$OS_ERROR` | **Yes** | `IrExpr::Var("ERRNO")` → map to `$!` | No — variable name remapping in backend |
| H. Unnecessary regex flags `/fn/msx` | **Yes** | `IrExpr::Regex { flags }` | No — backend omits irrelevant flags |
| I. Unused `@grep_filenames_1` | **No** (optimization pass needed) | Dead‑code elimination on IR | No, if an IR optimization pass is added |
| J. O(n²) membership test | **No** | N/A — algorithm baked into generator | **Yes** — generator should use a hash |
| K. `$CHILD_ERROR` infrastructure | **Partially** | Dead‑code elimination on IR | No, if IR optimization pass tracks liveness |
| L. `do { … { … } … }` double nesting | **Yes** | `IrStmt::DoWhile` / bare blocks | No — flatten scopes in pretty‑printer |

---

## 6. Key Takeaway

The generated code reads like a **line‑by‑line transliteration** of the shell script rather than native Perl because:

1. **Every backtick is wrapped in a `do { }` with a dead intermediate variable** — `ir_to_perl()` can fix this by emitting `qx{}` directly from `IrStmt::System` nodes, removing both the `do` block and the throwaway `$result_N`.

2. **The generator reimplements shell commands (`grep`, `comm`) in Perl** rather than shelling out or using concise Perl idioms. This is a *generator‑logic* decision that the IR cannot undo. These reimplementations are ~20–25 lines each when 1–5 lines would suffice.

3. **Output redirection is done by hijacking `STDOUT`** instead of opening a filehandle. This produces ~15 lines of boilerplate per redirect (with a bug) and is the single biggest source of bloat. The generator should emit `open my $fh, '>', $file; print $fh ...; close $fh;` instead.

4. **The `diff` command uses a low‑level pipe‑open** when `qx{}` would work identically and is already used everywhere else.

5. **Dead code and inefficient algorithms** (`@grep_filenames_1`, O(n²) membership test, `$CHILD_ERROR`) add noise that an IR optimizer could eventually clean up.

The IR design doc's vision — having the backend make all style decisions — would immediately fix Patterns A, E, F, G, H, and L. Patterns B, C, D, and J require changes in the generator logic itself; the IR can only prettify what it receives.