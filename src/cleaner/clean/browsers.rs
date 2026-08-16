use std::path::PathBuf;

pub struct BrowserCacheItem {
    pub browser_name: &'static str,
    pub path: PathBuf,
}

pub fn get_browser_cache_targets() -> Vec<BrowserCacheItem> {
    let mut items = Vec::new();
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    let browser_specs: &[(&str, &str)] = &[
        // Google Chrome
        ("Google Chrome Cache", "Library/Caches/Google/Chrome/Default/Cache"),
        ("Google Chrome Media Cache", "Library/Caches/Google/Chrome/Default/Media Cache"),
        ("Google Chrome GPUCache", "Library/Caches/Google/Chrome/Default/GPUCache"),
        // Apple Safari
        ("Safari Cache", "Library/Caches/com.apple.Safari"),
        ("Safari WebKit Cache", "Library/Containers/com.apple.Safari/Data/Library/Caches"),
        // Mozilla Firefox
        ("Mozilla Firefox Cache", "Library/Caches/Firefox/Profiles"),
        // Brave Browser
        ("Brave Browser Cache", "Library/Caches/BraveSoftware/Brave-Browser/Default/Cache"),
        ("Brave GPUCache", "Library/Caches/BraveSoftware/Brave-Browser/Default/GPUCache"),
        // Microsoft Edge
        ("Microsoft Edge Cache", "Library/Caches/Microsoft Edge/Default/Cache"),
        // Arc Browser
        ("Arc Browser Cache", "Library/Caches/company.thebrowser.Browser/Default/Cache"),
        // Opera & Vivaldi
        ("Opera Cache", "Library/Caches/com.operasoftware.Opera"),
        ("Vivaldi Cache", "Library/Caches/Vivaldi/Default/Cache"),
    ];

    for &(browser_name, rel_path) in browser_specs {
        let full_path = home.join(rel_path);
        if full_path.exists() {
            items.push(BrowserCacheItem {
                browser_name,
                path: full_path,
            });
        }
    }

    items
}
