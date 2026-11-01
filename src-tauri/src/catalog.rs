// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::collections::{HashMap, HashSet};

use rusqlite::params;
use serde_json::{json, Value};

use crate::db::{Db, TrackStub};
use crate::error::AppResult;

#[derive(Debug, Clone)]
struct LocalRow {
    local_file_id: String,
    content_id: Option<String>,
    title: String,
    artist: String,
    album: String,
    track_number: Option<i64>,
    duration_ms: Option<i64>,
    folder: String,
    file_name: String,
    added_at: String,
    access_status: String,
    metadata_status: String,
    playback_status: String,
    playback_reason: Option<String>,
    access_reason: Option<String>,
    video: bool,
    cover_path: Option<String>,
    absolute_path: String,
    favorite: bool,
    unplayable: bool,
}

pub fn library(db: &Db, online: &[String]) -> AppResult<Value> {
    let local = load_local(db)?;
    let holdings = load_holdings(db)?;
    // One listing of the picture cache instead of a disk check per song.
    let cached = CoverCache::read(db);
    let remote_covers = load_remote_covers(db, &cached)?;
    let online: HashSet<&str> = online.iter().map(String::as_str).collect();
    // Grouped once, so a large library is not scanned again for every song.
    let mut holding_groups: HashMap<&str, Vec<&HoldingRow>> = HashMap::new();
    for row in &holdings {
        holding_groups.entry(row.content_id.as_str()).or_default().push(row);
    }
    let unplayable_content = db.unplayable_content_ids()?;
    let favorite_content = db.favorite_content_set()?;
    let mut tracks = Vec::new();
    let mut seen_content = HashSet::new();

    let mut by_content: HashMap<String, Vec<&LocalRow>> = HashMap::new();
    for row in &local {
        if let Some(content_id) = &row.content_id {
            by_content.entry(content_id.clone()).or_default().push(row);
        } else {
            let badge = if row.access_status == "available" { "local" } else { "unavailable" };
            tracks.push(stub_from_local(&cached, row, &[], badge));
        }
    }
    for (content_id, rows) in &by_content {
        seen_content.insert(content_id.clone());
        let holders = holders_of(&holding_groups, content_id, &online);
        let readable = rows.iter().find(|row| row.access_status == "available" && !row.video);
        // Every local copy is a video file: it is never offered as audio from a paired device either.
        let all_video = rows.iter().all(|row| row.video);
        let unplayable = rows.iter().any(|row| row.unplayable);
        let badge = if readable.is_some() && !all_video {
            "local"
        } else if !unplayable && !holders.is_empty() && !all_video {
            "remote"
        } else if readable.is_some() {
            "local"
        } else {
            "unavailable"
        };
        let source = readable.copied().unwrap_or(rows[0]);
        let mut stub = stub_from_local(&cached, source, &holders, badge);
        if stub.cover_url.is_none() {
            stub.cover_url = remote_covers.get(content_id).cloned();
        }
        // A readable local copy is always retried, even after a confirmed decode failure.
        stub.playable = (badge == "local" && !source.video && source.access_status == "available")
            || (badge == "remote" && !unplayable);
        // A missing local file can still play from a paired device, so only an unplayable row is dimmed.
        stub.dimmed = badge == "unavailable";
        if source.video {
            stub.reason_key = Some("playback.video".into());
            stub.playable = false;
        }
        tracks.push(stub);
    }
    let mut emitted_remote = HashSet::new();
    for holding in &holdings {
        if seen_content.contains(&holding.content_id) || !emitted_remote.insert(holding.content_id.clone()) {
            continue;
        }
        let group: &[&HoldingRow] = holding_groups.get(holding.content_id.as_str()).map(Vec::as_slice).unwrap_or_default();
        let holders = holders_of(&holding_groups, &holding.content_id, &online);
        let unplayable = unplayable_content.contains(&holding.content_id);
        let badge = if !unplayable && !holders.is_empty() { "remote" } else { "unavailable" };
        tracks.push(TrackStub {
            local_file_id: None,
            content_id: Some(holding.content_id.clone()),
            title: group.iter().map(|row| row.title.clone()).find(|title| !title.is_empty()).unwrap_or_default(),
            artist: group.iter().map(|row| row.artist.as_str()).find(|text| !text.is_empty()).unwrap_or("").to_string(),
            album: group.iter().map(|row| row.album.as_str()).find(|text| !text.is_empty()).unwrap_or("").to_string(),
            track_number: group.iter().find_map(|row| row.track_number),
            duration_ms: group.iter().find_map(|row| row.duration_ms),
            folder: String::new(),
            file_name: String::new(),
            added_at: String::new(),
            favorite: favorite_content.contains(&holding.content_id),
            badge: badge.into(),
            access_status: "missing".into(),
            metadata_status: "ready".into(),
            playback_status: "untried".into(),
            reason_key: if badge == "unavailable" { Some("notice.offlineTrack".into()) } else { None },
            video: false,
            cover_url: remote_covers.get(&holding.content_id).cloned(),
            absolute_path: None,
            holder_ids: holders,
            playable: badge == "remote",
            dimmed: badge == "unavailable",
        });
    }
    let positions = db.library_positions()?;
    tracks.sort_by(|a, b| match (order_position(a, &positions), order_position(b, &positions)) {
        (Some(left), Some(right)) => left.cmp(&right).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
    });
    Ok(json!(tracks))
}

fn order_position(track: &TrackStub, positions: &HashMap<String, i64>) -> Option<i64> {
    track
        .content_id
        .as_ref()
        .and_then(|id| positions.get(id).copied())
        .or_else(|| track.local_file_id.as_ref().and_then(|id| positions.get(id).copied()))
}

fn stub_from_local(cached: &CoverCache, row: &LocalRow, holders: &[String], badge: &str) -> TrackStub {
    let reason = if row.video {
        Some("playback.video".into())
    } else if row.access_status == "missing" {
        row.access_reason.clone().or(Some("file.missing".into()))
    } else if row.playback_status == "failed" {
        row.playback_reason.clone()
    } else if row.metadata_status == "failed" {
        Some("metadata.failed".into())
    } else {
        None
    };
    TrackStub {
        local_file_id: Some(row.local_file_id.clone()),
        content_id: row.content_id.clone(),
        title: row.title.clone(),
        artist: row.artist.clone(),
        album: row.album.clone(),
        track_number: row.track_number,
        duration_ms: row.duration_ms,
        folder: row.folder.clone(),
        file_name: row.file_name.clone(),
        added_at: row.added_at.clone(),
        favorite: row.favorite,
        badge: badge.into(),
        access_status: row.access_status.clone(),
        metadata_status: row.metadata_status.clone(),
        playback_status: row.playback_status.clone(),
        reason_key: reason,
        video: row.video,
        cover_url: cached.local_cover(&row.local_file_id, row.cover_path.as_deref()),
        absolute_path: Some(row.absolute_path.clone()),
        holder_ids: holders.to_vec(),
        playable: badge == "local" && !row.video && row.access_status == "available",
        dimmed: badge == "unavailable" || row.access_status != "available",
    }
}

/// File names in this install's picture cache, read once per listing.
struct CoverCache {
    dir: std::path::PathBuf,
    names: HashSet<std::ffi::OsString>,
}

impl CoverCache {
    fn read(db: &Db) -> Self {
        let dir = db.cache_dir.join("covers");
        let names = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                    .map(|entry| entry.file_name())
                    .collect()
            })
            .unwrap_or_default();
        Self { dir, names }
    }

    /// True for a file directly inside the cache folder.
    fn holds(&self, path: &std::path::Path) -> bool {
        path.parent() == Some(self.dir.as_path()) && path.file_name().is_some_and(|name| self.names.contains(name))
    }

    /// Only a file already in this install's cover cache is shown. Other stored paths are ignored.
    fn local_cover(&self, local_file_id: &str, stored: Option<&str>) -> Option<String> {
        let dest = self.dir.join(format!("{local_file_id}.jpg"));
        if self.holds(&dest) {
            return Some(dest.to_string_lossy().into_owned());
        }
        let stored = stored?;
        self.holds(std::path::Path::new(stored)).then(|| stored.to_string())
    }
}

fn holders_of(groups: &HashMap<&str, Vec<&HoldingRow>>, content_id: &str, online: &HashSet<&str>) -> Vec<String> {
    groups
        .get(content_id)
        .into_iter()
        .flatten()
        .filter(|row| online.contains(row.device_id.as_str()))
        .map(|row| row.device_id.clone())
        .collect()
}

struct HoldingRow {
    device_id: String,
    content_id: String,
    title: String,
    artist: String,
    album: String,
    track_number: Option<i64>,
    duration_ms: Option<i64>,
}

fn load_holdings(db: &Db) -> AppResult<Vec<HoldingRow>> {
    let mut stmt = db.conn.prepare(
        "SELECT device_id, content_id, COALESCE(title, ''), COALESCE(artist, ''), COALESCE(album, ''), track_number, duration_ms
         FROM holdings",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(HoldingRow {
            device_id: row.get(0)?,
            content_id: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            track_number: row.get(5)?,
            duration_ms: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_local(db: &Db) -> AppResult<Vec<LocalRow>> {
    let mut stmt = db.conn.prepare(
        "SELECT f.local_file_id, f.content_id, f.title, f.artist, f.album, f.track_number, f.duration_ms,
                f.absolute_path, f.file_name, f.added_at, f.access_status, f.metadata_status,
                f.playback_status, f.playback_reason, f.access_reason, f.video, f.cover_path,
                EXISTS(SELECT 1 FROM favorites fav WHERE fav.local_file_id = f.local_file_id OR (f.content_id IS NOT NULL AND fav.content_id = f.content_id)),
                COALESCE((SELECT c.unplayable FROM content_items c WHERE c.content_id = f.content_id), 0)
         FROM local_files f",
    )?;
    let rows = stmt.query_map([], |row| {
        let absolute: String = row.get(7)?;
        let folder = std::path::Path::new(&absolute)
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(LocalRow {
            local_file_id: row.get(0)?,
            content_id: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            track_number: row.get(5)?,
            duration_ms: row.get(6)?,
            absolute_path: absolute,
            file_name: row.get(8)?,
            added_at: row.get(9)?,
            access_status: row.get(10)?,
            metadata_status: row.get(11)?,
            playback_status: row.get(12)?,
            playback_reason: row.get(13)?,
            access_reason: row.get(14)?,
            video: row.get::<_, i64>(15)? != 0,
            cover_path: row.get(16)?,
            favorite: row.get::<_, i64>(17)? != 0,
            folder,
            unplayable: row.get::<_, i64>(18)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn local_publishable(db: &Db) -> AppResult<Vec<crate::db::HoldingInput>> {
    if db.setting_bool("incognito").unwrap_or(false) {
        return Ok(Vec::new());
    }
    let mut stmt = db.conn.prepare(
        "SELECT content_id,
                CASE WHEN metadata_status = 'failed' THEN '' ELSE title END,
                CASE WHEN metadata_status = 'failed' THEN '' ELSE artist END,
                CASE WHEN metadata_status = 'failed' THEN '' ELSE album END,
                track_number, duration_ms, codec, file_name
         FROM local_files
         WHERE access_status = 'available' AND content_id IS NOT NULL AND video = 0",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(crate::db::HoldingInput {
            content_id: row.get(0)?,
            title: blank_if_filename(row.get(1)?, row.get::<_, Option<String>>(7)?.unwrap_or_default()),
            artist: row.get(2)?,
            album: row.get(3)?,
            track_number: row.get(4)?,
            duration_ms: row.get(5)?,
            codec: row.get(6)?,
        })
    })?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        if seen.insert(row.content_id.clone()) {
            out.push(row);
        }
    }
    Ok(out)
}

fn blank_if_filename(title: String, file_name: String) -> String {
    if title == file_name { String::new() } else { title }
}

pub fn readable_paths(db: &Db, content_id: &str) -> AppResult<Vec<(String, String)>> {
    let mut stmt = db.conn.prepare(
        "SELECT local_file_id, absolute_path FROM local_files
         WHERE content_id = ?1 AND access_status = 'available'",
    )?;
    let rows = stmt.query_map(params![content_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// FTS covers local title, artist, lyrics and folder. Remote rows and substrings inside CJK words match on title, artist and album.
pub fn search(db: &Db, query: &str, online: &[String]) -> AppResult<Value> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return Ok(json!([]));
    }
    let hits = db.search_hits(query, 400)?;
    let show_unavailable = db.setting_bool("showUnavailable").unwrap_or(true);
    let all = library(db, online)?;
    let text = |track: &Value, key: &str| track.get(key).and_then(|value| value.as_str()).unwrap_or("").to_lowercase();
    let mut out = Vec::new();
    let all = match all {
        Value::Array(items) => items,
        _ => Vec::new(),
    };
    for mut track in all {
        if !show_unavailable && track.get("badge").and_then(|value| value.as_str()) == Some("unavailable") {
            continue;
        }
        let hit = track
            .get("localFileId")
            .and_then(|value| value.as_str())
            .and_then(|id| hits.get(id))
            .cloned();
        let haystack = format!(
            "{} {} {} {}",
            text(&track, "title"),
            text(&track, "artist"),
            text(&track, "album"),
            text(&track, "fileName")
        );
        let plain = tokens.iter().all(|token| haystack.contains(token.as_str()));
        if hit.is_none() && !plain {
            continue;
        }
        if let Some(Some(snippet)) = hit.filter(|_| !plain) {
            track["snippet"] = json!(snippet);
        }
        out.push(track);
    }
    Ok(json!(out))
}

/// Cover pictures of readable local copies of this content.
pub fn cover_paths(db: &Db, content_id: &str) -> AppResult<Vec<(String, String)>> {
    let mut stmt = db.conn.prepare(
        "SELECT local_file_id, cover_path FROM local_files
         WHERE content_id = ?1 AND access_status = 'available' AND cover_path IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![content_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// True while a local copy has not been checked for an embedded picture yet.
pub fn cover_pending(db: &Db, content_id: &str) -> bool {
    db.conn
        .query_row(
            "SELECT COUNT(*) FROM local_files WHERE content_id = ?1 AND cover_checked = 0",
            params![content_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false)
}

/// Content ids held by this device that have no picture here yet and were not tried recently.
pub fn missing_covers(db: &Db, device_id: &str, retry_before: i64) -> AppResult<Vec<String>> {
    let mut stmt = db.conn.prepare(
        "SELECT h.content_id FROM holdings h
         WHERE h.device_id = ?1
           AND NOT EXISTS (SELECT 1 FROM remote_covers c WHERE c.content_id = h.content_id AND (c.path IS NOT NULL OR c.tried_at > ?2))
           AND NOT EXISTS (SELECT 1 FROM local_files f WHERE f.content_id = h.content_id AND f.cover_path IS NOT NULL)
         LIMIT 2000",
    )?;
    let rows = stmt.query_map(params![device_id, retry_before], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<String>, _>>()?)
}

pub fn save_remote_cover(db: &Db, content_id: &str, path: Option<&str>) -> AppResult<()> {
    db.conn.execute(
        "INSERT INTO remote_covers(content_id, path, tried_at) VALUES(?1, ?2, ?3)
         ON CONFLICT(content_id) DO UPDATE SET path = excluded.path, tried_at = excluded.tried_at",
        params![content_id, path, chrono::Utc::now().timestamp()],
    )?;
    Ok(())
}

fn load_remote_covers(db: &Db, cached: &CoverCache) -> AppResult<HashMap<String, String>> {
    let mut stmt = db.conn.prepare("SELECT content_id, path FROM remote_covers WHERE path IS NOT NULL")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, path) = row?;
        if cached.holds(std::path::Path::new(&path)) {
            map.insert(id, path);
        }
    }
    Ok(map)
}
