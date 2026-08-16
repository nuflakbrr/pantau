use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

use crate::cleaner::clean::calculate_dir_size;

#[derive(Debug, Clone)]
pub struct ProjectArtifact {
    pub project_name: String,
    pub artifact_type: &'static str,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub is_recent: bool,
}

const ARTIFACT_NAMES: &[(&str, &str)] = &[
    ("node_modules", "Node.js Dependencies"),
    ("target", "Rust / Cargo Build"),
    (".build", "Swift / SPM Build"),
    ("build", "Build Output"),
    ("dist", "Distribution Bundle"),
    (".next", "Next.js Build"),
    (".turbo", "Turborepo Cache"),
    (".nuxt", "Nuxt Build"),
    ("venv", "Python Virtualenv"),
    (".venv", "Python Virtualenv"),
    (".gradle", "Gradle Cache"),
    ("DerivedData", "Xcode DerivedData"),
];

pub fn scan_project_artifacts(scan_roots: &[PathBuf]) -> Vec<ProjectArtifact> {
    let mut artifacts = Vec::new();
    let seven_days_ago = SystemTime::now()
        .checked_sub(Duration::from_secs(7 * 24 * 3600))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    for root in scan_roots {
        if !root.exists() {
            continue;
        }

        for entry in WalkDir::new(root)
            .min_depth(2)
            .max_depth(5)
            .into_iter()
            .filter_entry(|e| {
                // Don't descend into hidden version control or deep node_modules
                let name = e.file_name().to_string_lossy();
                name != ".git" && name != ".hg" && name != ".svn"
            })
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                let file_name = entry.file_name().to_string_lossy();
                for &(art_name, art_desc) in ARTIFACT_NAMES {
                    if file_name == art_name {
                        let path = entry.path().to_path_buf();
                        let project_name = path
                            .parent()
                            .and_then(|p| p.file_name())
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();

                        let is_recent = entry
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map_or(false, |mtime| mtime > seven_days_ago);

                        let size_bytes = calculate_dir_size(&path);

                        if size_bytes > 1024 * 1024 {
                            // > 1MB
                            artifacts.push(ProjectArtifact {
                                project_name,
                                artifact_type: art_desc,
                                path,
                                size_bytes,
                                is_recent,
                            });
                        }
                        break;
                    }
                }
            }
        }
    }

    artifacts.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    artifacts
}
