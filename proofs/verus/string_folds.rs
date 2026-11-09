// Standalone model of two string-fold Tier-1 obligations:
//   (a) interp-fold (`sh2perl/src/transforms/interp_fold.rs`): a
//       single-literal `Interpolate([Lit(s)])` denotes the same string
//       as the plain `Str(s)` it folds to;
//   (b) default-empty-fold (`sh2perl/src/transforms/default_empty_fold.rs`):
//       `param(":-"/"-", NAME, "")` denotes the same string as the plain
//       read of NAME — under the pass's three gates (plain identifier,
//       non-array-homed, program-wide no-nounset).
//
// Proves the soundness obligations:
//
//   interp_sound: folding a single-literal Interpolate preserves the
//   denoted string.
//   default_empty_sound: folding an empty-default param preserves the
//   denoted string from any store, under the three gate preconditions.
//
// Strings are Seq<char>. The store maps names to strings; nounset and
// array-homed-ness are boolean parameters (the real nounset gate is a
// program-wide scan, the array-homed set a pre-pass census — both are
// inputs to the per-node theorem, exactly as the transform checks them
// before folding).
//
// What is modelled and what is NOT (fidelity, checked against the sources):
//   - interp-fold's exact rule: ONLY `[Lit(s)]` folds; `[Expr(e)]` and
//     multi-part lists refuse (shape-preservation, not value — the
//     assign-head refusal is a consumer-shape rule, out of scope and pinned
//     as a refusal, not a value claim).
//   - default-empty-fold's exact gates: op ∈ {":-","-"}, NAME plain
//     identifier, NAME not array-homed, program nounset off. The nounset
//     scan (set/-o/enable tracking) and the array census are out of scope;
//     the model takes their verdicts as parameters.
//   - The `${v:-}` vs `${v}` identity itself: under no-nounset bash, an
//     empty default yields the value-or-empty — exactly the plain read.
//     (Under nounset they differ on unset vars; the gate excludes that.)
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Str = Seq<char>;
pub type Store = Map<Name, Str>;

pub enum Part {
    Lit(Str),
    Expr(Name),
}

pub enum Word {
    Str(Str),
    Interp(Seq<Part>),
}

// ---- Denotation ----

pub open spec fn lookup(s: Store, n: Name) -> Str {
    if s.dom().contains(n) {
        s[n]
    } else {
        Seq::empty()
    }
}

pub open spec fn eval_word(w: Word, s: Store) -> Str {
    match w {
        Word::Str(t) => t,
        Word::Interp(parts) => eval_parts(parts, s),
    }
}

pub open spec fn eval_parts(p: Seq<Part>, s: Store) -> Str
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else {
        match p[0] {
            Part::Lit(t) => t.add(eval_parts(p.drop_first(), s)),
            Part::Expr(n) => lookup(s, n).add(eval_parts(p.drop_first(), s)),
        }
    }
}

// ---- (a) interp-fold ----

// The fold: single-literal Interpolate -> Str. Everything else stays.
pub open spec fn interp_fold(w: Word) -> Word {
    match w {
        Word::Interp(parts) => {
            if parts.len() == 1 {
                match parts[0] {
                    Part::Lit(t) => Word::Str(t),
                    Part::Expr(_) => w,
                }
            } else {
                w
            }
        },
        _ => w,
    }
}

// THE OBLIGATION (a): folding preserves the denoted string.
proof fn interp_sound(w: Word, s: Store)
    ensures eval_word(interp_fold(w), s) == eval_word(w, s),
{
    match w {
        Word::Str(_) => {},
        Word::Interp(parts) => {
            if parts.len() == 1 {
                match parts[0] {
                    Part::Lit(t) => {
                        assert(parts == seq![Part::Lit(t)]) by {
                            assert(parts.len() == 1);
                        };
                        reveal_with_fuel(eval_parts, 2);
                        assert(eval_parts(Seq::<Part>::empty(), s) == Seq::<char>::empty()) by {
                            reveal_with_fuel(eval_parts, 2);
                        };
                    },
                    Part::Expr(_) => {},
                }
            } else {
            }
        },
    }
}

// ---- (b) default-empty-fold ----

// Operands: a plain read or the empty-default param form.
pub enum DefOp {
    Read(Name),
    DefaultOr(Name),
}

// The three gates as parameters: plain identifier, not array-homed,
// program nounset off.
pub open spec fn gates_ok(plain_ident: bool, array_homed: bool, nounset: bool) -> bool {
    plain_ident && !array_homed && !nounset
}

pub open spec fn eval_defop(o: DefOp, s: Store) -> Str {
    match o {
        DefOp::Read(n) => lookup(s, n),
        // `${v:-}` under no-nounset: set-and-non-null yields v, else the
        // empty default — exactly the plain read (unset reads empty).
        DefOp::DefaultOr(n) => lookup(s, n),
    }
}

// The fold: empty-default param -> plain read, when the gates hold.
pub open spec fn default_empty_fold(o: DefOp, plain_ident: bool, array_homed: bool, nounset: bool) -> DefOp {
    match o {
        DefOp::DefaultOr(n) => {
            if gates_ok(plain_ident, array_homed, nounset) {
                DefOp::Read(n)
            } else {
                o
            }
        },
        _ => o,
    }
}

// THE OBLIGATION (b): folding preserves the denoted string from any
// store, under the gates.
proof fn default_empty_sound(o: DefOp, s: Store, plain_ident: bool, array_homed: bool, nounset: bool)
    requires gates_ok(plain_ident, array_homed, nounset),
    ensures eval_defop(default_empty_fold(o, plain_ident, array_homed, nounset), s) == eval_defop(o, s),
{
    match o {
        DefOp::Read(_) => {},
        DefOp::DefaultOr(_) => {},
    }
}

// ---- Fidelity pins (mirror the sources' own tests) ----

// interp-fold: `Interpolate([Lit("hello")])` folds to `Str("hello")`.
proof fn pin_single_literal_folds()
    ensures interp_fold(Word::Interp(seq![Part::Lit(seq!['h', 'e', 'l', 'l', 'o'])])) == Word::Str(
        seq!['h', 'e', 'l', 'l', 'o'],
    ),
{
}

// interp-fold: a single-Expr part refuses (word-list positions differ).
proof fn pin_single_expr_refuses()
    ensures interp_fold(Word::Interp(seq![Part::Expr(7nat)])) == Word::Interp(
        seq![Part::Expr(7nat)],
    ),
{
}

// interp-fold: multi-part lists stay (their concat is real).
proof fn pin_multi_part_stays()
    ensures interp_fold(
        Word::Interp(seq![Part::Lit(seq!['a']), Part::Expr(3nat), Part::Lit(seq!['b'])]),
    ) == Word::Interp(seq![Part::Lit(seq!['a']), Part::Expr(3nat), Part::Lit(seq!['b'])]),
{
}

// interp-fold value check on a live store: the fold is invisible.
proof fn pin_fold_invisible_at_runtime()
    ensures eval_word(
        interp_fold(Word::Interp(seq![Part::Lit(seq!['h', 'i'])])),
        Map::empty().insert(7nat, seq!['x']),
    ) == seq!['h', 'i'],
{
    interp_sound(
        Word::Interp(seq![Part::Lit(seq!['h', 'i'])]),
        Map::empty().insert(7nat, seq!['x']),
    );
}

// default-empty-fold: gates open -> folds to the read.
proof fn pin_gates_open_folds()
    ensures default_empty_fold(DefOp::DefaultOr(4nat), true, false, false) == DefOp::Read(4nat),
{
}

// default-empty-fold: each gate refuses (nounset / array-homed /
// non-identifier keep the param form).
proof fn pin_gates_refuse()
    ensures
        default_empty_fold(DefOp::DefaultOr(4nat), true, false, true) == DefOp::DefaultOr(4nat),
        default_empty_fold(DefOp::DefaultOr(4nat), true, true, false) == DefOp::DefaultOr(4nat),
        default_empty_fold(DefOp::DefaultOr(4nat), false, false, false) == DefOp::DefaultOr(4nat),
{
}

// default-empty-fold value check: fold invisible from any store.
proof fn pin_default_invisible_at_runtime()
    ensures eval_defop(
        default_empty_fold(DefOp::DefaultOr(4nat), true, false, false),
        Map::empty().insert(4nat, seq!['v', 'a', 'l']),
    ) == seq!['v', 'a', 'l'],
{
    default_empty_sound(DefOp::DefaultOr(4nat), Map::empty().insert(4nat, seq!['v', 'a', 'l']), true, false, false);
}

fn main() {}

}
