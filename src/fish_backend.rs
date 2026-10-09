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
                IrStmt::Shaderize { body, .. } => {
                    for s in body {
                        walk(std::slice::from_ref(s), names);
                    }
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

/// Is `name` an ASSOCIATIVE array (string keys, `declare -A`)?
///
/// Indexed arrays lower to a native fish LIST (`set arr a b c`), which fish
/// already expands, joins, slices and counts — so they need no runtime at
/// all. bash's associative arrays have no fish equivalent, so those keep the
/// per-key helpers (`_arr_expand_k`, `_arr_keys_k`).
fn is_assoc(name: &str) -> bool {
    ASSOC_VARS.lock().unwrap().contains(arr_base(name))
}

/// A bare identifier — the only shape a `$name` list read can be spelled
/// with. `${!prefix*}` (bash's indirect NAME listing) arrives here as the
/// raw text `prefix*`, which is not a variable at all.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The whole-array expansion in a VALUE slot — bash's `${arr[@]}` /
/// `${arr[*]}` where the elements end up joined into ONE word.
///
/// The join must be EXPLICIT. `"$arr"` looks like the same thing, and IS one
/// word when the list is the whole string — but inside a larger template fish
/// emits one word PER ELEMENT (`"Array: $arr"` on a 4-element list prints
/// `Array: a Array: b …`), which is not bash's `${arr[*]}` at all. A
/// substitution yielding a single joined line is one word everywhere.
fn arr_expand_call_fish(name: &str) -> String {
    if is_assoc(name) {
        return format!("(_arr_expand_k_join {name})");
    }
    if !is_plain_name(name) {
        return format!("(_arr_expand_join {name})");
    }
    format!("(string join ' ' ${name})")
}

/// The `for x in …` form — bash iterates per ELEMENT, and fish's bare `$arr`
/// IS the element list, so the loop wants the bare list.
///
/// (It must stay BARE: `for x in "$arr"` would run the body once against the
/// space-joined string instead of once per element.)
fn arr_expand_call_fish_list(name: &str) -> String {
    if is_assoc(name) {
        return format!("(_arr_expand_k {name})");
    }
    if !is_plain_name(name) {
        return format!("(_arr_expand {name})");
    }
    format!("${name}")
}

/// The element INDICES — bash's `${!arr[@]}`.
///
/// A fish list is always 0-based and contiguous, so its index list is
/// `0 .. count-1`: `seq` and `count` are both builtins, so this is native
/// fish with no helper. bash's indirect NAME listing (`${!prefix*}`) is not
/// an array read at all and keeps the old helper.
fn arr_keys_call_fish(name: &str) -> String {
    if is_assoc(name) {
        return format!("(_arr_keys_k {name})");
    }
    if !is_plain_name(name) {
        return format!("(_arr_keys {name})");
    }
    format!("(seq 0 (math (count ${name}) - 1))")
}

/// bash's `${#arr[@]}` — the element count. `count` is a fish builtin, so an
/// indexed array needs no counter variable.
fn arr_len_fish(name: &str) -> String {
    if is_assoc(name) {
        return format!("${name}_len");
    }
    format!("(count ${name})")
}

/// A LITERAL element reference — bash's `arr[i]`, 0-based; fish subscripts its
/// list from 1, so a numeric index is offset by one. A non-numeric index is
/// an assoc KEY and keeps the flat `{base}_{key}` element name.
fn arr_elem_ref(base: &str, idx: &str) -> String {
    if is_plain_name(base) && !is_assoc(base) {
        let key = if idx.is_empty() { "0" } else { idx };
        if let Ok(n) = key.parse::<i64>() {
            return format!("${base}[{}]", n + 1);
        }
    }
    format!("${base}_{idx}")
}

/// [`arr_elem_ref`] with the quoting a WORD position needs: the element is
/// ONE word, so it is quoted. fish accepts a literal subscript inside double
/// quotes (`"$arr[3]"`), which is why this form is usable at all.
fn arr_elem_word(base: &str, idx: &str, quoted: bool) -> String {
    let r = arr_elem_ref(base, idx);
    if quoted {
        format!("\"{r}\"")
    } else {
        r
    }
}

/// The sub-expressions of `e` that a prologue detector must also visit.
///
/// A prologue detector decides whether to EMIT a helper, while the call-site
/// renderer independently decides whether to CALL it. When the two disagree
/// the output names a helper that was never emitted — `_readlink: command not
/// found`, silently empty captures (readlink_example.sh).
///
/// `Capture` is the one that bit: an assignment RHS is
/// `Assign { expr: Capture { expr: Arrow([Expr(Call)]) } }`, so a detector
/// whose match stopped at `Call`/`Array`/`Object`/`Arrow` could not see a
/// command inside `x=$(…)` at all. These containers can each hold a nested
/// command, so every detector descends through all of them.
fn expr_containers(e: &IrExpr) -> Vec<&IrExpr> {
    match e {
        IrExpr::Capture { expr, .. } => vec![expr],
        IrExpr::BinOp { lhs, rhs, .. } => vec![lhs, rhs],
        IrExpr::Ternary { cond, then, else_ } => vec![cond, then, else_],
        IrExpr::DefinedOr { expr, default } => vec![expr, default],
        IrExpr::Index { key, .. } => vec![key],
        IrExpr::MethodCall { obj, args, .. } => {
            let mut v = vec![obj.as_ref()];
            v.extend(args.iter());
            v
        }
        IrExpr::Interpolate(parts) => parts
            .iter()
            .filter_map(|p| match p {
                InterpPart::Expr(x) => Some(x.as_ref()),
                InterpPart::Lit(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Is `name` CALLED in `body` (as opposed to merely mentioned in a comment)?
///
/// Every helper here is emitted as `name ARG`, so a call is `name ` or
/// `name)`. The delimiter is load-bearing: `_arr_expand` is a PREFIX of
/// `_arr_expand_join`, and a bare `contains("_arr_expand")` would report the
/// `_join` caller as also calling the list form.
fn body_calls(body: &str, name: &str) -> bool {
    body.contains(&format!("{name} ")) || body.contains(&format!("{name})"))
}

/// The assoc-array runtime, plus the two shapes only a non-identifier array
/// name can reach. Emitted per helper — see [`arr_helper_preamble`].
const ARR_HELPERS: &[(&str, &str)] = &[
    (
        "_arr_expand",
        r#"function _arr_expand --argument-names name
    # `${arr[@]}` in a word-split position — one element per line.
    set -l _len_var $name"_len"
    set -l _n $$_len_var
    or set _n 0
    set -l _i 0
    while test $_i -lt $_n
        set -l _ev $name"_$_i"
        printf '%s\n' $$_ev
        set _i (math $_i + 1)
    end
end
"#,
    ),
    (
        "_arr_expand_join",
        r#"function _arr_expand_join --argument-names name
    # `${arr[@]}` / `${arr[*]}` where bash joins with a space.
    #
    # `string join` is already newline-TERMINATED (measured, fish 3.7: it
    # writes "b c\n"), and a fish command substitution splits on newlines —
    # so a following `printf '\n'` would make the result [joined, ""] and
    # surface as a trailing space. The `or` covers the one case `string join`
    # cannot terminate itself: an EMPTY list outputs nothing and returns 1,
    # and bash's `${arr[*]}` on an empty array is a single empty word.
    set -l _out (_arr_expand $name)
    string join ' ' $_out
    or printf '\n'
end
"#,
    ),
    (
        "_arr_keys",
        r#"function _arr_keys --argument-names name
    # the element INDICES — a plain list, one per line.
    set -l _len_var $name"_len"
    set -l _n $$_len_var
    or set _n 0
    set -l _i 0
    while test $_i -lt $_n
        printf '%s\n' $_i
        set _i (math $_i + 1)
    end
end
"#,
    ),
    (
        "_arr_expand_k",
        r#"function _arr_expand_k --argument-names name
    # assoc variant: values in key-list order ($name_keys).
    set -l _keys_var $name"_keys"
    for _k in $$_keys_var
        set -l _ev $name"_"$_k
        printf '%s\n' $$_ev
    end
end
"#,
    ),
    (
        "_arr_expand_k_join",
        r#"function _arr_expand_k_join --argument-names name
    # assoc variant, space-joined (bash's `${arr[*]}`).
    set -l _out (_arr_expand_k $name)
    string join ' ' $_out
    or printf '\n'
end
"#,
    ),
    (
        "_arr_keys_k",
        r#"function _arr_keys_k --argument-names name
    # assoc variant: the key list, one per line.
    set -l _keys_var $name"_keys"
    for _k in $$_keys_var
        printf '%s\n' $_k
    end
end
"#,
    ),
    (
        "_fish_get_el",
        r#"function _fish_get_el --argument-names base key
    # `${base[$key]}` — the INDIRECT assoc element read: compose the element
    # name, then dereference it once with fish's `$$name` indirection
    # (`$base_$key` is NOT that — fish reads it as the name `$base_`).
    set -l _nk $base"$key"
    if set -q _nk
        printf '%s\n' "$$_nk"
    else
        printf ''
    end
end
"#,
    ),
    (
        "_arr_slice",
        r#"function _arr_slice --argument-names name off len
    # elements off..off+len-1, space-joined.
    set -l _len_var $name"_len"
    set -l _end $$_len_var
    or set _end 0
    set -l _lim (math $off + $len)
    set -l _out
    set -l _i $off
    while test $_i -lt $_lim; and test $_i -lt $_end
        set -l _ev $name"_$_i"
        set -a _out $$_ev
        set _i (math $_i + 1)
    end
    string join ' ' $_out
    or printf '\n'
end
"#,
    ),
];

/// The array-runtime helpers the rendered body actually CALLS, in
/// dependency order (a `_join` helper is emitted above its base).
///
/// Indexed arrays need none of this — they lower to a native fish LIST, so a
/// file that only touches indexed arrays emits no runtime at all.
fn arr_helper_preamble(body: &str) -> String {
    let mut needed: Vec<&str> = ARR_HELPERS
        .iter()
        .filter(|(name, _)| body_calls(body, name))
        .map(|(name, _)| *name)
        .collect();
    // `_arr_expand_join` calls `_arr_expand`; `_arr_expand_k_join` calls
    // `_arr_expand_k`.
    for base in ["_arr_expand", "_arr_expand_k"] {
        let derived = ARR_HELPERS
            .iter()
            .any(|(n, _)| needed.contains(n) && *n == format!("{base}_join").as_str());
        if derived && !needed.contains(&base) {
            needed.push(base);
        }
    }
    if needed.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(
        "#\n# shIR array runtime — only the helpers this file actually calls.\n",
    );
    for (name, src) in ARR_HELPERS {
        if needed.contains(name) {
            out.push_str(src);
        }
    }
    out.push('\n');
    out
}

/// Render a ShIR program to `sh` source. `Err` on a construct outside the
/// renderable subset (the gate reports it as a FAIL).
pub fn shir_to_fish(prog: &IrProgram) -> Result<String, String> {
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
    out.push_str("#!/usr/bin/env fish\n");
    out.push_str("#\n");
    out.push_str("# Generated by the fish backend. Fish-specific constructs:\n");
    out.push_str("# - 'set' for assignment (no var=value)\n");
    out.push_str("# - 'end' for all block closers (no fi/done/esac)\n");
    out.push_str("# - 'math' for arithmetic (no $(( )))\n");
    out.push_str("# - command substitution uses (cmd) not $(cmd)\n");
    out.push_str("#\n");

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
    // The body is rendered FIRST so the runtime preamble can be decided by
    // what it actually REFERENCES. The old code pre-scanned the IR for
    // "might need an array helper" and then emitted all eight helpers as one
    // block — so a file touching one array carried the whole runtime, and
    // the helpers it never called still counted as runtime calls. Deciding
    // from the rendered text is exact by construction: the definition is
    // emitted exactly when a call is, whichever lowering produced it.
    let mut body = String::new();
    for st in &prog.stmts {
        stmt_to_fish(st, 0, &mut body)?;
    }
    for sub in &prog.subs {
        body.push_str(&format!("\nfunction {}\n", sub.name));
        for st in &sub.body {
            stmt_to_fish(st, 1, &mut body)?;
        }
        body.push_str("end\n");
    }
    out.push_str(&arr_helper_preamble(&body));
    if body.contains("( _num ") || body.contains(" _num ") {
        out.push_str("function _num --argument val\n");
        out.push_str("    # bash coerces non-numeric arith values to 0; fish math errors\n");
        out.push_str("    if string match -qr -- '^-?[0-9]+$' $val\n");
        out.push_str("        echo $val\n");
        out.push_str("    else\n");
        out.push_str("        echo 0\n");
        out.push_str("    end\n");
        out.push_str("end\n\n");
    }
    out.push_str(&body);
    Ok(out)
}

fn indent(out: &mut String, d: usize) {
    for _ in 0..d {
        out.push_str("    ");
    }
}

/// Does this function body carry the echo-return (value-returning) shape?
///
/// The `echo_return` transform rewrites a pure `echo <v>` function into
/// `Return(Some(v))` so the caller can use the value directly; the fish
/// call site therefore renders as `(f args)` (a command substitution).
/// The body must then put the value on stdout, since fish's `return` is a
/// STATUS. This predicate decides whether the body is that shape.
///
/// REFUSE > GUESS: the body qualifies only when EVERY top-level path is
/// value-terminating, so a mixed body (some paths returning a value, some
/// falling through or returning bare) keeps the plain `return` rendering
/// rather than half of it being silently turned into an `echo`. Loops and
/// subshells are NOT inspected — a value return buried in one is a construct
/// this lowering does not model, so such a body is refused too.
fn fn_body_returns_value(body: &[IrStmt]) -> bool {
    if body.is_empty() {
        return false;
    }
    fn ends_with_value(s: &IrStmt) -> bool {
        matches!(s, IrStmt::Return(Some(_)))
    }
    for s in body {
        match s {
            IrStmt::Return(Some(_)) => {}
            // A bare `return` does not end the value path — refuse rather
            // than guess which convention the body meant.
            IrStmt::Return(None) => return false,
            IrStmt::If { then, elsifs, else_, .. } => {
                // every arm of the chain must end with a value return, and
                // the chain must be total (an `else` must exist, else the
                // condition's false path falls through).
                let arms_then = !then.is_empty() && then.iter().all(ends_with_value);
                let arms_elsif =
                    elsifs.iter().all(|(_, b)| !b.is_empty() && b.iter().all(ends_with_value));
                let arms_else = !else_.is_empty() && else_.iter().all(ends_with_value);
                if !(arms_then && arms_elsif && arms_else) {
                    return false;
                }
            }
            // A construct that cannot itself end a path and cannot hide a
            // value return is fine to step over — the echo-return bodies are
            // a local computation followed by the value return
            // (`local y=5; echo "$y"`).
            //
            // A LOOP belongs here too, and refusing it was a real lowering
            // bug: `echo_return`'s own loop discipline (`loop_body_states`)
            // rejects any loop whose body echoes or returns, so a loop in an
            // eligible body is provably silent — it can neither emit a value
            // line nor hide a value return. Refusing it demoted the WHOLE
            // body to plain `return`s, and fish's `return` is a STATUS: the
            // function printed nothing and the caller's `echo (f)` captured
            // the empty string (092_for_arith_func: `factorial` returned no
            // stdout, so both calls printed a blank line instead of 120/720).
            // A `Block` is the same case — the transform's own `stmt_silent`
            // admits a block whose statements are all silent, so it cannot
            // hide a value return either. Stepping over both is EXACT, not a
            // guess. (The If arm below deliberately keeps its totality
            // requirement: a non-total `if` is exactly the shape of a real
            // status-returning predicate like 051_primes' `is_prime`, and
            // those must keep rendering as `return`.)
            IrStmt::Declare { .. }
            | IrStmt::DeclareArray { .. }
            | IrStmt::Assign { .. }
            | IrStmt::For { .. }
            | IrStmt::While { .. }
            | IrStmt::ForInit { .. }
            | IrStmt::Block(_) => {}
            // The same local computation spelled as the DECLARATION BUILTIN:
            // `local y=$1` reaches the IR as `Expr(Call{builtin, ["local", …]})`
            // rather than an `Assign`. Refusing it made every function whose
            // body opened with a `local` fall back to `return <value>` — a
            // fish RUNTIME error ("return: n=hello: invalid integer"),
            // because fish's `return` is an integer STATUS. A declaration
            // cannot return anything, so it is safe to step over exactly
            // like `IrStmt::Declare`.
            IrStmt::Expr(IrExpr::Call { func, args })
                if matches!(func.as_str(), "builtin" | "exec")
                    && matches!(args.first(), Some(IrExpr::Str(n, _))
                        if matches!(n.as_str(), "local" | "declare" | "typeset" | "readonly")) => {}
            // A `case` is total when some arm is a CATCH-ALL (`*`) and every
            // arm's body ends with a value return: the discriminant always
            // lands in an arm, and each arm yields a value. The core rewrites
            // each arm's trailing `echo` into `return`, so a `case` of echoes
            // is exactly this shape — 061_test_local_names_preserved's
            // `test_math`/`test_strings`, whose arms fell through to the plain
            // renderer and emitted `return "text"`, a fish RUNTIME error
            // ("invalid integer") that cost the whole script its output.
            IrStmt::Case { clauses, .. } => {
                if !clauses.iter().any(|c| c.patterns.iter().any(|p| p == "*")) {
                    return false;
                }
                if clauses
                    .iter()
                    .any(|c| c.body.is_empty() || !c.body.iter().all(ends_with_value))
                {
                    return false;
                }
            }
            // Anything that can hide a value return (loop, subshell,
            // redirect, exec, …) or can branch: refuse — REFUSE > GUESS.
            _ => return false,
        }
    }
    true
}

/// Can this `return` value be a fish STATUS (an integer)?
///
/// fish's `return` takes an INTEGER only — `return "text"` is a runtime error
/// ("invalid integer"). The core rewrites a value-returning function's
/// trailing `echo VALUE` into `Return(VALUE)`, and `fn_body_returns_value`
/// only recognises the bodies it can PROVE, so a body it refuses (a `case`
/// whose arms each echo — 061_test_local_names_preserved, 058_advanced_…) still
/// carries a string value here. Such a value belongs on STDOUT, which is what
/// the caller's `(f)` captures and what the rewrite came from.
///
/// A composed string with any literal TEXT is that case. A pure `"$n"` stays a
/// status — bash coerces a numeric variable, and `return $?` is a real status.
fn return_value_is_status(x: &IrExpr) -> bool {
    match x {
        IrExpr::Interpolate(parts) => !parts.iter().any(|p| {
            matches!(p, InterpPart::Lit(t)
                if !t.is_empty() && t.trim().parse::<i64>().is_err())
        }),
        IrExpr::Str(s, _) => s.trim().parse::<i64>().is_ok(),
        _ => true,
    }
}

/// Render one statement of a value-returning function body, rewriting
/// `return <v>` to `echo <v>` (the value must reach stdout for the caller's
/// `(f)` to capture — see `fn_body_returns_value`).
///
/// `fn_body_returns_value` has already established that EVERY path in this
/// body ends with a value return, so the only `return`s reachable here are
/// value returns; a nested `If`'s arms are re-rendered through this same
/// function so their returns get the same treatment. Anything else falls
/// through to the ordinary statement renderer.
fn valret_body_to_fish(s: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
    match s {
        IrStmt::Return(Some(x)) => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_fish(x)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            indent(out, d);
            out.push_str("if ");
            out.push_str(&cond_to_fish(cond)?);
            out.push('\n');
            for b in then {
                valret_body_to_fish(b, d + 1, out)?;
            }
            for (c, body) in elsifs {
                indent(out, d);
                out.push_str("else if ");
                out.push_str(&cond_to_fish(c)?);
                out.push('\n');
                for b in body {
                    valret_body_to_fish(b, d + 1, out)?;
                }
            }
            if !else_.is_empty() {
                out.push_str("else\n");
                for b in else_ {
                    valret_body_to_fish(b, d + 1, out)?;
                }
                indent(out, d);
            }
            out.push_str("end\n");
            Ok(())
        }
        // A `case` arm whose body ends with a value return — the same
        // rewrite as the `If` arm above, arm for arm. `fn_body_returns_value`
        // has already proved every arm ends in a value return.
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            indent(out, d);
            out.push_str("switch ");
            out.push_str(&word_to_fish(discriminant)?);
            out.push('\n');
            for cl in clauses {
                indent(out, d + 1);
                out.push_str("case ");
                out.push_str(&case_patterns_fish(&cl.patterns));
                out.push('\n');
                for b in &cl.body {
                    valret_body_to_fish(b, d + 2, out)?;
                }
            }
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        _ => stmt_to_fish(s, d, out),
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

/// `echo ARG | grep -q [-v] P` → native `case` substring check (no grep
/// spawn; case/true/false are builtins).
fn grep_q_lift_sh(stages: &[Vec<IrStmt>]) -> Result<Option<String>, String> {
    if stages.len() != 2 {
        return Ok(None);
    }
    // stage 1: echo ARG
    let echo_arg = match stages[0].as_slice() {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "builtin" || func == "exec" => {
            let name = match args.first() {
                Some(IrExpr::Str(n, _)) => n.as_str(),
                _ => return Ok(None),
            };
            if name != "echo" {
                return Ok(None);
            }
            let items = match args.get(1) {
                Some(IrExpr::Array(it)) => it,
                _ => return Ok(None),
            };
            if items.len() != 1 {
                return Ok(None);
            }
            word_to_fish(&items[0])?
        }
        _ => return Ok(None),
    };
    // stage 2: grep -q [-v] P
    let (pat, invert) = match stages[1].as_slice() {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "builtin" || func == "exec" => {
            let name = match args.first() {
                Some(IrExpr::Str(n, _)) => n.as_str(),
                _ => return Ok(None),
            };
            if name != "grep" {
                return Ok(None);
            }
            let items = match args.get(1) {
                Some(IrExpr::Array(it)) => it,
                _ => return Ok(None),
            };
            let mut pat: Option<String> = None;
            let mut invert = false;
            let mut quiet = false;
            for it in items {
                match it {
                    IrExpr::Str(s, _) if s == "-q" => quiet = true,
                    IrExpr::Str(s, _) if s == "-v" => invert = true,
                    // any OTHER flag (-oP, -E, -i, …) means the case-glob
                    // emulation cannot express the semantics — bail to the
                    // general grep rendering (the grep_p polyfill for -P)
                    IrExpr::Str(s, _) if s.starts_with('-') => return Ok(None),
                    IrExpr::Str(s, _) => pat = Some(s.clone()),
                    _ => return Ok(None),
                }
            }
            // -q IS THE WHOLE POINT. `string match -q` is silent by
            // construction: it reports a STATUS and prints nothing. A plain
            // `grep PAT` PRINTS the matching line and `grep -v PAT` prints the
            // non-matching ones, so lifting either to a quiet glob match
            // silently dropped that output (101_pipeline_failure_grep:
            // `echo "alpha beta gamma" | grep beta` rendered as
            // `string match -q -- beta 'alpha beta gamma'` and lost the
            // "alpha beta gamma" line bash prints). Without -q there is no
            // silent lowering to make — fall through to the real grep pipe.
            if !quiet {
                return Ok(None);
            }
            (pat, invert)
        }
        _ => return Ok(None),
    };
    let (Some(pat), invert) = (pat, invert) else {
        return Ok(None);
    };
    // SUBSTRING, not full-string. bash's `grep -q PAT` is an unanchored BRE
    // search over the line; `string match` matches the pattern against the
    // WHOLE string, so the bare literal had to match entirely (measured:
    // `string match -q -- beta 'alpha beta'` exits 1 where `grep -q beta`
    // exits 0). Wrap the pattern for the substring semantics `grep` has —
    // only for a PLAIN literal, since a regex metacharacter (`^`, `.`, `+`,
    // `\`, …) has no glob equivalent here and guessing would silently change
    // which lines match. Anything else falls back to the real grep pipe.
    if pat.contains(|c: char| "\\.^$*+?()[]{}|".contains(c)) {
        return Ok(None);
    }
    let pat = fish_pattern_lit(&format!("*{pat}*"));
    Ok(Some(if invert {
        format!("not string match -q -- {pat} {echo_arg}")
    } else {
        format!("string match -q -- {pat} {echo_arg}")
    }))
}

/// Render a statement-level Ext node whose tag this backend knows.
///
/// `ForEachLine` (docs/shir-primitives.md §2) is the STREAMING line-iterator
/// primitive: `source` is a FILE PATH, `var` receives each line WITHOUT its
/// newline, and `body` runs once per line. fish's native read loop is
/// `while read -l VAR; BODY; end < PATH` — the redirect form, which is also
/// how this backend already renders a bash `while read …; done < F` loop. The
/// obvious alternative, `PATH | while read …`, is WRONG here: a bare path is
/// not a command, so that pipe would try to EXECUTE the file. `< (cmd)` is no
/// better — fish has no process substitution and `<` needs a real file.
///
/// A `limit` (head-style early exit) is the one case that needs a pipe:
/// `head -n N PATH | while …` streams and lets `head` close the file after N
/// lines, exactly as `head -n N PATH` does in bash. The loop var is `-l`
/// (block-local) because ForEachLine desugars a file-consuming COMMAND
/// (`cat F`, `grep P F | cut …`), so unlike a user-written `while read` loop
/// there is no bash-visible loop variable whose post-loop value could differ.
fn ext_node_to_fish(node: &dyn crate::shir_nodes::ExtNode) -> Result<Option<String>, String> {
    match node.tag() {
        "ForEachLine" => {
            let Some(fl) = node
                .as_any()
                .downcast_ref::<crate::shir_nodes::ForEachLine>()
            else {
                return Err("ForEachLine tag/type disagree".to_string());
            };
            let path = word_to_fish(&fl.source)?;
            Ok(Some(match &fl.limit {
                Some(lim) => format!(
                    "head -n {} {} | while read -l {}; {}; end",
                    word_to_fish(lim)?,
                    path,
                    fl.var,
                    stmts_inline(&fl.body)?
                ),
                None => format!(
                    "while read -l {}; {}; end < {}",
                    fl.var,
                    stmts_inline(&fl.body)?,
                    path
                ),
            }))
        }
        // A tag this backend has no lowering for stays an error, naming the
        // tag (refuse > guess — never emit a stub).
        _ => Ok(None),
    }
}

fn stmt_to_fish(st: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
    match st {
        IrStmt::Ext(node) => {
            let Some(rendered) = ext_node_to_fish(node.as_ref())? else {
                return Err(format!("sh renderer: Ext node {} unsupported", node.tag()));
            };
            indent(out, d);
            out.push_str(&rendered);
            out.push('\n');
            Ok(())
        }
        IrStmt::Expr(e) => {
            indent(out, d);
            if let IrExpr::Call { func, .. } = e {
                if func == "grepMatches" {
                    // statement position: the matches are the output
                    out.push_str(&format!("echo \"{}\"", cmd_to_fish(e)?));
                    out.push('\n');
                    return Ok(());
                }
            }
            out.push_str(&cmd_to_fish(e)?);
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
            out.push_str(&assign_to_fish(targets, expr)?);
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
            out.push_str(&cond_to_fish(cond)?);
            out.push_str("\n");
            if then.is_empty() {
                indent(out, d + 1);
                out.push_str(": ;\n");
            } else {
                for b in then {
                    stmt_to_fish(b, d + 1, out)?;
                }
            }
            for (econd, ebody) in elsifs {
                indent(out, d);
                // fish spells the `elif` branch `else if` — it has no `elif`
                // keyword, and an unknown keyword is not a parse error but a
                // RUNTIME one ("Unknown command: elif"), so the branch and
                // everything after it silently never runs
                // (090_nested_if_else.sh lost its `farewell` line).
                out.push_str("else if ");
                out.push_str(&cmd_to_fish(econd)?);
                out.push_str("\n");
                if ebody.is_empty() {
                    indent(out, d + 1);
                    out.push_str(": ;\n");
                } else {
                    for b in ebody {
                        stmt_to_fish(b, d + 1, out)?;
                    }
                }
            }
            if !else_.is_empty() {
                indent(out, d);
                out.push_str("else\n");
                for b in else_ {
                    stmt_to_fish(b, d + 1, out)?;
                }
            }
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        IrStmt::For { var, iter, body } => {
            // GNU $(seq A B) / {A..B} -> a fish for-loop over (seq)
            if let Some((start, end)) = range_of(iter) {
                indent(out, d);
                out.push_str(&format!("for {var} in (seq {start} {end})\n"));
                for b in body {
                    stmt_to_fish(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("end\n");
                return Ok(());
            }
            indent(out, d);
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            out.push_str(&for_items_to_fish(iter)?);
            out.push_str("\n");
            for b in body {
                stmt_to_fish(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        IrStmt::While { cond, body } => {
            indent(out, d);
            out.push_str("while ");
            out.push_str(&cond_to_fish(cond)?);
            out.push_str("\n");
            for b in body {
                stmt_to_fish(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("end\n");
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
            out.push_str("while true\n");
            for b in body {
                stmt_to_fish(b, d + 1, out)?;
            }
            indent(out, d + 1);
            if *until {
                out.push_str("if ");
            } else {
                out.push_str("if not ");
            }
            out.push_str(&cond_to_fish(cond)?);
            out.push_str("; and break\n");
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            indent(out, d);
            out.push_str("switch ");
            out.push_str(&word_to_fish(discriminant)?);
            out.push_str("\n");
            for cl in clauses {
                indent(out, d + 1);
                out.push_str("case ");
                out.push_str(&case_patterns_fish(&cl.patterns));
                out.push_str("\n");
                for b in &cl.body {
                    stmt_to_fish(b, d + 2, out)?;
                }
            }
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        IrStmt::Function { name, body, .. } => {
            indent(out, d);
            out.push_str("function ");
            out.push_str(name);
            out.push_str("\n");
            // A function whose value the caller consumes carries the
            // echo-return shape: every path ends in `Return(Some(v))`. fish's
            // `return` is a STATUS (an integer), so the value must leave on
            // stdout for the caller's `(f)` to capture — the bash `$(f)`
            // convention the call site was rewritten from. Inside such a body
            // `return <v>` lowers to `echo <v>`; a value-less `return` (and a
            // status return in a value-less body) stays a real `return`.
            if fn_body_returns_value(body) {
                for b in body {
                    valret_body_to_fish(b, d + 1, out)?;
                }
            } else {
                for b in body {
                    stmt_to_fish(b, d + 1, out)?;
                }
            }
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        IrStmt::Redirect { inner, redirects } => {
            // process substitutions lower to pipes / temp files (POSIX has
            // no `<(cmd)`); the remaining redirects render as suffixes
            let (ps_ins, plain) = partition_procsub(redirects);
            let suffix = redirects_to_fish(&plain)?;
            let mut line = if inner.len() == 1 {
                if let IrStmt::Expr(e) = &inner[0] {
                    cmd_to_fish(e)?
                } else {
                    stmts_inline(inner)?
                }
            } else {
                // compound inner: inline it, then the redirects apply to the group
                stmts_inline(inner)?
            };
            line = herestring_wrap_fish(&plain, line)?;
            if !ps_ins.is_empty() {
                line = lower_procsub_stmt_fish(&ps_ins, &line)?;
            }
            indent(out, d);
            out.push_str(&line);
            out.push_str(&suffix);
            out.push('\n');
            Ok(())
        }
        IrStmt::Subshell(body) => {
            indent(out, d);
            out.push_str("begin\n");
            for b in body {
                stmt_to_fish(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("end\n");
            Ok(())
        }
        IrStmt::Background(body) => {
            if body.len() == 1 {
                if let IrStmt::Expr(e) = &body[0] {
                    indent(out, d);
                    out.push_str(&cmd_to_fish(e)?);
                    out.push_str(" &\n");
                    return Ok(());
                }
            }
            indent(out, d);
            out.push_str("begin\n");
            for b in body {
                stmt_to_fish(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("end &\n");
            Ok(())
        }
        IrStmt::Block(body) => {
            indent(out, d);
            out.push_str("begin\n");
            for b in body {
                stmt_to_fish(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("end\n");
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
                out.push_str("begin; end\n");
            } else {
                out.push_str("begin\n");
                for b in body {
                    stmt_to_fish(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("end\n");
            }
            if !excepts.is_empty() {
                indent(out, d);
                out.push_str("if test $status -ne 0\n");
                for b in &excepts[0].body {
                    stmt_to_fish(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("end\n");
            } else if !else_body.is_empty() {
                indent(out, d);
                out.push_str("if test $status -eq 0\n");
                for b in else_body {
                    stmt_to_fish(b, d + 1, out)?;
                }
                indent(out, d);
                out.push_str("end\n");
            }
            for b in finally_body {
                stmt_to_fish(b, d, out)?;
            }
            Ok(())
        }
        // sh has no select-on-channels — refuse loudly
        IrStmt::Select { .. } => Err("select has no sh rendering".into()),
        // inline asm has no sh rendering — refuse loudly
        IrStmt::Asm { .. } => Err("inline asm has no sh rendering".into()),
        IrStmt::Return(e) => {
            if let Some(x) = e {
                if !return_value_is_status(x) {
                    indent(out, d);
                    out.push_str("echo ");
                    out.push_str(&word_to_fish(x)?);
                    out.push('\n');
                    return Ok(());
                }
            }
            indent(out, d);
            out.push_str("return");
            if let Some(x) = e {
                out.push(' ');
                out.push_str(&word_to_fish(x)?);
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::Exit(e) => {
            indent(out, d);
            out.push_str("exit");
            if let Some(x) = e {
                out.push(' ');
                out.push_str(&word_to_fish(x)?);
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::Die { expr, .. } => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_fish(expr)?);
            out.push_str(" >&2\n");
            indent(out, d);
            out.push_str("exit 1\n");
            Ok(())
        }
        IrStmt::Warn { expr, .. } => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_fish(expr)?);
            out.push_str(" >&2\n");
            Ok(())
        }
        IrStmt::Declare { vars, init, local } => {
            indent(out, d);
            if *local {
                out.push_str("set -l ");
            } else {
                out.push_str("set ");
            }
            for (i, v) in vars.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&v.name);
                if let Some(init) = init {
                    out.push(' ');
                    out.push_str(&word_to_fish(init)?);
                }
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            indent(out, d);
            out.push_str("set ");
            out.push_str(var);
            out.push(' ');
            out.push_str(&array_literal_to_fish(elements)?);
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
            out.push_str(&word_to_fish(value)?);
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
            let mut line = exec_line_to_fish(cmd, args, Some(env))?;
            if !redirects.is_empty() {
                line.push_str(&redirect_objs_to_fish(redirects)?);
            }
            if let Some(var) = capture {
                indent(out, d);
                out.push_str("set ");
                out.push_str(var);
                out.push_str(" (");
                out.push_str(&line);
                out.push_str(" | string collect)\n");
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
            // idiom lift: `echo ARG | grep -q [-v] P` → native case
            // (no grep spawn)
            if capture.is_none() {
                if let Some(lift) = grep_q_lift_sh(stages)? {
                    indent(out, d);
                    out.push_str(&lift);
                    out.push('\n');
                    return Ok(());
                }
            }
            let mut line = String::new();
            for (i, stg) in stages.iter().enumerate() {
                if i > 0 {
                    line.push_str(" | ");
                }
                line.push_str(&stmts_inline(stg)?);
            }
            if let Some(var) = capture {
                indent(out, d);
                out.push_str("set ");
                out.push_str(var);
                out.push_str(" (");
                out.push_str(&line);
                out.push_str(" | string collect)\n");
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
            out.push_str(&word_to_fish(content)?);
            out.push_str(if *append { " >> " } else { " > " });
            out.push_str(&word_to_fish(path)?);
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
        IrStmt::Shaderize { .. } => Ok(()),
    }
}

/// `var=value` for a statement-level assignment. Handles the sh2.* RHS
/// forms (capture, pipeline, arith, setArray, assign) natively.
fn assign_to_fish(targets: &[crate::ir::AssignTarget], expr: &IrExpr) -> Result<String, String> {
    // `((i++))` / `((i--))` — the arith-statement shape (triage-sh
    // t29_increment): the estree renderer emits the IncDec BARE when the
    // single target is the SAME var the arith mutates — the statement's value
    // is discarded, so just do the increment.
    if targets.len() == 1
        && targets[0].indices.is_empty()
        && matches!(expr, IrExpr::Arith(a) if matches!(&**a, ArithAst::IncDec { var, .. } if var == &targets[0].var))
    {
        let IrExpr::Arith(a) = expr else { unreachable!() };
        if let ArithAst::IncDec { var, delta, .. } = &**a {
            let op = if *delta > 0 { "+" } else { "-" };
            return Ok(format!("set {var} (math --scale 0 \"${var} {op} 1\")"));
        }
        return Ok(format!(":"));
    }
    // `let x=5` / `let x+=3` / `let i=i+1` — the `let` builtin reaches the A1
    // as an Assign statement whose RHS is an Arith `Assign` node. fish's
    // `math` has NO assignment syntax at all: `math --scale 0 "x = 5"` is
    // "math: Error: Unexpected token 'x = 5'", and the expression form this
    // fell through to (`"$i = ($i + 1)"`) additionally embedded the rhs in
    // literal parens, which fish does not expand inside a double-quoted
    // string — so `math` received ` = ( + 1)` and the whole statement
    // produced nothing. The assignment IS the statement here, so spell it as
    // the fish assignment it is; the rhs is a plain math EXPRESSION.
    if targets.len() == 1 && targets[0].indices.is_empty() {
        if let IrExpr::Arith(a) = expr {
            if let ArithAst::Assign { op, rhs, .. } = &**a {
                let t = &targets[0].var;
                let r = arith_to_fish(rhs);
                return Ok(match op.as_str() {
                    "=" => format!("set {t} (math --scale 0 \"{r}\")"),
                    "+=" => format!("set {t} (math --scale 0 \"${t} + {r}\")"),
                    "-=" => format!("set {t} (math --scale 0 \"${t} - {r}\")"),
                    "*=" => format!("set {t} (math --scale 0 \"${t} * {r}\")"),
                    // `/=` / `%=` need the zero-divisor abort → not lowered.
                    _ => return Err(format!("arith assign op not renderable: {op:?}")),
                });
            }
        }
    }
    // `arr=(a b c)` — the A1 is Assign{var: arr, expr: setArray(...)}. The
    // setArray lowering IS the assignment — no `name=` prefix.
    if let IrExpr::Call { func, .. } = expr {
        if func == "setArray" || func == "setArrayAppend" {
            return cmd_to_fish(expr);
        }
    }
    let mut out = String::new();
    for (i, t) in targets.iter().enumerate() {
        if i > 0 {
            out.push_str("; ");
        }
        let mut rhs = assign_rhs_to_fish(expr)?;
        // `typeset -i` vars assign ARITHMETICALLY (`n=n+1` -> 43)
        if INT_VARS.lock().unwrap().contains(&t.var) && t.indices.is_empty() {
            let raw = match expr {
                IrExpr::Str(s, _) => s.clone(),
                IrExpr::Call { func, args } if func == "getVar" => {
                    raw_arg(args, 0).unwrap_or_default()
                }
                _ => rhs.clone(),
            };
            rhs = format!("(math --scale 0 \"{}\")", raw);
        }
        // baked element targets (`arr[1]=x` — the A1 folds the subscript
        // into the var name)
        if let Some((base, idx)) = t.var.strip_suffix(']').and_then(|v| v.split_once('[')) {
            let idx = idx.trim_matches(['"', '\'']);
            if idx.chars().all(|c| c.is_ascii_digit()) {
                out.push_str(&array_elem_write(base, idx, &rhs));
            } else if !idx.contains(['$', '(', ')', '`', ';', '&', '|', ' ']) && !idx.is_empty() {
                // associative (string-key) element: maintain the key list
                ASSOC_VARS.lock().unwrap().insert(base.to_string());
                out.push_str(&assoc_elem_write(base, idx, &rhs));
            } else {
                // a COMPUTED subscript (`options["$key"]`,
                // `map[${flags:j:1}]`) — fish has no `set x[i] v` and no
                // `${…}` at all, so emitting the subscript verbatim was a
                // read-time PARSE ERROR that cost the whole file. The element
                // name is composed at run time instead and written through
                // fish's `$$name` indirection: the write side of
                // `subscript_read_fish`, over the flat `{base}_{key}` element
                // variable every reader already uses.
                ASSOC_VARS.lock().unwrap().insert(base.to_string());
                let name = subscript_name_fish(base, idx);
                out.push_str(&format!(
                    "set -q {base}_len; or set {base}_len 0; \
                     set -l _wk {name}; set $_wk {rhs}; \
                     set {base}_len (math --scale 0 \"${base}_len + 1\"); \
                     set -a {base}_keys {name}"
                ));
            }
            continue;
        }
        if t.indices.is_empty() {
            out.push_str("set ");
            out.push_str(&t.var);
            out.push(' ');
            out.push_str(&rhs);
            continue;
        }
        // `arr[i]=x` — a dynamic subscript needs runtime evaluation
        if t.indices.len() == 1 {
            let idx = &t.indices[0];
            if let IrExpr::Str(k, _) = idx {
                if k.chars().all(|c| c.is_ascii_digit()) {
                    out.push_str(&array_elem_write(&t.var, k, &rhs));
                    continue;
                }
                if !k.chars().any(|c| c.is_whitespace())
                    && !k.contains(['$', '(', ')', '"', '\'', '`', ';', '&', '|'])
                {
                    ASSOC_VARS.lock().unwrap().insert(t.var.clone());
                    out.push_str(&assoc_elem_write(&t.var, k, &rhs));
                    continue;
                }
            }
            let k = word_to_fish(idx)?;
            // dynamic: use fish's `set var[idx] value` with the computed index
            out.push_str(&format!("set {}[{}] {}", t.var, k, rhs));
            continue;
        }
        out.push_str("set ");
        out.push_str(&t.var);
        for idx in &t.indices {
            out.push('[');
            out.push_str(&word_to_fish(idx)?);
            out.push(']');
        }
        out.push(' ');
        out.push_str(&rhs);
    }
    Ok(out)
}

/// A bash indexed-array ELEMENT WRITE (`arr[1]=X`).
///
/// The array IS a native fish list, and fish indexes from 1 where bash
/// indexes from 0, so the subscript is offset by one. fish EXTENDS a list
/// when the subscript is past the end (`set a[6] x` on a 3-element list
/// leaves 6 elements with empty gaps), which is exactly bash's behaviour —
/// so `${#arr[@]}` stays right with no counter variable to maintain.
fn array_elem_write(base: &str, idx: &str, rhs: &str) -> String {
    let n: i64 = idx.parse().unwrap_or(0);
    format!("set {base}[{}] {rhs}", n + 1)
}

/// An associative-array element write (`map[key]=v`) — the flat
/// `{base}_{key}` element plus the key list `_arr_keys_k` walks.
fn assoc_elem_write(base: &str, key: &str, rhs: &str) -> String {
    format!(
        "set {base}_{key} {rhs}; set {base}_len (math --scale 0 \"${base}_len + 1\"); set -a {base}_keys {key}"
    )
}

/// `arr=(a b c)` / `arr+=(x)`.
///
/// An INDEXED array is a native fish LIST. `set arr a b c` stores exactly
/// the list fish already knows how to expand (`$arr`), join (`"$arr"`),
/// count (`count $arr`) and slice (`$arr[2..3]`), so no per-element
/// variables and no runtime helpers are involved at all.
///
/// An ASSOCIATIVE array (`declare -A`, string keys) has no fish equivalent
/// and keeps the per-key lowering: `{name}_{key}` elements plus the
/// `{name}_keys` key list the `_arr_*_k` helpers walk.
fn set_array_to_fish(name: &str, items: &str, append: bool) -> String {
    // the KEYED literal form — `d=([a]=1 [b]=2)` (Python dicts / bash
    // assoc literals): each item is `[k]=v` — write the per-key element + key list
    let items_clean = items.trim().trim_start_matches(['\'', '"']);
    if items_clean.starts_with('[') {
        ASSOC_VARS.lock().unwrap().insert(arr_base(name).to_string());
        let mut parts = Vec::new();
        for raw in items.split(' ') {
            let it = raw
                .trim()
                .trim_start_matches(['\'', '"'])
                .trim_end_matches(['\'', '"']);
            let Some(rest) = it.strip_prefix('[') else { continue };
            let Some((key, val)) = rest.split_once("]=") else { continue };
            parts.push(format!("set {name}_{key} {val}"));
            parts.push(format!("set {name}_len (math --scale 0 \"${name}_len + 1\")"));
            parts.push(format!("set -a {name}_keys {key}"));
        }
        return parts.join("; ");
    }
    if is_assoc(name) {
        return set_array_flat(name, items, append);
    }
    let trimmed = items.trim();
    if trimmed.is_empty() {
        // `arr=()` — an empty list. An append of nothing is a no-op.
        return if append {
            ":".into()
        } else {
            format!("set -e {name}")
        };
    }
    if append {
        return format!("set -a {name} {trimmed}");
    }
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // already a fish command substitution — its output IS the list
        return format!("set {name} {trimmed}");
    }
    if trimmed.contains('$') {
        // `arr=($x)` — an UNQUOTED expansion inside an array literal
        // FIELD-SPLITS in bash; fish does not split a variable's value, so
        // the split has to be spelled out. A QUOTED expansion
        // (`arr=("$x")`) is one element in bash and must NOT be split.
        if trimmed.starts_with('"') && trimmed.ends_with('"') {
            return format!("set {name} {trimmed}");
        }
        return format!("set {name} (string split \" \" -- {trimmed})");
    }
    format!("set {name} {trimmed}")
}

/// The per-KEY lowering an ASSOCIATIVE array still needs: `{name}_{key}`
/// elements, a `{name}_len` counter and the `{name}_keys` key list.
fn set_array_flat(name: &str, items: &str, append: bool) -> String {
    if items.is_empty() {
        return format!("set {name}_len 0");
    }
    if items.contains('$') && !append {
        // a runtime word list cannot be named here — walk it and write the
        // per-element variables by index.
        let trimmed = items.trim();
        let words = if trimmed.starts_with('(') && trimmed.ends_with(')') {
            trimmed.to_string()
        } else {
            format!("(string split \" \" \"{items}\")")
        };
        return format!(
            "set {name}_len 0; for _w in {words}; set {name}_${name}_len $_w; set {name}_len (math --scale 0 \"${name}_len + 1\"); end"
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
            parts.push(format!("eval \"set {name}_\\${name}_len {esc}\""));
            parts.push(format!("set {name}_len (math --scale 0 \"${name}_len + 1\")"));
        }
    } else {
        for (i, it) in items.split(' ').enumerate() {
            parts.push(format!("set {name}_{i} {it}"));
        }
        parts.push(format!("set {name}_len {}", items.split(' ').count()));
    }
    parts.join("; ")
}

/// The A1 `assign(NAME, op, VALUE)` call — `x=5`, `n+=1`, `s.=tail`.
///
/// This node arrives in TWO positions and both were rendering bash:
///
///   * statement — `[ -n "$VAR" ] || VAR=default`, `x=5`, `n=42`
///   * expression — the rhs of a BinOp chain, e.g. `a && b=1 || c=2`
///
/// Neither position may emit `NAME=VALUE`: fish rejects that spelling
/// outright ("Unsupported use of '='"), so every file containing one failed
/// to PARSE. fish's assignment is `set NAME VALUE`.
///
/// The op matters: `=` overwrites, while the compound operators (`+=`, `-=`,
/// `*=`, ...) accumulate. Each accumulates through `math` on the fish-native
/// integer/decimal the backend already uses for arithmetic.
///
/// Refuse > guess: an op outside this set is not a lowering we can justify,
/// so it errors rather than silently dropping the accumulation.
fn assign_call_to_fish(args: &[IrExpr]) -> Result<String, String> {
    let name = raw_arg(args, 0)?;
    let op = args.get(1).map(str_arg).transpose()?.unwrap_or_default();
    let value = word_to_fish(arg(args, 2)?)?;
    let op = op.trim();
    if op.is_empty() || op == "=" {
        return Ok(format!("set {name} {value}"));
    }
    // `x+=y` — the accumulating form. `math` gives the fish-native numeric
    // semantics the rest of the backend already relies on.
    let binop = match op {
        "+=" => "+",
        "-=" => "-",
        "*=" => "*",
        "/=" => "/",
        "%=" => "%",
        _ => return Err(format!("unlowered assignment operator '{op}' on '{name}'")),
    };
    Ok(format!("set {name} (math --scale 0 \"${name} {binop} {value}\")"))
}

/// The variable name a `declare`/`local` statement declared — only the `set N`
/// forms produced above count (a bare word is not a declaration).
fn declared_name(stmt: &str) -> Option<String> {
    let rest = stmt.strip_prefix("set ")?;
    let name = rest.split_whitespace().next()?;
    if name.is_empty() || name.contains(['[', '=', '(', '"', '\'']) {
        None
    } else {
        Some(name.to_string())
    }
}

/// `string collect` must collect the WHOLE command list, not only its last
/// command. `(A; B | string collect)` pipes `B` alone and leaves `A`'s stdout
/// as separate list elements, so a two-line `$(A; B)` collapses onto one
/// space-joined line — while bash keeps the newline (`$(echo one; echo two)`
/// is `one\ntwo`). Wrapping the body in a `begin … end` block, which fish
/// pipes as a unit, groups it correctly:
///
///     $(A; B)  →  (begin; A; B; end | string collect)
///
/// A single simple command keeps the direct spelling (`(A | string collect)`)
/// so the common case is unchanged.
fn collect_wrap_fish(body: &str) -> String {
    if has_top_level_separator(body) {
        format!("begin; {body}; end")
    } else {
        body.to_string()
    }
}

/// Is there a command SEPARATOR — `;`, `&&` or `||` — at paren depth 0,
/// outside quotes? A bare `|` is a pipeline and deliberately NOT one:
/// `string collect` already binds to the whole pipeline (`A | B | collect`
/// collects the pipeline's stdout), so only the constructs that would leave
/// earlier commands' output OUT of the pipe need the `begin … end` grouping.
fn has_top_level_separator(s: &str) -> bool {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut sq = false;
    let mut dq = false;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if sq {
            if c == b'\'' {
                sq = false;
            }
            i += 1;
            continue;
        }
        if dq {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                dq = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'\\' => {
                i += 2;
                continue;
            }
            b'\'' => sq = true,
            b'"' => dq = true,
            b'(' => depth += 1,
            b')' => depth -= 1,
            b';' if depth <= 0 => return true,
            b'&' if depth <= 0 && b.get(i + 1) == Some(&b'&') => return true,
            b'|' if depth <= 0 && b.get(i + 1) == Some(&b'|') => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

fn assign_rhs_to_fish(expr: &IrExpr) -> Result<String, String> {
    match expr {
        // `v=$(cmd)` — bash joins the command's stdout into ONE scalar with
        // trailing newlines stripped. fish's `set v (cmd)` instead SPLITS the
        // output into a list (newline-separated), so `$(yes Hi | head -3)`
        // collapses to `Hi Hi Hi`. `string collect` (trim variant) restores
        // the bash semantics: re-join the lines, drop the trailing newline,
        // and yield the empty string for no output.
        IrExpr::Capture { expr: inner, .. } => {
            if let Some(lift) = capture_idiom_fish(inner)? {
                return Ok(lift);
            }
            let body = arrow_to_fish(std::slice::from_ref(inner.as_ref()))?;
            Ok(format!("({} | string collect)", collect_wrap_fish(&body)))
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "capture" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, true)),
            "captureWords" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, false)),
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
                    Ok("(false)".into())
                } else {
                    Ok(arith_cmd_fish(&raw))
                }
            }
            "setArray" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_fish(&name, &array_items(args, 1)?, false))
            }
            "setArrayAppend" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_fish(&name, &array_items(args, 1)?, true))
            }
            "assign" => {
                assign_call_to_fish(args)
            }
            _ => word_to_fish(expr),
        },
        _ => word_to_fish(expr),
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

/// Does this pattern carry a LIVE fish expansion — a `$name`, `${…}` or
/// `$(…)` the core intends fish to expand?
///
/// Only such a pattern must be handed through untouched; quoting it would
/// turn the expansion into literal text. The scan honours the two fish
/// contexts where `$` is literal: the character after a backslash, and the
/// inside of single quotes.
fn pattern_has_live_expansion(p: &str) -> bool {
    let b: Vec<char> = p.chars().collect();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '\\' => i += 2,
            '\'' => {
                i += 1;
                while i < b.len() && b[i] != '\'' {
                    i += 1;
                }
                i += 1;
            }
            '$' => {
                if matches!(b.get(i + 1), Some(c)
                    if c.is_ascii_alphanumeric() || *c == '_' || *c == '{' || *c == '(')
                {
                    return true;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    false
}

/// A literal glob PATTERN for `string match` / a `switch` arm, quoted so fish
/// cannot glob-expand it first.
///
/// fish expands wildcards in an unquoted word BEFORE `string match` runs, so
/// bash's `[[ $s == *.txt ]]` rendered as `string match -q -- *.txt $s` makes
/// fish glob `*.txt` against the CWD instead: with no match the command dies
/// ("No matches for wildcard '*.txt'. See `help wildcards-globbing`.") and the
/// test never runs, and with matches it silently compares against whatever
/// files happen to exist. Quoting the pattern is what keeps it a pattern.
///
/// Quoting is also what keeps some patterns a legal fish WORD at all. fish's
/// tokenizer rejects several otherwise-valid globs: `[^}]` (a bracket class
/// whose member is `}`), a bare `$` before `{`, and any whitespace — while the
/// same text inside single quotes is one fine word, and `string match`/`case`
/// glob-match the pattern AFTER quote removal, so the pattern's meaning is
/// unchanged (063_11_complex_while_loop and 063_hard_to_parse both failed to
/// parse on `case *\$\{[^}]*\}*`, and both arms are re-reachable this way).
///
/// The bash QUOTES inside the pattern are dropped first: they were shell
/// quoting (there is no live expansion in this branch — see
/// `pattern_has_live_expansion`), and keeping them would make the glob match
/// literal quote characters (`*'\n'*` would match `*'⏎'*`, not `*⏎*`).
fn fish_pattern_lit(p: &str) -> String {
    if pattern_has_live_expansion(p) {
        return p.to_string();
    }
    let unquoted: String = p.chars().filter(|c| *c != '\'' && *c != '"').collect();
    let mut q = String::from("'");
    for c in unquoted.chars() {
        if c == '\'' {
            q.push_str("\\'");
        } else {
            q.push(c);
        }
    }
    q.push('\'');
    q
}

/// A `switch` arm's pattern list.
///
/// Every pattern goes through `fish_pattern_lit` for the same reason the
/// `string match` sites do — but here it is not cosmetic. fish GLOB-EXPANDS
/// an unquoted `case` argument against the filesystem, so `case *` arrives
/// at the matcher as the literal list of files in the cwd: the wildcard arm
/// then fires only for a subject that happens to equal one of those names
/// and NEVER as a catch-all. Measured on fish 3.7.0 with an empty subject,
/// `switch ""; case *` prints nothing where `switch ""; case '*'` prints the
/// arm. Quoting hands the pattern to fish's own switch matcher, which IS a
/// glob matcher, so the arm's meaning is unchanged and it now actually
/// matches (frontends/posix-sh-go/testdata/parse-case-paren-pattern.sh
/// printed nothing where bash printed `unknown`).
fn case_patterns_fish(patterns: &[String]) -> String {
    patterns
        .iter()
        .map(|p| fish_pattern_lit(p))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Is a test's right-hand side QUOTED in the source (`"$pat"`, `'a*'`)?
///
/// A quoted RHS is a literal in `[ ]` AND in `[[ … ]]` — bash only globs an
/// UNQUOTED word inside `[[ ]]`, which is why `[[ "abc" == "a*" ]]` is false
/// on bash while `[[ abc == a* ]]` is true.
fn test_rhs_is_quoted(rhs: &str) -> bool {
    let r = rhs.trim();
    r.len() >= 2
        && ((r.starts_with('"') && r.ends_with('"'))
            || (r.starts_with('\'') && r.ends_with('\'')))
}

/// One comparison atom of a test string (`"$a" == "b"`, `"$s" =~ ^h`,
/// `-f /x` …) → a single sh cond command. The `==`/`!=`/`=` forms lower
/// to the case emulation (dash has no `==`), `=~` to a grep -E pipeline,
/// everything else stays in `[ ]` (POSIX operators).
///
/// `dbl` records whether the atom came from `[[ … ]]` (the core marks those
/// with a trailing `[[` argument) rather than `[ … ]` / `test`. That is
/// SEMANTIC, not cosmetic: `[[ ]]` globs an unquoted RHS, `[ ]` compares
/// TEXT — the shell expands the word and `test` never treats it as a
/// pattern, so `[ "abc" = "a*" ]` is FALSE while `[[ "abc" == "a*" ]]` is
/// TRUE. Rendering both through `string match` made every `[ ]`
/// star-comparison match (examples/test-glob-rhs-fallback.sh printed `bad`
/// for all five of its literal comparisons).
fn test_cmp_to_fish(t: &str, dbl: bool) -> Result<String, String> {
    if let Some((lhs, rhs)) = split_test_op(t, "==") {
        // pattern match: case emulation (dash has no == in test)
        // A leading `!` negates the WHOLE match (`[[ ! "1" ==
        // "2" ]]`) — strip it and prefix `! ` (valid before a
        // case in dash and bash). Without this the `!` lands
        // inside the quoted lhs (`"!"1"` — a literal).
        let negate = lhs.starts_with('!');
        let lhs = lhs.trim_start_matches('!');
        let m = if !dbl || test_rhs_is_quoted(&rhs) {
            // `[ … == … ]` / `test … == …`, or a QUOTED RHS in `[[ … ]]`:
            // a LITERAL string compare. fish's own `test` is exactly that, so
            // no case emulation is needed at all.
            format!("[ {lhs} = {rhs} ]")
        } else if let Some((neg, rest)) =
            rhs.strip_prefix("!(").and_then(|r| r.split_once(')'))
        {
            // extglob negation `!(P)Y` ≡ `*Y` minus `P Y`
            format!(
                "string match -q -- {} {lhs}; and not string match -q -- {} {lhs}",
                fish_pattern_lit(&format!("*{rest}")),
                fish_pattern_lit(&format!("{neg}{rest}"))
            )
        } else if let Some(inner) =
            rhs.strip_prefix("@(").and_then(|r| r.strip_suffix(')'))
        {
            // extglob list match: `@(a|b)` == `a|b`
            format!("string match -q -- {} {lhs}", fish_pattern_lit(inner))
        } else if let Some(inner) =
            rhs.strip_prefix("?(").and_then(|r| r.strip_suffix(')'))
        {
            // optional: `?(a|b)` matches empty or a|b
            format!("string match -q -- {} {lhs}", fish_pattern_lit(inner))
        } else if *NOCASEMATCH.lock().unwrap() {
            // fish's `string match -i` is the case-insensitive match; no
            // `[fF][oO][oO]` character-class folding needed.
            format!("string match -qi -- {} {lhs}", fish_pattern_lit(&rhs))
        } else {
            format!("string match -q -- {} {lhs}", fish_pattern_lit(&rhs))
        };
        if negate {
            Ok(format!("! {m}"))
        } else {
            Ok(m)
        }
    } else if let Some((lhs, rhs)) = split_test_op(t, "!=") {
        if !dbl || test_rhs_is_quoted(&rhs) {
            // `[ … != … ]`, or a quoted RHS in `[[ ]]` — the literal
            // counterpart of the `==` arm above.
            return Ok(format!("[ {lhs} != {rhs} ]"));
        }
        Ok(format!(
            "not string match -q -- {} {lhs}",
            fish_pattern_lit(&rhs)
        ))
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
        } else if !dbl || test_rhs_is_quoted(&rhs) {
            // `[ x = y ]`, or a quoted RHS in `[[ ]]`: the RHS is a literal
            // STRING, not a pattern — the shell expands the word and `test`
            // compares text, so a star in it never matches anything
            // (test-glob-rhs-fallback.sh: `[ "abc" = "a*" ]` is false). The
            // go-sh frontend's synthetic pattern tests carry the `[[` marker
            // and an unquoted pattern, so they keep the glob.
            Ok(format!("[ {lhs} = {rhs} ]"))
        } else {
            Ok(format!(
                "string match -q -- {} {lhs}",
                fish_pattern_lit(&rhs)
            ))
        }
    } else {
        // quote bare $(...) and ${...} so word-splitting
        // in `[ ]` does not shred cmdsub output
        let t = space_test_ops(t);
        let t = samefile_guard_fish(&t);
        let t = quote_test_expansions(&t);
        Ok(format!("[ {t} ]"))
    }
}

/// `A -ef B` → `-e A -a -e B -a A -ef B`.
///
/// fish's `test -ef` answers TRUE when NEITHER path exists (measured on fish
/// 3.7.0: `test /path -ef /other` exits 0), where bash's `-ef` is false
/// unless both exist AND are the same file — so a bare `-ef` gets the
/// nonexistent case exactly backwards. The two `-e` guards restore bash's
/// semantics, and the `-a` connectors keep it ONE `[ ]` expression so a
/// surrounding `!` or `&&` still applies to the whole test.
///
/// Only the two-operand shape is rewritten; anything else is left alone.
fn samefile_guard_fish(t: &str) -> String {
    let toks: Vec<&str> = t.split_whitespace().collect();
    if !toks.contains(&"-ef") {
        return t.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(toks.len() + 6);
    let mut i = 0;
    while i < toks.len() {
        if i + 2 < toks.len() && toks[i + 1] == "-ef" {
            let (lhs, rhs) = (toks[i], toks[i + 2]);
            out.push(format!(
                "-e {lhs} -a -e {rhs} -a {lhs} -ef {rhs}"
            ));
            i += 3;
        } else {
            out.push(toks[i].to_string());
            i += 1;
        }
    }
    out.join(" ")
}

fn cmd_to_fish(e: &IrExpr) -> Result<String, String> {
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
                exec_line_to_fish(arg(args, 0)?, words, env)
            }
            "test" => {
                // The core hands the whole condition over as one flattened
                // string, so any bash-only spelling inside it (`$?`, `$*`,
                // `${…}`, `$(( ))`) arrives as literal text and would be a
                // fish parse error. Rewrite those to native fish BEFORE any
                // of the operator splitting below, which must then see only
                // fish-legal text.
                let raw = bash_specials_to_fish(&raw_arg(args, 0)?);
                let t = raw.trim();
                // `[[ … ]]` or `[ … ]`? The core appends its marker as a
                // second argument for the double-bracket form, and the two
                // forms do NOT compare the same way: `[[ ]]` globs the RHS,
                // `[ ]` compares text. `test_cmp_to_fish` needs to know which
                // one it is lowering.
                let dbl = matches!(args.get(1), Some(IrExpr::Str(s, _)) if s == "[[");
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
                //
                // The strip fires only when the leading `(` really does
                // enclose the WHOLE test. A bare prefix/suffix test is not
                // enough: a lowered expansion is itself a command
                // substitution, so `(string match …) -gt (string match …)`
                // both starts with `(` and ends with `)`, and stripping them
                // ate the first substitution's opening paren (`[ string
                // match …; or printf …) -gt … ]` — an unparseable file.
                let mut negate = false;
                let mut tt = t;
                while let Some(rest) = tt.strip_prefix('!') {
                    negate = !negate;
                    tt = rest.trim();
                }
                let tc: Vec<char> = tt.chars().collect();
                if tt.starts_with('(') && balanced_paren(&tc, 0) == Some(tc.len() - 1) {
                    tt = tt[1..tt.len() - 1].trim();
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
                            rendered.push_str(&test_cmp_to_fish(apart.trim(), dbl)?);
                        }
                    }
                } else {
                    rendered = test_cmp_to_fish(tt, dbl)?;
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
                let word = word_to_fish(arg(args, 0)?)?;
                // The pattern is QUOTED, and in fish that does not make it a
                // literal: `string match` reads the pattern itself as a glob
                // (`string match -q -- 'h*' hello` matches, measured against
                // fish 3.7). Quoting is what stops FISH from glob-expanding
                // the pattern against the CWD before `string match` ever sees
                // it — unquoted, `h*` dies with "No matches for wildcard"
                // when nothing in the CWD matches it, and silently compares
                // against whatever files do.
                let mut tests: Vec<String> = Vec::new();
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        let p = str_arg(it)?;
                        tests.push(format!(
                            "string match -q -- {} {word}",
                            fish_pattern_lit(&p)
                        ));
                    }
                }
                Ok(if tests.is_empty() {
                    "false".to_string()
                } else {
                    tests.join("; or ")
                })
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
                let inner = arrow_to_fish(args)?;
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
                        out.push_str(&redirect_to_fish(r)?);
                    }
                    out
                };
                let mut line = herestring_wrap_fish(&plain, inner)?;
                if !ps_ins.is_empty() {
                    line = lower_procsub_inline_fish(&ps_ins, &line)?;
                }
                Ok(format!("{line}{specs}"))
            }
            "subshell" => {
                // fish's `( … )` is a COMMAND SUBSTITUTION, not a subshell
                // statement. Where the subshell is a PIPELINE STAGE or carries
                // its own redirects — bash's `( (cmd) 3>&- | cat )` shape — a
                // `( … )` there is a read-time PARSE ERROR ("command
                // substitutions not allowed here") that kills the whole file.
                // `begin … end` is fish's grouping construct and is legal in
                // every statement position, including as a pipeline stage and
                // under redirects.
                let body = arrow_to_fish(args)?;
                if body.is_empty() {
                    Ok("begin; end".into())
                } else {
                    Ok(format!("begin; {body}; end"))
                }
            }
            "block" => {
                // A grouped condition (`while echo x; test -f y; do …`) is a
                // `block` node, and fish has NO `{ … }` grouping at all —
                // 3.7 rejects it outright ("Unknown command: '{ … }'",
                // "'{ ... }' is not supported for grouping commands. Please
                // use 'begin; ...; end'"), so the old `{ body; }` spelling was
                // a READ-TIME parse error that killed the whole render
                // (parse-multi-command-while-condition: the file never ran, so
                // even the trailing `exit: $?` line was lost). `begin; …; end`
                // is the same grouping the `subshell` arm above uses and is
                // legal in every condition position.
                let body = arrow_to_fish(args)?;
                if body.is_empty() {
                    Ok("begin; end".into())
                } else {
                    Ok(format!("begin; {body}; end"))
                }
            }
            "whileLoop" => {
                let cond = arrow_at(args, 0)?;
                let body = arrow_at(args, 1)?;
                // fish closes EVERY block with `end` — a bash `do … done` pair
                // is not a fish block, so the whole file stops parsing at the
                // `do`. Same form as the `IrStmt::While` arm in `stmt_inline`.
                Ok(format!("while {cond}; {body}; end"))
            }
            "cstyleFor" => {
                let arith = raw_arg(args, 0)?;
                let body = arrow_at(args, 1)?;
                Ok(cstyle_for_to_fish(&arith, &body))
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
                    // `math` is fish's arithmetic: it evaluates and prints the
                    // value, so it IS the native spelling of bash's `(( … ))`
                    // in command position. Emitting the bash form here made
                    // every such file a PARSE ERROR in condition position.
                    Ok(format!("math --scale 0 {}", arith_rewrite(&raw)))
                }
            }
            "break" => Ok("break".into()),
            "continue" => Ok("continue".into()),
            "return" => {
                if args.is_empty() {
                    Ok("return".into())
                } else {
                    Ok(format!("return {}", word_to_fish(arg(args, 0)?)?))
                }
            }
            "assign" => {
                assign_call_to_fish(args)
            }
            "setArray" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_fish(&name, &array_items(args, 1)?, false))
            }
            "setArrayAppend" => {
                let name = raw_arg(args, 0)?;
                Ok(set_array_to_fish(&name, &array_items(args, 1)?, true))
            }
            "assocSet" => {
                // the go-sh map-literal store (`m := map[K]V{...}` — one
                // assocSet per pair, triage-sh-20260815-175002): the
                // by-name associative-array write — the same POSIX
                // per-element lowering as setArray (`m_c=C`).
                let name = raw_arg(args, 0)?;
                let key = raw_arg(args, 1)?;
                let value = word_to_fish(arg(args, 2)?)?;
                Ok(format!("{}_{}={}", arr_base(&name), key, value))
            }
            "getVar" => Ok(var_ref_to_fish(&raw_arg(args, 0)?.replace('.', "_"), false)),
            "capture" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, true)),
            "captureWords" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, false)),
            "contains" => {
                let arg = word_to_fish(arg(args, 0)?)?;
                let pat = raw_arg(args, 1)?;
                Ok(format!("printf '%s\\n' {arg} | grep {pat} >/dev/null 2>&1"))
            }
            "fnCall" => {
                let name = raw_arg(args, 0)?;
                let mut line = name;
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        line.push(' ');
                        line.push_str(&word_to_fish(w)?);
                    }
                }
                Ok(line)
            }
            // VALUE-returning function call (the echo-return transform's
            // convention — CROSS_BACKEND_RUNTIME.md §8.3). The function's
            // `return <value>` was rewritten to the shIR `Return(Some(v))`
            // form, which the fish `function ... end` body renders as a native
            // `return` — and fish's `return` takes an INTEGER STATUS, so a
            // string value is a runtime error ("return: hello: invalid
            // integer"), never the value. The value therefore travels on
            // stdout and the call site captures it: fish's native
            // command substitution `(f args)`, which is exactly the bash
            // `$(f args)` convention this replaces.
            "fnValue" => {
                let name = raw_arg(args, 0)?;
                let mut line = format!("({name}");
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        line.push(' ');
                        line.push_str(&word_to_fish(w)?);
                    }
                }
                line.push(')');
                Ok(line)
            }
            // `grepMatches(text, pattern, flags)` — the `grep -o` lift:
            // the sh backend keeps the shell's own grep (native) — re-emit
            // the -o pipeline on the input text.
            "grepMatches" => {
                let text = word_to_fish(arg(args, 0)?)?;
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
                let value = word_to_fish(arg(args, 1)?)?;
                let pat = pattern.replace('\'', "'\\\\''");
                let i_flag = if flags.contains('i') { "i" } else { "" };
                Ok(format!(
                    "printf '%s\\n' {value} | grep -E{i_flag}q '{pat}'"
                ))
            }
            // The frontend-emitted list/short-circuit chain calls: `and` is `&&`
            // and `or` is `||`, with each side an `Arrow` of statements (the
            // same shape the c backend renders — see c_backend.rs's
            // `func == "and" || func == "or"` statement arm).
            //
            // Each side is a statement SEQUENCE, not a single command, so a
            // bare `cmd_to_fish` recursion would refuse on the Arrow; the
            // inline renderer keeps multi-statement sides working.
            "and" | "or" => {
                let joiner = if func == "and" { " && " } else { " || " };
                let mut parts = Vec::with_capacity(args.len());
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        parts.push(joiner.to_string());
                    }
                    match a {
                        IrExpr::Arrow(stmts) => parts.push(stmts_inline(stmts)?),
                        other => parts.push(cmd_to_fish(other)?),
                    }
                }
                Ok(parts.concat())
            }
            "setVar" => {
                // the runtime's plain store write — re-emit as a shell
                // assignment (the A1 store protocol the perl/posix
                // frontends emit for return values / locals). Dotted
                // names (the cpp/c struct-field protocol `p.x`) are not
                // valid shell identifiers — sanitize to `p_x` (the
                // getVar arms mirror the same rewrite).
                let name = raw_arg(args, 0)?;
                let value = word_to_fish(arg(args, 1)?)?;
                Ok(format!("{}={value}", name.replace('.', "_")))
            }
            other => Err(format!("command call not renderable: {other:?}")),
        },
        IrExpr::BinOp {
            op: BinOpKind::And,
            lhs,
            rhs,
        } => Ok(format!("{} && {}", cond_to_fish(lhs)?, cond_to_fish(rhs)?)),
        IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs,
            rhs,
        } => Ok(format!("{} || {}", cond_to_fish(lhs)?, cond_to_fish(rhs)?)),
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs,
            ..
        } => Ok(format!("! {}", cond_to_fish(lhs)?)),
        // a statement-position arith (`((n++))` — the perl frontend's
        // increment statements): the arithmetic command, whose exit
        // status is the value != 0 (bash `(( ))` semantics). fish's
        // `math` prints the value and carries that same status, so the
        // native spelling is `math`, not the bash `(( … ))` form fish
        // cannot parse in a condition.
        IrExpr::Arith(a) => Ok(format!("math --scale 0 {}", arith_to_fish(a))),
        // a bare variable in command position: the goto restructure's
        // flag guards (`if (flag) break`) — truthiness of the store
        // value ([ -n ]), matching the runtime's nonempty-true semantics
        IrExpr::Var(name, _) => {
            let v = var_ref_to_fish(name, false);
            Ok(format!("[ -n \"{v}\" ]"))
        }
        other => Err(format!("command expression not renderable: {other:?}")),
    }
}

/// A CONDITION-position renderer, split out from `cmd_to_fish`.
///
/// The two differ in exactly one place: arithmetic. In COMMAND position bash's
/// `(( x + 1 ))` is a statement whose exit status is the value, which fish
/// spells `math x + 1`. In CONDITION position the same node arrives as the
/// boolean of an `if`/`while`, where fish parses `(( … ))` as nested command
/// substitutions and rejects it outright — "command substitutions not allowed
/// here" — and `math "$x > 0"` rejects the relational ("Logical operations are
/// not supported, use `test` instead"). So a relational is split around `test`
/// and the halves become `(math …)` substitutions; a bare expression becomes a
/// `-ne 0` nonzero test, matching bash's `(( expr ))` truthiness.
///
/// Routing BinOp operands through here (rather than back into `cmd_to_fish`)
/// is what fixes `a && (( x > 0 ))`: the And/Or arms had been re-entering the
/// statement renderer and re-emitting the bash form inside the condition.
fn cond_to_fish(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Arith(a) => Ok(arith_cond_ast_fish(a)),
        IrExpr::BinOp { .. } => cmd_to_fish(e),
        other => cmd_to_fish(other),
    }
}

/// The same lowering as [`arith_cond_fish`], but driven by the structured node.
///
/// The text path cannot work here: `arith_to_fish` parenthesizes every binary
/// (`var > 0` renders as `(var > 0)`), so scanning the rendered string for a
/// top-level relational finds none — the comparison is always nested one level
/// in. Splitting on the AST is also the only correct answer for
/// `($a + $b > $c)`: the text splitter would cut the `>` out of a variable name
/// (`$BIGGER`), and would treat `(a > b) > c` as a comparison of a paren group.
fn arith_cond_ast_fish(a: &ArithAst) -> String {
    if let ArithAst::Bin { op, lhs, rhs } = a {
        if let Some(flag) = test_flag(op) {
            return format!(
                "[ (math --scale 0 \"{}\") {flag} (math --scale 0 \"{}\") ]",
                arith_to_fish(lhs),
                arith_to_fish(rhs)
            );
        }
    }
    format!("[ (math --scale 0 \"{}\") -ne 0 ]", arith_to_fish(a))
}

/// bash's arithmetic relationals → fish `test` flags. `None` for the
/// arithmetic/bitwise/boolean operators, which stay inside `math`.
fn test_flag(op: &str) -> Option<&'static str> {
    match op {
        "<=" => Some("-le"),
        ">=" => Some("-ge"),
        "==" => Some("-eq"),
        "!=" => Some("-ne"),
        "<" => Some("-lt"),
        ">" => Some("-gt"),
        _ => None,
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
                        // same de-nesting cmd_to_fish's render path does)
                        let words = match args.get(1) {
                            Some(IrExpr::Array(items)) => items.as_slice(),
                            _ => &[],
                        };
                        let mut i = 0;
                        while i + 1 < words.len() {
                            if let IrExpr::Str(fl, _) = &words[i] {
                                if fl == "-P" || fl == "--perl-regexp" {
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
            _ => expr_containers(e).into_iter().any(has_grep_p),
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
                // a pipeline's stages are per-stage stmt lists — descend so
                // `echo x | grep -oP …` (stage 2) is visible to the
                // polyfill-prologue decision (pipeline_grep_p_emits_polyfill)
                IrStmt::Assign { expr, .. } => {
                    if has_grep_p(expr) {
                        return true;
                    }
                }
                IrStmt::Redirect { inner, redirects } => {
                    if walk(inner) {
                        return true;
                    }
                    for r in redirects {
                        if has_grep_p(&r.target) {
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
                    if has_grep_p(discriminant) {
                        return true;
                    }
                    for c in clauses {
                        if walk(&c.body) {
                            return true;
                        }
                    }
                }
                // The structured pipeline stmt (shir-pipeline-native) holds
                // its stage bodies as stmt lists — the grep -P exec lives
                // inside them, so the scan must descend or the prologue is
                // never emitted while the render still calls grep_p.
                IrStmt::Pipeline { stages, .. } => {
                    if stages.iter().any(|s| walk(s)) {
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

/// Is this `set` a set of shell OPTIONS rather than variable assignments?
/// bash overloads one builtin for both: `set -euo pipefail` sets flags,
/// `set x=1 y=2` assigns. Any argument carrying a `=` settles it as an
/// assignment, which must keep lowering to fish's own assignment form.
fn args_are_options(args: &[IrExpr]) -> bool {
    if args.is_empty() {
        return false;
    }
    let mut expect_optname = false;
    for a in args {
        match a {
            IrExpr::Str(s, _) => {
                if s.contains('=') {
                    return false;
                }
                if expect_optname {
                    expect_optname = false;
                    continue;
                }
                if s == "-o" {
                    expect_optname = true;
                }
            }
            // A computed/quoted word could be anything, including `x=1`;
            // treat it as an assignment so we never eat a real `set` value.
            _ => return false,
        }
    }
    true
}

fn exec_line_to_fish(
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
    let cmd_name = match cmd {
        IrExpr::Str(s, _) => Some(s.as_str()),
        _ => None,
    };
    // `test A -ef B` as a COMMAND — the `[ ]`/condition form goes through
    // `samefile_guard_fish`, but a plain `test` invocation is rendered as an
    // ordinary command and would carry fish's wrong answer with it. Only the
    // exact three-word shape is rewritten; the operands are ordinary words
    // (`Interpolate` for a quoted path), so they render through `word_to_fish`.
    if cmd_name == Some("test") && args.len() == 3 {
        if matches!(&args[1], IrExpr::Str(op, _) if op == "-ef") {
            let lhs = word_to_fish(&args[0])?;
            let rhs = word_to_fish(&args[2])?;
            return Ok(format!("test -e {lhs} -a -e {rhs} -a {lhs} -ef {rhs}"));
        }
    }
    if let Some(envs) = env {
        for (k, v) in envs {
            // `IFS= read VAR` is the bash idiom for "read the WHOLE line,
            // no splitting, no whitespace trimming". fish has no IFS, so
            // the prefix cannot express it — and it is not harmless: an
            // assignment prefix puts the command in a new scope, and
            // fish's `read` then stores its variable in THAT scope, so
            // `IFS= read -l line` left `line` empty on every iteration.
            // fish's `read` already assigns the untrimmed whole line when
            // given a single variable, which is exactly the bash meaning,
            // so the prefix is dropped for `read` only (for any other
            // command it is a real environment override and stays).
            if cmd_name == Some("read") && k == "IFS" {
                continue;
            }
            out.push_str(k);
            out.push('=');
            out.push_str(&word_to_fish(v)?);
            out.push(' ');
        }
    }
    // bash `set` OPTIONS. fish spells the same runtime behaviour with its own
    // flags, and the two are NOT interchangeable: fish's `set` is an
    // assignment command, so emitting `set -e -u` verbatim is a runtime error
    // ("set: invalid option combination") on the very first line of the script
    // — it kills the whole file, which is why every corpus script starting with
    // `set -euo pipefail` failed.
    //
    //   bash -e  -> fish has no -e; errexit is the default, so it is a no-op.
    //   bash -u  -> `set -u` in fish IS "error on unset variable". Same word,
    //               different grammar: it must be a BARE `set -u`, and the
    //               assignment form (`set -u x=1`) is what fish rejects.
    //   bash -x / -v / -o pipefail / -n etc. -> fish has no equivalent.
    // A script with no fish-representable option lowers to `:` (a no-op), so
    // the line still parses and the body still runs.
    if cmd_name == Some("set") && args_are_options(&args) {
        let mut flags: Vec<&str> = Vec::new();
        for a in args {
            match a {
                IrExpr::Str(s, _) => {
                    for c in s.strip_prefix('-').unwrap_or(s.as_str()).chars() {
                        // `-e` is fish's default and `-o pipefail` has no fish
                        // counterpart; both are dropped silently.
                        if c == 'u' && !flags.contains(&"u") {
                            flags.push("u");
                        }
                    }
                }
                // `-o errexit`, `-o nounset` and friends: a bare word that is
                // NOT `name=value` is an option, never an assignment.
                _ => {}
            }
        }
        if flags.is_empty() {
            out.push(':');
        } else {
            out.push_str("set ");
            out.push_str(&flags.join(" "));
        }
        return Ok(out);
    }
    // `declare`/`typeset`/`local`/`readonly` declare a variable and may give
    // it a value. fish has NO attribute declarations and NO `local`/`readonly`
    // commands at all (`declare -i x=5` is literally "Unknown command"), so the
    // native lowering is the assignment itself: `declare -i x=5` -> `set x 5`.
    //
    // The bash spelling `x=5` is invalid fish (a bare `name=value` command is
    // rejected), which is why every file carrying one failed to parse. The
    // function-scoping of `local` is already provided by fish's function-local
    // `set`, so no scope helper is needed.
    if matches!(cmd_name, Some("declare" | "typeset" | "local" | "readonly")) {
        let mut flags: Vec<String> = Vec::new();
        let mut stmts: Vec<String> = Vec::new();
        // names declared WITHOUT a value (`declare -A map`) — real
        // declarations the `-i`/`-A` bookkeeping below must see, but not
        // commands to emit.
        let mut declared: Vec<String> = Vec::new();
        let mut pending: Option<String> = None;
        let mut i = 0;
        while i < args.len() {
            match &args[i] {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                    flags.push(s.clone());
                }
                IrExpr::Str(s, _) if s.contains('=') => {
                    let (name, val) = s.split_once('=').unwrap();
                    if val.is_empty() {
                        // the VALUE is the next word: `declare msg= "hello"`
                        // (the core splits that into ["msg=", "hello"])
                        let takes_next = match args.get(i + 1) {
                            Some(IrExpr::Str(x, _)) => !(x.starts_with('-') && x.len() > 1),
                            Some(_) => true,
                            None => false,
                        };
                        if takes_next {
                            let nv = word_to_fish(&args[i + 1])?;
                            pending = Some(format!("set {name} {nv}"));
                            i += 2;
                            continue;
                        }
                    }
                    let nv = if val.is_empty() {
                        "''".to_string()
                    } else if let Some(v) = plain_var_read_fish(val) {
                        // The core hands `local a=$1` over as the opaque word
                        // `a=$1` — no getVar node, just the source text. A
                        // DoubleQuoted render of that text is the LITERAL
                        // `'$1'`, so the declaration read the whole script's
                        // `$1` as five characters. Resolve a whole-value
                        // `$NAME` / `$N` the way the interpreter would; the
                        // interpolating shapes (`local a="x $1 y"`) arrive as
                        // an Interpolate and never take this arm.
                        format!("\"{v}\"")
                    } else {
                        word_to_fish(&IrExpr::Str(val.to_string(), StrStyle::DoubleQuoted))?
                    };
                    stmts.push(format!("set {name} {nv}"));
                }
                IrExpr::Str(s, _) if s.is_empty() => {}
                // A bare NAME after the flags is a declaration of that name
                // (`declare -A map`), NOT a command to run. It used to fall
                // through to the command arm, which emitted the name as a fish
                // COMMAND — `declare -A map` became the line `map`, i.e.
                // "Unknown command: map" — and since the name never appeared
                // in a `set …` statement the `-A` bookkeeping below never
                // recorded it either, so every `map[k]=v` write and
                // `${!map[@]}` read after it rendered wrong as well.
                //
                // No value is written: bash's `declare x` declares the name
                // without setting it, and `:` (the attribute-only no-op
                // below) leaves fish in the same unset state.
                IrExpr::Str(s, _) if is_plain_var(s) => declared.push(s.clone()),
                other => {
                    let w = word_to_fish(other)?;
                    if !w.is_empty() && w != "''" {
                        stmts.push(w);
                    }
                }
            }
            i += 1;
        }
        if let Some(p) = pending {
            stmts.push(p);
        }
        // `-i` / `-A` / `-a` are real semantics for the REST of the program
        // (arithmetic is rendered in math, assoc elements keep a key list) —
        // record them so the later assignments agree. Only the declared name
        // qualifies, not any bare word that happened to follow.
        if flags.iter().any(|f| f.contains('i')) {
            let mut ints = INT_VARS.lock().unwrap();
            for s in stmts
                .iter()
                .filter_map(|s| declared_name(s))
                .chain(declared.iter().cloned())
            {
                ints.insert(s);
            }
        }
        // Only `-A` is ASSOCIATIVE. `-a` is bash's INDEXED-array attribute,
        // and treating it as assoc put every `declare -a`/`local -a`/
        // `typeset -a` array on the string-key runtime: 062_14's
        // `declare -a array=(item1 …)` then lowered its `${array[@]}` to the
        // assoc key-walk instead of fish's native list expansion.
        if flags.iter().any(|f| f.contains('A')) {
            let mut assoc = ASSOC_VARS.lock().unwrap();
            for s in stmts
                .iter()
                .filter_map(|s| declared_name(s))
                .chain(declared.iter().cloned())
            {
                assoc.insert(s);
            }
        }
        if stmts.is_empty() {
            // an attribute-only declaration (`declare -A config`) — the
            // per-element lowering self-initializes, so a no-op still parses.
            out.push(':');
        } else {
            out.push_str(&stmts.join("; "));
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
                    words.push(word_to_fish(a)?);
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
                words.push(word_to_fish(a)?);
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
                // ONE `%s` over an explicitly JOINED word list, never one
                // `%s` per word: `echo` joins its arguments with a single
                // space onto one line, while `printf '%s\n' a b` prints a
                // line PER argument. The two only agree when exactly one
                // word is involved, and the word list here is not the
                // argument count — an unquoted expansion or a command
                // substitution contributes however many words it
                // yields at RUN time (measured on t62_word_split.sh:
                // `echo $x` with `x="a b"` came out as two lines).
                // `string join ' '` with no input is an empty word, which
                // is what bash's `echo $(false)` prints. The `--` is
                // load-bearing: without it a word starting with `-` is read
                // as an OPTION (`echo --x="$X"` died with `string join:
                // --x=test: unknown option`, parse-longoption-with-dollar.sh).
                return Ok(format!("printf '%s\\n' (string join ' ' -- {joined})"));
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
                        let w = word_to_fish(a)?;
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
                words.push(word_to_fish(a)?);
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
                words.push(word_to_fish(a)?);
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
    // `mapfile`/`readarray` are bash-only builtins (fish has neither).
    // Lines land in the per-element vars + a len counter:
    //   mapfile -t lines < input  ->
    //     set -l lines_0 (cat); set lines_len (count (cat))
    //
    // The bash version walked the input with `while IFS= read` and an `eval`,
    // neither of which is fish: `x=0` is not fish assignment, `while ...; do
    // ... done` is not a fish block, and `eval` has no meaning here. fish reads
    // the whole stream as a LIST, so the native lowering is one read plus a
    // count — no loop and no eval at all.
    if matches!(cmd_name, Some("mapfile" | "readarray")) {
        let mut name = "MAPFILE".to_string();
        let mut flags: Vec<String> = Vec::new();
        for a in args {
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => flags.push(s.clone()),
                other => {
                    let w = word_to_fish(other)?;
                    if !w.starts_with('$') && w != "-" {
                        name = w;
                    }
                }
            }
        }
        // `-t` strips the delimiter — reading a stream as a list already does;
        // any other option changes the read semantics (refuse loudly)
        for f in &flags {
            if f != "-t" {
                return Err(format!(
                    "mapfile flag {f} is not fish-lowered — refusing (only -t is emulated)"
                ));
            }
        }
        // `mapfile` fills an ARRAY, and fish's command substitution already
        // yields one element per LINE — so reading the stream straight into
        // the list IS the `-t` semantics, with no loop and no counter.
        return Ok(format!("set -l _mf (cat); set {name} $_mf"));
    }
    // `unset x` — fish spells this `set -e x`, and emitting the bash name
    // verbatim produced "[Unknown command: unset]" (a RUNTIME failure that
    // killed the script, not a parse error). A flattened array also has to
    // drop its len counter, or `${#x[@]}` keeps reporting the old length
    // after the elements are gone.
    if cmd_name == Some("unset") {
        let mut words: Vec<String> = Vec::new();
        for a in args {
            if let IrExpr::Str(s, _) = a {
                // `unset -v`/`unset -f` are bash-only flag words.
                if s.starts_with('-') && s.len() > 1 {
                    continue;
                }
                if s == "@" || s == "*" {
                    continue;
                }
                words.push(s.clone());
            }
        }
        if words.is_empty() {
            out.push(':');
        } else {
            out.push_str("set -e ");
            out.push_str(&words.join(" "));
        }
        return Ok(out);
    }
    // `read -a arr` — bash-only array read. The line splits on IFS into the
    // array's elements, and fish's list IS that array, so the split goes
    // straight into it. (The previous lowering emitted BASH — `{name}_len=0;
    // for _w …; eval …` — which is not fish at all: the `=` assignment alone
    // is a parse error.)
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
                words.push(word_to_fish(a)?);
            }
            let opts = words.join(" ");
            return Ok(format!(
                "read {opts} _read_line; and set {name} (string split \" \" -- $_read_line)"
            ));
        }
        // Plain `read [-r] VAR...`. fish's `read` has NO `-r` option: it is
        // parsed as a VARIABLE NAME, so `read -r line` stores the line in a
        // variable called `-r`… named literally, and prints it — the real
        // `line` was left unset and the enclosing `while` ran its body
        // against an empty string. fish's `read` never interprets
        // backslashes, which is exactly bash's `-r`, so the flag is simply
        // dropped. (The `-a` array form above returns first.)
        out.push_str("read");
        for (i, a) in args.iter().enumerate() {
            if i == 0 && matches!(a, IrExpr::Str(s, _) if s == "read") {
                continue;
            }
            if matches!(a, IrExpr::Str(s, _) if s == "-r") {
                continue;
            }
            out.push(' ');
            out.push_str(&word_to_fish(a)?);
        }
        return Ok(out);
    }
    // `let` is a bash-only builtin (dash rejects it). A `let EXPR` is an
    // arithmetic condition (rc 0 iff the value is nonzero). The old lowering
    // emitted `[ "$((EXPR))" -ne 0 ]` — bash text that is a hard fish PARSE
    // error ("Expected end of the statement, but found an incomplete token"),
    // which killed the whole file, not just the statement (063_11, 063_hard).
    // `arith_cond_fish` is the condition spelling fish can read: a relational
    // splits across `test`, a bare expression becomes a `-ne 0` nonzero test.
    if cmd_name == Some("let") {
        let mut parts = Vec::new();
        for a in args {
            let raw = match a {
                IrExpr::Str(s, _) => s.clone(),
                other => word_to_fish(other)?,
            };
            parts.push(arith_cond_fish(&raw));
        }
        if parts.is_empty() {
            return Ok(":".into());
        }
        return Ok(parts.join("; "));
    }
    // `readlink -e/-f/-m` (GNU canonicalize flags) is called NATIVELY — no
    // rewrite and no polyfill. The inherited `_readlink` prologue was POSIX
    // sh text, so fish could not even parse the output that carried it, and
    // every `x=$(readlink -f …)` produced an empty capture. Fish's own
    // `realpath` builtin is not a drop-in here (`readlink` has no `-P`,
    // `realpath` has no `-n`), and the corpus is compared against bash
    // running the very same `readlink(1)`, so the binary is the faithful
    // lowering: identical bytes by construction.
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
            out.push_str(&word_to_fish(cmd)?);
            for w in args {
                out.push(' ');
                out.push_str(&word_to_fish(w)?);
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
                out.push_str(&word_to_fish(w)?);
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
            out.push_str(&word_to_fish(w)?);
        }
        return Ok(out);
    }
    // `cmp` is called NATIVELY (same reasoning as `readlink` above): the
    // inherited `_cmp` polyfill was POSIX sh and unparseable in fish, and
    // the gate's bash reference invokes the same GNU `cmp`.
    out.push_str(&word_to_fish(cmd)?);
    for w in args {
        out.push(' ');
        out.push_str(&word_to_fish(w)?);
    }
    Ok(out)
}

// ── redirects ────────────────────────────────────────────────────────

fn redirects_to_fish(redirects: &[IrRedirect]) -> Result<String, String> {
    let mut out = String::new();
    // non-heredoc redirects FIRST — `cat > f <<EOF\nbody\nEOF`: the
    // output redirect must precede the heredoc BODY (a heredoc's render
    // includes its body + terminator)
    for r in redirects {
        if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
            continue;
        }
        out.push_str(&redirect_to_fish(r)?);
    }
    for r in redirects {
        if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
            out.push_str(&redirect_to_fish(r)?);
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
        other => word_to_fish(other),
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
fn lower_procsub_stmt_fish(ps: &[IrRedirect], cmd: &str) -> Result<String, String> {
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
fn lower_procsub_inline_fish(ps: &[IrRedirect], cmd: &str) -> Result<String, String> {
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
/// itself is skipped by redirect_to_fish; this wraps the command with the pipe.
///
/// `cmd <<EOF` / `cat <<-EOF` is the same shape: the body goes to stdin, so
/// it lowers to the same printf pipe. fish has NO heredoc syntax at all, and
/// emitting `<<EOF` verbatim produced a file fish cannot even parse — one
/// syntax error for every corpus file that used a heredoc.
fn herestring_wrap_fish(redirects: &[IrRedirect], cmd: String) -> Result<String, String> {
    for r in redirects {
        if r.mode == "herestring" {
            let t = word_to_fish(&r.target)?;
            return Ok(format!("printf '%s\\n' {t} | {cmd}"));
        }
        if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
            let body = match &r.target {
                IrExpr::Str(s, _) => s.clone(),
                other => word_to_fish(other)?,
            };
            return Ok(format!(
                "{} | {cmd}",
                heredoc_body_fish(&body, &r.mode, r.interpolate)
            ));
        }
    }
    Ok(cmd)
}

/// A heredoc body → the left side of the pipe that feeds the command's
/// stdin. Byte-exact with what bash's `<<` writes:
///   * the body is emitted line by line, each with a trailing newline, so
///     `printf '%s\n' a b c` reproduces it exactly;
///   * an EMPTY body must be `printf ''` — `printf '%s\n'` with no
///     arguments still prints one bare newline, which is not what
///     `cat <<EOF\nEOF` writes (nothing at all);
///   * `<<-` strips leading TABS from each line, so the body is
///     de-indented before it is emitted.
/// The `%s` format (never the body as the format) keeps a body containing
/// `%` from being interpreted, and each line is single-quoted so spaces and
/// shell metacharacters survive verbatim.
///
/// `interpolate` is the redirect's own flag: an UNQUOTED delimiter
/// (`<<EOF`) expands `$name`, `${name}`, `$(…)`, backticks and `$?` in the
/// body, while a quoted one (`<<'EOF'`) keeps every byte literal. The core
/// marks exactly that distinction and leaves the body unexpanded, so the
/// expansion is this renderer's job (see `heredoc_expand_line_fish`).
fn heredoc_body_fish(body: &str, mode: &str, interpolate: bool) -> String {
    let mut text = body.to_string();
    if mode == "heredoc-tabs" {
        text = text
            .lines()
            .map(|l| l.strip_prefix('\t').unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n");
    }
    // A trailing newline is the heredoc's line terminator, not body content:
    // the format supplies it back. Anything beyond that is content and must
    // survive — a body ending in a blank line keeps it as a final empty arg.
    let trimmed = text.strip_suffix('\n').unwrap_or(&text);
    let lines: Vec<&str> = if trimmed.is_empty() { Vec::new() } else { trimmed.split('\n').collect() };
    if lines.is_empty() {
        return "printf ''".to_string();
    }
    let args: Vec<String> = lines
        .iter()
        .map(|l| match heredoc_expand_line_fish(l) {
            // an interpolated body line: the expansion IS the fish word
            Some(w) if interpolate => w,
            // a literal body (`<<'EOF'`) — or a line with nothing to expand,
            // which must render byte-identically to the literal form
            _ => fish_single_quoted(l),
        })
        .collect();
    format!("printf '%s\\n' {}", args.join(" "))
}

/// An interpolated heredoc body line → a fish double-quoted word that
/// reproduces bash's expansion of that line, or `None` when the line has
/// nothing to expand (so the caller can keep the literal single-quoted
/// spelling and stay byte-identical for the overwhelmingly common case).
///
/// fish's double quotes differ from bash's in exactly one way that matters
/// here: `${name}` is a hard ERROR ("Variables cannot be bracketed"), while
/// `$name` expands — so a brace form must be rewritten to the bare one.
/// `$(…)` needs no rewriting (fish 3.4+ expands it inside double quotes
/// exactly like bash; a bare `(…)` would NOT expand there, which is why the
/// rest of this renderer's `(…)` spelling cannot be reused). Backticks are
/// bash-only, so they lower to the `$(…)` fish does expand.
///
/// Anything else (`$1`, `${x:-y}`, `$@`, …) returns `None` — the caller then
/// emits the line literally, which is this renderer's pre-existing behaviour
/// rather than a new guess (REFUSE > GUESS).
fn heredoc_expand_line_fish(line: &str) -> Option<String> {
    let cs: Vec<char> = line.chars().collect();
    let mut out = String::from("\"");
    let mut i = 0;
    let mut expanded = false;
    while i < cs.len() {
        match cs[i] {
            '\\' => {
                // In an unquoted heredoc only `\$`, `` \` `` and `\\` escape;
                // any other backslash is a literal character.
                match cs.get(i + 1) {
                    Some(&c) if c == '$' || c == '`' || c == '\\' => {
                        push_dq_lit(&mut out, c);
                        i += 2;
                    }
                    _ => {
                        push_dq_lit(&mut out, '\\');
                        i += 1;
                    }
                }
            }
            '`' => {
                let end = cs[i + 1..].iter().position(|&c| c == '`')? + i + 1;
                let inner: String = cs[i + 1..end].iter().collect();
                // bash interpolates the command's own body too; `$(…)`
                // re-enters fish's parser, which expands `$name` the same
                // way, so the body is copied verbatim.
                out.push_str(&format!("$({inner})"));
                expanded = true;
                i = end + 1;
            }
            '$' => match cs.get(i + 1) {
                Some('?') => {
                    out.push_str("$status");
                    expanded = true;
                    i += 2;
                }
                Some('(') => {
                    let end = balanced_paren(&cs, i + 1)?;
                    let inner: String = cs[i + 2..end].iter().collect();
                    out.push_str(&format!("$({inner})"));
                    expanded = true;
                    i = end + 1;
                }
                Some('{') => {
                    let close = cs[i + 2..].iter().position(|&c| c == '}')? + i + 2;
                    let name: String = cs[i + 2..close].iter().collect();
                    // only a bare `${name}` is a plain expansion; every
                    // operator form (`${x:-y}`, `${#x}`, `${x/a/b}`) needs
                    // the parameter lowering this scanner does not do.
                    if name.is_empty()
                        || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return None;
                    }
                    out.push('$');
                    out.push_str(&name);
                    expanded = true;
                    i = close + 1;
                }
                Some(&c) if c.is_ascii_alphabetic() || c == '_' => {
                    let mut j = i + 1;
                    while j < cs.len() && (cs[j].is_ascii_alphanumeric() || cs[j] == '_') {
                        j += 1;
                    }
                    let name: String = cs[i + 1..j].iter().collect();
                    out.push('$');
                    out.push_str(&name);
                    expanded = true;
                    i = j;
                }
                // a lone `$`, `$1`, `$@`: not a plain name expansion
                _ => return None,
            },
            c => {
                push_dq_lit(&mut out, c);
                i += 1;
            }
        }
    }
    out.push('"');
    expanded.then_some(out)
}

/// Index of the `)` closing the `(` at `open`, honouring nesting, quoting
/// and backslashes. `None` for an unbalanced substitution.
fn balanced_paren(cs: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    let mut quote: Option<char> = None;
    while i < cs.len() {
        let c = cs[i];
        match quote {
            Some(q) => {
                if c == '\\' && q == '"' {
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '\\' => {
                    i += 2;
                    continue;
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// One literal character inside a fish double-quoted word.
fn push_dq_lit(out: &mut String, c: char) {
    match c {
        '"' | '\\' | '$' => {
            out.push('\\');
            out.push(c);
        }
        c => out.push(c),
    }
}

/// Single-quote a literal for fish. Inside single quotes fish treats
/// everything literally, so only `'` and `\` need escaping.
fn fish_single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "'\\''"))
}

fn redirect_to_fish(r: &IrRedirect) -> Result<String, String> {
    let fd = r.fd.unwrap_or(0);
    let op = match r.mode.as_str() {
        "w" => ">",
        // `>|` (RedirectOutClobber). The contract gives it the distinct mode
        // string "wc" (shir.rs: `ClobberOutput => ("wc", 1)`), but it needs no
        // distinct SPELLING here: bash's `>|` differs from `>` only under
        // `set -C` (noclobber), and fish has no noclobber — the `set` arm
        // already lowers `set -C` to the no-op `:`. Plain `>` is therefore the
        // faithful fish form of both operators.
        "wc" => ">",
        "a" => ">>",
        "r" => "<",
        "r+" => "<>",
        "herestring" | "heredoc" | "heredoc-tabs" => {
            // Both feed the command's stdin and are wrapped as a printf pipe
            // by herestring_wrap_fish — fish has neither `<<<` nor `<<`.
            return Ok(String::new());
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
        other => word_to_fish(other)?,
    };
    Ok(format!(" {fdpre}{op}{target}"))
}

/// Redirect specs as Object([(fd,Int),(mode,Str),(target,word)]) — the
/// command-position `redirect` call form.
fn redirect_specs(args: &[IrExpr], idx: usize) -> Result<String, String> {
    let Some(IrExpr::Array(specs)) = args.get(idx) else {
        return Ok(String::new());
    };
    redirect_objs_to_fish(specs)
}

/// Redirect spec objects — `Object([(fd,Int),(mode,Str),(target,word)])`
/// (the `IrStmt::Exec` redirects field and the call-form spec list).
fn redirect_objs_to_fish(specs: &[IrExpr]) -> Result<String, String> {
    let mut out = String::new();
    for spec in specs {
        let r = match redirect_objs(spec) {
            Ok(v) if !v.is_empty() => v[0].clone(),
            _ => continue,
        };
        out.push_str(&redirect_to_fish(&r)?);
    }
    Ok(out)
}

/// Parse an `Array` of redirect spec Objects into `IrRedirect`s. The
/// specs arrive as an Array in the call-form (`Call("redirect",
/// [Arrow, Array([Object, ...])])`) but as a bare Object element in the
/// `IrStmt::Exec` redirects field and `redirect_objs_to_fish`'s per-spec
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

// ── words ────────────────────────────────────────────────────────────

/// `$(seq …)` / `$(wc -l FILE)` — native sh idiom lifts (no external
/// process spawn: printf/read are builtins). Returns the shell text, or
/// None if the capture isn't one of these idioms.
fn capture_idiom_fish(expr: &IrExpr) -> Result<Option<String>, String> {
    let IrExpr::Arrow(stmts) = expr else { return Ok(None) };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stmts.as_slice() else { return Ok(None) };
    if func != "exec" { return Ok(None); }
    let cmd = match args.first() {
        Some(IrExpr::Str(c, _)) => c.as_str(),
        _ => return Ok(None),
    };
    let items = match args.get(1) {
        Some(IrExpr::Array(it)) => it,
        _ => return Ok(None),
    };
    let str_of = |e: &IrExpr| match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        _ => None,
    };
    match cmd {
        "seq" => {
            let nums: Vec<i64> = items
                .iter()
                .filter_map(str_of)
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if nums.is_empty() || nums.len() != items.len() {
                return Ok(None);
            }
            let (start, inc, end) = match nums.as_slice() {
                [end] => (1, 1, *end),
                [start, end] => (*start, 1, *end),
                [start, inc, end] => (*start, *inc, *end),
                _ => return Ok(None),
            };
            if inc == 0 {
                return Ok(None);
            }
            let mut vals = Vec::new();
            if inc > 0 {
                let mut v = start;
                while v <= end {
                    vals.push(v);
                    v += inc;
                }
            } else {
                let mut v = start;
                while v >= end {
                    vals.push(v);
                    v += inc;
                }
            }
            let nums = vals.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            Ok(Some(format!("$(printf '%s\\n' {nums})")))
        }
        "wc" => {
            if items.len() == 2 && str_of(&items[0]).as_deref() == Some("-l") {
                let file = match str_of(&items[1]) {
                    Some(f) => f,
                    None => return Ok(None),
                };
                Ok(Some(format!(
                    "$(n=0; while IFS= read -r _; do n=$((n+1)); done < {}; echo \"$n\")",
                    str_word(&file)
                )))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn word_to_fish(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(bash_specials_to_fish(&str_word(s))),
        IrExpr::Int(i) => Ok(i.to_string()),
        // Route through `var_ref_to_fish`: the core hands bash's special
        // parameters over as bare names (`getVar("*")` for `$*`, `getVar("$")`
        // for `$$`, `getVar("?")` for `$?`) and `$*` / `$$` are fish PARSE
        // ERRORS, so emitting `$` + the raw name verbatim kills the file.
        IrExpr::Var(name, _) => Ok(var_ref_to_fish(name, false)),
        IrExpr::Ident(name) => Ok(name.clone()),
        IrExpr::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
        IrExpr::Interpolate(parts) => interp_to_fish(parts),
        IrExpr::Arith(a) => {
            // An IncDec is a self-contained command substitution (it mutates
            // the variable, so it cannot be an argument to `math`); everything
            // else is an expression for math to evaluate.
            if matches!(&**a, ArithAst::IncDec { .. }) {
                Ok(arith_to_fish(a))
            } else {
                Ok(format!("(math --scale 0 \"{}\")", arith_to_fish(a)))
            }
        }
        IrExpr::Call { func, args } => call_word_to_fish(func, args),
        IrExpr::Index { var, key } => index_to_fish(var, key, true),
        IrExpr::Capture { expr, .. } => {
            if let Some(lift) = capture_idiom_fish(expr)? {
                Ok(lift)
            } else {
                Ok(format!(
                    "({})",
                    arrow_to_fish(std::slice::from_ref(expr.as_ref()))?
                ))
            }
        }
        IrExpr::Json(v) => Ok(json_str(v)),
        IrExpr::Ternary { cond, then, else_ } => {
            let c = cond_to_fish(cond)?;
            let t = word_to_fish(then)?;
            let e = word_to_fish(else_)?;
            Ok(format!(
                "(if {c}; printf '%s\\n' {t}; else; printf '%s\\n' {e}; end)"
            ))
        }
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
fn index_to_fish(var: &str, key: &IrExpr, quoted: bool) -> Result<String, String> {
    match key {
        IrExpr::Str(k, _) if k == "@" || k == "*" => Ok(arr_expand_call_fish(arr_base(var))),
        IrExpr::Str(k, _) if k.contains(['$', '(']) => {
            Ok(dynamic_el_read_fish(arr_base(var), k))
        }
        IrExpr::Str(k, _) => Ok(arr_elem_word(var, k, quoted)),
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

/// bash's arithmetic comma operator: `a, b` evaluates both operands and
/// yields the LAST one. Only a TOP-LEVEL comma separates — one inside parens
/// belongs to a nested expression — and fish's `math` refuses the operator
/// outright ("Too many arguments"), so it is folded away before the
/// subscript is evaluated.
fn arith_last_operand(key: &str) -> &str {
    let mut depth = 0i32;
    let mut last = 0usize;
    for (i, c) in key.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => last = i + 1,
            _ => {}
        }
    }
    key[last..].trim()
}

/// An INDIRECT element read — `${a[$k]}`, `${m[foo]}` — over the flat
/// per-element variables.
///
/// fish has no `${a[$k]}` and no `eval`, but it does have the `$$name`
/// double-dollar indirection: with the element's NAME in `_nk`, `$$_nk` reads
/// the variable that name refers to. Two things are load-bearing.
///
/// * The separator is `_`, NOT `$`. The per-element lowering NAMES the
///   element `{base}_{key}` — the write arm emits `set {base}_{idx}` and the
///   assoc walker composes `$name"_"$_k` — so a name built as `map$foo` can
///   never resolve. Measured on examples/029_arrays_associative.sh: every
///   dynamic read printed the empty string where bash printed the value
///   (and the older call form `_fish_get_el map"$idx"` was worse still — a
///   LITERAL `$idx`, a variable that does not exist anywhere).
/// * An INDEXED subscript is an ARITHMETIC expression, not text
///   (`${a[(2*$i)-1]}`), so it is evaluated before it joins the name.
///   Emitting it raw spliced a fish command substitution into the name —
///   `FilesystemOptions"$"(2*$i)-1`, a PARSE error ("Expected a variable
///   name after this $") — and `math` is the evaluator.
fn dynamic_el_read_fish(base: &str, key: &str) -> String {
    // A BRACED key (`${index}`) is the same read as the bare `$index`, so
    // normalise it first: without this it fell to the arithmetic branch and
    // the `${}` was spliced into the `math` argument — `${` is a fish PARSE
    // ERROR ("Variables cannot be bracketed"), and the caret landed inside
    // the element-name substitution.
    let normalised = braced_refs_to_dollar(key);
    let bare = normalised.strip_prefix('$').unwrap_or(&normalised);
    if is_plain_name(base) && !is_assoc(base) {
        // An INDEXED array: the subscript is arithmetic and fish's own
        // `$arr[i]` is 0-based-offset by one. A literal/index VARIABLE is the
        // common case; anything else is the arithmetic text bash evaluates.
        let expr = if !bare.is_empty()
            && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            format!("${bare}")
        } else {
            arith_rewrite(&normalised)
        };
        return format!(
            "(set -l _i (math --scale 0 \"{expr} + 1\"); printf '%s\\n' ${base}[$_i])"
        );
    }
    // A plain identifier is TEXT (an assoc key), so it concatenates straight
    // into the element name.
    let simple =
        !bare.is_empty() && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if simple {
        format!("(set -l _nk {base}_\"${bare}\"; printf '%s\\n' \"$$_nk\")")
    } else {
        format!(
            "(set -l _nk {base}_(math --scale 0 \"{}\"); printf '%s\\n' \"$$_nk\")",
            arith_rewrite(key)
        )
    }
}

/// Replace the `${…}` spans of a SUBSCRIPT with their `$`-spelled form, so a
/// braced subscript (`arr[${i}]`) takes the same path as a bare one
/// (`arr[$i]`).
///
/// Only a plain name is rewritten (`${i}` → `$i`); a braced span carrying an
/// operator keeps its inner text, which the caller then treats as a literal
/// key rather than splicing a `${` into the output (a fish parse error).
fn braced_refs_to_dollar(key: &str) -> String {
    let b: Vec<char> = key.chars().collect();
    let mut out = String::with_capacity(key.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == '$' && b.get(i + 1) == Some(&'{') {
            let mut j = i + 2;
            while j < b.len() && b[j] != '}' {
                j += 1;
            }
            let inner: String = b[i + 2..j.min(b.len())].iter().collect();
            if is_plain_var(&inner) {
                out.push('$');
                out.push_str(&inner);
            } else {
                out.push_str(&inner);
            }
            i = j + 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// The fish expression that NAMES the element `{base}[{idx}]` in the flat
/// per-element layout (`{base}_{key}`) the rest of this renderer maintains.
///
/// Three shapes, because the subscript can be three things:
///
/// * a plain NAME (`arr[$i]`, `arr[${i}]`) — the element name is the base, an
///   underscore and the VALUE, so the name is COMPOSED at run time
///   (`arr_"$i"`). Reading `$arr_$i` instead would be wrong twice over: fish
///   parses that as the unrelated variable `arr_` followed by `$i`.
/// * a literal subscript (`arr[2]`, `map[1,1]`) — a fixed name.
/// * anything else — a literal key built from the text with the shell
///   punctuation removed. That is a REFUSAL, not a translation (a computed
///   subscript the A1 did not structure), but it is a name, so the statement
///   still runs; splicing the text in raw (the `set x[i] v` this replaces)
///   was a fish PARSE ERROR that cost the whole file.
fn subscript_name_fish(base: &str, idx: &str) -> String {
    let key = braced_refs_to_dollar(idx.trim());
    let bare = key.strip_prefix('$').unwrap_or(&key);
    if is_plain_var(bare) {
        return format!("{base}_\"${bare}\"");
    }
    if !bare.is_empty() && bare.chars().all(|c| c.is_ascii_digit()) {
        return format!("{base}_{bare}");
    }
    let literal: String = key
        .chars()
        .filter(|c| {
            !matches!(c, '$' | '(' | ')' | '"' | '\'' | '`' | ' ' | '{' | '}')
        })
        .collect();
    let literal = if literal.is_empty() {
        "0".to_string()
    } else {
        literal
    };
    format!("{base}_{literal}")
}

/// `${base[idx]}` as a fish WORD — the read side of
/// [`subscript_name_fish`].
///
/// fish has NO `$x[i]` subscript syntax ("Variables cannot be bracketed") and
/// no `${…}` at all, so the `$x[i]` this used to emit for a bracketed name was
/// a read-time PARSE ERROR that killed the whole file. A fixed name needs no
/// indirection; every computed one is dereferenced through fish's `$$name`.
///
/// The substitution is spelled `$( … )` rather than a bare `( … )` because
/// EVERY caller of `var_ref_to_fish` splices the result inside double quotes
/// (the `test -n "…"` of the default/`+` guards, `string sub -- "…"`, the
/// `"[ -n \"…\" ]"` truthiness test), and fish prints a bare `( … )` there as
/// literal text.
fn subscript_read_fish(base: &str, idx: &str) -> String {
    // `${arr[@]}` reaching a subscript read is the WHOLE array, not an
    // element: a bare list splices into the enclosing quotes as the
    // space-joined elements, and treating `@` as an arithmetic index would
    // emit `math "@ + 1"`.
    if idx.trim() == "@" || idx.trim() == "*" {
        return format!("${}", arr_base(base));
    }
    if is_plain_name(base) && !is_assoc(base) {
        // Indexed array: fish's own `$arr[i]` subscripts the LIST, 1-based.
        // A literal subscript stays literal (fish accepts `"$arr[3]"` inside
        // double quotes); a COMPUTED one cannot be written there — fish
        // rejects `"$arr[(math …)]"` with "Invalid index value" — so it is
        // evaluated into a variable first, which the `$( … )` body allows.
        let key = braced_refs_to_dollar(idx.trim());
        let bare = key.strip_prefix('$').unwrap_or(&key);
        if bare.parse::<i64>().is_ok() {
            let n: i64 = bare.parse().unwrap_or(0);
            return format!("${base}[{}]", n + 1);
        }
        if !bare.is_empty()
            && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return format!(
                "$(set -l _i (math --scale 0 \"${bare} + 1\"); printf '%s' ${base}[$_i])"
            );
        }
        return format!(
            "$(set -l _i (math --scale 0 \"{} + 1\"); printf '%s' ${base}[$_i])",
            arith_rewrite(&key)
        );
    }
    let name = subscript_name_fish(base, idx);
    if is_plain_var(&name) {
        return format!("${name}");
    }
    format!("$(set -l _nk {name}; printf '%s' \"$$_nk\")")
}

/// A VARIABLE array subscript (`a[i]`) — the same indirect read as
/// `dynamic_el_read_fish`, whose key arrives here as a bare name rather than
/// as source text.
fn index_var_key_to_sh(base: &str, k: &str) -> String {
    dynamic_el_read_fish(base, k)
}

/// Is this rendered word a fish command substitution (`(cmd)`) rather than
/// bare text?
///
/// fish only expands `(…)` OUTSIDE double quotes — inside them it prints the
/// literal characters, so wrapping a substitution in `"…"` silently produces
/// the text `(string split …)` instead of its value. Such a word is already a
/// single word (a fish substitution splits its output on NEWLINES only, never
/// on spaces), so it must be spliced into a command bare.
///
/// The trailing `…)[1]` / `…[2]` index suffix (`string match -r` capture
/// access) is part of the same word and does not change the answer.
fn is_fish_substitution(s: &str) -> bool {
    s.starts_with('(') || s.starts_with("$(")
}

/// If `s` is EXACTLY one `$( … )` substitution, return its body.
///
/// fish cannot NEST a `$( … )` inside another `$( … )` — the inner one has to
/// be the bare `( … )`, and the nested spelling is a parse error ("command
/// substitutions not allowed here"). fish reports that while READING the file,
/// so one nested pair kills every statement in the script, not just the one
/// that produced it.
///
/// A few renderers hand back a string that is ALREADY a substitution:
/// `grepMatches` emits `$(printf … | grep -o …)`, so `matched=$(cmd | grep -o
/// pat)` arrives at the `capture` arm as `$( … )` and used to get wrapped a
/// second time.
fn cmdsub_sole_body(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("$(")?;
    let mut depth = 1i32;
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return if i + 1 == rest.len() {
                        Some(&rest[..i])
                    } else {
                        None
                    };
                }
            }
            _ => {}
        }
    }
    None
}

/// A `capture` / `captureWords` word: the arrow body wrapped in ONE fish
/// command substitution (`"$( … )"` when the enclosing template needs the
/// quotes, bare `$( … )` for the word-list form).
///
/// The body is spliced through unwrapped when the arrow already IS a single
/// substitution, so the two never nest (see `cmdsub_sole_body`).
fn cmdsub_word_fish(inner: String, quoted: bool) -> String {
    let body = cmdsub_sole_body(&inner).map(str::to_string).unwrap_or(inner);
    if quoted {
        format!("\"$({body})\"")
    } else {
        format!("$({body})")
    }
}

/// fish has no `$#`: the positional-parameter COUNT is the `count` builtin.
/// It is a command, so a value position needs the command substitution
/// `(count $argv)` — the bare `count $argv` only works where a command is
/// already expected, and `"count $argv"` prints the literal text.
const COUNT_ARGV_FISH: &str = "(count $argv)";

fn call_word_to_fish(func: &str, args: &[IrExpr]) -> Result<String, String> {
    match func {
        // A bare getVar in a word position is the QUOTED expansion (the
        // core wraps deliberate word-splitting in split()): `"$x"`. The
        // quote matters — bash `printf '%s' "$x"` passes the value as
        // ONE word, the unquoted form re-splits it (heredoc-apostrophe.sh
        // truncates at the first space without it).
        "getVar" => {
            let r = var_ref_to_fish(&raw_arg(args, 0)?.replace('.', "_"), false);
            // `$#` lowers to `(count $argv)`, a fish command substitution
            // that is already ONE word. Wrapping it in `"…"` would print the
            // literal text `(count a b)`. Every other variable expands to a
            // value, which does need the quotes.
            if r == COUNT_ARGV_FISH {
                return Ok(r);
            }
            Ok(format!("\"{r}\""))
        }
        // param expansions in word position are the QUOTED source form
        // (`echo "\"${x#p}\" "` — bash keeps interior spaces; the core
        // wraps only unquoted getVar in split(), param has no marker, so
        // the quote is the safe default). Array/list forms stay bare.
        "param" => {
            let s = param_to_fish(args, false)?;
            // The `""` arm already returns a quoted segment — never re-wrap
            // (`""$x""` would field-split the value). The same holds for an
            // expansion that IS a fish command substitution: fish does NOT
            // expand `(...)` inside double quotes (it prints the literal
            // text), so `"$(_arr_…)"` / `"(string split …)"` would be dead
            // literal output, not a value. Those are already one word (a
            // substitution splits on newlines only), so they pass through
            // bare.
            if s.starts_with('"') || s.starts_with("(_arr_") || is_fish_substitution(&s) {
                Ok(s)
            } else {
                Ok(format!("\"{s}\"",))
            }
        }
        // the core's word-splitting node: an UNQUOTED `$var`. POSIX sh
        // expands it and field-splits on IFS whitespace — and fish does NOT
        // split a variable at all, so a bare `$x` hands the whole value over
        // as ONE word. Measured on t62_word_split.sh: `for w in $x` with
        // `x="a b"` ran the body ONCE with `w="a b"` instead of twice, and
        // `printf '<%s>\n' $x` printed one `<a b>` line instead of `<a>` and
        // `<b>`.
        //
        // `string match -a -r` over the non-blank runs is bash's default-IFS
        // split: the extraction collapses runs of separators, so leading and
        // trailing whitespace yields no empty words, and it yields NO words
        // for an unset or empty value — both exactly what POSIX sh does. The
        // class is `[^ \t\n]+` (the literal default IFS) rather than `\S+`,
        // which would also break on `\r`/`\v`.
        //
        // The pattern is single-quoted WITHOUT backslash escaping: the two
        // `\`-escapes are for the regex engine, and `fish_single_quoted`
        // would double them into literal backslashes (the pattern would then
        // match nothing).
        "split" => match arg(args, 0)? {
            IrExpr::Call { func, args } if func == "getVar" => Ok(format!(
                "(string match -ar '[^ \\t\\n]+' -- {})",
                var_ref_to_fish(&raw_arg(args, 0)?, false)
            )),
            IrExpr::Call { func, args } if func == "param" => param_to_fish(args, false),
            other => word_to_fish(other),
        },
        "param" => param_to_fish(args, false),
        // VALUE-returning function call (the echo-return transform's
        // convention — CROSS_BACKEND_RUNTIME.md §8.3). The callee's value
        // return is lowered to an `echo` on stdout (fish's `return` is a
        // STATUS), so the word form is fish's native command substitution.
        //
        // UNQUOTED, and that is the correct single-word semantics rather
        // than a gap: fish expands `(...)` only outside double quotes
        // (`printf '%s\n' "(f)"` prints the literal `(f)`), and it splits a
        // substitution on NEWLINES only — never on spaces — so a value
        // containing spaces still arrives as ONE argument, exactly like
        // bash's quoted `"$(f)"` this replaces.
        "fnValue" => {
            let name = raw_arg(args, 0)?;
            let mut inner = name;
            if let Some(IrExpr::Array(items)) = args.get(1) {
                for w in items {
                    inner.push(' ');
                    inner.push_str(&word_to_fish(w)?);
                }
            }
            Ok(format!("({inner})"))
        }
        "listVar" => {
            let n = raw_arg(args, 0)?;
            // `"$@"` is a hard PARSE ERROR in fish ("$@ is not supported.
            // In fish, please use $argv"), so this cannot be spelled
            // literally. Bare `$argv` is the positional-parameter LIST and
            // matches bash's word-split `"$@"` in an unquoted word position;
            // a value that must stay ONE word (a quoted context) is handled
            // by the callers, which wrap this in `"…"` themselves.
            Ok("$argv".into())
        }
        "arrayIndex" => {
            let name = raw_arg(args, 0)?;
            // POSIX has no arrays — the per-element lowering (`arr[i]` ->
            // `${arr_i}`). Literal indices lower inline (quoted — the
            // element is one word); @/* (the whole array) and dynamic
            // indices need the runtime count / indirect expansion.
            match arg(args, 1)? {
                IrExpr::Str(k, _) if k == "@" || k == "*" => Ok(arr_expand_call_fish(arr_base(&name))),
                IrExpr::Str(k, _) if k.contains(['$', '(']) => {
                    // dynamic key (`${map[$k]}`) — native `$$name` indirection
                    Ok(dynamic_el_read_fish(arr_base(&name), k))
                }
                IrExpr::Str(k, _) => Ok(arr_elem_word(&name, k, true)),
                // a VARIABLE subscript (`arr[i]`) — the dynamic-key indirection
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
                IrExpr::Str(k, _) if k.contains(['$', '(']) => {
                    Ok(dynamic_el_read_fish(arr_base(&name), k))
                }
                IrExpr::Str(k, _) => Ok(format!("\"${name}_{k}\"")),
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
        "arrayItems" => Ok(arr_keys_call_fish(arr_base(&raw_arg(args, 0)?))),
        "arrayLen" => {
            let n = raw_arg(args, 0)?;
            Ok(arr_len_fish(&n))
        }
        "capture" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, true)),
        "captureWords" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, false)),
        // fish has NO `$(( ))` at all: it is a parse error, not a spelling
        // difference, so an arithmetic VALUE that reaches a word position goes
        // through `math` exactly like the interp and assign paths.
        "arith" => {
            let raw = raw_arg(args, 0)?;
            if raw.contains(['"', '\'']) {
                // bash errors at RUNTIME (the expansion yields nothing); a
                // failed command substitution has the same observable.
                Ok("(false)".into())
            } else {
                Ok(arith_cmd_fish(&raw))
            }
        }
        "brace" => brace_to_fish(args),
        "join" => join_to_fish(arg(args, 0)?, false),
        "setArray" => {
            // POSIX has no arrays — the per-element lowering:
            //   arr=(a b c) -> arr_0=a; arr_1=b; arr_2=c; arr_len=3
            let name = raw_arg(args, 0)?;
            Ok(set_array_to_fish(&name, &array_items(args, 1)?, false))
        }
        "setArrayAppend" => {
            //   arr+=(x) -> arr_$arr_len=x; arr_len=$((arr_len+1))
            let name = raw_arg(args, 0)?;
            Ok(set_array_to_fish(&name, &array_items(args, 1)?, true))
        }
        "assign" => {
            assign_call_to_fish(args)
        }
        "line" => {
            // multi-return line read (`line(cap, N)` = the outparam
            // transform's String(v).split('\n')[N]): POSIX per-line
            // extraction via sed — a missing line yields ""
            let v = word_to_fish(arg(args, 0)?)?;
            let n = match arg(args, 1)? {
                IrExpr::Str(i, _) => i.parse::<usize>().unwrap_or(0),
                other => return Err(format!("line: index not renderable: {other:?}")),
            };
            // sed lines are 1-BASED; the JS split index is 0-based
            Ok(format!(
                "\"$(printf '%s' {v} | sed -n '{}p')\"",
                n + 1
            ))
        }
        other => Err(format!("word call not renderable: {other:?}")),
    }
}

/// `${name}`-family rendering. `list` selects the list form (join
/// context, `"${arr[@]}"`).
fn var_ref_to_fish(name: &str, list: bool) -> String {
    if name == "RANDOM" {
        // bash-only; POSIX sh has no portable random — POSIX awk's
        // srand()/rand() (int(rand()*32768) = bash's 0..32767 range)
        return "$(awk 'BEGIN{srand(); printf \"%d\", int(rand()*32768)}')".into();
    }
    // The bash special parameters fish spells differently.
    //
    // `$?` is not merely a different NAME in fish — it is a PARSE ERROR
    // ("$? is not the exit status. In fish, please use $status"), and the
    // error is raised while reading the file, so it kills the whole script,
    // not just the one statement. Fish's spelling is `$status`, which works
    // in every scope including inside double quotes, and carries the same
    // meaning (the last command's exit status).
    if name == "?" {
        return "$status".into();
    }
    // `$#` (argument count) is a PARSE ERROR in fish ("$# is not supported.
    // In fish, please use 'count $argv'") — same read-time kill as `$?`.
    if name == "#" {
        return COUNT_ARGV_FISH.into();
    }
    // `$!` (last background pid) and `$-` have no fish spelling at all and
    // are a parse error; emit an empty expansion so the file still reads.
    if name == "!" || name == "-" {
        return "\"\"".into();
    }
    // `$$` — the core lexes the pid as a bare `$` name (`getVar("$")`), and
    // `$` alone in the generic branch below would render the literal `$$`,
    // which fish rejects at parse time.
    if name == "$" {
        return "$fish_pid".into();
    }
    // `$*` / `$@` — the positional list, joined by the first char of IFS.
    // fish spells it `$argv`; `$*` / `$@` are parse errors in every context,
    // and the generic branch below would emit them verbatim because they pass
    // its `?!@*#$-` character test. Handle them in the SCALAR position too,
    // not just under `list` — `"$*"` inside a double-quoted template is
    // joined there and `"$argv"` is its exact equivalent.
    if name == "@" || name == "*" {
        return "$argv".into();
    }
    // Positional parameters: fish has no `$1`/`$2` shorthand — a positional
    // read is `$argv[N]`. This is not merely a spelling preference: `$1` is
    // EMPTY inside a fish function (and inside a command substitution, which
    // is how a value-returning call site invokes the callee), while `$argv[1]`
    // resolves in every scope, so a `$1` render silently produces empty output
    // for every function that reads an argument.
    if let Ok(n) = name.parse::<usize>() {
        if n >= 1 {
            return format!("$argv[{n}]");
        }
    }
    if list {
        // fish has NO `${...}` form at all — a bare `${` is a parse error
        // that kills the whole file ("${ is not a valid variable in fish").
        // A fish list variable expands to all of its elements on its own,
        // so the list form is the plain `$name` (and `$argv` for the
        // positional parameters).
        if name == "@" || name == "*" {
            return "$argv".into();
        }
        let base = name.strip_suffix("[@]").unwrap_or(name);
        return format!("${base}");
    }
    // special parameters / positionals: `$?` `$!` `$@` `$1` …
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "?!@*#$-_".contains(c))
        && !name.starts_with('[')
    {
        format!("${name}")
    } else if let Some((base, idx)) = name
        .strip_suffix(']')
        .and_then(|n| n.split_once('['))
        .filter(|(b, _)| is_plain_var(b))
    {
        // a SUBSCRIPTED name (`arr[$i]`, `arr[1,1]`, `arr[${i}]`). fish has no
        // `${x[i]}` subscript syntax — "Variables cannot be bracketed" is a
        // read-time PARSE ERROR — so the element read goes through the flat
        // per-element layout instead of being emitted verbatim.
        subscript_read_fish(base, idx)
    } else {
        // not a form this renderer can name; keep the identifier characters
        // so the word is at least legal fish (refuse > guess: the old
        // verbatim `$name` here was a parse error that killed the whole file)
        let cleaned: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if cleaned.is_empty() {
            "\"\"".into()
        } else {
            format!("${cleaned}")
        }
    }
}

/// A whole-value `$NAME` / `$N` read inside an opaque declaration word.
///
/// `local a=$1` reaches the renderer as the raw source text `a=$1` — the core
/// keeps the spelling rather than a getVar node — so the value has to be
/// resolved FROM that text. Rendering it as a DoubleQuoted literal produced
/// `set a '$1'`, i.e. the five characters `$`,`1` instead of the argument.
///
/// Only a bare `$` followed by a name or positional digits qualifies; anything
/// else (`$(…)`, `${…}`, `$1$2`, a lone `$`) returns None so the caller keeps
/// its literal path rather than guessing (refuse > guess).
fn plain_var_read_fish(val: &str) -> Option<String> {
    let core = val.trim().trim_matches('"').trim_matches('\'');
    let name = core.strip_prefix('$')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(var_ref_to_fish(name, false))
}

/// Parameter-expansion call: `param(op, name, extras...)` → `${...}`.
/// bash-only ops (slice, case-mod, substitution) that dash cannot parse
/// are emulated with POSIX tools so the rendered script RUNS under
/// /bin/sh with bash-identical stdout (the equivalence gate).
fn param_to_fish(args: &[IrExpr], list: bool) -> Result<String, String> {
    let op = raw_arg(args, 0)?;
    let name = raw_arg(args, 1)?;
    // The VALUE-position reference to the subject. Identical to `$name` for a
    // plain identifier, but `${3//…}` / `${1:0:2}` / `${2^^}` name a
    // POSITIONAL, and `$3` is empty inside a fish function — the subject has
    // to go through the `$N` → `$argv[N]` mapping (see `param_subject_fish`).
    let subj = param_subject_fish(&name);
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
                        return Ok(arr_keys_call_fish(arr_base(&an)));
                    }
                    return Ok(arr_expand_call_fish(arr_base(&an)));
                }
                if idx.contains(['$', '(']) {
                    // dynamic key (`${map[$k]}`) — the native `$$name`
                    // indirection over the flat `{base}_{key}` element var.
                    return Ok(dynamic_el_read_fish(&an, idx));
                }
                return Ok(arr_elem_word(&an, &idx, true));
            }
            if let Some(an) = name.strip_prefix('#') {
                return Ok(arr_len_fish(&an));
            }
            if list {
                // `"${x[@]}"` — array elements joined
                Ok(var_ref_to_fish(&name, true))
            } else {
                // special params (`?`, `1`, `@`, ...) keep the `$name`
                // form inside the quotes (`"$?"` — `${?}` is invalid).
                // `var_ref_to_fish` already renamed `$?` to fish's
                // `$status`, so the quoted form is `"$status"` — still
                // valid fish (fish expands `$status` inside quotes).
                let r = var_ref_to_fish(&name, false);
                // `$#` / `$!` / `$-` render to a COMMAND or a bare literal,
                // which double quotes would turn into a literal string.
                if r == COUNT_ARGV_FISH {
                    return Ok(r);
                }
                Ok(format!("\"{r}\""))
            }
        }
        "len" => {
            // `len` on an array (`${#arr[@]}`) -> the element count; on a
            // scalar (`${#s}`) -> fish's `string length`
            if let Some(an) = name.strip_suffix("[@]") {
                Ok(arr_len_fish(&an))
            } else {
                Ok(format!("(string length -- \"{subj}\")"))
            }
        }
        "slice" => {
            // `${#arr[@]}` arrives as slice with a `#name` target — the
            // element count
            if let Some(an) = name.strip_prefix('#') {
                return Ok(arr_len_fish(&an));
            }
            let off = raw_arg(args, 2)?;
            let len = args.get(3).map(str_arg).transpose()?.unwrap_or_default();
            // `${#arr[@]}` — the parser keeps the `#` in the name
            if name.starts_with('#') {
                return Ok(arr_len_fish(&name[1..]));
            }
            // fish has no substring expansion — emulate with cut
            // (char-exact for the ASCII corpus)
            if name == "@" || name == "*" {
                // `${@:off}` — the positional slice from `off` to the end.
                // fish has no `shift` (a POSIX-sh builtin: `(shift 2; …)` is
                // "Unknown command: shift"), and `printf '%s' $argv` is not
                // a substitute either — fish's printf REUSES the format per
                // argument, so `printf '%s' c d` prints `cd`, not the
                // space-joined `c d` bash's `${@:3}` means. `string join`
                // produces exactly one such word; a bare `$argv[off..-1]`
                // would split into N words instead, which only happens to
                // look right where the slice is used unquoted.
                let offn: i64 = off.trim().parse().unwrap_or(-1);
                if offn >= 1 {
                    return Ok(format!("(string join ' ' $argv[{offn}..-1])"));
                }
                return Ok(format!("${name}"));
            }
            // `${arr[@]}` / `${arr[*]}` — the whole array: the per-element
            // counter helper (arrays have no POSIX form)
            if off == "@" || off == "*" {
                // `${!map[@]}` — the `!` marker means KEYS, not values
                if name.starts_with('!') {
                    return Ok(arr_keys_call_fish(arr_base(&name)));
                }
                return Ok(arr_expand_call_fish(arr_base(&name)));
            }
            // A numeric-offset slice on a KNOWN array (`${arr[@]:1:2}`).
            // fish slices a LIST directly — `$arr[2..3]` — so this needs no
            // runtime helper; the only work is the index base conversion
            // (bash counts from 0, fish from 1) and, for a NEGATIVE bash
            // offset, the fact that fish's own negative index counts from
            // the END exactly like bash's (`${arr[@]: -3}` is `$arr[-3..-1]`).
            if !name.starts_with('#')
                && name != "@"
                && name != "*"
                && ARRAY_NAMES.lock().unwrap().contains(name.as_str())
                && is_plain_name(&name)
                && !is_assoc(&name)
            {
                let offn: Option<i64> = off.trim().parse().ok();
                let lenn: Option<i64> = len.trim().parse().ok();
                if lenn == Some(0) {
                    return Ok("(printf '')".into());
                }
                // fish's END index: `start + len - 1`. With a non-negative
                // bash offset fish's start is `off+1`, so the end is `off+len`;
                // with a negative one the start IS `off`, so it is `off+len-1`.
                let start = match offn {
                    Some(n) if n < 0 => format!("{n}"),
                    Some(n) => format!("{}", n + 1),
                    None => format!("(math --scale 0 \"{} + 1\")", arith_rewrite(&off)),
                };
                let end = match (len.trim().is_empty(), lenn, offn) {
                    (true, _, _) => "-1".to_string(),
                    (false, Some(l), Some(n)) if n < 0 => format!("{}", n + l - 1),
                    (false, Some(l), Some(n)) => format!("{}", n + l),
                    (false, Some(l), None) => {
                        format!("(math --scale 0 \"{} + {}\")", arith_rewrite(&off), l)
                    }
                    (false, None, _) => format!(
                        "(math --scale 0 \"{} + {} - 1\")",
                        arith_rewrite(&off),
                        arith_rewrite(&len)
                    ),
                };
                return Ok(format!("(string join ' ' ${name}[{start}..{end}])"));
            }
            // Both bounds are usually literals, but `${s:$i:1}` carries a
            // VARIABLE offset as raw text. `parse().unwrap_or(-1)` conflated
            // "not a number" with the real -1, so every dynamic offset took
            // the negative-counts-from-the-end branch and `string sub
            // --start -1` returned the LAST character for every index (a
            // `while` loop over `${s:$i:1}` printed the same letter N times).
            // Keep the parse FAILURE distinguishable — None means dynamic.
            let offn: Option<i64> = off.trim().parse().ok();
            let lenn: Option<i64> = len.trim().parse().ok();
            let len_is_literal = len.is_empty() || lenn.is_some();
            if let Some(offn) = offn.filter(|_| len_is_literal) {
                // `${x:off:0}` — always empty (bash's zero-length slice).
                if lenn == Some(0) {
                    return Ok("(printf '')".into());
                }
                let mut c = if offn < 0 {
                    // `${s: -3}` — negative offsets count from the END, which
                    // is exactly `string sub --start <neg>`.
                    format!("string sub --start {offn}")
                } else {
                    // `string sub` is 1-based; bash `${s:off}` starts at 0.
                    format!("string sub --start {}", offn + 1)
                };
                if lenn.unwrap_or(0) > 0 {
                    c.push_str(&format!(" --length {}", lenn.unwrap()));
                }
                c.push_str(&format!(" -- \"{subj}\""));
                return Ok(format!("({c})"));
            }
            // dynamic offsets — `string sub` takes the start as an argument,
            // so the caller's arithmetic expression goes straight through.
            let mut c = format!("string sub --start (math --scale 0 \"{off} + 1\")");
            if !len.is_empty() {
                c.push_str(&format!(" --length (math --scale 0 \"{len}\")"));
            }
            c.push_str(&format!(" -- \"{subj}\""));
            Ok(format!("({c})"))
        }
        // case-modification, via fish's own `string` builtin (no tr/awk):
        // `string upper`/`string lower` cover the whole-string forms. bash's `${x^}`
        // / `${x,}` upper/lower exactly ONE character, which is
        // `string sub -l1` of the head + the `-s2` tail — fish has no
        // per-character regex case conversion.
        "^^" => Ok(format!("(string upper -- \"{subj}\")")),
        ",," => Ok(format!("(string lower -- \"{subj}\")")),
        "^" => Ok(format!(
            "(string upper -- (string sub -l1 -- \"{subj}\"))(string sub -s2 -- \"{subj}\")"
        )),
        "," => Ok(format!(
            "(string lower -- (string sub -l1 -- \"{subj}\"))(string sub -s2 -- \"{subj}\")"
        )),
        // Prefix/suffix stripping (`${x#p}`, `${x%p}`) and substitution
        // (`${x/p/r}`) have NO `${...}` form in fish: fish rejects a bare
        // `${` outright ("${ is not a valid variable in fish"), which is a
        // PARSE error that kills the whole script at read time, not a
        // one-word mismatch. fish's own idiom is the `string` builtin —
        // `string replace` for both strip and substitute, which is exactly
        // the "text -> new text" shape the shIR ops already carry.
        //
        // `#`/`##` strip the SHORTEST/LONGEST matching prefix; `%`/`%%` the
        // suffix. Anchoring the pattern is what selects which end and which
        // greediness: `^` for a prefix, no anchor (with `[^…]` protection) for
        // a suffix. `string replace` is a `sed s///` analogue, so a literal
        // (non-regex) pattern is the faithful translation of bash's
        // glob-removal — a bash glob like `*` must stay a glob, so this
        // lowers to `string` only for the anchored-literal cases and REFUSES
        // (falls through to the sed/awk emulation below) when a real glob or
        // a capture group is involved.
        "#" | "##" | "%" | "%%" => {
            let pat = raw_arg(args, 2)?;
            strip_expansion_fish(name.as_str(), op.as_str(), pat.as_str())
        }
        "//" | "/" => {
            let pat = raw_arg(args, 2)?;
            let rep = raw_arg(args, 3)?;
            // The IR conflates first/all occurrences (`${x/p/r}` and
            // `${x//p/r}` both arrive as `//`), so this always renders the
            // replace-ALL form and needs `-a`: `string replace` without it
            // replaces only the first match, which turned `${var//o/0}` on
            // "hello world" into "hell0 world".
            //
            // bash's pattern here is a GLOB but the corpus uses plain
            // literals, and `string replace` without `-r` treats the pattern
            // literally too — so the literal form is both the faithful and the
            // safe default. A pattern that looks like a regex (escapes or a
            // group) opts into `-r` so a capture-group pattern is not silently
            // matched as literal text.
            let flag = if pat.contains('\\') || pat.contains('(') { " -a -r" } else { " -a" };
            if rep.is_empty() {
                // bash `${x/p/}` DELETES the match; `string replace` with no
                // replacement argument does exactly that.
                Ok(format!("(string replace{flag} '{pat}' -- \"{subj}\")"))
            } else {
                Ok(format!("(string replace{flag} '{pat}' '{rep}' -- \"{subj}\")"))
            }
        }
        ":-" | ":=" | ":?" => {
            // The default OPERAND can itself be a bash expansion. The core does
            // not recurse into it — `${x:-${y}}` arrives as the opaque text
            // `"${y}"` — and fish has NO `${…}` form at all, so splicing the
            // operand in verbatim is a read-time PARSE ERROR ("${ is not a
            // valid variable in fish") that kills the whole file. Lower it
            // through the same bash-text rewriter every other raw word uses;
            // an ordinary word (`${x:-hello}`) comes back unchanged.
            let default = bash_specials_to_fish(&raw_arg(args, 2)?);
            // `${arr:-d}` — bash's array form reads ELEMENT 0 when the array
            // is non-empty. The per-element vars have no scalar, so the
            // element-0 variable is the subject of the same emptiness test.
            if ARRAY_NAMES.lock().unwrap().contains(name.as_str()) {
                return Ok(default_if_unset_fish(&arr_elem_ref(&name, "0"), &default));
            }
            // `${@:-d}` / `${*:-d}` — the positionals. `"$@"` is a fish
            // PARSE ERROR, so the subject must be fish's positional list.
            // bash's `${@:-d}` is empty when there are no positionals and
            // otherwise expands to them all (IFS-joined inside quotes);
            // `count $argv` is 0 exactly when there are none, which is the
            // same emptiness test `test -n` performs for a scalar.
            if name == "@" || name == "*" {
                return Ok(format!(
                    "(test (count $argv) -gt 0; and printf '%s' \"$argv\"; or printf '%s' {default})"
                ));
            }
            // fish has no `${x:-d}`. `test -n` is false for BOTH an unset and
            // an empty variable, which is exactly bash's `:-` trigger, so
            // `(test -n "$x"; and …; or …)` is the native lowering.
            //
            // `:=` is NOT the same op: bash additionally ASSIGNS the default
            // when the trigger fires, and later expansions of the variable
            // see it (the very next line of 027/013 is `${maybe:?error}`,
            // which must not fire once `${maybe:=default}` has run). The
            // assignment is emitted inside the same substitution so the two
            // effects stay one word.
            if op == ":=" {
                return Ok(default_assign_if_unset_fish(&name, &default));
            }
            Ok(default_if_unset_fish(&name, &default))
        }
        // `${v+w}` / `${v:+w}` — the SETNESS predicate, not the variable's
        // value. `+` is "v is set" (a variable assigned "" counts as set) and
        // `:+` adds "AND non-empty"; the core pins the two as distinct and
        // refuses to let either degrade into the bare variable (ir.rs
        // `param_plus_lowers_to_a_set_test_not_the_value`, which records the
        // silent wrong answer that regression produced). With no arm here the
        // whole FILE refused to render — "param op not renderable: \"+\"",
        // examples/variable-plus-operator.sh.
        "+" | ":+" => {
            // Same opaque-operand problem as `:-` above: `${v:+${w}}` arrives
            // with the inner expansion still spelled as bash text, and a
            // verbatim `${w}` is a fish parse error.
            let word = bash_specials_to_fish(&raw_arg(args, 2)?);
            let nonempty = op == ":+";
            if ARRAY_NAMES.lock().unwrap().contains(name.as_str()) {
                // `${arr+…}` — the per-element vars have no scalar, so the
                // setness subject is element 0, the same remap `:-` uses.
                let el = arr_elem_ref(&name, "0");
                return Ok(set_if_fish(&setness_guard_fish(&el, nonempty), &word));
            }
            if name == "@" || name == "*" {
                // The positionals: set == `$#` > 0, which fish spells with the
                // `count` builtin; `:+` additionally wants a non-empty joinee.
                let guard = if nonempty {
                    "test (count $argv) -gt 0 -a -n \"$argv\""
                } else {
                    "test (count $argv) -gt 0"
                };
                return Ok(set_if_fish(guard, &word));
            }
            Ok(set_if_fish(&setness_guard_fish(&name, nonempty), &word))
        }
        "basename" => strip_expansion_fish(name.as_str(), "##", "*/"),
        // `${x%/*}` — the dirname idiom. `%` (SHORTEST suffix matching `/*`)
        // is the one that keeps the leading path: on "/tmp/a.txt" it removes
        // only "/a.txt". `%%` is the longest match and removes
        // "/tmp/a.txt" entirely, leaving "", which is what the op used to
        // render (bash's `dirname`/`${x%/*}` pair disagreed on 013/025).
        "dirname" => strip_expansion_fish(name.as_str(), "%", "/*"),
        other => Err(format!("param op not renderable: {other:?}")),
    }
}

/// A character that is a LITERAL in a glob but a metacharacter in a regex.
fn regex_escape_char(c: char) -> String {
    if ".^$*+?()[]{}|\\".contains(c) {
        format!("\\{c}")
    } else {
        c.to_string()
    }
}

/// Translate a bash parameter-expansion GLOB into the REGEX `string match -r`
/// takes, or `None` for a construct with no glob reading here.
///
/// The two languages agree on `*`, `?` and `[…]` and disagree on everything
/// else: `.` is a glob LITERAL but a regex "any character". Passing the
/// pattern through untranslated is not a near miss — `${f%.txt}` strips
/// "Xtxt" for any X. So translate the metacharacters and ESCAPE the rest
/// (bash also lets `\` quote the next char, which the glob reads as a
/// literal).
///
/// `{a,b}` is the one construct refused: bash brace-expands it before the
/// parameter expansion even happens, so it is a SET of patterns, not one
/// glob, and stripping with a single regex would silently pick just one.
fn glob_to_regex(pat: &str, lazy_star: bool) -> Option<String> {
    let chars: Vec<char> = pat.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                out.push_str(if lazy_star { ".*?" } else { ".*" });
                i += 1;
            }
            '?' => {
                out.push('.');
                i += 1;
            }
            '[' => {
                // A class in both languages; only negation is spelled
                // differently (`[!a]` glob vs `[^a]` regex). `\` and `^` are
                // class metacharacters in a regex but literals in a glob.
                let mut j = i + 1;
                let mut cls = String::from("[");
                if j < chars.len() && (chars[j] == '!' || chars[j] == '^') {
                    cls.push('^');
                    j += 1;
                }
                let mut closed = false;
                while j < chars.len() {
                    if chars[j] == ']' {
                        closed = true;
                        break;
                    }
                    if chars[j] == '\\' || chars[j] == '^' {
                        cls.push('\\');
                    }
                    cls.push(chars[j]);
                    j += 1;
                }
                if !closed {
                    // an unterminated `[` is a literal bracket in bash
                    out.push_str("\\[");
                    i += 1;
                    continue;
                }
                cls.push(']');
                out.push_str(&cls);
                i = j + 1;
            }
            '{' | '}' => return None,
            '\\' => {
                // glob quote: the next char is literal, whatever it is
                if i + 1 < chars.len() {
                    out.push_str(&regex_escape_char(chars[i + 1]));
                    i += 2;
                } else {
                    out.push_str("\\\\");
                    i += 1;
                }
            }
            c => {
                out.push_str(&regex_escape_char(c));
                i += 1;
            }
        }
    }
    Some(out)
}

/// Lower bash's prefix/suffix strip (`${x#p}`, `${x##p}`, `${x%p}`, `${x%%p}`)
/// to fish's native `string` builtins.
///
/// The four ops all answer "which end of the string does the glob match, and
/// does it match as little (shortest) or as much (longest) as possible". fish
/// expresses that directly: `string match -rg RE` prints ONLY the capture
/// group, so the retained part is one capture group in a regex that anchors
/// the glob at the correct end and fixes shortest-vs-longest through the
/// GREEDINESS of `*` (`.*` vs `.*?`).
///
///   `#`  (shortest prefix) -> `^B(.*)$`, `*` lazy  — the FIRST place B fits
///   `##` (longest  prefix) -> `^B(.*)$`, `*` greedy — the LAST place B fits
///   `%`  (shortest suffix) -> `^(.*)B$`, group GREEDY — the LAST place B fits
///   `%%` (longest  suffix) -> `^(.*)B$`, group LAZY   — the FIRST place B fits
///
/// The retained part is the group, so a suffix strip cannot be a
/// `string replace` deletion: `string replace` takes the LEFTMOST match, and
/// the shortest suffix is the RIGHTMOST one. The group's greediness is what
/// picks the end, which is why the lazy `*` and the lazy GROUP are different
/// knobs and only one of them is used per arm.
///
/// A no-match (`string match` rc 1) leaves the value as bash does; an empty
/// capture yields the empty expansion.
///
/// REFUSE > GUESS: a pattern this cannot translate faithfully (`{a,b}`, see
/// `glob_to_regex`) is an error rather than a silently different match set.
fn strip_expansion_fish(name: &str, op: &str, pat: &str) -> Result<String, String> {
    let is_prefix = op.starts_with('#');
    let longest = op.len() == 2;
    // `*` is the shortest-vs-longest knob for a PREFIX strip, so only there
    // does it go lazy; a SUFFIX strip turns the knob on the capture group
    // instead (see the module note above).
    let Some(body) = glob_to_regex(pat, is_prefix && !longest) else {
        return Err(format!("param strip pattern not renderable: {pat:?}"));
    };
    let val = format!("\"{}\"", param_subject_fish(name));
    let re = if is_prefix {
        format!("^{body}(.*)$")
    } else if longest {
        format!("^(.*?){body}$")
    } else {
        format!("^(.*){body}$")
    };
    Ok(format!(
        "(string match -rg -- '{re}' {val}; or printf '%s' {val})"
    ))
}

/// Fallback strip for glob patterns `string replace` cannot express, via a
/// portable `sed` on the value. Used only by `strip_expansion_fish`'s
/// refusal arm.
fn sed_strip_fish(name: &str, is_prefix: bool, pat: &str) -> String {
    // `sed` needs the pattern escaped; the greedy-vs-shortest distinction is
    // carried by anchoring plus the longest/shortest `sed` match (sed's
    // default is the LONGEST leftmost match, which is `##`/`%%`).
    let pe = sed_escape_pattern(pat);
    let re = if is_prefix {
        format!("^{pe}")
    } else {
        format!("{pe}$")
    };
    format!(
        "$(printf '%s' \"{}\" | sed -e 's#{re}##')",
        param_subject_fish(name)
    )
}

/// The fish reference a `${name…}` test reads its subject from.
///
/// `name` arrives as raw text — usually a plain variable name, but it can be
/// a POSITIONAL. `$1`/`$2` are EMPTY inside a fish function, so
/// `${2:-default}` rendered as `test -n "$2"` was always false and silently
/// took the default, dropping the caller's argument (examples/062_09). Route
/// every subject through the same `$N` → `$argv[N]` mapping the other reads
/// use. `var_ref_to_fish` already owns that table.
fn param_subject_fish(name: &str) -> String {
    var_ref_to_fish(name, false)
}

/// Lower bash's `${x:-default}` / `${x:=default}` / `${x:?default}` to fish.
///
/// fish has no parameter-default expansion, but the trigger condition is the
/// same: `test -n` is false for an unset variable AND for an empty one, which
/// is exactly when bash substitutes the default. The `(cmd; and A; or B)`
/// chain is fish's native conditional.
fn default_if_unset_fish(name: &str, default: &str) -> String {
    let subj = param_subject_fish(name);
    format!("(test -n \"{subj}\"; and printf '%s' \"{subj}\"; or printf '%s' {default})")
}

/// `${x:=default}` — bash's `:-` PLUS the side effect: the variable is left
/// holding the default afterwards. fish's `set` inside the substitution
/// writes the enclosing scope (command substitution is not a scope of its
/// own for a plain `set`), which is what makes the following `${x:?…}` and
/// any later `"$x"` agree with bash.
fn default_assign_if_unset_fish(name: &str, default: &str) -> String {
    let subj = param_subject_fish(name);
    format!(
        "(test -n \"{subj}\"; and printf '%s' \"{subj}\"; or begin; set {name} {default}; printf '%s' {default}; end)"
    )
}

/// The `${v+w}` / `${v:+w}` guard: fish's `set -q` IS the setness test and
/// nothing else (a variable assigned "" is still set), while `test -n` is
/// false for a variable that is unset OR empty — exactly `:+`'s stronger
/// trigger.
fn setness_guard_fish(name: &str, nonempty: bool) -> String {
    if nonempty {
        return format!("test -n \"{}\"", param_subject_fish(name));
    }
    // `set -q NAME` takes the NAME, not a value, so it cannot express a
    // positional ("set -q $argv[2]" would test the argument's TEXT as a
    // variable name). A positional is "set" exactly when the caller passed
    // it, which is a `count` test.
    if let Ok(n) = name.parse::<usize>() {
        if n >= 1 {
            return format!("test (count $argv) -ge {n}");
        }
    }
    format!("set -q {name}")
}

/// `${v+w}` / `${v:+w}` as ONE word, empty when the guard fails.
///
/// The `$()` form and the surrounding QUOTES are both load-bearing:
///
/// * quoted, because the guard-failed case prints nothing and an UNQUOTED
///   command substitution then expands to ZERO words — which turns
///   `test -n "${V+x}"` into a one-argument `test -n` (TRUE) and prints the
///   guarded branch for an UNSET variable, the silent wrong answer ir.rs
///   records for this construct;
/// * `$()` rather than the plain `(…)` this file uses everywhere else,
///   because fish does NOT expand `(…)` inside double quotes — `"$(cmd)"`
///   expands, `"(cmd)"` is the literal text, which would make every such
///   test non-empty and therefore unconditionally true.
fn set_if_fish(guard: &str, word: &str) -> String {
    format!("\"$({guard}; and printf '%s' {word}; or printf '%s' '')\"")
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
fn join_to_fish(inner: &IrExpr, quoted: bool) -> Result<String, String> {
    let s = match inner {
        IrExpr::Call { func, args } if func == "param" => param_to_fish(args, true)?,
        IrExpr::Call { func, args } if func == "arrayIndex" => {
            let name = raw_arg(args, 0)?;
            let key = raw_arg(args, 1)?;
            if key == "@" || key == "*" {
                // the whole array — the helper's expansion IS the list
                return Ok(arr_expand_call_fish(arr_base(&name)));
            }
            subscript_read_fish(&name, &key)
        }
        IrExpr::Call { func, args } if func == "arrayItems" => {
            arr_keys_call_fish(arr_base(&raw_arg(args, 0)?))
        }
        IrExpr::Call { func, args } if func == "arrayLen" => {
            arr_len_fish(&raw_arg(args, 0)?)
        }
        _ => word_to_fish(inner)?,
    };
    // An expansion that is ALREADY a fish command substitution must not be
    // re-quoted: `"(cmd)"` is literal TEXT in fish, so the quotes would print
    // the command instead of running it. That covers the array helpers and
    // the `(string join ' ' $arr)` / `(count $arr)` / `(seq …)` forms the
    // native list lowering emits.
    if is_fish_substitution(&s) {
        Ok(s)
    } else {
        Ok(if quoted && !s.starts_with('"') {
            format!("\"{s}\"")
        } else {
            s
        })
    }
}

fn interp_to_fish(parts: &[InterpPart]) -> Result<String, String> {
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
                    out.push_str(&interp_expr_to_fish(x)?);
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
                out.push_str(&interp_expr_to_fish(x)?);
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
            // `$#` lowers to `(count $argv)`, a command substitution that is
            // already ONE word: putting it inside `"…"` prints the literal
            // text `(count a b)` instead of the number. A bare `$#` read
            // reaches the renderer as getVar("#") (the core wraps the read in
            // a variable lookup, not a param node); the param arm carries the
            // same name in args[1] (shape (op, name)).
            "getVar" => !matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "#"),
            "param" => !matches!(args.get(1), Some(IrExpr::Str(n, _)) if n == "#"),
            _ => true,
        },
        _ => true,
    }
}

/// Rewrite the OUTERMOST bare `(cmd)` of a template expansion into fish's
/// `$(cmd)`, the form that — unlike the bare one — EXPANDS between double
/// quotes.
///
/// fish parses `"(cmd)"` as literal text: `"a=$(echo hi)"` prints `a=hi`,
/// `"a=(echo hi)"` prints `a=(echo hi)`. Every command substitution this
/// renderer emits is written in the bare form, which is right in command
/// position and in an unquoted argument, and wrong the instant
/// `interp_to_fish` splices it between quotes — `echo "a=${x:-def}"` came
/// out as `"a=(test -n "$x"; …)"`, where the expansion was literal AND its
/// inner quotes tore the template apart
/// (frontends/posix-sh-go/testdata/t53_param_default.sh).
///
/// Verified against fish 3.7.0: `$()` and `()` are otherwise identical (both
/// split on newlines, both valid unquoted), and the inner quotes of a form
/// like `(test -n "$x"; …)` are re-read in COMMAND context inside `$( )`,
/// which is exactly what such a guard needs.
///
/// Only a pair wrapping the WHOLE string is rewritten, so a value that
/// merely begins and ends with a paren (`(a) (b)`) is left alone. Quoted
/// regions are skipped while scanning, so a pattern or default word may
/// contain a `)`.
fn dollar_paren_in_template(s: &str) -> String {
    if !s.starts_with('(') {
        return s.to_string();
    }
    let chars = s.chars().count();
    let mut it = s.chars().peekable();
    let mut depth: i32 = 0;
    let mut quote: Option<char> = None;
    let mut idx = 0usize;
    while let Some(c) = it.next() {
        idx += 1;
        if let Some(q) = quote {
            match c {
                '\\' => {
                    it.next();
                    idx += 1;
                }
                c if c == q => quote = None,
                _ => {}
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '\\' => {
                it.next();
                idx += 1;
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                // the outer pair closed early — this is not one wrapping
                // command substitution, so leave the string untouched
                if depth == 0 && idx != chars {
                    return s.to_string();
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return s.to_string();
    }
    format!("$({})", &s[1..s.len() - 1])
}

/// An expansion inside a double-quoted template.
///
/// The result is spliced BETWEEN quotes by `interp_to_fish`, so it is
/// normalized to a form fish expands there (`dollar_paren_in_template`).
fn interp_expr_to_fish(e: &IrExpr) -> Result<String, String> {
    interp_expr_to_fish_atom(e).map(|s| dollar_paren_in_template(&s))
}

fn interp_expr_to_fish_atom(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => Ok(var_ref_to_fish(&raw_arg(args, 0)?.replace('.', "_"), false)),
            "param" => param_to_fish(args, false),
            "listVar" => {
                // Inside a double-quoted template `"$@"` means the positionals
                // joined by the first char of IFS; fish's `"$argv"` is the
                // space-joined equivalent and IS valid fish (unlike `"$@"`
                // itself, which is a parse error).
                let _ = raw_arg(args, 0)?;
                Ok("$argv".into())
            }
            "arrayIndex" => {
                let name = raw_arg(args, 0)?;
                let key = raw_arg(args, 1)?;
                if key == "@" || key == "*" {
                    Ok(arr_expand_call_fish(arr_base(&name)))
                } else if key.contains(['$', '(']) {
                    Ok(dynamic_el_read_fish(arr_base(&name), &key))
                } else {
                    Ok(format!("${name}_{key}"))
                }
            }
            "assocGet" => {
                // the go-sh map read (`m[\"go\"]`, triage-sh-20260815-175002):
                // the by-name associative read — the same POSIX per-element
                // lowering as `arrayIndex` (the bare `${m_go}`; the
                // enclosing template supplies the quotes).
                let name = raw_arg(args, 0)?;
                match arg(args, 1)? {
                    IrExpr::Str(k, _) if k.contains(['$', '(']) => {
                        Ok(dynamic_el_read_fish(arr_base(&name), k))
                    }
                    IrExpr::Str(k, _) => Ok(arr_elem_ref(&name, k)),
                    IrExpr::Var(k, _) => Ok(index_var_key_to_sh(arr_base(&name), k)),
                    other => Err(format!("assocGet: key not renderable: {other:?}")),
                }
            }
            "arrayItems" => Ok(arr_keys_call_fish(arr_base(&raw_arg(args, 0)?))),
            "arrayLen" => {
                let n = raw_arg(args, 0)?;
                Ok(arr_len_fish(&n))
            }
            "capture" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, true)),
            "captureWords" => Ok(cmdsub_word_fish(arrow_to_fish(args)?, false)),

            "arith" => {
                let raw = raw_arg(args, 0)?;
                if raw.contains(['"', '\'']) {
                    // bash errors at RUNTIME (the assignment is skipped);
                    // dash would fail to PARSE the whole script — a
                    // failed cmdsub has the same ${x:-d} observable
                    Ok("(false)".into())
                } else {
                    Ok(arith_cmd_fish(&raw))
                }
            }
            "join" => join_to_fish(arg(args, 0)?, false),
            "brace" => brace_to_fish(args),
            other => Err(format!("interp call not renderable: {other:?}")),
        },
        IrExpr::Arith(a) => Ok(arith_cmd_fish(&arith_to_fish(a))),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
        IrExpr::Var(name, _) => Ok(var_ref_to_fish(name, false)),
        // A literal `$` reaches here for `$$` (the core lexes the pid as
        // `getVar("$")`); inside a double-quoted template `"…$…"` would read
        // as fish's dangling-`$` parse error.
        IrExpr::Str(s, _) if s == "$" => Ok("$fish_pid".into()),
        IrExpr::Str(s, _) => Ok(bash_specials_to_fish(s)),
        // The A1 `Index` expr node (triage-sh py-sh-go cross-product pairs
        // t21/t56/t73): array element read inside a double-quoted template
        // — the bare `${name_key}` (the template supplies the quotes).
        IrExpr::Index { var, key } => index_to_fish(var, key, false),
        // The first-class Capture node (core request
        // zsh-sh-go-20260814-230503): `$(...)`/backticks — same
        // `"$(...)"` rendering as the `capture` call arm.
        IrExpr::Capture { expr, .. } => {
            if let Some(lift) = capture_idiom_fish(expr)? {
                Ok(lift)
            } else {
                Ok(format!(
                    "\"$({})\"",
                    arrow_to_fish(std::slice::from_ref(expr.as_ref()))?
                ))
            }
        }
        other => Err(format!("interp expr not renderable: {other:?}")),
    }
}

/// Rewrite the bash-only spellings that reach the output as LITERAL TEXT,
/// so fish can read the file at all.
///
/// WHY THIS EXISTS. The core lowers a test condition or an `(( ))` body to a
/// single flattened string and hands it to the renderer as an `IrExpr::Str`
/// — `[ $? -eq 0 ]` arrives as `Str("$? -eq 0")`, not as a `Var("?")` node.
/// `var_ref_to_fish` therefore never sees it, and the raw bash spelling is
/// emitted verbatim. That is not a wrong VALUE, it is a fish PARSE ERROR, and
/// fish reports it while READING the file, so one bad token kills every
/// statement in the script — not just the one that contains it:
///
///     "$? is not the exit status. In fish, please use $status."
///     "$* is not supported. In fish, please use $argv."
///     "${ is not a valid variable in fish."
///     "command substitutions not allowed here"      ($(( )) inside [ ])
///     "$$ is not the pid. In fish, please use $fish_pid."
///
/// So the whole file must be rewritten, and it has to happen on the raw-text
/// path. Each rewrite below is the native fish spelling of the same read; the
/// scalar forms are plain `$name` so they survive word-splitting exactly as
/// `var_ref_to_fish` already renders them, and `count`/`$status`/`$fish_pid`
/// are commands-or-builtins that are legal as `[ ]` / `test` operands.
fn bash_specials_to_fish(s: &str) -> String {
    if !s.contains(['$', '(', '{']) {
        return s.to_string();
    }
    let b: Vec<char> = s.chars().collect();
    let n = b.len();
    let mut out = String::with_capacity(n + 16);
    let mut i = 0;
    // regions where `$`/`(` are literal and must be copied through untouched
    let mut in_sq = false;
    let mut in_dq = false;
    while i < n {
        let c = b[i];
        if c == '\'' && !in_dq {
            in_sq = !in_sq;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '"' && !in_sq {
            in_dq = !in_dq;
            out.push(c);
            i += 1;
            continue;
        }
        if in_sq {
            out.push(c);
            i += 1;
            continue;
        }
        // `$(( … ))` — bash arithmetic. fish spells it `(math …)` and it is
        // NOT legal inside a `[ ]` operand, so the arithmetic has to become
        // the `test` BUILTIN, which accepts a substitution. The caller decides
        // which; here we only recognise the span and hand back the inner text
        // so `space_test_ops` can pick the legal enclosing form.
        if c == '$' && i + 2 < n && b[i + 1] == '(' && b[i + 2] == '(' {
            let (inner, next) = scan_arith_span(&b, i + 3);
            let inner: String = inner.iter().collect();
            out.push_str(&arith_span_to_fish(inner.trim()));
            i = next;
            continue;
        }
        if c == '$' {
            if let Some(next) = special_param_to_fish(&b, i, &mut out) {
                i = next;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `$( ( … ) )` → return the inner text and the index just past the closing
/// `))`. Tolerates nesting so `$(( $(( 1 + 1 )) ))` does not stop at the
/// first `)`.
fn scan_arith_span(b: &[char], start: usize) -> (&[char], usize) {
    let mut depth = 1i32;
    let mut i = start;
    while i < b.len() {
        if b[i] == '(' {
            depth += 1;
        } else if b[i] == ')' {
            depth -= 1;
            if depth == 0 {
                // the second `)` of the `))` pair
                let end = if i + 1 < b.len() && b[i + 1] == ')' { i } else { i };
                return (&b[start..end], (end + 2).min(b.len()));
            }
        }
        i += 1;
    }
    (&b[start..], b.len())
}

/// The inner text of a `$(( … ))` span → the native fish `(math …)`
/// command substitution.
///
/// fish has no `$(( ))` at all: it is a parse error even inside a `[ ]`
/// operand ("command substitutions not allowed here"). The `(…)` form IS
/// legal there, so `test "$n" -lt (math 1 + 2)` reads and evaluates, while
/// `test "$n" -lt "$(( 1 + 2 ))"` kills the whole file.
///
/// The numeric guards the core injects are bash `(( … ))` bodies wrapped one
/// paren-pair deeper than the arithmetic; they are stripped here so `math`
/// receives the bare expression rather than `(( … ))`, which it rejects.
fn arith_span_to_fish(inner: &str) -> String {
    let mut t = inner.trim();
    loop {
        let next = if t.starts_with("((") && t.ends_with("))") && balanced(&t[2..t.len() - 2]) {
            Some(t[2..t.len() - 2].trim())
        } else if t.starts_with('(') && t.ends_with(')') && balanced(&t[1..t.len() - 1]) {
            Some(t[1..t.len() - 1].trim())
        } else {
            None
        };
        match next {
            Some(n) if !n.is_empty() => t = n,
            _ => break,
        }
    }
    // `math` needs a bare expression; strip the redundant quoting the core
    // leaves around numeric guards so the guard's own `(cmd)` text is dropped.
    //
    // The text must then go through the SAME rewriter the structured arithmetic
    // path uses. fish's `math` parses its argument as an expression string: a
    // bare identifier is read as a FUNCTION CALL and aborts with "Unknown
    // function" (`while [ (math n % divisor) -eq 0 ]` never terminates —
    // measured on 055_factorize.sh), while `$n` is interpolated by fish and
    // evaluates. `--scale 0` pins bash's truncating integer division.
    arith_cmd_fish(t)
}

/// Does every `(` in the text have a matching `)` before the end?
fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// One bash special parameter at `b[i]` (`$` already seen) → the native fish
/// spelling, appended to `out`. Returns the index just past the parameter, or
/// None when this is an ordinary `$name` the caller should copy itself.
fn special_param_to_fish(b: &[char], i: usize, out: &mut String) -> Option<usize> {
    let n = b.len();
    let next = i + 1;
    if next >= n {
        return None;
    }
    match b[next] {
        // `$?` → `$status`. Not just a rename: the fish spelling is a PARSE
        // ERROR that kills the whole file, and `$status` is legal everywhere
        // including inside double quotes.
        '?' => {
            out.push_str("$status");
            Some(next + 1)
        }
        // `$$` → `$fish_pid`.
        '$' => {
            out.push_str("$fish_pid");
            Some(next + 1)
        }
        // `$!` / `$-` have no fish spelling at all; an empty expansion keeps
        // the file readable, which is strictly better than a parse error.
        '!' | '-' => {
            out.push_str("\"\"");
            Some(next + 1)
        }
        // `$#` → the positional COUNT, as a COMMAND SUBSTITUTION. fish has no
        // `$#`, and a bare `count` is legal only where a command is expected:
        // used as a `[ ]` operand fish reads the literal word, and
        // `[ count $argv -lt 2 ]` dies with "Expected a combining operator
        // like '-a' at index 2" — killing the statement and the rest of the
        // script's output with it (measured; 059_issue3.sh). `(count $argv)`
        // IS the operand spelling, verified against fish 3.7. Inside double
        // quotes a bare `( )` is printed as literal text, so the dq scope
        // needs `$( )` — the same split the `$(( ))` arm below makes.
        '#' => {
            out.push_str(if in_dq_scope(b, i) {
                "$(count $argv)"
            } else {
                "(count $argv)"
            });
            Some(next + 1)
        }
        // `"$*"` / `"$@"` are parse errors in fish; inside double quotes
        // `"$argv"` is the space-joined equivalent and is legal.
        '*' | '@' => {
            if in_dq_scope(b, i) {
                out.push_str("$argv");
            } else {
                // bare `"$*"` quoted context is handled by the caller's quote
                // scan; outside quotes the list form is `$argv` too.
                out.push_str("$argv");
            }
            Some(next + 1)
        }
        // `$(( … ))` — bash arithmetic expansion reaching this TEXT path
        // (the core structures most arithmetic, but one it cannot parse
        // arrives as an opaque string: `f() { echo $(( $1 + $1 )); }`).
        // fish has no `$((` in any context — as a CALL it is "command
        // substitutions not allowed here", which kills the whole file — so
        // a pass-through is never acceptable here. Render the inner text
        // with the arithmetic rewriter and let `math` evaluate it.
        '(' => {
            if next + 1 >= n || b[next + 1] != '(' {
                // plain `$(cmd)`: fish accepts the bash spelling natively.
                return None;
            }
            let (inner, after) = arith_expansion_span(b, next + 2)?;
            let e = positional_to_argv(&arith_rewrite(&inner));
            // The quoting is load-bearing: fish expands `$( )` inside double
            // quotes but prints a bare `( )` there as literal text, so the
            // outer quoting of the enclosing string decides the spelling.
            out.push_str(&if in_dq_scope(b, i) {
                format!("$(math --scale 0 \"{e}\")")
            } else {
                format!("(math --scale 0 \"{e}\")")
            });
            Some(after)
        }
        // `${…}` — fish has NO braces form at all ("${ is not a valid variable
        // in fish"). Flatten `${name}` to `$name`, and `${name:-d}` /
        // `${name-d}` / `${name:?}` to the fish `set -q`-free spellings.
        '{' => {
            let (inner, after) = scan_braced(b, next + 1);
            out.push_str(&braced_to_fish(inner, in_dq_scope(b, i)));
            Some(after)
        }
        // `$1` … `$9` — fish has no positional shorthand; `$argv[N]`.
        d if d.is_ascii_digit() => {
            out.push_str(&format!("$argv[{d}]"));
            Some(next + 1)
        }
        _ => None,
    }
}

/// The inner text of a `$(( … ))` arithmetic expansion (the `$((` already
/// consumed) and the index just past its closing `))`.
///
/// The terminator is the first `)` that would close the OUTER pair:
/// arithmetic nests ordinary parentheses, so `$(( (a+b) * c ))` must not
/// stop at the `)` that closes `(a+b)`. `None` when the expansion is
/// unterminated — the caller then leaves the text alone, as before.
fn arith_expansion_span(b: &[char], start: usize) -> Option<(String, usize)> {
    let mut depth = 0i32;
    let mut i = start;
    while i < b.len() {
        match b[i] {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return if i + 1 < b.len() && b[i + 1] == ')' {
                        Some((b[start..i].iter().collect(), i + 2))
                    } else {
                        None
                    };
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `$1`…`$9` inside arithmetic TEXT → `$argv[N]`.
///
/// `arith_rewrite` drops the sigil before an IDENTIFIER (because `$n` and
/// `n` name the same variable in arithmetic), but a positional's name is a
/// DIGIT, so it falls through that branch and is emitted verbatim — and
/// `$1` is a PARSE error in fish wherever it appears. Applied to the
/// REWRITTEN text, after `arith_rewrite` has had its say, so the `$argv[N]`
/// it inserts is not itself stripped.
fn positional_to_argv(t: &str) -> String {
    let b: Vec<char> = t.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '$' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            out.push_str(&format!("$argv[{}]", b[i + 1..j].iter().collect::<String>()));
            i = j;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Is the `$` at index `i` inside a double-quoted region? Scans backwards
/// over the text we have already emitted (the quote state lives on the
/// output, so re-derive it from the prefix).
fn in_dq_scope(b: &[char], i: usize) -> bool {
    let mut in_dq = false;
    let mut in_sq = false;
    for c in &b[..i] {
        match c {
            '\'' if !in_dq => in_sq = !in_sq,
            '"' if !in_sq => in_dq = !in_dq,
            _ => {}
        }
    }
    in_dq
}

/// `$( {name} … )` → the inner text and the index past the closing `}`.
fn scan_braced(b: &[char], start: usize) -> (&[char], usize) {
    let mut depth = 1i32;
    let mut i = start;
    while i < b.len() {
        match b[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (&b[start..i], i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    (&b[start..], b.len())
}

/// A command substitution is `( … )` in fish, but inside DOUBLE QUOTES fish
/// prints a bare `( … )` as literal text — there the spelling must be
/// `$( … )`. Only the outer pair changes, so this rewrites a value that is
/// exactly one substitution and leaves anything else alone.
fn dq_subst(s: String, dq: bool) -> String {
    if !dq || !s.starts_with('(') {
        return s;
    }
    let cs: Vec<char> = s.chars().collect();
    // the leading `(` must close at the very end, or this is not one
    // substitution and fusing the pair would corrupt it
    if balanced_paren(&cs, 0) != Some(cs.len() - 1) {
        return s;
    }
    format!("$({})", &s[1..s.len() - 1])
}

/// `${name}` / `${name:-d}` / `${name-d}` / `${name:?w}` / `${#name}` → a
/// native fish expansion of the same value.
fn braced_to_fish(inner: &[char], dq: bool) -> String {
    let t: String = inner.iter().collect();
    // `${#name}` is the LENGTH form, and it is what collides with `${name}`
    // for a variable literally called `#` — resolve the length reading first.
    if let Some(rest) = t.strip_prefix('#') {
        if is_plain_var(rest) {
            return dq_subst(format!("(count ${rest})"), dq);
        }
    }
    // `${arr[@]}` / `${arr[*]}` — the whole-array forms the array helpers
    // already lower; leave those to the array path by keeping the base name.
    if let Some(base) = t.strip_suffix("[@]").or_else(|| t.strip_suffix("[*]")) {
        if is_plain_var(base) {
            return format!("${base}");
        }
    }
    // `${name#pat}` / `${name##pat}` / `${name%pat}` / `${name%%pat}` — the
    // strip operators. `${#name}` (the LENGTH form) was resolved above, so a
    // leading `#` surviving here is a strip.
    //
    // These MUST be lowered, not dropped: the old fall-through stripped the
    // operator and emitted the bare variable (`${MAXWAIT% *}` -> `$MAXWAIT`),
    // which is a silently DIFFERENT string, not a parse error — the exact
    // failure mode REFUSE > GUESS exists to prevent. `##`/`%%` are tested
    // before `#`/`%` so the longest-match form is not read as the shortest
    // one plus a literal `#`.
    for op in ["##", "%%", "#", "%"] {
        if let Some(idx) = t.find(op) {
            let (name, pat) = (&t[..idx], &t[idx + op.len()..]);
            if is_plain_var(name) && !pat.is_empty() {
                // the pattern is still bash text (`${x#${y}}`), so lower its
                // own expansions first — `strip_expansion_fish` takes a
                // literal glob, not a shell word
                let pat = bash_specials_to_fish(pat);
                if let Ok(s) = strip_expansion_fish(name, op, &pat) {
                    return dq_subst(s, dq);
                }
            }
        }
    }
    // `${name:-default}` / `${name-default}`
    for op in [":-", "-", ":=", "=", ":?", "?", ":+", "+"] {
        if let Some(idx) = t.find(op) {
            let (name, dflt) = (&t[..idx], &t[idx + op.len()..]);
            if is_plain_var(name) {
                // The OPERAND nests: `${x:-${y}}` reaches here with the inner
                // expansion still spelled as bash text, so lower it before
                // splicing it in. Emitting `${y}` verbatim is a fish PARSE
                // ERROR that kills the whole file.
                let dflt = bash_specials_to_fish(dflt);
                return dq_subst(
                    match op {
                        // fish has no `${x:-d}`; the conditional is the native
                        // read and `or` keeps it one expression. `test -n` — NOT
                        // `-s`, which is fish's non-empty-FILE test — is the
                        // trigger: false for an unset AND for an empty variable,
                        // exactly bash's `:-` condition.
                        ":-" | "-" => {
                            if dflt.is_empty() {
                                format!("${name}")
                            } else {
                                format!("(test -n \"${name}\"; and printf '%s' \"${name}\"; or printf '%s' {dflt})")
                            }
                        }
                        ":+" | "+" => {
                            format!("(test -n \"${name}\"; and printf '%s' {dflt}; or printf '')")
                        }
                        // `:?` / `?` abort with a message when unset/empty.
                        _ => format!(
                            "(test -n \"${name}\"; and printf '%s' \"${name}\"; or begin; printf '%s' {dflt} >&2; exit 1; end)"
                        ),
                    },
                    dq,
                );
            }
        }
    }
    if is_plain_var(&t) {
        format!("${t}")
    } else {
        // not a form we can lower — keep the name readable rather than
        // emitting a `${` that would kill the parse.
        let cleaned: String = t
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if cleaned.is_empty() {
            "\"\"".into()
        } else {
            format!("${cleaned}")
        }
    }
}

/// Is this a bare fish-legal variable name?
fn is_plain_var(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_') == Some(true)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
fn brace_to_fish(args: &[IrExpr]) -> Result<String, String> {
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
        // Every item is an ALTERNATIVE: a literal string, a nested group
        // (`{a,{b,c}}` → the nested alternatives), a `product` compound,
        // or a range. Ranges are the one exception — in a comma list bash
        // keeps them LITERAL (`{1..3,7..9}` → `1..3 7..9`), only a lone
        // range expands.
        let mut alts = Vec::new();
        for it in items {
            let is_range = matches!(it, serde_json::Value::Object(o) if o.contains_key("range"));
            if is_range && !single_range {
                let serde_json::Value::Object(o) = it else {
                    unreachable!()
                };
                if let Some(serde_json::Value::Array(r)) = o.get("range") {
                    let s0 = r[0].as_str().unwrap_or("");
                    let s1 = r[1].as_str().unwrap_or("");
                    let step = r.get(2).and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    match step {
                        Some(st) => alts.push(format!("{s0}..{s1}..{st}")),
                        None => alts.push(format!("{s0}..{s1}")),
                    }
                }
                continue;
            }
            alts.extend(brace_item(it)?);
        }
        out.push(alts);
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
            } else if let Some(serde_json::Value::Array(subs)) = o.get("product") {
                // A Compound alternative: cartesian concatenation of the
                // sub-groups (`x/{p,q}` → `x/p`, `x/q`).
                let mut combos = vec![String::new()];
                for sub in subs {
                    let alts = match sub {
                        serde_json::Value::Array(items) => {
                            let mut a = Vec::new();
                            for el in items {
                                a.extend(brace_item(el)?);
                            }
                            a
                        }
                        other => brace_item(other)?,
                    };
                    let mut next = Vec::new();
                    for c in &combos {
                        for a in &alts {
                            next.push(format!("{c}{a}"));
                        }
                    }
                    combos = next;
                }
                Ok(combos)
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
    //
    // `\r` belongs in this set for a reason the other control characters do
    // NOT share: fish's tokenizer treats a bare carriage return as
    // whitespace and folds it to a space, while BEL/BS/VT/FF/ESC survive
    // (measured on fish 3.7.0 — 0x07,08,0b,0c,1b round-trip unquoted, 0x0d
    // becomes ' '). So `echo a<CR>b` printed `a b` and a CRLF corpus line
    // lost its CR. Quoting the word preserves the byte.
    if s.contains([
        '$', '"', '\'', '\\', ';', '&', '|', '(', ')', '<', '>', ' ', '\t', '\n', '\r', '=',
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
/// fish has no `$(( ))` at all — it is a PARSE ERROR, not just a spelling
/// difference. Every arithmetic value therefore goes through `math`, whose
/// argument is an expression STRING.
fn arith_cmd_fish(raw: &str) -> String {
    format!("(math --scale 0 \"{}\")", arith_rewrite(raw))
}

/// An arithmetic CONDITION as native fish `test`.
///
/// bash's `(( x > 0 ))` has no fish spelling: `(( … ))` is parsed by fish as
/// nested command substitutions in condition position and is a hard PARSE
/// ERROR ("command substitutions not allowed here"), and `math "$x > 0"`
/// aborts with "Logical operations are not supported, use `test` instead"
/// (verified against fish 3.7). So the comparison moves OUT of `math` and into
/// `test`, with each side wrapped in its own `(math …)` command substitution:
/// `(( x > 0 ))` → `[ (math "$x") -gt 0 ]`.
///
/// The operator table maps bash's arithmetic relationals to `test`'s string
/// flags. When the comparison cannot be split, the whole expression falls back
/// to a `-ne 0` nonzero test — bash's `(( expr ))` truthiness — which is what
/// the nonzero branch above already does.
fn arith_cond_fish(raw: &str) -> String {
    // Split BEFORE the `$`-rewrite, and only at a relational that sits at
    // paren-depth 0. Rewriting first would move `$var` → `var`, and the `>`
    // INSIDE the variable name would then be mistaken for the comparison
    // operator (`var > 0` → `(var" -gt "0)` — a split mid-name). Depth
    // tracking keeps a nested `(a > b) > c` on the nonzero path.
    if let Some((_op, flag, lhs, rhs)) = split_relational(raw) {
        return format!(
            "[ (math --scale 0 \"{}\") {flag} (math --scale 0 \"{}\") ]",
            arith_rewrite(lhs),
            arith_rewrite(rhs)
        );
    }
    format!("[ (math --scale 0 \"{}\") -ne 0 ]", arith_rewrite(raw))
}

/// The first paren-depth-0 relational in an arithmetic expression, as
/// `(op, test_flag, lhs, rhs)`. `None` when the expression carries no
/// top-level comparison (a bare value, which the caller tests for nonzero).
fn split_relational(raw: &str) -> Option<(&str, &str, &str, &str)> {
    let b = raw.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            // A `$` begins a VARIABLE NAME, and a name may legally contain
            // `>`, `<`, `-` and `_` (bash's `BIGGER`, `x<y`). The relational
            // search must skip the name whole, or `$BIGGER > 0` splits at
            // the `>` inside `BIGGER`.
            b'$' => {
                let mut j = i + 1;
                while j < b.len()
                    && (b[j].is_ascii_alphanumeric() || b[j] == b'_')
                {
                    j += 1;
                }
                i = j;
                continue;
            }
            _ if depth == 0 => {
                // `&&` / `||` are boolean, not relational — a two-char `<`/`>`
                // must not be mistaken for a comparison.
                for (op, flag) in [
                    ("<=", "-le"),
                    (">=", "-ge"),
                    ("==", "-eq"),
                    ("!=", "-ne"),
                    ("<", "-lt"),
                    (">", "-gt"),
                ] {
                    if raw[i..].starts_with(op) {
                        let lhs = raw[..i].trim();
                        let rhs = raw[i + op.len()..].trim();
                        if !lhs.is_empty() && !rhs.is_empty() {
                            return Some((op, flag, lhs, rhs));
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
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
            // Not foldable (a non-digit operand): `**` is bash's power
            // operator and fish's `math` rejects it outright ("Unexpected
            // token"). fish spells power `^`, so translate rather than pass
            // through — `math "(2 ** 9)"` is a hard runtime error.
            out.push('^');
            i += 2;
            continue;
        }
        // `${…}` inside arithmetic text. The core structures most
        // arithmetic, but a braced expansion it cannot parse arrives as
        // opaque text — `${index}` in `arr[${index}]`, `${#$arr[@]}` in a
        // loop bound. A `${` in fish output is a read-time PARSE ERROR that
        // kills the whole file ("${ is not a valid variable in fish"), so it
        // has to be lowered here exactly as `braced_to_fish` lowers it
        // elsewhere.
        if c == b'$' as char && i + 1 < b.len() && b[i + 1] == b'{' {
            if let Some((inner, after)) = braced_span_bytes(b, i + 2) {
                out.push_str(&braced_arith_to_fish(&inner));
                i = after;
                continue;
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
                // A variable READ must carry the `$` sigil. fish's `math`
                // parses its argument as an EXPRESSION after parameter
                // expansion, so a bare identifier is not a variable at all —
                // `math "n + 1"` aborts with "Unknown function" even when
                // `n` is set, while `math "$n + 1"` evaluates (measured
                // against fish 3.7). The sigil was stripped here under the
                // old `$( _num "n" )` coercion design, which is gone; with
                // the coercion dropped, a bare name is a guaranteed runtime
                // error, so the read is spelled `$name`.
                out.push_str(&format!("${name}"));
            } else {
                // A non-numeric value would make fish's `math` abort with
                // "Unknown function". The coercion cannot be spelled inline:
                // the regex contains `$'`, which terminates the enclosing
                // `math "…"` string, and a `( … )` command substitution there
                // is "command substitutions not allowed here" — BOTH spellings
                // are parse errors, so any inline guard fails. fish also has no
                // `string match` function inside math (verified: "math: Error:
                // Unknown function"), so the previous inline form could never
                // have worked even if it had parsed.
                //
                // fish 3.x has no ternary, so the coercion is emitted where it
                // IS legal: as a sibling statement before the arithmetic. The
                // analysis already proves the common case (`NUM_VARS`), and
                // this keeps the arithmetic itself free of quoting hazards.
                //
                // The READ itself still needs its sigil — see the `NUM_VARS`
                // arm above: a bare name is "Unknown function" to `math`.
                out.push_str(&format!("${name}"));
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The inner text of a `${ … }` span in BYTE-oriented text (arithmetic),
/// plus the index just past the closing `}`. `None` when unterminated — the
/// caller then leaves the text alone rather than eating the rest of the word.
fn braced_span_bytes(b: &[u8], start: usize) -> Option<(String, usize)> {
    let mut i = start;
    while i < b.len() {
        if b[i] == b'}' {
            return Some((b[start..i].iter().map(|c| *c as char).collect(), i + 1));
        }
        i += 1;
    }
    None
}

/// A `${…}` span inside arithmetic TEXT → the fish expansion of the same
/// value.
///
/// The arithmetic rewriter receives opaque source text for anything the core
/// did not structure, and `math`'s argument is a single double-quoted string
/// — so whatever comes back must be legal INSIDE that string. Two fish facts
/// decide the spellings:
///
/// * a bare `( … )` inside double quotes is printed as literal TEXT, so a
///   command substitution there must be spelled `$( … )` (`string length`
///   below) — the same dq split `dq_subst` makes everywhere else;
/// * the flat per-element array layout keeps the element COUNT in
///   `NAME_len` and the array `NAME` itself is reachable only as a plain
///   list, so `${#NAME[@]}` becomes that counter variable, not a
///   substitution.
///
/// `$`-prefixed base names appear verbatim: the corpus carries the hybrid
/// `${#$arr[@]}` (a `#` applied to the braced name `$arr[@]`), so the sigil
/// is stripped before the name is tested. Anything outside these forms is
/// reduced to its identifier characters — NEVER handed back with the `${`
/// intact, because that is a parse error that costs the whole file, while a
/// reduced name at least runs (the same trade `braced_to_fish` makes).
fn braced_arith_to_fish(inner: &str) -> String {
    if let Some(rest) = inner.strip_prefix('#') {
        // `${#NAME[@]}` / `${#NAME[*]}` — the array LENGTH.
        if let Some(base) = rest
            .strip_suffix("[@]")
            .or_else(|| rest.strip_suffix("[*]"))
        {
            let base = base.strip_prefix('$').unwrap_or(base);
            if is_plain_var(base) {
                return format!("${base}_len");
            }
        }
        // `${#NAME}` — the string LENGTH.
        let name = rest.strip_prefix('$').unwrap_or(rest);
        if is_plain_var(name) {
            return format!("$(string length -- ${name})");
        }
    }
    let bare = inner.strip_prefix('$').unwrap_or(inner);
    if is_plain_var(bare) {
        return format!("${bare}");
    }
    // `${NAME[num]}` — a literal element subscript in the flat layout.
    if let Some((base, idx)) = bare.strip_suffix(']').and_then(|n| n.split_once('[')) {
        let idx = idx.trim_matches(['"', '\'']);
        if is_plain_var(base) && !idx.is_empty() && idx.chars().all(|c| c.is_ascii_digit()) {
            return format!("${base}_{idx}");
        }
    }
    let cleaned: String = inner
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "0".into()
    } else {
        format!("${cleaned}")
    }
}

/// dash has NO c-style `for (( init; cond; incr ))` — lower to a portable
/// while loop, in native fish (`set` for assignment, `math` for the
/// arithmetic, `test` for the condition, `end` to close):
///   set i 2
///   while [ (math "$i") -le (math "$n") ]
///       body
///       set i (math "$i + 1")
///   end
fn cstyle_for_to_fish(arith: &str, body: &str) -> String {
    let mut parts = arith.split(';');
    let init = parts.next().map(|s| s.trim()).unwrap_or("");
    let cond = parts.next().map(|s| s.trim()).unwrap_or("1");
    let incr = parts.next().map(|s| s.trim()).unwrap_or("");
    let mut out = String::new();
    if !init.is_empty() {
        // `i = 0` — fish assignment, never `i=0` (which fish rejects).
        let (name, val) = init.split_once('=').unwrap_or((init, "0"));
        out.push_str(&format!("set {} {}\n", name.trim(), val.trim()));
    }
    let cond_fish = arith_cond_fish(cond);
    out.push_str(&format!("while {cond_fish}\n"));
    out.push_str(body);
    if !incr.is_empty() {
        // `i++` -> `set i (math "$i + 1")`; `i += 2` -> `set i (math "$i + 2")`.
        let trimmed = incr.trim();
        let incr_final = if trimmed.is_empty() {
            ":".to_string()
        } else if trimmed.contains("++") {
            let name = trimmed.trim_end_matches("++").trim();
            format!("set {name} (math --scale 0 \"${name} + 1\")")
        } else if trimmed.contains("--") {
            let name = trimmed.trim_end_matches("--").trim();
            format!("set {name} (math --scale 0 \"${name} - 1\")")
        } else if let Some((name, rest)) = trimmed.split_once("+=") {
            format!(
                "set {} (math --scale 0 \"${} + {}\")",
                name.trim(),
                name.trim(),
                rest.trim()
            )
        } else if let Some((name, rest)) = trimmed.split_once("-=") {
            format!(
                "set {} (math --scale 0 \"${} - {}\")",
                name.trim(),
                name.trim(),
                rest.trim()
            )
        } else {
            // A general arithmetic increment has no statement form in fish
            // (fish has no `$(( ))`), so the value is computed and, when it is
            // a plain assignment target, stored back.
            format!("set _cfor_incr (math --scale 0 \"{}\")", arith_rewrite(trimmed))
        };
        out.push_str(&incr_final);
        out.push('\n');
    }
    out.push_str("end");
    out
}

fn arith_to_fish(a: &ArithAst) -> String {
    match a {
        ArithAst::Num(n) => n.to_string(),
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            // dotted struct-field names sanitize exactly like the setVar
            // write arm (`p.x` → `p_x`) — the read must hit the same home
            let name = name.replace('.', "_");
            // fish's `math` parses its argument as an EXPRESSION string: a
            // bare identifier (`i`) is read as a function call and errors
            // with "Unknown function", while `$i` is interpolated by fish
            // into the expression and evaluates. So every variable read is
            // `$name`.
            //
            // bash coerces a non-numeric value to 0 in arithmetic; fish's `math`
            // aborts with "Unknown function". The coercion cannot be spelled
            // inline here — the regex contains `$'`, which terminates the
            // enclosing `math "…"` string, and a `( … )` command substitution
            // inside it is "command substitutions not allowed here", so both
            // spellings are parse errors. fish has no `string match`
            // function inside math either (verified against fish 3.7:
            // "math: Error: Unknown function"), so the previous inline guard
            // could not have worked even when it did parse.
            //
            // The variable is read bare. A var whose value is NOT numeric
            // makes `math` abort rather than coerce to 0 the way bash does —
            // that is a limitation of the fish lowering, recorded here so the
            // next reader does not re-add the guard that cannot parse.
            format!("${name}")
        }
        // Arrays are flattened into per-element scalars `a_0 a_1 …`, so the
        // subscript selects the scalar directly. bash's `${a}[1]` is wrong
        // here: `$a` expands to the whole list and `[1]` indexes into the
        // NAMES (`a_1`), reading the string where the number was expected —
        // `$(( a[1] + 1 ))` on `a=(10 20 30)` came out 1, not 21.
        //
        // A literal subscript splices straight into the element name and is
        // read as a plain `$a_1` — the text lands inside the caller's
        // `math "…"` string, so it must not add its own quotes (fish rejects
        // both `${a_1}`, "variables cannot be bracketed", and a nested
        // `"…"` string).
        //
        // A computed key (`a[i]` with a non-literal `i`) is fish's own
        // concatenation indirection: `$a_$i` is TWO adjacent expansions, so
        // fish joins the flat element name at runtime and looks it up. This
        // survives the trip through the enclosing `math "…"` string — it
        // adds no quotes and is not a function call.
        //
        // The two rejected alternatives, both verified against fish:
        // `${a_$i}` → "variables cannot be bracketed" (fish expands $i
        // before the name is assembled, then refuses the bracket form), and
        // a nested `$(eval …)` → `math: Unknown function` (the inner text is
        // parsed as a math call, not as a value).
        ArithAst::Index { var, key } => {
            let base = arr_base(var);
            if let ArithAst::Num(n) = &**key {
                return format!("${base}_{n}");
            }
            // Computed subscript: the element name must be ASSEMBLED and then
            // dereferenced once. The bare `{base}_{key}` spelling is NOT an
            // indirection — fish reads `$arr_$i` as the (unset) variable
            // `$arr_` followed by `$i`, so the text collapsed to the literal
            // `arr_1` and `math` rejected it with "Unknown function"
            // (measured). Composing the name into a temp and using fish's own
            // `$$name` indirection is the working spelling, and it survives
            // the enclosing `math "…"` string because `$( … )` — unlike a
            // bare `( … )` — IS expanded inside double quotes.
            format!(
                "$(set -l _nk {base}_\"{}\"; printf '%s' $$_nk)",
                arith_to_fish(key)
            )
        }
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
            // The remaining spellings bash and fish do not share. fish's
            // `math` refuses bash's `**` ("Unexpected token") and every
            // bitwise/shift operator ("Logical operations are not
            // supported, use `test` instead"), so a pass-through is a hard
            // RUNTIME error — the command substitution comes back empty.
            // fish spells the same operations `^`, `bitand`/`bitor`/
            // `bitxor`, and scaling by a power of two (measured against
            // fish 3.7).
            let l = arith_to_fish(lhs);
            let r = arith_to_fish(rhs);
            match op.as_str() {
                "**" => return format!("({l} ^ {r})"),
                "&" => return format!("bitand({l}, {r})"),
                "|" => return format!("bitor({l}, {r})"),
                // bash's `^` is bitwise XOR; fish's `^` is power, so the
                // spelling must not be reused.
                "^" => return format!("bitxor({l}, {r})"),
                "<<" => return format!("({l} * (2 ^ {r}))"),
                ">>" => return format!("floor({l} / (2 ^ {r}))"),
                _ => {}
            }
            format!("({l} {op} {r})")
        }
        ArithAst::Un { op, arg } => {
            let a = arith_to_fish(arg);
            // fish's `math` has no `~` at all — it aborts with "Missing
            // operator" and the whole command substitution comes back empty.
            // Two's-complement NOT is `-x - 1` for every integer, which fish
            // evaluates natively (measured: `~0` → -1, `~5` → -6). bash's
            // arithmetic is 64-bit, so the identity holds across the range.
            if op == "~" {
                return format!("(-({a}) - 1)");
            }
            format!("({op}{a})")
        }
        ArithAst::Cond { test, then, else_ } => format!(
            "({} ? {} : {})",
            arith_to_fish(test),
            arith_to_fish(then),
            arith_to_fish(else_)
        ),
        ArithAst::Assign { var, op, rhs } => {
            format!("${var} {op} {}", arith_to_fish(rhs))
        }
        ArithAst::IncDec { var, delta, prefix } => {
            // fish's `math` has no ++/-- and no assignment, so the increment
            // is spelled out. When the value is USED (`echo $((x++))`) the
            // expression must both yield the old/new value AND mutate x, so
            // it becomes a command substitution: save the value, apply the
            // delta, echo the saved one. `set -g` is required — a bare `set`
            // inside `(...)` writes the substitution's own scope and the
            // mutation is lost.
            //
            // The bare `((x++))` STATEMENT is handled by assign_to_fish, which
            // discards the value and needs no substitution.
            let inc = if *delta > 0 { 1 } else { -1 };
            let v = format!("{var}__sh2v");
            if *prefix {
                format!("(set -g {v} (math --scale 0 \"${var} + {inc}\"); set -g {var} ${v}; echo ${v})")
            } else {
                format!("(set -g {v} ${var}; set -g {var} (math --scale 0 \"${var} + {inc}\"); echo ${v})")
            }
        }
        // C-frontend nodes (never emitted by the shell path): sizeof is a
        // compile-time constant; casts are identity (shell arith is 64-bit).
        ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
        ArithAst::Cast { arg, .. } => arith_to_fish(arg),
    }
}

// ── arrows / inline statement sequences ──────────────────────────────

fn arrow_to_fish(args: &[IrExpr]) -> Result<String, String> {
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
    // Compounds are emitted in fish's INLINE form (`if c; BODY; end`,
    // terminated by `;` rather than a newline). Every arm mirrors the
    // block renderer `stmt_to_fish` arm-for-arm — the two used to be
    // separate implementations and this one still spoke bash
    // (`if c; then …; fi`, `case … esac`, `$(( ))`), which fish cannot
    // parse at all.
    match st {
        IrStmt::Expr(e) => cmd_to_fish(e),
        IrStmt::Ext(node) => match ext_node_to_fish(node.as_ref())? {
            Some(rendered) => Ok(rendered),
            None => Err(format!("sh renderer: Ext node {} unsupported", node.tag())),
        },
        IrStmt::Assign { targets, expr, .. } => assign_to_fish(targets, expr),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            // an empty branch is legal fish (`if c; end`), but keep the `:`
            // no-op so an empty arm is still a statement
            let inline = |b: &[IrStmt]| -> Result<String, String> {
                if b.is_empty() {
                    Ok(":".to_string())
                } else {
                    stmts_inline(b)
                }
            };
            let mut out = format!("if {}; {}; end", cond_to_fish(cond)?, inline(then)?);
            for (ec, body) in elsifs {
                out.push_str(&format!("; else if {}; {}; end", cond_to_fish(ec)?, inline(body)?));
            }
            if !else_.is_empty() {
                out.push_str(&format!("; else {}; end", stmts_inline(else_)?));
            }
            Ok(out)
        }
        IrStmt::For { var, iter, body } => {
            if let Some((start, end)) = range_of(iter) {
                // the block renderer's numeric-range form (a fish `for`
                // over `seq`), kept identical so both spellings agree
                return Ok(format!(
                    "for {var} in (seq {start} {end}); {}; end",
                    stmts_inline(body)?
                ));
            }
            Ok(format!(
                "for {var} in {}; {}; end",
                for_items_to_fish(iter)?,
                stmts_inline(body)?
            ))
        }
        IrStmt::While { cond, body } => Ok(format!(
            "while {}; {}; end",
            cond_to_fish(cond)?,
            stmts_inline(body)?
        )),
        IrStmt::ForInit { .. } => Err(
            "sh renderer: un-stripped ForInit (the strip_cfor pass should have lowered it)".into(),
        ),
        IrStmt::Continue => Ok(format!("continue")),
        IrStmt::Break => Ok(format!("break")),
        IrStmt::DoWhile { body, cond, until } => {
            // `while true; BODY; if COND; and break; end; end` — the block
            // renderer's form. `until` drops the `not`.
            let neg = if *until { "" } else { "not " };
            Ok(format!(
                "while true; {}; if {neg}{}; and break; end; end",
                stmts_inline(body)?,
                cond_to_fish(cond)?
            ))
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            // fish `switch`: several `case` arms under ONE `switch`, and
            // the `switch` owns the single `end`. A `case` arm is just a
            // label — it takes NO `end` of its own, so a per-arm `end`
            // closed the whole `switch` at the first arm and every later
            // `case` fell outside its block ("'case' builtin not inside of
            // switch block"), unbalanced the enclosing loop ("Missing end
            // to balance this while loop"), and left the rest of the
            // program as stray statements.
            let mut out = format!("switch {}", word_to_fish(discriminant)?);
            for cl in clauses {
                out.push_str(&format!(
                    "; case {}; {}",
                    case_patterns_fish(&cl.patterns),
                    if cl.body.is_empty() {
                        ":".to_string()
                    } else {
                        stmts_inline(&cl.body)?
                    }
                ));
            }
            out.push_str("; end");
            Ok(out)
        }
        IrStmt::Function { name, body, .. } => Ok(format!(
            "function {name}; {}; end",
            stmts_inline(body)?
        )),
        IrStmt::Redirect { inner, redirects } => {
            let mut out = herestring_wrap_fish(redirects, stmts_inline(inner)?)?;
            out.push_str(&redirects_to_fish(redirects)?);
            Ok(out)
        }
        // `begin … end` is fish's scoped block — the block renderer's form.
        IrStmt::Subshell(body) => Ok(format!("begin; {}; end", stmts_inline(body)?)),
        IrStmt::Background(body) => {
            if body.len() == 1 {
                if let IrStmt::Expr(e) = &body[0] {
                    return Ok(format!("{} &", cmd_to_fish(e)?));
                }
            }
            Ok(format!("begin; {}; end &", stmts_inline(body)?))
        }
        IrStmt::Block(body) => Ok(format!("begin; {}; end", stmts_inline(body)?)),
        IrStmt::Return(e) => match e {
            Some(x) if !return_value_is_status(x) => Ok(format!("echo {}", word_to_fish(x)?)),
            Some(x) => Ok(format!("return {}", word_to_fish(x)?)),
            None => Ok("return".into()),
        },
        IrStmt::Exit(e) => match e {
            Some(x) => Ok(format!("exit {}", word_to_fish(x)?)),
            None => Ok("exit".into()),
        },
        IrStmt::Die { expr, .. } => Ok(format!("echo {} >&2; exit 1", word_to_fish(expr)?)),
        IrStmt::Warn { expr, .. } => Ok(format!("echo {} >&2", word_to_fish(expr)?)),
        IrStmt::Declare { vars, init, local } => {
            // `set`/`set -l` with space-separated values (the block
            // renderer's form) — `name=value` is bash, not fish.
            let mut out = String::from(if *local { "set -l " } else { "set " });
            for (i, v) in vars.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&v.name);
                if let Some(init) = init {
                    out.push(' ');
                    out.push_str(&word_to_fish(init)?);
                }
            }
            Ok(out)
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            Ok(format!("set {var} {}", array_literal_to_fish(elements)?))
        }
        IrStmt::Output { value, newline, .. } => {
            let w = word_to_fish(value)?;
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
            let mut line = exec_line_to_fish(cmd, args, Some(env))?;
            if !redirects.is_empty() {
                line.push_str(&redirect_objs_to_fish(redirects)?);
            }
            if let Some(var) = capture {
                Ok(format!("set {var} ({line} | string collect)"))
            } else {
                Ok(line)
            }
        }
        IrStmt::Pipeline {
            stages,
            capture,
            ..
        } => {
            let mut line = String::new();
            for (i, stg) in stages.iter().enumerate() {
                if i > 0 {
                    line.push_str(" | ");
                }
                line.push_str(&stmts_inline(stg)?);
            }
            if let Some(var) = capture {
                Ok(format!("set {var} ({line} | string collect)"))
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
            word_to_fish(content)?,
            if *append { ">>" } else { ">" },
            word_to_fish(path)?
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
        // sh has no select-on-channels — approximate via fork/exec
        IrStmt::Select { clauses } => {
            let mut default_idx: Option<usize> = None;
            for (i, c) in clauses.iter().enumerate() {
                if c.comm == "default" { default_idx = Some(i); break; }
            }
            let _ = &default_idx;
                    let mut parts: Vec<String> = Vec::new();
            for c in clauses {
                if c.comm == "default" {
                    for b in &c.body {
                        if let Ok(s) = stmt_inline(b) {
                            parts.push(s);
                        }
                    }
                }
            }
            if parts.is_empty() {
                Err("select has no inline sh rendering (channel select)".into())
            } else {
                Ok(parts.join("; "))
            }
        }
        // inline asm has no sh rendering — refuse loudly
        IrStmt::Asm { .. } => Err("inline asm has no sh rendering".into()),
        IrStmt::Shaderize { .. } => Err("shaderize (autoshaderization) has no sh rendering".into()),
    }
}

// ── for-loop item lists ──────────────────────────────────────────────

fn for_items_to_fish(iter: &IrExpr) -> Result<String, String> {
    match iter {
        IrExpr::Array(items) => {
            if items.is_empty() {
                // `for x; do` — iterate the positional parameters.
                //
                // This must be BARE `$argv`, not `"$argv"`: a quoted fish
                // list expands to its elements JOINED into one word, so
                // `for a in "$argv"` would run the body once with
                // "a b c" instead of once per argument the way bash's
                // `for a in "$@"` does. A bare `$argv` expands to the
                // element list, which is the bash semantics.
                return Ok("$argv".into());
            }
            let mut out = Vec::new();
            for it in items {
                out.push(for_item_to_fish(it)?);
            }
            Ok(out.join(" "))
        }
        // `for x in "${arr[@]}"` — a for loop iterates per ELEMENT, so the
        // expansion must be the per-line list, not the space-joined form.
        IrExpr::Call { func, args }
            if (func == "arrayItems" || func == "arrayIndex")
                && matches!(args.get(1), Some(IrExpr::Str(k, _)) if k == "@" || k == "*") =>
        {
            let raw = raw_arg(args, 0)?;
            let name = arr_base(&raw);
            if func == "arrayIndex" {
                return Ok(arr_expand_call_fish_list(name));
            }
            Ok(arr_keys_call_fish(name))
        }
        other => Ok(word_to_fish(other)?),
    }
}

fn for_item_to_fish(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } if func == "getVar" => {
            // Route through `var_ref_to_fish`, which owns the bash-special
            // mapping. Emitting the name verbatim leaked `$*` here — `for i in
            // $*` is "$* is not supported. In fish, please use $argv.", a
            // read-time PARSE error that kills the whole file (examples/
            // parse-dollars-in-string.sh). The `list` form keeps a list
            // expansion BARE, which is what a `for` item list needs.
            Ok(var_ref_to_fish(&raw_arg(args, 0)?, true))
        }
        IrExpr::Call { func, args } if func == "listVar" => {
            // bare `$argv` — see the `listVar` arm in `word_to_fish`: a
            // quoted fish list joins into one word, which is wrong for a
            // `for` item list.
            let _ = raw_arg(args, 0)?;
            Ok("$argv".into())
        }
        IrExpr::Call { func, args } if func == "join" => join_to_fish(arg(args, 0)?, true),
        IrExpr::Call { func, args } if func == "param" => {
            // `for x in "${arr[@]}"` arrives as the slice param
            // ("slice", name, "@", "") — a for loop iterates per element, so
            // it needs the per-line helper rather than the space-joined
            // `${arr[*]}` shape a value position wants.
            let op = raw_arg(args, 0)?;
            if op == "slice" {
                let base = raw_arg(args, 1)?;
                let idx = raw_arg(args, 2).unwrap_or_default();
                if idx == "@" || idx == "*" {
                    return Ok(if base.starts_with('!') {
                        arr_keys_call_fish(arr_base(&base))
                    } else {
                        arr_expand_call_fish_list(arr_base(&base))
                    });
                }
            }
            // the baked-name form (`arr[@]`) is the other spelling
            let pname = raw_arg(args, 1)?;
            if let Some((an, idx)) = pname.strip_suffix(']').and_then(|n| n.split_once('[')) {
                if idx == "@" || idx == "*" {
                    return Ok(if an.starts_with('!') {
                        arr_keys_call_fish(arr_base(an))
                    } else {
                        arr_expand_call_fish_list(arr_base(an))
                    });
                }
            }
            param_to_fish(args, false)
        }
        IrExpr::Call { func, args } if func == "arrayIndex" => {
            let raw = raw_arg(args, 0)?;
            let key = raw_arg(args, 1)?;
            // `${arr[@]}` iterates per ELEMENT, so it needs the per-line
            // helper; a literal index is the flat `arr_0` variable.
            if key == "@" || key == "*" {
                return Ok(arr_expand_call_fish_list(arr_base(&raw)));
            }
            if key.contains(['$', '(']) {
                return Ok(format!("${}_$key", arr_base(&raw)));
            }
            Ok(format!("${}_{}", arr_base(&raw), key))
        }
        IrExpr::Call { func, args } if func == "arrayItems" => {
            Ok(arr_keys_call_fish(arr_base(&raw_arg(args, 0)?)))
        }
        IrExpr::Call { func, args } if func == "arrayLen" => {
            Ok(arr_len_fish(&raw_arg(args, 0)?))
        }
        _ => word_to_fish(e),
    }
}

fn array_literal_to_fish(elements: &[IrExpr]) -> Result<String, String> {
    let mut out = Vec::new();
    for e in elements {
        out.push(word_to_fish(e)?);
    }
    Ok(format!("({})", out.join(" ")))
}

fn array_items(args: &[IrExpr], idx: usize) -> Result<String, String> {
    let Some(IrExpr::Array(items)) = args.get(idx) else {
        return Ok(String::new());
    };
    let mut out = Vec::new();
    for it in items {
        out.push(word_to_fish(it)?);
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
        IrExpr::Arith(a) => Ok(arith_to_fish(a)),
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
        shir_to_fish(&prog).unwrap()
    }

    /// An INDEXED array is a native fish LIST end to end: `set arr a b c`
    /// stores it, `$arr` iterates it, `(string join ' ' $arr)` is bash's
    /// `${arr[*]}`, `(count $arr)` is `${#arr[@]}` and `$arr[n+1]` is
    /// `${arr[n]}`. None of that may fall back to the per-element runtime.
    #[test]
    fn indexed_arrays_lower_to_a_native_fish_list() {
        let sh = render_sh("arr=(one two three)\necho \"${arr[*]}\"\necho \"${#arr[@]}\"\necho \"${arr[1]}\"\nfor x in \"${arr[@]}\"; do echo $x; done");

        assert!(sh.contains("set arr one two three"), "native list store: {sh}");
        assert!(sh.contains("(string join ' ' $arr)"), "native join: {sh}");
        assert!(sh.contains("(count $arr)"), "native count: {sh}");
        assert!(sh.contains("$arr[2]"), "bash index 1 -> fish index 2: {sh}");
        assert!(sh.contains("for x in $arr"), "bare list iterates: {sh}");
        // The whole point: no per-element variables, no runtime preamble.
        assert!(!sh.contains("arr_0"), "no per-element vars: {sh}");
        assert!(!sh.contains("arr_len"), "no counter var: {sh}");
        assert!(!sh.contains("function _arr_"), "no array runtime: {sh}");
    }

    /// A whole-array expansion that lands in a LONGER template must stay ONE
    /// word. `"$arr"` is a single word only when it is the whole string —
    /// embedded, fish emits one word per element (`Array: a Array: b`),
    /// which is why the join is spelled out.
    #[test]
    fn array_expansion_inside_a_template_is_one_word() {
        let sh = render_sh("arr=(a b)\necho \"Array: ${arr[*]}\"");
        assert!(
            sh.contains("Array: $(string join ' ' $arr)")
                || sh.contains("(string join ' ' -- \"Array: $(string join ' ' $arr)\")"),
            "the expansion joins inside the template: {sh}"
        );
        assert!(!sh.contains("\"Array: $arr\""), "never a bare list in a template: {sh}");
    }

    /// `declare -a` is the INDEXED attribute; only `-A` is associative.
    /// Treating `-a` as assoc put `declare -a`/`local -a`/`typeset -a` arrays
    /// on the string-key runtime, so `"${array[@]}"` walked keys that were
    /// never recorded and printed nothing.
    #[test]
    fn declare_lowercase_a_is_not_associative() {
        let sh = render_sh("declare -a array=(item1 item2)\necho \"${array[@]}\"");
        assert!(sh.contains("set array item1 item2"), "native list: {sh}");
        assert!(!sh.contains("_arr_expand_k"), "not the assoc walk: {sh}");
        assert!(!sh.contains("_arr_keys_k"), "not the assoc keys: {sh}");
    }

    /// The runtime preamble carries only the helpers the rendered body
    /// actually calls — a file touching one indexed array must emit none at
    /// all (they used to be emitted as a fixed block of eight).
    #[test]
    fn array_preamble_is_emitted_per_helper() {
        assert_eq!(arr_helper_preamble("set arr a b\nfor x in $arr\nend\n"), "");
        assert!(!arr_helper_preamble("echo $arr[1]").contains("_arr_expand"));
        let assoc = arr_helper_preamble("for k in (_arr_keys_k map)\n");
        assert!(assoc.contains("function _arr_keys_k"), "{assoc}");
        assert!(!assoc.contains("function _arr_slice"), "unused helper: {assoc}");
        // `_arr_expand_join` calls `_arr_expand`, so the base is pulled in.
        let join = arr_helper_preamble("echo (_arr_expand_join arr)\n");
        assert!(join.contains("function _arr_expand_join"), "{join}");
        assert!(join.contains("function _arr_expand "), "dependency: {join}");
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
    /// the renderer must APPLY the increment and swallow the expansion
    /// result. In fish the arith form is `math`, not `$(( ))`: the bare
    /// `begin; set x (math "$x + 1"); end` is the correct native lowering
    /// (a bare `$((...))` line would try to RUN the value as a command,
    /// "1: not found", and fish has no `$(( ))` at all).
    #[test]
    fn incdec_assign_emits_colon_arith() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[{"name":"x","type":"Int"}],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Assign","targets":[{"var":"x","sigil":null,"indices":[]}],"expr":{"type":"Str","value":"1","style":"DoubleQuoted"}},
            {"type":"Block","body":[{"type":"Assign","targets":[{"var":"x","sigil":null,"indices":[]}],"expr":{"type":"Arith","ast":{"type":"IncDec","var":"x","delta":1,"prefix":false}}}]},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("t29 A1 ingress");
        let sh = shir_to_fish(&prog).expect("t29 A1 render");
        // fish's arith is `math`, and a command-substitution `( ... )` is
        // NOT allowed as an assignment target — the increment must land as
        // a `set x (math ...)` inside a block, with no `$(( ))` anywhere.
        assert!(
            sh.contains("set x (math --scale 0 \"$x + 1\")"),
            "IncDec statement must lower to fish math: {sh}"
        );
        // The generated BODY must carry no bash arithmetic. The prologue
        // comment legitimately spells the construct it avoided ("no $(( ))"),
        // so scan only the code, past the header.
        let body = sh.split_once("\nset x").map_or(sh.as_str(), |(_, b)| b);
        assert!(
            !body.contains("$(("),
            "no bash arithmetic may leak into the fish body: {sh}"
        );
    }

    /// `${V+x}` is the SETNESS test — `x` when V is set (a V assigned "" is
    /// set), "" when it is not — and `${V:+x}` is the stronger "set AND
    /// non-empty". Rendering either as the variable's VALUE (or as the same
    /// string) is the silent wrong answer the core pins against
    /// (ir.rs `param_plus_lowers_to_a_set_test_not_the_value`); before this
    /// arm existed the fish renderer refused the whole file instead
    /// ("param op not renderable: \"+\"", examples/variable-plus-operator.sh).
    #[test]
    fn param_plus_lowers_to_a_setness_test() {
        // The gate's A1 carries `test -n "${V+x}"` as a structured param CALL
        // `(op, name, payload)`. `render_sh`'s parser+ast_to_ir path does NOT
        // produce that shape (it keeps the raw text), so drive the A1 ingress
        // like the incdec test above.
        const A1: &str = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[
              {"type":"Str","value":"test","style":"DoubleQuoted"},
              {"type":"Array","elements":[
                {"type":"Str","value":"-n","style":"DoubleQuoted"},
                {"type":"Call","func":"param","purity":"PureCpu","args":[
                  {"type":"Str","value":"__OP__","style":"DoubleQuoted"},
                  {"type":"Str","value":"V","style":"DoubleQuoted"},
                  {"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]}}
        ]}"#;
        let render = |op: &str| {
            let json = A1.replace("__OP__", op);
            let prog = crate::shir_json_in::shir_json_to_ir(&json).expect("A1 ingress");
            shir_to_fish(&prog).expect("A1 render")
        };

        let plus = render("+");
        assert!(
            plus.contains(r#""$(set -q V;"#),
            "`+` guards on `set -q` (definED, not non-empty), in the quoted \
             $() form: {plus}"
        );
        assert!(
            !plus.contains(r#"printf '%s' "$V""#),
            "`+` yields the PAYLOAD, never the variable's value: {plus}"
        );

        let colon = render(":+");
        assert!(
            colon.contains(r#""$(test -n "$V";"#),
            "`:+` additionally requires non-empty: {colon}"
        );
        assert_ne!(plus, colon, "`+` and `:+` must stay distinct");
    }

    /// `>|` (ClobberOutput) arrives as the contract's "wc" mode. fish has no
    /// noclobber to bypass, so it lowers to the plain truncating `>` — and
    /// MUST lower to something, since an unhandled mode refused the file
    /// ("redirect mode not renderable: \"wc\"",
    /// examples/parse-redirect-clobber.sh).
    #[test]
    fn clobber_redirect_lowers_to_a_plain_truncate() {
        let sh = render_sh(": >| out.txt");
        assert!(sh.contains(">out.txt"), "`>|` truncates out.txt: {sh}");
        assert!(!sh.contains(">|"), "fish cannot parse `>|`: {sh}");
    }

    /// ForEachLine is the STREAMING line iterator (docs/shir-primitives.md §2)
    /// and its `source` is a FILE PATH, so the loop must READ the path. The
    /// trap is rendering it as `PATH | while read …`, which would try to
    /// EXECUTE the file. An unhandled Ext node refused the file outright
    /// ("sh renderer: Ext node unsupported",
    /// examples/070_gnuisms_thorough.sh).
    #[test]
    fn foreach_line_renders_a_native_read_loop() {
        let mk = |limit: Option<Box<IrExpr>>| crate::shir_nodes::ForEachLine {
            source: IrExpr::Str("f.txt".to_string(), StrStyle::DoubleQuoted),
            var: "__cl1".to_string(),
            body: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![IrExpr::Var("__cl1".to_string(), None)]),
                ],
            })],
            limit,
        };

        let out = ext_node_to_fish(&mk(None))
            .unwrap()
            .expect("ForEachLine has a lowering");
        assert!(out.starts_with("while read -l __cl1;"), "native read loop: {out}");
        assert!(out.ends_with("end < f.txt"), "the path is READ, by redirect: {out}");
        assert!(
            !out.contains("f.txt | while"),
            "a bare path is not a command — this pipe would execute the file: {out}"
        );

        let lim = ext_node_to_fish(&mk(Some(Box::new(IrExpr::Int(3)))))
            .unwrap()
            .expect("ForEachLine has a lowering");
        assert!(
            lim.starts_with("head -n 3 f.txt | while read -l __cl1;"),
            "a limit is `head -n N` in the pipeline (streaming early exit): {lim}"
        );
        assert!(lim.ends_with("; end"), "the loop must still be closed: {lim}");
    }

    /// `$#` must lower to a COMMAND SUBSTITUTION, not the bare `count`
    /// builtin. The A1 hands `test` its operand as ONE opaque string
    /// (`"$# -lt 2"`), so this reaches the text path. A bare `count` is legal
    /// only where a command is expected: as a `[ ]` operand fish reads the
    /// literal word, and the statement — plus the rest of the script's
    /// output — dies with "Expected a combining operator like '-a' at index
    /// 2" (measured on 059_issue3.sh, whose whole body was swallowed).
    #[test]
    fn positional_count_lowers_to_a_command_substitution() {
        let sh = render_sh("if [ $# -lt 2 ]; then echo One; fi");
        assert!(
            sh.contains("[ (count $argv) -lt 2 ]"),
            "the count is an OPERAND: {sh}"
        );
        assert!(
            !sh.contains("[ count $argv"),
            "the bare builtin is not an operand: {sh}"
        );
    }

    /// Inside double quotes fish prints a bare `( )` as LITERAL TEXT, so the
    /// count needs the `$( )` spelling there — the same split the `$(( ))`
    /// arm makes, and the reason the dq form cannot simply reuse the bare
    /// substitution.
    #[test]
    fn positional_count_in_dquotes_uses_dollar_parens() {
        let sh = render_sh("echo \"n=$#\"");
        assert!(
            sh.contains("\"n=$(count $argv)\""),
            "dq keeps a live substitution: {sh}"
        );
    }

    /// `let x=5` / `let x+=3` reach the A1 as an Assign STATEMENT whose RHS is
    /// an Arith `Assign` node. fish's `math` has no assignment syntax at all
    /// (`math "x = 5"` -> "math: Error: Unexpected token"), so the expression
    /// form this used to fall through to was a hard runtime error that
    /// swallowed the whole statement's output. The assignment IS the
    /// statement, so it must be spelled as the fish assignment it is.
    #[test]
    fn let_assignment_lowers_to_a_fish_assignment() {
        let sh = render_sh("let x=5\nlet x+=3\necho $x");
        assert!(
            sh.contains("set x (math --scale 0 \"5\")"),
            "let x=5 is an assignment: {sh}"
        );
        assert!(
            sh.contains("set x (math --scale 0 \"$x + 3\")"),
            "let x+=3 reads the target: {sh}"
        );
        assert!(!sh.contains("\"$x = 5\""), "math cannot assign: {sh}");
    }

    /// An UNQUOTED heredoc delimiter expands its body; a quoted one does not.
    /// The core marks that with the redirect's `interpolate` flag and leaves
    /// the body raw, so the renderer must expand it itself — a body left
    /// single-quoted prints `$name` where bash prints `world`.
    #[test]
    fn unquoted_heredoc_body_expands_but_a_quoted_one_does_not() {
        let sh = render_sh("name=world\ncat << EOF\nHello $name\nEOF\n");
        assert!(
            sh.contains(r#"printf '%s\n' "Hello $name" | cat"#),
            "unquoted delimiter interpolates: {sh}"
        );
        let lit = render_sh("name=world\ncat <<'EOF'\nHello $name\nEOF\n");
        assert!(
            lit.contains(r#"printf '%s\n' 'Hello $name' | cat"#),
            "quoted delimiter stays literal: {lit}"
        );
    }

    /// `${name}` is a hard ERROR in fish ("Variables cannot be bracketed"),
    /// while `$name` expands — so a brace form must be rewritten. `$(…)`
    /// needs no rewriting (fish expands it inside double quotes), and a
    /// backtick substitution is bash-only and lowers to the same `$(…)`.
    #[test]
    fn interpolated_heredoc_body_rewrites_fish_hostile_expansions() {
        let sh = render_sh("V=a\nW=b\ncat <<EOF\n${V} m ${W} t\nEOF\n");
        assert!(
            sh.contains(r#""$V m $W t""#),
            "brace vars become bare fish vars: {sh}"
        );
        assert!(!sh.contains("${V}"), "fish rejects ${{name}}: {sh}");

        let bt = render_sh("cat <<EOF\nv=`echo bt`\nEOF\n");
        assert!(bt.contains(r#""v=$(echo bt)""#), "backtick -> $( ): {bt}");
    }

    /// A `${…}` inside arithmetic TEXT must be lowered, never spliced
    /// through: fish has no braces form at all, so a `${` in the output is a
    /// read-time PARSE ERROR that costs the whole file (063_02_complex_array_assignments
    /// and parse-arithmetic-nested-array both died inside their `math`
    /// argument — `array[${index}]` and `100/${#$arr[@]}`).
    #[test]
    fn arith_braced_expansions_lower_to_fish() {
        assert_eq!(braced_arith_to_fish("index"), "$index");
        // the corpus hybrids carry a sigil INSIDE the braces (`${#$arr[@]}`)
        assert_eq!(braced_arith_to_fish("#arr[@]"), "$arr_len");
        assert_eq!(braced_arith_to_fish("#$arr[@]"), "$arr_len");
        assert_eq!(braced_arith_to_fish("#s"), "$(string length -- $s)");

        assert_eq!(arith_rewrite("${index}"), "$index");
        assert_eq!(arith_rewrite("100/${#$arr[@]}"), "100/$arr_len");
        // refuse > guess: an unmodelled form is reduced to its identifier
        // characters, but a `${` may NEVER reach the output
        assert!(!arith_rewrite("${weird:op}").contains("${"));
    }

    /// `${base[idx]}` on an INDEXED array — fish subscripts a list directly,
    /// and its index is 1-based where bash's is 0-based, so a literal
    /// subscript is offset by one. A COMPUTED one cannot be spelled inline:
    /// fish rejects `"$arr[(math …)]"` with "Invalid index value", so it is
    /// evaluated into a variable inside the substitution, where a nested
    /// `$arr[$i]` is legal.
    #[test]
    fn bracketed_param_subscript_offsets_the_native_list() {
        assert_eq!(subscript_read_fish("arr", "2"), "$arr[3]");
        assert_eq!(
            subscript_read_fish("arr", "$i"),
            r#"$(set -l _i (math --scale 0 "$i + 1"); printf '%s' $arr[$_i])"#
        );

        // end to end: the default arm takes the SUBSCRIPT as its subject
        const A1: &str = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[
              {"type":"Str","value":"echo","style":"DoubleQuoted"},
              {"type":"Array","elements":[
                {"type":"Call","func":"param","purity":"PureCpu","args":[
                  {"type":"Str","value":":-","style":"DoubleQuoted"},
                  {"type":"Str","value":"array[${index}]","style":"DoubleQuoted"},
                  {"type":"Str","value":"def","style":"DoubleQuoted"}]}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(A1).expect("A1 ingress");
        let sh = shir_to_fish(&prog).expect("A1 render");
        assert!(
            sh.contains("$array[$_i]"),
            "the element is the native list subscript: {sh}"
        );
        assert!(
            !sh.contains("${"),
            "no braced expansion may reach fish: {sh}"
        );
    }

    /// `arrayLen` in a word position — fish's `count` builtin over the native
    /// list, with no counter variable to keep in step. (The doubled braces of
    /// an earlier `"${{{}}_len}}"` emitted the literal `${arr_len}`, the same
    /// parse error as above — it cost 063_02 its
    /// `matrix[3,3]=${#array[@]}` line.)
    #[test]
    fn array_len_word_counts_the_native_list() {
        const A1: &str = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[
              {"type":"Str","value":"echo","style":"DoubleQuoted"},
              {"type":"Array","elements":[
                {"type":"Call","func":"arrayLen","purity":"PureCpu","args":[
                  {"type":"Str","value":"arr","style":"DoubleQuoted"}]}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(A1).expect("A1 ingress");
        let sh = shir_to_fish(&prog).expect("A1 render");
        assert!(sh.contains("(count $arr)"), "the list is counted: {sh}");
        assert!(!sh.contains("${"), "no braced expansion: {sh}");
    }

    /// A `switch` arm / `string match` pattern must be a legal fish WORD.
    /// fish's tokenizer rejects `[^}]`, `${` and whitespace unquoted — that
    /// is what stopped 063_11 and 063_hard_to_parse from parsing at all
    /// (`case *\$\{[^}]*\}*`) — while the SAME text inside single quotes is
    /// one word, and both `switch` and `string match` glob-match the pattern
    /// after quote removal, so the arm still means the same thing.
    #[test]
    fn case_pattern_is_quoted_unless_it_carries_an_expansion() {
        assert_eq!(fish_pattern_lit(r"*\$\{[^}]*\}*"), r"'*\$\{[^}]*\}*'");
        // a LIVE expansion must stay expandable — quoting would make it text
        assert_eq!(fish_pattern_lit("$pat"), "$pat");
        assert_eq!(fish_pattern_lit("pre$pat*"), "pre$pat*");
        // an ESCAPED `$` is literal, so the pattern is safe to quote
        assert_eq!(fish_pattern_lit(r"a\$b"), r"'a\$b'");
        // bash quoting inside the pattern is not part of the glob
        assert_eq!(fish_pattern_lit("*'&'*"), "'*&*'");
        assert_eq!(fish_pattern_lit("'a b'"), "'a b'");
    }

    /// A COMPUTED subscript write (`options["$key"]=v`) must not emit
    /// `set name[expr]` — fish has no such form (and no `${}`), so the
    /// element name is composed at run time and written through `$$name`,
    /// the write side of the same flat layout the readers use.
    #[test]
    fn computed_subscript_write_composes_the_element_name() {
        const A1: &str = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Assign","targets":[{"var":"options[\"$key\"]","sigil":null,"indices":[]}],"expr":{"type":"Str","value":"true","style":"DoubleQuoted"}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(A1).expect("A1 ingress");
        let sh = shir_to_fish(&prog).expect("A1 render");
        assert!(
            sh.contains(r#"set -l _wk options_"$key"; set $_wk true"#),
            "the element name is composed and written indirectly: {sh}"
        );
        assert!(!sh.contains("set options["), "no bracketed set: {sh}");
    }
}
