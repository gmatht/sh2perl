//! text_ops: Recognize common shell commands and lower to semantic IR nodes.
//!
//! `echo X | cut -d',' -f2`  → FieldExtract
//! `echo X | tr 'a-z' 'A-Z'` → CaseTransform / CharTranslate
//! `echo X | sed 's/p/r/'`   → RegSub
//! `echo X | head -n 5`      → TakeLines
//! `echo X | tail -n 5`      → TakeLines
//! `echo X | wc -l`          → WordCount
//! `${#var}`                 → StrLen
//! `expr substr "$x" 1 5`   → SubStrExtract
//! `echo X | xargs`          → StringTrim
//!
//! Each transform walks the statement list, recognizes a pattern,
//! and replaces the pipeline/exec with an IrStmt::Expr(IrExpr::Ext(...)).

use crate::ir::*;
use crate::shir_nodes::*;
use crate::shir_nodes::ExtExpr;
use std::sync::atomic::{AtomicUsize, Ordering};

static LIFT_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let before = LIFT_COUNT.load(Ordering::Relaxed);
    for stmt in stmts.iter_mut() {
        lower_stmt(stmt);
    }
    let after = LIFT_COUNT.load(Ordering::Relaxed);
    if after > before {
    }
    after > before
}

fn lower_stmt(stmt: &mut IrStmt) {
    match stmt {
        // shIR pipeline: IrExpr::Call { func: "pipeline", args: [Array(stages)] }
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
            if let [IrExpr::Array(stages)] = args.as_slice() {
                if stages.len() == 2 {
                    if let Some(replacement) = try_lower_pipeline(stages) {
                        *stmt = IrStmt::Expr(replacement);
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        IrStmt::Expr(expr) => {
            lower_expr(expr);
        }
        _ => {}
    }
}

fn lower_expr(expr: &mut IrExpr) {
    match expr {
        // ${#var} → StrLen
        IrExpr::Call { func, args } if func == "param" => {
            if let Some(replacement) = try_lower_param_len(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }
        _ => {}
    }
}

// ── Pipeline lowering ────────────────────────────────────────────────

/// Try to lower a two-stage pipeline `stage1 | stage2` to a semantic node.
/// Stages are IrExpr::Arrow(body) from the `Call { func: "pipeline", args: [Array(stages)] }` form.
fn try_lower_pipeline(stages: &[IrExpr]) -> Option<IrExpr> {
    // Each stage is an Arrow function: Arrow([Stmt])
    let stage1_body = match &stages[0] {
        IrExpr::Arrow(body) => body.as_slice(),
        _ => return None,
    };
    let stage2_body = match &stages[1] {
        IrExpr::Arrow(body) => body.as_slice(),
        _ => return None,
    };

    // stage2 must be an exec/builtin call
    let (cmd_name, cmd_args) = match stage2_body {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "exec" || func == "builtin" => {
            if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                (name.as_str(), a.as_slice())
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // stage1 produces text (echo, capture, etc.)
    let text_expr = match extract_text_from_stage(stage1_body) {
        Some(e) => {
            e
        }
        None => {
            return None;
        }
    };

    match cmd_name {
        "cut" => {
            let result = try_lower_cut(text_expr, cmd_args);
            result
        }
        "tr" => {
            let result = try_lower_tr(text_expr, cmd_args);
            result
        }
        "head" => try_lower_head_tail(text_expr, cmd_args, false),
        "tail" => try_lower_head_tail(text_expr, cmd_args, true),
        "wc" => try_lower_wc(text_expr, cmd_args),
        "sed" => {
            let result = try_lower_sed(text_expr, cmd_args);
            result
        }
        "xargs" => try_lower_xargs(text_expr),
        _ => None,
    }
}

/// Extract the text expression from a pipeline stage body (echo, capture, etc.)
fn extract_text_from_stage(stmts: &[IrStmt]) -> Option<IrExpr> {
    match stmts {
        // echo ARGS → join args as string
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec" || func == "builtin" =>
        {
            if let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args.as_slice() {
                if name == "echo" || name == "printf" {
                    // Simple case: echo with string/literal args → concatenate
                    let all_strs: Option<Vec<&str>> = echo_args.iter().map(|a| {
                        match a {
                            IrExpr::Str(s, _) => Some(s.as_str()),
                            IrExpr::Interpolate(parts) if parts.len() == 1 => {
                                match &parts[0] {
                                    InterpPart::Lit(s) => Some(s.as_str()),
                                    _ => None,
                                }
                            }
                            _ => None,
                        }
                    }).collect();
                    if let Some(strs) = all_strs {
                        let joined = strs.join(" ");
                        return Some(IrExpr::Str(joined, StrStyle::DoubleQuoted));
                    }
                }
            }
            None
        }
        // Simple expression
        [IrStmt::Expr(e)] => Some(e.clone()),
        _ => None,
    }
}

// ── cut ──────────────────────────────────────────────────────────────

fn try_lower_cut(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        if let IrExpr::Str(s, _) = a { Some(s.as_str()) } else { None }
    }).collect();

    let mut delimiter = ",".to_string();
    let mut fields_str = "";
    let mut suppress = false;
    let mut i = 0;
    while i < args_str.len() {
        let arg = args_str[i];
        if let Some(d) = arg.strip_prefix("-d") {
            if !d.is_empty() {
                delimiter = d.to_string();
            } else if let Some(next) = args_str.get(i + 1) {
                delimiter = next.to_string();
                i += 1;
            }
        } else if let Some(f) = arg.strip_prefix("-f") {
            if !f.is_empty() {
                fields_str = f;
            } else if let Some(next) = args_str.get(i + 1) {
                fields_str = next;
                i += 1;
            }
        } else if arg == "-s" {
            suppress = true;
        }
        i += 1;
    }

    if fields_str.is_empty() {
        return None;
    }

    // Parse field spec: "1", "1,3", "1-3", "1-3,5"
    let fields = parse_field_spec(fields_str);

    let mut node = FieldExtract {
        text: text,
        delimiter,
        fields,
        suppress_no_delim: suppress,
        output_delimiter: None,
    };

    // Check for -o (output delimiter) — last arg that starts with -o
    for arg in &args_str {
        if let Some(d) = arg.strip_prefix("-o") {
            node.output_delimiter = Some(d.to_string());
        }
    }

    Some(IrExpr::Ext(Box::new(node)))
}

fn parse_field_spec(spec: &str) -> Vec<FieldRange> {
    spec.split(',').filter_map(|part| {
        if let Some((start, end)) = part.split_once('-') {
            let s: u32 = start.parse().ok()?;
            let e: u32 = end.parse().ok()?;
            Some(FieldRange::Range { start: s, end: e })
        } else {
            let n: u32 = part.parse().ok()?;
            Some(FieldRange::Single(n))
        }
    }).collect()
}

// ── tr ───────────────────────────────────────────────────────────────

fn try_lower_tr(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        match a {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Interpolate(parts) if parts.len() == 1 => {
                match &parts[0] {
                    InterpPart::Lit(s) => Some(s.as_str()),
                    _ => None,
                }
            }
            _ => None,
        }
    }).collect();

    let mut delete = false;
    let mut squeeze = false;
    let mut from = "";
    let mut to = "";

    for arg in &args_str {
        if arg == &"-d" { delete = true; }
        else if arg == &"-s" { squeeze = true; }
        else if from.is_empty() { from = arg; }
        else if to.is_empty() { to = arg; }
    }

    if from.is_empty() {
        return None;
    }

    // Special case: tr 'a-z' 'A-Z' (case transform)
    if !delete && !squeeze && from == "a-z" && to == "A-Z" {
        return Some(IrExpr::Ext(Box::new(CaseTransform {
            text: text,
            upper: true,
        })));
    }
    if !delete && !squeeze && from == "A-Z" && to == "a-z" {
        return Some(IrExpr::Ext(Box::new(CaseTransform {
            text: text,
            upper: false,
        })));
    }

    Some(IrExpr::Ext(Box::new(CharTranslate {
        text: text,
        from: from.to_string(),
        to: to.to_string(),
        delete,
        squeeze,
    })))
}

// ── head / tail ──────────────────────────────────────────────────────

fn try_lower_head_tail(text: IrExpr, args: &[IrExpr], from_end: bool) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        if let IrExpr::Str(s, _) = a { Some(s.as_str()) } else { None }
    }).collect();

    let mut count_str = "10"; // default
    let mut bytes = false;

    let mut i = 0;
    while i < args_str.len() {
        if args_str[i] == "-n" || args_str[i] == "-c" {
            if args_str[i] == "-c" { bytes = true; }
            if let Some(c) = args_str.get(i + 1) {
                count_str = c;
                i += 2;
                continue;
            }
        } else if args_str[i].starts_with('-') && args_str[i].len() > 1 {
            // -5 or -c5
            let rest = &args_str[i][1..];
            if rest.starts_with('c') { bytes = true; count_str = &rest[1..]; }
            else { count_str = rest; }
        } else {
            count_str = args_str[i];
        }
        i += 1;
    }

    let count = count_str.parse::<i64>().ok().map(|n| IrExpr::Int(n))
        .unwrap_or_else(|| IrExpr::Str(count_str.to_string(), StrStyle::DoubleQuoted));

    Some(IrExpr::Ext(Box::new(TakeLines {
        text: text,
        count: count,
        from_end,
        bytes,
    })))
}

// ── wc ───────────────────────────────────────────────────────────────

fn try_lower_wc(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let mode = args.iter().filter_map(|a| {
        if let IrExpr::Str(s, _) = a { Some(s.as_str()) } else { None }
    }).find(|s| s.starts_with('-'))
        .and_then(|s| s.chars().nth(1))
        .unwrap_or('l'); // default: lines

    Some(IrExpr::Ext(Box::new(WordCount {
        text: text,
        mode: mode.to_string(),
    })))
}

// ── sed ──────────────────────────────────────────────────────────────

fn try_lower_sed(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        match a {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Interpolate(parts) if parts.len() == 1 => {
                match &parts[0] {
                    InterpPart::Lit(s) => Some(s.as_str()),
                    _ => None,
                }
            }
            _ => None,
        }
    }).collect();

    // Look for 's/pattern/replacement/flags'
    for arg in &args_str {
        if let Some(rest) = arg.strip_prefix("s/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 2 {
                let pattern = parts[0].to_string();
                let replacement = parts[1].to_string();
                let global = parts.get(2).map(|f| f.contains('g')).unwrap_or(false);

                return Some(IrExpr::Ext(Box::new(RegSub {
                    text: text,
                    pattern,
                    replacement,
                    global,
                    line_mode: true,
                })));
            }
        }
    }
    None
}

// ── xargs (trim) ─────────────────────────────────────────────────────

fn try_lower_xargs(text: IrExpr) -> Option<IrExpr> {
    Some(IrExpr::Ext(Box::new(StringTrim {
        text: text,
        leading: true,
        trailing: true,
    })))
}

// ── ${#var} → StrLen ────────────────────────────────────────────────

fn try_lower_param_len(args: &[IrExpr]) -> Option<IrExpr> {
    // param("length", var_name) or similar
    if args.len() >= 2 {
        if let IrExpr::Str(op, _) = &args[0] {
            if op == "length" {
                let var = args[1].clone();
                return Some(IrExpr::Ext(Box::new(StrLen {
                    text: var,
                })));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_field_spec() {
        let fields = parse_field_spec("2");
        assert_eq!(fields, vec![FieldRange::Single(2)]);
    }

    #[test]
    fn parse_multi_field_spec() {
        let fields = parse_field_spec("1,3");
        assert_eq!(fields, vec![FieldRange::Single(1), FieldRange::Single(3)]);
    }

    #[test]
    fn parse_range_field_spec() {
        let fields = parse_field_spec("1-3");
        assert_eq!(fields, vec![FieldRange::Range { start: 1, end: 3 }]);
    }

    #[test]
    fn parse_mixed_field_spec() {
        let fields = parse_field_spec("1-3,5");
        assert_eq!(fields, vec![
            FieldRange::Range { start: 1, end: 3 },
            FieldRange::Single(5),
        ]);
    }
}
