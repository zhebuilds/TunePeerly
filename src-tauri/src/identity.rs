// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ids::{app_device_id, cert_id_display};

/// This device's TLS key pair. The public key (its SubjectPublicKeyInfo) is the device's identity:
/// the device id is its hash, and paired devices pin it.
#[derive(Clone)]
pub struct IdentityMaterial {
    pub device_id: String,
    /// DER SubjectPublicKeyInfo of the TLS key.
    pub public_key: Vec<u8>,
    pub cert_der: Vec<u8>,
    pub cert_key_pkcs8: Vec<u8>,
    /// Relay address: the strelaysrv id of the whole certificate.
    pub cert_id: String,
}

pub const TLS_DNS_NAME: &str = "tunepeerly.local";

pub fn load_or_create(db: &Db) -> AppResult<IdentityMaterial> {
    let saved = (db.identity_blob("cert_der")?, db.identity_blob("cert_key")?);
    let (cert_der, cert_key_pkcs8) = match saved {
        (Some(cert_der), Some(cert_key)) => (cert_der, cert_key),
        _ => {
            let (cert_der, cert_key) = issue_certificate()?;
            db.set_identity_blob("cert_der", &cert_der)?;
            db.set_identity_blob("cert_key", &cert_key)?;
            (cert_der, cert_key)
        }
    };
    let public_key = public_key_of(&cert_der)?;
    Ok(IdentityMaterial {
        device_id: app_device_id(&public_key),
        public_key,
        cert_id: cert_id_display(&cert_der),
        cert_der,
        cert_key_pkcs8,
    })
}

/// A self-signed certificate for a new ECDSA P-256 key. It never expires; the key, not the
/// certificate, is what paired devices remember.
fn issue_certificate() -> AppResult<(Vec<u8>, Vec<u8>)> {
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec![TLS_DNS_NAME.into()])?;
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "TunePeerly");
    params.subject_alt_names = vec![SanType::DnsName(
        TLS_DNS_NAME.try_into().expect("static dns name"),
    )];
    let cert = params.self_signed(&key)?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

/// The DER SubjectPublicKeyInfo inside a certificate.
pub fn public_key_of(cert_der: &[u8]) -> AppResult<Vec<u8>> {
    let (_, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|err| AppError::Message(err.to_string()))?;
    Ok(cert.public_key().raw.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db(name: &str) -> Db {
        let dir = std::env::temp_dir().join(format!("tunepeerly-identity-{name}-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("dir");
        Db::open(&dir.join("library.db"), dir.clone(), dir.join("cache")).expect("db")
    }

    #[test]
    fn identity_is_created_once_and_named_by_its_public_key() {
        let db = test_db("once");
        let first = load_or_create(&db).expect("identity");
        let again = load_or_create(&db).expect("identity");
        assert_eq!(first.device_id, again.device_id);
        assert_eq!(first.cert_der, again.cert_der);
        assert_eq!(first.device_id.len(), 64);
        assert_eq!(first.device_id, app_device_id(&public_key_of(&first.cert_der).unwrap()));
        assert_eq!(first.public_key, public_key_of(&first.cert_der).unwrap());
    }
}
