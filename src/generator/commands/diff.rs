use crate::ast::*;
use crate::generator::Generator;
use crate::ir::{stmt_to_perl, IrExpr, IrStmt, Sigil, StrStyle};

pub fn generate_diff_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
    _command_index: usize,
    _is_final_command: bool,
) -> String {
    let mut output = String::new();

    // Build the diff command arguments as IR expressions.
    // `perl_string_literal` emits Perl-compatible string expressions
    // (e.g. `'file.txt'`, `$var`), which work correctly inside `qx{...}`
    // because `qx{}` interpolates like a double-quoted string.
    let mut args_ir: Vec<IrExpr> = Vec::new();
    for arg in &cmd.args {
        let arg_str = generator.perl_string_literal(arg);
        args_ir.push(IrExpr::RawExpr(arg_str));
    }

    if !args_ir.is_empty() {
        // Use IrStmt::System with capture to emit clean `qx{...}` instead of
        // the verbose pipe-open boilerplate (Pattern E fix).
        //
        // The variable declared by System (e.g. `$diff_output`) is scoped to
        // the surrounding block.  When called from command_substitution
        // (is_final_command = false), the caller wraps us in `do { ... }` so
        // we emit `$diff_output;` as the final expression.
        let sys_stmt = IrStmt::System {
            cmd: IrExpr::Str("diff".to_string(), StrStyle::SingleQuoted),
            args: args_ir,
            capture: Some("diff_output".to_string()),
        };
        output.push_str(&stmt_to_perl(&sys_stmt, generator.indent_level));

        if _is_final_command {
            // Standalone diff: print the captured output.
            let print_stmt = IrStmt::Output {
                value: IrExpr::Var("diff_output".to_string(), Some(Sigil::Scalar)),
                newline: true,
                target: None,
            };
            output.push_str(&stmt_to_perl(&print_stmt, generator.indent_level));
        } else {
            // Command-substitution context: return the value as the do-block result.
            output.push_str(&generator.indent());
            output.push_str("$diff_output;\n");
        }
    } else {
        // No arguments: diff with no files produces empty output.
        output.push_str(&generator.indent());
        output.push_str("my $diff_output = q{};\n");
        if !_is_final_command {
            output.push_str(&generator.indent());
            output.push_str("$diff_output;\n");
        }
    }

    output
}
