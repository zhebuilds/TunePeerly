// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use std::sync::Arc;

use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, DigitallySignedStruct, DistinguishedName, Error, ServerConfig, SignatureScheme,
};

use sha2::{Digest, Sha256};

/// Pins the certificate a device or relay presents. Devices are pinned by the hash of their public
/// key, so only the key a paired device holds matters; relays are pinned by their whole-certificate id.
/// Handshake signatures still go through the rustls crypto provider, which proves the key is held.
#[derive(Debug)]
pub struct IdentityVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    pin: Pin,
}

#[derive(Debug)]
enum Pin {
    /// Any device key; the caller decides from the device id afterwards.
    AnyDevice,
    /// SHA-256 of the device's SubjectPublicKeyInfo.
    Device([u8; 32]),
    /// strelaysrv id: SHA-256 of the relay's certificate.
    Relay([u8; 32]),
}

impl IdentityVerifier {
    pub fn peer(provider: Arc<rustls::crypto::CryptoProvider>, expected_key_hash: Option<[u8; 32]>) -> Self {
        Self {
            provider,
            pin: expected_key_hash.map_or(Pin::AnyDevice, Pin::Device),
        }
    }

    pub fn relay(provider: Arc<rustls::crypto::CryptoProvider>, expected_cert_id: [u8; 32]) -> Self {
        Self {
            provider,
            pin: Pin::Relay(expected_cert_id),
        }
    }

    fn check_end_entity(&self, end_entity: &CertificateDer<'_>) -> Result<(), Error> {
        let (_, cert) = x509_parser::parse_x509_certificate(end_entity.as_ref())
            .map_err(|err| Error::General(err.to_string()))?;
        match &self.pin {
            Pin::AnyDevice => Ok(()),
            Pin::Device(expected) => {
                if Sha256::digest(cert.public_key().raw).as_slice() != expected {
                    return Err(Error::General("peer key does not match the pinned key".into()));
                }
                Ok(())
            }
            Pin::Relay(expected) => {
                if &crate::ids::cert_id_bytes(end_entity.as_ref()) != expected {
                    return Err(Error::General("relay certificate id does not match".into()));
                }
                cert.verify_signature(None)
                    .map_err(|err| Error::General(err.to_string()))
            }
        }
    }
}

/// Every connection negotiates TLS 1.3; these handshakes never reach a TLS 1.2 signature check.
fn refuse_tls12() -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
    Err(Error::General("TLS 1.2 is not allowed".into()))
}

impl ServerCertVerifier for IdentityVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, Error> {
        self.check_end_entity(end_entity)?;
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
        refuse_tls12()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for IdentityVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }
    fn client_auth_mandatory(&self) -> bool {
        !matches!(self.pin, Pin::Relay(_))
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        self.check_end_entity(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
        refuse_tls12()
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

pub fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub fn client_config(
    provider: Arc<rustls::crypto::CryptoProvider>,
    cert_der: &[u8],
    key_pkcs8: &[u8],
    verifier: Arc<IdentityVerifier>,
    alpn: Option<&[u8]>,
) -> Result<ClientConfig, Error> {
    let cert = CertificateDer::from(cert_der.to_vec());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pkcs8.to_vec()));
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|err| Error::General(err.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![cert], key)?;
    if let Some(protocol) = alpn {
        config.alpn_protocols = vec![protocol.to_vec()];
    } else {
        config.alpn_protocols = vec![b"tunepeerly".to_vec()];
    }
    Ok(config)
}

pub fn server_config(
    provider: Arc<rustls::crypto::CryptoProvider>,
    cert_der: &[u8],
    key_pkcs8: &[u8],
    verifier: Arc<IdentityVerifier>,
) -> Result<ServerConfig, Error> {
    let cert = CertificateDer::from(cert_der.to_vec());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pkcs8.to_vec()));
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|err| Error::General(err.to_string()))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![cert], key)?;
    config.alpn_protocols = vec![b"tunepeerly".to_vec()];
    Ok(config)
}
