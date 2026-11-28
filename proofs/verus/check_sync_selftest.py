#!/usr/bin/env python3
"""Mutation self-test for `check_sync.py`: every detector class must fire.

Each case edits one manifest (or one proof file's name reference), runs the
checker on that manifest alone, and restores the file byte-identically —
the restore is asserted by sha256, so a failed edit can never be left behind.

    python3 proofs/verus/check_sync_selftest.py

Exit 0 iff every case produced exit status 1 and the expected failure kind.
"""
import hashlib
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

# (label, manifest, text to replace, replacement, expected failure kind, run tests)
CASES = [
    ("phantom pin", "status_reads",
     "pin pin_bare_exit_reads_the_status", "pin pin_renamed_away", "phantom-pin", False),
    ("stale source sha", "bc_float_clean",
     "bc_float_clean=c5767ccaaa36", "bc_float_clean=deadbeef1234", "stale-source", False),
    # The old admitted-staleness shape: a wrong sha carrying a STALE (…) note.
    # The checker must NOT honour the annotation — same failure as a bare row.
    ("stale + STALE note", "copy_propagation",
     "copy_propagation=eb5a02ad209f",
     "copy_propagation=deadbeef1234 STALE (audited: no drift in the region)",
     "stale-source", False),
    ("wrong proof-fns", "split_set",
     "proof-fns: 11", "proof-fns: 12", "counts", False),
    ("mapped, no exec", "affix_strip_fold",
     "exec pin_magic_is_refused", "exec pin_magic_is_refused_unused", "unrun", False),
    ("test does not exist", "text_capture_fold",
     "(pin_analysis_shapes)", "(no_such_test_anywhere)", "unrun", True),
    ("test is FAILED", "used_before_decl",
     "(straight_line_use_before_decl)", "(if_arith_test)", "unrun", True),
]

# Positive cases: these must PASS (exit 0) and say so. The state rule they
# exercise only fires while the worktree copy of shir_native_stmt.rs is dirty
# (another worker mid-edit); on a clean tree they are trivially green, which is
# the point — a clean tree makes the worktree copy and the committed blob the
# same bytes.
PASS_CASES = [
    ("HEAD blob accepted while dirty", "shir_native_pipelines",
     "shir_native_stmt=3ae1b4d0bc71", "shir_native_stmt=3ae1b4d0bc71",
     "HEAD blob of src/transforms/shir_native_stmt.rs"),
    ("worktree copy accepted", "shir_native_find_ls",
     "shir_native_stmt=3ae1b4d0bc71",
     "shir_native_stmt=" + hashlib.sha256(
         open(os.path.join(REPO, "src/transforms/shir_native_stmt.rs"), "rb").read()
     ).hexdigest()[:12],
     "0 failure(s)"),
]


def run(manifest: str, with_tests: bool) -> tuple[int, str]:
    args = [sys.executable, os.path.join(HERE, "check_sync.py"), "-m", manifest]
    if not with_tests:
        args.append("--no-tests")
    r = subprocess.run(args, cwd=REPO, capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def main() -> int:
    ok = True
    for label, name, old, new, want, with_tests in CASES:
        path = os.path.join(HERE, name + "_sync.txt")
        orig = open(path, encoding="utf-8").read()
        digest = hashlib.sha256(orig.encode()).hexdigest()
        if orig.count(old) < 1:
            print(f"MISS {label:22s} fixture text not found: {old!r}")
            ok = False
            continue
        try:
            open(path, "w", encoding="utf-8").write(orig.replace(old, new, 1))
            code, out = run(name, with_tests)
        finally:
            open(path, "w", encoding="utf-8").write(orig)
            restored = hashlib.sha256(open(path, encoding="utf-8").read().encode()).hexdigest()
            assert restored == digest, f"{path} was not restored byte-identically"
        hit = want in out and code == 1
        ok &= hit
        line = next((l for l in out.splitlines() if l.startswith("FAIL")), "(no FAIL line)")
        print(f"{'OK  ' if hit else 'MISS'} {label:22s} exit={code}  {line[:120]}")
    for label, name, old, new, want in PASS_CASES:
        path = os.path.join(HERE, name + "_sync.txt")
        orig = open(path, encoding="utf-8").read()
        digest = hashlib.sha256(orig.encode()).hexdigest()
        if orig.count(old) < 1:
            print(f"MISS {label:22s} fixture text not found: {old!r}")
            ok = False
            continue
        try:
            open(path, "w", encoding="utf-8").write(orig.replace(old, new, 1))
            code, out = run(name, False)
        finally:
            open(path, "w", encoding="utf-8").write(orig)
            restored = hashlib.sha256(open(path, encoding="utf-8").read().encode()).hexdigest()
            assert restored == digest, f"{path} was not restored byte-identically"
        hit = want in out and code == 0
        ok &= hit
        line = next((l for l in out.splitlines() if " ok (" in l), "(no ok line)")
        print(f"{'OK  ' if hit else 'MISS'} {label:22s} exit={code}  {line[:120]}")
    print("all detector cases fired:", ok)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
