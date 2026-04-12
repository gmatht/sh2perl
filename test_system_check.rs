use std::fs;

fn main() {
    let perl_code = fs::read_to_string("test_direct_system_check.pl").expect("Failed to read test file");
    
    // Test the regex pattern directly
    use regex::Regex;
    
    let builtin_commands = [
        "find", "ls", "grep", "sed", "awk", "sort", "uniq", "wc", "head", "tail",
        "cat", "echo", "printf", "touch", "mkdir", "rmdir", "rm", "cp", "mv",
        "chmod", "chown", "ln", "readlink", "realpath", "basename", "dirname",
        "date", "sleep", "kill", "ps", "jobs", "fg", "bg", "wait", "nohup",
        "cd", "pwd", "pushd", "popd", "dirs", "hash", "type", "which", "whereis",
        "man", "info", "help", "history", "alias", "unalias", "set", "unset",
        "export", "readonly", "declare", "local", "read", "printf", "echo",
        "test", "[", "[[", "let", "expr", "bc", "dc", "seq", "factor", "yes",
        "true", "false", "strings", ":", "exit", "return", "break",
        "continue", "shift", "unshift", "pop", "push", "splice", "join", "split"
    ];
    
    let mut violations = Vec::new();
    
    for builtin in &builtin_commands {
        let pattern = format!("system\\s*['\"]{}['\"]", regex::escape(builtin));
        let regex = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(e) => {
                eprintln!("Warning: Failed to create regex for builtin '{}': {}", builtin, e);
                continue;
            }
        };
        
        for mat in regex.find_iter(&perl_code) {
            let line_num = perl_code[..mat.start()].matches('\n').count() + 1;
            violations.push(format!(
                "Line {}: Builtin command '{}' is using system() call instead of native Perl implementation",
                line_num, builtin
            ));
        }
    }
    
    if violations.is_empty() {
        println!("No violations found");
    } else {
        println!("Violations found:\n{}", violations.join("\n"));
    }
}



