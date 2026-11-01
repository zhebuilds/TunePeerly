// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rand::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct AudioPiece {
    pub offset: u64,
    pub eof: bool,
    pub data: Vec<u8>,
    pub error: Option<String>,
    /// Sent once before the first bytes so the proxy can answer with the real length.
    pub total: Option<u64>,
}

impl AudioPiece {
    pub fn chunk(offset: u64, eof: bool, data: Vec<u8>) -> Self {
        Self { offset, eof, data, error: None, total: None }
    }

    pub fn failed(reason: &str) -> Self {
        Self { offset: 0, eof: true, data: Vec::new(), error: Some(reason.to_string()), total: None }
    }

    pub fn head(total: u64) -> Self {
        Self { offset: 0, eof: false, data: Vec::new(), error: None, total: Some(total) }
    }
}

pub enum Pull {
    Open {
        id: u64,
        device_id: String,
        content_id: String,
        start: u64,
        end: Option<u64>,
        reply: mpsc::UnboundedSender<AudioPiece>,
        /// Bytes delivered but not yet taken by the reader.
        queued: Arc<AtomicUsize>,
    },
    Cancel {
        id: u64,
        device_id: String,
    },
}

#[derive(Clone)]
struct Route {
    content_id: String,
    device_id: String,
    created: std::time::Instant,
}

const ROUTE_LIMIT: usize = 32;

/// Bytes one transfer may hold in memory before its reader catches up. The player stops reading once it
/// has buffered enough, while the other device keeps sending; past this the transfer is dropped and the
/// player asks again from where it is.
pub const QUEUE_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone)]
struct Waiter {
    sender: mpsc::UnboundedSender<AudioPiece>,
    queued: Option<Arc<AtomicUsize>>,
}

/// Picks an online device holding a song, skipping the given devices. Installed by the app (see `lib.rs`).
pub type HolderPicker = Arc<dyn Fn(&str, &[String]) -> Option<String> + Send + Sync>;

/// How long a device that failed to deliver a song is skipped for that song on stream URLs.
const STREAM_FAILURE_MEMORY: Duration = Duration::from_secs(60);

pub struct AudioBus {
    waiters: Mutex<HashMap<u64, Waiter>>,
    routes: Mutex<HashMap<String, Route>>,
    pub base: Mutex<String>,
    /// Secret part of the stream URLs (`stream_base`), new on every start.
    secret: String,
    picker: Mutex<Option<HolderPicker>>,
    /// Devices that recently failed a stream request, per content id.
    stream_failures: Mutex<HashMap<String, Vec<(String, std::time::Instant)>>>,
}

impl AudioBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            waiters: Mutex::new(HashMap::new()),
            routes: Mutex::new(HashMap::new()),
            base: Mutex::new(String::new()),
            secret: random_hex(),
            picker: Mutex::new(None),
            stream_failures: Mutex::new(HashMap::new()),
        })
    }

    pub fn set_picker(&self, picker: HolderPicker) {
        *self.picker.lock().unwrap_or_else(PoisonError::into_inner) = Some(picker);
    }

    /// A song's stream URL is this followed by its content id. Unlike `open_route`, such a URL does not
    /// expire and picks the device to read from on every request, so a player can reach any song of the
    /// queue, and retry it after a dropped link, without asking the page for a new URL first.
    pub fn stream_base(&self) -> String {
        format!("{}/s{}.", self.base.lock().unwrap_or_else(PoisonError::into_inner), self.secret)
    }

    /// The route a stream URL token stands for right now, or None when it is not one or no device has the song.
    fn stream_route(&self, token: &str) -> Option<Result<Route, ()>> {
        let rest = token.strip_prefix('s')?.strip_prefix(self.secret.as_str())?.strip_prefix('.')?;
        if rest.is_empty() || rest.len() > 200 || !rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Some(Err(()));
        }
        let skip: Vec<String> = {
            let mut failures = self.stream_failures.lock().unwrap_or_else(PoisonError::into_inner);
            failures.retain(|_, list| {
                list.retain(|(_, at)| at.elapsed() < STREAM_FAILURE_MEMORY);
                !list.is_empty()
            });
            failures.get(rest).map(|list| list.iter().map(|(device, _)| device.clone()).collect()).unwrap_or_default()
        };
        let picker = self.picker.lock().unwrap_or_else(PoisonError::into_inner).clone()?;
        // Every device failed recently: try them all again rather than give up.
        let device = picker(rest, &skip).or_else(|| if skip.is_empty() { None } else { picker(rest, &[]) });
        Some(device.map(|device_id| Route { content_id: rest.to_string(), device_id, created: std::time::Instant::now() }).ok_or(()))
    }

    fn stream_failed(&self, content_id: &str, device_id: &str) {
        let mut failures = self.stream_failures.lock().unwrap_or_else(PoisonError::into_inner);
        let list = failures.entry(content_id.to_string()).or_default();
        list.retain(|(device, _)| device != device_id);
        list.push((device_id.to_string(), std::time::Instant::now()));
    }

    pub fn open_route(&self, content_id: &str, device_id: &str) -> String {
        let token = random_hex();
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        // Playback fetches one token many times (ranges), so tokens cannot be spent. Keep only the newest few.
        while routes.len() >= ROUTE_LIMIT {
            let Some(oldest) = routes.iter().min_by_key(|(_, route)| route.created).map(|(key, _)| key.clone()) else { break };
            routes.remove(&oldest);
        }
        routes.insert(
            token.clone(),
            Route {
                content_id: content_id.to_string(),
                device_id: device_id.to_string(),
                created: std::time::Instant::now(),
            },
        );
        drop(routes);
        format!("{}/{token}", self.base.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Drops every playback token and fails transfers in flight.
    pub fn close_routes(&self) {
        self.routes.lock().unwrap_or_else(PoisonError::into_inner).clear();
        let waiters = std::mem::take(&mut *self.waiters.lock().unwrap_or_else(PoisonError::into_inner));
        for (_, waiter) in waiters {
            let _ = waiter.sender.send(AudioPiece::failed("cancelled"));
        }
    }

    pub fn register(&self, id: u64, sender: mpsc::UnboundedSender<AudioPiece>) {
        self.waiters.lock().unwrap_or_else(PoisonError::into_inner).insert(id, Waiter { sender, queued: None });
    }

    /// Like `register`, but the transfer is dropped once more than `QUEUE_LIMIT` bytes wait in `queued`.
    /// The reader subtracts what it takes (see `taken`).
    pub fn register_limited(&self, id: u64, sender: mpsc::UnboundedSender<AudioPiece>, queued: Arc<AtomicUsize>) {
        self.waiters.lock().unwrap_or_else(PoisonError::into_inner).insert(id, Waiter { sender, queued: Some(queued) });
    }

    /// Returns false when the transfer was dropped because its reader fell too far behind;
    /// the sender should then be told to stop.
    pub fn deliver(&self, id: u64, piece: AudioPiece) -> bool {
        let waiter = self.waiters.lock().unwrap_or_else(PoisonError::into_inner).get(&id).cloned();
        let Some(waiter) = waiter else { return true };
        if let Some(queued) = &waiter.queued {
            let size = piece.data.len();
            if queued.fetch_add(size, Ordering::Relaxed).saturating_add(size) > QUEUE_LIMIT {
                self.waiters.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);
                let _ = waiter.sender.send(AudioPiece::failed("busy"));
                return false;
            }
        }
        let last = piece.eof || piece.error.is_some();
        if waiter.sender.send(piece).is_err() || last {
            self.waiters.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);
        }
        true
    }

    pub fn finish(&self, id: u64) {
        self.waiters.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);
    }
}

fn random_hex() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
}

/// Starts the loopback media proxy and signals once route URLs are usable.
pub async fn serve(
    bus: Arc<AudioBus>,
    pulls: mpsc::UnboundedSender<Pull>,
    ready: std::sync::mpsc::SyncSender<()>,
) {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else { return };
    if let Ok(addr) = listener.local_addr() {
        *bus.base.lock().unwrap_or_else(PoisonError::into_inner) = format!("http://127.0.0.1:{}", addr.port());
    }
    let _ = ready.send(());
    loop {
        let socket = match listener.accept().await {
            Ok((socket, _)) => socket,
            Err(_) => {
                // For example too many open sockets; wait instead of spinning.
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };
        let bus = bus.clone();
        let pulls = pulls.clone();
        tokio::task::spawn_local(async move {
            let _ = handle(socket, bus, pulls).await;
        });
    }
}

const CORS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Range\r\nAccess-Control-Expose-Headers: Content-Length, Content-Range, Accept-Ranges\r\n";

fn status_response(code: u16, reason: &str) -> String {
    format!("HTTP/1.1 {code} {reason}\r\n{CORS}Connection: close\r\n\r\n")
}

async fn handle(mut socket: TcpStream, bus: Arc<AudioBus>, pulls: mpsc::UnboundedSender<Pull>) -> std::io::Result<()> {
    // A local process can connect without ever finishing its HTTP headers.  Do
    // not leave one task alive indefinitely for each such connection.
    let header = match tokio::time::timeout(Duration::from_secs(10), read_header(&mut socket)).await {
        Ok(Ok(header)) => header,
        _ => {
            socket.write_all(status_response(400, "Bad Request").as_bytes()).await?;
            return Ok(());
        }
    };
    // Range makes the playback probe a preflight. Answer it before opening a transfer.
    if header.starts_with(b"OPTIONS ") {
        socket
            .write_all(format!("HTTP/1.1 204 No Content\r\n{CORS}Content-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
            .await?;
        return Ok(());
    }
    let Some(request) = parse_request(&header) else {
        socket.write_all(status_response(400, "Bad Request").as_bytes()).await?;
        return Ok(());
    };
    let stream = bus.stream_route(&request.token);
    let streamed = stream.is_some();
    let route = match stream {
        Some(Ok(route)) => Some(route),
        Some(Err(())) => {
            // A stream URL, but no device that has the song is connected now. The player retries.
            socket.write_all(status_response(503, "Service Unavailable").as_bytes()).await?;
            return Ok(());
        }
        None => bus.routes.lock().unwrap_or_else(PoisonError::into_inner).get(&request.token).cloned(),
    };
    let Some(route) = route else {
        socket.write_all(status_response(404, "Not Found").as_bytes()).await?;
        return Ok(());
    };
    let (mut start, end, suffix) = match &request.range {
        None => (0, None, None),
        Some(ByteRange::From(start, end)) => (*start, *end, None),
        Some(ByteRange::Last(count)) => (0, None, Some(*count)),
    };
    let device_id = route.device_id.clone();
    let mut opened = match open_pull(&bus, &pulls, &route.device_id, &route.content_id, start, end).await {
        Ok(opened) => opened,
        Err((code, reason)) => {
            if streamed && code != 416 {
                bus.stream_failed(&route.content_id, &route.device_id);
            }
            socket.write_all(status_response(code, reason).as_bytes()).await?;
            return Ok(());
        }
    };
    // "bytes=-N" asks for the last N bytes. The length is only known once the peer answers.
    if let Some(count) = suffix {
        if count == 0 {
            bus.finish(opened.id);
            let _ = pulls.send(Pull::Cancel { id: opened.id, device_id: device_id.clone() });
            socket.write_all(status_response(416, "Range Not Satisfiable").as_bytes()).await?;
            return Ok(());
        }
        start = opened.total.saturating_sub(count);
        if start > 0 {
            bus.finish(opened.id);
            let _ = pulls.send(Pull::Cancel { id: opened.id, device_id: device_id.clone() });
            opened = match open_pull(&bus, &pulls, &route.device_id, &route.content_id, start, None).await {
                Ok(opened) => opened,
                Err((code, reason)) => {
                    socket.write_all(status_response(code, reason).as_bytes()).await?;
                    return Ok(());
                }
            };
        }
    }
    let Opened { id, mut rx, queued, total } = opened;
    let cancel = |pulls: &mpsc::UnboundedSender<Pull>| {
        let _ = pulls.send(Pull::Cancel { id, device_id: device_id.clone() });
    };
    let stop = if request.range.is_some() {
        let Some(stop) = range_stop(start, end, total) else {
            bus.finish(id);
            cancel(&pulls);
            socket.write_all(status_response(416, "Range Not Satisfiable").as_bytes()).await?;
            return Ok(());
        };
        stop
    } else {
        total
    };
    let length = stop.saturating_sub(start);
    let head = if request.range.is_some() {
        format!(
            "HTTP/1.1 206 Partial Content\r\n{CORS}Content-Type: application/octet-stream\r\nContent-Length: {length}\r\nContent-Range: bytes {start}-{}/{total}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            stop.saturating_sub(1)
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\n{CORS}Content-Type: application/octet-stream\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
        )
    };
    if socket.write_all(head.as_bytes()).await.is_err() || request.head_only {
        bus.finish(id);
        cancel(&pulls);
        return Ok(());
    }

    let mut written = start;
    // The other device stopped sending, as opposed to the player hanging up (a seek or a stop).
    let mut peer_failed = false;
    loop {
        let piece = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await;
        let Ok(Some(piece)) = piece else {
            peer_failed = true;
            break;
        };
        taken(&queued, &piece);
        if piece.error.is_some() {
            peer_failed = piece.error.as_deref() != Some("cancelled");
            break;
        }
        if piece.offset > written {
            peer_failed = true;
            break;
        }
        let skip = (written - piece.offset) as usize;
        if skip < piece.data.len() {
            let room = stop.saturating_sub(written) as usize;
            let slice = &piece.data[skip..piece.data.len().min(skip + room)];
            if socket.write_all(slice).await.is_err() {
                break;
            }
            written += slice.len() as u64;
        }
        if piece.eof || written >= stop {
            break;
        }
    }
    bus.finish(id);
    if written < stop {
        cancel(&pulls);
        if streamed && peer_failed {
            bus.stream_failed(&route.content_id, &route.device_id);
        }
    }
    let _ = socket.shutdown().await;
    Ok(())
}

/// The reader took `piece` off the queue.
pub fn taken(queued: &AtomicUsize, piece: &AudioPiece) {
    let size = piece.data.len();
    let mut current = queued.load(Ordering::Relaxed);
    loop {
        match queued.compare_exchange_weak(current, current.saturating_sub(size), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(seen) => current = seen,
        }
    }
}

struct Opened {
    id: u64,
    rx: mpsc::UnboundedReceiver<AudioPiece>,
    queued: Arc<AtomicUsize>,
    total: u64,
}

/// Asks the peer for the song from `start` and waits for its first answer, which carries the full length.
async fn open_pull(
    bus: &AudioBus,
    pulls: &mpsc::UnboundedSender<Pull>,
    device_id: &str,
    content_id: &str,
    start: u64,
    end: Option<u64>,
) -> Result<Opened, (u16, &'static str)> {
    let id = rand::random::<u64>();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let queued = Arc::new(AtomicUsize::new(0));
    if pulls.send(Pull::Open {
        id,
        device_id: device_id.to_string(),
        content_id: content_id.to_string(),
        start,
        end,
        reply: tx,
        queued: queued.clone(),
    }).is_err() {
        return Err((502, "Bad Gateway"));
    }
    let first = tokio::time::timeout(Duration::from_secs(15), rx.recv()).await;
    let failed = match first {
        Ok(Some(piece)) if piece.error.is_none() && piece.total.is_some() => {
            return Ok(Opened { id, rx, queued, total: piece.total.unwrap_or(0) });
        }
        Ok(Some(piece)) if piece.error.as_deref() == Some("range") => (416, "Range Not Satisfiable"),
        _ => (502, "Bad Gateway"),
    };
    bus.finish(id);
    let _ = pulls.send(Pull::Cancel { id, device_id: device_id.to_string() });
    Err(failed)
}

async fn read_header(socket: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    while !buf.windows(4).any(|window| window == b"\r\n\r\n") && buf.len() < 8192 {
        let n = socket.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    if !buf.windows(4).any(|window| window == b"\r\n\r\n") {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "incomplete HTTP header"));
    }
    Ok(buf)
}

#[derive(Debug, PartialEq)]
enum ByteRange {
    /// From a byte, up to an exclusive end or the end of the file.
    From(u64, Option<u64>),
    /// The last this many bytes.
    Last(u64),
}

struct Request {
    token: String,
    range: Option<ByteRange>,
    head_only: bool,
}

fn parse_request(bytes: &[u8]) -> Option<Request> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.split("\r\n");
    let mut request = lines.next()?.split_whitespace();
    let method = request.next()?;
    if method != "GET" && method != "HEAD" {
        return None;
    }
    let token = request.next()?.trim_start_matches('/').split('?').next()?.to_string();
    let version = request.next()?;
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") || request.next().is_some() || token.is_empty() || token.contains('/') {
        return None;
    }
    let mut range = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.eq_ignore_ascii_case("range") {
            range = parse_range(value.trim());
        }
    }
    Some(Request { token, range, head_only: method == "HEAD" })
}

fn parse_range(value: &str) -> Option<ByteRange> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        return Some(ByteRange::Last(end.parse().ok()?));
    }
    let start = start.parse().ok()?;
    let end = if end.is_empty() { None } else { Some(end.parse::<u64>().ok()?.saturating_add(1)) };
    Some(ByteRange::From(start, end))
}

/// Resolves an inclusive HTTP byte range (stored internally with an exclusive
/// end) against a known object length.  A `Range` request may not describe an
/// empty response: those must use HTTP 416 rather than a malformed 206 header.
fn range_stop(start: u64, end: Option<u64>, total: u64) -> Option<u64> {
    let stop = end.unwrap_or(total).min(total);
    (start < total && stop > start).then_some(stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_and_closed_ranges() {
        assert_eq!(parse_range("bytes=0-"), Some(ByteRange::From(0, None)));
        assert_eq!(parse_range("bytes=10-19"), Some(ByteRange::From(10, Some(20))));
        assert_eq!(parse_range("items=1-2"), None);
    }

    #[test]
    fn parses_suffix_ranges() {
        assert_eq!(parse_range("bytes=-500"), Some(ByteRange::Last(500)));
        assert_eq!(parse_range("bytes=-0"), Some(ByteRange::Last(0)));
        assert_eq!(parse_range("bytes=-"), None);
        assert_eq!(parse_range("bytes=-x"), None);
    }

    #[test]
    fn rejects_unsatisfiable_or_empty_ranges() {
        assert_eq!(range_stop(10, Some(10), 20), None);
        assert_eq!(range_stop(10, Some(9), 20), None);
        assert_eq!(range_stop(20, None, 20), None);
        assert_eq!(range_stop(0, None, 0), None);
        assert_eq!(range_stop(10, Some(99), 20), Some(20));
    }

    #[test]
    fn stream_urls_pick_a_device_per_request_and_skip_recent_failures() {
        let bus = AudioBus::new();
        *bus.base.lock().unwrap() = "http://127.0.0.1:9".into();
        let base = bus.stream_base();
        assert!(base.starts_with("http://127.0.0.1:9/s") && base.ends_with('.'));
        let token = format!("{}abc", base.trim_start_matches("http://127.0.0.1:9/"));
        // No picker yet: nothing can serve it.
        assert!(matches!(bus.stream_route(&token), None));
        bus.set_picker(Arc::new(|content: &str, skip: &[String]| {
            assert_eq!(content, "abc");
            ["one", "two"].iter().find(|device| !skip.iter().any(|s| s == *device)).map(|d| d.to_string())
        }));
        assert_eq!(bus.stream_route(&token).unwrap().unwrap().device_id, "one");
        bus.stream_failed("abc", "one");
        assert_eq!(bus.stream_route(&token).unwrap().unwrap().device_id, "two");
        bus.stream_failed("abc", "two");
        // Both failed lately: start over instead of giving up.
        assert_eq!(bus.stream_route(&token).unwrap().unwrap().device_id, "one");
        // Wrong secret, a bad id, or an ordinary route token are not stream URLs.
        assert!(bus.stream_route("sdeadbeef.abc").is_none());
        assert!(matches!(bus.stream_route(&format!("{}a.b", token.trim_end_matches("abc"))), Some(Err(()))));
        assert!(bus.stream_route("0123456789abcdef0123456789abcdef").is_none());
    }

    #[test]
    fn parses_request_token_and_method() {
        let request = parse_request(b"HEAD /abc?x=1 HTTP/1.1\r\nRange: bytes=5-\r\n\r\n").expect("request");
        assert_eq!(request.token, "abc");
        assert_eq!(request.range, Some(ByteRange::From(5, None)));
        assert!(request.head_only);
        assert!(parse_request(b"POST /abc HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_request(b"GET /abc\r\n\r\n").is_none());
    }
}
