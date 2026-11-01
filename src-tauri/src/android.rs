// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
//! The bridge to the Kotlin side (`TunePlugin.kt`): the Keystore that wraps the database key, the
//! system file picker, and the media session the notification, lock screen and widget read.
//! Calls block until Kotlin answers, so they are made from command or setup threads, never the UI thread.
use std::path::Path;
use std::sync::OnceLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::Wry;

use crate::desktop::DesktopTrack;
use crate::error::{AppError, AppResult};

static HANDLE: OnceLock<PluginHandle<Wry>> = OnceLock::new();

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("tpandroid")
        .setup(|_app, api| {
            let handle = api.register_android_plugin("com.zhebuilds.tunepeerly", "TunePlugin")?;
            let _ = HANDLE.set(handle);
            Ok(())
        })
        .build()
}

fn call<T: serde::de::DeserializeOwned>(command: &str, payload: impl Serialize) -> AppResult<T> {
    let handle = HANDLE.get().ok_or_else(|| AppError::Message("android bridge not ready".into()))?;
    handle
        .run_mobile_plugin(command, payload)
        .map_err(|err| AppError::Message(format!("android {command}: {err}")))
}

#[derive(Serialize, Deserialize)]
struct Blob {
    data: String,
}

fn blob_call(command: &str, bytes: &[u8]) -> AppResult<Vec<u8>> {
    let out: Blob = call(command, Blob { data: STANDARD.encode(bytes) })?;
    STANDARD.decode(out.data).map_err(|err| AppError::Message(format!("android {command}: {err}")))
}

/// Wraps the database key with an AES-GCM key that never leaves the Android Keystore.
pub fn seal(plain: &[u8]) -> AppResult<Vec<u8>> {
    blob_call("seal", plain)
}

pub fn unseal(sealed: &[u8]) -> AppResult<Vec<u8>> {
    blob_call("unseal", sealed)
}

#[derive(Serialize)]
struct PickArgs<'a> {
    folder: bool,
    dest: &'a str,
}

#[derive(Deserialize)]
struct Picked {
    count: u32,
}

/// Opens the system picker and copies what was chosen into `dest`. Returns how many songs were copied.
pub fn pick(folder: bool, dest: &Path) -> AppResult<u32> {
    std::fs::create_dir_all(dest)?;
    let dest = dest.to_string_lossy();
    let picked: Picked = call("pick", PickArgs { folder, dest: &dest })?;
    Ok(picked.count)
}

#[derive(Deserialize)]
struct Ignored {}

/// Mirrors what the player shows into the media session, its notification and the home-screen widget.
pub fn playback(track: &DesktopTrack) {
    let _ = call::<Ignored>("playback", track);
}
