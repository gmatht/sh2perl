//! Cluster G (part 1): the constant-pipeline fold — `false | true` is `true`.
//!
//! `native_constant_pipeline` lowers a pipeline whose every stage is a
//! word-free `true`/`false` exec to `status_exec(status of the LAST stage)`:
//!
//!     fn native_constant_pipeline(st: &mut IrStmt) -> bool
//!         func != "pipeline"                      -> refuse
//!         not `[IrExpr::Array(stages)]`           -> refuse
//!         stages.is_empty()                       -> refuse
//!         every stage: Arrow body of exactly one `exec`/`builtin` Call,
//!                      `true`/`false`, and NO words  (else refuse)
//!         *st = status_exec(last stage == "true")
//!
//! What this file proves, and why it is a refinement rather than a restatement:
//!
//!   The pass's side is `pass_decides`: a universal gate over the stages' SYNT AX
//!   (`stages.iter().all(...)` as the source writes it) plus a read of the LAST
//!   stage's NAME.  The semantic side is stated over the stages' BEHAVIOURS —
//!   the bytes each stage writes and the status it exits with.  The two are
//!   related only by `stage_spec` (what `true`/`false` do: they write nothing
//!   and exit 0/1) and by the index alignment of "the last stage".  A forgotten
//!   gate, a wrong index, or a stage whose behaviour is not its name's turns
//!   `fold_is_sound` red (see the measured mutants in the manifest).
//!
//! Residue, stated rather than claimed: the pipeline's reported status is
//! modelled as `pipeline_status(sts) = sts.last()` — bash's documented rule
//! ("the exit status of a pipeline is the exit status of its last command",
//! without `pipefail`) — rather than as an inductive execution fold.  An
//! earlier draft of this file carried the fold (`run_status`: consume each
//! stage's status in turn, report the last) and the induction
//! `fold == last_status`.  Verus's per-function fuel makes that induction
//! expensive to state at this rlimit, so it is not claimed here: the fold is
//! the reason `last` is the right rule, and `pipeline_status` is the rule.
//! What remains proved is the part a pass can get wrong — which stage the status
//! comes from, and whether that stage's status is what its name says.
//!
//! Three shapes are *conservatisms*, not divergences: bash's `true x` still
//! exits 0 and prints nothing, a stage body with two statements is still
//! `true`, and a `true` builtin reached through `redirect` is still `true`.
//! The fold keeps the shell-out for all three; the pins record that the model
//! agrees with bash and the pass refuses anyway.
//!
//! One shape IS a divergence from bash, and it is pinned (§5, d1): the fold
//! ignores `set -o pipefail`, under which `false | true` exits 1 but the fold
//! writes `status_exec(true)`.  The IR carries no pipefail state at this point,
//! so the fold has no gate for it: a source defect for a later stage, recorded
//! here rather than modelled away.  Three further guards: the stage's name is
//! taken to be the builtin (a script may define `true() { … }`, which the IR
//! does not distinguish), a stage's stdin is not modelled (both builtins ignore
//! it), and `shopt -s lastpipe` (same status, different environment) is out of
//! scope.
//!
//! Verus: 0.2026.10.18.7d82994, `--triggers-mode silent --rlimit 40`.

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// §1 The model: a stage as syntax.
// ---------------------------------------------------------------------------

/// A pipeline stage as the pass sees it.  `is_call` is "the body element is a
/// statement", `func_exec` is "that statement's Call has func exec/builtin",
/// `body_len` the Arrow body's length, `name` the command word and `words` the
/// remaining argument words.  Nothing about stdin/stdout appears: after the
/// gates, all that is left to decide is the status.
pub struct Stage {
    pub is_call: bool,
    pub func_exec: bool,
    pub body_len: int,
    pub name: Seq<u8>,
    pub words: Seq<Seq<u8>>,
}

pub open spec fn is_true(s: Stage) -> bool {
    s.name == seq![b't', b'r', b'u', b'e']
}

pub open spec fn is_false(s: Stage) -> bool {
    s.name == seq![b'f', b'a', b'l', b's', b'e']
}

/// A stage the fold accepts: one statement, an exec/builtin Call, `true` or
/// `false`, and no words.  This is `matches!`/`words.is_empty()` transcribed.
pub open spec fn stage_clean(s: Stage) -> bool {
    s.is_call && s.func_exec && s.body_len == 1 && s.words.len() == 0
        && (is_true(s) || is_false(s))
}

// ---------------------------------------------------------------------------
// §2 The pass's side: the universal gate, and the LAST stage's name.
// ---------------------------------------------------------------------------

/// `stages.iter().all(stage_clean)` as the source writes it — the universal
/// gate, not a per-prefix recursion, so nothing about it depends on fuel.
pub open spec fn every_stage_clean(stages: Seq<Stage>) -> bool {
    forall|i: int| 0 <= i < stages.len() ==> stage_clean(#[trigger] stages[i])
}

/// The status the fold writes: the LAST stage's `true`/`false`.
pub open spec fn pass_status(stages: Seq<Stage>) -> bool {
    is_true(stages.last())
}

/// The fold, transcribed: `Some(b)` exactly when it rewrites, and then the
/// status it writes is `b`; `None` when it refuses and keeps the shell-out.
pub open spec fn pass_decides(pipeline_shape: bool, args_ok: bool, stages: Seq<Stage>) -> Option<bool> {
    if !pipeline_shape || !args_ok || stages.len() == 0 || !every_stage_clean(stages) {
        None
    } else {
        Some(pass_status(stages))
    }
}

// ---------------------------------------------------------------------------
// §3 The semantic side: behaviours, and bash's pipeline rule.
// ---------------------------------------------------------------------------

/// What one stage does: `true` writes nothing and exits 0; `false` writes
/// nothing and exits 1; any other program is constrained by nothing (it may
/// print anything and exit with either status).
pub open spec fn stage_spec(s: Stage, out: Seq<u8>, status: bool) -> bool {
    (is_true(s) || is_false(s)) ==> (out.len() == 0 && status == is_true(s))
}

/// bash: "the exit status of a pipeline is the exit status of its last
/// command" (without `pipefail`).
pub open spec fn pipeline_status(sts: Seq<bool>) -> bool {
    sts.last()
}

/// bash: the pipeline's stdout is its last stage's stdout.
pub open spec fn pipeline_stdout(outs: Seq<Seq<u8>>) -> Seq<u8> {
    outs.last()
}

/// `set -o pipefail`: the pipeline's status is a failure if ANY stage failed,
/// so a failing stage anywhere decides it.  The fold does not model this (d1).
pub open spec fn pipefail_status(sts: Seq<bool>) -> bool {
    forall|i: int| 0 <= i < sts.len() ==> #[trigger] sts[i]
}

// ---------------------------------------------------------------------------
// §4 The laws.
// ---------------------------------------------------------------------------

/// **The status law.**  When the pass rewrites the pipeline, the status it
/// writes is the status bash reports.  The pass reads the last stage's NAME
/// (`pass_status`); bash reports the last stage's STATUS (`pipeline_status`).
/// They meet only because `stage_spec` says an accepted stage's status is its
/// name's — which is exactly what a wrong `stage_clean` would break.
proof fn fold_status_is_sound(
    pipeline_shape: bool,
    args_ok: bool,
    stages: Seq<Stage>,
    outs: Seq<Seq<u8>>,
    sts: Seq<bool>,
    b: bool,
)
    requires
        stages.len() >= 1,
        stages.len() == outs.len(),
        stages.len() == sts.len(),
        pass_decides(pipeline_shape, args_ok, stages) == Some(b),
        forall|i: int| 0 <= i < stages.len() ==> stage_spec(
            #[trigger] stages[i],
            #[trigger] outs[i],
            #[trigger] sts[i],
        ),
    ensures
        pipeline_status(sts) == b,
{
    let last = stages.len() - 1;
    assert(0 <= last < stages.len());
    // the pass's answer is the last stage's name status
    assert(b == is_true(stages.last()));
    // the last stage is accepted, so its behaviour is its name's
    assert(stage_clean(stages[last]));
    assert(is_true(stages[last]) || is_false(stages[last]));
    assert(stage_spec(stages[last], outs[last], sts[last]));
    assert(sts[last] == is_true(stages[last]));
    // and index `len-1` is the last element on both sides
    assert(stages[last] == stages.last());
    assert(sts[last] == sts.last());
    assert(pipeline_status(sts) == sts.last());
}

/// **The output law.**  Every accepted stage writes nothing, so the pipeline's
/// stdout (its last stage's) is empty: dropping the pipeline loses no output.
proof fn fold_output_is_empty(
    pipeline_shape: bool,
    args_ok: bool,
    stages: Seq<Stage>,
    outs: Seq<Seq<u8>>,
    sts: Seq<bool>,
)
    requires
        stages.len() >= 1,
        stages.len() == outs.len(),
        stages.len() == sts.len(),
        pass_decides(pipeline_shape, args_ok, stages).is_some(),
        forall|i: int| 0 <= i < stages.len() ==> stage_spec(
            #[trigger] stages[i],
            #[trigger] outs[i],
            #[trigger] sts[i],
        ),
    ensures
        pipeline_stdout(outs).len() == 0,
{
    let last = stages.len() - 1;
    assert(0 <= last < stages.len());
    assert(stage_clean(stages[last]));
    assert(is_true(stages[last]) || is_false(stages[last]));
    assert(stage_spec(stages[last], outs[last], sts[last]));
    assert(outs[last].len() == 0);
    assert(outs[last] == outs.last());
    assert(pipeline_stdout(outs) == outs.last());
}

/// **The central law, put together.**  For a pipeline the pass rewrites, the
/// status matches bash's and the output it drops is empty — for every gate the
/// fold applies and every stage behaviour `stage_spec` permits.
proof fn fold_is_sound(
    pipeline_shape: bool,
    args_ok: bool,
    stages: Seq<Stage>,
    outs: Seq<Seq<u8>>,
    sts: Seq<bool>,
    b: bool,
)
    requires
        stages.len() >= 1,
        stages.len() == outs.len(),
        stages.len() == sts.len(),
        pass_decides(pipeline_shape, args_ok, stages) == Some(b),
        forall|i: int| 0 <= i < stages.len() ==> stage_spec(
            #[trigger] stages[i],
            #[trigger] outs[i],
            #[trigger] sts[i],
        ),
    ensures
        pipeline_status(sts) == b,
        pipeline_stdout(outs).len() == 0,
{
    fold_status_is_sound(pipeline_shape, args_ok, stages, outs, sts, b);
    fold_output_is_empty(pipeline_shape, args_ok, stages, outs, sts);
}

/// A refused pipeline is *not* claimed equivalent: `None` carries no soundness
/// statement.  Stated so a reader cannot mistake the pin set for one.
proof fn refused_pipeline_claims_nothing(pipeline_shape: bool, args_ok: bool, stages: Seq<Stage>)
    requires
        !pass_decides(pipeline_shape, args_ok, stages).is_some(),
    ensures
        true,
{
}

/// A stage the gate rejects may print: `stage_spec` constrains nothing about an
/// opaque stage, so folding it would drop real output.
proof fn an_opaque_stage_can_print()
    ensures
        stage_spec(opaque_stage(), seq![b'h', b'i'], true),
        stage_spec(opaque_stage(), seq![b'h', b'i'], false),
{
}

// ---------------------------------------------------------------------------
// §5 Pins: the source's answers, each a proof fn the sync checker can find.
// ---------------------------------------------------------------------------

pub open spec fn t_stage() -> Stage {
    Stage {
        is_call: true,
        func_exec: true,
        body_len: 1,
        name: seq![b't', b'r', b'u', b'e'],
        words: Seq::<Seq<u8>>::empty(),
    }
}

pub open spec fn f_stage() -> Stage {
    Stage {
        is_call: true,
        func_exec: true,
        body_len: 1,
        name: seq![b'f', b'a', b'l', b's', b'e'],
        words: Seq::<Seq<u8>>::empty(),
    }
}

pub open spec fn opaque_stage() -> Stage {
    Stage {
        is_call: true,
        func_exec: true,
        body_len: 1,
        name: seq![b'd', b'a', b't', b'e'],
        words: Seq::<Seq<u8>>::empty(),
    }
}

/// `false | true` is `true`: the fold writes the last stage's status.
proof fn pin_false_then_true_is_true()
{
    let stages = seq![f_stage(), t_stage()];
    assert(pass_decides(true, true, stages) == Some(true));
    assert(pipeline_status(seq![false, true]) == true);
}

/// `true | false` is `false`.
proof fn pin_true_then_false_is_false()
{
    let stages = seq![t_stage(), f_stage()];
    assert(pass_decides(true, true, stages) == Some(false));
    assert(pipeline_status(seq![true, false]) == false);
}

/// Three stages: the last one decides, the first two do not.
proof fn pin_the_last_stage_decides_in_a_long_pipeline()
{
    let stages = seq![t_stage(), t_stage(), f_stage()];
    assert(pass_decides(true, true, stages) == Some(false));
    assert(pipeline_status(seq![true, true, false]) == false);
}

/// A non-`true`/`false` stage keeps the shell-out: it might print, and might
/// exit with either status.
proof fn pin_an_opaque_stage_keeps_the_shell_out()
{
    assert(!stage_clean(opaque_stage()));
    assert(!pass_decides(true, true, seq![t_stage(), opaque_stage()]).is_some());
    an_opaque_stage_can_print();
}

/// A worded builtin (`true x`) keeps the shell-out — although bash's `true`
/// ignores its arguments, so this is a conservatism, not a divergence.
proof fn pin_a_worded_builtin_keeps_the_shell_out()
{
    let wordy = Stage { words: seq![seq![b'x']], ..t_stage() };
    assert(!stage_clean(wordy));
    assert(!pass_decides(true, true, seq![wordy]).is_some());
    // bash's `true x` still exits 0 and prints nothing
    assert(stage_spec(t_stage(), Seq::<u8>::empty(), true));
}

/// An EMPTY pipeline keeps the shell-out (the fold would have no status to
/// read).
proof fn pin_an_empty_pipeline_keeps_the_shell_out()
{
    let none: Seq<Stage> = Seq::<Stage>::empty();
    assert(!pass_decides(true, true, none).is_some());
}

/// A stage whose body is not a single statement keeps the shell-out.
proof fn pin_a_multi_statement_stage_keeps_the_shell_out()
{
    let multi = Stage { body_len: 2, ..t_stage() };
    assert(!stage_clean(multi));
    assert(!pass_decides(true, true, seq![multi]).is_some());
}

/// A body element that is not an exec/builtin Call keeps the shell-out.
proof fn pin_a_non_call_stage_keeps_the_shell_out()
{
    let other = Stage { func_exec: false, ..t_stage() };
    assert(!stage_clean(other));
    assert(!pass_decides(true, true, seq![other]).is_some());
}

/// A non-pipeline expression, or a pipeline whose argument is not an array,
/// keeps the shell-out.
proof fn pin_a_non_pipeline_keeps_the_shell_out()
{
    assert(pass_decides(true, true, seq![t_stage()]) == Some(true));
    assert(!pass_decides(false, true, seq![t_stage()]).is_some());
    assert(!pass_decides(true, false, seq![t_stage()]).is_some());
}

/// **d1, a recorded divergence.**  Under `set -o pipefail`, `false | true`
/// exits 1, because the failing stage poisons the pipeline's status.  The fold
/// has no pipefail gate and writes `status_exec(true)`, i.e. 0.  Pinned as the
/// source's answer: the model keeps `pipefail_status` and the disagreement is a
/// fact, not a hypothesis.
proof fn pin_pipefail_disagrees()
{
    let sts = seq![false, true];
    assert(pipeline_status(sts) == true);
    assert(pipefail_status(sts) == false);
    assert(pipefail_status(sts) != pipeline_status(sts));
}

fn main() {}

} // verus!
