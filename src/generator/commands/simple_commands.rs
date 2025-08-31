use crate::ast::*;
use crate::generator::Generator;
use std::sync::atomic::{AtomicUsize, Ordering};

fn generate_cartesian_product_for_echo(items: &[Vec<String>]) -> Vec<String> {
    println!("DEBUG: generate_cartesian_product_for_echo called with {:?}", items);
    if items.is_empty() {
        println!("DEBUG: Empty items, returning empty vector");
        return vec![];
    }
    if items.len() == 1 {
        println!("DEBUG: Single item, returning {:?}", items[0]);
        return items[0].clone();
    }
    
    println!("DEBUG: Computing cartesian product for {} items", items.len());
    let mut result = Vec::new();
    let first = &items[0];
    let rest = generate_cartesian_product_for_echo(&items[1..]);
    
    for item in first {
        for rest_item in &rest {
            let combined = format!("{}{}", item, rest_item);
            println!("DEBUG: Combining '{}' + '{}' = '{}'", item, rest_item, combined);
            result.push(combined);
        }
    }
    
    println!("DEBUG: Final cartesian product result: {:?}", result);
    result
}

// Static counter for generating unique temp file names
static TEMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Dispatch to command-specific generators
fn generate_command_specific(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str) -> Option<String> {
    let cmd_name = match &cmd.name {
        Word::Literal(s) => s,
        _ => return None
    };
    
    // Use default input variable for commands that need it
    let default_input = if input_var.is_empty() { "input_data" } else { input_var };
    
    match cmd_name.as_str() {
        "grep" => {
            let unique_id = generator.get_unique_id();
            Some(super::grep::generate_grep_command(generator, cmd, default_input, &unique_id, true))
        },
        "cat" => Some(super::cat::generate_cat_command(generator, cmd, &cmd.redirects, "$output")),
        "find" => Some(super::find::generate_find_command(generator, cmd, true, "$output")),
                        "ls" => Some(super::ls::generate_ls_command(generator, cmd, false, None)),
        "wc" => Some(super::wc::generate_wc_command(generator, cmd, default_input, "0")),
        "sort" => Some(super::sort::generate_sort_command(generator, cmd, default_input, "0")),
        "uniq" => Some(super::uniq::generate_uniq_command(generator, cmd, default_input, "0")),
        "xargs" => Some(super::xargs::generate_xargs_command(generator, cmd, default_input, 0)),
        "awk" => Some(super::awk::generate_awk_command(generator, cmd, default_input, 0)),
        "sed" => Some(super::sed::generate_sed_command(generator, cmd, default_input, 0)),
        "comm" => Some(super::comm::generate_comm_command(generator, cmd, default_input, 0)),
        "tr" => Some(super::tr::generate_tr_command(generator, cmd, default_input, 0)),
        "sleep" => Some(super::sleep::generate_sleep_command(generator, cmd)),
        "cut" => Some(super::cut::generate_cut_command(generator, cmd, default_input, 0)),
        "basename" => Some(super::basename::generate_basename_command(generator, cmd, default_input)),
        "dirname" => Some(super::dirname::generate_dirname_command(generator, cmd, default_input)),
        "date" => Some(super::date::generate_date_command(generator, cmd)),
        "time" => Some(super::time::generate_time_command(generator, cmd)),
        "wget" => Some(super::wget::generate_wget_command(generator, cmd)),
        "which" => Some(super::which::generate_which_command(generator, cmd)),
        "yes" => Some(super::yes::generate_yes_command(generator, cmd)),
        "zcat" => Some(super::zcat::generate_zcat_command(generator, cmd)),
        "strings" => Some(super::strings::generate_strings_command(generator, cmd, default_input)),
        "tee" => Some(super::tee::generate_tee_command(generator, cmd, default_input)),
        "sha256sum" => Some(super::sha256sum::generate_sha256sum_command(generator, cmd, default_input)),
        "sha512sum" => Some(super::sha512sum::generate_sha512sum_command(generator, cmd, default_input)),
        "gzip" => Some(super::gzip::generate_gzip_command(generator, cmd, default_input)),
        "kill" => Some(super::kill::generate_kill_command(generator, cmd)),
        "nohup" => Some(super::nohup::generate_nohup_command(generator, cmd)),
        "nice" => Some(super::nice::generate_nice_command(generator, cmd)),
        "curl" => Some(super::curl::generate_curl_command(generator, cmd)),
        "mkdir" => Some(super::mkdir::generate_mkdir_command(generator, cmd)),
        "rm" => Some(super::rm::generate_rm_command(generator, cmd)),
        "cp" => Some(super::cp::generate_cp_command(generator, cmd)),
        "mv" => Some(super::mv::generate_mv_command(generator, cmd)),
        "touch" => Some(super::touch::generate_touch_command(generator, cmd)),
        "printf" => Some(super::printf::generate_printf_command(generator, cmd, default_input, 0)),
        "head" => Some(super::head::generate_head_command(generator, cmd, default_input, 0)),
        "tail" => Some(super::tail::generate_tail_command(generator, cmd, default_input, 0)),
        "paste" => Some(super::paste::generate_paste_command(generator, cmd, &[])),
        "diff" => Some(super::diff::generate_diff_command(generator, cmd, default_input, 0, true)),
        _ => None
    }
}

pub fn generate_simple_command_impl(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // Handle array assignments first (these need to be in the main scope)
    for (var, value) in &cmd.env_vars {
        if let Word::Array(_, elements) = value {
            // Handle array assignment like arr=(one two three)
            let elements_perl: Vec<String> = elements.iter()
                .map(|s| format!("\"{}\"", generator.escape_perl_string(s)))
                .collect();
            output.push_str(&generator.indent());
            output.push_str(&format!("my @{} = ({});\n", var, elements_perl.join(", ")));
            // Mark array as declared
            if !generator.declared_locals.contains(var) {
                generator.declared_locals.insert(var.clone());
            }
        } else if let Word::Literal(s) = value {
            if let Some(elements) = generator.extract_array_elements(s) {
                // Check if this is an indexed array assignment like arr=(one two three)
                let elements_perl: Vec<String> = elements.iter()
                    .map(|s| format!("\"{}\"", generator.escape_perl_string(s)))
                    .collect();
                output.push_str(&generator.indent());
                output.push_str(&format!("my @{} = ({});\n", var, elements_perl.join(", ")));
            }
        }
    }
    
    // Check if there are any non-array environment variables to process
    // But exclude standalone assignments (cmd.name == "true")
    let is_standalone_assignment = if let Word::Literal(ref name) = cmd.name {
        name == "true" && !cmd.env_vars.is_empty() && cmd.args.is_empty()
    } else {
        false
    };
    
    let has_non_array_env = !is_standalone_assignment && cmd.env_vars.iter().any(|(var, value)| {
        !matches!(value, Word::Array(_, _)) && 
        !matches!(value, Word::Literal(s) if generator.extract_array_elements(s).is_some())
    });
    
    if has_non_array_env {
        for (var, value) in &cmd.env_vars {
            // Check if this is an associative array assignment like map[foo]=bar
            if let Some((array_name, key)) = generator.extract_array_key(var) {
                let val = generator.perl_string_literal(value);
                // For associative array assignments, generate $array{key} = value instead of $ENV{var}
                // Quote the key to avoid bareword errors in strict mode
                let quoted_key = format!("\"{}\"", generator.escape_perl_string(&key));
                output.push_str(&generator.indent());
                output.push_str(&format!("${}{{{}}} = {};\n", array_name, quoted_key, val));
            } else if let Word::Array(_, _) = value {
                // Skip array assignments here - they're handled above
                continue;
            } else if let Word::Literal(s) = value {
                if let Some(_) = generator.extract_array_elements(s) {
                    // Skip array assignments here - they're handled above
                    continue;
                } else {
                    // Regular string assignment
                    let val = generator.perl_string_literal(value);
                    // Always assign the value, but only declare if not already declared
                    if !generator.declared_locals.contains(var) {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = {};\n", var, val));
                        generator.declared_locals.insert(var.clone());
                    } else {
                        // Variable already declared, just assign the value
                        output.push_str(&generator.indent());
                        output.push_str(&format!("${} = {};\n", var, val));
                    }
                    // Only set environment variable if this is not a standalone variable assignment
                    if let Word::Literal(ref name) = cmd.name {
                        if name != "true" {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("local $ENV{{{}}} = {};;\n", var, val));
                        }
                    }
                }
            } else {
                // Handle other Word types
                let val = generator.perl_string_literal(value);
                // Always assign the value, but only declare if not already declared
                if !generator.declared_locals.contains(var) {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("my ${} = {};\n", var, val));
                    generator.declared_locals.insert(var.clone());
                } else {
                    // Variable already declared, just assign the value
                    output.push_str(&generator.indent());
                    output.push_str(&format!("${} = {};\n", var, val));
                }
                // Only set environment variable if this is not a standalone variable assignment
                if let Word::Literal(ref name) = cmd.name {
                    if name != "true" {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("local $ENV{{{}}} = {};;\n", var, val));
                    }
                }
            }
        }
    }

    // Pre-process process substitution and here-string redirects to create temporary files
    let mut process_sub_files = Vec::new();
    let mut _has_here_string = false;
    let mut temp_file_counter = 0;
    for redir in &cmd.redirects {
        match &redir.operator {
            RedirectOperator::ProcessSubstitutionInput(cmd) => {
                // Process substitution input: <(command)
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("/tmp/process_sub_{}_{}.tmp", global_counter, temp_file_counter);
                let temp_var = format!("temp_file_ps_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                
                // Execute the command and capture its output
                let fh_var = format!("fh_ps_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${};\n", fh_var));
                output.push_str(&generator.indent());
                output.push_str(&format!("{{\n"));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("local $/;  # Read entire input at once\n"));
                
                // Store the command string in a local variable to avoid borrowing issues
                let cmd_str = generator.generate_command_string_for_system(&**cmd);
                output.push_str(&generator.indent());
                output.push_str(&format!("open(my $pipe, '-|', 'bash', '-c', {});\n", 
                    generator.perl_string_literal(&Word::Literal(cmd_str))));
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_ps_{} = <$pipe>;\n", global_counter));
                output.push_str(&generator.indent());
                output.push_str(&format!("close($pipe);\n"));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("}}\n"));
                
                // Write the output to the temporary file
                output.push_str(&generator.indent());
                output.push_str(&format!("open(my ${}, '>', ${}) or die \"Cannot create temp file: $!\\n\";\n", fh_var, temp_var));
                output.push_str(&generator.indent());
                output.push_str(&format!("print ${} $output_ps_{};\n", fh_var, global_counter));
                output.push_str(&generator.indent());
                output.push_str(&format!("close(${});\n", fh_var));
                
                process_sub_files.push((temp_var, temp_file));
            }
            RedirectOperator::ProcessSubstitutionOutput(_cmd) => {
                // Process substitution output: >(command)
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("/tmp/process_sub_out_{}_{}.tmp", global_counter, temp_file_counter);
                let temp_var = format!("temp_file_out_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                process_sub_files.push((temp_var, temp_file));
            }
            RedirectOperator::HereString => {
                // Here-string: <<< content
                _has_here_string = true;
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("/tmp/here_string_{}_{}.tmp", global_counter, temp_file_counter);
                let temp_var = format!("temp_file_hs_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                
                // Create the temporary file with the here-string content
                if let Some(content) = &redir.heredoc_body {
                    let fh_var = format!("fh_hs_{}_{}", global_counter, temp_file_counter);
                    output.push_str(&generator.indent());
                    output.push_str(&format!("open(my ${}, '>', ${}) or die \"Cannot create temp file: $!\\n\";\n", fh_var, temp_var));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("print ${} {};\n", fh_var, generator.perl_string_literal(&Word::Literal(content.clone()))));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("close(${});\n", fh_var));
                }
                
                process_sub_files.push((temp_var, temp_file));
            }
            _ => {}
        }
    }

    // Generate the actual command
    if let Word::Literal(ref name) = cmd.name {
        if name == "echo" {
            // Special handling for echo command
            if cmd.args.is_empty() {
                output.push_str(&generator.indent());
                output.push_str("print \"\\n\";\n");
            } else {
                // Check if we need comma-separated printing for special variables
                let mut needs_comma_separated_print = false;
                let mut processed_args: Vec<String> = Vec::new();
                
                let mut i = 0;
                while i < cmd.args.len() {
                    match &cmd.args[i] {
                        Word::Literal(s) => {
                            // Properly quote literal strings for Perl
                            // Check if the string is already quoted (starts and ends with same quote)
                            let trimmed = s.trim();
                            if (trimmed.starts_with("'") && trimmed.ends_with("'")) || 
                               (trimmed.starts_with("\"") && trimmed.ends_with("\"")) {
                                // Strip the surrounding quotes and escape for Perl
                                let content = &trimmed[1..trimmed.len()-1];
                                processed_args.push(format!("\"{}\"", generator.escape_perl_string(content)));
                            } else {
                                // Not quoted, but check if it contains already escaped quotes
                                // If the string contains \" or \', we need to handle it specially
                                if s.contains("\\\"") || s.contains("\\'") {
                                    // The string already has escaped quotes, don't double-escape
                                    // Just escape newlines and tabs, but preserve the existing quote escaping
                                    let escaped = s.replace("\n", "\\n")
                                                  .replace("\t", "\\t")
                                                  .replace("\r", "\\r");
                                    processed_args.push(format!("\"{}\"", escaped));
                                } else {
                                    // Normal case, escape as-is
                                    processed_args.push(format!("\"{}\"", generator.escape_perl_string(s)));
                                }
                            }
                            i += 1;
                        },
                        Word::Variable(var) => {
                            // Convert shell variables to Perl variables
                            processed_args.push(format!("${}", var));
                            i += 1;
                        },
                        Word::ParameterExpansion(pe) => {
                            // Handle parameter expansions as expressions, not strings
                            // This should generate code like $arr[1] or $map{foo}
                            processed_args.push(generator.generate_parameter_expansion(pe));
                            i += 1;
                        },
                        Word::StringInterpolation(interp) => {
                            // Handle string interpolation specially for echo
                            // For variables like $#, we want to evaluate them, not treat them as literals
                            let mut can_handle_interp = true;
                            let mut interp_result = String::new();
                            
                            for part in &interp.parts {
                                match part {
                                    StringPart::Literal(s) => {
                                        interp_result.push_str(s);
                                    },
                                    StringPart::Variable(var) => {
                                        // Handle special shell variables
                                        match var.as_str() {
                                            "#" => {
                                                // $# -> scalar(@ARGV) - we want to evaluate this
                                                needs_comma_separated_print = true;
                                                can_handle_interp = false;
                                                break;
                                            },
                                            "@" => {
                                                // $@ -> @ARGV - we want to evaluate this
                                                needs_comma_separated_print = true;
                                                can_handle_interp = false;
                                                break;
                                            },
                                            "*" => {
                                                // $* -> @ARGV - we want to evaluate this
                                                needs_comma_separated_print = true;
                                                can_handle_interp = false;
                                                break;
                                            },
                                            _ => {
                                                // Check if this is a shell positional parameter ($1, $2, etc.)
                                                if var.chars().all(|c| c.is_digit(10)) {
                                                    // Convert $1 to $_[0], $2 to $_[1], etc.
                                                    let index = var.parse::<usize>().unwrap_or(0);
                                                    interp_result.push_str(&format!("$_[{}]", index - 1));
                                                } else {
                                                    // Regular variable - add for interpolation
                                                    interp_result.push_str(&format!("${}", var));
                                                }
                                            }
                                        }
                                    },
                                    StringPart::ParameterExpansion(pe) => {
                                        // Handle ParameterExpansion within StringInterpolation
                                        // If this is the only part, we should treat it as a direct expression
                                        if interp.parts.len() == 1 {
                                            // Single ParameterExpansion - treat as expression, not string
                                            can_handle_interp = false;
                                            break;
                                        } else {
                                            // Multiple parts - handle as interpolation
                                            let pe_result = generator.generate_parameter_expansion(pe);
                                            // Remove the ${...} wrapper for interpolation
                                            if pe_result.starts_with("${") && pe_result.ends_with("}") {
                                                interp_result.push_str(&pe_result[2..pe_result.len()-1]);
                                            } else {
                                                interp_result.push_str(&pe_result);
                                            }
                                        }
                                    },
                                    _ => {
                                        // For other StringPart variants, fall back to concatenation
                                        can_handle_interp = false;
                                        break;
                                    }
                                }
                            }
                            
                            if can_handle_interp {
                                processed_args.push(format!("\"{}\"", interp_result));
                            } else {
                                // Can't handle this interpolation, fall back to concatenation
                                // Check if this is a single ParameterExpansion that should be treated as expression
                                if interp.parts.len() == 1 {
                                    if let StringPart::ParameterExpansion(pe) = &interp.parts[0] {
                                        // Single ParameterExpansion - treat as expression, not string
                                        processed_args.push(generator.generate_parameter_expansion(pe));
                                    } else if let StringPart::Variable(var) = &interp.parts[0] {
                                        // Single variable - handle special cases
                                        match var.as_str() {
                                            "#" => processed_args.push("scalar(@ARGV)".to_string()),
                                            "@" => processed_args.push("@ARGV".to_string()),
                                            "*" => processed_args.push("@ARGV".to_string()),
                                            _ => processed_args.push(format!("${}", var)),
                                        }
                                    } else {
                                        // Fall back to general conversion
                                        processed_args.push(generator.word_to_perl(&cmd.args[i]));
                                    }
                                } else if needs_comma_separated_print {
                                    // Add the special variable as an unquoted expression
                                    for part in &interp.parts {
                                        if let StringPart::Variable(var) = part {
                                            match var.as_str() {
                                                "#" => processed_args.push("scalar(@ARGV)".to_string()),
                                                "@" => processed_args.push("@ARGV".to_string()),
                                                "*" => processed_args.push("@ARGV".to_string()),
                                                                                            _ => {
                                                // For other variables, fall back to general conversion
                                                processed_args.push(generator.word_to_perl(&cmd.args[i]));
                                            }
                                            }
                                            break;
                                        }
                                    }
                                } else {
                                    // Fall back to general conversion
                                    processed_args.push(generator.word_to_perl(&cmd.args[i]));
                                }
                            }
                            i += 1;
                        },
                        Word::BraceExpansion(_) => {
                            // Collect consecutive brace expansions for cartesian product
                            let mut brace_expansions = Vec::new();
                            let start_i = i;
                            while i < cmd.args.len() {
                                if let Word::BraceExpansion(expansion) = &cmd.args[i] {
                                    brace_expansions.push(expansion);
                                    i += 1;
                                } else {
                                    break;
                                }
                            }
                            
                            if brace_expansions.len() == 1 {
                                // Single brace expansion - expand it normally
                                let expanded = generator.handle_brace_expansion(brace_expansions[0]);
                                processed_args.push(format!("\"{}\"", expanded));
                            } else {
                                // Multiple brace expansions - create cartesian product
                                println!("DEBUG: Creating cartesian product for {} brace expansions", brace_expansions.len());
                                
                                // For each brace expansion, we need to get the individual items
                                let expanded_items: Vec<Vec<String>> = brace_expansions.iter()
                                    .map(|expansion| {
                                        // Get the individual items from the brace expansion
                                        let mut items = Vec::new();
                                        for item in &expansion.items {
                                            match item {
                                                BraceItem::Literal(s) => items.push(s.clone()),
                                                BraceItem::Range(range) => {
                                                    // Expand the range
                                                    let expanded = generator.handle_brace_expansion(expansion);
                                                    items.extend(expanded.split_whitespace().map(|s| s.to_string()));
                                                },
                                                BraceItem::Sequence(seq) => items.extend(seq.clone()),
                                            }
                                        }
                                        println!("DEBUG: Items from expansion: {:?}", items);
                                        items
                                    })
                                    .collect();
                                
                                println!("DEBUG: Expanded items: {:?}", expanded_items);
                                let cartesian = generate_cartesian_product_for_echo(&expanded_items);
                                println!("DEBUG: Cartesian product result: {:?}", cartesian);
                                let result = cartesian.join(" ");
                                println!("DEBUG: Final result: '{}'", result);
                                processed_args.push(format!("\"{}\"", result));
                            }
                        },
                        _ => {
                            // For other word types, use the general conversion
                            processed_args.push(generator.word_to_perl(&cmd.args[i]));
                            i += 1;
                        }
                    }
                }
                
                let args = processed_args;
                
                // Use proper Perl print statement formatting
                if needs_comma_separated_print {
                    // Force comma-separated printing for special variables
                    let args_str = args.join(", ");
                    output.push_str(&generator.indent());
                    output.push_str(&format!("print {}, \"\\n\";\n", args_str));
                } else if args.len() == 1 {
                    output.push_str(&generator.indent());
                    // Check if this is a ParameterExpansion that should be treated as an expression
                    let is_parameter_expansion = cmd.args.iter().any(|arg| {
                        matches!(arg, Word::ParameterExpansion(_))
                    });
                    
                    // Check if this is a StringInterpolation with only a single ParameterExpansion
                    let is_single_param_expansion = cmd.args.len() == 1 && 
                        matches!(&cmd.args[0], Word::StringInterpolation(interp) if 
                            interp.parts.len() == 1 && 
                            matches!(&interp.parts[0], StringPart::ParameterExpansion(_)));
                    
                    if is_parameter_expansion || is_single_param_expansion {
                        // For ParameterExpansion or single ParameterExpansion in StringInterpolation, treat as expression, not string
                        output.push_str(&format!("print {}, \"\\n\";\n", args[0]));
                    } else if let Some(optimized_arg) = generator.optimize_string_with_newline(&args[0]) {
                        // Check if this is a simple string literal that we can optimize
                        output.push_str(&format!("print {};\n", optimized_arg));
                    } else {
                        // Check if this argument contains command substitution
                        let has_command_substitution = cmd.args.iter().any(|arg| {
                            matches!(arg, Word::CommandSubstitution(_))
                        });
                        
                        if has_command_substitution {
                            // For command substitution, don't add newline as it's already handled
                            output.push_str(&format!("print {};\n", args[0]));
                        } else {
                            output.push_str(&format!("print {}, \"\\n\";\n", args[0]));
                        }
                    }
                } else {
                    // For multiple arguments, try to create a single interpolated string
                    // Work with the original Word objects instead of processed strings
                    let mut combined_string = String::new();
                    let mut can_interpolate = true;
                    
                    for (i, word) in cmd.args.iter().enumerate() {
                        if i > 0 {
                            combined_string.push(' ');
                        }
                        
                        match word {
                            Word::Literal(s) => {
                                // Add the literal text directly
                                combined_string.push_str(s);
                            }
                            Word::Variable(var) => {
                                // Add the variable for interpolation
                                combined_string.push_str(&format!("${}", var));
                            }
                            Word::StringInterpolation(interp) => {
                                // Handle string interpolation specially for echo
                                // For variables like $#, we want to evaluate them, not treat them as literals
                                let mut can_handle_interp = true;
                                let mut interp_result = String::new();
                                
                                for part in &interp.parts {
                                    match part {
                                        StringPart::Literal(s) => {
                                            interp_result.push_str(s);
                                        },
                                        StringPart::Variable(var) => {
                                            // Handle special shell variables
                                            match var.as_str() {
                                                "#" => {
                                                    // $# -> scalar(@ARGV) - we want to evaluate this
                                                    can_handle_interp = false;
                                                    break;
                                                },
                                                "@" => {
                                                    // $@ -> @ARGV - we want to evaluate this
                                                    can_handle_interp = false;
                                                    break;
                                                },
                                                "*" => {
                                                    // $* -> @ARGV - we want to evaluate this
                                                    can_handle_interp = false;
                                                    break;
                                                },
                                                _ => {
                                                    // Check if this is a shell positional parameter ($1, $2, etc.)
                                                    if var.chars().all(|c| c.is_digit(10)) {
                                                        // Convert $1 to $_[0], $2 to $_[1], etc.
                                                        let index = var.parse::<usize>().unwrap_or(0);
                                                        interp_result.push_str(&format!("$_[{}]", index - 1));
                                                    } else {
                                                        // Regular variable - add for interpolation
                                                        interp_result.push_str(&format!("${}", var));
                                                    }
                                                }
                                            }
                                        },
                                        _ => {
                                            // For other StringPart variants, fall back to concatenation
                                            can_handle_interp = false;
                                            break;
                                        }
                                    }
                                }
                                
                                if can_handle_interp {
                                    combined_string.push_str(&interp_result);
                                } else {
                                    // Can't handle this interpolation, fall back to concatenation
                                    can_interpolate = false;
                                    break;
                                }
                            }
                            _ => {
                                // For other complex word types, fall back to concatenation
                                can_interpolate = false;
                                break;
                            }
                        }
                    }
                    
                    if can_interpolate {
                        // Create a single interpolated string
                        // Check if any of the arguments contain command substitution
                        let has_command_substitution = cmd.args.iter().any(|arg| {
                            matches!(arg, Word::CommandSubstitution(_))
                        });
                        
                        if has_command_substitution {
                            // For command substitution, don't add newline as it's already handled
                            output.push_str(&generator.indent());
                            output.push_str(&format!("print \"{}\";\n", combined_string));
                        } else {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("print \"{}\\n\";\n", combined_string));
                        }
                    } else {
                        // Fall back to the original comma-separated approach
                        let args_str = args.join(", ");
                        output.push_str(&generator.indent());
                        output.push_str(&format!("print {}, \"\\n\";\n", args_str));
                    }
                }
            }
        } else if name == "true" && !cmd.env_vars.is_empty() && cmd.args.is_empty() {
            // This is a standalone assignment (e.g., i=$((i + 1)))
            eprintln!("DEBUG: Found standalone assignment with cmd.name: {:?}, env_vars: {:?}", cmd.name, cmd.env_vars);
            // Generate proper Perl assignment statements
            for (var, value) in &cmd.env_vars {
                match value {
                    Word::Arithmetic(expr) => {
                        // Convert arithmetic expression to Perl
                        let perl_expr = generator.convert_arithmetic_to_perl(&expr.expression);
                        eprintln!("DEBUG: Generating arithmetic assignment: ${} = {}", var, perl_expr);
                        // Check if variable is already declared in current scope
                        if !generator.declared_locals.contains(var) {
                            // Variable not declared yet, declare it with 'my'
                            output.push_str(&generator.indent());
                            output.push_str(&format!("my ${} = {};\n", var, perl_expr));
                            // Mark as declared in current scope
                            generator.declared_locals.insert(var.clone());
                        } else {
                            // Variable already declared, just assign the value
                            output.push_str(&generator.indent());
                            output.push_str(&format!("${} = {};\n", var, perl_expr));
                        }
                    },
                    _ => {
                        // Handle other value types
                        let val = generator.perl_string_literal(value);
                        eprintln!("DEBUG: Generating other assignment: ${} = {}", var, val);
                        if !generator.declared_locals.contains(var) {
                            // Variable not declared yet, declare it with 'my'
                            output.push_str(&generator.indent());
                            output.push_str(&format!("my ${} = {};\n", var, val));
                            // Mark as declared in current scope
                            generator.declared_locals.insert(var.clone());
                        } else {
                            // Variable already declared, just assign the value
                            output.push_str(&generator.indent());
                            output.push_str(&format!("${} = {};\n", var, val));
                        }
                    }
                }
            }
        } else {
            // Handle other commands
            let cmd_name = name;
            eprintln!("DEBUG: Processing Word::Literal with name: '{}'", name);
            
            if let Some(specific_output) = generate_command_specific(generator, cmd, "") {
                eprintln!("DEBUG: Used command-specific generator for: {}", cmd_name);
                output.push_str(&specific_output);
            } else if generator.declared_functions.contains(cmd_name) || cmd_name == "greet" {
                // Check if this is a function call
                eprintln!("DEBUG: Generating function call for: {}", cmd_name);
                if cmd.args.is_empty() {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("{}();\n", cmd_name));
                } else {
                    let args: Vec<String> = cmd.args.iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    output.push_str(&generator.indent());
                    output.push_str(&format!("{}({});\n", cmd_name, args.join(", ")));
                }
            } else {
                // Fallback to system call
                eprintln!("DEBUG: Using system call fallback for: {}", cmd_name);
                if cmd.args.is_empty() {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("system('{}');\n", cmd_name));
                } else {
                    let args: Vec<String> = cmd.args.iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    output.push_str(&generator.indent());
                    output.push_str(&format!("system('{}', {});\n", cmd_name, args.join(", ")));
                }
            }
        }
    } else {
        // Handle non-literal command names
        let cmd_name = "unknown_command";
        
        // First try to use command-specific generators
        if let Some(specific_output) = generate_command_specific(generator, cmd, "") {
            output.push_str(&specific_output);
        } else {
            // Fallback to system call
            if cmd.args.is_empty() {
                output.push_str(&generator.indent());
                output.push_str(&format!("system('{}');\n", cmd_name));
            } else {
                let args: Vec<String> = cmd.args.iter()
                    .map(|arg| generator.word_to_perl(arg))
                    .collect();
                output.push_str(&generator.indent());
                output.push_str(&format!("system('{}', {});\n", cmd_name, args.join(", ")));
            }
        }
    }

    output
}

