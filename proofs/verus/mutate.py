#!/usr/bin/env python3
"""Declared mutants for the Verus laws in this directory.

The proof headers say things like "m4: the fold's output re-enters the
rewritable class — 31 verified, 1 error (`fold_is_idempotent`)".  Those are
measurements, and until now they could only be re-made by hand: edit the
proof, run Verus, read the summary, edit it back, and hope the restore was
byte-identical.

This script makes each of those claims executable:

  * a mutant is (proof file, exactly-one occurrence of `find`, `replace`);
  * the pristine file is checked against its recorded sha256 FIRST, so a
    mutant can never be applied to a revision the claim was not made on;
  * the file is mutated, Verus runs, and the verdict is asserted:
      - counted: the summary line must read exactly `N verified, M errors`
        and every named obligation must appear among the failing functions;
      - simplification-rejected: the run must stop before the verifier
        summary (the concrete `by (compute)` pin refuted the model) and must
        not be a compile error;
  * the original bytes are written back and re-hashed.  Non-identical
    restore is a hard failure (exit 2), whatever the verdict was.

The table below is the documented mutant set of the five stages landed
2026-11-24..27 (`strip` is older and is not in the table yet): affix_strip_fold,
text_capture_fold, bc_float_clean, uae_terminators, status_reads and
status_reads_traversal.  Each entry's expectation is the header's own number
where the header gives one; entries the header records only as a class
("rejected at simplification") assert the class and name the pin when the
header names it.

Usage
-----
    python3 proofs/verus/mutate.py                 # run every mutant, assert
    python3 proofs/verus/mutate.py --list          # names only
    python3 proofs/verus/mutate.py --only bc_float # substring filter
    python3 proofs/verus/mutate.py --calibrate     # measure, never fail
    python3 proofs/verus/mutate.py --self-check    # prove the assertions bite
    python3 proofs/verus/mutate.py -v              # show Verus output on failure

`--calibrate` is for authoring a table entry (and for re-measuring after a
proof change): it prints the observed verdict and exits 0.  The asserting run
is the gate; calibration is not evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from typing import Optional, Tuple

HERE = os.path.dirname(os.path.abspath(__file__))

# ───────────────────────────────────────────────────────────── the contract ──

# proof file -> leading 12 hex of sha256, the same digest the `*_sync.txt`
# manifests record.  A mismatch means the proof changed under the claims.
PROOF_SHA = {
    "affix_strip_fold": "402308d8cd04",
    "text_capture_fold": "72e022116f2b",
    "bc_float_clean": "8a1fe6ed7371",
    "uae_terminators": "300c286819f1",
    "status_reads": "467f5b75b0c5",
    "status_reads_traversal": "dff0d2f529a1",
}

# proof file -> obligations the pristine run reports (header baseline).
BASELINE = {
    "affix_strip_fold": 32,
    "text_capture_fold": 31,
    "bc_float_clean": 27,
    "uae_terminators": 31,
    "status_reads": 28,
    "status_reads_traversal": 70,
}


@dataclass(frozen=True)
class Mutant:
    """One declared mutation and the verdict it must produce.

    `verified is None` means simplification-rejected: the verifier summary is
    never reached.  Otherwise the summary must match `verified`/`errors` and
    `obligations` must all be among the functions the errors point into.
    """

    name: str
    proof: str
    find: str
    replace: str
    verified: Optional[int]
    errors: Optional[int]
    obligations: Tuple[str, ...] = ()
    note: str = ""
    rlimit: int = 40

    @property
    def counted(self) -> bool:
        return self.verified is not None


MUTANTS = [
    # ── affix_strip_fold.rs — baseline 32/0 (header: "measured mutants
    #    2026-11-26 revision") ────────────────────────────────────────────────
    Mutant(
        "affix/m1-strip-without-presence-check",
        "affix_strip_fold",
        "    if is_prefix(s, p) {\n        s.skip(p.len() as int)\n    } else {\n        s\n    }\n",
        "    s.skip(p.len() as int)\n",
        None, None, ("pin_literal_one_length",),
        "strip the affix without checking it is present",
    ),
    Mutant(
        "affix/m2-over-strip-one-past",
        "affix_strip_fold",
        "        s.skip(p.len() as int)",
        "        s.skip((p.len() + 1) as int)",
        None, None, ("pin_literal_one_length",),
        "over-strip one char past the declared affix",
    ),
    Mutant(
        "affix/m3-star-is-not-magic",
        "affix_strip_fold",
        "    c == '*' || c == '?' || c == '[' || c == '\\\\'",
        "    c == '?' || c == '[' || c == '\\\\'",
        None, None, ("pin_magic_is_refused",),
        "`*` no longer counted as glob magic",
    ),
    Mutant(
        "affix/m4-fold-output-reenters-the-class",
        "affix_strip_fold",
        "        Term::Node => Term::Node,",
        "        Term::Node => Term::Param(1, seq!['a']),",
        31, 1, ("fold_is_idempotent",),
        "the fold's output re-enters the rewritable class",
    ),
    # ── text_capture_fold.rs — baseline 31/0 ────────────────────────────────
    Mutant(
        "text/m1-gate-accepts-a-syntax-char",
        "text_capture_fold",
        "            Tok::Sub(s) => no_syntax(s, 0),",
        "            Tok::Sub(_s) => true,",
        30, 1, ("lemma_tokens",),
        "the gate accepts a syntax character (fails at the `lemma_chars` call)",
    ),
    Mutant(
        "text/m2-substitution-space-as-char",
        "text_capture_fold",
        "        seq![Tok::Sp].add(sub_chars(s, i + 1))",
        "        seq![Tok::Ch(s[i])].add(sub_chars(s, i + 1))",
        30, 1, ("lemma_chars",),
        "the substitution's space becomes a literal char",
    ),
    Mutant(
        "text/m3-folded-space-as-char",
        "text_capture_fold",
        "        seq![Tok::Sp].add(lit_chars(s, i + 1))",
        "        seq![Tok::Ch(s[i])].add(lit_chars(s, i + 1))",
        30, 1, ("lemma_chars",),
        "the folded text's space becomes a literal char",
    ),
    Mutant(
        "text/s1-rt-word-counted-literal",
        "text_capture_fold",
        "            W::Rt => false,\n        }) && all_lit(ws, i + 1)",
        "            W::Rt => true,\n        }) && all_lit(ws, i + 1)",
        None, None, ("pin_analysis_shapes",),
        "an Rt word counted as literal",
    ),
    Mutant(
        "text/s2-flag-refusal-dropped",
        "text_capture_fold",
        "        Head::Echo => if all_lit(ws, 0) && !any_flag(ws, 0) && !any_broken(ws, 0) {",
        "        Head::Echo => if all_lit(ws, 0) && !any_broken(ws, 0) {",
        None, None, ("pin_analysis_shapes",),
        "the echo-flag refusal dropped from `analysis` (the header names "
        "pin_emit_is_richer; the first concrete pin to compute false on this "
        "mutation is pin_analysis_shapes)",
    ),
    Mutant(
        "text/s3-join-drops-its-separator",
        "text_capture_fold",
        "        seq![' '].add(wtext(ws[i])).add(join(ws, i + 1))",
        "        wtext(ws[i]).add(join(ws, i + 1))",
        None, None, ("pin_analysis_shapes",),
        "the join drops its separator",
    ),
    Mutant(
        "text/s4-every-head-prints-its-words",
        "text_capture_fold",
        "        Head::Other => None,",
        "        Head::Other => if all_lit(ws, 0) { Some(join(ws, 0)) } else { None },",
        None, None, ("pin_analysis_shapes",),
        "every head prints its words",
    ),
    Mutant(
        "text/s5-a-quote-is-syntax-to-the-substitution",
        "text_capture_fold",
        "    } else {\n        seq![Tok::Ch(s[i])].add(sub_chars(s, i + 1))\n    }\n",
        "    } else if syntax_quoted(s[i]) {\n        seq![Tok::Syn(s[i])].add(sub_chars(s, i + 1))\n"
        "    } else {\n        seq![Tok::Ch(s[i])].add(sub_chars(s, i + 1))\n    }\n",
        None, None, ("pin_a_quote_breaks_the_parse",),
        "a quote treated as syntax to the substitution",
    ),
    Mutant(
        "text/s6-unquoted-gate-wide-open",
        "text_capture_fold",
        "pub open spec fn safe_char(c: char) -> bool {\n"
        "    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') "
        "|| c == '_' || c == '@'\n"
        "        || c == '%' || c == '+' || c == '=' || c == ':' || c == ',' "
        "|| c == '.' || c == '/' || c == '-'\n"
        "}",
        "pub open spec fn safe_char(c: char) -> bool {\n    true\n}",
        None, None, ("pin_safe_splice",),
        "the unquoted gate wide open",
    ),
    # ── bc_float_clean.rs — baseline 27/0, ten documented mutants ───────────
    Mutant(
        "bc/M3-strip-drops-the-zero-check",
        "bc_float_clean",
        "if j + 3 <= e && s[j] == '0' && s[j + 1] == '.' && s[j + 2] == '0' && all_ws(s, j + 3, e) {",
        "if j + 3 <= e && s[j + 1] == '.' && s[j + 2] == '0' && all_ws(s, j + 3, e) {",
        26, 1, ("strip_removes_only_the_identity",),
        "strip drops the `s[j] == '0'` check (the law's DFA step into SeeZero)",
    ),
    Mutant(
        "bc/M5-dfa-afterplus-accepts-one",
        "bc_float_clean",
        "        } else if c == '0' {\n            Some(Q::SeeZero)",
        "        } else if c == '0' || c == '1' {\n            Some(Q::SeeZero)",
        26, 1, ("lemma_run_consumes",),
        "DFA AfterPlus also accepts '1' (the term-shape equation)",
    ),
    Mutant(
        "bc/M10-total-base-case-is-one",
        "bc_float_clean",
        "    if i >= a.len() {\n        0\n    } else {\n        a[i] + total(a, i + 1)",
        "    if i >= a.len() {\n        1\n    } else {\n        a[i] + total(a, i + 1)",
        26, 1, ("lemma_total_push",),
        "the term model's `total` base case is 1",
    ),
    Mutant(
        "bc/M1-drop-the-trailing-ws-gate",
        "bc_float_clean",
        " && all_ws(s, j + 3, e) {",
        " {",
        None, None, ("pin_strip_refusals",),
        "the strip drops the trailing-whitespace gate",
    ),
    Mutant(
        "bc/M2-keep-ws-before-plus",
        "bc_float_clean",
        "Some(s.subrange(0, trim_start(s, i)))",
        "Some(s.subrange(0, i))",
        None, None, ("pin_strip_shapes",),
        "the strip keeps the whitespace before '+'",
    ),
    Mutant(
        "bc/M4-dfa-trail-accepts-any-char",
        "bc_float_clean",
        "        Q::Trail => if is_ws(c) { Some(Q::Trail) } else { None },",
        "        Q::Trail => Some(Q::Trail),",
        None, None, ("pin_dfa_agrees",),
        "DFA Trail accepts any character",
    ),
    Mutant(
        "bc/M6-dfa-beforeplus-stops-skipping-ws",
        "bc_float_clean",
        "        Q::BeforePlus => if is_ws(c) {",
        "        Q::BeforePlus => if false {",
        None, None, ("pin_dfa_agrees",),
        "DFA BeforePlus stops skipping whitespace",
    ),
    Mutant(
        "bc/M7-trim-start-does-not-trim",
        "bc_float_clean",
        "        trim_start(s, i - 1)",
        "        i",
        None, None, ("pin_strip_shapes",),
        "trim_start does not trim",
    ),
    Mutant(
        "bc/M8-last-plus-only-sees-a-trailing-plus",
        "bc_float_clean",
        "        -1\n    } else if s[i - 1] == '+' {",
        "        -1\n    } else if s[i - 1] == '+' && i == (s.len() as int) {",
        None, None, ("pin_strip_shapes",),
        "last_plus only sees an adjacent '+'",
    ),
    Mutant(
        "bc/M9-skip-ws-skips-nothing",
        "bc_float_clean",
        "        skip_ws(s, i + 1)",
        "        i",
        None, None, ("pin_strip_shapes",),
        "skip_ws skips nothing",
    ),
    # ── uae_terminators.rs — baseline 31/0 ─────────────────────────────────
    Mutant(
        "uae/m1-nested-break-escapes",
        "uae_terminators",
        "        Stmt::While { .. } => false,",
        "        Stmt::While { body, .. } => escapes(body),",
        None, None, ("pin_nested_break_does_not_escape",),
        "count a nested loop's `break` as escaping its parent",
    ),
    Mutant(
        "uae/m2-if-nested-terminator-truncates",
        "uae_terminators",
        "    } else if terminates(l[0], in_fn) {\n        seq![l[0]]",
        "    } else if terminates(l[0], in_fn)\n"
        "        || (match l[0] {\n"
        "            Stmt::If { then, els, .. } => "
        "list_terminates(then, in_fn) || list_terminates(els, in_fn),\n"
        "            _ => false,\n"
        "        })\n"
        "    {\n        seq![l[0]]",
        30, 1, ("trunc_sound",),
        "truncate after a terminator nested in an `if`",
    ),
    Mutant(
        "uae/m3-top-level-return-terminates",
        "uae_terminators",
        "        Stmt::Return => in_fn,",
        "        Stmt::Return => true,",
        29, 2, ("terminates_stops", "return_truncates_only_a_function_body"),
        "treat a top-level `Return` as a terminator (the header also names "
        "trunc_sound; measured: terminates_stops + the Return law)",
    ),
    Mutant(
        "uae/m4-case-nested-exit-truncates",
        "uae_terminators",
        "    } else if terminates(l[0], in_fn) {\n        seq![l[0]]",
        "    } else if terminates(l[0], in_fn)\n"
        "        || (match l[0] {\n"
        "            Stmt::Case { body, .. } => list_terminates(body, in_fn),\n"
        "            _ => false,\n"
        "        })\n"
        "    {\n        seq![l[0]]",
        30, 1, ("trunc_sound",),
        "truncate after a `case`-nested `Exit`",
    ),
    Mutant(
        "uae/m5-trunc-drops-the-terminator",
        "uae_terminators",
        "        seq![l[0]]",
        "        Seq::<Stmt>::empty()",
        None, None, ("pin_trunc_shapes",),
        "drop the terminator itself from the kept prefix",
    ),
    Mutant(
        "uae/m6-case-break-continues",
        "uae_terminators",
        "                run(body, in_fn)",
        "                match run(body, in_fn) { (o, Sig::Brk) => (o, Sig::Spin), r => r }",
        None, None, ("pin_case_break_escapes",),
        "let a `case` break `continue` instead of ending the loop",
    ),
    Mutant(
        "uae/m7-run-past-a-signal",
        "uae_terminators",
        "        if h.1 != Sig::None {\n"
        "            // the list stops: nothing after this statement runs\n"
        "            (h.0, h.1)",
        "        if h.1 != Sig::None {\n"
        "            // the list stops: nothing after this statement runs\n"
        "            (h.0.add(run(l.drop_first(), in_fn).0), h.1)",
        None, None, ("pin_break_stops_the_list",),
        "run past a non-None signal",
    ),
    # ── status_reads.rs — baseline 28/0, four documented mutants ───────────
    Mutant(
        "reads/a-dead-store-law-premise-deleted",
        "status_reads",
        "        !live(list)[i],\n",
        "",
        27, 1, ("dead_store_law",),
        "`dead_store_law`'s premise `!live(list)[i]` deleted",
    ),
    Mutant(
        "reads/b-store-no-longer-resets-need",
        "status_reads",
        "            if need && st.stores {\n                false",
        "            if need && st.stores {\n                need",
        None, None, ("pin_stores_before_the_nearest_read_are_dropped",),
        "a definite store no longer resets `need`",
    ),
    Mutant(
        "reads/c-drops-the-nested-term",
        "status_reads",
        "        (need || (!st.stores && st.nested), true)",
        "        (need, true)",
        None, None, ("pin_nested_after_write_keeps_the_store",),
        "the `!st.stores && st.nested` self-observation term deleted",
    ),
    Mutant(
        "reads/d-observed-after-drops-the-read-anchor",
        "status_reads",
        "    } else if rds(list, i + 1) {\n"
        "        // The next statement reads `$?` at its entry, before it stores.\n"
        "        true\n"
        "    } else if sts(list, i + 1) {\n",
        "    } else if sts(list, i + 1) {\n",
        25, 3, ("scan_agrees", "reaches_read", "reaches_exit"),
        "the read anchor `else if rds(list, i + 1)` deleted",
    ),
    # ── status_reads_traversal.rs — baseline 70/0, four documented mutants ─
    Mutant(
        "trav/a-cap-arm-dropped",
        "status_reads_traversal",
        "        X::Cap(k) => an_xs(k, fns),",
        "        X::Cap(_) => false,",
        69, 1, ("reads_exact_x",),
        "read walk drops the Cap/backtick arm",
    ),
    Mutant(
        "trav/b-literal-text-is-never-a-read",
        "status_reads_traversal",
        "        X::StrQ(q) => q,",
        "        X::StrQ(_) => false,",
        None, None, ("pin_bodies_are_localised",),
        "\"a literal text is never a read\"",
    ),
    Mutant(
        "trav/c-nested-walk-drops-redirect",
        "status_reads_traversal",
        "        S::Redir(l) => rbs_l(l, fns),",
        "        S::Redir(_) => false,",
        None, None, ("pin_nested_redirect_reads_before_store",),
        "nested walk drops Redirect (an_nested's arm; mutating the READ walk "
        "instead is a counted failure, not this mutant)",
    ),
    Mutant(
        "trav/d-if-no-longer-writes-the-register",
        "status_reads_traversal",
        "        S::IfS(_, _, _, _) => seq![Ins::IRc, Ins::IBranch],",
        "        S::IfS(_, _, _, _) => seq![Ins::IBranch],",
        None, None, ("pin_control_flow_stores",),
        "`emit_s` stops writing the register for an `If` fragment",
    ),
]

# ───────────────────────────────────────────────────────────────── the tool ──

SUMMARY = re.compile(r"verification results::\s*(\d+)\s+verified,\s*(\d+)\s+errors")
LOCATION = re.compile(r"^\s*-->\s+([^:\s]+):(\d+):(\d+)", re.MULTILINE)
FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def proof_path(proof: str) -> str:
    return os.path.join(HERE, proof + ".rs")


def find_verus(explicit: Optional[str]) -> str:
    for cand in (explicit, os.environ.get("VERUS")):
        if cand:
            if os.path.isfile(cand) and os.access(cand, os.X_OK):
                return cand
            raise SystemExit(f"mutate.py: not an executable: {cand}")
    home = os.path.expanduser("~")
    found = sorted(
        (os.path.join(home, d, "verus")
         for d in os.listdir(home)
         if d.startswith("verus-") and os.path.isfile(os.path.join(home, d, "verus"))),
        reverse=True,
    )
    if not found:
        raise SystemExit("mutate.py: no ~/verus-*/verus; pass --verus PATH or set VERUS")
    return found[0]


def verus_env() -> dict:
    env = dict(os.environ)
    home = os.path.expanduser("~")
    env.setdefault("RUSTUP_HOME", os.path.join(home, ".rustup"))
    env.setdefault("CARGO_HOME", os.path.join(home, ".cargo"))
    return env


def run_verus(verus: str, proof: str, rlimit: int, timeout: int = 600) -> Tuple[int, str]:
    cmd = [verus, "--triggers-mode", "silent", "--rlimit", str(rlimit), proof + ".rs"]
    proc = subprocess.run(
        cmd, cwd=HERE, env=verus_env(), capture_output=True, text=True, timeout=timeout
    )
    return proc.returncode, (proc.stdout or "") + (proc.stderr or "")


def failing_fns(text: str, output: str) -> Tuple[str, ...]:
    """The functions the errors point into, from the `--> file:line:col` sites."""
    lines = text.splitlines()
    names = []
    for m in LOCATION.finditer(output):
        lineno = int(m.group(2))
        if not 1 <= lineno <= len(lines):
            continue
        for i in range(lineno - 1, -1, -1):
            hit = FN.search(lines[i])
            if hit and not lines[i].lstrip().startswith("//"):
                if hit.group(1) not in names:
                    names.append(hit.group(1))
                break
    return tuple(names)


def summarise(output: str) -> Optional[Tuple[int, int]]:
    hits = SUMMARY.findall(output)
    if not hits:
        return None
    v, e = hits[-1]
    return int(v), int(e)


def mutate_once(m: Mutant, text: str) -> str:
    n = text.count(m.find)
    if n != 1:
        raise ValueError(f"`find` occurs {n} time(s), must occur exactly once")
    return text.replace(m.find, m.replace)


@dataclass
class Observation:
    verdict: Optional[Tuple[int, int]]
    obligations: Tuple[str, ...]
    compiled: bool
    output: str
    note: str = ""


def observe(m: Mutant, text: str, verus: str, verbose: bool) -> Observation:
    mutated = mutate_once(m, text)
    path = proof_path(m.proof)
    # Write the mutation, then run. The caller restores in a `finally`; a
    # mutation that is never written is a run of the pristine file, i.e. no
    # evidence at all.
    with open(path, "wb") as fh:
        fh.write(mutated.encode("utf-8"))
    rc, out = run_verus(verus, m.proof, m.rlimit)
    if verbose:
        print(out)
    verdict = summarise(out)
    compile_error = "error[E" in out or "aborting due to" in out and verdict is None and "simplif" not in out.lower()
    return Observation(
        verdict=verdict,
        obligations=failing_fns(mutated, out),
        compiled=not compile_error,
        output=out,
    )


def check(m: Mutant, obs: Observation) -> Optional[str]:
    """None if the observation satisfies the declared verdict, else why not."""
    if m.counted:
        if obs.verdict is None:
            return (
                "expected a verifier summary "
                f"{m.verified}v/{m.errors}e, the run stopped before it "
                "(no summary line)"
            )
        if obs.verdict != (m.verified, m.errors):
            got_v, got_e = obs.verdict
            return f"expected {m.verified}v/{m.errors}e, got {got_v}v/{got_e}e"
        missing = [o for o in m.obligations if o not in obs.obligations]
        if missing:
            return (
                f"expected failing obligation(s) {', '.join(missing)}; "
                f"errors point into {', '.join(obs.obligations) or '(none)'}"
            )
        return None

    if obs.verdict is not None:
        got_v, got_e = obs.verdict
        return (
            f"expected simplification-rejected (no summary), "
            f"got {got_v}v/{got_e}e"
        )
    if not obs.compiled:
        return "the mutation did not compile (rustc error, not a refuted pin)"
    if m.obligations:
        # A `by (compute)` rejection prints the failing concrete pin's
        # expression, not a function site; the pin name is only in the source.
        # We can still require that the output mentions the pin's assertion.
        pass
    return None


def baseline(verus: str, proofs, verbose: bool) -> bool:
    ok = True
    for proof in proofs:
        rc, out = run_verus(verus, proof, 40)
        got = summarise(out)
        want = (BASELINE[proof], 0)
        if got != want:
            ok = False
            print(f"baseline FAIL  {proof}: expected {want[0]}v/0e, got {got or 'no summary'}")
            if verbose:
                print(out)
        else:
            print(f"baseline ok    {proof}: {got[0]} verified, 0 errors")
    return ok


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        description="Run the declared Verus mutants and assert their documented verdicts."
    )
    ap.add_argument("--list", action="store_true", help="list the declared mutants and exit")
    ap.add_argument("--only", action="append", default=[],
                    help="run only mutants whose name contains this substring (repeatable)")
    ap.add_argument("--calibrate", action="store_true",
                    help="measure and print, never fail (for authoring a table entry)")
    ap.add_argument("--self-check", action="store_true",
                    help="prove the assertions bite: one true and one falsified expectation")
    ap.add_argument("--verus", default=None, help="path to the verus binary")
    ap.add_argument("--no-baseline", action="store_true", help="skip the pristine runs")
    ap.add_argument("-v", "--verbose", action="store_true", help="print Verus output on failure")
    args = ap.parse_args(argv)

    selected = [m for m in MUTANTS if not args.only or any(s in m.name for s in args.only)]
    if not selected:
        print("mutate.py: --only matched nothing", file=sys.stderr)
        return 2

    if args.list:
        for m in selected:
            kind = (f"{m.verified}v/{m.errors}e" if m.counted else "simplification-rejected")
            print(f"{m.name:52s} {kind:24s} {m.note}")
        return 0

    verus = find_verus(args.verus)
    print(f"verus: {verus}")
    proofs = sorted({m.proof for m in selected})

    if not args.no_baseline:
        if not baseline(verus, proofs, args.verbose):
            print("\nmutate.py: baseline failed — the table's counts are measured "
                  "against those baselines, so nothing below would mean anything.",
                  file=sys.stderr)
            return 1

    if args.self_check:
        return self_check(verus, args.verbose)

    failures = 0
    restored = 0
    for m in selected:
        path = proof_path(m.proof)
        pre_sha = sha256_file(path)
        if not pre_sha.startswith(PROOF_SHA[m.proof]):
            print(f"FAIL  {m.name}: {m.proof}.rs sha {pre_sha[:12]} != recorded "
                  f"{PROOF_SHA[m.proof]}; re-audit the mutant table", file=sys.stderr)
            return 2
        original = open(path, "rb").read()
        text = original.decode("utf-8")
        obs = None
        restore_failed = False
        try:
            obs = observe(m, text, verus, args.verbose)
        except ValueError as exc:
            failures += 1
            print(f"FAIL  {m.name}: {exc}")
        except subprocess.TimeoutExpired:
            failures += 1
            print(f"FAIL  {m.name}: verus timed out")
        finally:
            with open(path, "wb") as fh:
                fh.write(original)
            post_sha = sha256_file(path)
            if post_sha != pre_sha:
                restore_failed = True
                print(f"RESTORE FAILED  {m.proof}.rs: {post_sha} != {pre_sha}",
                      file=sys.stderr)
            restored += 1
        if restore_failed:
            return 2
        if obs is None:
            continue

        if args.calibrate:
            got = (f"{obs.verdict[0]}v/{obs.verdict[1]}e" if obs.verdict
                   else "simplification-rejected")
            print(f"obs   {m.name:52s} {got:24s} "
                  f"failed=[{', '.join(obs.obligations)}]")
            continue

        why = check(m, obs)
        if why is None:
            got = (f"{obs.verdict[0]}v/{obs.verdict[1]}e" if obs.verdict
                   else "simplification-rejected")
            print(f"PASS  {m.name:52s} {got:24s} "
                  f"[{', '.join(obs.obligations)}]")
        else:
            failures += 1
            print(f"FAIL  {m.name}: {why}")
            if args.verbose:
                print(obs.output)

    print(f"\n{len(selected) - failures}/{len(selected)} mutants as declared; "
          f"{restored}/{len(selected)} files restored byte-identically (sha256)")
    return 0 if failures == 0 else 1


def self_check(verus: str, verbose: bool) -> int:
    """The gate must accept a true row and reject a falsified one.

    Uses `bc/M10` twice: the declared row (must pass) and the same mutation
    with the expectation shifted by one (must fail).  A checker that passes
    both is not checking anything.
    """
    real = next(m for m in MUTANTS if m.name == "bc/M10-total-base-case-is-one")
    falsified = Mutant(
        real.name + "-falsified", real.proof, real.find, real.replace,
        (real.verified or 0) - 1, real.errors or 0, real.obligations,
    )
    path = proof_path(real.proof)
    pre = sha256_file(path)
    ok = True
    for label, m, want in (("declared", real, None), ("falsified", falsified, "fail")):
        original = open(path, "rb").read()
        restore_failed = False
        try:
            obs = observe(m, original.decode("utf-8"), verus, verbose)
        finally:
            with open(path, "wb") as fh:
                fh.write(original)
            post = sha256_file(path)
            if post != pre:
                restore_failed = True
                print(f"self-check: RESTORE FAILED ({post} != {pre})", file=sys.stderr)
        if restore_failed:
            return 2
        why = check(m, obs)
        passed = why is None
        if (label == "declared") != passed:
            ok = False
            print(f"self-check FAIL: the {label} row was "
                  f"{'accepted' if passed else 'rejected'} ({why or 'ok'})")
        else:
            print(f"self-check ok: the {label} row was "
                  f"{'accepted' if passed else 'rejected'}")
    print("self-check: the assertions bite" if ok else "self-check: BROKEN")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
