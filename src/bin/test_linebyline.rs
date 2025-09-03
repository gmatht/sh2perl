use std::collections::HashMap;
use debashl::ast::*;
use debashl::generator::commands::builtins::{get_builtin_commands, pipeline_supports_linebyline};

fn main() {
    // Test the line-by-line pipeline support
    println!("Testing line-by-line pipeline support...\n");
    
    // Show which commands support line-by-line processing
    let builtins = get_builtin_commands();
    println!("Commands that support line-by-line processing:");
    for (name, builtin) in &builtins {
        if builtin.supports_linebyline {
            println!("  ✓ {} - {}", name, builtin.description);
        }
    }
    
    println!("\nCommands that do NOT support line-by-line processing:");
    for (name, builtin) in &builtins {
        if !builtin.supports_linebyline {
            println!("  ✗ {} - {}", name, builtin.description);
        }
    }
    
    // Test pipeline analysis
    println!("\n=== Pipeline Analysis Examples ===");
    
    // Example 1: All commands support line-by-line
    let pipeline1 = Pipeline {
        source_text: Some("cat | grep 'test' | wc -l".to_string()),
        stdout_used: true,
        stderr_used: true,
        commands: vec![
            Command::Simple(SimpleCommand {
                name: Word::literal("cat".to_string()),
                args: vec![],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
            Command::Simple(SimpleCommand {
                name: Word::literal("tr".to_string()),
                args: vec![Word::literal("a".to_string()), Word::literal("b".to_string())],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
            Command::Simple(SimpleCommand {
                name: Word::literal("grep".to_string()),
                args: vec![Word::literal("hello".to_string())],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
        ],
    };
    
    println!("\nPipeline 1: cat | tr 'a' 'b' | grep 'hello'");
    if pipeline_supports_linebyline(&pipeline1) {
        println!("  ✓ This pipeline can use line-by-line processing!");
        println!("  → Will generate streaming Perl code");
    } else {
        println!("  ✗ This pipeline cannot use line-by-line processing");
        println!("  → Will fall back to buffered processing");
    }
    
    // Example 2: Some commands don't support line-by-line
    let pipeline2 = Pipeline {
        source_text: Some("ls | tr 'a-z' 'A-Z' | sort".to_string()),
        stdout_used: true,
        stderr_used: true,
        commands: vec![
            Command::Simple(SimpleCommand {
                name: Word::literal("cat".to_string()),
                args: vec![],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
            Command::Simple(SimpleCommand {
                name: Word::literal("sort".to_string()),
                args: vec![],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
            Command::Simple(SimpleCommand {
                name: Word::literal("grep".to_string()),
                args: vec![Word::literal("hello".to_string())],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
        ],
    };
    
    println!("\nPipeline 2: cat | sort | grep 'hello'");
    if pipeline_supports_linebyline(&pipeline2) {
        println!("  ✓ This pipeline can use line-by-line processing!");
        println!("  → Will generate streaming Perl code");
    } else {
        println!("  ✗ This pipeline cannot use line-by-line processing");
        println!("  → Will fall back to buffered processing");
        println!("  → Reason: 'sort' requires all input to sort properly");
    }
    
    // Example 3: Non-builtin command
    let pipeline3 = Pipeline {
        source_text: Some("head -n 10 | tail -n 5 | cut -d' ' -f1".to_string()),
        stdout_used: true,
        stderr_used: true,
        commands: vec![
            Command::Simple(SimpleCommand {
                name: Word::literal("cat".to_string()),
                args: vec![],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
            Command::Simple(SimpleCommand {
                name: Word::literal("custom_script".to_string()),
                args: vec![],
                redirects: vec![],
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            }),
        ],
    };
    
    println!("\nPipeline 3: cat | custom_script");
    if pipeline_supports_linebyline(&pipeline3) {
        println!("  ✓ This pipeline can use line-by-line processing!");
        println!("  → Will generate streaming Perl code");
    } else {
        println!("  ✗ This pipeline cannot use line-by-line processing");
        println!("  → Will fall back to buffered processing");
        println!("  → Reason: 'custom_script' is not a builtin command");
    }
    
    println!("\n=== Summary ===");
    println!("Line-by-line processing provides:");
    println!("  • Memory efficiency (no buffering entire files)");
    println!("  • True streaming (process data as it arrives)");
    println!("  • Better performance for large files");
    println!("  • Shell-like behavior");
    
    println!("\nWhen line-by-line is not possible:");
    println!("  • Falls back to buffered processing");
    println!("  • Maintains compatibility");
    println!("  • Still generates optimized Perl code");
}
