use std::process::{Command, Stdio};

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
/// Optimized for speed - runs bash directly on the script
pub fn run_shell_script(filename: &str) -> Result<std::process::Output, String> {
    // Extract just the filename part from the full path
    let script_name = filename.split(['\\', '/']).last().unwrap_or(filename);
    
    // Run bash directly on the script file - much faster than bash -c "bash script"
    let mut cmd = Command::new("bash");
    cmd.current_dir("examples");
    cmd.arg(script_name);
    
    // Use direct execution instead of polling with sleep
    let output = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output() {
        Ok(output) => output,
        Err(e) => { 
            return Err(format!("Failed to run bash script: {}", e)); 
        }
    };
    
    Ok(output)
}
