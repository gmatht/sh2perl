use crate::ast::*;
use crate::generator::Generator;

pub fn generate_touch_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // touch command syntax: touch [options] file...
    let mut files = Vec::new();
    
    // Parse touch options and expand brace expansions
    for arg in &cmd.args {
        match arg {
            Word::Literal(arg_str, _) => {
                if !arg_str.starts_with('-') {
                    files.push(format!("\"{}\"", arg_str));
                }
            }
            Word::BraceExpansion(expansion, _) => {
                // Handle brace expansion by storing it as a special marker for later processing
                let mut expanded_items = Vec::new();
                if expansion.items.len() == 1 {
                    match &expansion.items[0] {
                        BraceItem::Range(range) => {
                            // Convert {001..005} to individual numbers
                            if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
                                let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
                                let mut current = start_num;
                                if step > 0 {
                                    while current <= end_num {
                                        // Preserve leading zeros by formatting with the same width as the original
                                        let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                                            format!("{:0width$}", current, width = range.start.len())
                                        } else {
                                            current.to_string()
                                        };
                                        expanded_items.push(formatted);
                                        current += step;
                                    }
                                } else {
                                    while current >= end_num {
                                        let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                                            format!("{:0width$}", current, width = range.start.len())
                                        } else {
                                            current.to_string()
                                        };
                                        expanded_items.push(formatted);
                                        current += step;
                                    }
                                }
                            } else {
                                // Fallback for non-numeric ranges
                                files.push(generator.word_to_perl(arg));
                                continue;
                            }
                        }
                        BraceItem::Literal(s) => {
                            expanded_items.push(s.clone());
                        }
                        BraceItem::Sequence(seq) => {
                            expanded_items.extend(seq.clone());
                        }
                    }
                } else {
                    // Multiple brace items - expand each one
                    for item in &expansion.items {
                        match item {
                            BraceItem::Literal(s) => expanded_items.push(s.clone()),
                            BraceItem::Range(range) => {
                                if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
                                    let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
                                    let mut current = start_num;
                                    if step > 0 {
                                        while current <= end_num {
                                            let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                                                format!("{:0width$}", current, width = range.start.len())
                                            } else {
                                                current.to_string()
                                            };
                                            expanded_items.push(formatted);
                                            current += step;
                                        }
                                    } else {
                                        while current >= end_num {
                                            let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                                                format!("{:0width$}", current, width = range.start.len())
                                            } else {
                                                current.to_string()
                                            };
                                            expanded_items.push(formatted);
                                            current += step;
                                        }
                                    }
                                } else {
                                    expanded_items.push(range.start.clone());
                                }
                            }
                            BraceItem::Sequence(seq) => {
                                expanded_items.extend(seq.clone());
                            }
                        }
                    }
                }
                // Store as a special marker that can be detected later
                files.push(format!("BRACE_EXPANSION:{}", expanded_items.join(" ")));
            }
            _ => {
                files.push(generator.word_to_perl(arg));
            }
        }
    }
    
    if files.is_empty() {
        output.push_str("croak \"touch: missing file operand\\n\";\n");
    } else {
        
        // Handle the case where we have prefix + brace expansion + suffix
        // This happens when the parser separates file_{001..005}.txt into [file_, {001..005}, .txt]
        let mut expanded_files = Vec::new();
        let mut i = 0;
        
        while i < files.len() {
            if i + 2 < files.len() {
                // Check if we have a pattern like: literal + brace_expansion + literal
                // This suggests prefix + expansion + suffix
                let current = &files[i];
                let next = &files[i + 1];
                let next_next = &files[i + 2];
                
                // If the first and third are literals and the middle is a brace expansion result
                if current.starts_with('"') && next_next.starts_with('"') && next.starts_with("BRACE_EXPANSION:") {
                    // This looks like prefix + expansion + suffix
                    let prefix = current.trim_matches('"');
                    let suffix = next_next.trim_matches('"');
                    
                    // Extract the expansion items
                    let expansion_content = &next[16..]; // Remove "BRACE_EXPANSION:" prefix
                    let items: Vec<String> = expansion_content.split_whitespace()
                        .map(|item| format!("\"{}{}{}\"", prefix, item, suffix))
                        .collect();
                    
                    expanded_files.extend(items);
                    i += 3; // Skip all three
                    continue;
                }
            }
            
            // Check if this is a standalone brace expansion (no prefix/suffix)
            if files[i].starts_with("BRACE_EXPANSION:") {
                let expansion_content = &files[i][16..]; // Remove "BRACE_EXPANSION:" prefix
                let items: Vec<String> = expansion_content.split_whitespace()
                    .map(|item| format!("\"{}\"", item))
                    .collect();
                expanded_files.extend(items);
                i += 1;
                continue;
            }
            
            // Regular case: just add the file
            expanded_files.push(files[i].clone());
            i += 1;
        }
        
        for file in &expanded_files {
            let quoted_file = if file.starts_with('"') || file.starts_with("'") {
                file.clone()
            } else {
                format!("\"{}\"", file)
            };
            output.push_str(&format!("if (-e {}) {{\n", quoted_file));
            // File exists, update timestamp
            output.push_str(&format!("my $current_time = time;\n"));
            output.push_str(&format!("utime $current_time, $current_time, {};\n", quoted_file));
            // Silent operation - no output unless error
            output.push_str("} else {\n");
            // File doesn't exist, create it
            output.push_str(&format!("if (open my $fh, '>', {}) {{\n", quoted_file));
            output.push_str("close $fh or croak \"Close failed: $ERRNO\";\n");
            // Silent operation - no output unless error
            output.push_str("} else {\n");
            output.push_str(&format!("croak \"touch: cannot create \", {}, \": $ERRNO\\n\";\n", quoted_file));
            output.push_str("}\n");
            output.push_str("}\n");
        }
    }
    
    output
}
