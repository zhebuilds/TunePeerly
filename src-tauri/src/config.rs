// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "TunePeerly";
pub const DATABASE_NAME: &str = "library.db";

pub fn app_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(APP_DIR_NAME)
}

pub fn database_path(app_dir: &Path) -> PathBuf {
    app_dir.join(DATABASE_NAME)
}

/// Always `%LOCALAPPDATA%\TunePeerly`. Covers live in `covers` under this directory.
/// Without the variable the cache would land in whatever folder the app was started from,
/// so fall back to the profile's own local folder, then to the temp folder.
/// Only the desktop build uses this: mobile asks Tauri for the platform cache folder instead.
#[cfg_attr(not(desktop), allow(dead_code))]
pub fn cache_dir() -> PathBuf {
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let local = set("LOCALAPPDATA")
        .or_else(|| set("USERPROFILE").map(|home| home.join("AppData").join("Local")))
        .unwrap_or_else(std::env::temp_dir);
    local.join(APP_DIR_NAME)
}
