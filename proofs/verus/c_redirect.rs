// Standalone model of the C backend's REDIRECT EMISSION — `sh_redirect_text`
// (sh2perl/src/c_backend.rs:22655) and the `_sh_word` quoter it drives
// (helper emitted at c_backend.rs:12122), read against bash's redirection
// semantics. The mode table it transcribes is shir.rs:19459-19483; the fd
// defaults come from there (`heredoc`/`herestring`/`process-in` → 0,
// `w`/`wc`/`a`/`process-out` → 1, `r`/`r+` → 0), and ir.rs:567-570 pins the
// mode strings ("r" | "w" | "a" | "r+" | "wc" | "heredoc" | "herestring" |
// "unsupported"; `heredoc-tabs`/`process-in`/`process-out` are added by
// shir.rs:19469-19481).
//
// THE DENOTATIONAL EXTENSION (this file): a redirect list denotes a
// TRANSFORMATION ON BASH'S fd TABLE — `Tab = Map<int, Spec>`, applied in
// SOURCE ORDER by bash (a redirection is applied where it stands, and the
// LAST writer of an fd wins). On top of that denotation sits the one
// structural move the renderer makes:
//
//   emit_order(rds) = non_heres(rds) ++ heres(rds)
//
// heredoc/heredoc-tabs/herestring fragments are emitted LAST — forced:
// their text ends in a delimiter line that must stand alone, so any
// redirect word after it would be swallowed as body text
// (`heredoc_last_forced` below; the source comment at c_backend.rs:22697
// states the same reason). The reorder therefore CHANGES the fd-table
// semantics whenever a non-heredoc redirect touches fd 0 — the exact
// condition this model isolates and proves.
//
// Obligations (each maps to a named proof or a measured probe):
//   quote_roundtrip        `_sh_word`'s escaper is lossless: the value the
//                          child bash word-parses equals the bytes handed
//                          in (`run` is a 3-state lexer over one char per
//                          step; the `'`→`'"'"'` fragment is the only
//                          non-identity case and returns to state Sq).
//   m1_esc_dropped         mutant: emitting the raw bytes between quotes
//                          loses everything after an embedded `'`.
//   digit_roundtrip / fd_parse
//                          the prefix text is exactly the fd: `""` ⟺ fd 1
//                          (bash's default for the `>` family), one digit
//                          otherwise — under fd_wf (0..9, and fd 1 only
//                          where the operator's default IS 1).
//   raw_arm_no_quotes      the dup/close arm (`2>&1`, `2>&-`) emits NO
//                          quote characters; m2 shows what quoting it
//                          would produce (`>&'1'` names a file).
//   heredoc_last_forced    appending a non-heredoc after a heredoc frag
//                          moves the terminator: `ends_with(a+b, d)
//                          == ends_with(b, d)`. m3 computes the concrete
//                          `cat <<EOF` + `0< 'in.txt'` pair.
//   commute_here           a heredoc (writes fd 0 with a constant, reads
//                          nothing) commutes with a redirect that does
//                          NOT touch fd 0 — the swap the reorder lives on.
//   fold_commute / fold_concat / reorder_safe
//                          THEOREM: if `safe(rds)` — no non-heredoc that
//                          touches fd 0 occurs at or after the FIRST
//                          heredoc — then `emit_order` preserves the fd
//                          table for EVERY starting table.
//   n1_fd0_conflict        the condition is NECESSARY, not decorative:
//                          `cat <<EOF < in.txt` is unsafe, and at fd 0 the
//                          two orders disagree for every starting table.
//                          MEASURED (2026-11-01, /tmp/redprobe):
//                            bash src1.sh  → FROM_FILE   (source order:
//                                             `<<EOF` applied, then `< in.txt`
//                                             overrides fd 0)
//                            ./src1.bin    → FROM_HEREDOC (emitted order:
//                                             `0< 'in.txt'` first, heredoc last)
//                          src2 (`cat < in.txt <<EOF`) is SAFE — the fd-0
//                          non-heredoc precedes every heredoc, so the
//                          partition does not invert the pair; both
//                          produce FROM_HEREDOC.
//   n2_close_is_a_file     MEASURED (2026-11-01): `echo hi 2>&-` and
//                          `echo hi 2>-` render BYTE-IDENTICAL C — the IR
//                          has no Close mode (ir.rs:569), so both map to
//                          (fd 2, mode "w", target "-"). The emitted text
//                          is `2>-`, and bash reads `>-` as OPEN A FILE
//                          NAMED `-` (measured: file `-` created, rc 0)
//                          while `>&-` CLOSES the fd (measured: "Bad file
//                          descriptor", rc 1). So for the close source the
//                          generated program opens a file where bash
//                          closes an fd — and no renderer over this IR can
//                          be correct for both sources. The corpus reaches
//                          it (3 examples: arith-ambiguous-double-paren,
//                          nested-subshell-paren, parse-dollar-paren-pipe);
//                          their stdout still matches, so the C gate is
//                          green over it. Fix belongs in the SHARED CORE
//                          (parser/ir.rs: a Close operator or an `&-`
//                          target spelling) — out of scope for a backend
//                          model; recorded here as the proven defect.
//   n2' also pins the NATIVE exec path: for `exec 3>&-` the generator
//                          emits `open("-", O_WRONLY|O_CREAT|O_TRUNC, ...)`
//                          + `dup2` (measured in dup.c:870) — the same
//                          confusion, one layer down.
//   flip_len / flip_frame / first_here_ext / flip_safe_mono
//                          the process-substitution producer flip runs
//                          BEFORE the partition (c_backend.rs:22660-22679):
//                          the FIRST `r`-on-fd-0 read of a `__ps_*` Var
//                          becomes `w` on fd 1 (the producer writes). It
//                          is length-preserving, keeps every element a
//                          heredoc or not, and can only REMOVE fd-0
//                          touches — so safety of the source list implies
//                          safety of the flipped list.
//   m1..m3                 mutants: dropped escaper, quoted dup arm,
//                          heredoc left mid-list.
//
// NOT modelled (fidelity, checked against the source):
//   * fd >= 10 and `&NN` targets — the model's digit domain is 0..9
//     (fd_wf / Amp::OutOfDomain); the emitted TEXT is still byte-faithful
//     there, only the Table value is `Unknown`.
//   * glob EXPANSION of a redirect target (`Tgt::Glob` emits its pattern
//     unquoted so the child expands it) — `Spec::File` records the
//     pattern, not the expanded filename list.
//   * heredoc body INTERPOLATION content (the export-vars step,
//     c_backend.rs:22726-22742) and the `<<-` tab-strip (`heredoc_body_c`
//     — the core pre-strips, per the source comment at 22717); `Spec::
//     Content` carries the final text either way.
//   * `set -C` noclobber: `>` and `>|` denote the same Spec here; the two
//     operators differ only under the option, and the renderer emits the
//     SAME operator text bash saw, so the option behaves identically.
//   * the NATIVE exec path (redirects on `exec` are rendered as
//     open/dup2 syscalls, not shell text — see n2'), and every other
//     statement-level optimization.
//   * an unset fd (`rd.fd == None`) reaching the renderer: the producer
//     always supplies the operator-correct default (shir.rs:19459-19483),
//     and fd_wf excludes the `unwrap_or(1)` fallback on input operators.
use vstd::prelude::*;

verus! {

// vstd's broadcast lemma groups are NOT in scope by default; the seq
// skip/subrange laws carry every index step below.
broadcast use {vstd::seq::group_seq_axioms, vstd::seq_lib::group_seq_lib_default};

pub type Str = Seq<char>;
pub type Cmd = Seq<char>;
pub type Tab = Map<int, Spec>;

// ---------------------------------------------------------------------
// 1. The IR redirect as the renderer branches on it
// ---------------------------------------------------------------------

pub enum Mode {
    W,          // "w"      `>`      (also `2>` and the dup/close arm)
    Wc,         // "wc"     `>|`
    A,          // "a"      `>>`
    R,          // "r"      `<`
    Rplus,      // "r+"     `<`      (same arm as R)
    Here,       // "heredoc"
    HereTabs,   // "heredoc-tabs"
    Hstr,       // "herestring"
    ProcIn,     // "process-in"
    ProcOut,    // "process-out"
    Other(Str), // every other mode string, incl. "unsupported" → dropped
}

pub enum Tgt {
    Lit(Str),       // IrExpr::Str — the text is also the runtime value
    Glob(Str),      // IrExpr::Str with the \x01SH2GLOB\x01 marker; s is the
                    // STRIPPED pattern (sh_word emits it raw)
    Var(Str, Str),  // IrExpr::Var — (name, runtime value)
    Dyn(Str),       // any other IrExpr, abstracted to its runtime value
}

pub struct Rd {
    pub fd: Option<int>,
    pub mode: Mode,
    pub tgt: Tgt,
    pub interp: bool, // the heredoc's unquoted-delimiter flag
}

pub open spec fn glob_marker() -> Str {
    seq!['\u{1}', 'S', 'H', '2', 'G', 'L', 'O', 'B', '\u{1}']
}

// the value the child shell sees for this target
pub open spec fn val(t: Tgt) -> Str {
    match t {
        Tgt::Lit(s) => s,
        Tgt::Glob(s) => s,
        Tgt::Var(_, v) => v,
        Tgt::Dyn(s) => s,
    }
}

// the IrExpr::Str text the SOURCE matches on (globs still carry the marker)
pub open spec fn str_text(t: Tgt) -> Option<Str> {
    match t {
        Tgt::Lit(s) => Some(s),
        Tgt::Glob(s) => Some(glob_marker() + s),
        _ => None,
    }
}

pub open spec fn contains_char(s: Str, c: char) -> bool
    decreases s.len()
{
    if s.len() == 0 { false } else { s[0] == c || contains_char(s.skip(1), c) }
}

// the source's heredoc sub-arm condition:
//   `rd.interpolate && !matches!(&rd.target, Str(s, _) if !s.contains('$'))`
pub open spec fn lit_no_dollar(t: Tgt) -> bool {
    match str_text(t) {
        Some(s) => !contains_char(s, '$'),
        None => false,
    }
}

pub open spec fn is_here_rd(rd: Rd) -> bool {
    match rd.mode {
        Mode::Here | Mode::HereTabs | Mode::Hstr => true,
        _ => false,
    }
}

pub open spec fn is_dropped(rd: Rd) -> bool {
    match rd.mode {
        Mode::Other(_) => true,
        _ => false,
    }
}

// ---------------------------------------------------------------------
// 2. The fd prefix — `format!("{fd}")` unless fd == 1 (c_backend.rs:22704)
// ---------------------------------------------------------------------

pub open spec fn eff_fd(rd: Rd) -> int {
    match rd.fd { Some(n) => n, None => 1 }
}

// bash's default fd when the operator carries no numeric prefix
pub open spec fn op_default(m: Mode) -> int {
    match m {
        Mode::R | Mode::Rplus | Mode::Here | Mode::HereTabs | Mode::Hstr | Mode::ProcIn => 0,
        Mode::W | Mode::Wc | Mode::A | Mode::ProcOut | Mode::Other(_) => 1,
    }
}

pub open spec fn digit_char(n: int) -> char {
    if n == 0 { '0' } else if n == 1 { '1' } else if n == 2 { '2' } else if n == 3 { '3' }
    else if n == 4 { '4' } else if n == 5 { '5' } else if n == 6 { '6' } else if n == 7 { '7' }
    else if n == 8 { '8' } else if n == 9 { '9' } else { '?' }
}

pub open spec fn digit_val(c: char) -> int {
    if c == '0' { 0 } else if c == '1' { 1 } else if c == '2' { 2 } else if c == '3' { 3 }
    else if c == '4' { 4 } else if c == '5' { 5 } else if c == '6' { 6 } else if c == '7' { 7 }
    else if c == '8' { 8 } else if c == '9' { 9 } else { -1 }
}

pub open spec fn fd_pre(rd: Rd) -> Str {
    if eff_fd(rd) == 1 { Seq::empty() } else { seq![digit_char(eff_fd(rd))] }
}

// what bash parses from the prefix (absent → the operator's default)
pub open spec fn parse_pre(pre: Str, dflt: int) -> int {
    if pre.len() == 0 { dflt } else if pre.len() == 1 { digit_val(pre[0]) } else { -1 }
}

// the fd the EMITTED TEXT attaches this redirection to. Heredoc and
// herestring markers carry no prefix in any case — bash bolts them to
// stdin (the emitter never emits fd_pre for these arms).
pub open spec fn parsed_fd(rd: Rd) -> int {
    match rd.mode {
        Mode::Here | Mode::HereTabs | Mode::Hstr => 0,
        _ => parse_pre(fd_pre(rd), op_default(rd.mode)),
    }
}

pub open spec fn target_fd(rd: Rd) -> int {
    parsed_fd(rd)
}

// The model's fd domain: 0..9, and fd 1 only where the operator's own
// default is 1 (the `>` family). The second conjunct is what makes the
// renderer's `unwrap_or(1)` + "empty prefix" rule sound for INPUT
// operators, whose bash default is 0 — the producer supplies `Some(0)`
// for those (shir.rs), so the conjunct never bites in practice.
pub open spec fn fd_wf(rd: Rd) -> bool {
    0 <= eff_fd(rd) < 10
        && (!is_here_rd(rd) && eff_fd(rd) == 1 ==> op_default(rd.mode) == 1)
}

// ---------------------------------------------------------------------
// 3. The word quoter — `_sh_word` (emitted at c_backend.rs:12122)
//
//   _sh_add(" '");  for each c:  c == '\'' ? _sh_add("'\"'\"'") : _sh_addc(c);
//   _sh_addc('\'');
// ---------------------------------------------------------------------

pub open spec fn esc(v: Str) -> Str
    decreases v.len()
{
    if v.len() == 0 {
        Seq::empty()
    } else {
        let c = v[0];
        let head = if c == '\'' { seq!['\'', '"', '\'', '"', '\''] } else { seq![c] };
        head + esc(v.skip(1))
    }
}

// the WORD itself: an opening quote, the escaped bytes, a closing quote
pub open spec fn quoted_word(v: Str) -> Cmd {
    seq!['\''] + esc(v) + seq!['\'']
}

// the whole contribution of `_sh_word(v)`: a word-SEPARATING space
// (bash tokenizes on it) then the quoted word
pub open spec fn word_frag(v: Str) -> Cmd {
    seq![' '] + quoted_word(v)
}

// `sh_word`'s contribution per target shape: a glob-marked literal is
// emitted RAW (stripped) so the child expands it; everything else is
// single-quoted (c_backend.rs:20597-20614).
pub open spec fn word_text(t: Tgt) -> Cmd {
    match t {
        Tgt::Glob(s) => seq![' '] + s,
        _ => word_frag(val(t)),
    }
}

// A three-state lexer over ONE CHAR PER STEP (the c_str_lit.rs discipline):
// Bare sees quotes and opens them; Sq copies everything until the closing
// quote; Dq copies everything until the closing double quote — including
// `'`, which is literal inside double quotes and is exactly the char the
// escaper's `'"'"'` fragment needs to pass through.
pub enum Q { Bare, Sq, Dq }

pub struct St { pub q: Q, pub out: Str }

pub open spec fn step(c: char, s: St) -> St {
    match s.q {
        Q::Bare => if c == '\'' {
            St { q: Q::Sq, out: s.out }
        } else if c == '"' {
            St { q: Q::Dq, out: s.out }
        } else {
            St { q: Q::Bare, out: s.out.push(c) }
        },
        Q::Sq => if c == '\'' {
            St { q: Q::Bare, out: s.out }
        } else {
            St { q: Q::Sq, out: s.out.push(c) }
        },
        Q::Dq => if c == '"' {
            St { q: Q::Bare, out: s.out }
        } else {
            St { q: Q::Dq, out: s.out.push(c) }
        },
    }
}

pub open spec fn run(t: Str, s: St) -> St
    decreases t.len()
{
    if t.len() == 0 { s } else { run(t.skip(1), step(t[0], s)) }
}

// ---------------------------------------------------------------------
// 4. frag — the exact shell text one redirect contributes
// ---------------------------------------------------------------------

pub open spec fn delim() -> Str {
    seq!['_', 'S', 'H', '2', 'E', 'O', 'F', '_']
}

// the emitted ensure-newline snippet: append "\n" iff the body is
// non-empty and does not already end in one (c_backend.rs:22743-22746)
pub open spec fn ensure_nl(b: Str) -> Str {
    if b.len() > 0 && b[b.len() - 1] != '\n' { seq!['\n'] } else { Seq::empty() }
}

// the raw-arm condition, verbatim: `t.starts_with('&') || *t == "-"`
pub open spec fn raw_arm(t: Tgt) -> bool {
    match t {
        Tgt::Lit(s) => (s.len() > 0 && s[0] == '&') || s == seq!['-'],
        _ => false,
    }
}

// how bash reads an `&`-form target once it is emitted: `&N` dups fd N,
// `&-` closes. `&NN` (N >= 10) is outside the model's digit domain.
pub enum Amp { None, Dup(int), Close, OutOfDomain }

pub open spec fn amp_form(t: Tgt) -> Amp {
    match t {
        Tgt::Lit(s) => {
            if s.len() >= 2 && s[0] == '&' {
                if s.len() > 2 {
                    Amp::OutOfDomain
                } else if s[1] == '-' {
                    Amp::Close
                } else if digit_val(s[1]) >= 0 {
                    Amp::Dup(digit_val(s[1]))
                } else {
                    Amp::OutOfDomain
                }
            } else {
                Amp::None
            }
        }
        _ => Amp::None,
    }
}

pub open spec fn access_of(m: Mode) -> Access {
    match m {
        Mode::A => Access::Append,
        Mode::R | Mode::Rplus => Access::Read,
        _ => Access::Write,
    }
}

pub open spec fn frag(rd: Rd) -> Cmd {
    let p = fd_pre(rd);
    let t = rd.tgt;
    let v = val(t);
    match rd.mode {
        // `w`: the dup/close arm first (NO quoting — a quoted `&1` would
        // name a file), then the plain file arm through sh_word.
        Mode::W => {
            if raw_arm(t) {
                seq![' '] + p + seq!['>'] + v
            } else {
                seq![' '] + p + seq!['>'] + word_text(t)
            }
        },
        Mode::Wc => seq![' '] + p + seq!['>', '|'] + word_text(t),
        Mode::A => seq![' '] + p + seq!['>', '>'] + word_text(t),
        Mode::R | Mode::Rplus => {
            if raw_arm(t) {
                seq![' '] + p + seq!['<'] + v
            } else {
                seq![' '] + p + seq!['<'] + word_text(t)
            }
        },
        Mode::Here | Mode::HereTabs => {
            // interpolating body → unquoted delimiter + exported vars;
            // everything else → quoted delimiter (literal body). Both
            // shapes end: body, ensure-newline, delimiter.
            if rd.interp && !lit_no_dollar(t) {
                seq![' '] + seq!['<', '<'] + delim() + seq!['\n'] + v + ensure_nl(v) + delim()
            } else {
                seq![' '] + seq!['<', '<', '\''] + delim() + seq!['\'', '\n'] + v + ensure_nl(v)
                    + delim()
            }
        },
        Mode::Hstr => seq![' '] + seq!['<', '<', '<'] + word_text(t),
        // raw(" <(") + addv(v) + raw(")") — note the space before `)`,
        // transcribed exactly (bash accepts it).
        Mode::ProcIn => seq![' '] + seq!['<', '('] + v + seq![' ', ')'],
        Mode::ProcOut => seq![' '] + seq!['>', '('] + v + seq![' ', ')'],
        Mode::Other(_) => Seq::empty(),
    }
}

// ---------------------------------------------------------------------
// 5. The denotation: bash's fd table
// ---------------------------------------------------------------------

pub enum Access { Read, Write, Append }

pub enum Spec {
    Dev(Str),        // the startup endpoints: stdin/stdout/stderr
    File(Str, Access),
    Content(Str),    // heredoc/herestring materialised on the fd
    Fifo(Str),       // process substitution
    Closed,          // `>&-` / `<&-`
    Bad,             // dup of an fd that was never opened (bash errors)
    Unknown,         // outside the model's fd domain (see header)
}

pub open spec fn written(rd: Rd, t: Tab) -> Spec {
    match rd.mode {
        Mode::W | Mode::Wc | Mode::A | Mode::R | Mode::Rplus => {
            match amp_form(rd.tgt) {
                Amp::Dup(n) => match t.get(n) { Some(sp) => sp, None => Spec::Bad },
                Amp::Close => Spec::Closed,
                Amp::OutOfDomain => Spec::Unknown,
                Amp::None => Spec::File(val(rd.tgt), access_of(rd.mode)),
            }
        },
        Mode::Here | Mode::HereTabs => Spec::Content(val(rd.tgt)),
        Mode::Hstr => Spec::Content(val(rd.tgt) + seq!['\n']),
        Mode::ProcIn | Mode::ProcOut => Spec::Fifo(val(rd.tgt)),
        Mode::Other(_) => Spec::Unknown,
    }
}

pub open spec fn apply(rd: Rd, t: Tab) -> Tab {
    if is_dropped(rd) { t } else { t.insert(target_fd(rd), written(rd, t)) }
}

// fold-LEFT: the head redirect applies first (bash's source order)
pub open spec fn apply_list(rds: Seq<Rd>, t: Tab) -> Tab
    decreases rds.len()
{
    if rds.len() == 0 { t } else { apply_list(rds.skip(1), apply(rds[0], t)) }
}

pub open spec fn startup() -> Tab {
    Map::empty().insert(0, Spec::Dev(seq!['s', 't', 'd', 'i', 'n']))
        .insert(1, Spec::Dev(seq!['s', 't', 'd', 'o', 'u', 't']))
        .insert(2, Spec::Dev(seq!['s', 't', 'd', 'e', 'r', 'r']))
}

// ---------------------------------------------------------------------
// 6. Who touches fd 0 — the safety condition
// ---------------------------------------------------------------------

pub open spec fn writes0(rd: Rd) -> bool {
    !is_dropped(rd) && target_fd(rd) == 0
}

// `written` matches `W | Wc | A | R | Rplus` before consulting the dup
// form, so THOSE are exactly the modes under which a Dup(0) target makes
// it read fd 0. (Widened 2026-11-01: with the old W|R|Rplus set a Wc/A
// Rd whose target is `&0` had reads0 == false while `written` still
// read t.get(0) — `written_frame`'s frame rule was provably false there.)
pub open spec fn reads0(rd: Rd) -> bool {
    !is_dropped(rd)
        && match rd.mode {
            Mode::W | Mode::Wc | Mode::A | Mode::R | Mode::Rplus => true,
            _ => false,
        }
        && amp_form(rd.tgt) == Amp::Dup(0)
}

pub open spec fn touches0(rd: Rd) -> bool {
    writes0(rd) || reads0(rd)
}

// index of the FIRST heredoc, or rds.len() when there is none
pub open spec fn first_here(rds: Seq<Rd>) -> int
    decreases rds.len()
{
    if rds.len() == 0 {
        0
    } else if is_here_rd(rds[0]) {
        0
    } else {
        let f = first_here(rds.skip(1));
        if f == rds.len() as int - 1 { rds.len() as int } else { f + 1 }
    }
}

// SAFE: every non-heredoc at or after the first heredoc leaves fd 0 alone.
// This is exactly the condition under which the partition does not invert
// a heredoc past a redirect that reads or writes fd 0.
pub open spec fn safe(rds: Seq<Rd>) -> bool {
    forall|j: int| first_here(rds) <= j && j < rds.len() ==> (!is_here_rd(rds[j]) ==> !touches0(rds[j]))
}

// ---------------------------------------------------------------------
// 7. The emission order (partition) and the producer flip
// ---------------------------------------------------------------------

pub open spec fn non_heres(rds: Seq<Rd>) -> Seq<Rd>
    decreases rds.len()
{
    if rds.len() == 0 {
        Seq::empty()
    } else if is_here_rd(rds[0]) {
        non_heres(rds.skip(1))
    } else {
        seq![rds[0]] + non_heres(rds.skip(1))
    }
}

pub open spec fn heres(rds: Seq<Rd>) -> Seq<Rd>
    decreases rds.len()
{
    if rds.len() == 0 {
        Seq::empty()
    } else if is_here_rd(rds[0]) {
        seq![rds[0]] + heres(rds.skip(1))
    } else {
        heres(rds.skip(1))
    }
}

pub open spec fn emit_order(rds: Seq<Rd>) -> Seq<Rd> {
    non_heres(rds) + heres(rds)
}

// the producer-direction flip (c_backend.rs:22660-22679): the FIRST
// `r`-on-fd-0 read of a `__ps_*` Var is re-tagged `w` on fd 1 — the
// process-substitution producer writes into the FIFO the first time.
pub open spec fn is_ps_read(rd: Rd) -> bool {
    match rd.mode {
        Mode::R => match rd.fd {
            Some(n) => n == 0 && match rd.tgt {
                Tgt::Var(nm, _) => nm.len() >= 5 && nm.subrange(0, 5) == seq!['_', '_', 'p', 's', '_'],
                _ => false,
            },
            None => false,
        },
        _ => false,
    }
}

pub open spec fn ps_name(rd: Rd) -> Str {
    match rd.tgt { Tgt::Var(nm, _) => nm, _ => seq![] }
}

pub open spec fn flip_from(rds: Seq<Rd>, seen: Set<Str>) -> Seq<Rd>
    decreases rds.len()
{
    if rds.len() == 0 {
        Seq::empty()
    } else {
        let rd = rds[0];
        let tail = rds.skip(1);
        if is_ps_read(rd) && !seen.contains(ps_name(rd)) {
            seq![Rd { fd: Some(1), mode: Mode::W, tgt: rd.tgt, interp: rd.interp }]
                + flip_from(tail, seen.insert(ps_name(rd)))
        } else {
            seq![rd] + flip_from(tail, seen)
        }
    }
}

pub open spec fn flip(rds: Seq<Rd>) -> Seq<Rd> {
    flip_from(rds, Set::empty())
}

// =====================================================================
// PROOFS
// =====================================================================

// --- 2. the prefix ----------------------------------------------------

proof fn digit_roundtrip(n: int)
    ensures 0 <= n < 10 ==> digit_val(digit_char(n)) == n
{
    if 0 <= n < 10 {
        if n == 0 { assert(digit_val(digit_char(0)) == 0); }
        else if n == 1 { assert(digit_val(digit_char(1)) == 1); }
        else if n == 2 { assert(digit_val(digit_char(2)) == 2); }
        else if n == 3 { assert(digit_val(digit_char(3)) == 3); }
        else if n == 4 { assert(digit_val(digit_char(4)) == 4); }
        else if n == 5 { assert(digit_val(digit_char(5)) == 5); }
        else if n == 6 { assert(digit_val(digit_char(6)) == 6); }
        else if n == 7 { assert(digit_val(digit_char(7)) == 7); }
        else if n == 8 { assert(digit_val(digit_char(8)) == 8); }
        else { assert(n == 9); assert(digit_val(digit_char(9)) == 9); }
        assert(digit_val(digit_char(n)) == n);
    }
}

// The prefix parses back to the fd it was rendered from.
pub proof fn fd_parse(rd: Rd)
    requires fd_wf(rd), !is_here_rd(rd)
    ensures parsed_fd(rd) == eff_fd(rd)
{
    if eff_fd(rd) == 1 {
        assert(fd_pre(rd).len() == 0);
        assert(parse_pre(fd_pre(rd), op_default(rd.mode)) == op_default(rd.mode));
        assert(op_default(rd.mode) == 1);
        assert(parsed_fd(rd) == 1);
    } else {
        digit_roundtrip(eff_fd(rd));
        assert(fd_pre(rd).len() == 1);
        assert(digit_val(fd_pre(rd)[0]) == eff_fd(rd));
        assert(parsed_fd(rd) == eff_fd(rd));
    }
}

// --- 3. the quoter ----------------------------------------------------

// composition: running a ++ b is running a, then b
proof fn run_concat(a: Str, b: Str, s: St)
    ensures run(a + b, s) == run(b, run(a, s))
    decreases a.len()
{
    if a.len() == 0 {
        assert(a + b == b);
    } else {
        run_concat(a.skip(1), b, step(a[0], s));
        assert((a + b).skip(1) == a.skip(1) + b);
    }
}

// the escaper, run from inside single quotes, reproduces its input and
// returns to Sq — the `'` case walks `'` `"` `'` `"` `'`:
//   Sq --'>  Bare --'"'--> Dq --'\''--> Dq(out+'\'') --'"'--> Bare --'\''--> Sq
proof fn esc_in_sq(v: Str, p: Str)
    ensures run(esc(v), St { q: Q::Sq, out: p }) == (St { q: Q::Sq, out: p + v })
    decreases v.len()
{
    if v.len() == 0 {
        assert(esc(v).len() == 0);
        assert(v == Seq::empty());
        assert(esc(v) == Seq::empty());
        assert(run(esc(v), St { q: Q::Sq, out: p }) == St { q: Q::Sq, out: p + v });
    } else {
        let c = v[0];
        let rest = v.skip(1);
        assert(v == seq![c] + rest);
        if c == '\'' {
            let five = seq!['\'', '"', '\'', '"', '\''];
            assert(esc(v) == five + esc(rest));
            run_concat(five, esc(rest), St { q: Q::Sq, out: p });
            let s1 = step('\'', St { q: Q::Sq, out: p });
            assert(s1.q == Q::Bare && s1.out == p);
            let s2 = step('"', s1);
            assert(s2.q == Q::Dq && s2.out == p);
            let s3 = step('\'', s2);
            assert(s3.q == Q::Dq && s3.out == p + seq!['\'']);
            let s4 = step('"', s3);
            assert(s4.q == Q::Bare && s4.out == p + seq!['\'']);
            let s5 = step('\'', s4);
            assert(s5.q == Q::Sq && s5.out == p + seq!['\'']);
            let four = seq!['"', '\'', '"', '\''];
            let three = seq!['\'', '"', '\''];
            let two = seq!['"', '\''];
            let one_q = seq!['\''];
            assert(five == seq!['\''] + four);
            assert(four == seq!['"'] + three);
            assert(three == seq!['\''] + two);
            assert(two == seq!['"'] + one_q);
            run_one('\'', St { q: Q::Sq, out: p });
            run_concat(seq!['\''], four, St { q: Q::Sq, out: p });
            run_one('"', s1);
            run_concat(seq!['"'], three, s1);
            run_one('\'', s2);
            run_concat(seq!['\''], two, s2);
            run_one('"', s3);
            run_concat(seq!['"'], one_q, s3);
            run_one('\'', s4);
            assert(run(five, St { q: Q::Sq, out: p }) == s5);
            esc_in_sq(rest, p + seq!['\'']);
            assert(run(five + esc(rest), St { q: Q::Sq, out: p }) == run(esc(rest), s5));
            assert(run(esc(v), St { q: Q::Sq, out: p }) == run(esc(rest), s5));
            assert(run(esc(rest), s5) == St { q: Q::Sq, out: p + seq!['\''] + rest });
            assert(p + seq!['\''] + rest == p + v);
        } else {
            let one = seq![c];
            assert(esc(v) == one + esc(rest));
            run_concat(one, esc(rest), St { q: Q::Sq, out: p });
            let s1 = step(c, St { q: Q::Sq, out: p });
            assert(s1.q == Q::Sq && s1.out == p + seq![c]);
            run_one(c, St { q: Q::Sq, out: p });
            assert(run(one, St { q: Q::Sq, out: p }) == s1);
            esc_in_sq(rest, p + seq![c]);
            assert(run(one + esc(rest), St { q: Q::Sq, out: p }) == run(esc(rest), s1));
            assert(run(esc(v), St { q: Q::Sq, out: p }) == run(esc(rest), s1));
            assert(run(esc(rest), s1) == St { q: Q::Sq, out: p + seq![c] + rest });
            assert(p + seq![c] + rest == p + v);
        }
    }
}

// run over a ONE-element list is its single step: the unfold plus the
// literal skip/index facts, stated explicitly. (`by (compute)` on `run`
// with a symbolic state was measured to spin in the interpreter thread
// for >15 minutes, so the structural form is used instead.)
proof fn run_one(c: char, s: St)
    ensures run(seq![c], s) == step(c, s)
{
    assert(seq![c].skip(1) == Seq::empty());
    assert(seq![c][0] == c);
    assert(run(seq![c], s) == run(seq![c].skip(1), step(seq![c][0], s)));
    assert(run(Seq::empty(), step(c, s)) == step(c, s));
}

// THE QUOTING THEOREM: the bytes the child shell word-parses out of
// `_sh_word(v)` are exactly v.
pub proof fn quote_roundtrip(v: Str)
    ensures run(quoted_word(v), St { q: Q::Bare, out: Seq::empty() }) == (St { q: Q::Bare, out: v })
{
    let open_q = seq!['\''];
    assert(quoted_word(v) == open_q + esc(v) + open_q);
    run_concat(open_q, esc(v) + open_q, St { q: Q::Bare, out: Seq::empty() });
    let s0 = step('\'', St { q: Q::Bare, out: Seq::empty() });
    assert(s0.q == Q::Sq && s0.out == Seq::empty());
    run_one('\'', St { q: Q::Bare, out: Seq::empty() });
    assert(run(open_q, St { q: Q::Bare, out: Seq::empty() }) == s0);
    esc_in_sq(v, Seq::empty());
    run_concat(esc(v), open_q, St { q: Q::Sq, out: Seq::empty() });
    assert(run(esc(v), St { q: Q::Sq, out: Seq::empty() }) == St { q: Q::Sq, out: v });
    let s1 = step('\'', St { q: Q::Sq, out: v });
    assert(s1.q == Q::Bare && s1.out == v);
    run_one('\'', St { q: Q::Sq, out: v });
    assert(run(open_q, St { q: Q::Sq, out: v }) == s1);
    assert(s1.q == Q::Bare && s1.out == v);
    assert(s1 == St { q: Q::Bare, out: v });
    assert(run(quoted_word(v), St { q: Q::Bare, out: Seq::empty() })
        == run(open_q + esc(v) + open_q, St { q: Q::Bare, out: Seq::empty() }));
    // (run_concat builds RIGHT-nested sums; the chain is left-nested, so
    // split at the inner boundary instead of appealing to associativity)
    run_concat(open_q + esc(v), open_q, St { q: Q::Bare, out: Seq::empty() });
    run_concat(open_q, esc(v), St { q: Q::Bare, out: Seq::empty() });
    assert(run(open_q + esc(v) + open_q, St { q: Q::Bare, out: Seq::empty() })
        == run(open_q, St { q: Q::Sq, out: v }));
}

// m1 — the escaper dropped: the raw bytes between quotes. An embedded
// `'` CLOSES the word early; the state machine ends inside Sq with the
// tail lost (compute over the concrete three-char input).
proof fn m1_esc_dropped()
    ensures run(seq![' '] + seq!['\''] + seq!['a', '\'', 'b'] + seq!['\''],
                St { q: Q::Bare, out: Seq::empty() })
                != (St { q: Q::Bare, out: seq!['a', '\'', 'b'] })
{
    assert(
        run(seq![' '] + seq!['\''] + seq!['a', '\'', 'b'] + seq!['\''],
            St { q: Q::Bare, out: Seq::empty() })
            != St { q: Q::Bare, out: seq!['a', '\'', 'b'] }
    ) by (compute);
}

// --- 4. the terminator is why the reorder exists ----------------------

// GUARDED: the tail window of `pre + s` is s's tail window only once the
// window FITS INSIDE s (|suf| <= |s|); without the guard the lemma is
// false — pre itself can end in suf (`pre="ab", s="", suf="b"`).
pub proof fn ends_with_concat(pre: Str, s: Str, suf: Str)
    requires suf.len() as int <= s.len() as int
    ensures ends_with(pre + s, suf) == ends_with(s, suf)
{
    assert((pre + s).len() == pre.len() + s.len());
    let k: int = s.len() as int - suf.len() as int;
    let off: int = pre.len() as int + k;
    assert((pre + s).subrange(off, (pre + s).len() as int) == s.subrange(k, s.len() as int));
}

pub open spec fn ends_with(s: Str, suf: Str) -> bool {
    s.len() as int >= suf.len() as int
        && s.subrange(s.len() as int - suf.len() as int, s.len() as int) == suf
}

// A redirect word AFTER a heredoc fragment would sit on the delimiter
// line — bash then never sees the end of the body. This is the necessity
// half of the reorder: heres must be last, so the safety condition below
// is the price, not a choice.
pub proof fn heredoc_last_forced(hd: Str, o: Str)
    requires ends_with(hd, delim()), o.len() as int >= delim().len() as int
    ensures ends_with(hd + o, delim()) == ends_with(o, delim())
{
    ends_with_concat(hd, o, delim());
}

// --- 5. table lemmas --------------------------------------------------

proof fn insert_commute(t: Tab, k1: int, v1: Spec, k2: int, v2: Spec)
    requires k1 != k2
    ensures t.insert(k1, v1).insert(k2, v2) == t.insert(k2, v2).insert(k1, v1)
{
    let lhs = t.insert(k1, v1).insert(k2, v2);
    let rhs = t.insert(k2, v2).insert(k1, v1);
    assert forall|x: int| lhs.get(x) == rhs.get(x) by {
        if x == k2 {
            assert(lhs.get(x) == Some(v2));
            assert(rhs.get(x) == Some(v2));
        } else if x == k1 {
            assert(lhs.get(x) == Some(v1));
            assert(rhs.get(x) == Some(v1));
        } else {
            assert(lhs.get(x) == t.get(x));
            assert(rhs.get(x) == t.get(x));
        }
    }
    assert(lhs == rhs);
}

proof fn insert_get_other(t: Tab, k: int, v: Spec, x: int)
    requires x != k
    ensures t.insert(k, v).get(x) == t.get(x)
{
    assert(t.insert(k, v).get(x) == t.get(x));
}

// `written` reads the table ONLY at the dup source fd.
proof fn written_frame(o: Rd, t: Tab, v: Spec)
    requires !reads0(o)
    ensures written(o, t.insert(0, v)) == written(o, t)
{
    match o.mode {
        Mode::W | Mode::Wc | Mode::A | Mode::R | Mode::Rplus => {
            match amp_form(o.tgt) {
                Amp::Dup(n) => {
                    // reads0 is exactly (mode in W|Wc|A|R|Rplus && amp == Dup(0)),
                    // so !reads0 with this arm forces n != 0
                    assert(n != 0);
                    insert_get_other(t, 0, v, n);
                },
                _ => { },
            }
        },
        _ => { },
    }
}

// heredoc/herestring bodies never read the table
proof fn written_here_const(hd: Rd, t1: Tab, t2: Tab)
    requires is_here_rd(hd)
    ensures written(hd, t1) == written(hd, t2)
{
}

// THE SWAP: a heredoc and a redirect that does not touch fd 0 commute.
pub proof fn commute_here(hd: Rd, o: Rd, t: Tab)
    requires is_here_rd(hd), !touches0(o)
    ensures apply(hd, apply(o, t)) == apply(o, apply(hd, t))
{
    assert(target_fd(hd) == 0);
    if is_dropped(o) {
        assert(apply(o, t) == t);
    } else {
        let k = target_fd(o);
        assert(writes0(o) == (!is_dropped(o) && k == 0));
        assert(k != 0);
        let c = written(hd, t);
        written_here_const(hd, t, apply(o, t));
        let w0 = written(o, t);
        written_frame(o, t, c);
        assert(written(o, t.insert(0, c)) == w0);
        // left:  (t.insert(k, w0)).insert(0, c)
        // right: (t.insert(0, c)).insert(k, w0)
        assert(apply(o, t) == t.insert(k, w0));
        assert(apply(hd, t) == t.insert(0, c));
        assert(apply(hd, apply(o, t)) == t.insert(k, w0).insert(0, c));
        assert(apply(o, apply(hd, t)) == t.insert(0, c).insert(k, w0));
        insert_commute(t, k, w0, 0, c);
    }
}

pub proof fn fold_concat(x: Seq<Rd>, y: Seq<Rd>, t: Tab)
    ensures apply_list(x + y, t) == apply_list(y, apply_list(x, t))
    decreases x.len()
{
    if x.len() == 0 {
        assert(x + y == y);
    } else {
        fold_concat(x.skip(1), y, apply(x[0], t));
        assert((x + y).skip(1) == x.skip(1) + y);
        assert((x + y)[0] == x[0]);
    }
}

// a heredoc commutes with a whole run of fd-0-blind redirects
pub proof fn fold_commute(hd: Rd, a: Seq<Rd>, t: Tab)
    requires is_here_rd(hd), forall|i: int| 0 <= i < a.len() ==> !touches0(a[i])
    ensures apply_list(a, apply(hd, t)) == apply(hd, apply_list(a, t))
    decreases a.len()
{
    if a.len() == 0 {
    } else {
        commute_here(hd, a[0], t);
        fold_commute(hd, a.skip(1), apply(a[0], t));
        assert(apply_list(a, apply(hd, t))
            == apply_list(a.skip(1), apply(a[0], apply(hd, t))));
        assert(apply(a[0], apply(hd, t)) == apply(hd, apply(a[0], t)));
        assert(apply_list(a.skip(1), apply(hd, apply(a[0], t)))
            == apply(hd, apply_list(a.skip(1), apply(a[0], t))));
    }
}

// --- 6. index plumbing for the partition ------------------------------

// every element of `non_heres(rds)` sits at an index >= its own in rds,
// and is not a heredoc there
pub proof fn non_heres_index(rds: Seq<Rd>, i: int)
    ensures 0 <= i < non_heres(rds).len() ==>
        (exists|j: int| i <= j && j < rds.len() && non_heres(rds)[i] == rds[j]
            && !is_here_rd(rds[j]))
    decreases rds.len()
{
    if rds.len() == 0 {
    } else if is_here_rd(rds[0]) {
        let tail = rds.skip(1);
        assert(non_heres(rds) == non_heres(tail));
        non_heres_index(tail, i);
        if 0 <= i < non_heres(rds).len() {
            let j = choose|jj: int|
                i <= jj && jj < tail.len() && non_heres(tail)[i] == tail[jj]
                    && !is_here_rd(tail[jj]);
            assert(tail[j] == rds[j + 1]);
            assert(non_heres(rds)[i] == rds[j + 1]);
            assert(!is_here_rd(rds[j + 1]));
            assert(i <= j + 1 && j + 1 < rds.len());
        }
    } else {
        let tail = rds.skip(1);
        assert(non_heres(rds) == seq![rds[0]] + non_heres(tail));
        non_heres_index(tail, i - 1);
        if 0 <= i < non_heres(rds).len() {
            if i == 0 {
                assert(non_heres(rds)[0] == rds[0]);
            } else {
                assert(0 <= i - 1 < non_heres(tail).len());
                let j = choose|jj: int|
                    (i - 1) <= jj && jj < tail.len() && non_heres(tail)[i - 1] == tail[jj]
                        && !is_here_rd(tail[jj]);
                assert(non_heres(rds)[i] == non_heres(tail)[i - 1]);
                assert(rds[j + 1] == tail[j]);
                assert(non_heres(rds)[i] == rds[j + 1]);
                assert(i <= j + 1 && j + 1 < rds.len());
                assert(!is_here_rd(rds[j + 1]));
            }
        }
    }
}

pub proof fn first_here_nonneg(rds: Seq<Rd>)
    ensures first_here(rds) >= 0
    decreases rds.len()
{
    if rds.len() == 0 {
    } else if is_here_rd(rds[0]) {
    } else {
        first_here_nonneg(rds.skip(1));
        let f = first_here(rds.skip(1));
        if f == rds.len() as int - 1 {
            assert(rds.len() as int >= 1);
        } else {
        }
    }
}

pub proof fn first_here_tail(rds: Seq<Rd>)
    requires rds.len() > 0
    ensures first_here(rds.skip(1)) + 1 >= first_here(rds)
    decreases rds.len()
{
    if is_here_rd(rds[0]) {
        assert(first_here(rds) == 0);
        first_here_nonneg(rds.skip(1));
    } else {
        let f = first_here(rds.skip(1));
        if f == rds.len() - 1 {
            assert(first_here(rds) == rds.len());
            assert(f + 1 == rds.len());
        } else {
            assert(first_here(rds) == f + 1);
        }
    }
}

pub proof fn safe_tail(rds: Seq<Rd>)
    requires rds.len() > 0, safe(rds)
    ensures safe(rds.skip(1))
{
    let tail = rds.skip(1);
    first_here_tail(rds);
    first_here_nonneg(tail);
    assert forall|j: int| first_here(tail) <= j && j < tail.len() implies
        (!is_here_rd(tail[j]) ==> !touches0(tail[j]))
    by {
        if !is_here_rd(tail[j]) {
            let i = j + 1;
            assert(rds.skip(1).len() == rds.len() as int - 1);
            assert(0 <= i && i < rds.len());
            assert(tail[j] == rds[i]);
            assert(first_here(rds) <= i);
            assert(!is_here_rd(rds[i]));
            assert(!touches0(rds[i]));
        }
    }
}

// --- 7. THE REORDER THEOREM ------------------------------------------

pub proof fn reorder_safe(rds: Seq<Rd>, t: Tab)
    requires safe(rds)
    ensures apply_list(emit_order(rds), t) == apply_list(rds, t)
    decreases rds.len()
{
    if rds.len() == 0 {
        assert(non_heres(rds).len() == 0 && heres(rds).len() == 0);
        assert(emit_order(rds) == rds);
    } else {
        let hd = rds[0];
        let tail = rds.skip(1);
        safe_tail(rds);
        if is_here_rd(hd) {
            let a = non_heres(tail);
            let h = heres(tail);
            assert(non_heres(rds) == a);
            assert(heres(rds) == seq![hd] + h);
            assert(emit_order(rds) == a + (seq![hd] + h));
            assert(first_here(rds) == 0);
            // every element of a sits at rds-index >= 1 (past the
            // heredoc at 0), so safe(rds) keeps it away from fd 0
            assert forall|i: int| 0 <= i < a.len() implies !touches0(a[i]) by {
                non_heres_index(tail, i);
                let j = choose|jj: int|
                    i <= jj && jj < tail.len() && non_heres(tail)[i] == tail[jj]
                        && !is_here_rd(tail[jj]);
                assert(a[i] == tail[j]);
                assert(tail[j] == rds[j + 1]);
                assert(1 <= j + 1 && j + 1 < rds.len());
                assert(!is_here_rd(rds[j + 1]));
                assert(!touches0(rds[j + 1]));
            }
            fold_commute(hd, a, t);
            fold_concat(a, seq![hd] + h, t);
            fold_concat(seq![hd], h, apply_list(a, t));
            apply_list_one(seq![hd], apply_list(a, t));
            assert(seq![hd][0] == hd);
            assert(apply_list(emit_order(rds), t)
                == apply_list(h, apply(hd, apply_list(a, t))));
            assert(apply(hd, apply_list(a, t)) == apply_list(a, apply(hd, t)));
            assert(apply_list(emit_order(rds), t)
                == apply_list(h, apply_list(a, apply(hd, t))));
            fold_concat(a, h, apply(hd, t));
            assert(apply_list(a + h, apply(hd, t)) == apply_list(emit_order(tail), apply(hd, t)));
            reorder_safe(tail, apply(hd, t));
            fold_concat(seq![hd], tail, t);
            apply_list_one(seq![hd], t);
            assert(seq![hd][0] == hd);
            assert(apply_list(emit_order(tail), apply(hd, t)) == apply_list(tail, apply(hd, t)));
            assert(apply_list(tail, apply(hd, t)) == apply_list(seq![hd] + tail, t));
            assert(seq![hd] + tail == rds);
        } else {
            assert(non_heres(rds) == seq![hd] + non_heres(tail));
            assert(heres(rds) == heres(tail));
            assert(emit_order(rds) == seq![hd] + emit_order(tail));
            reorder_safe(tail, apply(hd, t));
            fold_concat(seq![hd], emit_order(tail), t);
            apply_list_one(seq![hd], t);
            assert(seq![hd][0] == hd);
            assert(apply_list(emit_order(rds), t) == apply_list(emit_order(tail), apply(hd, t)));
            fold_concat(seq![hd], tail, t);
            assert(apply_list(tail, apply(hd, t)) == apply_list(seq![hd] + tail, t));
            assert(seq![hd] + tail == rds);
        }
    }
}

// helper: name the index non_heres_index produced (keeps the `by` block
// from having to unpack the exists inline)
// (witnesses are bound inline with `choose` where they are needed)

// apply_list over a ONE-element list reduces to its head apply: the two
// unfolds plus the empty-list identity, spelled out (the literal skip
// does not normalize on its own).
pub proof fn apply_list_one(x: Seq<Rd>, t: Tab)
    requires x.len() == 1
    ensures apply_list(x, t) == apply(x[0], t)
{
    assert(x.skip(1).len() == 0);
    assert(x.skip(1) == Seq::empty());
    assert(apply_list(x, t) == apply_list(x.skip(1), apply(x[0], t)));
    assert(apply_list(Seq::empty(), apply(x[0], t)) == apply(x[0], t));
}

// --- 8. the two measured divergences ----------------------------------

// n1: `cat <<EOF < in.txt` — heredoc first in SOURCE, file redirect
// second. MEASURED: bash prints FROM_FILE (the later `< in.txt` wins fd
// 0), the generated binary prints FROM_HEREDOC (the partition inverted
// the pair). Emitted text of the real CLI for this exact program:
//   _sh_addraw("0<"); _sh_word("in.txt"); _sh_addraw("<<'_SH2EOF_'\n"); …
pub open spec fn here_n1() -> Rd {
    Rd { fd: Some(0), mode: Mode::Here, tgt: Tgt::Lit(seq!['F', 'R', 'O', 'M', '_', 'H', '\n']),
         interp: false }
}

pub open spec fn file_n1() -> Rd {
    Rd { fd: Some(0), mode: Mode::R, tgt: Tgt::Lit(seq!['i', 'n', '.', 't', 'x', 't']),
         interp: false }
}

pub open spec fn n1() -> Seq<Rd> {
    seq![here_n1(), file_n1()]
}

pub proof fn n1_unsafe()
    ensures !safe(n1())
{
    assert(first_here(n1()) == 0);
    assert(n1()[1] == file_n1());
    assert(!is_here_rd(n1()[1]));
    assert(touches0(n1()[1]));
    assert(first_here(n1()) <= 1 && 1 < n1().len());
    assert(!safe(n1()));
}

// The condition is NECESSARY: at fd 0 the two orders disagree for EVERY
// starting table (source order ends on the file, emitted order on the
// body).
pub proof fn n1_fd0_conflict(t: Tab)
    ensures apply_list(emit_order(n1()), t).get(0) != apply_list(n1(), t).get(0)
{
    assert(non_heres(n1()) == seq![file_n1()]) by (compute);
    assert(heres(n1()) == seq![here_n1()]) by (compute);
    assert(emit_order(n1()) == seq![file_n1(), here_n1()]);
    let c = written(here_n1(), t);
    let f = written(file_n1(), t);
    assert(c == Spec::Content(val(here_n1().tgt)));
    assert(f == Spec::File(val(file_n1().tgt), Access::Read));
    assert(c != f);
    // SOURCE order: heredoc first, then `< in.txt` — the file is fd 0's
    // last writer (bash: FROM_FILE, measured)
    assert(apply(here_n1(), t) == t.insert(0, c));
    written_here_const(here_n1(), t, t.insert(0, c));
    assert(written(here_n1(), t.insert(0, c)) == c);
    assert(n1().skip(1) == seq![file_n1()]);
    assert(apply_list(n1(), t) == apply_list(n1().skip(1), apply(here_n1(), t)));
    apply_list_one(seq![file_n1()], apply(here_n1(), t));
    assert(apply_list(n1(), t) == apply(file_n1(), t.insert(0, c)));
    let f2 = written(file_n1(), t.insert(0, c));
    written_frame(file_n1(), t, c);
    assert(f2 == f);
    assert(apply_list(n1(), t).get(0) == Some(f));
    // EMITTED order: `0< 'in.txt'` first, heredoc last — the BODY is
    // fd 0's last writer (generated C: FROM_HEREDOC, measured)
    assert(apply(file_n1(), t) == t.insert(0, f));
    written_here_const(here_n1(), t, t.insert(0, f));
    assert(written(here_n1(), t.insert(0, f)) == c);
    assert(seq![file_n1(), here_n1()].skip(1) == seq![here_n1()]);
    assert(seq![file_n1(), here_n1()][0] == file_n1());
    assert(apply_list(seq![file_n1(), here_n1()], t)
        == apply_list(seq![file_n1(), here_n1()].skip(1),
                      apply(seq![file_n1(), here_n1()][0], t)));
    assert(apply_list(seq![file_n1(), here_n1()], t)
        == apply_list(seq![here_n1()], apply(file_n1(), t)));
    apply_list_one(seq![here_n1()], apply(file_n1(), t));
    assert(apply_list(emit_order(n1()), t) == apply(here_n1(), t.insert(0, f)));
    assert(apply_list(emit_order(n1()), t).get(0) == Some(c));
    assert(Some(c) != Some(f));
}

// n2: `2>&-` (bash: CLOSE fd 2) and `2>-` (bash: open a file named `-`)
// render byte-identical C — one Rd covers both sources, and the emitted
// text is `2>-`, which bash reads as a file. MEASURED 2026-11-01:
//   bash -c 'echo hi >-'   → rc 0, file `-` created with "hi"
//   bash -c 'echo hi >&-'   → rc 1, "write error: Bad file descriptor"
// The IR has no Close mode (ir.rs:569), so no renderer over this IR can
// serve both sources: the close source's table and the generated
// program's table differ at fd 2.
pub open spec fn n2() -> Rd {
    Rd { fd: Some(2), mode: Mode::W, tgt: Tgt::Lit(seq!['-']), interp: false }
}

pub proof fn n2_close_is_a_file(t: Tab)
    ensures apply(n2(), t).get(2) == Some(Spec::File(seq!['-'], Access::Write)),
        Spec::Closed != Spec::File(seq!['-'], Access::Write)
{
    assert(amp_form(n2().tgt) == Amp::None);
    assert(access_of(n2().mode) == Access::Write);
    assert(written(n2(), t) == Spec::File(seq!['-'], Access::Write));
    assert(apply(n2(), t) == t.insert(2, written(n2(), t)));
    assert(apply(n2(), t).get(2) == Some(Spec::File(seq!['-'], Access::Write)));
    assert(Spec::Closed != Spec::File(seq!['-'], Access::Write));
}

pub proof fn n2_bash_denotation_differs()
    ensures Map::<int, Spec>::empty().insert(2, Spec::Closed).get(2)
                != apply(n2(), Map::empty()).get(2)
{
    assert(Map::<int, Spec>::empty().insert(2, Spec::Closed).get(2) == Some(Spec::Closed));
    n2_close_is_a_file(Map::<int, Spec>::empty());
}

// --- 9. shape obligations and their mutants ---------------------------

// the dup/close arm never quotes: `2>&1` must not become `>&'1'` (a
// quoted form would NAME A FILE called `&1`).
pub open spec fn contains_concat(a: Str, b: Str, c: char) -> bool { contains_char(a + b, c) }

pub proof fn contains_concat_lemma(a: Str, b: Str, c: char)
    ensures contains_char(a + b, c) == (contains_char(a, c) || contains_char(b, c))
    decreases a.len()
{
    if a.len() == 0 {
        assert(a + b == b);
    } else {
        contains_concat_lemma(a.skip(1), b, c);
        assert((a + b).skip(1) == a.skip(1) + b);
        assert((a + b)[0] == a[0]);
    }
}

proof fn fd_pre_quote_free(rd: Rd)
    ensures !contains_char(fd_pre(rd), '\'')
{
    if eff_fd(rd) == 1 {
        assert(fd_pre(rd).len() == 0);
        assert(fd_pre(rd) == Seq::empty());
        assert(!contains_char(Seq::empty(), '\''));
    } else {
        let d = digit_char(eff_fd(rd));
        assert(fd_pre(rd) == seq![d]);
        assert(d != '\'');
        assert(seq![d].skip(1) == Seq::empty());
        assert(!contains_char(seq![d].skip(1), '\''));
        assert(!contains_char(seq![d], '\''));
    }
}

pub proof fn raw_arm_no_quotes(rd: Rd)
    requires raw_arm(rd.tgt), !contains_char(val(rd.tgt), '\''), fd_wf(rd),
        match rd.mode { Mode::W | Mode::R | Mode::Rplus => true, _ => false }
    ensures !contains_char(frag(rd), '\'')
{
    fd_pre_quote_free(rd);
    contains_concat_lemma(seq![' '], fd_pre(rd), '\'');
    assert(seq![' '].skip(1) == Seq::empty());
    assert(!contains_char(seq![' '].skip(1), '\''));
    assert(!contains_char(seq![' '], '\''));
    assert(!contains_char(seq![' '] + fd_pre(rd), '\''));
    match rd.mode {
        Mode::W => {
            assert(frag(rd) == seq![' '] + fd_pre(rd) + seq!['>'] + val(rd.tgt));
            contains_concat_lemma(seq![' '] + fd_pre(rd), seq!['>'], '\'');
            assert(seq!['>'].skip(1) == Seq::empty());
            assert(!contains_char(seq!['>'].skip(1), '\''));
            assert(!contains_char(seq!['>'], '\''));
            assert(!contains_char(seq![' '] + fd_pre(rd) + seq!['>'], '\''));
            contains_concat_lemma(seq![' '] + fd_pre(rd) + seq!['>'], val(rd.tgt), '\'');
            assert(!contains_char(frag(rd), '\''));
        },
        Mode::R | Mode::Rplus => {
            assert(frag(rd) == seq![' '] + fd_pre(rd) + seq!['<'] + val(rd.tgt));
            contains_concat_lemma(seq![' '] + fd_pre(rd), seq!['<'], '\'');
            assert(seq!['<'].skip(1) == Seq::empty());
            assert(!contains_char(seq!['<'].skip(1), '\''));
            assert(!contains_char(seq!['<'], '\''));
            assert(!contains_char(seq![' '] + fd_pre(rd) + seq!['<'], '\''));
            contains_concat_lemma(seq![' '] + fd_pre(rd) + seq!['<'], val(rd.tgt), '\'');
            assert(!contains_char(frag(rd), '\''));
        },
        _ => { },
    }
}

// m2 — the dup arm routed through the quoter: `2>&1` would render
// `2>&'1'`, a FILE named `&1` instead of a dup of fd 1.
proof fn m2_dup_quoted()
    ensures contains_char(seq![' '] + seq!['2', '>'] + word_frag(seq!['&', '1']), '\'')
{
    assert(contains_char(seq![' '] + seq!['2', '>'] + word_frag(seq!['&', '1']), '\'')) by (compute);
}

// m3 — a heredoc left MID-LIST: the redirect word lands on the
// delimiter line, so bash never sees the end of the body. This is the
// necessity of emit_order, computed over the real fragment shapes.
proof fn m3_heredoc_midlist()
    ensures !ends_with(frag(here_n1()) + frag(file_n1()), delim())
{
    assert(!ends_with(frag(here_n1()) + frag(file_n1()), delim())) by (compute);
}

// the shipped order puts the non-heredoc first, and the delimiter line
// then stands alone (the shipped shape, computed)
proof fn shipped_order_ends_on_delim()
    ensures ends_with(frag(file_n1()) + frag(here_n1()), delim())
{
    assert(ends_with(frag(file_n1()) + frag(here_n1()), delim())) by (compute);
}

// ensure-newline: the emitted body always ends in a newline, so the
// delimiter can never run into it
pub proof fn ensure_nl_terminates(b: Str)
    ensures ends_with(b + ensure_nl(b), seq!['\n']) || b.len() == 0
{
    if b.len() == 0 {
    } else {
        if b[b.len() - 1] == '\n' {
            assert(ensure_nl(b).len() == 0);
            assert(b.subrange(b.len() as int - 1, b.len() as int) == seq!['\n']);
        } else {
            assert(ensure_nl(b) == seq!['\n']);
            assert((b + seq!['\n']).len() == b.len() + 1);
            assert((b + seq!['\n']).subrange(b.len() as int, b.len() as int + 1) == seq!['\n']);
        }
    }
}

// --- 10. the producer flip -------------------------------------------

pub proof fn flip_len(rds: Seq<Rd>)
    ensures flip(rds).len() == rds.len()
    decreases rds.len()
{
    if rds.len() == 0 {
    } else {
        let rd = rds[0];
        if is_ps_read(rd) && !Set::empty().contains(ps_name(rd)) {
            let tail = rds.skip(1);
            let seen1 = Set::<Str>::empty().insert(ps_name(rd));
            assert(flip_from(rds, Set::empty())
                == seq![Rd { fd: Some(1), mode: Mode::W, tgt: rd.tgt, interp: rd.interp }]
                    + flip_from(tail, seen1));
            flip_from_len(rds.skip(1), seen1);
        } else {
            assert(flip_from(rds, Set::empty()) == seq![rd] + flip_from(rds.skip(1), Set::empty()));
            flip_from_len(rds.skip(1), Set::empty());
        }
        assert(flip(rds).len() == rds.len());
    }
}

proof fn flip_from_len(rds: Seq<Rd>, seen: Set<Str>)
    ensures flip_from(rds, seen).len() == rds.len()
    decreases rds.len()
{
    if rds.len() == 0 {
    } else {
        let rd = rds[0];
        if is_ps_read(rd) && !seen.contains(ps_name(rd)) {
            flip_from_len(rds.skip(1), seen.insert(ps_name(rd)));
        } else {
            flip_from_len(rds.skip(1), seen);
        }
    }
}

// elementwise: the flip keeps the heredoc/non-heredoc shape and can only
// REMOVE fd-0 touches (the flipped element is a `w` on fd 1 whose target
// is a Var, so it neither writes nor dups fd 0)
pub proof fn flip_frame(rds: Seq<Rd>, seen: Set<Str>, i: int)
    requires 0 <= i < rds.len()
    ensures is_here_rd(flip_from(rds, seen)[i]) == is_here_rd(rds[i]),
        touches0(flip_from(rds, seen)[i]) ==> touches0(rds[i])
    decreases rds.len()
{
    let rd = rds[0];
    flip_from_len(rds, seen);
    flip_from_len(rds.skip(1), seen);
    flip_from_len(rds.skip(1), seen.insert(ps_name(rd)));
    assert(flip_from(rds, seen).len() == rds.len());
    assert(flip_from(rds.skip(1), seen).len() == rds.skip(1).len());
    assert(flip_from(rds.skip(1), seen.insert(ps_name(rd))).len() == rds.skip(1).len());
    assert(rds.skip(1).len() == rds.len() as int - 1);
    if is_ps_read(rd) && !seen.contains(ps_name(rd)) {
        if i == 0 {
            let f = Rd { fd: Some(1), mode: Mode::W, tgt: rd.tgt, interp: rd.interp };
            assert(is_here_rd(f) == false && is_here_rd(rd) == false);
            assert(writes0(f) == false);
            assert(amp_form(f.tgt) == Amp::None);
            assert(reads0(f) == false);
            assert(touches0(f) == false);
        } else {
            assert(flip_from(rds, seen)
                == seq![Rd { fd: Some(1), mode: Mode::W, tgt: rd.tgt, interp: rd.interp }]
                    + flip_from(rds.skip(1), seen.insert(ps_name(rd))));
            assert(0 <= i - 1 && i - 1 < rds.skip(1).len());
            flip_frame(rds.skip(1), seen.insert(ps_name(rd)), i - 1);
            assert(flip_from(rds, seen)[i] == flip_from(rds.skip(1), seen.insert(ps_name(rd)))[i - 1]);
            assert(rds[i] == rds.skip(1)[i - 1]);
        }
    } else {
        if i == 0 {
            assert(flip_from(rds, seen)[0] == rd);
        } else {
            assert(flip_from(rds, seen) == seq![rd] + flip_from(rds.skip(1), seen));
            assert(0 <= i - 1 && i - 1 < rds.skip(1).len());
            flip_frame(rds.skip(1), seen, i - 1);
            assert(flip_from(rds, seen)[i] == flip_from(rds.skip(1), seen)[i - 1]);
            assert(rds[i] == rds.skip(1)[i - 1]);
        }
    }
}

pub proof fn first_here_ext(a: Seq<Rd>, b: Seq<Rd>)
    requires a.len() == b.len(),
        forall|i: int| 0 <= i < a.len() ==> is_here_rd(a[i]) == is_here_rd(b[i])
    ensures first_here(a) == first_here(b)
    decreases a.len()
{
    if a.len() == 0 {
    } else {
        assert(is_here_rd(a[0]) == is_here_rd(b[0]));
        if is_here_rd(a[0]) {
            assert(first_here(a) == 0 && first_here(b) == 0);
        } else {
            first_here_ext(a.skip(1), b.skip(1));
            let f = first_here(a.skip(1));
            if f == a.len() - 1 {
                assert(first_here(a) == a.len() && first_here(b) == b.len());
            } else {
                assert(first_here(a) == f + 1 && first_here(b) == f + 1);
            }
        }
    }
}

// safety of the source list implies safety of the flipped list
pub proof fn flip_safe_mono(rds: Seq<Rd>)
    ensures safe(rds) ==> safe(flip(rds))
{
    if safe(rds) {
        flip_len(rds);
        assert(flip(rds).len() == rds.len());
        assert forall|i: int| 0 <= i < rds.len() implies
            is_here_rd(flip(rds)[i]) == is_here_rd(rds[i])
        by {
            flip_frame(rds, Set::empty(), i);
        }
        first_here_ext(rds, flip(rds));
        first_here_nonneg(flip(rds));
        assert(first_here(flip(rds)) >= 0);
        assert forall|j: int| first_here(flip(rds)) <= j && j < flip(rds).len() implies
            (!is_here_rd(flip(rds)[j]) ==> !touches0(flip(rds)[j]))
        by {
            assert(0 <= j && j < rds.len());
            flip_frame(rds, Set::empty(), j);
            assert(is_here_rd(flip(rds)[j]) == is_here_rd(rds[j]));
            if !is_here_rd(flip(rds)[j]) {
                assert(!is_here_rd(rds[j]));
                assert(first_here(rds) == first_here(flip(rds)));
                assert(!touches0(rds[j]));
                assert(touches0(flip(rds)[j]) ==> touches0(rds[j]));
            }
        }
    }
}

} // verus!

fn main() {}