Now I have all the information needed. Let me write the idiom review.

---

# Idiom Review: `examples/000__04d_system_utilities.sh` → Perl

## Source Shell Script

```bash
#!/bin/bash
echo "=== System Utilities ==="
formatted_date=`date '+%Y-%m-%d'`
echo "Formatted date: $formatted_date"
sleep_duration=`echo "1"`
echo "Sleeping for $sleep_duration seconds..."
sleep $sleep_duration
yes_result=`yes "Hello" | head -3`
echo "Yes command result:"
echo "$yes_result"
echo "=== System Utilities Complete ==="
```

## Generated Perl Code

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR
               $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04d_system_utilities.sh';
print "=== System Utilities ===\n";
my $formatted_date;
my @formatted_date;
my %formatted_date;
$formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime(time())) . "\n"
};
do {
    my $__echo_line = "Formatted date: $formatted_date";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $sleep_duration;
my @sleep_duration;
my %sleep_duration;
$sleep_duration = ("1");
do {
    my $__echo_line = "Sleeping for $sleep_duration seconds...";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
require Time::HiRes; Time::HiRes::sleep($sleep_duration);
my $yes_result;
my @yes_result;
my %yes_result;
$yes_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_66 = q{};
my $output_printed_66;
my $head_line_count = 0;
while (1) {
    my $line = 'Hello';
    if ($head_line_count < 3) {
    $output_66 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; last;
    }
}
$output_66 };
}; $_pipeline_result; };
print "Yes command result:\n";
print $yes_result;
if ( !( ($yes_result) =~ m{\n\z}msx ) ) { print "\n"; }
print "=== System Utilities Complete ===\n";

exit $main_exit_code;
```

---

## Non-Idiomatic Patterns

### 1. Triple Declaration of Every Variable

**Generated:**
```perl
my $formatted_date;
my @formatted_date;
my %formatted_date;
...
my $yes_result;
my @yes_result;
my %yes_result;
```

**Preferred idiomatic Perl:**
```perl
my $formatted_date;
my $yes_result;
```

The generator emits `$`, `@`, and `%` declarations for every variable regardless of how it's actually used. This is defensive shotgun-dotting that bloats the output 3×. None of these variables are ever used as arrays or hashes.

**IR-fixable?** Partially. The IR `Declare` node has a `sigil` field, so the IR backend could emit only the correct sigil. However, the *generator* currently emits **three separate `Declare` IR nodes** (one per sigil). To fix this, the generator must stop emitting the unused sigils. The IR backend could add a "deduplicate declarations" pass that merges them into one, but the root cause is in the generator's "declare all three sigils" policy.

**Verdict:** NOT IR-fixable (requires generator change). The generator's `generate_assignment` method explicitly declares all three sigils for every new variable. The IR cannot know which sigils are actually needed without cross-referencing all uses — that's a whole-program analysis the IR currently doesn't perform.

---

### 2. `print` Instead of `say` for Newline-Terminated Output

**Generated:**
```perl
print "=== System Utilities ===\n";
...
print "Yes command result:\n";
print "=== System Utilities Complete ===\n";
```

**Preferred idiomatic Perl:**
```perl
say "=== System Utilities ===";
say "Yes command result:";
say "=== System Utilities Complete ===";
```

The file already `use feature 'say';`, so `say` is available and preferred. The literal strings end with `\n`, making them perfect `say` candidates.

**IR-fixable?** YES. The IR `Output { value: IrExpr, newline: true }` node exists exactly for this purpose. In `emit_stmt()`, `newline: true` already maps to `say` — but the current generator bypasses the IR for these `echo` statements and emits `RawText` instead of `Output` nodes. Once the generator produces `Output` nodes, the backend cleanly emits `say`.

**IR node involved:** `IrStmt::Output { value, newline: true }`

**Cleaned-up output:**
```perl
say "=== System Utilities ===";
say "Formatted date: $formatted_date";
say "Sleeping for $sleep_duration seconds...";
say "Yes command result:";
say $yes_result;
say "=== System Utilities Complete ===";
```

This also eliminates the verbose newline-checking blocks (next pattern).

---

### 3. Verbose `echo` → Manual Newline-Checking `do` Block

**Generated (6 lines for one `echo`):**
```perl
do {
    my $__echo_line = "Formatted date: $formatted_date";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
```

**Preferred idiomatic Perl (1 line):**
```perl
say "Formatted date: $formatted_date";
```

This is the single worst verbosity pattern. Every `echo` in the shell source becomes a ~6-line `do` block with:
- A temp variable `$__echo_line`
- Manual `print` + newline-or-not check
- Appending to `$output` (which is never read afterwards)

**IR-fixable?** YES. This is the poster child for IR-based simplification. The generator should produce `IrStmt::Output { value: IrExpr::Interpolate(["Formatted date: ", IrExpr::Var("formatted_date")]), newline: true }` instead of `IrStmt::RawText(text)`. The backend then emits a clean `say`.

**IR node involved:** `IrStmt::Output { newline: true }`

**Cleaned-up output:**
```perl
say "Formatted date: $formatted_date";
say "Sleeping for $sleep_duration seconds...";
```

---

### 4. Unnecessary Parentheses Around String Literal

**Generated:**
```perl
$sleep_duration = ("1");
```

**Preferred:**
```perl
$sleep_duration = "1";
```

**IR-fixable?** YES. This is a `Declare` node with `init: Some(IrExpr::Str("1", SingleQuoted))` or an `Assign` node. The parens come from the `RawExpr` text emitted by the generator. When the IR uses `IrExpr::Str` directly, the `emit_stmt` for `Assign` does not add extraneous parentheses around the RHS.

**IR node involved:** `IrStmt::Assign { targets: [...], expr: IrExpr::Str("1", SingleQuoted) }`

**Cleaned-up output:**
```perl
my $sleep_duration = "1";
```

---

### 5. `yes "Hello" | head -3` → 18-Line Pipeline Simulation

**Generated (18 lines):**
```perl
$yes_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_66 = q{};
my $output_printed_66;
my $head_line_count = 0;
while (1) {
    my $line = 'Hello';
    if ($head_line_count < 3) {
    $output_66 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; last;
    }
}
$output_66 };
}; $_pipeline_result; };
```

**Preferred idiomatic Perl (1 line to generate, 3 with formatting):**
```perl
my $yes_result = join("", ("Hello\n") x 3);
```
or more readably:
```perl
my $yes_result = ("Hello\n") x 3;
```

The pipeline generator has no special case for `yes | head -n`. It builds the full buffered-pipeline infrastructure: an output buffer variable (`$output_66`), a printed-flag variable (`$output_printed_66`), a yes loop that runs forever until `last`, and a head processor that manually counts lines. This is the most extreme example of transliteration-style code.

**IR-fixable?** NO. The IR backend cannot combine two separate pipeline stages into a single expression. The pipeline is represented as `IrStmt::Pipeline { stages: [yes_stage, head_stage], ... }` and each stage is emitted independently. To recognize `yes | head -n` as a special pattern and emit a simple `join/repeat` or `x` operator, you would need:
1. A pattern-matching optimization pass over the IR (which doesn't exist yet)
2. Knowledge that `yes` produces repeating lines and `head -n` takes the first N
3. The ability to rewrite two stages into one expression

This requires generator-level knowledge or a sophisticated IR transformation pass. The current IR backend is a style pretty-printer, not a semantics-aware optimizer.

**Why the generator should handle this:** The `yes` command already has `generate_yes_command_with_context` which recognizes pipeline context. The `head` command has `get_head_num_lines`. What's missing is a pattern match in `generate_pipeline_impl` that detects "stage1 is `yes` + stage2 is `head`" and emits the simplified form directly. This is a generator-level shortcut, not an IR backend fix.

---

### 6. `require POSIX` Inline Inside a `do` Block

**Generated:**
```perl
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime(time())) . "\n"
};
```

**Preferred:**
```perl
use POSIX qw(strftime);
my $formatted_date = strftime('%Y-%m-%d', localtime);
```

Issues:
- `require POSIX` at runtime instead of `use POSIX` at compile time
- `require` is inside a `do` block (redundantly evaluated every time)
- `time()` is redundant inside `localtime()` (`localtime` with no args uses current time)
- `. "\n"` appended, only to be stripped by backtick semantics later

**IR-fixable?** NO. The IR does not have a module-management system that can hoist `require`/`use` statements from inside expressions to the top level. The `require POSIX;` is textually embedded inside the expression's `RawText`. The IR would need:
1. A module-dependency tracking field in `IrProgram`
2. An analysis pass that extracts embedded `require`/`use` calls
3. The ability to merge them into the top-level `imports` list

The design doc mentions "Import minimization" as a future optimization, but the current IR has no mechanism for this.

---

### 7. Dead Infrastructure Variables

**Generated:**
```perl
my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;
```

Of these:
- `$main_exit_code` is used only in `exit $main_exit_code;` at the end
- `$ls_success` is assigned but never read (dead)
- `$__set_e` is assigned but never read (dead)
- `$output` is appended to but never read (dead for program semantics; only matters if some later code reads it)
- `$CHILD_ERROR` is set but only `local`ized in one place; never used for control flow

**Preferred:**
```perl
my $main_exit_code;   # if we really need exit tracking
# (nothing else)
```

**IR-fixable?** Partially. Dead assignment elimination is listed as a planned IR optimization. The IR could, in theory, trace which variables are referenced after their last assignment and elide dead ones. However:
- `$output` is written to multiple times but never read — dead output
- `$ls_success` and `$__set_e` are truly dead
- The generator emits these based on `needs_*` heuristics that are too conservative

The IR currently doesn't track variable lifetimes. This would require a data-flow analysis pass.

**Verdict:** NOT IR-fixable with the current IR design. A future optimization pass could handle it.

---

### 8. Unnecessary Imports

**Generated:**
```perl
use Carp;
use English qw(-no_match_vars $ERRNO ... $PROGRAM_NAME);
use locale;
use IPC::Open3;
```

None of these are used:
- No `croak`/`carp` calls → `Carp` unnecessary
- No `$ERRNO`, `$EVAL_ERROR`, etc. → `English` unnecessary
- No locale-dependent operations → `locale` unnecessary
- No `open3` calls → `IPC::Open3` unnecessary

**Preferred:**
```perl
use strict;
use warnings;
use feature 'say';
```

**IR-fixable?** YES (partially). The IR tracks imports in `IrProgram::imports`. An import-minimization pass can scan the generated statements for references to imported symbols and omit unused ones. The current code already has some of this logic (see `needs_carp_import`, `needs_english_import` in `mod.rs`), but the heuristics are too conservative. An IR-level dead import elimination would cleanly solve this.

**IR node involved:** `IrProgram::imports`

**Cleaned-up output:**
```perl
use strict;
use warnings;
use feature 'say';
```

---

### 9. Redundant Newline Check After `print $yes_result`

**Generated:**
```perl
print $yes_result;
if ( !( ($yes_result) =~ m{\n\z}msx ) ) { print "\n"; }
```

The `$yes_result` is known to end with `\n` (the yes/head pipeline generates strings with `\n` appended). The newline check is always false at runtime, making it dead code. Also, the double-parens `( ($yes_result) )` are unnecessary.

**Preferred:**
```perl
say $yes_result;   # say always adds a newline, even if already present
```
or simply:
```perl
print $yes_result;  # $yes_result already ends with \n
```

**IR-fixable?** YES. `say $yes_result;` handles this correctly. If the generator emits `Output { value: Var("yes_result"), newline: true }`, the backend produces `say $yes_result;`. The newline check is only needed when the output is going to `$output` for later use — but in this script, `$output` is never read.

**IR node involved:** `IrStmt::Output { value: IrExpr::Var("yes_result", Scalar), newline: true }`

**Cleaned-up output:**
```perl
say $yes_result;
```

---

### 10. `require Time::HiRes` Inline for `sleep`

**Generated:**
```perl
my $sleep_duration = ("1");
say "Sleeping for $sleep_duration seconds...";
require Time::HiRes; Time::HiRes::sleep($sleep_duration);
```

**Preferred:**
```perl
use Time::HiRes qw(sleep);
my $sleep_duration = "1";
say "Sleeping for $sleep_duration seconds...";
sleep $sleep_duration;
```

**IR-fixable?** Same issue as #6 — the `require` is embedded inline. However, the `sleep` IR could use `IrStmt::System` or a dedicated sleep IR node that knows to import `Time::HiRes` at compile time. The current generator emits `require Time::HiRes; Time::HiRes::sleep(...)` as raw text.

---

### 11. `. "\n"` Appended to Date Expression

**Generated:**
```perl
$formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime(time())) . "\n"
};
```

The `. "\n"` is appended because the date generator treats it as a standalone `date` command that should print with a newline. But in a backtick context, trailing newlines are stripped by shell semantics. The generator should not append `"\n"` when the date is inside a backtick substitution.

**IR-fixable?** NO. The generator's `generate_date_expression` always appends `"\n"` regardless of context. The IR cannot distinguish "inside backtick" from "standalone" because the context is lost by the time the expression is built. The generator needs to be context-aware and skip the newline when inside a backtick.

---

## Unnecessarily Verbose Translations (Summary)

These are the most egregious cases where simple shell operations get wrapped in complex Perl control structures:

| Shell Line | Lines of Generated Perl | Idiomatic Perl | Verbosity Ratio |
|---|---|---|---|
| `echo "Formatted date: $formatted_date"` | 7 lines | `say "Formatted date: $formatted_date";` | 7× |
| `echo "Sleeping for $sleep_duration seconds..."` | 7 lines | `say "Sleeping for $sleep_duration seconds...";` | 7× |
| `yes_result=\`yes "Hello" \| head -3\`` | 18 lines | `my $yes_result = ("Hello\n") x 3;` | 18× |
| `echo "=== System Utilities ==="` | 1 line | `say "=== System Utilities ===";` | 1× (already simple) |

The pipeline translation is the worst offender: a simple `yes | head -3` becomes an infinite `while(1)` loop with manual line counting and pipeline buffer variables. This is the textbook example of "translating structure instead of semantics."

**Prime candidates for IR-based simplification (highest impact):**

1. **Echo translation** (#3) — Converting `echo` to `Output { newline: true }` IR nodes would turn 7-line `do` blocks into single `say` statements. This is the single highest-impact change.

2. **Pipeline simplification** (#5) — The `yes | head -3` pattern could be detected at the generator level and emit a one-liner. Not IR-fixable, but the IR backend could at least clean up the indentation and nesting of the pipeline infrastructure once the generator produces proper `Pipeline` IR nodes instead of `RawText`.

3. **Dead-code elimination** (#7, #8) — Removing unused imports and dead variables would cut the preamble from 12 lines to 4.

---

## Summary Table

| # | Pattern | IR-Fixable? | IR Node | Generator Change Needed? |
|---|---|---|---|---|
| 1 | Triple declaration (`$`, `@`, `%` for every var) | No | Declare | Yes — stop emitting unused sigils |
| 2 | `print "...\n"` instead of `say` | Yes | `Output { newline: true }` | No — just use IR |
| 3 | Verbose echo → `do` block with newline check | Yes | `Output { newline: true }` | No — emit `Output` instead of `RawText` |
| 4 | Unnecessary parens `("1")` | Yes | `Assign`/`Declare` | No — emit `Str` instead of `RawExpr` |
| 5 | `yes \| head` → 18-line pipeline | No | Pipeline | Yes — special-case pattern in generator |
| 6 | `require POSIX` inline inside `do` | No | (no module-hoisting IR yet) | Yes — extract to top-level `use` |
| 7 | Dead infrastructure variables | No | (no data-flow IR yet) | Yes — fix `needs_*` heuristics |
| 8 | Unnecessary imports | Yes | `IrProgram::imports` | No — IR import-minimization pass |
| 9 | Redundant newline check after `print` | Yes | `Output { newline: true }` | No — emit `say` via IR |
| 10 | `require Time::HiRes` inline | No | (same as #6) | Yes — extract to top-level |
| 11 | `. "\n"` on date inside backtick | No | (context lost) | Yes — context-aware date generator |

**Overall assessment:** 5 of the 11 patterns are IR-fixable (2, 3, 4, 8, 9) with the existing IR design. These are primarily the echo/print style fixes and import elimination. The remaining 6 patterns require generator-level changes: the pipeline special-casing, the triple-declaration policy, the inline `require` placement, the dead variable heuristics, and the context-aware newline handling. The IR is well-suited for style fixes but cannot fix semantic generator decisions like "what to emit for `yes | head`" or "how many sigils to declare."