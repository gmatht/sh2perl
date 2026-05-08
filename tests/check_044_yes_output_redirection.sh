#!/usr/bin/env bash
set -euo pipefail

# Regression test: ensure purify's generated pure-Perl reproduces redirected
# pipeline output. Specifically for examples.impurl/044_yes_command.pl the
# pipeline `yes 'Output to file' | head -5 > yes_output.txt` should create
# yes_output.txt containing exactly five lines "Output to file\n".

EXAMPLE=examples.impurl/044_yes_command.pl

# Build debashc so purify can invoke it
cargo build --bin debashc

# Run purify in verbose mode and capture output into .test-work for inspection
mkdir -p .test-work/purify
perl purify.pl -v "$EXAMPLE" > .test-work/purify/044_yes_command.pure.pl 2>&1

# Extract the generated pure script from the verbose output (the script is
# printed at the end of the verbose trace). We heuristically extract the
# portion starting with the first #!/usr/bin/perl shebang encountered.
mkdir -p pure
awk '/^#!\/usr\/bin\/perl/{found=NR} {lines[NR]=$0} END{for(i=found;i<=NR;i++) print lines[i]}' .test-work/purify/044_yes_command.pure.pl > pure/044_yes_command.pl

if [[ ! -f pure/044_yes_command.pl ]]; then
  echo "Failed to extract generated pure script" >&2
  sed -n '1,200p' .test-work/purify/044_yes_command.pure.pl >&2 || true
  exit 2
fi

# Ensure a clean output file
rm -f yes_output.txt

# Run the generated pure script and capture its output so we can inspect
# the printed "File content:" block (the generated script deletes the
# file before exiting so we read the content from stdout).
perl pure/044_yes_command.pl > .test-work/purify/044.run.out 2>&1

# Extract the printed file content (lines following the literal "File content:")
awk 'found { if (length($0)==0) exit; print } /File content:/ { found=1; next }' .test-work/purify/044.run.out > .test-work/purify/044.file_content

if [[ ! -s .test-work/purify/044.file_content ]]; then
  echo "Failed to capture printed file content from generated script" >&2
  sed -n '1,200p' .test-work/purify/044.run.out >&2 || true
  exit 3
fi

# Validate captured content: exactly five lines 'Output to file' with trailing newlines
expected=$(printf "Output to file\n%.0s" {1..5})
actual=$(cat .test-work/purify/044.file_content)

if [[ "$actual" != "$expected" ]]; then
  echo "Captured file content unexpected" >&2
  echo "---- captured ----" >&2
  sed -n '1,200p' .test-work/purify/044.file_content >&2 || true
  echo "---- expected ----" >&2
  printf "%s" "$expected" >&2
  exit 4
fi

echo "OK: captured file content contains expected five lines"
