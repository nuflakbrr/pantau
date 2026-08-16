use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct InstallerFile {
    pub file_name: String,
    pub source_category: &'static str,
    pub path: PathBuf,
    pub size_bytes: u64,
}

pub fn scan_installer_files() -> Vec<InstallerFile> {
    let mut files = Vec::new();
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    let scan_sources: &[(&str, PathBuf)] = &[
        ("Downloads", home.join("Downloads")),
        ("Desktop", home.join("Desktop")),
        ("Documents", home.join("Documents")),
        ("Homebrew Cache", home.join("Library/Caches/Homebrew")),
        (
            "Mail Downloads",
            home.join("Library/Containers/com.apple.mail/Data/Library/Mail Downloads"),
        ),
        (
            "Telegram Downloads",
            home.join("Downloads/Telegram Desktop"),
        ),
    ];

    for &(category, ref root) in scan_sources {
        if !root.exists() {
            continue;
        }

        for entry in WalkDir::new(root)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "dmg"
                        || ext_lower == "pkg"
                        || ext_lower == "mpkg"
                        || ext_lower == "iso"
                        || ext_lower == "xip"
                    {
                        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        if size_bytes > 5 * 1024 * 1024 {
                            // > 5MB
                            let file_name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            files.push(InstallerFile {
                                file_name,
                                source_category: category,
                                path: path.to_path_buf(),
                                size_bytes,
                            });
                        }
                    }
                }
            }
        }
    }

    files.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    files
}
