use std::path::PathBuf;

pub struct DevCacheItem {
    pub name: &'static str,
    pub path: PathBuf,
}

pub fn get_dev_cache_targets() -> Vec<DevCacheItem> {
    let mut items = Vec::new();
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    let dev_specs: &[(&str, &str)] = &[
        // Node / JavaScript
        ("npm Cache", ".npm/_cacache"),
        ("yarn Cache", "Library/Caches/Yarn"),
        ("pnpm Store", "Library/pnpm/store"),
        ("bun Cache", ".bun/install/cache"),
        ("Corepack Cache", ".cache/corepack"),
        // Rust / Cargo
        ("Cargo Registry Cache", ".cargo/registry/cache"),
        ("Cargo Git DB", ".cargo/git/db"),
        // Go
        ("Go Build Cache", "Library/Caches/go-build"),
        // Python
        ("pip Cache", "Library/Caches/pip"),
        ("Poetry Cache", "Library/Caches/pypoetry"),
        ("uv Cache", "Library/Caches/uv"),
        // Java / JVM
        ("Gradle Caches", ".gradle/caches"),
        ("Maven Repository Cache", ".m2/repository"),
        // Apple / iOS
        ("CocoaPods Cache", "Library/Caches/CocoaPods"),
        ("SwiftPM Cache", "Library/Caches/org.swift.swiftpm"),
        ("Xcode DerivedData", "Library/Developer/Xcode/DerivedData"),
        ("Xcode Archives", "Library/Developer/Xcode/Archives"),
        ("Xcode Simulator Cryptex", "Library/Developer/CoreSimulator/Volumes"),
        ("iOS DeviceSupport", "Library/Developer/Xcode/iOS DeviceSupport"),
        ("watchOS DeviceSupport", "Library/Developer/Xcode/watchOS DeviceSupport"),
        // Android
        ("Android Gradle Cache", ".android/build-cache"),
        ("Android Cache", ".android/cache"),
        // Homebrew
        ("Homebrew Cache", "Library/Caches/Homebrew"),
        // Docker
        ("Docker Container Logs & Cache", "Library/Containers/com.docker.docker/Data/log"),
    ];

    for &(name, rel_path) in dev_specs {
        let full_path = home.join(rel_path);
        if full_path.exists() {
            items.push(DevCacheItem {
                name,
                path: full_path,
            });
        }
    }

    items
}
