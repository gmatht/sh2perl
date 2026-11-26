#!/usr/bin/env python3
"""Check the `proofs/verus/*_sync.txt` manifests against the tree they describe.

A manifest is evidence: "this proof models THIS source, its pins exist, and
these rows were executed". Three things make that evidence rot, and this
checker fails on each:

  1. SHA DRIFT — the recorded `# source sha:` / `proof sha:` no longer match
     the files on disk. A stale source sha means the proof was written
     against a different source; it is NOT repaired here, it is reported.
     A `STALE (…)` annotation on the row is deliberately NOT honoured — an
     annotated row fails exactly like a bare one, because a manifest that no
     longer matches its source must never read as evidence.
  2. PHANTOM PINS — a `pin <name>` row whose proof fn no longer exists in the
     sibling `.rs` (renamed, deleted, or typo'd).
  3. UNRUN EVIDENCE — an `exec <pin> (<test>): PASS` row whose test either no
     longer exists or did not pass in an actual `cargo test --lib` run.

It also enforces the manifest's internal consistency: a `MAPPED` pin row
needs a matching `exec …: PASS` row and vice versa, so a row cannot claim a
test that nobody ran, and a run cannot be claimed for a pin the [pins]
section calls UNMIRRORED.

Usage:
    python3 proofs/verus/check_sync.py                # everything, runs the tests
    python3 proofs/verus/check_sync.py --no-tests     # shas + pins + parity only
    python3 proofs/verus/check_sync.py -m status_reads -v
    python3 proofs/verus/check_sync.py --list

Exit status is 1 if anything failed, 0 otherwise. `--no-tests` says so in the
summary: a checker run without the tests is not evidence of execution.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

# Source keys that are not `{key}.rs` anywhere under src/.
ALIASES = {
    "shir": "src/shir.rs",
    "uae": "src/transforms/unreachable_after_exit.rs",
    "unreachable_after_exit": "src/transforms/unreachable_after_exit.rs",
}

STANDARD_SECTIONS = ("[pins]", "[counts]", "[exec]")

SHA12 = re.compile(r"\b[0-9a-f]{12,40}\b")
KEY_SHA = re.compile(r"\b([A-Za-z0-9_-]+)=([0-9a-f]{8,40})\b")
PROOF_SHA = re.compile(r"proof sha:\s*([0-9a-f]{8,40})")
PIN_ROW = re.compile(
    r"^pin\s+(?P<pin>[A-Za-z0-9_]+)\s*"
    r"(?:(?:->\s*test\s+(?P<test>[A-Za-z0-9_:]+))|(?:->\s*(?P<other>[A-Za-z]+))|"
    r"(?::\s*(?P<bare>[A-Za-z]+)))?"
)
EXEC_ROW = re.compile(
    r"^exec\s+(?P<pin>[A-Za-z0-9_]+)\s*"
    r"(?:\((?P<test>[A-Za-z0-9_:]+)\))?\s*:\s*(?P<verdict>[A-Z]+)"
)
COUNT_ROW = re.compile(r"(\d+)\s+verified,\s*(\d+)\s+errors")


@dataclass
class Failure:
    manifest: str
    kind: str
    detail: str


@dataclass
class Manifest:
    path: str
    name: str
    proof: str                      # sibling .rs path
    standard: bool                  # has [pins]/[counts]/[exec]
    proof_sha: str | None = None
    source_shas: dict[str, str] = field(default_factory=dict)
    pins: dict[str, str] = field(default_factory=dict)      # pin -> MAPPED/UNMIRRORED
    pin_tests: dict[str, str] = field(default_factory=dict)  # pin -> test name
    execs: dict[str, str] = field(default_factory=dict)     # pin -> PASS/UNMIRRORED/…
    exec_tests: dict[str, str] = field(default_factory=dict)
    counts: tuple[int, int] | None = None          # (verified, errors)
    proof_fns: int | None = None                    # old dialect
    pin_rows: int | None = None                     # old dialect


def parse_manifest(path: str) -> Manifest:
    name = os.path.basename(path)[: -len("_sync.txt")]
    m = Manifest(
        path=path,
        name=name,
        proof=os.path.join(os.path.dirname(path), name + ".rs"),
        standard=False,
    )
    text = open(path, encoding="utf-8", errors="replace").read()
    m.standard = all(sec in text for sec in STANDARD_SECTIONS)
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            if "sha" in stripped:
                ps = PROOF_SHA.search(stripped)
                if ps:
                    m.proof_sha = ps.group(1)
                hits = list(KEY_SHA.finditer(stripped))
                for hit in hits:
                    key, sha = hit.group(1), hit.group(2)
                    if key == "proof":
                        continue
                    m.source_shas[key] = sha
            continue
        pm = PIN_ROW.match(stripped)
        if pm:
            pin = pm.group("pin")
            verdict = pm.group("bare") or pm.group("other") or "MAPPED"
            # `pin x -> test y: MAPPED` — the verdict follows the test name.
            tail = stripped[pm.end():].lstrip(" :")
            if tail:
                word = re.match(r"([A-Za-z]+)", tail)
                if word:
                    verdict = word.group(1)
            m.pins[pin] = verdict.upper()
            if pm.group("test"):
                m.pin_tests[pin] = pm.group("test")
            continue
        em = EXEC_ROW.match(stripped)
        if em:
            m.execs[em.group("pin")] = em.group("verdict").upper()
            if em.group("test"):
                m.exec_tests[em.group("pin")] = em.group("test")
            continue
        cm = COUNT_ROW.search(stripped)
        if cm:
            m.counts = (int(cm.group(1)), int(cm.group(2)))
            continue
        ff = re.search(r"proof-fns:\s*(\d+)", stripped)
        if ff:
            m.proof_fns = int(ff.group(1))
            continue
        pr = re.search(r"\bpins:\s*(\d+)", stripped)
        if pr:
            m.pin_rows = int(pr.group(1))
    return m


SHA_LEN = 12


def sha256_full(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_prefix(path: str, n: int = SHA_LEN) -> str:
    return sha256_full(path)[:n]


def digest_matches(recorded: str, path: str) -> bool:
    """The ONE way a recorded sha matches: it must be the leading 12+ hex
    characters of THIS file's sha256 (or the full 64-char digest).

    There is deliberately no second rule: no alternate digest of the same
    file, no shortened stub, no annotation, nothing that could let a row
    that no longer describes the file read as a match.
    """
    if not re.fullmatch(r"[0-9a-f]{12,64}", recorded or ""):
        return False
    return sha256_full(path).startswith(recorded)


def resolve_source(key: str) -> str | None:
    """Map a manifest source key onto a file in src/ (or an alias).

    Keys may name a REGION of a bigger file (`c_backend-walkers`), so a
    dash-separated key falls back to its head as the file stem.
    """
    if key in ALIASES:
        p = os.path.join(REPO, ALIASES[key])
        return p if os.path.exists(p) else None
    exact = os.path.join(REPO, "src", key + ".rs")
    if os.path.exists(exact):
        return exact
    exacts = [
        os.path.join(root, key + ".rs")
        for root, _dirs, files in os.walk(os.path.join(REPO, "src"))
        if key + ".rs" in files
    ]
    if len(exacts) == 1:
        return exacts[0]
    found: list[str] = []
    for root, _dirs, files in os.walk(os.path.join(REPO, "src")):
        for f in files:
            if not f.endswith(".rs"):
                continue
            stem = f[:-3]
            if stem == key or stem == key + "_fold" or stem.startswith(key + "_"):
                found.append(os.path.join(root, f))
    if len(found) == 1:
        return found[0]
    if "-" in key:
        # a region key: `c_backend-walkers` describes a part of c_backend.rs
        for head in key.split("-")[:-1]:
            got = resolve_source(head)
            if got:
                return got
    return None


def run_lib_tests(jobs: int) -> tuple[dict[str, list[str]], str | None]:
    """Run `cargo test --lib` once; return {test basename: [verdicts]}.

    A basename can exist in several modules (`if_arith_test` is in more than
    one backend), and a manifest row does not always name the module, so the
    list keeps every instance and the caller demands that ALL of them pass —
    an ambiguous row must not be satisfied by a different module's copy.
    """
    # NB: no `-q` — cargo's quiet mode also silences libtest's per-test lines,
    # and those lines are the evidence this checker exists to read.
    cmd = ["cargo", "test", "--lib", "--", "--test-threads", str(jobs)]
    print(f"$ {' '.join(cmd)}   (in {REPO})", flush=True)
    try:
        proc = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, timeout=3600)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {}, f"cargo test could not run: {exc}"
    out = proc.stdout + proc.stderr
    verdicts: dict[str, list[str]] = {}
    # Unanchored: with several test threads libtest interleaves output, so a
    # verdict can share a line with another test's name.
    pat = re.compile(r"test\s+(\S+)\s+\.\.\.\s+(ok|FAILED|ignored)")
    for full, verdict in pat.findall(out):
        verdicts.setdefault(full.split("::")[-1], []).append(verdict)
    if not verdicts:
        tail = "\n".join(out.strip().splitlines()[-12:])
        return {}, f"cargo test produced no per-test verdicts. Tail:\n{tail}"
    flat = [v for vs in verdicts.values() for v in vs]
    print(f"  parsed {len(flat)} test verdicts in {len(verdicts)} tests "
          f"({sum(v == 'ok' for v in flat)} ok)", flush=True)
    return verdicts, None


skipped: list[tuple[str, str]] = []


def check(m: Manifest, verdicts: dict[str, list[str]] | None, verbose: bool) -> list[Failure]:
    fails: list[Failure] = []

    def bad(kind: str, detail: str) -> None:
        fails.append(Failure(m.name, kind, detail))

    if not os.path.exists(m.proof):
        bad("missing-proof", f"{os.path.basename(m.proof)} does not exist")
        return fails

    if not m.standard:
        skipped.append(
            (m.name, "no [pins]/[counts]/[exec] sections — a generated fold "
                     "table, not a manifest; left to its generator")
        )
        return fails

    proof_text = open(m.proof, encoding="utf-8", errors="replace").read()

    # 1. shas
    if m.proof_sha is None:
        bad("sha", "no `proof sha:` line")
    else:
        actual = sha256_prefix(m.proof)
        if not digest_matches(m.proof_sha, m.proof):
            bad("sha", f"proof sha {m.proof_sha} != sha256 {actual} of {os.path.basename(m.proof)}")
        elif verbose:
            print(f"  {m.name}: proof sha ok ({actual})")
    if not m.source_shas:
        bad("sha", "no `# source sha:` line")
    for key, recorded in m.source_shas.items():
        src = resolve_source(key)
        if src is None:
            bad("source", f"cannot resolve source key `{key}` to a file under src/")
            continue
        actual = sha256_prefix(src)
        rel = os.path.relpath(src, REPO)
        if digest_matches(recorded, src):
            if verbose:
                print(f"  {m.name}: {key} ok ({actual} = sha256 of {rel})")
            continue
        shape = (
            ""
            if re.fullmatch(r"[0-9a-f]{12,64}", recorded)
            else " (and the recorded value is not a 12+ hex digest)"
        )
        bad(
            "stale-source",
            f"{key}={recorded}: {rel} is sha256 {actual}{shape}"
            " — the proof was written against a different source; re-audit it, "
            "do not just bump the sha (an annotation on the row changes nothing: "
            "there is no admitted-staleness escape hatch)",
        )

    # 2. pins exist as proof fns
    if not m.pins:
        bad("pins", "[pins] section has no rows")
    for pin in m.pins:
        if not re.search(rf"\bfn\s+{re.escape(pin)}\s*\(", proof_text):
            bad("phantom-pin", f"`pin {pin}` names no `fn {pin}(` in {os.path.basename(m.proof)}")

    # 3. mapped/exec parity
    mapped = {p for p, v in m.pins.items() if v == "MAPPED"}
    passed = {p for p, v in m.execs.items() if v == "PASS"}
    for pin in sorted(mapped - passed):
        bad("unrun", f"`pin {pin}` is MAPPED but has no `exec …: PASS` row")
    for pin in sorted(passed - mapped):
        bad("bogus-exec", f"`exec {pin}: PASS` has no MAPPED row in [pins]")
    for pin, verdict in m.execs.items():
        if pin not in m.pins:
            bad("bogus-exec", f"`exec {pin}` names a pin the [pins] section does not list")
        elif verdict == "UNMIRRORED" and m.pins[pin] != "UNMIRRORED":
            bad("parity", f"`exec {pin}: UNMIRRORED` contradicts `pin {pin}: {m.pins[pin]}`")
    # a MAPPED row must name the test it was mapped to
    for pin in mapped:
        if pin not in m.pin_tests and pin not in m.exec_tests:
            bad("unrun", f"`pin {pin}: MAPPED` names no test")

    # 4. the tests named by PASS rows really ran and passed
    if verdicts is None:
        pass
    else:
        for pin, verdict in sorted(m.execs.items()):
            if verdict != "PASS":
                continue
            test = m.exec_tests.get(pin) or m.pin_tests.get(pin)
            if not test:
                bad("unrun", f"`exec {pin}: PASS` names no test")
                continue
            base = test.split("::")[-1]
            got = verdicts.get(base)
            if got is None:
                bad("unrun", f"`exec {pin}: PASS` names test `{test}`, which did not run")
            elif all(v == "ok" for v in got):
                pass
            elif len(got) > 1:
                bad(
                    "unrun",
                    f"`exec {pin}: PASS` names `{test}`, an ambiguous basename "
                    f"({len(got)} modules) with verdicts {got}",
                )
            else:
                bad("unrun", f"`exec {pin}: PASS` names test `{test}`, which is {got[0]}")

    # 5. counts: either the Verus dialect or the file-shape dialect, checked
    # against the file it describes.
    if m.counts is not None:
        if m.counts[1] != 0:
            bad("counts", f"[counts] claims {m.counts[1]} errors; a green manifest must claim 0")
    elif m.proof_fns is not None:
        actual_fns = len(
            re.findall(r"^\s*(?:pub\s+)?(?:open\s+)?proof\s+fn\s+\w+", proof_text, re.M)
        )
        if actual_fns != m.proof_fns:
            bad(
                "counts",
                f"[counts] says proof-fns: {m.proof_fns}, but the proof declares {actual_fns}",
            )
        if m.pin_rows is not None and m.pin_rows != len(m.pins):
            bad("counts", f"[counts] says pins: {m.pin_rows}, but [pins] has {len(m.pins)} rows")
    else:
        bad("counts", "[counts] states neither `N verified, M errors` nor `proof-fns: N`")

    return fails


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("-m", "--manifest", action="append", default=[], help="manifest stem, e.g. status_reads")
    ap.add_argument("-k", "--filter", default=None, help="substring filter on manifest stems")
    ap.add_argument("--no-tests", action="store_true", help="skip `cargo test --lib` (NOT evidence of execution)")
    ap.add_argument("--jobs", type=int, default=min(8, (os.cpu_count() or 2)))
    ap.add_argument("-v", "--verbose", action="store_true")
    ap.add_argument("--list", action="store_true", help="list manifests and exit")
    args = ap.parse_args()

    paths = sorted(
        os.path.join(HERE, f) for f in os.listdir(HERE) if f.endswith("_sync.txt")
    )
    if args.list:
        for p in paths:
            print(os.path.basename(p)[: -len("_sync.txt")])
        return 0
    if args.manifest:
        wanted = set(args.manifest)
        paths = [p for p in paths if os.path.basename(p)[: -len("_sync.txt")] in wanted]
    if args.filter:
        paths = [p for p in paths if args.filter in os.path.basename(p)]
    if not paths:
        print("no manifests selected", file=sys.stderr)
        return 1

    manifests = [parse_manifest(p) for p in paths]
    verdicts: dict[str, list[str]] | None = None
    if not args.no_tests:
        verdicts, err = run_lib_tests(args.jobs)
        if err:
            print(f"\nTEST RUN FAILED: {err}\n", file=sys.stderr)
            verdicts = {}

    failures: list[Failure] = []
    for m in manifests:
        if args.verbose:
            print(f"[{m.name}]", flush=True)
        failures.extend(check(m, verdicts, args.verbose))

    print()
    for name, why in skipped:
        print(f"SKIP {name}: {why}")
    for f in failures:
        print(f"FAIL {f.manifest}: [{f.kind}] {f.detail}")
    mode = "WITHOUT the test run (not evidence of execution)" if verdicts is None else "with the test run"
    print(
        f"\n{len(manifests)} manifests checked {mode}: "
        f"{len(failures)} failure(s), {len(skipped)} skipped"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
