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
#[cfg(not(target_os = "android"))]
pub fn cache_dir() -> PathBuf {
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let local = set("LOCALAPPDATA")
        .or_else(|| set("USERPROFILE").map(|home| home.join("AppData").join("Local")))
        .unwrap_or_else(std::env::temp_dir);
    local.join(APP_DIR_NAME)
}

/// Android keeps everything in the app's own internal storage, which no other app can read and the
/// system deletes with the app. Covers go to its cache folder, songs the user adds to `Music`.
#[cfg(target_os = "android")]
pub fn android_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join(APP_DIR_NAME)
}

#[cfg(target_os = "android")]
pub fn android_imported_dir(app_dir: &Path) -> PathBuf {
    app_dir.join("Music").join("Imported")
}

#[cfg(target_os = "android")]
pub fn android_downloads_dir(app_dir: &Path) -> PathBuf {
    app_dir.join("Music").join("Downloads")
}
