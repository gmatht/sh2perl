use crate::ast::*;
use crate::generator::Generator;

pub fn generate_cut_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    _command_index: usize,
) -> String {
    generate_cut_command_with_output(generator, cmd, input_var, _command_index, "")
}

pub fn generate_cut_command_with_output(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    _command_index: usize,
    output_var: &str,
) -> String {
    let mut output = String::new();
    // (debug logging removed)

    // Support both attached (-d, -f1,3) and separated (-d , -f 1,3) forms.
    let mut delimiter_word: Option<Word> = None;
    let mut field_num: usize = 1; // default to first field (1-based)
    let mut selected_fields: Option<Vec<usize>> = None;
    let mut open_ended_from: Option<usize> = None; // e.g. -f2- means "from field 2 to end"

    // Parse options
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(arg, _) = &cmd.args[i] {
            if arg.starts_with("-d") {
                let rest = &arg[2..];
                if !rest.is_empty() {
                    // attached form: -d,
                    delimiter_word = Some(Word::literal(rest.to_string()));
                } else if i + 1 < cmd.args.len() {
                    delimiter_word = Some(cmd.args[i + 1].clone());
                    i += 1; // skip delimiter token
                }
            } else if arg.starts_with("-f") {
                let rest = &arg[2..];
                let mut parsed: Vec<usize> = Vec::new();
                let parse_field_spec =
                    |spec: &str, parsed: &mut Vec<usize>, open_ended: &mut Option<usize>| {
                        for p in spec.split(',') {
                            let p = p.trim();
                            // Open-ended range like "2-" means from field 2 to end
                            if p.ends_with('-') {
                                if let Ok(n) = p[..p.len() - 1].parse::<usize>() {
                                    *open_ended = Some(n);
                                }
                            } else if let Ok(n) = p.parse::<usize>() {
                                parsed.push(n);
                            }
                        }
                    };
                if !rest.is_empty() {
                    parse_field_spec(rest, &mut parsed, &mut open_ended_from);
                } else if i + 1 < cmd.args.len() {
                    // separated form -f 1,3
                    let field_token = &cmd.args[i + 1];
                    let field_str = generator.strip_shell_quotes_for_regex(field_token);
                    parse_field_spec(&field_str, &mut parsed, &mut open_ended_from);
                    i += 1; // skip field token
                }

                if !parsed.is_empty() {
                    if parsed.len() == 1 {
                        field_num = parsed[0];
                    } else {
                        selected_fields = Some(parsed);
                    }
                }
            }
        }
        i += 1;
    }

    // Collect file arguments (non-option args)
    let mut file_args: Vec<Word> = Vec::new();
    let mut i_arg = 0;
    while i_arg < cmd.args.len() {
        if let Word::Literal(arg, _) = &cmd.args[i_arg] {
            if arg.starts_with("-") {
                // Skip options and their values
                if arg == "-d" && i_arg + 1 < cmd.args.len() {
                    i_arg += 1; // skip the delimiter value
                } else if arg.starts_with("-f") {
                    if arg.len() == 2 && i_arg + 1 < cmd.args.len() {
                        i_arg += 1; // skip the field value
                    }
                }
            } else {
                // Non-option argument = file argument
                file_args.push(cmd.args[i_arg].clone());
            }
        }
        i_arg += 1;
    }

    // Build regex pattern for split; default is tab
    let delim_for_regex = if let Some(ref w) = delimiter_word {
        generator.strip_shell_quotes_for_regex(w)
    } else {
        "\\t".to_string()
    };

    // Build a Perl literal for joining selected fields. Decode common shell escapes
    // (eg "\\t" -> actual tab) so join emits the intended character.
    let delim_for_join_raw = crate::generator::utils::decode_shell_escapes_impl(&delim_for_regex);
    let join_lit = generator.perl_string_literal(&Word::literal(delim_for_join_raw));

    let effective_input = if input_var.is_empty() && !file_args.is_empty() {
        // Read from file arguments
        let temp_var = format!("cut_input_{}", generator.get_unique_id());
        let first_file = generator.word_to_perl(&file_args[0]);
        output.push_str(&format!(
            "my ${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; my $fh; if (open $fh, \'<\', {}) {{ <$fh> }} else {{ warn \"Cannot open {}: $OS_ERROR\\n\"; q{{}} }} }};\n",
            temp_var, first_file, first_file
        ));
        temp_var
    } else if input_var.is_empty() {
        // No file arguments and no input variable - read from STDIN
        let temp_var = format!("cut_input_{}", generator.get_unique_id());
        output.push_str(&format!(
            "my ${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <STDIN> }};\n",
            temp_var
        ));
        temp_var
    } else {
        input_var.to_string()
    };

    let unique_id = generator.get_unique_id();
    output.push_str(&format!(
        "my @lines_{} = split /\\n/msx, ${};\n",
        unique_id, effective_input
    ));
    output.push_str(&format!("my @result_{};\n", unique_id));
    output.push_str(&format!("foreach my $line (@lines_{}) {{\n", unique_id));
    output.push_str("chomp $line;\n");

    // Split into fields using a properly formatted regex
    output.push_str(&format!(
        "my @fields = split {}, $line;\n",
        generator.format_regex_pattern(&delim_for_regex)
    ));

    if let Some(from) = open_ended_from {
        // Open-ended range: fields from N to end (e.g. -f2-)
        let from_index = if from > 0 { from - 1 } else { 0 };
        output.push_str(&format!(
            "if (@fields > {}) {{ push @result_{}, join({}, @fields[{}..@fields-1]); }}\n",
            from_index, unique_id, join_lit, from_index
        ));
    } else if let Some(fields_vec) = selected_fields {
        // Multiple fields: select and join with original delimiter
        let zero_based: Vec<usize> = fields_vec
            .into_iter()
            .map(|n| if n > 0 { n - 1 } else { 0 })
            .collect();

        output.push_str("my @sel = ();\n");
        for idx in zero_based.iter() {
            output.push_str(&format!(
                "if (@fields > {}) {{ push @sel, $fields[{}]; }}\n",
                idx, idx
            ));
        }
        output.push_str(&format!(
            "push @result_{}, join({}, @sel);\n",
            unique_id, join_lit
        ));
    } else {
        // Single field selection
        let field_index = if field_num > 0 { field_num - 1 } else { 0 };
        output.push_str(&format!("if (@fields > {}) {{\n", field_index));
        output.push_str(&format!(
            "    push @result_{}, $fields[{}];\n",
            unique_id, field_index
        ));
        output.push_str("}\n");
    }

    output.push_str("}\n");
    let result_var = if !output_var.is_empty() {
        output_var.to_string()
    } else if !input_var.is_empty() {
        input_var.to_string()
    } else {
        // Both empty - use a temp var (shouldn't normally happen)
        format!("cut_result_{}", generator.get_unique_id())
    };
    if !result_var.is_empty() {
        output.push_str(&format!(
            "${} = join \"\\n\", @result_{};\n",
            result_var, unique_id
        ));
        output.push_str(&format!(
            "if (${} ne q{{}} && !(${}  =~ m{{\\n\\z}}msx)) {{ ${} .= \"\\n\"; }}\n",
            result_var, result_var, result_var
        ));
    }
    output.push_str("\n");

    output
}
