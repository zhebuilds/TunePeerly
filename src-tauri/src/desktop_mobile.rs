// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
// Mobile stand-in for `desktop.rs`.
//
// The desktop module drives things a phone or a tablet does not have: the tray icon,
// the mini player window, the taskbar buttons and the "hide to tray" close
// behaviour. The commands below keep the same names and the same payloads so the
// frontend never has to care which build it is talking to; every one of them is a
// no-op on mobile.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(default)]
    pub cover_path: Option<String>,
    pub playing: bool,
    pub favorite: bool,
    pub duration: f64,
    pub position: f64,
    #[serde(default = "dark")]
    pub theme: String,
    #[serde(default = "english")]
    pub language: String,
    pub tips: Tips,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tips {
    pub previous: String,
    pub play: String,
    pub pause: String,
    pub next: String,
    pub like: String,
}

fn dark() -> String {
    "dark".into()
}
fn english() -> String {
    "en".into()
}

impl Default for Tips {
    fn default() -> Self {
        Self { previous: "Previous".into(), play: "Play".into(), pause: "Pause".into(), next: "Next".into(), like: "Like".into() }
    }
}

impl Default for DesktopTrack {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            cover_path: None,
            playing: false,
            favorite: false,
            duration: 0.0,
            position: 0.0,
            theme: dark(),
            language: english(),
            tips: Tips::default(),
        }
    }
}

pub fn install(_app: &AppHandle) {}

pub fn set_tray_tooltip(_app: &AppHandle, _name: &str) {}

pub fn set_tray_labels(_show: &str, _quit_label: &str) {}

pub fn quit(app: &AppHandle) {
    app.exit(0);
}

pub fn hide_to_tray(_app: &AppHandle) {}

#[allow(dead_code)]
pub fn show_main(_app: &AppHandle) {}

pub fn update(_app: &AppHandle, _track: DesktopTrack) {}

pub fn current() -> DesktopTrack {
    DesktopTrack::default()
}

pub fn sync_mini(_app: &AppHandle, _minimized: bool, _tray: bool) {}

#[allow(dead_code)]
pub fn refresh_mini(_app: &AppHandle) {}

pub fn command(_app: &AppHandle, _action: &str) {}
