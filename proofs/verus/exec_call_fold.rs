// Standalone model of exec-call-fold
// (sh2perl/src/transforms/exec_call_fold.rs): the payload-free `IrStmt::Exec`
// inside a command carrier (arrow body / pipeline stage) becomes the
// canonical `Expr(Call{"exec", [cmd, Array(argv)]})` command spelling.
// Proves the information-preservation obligation:
//
//   fold_preserves_payload: folding never changes the command payload —
//   the (cmd, argv) pair both carriers hold is identical before and after.
//
// Both carriers denote the same command (module doc "invariant": "the two
// are the same command in two carriers, so the rewrite is information
// preserving"). The traversal (`walk` with its carrier flag — Arrow sets
// it, Lambda/Function reset it, Pipeline stages set it) is out of scope:
// per-site theorem over a carrier boolean parameter (array_len_fold
// precedent). The `is_ps_tmp_cleanup` oracle and the `shir`-target gate
// in `transform_applies` are pipeline placement, not value claims
// (ternary_desugar precedent: the estree-path exemption is placement).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - the exact gate conjunction, mirroring `fold_here`'s check order:
//     carrier position, Exec carrier, non-cleanup, capture None, empty
//     redirects, empty env. The rebuilt shape (`Call{"exec"}` over
//     [cmd, Array(argv)]) carries the SAME payload.
//   - the three refusals as pins (statement position, capture payload,
//     ps-tmp cleanup) mirroring the 4 source unit tests.
//   - idempotence: the folded call form is not an Exec, so a second
//     pass is the identity (the source's own idempotence assertion).
//
// MUTANT (checked red): treating the ps-tmp cleanup as foldable
// (dropping the cleanup refusal) makes the same command report 7
// verified, 1 error — pin_ps_tmp_cleanup_kept goes red. The file was
// restored and re-checked at 8 verified, 0 errors. Note the value
// theorem still holds without the gate (both sides carry the same
// payload) — the pin, not the soundness lemma, is load-bearing here,
// exactly the slice_identity/array_len_fold offset-guard shape: the
// gates preserve recognisability, not value.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 exec_call_fold.rs
//   -> verification results:: 8 verified, 0 errors
use vstd::prelude::*;

verus! {

// The command payload both carriers hold: command word + argv list.
pub type Cmd = Seq<char>;
pub type Argv = Seq<Seq<char>>;

pub open spec fn payload_cmd(cmd: Cmd, _argv: Argv) -> Cmd {
    cmd
}

// Statement shapes the model distinguishes: the Exec carrier (with its
// refusal flags as data), the folded call spelling, and everything else.
pub enum Stmt {
    Exec {
        cmd: Cmd,
        argv: Argv,
        has_capture: bool,
        has_redirects: bool,
        has_env: bool,
        ps_cleanup: bool,
    },
    Call { cmd: Cmd, argv: Argv },
    Other,
}

// The payload a statement carries: identical for both command spellings.
pub open spec fn payload(s: Stmt) -> (Cmd, Argv) {
    match s {
        Stmt::Exec { cmd, argv, .. } => (cmd, argv),
        Stmt::Call { cmd, argv } => (cmd, argv),
        Stmt::Other => (Seq::empty(), Seq::empty()),
    }
}

// The gate conjunction, mirroring `fold_here`'s check order: carrier
// position, Exec carrier (given), non-cleanup, no capture channel, no
// redirect set, no command-scoped env.
pub open spec fn foldable(s: Stmt, carrier: bool) -> bool {
    match s {
        Stmt::Exec { has_capture, has_redirects, has_env, ps_cleanup, .. } => {
            carrier && !ps_cleanup && !has_capture && !has_redirects && !has_env
        },
        Stmt::Call { .. } => false,
        Stmt::Other => false,
    }
}

// The fold: gate-satisfying Exec -> the call spelling with the SAME
// payload; everything else stays.
pub open spec fn fold(s: Stmt, carrier: bool) -> Stmt {
    match s {
        Stmt::Exec { cmd, argv, .. } => {
            if foldable(s, carrier) {
                Stmt::Call { cmd, argv }
            } else {
                s
            }
        },
        _ => s,
    }
}

// THE OBLIGATION: folding never changes the command payload.
proof fn fold_preserves_payload(s: Stmt, carrier: bool)
    ensures payload(fold(s, carrier)) == payload(s),
{
    match s {
        Stmt::Exec { cmd, argv, .. } => {
            if foldable(s, carrier) {
                assert(fold(s, carrier) == Stmt::Call { cmd, argv });
            } else {
                assert(fold(s, carrier) == s);
            }
        },
        Stmt::Call { .. } => {},
        Stmt::Other => {},
    }
}

// The folded form is stable: a second pass changes nothing (the call
// spelling is not an Exec, so the gate never re-fires).
proof fn fold_idempotent(s: Stmt, carrier: bool)
    ensures fold(fold(s, carrier), carrier) == fold(s, carrier),
{
    match s {
        Stmt::Exec { cmd, argv, .. } => {
            if foldable(s, carrier) {
                assert(fold(s, carrier) == Stmt::Call { cmd, argv });
            } else {
                assert(fold(s, carrier) == s);
            }
        },
        Stmt::Call { .. } => {},
        Stmt::Other => {},
    }
}

// ---- Fidelity pins (mirror the source's own tests) ----

// Arrow-body payload-free Exec folds to the call form.
proof fn pin_arrow_body_folds()
    ensures fold(
        Stmt::Exec {
            cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
            argv: Seq::empty(),
            has_capture: false,
            has_redirects: false,
            has_env: false,
            ps_cleanup: false,
        },
        true,
    ) == (Stmt::Call { cmd: seq!['m', 'k', 't', 'e', 'm', 'p'], argv: Seq::empty() }),
{
}

// Statement-position Exec is left alone (the carriers the renderers'
// own Exec arms key on).
proof fn pin_statement_position_kept()
    ensures fold(
        Stmt::Exec {
            cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
            argv: Seq::empty(),
            has_capture: false,
            has_redirects: false,
            has_env: false,
            ps_cleanup: false,
        },
        false,
    ) == (Stmt::Exec {
        cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
        argv: Seq::empty(),
        has_capture: false,
        has_redirects: false,
        has_env: false,
        ps_cleanup: false,
    }),
{
}

// A capture channel keeps its carrier (the call form has nowhere to
// put it).
proof fn pin_capture_kept()
    ensures fold(
        Stmt::Exec {
            cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
            argv: Seq::empty(),
            has_capture: true,
            has_redirects: false,
            has_env: false,
            ps_cleanup: false,
        },
        true,
    ) == (Stmt::Exec {
        cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
        argv: Seq::empty(),
        has_capture: true,
        has_redirects: false,
        has_env: false,
        ps_cleanup: false,
    }),
{
}

// The synthetic process-substitution cleanup keeps its carrier
// (recognisable only by this carrier).
proof fn pin_ps_tmp_cleanup_kept()
    ensures fold(
        Stmt::Exec {
            cmd: seq!['r', 'm'],
            argv: Seq::empty(),
            has_capture: false,
            has_redirects: false,
            has_env: false,
            ps_cleanup: true,
        },
        true,
    ) == (Stmt::Exec {
        cmd: seq!['r', 'm'],
        argv: Seq::empty(),
        has_capture: false,
        has_redirects: false,
        has_env: false,
        ps_cleanup: true,
    }),
{
}

// Payload check on live values: the fold is invisible to the payload.
proof fn pin_fold_invisible_at_runtime()
    ensures payload(
        fold(
            Stmt::Exec {
                cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
                argv: Seq::empty(),
                has_capture: false,
                has_redirects: false,
                has_env: false,
                ps_cleanup: false,
            },
            true,
        ),
    ) == (seq!['m', 'k', 't', 'e', 'm', 'p'], Seq::<Seq<char>>::empty()),
{
    fold_preserves_payload(
        Stmt::Exec {
            cmd: seq!['m', 'k', 't', 'e', 'm', 'p'],
            argv: Seq::empty(),
            has_capture: false,
            has_redirects: false,
            has_env: false,
            ps_cleanup: false,
        },
        true,
    );
}

fn main() {}

}
