Fix: Preserve shell command strings when embedding in generated Perl

Problem
-------
Some command strings (notably pipelines containing single-quoted awk/sed programs
with "$" variables such as "$0") were inserted raw into generated Perl code
snippets like open3(..., 'bash', '-c', '{}') without being wrapped as a
non-interpolating Perl literal. If the command contained single quotes the
outer Perl single-quote would be broken and embedded "$" sequences could be
interpreted by Perl (for example $0 became the Perl $PROGRAM_NAME path), which
changed the runtime behavior and caused errors.

Fix
---
Wrap command strings that are embedded into generated Perl code with
perl_string_literal_no_interp so they become q{}-style or other safe Perl
non-interpolating literals. This preserves byte-for-byte contents (including
single quotes and $ sequences) and prevents accidental Perl interpolation.

Files changed
-------------
- src/generator/utils.rs: ensure command strings used in open3('bash','-c', ...)
  are wrapped with perl_string_literal_no_interp before embedding.

Also changed purify.pl: do not force double-quoting when a shell snippet
contains "$" or "@". These characters are common in awk/sed programs and
should be preserved verbatim in Perl literals to avoid accidental Perl
interpolation (for example $0 becoming $PROGRAM_NAME). The script now only
uses double-quoted Perl literals when the text contains characters that
require escaping (newlines, double quotes, backslashes, or control chars).

Why this is minimal and safe
---------------------------
This change only affects how command strings are quoted when they are embedded
into generated Perl code. It uses an existing helper that already exists in the
generator (perl_string_literal_no_interp) and does not change the runtime
semantics of the generator except to preserve the original shell command text
literally. It fixes the specific failing example examples.impurl/035_pipeline_basic.pl
where awk's $0 was being replaced by the script path.

Verification
------------
After this change regenerate the purified Perl for the failing example and run
it. The awk program should receive the literal string containing $0 (not the
script path), and the runtime error about "No such file or directory" should
no longer occur.

Fix: Locate 'system' tokens precisely in purify.pl
-------------------------------------------------
Problem
-------
purify.pl previously used a PPI::Find predicate that matched entire
PPI::Statement nodes containing the word 'system'. In some cases that
returned an enclosing compound statement (for/if/etc.) instead of the
actual statement containing the system() call. The argument-list
extraction then picked up unrelated tokens (e.g. loop ranges like
"1..3") and produced incorrect reconstructions which left raw
system() calls or backticks in the purified output.

Fix
---
Match the PPI::Token::Word nodes whose content is 'system' and then
walk up to the nearest enclosing PPI::Statement. This reliably finds
the real call site and prevents accidentally using surrounding
constructs as the argument list.

Files changed
-------------
- purify.pl: find 'system' token nodes and climb to enclosing
  PPI::Statement instead of matching whole statements. This keeps the
  purify logic robust for constructs like for/if where the previous
  approach could misidentify the argument list.

Why this is minimal and safe
---------------------------
  This only changes how purify.pl locates system() calls in the parsed
  Perl AST; it does not change the conversion logic or the Rust
  debashc behaviour. It fixes the specific failing example
  examples.impurl/036_control_flow_basic.pl where loop headers were
  being mistaken for system() arguments.

Fix: tolerate non-executable debashc binary in test harness
---------------------------------------------------------
Problem
-------
The test harness (test_purify.pl) previously required the debashc
binary to be present and executable at target/debug/debashc. In some
environments the file may exist but lack the executable bit (for
example due to umask or filesystem extraction), causing the test to
fail early with a confusing "not found" message.

Fix
---
Modify test_purify.pl to: (1) try the .exe suffix on Windows when the
plain name isn't executable, (2) if the file exists but isn't
executable on Unix-like systems, attempt chmod +x on it, and (3) only
error out when the file truly doesn't exist or cannot be made
executable. This makes the test harness more robust in CI and local
builds where file modes may differ.

Files changed
-------------
- test_purify.pl: attempt to set the executable bit on the built
  debashc binary when present but not executable; improve error
  messaging.

Why this is minimal and safe
---------------------------
This change only affects the test harness and is a small, defensive
improvement to avoid spurious failures when the debashc binary exists
but lacks execute permissions. It does not change any code generation
paths or runtime behaviour of the debashc program itself.

Fix: Recombine split short-options when serializing shell commands
-----------------------------------------------------------------
Problem
-------
When reconstructing shell command strings from the parsed AST, some short
options that were originally combined (for example "-nr") could be split into
two separate tokens ("-n", "r"). When the generator naively joined tokens with
spaces this produced strings like "-n r" which many utilities interpret as a
filename "r" instead of the combined flag.

Fix
---
Post-process argument lists when generating shell command strings to conservatively
merge occurrences where a token of the form "-x" is immediately followed by a
single ASCII letter token. For example ["-n", "r"] becomes ["-nr"]. This is
applied in the bash/string generation code paths used for system/inline commands
and process-substitution generation.

Files changed
-------------
- src/generator/redirects.rs: merge short-option fragments in generate_bash_command_string
- src/generator/commands/system_commands.rs: merge short-option fragments in generate_command_string_for_system_impl

Why this is minimal and safe
---------------------------
The change is conservative (only merges when the second token is a single ASCII
letter) and limited to the serialization step. It avoids broad parser or AST
changes while fixing a practical class of errors (notably "sort -nr" becoming
"sort -n r").

Fix: Escape awk-style $ variables in example 037
------------------------------------------------
Problem
-------
In example 037 some embedded awk snippets used unescaped "$" tokens inside
Perl backtick strings. When the example was parsed and regenerated the
embedded $1/$2/etc. could be interpreted by Perl or altered during
reconstruction, causing the purified script to behave differently from the
original.

Fix
---
Escape awk-style variables in examples.impurl/037_complex_pipeline.pl so they
remain literal in the shell/awk programs when embedded within Perl
backtick contexts (i.e. use \$1 instead of $1 inside those strings). This
keeps purify.pl as a thin wrapper around the Rust generator and avoids adding
ad-hoc quoting logic for this specific case in the generator.

Why this is minimal and safe
---------------------------
This change only updates the example source to use the correct escaping when a
shell snippet containing awk variables is embedded in a Perl string. It does not
change generator logic and resolves the output mismatch observed in the test
suite.

Fix: Avoid double-qualifying Carp helpers in purify.pl
----------------------------------------------------
Problem
-------
When splicing generated Perl snippets back into an existing document we
previously replaced unqualified Carp helpers (croak/confess) with fully
qualified Carp::croak/Carp::confess. However the replacement pattern did not
guard against already-qualified names like Carp::croak; this produced
Carp::Carp::croak which is a syntax error in Perl and caused purify to emit
invalid code.

Fix
---
Update purify.pl to only replace unqualified occurrences of croak/confess and
avoid touching identifiers that are already namespace-qualified. This prevents
creating Carp::Carp::croak tokens and keeps the purified output syntactically
valid.

Files changed
-------------
- purify.pl: only qualify unqualified croak/confess identifiers when
  normalizing generated snippets.

Why this is minimal and safe
---------------------------
This change tightens a single regex used for post-processing generated snippets
and fixes a concrete syntax-error observed in an example. It does not change
the broader generator logic and keeps purify.pl as a thin wrapper around the
Rust debashc output.

Fix: Serialize Command::Block in bash string generation
------------------------------------------------------
Problem
-------
Subshells containing multiple commands were sometimes serialized into the
placeholder message "Complex command not supported in bash string generation"
when the generator recursed into a Command::Block that wasn't explicitly
handled. That caused purify.pl to embed an echo of the placeholder into the
generated Perl which broke behavior for examples that use subshells with
multiple statements (e.g. examples.impurl/039_subshell_operations.pl).

Fix
---
Handle the Command::Block variant in generate_bash_command_string by
serializing the inner commands and joining them with "; ". This makes
subshells like (cmd1; cmd2) round-trip correctly into bash -c invocations and
avoids the fallback placeholder for multi-command subshells.

Files changed
-------------
- src/generator/redirects.rs: add Command::Block arm in
  generate_bash_command_string which maps inner commands through the same
  generator and joins them with "; ".

- src/generator/commands/system_commands.rs: ensure Command::Block inside
  Command::Subshell is handled by delegating to generate_bash_command_string so
  subshells with multiple commands (e.g. (cmd1; cmd2)) serialize into proper
  bash strings instead of falling back to the placeholder message.

Why this is minimal and safe
---------------------------
The change is localized to the string-serialization helper used when a Perl
snippet needs to construct a bash -c command (process substitution, qx{},
etc.). It preserves shell semantics by joining commands with the standard
command-separator and avoids broader changes to the AST or generator logic.

Verification
------------
Regenerate the purified Perl for the failing example (examples.impurl/039_subshell_operations.pl)
and confirm the placeholder no longer appears in the generated output.


Fix: Ensure Digest::SHA is imported for generated sha*_hex usages
----------------------------------------------------------------
Problem
-------
Some purified Perl snippets generated by the Rust debashc output call
Digest::SHA functions (sha256_hex, sha512_hex) but the final spliced
document did not always include the corresponding "use Digest::SHA"
import. That caused runtime errors like "Undefined subroutine
&main::sha256_hex called" when running the purified examples.

Fix
---
Detect when the final serialized document contains calls to
sha256_hex or sha512_hex and, if the document lacks a "use
Digest::SHA" import, prepend the appropriate import line. This keeps
purify.pl as a thin wrapper while ensuring the generated helpers are
available at runtime.

Files changed
-------------
- purify.pl: add an insertion of "use Digest::SHA   qw(sha256_hex sha512_hex);"
  when needed.

Why this is minimal and safe
---------------------------
This change only adds an import when the generated code references the
Digest::SHA helpers and the import is absent. It avoids changing the
generator or the examples and keeps purify.pl focused on integration
concerns.

Fix: Wrap sha*sum check-mode and && results for command-substitution
-------------------------------------------------------------------
Problem
-------
The generator emitted multi-statement Perl fragments for sha256sum/sha512sum
in check mode (-c) and for Command::And (left && right). When such fragments
were inserted into command-substitution contexts (backticks) they needed to be
single expressions; otherwise constructs like assigning a multi-declaration to
a scalar produced invalid Perl (e.g. "my $r = my @lines = ...;").

Fix
---
The generator now wraps multi-statement check-mode verifiers in a `do { ... }`
block when the output is intended for command-substitution (the `input_var`
is empty). Command::And right-hand results are also wrapped in a `do { ... }`
block before being assigned, preventing invalid scalar assignments.

Why this is minimal and safe
---------------------------
Wrapping existing multi-statement output in a `do { ... }` block doesn't change
the generated logic; it merely ensures the fragment is a single expression and
thus valid in substitution contexts. This is localized to generator output
formatting and keeps purify.pl as a thin integration layer.
