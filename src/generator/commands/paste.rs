use crate::generator::Generator;
use crate::ast::*;

pub fn generate_paste_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    process_sub_files: &[(String, String)],
) -> String {
    let mut result = String::new();
    
    if !process_sub_files.is_empty() {
        // Handle process substitution case
        if process_sub_files.len() >= 2 {
            let file1 = &process_sub_files[0];
            let file2 = &process_sub_files[1];
            
            // Read both files and paste them together
            let paste_id = generator.get_unique_file_handle();
            result.push_str(&generator.indent());
            result.push_str(&format!("my @paste_file1_lines_{};\n", paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("my @paste_file2_lines_{};\n", paste_id));
            
            // Read first file
            result.push_str(&generator.indent());
            result.push_str(&format!("if (open my $fh1, '<', ${}) {{\n", file1.0));
            result.push_str(&generator.indent());
            result.push_str("    while (my $line = <$fh1>) {\n");
            result.push_str(&generator.indent());
            result.push_str("        chomp($line);\n");
            result.push_str(&generator.indent());
            result.push_str(&format!("        push @paste_file1_lines_{}, $line;\n", paste_id));
            result.push_str(&generator.indent());
            result.push_str("    }\n");
            result.push_str(&generator.indent());
            result.push_str("    close $fh1;\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");
            
            // Read second file
            result.push_str(&generator.indent());
            result.push_str(&format!("if (open my $fh2, '<', ${}) {{\n", file2.0));
            result.push_str(&generator.indent());
            result.push_str("    while (my $line = <$fh2>) {\n");
            result.push_str(&generator.indent());
            result.push_str("        chomp($line);\n");
            result.push_str(&generator.indent());
            result.push_str(&format!("        push @paste_file2_lines_{}, $line;\n", paste_id));
            result.push_str(&generator.indent());
            result.push_str("    }\n");
            result.push_str(&generator.indent());
            result.push_str("    close $fh2;\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");
            
            // Paste the lines together
            result.push_str(&generator.indent());
            result.push_str(&format!("my $max_lines = @paste_file1_lines_{} > @paste_file2_lines_{} ? @paste_file1_lines_{} : @paste_file2_lines_{};\n", paste_id, paste_id, paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str("for (my $i = 0; $i < $max_lines; $i++) {\n");
            result.push_str(&generator.indent());
            result.push_str(&format!("    my $line1 = $i < @paste_file1_lines_{} ? $paste_file1_lines_{}[$i] : q{{}};\n", paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("    my $line2 = $i < @paste_file2_lines_{} ? $paste_file2_lines_{}[$i] : q{{}};\n", paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str("    print \"$line1\\t$line2\\n\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");
        }
    } else {
        // Handle regular paste command (fallback to system)
        let args: Vec<String> = cmd.args.iter()
            .map(|arg| generator.word_to_perl(arg))
            .collect();
        
        if !args.is_empty() {
            result.push_str(&generator.indent());
            result.push_str(&format!("system('paste {}');\n", args.join(" ")));
        } else {
            result.push_str(&generator.indent());
            result.push_str("system('paste');\n");
        }
    }
    
    result
}
