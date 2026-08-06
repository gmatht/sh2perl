//! cfront.rs — the MINIMAL C frontend (frontend-c-core-needs.md).
//!
//! Parses a portable-C subset into an [`IrProgram`] — the SAME contract
//! the shell frontend produces — so C programs reach every backend
//! (C → ShIR → JS/Perl/...). FOR NOW the subset envelope applies
//! (frontend-c-core-needs.md §5/§6 — a provisional boundary; the ESTree
//! worker owns the core and may lift it later).
//!
//! Subset (the "string + integer + printf" sweet spot):
//!   * types: int/long/unsigned/short/double/float/char, `char*`/`char[]`
//!     (strings), `void`; `const` ignored; no structs/unions/typedefs.
//!   * statements: blocks, if/else, while, do-while, for (→ While),
//!     return, break, continue, declarations, assignments, increments,
//!     printf/puts/exit (the I/O bridge), expression statements.
//!   * expressions: the full C precedence ladder minus structs/pointers
//!     (char* only), casts, comma, and in-expression assignments (the
//!     IR's assignment is a statement node — statement-level only).
//!   * `#include`/`#define` lines are consumed (the preprocessor stage
//!     is a later slice); only `main()` at top level (helpers: later).
//!
//! The lowering avoids `IrExpr::Arith` (ESTree-path-only — the Perl
//! renderer rejects it) — increments/compound assignments expand to
//! `IrStmt::Assign` with the general `BinOp` form. Types map to the A2
//! verdicts ({Int, Str, Any}) for `var_types` — the F1-minimal lattice
//! (Plan 13) refines them later.

use crate::ir::{BinOpKind, IrExpr, IrProgram, IrStmt, InterpPart, StrStyle};

// ── lexer ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    IntLit(i64),
    StrLit(String), // decoded escapes
    CharLit(char),
    Op(String),
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut line = 1usize;
    let mut toks = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        match c {
            ' ' | '\t' | '\r' => i += 1,
            '\n' => {
                i += 1;
                line += 1;
            }
            '#' => {
                // preprocessor line — consume to the newline (the
                // #include/#define stage is a later slice)
                while i < b.len() && b[i] as char != '\n' {
                    i += 1;
                }
            }
            '0'..='9' => {
                let start = i;
                while i < b.len() && (b[i] as char).is_ascii_digit() {
                    i += 1;
                }
                if b[start] as char == '0'
                    && i < b.len()
                    && matches!(b[i] as char, 'x' | 'X')
                {
                    i += 1;
                    while i < b.len() && (b[i] as char).is_ascii_hexdigit() {
                        i += 1;
                    }
                    let v = i64::from_str_radix(&src[start + 2..i], 16)
                        .map_err(|_| "cfront: bad hex literal".to_string())?;
                    toks.push((Tok::IntLit(v), line));
                } else {
                    let v = src[start..i]
                        .parse::<i64>()
                        .map_err(|_| "cfront: bad int literal".to_string())?;
                    toks.push((Tok::IntLit(v), line));
                }
                if i < b.len() && b[i] as char == '.' {
                    return Err(
                        "cfront: float literals unsupported (for now)"
                            .to_string(),
                    );
                }
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= b.len() {
                        return Err("cfront: unterminated string".to_string());
                    }
                    let ch = b[i] as char;
                    i += 1;
                    match ch {
                        '"' => break,
                        '\\' => {
                            if i >= b.len() {
                                return Err("cfront: bad escape".to_string());
                            }
                            let e = b[i] as char;
                            i += 1;
                            match e {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                'r' => s.push('\r'),
                                '\\' => s.push('\\'),
                                '"' => s.push('"'),
                                '\'' => s.push('\''),
                                '0' => s.push('\0'),
                                _ => {
                                    return Err(format!(
                                        "cfront: unsupported escape \\{e}"
                                    ))
                                }
                            }
                        }
                        _ => s.push(ch),
                    }
                }
                toks.push((Tok::StrLit(s), line));
            }
            '\'' => {
                i += 1;
                let mut ch = b[i] as char;
                i += 1;
                if ch == '\\' {
                    ch = b[i] as char;
                    i += 1;
                    ch = match ch {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\\' => '\\',
                        '\'' => '\'',
                        '0' => '\0',
                        other => other,
                    };
                }
                if i >= b.len() || b[i] as char != '\'' {
                    return Err("cfront: bad char literal".to_string());
                }
                i += 1;
                toks.push((Tok::CharLit(ch), line));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < b.len()
                    && ((b[i] as char).is_ascii_alphanumeric()
                        || b[i] as char == '_')
                {
                    i += 1;
                }
                toks.push((Tok::Ident(src[start..i].to_string()), line));
            }
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&'
            | '|' | '^' | '~' | '?' | ':' | '(' | ')' | '[' | ']' | '{'
            | '}' | ';' | ',' | '.' => {
                let two: &[&str] = &[
                    "++", "--", "+=", "-=", "*=", "/=", "%=", "==", "!=",
                    "<=", ">=", "&&", "||", "<<", ">>", "->",
                ];
                if i + 1 < b.len() {
                    let pair = &src[i..i + 2];
                    if let Some(op) = two.iter().find(|o| **o == pair) {
                        toks.push((Tok::Op(op.to_string()), line));
                        i += 2;
                        continue;
                    }
                }
                toks.push((Tok::Op(src[i..i + 1].to_string()), line));
                i += 1;
            }
            other => {
                return Err(format!(
                    "cfront: unexpected character '{other}'"
                ))
            }
        }
    }
    Ok(toks)
}

// ── parser ───────────────────────────────────────────────────────────

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    var_types: Vec<(String, crate::ir::IrType)>,
    /// one source line per emitted statement, in emission order
    stmt_lines: Vec<usize>,
    /// statement-recursion depth — only depth-0 (top-level) statements
    /// record their lines (the nested ones live inside If/While nodes,
    /// not in prog.stmts)
    depth: usize,
    /// >0 while parsing a C-for body: the for lowers to a While whose
    /// update sits at the body end, so a `continue` (Perl next / JS
    /// continue) would SKIP the update and loop forever. Rejected for
    /// now (the subset boundary; use a while loop).
    for_continue_depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned().map(|(t, _)| t);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    /// the source line of the current token (statement starts)
    fn current_line(&self) -> usize {
        let l = self.toks.get(self.pos).map(|(_, l)| *l).unwrap_or(0);
        l
    }
    /// record `count` emitted statements as starting at `line`
    fn note_lines(&mut self, count: usize, line: usize) {
        for _ in 0..count {
            self.stmt_lines.push(line);
        }
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect_op(&mut self, op: &str) -> Result<(), String> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(format!("cfront: expected '{op}'"))
        }
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == kw)
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// `[const] T [* | [N]]` — the A2 verdict. Returns None when not a
    /// type keyword (a plain expression/statement follows).
    fn parse_type(&mut self) -> Result<Option<(String, crate::ir::IrType)>, String> {
        let _ = self.eat_kw("const");
        let base = match self.peek() {
            Some(Tok::Ident(s)) if matches!(
                s.as_str(),
                "int" | "long" | "unsigned" | "short" | "double" | "float"
                    | "char" | "void"
            ) => s.clone(),
            _ => return Ok(None),
        };
        self.pos += 1;
        while let Some(Tok::Ident(s)) = self.peek() {
            if matches!(s.as_str(), "long" | "int" | "unsigned" | "short") {
                self.pos += 1;
            } else {
                break;
            }
        }
        let ptr = self.eat_op("*");
        if ptr && base != "char" {
            return Err(
                "cfront: only char* supported (the pointer subset — for                  now)".to_string(),
            );
        }
        let verdict = match base.as_str() {
            "char" => crate::ir::IrType::Str,
            "void" => crate::ir::IrType::Any,
            _ => crate::ir::IrType::Int,
        };
        Ok(Some((base, verdict)))
    }

    // ── expressions (precedence climbing) ────────────────────────────
    fn expr(&mut self) -> Result<IrExpr, String> {
        self.ternary()
    }
    fn ternary(&mut self) -> Result<IrExpr, String> {
        let cond = self.logical_or()?;
        if self.eat_op("?") {
            let then = self.expr()?;
            self.expect_op(":")?;
            let else_ = self.ternary()?;
            return Ok(IrExpr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                else_: Box::new(else_),
            });
        }
        Ok(cond)
    }
    fn bin_level(
        &mut self,
        ops: &[&str],
        next: fn(&mut Parser) -> Result<IrExpr, String>,
        kinds: &[BinOpKind],
    ) -> Result<IrExpr, String> {
        let mut lhs = next(self)?;
        loop {
            let mut matched: Option<BinOpKind> = None;
            for (op, kind) in ops.iter().zip(kinds) {
                if self.eat_op(op) {
                    matched = Some(kind.clone());
                    break;
                }
            }
            match matched {
                Some(kind) => {
                    let rhs = next(self)?;
                    lhs = IrExpr::BinOp {
                        lhs: Box::new(lhs),
                        op: kind,
                        rhs: Box::new(rhs),
                    };
                }
                None => return Ok(lhs),
            }
        }
    }
    fn logical_or(&mut self) -> Result<IrExpr, String> {
        self.bin_level(&["||"], Parser::logical_and, &[BinOpKind::Or])
    }
    fn logical_and(&mut self) -> Result<IrExpr, String> {
        self.bin_level(&["&&"], Parser::bit_or, &[BinOpKind::And])
    }
    fn bit_or(&mut self) -> Result<IrExpr, String> {
        self.bin_level(&["|"], Parser::bit_xor, &[BinOpKind::BitOr])
    }
    fn bit_xor(&mut self) -> Result<IrExpr, String> {
        self.bin_level(&["^"], Parser::bit_and, &[BinOpKind::BitXor])
    }
    fn bit_and(&mut self) -> Result<IrExpr, String> {
        self.bin_level(&["&"], Parser::equality, &[BinOpKind::BitAnd])
    }
    fn equality(&mut self) -> Result<IrExpr, String> {
        self.bin_level(
            &["==", "!="],
            Parser::relational,
            &[BinOpKind::Eq, BinOpKind::Ne],
        )
    }
    fn relational(&mut self) -> Result<IrExpr, String> {
        self.bin_level(
            &["<=", ">=", "<", ">"],
            Parser::shift,
            &[BinOpKind::Le, BinOpKind::Ge, BinOpKind::Lt, BinOpKind::Gt],
        )
    }
    fn shift(&mut self) -> Result<IrExpr, String> {
        self.bin_level(
            &["<<", ">>"],
            Parser::additive,
            &[BinOpKind::ShiftL, BinOpKind::ShiftR],
        )
    }
    fn additive(&mut self) -> Result<IrExpr, String> {
        self.bin_level(
            &["+", "-"],
            Parser::multiplicative,
            &[BinOpKind::Add, BinOpKind::Sub],
        )
    }
    fn multiplicative(&mut self) -> Result<IrExpr, String> {
        self.bin_level(
            &["*", "/", "%"],
            Parser::unary,
            &[BinOpKind::Mul, BinOpKind::Div, BinOpKind::Mod],
        )
    }
    fn unary(&mut self) -> Result<IrExpr, String> {
        if self.eat_op("!") {
            return Ok(IrExpr::BinOp {
                lhs: Box::new(self.unary()?),
                op: BinOpKind::Not,
                rhs: Box::new(IrExpr::Int(0)),
            });
        }
        if self.eat_op("-") {
            return Ok(IrExpr::BinOp {
                lhs: Box::new(IrExpr::Int(0)),
                op: BinOpKind::Sub,
                rhs: Box::new(self.unary()?),
            });
        }
        if self.eat_op("+") {
            return self.unary();
        }
        if self.eat_op("~") {
            return Ok(IrExpr::BinOp {
                lhs: Box::new(self.unary()?),
                op: BinOpKind::BitXor,
                rhs: Box::new(IrExpr::Int(-1)),
            });
        }
        if self.eat_op("++") || self.eat_op("--") {
            return Err(
                "cfront: increments only at statement level (for now)"
                    .to_string(),
            );
        }
        if self.eat_op("&") || self.eat_op("*") {
            return Err(
                "cfront: address-of/dereference unsupported (for now)"
                    .to_string(),
            );
        }
        // casts — reject (the F2 Cast node is the C-frontend project's)
        if matches!(self.peek(), Some(Tok::Op(s)) if s == "(") {
            let save = self.pos;
            self.pos += 1;
            if let Some(Tok::Ident(s)) = self.peek() {
                if matches!(
                    s.as_str(),
                    "int" | "long" | "unsigned" | "short" | "double"
                        | "float" | "char" | "void"
                ) {
                    return Err(
                        "cfront: casts unsupported (for now)".to_string(),
                    );
                }
            }
            self.pos = save;
        }
        self.postfix()
    }
    fn postfix(&mut self) -> Result<IrExpr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat_op("++") || self.eat_op("--") {
                return Err(
                    "cfront: increments only at statement level (for now)"
                        .to_string(),
                );
            } else if self.eat_op("[") {
                let idx = self.expr()?;
                self.expect_op("]")?;
                let name = match e {
                    IrExpr::Var(n, _) => n,
                    _ => {
                        return Err(
                            "cfront: index target must be a var".to_string()
                        )
                    }
                };
                e = IrExpr::Index {
                    var: name,
                    key: Box::new(idx),
                };
            } else if self.eat_op("(") {
                let name = match e {
                    IrExpr::Var(n, _) => n,
                    _ => {
                        return Err(
                            "cfront: call target must be a name".to_string()
                        )
                    }
                };
                let mut args = Vec::new();
                if !self.eat_op(")") {
                    loop {
                        args.push(self.expr()?);
                        if self.eat_op(")") {
                            break;
                        }
                        self.expect_op(",")?;
                    }
                }
                if !matches!(name.as_str(), "printf" | "puts" | "exit") {
                    return Err(format!(
                        "cfront: unsupported function call `{name}` (only \
                         printf/puts/exit for now)"
                    ));
                }
                // printf/puts/exit are statement-level — keep the call as
                // an expression; the statement handler rewrites it
                e = IrExpr::Call {
                    func: name,
                    args,
                };
            } else if self.eat_op(".") || self.eat_op("->") {
                return Err("cfront: structs unsupported (for now)".to_string());
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn primary(&mut self) -> Result<IrExpr, String> {
        match self.next() {
            Some(Tok::IntLit(v)) => Ok(IrExpr::Int(v)),
            Some(Tok::StrLit(s)) => Ok(IrExpr::Str(s, StrStyle::DoubleQuoted)),
            Some(Tok::CharLit(c)) => {
                Ok(IrExpr::Str(c.to_string(), StrStyle::DoubleQuoted))
            }
            Some(Tok::Ident(name)) => Ok(IrExpr::Var(name, None)),
            Some(Tok::Op(s)) if s == "(" => {
                let e = self.expr()?;
                self.expect_op(")")?;
                Ok(e)
            }
            other => Err(format!("cfront: unexpected token {other:?}")),
        }
    }

    // ── statements ───────────────────────────────────────────────────
    fn stmt(&mut self) -> Result<Vec<IrStmt>, String> {
        let top = self.depth == 0;
        let line = self.current_line();
        self.depth += 1;
        let r = self.stmt_inner();
        self.depth -= 1;
        if top {
            if let Ok(v) = &r {
                self.note_lines(v.len(), line);
            }
        }
        r
    }

    fn stmt_inner(&mut self) -> Result<Vec<IrStmt>, String> {
        if self.eat_op("{") {
            let mut body = Vec::new();
            while !self.eat_op("}") {
                if self.peek().is_none() {
                    return Err("cfront: unterminated block".to_string());
                }
                body.extend(self.stmt()?);
            }
            return Ok(body);
        }
        if self.eat_kw("if") {
            self.expect_op("(")?;
            let cond = self.expr()?;
            self.expect_op(")")?;
            let then = self.stmt()?;
            let mut else_: Vec<IrStmt> = Vec::new();
            if self.eat_kw("else") {
                else_ = self.stmt()?;
            }
            let (elsifs, else_arm) = if else_.len() == 1
                && matches!(else_[0], IrStmt::If { .. })
            {
                match else_.pop() {
                    Some(IrStmt::If {
                        cond,
                        then,
                        elsifs,
                        else_: inner_else,
                    }) => {
                        let mut e2 = elsifs;
                        e2.insert(0, (cond, then));
                        (e2, inner_else)
                    }
                    _ => unreachable!(),
                }
            } else {
                (Vec::new(), else_)
            };
            return Ok(vec![IrStmt::If {
                cond,
                then,
                elsifs,
                else_: else_arm,
            }]);
        }
        if self.eat_kw("while") {
            self.expect_op("(")?;
            let cond = self.expr()?;
            self.expect_op(")")?;
            let body = self.stmt()?;
            return Ok(vec![IrStmt::While { cond, body }]);
        }
        if self.eat_kw("do") {
            let body = self.stmt()?;
            if !self.eat_kw("while") {
                return Err("cfront: expected 'while' after do-block".to_string());
            }
            self.expect_op("(")?;
            let cond = self.expr()?;
            self.expect_op(")")?;
            self.expect_op(";")?;
            return Ok(vec![IrStmt::DoWhile { body, cond, until: false }]);
        }
        if self.eat_kw("for") {
            self.expect_op("(")?;
            let init = self.stmt_for_init()?;
            self.expect_op(";")?;
            let mut init_stmts = init;
            let cond = if self.eat_op(";") {
                IrExpr::Int(1)
            } else {
                let c = self.expr()?;
                self.expect_op(";")?;
                c
            };
            let upd: Option<IrStmt> = if self.eat_op(")") {
                None
            } else {
                // the update is a statement-level form (`i++`, `i+=2`,
                // `i = i + 1`) — the no-semi statement parser
                let u = self.expr_stmt_no_semi()?;
                self.expect_op(")")?;
                Some(u)
            };
            self.for_continue_depth += 1;
            let body = self.stmt()?;
            self.for_continue_depth -= 1;
            let mut inner: Vec<IrStmt> = body;
            if let Some(u) = upd {
                inner.push(u);
            }
            init_stmts.push(IrStmt::While {
                cond,
                body: inner,
            });
            return Ok(init_stmts);
        }
        if self.eat_kw("return") {
            // main's return IS the program exit — the shell `Exit` node
            // (a top-level `Return` would render as a subroutine return
            // in Perl — invalid at program scope)
            let v = if self.eat_op(";") {
                None
            } else {
                let e = self.expr()?;
                self.expect_op(";")?;
                Some(e)
            };
            return Ok(vec![IrStmt::Exit(v)]);
        }
        if self.eat_kw("break") {
            self.expect_op(";")?;
            return Ok(vec![IrStmt::Expr(IrExpr::Call {
                func: "break".to_string(),
                args: vec![],
            })]);
        }
        if self.eat_kw("continue") {
            self.expect_op(";")?;
            if self.for_continue_depth > 0 {
                return Err(
                    "cfront: continue inside a for-loop unsupported (for                      now) — the for lowers to a while whose update would                      be skipped; use a while loop".to_string(),
                );
            }
            return Ok(vec![IrStmt::Expr(IrExpr::Call {
                func: "continue".to_string(),
                args: vec![],
            })]);
        }
        self.decl_or_expr_stmt()
    }

    /// The for-loop init: a declaration OR an assignment/expr.
    fn stmt_for_init(&mut self) -> Result<Vec<IrStmt>, String> {
        if let Some((_, verdict)) = self.parse_type()? {
            self.decl_list(true, verdict)
        } else {
            Ok(vec![self.expr_stmt_no_semi()?])
        }
    }

    /// `T name [= init] [, name [= init]] ;`
    fn decl_list(
        &mut self,
        no_semi: bool,
        verdict: crate::ir::IrType,
    ) -> Result<Vec<IrStmt>, String> {
        let mut out = Vec::new();
        loop {
            let name = match self.next() {
                Some(Tok::Ident(n)) => n,
                _ => return Err("cfront: expected a variable name".to_string()),
            };
            if self.eat_op("[") {
                let _ = self.expr()?;
                self.expect_op("]")?;
            }
            self.var_types.push((name.clone(), verdict));
            // a declaration statement (`my $x` in Perl; the estree path
            // renders the shell declare) — before the initializer
            out.push(IrStmt::Declare {
                vars: vec![crate::ir::Decl {
                    name: name.clone(),
                    sigil: None,
                }],
                init: None,
                local: false,
            });
            if self.eat_op("=") {
                let rhs = self.assign_expr()?;
                out.push(IrStmt::Assign {
                    targets: vec![crate::ir::AssignTarget {
                        var: name.clone(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: rhs,
                });
            }
            if !no_semi && self.eat_op(";") {
                break;
            }
            if no_semi && self.eat_op(")") {
                break;
            }
            if !no_semi {
                self.expect_op(",")?;
            } else if matches!(self.peek(), Some(Tok::Op(op)) if op == ";" || op == ")") {
                // a single declaration in a for-init — stop at the `;`
                // (the for parser consumes it) or the closing `)`
                break;
            } else {
                // a comma-separated declaration list inside a for-init
                // (minimal: single declarations only)
                return Err(
                    "cfront: multi-variable for-init declarations unsupported (for now)"
                        .to_string(),
                );
            }
        }
        if out.is_empty() {
            Ok(vec![IrStmt::Expr(IrExpr::Int(0))])
        } else {
            Ok(out)
        }
    }

    fn decl_or_expr_stmt(&mut self) -> Result<Vec<IrStmt>, String> {
        if let Some((_, verdict)) = self.parse_type()? {
            // the type was consumed; the declaration list follows
            self.decl_list(false, verdict)
        } else {
            Ok(vec![self.expr_stmt()?])
        }
    }

    /// An expression statement — with the statement-level forms the IR
    /// supports: assignments, increments, printf/puts/exit, plain exprs.
    fn expr_stmt(&mut self) -> Result<IrStmt, String> {
        let s = self.expr_stmt_no_semi()?;
        self.expect_op(";")?;
        Ok(s)
    }

    /// The expression-statement body WITHOUT the trailing `;` — shared by
    /// plain statements and the for-loop init.
    fn expr_stmt_no_semi(&mut self) -> Result<IrStmt, String> {
        if self.eat_op(";") {
            return Ok(IrStmt::Expr(IrExpr::Int(0)));
        }
        // `name op= ...` / `name++` / `++name` / `name--` / `--name`
        // (lookahead into OWNED values first — the mutable calls below
        // can't run while a borrow of self.toks is live)
        let head: Option<(String, String)> = match (
            self.toks.get(self.pos).map(|(t, _)| t).cloned(),
            self.toks.get(self.pos + 1).map(|(t, _)| t).cloned(),
        ) {
            (Some(Tok::Ident(n)), Some(Tok::Op(op))) => Some((n, op)),
            _ => None,
        };
        if let Some((name, op)) = head {
            if matches!(op.as_str(), "=" | "+=" | "-=" | "*=" | "/=" | "%=") {
                self.pos += 2;
                let rhs = self.assign_expr()?;
                return self.make_assign(&name, &op, rhs);
            }
            if op == "++" || op == "--" {
                self.pos += 2;
                let aop = if op == "++" { "+=" } else { "-=" };
                return self.make_assign(&name, aop, IrExpr::Int(1));
            }
            // prefix `++name;` / `--name;`
            if matches!(op.as_str(), "++" | "--") {
                self.pos += 1;
                match self.next() {
                    Some(Tok::Ident(n)) => {
                        let aop = if op == "++" { "+=" } else { "-=" };
                        return self.make_assign(&n, aop, IrExpr::Int(1));
                    }
                    _ => unreachable!(),
                }
            }
        }
        let e = self.expr()?;
        // statement-level printf/puts/exit → the I/O bridge
        if let IrExpr::Call { func, args } = e {
            match func.as_str() {
                "exit" => {
                    return Ok(IrStmt::Exit(args.into_iter().next()));
                }
                "puts" => {
                    let v = args.into_iter().next().unwrap_or(IrExpr::Str(
                        String::new(),
                        StrStyle::DoubleQuoted,
                    ));
                    return Ok(IrStmt::Output {
                        value: v,
                        newline: true,
                        target: None,
                    });
                }
                "printf" => {
                    let mut it = args.into_iter();
                    let fmt = match it.next() {
                        Some(IrExpr::Str(s, _)) => s,
                        _ => {
                            return Err(
                                "cfront: printf format must be a literal"
                                    .to_string(),
                            )
                        }
                    };
                    let rest: Vec<IrExpr> = it.collect();
                    let value = build_printf_interp(&fmt, rest)?;
                    return Ok(IrStmt::Output {
                        value,
                        newline: false,
                        target: None,
                    });
                }
                _ => {}
            }
            return Ok(IrStmt::Expr(IrExpr::Call { func, args }));
        }
        Ok(IrStmt::Expr(e))
    }

    fn make_assign(
        &mut self,
        var: &str,
        op: &str,
        rhs: IrExpr,
    ) -> Result<IrStmt, String> {
        let expr = match op {
            "=" => rhs,
            "+=" => IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(var.to_string(), None)),
                op: BinOpKind::Add,
                rhs: Box::new(rhs),
            },
            "-=" => IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(var.to_string(), None)),
                op: BinOpKind::Sub,
                rhs: Box::new(rhs),
            },
            "*=" => IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(var.to_string(), None)),
                op: BinOpKind::Mul,
                rhs: Box::new(rhs),
            },
            "/=" => IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(var.to_string(), None)),
                op: BinOpKind::Div,
                rhs: Box::new(rhs),
            },
            "%=" => IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(var.to_string(), None)),
                op: BinOpKind::Mod,
                rhs: Box::new(rhs),
            },
            _ => unreachable!(),
        };
        Ok(IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
        })
    }

    /// `=`-assignment RHS (expression-level assignments are rejected —
    /// the IR's assignment is a statement).
    fn assign_expr(&mut self) -> Result<IrExpr, String> {
        self.expr()
    }
}

fn name_is_next_var(p: &Parser) -> bool {
    matches!(p.toks.get(p.pos + 1).map(|(t, _)| t), Some(Tok::Ident(_)))
}

/// `printf("x=%d, %s", i, s)` → Interpolate([Lit("x="), Expr(i), ...]).
/// The format's %d/%s/%c/%f/%i/%u/%ld/%lu consume one arg each; %% is a
/// literal percent.
fn build_printf_interp(fmt: &str, args: Vec<IrExpr>) -> Result<IrExpr, String> {
    let mut parts: Vec<InterpPart> = Vec::new();
    let mut lit = String::new();
    let b: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    let mut args = args.into_iter();
    while i < b.len() {
        if b[i] == '%' {
            if i + 1 < b.len() && b[i + 1] == '%' {
                lit.push('%');
                i += 2;
                continue;
            }
            if !lit.is_empty() {
                parts.push(InterpPart::Lit(std::mem::take(&mut lit)));
            }
            // skip flags/width/precision/length modifiers (`%ld`, `%-5d`,
            // `%08.2f` — minimal: any non-conversion chars), then the
            // conversion char
            let mut j = i + 1;
            while j < b.len()
                && !matches!(
                    b[j],
                    'd' | 'i' | 'u' | 'f' | 's' | 'c' | 'x' | 'X' | 'o'
                )
            {
                j += 1;
            }
            if j >= b.len() {
                return Err("cfront: bad printf format".to_string());
            }
            i = j + 1;
            let arg = args
                .next()
                .ok_or_else(|| "cfront: printf: not enough arguments".to_string())?;
            parts.push(InterpPart::Expr(Box::new(arg)));
        } else {
            lit.push(b[i]);
            i += 1;
        }
    }
    if !lit.is_empty() {
        parts.push(InterpPart::Lit(lit));
    }
    Ok(IrExpr::Interpolate(parts))
}

/// Parse a C program (the subset) into an [`IrProgram`]. Only `main()`
/// at top level (the helper-function slice is next); global
/// declarations become top-level assignments.
pub fn c_to_ir(src: &str) -> Result<IrProgram, String> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        pos: 0,
        var_types: Vec::new(),
        stmt_lines: Vec::new(),
        depth: 0,
        for_continue_depth: 0,
    };
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut found_main = false;
    while p.peek().is_some() {
        // a type keyword: a global declaration OR a function definition
        if let Some((_base, verdict)) = p.parse_type()? {
            let name = match p.next() {
                Some(Tok::Ident(n)) => n,
                _ => return Err("cfront: expected a global/function name".to_string()),
            };
            if p.eat_op("(") {
                // a function definition
                if !p.eat_op(")") {
                    loop {
                        let _t = p.parse_type()?;
                        match p.next() {
                            Some(Tok::Ident(pn)) => {
                                p.var_types
                                    .push((pn, crate::ir::IrType::Any));
                            }
                            _ => {
                                return Err("cfront: bad parameter".to_string())
                            }
                        }
                        if p.eat_op(")") {
                            break;
                        }
                        p.expect_op(",")?;
                    }
                }
                if name != "main" {
                    return Err(format!(
                        "cfront: only main() supported for now (function \
                         `{name}` found)"
                    ));
                }
                found_main = true;
                p.expect_op("{")?;
                let body = p.block_body()?;
                stmts.extend(body);
                continue;
            }
            // a global declaration (with the `[N]` array form)
            p.var_types.push((name.clone(), verdict));
            if p.eat_op("[") {
                let _ = p.expr()?;
                p.expect_op("]")?;
            }
            let rhs = if p.eat_op("=") {
                p.assign_expr()?
            } else {
                IrExpr::Int(0)
            };
            p.note_lines(1, p.current_line());
            stmts.push(IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: name,
                    sigil: None,
                    indices: vec![],
                }],
                expr: rhs,
            });
            p.expect_op(";")?;
            continue;
        }
        return Err(format!(
            "cfront: unexpected top-level construct {:?}",
            p.peek()
        ));
    }
    if !found_main {
        return Err("cfront: no main() found".to_string());
    }
    let mut var_types = p.var_types;
    var_types.sort_by(|a, b| a.0.cmp(&b.0));
    var_types.dedup_by(|a, b| a.0 == b.0);
    // the statement lines: (top-level stmt index, source line) — the
    // stmt_lines parallel prog.stmts in emission order
    let stmt_lines: Vec<(usize, usize)> = p
        .stmt_lines
        .iter()
        .enumerate()
        .map(|(i, l)| (i, *l))
        .collect();
    Ok(IrProgram {
        imports: vec![],
        requires: vec![],
        stmts,
        subs: vec![],
        var_types,
        stmt_lines,
        var_lengths: vec![],
        var_const: vec![],
    })
}

impl Parser {
    /// Parse the main body — the opening `{` was already consumed.
    fn block_body(&mut self) -> Result<Vec<IrStmt>, String> {
        let mut body = Vec::new();
        while !self.eat_op("}") {
            if self.peek().is_none() {
                return Err("cfront: unterminated main body".to_string());
            }
            body.extend(self.stmt()?);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(src: &str) -> IrProgram {
        c_to_ir(src).expect("parse")
    }

    #[test]
    fn hello_world() {
        let prog = t(
            "#include <stdio.h>\nint main() { printf(\"hello\\n\"); return 0; }",
        );
        assert_eq!(prog.stmts.len(), 2);
        match &prog.stmts[0] {
            IrStmt::Output { newline: false, value, .. } => {
                match value {
                    IrExpr::Interpolate(parts) => match &parts[0] {
                        InterpPart::Lit(s) => assert_eq!(s, "hello\n"),
                        _ => panic!("expected literal"),
                    },
                    _ => panic!("expected interpolate"),
                }
            }
            other => panic!("expected Output, got {:?}", other),
        }
        match &prog.stmts[1] {
            IrStmt::Exit(Some(IrExpr::Int(0))) => {}
            other => panic!("expected exit 0, got {:?}", other),
        }
    }

    #[test]
    fn loop_and_types() {
        let prog = t(
            "int main() { int i; long sum = 0; for (i = 1; i <= 100; i++) { sum += i; } printf(\"sum=%ld\\n\", sum); return 0; }",
        );
        let types: Vec<(String, _)> = prog.var_types.clone();
        assert!(types
            .iter()
            .any(|(n, t)| n == "i" && *t == crate::ir::IrType::Int));
        assert!(types
            .iter()
            .any(|(n, t)| n == "sum" && *t == crate::ir::IrType::Int));
        let has_while = prog
            .stmts
            .iter()
            .any(|s| matches!(s, IrStmt::While { .. }));
        assert!(has_while, "for must lower to a While");
    }

    #[test]
    fn rejects_pointers() {
        assert!(c_to_ir("int main() { int *p; return 0; }").is_err());
    }
}
