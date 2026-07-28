Now I have a thorough understanding of the generated code and the IR system. Let me compile the idiom review.

---

## Idiom Review: `sh/000__04h_complex_examples.sh` → Generated Perl

### Source script recap

The shell script exercises: nested backticks, backticks in assignments, conditionals, case statements, functions, arrays, process substitution, here-strings, and `perl -e` with backticks.

---

### Pattern A — Pipeline boilerplate for a trivial pipeline (`ls -1 | wc -l`)

**Generated code** (the `$count` assignment):

```perl
my $count = do {
    do { do {
        my $output_1 = q{};
        my $output_printed_1;
        my $pipeline_success_1 = 1;
        $output_1 = do {
            my @ls_files_2 = ();
            if ( -f q{.} ) {
                push @ls_files_2, q{.};
            }
            elsif ( -d q{.} ) {
                if ( opendir my $dh, q{.} ) {
                    while ( my $file = readdir $dh ) {
                        next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                        push @ls_files_2, $file;
                    }
                    closedir $dh;
                    @ls_files_2 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_files_2;
                }
            }
            (@ls_files_2 ? join("\n", @ls_files_2) . "\n" : q{});
        };
        ;
        if ($CHILD_ERROR != 0) { $pipeline_success_1 = 0; }
        $output_1 = do {
            my $_wc_data = $output_1;
            my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
            my $_wc_result = sprintf("%d \n", $_wc_lines);
            $_wc_result;
        };
        if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
        chomp $output_1;
        $output_1;
    }; };
};
```

**Preferred idiomatic Perl:**

```perl
my $count = qx{ls -1 | wc -l};
chomp $count;
```

Or, if we really want native Perl:

```perl
my $count = (() = glob('*')) . "\n";
```

**IR-fixable?** Yes. The generator is emitting `Pipeline { stages: [...] }` with individual command IR nodes for `ls` and `wc`, plus `Capture` and `Assign` IR. The backend (`ir_to_perl`) sees the full pipeline IR and could detect that the entire pipeline has a simple `Command::Simple` at each stage and no redirects. It could collapse the whole thing into an `IrExpr::Backtick { expr: ..., native: false }`. The IR node involved would be `IrStmt::Pipeline` combined with `IrExpr::Backtick`. When the pretty-printer sees a pipeline with only simple commands and no internal Perl-emulatable logic, it emits `qx{...}` instead of the 30-line scaffold.

---

### Pattern B — Nested backtick expansion turned into a `while` loop (`yes well | head -3`)

**Generated code** (inside `$nested_result`):

```perl
my $nested_result = "Three wells: " . (do {
    do { my $output_0 = q{};
        my $output_printed_0;
        my $head_line_count = 0;
        while (1) {
            my $line = 'well';
            if ($head_line_count < 3) {
                $output_0 .= $line . "\n";
                ++$head_line_count;
            } else {
                $line = q{};
```

**Preferred idiomatic Perl:**

```perl
my $nested_result = "Three wells: " . qx{yes well | head -3};
chomp $nested_result;  # or keep trailing newline
```

**IR-fixable?** Yes. The `yes` and `head` commands are being expanded into their internal emulation (a `while(1)` loop for `yes`, a line counter for `head`). The IR node for command substitution is `IrExpr::Backtick`. When the backend encounters a `Backtick` whose inner expression is a `BinOp`/pipeline involving emulated commands, it should have the option to **fall back to native qx** if the emulation makes the code complex. The decision belongs in `ir_to_perl()`: if the expression tree is "too deep" or contains multiple emulated commands, emit `qx{...}` with the original shell text rather than expanding each command into Perl.

---

### Pattern C — Redundant triple variable declaration (`my $files; my @files; my %files`)

**Generated code:**

```perl
my $files;
my @files = (do { my $_result = `ls -1 *.sh examples/*.sh 2>/dev/null`; chomp $_result; $CHILD_ERROR = $? >> 8; split("\n", $_result); });
my %files;
```

**Preferred idiomatic Perl:**

```perl
my @files = (do { my $_result = `ls -1 *.sh examples/*.sh 2>/dev/null`; chomp $_result; split("\n", $_result); });
```

(or more idiomatically:)

```perl
my @files = glob('*.sh examples/*.sh');
```

**IR-fixable?** Partially. The IR currently has separate `Declare { sigil: Scalar }`, `DeclareArray { sigil: Array }`, and another `Declare { sigil: Hash }` for the same name. An IR optimization pass (dead-assignment elimination) could remove `my $files;` since it's immediately overwritten by `my @files`. And `my %files;` is unused — a dead-code elimination pass would remove it. **However**, the root cause is that the generator emits a scalar, array, and hash declaration for the same shell variable name regardless of usage. That requires a **generator change**: the generator should track which sigil is actually used and only emit that one declaration.

---

### Pattern D — Echo > file via STDOUT save/restore (`echo -e "apple\nbanana\ncherry" > file1.txt`)

**Generated code:**

```perl
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
        say "apple\nbanana\ncherry";
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
```

**Preferred idiomatic Perl:**

```perl
use autodie;
open my $fh, '>', 'file1.txt';
say $fh "apple\nbanana\ncherry";
close $fh;
```

Or for one-offs:

```perl
system("echo -e 'apple\nbanana\ncherry' > file1.txt");
```

**IR-fixable?** Yes. The IR has `Redirect { ... }` and `Output { value: ..., file: ... }`. In the IR design, `IrStmt::Output` can take an optional filehandle target. The backend should emit `say $fh ...` when a filehandle is present, not the STDOUT-swizzling pattern. The IR node is `IrStmt::Output` with a filehandle or path attached. The cleanup is entirely in `ir_to_perl()`: instead of the save-restore scaffolding, emit `open my $fh, '>', $path; say $fh ...; close $fh;`.

---

### Pattern E — `wc -c < "$file"` simulated by Perl file-slurp + `length()`

**Generated code:**

```perl
my $size = do {
    my $wc_file = "$file";
    my $wc_file_opened = 0;
    my $content = do {
        my $result = q{};
        if (open my $fh, '<', $wc_file) {
            $wc_file_opened = 1;
            local $INPUT_RECORD_SEPARATOR = undef;
            $result = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR\n";
        } else {
            warn "Cannot open $wc_file: $OS_ERROR\n";
        }
        $result;
    };
    $wc_file_opened ? do {
        my $wc_bytes = length($content);
        $wc_bytes;
    } : q{};
};
```

**Preferred idiomatic Perl:**

```perl
my $size = -s $file;
```

Or, more robustly:

```perl
my $size = (-s $file) // warn("Cannot stat $file: $!"), 0;
```

**IR-fixable?** Yes. The IR for this is a `Pipeline { stages: [wc capture redirect from file] }` which gets lowered to an `IrExpr::Backtick` or worse, to individual filesystem operations. When the backend (`ir_to_perl()`) sees a `FileTest { path, test: Size }` or more generally when it recognizes the pattern of `wc -c < file` as just checking file size, it should emit the `-s` operator. This requires either a generator-level recognition (emitting `IrExpr::Call { func: "-s", args: [...] }`) or an IR optimization pass that pattern-matches the open+slurp+length tree and replaces it with `IrExpr::Call { func: "-s", ... }`.

---

### Pattern F — Case/switch translated to regex matches for literal values

**Generated code:**

```perl
if ($system_name =~ /^Linux$/msx) {
    say "Running on Linux";
} elsif ($system_name =~ /^Darwin$/msx) {
    say "Running on macOS";
} elsif (1) {
    say "Running on other " . "sys" . "tem";
}
```

**Preferred idiomatic Perl:**

```perl
if ($system_name eq 'Linux') {
    say "Running on Linux";
} elsif ($system_name eq 'Darwin') {
    say "Running on macOS";
} else {
    say "Running on other system";
}
```

**IR-fixable?** Yes. The `case` statement in shell can contain glob patterns (`Linux*`, `Darwin*`, etc.) or literals. The generator already knows whether the pattern is a simple literal or a glob. It should emit `IrExpr::BinOp { op: Eq, ... }` for literal patterns instead of `IrExpr::RawExpr("... =~ /^...$/msx")`. The IR node is `IrStmt::If { cond: IrExpr::BinOp { op: Eq, ... }, ... }`. The backend would then emit `eq`. Also, `"other " . "sys" . "tem"` should obviously be `"other system"` — this is a string constant folding opportunity in the IR.

---

### Pattern G — `return;` at end of subroutine

**Generated code:**

```perl
sub get_file_size {
    my ($file) = @_;
    my $file = $_[0];
    ...
    say "File $file has $size bytes";
    return;
}
```

**Preferred idiomatic Perl:**

```perl
sub get_file_size {
    my ($file) = shift;
    ...
    say "File $file has $size bytes";
}
```

**IR-fixable?** Yes. The IR has `IrStmt::Return(None)`. The backend `ir_to_perl()` can simply omit the `return;` statement when it is the last statement in a subroutine and is guaranteed to return `undef` (or the value of the preceding statement). This is a style rule in the pretty-printer: "If `Return(None)` is final in a sub, skip it."

The double parameter binding (`my ($file) = @_; my $file = $_[0];`) is a **generator bug** — the generator emits both forms. This requires a generator-level fix to choose one style and stick with it.

---

### Pattern H — `$current_user = ('root');` with extra parens

**Generated code:**

```perl
$current_user = ('root');
```

**Preferred idiomatic Perl:**

```perl
$current_user = 'root';
```

**IR-fixable?** Yes. The parens come from how assignment expressions are wrapped. The IR has `IrStmt::Assign { targets: [...], expr: IrExpr::Str("root", SingleQuoted) }`. The backend `ir_to_perl()` should not emit parentheses around a simple scalar expression in assignment context. This is a trivial style rule in the pretty-printer.

---

### Pattern I — `perl -e` captured via `eval` + `capture_stdout` instead of `qx`

**Generated code:**

```perl
my $perl_result = do {
    my $result;
    my $eval_success = eval {
        $result = capture_stdout( sub { print "Hello from Perl\n" } );
        1;
    };
    if ( !$eval_success ) {
        $result = "Error executing Perl code: $EVAL_ERROR";
    }
    $result;
};
```

**Preferred idiomatic Perl:**

```perl
my $perl_result = qx{perl -e 'print "Hello from Perl\n"'};
chomp $perl_result;
```

**IR-fixable?** Yes. The `perl -e` command is recognized as a special case and the generator tries to inline it by running the Perl code directly via `capture_stdout`. But for a simple `perl -e 'print ...'`, using `qx` is cleaner. The IR has `IrExpr::Backtick { expr: ..., native: false }` with the command as a string. The backend can decide: if the command is `perl -e` with a simple string, it can still emit `qx{...}`. The decision to inline should be made by the backend, not hardcoded in the generator.

---

### Pattern J — Triple-nested `do { do { do { ... } }; };` for simple operations

This is pervasive. Every backtick assignment gets wrapped in `do { do { do { ... } }; };`. For example, the `$nested_result` code has:

```
"Three wells: " . (do {
    do { my $output_0 = q{};
```

**IR-fixable?** Yes. The extra `do` blocks come from the generator emitting `IrStmt::RawText(...)` for intermediate results that are then wrapped in `do { ... }` at the expression level. When the generator produces proper IR nodes (not `RawText`), the backend can emit a single `do { ... }` or none at all if the expression is simple. The IR node pattern is `IrExpr::Backtick` containing a `Pipeline` — currently the pipeline generator returns a block of text that gets placed inside a `do { }` expression. The fix is in `ir_to_perl()`: when a `Backtick` expression would produce a trivial `qx{...}`, emit it directly without wrapping.

---

### Pattern K — `comm -23 <(sort ...) <(sort ...)` via bash subprocess

**Generated code:**

```perl
my $process_result = do { my @_qx_cmd = ("bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
```

**Preferred idiomatic Perl:**

```perl
my $process_result = `bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'`;
chomp $process_result;
```

Or, for native Perl:

```perl
use File::Temp;
my @file1 = sort { $a cmp $b } do { open my $f, '<', 'file1.txt'; chomp(my @l = <$f>); @l };
my @file2 = sort { $a cmp $b } do { open my $f, '<', 'file2.txt'; chomp(my @l = <$f>); @l };
my %seen; $seen{$_}++ for @file1, @file2;
my @process_result = grep { $seen{$_} == 1 && (grep { $_ eq $_ } @file1) } sort keys %seen;
```

**IR-fixable?** This one is already close to idiomatic — the `do { my @_qx_cmd = (...); ... }` wrapper is unnecessary but the core is `qx{...}`. The `@_qx_cmd` indirection and `$? >> 8` can be removed by the backend. The IR node is `IrExpr::Backtick { expr: ..., native: false }`. The backend should emit `qx{...}` directly, setting `$CHILD_ERROR` only if the caller references it. This is a backend pretty-printing choice.

---

### Pattern L — `tr` here-string via `do { my $input_data = ...; ... }` with trailing `;;`

**Generated code:**

```perl
my $here_string_result = do { my $input_data = "hello world"; my $set1_5 = 'a-z';
my $set2_5 = 'A-Z';
my $input_5 = $input_data;;   # <-- note double semicolon
say "Here string result: $here_string_result";
```

**Preferred idiomatic Perl:**

```perl
my $here_string_result = "hello world" =~ tr/a-z/A-Z/r;
```

**IR-fixable?** The `tr` command with a here-string is being expanded into a pipeline infrastructure when it could be a single `tr///r` expression. The IR has `Pipeline { stages: [tr] }` with a here-string redirect. An IR optimization pass could recognize this pattern and lower it to `IrExpr::Call { func: "tr", ... }` or directly to `IrExpr::RawExpr("\"hello world\" =~ tr/a-z/A-Z/r")`. This is a generator-level recognition that should produce a higher-level IR node rather than a full pipeline.

**The double semicolon (`;;`)** is a bug in the generator — it's not an idiom issue but a code quality issue.

---

### Summary Table

| # | Pattern | Shell line(s) | IR node(s) | IR-fixable? | How |
|---|---------|--------------|------------|-------------|-----|
| **A** | Pipeline boilerplate for `ls \| wc -l` | `ls -1 \| wc -l` | `Pipeline`, `Backtick` | ✅ Yes | Emit `qx{...}` instead of 30-line scaffold |
| **B** | Nested backtick `yes \| head -3` expanded to while-loop | `` `echo "Three wells: \`yes well \| head -3\`"` `` | `Backtick`, nested `Pipeline` | ✅ Yes | Fall back to `qx{...}` when emulated commands are nested |
| **C** | Triple `my $files; my @files; my %files` | `files=(...)` | `Declare` ×3 | ⚠️ Partial | Dead-code elimination in IR can remove unused sigils, but root cause needs generator fix |
| **D** | Echo > file via STDOUT save/restore | `echo -e "..." > file1.txt` | `Output`, `Redirect` | ✅ Yes | Emit `say $fh ...` with direct open |
| **E** | `wc -c < "$file"` → slurp+length | `` `wc -c < "$file"` `` | `Pipeline` → `Backtick` | ✅ Yes | Emit `-s $file` when pattern is recognized |
| **F** | `case` literals → regex `=~ /^...$/msx` | `case $system_name in Linux)` | `If` with regex `Cond` | ✅ Yes | Use `eq` for non-glob patterns |
| **G** | `return;` at sub end | implicit end of function | `Return(None)` | ✅ Yes | Omit trailing `return;` in backend |
| **H** | `('root')` extra parens | `` `echo root` `` | `Assign` with `Str` | ✅ Yes | Don't parenthesize scalars in assignment |
| **I** | `perl -e` via eval+capture_stdout | `` `perl -e 'print ...'` `` | `Backtick` | ✅ Yes | Emit `qx{perl -e ...}` when no complex features needed |
| **J** | Triple-nested `do { do { do { ... } }; };` | all backtick assignments | nested `Backtick` + `Pipeline` | ✅ Yes | Flatten to single `do { }` or none |
| **K** | `comm` via `@_qx_cmd` indirection | `` `comm -23 <(...) <(...)` `` | `Backtick` | ✅ Yes | Emit `qx{...}` directly, no array wrapper |
| **L** | `tr` here-string via pipeline scaffold | `` `tr 'a-z' 'A-Z' <<< "hello world"` `` | `Pipeline` with `Redirect` | ✅ Yes | Emit `=~ tr///r` directly |

---

### Unnecessarily verbose translations (prime IR simplification targets)

These are the most egregious cases where a 1–2 line Perl idiom is replaced by 10–50 lines of scaffolding:

1. **`ls -1 | wc -l`** → **30 lines** of `do { do { do { my $output_1 = q{}; ... } } }` with `ls` emulation and `wc` emulation. Should be `qx{ls -1 | wc -l}` (1 line).

2. **`echo -e "apple\nbanana\ncherry" > file1.txt`** → **16 lines** of STDOUT save/restore. Should be `say { filehandle }` (3 lines).

3. **`wc -c < "$file"`** → **18 lines** of file-open, slurp, length. Should be `-s $file` (1 line).

4. **`tr 'a-z' 'A-Z' <<< "hello world"`** → **4 lines** of do-block with temp variables. Should be `$str =~ tr/a-z/A-Z/r` (1 line).

5. **`perl -e 'print "Hello from Perl\n"'`** → **10 lines** of eval/capture_stdout with error handling. Should be `qx{perl -e 'print "Hello from Perl\n"'}` (1 line).

6. The **`yes well | head -3`** nested inside the outer backtick expands to a 12-line while-loop. Should be `qx{yes well | head -3}` (1 line).

Each of these cases shares a common root cause: the generator lowers each shell command into its Perl emulation (ls, wc, head, yes, tr) and then wraps the whole thing in pipeline infrastructure. The IR backend cannot currently see that these emulations, when composed together, produce far more code than a simple `qx{...}` callout. The fix is to give the backend a heuristic: if the expression tree has more than N nodes or the pipeline has more than one emulated command, collapse to `qx{...}` with the original shell text. This heuristic belongs in `ir_to_perl()` when processing `IrExpr::Backtick` nodes.

---

### Structural issue (not just idiom)

The generated code has **mismatched braces** — the `say "Nested backticks: $nested_result";` statement appears inside the `do` block that computes `$nested_result`, which means it's part of the expression for `$nested_result` rather than a subsequent statement. This is a **generator bug** in how nested backtick results are spliced into the surrounding code, not an IR pretty-printing issue. The generator is emitting raw text into the wrong position in the output. Fixing this requires correcting the generator's AST-walk order so that statements following a backtick assignment are placed *after* the backtick expression closes.