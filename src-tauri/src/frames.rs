// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::io;

pub const MAX_PAYLOAD: usize = 32 * 1024;

#[derive(Debug, Clone)]
pub enum Frame {
    Keepalive,
    /// One JSON app message: sync, history, holdings, snapshots, hello.
    Message(Vec<u8>),
    AudioReq {
        id: u64,
        content_id: String,
        start: u64,
        end: Option<u64>,
    },
    AudioChunk {
        id: u64,
        offset: u64,
        eof: bool,
        data: Vec<u8>,
    },
    AudioErr { id: u64, reason: String },
    AudioCancel { id: u64 },
    AudioHead { id: u64, total: u64 },
    /// Pairing, asking side: its SPAKE2 message, made with the code the user typed.
    PairStart { message: Vec<u8> },
    /// Pairing, asked side: its SPAKE2 message and its confirmation tag.
    PairAnswer { message: Vec<u8>, tag: [u8; 32] },
    /// Pairing, asking side: its confirmation tag. Both sides are paired once it checks out.
    PairConfirm { tag: [u8; 32] },
    /// Pairing stopped: "wrong", "expired", "locked", "busy" or "cancelled".
    PairFail { reason: String },
    /// The sender disconnected this device on purpose, so do not dial it again until it comes back
    /// or the user presses Connect. Empty payload.
    Hold,
    /// The sender no longer has this device saved. `pair: false`: it was removed there, so forget the
    /// sender and stay quiet. `pair: true`: the user is pairing again from the sender, so forget the
    /// sender and show a new code. One byte.
    Unpaired { pair: bool },
}

impl Frame {
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let (kind, payload) = match self {
            Self::Keepalive => (1u8, Vec::new()),
            Self::AudioReq { id, content_id, start, end } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&id.to_be_bytes());
                payload.extend_from_slice(&start.to_be_bytes());
                payload.extend_from_slice(&end.unwrap_or(u64::MAX).to_be_bytes());
                payload.extend_from_slice(content_id.as_bytes());
                (4, payload)
            }
            Self::AudioChunk { id, offset, eof, data } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&id.to_be_bytes());
                payload.extend_from_slice(&offset.to_be_bytes());
                payload.push(u8::from(*eof));
                payload.extend_from_slice(data);
                (5, payload)
            }
            Self::AudioErr { id, reason } => {
                let mut payload = id.to_be_bytes().to_vec();
                payload.extend_from_slice(reason.as_bytes());
                (6, payload)
            }
            Self::AudioCancel { id } => (7, id.to_be_bytes().to_vec()),
            Self::AudioHead { id, total } => {
                let mut payload = id.to_be_bytes().to_vec();
                payload.extend_from_slice(&total.to_be_bytes());
                (8, payload)
            }
            Self::PairStart { message } => (12, message.clone()),
            Self::PairAnswer { message, tag } => (13, [tag.as_slice(), message].concat()),
            Self::PairConfirm { tag } => (14, tag.to_vec()),
            Self::PairFail { reason } => (15, reason.as_bytes().to_vec()),
            Self::Hold => (10, Vec::new()),
            Self::Unpaired { pair } => (16, vec![u8::from(*pair)]),
            Self::Message(bytes) => (11, bytes.clone()),
        };
        if payload.len() > MAX_PAYLOAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
        }
        let mut out = Vec::with_capacity(5 + payload.len());
        out.push(kind);
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&payload);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 5 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short frame"));
        }
        let kind = bytes[0];
        let len = u32::from_be_bytes(bytes[1..5].try_into().unwrap()) as usize;
        if len > MAX_PAYLOAD || bytes.len() != 5 + len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad frame length"));
        }
        let payload = &bytes[5..];
        let need = match kind {
            4 => 24,
            5 => 17,
            6 | 7 => 8,
            8 => 16,
            13 | 14 => 32,
            16 => 1,
            _ => 0,
        };
        if payload.len() < need {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "short payload"));
        }
        match kind {
            1 => Ok(Self::Keepalive),
            4 => {
                let id = u64::from_be_bytes(payload[0..8].try_into().unwrap());
                let start = u64::from_be_bytes(payload[8..16].try_into().unwrap());
                let end_raw = u64::from_be_bytes(payload[16..24].try_into().unwrap());
                let content_id = String::from_utf8_lossy(&payload[24..]).to_string();
                Ok(Self::AudioReq {
                    id,
                    content_id,
                    start,
                    end: if end_raw == u64::MAX { None } else { Some(end_raw) },
                })
            }
            5 => {
                let id = u64::from_be_bytes(payload[0..8].try_into().unwrap());
                let offset = u64::from_be_bytes(payload[8..16].try_into().unwrap());
                let eof = payload[16] == 1;
                Ok(Self::AudioChunk {
                    id,
                    offset,
                    eof,
                    data: payload[17..].to_vec(),
                })
            }
            6 => Ok(Self::AudioErr {
                id: u64::from_be_bytes(payload[0..8].try_into().unwrap()),
                reason: String::from_utf8_lossy(&payload[8..]).to_string(),
            }),
            7 => Ok(Self::AudioCancel {
                id: u64::from_be_bytes(payload[0..8].try_into().unwrap()),
            }),
            8 => Ok(Self::AudioHead {
                id: u64::from_be_bytes(payload[0..8].try_into().unwrap()),
                total: u64::from_be_bytes(payload[8..16].try_into().unwrap()),
            }),
            10 => Ok(Self::Hold),
            11 => Ok(Self::Message(payload.to_vec())),
            12 => Ok(Self::PairStart { message: payload.to_vec() }),
            13 => Ok(Self::PairAnswer {
                tag: payload[..32].try_into().unwrap(),
                message: payload[32..].to_vec(),
            }),
            14 => Ok(Self::PairConfirm { tag: payload[..32].try_into().unwrap() }),
            16 => Ok(Self::Unpaired { pair: payload[0] == 1 }),
            15 => Ok(Self::PairFail { reason: String::from_utf8_lossy(payload).chars().take(32).collect() }),
            // A frame kind from a newer version: skip it instead of dropping the whole connection.
            _ => Ok(Self::Keepalive),
        }
    }
}

pub async fn write_frame(stream: &mut (impl tokio::io::AsyncWriteExt + Unpin), frame: &Frame) -> io::Result<()> {
    let bytes = frame.encode()?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}

pub async fn read_frame(stream: &mut (impl tokio::io::AsyncReadExt + Unpin)) -> io::Result<Frame> {
    let mut header = [0u8; 5];
    stream.read_exact(&mut header).await?;
    let len = u32::from_be_bytes(header[1..5].try_into().unwrap()) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    let mut all = header.to_vec();
    all.extend_from_slice(&body);
    Frame::decode(&all)
}

#[cfg(test)]
mod tests {
    use super::Frame;

    #[test]
    fn unknown_frame_kinds_are_skipped() {
        assert!(matches!(Frame::decode(&[200, 0, 0, 0, 2, 1, 2]), Ok(Frame::Keepalive)));
        let encoded = Frame::AudioCancel { id: 7 }.encode().unwrap();
        assert!(matches!(Frame::decode(&encoded), Ok(Frame::AudioCancel { id: 7 })));
        assert!(Frame::decode(&[200, 0, 0, 0, 9, 1]).is_err());
    }

    #[test]
    fn hold_frame_round_trips() {
        let encoded = Frame::Hold.encode().unwrap();
        assert_eq!(encoded, vec![10, 0, 0, 0, 0]);
        assert!(matches!(Frame::decode(&encoded), Ok(Frame::Hold)));
    }

    #[test]
    fn message_frames_round_trip_within_the_size_limit() {
        let body = br#"{"kind":"hello","name":"Desk"}"#.to_vec();
        let encoded = Frame::Message(body.clone()).encode().unwrap();
        assert!(matches!(Frame::decode(&encoded), Ok(Frame::Message(bytes)) if bytes == body));
        assert!(Frame::Message(vec![b' '; super::MAX_PAYLOAD + 1]).encode().is_err());
        assert!(matches!(Frame::decode(&[2, 0, 0, 0, 1, 0]), Ok(Frame::Keepalive)));
    }

    #[test]
    fn pairing_frames_round_trip() {
        let start = Frame::PairStart { message: vec![0x41; 33] }.encode().unwrap();
        assert!(matches!(Frame::decode(&start), Ok(Frame::PairStart { message }) if message == vec![0x41; 33]));
        let answer = Frame::PairAnswer { message: vec![0x42; 33], tag: [7; 32] }.encode().unwrap();
        assert!(matches!(Frame::decode(&answer), Ok(Frame::PairAnswer { message, tag }) if message == vec![0x42; 33] && tag == [7; 32]));
        let confirm = Frame::PairConfirm { tag: [9; 32] }.encode().unwrap();
        assert!(matches!(Frame::decode(&confirm), Ok(Frame::PairConfirm { tag }) if tag == [9; 32]));
        let fail = Frame::PairFail { reason: "locked".into() }.encode().unwrap();
        assert!(matches!(Frame::decode(&fail), Ok(Frame::PairFail { reason }) if reason == "locked"));
        // A tag shorter than 32 bytes is refused instead of read past the end.
        assert!(Frame::decode(&[14, 0, 0, 0, 3, 1, 2, 3]).is_err());
        assert!(Frame::decode(&[13, 0, 0, 0, 1, 1]).is_err());
    }

    #[test]
    fn unpaired_frame_round_trips() {
        assert_eq!(Frame::Unpaired { pair: false }.encode().unwrap(), vec![16, 0, 0, 0, 1, 0]);
        assert_eq!(Frame::Unpaired { pair: true }.encode().unwrap(), vec![16, 0, 0, 0, 1, 1]);
        assert!(matches!(Frame::decode(&[16, 0, 0, 0, 1, 0]), Ok(Frame::Unpaired { pair: false })));
        assert!(matches!(Frame::decode(&[16, 0, 0, 0, 1, 1]), Ok(Frame::Unpaired { pair: true })));
        // An empty payload is refused instead of read past the end.
        assert!(Frame::decode(&[16, 0, 0, 0, 0]).is_err());
    }
}
