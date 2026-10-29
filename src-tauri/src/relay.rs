// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAGIC: u32 = 0x9E79BC40;

#[derive(Debug, Clone)]
pub struct Invitation {
    pub from: Vec<u8>,
    pub key: Vec<u8>,
    pub address: Vec<u8>,
    pub port: u16,
    pub server_socket: bool,
}

pub async fn read_message(stream: &mut (impl AsyncReadExt + Unpin)) -> io::Result<(i32, Vec<u8>)> {
    let mut header = [0u8; 12];
    stream.read_exact(&mut header).await?;
    let magic = u32::from_be_bytes(header[0..4].try_into().unwrap());
    if magic != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "relay magic"));
    }
    let kind = i32::from_be_bytes(header[4..8].try_into().unwrap());
    let len = i32::from_be_bytes(header[8..12].try_into().unwrap());
    if len < 0 || len > 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "relay length"));
    }
    let mut body = vec![0u8; len as usize];
    if len > 0 {
        stream.read_exact(&mut body).await?;
    }
    Ok((kind, body))
}

pub async fn write_message(
    stream: &mut (impl AsyncWriteExt + Unpin),
    kind: i32,
    body: &[u8],
) -> io::Result<()> {
    let mut header = [0u8; 12];
    header[0..4].copy_from_slice(&MAGIC.to_be_bytes());
    header[4..8].copy_from_slice(&kind.to_be_bytes());
    header[8..12].copy_from_slice(&(body.len() as i32).to_be_bytes());
    stream.write_all(&header).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

pub fn join_relay(token: &str) -> Vec<u8> {
    xdr_string(token)
}

pub fn connect_request(cert_id: &[u8; 32]) -> Vec<u8> {
    xdr_bytes(cert_id)
}

pub fn join_session(key: &[u8]) -> Vec<u8> {
    xdr_bytes(key)
}

pub fn parse_response(body: &[u8]) -> io::Result<(i32, String)> {
    if body.len() < 8 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "response"));
    }
    let code = i32::from_be_bytes(body[0..4].try_into().unwrap());
    let (message, _) = read_string(&body[4..])?;
    Ok((code, message))
}

pub fn parse_invitation(body: &[u8]) -> io::Result<Invitation> {
    let (from, rest) = read_bytes(body)?;
    let (key, rest) = read_bytes(rest)?;
    let (address, rest) = read_bytes(rest)?;
    if rest.len() < 8 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "invitation"));
    }
    let port = u16::from_be_bytes(rest[2..4].try_into().unwrap());
    let server_socket = u32::from_be_bytes(rest[4..8].try_into().unwrap()) != 0;
    Ok(Invitation {
        from,
        key,
        address,
        port,
        server_socket,
    })
}

pub fn invitation_target(invitation: &Invitation, fallback_host: Ipv4Addr, fallback_port: u16) -> io::Result<SocketAddr> {
    let port = if invitation.port == 0 { fallback_port } else { invitation.port };
    let ip = if invitation.address.len() == 4 {
        Ipv4Addr::new(
            invitation.address[0],
            invitation.address[1],
            invitation.address[2],
            invitation.address[3],
        )
    } else if invitation.address.is_empty() {
        fallback_host
    } else if invitation.address.len() == 16 {
        // An IPv4 address may arrive in IPv6 form (::ffff:a.b.c.d); other IPv6 addresses use the relay's own.
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&invitation.address);
        std::net::Ipv6Addr::from(octets).to_ipv4_mapped().unwrap_or(fallback_host)
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "relay address"));
    };
    if ip.is_unspecified() {
        return Ok(SocketAddr::new(IpAddr::V4(fallback_host), port));
    }
    Ok(SocketAddr::new(IpAddr::V4(ip), port))
}

fn xdr_string(text: &str) -> Vec<u8> {
    xdr_bytes(text.as_bytes())
}

fn xdr_bytes(bytes: &[u8]) -> Vec<u8> {
    let pad = (4 - (bytes.len() % 4)) % 4;
    let mut out = Vec::with_capacity(4 + bytes.len() + pad);
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    out.extend(std::iter::repeat(0).take(pad));
    out
}

fn read_bytes(input: &[u8]) -> io::Result<(Vec<u8>, &[u8])> {
    if input.len() < 4 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "xdr"));
    }
    let len = u32::from_be_bytes(input[0..4].try_into().unwrap()) as usize;
    let pad = (4 - (len % 4)) % 4;
    let end = 4 + len + pad;
    if input.len() < end {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "xdr"));
    }
    Ok((input[4..4 + len].to_vec(), &input[end..]))
}

fn read_string(input: &[u8]) -> io::Result<(String, &[u8])> {
    let (bytes, rest) = read_bytes(input)?;
    Ok((String::from_utf8_lossy(&bytes).to_string(), rest))
}

#[cfg(test)]
mod tests {
    use super::{
        invitation_target, join_relay, join_session, parse_invitation, parse_response, read_message,
        write_message, Invitation,
    };
    use std::net::{Ipv4Addr, SocketAddr};

    fn invitation(address: Vec<u8>, port: u16) -> Invitation {
        Invitation { from: vec![1], key: vec![2], address, port, server_socket: false }
    }

    fn xdr(bytes: &[u8]) -> Vec<u8> {
        let pad = (4 - (bytes.len() % 4)) % 4;
        let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(bytes);
        out.extend(std::iter::repeat(0).take(pad));
        out
    }

    #[test]
    fn invitation_addresses_in_ipv6_form_are_read() {
        let relay = Ipv4Addr::new(203, 0, 113, 9);
        let mut mapped = vec![0u8; 10];
        mapped.extend_from_slice(&[0xff, 0xff, 198, 51, 100, 7]);
        assert_eq!(invitation_target(&invitation(mapped, 443), relay, 22067).unwrap(), "198.51.100.7:443".parse::<SocketAddr>().unwrap());
        let mut v6 = vec![0x20, 0x01, 0x0d, 0xb8];
        v6.extend_from_slice(&[0u8; 12]);
        assert_eq!(invitation_target(&invitation(v6, 0), relay, 22067).unwrap(), "203.0.113.9:22067".parse::<SocketAddr>().unwrap());
        assert_eq!(invitation_target(&invitation(vec![10, 0, 0, 2], 5000), relay, 22067).unwrap(), "10.0.0.2:5000".parse::<SocketAddr>().unwrap());
        assert!(invitation_target(&invitation(vec![1, 2, 3], 5000), relay, 22067).is_err());
    }

    #[test]
    fn unspecified_invitation_addresses_fall_back_to_the_relay() {
        let relay = Ipv4Addr::new(203, 0, 113, 9);
        let unspecified = vec![0, 0, 0, 0];
        assert_eq!(invitation_target(&invitation(unspecified.clone(), 443), relay, 22067).unwrap(), "203.0.113.9:443".parse::<SocketAddr>().unwrap());
        assert_eq!(
            invitation_target(&invitation(unspecified, 0), relay, 22067).unwrap(),
            "203.0.113.9:22067".parse::<SocketAddr>().unwrap(),
            "an empty port falls back to the configured relay port"
        );
    }

    #[tokio::test]
    async fn frames_round_trip_without_the_padding_leaking_into_the_body() {
        let token = "abcdefg"; // 7 bytes: one byte of XDR padding
        let body = join_relay(token);
        assert_eq!(body.len(), 12, "length prefix, token, padding");
        assert_eq!(body, xdr(token.as_bytes()));
        let mut wire = Vec::new();
        write_message(&mut wire, 2, &body).await.unwrap();
        assert_eq!(wire.len(), 12 + body.len());
        assert_eq!(&wire[0..4], &super::MAGIC.to_be_bytes());
        let mut reader: &[u8] = &wire;
        let (kind, read) = read_message(&mut reader).await.unwrap();
        assert_eq!(kind, 2);
        assert_eq!(read, body);
        assert!(reader.is_empty(), "the whole frame is consumed");

        // A session key is a 32-byte blob that needs no padding.
        assert_eq!(join_session(&[7u8; 32]).len(), 36);
    }

    #[tokio::test]
    async fn a_truncated_frame_is_an_error_rather_than_a_short_body() {
        let mut wire = Vec::new();
        write_message(&mut wire, 2, &join_relay("token")).await.unwrap();
        let cut = wire.len() - 2;
        let mut reader: &[u8] = &wire[..cut];
        assert!(read_message(&mut reader).await.is_err());

        let mut bad_magic = wire.clone();
        bad_magic[0] ^= 0xff;
        let mut reader: &[u8] = &bad_magic;
        assert!(read_message(&mut reader).await.is_err(), "wrong magic is rejected");

        let mut too_long = wire.clone();
        too_long[8..12].copy_from_slice(&(2 * 1024 * 1024i32).to_be_bytes());
        let mut reader: &[u8] = &too_long;
        assert!(read_message(&mut reader).await.is_err(), "an oversized body is rejected");
    }

    #[test]
    fn responses_carry_a_code_and_a_message() {
        let mut body = 1i32.to_be_bytes().to_vec();
        body.extend_from_slice(&xdr(b"device not found"));
        assert_eq!(parse_response(&body).unwrap(), (1, "device not found".to_string()));

        let mut ok = 0i32.to_be_bytes().to_vec();
        ok.extend_from_slice(&xdr(b"OK"));
        assert_eq!(parse_response(&ok).unwrap(), (0, "OK".to_string()));

        assert!(parse_response(&body[..3]).is_err(), "a truncated response is rejected");
        assert!(parse_response(&[0u8; 8]).is_err(), "a response without a message is rejected");
    }

    #[test]
    fn invitations_carry_the_session_port_and_which_side_serves() {
        let from = [3u8; 32];
        let key = [4u8; 32];
        let mut body = xdr(&from);
        body.extend_from_slice(&xdr(&key));
        body.extend_from_slice(&xdr(&[0, 0, 0, 0]));
        // Two unused bytes, the port, then the serverSocket flag (see strelaysrv).
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(&22068u16.to_be_bytes());
        body.extend_from_slice(&1u32.to_be_bytes());

        let parsed = parse_invitation(&body).unwrap();
        assert_eq!(parsed.from, from);
        assert_eq!(parsed.key, key);
        assert!(parsed.address.is_empty());
        assert_eq!(parsed.port, 22068);
        assert!(parsed.server_socket);
        assert_eq!(
            invitation_target(&parsed, Ipv4Addr::new(203, 0, 113, 9), 22067).unwrap(),
            "203.0.113.9:22068".parse::<SocketAddr>().unwrap(),
            "the relay's session port wins over the join port"
        );

        assert!(parse_invitation(&body[..body.len() - 1]).is_err(), "a short invitation is rejected");
    }
}
