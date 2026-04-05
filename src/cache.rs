use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub code_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandCache {
    bash_cache: HashMap<String, CachedOutput>,
    perl_cache: HashMap<String, CachedOutput>,
}

impl CommandCache {
    pub fn load() -> Self {
        let cache_file = "command_cache.json";
        if Path::new(cache_file).exists() {
            match fs::read_to_string(cache_file) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(cache) => cache,
                        Err(_) => Self::default(),
                    }
                }
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let cache_file = "command_cache.json";
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(cache_file, json);
        }
    }

    pub fn is_bash_cache_valid(&self, filename: &str) -> bool {
        self.bash_cache.contains_key(filename)
    }

    pub fn get_cached_bash_output(&self, filename: &str) -> Option<CachedOutput> {
        self.bash_cache.get(filename).cloned()
    }

    pub fn update_bash_cache(&mut self, filename: &str, stdout: String, stderr: String, exit_code: i32) {
        self.bash_cache.insert(
            filename.to_string(),
            CachedOutput {
                stdout,
                stderr,
                exit_code,
                code_hash: None,
            },
        );
    }

    pub fn invalidate_bash_cache(&mut self, filename: &str) {
        self.bash_cache.remove(filename);
    }

    pub fn is_perl_cache_valid(&self, filename: &str, code: &str) -> bool {
        if let Some(cached) = self.perl_cache.get(filename) {
            // Check if the code hash matches
            if let Some(ref code_hash) = cached.code_hash {
                let current_hash = format!("{:x}", md5::compute(code));
                code_hash == &current_hash
            } else {
                // No hash stored, consider invalid
                false
            }
        } else {
            false
        }
    }

    pub fn get_cached_perl_output(&self, filename: &str) -> Option<CachedOutput> {
        self.perl_cache.get(filename).cloned()
    }

    pub fn update_perl_cache(&mut self, filename: &str, stdout: String, stderr: String, exit_code: i32, code: &str) {
        let code_hash = format!("{:x}", md5::compute(code));
        self.perl_cache.insert(
            filename.to_string(),
            CachedOutput {
                stdout,
                stderr,
                exit_code,
                code_hash: Some(code_hash),
            },
        );
    }
}

