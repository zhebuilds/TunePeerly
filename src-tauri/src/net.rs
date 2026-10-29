// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsConnector;

use crate::catalog::{local_publishable, readable_paths};
use crate::media::{self, AudioBus, AudioPiece, Pull};
use crate::db::{merge_history_into_db, Db, HoldingInput};
use crate::error::{AppError, AppResult};
use crate::frames::{self, Frame};
use crate::identity::{self, IdentityMaterial, SignalStore};
use crate::ids::{self, smaller_device};
use crate::relay;
use crate::sync_logic::{self, HistoryEntry, PlaySource, SourceChoice, SourceKind};
use crate::tls;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nearby {
    pub device_id: String,
    pub name: String,
    pub host: String,
    /// Every IPv4 address of the device that this computer can use, best first.
    pub hosts: Vec<String>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    pub device_id: String,
    pub name: String,
    pub state: String,
    pub path: String,
    pub detail: String,
    pub trust: String,
}

#[derive(Clone)]
pub(crate) struct OverwriteAsk {
    id: u64,
    device_id: String,
    name: String,
    feature: String,
    items: Value,
    shown: Option<Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingOffer {
    pub device_id: String,
    pub name: String,
    pub safety_number: String,
    pub cert_der: Vec<u8>,
    pub identity_public: Vec<u8>,
    pub cert_id: String,
    /// "incoming" asks this device to accept or decline. "outgoing" is waiting on the other side.
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    /// Bumped by `store_download` on every new status, so the UI can tell a repeated request from a refresh.
    pub seq: u64,
    pub content_id: String,
    pub state: String,
    pub detail: String,
    pub title: String,
    pub received: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedDownload {
    pub content_id: String,
    pub title: String,
}

#[derive(Clone)]
pub struct Live {
    pub inner: Arc<Mutex<LiveInner>>,
}

#[derive(Default)]
pub struct LiveInner {
    pub nearby: Vec<Nearby>,
    pub links: Vec<LinkStatus>,
    pub listen_port: u16,
    /// Saved port that was busy, while `listen_port` is a temporary one.
    pub port_fallback: Option<u16>,
    pub relay: String,
    pub download: Option<DownloadStatus>,
    /// Songs waiting because another download is already copying.
    pub download_queue: VecDeque<QueuedDownload>,
    pub offer: Option<PairingOffer>,
    pub manual: Option<String>,
    pub failures: HashMap<String, String>,
    pub playing: bool,
    pub current_local: Option<String>,
    pub buffer_enough: bool,
    pub serving: bool,
    pub serving_count: u32,
    /// Requests currently being served.  This keeps stale cancellation IDs
    /// from accumulating after a request has completed.
    pub active_audio: HashSet<u64>,
    pub cancelled: HashSet<u64>,
    /// Devices whose song list just changed and whose pictures may need fetching.
    pub covers_wanted: HashSet<String>,
    pub covers_busy: HashSet<String>,
    pub pending_snapshots: HashMap<String, SnapshotBuild>,
    /// Paired devices this computer disconnected on purpose. Automatic redial waits until Connect.
    pub paused: HashSet<String>,
    /// Paired devices that disconnected this computer on purpose (they sent `Frame::Hold`).
    /// They are not dialed again until they connect back or the user presses Connect here.
    pub held: HashSet<String>,
    pub overwrite_waiting: VecDeque<OverwriteAsk>,
    pub overwrite_shown: Vec<OverwriteAsk>,
    pub overwrite_next: u64,
    /// Devices we asked for Recently Played during the one startup merge.
    pub history_asked: HashSet<String>,
    pub history_waiting: HashSet<String>,
    pub history_deadline: Option<Instant>,
    pub history_done: bool,
    /// Copies this computer asked for with "sync-request", keyed by device and feature, until the deadline.
    /// Only these replies may skip the overwrite question.
    pub sync_requests: HashMap<String, Instant>,
    /// Sync requests this computer answered recently, keyed like `sync_requests`.
    pub sync_replies: HashMap<String, Instant>,
    /// Last device-control report seen from each device, as (epoch, seq).
    pub connect_seen: HashMap<String, (u64, u64)>,
    /// Device-control messages counted per sender and kind.
    pub connect_rate: HashMap<String, crate::connect::RateWindow>,
    /// Number of the last device-control report this computer sent.
    pub connect_seq: u64,
    /// This computer left device control to play on its own. Kept in memory only, so every start
    /// joins the group again.
    pub connect_detached: bool,
}

pub enum Cmd {
    Apply,
    Publish,
    AutoSync(String),
    Pair { device_id: String, host: Option<String> },
    Confirm { device_id: String, trust: String },
    Reject(String),
    Block(String),
    Unblock(String),
    Trust { device_id: String, trust: String },
    ClearNotices,
    Notice { action: String, id: i64, ids: Vec<i64> },
    Remove(String),
    Paste(String),
    Sync { feature: String, refresh: bool },
    RequestSync { feature: String, device_id: String },
    Resolve { feature: String, take: bool },
    Download(String),
    Activity { playing: bool, local_file_id: Option<String>, buffer_enough: bool },
    /// A device-control message from the player. Reports go to every device, the rest to `to`.
    Connect { kind: String, to: Option<String>, body: Value },
    /// Leaves device control to play on its own, or joins it again.
    ConnectMode { detached: bool },
    CloseRoutes,
    Disconnect(String),
    ResolveOverwrite { id: u64, accept: bool },
}

pub struct Network {
    pub tx: mpsc::UnboundedSender<Cmd>,
    pub live: Live,
    pub bus: Arc<AudioBus>,
}

pub fn spawn(
    app: AppHandle,
    db: Arc<Mutex<Db>>,
    material: IdentityMaterial,
) -> AppResult<Network> {
    let live = Live {
        inner: Arc::new(Mutex::new(LiveInner::default())),
    };
    let bus = AudioBus::new();
    let (tx, rx) = mpsc::unbounded_channel();
    let tx_loop = tx.clone();
    let (pull_tx, pull_rx) = mpsc::unbounded_channel();
    let (media_ready_tx, media_ready_rx) = std::sync::mpsc::sync_channel(1);
    let live_thread = live.clone();
    let bus_thread = bus.clone();
    std::thread::spawn(move || {
        // A failure here must not take the whole app down: dropping media_ready_tx makes spawn report the error.
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(err) => {
                eprintln!("network runtime: {err}");
                return;
            }
        };
        let local = tokio::task::LocalSet::new();
        runtime.block_on(local.run_until(async move {
            tokio::task::spawn_local(media::serve(bus_thread.clone(), pull_tx, media_ready_tx));
            run(app, db, material, tx_loop, rx, pull_rx, live_thread, bus_thread).await;
        }));
    });
    media_ready_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| AppError::Message("media proxy unavailable".into()))?;
    Ok(Network { tx, live, bus })
}

struct PeerOut {
    device_id: String,
    conn: u64,
    dialed_by: String,
    confirmed: bool,
    incoming: bool,
    path: String,
    tx: mpsc::UnboundedSender<Frame>,
    /// Bounded so file reads wait for the socket instead of queueing a whole file.
    audio: mpsc::Sender<Frame>,
    /// Turns false when this connection is dropped, so transfers still reading a file stop at once.
    alive: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for PeerOut {
    fn drop(&mut self) {
        self.alive.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

async fn run(
    app: AppHandle,
    db: Arc<Mutex<Db>>,
    material: IdentityMaterial,
    tx: mpsc::UnboundedSender<Cmd>,
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    mut pull_rx: mpsc::UnboundedReceiver<Pull>,
    live: Live,
    bus: Arc<AudioBus>,
) {
    let mut store = match SignalStore::open(db.clone(), material.identity, material.registration_id) {
        Ok(store) => store,
        Err(err) => {
            // Stay offline instead of aborting: local playback keeps working and the reason is shown as a notice.
            eprintln!("signal store: {err}");
            let _ = db
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .add_notice("networkFailed", "", &err.to_string(), "");
            let _ = app.emit("network-updated", json!({}));
            // Keep the thread (and the media proxy beside it) alive and drop commands until the app quits.
            while rx.recv().await.is_some() {}
            return;
        }
    };
    let _ = store.ensure_prekeys().await;
    // Read the Windows network profiles once now so the first screen already knows which adapters are usable.
    std::thread::spawn(|| {
        let _ = lan_ipv4();
    });
    let mut peers: HashMap<String, PeerOut> = HashMap::new();
    let mut pending: HashSet<String> = HashSet::new();
    let mut listener: Option<tokio::task::JoinHandle<()>> = None;
    let mut relay_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut discovery: Option<std::sync::mpsc::Sender<()>> = None;
    let mut actual_port = 0u16;
    let mut ready_sessions: HashSet<String> = HashSet::new();
    let mut pending_holdings: HashMap<String, HoldingsBuild> = HashMap::new();
    let mut published_holdings = String::new();
    let provider = tls::provider();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<NetEvent>();
    trust_saved_peers(&db, &mut store);
    publish(&app, &db, &live, &peers);
    let mut reconnect = tokio::time::interval(Duration::from_secs(30));
    let mut overwrite_tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = overwrite_tick.tick() => {
                expire_overwrites(&app, &db, &live);
                tick_history_startup(&db, &material, &mut store, &peers, &live).await;
            }
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Cmd::Apply => {
                        let open = db.lock().map(|guard| network_open(&guard)).unwrap_or(false);
                        if !open {
                            publish_holdings(&db, &material, &mut store, &peers, &mut published_holdings).await;
                            peers.clear();
                            ready_sessions.clear();
                            bus.close_routes();
                            if let Ok(mut guard) = live.inner.lock() {
                                guard.offer = None;
                            }
                        }
                        restart_listener(&db, &material, &provider, &event_tx, &mut listener, &mut actual_port, &live).await;
                        restart_discovery(&db, &material, actual_port, &event_tx, &mut discovery, &live).await;
                        restart_relay(&db, &material, &provider, &event_tx, &mut relay_task, &live).await;
                        if open {
                            publish_holdings(&db, &material, &mut store, &peers, &mut published_holdings).await;
                            dial_known(&db, &material, &provider, &event_tx, &live, &peers).await;
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Publish => {
                        publish_holdings(&db, &material, &mut store, &peers, &mut published_holdings).await;
                    }
                    Cmd::AutoSync(feature) => {
                        auto_sync(&db, &material, &mut store, &peers, &feature).await;
                    }
                    Cmd::Pair { device_id, host } => {
                        if let Ok(mut guard) = live.inner.lock() {
                            guard.paused.remove(&device_id);
                            guard.held.remove(&device_id);
                            guard.failures.remove(&device_id);
                        }
                        pending.insert(device_id.clone());
                        if let Some(target) = nearby_of(&live, &device_id) {
                            // A chosen address is the only one used; otherwise every known address is tried.
                            let hosts = match host.filter(|chosen| target.hosts.contains(chosen)) {
                                Some(chosen) => vec![chosen],
                                None => target.hosts.clone(),
                            };
                            dial_peer(&db, &material, &provider, &event_tx, hosts, target.port, Some(device_id), &material.device_id).await;
                        } else {
                            let _ = event_tx.send(NetEvent::DialFailed { device_id: Some(device_id), target: None });
                        }
                    }
                    Cmd::Confirm { device_id, trust } => {
                        if confirm_peer(&db, &material, &live, &mut peers, &device_id, &trust) {
                            trust_saved_peers(&db, &mut store);
                            let _ = peers.get(&device_id).and_then(|peer| peer.tx.send(Frame::PairAccept).ok());
                            send_bundle(&db, &mut store, &peers, &device_id).await;
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Reject(device_id) => {
                        pending.remove(&device_id);
                        let incoming = peers.get(&device_id).is_some_and(|peer| peer.incoming && !peer.confirmed);
                        if incoming
                            && let Ok(guard) = db.lock()
                        {
                            let _ = guard.silence(&device_id);
                        }
                        if peers.get(&device_id).is_some_and(|peer| !peer.confirmed) {
                            peers.remove(&device_id);
                        }
                        if let Ok(mut guard) = live.inner.lock() {
                            if guard.offer.as_ref().is_some_and(|offer| offer.device_id == device_id) {
                                guard.offer = None;
                            }
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Block(device_id) => {
                        let name = block_name(&db, &live, &device_id);
                        if let Ok(guard) = db.lock() {
                            let _ = guard.block_device(&device_id, &name);
                        }
                        peers.remove(&device_id);
                        pending.remove(&device_id);
                        ready_sessions.remove(&device_id);
                        pending_holdings.remove(&device_id);
                        drop_partial_snapshots(&live, &device_id);
                        if let Ok(mut guard) = live.inner.lock() {
                            if guard.offer.as_ref().is_some_and(|offer| offer.device_id == device_id) {
                                guard.offer = None;
                            }
                            // Questions this device asked before it was blocked are dropped.
                            guard.overwrite_waiting.retain(|ask| ask.device_id != device_id);
                            guard.overwrite_shown.retain(|ask| ask.device_id != device_id);
                            promote_overwrites(&mut guard);
                        }
                        emit_overwrite_asks(&app, &live);
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Unblock(device_id) => {
                        if let Ok(guard) = db.lock() {
                            let _ = guard.unblock_device(&device_id);
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Trust { device_id, trust } => {
                        if let Ok(guard) = db.lock() {
                            let _ = guard.set_peer_trust(&device_id, &trust);
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::ClearNotices => {
                        if let Ok(guard) = db.lock() {
                            let _ = guard.clear_notices(false);
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Notice { action, id, ids } => {
                        if let Ok(guard) = db.lock() {
                            let _ = match action.as_str() {
                                "read" => guard.set_notice_read(id, true),
                                "read-ids" => guard.mark_notices_read(&ids),
                                "read-all" => guard.mark_all_notices_read(),
                                "unread" => guard.set_notice_read(id, false),
                                "pin" => guard.set_notice_pinned(id, true),
                                "unpin" => guard.set_notice_pinned(id, false),
                                "delete" => guard.delete_notice(id),
                                "clear-unpinned" => guard.clear_notices(true),
                                "clear-all" => guard.clear_notices(false),
                                _ => Ok(()),
                            };
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Remove(device_id) => {
                        peers.remove(&device_id);
                        pending.remove(&device_id);
                        ready_sessions.remove(&device_id);
                        pending_holdings.remove(&device_id);
                        drop_partial_snapshots(&live, &device_id);
                        if let Ok(guard) = db.lock() {
                            let _ = guard.remove_peer(&device_id);
                        }
                        store.forget_peer(&device_id);
                        store.forget_sender_key(&material.device_id, material.distribution_id);
                        let remaining: Vec<String> = peers.keys().filter(|id| ready_sessions.contains(*id)).cloned().collect();
                        for peer_id in remaining {
                            send_sender_key(&material, &mut store, &peers, &peer_id).await;
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::Paste(text) => {
                        match identity::parse_pairing_text(&text) {
                            Ok(parsed) => {
                                stage_offer(&db, &material, &live, &parsed.identity_public, &parsed.cert_der, &parsed.display_name, "incoming", &app);
                            }
                            Err(_) => {
                                set_manual(&live, "badText");
                                publish(&app, &db, &live, &peers);
                            }
                        }
                    }
                    Cmd::Sync { feature, refresh } => {
                        send_sync(&db, &material, &mut store, &peers, &feature, refresh).await;
                    }
                    Cmd::RequestSync { feature, device_id } => {
                        request_sync(&db, &material, &mut store, &peers, &live, &feature, &device_id).await;
                    }
                    Cmd::Resolve { feature, take } => {
                        resolve_pending(&app, &db, &feature, take);
                    }
                    Cmd::Download(content_id) => {
                        start_download(&app, &db, &peers, &live, &bus, &tx, content_id).await;
                    }
                    Cmd::Activity { playing, local_file_id, buffer_enough } => {
                        if let Ok(mut guard) = live.inner.lock() {
                            guard.playing = playing;
                            guard.current_local = local_file_id;
                            guard.buffer_enough = buffer_enough;
                        }
                    }
                    Cmd::Connect { kind, to, body } => {
                        send_connect(&db, &live, &material, &mut store, &peers, &kind, to.as_deref(), &body).await;
                    }
                    Cmd::ConnectMode { detached } => {
                        let changed = live.inner.lock().map(|mut state| std::mem::replace(&mut state.connect_detached, detached) != detached).unwrap_or(false);
                        // The others forget this computer at once instead of waiting for it to go quiet.
                        if changed && detached {
                            send_connect(&db, &live, &material, &mut store, &peers, "connect-leave", None, &json!({"v": crate::connect::VERSION})).await;
                        }
                    }
                    Cmd::CloseRoutes => bus.close_routes(),
                    Cmd::Disconnect(device_id) => {
                        if let Ok(mut guard) = live.inner.lock() {
                            guard.paused.insert(device_id.clone());
                        }
                        // Queued before the writer is dropped, so it still goes out; the other side stops dialing back.
                        if let Some(peer) = peers.get(&device_id) {
                            let _ = peer.tx.send(Frame::Hold);
                        }
                        peers.remove(&device_id);
                        ready_sessions.remove(&device_id);
                        pending_holdings.remove(&device_id);
                        drop_partial_snapshots(&live, &device_id);
                        publish(&app, &db, &live, &peers);
                    }
                    Cmd::ResolveOverwrite { id, accept } => {
                        resolve_overwrite(&app, &db, &live, id, accept);
                    }
                }
            }
            event = event_rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    NetEvent::Nearby(list) => {
                        if let Ok(mut guard) = live.inner.lock() {
                            guard.nearby = list;
                        }
                        dial_known(&db, &material, &provider, &event_tx, &live, &peers).await;
                        publish(&app, &db, &live, &peers);
                    }
                    NetEvent::Connected { device_id, conn, target, dialed_by, path, writer, audio, identity_public, cert_der, name, incoming } => {
                        if let Ok(mut guard) = live.inner.lock() {
                            guard.failures.remove(&device_id);
                            guard.held.remove(&device_id);
                        }
                        if let Some(target) = &target {
                            eprintln!("connected to {} at {target}", short_id(&device_id));
                        }
                        let before = peers.get(&device_id).map(|peer| peer.conn);
                        adopt_connection(&db, &material, &live, &mut peers, &pending, &app, device_id.clone(), conn, dialed_by, incoming, path, writer, audio, identity_public, cert_der, name);
                        if before != Some(conn) && peers.get(&device_id).is_some_and(|peer| peer.confirmed && peer.conn == conn) {
                            ready_sessions.remove(&device_id);
                            send_bundle(&db, &mut store, &peers, &device_id).await;
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    NetEvent::DialFailed { device_id, target } => {
                        if let Some(target) = &target {
                            eprintln!("could not connect to {target}");
                        }
                        if let Ok(mut guard) = live.inner.lock() {
                            if let Some(device_id) = device_id {
                                guard.failures.insert(device_id, "failed".into());
                            }
                        }
                        publish(&app, &db, &live, &peers);
                    }
                    NetEvent::Frame { device_id, frame } => {
                        if matches!(frame, Frame::PairAccept) {
                            if accept_remote_pair(&db, &live, &mut peers, &device_id) {
                                trust_saved_peers(&db, &mut store);
                                send_bundle(&db, &mut store, &peers, &device_id).await;
                            }
                            publish(&app, &db, &live, &peers);
                        } else if matches!(frame, Frame::Hold) {
                            // Arrives before the close, so the redial that follows the close already skips it.
                            if peers.contains_key(&device_id)
                                && let Ok(mut guard) = live.inner.lock()
                            {
                                guard.held.insert(device_id.clone());
                            }
                        } else {
                            handle_frame(&app, &db, &material, &mut store, &peers, &live, &bus, &mut ready_sessions, &mut pending_holdings, &mut published_holdings, &device_id, frame).await;
                            let wanted: Vec<String> = live
                                .inner
                                .lock()
                                .map(|mut guard| guard.covers_wanted.drain().collect())
                                .unwrap_or_default();
                            for wanted_id in wanted {
                                spawn_cover_fetch(&app, &db, &peers, &live, &bus, wanted_id);
                            }
                        }
                    }
                    NetEvent::Closed { device_id, conn } => {
                        if peers.get(&device_id).is_none_or(|peer| peer.conn != conn) {
                            continue;
                        }
                        let removed = peers.remove(&device_id);
                        ready_sessions.remove(&device_id);
                        pending_holdings.remove(&device_id);
                        drop_partial_snapshots(&live, &device_id);
                        if let Some(peer) = removed {
                            if peer.confirmed {
                                let paused = live.inner.lock().map(|guard| guard.paused.contains(&device_id)).unwrap_or(false);
                                if !paused {
                                    let name = block_name(&db, &live, &device_id);
                                    if let Ok(guard) = db.lock() {
                                        let _ = guard.add_notice("disconnected", &device_id, &name, &peer.path);
                                    }
                                }
                            } else if !peer.incoming {
                                if let Ok(mut guard) = live.inner.lock() {
                                    guard.failures.insert(device_id.clone(), "failed".into());
                                }
                            }
                            if let Ok(mut guard) = live.inner.lock() {
                                if guard.offer.as_ref().is_some_and(|offer| offer.device_id == device_id) {
                                    guard.offer = None;
                                }
                            }
                        }
                        publish(&app, &db, &live, &peers);
                        // Try the LAN again right away; the nearby list does not change when a connection drops.
                        dial_known(&db, &material, &provider, &event_tx, &live, &peers).await;
                        if !live.inner.lock().is_ok_and(|guard| guard.on_hold().contains(&device_id)) {
                            maybe_relay_fallback(&db, &material, &provider, &event_tx, &device_id).await;
                        }
                    }
                    NetEvent::Invitation(invitation) => {
                        join_invited(&db, &material, &provider, &event_tx, invitation).await;
                    }
                    NetEvent::RelayState(text) => {
                        set_relay(&live, &text);
                        publish(&app, &db, &live, &peers);
                    }
                }
            }
            _ = reconnect.tick() => {
                let open = db.lock().map(|guard| network_open(&guard)).unwrap_or(false);
                if open {
                    dial_known(&db, &material, &provider, &event_tx, &live, &peers).await;
                    let saved: Vec<String> = {
                        let Ok(guard) = db.lock() else { continue };
                        guard.peers().unwrap_or_default().into_iter().filter(|peer| {
                            peer.trust != "verify" && !guard.is_blocked(&peer.device_id).unwrap_or(false)
                        }).map(|peer| peer.device_id).collect()
                    };
                    // The relay is not asked for a device the user disconnected either.
                    let paused = live.inner.lock().map(|guard| guard.on_hold()).unwrap_or_default();
                    let offline = redial_ids(saved, |id| peers.contains_key(id), &paused);
                    for device_id in offline {
                        maybe_relay_fallback(&db, &material, &provider, &event_tx, &device_id).await;
                    }
                }
            }
            pull = pull_rx.recv() => {
                match pull {
                    Some(Pull::Open { id, device_id, content_id, start, end, reply, queued }) => {
                        bus.register_limited(id, reply, queued);
                        if let Some(peer) = peers.get(&device_id).filter(|peer| peer.confirmed) {
                            if peer.tx.send(Frame::AudioReq { id, content_id, start, end }).is_err() {
                                bus.deliver(id, AudioPiece::failed("offline"));
                            }
                        } else {
                            bus.deliver(id, AudioPiece::failed("offline"));
                        }
                    }
                    Some(Pull::Cancel { id, device_id }) => {
                        bus.finish(id);
                        if let Some(peer) = peers.get(&device_id) {
                            let _ = peer.tx.send(Frame::AudioCancel { id });
                        }
                    }
                    None => {}
                }
            }
        }
    }
}

enum NetEvent {
    Nearby(Vec<Nearby>),
    Connected {
        device_id: String,
        conn: u64,
        target: Option<String>,
        dialed_by: String,
        path: String,
        writer: mpsc::UnboundedSender<Frame>,
        audio: mpsc::Sender<Frame>,
        identity_public: Vec<u8>,
        cert_der: Vec<u8>,
        name: String,
        incoming: bool,
    },
    Frame { device_id: String, frame: Frame },
    Closed { device_id: String, conn: u64 },
    DialFailed { device_id: Option<String>, target: Option<String> },
    Invitation(relay::Invitation),
    RelayState(String),
}

fn network_open(db: &Db) -> bool {
    let (incognito, discovery, sync, audio) = switches(db);
    !incognito && (discovery || sync || audio)
}

fn set_manual(live: &Live, state: &str) {
    if let Ok(mut guard) = live.inner.lock() {
        guard.manual = Some(state.to_string());
    }
}


/// Paired devices that are not connected get dialed at the address Nearby discovery found for them.
async fn dial_known(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    live: &Live,
    peers: &HashMap<String, PeerOut>,
) {
    let (saved, allowed) = {
        let Ok(guard) = db.lock() else { return };
        let (incognito, _, sync, audio) = switches(&guard);
        let saved: Vec<String> = guard
            .peers()
            .unwrap_or_default()
            .into_iter()
            .filter(|peer| peer.trust != "verify" && !guard.is_blocked(&peer.device_id).unwrap_or(false))
            .map(|peer| peer.device_id)
            .collect();
        (saved, !incognito && (sync || audio))
    };
    if !allowed {
        return;
    }
    let paused = live.inner.lock().map(|guard| guard.on_hold()).unwrap_or_default();
    let missing = redial_ids(saved, |id| peers.contains_key(id), &paused);
    if missing.is_empty() {
        return;
    }
    let nearby = live.inner.lock().map(|guard| guard.nearby.clone()).unwrap_or_default();
    for device_id in &missing {
        if let Some(target) = nearby.iter().find(|item| &item.device_id == device_id) {
            if !dial_gate(device_id) {
                continue;
            }
            dial_peer(db, material, provider, events, target.hosts.clone(), target.port, Some(device_id.clone()), &material.device_id).await;
        }
    }
}

/// Lets automatic reconnects dial one device at most every 6 seconds.
fn dial_gate(device_id: &str) -> bool {
    static GATE: std::sync::OnceLock<Mutex<HashMap<String, Instant>>> = std::sync::OnceLock::new();
    let Ok(mut map) = GATE.get_or_init(Default::default).lock() else { return true };
    let now = Instant::now();
    if map.get(device_id).is_some_and(|last| now.duration_since(*last) < Duration::from_secs(6)) {
        return false;
    }
    map.insert(device_id.to_string(), now);
    true
}

fn switches(db: &Db) -> (bool, bool, bool, bool) {
    let incognito = db.setting_bool("incognito").unwrap_or(false);
    let discovery = db.setting_bool("discovery").unwrap_or(false);
    // Sync and audio stay available. Exposure decides which songs leave this computer,
    // and Auto send decides which of your edits are pushed out.
    (incognito, discovery, true, true)
}

fn relay_ready(db: &Db) -> bool {
    let (incognito, _, sync, audio) = switches(db);
    if incognito || (!sync && !audio) {
        return false;
    }
    let host = db.setting_string("relayHost").unwrap_or_default();
    let id = db.setting_string("relayDeviceId").unwrap_or_default();
    let token = db.setting_string("relayToken").unwrap_or_default();
    !host.is_empty() && !id.is_empty() && !token.is_empty()
}

async fn restart_listener(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    listener: &mut Option<tokio::task::JoinHandle<()>>,
    actual_port: &mut u16,
    live: &Live,
) {
    let ((incognito, discovery, sync, audio), port) = {
        let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
        let switches = switches(&guard);
        let port = guard.setting_i64("listenPort").unwrap_or(0).clamp(0, u16::MAX as i64) as u16;
        (switches, port)
    };
    let wanted = !incognito && (discovery || sync || audio);
    // Keep a healthy listener so the announced port stays valid across settings changes.
    if wanted
        && *actual_port != 0
        && (port == 0 || port == *actual_port)
        && listener.as_ref().is_some_and(|task| !task.is_finished())
    {
        return;
    }
    if let Some(task) = listener.take() {
        task.abort();
        // Wait until the old socket is really closed, otherwise a fixed port is still busy.
        let _ = task.await;
    }
    if !wanted {
        *actual_port = 0;
        if let Ok(mut live) = live.inner.lock() {
            live.listen_port = 0;
            live.port_fallback = None;
        }
        return;
    }
    let mut fell_back = false;
    let bind = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await {
        Ok(tcp) => Ok(tcp),
        Err(_) if port != 0 => {
            fell_back = true;
            TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await
        }
        Err(err) => Err(err),
    };
    let Ok(tcp) = bind else {
        *actual_port = 0;
        if let Ok(mut live) = live.inner.lock() {
            live.listen_port = 0;
            live.port_fallback = None;
        }
        return;
    };
    *actual_port = tcp.local_addr().map(|addr| addr.port()).unwrap_or(0);
    if port == 0 && *actual_port != 0 {
        if let Ok(guard) = db.lock() {
            let _ = guard.set_device_setting("listenPort", json!(*actual_port));
        }
    }
    if let Ok(mut live) = live.inner.lock() {
        live.listen_port = *actual_port;
        live.port_fallback = (fell_back && *actual_port != 0 && *actual_port != port).then_some(port);
    }
    let events = events.clone();
    let db = db.clone();
    let material = material.clone_parts();
    let provider = provider.clone();
    *listener = Some(tokio::task::spawn_local(async move {
        // Anyone on the network can connect, so only a few handshakes run at once and each one is timed.
        let handshakes = Arc::new(tokio::sync::Semaphore::new(MAX_HANDSHAKES));
        loop {
            let stream = match tcp.accept().await {
                Ok((stream, _)) => stream,
                Err(_) => {
                    // For example too many open sockets; wait instead of spinning.
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };
            let Ok(permit) = handshakes.clone().try_acquire_owned() else {
                drop(stream);
                continue;
            };
            let events = events.clone();
            let db = db.clone();
            let material = material.clone();
            let provider = provider.clone();
            tokio::task::spawn_local(async move {
                let _ = tokio::time::timeout(HANDSHAKE_TIMEOUT, accept_lan(stream, db, material, provider, events)).await;
                drop(permit);
            });
        }
    }));
}

/// Handshakes from unknown devices that may run at the same time.
const MAX_HANDSHAKES: usize = 16;
/// Time allowed for TCP, relay and TLS setup before a connection is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Both sides send a keepalive every 45 seconds; a connection that stays silent longer is gone.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(150);
/// Unconfirmed devices that may wait for an answer at the same time.
const MAX_UNCONFIRMED: usize = 8;

async fn accept_lan(
    stream: TcpStream,
    db: Arc<Mutex<Db>>,
    material: IdentityParts,
    provider: Arc<rustls::crypto::CryptoProvider>,
    events: mpsc::UnboundedSender<NetEvent>,
) -> AppResult<()> {
    let verifier = Arc::new(tls::IdentityVerifier::peer(provider.clone(), None));
    let config = tls::server_config(provider, &material.cert_der, &material.cert_key, verifier)
        .map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let tls = acceptor.accept(stream).await.map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    let certs = tls.get_ref().1.peer_certificates().map(|certs| certs.to_vec()).unwrap_or_default();
    let dialed_by = material.device_id.clone();
    finish_handshake(tls, certs, db, material, events, "lan".into(), dialed_by, true, None).await
}

async fn dial_peer(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    hosts: Vec<String>,
    port: u16,
    expected_device: Option<String>,
    dialed_by: &str,
) {
    let events = events.clone();
    let db = db.clone();
    let parts = material.clone_parts();
    let provider = provider.clone();
    let dialed_by = dialed_by.to_string();
    tokio::task::spawn_local(async move {
        let mut target = format!("{}:{port}", hosts.first().cloned().unwrap_or_default());
        let failed = |events: &mpsc::UnboundedSender<NetEvent>, target: &str| {
            let _ = events.send(NetEvent::DialFailed { device_id: expected_device.clone(), target: Some(target.to_string()) });
        };
        // The device may be reachable on only one of its addresses, so try them in order.
        let mut connected = None;
        for host in &hosts {
            target = format!("{host}:{port}");
            let connect = TcpStream::connect((host.as_str(), port));
            if let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_millis(2500), connect).await {
                connected = Some(stream);
                break;
            }
        }
        let Some(stream) = connected else { return failed(&events, &target) };
        let failed = |events: &mpsc::UnboundedSender<NetEvent>| failed(events, &target);
        let expected = expected_device.as_ref().and_then(|id| ids::identity_bytes_from_app_id(id));
        let verifier = Arc::new(tls::IdentityVerifier::peer(provider.clone(), expected));
        let config = match tls::client_config(provider, &parts.cert_der, &parts.cert_key, verifier, None) {
            Ok(config) => config,
            Err(_) => return failed(&events),
        };
        let connector = TlsConnector::from(Arc::new(config));
        let name = rustls::pki_types::ServerName::try_from(crate::identity::TLS_DNS_NAME).expect("name");
        let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(5), connector.connect(name, stream)).await else {
            return failed(&events);
        };
        let certs = tls.get_ref().1.peer_certificates().map(|certs| certs.to_vec()).unwrap_or_default();
        if finish_handshake(tls, certs, db, parts, events.clone(), "lan".into(), dialed_by, false, Some(target.clone())).await.is_err() {
            failed(&events);
        }
    });
}

async fn finish_handshake<S>(
    stream: S,
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    db: Arc<Mutex<Db>>,
    material: IdentityParts,
    events: mpsc::UnboundedSender<NetEvent>,
    path: String,
    dialed_by: String,
    incoming: bool,
    target: Option<String>,
) -> AppResult<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + 'static,
{
    let peer_cert = certs.first().ok_or_else(|| crate::error::AppError::Message("missing peer cert".into()))?;
    let identity_public = identity_of(peer_cert.as_ref())?;
    if !identity::certificate_binds(peer_cert.as_ref(), &identity_public)? {
        return Err(crate::error::AppError::Message("bad peer cert".into()));
    }
    let device_id = ids::app_device_id(&identity_public);
    // On a connection we accepted, the other device is the one that dialed. Both sides must agree on this
    // so that, when two devices dial each other at once, they keep the same connection.
    // An empty value marks a relay session where the other device is the TLS client, so both sides
    // name the same dialer and keep the same connection.
    let dialed_by = if incoming || dialed_by.is_empty() { device_id.clone() } else { dialed_by };
    let refuse = db.lock().ok().is_some_and(|guard| {
        guard.is_blocked(&device_id).unwrap_or(false) || (incoming && guard.silence_active(&device_id))
    });
    if refuse {
        return Err(crate::error::AppError::Message("unreachable".into()));
    }
    let conn = rand::random::<u64>();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (audio_tx, mut audio_rx) = mpsc::channel::<Frame>(48);
    let _ = events.send(NetEvent::Connected {
        device_id: device_id.clone(),
        conn,
        target,
        dialed_by,
        path,
        writer: tx,
        audio: audio_tx,
        identity_public,
        cert_der: peer_cert.as_ref().to_vec(),
        name: device_id.clone(),
        incoming,
    });
    let (mut reader, mut writer) = tokio::io::split(stream);
    let events_read = events.clone();
    let device_read = device_id.clone();
    // Dropped when the writer stops (connection not used, replaced or removed), so the reader stops too.
    let (writer_done, mut stop_reading) = tokio::sync::oneshot::channel::<()>();
    tokio::task::spawn_local(async move {
        loop {
            let next = tokio::select! {
                _ = &mut stop_reading => break,
                next = tokio::time::timeout(READ_IDLE_TIMEOUT, frames::read_frame(&mut reader)) => next,
            };
            match next {
                Ok(Ok(frame)) => {
                    if events_read.send(NetEvent::Frame { device_id: device_read.clone(), frame }).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
        let _ = events_read.send(NetEvent::Closed { device_id: device_read, conn });
    });
    tokio::task::spawn_local(async move {
        let _writer_done = writer_done;
        let mut interval = tokio::time::interval(Duration::from_secs(45));
        loop {
            tokio::select! {
                biased;
                frame = rx.recv() => {
                    let Some(frame) = frame else { break };
                    if write_or_stop(&mut writer, &frame).await {
                        break;
                    }
                }
                Some(frame) = audio_rx.recv() => {
                    if write_or_stop(&mut writer, &frame).await {
                        break;
                    }
                }
                _ = interval.tick() => {
                    if frames::write_frame(&mut writer, &Frame::Keepalive).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = writer.shutdown().await;
    });
    let _ = material;
    Ok(())
}

fn identity_of(cert_der: &[u8]) -> AppResult<Vec<u8>> {
    let (_, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    for extension in cert.extensions() {
        if extension.oid.to_id_string() == "1.3.6.1.4.1.55529.1" {
            return Ok(extension.value.to_vec());
        }
    }
    Err(crate::error::AppError::Message("no identity".into()))
}

fn adopt_connection(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    live: &Live,
    peers: &mut HashMap<String, PeerOut>,
    pending: &HashSet<String>,
    app: &AppHandle,
    device_id: String,
    conn: u64,
    dialed_by: String,
    incoming: bool,
    path: String,
    writer: mpsc::UnboundedSender<Frame>,
    audio: mpsc::Sender<Frame>,
    identity_public: Vec<u8>,
    cert_der: Vec<u8>,
    name: String,
) {
    let known = db.lock().ok().and_then(|guard| {
        if guard.is_blocked(&device_id).unwrap_or(false) {
            return None;
        }
        guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id)
    });
    let blocked = db.lock().ok().is_some_and(|guard| guard.is_blocked(&device_id).unwrap_or(false));
    if blocked {
        return;
    }
    if live.inner.lock().ok().is_some_and(|guard| guard.paused.contains(&device_id)) {
        let _ = writer.send(Frame::Hold);
        return;
    }
    let waiting = pending.contains(&device_id);
    // First contact is only accepted on a connection the other device opened, or one we dialed on purpose.
    if known.is_none() && !incoming && !waiting {
        return;
    }
    // Strangers cannot fill the device list: only a few unconfirmed devices wait at once.
    if known.is_none()
        && !waiting
        && !peers.contains_key(&device_id)
        && peers.values().filter(|peer| !peer.confirmed).count() >= MAX_UNCONFIRMED
    {
        return;
    }
    if let Some(peer) = &known {
        if peer.identity_public != identity_public {
            return;
        }
    }
    // A paired device keeps its saved name; an unauthenticated announce must not rename it.
    let saved_name = known.as_ref().map(|peer| peer.display_name.clone()).filter(|text| !text.is_empty() && *text != device_id);
    let name = saved_name.unwrap_or_else(|| {
        live.inner
            .lock()
            .ok()
            .and_then(|guard| guard.nearby.iter().find(|item| item.device_id == device_id).map(|item| item.name.clone()))
            .filter(|text| !text.is_empty())
            .unwrap_or(name)
    });
    if let Some(existing) = peers.get(&device_id) {
        let smaller = smaller_device(&material.device_id, &device_id);
        let preferred = match smaller {
            std::cmp::Ordering::Less => dialed_by == material.device_id,
            _ => dialed_by == device_id,
        };
        let replaces_same_side = existing.dialed_by == dialed_by;
        if !preferred && !replaces_same_side {
            return;
        }
    }
    let confirmed = known.as_ref().is_some_and(|peer| peer.trust != "verify");
    peers.insert(device_id.clone(), PeerOut {
        device_id: device_id.clone(),
        conn,
        dialed_by,
        confirmed,
        incoming,
        path: path.clone(),
        tx: writer,
        audio,
        alive: Arc::new(std::sync::atomic::AtomicBool::new(true)),
    });
    if confirmed {
        let shown = name.clone();
        if let Ok(guard) = db.lock() {
            let _ = guard.add_notice("connected", &device_id, &shown, &path);
        }
    } else {
        // The other device opened this connection. Keep that request; accepting or declining it is local and is not logged.
        if incoming
            && let Ok(guard) = db.lock()
        {
            let _ = guard.add_notice("request", &device_id, &name, &path);
        }
        // An unasked request does not replace the pairing question already on screen for another device.
        let busy = incoming
            && !waiting
            && live.inner.lock().ok().is_some_and(|guard| guard.offer.as_ref().is_some_and(|offer| offer.device_id != device_id));
        if !busy {
            stage_offer(db, material, live, &identity_public, &cert_der, &name, if incoming { "incoming" } else { "outgoing" }, app);
        }
    }
}

fn stage_offer(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    live: &Live,
    identity_public: &[u8],
    cert_der: &[u8],
    name: &str,
    role: &str,
    app: &AppHandle,
) {
    let Ok(remote_key) = libsignal_protocol::IdentityKey::decode(identity_public) else { return };
    let number = identity::safety_number(material.identity.identity_key(), &remote_key).unwrap_or_default();
    let device_id = ids::app_device_id(identity_public);
    if let Ok(mut guard) = live.inner.lock() {
        let shown = guard
            .nearby
            .iter()
            .find(|item| item.device_id == device_id)
            .map(|item| item.name.clone())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| if name.is_empty() || name == device_id { short_id(&device_id) } else { name.to_string() });
        guard.offer = Some(PairingOffer {
            device_id,
            name: shown,
            safety_number: number,
            cert_der: cert_der.to_vec(),
            identity_public: identity_public.to_vec(),
            cert_id: ids::cert_id_display(cert_der),
            role: role.to_string(),
        });
    }
    let _ = db;
    let _ = app.emit("network-updated", json!({}));
}

fn confirm_peer(
    db: &Arc<Mutex<Db>>,
    _material: &IdentityMaterial,
    live: &Live,
    peers: &mut HashMap<String, PeerOut>,
    device_id: &str,
    trust: &str,
) -> bool {
    let offer = live.inner.lock().ok().and_then(|guard| guard.offer.clone());
    let Some(offer) = offer.filter(|offer| offer.device_id == device_id && offer.role == "incoming") else { return false };
    let number = offer.safety_number.clone();
    let trust = if trust == "verify" { "verify" } else { "full" };
    if let Ok(guard) = db.lock() {
        let _ = guard.upsert_peer(&crate::db::PeerRow {
            device_id: offer.device_id.clone(),
            identity_public: offer.identity_public,
            cert_der: offer.cert_der,
            cert_id: offer.cert_id,
            display_name: offer.name.clone(),
            safety_number: number,
            trust: trust.to_string(),
        });
    }
    if let Some(peer) = peers.get_mut(device_id) {
        peer.confirmed = true;
    }
    if let Ok(mut guard) = live.inner.lock() {
        guard.offer = None;
    }
    true
}

/// The device we dialed has accepted. Save the identity it already presented.
fn accept_remote_pair(db: &Arc<Mutex<Db>>, live: &Live, peers: &mut HashMap<String, PeerOut>, device_id: &str) -> bool {
    let offer = live.inner.lock().ok().and_then(|guard| guard.offer.clone());
    let Some(offer) = offer.filter(|offer| offer.device_id == device_id && offer.role == "outgoing") else { return false };
    if let Ok(guard) = db.lock() {
        let _ = guard.upsert_peer(&crate::db::PeerRow {
            device_id: offer.device_id.clone(),
            identity_public: offer.identity_public,
            cert_der: offer.cert_der,
            cert_id: offer.cert_id,
            display_name: offer.name.clone(),
            safety_number: offer.safety_number,
            trust: "full".into(),
        });
        let _ = guard.add_notice("paired", &offer.device_id, &offer.name, "full");
    }
    if let Some(peer) = peers.get_mut(device_id) {
        peer.confirmed = true;
    }
    if let Ok(mut guard) = live.inner.lock() {
        guard.offer = None;
        guard.failures.remove(device_id);
    }
    true
}

fn block_name(db: &Arc<Mutex<Db>>, live: &Live, device_id: &str) -> String {
    if let Ok(guard) = db.lock() {
        if let Some(name) = guard.blocks().ok().into_iter().flatten().find(|row| row.device_id == device_id).map(|row| row.name) {
            if !name.is_empty() {
                return name;
            }
        }
        if let Some(name) = guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id).map(|peer| peer.display_name) {
            if !name.is_empty() && name != device_id {
                return name;
            }
        }
    }
    live.inner
        .lock()
        .ok()
        .and_then(|guard| guard.nearby.iter().find(|item| item.device_id == device_id).map(|item| item.name.clone()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| short_id(device_id))
}

fn short_id(device_id: &str) -> String {
    device_id.chars().take(7).collect()
}

/// Paired devices are listed even while offline. Unconfirmed connections show as pending.
fn enqueue_overwrite(live: &Live, device_id: &str, name: &str, feature: &str, items: Value) -> bool {
    let Ok(mut guard) = live.inner.lock() else { return false };
    if guard.overwrite_waiting.len() + guard.overwrite_shown.len() >= 20 {
        return false;
    }
    guard.overwrite_next += 1;
    let id = guard.overwrite_next;
    guard.overwrite_waiting.push_back(OverwriteAsk {
        id,
        device_id: device_id.to_string(),
        name: name.to_string(),
        feature: feature.to_string(),
        items,
        shown: None,
    });
    promote_overwrites(&mut guard);
    true
}

fn promote_overwrites(guard: &mut LiveInner) {
    while guard.overwrite_shown.len() < 3 {
        let Some(mut ask) = guard.overwrite_waiting.pop_front() else { break };
        ask.shown = Some(Instant::now());
        guard.overwrite_shown.push(ask);
    }
}

fn emit_overwrite_asks(app: &AppHandle, live: &Live) {
    let rows = live.inner.lock().map(|guard| {
        guard.overwrite_shown.iter().map(|ask| {
            let remaining = ask.shown.map(|start| 60u64.saturating_sub(start.elapsed().as_secs())).unwrap_or(60);
            json!({"id": ask.id, "deviceId": ask.device_id, "name": ask.name, "feature": ask.feature, "remaining": remaining})
        }).collect::<Vec<_>>()
    }).unwrap_or_default();
    let _ = app.emit("overwrite-asks", json!(rows));
}

fn expire_overwrites(app: &AppHandle, db: &Arc<Mutex<Db>>, live: &Live) {
    let expired = {
        let Ok(mut guard) = live.inner.lock() else { return };
        if guard.overwrite_shown.is_empty() && guard.overwrite_waiting.is_empty() {
            return;
        }
        let mut expired = Vec::new();
        guard.overwrite_shown.retain(|ask| {
            let timed_out = ask.shown.is_some_and(|start| start.elapsed() >= Duration::from_secs(60));
            if timed_out {
                expired.push(ask.clone());
            }
            !timed_out
        });
        let changed = !expired.is_empty();
        promote_overwrites(&mut guard);
        if !changed && expired.is_empty() {
            // Still refresh the countdown while a prompt is on screen.
        }
        expired
    };
    if let Ok(guard) = db.lock() {
        for ask in &expired {
            let _ = guard.add_notice("overwrite", &ask.device_id, &ask.name, &format!("{}:timeout", ask.feature));
        }
    }
    if !expired.is_empty() {
        let _ = app.emit("network-updated", json!({}));
    }
    emit_overwrite_asks(app, live);
}

fn resolve_overwrite(app: &AppHandle, db: &Arc<Mutex<Db>>, live: &Live, id: u64, accept: bool) {
    let ask = {
        let Ok(mut guard) = live.inner.lock() else { return };
        let Some(index) = guard.overwrite_shown.iter().position(|ask| ask.id == id) else { return };
        let ask = guard.overwrite_shown.remove(index);
        promote_overwrites(&mut guard);
        ask
    };
    if let Ok(guard) = db.lock() {
        let accept = accept && !guard.is_blocked(&ask.device_id).unwrap_or(true);
        if accept {
            apply_snapshot_payload(&guard, &ask.feature, &ask.items);
            sync_appearance_window(app, &guard, &ask.feature);
            let _ = guard.mark_dirty(&ask.feature, false);
            let _ = guard.add_notice("sync", &ask.device_id, &ask.name, &ask.feature);
        } else {
            let _ = guard.add_notice("overwrite", &ask.device_id, &ask.name, &format!("{}:cancel", ask.feature));
        }
    }
    let _ = app.emit("library-updated", json!({}));
    let _ = app.emit("network-updated", json!({}));
    emit_overwrite_asks(app, live);
}

fn publish(app: &AppHandle, db: &Arc<Mutex<Db>>, live: &Live, peers: &HashMap<String, PeerOut>) {
    let saved = db.lock().ok().and_then(|guard| {
        let blocked: HashSet<String> = guard.blocks().unwrap_or_default().into_iter().map(|row| row.device_id).collect();
        let saved = guard.peers().ok().map(|rows| rows.into_iter().filter(|peer| !blocked.contains(&peer.device_id)).collect::<Vec<_>>())?;
        Some(saved)
    }).unwrap_or_default();
    if let Ok(mut guard) = live.inner.lock() {
        let mut links: Vec<LinkStatus> = saved
            .iter()
            .map(|peer| {
                let connected = peers.get(&peer.device_id);
                let state = match connected {
                    Some(out) if out.confirmed => "online",
                    Some(_) => "pending",
                    None if guard.failures.contains_key(&peer.device_id) => "failed",
                    None => "offline",
                };
                LinkStatus {
                    device_id: peer.device_id.clone(),
                    name: if peer.display_name.is_empty() || peer.display_name == peer.device_id {
                        short_id(&peer.device_id)
                    } else {
                        peer.display_name.clone()
                    },
                    state: state.into(),
                    path: connected.map(|out| out.path.clone()).unwrap_or_default(),
                    detail: connected
                        .map(|out| if out.dialed_by == peer.device_id { "incoming".to_string() } else { "outgoing".to_string() })
                        .unwrap_or_default(),
                    trust: if peer.trust == "verify" { "verify".into() } else { "full".into() },
                }
            })
            .collect();
        for out in peers.values().filter(|out| !saved.iter().any(|peer| peer.device_id == out.device_id)) {
            links.push(LinkStatus {
                device_id: out.device_id.clone(),
                name: short_id(&out.device_id),
                state: "pending".into(),
                path: out.path.clone(),
                detail: String::new(),
                trust: "full".into(),
            });
        }
        links.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        guard.links = links;
    }
    let _ = app.emit("network-updated", json!({}));
}

/// Saved devices to dial again: not connected now and not held off by a manual Disconnect.
/// A manual Disconnect holds until the user presses Connect (or the app restarts).
fn redial_ids(saved: Vec<String>, connected: impl Fn(&str) -> bool, paused: &HashSet<String>) -> Vec<String> {
    saved.into_iter().filter(|id| !connected(id) && !paused.contains(id)).collect()
}

impl LiveInner {
    /// Devices automatic redial leaves alone: disconnected here, or disconnected by the other side.
    fn on_hold(&self) -> HashSet<String> {
        self.paused.union(&self.held).cloned().collect()
    }
}

fn nearby_of(live: &Live, device_id: &str) -> Option<Nearby> {
    live.inner.lock().ok()?.nearby.iter().find(|item| item.device_id == device_id).cloned()
}

fn set_relay(live: &Live, text: &str) {
    if let Ok(mut guard) = live.inner.lock() {
        guard.relay = text.to_string();
    }
}

async fn restart_discovery(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    port: u16,
    events: &mpsc::UnboundedSender<NetEvent>,
    discovery: &mut Option<std::sync::mpsc::Sender<()>>,
    live: &Live,
) {
    if let Some(stop) = discovery.take() {
        let _ = stop.send(());
    }
    let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
    let (incognito, enabled, _, _) = switches(&guard);
    let name = guard.setting_string("displayName").unwrap_or_else(|_| "Device".into());
    drop(guard);
    if incognito || !enabled || port == 0 {
        if let Ok(mut guard) = live.inner.lock() {
            guard.nearby.clear();
        }
        return;
    }
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let events = events.clone();
    let device_id = material.device_id.clone();
    let _ = std::thread::Builder::new().name("lan-discovery".into()).spawn(move || {
        if let Err(err) = run_discovery(stop_rx, events, device_id, name, port) {
            eprintln!("discovery: {err}");
        }
    });
    *discovery = Some(stop_tx);
}

const DISCOVERY_PORT: u16 = 41220;
const DISCOVERY_MAX_DEVICES: usize = 64;
const DISCOVERY_NETS_REFRESH: Duration = Duration::from_secs(5);
const DISCOVERY_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 168);

#[derive(serde::Serialize, serde::Deserialize)]
struct Announce {
    v: u8,
    id: String,
    name: String,
    port: u16,
    announce: bool,
    /// IPv4 addresses of the sender that may accept connections.
    #[serde(default)]
    addrs: Vec<String>,
}

struct Heard {
    name: String,
    port: u16,
    /// Source address of each packet that arrived, with when it was last heard.
    sources: HashMap<String, Instant>,
    announced: Vec<String>,
    seen: Instant,
}

fn heard_nearby(id: &str, item: &Heard, nets: &[(Ipv4Addr, Ipv4Addr)]) -> Nearby {
    let now = Instant::now();
    let mut hosts: Vec<(u8, String)> = Vec::new();
    let mut add = |text: &str, heard: bool| {
        let Ok(ip) = text.parse::<Ipv4Addr>() else { return };
        if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() || ip.is_multicast() || hosts.iter().any(|(_, known)| known == text) {
            return;
        }
        let shared = nets.iter().any(|(local, mask)| u32::from(*local) & u32::from(*mask) == u32::from(ip) & u32::from(*mask));
        // A packet is unauthenticated, so an address it only claims is dialed only when it is on our own subnet.
        if !heard && !shared {
            return;
        }
        // Same subnet as one of our own interfaces first, then addresses we actually heard from.
        let rank = match (shared, heard) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        hosts.push((rank, text.to_string()));
    };
    for (source, at) in &item.sources {
        if now.duration_since(*at) < Duration::from_secs(12) {
            add(source, true);
        }
    }
    for address in &item.announced {
        add(address, false);
    }
    hosts.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let hosts: Vec<String> = hosts.into_iter().map(|(_, text)| text).collect();
    Nearby {
        device_id: id.to_string(),
        name: item.name.clone(),
        host: hosts.first().cloned().unwrap_or_default(),
        hosts,
        port: item.port,
    }
}

/// Announces repeat every few seconds; only a real change reaches the app, so screens are not refreshed for nothing.
fn send_nearby(events: &mpsc::UnboundedSender<NetEvent>, found: &HashMap<String, Heard>, nets: &[(Ipv4Addr, Ipv4Addr)], last_sent: &mut String) {
    let list = nearby_list(found, nets);
    let text = serde_json::to_string(&list).unwrap_or_default();
    if text != *last_sent {
        *last_sent = text;
        let _ = events.send(NetEvent::Nearby(list));
    }
}

fn nearby_list(found: &HashMap<String, Heard>, nets: &[(Ipv4Addr, Ipv4Addr)]) -> Vec<Nearby> {
    let mut list: Vec<Nearby> = found.iter().map(|(id, item)| heard_nearby(id, item, nets)).filter(|item| !item.hosts.is_empty()).collect();
    list.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.device_id.cmp(&right.device_id)));
    list
}

fn run_discovery(
    stop: std::sync::mpsc::Receiver<()>,
    events: mpsc::UnboundedSender<NetEvent>,
    device_id: String,
    name: String,
    listen_port: u16,
) -> std::io::Result<()> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&std::net::SocketAddr::from((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())?;
    socket.set_multicast_loop_v4(true)?;
    socket.set_multicast_ttl_v4(255)?;
    let _ = socket.set_broadcast(true);
    let mut joined: std::collections::HashSet<Ipv4Addr> = std::collections::HashSet::new();
    let group = std::net::SocketAddrV4::new(DISCOVERY_GROUP, DISCOVERY_PORT);
    let mut found: HashMap<String, Heard> = HashMap::new();
    let mut last_sent = String::new();
    let mut last_announce = Instant::now() - Duration::from_secs(30);
    let mut buf = [std::mem::MaybeUninit::<u8>::uninit(); 2048];
    // Reading the adapters is slow on Windows, so the list is refreshed every few seconds, not per packet.
    let mut nets = lan_nets();
    let mut nets_read = Instant::now();
    let mut replied: HashMap<std::net::IpAddr, Instant> = HashMap::new();
    loop {
        if stop.try_recv() != Err(std::sync::mpsc::TryRecvError::Empty) {
            break;
        }
        if nets_read.elapsed() >= DISCOVERY_NETS_REFRESH {
            nets = lan_nets();
            nets_read = Instant::now();
        }
        let local: Vec<Ipv4Addr> = nets.iter().map(|(ip, _)| *ip).collect();
        for ip in &local {
            if joined.insert(*ip) {
                let _ = socket.join_multicast_v4(&DISCOVERY_GROUP, ip);
            }
        }
        socket.set_read_timeout(Some(Duration::from_millis(400)))?;
        match socket.recv_from(&mut buf) {
            Ok((size, src)) => {
                let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, size) };
                let parsed = src.as_socket().and_then(|addr| read_announce(bytes, addr, &device_id));
                // Packets are unauthenticated: a flood of made-up ids must not grow the list without end.
                let room = parsed.as_ref().is_some_and(|peer| found.contains_key(&peer.id) || found.len() < DISCOVERY_MAX_DEVICES);
                if let Some(peer) = parsed.filter(|_| room) {
                    let now = Instant::now();
                    // One answer per sender every second is enough; announces repeat every three seconds.
                    let reply = peer.announce
                        && src.as_socket().is_some_and(|addr| {
                            replied.retain(|_, at| now.duration_since(*at) < Duration::from_secs(1));
                            replied.len() < DISCOVERY_MAX_DEVICES && replied.insert(addr.ip(), now).is_none()
                        });
                    let entry = found.entry(peer.id.clone()).or_insert_with(|| Heard {
                        name: String::new(),
                        port: peer.port,
                        sources: HashMap::new(),
                        announced: Vec::new(),
                        seen: now,
                    });
                    entry.name = peer.name;
                    entry.port = peer.port;
                    entry.announced = peer.addrs;
                    entry.sources.insert(peer.host, now);
                    entry.sources.retain(|_, at| now.duration_since(*at) < Duration::from_secs(12));
                    entry.seen = now;
                    send_nearby(&events, &found, &nets, &mut last_sent);
                    if reply {
                        let packet = announce_packet(&device_id, &name, listen_port, false, &local);
                        let _ = socket.send_to(&packet, &src);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut || err.kind() == std::io::ErrorKind::WouldBlock => {}
            // Windows reports ICMP "port unreachable" from an earlier reply as ConnectionReset; keep running.
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
        let now = Instant::now();
        let before = found.len();
        found.retain(|_, item| now.duration_since(item.seen) < Duration::from_secs(12));
        if found.len() != before {
            send_nearby(&events, &found, &nets, &mut last_sent);
        }
        if last_announce.elapsed() >= Duration::from_secs(3) {
            let packet = announce_packet(&device_id, &name, listen_port, true, &local);
            let target: socket2::SockAddr = std::net::SocketAddr::from(group).into();
            for ip in &local {
                if socket.set_multicast_if_v4(ip).is_ok() {
                    let _ = socket.send_to(&packet, &target);
                }
            }
            // Some routers drop multicast; subnet broadcast reaches those networks too.
            for broadcast in lan_broadcasts() {
                let to: socket2::SockAddr = std::net::SocketAddr::from((broadcast, DISCOVERY_PORT)).into();
                let _ = socket.send_to(&packet, &to);
            }
            last_announce = Instant::now();
        }
    }
    Ok(())
}

struct ParsedAnnounce {
    id: String,
    name: String,
    host: String,
    port: u16,
    announce: bool,
    addrs: Vec<String>,
}

fn announce_packet(id: &str, name: &str, port: u16, announce: bool, local: &[Ipv4Addr]) -> Vec<u8> {
    serde_json::to_vec(&Announce {
        v: 1,
        id: id.to_string(),
        name: txt_value(name),
        port,
        announce,
        addrs: local.iter().map(|ip| ip.to_string()).collect(),
    })
    .unwrap_or_default()
}

fn read_announce(bytes: &[u8], src: std::net::SocketAddr, self_id: &str) -> Option<ParsedAnnounce> {
    let std::net::SocketAddr::V4(src) = src else { return None };
    let ip = *src.ip();
    if ip.is_unspecified() || ip.is_multicast() || ip.is_loopback() {
        return None;
    }
    let message: Announce = serde_json::from_slice(bytes).ok()?;
    if message.v != 1 || message.id.is_empty() || message.id == self_id || message.port == 0 {
        return None;
    }
    if message.id.len() > 128 {
        return None;
    }
    Some(ParsedAnnounce {
        id: message.id,
        name: clean_name(&message.name, 64),
        host: ip.to_string(),
        port: message.port,
        announce: message.announce,
        addrs: message.addrs.into_iter().take(16).collect(),
    })
}

/// A device name from the network, without control or text-direction characters that could disguise it.
pub(crate) fn clean_name(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|ch| !ch.is_control() && !matches!(ch, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
        .collect::<String>()
        .trim()
        .chars()
        .take(max_chars)
        .collect()
}

fn txt_value(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.len() + ch.len_utf8() > 200 {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() { "Device".into() } else { out }
}

/// IPv4 addresses on networks this app may use, as (address, netmask).
/// Adapters on a Windows "Public" network are left out because the firewall blocks them.
fn lan_nets() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    let all = if_addrs::get_if_addrs().unwrap_or_default();
    let public = public_adapters();
    let usable = all
        .iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(ref addr) if lan_interface(&iface.name, addr.ip) && !public.contains(&iface.name.to_lowercase()) => Some((addr.ip, addr.netmask)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !usable.is_empty() {
        return usable;
    }
    // Only virtual-looking adapters are left: use them, but still never a Public network.
    all.into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(addr)
                if !addr.ip.is_loopback() && !addr.ip.is_link_local() && !addr.ip.is_unspecified() && !public.contains(&iface.name.to_lowercase()) =>
            {
                Some((addr.ip, addr.netmask))
            }
            _ => None,
        })
        .collect()
}

pub fn lan_ipv4() -> Vec<Ipv4Addr> {
    lan_nets().into_iter().map(|(ip, _)| ip).collect()
}

/// Names (lower case) of adapters whose Windows network profile is Public.
/// The answer is cached; a stale cache refreshes in the background.
fn public_adapters() -> std::collections::HashSet<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Mutex, OnceLock};
        type Cache = (Option<Instant>, std::collections::HashSet<String>);
        static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
        static BUSY: AtomicBool = AtomicBool::new(false);
        let cache = CACHE.get_or_init(|| Mutex::new((None, Default::default())));
        let fetch = || {
            // For each network: its category, and whether Windows Firewall lets this program in on that profile.
            // A block rule wins over an allow rule. A profile with the firewall switched off lets everything in.
            const SCRIPT: &str = r#"
$exe = $env:DM_EXE
$fw = @{}
Get-NetFirewallProfile | ForEach-Object { $fw[[string]$_.Name] = ([string]$_.Enabled -ne 'False') }
$rules = @(Get-NetFirewallApplicationFilter -Program $exe -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object { [string]$_.Enabled -eq 'True' -and [string]$_.Direction -eq 'Inbound' })
Get-NetConnectionProfile | ForEach-Object {
  $cat = [string]$_.NetworkCategory
  $pn = if ($cat -eq 'DomainAuthenticated') { 'Domain' } else { $cat }
  $hits = { param($r) $p = [string]$r.Profile; ($p -match 'Any') -or ($p -match $pn) }
  $block = @($rules | Where-Object { [string]$_.Action -eq 'Block' -and (& $hits $_) }).Count -gt 0
  $allow = @($rules | Where-Object { [string]$_.Action -eq 'Allow' -and (& $hits $_) }).Count -gt 0
  $ok = (-not $fw[$pn]) -or ($allow -and -not $block)
  $_.InterfaceAlias + '|' + $cat + '|' + $ok
}
"#;
            let exe = std::env::current_exe().map(|path| path.to_string_lossy().to_string()).unwrap_or_default();
            let output = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
                .env("DM_EXE", exe)
                .creation_flags(0x0800_0000)
                .output();
            let mut names = std::collections::HashSet::new();
            if let Ok(output) = output {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let mut parts = line.trim().rsplitn(3, '|');
                    let (Some(ok), Some(category), Some(alias)) = (parts.next(), parts.next(), parts.next()) else { continue };
                    // Only a Public network the firewall keeps this program out of is unusable.
                    if category.trim().eq_ignore_ascii_case("public") && ok.trim().eq_ignore_ascii_case("false") {
                        names.insert(alias.trim().to_lowercase());
                    }
                }
            }
            if let Ok(mut guard) = cache.lock() {
                *guard = (Some(Instant::now()), names);
            }
        };
        let (stamp, names) = cache.lock().map(|guard| guard.clone()).unwrap_or_default();
        match stamp {
            None => {
                if !BUSY.swap(true, Ordering::SeqCst) {
                    fetch();
                    BUSY.store(false, Ordering::SeqCst);
                }
                cache.lock().map(|guard| guard.1.clone()).unwrap_or_default()
            }
            Some(at) => {
                if at.elapsed() > Duration::from_secs(20) && !BUSY.swap(true, Ordering::SeqCst) {
                    std::thread::spawn(move || {
                        fetch();
                        BUSY.store(false, Ordering::SeqCst);
                    });
                }
                names
            }
        }
    }
    #[cfg(not(windows))]
    {
        Default::default()
    }
}

fn lan_broadcasts() -> Vec<Ipv4Addr> {
    let public = public_adapters();
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(ref addr) if lan_interface(&iface.name, addr.ip) && !public.contains(&iface.name.to_lowercase()) => addr.broadcast,
            _ => None,
        })
        .collect()
}

fn lan_interface(name: &str, ip: Ipv4Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
        return false;
    }
    let name = name.to_ascii_lowercase();
    const SKIP: &[&str] = &[
        "vethernet", "vmware", "virtualbox", "vbox", "hyper-v", "wsl", "docker", "loopback", "bluetooth", "npcap", "tap-windows", "tailscale", "zerotier", "wireguard", "hamachi",
    ];
    !SKIP.iter().any(|word| name.contains(word))
}

#[cfg(test)]
mod lan_tests {
    use super::{
        accept_holdings_part, accept_snapshot_part, clean_name, download_name, is_cover_image, history_part_messages, holdings_messages, lan_interface,
        port_setting, HoldingsBuild, HISTORY_PARTS_KEY, HOLDINGS_PART_BYTES, SNAPSHOT_PART_BYTES,
    };
    use crate::db::HoldingInput;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    #[test]
    fn download_names_are_safe_windows_file_names() {
        assert_eq!(download_name("a/b:c?", "mp3", "id"), "a_b_c_.mp3");
        assert_eq!(download_name("CON", "flac", "id"), "_CON.flac");
        assert_eq!(download_name("com1.live", "flac", "id"), "_com1.live.flac");
        assert_eq!(download_name("Lpt9 ", "flac", "id"), "_Lpt9.flac");
        assert_eq!(download_name("Console", "flac", "id"), "Console.flac");
        assert_eq!(download_name("song. . ", "mp3", "id"), "song.mp3");
        assert_eq!(download_name("..", "mp3", "abc"), "abc.mp3");
        assert_eq!(download_name("  ", "mp3", "../x"), ".._x.mp3");
        assert_eq!(download_name("", "mp3", ""), "track.mp3");
        assert_eq!(download_name("a\tb\u{0}", "mp3", "id"), "a_b_.mp3");
        let long = "长".repeat(300);
        assert_eq!(download_name(&long, "mp3", "id").chars().count(), 124);
    }

    #[test]
    fn only_picture_bytes_are_kept_as_covers() {
        assert!(is_cover_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10]));
        assert!(is_cover_image(b"\x89PNG\r\n\x1a\n\0\0"));
        assert!(!is_cover_image(b"<html>not a picture</html>"));
        assert!(!is_cover_image(b""));
    }

    #[test]
    fn long_history_is_sent_in_parts_and_rebuilt_in_order() {
        let entries: Vec<Value> = (0..3000)
            .map(|index| json!({"contentId": format!("{index:064}"), "at": "2026-10-28T00:00:00Z"}))
            .collect();
        let messages = history_part_messages(&json!(entries));
        assert!(messages.len() > 1);
        let count = messages.len();
        let mut pending = HashMap::new();
        let mut rebuilt = None;
        for (index, message) in messages.iter().enumerate() {
            assert_eq!(message["part"], json!(index));
            assert_eq!(message["parts"], json!(count));
            assert!(serde_json::to_vec(&message["entries"]).unwrap().len() <= SNAPSHOT_PART_BYTES);
            let done = accept_snapshot_part(&mut pending, "dev", HISTORY_PARTS_KEY, index, count, message["entries"].clone());
            assert_eq!(done.is_some(), index + 1 == count);
            rebuilt = rebuilt.or(done);
        }
        assert_eq!(rebuilt, Some(json!(entries)));
        assert!(pending.is_empty());
        let empty = history_part_messages(&json!([]));
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0]["entries"], json!([]));
    }

    #[test]
    fn history_parts_from_another_device_stay_apart() {
        let mut pending = HashMap::new();
        assert!(accept_snapshot_part(&mut pending, "a", HISTORY_PARTS_KEY, 0, 2, json!([1])).is_none());
        assert!(accept_snapshot_part(&mut pending, "b", HISTORY_PARTS_KEY, 1, 2, json!([9])).is_none());
        assert_eq!(accept_snapshot_part(&mut pending, "a", HISTORY_PARTS_KEY, 1, 2, json!([2])), Some(json!([1, 2])));
        assert!(accept_snapshot_part(&mut pending, "a", HISTORY_PARTS_KEY, 2, 2, json!([3])).is_none());
    }

    #[test]
    fn port_settings_outside_the_tcp_range_are_refused() {
        assert_eq!(port_setting(Some(22067)), Some(22067));
        assert_eq!(port_setting(Some(65535)), Some(65535));
        assert_eq!(port_setting(Some(0)), None);
        assert_eq!(port_setting(Some(-1)), None);
        assert_eq!(port_setting(Some(65536 + 80)), None);
        assert_eq!(port_setting(None), None);
    }

    #[test]
    fn network_names_lose_control_and_direction_characters() {
        assert_eq!(clean_name("  Living room  ", 64), "Living room");
        assert_eq!(clean_name("Bob\u{202E}gpj.exe", 64), "Bobgpj.exe");
        assert_eq!(clean_name("a\nb\u{0007}c\u{2066}d", 64), "abcd");
        assert_eq!(clean_name("音乐电脑", 2), "音乐");
        assert_eq!(clean_name("\u{200F}", 64), "");
    }

    #[test]
    fn discovery_skips_virtual_and_link_local_adapters() {
        assert!(lan_interface("Wi-Fi", "192.168.1.20".parse().unwrap()));
        assert!(lan_interface("Ethernet", "10.0.0.4".parse().unwrap()));
        assert!(!lan_interface("vEthernet (WSL)", "172.28.0.1".parse().unwrap()));
        assert!(!lan_interface("VMware Network Adapter VMnet8", "192.168.137.1".parse().unwrap()));
        assert!(!lan_interface("Ethernet", "169.254.8.1".parse().unwrap()));
    }

    fn test_store(name: &str) -> (crate::identity::IdentityMaterial, crate::identity::SignalStore) {
        let dir = std::env::temp_dir().join(format!("tunepeerly-net-test-{name}-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("dir");
        let db = crate::db::Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).expect("db");
        let material = crate::identity::load_or_create(&db).expect("identity");
        let store = crate::identity::SignalStore::open(std::sync::Arc::new(std::sync::Mutex::new(db)), material.identity, material.registration_id).expect("store");
        (material, store)
    }

    /// Both directions of the private channel and the group channel must open on the other side.
    #[tokio::test]
    async fn signal_frames_open_in_both_directions() {
        use crate::identity::SignalStore;
        use super::{decode_ciphertext, tag_ciphertext, tag_group, SIGNAL_GROUP};
        let (a_mat, mut a) = test_store("a");
        let (b_mat, mut b) = test_store("b");
        let a_addr = SignalStore::address_for(&a_mat.device_id);
        let b_addr = SignalStore::address_for(&b_mat.device_id);
        let mut rng = rand::rng();
        let b_bundle = b.ensure_prekeys().await.expect("bundle");
        let _ = a.ensure_prekeys().await.expect("bundle");
        a.trust(&b_mat.device_id, &b_mat.identity.identity_key().serialize());
        b.trust(&a_mat.device_id, &a_mat.identity.identity_key().serialize());
        libsignal_protocol::process_prekey_bundle(&b_addr, &a_addr, &mut a.clone(), &mut a.clone(), &b_bundle, std::time::SystemTime::now(), &mut rng)
            .await
            .expect("session");

        async fn open(store: &SignalStore, remote: &libsignal_protocol::ProtocolAddress, local: &libsignal_protocol::ProtocolAddress, frame: &[u8]) -> Vec<u8> {
            let (&kind, body) = frame.split_first().expect("kind");
            let message = decode_ciphertext(kind, body).expect("decodes");
            let mut rng = rand::rng();
            libsignal_protocol::message_decrypt(
                &message, remote, local,
                &mut store.clone(), &mut store.clone(), &mut store.clone(), &store.clone(), &mut store.clone(), &mut rng,
            ).await.expect("decrypts")
        }

        // A (the first to speak) -> B: a prekey message.
        let first = libsignal_protocol::message_encrypt(b"hello", &b_addr, &a_addr, &mut a.clone(), &mut a.clone(), std::time::SystemTime::now(), &mut rng).await.expect("encrypt");
        let frame = tag_ciphertext(&first);
        assert_eq!(open(&b, &a_addr, &b_addr, &frame).await, b"hello");

        // B -> A: an ordinary message that has no prekey part.
        let reply = libsignal_protocol::message_encrypt(b"reply", &a_addr, &b_addr, &mut b.clone(), &mut b.clone(), std::time::SystemTime::now(), &mut rng).await.expect("encrypt");
        let frame = tag_ciphertext(&reply);
        assert_eq!(open(&a, &b_addr, &a_addr, &frame).await, b"reply");

        // B's group channel: A learns B's sender key privately, then opens B's group message.
        let skdm = libsignal_protocol::create_sender_key_distribution_message(&b_addr, b_mat.distribution_id, &mut b.clone(), &mut rng).await.expect("skdm");
        libsignal_protocol::process_sender_key_distribution_message(&b_addr, &skdm, &mut a.clone()).await.expect("process");
        let group = libsignal_protocol::group_encrypt(&mut b.clone(), &b_addr, b_mat.distribution_id, b"holdings", &mut rng).await.expect("group");
        let frame = tag_group(group.serialized());
        assert_eq!(frame[0], SIGNAL_GROUP);
        let plain = libsignal_protocol::group_decrypt(&frame[1..], &mut a.clone(), &b_addr).await.expect("group opens");
        assert_eq!(plain, b"holdings");
    }

    fn row(n: usize) -> HoldingInput {
        HoldingInput {
            content_id: format!("content-{n:04}"),
            title: format!("Title {n} {}", "x".repeat(80)),
            artist: format!("Artist {n}"),
            album: format!("Album {n}"),
            track_number: Some(n as i64),
            duration_ms: Some(180_000),
            codec: "flac".into(),
        }
    }

    #[test]
    fn holdings_list_is_split_into_frames_and_reassembled_in_order() {
        let rows: Vec<HoldingInput> = (0..400).map(row).collect();
        let messages = holdings_messages(7, &rows);
        assert!(messages.len() > 1);
        for message in &messages {
            let bytes = serde_json::to_vec(message).expect("json");
            assert!(bytes.len() < HOLDINGS_PART_BYTES + 512);
            assert!(bytes.len() < crate::frames::MAX_PAYLOAD);
        }
        let mut pending: HashMap<String, HoldingsBuild> = HashMap::new();
        let mut done = None;
        for message in messages.iter().rev() {
            let part = message.get("part").and_then(|item| item.as_u64()).unwrap() as usize;
            let count = message.get("parts").and_then(|item| item.as_u64()).unwrap() as usize;
            let part_rows: Vec<HoldingInput> = serde_json::from_value(message.get("rows").cloned().unwrap()).unwrap();
            done = accept_holdings_part(&mut pending, "peer", 7, part, count, part_rows);
        }
        let done = done.expect("complete");
        assert_eq!(done.len(), rows.len());
        assert_eq!(done[0].content_id, "content-0000");
        assert_eq!(done.last().unwrap().content_id, "content-0399");
        assert!(pending.is_empty());
    }
}

async fn restart_relay(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    task: &mut Option<tokio::task::JoinHandle<()>>,
    live: &Live,
) {
    if let Some(task) = task.take() {
        task.abort();
    }
    let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
    if !relay_ready(&guard) {
        set_relay(live, "");
        return;
    }
    let host = guard.setting_string("relayHost").unwrap_or_default();
    let port = relay_port(&guard);
    let relay_id = guard.setting_string("relayDeviceId").unwrap_or_default();
    let token = guard.setting_string("relayToken").unwrap_or_default();
    drop(guard);
    let Some(expected) = ids::parse_cert_id(&relay_id) else {
        set_relay(live, "badId");
        return;
    };
    set_relay(live, "connecting");
    let events = events.clone();
    let material = material.clone_parts();
    let provider = provider.clone();
    *task = Some(tokio::task::spawn_local(async move {
        let mut wait = 5;
        loop {
            let _ = events.send(NetEvent::RelayState("connecting".into()));
            let started = Instant::now();
            let parts = material.clone();
            let result = relay_session(host.clone(), port, relay_id.clone(), expected, token.clone(), parts, provider.clone(), events.clone()).await;
            if let Err(err) = result {
                let _ = events.send(NetEvent::RelayState(format!("failed:{err}")));
            }
            if started.elapsed() > Duration::from_secs(120) {
                wait = 5;
            }
            tokio::time::sleep(Duration::from_secs(wait)).await;
            wait = (wait * 2).min(120);
        }
    }));
}

async fn relay_session(
    host: String,
    port: u16,
    relay_id: String,
    expected: [u8; 32],
    token: String,
    parts: IdentityParts,
    provider: Arc<rustls::crypto::CryptoProvider>,
    events: mpsc::UnboundedSender<NetEvent>,
) -> Result<(), String> {
    let setup = async {
        let addr = resolve_v4(&host, port).await?;
        let stream = TcpStream::connect(addr).await.map_err(|err| err.to_string())?;
        let verifier = Arc::new(tls::IdentityVerifier::relay(provider.clone(), expected));
        let config = tls::client_config(provider, &parts.cert_der, &parts.cert_key, verifier, Some(b"bep-relay"))
            .map_err(|err| err.to_string())?;
        let connector = TlsConnector::from(Arc::new(config));
        let name = rustls::pki_types::ServerName::try_from("relay.local").map_err(|err| err.to_string())?;
        connector.connect(name, stream).await.map_err(|err| err.to_string())
    };
    let mut tls = tokio::time::timeout(HANDSHAKE_TIMEOUT, setup).await.map_err(|_| "Connection timed out".to_string())??;
    let peer_certs = tls.get_ref().1.peer_certificates().map(|certs| certs.to_vec()).unwrap_or_default();
    let cert = peer_certs.first().ok_or("missing relay certificate")?;
    if !ids::same_cert_id(&relay_id, cert.as_ref()) || ids::cert_id_bytes(cert.as_ref()) != expected {
        return Err("relay certificate id does not match".into());
    }
    relay::write_message(&mut tls, 2, &relay::join_relay(&token)).await.map_err(|err| err.to_string())?;
    let (kind, body) = tokio::time::timeout(HANDSHAKE_TIMEOUT, relay::read_message(&mut tls))
        .await
        .map_err(|_| "Connection timed out".to_string())?
        .map_err(|err| err.to_string())?;
    if kind != 4 {
        return Err("unexpected relay response".into());
    }
    let (code, message) = relay::parse_response(&body).map_err(|err| err.to_string())?;
    if code != 0 && code != 2 {
        return Err(message);
    }
    let _ = events.send(NetEvent::RelayState("registered".into()));
    // Pings go out from their own task, so a half-read message is never cancelled by the timer.
    let (mut reader, mut writer) = tokio::io::split(tls);
    let _pinger = AbortOnDrop(tokio::task::spawn_local(async move {
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        loop {
            ping.tick().await;
            if relay::write_message(&mut writer, 0, &[]).await.is_err() {
                break;
            }
        }
    }));
    loop {
        // The relay answers our pings and pings us; long silence means the connection is gone.
        let (kind, body) = tokio::time::timeout(RELAY_IDLE_TIMEOUT, relay::read_message(&mut reader))
            .await
            .map_err(|_| "Connection timed out".to_string())?
            .map_err(|err| err.to_string())?;
        if kind == 1 {
            continue;
        }
        if kind == 6 {
            if let Ok(invitation) = relay::parse_invitation(&body) {
                let _ = events.send(NetEvent::Invitation(invitation));
            }
        }
    }
}

const RELAY_IDLE_TIMEOUT: Duration = Duration::from_secs(100);

/// Stops a helper task when its owner returns or is aborted.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A saved port setting, or `None` when it is not a usable TCP port.
fn port_setting(value: Option<i64>) -> Option<u16> {
    value.and_then(|port| u16::try_from(port).ok()).filter(|port| *port != 0)
}

fn relay_port(db: &Db) -> u16 {
    port_setting(db.setting_i64("relayPort").ok()).unwrap_or(22067)
}

async fn resolve_v4(host: &str, port: u16) -> Result<SocketAddr, String> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(SocketAddr::from((ip, port)));
    }
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        return Err("Connection failed".into());
    }
    let mut addrs = tokio::net::lookup_host((host, port)).await.map_err(|err| err.to_string())?;
    addrs.find(|addr| addr.is_ipv4()).ok_or_else(|| "Connection failed".into())
}

async fn maybe_relay_fallback(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    device_id: &str,
) {
    let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
    if !relay_ready(&guard) {
        return;
    }
    let Some(peer) = guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id && peer.trust != "verify" && !guard.is_blocked(&peer.device_id).unwrap_or(false)) else {
        return;
    };
    let host = guard.setting_string("relayHost").unwrap_or_default();
    let port = relay_port(&guard);
    let relay_id = guard.setting_string("relayDeviceId").unwrap_or_default();
    drop(guard);
    let Some(expected) = ids::parse_cert_id(&relay_id) else { return };
    let Some(target) = ids::parse_cert_id(&peer.cert_id) else { return };
    let parts = material.clone_parts();
    let provider = provider.clone();
    let events = events.clone();
    let failed_id = peer.device_id.clone();
    tokio::task::spawn_local(async move {
        let inner = request_relay_session(host, port, expected, target, parts, provider, events.clone());
        // The relay refused, or never answered: say so instead of leaving the device pending forever.
        let reason = match tokio::time::timeout(HANDSHAKE_TIMEOUT, inner).await {
            Ok(Ok(())) => return,
            Ok(Err(err)) => err,
            Err(_) => "the relay did not answer in time".to_string(),
        };
        eprintln!("relay session failed for {}: {reason}", short_id(&failed_id));
        let _ = events.send(NetEvent::DialFailed { device_id: Some(failed_id), target: None });
    });
}

async fn request_relay_session(
    host: String,
    port: u16,
    expected: [u8; 32],
    target: [u8; 32],
    parts: IdentityParts,
    provider: Arc<rustls::crypto::CryptoProvider>,
    events: mpsc::UnboundedSender<NetEvent>,
) -> Result<(), String> {
    let addr = resolve_v4(&host, port).await?;
    let stream = TcpStream::connect(addr).await.map_err(|err| err.to_string())?;
    let verifier = Arc::new(tls::IdentityVerifier::relay(provider.clone(), expected));
    let config = tls::client_config(provider, &parts.cert_der, &parts.cert_key, verifier, Some(b"bep-relay"))
        .map_err(|err| err.to_string())?;
    let mut tls = TlsConnector::from(Arc::new(config))
        .connect(rustls::pki_types::ServerName::try_from("relay.local").map_err(|err| err.to_string())?, stream)
        .await
        .map_err(|err| err.to_string())?;
    relay::write_message(&mut tls, 5, &relay::connect_request(&target)).await.map_err(|err| err.to_string())?;
    let (kind, body) = relay::read_message(&mut tls).await.map_err(|err| err.to_string())?;
    if kind != 6 {
        // The relay answered with a refusal (the target is not connected to it, for instance).
        return Err(match relay::parse_response(&body) {
            Ok((code, message)) if !message.is_empty() => format!("relay refused the session ({code}): {message}"),
            Ok((code, _)) => format!("relay refused the session ({code})"),
            Err(_) => format!("relay answered with kind {kind}"),
        });
    }
    let invitation = relay::parse_invitation(&body).map_err(|err| format!("relay invitation: {err}"))?;
    let _ = events.send(NetEvent::Invitation(invitation));
    Ok(())
}

async fn join_invited(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    invitation: relay::Invitation,
) {
    // The invitation names the device that asked for us by its relay certificate id.
    let inviter = <[u8; 32]>::try_from(invitation.from.as_slice())
        .ok()
        .map(|id| ids::cert_id_text(&id));
    let session = invited_session(db, material, provider, events, invitation);
    let reason = match tokio::time::timeout(HANDSHAKE_TIMEOUT, session).await {
        Ok(Ok(())) => return,
        Ok(Err(err)) => err,
        Err(_) => "the relay did not answer in time".to_string(),
    };
    eprintln!("relay session failed: {reason}");
    if let Some(device_id) = inviter {
        let _ = events.send(NetEvent::DialFailed { device_id: Some(device_id), target: None });
    }
}

async fn invited_session(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    provider: &Arc<rustls::crypto::CryptoProvider>,
    events: &mpsc::UnboundedSender<NetEvent>,
    invitation: relay::Invitation,
) -> Result<(), String> {
    let (host, port) = {
        let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
        (
            guard.setting_string("relayHost").unwrap_or_default(),
            relay_port(&guard),
        )
    };
    let fallback = resolve_v4(&host, port).await?;
    if invitation.from.is_empty() {
        return Err("relay invitation without a sender".into());
    }
    let target = relay::invitation_target(&invitation, match fallback.ip() {
        std::net::IpAddr::V4(ip) => ip,
        _ => return Err("relay invitation without an address".into()),
    }, fallback.port()).map_err(|err| err.to_string())?;
    let parts = material.clone_parts();
    let provider = provider.clone();
    let events = events.clone();
    let db = db.clone();
    let mut stream = TcpStream::connect(target).await.map_err(|err| err.to_string())?;
    relay::write_message(&mut stream, 3, &relay::join_session(&invitation.key)).await.map_err(|err| err.to_string())?;
    let (kind, body) = relay::read_message(&mut stream).await.map_err(|err| err.to_string())?;
    if kind != 4 {
        return Err(format!("relay session answered with kind {kind}"));
    }
    let (code, message) = relay::parse_response(&body).map_err(|err| err.to_string())?;
    if code != 0 {
        return Err(if message.is_empty() {
            format!("relay refused the session ({code})")
        } else {
            format!("relay refused the session ({code}): {message}")
        });
    }
    let expected = None;
    let verifier = Arc::new(tls::IdentityVerifier::peer(provider.clone(), expected));
    // The TLS client counts as the dialer on both sides; empty means "the other device".
    let dialed_by = if invitation.server_socket { String::new() } else { parts.device_id.clone() };
    if invitation.server_socket {
        let config = tls::server_config(provider, &parts.cert_der, &parts.cert_key, verifier).map_err(|err| err.to_string())?;
        let tls = tokio_rustls::TlsAcceptor::from(Arc::new(config)).accept(stream).await.map_err(|err| err.to_string())?;
        let certs = tls.get_ref().1.peer_certificates().map(|certs| certs.to_vec()).unwrap_or_default();
        finish_handshake(tls, certs, db, parts, events, "relay".into(), dialed_by, false, None).await.map_err(|err| err.to_string())?;
    } else {
        let config = tls::client_config(provider, &parts.cert_der, &parts.cert_key, verifier, None).map_err(|err| err.to_string())?;
        let name = rustls::pki_types::ServerName::try_from(crate::identity::TLS_DNS_NAME).map_err(|err| err.to_string())?;
        let tls = TlsConnector::from(Arc::new(config)).connect(name, stream).await.map_err(|err| err.to_string())?;
        let certs = tls.get_ref().1.peer_certificates().map(|certs| certs.to_vec()).unwrap_or_default();
        finish_handshake(tls, certs, db, parts, events, "relay".into(), dialed_by, false, None).await.map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn handle_frame(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
    bus: &AudioBus,
    ready_sessions: &mut HashSet<String>,
    pending_holdings: &mut HashMap<String, HoldingsBuild>,
    published_holdings: &mut String,
    device_id: &str,
    frame: Frame,
) {
    let Some(peer) = peers.get(device_id) else { return };
    if !peer.confirmed {
        return;
    }
    match frame {
        Frame::Keepalive | Frame::PairAccept | Frame::Hold => {}
        Frame::Bundle(bytes) => {
            if smaller_device(&material.device_id, device_id).is_lt() {
                if let Ok(bundle) = decode_bundle(&bytes) {
                    let remote = SignalStore::address_for(device_id);
                    let local = SignalStore::address_for(&material.device_id);
                    let mut sessions = store.clone();
                    let mut identities = store.clone();
                    let mut rng = rand::rng();
                    if libsignal_protocol::process_prekey_bundle(
                        &remote,
                        &local,
                        &mut sessions,
                        &mut identities,
                        &bundle,
                        std::time::SystemTime::now(),
                        &mut rng,
                    ).await.is_ok() {
                        session_ready(db, material, store, peers, ready_sessions, published_holdings, device_id).await;
                    }
                }
            }
        }
        Frame::Signal(bytes) => {
            // The first byte says which kind of message follows; libsignal does not mark it in its own bytes.
            let Some((&kind, body)) = bytes.split_first() else { return };
            let bytes = body.to_vec();
            if kind != SIGNAL_GROUP && let Some(message) = decode_ciphertext(kind, &bytes) {
                let remote = SignalStore::address_for(device_id);
                let local = SignalStore::address_for(&material.device_id);
                let mut sessions = store.clone();
                let mut identities = store.clone();
                let mut prekeys = store.clone();
                let signed = store.clone();
                let mut kyber = store.clone();
                let mut rng = rand::rng();
                if let Ok(plain) = libsignal_protocol::message_decrypt(
                    &message,
                    &remote,
                    &local,
                    &mut sessions,
                    &mut identities,
                    &mut prekeys,
                    &signed,
                    &mut kyber,
                    &mut rng,
                ).await {
                    if smaller_device(&material.device_id, device_id).is_gt() {
                        session_ready(db, material, store, peers, ready_sessions, published_holdings, device_id).await;
                    }
                    if accept_private(db, store, device_id, &plain).await {
                        publish(app, db, live, peers);
                    } else if refuse_detached_connect(db, material, store, peers, live, device_id, &plain).await {
                        // A control for a computer that plays on its own: refused above.
                    } else if !reply_sync_request(db, material, store, peers, live, device_id, &plain).await
                        && !reply_history_request(db, material, store, peers, device_id, &plain).await
                    {
                        note_history_reply(live, device_id, &plain);
                        apply_plain(app, db, live, pending_holdings, device_id, &plain);
                    }
                }
            } else if kind == SIGNAL_GROUP
                && let Ok(plain) = libsignal_protocol::group_decrypt(&bytes, store, &SignalStore::address_for(device_id)).await
            {
                note_history_reply(live, device_id, &plain);
                apply_plain(app, db, live, pending_holdings, device_id, &plain);
            }
        }
        Frame::AudioReq { id, content_id, start, end } => {
            serve_audio(db, peer, live, id, &content_id, start, end);
        }
        Frame::AudioChunk { id, offset, eof, data } => {
            if !bus.deliver(id, AudioPiece::chunk(offset, eof, data)) {
                let _ = peer.tx.send(Frame::AudioCancel { id });
            }
        }
        Frame::AudioHead { id, total } => {
            bus.deliver(id, AudioPiece::head(total));
        }
        Frame::AudioErr { id, reason } => {
            bus.deliver(id, AudioPiece::failed(&reason));
        }
        Frame::AudioCancel { id } => {
            if let Ok(mut guard) = live.inner.lock() {
                if guard.active_audio.contains(&id) {
                    guard.cancelled.insert(id);
                }
            }
        }
    }
}

/// Fetches the small pictures of a device's songs one by one, after the song list itself has arrived.
/// The list shows at once; pictures fill in as they come.
fn spawn_cover_fetch(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
    bus: &Arc<AudioBus>,
    device_id: String,
) {
    let Some(peer) = peers.get(&device_id).filter(|peer| peer.confirmed) else { return };
    let sender = peer.tx.clone();
    {
        let Ok(mut guard) = live.inner.lock() else { return };
        if !guard.covers_busy.insert(device_id.clone()) {
            // A pass is running; it makes one more pass when it sees this mark.
            guard.covers_wanted.insert(device_id);
            return;
        }
    }
    let (app, db, live, bus) = (app.clone(), db.clone(), live.clone(), bus.clone());
    tokio::task::spawn_local(async move {
        loop {
            let retry_before = chrono::Utc::now().timestamp() - 3600;
            let (missing, dir) = db
                .lock()
                .ok()
                .map(|guard| {
                    (
                        crate::catalog::missing_covers(&guard, &device_id, retry_before).unwrap_or_default(),
                        guard.cache_dir.join("covers"),
                    )
                })
                .unwrap_or_default();
            let mut saved = 0usize;
            for content_id in missing {
                let id = rand::random::<u64>();
                let (tx, mut rx) = mpsc::unbounded_channel();
                bus.register(id, tx);
                let request = Frame::AudioReq { id, content_id: format!("cover:{content_id}"), start: 0, end: None };
                if sender.send(request).is_err() {
                    bus.finish(id);
                    break;
                }
                let mut bytes: Vec<u8> = Vec::new();
                // Some(true): got the picture. Some(false): the other side has none. None: try again later.
                let mut outcome: Option<bool> = None;
                loop {
                    let Ok(Some(piece)) = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await else {
                        bus.finish(id);
                        break;
                    };
                    if let Some(reason) = &piece.error {
                        outcome = if reason == "file.missing" { Some(false) } else { None };
                        break;
                    }
                    if piece.total.is_some() && piece.data.is_empty() && !piece.eof {
                        continue;
                    }
                    let at = piece.offset as usize;
                    if bytes.len() < at || bytes.len() + piece.data.len() > 4 * 1024 * 1024 {
                        bus.finish(id);
                        break;
                    }
                    bytes.truncate(at);
                    bytes.extend_from_slice(&piece.data);
                    if piece.eof {
                        outcome = Some(is_cover_image(&bytes));
                        break;
                    }
                }
                match outcome {
                    Some(true) => {
                        let file = dir.join(format!("remote-{}.jpg", blake3::hash(content_id.as_bytes()).to_hex()));
                        if std::fs::write(&file, &bytes).is_ok() {
                            if let Ok(guard) = db.lock() {
                                let _ = crate::catalog::save_remote_cover(&guard, &content_id, Some(&file.to_string_lossy()));
                            }
                            saved += 1;
                            if saved % 8 == 0 {
                                let _ = app.emit("library-updated", json!({}));
                            }
                        }
                    }
                    Some(false) => {
                        if let Ok(guard) = db.lock() {
                            let _ = crate::catalog::save_remote_cover(&guard, &content_id, None);
                        }
                    }
                    None => {}
                }
            }
            if saved > 0 {
                let _ = app.emit("library-updated", json!({}));
            }
            let again = live.inner.lock().map(|mut guard| guard.covers_wanted.remove(&device_id)).unwrap_or(false);
            if !again {
                break;
            }
        }
        if let Ok(mut guard) = live.inner.lock() {
            guard.covers_busy.remove(&device_id);
        }
    });
}

/// Streams one byte range of a still-exposed local file. Exposure is re-checked per request.
fn serve_audio(
    db: &Arc<Mutex<Db>>,
    peer: &PeerOut,
    live: &Live,
    id: u64,
    content_id: &str,
    start: u64,
    end: Option<u64>,
) {
    // "cover:<content id>" asks for the small picture of a song; it follows the same exposure rules as the audio.
    let cover_of = content_id.strip_prefix("cover:");
    let mut pending_cover = false;
    let paths = db
        .lock()
        .ok()
        .and_then(|guard| match cover_of {
            Some(song) => {
                pending_cover = crate::catalog::cover_pending(&guard, song);
                crate::catalog::cover_paths(&guard, song).ok()
            }
            None => readable_paths(&guard, content_id).ok(),
        })
        .unwrap_or_default();
    let tx = peer.tx.clone();
    let audio = peer.audio.clone();
    let alive = peer.alive.clone();
    let live = live.clone();
    if let Ok(mut guard) = live.inner.lock() {
        guard.active_audio.insert(id);
    }
    tokio::task::spawn_local(async move {
        if live.inner.lock().map(|mut guard| guard.cancelled.remove(&id)).unwrap_or(false) {
            finish_audio_request(&live, id, false);
            return;
        }
        // File work runs on the blocking pool so a slow disk does not stall the network thread.
        let opened = tokio::task::spawn_blocking(move || {
            let mut reason = if pending_cover { "cover.pending" } else { "file.missing" };
            for (_, path) in &paths {
                match File::open(path).and_then(|file| file.metadata().map(|meta| (file, meta.len()))) {
                    Ok(found) => return Ok(found),
                    Err(_) => reason = "file.unreadable",
                }
            }
            Err(reason)
        })
        .await
        .unwrap_or(Err("file.unreadable"));
        let (mut file, total) = match opened {
            Ok(found) => found,
            Err(reason) => {
                let _ = tx.send(Frame::AudioErr { id, reason: reason.into() });
                finish_audio_request(&live, id, false);
                return;
            }
        };
        let stop = end.unwrap_or(total).min(total);
        // `start == total` only represents a valid full-file request for an
        // empty file.  Any other empty or inverted range has no bytes to send.
        if start > total || (start == total && total > 0) || end.is_some_and(|value| value <= start) || file.seek(SeekFrom::Start(start)).is_err() {
            let _ = tx.send(Frame::AudioErr { id, reason: "range".into() });
            finish_audio_request(&live, id, false);
            return;
        }
        if let Ok(mut guard) = live.inner.lock() {
            guard.serving_count += 1;
            guard.serving = true;
        }
        if tx.send(Frame::AudioHead { id, total }).is_err() {
            finish_audio_request(&live, id, true);
            return;
        }
        let mut offset = start;
        let mut buf = vec![0u8; 24 * 1024];
        loop {
            if !alive.load(std::sync::atomic::Ordering::Relaxed) || live.inner.lock().map(|mut guard| guard.cancelled.remove(&id)).unwrap_or(false) {
                break;
            }
            if offset >= stop {
                let _ = audio.send(Frame::AudioChunk { id, offset, eof: true, data: Vec::new() }).await;
                break;
            }
            let max = ((stop - offset) as usize).min(buf.len());
            let read = tokio::task::spawn_blocking(move || {
                let result = file.read(&mut buf[..max]);
                (file, buf, result)
            })
            .await;
            let Ok((returned_file, returned_buf, result)) = read else {
                let _ = tx.send(Frame::AudioErr { id, reason: "file.unreadable".into() });
                break;
            };
            file = returned_file;
            buf = returned_buf;
            let n = match result {
                Ok(n) => n,
                Err(_) => {
                    let _ = tx.send(Frame::AudioErr { id, reason: "file.unreadable".into() });
                    break;
                }
            };
            let eof = n == 0 || offset + n as u64 >= stop;
            if audio.send(Frame::AudioChunk { id, offset, eof, data: buf[..n].to_vec() }).await.is_err() || eof {
                break;
            }
            offset += n as u64;
        }
        finish_audio_request(&live, id, true);
    });
}

fn finish_audio_request(live: &Live, id: u64, was_serving: bool) {
    if let Ok(mut guard) = live.inner.lock() {
        guard.active_audio.remove(&id);
        guard.cancelled.remove(&id);
        if was_serving {
            guard.serving_count = guard.serving_count.saturating_sub(1);
            guard.serving = guard.serving_count > 0;
        }
    }
}

fn apply_plain(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    live: &Live,
    pending_holdings: &mut HashMap<String, HoldingsBuild>,
    device_id: &str,
    plain: &[u8],
) {
    let Ok(value) = serde_json::from_slice::<Value>(plain) else { return };
    let payload = value.get("payload").cloned().unwrap_or(value.clone());
    if !verify_author(db, device_id, &value) {
        return;
    }
    let value = payload;
    let Ok(guard) = db.lock() else { return };
    let kind = value.get("kind").and_then(|item| item.as_str()).unwrap_or("");
    match kind {
        "holdings" => {
            let revision = value.get("revision").and_then(|item| item.as_i64()).unwrap_or(0);
            let part = value.get("part").and_then(|item| item.as_u64()).unwrap_or(0) as usize;
            let part_count = value.get("parts").and_then(|item| item.as_u64()).unwrap_or(1) as usize;
            let Some(rows) = value.get("rows").and_then(|item| serde_json::from_value::<Vec<HoldingInput>>(item.clone()).ok()) else {
                return;
            };
            if let Some(rows) = accept_holdings_part(pending_holdings, device_id, revision, part, part_count, rows) {
                let applied = guard.replace_peer_holdings(device_id, revision, &rows).unwrap_or(false);
                eprintln!("holdings from {}: {} songs (revision {revision}, applied: {applied})", short_id(device_id), rows.len());
                if applied && let Ok(mut state) = live.inner.lock() {
                    state.covers_wanted.insert(device_id.to_string());
                }
            }
        }
        "history" => {
            // Apply only when this computer has recently-played sync on. Equal timestamps keep one copy.
            if !data_sync_open(&guard) || !guard.setting_bool("historySync").unwrap_or(false) {
                return;
            }
            // Long histories arrive in parts, like snapshots. Older senders send one message without parts.
            let part = value.get("part").and_then(|item| item.as_u64()).unwrap_or(0) as usize;
            let part_count = value.get("parts").and_then(|item| item.as_u64()).unwrap_or(1) as usize;
            let piece = value.get("entries").cloned().unwrap_or(json!([]));
            let entries = if part_count <= 1 {
                piece
            } else {
                let Some(done) = live.inner.lock().ok().and_then(|mut state| {
                    accept_snapshot_part(&mut state.pending_snapshots, device_id, HISTORY_PARTS_KEY, part, part_count, piece)
                }) else {
                    return;
                };
                done
            };
            let entries = entries.as_array().cloned().unwrap_or_default();
            let parsed = entries.iter().filter_map(|item| {
                let content_id = item.get("contentId")?.as_str()?.to_string();
                let at = chrono::DateTime::parse_from_rfc3339(item.get("at")?.as_str()?).ok()?;
                Some(HistoryEntry { content_id, at: at.with_timezone(&chrono::Utc) })
            }).collect::<Vec<_>>();
            eprintln!("history from {}: {} entries", short_id(device_id), parsed.len());
            let _ = merge_history_into_db(&guard, parsed);
            let _ = app.emit("library-updated", json!({}));
        }
        "sync-denied" => {
            let feature = value.get("feature").and_then(|item| item.as_str()).unwrap_or("");
            if let Ok(mut state) = live.inner.lock() {
                take_sync_request(&mut state, device_id, feature);
            }
            let name = guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id).map(|peer| peer.display_name).unwrap_or_default();
            let _ = app.emit("sync-denied", json!({"feature": feature, "name": name}));
        }
        "snapshot" => {
            if !data_sync_open(&guard) {
                return;
            }
            let feature = value.get("feature").and_then(|item| item.as_str()).unwrap_or("").to_string();
            if !crate::db::FEATURES.contains(&feature.as_str()) {
                return;
            }
            let manual = value.get("manual").and_then(|item| item.as_bool()).unwrap_or(false);
            let part = value.get("part").and_then(|item| item.as_u64()).unwrap_or(0) as usize;
            let part_count = value.get("parts").and_then(|item| item.as_u64()).unwrap_or(1) as usize;
            let piece = value.get("items").cloned().unwrap_or(json!([]));
            let items = if part_count <= 1 {
                piece
            } else {
                let Some(done) = live.inner.lock().ok().and_then(|mut state| {
                    accept_snapshot_part(&mut state.pending_snapshots, device_id, &feature, part, part_count, piece)
                }) else {
                    return;
                };
                done
            };
            // A reply counts as requested only when this computer really asked that device for this feature.
            let requested = value.get("requested").and_then(|item| item.as_bool()).unwrap_or(false)
                && live.inner.lock().map(|mut state| take_sync_request(&mut state, device_id, &feature)).unwrap_or(false);
            if manual && !requested && guard.overwrite_mode().unwrap_or_else(|_| "silent".into()) == "ask" {
                let name = guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id).map(|peer| peer.display_name).unwrap_or_default();
                drop(guard);
                if enqueue_overwrite(&live, device_id, &name, &feature, items) {
                    emit_overwrite_asks(app, &live);
                }
                return;
            }
            if !manual {
                let blocks = guard.block_flags(&feature).unwrap_or(crate::db::BlockFlags { manual: false, source: false, all: false });
                let source = guard.sync_source(&feature).unwrap_or(None);
                if blocks.source || blocks.all || source.as_deref() != Some(device_id) {
                    return;
                }
                if guard.is_dirty(&feature).unwrap_or(false) {
                    let policy = guard.setting_string("conflictPolicy").unwrap_or_else(|_| "ask".into());
                    let name = guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id).map(|peer| peer.display_name).unwrap_or_default();
                    if policy == "keep" {
                        let _ = guard.add_notice("conflict", device_id, &name, &format!("{feature}:keep"));
                        return;
                    }
                    if policy == "take" {
                        apply_snapshot_payload(&guard, &feature, &items);
                        sync_appearance_window(app, &guard, &feature);
                        let _ = guard.mark_dirty(&feature, false);
                        let _ = guard.add_notice("conflict", device_id, &name, &format!("{feature}:take"));
                        drop(guard);
                        let _ = app.emit("library-updated", json!({}));
                        return;
                    }
                    let mut waiting = guard.setting_json("pendingSnapshots").unwrap_or(json!({}));
                    if !waiting.is_object() {
                        waiting = json!({});
                    }
                    waiting[&feature] = json!({"source": device_id, "items": items});
                    let _ = guard.set_device_setting("pendingSnapshots", waiting);
                    let _ = guard.add_notice("conflict", device_id, &name, &format!("{feature}:ask"));
                    drop(guard);
                    let _ = app.emit("network-updated", json!({}));
                    return;
                }
            }
            apply_snapshot_payload(&guard, &feature, &items);
            sync_appearance_window(app, &guard, &feature);
            let _ = guard.mark_dirty(&feature, false);
            clear_pending(&guard, &feature);
            let name = guard.peers().ok().into_iter().flatten().find(|peer| peer.device_id == device_id).map(|peer| peer.display_name).unwrap_or_default();
            let _ = guard.add_notice("sync", device_id, &name, &feature);
            eprintln!("sync {feature} from {}: applied", short_id(device_id));
        }
        kind if crate::connect::KINDS.contains(&kind) => {
            // Like a private session: an incognito computer neither shows nor obeys other devices.
            if !data_sync_open(&guard) || guard.is_blocked(device_id).unwrap_or(true) {
                return;
            }
            drop(guard);
            let Some(body) = crate::connect::sanitize(kind, &value) else { return };
            let Ok(mut state) = live.inner.lock() else { return };
            // A computer that left plays on its own: no reports, controls or transfers get through.
            if crate::connect::inbound(state.connect_detached, kind) != crate::connect::Inbound::Forward {
                return;
            }
            let (rate_key, max) = if kind == "connect-state" {
                (format!("{device_id}:state"), crate::connect::STATE_RATE)
            } else {
                (format!("{device_id}:cmd"), crate::connect::COMMAND_RATE)
            };
            if !crate::connect::allow(&mut state.connect_rate, &rate_key, Instant::now(), max) {
                return;
            }
            if kind == "connect-state" {
                let epoch = body.get("epoch").and_then(Value::as_u64).unwrap_or(0);
                let seq = body.get("seq").and_then(Value::as_u64).unwrap_or(0);
                if !crate::connect::fresh_state(&mut state.connect_seen, device_id, epoch, seq) {
                    return;
                }
            }
            drop(state);
            let _ = app.emit("connect", json!({"from": device_id, "kind": kind, "body": body}));
            return;
        }
        // Messages this version does not know change nothing, so the library is not reloaded.
        _ => return,
    }
    drop(guard);
    let _ = app.emit("library-updated", json!({}));
}

const SNAPSHOT_PART_BYTES: usize = 16 * 1024;
/// Most parts one snapshot or history may arrive in (about 4 MiB of rows).
const SNAPSHOT_MAX_PARTS: usize = 256;
/// Pending-part key for history messages. It is not a feature name, so it never mixes with a snapshot.
const HISTORY_PARTS_KEY: &str = "\0history-entries";

pub(crate) struct SnapshotBuild {
    slots: Vec<Option<Value>>,
}

fn snapshot_part_messages(feature: &str, manual: bool, items: &Value) -> Vec<Value> {
    let Some(rows) = items.as_array() else {
        return vec![json!({"kind": "snapshot", "feature": feature, "manual": manual, "part": 0, "parts": 1, "items": items})];
    };
    let groups = part_groups(rows);
    let count = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(index, rows)| json!({"kind": "snapshot", "feature": feature, "manual": manual, "part": index, "parts": count, "items": rows}))
        .collect()
}

fn history_part_messages(entries: &Value) -> Vec<Value> {
    let Some(rows) = entries.as_array() else {
        return vec![json!({"kind": "history", "part": 0, "parts": 1, "entries": entries})];
    };
    let groups = part_groups(rows);
    let count = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(index, rows)| json!({"kind": "history", "part": index, "parts": count, "entries": rows}))
        .collect()
}

/// Splits rows so each part stays near SNAPSHOT_PART_BYTES, well under one frame.
fn part_groups(rows: &[Value]) -> Vec<Vec<Value>> {
    let mut groups: Vec<Vec<Value>> = Vec::new();
    let mut current = Vec::new();
    let mut size = 2usize;
    for row in rows {
        let encoded = serde_json::to_vec(row).unwrap_or_default();
        if !current.is_empty() && size + encoded.len() + 1 > SNAPSHOT_PART_BYTES {
            groups.push(std::mem::take(&mut current));
            size = 2;
        }
        size += encoded.len() + 1;
        current.push(row.clone());
    }
    if groups.is_empty() || !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn expand_sync_body(body: Value) -> Vec<Value> {
    if body.get("kind").and_then(|item| item.as_str()) == Some("history") {
        let entries = body.get("entries").cloned().unwrap_or(json!([]));
        return history_part_messages(&entries);
    }
    if body.get("kind").and_then(|item| item.as_str()) != Some("snapshot") {
        return vec![body];
    }
    let feature = body.get("feature").and_then(|item| item.as_str()).unwrap_or("");
    let manual = body.get("manual").and_then(|item| item.as_bool()).unwrap_or(false);
    let items = body.get("items").cloned().unwrap_or(json!([]));
    snapshot_part_messages(feature, manual, &items)
}

async fn deliver_sync(
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    bodies: Vec<Value>,
) {
    let targets: Vec<String> = peers.iter().filter(|(_, peer)| peer.confirmed).map(|(id, _)| id.clone()).collect();
    for body in &bodies {
        for id in &targets {
            send_private_signed(material, store, peers, id, body).await;
        }
    }
}

fn accept_snapshot_part(
    pending: &mut HashMap<String, SnapshotBuild>,
    device_id: &str,
    feature: &str,
    part: usize,
    part_count: usize,
    items: Value,
) -> Option<Value> {
    if part_count == 0 || part >= part_count || part_count > SNAPSHOT_MAX_PARTS {
        return None;
    }
    let key = format!("{device_id}\0{feature}");
    let entry = pending.entry(key.clone()).or_insert_with(|| SnapshotBuild { slots: vec![None; part_count] });
    if part == 0 || entry.slots.len() != part_count {
        *entry = SnapshotBuild { slots: vec![None; part_count] };
    }
    entry.slots[part] = Some(items);
    if entry.slots.iter().any(|slot| slot.is_none()) {
        return None;
    }
    let mut merged = Vec::new();
    for slot in entry.slots.iter_mut() {
        if let Some(value) = slot.take() {
            if let Some(rows) = value.as_array() {
                merged.extend(rows.iter().cloned());
            } else {
                merged.push(value);
            }
        }
    }
    pending.remove(&key);
    Some(json!(merged))
}

/// Parts of a copy that a device was still sending when it went away are not kept.
fn drop_partial_snapshots(live: &Live, device_id: &str) {
    let prefix = format!("{device_id}\0");
    if let Ok(mut guard) = live.inner.lock() {
        guard.pending_snapshots.retain(|key, _| !key.starts_with(&prefix));
    }
}

const SYNC_REQUEST_WINDOW: Duration = Duration::from_secs(120);
const SYNC_REPLY_INTERVAL: Duration = Duration::from_secs(10);

fn sync_request_key(device_id: &str, feature: &str) -> String {
    format!("{device_id}\0{feature}")
}

/// Uses up the matching request. An expired or unknown request does not count.
fn take_sync_request(state: &mut LiveInner, device_id: &str, feature: &str) -> bool {
    let now = Instant::now();
    state.sync_requests.retain(|_, deadline| *deadline > now);
    state.sync_requests.remove(&sync_request_key(device_id, feature)).is_some()
}

async fn request_sync(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
    feature: &str,
    device_id: &str,
) {
    if !crate::db::FEATURES.contains(&feature) {
        return;
    }
    let open = db.lock().map(|guard| data_sync_open(&guard)).unwrap_or(false);
    if !open {
        eprintln!("sync {feature}: request not sent, sync is off");
        return;
    }
    if !peers.get(device_id).is_some_and(|peer| peer.confirmed) {
        eprintln!("sync {feature}: {} is not connected", short_id(device_id));
        return;
    }
    eprintln!("sync {feature}: requesting a copy from {}", short_id(device_id));
    if let Ok(mut state) = live.inner.lock() {
        let now = Instant::now();
        state.sync_requests.retain(|_, deadline| *deadline > now);
        state.sync_requests.insert(sync_request_key(device_id, feature), now + SYNC_REQUEST_WINDOW);
    }
    send_private_signed(material, store, peers, device_id, &json!({"kind": "sync-request", "feature": feature})).await;
}

/// The other computer asked for our copy. Send it back only to them, so it replaces their copy.
async fn reply_sync_request(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
    device_id: &str,
    plain: &[u8],
) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(plain) else { return false };
    let payload = value.get("payload").cloned().unwrap_or(value.clone());
    if payload.get("kind").and_then(|item| item.as_str()) != Some("sync-request") {
        return false;
    }
    if !verify_author(db, device_id, &value) {
        return true;
    }
    let feature = payload.get("feature").and_then(|item| item.as_str()).unwrap_or("").to_string();
    if !crate::db::FEATURES.contains(&feature.as_str()) {
        return true;
    }
    // Building a full copy is expensive; repeated requests for the same copy are answered once per window.
    let fresh = live.inner.lock().map(|mut state| {
        let now = Instant::now();
        state.sync_replies.retain(|_, at| now.duration_since(*at) < SYNC_REPLY_INTERVAL);
        state.sync_replies.insert(sync_request_key(device_id, &feature), now).is_none()
    }).unwrap_or(true);
    if !fresh {
        return true;
    }
    let answer = {
        let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
        // No manual and Stop all only stop this computer from pushing or following.
        // A device that asked for this copy still receives it.
        if !data_sync_open(&guard) {
            eprintln!("sync {feature}: refused a request from {}", short_id(device_id));
            vec![json!({"kind": "sync-denied", "feature": feature})]
        } else {
            let items = snapshot_items(&guard, &feature);
            let count = items.as_array().map(|rows| rows.len()).unwrap_or(1);
            let mut parts = snapshot_part_messages(&feature, true, &items);
            for part in &mut parts {
                if let Some(object) = part.as_object_mut() {
                    object.insert("requested".into(), json!(true));
                }
            }
            eprintln!("sync {feature}: sending our {count} item(s) to {}", short_id(device_id));
            parts
        }
    };
    for body in &answer {
        send_private_signed(material, store, peers, device_id, body).await;
    }
    true
}

fn note_history_reply(live: &Live, device_id: &str, plain: &[u8]) {
    let Ok(value) = serde_json::from_slice::<Value>(plain) else { return };
    let payload = value.get("payload").cloned().unwrap_or(value);
    if payload.get("kind").and_then(|item| item.as_str()) != Some("history") {
        return;
    }
    if let Ok(mut guard) = live.inner.lock() {
        guard.history_waiting.remove(device_id);
    }
}

/// A startup request for this computer's play history. Sync off means no reply and no notice.
async fn reply_history_request(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    device_id: &str,
    plain: &[u8],
) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(plain) else { return false };
    let payload = value.get("payload").cloned().unwrap_or(value.clone());
    if payload.get("kind").and_then(|item| item.as_str()) != Some("history-request") {
        return false;
    }
    if !verify_author(db, device_id, &value) {
        return true;
    }
    let Some(body) = db.lock().ok().and_then(|guard| history_message(&guard)) else { return true };
    for part in expand_sync_body(body) {
        send_private_signed(material, store, peers, device_id, &part).await;
    }
    true
}

/// Once, after this computer comes online: merge every synced device's history, then send the result.
async fn tick_history_startup(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
) {
    let enabled = db.lock().map(|guard| {
        data_sync_open(&guard) && guard.setting_bool("historySync").unwrap_or(false) && guard.setting_bool("historyStartup").unwrap_or(false)
    }).unwrap_or(false);
    let online: Vec<String> = peers.iter().filter(|(_, peer)| peer.confirmed).map(|(id, _)| id.clone()).collect();
    let (ask, send) = {
        let Ok(mut guard) = live.inner.lock() else { return };
        if guard.history_done || !enabled || online.is_empty() {
            return;
        }
        if guard.history_deadline.is_none() {
            guard.history_deadline = Some(Instant::now() + Duration::from_secs(8));
        }
        let ask: Vec<String> = online.iter().filter(|id| guard.history_asked.insert((*id).clone())).cloned().collect();
        for id in &ask {
            guard.history_waiting.insert(id.clone());
        }
        let due = guard.history_deadline.is_some_and(|deadline| Instant::now() >= deadline)
            || (!guard.history_asked.is_empty() && guard.history_waiting.is_empty());
        let send = due && ask.is_empty();
        if send {
            guard.history_done = true;
            guard.history_waiting.clear();
        }
        (ask, send)
    };
    for id in &ask {
        send_private_signed(material, store, peers, id, &json!({"kind": "history-request"})).await;
    }
    if send {
        if let Some(body) = db.lock().ok().and_then(|guard| history_message(&guard)) {
            deliver_sync(material, store, peers, expand_sync_body(body)).await;
        }
    }
}

fn history_message(db: &Db) -> Option<Value> {
    if !data_sync_open(db) || !db.setting_bool("historySync").unwrap_or(false) {
        return None;
    }
    let entries = db
        .history_for_sync()
        .ok()?
        .into_iter()
        .map(|entry| json!({"contentId": entry.content_id, "at": entry.at.to_rfc3339()}))
        .collect::<Vec<_>>();
    Some(json!({"kind": "history", "entries": entries}))
}

async fn send_sync(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    feature: &str,
    refresh: bool,
) {
    if feature == "history" {
        let Some(body) = db.lock().ok().and_then(|guard| history_message(&guard)) else { return };
        deliver_sync(material, store, peers, expand_sync_body(body)).await;
        return;
    }
    let bodies = {
        let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
        if !data_sync_open(&guard) {
            eprintln!("sync {feature}: not sent, sync is off");
            return;
        }
        let blocks = guard.block_flags(feature).unwrap_or(crate::db::BlockFlags { manual: false, source: false, all: false });
        if blocks.manual || blocks.all {
            eprintln!("sync {feature}: not sent, manual sync is blocked");
            return;
        }
        if let Some(source) = guard.sync_source(feature).unwrap_or(None)
            && source != material.device_id && !refresh
        {
            return;
        }
        // Unsent changes stay marked when no paired device is connected to receive them.
        if peers.values().all(|peer| !peer.confirmed) {
            eprintln!("sync {feature}: not sent, no device is connected");
            return;
        }
        let items = snapshot_items(&guard, feature);
        let count = items.as_array().map(|rows| rows.len()).unwrap_or(1);
        let _ = guard.mark_dirty(feature, false);
        eprintln!("sync {feature}: sending {count} item(s)");
        snapshot_part_messages(feature, true, &items)
    };
    deliver_sync(material, store, peers, bodies).await;
}

/// A device that is its own sync source sends each local change. History in time-merge mode sends the trimmed list.
async fn auto_sync(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    feature: &str,
) {
    if peers.values().all(|peer| !peer.confirmed) {
        return;
    }
    let Some(body) = db.lock().ok().and_then(|guard| auto_body(&guard, &material.device_id, feature)) else { return };
    deliver_sync(material, store, peers, expand_sync_body(body)).await;
}

fn auto_body(db: &Db, self_id: &str, feature: &str) -> Option<Value> {
    if !data_sync_open(db) {
        return None;
    }
    if feature == "history" {
        return history_message(db);
    }
    // Auto send pushes this computer's edit out. It does not pull anyone else's copy.
    let auto = db.setting_json("autoSend").unwrap_or(json!({}));
    if auto.get(feature).and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let blocks = db.block_flags(feature).ok()?;
    if blocks.all {
        return None;
    }
    // An empty source is the "This computer" row: this device publishes.
    // A different device id means this computer is following that device.
    match db.sync_source(feature) {
        Ok(Some(id)) if id != self_id => return None,
        Ok(_) => {}
        Err(_) => return None,
    }
    let items = snapshot_items(db, feature);
    let _ = db.mark_dirty(feature, false);
    Some(json!({"kind": "snapshot", "feature": feature, "manual": false, "items": items}))
}

fn clear_pending(db: &Db, feature: &str) {
    let mut waiting = db.setting_json("pendingSnapshots").unwrap_or(json!({}));
    if let Some(map) = waiting.as_object_mut() {
        if map.remove(feature).is_some() {
            let _ = db.set_device_setting("pendingSnapshots", waiting);
        }
    }
}

/// The follower chose between its unsynced local copy and the source's snapshot.
fn resolve_pending(app: &AppHandle, db: &Arc<Mutex<Db>>, feature: &str, take: bool) {
    let Ok(guard) = db.lock() else { return };
    let waiting = guard.setting_json("pendingSnapshots").unwrap_or(json!({}));
    let Some(item) = waiting.get(feature).cloned() else { return };
    if take {
        let items = item.get("items").cloned().unwrap_or(json!([]));
        apply_snapshot_payload(&guard, feature, &items);
        sync_appearance_window(app, &guard, feature);
        let _ = guard.mark_dirty(feature, false);
    }
    clear_pending(&guard, feature);
    drop(guard);
    let _ = app.emit("library-updated", json!({}));
}

async fn send_sender_key(
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    device_id: &str,
) {
    let sender = SignalStore::address_for(&material.device_id);
    let mut rng = rand::rng();
    if let Ok(message) = libsignal_protocol::create_sender_key_distribution_message(&sender, material.distribution_id, store, &mut rng).await {
        send_private(material, store, peers, device_id, &json!({
            "kind": "skdm",
            "bytes": b64_bytes(message.serialized()),
        })).await;
    }
}

/// A control or transfer that reached this computer while it plays on its own is answered with a
/// refusal here, so it never reaches the player and the sender is not left waiting. True when the
/// message was one of those.
async fn refuse_detached_connect(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
    device_id: &str,
    plain: &[u8],
) -> bool {
    if !live.inner.lock().map(|state| state.connect_detached).unwrap_or(false) {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<Value>(plain) else { return false };
    let Some(payload) = value.get("payload") else { return false };
    let kind = payload.get("kind").and_then(Value::as_str).unwrap_or("");
    if crate::connect::inbound(true, kind) != crate::connect::Inbound::Refuse {
        return false;
    }
    let allowed = db.lock().ok().is_some_and(|guard| data_sync_open(&guard) && !guard.is_blocked(device_id).unwrap_or(true));
    if !allowed || !verify_author(db, device_id, &value) {
        return true;
    }
    let within = live.inner.lock().is_ok_and(|mut state| {
        crate::connect::allow(&mut state.connect_rate, &format!("{device_id}:cmd"), Instant::now(), crate::connect::COMMAND_RATE)
    });
    if !within {
        return true;
    }
    if let Some(answer) = crate::connect::refusal(kind, payload).and_then(|body| crate::connect::sanitize("connect-ack", &body)) {
        send_private_signed(material, store, peers, device_id, &answer).await;
    }
    true
}

/// A random number for this run of the app, so devices can tell a restart from an old report.
fn connect_epoch() -> u64 {
    static EPOCH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(|| rand::random::<u32>() as u64)
}

#[allow(clippy::too_many_arguments)]
async fn send_connect(
    db: &Arc<Mutex<Db>>,
    live: &Live,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    kind: &str,
    to: Option<&str>,
    body: &Value,
) {
    let open = db.lock().ok().is_some_and(|guard| data_sync_open(&guard));
    // Encrypting saves key state to disk, so nothing is sealed when no paired device is connected.
    if !open || peers.values().all(|peer| !peer.confirmed) {
        return;
    }
    let detached = live.inner.lock().map(|state| state.connect_detached).unwrap_or(false);
    if !crate::connect::outbound_allowed(detached, kind) {
        return;
    }
    let Some(mut message) = crate::connect::sanitize(kind, body) else {
        eprintln!("connect: {kind} not sent, malformed");
        return;
    };
    if kind == "connect-leave" {
        seal_and_send(material, store, peers, &message).await;
        return;
    }
    if kind == "connect-state" {
        let seq = live.inner.lock().map(|mut state| {
            state.connect_seq += 1;
            state.connect_seq
        });
        let Ok(seq) = seq else { return };
        message["epoch"] = json!(connect_epoch());
        message["seq"] = json!(seq);
        seal_and_send(material, store, peers, &message).await;
        return;
    }
    let Some(to) = to.filter(|id| crate::connect::is_device_id(id)) else { return };
    if !peers.get(to).is_some_and(|peer| peer.confirmed) {
        return;
    }
    send_private_signed(material, store, peers, to, &message).await;
}

async fn seal_and_send(
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    body: &Value,
) {
    let payload = serde_json::to_vec(body).unwrap_or_default();
    let mut rng = rand::rng();
    let Ok(signature) = material.identity.private_key().calculate_signature(&payload, &mut rng) else { return };
    let envelope = json!({
        "author": material.device_id,
        "signature": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, signature.as_ref()),
        "payload": body,
    });
    let plain = serde_json::to_vec(&envelope).unwrap_or_default();
    let sender = SignalStore::address_for(&material.device_id);
    if let Ok(message) = libsignal_protocol::group_encrypt(store, &sender, material.distribution_id, &plain, &mut rng).await {
        let frame = Frame::Signal(tag_group(message.serialized()));
        for peer in peers.values().filter(|peer| peer.confirmed) {
            let _ = peer.tx.send(frame.clone());
        }
    }
}

fn data_sync_open(db: &Db) -> bool {
    !db.setting_bool("incognito").unwrap_or(false)
}

fn snapshot_items(db: &Db, feature: &str) -> Value {
    match feature {
        "favorites" => json!(db.favorite_content_ids().unwrap_or_default()),
        "appearance" => json!({
            "theme": db.synced("theme").unwrap_or_else(|_| "dark".into()),
            "language": db.synced("language").unwrap_or_else(|_| "en".into()),
            "miniPlayer": db.mini_flags().0,
            "miniTray": db.mini_flags().1,
            "closeAction": db.setting_string("closeAction").unwrap_or_default(),
        }),
        "history" => json!(db
            .history_for_sync()
            .unwrap_or_default()
            .into_iter()
            .map(|entry| json!({"contentId": entry.content_id, "at": entry.at.to_rfc3339()}))
            .collect::<Vec<_>>()),
        _ => json!([]),
    }
}

fn sync_appearance_window(app: &AppHandle, db: &Db, feature: &str) {
    if feature != "appearance" {
        return;
    }
    let (on_minimize, in_tray) = db.mini_flags();
    crate::desktop::sync_mini(app, on_minimize, in_tray);
}

fn apply_snapshot_payload(db: &Db, feature: &str, items: &Value) {
    match feature {
        "history" => {
            let entries = items
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|item| {
                            let content_id = item.get("contentId")?.as_str()?.to_string();
                            let at = chrono::DateTime::parse_from_rfc3339(item.get("at")?.as_str()?).ok()?;
                            Some(HistoryEntry { content_id, at: at.with_timezone(&chrono::Utc) })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let _ = merge_history_into_db(db, entries);
        }
        "favorites" => {
            let ids = items.as_array().map(|rows| rows.iter().filter_map(|item| item.as_str().map(str::to_string)).collect::<Vec<_>>()).unwrap_or_default();
            let existing = db.content_member_refs("favorites", None).unwrap_or_default();
            let kept = sync_logic::apply_member_snapshot(&existing, &ids);
            let _ = db.replace_favorites(&kept);
        }
        "appearance" => {
            if let Some(text) = items.get("theme").and_then(|value| value.as_str()) {
                let _ = db.set_synced("theme", text);
            }
            if let Some(text) = items.get("language").and_then(|value| value.as_str()) {
                let _ = db.set_synced("language", text);
            }
            if let Some(mini) = items.get("miniPlayer").and_then(|value| value.as_bool()) {
                let _ = db.set_device_setting("miniPlayer", json!(mini));
            }
            if let Some(tray) = items.get("miniTray").and_then(|value| value.as_bool()) {
                let _ = db.set_device_setting("miniTray", json!(tray));
            }
            if let Some(action) = items.get("closeAction").and_then(|value| value.as_str()) {
                let _ = db.set_device_setting("closeAction", json!(action));
            }
        }
        _ => {}
    }
}

fn verify_author(db: &Arc<Mutex<Db>>, device_id: &str, envelope: &Value) -> bool {
    let Some(payload) = envelope.get("payload") else { return false };
    let Some(signature) = envelope.get("signature").and_then(|item| item.as_str()) else { return false };
    if envelope.get("author").and_then(|item| item.as_str()) != Some(device_id) {
        return false;
    }
    let Ok(signature) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature) else { return false };
    let Ok(bytes) = serde_json::to_vec(payload) else { return false };
    let Ok(guard) = db.lock() else { return false };
    let Ok(peers) = guard.peers() else { return false };
    let Some(peer) = peers.into_iter().find(|peer| peer.device_id == device_id) else { return false };
    let Ok(identity) = libsignal_protocol::IdentityKey::decode(&peer.identity_public) else { return false };
    identity.public_key().verify_signature(&bytes, &signature)
}

fn trust_saved_peers(db: &Arc<Mutex<Db>>, store: &mut SignalStore) {
    let Ok(guard) = db.lock() else { return };
    let Ok(peers) = guard.peers() else { return };
    for peer in peers {
        store.trust(&peer.device_id, &peer.identity_public);
    }
}

async fn send_bundle(db: &Arc<Mutex<Db>>, store: &mut SignalStore, peers: &HashMap<String, PeerOut>, device_id: &str) {
    let _ = db;
    let Ok(bundle) = store.ensure_prekeys().await else { return };
    let Ok(identity) = bundle.identity_key() else { return };
    let Ok(signed_public) = bundle.signed_pre_key_public() else { return };
    let Ok(signed_signature) = bundle.signed_pre_key_signature() else { return };
    let Ok(kyber_public) = bundle.kyber_pre_key_public() else { return };
    let Ok(kyber_signature) = bundle.kyber_pre_key_signature() else { return };
    let body = json!({
        "identity": b64_bytes(&identity.serialize()),
        "signedPublic": b64_bytes(&signed_public.serialize()),
        "signedSignature": b64_bytes(signed_signature),
        "kyberPublic": b64_bytes(&kyber_public.serialize()),
        "kyberSignature": b64_bytes(kyber_signature),
        "registrationId": bundle.registration_id().unwrap_or(1),
    });
    if let Some(peer) = peers.get(device_id) {
        let _ = peer.tx.send(Frame::Bundle(serde_json::to_vec(&body).unwrap_or_default()));
    }
}

async fn session_ready(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    ready_sessions: &mut HashSet<String>,
    published_holdings: &mut String,
    device_id: &str,
) {
    if !ready_sessions.insert(device_id.to_string()) {
        return;
    }
    let name = db.lock().ok().and_then(|guard| guard.setting_string("displayName").ok()).unwrap_or_default();
    eprintln!("secure session with {} is ready", short_id(device_id));
    send_private(material, store, peers, device_id, &json!({"kind": "hello", "name": name})).await;
    send_sender_key(material, store, peers, device_id).await;
    // The reconnecting peer needs the full current state even when nothing changed here.
    // A changed list goes to every device below; then this device already has it.
    let published = publish_holdings(db, material, store, peers, published_holdings).await;
    let catch_up = db.lock().ok().map(|guard| {
        let mut bodies = Vec::new();
        if !published {
            let rows = local_publishable(&guard).unwrap_or_default();
            let revision = guard.our_holdings_revision().unwrap_or(0).max(1);
            bodies.extend(holdings_messages(revision, &rows));
        }
        for feature in crate::db::FEATURES {
            if let Some(body) = auto_body(&guard, &material.device_id, feature) {
                bodies.extend(expand_sync_body(body));
            }
        }
        bodies
    }).unwrap_or_default();
    for body in catch_up {
        send_private_signed(material, store, peers, device_id, &body).await;
    }
}

async fn send_private_signed(
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    device_id: &str,
    body: &Value,
) {
    let payload = serde_json::to_vec(body).unwrap_or_default();
    let mut rng = rand::rng();
    let Ok(signature) = material.identity.private_key().calculate_signature(&payload, &mut rng) else { return };
    let envelope = json!({
        "author": material.device_id,
        "signature": b64_bytes(signature.as_ref()),
        "payload": body,
    });
    send_private(material, store, peers, device_id, &envelope).await;
}

async fn send_private(
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    device_id: &str,
    body: &Value,
) {
    let Some(peer) = peers.get(device_id) else { return };
    let plain = serde_json::to_vec(body).unwrap_or_default();
    let remote = SignalStore::address_for(device_id);
    let local = SignalStore::address_for(&material.device_id);
    let mut sessions = store.clone();
    let mut identities = store.clone();
    let mut rng = rand::rng();
    if let Ok(message) = libsignal_protocol::message_encrypt(
        &plain,
        &remote,
        &local,
        &mut sessions,
        &mut identities,
        std::time::SystemTime::now(),
        &mut rng,
    ).await {
        let _ = peer.tx.send(Frame::Signal(tag_ciphertext(&message)));
    }
}

async fn accept_private(db: &Arc<Mutex<Db>>, store: &mut SignalStore, device_id: &str, plain: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(plain) else { return false };
    match value.get("kind").and_then(|item| item.as_str()) {
        Some("hello") => {
            let name = clean_name(value.get("name").and_then(|item| item.as_str()).unwrap_or(""), 64);
            if !name.is_empty() {
                let _ = db.lock().map(|guard| guard.rename_peer(device_id, &name));
            }
            return true;
        }
        Some("skdm") => {}
        _ => return false,
    }
    let Some(text) = value.get("bytes").and_then(|item| item.as_str()) else { return true };
    let Ok(bytes) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, text) else { return true };
    if let Ok(message) = libsignal_protocol::SenderKeyDistributionMessage::try_from(bytes.as_slice()) {
        let _ = libsignal_protocol::process_sender_key_distribution_message(
            &SignalStore::address_for(device_id),
            &message,
            store,
        ).await;
    }
    true
}

async fn publish_holdings(
    db: &Arc<Mutex<Db>>,
    material: &IdentityMaterial,
    store: &mut SignalStore,
    peers: &HashMap<String, PeerOut>,
    published_holdings: &mut String,
) -> bool {
    if peers.values().all(|peer| !peer.confirmed) {
        return false;
    }
    let Some((fingerprint, revision, rows)) = ({
        let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
        let rows = local_publishable(&guard).unwrap_or_default();
        let fingerprint = serde_json::to_string(&rows).unwrap_or_default();
        if fingerprint == *published_holdings {
            None
        } else {
            let revision = guard.our_holdings_revision().unwrap_or(0) + 1;
            let _ = guard.set_holdings_revision(revision);
            Some((fingerprint, revision, rows))
        }
    }) else {
        return false;
    };
    *published_holdings = fingerprint;
    for body in holdings_messages(revision, &rows) {
        seal_and_send(material, store, peers, &body).await;
    }
    true
}

/// One signal frame cannot carry a whole library, so the list is split.
/// The receiver replaces its copy only after every part of this revision has arrived.
const HOLDINGS_PART_BYTES: usize = 20 * 1024;

fn holdings_messages(revision: i64, rows: &[HoldingInput]) -> Vec<Value> {
    let mut groups: Vec<Vec<HoldingInput>> = Vec::new();
    let mut current = Vec::new();
    let mut size = 2usize;
    for row in rows {
        let encoded = serde_json::to_vec(row).unwrap_or_default();
        if !current.is_empty() && size + encoded.len() + 1 > HOLDINGS_PART_BYTES {
            groups.push(std::mem::take(&mut current));
            size = 2;
        }
        size += encoded.len() + 1;
        current.push(row.clone());
    }
    groups.push(current);
    let count = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(index, rows)| json!({"kind": "holdings", "revision": revision, "part": index, "parts": count, "rows": rows}))
        .collect()
}

struct HoldingsBuild {
    revision: i64,
    slots: Vec<Option<Vec<HoldingInput>>>,
}

fn accept_holdings_part(
    pending: &mut HashMap<String, HoldingsBuild>,
    device_id: &str,
    revision: i64,
    part: usize,
    part_count: usize,
    rows: Vec<HoldingInput>,
) -> Option<Vec<HoldingInput>> {
    if part_count == 0 || part_count > 4096 || part >= part_count {
        return None;
    }
    let entry = pending.entry(device_id.to_string()).or_insert_with(|| HoldingsBuild {
        revision,
        slots: vec![None; part_count],
    });
    if entry.revision != revision || entry.slots.len() != part_count {
        if entry.revision > revision {
            return None;
        }
        entry.revision = revision;
        entry.slots = vec![None; part_count];
    }
    entry.slots[part] = Some(rows);
    if entry.slots.iter().any(|slot| slot.is_none()) {
        return None;
    }
    let done = entry.slots.iter_mut().flat_map(|slot| slot.take().unwrap_or_default()).collect();
    pending.remove(device_id);
    Some(done)
}

/// An oversized frame is skipped and logged. A real write error ends the connection.
async fn write_or_stop(writer: &mut (impl tokio::io::AsyncWriteExt + Unpin), frame: &Frame) -> bool {
    match frames::write_frame(writer, frame).await {
        Ok(()) => false,
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
            eprintln!("frame not sent: {err}");
            false
        }
        Err(_) => true,
    }
}

fn b64_bytes(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

fn dequeue_download(live: &Live, tx: &mpsc::UnboundedSender<Cmd>) {
    let next = live.inner.lock().ok().and_then(|mut guard| guard.download_queue.pop_front());
    if let Some(next) = next {
        let _ = tx.send(Cmd::Download(next.content_id));
    }
}

fn song_title(db: &Db, content_id: &str) -> String {
    let from_library = db
        .conn
        .query_row(
            "SELECT title FROM content_items WHERE content_id = ?1",
            rusqlite::params![content_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .filter(|title| !title.is_empty());
    if let Some(title) = from_library {
        return title;
    }
    db.conn
        .query_row(
            "SELECT title FROM holdings WHERE content_id = ?1 AND title != '' LIMIT 1",
            rusqlite::params![content_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default()
}

fn store_download(live: &Live, mut status: DownloadStatus) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    status.seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut guard) = live.inner.lock() {
        guard.download = Some(status);
    }
}

fn publish_download(app: &AppHandle, live: &Live, history: Option<Value>, refresh: bool) {
    let body = live.inner.lock().ok().map(|guard| {
        json!({
            "current": guard.download.clone(),
            "queue": guard.download_queue.clone(),
            "history": history,
        })
    });
    if let Some(body) = body {
        let _ = app.emit("download-updated", body);
    }
    if refresh {
        let _ = app.emit("network-updated", json!({}));
    }
}

async fn start_download(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    peers: &HashMap<String, PeerOut>,
    live: &Live,
    bus: &Arc<AudioBus>,
    tx: &mpsc::UnboundedSender<Cmd>,
    content_id: String,
) {
    let set_status = |content_id: &str, state: &str, detail: &str, title: &str| {
        store_download(live, DownloadStatus {
            seq: 0,
            content_id: content_id.to_string(),
            state: state.into(),
            detail: detail.into(),
            title: title.into(),
            received: 0,
            total: 0,
        });
        publish_download(app, live, None, true);
    };
    if live.inner.lock().map(|guard| guard.download.as_ref().is_some_and(|item| item.state == "started")).unwrap_or(false) {
        let title = db.lock().ok().map(|guard| song_title(&guard, &content_id)).unwrap_or_default();
        if let Ok(mut guard) = live.inner.lock() {
            let current = guard.download.as_ref().map(|item| item.content_id.clone());
            if current.as_deref() != Some(content_id.as_str()) && !guard.download_queue.iter().any(|item| item.content_id == content_id) {
                guard.download_queue.push_back(QueuedDownload { content_id, title });
            }
        }
        publish_download(app, live, None, false);
        return;
    }
    let guard = db.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.setting_bool("incognito").unwrap_or(false) {
        drop(guard);
        if let Ok(mut queued) = live.inner.lock() {
            queued.download_queue.clear();
        }
        return set_status(&content_id, "failed", "notice.offlineTrack", "");
    }
    let folder = guard.setting_string("downloadFolder").unwrap_or_default();
    if folder.is_empty() {
        drop(guard);
        // Keep what was already waiting for a folder. Choosing one restarts the shown song and the queue follows it.
        if let Ok(mut queued) = live.inner.lock() {
            queued.download_queue.retain(|item| item.content_id != content_id);
            let waiting = queued
                .download
                .as_ref()
                .filter(|item| item.state == "need-folder" && item.content_id != content_id)
                .map(|item| QueuedDownload { content_id: item.content_id.clone(), title: item.title.clone() });
            if let Some(waiting) = waiting {
                queued.download_queue.push_back(waiting);
            }
        }
        return set_status(&content_id, "need-folder", "", "");
    }
    let holders: Vec<(String, String, String)> = {
        let mut stmt = guard.conn.prepare(
            "SELECT device_id, COALESCE(title, ''), COALESCE(codec, '') FROM holdings WHERE content_id = ?1",
        ).ok();
        stmt.as_mut().and_then(|stmt| {
            stmt.query_map(rusqlite::params![content_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            }).ok().and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
        }).unwrap_or_default()
    };
    drop(guard);
    let sources = holders.iter().filter(|row| peers.get(&row.0).is_some_and(|peer| peer.confirmed)).map(|row| PlaySource {
        device_id: row.0.clone(),
        path_id: content_id.clone(),
        kind: SourceKind::Remote,
    }).collect::<Vec<_>>();
    let senders: Vec<(PlaySource, mpsc::UnboundedSender<Frame>)> = sources
        .iter()
        .filter_map(|source| peers.get(&source.device_id).map(|peer| (source.clone(), peer.tx.clone())))
        .collect();
    let SourceChoice::Play(first) = choose_download_source(&sources, None) else {
        set_status(&content_id, "failed", "notice.offlineTrack", "");
        dequeue_download(live, tx);
        return;
    };
    let title = holders.iter().find(|row| row.0 == first.device_id && !row.1.is_empty()).map(|row| row.1.clone()).unwrap_or_default();
    let title = if title.is_empty() { db.lock().ok().map(|guard| song_title(&guard, &content_id)).unwrap_or_default() } else { title };
    let codec = holders.iter().find(|row| row.0 == first.device_id).map(|row| row.2.clone()).unwrap_or_else(|| "audio".into());
    set_status(&content_id, "started", &folder, &title);
    let app = app.clone();
    let db = db.clone();
    let live = live.clone();
    let bus = bus.clone();
    let cmd_tx = tx.clone();
    tokio::task::spawn_local(async move {
        let directory = PathBuf::from(&folder);
        // The song goes straight to disk; only a small part of it is in memory at any time.
        let temp = directory.join(format!(".tunepeerly-download-{content_id}.part"));
        let mut attempt = Some(first);
        let mut size = 0u64;
        let mut ok = false;
        while let Some(source) = attempt.take() {
            let Some((_, sender)) = senders.iter().find(|(item, _)| item == &source) else { break };
            let created = {
                let directory = directory.clone();
                let temp = temp.clone();
                tokio::task::spawn_blocking(move || std::fs::create_dir_all(&directory).and_then(|_| File::create(&temp))).await
            };
            let Ok(Ok(file)) = created else { break };
            let mut part = Some(file);
            let id = rand::random::<u64>();
            let (tx, mut rx) = mpsc::unbounded_channel();
            let queued = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            bus.register_limited(id, tx, queued.clone());
            if sender
                .send(Frame::AudioReq {
                    id,
                    content_id: content_id.clone(),
                    start: 0,
                    end: None,
                })
                .is_err()
            {
                bus.finish(id);
                let sources: Vec<PlaySource> = senders.iter().map(|(item, _)| item.clone()).collect();
                if let SourceChoice::Play(next) = choose_download_source(&sources, Some(&source)) {
                    attempt = Some(next);
                }
                continue;
            }
            let mut hasher = blake3::Hasher::new();
            let mut written = 0u64;
            let mut failed = false;
            let mut total_bytes = 0u64;
            let mut last_progress = Instant::now() - Duration::from_secs(1);
            loop {
                let piece = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await;
                let Ok(Some(piece)) = piece else { failed = true; break };
                media::taken(&queued, &piece);
                if piece.error.is_some() {
                    failed = true;
                    break;
                }
                if let Some(total) = piece.total {
                    if total > MAX_DOWNLOAD_BYTES {
                        failed = true;
                        break;
                    }
                    total_bytes = total_bytes.max(total);
                }
                if !piece.data.is_empty() {
                    let at = piece.offset;
                    let end = at.saturating_add(piece.data.len() as u64);
                    // The sender streams from the start. A gap, or more bytes than the announced size, is a bad peer.
                    let over = (total_bytes > 0 && end > total_bytes) || end > MAX_DOWNLOAD_BYTES;
                    if at > written || over {
                        failed = true;
                        break;
                    }
                    // Bytes sent again are already on disk; only the new part is written.
                    let fresh = &piece.data[(written - at).min(piece.data.len() as u64) as usize..];
                    if !fresh.is_empty() {
                        hasher.update(fresh);
                        let chunk = fresh.to_vec();
                        let Some(mut file) = part.take() else { failed = true; break };
                        let wrote = tokio::task::spawn_blocking(move || {
                            use std::io::Write;
                            let result = file.write_all(&chunk);
                            (file, result)
                        })
                        .await;
                        let Ok((returned, result)) = wrote else { failed = true; break };
                        part = Some(returned);
                        if result.is_err() {
                            failed = true;
                            break;
                        }
                        written += fresh.len() as u64;
                    }
                }
                let due = last_progress.elapsed() >= Duration::from_millis(200) || piece.eof;
                if due {
                    last_progress = Instant::now();
                    if let Ok(mut guard) = live.inner.lock() {
                        if let Some(status) = guard.download.as_mut() {
                            if status.content_id == content_id && status.state == "started" {
                                status.received = written;
                                status.total = total_bytes;
                            }
                        }
                    }
                    publish_download(&app, &live, None, false);
                }
                if piece.eof {
                    break;
                }
            }
            bus.finish(id);
            if failed {
                let _ = sender.send(Frame::AudioCancel { id });
            }
            let flushed = match part.take() {
                Some(file) => tokio::task::spawn_blocking(move || file.sync_all()).await.is_ok_and(|result| result.is_ok()),
                None => false,
            };
            if !failed && flushed && hasher.finalize().to_hex().as_str() == content_id {
                size = written;
                ok = true;
                break;
            }
            let sources: Vec<PlaySource> = senders.iter().map(|(item, _)| item.clone()).collect();
            if let SourceChoice::Play(next) = choose_download_source(&sources, Some(&source)) {
                attempt = Some(next);
            }
        }
        let finish = |state: &str, detail: &str| {
            store_download(&live, DownloadStatus {
                seq: 0,
                content_id: content_id.clone(),
                state: state.into(),
                detail: detail.into(),
                title: title.clone(),
                received: size,
                total: size,
            });
        };
        let remember = |path: &str, saved_ok: bool| -> Option<Value> {
            db.lock().ok().and_then(|guard| guard.push_download_history(&content_id, &title, path, saved_ok).ok())
        };
        if !ok {
            let temp = temp.clone();
            let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(&temp)).await;
            finish("failed", "");
            let history = remember("", false);
            publish_download(&app, &live, history, true);
            dequeue_download(&live, &cmd_tx);
            return;
        }
        let name = download_name(&title, &codec, &content_id);
        // Moving the file and reading its tags run on the blocking pool, not the network thread.
        // Only the new file is imported; the rest of the folder is not rescanned.
        let saved = {
            let directory = directory.clone();
            let db = db.clone();
            tokio::task::spawn_blocking(move || {
                let final_path = unique_path(&directory, &name);
                if std::fs::rename(&temp, &final_path).is_err() {
                    let _ = std::fs::remove_file(&temp);
                    return None;
                }
                let imported = crate::scan::import_downloaded_file(&db, &directory, &final_path).is_ok();
                Some((final_path, imported))
            })
            .await
            .ok()
            .flatten()
        };
        let (history, saved_ok) = if let Some((final_path, imported)) = saved {
            if imported {
                use tauri::Manager;
                let _ = app.asset_protocol_scope().allow_directory(&directory, true);
            }
            let path = final_path.to_string_lossy().to_string();
            finish("saved", &path);
            (remember(&path, true), true)
        } else {
            finish("failed", &folder);
            (remember(&folder, false), false)
        };
        publish_download(&app, &live, history, true);
        if saved_ok {
            let _ = app.emit("library-updated", json!({}));
        }
        dequeue_download(&live, &cmd_tx);
    });
}

/// A peer's cover is shown by the WebView, so keep only bytes that are a picture it can draw.
fn is_cover_image(bytes: &[u8]) -> bool {
    matches!(
        image::guess_format(bytes),
        Ok(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::Gif | image::ImageFormat::WebP | image::ImageFormat::Bmp)
    )
}

/// Largest file a download accepts.
const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024 * 1024;

fn download_name(title: &str, codec: &str, content_id: &str) -> String {
    let stem = match safe_file_stem(title) {
        Some(stem) => stem,
        None => safe_file_stem(content_id).unwrap_or_else(|| "track".into()),
    };
    let ext = crate::scan::file_extension_for_codec(codec);
    format!("{stem}.{ext}")
}

/// A peer's title as a Windows file name: no path characters, no trailing dots or spaces
/// (Windows drops them), no device names such as `CON` or `COM1`, and not overly long.
fn safe_file_stem(text: &str) -> Option<String> {
    let clean = text
        .chars()
        .map(|ch| if ch.is_control() || "\\/:*?\"<>|".contains(ch) { '_' } else { ch })
        .take(120)
        .collect::<String>();
    let clean = clean.trim().trim_end_matches(['.', ' ']);
    if clean.is_empty() || clean.chars().all(|ch| ch == '.') {
        return None;
    }
    let device = clean.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.as_bytes()[3].is_ascii_digit());
    Some(if reserved { format!("_{clean}") } else { clean.to_string() })
}

fn unique_path(directory: &PathBuf, name: &str) -> PathBuf {
    let direct = directory.join(name);
    if !direct.exists() {
        return direct;
    }
    let path = std::path::Path::new(name);
    let stem = path.file_stem().and_then(|text| text.to_str()).unwrap_or("track");
    let ext = path.extension().and_then(|text| text.to_str()).unwrap_or("audio");
    for index in 2..10_000 {
        let candidate = directory.join(format!("{stem} ({index}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    let tick = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0);
    directory.join(format!("{stem}-{tick}.{ext}"))
}

fn decode_bundle(bytes: &[u8]) -> AppResult<libsignal_protocol::PreKeyBundle> {
    let value: Value = serde_json::from_slice(bytes)?;
    let b64 = |key: &str| {
        base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            value.get(key).and_then(|item| item.as_str()).unwrap_or(""),
        ).map_err(|err| crate::error::AppError::Message(err.to_string()))
    };
    let identity = libsignal_protocol::IdentityKey::decode(&b64("identity")?).map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    let signed_public = libsignal_protocol::PublicKey::deserialize(&b64("signedPublic")?).map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    let kyber_public = libsignal_protocol::kem::PublicKey::deserialize(&b64("kyberPublic")?).map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    libsignal_protocol::PreKeyBundle::new(
        value.get("registrationId").and_then(|item| item.as_u64()).unwrap_or(1) as u32,
        libsignal_protocol::DeviceId::new(1).expect("id"),
        None,
        libsignal_protocol::SignedPreKeyId::from(1u32),
        signed_public,
        b64("signedSignature")?,
        libsignal_protocol::KyberPreKeyId::from(1u32),
        kyber_public,
        b64("kyberSignature")?,
        identity,
    ).map_err(|err| crate::error::AppError::Message(err.to_string()))
}

/// Kind byte placed in front of every Signal frame.
const SIGNAL_WHISPER: u8 = 2;
const SIGNAL_PREKEY: u8 = 3;
const SIGNAL_GROUP: u8 = 4;

fn tag_ciphertext(message: &libsignal_protocol::CiphertextMessage) -> Vec<u8> {
    let kind = match message {
        libsignal_protocol::CiphertextMessage::PreKeySignalMessage(_) => SIGNAL_PREKEY,
        _ => SIGNAL_WHISPER,
    };
    let mut out = vec![kind];
    out.extend_from_slice(message.serialize());
    out
}

fn tag_group(serialized: &[u8]) -> Vec<u8> {
    let mut out = vec![SIGNAL_GROUP];
    out.extend_from_slice(serialized);
    out
}

fn decode_ciphertext(kind: u8, bytes: &[u8]) -> Option<libsignal_protocol::CiphertextMessage> {
    match kind {
        SIGNAL_WHISPER => libsignal_protocol::SignalMessage::try_from(bytes).ok().map(libsignal_protocol::CiphertextMessage::SignalMessage),
        SIGNAL_PREKEY => libsignal_protocol::PreKeySignalMessage::try_from(bytes).ok().map(libsignal_protocol::CiphertextMessage::PreKeySignalMessage),
        _ => None,
    }
}

#[derive(Clone)]
struct IdentityParts {
    device_id: String,
    cert_der: Vec<u8>,
    cert_key: Vec<u8>,
}

impl IdentityMaterial {
    fn clone_parts(&self) -> IdentityParts {
        IdentityParts {
            device_id: self.device_id.clone(),
            cert_der: self.cert_der.clone(),
            cert_key: self.cert_key_pkcs8.clone(),
        }
    }
}

pub fn choose_download_source(sources: &[PlaySource], failed: Option<&PlaySource>) -> SourceChoice {
    sync_logic::select_source(sources, sync_logic::FailureClass::Transport, failed, false)
}

#[cfg(test)]
mod disconnect_tests {
    use super::{redial_ids, LiveInner};
    use std::collections::HashSet;

    #[test]
    fn a_hold_from_either_side_stops_automatic_redial() {
        let mut live = LiveInner::default();
        live.paused.insert("we-disconnected".into());
        live.held.insert("they-disconnected".into());
        let saved = vec!["we-disconnected".to_string(), "they-disconnected".to_string(), "offline".to_string()];
        assert_eq!(redial_ids(saved, |_| false, &live.on_hold()), vec!["offline".to_string()]);
    }

    #[test]
    fn a_manually_disconnected_device_is_not_dialed_again() {
        let saved = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let paused: HashSet<String> = ["b".to_string()].into_iter().collect();
        assert_eq!(redial_ids(saved.clone(), |id| id == "a", &paused), vec!["c".to_string()]);
        // After Connect the hold is lifted and the device is dialed again.
        assert_eq!(redial_ids(saved, |id| id == "a", &HashSet::new()), vec!["b".to_string(), "c".to_string()]);
    }
}
