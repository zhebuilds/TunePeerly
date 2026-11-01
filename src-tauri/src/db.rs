// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::dbkey::{self, KEY_LEN};
use crate::error::{AppError, AppResult};
use crate::sync_logic::{HistoryEntry, MemberRef};

/// What Send and Override copy between devices. Recently Played has its own Refresh.
pub const FEATURES: [&str; 2] = [
    "favorites",
    "appearance",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackStub {
    pub local_file_id: Option<String>,
    pub content_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<i64>,
    pub duration_ms: Option<i64>,
    pub folder: String,
    pub file_name: String,
    pub added_at: String,
    pub favorite: bool,
    pub exposed: bool,
    pub badge: String,
    pub access_status: String,
    pub metadata_status: String,
    pub playback_status: String,
    pub reason_key: Option<String>,
    pub video: bool,
    pub cover_url: Option<String>,
    pub absolute_path: Option<String>,
    pub holder_ids: Vec<String>,
    pub playable: bool,
    pub dimmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantRow {
    pub id: i64,
    pub kind: String,
    pub path: String,
    pub exposure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistRow {
    pub id: String,
    pub name: String,
    pub exposure: String,
    pub count: i64,
}

pub struct Db {
    pub conn: Connection,
    pub app_dir: PathBuf,
    pub cache_dir: PathBuf,
}

/// The library database, encrypted with the key in `db.key` beside it.
/// A database that key cannot open (the key file lost, or sealed under another Windows account) is
/// renamed aside with its key file and a new library starts, so the app still opens and the old
/// file stays recoverable by whoever has the right key.
fn open_encrypted(database: &Path) -> AppResult<Connection> {
    let key_file = database.with_file_name(dbkey::KEY_NAME);
    let key = match dbkey::load_or_create(&key_file) {
        Ok(key) => key,
        Err(err) if database.exists() => {
            set_aside(database, &key_file, &err.to_string())?;
            dbkey::load_or_create(&key_file)?
        }
        Err(err) => return Err(err),
    };
    match unlock(database, &key) {
        Err(AppError::Sql(err)) if err.sqlite_error_code() == Some(rusqlite::ErrorCode::NotADatabase) => {
            set_aside(database, &key_file, &err.to_string())?;
            unlock(database, &dbkey::load_or_create(&key_file)?)
        }
        other => other,
    }
}

fn unlock(database: &Path, key: &[u8; KEY_LEN]) -> AppResult<Connection> {
    // A raw 256-bit key in SQLCipher's `x'…'` form skips its passphrase KDF; the key is random already.
    let key = format!("x'{}'", data_encoding::HEXUPPER.encode(key));
    let mut header = [0u8; 16];
    let plain = std::fs::File::open(database).and_then(|mut file| std::io::Read::read_exact(&mut file, &mut header)).is_ok()
        && &header == b"SQLite format 3\0";
    if plain {
        encrypt_plain(database, &key)?;
    }
    let conn = Connection::open(database)?;
    conn.pragma_update(None, "key", &key)?;
    // A wrong key only shows on the first read.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
    Ok(conn)
}

/// A library from before encryption. SQLCipher cannot encrypt a file in place, so the data is
/// exported into an encrypted copy, which then replaces the plain file.
fn encrypt_plain(database: &Path, key: &str) -> AppResult<()> {
    let copy = beside(database, ".encrypting");
    if copy.exists() {
        std::fs::remove_file(&copy)?;
    }
    {
        let plain = Connection::open(database)?;
        // Folds any WAL into the main file first, so the export sees every committed row.
        plain.pragma_update(None, "journal_mode", "DELETE")?;
        let version: i64 = plain.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        plain.execute("ATTACH DATABASE ?1 AS encrypted KEY ?2", rusqlite::params![copy.to_string_lossy(), key])?;
        plain.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
        plain.execute_batch(&format!("PRAGMA encrypted.user_version = {version}; DETACH DATABASE encrypted;"))?;
    }
    for stale in [beside(database, "-wal"), beside(database, "-shm")] {
        if stale.exists() {
            std::fs::remove_file(stale)?;
        }
    }
    std::fs::rename(&copy, database)?;
    Ok(())
}

fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn set_aside(database: &Path, key_file: &Path, reason: &str) -> AppResult<()> {
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    eprintln!("database {} cannot be opened ({reason}); starting a new library and keeping the old one beside it", database.display());
    for path in [database.to_path_buf(), beside(database, "-wal"), beside(database, "-shm"), key_file.to_path_buf()] {
        if path.exists() {
            std::fs::rename(&path, beside(&path, &format!(".unreadable-{stamp}")))?;
        }
    }
    Ok(())
}

impl Db {
    pub fn open(database: &Path, app_dir: PathBuf, cache_dir: PathBuf) -> AppResult<Self> {
        if let Some(parent) = database.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir_all(cache_dir.join("covers"))?;
        let conn = open_encrypted(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self {
            conn,
            app_dir,
            cache_dir,
        };
        db.migrate()?;
        db.seed()?;
        db.queue_missing_covers()?;
        Ok(db)
    }

    fn migrate(&self) -> AppResult<()> {
        // Pairings made under the retired Signal identity pinned keys this device no longer has.
        // Drop them with that identity, so the device gets a new TLS key and pairs again.
        let signal_era: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'signal_state')",
            [],
            |row| row.get(0),
        )?;
        if signal_era {
            self.conn.execute_batch("DROP TABLE signal_state; DROP TABLE IF EXISTS peers;")?;
        }
        self.conn.execute_batch(SCHEMA)?;
        self.add_column_if_missing("local_files", "cover_checked", "INTEGER NOT NULL DEFAULT 0")?;
        // Time and size of the `.lrc` next to the song, so a lyrics file added later is noticed.
        self.add_column_if_missing("local_files", "lyrics_stamp", "TEXT")?;
        self.add_column_if_missing("notices", "read", "INTEGER NOT NULL DEFAULT 1")?;
        self.add_column_if_missing("notices", "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        // Left over from removals that were shared between devices; a removal is local now.
        self.drop_column_if_present("peers", "paired_at")?;
        self.drop_column_if_present("removed_devices", "removed_by")?;
        // Left over from the retired safety-number check; pairing checks a code now.
        self.drop_column_if_present("peers", "safety_number")?;
        // Tables and switches left behind when sync revisions, the mailbox, and the old on/off toggles were removed.
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS mailbox_acks;
             DROP TABLE IF EXISTS mailbox;
             DROP TABLE IF EXISTS applied_ops;
             DROP TABLE IF EXISTS snapshots;
             DROP TABLE IF EXISTS feature_revisions;
             DELETE FROM device_settings WHERE key IN ('historySyncMode', 'syncEnabled', 'audioShare', 'positionSync', 'positionSyncDevice');",
        )?;
        // Lookups by song and by file. Without these, large libraries scan whole tables on every listing.
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS local_files_content ON local_files(content_id);
             CREATE INDEX IF NOT EXISTS holdings_content ON holdings(content_id);
             CREATE INDEX IF NOT EXISTS play_history_content ON play_history(content_id);
             CREATE INDEX IF NOT EXISTS play_history_file ON play_history(local_file_id);
             CREATE INDEX IF NOT EXISTS playlist_tracks_content ON playlist_tracks(content_id);
             CREATE INDEX IF NOT EXISTS playlist_tracks_file ON playlist_tracks(local_file_id);",
        )?;
        Ok(())
    }

    fn add_column_if_missing(&self, table: &str, column: &str, definition: &str) -> AppResult<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !names.iter().any(|name| name == column) {
            self.conn
                .execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"))?;
        }
        Ok(())
    }

    fn drop_column_if_present(&self, table: &str, column: &str) -> AppResult<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if names.iter().any(|name| name == column) {
            self.conn
                .execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))?;
        }
        Ok(())
    }

    fn seed(&self) -> AppResult<()> {
        for (key, value) in device_defaults() {
            self.conn.execute(
                "INSERT INTO device_settings(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO NOTHING",
                params![key, value.to_string()],
            )?;
        }
        for (key, value) in [("theme", "dark"), ("language", "en")] {
            self.conn.execute(
                "INSERT INTO synced_settings(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO NOTHING",
                params![key, value],
            )?;
        }
        self.drop_auto_sync()?;
        self.drop_listen_port()?;
        self.split_overwrite_mode()?;
        let name = self.setting_string("displayName")?;
        if name.is_empty() {
            let host = whoami::fallible::hostname().unwrap_or_else(|_| "This computer".into());
            self.set_device_setting("displayName", json!(host))?;
        }
        let applied = self
            .conn
            .query_row(
                "SELECT value FROM device_settings WHERE key = 'darkDefaultApplied'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_default();
        if applied != "true" {
            if self.synced("theme")?.is_empty() || self.synced("theme")? == "light" {
                self.set_synced("theme", "dark")?;
            }
            self.set_device_setting("darkDefaultApplied", json!(true))?;
        }
        Ok(())
    }

    /// Favorites and appearance sync only by hand now, and another device's Recently Played is only
    /// viewed. Drop the settings that automatic sync left behind.
    /// The LAN port is picked by the system on every start now. A port saved by an older version is
    /// not used anymore.
    fn drop_listen_port(&self) -> AppResult<()> {
        self.conn.execute("DELETE FROM device_settings WHERE key = 'listenPort'", [])?;
        Ok(())
    }

    fn drop_auto_sync(&self) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM device_settings WHERE key IN ('dirty', 'pendingSnapshots', 'syncSources', 'syncBlocks', 'autoSend', 'conflictPolicy', 'historySync', 'historyStartup')",
            [],
        )?;
        Ok(())
    }

    /// The single "overwriteMode" (silent or ask) became one choice per feature. Carry the old choice over.
    fn split_overwrite_mode(&self) -> AppResult<()> {
        let old: Option<String> = self
            .conn
            .query_row("SELECT value FROM device_settings WHERE key = 'overwriteMode'", [], |row| row.get(0))
            .optional()?;
        let Some(old) = old else { return Ok(()) };
        let mode = if serde_json::from_str::<Value>(&old).ok().and_then(|value| value.as_str().map(str::to_string)).as_deref() == Some("ask") { "ask" } else { "silent" };
        let policy: serde_json::Map<String, Value> = FEATURES.iter().map(|feature| (feature.to_string(), json!(mode))).collect();
        self.set_device_setting("overwritePolicy", Value::Object(policy))?;
        self.conn.execute("DELETE FROM device_settings WHERE key = 'overwriteMode'", [])?;
        Ok(())
    }

    /// Whether the corner bar shows when the window is minimized, and when it is hidden in the tray.
    pub fn mini_flags(&self) -> (bool, bool) {
        (
            self.setting_bool("miniPlayer").unwrap_or(false),
            self.setting_bool("miniTray").unwrap_or(true),
        )
    }

    pub fn setting_raw(&self, key: &str) -> AppResult<String> {
        self.conn
            .query_row(
                "SELECT value FROM device_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::Message(format!("Missing setting {key}")))
    }

    pub fn setting_string(&self, key: &str) -> AppResult<String> {
        let raw = self.setting_raw(key)?;
        Ok(serde_json::from_str::<String>(&raw).unwrap_or(raw))
    }

    pub fn setting_bool(&self, key: &str) -> AppResult<bool> {
        Ok(serde_json::from_str(&self.setting_raw(key)?).unwrap_or(false))
    }

    pub fn setting_i64(&self, key: &str) -> AppResult<i64> {
        Ok(serde_json::from_str(&self.setting_raw(key)?).unwrap_or(0))
    }

    pub fn setting_json(&self, key: &str) -> AppResult<Value> {
        Ok(serde_json::from_str(&self.setting_raw(key)?).unwrap_or(Value::Null))
    }

    pub fn set_device_setting(&self, key: &str, value: Value) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO device_settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value.to_string()],
        )?;
        Ok(())
    }

    pub fn synced(&self, key: &str) -> AppResult<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM synced_settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn set_synced(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO synced_settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn device_settings_map(&self) -> AppResult<Value> {
        let mut out = serde_json::Map::new();
        let mut stmt = self.conn.prepare("SELECT key, value FROM device_settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            out.insert(key, serde_json::from_str(&value).unwrap_or(Value::String(value)));
        }
        Ok(Value::Object(out))
    }

    pub fn identity_blob(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM device_identity WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_identity_blob(&self, key: &str, value: &[u8]) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO device_identity(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn grants(&self) -> AppResult<Vec<GrantRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, path, exposure FROM grants ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok(GrantRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                path: row.get(2)?,
                exposure: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn insert_grant(&self, kind: &str, path: &str) -> AppResult<i64> {
        // The same folder written another way (trailing slash, other letter case on Windows) is the same grant.
        let path = grant_path(path);
        let path = path.as_str();
        let same: Option<i64> = {
            let mut stmt = self.conn.prepare("SELECT id, path FROM grants")?;
            let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?;
            let mut found = None;
            for row in rows {
                let (id, stored) = row?;
                if same_grant_path(&stored, path) {
                    found = Some(id);
                    break;
                }
            }
            found
        };
        if let Some(id) = same {
            self.conn.execute("UPDATE grants SET kind = ?1 WHERE id = ?2", params![kind, id])?;
            return Ok(id);
        }
        self.conn.execute(
            "INSERT INTO grants(kind, path, exposure) VALUES(?1, ?2, 'follow')
             ON CONFLICT(path) DO UPDATE SET kind = excluded.kind",
            params![kind, path],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM grants WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// With `keep_tracks`, the songs stay in the library with their tags, lyrics, covers, favorites,
    /// playlist entries and history, marked unavailable. They rejoin a folder that is added again.
    /// Without it, every song of the grant is dropped.
    pub fn delete_grant(&self, id: i64, keep_tracks: bool) -> AppResult<Option<GrantRow>> {
        let grant = self
            .conn
            .query_row(
                "SELECT id, kind, path, exposure FROM grants WHERE id = ?1",
                params![id],
                |row| {
                    Ok(GrantRow {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        path: row.get(2)?,
                        exposure: row.get(3)?,
                    })
                },
            )
            .optional()?;
        if grant.is_some() {
            // All or nothing, so a failure never leaves half of the folder's songs behind.
            let tx = self.conn.unchecked_transaction()?;
            // A song that another folder also covers is already in the library through that folder.
            let ids: Vec<String> = {
                let mut stmt = self.conn.prepare(if keep_tracks {
                    "SELECT a.local_file_id FROM local_files a
                     WHERE a.grant_id = ?1 AND EXISTS (
                        SELECT 1 FROM local_files b
                        WHERE b.grant_id IS NOT NULL AND b.grant_id != ?1 AND b.absolute_path = a.absolute_path)"
                } else {
                    "SELECT local_file_id FROM local_files WHERE grant_id = ?1"
                })?;
                let rows = stmt.query_map(params![id], |row| row.get(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for file_id in ids {
                self.delete_local_file_record(&file_id)?;
            }
            if keep_tracks {
                self.conn.execute(
                    "UPDATE local_files
                     SET grant_id = NULL, access_status = 'missing', access_reason = 'file.folderRemoved'
                     WHERE grant_id = ?1",
                    params![id],
                )?;
            }
            self.conn
                .execute("DELETE FROM grants WHERE id = ?1", params![id])?;
            tx.commit()?;
        }
        Ok(grant)
    }

    pub(crate) fn delete_local_file_record(&self, local_file_id: &str) -> AppResult<()> {
        // A hashed row is identified by its content, so it outlives the file. Only unhashed rows go with the file.
        self.conn.execute(
            "DELETE FROM favorites WHERE local_file_id = ?1 AND content_id IS NULL",
            params![local_file_id],
        )?;
        self.conn.execute(
            "UPDATE favorites SET local_file_id = NULL WHERE local_file_id = ?1",
            params![local_file_id],
        )?;
        self.conn.execute(
            "DELETE FROM playlist_tracks WHERE local_file_id = ?1 AND content_id IS NULL",
            params![local_file_id],
        )?;
        self.conn.execute(
            "UPDATE playlist_tracks SET local_file_id = NULL WHERE local_file_id = ?1",
            params![local_file_id],
        )?;
        self.conn.execute(
            "DELETE FROM library_fts WHERE local_file_id = ?1",
            params![local_file_id],
        )?;
        self.conn.execute(
            "DELETE FROM local_files WHERE local_file_id = ?1",
            params![local_file_id],
        )?;
        Ok(())
    }

    pub fn exposure_for_new_folder_file(&self, grant_id: i64) -> AppResult<bool> {
        let exposure: String = self.conn.query_row(
            "SELECT exposure FROM grants WHERE id = ?1",
            params![grant_id],
            |row| row.get(0),
        )?;
        self.resolve_exposure(&exposure)
    }

    pub fn resolve_exposure(&self, exposure: &str) -> AppResult<bool> {
        let mode = if exposure == "follow" {
            self.setting_string("exposeDefault")?
        } else {
            exposure.to_string()
        };
        Ok(mode == "expose")
    }

    pub fn set_grant_exposure(&self, id: i64, exposure: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE grants SET exposure = ?1 WHERE id = ?2",
            params![exposure, id],
        )?;
        Ok(())
    }

    pub fn expose_grant_members(&self, id: i64, expose: bool) -> AppResult<()> {
        self.conn.execute(
            "UPDATE local_files SET exposed = ?1 WHERE grant_id = ?2",
            params![expose, id],
        )?;
        Ok(())
    }

    pub fn expose_library_members(&self, expose: bool) -> AppResult<()> {
        self.conn.execute("UPDATE local_files SET exposed = ?1", params![expose])?;
        Ok(())
    }

    pub fn playlists(&self) -> AppResult<Vec<PlaylistRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.exposure,
                    (SELECT COUNT(*) FROM playlist_tracks t WHERE t.playlist_id = p.id)
             FROM playlists p ORDER BY p.position, p.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PlaylistRow {
                id: row.get(0)?,
                name: row.get(1)?,
                exposure: row.get(2)?,
                count: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create_playlist(&self, name: &str) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();
        let position: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM playlists", [], |row| {
                row.get(0)
            })?;
        self.conn.execute(
            "INSERT INTO playlists(id, name, position, exposure, revision) VALUES(?1, ?2, ?3, 'follow', 0)",
            params![id, name, position],
        )?;
        Ok(id)
    }

    pub fn rename_playlist(&self, id: &str, name: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: &str) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![id])?;
        tx.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn add_to_playlist(
        &self,
        playlist_id: &str,
        content_id: Option<&str>,
        local_file_id: Option<&str>,
    ) -> AppResult<()> {
        let already: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM playlist_tracks
             WHERE playlist_id = ?1 AND ((?2 IS NOT NULL AND content_id = ?2) OR (?3 IS NOT NULL AND local_file_id = ?3))",
            params![playlist_id, content_id, local_file_id],
            |row| row.get(0),
        )?;
        if already > 0 {
            return Err(crate::error::AppError::Message("playlist.already".into()));
        }
        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO playlist_tracks(playlist_id, position, content_id, local_file_id)
             VALUES(?1, ?2, ?3, ?4)",
            params![playlist_id, position, content_id, local_file_id],
        )?;
        if let Some(local_file_id) = local_file_id {
            let exposure: String = self.conn.query_row(
                "SELECT exposure FROM playlists WHERE id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )?;
            // A list that only follows the default leaves the song as its folder or the user set it.
            if exposure == "follow" {
                return Ok(());
            }
            let exposed = self.resolve_exposure(&exposure)?;
            self.conn.execute(
                "UPDATE local_files SET exposed = ?1 WHERE local_file_id = ?2",
                params![exposed, local_file_id],
            )?;
        }
        Ok(())
    }

    /// Moves one member to `to` (0-based) and renumbers the list.
    pub fn move_member(&self, playlist_id: &str, from: usize, to: usize) -> AppResult<()> {
        let rows: Vec<(Option<String>, Option<String>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT content_id, local_file_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )?;
            let rows = stmt.query_map(params![playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if from >= rows.len() || to >= rows.len() || from == to {
            return Ok(());
        }
        let mut rows = rows;
        let item = rows.remove(from);
        rows.insert(to, item);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        for (index, (content_id, local_file_id)) in rows.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks(playlist_id, position, content_id, local_file_id) VALUES(?1, ?2, ?3, ?4)",
                params![playlist_id, index as i64 + 1, content_id, local_file_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn library_positions(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare("SELECT track_key, position FROM library_order")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    }

    /// Saves the All music order. Keys missing from `keys` drop out; new files sort after these.
    pub fn set_library_order(&self, keys: &[String]) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM library_order", [])?;
        let mut seen = HashSet::new();
        let mut position = 0i64;
        for key in keys {
            if key.is_empty() || !seen.insert(key.clone()) {
                continue;
            }
            position += 1;
            tx.execute(
                "INSERT INTO library_order(track_key, position) VALUES(?1, ?2)",
                params![key, position],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn retarget_library_order(&self, previous: &str, next: &str) -> AppResult<()> {
        let exists: Option<i64> = self
            .conn
            .query_row("SELECT position FROM library_order WHERE track_key = ?1", params![next], |row| row.get(0))
            .optional()?;
        if exists.is_some() {
            self.conn.execute("DELETE FROM library_order WHERE track_key = ?1", params![previous])?;
        } else {
            self.conn.execute(
                "UPDATE library_order SET track_key = ?1 WHERE track_key = ?2",
                params![next, previous],
            )?;
        }
        Ok(())
    }

    pub fn remove_member(
        &self,
        playlist_id: &str,
        content_id: Option<&str>,
        local_file_id: Option<&str>,
    ) -> AppResult<()> {
        if let Some(content_id) = content_id {
            self.conn.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND content_id = ?2",
                params![playlist_id, content_id],
            )?;
        }
        if let Some(local_file_id) = local_file_id {
            self.conn.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND local_file_id = ?2",
                params![playlist_id, local_file_id],
            )?;
        }
        Ok(())
    }

    pub fn set_playlist_exposure(&self, id: &str, exposure: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE playlists SET exposure = ?1 WHERE id = ?2",
            params![exposure, id],
        )?;
        Ok(())
    }

    pub fn expose_playlist_members(&self, id: &str, expose: bool) -> AppResult<()> {
        let ids = self.playlist_local_ids(id)?;
        for local_file_id in ids {
            self.conn.execute(
                "UPDATE local_files SET exposed = ?1 WHERE local_file_id = ?2",
                params![expose, local_file_id],
            )?;
        }
        Ok(())
    }

    fn playlist_local_ids(&self, id: &str) -> AppResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT local_file_id FROM playlist_tracks
             WHERE playlist_id = ?1 AND local_file_id IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![id], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_track_exposure(&self, local_file_id: &str, expose: bool) -> AppResult<()> {
        self.conn.execute(
            "UPDATE local_files SET exposed = ?1 WHERE local_file_id = ?2",
            params![expose, local_file_id],
        )?;
        Ok(())
    }

    pub fn toggle_favorite(
        &self,
        content_id: Option<&str>,
        local_file_id: Option<&str>,
    ) -> AppResult<bool> {
        let existing: Option<i64> = if let Some(content_id) = content_id {
            self.conn
                .query_row(
                    "SELECT id FROM favorites WHERE content_id = ?1",
                    params![content_id],
                    |row| row.get(0),
                )
                .optional()?
        } else if let Some(local_file_id) = local_file_id {
            self.conn
                .query_row(
                    "SELECT id FROM favorites WHERE local_file_id = ?1",
                    params![local_file_id],
                    |row| row.get(0),
                )
                .optional()?
        } else {
            None
        };
        if let Some(id) = existing {
            self.conn
                .execute("DELETE FROM favorites WHERE id = ?1", params![id])?;
            Ok(false)
        } else {
            self.conn.execute(
                "INSERT INTO favorites(content_id, local_file_id) VALUES(?1, ?2)",
                params![content_id, local_file_id],
            )?;
            Ok(true)
        }
    }

    /// Replaces liked songs. Unhashed local rows in `members` are kept. A failed write leaves the previous list.
    pub fn replace_favorites(&self, members: &[MemberRef]) -> AppResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let write = (|| {
            self.conn.execute("DELETE FROM favorites", [])?;
            for member in members {
                self.conn.execute(
                    "INSERT INTO favorites(content_id, local_file_id) VALUES(?1, ?2)",
                    params![member.content_id, member.local_file_id],
                )?;
            }
            Ok(())
        })();
        match write {
            Ok(()) => {
                if let Err(error) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(error.into());
                }
                Ok(())
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// A cover is `{cache}/covers/{local file id}.jpg`. Anything else is extracted again into that file.
    fn queue_missing_covers(&self) -> AppResult<()> {
        let covers = self.cache_dir.join("covers");
        std::fs::create_dir_all(&covers)?;
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT local_file_id FROM local_files WHERE video = 0 AND access_status = 'available'",
            )?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for id in ids {
            let dest = covers.join(format!("{id}.jpg"));
            if dest.is_file() {
                self.conn.execute(
                    "UPDATE local_files SET cover_path = ?1, cover_checked = 1 WHERE local_file_id = ?2 AND cover_path IS NOT ?1",
                    params![dest.to_string_lossy(), id],
                )?;
            } else {
                self.conn.execute(
                    "UPDATE local_files SET cover_path = NULL, cover_checked = 0 WHERE local_file_id = ?1 AND cover_checked != 0",
                    params![id],
                )?;
            }
        }
        Ok(())
    }

    pub fn push_download_history(&self, content_id: &str, title: &str, path: &str, ok: bool) -> AppResult<Value> {
        let mut list = self.setting_json("downloadHistory").unwrap_or(json!([]));
        if !list.is_array() {
            list = json!([]);
        }
        let Some(items) = list.as_array_mut() else { return Ok(json!([])) };
        items.insert(
            0,
            json!({
                "contentId": content_id,
                "title": title,
                "path": path,
                "ok": ok,
                "at": Utc::now().to_rfc3339(),
            }),
        );
        items.truncate(40);
        let saved = list.clone();
        self.set_device_setting("downloadHistory", list)?;
        Ok(saved)
    }

    pub fn favorite_content_ids(&self) -> AppResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT COALESCE(f.content_id, lf.content_id)
             FROM favorites f
             LEFT JOIN local_files lf ON lf.local_file_id = f.local_file_id
             WHERE COALESCE(f.content_id, lf.content_id) IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn our_holdings_revision(&self) -> AppResult<i64> {
        self.setting_i64("holdingsRevision")
    }

    pub fn set_holdings_revision(&self, revision: i64) -> AppResult<()> {
        self.set_device_setting("holdingsRevision", json!(revision))
    }

    pub fn replace_peer_holdings(
        &self,
        device_id: &str,
        revision: i64,
        rows: &[HoldingInput],
    ) -> AppResult<bool> {
        let applied: i64 = self
            .conn
            .query_row(
                "SELECT revision FROM holding_revisions WHERE device_id = ?1",
                params![device_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if revision <= applied {
            return Ok(false);
        }
        // One transaction, so a failure never leaves this device's catalog empty without a new revision.
        // Dropping it without a commit, including after a failed commit, rolls it back.
        let tx = self.conn.unchecked_transaction()?;
        self.write_peer_holdings(device_id, revision, rows)?;
        tx.commit()?;
        Ok(true)
    }

    fn write_peer_holdings(&self, device_id: &str, revision: i64, rows: &[HoldingInput]) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM holdings WHERE device_id = ?1",
            params![device_id],
        )?;
        for row in rows {
            self.conn.execute(
                "INSERT OR REPLACE INTO holdings(device_id, revision, content_id, title, artist, album, track_number, duration_ms, codec)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    device_id,
                    revision,
                    row.content_id,
                    row.title,
                    row.artist,
                    row.album,
                    row.track_number,
                    row.duration_ms,
                    row.codec
                ],
            )?;
        }
        self.conn.execute(
            "INSERT INTO holding_revisions(device_id, revision) VALUES(?1, ?2)
             ON CONFLICT(device_id) DO UPDATE SET revision = excluded.revision",
            params![device_id, revision],
        )?;
        Ok(())
    }

    pub fn delete_peer_holdings(&self, device_id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM holdings WHERE device_id = ?1", params![device_id])?;
        self.conn.execute(
            "DELETE FROM holding_revisions WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(())
    }

    pub fn record_history_local(&self, content_id: Option<&str>, local_file_id: Option<&str>) -> AppResult<()> {
        if self.setting_bool("incognito")? {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        if let Some(content_id) = content_id {
            self.conn.execute(
                "DELETE FROM play_history WHERE content_id = ?1",
                params![content_id],
            )?;
            self.conn.execute(
                "INSERT INTO play_history(content_id, local_file_id, played_at) VALUES(?1, NULL, ?2)",
                params![content_id, now],
            )?;
        } else if let Some(local_file_id) = local_file_id {
            self.conn.execute(
                "DELETE FROM play_history WHERE local_file_id = ?1 AND content_id IS NULL",
                params![local_file_id],
            )?;
            self.conn.execute(
                "INSERT INTO play_history(content_id, local_file_id, played_at) VALUES(NULL, ?1, ?2)",
                params![local_file_id, now],
            )?;
        }
        self.trim_history()?;
        Ok(())
    }

    pub fn trim_history(&self) -> AppResult<()> {
        let cap = self.setting_i64("historyCap")?.max(1);
        self.conn.execute(
            "DELETE FROM play_history WHERE id NOT IN (
                SELECT id FROM play_history ORDER BY played_at DESC LIMIT ?1
             )",
            params![cap],
        )?;
        Ok(())
    }

    pub fn history_for_sync(&self) -> AppResult<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(h.content_id, lf.content_id), MAX(h.played_at)
             FROM play_history h
             LEFT JOIN local_files lf ON lf.local_file_id = h.local_file_id
             WHERE COALESCE(h.content_id, lf.content_id) IS NOT NULL
             GROUP BY COALESCE(h.content_id, lf.content_id)
             ORDER BY MAX(h.played_at) DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (content_id, played_at) = row?;
            if let Ok(at) = DateTime::parse_from_rfc3339(&played_at) {
                out.push(HistoryEntry {
                    content_id,
                    at: at.with_timezone(&Utc),
                });
            }
        }
        Ok(out)
    }

    pub fn player_json(&self) -> AppResult<Value> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM player_state WHERE key = 'state'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(text
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(|value| match value.get("queue") {
                Some(queue) if queue.is_object() => {
                    let mut flat = queue.clone();
                    if let Some(position) = value.get("positionSec") {
                        flat["positionSec"] = position.clone();
                    }
                    flat
                }
                _ => value,
            })
            .unwrap_or_else(|| json!({
                "order": "manual",
                "repeat": "off",
                "past": [],
                "current": null,
                "future": [],
                "positionSec": 0,
                "contextId": "library"
            })))
    }

    pub fn save_player_json(&self, value: &Value) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO player_state(key, value) VALUES('state', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![value.to_string()],
        )?;
        Ok(())
    }

    /// One statement, so a queue save that lands in between cannot be overwritten by an older copy.
    pub fn save_progress(&self, position_sec: f64) -> AppResult<()> {
        let changed = self.conn.execute(
            "UPDATE player_state SET value = json_set(value, '$.positionSec', ?1) WHERE key = 'state' AND json_valid(value)",
            params![position_sec],
        )?;
        if changed == 0 {
            let mut state = self.player_json()?;
            state["positionSec"] = json!(position_sec);
            self.save_player_json(&state)?;
        }
        Ok(())
    }

    pub fn history_keys(&self) -> AppResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(content_id, local_file_id) FROM play_history
             WHERE COALESCE(content_id, local_file_id) IS NOT NULL
             ORDER BY played_at DESC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn content_member_refs(&self, table: &str, owner: Option<&str>) -> AppResult<Vec<MemberRef>> {
        let sql = if table == "favorites" {
            "SELECT content_id, local_file_id FROM favorites".to_string()
        } else {
            "SELECT content_id, local_file_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position"
                .to_string()
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(MemberRef {
                content_id: row.get(0)?,
                local_file_id: row.get(1)?,
            })
        };
        let rows = if let Some(owner) = owner {
            stmt.query_map(params![owner], map)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    pub fn note_failure(
        &self,
        source_device_id: Option<&str>,
        content_id: Option<&str>,
        local_file_id: Option<&str>,
        class_name: &str,
        reason: &str,
        attempt_id: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO playback_notes(source_device_id, content_id, local_file_id, class, reason, attempt_id)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_device_id, content_id, local_file_id, class_name, reason, attempt_id],
        )?;
        if matches!(class_name, "decode" | "unsupported") {
            if let Some(content_id) = content_id {
                self.conn.execute(
                    "UPDATE content_items SET unplayable = 1, unplayable_reason = ?1 WHERE content_id = ?2",
                    params![reason, content_id],
                )?;
            }
        }
        Ok(())
    }

    pub fn clear_transport_notes(&self, attempt_id: &str) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM playback_notes WHERE class = 'transport' AND attempt_id = ?1",
            params![attempt_id],
        )?;
        Ok(())
    }

    pub fn clear_unplayable(&self, content_id: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE content_items SET unplayable = 0, unplayable_reason = NULL WHERE content_id = ?1",
            params![content_id],
        )?;
        self.conn.execute(
            "DELETE FROM playback_notes WHERE content_id = ?1 AND class IN ('decode', 'unsupported')",
            params![content_id],
        )?;
        Ok(())
    }

    pub fn unplayable_reason(&self, content_id: &str) -> AppResult<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT unplayable_reason FROM content_items WHERE content_id = ?1 AND unplayable = 1",
                params![content_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Content marked unplayable, for listing many songs at once.
    pub fn unplayable_content_ids(&self) -> AppResult<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT content_id FROM content_items WHERE unplayable = 1")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<HashSet<_>, _>>()?)
    }

    /// Content ids marked as favorites, for listing many songs at once.
    pub fn favorite_content_set(&self) -> AppResult<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT content_id FROM favorites WHERE content_id IS NOT NULL")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<HashSet<_>, _>>()?)
    }

    /// Returns each matching local file with a lyrics excerpt when the lyrics column matched.
    pub fn search_hits(&self, query: &str, limit: i64) -> AppResult<std::collections::HashMap<String, Option<String>>> {
        let match_query = fts_query(query);
        if match_query.is_empty() {
            return Ok(Default::default());
        }
        let mut stmt = self.conn.prepare(
            "SELECT local_file_id, snippet(library_fts, 4, char(2), char(3), '…', 10)
             FROM library_fts WHERE library_fts MATCH ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![match_query, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (id, snippet) = row?;
            out.insert(id, snippet.filter(|text| text.contains('\u{2}')));
        }
        Ok(out)
    }

    pub fn lyrics(&self, local_file_id: &str) -> AppResult<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(lyrics, '') FROM local_files WHERE local_file_id = ?1",
                params![local_file_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn peers(&self) -> AppResult<Vec<PeerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT device_id, public_key, cert_id, display_name, trust FROM peers",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PeerRow {
                device_id: row.get(0)?,
                public_key: row.get(1)?,
                cert_id: row.get(2)?,
                display_name: row.get(3)?,
                trust: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_peer(&self, peer: &PeerRow) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO peers(device_id, public_key, cert_id, display_name, trust)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(device_id) DO UPDATE SET
               public_key = excluded.public_key,
               cert_id = excluded.cert_id,
               display_name = excluded.display_name,
               trust = excluded.trust",
            params![
                peer.device_id,
                peer.public_key,
                peer.cert_id,
                peer.display_name,
                peer.trust
            ],
        )?;
        Ok(())
    }

    pub fn rename_peer(&self, device_id: &str, name: &str) -> AppResult<bool> {
        let changed = self.conn.execute(
            "UPDATE peers SET display_name = ?2 WHERE device_id = ?1 AND display_name <> ?2",
            params![device_id, name],
        )?;
        Ok(changed > 0)
    }

    pub fn remove_peer(&self, device_id: &str) -> AppResult<()> {
        self.delete_peer_holdings(device_id)?;
        self.conn
            .execute("DELETE FROM peers WHERE device_id = ?1", params![device_id])?;
        self.conn.execute(
            "DELETE FROM peer_list_prefs WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(())
    }

    /// Pairing with the code again lets a removed device back in.
    pub fn end_removal(&self, device_id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM removed_devices WHERE device_id = ?1", params![device_id])?;
        Ok(())
    }

    /// Keeps a device removed on this computer out, here only.
    pub fn record_removal(&self, device_id: &str, name: &str, at_ms: i64) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO removed_devices(device_id, name, removed_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(device_id) DO UPDATE SET name = excluded.name, removed_at = excluded.removed_at",
            params![device_id, name, at_ms],
        )?;
        Ok(())
    }

    /// When the device was removed, if it is still kept out.
    pub fn removed_at(&self, device_id: &str) -> AppResult<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT removed_at FROM removed_devices WHERE device_id = ?1", params![device_id], |row| row.get(0))
            .optional()?)
    }

    pub fn is_removed(&self, device_id: &str) -> bool {
        self.removed_at(device_id).ok().flatten().is_some()
    }

    /// Removed devices, newest first.
    pub fn removals(&self) -> AppResult<Vec<RemovalRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT device_id, name, removed_at FROM removed_devices ORDER BY removed_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RemovalRow { device_id: row.get(0)?, name: row.get(1)?, removed_at: row.get(2)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_peer_trust(&self, device_id: &str, trust: &str) -> AppResult<()> {
        let trust = if trust == "verify" { "verify" } else { "full" };
        self.conn.execute("UPDATE peers SET trust = ?2 WHERE device_id = ?1", params![device_id, trust])?;
        Ok(())
    }

    /// What this computer does when another device presses Send for `feature`: `silent` applies it,
    /// `ask` asks first, `deny` refuses it. Override, asked for from here, is not affected.
    pub fn overwrite_policy(&self, feature: &str) -> &'static str {
        let value = self.setting_json("overwritePolicy").unwrap_or(Value::Null);
        match value.get(feature).and_then(|item| item.as_str()) {
            Some("ask") => "ask",
            Some("deny") => "deny",
            _ => "silent",
        }
    }

    pub fn is_blocked(&self, device_id: &str) -> AppResult<bool> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM blocks WHERE device_id = ?1", params![device_id], |row| row.get(0))?;
        Ok(count > 0)
    }

    pub fn blocks(&self) -> AppResult<Vec<BlockRow>> {
        let mut stmt = self.conn.prepare("SELECT device_id, name, blocked_at FROM blocks ORDER BY blocked_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(BlockRow { device_id: row.get(0)?, name: row.get(1)?, blocked_at: row.get(2)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn block_device(&self, device_id: &str, name: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO blocks(device_id, name, blocked_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(device_id) DO UPDATE SET name = excluded.name",
            params![device_id, name, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn unblock_device(&self, device_id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM blocks WHERE device_id = ?1", params![device_id])?;
        Ok(())
    }

    /// Incoming attempts from this device are dropped until the silence window ends.
    pub fn silence(&self, device_id: &str) -> AppResult<()> {
        let seconds = self.setting_i64("silenceSeconds").unwrap_or(60).clamp(1, 24 * 60 * 60);
        let until = Utc::now().timestamp_millis() + seconds * 1000;
        let mut map = self.setting_json("silenced").unwrap_or(json!({}));
        if !map.is_object() {
            map = json!({});
        }
        map[device_id] = json!(until);
        self.set_device_setting("silenced", map)
    }

    pub fn silence_active(&self, device_id: &str) -> bool {
        let Ok(map) = self.setting_json("silenced") else { return false };
        let until = map.get(device_id).and_then(|value| value.as_i64()).unwrap_or(0);
        until > Utc::now().timestamp_millis()
    }

    pub fn add_notice(&self, kind: &str, device_id: &str, name: &str, detail: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO notices(at, kind, device_id, name, detail, read, pinned) VALUES(?1, ?2, ?3, ?4, ?5, 0, 0)",
            params![Utc::now().to_rfc3339(), kind, device_id, name, detail],
        )?;
        self.conn.execute(
            "DELETE FROM notices WHERE pinned = 0 AND id NOT IN (SELECT id FROM notices WHERE pinned = 0 ORDER BY id DESC LIMIT 200)",
            [],
        )?;
        Ok(())
    }

    pub fn notices(&self) -> AppResult<Vec<NoticeRow>> {
        // Block, unblock, and decline are local actions. Drop any already stored, and the conflict
        // notices that automatic sync used to leave.
        self.conn.execute(
            "DELETE FROM notices WHERE kind IN ('blocked', 'unblocked', 'declined', 'conflict') OR (kind = 'disconnected' AND detail = 'incognito') OR (kind = 'sync' AND detail = 'queue')",
            [],
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT id, at, kind, device_id, name, detail, read, pinned FROM notices ORDER BY pinned DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(NoticeRow {
                id: row.get(0)?,
                at: row.get(1)?,
                kind: row.get(2)?,
                device_id: row.get(3)?,
                name: row.get(4)?,
                detail: row.get(5)?,
                read: row.get::<_, i64>(6)? != 0,
                pinned: row.get::<_, i64>(7)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_notice_read(&self, id: i64, read: bool) -> AppResult<()> {
        self.conn.execute("UPDATE notices SET read = ?2 WHERE id = ?1", params![id, i64::from(read)])?;
        Ok(())
    }

    pub fn mark_notices_read(&self, ids: &[i64]) -> AppResult<()> {
        for id in ids {
            self.set_notice_read(*id, true)?;
        }
        Ok(())
    }

    pub fn mark_all_notices_read(&self) -> AppResult<()> {
        self.conn.execute("UPDATE notices SET read = 1 WHERE read = 0", [])?;
        Ok(())
    }

    pub fn set_notice_pinned(&self, id: i64, pinned: bool) -> AppResult<()> {
        self.conn.execute("UPDATE notices SET pinned = ?2 WHERE id = ?1", params![id, i64::from(pinned)])?;
        Ok(())
    }

    pub fn delete_notice(&self, id: i64) -> AppResult<()> {
        self.conn.execute("DELETE FROM notices WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_notices(&self, keep_pinned: bool) -> AppResult<()> {
        if keep_pinned {
            self.conn.execute("DELETE FROM notices WHERE pinned = 0", [])?;
        } else {
            self.conn.execute("DELETE FROM notices", [])?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PeerRow {
    pub device_id: String,
    /// DER SubjectPublicKeyInfo of the device's TLS key; the device id is its hash.
    pub public_key: Vec<u8>,
    /// strelaysrv id of the device's certificate, used to meet it through a relay.
    pub cert_id: String,
    pub display_name: String,
    pub trust: String,
}

/// A device removed on this computer. Its key is deleted here and this computer refuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalRow {
    pub device_id: String,
    pub name: String,
    /// Milliseconds since 1970.
    pub removed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRow {
    pub device_id: String,
    pub name: String,
    pub blocked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeRow {
    pub id: i64,
    pub at: String,
    pub kind: String,
    pub device_id: String,
    pub name: String,
    pub detail: String,
    pub read: bool,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingInput {
    pub content_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<i64>,
    pub duration_ms: Option<i64>,
    pub codec: String,
}

pub fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| {
            let clean: String = token
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{clean}\"*")
            }
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn device_defaults() -> Vec<(&'static str, Value)> {
    vec![
        ("historyCap", json!(100)),
        ("historyPaused", json!(false)),
        ("incognito", json!(false)),
        ("exposeDefault", json!("hide")),
        ("downloadFolder", json!("")),
        ("volume", json!(1.0)),
        ("window", json!({})),
        ("relayHost", json!("")),
        ("relayPort", json!(22067)),
        ("relayDeviceId", json!("")),
        ("displayName", json!("")),
        ("holdingsRevision", json!(0)),
        ("closeAction", json!("")),
        ("miniTray", json!(true)),
        ("silenceSeconds", json!(60)),
        ("overwritePolicy", json!({"favorites": "silent", "appearance": "silent"})),
        ("syncRequestDevice", json!("")),
        ("downloadHistory", json!([])),
        ("noticeRead", json!("visible")),
        ("noticeClear", json!("ask")),
        ("silenced", json!({})),
    ]
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS content_items (
    content_id TEXT PRIMARY KEY,
    title TEXT,
    artist TEXT,
    album TEXT,
    track_number INTEGER,
    duration_ms INTEGER,
    codec TEXT,
    sample_rate INTEGER,
    bit_depth INTEGER,
    video INTEGER NOT NULL DEFAULT 0,
    unplayable INTEGER NOT NULL DEFAULT 0,
    unplayable_reason TEXT
);
CREATE TABLE IF NOT EXISTS grants (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    exposure TEXT NOT NULL DEFAULT 'follow'
);
CREATE TABLE IF NOT EXISTS local_files (
    local_file_id TEXT PRIMARY KEY,
    grant_id INTEGER,
    relative_path TEXT NOT NULL,
    absolute_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    lyrics TEXT,
    metadata_status TEXT NOT NULL,
    metadata_reason TEXT,
    access_status TEXT NOT NULL,
    access_reason TEXT,
    playback_status TEXT NOT NULL,
    playback_reason TEXT,
    exposed INTEGER NOT NULL,
    added_at TEXT NOT NULL,
    mtime_ms INTEGER,
    size_bytes INTEGER,
    content_id TEXT,
    duration_ms INTEGER,
    title TEXT,
    artist TEXT,
    album TEXT,
    track_number INTEGER,
    codec TEXT,
    sample_rate INTEGER,
    bit_depth INTEGER,
    video INTEGER NOT NULL DEFAULT 0,
    cover_path TEXT,
    cover_checked INTEGER NOT NULL DEFAULT 0,
    UNIQUE(grant_id, relative_path)
);
CREATE TABLE IF NOT EXISTS favorites (
    id INTEGER PRIMARY KEY,
    content_id TEXT UNIQUE,
    local_file_id TEXT UNIQUE
);
CREATE TABLE IF NOT EXISTS holdings (
    device_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    content_id TEXT NOT NULL,
    title TEXT,
    artist TEXT,
    album TEXT,
    track_number INTEGER,
    duration_ms INTEGER,
    codec TEXT,
    PRIMARY KEY (device_id, content_id)
);
CREATE TABLE IF NOT EXISTS holding_revisions (
    device_id TEXT PRIMARY KEY,
    revision INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS remote_covers (
    content_id TEXT PRIMARY KEY,
    path TEXT,
    tried_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS playback_notes (
    id INTEGER PRIMARY KEY,
    source_device_id TEXT,
    content_id TEXT,
    local_file_id TEXT,
    class TEXT NOT NULL,
    reason TEXT,
    attempt_id TEXT
);
CREATE TABLE IF NOT EXISTS playlists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    position INTEGER NOT NULL,
    exposure TEXT NOT NULL DEFAULT 'follow',
    revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    content_id TEXT,
    local_file_id TEXT,
    PRIMARY KEY (playlist_id, position)
);
CREATE TABLE IF NOT EXISTS play_history (
    id INTEGER PRIMARY KEY,
    content_id TEXT,
    local_file_id TEXT,
    played_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS device_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS synced_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS peer_list_prefs (
    device_id TEXT PRIMARY KEY,
    exposure TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS peers (
    device_id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL,
    cert_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    trust TEXT NOT NULL DEFAULT 'full'
);
CREATE TABLE IF NOT EXISTS removed_devices (
    device_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    removed_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS blocks (
    device_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    blocked_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS notices (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL,
    kind TEXT NOT NULL,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS device_identity (
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS player_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pending_content_move (
    local_file_id TEXT PRIMARY KEY,
    previous_content_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS library_order (
    track_key TEXT PRIMARY KEY,
    position INTEGER NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS library_fts USING fts5(
    local_file_id,
    content_id,
    title,
    artist,
    lyrics,
    folder
);
"#;

/// A grant path without trailing separators. A drive root such as `D:\` or `/` keeps its separator.
fn grant_path(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        let root_len = trimmed.len() + usize::from(path.len() > trimmed.len());
        return path[..root_len].to_string();
    }
    trimmed.to_string()
}

/// Windows paths ignore letter case and accept either slash.
fn same_grant_path(left: &str, right: &str) -> bool {
    let left = grant_path(left);
    let right = grant_path(right);
    if cfg!(windows) {
        left.replace('/', "\\").to_lowercase() == right.replace('/', "\\").to_lowercase()
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db(name: &str) -> (Db, PathBuf) {
        let dir = std::env::temp_dir().join(format!("tunepeerly-db-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        (db, dir)
    }

    fn put_marker(db: &Db, value: &str) {
        db.conn.execute("INSERT OR REPLACE INTO device_settings (key, value) VALUES ('marker', ?1)", [value]).unwrap();
    }

    fn marker(db: &Db) -> Option<String> {
        db.conn.query_row("SELECT value FROM device_settings WHERE key = 'marker'", [], |row| row.get(0)).optional().unwrap()
    }

    fn not_a_database(conn: &Connection) -> bool {
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
            .is_err_and(|err| err.sqlite_error_code() == Some(rusqlite::ErrorCode::NotADatabase))
    }

    #[test]
    fn the_library_file_is_encrypted_and_opens_only_with_its_key() {
        let (db, dir) = open_db("encrypted");
        let cipher: String = db.conn.query_row("PRAGMA cipher_version", [], |row| row.get(0)).unwrap();
        assert!(!cipher.is_empty());
        put_marker(&db, "plain-marker-1234");
        drop(db);
        let file = dir.join("library.db");
        let bytes = std::fs::read(&file).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3\0"));
        assert!(!bytes.windows(17).any(|part| part == b"plain-marker-1234"));
        assert!(not_a_database(&Connection::open(&file).unwrap()));
        let other = Connection::open(&file).unwrap();
        other.pragma_update(None, "key", format!("x'{}'", "ab".repeat(KEY_LEN))).unwrap();
        assert!(not_a_database(&other));
        let db = Db::open(&file, dir.clone(), dir.join("cache")).unwrap();
        assert_eq!(marker(&db).as_deref(), Some("plain-marker-1234"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_old_safety_number_column_is_dropped_and_pairings_kept() {
        let (db, dir) = open_db("safety-number");
        db.conn.execute_batch(
            "DROP TABLE peers;
             CREATE TABLE peers (device_id TEXT PRIMARY KEY, public_key BLOB NOT NULL, cert_id TEXT NOT NULL,
               display_name TEXT NOT NULL, safety_number TEXT NOT NULL, trust TEXT NOT NULL DEFAULT 'full');
             INSERT INTO peers VALUES ('dev', x'01', '', 'Old', '', 'full');",
        ).unwrap();
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        let mut stmt = db.conn.prepare("PRAGMA table_info(peers)").unwrap();
        let names = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert!(!names.iter().any(|name| name == "safety_number"));
        drop(stmt);
        let peers = db.peers().unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].display_name, "Old");
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removals_stay_local_and_older_shared_removal_columns_are_dropped() {
        let (db, dir) = open_db("removals");
        db.conn.execute_batch(
            "DROP TABLE removed_devices;
             CREATE TABLE removed_devices (device_id TEXT PRIMARY KEY, name TEXT NOT NULL, removed_at INTEGER NOT NULL, removed_by TEXT NOT NULL);
             INSERT INTO removed_devices VALUES ('gone', 'Laptop', 5, 'someone');
             ALTER TABLE peers ADD COLUMN paired_at INTEGER NOT NULL DEFAULT 0;",
        ).unwrap();
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        let columns = |table: &str| -> Vec<String> {
            let mut stmt = db.conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
            stmt.query_map([], |row| row.get::<_, String>(1)).unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert!(!columns("removed_devices").contains(&"removed_by".to_string()));
        assert!(!columns("peers").contains(&"paired_at".to_string()));
        assert!(db.is_removed("gone"));
        db.record_removal("gone", "Laptop", 9).unwrap();
        db.record_removal("other", "Tower", 7).unwrap();
        let rows = db.removals().unwrap();
        assert_eq!(rows.iter().map(|row| (row.device_id.as_str(), row.removed_at)).collect::<Vec<_>>(), vec![("gone", 9), ("other", 7)]);
        db.end_removal("gone").unwrap();
        assert!(!db.is_removed("gone"));
        assert!(db.is_removed("other"));
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn automatic_sync_settings_and_conflict_notices_are_dropped() {
        let (db, dir) = open_db("auto-sync");
        for key in ["dirty", "pendingSnapshots", "syncSources", "syncBlocks", "autoSend", "conflictPolicy", "historySync", "historyStartup"] {
            db.set_device_setting(key, json!({"favorites": true})).unwrap();
        }
        db.add_notice("conflict", "peer", "Laptop", "favorites:peer").unwrap();
        db.add_notice("sync", "peer", "Laptop", "favorites").unwrap();
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        let settings = db.device_settings_map().unwrap();
        for key in ["dirty", "pendingSnapshots", "syncSources", "syncBlocks", "autoSend", "conflictPolicy", "historySync", "historyStartup"] {
            assert!(settings.get(key).is_none(), "{key} is still stored");
        }
        assert!(settings.get("overwritePolicy").is_some());
        let kinds: Vec<String> = db.notices().unwrap().into_iter().map(|row| row.kind).collect();
        assert_eq!(kinds, vec!["sync".to_string()]);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_saved_lan_port_is_dropped() {
        let (db, dir) = open_db("listen-port");
        assert!(db.device_settings_map().unwrap().get("listenPort").is_none());
        db.set_device_setting("listenPort", json!(22000)).unwrap();
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        assert!(db.device_settings_map().unwrap().get("listenPort").is_none());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_old_overwrite_mode_becomes_one_choice_per_feature() {
        let (db, dir) = open_db("overwrite-mode");
        assert_eq!(db.overwrite_policy("favorites"), "silent");
        db.set_device_setting("overwriteMode", json!("ask")).unwrap();
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        assert_eq!(db.overwrite_policy("favorites"), "ask");
        assert_eq!(db.overwrite_policy("appearance"), "ask");
        assert!(db.device_settings_map().unwrap().get("overwriteMode").is_none());
        db.set_device_setting("overwritePolicy", json!({"favorites": "deny", "appearance": "silent"})).unwrap();
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        assert_eq!(db.overwrite_policy("favorites"), "deny");
        assert_eq!(db.overwrite_policy("appearance"), "silent");
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_plain_library_is_encrypted_on_first_open() {
        let dir = std::env::temp_dir().join(format!("tunepeerly-db-plain-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("library.db");
        let plain = Connection::open(&file).unwrap();
        plain.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE device_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO device_settings VALUES ('marker', 'kept');").unwrap();
        drop(plain);
        assert!(std::fs::read(&file).unwrap().starts_with(b"SQLite format 3\0"));
        let db = Db::open(&file, dir.clone(), dir.join("cache")).unwrap();
        assert_eq!(marker(&db).as_deref(), Some("kept"));
        drop(db);
        assert!(!std::fs::read(&file).unwrap().starts_with(b"SQLite format 3\0"));
        assert!(!dir.join("library.db.encrypting").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_library_without_its_key_is_set_aside_not_wiped() {
        let (db, dir) = open_db("lost-key");
        put_marker(&db, "old");
        drop(db);
        std::fs::remove_file(dir.join(dbkey::KEY_NAME)).unwrap();
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        assert_eq!(marker(&db), None);
        let aside: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|entry| entry.ok()?.file_name().into_string().ok()).collect();
        assert!(aside.iter().any(|name| name.starts_with("library.db.unreadable-")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn grant_paths_lose_trailing_separators_but_roots_stay() {
        assert_eq!(grant_path(r"D:\Music\"), r"D:\Music");
        assert_eq!(grant_path("/home/me/music//"), "/home/me/music");
        assert_eq!(grant_path(r"D:\"), r"D:\");
        assert_eq!(grant_path("D:"), "D:");
        assert_eq!(grant_path("/"), "/");
        assert!(same_grant_path(r"D:\Music", r"D:\Music\"));
        assert_eq!(same_grant_path(r"D:\Music", r"d:\music"), cfg!(windows));
    }

    #[test]
    fn the_same_folder_added_twice_is_one_grant() {
        let (db, dir) = open_db("grant");
        let first = db.insert_grant("directory", r"D:\Music").unwrap();
        let second = db.insert_grant("directory", r"D:\Music\").unwrap();
        assert_eq!(first, second);
        assert_eq!(db.grants().unwrap().len(), 1);
        assert_eq!(db.grants().unwrap()[0].path, r"D:\Music");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn opening_twice_keeps_removed_settings_away_and_indexes_in_place() {
        let (db, dir) = open_db("migrate");
        drop(db);
        let db = Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).unwrap();
        let legacy: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM device_settings WHERE key IN ('historySyncMode', 'syncEnabled', 'audioShare', 'positionSync', 'positionSyncDevice')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy, 0);
        let indexes: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'holdings_content'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(indexes, 1);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_holdings_write_keeps_the_earlier_catalog() {
        let (db, dir) = open_db("holdings");
        let row = |id: &str| HoldingInput {
            content_id: id.into(),
            title: "t".into(),
            artist: String::new(),
            album: String::new(),
            track_number: None,
            duration_ms: None,
            codec: String::new(),
        };
        assert!(db.replace_peer_holdings("peer", 1, &[row("a")]).unwrap());
        db.conn.execute_batch("CREATE TRIGGER fail_b BEFORE INSERT ON holdings WHEN NEW.content_id = 'b' BEGIN SELECT RAISE(ABORT, 'no'); END;").unwrap();
        assert!(db.replace_peer_holdings("peer", 2, &[row("c"), row("b")]).is_err());
        assert!(db.conn.is_autocommit());
        let held: Vec<String> = {
            let mut stmt = db.conn.prepare("SELECT content_id FROM holdings WHERE device_id = 'peer'").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
        };
        assert_eq!(held, vec!["a".to_string()]);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
