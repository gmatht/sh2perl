Let me look at the actual generated Perl for the pipeline script and also examine the project structure.Let me read the rest of the pipeline generation code and some of the individual command generators.Now I have a thorough understanding. Let me produce the idiom review.

---

# Idiom Review: Shell-to-Perl Translation of `sh/003_pipeline.sh`

## Source Script (Original)

```bash
ls | grep "\.txt$" | wc -l
echo
cat file.txt | sort | uniq -c | sort -nr
echo
find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
echo
cat file.txt | tr 'a' 'b' | grep 'hello'
echo
cat file.txt | sort | grep 'hello'
```

## Generated Perl (Abridged to Key Patterns)

The generated code is ~250 lines of Perl for this ~15-line script. Below I show representative excerpts that illustrate each pattern.

---

## Non-Idiomatic Patterns

### Pattern A: Over-defensive newline finalization on intermediate variables

Every pipeline stage that produces a string result has a trailing newline guard, even though that string will immediately be `split /\n/` by the next stage:

```perl
my $grep_result_0_1;
my @grep_lines_0_1 = split /\n/msx, $output_0;
my @grep_filtered_0_1 = grep { /[.]txt$/msx } @grep_lines_0_1;
$grep_result_0_1 = join "\n", @grep_filtered_0_1;
if (!($grep_result_0_1 =~ m{\n\z} || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
}
```

The next stage that reads `$output_0` will split on `\n` anyway, so ensuring the final newline at this point is wasted effort. Worse, it mis-models the semantics: the shell truth about newline preservation at the pipeline boundary is irrelevant when the translator is reimplementing the entire pipeline in in-memory Perl.

**IR-fixable?** Yes, via the `IrStmt::Pipeline` node. When the backend sees a Pipeline node whose stages are all in-memory (not `qx{}`), it can elide the trailing-newline guards on intermediate `Output { value: ... }` nodes. The backend knows the next stage will split by newline. Only the final stage (or the print to stdout) needs the guard.

---

### Pattern B: Split/Join sandwich for every pipeline stage

Every buffered stage pattern is: `split /regex/, $input → @lines → process → join "\n", @processed`. This is the mechanical shell-to-Perl transliteration: each command is treated as a line-at-a-time filter even in buffered mode.

```perl
# grep stage:
my @grep_lines_0_1 = split /\n/msx, $output_0;
my @grep_filtered_0_1 = grep { /[.]txt$/msx } @grep_lines_0_1;
$grep_result_0_1 = join "\n", @grep_filtered_0_1;

# sort stage:
my @sort_lines_3_1 = split /\n/, $output_3;
my @sort_sorted_3_1 = sort @sort_lines_3_1;
my $output_3_1 = join "\n", @sort_sorted_3_1;

# uniq -c stage:
my @uniq_lines_3_2 = split /\n/, $output_3;
@uniq_lines_3_2 = grep { $_ ne q{} } @uniq_lines_3_2;
my %uniq_counts_3_2;
...
my $output_3_2 = join "\n", @uniq_result_3_2;
```

In idiomatic Perl, `grep` on a multi-line string can use the `m//m` flag directly without split/join:

```perl
# Idiomatic: use /m on the string directly
my @filtered = $output =~ /^.*?[.]txt$/mg;
```

Similarly, `sort` on the lines of a string can use split/join but would be written as a single expression:

```perl
# Idiomatic one-liner:
my $sorted = join "\n", sort split /\n/, $output;
```

**IR-fixable?** Partially. The IR's `Pipeline` node knows about the stages. If the backend sees that stage N produces a string and stage N+1 reads that string, it could fuse the split/join sandwich. This would require an additional **IR-to-IR optimization pass** (fusion of adjacent split/process/join patterns). The `Pipeline` node is the right place to attach this optimization.

---

### Pattern C: Double assignment of pipeline output routing

Generated code contains duplicate identical assignments:

```perl
$output_0 = $grep_result_0_1;
$output_0 = $grep_result_0_1;   # <-- identical duplicate

$output_3 = $output_3_1;
$output_3 = $output_3_1;        # <-- identical duplicate

$output_4 = $tr_result_4_2;
$output_4 = $tr_result_4_2;     # <-- identical duplicate
```

This is a dead store — the second assignment has no effect. It's a code-generation artifact from routing the output through `$output_var` and then again through a pipeline synchronization step.

**IR-fixable?** Yes. The IR's optimization pass (`optimize_stmts`) already has dead-assignment elimination for `$x = $x;` self-assignments. Extending it to eliminate `$x = $y; $x = $y;` (consecutive identical duplicates) is straightforward. Add a peephole pass in `optimize_stmts` that detects consecutive `Assign` IR nodes with identical targets and values.

**Cleaned output:**
```perl
# Before                      # After
$output_0 = $grep_result_0_1;  $output_0 = $grep_result_0_1;
$output_0 = $grep_result_0_1;
```

---

### Pattern D: Manual `for`-loop character transliteration for `tr`

The `tr` command `tr 'a' 'b'` expands to ~50 lines of range-expansion boilerplate plus a character-by-character loop:

```perl
my $set1_7 = q{a};
my $set2_7 = q{b};
my $input_7 = $output_6;
# Expand character ranges for tr command
my $expanded_set1_7 = $set1_7;
my $expanded_set2_7 = $set2_7;
# Handle a-z range in set1
if ($expanded_set1_7 =~ /a-z/msx) {
    $expanded_set1_7 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# ... (many more range handlers) ...
my $tr_result_6_1 = q{};
for my $char ( split //msx, $input_7 ) {
    my $pos_7 = index $expanded_set1_7, $char;
    if ( $pos_7 >= 0 && $pos_7 < length $expanded_set2_7 ) {
        $tr_result_6_1 .= substr $expanded_set2_7, $pos_7, 1;
    } else {
        $tr_result_6_1 .= $char;
    }
}
```

Perl has a native `tr///` operator that does this in one line:

```perl
$output_6 =~ tr/a/b/;
```

And for the delete variant (`tr -d "\\/"`), Perl's `tr///d`:

```perl
$output_4 =~ tr|\\/||d;
```

**IR-fixable?** Yes, but it requires changes to **both** the generator logic and the IR backend. The generator currently emits `RawText` for the `tr` implementation. To fix this properly, the generator should emit an `IrExpr::Call { func: "tr", args: [...] }` or a dedicated `IrStmt::Translate` node. The IR backend's `ir_to_perl()` would then emit `$var =~ tr/SET1/SET2/;`.

The generator change is needed because the current generator doesn't emit semantic IR nodes for `tr` — it constructs the full loop as string text. However, once the generator produces the right IR node type, the backend can cleanly produce the idiomatic form.

**Candidate IR node:** A new `IrStmt::Translate { target: String, set1: String, set2: Option<String>, delete: bool }` or alternatively fold into `IrExpr::Call`.

**Cleaned output:**
```perl
# cat file.txt | tr 'a' 'b' | grep 'hello'
# becomes simply:
my $output = do { local $/; open my $fh, '<', 'file.txt'; <$fh>; };
$output =~ tr/a/b/;
my @matching = grep /hello/, split /\n/, $output;
print join "\n", @matching;
print "\n";
```

---

### Pattern E: Over-verbose `uniq -c` implementation

The `uniq -c` command generates a double-loop pattern with hash tracking:

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
my $output_3_2 = join "\n", @uniq_result_3_2;
if ($output_3_2 ne q{} && !($output_3_2 =~ m{\n\z})) {
    $output_3_2 .= "\n";
}
```

Idiomatic Perl would use a single loop with `sprintf`:

```perl
my %count;
$count{$_}++ for split /\n/, $output_3;
my $uniq_output = join "\n", map { sprintf "%7d %s", $count{$_}, $_ } keys %count;
# (or preserve order with a slice)
```

Or using a CPAN module:
```perl
use List::Util qw(uniq);
# or just use the inline approach
```

**IR-fixable?** This is a **generator logic** issue. The generator chooses to emit the `foreach` loops with hash/order tracking. An IR-only fix can't change the algorithm — the semantic decisions (order-preserving hash, sprintf format) are baked into the IR nodes the generator produces. To get idiomatic output, the generator would need to produce different IR, e.g., using `map` + `keys` or a single-pass `for` with a simpler body.

The IR could help by providing `map` and `grep` expression nodes, but the underlying algorithm choice belongs to the generator.

---

### Pattern F: Complex sort comparator for simple `-nr` flag

`sort -nr` (numeric reverse) generates:

```perl
my @sort_lines_3_3 = split /\n/, $output_3;
my @sort_sorted_3_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{};
    $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{};
    $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
} @sort_lines_3_3;
@sort_sorted_3_3 = reverse @sort_sorted_3_3;
```

Idiomatic Perl:

```perl
my @sorted = sort { $b <=> $a } split /\n/, $output_3;
```

(Shell's `sort -n` with no `-k` uses the full text as numeric key; even if `-k` were present, the multi-field extraction is overengineered for the common case.)

**IR-fixable?** This is a **generator logic** issue. The current generator always emits a field-splitting comparator for numeric sort, regardless of whether `-k` is specified. The generator should produce different IR for the `-n`-without-`-k` case vs the `-k` case.

Once the generator emits the right IR nodes (e.g., `sort { $b <=> $a } @list` as an `IrExpr::Call` with a comparator `IrStmt::Sort`), the backend would naturally produce clean output.

---

### Pattern G: Redundant variable name suffixes and pipeline scaffolding

The pipeline infrastructure emits names like `$output_0`, `$output_printed_0`, `$pipeline_success_0`, `$grep_result_0_1`, `$grep_filtered_0_1`, etc. Every pipeline gets a `do { my $output_N = q{}; my $output_printed_N; my $pipeline_success_N = 1; ... print/output handling ... }` wrapper, even when the pipeline is a simple single-command or three trivial stages.

For the first pipeline `ls | grep "\.txt$" | wc -l`:

```perl
do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    # ... ls, grep, wc ...
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
}
```

For a three-command pipeline that just prints, the idiomatic approach would be:

```perl
print do {
    my @files = grep /\.txt$/, do { opendir my $dh, '.'; readdir $dh };
    scalar @files, "\n";
};
```

**IR-fixable?** Partially. The `IrStmt::Pipeline` node already has a `capture` field for the `qx{}` shorthand. For side-effect pipelines (those that print), the IR backend could recognize simple compositions (all stages are filter/transform, not full subprocesses) and emit them as chained expressions instead of the full `do { $output=qq; ... print $output }` block. This is an **IR reduction pass**: detect when the pipeline stages are all in-memory pure functions (no subprocess calls) and eliminate the intermediate variables.

---

### Pattern H: Unnecessary `do { ... }; ;` with bare semicolon

The generated code has a trailing `}; ;` pattern:

```perl
$output_0 = do { ... };
;
```

This is a syntax artifact from the generator emitting a semicolon after the closing brace of a do-block, then a separate empty statement.

**IR-fixable?** Yes. The empty statement `;` is an `IrStmt::RawText(";")` or similar artifact in the generator's pipeline routing. The optimization pass can strip zero-length statements and bare semicolons. This is a simple peephole in `optimize_stmts()`.

---

### Pattern I: Manual `find` + `File::Find` instead of glob

The `find . -name "*.sh"` uses `File::Find`:

```perl
$output_4 = do {
    require File::Find;
    my @find_results = ();
    File::Find::find(sub { if ($_ =~ /^.*\.sh$/) { push @find_results, $File::Find::name; } }, '.');
    my $result = join "\n", @find_results;
    if ($result ne '') {
        $result .= "\n";
    }
    $CHILD_ERROR = 0;
    $result;
};
```

Perl can use the built-in `glob` operator:

```perl
my @files = glob('*.sh');
my $output = join "\n", @files;
$output .= "\n" if @files;
```

Or for recursive find, use `File::Find::Rule` or `glob` with `**/`.

**IR-fixable?** This is a **generator logic** issue. The `find` command generator chooses `File::Find`. Changing to `glob` requires modifying the generator. However, the IR backend can improve the formatting once the generator emits the right IR nodes (e.g., `IrExpr::Call { func: "glob", args: [...] }` for non-recursive, or `IrStmt::For` over `File::Find` results for recursive).

---

### Pattern J: $CHILD_ERROR tracking noise

Every command or pipeline sets `$CHILD_ERROR`:

```perl
$CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
$CHILD_ERROR = 0;
```

This is cargo-culted from shell's `$?` but rarely used in the generated script. The `main_exit_code` machinery already tracks failures.

**IR-fixable?** Yes. The `IrStmt::SetChildError` node can be omitted by an optimization pass when the value is never read. The current IR already has `SetChildError(IrExpr)` — an unused-assignment elimination pass can remove it.

---

### Pattern K: `cat file.txt` → do-block slurp with redundant error carp

```perl
$output = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) {
    local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh;
} else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
```

Idiomatic Perl slurp:

```perl
my $content = do { local $/; open my $fh, '<', 'file.txt'; <$fh> };
# or even shorter with Path::Tiny:
my $content = path('file.txt')->slurp;
```

**IR-fixable?** Yes, once the generator produces an `IrStmt::System`, `IrStmt::Assign`, or `IrStmt::Declare` with a `do { ... }` expression using IR nodes for `open`/`local`/`read`. The backend can recognize the "slurp" pattern (open + local $/ + <$fh>) and emit it more compactly. But the generator must first switch from emitting `RawText` to emitting proper IR nodes for the `cat` command.

---

## Summary Table

| Pattern | Description | IR-Fixable? | IR Node(s) Involved |
|---------|-------------|-------------|---------------------|
| **A** | Newline guard on intermediate pipeline vars | **Yes** | `Pipeline`, `Output` — elide newline guards on non-terminal stages |
| **B** | Split/join sandwich per stage | **Partial** (fusion pass) | `Pipeline` — fuse adjacent split/process/join stage patterns |
| **C** | Duplicate pipeline output assignment | **Yes** | `Assign` — dead-store elimination for identical consecutive assignments |
| **D** | Manual `for`-loop for `tr` | **Requires generator** | Needs new `IrStmt::Translate` or `Call { func: "tr" }` in generator; backend then emits `tr///` |
| **E** | Over-verbose `uniq -c` with double foreach | **Generator** | Algorithm choice is in generator; can't fix purely in IR |
| **F** | Complex comparator for `sort -nr` | **Generator** | Generator should emit `sort { $b <=> $a }` directly for simple cases |
| **G** | Pipeline `do { ... }` scaffolding for trivial pipes | **Yes** (simplification pass) | `Pipeline` — reduce to chained expression when stages are pure functions |
| **H** | Bare semicolons after do-blocks | **Yes** | Peephole in `optimize_stmts()` — strip empty statements |
| **I** | `File::Find` instead of `glob` | **Generator** | Generator decides implementation; IR only formats it |
| **J** | Dead `$CHILD_ERROR` writes | **Yes** | `SetChildError` — unused-write elimination |
| **K** | Verbose file slurp | **Partial** | Generator must emit IR nodes; backend can compact the slurp idiom |

---

## Unnecessarily Verbose Translations (Prime IR Candidates)

These are the worst offenders where a simple shell operation is drowned in control-structure boilerplate:

### 1. `cat file.txt | tr 'a' 'b' | grep 'hello'`

**Generated:** ~80 lines (slurp + 50-line tr impl + grep split/join + pipeline wrapping)

**Should be:**
```perl
my $content = do { local $/; open my $fh, '<', 'file.txt'; <$fh> };
$content =~ tr/a/b/;
my @matching = grep /hello/, split /\n/, $content;
print join "\n", @matching;
print "\n";
```

**Verdict:** ~80 → ~6 lines. The `for`-loop `tr` implementation is the dominant cost; using Perl's native `tr///` eliminates a 50-line block.

### 2. `ls | grep "\.txt$" | wc -l`

**Generated:** ~40 lines of pipeline boilerplate + ls simulation + split/join grep + wc do-block

**Should be:**
```perl
opendir my $dh, '.' or die;
my @txt_files = grep /\.txt$/, readdir $dh;
closedir $dh;
print scalar @txt_files, "\n";
```

**Verdict:** ~40 → ~5 lines. The pipeline scaffold is completely unnecessary when the operations are purely in-memory.

### 3. `cat file.txt | sort | uniq -c | sort -nr`

**Generated:** ~60 lines covering sort (split/join + field comparator), uniq (double foreach), sort-r (split/join + numeric field comparator + reverse)

**Should be:**
```perl
use List::Util qw(uniq);
my $content = do { local $/; open my $fh, '<', 'file.txt'; <$fh> };
my %count;
$count{$_}++ for split /\n/, $content;
my @sorted = sort { $count{$b} <=> $count{$a} } keys %count;
print join "\n", map { sprintf "%7d %s", $count{$_}, $_ } @sorted;
print "\n";
```

**Verdict:** ~60 → ~10 lines. The pipeline is a `cat` + three in-memory filters; the generated code treats each as a separate "stage" with full buffering and variable routing.

### 4. `cat file.txt | sort | grep 'hello'`

**Generated:** ~40 lines (buffered pipeline, same split/join pattern)

**Should be:**
```perl
my $content = do { local $/; open my $fh, '<', 'file.txt'; <$fh> };
my @matching = grep /hello/, sort split /\n/, $content;
print join "\n", @matching;
print "\n";
```

**Verdict:** ~40 → ~5 lines.

---

## Recommendations for the IR Backend

1. **Add a pipeline fusion pass** that recognizes when all stages are in-memory operations (no subprocess calls, no redirects) and eliminates the intermediate `$output_N` variables. Emit a single chained expression instead.

2. **Add dead-assignment elimination** for consecutive duplicate `$x = $y; $x = $y;` patterns (already partially done for `$x = $x;`).

3. **Add unused-write elimination** for `$CHILD_ERROR` assignments that are never read.

4. **Add semicolon/whitespace cleanup** as a peephole pass over the final statement list.

5. **For the pipeline wrapper block** (`do { my $output_N = q{}; ... }`): when the pipeline has no redirects and consists entirely of simple filter commands, emit a straight-line sequence without the `do` wrapper and the `$pipeline_success_N`/`$output_printed_N` machinery.

## Recommendations for the Generator

1. **`tr`**: Emit `IrStmt::Translate { target, set1, set2, delete }` instead of the 50-line character-loop text. The backend then emits `$var =~ tr/a/b/;`.

2. **`sort`**: Emit simpler IR for `sort -n` without `-k`: `sort { $a <=> $b } @lines`. Only emit the field-splitting comparator when `-k` is actually specified.

3. **`uniq`**: Consider emitting `map`+`keys` or a single-pass loop instead of the double-foreach pattern.

4. **`find`**: For non-recursive `find . -name "X"`, use `glob("X")` instead of `File::Find`.

5. **`cat`**: Emit proper `IrStmt::Declare` with a slurp expression using IR nodes (open, local, read) instead of `RawText` with the do-block. The backend can then produce the compact `do { local $/; ... }` form.

6. **First-command-in-pipeline optimization**: When the first command is `cat file`, `echo`, or another pure-data-source command that doesn't need pipeline infrastructure, fuse its output directly into the downstream expressions rather than routing through a pipeline variable.

The IR architecture described in `docs/ir-design.md` already provides the right foundation. The key insight from this review is that **the biggest wins come from adding IR-to-IR transformation passes (fusion, elimination, sinking) rather than from changing how individual nodes are pretty-printed.** A `Pipeline` fuser that recognizes "all in-memory, no subprocesses, no redirects" and collapses the intermediate variables would be the single most impactful optimization.