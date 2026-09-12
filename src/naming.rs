//! Shared meaningful temp-name heuristic for native function call results.
//!
//! The core logic (fn-part abbreviation, arg-part tags, structural arith
//! tags) is a pure function of the shared IR, so every backend can use it.
//! Each backend prepends its own language-specific prefix and handles
//! uniqueness (collision fallback) in its own namespace. See
//! docs/MEANINGFUL_NAMES.md for the design.

use crate::ir::{ArithAst, IrExpr};

/// A distinctive, unique, concise, user-meaningful base name for a native
/// function call result: `_<fn-part>_<arg-part>_...` WITHOUT the
/// language-specific prefix or uniqueness suffix. Each backend prepends
/// its own prefix (e.g. C uses `_`, Rust uses `__`) and appends a
/// sequence on collision.
///
/// - fn-part: multi-word names abbreviate to first letters (`all_factors`
///   -> `af`); short single words stay whole (`greet` -> `greet`).
/// - arg-part: literals keep their value (`10` -> `10`, `2147483645`
///   verbatim); variables keep their NAME (stable across value changes);
///   a binary arith shows its shape (`67108847 * 67108837` ->
///   `67_x_67_`); anything else keeps its first literal or a short hash.
pub fn temp_base_name(fname: &str, args: &[IrExpr]) -> String {
    let fn_part = abbrev_fn_name(fname);
    let mut parts = vec![fn_part];
    for a in args {
        parts.push(abbrev_arg_name(a));
    }
    parts.join("_")
}

/// A meaningful name for a positional-param temp: the positional index
/// IS the signal (`p1` = param 1), so the name is kept as-is. Backends
/// add their own prefix and a kind suffix (`_p1` in C, `__p1n` in JS
/// where `n` marks the numeric form).
pub fn param_temp_name(p: &str) -> String {
    p.to_string()
}

fn abbrev_fn_name(fname: &str) -> String {
    if fname.contains('_') {
        // multi-word: first letter of each word (all_factors -> af)
        fname
            .split('_')
            .filter(|w| !w.is_empty())
            .map(|w| w.chars().next().unwrap())
            .collect::<String>()
    } else if fname.len() <= 6 {
        // short single word: keep whole (greet -> greet)
        fname.to_string()
    } else {
        // long single word: first 3 chars
        fname.chars().take(3).collect::<String>()
    }
}

/// Identifier-sanitize a verbatim fragment: temp names must be valid
/// identifiers in every backend language (`--flag` short options,
/// `"a b"`, negative ints all arrive here verbatim and would otherwise
/// emit `--`/` ` into C). Collisions from the mapping (`a-b` vs `a_b`)
/// are the backend's job (uniqueness suffix) — this only guarantees the
/// alphabet.
fn sanitize_fragment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn abbrev_arg_name(e: &IrExpr) -> String {
    match e {
        IrExpr::Int(i) => sanitize_fragment(&i.to_string()),
        // numeric strings are meaningful verbatim (shell values are
        // strings, so `2147483645` arrives as Str); short non-numeric
        // strings too; long opaque strings are hashed.
        IrExpr::Str(s, _) if s.len() <= 8 || s.chars().all(|c| c.is_ascii_digit()) => {
            sanitize_fragment(s)
        }
        IrExpr::Var(name, _) | IrExpr::Ident(name) if name.len() <= 8 => {
            sanitize_fragment(name)
        }
        IrExpr::Arith(a) => abbrev_arith_name(a),
        _ => {
            let s = format!("{:?}", e);
            // a literal operand is the most meaningful prefix: take the
            // first few digits, so the reader sees the beginning of the
            // expression instead of an opaque hash. Fall back to a hash
            // when the expression has no literal (a long nested call).
            let digits: String = s
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .take(4)
                .collect();
            if digits.len() >= 3 {
                format!("{digits}_")
            } else {
                let mut h = 0u64;
                for b in s.bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as u64);
                }
                format!("x{:x}", h & 0xffff)
            }
        }
    }
}

/// A structural tag for an arith expression: a binary op shows the
/// operator and truncated operands (`67108847 * 67108837` ->
/// `67_x_67_`), so the reader sees the SHAPE of the expression (a
/// multiplication), not just a number that hides the operation.
fn abbrev_arith_name(a: &ArithAst) -> String {
    match a {
        ArithAst::Bin { op, lhs, rhs } => {
            let op_c = match op.as_str() {
                "*" => "x",
                "+" => "p",
                "-" => "m",
                "/" => "d",
                "%" => "mod",
                "**" => "pow",
                _ => "o",
            };
            let l = abbrev_arith_operand_name(lhs);
            let r = abbrev_arith_operand_name(rhs);
            format!("{l}_{op_c}_{r}_")
        }
        _ => {
            let s = format!("{:?}", a);
            let digits: String = s
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .take(4)
                .collect();
            if digits.len() >= 3 {
                format!("{digits}_")
            } else {
                let mut h = 0u64;
                for b in s.bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as u64);
                }
                format!("x{:x}", h & 0xffff)
            }
        }
    }
}

/// A short tag for one side of a binary arith op: a literal keeps its
/// first 2 digits, a variable keeps its name, anything else is hashed.
fn abbrev_arith_operand_name(a: &ArithAst) -> String {
    match a {
        ArithAst::Num(n) => n.to_string().chars().take(2).collect(),
        ArithAst::Var(name) | ArithAst::Ident(name) if name.len() <= 8 => name.to_string(),
        _ => {
            let s = format!("{:?}", a);
            let digits: String = s
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .take(2)
                .collect();
            if digits.is_empty() {
                "?".into()
            } else {
                digits
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim fragments must stay identifier-safe: short options
    /// (`--flag`), spaced words, and negative ints all reach the
    /// verbatim arms and would otherwise emit `--`/spaces into C.
    #[test]
    fn verbatim_args_stay_identifiers() {
        let frag = |e: &IrExpr| {
            let n = temp_base_name("f", std::slice::from_ref(e));
            assert!(
                n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "non-identifier fragment in {n}"
            );
            n
        };
        assert!(frag(&IrExpr::Str("--flag".into(), crate::ir::StrStyle::DoubleQuoted)).contains("__flag"));
        assert!(frag(&IrExpr::Int(-5)).contains("_5"));
        assert!(frag(&IrExpr::Str("a b".into(), crate::ir::StrStyle::DoubleQuoted)).contains("a_b"));
        // safe names pass through untouched (stability: no rename noise)
        assert!(frag(&IrExpr::Str("plain".into(), crate::ir::StrStyle::DoubleQuoted)).contains("plain"));
    }
}
