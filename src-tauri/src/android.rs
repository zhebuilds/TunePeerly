// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
//! The bridge to the Kotlin side (`TunePlugin.kt`): the Keystore that wraps the database key, the
//! system file picker, the media session the notification, lock screen and widget read, the phone's
//! name, and network changes.
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

/// A command for the phone's own player (`NativeAudio.kt`), which plays the songs on Android.
pub fn audio(args: &serde_json::Value) -> AppResult<()> {
    call::<Ignored>("audio", args).map(|_| ())
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

/// The name the user gave the phone (Settings, About phone), else its model. None when Kotlin has none.
pub fn device_name() -> Option<String> {
    let named: Named = call("deviceName", serde_json::json!({})).ok()?;
    let name = named.name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkWatch {
    on_event: tauri::ipc::Channel<serde_json::Value>,
}

/// Calls `on_change` whenever Android reports the network came, went or changed. Kotlin calls into
/// Rust directly, so this works while the page's script is held back with the screen off.
pub fn watch_network(on_change: impl Fn() + Send + Sync + 'static) -> AppResult<()> {
    let channel = tauri::ipc::Channel::new(move |_body| {
        on_change();
        Ok(())
    });
    call::<serde_json::Value>("networkWatch", NetworkWatch { on_event: channel }).map(|_| ())
}
