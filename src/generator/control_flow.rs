use super::Generator;
use crate::ast::*;
use regex::Regex;
use std::collections::HashMap;

pub fn generate_if_statement_impl(generator: &mut Generator, if_stmt: &IfStatement) -> String {
    let mut output = String::new();

    // Pre-declare variables that are assigned in any branch, so that
    // `my $var = ...` does not end up inside the conditional body (which
    // Perl::Critic's ProhibitConditionalDeclarations would flag).
    {
        let mut branch_vars = std::collections::HashSet::new();
        collect_assigned_vars(&if_stmt.then_branch, &mut branch_vars);
        if let Some(else_branch) = &if_stmt.else_branch {
            collect_assigned_vars(else_branch, &mut branch_vars);
        }
        hoist_my_declarations(generator, &branch_vars, &mut output);
    }

    // Generate condition
    output.push_str("if (");
    match &*if_stmt.condition {
        Command::Simple(cmd) if cmd.name == "[" || cmd.name == "test" => {
            generator.generate_test_command(cmd, &mut output);
        }
        Command::Simple(cmd) if cmd.name == "let" => {
            // Generate arithmetic expression directly without assigning
            // to $main_exit_code, so the program exit code is not
            // polluted by the let condition.
            let mut parts = Vec::new();
            for arg in &cmd.args {
                let expr = match arg {
                    Word::Literal(s, _) => s.clone(),
                    _ => generator.word_to_perl(arg),
                };
                let perl_expr = generator.convert_arithmetic_to_perl(&expr);
                // convert_arithmetic_to_perl already wraps the expression
                // in eval { int(...) } // "", so use it directly.
                parts.push(perl_expr);
            }
            output.push_str(&parts.join(" && "));
        }
        Command::TestExpression(test_expr) => {
            let test_result = generator.generate_test_expression(test_expr);
            // Avoid double parentheses: the test expression generator already
            // wraps its result in (...), and the outer `if (...)` adds another
            // layer.  Strip the outer parens here.
            let trimmed = test_result.trim();
            if trimmed.starts_with('(') && trimmed.ends_with(')') {
                output.push_str(&trimmed[1..trimmed.len()-1]);
            } else {
                output.push_str(&test_result);
            }
        }
        Command::And(_, _) | Command::Or(_, _) => {
            let cond = generate_combined_test_condition(generator, &if_stmt.condition);
            output.push_str(&cond);
        }
        Command::Not(inner) => {
            // `! cmd` in shell: enter then-branch when cmd fails (exit != 0).
            // Generate the inner command as a raw exit-code expression (no !() wrapper)
            // so that non-zero (failure) is truthy in Perl.
            generator.suppress_set_e_depth += 1;
            let mut cond = generator.generate_command(inner);
            generator.suppress_set_e_depth -= 1;
            let cond = cond
                .trim_start()
                .strip_prefix("$main_exit_code = ")
                .unwrap_or(&cond)
                .trim_end_matches(|c: char| c == ';' || c == '\n' || c == ' ' || c == '\t')
                .trim_end_matches(';')
                .to_string();
            output.push_str(&cond);
        }
        _ => {
            generator.suppress_set_e_depth += 1;
            let mut cond = generator.generate_command(&if_stmt.condition);
            generator.suppress_set_e_depth -= 1;
            // Strip trailing semicolons and whitespace - the condition
            // is used inside if(...) not as a standalone statement
            let mut cond = cond
                .trim_start()
                .strip_prefix("$main_exit_code = ")
                .unwrap_or(&cond)
                .trim_end_matches(|c: char| c == ';' || c == '\n' || c == ' ' || c == '\t')
                .trim_end_matches(';')
                .to_string();
            // If the stripped condition ends with `}` (e.g. from a pipeline's
            // do { ... } block) and is used inside another do { } block (as in
            // a combined condition), preserve the trailing semicolon so the
            // inner do block is properly terminated.
            if cond.ends_with('}') {
                cond.push(';');
            }
            // Negate the condition for shell functions:
            // shell returns 0 for success, non-zero for failure.
            // In Perl 0 is falsy, so we write
            // `if (!cond()) { ... }` to match shell semantics
            // where `if func; then` enters when func returns 0.
            output.push_str(&format!("!({})", cond));
        }
    }
    output.push_str(") {\n");

    // Generate then branch
    generator.indent_level += 1;

    // Emit the then-branch commands.  The surrounding `if (cond) { ... }` already
    // provides a Perl block, so we must NOT emit an extra bare block wrapper here.
    // An extra `{ }` would make `last`/`next` inside the branch exit the bare block
    // instead of the intended enclosing loop.
    match &*if_stmt.then_branch {
        Command::Block(block) => {
            output.push_str(&generator.generate_block_commands(block));
        }
        _ => {
            output.push_str(&generator.indent());
            output.push_str(&generator.generate_command(&if_stmt.then_branch));
            // Ensure a newline after the command so that the closing `}`
            // does not end up on the same line (which confuses perlcritic
            // and makes subsequent `my` declarations appear conditional).
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }
    }

    generator.indent_level -= 1;

    // Generate else branch if present
    if let Some(else_branch) = &if_stmt.else_branch {
        output.push_str("}\n");
        output.push_str(&generator.indent());
        output.push_str("else {\n");
        generator.indent_level += 1;

        // Same: don't add an extra bare-block wrapper around the else body.
        match &**else_branch {
            Command::Block(block) => {
                output.push_str(&generator.generate_block_commands(block));
            }
            _ => {
                output.push_str(&generator.indent());
                output.push_str(&generator.generate_command(else_branch));
                // Ensure a newline after the command so that the closing `}`
                // does not end up on the same line.
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
        }

        generator.indent_level -= 1;
    }

    output.push_str(&generator.indent());
    output.push_str("}\n");

    output
}

pub fn generate_case_statement_impl(
    generator: &mut Generator,
    case_stmt: &CaseStatement,
) -> String {
    let mut output = String::new();

    // Pre-declare variables assigned in any case body, so that
    // `my $var = ...` does not end up inside the if/elsif block.
    {
        let mut case_vars = std::collections::HashSet::new();
        for case in &case_stmt.cases {
            for cmd in &case.body {
                collect_assigned_vars(cmd, &mut case_vars);
            }
        }
        hoist_my_declarations(generator, &case_vars, &mut output);
    }

    // Convert bash case statement to Perl if/elsif/else
    let mut first_case = true;

    for case_clause in &case_stmt.cases {
        if first_case {
            // First case becomes 'if'
            output.push_str("if (");
            first_case = false;
        } else {
            // Subsequent cases become 'elsif'
            output.push_str(&generator.indent());
            output.push_str("} elsif (");
        }

        // Handle multiple patterns in a single case clause
        let mut pattern_conditions = Vec::new();
        for pattern in &case_clause.patterns {
            // Get the raw pattern string, stripping any surrounding shell quotes.
            // `perl_string_literal` wraps the content in Perl string delimiters which
            // then pollute the regex; `strip_shell_quotes_for_regex` gives us the
            // bare content we want.
            let pattern_str = generator.strip_shell_quotes_for_regex(pattern);
            if pattern_str == "*" {
                // Default case (*)
                pattern_conditions.push("1".to_string()); // Always true
            } else {
                // Check whether this is a simple literal pattern (no glob characters).
                // If so, use `eq` instead of a regex match — it's cleaner and avoids
                // the `msx` flags that are unnecessary for plain string equality.
                let has_glob = pattern_str.contains('*') || pattern_str.contains('?') || pattern_str.contains('[') || pattern_str.contains(']');

                let word_str = generator.word_to_perl(&case_stmt.word);

                // Handle positional parameters in case statements
                let processed_word = if word_str.contains("$1")
                    || word_str.contains("$2")
                    || word_str.contains("$3")
                {
                    word_str
                        .replace("$1", "$arg1")
                        .replace("$2", "$arg2")
                        .replace("$3", "$arg3")
                } else if word_str.contains("$name") {
                    word_str.replace("$name", "$arg1")
                } else {
                    word_str
                };

                if has_glob {
                    // Convert bash glob patterns to Perl regex
                    let mut perl_pattern = pattern_str.to_string();
                    perl_pattern = perl_pattern.replace("*", ".*");
                    perl_pattern = perl_pattern.replace("?", ".");
                    perl_pattern = perl_pattern.replace("[", "\\[");
                    perl_pattern = perl_pattern.replace("]", "\\]");
                    let clean_pattern = perl_pattern.trim_matches('"').trim_matches('\'');
                    let regex_pattern = format!("^{}$", clean_pattern);
                    pattern_conditions.push(format!("{} =~ /{}/msx", processed_word, regex_pattern));
                } else {
                    // Simple literal — use eq for clarity and performance.
                    // Quote the pattern for Perl: wrap in single quotes (escape embedded quotes).
                    let quoted_pattern = format!("'{}'", pattern_str.replace("\\", "\\\\").replace("'", "\\'"));
                    pattern_conditions.push(format!("{} eq {}", processed_word, quoted_pattern));
                }
            }
        }

        // Join multiple patterns with 'or'
        output.push_str(&pattern_conditions.join(" or "));
        output.push_str(") {\n");

        generator.indent_level += 1;
        // Generate body commands
        for command in &case_clause.body {
            output.push_str(&generator.indent());
            output.push_str(&generator.generate_command(command));
        }
        generator.indent_level -= 1;
    }

    // Close the if/elsif chain
    output.push_str(&generator.indent());
    output.push_str("}\n");

    output
}

pub fn generate_while_loop_impl(generator: &mut Generator, while_loop: &WhileLoop) -> String {
    let mut output = String::new();

    // Pre-declare variables assigned inside the loop body so they are
    // not declared with `my` inside the while/until block.
    {
        let mut body_vars = std::collections::HashSet::new();
        for cmd in &while_loop.body.commands {
            collect_assigned_vars(cmd, &mut body_vars);
        }
        hoist_my_declarations(generator, &body_vars, &mut output);
    }

    let loop_keyword = if while_loop.is_until { "until" } else { "while" };

    // Check if the while loop condition uses variables that might need initialization
    // This is needed for shell compatibility where loop variables persist
    let mut read_vars: Vec<String> = extract_read_vars_from_condition(&while_loop.condition);
    // Also check in initial simple condition (for direct `while read` case)
    if let Command::Simple(cmd) = &*while_loop.condition {
        if let Word::Literal(name, _) = &cmd.name {
            if name == "read" {
                // Extract variable names from read command args (skip flags)
                for arg in &cmd.args {
                    if let Word::Literal(s, _) = arg {
                        if s != "-r" && s != "-p" && s != "-n" && s != "-t" && !s.starts_with('-') {
                            if !read_vars.contains(s) {
                                read_vars.push(s.clone());
                            }
                        }
                    }
                }
            }
        }
        if cmd.name == "[" || cmd.name == "test" {
            // For test commands, check if variables need initialization
            if cmd.args.len() >= 3 {
                // Check both operands for variables that need initialization
                let operand1 = &cmd.args[0];
                let operand2 = &cmd.args[2];

                // Initialize first operand if it's a variable
                if let Word::Variable(var_name, _, _) = operand1 {
                    if !generator.declared_locals.contains(var_name) {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = 0;\n", var_name));
                        generator.declared_locals.insert(var_name.to_string());
                    }
                    // Mark this variable as used at function level so for loops know to preserve it
                    generator.function_level_vars.insert(var_name.to_string());
                }

                // Initialize second operand if it's a variable
                if let Word::Variable(var_name, _, _) = operand2 {
                    if !generator.declared_locals.contains(var_name) {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = 0;\n", var_name));
                        generator.declared_locals.insert(var_name.to_string());
                    }
                    // Mark this variable as used at function level so for loops know to preserve it
                    generator.function_level_vars.insert(var_name.to_string());
                }
            }
        }
    } else if let Command::TestExpression(test_expr) = &*while_loop.condition {
        // For test expressions, check if variables are used in the expression
        // and mark them as function-level variables so for loops know to preserve them
        // Extract variable names from the test expression
        let re = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        for cap in re.captures_iter(&test_expr.expression) {
            if let Some(var_name) = cap.get(1) {
                generator
                    .function_level_vars
                    .insert(var_name.as_str().to_string());
            }
        }
    }

    // Declare read variables before the loop (for all condition types)
    for var in &read_vars {
        if !generator.declared_locals.contains(var) {
            output.push_str(&generator.indent());
            output.push_str(&format!("my ${};\n", var));
            generator.declared_locals.insert(var.clone());
        }
    }

    // Generate while loop
    // Handle 'while read -r var1 var2 ...' specially to avoid multiple statements in condition
    if !read_vars.is_empty() {
        if let Command::Simple(cmd) = &*while_loop.condition {
            let ifs_sep = if let Some(Word::Literal(sep, _)) = cmd.env_vars.get("IFS") {
                format!("{}", regex::escape(sep))
            } else {
                r"\s+".to_string()
            };
            output.push_str(&format!("{} ( my $L = <> ) {{\n", loop_keyword));
            output.push_str(&format!("    chomp $L;\n"));
            output.push_str(&format!("    my @_fields = split /{}/msx, $L;\n", ifs_sep));
            for (i, var) in read_vars.iter().enumerate() {
                output.push_str(&format!("    ${} = $_fields[{}] // q{{}};\n", var, i));
            }
            generator.indent_level += 1;
            output.push_str(&generator.generate_block_commands(&while_loop.body));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            return output;
        }
    }

    // Handle And/Or conditions specially: generate a while (1) loop
    // with explicit condition checks via last unless/last if.
    match &*while_loop.condition {
        Command::And(_, _) | Command::Or(_, _) => {
            output.push_str(&format!("{} (1) {{\n", loop_keyword));
            generator.indent_level += 1;
            // Flatten the And/Or tree and generate each condition as a last check
            let mut conds = Vec::new();
            let is_and = matches!(&*while_loop.condition, Command::And(_, _));
            flatten_conditions(&while_loop.condition, &mut conds);
            for cond in &conds {
                // Test expressions generate a boolean expression directly
                // (e.g., "$line" ne q{}). Other commands generate code that
                // sets $CHILD_ERROR.
                if matches!(cond, Command::TestExpression(_)) {
                    generator.suppress_set_e_depth += 1;
                    let cond_code = generator.generate_command(cond);
                    generator.suppress_set_e_depth -= 1;
                    let cond_code = cond_code.trim().to_string();
                    if is_and {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("last unless ({});\n", cond_code));
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("last if ({});\n", cond_code));
                    }
                } else {
                    generator.suppress_set_e_depth += 1;
                    let cond_code = generator.generate_command(cond);
                    generator.suppress_set_e_depth -= 1;
                    output.push_str(&generator.indent());
                    if is_and {
                        output.push_str("last unless do {\n");
                    } else {
                        output.push_str("last if do {\n");
                    }
                    generator.indent_level += 1;
                    for line in cond_code.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            output.push_str(&generator.indent());
                            output.push_str(trimmed);
                            output.push('\n');
                        }
                    }
                    output.push_str(&generator.indent());
                    output.push_str("$CHILD_ERROR == 0\n");
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("};\n");
                }
            }
            // Generate body
            output.push_str(&generator.generate_block_commands(&while_loop.body));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
        Command::Block(block) => {
            // Block conditions arise when env vars are assigned before a command
            // (e.g. `IFS= read -r line && ...`). Generate each command in the block
            // as a step in a while (1) loop, checking exit code after each.
            // NOTE: Always use `while (1)` — using `until (1)` would never execute
            // the body because `until` runs while the condition is false.
            output.push_str("while (1) {\n");
            generator.indent_level += 1;
            // Generate all commands except the last one as plain statements
            let is_until = while_loop.is_until;
            let len = block.commands.len();
            for (i, cmd) in block.commands.iter().enumerate() {
                if i < len - 1 {
                    // Non-last commands: execute and check exit code
                    generator.suppress_set_e_depth += 1;
                    let cmd_code = generator.generate_command(cmd);
                    generator.suppress_set_e_depth -= 1;
                    for line in cmd_code.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            output.push_str(&generator.indent());
                            output.push_str(trimmed);
                            output.push('\n');
                        }
                    }
                    output.push_str(&generator.indent());
                    if is_until {
                        // For `until`, exit the loop when a command succeeds
                        output.push_str("last if $CHILD_ERROR == 0;\n");
                    } else {
                        output.push_str("last unless $CHILD_ERROR == 0;\n");
                    }
                } else {
                    // Last command: treat as the main condition
                    match cmd {
                        Command::And(_, _) | Command::Or(_, _) => {
                            // Flatten and generate each condition as a last check
                            let mut conds = Vec::new();
                            let is_and = matches!(cmd, Command::And(_, _));
                            flatten_conditions(cmd, &mut conds);
                            for cond in &conds {
                                if matches!(cond, Command::TestExpression(_)) {
                                    generator.suppress_set_e_depth += 1;
                                    let cond_code = generator.generate_command(cond);
                                    generator.suppress_set_e_depth -= 1;
                                    let cond_code = cond_code.trim().to_string();
                                    if is_and {
                                        // For AND: exit unless all succeed
                                        if is_until {
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("last if ({});\n", cond_code));
                                        } else {
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("last unless ({});\n", cond_code));
                                        }
                                    } else {
                                        // For OR: exit if any succeeds
                                        if is_until {
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("last unless ({});\n", cond_code));
                                        } else {
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("last if ({});\n", cond_code));
                                        }
                                    }
                                } else {
                                    generator.suppress_set_e_depth += 1;
                                    let cond_code = generator.generate_command(cond);
                                    generator.suppress_set_e_depth -= 1;
                                    output.push_str(&generator.indent());
                                    if is_and {
                                        if is_until {
                                            output.push_str("last if do {\n");
                                        } else {
                                            output.push_str("last unless do {\n");
                                        }
                                    } else {
                                        if is_until {
                                            output.push_str("last unless do {\n");
                                        } else {
                                            output.push_str("last if do {\n");
                                        }
                                    }
                                    generator.indent_level += 1;
                                    for line in cond_code.lines() {
                                        let trimmed = line.trim();
                                        if !trimmed.is_empty() {
                                            output.push_str(&generator.indent());
                                            output.push_str(trimmed);
                                            output.push('\n');
                                        }
                                    }
                                    output.push_str(&generator.indent());
                                    output.push_str("$CHILD_ERROR == 0\n");
                                    generator.indent_level -= 1;
                                    output.push_str(&generator.indent());
                                    output.push_str("};\n");
                                }
                            }
                        }
                        _ => {
                            // Simple condition: wrap in do {} and check
                            generator.suppress_set_e_depth += 1;
                            let cond_code = generator.generate_command(cmd);
                            generator.suppress_set_e_depth -= 1;
                            output.push_str(&generator.indent());
                            if is_until {
                                output.push_str("last if do {\n");
                            } else {
                                output.push_str("last unless do {\n");
                            }
                            generator.indent_level += 1;
                            for line in cond_code.lines() {
                                let trimmed = line.trim();
                                if !trimmed.is_empty() {
                                    output.push_str(&generator.indent());
                                    output.push_str(trimmed);
                                    output.push('\n');
                                }
                            }
                            output.push_str(&generator.indent());
                            output.push_str("$CHILD_ERROR == 0\n");
                            generator.indent_level -= 1;
                            output.push_str(&generator.indent());
                            output.push_str("};\n");
                        }
                    }
                }
            }
            // Generate body
            output.push_str(&generator.generate_block_commands(&while_loop.body));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
        Command::Simple(cmd) if cmd.name == "[" || cmd.name == "test" => {
            output.push_str(&format!("{} ( ", loop_keyword));
            generator.generate_test_command(cmd, &mut output);
            output.push_str(" ) {\n");
            // Generate body
            generator.indent_level += 1;
            output.push_str(&generator.generate_block_commands(&while_loop.body));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
        Command::TestExpression(test_expr) => {
            output.push_str(&format!("{} ( ", loop_keyword));
            let test_result = generator.generate_test_expression(test_expr);
            // Remove outer parentheses if present to avoid double parentheses
            if test_result.starts_with('(') && test_result.ends_with(')') {
                output.push_str(&test_result[1..test_result.len() - 1]);
            } else {
                output.push_str(&test_result);
            }
            output.push_str(" ) {\n");
            // Generate body
            generator.indent_level += 1;
            output.push_str(&generator.generate_block_commands(&while_loop.body));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
        _ => {
            // For `let` commands, the generated code now produces
            // $CHILD_ERROR = ($main_exit_code = ...) ? 0 : 1 which follows
            // bash exit-code convention (0 = success, 1 = failure).
            // The while loop needs the Perl-truthy value (non-zero = enter),
            // so we negate: while (!(...)) { ... }.
            let is_let = matches!(
                &*while_loop.condition,
                Command::Simple(cmd) if cmd.name == "let"
            );
            output.push_str(&format!("{} ( ", loop_keyword));
            generator.suppress_set_e_depth += 1;
            let mut cond = generator.generate_command(&while_loop.condition);
            generator.suppress_set_e_depth -= 1;
            let cond = cond
                .trim_end_matches(|c: char| c == ';' || c == '\n' || c == ' ' || c == '\t')
                .to_string();
            if is_let {
                output.push_str(&format!("!({})", cond));
            } else {
                output.push_str(&cond);
            }
            output.push_str(" ) {\n");
            // Generate body
            generator.indent_level += 1;
            output.push_str(&generator.generate_block_commands(&while_loop.body));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
    }

    output
}

// Helper function to analyze if a variable is used after a for loop
fn is_variable_used_after_for_loop(
    commands: &[Command],
    for_loop_var: &str,
    for_loop_index: usize,
) -> bool {
    for (i, command) in commands.iter().enumerate() {
        if i <= for_loop_index {
            continue; // Skip commands before and including the for loop
        }

        match command {
            Command::While(while_loop) => {
                // Check if variable is used in while loop condition
                if let Command::TestExpression(test_expr) = &*while_loop.condition {
                    if test_expr.expression.contains(&format!("${}", for_loop_var)) {
                        return true;
                    }
                }
            }
            Command::Simple(cmd) => {
                // Check if variable is used in simple commands
                for arg in &cmd.args {
                    if let Word::Variable(var_name, _, _) = arg {
                        if var_name == for_loop_var {
                            return true;
                        }
                    }
                }
            }
            _ => {
                // For other command types, we could add more analysis here
            }
        }
    }
    false
}

pub fn generate_cstyle_for_loop_impl(
    generator: &mut Generator,
    for_loop: &CStyleForLoop,
) -> String {
    let mut output = String::new();

    // Pre-declare variables assigned inside the loop body.
    {
        let mut body_vars = std::collections::HashSet::new();
        for cmd in &for_loop.body.commands {
            collect_assigned_vars(cmd, &mut body_vars);
        }
        hoist_my_declarations(generator, &body_vars, &mut output);
    }

    // Parse "init; cond; incr" from arith_content
    let parts: Vec<&str> = for_loop.arith_content.splitn(3, ';').collect();
    let init_raw = parts.first().map(|s| s.trim()).unwrap_or("");
    let cond_raw = parts.get(1).map(|s| s.trim()).unwrap_or("");
    let incr_raw = parts.get(2).map(|s| s.trim()).unwrap_or("");

    let init_perl = if init_raw.is_empty() {
        String::new()
    } else {
        generator.convert_arithmetic_to_perl(init_raw)
    };
    let cond_perl = if cond_raw.is_empty() {
        "1".to_string()
    } else {
        generator.convert_arithmetic_to_perl(cond_raw)
    };
    let incr_perl = if incr_raw.is_empty() {
        String::new()
    } else {
        generator.convert_arithmetic_to_perl(incr_raw)
    };

    // Convert shell comparison operators to Perl
    let cond_perl = cond_perl
        .replace("<=", "<=")
        .replace(">=", ">=")
        .replace("!=", "!=")
        .replace("==", "==");

    // Strip the outer eval { int(EXPR) } // "" wrapper from each component
    // because PPI cannot correctly parse eval { } blocks inside a for-loop
    // condition (it confuses the eval-block with the for-loop body).
    // The for-loop init/cond/incr are typically simple assignments and
    // comparisons that do not need the eval-wrapping (which was only added
    // to catch division-by-zero errors in general arithmetic expressions).
    let strip_eval_wrapper = |s: &str| -> String {
        let trimmed = s.trim();
        if trimmed.starts_with("eval { int(") && trimmed.ends_with("} // \"\"") {
            let inner = &trimmed["eval { int(".len()..trimmed.len() - "} // \"\"".len()];
            inner.to_string()
        } else {
            trimmed.to_string()
        }
    };
    let init_clean = strip_eval_wrapper(&init_perl);
    let cond_clean = strip_eval_wrapper(&cond_perl);
    let incr_clean = strip_eval_wrapper(&incr_perl);

    output.push_str(&generator.indent());
    output.push_str(&format!("for ({init_clean}; {cond_clean}; {incr_clean}) {{\n"));

    generator.indent_level += 1;
    let body_output = generator.generate_block(&for_loop.body);
    generator.indent_level -= 1;

    output.push_str(&body_output);
    output.push_str(&generator.indent());
    output.push_str("}\n");

    output
}

pub fn generate_for_loop_impl(generator: &mut Generator, for_loop: &ForLoop) -> String {
    let mut output = String::new();

    // Pre-declare variables assigned inside the loop body so they are
    // not declared with `my` inside the for block.
    {
        let mut body_vars = std::collections::HashSet::new();
        for cmd in &for_loop.body.commands {
            collect_assigned_vars(cmd, &mut body_vars);
        }
        hoist_my_declarations(generator, &body_vars, &mut output);
    }

    // The loop variable is declared by `for my $i (...)` — an outer `my $i;`
    // before the loop is redundant because `for my $i` creates a new lexical
    // variable scoped to the loop body.  If the variable is needed after the
    // loop (shell-compatibility persistence), the pre-analysis pass has
    // already added it to `function_level_vars` which declares it in the
    // program preamble.  In either case, the outer declaration here is dead
    // code.
    //
    // Pre-existing declaration is tracked so that post-loop assignments
    // (see below) don't violate strict.
    let loop_var = &for_loop.variable;
    if !generator.declared_locals.contains(loop_var)
        && !generator.function_level_vars.contains(loop_var)
    {
        // Variable is not declared anywhere — no need to insert a dead
        // `my $i;` because `for my $i` declares it lexically.
        // Just mark it as declared so post-loop code knows it exists.
        generator.declared_locals.insert(loop_var.clone());
    }

    // Generate for loop using IR nodes for simple cases, falling back to
    // string formatting for complex cases.
    //
    // Check if this is a simple numeric range for loop (e.g. `for i in {1..5}`).
    // If so, emit an `IrStmt::For` with `IrExpr::Range` so the IR backend
    // controls formatting (clean spacing, no magic-number constants, etc.).
    //
    // For complex cases (string lists, brace expansions with prefixes/suffixes,
    // array variables, etc.) we fall back to the existing string-based approach.
    let is_simple_range = for_loop.items.len() == 1
        && matches!(&for_loop.items[0], Word::BraceExpansion(e, _) if e.items.len() == 1 && matches!(&e.items[0], BraceItem::Range(_)) && e.prefix.is_none() && e.suffix.is_none());

    if is_simple_range {
        // Build IrStmt::For with IrExpr::Range, then format via stmt_to_perl.
        if let Word::BraceExpansion(expansion, _) = &for_loop.items[0] {
            if let BraceItem::Range(range) = &expansion.items[0] {
                if let (Ok(start_num), Ok(end_num)) =
                    (range.start.parse::<i64>(), range.end.parse::<i64>())
                {
                    let step = range
                        .step
                        .as_ref()
                        .and_then(|s| s.parse::<i64>().ok())
                        .unwrap_or(1);
                    if step == 1 {
                        // Build the body as RawText.  The body generator still
                        // produces string output; it uses generator.indent_level
                        // for indentation, so bump it first.
                        generator.indent_level += 1;
                        let body_str = generator.generate_block_commands(&for_loop.body);
                        generator.indent_level -= 1;

                        let ir_for = crate::ir::IrStmt::For {
                            var: for_loop.variable.clone(),
                            iter: crate::ir::IrExpr::Range { start: start_num, end: end_num },
                            body: vec![crate::ir::IrStmt::RawText(body_str)],
                        };
                        output.push_str(&crate::ir::stmt_to_perl(&ir_for, generator.indent_level));

                        // Post-loop persistence: if the variable is used after the loop,
                        // assign it the final value (shell-compatibility).
                        if generator.function_level_vars.contains(&for_loop.variable) {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("${} = {};\n", for_loop.variable, end_num));
                        }

                        return output;
                    }
                }
            }
        }
    }

    // Fallback: string-based approach for complex cases
    output.push_str(&generator.indent());
    output.push_str(&format!("for my ${} (", for_loop.variable));

    // Handle different types of for loop items
    let mut all_items = Vec::new();

    for word in &for_loop.items {
        match word {
            Word::StringInterpolation(interp, _) => {
                // Check if this is just a single array variable like "$@" or "$*"
                if interp.parts.len() == 1 {
                    if let StringPart::Variable(var) = &interp.parts[0] {
                        match var.as_str() {
                            "@" => all_items.push("@ARGV".to_string()), // $@ -> @ARGV (no quotes)
                            "*" => all_items.push("@ARGV".to_string()), // $* -> @ARGV (no quotes)
                            _ => all_items.push(generator.word_to_perl(word)),
                        }
                    } else if let StringPart::ParameterExpansion(pe) = &interp.parts[0] {
                        // Handle ${arr[@]} -> @arr for array iteration or ${!map[@]} -> keys %map for map keys
                        if pe.operator
                            == ParameterExpansionOperator::ArraySlice("@".to_string(), None)
                        {
                            if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                all_items.push(format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> @arr (array iteration)
                                all_items.push(format!("@{}", pe.variable));
                            }
                        } else {
                            all_items.push(generator.word_to_perl(word));
                        }
                    } else {
                        all_items.push(generator.word_to_perl(word));
                    }
                } else {
                    all_items.push(generator.word_to_perl(word));
                }
            }
            Word::BraceExpansion(expansion, _) => {
                // Handle brace expansion directly
                if expansion.items.len() == 1 {
                    match &expansion.items[0] {
                        BraceItem::Range(range) => {
                            // Convert {1..5} to Perl range syntax (1..5)
                            if let (Ok(start_num), Ok(end_num)) =
                                (range.start.parse::<i64>(), range.end.parse::<i64>())
                            {
                                let step = range
                                    .step
                                    .as_ref()
                                    .and_then(|s| s.parse::<i64>().ok())
                                    .unwrap_or(1);
                                if step == 1 {
                                    // Simple range: 1..5
                                    if end_num > 2 && !generator.no_magic_numbers {
                                        // Use constant for magic numbers > 2
                                        let const_name = format!("$MAX_LOOP_{}", end_num);
                                        all_items
                                            .push(format!(" {} .. {} ", start_num, const_name));
                                    } else {
                                        all_items.push(format!("{}..{}", start_num, end_num));
                                    }
                                } else {
                                    // Step range: use list with step
                                    let mut values = Vec::new();
                                    let mut current = start_num;
                                    if step > 0 {
                                        while current <= end_num {
                                            values.push(current.to_string());
                                            current += step;
                                        }
                                    } else {
                                        while current >= end_num {
                                            values.push(current.to_string());
                                            current += step;
                                        }
                                    }
                                    all_items.push(format!("({})", values.join(", ")));
                                }
                            } else {
                                // Fallback for non-numeric ranges
                                all_items.push(generator.word_to_perl(word));
                            }
                        }
                        BraceItem::Literal(s) => {
                            // Single literal item, include prefix/suffix
                            let val = format!("{}{}{}",
                                expansion.prefix.as_deref().unwrap_or(""),
                                s,
                                expansion.suffix.as_deref().unwrap_or(""));
                            // If the value contains glob metacharacters (*, ?, [), use glob() with sort and fallback
                            if val.contains('*') || val.contains('?') || val.contains('[') {
                                all_items.push(format!("do {{ my @_g = sort glob(\"{}\"); @_g ? @_g : (\"{}\") }}", val, val));
                            } else {
                                all_items.push(format!("\"{}\"", val));
                            }
                        }
                        BraceItem::Sequence(seq) => {
                            // Convert {a,b,c} to separate quoted items, include prefix/suffix
                            for item in seq {
                                let val = format!("{}{}{}",
                                    expansion.prefix.as_deref().unwrap_or(""),
                                    item,
                                    expansion.suffix.as_deref().unwrap_or(""));
                                // If the value contains glob metacharacters (*, ?, [), use glob() with sort and fallback
                                if val.contains('*') || val.contains('?') || val.contains('[') {
                                    all_items.push(format!("do {{ my @_g = sort glob(\"{}\"); @_g ? @_g : (\"{}\") }}", val, val));
                                } else {
                                    all_items.push(format!("\"{}\"", val));
                                }
                            }
                        }
                        BraceItem::Nested(_) => todo!(),
                        BraceItem::Compound(_) => todo!(),
                    }
                } else {
                    // Multiple brace items - expand each one
                    for item in &expansion.items {
                        match item {
                            BraceItem::Literal(s) => {
                                let val = format!("{}{}{}",
                                    expansion.prefix.as_deref().unwrap_or(""),
                                    s,
                                    expansion.suffix.as_deref().unwrap_or(""));
                                // If the value contains glob metacharacters (*, ?, [), use glob() with sort and fallback
                                if val.contains('*') || val.contains('?') || val.contains('[') {
                                    all_items.push(format!("do {{ my @_g = sort glob(\"{}\"); @_g ? @_g : (\"{}\") }}", val, val));
                                } else {
                                    all_items.push(format!("\"{}\"", val));
                                }
                            },
                            BraceItem::Range(range) => {
                                if let (Ok(start_num), Ok(end_num)) =
                                    (range.start.parse::<i64>(), range.end.parse::<i64>())
                                {
                                    let step = range
                                        .step
                                        .as_ref()
                                        .and_then(|s| s.parse::<i64>().ok())
                                        .unwrap_or(1);
                                    if step == 1 {
                                        all_items.push(format!("{}..{}", start_num, end_num));
                                    } else {
                                        let mut values = Vec::new();
                                        let mut current = start_num;
                                        if step > 0 {
                                            while current <= end_num {
                                                values.push(current.to_string());
                                                current += step;
                                            }
                                        } else {
                                            while current >= end_num {
                                                values.push(current.to_string());
                                                current += step;
                                            }
                                        }
                                        all_items.push(format!("({})", values.join(", ")));
                                    }
                                } else {
                                    all_items.push(format!("\"{}\"", range.start));
                                }
                            }
                            BraceItem::Sequence(seq) => {
                                for item in seq {
                                    let val = format!("{}{}{}",
                                        expansion.prefix.as_deref().unwrap_or(""),
                                        item,
                                        expansion.suffix.as_deref().unwrap_or(""));
                                    if val.contains('*') || val.contains('?') || val.contains('[') {
                                        all_items.push(format!("do {{ my @_g = sort glob(\"{}\"); @_g ? @_g : (\"{}\") }}", val, val));
                                    } else {
                                        all_items.push(format!("\"{}\"", val));
                                    }
                                }
                            }
                            BraceItem::Nested(_) => todo!(),
                            BraceItem::Compound(_) => todo!(),
                        }
                    }
                }
            }
            Word::Literal(s, _) => {
                // Check if this literal contains space-separated values (likely from brace expansion)
                if s.contains(' ')
                    && s.chars()
                        .all(|c| c.is_ascii_digit() || c.is_ascii_whitespace())
                {
                    // Split by whitespace and add each item separately
                    let items: Vec<String> = s
                        .split_whitespace()
                        .map(|item| format!("\"{}\"", item))
                        .collect();
                    all_items.extend(items);
                } else {
                    all_items.push(generator.word_to_perl(word));
                }
            }
            _ => all_items.push(generator.word_to_perl(word)),
        }
    }

    let items_str = all_items.join(", ");
    output.push_str(&items_str);
    output.push_str(") {\n");

    // Generate body
    generator.indent_level += 1;
    output.push_str(&generator.generate_block_commands(&for_loop.body));
    generator.indent_level -= 1;

    output.push_str(&generator.indent());
    output.push_str("}\n");

    // After the loop, set the variable to the last value to mimic shell behavior
    // But only if the variable is used later (to avoid unnecessary assignments)
    // This is important for shell compatibility where loop variables retain their last value
    // However, we should only do this if the variable is actually used after the loop
    if generator.function_level_vars.contains(&for_loop.variable) && !all_items.is_empty() {
        // For simple ranges like 1..3, the last value is 3
        if all_items.len() == 1 && items_str.contains("..") {
            // This is a range like "1..3"
            let range_parts: Vec<&str> = items_str.split("..").collect();
            if range_parts.len() == 2 {
                let end_value = range_parts[1].trim();
                // Don't use constants in post-loop assignments, use the actual value
                let actual_end_value =
                    if end_value.starts_with('$') && end_value.contains("MAX_LOOP_") {
                        // Extract the number from the constant name (e.g., $MAX_LOOP_5 -> 5)
                        if let Some(num_str) = end_value.strip_prefix("$MAX_LOOP_") {
                            num_str.to_string()
                        } else {
                            end_value.to_string()
                        }
                    } else {
                        end_value.to_string()
                    };
                output.push_str(&generator.indent());
                output.push_str(&format!("${} = {};\n", for_loop.variable, actual_end_value));
            }
        } else if all_items.len() > 1 {
            // For multiple items, set to the last item
            if let Some(last_item) = all_items.last() {
                output.push_str(&generator.indent());
                output.push_str(&format!("${} = {};\n", for_loop.variable, last_item));
            }
        }
    }

    output
}

pub fn generate_function_impl(generator: &mut Generator, func: &Function) -> String {
    let mut output = String::new();

    // Build parameter-name map by scanning for `name=$N` assignments.
    // e.g. `x=$1; y=$2` → {1: "x", 2: "y"}
    let param_map = build_param_name_map(&func.body);
    if !param_map.is_empty() || !func.parameters.is_empty() {
        generator.fn_param_names.insert(func.name.clone(), param_map.clone());
    }

    // Determine if this function is nested inside another function
    let is_nested = generator.fn_nesting_depth > 0;

    if is_nested {
        // For nested functions, emit a lexical anonymous sub assigned to a
        // variable instead of a named sub, to avoid Perl::Critic's
        // "Nested named subroutine" violation.
        output.push_str("\n");
        output.push_str(&generator.indent());
        output.push_str(&format!("my ${} = sub {{\n", func.name));
        generator.indent_level += 1;

        // Mark this function as lexical so call sites use $name->(...)
        generator.lexical_functions.insert(func.name.clone());
    } else {
        // Add blank line before function definition for better formatting
        output.push_str("\n");

        // Check if function uses positional parameters ($1, $2, etc.) in its body
        let uses_positional_params = check_function_uses_positional_params(&func.body);

        if generator.use_function_signatures {
            // Use modern function signatures
            if !func.parameters.is_empty() {
                // Function has declared parameters
                let params: Vec<String> = func
                    .parameters
                    .iter()
                    .map(|param| format!("${}", param))
                    .collect();
                output.push_str(&format!("sub {}({}) {{\n", func.name, params.join(", ")));
            } else if uses_positional_params && param_map.is_empty() {
                // Function uses $1, $2, etc. but has no declared parameters AND
                // no named-param assignment pattern (name=$1).  Emit the old-style
                // positional unpacking as a fallback.
                output.push_str(&format!("sub {} {{\n", func.name));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("my ($file) = @_;\n");
            } else if uses_positional_params && !param_map.is_empty() {
                // Named parameters will be unpacked later via the param-map.
                output.push_str(&format!("sub {} {{\n", func.name));
                generator.indent_level += 1;
            } else {
                // No parameters
                output.push_str(&format!("sub {} {{\n", func.name));
                generator.indent_level += 1;
            }
        } else {
            // Use traditional @_ unpacking approach
            output.push_str(&format!("sub {} {{\n", func.name));
            generator.indent_level += 1;

            // Handle function parameters - always unpack @_ first
            if !func.parameters.is_empty() {
                output.push_str(&generator.indent());
                output.push_str("my (");
                let params: Vec<String> = func
                    .parameters
                    .iter()
                    .map(|param| format!("${}", param))
                    .collect();
                output.push_str(&params.join(", "));
                output.push_str(") = @_;\n");
            } else if uses_positional_params {
                // Function uses $1, $2, etc. but has no declared parameters
                // Check if the function body already has local commands that handle parameters
                let has_local_commands = func
                    .body
                    .commands
                    .iter()
                    .any(|cmd| matches!(cmd, Command::BuiltinCommand(cmd) if cmd.name == "local"));

                if !has_local_commands {
                    // Generate parameter unpacking for the first parameter using proper @_ unpacking
                    output.push_str(&generator.indent());
                    output.push_str("my ($file) = @_;\n");
                }
            } else {
                // Even if no parameters, unpack @_ to satisfy Perl::Critic
                // Note: @_ is a special variable and cannot be redeclared, so we don't need to do anything
            }
        }
    }

    // Generate function body
    // DEBUG: eprintln!("DEBUG: Generating function body for {}", func.name);
    // DEBUG: eprintln!("DEBUG: Function body commands: {:?}", func.body.commands);

    // Use all commands from the function body. The `local` commands
    // (e.g. `local file=$1`) are handled by redirects.rs which generates
    // proper `my $var = $_[0];` declarations.
    let filtered_commands = func.body.commands.clone();

    // Save the current output length so we can measure the body's brace balance.
    let saved_output_len = output.len();

    // Save declared_locals, function_level_vars and associative_arrays so that
    // variables declared inside the function (via `local` etc.) do not leak into
    // the outer scope.
    let saved_declared_locals = generator.declared_locals.clone();
    let saved_function_level_vars = generator.function_level_vars.clone();
    let saved_associative_arrays = generator.associative_arrays.clone();

    // Increment nesting depth so any further nested function definitions
    // are also treated as lexical.
    generator.fn_nesting_depth += 1;

    // If we have named parameters, mark them as already-declared so the
    // body generator skips `my` (we'll emit the declaration ourselves).
    for (_idx, pname) in &param_map {
        generator.declared_locals.insert(pname.clone());
    }

    // Create a temporary block with filtered commands
    let filtered_block = Block {
        commands: filtered_commands,
    };
    let mut body_code = generator.generate_block_commands(&filtered_block);

    // Post-process the body: replace $N → $_[N-1] for positional params.
    // This must be done AFTER generating, because the generator emits $1 from
    // Word::Variable("1", ...) which currently produces perl `$1` (regex capture).
    // We fix it here so function args resolve to @_ instead.
    // Apply the fix whenever the function uses ANY positional params, regardless
    // of whether a param-name map (from name=$1 assignments) was built.
    let uses_pos = check_function_uses_positional_params(&func.body);
    if uses_pos {
        // Replace $1 through $9 with $_[0] through $_[8]
        for i in 1..=9 {
            let old_ref = format!("${}", i);
            let new_ref = format!("$_[{}]", i - 1);
            body_code = body_code.replace(&old_ref, &new_ref);
        }
    }

    // If we have named parameters, prepend a clean unpacking line and
    // remove the now-redundant `my $name = $_[N-1];` declarations.
    if !param_map.is_empty() {
        // Sort by index so params appear in order: ($x, $y)
        let mut sorted: Vec<_> = param_map.iter().collect();
        sorted.sort_by_key(|(idx, _)| **idx);
        let params_str = sorted
            .iter()
            .map(|(_, pname)| format!("${}", pname))
            .collect::<Vec<_>>()
            .join(", ");
        // Build the unpacking line
        let unpack_line = format!("    my ({}) = @_;
", params_str);
        output.push_str(&unpack_line);

        // Remove the individual `$x = $_[0];` lines since they're now
        // handled by the unpacking `my ($x, $y) = @_;` above.
        for (idx, pname) in &param_map {
            // The param was marked as declared, so the body emits assignment
            // without `my`: `    $x = $_[0];\n`
            let redundant = format!("    ${} = $_[{}];\n", pname, idx - 1);
            body_code = body_code.replace(&redundant, "");
        }
    }

    output.push_str(&body_code);

    // Balance braces inside the function body so that extra opens from
    // pipeline/command-substitution code do not leak into the outer scope.
    // The closing `}` we emit below only closes the function `sub { ... }`
    // itself; any surplus `{` inside the body would make perlcritic report
    // "Nested named subroutine" for subs defined later.
    {
        let body_text = &output[saved_output_len..];
        let opens = body_text.chars().filter(|&c| c == '{').count();
        let closes = body_text.chars().filter(|&c| c == '}').count();
        for _ in 0..(opens.saturating_sub(closes)) {
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
    }

    // Restore nesting depth
    generator.fn_nesting_depth -= 1;

    // Restore scope state — function-level declarations are scoped.
    generator.declared_locals = saved_declared_locals;
    generator.function_level_vars = saved_function_level_vars;
    generator.associative_arrays = saved_associative_arrays;

    // Add final return statement to satisfy Perl::Critic
    output.push_str(&generator.indent());
    output.push_str("return;\n");

    generator.indent_level -= 1;

    if is_nested {
        // Close the anonymous sub with a semicolon
        output.push_str(&generator.indent());
        output.push_str("};\n");
    } else {
        output.push_str("}\n");
        // Mark function as declared (global)
        generator.declared_functions.insert(func.name.clone());
    }

    output
}

fn check_function_uses_positional_params(block: &Block) -> bool {
    for command in &block.commands {
        if check_command_uses_positional_params(command) {
            return true;
        }
    }
    false
}

fn check_commands_use_positional_params(commands: &[Command]) -> bool {
    for command in commands {
        if check_command_uses_positional_params(command) {
            return true;
        }
    }
    false
}

fn check_command_uses_positional_params(command: &Command) -> bool {
    match command {
        Command::Simple(cmd) => {
            // Check command name and arguments
            if check_word_uses_positional_params(&cmd.name) {
                return true;
            }
            for arg in &cmd.args {
                if check_word_uses_positional_params(arg) {
                    return true;
                }
            }
            false
        }
        Command::BuiltinCommand(cmd) => {
            for arg in &cmd.args {
                if check_word_uses_positional_params(arg) {
                    return true;
                }
            }
            false
        }
        Command::Block(block) => check_function_uses_positional_params(block),
        Command::Pipeline(pipeline) => {
            for cmd in &pipeline.commands {
                if check_command_uses_positional_params(cmd) {
                    return true;
                }
            }
            false
        }
        Command::Function(func) => check_function_uses_positional_params(&func.body),
        Command::If(if_stmt) => {
            check_command_uses_positional_params(&if_stmt.then_branch)
                || if_stmt.else_branch.as_ref().map_or(false, |else_branch| {
                    check_command_uses_positional_params(else_branch)
                })
        }
        Command::Case(case_stmt) => {
            for case_clause in &case_stmt.cases {
                if check_commands_use_positional_params(&case_clause.body) {
                    return true;
                }
            }
            false
        }
        Command::For(for_loop) => check_function_uses_positional_params(&for_loop.body),
        Command::While(while_loop) => check_function_uses_positional_params(&while_loop.body),
        Command::Assignment(assign) => {
            check_word_uses_positional_params(&assign.value)
        }
        Command::Redirect(redir) => check_command_uses_positional_params(&redir.command),
        Command::And(left, right) | Command::Or(left, right) => {
            check_command_uses_positional_params(left)
                || check_command_uses_positional_params(right)
        }
        Command::Subshell(c) | Command::Background(c) | Command::Not(c) => {
            check_command_uses_positional_params(c)
        }
        Command::Return(w) => w.as_ref().map_or(false, |w| check_word_uses_positional_params(w)),
        Command::CStyleFor(c) => check_function_uses_positional_params(&c.body),
        _ => false,
    }
}

fn check_word_uses_positional_params(word: &Word) -> bool {
    match word {
        Word::Literal(s, _) => {
            // Check if the literal contains $1, $2, etc.
            s.contains("$1")
                || s.contains("$2")
                || s.contains("$3")
                || s.contains("$4")
                || s.contains("$5")
                || s.contains("$6")
                || s.contains("$7")
                || s.contains("$8")
                || s.contains("$9")
        }
        Word::StringInterpolation(interp, _) => {
            for part in &interp.parts {
                match part {
                    StringPart::Variable(var) => {
                        if var.chars().all(|c| c.is_digit(10)) {
                            return true;
                        }
                    }
                    StringPart::CommandSubstitution(cmd) => {
                        if check_command_uses_positional_params(cmd) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        }
        Word::CommandSubstitution(cmd, _) => check_command_uses_positional_params(cmd),
        Word::Variable(var, _, _) => {
            // $1, $2, etc. are positional parameter references
            var.parse::<usize>().map_or(false, |n| n >= 1 && n <= 9)
        }
        _ => false,
    }
}

pub fn generate_block_impl(generator: &mut Generator, block: &Block) -> String {
    let mut output = String::new();

    // Generate block commands without wrapping in { } scope
    // Shell { } does not create a new variable scope, so a Perl bare block
    // would incorrectly scope my declarations. Emit commands directly
    // at the current indentation level instead.
    generator.indent_level += 1;
    output.push_str(&generator.generate_block_commands(block));
    generator.indent_level -= 1;

    output
}

pub fn generate_break_statement_impl(_generator: &Generator, level: &Option<String>) -> String {
    match level {
        Some(level_str) => format!("last LABEL{};", level_str),
        None => "last;".to_string(),
    }
}

pub fn generate_continue_statement_impl(_generator: &Generator, level: &Option<String>) -> String {
    match level {
        Some(level_str) => format!("next LABEL{};", level_str),
        None => "next;".to_string(),
    }
}

pub fn generate_return_statement_impl(generator: &mut Generator, value: &Option<Word>) -> String {
    match value {
        Some(word) => {
            let perl_value = generator.perl_string_literal(word);
            format!("return {};", perl_value)
        }
        None => "return;".to_string(),
    }
}

// Helper method for indentation
pub fn indent_impl(generator: &Generator) -> String {
    "    ".repeat(generator.indent_level)
}

pub fn generate_block_commands_impl(generator: &mut Generator, block: &Block) -> String {
    let mut output = String::new();
    for command in &block.commands {
        let cmd_out = generator.generate_command(command);
        output.push_str(&cmd_out);
        // If the generated command ends with `}` without `;`, add a semicolon
        // so it can be used as a statement inside a `do { }` block.
        let trimmed = cmd_out.trim();
        if trimmed.ends_with('}') && !trimmed.ends_with(';') && !trimmed.ends_with(";}")
            && !trimmed.ends_with("};") && !trimmed.starts_with("if")
            && !trimmed.starts_with("while") && !trimmed.starts_with("for")
            && !trimmed.starts_with("foreach") && !trimmed.starts_with("sub")
        {
            output.push(';');
        }
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}

/// Recursively convert a tree of `And`/`Or` commands (whose leaves are
/// `TestExpression` or other commands) into a single Perl boolean expression.
fn generate_combined_test_condition(generator: &mut Generator, cmd: &Command) -> String {
    fn combine(generator: &mut Generator, cmd: &Command) -> String {
        match cmd {
            Command::TestExpression(te) => generator.generate_test_expression(te),
            Command::And(l, r) => {
                format!("({} && {})", combine(generator, l), combine(generator, r))
            }
            Command::Or(l, r) => {
                format!("({} || {})", combine(generator, l), combine(generator, r))
            }
            Command::Block(block) => {
                // A compound list { ... } in a condition: exit status =
                // exit status of the last command in the block.
                if block.commands.is_empty() {
                    "1".to_string()
                } else if block.commands.len() == 1 {
                    combine(generator, &block.commands[0])
                } else {
                    // Multiple commands: use do { } to execute all for side effects
                    // and return the last command's exit status as the condition.
                    let mut body = String::new();
                    for cmd in &block.commands[..block.commands.len() - 1] {
                        generator.suppress_set_e_depth += 1;
                        let line = generator.generate_command(cmd);
                        generator.suppress_set_e_depth -= 1;
                        body.push_str(&line);
                        // Ensure the generated command is properly terminated with `;`
                        // if it ends with `}` and is used as a statement inside `do { }`.
                        let trimmed = line.trim();
                        if trimmed.ends_with('}')
                            && !trimmed.ends_with(';')
                            && !trimmed.starts_with("if")
                            && !trimmed.starts_with("while")
                            && !trimmed.starts_with("for")
                            && !trimmed.starts_with("foreach")
                        {
                            body.push(';');
                        }
                        if !body.ends_with('\n') {
                            body.push('\n');
                        }
                    }
                    body.push_str(&combine(generator, &block.commands[block.commands.len() - 1]));
                    format!("do {{ {} }}", body)
                }
            }
            _ => {
                generator.suppress_set_e_depth += 1;
                let mut c = generator.generate_command(cmd);
                generator.suppress_set_e_depth -= 1;
                let c = c
                    .trim_start()
                    .strip_prefix("$main_exit_code = ")
                    .unwrap_or(&c)
                    .trim_end_matches(|c: char| c == ';' || c == '\n' || c == ' ' || c == '\t')
                    .trim_end_matches(';')
                    .to_string();
                format!("!({})", c)
            }
        }
    }
    combine(generator, cmd)
}

/// Recursively flatten an `And`/`Or` tree into a list of leaf commands.
fn flatten_conditions(cmd: &Command, conds: &mut Vec<Command>) {
    match cmd {
        Command::And(left, right) | Command::Or(left, right) => {
            flatten_conditions(left, conds);
            flatten_conditions(right, conds);
        }
        other => {
            conds.push(other.clone());
        }
    }
}

/// Recursively collect all variable names that are assigned via `Assignment` commands
/// in a command tree. This is used to hoist `my` declarations before conditional
/// statements (if/elsif/while/for) to satisfy Perl::Critic's
/// `ProhibitConditionalDeclarations` policy.
fn collect_assigned_vars(cmd: &Command, vars: &mut std::collections::HashSet<String>) {
    match cmd {
        Command::Assignment(assignment) => {
            vars.insert(assignment.variable.clone());
        }
        Command::Block(block) => {
            for c in &block.commands {
                collect_assigned_vars(c, vars);
            }
        }
        Command::If(if_stmt) => {
            collect_assigned_vars(&if_stmt.then_branch, vars);
            if let Some(else_branch) = &if_stmt.else_branch {
                collect_assigned_vars(else_branch, vars);
            }
        }
        Command::Pipeline(pipeline) => {
            for c in &pipeline.commands {
                collect_assigned_vars(c, vars);
            }
        }
        Command::And(left, right) | Command::Or(left, right) => {
            collect_assigned_vars(left, vars);
            collect_assigned_vars(right, vars);
        }
        Command::While(while_loop) => {
            collect_assigned_vars(&Command::Block(while_loop.body.clone()), vars);
        }
        Command::For(for_loop) => {
            collect_assigned_vars(&Command::Block(for_loop.body.clone()), vars);
        }
        Command::CStyleFor(for_loop) => {
            collect_assigned_vars(&Command::Block(for_loop.body.clone()), vars);
        }
        Command::Function(func) => {
            collect_assigned_vars(&Command::Block(func.body.clone()), vars);
        }
        Command::Case(case_stmt) => {
            for case in &case_stmt.cases {
                for c in &case.body {
                    collect_assigned_vars(c, vars);
                }
            }
        }
        Command::Redirect(redirect_cmd) => {
            collect_assigned_vars(&redirect_cmd.command, vars);
        }
        Command::Subshell(cmd) | Command::Background(cmd) | Command::Not(cmd) => {
            collect_assigned_vars(cmd, vars);
        }
        Command::BuiltinCommand(_)
        | Command::Simple(_)
        | Command::ShoptCommand(_)
        | Command::TestExpression(_)
        | Command::Break(_)
        | Command::Continue(_)
        | Command::Return(_)
        | Command::BlankLine => {}
    }
}

/// Emit `my $var;` declarations for any variables in `vars` that have not yet
/// been declared in the generator.  This is used before conditional statements
/// so that the `my` declaration sits outside the conditional body, satisfying
/// Perl::Critic's `ProhibitConditionalDeclarations` policy.
fn hoist_my_declarations(generator: &mut Generator, vars: &std::collections::HashSet<String>, output: &mut String) {
    for var in vars {
        if !generator.declared_locals.contains(var)
            && !generator.function_level_vars.contains(var)
        {
            output.push_str(&generator.indent());
            output.push_str(&format!("my ${};\n", var));
            generator.declared_locals.insert(var.clone());
        }
    }
}

/// Extract variable names from `read` commands in a command tree.
/// Returns a list of variable names that should be declared before a while loop.
fn extract_read_vars_from_condition(cmd: &Command) -> Vec<String> {
    let mut vars = Vec::new();
    match cmd {
        Command::Simple(cmd) => {
            if let Word::Literal(name, _) = &cmd.name {
                if name == "read" {
                    for arg in &cmd.args {
                        if let Word::Literal(s, _) = arg {
                            if s != "-r" && s != "-p" && s != "-n" && s != "-t" && !s.starts_with('-') {
                                vars.push(s.clone());
                            }
                        }
                    }
                }
            }
        }
        Command::And(left, right) | Command::Or(left, right) => {
            vars.extend(extract_read_vars_from_condition(left));
            vars.extend(extract_read_vars_from_condition(right));
        }
        Command::Block(block) => {
            for cmd in &block.commands {
                vars.extend(extract_read_vars_from_condition(cmd));
            }
        }
        Command::Pipeline(pipeline) => {
            for cmd in &pipeline.commands {
                vars.extend(extract_read_vars_from_condition(cmd));
            }
        }
        _ => {}
    }
    vars
}

/// Scan a function body for assignments from positional parameters, e.g.
/// `x=\$1; y=\$2`.  Returns `{1 → "x", 2 → "y"}`.
/// These are used to generate clean `my ($x, $y) = @_;` unpacking and to
/// pass named arguments at call sites.
pub fn build_param_name_map(block: &Block) -> HashMap<usize, String> {
    let mut map = HashMap::new();
    for cmd in &block.commands {
        if let Command::Assignment(assign) = cmd {
            // Check if the value is a positional parameter reference like $1, $2
            if let Word::Variable(var, _, _) = &assign.value {
                if let Ok(idx) = var.parse::<usize>() {
                    if idx >= 1 {
                        map.insert(idx, assign.variable.clone());
                    }
                }
            }
            // Also check StringInterpolation containing just a Variable
            if let Word::StringInterpolation(interp, _) = &assign.value {
                if interp.parts.len() == 1 {
                    if let StringPart::Variable(var) = &interp.parts[0] {
                        if let Ok(idx) = var.parse::<usize>() {
                            if idx >= 1 {
                                map.insert(idx, assign.variable.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    map
}
