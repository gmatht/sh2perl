#!/usr/bin/env bash
#  proofs/verus/ci_verify.sh — actually RUN Verus on the proofs a change
#  touches, plus a small always-verified subset.
#
#  The manifest gate (`check_sync.py`) recomputes sha256 pins, but a pin is
#  evidence about bytes, not a verification: nothing in it recompiles a proof.
#  This script is the missing half — it invokes the pinned Verus binary on:
#
#    * every `proofs/verus/*.rs` file changed between BASE and HEAD, and
#    * a small fixed subset, so a change to a proof's shared assumptions (or a
#      toolchain regression) is caught even when the PR touches no proof.
#
#  Every file must report `0 errors`; any failure fails the job.
#
#  Usage:
#     proofs/verus/ci_verify.sh <verus-home> [<base-sha-or-empty>]
#
#  With an empty/unknown base only the fixed subset runs (that is the
#  push-to-main-after-squash case). `i32_bounds_sync.rs` is a GENERATED
#  transcript, not paste-ready Verus, and is skipped (see the README).
set -uo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <verus-home> [<base-sha-or-empty>]" >&2
    exit 2
fi

verus_home=$1
base=${2:-}

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 2

verus="$verus_home/verus"
if [ ! -x "$verus" ]; then
    echo "::error::no verus binary at $verus" >&2
    exit 2
fi

# The fixed subset: the laws a change is most likely to invalidate, kept small
# enough that a no-proof PR still finishes quickly.
fixed=(
    status_reads
    status_reads_traversal
    uae_terminators
    affix_strip_fold
    text_capture_fold
    bc_float_clean
    shir_native_herestring
)

# Not paste-ready Verus: a generated transcript checked in for review only.
skip=(i32_bounds_sync)

files=()
declare -A seen=()

add_file() {
    local f=$1 name
    name=$(basename "$f" .rs)
    for s in "${skip[@]}"; do
        if [ "$name" = "$s" ]; then
            echo "skip  proofs/verus/$name.rs (generated transcript, not Verus)"
            return
        fi
    done
    if [ ! -f "$f" ]; then
        return
    fi
    if [ -n "${seen[$f]:-}" ]; then
        return
    fi
    seen[$f]=1
    files+=("$f")
}

if [ -n "$base" ] && git rev-parse --verify --quiet "$base^{commit}" >/dev/null; then
    echo "Comparing $base..HEAD for changed proofs"
    while IFS= read -r f; do
        [ -n "$f" ] && add_file "$f"
    done < <(git diff --name-only --diff-filter=ACMR "$base" HEAD -- 'proofs/verus/*.rs')
else
    echo "No usable base commit ('$base'): fixed subset only"
fi

for name in "${fixed[@]}"; do
    add_file "proofs/verus/$name.rs"
done

if [ ${#files[@]} -eq 0 ]; then
    echo "No proofs to verify"
    exit 0
fi

echo "Verifying ${#files[@]} proof(s) with $("$verus" --version 2>/dev/null | head -n1)"

failed=()
for f in "${files[@]}"; do
    echo "::group::verus $f"
    out=$("$verus" --triggers-mode silent --rlimit 40 "$f" 2>&1)
    rc=$?
    printf '%s\n' "$out"
    echo "::endgroup::"
    summary=$(printf '%s\n' "$out" | grep -E 'verification results::' | tail -n1)
    if [ $rc -ne 0 ] || ! printf '%s' "$summary" | grep -qE '[0-9]+ verified, 0 errors'; then
        failed+=("$f")
        echo "::error file=$f::Verus failed (exit $rc): ${summary:-no verification summary}"
    else
        echo "ok    $f — $summary"
    fi
done

if [ ${#failed[@]} -ne 0 ]; then
    echo
    echo "FAILED ${#failed[@]} of ${#files[@]} proof(s):"
    printf '  %s\n' "${failed[@]}"
    exit 1
fi

echo
echo "All ${#files[@]} proof(s) verified, 0 errors"
