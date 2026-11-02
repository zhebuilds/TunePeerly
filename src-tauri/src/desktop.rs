// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
//! Window chrome and the system “now playing” surface.
//!
//! Windows replaces the taskbar thumbnail with a media card (the whole cover) and shows the song on the lock screen.
//! The optional corner bar is a second window that only sends commands back to the main one.
//! It appears when the main window is minimized and/or hidden to the tray, as the settings say.
//! Android and iOS have no tray, menus or second window, so those parts build only on desktop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem};
#[cfg(desktop)]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(desktop)]
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent};

static ALLOW_CLOSE: AtomicBool = AtomicBool::new(false);
#[cfg(desktop)]
static TRAY_SHOW: Mutex<Option<MenuItem<tauri::Wry>>> = Mutex::new(None);
#[cfg(desktop)]
static TRAY_QUIT: Mutex<Option<MenuItem<tauri::Wry>>> = Mutex::new(None);

/// Show the corner bar while the main window is minimized.
static MINI_ON: AtomicBool = AtomicBool::new(false);
/// Show the corner bar while the main window is hidden in the tray.
static MINI_TRAY: AtomicBool = AtomicBool::new(true);
/// The user closed the bar with its cross. It stays away until the main window is open again.
static MINI_DISMISSED: AtomicBool = AtomicBool::new(false);
static MINI_SHOWN: AtomicBool = AtomicBool::new(false);
static MINI_PLACED: AtomicBool = AtomicBool::new(false);
static TRACK: LazyLock<Mutex<DesktopTrack>> = LazyLock::new(|| Mutex::new(DesktopTrack::default()));

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

pub fn install(app: &AppHandle) {
    #[cfg(desktop)]
    if let Ok(mini) = WebviewWindowBuilder::new(app, "mini", WebviewUrl::App("index.html".into()))
        .title("Now playing")
        .inner_size(400.0, 76.0)
        .min_inner_size(320.0, 76.0)
        .max_inner_size(480.0, 76.0)
        .decorations(false)
        .shadow(true)
        .background_color(tauri::window::Color(27, 23, 20, 255))
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .build()
    {
        let mini_window = mini.clone();
        mini.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                MINI_DISMISSED.store(true, Ordering::Relaxed);
                MINI_SHOWN.store(false, Ordering::Relaxed);
                let _ = mini_window.hide();
            }
        });
    }
    install_close(app);
    #[cfg(desktop)]
    install_tray(app);
    #[cfg(windows)]
    windows_media::attach(app);
}

fn install_close(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            if ALLOW_CLOSE.load(Ordering::Relaxed) {
                return;
            }
            api.prevent_close();
            let _ = handle.emit("close-requested", ());
        }
    });
}

#[cfg(desktop)]
fn install_tray(app: &AppHandle) {
    let Ok(show_item) = MenuItem::with_id(app, "show", "Show", true, None::<&str>) else { return };
    let Ok(quit_item) = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>) else { return };
    let Ok(menu) = Menu::with_items(app, &[&show_item, &quit_item]) else { return };
    if let Ok(mut slot) = TRAY_SHOW.lock() {
        *slot = Some(show_item);
    }
    if let Ok(mut slot) = TRAY_QUIT.lock() {
        *slot = Some(quit_item);
    }
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("TunePeerly")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let open = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );
            if open {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let _ = builder.build(app);
}

pub fn set_tray_tooltip(app: &AppHandle, name: &str) {
    #[cfg(mobile)]
    let _ = (app, name);
    #[cfg(desktop)]
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(name));
    }
}

pub fn set_tray_labels(show: &str, quit_label: &str) {
    #[cfg(mobile)]
    let _ = (show, quit_label);
    #[cfg(desktop)]
    for (slot, text) in [(&TRAY_SHOW, show), (&TRAY_QUIT, quit_label)] {
        if let Some(item) = slot.lock().ok().as_deref().and_then(Option::as_ref) {
            let _ = item.set_text(text);
        }
    }
}

pub fn quit(app: &AppHandle) {
    ALLOW_CLOSE.store(true, Ordering::Relaxed);
    app.exit(0);
}

pub fn hide_to_tray(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    refresh_mini(app);
}

#[cfg(desktop)]
pub fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    refresh_mini(app);
}

pub fn update(app: &AppHandle, track: DesktopTrack) {
    if let Ok(mut slot) = TRACK.lock() {
        *slot = track.clone();
    }
    let _ = app.emit_to("mini", "desktop-state", &track);
    #[cfg(target_os = "android")]
    crate::android::playback(&track);
    #[cfg(windows)]
    windows_media::publish(app, track);
}

pub fn current() -> DesktopTrack {
    TRACK.lock().map(|guard| guard.clone()).unwrap_or_default()
}

/// `minimized`: show the bar while the window is minimized. `tray`: show it while the window is hidden in the tray.
pub fn sync_mini(app: &AppHandle, minimized: bool, tray: bool) {
    MINI_ON.store(minimized, Ordering::Relaxed);
    MINI_TRAY.store(tray, Ordering::Relaxed);
    refresh_mini(app);
}

/// Shows or hides the corner bar to match the main window and the settings.
pub fn refresh_mini(app: &AppHandle) {
    let Some(main) = app.get_webview_window("main") else { return };
    let visible = main.is_visible().unwrap_or(true);
    let minimized = main.is_minimized().unwrap_or(false);
    if visible && !minimized {
        // The window is open again, so a closed bar may come back the next time.
        MINI_DISMISSED.store(false, Ordering::Relaxed);
    }
    let wanted = (MINI_ON.load(Ordering::Relaxed) && visible && minimized) || (MINI_TRAY.load(Ordering::Relaxed) && !visible);
    apply_mini(app, wanted && !MINI_DISMISSED.load(Ordering::Relaxed));
}

pub fn command(app: &AppHandle, action: &str) {
    match action {
        "restore" => {
            if let Some(main) = app.get_webview_window("main") {
                #[cfg(desktop)]
                let _ = main.unminimize();
                let _ = main.show();
                let _ = main.set_focus();
            }
            refresh_mini(app);
        }
        // The cross on the bar closes only the bar. It returns the next time the window is minimized or hidden.
        "closeMini" => {
            MINI_DISMISSED.store(true, Ordering::Relaxed);
            apply_mini(app, false);
        }
        _ => {
            let _ = app.emit_to("main", "desktop-action", serde_json::json!({ "action": action }));
        }
    }
}

fn apply_mini(app: &AppHandle, show: bool) {
    let Some(mini) = app.get_webview_window("mini") else { return };
    if MINI_SHOWN.swap(show, Ordering::AcqRel) == show {
        return;
    }
    if !show {
        let _ = mini.hide();
        return;
    }
    // The corner is only the first position. After that, a drag is kept.
    if !MINI_PLACED.swap(true, Ordering::AcqRel) {
        if let Some(main) = app.get_webview_window("main") {
            if let Ok(Some(monitor)) = main.current_monitor() {
                let area = monitor.work_area();
                let scale = monitor.scale_factor();
                let width = (400.0 * scale) as i32;
                let height = (76.0 * scale) as i32;
                let x = area.position.x + area.size.width as i32 - width - (16.0 * scale) as i32;
                let y = area.position.y + area.size.height as i32 - height - (16.0 * scale) as i32;
                let _ = mini.set_position(PhysicalPosition::new(x, y));
            }
        }
    }
    let _ = mini.show();
}

#[cfg(windows)]
mod windows_media {
    use std::cell::RefCell;
    use std::sync::Mutex;

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use super::{command, refresh_mini, DesktopTrack};
    use tauri::{AppHandle, Manager};
    use windows::{
        core::{factory, w, HSTRING},
        Foundation::{TimeSpan, TypedEventHandler},
        Media::{
            MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls, SystemMediaTransportControlsButton,
            SystemMediaTransportControlsButtonPressedEventArgs, SystemMediaTransportControlsTimelineProperties,
        },
        Storage::Streams::{DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference},
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
            Graphics::{
                Dwm::{
                    DwmInvalidateIconicBitmaps, DwmSetIconicLivePreviewBitmap, DwmSetIconicThumbnail, DwmSetWindowAttribute,
                    DWMWINDOWATTRIBUTE,
                },
                Gdi::{
                    ClientToScreen, CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, BITMAPINFO,
                    BITMAPINFOHEADER, DIB_RGB_COLORS, HDC, HGDIOBJ,
                },
            },
            System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            System::WinRT::ISystemMediaTransportControlsInterop,
            UI::{
                Shell::{
                    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THUMBBUTTON, THUMBBUTTONMASK, THBF_ENABLED,
                    THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP,
                },
                WindowsAndMessaging::{
                    CreateIconIndirect, GetClientRect, GetSystemMetrics, GetWindowPlacement, GetWindowRect, IsIconic,
                    IsWindowVisible, PostMessageW, RegisterWindowMessageW, ICONINFO, SM_CXSMICON, WINDOWPLACEMENT, WINDOWPOS, WM_COMMAND,
                    WM_SIZE, WM_SYSCOMMAND, WM_WINDOWPOSCHANGING,
                },
            },
        },
    };

    const DWMWA_FORCE_ICONIC_REPRESENTATION: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(7);
    const DWMWA_HAS_ICONIC_BITMAP: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(10);
    const DWMWA_BORDER_COLOR: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(34);
    const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
    const DWMWA_WINDOW_CORNER_PREFERENCE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(33);
    const DWMWCP_ROUND: u32 = 2;
    const SIZE_RESTORED: u32 = 0;
    const SIZE_MINIMIZED: u32 = 1;
    const SIZE_MAXIMIZED: u32 = 2;
    const ID_PREV: u32 = 1;
    const ID_PLAY: u32 = 2;
    const ID_NEXT: u32 = 3;
    const ID_LIKE: u32 = 4;

    struct Shell {
        hwnd: HWND,
        controls: SystemMediaTransportControls,
        taskbar: ITaskbarList3,
        buttons: bool,
        play_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        pause_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        prev_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        next_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        like_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        liked_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        shown: String,
    }

    thread_local! {
        static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    }
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

    pub fn attach(app: &AppHandle) {
        if let Ok(mut slot) = APP.lock() {
            *slot = Some(app.clone());
        }
        let Some(window) = app.get_webview_window("main") else { return };
        let Ok(raw) = window.hwnd() else { return };
        let hwnd = HWND(raw.0);
        unsafe {
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &DWMWA_COLOR_NONE as *const _ as *const _, 4);
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND as *const _ as *const _, 4);
            mark_iconic(hwnd);
            TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarButtonCreated")), Ordering::Relaxed);
            let _ = SetWindowSubclass(hwnd, Some(subclass), 1, 0);
        }
        let Ok(interop) = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>() else { return };
        let Ok(controls) = (unsafe { interop.GetForWindow::<SystemMediaTransportControls>(hwnd) }) else { return };
        let _ = controls.SetIsEnabled(false);
        let _ = controls.SetIsPlayEnabled(true);
        let _ = controls.SetIsPauseEnabled(true);
        let _ = controls.SetIsNextEnabled(true);
        let _ = controls.SetIsPreviousEnabled(true);
        let handler = TypedEventHandler::new(|_, args: windows::core::Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
            if let Some(args) = args.as_ref() {
                if let Ok(button) = args.Button() {
                    let action = match button {
                        SystemMediaTransportControlsButton::Play => "play",
                        SystemMediaTransportControlsButton::Pause => "pause",
                        SystemMediaTransportControlsButton::Next => "next",
                        SystemMediaTransportControlsButton::Previous => "previous",
                        _ => "",
                    };
                    if !action.is_empty() {
                        command_from_ui(action);
                    }
                }
            }
            Ok(())
        });
        let _ = controls.ButtonPressed(&handler);
        let Ok(taskbar) = (unsafe { CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) }) else { return };
        let _ = unsafe { taskbar.HrInit() };
        SHELL.with(|slot| {
            *slot.borrow_mut() = Some(Shell {
                hwnd,
                controls,
                taskbar,
                buttons: false,
                play_icon: glyph_icon(Glyph::Play),
                pause_icon: glyph_icon(Glyph::Pause),
                prev_icon: glyph_icon(Glyph::Prev),
                next_icon: glyph_icon(Glyph::Next),
                like_icon: glyph_icon(Glyph::Heart),
                liked_icon: glyph_icon(Glyph::HeartFill),
                shown: String::new(),
            });
        });
    }

    pub fn publish(app: &AppHandle, track: DesktopTrack) {
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || apply(&app, &track));
    }

    fn command_from_ui(action: &str) {
        if let Some(app) = APP.lock().ok().and_then(|guard| guard.clone()) {
            command(&app, action);
        }
    }

    fn apply(app: &AppHandle, track: &DesktopTrack) {
        SHELL.with(|slot| {
            // The taskbar calls below can let other window messages run first. If one of them is using the shell
            // right now, skip this update; the next one comes within a second.
            let Ok(mut guard) = slot.try_borrow_mut() else { return };
            let Some(shell) = guard.as_mut() else { return };
            let enabled = !track.title.is_empty();
            let signature = format!("{}|{}|{}|{}", track.title, track.artist, track.album, track.cover_path.as_deref().unwrap_or(""));
            let metadata_changed = signature != shell.shown;
            if metadata_changed {
                shell.shown = signature;
            }
            let _ = shell.controls.SetIsEnabled(enabled);
            let _ = shell.controls.SetPlaybackStatus(if !enabled {
                MediaPlaybackStatus::Closed
            } else if track.playing {
                MediaPlaybackStatus::Playing
            } else {
                MediaPlaybackStatus::Paused
            });
            if metadata_changed {
                if let Ok(updater) = shell.controls.DisplayUpdater() {
                    let _ = updater.SetType(MediaPlaybackType::Music);
                    if let Ok(music) = updater.MusicProperties() {
                        let _ = music.SetTitle(&HSTRING::from(&track.title));
                        let _ = music.SetArtist(&HSTRING::from(&track.artist));
                        let _ = music.SetAlbumTitle(&HSTRING::from(&track.album));
                    }
                    if let Some(path) = track.cover_path.as_deref() {
                        if let Some(reference) = stream_from_file(path) {
                            let _ = updater.SetThumbnail(&reference);
                        }
                    }
                    let _ = updater.Update();
                }
            }
            if let Ok(timeline) = SystemMediaTransportControlsTimelineProperties::new() {
                let end = seconds(track.duration);
                let position = seconds(track.position.min(track.duration));
                let _ = timeline.SetStartTime(seconds(0.0));
                let _ = timeline.SetEndTime(end);
                let _ = timeline.SetMinSeekTime(seconds(0.0));
                let _ = timeline.SetMaxSeekTime(end);
                let _ = timeline.SetPosition(position);
                let _ = shell.controls.UpdateTimelineProperties(&timeline);
            }
            refresh_buttons(shell, track);
            if metadata_changed {
                // Decode the cover now, off the interface thread, so the next hover only has to scale it.
                if let Some(path) = track.cover_path.clone() {
                    std::thread::spawn(move || prepare_card(&path));
                }
                let _ = unsafe { DwmInvalidateIconicBitmaps(shell.hwnd) };
            }
            let _ = app;
        });
    }

    fn seconds(value: f64) -> TimeSpan {
        TimeSpan { Duration: (value.max(0.0) * 10_000_000.0) as i64 }
    }

    fn stream_from_file(path: &str) -> Option<RandomAccessStreamReference> {
        let bytes = std::fs::read(path).ok()?;
        let stream = InMemoryRandomAccessStream::new().ok()?;
        let writer = DataWriter::CreateDataWriter(&stream).ok()?;
        writer.WriteBytes(&bytes).ok()?;
        writer.StoreAsync().ok()?.get().ok()?;
        writer.DetachStream().ok()?;
        stream.Seek(0).ok()?;
        RandomAccessStreamReference::CreateFromStream(&stream).ok()
    }

    fn refresh_buttons(shell: &mut Shell, track: &DesktopTrack) {
        let play_tip = if track.playing { &track.tips.pause } else { &track.tips.play };
        let play_icon = if track.playing { shell.pause_icon } else { shell.play_icon };
        let like_icon = if track.favorite { shell.liked_icon } else { shell.like_icon };
        let mut buttons = [
            thumb(ID_PREV, shell.prev_icon, &track.tips.previous),
            thumb(ID_PLAY, play_icon, play_tip),
            thumb(ID_NEXT, shell.next_icon, &track.tips.next),
            thumb(ID_LIKE, like_icon, &track.tips.like),
        ];
        unsafe {
            if shell.buttons {
                let _ = shell.taskbar.ThumbBarUpdateButtons(shell.hwnd, &buttons);
            } else if shell.taskbar.ThumbBarAddButtons(shell.hwnd, &buttons).is_ok() {
                shell.buttons = true;
            } else if shell.taskbar.ThumbBarUpdateButtons(shell.hwnd, &buttons).is_ok() {
                // They were already there, so only the state needed refreshing.
                shell.buttons = true;
            }
        }
        let _ = &mut buttons;
    }

    /// Explorer drops the thumbnail buttons whenever the taskbar button goes away (the window was hidden to the tray)
    /// and tells the window with this message once the button is back. Add them again from the current song.
    static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

    fn taskbar_button_created(hwnd: HWND, msg: u32) {
        unsafe { mark_iconic(hwnd) };
        SHELL.with(|slot| {
            // This message can arrive while apply() is inside a taskbar call and still holds the shell.
            // Borrowing again would panic, so ask for it again once the current work is done.
            let Ok(mut guard) = slot.try_borrow_mut() else {
                let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)) };
                return;
            };
            let Some(shell) = guard.as_mut() else { return };
            shell.buttons = false;
            refresh_buttons(shell, &super::current());
            let _ = unsafe { DwmInvalidateIconicBitmaps(shell.hwnd) };
        });
    }

    /// The card and the peek come from this program, so Windows must ask for them instead of drawing the window.
    unsafe fn mark_iconic(hwnd: HWND) {
        let yes: i32 = 1;
        unsafe {
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_FORCE_ICONIC_REPRESENTATION, &yes as *const _ as *const _, 4);
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_HAS_ICONIC_BITMAP, &yes as *const _ as *const _, 4);
        }
    }

    fn thumb(id: u32, icon: windows::Win32::UI::WindowsAndMessaging::HICON, tip: &str) -> THUMBBUTTON {
        let mut button = THUMBBUTTON::default();
        button.dwMask = THUMBBUTTONMASK(THB_ICON.0 | THB_TOOLTIP.0 | THB_FLAGS.0);
        button.iId = id;
        button.hIcon = icon;
        button.dwFlags = THBF_ENABLED;
        let mut encoded = tip.encode_utf16();
        for slot in button.szTip.iter_mut().take(259) {
            *slot = encoded.next().unwrap_or(0);
            if *slot == 0 {
                break;
            }
        }
        button
    }

    unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        match msg {
            WM_DWMSEND => {
                let width = ((lparam.0 as u32) >> 16) as i32;
                let height = ((lparam.0 as u32) & 0xFFFF) as i32;
                paint_thumbnail(hwnd, width.max(1), height.max(1));
                LRESULT(0)
            }
            WM_DWMSENDICONICLIVEPREVIEWBITMAP => {
                paint_peek(hwnd);
                LRESULT(0)
            }
            _ if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) => {
                taskbar_button_created(hwnd, msg);
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_SYSCOMMAND => {
                // The minimize button and the taskbar both send this first. Keep a picture for the peek.
                if (wparam.0 & 0xFFF0) as u32 == SC_MINIMIZE {
                    remember_window(hwnd);
                }
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_WINDOWPOSCHANGING => {
                // A window that is about to be minimized is moved off screen. Take its picture while it can still be drawn.
                let pos = unsafe { &*(lparam.0 as *const WINDOWPOS) };
                if pos.x == OFFSCREEN && pos.y == OFFSCREEN {
                    remember_window(hwnd);
                }
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_COMMAND => {
                let note = ((wparam.0 as u32) >> 16) as u16;
                let button = (wparam.0 as u32) & 0xFFFF;
                if note == THBN_CLICKED as u16 {
                    let action = match button {
                        ID_PREV => "previous",
                        ID_PLAY => "toggle",
                        ID_NEXT => "next",
                        ID_LIKE => "like",
                        _ => "",
                    };
                    if !action.is_empty() {
                        command_from_ui(action);
                        return LRESULT(0);
                    }
                }
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_SIZE => {
                let kind = wparam.0 as u32;
                if kind == SIZE_MINIMIZED || kind == SIZE_RESTORED || kind == SIZE_MAXIMIZED {
                    if let Some(app) = APP.lock().ok().and_then(|guard| guard.clone()) {
                        refresh_mini(&app);
                    }
                }
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }

    const WM_DWMSEND: u32 = 0x0323;
    const WM_DWMSENDICONICLIVEPREVIEWBITMAP: u32 = 0x0326;

    /// Top-down BGRA pixels, opaque.
    struct Pixels {
        width: i32,
        height: i32,
        bgra: Vec<u8>,
    }

    /// The cover as decoded, at most `COVER_EDGE` on its longest side. Decoded once per song.
    static COVER: Mutex<Option<(String, Arc<image::RgbaImage>)>> = Mutex::new(None);
    /// The last card that was drawn, with the box it was drawn for.
    static CARD: Mutex<Option<(String, i32, i32, Arc<Pixels>)>> = Mutex::new(None);
    /// The size of the box Windows last asked for, so a new song's card can be made before the next hover.
    static LAST_BOX: Mutex<(i32, i32)> = Mutex::new((0, 0));
    const COVER_EDGE: u32 = 640;

    fn cover_image(path: &str) -> Option<Arc<image::RgbaImage>> {
        if let Some((known, image)) = COVER.lock().ok()?.as_ref() {
            if known == path {
                return Some(image.clone());
            }
        }
        let mut rgba = image::open(path).ok()?.to_rgba8();
        let (width, height) = rgba.dimensions();
        if width == 0 || height == 0 {
            return None;
        }
        let longest = width.max(height);
        if longest > COVER_EDGE {
            let scale = COVER_EDGE as f32 / longest as f32;
            let to_w = ((width as f32 * scale).round() as u32).max(1);
            let to_h = ((height as f32 * scale).round() as u32).max(1);
            rgba = image::imageops::resize(&rgba, to_w, to_h, image::imageops::FilterType::Triangle);
        }
        let image = Arc::new(rgba);
        if let Ok(mut slot) = COVER.lock() {
            *slot = Some((path.to_string(), image.clone()));
        }
        Some(image)
    }

    /// The whole cover, scaled down in proportion to fit inside the box. Nothing is cropped, so a wide or tall
    /// cover gives a wide or tall card. Without a cover it is a plain dark square.
    fn card(max_w: i32, max_h: i32, path: Option<&str>) -> Arc<Pixels> {
        let key = path.unwrap_or("").to_string();
        if let Ok(slot) = CARD.lock() {
            if let Some((known, w, h, pixels)) = slot.as_ref() {
                if *known == key && *w == max_w && *h == max_h {
                    return pixels.clone();
                }
            }
        }
        let drawn = match path.and_then(cover_image) {
            Some(image) => {
                let (source_w, source_h) = image.dimensions();
                let scale = (max_w as f32 / source_w as f32).min(max_h as f32 / source_h as f32);
                let to_w = ((source_w as f32 * scale).round() as i32).clamp(1, max_w);
                let to_h = ((source_h as f32 * scale).round() as i32).clamp(1, max_h);
                let fitted = if (to_w as u32, to_h as u32) == (source_w, source_h) {
                    (*image).clone()
                } else {
                    image::imageops::resize(&*image, to_w as u32, to_h as u32, image::imageops::FilterType::CatmullRom)
                };
                let mut bgra = Vec::with_capacity((to_w * to_h * 4) as usize);
                for pixel in fitted.pixels() {
                    bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 0xFF]);
                }
                Pixels { width: to_w, height: to_h, bgra }
            }
            None => {
                let edge = max_w.min(max_h).clamp(1, 512);
                let bgra = [0x14u8, 0x12, 0x10, 0xFF].repeat((edge * edge) as usize);
                Pixels { width: edge, height: edge, bgra }
            }
        };
        let drawn = Arc::new(drawn);
        if let Ok(mut slot) = CARD.lock() {
            *slot = Some((key, max_w, max_h, drawn.clone()));
        }
        drawn
    }

    /// Prepares the card for the box Windows last used, so the hover after a song change has nothing to compute.
    fn prepare_card(path: &str) {
        let _ = cover_image(path);
        let (w, h) = LAST_BOX.lock().map(|guard| *guard).unwrap_or((0, 0));
        if w > 0 && h > 0 {
            let _ = card(w, h, Some(path));
        }
    }

    fn make_bitmap(width: i32, height: i32, bgra: &[u8]) -> Option<windows::Win32::Graphics::Gdi::HBITMAP> {
        if width <= 0 || height <= 0 || bgra.len() < (width * height * 4) as usize {
            return None;
        }
        unsafe {
            let screen = GetDC(None);
            let info = dib_info(width, height);
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok();
            ReleaseDC(None, screen);
            let bitmap = bitmap?;
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                return None;
            }
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, (width * height * 4) as usize);
            Some(bitmap)
        }
    }

    /// The small card in the taskbar popup.
    fn paint_thumbnail(hwnd: HWND, max_w: i32, max_h: i32) {
        if let Ok(mut last) = LAST_BOX.lock() {
            *last = (max_w, max_h);
        }
        let track = super::current();
        let pixels = card(max_w, max_h, track.cover_path.as_deref());
        let Some(bitmap) = make_bitmap(pixels.width, pixels.height, &pixels.bgra) else { return };
        unsafe {
            let _ = DwmSetIconicThumbnail(hwnd, bitmap, 0);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
    }

    /// The card centered on a dark canvas of the given size, for when no window picture exists.
    fn card_canvas(width: i32, height: i32) -> Option<windows::Win32::Graphics::Gdi::HBITMAP> {
        let track = super::current();
        let pixels = card(width, height, track.cover_path.as_deref());
        let mut canvas = [0x14u8, 0x12, 0x10, 0xFF].repeat((width * height) as usize);
        let left = ((width - pixels.width) / 2).max(0);
        let top = ((height - pixels.height) / 2).max(0);
        for row in 0..pixels.height.min(height) {
            let from = (row * pixels.width * 4) as usize;
            let to = (((top + row) * width + left) * 4) as usize;
            let take = (pixels.width.min(width - left) * 4) as usize;
            canvas[to..to + take].copy_from_slice(&pixels.bgra[from..from + take]);
        }
        make_bitmap(width, height, &canvas)
    }

    /// The last picture of the window's client area, kept so a minimized window can still be previewed.
    struct Snapshot {
        width: i32,
        height: i32,
        /// Where the client area starts inside the window rectangle.
        offset: POINT,
        /// Top-down BGRA, one word per pixel.
        pixels: Vec<u32>,
        taken: std::time::Instant,
    }

    thread_local! {
        static SNAPSHOT: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(hwnd: HWND, hdc: HDC, flags: u32) -> i32;
    }
    const PW_RENDERFULLCONTENT: u32 = 2;
    const SC_MINIMIZE: u32 = 0xF020;
    const OFFSCREEN: i32 = -32000;
    /// A picture younger than this is reused, so a hover answers at once.
    const SNAPSHOT_FRESH: std::time::Duration = std::time::Duration::from_millis(1500);

    fn dib_info(width: i32, height: i32) -> BITMAPINFO {
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = 0;
        info
    }

    /// Takes the real interface (WebView included) as it is drawn now. None while the window is minimized or hidden.
    fn capture_window(hwnd: HWND) -> Option<Snapshot> {
        unsafe {
            if IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                return None;
            }
            let mut frame = RECT::default();
            let mut client = RECT::default();
            GetWindowRect(hwnd, &mut frame).ok()?;
            GetClientRect(hwnd, &mut client).ok()?;
            let (full_w, full_h) = (frame.right - frame.left, frame.bottom - frame.top);
            let (width, height) = (client.right - client.left, client.bottom - client.top);
            if full_w <= 0 || full_h <= 0 || width <= 0 || height <= 0 || full_w > 8192 || full_h > 8192 {
                return None;
            }
            let mut origin = POINT { x: 0, y: 0 };
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return None;
            }
            let offset = POINT { x: origin.x - frame.left, y: origin.y - frame.top };

            let screen = GetDC(None);
            let info = dib_info(full_w, full_h);
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok();
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let Some(bitmap) = bitmap else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, HGDIOBJ(bitmap.0));
            let drawn = !bits.is_null() && PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT) != 0;
            SelectObject(dc, old);
            let _ = DeleteDC(dc);
            let mut snapshot = None;
            if drawn {
                let source = std::slice::from_raw_parts(bits as *const u32, (full_w * full_h) as usize);
                let mut pixels = vec![0u32; (width * height) as usize];
                let from_x = offset.x.max(0);
                let take = width.min(full_w - from_x);
                if take > 0 {
                    for row in 0..height {
                        let from_y = row + offset.y;
                        if from_y < 0 || from_y >= full_h {
                            continue;
                        }
                        let src = (from_y * full_w + from_x) as usize;
                        let dst = (row * width) as usize;
                        pixels[dst..dst + take as usize].copy_from_slice(&source[src..src + take as usize]);
                    }
                }
                for pixel in pixels.iter_mut() {
                    *pixel |= 0xFF00_0000;
                }
                snapshot = Some(Snapshot { width, height, offset, pixels, taken: std::time::Instant::now() });
            }
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            snapshot
        }
    }

    fn remember_window(hwnd: HWND) {
        if let Some(snapshot) = capture_window(hwnd) {
            SNAPSHOT.with(|slot| {
                if let Ok(mut slot) = slot.try_borrow_mut() {
                    *slot = Some(snapshot);
                }
            });
        }
    }

    fn snapshot_bitmap(snapshot: &Snapshot) -> Option<windows::Win32::Graphics::Gdi::HBITMAP> {
        let bytes = unsafe { std::slice::from_raw_parts(snapshot.pixels.as_ptr() as *const u8, snapshot.pixels.len() * 4) };
        make_bitmap(snapshot.width, snapshot.height, bytes)
    }

    /// The big preview when the taskbar popup is hovered: the real interface. An open window is drawn now
    /// (or reused if it was drawn a moment ago); a minimized one cannot be drawn, so it shows the picture
    /// taken just before it was minimized.
    fn paint_peek(hwnd: HWND) {
        let stale = SNAPSHOT.with(|slot| slot.borrow().as_ref().is_none_or(|snapshot| snapshot.taken.elapsed() > SNAPSHOT_FRESH));
        if stale {
            remember_window(hwnd);
        }
        let shown = SNAPSHOT.with(|slot| {
            let slot = slot.borrow();
            let snapshot = slot.as_ref()?;
            let bitmap = snapshot_bitmap(snapshot)?;
            unsafe {
                let _ = DwmSetIconicLivePreviewBitmap(hwnd, bitmap, Some(&snapshot.offset), 0);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            Some(())
        });
        if shown.is_some() {
            return;
        }
        // Nothing was ever drawn (for example it started minimized): show the card at the restored size.
        let mut place = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        if unsafe { GetWindowPlacement(hwnd, &mut place) }.is_err() {
            return;
        }
        let area = place.rcNormalPosition;
        let width = (area.right - area.left).clamp(1, 4096);
        let height = (area.bottom - area.top).clamp(1, 4096);
        let Some(bitmap) = card_canvas(width, height) else { return };
        unsafe {
            let _ = DwmSetIconicLivePreviewBitmap(hwnd, bitmap, None, 0);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
    }

    #[derive(Clone, Copy)]
    enum Glyph { Play, Pause, Prev, Next, Heart, HeartFill }

    fn glyph_icon(kind: Glyph) -> windows::Win32::UI::WindowsAndMessaging::HICON {
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(20, 40);
        hicon_from_bgra(&draw_glyph(kind, size), size)
    }

    fn draw_glyph(kind: Glyph, size: i32) -> Vec<u8> {
        let step = 8i32;
        let big = size * step;
        let heart = matches!(kind, Glyph::Heart | Glyph::HeartFill);
        let zoom = if heart { 1.25f32 } else { 1.3f32 };
        // 0 = empty, 1 = outline, 2 = fill
        let mut mask = vec![0u8; (big * big) as usize];
        for y in 0..big {
            for x in 0..big {
                let u = (x as f32 + 0.5) / big as f32;
                let v = (y as f32 + 0.5) / big as f32;
                let su = 0.5 + (u - 0.5) / zoom;
                let sv = 0.5 + (v - 0.5) / zoom;
                let value = if heart {
                    if !heart_shape(su, sv, 0.0) {
                        0
                    } else if heart_shape(su, sv, 0.30) {
                        2
                    } else {
                        1
                    }
                } else if shape_covers(kind, su, sv) {
                    1
                } else {
                    0
                };
                mask[(y * big + x) as usize] = value;
            }
        }
        let outline = match kind {
            Glyph::HeartFill => (232u32, 84u32, 96u32),
            _ => (255u32, 255u32, 255u32),
        };
        let fill = match kind {
            Glyph::HeartFill => (232u32, 84u32, 96u32),
            Glyph::Heart => (255u32, 255u32, 255u32),
            _ => outline,
        };
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        let samples = (step * step) as u32;
        for y in 0..size {
            for x in 0..size {
                let (mut line, mut body) = (0u32, 0u32);
                for oy in 0..step {
                    for ox in 0..step {
                        match mask[((y * step + oy) * big + x * step + ox) as usize] {
                            1 => line += 1,
                            2 => body += 1,
                            _ => {}
                        }
                    }
                }
                let coverage = (line + body) * 255 / samples;
                let index = ((y * size + x) * 4) as usize;
                pixels[index] = ((outline.2 * line + fill.2 * body) / samples) as u8;
                pixels[index + 1] = ((outline.1 * line + fill.1 * body) / samples) as u8;
                pixels[index + 2] = ((outline.0 * line + fill.0 * body) / samples) as u8;
                pixels[index + 3] = coverage as u8;
            }
        }
        pixels
    }

    fn shape_covers(kind: Glyph, u: f32, v: f32) -> bool {
        match kind {
            Glyph::Play => rounded_tri(u, v, (0.36, 0.26), (0.36, 0.74), (0.72, 0.50)),
            Glyph::Pause => rounded_box(u, v, 0.30, 0.42, 0.24, 0.76) || rounded_box(u, v, 0.58, 0.70, 0.24, 0.76),
            Glyph::Prev => {
                rounded_box(u, v, 0.24, 0.34, 0.24, 0.76) || rounded_tri(u, v, (0.70, 0.28), (0.70, 0.72), (0.42, 0.50))
            }
            Glyph::Next => {
                rounded_box(u, v, 0.66, 0.76, 0.24, 0.76) || rounded_tri(u, v, (0.30, 0.28), (0.30, 0.72), (0.58, 0.50))
            }
            Glyph::HeartFill => heart_shape(u, v, 0.0),
            Glyph::Heart => heart_shape(u, v, 0.0),
        }
    }

    /// The classic heart: a square turned 45 degrees with a round lobe on each upper side.
    /// `shrink` > 0 scales it toward the middle, which gives an even outline when subtracted.
    fn heart_shape(u: f32, v: f32, shrink: f32) -> bool {
        let scale = 1.0 - shrink;
        let (cx, cy) = (0.5f32, 0.52f32);
        let u = cx + (u - cx) / scale;
        let v = cy + (v - cy) / scale;
        let half = 0.2546f32;
        let square = (u - 0.5).abs() + (v - 0.56).abs() <= half;
        let radius = 0.18f32;
        let left = (u - 0.3725).powi(2) + (v - 0.4325).powi(2) <= radius * radius;
        let right = (u - 0.6275).powi(2) + (v - 0.4325).powi(2) <= radius * radius;
        square || left || right
    }

    fn rounded_box(u: f32, v: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> bool {
        let r = ((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
        let cx = u.clamp(x0 + r, x1 - r);
        let cy = v.clamp(y0 + r, y1 - r);
        (u - cx).powi(2) + (v - cy).powi(2) <= r * r
    }

    fn rounded_tri(u: f32, v: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
        point_in_triangle(u, v, a, b, c) || edge_dist(u, v, a, b).min(edge_dist(u, v, b, c)).min(edge_dist(u, v, c, a)) <= 0.05
    }

    fn edge_dist(u: f32, v: f32, p: (f32, f32), q: (f32, f32)) -> f32 {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let t = (((u - p.0) * dx + (v - p.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((u - (p.0 + t * dx)).powi(2) + (v - (p.1 + t * dy)).powi(2)).sqrt()
    }

    fn point_in_triangle(u: f32, v: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
        let sign = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1);
        let d1 = sign((u, v), a, b);
        let d2 = sign((u, v), b, c);
        let d3 = sign((u, v), c, a);
        !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
    }

    fn hicon_from_bgra(pixels: &[u8], size: i32) -> windows::Win32::UI::WindowsAndMessaging::HICON {
        unsafe {
            let screen = GetDC(None);
            let mut info = BITMAPINFO::default();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = size;
            info.bmiHeader.biHeight = -size;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let color = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
            if !bits.is_null() {
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len().min((size * size * 4) as usize));
            }
            let stride = ((size + 15) / 16) * 2;
            let mut mask_bits = vec![0xFFu8; (stride * size) as usize];
            for y in 0..size {
                for x in 0..size {
                    let alpha = pixels.get(((y * size + x) * 4 + 3) as usize).copied().unwrap_or(0);
                    if alpha > 32 {
                        let index = (y * stride + x / 8) as usize;
                        mask_bits[index] &= !(0x80u8 >> (x % 8));
                    }
                }
            }
            let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
            let icon_info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
            let icon = CreateIconIndirect(&icon_info).unwrap_or_default();
            let _ = DeleteObject(HGDIOBJ(color.0));
            let _ = DeleteObject(HGDIOBJ(mask.0));
            ReleaseDC(None, screen);
            icon
        }
    }

}

