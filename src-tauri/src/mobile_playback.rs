// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

//! Bridges the web view player to the Android foreground service.
//!
//! The player is an `<audio>` element inside the web view, so Android cannot see it. The service in
//! `PlaybackService.kt` draws the notification and the media session, the front end pushes the current
//! title, artist and position here, and the notification buttons come back the other way as a Tauri
//! event the front end turns into `toggle`/`next`/`previous`.
//!
//! Every function is a no-op on the desktop, where the notification is the operating system's.

#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "android")]
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Keeps the app handle around: the media session calls in from an Android thread, where no Tauri
/// window or state is in scope.
pub fn remember(app: &tauri::AppHandle) {
    #[cfg(target_os = "android")]
    {
        let _ = APP.set(app.clone());
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
    }
}

/// Mirrors the player into the notification. Called about once a second while something is playing.
#[tauri::command]
pub fn mobile_playback_update(
    title: String,
    artist: String,
    playing: bool,
    position_ms: u64,
    duration_ms: u64,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let payload = serde_json::json!({
            "title": title,
            "artist": artist,
            "playing": playing,
            "positionMs": position_ms,
            "durationMs": duration_ms,
        })
        .to_string();
        return activity_call("updatePlayback", "(Ljava/lang/String;)V", &payload);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (title, artist, playing, position_ms, duration_ms);
        Ok(())
    }
}

#[cfg(target_os = "android")]
fn activity_call(method: &str, signature: &str, payload: &str) -> Result<(), String> {
    use jni::objects::{JObject, JValue};

    let context = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }.map_err(|err| format!("vm: {err}"))?;
    let mut env = vm.attach_current_thread().map_err(|err| format!("thread: {err}"))?;
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let text = env.new_string(payload).map_err(|err| format!("string: {err}"))?;
    env.call_method(&activity, method, signature, &[JValue::Object(&text)])
        .map_err(|err| format!("{method}: {err}"))?;
    Ok(())
}

/// Called from `NativeBridge.mediaAction` in Kotlin, on whichever thread the media session used.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_zhebuilds_tunepeerly_NativeBridge_mediaAction(
    mut env: jni::JNIEnv,
    _this: jni::objects::JObject,
    action: jni::objects::JString,
    position_ms: jni::sys::jlong,
) {
    let action = match env.get_string(&action) {
        Ok(text) => String::from(text),
        Err(err) => {
            eprintln!("TunePeerly cannot read the media action: {err}");
            return;
        }
    };
    let Some(app) = APP.get() else { return };
    let payload = serde_json::json!({ "action": action, "positionMs": position_ms });
    let _ = app.emit("mobile-media-action", payload);
}
