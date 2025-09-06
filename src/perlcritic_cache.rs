use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use filetime::FileTime;

/// Perl Critic cache manager
pub struct PerlCriticCache {
    cache_dir: PathBuf,
    config_file: PathBuf,
}

impl PerlCriticCache {
    /// Create a new cache manager
    pub fn new() -> Self {
        Self {
            cache_dir: PathBuf::from("cache/perlcritic"),
            config_file: PathBuf::from("docs/perlcritic.conf"),
        }
    }

    /// Generate SHA256 hash of the Perl code
    fn hash_perl_code(&self, perl_code: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(perl_code.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get the cache file path for given Perl code
    fn get_cache_path(&self, perl_code: &str) -> PathBuf {
        let hash = self.hash_perl_code(perl_code);
        self.cache_dir.join(hash)
    }

    /// Check if cache is valid (newer than config file)
    fn is_cache_valid(&self, cache_path: &Path) -> bool {
        if !cache_path.exists() {
            return false;
        }

        // If no config file exists, cache is always valid
        if !self.config_file.exists() {
            return true;
        }

        // Check if cache file is newer than config file
        let cache_meta = match fs::metadata(cache_path) {
            Ok(meta) => meta,
            Err(_) => return false,
        };

        let config_meta = match fs::metadata(&self.config_file) {
            Ok(meta) => meta,
            Err(_) => return false,
        };

        let cache_time = FileTime::from_last_modification_time(&cache_meta);
        let config_time = FileTime::from_last_modification_time(&config_meta);

        cache_time >= config_time
    }

    /// Get cached result if it exists
    pub fn get_cached_result(&self, perl_code: &str) -> Option<String> {
        let cache_path = self.get_cache_path(perl_code);
        
        // Always return cached result if it exists, regardless of validity
        match fs::read_to_string(&cache_path) {
            Ok(content) => Some(content),
            Err(_) => None,
        }
    }

    /// Store result in cache
    pub fn store_result(&self, perl_code: &str, result: &str) -> Result<(), String> {
        // Ensure cache directory exists
        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            return Err(format!("Failed to create cache directory: {}", e));
        }

        let cache_path = self.get_cache_path(perl_code);
        
        if let Err(e) = fs::write(&cache_path, result) {
            return Err(format!("Failed to write cache file: {}", e));
        }

        Ok(())
    }

    /// Check if we should use cache (for compatibility)
    pub fn should_use_cache(&self) -> bool {
        true
    }

    /// Check if cache should be invalidated (config file changed)
    pub fn should_invalidate_cache(&self, perl_code: &str) -> bool {
        let cache_path = self.get_cache_path(perl_code);
        !self.is_cache_valid(&cache_path)
    }
}

impl Default for PerlCriticCache {
    fn default() -> Self {
        Self::new()
    }
}
