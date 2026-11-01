// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
//! Pairing with a short code.
//!
//! The device that was asked to pair shows a 6-digit code. The device that asked types it in. Both
//! run SPAKE2 with the code over the TLS connection that is already open between them, so the code
//! itself never crosses the network and a wrong guess teaches nothing about the right one.
//!
//! The SPAKE2 identities are the TLS public keys (SubjectPublicKeyInfo) each side sees: the asking
//! device's key first, then the asked device's key. Both confirmation tags cover those keys and both
//! SPAKE2 messages again. Someone in the middle has to run two TLS sessions with keys of its own, so
//! the two ends disagree about the keys, derive different secrets and the tags do not match, even
//! when the person in the middle knows the code.

use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

/// How long a shown code can be used.
pub const CODE_LIFETIME: Duration = Duration::from_secs(90);
/// How long the asked device waits for the other side's tag after answering.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(15);
/// Wrong codes in a row before pairing stops for a while.
pub const MAX_WRONG: u32 = 3;
/// How long pairing stays stopped after too many wrong codes.
pub const LOCK_TIME: Duration = Duration::from_secs(60);

const LABEL: &[u8] = b"tunepeerly-pair-v1";
const TAG_ASKED: &[u8] = b"asked device confirms";
const TAG_ASKING: &[u8] = b"asking device confirms";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairError {
    /// The code was wrong, or the two sides saw different TLS keys.
    Mismatch,
    /// The other side sent something that is not a SPAKE2 message.
    Malformed,
}

/// A fresh 6-digit code from the system's secure random generator, every value equally likely.
pub fn new_code() -> String {
    format!("{:06}", rand::rng().random_range(0..1_000_000u32))
}

/// Keeps only digits, so "123 456" and "123-456" both work. Anything but exactly six digits is None.
pub fn clean_code(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(|ch| !ch.is_whitespace() && *ch != '-').collect();
    (digits.len() == 6 && digits.chars().all(|ch| ch.is_ascii_digit())).then_some(digits)
}

fn password(code: &str) -> Password {
    Password::new([LABEL, b" code ", code.as_bytes()].concat())
}

fn mac(key: &[u8], which: &[u8], asking_key: &[u8], asked_key: &[u8], asking_msg: &[u8], asked_msg: &[u8]) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes any key length");
    // Every part is length-prefixed, so no two different sets of parts give the same input.
    for part in [LABEL, which, asking_key, asked_key, asking_msg, asked_msg] {
        mac.update(&(part.len() as u64).to_be_bytes());
        mac.update(part);
    }
    mac
}

fn tag(key: &[u8], which: &[u8], asking_key: &[u8], asked_key: &[u8], asking_msg: &[u8], asked_msg: &[u8]) -> [u8; 32] {
    mac(key, which, asking_key, asked_key, asking_msg, asked_msg).finalize().into_bytes().into()
}

fn tag_matches(key: &[u8], which: &[u8], asking_key: &[u8], asked_key: &[u8], asking_msg: &[u8], asked_msg: &[u8], seen: &[u8]) -> bool {
    // Constant time, so timing does not reveal how much of a tag was right.
    mac(key, which, asking_key, asked_key, asking_msg, asked_msg).verify_slice(seen).is_ok()
}

/// The asking side, between sending its message and hearing back.
pub struct Asking {
    state: Spake2<Ed25519Group>,
    own_key: Vec<u8>,
    peer_key: Vec<u8>,
    message: Vec<u8>,
}

impl Asking {
    /// Starts with the typed code, this device's TLS key and the key the other device showed in TLS.
    /// Returns the state to keep and the message to send.
    pub fn start(code: &str, own_key: &[u8], peer_key: &[u8]) -> (Self, Vec<u8>) {
        let (state, message) = Spake2::<Ed25519Group>::start_a(&password(code), &Identity::new(own_key), &Identity::new(peer_key));
        let asking = Self { state, own_key: own_key.to_vec(), peer_key: peer_key.to_vec(), message: message.clone() };
        (asking, message)
    }

    /// Checks the asked device's answer. On success returns the tag to send back; both sides are then paired.
    pub fn finish(self, answer: &[u8], answer_tag: &[u8]) -> Result<[u8; 32], PairError> {
        let key = self.state.finish(answer).map_err(|_| PairError::Malformed)?;
        if !tag_matches(&key, TAG_ASKED, &self.own_key, &self.peer_key, &self.message, answer, answer_tag) {
            return Err(PairError::Mismatch);
        }
        Ok(tag(&key, TAG_ASKING, &self.own_key, &self.peer_key, &self.message, answer))
    }
}

/// The asked side's answer: the message and tag to send, and what the asking side's tag must be.
pub struct Answer {
    pub message: Vec<u8>,
    pub tag: [u8; 32],
    expected: Expected,
}

/// What the asked side keeps until the asking side's tag arrives.
pub struct Expected {
    key: Vec<u8>,
    own_key: Vec<u8>,
    peer_key: Vec<u8>,
    asking_msg: Vec<u8>,
    asked_msg: Vec<u8>,
}

impl Expected {
    pub fn accepts(&self, seen: &[u8]) -> bool {
        tag_matches(&self.key, TAG_ASKING, &self.peer_key, &self.own_key, &self.asking_msg, &self.asked_msg, seen)
    }
}

impl Answer {
    pub fn into_expected(self) -> (Vec<u8>, [u8; 32], Expected) {
        (self.message, self.tag, self.expected)
    }
}

/// The asked side answers the asking side's message with the code it shows.
pub fn answer(code: &str, own_key: &[u8], peer_key: &[u8], asking_msg: &[u8]) -> Result<Answer, PairError> {
    let (state, message) = Spake2::<Ed25519Group>::start_b(&password(code), &Identity::new(peer_key), &Identity::new(own_key));
    let key = state.finish(asking_msg).map_err(|_| PairError::Malformed)?;
    let tag = tag(&key, TAG_ASKED, peer_key, own_key, asking_msg, &message);
    Ok(Answer {
        expected: Expected { key, own_key: own_key.to_vec(), peer_key: peer_key.to_vec(), asking_msg: asking_msg.to_vec(), asked_msg: message.clone() },
        message,
        tag,
    })
}

/// The asked device's side of one pairing: the code on screen, its age, wrong tries and the lock.
#[derive(Default)]
pub struct Guard {
    wrong: u32,
    locked_until: Option<Instant>,
}

impl Guard {
    /// True while pairing is stopped after too many wrong codes. An ended lock is cleared here.
    pub fn locked(&mut self, now: Instant) -> bool {
        match self.locked_until {
            Some(until) if now < until => true,
            Some(_) => {
                self.locked_until = None;
                self.wrong = 0;
                false
            }
            None => false,
        }
    }

    /// Seconds left on the lock, rounded up.
    pub fn lock_left(&self, now: Instant) -> u64 {
        self.locked_until.map(|until| until.saturating_duration_since(now)).map(|left| left.as_secs() + u64::from(left.subsec_nanos() > 0)).unwrap_or(0)
    }

    /// Counts one wrong code. Returns true when this one started the lock.
    pub fn wrong(&mut self, now: Instant) -> bool {
        self.wrong += 1;
        if self.wrong >= MAX_WRONG {
            self.locked_until = Some(now + LOCK_TIME);
            return true;
        }
        false
    }

    pub fn tries_left(&self) -> u32 {
        MAX_WRONG.saturating_sub(self.wrong)
    }

    pub fn succeeded(&mut self) {
        self.wrong = 0;
        self.locked_until = None;
    }
}

/// One code the asked device shows. It works once, and only until it expires.
pub struct Code {
    value: String,
    made: Instant,
}

impl Code {
    pub fn new(now: Instant) -> Self {
        Self { value: new_code(), made: now }
    }

    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.made) >= CODE_LIFETIME
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Hands the code out for a single try. The caller drops this `Code`, so it cannot be used again.
    pub fn use_once(self, now: Instant) -> Option<String> {
        (!self.expired(now)).then_some(self.value)
    }

    /// Seconds until the code expires, rounded up.
    pub fn seconds_left(&self, now: Instant) -> u64 {
        let left = CODE_LIFETIME.saturating_sub(now.saturating_duration_since(self.made));
        left.as_secs() + u64::from(left.subsec_nanos() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASKING_KEY: &[u8] = b"spki of the asking device";
    const ASKED_KEY: &[u8] = b"spki of the asked device";

    /// Runs one whole exchange. Each side passes the keys it saw in its own TLS session.
    fn run(asking_code: &str, asked_code: &str, asking_view: (&[u8], &[u8]), asked_view: (&[u8], &[u8])) -> (Result<[u8; 32], PairError>, Option<bool>) {
        let (asking, first) = Asking::start(asking_code, asking_view.0, asking_view.1);
        let (message, tag, expected) = answer(asked_code, asked_view.0, asked_view.1, &first).expect("answer").into_expected();
        match asking.finish(&message, &tag) {
            Ok(back) => (Ok(back), Some(expected.accepts(&back))),
            Err(err) => (Err(err), None),
        }
    }

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..200 {
            let code = new_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|ch| ch.is_ascii_digit()));
        }
        assert_eq!(clean_code(" 123 456 "), Some("123456".into()));
        assert_eq!(clean_code("123-456"), Some("123456".into()));
        assert_eq!(clean_code("12345"), None);
        assert_eq!(clean_code("1234567"), None);
        assert_eq!(clean_code("12a456"), None);
        assert_eq!(clean_code("１２３４５６"), None);
    }

    #[test]
    fn the_right_code_pairs_both_sides() {
        let (asking, asked) = run("042137", "042137", (ASKING_KEY, ASKED_KEY), (ASKED_KEY, ASKING_KEY));
        assert!(asking.is_ok());
        assert_eq!(asked, Some(true));
    }

    #[test]
    fn a_wrong_code_fails_on_both_sides() {
        let (asking, asked) = run("042137", "042138", (ASKING_KEY, ASKED_KEY), (ASKED_KEY, ASKING_KEY));
        assert_eq!(asking, Err(PairError::Mismatch));
        assert_eq!(asked, None);
    }

    #[test]
    fn the_asked_side_rejects_a_forged_tag() {
        let (asking, first) = Asking::start("555000", ASKING_KEY, ASKED_KEY);
        let (message, tag, expected) = answer("555000", ASKED_KEY, ASKING_KEY, &first).expect("answer").into_expected();
        let good = asking.finish(&message, &tag).expect("right code");
        let mut forged = good;
        forged[0] ^= 1;
        assert!(!expected.accepts(&forged));
        assert!(!expected.accepts(&[0u8; 32]));
        assert!(!expected.accepts(&good[..31]));
        assert!(expected.accepts(&good));
    }

    #[test]
    fn someone_in_the_middle_fails_even_with_the_right_code() {
        // The middle runs one TLS session to each end with keys of its own and passes the SPAKE2
        // messages along unchanged. The asking device sees key M1, the asked device sees key M2.
        let (asking, asked) = run("777777", "777777", (ASKING_KEY, b"middle key one"), (ASKED_KEY, b"middle key two"));
        assert_eq!(asking, Err(PairError::Mismatch));
        assert_eq!(asked, None);
        // Even one key in the middle shared by both sessions does not help.
        let (asking, asked) = run("777777", "777777", (ASKING_KEY, b"middle key"), (ASKED_KEY, b"middle key"));
        assert_eq!(asking, Err(PairError::Mismatch));
        assert_eq!(asked, None);
    }

    #[test]
    fn a_tampered_message_fails() {
        let (asking, mut first) = Asking::start("123123", ASKING_KEY, ASKED_KEY);
        first[5] ^= 0x40;
        match answer("123123", ASKED_KEY, ASKING_KEY, &first) {
            Err(_) => {}
            Ok(reply) => {
                let (message, tag, _) = reply.into_expected();
                assert!(asking.finish(&message, &tag).is_err());
            }
        }
        assert_eq!(answer("123123", ASKED_KEY, ASKING_KEY, b"short").err(), Some(PairError::Malformed));
    }

    #[test]
    fn a_code_expires_and_works_once() {
        let start = Instant::now();
        let code = Code::new(start);
        assert!(!code.expired(start + CODE_LIFETIME - Duration::from_millis(1)));
        assert!(code.expired(start + CODE_LIFETIME));
        assert_eq!(code.seconds_left(start), CODE_LIFETIME.as_secs());
        assert!((60..=120).contains(&CODE_LIFETIME.as_secs()));
        let value = code.value().to_string();
        assert_eq!(code.use_once(start + Duration::from_secs(10)), Some(value));
        let late = Code::new(start);
        assert_eq!(late.use_once(start + CODE_LIFETIME + Duration::from_secs(1)), None);
    }

    #[test]
    fn three_wrong_codes_lock_pairing_for_a_minute() {
        let start = Instant::now();
        let mut guard = Guard::default();
        assert!(!guard.locked(start));
        assert!(!guard.wrong(start));
        assert_eq!(guard.tries_left(), 2);
        assert!(!guard.wrong(start));
        assert!(guard.wrong(start));
        assert!(guard.locked(start + Duration::from_secs(59)));
        assert_eq!(guard.lock_left(start + Duration::from_millis(59_500)), 1);
        assert!(!guard.locked(start + LOCK_TIME));
        assert_eq!(guard.tries_left(), MAX_WRONG);
    }

    #[test]
    fn a_success_clears_wrong_tries() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.wrong(start);
        guard.wrong(start);
        guard.succeeded();
        assert!(!guard.wrong(start));
        assert!(!guard.locked(start));
    }
}
