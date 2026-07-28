use crate::ast::SimpleCommand;
use crate::ast::Word;
use crate::generator::commands::system_commands::word_to_bash_string_for_system;
use crate::generator::Generator;

/// Simple transformation: replace bareword file handles in Perl code with lexical ones.
/// This avoids Perl::Critic "Bareword file handle" violations.
/// Strategy: find all open(NAME, or open NAME, patterns, collect names,
/// then do simple string replacements on `NAME` -> `$NAME` in filehandle positions.
fn bareword_fh_to_lexical(code: &str) -> String {
    let mut result = code.to_string();

    // 1. Collect bareword file handle names from open() calls
    let mut names: Vec<String> = Vec::new();
    // Simpler: just hardcode the known filehandle patterns that appear in practice
    // Collect all uppercase identifiers followed by , in open/open(my context
    let mut i = 0;
    let bytes = result.as_bytes().to_vec();
    while i < bytes.len() {
        // look for "open" or "open("
        if (i + 4 <= bytes.len() && &bytes[i..i+4] == b"open") {
            // skip past "open" and any whitespace/parens
            let mut j = i + 4;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'(' || bytes[j] == b'\n') {
                j += 1;
            }
            // check for uppercase identifier (filehandle name)
            let start = j;
            while j < bytes.len() && bytes[j].is_ascii_uppercase() {
                j += 1;
            }
            if j > start {
                let name = String::from_utf8_lossy(&bytes[start..j]).to_string();
                if !names.contains(&name) && name != "STDIN" && name != "STDOUT" && name != "STDERR" && name.len() >= 1 {
                    names.push(name);
                }
            }
        }
        i += 1;
    }

    if names.is_empty() {
        return result;
    }

    // 2. For each name, do targeted replacements
    for name in &names {
        // open(NAME, -> open(my $NAME,
        result = result.replace(&format!("open({},", name), &format!("open(my ${},", name));
        // open NAME, -> open my $NAME, (but not "open my $NAME," already)
        result = result.replace(&format!("open {},", name), &format!("open my ${},", name));
        // while (<NAME>) -> while (<$NAME>)
        result = result.replace(&format!("while (<{}>)", name), &format!("while (<${}>)", name));
        // (<NAME>) in other contexts -> (<$NAME>)
        // but be careful: close(NAME) -> close($NAME)
        result = result.replace(&format!("close({})", name), &format!("close(${})", name));
        result = result.replace(&format!("close {}", name), &format!("close ${}", name));
        // print NAME -> print {$NAME}
        result = result.replace(&format!("print {} ", name), &format!("print {{${}}} ", name));
        result = result.replace(&format!("print {}\n", name), &format!("print {{${}}}\n", name));
        result = result.replace(&format!("print {};", name), &format!("print {{${}}};", name));
        result = result.replace(&format!("print {} or", name), &format!("print {{${}}} or", name));
        // <NAME> (angle-bracket read from filehandle)
        result = result.replace(&format!("<{}>", name), &format!("<${}>", name));
    }

    // 3. Convert two-argument opens to three-argument opens for
    //    bareword-filehandle patterns like: open FH, ">file"
    //    We handle the common case: open $name, "MODE..."
    for name in &names {
        let pat = format!(" ${}, \"", name);
        let mut new_result = String::new();
        let mut scan = 0usize;
        let rb = result.as_bytes();
        while scan < rb.len() {
            if let Some(idx) = result[scan..].find(&pat) {
                let abs = scan + idx;
                new_result.push_str(&result[scan..abs + pat.len() - 1]); // includes " ${}, "
                let mode_start = abs + pat.len() - 1; // position of the opening quote
                if mode_start < rb.len() && rb[mode_start] == b'"' {
                    // The quote was already consumed by pat, which ends at '\"'
                    // Actually pat ends with '\"' which is the backslash-escaped quote in pattern.
                    // Let's re-check: pat = format!(" ${}, \"", name) -> " $name, \"" where \" is
                    // the escaped double-quote. So the pattern ends BEFORE the quote.
                    // The opening " is at offset abs + pat.len()
                    // Let's fix the logic:
                    // pat is " $name, " (the , " at the end)
                    // After all the replacements, we need to find the actual quote.
                }
                // Simplified approach: scan for mode characters after the match
                let mut mode_pos = abs + pat.len();
                while mode_pos < rb.len() && (rb[mode_pos] == b'>' || rb[mode_pos] == b'<' || rb[mode_pos] == b'&' || rb[mode_pos] == b'|' || rb[mode_pos] == b'-') {
                    mode_pos += 1;
                }
                if mode_pos > abs + pat.len() {
                    let mode = &result[abs + pat.len()..mode_pos];
                    new_result.push_str(&format!("q{{{}}}, \"", mode));
                    scan = mode_pos;
                } else {
                    new_result.push('"');
                    scan = abs + pat.len();
                }
            } else {
                new_result.push_str(&result[scan..]);
                break;
            }
        }
        result = new_result;
    }

    result
}

/// Handle Perl commands by embedding the Perl code directly
pub fn generate_perl_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    eprintln!(
        "DEBUG: generate_perl_command called with args: {:?}",
        cmd.args
    );
    let mut output = String::new();

    if cmd.args.len() >= 2 {
        // Check for -e flag (execute code)
        if let Word::Literal(flag, _) = &cmd.args[0] {
            if flag == "-e" {
                // Extract the Perl code from the second argument
                let perl_code = if let Word::Literal(perl_code, _) = &cmd.args[1] {
                    Some(perl_code.clone())
                } else if let Word::StringInterpolation(interp, _) = &cmd.args[1] {
                    Some(generator.convert_string_interpolation_to_perl(&interp))
                } else {
                    None
                };

                if let Some(perl_code) = perl_code {
                    eprintln!("DEBUG: Found perl code: {}", perl_code);
                    let mut clean_code = perl_code.clone();
                    if (clean_code.starts_with('"') && clean_code.ends_with('"'))
                        || (clean_code.starts_with('\'') && clean_code.ends_with('\''))
                    {
                        clean_code = clean_code[1..clean_code.len() - 1].to_string();
                    }
                    eprintln!("DEBUG: Clean perl code: {}", clean_code);

                    // Transform bareword file handles to lexical file handles
                    clean_code = bareword_fh_to_lexical(&clean_code);
                    eprintln!("DEBUG: After bareword fix: {}", clean_code);

                    if cmd.args.len() > 2 {
                        output.push_str("@ARGV = (");
                        for (i, arg) in cmd.args.iter().skip(2).enumerate() {
                            if i > 0 {
                                output.push_str(", ");
                            }
                            output.push_str(&generator.word_to_perl(arg));
                        }
                        output.push_str(");\n");
                    }

                    output.push_str("if (!defined $ENV{SHELL_VAR}) { $ENV{SHELL_VAR} = q{}; }\n");

                    eprintln!("DEBUG: About to execute perl code lines");
                    for line in clean_code.lines() {
                        let trimmed_line = line.trim();
                        if !trimmed_line.is_empty() {
                            output.push_str(&generator.indent());
                            let mut final_line = trimmed_line.to_string();
                            if trimmed_line.starts_with("foreach $")
                                && !trimmed_line.contains("my $")
                            {
                                final_line = trimmed_line.replace("foreach $", "foreach my $");
                            }
                            if !final_line.ends_with(';')
                                && !final_line.ends_with('{')
                                && !final_line.ends_with('}')
                                && !final_line.starts_with('#')
                            {
                                output.push_str(&format!("{};\n", final_line));
                            } else {
                                output.push_str(&format!("{}\n", final_line));
                            }
                        }
                    }
                    eprintln!("DEBUG: Returning perl output: {}", output);
                    return output;
                }
            } else if flag == "-ne" {
                let perl_code = if let Word::Literal(perl_code, _) = &cmd.args[1] {
                    Some(perl_code.clone())
                } else if let Word::StringInterpolation(interp, _) = &cmd.args[1] {
                    Some(generator.convert_string_interpolation_to_perl(&interp))
                } else {
                    None
                };

                if let Some(perl_code) = perl_code {
                    let mut clean_code = perl_code.clone();
                    if (clean_code.starts_with('"') && clean_code.ends_with('"'))
                        || (clean_code.starts_with('\'') && clean_code.ends_with('\''))
                    {
                        clean_code = clean_code[1..clean_code.len() - 1].to_string();
                    }

                    // Also apply bareword fix for -ne path
                    clean_code = bareword_fh_to_lexical(&clean_code);

                    output.push_str(&generator.indent());
                    output.push_str(&format!("# Perl -ne: {}\n", clean_code));
                    for line in clean_code.lines() {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("{}\n", line));
                    }
                    return output;
                }
            }
        }
    }

    // Fallback to system call if not a -e or -ne command
    let args_list = cmd
        .args
        .iter()
        .map(|arg| {
            let word = Word::Literal(
                word_to_bash_string_for_system(generator, arg),
                Default::default(),
            );
            generator.perl_string_literal(&word)
        })
        .collect::<Vec<_>>();

    let output_var = format!("perl_output_{}", generator.get_unique_id());

    if args_list.len() >= 2 && args_list[0] == "\"-e\"" {
        let code = &args_list[1];
        output.push_str(&format!(
            "my ${} = do {{ 
            my $result;
            my $eval_success = eval {{
                $result = capture_stdout( sub {{ {} }} );
                1;
            }};
            if ( !$eval_success ) {{
                $result = \"Error executing Perl code: $EVAL_ERROR\";
            }}
            $result;
        }};\n",
            output_var, code
        ));
    } else {
        let formatted_args = args_list.join(" ");
        output.push_str(&format!(
            "my ${} = do {{ 
            my $result = qx{{perl {}}};
            chomp $result;
            $result;
        }};\n",
            output_var, formatted_args
        ));
    }
    output.push_str(&format!("print ${};\n", output_var));

    output
}

/// Handle Perl commands within pipelines
pub fn generate_perl_pipeline_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
) -> String {
    let mut output = String::new();
    let mut perl_code = String::new();
    let mut is_ne = false;

    for (i, arg) in cmd.args.iter().enumerate() {
        if let Word::Literal(s, _) = arg {
            if s == "-e" {
                if i + 1 < cmd.args.len() {
                    if let Word::Literal(code, _) = &cmd.args[i + 1] {
                        perl_code = code.clone();
                        break;
                    } else if let Word::StringInterpolation(interp, _) = &cmd.args[i + 1] {
                        perl_code = generator.convert_string_interpolation_to_perl(&interp);
                        break;
                    }
                }
            } else if s == "-ne" {
                if i + 1 < cmd.args.len() {
                    if let Word::Literal(code, _) = &cmd.args[i + 1] {
                        perl_code = code.clone();
                        is_ne = true;
                        break;
                    } else if let Word::StringInterpolation(interp, _) = &cmd.args[i + 1] {
                        perl_code = generator.convert_string_interpolation_to_perl(&interp);
                        is_ne = true;
                        break;
                    }
                }
            }
        }
    }

    if !perl_code.is_empty() {
        let mut clean_code = perl_code.clone();

        if (clean_code.starts_with('\'') && clean_code.ends_with('\''))
            || (clean_code.starts_with('"') && clean_code.ends_with('"'))
        {
            clean_code = clean_code[1..clean_code.len() - 1].to_string();
        }

        // Also apply bareword fix for pipeline path
        clean_code = bareword_fh_to_lexical(&clean_code);

        let output_var = format!("perl_output_{}", generator.get_unique_id());
        output.push_str(&format!("my ${} = q{{}};\n", output_var));

        if is_ne {
            output.push_str(&format!(
                "for my $line (split /\\n/, ${}) {{\n",
                input_var
            ));
            output.push_str(&format!("    $_ = \"$line\\n\";\n"));
        } else {
            output.push_str(&format!("$_ = ${};\n", input_var));
        }

        output.push_str("if (!defined $ENV{SHELL_VAR}) { $ENV{SHELL_VAR} = q{}; }\n");

        for line in clean_code.lines() {
            let trimmed_line = line.trim();
            if !trimmed_line.is_empty() {
                let mut final_line = trimmed_line.to_string();
                if trimmed_line.starts_with("foreach $") && !trimmed_line.contains("my $") {
                    final_line = trimmed_line.replace("foreach $", "foreach my $");
                }

                if final_line.contains("print ") {
                    let parts: Vec<&str> = final_line.split(';').collect();
                    let mut processed_parts = Vec::new();

                    for part in parts {
                        let trimmed_part = part.trim();
                        if trimmed_part.starts_with("print ") {
                            processed_parts.push(
                                trimmed_part.replace("print ", &format!("${} .= ", output_var)),
                            );
                        } else if !trimmed_part.is_empty() {
                            processed_parts.push(trimmed_part.to_string());
                        }
                    }

                    final_line = processed_parts.join("; ");
                }

                if !final_line.ends_with(';')
                    && !final_line.ends_with('{')
                    && !final_line.ends_with('}')
                    && !final_line.starts_with('#')
                {
                    output.push_str(&format!("{};\n", final_line));
                } else {
                    output.push_str(&format!("{}\n", final_line));
                }
            }
        }

        if is_ne {
            output.push_str("}\n");
        }

        output.push_str(&format!("${} = ${};\n", input_var, output_var));
    } else {
        let args_list = cmd
            .args
            .iter()
            .map(|arg| {
                let word = Word::Literal(
                    word_to_bash_string_for_system(generator, arg),
                    Default::default(),
                );
                generator.perl_string_literal(&word)
            })
            .collect::<Vec<_>>();

        let output_var = format!("perl_output_{}", generator.get_unique_id());

        if args_list.len() >= 2 && args_list[0] == "\"-e\"" {
            let code = &args_list[1];
            output.push_str(&format!(
                "my ${} = do {{ 
                my $result;
                my $eval_success = eval {{
                    $result = capture_stdout(sub {{ {} }});
                    1;
                }};
                if (!$eval_success) {{
                    $result = \"Error executing Perl code: $EVAL_ERROR\";
                }}
                $result;
            }};\n",
                output_var, code
            ));
        } else {
            let formatted_args = args_list.join(" ");
            output.push_str(&format!(
                "my ${} = do {{ 
                my $result = qx{{perl {}}};
                chomp $result;
                $result;
            }};\n",
                output_var, formatted_args
            ));
        }
        output.push_str(&format!("print ${};\n", output_var));
    }

    output
}
