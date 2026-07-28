Let me start by examining the relevant files.Let me also look at how the generated Perl was saved or what the actual output file is.Let me run the translator to get clean generated output:Let me get the full generated output more cleanly:Now I have all the pieces. Let me examine the IR design more closely and also check for any other relevant aspects of the generator:Let me get the full generated code cleanly and also look at a few more details:Now let me also look at how the generator currently works to understand the architecture better:Now I have all the information needed. Let me write the comprehensive review.

---

# Idiom Review: Shell-to-Perl Translation of `000__04h_complex_examples.sh`

## 1. Original Shell Script (abridged to key patterns)

```bash
#!/bin/bash
echo "=== Complex Backtick Examples ==="

# Nested backticks
nested_result=`echo "Three wells: \`yes well | head -3\`"`

# Backticks in arithmetic
count=`ls -1 | wc -l`

# Backticks in conditional
current_user=`echo root`
if [ "$current_user" = "root" ]; then ...

# Backticks in function
get_file_size() {
    local file=$1
    local size=`wc -c < "$file"`
}

# Backticks in array
files=(`ls -1 *.sh examples/*.sh 2>/dev/null`)

# Backticks with process substitution
process_result=`comm -23 <(sort file1.txt) <(sort file2.txt)`

# Backticks with here strings
here_string_result=`tr 'a-z' 'A-Z' <<< "hello world"`

# perl command
perl_result=`perl -e 'print "Hello from Perl\n"'`
```

## 2. Generated Perl Code

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);
sub capture_stdout {
    my ($code) = @_;
    my $captured = q{};
    {
        local *STDOUT;
        open STDOUT, '>', \$captured
          or die "Cannot capture stdout: $OS_ERROR\n";
        $code->();
    }
    return $captured;
}

my $output         = q{};
our $CHILD_ERROR;
my $current_user;

say "=== Complex Backtick Examples ===";

# ── Nested backtick expansion (severely broken) ──
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
    # <<< BRACE BUG: subsequent top-level statements
    #     appear INSIDE this while-else branch >>>

say "Nested backticks: $nested_result";

# ── ls | wc pipeline reimplemented in Perl ──
my $count = do {
    do { do {
    my $output_1 = q{};
    my $pipeline_success_1 = 1;
    $output_1 = do {
        my @ls_files_2 = ();
        if ( -f q{.} ) { ... } elsif ( -d q{.} ) {
            opendir my $dh, q{.};
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                push @ls_files_2, $file;
            }
            closedir $dh;
            @ls_files_2 = map { $_->[0] }
                sort { $a->[1] cmp $b->[1] }
                map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] }
                @ls_files_2;
        }
        (@ls_files_2 ? join("\n", @ls_files_2) . "\n" : q{});
    };
    if ($CHILD_ERROR != 0) { $pipeline_success_1 = 0; }
    $output_1 = do {
        my $_wc_data = $output_1;
        my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
        my $_wc_result = sprintf("%d \n", $_wc_lines);
        $_wc_result;
    };
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    chomp $output_1; $output_1;
}; }; };

say "File count: $count";

# ── Simple assignment wrapped in parens ──
$current_user = ('root');

# ── case → if/elsif chain ──
my $system_name = 'Darwin';
if ($system_name =~ /^Linux$/msx) {
    say "Running on Linux";
} elsif ($system_name =~ /^Darwin$/msx) {
    say "Running on macOS";
} elsif (1) {
    say "Running on other " . "sys" . "tem";   # weird concatenation
}

# ── wc -c reimplemented as file read ──
sub get_file_size {
    my ($file) = @_;
    my $file = $_[0];           # BUG: shadows parameter
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
            } else { warn "Cannot open $wc_file: $OS_ERROR\n"; }
            $result;
        };
        $wc_file_opened ? do { my $wc_bytes = length($content); $wc_bytes; } : q{};
    };
    say "File $file has $size bytes";
    return;
}

# ── Triple declaration ──
my $files;
my @files = (do {
    my $_result = `ls -1 *.sh examples/*.sh 2>/dev/null`;
    chomp $_result; $CHILD_ERROR = $? >> 8; split("\n", $_result);
});
my %files;
my $file;
for my $file (@files) { say "  - $file"; }

# ── Redirect via STDOUT manipulation ──
do {
    open my $original_stdout, '>&', STDOUT or die "...";
    open STDOUT, '>', 'file1.txt' or die "...";
    my $tmp = do { say "apple\nbanana\ncherry"; };
    print $tmp;
    open STDOUT, '>&', $original_stdout or die "...";
    close $original_stdout or die "...";
};
# ... (identical block for file2.txt)

# ── Process substitution falls back to bash -c ──
my $process_result = do {
    my @_qx_cmd = ("bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'");
    chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result;
};

# ── Here string translation (incomplete) ──
my $here_string_result = do {
    my $input_data = "hello world";
    my $set1_5 = 'a-z';
    my $set2_5 = 'A-Z';
    my $input_5 = $input_data;;   # double semicolon, then tail of code lost
    # ... structure collapses into braces

# ── perl command uses capture_stdout ──
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

---

## 3. Non-Idiomatic Patterns and IR Fixability Analysis

### Pattern A — Inline expansion of shell commands instead of `qx{}`

**Location:** Lines for `count` (ls | wc), `get_file_size` (wc -c)

**Generated:** ~50 lines of Perl reimplementing `ls` (opendir/readdir/filter/sort), `wc -l` (newline counting), and `wc -c` (read entire file, take `length`).

**Preferred idiomatic Perl:**
```perl
my $count  = `ls -1 | wc -l`;
chomp $count;

my $size   = -s $file;                       # wc -c
```

**IR-fixable?** ❌ **Not fixable in the IR pretty-printer.** The generator *chooses* to expand these commands into Perl logic rather than emit an `IrStmt::System { capture: Some(...) }` node (which would become `qx{...}`). This is a generator-level strategy decision: the command-specific generators (e.g. `commands/ls.rs`, `commands/wc.rs`) produce expanded Perl code. To fix this, those generators would need to either fall through to a backtick-based system call, or emit a new high-level IR node like `IrExpr::FileSize(String)` or `IrExpr::LineCount(String)`.

**Why this happens:** Each command module (e.g. `ls`, `wc`) has its own `generate_*_impl` function that produces Perl code to simulate the command's behaviour. The pipeline generator (`pipeline_commands.rs`) connects these. There is no "fall back to `qx{}` for unknown/complex commands" strategy in the pipeline path.

---

### Pattern B — Nested backtick expansion producing malformed code

**Location:** `nested_result` assignment

**Generated:** A `while (1)` loop inside a `do` block that simulates `yes well | head -3`, but with broken brace nesting that causes subsequent top-level code to appear inside the loop body.

**Preferred idiomatic Perl:**
```perl
my $nested_result = `echo "Three wells: \`yes well | head -3\`"`;
chomp $nested_result;
```

Perl's `qx{}` natively supports nested backticks with backslash escaping.

**IR-fixable?** ❌ **Not fixable in the pretty-printer.** The generator recursively descends into nested backtick substitutions and tries to expand them inline. It should instead recognize that the outer backtick is a command substitution and emit `IrStmt::System { capture: Some("nested_result"), cmd: "echo \"Three wells: \\`yes well | head -3\\`\"" }`. The pretty-printer would then just produce `my $nested_result = qx{...};`.

**Additionally**, the brace-balancing hack at the end of `Generator::generate()` (lines counting `{` and `}` and appending extra `}`) is a symptom of the structural issue. It tries to fix broken nesting after the fact, which cannot produce correct code.

---

### Pattern C — Pipeline infrastructure for trivial pipelines

**Location:** `count` assignment

**Generated:**
```perl
my $count = do {
    do { do {
    my $output_1 = q{};
    my $pipeline_success_1 = 1;
    $output_1 = do { ... ls simulation ... };
    ;
    if ($CHILD_ERROR != 0) { $pipeline_success_1 = 0; }
    $output_1 = do { ... wc simulation ... };
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    chomp $output_1;
    $output_1;
}; }; };
```

Three levels of `do { }` nesting, pipeline success tracking, `CHILD_ERROR` checks — all for a two-command pipeline that in idiomatic Perl is just:
```perl
my $count = `ls -1 | wc -l`;
chomp $count;
```

**IR-fixable?** ✅ **Fixable at the IR level**, *if* the generator emits an `IrStmt::Pipeline` node with semantic awareness that the pipeline is used for capture. The `ir_to_perl()` function for pipeline could then detect a capture pipeline and emit:
```perl
my $output_1 = qx{ls -1 | wc -l};
chomp $output_1;
$output_1;
```
But the current IR design has no `Pipeline` variant that distinguishes "running for side effects" from "running for output capture". The existing `IrStmt::Pipeline` would need a `capture: Option<String>` field. Adding that would let the backend emit a clean `qx{}` call.

**Alternatively**, if the generator simply emitted `IrStmt::System { capture: Some("count"), cmd: "ls -1 | wc -l" }` directly (skipping pipeline expansion), the backend would produce idiomatic code immediately.

---

### Pattern D — Unnecessary `do {}` wrapping around simple expressions

**Location:** Several places:
- `$current_user = ('root');` — the outer parens and implied `do` context
- `my $size = do { my $wc_file = "$file"; ... }` — entire file-read logic wrapped in `do`
- `my $process_result = do { my @_qx_cmd = (...); ... }` — `do { }` around a simple qx call

**Preferred:**
```perl
$current_user = 'root';
my $size = -s $file;
my $process_result = `comm -23 <(sort file1.txt) <(sort file2.txt)`;
```

**IR-fixable?** ✅ **Partially fixable in the IR pretty-printer.** If the generator emits `IrStmt::Assign { targets: [...], expr: IrExpr::Str("root") }`, the backend would output `$current_user = 'root';` without wrapping. The `do { }` wrapper appears because the current generator emits raw text that already contains `do { ... }` — i.e., it's a `RawText` bridge issue. Migrating to semantic IR nodes would eliminate the wrapper. However, for the `size` case, the *content* inside the `do` is also non-idiomatic (see pattern A).

---

### Pattern E — STDOUT redirection via filehandle manipulation

**Location:** Both `echo ... > file1.txt` / `echo ... > file2.txt` blocks

**Generated (~15 lines each):**
```perl
do {
    open my $original_stdout, '>&', STDOUT or die "...";
    open STDOUT, '>', 'file1.txt' or die "...";
    my $tmp = do { say "apple\nbanana\ncherry"; };
    print $tmp;
    open STDOUT, '>&', $original_stdout or die "...";
    close $original_stdout or die "...";
};
```

**Preferred idiomatic Perl:**
```perl
use File::Slurp qw(write_file);
write_file('file1.txt', "apple\nbanana\ncherry\n");
```
or (no extra module):
```perl
{
    open my $fh, '>', 'file1.txt' or die "Cannot open 'file1.txt': $!\n";
    print $fh "apple\nbanana\ncherry\n";
    close $fh;
}
```

**IR-fixable?** ✅ **Fixable with a new IR node.** The IR currently has no concept of "redirect output to file". Adding an `IrStmt::WriteFile { target: IrExpr, content: IrExpr }` would let the backend emit the clean filehandle version above. The generator's redirect module (`src/generator/redirects.rs`) currently emits STDOUT-manipulation code. If it instead emitted an `IrStmt::WriteFile` node, the pretty-printer could generate idiomatic Perl.

**Level of effort:** Medium. Requires adding a new IR variant and changing the redirect generator.

---

### Pattern F — Triple declaration of same variable name

**Location:** `files` array assignment

**Generated:**
```perl
my $files;       # never used
my @files = (...);
my %files;       # never used
my $file;        # redundant — for my $file (@files) declares it
for my $file (@files) { ... }
```

**Preferred:**
```perl
my @files = (do { ... });
for my $file (@files) { ... }
```

**IR-fixable?** ✅ **Fixable via IR optimization passes.** The IR could have:
1. **Dead code elimination** — remove `my $files;` and `my %files;` if they are never referenced.
2. **Redundant declaration removal** — remove `my $file;` when immediately followed by `for my $file`.

These are *IR-to-IR transformation passes* (MIR), not pretty-printer changes. The IR design doc mentions "Dead assignment elimination" as a planned optimization.

**Generator root cause:** The `generate_assignment` method in `mod.rs` (around line 600) explicitly emits all three sigils for array assignments:
```rust
output.push_str(&format!("my ${};\n", name));
output.push_str(&format!("my @{} = ({});\n", name, elements_perl.join(", ")));
output.push_str(&format!("my %{};\n", name));
```
This is a generator-level decision that would need to change. The triple declaration was presumably added to satisfy `perlcritic`'s `ProhibitImplicitNames` policy, but it's over-broad.

---

### Pattern G — `wc -c` expanded to file read + length

**Location:** `get_file_size` function

**Generated (~20 lines):**
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
        } else { warn "Cannot open $wc_file: $OS_ERROR\n"; }
        $result;
    };
    $wc_file_opened ? do { my $wc_bytes = length($content); $wc_bytes; } : q{};
};
```

**Preferred idiomatic Perl (4 chars):**
```perl
my $size = -s $file;
```

**IR-fixable?** ❌ **Not fixable in the pretty-printer.** The `wc` command generator (`src/generator/commands/wc.rs`) intentionally expands this to Perl file I/O. To get the idiomatic `-s` operator, either:
1. The `wc` generator must recognize the `-c` flag and emit a new IR node like `IrExpr::FileSize(String)`.
2. Or the generator should fall back to `qx{wc -c < "$file"}`.

This is a design choice in the generator. The IR doesn't currently have a `FileSize` expression node.

---

### Pattern H — `perl -e` captured via `capture_stdout` + `eval` instead of backtick

**Location:** `perl_result` assignment

**Generated:**
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
my $perl_result = `perl -e 'print "Hello from Perl\n"'`;
chomp $perl_result;
```

**IR-fixable?** ✅ **Fixable if the generator emits `IrStmt::System`.** The special-case handling for `perl` commands (likely in `src/generator/commands/perl.rs`) currently translates `perl -e '...'` into inline Perl code using `capture_stdout`. If instead it emitted `IrStmt::System { capture: Some("perl_result"), cmd: "perl -e 'print \"Hello from Perl\\n\"'"}`, the backend would produce a clean `qx{}` call. The `capture_stdout` subroutine would become unnecessary and could be omitted from the import list.

---

### Pattern I — `case` translated to if/elsif with regex anchoring and `msx` flags

**Location:** `system_name` case statement

**Generated:**
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
use feature 'switch';  # or just if/elsif
given ($system_name) {
    when (/^Linux$/)   { say "Running on Linux"; }
    when (/^Darwin$/)  { say "Running on macOS"; }
    default            { say "Running on other system"; }
}
```
Or keep if/elsif but drop the `msx` flags and `$` anchor (not needed for simple equality):
```perl
if ($system_name eq 'Linux') {
    say "Running on Linux";
} elsif ($system_name eq 'Darwin') {
    say "Running on macOS";
} else {
    say "Running on other system";
}
```

**IR-fixable?** ✅ **Partially.** The `msx` flags and regex anchoring are pretty-printer decisions — the IR node is `If { cond: MatchExpr { ... } }`. The backend could choose to emit `eq` for simple patterns like `/^Linux$/`. However, the `"sys" . "tem"` concatenation is a generator bug: the `*` wildcard pattern text `"Running on other system"` is being mangled. The string `"system"` appears to be split because `'sys'` and `'tem'` are substrings of `"case", "esac"` or some other parsing artifact. This would need generator debugging.

**Verdict:** The if/elsif structure is acceptable Perl. The `msx` flags can be cleaned in the backend. The `"sys" . "tem"` bug is a generator issue, not IR-fixable.

---

### Pattern J — Here string translation (broken)

**Location:** `here_string_result`

**Generated (appears incomplete):**
```perl
my $here_string_result = do {
    my $input_data = "hello world";
    my $set1_5 = 'a-z';
    my $set2_5 = 'A-Z';
    my $input_5 = $input_data;;
    # ... structure never completes properly
```

**Preferred:**
```perl
my $here_string_result = `tr 'a-z' 'A-Z' <<< "hello world"`;
chomp $here_string_result;
```

**IR-fixable?** ❌ **Not fixable in the pretty-printer.** The `tr` command generator tries to inline the `tr` operation in Perl (to avoid forking) but fails to handle the here-string input properly. The generator should fall back to `qx{tr 'a-z' 'A-Z' <<< "hello world"}` when it can't produce a clean inline translation.

---

### Pattern K — Magic brace-balancing pass

**Location:** End of `Generator::generate()`

**Generated code:** At the end of the output, 6 closing braces `}}}}} }` are appended by the brace-balancing hack:
```rust
let opens = output.chars().filter(|&c| c == '{').count();
let closes = output.chars().filter(|&c| c == '}').count();
for _ in 0..(opens.saturating_sub(closes)) {
    output.push_str("}\n");
}
```

**Problem:** This is a post-hoc patch that treats the symptom, not the cause. It cannot produce correctly structured code if the generator emits statements at wrong nesting levels.

**IR-fixable?** ✅ **Fixable by eliminating the need for it.** With proper IR nodes and structured emission, the `ir_to_perl()` backend naturally produces balanced braces because it iterates statement lists and adds `{ }` based on semantic structure, not string counting. The brace-balancing hack should be removed once all generators produce proper IR.

---

## 4. Unnecessarily Verbose Translations (Prime IR Candidates)

These are translations where a simple shell operation is wrapped in layers of control structure that could be collapsed:

| Original Shell | Generated (approx lines) | Ideal Perl | Verbosity Ratio |
|---|---|---|---|
| `` count=`ls -1 \| wc -l` ``  | ~35 lines (ls reimpl + wc reimpl + pipeline machinery) | `my $count = \`ls -1 \| wc -l\``; | **35×** |
| `` size=`wc -c < "$file"` `` | ~20 lines (open/read/length + error handling) | `my $size = -s $file;` | **20×** |
| `echo ... > file1.txt` | ~15 lines (STDOUT save/redirect/restore) | `write_file('file1.txt', ...)` | **15×** |
| `` nested_result=`echo "...\`...\`"` `` | ~15 lines (while loop + broken nesting) | `` my \$r = \`echo "...\`...\`"\` `` | **15×** |
| `` here_string_result=`tr ... <<< "..."` `` | ~8 lines (incomplete, broken) | `` my \$r = \`tr ... <<< "..."\` `` | **8×** |
| `` perl_result=`perl -e '...'` `` | ~12 lines (capture_stdout + eval + error handling) | `` my \$r = \`perl -e '...'\` `` | **12×** |

## 5. Summary Table

| Pattern | IR-Fixable? | How / Which IR Node | Clean Output |
|---|---|---|---|
| **A** Inline command expansion (ls, wc) | ❌ Generator strategy | — | `my $count = \`ls -1 \| wc -l\``; |
| **B** Nested backtick expansion | ❌ Generator strategy | — | `my $nested_result = \`echo "...\\\`...\\\`"\``; |
| **C** Pipeline infrastructure | ✅ With `Pipeline.capture` field | `IrStmt::Pipeline { capture: Some("var") }` | `my $var = qx{ls -1 \| wc -l};` |
| **D** Unnecessary `do {}` wrapping | ✅ Via semantic nodes | `IrStmt::Assign { expr: IrExpr::Str }` | `$current_user = 'root';` |
| **E** STDOUT redirect simulation | ✅ New `IrStmt::WriteFile` | `IrStmt::WriteFile { target, content }` | `{ open $fh, '>', $f; print $fh ... }` |
| **F** Triple variable declarations | ✅ Dead code elimination pass | IR-to-IR MIR transform | `my @files = (...);` |
| **G** `wc -c` → open+read+length | ❌ Generator strategy | — | `my $size = -s $file;` |
| **H** `perl -e` → capture_stdout+eval | ✅ Via `IrStmt::System` | `IrStmt::System { capture: Some("var") }` | `my $var = \`perl -e '...'\``; |
| **I** `case` → if/elsif with `msx` | ✅ Partially (flags + anchoring) | `IrStmt::If { cond: MatchExpr }` | `if ($x eq 'foo') { ... }` |
| **J** Here-string translation | ❌ Generator strategy | — | `my $r = \`tr ... <<< "..."\``; |
| **K** Brace-balancing hack | ✅ Eliminated by IR | Structured emission | Balanced braces naturally |

## 6. Key Recommendations

1. **Add a `capture` field to `IrStmt::Pipeline`.** This lets the backend detect capture pipelines (e.g. the output of `ls | wc` used in a substitution) and emit a single `qx{}` call instead of simulating the pipeline in Perl.

2. **Add `IrStmt::WriteFile` and `IrStmt::AppendFile`.** These capture shell redirects (`> file`, `>> file`) as semantic nodes rather than the current STDOUT-manipulation approach.

3. **Make command generators fall back to `IrStmt::System`.** Instead of every command module (ls, wc, tr, etc.) producing inline Perl expansions, they should check complexity thresholds and fall back to `qx{original_command}` for anything non-trivial. Only trivially simple commands (echo, printf) should be inlined.

4. **Add dead code elimination as an IR-to-IR pass.** This would clean up triple declarations, unused variables, and unreachable code without changing any generator function.

5. **Stop recursively expanding nested backticks.** The backtick handler should produce `IrStmt::System` nodes with the literal command string (properly escaped), letting Perl's native `qx{}` handle nested backticks.

6. **Remove the brace-balancing hack.** It masks structural bugs and can never produce correct code. Instead, ensure every code path produces balanced braces by emitting IR nodes.

7. **Change the `case` statement generator** to emit simple `eq` comparisons for literal patterns instead of regex matches with `msx` flags. The regex engine is overkill for string equality.