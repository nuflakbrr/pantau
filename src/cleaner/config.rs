use directories::BaseDirs;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct WhitelistCacheItem {
    pub display_name: &'static str,
    pub pattern: &'static str,
    pub category: &'static str,
}

#[derive(Debug, Clone)]
pub struct DynamicCacheItem {
    pub display_name: String,
    pub pattern: String,
    pub category: String,
}

pub const ALL_CACHE_ITEMS: &[WhitelistCacheItem] = &[
    WhitelistCacheItem { display_name: "Gradle daemon processes cache", pattern: "~/.gradle/daemon/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "R renv global cache (virtual environments)", pattern: "~/Library/Caches/org.R-project.R/R/renv/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "tealdeer tldr pages cache", pattern: "~/Library/Caches/tealdeer/tldr-pages", category: "package_manager" },
    WhitelistCacheItem { display_name: "Playwright browser binaries", pattern: "~/Library/Caches/ms-playwright*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Ollama local AI models", pattern: "~/.ollama/models/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Surge proxy cache", pattern: "~/Library/Caches/com.nssurge.surge-mac/*", category: "network_tools" },
    WhitelistCacheItem { display_name: "Surge configuration and data", pattern: "~/Library/Application Support/com.nssurge.surge-mac/*", category: "network_tools" },
    WhitelistCacheItem { display_name: "Finder metadata, .DS_Store", pattern: "FINDER_METADATA", category: "system_cache" },
    WhitelistCacheItem { display_name: "Apple Mail cache", pattern: "~/Library/Caches/com.apple.mail/*", category: "system_cache" },
    WhitelistCacheItem { display_name: "Gradle build cache (Android Studio, Gradle projects)", pattern: "~/.gradle/caches/build-cache-*/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Gradle worker cache", pattern: "~/.gradle/workers/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Xcode DerivedData (build outputs, indexes)", pattern: "~/Library/Developer/Xcode/DerivedData/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Xcode internal cache files", pattern: "~/Library/Caches/com.apple.dt.Xcode/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Xcode iOS device support symbols", pattern: "~/Library/Developer/Xcode/iOS DeviceSupport/*/Symbols/System/Library/Caches/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Maven local repository (Java dependencies)", pattern: "~/.m2/repository/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "JetBrains IDEs data (IntelliJ, PyCharm, WebStorm, GoLand)", pattern: "~/Library/Application Support/JetBrains/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "JetBrains IDEs cache", pattern: "~/Library/Caches/JetBrains/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Android Studio cache and indexes", pattern: "~/Library/Caches/Google/AndroidStudio*/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Android build cache", pattern: "~/.android/build-cache/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "VS Code runtime cache", pattern: "~/Library/Application Support/Code/Cache/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "VS Code extension and update cache", pattern: "~/Library/Application Support/Code/CachedData/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "VS Code system cache (Cursor, VSCodium)", pattern: "~/Library/Caches/com.microsoft.VSCode/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "Cursor editor cache", pattern: "~/Library/Caches/com.todesktop.230313mzl4w4u92/*", category: "ide_cache" },
    WhitelistCacheItem { display_name: "LM Studio app cache", pattern: "~/Library/Caches/com.lmstudio.lmstudio/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Codex Desktop update staging", pattern: "~/Library/Caches/com.openai.codex/org.sparkle-project.Sparkle/Installation", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Chrome on-device AI models", pattern: "~/Library/Application Support/Google/Chrome/OptGuideOnDevice*/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Chrome optimization guide models", pattern: "~/Library/Application Support/Google/Chrome/optimization_guide_model_store/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Bazel build cache", pattern: "~/.cache/bazel/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Go build cache", pattern: "~/Library/Caches/go-build/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Go module cache", pattern: "~/go/pkg/mod/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Rust Cargo registry cache", pattern: "~/.cargo/registry/cache/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Rust Cargo extracted sources", pattern: "~/.cargo/registry/src/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Rust documentation cache", pattern: "~/.rustup/toolchains/*/share/doc/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Rustup toolchain downloads", pattern: "~/.rustup/downloads/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "ccache compiler cache", pattern: "~/.ccache/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "sccache distributed compiler cache", pattern: "~/.cache/sccache/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "SBT Scala build cache", pattern: "~/.sbt/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Ivy dependency cache", pattern: "~/.ivy2/cache/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Turbo monorepo build cache", pattern: "~/.turbo/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Next.js build cache", pattern: "~/.next/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Vite build cache", pattern: "~/.vite/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Parcel bundler cache", pattern: "~/.parcel-cache/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "pre-commit hooks cache", pattern: "~/.cache/pre-commit/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Ruff Python linter cache", pattern: "~/.cache/ruff/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "MyPy type checker cache", pattern: "~/.cache/mypy/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Pytest test cache", pattern: "~/.pytest_cache/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "PyInstaller binary cache", pattern: "~/Library/Caches/pyinstaller/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Flutter SDK cache", pattern: "~/.cache/flutter/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Swift Package Manager cache", pattern: "~/.cache/swift-package-manager/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Zig compiler cache", pattern: "~/.cache/zig/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "Deno cache", pattern: "~/Library/Caches/deno/*", category: "compiler_cache" },
    WhitelistCacheItem { display_name: "CocoaPods cache (iOS dependencies)", pattern: "~/Library/Caches/CocoaPods/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "npm package cache", pattern: "~/.npm/_cacache/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "pip Python package cache", pattern: "~/.cache/pip/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "uv Python package cache", pattern: "~/.cache/uv/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "Homebrew downloaded packages", pattern: "~/Library/Caches/Homebrew/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "Yarn package manager cache", pattern: "~/.cache/yarn/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "pnpm package store", pattern: "~/Library/pnpm/store/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "Composer PHP dependencies cache (legacy)", pattern: "~/.composer/cache/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "Composer PHP dependencies cache", pattern: "~/Library/Caches/composer/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "RubyGems cache", pattern: "~/.gem/cache/*", category: "package_manager" },
    WhitelistCacheItem { display_name: "Conda package metadata/tarball cache", pattern: "~/.conda/pkgs", category: "package_manager" },
    WhitelistCacheItem { display_name: "Anaconda package metadata/tarball cache", pattern: "~/anaconda3/pkgs", category: "package_manager" },
    WhitelistCacheItem { display_name: "PyTorch model cache", pattern: "~/.cache/torch/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "TensorFlow model and dataset cache", pattern: "~/.cache/tensorflow/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "HuggingFace models and datasets", pattern: "~/.cache/huggingface/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Selenium WebDriver binaries", pattern: "~/.cache/selenium/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Weights & Biases ML experiments cache", pattern: "~/.cache/wandb/*", category: "ai_ml_cache" },
    WhitelistCacheItem { display_name: "Safari web browser cache", pattern: "~/Library/Caches/com.apple.Safari/*", category: "browser_cache" },
    WhitelistCacheItem { display_name: "Chrome browser cache", pattern: "~/Library/Caches/Google/Chrome/*", category: "browser_cache" },
    WhitelistCacheItem { display_name: "Firefox browser cache", pattern: "~/Library/Caches/Firefox/*", category: "browser_cache" },
    WhitelistCacheItem { display_name: "Brave browser cache", pattern: "~/Library/Caches/BraveSoftware/Brave-Browser/*", category: "browser_cache" },
    WhitelistCacheItem { display_name: "Docker BuildX cache", pattern: "~/.docker/buildx/cache/*", category: "container_cache" },
    WhitelistCacheItem { display_name: "Podman container cache", pattern: "~/.local/share/containers/cache/*", category: "container_cache" },
    WhitelistCacheItem { display_name: "Tart OCI/IPSW cache", pattern: "~/.tart/cache", category: "container_cache" },
    WhitelistCacheItem { display_name: "Font cache", pattern: "~/Library/Caches/com.apple.FontRegistry/*", category: "system_cache" },
    WhitelistCacheItem { display_name: "Spotlight metadata cache", pattern: "~/Library/Caches/com.apple.spotlight/*", category: "system_cache" },
    WhitelistCacheItem { display_name: "CloudKit cache", pattern: "~/Library/Caches/CloudKit/*", category: "system_cache" },
    WhitelistCacheItem { display_name: "Trash", pattern: "~/.Trash", category: "system_cache" },
];

pub fn get_all_discoverable_cache_items() -> Vec<DynamicCacheItem> {
    let mut items = Vec::new();
    let mut seen_patterns = HashSet::new();

    // 1. Curated standard presets
    for item in ALL_CACHE_ITEMS {
        seen_patterns.insert(item.pattern.to_string());
        items.push(DynamicCacheItem {
            display_name: item.display_name.to_string(),
            pattern: item.pattern.to_string(),
            category: item.category.to_string(),
        });
    }

    let home = BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    // 2. Dynamically discover all installed applications on this Mac
    let app_roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        home.join("Applications"),
    ];

    for root in app_roots {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.extension().map_or(false, |ext| ext == "app") {
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let name = file_name.trim_end_matches(".app").to_string();

                    if let Some(bid) = crate::cleaner::uninstall::read_bundle_id(&path) {
                        let pat = format!("~/Library/Caches/{}/*", bid);
                        let cache_path = home.join("Library/Caches").join(&bid);
                        if cache_path.exists() && seen_patterns.insert(pat.clone()) {
                            items.push(DynamicCacheItem {
                                display_name: format!("{} cache ({})", name, bid),
                                pattern: pat,
                                category: "installed_app".to_string(),
                            });
                        }
                    } else {
                        let pat = format!("~/Library/Caches/{}/*", name);
                        let cache_path = home.join("Library/Caches").join(&name);
                        if cache_path.exists() && seen_patterns.insert(pat.clone()) {
                            items.push(DynamicCacheItem {
                                display_name: format!("{} cache", name),
                                pattern: pat,
                                category: "installed_app".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // 3. Dynamically discover any active user app cache folders in ~/Library/Caches
    let user_caches_dir = home.join("Library/Caches");
    if let Ok(entries) = fs::read_dir(user_caches_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let fname = path.file_name().unwrap_or_default().to_string_lossy();
                let pat = format!("~/Library/Caches/{}/*", fname);
                if !fname.starts_with('.') && seen_patterns.insert(pat.clone()) {
                    items.push(DynamicCacheItem {
                        display_name: format!("{} cache", fname),
                        pattern: pat,
                        category: "user_cache".to_string(),
                    });
                }
            }
        }
    }

    // 4. Dynamically discover any active tool cache folders in ~/.cache
    let dot_cache_dir = home.join(".cache");
    if let Ok(entries) = fs::read_dir(dot_cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let fname = path.file_name().unwrap_or_default().to_string_lossy();
                let pat = format!("~/.cache/{}/*", fname);
                if !fname.starts_with('.') && seen_patterns.insert(pat.clone()) {
                    items.push(DynamicCacheItem {
                        display_name: format!("{} dev/tool cache", fname),
                        pattern: pat,
                        category: "dev_cache".to_string(),
                    });
                }
            }
        }
    }

    items
}

pub struct CleanerConfig {
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
    pub whitelist_file: PathBuf,
    pub purge_paths_file: PathBuf,
}

impl CleanerConfig {
    pub fn new() -> Self {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let config_dir = home.join(".config").join("pantau");
        let log_dir = home.join("Library").join("Logs").join("pantau");
        let whitelist_file = config_dir.join("whitelist");
        let purge_paths_file = config_dir.join("purge_paths");

        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&log_dir);

        Self {
            config_dir,
            log_dir,
            whitelist_file,
            purge_paths_file,
        }
    }

    pub fn load_whitelist(&self) -> Vec<String> {
        if !self.whitelist_file.exists() {
            return Self::default_whitelist();
        }

        match fs::read_to_string(&self.whitelist_file) {
            Ok(content) => {
                let lines: Vec<String> = content
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect();
                if lines.is_empty() {
                    Self::default_whitelist()
                } else {
                    lines
                }
            }
            Err(_) => Self::default_whitelist(),
        }
    }

    pub fn save_whitelist(&self, patterns: &[String]) -> std::io::Result<()> {
        let mut content = String::from(
            "# Pantau Whitelist - Protected paths won't be deleted\n\
             # Default protections: Playwright browsers, HuggingFace models, Maven repo, Ollama models, Surge Mac, R renv, Finder metadata\n\
             # Add one pattern per line to keep items safe.\n\n",
        );
        for p in patterns {
            content.push_str(p);
            content.push('\n');
        }
        fs::write(&self.whitelist_file, content)
    }

    pub fn default_whitelist() -> Vec<String> {
        vec![
            "~/Library/Caches/ms-playwright*".into(),
            "~/.cache/huggingface*".into(),
            "~/.m2/repository/*".into(),
            "~/.gradle/caches/*".into(),
            "~/.gradle/daemon/*".into(),
            "~/.ollama/models/*".into(),
            "~/Library/Caches/com.nssurge.surge-mac/*".into(),
            "~/Library/Application Support/com.nssurge.surge-mac/*".into(),
            "~/Library/Caches/org.R-project.R/R/renv/*".into(),
            "~/Library/Caches/pypoetry/virtualenvs*".into(),
            "~/Library/Caches/JetBrains*".into(),
            "~/Library/Caches/com.jetbrains.toolbox*".into(),
            "~/Library/Caches/tealdeer/tldr-pages".into(),
            "~/Library/Application Support/JetBrains*".into(),
            "~/Library/Caches/com.apple.finder".into(),
            "~/Library/Mobile Documents*".into(),
            "~/Library/Caches/com.apple.FontRegistry*".into(),
            "~/Library/Caches/com.apple.spotlight*".into(),
            "~/Library/Caches/CloudKit*".into(),
            "FINDER_METADATA".into(),
        ]
    }

    pub fn load_purge_paths(&self) -> Vec<PathBuf> {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        if self.purge_paths_file.exists() {
            if let Ok(content) = fs::read_to_string(&self.purge_paths_file) {
                let paths: Vec<PathBuf> = content
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(|l| {
                        if l.starts_with('~') {
                            home.join(l.trim_start_matches("~/"))
                        } else {
                            PathBuf::from(l)
                        }
                    })
                    .filter(|p| p.exists())
                    .collect();

                if !paths.is_empty() {
                    return paths;
                }
            }
        }

        // Default project search directories
        let defaults = vec![
            home.join("Projects"),
            home.join("GitHub"),
            home.join("dev"),
            home.join("Development"),
            home.join("Workspace"),
            home.join("Documents"),
            home.join("Personal Projects"),
            home.join("Daily"),
        ];

        defaults.into_iter().filter(|p| p.exists()).collect()
    }
}
