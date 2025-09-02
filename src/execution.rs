use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::thread;
use debashl::shared_utils;

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
/// Always uses bash -c for consistency across platforms
pub fn run_shell_script(filename: &str) -> Result<std::process::Output, String> {
    // Extract just the filename part from the full path
    let script_name = filename.split(['\\', '/']).last().unwrap_or(filename);
    
    // Always use bash -c with cd examples for consistency
    let mut cmd = Command::new("bash");
    cmd.current_dir("examples");
    cmd.args(&["-c", &format!("bash {}", script_name)]);
    
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => { 
            return Err(format!("Failed to spawn bash: {}", e)); 
        }
    };
    
    let start = std::time::Instant::now();
    let output = loop {
        match child.try_wait() {
            Ok(Some(_)) => break child.wait_with_output().unwrap(),
            Ok(None) => {
                if start.elapsed() > Duration::from_millis(10000) { // Increased timeout to 10 seconds
                    let _ = child.kill(); 
                    break child.wait_with_output().unwrap(); 
                }
                thread::sleep(Duration::from_millis(100)); // Increased sleep interval
            }
            Err(_) => break child.wait_with_output().unwrap(),
        }
    };
    
    Ok(output)
}
