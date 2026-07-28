Now I have a thorough understanding of both the generated code and the generator internals. Let me compose the review.

---

## Idiom Review: `examples/000__02_output_formatting_commands.sh`

### 1. Original Shell Script vs. Generated Perl

**Original Shell (excerpts):**
```bash
echo_result=`echo "Hello from backticks"`
echo "Echo result: $echo_result"

printf_result=`printf "Number: %d, String: %s\n" 42 "test"`
echo "Printf result: $printf_result"

echo "test content" > test_checksum.txt
sha256_result=`sha256sum test_checksum.txt`
strings_result=`strings test_binary.txt | head -3`

tee_result=`echo "test output" | tee test_tee.txt`

perl_result=`perl -e 'print "Hello from Perl\n"'`
echo "Perl result: $perl_result"
rm -f test_checksum.txt test_tee.txt
```

**Generated Perl (full):**
```perl
#!/usr/bin/env perl
use strict; use warnings; use Carp; use English qw(...); use locale;
use IPC::Open3; use Digest::SHA qw(sha256_hex sha512_hex);
use File::Path qw(make_path remove_tree);

sub capture_stdout { ... }  # helper for `perl -e ...`

my $main_exit_code = 0;  my $ls_success = 0;  my $__set_e = 0;  my $output = q{};
our $CHILD_ERROR;

# -- echo_result=`echo "Hello from backticks"`
my $echo_result; my @echo_result; my %echo_result;
$echo_result = ("Hello from backticks");
do {
    my $__echo_line = "Echo result: $echo_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) { print "\n"; $__echo_line .= "\n"; }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;

# -- printf_result=`printf "Number: %d, String: %s\n" 42 "test"`
my $printf_result; my @printf_result; my %printf_result;
$printf_result = sprintf("Number: %d, String: %s\n", '42', "test");;
do {
    ...same echo pattern...
};
$CHILD_ERROR = 0;

# -- echo "test content" > test_checksum.txt
do {
    open my $original_stdout, '>&', STDOUT or die ...;
    open STDOUT, '>', 'test_checksum.txt' or die ...;
    print "test content\n";
    open STDOUT, '>&', $original_stdout or die ...;
    close $original_stdout or die ...;
};

# -- sha256_result=`sha256sum test_checksum.txt`
my $sha256_result; my @sha256_result; my %sha256_result;
$sha256_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha256_hex(do {
            local $INPUT_RECORD_SEPARATOR = undef;
            open my $fh, '<', 'test_checksum.txt' or croak ...;
            my $content = <$fh>; close $fh or croak ...;
            $content;
        });
        push @results, "$hash  test_checksum.txt";
    } else {
        push @results, "000000000000...  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
...same echo pattern for SHA256 result...

# -- sha512_result=`sha512sum test_checksum.txt`
...identical do-block structure with 128 zero hex digits...

# -- strings_result=`strings test_binary.txt | head -3`
my $strings_result; my @strings_result; my %strings_result;
$strings_result = do {
    local $CHILD_ERROR = 0;
    my $_pipeline_result = do {
        my $output_0 = q{};
        my $output_printed_0;
        my $pipeline_success_0 = 1;
        my $input_data;
        if ( open my $fh, '<', 'test_binary.txt' ) {
            local $INPUT_RECORD_SEPARATOR = undef;
            $input_data = <$fh>; close $fh or croak ...;
        } else {
            print {*STDERR} "strings: ... No such file\n"; $input_data = q{};
        }
        my @result;
        while ($input_data =~ /([\x20-\x7E]{4,})/g) { push @result, $1; }
        my $line = join "\n", @result;
        if ($line ne q{} && !($line =~ m{\n\z}msx)) { $line .= "\n"; }
        $output_0 = $line;
        if ($CHILD_ERROR != 0) { $pipeline_success_0 = 0; }
        my $num_lines = 3;
        my $head_line_count = 0;
        my $result = q{};
        my $input = $output_0;
        my $pos = 0;
        while ( $pos < length $input && $head_line_count < $num_lines ) {
            my $line_end = index $input, "\n", $pos;
            if ( $line_end == -1 ) { $line_end = length $input; }
            my $head_line = substr $input, $pos, $line_end - $pos;
            $result .= $head_line . "\n";
            $pos = $line_end + 1;
            ++$head_line_count;
        }
        $output_0 = $result;
        if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
        $output_0 =~ s/\n+\z//msx;
        $output_0;
    };
    $_pipeline_result;
};

# -- tee_result=`echo "test output" | tee test_tee.txt`
my $tee_result; my @tee_result; my %tee_result;
$tee_result = do {
    local $CHILD_ERROR = 0;
    my $_pipeline_result = do {
        my $output_1 = q{};
        ... similar pipeline infrastructure ...
        $output_1 .= 'test output' . "\n";
        if ( !($output_1 =~ m{\n\z}msx) ) { $output_1 .= "\n"; }
        $CHILD_ERROR = 0;
        if ($CHILD_ERROR != 0) { $pipeline_success_1 = 0; }
        use Carp qw(carp croak);
        if ( open my $fh, '>', 'test_tee.txt' ) {
            print {$fh} $output_1;
            close $fh or croak "Close failed: $ERRNO";
        } else { carp "tee: Cannot open ..."; }
        $output_1 = $output_1;
        if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
        $output_1 =~ s/\n+\z//msx;
        $output_1;
    };
    $_pipeline_result;
};

# -- perl_result=`perl -e 'print "Hello from Perl\n"'`
my $perl_result; my @perl_result; my %perl_result;
$perl_result = do {
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

# -- rm -f test_checksum.txt test_tee.txt
if ( -e "test_checksum.txt" ) {
    if ( -d "test_checksum.txt" ) {
        carp "rm: carping: ... is a directory ...";
    } else {
        if ( unlink "test_checksum.txt" ) { }
        else { carp "rm: carping: could not remove ..."; }
    }
} else { local $CHILD_ERROR = 0; }
if ( -e "test_tee.txt" ) { ... same pattern ... }
```

---

### 2. Non-Idiomatic Patterns

#### Pattern A: Echo output wrapped in `do { ... }` with manual trailing-newline check

**Generated code (appears 7 times):**
```perl
do {
    my $__echo_line = "Echo result: $echo_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
```

This constructs a local variable `$__echo_line`, prints it, checks if it ends with `\n` via a regex, appends one if not, and also appends to `$output`. It does *four* things that should be one or two.

**Idiomatic Perl:**
```perl
print "Echo result: $echo_result\n";
$output .= "Echo result: $echo_result\n";
```

Or if accumulating `$output` is genuinely needed (it appears unused beyond this file), use a single statement:
```perl
my $line = "Echo result: $echo_result\n";
print $line;
$output .= $line;
```

**IR-fixable?** **Yes.** This corresponds to `IrStmt::Output { value: IrExpr, newline: bool }`. The IR design doc's style table already shows the fix:

| Pattern in IR | Current output (ugly) | Future output (clean) |
|---|---|---|
| `Output { value: Var("x"), newline: true }` | `print $x; if (!($x =~ m{\n\z}msx)) { print "\n"; }` | `say $x;` |

With an `IrStmt::Output` node, the `ir_to_perl()` backend would emit `say "Echo result: $echo_result";` or `print "Echo result: $echo_result\n";`. The `$output .= ...` accumulation is a separate concern — it should be handled by an explicit `IrStmt::Assign` node appended to the output, not embedded in the echo generator.

---

#### Pattern B: Triple-declaration bloat (`$var`, `@var`, `%var` for every scalar)

**Generated code (appears for every variable):**
```perl
my $echo_result;
my @echo_result;
my %echo_result;
```
The code declares all three sigils for every variable, even when only the scalar form is ever used.

**Idiomatic Perl:**
```perl
my $echo_result;
```
Only the sigil that is actually used should be declared.

**IR-fixable?** **Yes.** The IR has `IrStmt::Declare { vars: Vec<Decl>, init: Option<IrExpr> }` where each `Decl { name, sigil }` chooses exactly one sigil. An IR node for `echo_result` from a backtick would be `Declare { vars: [Decl { name: "echo_result", sigil: Scalar }], init: None }`. The backend emits exactly `my $echo_result;`. The triple-declaration is a generator bug — the generator emits all three forms because its pre-analysis can't always determine which sigil will be needed. In the IR, this analysis would happen at the IR-lowering level, and only the needed sigil would be declared.

---

#### Pattern C: Full `sha256sum` / `sha512sum` emulation in a `do` block instead of `qx{}`

**Generated code (sha256sum):**
```perl
$sha256_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha256_hex(do {
            local $INPUT_RECORD_SEPARATOR = undef;
            open my $fh, '<', 'test_checksum.txt' or croak ...;
            my $content = <$fh>; close $fh or croak ...;
            $content;
        });
        push @results, "$hash  test_checksum.txt";
    } else {
        push @results, "000000000000...  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
```

This is a *native Perl* reimplementation of the `sha256sum` command. The shell backtick `sha256sum test_checksum.txt` returns a single line like `d1f7c...  test_checksum.txt`. The generated code replaces the external command with a full Perl-native `sha256_hex` + file read + formatting + error handling.

**Idiomatic Perl:**
```perl
my $sha256_result = qx{sha256sum test_checksum.txt};
chomp $sha256_result if defined $sha256_result;
```

Or, if the goal is to stay native:
```perl
my $sha256_result = sprintf "%s  test_checksum.txt\n", sha256_hex(do {
    local $/; open my $fh, '<', 'test_checksum.txt' or die "$!"; <$fh>
});
```

But the generated version is **unnecessarily verbose** — it wraps a single command in a 20-line `do { ... }` block with error emulation, when `qx{}` would suffice.

**IR-fixable?** **Partially.** The IR node `IrStmt::System { capture: Some("sha256_result") }` can represent `qx{sha256sum test_checksum.txt}`. The current generator chooses to inline the native Perl `sha256_hex` call rather than emit `qx{}`. This is a **generator policy decision** (in `sha256sum.rs`), not an IR formatting issue. To produce `qx{}`, the generator must be changed to emit `IrStmt::System { cmd: ..., capture: Some("sha256_result") }` when the translation mode prefers shell fallback. The IR backend could then pretty-print that as `my $sha256_result = qx{...};`.

However, the *formatting* of the native-perl `do { ... }` block (indentation, line breaks) **is** IR-fixable once the generator emits `IrStmt::Assign { targets, expr: IrExpr::Block(...) }` — the backend controls brace placement and whitespace.

**Verdict:** The choice between native-Perl and `qx{}` is a generator-level decision. The *verbosity* of the `do { my @results; if (...) { ... } else { ... } join ... }` construct for a single file could be simplified by the generator. The IR can only fix the *formatting* of whatever the generator emits.

---

#### Pattern D: Pipeline infrastructure for a single command (`strings | head -3`)

**Generated code (60+ lines for `strings test_binary.txt | head -3`):**
```perl
$strings_result = do {
    local $CHILD_ERROR = 0;
    my $_pipeline_result = do {
        my $output_0 = q{};
        my $output_printed_0;
        my $pipeline_success_0 = 1;
        my $input_data;
        if ( open my $fh, '<', 'test_binary.txt' ) {
            local $INPUT_RECORD_SEPARATOR = undef;
            $input_data = <$fh>; close $fh or croak ...;
        } else { print {*STDERR} "strings: ..."; $input_data = q{}; }
        my @result;
        while ($input_data =~ /([\x20-\x7E]{4,})/g) { push @result, $1; }
        my $line = join "\n", @result;
        if ($line ne q{} && !($line =~ m{\n\z}msx)) { $line .= "\n"; }
        $output_0 = $line;
        if ($CHILD_ERROR != 0) { $pipeline_success_0 = 0; }
        # head -3 implementation:
        my $num_lines = 3;
        my $head_line_count = 0; my $result = q{}; my $input = $output_0; my $pos = 0;
        while ( $pos < length $input && $head_line_count < $num_lines ) {
            my $line_end = index $input, "\n", $pos;
            ... head logic ...
        }
        $output_0 = $result;
        if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
        $output_0 =~ s/\n+\z//msx;
        $output_0;
    };
    $_pipeline_result;
};
```

This is the **most egregious case** of unnecessary verbosity. A two-command pipeline (`strings test_binary.txt | head -3`) generates:
- A `$CHILD_ERROR` wrapper
- A nested `$_pipeline_result` `do` block
- `$output_0`, `$output_printed_0`, `$pipeline_success_0` accounting variables
- File reading with error handling
- Native Perl `strings` emulation (the `while ($input_data =~ /([\x20-\x7E]{4,})/g)` loop)
- A complete `head -3` reimplementation
- `$main_exit_code` tracking

**Idiomatic Perl:**
```perl
my $strings_result = qx{strings test_binary.txt | head -3};
```

Or, if native Perl is desired for portability:
```perl
my $strings_result = do {
    open my $fh, '<', 'test_binary.txt' or die "Cannot open test_binary.txt: $!";
    local $/; my $data = <$fh>; close $fh;
    my @strings = $data =~ /([\x20-\x7E]{4,})/g;
    join "\n", @strings[0..2];
};
```

**IR-fixable?** **No, this is a generator architecture problem.** The pipeline generator (`pipeline_commands.rs`) constructs this infrastructure for *every* pipeline. The IR can only pretty-print whatever `IrStmt::Pipeline { stages, last_output }` nodes it receives. If the generator emits a `Pipeline` with two `RawText` stages, the backend has to render whatever text it gets.

To produce the idiomatic version, the **generator** needs to decide: "this is a simple two-command pipeline that can be replaced by a single `qx{}` call." That's a generator-level optimization, not an IR-level formatting choice.

However, the *design* of the IR addresses this: the IR has `IrStmt::System { capture: Some("out") }` which the backend can render as `my $out = qx{...};`. If the shell→IR lowering phase mapped backtick pipelines directly to `System` nodes (with `cmd` being a single string), the verbose pipeline machinery would be bypassed entirely. The key insight: **the generator currently lower the pipeline to fine-grained Perl statements; an IR-based approach would preserve the pipeline as a single `System` node until the backend.**

---

#### Pattern E: `echo "test output" > test_checksum.txt` via STDOUT redirect

**Generated code:**
```perl
do {
    open my $original_stdout, '>&', STDOUT or die ...;
    open STDOUT, '>', 'test_checksum.txt' or die ...;
    print "test content\n";
    open STDOUT, '>&', $original_stdout or die ...;
    close $original_stdout or die ...;
};
```

The shell redirect (`> test_checksum.txt`) is translated by saving/restoring STDOUT around a `print`.

**Idiomatic Perl:**
```perl
print {*STDOUT} "test content\n";   # already going to STDOUT, or:
system "echo 'test content' > test_checksum.txt";
```

Even simpler — since there's no genuine reason to capture STDOUT here, just:
```perl
my $content = "test content\n";
write_file('test_checksum.txt', $content);   # or use File::Slurper
```

Or staying minimal:
```perl
open my $fh, '>', 'test_checksum.txt' or die "Cannot open: $!";
print $fh "test content\n";
close $fh;
```

**IR-fixable?** **Partially.** The redirect infrastructure is a generator construct. In the IR, a simple redirect `> file` on an output command would be modeled as `IrStmt::Output { value: "test content\n", newline: false }` with a file handle annotation, or as `IrStmt::System { cmd: "echo 'test content' > test_checksum.txt" }`. The IR backend can clean up the *formatting* of whatever node it receives, but the choice to use STDOUT-save/restore vs. direct file open is a generator-level decision.

---

#### Pattern F: `perl -e` backtick wrapping in `capture_stdout` + `eval`

**Generated code:**
```perl
$perl_result = do {
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

This is a backtick command `perl -e 'print "Hello from Perl\n"'` that the generator recognizes as native Perl and inlines. The `capture_stdout` subroutine uses `local *STDOUT; open STDOUT, '>', \$captured` to capture the output of the Perl `print`.

**Idiomatic Perl:**
```perl
my $perl_result = "Hello from Perl\n";
```

Or if the point is that it's a backtick:
```perl
my $perl_result = qx{perl -e 'print "Hello from Perl\n"'};
```

The `eval` + `capture_stdout` wrapper is extreme overkill for a tiny inline Perl one-liner.

**IR-fixable?** **Yes, but the fix is at the generator level.** The IR node `IrStmt::System { capture: Some("perl_result") }` would let the backend render `my $perl_result = qx{perl -e 'print "Hello from Perl\n"'};`. The generator in `simple_commands.rs` recognizes the `perl -e` pattern and chooses to inline it via `capture_stdout`. If it emitted an `IrStmt::System { capture, cmd: IrExpr::Call("perl", ...) }` instead, the backend could produce the clean version.

---

#### Pattern G: `rm` translated to safety-wrapped `unlink`

**Generated code:**
```perl
if ( -e "test_checksum.txt" ) {
    if ( -d "test_checksum.txt" ) {
        carp "rm: carping: ", "test_checksum.txt", " is a directory (use -r to remove recursively)\n";
    } else {
        if ( unlink "test_checksum.txt" ) { }
        else { carp "rm: carping: could not remove ", "test_checksum.txt", ": $OS_ERROR\n"; }
    }
} else { local $CHILD_ERROR = 0; }
```

The generator wraps every `rm` in existence checks, directory guards, and error messages. For `rm -f`, which should silently succeed even if the file doesn't exist, this is absurdly defensive.

**Idiomatic Perl:**
```perl
unlink 'test_checksum.txt';
unlink 'test_tee.txt';
```

Or with `-f` semantics (ignore errors):
```perl
unlink 'test_checksum.txt';
unlink 'test_tee.txt';
# unlink returns the number of files deleted; non-existence is not an error
```

**IR-fixable?** **No.** The generator in `rm.rs` chooses to emit this safety wrapping. IR nodes like `IrStmt::System { cmd: "rm", args: [...] }` could be rendered as `system 'rm', '-f', 'file'`, but the generator currently emits Perl-native `unlink` with guard clauses. The IR would need an `IrStmt::Unlink { files: [...], force: bool }` node, or the generator must change to emit a simpler `system` call.

---

#### Pattern H: Semantic-free `$CHILD_ERROR = 0;` after every echo

**Generated code (appears after every echo block):**
```perl
$CHILD_ERROR = 0;
```

This is inserted because `echo` in shell always succeeds (exit code 0), so the generator models that by resetting `$CHILD_ERROR`. But this variable is *global state* that tracks the last command's exit status for `&&` / `||` chaining. In the generated Perl, it's set to 0 after each echo regardless of whether anything downstream reads it.

**Idiomatic Perl:** Omit `$CHILD_ERROR = 0;` entirely for top-level echo commands where the exit code is never consumed.

**IR-fixable?** **Yes.** An `IrStmt::Output` node implies a successful exit. The IR analysis pass could detect that `$CHILD_ERROR` is never read after this point and perform **dead-assignment elimination**, removing the `$CHILD_ERROR = 0;` assignment. The IR design doc explicitly mentions "Dead assignment elimination" as a planned optimization.

---

#### Pattern I: `$output .= ...` accumulation with no consumer

**Generated code (collects in `$output` from every echo):**
```perl
$output .= $__echo_line;
```
The variable `$output` is declared as `my $output = q{};` at the top, accumulated into by every `echo` block, but **never printed or used** before `exit $main_exit_code`. This is dead code.

**Idiomatic Perl:** Omit `$output` and its accumulation entirely.

**IR-fixable?** **Yes.** A live-variable analysis on the IR would show that `$output` is written but never read before program exit. The dead-code elimination pass would remove `my $output = q{};` and all `$output .= ...;` statements.

---

#### Pattern J: `$main_exit_code` / `$ls_success` / `$__set_e` / `$CHILD_ERROR` overhead

**Generated prologue:**
```perl
my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;
```

These four variables are declared for *every* script regardless of whether they're needed. In this script:
- `$main_exit_code` is only set (from pipeline failures) but the exit code would be 0 anyway
- `$ls_success` is never used
- `$__set_e` is never used
- `$output` is accumulated but never consumed

**Idiomatic Perl:** Only declare what's used:
```perl
use strict; use warnings;
# That's it. No exit code tracking needed for this script.
```

**IR-fixable?** **Yes.** The IR's `IrProgram { imports, stmts, subs }` would only include `Declare` nodes for variables that are actually used. An import-minimization pass would remove `use IPC::Open3;`, `use File::Path;`, and the `capture_stdout` subroutine since they're unused in the final output.

---

### 3. Unnecessarily Verbose Translations (Prime IR Candidates)

These are places where the generated code wraps simple operations in elaborate control structures:

| Shell Line | Generated Code Complexity | Simplification |
|---|---|---|
| `` echo_result=`echo "Hello from backticks` `` | 1 assignment + 12-line echo block | `my $echo_result = "Hello from backticks";` |
| `` printf_result=`printf ...` `` | 1 assignment + 12-line echo block | `my $printf_result = sprintf("Number: %d, String: %s\n", 42, "test");` |
| `echo "test content" > file` | 9-line STDOUT redirect block | `open my $fh, '>', 'file'; print $fh "test content\n";` |
| `` sha256_result=`sha256sum ...` `` | 24-line `do { ... }` block | `my $sha256_result = qx{sha256sum test_checksum.txt};` |
| `` strings_result=`strings ... \| head -3` `` | 60+ line pipeline infrastructure | `my $strings_result = qx{strings test_binary.txt | head -3};` |
| `` tee_result=`echo ... \| tee ...` `` | 30+ line pipeline + file write | `my $tee_result = "test output\n"; write_file('test_tee.txt', $tee_result);` |
| `` perl_result=`perl -e '...'` `` | 12-line `capture_stdout` + `eval` block | `my $perl_result = "Hello from Perl\n";` |
| `rm -f test_checksum.txt` | 10-line safety-wrapped unlink | `unlink 'test_checksum.txt';` |

Each of these is a prime candidate for IR-based simplification because:

1. The **semantic essence** is simple (assignment of a string or command output).
2. The **generated complexity** comes from the generator's defensive patterns (error handling, exit status tracking, output accumulation, pipeline infrastructure).
3. An IR-based backend with **optimization passes** (dead code elimination, constant folding, import minimization) could reduce them significantly.
4. With `IrStmt::System { capture: Some("var") }`, whole pipelines become single `qx{...}` calls.

---

### Summary Table

| Pattern | IR-Fixable? | IR Node Involved | Cleaned-Up Output |
|---|---|---|---|
| A: Echo `do` block with trailing newline check | **Yes** | `IrStmt::Output { newline: true }` | `say "Echo result: $echo_result";` |
| B: Triple-declaration bloat | **Yes** | `IrStmt::Declare { vars: [Decl { sigil: Scalar }] }` | `my $echo_result;` |
| C: sha256sum native emulation | **Formatting only** (generator chooses native vs `qx`) | `IrStmt::System { capture }` or `IrStmt::Assign` | `my $sha256_result = qx{sha256sum test_checksum.txt};` |
| D: Pipeline infrastructure for 2 commands | **No** (generator must decide to use `qx{}` instead) | `IrStmt::Pipeline` (too low-level) | `my $strings_result = qx{strings ... \| head -3};` |
| E: STDOUT redirect for `echo > file` | **Formatting only** (generator chooses save/restore vs direct) | `IrStmt::Output` with file handle | `open my $fh, '>', 'file'; print $fh "test content\n";` |
| F: `perl -e` in `capture_stdout` + `eval` | **Yes** (via `IrStmt::System { capture }`) | `IrStmt::System { capture: Some("perl_result") }` | `my $perl_result = qx{perl -e '...'};` |
| G: `rm` safety wrapping | **No** (generator policy) | `IrStmt::System` or `IrStmt::RawText` | `unlink 'test_checksum.txt';` |
| H: `$CHILD_ERROR = 0` after echo | **Yes** (dead assignment elimination) | Dead store in IR control flow | Removed entirely |
| I: `$output` accumulation (dead code) | **Yes** (dead code elimination) | Unused `IrStmt::Assign` target | Removed entirely |
| J: Unused global variables | **Yes** (import minimization + DCE) | `IrProgram { imports, stmts }` | Only used imports/vars remain |

### Key Architectural Observation

The IR design doc identifies the correct root cause: the current generator emits Perl text directly via `format!()` calls in 80+ generator functions. The proposed IR adds a semantic layer between the generator and the text output. However, **the IR alone cannot fix patterns D, E, and G** — these require changes to the generator's own logic (choosing `qx{}` over pipeline infrastructure, using direct file opens instead of STDOUT save/restore, not wrapping `rm` in guards).

The IR *can* fix patterns A, B, H, I, and J purely through better pretty-printing and optimization passes. Patterns C and F sit in the middle: the *formatting* is IR-fixable, but the *decision* to emit native-Perl emulation vs. `qx{}` is a generator-level choice that must be made before IR nodes are constructed.