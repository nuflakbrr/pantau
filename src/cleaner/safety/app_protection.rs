/// List of critical macOS system applications and core utilities that must NEVER be uninstalled or removed.
pub const PROTECTED_SYSTEM_APPS: &[&str] = &[
    "Finder",
    "Safari",
    "System Settings",
    "System Preferences",
    "App Store",
    "Terminal",
    "Console",
    "Activity Monitor",
    "Disk Utility",
    "Keychain Access",
    "Automator",
    "Shortcuts",
    "Time Machine",
    "Archive Utility",
    "pantau-app",
    "Pantau",
];

pub fn is_protected_app(app_name: &str) -> bool {
    let clean_name = app_name.trim_end_matches(".app");
    for &protected in PROTECTED_SYSTEM_APPS {
        if clean_name.eq_ignore_ascii_case(protected) {
            return true;
        }
    }
    false
}
