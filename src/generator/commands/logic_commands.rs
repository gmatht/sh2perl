use crate::ast::*;
use crate::generator::control_flow::{collect_assigned_vars, hoist_my_declarations};
use crate::generator::Generator;
use crate::ir::{AssignTarget, IrExpr, IrStmt, Sigil};

/// Generate logical AND operation (left && right)
pub fn generate_logical_and(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();

    // Generate: left && right
    output.push_str(&generator.indent());

    // For TestExpression, use the test expression directly as the condition
    if let Command::TestExpression(_) = left {
        output.push_str("if (");
        let test_result = generator.generate_command(left);
        output.push_str(&test_result);
        output.push_str(") {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        let right_perl = generator.generate_command(right);
        output.push_str(&right_perl);
        // The right side may be another TestExpression — a bare boolean
        // expression used as a statement MUST end with `;` (e.g. the second
        // `[[ -f "$1" ]]` in `[[ -n "$1" ]] && [[ -f "$1" ]] && echo`).
        let right_trim = right_perl.trim_end();
        if !right_trim.ends_with(';') && !right_trim.ends_with('}') {
            output.push(';');
        }
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&generator.indent());
        output.push_str("$CHILD_ERROR = 0;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} else {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("$CHILD_ERROR = 1;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        return output;
    }

    // For continue/break, the left command transfers control (jumps),
    // so the right side is unreachable inside a loop.  Emit both
    // sequentially and let `next`/`last` skip the right code.
    if matches!(left, Command::Continue(_) | Command::Break(_)) {
        generator.suppress_set_e_depth += 1;
        output.push_str(&generator.generate_command(left));
        generator.suppress_set_e_depth -= 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        return output;
    }

    // Pre-declare variables assigned in the right branch BEFORE the if statement,
    // so that `my $var = ...` does not end up inside the conditional body or
    // inside the if(...) condition parentheses (which would be a syntax error).
    {
        let mut right_vars = std::collections::HashSet::new();
        collect_assigned_vars(right, &mut right_vars);
        hoist_my_declarations(generator, &right_vars, &mut output);
    }
    // Pre-declare variables assigned in the LEFT branch as well: bash has no
    // block scoping, so `n=$(...) && test "$n" = ...` leaves $n visible after
    // the statement even though the condition body is emitted inside a do {}.
    {
        let mut left_vars = std::collections::HashSet::new();
        collect_assigned_vars(left, &mut left_vars);
        hoist_my_declarations(generator, &left_vars, &mut output);
    }

    // For other commands, use the original pattern with exit code checking
    output.push_str("if (");

    // For RedirectCommand, we need to check exit code
    if let Command::Redirect(_) = left {
        // Generate the redirect command first, then check exit code
        output.push_str("do {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(left));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} == 0");
    } else if let Command::Simple(simple_cmd) = left {
        if let Word::Literal(name, _) = &simple_cmd.name {
            if name == "grep" {
                // For grep commands in logical AND, generate the command in a block
                // and check if it found any matches
                output.push_str("do {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                let grep_result = generator.generate_command(left);

                // Extract the grep_result variable name from the generated code
                let mut _grep_result_var = String::new();
                for line in grep_result.lines() {
                    if line.trim_start().starts_with("my $grep_result_") {
                        if let Some(end) = line.find(';') {
                            let var_decl = &line[3..end]; // Remove "my " prefix
                            _grep_result_var = var_decl.to_string();
                        }
                    }
                    if !line.trim().is_empty() {
                        output.push_str(&generator.indent());
                        output.push_str(line);
                        output.push_str("\n");
                    }
                }

                output.push_str(&generator.indent());
                // For grep commands, check if matches were found by looking at the filtered array
                // The grep command should have already set $CHILD_ERROR correctly
                output.push_str("$CHILD_ERROR == 0\n");

                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}");
            } else {
                // For other command types, generate the command and check exit code
                output.push_str("do {\n");
                generator.indent_level += 1;
                // Temporarily save the current indent level and reset it for command generation
                let saved_indent_level = generator.indent_level;
                generator.indent_level = 0;
                let command = if let Command::Pipeline(pipeline) = left {
                    crate::generator::commands::pipeline_commands::generate_pipeline_with_print_option(
                        generator,
                        pipeline,
                        false,
                    )
                } else {
                    generator.generate_command(left)
                };
                // Restore the indent level
                generator.indent_level = saved_indent_level;
                // The command generator already handles indentation, so we don't need to add extra indentation
                output.push_str(&command);
                output.push_str(&generator.indent());
                output.push_str("$CHILD_ERROR == 0\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}");
            }
        } else {
            // For non-literal command names, generate the command and check exit code
            output.push_str("do {\n");
            generator.indent_level += 1;
            // Temporarily save the current indent level and reset it for command generation
            let saved_indent_level = generator.indent_level;
            generator.indent_level = 0;
            let command = generator.generate_command(left);
            // Restore the indent level
            generator.indent_level = saved_indent_level;
            // The command generator already handles indentation, so we don't need to add extra indentation
            output.push_str(&command);
            output.push_str(&generator.indent());
            output.push_str("$CHILD_ERROR == 0\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}");
        }
    } else {
        // For other command types, generate the command and check exit code
        output.push_str("do {\n");
        generator.indent_level += 1;
        // Temporarily save the current indent level and reset it for command generation
        let saved_indent_level = generator.indent_level;
        generator.indent_level = 0;
        let command = generator.generate_command(left);
        // Restore the indent level
        generator.indent_level = saved_indent_level;
        // The command generator already handles indentation, so we don't need to add extra indentation
        output.push_str(&command);
        output.push_str(&generator.indent());
        output.push_str("$CHILD_ERROR == 0\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}");
    }

    output.push_str(") {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    let right_perl = generator.generate_command(right);
    output.push_str(&right_perl);
    if !right_perl.ends_with('\n') {
        output.push('\n');
    }
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");

    output
}

/// Generate logical OR operation (left || right)
pub fn generate_logical_or(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();

    // Generate: left || right
    // OR operations should NEVER capture STDOUT - they're about conditional execution
    output.push_str(&generator.indent());

    // Check if left is a test expression
    if let Command::TestExpression(_) = left {
        // Pre-declare variables assigned in the right branch so that `my $var = ...`
        // does not end up inside the conditional body.
        {
            let mut right_vars = std::collections::HashSet::new();
            collect_assigned_vars(right, &mut right_vars);
            hoist_my_declarations(generator, &right_vars, &mut output);
        }
        // For test expressions, generate: if (!left) { right }
        output.push_str("if (!(");
        generator.suppress_set_e_depth += 1;
        output.push_str(&generator.generate_command(left));
        generator.suppress_set_e_depth -= 1;
        output.push_str(")) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        let right_perl = generator.generate_command(right);
        output.push_str(&right_perl);
        if !right_perl.ends_with('\n') {
            output.push('\n');
        }
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else if let Command::And(_and_left, _and_right) = left {
        // Special handling for AND operations in OR context
        // Use the logical AND generation function to handle the AND operation properly
        generator.suppress_set_e_depth += 1;
        let and_result = generator.generate_command(left);
        generator.suppress_set_e_depth -= 1;
        output.push_str(&and_result);
        // Pre-declare variables assigned in the right branch
        {
            let mut right_vars = std::collections::HashSet::new();
            collect_assigned_vars(right, &mut right_vars);
            hoist_my_declarations(generator, &right_vars, &mut output);
        }
        output.push_str(&generator.indent());
        output.push_str("if ($CHILD_ERROR != 0) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        let right_perl = generator.generate_command(right);
        output.push_str(&right_perl);
        if !right_perl.ends_with('\n') {
            output.push('\n');
        }
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        return output;
    } else if matches!(left, Command::Continue(_) | Command::Break(_)) {
        // `continue || X` and `break || X` in bash: the left command transfers
        // control (jumps to next iteration / exits the loop), so the right-hand
        // side is unreachable inside a loop.  In Perl `next`/`last` also transfer
        // control, so we emit both statements sequentially — the right side is
        // dead code inside a loop (which matches bash behaviour where the right
        // side of `||` only runs if the left side fails, and `continue`/`break`
        // never fail inside a loop).
        generator.suppress_set_e_depth += 1;
        output.push_str(&generator.generate_command(left));
        generator.suppress_set_e_depth -= 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
    } else {
        // For commands that generate Perl code (like grep, ls), we need to handle them specially
        // to avoid embedding Perl code inside shell backticks
        if let Command::Simple(simple_cmd) = left {
            if let Word::Literal(name, _) = &simple_cmd.name {
                if name == "grep" {
                    // For grep commands in logical OR, generate the command and check exit code
                    generator.suppress_set_e_depth += 1;
                    output.push_str(&generator.generate_command(left));
                    generator.suppress_set_e_depth -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("if ($CHILD_ERROR != 0) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(right));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    return output;
                } else if name == "ls" {
                    // For ls commands in logical OR, generate the command and check if files were found
                    generator.suppress_set_e_depth += 1;
                    output.push_str(&generator.generate_command(left));
                    generator.suppress_set_e_depth -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("if ( !defined $ls_success || $ls_success == 0 ) {\n");
                    generator.indent_level += 1;
                    // Right command should be indented inside the if block (4 spaces)
                    // The echo command generates code with its own indentation based on indent_level
                    // We need to ensure it generates with no indentation, then add exactly 4 spaces
                    // Save the current indent_level (which is now 1 after the increment above)
                    let saved_indent = generator.indent_level;
                    // Set indent_level to 0 so echo generates with no indentation
                    generator.indent_level = 0;
                    let right_cmd_raw = generator.generate_command(right);
                    // Restore indent level
                    generator.indent_level = saved_indent;
                    // The echo command may generate code with indentation even when indent_level=0
                    // We MUST strip ALL leading whitespace from every line and add exactly 4 spaces
                    // Process each line: remove ALL leading whitespace, then add exactly 4 spaces
                    for line in right_cmd_raw.lines() {
                        // Remove ALL leading whitespace using trim_start
                        let trimmed = line.trim_start();
                        if !trimmed.is_empty() {
                            // CRITICAL: Add exactly 4 spaces (literal string), not using generator.indent()
                            // This ensures we always have exactly 4 spaces, regardless of what the echo command generated
                            output.push_str("    ");
                            output.push_str(trimmed);
                            output.push_str("\n");
                        }
                    }
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    output.push_str(&crate::ir::stmt_to_perl(
                        &IrStmt::Assign {
                            targets: vec![AssignTarget {
                                var: "main_exit_code".to_string(),
                                sigil: Some(Sigil::Scalar),
                                indices: vec![],
                            }],
                            expr: IrExpr::Int(0),
                            asm: None,
                        },
                        generator.indent_level,
                    ));
                    return output;
                }
            }
        } else if let Command::Redirect(redirect_cmd) = left {
            // Handle Redirect commands that might contain ls commands
            if let Command::Simple(simple_cmd) = &*redirect_cmd.command {
                if let Word::Literal(name, _) = &simple_cmd.name {
                    if name == "ls" {
                        // For ls commands in logical OR, generate the command and check if files were found
                        generator.suppress_set_e_depth += 1;
                        output.push_str(&generator.generate_command(left));
                        generator.suppress_set_e_depth -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("if ( !defined $ls_success || $ls_success == 0 ) {\n");
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        let right_perl = generator.generate_command(right);
                        output.push_str(&right_perl);
                        if !right_perl.ends_with('\n') {
                            output.push('\n');
                        }
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                        output.push_str(&crate::ir::stmt_to_perl(
                            &IrStmt::Assign {
                                targets: vec![AssignTarget {
                                    var: "main_exit_code".to_string(),
                                    sigil: Some(Sigil::Scalar),
                                    indices: vec![],
                                }],
                                expr: IrExpr::Int(0),
                                asm: None,
                            },
                            generator.indent_level,
                        ));
                        return output;
                    }
                }
            }
        }

        // Execute left command and check exit code
        generator.suppress_set_e_depth += 1;
        output.push_str(&generator.generate_command(left));
        generator.suppress_set_e_depth -= 1;

        // Pre-declare variables assigned in the right branch so that `my $var = ...`
        // does not end up inside the conditional body (which would make the variable
        // inaccessible to code after the `||` expression).
        {
            let mut right_vars = std::collections::HashSet::new();
            collect_assigned_vars(right, &mut right_vars);
            hoist_my_declarations(generator, &right_vars, &mut output);
        }

        // Execute right command if left command fails
        let exit_code_var = "$CHILD_ERROR";

        output.push_str(&generator.indent());
        output.push_str(&format!("if ({} != 0) {{\n", exit_code_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        let right_perl = generator.generate_command(right);
        output.push_str(&right_perl);
        if !right_perl.ends_with('\n') {
            output.push('\n');
        }
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }

    output
}

/// Check if a command is a diff command (for exit code handling)
fn contains_diff_command(cmd: &Command) -> bool {
    match cmd {
        Command::Simple(simple_cmd) => {
            if let Word::Literal(name, _) = &simple_cmd.name {
                name == "diff"
            } else {
                false
            }
        }
        Command::Redirect(redirect_cmd) => contains_diff_command(&redirect_cmd.command),
        _ => false,
    }
}
