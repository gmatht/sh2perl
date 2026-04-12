use std::time::{Duration, Instant};
use std::thread;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

/// Different types of operations that can have timeouts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    ShellExecution,      // Running shell scripts
    PerlExecution,       // Running Perl code
    Parsing,            // Parsing shell scripts
    CodeGeneration,     // Generating Perl code
    FileOperations,     // File I/O operations
    TestExecution,      // Overall test execution
    DebugFreeze,        // Debug freeze pause
}

/// Timeout configuration for different operations
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub shell_execution: Duration,
    pub perl_execution: Duration,
    pub parsing: Duration,
    pub code_generation: Duration,
    pub file_operations: Duration,
    pub test_execution: Duration,
    pub debug_freeze: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            shell_execution: Duration::from_secs(120),   // 2 minutes for shell scripts
            perl_execution: Duration::from_secs(30),     // 30 seconds for Perl execution
            parsing: Duration::from_secs(5),             // 5 seconds for parsing
            code_generation: Duration::from_secs(5),     // 5 seconds for code generation
            file_operations: Duration::from_secs(2),     // 2 seconds for file operations
            test_execution: Duration::from_secs(300),    // 5 minutes for overall test
            debug_freeze: Duration::from_secs(300),      // 5 minutes for debug freeze
        }
    }
}

/// Debug freeze manager
#[derive(Debug)]
pub struct DebugFreezeManager {
    is_frozen: Arc<AtomicBool>,
    freeze_duration: Duration,
}

impl DebugFreezeManager {
    pub fn new() -> Self {
        Self {
            is_frozen: Arc::new(AtomicBool::new(false)),
            freeze_duration: Duration::from_secs(300), // 5 minutes default
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.is_frozen.load(Ordering::Relaxed)
    }

    pub fn freeze(&self) {
        self.is_frozen.store(true, Ordering::Relaxed);
        eprintln!("DEBUG: Execution frozen for debugging. Press Ctrl+C to continue or wait {} seconds.", 
                 self.freeze_duration.as_secs());
    }

    pub fn unfreeze(&self) {
        self.is_frozen.store(false, Ordering::Relaxed);
        eprintln!("DEBUG: Execution unfrozen, continuing...");
    }

    pub fn set_freeze_duration(&mut self, duration: Duration) {
        self.freeze_duration = duration;
    }

    pub fn get_freeze_duration(&self) -> Duration {
        self.freeze_duration
    }
}

/// Timeout manager with fine-grained control
#[derive(Debug)]
pub struct TimeoutManager {
    config: TimeoutConfig,
    debug_freeze: DebugFreezeManager,
}

impl TimeoutManager {
    pub fn new() -> Self {
        Self {
            config: TimeoutConfig::default(),
            debug_freeze: DebugFreezeManager::new(),
        }
    }

    pub fn with_config(config: TimeoutConfig) -> Self {
        Self {
            config,
            debug_freeze: DebugFreezeManager::new(),
        }
    }

    pub fn get_timeout(&self, operation: OperationType) -> Duration {
        match operation {
            OperationType::ShellExecution => self.config.shell_execution,
            OperationType::PerlExecution => self.config.perl_execution,
            OperationType::Parsing => self.config.parsing,
            OperationType::CodeGeneration => self.config.code_generation,
            OperationType::FileOperations => self.config.file_operations,
            OperationType::TestExecution => self.config.test_execution,
            OperationType::DebugFreeze => self.config.debug_freeze,
        }
    }

    pub fn set_timeout(&mut self, operation: OperationType, duration: Duration) {
        match operation {
            OperationType::ShellExecution => self.config.shell_execution = duration,
            OperationType::PerlExecution => self.config.perl_execution = duration,
            OperationType::Parsing => self.config.parsing = duration,
            OperationType::CodeGeneration => self.config.code_generation = duration,
            OperationType::FileOperations => self.config.file_operations = duration,
            OperationType::TestExecution => self.config.test_execution = duration,
            OperationType::DebugFreeze => self.config.debug_freeze = duration,
        }
    }

    pub fn get_debug_freeze_manager(&self) -> &DebugFreezeManager {
        &self.debug_freeze
    }

    /// Execute an operation with timeout and debug freeze support
    pub fn execute_with_timeout<F, T>(&self, operation: OperationType, operation_fn: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String> + Send + 'static,
        T: Send + 'static,
    {
        let timeout_duration = self.get_timeout(operation);
        let debug_freeze = self.debug_freeze.is_frozen.clone();
        eprintln!("DEBUG: Timeout manager starting {:?} with timeout of {:.1} seconds", operation, timeout_duration.as_secs_f64());
        
        // Check if we're in debug freeze mode
        if debug_freeze.load(Ordering::Relaxed) {
            eprintln!("DEBUG: Operation {:?} is frozen for debugging", operation);
            thread::sleep(Duration::from_millis(100));
            return Err("Operation frozen for debugging".to_string());
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let operation_handle = thread::spawn(move || {
            let result = operation_fn();
            let _ = tx.send(result);
        });

        // Wait for completion or timeout
        match rx.recv_timeout(timeout_duration) {
            Ok(result) => {
                eprintln!("DEBUG: Timeout manager completed {:?} successfully", operation);
                result
            },
            Err(_) => {
                // Check if we're in debug freeze mode before timing out
                if debug_freeze.load(Ordering::Relaxed) {
                    eprintln!("DEBUG: Operation {:?} timed out but debug freeze is active", operation);
                    return Err("Operation timed out during debug freeze".to_string());
                }
                
                eprintln!("DEBUG: Timeout manager timed out {:?} after {:.1} seconds", operation, timeout_duration.as_secs_f64());
                let operation_name = format!("{:?}", operation);
                Err(format!("{} timed out after {:.1} seconds", 
                           operation_name, timeout_duration.as_secs_f64()))
            }
        }
    }

    /// Execute an operation with timeout and progress reporting
    pub fn execute_with_progress<F, T>(&self, operation: OperationType, operation_fn: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String> + Send + 'static,
        T: Send + 'static,
    {
        let timeout_duration = self.get_timeout(operation);
        let debug_freeze = self.debug_freeze.is_frozen.clone();
        
        // Check if we're in debug freeze mode
        if debug_freeze.load(Ordering::Relaxed) {
            eprintln!("DEBUG: Operation {:?} is frozen for debugging", operation);
            thread::sleep(Duration::from_millis(100));
            return Err("Operation frozen for debugging".to_string());
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let operation_handle = thread::spawn(move || {
            let result = operation_fn();
            let _ = tx.send(result);
        });

        let start = Instant::now();
        let mut last_progress = Instant::now();
        let progress_interval = Duration::from_secs(1); // Report progress every second

        // Wait for completion or timeout with progress reporting
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return result,
                Err(_) => {
                    let elapsed = start.elapsed();
                    
                    // Check if we're in debug freeze mode
                    if debug_freeze.load(Ordering::Relaxed) {
                        eprintln!("DEBUG: Operation {:?} timed out but debug freeze is active", operation);
                        return Err("Operation timed out during debug freeze".to_string());
                    }
                    
                    // Check for timeout
                    if elapsed > timeout_duration {
                        let operation_name = format!("{:?}", operation);
                        return Err(format!("{} timed out after {:.1} seconds", 
                                         operation_name, timeout_duration.as_secs_f64()));
                    }
                    
                    // Report progress if enough time has passed
                    if last_progress.elapsed() > progress_interval {
                        let progress_percent = (elapsed.as_secs_f64() / timeout_duration.as_secs_f64() * 100.0).min(100.0);
                        eprintln!("DEBUG: {} in progress... {:.1}% ({:.1}s/{:.1}s)", 
                                 format!("{:?}", operation), progress_percent, 
                                 elapsed.as_secs_f64(), timeout_duration.as_secs_f64());
                        last_progress = Instant::now();
                    }
                }
            }
        }
    }

    /// Create a timeout configuration for fast tests
    pub fn fast_test_config() -> TimeoutConfig {
        TimeoutConfig {
            shell_execution: Duration::from_secs(10),    // 10 seconds for shell scripts
            perl_execution: Duration::from_secs(5),      // 5 seconds for Perl execution
            parsing: Duration::from_secs(2),             // 2 seconds for parsing
            code_generation: Duration::from_secs(2),     // 2 seconds for code generation
            file_operations: Duration::from_secs(1),     // 1 second for file operations
            test_execution: Duration::from_secs(30),     // 30 seconds for overall test
            debug_freeze: Duration::from_secs(60),       // 1 minute for debug freeze
        }
    }

    /// Create a timeout configuration for slow tests
    pub fn slow_test_config() -> TimeoutConfig {
        TimeoutConfig {
            shell_execution: Duration::from_secs(60),    // 60 seconds for shell scripts
            perl_execution: Duration::from_secs(30),     // 30 seconds for Perl execution
            parsing: Duration::from_secs(10),            // 10 seconds for parsing
            code_generation: Duration::from_secs(10),    // 10 seconds for code generation
            file_operations: Duration::from_secs(5),     // 5 seconds for file operations
            test_execution: Duration::from_secs(300),    // 5 minutes for overall test
            debug_freeze: Duration::from_secs(600),      // 10 minutes for debug freeze
        }
    }

    /// Create a timeout configuration for debug mode
    pub fn debug_config() -> TimeoutConfig {
        TimeoutConfig {
            shell_execution: Duration::from_secs(120),   // 2 minutes for shell scripts
            perl_execution: Duration::from_secs(60),     // 1 minute for Perl execution
            parsing: Duration::from_secs(30),            // 30 seconds for parsing
            code_generation: Duration::from_secs(30),    // 30 seconds for code generation
            file_operations: Duration::from_secs(10),    // 10 seconds for file operations
            test_execution: Duration::from_secs(600),    // 10 minutes for overall test
            debug_freeze: Duration::from_secs(1800),     // 30 minutes for debug freeze
        }
    }
}

impl Default for TimeoutManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global timeout manager instance
lazy_static::lazy_static! {
    pub static ref TIMEOUT_MANAGER: Arc<Mutex<TimeoutManager>> = Arc::new(Mutex::new(TimeoutManager::new()));
}

/// Convenience functions for global timeout manager
pub fn get_timeout_manager() -> Arc<Mutex<TimeoutManager>> {
    TIMEOUT_MANAGER.clone()
}

pub fn execute_with_timeout<F, T>(operation: OperationType, operation_fn: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let manager = get_timeout_manager();
    let manager = manager.lock().unwrap();
    manager.execute_with_timeout(operation, operation_fn)
}

pub fn execute_with_progress<F, T>(operation: OperationType, operation_fn: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let manager = get_timeout_manager();
    let manager = manager.lock().unwrap();
    manager.execute_with_progress(operation, operation_fn)
}

pub fn freeze_execution() {
    let manager = get_timeout_manager();
    let manager = manager.lock().unwrap();
    manager.get_debug_freeze_manager().freeze();
}

pub fn unfreeze_execution() {
    let manager = get_timeout_manager();
    let manager = manager.lock().unwrap();
    manager.get_debug_freeze_manager().unfreeze();
}

pub fn is_execution_frozen() -> bool {
    let manager = get_timeout_manager();
    let manager = manager.lock().unwrap();
    manager.get_debug_freeze_manager().is_frozen()
}
