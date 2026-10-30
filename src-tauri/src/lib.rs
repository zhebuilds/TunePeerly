// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
#[cfg(desktop)]
mod desktop;
#[cfg(not(desktop))]
#[path = "desktop_mobile.rs"]
mod desktop;
mod catalog;
mod media;
mod config;
mod connect;
mod db;
mod error;
mod frames;
mod identity;
mod ids;
mod mobile_playback;
mod net;
mod relay;
mod scan;
mod sync_logic;
mod tls;

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::identity::IdentityMaterial;

struct AppState {
    db: Arc<Mutex<Db>>,
    net: net::Network,
    material: IdentityMaterial,
}

const NETWORK_KEYS: [&str; 8] = [
    "discovery",
    "incognito",
    "relayHost",
    "relayPort",
    "relayDeviceId",
    "relayToken",
    "displayName",
    "listenPort",
];

fn online_ids(state: &AppState) -> Vec<String> {
    let live = state.net.live.inner.lock().unwrap_or_else(PoisonError::into_inner);
    live.links.iter().filter(|link| link.state == "online").map(|link| link.device_id.clone()).collect()
}

#[tauri::command(async)]
fn bootstrap(state: State<'_, AppState>) -> AppResult<Value> {
    let online = online_ids(&state);
    let local_addresses = net::lan_ipv4().iter().map(|ip| ip.to_string()).collect::<Vec<_>>();
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    let live = state.net.live.inner.lock().unwrap_or_else(PoisonError::into_inner);
    let blocks = db.blocks()?;
    let notices = db.notices()?;
    let nearby: Vec<_> = live.nearby.iter().filter(|item| !blocks.iter().any(|row| row.device_id == item.device_id)).cloned().collect();
    let pending = db.setting_json("pendingSnapshots")?;
    let pending = pending
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(feature, item)| json!({"feature": feature, "source": item.get("source").cloned().unwrap_or(Value::Null)}))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "deviceId": state.material.device_id,
        "certId": state.material.cert_id,
        "deviceName": db.setting_string("displayName").unwrap_or_default(),
        "pairingText": identity::pairing_text(
            &state.material.identity.identity_key().serialize(),
            &state.material.cert_der,
            &db.setting_string("displayName").unwrap_or_default(),
        ),
        "settings": db.device_settings_map()?,
        "theme": db.synced("theme")?,
        "language": db.synced("language")?,
        "grants": db.grants()?,
        "playlists": db.playlists()?,
        "player": db.player_json()?,
        "history": db.history_keys()?,
        "pending": pending,
        "tracks": catalog::library(&db, &online)?,
        "favoriteIds": db.favorite_content_ids()?,
        "listenPort": live.listen_port,
        "portFallback": live.port_fallback,
        "localAddresses": local_addresses,
        "nearby": nearby,
        "links": live.links,
        "relay": live.relay,
        "offer": live.offer,
        "download": live.download,
        "downloadQueue": live.download_queue,
        "downloadHistory": db.setting_json("downloadHistory").unwrap_or(json!([])),
        "manual": live.manual,
        "failures": live.failures,
        "blocks": blocks,
        "notices": notices,
    }))
}

#[tauri::command(async)]
fn import_paths(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>, kind: String) -> AppResult<Value> {
    let mut summaries = Vec::new();
    for path in &paths {
        let summary = scan::import_path(&state.db, Path::new(path), &kind)?;
        if !allow_grant(&app, &summary.kind, path) {
            let _ = state.db.lock().unwrap_or_else(PoisonError::into_inner).delete_grant(summary.grant_id, false);
            return Err(AppError::Message("file.unreadable".into()));
        }
        summaries.push(summary);
    }
    let _ = state.net.tx.send(net::Cmd::Publish);
    let _ = app.emit("library-updated", json!({}));
    Ok(json!(summaries))
}

#[tauri::command(async)]
fn rescan(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let grants = state.db.lock().unwrap_or_else(PoisonError::into_inner).grants()?;
    for grant in grants {
        if Path::new(&grant.path).exists() {
            // A drive that was connected after start-up is not in the asset scope yet.
            allow_grant(&app, &grant.kind, &grant.path);
            let _ = scan::import_path(&state.db, Path::new(&grant.path), &grant.kind);
        }
    }
    scan::refresh_access(&state.db)?;
    let _ = state.net.tx.send(net::Cmd::Publish);
    let _ = app.emit("library-updated", json!({}));
    Ok(())
}

fn allow_grant(app: &AppHandle, kind: &str, path: &str) -> bool {
    let scope = app.asset_protocol_scope();
    if kind == "directory" {
        scope.allow_directory(path, true).is_ok()
    } else {
        scope.allow_file(path).is_ok()
    }
}

#[tauri::command(async)]
fn remove_grant(app: AppHandle, state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    // The asset scope is left alone on purpose. A forbidden path wins over any later allow and cannot
    // be lifted, so adding the same folder again would fail every play with "format not supported".
    // Its songs stay in the library as unavailable, and nothing plays an unavailable song.
    db.delete_grant(id, true)?;
    drop(db);
    let _ = state.net.tx.send(net::Cmd::Publish);
    let _ = app.emit("library-updated", json!({}));
    Ok(())
}

#[tauri::command(async)]
fn library(state: State<'_, AppState>) -> AppResult<Value> {
    let online = online_ids(&state);
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    catalog::library(&db, &online)
}

#[tauri::command(async)]
fn search(state: State<'_, AppState>, query: String) -> AppResult<Value> {
    let online = online_ids(&state);
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    catalog::search(&db, &query, &online)
}

#[tauri::command(async)]
fn lyrics(state: State<'_, AppState>, local_file_id: String) -> AppResult<String> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).lyrics(&local_file_id)
}

#[tauri::command(async)]
fn save_player(state: State<'_, AppState>, state_json: Value) -> AppResult<()> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).save_player_json(&state_json)
}

#[tauri::command(async)]
fn save_progress(state: State<'_, AppState>, position_sec: f64) -> AppResult<()> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).save_progress(position_sec)
}

#[tauri::command(async)]
fn record_history(state: State<'_, AppState>, content_id: Option<String>, local_file_id: Option<String>) -> AppResult<()> {
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    if db.setting_bool("incognito").unwrap_or(false) || db.setting_bool("historyPaused").unwrap_or(false) {
        return Ok(());
    }
    db.record_history_local(content_id.as_deref(), local_file_id.as_deref())?;
    let sync = db.setting_bool("historySync").unwrap_or(false);
    drop(db);
    if sync {
        let _ = state.net.tx.send(net::Cmd::AutoSync("history".into()));
    }
    Ok(())
}

#[tauri::command(async)]
fn reveal_path(path: String) -> AppResult<()> {
    let file = Path::new(&path);
    if !file.is_file() {
        return Err(AppError::Message("file.missing".into()));
    }
    tauri_plugin_opener::reveal_item_in_dir(file).map_err(|err| AppError::Message(err.to_string()))
}

#[tauri::command(async)]
fn show_in_folder(state: State<'_, AppState>, local_file_id: String) -> AppResult<()> {
    let path: String = state.db.lock().unwrap_or_else(PoisonError::into_inner).conn.query_row(
        "SELECT absolute_path FROM local_files WHERE local_file_id = ?1",
        rusqlite::params![local_file_id],
        |row| row.get(0),
    )?;
    if !Path::new(&path).exists() {
        return Err(AppError::Message("file.missing".into()));
    }
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|err| AppError::Message(err.to_string()))
}

#[tauri::command(async)]
fn playback_result(
    state: State<'_, AppState>,
    local_file_id: Option<String>,
    content_id: Option<String>,
    status: String,
    reason: Option<String>,
) -> AppResult<()> {
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(id) = &local_file_id {
        db.conn.execute(
            "UPDATE local_files SET playback_status = ?1, playback_reason = ?2 WHERE local_file_id = ?3",
            rusqlite::params![status, reason, id],
        )?;
    }
    if status == "playable" {
        let _ = db.clear_transport_notes("local");
        if let Some(content_id) = &content_id {
            db.clear_unplayable(content_id)?;
        }
    } else if matches!(reason.as_deref(), Some("playback.decodeFailed") | Some("playback.unsupported") | Some("playback.video")) {
        if let Some(content_id) = &content_id {
            let class_name = if reason.as_deref() == Some("playback.unsupported") || reason.as_deref() == Some("playback.video") {
                "unsupported"
            } else {
                "decode"
            };
            db.note_failure(Some("local"), Some(content_id), local_file_id.as_deref(), class_name, reason.as_deref().unwrap_or(""), "local")?;
        }
    } else if status == "failed" && reason.as_deref() == Some("file.missing") {
        if let Some(id) = &local_file_id {
            db.conn.execute(
                "UPDATE local_files SET access_status = 'missing', access_reason = 'file.missing' WHERE local_file_id = ?1",
                rusqlite::params![id],
            )?;
        }
    }
    Ok(())
}

#[tauri::command(async)]
fn remote_failure(state: State<'_, AppState>, content_id: String, device_id: String, class: String, reason: String) -> AppResult<()> {
    state
        .db
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .note_failure(Some(&device_id), Some(&content_id), None, &class, &reason, &device_id)
}

#[tauri::command(async)]
fn toggle_favorite(state: State<'_, AppState>, content_id: Option<String>, local_file_id: Option<String>) -> AppResult<bool> {
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    let added = db.toggle_favorite(content_id.as_deref(), local_file_id.as_deref())?;
    db.mark_dirty("favorites", true)?;
    drop(db);
    let _ = state.net.tx.send(net::Cmd::AutoSync("favorites".into()));
    Ok(added)
}

#[tauri::command(async)]
fn create_playlist(state: State<'_, AppState>, name: String) -> AppResult<String> {
    if name.trim().is_empty() {
        return Err(AppError::Message("playlist.nameEmpty".into()));
    }
    let id = state.db.lock().unwrap_or_else(PoisonError::into_inner).create_playlist(name.trim())?;
    Ok(id)
}

#[tauri::command(async)]
fn rename_playlist(state: State<'_, AppState>, id: String, name: String) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Message("playlist.nameEmpty".into()));
    }
    state.db.lock().unwrap_or_else(PoisonError::into_inner).rename_playlist(&id, name.trim())?;
    Ok(())
}

#[tauri::command(async)]
fn delete_playlist(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).delete_playlist(&id)?;
    Ok(())
}

#[tauri::command(async)]
fn add_to_playlist(state: State<'_, AppState>, playlist_id: String, content_id: Option<String>, local_file_id: Option<String>) -> AppResult<()> {
    state
        .db
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .add_to_playlist(&playlist_id, content_id.as_deref(), local_file_id.as_deref())?;
    let _ = state.net.tx.send(net::Cmd::Publish);
    Ok(())
}

#[tauri::command(async)]
fn remove_playlist_member(
    state: State<'_, AppState>,
    playlist_id: String,
    content_id: Option<String>,
    local_file_id: Option<String>,
) -> AppResult<()> {
    state
        .db
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove_member(&playlist_id, content_id.as_deref(), local_file_id.as_deref())?;
    Ok(())
}

#[tauri::command(async)]
fn move_playlist_member(state: State<'_, AppState>, playlist_id: String, from: usize, to: usize) -> AppResult<()> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).move_member(&playlist_id, from, to)?;
    Ok(())
}

#[tauri::command(async)]
fn set_library_order(state: State<'_, AppState>, keys: Vec<String>) -> AppResult<()> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).set_library_order(&keys)?;
    Ok(())
}

#[tauri::command(async)]
fn set_exposure(state: State<'_, AppState>, scope: String, id: String, value: String) -> AppResult<()> {
    let allowed: &[&str] = if scope == "global" { &["expose", "hide"] } else { &["expose", "hide", "follow"] };
    if !allowed.contains(&value.as_str()) {
        return Err(AppError::Message("problems.unknown".into()));
    }
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    if scope == "grant" {
        db.set_grant_exposure(grant_id(&id)?, &value)?;
    } else if scope == "playlist" {
        db.set_playlist_exposure(&id, &value)?;
    } else if scope == "global" {
        db.set_device_setting("exposeDefault", json!(value))?;
    }
    Ok(())
}

#[tauri::command(async)]
fn expose_members(state: State<'_, AppState>, scope: String, id: String, expose: bool) -> AppResult<()> {
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    if scope == "grant" {
        db.expose_grant_members(grant_id(&id)?, expose)?;
    } else if scope == "library" {
        db.expose_library_members(expose)?;
    } else {
        db.expose_playlist_members(&id, expose)?;
    }
    drop(db);
    let _ = state.net.tx.send(net::Cmd::Publish);
    Ok(())
}

/// A folder id from the page. A malformed one is an error instead of silently doing nothing.
fn grant_id(id: &str) -> AppResult<i64> {
    id.parse().map_err(|_| AppError::Message("problems.unknown".into()))
}

#[tauri::command(async)]
fn set_track_exposure(state: State<'_, AppState>, local_file_id: String, expose: bool) -> AppResult<()> {
    state.db.lock().unwrap_or_else(PoisonError::into_inner).set_track_exposure(&local_file_id, expose)?;
    let _ = state.net.tx.send(net::Cmd::Publish);
    Ok(())
}

#[tauri::command(async)]
fn update_setting(app: AppHandle, state: State<'_, AppState>, key: String, value: Value) -> AppResult<()> {
    if key == "listenPort" {
        let port = value.as_u64().unwrap_or(0);
        if !(1024..=65535).contains(&port) {
            return Err(AppError::Message("settings.portInvalid".into()));
        }
        let port = u16::try_from(port).unwrap_or_default();
        let current = state.net.live.inner.lock().map(|live| live.listen_port).unwrap_or(0);
        if port != current && std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).is_err() {
            return Err(AppError::Message("settings.portBusy".into()));
        }
    }
    if key == "relayPort" && !value.as_u64().is_some_and(|port| (1..=65535).contains(&port)) {
        return Err(AppError::Message("settings.portInvalid".into()));
    }
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    if key == "theme" || key == "language" {
        let text = value.as_str().unwrap_or("").to_string();
        db.set_synced(&key, &text)?;
        db.mark_dirty("appearance", true)?;
        drop(db);
        let _ = state.net.tx.send(net::Cmd::AutoSync("appearance".into()));
        return Ok(());
    }
    let mini_key = key == "miniPlayer" || key == "miniTray";
    let appearance = mini_key || key == "closeAction";
    db.set_device_setting(&key, value)?;
    if appearance {
        db.mark_dirty("appearance", true)?;
    }
    if key == "historyCap" {
        db.trim_history()?;
    }
    let (on_minimize, in_tray) = db.mini_flags();
    drop(db);
    if mini_key {
        desktop::sync_mini(&app, on_minimize, in_tray);
    }
    if appearance {
        let _ = state.net.tx.send(net::Cmd::AutoSync("appearance".into()));
    }
    if NETWORK_KEYS.contains(&key.as_str()) {
        let _ = state.net.tx.send(net::Cmd::Apply);
    }
    Ok(())
}

#[tauri::command(async)]
fn desktop_update(app: AppHandle, track: desktop::DesktopTrack) {
    desktop::update(&app, track);
}

#[tauri::command(async)]
fn desktop_state() -> desktop::DesktopTrack {
    desktop::current()
}

#[tauri::command(async)]
fn desktop_command(app: AppHandle, action: String) {
    desktop::command(&app, &action);
}

#[tauri::command(async)]
fn network_command(state: State<'_, AppState>, action: String, payload: Value) -> AppResult<()> {
    let text = || payload.as_str().unwrap_or("").to_string();
    let cmd = match action.as_str() {
        "pair" => net::Cmd::Pair {
            device_id: payload
                .get("deviceId")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(text),
            host: payload.get("host").and_then(|v| v.as_str()).map(str::to_string),
        },
        "confirm" => net::Cmd::Confirm {
            device_id: payload.get("deviceId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            trust: payload.get("trust").and_then(|v| v.as_str()).unwrap_or("full").to_string(),
        },
        "reject" => net::Cmd::Reject(text()),
        "block" => net::Cmd::Block(text()),
        "unblock" => net::Cmd::Unblock(text()),
        "trust" => net::Cmd::Trust {
            device_id: payload.get("deviceId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            trust: payload.get("trust").and_then(|v| v.as_str()).unwrap_or("full").to_string(),
        },
        "clear-notices" => net::Cmd::ClearNotices,
        "notice" => net::Cmd::Notice {
            action: payload.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            id: payload.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
            ids: payload
                .get("ids")
                .and_then(|v| v.as_array())
                .map(|items| items.iter().filter_map(|item| item.as_i64()).collect())
                .unwrap_or_default(),
        },
        "remove" => net::Cmd::Remove(text()),
        "disconnect" => net::Cmd::Disconnect(text()),
        "resolve-overwrite" => net::Cmd::ResolveOverwrite {
            id: payload.get("id").and_then(|v| v.as_u64()).unwrap_or(0),
            accept: payload.get("accept").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "paste" => net::Cmd::Paste(text()),
        "sync" => net::Cmd::Sync {
            feature: payload.get("feature").and_then(|v| v.as_str()).unwrap_or("favorites").to_string(),
            refresh: payload.get("refresh").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "request-sync" => net::Cmd::RequestSync {
            feature: payload.get("feature").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            device_id: payload.get("deviceId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        },
        "resolve" => net::Cmd::Resolve {
            feature: payload.get("feature").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            take: payload.get("take").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "download" => net::Cmd::Download(text()),
        "activity" => net::Cmd::Activity {
            playing: payload.get("playing").and_then(|v| v.as_bool()).unwrap_or(false),
            local_file_id: payload.get("localFileId").and_then(|v| v.as_str()).map(str::to_string),
            buffer_enough: payload.get("bufferEnough").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "connect" => net::Cmd::Connect {
            kind: payload.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            to: payload.get("to").and_then(|v| v.as_str()).map(str::to_string),
            body: payload.get("body").cloned().unwrap_or(Value::Null),
        },
        "connect-mode" => net::Cmd::ConnectMode {
            detached: payload.get("detached").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "close-route" => net::Cmd::CloseRoutes,
        _ => net::Cmd::Apply,
    };
    state.net.tx.send(cmd).map_err(|_| AppError::Message("network stopped".into()))?;
    Ok(())
}

#[tauri::command(async)]
fn remote_url(state: State<'_, AppState>, content_id: String, exclude: Option<Vec<String>>) -> AppResult<Value> {
    if state.db.lock().unwrap_or_else(PoisonError::into_inner).setting_bool("incognito")? {
        return Err(AppError::Message("notice.offlineTrack".into()));
    }
    if let Some(reason) = state.db.lock().unwrap_or_else(PoisonError::into_inner).unplayable_reason(&content_id)? {
        match sync_logic::failure_class(&reason) {
            sync_logic::FailureClass::Decode | sync_logic::FailureClass::Unsupported => {
                return Err(AppError::Message(reason));
            }
            sync_logic::FailureClass::Transport => {}
        }
    }
    let mut holders: Vec<String> = {
        let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
        let mut stmt = db.conn.prepare("SELECT DISTINCT device_id FROM holdings WHERE content_id = ?1 ORDER BY device_id")?;
        let rows = stmt.query_map(rusqlite::params![content_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let exclude = exclude.unwrap_or_default();
    holders.retain(|holder| !exclude.contains(holder));
    let online = state.net.live.inner.lock().unwrap_or_else(PoisonError::into_inner).links.clone();
    let device_id = holders
        .into_iter()
        .find(|holder| online.iter().any(|link| &link.device_id == holder && link.state == "online"))
        .ok_or_else(|| AppError::Message(if exclude.is_empty() { "notice.offlineTrack" } else { "playback.allSourcesFailed" }.into()))?;
    Ok(json!({"url": state.net.bus.open_route(&content_id, &device_id), "deviceId": device_id}))
}

#[tauri::command(async)]
fn playlist_members(state: State<'_, AppState>, id: String) -> AppResult<Vec<Value>> {
    let refs = state.db.lock().unwrap_or_else(PoisonError::into_inner).content_member_refs("playlist", Some(&id))?;
    Ok(refs.into_iter().map(|item| json!({"contentId": item.content_id, "localFileId": item.local_file_id})).collect())
}

#[tauri::command(async)]
fn move_membership(state: State<'_, AppState>, previous: String, next: String, accept: bool) -> AppResult<()> {
    if !accept {
        return Ok(());
    }
    if previous == next {
        return Ok(());
    }
    let db = state.db.lock().unwrap_or_else(PoisonError::into_inner);
    // All or nothing: favorites, playlists and order move together.
    let tx = db.conn.unchecked_transaction()?;
    let already: Option<i64> = db
        .conn
        .query_row("SELECT id FROM favorites WHERE content_id = ?1", rusqlite::params![next], |row| row.get(0))
        .ok();
    if already.is_some() {
        db.conn.execute("DELETE FROM favorites WHERE content_id = ?1", rusqlite::params![previous])?;
    } else {
        db.conn.execute(
            "UPDATE favorites SET content_id = ?1 WHERE content_id = ?2",
            rusqlite::params![next, previous],
        )?;
    }
    // A list that already has the new version keeps that entry, so the song is not listed twice.
    db.conn.execute(
        "DELETE FROM playlist_tracks WHERE content_id = ?2
         AND playlist_id IN (SELECT playlist_id FROM playlist_tracks WHERE content_id = ?1)",
        rusqlite::params![next, previous],
    )?;
    db.conn.execute(
        "UPDATE playlist_tracks SET content_id = ?1 WHERE content_id = ?2",
        rusqlite::params![next, previous],
    )?;
    db.retarget_library_order(&previous, &next)?;
    db.mark_dirty("favorites", true)?;
    tx.commit()?;
    drop(db);
    let _ = state.net.tx.send(net::Cmd::AutoSync("favorites".into()));
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    desktop::quit(&app);
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) {
    desktop::hide_to_tray(&app);
}

#[tauri::command]
fn set_tray_menu(app: AppHandle, show: String, quit: String, name: String) {
    desktop::set_tray_labels(&show, &quit);
    desktop::set_tray_tooltip(&app, &name);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(desktop)]
    let builder = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        desktop::show_main(app);
    }));
    #[cfg(not(desktop))]
    let builder = tauri::Builder::default();
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = app.path().data_dir().unwrap_or_else(|err| fatal(&format!("data folder: {err}")));
            let cache = cache_root(app.handle());
            let app_dir = config::app_dir(&data);
            let database = config::database_path(&app_dir);
            let db = Db::open(&database, app_dir.clone(), cache.clone())
                .unwrap_or_else(|err| fatal(&format!("database {}: {err}", database.display())));
            for grant in db.grants().unwrap_or_default() {
                if Path::new(&grant.path).exists() {
                    allow_grant(app.handle(), &grant.kind, &grant.path);
                }
            }
            let material = identity::load_or_create(&db).unwrap_or_else(|err| fatal(&format!("identity: {err}")));
            let db = Arc::new(Mutex::new(db));
            let net = net::spawn(app.handle().clone(), db.clone(), material.clone_for_net())
                .unwrap_or_else(|err| fatal(&format!("network: {err}")));
            let _ = net.tx.send(net::Cmd::Apply);
            let (on_minimize, in_tray) = db.lock().unwrap_or_else(PoisonError::into_inner).mini_flags();
            let worker = BackgroundWork {
                app: app.handle().clone(),
                db: db.clone(),
                live: net.live.clone(),
                tx: net.tx.clone(),
                hash_retry: Default::default(),
            };
            std::thread::spawn(move || worker.run());
            app.manage(AppState { db, net, material });
            mobile_playback::remember(app.handle());
            desktop::install(app.handle());
            desktop::sync_mini(app.handle(), on_minimize, in_tray);
            // Only the phone keeps imported music in a folder of its own; on desktop the user's
            // own folders are the library, and a stray `%APPDATA%/music` must not be swept up.
            #[cfg(target_os = "android")]
            import_mobile_music(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            import_paths,
            mobile_pick_folder,
            mobile_playback::mobile_playback_update,
            rescan,
            remove_grant,
            library,
            search,
            lyrics,
            save_player,
            save_progress,
            record_history,
            playback_result,
            remote_failure,
            toggle_favorite,
            create_playlist,
            rename_playlist,
            delete_playlist,
            add_to_playlist,
            remove_playlist_member,
            move_playlist_member,
            set_library_order,
            set_exposure,
            expose_members,
            set_track_exposure,
            update_setting,
            network_command,
            remote_url,
            playlist_members,
            move_membership,
            show_in_folder,
            reveal_path,
            desktop_update,
            desktop_state,
            desktop_command,
            quit_app,
            hide_to_tray,
            set_tray_menu
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| fatal(&format!("{err}")));
}

/// Windows keeps covers under `%LOCALAPPDATA%`; Android and iOS have no such variable, so ask Tauri.
fn cache_root(app: &AppHandle) -> std::path::PathBuf {
    #[cfg(desktop)]
    {
        let _ = app;
        config::cache_dir()
    }
    #[cfg(not(desktop))]
    {
        app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir())
    }
}

/// Android only: where the folder picker copies what the user picked. The activity copies into
/// `Context.getDataDir()/music`, which is what `data_dir()` resolves to on Android, so both sides
/// always look at the same folder.
#[cfg(target_os = "android")]
fn mobile_music_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().data_dir().ok().map(|dir| dir.join("music"))
}

/// The activity writes this once the picked tree is copied: the number of new files, `-1` when the
/// copy failed, `-2` when the user cancelled the picker. Rust deletes it before opening the picker,
/// so a stale value can never be mistaken for a fresh one.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const MOBILE_IMPORT_MARKER: &str = ".import-done";

/// Imports what already sits in the private music folder, e.g. after a restart.
#[cfg(target_os = "android")]
fn import_mobile_music(app: &AppHandle) {
    let Some(dir) = mobile_music_dir(app) else { return };
    if !dir.is_dir() {
        return;
    }
    let db = app.state::<AppState>().db.clone();
    let handle = app.clone();
    std::thread::spawn(move || {
        let _ = scan::import_path(&db, &dir, "auto");
        let _ = handle.emit("library-updated", json!({}));
    });
}

/// Android only: shows the system folder picker, which copies the chosen tree into app storage
/// because Rust cannot read `content://` URIs. Returns at once; the import happens afterwards and
/// is reported through `mobile-import-finished`, including when the user backs out.
#[tauri::command(async)]
fn mobile_pick_folder(app: AppHandle) -> AppResult<()> {
    #[cfg(target_os = "android")]
    {
        let dir = mobile_music_dir(&app).ok_or_else(|| AppError::Message("file.unreadable".into()))?;
        std::fs::create_dir_all(&dir).map_err(|err| AppError::Message(format!("music folder: {err}")))?;
        let marker = dir.join(MOBILE_IMPORT_MARKER);
        let _ = std::fs::remove_file(&marker);
        open_folder_picker().map_err(AppError::Message)?;
        let handle = app.clone();
        std::thread::spawn(move || {
            // The copy runs on the activity side, so the marker is the only completion signal.
            for _ in 0..1_200 {
                if let Ok(text) = std::fs::read_to_string(&marker) {
                    let _ = std::fs::remove_file(&marker);
                    let status = match text.trim().parse::<i64>() {
                        Ok(-2) => "cancelled",
                        Ok(-1) => "failed",
                        Ok(_) => "ok",
                        Err(_) => "failed",
                    };
                    let added = if status == "ok" {
                        handle
                            .try_state::<AppState>()
                            .and_then(|state| scan::import_path(&state.db, &dir, "auto").ok())
                            .map(|summary| summary.added)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    if status == "ok" {
                        let _ = handle.emit("library-updated", json!({}));
                    }
                    let _ = handle.emit(
                        "mobile-import-finished",
                        json!({ "added": added, "status": status }),
                    );
                    return;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            // Never leave the player waiting forever if the activity never reports back.
            let _ = handle.emit("mobile-import-finished", json!({ "added": 0, "status": "timeout" }));
        });
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(AppError::Message("file.unreadable".into()))
    }
}

/// The activity owns the picker: Android's UI thread has to present it, so Rust only asks over JNI.
#[cfg(target_os = "android")]
fn open_folder_picker() -> Result<(), String> {
    use jni::objects::JObject;
    let context = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }.map_err(|err| format!("vm: {err}"))?;
    let mut env = vm.attach_current_thread().map_err(|err| format!("thread: {err}"))?;
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    env.call_method(&activity, "openMusicFolderPicker", "()V", &[])
        .map_err(|err| format!("picker: {err}"))?;
    Ok(())
}

/// Release builds have no console and abort on panic, so a startup failure would just vanish.
/// Say what went wrong in a message box, then leave.
fn fatal(message: &str) -> ! {
    eprintln!("TunePeerly cannot start: {message}");
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let text = HSTRING::from(format!("TunePeerly cannot start.\n\n{message}"));
        unsafe {
            MessageBoxW(None, &text, &HSTRING::from("TunePeerly"), MB_OK | MB_ICONERROR);
        }
    }
    std::process::exit(1)
}

/// Access checks, covers and content hashing after the window is up. Playback keeps the disk first.
struct BackgroundWork {
    app: AppHandle,
    db: Arc<Mutex<Db>>,
    live: net::Live,
    tx: tokio::sync::mpsc::UnboundedSender<net::Cmd>,
    /// Files whose hash failed for a passing reason: when to try again, and how many tries so far.
    hash_retry: std::cell::RefCell<std::collections::HashMap<String, (Instant, u32)>>,
}

impl BackgroundWork {
    fn run(self) {
        if scan::refresh_access(&self.db).is_ok() {
            let _ = self.app.emit("library-updated", json!({}));
        }
        let mut changed = false;
        let mut hashed = false;
        let mut last_emit = Instant::now();
        loop {
            let (playing, serving, current, buffer_enough) = self
                .live
                .inner
                .lock()
                .map(|guard| (guard.playing, guard.serving, guard.current_local.clone(), guard.buffer_enough))
                .unwrap_or((false, false, None, false));
            let covered = self.cover_step(playing);
            let hash = self.hash_step(playing, serving, current.as_deref(), buffer_enough);
            let did_work = covered || hash;
            changed |= did_work;
            hashed |= hash;
            if changed && (!did_work || last_emit.elapsed() > Duration::from_secs(3)) {
                let _ = self.app.emit("library-updated", json!({}));
                let _ = self.tx.send(net::Cmd::Publish);
                if hashed {
                    let _ = self.tx.send(net::Cmd::AutoSync("favorites".into()));
                }
                last_emit = Instant::now();
                changed = false;
                hashed = false;
            }
            if !did_work {
                std::thread::sleep(Duration::from_millis(1500));
            }
        }
    }

    fn cover_step(&self, playing: bool) -> bool {
        let job = self.db.lock().ok().and_then(|guard| scan::next_cover_job(&guard).ok().flatten());
        let Some(job) = job else { return false };
        let saved = scan::extract_cover(&job);
        if let Ok(guard) = self.db.lock() {
            let _ = scan::finish_cover(&guard, &job.local_file_id, saved.as_deref());
        }
        if playing {
            std::thread::sleep(Duration::from_millis(120));
        }
        true
    }

    fn hash_step(&self, playing: bool, serving: bool, current: Option<&str>, buffer_enough: bool) -> bool {
        if playing && (!buffer_enough || serving) {
            return false;
        }
        let now = Instant::now();
        let waiting: std::collections::HashSet<String> =
            self.hash_retry.borrow().iter().filter(|(_, (at, _))| *at > now).map(|(id, _)| id.clone()).collect();
        let job = self
            .db
            .lock()
            .ok()
            .and_then(|guard| scan::next_hash_job(&guard, current, playing, &waiting).ok().flatten());
        let Some(job) = job else { return false };
        let result = scan::compute_content_id(&job.path);
        if result.is_ok() {
            self.hash_retry.borrow_mut().remove(&job.local_file_id);
        }
        match result {
            Ok(content_id) => {
                if let Ok(guard) = self.db.lock() {
                    if scan::finish_hash(&guard, &job, &content_id).unwrap_or(false) {
                        if let Ok(Some(change)) = scan::take_id_change(&guard, &job.local_file_id, &content_id) {
                            let _ = self.app.emit("content-moved", change);
                        }
                    }
                }
            }
            Err(err) => {
                // Only a file that is gone or forbidden is unreadable. A busy file, or the song
                // currently playing that refuses a second reader, is left for a later pass.
                let permanent = matches!(err.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied);
                let playing_this = playing && current == Some(job.local_file_id.as_str());
                if permanent && !playing_this {
                    self.hash_retry.borrow_mut().remove(&job.local_file_id);
                    if let Ok(guard) = self.db.lock() {
                        let _ = scan::mark_unreadable(&guard, &job.local_file_id);
                    }
                } else {
                    // Back off this file (30 s, doubling, at most 30 min) so the files after it keep moving.
                    let mut retry = self.hash_retry.borrow_mut();
                    let tries = retry.get(&job.local_file_id).map(|(_, tries)| tries + 1).unwrap_or(1);
                    let wait = Duration::from_secs(30u64.saturating_mul(1 << (tries - 1).min(6)).min(30 * 60));
                    retry.insert(job.local_file_id.clone(), (Instant::now() + wait, tries));
                    return false;
                }
            }
        }
        true
    }
}

impl IdentityMaterial {
    fn clone_for_net(&self) -> Self {
        Self {
            device_id: self.device_id.clone(),
            identity: self.identity,
            registration_id: self.registration_id,
            cert_der: self.cert_der.clone(),
            cert_key_pkcs8: self.cert_key_pkcs8.clone(),
            cert_id: self.cert_id.clone(),
            distribution_id: self.distribution_id,
        }
    }
}
