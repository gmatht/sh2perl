#!/usr/bin/env python3
"""A.2: the fold list feeding `transform_stmt` must be COMPLETE.

`shir_native_stmt.rs` is a dispatch over ~60 independent folds. No law about an
individual fold can see a fold that was dropped from (or never added to) the
dispatch, so the plan's structural claim A.2 is checked here instead of proved:
the committed list `shir_native_stmt_dispatch.txt` and the source must agree in
BOTH directions.

  * DEFINED-BUT-UNDISPATCHED — a fold of the shape
    `fn NAME(st: &mut IrStmt) -> bool` that is never called from the dispatch
    (`transform` / `transform_stmt` / `transform_expr`). This is the failure a
    law cannot catch: the fold still exists, still verifies, and never runs.
  * DISPATCHED-BUT-UNDEFINED — a call in the dispatch to a name that is not a
    fold definition (a rename left a stale call, or the list is stale).
  * LIST DRIFT — the committed list differs from the definitions found, so a
    newly added or renamed fold is a deliberate, reviewable edit rather than a
    silent one.

Usage:
    python3 proofs/verus/check_dispatch.py                     # the tree
    python3 proofs/verus/check_dispatch.py --source COPY.rs    # a copy (mutation test)

Exit status is 1 on any failure, 0 otherwise.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

SOURCE = os.path.join(REPO, "src", "transforms", "shir_native_stmt.rs")
LIST = os.path.join(HERE, "shir_native_stmt_dispatch.txt")

FOLD_DEF = re.compile(r"^fn\s+(\w+)\s*\(\s*st:\s*&mut\s+IrStmt\s*\)\s*->\s*bool", re.M)
CALL = re.compile(r"\b(\w+)\s*\(\s*st\s*\)")


def dispatch_region(src: str) -> str:
    """The three functions that decide which fold runs on a statement."""
    start = src.find("fn transform_stmt(")
    if start < 0:
        return ""
    # through the end of transform_expr: the last of the three dispatch fns
    end = src.find("\nfn ", src.find("fn transform_expr("))
    if end < 0:
        end = len(src)
    return src[start:end]


def read_list(path: str) -> list[str]:
    out = []
    for line in open(path):
        line = line.split("#", 1)[0].strip()
        if line:
            out.append(line)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default=SOURCE)
    ap.add_argument("--list", default=LIST)
    ns = ap.parse_args()

    src = open(ns.source).read()
    defined = sorted(set(FOLD_DEF.findall(src)))
    region = dispatch_region(src)
    dispatched = set(CALL.findall(region))
    listed = read_list(ns.list)

    failures: list[str] = []

    undisp = [f for f in defined if f not in dispatched]
    for f in undisp:
        failures.append(f"DEFINED-BUT-UNDISPATCHED {f}")

    for c in sorted(dispatched - set(defined)):
        failures.append(f"DISPATCHED-BUT-UNDEFINED {c}")

    for f in listed:
        if f not in defined:
            failures.append(f"LIST-DRIFT (listed, not defined) {f}")
    for f in defined:
        if f not in listed:
            failures.append(f"LIST-DRIFT (defined, not listed) {f}")

    print(f"folds defined: {len(defined)}, listed: {len(listed)}, "
          f"called from the dispatch: {len(dispatched)}")
    for msg in failures:
        print(f"  FAIL {msg}")
    if failures:
        print(f"{len(failures)} failure(s)")
        return 1
    print("0 failures: every fold is listed and dispatched, and every dispatch "
          "call names a fold")
    return 0


if __name__ == "__main__":
    sys.exit(main())
