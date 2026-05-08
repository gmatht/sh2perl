#!/usr/bin/env bash
set -euo pipefail

# Run purify on the single example and check that the generated pure
# Perl file preserves the single-quoted glob pattern '*.txt'. This
# guards against regressions where the outer shell would expand the
# glob before passing it to an inner find command.

EXAMPLE=examples.impurl/037_complex_pipeline.pl

perl purify.pl "$EXAMPLE"

OUT=.test-work/purify/037_complex_pipeline/pure/037_complex_pipeline.pl

if [[ ! -f "$OUT" ]]; then
  echo "Generated file not found: $OUT" >&2
  exit 2
fi

if ! grep -F -q "find . -name '*.txt'" "$OUT"; then
  echo "Regression detected: expected single-quoted '*.txt' in generated file" >&2
  echo "Generated file content:" >&2
  sed -n '1,200p' "$OUT" >&2 || true
  exit 1
fi

echo "OK: generated file preserves '*.txt'"
