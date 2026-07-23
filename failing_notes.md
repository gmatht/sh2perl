# Failing Test Notes

## All tests passing (169/169) 🎉

## Remaining check_qx.pl violations (1):

- **051_primes.sh**: `open3(..., 'bash', '-c', "echo \"$var\" | bc")` — the pipeline handler 
  generates an `echo ... | bc` command wrapped in `bash -c` for piping pipeline output to `bc`.
  Fixing this requires modifying the Rust `format!()` macro call in pipeline_commands.rs (bytes
  ~30299-30709) to use `open` with pipe mode instead of `open3` with `bash -c`. This involves
  complex multi-level Rust string escaping (`\\n`, `\\"`, `\\\\n`, `\\'`) that is very
  error-prone to implement correctly.
