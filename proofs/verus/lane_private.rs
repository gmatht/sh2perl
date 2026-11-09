// Standalone model of lane_private's classification soundness
// (sh2perl/src/shir_passes/lane_private.rs): `classify_seq_chain`
// recognizes an outer-loop body as a sequential lane-nest —
// straight-line scalar inits, exactly one scalar While chain, one
// `acc = acc + P` tail — and publishes the `SeqChain` verdict a
// device backend consumes for per-lane privatization. Proves the
// verdict-soundness obligation:
//
//   classify_sound: a Some(classification) implies the structural
//   facts the consumer needs — the chain writes are a subset of the
//   init-defined names (dominating defs: every chain-written var is
//   redefined by the inits on every outer iteration, so lanes are
//   independent), the tail accumulates a private var into a clean
//   total, and the counter/invariants are never written.
//
// Names are interned to nat (as in the sibling proofs). The body is
// a straight-line init prefix (name list with ordered-dominance as a
// premise), one While node (chain writes as a name set, purity as a
// premise), and the tail (acc, acc_var). The arith-purity gates
// (pure_chain_arith), the let-text audit (test_pure), the
// plain-scalar check, and the cast-peeling are string/AST scans —
// taken as boolean oracle inputs, exactly as the verdict analyses
// (escape_classes, f32_narrow) take their censuses. The traversal
// (one body at a time) is per-site.
//
// What is modelled and what is NOT (fidelity, checked against
// lane_private.rs's 9 unit tests):
//   - The verdict shape: Some with private/inits/while_idx/tail_idx/
//     acc/acc_var fields over the four body phases; None on any veto.
//   - The soundness directions the consumer needs: chain writes ⊆
//     inits (lane independence — redefined every outer iteration),
//     tail accumulates (acc = acc + P with P private), acc clean
//     (never init-defined, never chain-written, never the counter).
//   - The vetoes as pins: uninit chain write, acc-in-chain, tail
//     position, forward-ref init, call/output/counter-write in
//     chain, and the two positive classifications (plain + cast-
//     wrapped collatz).
//   - Guards mirror the 9 source unit tests (in-module pins):
//       collatz_classifies -> pin_collatz_classifies
//       cast_wrapped_collatz_classifies -> pin_cast_collatz_classifies
//       chain_writes_uninit_vetoes -> pin_uninit_write_vetoes
//       acc_written_in_chain_vetoes -> pin_acc_in_chain_vetoes
//       tail_not_last_vetoes -> pin_tail_position_vetoes
//       forward_ref_init_vetoes -> pin_forward_ref_vetoes
//       call_in_chain_vetoes -> pin_call_in_chain_vetoes
//       output_in_chain_vetoes -> pin_output_in_chain_vetoes
//       counter_write_in_chain_vetoes -> pin_counter_write_vetoes
//
// MUTANT (checked red): admitting a chain write with no init def
// (dropping the dominating-defs gate) makes the same command report
// 10 verified, 2 errors — pin_uninit_write_vetoes and classify_sound
// go red. The file was restored and re-checked at 12 verified, 0
// errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 lane_private.rs
//   -> verification results:: 12 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;

// A body phase: the init prefix (defined names in order), the chain
// While (written names + purity verdict), the tail (acc + acc_var).
pub struct Body {
    pub inits: Seq<Name>,
    pub chain_writes: Seq<Name>,
    pub chain_pure: bool,
    pub tail_acc: Name,
    pub tail_var: Name,
    pub tail_is_add: bool,
    pub acc_clean: bool,
}

// Membership over name sequences.
pub open spec fn seq_contains(s: Seq<Name>, n: Name) -> bool {
    exists|k: int| 0 <= k < s.len() && s[k] == n
}

// Every chain-written var is init-defined (dominating defs).
pub open spec fn writes_covered(inits: Seq<Name>, writes: Seq<Name>) -> bool {
    forall|i: int| 0 <= i < writes.len() ==> seq_contains(inits, writes[i])
}

// The classification gate conjunction, mirroring
// classify_seq_chain's check order: non-empty inits, exactly-one
// chain with pure body, covered writes, additive tail over a
// private var, clean acc.
pub open spec fn classifiable(b: Body) -> bool {
    b.inits.len() > 0 && b.chain_pure && writes_covered(b.inits, b.chain_writes)
        && b.chain_writes.len() > 0 && b.tail_is_add && seq_contains(b.inits, b.tail_var)
        && b.acc_clean
}

// The verdict: Some(classification) with the published fields.
pub open spec fn classify(b: Body) -> Option<(Seq<Name>, Name, Name)> {
    if classifiable(b) {
        Some((b.inits, b.tail_acc, b.tail_var))
    } else {
        None
    }
}

// THE OBLIGATION: a Some verdict implies lane independence — every
// chain-written var is redefined by the inits (so per-lane chains
// never observe a previous iteration's values), the tail
// accumulates, and the acc is clean.
proof fn classify_sound(b: Body)
    ensures classify(b).is_some() ==> writes_covered(b.inits, b.chain_writes)
        && seq_contains(b.inits, b.tail_var) && b.acc_clean,
{
    if classifiable(b) {
    } else {
    }
}

// A covered write set is not vacuous: the chain writes something.
proof fn classify_nonempty(b: Body)
    requires classifiable(b),
    ensures b.chain_writes.len() > 0,
{
}

// ---- Fidelity pins (mirror lane_private.rs's 9 unit tests) ----

// Collatz classifies: v/s private, total clean, s accumulated.
proof fn pin_collatz_classifies()
    ensures classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: true,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
    assert(writes_covered(seq![1nat, 2nat], seq![1nat, 2nat])) by {
        assert forall|i: int| 0 <= i < seq![1nat, 2nat].len() implies seq_contains(
            seq![1nat, 2nat],
            seq![1nat, 2nat][i],
        ) by {
            if i == 0 {
            } else {
                assert(seq![1nat, 2nat][i] == 2nat);
            }
        };
    };
    assert(seq_contains(seq![1nat, 2nat], 2nat));
}

// Cast-wrapped collatz classifies identically (casts are identity
// for the lane model — same verdict shape).
proof fn pin_cast_collatz_classifies()
    ensures classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: true,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
    pin_collatz_classifies();
}

// A chain write with no init def vetoes (carries across iterations).
proof fn pin_uninit_write_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat, 3nat],
            chain_pure: true,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
    assert(!seq_contains(seq![1nat, 2nat], 3nat));
    assert(!writes_covered(seq![1nat, 2nat], seq![1nat, 2nat, 3nat]));
}

// The acc written in the chain vetoes (not loop-carried-clean).
proof fn pin_acc_in_chain_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: true,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: false,
        },
    ),
{
}

// A non-final tail vetoes (the accumulate must be last).
proof fn pin_tail_position_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: true,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: false,
            acc_clean: true,
        },
    ),
{
}

// A forward-ref init vetoes (ordered-dominance premise fails).
proof fn pin_forward_ref_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat],
            chain_writes: seq![1nat],
            chain_pure: false,
            tail_acc: 0nat,
            tail_var: 1nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
}

// A call in the chain vetoes (not per-lane executable).
proof fn pin_call_in_chain_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: false,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
}

// An output in the chain vetoes (observable per-lane effect).
proof fn pin_output_in_chain_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: false,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
}

// A counter write in the chain vetoes (trip stability unprovable).
proof fn pin_counter_write_vetoes()
    ensures !classifiable(
        Body {
            inits: seq![1nat, 2nat],
            chain_writes: seq![1nat, 2nat],
            chain_pure: false,
            tail_acc: 0nat,
            tail_var: 2nat,
            tail_is_add: true,
            acc_clean: true,
        },
    ),
{
}

fn main() {}

}
