Now I have a thorough understanding. Let me write the review.

---

## Idiom Review: `sh/000__04c_text_processing_commands.sh` → Generated Perl

### 1. Original Shell Script and Generated Perl

**Original shell** (highlights):
```bash
file_content=`cat 000__04c_text_processing_commands.sh | head -5`
grep_result=`grep -n "echo" 000__04c_text_processing_commands.sh`
sed_result=`echo "Hello World" | sed 's/World/Universe/'`
awk_result=`echo "1 2 3 4 5" | awk '{print $1 + $2}'`
sort_result=`echo -e "zebra\napple\nbanana" | sort`
uniq_result=`echo -e "apple\napple\nbanana\nbanana\ncherry" | uniq`
word_count=`echo "Hello World" | wc -w`
line_count=`echo -e "line1\nline2\nline3" | wc -l`
head_result=`seq 1 10 | head -3`
tail_result=`seq 1 10 | tail -3`
cut_result=`echo "apple:banana:cherry" | cut -d: -f2`
tr_result=`echo "HELLO WORLD" | tr 'A-Z' 'a-z'`
xargs_result=`echo "1 2 3" | xargs -n1 echo "Number:"`
```

**Generated Perl** (representative excerpts — full output is ~480 lines for 44 lines of shell):

Every backtick substitution becomes a 3-level `do { ... }` block with pipeline boilerplate. Here's the typical structure:
```perl
my $VARIABLE = do { local $CHILD_ERROR = 0; do {
    do { do {
    my $output_N = q{};
    my $output_printed_N;
    my $pipeline_success_N = 1;
    $output_N .= 'constant string' . "\n";
    if ( !($output_N =~ m{\n\z}) ) { $output_N .= "\n"; }
    if ($CHILD_ERROR != 0) { $pipeline_success_N = 0; }
    # ... command-specific logic (split, loop, join) ...
    if ( !$pipeline_success_N ) { $main_exit_code = 1; }
    $output_N =~ s/\n+\z//msx;
    $output_N;
}; };
}; };
```

---

### 2. Non-Idiomatic Patterns

#### Pattern 1 — Triple pipeline `do` nesting for every backtick

**Generated:**
```perl
my $sed_result = do { local $CHILD_ERROR = 0; do {
    do { do {
    my $output_2 = q{};
    my $output_printed_2;
    my $pipeline_success_2 = 1;
    # ...echo string, sed substitution...
    if ( !$pipeline_success_2 ) { $main_exit_code = 1; }
    $output_2 =~ s/\n+\z//msx;
    $output_2;
}; };
}; };
```

The `local $CHILD_ERROR = 0; do { do { do { ... } }; }; };` is three layers of wrapping: the outer `do { local $CHILD_ERROR ... }` handles pipeline error scoping, the middle `do { do { ... } }` appears to be a remnant of multi-pipeline stage handling, and the inner block holds the actual logic.

**Preferred idiomatic Perl:**
```perl
my $sed_result = do { my $x = 'Hello World'; $x =~ s/World/Universe/r };
# or simply:
(my $sed_result = 'Hello World') =~ s/World/Universe/r;
```

**IR-fixable?** ✅ **Yes.** The IR design doc's `Pipeline` node already addresses this: a single-stage pipeline should emit just the stage body, not the scaffolding. The `IrStmt::System { capture: Some("out") }` node would produce `my $out = qx{...}`. For inlined native commands, the generator would emit `IrStmt::Declare` + `IrStmt::Assign` directly.

**IR nodes involved:** `IrStmt::Pipeline { stages: [single stage], last_output: Some("sed_result") }` — the backend knows it's a single stage and skips the `output_printed`, `pipeline_success`, and `local CHILD_ERROR` boilerplate.

**Cleaned output:**
```perl
(my $sed_result = 'Hello World') =~ s/World/Universe/r;
```

---

#### Pattern 2 — Newline bookkeeping round-trip

**Generated (every pipeline block):**
```perl
$output_4 .= "zebra\napple\nbanana";
if ( !($output_4 =~ m{\n\z}) ) { $output_4 .= "\n"; }
# ... process ...
$output_4 =~ s/\n+\z//msx;
```

This appends a trailing newline, then later strips all trailing newlines. For a constant string that already ends with `\n`, the `if` check is a no-op. The final `s/\n+\z//msx` undoes any newline appending.

**Preferred idiomatic Perl:**
```perl
my $sort_result = join "\n", sort qw(zebra apple banana);
```

**IR-fixable?** ✅ **Yes.** The backend controls newline normalization. If the IR node has a constant string or known-safe expression, the backend can skip the entire round-trip. This is a pure formatting decision in `ir_to_perl()`.

**IR nodes involved:** `IrStmt::Pipeline` — the backend's `emit_pipeline()` for a single known-safe stage can omit the `if (!$output =~ m{\n\z})` and the final `s/\n+\z//`.

**Cleaned output:** The `if (!($x =~ m{\n\z}))` and `$x =~ s/\n+\z//` disappear entirely for simple cases.

---

#### Pattern 3 — Manual `head` via index/substr while-loop

**Generated (for `cat file | head -5`):**
```perl
my $num_lines       = 5;
my $head_line_count = 0;
my $result          = q{};
my $input           = $output_0;
my $pos             = 0;

while ( $pos < length $input && $head_line_count < $num_lines ) {
    my $line_end = index $input, "\n", $pos;
    if ( $line_end == -1 ) { $line_end = length $input; }
    my $head_line = substr $input, $pos, $line_end - $pos;
    $result .= $head_line . "\n";
    $pos = $line_end + 1;
    ++$head_line_count;
}
```

**Preferred idiomatic Perl:**
```perl
my $result = join '', (split /\n/, $output_0)[0..4];
# Or more efficiently with a filehandle:
open my $fh, '<', 'file.sh';
my $result = join '', map { "$_\n" } (<$fh>)[0..4];
close $fh;
```

**IR-fixable?** ❌ **No.** This is an algorithmic choice by the generator. The IR sees `IrStmt::While` with `IrStmt::Assign` and `IrExpr::BinOp` nodes inside — the backend can only pretty-print that control flow. It cannot infer that this is a `head` operation and replace it with array slicing. The `generate_head_command()` function in `src/generator/commands/head.rs` explicitly emits this low-level pattern.

**What needs to change:** The generator should detect `head N` and emit `IrStmt::Assign { targets: [Scalar("result")], expr: IrExpr::Call("join", [...]) }` with an array-slice expression.

---

#### Pattern 4 — `grep -n` with quadratic membership test

**Generated (for `grep -n "echo" file`):**
```perl
my @grep_filtered_1 = grep { /echo/msx } @grep_lines_1;
my @grep_numbered_1;
for my $i (0..@grep_lines_1-1) {
    if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
        push @grep_numbered_1, sprintf "%d:%s", $i + 1, $grep_lines_1[$i];
    }
}
```

This is O(n*m) — for each line, it does a full `grep` across the filtered list to check membership. Also, `@grep_filenames_1` is populated but never used.

**Preferred idiomatic Perl:**
```perl
open my $fh, '<', '000__04c_text_processing_commands.sh' or croak "Cannot open: $ERRNO";
my @grep_numbered_1;
while (my $line = <$fh>) {
    chomp $line;
    if ($line =~ /echo/) {
        push @grep_numbered_1, sprintf "%d:%s", $., $line;
    }
}
close $fh;
```

**IR-fixable?** ❌ **No.** The IR sees a `For` loop with a nested `Call("grep")` — the backend cannot transform this into a `while` loop with `$.`. The generator's `generate_grep_command()` function (at `src/generator/commands/grep.rs:943-958`) explicitly emits this two-pass pattern: first filter, then number via nested grep.

**What needs to change:** The generator should use `$.` inside a `while (<$fh>)` loop for `grep -n`, or at minimum convert the membership check from `scalar grep { $_ eq $val } @filtered` to a hash lookup.

---

#### Pattern 5 — `sed` as split-loop-join

**Generated (for `echo "Hello World" | sed 's/World/Universe/'`):**
```perl
my @sed_lines_2 = split /\n/, $output_2;
my @sed_result_2;
foreach my $line (@sed_lines_2) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_2, $line;
}
$output_2 = join "\n", @sed_result_2;
```

**Preferred idiomatic Perl:**
```perl
$output_2 =~ s/World/Universe/grmx;
# Or for a single string:
(my $output_2 = 'Hello World') =~ s/World/Universe/r;
```

**IR-fixable?** ❌ **No.** The generator chose to implement `sed` via line-at-a-time split/loop/join. The IR would see `IrStmt::DeclareArray`, `IrStmt::For`, `IrStmt::Assign`. There's no IR node that says "this is a substitution operation." The backend cannot fuse the loop into a single `s///`.

**What needs to change:** The generator's `generate_sed_command()` (at `src/generator/commands/sed.rs`) should detect a simple `s///` with no line-range addresses and emit a single `=~ s///r` instead of the split/loop/join.

---

#### Pattern 6 — `cut` as split-loop-join

**Generated (for `echo "apple:banana:cherry" | cut -d: -f2`):**
```perl
my @lines_13 = split /\n/, $output_12;
my @result_13;
foreach my $line (@lines_13) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 1) {
        push @result_13, $fields[1];
    }
}
$output_12 = join "\n", @result_13;
```

**Preferred idiomatic Perl:**
```perl
# For a single line:
my $cut_result = (split /:/, 'apple:banana:cherry')[1];
# For multiple lines:
$output_12 = join "\n", map { (split /:/)[1] } split /\n/, $output_12;
```

**IR-fixable?** ⚠️ **Partially.** The verbosity of `push` vs `map` could be fixed by the IR backend if the generator emitted `IrExpr::Call("map", ...)`. But the generator currently emits `IrStmt::For` with `IrStmt::Assign` and `IrStmt::Push`. The backend cannot turn a `For` loop into `map` — it can only format the `For` as written.

**What needs to change:** The generator should emit `map`-based IR (a `Call` node) for simple field extraction, or the IR could add a `Map` combinator node that the backend formats as `map { ... } @list`.

---

#### Pattern 7 — `echo "..." > file` via STDOUT save/restore

**Generated (for `echo -e "1\n2\n3" > temp1.txt`):**
```perl
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    say "1\n2\n3";
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
```

This is 14 lines of Perl. The `my $tmp = do { say "1\n2\n3"; }; print $tmp;` line is especially odd — it captures the output of `say` (which returns success/failure, not the string) into `$tmp`, then prints `$tmp`. The `say` already printed to the redirected STDOUT, so `print $tmp` prints the return value (usually `1`).

**Preferred idiomatic Perl:**
```perl
open my $fh, '>', 'temp1.txt' or die "Cannot open temp1.txt: $OS_ERROR\n";
print $fh "1\n2\n3";
close $fh;
```

**IR-fixable?** ❌ **No.** The generator has a redirects module (`src/generator/redirects.rs`) that implements output redirect by manipulating STDOUT. The IR has no concept of "redirect this output to a file" — it only sees `do { open ...; print ...; open ...; }` as opaque `RawText` or `System` nodes. Changing this requires modifying the generator to emit `IrStmt::FileWrite { file, content }` or to use `system("echo", "text", ">", "file")`.

---

#### Pattern 8 — `rm -f` with directory check and error messages

**Generated (for `rm -f temp1.txt`):**
```perl
if ( -e "temp1.txt" ) {
    if ( -d "temp1.txt" ) {
        carp "rm: carping: ", "temp1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp1.txt" ) { }
        else {
            carp "rm: carping: could not remove ", "temp1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
```

**Preferred idiomatic Perl:**
```perl
unlink 'temp1.txt';
# or with error checking:
unlink 'temp1.txt' or carp "cannot remove temp1.txt: $OS_ERROR\n";
```

**IR-fixable?** ❌ **No.** The generator inlines the full `rm` logic from `src/generator/commands/rm.rs` with directory checks, existence checks, and custom error messages. The IR would see `IrStmt::If`, `IrStmt::System`, etc. — the backend cannot simplify this to a bare `unlink`.

---

#### Pattern 9 — `xargs -n1` with dead inner loop

**Generated (for `echo "1 2 3" | xargs -n1 echo "Number:"`):**
```perl
my @xargs_input_16_1 = grep { $_ ne q{} } split /\s+/, $output_16;
my @xargs_output_16_1;
for my $i (0..scalar @xargs_input_16_1-1) {
    my @xargs_args_16_1;
    for my $j (0..1-1) {    # ← always 0..0, always 1 iteration
        push @xargs_args_16_1, $xargs_input_16_1[$i + $j];
    }
    my $xargs_line_16_1 = q{};
    $xargs_line_16_1 .= "Number:";
    foreach my $arg (@xargs_args_16_1) {
        $xargs_line_16_1 .= q{ } . $arg;
    }
    push @xargs_output_16_1, $xargs_line_16_1;
}
```

The inner `for $j (0..1-1)` is a loop that always runs once — it's a generic chunking mechanism that isn't optimized for `-n1`.

**Preferred idiomatic Perl:**
```perl
my $xargs_result = join "\n", map { "Number: $_" } split ' ', '1 2 3';
```

**IR-fixable?** ❌ **No.** The generator has a generic `xargs` handler. The IR would see nested `For` loops. The backend cannot eliminate the dead inner loop or replace the whole thing with `map`.

---

#### Pattern 10 — `tr` translation via character-by-character loop

**Generated (for `echo "HELLO WORLD" | tr 'A-Z' 'a-z'`):**
```perl
# Set up variables (A-Z → expanded to ABCDEFGHIJKLMNOPQRSTUVWXYZ, same for a-z)
my $set1_N = 'A-Z';
my $set2_N = 'a-z';
# ... then 50+ lines of range expansion, character-by-character loop ...
for my $char ( split //msx, $input_N ) {
    my $pos_N = index $expanded_set1_N, $char;
    if ( $pos_N >= 0 && $pos_N < length $expanded_set2_N ) {
        $output .= substr $expanded_set2_N, $pos_N, 1;
    } else {
        $output .= $char;
    }
}
```

**Preferred idiomatic Perl:**
```perl
$output =~ tr/A-Z/a-z/;
```

**IR-fixable?** ❌ **No.** The generator expands the tr ranges and does character-by-character index/append. The IR would see `For` + `If` + `Assign` — the backend cannot fuse this into a `tr///`.

**What needs to change:** The `generate_tr_command()` in `src/generator/commands/tr.rs` should detect simple `tr` cases and emit `IrStmt::Assign { expr: IrExpr::Tr { ... } }` or just emit the Perl `tr///` directly.

---

#### Pattern 11 — `wc` verbose result assembly

**Generated (for `wc -w`):**
```perl
my $_wc_data = $output_6;
my $_wc_words = scalar split /\s+/msx, $_wc_data;
my $_wc_result = q{};
$_wc_result .= sprintf q{%d}, $_wc_words;
$_wc_result .= "\n";
$_wc_result;
```

**Preferred idiomatic Perl:**
```perl
scalar split ' ', $output_6;
# or with sprintf:
sprintf "%d\n", scalar split ' ', $output_6;
```

**IR-fixable?** ✅ **Yes.** The three-line `$_wc_result` assembly (declare, append, append, return) is a backend formatting choice. If the generator emitted `IrExpr::Call("sprintf", ["%d\n", IrExpr::Call("scalar split", ...)])`, the backend would emit `sprintf "%d\n", scalar split ' ', $data` instead of the piecemeal concatenation.

**IR nodes involved:** `IrStmt::Assign { expr: IrExpr::Call("sprintf", [...]) }`

**Cleaned output:**
```perl
sprintf "%d\n", scalar split ' ', $output_6;
```

---

#### Pattern 12 — `diff` via forked process

**Generated (for `diff file1.txt file2.txt`):**
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

This is the only case in this script that correctly shells out to an external command. It's verbose but not bad.

**Preferred idiomatic Perl:**
```perl
my $diff_output = qx{diff file1.txt file2.txt};
$CHILD_ERROR = $? >> 8;
```

**IR-fixable?** ✅ **Yes.** The IR has `IrStmt::System { cmd: "diff", args: [...], capture: Some("diff_result") }`. The backend can format this as `my $diff_result = qx{diff file1.txt file2.txt};`.

**IR nodes involved:** `IrStmt::System { capture: Some("diff_result") }`

**Cleaned output:**
```perl
my $diff_result = qx{diff file1.txt file2.txt};
$CHILD_ERROR = $? >> 8;
```

---

#### Pattern 13 — `paste` and `comm` with manual file I/O

**Generated (for `paste temp1.txt temp2.txt`):**
```perl
my @paste_file1_lines_fh_1;
my @paste_file2_lines_fh_1;
if (open my $fh1, '<', 'temp1.txt') {
    while (my $line = <$fh1>) {
        chomp $line;
        push @paste_file1_lines_fh_1, $line;
    }
    close $fh1 or croak "Close failed: $OS_ERROR";
}
if (open my $fh2, '<', 'temp2.txt') {
    ... same for file2 ...
}
my $max_lines = scalar @paste_file1_lines_fh_1 > scalar @paste_file2_lines_fh_1
    ? scalar @paste_file1_lines_fh_1 : scalar @paste_file2_lines_fh_1;
my $paste_output = q{};
for my $i (0..$max_lines-1) {
    my $line1 = $i < scalar @paste_file1_lines_fh_1 ? $paste_file1_lines_fh_1[$i] : q{};
    my $line2 = $i < scalar @paste_file2_lines_fh_1 ? $paste_file2_lines_fh_1[$i] : q{};
    $paste_output .= "$line1\t$line2\n";
}
```

**Preferred idiomatic Perl:**
```perl
use List::Util qw(max);
open my $fh1, '<', 'temp1.txt';
open my $fh2, '<', 'temp2.txt';
my @l1 = map { chomp; $_ } <$fh1>;
my @l2 = map { chomp; $_ } <$fh2>;
my $paste = join "\n", map { "$l1[$_]\t$l2[$_]" } 0..max($#l1, $#l2);
```

**IR-fixable?** ⚠️ **Partially.** The verbosity of the manual `while (my $line = <$fh>) { chomp; push }` for reading files could be simplified if the generator emitted `IrStmt::SlurpFile` or used `map { chomp; $_ } <$fh>`. The IR backend can't shorten the loop, but if the generator emitted a higher-level `ReadFileLines` node, the backend could format it as `map { chomp; $_ } <$fh>`.

---

#### Pattern 14 — Brace explosion at file end

The generated file ends with 20+ lines of closing braces `}`:
```
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
```

This indicates unbalanced brace counting in the generator — each nested `do` block, `for` loop, `if` statement, and pipeline stage adds closing braces, and many of them are never matched by corresponding opening braces in the output.

**IR-fixable?** ✅ **Yes.** An IR backend with proper program structure would never produce unbalanced braces. The issue is that the current generator emits raw text with ad-hoc brace management. With an IR, the `ir_to_perl()` function manages indentation and brace emission systematically.

**IR nodes involved:** All `IrStmt` variants — the backend's `emit_stmt()` handles indentation and block delimiters correctly for each node type.

---

### 3. Summary: IR-fixable vs. Generator-fixable

| # | Pattern | IR-fixable? | IR Node | What the backend would change |
|---|---|---|---|---|
| 1 | Triple `do` nesting | ✅ Yes | `Pipeline { stages: [single] }` | Emit just the stage body |
| 2 | Newline round-trip | ✅ Yes | `Pipeline` / `Output` | Omit trailing-newline fiddling for known-safe values |
| 3 | Manual `head` via `index`/`substr` | ❌ No | N/A — algorithmic choice | Requires generator to emit array slice |
| 4 | `grep -n` quadratic membership | ❌ No | N/A — algorithmic choice | Requires generator to use `$.` or hash lookup |
| 5 | `sed` split-loop-join | ❌ No | N/A — algorithmic choice | Requires generator to detect simple `s///` |
| 6 | `cut` split-loop-join | ⚠️ Partial | `Call("map", ...)` if generator emits it; else N/A | Generator would need to emit `map` IR |
| 7 | STDOUT save/restore for redirect | ❌ No | N/A — structural choice | Requires generator to emit file I/O directly |
| 8 | `rm` with directory check | ❌ No | N/A — structural choice | Requires generator to emit bare `unlink` |
| 9 | `xargs -n1` dead inner loop | ❌ No | N/A — algorithmic choice | Requires generator to special-case `-n1` |
| 10 | `tr` character-by-character | ❌ No | N/A — algorithmic choice | Requires generator to emit `tr///` |
| 11 | `wc` verbose result assembly | ✅ Yes | `Call("sprintf", ...)` | Emit `sprintf "%d\n", scalar split ' ', $data` |
| 12 | `diff` forked process | ✅ Yes | `System { capture }` | `my $out = qx{diff ...};` |
| 13 | `paste` manual file I/O | ⚠️ Partial | `SlurpFile` or `ReadLines` if added | Backend formats as `map { chomp; $_ } <$fh>` |
| 14 | Brace explosion | ✅ Yes | All `IrStmt` nodes | Backend manages braces systematically |

**IR-fixable patterns: 5 out of 14 (1, 2, 11, 12, 14)**
**Partially fixable: 2 (6, 13)**
**Not fixable: 7 (3, 4, 5, 7, 8, 9, 10)**

---

### 4. Unnecessarily Verbose Translations — Prime IR Simplification Candidates

These are places where a simple shell operation is wrapped in a wildly disproportionate amount of Perl infrastructure. Each is a candidate where the IR backend could produce drastically cleaner output **if the generator emits high-level IR nodes** instead of raw control flow.

| Shell line | Generated lines | Core logic | Could be | Bloat |
|---|---|---|---|---|
| `echo "Hello World" \| sed 's/World/Universe/'` | ~25 lines | 1 `s///` | `(my $x = 'Hello World') =~ s/World/Universe/r` | ~20× |
| `echo "1 2 3 4 5" \| awk '{print $1 + $2}'` | ~25 lines | `($F[0]+$F[1])` | `my @F = split ' ', '1 2 3 4 5'; $F[0] + $F[1]` | ~20× |
| `seq 1 10 \| head -3` | ~30 lines | `(1..10)[0..2]` | `join "\n", (1..10)[0..2]` | ~25× |
| `seq 1 10 \| tail -3` | ~30 lines | `(1..10)[-3..-1]` | `join "\n", (1..10)[-3..-1]` | ~25× |
| `echo -e "zebra\n..." \| sort` | ~25 lines | `sort @lines` | `join "\n", sort qw(zebra apple banana)` | ~20× |
| `echo "..." \| cut -d: -f2` | ~20 lines | `(split /:/)[1]` | `(split /:/, 'apple:banana:cherry')[1]` | ~15× |
| `echo "1 2 3" \| xargs -n1 echo "Number:"` | ~30 lines | `map { "Number: $_" } split...` | `join "\n", map { "Number: $_" } split ' ', '1 2 3'` | ~25× |
| `echo "Hello World" \| wc -w` | ~15 lines | `scalar split ' '` | `scalar split ' ', 'Hello World'` | ~12× |
| `echo -e "1\n2\n3" > temp1.txt` | ~14 lines | `print $fh "1\n2\n3"` | `open fh, '>', 't'; print fh "1\n2\n3"` | ~10× |
| `echo -e "apple\n..." \| uniq` | ~10 lines | unique-adjacent filter | `my @u; my $p; for (split /\n/) { push @u, $_ if $_ ne $p; $p = $_ }` | ~5× |
| `rm -f temp1.txt` | ~12 lines | `unlink` | `unlink 'temp1.txt'` | ~10× |
| `echo "HELLO WORLD" \| tr 'A-Z' 'a-z'` | ~60 lines | `tr/A-Z/a-z/` | `$x =~ tr/A-Z/a-z/` | ~40× |

**The top 5 most egregious examples for IR-driven simplification:**

1. **`tr 'A-Z' 'a-z'`** — 60 lines of Perl for `y/A-Z/a-z/`. The 50+ lines of range expansion (`if ($set =~ /a-z/) { $set =~ s/a-z/abcdefghijklmnopqrstuvwxyz/ }`) are completely unnecessary. A simple `tr///` or `s///r` with character classes suffices.

2. **`head -3`** — 30 lines for array slicing. The `index`/`substr` while-loop is a C-style approach; Perl's array slice notation is idiomatic.

3. **`xargs -n1 echo "Number:"`** — 30 lines with a dead inner loop for `map` with concatenation.

4. **`sed 's/World/Universe/'`** — 25 lines split-loop-join for a single `s///r`.

5. **`sort`** — 25 lines of pipeline scaffold + split/sort/join + trailing-newline check. Could be `join "\n", sort @lines`.

---

### 5. Diagnosing the Root Cause

The generator has two layers of verbosity:

**Layer 1: The pipeline scaffold** (`do { local $CHILD_ERROR = 0; do { do { my $output_N = q{}; my $output_printed_N; my $pipeline_success_N = 1; ... } } }`) — this is **IR-fixable**. The IR backend sees a single-stage pipeline and elides the scaffolding, producing just the stage body.

**Layer 2: The command implementation** (manual `head` via `index`/`substr`, `sed` as split-loop-join, `tr` as character-by-character index/append, etc.) — this is **NOT IR-fixable**. The generator chooses to implement each Unix utility in Perl using low-level primitives rather than mapping to Perl's built-in operations (`s///`, `tr///`, array slices, `map`, `grep`, `.` for line numbers, etc.).

For the IR to produce truly idiomatic Perl, the generator must emit **semantic IR nodes** — not just `While`/`For`/`Assign` for everything. Key missing high-level IR nodes:

| Shell utility | Current generator output | Needed IR node |
|---|---|---|
| `head N` | `While` + `index`/`substr` | `ArraySlice { list: IrExpr, range: Range }` or `Call("splice", ...)` |
| `tail N` | `For` + `@tail_lines` push/shift | `ArraySlice { list: IrExpr, range: Range(end-N..-1) }` |
| `sed s///` | `For` + `s///` + push | `Substitute { pattern, replacement, options }` |
| `tr` | `For` + `index`/`substr` append | `Translate { set1, set2, options }` |
| `cut -f` | `For` + `split` + push | `Call("map", "split", ...)` |
| `xargs -nN` | Nested `For` loops | `Call("map", ...)` |
| `sort` | `sort` + `join` (decent) | `Call("sort", ...)` — already decent |
| `uniq` | (missing in output) | `UniqueAdjacent { list }` |
| `wc -w` | `scalar split` (decent) | `Call("scalar split", ...)` |
| `echo > file` | STDOUT redirect | `FileWrite { file, content }` or `System { redirect: Write }` |
| `grep -n` | `For` + `scalar grep` | `GrepLineNumber { pattern, file }` using `$.` |

The IR design doc's `RawText` bridge was intended for incremental migration. But for these patterns, the generator needs new high-level IR nodes, not just `RawText` → `While`/`For` conversion. The backend's `ir_to_perl()` can only format the nodes it receives — it cannot invent `s///` from a `For` loop over `split`/`s///`/`push`.