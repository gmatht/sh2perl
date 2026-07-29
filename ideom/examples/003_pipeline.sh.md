Now I have a thorough understanding of the generator, IR system, and the generated output. Let me write the comprehensive idiom review.

---

# Idiom Review: `examples/003_pipeline.sh` → Generated Perl

## Original Shell Script

```bash
#!/bin/bash

# Pipeline examples
ls | grep "\.txt$" | wc -l
echo
cat file.txt | sort | uniq -c | sort -nr
echo
find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
echo
# This pipeline will use line-by-line processing:
cat file.txt | tr 'a' 'b' | grep 'hello'
echo
# This pipeline will fall back to buffered processing:
cat file.txt | sort | grep 'hello'
```

---

## Summary of Non-Idiomatic Patterns

| # | Pattern | IR-Fixable? | Severity |
|---|---------|-------------|----------|
| A | Pipeline scaffolding boilerplate (`do { my $output... }`) | ✅ Yes | High |
| B | Contradictory double newline handling | ✅ Yes | High |
| C | Reimplementing standard Unix tools in Perl (`ls`, `wc`, `tr`, `uniq`) | ❌ No | Medium |
| D | Split/join pipeline for `grep` instead of single regex on whole string | ❌ No | Medium |
| E | Custom sort block for `-nr` using field parsing instead of numeric sort | ❌ No | Low |
| F | Duplicate assignments (`$output_N = $result;` appearing twice) | ✅ Yes | Low |
| G | `q{}` for empty strings instead of `''` | ✅ Yes | Low |
| H | Spurious `/msx` regex flags on every pattern | ✅ Yes | Low |
| I | Trailing-newline dance instead of `chomp`/`say` | ✅ Yes | High |
| J | Variable naming with opaque numeric suffixes | ❌ No | Low |

---

## Detailed Pattern Analysis

### Pattern A — Pipeline Scaffolding Boilerplate (HIGH)

Every pipeline is wrapped in ~15 lines of orchestration:

```perl
do {
    my $output_6 = q{};
    my $output_printed_6;
    my $pipeline_success_6 = 1;
    # ... commands that populate $output_6 ...
    if ($output_6 ne q{} && !defined $output_printed_6) {
        print $output_6;
        if (!($output_6 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_6 ) { $main_exit_code = 1; }
}
```

This is repeated **five times** in the output. For a trivial pipeline like `cat file.txt | tr 'a' 'b' | grep 'hello'`, the scaffolding is ~3× the size of the actual data-processing logic.

**IR-fixable?** ✅ **Yes.**

The `IrStmt::Pipeline` node already has a `capture` field. When set, the backend emits a single clean `qx{...}` call:

```perl
my $output_6 = qx{cat file.txt | tr 'a' 'b' | grep 'hello'};
chomp $output_6;
print $output_6, "\n";
```

**Cleaned-up output with IR `Pipeline { capture }`:**

```perl
my $output_0 = qx{ls | grep '\.txt$' | wc -l};
chomp $output_0;
print $output_0, "\n";

my $output_3 = qx{cat file.txt | sort | uniq -c | sort -nr};
chomp $output_3;
print $output_3, "\n";
# etc.
```

No more `$output_printed`, `$pipeline_success`, or `$main_exit_code` tracking for simple pipelines. The `Pipeline { capture, cmd_str }` IR node already has this design — see `emit_stmt` in `ir.rs` lines 638–650:

```rust
IrStmt::Pipeline { stages, capture, cmd_str, .. } => {
    if let Some(var) = capture {
        if let Some(cmd) = cmd_str {
            emit_indent(out, indent);
            out.push_str(&format!("my ${} = qx{{{}}};\n", var, cmd));
            emit_indent(out, indent);
            out.push_str(&format!("chomp ${};\n", var));
        }
    }
}
```

The generator just needs to stop emitting `RawText` and start emitting `IrStmt::Pipeline { capture }`.

---

### Pattern B — Contradictory Double Newline Handling (HIGH)

The generated code adds a trailing newline to every intermediate result, then later checks whether the output ends with `\n` and adds another one:

```perl
# Stage 1: Add trailing newline to grep result
$grep_result_0_1 = join "\n", @grep_filtered_0_1;
if (!($grep_result_0_1 =~ m{\n\z} || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
}

# ... later, at print time ...
if ($output_0 ne q{} && !defined $output_printed_0) {
    print $output_0;
    if (!($output_0 =~ m{\n\z})) {   # <-- checks AGAIN
        print "\n";
    }
}
```

The inner stage adds `\n` unconditionally (the guard `m{\n\z}` is vacuously false because `join` never produces a trailing `\n`), so the outer check always skips the extra print. The inner guard is dead code — the `join` result never ends with `\n`.

**IR-fixable?** ✅ **Yes.**

The `IrStmt::Output { value, newline: true }` node handles this in one place. The backend would emit:

```perl
say $output_0;
```

or equivalently:

```perl
print $output_0, "\n";
```

No intermediate newline patching needed. The IR backend already implements this pattern — see `ir.rs` lines 387–399:

```rust
IrStmt::Output { value, newline, target } => {
    let expr = ir_expr_to_perl(value);
    if *newline {
        if /* is double-quoted string */ {
            print "\"$inner\\n\"";
        } else {
            print "$expr.\"\\n\"";
        }
    } else {
        print "$expr;";
    }
}
```

The generator just needs to use `IrStmt::Output` instead of the raw newline-dance.

---

### Pattern C — Reimplementing Standard Unix Tools in Perl (MEDIUM)

The generator replaces `ls`, `wc -l`, `tr`, `uniq -c`, `xargs grep -l`, and `sort -nr` with hand-coded Perl implementations. This is the biggest source of verbosity.

#### `ls` → 15 lines of `opendir`/`readdir` + sort:

```perl
my @ls_files_1 = ();
if ( -f q{.} ) {
    push @ls_files_1, q{.};
}
elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
    push @ls_files_1, $file;
    }
    closedir $dh;
    @ls_files_1 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_files_1;
    }
}
(@ls_files_1 ? join("\n", @ls_files_1) . "\n" : q{});
```

**Idiomatic Perl** (if you must reimplement):

```perl
my @ls_files_1 = grep !/^\./, glob '*';
my $output_0 = @ls_files_1 ? join("\n", @ls_files_1) . "\n" : '';
```

Or better, with pipeline capture (Pattern A), none of this is needed.

#### `wc -l` → newline counter:

```perl
my $_wc_data = $output_0;
my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
my $_wc_result = sprintf("%d \n", $_wc_lines);
$_wc_result;
```

**Idiomatic Perl:**

```perl
my $wc_result = sprintf "%d\n", ($output_0 =~ tr/\n//);
```

(The `tr///` operator returns the count in scalar context — no regex match needed.)

#### `tr -d "\\/"` → character-loop:

```perl
my $set1_5 = "\\/";
my $input_5 = $output_4;
my $tr_result_4_2 = q{};
for my $char ( split //msx, $input_5 ) {
    if ( (index $set1_5, $char) == -1 ) {
        $tr_result_4_2 .= $char;
    }
}
```

**Idiomatic Perl** (2 chars, use `tr///`):

```perl
(my $tr_result_4_2 = $output_4) =~ tr|\\/||d;
```

#### `uniq -c ` → hash-count + loop + format:

```perl
my @uniq_lines_3_2 = split /\n/, $output_3;
@uniq_lines_3_2 = grep { $_ ne q{} } @uniq_lines_3_2;
my %uniq_counts_3_2;
my @uniq_order_3_2;
foreach my $line (@uniq_lines_3_2) {
    if (!exists $uniq_counts_3_2{$line}) { push @uniq_order_3_2, $line; }
    $uniq_counts_3_2{$line}++;
}
my @uniq_result_3_2;
foreach my $line (@uniq_order_3_2) {
    push @uniq_result_3_2, sprintf "%7d %s", $uniq_counts_3_2{$line}, $line;
}
```

This is correct but verbose. **Idiomatic Perl** (preserving order):

```perl
my @lines = grep $_, split /\n/, $output_3;
my %count;
$count{$_}++ for @lines;
my @uniq_result = map sprintf("%7d %s", $count{$_}, $_), @lines;
```

Note: `$count{$_}++ for @lines` is the standard Perl idiom: a postfix `for` loop that populates a hash — no existence check needed because `++` on `undef` yields 1.

**IR-fixable?** ❌ **No.**

These are generator-level choices. The generator explicitly decides to emit Perl code that mimics `ls` instead of calling the system `ls`. The IR has no way to know that this 15-line block is "implementing `ls`" — it would need to see `IrStmt::RawText(...)` or an `IrStmt::System { cmd: "ls" }`. The fix is in the generator: emit `IrStmt::System { cmd: IrExpr::Str("ls"), args: [...] }` or better, use `Pipeline { capture }` so the whole composite runs externally.

---

### Pattern D — Split/join pipeline for `grep` (MEDIUM)

Every `grep` in a pipeline is implemented as split → array `grep` → join → newline-patch:

```perl
my $grep_result_6_2;
my @grep_lines_6_2 = split /\n/msx, $output_6;
my @grep_filtered_6_2 = grep { /hello/msx } @grep_lines_6_2;
$grep_result_6_2 = join "\n", @grep_filtered_6_2;
if (!($grep_result_6_2 =~ m{\n\z} || $grep_result_6_2 eq q{})) {
    $grep_result_6_2 .= "\n";
}
```

**Idiomatic Perl** — use a single regex match on the whole string:

```perl
my @grep_filtered_6_2 = $output_6 =~ /^.*hello.*$/gm;
```

Or simply:

```perl
my @grep_filtered_6_2 = grep /hello/, split /\n/, $output_6;
$grep_result_6_2 = join "\n", @grep_filtered_6_2;
```

Without `/msx` flags that are meaningless here (no `.` that needs `/s`, no `^`/`$` that need `/m`).

**IR-fixable?** ❌ **No.**

The IR doesn't understand that `grep` from the shell maps to a regex match. The generator decides the implementation strategy. However, `Pipeline { capture }` would side-step this entirely by delegating to the system `grep`.

---

### Pattern E — Custom Sort Block for `-nr` (LOW)

```perl
my @sort_sorted_3_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
} @sort_lines_3_3;
@sort_sorted_3_3 = reverse @sort_sorted_3_3;
```

The data was just produced by `uniq -c` with format `%7d %s`, so the first field is always a right-justified number. A Schwartzian transform or a simple numeric sort would suffice:

```perl
my @sort_sorted_3_3 = sort { $b <=> $a } @sort_lines_3_3;
```

Since the lines are already `"      5 foo"`, Perl's `<=>` will coerce the string to a number (stopping at the first space). No field splitting needed.

**IR-fixable?** ❌ **No.**

The generator emits this logic because it doesn't know the data format. This is a semantic understanding problem that would require the generator to track data provenance through the pipeline.

---

### Pattern F — Duplicate Assignments (LOW)

The same value is assigned twice:

```perl
$output_0 = $grep_result_0_1;
$output_0 = $grep_result_0_1;
```

This pattern occurs in multiple pipelines. It's dead code — the first assignment is immediately overwritten.

**IR-fixable?** ✅ **Yes.**

A simple dead-assignment elimination optimization pass on the IR would remove the first (or second) duplicate. The `optimize_stmts` function in `ir.rs` already exists for this kind of pass. The IR node involved is `IrStmt::Assign`. The cleaned output would simply have one assignment instead of two.

---

### Pattern G — `q{}` Instead of `''` for Empty Strings (LOW)

```perl
my $output_0 = q{};
```

Perl's `q{}` is a general quoting operator. For an empty string, `''` is the standard Perl convention:

```perl
my $output_0 = '';
```

**IR-fixable?** ✅ **Yes.**

The `IrStmt::Declare { init: Some(IrExpr::Str("", StrStyle::SingleQuoted)) }` node's backend would emit `'...'`. Currently the generator emits `q{}` via `RawText`. Once migrated to IR `Str` expressions, the backend would produce `''` for empty strings.

The `StrStyle::SingleQuoted` emitter in `ir.rs` (lines 793–807) already handles this correctly — it uses `'...'` quoting:

```rust
StrStyle::SingleQuoted => {
    format!("'{}'", s.replace('\'', "\\'"))
}
```

An empty `s` would produce `''`.

---

### Pattern H — Spurious `/msx` Regex Flags on Every Pattern (LOW)

```perl
my @grep_lines_0_1 = split /\n/msx, $output_0;
my @grep_filtered_0_1 = grep { /[.]txt$/msx } @grep_lines_0_1;
```

The `/msx` flags:
- `/m` — makes `^`/`$` match line boundaries. The pattern `\.txt$` uses `$` (end of string, not line) and there's no `^`.
- `/s` — makes `.` match `\n`. No `.` in the pattern.
- `/x` — allows whitespace/comments. No whitespace in the pattern that would change meaning.

These flags are cargo-culted from the generator's boilerplate. They add visual noise and can mask bugs (e.g., `$` with `/m` matches end-of-line, not end-of-string, which is usually wrong for a pipeline output).

**IR-fixable?** ✅ **Yes.**

The `IrExpr::Regex` backend in `ir.rs` (lines 782–790) already has a flag-cleanup pass:

```rust
IrExpr::Regex { pattern, flags } => {
    let clean_flags: String = flags.chars().filter(|&c| {
        if c == 'm' {
            !pattern.contains('^') && !pattern.contains('$')
        } else if c == 's' {
            !pattern.contains('.')
        } else if c == 'x' {
            true  // always strip /x
        } else { true }
    }).collect();
}
```

If the generator emitted `IrExpr::Regex` instead of embedding `/msx` in `RawText`, the backend would strip the spurious flags automatically, producing:

```perl
my @grep_lines_0_1 = split /\n/, $output_0;
my @grep_filtered_0_1 = grep { /[.]txt$/ } @grep_lines_0_1;
```

---

### Pattern I — Trailing-Newline Dance Instead of `chomp`/`say` (HIGH)

The generated code has an elaborate pattern for every output line:

```perl
if ($output_N ne q{} && !defined $output_printed_N) {
    print $output_N;
    if (!($output_N =~ m{\n\z})) {
        print "\n";
    }
}
```

This is a defensive "print with guaranteed trailing newline" that:
1. Checks if output is non-empty
2. Checks if it hasn't been printed yet (via `$output_printed_N`)
3. Prints the value
4. Checks if the value already ends with `\n`
5. If not, prints an extra `\n`

**Idiomatic Perl** — just use `say` or `print` with concatenated `\n`:

```perl
print $output_N, "\n" if $output_N ne '';
```

Or if you must preserve the exact content:

```perl
say $output_N if $output_N ne '';
```

No need for `$output_printed_N` tracking when the pipeline output is a single value.

**IR-fixable?** ✅ **Yes.**

The `IrStmt::Output { newline: true }` node produces exactly this — and the backend does it cleanly. The `$output_printed` guard exists because the pipeline uses a shared `$output_N` variable that could be printed by multiple stages (in case of fallback paths). With pipeline capture (`qx{...}`), there's no intermediate variable to double-print.

---

### Pattern J — Variable Naming with Opaque Numeric Suffixes (LOW)

Variables like `$output_3_1`, `$grep_result_0_1`, `$output_printed_8`, `$xargs_result_4_1` use compound numeric suffixes (`<pipeline_id>_<stage_id>`) that make the code read like assembly:

```perl
$output_0 = $grep_result_0_1;
$output_0 = $output_0_2;
```

This is the most visible symptom of line-by-line transliteration. A human would name variables by their role (`$ls_out`, `$grep_result`, `$wc_out`, or better, chain them without intermediate variables).

**IR-fixable?** ❌ **No.**

The IR doesn't control naming — it receives variable names from the generator. Fixing this requires changing the generator to assign meaningful names or to use expression composition instead of named temporaries.

However, with `Pipeline { capture }` (Pattern A), the intermediate variables largely disappear — you just need one `$output_N` variable for the final result.

---

## Unnecessarily Verbose Translations (Prime Candidates for IR Simplification)

### 1. Single-command pipelines wrapped in `for` loops with `open3` infrastructure

When a pipeline has one command (or a command in a for-loop body), the generator wraps it in `open3` with `bash -c '...'` subprocess calls. Example from the `for` loop in `find | xargs`:

```perl
my ($in_xx, $out_xx);
my $pid_xx = open3($in_xx, $out_xx, '>&STDERR', @_pcmd_xx);
close $in_xx ...;
while (my $line = <$out_xx>) { $output .= $line; }
close $out_xx ...;
waitpid $pid_xx, 0;
$CHILD_ERROR = $? >> 8;
```

This is ~8 lines of infrastructure per command. With IR `Pipeline { capture }`, this becomes a single `qx{...}` call.

### 2. `cat file.txt` → 6-line `open`/`close` block

```perl
do {
    my $cat_chunk = q{};
    if ( open my $fh, '<', 'file.txt' ) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $cat_chunk = <$fh>;
        close $fh;
    } else {
        carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n";
    }
    $cat_chunk;
};
```

Idiomatic Perl: `do { local (@ARGV, $/) = 'file.txt'; <> }` or simply `qx{cat file.txt}`.

### 3. Nested `do` blocks for simple expressions

The `wc -l` implementation is wrapped in `do { ...; }`:

```perl
my $output_0_2 = do {
    my $_wc_data = $output_0;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = sprintf("%d \n", $_wc_lines);
    $_wc_result;
};
```

The `do` block is unnecessary — it's used as a function body but Perl doesn't need it for a straight-line expression:

```perl
my $output_0_2 = sprintf "%d\n", scalar( $output_0 =~ tr/\n// );
```

### 4. Trivial `tr 'a' 'b'` expands character ranges

For `tr 'a' 'b'`, the generator emits range-expansion code with 10 `if` blocks checking for `a-z`, `A-Z`, `[:upper:]`, `[:lower:]`:

```perl
my $expanded_set1_7 = $set1_7;
my $expanded_set2_7 = $set2_7;
if ($expanded_set1_7 =~ /a-z/msx) { ... }
if ($expanded_set1_7 =~ /A-Z/msx) { ... }
# ... 8 more if blocks ...
```

For single-character sets, the expansion is a no-op. The generator doesn't optimize for the trivial case.

**IR fix**: With `Pipeline { capture }`, `cat file.txt | tr 'a' 'b'` becomes `qx{cat file.txt | tr 'a' 'b'}`, side-stepping the entire tr implementation. Without capture, a generator optimization could check if set length ≤ 1 and skip expansion.

---

## Summary Table

| Pattern | Root Cause | IR Fix | Effort | Impact |
|---------|-----------|--------|--------|--------|
| A. Pipeline scaffolding | `RawText` + rewrite of every pipeline | Use `Pipeline { capture, cmd_str }` | Medium | Huge — removes ~70% of generated code |
| B. Double newline handling | Each stage adds `\n`, then output also adds `\n` | Use `Output { newline: true }` | Low | Medium — cleans print logic |
| C. Tool reimplementation (ls, wc, tr, ...) | Generator chooses Perl-native impl over `qx` | Generator fix: prefer `qx` for non-trivial tools | High | Medium — most tools are just a function call away |
| D. Split/join for grep | Generator implements `grep` as array filter | Generator fix: single regex on whole string | Low | Low-Medium |
| E. Complex `sort -nr` | Generator doesn't trust data format | Generator fix: emit simpler sort | Low | Low |
| F. Duplicate assignments | Generator writes same var twice | IR optimization pass | Low | Low |
| G. `q{}` instead of `''` | Generator default quoting | IR `Str` node already uses `''` | Low | Cosmetic |
| H. Spurious `/msx` flags | Generator boilerplate | IR `Regex` node strips them | Low | Cosmetic |
| I. Newline dance | Generator defensive output | Use `Output { newline: true }` | Low | Medium |
| J. Numeric variable suffixes | Generator uses counter-based IDs | Generator fix: role-based names | Medium | Low-medium |

**Bottom line**: The single highest-impact fix is migrating pipeline generation from `RawText` to `IrStmt::Pipeline { capture, cmd_str }`. This would transform the entire output from ~200 lines of scaffolding + reimplemented tools into ~15 lines of clean `qx{...}` calls with `chomp` and `print`. All five pipelines would be handled uniformly, and Patterns A, B, C (partially), F, and I would be resolved in one change.