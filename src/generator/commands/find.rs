use crate::ast::*;
use crate::generator::Generator;
use crate::ir::{self, IrExpr, IrStmt, Sigil, StrStyle};

/// Extract a simple literal value from a Word, stripping surrounding quotes
/// if present. Handles both Word::Literal and Word::StringInterpolation
/// containing a single Literal part.
fn extract_literal_from_word(word: &Word) -> Option<String> {
    match word {
        Word::Literal(s, _) => {
            Some(s.trim_matches('"').trim_matches('\'').to_string())
        }
        Word::StringInterpolation(interp, _) => {
            if interp.parts.len() == 1 {
                if let StringPart::Literal(s) = &interp.parts[0] {
                    return Some(s.trim_matches('"').trim_matches('\'').to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Convert a glob pattern (like "*.sh") to a Perl regex pattern.
fn escape_glob_to_regex(pattern: &str) -> String {
    let mut result = String::new();
    result.push('^');
    for c in pattern.chars() {
        match c {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '.' => result.push_str("\\."),
            '[' => result.push_str("["),
            ']' => result.push_str("]"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '+' => result.push_str("\\+"),
            '^' => result.push_str("\\^"),
            '$' => result.push_str("$"),
            '|' => result.push_str("\\|"),
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            '/' => result.push_str("\\/"),
            '\\' => result.push_str("\\\\"),
            _ => result.push(c),
        }
    }
    result.push('$');
    result
}

/// Result of parsing find arguments.
struct FindArgs {
    /// Raw start directory string (e.g. ".") or None if not a simple literal.
    start_dir_raw: Option<String>,
    /// The formatted Perl expression for the start directory
    /// (from `generator.perl_string_literal()` for complex words).
    start_dir_perl: String,
    /// Glob name pattern, e.g. "*.sh"
    name_pattern: Option<String>,
    /// File type, e.g. "f", "d"
    file_type: Option<String>,
    /// Maxdepth value, e.g. "3"
    maxdepth: Option<String>,
}

/// Shared argument-parsing logic used by both `generate_find_command` and
/// `generate_find_for_substitution`.
fn parse_find_args(
    generator: &mut Generator,
    cmd: &SimpleCommand,
) -> FindArgs {
    let mut start_dir_raw: Option<String> = None;
    let mut start_dir_perl = String::from(".");
    let mut name_pattern: Option<String> = None;
    let mut file_type: Option<String> = None;
    let mut maxdepth: Option<String> = None;
    let mut had_start_dir = false;

    let mut args_iter = cmd.args.iter();
    while let Some(arg) = args_iter.next() {
        if let Some(lit) = extract_literal_from_word(arg) {
            match lit.as_str() {
                "-name" => {
                    if let Some(next_arg) = args_iter.next() {
                        name_pattern = extract_literal_from_word(next_arg);
                    }
                }
                "-type" => {
                    if let Some(next_arg) = args_iter.next() {
                        file_type = extract_literal_from_word(next_arg);
                    }
                }
                "-maxdepth" => {
                    if let Some(next_arg) = args_iter.next() {
                        maxdepth = extract_literal_from_word(next_arg);
                    }
                }
                s if s.starts_with('-') => {
                    // Skip other flags
                }
                _ => {
                    // First non-flag argument is the start directory
                    if !had_start_dir {
                        had_start_dir = true;
                        // Save raw literal value for IR formatting
                        start_dir_raw = Some(lit);
                        // Also get the Perl-formatted version from the generator
                        // for cases where the word is too complex for IR formatting.
                        start_dir_perl = generator.perl_string_literal(arg);
                    }
                }
            }
        }
    }

    FindArgs {
        start_dir_raw,
        start_dir_perl,
        name_pattern,
        file_type,
        maxdepth,
    }
}

/// Produce a Perl string expression for the start directory, using IR nodes
/// when the raw value is a simple literal, falling back to the generator's
/// formatted string for complex expressions.
fn start_dir_to_ir_expr(args: &FindArgs) -> IrExpr {
    if let Some(ref raw) = args.start_dir_raw {
        // For simple literals, use IrExpr::Str so the IR backend controls quoting.
        IrExpr::Str(raw.clone(), StrStyle::SingleQuoted)
    } else {
        // Complex expression: wrap the generator's formatted string as a raw expression.
        IrExpr::RawExpr(args.start_dir_perl.clone())
    }
}

/// Build the find callback lines for the "generate_output" case (push results).
/// Each line includes its own 4-space indentation.
fn build_callback_lines_push(
    input_var: &str,
    file_type: &Option<String>,
    name_pattern: &Option<String>,
    maxdepth: &Option<String>,
) -> Vec<String> {
    let mut conditions = Vec::new();

    // Type filter
    if let Some(ref ftype) = file_type {
        let test = match ftype.as_str() {
            "f" => "-f $_",
            "d" => "-d $_",
            "l" => "-l $_",
            _ => "-e $_",
        };
        conditions.push(format!("next unless {}", test));
    }

    // Name filter
    if let Some(ref pat) = name_pattern {
        let regex = escape_glob_to_regex(pat);
        conditions.push(format!("next unless $_ =~ /{}/", regex));
    }

    // Maxdepth
    if let Some(ref depth) = maxdepth {
        if let Ok(d) = depth.parse::<usize>() {
            conditions.push(format!(
                "my $depth = ($File::Find::dir =~ tr/\\///) + 1; next if $depth > {};",
                d
            ));
        }
    }

    // Push result
    let push_line = format!("push @{}, $File::Find::name", input_var);
    if conditions.is_empty() {
        vec![format!("    {};", push_line)]
    } else {
        let joined = conditions.join(" && ");
        vec![format!("    {} if ({});", push_line, joined)]
    }
}

/// Build the find callback lines for the print case.
/// Each line includes its own 4-space indentation.
fn build_callback_lines_print(
    file_type: &Option<String>,
    name_pattern: &Option<String>,
    maxdepth: &Option<String>,
) -> Vec<String> {
    let mut lines = Vec::new();

    // Type filter
    if let Some(ref ftype) = file_type {
        let test = match ftype.as_str() {
            "f" => "-f",
            "d" => "-d",
            "l" => "-l",
            _ => "-e",
        };
        lines.push(format!("    next unless {} $_;", test));
    }

    // Name filter
    if let Some(ref pat) = name_pattern {
        let regex = escape_glob_to_regex(pat);
        lines.push(format!("    next unless $_ =~ /{}/;", regex));
    }

    // Maxdepth
    if let Some(ref depth) = maxdepth {
        if let Ok(d) = depth.parse::<usize>() {
            lines.push(format!(
                "    my $depth = ($File::Find::dir =~ tr/\\///) + 1; next if $depth > {};",
                d
            ));
        }
    }

    // Print result
    lines.push("    print \"$File::Find::name\\n\";".to_string());

    lines
}

/// Generate a native Perl find command.
///
/// When `generate_output` is true and `input_var` is non-empty, the results
/// are assigned to `$input_var` (via join + chomp).  Otherwise the results
/// are printed directly via `print "$File::Find::name\n"`.
///
/// Internally builds `IrStmt` nodes and uses `ir::stmt_to_perl()` so the
/// backend controls style.
pub fn generate_find_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    generate_output: bool,
    input_var: &str,
) -> String {
    let indent = generator.indent_level;
    let args = parse_find_args(generator, cmd);

    // Build the Perl expression for the start directory using IR.
    let start_dir_ir = start_dir_to_ir_expr(&args);
    let start_dir_expr = ir::expr_to_perl(&start_dir_ir);

    let indent_str = "    ".repeat(indent);

    let mut stmts: Vec<IrStmt> = Vec::new();

    // require File::Find;
    stmts.push(IrStmt::Require("File::Find".to_string()));

    if generate_output && !input_var.is_empty() {
        // Declare array: my @input_var;
        stmts.push(IrStmt::DeclareArray {
            var: input_var.to_string(),
            sigil: Some(Sigil::Array),
            elements: vec![],
        });

        // Build the find call with push-style callback
        let callback_lines = build_callback_lines_push(
            input_var,
            &args.file_type,
            &args.name_pattern,
            &args.maxdepth,
        );

        let find_call = format!(
            "{0}File::Find::find(sub {{\n{1}\n{0}    }}, {2});\n",
            indent_str,
            callback_lines.join("\n"),
            start_dir_expr,
        );
        stmts.push(IrStmt::RawText(find_call));

        // $input_var = join "\n", @input_var;
        stmts.push(IrStmt::Assign {
            targets: vec![ir::AssignTarget {
                var: input_var.to_string(),
                sigil: Some(Sigil::Scalar),
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "join".to_string(),
                args: vec![
                    IrExpr::Str("\n".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Var(input_var.to_string(), Some(Sigil::Array)),
                ],
            },
        });

        // chomp $input_var;
        stmts.push(IrStmt::RawText(format!(
            "{}chomp ${};\n",
            indent_str, input_var
        )));
    } else {
        // Print results directly
        let callback_lines = build_callback_lines_print(
            &args.file_type,
            &args.name_pattern,
            &args.maxdepth,
        );

        let find_call = format!(
            "{0}File::Find::find(sub {{\n{1}\n{0}    }}, {2});\n",
            indent_str,
            callback_lines.join("\n"),
            start_dir_expr,
        );
        stmts.push(IrStmt::RawText(find_call));
    }

    // Convert IR statements to Perl text
    stmts.iter()
        .map(|s| ir::stmt_to_perl(s, indent))
        .collect::<Vec<_>>()
        .join("")
}

/// Generate native Perl find for substitution (backtick/capture) context.
///
/// Returns a `do { ... }` block expression (as a Perl string) that evaluates
/// to the find output.  Internally builds `IrStmt` nodes for the block body
/// so the backend controls statement formatting.
pub fn generate_find_for_substitution(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
) -> String {
    let indent = 1; // Inside do-block, indent by 1 level
    let args = parse_find_args(generator, cmd);

    // Build the Perl expression for the start directory using IR.
    let start_dir_ir = start_dir_to_ir_expr(&args);
    let start_dir_expr = ir::expr_to_perl(&start_dir_ir);

    // Build conditions for the walker: file tests use the full path ($__p),
    // name tests use the basename ($__e).
    let mut conditions = Vec::new();

    // Type filter
    if let Some(ref ftype) = args.file_type {
        let test = match ftype.as_str() {
            "f" => "-f $__p",
            "d" => "-d $__p",
            "l" => "-l $__p",
            _ => "-e $__p",
        };
        conditions.push(test.to_string());
    }

    // Name filter (convert glob pattern to regex)
    if let Some(ref pat) = args.name_pattern {
        let regex = escape_glob_to_regex(pat);
        conditions.push(format!("$__e =~ /{}/", regex));
    }

    let condition_code = if conditions.is_empty() {
        String::from("1")
    } else {
        conditions.join(" && ")
    };

    // Maxdepth condition for inside the walker loop
    let mut maxdepth_cond = String::new();
    if let Some(ref depth) = args.maxdepth {
        if let Ok(d) = depth.parse::<usize>() {
            maxdepth_cond = format!(
                "my $__depth = ($__p =~ tr{{/}}{{}}); next if $__depth > {}; ",
                d
            );
        }
    }

    let indent_str = "    ".repeat(indent);

    // Build IR statements for the do-block body
    let mut stmts: Vec<IrStmt> = Vec::new();

    // my @find_results;
    stmts.push(IrStmt::DeclareArray {
        var: "find_results".to_string(),
        sigil: Some(Sigil::Array),
        elements: vec![],
    });

    // Custom recursive walker instead of File::Find: real find(1) descends
    // into a subdirectory at its position in readdir order, while File::Find
    // visits all of a directory's entries before descending — the output
    // order differed from the shell.
    let find_call = format!(
        "{indent}my $__find_walk; $__find_walk = sub {{ my ($__dir) = @_; opendir(my $__dh, $__dir) or return; my @__entries = readdir($__dh); closedir($__dh); for my $__e (@__entries) {{ next if $__e eq q{{.}} || $__e eq q{{..}}; my $__p = \"$__dir/$__e\"; {maxdepth_cond}if ({condition_code}) {{ push @find_results, $__p; }} if (-d $__p && !-l $__p) {{ $__find_walk->($__p); }} }} }}; do {{ my $__p = {start_dir}; my $__e = $__p; if ({condition_code}) {{ push @find_results, $__p; }} }}; $__find_walk->({start_dir});\n",
        indent = indent_str,
        condition_code = condition_code,
        start_dir = start_dir_expr,
        maxdepth_cond = maxdepth_cond,
    );
    stmts.push(IrStmt::RawText(find_call));

    // my $result = join "\n", @find_results;
    stmts.push(IrStmt::Declare {
        vars: vec![ir::Decl {
            name: "result".to_string(),
            sigil: Some(Sigil::Scalar),
        }],
        init: Some(IrExpr::Call {
            func: "join".to_string(),
            args: vec![
                IrExpr::Str("\n".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Var("find_results".to_string(), Some(Sigil::Array)),
            ],
        }),
        local: false,
    });

    // if ($result ne "") { $result .= "\n"; }
    stmts.push(IrStmt::If {
        cond: IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var("result".to_string(), Some(Sigil::Scalar))),
            op: ir::BinOpKind::Ne,
            rhs: Box::new(IrExpr::Str("".to_string(), StrStyle::SingleQuoted)),
        },
        then: vec![IrStmt::Assign {
            targets: vec![ir::AssignTarget {
                var: "result".to_string(),
                sigil: Some(Sigil::Scalar),
                indices: vec![],
            }],
            expr: IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var("result".to_string(), Some(Sigil::Scalar))),
                op: ir::BinOpKind::Concat,
                rhs: Box::new(IrExpr::Str("\n".to_string(), StrStyle::DoubleQuoted)),
            },
        }],
        elsifs: vec![],
        else_: vec![],
    });

    // $CHILD_ERROR = 0;
    stmts.push(IrStmt::SetChildError(IrExpr::Int(0)));

    // Serialize the inner statements indented inside the do-block
    let inner_code: String = stmts.iter()
        .map(|s| ir::stmt_to_perl(s, indent))
        .collect::<Vec<_>>()
        .join("");

    // Wrap in do { ... } with trailing $result;
    format!("do {{\n{}{indent_str}$result;\n}}",
        inner_code,
        indent_str = "    ",
    )
}
