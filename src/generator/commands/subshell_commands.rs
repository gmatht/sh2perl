use crate::ast::*;
use crate::generator::Generator;

pub fn generate_subshell_impl(generator: &mut Generator, command: &Command) -> String {
    let mut output = String::new();

    // Generate subshell command with proper variable scoping
    // Save current variable state and create local copies for the subshell
    output.push_str(&generator.indent());
    output.push_str("do {\n");
    generator.indent_level += 1;

    // Create local copies of all declared variables to isolate subshell scope
    for var_name in &generator.declared_locals {
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "my ${} = ${} if defined ${};\n",
            var_name, var_name, var_name
        ));
    }

    output.push_str(&generator.generate_command(command));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("};\n");

    output
}

pub fn generate_background_impl(generator: &mut Generator, command: &Command) -> String {
    let mut output = String::new();

    // Helper: recursively detect background operators in the command tree
    fn contains_background(cmd: &Command) -> bool {
        match cmd {
            Command::Background(_) => true,
            Command::Subshell(c) => contains_background(&*c),
            Command::Block(b) => b.commands.iter().any(|c| contains_background(c)),
            Command::Pipeline(p) => p.commands.iter().any(|c| contains_background(c)),
            Command::And(l, r) => contains_background(&*l) || contains_background(&*r),
            Command::Or(l, r) => contains_background(&*l) || contains_background(&*r),
            Command::Redirect(r) => contains_background(&*r.command),
            Command::If(ifc) => {
                contains_background(&*ifc.condition)
                    || contains_background(&*ifc.then_branch)
                    || ifc
                        .else_branch
                        .as_ref()
                        .map_or(false, |b| contains_background(&*b))
            }
            Command::Case(case_stmt) => case_stmt
                .cases
                .iter()
                .any(|cl| cl.body.iter().any(|c| contains_background(c))),
            Command::While(w) => {
                contains_background(&*w.condition)
                    || w.body.commands.iter().any(|c| contains_background(c))
            }
            Command::For(f) => f.body.commands.iter().any(|c| contains_background(c)),
            Command::Function(func) => func.body.commands.iter().any(|c| contains_background(c)),
            Command::Simple(_)
            | Command::BuiltinCommand(_)
            | Command::TestExpression(_)
            | Command::Assignment(_)
            | Command::Return(_)
            | Command::Break(_)
            | Command::Continue(_)
            | Command::ShoptCommand(_)
            | Command::BlankLine => false,
        }
    }

    // Prefer shell fallback when the command is a subshell/block or contains background constructs
    let prefer_shell_fallback =
        matches!(command, Command::Subshell(_) | Command::Block(_)) || contains_background(command);

    output.push_str(&generator.indent());
    output.push_str("if (my $pid = fork()) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("# Parent process continues\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} elsif (defined $pid) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("# Child process executes the background command\n");

    if prefer_shell_fallback {
        // Reconstruct the original shell command and exec bash -c so parsing and
        // diagnostics match the host shell exactly (preserves syntax errors).
        let cmd_str = crate::generator::redirects::generate_bash_command_string(command);
        let cmd_lit = generator.perl_string_literal_no_interp(&Word::literal(cmd_str));
        output.push_str(&generator.indent());
        output.push_str(&format!("exec 'bash', '-c', {};\n", cmd_lit));
        output.push_str(&generator.indent());
        output.push_str("croak \"exec failed: $OS_ERROR\\n\";\n");
    } else {
        output.push_str(&generator.generate_command(command));
        output.push_str(&generator.indent());
        output.push_str("exit(0);\n");
    }

    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} else {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("die \"Cannot fork: $ERRNO\\n\";\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");

    output
}
