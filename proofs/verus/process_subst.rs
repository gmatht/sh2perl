// Conditional model of `process-subst`
// (`sh2perl/src/transforms/process_subst.rs`), the fd-carrying shapes.
//
// NOT a Tier-1 artifact: it mentions the denotation of a STATEMENT (`prun`),
// so it is the conditional "middle rung".
//
// ## The obligation
//
// bash's `cmd <(P)` passes the producer's stdout to `cmd` through a
// `/dev/fd/N` path argument. A POSIX target cannot express that, so the pass
// materializes it:
//
//   tmp=$(mktemp); { P; } > tmp; cmd … tmp …; rm -f tmp
//
// The routing is sound iff the consumer sees exactly the producer's output
// and the temp file is gone afterwards -- `materialization_preserves_output`
// and `materialization_leaves_no_residue` below.
//
// ## Environment model
//
// A machine with a file store (path -> bytes) and an output stream. `Fill` is
// the pass's `mktemp` + producer-write pair (the path is fresh by
// construction: the pass's `__ps_tmp{n}` counter), `Read` is the consumer
// reading the materialized path, `Rm` is the cleanup exec. Only the consumer's
// INPUT bytes and residue are observed -- what matters for `<(...)`.
//
// ## What is proved
//
//   - `materialization_preserves_output`: the consumer's input is exactly the
//     producer's output, byte for byte, for any prior store and any other
//     temp files present.
//   - `materialization_leaves_no_residue`: the temp path is unlinked.
//   - `materialization_preserves_other_temp`: other paths are untouched.
//   - `buggy_pair_inversion_is_empty`: the negative control. The pass's tests
//     record that reusing the CONSUMER's redirect pair for the producer made
//     the producer a READ of the temp file -- nothing wrote it, so the
//     consumer read an empty file. The model proves that shape yields exactly
//     the empty input, so the fd/mode inversion the tests pin is load-bearing.
//
// ## What is NOT proved
//
//   - ordering between producer and consumer *in real time*: bash starts the
//     producer before the consumer runs, and a fully-read temp file models
//     that. A producer that blocks forever (`<(yes)`) is not modelled.
//   - `process-out` (`>(...)`): the consumer writes and the producer reads
//     afterwards; the same three lemmas invert, and the ordering caveat (the
//     producer-read runs after the exec, so it can miss buffered output) is
//     left as an accepted limitation, not a theorem.
//   - exit status: the trailing `rm` clobbers `$?`; the pass documents that
//     as an accepted limitation, so this model observes input bytes and
//     residue only.
//   - path/word quoting, `mapfile`'s stdin form (modelled as the same read),
//     and the parse-back of the producer's shell text (`parse_producer`'s
//     Debug-leak refusal) -- traversal/refusal structure, not semantics.
use vstd::prelude::*;

verus! {

pub enum P {
    /// `tmp=$(mktemp)` + `{ producer; } > tmp` -- the producer's output lands
    /// in a fresh temp path.
    Fill(int, Seq<int>),
    /// the consumer takes the temp path (arg position, or a `r` redirect fd).
    Read(int),
    /// `rm -f tmp`.
    Rm(int),
    Seq(Box<P>, Box<P>),
}

pub struct F {
    pub store: Map<int, Seq<int>>,
    /// bytes the consumer received
    pub out: Seq<int>,
}

pub open spec fn content(f: F, p: int) -> Seq<int> {
    if f.store.dom().contains(p) {
        f.store[p]
    } else {
        Seq::empty()
    }
}

pub open spec fn prun(s: P, f: F) -> F
    decreases s,
{
    match s {
        P::Fill(p, v) => F { store: f.store.insert(p, v), out: f.out },
        P::Read(p) => F { store: f.store, out: f.out + content(f, p) },
        P::Rm(p) => F { store: f.store.remove(p), out: f.out },
        P::Seq(a, b) => prun(*b, prun(*a, f)),
    }
}

/// What the pass emits for one `process-in` on an exec consumer.
pub open spec fn lowering(p: int, v: Seq<int>) -> P {
    P::Seq(
        Box::new(P::Fill(p, v)),
        Box::new(P::Seq(Box::new(P::Read(p)), Box::new(P::Rm(p)))),
    )
}

/// The shape the pass's tests reject: the producer never writes (its redirect
/// pair was taken from the consumer, so it reads instead of writing).
pub open spec fn inverted(p: int) -> P {
    P::Seq(Box::new(P::Read(p)), Box::new(P::Rm(p)))
}

proof fn materialization_preserves_output(p: int, v: Seq<int>, f: F)
    requires
        !f.store.dom().contains(p),
    ensures
        prun(lowering(p, v), f).out == f.out + v,
{
    let f1 = F { store: f.store.insert(p, v), out: f.out };
    let f2 = F { store: f1.store, out: f1.out + content(f1, p) };
    let f3 = F { store: f2.store.remove(p), out: f2.out };
    assert(f.store.insert(p, v).dom().contains(p));
    assert(content(f1, p) == v);
    assert(prun(lowering(p, v), f) == f3) by {
        reveal(lowering);
        reveal_with_fuel(prun, 6);
    }
    assert(f3.out == f.out + v);
}

proof fn materialization_leaves_no_residue(p: int, v: Seq<int>, f: F)
    ensures
        !prun(lowering(p, v), f).store.dom().contains(p),
{
    let f1 = F { store: f.store.insert(p, v), out: f.out };
    let f2 = F { store: f1.store, out: f1.out + content(f1, p) };
    let f3 = F { store: f2.store.remove(p), out: f2.out };
    assert(prun(lowering(p, v), f) == f3) by {
        reveal(lowering);
        reveal_with_fuel(prun, 6);
    }
    assert(!f.store.insert(p, v).remove(p).dom().contains(p));
}

proof fn materialization_preserves_other_temp(p: int, q: int, v: Seq<int>, f: F)
    requires
        q != p,
    ensures
        content(prun(lowering(p, v), f), q) == content(f, q),
{
    let f1 = F { store: f.store.insert(p, v), out: f.out };
    let f2 = F { store: f1.store, out: f1.out + content(f1, p) };
    let f3 = F { store: f2.store.remove(p), out: f2.out };
    assert(f.store.insert(p, v).remove(p).dom().contains(q)
        == f.store.insert(p, v).dom().contains(q));
    if f.store.dom().contains(q) {
        assert(!f.store.insert(p, v).dom().contains(q) == false);
        assert(f.store.insert(p, v).dom().contains(q));
        assert(f.store.insert(p, v)[q] == f.store[q]);
        assert(f.store.insert(p, v).remove(p).dom().contains(q));
        assert(f.store.insert(p, v).remove(p)[q] == f.store[q]);
        assert(content(f1, p) == v);
        assert(content(f3, q) == f.store[q]);
    } else {
        assert(content(f, q) == Seq::empty());
        assert(!f.store.insert(p, v).dom().contains(q));
        assert(content(f3, q) == Seq::empty());
    }
    assert(content(prun(lowering(p, v), f), q) == content(f, q)) by {
        reveal(lowering);
        reveal_with_fuel(prun, 6);
        reveal(content);
    }
}

/// NEGATIVE CONTROL: without the producer write the consumer's input is
/// empty, so the fd/mode inversion is not a cosmetic choice.
proof fn buggy_pair_inversion_is_empty(p: int, f: F)
    requires
        !f.store.dom().contains(p),
    ensures
        prun(inverted(p), f).out == f.out,
{
    assert(content(f, p) == Seq::empty());
    assert(prun(inverted(p), f).out == f.out + Seq::empty()) by {
        reveal(inverted);
        reveal_with_fuel(prun, 4);
    }
    assert(f.out + Seq::empty() == f.out);
}

fn main() {}

} // verus!
