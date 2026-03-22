use std::sync::Arc;

use quinn::{
    ClientConfig, ServerConfig,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};

use crate::{transport::MAX_DATAGRAM_BUFFER, wire::TransportError};

/// Generates a self-signed TLS certificate and a matching [`ServerConfig`]
/// with datagram support enabled.
pub(crate) fn make_server_config() -> Result<(CertificateDer<'static>, ServerConfig), TransportError>
{
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(["localhost".to_owned()])
            .map_err(|e| TransportError::TlsError(e.to_string()))?;

    let cert_der: CertificateDer<'static> = cert.der().clone();
    let priv_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));

    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], priv_key)
        .map_err(|e| TransportError::TlsError(e.to_string()))?;

    let quic_server_config = QuicServerConfig::try_from(server_crypto)
        .map_err(|e| TransportError::TlsError(e.to_string()))?;

    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));
    // Enable datagram support and allow one bidirectional stream for the
    // session control channel (ADR-010).
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.datagram_receive_buffer_size(Some(MAX_DATAGRAM_BUFFER));
    transport_config.max_concurrent_bidi_streams(1u32.into());
    server_config.transport = Arc::new(transport_config);

    Ok((cert_der, server_config))
}

/// Builds a [`ClientConfig`] that trusts exactly one server certificate, with
/// datagram support enabled.
pub(crate) fn make_client_config(
    server_cert: CertificateDer<'static>,
) -> Result<ClientConfig, TransportError> {
    let mut roots = RootCertStore::empty();
    roots
        .add(server_cert)
        .map_err(|e| TransportError::TlsError(e.to_string()))?;

    let client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let quic_client_config = QuicClientConfig::try_from(client_crypto)
        .map_err(|e| TransportError::TlsError(e.to_string()))?;

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.datagram_receive_buffer_size(Some(MAX_DATAGRAM_BUFFER));
    transport_config.max_concurrent_bidi_streams(1u32.into());

    let mut client_config = ClientConfig::new(Arc::new(quic_client_config));
    client_config.transport_config(Arc::new(transport_config));

    Ok(client_config)
}

/// TLS certificate verifier that accepts any server certificate.
///
/// Used during SPAKE2 pairing where the PIN-based key agreement provides
/// authentication independently of TLS certificate validation.
#[derive(Debug)]
struct InsecureServerCertVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Builds a [`ClientConfig`] that accepts any server certificate.
///
/// This is used during the SPAKE2 pairing flow where authentication is
/// provided by the PIN-based key agreement rather than TLS certificate
/// validation.
pub fn make_client_config_insecure() -> Result<ClientConfig, TransportError> {
    let client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(InsecureServerCertVerifier))
        .with_no_client_auth();

    let quic_client_config = QuicClientConfig::try_from(client_crypto)
        .map_err(|e| TransportError::TlsError(e.to_string()))?;

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.datagram_receive_buffer_size(Some(MAX_DATAGRAM_BUFFER));
    transport_config.max_concurrent_bidi_streams(1u32.into());

    let mut client_config = ClientConfig::new(Arc::new(quic_client_config));
    client_config.transport_config(Arc::new(transport_config));

    Ok(client_config)
}
