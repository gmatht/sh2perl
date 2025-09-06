use crate::ast::*;
use crate::generator::Generator;

pub fn generate_comm_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: usize) -> String {
    let mut output = String::new();
    
    // comm compares two sorted files and shows lines unique to each
    // Format: comm [-123] file1 file2
    // Output columns: lines unique to file1, lines unique to file2, lines common to both
    
    let mut suppress_col1 = false;
    let mut suppress_col2 = false;
    let mut suppress_col3 = false;
    
    // Parse options
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('-') {
                if s.contains('1') { suppress_col1 = true; }
                if s.contains('2') { suppress_col2 = true; }
                if s.contains('3') { suppress_col3 = true; }
            }
        }
    }
    
    // Check if we have process substitution redirects
    let mut has_process_sub = false;
    for redir in &cmd.redirects {
        if matches!(redir.operator, RedirectOperator::ProcessSubstitutionInput(_)) {
            has_process_sub = true;
            break;
        }
    }
    
    if has_process_sub {
        // For process substitution, we need to handle the files differently
        // The process substitution files will be created by the main generator
        // We'll read from them directly
        
        output.push_str("my @file1_lines;\n");
        output.push_str("my @file2_lines;\n");
        
        // Find the process substitution files
        let mut ps_files = Vec::new();
        for redir in &cmd.redirects {
            if matches!(redir.operator, RedirectOperator::ProcessSubstitutionInput(_)) {
                if let Word::Literal(target, _) = &redir.target {
                    if target.starts_with("/tmp/process_sub_") {
                        ps_files.push(target.clone());
                    }
                }
            }
        }
        
        if ps_files.len() >= 2 {
            let file1 = &ps_files[0];
            let file2 = &ps_files[1];
            
            // Read first file
            output.push_str(&format!("if (open(my $fh1, '<', '{}')) {{\n", file1));
            output.push_str("    while (my $line = <$fh1>) {\n");
            output.push_str("        chomp($line);\n");
            output.push_str("        push @file1_lines, $line;\n");
            output.push_str("    }\n");
            output.push_str("    close($fh1);\n");
            output.push_str("}\n");
            
            // Read second file
            output.push_str(&format!("if (open(my $fh2, '<', '{}')) {{\n", file2));
            output.push_str("    while (my $line = <$fh2>) {\n");
            output.push_str("        chomp($line);\n");
            output.push_str("        push @file2_lines, $line;\n");
            output.push_str("    }\n");
            output.push_str("    close($fh2);\n");
            output.push_str("}\n");
            
            // Create hashes for efficient lookup
            output.push_str("my %file1_set = map { $_ => 1 } @file1_lines;\n");
            output.push_str("my %file2_set = map { $_ => 1 } @file2_lines;\n");
            
            // Find common lines
            output.push_str("my @common_lines;\n");
            output.push_str("foreach my $line (@file1_lines) {\n");
            output.push_str("    if (exists($file2_set{$line})) {\n");
            output.push_str("        push @common_lines, $line;\n");
            output.push_str("    }\n");
            output.push_str("}\n");
            
            // Generate output based on suppression flags
            output.push_str(&format!("${} = \"\";\n", input_var));
            
            if !suppress_col1 {
                output.push_str("foreach my $line (@file1_lines) {\n");
                output.push_str("    if (!exists($file2_set{$line})) {\n");
                output.push_str(&format!("        ${} .= $line . \"\\n\";\n", input_var));
                output.push_str("    }\n");
                output.push_str("}\n");
            }
            
            if !suppress_col2 {
                output.push_str("foreach my $line (@file2_lines) {\n");
                output.push_str("    if (!exists($file1_set{$line})) {\n");
                output.push_str(&format!("        ${} .= $line . \"\\n\";\n", input_var));
                output.push_str("    }\n");
                output.push_str("}\n");
            }
            
            if !suppress_col3 {
                output.push_str(&format!("${} .= join \"\\n\", @common_lines . \"\\n\";\n", input_var));
            }
            
            // Remove trailing newline
            output.push_str(&format!("chomp(${}); \n", input_var));
        } else {
            // Fallback: treat input as a single file
            output.push_str(&format!("my @lines = split /\\n/msx, ${};\n", input_var));
            output.push_str("my %seen;\n");
            output.push_str("my @result;\n");
            output.push_str("foreach my $line (@lines) {\n");
            output.push_str("    chomp($line);\n");
            output.push_str("    if (!exists($seen{$line})) {\n");
            output.push_str("        $seen{$line} = 1;\n");
            output.push_str("        push @result, $line;\n");
            output.push_str("    } else {\n");
            output.push_str("        $seen{$line}++;\n");
            output.push_str("    }\n");
            output.push_str("}\n");
            output.push_str(&format!("${} = join \"\\n\", @result;\n", input_var));
        }
    } else {
        // For now, implement a basic version that works with input
        // The process substitution files will be handled by the main generator
        output.push_str(&format!("my @lines = split /\\n/, ${};\n", input_var));
        output.push_str("my %seen;\n");
        output.push_str("my @result;\n");
        output.push_str("foreach my $line (@lines) {\n");
        output.push_str("    chomp($line);\n");
        output.push_str("    if (!exists($seen{$line})) {\n");
        output.push_str("        $seen{$line} = 1;\n");
        output.push_str("        push @result, $line;\n");
        output.push_str("    } else {\n");
        output.push_str("        $seen{$line}++;\n");
        output.push_str("    }\n");
        output.push_str("}\n");
        output.push_str(&format!("${} = join \"\\n\", @result;\n", input_var));
    }
    
    output
}
