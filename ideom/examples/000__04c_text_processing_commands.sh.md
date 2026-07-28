I now have a thorough picture. Let me compile the review.

---

## Idiom Review: `examples/000__04c_text_processing_commands.sh` → Perl

### 1. Original shell script and generated Perl code

**Original shell snippet** (each backtick substitution):
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

**Generated Perl** (representative snippets inline below in each section).

---

### 2. Non-idiomatic patterns in the generated Perl

#### Pattern A — Pipeline infrastructure boilerplate for single commands

Every backtick substitution is wrapped in a three-level `do` block with pipeline bookkeeping variables:

```perl
my $sed_result = do { local $CHILD_ERROR = 0; do {
    do { do {
    my $output_2 = q{};
    my $output_printed_2;
    my $pipeline_success_2 = 1;
    $output_2 .= 'Hello World' . "\n";
    if ( !($output_2 =~ m{\n\z}) ) { $output_2 .= "\n"; }
    if ($CHILD_ERROR != 0) { $pipeline_success_2 = 0; }
    # ... command logic ...
    if ( !$pipeline_success_2 ) { $main_exit_code = 1; }
    $output_2 =~ s/\n+\z//msx;
    $output_2;
}; };
}; };
```

**Preferred idiomatic Perl**:
```perl
my $sed_result = do { (my $tmp = 'Hello World') =~ s/World/Universe/r };
```

**IR-fixable?**  
**Yes.** If the generator emits an `IrStmt::System { capture: Some("sed_result"), ... }` node or a single-stage `IrStmt::Pipeline`, the backend (`ir_to_perl()`) can choose to emit a simple expression instead of the full pipeline scaffold. The IR design doc already shows this:  
| `System { capture: Some("out") }` | `my $out = do { ... qx{...} ... };` | `my $out = qx{...};` |

For Perl-native operations (sed, awk, etc.), the IR could also emit a raw expression. The verbosity of `$output_N`, `$pipeline_success_N`, `$output_printed_N` is a backend pretty-printing decision.

---

#### Pattern B — Contradictory newline bookkeeping

Almost every block does this dance:

```perl
$output_4 .= "zebra\napple\nbanana";
if ( !($output_4 =~ m{\n\z}) ) { $output_4 .= "\n"; }
# ... process ...
$output_4 =~ s/\n+\z//msx;
```

First it ensures there's a trailing newline, then later strips trailing newlines. For simple expressions this is noise.

**Preferred idiomatic Perl**:
```perl
# No newline fiddling at all for simple cases
my $result = join "\n", sort qw(zebra apple banana);
```

**IR-fixable?**  
**Yes.** The backend knows the shape of the value. If it's a constant string or a simple expression, it can skip the trailing-newline round-trip. This is purely a pretty-printing decision in `ir_to_perl()` when handling `IrStmt::System` or `IrStmt::Pipeline` with known-safe inputs.

---

#### Pattern C — Manual `head` implemented with index/substr while-loop

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

**Preferred idiomatic Perl**:
```perl
my $result = join '', (split /\n/, $output_0, -1)[0..4];
```

Or reading the file directly:
```perl
open my $fh, '<', '000__04c_text_processing_commands.sh';
my $result = join '', map { "$_\n" } ( <$fh> )[0..4];
close $fh;
```

**IR-fixable?**  
**No.** This is an algorithmic choice made by the generator. The generator decided to inline a `head` implementation using low-level `index`/`substr` rather than emitting a high-level Perl idiom. The IR would see an `IrStmt::While` with `IrStmt::Assign` and `IrExpr::BinOp` nodes inside — the backend can only pretty-print that control flow, not replace it with array slicing. The generator must be changed to detect `head` and emit a simpler IR (e.g., an array slice or a `For` over a bounded range).

---

#### Pattern D — `grep -n` with quadratic membership test

```perl
my @grep_filtered_1 = grep { /echo/msx } @grep_lines_1;
my @grep_numbered_1;
for my $i (0..@grep_lines_1-1) {
    if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
        push @grep_numbered_1, sprintf "%d:%s", $i + 1, $grep_lines_1[$i];
    }
}
```

This is O(n*m) — it does a full grep inside the loop to check membership — and unnecessarily wastes the `@grep_filenames_1` array.

**Preferred idiomatic Perl**:
```perl
my @grep_numbered_1;
open my $fh, '<', '000__04c_text_processing_commands.sh' or croak ...;
while (my $line = <$fh>) {
    chomp $line;
    if ($line =~ /echo/) {
        push @grep_numbered_1, sprintf "%d:%s", $., $line;
    }
}
close $fh;
```

Or with `grep` on the whole file:
```perl
open my $fh, '<', '000__04c_text_processing_commands.sh';
my @grep_numbered_1 = map { sprintf "%d:%s", $_, (grep /echo/, <$fh>)[$_-1] } 1..(grep /echo/, <$fh>);
# (still awkward) — better to just use while+$.
```

**IR-fixable?**  
**No.** The generator chose the wrong algorithm (pre-filter then post-hoc number via nested grep). The IR sees `For` + `If` + `Call("grep")` — it cannot transform this into a streaming `while` loop with `$.`. The generator needs to be rewritten to emit a simpler pattern for `grep -n`.

---

#### Pattern E — `sed` as split-loop-join

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

**Preferred idiomatic Perl**:
```perl
# Single-line substitution
$output_2 =~ s/World/Universe/grmx;
# or
my $result = do { (my $x = 'Hello World') =~ s/World/Universe/r };
```

**IR-fixable?**  
**No.** The generator chose to implement `sed` as a line-at-a-time substitution with split/join. For a simple `s///` pattern (no line-range address, no multi-line advanced features), it could have emitted a single `=~ s///r`. The IR would see `For` + `Assign` + `BinOp(Concat)` — the backend cannot fuse that into a single `s///`. Generator logic change needed.

---

#### Pattern F — `cut` as split-loop-join

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

**Preferred idiomatic Perl**:
```perl
my @result_13 = map { (split /:/)[1] } split /\n/, $output_12;
$output_12 = join "\n", @result_13;
```

Or for a single line:
```perl
my $cut_result = (split /:/, 'apple:banana:cherry')[1];
```

**IR-fixable?**  
**Partially.** The verbosity of `push` vs `map` is a backend choice if the generator emitted an `IrExpr::Call("map", ...)` instead of a `For` loop. But the generator currently emits a `For` loop with `Assign` and `If`. If the generator emitted `IrStmt::DeclareArray` or an `IrExpr` using a `map` call, the backend could pretty-print it concisely. So: **fixable if the generator emits higher-level IR nodes** (e.g., `Call { func: "map", args: [...] }`). If it always emits `For`, the backend cannot turn it into `map`.

---

#### Pattern G — Output redirection via STDOUT save/restore

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

**Preferred idiomatic Perl**:
```perl
open my $fh, '>', 'temp1.txt' or die "Cannot open temp1.txt: $OS_ERROR\n";
print $fh "1\n2\n3";
close $fh;
```

**IR-fixable?**  
**No.** The generator chose to model `echo "..." > file` as "redirect STDOUT, run the echo, restore STDOUT" rather than "open file, write content, close file". This is a fundamental approach difference. The IR would see a block of `System`/`Output` nodes wrapped in redirect save/restore; the backend cannot infer that it should be a simple file write. The generator must be changed to emit `IrStmt::System { redirect: Some(FileWrite) }` or a file-write primitive.

---

#### Pattern H — `rm` with directory check and `carp`

```perl
if ( -e "temp1.txt" ) {
    if ( -d "temp1.txt" ) {
        carp "rm: carping: ", "temp1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp1.txt" ) {  }
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

**Preferred idiomatic Perl**:
```perl
unlink 'temp1.txt' or carp "could not remove temp1.txt: $OS_ERROR\n" if -e 'temp1.txt';
```

Or even simpler for `rm -f`:
```perl
unlink 'temp1.txt';
```

**IR-fixable?**  
**No.** The directory-check-and-carp logic is generated verbatim. The IR would see an `If` node with `TestExpr { op: FileTestExists }`, nested `If` with `TestExpr { op: FileTestDir }`, etc. The backend cannot simplify this to a bare `unlink` — the generator should emit a higher-level `System { cmd: "rm", args: [...] }` or just `Assign { targets: [...], expr: IrExpr::Call("unlink", ...) }`.

---

#### Pattern I — `xargs -n1` with nested `for` loop over 1-item chunks

```perl
my @xargs_input_16_1 = grep { $_ ne q{} } split /\s+/, $output_16;
my @xargs_output_16_1;
for my $i (0..scalar @xargs_input_16_1-1) {
    my @xargs_args_16_1;
    for my $j (0..1-1) {                    # inner loop: 0..0, always 1 iteration
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

The inner `for $j (0..1-1)` is a loop that always runs exactly once, suggesting the generator has a generic chunking mechanism that isn't optimized for `-n1`.

**Preferred idiomatic Perl**:
```perl
my $xargs_result = join "\n", map { "Number: $_" } split ' ', '1 2 3';
```

**IR-fixable?**  
**No.** The generator has a generic `xargs` expansion that always generates chunked loops. For `-n1`, the inner loop is dead code (always 1 iteration). An IR optimizer could theoretically detect and eliminate the dead inner loop, but that's optimizer work, not pretty-printing. The simpler fix is for the generator to special-case `-n1` and emit `map`.

---

#### Pattern J — `tr` translation incomplete/skeletal

```perl
my $tr_result = do { local $CHILD_ERROR = 0; do {
    do { do {
    my $output_14 = q{};
    my $output_printed_14;
    my $pipeline_success_14 = 1;
    $output_14 .= 'HELLO WORLD' . "\n";
    if ( !($output_14 =~ m{\n\z}) ) { $output_14 .= "\n"; }
    if ($CHILD_ERROR != 0) { $pipeline_success_14 = 0; }
    my $set1_15 = 'A-Z';
    my $set2_15 = 'a-z';
    my $input_15 = $output_14;;
say "Lowercase: $tr_result";
```

The actual `tr` translation (`y/A-Z/a-z/` on `$input_15`) is missing — it sets up variables and then skips straight to `say`. This is a generator bug, not a style issue.

---

#### Pattern K — `sort` with extra trailing-newline check

```perl
my @sort_sorted_4_1 = sort @sort_lines_4_1;
$output_4 = join "\n", @sort_sorted_4_1;
if ($output_4 ne q{} && !($output_4 =~ m{\n\z})) {
    $output_4 .= "\n";
}
```

The trailing-newline check is redundant after `join`, which never adds a trailing newline.

**Preferred idiomatic Perl**:
```perl
$output_4 = join "\n", sort split /\n/, $output_4;
```

**IR-fixable?**  
**Yes.** The backend knows the result of `join` never has a trailing newline, so the `if` check is always true — a constant-folding optimization in the IR.

---

#### Pattern L — `wc -w` and `wc -l` with verbose do-block

```perl
$output_6 = do {
    my $_wc_data = $output_6;
    my $_wc_words = scalar split /\s+/msx, $_wc_data;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_words;
    $_wc_result .= "\n";
    $_wc_result;
};
```

**Preferred idiomatic Perl**:
```perl
$output_6 = scalar split ' ', $output_6;
```

**IR-fixable?**  
**Partially.** The verbose string building (`$_wc_result .= sprintf ...; $_wc_result .= "\n"; $_wc_result`) is a backend choice — it could emit `sprintf "%d\n", ...` or just the number. However, the fundamental approach (split in scalar context) is fine; the verbosity is in how the result string is assembled.

---

#### Pattern M — Brace explosion and unmatched braces

The generated file ends with 20 closing braces `}` on separate lines, suggesting the brace-counting in the emitter is broken. This is a generator correctness bug.

**IR-fixable?**  
**Yes.** The IR backend is responsible for emitting properly balanced delimiters. If the IR program has correct nesting, `ir_to_perl()` would produce correct braces.

---

### 3. Summary: IR-fixable vs. generator-fixable

| Pattern | IR-fixable? | What would need to change |
|---|---|---|
| **A** — Pipeline boilerplate | ✅ Yes | Backend: single-stage pipeline → just the stage body |
| **B** — Contradictory newline handling | ✅ Yes | Backend: skip round-trip for known-safe expressions |
| **C** — Manual `head` via index/substr | ❌ No | Generator: detect `head N` → emit array slice |
| **D** — `grep -n` with quadratic membership | ❌ No | Generator: emit while+$. loop |
| **E** — `sed` as split-loop-join | ❌ No | Generator: detect simple s/// → emit single s///r |
| **F** — `cut` as split-loop-join | ⚠️ Partial | If generator emits `Call("map")` IR — fixable. If `For` — not. |
| **G** — STDOUT save/restore for redirect | ❌ No | Generator: emit file-write directly |
| **H** — `rm` with directory check | ❌ No | Generator: emit `unlink` directly |
| **I** — `xargs -n1` with dead inner loop | ❌ No | Generator: special-case -n1 → `map` |
| **J** — `tr` missing / incomplete | ❌ No | Generator: emit `y///` or `tr///` |
| **K** — Redundant trailing-newline after sort | ✅ Yes | Backend: constant-folding / dead-code elimination |
| **L** — `wc` verbose string building | ✅ Yes | Backend: emit `sprintf "%d\n", ...` instead of piecewise concat |
| **M** — Brace imbalance | ✅ Yes | Backend: correct brace emission |

### 4. Unnecessarily verbose translations (prime IR simplification candidates)

These are places where the generated code uses a sledgehammer for a thumbtack — complex control structures for trivial operations:

| Shell line | Generated size | Could be | Bloat factor |
|---|---|---|---|
| `echo "Hello World" \| sed 's/World/Universe/'` | ~25 lines | `(my $x = 'Hello World') =~ s/World/Universe/r` | ~20× |
| `echo "1 2 3" \| xargs -n1 echo "Number:"` | ~25 lines | `join "\n", map { "Number: $_" } split ' ', '1 2 3'` | ~20× |
| `seq 1 10 \| head -3` | ~30 lines | `join "\n", (1..10)[0..2]` | ~25× |
| `seq 1 10 \| tail -3` | ~25 lines | `join "\n", (1..10)[-3..-1]` | ~20× |
| `echo "apple:banana:cherry" \| cut -d: -f2` | ~20 lines | `(split /:/, 'apple:banana:cherry')[1]` | ~15× |
| `echo "Hello World" \| wc -w` | ~15 lines | `scalar split ' ', 'Hello World'` | ~12× |
| `echo -e "1\n2\n3" \| head -5` (cat version) | ~30 lines | `join '', (split /\n/, $input)[0..4]` | ~25× |
| `echo -e "1\n2\n3" > temp1.txt` | ~10 lines | `print {$fh} "1\n2\n3"` | ~8× |
| `rm -f temp1.txt` | ~10 lines | `unlink 'temp1.txt'` | ~8× |
| `echo -e "apple\napple\nbanana\nbanana\ncherry" \| uniq` | ~10 lines (incomplete) | `my @u; my $prev; for (split /\n/) { push @u, $_ if $_ ne $prev; $prev = $_ }` | ~3× |

### 5. Root cause

The generator translates shell pipeline semantics *literally* — each stage of a pipeline becomes a Perl data-flow variable (`$output_N`), with success/failure tracking, newline normalization, and do-block scoping — regardless of whether the pipeline contains one command or ten. The IR backend can compress boilerplate (single-stage pipelines, newline bookkeeping, brace correctness), but it **cannot** change the algorithm the generator chose to implement each command.

To get clean Perl, the generator itself must:

1. **Recognize trivial pipelines** — if a backtick contains `echo string | command args`, the generator should inline the string and apply the transformation directly rather than routing through the pipeline machinery.

2. **Use Perl built-ins** for common utilities instead of emulating them line-by-line (e.g., `s///` for `sed`, `split`/`join` for `cut`, `map` for `xargs -n1`).

3. **Write files directly** rather than redirecting STDOUT.

4. **Special-case `tr`**, `sort -u` (uniq), `head`, `tail`, and `wc` to use native Perl equivalents rather than manual implementations.

5. **Stop generating the pipeline scaffold** when the pipeline has no error-recovery logic that depends on it. The `$pipeline_success_N` → `$main_exit_code` dance is unused in this script.

The IR-based backend is a necessary but not sufficient condition for idiomatic output. It can fix *presentation* (braces, newlines, say/print, do-block depth) but not *substance* (what algorithm or data structure is used). For the latter, the generator's `generate_*_command()` functions need to be rewritten to emit higher-level IR nodes (e.g., `Call("map")`, `Expr::S///`, `DeclareArray` with list comprehensions) rather than lowering everything to `While`/`For`/`Assign`.