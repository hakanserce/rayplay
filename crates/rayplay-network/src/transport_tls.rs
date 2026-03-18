use std::sync::Arc;

use quinn::{
    ClientConfig, ServerConfig,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

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
    // Enable datagram support by setting a non-None receive buffer.
    Arc::get_mut(&mut server_config.transport)
        .expect("no other Arc references at construction time")
        .datagram_receive_buffer_size(Some(MAX_DATAGRAM_BUFFER));

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

    let mut client_config = ClientConfig::new(Arc::new(quic_client_config));
    client_config.transport_config(Arc::new(transport_config));

    Ok(client_config)
}
