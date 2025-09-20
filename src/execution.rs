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
/// Optimized for speed - runs bash directly on the script with timeout
pub fn run_shell_script(filename: &str) -> Result<std::process::Output, String> {
    use std::time::{Duration, Instant};
    use std::thread;
    
    // Extract just the filename part from the full path
    let script_name = filename.split(['\\', '/']).last().unwrap_or(filename);
    
    // Run bash directly on the script file - much faster than bash -c "bash script"
    let mut cmd = Command::new("bash");
    let examples_dir = std::env::current_dir().unwrap_or_default().join("examples");
    cmd.current_dir(&examples_dir);
    cmd.arg(script_name);
    
    // Add timeout handling to prevent hanging on interactive scripts
    let timeout_duration = Duration::from_secs(30); // 30 second timeout
    
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(e) => { 
            return Err(format!("Failed to start bash script: {}", e)); 
        }
    };
    
    let start = Instant::now();
    let output = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process completed
                break child.wait_with_output().unwrap();
            }
            Ok(None) => {
                // Process still running, check timeout
                if start.elapsed() > timeout_duration {
                    let _ = child.kill();
                    return Err(format!("Script {} timed out after {} seconds", script_name, timeout_duration.as_secs()));
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(format!("Error waiting for script {}: {}", script_name, e));
            }
        }
    };
    
    Ok(output)
}
