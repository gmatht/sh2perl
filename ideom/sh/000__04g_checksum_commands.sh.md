Now I have all the pieces. Let me look at the broken pipeline code more carefully.Now I have everything I need to write the idiom review. Here it is:

---

# Idiom review: sh/000__04g_checksum_commands.sh

## Source

```bash
#!/bin/bash

# Checksum commands using backticks
# This file demonstrates checksum and related commands with backticks

echo "=== Checksum Commands ==="

# sha256sum command with backticks
#PERL_MUST_NOT_CONTAIN `sha256sum
echo "test content" > test_checksum.txt
sha256_result=`sha256sum test_checksum.txt`
echo "SHA256 result: $sha256_result"

# sha512sum command with backticks
#PERL_MUST_NOT_CONTAIN `sha512sum
sha512_result=`sha512sum test_checksum.txt`
echo "SHA512 result: $sha512_result"

# strings command with backticks
#PERL_MUST_NOT_CONTAIN `strings
strings_result=`strings target/debug/debashc.exe | head -3`
echo "Strings result:"
echo "$strings_result"

# Cleanup
rm -f test_checksum.txt

echo "=== Checksum Commands Complete ==="
```

## Generated Perl

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

say "=== Checksum Commands ===";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_checksum.txt'
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
my $sha256_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha256_hex(
            do {
                local $INPUT_RECORD_SEPARATOR = undef;
                open my $fh, '<', 'test_checksum.txt'
                  or croak "Cannot open 'test_checksum.txt': $ERRNO";
                my $content = <$fh>;
                close $fh
                  or croak "Close failed: $ERRNO";
                $content;
            }
        );
        push @results, "$hash  test_checksum.txt";
    }
    else {
        push @results,
"0000000000000000000000000000000000000000000000000000000000000000  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
say "SHA256 result: $sha256_result";
my $sha512_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha512_hex(
            do {
                local $INPUT_RECORD_SEPARATOR = undef;
                open my $fh, '<', 'test_checksum.txt'
                  or croak "Cannot open 'test_checksum.txt': $ERRNO";
                my $content = <$fh>;
                close $fh
                  or croak "Close failed: $ERRNO";
                $content;
            }
        );
        push @results, "$hash  test_checksum.txt";
    }
    else {
        push @results,
"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
say "SHA512 result: $sha512_result";
my $strings_result = do {
    do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    my $input_data;
    if ( open my $fh, '<', 'target/debug/debashc.exe' ) {
        local $INPUT_RECORD_SEPARATOR = undef;;
say "Strings result:";
say $strings_result;
if ( -e "test_checksum.txt" ) {
    if ( -d "test_checksum.txt" ) {
        carp "rm: carping: ", "test_checksum.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_checksum.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "test_checksum.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
say "=== Checksum Commands Complete ===";
}
}
}
}
```

## Idiom issues

| # | Pattern | Generated code | Idiomatic Perl | IR-fixable? |
|---|---------|---------------|----------------|-------------|
| 1 | **Redundant `do { ... }` wrapper for simple assignment** | `my $sha256_result = do { my @results; if (...) { ... } join(...) . "\n"; };` | `my $sha256_result = sha256_hex( slurp('test_checksum.txt') ) . "  test_checksum.txt\n";` | Yes |
| 2 | **STDOUT save/restore for redirect** | `do { open my $orig, '>&', STDOUT; open STDOUT, '>', $file; print $tmp; open STDOUT, '>&', $orig; close $orig; };` | `system 'echo', 'test content', '>', 'test_checksum.txt';` or `write_file('test_checksum.txt', "test content\n");` | Yes |
| 3 | **Verbose file slurp with manual open/close** | `do { local $INPUT_RECORD_SEPARATOR = undef; open my $fh, '<', $file or croak "..."; my $content = <$fh>; close $fh or croak "..."; $content; }` | `do { local $/; open my $fh, '<', $file; <$fh> }` | Yes |
| 4 | **Pipeline infrastructure for `strings... \| head -3`** | `do { do { my $output_0 = q{}; my $output_printed_0; my $pipeline_success_0 = 1; my $input_data; if (open ...) { local $INPUT_RECORD_SEPARATOR = undef;; ...` (with no actual command execution) | `my $strings_result = qx{strings target/debug/debashc.exe | head -3};` | Yes |
| 5 | **Over-verbose `rm -f` translation** | `if (-e "file") { if (-d "file") { carp "... is a directory ..." } else { if (unlink "file") { } else { carp "... could not remove ..." } } } else { local $CHILD_ERROR = 0; }` | `unlink 'test_checksum.txt'` or `system 'rm', '-f', 'test_checksum.txt';` | Yes |
| 6 | **Unused imports** | `use IPC::Open3; use File::Path qw(make_path remove_tree);` | (remove entirely) | Yes |
| 7 | **Non-idiomatic error variables** | `$OS_ERROR`, `$INPUT_RECORD_SEPARATOR`, `$ERRNO`, `croak`, `carp` without `use English` or `use Carp` | Use `$!`, `$/`, and import `croak`/`carp` from `Carp`, or use `die`/`warn` | Yes |
| 8 | **Nested `do { do {` for single pipeline stage** | `do { do { my $output_0 ... } }` — three levels of nesting for a simple `strings | head -3` | `my $output = qx{strings ... | head -3};` | Yes |

## Unnecessarily verbose translations

### #1: `echo "test content" > test_checksum.txt` → 14 lines of redirect infrastructure

The generated code saves STDOUT, redirects it to the file, runs a `do { say ... }`, prints the result, then restores STDOUT — for a simple one-line echo with a redirect. The entire 14-line `do { ... }` block should be either:

```perl
# Option A: delegate to the shell
system 'echo', 'test content', '>', 'test_checksum.txt';

# Option B: native Perl (best — no shell involved)
use File::Slurper qw(write_file);
write_file('test_checksum.txt', "test content\n");

# Option C: simple qx{} backtick emulation
qx{echo "test content" > test_checksum.txt};
```

The IR `IrStmt::System { cmd: "echo", args: ["test content"], redirects: [{op: ">", target: "test_checksum.txt"}] }` would let `ir_to_perl()` choose the cleanest strategy.

### #2: `sha256sum`/`sha512sum` backtick substitution → 21 lines each

The shell:
```bash
sha256_result=`sha256sum test_checksum.txt`
```

Becomes 21 lines of Perl that:
1. Checks if the file exists (`-f`)
2. Opens and slups the file with manual open/close
3. Calls `sha256_hex()` on the content
4. Formats the output with "  filename" suffix
5. Provides a fallback hex string for missing files
6. Joins results with newlines

Since `#PERL_MUST_NOT_CONTAIN `sha256sum` says backticks must not appear, using `Digest::SHA` is correct and desirable — but the framing is far too complex. The IR should emit:

```perl
my $sha256_result = sha256_hex(do { local $/; open my $fh, '<', 'test_checksum.txt'; <$fh> })
                   . "  test_checksum.txt\n";
```

Or, better, an `IrExpr::Slurp { path: "test_checksum.txt" }` node that `ir_to_perl()` renders compactly. The `@results` array + `join` + `if (-f)` is dead weight for a script that just wrote the file two lines earlier.

### #3: `rm -f test_checksum.txt` → 17 lines of defensive error handling

The shell:
```bash
rm -f test_checksum.txt
```

Generates 17 lines of Perl with:
- `-e` existence check
- `-d` directory check with `carp` warning
- `unlink` with error reporting
- `else { local $CHILD_ERROR = 0; }` for the non-existent case

For `rm -f` (force, no error on missing file), this is:

```perl
unlink 'test_checksum.txt';
```

That's it. `unlink` returns false if the file doesn't exist, but with `rm -f` semantics that's a no-op, not an error. The `-d` check is irrelevant for a file we just wrote. The `$CHILD_ERROR` dance is pointless for a native Perl `unlink`.

An `IrStmt::System { cmd: "rm", args: ["-f", "test_checksum.txt"] }` with a `force` flag would let `ir_to_perl()` choose between native `unlink` or shell delegation.

### #4: `strings ... | head -3` pipeline → completely broken generation

The generated code declares pipeline infrastructure variables (`$output_0`, `$output_printed_0`, `$pipeline_success_0`, `$input_data`) and opens the input file, but **never executes `strings` or `head`**. The `if (open ...)` block is opened with a dangling `;;` and then the cleanup/print code for later commands is emitted inline inside this never-closed block. The three closing braces at the end are the brace-balance safety net, not proper control flow.

This is the most egregious verbosity problem: a whole pipeline abstraction layer is instantiated but does nothing. The fix is to use `IrStmt::System { capture: Some("strings_result"), cmd: "strings", args: [...] }` for the individual commands, and let `ir_to_perl()` decide whether to use `qx{}`, `readpipe`, or `IPC::Open3`.

## IR-fixability

### Issue 1: Redundant `do { ... }` wrapper for simple capture → IR-fixable

**IR node involved:** `IrStmt::System { cmd, args, capture: Some(var) }` → the emitter already handles this case. When the generator emits `System` with `capture: Some("sha256_result")`, `ir_to_perl()` produces:

```perl
my $sha256_result = qx{sha256sum test_checksum.txt};
$CHILD_ERROR = $? >> 8;
```

That's it. No `do { my @results; if... }` block. However, the `#PERL_MUST_NOT_CONTAIN` annotation forces the generator to avoid backtick commands and use `Digest::SHA` instead. In that case, the IR needs a new node like `IrExpr::Digest { algorithm: "sha256", file: "test_checksum.txt" }` so that `ir_to_perl()` can produce the compact form:

```perl
my $sha256_result = sha256_hex(do { local $/; open my $fh, '<', 'test_checksum.txt'; <$fh> })
                   . "  test_checksum.txt\n";
```

**What the cleaned-up output would look like:**
```perl
my $sha256_result = sha256_hex(
    do { local $/; open my $fh, '<', 'test_checksum.txt'; <$fh> }
) . "  test_checksum.txt\n";
say "SHA256 result: $sha256_result";
```

### Issue 2: STDOUT save/restore for redirect → IR-fixable

**IR node involved:** A new `IrStmt::WriteFile { path, content }`, or the existing `IrStmt::System` with redirect information. If the generator emits `System { cmd: "echo", args: ["test content"], redirects: [{op: ">", target: "test_checksum.txt"}] }`, the `ir_to_perl()` can emit:

```perl
system 'echo', 'test content', '>', 'test_checksum.txt';
```

Or if the IR has a dedicated `WriteFile` node:
```perl
use File::Slurper qw(write_file);
write_file('test_checksum.txt', "test content\n");
```

Either way, the 14-line `do { open... }` is eliminated.

### Issue 3: Verbose file slurp → IR-fixable

**IR node involved:** `IrExpr::Slurp { path }` (new node). The generator would emit `IrExpr::Slurp { path: IrExpr::Str("test_checksum.txt", SingleQuoted) }` and `ir_to_perl()` would produce:

```perl
do { local $/; open my $fh, '<', 'test_checksum.txt'; <$fh> }
```

Instead of the current 9-line open/close/croak pattern.

### Issue 4: Pipeline infrastructure for simple commands → IR-fixable

**IR node involved:** `IrStmt::System { capture: Some("strings_result") }`. The generator should recognize that `strings ... | head -3` is a command substitution and emit a single `System` capture. The `ir_to_perl()` backend would produce:

```perl
my $strings_result = qx{strings target/debug/debashc.exe | head -3};
$CHILD_ERROR = $? >> 8;
```

No `do { do { my $output_0 ... } }` nonsense — just a clean `qx{}` assignment. The pipeline abstraction should only kick in for multi-stage pipelines where intermediate data needs Perl processing.

### Issue 5: Over-verbose `rm -f` → IR-fixable

**IR node involved:** `IrStmt::System { cmd: "rm", args: ["-f", "test_checksum.txt"] }`. The `ir_to_perl()` backend, recognizing `rm -f` (no errors on missing), can emit:

```perl
unlink 'test_checksum.txt';
```

Or for non-trivial `rm`:
```perl
system 'rm', '-f', 'test_checksum.txt';
```

### Issue 6: Unused imports → IR-fixable

**IR node involved:** The `IrProgram.{imports}` field at the top of `ir_to_perl()`. The backend already emits imports from the `IrProgram` struct. The fix is in the generator: don't emit `use IPC::Open3;` or `use File::Path;` when they're not needed. `IrProgram::from_raw_perl()` adds them unconditionally; once the migration away from `RawText` is complete, the generator will only add the imports actually needed by the IR nodes it emits.

### Issue 7: Non-idiomatic error variables → IR-fixable

**IR node involved:** The `emit_stmt` handler for `IrStmt::System` etc. in `ir_to_perl()`. Currently the emitter writes `$OS_ERROR`, `$INPUT_RECORD_SEPARATOR`, `$ERRNO` which require `use English`. The fix is to emit `$!` instead of `$OS_ERROR`/`$ERRNO` and `$/` instead of `$INPUT_RECORD_SEPARATOR`, or add `use English qw(-no_match_vars)` to the import block. The idiom decision belongs in `ir_to_perl()`, not in the generator.

### Issue 8: Nested `do { do {` → IR-fixable

This is the same root cause as Issue 4. The pipeline abstraction produces nested `do` blocks for each pipeline stage, even when there's only one stage. The IR fix is the same: use `IrStmt::System` with capture instead of `IrStmt::Pipeline` for single-command substitutions.

## Summary

**IR-fixable: 8/8 issues.** None of the non-idiomatic patterns require changing generator logic. Every issue stems from:

1. The `ir_to_perl()` backend being incomplete (still using `RawText` for most constructs)
2. The absence of semantic IR nodes for common operations (`Slurp`, `WriteFile`, `Digest`)
3. The pipeline generator emitting full infrastructure for single-command captures

The **highest-impact** migrations for this script:

| Priority | IR node | Replaces | Lines saved per occurrence |
|----------|---------|----------|----------------------------|
| P0 | `IrExpr::Slurp` | manual open/close/slurp | 9 lines → 1 line |
| P1 | `IrStmt::System` with capture | `do { my @results; if... join... }` | 21 lines → 2 lines |
| P2 | `IrStmt::System` with redirect | STDOUT save/restore | 14 lines → 1 line |
| P3 | Recognize `rm -f` as simple `unlink` | `if (-e)... if (-d)...` | 17 lines → 1 line |

Once these IR nodes are added and the generator emits them, the 105-line generated Perl would collapse to approximately 30 idiomatic lines.