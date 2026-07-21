# Acceptable qx{} / system() calls

These are the ONLY cases where qx{} or system() calls in generated Perl are
acceptable. Every other qx{}/system() call is a bug — the translator should
convert it to native Perl.

| Pattern | File | Reason |
|---------|------|--------|
| `bash -c` | `allowed_qx_calls.txt` | Arbitrary shell code inside a `-c` string — no general native translation possible. |
| `ls -l` | `allowed_qx_calls.txt` | The native `ls` handler produces simplified output. Replicating `ls -l`'s full format (permissions, owner, size, date, block count) is impractical. |
| `diff` | `allowed_qx_calls.txt` | `diff` output formatting is complex and environment-dependent. The native handler falls through to qx{} for backtick context. |

Patterns NOT in `allowed_qx_calls.txt` (e.g. `find`, `cp`, `mkdir`, `dirname`,
`basename`, `wc`, `echo`, `grep`, `sed`, etc.) **must** be translated to native
Perl. If you see a violation for one of these, fix the generator — do not add
it to the exemptions file.
