//! Frontend numeric typing — the A2 `Int` verdicts as a set, for the
//! ingress passes that may fold a frontend-emitted literal.
//!
//! The imperative frontends spell a source literal as the string `"1"`
//! (py-sh-go, c-sh-go, go-sh all emit `Str` for a numeric literal and let
//! the A2 verdict in `prog.var_types` say which domain it lives in). The
//! core's own analyses read the SHELL convention instead (`Int` nodes,
//! `arith` calls), so a name holding a python integer is invisible to
//! them: `x = 1 + 2; print(x)` keeps `x` as an untyped/`String` value,
//! and every renderer ends up with a read of a name nothing declared.
//!
//! This module is the one place that answers "is this name an INT in the
//! source language", so a pass can opt in per call site. It is
//! deliberately a pure function of the program: no env vars, no target
//! checks, no globals.

use std::collections::HashSet;

use crate::ir::{IrProgram, IrType};

/// Names the A2 analysis typed as INT.
pub fn int_names(prog: &IrProgram) -> HashSet<String> {
    prog.var_types
        .iter()
        .filter(|(_, t)| matches!(t, IrType::Int))
        .map(|(n, _)| n.clone())
        .collect()
}

/// Is the literal text an exact integer (`"42"`, `"-7"`)?
pub fn int_literal(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_verdicts_only() {
        let p = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![
                ("i".to_string(), IrType::Int),
                ("s".to_string(), IrType::Str),
            ],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        };
        let names = int_names(&p);
        assert!(names.contains("i"));
        assert!(!names.contains("s"));
    }

    #[test]
    fn literal_parse() {
        assert_eq!(int_literal("42"), Some(42));
        assert_eq!(int_literal(" -7 "), Some(-7));
        assert_eq!(int_literal("1.5"), None);
        assert_eq!(int_literal("0x10"), None);
    }
}
