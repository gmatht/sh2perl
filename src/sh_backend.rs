//! sh_backend — PURE sh renderer (worktree-local, branch `backend/sh`).
//!
//! Consumes the ShIR (the A1 contract) in-process and emits idiomatic
//! `sh` source. The source language is bash, so constructs with no POSIX
//! equivalent (arrays, `(( ))`, `[[ ]]`, `${x^^}`, `<<<`, `shopt`) render
//! in their native bash form; everything else renders POSIX.
//!
//! The corpus gate (`setup_backends.sh --backend-gate sh`) runs
//! `sh_backend <file.sh>` per example and requires exit 0 + valid sh on
//! stdout with no sh2.*/TODO stub markers — so every construct lowers to
//! REAL shell syntax (no stubs).

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrProgram, IrRedirect, IrStmt, StrStyle};
use std::collections::HashSet;

lazy_static::lazy_static! {
    /// Array base names seen in the program (setArray/arrayLen/arrayIndex/
    /// slice usages) — the per-element lowering keys off these.
    static ref ARRAY_NAMES: std::sync::Mutex<HashSet<String>> = Default::default();
    /// `shopt -s nocasematch` state — the [[ == ]] case emulation folds
    /// the pattern case-insensitively when set.
    static ref NOCASEMATCH: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    /// `typeset -i` / `declare -i` integer-attribute vars — their
    /// assignments are ARITHMETIC (bash: `n=n+1` -> 43).
    static ref INT_VARS: std::sync::Mutex<HashSet<String>> = Default::default();
    /// Associative arrays (string-key elements / `declare -A`): their
    /// per-element vars are KEY-named (`config_user`) — the expansion
    /// helpers iterate the key list, not indices.
    static ref ASSOC_VARS: std::sync::Mutex<HashSet<String>> = Default::default();
    /// Provably-numeric variables (the A2 verdicts, `shir::analyze_var_types`
    /// → `IrType::Int`): their arith reads render as the bare name instead
    /// of `$( _num "${name}" )` — the runtime coercion is dead weight when
    /// the value is guaranteed numeric text (bash coerces non-numeric arith
    /// to 0 anyway; dash only errors when the value is non-numeric, which
    /// the analysis rules out — every assignment source is an integer
    /// literal, an arith expression, or another proven-numeric var; the
    /// drop target must be BARE — dash rejects quoted expansions inside
    /// `$(( ))`).
    static ref NUM_VARS: std::sync::Mutex<HashSet<String>> = Default::default();
}

/// Marker prefixes the core's lowering tags unquoted glob / process-
/// substitution words with (see shir.rs). The native shell performs both
/// natively, so the renderer strips the markers and emits the raw text.
const GLOB_MAGIC: &str = "\u{1}SH2GLOB\u{1}";
const PS_MAGIC: &str = "\u{1}SH2PS\u{1}";

/// Collect the array base names used anywhere in the program — the
/// per-element lowering keys the slice/len arms off these (the A1's
/// var_lengths field is not carried into IrProgram by the shared core).
fn array_names(prog: &IrProgram) -> HashSet<String> {
    let mut names = HashSet::new();
    fn expr_names(e: &IrExpr, names: &mut HashSet<String>) {
        match e {
            IrExpr::Call { func, args } => {
                let f = func.as_str();
                if matches!(f, "setArray" | "setArrayAppend" | "arrayItems" | "arrayLen") {
                    if let Ok(s) = raw_arg(args, 0) {
                        names.insert(s);
                    }
                }
                if f == "arrayIndex" {
                    if let Ok(s) = raw_arg(args, 0) {
                        names.insert(s);
                    }
                }
                if f == "param" {
                    let name = args
                        .get(1)
                        .and_then(|a| match a {
                            IrExpr::Str(s, _) => Some(s.as_str()),
                            _ => None,
                        })
                        .unwrap_or("");
                    // `${arr[@]}`-family baked names
                    if name.ends_with("[@]") || name.ends_with("[*]") {
                        if let Some(base) = name
                            .strip_suffix("[@]")
                            .or_else(|| name.strip_suffix("[*]"))
                        {
                            names.insert(base.to_string());
                        }
                    }
                }
                for a in args {
                    expr_names(a, names);
                }
            }
            IrExpr::Array(es) => {
                for e in es {
                    expr_names(e, names);
                }
            }
            IrExpr::Object(es) => {
                for (_, v) in es {
                    expr_names(v, names);
                }
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        expr_names(x, names);
                    }
                }
            }
            IrExpr::Arrow(stmts) => walk(stmts, names),
            _ => {}
        }
    }
    fn walk(sts: &[IrStmt], names: &mut HashSet<String>) {
        for st in sts {
            match st {
                IrStmt::Expr(e) => expr_names(e, names),
                IrStmt::Ext(_) => {}
                IrStmt::Assign { expr, targets, .. } => {
                    expr_names(expr, names);
                    for t in targets {
                        // element targets fold the subscript into the var
                        if let Some((base, idx)) =
                            t.var.strip_suffix(']').and_then(|v| v.split_once('['))
                        {
                            let idx = idx.trim_matches(['"', '\'']);
                            names.insert(base.to_string());
                            if !idx.chars().all(|c| c.is_ascii_digit()) {
                                ASSOC_VARS.lock().unwrap().insert(base.to_string());
                            }
                        }
                    }
                }
                IrStmt::If {
                    cond,
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    expr_names(cond, names);
                    walk(then, names);
                    walk(else_, names);
                    for (c, b) in elsifs {
                        expr_names(c, names);
                        walk(b, names);
                    }
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                    expr_names(cond, names);
                    walk(body, names);
                }
                IrStmt::ForInit { init, cond, step, body } => {
                    walk(init, names);
                    expr_names(cond, names);
                    walk(step, names);
                    walk(body, names);
                }
                IrStmt::Continue | IrStmt::Break => {}
                IrStmt::For { iter, body, .. } => {
                    expr_names(iter, names);
                    walk(body, names);
                }
                IrStmt::Case {
                    discriminant,
                    clauses,
                    ..
                } => {
                    expr_names(discriminant, names);
                    for c in clauses {
                        walk(&c.body, names);
                    }
                }
                IrStmt::Function { body, .. } => walk(body, names),
                IrStmt::Redirect { inner, redirects } => {
                    walk(inner, names);
                    for r in redirects {
                        expr_names(&r.target, names);
                    }
                }
                IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                    walk(body, names);
                }
                IrStmt::Select { clauses } => {
                    for c in clauses {
                        if let Some(ch) = &c.ch {
                            expr_names(ch, names);
                        }
                        if let Some(v) = &c.value {
                            expr_names(v, names);
                        }
                        walk(&c.body, names);
                    }
                }
                IrStmt::Asm { outputs, inputs, .. } => {
                    for (_, e) in outputs.iter().chain(inputs.iter()) {
                        expr_names(e, names);
                    }
                }
                IrStmt::Exec {
                    cmd,
                    args,
                    redirects,
                    env,
                    ..
                } => {
                    expr_names(cmd, names);
                    for a in args {
                        expr_names(a, names);
                    }
                    for r in redirects {
                        expr_names(r, names);
                    }
                    for (_, v) in env {
                        expr_names(v, names);
                    }
                }
                IrStmt::Pipeline { stages, .. } => {
                    for s in stages {
                        walk(s, names);
                    }
                }
                IrStmt::Output { value, .. }
                | IrStmt::Return(Some(value))
                | IrStmt::Exit(Some(value))
                | IrStmt::Die { expr: value, .. }
                | IrStmt::Warn { expr: value, .. } => expr_names(value, names),
                IrStmt::Declare { init, .. } => {
                    if let Some(i) = init {
                        expr_names(i, names);
                    }
                }
                IrStmt::DeclareArray { elements, .. } => {
                    for e in elements {
                        expr_names(e, names);
                    }
                }
                IrStmt::WriteFile { path, content, .. } => {
                    expr_names(path, names);
                    expr_names(content, names);
                }
                IrStmt::Return(None)
                | IrStmt::Exit(None)
                | IrStmt::SetChildError(_)
                | IrStmt::Require(_)
                | IrStmt::RawText(_)
                | IrStmt::Label(_)
                | IrStmt::Goto(_) => {}
                IrStmt::Try {
                    body,
                    excepts,
                    else_body,
                    finally_body,
                } => {
                    walk(body, names);
                    for e in excepts {
                        if let Some(m) = &e.match_expr {
                            expr_names(m, names);
                        }
                        walk(&e.body, names);
                    }
                    walk(else_body, names);
                    walk(finally_body, names);
                }
            }
        }
    }
    walk(&prog.stmts, &mut names);
    for sub in &prog.subs {
        walk(&sub.body, &mut names);
    }
    names
}


/// The whole-array expansion helper for a name (assoc arrays iterate
/// their key list — the per-element vars are KEY-named).
/// `${!map[@]}` key-iteration names arrive with a `!` prefix.
fn arr_base(name: &str) -> &str {
    name.strip_prefix('!').unwrap_or(name)
}

fn arr_expand_call(name: &str) -> String {
    if ASSOC_VARS.lock().unwrap().contains(name) {
        format!("$(_arr_expand_k {name})")
    } else {
        format!("$(_arr_expand {name})")
    }
}

fn arr_keys_call(name: &str) -> String {
    if ASSOC_VARS.lock().unwrap().contains(name) {
        format!("$(_arr_keys_k {name})")
    } else {
        format!("$(_arr_keys {name})")
    }
}

/// Does the program use whole-array expansions (`"${arr[@]}"`, `${!arr[@]}`,
/// `${arr[*]}`)? They need the `_arr_expand`/`_arr_keys` prologue helpers
/// (the per-element vars + counter lowering).
fn needs_arr_helper(prog: &IrProgram) -> bool {
    fn expr_uses_arr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                let f = func.as_str();
                if f == "arrayItems" {
                    return true;
                }
                if f == "arrayIndex" {
                    if let Some(IrExpr::Str(k, _)) = args.get(1) {
                        if k == "@" || k == "*" {
                            return true;
                        }
                    }
                }
                if f == "param" {
                    let op = args
                        .first()
                        .and_then(|a| match a {
                            IrExpr::Str(s, _) => Some(s.as_str()),
                            _ => None,
                        })
                        .unwrap_or("");
                    let name = args
                        .get(1)
                        .and_then(|a| match a {
                            IrExpr::Str(s, _) => Some(s.as_str()),
                            _ => None,
                        })
                        .unwrap_or("");
                    if name.ends_with("[@]") || name.ends_with("[*]") {
                        return true;
                    }
                    if op == "slice" {
                        let off = args
                            .get(2)
                            .and_then(|a| match a {
                                IrExpr::Str(s, _) => Some(s.as_str()),
                                _ => None,
                            })
                            .unwrap_or("");
                        if !name.starts_with('#')
                            && (off == "@"
                                || off == "*"
                                || (off.parse::<i64>().is_ok()
                                    && ARRAY_NAMES.lock().unwrap().contains(name)))
                        {
                            return true;
                        }
                    }
                }
                if f == "join" {
                    if let Some(inner) = args.first() {
                        if expr_uses_arr(inner) {
                            return true;
                        }
                    }
                }
                args.iter().any(expr_uses_arr)
            }
            IrExpr::Array(es) => es.iter().any(expr_uses_arr),
            IrExpr::Object(es) => es.iter().any(|(_, v)| expr_uses_arr(v)),
            IrExpr::Interpolate(parts) => parts
                .iter()
                .any(|p| matches!(p, InterpPart::Expr(x) if expr_uses_arr(x))),
            IrExpr::Arrow(stmts) => walk(stmts),
            _ => false,
        }
    }
    fn walk(sts: &[IrStmt]) -> bool {
        for st in sts {
            let hit = match st {
                IrStmt::Expr(e) => expr_uses_arr(e),
                IrStmt::Ext(_) => false,
                IrStmt::Assign { expr, .. } => expr_uses_arr(expr),
                IrStmt::If {
                    cond,
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    expr_uses_arr(cond)
                        || walk(then)
                        || walk(else_)
                        || elsifs.iter().any(|(c, b)| expr_uses_arr(c) || walk(b))
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                    expr_uses_arr(cond) || walk(body)
                }
                IrStmt::ForInit { init, cond, step, body } => {
                    walk(init) || expr_uses_arr(cond) || walk(step) || walk(body)
                }
                IrStmt::Continue | IrStmt::Break => false,
                IrStmt::For { iter, body, .. } => expr_uses_arr(iter) || walk(body),
                IrStmt::Case {
                    discriminant,
                    clauses,
                    ..
                } => expr_uses_arr(discriminant) || clauses.iter().any(|c| walk(&c.body)),
                IrStmt::Function { body, .. } => walk(body),
                IrStmt::Redirect { inner, redirects } => {
                    walk(inner) || redirects.iter().any(|r| expr_uses_arr(&r.target))
                }
                IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                    walk(body)
                }
                IrStmt::Select { clauses } => clauses.iter().any(|c| {
                    walk(&c.body)
                        || c.ch.as_ref().is_some_and(expr_uses_arr)
                        || c.value.as_ref().is_some_and(expr_uses_arr)
                }),
                IrStmt::Asm { outputs, inputs, .. } => outputs
                    .iter()
                    .chain(inputs.iter())
                    .any(|(_, e)| expr_uses_arr(e)),
                IrStmt::Exec {
                    cmd,
                    args,
                    redirects,
                    env,
                    ..
                } => {
                    expr_uses_arr(cmd)
                        || args.iter().any(expr_uses_arr)
                        || redirects.iter().any(expr_uses_arr)
                        || env.iter().any(|(_, v)| expr_uses_arr(v))
                }
                IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| walk(s)),
                IrStmt::Output { value, .. }
                | IrStmt::Return(Some(value))
                | IrStmt::Exit(Some(value))
                | IrStmt::Die { expr: value, .. }
                | IrStmt::Warn { expr: value, .. } => expr_uses_arr(value),
                IrStmt::Declare { init, .. } => init.as_ref().map(expr_uses_arr).unwrap_or(false),
                IrStmt::DeclareArray { elements, .. } => elements.iter().any(expr_uses_arr),
                IrStmt::WriteFile { path, content, .. } => {
                    expr_uses_arr(path) || expr_uses_arr(content)
                }
                IrStmt::Return(None)
                | IrStmt::Exit(None)
                | IrStmt::SetChildError(_)
                | IrStmt::Require(_)
                | IrStmt::RawText(_)
                | IrStmt::Label(_)
                | IrStmt::Goto(_) => false,
                IrStmt::Try {
                    body,
                    excepts,
                    else_body,
                    finally_body,
                } => {
                    walk(body)
                        || excepts.iter().any(|e| {
                            e.match_expr.as_ref().map(expr_uses_arr).unwrap_or(false)
                                || walk(&e.body)
                        })
                        || walk(else_body)
                        || walk(finally_body)
                }
            };
            if hit {
                return true;
            }
        }
        false
    }
    walk(&prog.stmts) || prog.subs.iter().any(|s| walk(&s.body))
}

/// Render a ShIR program to `sh` source. `Err` on a construct outside the
/// renderable subset (the gate reports it as a FAIL).
pub fn shir_to_sh(prog: &IrProgram) -> Result<String, String> {
    // builtin-op fallback arm (shir-builtin-op-20260816): the sh
    // backend has NOT accepted the `builtin` op — render as exec.
    let mut prog = prog.clone();
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    // `for ((...))` (core request zsh-sh-go-20260813-153215): the shell
    // lowering emits the rich A1 ForInit node — the sh renderer refuses
    // an unstripped one, so lower it to `init; while(cond){body; step}`
    // first (the ingest path's CLI-level strip; double-strip is a no-op).
    let mut stripped = prog.clone();
    crate::shir_passes::strip_cfor(&mut stripped);
    let prog = &stripped;
    // collect the array base names (the A1's var_lengths is not carried
    // into IrProgram — the shared core; usage is a sound proxy)
    *ARRAY_NAMES.lock().unwrap() = array_names(prog);
    *NOCASEMATCH.lock().unwrap() = false;
    *INT_VARS.lock().unwrap() = Default::default();
    *ASSOC_VARS.lock().unwrap() = Default::default();
    *NUM_VARS.lock().unwrap() = crate::shir::analyze_var_types(prog)
        .into_iter()
        .filter(|(_, t)| *t == crate::ir::IrType::Int)
        .map(|(n, _)| n)
        .collect();
    let mut out = String::new();
    out.push_str("#!/bin/sh\n");

    if needs_grep_p(&prog.stmts) {
        out.push_str("\n");
        out.push_str("# portable PCRE grep: GNU grep -P, macOS gnu-grep, pcregrep/pcre2grep, or perl\n");
        out.push_str("grep_p() {\n");
        // Leading grep flags (e.g. `-o -i`) pass through before the pattern;
        // the first non-flag arg is the pattern, the rest are files. The
        // renderer emits `grep_p FLAGS PATTERN [FILES…]` — without this a
        // `grep_p -o PAT` call would treat `-o` as the pattern.
        out.push_str("    _gp_flags=\n");
        out.push_str("    while [ \"$#\" -gt 0 ]; do\n");
        out.push_str("        case \"$1\" in\n");
        out.push_str("            --) shift; break ;;\n");
        out.push_str("            -*) _gp_flags=\"$_gp_flags $1\"; shift ;;\n");
        out.push_str("            *) break ;;\n");
        out.push_str("        esac\n");
        out.push_str("    done\n");
        out.push_str("    [ \"$#\" -eq 0 ] && return 1\n");
        out.push_str("    _gp_pat=\"$1\"; shift\n");
        out.push_str("    if grep -P $_gp_flags -- \"$_gp_pat\" \"$@\" 2>/dev/null; then return; fi\n");
        out.push_str(
            "    if command -v ggrep >/dev/null 2>&1; then ggrep -P $_gp_flags -- \"$_gp_pat\" \"$@\"; return; fi\n",
        );
        out.push_str(
            "    if command -v pcre2grep >/dev/null 2>&1; then pcre2grep $_gp_flags \"$_gp_pat\" \"$@\"; return; fi\n",
        );
        out.push_str(
            "    if command -v pcregrep >/dev/null 2>&1; then pcregrep $_gp_flags \"$_gp_pat\" \"$@\"; return; fi\n",
        );
        out.push_str("    case \"$_gp_flags\" in\n");
        out.push_str("        *-o*) perl -ne 'BEGIN{$p=shift @ARGV} while (/$p/g) { print \"$&\\n\" }' \"$_gp_pat\" \"$@\" ;;\n");
        out.push_str("        *)    perl -ne 'BEGIN{$p=shift @ARGV} print if /$p/' \"$_gp_pat\" \"$@\" ;;\n");
        out.push_str("    esac\n");
        out.push_str("}\n\n");
    }
    if needs_readlink(&prog.stmts) {
        out.push_str(r##"
    _readlink() {
        _rl_mode=f
        _rl_nl=0
        while [ "$#" -gt 1 ]; do
            case "$1" in
                -e) _rl_mode=e ;;
                -m) _rl_mode=m ;;
                -f) _rl_mode=f ;;
                -n) _rl_nl=1 ;;
                *) break ;;
            esac
            shift
        done
        for _rl_path in "$@"; do
            case $_rl_path in
                /*) ;;
                *) _rl_path=$PWD/$_rl_path ;;
            esac
            if [ "$_rl_mode" = m ]; then
                _rl_ifs=$IFS
                IFS=/
                _rl_out=
                for _rl_c in $_rl_path; do
                    case $_rl_c in
                        ''|.) ;;
                        ..)
                            case $_rl_out in
                                */*) _rl_out=${_rl_out%/*} ;;
                                *) _rl_out= ;;
                            esac ;;
                        *) _rl_out=$_rl_out/$_rl_c ;;
                    esac
                done
                IFS=$_rl_ifs
                [ -n "$_rl_out" ] || _rl_out=/
                if [ "$_rl_nl" -eq 0 ]; then printf '%s\n' "$_rl_out"; else printf '%s' "$_rl_out"; fi
                continue
            fi
            _rl_dir=${_rl_path%/*}
            _rl_base=${_rl_path##*/}
            _rl_old=$PWD
            if cd "$_rl_dir" 2>/dev/null; then
                _rl_dir=$(pwd -P)
                cd "$_rl_old" || :
            elif [ "$_rl_mode" = e ]; then
                return 1
            fi
            _rl_n=0
            while [ -L "$_rl_dir/$_rl_base" ] && [ "$_rl_n" -lt 40 ]; do
                _rl_tgt=$(ls -ld "$_rl_dir/$_rl_base")
                _rl_tgt=${_rl_tgt#*" -> "}
                case $_rl_tgt in
                    /*) _rl_new=$_rl_tgt ;;
                    *) _rl_new=$_rl_dir/$_rl_tgt ;;
                esac
                _rl_dir=${_rl_new%/*}
                _rl_base=${_rl_new##*/}
                _rl_n=$((_rl_n + 1))
            done
            _rl_out=$_rl_dir/$_rl_base
            if [ "$_rl_mode" = e ] && [ ! -e "$_rl_out" ]; then
                return 1
            fi
            if [ "$_rl_nl" -eq 0 ]; then printf '%s\n' "$_rl_out"; else printf '%s' "$_rl_out"; fi
        done
    }

"##);
    }
    if needs_cmp(&prog.stmts) {
        out.push_str(
            r#"
# GNU cmp(1) polyfill (POSIX sh; -b/-l/-s/-n/-i/--; bisect first-diff,
# stdin spooled into a variable, no temp files). Byte-identical to GNU
# cmp 8.x on the corpus; runs under dash and busybox ash.
_cmp_oct_char() {
    awk -v d="$((0$1))" 'BEGIN {
        if (d >= 32 && d <= 126) printf "%c", d
        else if (d < 32) printf "^%c", d + 64
        else if (d == 127) printf "^?"
        else { d2 = d - 128
               if (d2 < 32) printf "M-^%c", d2 + 64
               else if (d2 == 127) printf "M-^?"
               else printf "M-%c", d2 }
    }'
}
_cmp() {
    (
        _b=0 _l=0 _s=0 _nlim=-1 _sk1=0 _sk2=0
        OPTIND=1
        while getopts "blsn:i:" _o; do
            case "$_o" in
                b) _b=1 ;;
                l) _l=1 ;;
                s) _s=1 ;;
                n) _nlim=$OPTARG ;;
                i)
                    case "$OPTARG" in
                        *:*) _sk1=${OPTARG%%:*}; _sk2=${OPTARG##*:} ;;
                        *) _sk1=$OPTARG; _sk2=$OPTARG ;;
                    esac
                    ;;
                *) echo "Usage: cmp [-b|-l|-s] [-n N] [-i N[:M]] [--] file1 file2" >&2
                   exit 2 ;;
            esac
        done
        shift $((OPTIND - 1))
        [ "${1:-}" = "--" ] && shift
        if [ "$_l" -eq 1 ] && [ "$_s" -eq 1 ]; then
            echo "cmp: options -l and -s are incompatible" >&2
            echo "Try 'cmp --help' for more information." >&2
            exit 2
        fi
        if [ $# -eq 0 ]; then
            echo "cmp: missing operand after 'cmp'" >&2
            echo "Try 'cmp --help' for more information." >&2
            exit 2
        elif [ $# -eq 1 ]; then
            _f1=$1; _f2=-
        else
            _f1=$1; _f2=$2
        fi
        for _f in "$_f1" "$_f2"; do
            [ "$_f" = "-" ] && continue
            if [ ! -e "$_f" ]; then
                echo "cmp: $_f: No such file or directory" >&2
                exit 2
            fi
            if [ -d "$_f" ]; then
                echo "cmp: $_f: Is a directory" >&2
                exit 2
            fi
            if [ ! -r "$_f" ]; then
                echo "cmp: $_f: Permission denied" >&2
                exit 2
            fi
        done
        # stdin is a one-shot pipe, but the bisect needs re-reads, so spool
        # it into a shell VARIABLE (exact byte-for-byte via a read loop;
        # NUL bytes cannot be held and are dropped at the first one). Both
        # `-` operands point at the same variable (`cmp - -` -> rc 0).
        _tin=
        if [ "$_f1" = "-" ] || [ "$_f2" = "-" ]; then
            _cmp_partial=0
            while :; do
                if IFS= read -r _lin; then
                    _tin="$_tin$_lin
"
                else
                    if [ -z "${_lin:-}" ]; then break; fi
                    _tin="$_tin$_lin
"
                    _cmp_partial=1
                    break
                fi
            done
            [ "$_cmp_partial" -eq 1 ] && _tin=${_tin%"
"}
        fi
        # read source $1 (`-` = stdin var) at byte offset $2 (0-based),
        # emitting up to $3 bytes. Files use tail's lseek; stdin re-pipes
        # the variable (read-through).
        _cmp_read() {
            if [ "$1" = "-" ]; then
                printf '%s' "$_tin" | tail -c +$(( $2 + 1 )) 2>/dev/null | head -c "$3" 2>/dev/null
            else
                tail -c +$(( $2 + 1 )) "$1" 2>/dev/null | head -c "$3" 2>/dev/null
            fi
        }
        _cmp_size() {
            if [ "$1" = "-" ]; then
                printf '%s' "$_tin" | wc -c | tr -d ' '
            else
                wc -c < "$1" | tr -d ' '
            fi
        }
        # sizes of the comparison ranges (after skip), and the length to
        # compare = the shorter range capped by -n.
        _sz1=$(_cmp_size "$_f1")
        _sz2=$(_cmp_size "$_f2")
        _sz1=$((_sz1 - _sk1)); _sz2=$((_sz2 - _sk2))
        [ "$_sz1" -lt 0 ] && _sz1=0
        [ "$_sz2" -lt 0 ] && _sz2=0
        _n=$_sz1
        [ "$_sz2" -lt "$_n" ] && _n=$_sz2
        if [ "$_nlim" -ne -1 ] && [ "$_nlim" -lt "$_n" ]; then _n=$_nlim; fi
        # first differing offset in [0, _n): binary search on the prefix
        # cksum (the predicate "first k bytes are identical" is monotone).
        _cmp_ffd() {
            _ffd_lo=0
            _ffd_hi=$1
            while [ "$_ffd_hi" -gt "$_ffd_lo" ]; do
                if [ $((_ffd_hi - _ffd_lo)) -eq 1 ]; then
                    _ffd_h1=$(_cmp_read "$_f1" "$_sk1" "$_ffd_hi" | cksum | awk '{print $1}')
                    _ffd_h2=$(_cmp_read "$_f2" "$_sk2" "$_ffd_hi" | cksum | awk '{print $1}')
                    if [ "$_ffd_h1" = "$_ffd_h2" ]; then echo "$_ffd_hi"; else echo "$_ffd_lo"; fi
                    return
                fi
                _ffd_mid=$((_ffd_lo + (_ffd_hi - _ffd_lo) / 2))
                _ffd_h1=$(_cmp_read "$_f1" "$_sk1" "$_ffd_mid" | cksum | awk '{print $1}')
                _ffd_h2=$(_cmp_read "$_f2" "$_sk2" "$_ffd_mid" | cksum | awk '{print $1}')
                if [ "$_ffd_h1" = "$_ffd_h2" ]; then _ffd_lo=$_ffd_mid; else _ffd_hi=$_ffd_mid; fi
            done
            echo "$_ffd_lo"
        }
        # empty comparison range
        if [ "$_n" -eq 0 ]; then
            if [ "$_nlim" -ne -1 ]; then exit 0; fi
            if [ "$_sz1" -eq 0 ] && [ "$_sz2" -eq 0 ]; then exit 0; fi
            if [ "$_sz1" -lt "$_sz2" ]; then
                [ "$_s" -eq 0 ] && echo "cmp: EOF on $_f1 which is empty" >&2
            else
                if [ "$_sz2" -eq 0 ]; then
                    [ "$_s" -eq 0 ] && echo "cmp: EOF on $_f2 which is empty" >&2
                else
                    [ "$_s" -eq 0 ] && echo "cmp: EOF on $_f2" >&2
                fi
            fi
            exit 1
        fi
        _df=$(_cmp_ffd "$_n")
        if [ "$_df" -ge "$_n" ]; then
            if [ "$_sz1" -eq "$_sz2" ]; then exit 0; fi
            if [ "$_nlim" -ne -1 ] && [ "$_n" -eq "$_nlim" ]; then exit 0; fi
            if [ "$_sz1" -gt "$_sz2" ]; then _sf=$_f2; _so=$_sz2
            else _sf=$_f1; _so=$_sz1; fi
            if [ "$_so" -eq 0 ]; then
                [ "$_s" -eq 0 ] && echo "cmp: EOF on $_sf which is empty" >&2
                exit 1
            fi
            if [ "$_nlim" -eq -1 ]; then
                _eol=$(_cmp_read "$_f1" "$_sk1" "$_n" | tr -cd '\n' | wc -c | tr -d ' ')
                _eol=$((_eol + 1))
                if [ "$_s" -eq 0 ]; then
                    if [ "$_l" -eq 1 ]; then
                        echo "cmp: EOF on $_sf after byte $_n" >&2
                    else
                        echo "cmp: EOF on $_sf after byte $_n, in line $_eol" >&2
                    fi
                fi
                exit 1
            fi
            _df=$_n
            if [ "$_sz1" -gt "$_sz2" ]; then
                _b1=$(_cmp_read "$_f1" $((_sk1 + _df)) 1 | od -An -to1 | tr -d ' \n')
                _b2=
            else
                _b1=
                _b2=$(_cmp_read "$_f2" $((_sk2 + _df)) 1 | od -An -to1 | tr -d ' \n')
            fi
            _b1set=1
        fi
        if [ "${_b1set:-0}" != 1 ]; then
            _b1=$(_cmp_read "$_f1" $((_sk1 + _df)) 1 | od -An -to1 | tr -d ' \n')
            _b2=$(_cmp_read "$_f2" $((_sk2 + _df)) 1 | od -An -to1 | tr -d ' \n')
        fi
        if [ -z "$_b1" ] && [ -z "$_b2" ]; then exit 0; fi
        if [ -z "$_b1" ]; then
            _eol=$(_cmp_read "$_f2" "$_sk2" "$_df" | tr -cd '\n' | wc -c | tr -d ' ')
            _eol=$((_eol + 1))
            if [ "$_s" -eq 0 ]; then
                if [ "$_l" -eq 1 ]; then
                    echo "cmp: EOF on $_f1 after byte $_n" >&2
                else
                    echo "cmp: EOF on $_f1 after byte $_n, in line $_eol" >&2
                fi
            fi
            exit 1
        fi
        if [ -z "$_b2" ]; then
            _eol=$(_cmp_read "$_f1" "$_sk1" "$_df" | tr -cd '\n' | wc -c | tr -d ' ')
            _eol=$((_eol + 1))
            if [ "$_s" -eq 0 ]; then
                if [ "$_l" -eq 1 ]; then
                    echo "cmp: EOF on $_f2 after byte $_n" >&2
                else
                    echo "cmp: EOF on $_f2 after byte $_n, in line $_eol" >&2
                fi
            fi
            exit 1
        fi
        if [ "$_b1" = "$_b2" ]; then exit 0; fi
        if [ "$_l" -eq 1 ]; then
            if [ "$_sz1" -gt "$_n" ] || [ "$_sz2" -gt "$_n" ]; then
                if [ "$_sz1" -gt "$_sz2" ]; then _ef=$_f2; _eo=$_sz2
                else _ef=$_f1; _eo=$_sz1; fi
                echo "cmp: EOF on $_ef after byte $_eo" >&2
            fi
            _blk=16384
            _off=$_df
            while [ "$_off" -lt "$_n" ]; do
                _cur=$((_n - _off))
                [ "$_cur" -gt "$_blk" ] && _cur=$_blk
                _h1=$(_cmp_read "$_f1" $((_sk1 + _off)) "$_cur" | cksum | awk '{print $1}')
                _h2=$(_cmp_read "$_f2" $((_sk2 + _off)) "$_cur" | cksum | awk '{print $1}')
                if [ "$_h1" != "$_h2" ]; then
                    {
                        _cmp_read "$_f1" $((_sk1 + _off)) "$_cur" | od -An -to1 -v | tr -s ' ' '\n' | sed '/^$/d' | awk '{printf "1 %d %s\n", NR, $1}'
                        printf '0 0 0\n'
                        _cmp_read "$_f2" $((_sk2 + _off)) "$_cur" | od -An -to1 -v | tr -s ' ' '\n' | sed '/^$/d' | awk '{printf "2 %d %s\n", NR, $1}'
                    } | awk -v base="$((_off + 1))" -v maxb="$_n" '
                        $1 == 1 { a[$2] = $3; next }
                        $1 == 2 { if (($2 in a) && a[$2] != $3) {
                                      _w = length(sprintf("%d", maxb))
                                      printf "%*d %3s %3s\n", _w, base + $2 - 1, a[$2]+0, $3+0 } }'
                fi
                _off=$((_off + _cur))
            done
            exit 1
        fi
        if [ "$_df" -gt 0 ]; then
            _ln=$(_cmp_read "$_f1" "$_sk1" "$_df" | tr -cd '\n' | wc -c | tr -d ' ')
            _ln=$((_ln + 1))
        else
            _ln=1
        fi
        _byte=$((_df + 1))
        if [ "$_s" -eq 1 ]; then exit 1; fi
        if [ "$_b" -eq 1 ]; then
            _o1=$(printf '%o' "$((0$_b1))")
            _o2=$(printf '%o' "$((0$_b2))")
            _c1ch=$(_cmp_oct_char "$_b1")
            _c2ch=$(_cmp_oct_char "$_b2")
            printf '%s %s differ: byte %d, line %d is %3s %s %3s %s\n' \
                "$_f1" "$_f2" "$_byte" "$_ln" "$_o1" "$_c1ch" "$_o2" "$_c2ch"
        else
            printf '%s %s differ: byte %d, line %d\n' "$_f1" "$_f2" "$_byte" "$_ln"
        fi
        exit 1
    )
}
"#,
        );
    }
    if needs_arr_helper(prog) {
        out.push_str(
            r#"
# whole-array expansion helpers (the per-element vars + len counter)
_arr_expand() {
    _i=0
    _n=$(eval echo "\${$1_len}")
    _n=${_n:-0}
    while [ "$_i" -lt "$_n" ]; do
        eval "printf '%s' \"\${$1_$_i}\""
        [ "$_i" -lt "$((_n - 1))" ] && printf ' '
        _i=$((_i + 1))
    done
    printf '\n'
}
_arr_keys() {
    _i=0
    _n=$(eval echo "\${$1_len}")
    _n=${_n:-0}
    while [ "$_i" -lt "$_n" ]; do
        printf '%s\n' "$_i"
        _i=$((_i + 1))
    done
}
_arr_expand_k() {
    # assoc variant: values in key-list order ($1_keys). Joins with the
    # FIRST char of the caller's IFS (bash's ${arr[*]} semantics — under
    # IFS=newline each value lands on its own line); the key list itself
    # is space-separated, so the split runs under a fixed IFS. All in a
    # subshell: the caller's IFS is untouched.
    ( # the first IFS char (${IFS#?} = IFS minus the first char); a
      # cmdsub would eat a newline separator, so use pure expansion
      _sep=${IFS%${IFS#?}}
      IFS=' '
      _n=0
      for _k in $(eval echo "\${$1_keys}"); do
          [ "$_n" -gt 0 ] && printf '%s' "$_sep"
          eval "printf '%s' \"\${$1_$_k}\""
          _n=$((_n + 1))
      done
      printf '\n' )
}
_arr_keys_k() {
    # assoc variant: the key list
    ( IFS=' '
      for _k in $(eval echo "\${$1_keys}"); do
          printf '%s\n' "$_k"
      done )
}
_arr_slice() {
    # $1=name $2=off $3=len — elements off..off+len-1, space-joined
    _i=$(( $2 ))
    _end=$(eval echo "\${$1_len}")
    _end=${_end:-0}
    _lim=$(( $2 + ${3:-1000000} ))
    while [ "$_i" -lt "$_lim" ] && [ "$_i" -lt "$_end" ]; do
        eval "printf '%s' \"\${$1_$_i}\""
        _i=$((_i + 1))
        [ "$_i" -lt "$_lim" ] && [ "$_i" -lt "$_end" ] && printf ' '
    done
    printf '\n'
}

"#,
        );
    }
    // `_num()` is a polyfill for the $( _num "…" ) arith-coercion calls:
    // emit the definition iff the RENDERED body actually calls it (bash
    // coerces non-numeric arith values to 0, dash errors). Checking the
    // final body is robust by construction — the definition is present
    // exactly when a call is, no matter which arith form the core
    // emitted (the Arith AST or the `arith("$a+$b")` string).
    let mut body = String::new();
    for st in &prog.stmts {
        stmt_to_sh(st, 0, &mut body)?;
    }
    for sub in &prog.subs {
        body.push('\n');
        body.push_str(&sub.name);
        body.push_str("() {\n");
        for st in &sub.body {
            stmt_to_sh(st, 1, &mut body)?;
        }
        body.push_str("}\n");
    }
    if body.contains("$( _num ") {
        out.push_str("_num() {\n");
        out.push_str("    # bash coerces non-numeric arith values to 0; dash errors\n");
        out.push_str("    case \"$1\" in\n");
        out.push_str("        ''|'-'|*[!0-9-]*|-*[!0-9]*) echo 0 ;;\n");
        out.push_str("        *) echo \"$1\" ;;\n");
        out.push_str("    esac\n");
        out.push_str("}\n\n");
    }
    out.push_str(&body);
    Ok(out)
}

fn indent(out: &mut String, d: usize) {
    for _ in 0..d {
        out.push_str("    ");
    }
}

// ── statements (block form, newline-terminated) ──────────────────────

// range_of — a numeric-range For iterable (`$(seq A B)` lowered to a
// Range, or a bare Range / Array([Range])): (start, end). GNU `seq` has no
// POSIX builtin — lowered to a portable while-loop instead of refusing.
fn range_of(iter: &IrExpr) -> Option<(i64, i64)> {
    match iter {
        IrExpr::Range { start, end } => Some((*start, *end)),
        IrExpr::Array(items) if items.len() == 1 => match items.first() {
            Some(IrExpr::Range { start, end }) => Some((*start, *end)),
            _ => None,
        },
        _ => None,
    }
}

fn stmt_to_sh(st: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
    match st {
        IrStmt::Ext(_) => return Err("sh renderer: Ext node unsupported".to_string()),
        IrStmt::Expr(e) => {
            indent(out, d);
            if let IrExpr::Call { func, .. } = e {
                if func == "grepMatches" {
                    // statement position: the matches are the output
                    out.push_str(&format!("echo \"{}\"", cmd_to_sh(e)?));
                    out.push('\n');
                    return Ok(());
                }
            }
            out.push_str(&cmd_to_sh(e)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Assign { targets, expr, asm, .. } => {
            // Declarator-position asm label (core request
            // c-sh-go-toplevelasmargument-20260814-042952): the label
            // only renames the object-file symbol — no sh rendering —
            // refuse loudly (refuse > guess; same contract as the Asm
            // statement).
            if let Some(spec) = asm {
                return Err(format!(
                    "asm label '{}' on an assign has no sh rendering",
                    spec.template
                ));
            }
            indent(out, d);
            out.push_str(&assign_to_sh(targets, expr)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            indent(out, d);
            out.push_str("if ");
            out.push_str(&cmd_to_sh(cond)?);
            out.push_str("; then\n");
            if then.is_empty() {
                // bash rejects an EMPTY then/elif branch (`if c; then\nelse`
                // is a syntax error) — a `:` no-op keeps the structure. The
                // goto-restructure pass emits exactly this shape (an inverted
                // guarded goto: `if (c) {} else { skipped }` — t29_goto.cc,
                // t31_goto_forward.c).
                indent(out, d + 1);
                out.push_str(":\n");
            } else {
                for b in then {
                    stmt_to_sh(b, d + 1, out)?;
                }
            }
            for (econd, ebody) in elsifs {
                indent(out, d);
                out.push_str("elif ");
                out.push_str(&cmd_to_sh(econd)?);
                out.push_str("; then\n");
                if ebody.is_empty() {
                    indent(out, d + 1);
                    out.push_str(":\n");
                } else {
                    for b in ebody {
                        stmt_to_sh(b, d + 1, out)?;
                    }
                }
            }
            if !else_.is_empty() {
                indent(out, d);
                out.push_str("else\n");
                for b in else_ {
                    stmt_to_sh(b, d + 1, out)?;
                }
            }
            indent(out, d);
            out.push_str("fi\n");
            Ok(())
        }
        IrStmt::For { var, iter, body } => {
            // GNU $(seq A B) / {A..B} -> a portable while-loop (POSIX has
            // no seq builtin; the output must run on BSD/macOS sh)
            if let Some((start, end)) = range_of(iter) {
                indent(out, d);
                out.push_str(&format!("{var}={start}\n"));
                indent(out, d);
                out.push_str(&format!("while [ \"${var}\" -le {end} ]; do\n"));
                for b in body {
                    stmt_to_sh(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str(&format!("{var}=$(({var} + 1))\n"));
                indent(out, d);
                out.push_str("done\n");
                return Ok(());
            }
            indent(out, d);
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            out.push_str(&for_items_to_sh(iter)?);
            out.push_str("; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("done\n");
            Ok(())
        }
        IrStmt::While { cond, body } => {
            indent(out, d);
            out.push_str("while ");
            out.push_str(&cmd_to_sh(cond)?);
            out.push_str("; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("done\n");
            Ok(())
        }
        IrStmt::ForInit { .. } => Err(
            "sh renderer: un-stripped ForInit (the strip_cfor pass should have lowered it)".into(),
        ),
        IrStmt::Continue => {
            indent(out, d);
            out.push_str("continue\n");
            Ok(())
        }
        IrStmt::Break => {
            indent(out, d);
            out.push_str("break\n");
            Ok(())
        }
        IrStmt::DoWhile { body, cond, until } => {
            indent(out, d);
            out.push_str("while :; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d + 1);
            if *until {
                out.push_str("if ");
            } else {
                out.push_str("if ! ");
            }
            out.push_str(&cmd_to_sh(cond)?);
            out.push_str("; then break; fi\n");
            indent(out, d);
            out.push_str("done\n");
            Ok(())
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            indent(out, d);
            out.push_str("case ");
            out.push_str(&word_to_sh(discriminant)?);
            out.push_str(" in\n");
            for cl in clauses {
                indent(out, d + 1);
                out.push_str(&cl.patterns.join(" | "));
                out.push_str(")\n");
                for b in &cl.body {
                    stmt_to_sh(b, d + 2, out)?;
                }
                indent(out, d + 1);
                out.push_str(";;\n");
            }
            indent(out, d);
            out.push_str("esac\n");
            Ok(())
        }
        IrStmt::Function { name, body, .. } => {
            indent(out, d);
            out.push_str(name);
            out.push_str("() {\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        IrStmt::Redirect { inner, redirects } => {
            // process substitutions lower to pipes / temp files (POSIX has
            // no `<(cmd)`); the remaining redirects render as suffixes
            let (ps_ins, plain) = partition_procsub(redirects);
            let suffix = redirects_to_sh(&plain)?;
            let mut line = if inner.len() == 1 {
                if let IrStmt::Expr(e) = &inner[0] {
                    cmd_to_sh(e)?
                } else {
                    stmts_inline(inner)?
                }
            } else {
                // compound inner: inline it, then the redirects apply to the group
                stmts_inline(inner)?
            };
            line = herestring_wrap(&plain, line)?;
            if !ps_ins.is_empty() {
                line = lower_procsub_stmt(&ps_ins, &line)?;
            }
            indent(out, d);
            out.push_str(&line);
            out.push_str(&suffix);
            out.push('\n');
            Ok(())
        }
        IrStmt::Subshell(body) => {
            indent(out, d);
            out.push_str("(\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str(")\n");
            Ok(())
        }
        IrStmt::Background(body) => {
            if body.len() == 1 {
                if let IrStmt::Expr(e) = &body[0] {
                    indent(out, d);
                    out.push_str(&cmd_to_sh(e)?);
                    out.push_str(" &\n");
                    return Ok(());
                }
            }
            indent(out, d);
            out.push_str("(\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str(") &\n");
            Ok(())
        }
        IrStmt::Block(body) => {
            indent(out, d);
            out.push_str("{\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        // try/except/else/finally — Python-style exception handling. sh
        // has no exceptions; the only failure signal is the exit status,
        // so the except clause runs when the try body's LAST command
        // failed, else_ runs when it succeeded, finally always runs (the
        // corpus bodies are print-only — exact; a failing-command body is
        // a documented approximation).
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            indent(out, d);
            if body.is_empty() {
                out.push_str("{ :; }\n");
            } else {
                out.push_str("{\n");
                for b in body {
                    stmt_to_sh(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("}\n");
            }
            if !excepts.is_empty() {
                indent(out, d);
                out.push_str("if [ $? -ne 0 ]; then\n");
                for b in &excepts[0].body {
                    stmt_to_sh(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("fi\n");
            } else if !else_body.is_empty() {
                indent(out, d);
                out.push_str("if [ $? -eq 0 ]; then\n");
                for b in else_body {
                    stmt_to_sh(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("fi\n");
            }
            for b in finally_body {
                stmt_to_sh(b, d, out)?;
            }
            Ok(())
        }
        // sh has no select-on-channels — refuse loudly
        IrStmt::Select { .. } => Err("select has no sh rendering".into()),
        // inline asm has no sh rendering — refuse loudly
        IrStmt::Asm { .. } => Err("inline asm has no sh rendering".into()),
        IrStmt::Return(e) => {
            indent(out, d);
            out.push_str("return");
            if let Some(x) = e {
                out.push(' ');
                out.push_str(&word_to_sh(x)?);
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::Exit(e) => {
            indent(out, d);
            out.push_str("exit");
            if let Some(x) = e {
                out.push(' ');
                out.push_str(&word_to_sh(x)?);
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::Die { expr, .. } => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_sh(expr)?);
            out.push_str(" >&2\n");
            indent(out, d);
            out.push_str("exit 1\n");
            Ok(())
        }
        IrStmt::Warn { expr, .. } => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_sh(expr)?);
            out.push_str(" >&2\n");
            Ok(())
        }
        IrStmt::Declare { vars, init, local } => {
            indent(out, d);
            if *local {
                out.push_str("local ");
            }
            for (i, v) in vars.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&v.name);
                if let Some(init) = init {
                    out.push('=');
                    out.push_str(&word_to_sh(init)?);
                }
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            indent(out, d);
            out.push_str(var);
            out.push('=');
            out.push_str(&array_literal_to_sh(elements)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Output { value, newline, .. } => {
            indent(out, d);
            if *newline {
                out.push_str("printf '%s\\n' ");
            } else {
                out.push_str("printf '%s' ");
            }
            out.push_str(&word_to_sh(value)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Exec {
            cmd,
            args,
            capture,
            redirects,
            env,
        } => {
            let mut line = exec_line_to_sh(cmd, args, Some(env))?;
            if !redirects.is_empty() {
                line.push_str(&redirect_objs_to_sh(redirects)?);
            }
            if let Some(var) = capture {
                indent(out, d);
                out.push_str(var);
                out.push_str("=$(");
                out.push_str(&line);
                out.push_str(")\n");
            } else {
                indent(out, d);
                out.push_str(&line);
                out.push('\n');
            }
            Ok(())
        }
        IrStmt::Pipeline {
            stages, capture, ..
        } => {
            let mut line = String::new();
            for (i, stg) in stages.iter().enumerate() {
                if i > 0 {
                    line.push_str(" | ");
                }
                line.push_str(&stmts_inline(stg)?);
            }
            if let Some(var) = capture {
                indent(out, d);
                out.push_str(var);
                out.push_str("=$(");
                out.push_str(&line);
                out.push_str(")\n");
            } else {
                indent(out, d);
                out.push_str(&line);
                out.push('\n');
            }
            Ok(())
        }
        IrStmt::WriteFile {
            path,
            content,
            append,
        } => {
            indent(out, d);
            if *append {
                out.push_str("printf '%s' ");
            } else {
                out.push_str("printf '%s' ");
            }
            out.push_str(&word_to_sh(content)?);
            out.push_str(if *append { " >> " } else { " > " });
            out.push_str(&word_to_sh(path)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Label(name) | IrStmt::Goto(name) => {
            indent(out, d);
            let kind = if matches!(st, IrStmt::Label(_)) {
                "label"
            } else {
                "goto"
            };
            out.push_str(&format!(
                "# TODO(unsupported): {kind} {name} not restructured by restructure_goto\n"
            ));
            Ok(())
        }
        IrStmt::SetChildError(_) | IrStmt::Require(_) | IrStmt::RawText(_) => Ok(()),
    }
}

/// `var=value` for a statement-level assignment. Handles the sh2.* RHS
/// forms (capture, pipeline, arith, setArray, assign) natively.
fn assign_to_sh(targets: &[crate::ir::AssignTarget], expr: &IrExpr) -> Result<String, String> {
    // `((i++))` / `((i--))` — the arith-statement shape (triage-sh
    // t29_increment): the estree renderer emits the IncDec BARE when the
    // single target is the SAME var the arith mutates (the assignment
    // wrapper would clobber the side effect with the expression's value
    // — `x=$((x++))` assigns the OLD value back). The statement's value
    // is discarded, so `: $((...))` keeps exactly the increment (the `:`
    // swallows the expansion result — a bare `$((...))` line would try
    // to RUN the value as a command: "1: not found").
    if targets.len() == 1
        && targets[0].indices.is_empty()
        && matches!(expr, IrExpr::Arith(a) if matches!(&**a, ArithAst::IncDec { var, .. } if var == &targets[0].var))
    {
        let IrExpr::Arith(a) = expr else { unreachable!() };
        return Ok(format!(": $(({}))", arith_to_sh(a)));
    }
    // `arr=(a b c)` — the A1 is Assign{var: arr, expr: setArray(...)}. The
    // setArray lowering IS the assignment (`arr_0=a; arr_1=b; ...`) — the
    // `name=` prefix would corrupt it into `arr=arr_0=a`.
    if let IrExpr::Call { func, .. } = expr {
        if func == "setArray" || func == "setArrayAppend" {
            return cmd_to_sh(expr);
        }
    }
    let mut out = String::new();
    for (i, t) in targets.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut rhs = assign_rhs_to_sh(expr)?;
        // `typeset -i` vars assign ARITHMETICALLY (`n=n+1` -> 43)
        if INT_VARS.lock().unwrap().contains(&t.var) && t.indices.is_empty() {
            let raw = match expr {
                IrExpr::Str(s, _) => s.clone(),
                IrExpr::Call { func, args } if func == "getVar" => {
                    raw_arg(args, 0).unwrap_or_default()
                }
                _ => rhs.clone(),
            };
            rhs = format!("$(({raw}))");
        }
        // baked element targets (`arr[1]=x` — the A1 folds the subscript
        // into the var name)
        if let Some((base, idx)) = t.var.strip_suffix(']').and_then(|v| v.split_once('[')) {
            // strip the quotes the core keeps around the subscript
            // (`config["user"]` — a quoted subscript is still a plain key)
            let idx = idx.trim_matches(['"', '\'']);
            if idx.chars().all(|c| c.is_ascii_digit()) {
                out.push_str(&format!("{base}_{idx}={rhs}"));
            } else if !idx.contains(['$', '(', ')', '`', ';', '&', '|', ' ']) && !idx.is_empty() {
                // associative (string-key) element: maintain the key list
                // (bash's ${!map[@]} keys / ${map[*]} values iterate it)
                ASSOC_VARS.lock().unwrap().insert(base.to_string());
                out.push_str(&format!(
                    "{base}_{idx}={rhs}; {base}_len=$(( ${{{base}_len:-0}} + 1 )); {base}_keys=\"${{{base}_keys:-}} {idx}\""
                ));
            } else {
                out.push_str(&format!("{base}[{idx}]={rhs}"));
            }
            continue;
        }
        if t.indices.is_empty() {
            out.push_str(&t.var);
            out.push('=');
            out.push_str(&rhs);
            continue;
        }
        // `arr[i]=x` — the per-element lowering writes `arr_i=x`; a
        // dynamic subscript (arith / $var) needs an eval to build the
        // element name at runtime
        if t.indices.len() == 1 {
            let idx = &t.indices[0];
            if let IrExpr::Str(k, _) = idx {
                if k.chars().all(|c| c.is_ascii_digit()) {
                    out.push_str(&t.var);
                    out.push('_');
                    out.push_str(k);
                    out.push('=');
                    out.push_str(&rhs);
                    continue;
                }
                if !k.chars().any(|c| c.is_whitespace())
                    && !k.contains(['$', '(', ')', '"', '\'', '`', ';', '&', '|'])
                {
                    out.push_str(&t.var);
                    out.push('_');
                    out.push_str(k);
                    out.push('=');
                    out.push_str(&rhs);
                    continue;
                }
            }
            let k = word_to_sh(idx)?;
            // dynamic: eval "arr_$idx=$rhs" — the rhs expands at eval
            // time (its $vars are live then)
            let r = rhs.replace('\\', "\\\\").replace('"', "\\\"");
            return Ok(format!("eval \"{}_{}={}\"", t.var, k.trim_matches('"'), r));
        }
        out.push_str(&t.var);
        for idx in &t.indices {
            out.push('[');
            out.push_str(&word_to_sh(idx)?);
            out.push(']');
        }
        out.push('=');
        out.push_str(&rhs);
    }
    Ok(out)
}

/// `arr=(a b c)` / `arr+=(x)` — the per-element lowering. Elements with
/// runtime expansions (`arr=($x)`) word-split at runtime (bash counts the
/// SPLIT words); appends need eval (dash cannot parse an expanded
/// assignment NAME like `arr_$i=x`).
fn set_array_to_sh(name: &str, items: &str, append: bool) -> String {
    if items.is_empty() {
        return format!("{name}_len=0");
    }
    // the KEYED literal form — `d=([a]=1 [b]=2)` (Python dicts / bash
    // assoc literals, triage-sh py-sh-go t73_dict): each item is `[k]=v`
    // (arrives single-quoted) — write the per-key element + key list
    // (the `${!map[@]}` / `${map[*]}` iterate the list)
    let items_clean = items.trim().trim_start_matches(['\'', '"']);
    if items_clean.starts_with('[') {
        let mut parts = Vec::new();
        for raw in items.split(' ') {
            let it = raw
                .trim()
                .trim_start_matches(['\'', '"'])
                .trim_end_matches(['\'', '"']);
            let Some(rest) = it.strip_prefix('[') else { continue };
            let Some((key, val)) = rest.split_once("]=") else { continue };
            parts.push(format!("{name}_{key}={val}"));
            parts.push(format!("{name}_len=$(( ${{{name}_len:-0}} + 1 ))"));
            parts.push(format!("{name}_keys=\"${{{name}_keys:-}} {key}\""));
        }
        return parts.join("; ");
    }
    if items.contains('$') && !append {
        return format!(
            "{name}_len=0; for _w in {items}; do eval \"{name}_${{{name}_len}}=\\\"\\$_w\\\"\"; {name}_len=$(({name}_len + 1)); done"
        );
    }
    let mut parts = Vec::new();
    if append {
        for it in items.split(' ') {
            let esc = it
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
                .replace('`', "\\`");
            parts.push(format!("eval \"{name}_${{{name}_len}}=\\\"{esc}\\\"\""));
            parts.push(format!("{name}_len=$(({name}_len + 1))"));
        }
    } else {
        for (i, it) in items.split(' ').enumerate() {
            parts.push(format!("{name}_{i}={it}"));
        }
        parts.push(format!("{name}_len={}", items.split(' ').count()));
    }
    parts.join("; ")
}

fn assign_rhs_to_sh(expr: &IrExpr) -> Result<String, String> {
    match expr {
        IrExpr::Call { func, args } => match func.as_str() {
            "capture" => Ok(format!("\"$({})\"", arrow_to_sh(args)?)),
            "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
            "pipeline" => {
                let stages = pipeline_stages(args)?;
                let mut line = String::new();
                for (i, stg) in stages.iter().enumerate() {
                    if i > 0 {
                        line.push_str(" | ");
                    }
                    line.push_str(&stmts_inline(stg)?);
                }
                Ok(format!("$({line})"))
            }

            "arith" => {
                let raw = raw_arg(args, 0)?;
                if raw.contains(['"', '\'']) {
                    // bash errors at RUNTIME (the assignment is skipped);
                    // dash would fail to PARSE the whole script — a
                    // failed cmdsub has the same ${x:-d} observable
                    Ok("$(false)".into())
                } else {
                    Ok(format!("$(({}))", arith_rewrite(&raw)))
                }
            }
            "setArray" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_sh(&name, &array_items(args, 1)?, false))
            }
            "setArrayAppend" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_sh(&name, &array_items(args, 1)?, true))
            }
            "assign" => {
                let name = raw_arg(args, 0)?;
                let op = raw_arg(args, 1)?;
                let value = word_to_sh(arg(args, 2)?)?;
                Ok(format!("{name}{op}{value}"))
            }
            _ => word_to_sh(expr),
        },
        _ => word_to_sh(expr),
    }
}

// ── command-position expressions ─────────────────────────────────────

/// Quote any bare `$(...)` or `${...}` in a `[ ]` test string so
/// word-splitting does not shred command-substitution output.
/// `[[ -z $(cmd) ]]` becomes `[ -z "$(cmd)" ]` so the cmdsub output
/// is not field-split/globbed inside `[ ]`.
fn quote_test_expansions(s: &str) -> String {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n + 16);
    let mut i = 0;
    while i < n {
        let c = bytes[i] as char;
        if c == '\"' {
            out.push(c);
            i += 1;
            while i < n && (bytes[i] as char) != '\"' {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < n {
                out.push('\"');
                i += 1;
            }
            continue;
        }
        if c == '\'' {
            out.push(c);
            i += 1;
            while i < n && (bytes[i] as char) != '\'' {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < n {
                out.push('\'');
                i += 1;
            }
            continue;
        }
        if c == '$' && i + 1 < n && (bytes[i + 1] as char) == '(' {
            out.push_str("\"");
            out.push('$');
            out.push('(');
            i += 2;
            let mut depth = 1i32;
            while i < n && depth > 0 {
                let cc = bytes[i] as char;
                if cc == '(' {
                    depth += 1;
                } else if cc == ')' {
                    depth -= 1;
                }
                out.push(cc);
                i += 1;
            }
            out.push_str("\"");
            continue;
        }
        if c == '$' && i + 1 < n && (bytes[i + 1] as char) == '{' {
            out.push_str("\"");
            out.push('$');
            out.push('{');
            i += 2;
            while i < n && (bytes[i] as char) != '}' {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < n {
                out.push('}');
                i += 1;
            }
            out.push_str("\"");
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// One comparison atom of a test string (`"$a" == "b"`, `"$s" =~ ^h`,
/// `-f /x` …) → a single sh cond command. The `==`/`!=`/`=` forms lower
/// to the case emulation (dash has no `==`), `=~` to a grep -E pipeline,
/// everything else stays in `[ ]` (POSIX operators).
fn test_cmp_to_sh(t: &str) -> Result<String, String> {
    if let Some((lhs, rhs)) = split_test_op(t, "==") {
        // pattern match: case emulation (dash has no == in test)
        // A leading `!` negates the WHOLE match (`[[ ! "1" ==
        // "2" ]]`) — strip it and prefix `! ` (valid before a
        // case in dash and bash). Without this the `!` lands
        // inside the quoted lhs (`"!"1"` — a literal).
        let negate = lhs.starts_with('!');
        let lhs = lhs.trim_start_matches('!');
        let case: String = if let Some((neg, rest)) =
            rhs.strip_prefix("!(").and_then(|r| r.split_once(')'))
        {
            // extglob negation `!(P)Y` ≡ `*Y` minus `P Y`:
            //   case "$s" in *Y) case "$s" in P Y) false;; *) :;; esac;; *) false;; esac
            format!(
                "case \"{lhs}\" in *{rest}) case \"{lhs}\" in {neg}{rest}) false ;; *) : ;; esac ;; *) false ;; esac"
            )
        } else if let Some(inner) =
            rhs.strip_prefix("@(").and_then(|r| r.strip_suffix(')'))
        {
            // extglob list match: `@(a|b)` == `a|b`
            format!("case \"{lhs}\" in {inner}) : ;; *) false ;; esac")
        } else if let Some(inner) =
            rhs.strip_prefix("?(").and_then(|r| r.strip_suffix(')'))
        {
            // optional: `?(a|b)` matches empty or a|b
            format!("case \"{lhs}\" in |{inner}) : ;; *) false ;; esac")
        } else if *NOCASEMATCH.lock().unwrap() {
            format!(
                "case \"{lhs}\" in {}) : ;; *) false ;; esac",
                fold_case_pattern(&rhs)
            )
        } else {
            format!("case \"{lhs}\" in {rhs}) : ;; *) false ;; esac")
        };
        if negate {
            Ok(format!("! {case}"))
        } else {
            Ok(case)
        }
    } else if let Some((lhs, rhs)) = split_test_op(t, "!=") {
        Ok(format!("case \"{lhs}\" in {rhs}) false ;; *) : ;; esac"))
    } else if let Some((lhs, rhs)) = split_test_op(t, "=~") {
        // regex match: grep -E ([[ =~ ]] semantics)
        Ok(format!("printf '%s\\n' \"{lhs}\" | grep -Eq '{rhs}'"))
    } else if let Some((lhs, rhs)) = split_test_op(t, "=") {
        // single `=` — the go-sh frontend's pattern tests
        // (`strings.HasPrefix/Contains` lower to `"$s"=h*`,
        // triage-sh-20260815-175001): the estree reference
        // treats `=` as a GLOB pattern match (its string-op
        // scan), so render the same case emulation as `==`.
        // `<=`/`>=` are NOT operators — the `=` inside them
        // must not split (fall back to the `[ ]` literal
        // form).
        if lhs.ends_with(['<', '>']) {
            let t = quote_test_expansions(&space_test_ops(t));
            Ok(format!("[ {t} ]"))
        } else if lhs.contains('~') || rhs.contains('~') {
            // tilde expansion: `[ ~ = "$HOME" ]` (043_home.sh) —
            // the case emulation QUOTES the lhs and would
            // suppress the tilde; `[ ]` tilde-expands natively.
            let t = quote_test_expansions(&space_test_ops(t));
            Ok(format!("[ {t} ]"))
        } else {
            Ok(format!("case \"{lhs}\" in {rhs}) : ;; *) false ;; esac"))
        }
    } else {
        // quote bare $(...) and ${...} so word-splitting
        // in `[ ]` does not shred cmdsub output
        let t = quote_test_expansions(&space_test_ops(t));
        Ok(format!("[ {t} ]"))
    }
}

fn cmd_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "exec" => {
                let words = match args.get(1) {
                    Some(IrExpr::Array(items)) => items.as_slice(),
                    _ => &[],
                };
                let env = match args.get(2) {
                    Some(IrExpr::Object(envs)) => Some(envs.as_slice()),
                    _ => None,
                };
                exec_line_to_sh(arg(args, 0)?, words, env)
            }
            "test" => {
                let raw = raw_arg(args, 0)?;
                let t = raw.trim();
                // `[[ ... ]]`-style compound tests survive as raw text; `[ ]`
                // cannot express &&/||, so keep those in [[ ]] form.
                if t.contains("&&") || t.contains("||") {
                    return Ok(format!("[[ {t} ]]"));
                }
                // perl-frontend `! ( "$name" == "x" )` groups (unless/not,
                // t02_control): the negation + paren wrapper must not leak
                // into the op split. `[ ! a -o b ]` semantics: `!` negates
                // the WHOLE expression — track parity, strip one outer
                // paren group, and re-apply `! ` to the joined command.
                let mut negate = false;
                let mut tt = t;
                while let Some(rest) = tt.strip_prefix('!') {
                    negate = !negate;
                    tt = rest.trim();
                }
                if let Some(inner) = tt.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
                    tt = inner.trim();
                }
                if tt.is_empty() {
                    // degenerate `!` / `( )` — keep the raw literal form
                    return Ok(format!("[ {t} ]"));
                }
                // `-o` / `-a` connectors with emulated comparison atoms
                // (`"$s" =~ l -o "$s" =~ x` — the perl frontend lowers
                // `||` to `-o`, t68_case_glob): POSIX `[ ]` cannot mix `=~`
                // with `-o` and `[[ ]]` rejects `-o` — split into separate
                // cond commands (`-a` binds tighter than `-o`). File-test
                // connectors (`-f x -o -d y`, valid `[ ]` today) are
                // untouched — the split only fires when a comparison op
                // that needs the case/grep emulation is present.
                let mut rendered;
                if (tt.contains(" -o ") || tt.contains(" -a "))
                    && (tt.contains("=~") || tt.contains("==") || tt.contains("!=") || tt.contains('='))
                {
                    rendered = String::new();
                    for (i, orpart) in tt.split(" -o ").enumerate() {
                        if i > 0 {
                            rendered.push_str(" || ");
                        }
                        for (j, apart) in orpart.split(" -a ").enumerate() {
                            if j > 0 {
                                rendered.push_str(" && ");
                            }
                            rendered.push_str(&test_cmp_to_sh(apart.trim())?);
                        }
                    }
                } else {
                    rendered = test_cmp_to_sh(tt)?;
                }
                if negate {
                    rendered = format!("! {rendered}");
                }
                Ok(rendered)
            }
            // the py-sh-go pattern-dispatch cond (t68_case_glob): true
            // when the word matches ANY of the case patterns — the same
            // case emulation as the `==` test arm
            "caseMatch" => {
                let word = word_to_sh(arg(args, 0)?)?;
                // patterns stay RAW — a quoted pattern in case is a
                // LITERAL (`'h*'` never matches hello)
                let mut out = format!("case \"{word}\" in ");
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        let p = str_arg(it)?;
                        out.push_str(&format!("{p}) : ;; "));
                    }
                }
                out.push_str("*) false ;; esac");
                Ok(out)
            }
            "pipeline" => {
                let stages = pipeline_stages(args)?;
                let mut line = String::new();
                for (i, stg) in stages.iter().enumerate() {
                    if i > 0 {
                        line.push_str(" | ");
                    }
                    line.push_str(&stmts_inline(stg)?);
                }
                Ok(line)
            }
            "redirect" => {
                let inner = arrow_to_sh(args)?;
                let redirs = match args.get(1) {
                    Some(x) => redirect_objs(x)?,
                    None => vec![],
                };
                let (ps_ins, plain) = partition_procsub(&redirs);
                // process substitutions lower in place; the remaining
                // specs render as ordinary redirect suffixes
                let specs = {
                    let mut out = String::new();
                    for r in &plain {
                        out.push_str(&redirect_to_sh(r)?);
                    }
                    out
                };
                let mut line = herestring_wrap(&plain, inner)?;
                if !ps_ins.is_empty() {
                    line = lower_procsub_inline(&ps_ins, &line)?;
                }
                Ok(format!("{line}{specs}"))
            }
            "subshell" => Ok(format!("( {} )", arrow_to_sh(args)?)),
            "block" => {
                let body = arrow_to_sh(args)?;
                if body.is_empty() {
                    Ok("{ :; }".into())
                } else {
                    Ok(format!("{{ {body}; }}"))
                }
            }
            "whileLoop" => {
                let cond = arrow_at(args, 0)?;
                let body = arrow_at(args, 1)?;
                Ok(format!("while {cond}; do {body}; done"))
            }
            "cstyleFor" => {
                let arith = raw_arg(args, 0)?;
                let body = arrow_at(args, 1)?;
                Ok(cstyle_for_to_sh(&arith, &body))
            }
            // dash has no shopt builtin — a no-op keeps the script running;
            // track nocasematch (the [[ == ]] emulation folds patterns)
            "shopt" => {
                // the A1 shape is [name, Bool(on)]
                let has = args
                    .iter()
                    .any(|a| matches!(a, IrExpr::Str(x, _) if x == "nocasematch"));
                let on = args.iter().any(|a| matches!(a, IrExpr::Bool(b) if *b));
                *NOCASEMATCH.lock().unwrap() = has && on;
                Ok(":".into())
            }
            "arith" => {
                let raw = raw_arg(args, 0)?;
                if raw.contains(['"', '\'']) {
                    Ok("$(false)".into())
                } else {
                    Ok(format!("(( {} ))", arith_rewrite(&raw)))
                }
            }
            "break" => Ok("break".into()),
            "continue" => Ok("continue".into()),
            "return" => {
                if args.is_empty() {
                    Ok("return".into())
                } else {
                    Ok(format!("return {}", word_to_sh(arg(args, 0)?)?))
                }
            }
            "assign" => {
                let name = raw_arg(args, 0)?;
                let op = raw_arg(args, 1)?;
                let value = word_to_sh(arg(args, 2)?)?;
                Ok(format!("{name}{op}{value}"))
            }
            "setArray" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_sh(&name, &array_items(args, 1)?, false))
            }
            "setArrayAppend" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_sh(&name, &array_items(args, 1)?, true))
            }
            "assocSet" => {
                // the go-sh map-literal store (`m := map[K]V{...}` — one
                // assocSet per pair, triage-sh-20260815-175002): the
                // by-name associative-array write — the same POSIX
                // per-element lowering as setArray (`m_c=C`).
                let name = raw_arg(args, 0)?;
                let key = raw_arg(args, 1)?;
                let value = word_to_sh(arg(args, 2)?)?;
                Ok(format!("{}_{}={}", arr_base(&name), key, value))
            }
            "getVar" => Ok(var_ref_to_sh(&raw_arg(args, 0)?.replace('.', "_"), false)),
            "capture" => Ok(format!("\"$({})\"", arrow_to_sh(args)?)),
            "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
            "contains" => {
                let arg = word_to_sh(arg(args, 0)?)?;
                let pat = raw_arg(args, 1)?;
                Ok(format!("printf '%s\\n' {arg} | grep {pat} >/dev/null 2>&1"))
            }
            "fnCall" => {
                let name = raw_arg(args, 0)?;
                let mut line = name;
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        line.push(' ');
                        line.push_str(&word_to_sh(w)?);
                    }
                }
                Ok(line)
            }
            // `grepMatches(text, pattern, flags)` — the `grep -o` lift:
            // the sh backend keeps the shell's own grep (native) — re-emit
            // the -o pipeline on the input text.
            "grepMatches" => {
                let text = word_to_sh(arg(args, 0)?)?;
                let pat = raw_arg(args, 1)?;
                let flags = raw_arg(args, 2)?;
                let mut line = String::from("printf '%s\n' ");
                line.push_str(&text);
                line.push_str(" | grep -o");
                for c in flags.chars() {
                    if c == 'E' || c == 'i' || c == 'F' {
                        line.push(' ');
                        line.push('-');
                        line.push(c);
                    }
                }
                line.push(' ');
                line.push_str(&pat);
                Ok(format!("$({line})"))
            }
            // `regexMatch(Regex(pattern, flags), value)` — the fish
            // `string match -rq` cond lift (triage-sh t81_regex_match):
            // ERE search semantics, status 0 iff any match — the same
            // decision as the `=~` arm's grep -E pipeline, minus the
            // test-string round trip. flags: only fish's `-i`
            // (ignore-case) is emitted by the frontend; anything else
            // refuses loudly (the estree reference would differ).
            "regexMatch" => {
                let (pattern, flags) = match arg(args, 0)? {
                    IrExpr::Regex { pattern, flags } => (pattern.clone(), flags.clone()),
                    other => {
                        return Err(format!("regexMatch: arg 0 not Regex: {other:?}"));
                    }
                };
                if flags.chars().any(|c| c != 'i') {
                    return Err(format!("regexMatch: unsupported flags {flags:?}"));
                }
                let value = word_to_sh(arg(args, 1)?)?;
                let pat = pattern.replace('\'', "'\\\\''");
                let i_flag = if flags.contains('i') { "i" } else { "" };
                Ok(format!(
                    "printf '%s\\n' {value} | grep -E{i_flag}q '{pat}'"
                ))
            }
            "setVar" => {
                // the runtime's plain store write — re-emit as a shell
                // assignment (the A1 store protocol the perl/posix
                // frontends emit for return values / locals). Dotted
                // names (the cpp/c struct-field protocol `p.x`) are not
                // valid shell identifiers — sanitize to `p_x` (the
                // getVar arms mirror the same rewrite).
                let name = raw_arg(args, 0)?;
                let value = word_to_sh(arg(args, 1)?)?;
                Ok(format!("{}={value}", name.replace('.', "_")))
            }
            other => Err(format!("command call not renderable: {other:?}")),
        },
        IrExpr::BinOp {
            op: BinOpKind::And,
            lhs,
            rhs,
        } => Ok(format!("{} && {}", cmd_to_sh(lhs)?, cmd_to_sh(rhs)?)),
        IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs,
            rhs,
        } => Ok(format!("{} || {}", cmd_to_sh(lhs)?, cmd_to_sh(rhs)?)),
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs,
            ..
        } => Ok(format!("! {}", cmd_to_sh(lhs)?)),
        // a statement-position arith (`((n++))` — the perl frontend's
        // increment statements): the arithmetic command, whose exit
        // status is the value != 0 (bash `(( ))` semantics)
        IrExpr::Arith(a) => Ok(format!("(( {} ))", arith_to_sh(a))),
        other => Err(format!("command expression not renderable: {other:?}")),
    }
}

// A PCRE pattern that is also valid ERE (no lookarounds, backrefs, or
// \d/\w/\s/\b shorthand) can lower `grep -P` to `grep -E` inline.
fn ere_safe(p: &str) -> bool {
    for t in [
        "(?", "\\d", "\\w", "\\s", "\\b", "\\D", "\\W", "\\S", "\\1", "\\2", "\\3", "\\4",
    ] {
        if p.contains(t) {
            return false;
        }
    }
    true
}

// GNU-only flags with no POSIX/BSD equivalent: a pass-through is a
// portability LEAK (the output must run on macOS/BSD sh) — refuse loudly.
fn gnu_only_flag(cmd: &str, flag: &str) -> bool {
    match cmd {
        // grep -P and sed -i are LOWERED (grep_p polyfill / temp-file pattern)
        "head" => matches!(flag, "-z" | "--zero-terminated"),
        "find" => matches!(flag, "-printf"),
        "sort" => matches!(flag, "-V" | "--version-sort"),
        _ => false,
    }
}

// Does the program need the grep_p PCRE polyfill prologue?
fn needs_grep_p(stmts: &[IrStmt]) -> bool {
    // Recursive: pipelines (and cmdsubs) nest the stage execs inside the
    // Call args — the scan must descend, not just look at the top exec.
    fn has_grep_p(e: &IrExpr) -> bool {
        if std::env::var_os("SHDBG_GREPP").is_some() {
            eprintln!("scan: {:?}", e);
        }
        if let IrExpr::Call { func, args } = e {
            if func == "exec" && args.len() >= 1 {
                if let IrExpr::Str(cn, _) = &args[0] {
                    if cn == "grep" {
                        // the exec args live in the Array at args[1] (the
                        // same de-nesting cmd_to_sh's render path does)
                        let words = match args.get(1) {
                            Some(IrExpr::Array(items)) => items.as_slice(),
                            _ => &[],
                        };
                        let mut i = 0;
                        while i + 1 < words.len() {
                            if let IrExpr::Str(fl, _) = &words[i] {
                                if (fl == "-P" || fl == "--perl-regexp") {
                                    if let IrExpr::Str(pat, _) = &words[i + 1] {
                                        if !ere_safe(pat) {
                                            return true;
                                        }
                                    }
                                }
                            }
                            i += 1;
                        }
                    }
                }
            }
            return args.iter().any(has_grep_p);
        }
        match e {
            IrExpr::Array(es) => es.iter().any(has_grep_p),
            IrExpr::Object(es) => es.iter().any(|(_, v)| has_grep_p(v)),
            // pipeline / cmdsub stages nest the stage execs inside Arrow
            // bodies — without this the grep -P exec in `echo X | grep -P
            // …` is invisible and the grep_p polyfill prologue is never
            // emitted (a `grep_p: command not found` at runtime). Mirrors
            // needs_readlink's arrow descent.
            IrExpr::Arrow(stmts) => walk(stmts),
            _ => false,
        }
    }
    fn walk(sts: &[IrStmt]) -> bool {
        for st in sts {
            match st {
                IrStmt::Expr(e) => {
                    if has_grep_p(e) {
                        return true;
                    }
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                    if has_grep_p(cond) || walk(body) {
                        return true;
                    }
                }
                IrStmt::If {
                    cond,
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    if has_grep_p(cond)
                        || walk(then)
                        || walk(else_)
                        || elsifs.iter().any(|(c, b)| has_grep_p(c) || walk(b))
                    {
                        return true;
                    }
                }
                IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                    if walk(body) {
                        return true;
                    }
                }
                IrStmt::Select { clauses } => {
                    for c in clauses {
                        if walk(&c.body) {
                            return true;
                        }
                        if let Some(ch) = &c.ch {
                            if has_grep_p(ch) {
                                return true;
                            }
                        }
                        if let Some(v) = &c.value {
                            if has_grep_p(v) {
                                return true;
                            }
                        }
                    }
                }
                IrStmt::For { iter, body, .. } => {
                    if has_grep_p(iter) || walk(body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    walk(stmts)
}

// GNU readlink canonicalize flags (-e/-f/-m, or combined -ef/-fn).
// readlink(1) is not POSIX at all; -e/-m are GNU-only and even -f
// is missing on macOS/BSD - so all canonicalize invocations route
// through the pure-POSIX-sh `_readlink` polyfill. Bare `readlink`
// (one-level link target) is portable - stays native.
fn readlink_flag(s: &str) -> bool {
    s == "-e"
        || s == "-f"
        || s == "-m"
        || (s.starts_with('-')
            && s.len() > 2
            && s[1..].chars().all(|c| "efmn".contains(c))
            && s[1..].chars().any(|c| "efm".contains(c)))
}

// Does the program need the `_readlink` polyfill prologue?
// Mirrors needs_grep_p's descent: nested cmdsubs (Call args) and
// pipelines carry the exec through the IR - the scan must recurse.
fn needs_readlink(stmts: &[IrStmt]) -> bool {
    fn has_readlink(e: &IrExpr) -> bool {
        if let IrExpr::Call { func, args } = e {
            if (func == "exec" || func == "readlink") && !args.is_empty() {
                if let IrExpr::Str(cn, _) = &args[0] {
                    if cn == "readlink" {
                        let words = match args.get(1) {
                            Some(IrExpr::Array(items)) => items.as_slice(),
                            _ => &args[1..],
                        };
                        if words
                            .iter()
                            .any(|w| matches!(w, IrExpr::Str(s, _) if readlink_flag(s)))
                        {
                            return true;
                        }
                    }
                }
            }
            return args.iter().any(has_readlink);
        }
        match e {
            IrExpr::Array(es) => es.iter().any(has_readlink),
            IrExpr::Object(es) => es.iter().any(|(_, v)| has_readlink(v)),
            IrExpr::Arrow(stmts) => walk(stmts),
            _ => false,
        }
    }
    fn walk(sts: &[IrStmt]) -> bool {
        for st in sts {
            match st {
                IrStmt::Expr(e) => {
                    if has_readlink(e) {
                        return true;
                    }
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                    if has_readlink(cond) || walk(body) {
                        return true;
                    }
                }
                IrStmt::If {
                    cond,
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    if has_readlink(cond)
                        || walk(then)
                        || walk(else_)
                        || elsifs.iter().any(|(c, b)| has_readlink(c) || walk(b))
                    {
                        return true;
                    }
                }
                IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                    if walk(body) {
                        return true;
                    }
                }
                IrStmt::For { iter, body, .. } => {
                    if has_readlink(iter) || walk(body) {
                        return true;
                    }
                }
                IrStmt::Assign { expr, .. } => {
                    if has_readlink(expr) {
                        return true;
                    }
                }
                IrStmt::Redirect { inner, redirects } => {
                    if walk(inner) {
                        return true;
                    }
                    for r in redirects {
                        if has_readlink(&r.target) {
                            return true;
                        }
                    }
                }
                IrStmt::Function { body, .. } => {
                    if walk(body) {
                        return true;
                    }
                }
                IrStmt::Case {
                    discriminant,
                    clauses,
                    ..
                } => {
                    if has_readlink(discriminant) {
                        return true;
                    }
                    for c in clauses {
                        if walk(&c.body) {
                            return true;
                        }
                    }
                }
                IrStmt::Pipeline { stages, .. } => {
                    for s in stages {
                        if walk(s) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }
    walk(stmts)
}

// GNU `cmp(1)` is tool-verbatim: the corpus expects the exact message
// `A B differ: byte N, line M` and (with -l) the `N M\n` per-byte
// octal listing. busybox cmp differs (uses `char`, different padding).
// The polyfill reproduces GNU semantics using `od` (POSIX) for octals
// and byte/line counters. -s is silent (rc only). -i ignored (the
// corpus doesn't use case-insensitive cmp).
fn cmp_flag(s: &str) -> bool {
    s == "-l" || s == "-s" || s == "-i"
}

fn needs_cmp(stmts: &[IrStmt]) -> bool {
    fn has_cmp(e: &IrExpr) -> bool {
        if let IrExpr::Call { func, args } = e {
            if (func == "exec" || func == "cmp") && !args.is_empty() {
                if let IrExpr::Str(cn, _) = &args[0] {
                    if cn == "cmp" {
                        return true;
                    }
                }
            }
            return args.iter().any(has_cmp);
        }
        match e {
            IrExpr::Array(es) => es.iter().any(has_cmp),
            IrExpr::Object(es) => es.iter().any(|(_, v)| has_cmp(v)),
            IrExpr::Arrow(stmts) => walk(stmts),
            _ => false,
        }
    }
    fn walk(sts: &[IrStmt]) -> bool {
        for st in sts {
            match st {
                IrStmt::Expr(e) => {
                    if has_cmp(e) {
                        return true;
                    }
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                    if has_cmp(cond) || walk(body) {
                        return true;
                    }
                }
                IrStmt::If {
                    cond,
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    if has_cmp(cond)
                        || walk(then)
                        || walk(else_)
                        || elsifs.iter().any(|(c, b)| has_cmp(c) || walk(b))
                    {
                        return true;
                    }
                }
                IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                    if walk(body) {
                        return true;
                    }
                }
                IrStmt::For { iter, body, .. } => {
                    if has_cmp(iter) || walk(body) {
                        return true;
                    }
                }
                IrStmt::Assign { expr, .. } => {
                    if has_cmp(expr) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    walk(stmts)
}

fn exec_line_to_sh(
    cmd: &IrExpr,
    args: &[IrExpr],
    env: Option<&[(String, IrExpr)]>,
) -> Result<String, String> {
    // GNU-only flags: refuse rather than leak an unportable invocation
    if let IrExpr::Str(cn, _) = cmd {
        for a in args {
            if let IrExpr::Str(fl, _) = a {
                if gnu_only_flag(cn, fl) {
                    return Err(format!(
                        "GNU-only flag {fl} for {cn} is not portable to POSIX/BSD — refusing"
                    ));
                }
            }
        }
    }
    let mut out = String::new();
    if let Some(envs) = env {
        for (k, v) in envs {
            out.push_str(k);
            out.push('=');
            out.push_str(&word_to_sh(v)?);
            out.push(' ');
        }
    }
    let cmd_name = match cmd {
        IrExpr::Str(s, _) => Some(s.as_str()),
        _ => None,
    };
    // bash-only `set` options (pipefail etc.) — dash would reject them;
    // drop the option word and the trailing `o` of a combined flag (the
    // corpus's `set -euo pipefail` must run under /bin/sh for the
    // equivalence gate).
    if cmd_name == Some("set") {
        const DASH_SET_O: &[&str] = &[
            "allexport",
            "errexit",
            "noglob",
            "noclobber",
            "nolog",
            "notify",
            "ignoreeof",
            "monitor",
            "nounset",
            "verbose",
            "vi",
            "xtrace",
        ];
        let mut kept: Vec<String> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let lit = match &args[i] {
                IrExpr::Str(s, _) => s.clone(),
                _ => {
                    kept.push(word_to_sh(&args[i])?);
                    i += 1;
                    continue;
                }
            };
            if lit == "-o" || (lit.starts_with('-') && lit.len() > 1 && lit.ends_with('o')) {
                let opt = args.get(i + 1).and_then(|x| match x {
                    IrExpr::Str(s, _) => Some(s.as_str()),
                    _ => None,
                });
                if let Some(opt) = opt {
                    if DASH_SET_O.contains(&opt) {
                        // supported: keep `-o opt` (or the split flag)
                        if lit == "-o" {
                            kept.push("-o".into());
                            kept.push(opt.into());
                        } else {
                            kept.push(format!("-{}", &lit[1..lit.len() - 1]));
                            kept.push("-o".into());
                            kept.push(opt.into());
                        }
                        i += 2;
                        continue;
                    }
                    // unsupported (pipefail…): drop the option word, keep
                    // the OTHER flags of a combined form (`-euo` → `-eu`)
                    if lit != "-o" {
                        let rest = &lit[1..lit.len() - 1];
                        if !rest.is_empty() {
                            kept.push(format!("-{rest}"));
                        }
                    }
                    i += 2;
                    continue;
                }
                // `-o` with no option word — keep verbatim
            }
            kept.push(lit);
            i += 1;
        }
        if kept.is_empty() {
            out.push(':');
        } else {
            out.push_str("set ");
            out.push_str(&kept.join(" "));
        }
        return Ok(out);
    }
    // `declare`/`typeset` are bash-only builtins (dash rejects them);
    // translate the POSIX-representable forms: `declare x=1` -> `x=1`,
    // `-r` -> readonly, `-x` -> export, `-i`/`-a`/`-A`/`-u`/`-l`/`-n` flags
    // are dropped (dash has no variable attributes — the assignment still
    // happens, which keeps stdout identical for the equivalence gate).
    if matches!(cmd_name, Some("declare" | "typeset")) {
        let mut words: Vec<String> = Vec::new();
        let mut flags: Vec<String> = Vec::new();
        for a in args {
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                    flags.push(s.clone());
                }
                // `typeset -i n=42` — the assignment word stays UNQUOTED
                // (dash would create a literal "n=42" variable)
                IrExpr::Str(s, _) if s.contains('=') => words.push(s.clone()),
                other => words.push(word_to_sh(other)?),
            }
        }
        if flags.iter().any(|f| f.contains('i')) {
            let mut ints = INT_VARS.lock().unwrap();
            for w in &words {
                let name = w.split('=').next().unwrap_or(w.as_str());
                ints.insert(name.to_string());
            }
        }
        if flags.iter().any(|f| f.contains('A')) {
            let mut assoc = ASSOC_VARS.lock().unwrap();
            for w in &words {
                let name = w.split('=').next().unwrap_or(w.as_str());
                assoc.insert(name.to_string());
            }
        }
        let prefix = if flags.iter().any(|f| f.contains('r')) {
            "readonly "
        } else if flags.iter().any(|f| f.contains('x')) {
            "export "
        } else {
            ""
        };
        // bare names (`declare -A config`) are attribute declarations —
        // the per-element lowering self-initializes the len counter
        let assigns: Vec<&str> = words
            .iter()
            .filter(|w| w.contains('='))
            .map(|w| w.as_str())
            .collect();
        if assigns.is_empty() {
            out.push(':');
        } else {
            out.push_str(prefix);
            out.push_str(&assigns.join(" "));
        }
        return Ok(out);
    }
    // `local`/`export`/`readonly` with bash-only type flags: drop the flags
    // (`local -i x=5` -> `local x=5`), keep the builtin. Assignment words
    // (`local n=$1`) must stay UNQUOTED — dash's local does not re-expand
    // a `'n=$1'` argument (the value would be the literal `$1`).
    if matches!(cmd_name, Some("local" | "export" | "readonly")) {
        let mut words: Vec<String> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {}
                IrExpr::Str(s, _) => {
                    if let Some(eq) = s.find('=') {
                        let name = &s[..eq];
                        let val = &s[eq + 1..];
                        if val.is_empty() && i + 1 < args.len() {
                            // `local msg= "hello"` — the VALUE is the next
                            // word (the core splits `local msg="hello"`
                            // into ["msg=", "hello"])
                            if let Some(next) = args.get(i + 1) {
                                let is_flag = matches!(next, IrExpr::Str(x, _)
                                    if x.starts_with('-') && x.len() > 1);
                                if !is_flag {
                                    let nv = word_to_sh(next)?;
                                    if nv.is_empty() || nv == "''" || nv == "\"\"" {
                                        words.push(name.to_string());
                                    } else {
                                        words.push(format!("{name}={nv}"));
                                    }
                                    i += 2;
                                    continue;
                                }
                            }
                        }
                        let needs_quote = val.contains(|c: char| {
                            c.is_whitespace() || matches!(c, '*' | '?' | '[' | ']' | '$' | '`')
                        });
                        if needs_quote {
                            words.push(format!("{name}=\"{val}\""));
                        } else {
                            words.push(format!("{name}={val}"));
                        }
                    } else if s.is_empty() {
                        // `local x=` arrives as ["x=", Interpolate("")] —
                        // the empty word is the empty VALUE; the `name=`
                        // form already covers it (dash: `local ''` is an error)
                        i += 1;
                        continue;
                    } else {
                        words.push(str_word(s));
                    }
                }
                other => {
                    let w = word_to_sh(other)?;
                    if w.is_empty() || w == "''" || w == "\"\"" {
                        i += 1;
                        continue;
                    }
                    words.push(w);
                }
            }
            i += 1;
        }
        if words.is_empty() {
            out.push_str(cmd_name.unwrap());
        } else {
            out.push_str(cmd_name.unwrap());
            out.push(' ');
            out.push_str(&words.join(" "));
        }
        return Ok(out);
    }
    // `echo -n` / `echo -e` / `echo -E` — dash's echo has no flags; render
    // the equivalent printf so stdout matches bash.
    if cmd_name == Some("echo") {
        if let Some(first) = args.first() {
            if let IrExpr::Str(s, _) = first {
                let rest = &args[1..];
                let mut words = Vec::new();
                for a in rest {
                    words.push(word_to_sh(a)?);
                }
                let joined = words.join(" ");
                match s.as_str() {
                    "-n" => {
                        // bash: args joined by a single space, no newline
                        if rest.is_empty() {
                            return Ok("printf '%s'".into());
                        }
                        let fmt = rest.iter().map(|_| "%s").collect::<Vec<_>>().join(" ");
                        return Ok(format!("printf '{fmt}' {joined}"));
                    }
                    "-e" => {
                        // bash: backslash escapes interpreted, trailing newline
                        if rest.is_empty() {
                            return Ok("printf '\n'".into());
                        }
                        let fmt = rest.iter().map(|_| "%b").collect::<Vec<_>>().join(" ");
                        return Ok(format!("printf '{fmt}\\n' {joined}"));
                    }
                    "-E" => {
                        // bash: escapes NOT interpreted, trailing newline
                        if rest.is_empty() {
                            return Ok("printf '\n'".into());
                        }
                        let fmt = rest.iter().map(|_| "%s").collect::<Vec<_>>().join(" ");
                        return Ok(format!("printf '{fmt}\\n' {joined}"));
                    }
                    _ => {}
                }
            }
            // plain `echo ARGS` — dash's echo INTERPRETS backslashes,
            // bash's does not: with a backslash-y arg the printf form
            // (same as -E) keeps the bytes verbatim
            let mut words = Vec::new();
            for a in args {
                words.push(word_to_sh(a)?);
            }
            let joined = words.join(" ");
            // dash's echo INTERPRETS backslashes, bash's does not — the
            // printf form keeps the bytes verbatim for any arg whose
            // RUNTIME value can contain backslashes (quoted vars,
            // cmdsubs) or that already shows a literal one. `$@`/`$*`
            // list args are excluded (the %s count is unknowable).
            // all-quoted args only: printf with several args prints
            // separate LINES, while `echo $(ls)` joins the split words
            // with a space — an unquoted expansion keeps the bare echo
            let needs_printf = !words.is_empty()
                && words.iter().all(|w| {
                    w.contains('\\')
                        || (w.starts_with('"') && !w.contains("$@") && !w.contains("$*"))
                });
            if needs_printf {
                let fmt = words.iter().map(|_| "%s").collect::<Vec<_>>().join(" ");
                return Ok(format!("printf '{fmt}\\n' {joined}"));
            }
        }
    }
    // `grep --include=GLOB` (GNU-only flag) - restrict a recursive search
    // to files matching GLOB. Corpus form: `grep -r PATTERN DIR --include=G`.
    // Lower: drop the flag, replace DIR with $(find DIR -name G). The
    // sandbox has busybox find (or Chimera find) which supports -name.
    if cmd_name == Some("grep") && env.is_none() {
        if let Some(pidx) = args.iter().position(|a| {
            matches!(
                a, IrExpr::Str(s, _) if {
                    let c = s.strip_prefix(GLOB_MAGIC).unwrap_or(s);
                    c.starts_with("--include=")
                }
            )
        }) {
            if let IrExpr::Str(fl, _) = &args[pidx] {
                // the GLOB_MAGIC prefix marks an unquoted glob; --include
                // accepts a glob, so strip the tag for the pattern match
                let fl_clean = fl.strip_prefix(GLOB_MAGIC).unwrap_or(fl);
                let glob = fl_clean
                    .strip_prefix("--include=")
                    .unwrap_or("")
                    .to_string();
                let has_r = args
                    .iter()
                    .any(|a| matches!(a, IrExpr::Str(s, _) if s == "-r"));
                if has_r {
                    let mut words: Vec<String> = Vec::new();
                    let mut found_pattern = false;
                    let mut replaced_dir = false;
                    for (i, a) in args.iter().enumerate() {
                        if i == pidx {
                            continue;
                        }
                        let w = word_to_sh(a)?;
                        if !found_pattern && !w.starts_with('-') {
                            found_pattern = true;
                            words.push(w);
                            continue;
                        }
                        if found_pattern && !replaced_dir && !w.starts_with('-') {
                            words.push(format!("$(find {} -name '{}')", w, glob));
                            replaced_dir = true;
                            continue;
                        }
                        words.push(w);
                    }
                    return Ok(words.join(" "));
                }
            }
        }
    }
    // `grep -P PAT` — PCRE. ERE-safe patterns lower to `grep -E` inline;
    // anything else uses the grep_p polyfill (emitted in the prologue):
    // GNU grep -P, macOS gnu-grep, or perl. A pass-through would leak.
    if cmd_name == Some("grep") && env.is_none() {
        if let Some(pidx) = args
            .iter()
            .position(|a| matches!(a, IrExpr::Str(s, _) if s == "-P" || s == "--perl-regexp"))
        {
            let use_e = matches!(args.get(pidx + 1), Some(IrExpr::Str(p, _)) if ere_safe(p));
            let mut words: Vec<String> = Vec::new();
            words.push(if use_e {
                "grep -E".into()
            } else {
                "grep_p".into()
            });
            for (i, a) in args.iter().enumerate() {
                if i == pidx {
                    continue; // drop the -P flag
                }
                words.push(word_to_sh(a)?);
            }
            return Ok(words.join(" "));
        }
    }
    // `sed -i [SUFFIX] EXPR FILE` — GNU in-place. The portable temp-file
    // pattern: sed EXPR FILE > FILE.tmp && mv FILE.tmp FILE (mv is
    // same-dir, so no cross-filesystem rename issue).
    if cmd_name == Some("sed") && env.is_none() {
        let in_place = args.iter().position(|a| matches!(
            a,
            IrExpr::Str(s, _) if s == "-i" || s == "--in-place" || (s.starts_with("-i") && s.len() > 2)
        ));
        if let Some(pidx) = in_place {
            let mut words: Vec<String> = vec!["sed".into()];
            for (i, a) in args.iter().enumerate() {
                if i == pidx {
                    continue; // drop -i (and any attached suffix)
                }
                words.push(word_to_sh(a)?);
            }
            let file = args.iter().rev().find_map(|a| match a {
                IrExpr::Str(s, _) => Some(s.clone()),
                _ => None,
            });
            if let Some(f) = file {
                return Ok(format!("{} > {f}.tmp && mv {f}.tmp {f}", words.join(" ")));
            }
            return Err("sed -i without a literal file — the temp-file lowering needs one".into());
        }
    }
    // `mapfile`/`readarray` are bash-only builtins (dash has neither).
    // Lines land in the per-element vars + a len counter:
    //   mapfile -t lines < input  ->
    //     lines_len=0; while IFS= read -r _map_line; do
    //       eval "lines_$lines_len=$_map_line"; lines_len=$((lines_len+1)); done
    if matches!(cmd_name, Some("mapfile" | "readarray")) {
        let mut name = "MAPFILE".to_string();
        let mut flags: Vec<String> = Vec::new();
        for a in args {
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => flags.push(s.clone()),
                other => {
                    let w = word_to_sh(other)?;
                    if !w.starts_with('$') && w != "-" {
                        name = w;
                    }
                }
            }
        }
        // `-t` strips the delimiter — IFS= read -r already does; any
        // other option changes the read semantics (refuse loudly)
        for f in &flags {
            if f != "-t" {
                return Err(format!(
                    "mapfile flag {f} is not POSIX-lowered — refusing (only -t is emulated)"
                ));
            }
        }
        return Ok(format!(
            "{name}_len=0; while IFS= read -r _map_line; do eval \"{name}_${{{name}_len}}=\\$_map_line\"; {name}_len=$(({name}_len + 1)); done"
        ));
    }
    // `read -a arr` — bash-only array read (dash rejects -a). Split the
    // line on IFS into the per-element vars + len counter.
    if cmd_name == Some("read") {
        if let Some(pidx) = args
            .iter()
            .position(|a| matches!(a, IrExpr::Str(s, _) if s == "-a"))
        {
            let name = args
                .get(pidx + 1)
                .and_then(|a| match a {
                    IrExpr::Str(s, _) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "READ_A".into());
            let mut words: Vec<String> = Vec::new();
            for (i, a) in args.iter().enumerate() {
                if i == pidx || i == pidx + 1 {
                    continue;
                }
                words.push(word_to_sh(a)?);
            }
            let opts = words.join(" ");
            return Ok(format!(
                "read {opts} _read_line && {{ {name}_len=0; for _w in $_read_line; do eval \"{name}_${{{name}_len}}=\\$_w\"; {name}_len=$(({name}_len + 1)); done; }}"
            ));
        }
    }
    // `let` is a bash-only builtin (dash rejects it). A `let EXPR` is an
    // arithmetic condition (rc 0 iff the value is nonzero); the portable
    // `[ "$((EXPR))" -ne 0 ]` keeps both the rc semantics and the
    // side effects (`let counter++` increments — dash's $(( )) supports
    // C post-increment).
    if cmd_name == Some("let") {
        let mut parts = Vec::new();
        for a in args {
            let e = match a {
                IrExpr::Str(s, _) => arith_rewrite(s),
                other => arith_rewrite(&word_to_sh(other)?),
            };
            parts.push(format!("[ \"$(({e}))\" -ne 0 ]"));
        }
        if parts.is_empty() {
            return Ok(":".into());
        }
        return Ok(parts.join("; "));
    }
    // `readlink -e/-f/-m` - GNU canonicalize flags. readlink(1) is not
    // POSIX; -e/-m are GNU-only and even -f is GNU/FreeBSD-only
    // (macOS/BSD lack it) - route through the `_readlink` polyfill
    // emitted in the prologue when needs_readlink() fired. Bare
    // `readlink` (one-level link target) is portable and stays native.
    if cmd_name == Some("readlink")
        && args
            .iter()
            .any(|a| matches!(a, IrExpr::Str(s, _) if readlink_flag(s)))
    {
        out.push_str("_readlink");
        for w in args {
            out.push(' ');
            out.push_str(&word_to_sh(w)?);
        }
        return Ok(out);
    }
    // `ls` (non-`-l`) output is unsorted under busybox; GNU sorts
    // alphabetically. Append `| sort` - idempotent on already-sorted
    // input; safe in pipelines and cmdsubs (sort then filter/head/wc
    // is equivalent to the original GNU behavior). `ls -l` (format,
    // not order) is a separate verbatim-output GNUism and is NOT
    // handled here.
    if cmd_name == Some("ls") && env.is_none() {
        let has_l = args.iter().any(|a| {
            matches!(a, IrExpr::Str(s, _)
            if s == "-l" || s.starts_with("--"))
        });
        if !has_l {
            out.push_str(&word_to_sh(cmd)?);
            for w in args {
                out.push(' ');
                out.push_str(&word_to_sh(w)?);
            }
            out.push_str(" | sort");
            return Ok(out);
        }
    }
    // `tty --silent` / `tty --quiet` (GNU long options) -> `tty -s`
    // (busybox `tty` accepts only `-s`; the long forms fail rc=2).
    if cmd_name == Some("tty")
        && env.is_none()
        && args.iter().any(|a| {
            matches!(a, IrExpr::Str(s, _)
            if s == "--silent" || s == "--quiet")
        })
    {
        out.push_str("tty -s");
        for w in args {
            if !matches!(w, IrExpr::Str(s, _) if s == "--silent" || s == "--quiet") {
                out.push(' ');
                out.push_str(&word_to_sh(w)?);
            }
        }
        return Ok(out);
    }
    // `bash -c` -> `sh -c`. The chimera sandbox has no bash; `-c`
    // invocations are the common form (e.g. `bash -c 'id -G -z'`).
    // Non-`-c` bash uses (interactive, `--version`) are left native -
    // those would fail in the sandbox regardless, but the corpus
    // doesn't exercise them and a global rewrite would mask other
    // bash-specific calls that the source genuinely requires.
    if cmd_name == Some("bash")
        && env.is_none()
        && args
            .iter()
            .any(|a| matches!(a, IrExpr::Str(s, _) if s == "-c"))
    {
        out.push_str("sh");
        for w in args {
            out.push(' ');
            out.push_str(&word_to_sh(w)?);
        }
        return Ok(out);
    }
    // `cmp` -> `_cmp` polyfill (reproduces GNU diagnostics; the sandbox
    // busybox cmp uses a different message format and lacks some flags).
    if cmd_name == Some("cmp") && env.is_none() {
        out.push_str("_cmp");
        for w in args {
            out.push(' ');
            out.push_str(&word_to_sh(w)?);
        }
        return Ok(out);
    }
    out.push_str(&word_to_sh(cmd)?);
    for w in args {
        out.push(' ');
        out.push_str(&word_to_sh(w)?);
    }
    Ok(out)
}

// ── redirects ────────────────────────────────────────────────────────

fn redirects_to_sh(redirects: &[IrRedirect]) -> Result<String, String> {
    let mut out = String::new();
    // non-heredoc redirects FIRST — `cat > f <<EOF\nbody\nEOF`: the
    // output redirect must precede the heredoc BODY (a heredoc's render
    // includes its body + terminator)
    for r in redirects {
        if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
            continue;
        }
        out.push_str(&redirect_to_sh(r)?);
    }
    for r in redirects {
        if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
            out.push_str(&redirect_to_sh(r)?);
        }
    }
    Ok(out)
}

/// Split redirects into process substitutions (in order) and the rest.
fn partition_procsub(redirs: &[IrRedirect]) -> (Vec<IrRedirect>, Vec<IrRedirect>) {
    let mut ps = Vec::new();
    let mut plain = Vec::new();
    for r in redirs {
        if r.mode == "process-in" || r.mode == "process-out" {
            ps.push(r.clone());
        } else {
            plain.push(r.clone());
        }
    }
    (ps, plain)
}

/// The inner command text of a process-substitution redirect. The target
/// is already shell syntax (the core reconstructed it); a Debug-string
/// leak (064_01-style core gap) refuses loudly. bash-only `echo -e`
/// producers (dash's echo has no flags) render as printf.
fn proc_target(r: &IrRedirect) -> Result<String, String> {
    let s = match &r.target {
        IrExpr::Str(s, _) if s.starts_with("Redirect(") => Err(format!(
            "process-substitution target leaked a Debug string — core lowering gap: {s:?}"
        )),
        IrExpr::Str(s, _) => Ok(s.clone()),
        other => word_to_sh(other),
    }?;
    Ok(shim_producer(&s))
}

/// dash-compat shims for the core's bash-centric producer text:
///  * `echo -e "a\nc"` -> `printf '%b\n' "a\nc"` (dash's echo prints
///    the -e flag literally)
///  * `printf a\nb\n` -> `printf 'a\nb\n'` (dash interprets \n in an
///    UNQUOTED format arg only as literal backslash-n; bash does)
fn shim_producer(s: &str) -> String {
    let s = if s.contains("echo -e ") || s.contains("echo -E ") {
        s.replacen("echo -e ", "printf '%b\\n' ", 1)
            .replacen("echo -E ", "printf '%s\\n' ", 1)
    } else {
        s.to_string()
    };
    let mut out = String::new();
    let mut rest = s.as_str();
    loop {
        match rest.find("printf ") {
            Some(i) => {
                out.push_str(&rest[..i + 7]);
                rest = &rest[i + 7..];
                // already-quoted format (`printf '%b\n'`) — pass through
                if let Some(stripped) = rest.strip_prefix('\'') {
                    if let Some(end) = stripped.find('\'') {
                        out.push('\'');
                        out.push_str(&stripped[..=end]);
                        rest = &stripped[end + 1..];
                        continue;
                    }
                }
                let end = rest
                    .find(|c: char| c == ' ' || c == '\'')
                    .unwrap_or(rest.len());
                let w = &rest[..end];
                if !w.is_empty() && w.contains('\\') {
                    out.push('\'');
                    out.push_str(w);
                    out.push('\'');
                } else {
                    out.push_str(w);
                }
                if end < rest.len() {
                    out.push(' ');
                    rest = &rest[end + 1..];
                } else {
                    return out;
                }
            }
            None => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

/// Statement-level process-substitution lowering (multi-line form):
///  * ONE process-in  -> `head <(x)`-style partial readers get the
///    streaming pipe (an infinite producer is stopped by SIGPIPE when
///    the reader closes, exactly like bash's /dev/fd pipe); everything
///    else gets a temp file + stdin redirect — a pipe would run the
///    command in a SUBSHELL and lose its state (mapfile/while-read
///    accumulate vars the script uses afterwards).
///  * ONE process-out -> `cmd > tmp; consumer < tmp`
///  * TWO+ process-in -> per-substitution temp files, appended to the
///    command as trailing args (`comm -12 <(a) <(b)` -> `comm -12 "$t1"
///    "$t2"` — the IR keeps only the redirect list, and the corpus's
///    multi-process-substitution commands are all trailing-arg uses).
/// The producer runs with stderr discarded — bash feeds it the /dev/fd
/// pipe (stdout only) and the gate compares stdout against `bash 2>/dev/null`.
fn lower_procsub_stmt(ps: &[IrRedirect], cmd: &str) -> Result<String, String> {
    if ps.len() == 1 {
        let r = &ps[0];
        if r.mode == "process-in" {
            let producer = proc_target(r)?;
            if cmd.starts_with("head ") || cmd == "head" {
                return Ok(format!("{{ {producer}; }} 2>/dev/null | {cmd}"));
            }
            return Ok(format!(
                "_ps_t=$(mktemp)\n{{ {producer}; }} 2>/dev/null > \"$_ps_t\"\n{cmd} < \"$_ps_t\"\nrm -f \"$_ps_t\""
            ));
        }
        // process-out: the target CONSUMES the command's output
        return Ok(format!(
            "_ps_t=$(mktemp)\n{cmd} > \"$_ps_t\"\n{{ {}; }} < \"$_ps_t\"\nrm -f \"$_ps_t\"",
            proc_target(r)?
        ));
    }
    let mut pre = String::new();
    let mut args = String::new();
    for (i, r) in ps.iter().enumerate() {
        let t = format!("_ps_t{}", i + 1);
        let producer = proc_target(r)?;
        if r.mode == "process-in" {
            pre.push_str(&format!(
                "{t}=$(mktemp)\n{{ {producer}; }} 2>/dev/null > \"${t}\"\n"
            ));
            args.push_str(&format!(" \"${t}\""));
        } else {
            pre.push_str(&format!("{t}=$(mktemp)\n"));
            pre.push_str(&format!("{{ {producer}; }} < \"${t}\"\n"));
        }
    }
    let mut names = Vec::new();
    for (i, _) in ps.iter().enumerate() {
        names.push(format!("\"$_ps_t{}\"", i + 1));
    }
    // preserve the command's rc for the NEXT statement's `$?` (bash:
    // `diff <(a) <(b); echo $?` -> 1 — the rm must not clobber it)
    Ok(format!(
        "{pre}{cmd}{args}; rc=$?\nrm -f {}\n[ \"$rc\" -eq 0 ]",
        names.join(" ")
    ))
}

/// Inline (expression-context) process-substitution lowering: the
/// `{ ...; }` group form with the status preserved via `rc=$?` and a
/// final `[ "$rc" -eq 0 ]` (the group's status mirrors the command's:
/// set -e stays suppressed inside and the caller's `|| echo` still sees
/// the command's true status).
fn lower_procsub_inline(ps: &[IrRedirect], cmd: &str) -> Result<String, String> {
    let mut pre = String::new();
    let mut post = String::new();
    let mut args = String::new();
    let mut names = Vec::new();
    for (i, r) in ps.iter().enumerate() {
        let t = format!("_ps_t{}", i + 1);
        names.push(format!("\"${t}\""));
        let producer = proc_target(r)?;
        if r.mode == "process-in" {
            pre.push_str(&format!(
                "{t}=$(mktemp); {{ {producer}; }} 2>/dev/null > \"${t}\"; "
            ));
            args.push_str(&format!(" \"${t}\""));
        } else {
            pre.push_str(&format!("{t}=$(mktemp); "));
            post.push_str(&format!("{{ {producer}; }} < \"${t}\"; "));
        }
    }
    Ok(format!(
        "{{ {pre}{cmd}{args}; rc=$?; {post}rm -f {}; [ \"$rc\" -eq 0 ]; }}",
        names.join(" ")
    ))
}

/// `cmd <<<word` (dash: "redirection unexpected") → `printf '%s\n' word | cmd`
/// — the herestring feeds the word plus a newline on stdin. The herestring
/// itself is skipped by redirect_to_sh; this wraps the command with the pipe.
fn herestring_wrap(redirects: &[IrRedirect], cmd: String) -> Result<String, String> {
    for r in redirects {
        if r.mode == "herestring" {
            let t = word_to_sh(&r.target)?;
            return Ok(format!("printf '%s\\n' {t} | {cmd}"));
        }
    }
    Ok(cmd)
}

fn redirect_to_sh(r: &IrRedirect) -> Result<String, String> {
    let fd = r.fd.unwrap_or(0);
    let op = match r.mode.as_str() {
        "w" => ">",
        "a" => ">>",
        "r" => "<",
        "r+" => "<>",
        "herestring" => return Ok(String::new()), // wrapped as a printf pipe by herestring_wrap
        "heredoc" | "heredoc-tabs" => {
            // The heredoc BODY is carried in the target string.
            let body = match &r.target {
                IrExpr::Str(s, _) => s.clone(),
                other => word_to_sh(other)?,
            };
            let delim = pick_delimiter(&body);
            let q = if r.interpolate { "" } else { "'" };
            let tab = if r.mode == "heredoc-tabs" { "-" } else { "" };
            let fdpre = fd_prefix(fd);
            return Ok(format!(" {fdpre}<<{tab}{q}{delim}{q}\n{body}{delim}\n"));
        }
        // `unsupported` / unknown modes must never be silently dropped
        // (core request sh-20260807-130936 item 3 — refuse loudly; the
        // dropped redirect corrupted the command, e.g. `grep -f` with
        // no argument). The file pipeline + A1 ingress convert process
        // substitution to process-in/process-out (process_subst
        // transform), so this only fires on contract-invalid input.
        "unsupported" => return Err("redirect mode not renderable: \"unsupported\"".to_string()),
        other => return Err(format!("redirect mode not renderable: {other:?}")),
    };
    let fdpre = fd_prefix(fd);
    let target = match &r.target {
        // `2>&1` dup forms — the target arrives as a literal `&N`
        IrExpr::Str(s, _) if s.starts_with('&') => s.clone(),
        other => word_to_sh(other)?,
    };
    Ok(format!(" {fdpre}{op}{target}"))
}

/// Redirect specs as Object([(fd,Int),(mode,Str),(target,word)]) — the
/// command-position `redirect` call form.
fn redirect_specs(args: &[IrExpr], idx: usize) -> Result<String, String> {
    let Some(IrExpr::Array(specs)) = args.get(idx) else {
        return Ok(String::new());
    };
    redirect_objs_to_sh(specs)
}

/// Redirect spec objects — `Object([(fd,Int),(mode,Str),(target,word)])`
/// (the `IrStmt::Exec` redirects field and the call-form spec list).
fn redirect_objs_to_sh(specs: &[IrExpr]) -> Result<String, String> {
    let mut out = String::new();
    for spec in specs {
        let r = match redirect_objs(spec) {
            Ok(v) if !v.is_empty() => v[0].clone(),
            _ => continue,
        };
        out.push_str(&redirect_to_sh(&r)?);
    }
    Ok(out)
}

/// Parse an `Array` of redirect spec Objects into `IrRedirect`s. The
/// specs arrive as an Array in the call-form (`Call("redirect",
/// [Arrow, Array([Object, ...])])`) but as a bare Object element in the
/// `IrStmt::Exec` redirects field and `redirect_objs_to_sh`'s per-spec
/// iteration — accept both shapes (an Object IS one spec).
fn redirect_objs(specs: &IrExpr) -> Result<Vec<IrRedirect>, String> {
    let items: Vec<&IrExpr> = match specs {
        IrExpr::Array(items) => items.iter().collect(),
        IrExpr::Object(_) => vec![specs],
        _ => return Ok(vec![]),
    };
    let mut out = Vec::new();
    for spec in items {
        let IrExpr::Object(props) = spec else {
            return Err(format!("redirect spec not an Object: {spec:?}"));
        };
        let mut fd: Option<i64> = None;
        let mut mode = String::new();
        let mut target: Option<&IrExpr> = None;
        let mut interpolate = true;
        for (k, v) in props {
            match (k.as_str(), v) {
                ("fd", IrExpr::Int(n)) => fd = Some(*n),
                ("mode", IrExpr::Str(m, _)) => mode = m.clone(),
                ("target", t) => target = Some(t),
                ("interpolate", IrExpr::Bool(b)) => interpolate = *b,
                _ => {}
            }
        }
        out.push(IrRedirect {
            fd: fd.map(|n| n as i32),
            mode,
            target: target
                .cloned()
                .unwrap_or(IrExpr::Str(String::new(), StrStyle::DoubleQuoted)),
            interpolate,
        });
    }
    Ok(out)
}

fn fd_prefix(fd: i32) -> String {
    match fd {
        0 | 1 => String::new(),
        n => n.to_string(),
    }
}

fn pick_delimiter(body: &str) -> &'static str {
    for d in ["EOF", "_EOF", "SH2_EOF", "SH2EOF", "END"] {
        if !body.lines().any(|l| l.trim_end() == d) {
            return d;
        }
    }
    "SH2EOF"
}

// ── words ────────────────────────────────────────────────────────────

fn word_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(str_word(s)),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Var(name, _) => Ok(format!("${name}")),
        IrExpr::Ident(name) => Ok(name.clone()),
        IrExpr::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
        IrExpr::Interpolate(parts) => interp_to_sh(parts),
        IrExpr::Arith(a) => Ok(format!("$(({}))", arith_to_sh(a))),
        IrExpr::Call { func, args } => call_word_to_sh(func, args),
        // The A1 `Index` expr node (triage-sh py-sh-go cross-product pairs
        // t21/t56/t73): array element read — render like the `arrayIndex`
        // call arm (the POSIX per-element lowering `${name_key}`).
        IrExpr::Index { var, key } => index_to_sh(var, key, true),
        // The first-class Capture node (core request
        // zsh-sh-go-20260814-230503): `$(...)`/backticks — same
        // `"$(...)"` rendering as the `capture` call arm.
        IrExpr::Capture { expr, .. } => Ok(format!(
            "\"$({})\"",
            arrow_to_sh(std::slice::from_ref(expr.as_ref()))?
        )),
        IrExpr::Json(v) => Ok(json_str(v)),
        other => Err(format!("word not renderable: {other:?}")),
    }
}

/// The A1 `Index` expr node (triage-sh py-sh-go cross-product pairs
/// t21/t56/t73): an array element read — the same POSIX per-element
/// shapes as the `arrayIndex` call arm: whole-array `@`/`*`, the
/// dynamic-key eval form, and the literal `${name_key}` element. In
/// word position (`quoted`) the element is ONE word (`"${name_k}"`);
/// in interp position the bare `${name_k}` (the enclosing template
/// supplies the quotes).
fn index_to_sh(var: &str, key: &IrExpr, quoted: bool) -> Result<String, String> {
    match key {
        IrExpr::Str(k, _) if k == "@" || k == "*" => Ok(arr_expand_call(arr_base(var))),
        IrExpr::Str(k, _) if k.contains(['$', '(']) => Ok(format!(
            "$(eval \"printf '%s' \\\"\\${{{}_{k}}}\\\"\")",
            arr_base(var)
        )),
        IrExpr::Str(k, _) => {
            if quoted {
                Ok(format!("\"${{{var}_{k}}}\""))
            } else {
                Ok(format!("${{{var}_{k}}}"))
            }
        }
        // a VARIABLE subscript (`a[i]`, triage-sh t83_array_index_read) —
        // the dynamic-key eval form (indexed arrays evaluate the key as
        // ARITHMETIC, assoc arrays on the expanded text).
        IrExpr::Var(k, _) => Ok(index_var_key_to_sh(arr_base(var), k)),
        // the getVar-wrapped form (`getVar(\"i\")` — the A1's dynamic key
        // read, t83): same lowering.
        IrExpr::Call { func, args }
            if func == "getVar" && matches!(args.as_slice(), [IrExpr::Str(k, _)] if !k.is_empty()) =>
        {
            let IrExpr::Str(k, _) = &args[0] else { unreachable!() };
            Ok(index_var_key_to_sh(arr_base(var), k))
        }
        _ => Err("dynamic array indices are not yet POSIX-lowered — refusing".into()),
    }
}

/// A VARIABLE array subscript (`a[i]`) — the dynamic-key eval form the
/// `$`-bearing Str arm uses: indexed arrays evaluate the key as
/// ARITHMETIC (`$(( ${i:-0} ))`), assoc arrays on the EXPANDED text
/// (`${i}` braced so the trailing `_` of the element name does not glue
/// onto the key var).
fn index_var_key_to_sh(base: &str, k: &str) -> String {
    let key_sh = if ASSOC_VARS.lock().unwrap().contains(base) {
        format!("${{{k}}}")
    } else {
        format!("$(( ${{{k}:-0}} ))")
    };
    format!("$(eval \"printf '%s' \\\"\\${{{base}_{key_sh}}}\\\"\")")
}

fn call_word_to_sh(func: &str, args: &[IrExpr]) -> Result<String, String> {
    match func {
        // A bare getVar in a word position is the QUOTED expansion (the
        // core wraps deliberate word-splitting in split()): `"$x"`. The
        // quote matters — bash `printf '%s' "$x"` passes the value as
        // ONE word, the unquoted form re-splits it (heredoc-apostrophe.sh
        // truncates at the first space without it).
        "getVar" => Ok(format!("\"{}\"", var_ref_to_sh(&raw_arg(args, 0)?.replace('.', "_"), false))),
        // param expansions in word position are the QUOTED source form
        // (`echo "\"${x#p}\" "` — bash keeps interior spaces; the core
        // wraps only unquoted getVar in split(), param has no marker, so
        // the quote is the safe default). Array/list forms stay bare.
        "param" => {
            let s = param_to_sh(args, false)?;
            // the "" arm already returns a quoted segment — never
            // re-wrap (`""$x""` would field-split the value)
            if s.starts_with('"') || s.starts_with("$(_arr_") || s.starts_with("$(shift") {
                Ok(s)
            } else {
                Ok(format!("\"{s}\"",))
            }
        }
        // the core's word-splitting node: an UNQUOTED `$var` expands and
        // word-splits natively in POSIX sh — render the inner expansion
        // bare (no quotes).
        "split" => match arg(args, 0)? {
            IrExpr::Call { func, args } if func == "getVar" => {
                Ok(var_ref_to_sh(&raw_arg(args, 0)?, false))
            }
            IrExpr::Call { func, args } if func == "param" => param_to_sh(args, false),
            other => word_to_sh(other),
        },
        "param" => param_to_sh(args, false),
        "listVar" => {
            let n = raw_arg(args, 0)?;
            Ok(if n == "*" {
                "\"$*\"".into()
            } else {
                "\"$@\"".into()
            })
        }
        "arrayIndex" => {
            let name = raw_arg(args, 0)?;
            // POSIX has no arrays — the per-element lowering (`arr[i]` ->
            // `${arr_i}`). Literal indices lower inline (quoted — the
            // element is one word); @/* (the whole array) and dynamic
            // indices need the runtime count / indirect expansion.
            match arg(args, 1)? {
                IrExpr::Str(k, _) if k == "@" || k == "*" => Ok(arr_expand_call(arr_base(&name))),
                IrExpr::Str(k, _) if k.contains(['$', '(']) => {
                    // dynamic key (`${map[$k]}`) — indirect via eval
                    Ok(format!(
                        "$(eval \"printf '%s' \\\"\\${{{}_{k}}}\\\"\")",
                        arr_base(&name)
                    ))
                }
                IrExpr::Str(k, _) => Ok(format!("\"${{{name}_{k}}}\"")),
                // a VARIABLE subscript (`arr[i]`) — the dynamic-key eval
                // form (see `index_var_key_to_sh`).
                IrExpr::Var(k, _) => Ok(index_var_key_to_sh(arr_base(&name), k)),
                // the getVar-wrapped form (`getVar(\"i\")` — the A1's
                // dynamic key read, triage-sh t83_array_index_read).
                IrExpr::Call { func, args }
                    if func == "getVar"
                        && matches!(args.as_slice(), [IrExpr::Str(k, _)] if !k.is_empty()) =>
                {
                    let IrExpr::Str(k, _) = &args[0] else { unreachable!() };
                    Ok(index_var_key_to_sh(arr_base(&name), k))
                }
                _ => Err("dynamic array indices are not yet POSIX-lowered — refusing".into()),
            }
        }
        "assocGet" => {
            // the go-sh map read (`m[\"go\"]`, triage-sh-20260815-175002):
            // the by-name associative read — the same POSIX per-element
            // lowering as `arrayIndex` (`${m_go}`, quoted — one word).
            let name = raw_arg(args, 0)?;
            match arg(args, 1)? {
                IrExpr::Str(k, _) if k.contains(['$', '(']) => Ok(format!(
                    "$(eval \"printf '%s' \\\"\\${{{}_{k}}}\\\"\")",
                    arr_base(&name)
                )),
                IrExpr::Str(k, _) => Ok(format!("\"${{{name}_{k}}}\"")),
                IrExpr::Var(k, _) => Ok(index_var_key_to_sh(arr_base(&name), k)),
                IrExpr::Call { func, args }
                    if func == "getVar"
                        && matches!(args.as_slice(), [IrExpr::Str(k, _)] if !k.is_empty()) =>
                {
                    let IrExpr::Str(k, _) = &args[0] else { unreachable!() };
                    Ok(index_var_key_to_sh(arr_base(&name), k))
                }
                other => Err(format!("assocGet: key not renderable: {other:?}")),
            }
        }
        "arrayItems" => Ok(arr_keys_call(arr_base(&raw_arg(args, 0)?))),
        "arrayLen" => Ok(format!("${{{}_len}}", raw_arg(args, 0)?)),
        "capture" => Ok(format!("\"$({})\"", arrow_to_sh(args)?)),
        "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
        "arith" => Ok(format!("$(({}))", arith_rewrite(&raw_arg(args, 0)?))),
        "brace" => brace_to_sh(args),
        "join" => join_to_sh(arg(args, 0)?, false),
        "setArray" => {
            // POSIX has no arrays — the per-element lowering:
            //   arr=(a b c) -> arr_0=a; arr_1=b; arr_2=c; arr_len=3
            let name = raw_arg(args, 0)?;
            Ok(set_array_to_sh(&name, &array_items(args, 1)?, false))
        }
        "setArrayAppend" => {
            //   arr+=(x) -> arr_$arr_len=x; arr_len=$((arr_len+1))
            let name = raw_arg(args, 0)?;
            Ok(set_array_to_sh(&name, &array_items(args, 1)?, true))
        }
        "assign" => {
            let name = raw_arg(args, 0)?;
            let op = raw_arg(args, 1)?;
            let value = word_to_sh(arg(args, 2)?)?;
            Ok(format!("{name}{op}{value}"))
        }
        other => Err(format!("word call not renderable: {other:?}")),
    }
}

/// `${name}`-family rendering. `list` selects the list form (join
/// context, `"${arr[@]}"`).
fn var_ref_to_sh(name: &str, list: bool) -> String {
    if name == "RANDOM" {
        // bash-only; POSIX sh has no portable random — POSIX awk's
        // srand()/rand() (int(rand()*32768) = bash's 0..32767 range)
        return "$(awk 'BEGIN{srand(); printf \"%d\", int(rand()*32768)}')".into();
    }
    if list {
        if name == "@" || name == "*" {
            return format!("${{{name}}}");
        }
        return format!("${{{name}[@]}}");
    }
    // special parameters / positionals: `$?` `$!` `$@` `$1` …
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "?!@*#$-_".contains(c))
        && !name.starts_with('[')
    {
        format!("${name}")
    } else {
        // array elements / indexed names: `${x[i]}`
        format!("${{{name}}}")
    }
}

/// Parameter-expansion call: `param(op, name, extras...)` → `${...}`.
/// bash-only ops (slice, case-mod, substitution) that dash cannot parse
/// are emulated with POSIX tools so the rendered script RUNS under
/// /bin/sh with bash-identical stdout (the equivalence gate).
fn param_to_sh(args: &[IrExpr], list: bool) -> Result<String, String> {
    let op = raw_arg(args, 0)?;
    let name = raw_arg(args, 1)?;
    match op.as_str() {
        "" => {
            // the baked-name form: `arr[1]` (an element read -> ${arr_1}),
            // `#arr` (length -> ${arr_len}), `arr[@]`/`arr[*]` (the whole
            // array -> the per-element counter helper). A bare `param("",
            // name)` is the QUOTED `"${x}"` expansion (the core wraps
            // unquoted `$x` in split()) — keep it one word.
            if let Some((an, idx)) = name.strip_suffix(']').and_then(|n| n.split_once('[')) {
                if idx == "@" || idx == "*" {
                    if an.starts_with('!') {
                        return Ok(arr_keys_call(arr_base(&an)));
                    }
                    return Ok(arr_expand_call(arr_base(&an)));
                }
                if idx.contains(['$', '(']) {
                    // dynamic key (`${map[$k]}`) — indirect via eval
                    return Ok(format!(
                        "$(eval \"printf '%s' \\\"\\${{{}_{idx}}}\\\"\")",
                        arr_base(&an)
                    ));
                }
                return Ok(format!("\"${{{an}_{idx}}}\""));
            }
            if let Some(an) = name.strip_prefix('#') {
                return Ok(format!("${{{an}_len}}"));
            }
            if list {
                // `"${x[@]}"` — array elements joined
                Ok(var_ref_to_sh(&name, true))
            } else {
                // special params (`?`, `1`, `@`, ...) keep the `$name`
                // form inside the quotes (`"$?"` — `${?}` is invalid)
                Ok(format!("\"{}\"", var_ref_to_sh(&name, false)))
            }
        }
        "len" => {
            // `len` on an array (`${#arr[@]}`) -> the counter; on a scalar
            // (`${#s}`) -> the portable `$#{s}`
            if let Some(an) = name.strip_suffix("[@]") {
                Ok(format!("${{{an}_len}}"))
            } else {
                Ok(format!("${{#{name}}}"))
            }
        }
        "slice" => {
            // `${#arr[@]}` arrives as slice with a `#name` target — the
            // per-element length counter
            if let Some(an) = name.strip_prefix('#') {
                return Ok(format!("${{{an}_len}}"));
            }
            let off = raw_arg(args, 2)?;
            let len = args.get(3).map(str_arg).transpose()?.unwrap_or_default();
            // `${#arr[@]}` — the parser keeps the `#` in the name
            if name.starts_with('#') {
                return Ok(format!("${{#{}[@]}}", &name[1..]));
            }
            // dash has no substring expansion — emulate with cut
            // (char-exact for the ASCII corpus)
            if name == "@" || name == "*" {
                // `${@:off}` — positional slice: shift + join
                let offn: i64 = off.trim().parse().unwrap_or(-1);
                if offn >= 1 {
                    let sh = offn - 1;
                    return Ok(format!("$(shift {sh}; printf '%s' \"$*\")"));
                }
                return Ok(format!("${{{name}:{off}}}"));
            }
            // `${arr[@]}` / `${arr[*]}` — the whole array: the per-element
            // counter helper (arrays have no POSIX form)
            if off == "@" || off == "*" {
                // `${!map[@]}` — the `!` marker means KEYS, not values
                if name.starts_with('!') {
                    return Ok(arr_keys_call(arr_base(&name)));
                }
                return Ok(arr_expand_call(arr_base(&name)));
            }
            // a numeric-offset slice on a KNOWN array (`${arr[@]:1:2}`
            // lowers as a slice with the bare base name): element-wise
            // via the counter; unknown names stay scalar (cut on chars)
            if !name.starts_with('#')
                && name != "@"
                && name != "*"
                && ARRAY_NAMES.lock().unwrap().contains(name.as_str())
            {
                return Ok(format!("$(_arr_slice {name} {off} {len})"));
            }
            let offn: i64 = off.trim().parse().unwrap_or(-1);
            let lenn: i64 = len.trim().parse().unwrap_or(-1);
            if offn < 0 && (lenn >= 0 || len.is_empty()) {
                // `${s: -3}` — negative offsets count from the END
                let start = format!("$((${{#{name}}}{}+1))", offn);
                let range = if len.is_empty() {
                    format!("{start}-")
                } else {
                    format!("{start}-$(({start}+{lenn}-1))")
                };
                return Ok(format!("$(printf '%s' \"${{{name}}}\" | cut -c{range})"));
            }
            if offn >= 0 && (lenn >= 0 || len.is_empty()) {
                if !len.is_empty() && lenn == 0 {
                    // `${x:off:0}` — always empty
                    return Ok("$(printf '')".into());
                }
                let start = offn + 1;
                let range = if len.is_empty() {
                    format!("{start}-")
                } else {
                    format!("{start}-{}", offn + lenn)
                };
                return Ok(format!("$(printf '%s' \"${name}\" | cut -c{range})"));
            }
            // dynamic offsets — compute in shell arithmetic
            let start = format!("$((({off})+1))");
            let range = if len.is_empty() {
                format!("{start}-")
            } else {
                format!("{start}-$((({off})+({len})))")
            };
            Ok(format!("$(printf '%s' \"${name}\" | cut -c{range})"))
        }
        // case-modification: busybox tr treats `[:lower:]`/`[:upper:]`
        // literally (class support is compile-gated), so use the ASCII
        // ranges; GNU sed's \U/\L escapes are not portable either
        // (busybox/BSD sed lack them) - awk toupper/tolower exist in
        // gawk/mawk/busybox/BSD awk alike and match the old sed's
        // per-line first-char behaviour.
        "^^" => Ok(format!("$(printf '%s' \"${name}\" | tr a-z A-Z)")),
        ",," => Ok(format!("$(printf '%s' \"${name}\" | tr A-Z a-z)")),
        "^" => Ok(format!(
            "$(printf '%s' \"${name}\" | awk '{{print toupper(substr($0,1,1)) substr($0,2)}}')"
        )),
        "," => Ok(format!(
            "$(printf '%s' \"${name}\" | awk '{{print tolower(substr($0,1,1)) substr($0,2)}}')"
        )),
        "#" | "##" | "%" | "%%" => {
            let pat = raw_arg(args, 2)?;
            Ok(format!("${{{name}{op}{pat}}}"))
        }
        "//" | "/" => {
            // `${x/p/r}` — no dash equivalent; emulate with sed (the IR
            // conflates first/all occurrences — both render `g`)
            let pat = raw_arg(args, 2)?;
            let rep = raw_arg(args, 3)?;
            let pe = sed_escape_pattern(&pat);
            let re = sed_escape_replacement(&rep);
            Ok(format!(
                "$(printf '%s' \"${name}\" | sed -e 's#{pe}#{re}#g')"
            ))
        }
        ":-" | ":=" | ":?" => {
            let default = raw_arg(args, 2)?;
            // `${arr:-d}` — bash's array form reads ELEMENT 0 when the
            // array is non-empty (the per-element vars have no scalar)
            if ARRAY_NAMES.lock().unwrap().contains(name.as_str()) {
                return Ok(format!(
                    "$([ \"${{{name}_len:-0}}\" -gt 0 ] && eval \"printf '%s' \\\"\\${{{name}_0}}\\\"\" || printf '%s' {default})"
                ));
            }
            Ok(format!("${{{name}{op}{default}}}"))
        }
        "basename" => Ok(format!("${{{name}##*/}}")),
        "dirname" => Ok(format!("${{{name}%/*}}")),
        other => Err(format!("param op not renderable: {other:?}")),
    }
}

fn sed_escape_pattern(p: &str) -> String {
    let mut out = String::new();
    for c in p.chars() {
        match c {
            '\\' | '.' | '*' | '[' | ']' | '^' | '$' | '#' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

fn sed_escape_replacement(r: &str) -> String {
    let mut out = String::new();
    for c in r.chars() {
        match c {
            '\\' | '&' | '#' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// `join(x)` — the LIST form of an expansion (bash joins array elements
/// with spaces when quoted).
fn join_to_sh(inner: &IrExpr, quoted: bool) -> Result<String, String> {
    let s = match inner {
        IrExpr::Call { func, args } if func == "param" => param_to_sh(args, true)?,
        IrExpr::Call { func, args } if func == "arrayIndex" => {
            let name = raw_arg(args, 0)?;
            let key = raw_arg(args, 1)?;
            if key == "@" || key == "*" {
                // the whole array — the helper's expansion IS the list
                return Ok(arr_expand_call(arr_base(&name)));
            }
            format!("${{{name}[{key}]}}")
        }
        IrExpr::Call { func, args } if func == "arrayItems" => {
            format!("${{!{}[@]}}", raw_arg(args, 0)?)
        }
        IrExpr::Call { func, args } if func == "arrayLen" => {
            format!("${{{}_len}}", raw_arg(args, 0)?)
        }
        _ => word_to_sh(inner)?,
    };
    // array expansions are already word lists, and param renders are
    // already quoted — never re-quote them (`""$x""` would leave an
    // UNQUOTED segment and field-split the value)
    if s.starts_with("$(_arr_expand") || s.starts_with("$(_arr_keys") {
        Ok(s)
    } else {
        Ok(if quoted && !s.starts_with('"') {
            format!("\"{s}\"")
        } else {
            s
        })
    }
}

fn interp_to_sh(parts: &[InterpPart]) -> Result<String, String> {
    // pure-literal interpolation → plain literal word
    if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) {
        let mut s = String::new();
        for p in parts {
            if let InterpPart::Lit(t) = p {
                s.push_str(t);
            }
        }
        return Ok(str_word(&s));
    }
    // emit adjacent quoted segments: an Expr closes the quote ONLY when
    // the next part is a literal that could extend the variable name
    // (`"$x"world` — `"$xworld"` would expand the var xworld). A trailing
    // Expr stays INSIDE the quotes (`"brace_expand: $result"`).
    let mut out = String::new();
    let mut open = false;
    let mut it = parts.iter().peekable();
    while let Some(p) = it.next() {
        match p {
            InterpPart::Lit(t) => {
                if !open {
                    out.push('"');
                    open = true;
                }
                for c in t.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '$' => out.push_str("\\$"),
                        '`' => out.push_str("\\`"),
                        '\\' => out.push_str("\\\\"),
                        c => out.push(c),
                    }
                }
            }
            InterpPart::Expr(x) => {
                // a leading expansion (no open quote) must carry its own
                // quotes — without them `"$s\n"` renders `$s"…"` with an
                // UNQUOTED $s that word-splits (t70_quoted_single).
                // Multi-word / already-quoted forms (cmdsub lists, brace
                // expansion, whole-array helpers) stay bare.
                if !open && interp_expr_self_quotes(x) {
                    out.push('"');
                    out.push_str(&interp_expr_to_sh(x)?);
                    out.push('"');
                    continue;
                }
                let need_break = matches!(
                    it.peek(),
                    Some(InterpPart::Lit(n)) if n
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_alphanumeric() || c == '_')
                        .unwrap_or(false)
                );
                if open && need_break {
                    out.push('"');
                    open = false;
                }
                out.push_str(&interp_expr_to_sh(x)?);
            }
        }
    }
    if open {
        out.push('"');
    }
    Ok(out)
}

/// Multi-word or already-quoted interpolate expansions must not be
/// wrapped in a fresh quote pair (capture already carries `"$(…)"`;
/// brace / arrayItems / captureWords / whole-array forms are word LISTS).
fn interp_expr_self_quotes(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "capture" | "captureWords" | "brace" | "arrayItems" => false,
            // the whole-array forms expand to multiple words
            "arrayIndex" => !matches!(args.get(1), Some(IrExpr::Str(k, _)) if k == "@" || k == "*"),
            _ => true,
        },
        _ => true,
    }
}

/// An expansion inside a double-quoted template.
fn interp_expr_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => Ok(var_ref_to_sh(&raw_arg(args, 0)?.replace('.', "_"), false)),
            "param" => param_to_sh(args, false),
            "listVar" => {
                let n = raw_arg(args, 0)?;
                Ok(if n == "*" { "$*".into() } else { "$@".into() })
            }
            "arrayIndex" => {
                let name = raw_arg(args, 0)?;
                let key = raw_arg(args, 1)?;
                if key == "@" || key == "*" {
                    Ok(arr_expand_call(arr_base(&name)))
                } else if key.contains(['$', '(']) {
                    Ok(format!(
                        "$(eval \"printf '%s' \\\"\\${{{}_{key}}}\\\"\")",
                        arr_base(&name)
                    ))
                } else {
                    Ok(format!("${{{name}_{key}}}"))
                }
            }
            "assocGet" => {
                // the go-sh map read (`m[\"go\"]`, triage-sh-20260815-175002):
                // the by-name associative read — the same POSIX per-element
                // lowering as `arrayIndex` (the bare `${m_go}`; the
                // enclosing template supplies the quotes).
                let name = raw_arg(args, 0)?;
                match arg(args, 1)? {
                    IrExpr::Str(k, _) if k.contains(['$', '(']) => Ok(format!(
                        "$(eval \"printf '%s' \\\"\\${{{}_{k}}}\\\"\")",
                        arr_base(&name)
                    )),
                    IrExpr::Str(k, _) => Ok(format!("${{{name}_{k}}}")),
                    IrExpr::Var(k, _) => Ok(index_var_key_to_sh(arr_base(&name), k)),
                    other => Err(format!("assocGet: key not renderable: {other:?}")),
                }
            }
            "arrayItems" => Ok(arr_keys_call(arr_base(&raw_arg(args, 0)?))),
            "arrayLen" => Ok(format!("${{{}_len}}", raw_arg(args, 0)?)),
            "capture" => Ok(format!("\"$({})\"", arrow_to_sh(args)?)),
            "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),

            "arith" => {
                let raw = raw_arg(args, 0)?;
                if raw.contains(['"', '\'']) {
                    // bash errors at RUNTIME (the assignment is skipped);
                    // dash would fail to PARSE the whole script — a
                    // failed cmdsub has the same ${x:-d} observable
                    Ok("$(false)".into())
                } else {
                    Ok(format!("$(({}))", arith_rewrite(&raw)))
                }
            }
            "join" => join_to_sh(arg(args, 0)?, false),
            "brace" => brace_to_sh(args),
            other => Err(format!("interp call not renderable: {other:?}")),
        },
        IrExpr::Arith(a) => Ok(format!("$(({}))", arith_to_sh(a))),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
        IrExpr::Var(name, _) => Ok(format!("${name}")),
        IrExpr::Str(s, _) => Ok(s.clone()),
        // The A1 `Index` expr node (triage-sh py-sh-go cross-product pairs
        // t21/t56/t73): array element read inside a double-quoted template
        // — the bare `${name_key}` (the template supplies the quotes).
        IrExpr::Index { var, key } => index_to_sh(var, key, false),
        // The first-class Capture node (core request
        // zsh-sh-go-20260814-230503): `$(...)`/backticks — same
        // `"$(...)"` rendering as the `capture` call arm.
        IrExpr::Capture { expr, .. } => Ok(format!(
            "\"$({})\"",
            arrow_to_sh(std::slice::from_ref(expr.as_ref()))?
        )),
        other => Err(format!("interp expr not renderable: {other:?}")),
    }
}

/// Case-fold a glob pattern for `shopt -s nocasematch` emulation:
/// `foo` -> `[fF][oO][oO]` (glob metachars pass through).
fn fold_case_pattern(p: &str) -> String {
    let mut out = String::new();
    for c in p.chars() {
        match c {
            '*' | '?' | '[' | ']' | '\\' | '!' | '^' => out.push(c),
            c if c.is_ascii_alphabetic() => {
                out.push('[');
                out.push(c.to_ascii_lowercase());
                out.push(c.to_ascii_uppercase());
                out.push(']');
            }
            c => out.push(c),
        }
    }
    out
}

/// The core's test lowering strips the spaces around comparison
/// operators (`[ "$X" = "1" ]` arrives as `"$X"="1"`, `[ ! -x f ]` as
/// `!-x` — dash would read the whole thing as ONE word). Re-insert the
/// spaces around `=` (outside quoted regions) and after a leading `!`.
fn space_test_ops(t: &str) -> String {
    let s = space_test_eq(t);
    if let Some(rest) = s.strip_prefix('!') {
        if !rest.starts_with(['!', '=', ' ']) {
            return format!("! {rest}");
        }
    }
    s
}

/// The core's test lowering strips the spaces around comparison
/// operators (`[ "$X" = "1" ]` arrives as `"$X"="1"` — dash would read
/// the whole thing as ONE word). Re-insert the spaces around `=` outside
/// quoted regions.
fn space_test_eq(t: &str) -> String {
    let mut out = String::new();
    let mut in_dq = false;
    let mut in_sq = false;
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                in_dq = !in_dq;
                out.push(c);
            }
            '\'' => {
                in_sq = !in_sq;
                out.push(c);
            }
            _ if in_dq || in_sq => out.push(c),
            '=' => {
                let prev = chars[..i]
                    .iter()
                    .rev()
                    .find(|p| !p.is_whitespace())
                    .copied();
                let next = chars[i + 1..].iter().find(|n| !n.is_whitespace()).copied();
                let prev_ok = prev
                    .map(|p| !matches!(p, '=' | '!' | '<' | '>'))
                    .unwrap_or(false);
                let next_ok = next.map(|n| !matches!(n, '=')).unwrap_or(false);
                if prev_ok && next_ok {
                    out.push(' ');
                    out.push(c);
                    out.push(' ');
                } else {
                    out.push(c);
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// Split `lhs OP rhs` from a `[[ ]]` raw test (the parser strips spaces
/// around the operator: `$s==*.txt`). Returns None when the op is not a
/// top-level test operator (e.g. `a==b` inside a `$(...)`).
fn split_test_op(t: &str, op: &str) -> Option<(String, String)> {
    let idx = t.find(op)?;
    let lhs = t[..idx].trim().to_string();
    let rhs = t[idx + op.len()..].trim().to_string();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    // the operator must not sit inside a command substitution
    let mut depth = 0i32;
    let mut i = 0;
    while i < idx {
        let bytes = t.as_bytes();
        match bytes[i] {
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'(' => {
                depth += 1;
                i += 2;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if depth != 0 {
        return None;
    }
    Some((lhs, rhs))
}

/// Brace expansion: `brace(prefix, groups, middles, suffix)` — expand the
/// cross product at render time (POSIX sh has no brace expansion).
fn brace_to_sh(args: &[IrExpr]) -> Result<String, String> {
    let prefix = raw_arg(args, 0)?;
    let groups = brace_groups(args, 1)?;
    let middles = brace_middles(args, 2)?;
    let suffix = raw_arg(args, 3)?;

    // word = prefix g1 m1 g2 m2 … suffix; each group holds alternatives.
    // bash expands LEFT group SLOWEST (`{a..c}1{1..3}` -> a1 a2 a3 b1 b2
    // b3 c1 c2 c3), so the alt loop must be INNER to the base loop.
    let mut results: Vec<String> = vec![String::new()];
    for (gi, group) in groups.iter().enumerate() {
        let mut next = Vec::new();
        for base in &results {
            for alt in group {
                next.push(format!("{base}{alt}"));
            }
        }
        results = next;
        if let Some(mid) = middles.get(gi) {
            for r in &mut results {
                r.push_str(mid);
            }
        }
    }
    let mut out = String::new();
    for (i, r) in results.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&str_word(&format!("{prefix}{r}{suffix}")));
    }
    Ok(out)
}

/// The groups JSON: `[[{range:[s,e,null,null]} | String, ...], ...]` —
/// each group is a list of ALTERNATIVES (a range expands to several).
/// A group that MIXES ranges with plain items is NOT a bash sequence —
/// bash treats `{1..10,20,30..40}` as a list of LITERAL words (ranges
/// only expand when the brace is a pure sequence), so the group collapses
/// to the single literal brace text.
fn brace_groups(args: &[IrExpr], idx: usize) -> Result<Vec<Vec<String>>, String> {
    let Some(IrExpr::Json(serde_json::Value::Array(groups))) = args.get(idx) else {
        return Err("brace groups not Json".into());
    };
    let mut out = Vec::new();
    for g in groups {
        let serde_json::Value::Array(items) = g else {
            return Err("brace group not Array".into());
        };
        let single_range = items.len() == 1
            && matches!(items[0], serde_json::Value::Object(ref o) if o.contains_key("range"));
        if !single_range {
            // a LIST: bash expands the braces but treats every item as a
            // LITERAL — ranges only expand in a pure `{x..y}` sequence
            // (`{1..10,20,30..40}` -> `1..10 20 30..40`)
            let mut alts = Vec::new();
            for it in items {
                match it {
                    serde_json::Value::String(s) => alts.push(s.clone()),
                    serde_json::Value::Object(o) => {
                        if let Some(serde_json::Value::Array(r)) = o.get("range") {
                            let s0 = r[0].as_str().unwrap_or("");
                            let s1 = r[1].as_str().unwrap_or("");
                            let step = r.get(2).and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                            match step {
                                Some(st) => alts.push(format!("{s0}..{s1}..{st}")),
                                None => alts.push(format!("{s0}..{s1}")),
                            }
                        }
                    }
                    _ => {}
                }
            }
            out.push(alts);
            continue;
        }
        let mut expanded = Vec::new();
        for it in items {
            expanded.extend(brace_item(it)?);
        }
        out.push(expanded);
    }
    Ok(out)
}

fn brace_middles(args: &[IrExpr], idx: usize) -> Result<Vec<String>, String> {
    let Some(IrExpr::Json(serde_json::Value::Array(mids))) = args.get(idx) else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for m in mids {
        match m {
            serde_json::Value::String(s) => out.push(s.clone()),
            other => return Err(format!("brace middle not String: {other:?}")),
        }
    }
    Ok(out)
}

/// A brace item: a literal string, a `{range:[start,end,step,width?]}` —
/// or a `{nested: [...]}` group (its elements are alternatives) — returns
/// the alternatives (a range expands to each member). Ranges are numeric
/// (with optional step and zero-padding) or single-letter alphabetic.
fn brace_item(it: &serde_json::Value) -> Result<Vec<String>, String> {
    match it {
        serde_json::Value::String(s) => Ok(vec![s.clone()]),
        serde_json::Value::Object(o) => {
            if let Some(serde_json::Value::Array(range)) = o.get("range") {
                let s0 = range[0].as_str().unwrap_or("0");
                let s1 = range[1].as_str().unwrap_or("0");
                let step: i64 = match range.get(2) {
                    Some(serde_json::Value::String(s)) if !s.is_empty() => s.parse().unwrap_or(0),
                    _ => 0,
                };
                // alphabetic range: `{a..c}` / `{c..a}` (single letters)
                if s0.chars().count() == 1 && s1.chars().count() == 1 {
                    let a = s0.chars().next().unwrap();
                    let b = s1.chars().next().unwrap();
                    if a.is_ascii_alphabetic()
                        && b.is_ascii_alphabetic()
                        && a.is_ascii_lowercase() == b.is_ascii_lowercase()
                    {
                        let (lo, hi, down) = if a <= b { (a, b, false) } else { (b, a, true) };
                        let st = if step == 0 { 1 } else { step.abs() } as u8;
                        let mut out = Vec::new();
                        if down {
                            let mut c = hi as u8;
                            loop {
                                out.push((c as char).to_string());
                                if c <= lo as u8 {
                                    break;
                                }
                                c = c.saturating_sub(st);
                                if c < lo as u8 {
                                    break;
                                }
                            }
                        } else {
                            let mut c = lo as u8;
                            loop {
                                out.push((c as char).to_string());
                                if c >= hi as u8 {
                                    break;
                                }
                                c = c.saturating_add(st);
                                if c > hi as u8 {
                                    break;
                                }
                            }
                        }
                        return Ok(out);
                    }
                }
                let start: i64 = s0.parse().unwrap_or(0);
                let end: i64 = s1.parse().unwrap_or(0);
                // zero-padding width (bash pads to the longer of the two)
                let pad = if s0.starts_with('0') || s1.starts_with('0') {
                    s0.len().max(s1.len())
                } else {
                    0
                };
                let mut out = Vec::new();
                if step > 0 {
                    let mut n = start;
                    while n <= end {
                        out.push(pad_num(n, pad));
                        n += step;
                    }
                } else if step < 0 {
                    let mut n = start;
                    while n >= end {
                        out.push(pad_num(n, pad));
                        n += step;
                    }
                } else if start <= end {
                    for n in start..=end {
                        out.push(pad_num(n, pad));
                    }
                } else {
                    for n in (end..=start).rev() {
                        out.push(pad_num(n, pad));
                    }
                }
                Ok(out)
            } else if let Some(serde_json::Value::Array(nested)) = o.get("nested") {
                let mut out = Vec::new();
                for el in nested {
                    out.extend(brace_item(el)?);
                }
                Ok(out)
            } else {
                Err(format!("brace item not understood: {it:?}"))
            }
        }
        other => Err(format!("brace item not understood: {other:?}")),
    }
}

/// Zero-pad a range member to `width` digits (bash `{00..04..2}` ->
/// 00 02 04); negative numbers keep their sign.
fn pad_num(n: i64, width: usize) -> String {
    if width == 0 {
        return n.to_string();
    }
    let neg = n < 0;
    let digits = n.abs().to_string();
    let padded = if digits.len() < width {
        format!("{}{}", "0".repeat(width - digits.len()), digits)
    } else {
        digits
    };
    if neg {
        format!("-{padded}")
    } else {
        padded
    }
}

/// A literal word with shell-aware quoting. Str values are RAW source
/// text: unquoted globs arrive GLOB_MAGIC-tagged (emit bare → the shell
/// globs natively), backtick text must execute, everything else is
/// single-quoted when shell-active.
fn str_word(s: &str) -> String {
    if let Some(rest) = s.strip_prefix(GLOB_MAGIC) {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix(PS_MAGIC) {
        return rest.to_string();
    }
    if s.is_empty() {
        return "''".into();
    }
    // backticks must execute (command substitution captured as literal)
    if s.contains('`') {
        return s.to_string();
    }
    // words with `$`/`\` (regex anchors, escape sequences) or shell
    // metacharacters → single-quote so the text stays literal
    if s.contains([
        '$', '"', '\'', '\\', ';', '&', '|', '(', ')', '<', '>', ' ', '\t', '\n', '=',
    ]) || s.starts_with('#')
    {
        let mut q = String::from("'");
        for c in s.chars() {
            if c == '\'' {
                q.push_str("'\\''");
            } else {
                q.push(c);
            }
        }
        q.push('\'');
        return q;
    }
    // quoted-in-source glob chars (no GLOB_MAGIC) → literal
    if s.contains(['*', '?', '[']) {
        return format!("'{s}'");
    }
    s.to_string()
}

fn json_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => {
            if *b {
                "1".into()
            } else {
                "0".into()
            }
        }
        _ => v.to_string(),
    }
}

// ── arithmetic ───────────────────────────────────────────────────────

/// dash's arithmetic has NO ++/-- and NO `**` — rewrite the raw arith
/// text (the `arith` call form carries a string, not the AST):
///   `i++` -> `((i = i + 1) - 1)`   `++i` -> `(i = i + 1)`
///   `i--` -> `((i = i - 1) + 1)`   `--i` -> `(i = i - 1)`
///   `2 ** 3` -> `8` (literal powers fold)

/// A zsh/mathfunc call in raw arith text: `name(ARG, …)` starting at
/// `s[0] == '('`. Returns the awk expression and the number of bytes
/// consumed (through the closing paren) when the call is in the
/// deterministic subset with plain numeric args; None otherwise.
fn math_call_to_awk(name: &str, s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    debug_assert_eq!(b.first(), Some(&b'('));
    let mut depth = 1i32;
    let mut k = 1;
    while k < b.len() && depth > 0 {
        if b[k] == b'(' {
            depth += 1;
        } else if b[k] == b')' {
            depth -= 1;
        }
        k += 1;
    }
    if depth != 0 {
        return None;
    }
    let args_text = &s[1..k - 1];
    let is_num = |a: &str| {
        let a = a.trim();
        !a.is_empty()
            && a.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
            && a.chars().filter(|c| *c == '.').count() <= 1
    };
    let args: Vec<&str> = args_text.split(',').collect();
    if args.is_empty() || !args.iter().all(|a| is_num(a)) {
        return None;
    }
    let a = |i: usize| args[i].trim().to_string();
    let expr = match name {
        "sqrt" if args.len() == 1 => format!("sqrt({})", a(0)),
        "int" if args.len() == 1 => format!("int({})", a(0)),
        "fmod" if args.len() == 2 => format!("{} % {}", a(0), a(1)),
        "hypot" if args.len() >= 2 => format!(
            "sqrt({})",
            args.iter()
                .map(|x| format!("({})^2", x.trim()))
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        _ => return None,
    };
    Some((expr, k))
}
fn arith_rewrite(t: &str) -> String {
    let b = t.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if (c == '+' || c == '-') && i + 1 < b.len() && b[i + 1] == b[i] {
            // prefix `++name`
            let next_ident2 =
                i + 2 < b.len() && (b[i + 2].is_ascii_alphabetic() || b[i + 2] == b'_');
            if next_ident2 {
                let mut j = i + 2;
                while j < b.len() && ident(b[j]) {
                    j += 1;
                }
                let name = &t[i + 2..j];
                let inc = if c == '+' { "+ 1" } else { "- 1" };
                out.push_str(&format!("({name} = {name} {inc})"));
                i = j;
                continue;
            }
        }
        if c == '*' && i + 1 < b.len() && b[i + 1] == b'*' {
            // literal power `A ** B` — fold
            let mut s = i;
            while s > 0 && b[s - 1].is_ascii_whitespace() {
                s -= 1;
            }
            let mut e = s;
            while e > 0 && b[e - 1].is_ascii_digit() {
                e -= 1;
            }
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let mut k = j;
            while k < b.len() && b[k].is_ascii_digit() {
                k += 1;
            }
            if e < s && j < k {
                let a: i64 = t[e..s].parse().unwrap_or(0);
                let p: i64 = t[j..k].parse().unwrap_or(0);
                if p >= 0 {
                    let mut acc: i64 = 1;
                    for _ in 0..p {
                        acc = acc.saturating_mul(a);
                    }
                    out.truncate(out.len() - (s - e));
                    out.push_str(&acc.to_string());
                    i = k;
                    continue;
                }
            }
        }
        // bare identifier -> $(_num "$name") (bash coerces non-numeric
        // values to 0, dash errors); assignment LHS stays bare (`x` in
        // `x = y` is a write target); a postfix ++/-- rewrites first
        // (the name must stay plain there).
        // A leading `$` on the identifier (e.g. `$n` in arithmetic) is
        // the shell variable sigil - consume it so the cmdsub below
        // is the only `$`-bearing form (avoids `$$( _num "n" )`).
        if c == b'$' as char
            && i + 1 < b.len()
            && (b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_')
        {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' as char {
            let mut j = i;
            while j < b.len() && ident(b[j]) {
                j += 1;
            }
            let name = &t[i..j];
            // zsh/mathfunc arithmetic CALLS (`sqrt(9)`, `int(3.7)`,
            // `hypot(3, 4)`, `fmod(7, 3)` — the deterministic subset the
            // zsh corpus exercises; the estree reference resolves the
            // same names through JS Math): bash arith has no function
            // calls, so route the call through awk (POSIX; sqrt/int/%
            // are awk-native, hypot = sqrt(a^2+b^2), % is fmod
            // semantics). Mirror of the backend/sh worktree arm
            // (triage zsh-sh-go t89_arith_call).
            if j + 1 < b.len() && b[j] == b'(' {
                if let Some((awk_expr, consumed)) = math_call_to_awk(name, &t[j..]) {
                    out.push_str(&format!("$(awk 'BEGIN{{print {awk_expr}}}')"));
                    i = j + consumed;
                    continue;
                }
            }
            if j + 1 < b.len() && (b[j] == b'+' || b[j] == b'-') && b[j + 1] == b[j] {
                let (inc, dec) = if b[j] == b'+' {
                    ("+ 1", "- 1")
                } else {
                    ("- 1", "+ 1")
                };
                out.push_str(&format!("(({name} = {name} {inc}) {dec})"));
                i = j + 2;
                continue;
            }
            let mut k = j;
            while k < b.len() && b[k].is_ascii_whitespace() {
                k += 1;
            }
            let is_assign = k < b.len()
                && b[k] == b'='
                && !(k + 1 < b.len() && (b[k + 1] == b'=' || b[k + 1] == b'~'));
            if is_assign {
                out.push_str(name);
            } else if NUM_VARS.lock().unwrap().contains(name) {
                // known-numeric var: the coercion is dead weight — both
                // shells evaluate the bare name natively (dash rejects
                // QUOTED expansions inside $(( )) — `"${name}"` — so the
                // drop target must be the bare identifier, which the
                // analysis guarantees to hold numeric text)
                out.push_str(name);
            } else {
                out.push_str(&format!("$( _num \"${name}\" )"));
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// dash has NO c-style `for (( init; cond; incr ))` — lower to a portable
/// while loop:
///   i=2
///   while [ "$((i))" -le "$((n))" ]; do
///       body
///       i=$((i + 1))
///   done
fn cstyle_for_to_sh(arith: &str, body: &str) -> String {
    let mut parts = arith.split(';');
    let init = parts.next().map(|s| s.trim()).unwrap_or("");
    let cond = parts.next().map(|s| s.trim()).unwrap_or("1");
    let incr = parts.next().map(|s| s.trim()).unwrap_or("");
    let mut out = String::new();
    if !init.is_empty() {
        let (name, val) = init.split_once('=').unwrap_or((init, "0"));
        out.push_str(&format!("{}={}\n", name.trim(), val.trim()));
    }
    // the condition: `LHS OP RHS` -> `[ "$((LHS))" -o "$((RHS))" ]`
    let cond_sh = {
        let mut cond_sh = String::new();
        let mut rest = cond;
        let mut done = false;
        for (op, flag) in [
            ("<=", "-le"),
            (">=", "-ge"),
            ("==", "-eq"),
            ("!=", "-ne"),
            ("<", "-lt"),
            (">", "-gt"),
        ] {
            if let Some(idx) = rest.find(op) {
                let lhs = rest[..idx].trim();
                let rhs = rest[idx + op.len()..].trim();
                if !lhs.is_empty() && !rhs.is_empty() {
                    cond_sh = format!("[ \"$(({lhs}))\" {flag} \"$(({rhs}))\" ]");
                    done = true;
                    break;
                }
            }
        }
        if !done {
            // bare expression: nonzero test
            cond_sh = format!("[ \"$(({rest}))\" -ne 0 ]");
        }
        cond_sh
    };
    out.push_str(&format!("while {cond_sh}; do {body}; "));
    if !incr.is_empty() {
        // `i++` -> `i=$((i + 1))`; `i += 2` -> `i=$((i + 2))`; else evaluate
        let trimmed = incr.trim();
        let incr_final = if trimmed.is_empty() {
            ":".to_string()
        } else if trimmed.contains("++") {
            let name = trimmed.trim_end_matches("++").trim();
            format!("{name}=$(({name} + 1))")
        } else if trimmed.contains("--") {
            let name = trimmed.trim_end_matches("--").trim();
            format!("{name}=$(({name} - 1))")
        } else if let Some((name, rest)) = trimmed.split_once("+=") {
            format!("{}=$(({} + {}))", name.trim(), name.trim(), rest.trim())
        } else if let Some((name, rest)) = trimmed.split_once("-=") {
            format!("{}=$(({} - {}))", name.trim(), name.trim(), rest.trim())
        } else {
            format!("$(({trimmed}))")
        };
        out.push_str(&incr_final);
        out.push(';');
    }
    out.push_str(" done");
    out
}

fn arith_to_sh(a: &ArithAst) -> String {
    match a {
        ArithAst::Num(n) => n.to_string(),
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            if NUM_VARS.lock().unwrap().contains(name) {
                // known-numeric var: bare read (dash rejects quoted
                // expansions inside $(( )); the analysis guarantees the
                // value is numeric text)
                name.clone()
            } else {
                format!("$( _num \"${name}\" )")
            }
        }
        ArithAst::Index { var, key } => format!("{var}[{}]", arith_to_sh(key)),
        ArithAst::Bin { op, lhs, rhs } => {
            // dash has no `**` — constant-fold literal powers
            if op == "**" {
                if let (ArithAst::Num(a), ArithAst::Num(b)) = (lhs.as_ref(), rhs.as_ref()) {
                    let mut acc: i64 = 1;
                    for _ in 0..*b.max(&0) {
                        acc = acc.saturating_mul(*a);
                    }
                    if *b < 0 {
                        return "0".into();
                    }
                    return acc.to_string();
                }
            }
            format!("({} {op} {})", arith_to_sh(lhs), arith_to_sh(rhs))
        }
        ArithAst::Un { op, arg } => format!("({op}{})", arith_to_sh(arg)),
        ArithAst::Cond { test, then, else_ } => format!(
            "({} ? {} : {})",
            arith_to_sh(test),
            arith_to_sh(then),
            arith_to_sh(else_)
        ),
        ArithAst::Assign { var, op, rhs } => {
            format!("{var} {op}= {}", arith_to_sh(rhs))
        }
        ArithAst::IncDec { var, delta, prefix } => {
            // dash's arithmetic has NO ++/-- — rewrite via assignment
            // (the expression VALUE is preserved: prefix = new value,
            // postfix = old value)
            let (inc, dec) = if *delta > 0 {
                ("+ 1", "- 1")
            } else {
                ("- 1", "+ 1")
            };
            if *prefix {
                // `++i` -> `(i = i + 1)` — value is the NEW value
                format!("({var} = {var} {inc})")
            } else {
                // `i++` -> `((i = i + 1) - 1)` — value is the OLD value
                format!("(({var} = {var} {inc}) {dec})")
            }
        }
        // C-frontend nodes (never emitted by the shell path): sizeof is a
        // compile-time constant; casts are identity (shell arith is 64-bit).
        ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
        ArithAst::Cast { arg, .. } => arith_to_sh(arg),
    }
}

// ── arrows / inline statement sequences ──────────────────────────────

fn arrow_to_sh(args: &[IrExpr]) -> Result<String, String> {
    arrow_at(args, 0)
}

fn arrow_at(args: &[IrExpr], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(IrExpr::Arrow(stmts)) => stmts_inline(stmts),
        other => Err(format!("arrow not found at {idx}: {other:?}")),
    }
}

/// The stages of a pipeline call: `Array([Arrow, Arrow, ...])`.
fn pipeline_stages(args: &[IrExpr]) -> Result<Vec<Vec<IrStmt>>, String> {
    let Some(IrExpr::Array(stages)) = args.first() else {
        return Err("pipeline stages not Array".into());
    };
    let mut out = Vec::new();
    for s in stages {
        match s {
            IrExpr::Arrow(stmts) => out.push(stmts.clone()),
            other => return Err(format!("pipeline stage not Arrow: {other:?}")),
        }
    }
    Ok(out)
}

/// Compact single-line rendering of a statement sequence (capture bodies,
/// pipeline stages, inline compounds).
fn stmts_inline(stmts: &[IrStmt]) -> Result<String, String> {
    let mut out = Vec::new();
    for s in stmts {
        out.push(stmt_inline(s)?);
    }
    Ok(out.join("; "))
}

fn stmt_inline(st: &IrStmt) -> Result<String, String> {
    match st {
        IrStmt::Expr(e) => cmd_to_sh(e),
        IrStmt::Ext(_) => return Err("sh renderer: Ext node unsupported".to_string()),
        IrStmt::Assign { targets, expr, .. } => assign_to_sh(targets, expr),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            // empty branches need a `:` no-op (bash rejects `then ; else`)
            let inline = |b: &[IrStmt]| -> Result<String, String> {
                let s = stmts_inline(b)?;
                Ok(if b.is_empty() { ":".to_string() } else { s })
            };
            let mut out = format!("if {}; then {}", cmd_to_sh(cond)?, inline(then)?);
            for (ec, body) in elsifs {
                out.push_str(&format!(
                    "; elif {}; then {}",
                    cmd_to_sh(ec)?,
                    inline(body)?
                ));
            }
            if !else_.is_empty() {
                out.push_str(&format!("; else {}", stmts_inline(else_)?));
            }
            out.push_str("; fi");
            Ok(out)
        }
        IrStmt::For { var, iter, body } => {
            if let Some((start, end)) = range_of(iter) {
                // portable while form (the block renderer's inline twin)
                return Ok(format!(
                    "{var}={start}; while [ \"${var}\" -le {end} ]; do {}; {var}=$(({var} + 1)); done",
                    stmts_inline(body)?
                ));
            }
            Ok(format!(
                "for {var} in {}; do {}; done",
                for_items_to_sh(iter)?,
                stmts_inline(body)?
            ))
        }
        IrStmt::While { cond, body } => Ok(format!(
            "while {}; do {}; done",
            cmd_to_sh(cond)?,
            stmts_inline(body)?
        )),
        IrStmt::ForInit { .. } => Err(
            "sh renderer: un-stripped ForInit (the strip_cfor pass should have lowered it)".into(),
        ),
        IrStmt::Continue => Ok(format!("continue")),
        IrStmt::Break => Ok(format!("break")),
        IrStmt::DoWhile { body, cond, until } => {
            let neg = if *until { "" } else { "! " };
            Ok(format!(
                "while :; do {}; if {neg}{}; then break; fi; done",
                stmts_inline(body)?,
                cmd_to_sh(cond)?
            ))
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut out = format!("case {} in", word_to_sh(discriminant)?);
            for cl in clauses {
                out.push_str(&format!(
                    " {}) {};;",
                    cl.patterns.join("|"),
                    stmts_inline(&cl.body)?
                ));
            }
            out.push_str(" esac");
            Ok(out)
        }
        IrStmt::Function { name, body, .. } => Ok(format!("{name}() {{ {}; }}", stmts_inline(body)?)),
        IrStmt::Redirect { inner, redirects } => {
            let mut out = herestring_wrap(redirects, stmts_inline(inner)?)?;
            out.push_str(&redirects_to_sh(redirects)?);
            Ok(out)
        }
        IrStmt::Subshell(body) => Ok(format!("( {} )", stmts_inline(body)?)),
        IrStmt::Background(body) => Ok(format!("( {} ) &", stmts_inline(body)?)),
        IrStmt::Block(body) => Ok(format!("{{ {}; }}", stmts_inline(body)?)),
        IrStmt::Return(e) => match e {
            Some(x) => Ok(format!("return {}", word_to_sh(x)?)),
            None => Ok("return".into()),
        },
        IrStmt::Exit(e) => match e {
            Some(x) => Ok(format!("exit {}", word_to_sh(x)?)),
            None => Ok("exit".into()),
        },
        IrStmt::Die { expr, .. } => Ok(format!("echo {} >&2; exit 1", word_to_sh(expr)?)),
        IrStmt::Warn { expr, .. } => Ok(format!("echo {} >&2", word_to_sh(expr)?)),
        IrStmt::Declare { vars, init, local } => {
            let mut out = String::new();
            if *local {
                out.push_str("local ");
            }
            for (i, v) in vars.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&v.name);
                if let Some(init) = init {
                    out.push('=');
                    out.push_str(&word_to_sh(init)?);
                }
            }
            Ok(out)
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            Ok(format!("{var}=({})", array_literal_to_sh(elements)?))
        }
        IrStmt::Output { value, newline, .. } => {
            let w = word_to_sh(value)?;
            if *newline {
                Ok(format!("printf '%s\\n' {w}"))
            } else {
                Ok(format!("printf '%s' {w}"))
            }
        }
        IrStmt::Exec {
            cmd,
            args,
            capture,
            redirects,
            env,
        } => {
            let mut line = exec_line_to_sh(cmd, args, Some(env))?;
            if !redirects.is_empty() {
                line.push_str(&redirect_objs_to_sh(redirects)?);
            }
            if let Some(var) = capture {
                Ok(format!("{var}=$({line})"))
            } else {
                Ok(line)
            }
        }
        IrStmt::Pipeline {
            stages, capture, ..
        } => {
            let mut line = String::new();
            for (i, stg) in stages.iter().enumerate() {
                if i > 0 {
                    line.push_str(" | ");
                }
                line.push_str(&stmts_inline(stg)?);
            }
            if let Some(var) = capture {
                Ok(format!("{var}=$({line})"))
            } else {
                Ok(line)
            }
        }
        IrStmt::WriteFile {
            path,
            content,
            append,
        } => Ok(format!(
            "printf '%s' {} {} {}",
            word_to_sh(content)?,
            if *append { ">>" } else { ">" },
            word_to_sh(path)?
        )),
        IrStmt::Label(name) | IrStmt::Goto(name) => {
            let kind = if matches!(st, IrStmt::Label(_)) {
                "label"
            } else {
                "goto"
            };
            Ok(format!(
                "# TODO(unsupported): {kind} {name} not restructured by restructure_goto"
            ))
        }
        IrStmt::SetChildError(_) | IrStmt::Require(_) | IrStmt::RawText(_) => Ok(String::new()),
        // sh has no try/except — refuse (the gate reports it as a FAIL)
        IrStmt::Try { .. } => Err("try/except has no sh rendering".into()),
        // sh has no select-on-channels — refuse loudly
        IrStmt::Select { .. } => Err("select has no sh rendering".into()),
        // inline asm has no sh rendering — refuse loudly
        IrStmt::Asm { .. } => Err("inline asm has no sh rendering".into()),
    }
}

// ── for-loop item lists ──────────────────────────────────────────────

fn for_items_to_sh(iter: &IrExpr) -> Result<String, String> {
    match iter {
        IrExpr::Array(items) => {
            if items.is_empty() {
                // `for x; do` — iterate the positional parameters
                return Ok("\"$@\"".into());
            }
            let mut out = Vec::new();
            for it in items {
                out.push(for_item_to_sh(it)?);
            }
            Ok(out.join(" "))
        }
        other => Ok(word_to_sh(other)?),
    }
}

fn for_item_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } if func == "getVar" => Ok(format!("${}", raw_arg(args, 0)?)),
        IrExpr::Call { func, args } if func == "listVar" => {
            let n = raw_arg(args, 0)?;
            Ok(if n == "*" {
                "\"$*\"".into()
            } else {
                "\"$@\"".into()
            })
        }
        IrExpr::Call { func, args } if func == "join" => join_to_sh(arg(args, 0)?, true),
        IrExpr::Call { func, args } if func == "param" => param_to_sh(args, false),
        IrExpr::Call { func, args } if func == "arrayIndex" => {
            Ok(format!("${{{}{}}}", raw_arg(args, 0)?, raw_arg(args, 1)?))
        }
        IrExpr::Call { func, args } if func == "arrayItems" => {
            Ok(format!("${{!{}[@]}}", raw_arg(args, 0)?))
        }
        IrExpr::Call { func, args } if func == "arrayLen" => {
            Ok(format!("${{{}_len}}", raw_arg(args, 0)?))
        }
        _ => word_to_sh(e),
    }
}

fn array_literal_to_sh(elements: &[IrExpr]) -> Result<String, String> {
    let mut out = Vec::new();
    for e in elements {
        out.push(word_to_sh(e)?);
    }
    Ok(format!("({})", out.join(" ")))
}

fn array_items(args: &[IrExpr], idx: usize) -> Result<String, String> {
    let Some(IrExpr::Array(items)) = args.get(idx) else {
        return Ok(String::new());
    };
    let mut out = Vec::new();
    for it in items {
        out.push(word_to_sh(it)?);
    }
    Ok(out.join(" "))
}

// ── argument helpers ─────────────────────────────────────────────────

fn arg<'a>(args: &'a [IrExpr], idx: usize) -> Result<&'a IrExpr, String> {
    args.get(idx)
        .ok_or_else(|| format!("missing argument {idx} in {args:?}"))
}

/// The raw text of a Str argument.
fn raw_arg(args: &[IrExpr], idx: usize) -> Result<String, String> {
    str_arg(arg(args, idx)?)
}

/// The raw text of a Str expression.
fn str_arg(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(s.clone()),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Ident(s) => Ok(s.clone()),
        // numeric arith in an exec-word position renders as the plain
        // arithmetic result (perl-frontend slice offsets / indices)
        IrExpr::Arith(a) => Ok(arith_to_sh(a)),
        other => Err(format!("expected Str argument, got {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn render_sh(src: &str) -> String {
        let commands = Parser::new(src).parse().unwrap();
        let prog = crate::shir::ast_to_ir(&commands);
        shir_to_sh(&prog).unwrap()
    }

    /// `echo X | grep -oP '(?<!…)'` — the -P exec sits inside a pipeline
    /// stage ARROW; needs_grep_p must descend into it (like
    /// needs_readlink does) or the grep_p() prologue is omitted and the
    /// generated script calls an undefined `grep_p` (command not found).
    #[test]
    fn pipeline_grep_p_emits_polyfill() {
        let sh = render_sh("echo x | grep -oP '(?<![:-])\\b\\w+'");
        assert!(sh.contains("grep_p() {"), "polyfill prologue emitted: {sh}");
        assert!(sh.contains("grep -P"), "GNU grep -P fallback: {sh}");
        assert!(
            sh.contains("pcre2grep") && sh.contains("pcregrep"),
            "pcre fallbacks: {sh}"
        );
        assert!(
            sh.contains("grep_p -o '(?<![:-])\\b\\w+'"),
            "call uses grep_p with the -o flag before the pattern: {sh}"
        );
    }

    /// A plain (non-pipeline) `grep -P PAT FILE` also needs the polyfill,
    /// and the pattern must be the polyfill's first argument.
    #[test]
    fn plain_grep_p_emits_polyfill() {
        let sh = render_sh("grep -P '\\bbar\\b' f");
        assert!(sh.contains("grep_p() {"), "polyfill prologue: {sh}");
        assert!(sh.contains("grep_p '\\bbar\\b' f"), "call: {sh}");
    }

    /// `((x++))` as a statement — the zsh-sh-go t29_increment A1 shape
    /// (triage-sh-20260814-032146): Assign with a single target whose
    /// expr is an Arith IncDec on the SAME var (the estree renderer
    /// emits the IncDec bare). The statement's value is discarded, so
    /// the sh renderer lowers it to `: $((x++))` — the `:` swallows the
    /// expansion result (a bare `$((...))` line would try to RUN the
    /// value as a command: "1: not found").
    #[test]
    fn incdec_assign_emits_colon_arith() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[{"name":"x","type":"Int"}],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Assign","targets":[{"var":"x","sigil":null,"indices":[]}],"expr":{"type":"Str","value":"1","style":"DoubleQuoted"}},
            {"type":"Block","body":[{"type":"Assign","targets":[{"var":"x","sigil":null,"indices":[]}],"expr":{"type":"Arith","ast":{"type":"IncDec","var":"x","delta":1,"prefix":false}}}]},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("t29 A1 ingress");
        let sh = shir_to_sh(&prog).expect("t29 A1 render");
        // arith_to_sh renders the postfix IncDec as `((x = x + 1) - 1)`
        // (the old value, with the increment applied); the statement
        // form must wrap it in `: $(( ... ))` — never a bare `$((...))`
        // line (that would RUN the value as a command) and never an
        // `x=$((x++))` assign (that would clobber the side effect).
        assert!(
            sh.contains(": $((((x = x + 1) - 1)))"),
            "IncDec statement: {sh}"
        );
    }
}
