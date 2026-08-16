use directories::BaseDirs;
use std::path::{Path, PathBuf};

pub struct PathValidator {
    home: PathBuf,
    whitelist: Vec<String>,
}

impl PathValidator {
    pub fn new(whitelist: Vec<String>) -> Self {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        Self { home, whitelist }
    }

    /// Checks if a path is strictly safe to delete or modify.
    pub fn is_safe_to_delete(&self, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy();

        // 1. Never allow root or direct top-level system paths
        if path_str == "/"
            || path_str == "/System"
            || path_str.starts_with("/System/")
            || path_str == "/bin"
            || path_str == "/sbin"
            || path_str == "/usr"
            || path_str == "/usr/bin"
            || path_str == "/usr/sbin"
            || path_str == "/etc"
            || path_str == "/var"
            || path_str == "/Library"
            || path_str == "/Applications"
            || path_str == "/Users"
            || path_str == self.home.to_string_lossy()
        {
            return Err(format!("Protected system/home directory: {}", path_str));
        }

        // 2. Prevent path traversal or relative dangerous paths
        if path_str.contains("..") {
            return Err("Path traversal (..) not allowed".into());
        }

        // 3. Check against user whitelist
        for pattern in &self.whitelist {
            let pat_str = pattern.trim_end_matches("/*").trim_end_matches('*');
            let expanded = if pat_str.starts_with('~') {
                self.home.join(pat_str.trim_start_matches("~/"))
            } else {
                PathBuf::from(pat_str)
            };

            if path == expanded || path.starts_with(&expanded) || expanded.starts_with(path) {
                return Err(format!("Path is protected by whitelist: {}", path_str));
            }
        }

        // 4. Must not delete user essential root folders directly (Desktop, Documents, Downloads itself)
        if path == self.home.join("Desktop")
            || path == self.home.join("Documents")
            || path == self.home.join("Downloads")
            || path == self.home.join("Library")
            || path == self.home.join("Pictures")
            || path == self.home.join("Music")
            || path == self.home.join("Movies")
        {
            return Err(format!("Direct user folder deletion prohibited: {}", path_str));
        }

        Ok(())
    }

    /// Checks if a path is symlink that might point outside intended boundary.
    pub fn is_symlink_safe(&self, path: &Path) -> bool {
        if path.is_symlink() {
            if let Ok(target) = std::fs::read_link(path) {
                if self.is_safe_to_delete(&target).is_err() {
                    return false;
                }
            }
        }
        true
    }
}
