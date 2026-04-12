Fixed `purify.pl` reconstructing `system(...)` calls by quoting shell operators like `|` as literal arguments. That caused debashc to see pipeline separators as filenames, breaking backtick and `system(...)` pipelines.

Fixed `purify.pl` treating multi-argument `system LIST` calls like shell commands. That broke cases such as `system("echo", "...", "|", "tee", ...)` because the pipe token was passed through as a real pipeline separator instead of a literal argument.

Fixed `purify.pl` preserving shell-operator tokens like `|` when reconstructing `system LIST` calls. Those arguments must stay literal for Perl's list-form `system`, otherwise pipeline separators get reinterpreted as real shell operators and the purified output changes behavior.

Fixed `purify.pl` keeping list-form `system(...)` arguments intact when they contain shell operators such as `|`. The purifier was rebuilding those calls through shell-style parsing, which turned literal arguments into pipeline separators and broke `test_purify.pl`'s pipeline examples.

Fixed the Rust `tee` generator treating `/dev/stdout` as an extra stdout copy. That duplicated output in the purified pipeline, so `/dev/stdout` is now treated as a literal tee target instead of changing the emitted stdout expression.

Fixed `generate_command_string_for_system_impl` reconstructing Perl `system LIST` pipeline examples as literal arguments. It now recognizes `|` tokens inside a single command string and emits a real shell pipeline string so the purified output matches the original behavior.
