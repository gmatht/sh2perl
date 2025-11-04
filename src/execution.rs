use std::process::{Command, Stdio};
use crate::timeout_manager::{OperationType, execute_with_timeout};

/// Cross-platform helper to create ExitStatus from exit code
/// This is a workaround since ExitStatus::from_raw is platform-specific
pub fn create_exit_status(exit_code: i32) -> std::process::ExitStatus {
    // On Unix systems, we can use the exit code directly
    // On Windows, we'll need to handle this differently
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(exit_code)
    }
    
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(exit_code.try_into().unwrap_or(0))
    }
    
    #[cfg(not(any(unix, windows)))]
    {
        // Fallback for other platforms - create a mock exit status
        // This is not ideal but allows compilation on other platforms
        let mut status = std::process::ExitStatus::default();
        // Note: This won't have the correct exit code, but it allows compilation
        status
    }
}

/// Cross-platform function to run shell scripts
/// Optimized for speed - runs bash directly on the script with fine-grained timeout
pub fn run_shell_script(filename: &str) -> Result<std::process::Output, String> {
    // Extract just the filename part from the full path
    let script_name = filename.split(['\\', '/']).last().unwrap_or(filename);
    
    // Use the timeout manager for shell execution
    let script_name = script_name.to_string();
    execute_with_timeout(OperationType::ShellExecution, move || {
        // Run bash directly on the script file - much faster than bash -c "bash script"
        let mut cmd = Command::new("bash");
        let examples_dir = std::env::current_dir().unwrap_or_default().join("examples");
        cmd.current_dir(&examples_dir);
        // Set TZ=UTC to avoid date differences between shell and Perl
        cmd.env("TZ", "UTC");
        cmd.arg(&script_name);
        
        let child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(e) => { 
                return Err(format!("Failed to start bash script: {}", e)); 
            }
        };
        
        // Wait for the process to complete
        match child.wait_with_output() {
            Ok(output) => Ok(output),
            Err(e) => Err(format!("Error waiting for script {}: {}", script_name, e)),
        }
    })
}

/// Run Perl script with fine-grained timeout
pub fn run_perl_script(script_path: &str) -> Result<std::process::Output, String> {
    let script_path = script_path.to_string();
    execute_with_timeout(OperationType::PerlExecution, move || {
        let mut cmd = Command::new("perl");
        cmd.arg(&script_path);
        
        let child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(e) => { 
                return Err(format!("Failed to start Perl script: {}", e)); 
            }
        };
        
        // Wait for the process to complete
        match child.wait_with_output() {
            Ok(output) => Ok(output),
            Err(e) => Err(format!("Error waiting for Perl script {}: {}", script_path, e)),
        }
    })
}
