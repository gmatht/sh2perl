use std::fs;

// Copy the function from utils.rs for testing
fn check_perl_no_system_builtins(perl_code: &str) -> Result<(), String> {
    use regex::Regex;
    
    eprintln!("DEBUG: check_perl_no_system_builtins called with {} characters", perl_code.len());
    
    // List of builtin commands that should not use system() calls
    let builtin_commands = [
        "find", "ls", "grep", "sed", "awk", "sort", "uniq", "wc", "head", "tail",
        "cat", "echo", "printf", "touch", "mkdir", "rmdir", "rm", "cp", "mv",
        "chmod", "chown", "ln", "readlink", "realpath", "basename", "dirname",
        "date", "sleep", "kill", "ps", "jobs", "fg", "bg", "wait", "nohup",
        "cd", "pwd", "pushd", "popd", "dirs", "hash", "type", "which", "whereis",
        "man", "info", "help", "history", "alias", "unalias", "set", "unset",
        "export", "readonly", "declare", "local", "read", "printf", "echo",
        "test", "[", "[[", "let", "expr", "bc", "dc", "seq", "factor", "yes",
        "true", "false", ":", "true", "false", "exit", "return", "break",
        "continue", "shift", "unshift", "pop", "push", "splice", "join", "split"
    ];
    
    let mut violations = Vec::new();
    
    // Create regex to match system calls with builtin commands
    for builtin in &builtin_commands {
        // Match patterns like: system 'BUILTIN' or system "BUILTIN"
        let pattern = format!("system\\s*['\"]{}['\"]", regex::escape(builtin));
        let regex = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(e) => {
                eprintln!("Warning: Failed to create regex for builtin '{}': {}", builtin, e);
                continue;
            }
        };
        
        // Find all matches
        for mat in regex.find_iter(perl_code) {
            let line_num = perl_code[..mat.start()].matches('\n').count() + 1;
            violations.push(format!(
                "Line {}: Builtin command '{}' is using system() call instead of native Perl implementation",
                line_num, builtin
            ));
        }
    }
    
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("SYSTEM_BUILTIN violations:\n{}", violations.join("\n")))
    }
}

fn main() {
    let perl_code = fs::read_to_string("test_system_builtins.pl").expect("Failed to read test file");
    
    match check_perl_no_system_builtins(&perl_code) {
        Ok(()) => println!("No violations found"),
        Err(msg) => println!("Violations found:\n{}", msg),
    }
}



