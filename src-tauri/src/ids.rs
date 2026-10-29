// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
//! App device ids come from the Signal identity public key.
//! Relay certificate ids use the strelaysrv / Syncthing device-id algorithm
//! on the certificate's raw bytes.

use data_encoding::BASE32;
use sha2::{Digest, Sha256};

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn app_device_id(identity_public: &[u8]) -> String {
    hex_encode(identity_public)
}

pub fn identity_bytes_from_app_id(id: &str) -> Option<Vec<u8>> {
    hex_decode(id)
}

pub fn smaller_device(a: &str, b: &str) -> std::cmp::Ordering {
    let aa = identity_bytes_from_app_id(a).unwrap_or_default();
    let bb = identity_bytes_from_app_id(b).unwrap_or_default();
    aa.cmp(&bb)
}

pub fn cert_id_bytes(cert_der: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(cert_der);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub fn cert_id_display(cert_der: &[u8]) -> String {
    format_device_id(&cert_id_bytes(cert_der))
}

/// The Syncthing-style text form of a relay certificate id, for the ids the relay reports back.
pub fn cert_id_text(id: &[u8; 32]) -> String {
    format_device_id(id)
}

pub fn parse_cert_id(text: &str) -> Option<[u8; 32]> {
    let mut id = text.trim().to_string();
    if id.is_empty() {
        return None;
    }
    id = id.to_uppercase();
    id = id.replace('-', "").replace(' ', "");
    id = id.replace('0', "O").replace('1', "I").replace('8', "B");
    id = id.trim_end_matches('=').to_string();
    let bare = match id.len() {
        56 => unluhnify(&id)?,
        52 => id,
        _ => return None,
    };
    let padded = format!("{bare}====");
    let bytes = BASE32.decode(padded.as_bytes()).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

pub fn same_cert_id(configured: &str, cert_der: &[u8]) -> bool {
    match parse_cert_id(configured) {
        Some(parsed) => parsed == cert_id_bytes(cert_der),
        None => false,
    }
}

fn format_device_id(bytes: &[u8; 32]) -> String {
    let encoded = BASE32.encode(bytes);
    let bare = encoded.trim_end_matches('=');
    let with_check = luhnify(bare);
    chunkify(&with_check)
}

fn luhnify(input: &str) -> String {
    debug_assert_eq!(input.len(), 52);
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(56);
    for group in 0..4 {
        let part = &bytes[group * 13..(group + 1) * 13];
        out.extend_from_slice(part);
        out.push(luhn32(part));
    }
    String::from_utf8(out).expect("alphabet is ascii")
}

fn unluhnify(input: &str) -> Option<String> {
    if input.len() != 56 {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(52);
    for group in 0..4 {
        let start = group * 14;
        let part = &bytes[start..start + 13];
        let check = bytes[start + 13];
        if luhn32(part) != check {
            return None;
        }
        out.extend_from_slice(part);
    }
    String::from_utf8(out).ok()
}

fn luhn32(part: &[u8]) -> u8 {
    let mut factor = 1;
    let mut sum = 0;
    const N: i32 = 32;
    for byte in part {
        let codepoint = codepoint32(*byte);
        let mut addend = factor * codepoint;
        factor = if factor == 2 { 1 } else { 2 };
        addend = (addend / N) + (addend % N);
        sum += addend;
    }
    let remainder = sum % N;
    let check = (N - remainder) % N;
    ALPHABET[check as usize]
}

fn codepoint32(byte: u8) -> i32 {
    match byte {
        b'A'..=b'Z' => (byte - b'A') as i32,
        b'2'..=b'7' => (byte + 26 - b'2') as i32,
        _ => 0,
    }
}

fn chunkify(input: &str) -> String {
    let bytes = input.as_bytes();
    let chunks = bytes.len() / 7;
    let mut out = Vec::with_capacity(chunks * 8);
    for i in 0..chunks {
        if i > 0 {
            out.push(b'-');
        }
        out.extend_from_slice(&bytes[i * 7..(i + 1) * 7]);
    }
    String::from_utf8(out).expect("alphabet is ascii")
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cert_id_roundtrip_ignores_case_and_dashes() {
        let display = cert_id_display(b"tunepeerly-cert");
        let parsed = parse_cert_id(&display).expect("parse");
        assert_eq!(parsed, cert_id_bytes(b"tunepeerly-cert"));
        let messy = display.to_lowercase().replace('-', " ");
        assert_eq!(parse_cert_id(&messy), Some(parsed));
        assert!(same_cert_id(&display, b"tunepeerly-cert"));
        assert!(!same_cert_id(&display, b"other"));
    }

    #[test]
    fn device_id_orders_by_public_key_bytes() {
        let low = app_device_id(&[0x01, 0x02]);
        let high = app_device_id(&[0x01, 0x03]);
        assert_eq!(smaller_device(&low, &high), std::cmp::Ordering::Less);
        assert_eq!(smaller_device(&high, &low), std::cmp::Ordering::Greater);
    }

    #[test]
    fn luhn_check_digit_rejects_a_changed_character() {
        let display = cert_id_display(&[7u8; 32]);
        assert_eq!(display.chars().filter(|c| *c == '-').count(), 7);
        assert!(parse_cert_id(&display).is_some());
        let mut chars: Vec<char> = display.chars().collect();
        let index = chars.iter().rposition(|c| *c != '-').expect("check digit");
        chars[index] = if chars[index] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(parse_cert_id(&tampered).is_none());
        assert!(parse_cert_id("not-a-device").is_none());
    }
}
