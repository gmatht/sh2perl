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
