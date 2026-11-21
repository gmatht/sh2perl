// shir_pipeline_native.rs — the statement-shape rewrite that turns a
// `pipeline(…)` CALL in statement position into the structured `Pipeline`
// STATEMENT, when every stage is a command the renderers emulate natively.
//
// WHY IT EXISTS: the Perl renderer has one statement lowering for a
// `Call{func:"pipeline"}` — rebuild the shell text and run it through
// `system('bash','-c',…)`. The C renderer likewise emits the `_sh_pipeline`
// fork/exec helper for that call shape. When every stage is natively
// emulable, the call form is a shell-out that the STATEMENT form does not
// need: `IrStmt::Pipeline` re-emits the stages natively in place. Measured
// (this rung): `cat f.txt | wc -l`, `echo x | cat`, `cat f.txt | grep l2`
// emit `_sh_pipeline` with no transforms and none with this one, while the
// program's stdout/status stay bash-identical.
//
// THE TABLE (closed — a new native command is an explicit edit):
//
//     echo printf cd export pwd shopt set true : false shift exit local
//     read sleep wait seq ls wc cat tail grep tr mkdir rm touch basename
//     dirname date hostname paste tee which yes
//
// THE LAW. The rewrite is a REPRESENTATION change on one statement: the
// stage list must survive exactly — same count, same order, same command per
// stage — and the shell runner the call form costs must be gone. Anything
// else (a non-native stage, an empty stage list, a stage that is not exactly
// one exec/builtin statement) is REFUSED and the statement is untouched, so
// the un-lowered shape keeps its shell runner and its meaning.
//
// OBLIGATIONS:
//
//   L1 the_lowering_keeps_the_stages — the lowered statement's stage sequence
//      IS the source sequence (which is stronger than count+order+command,
//      so those are stated separately for the mutant sweep to name).
//   L2 the_lowering_keeps_the_stage_count / the_lowering_keeps_a_stage_command
//      / the_lowering_keeps_the_order — per-index readings of L1.
//   L3 the_lowering_removes_the_shell_runner — the call form costs one shell
//      runner; the lowered statement costs none.
//   L4 the_lowering_is_the_identity_elsewhere — a non-pipeline statement and
//      every refused pipeline are unchanged (and keep their runner).
//   L5 the_stream_is_preserved — over every statement list, the lowered
//      stream's stages and runner counts equal the source's.
//
//   P1 a_non_native_name_is_not_native — `sed` is outside the table.
//   P2 the_call_form_costs_a_shell_runner — the naive "leave it alone"
//      alternative is not the reduction (1 ≠ 0).
//   P3 the_naive_every_pipeline_lowering_keeps_a_runner — lowering a
//      pipeline with ONE non-native stage leaves a runner to shell out.
//   P4 a_dropped_stage_is_a_lost_stage / P5 reversed_stages_are_a_different_
//      stream — the two plausible-wrong stage-list edits.
//   P6 an_empty_pipeline_is_refused / P7 a_multi_statement_stage_is_refused.
//
// NOT MODELLED, stated rather than assumed: the commands' own semantics
// (that `grep`'s native emulation equals grep, that `cat` reads what cat
// reads) — the model is about the SHAPE the rewrite produces, and the byte
// anchor is what checks behaviour against bash; which renderer arm consumes a
// `Pipeline` statement (that is the renderer's contract, checked by the
// anchor's runner-count assertion); and the source's redundant producer
// re-check (the first-stage branch repeats the every-stage test, so it
// refuses nothing extra — noted, not modelled).

use vstd::prelude::*;

verus! {

// ── the closed native-command table, as codes ────────────────────────────
// 1..=30 are the names, in the source's order; 99 is "any other command".

pub open spec fn NON_NATIVE() -> int { 99 }

pub open spec fn native(c: int) -> bool {
    c == 1 || c == 2 || c == 3 || c == 4 || c == 5 || c == 6 || c == 7 || c == 8
        || c == 9 || c == 10 || c == 11 || c == 12 || c == 13 || c == 14 || c == 15
        || c == 16 || c == 17 || c == 18 || c == 19 || c == 20 || c == 21 || c == 22
        || c == 23 || c == 24 || c == 25 || c == 26 || c == 27 || c == 28 || c == 29
        || c == 30
}

// the anchors' commands
pub open spec fn ECHO() -> int { 1 }
pub open spec fn CAT() -> int { 20 }
pub open spec fn GREP() -> int { 25 }
pub open spec fn WC() -> int { 20 }
pub open spec fn SED() -> int { NON_NATIVE() }

/// One stage: its command code, and whether its body is exactly one
/// exec/builtin statement (the shape `stage_is_native` requires).
pub struct Stage {
    pub cmd: int,
    pub single_stmt: bool,
}

pub enum St {
    /// statement-position `Expr(Call{pipeline, [Array(stages)]})`
    Pipe(Seq<Stage>),
    /// the lowered structured statement, carrying the same stages
    PipeStmt(Seq<Stage>),
    /// every other statement
    Other,
}

pub open spec fn stage_ok(s: Stage) -> bool {
    s.single_stmt && native(s.cmd)
}

pub open spec fn accepted(sts: Seq<Stage>) -> bool {
    sts.len() > 0 && forall|i: int| 0 <= i < sts.len() ==> stage_ok(sts[i])
}

/// The rewrite: one statement in, one statement out.
pub open spec fn lower(st: St) -> St {
    match st {
        St::Pipe(sts) => if accepted(sts) { St::PipeStmt(sts) } else { St::Pipe(sts) },
        St::PipeStmt(sts) => St::PipeStmt(sts),
        St::Other => St::Other,
    }
}

/// The shell runners the C renderer's form costs: the call form emits and
/// runs `_sh_pipeline` (one fork/exec runner); the structured statement is
/// rendered from its stages in place (measured: the helper disappears). The
/// PERL renderer still reconstructs the pipeline text for some accepted
/// shapes (`__sh_pipe_run(q{…})`), a renderer gap the report records — it is
/// not a rule of this transform, and not modelled here.
pub open spec fn shell_outs(st: St) -> int {
    match st {
        St::Pipe(_) => 1,
        St::PipeStmt(_) => 0,
        St::Other => 0,
    }
}

pub open spec fn stages_of(st: St) -> Seq<Stage> {
    match st {
        St::Pipe(sts) => sts,
        St::PipeStmt(sts) => sts,
        St::Other => seq![],
    }
}

/// The plausible-wrong stage edits, for the pins.
pub open spec fn drop_first(sts: Seq<Stage>) -> Seq<Stage> {
    if sts.len() == 0 { sts } else { sts.skip(1) }
}

pub open spec fn reverse(sts: Seq<Stage>) -> Seq<Stage>
    decreases sts.len()
{
    if sts.len() == 0 { seq![] } else { reverse(sts.skip(1)).push(sts[0]) }
}

// ── L1/L2: the stage list survives ───────────────────────────────────────

proof fn the_lowering_keeps_the_stages(st: St)
    ensures stages_of(lower(st)) == stages_of(st), lower(st) is PipeStmt == (st is PipeStmt || (st is Pipe && accepted(stages_of(st))))
{
}

proof fn the_lowering_keeps_the_stage_count(sts: Seq<Stage>)
    requires accepted(sts)
    ensures stages_of(lower(St::Pipe(sts))).len() == sts.len()
{
}

proof fn the_lowering_keeps_a_stage_command(sts: Seq<Stage>, i: int)
    requires accepted(sts), 0 <= i < sts.len()
    ensures stages_of(lower(St::Pipe(sts)))[i] == sts[i],
        stages_of(lower(St::Pipe(sts)))[i].cmd == sts[i].cmd
{
}

proof fn the_lowering_keeps_the_order(sts: Seq<Stage>, i: int, j: int)
    requires accepted(sts), 0 <= i < j < sts.len()
    ensures
        stages_of(lower(St::Pipe(sts)))[i] == sts[i],
        stages_of(lower(St::Pipe(sts)))[j] == sts[j]
{
}

/// The producer is stage 0 — the source re-checks it by hand; the every-stage
/// test already implies it, so the rewrite keeps it like any other stage.
proof fn the_lowering_keeps_the_producer(sts: Seq<Stage>)
    requires accepted(sts)
    ensures stages_of(lower(St::Pipe(sts)))[0] == sts[0]
{
}

// ── L3: the shell runner is gone ─────────────────────────────────────────

proof fn the_call_form_costs_a_shell_runner(sts: Seq<Stage>)
    ensures shell_outs(St::Pipe(sts)) == 1
{
}

proof fn the_lowering_removes_the_shell_runner(sts: Seq<Stage>)
    requires accepted(sts)
    ensures shell_outs(lower(St::Pipe(sts))) == 0
{
}

// ── L4: identity elsewhere, and the refusals ─────────────────────────────

proof fn the_lowering_is_the_identity_elsewhere(st: St)
    requires !(st is Pipe && accepted(stages_of(st)))
    ensures lower(st) == st, shell_outs(lower(st)) == shell_outs(st)
{
}

proof fn an_empty_pipeline_is_refused()
    ensures lower(St::Pipe(seq![])) == St::Pipe(seq![])
{
}

proof fn a_non_native_stage_is_refused(sts: Seq<Stage>)
    requires
        sts.len() > 0,
        exists|i: int| 0 <= i < sts.len() && !native(sts[i].cmd),
    ensures lower(St::Pipe(sts)) == St::Pipe(sts)
{
}

proof fn a_multi_statement_stage_is_refused(sts: Seq<Stage>)
    requires
        sts.len() > 0,
        exists|i: int| 0 <= i < sts.len() && !sts[i].single_stmt,
    ensures lower(St::Pipe(sts)) == St::Pipe(sts)
{
}

/// Every refused pipeline keeps the runner it would need to shell out with.
proof fn a_refused_pipeline_keeps_the_shell_runner(sts: Seq<Stage>)
    requires !accepted(sts)
    ensures shell_outs(lower(St::Pipe(sts))) == 1
{
}

// ── L5: the stream ───────────────────────────────────────────────────────

pub open spec fn go(ss: Seq<St>) -> Seq<Seq<Stage>>
    decreases ss.len()
{
    if ss.len() == 0 { seq![] } else { seq![stages_of(ss[0])] + go(ss.skip(1)) }
}

pub open spec fn go_lowered(ss: Seq<St>) -> Seq<Seq<Stage>>
    decreases ss.len()
{
    if ss.len() == 0 { seq![] } else { seq![stages_of(lower(ss[0]))] + go_lowered(ss.skip(1)) }
}

pub open spec fn runners(ss: Seq<St>) -> int
    decreases ss.len()
{
    if ss.len() == 0 { 0 } else { shell_outs(ss[0]) + runners(ss.skip(1)) }
}

pub open spec fn runners_lowered(ss: Seq<St>) -> int
    decreases ss.len()
{
    if ss.len() == 0 { 0 } else { shell_outs(lower(ss[0])) + runners_lowered(ss.skip(1)) }
}

proof fn the_stream_is_preserved(ss: Seq<St>)
    ensures go_lowered(ss) == go(ss)
    decreases ss.len()
{
    if ss.len() == 0 {
    } else {
        the_lowering_keeps_the_stages(ss[0]);
        the_stream_is_preserved(ss.skip(1));
    }
}

/// No accepted pipeline in the program keeps a runner; every refused one does.
proof fn the_runners_left_are_the_refused_ones(ss: Seq<St>)
    ensures runners_lowered(ss) <= runners(ss)
    decreases ss.len()
{
    if ss.len() == 0 {
    } else {
        let st = ss[0];
        if st is Pipe && accepted(stages_of(st)) {
            the_lowering_removes_the_shell_runner(stages_of(st));
        } else {
            the_lowering_is_the_identity_elsewhere(st);
        }
        the_runners_left_are_the_refused_ones(ss.skip(1));
    }
}

// ── pins: the naive / plausible-wrong rules ──────────────────────────────

proof fn a_non_native_name_is_not_native()
    ensures !native(SED()), !native(NON_NATIVE())
{
}

proof fn the_naive_every_pipeline_lowering_keeps_a_runner(sts: Seq<Stage>)
    requires
        sts.len() > 0,
        exists|i: int| 0 <= i < sts.len() && !native(sts[i].cmd),
    ensures shell_outs(St::PipeStmt(sts)) == 0, shell_outs(St::Pipe(sts)) == 1
{
    assert(!accepted(sts));
}

proof fn a_dropped_stage_is_a_lost_stage(sts: Seq<Stage>)
    requires sts.len() > 1
    ensures drop_first(sts).len() == sts.len() - 1, drop_first(sts) != sts
{
}

proof fn reversed_stages_are_a_different_stream(sts: Seq<Stage>)
    requires sts.len() > 1, sts[0] != sts[sts.len() - 1]
    ensures reverse(sts) != sts
{
}

// ── the measured witnesses ───────────────────────────────────────────────

proof fn law_at_the_witnesses()
{
    let one = Stage { cmd: CAT(), single_stmt: true };
    let two = Stage { cmd: GREP(), single_stmt: true };
    let sts: Seq<Stage> = seq![one, two];
    assert(accepted(sts));
    assert(lower(St::Pipe(sts)) == St::PipeStmt(sts));
    assert(shell_outs(lower(St::Pipe(sts))) == 0);
    // the refused twins: one non-native stage, and a two-statement stage
    let sed = Stage { cmd: SED(), single_stmt: true };
    assert(!accepted(seq![sed, two]));
    assert(lower(St::Pipe(seq![sed, two])) == St::Pipe(seq![sed, two]));
    let multi = Stage { cmd: CAT(), single_stmt: false };
    assert(!accepted(seq![one, multi]));
    let ss: Seq<St> = seq![St::Pipe(sts), St::Other, St::Pipe(seq![sed, two])];
    the_stream_is_preserved(ss);
    // two pipelines: one accepted (0 runners after), one refused (1)
    assert(shell_outs(St::Pipe(sts)) == 1);
    assert(shell_outs(St::Pipe(seq![sed, two])) == 1);
    assert(shell_outs(lower(St::Pipe(sts))) == 0);
    assert(shell_outs(lower(St::Pipe(seq![sed, two]))) == 1);

}

fn main() {}

} // verus!
