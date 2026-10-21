//! Every test here runs against the process-wide provider, as reqwest does.
//! In this test binary nothing installs a provider but
//! [`install_crypto_provider`], so it is always aws-lc-rs; keeping a host's
//! own provider is proven in `tests/host_crypto_provider.rs`, a process of its own.

use std::{net::TcpStream, sync::Arc, time::Duration};

use rustls::{
    ClientConfig, ClientConnection, NamedGroup, RootCertStore, crypto::CryptoProvider,
    pki_types::ServerName,
};

use super::install_crypto_provider;

const PRODUCTION_HOST: &str = "app.carbonaltdelete.eu";

/// An X25519MLKEM768 key share: an ML-KEM-768 encapsulation key (1184 bytes)
/// followed by an X25519 public key (32 bytes).
const X25519MLKEM768_SHARE_LEN: usize = 1184 + 32;

/// The process default provider after installation, as reqwest reads it.
fn default_provider() -> Arc<CryptoProvider> {
    install_crypto_provider();
    Arc::clone(CryptoProvider::get_default().expect("a process default provider"))
}

/// A client configuration on the process default provider, built the way
/// reqwest builds its own: webpki roots, TLS 1.2 and 1.3.
fn client_config() -> Arc<ClientConfig> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = ClientConfig::builder_with_provider(default_provider())
        .with_safe_default_protocol_versions()
        .expect("the provider supports TLS 1.2 and 1.3")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

#[test]
fn the_installed_provider_prefers_post_quantum_key_exchange() {
    let provider = default_provider();
    let groups: Vec<NamedGroup> = provider
        .kx_groups
        .iter()
        .map(|group| group.name())
        .collect();

    assert_eq!(
        groups.first(),
        Some(&NamedGroup::X25519MLKEM768),
        "{groups:?}"
    );
    // Servers without ML-KEM still get a classical group they support.
    assert!(groups.contains(&NamedGroup::X25519), "{groups:?}");
    assert!(groups.contains(&NamedGroup::secp256r1), "{groups:?}");
}

#[test]
fn installing_again_keeps_the_installed_provider() {
    let first = default_provider();
    install_crypto_provider();
    install_crypto_provider();

    let current = CryptoProvider::get_default().expect("a process default provider");
    assert!(Arc::ptr_eq(&first, current), "the default was replaced");
}

#[test]
fn the_first_client_hello_carries_an_x25519mlkem768_key_share() {
    let server = ServerName::try_from(PRODUCTION_HOST).expect("a valid DNS name");
    let mut connection =
        ClientConnection::new(client_config(), server).expect("a client connection");
    let mut hello = Vec::new();
    connection
        .write_tls(&mut hello)
        .expect("the ClientHello is written");

    let extensions = client_hello_extensions(&hello);

    let mut groups = Reader(extension(&extensions, SUPPORTED_GROUPS)).vec16();
    assert_eq!(
        NamedGroup::from(groups.u16()),
        NamedGroup::X25519MLKEM768,
        "X25519MLKEM768 must be the most preferred supported group"
    );

    // Only the first key share counts: a server that accepts it completes the
    // handshake in one round trip, without a HelloRetryRequest.
    let mut shares = Reader(extension(&extensions, KEY_SHARE)).vec16();
    assert_eq!(NamedGroup::from(shares.u16()), NamedGroup::X25519MLKEM768);
    assert_eq!(shares.vec16().0.len(), X25519MLKEM768_SHARE_LEN);
}

/// Proves the public platform host accepts the post-quantum key share.
///
/// Run with `cargo test -p cad-api --lib tls -- --ignored`. It needs network
/// access, so default runs and CI skip it.
#[test]
#[ignore = "needs network access to app.carbonaltdelete.eu"]
fn production_negotiates_x25519mlkem768() {
    let mut socket = TcpStream::connect((PRODUCTION_HOST, 443)).expect("a TCP connection");
    socket
        .set_read_timeout(Some(Duration::from_secs(15)))
        .expect("a read timeout");
    let server = ServerName::try_from(PRODUCTION_HOST).expect("a valid DNS name");
    let mut connection =
        ClientConnection::new(client_config(), server).expect("a client connection");

    while connection.is_handshaking() {
        connection
            .complete_io(&mut socket)
            .expect("the TLS handshake completes");
    }

    let group = connection
        .negotiated_key_exchange_group()
        .expect("a negotiated key exchange group");
    assert_eq!(group.name(), NamedGroup::X25519MLKEM768);
}

const SUPPORTED_GROUPS: u16 = 0x000a;
const KEY_SHARE: u16 = 0x0033;

/// The `(type, body)` extensions of the `ClientHello` in a TLS record.
fn client_hello_extensions(record: &[u8]) -> Vec<(u16, &[u8])> {
    let mut record = Reader(record);
    assert_eq!(record.u8(), 0x16, "a handshake record");
    record.take(2); // legacy record version
    let mut handshake = record.vec16();

    assert_eq!(handshake.u8(), 0x01, "a ClientHello");
    let length = handshake.u24();
    let mut hello = Reader(handshake.take(length));
    hello.take(2 + 32); // legacy version, random
    hello.vec8(); // legacy session ID
    hello.vec16(); // cipher suites
    hello.vec8(); // legacy compression methods

    let mut extensions = hello.vec16();
    let mut found = Vec::new();
    while !extensions.0.is_empty() {
        let kind = extensions.u16();
        found.push((kind, extensions.vec16().0));
    }
    found
}

fn extension<'a>(extensions: &[(u16, &'a [u8])], kind: u16) -> &'a [u8] {
    extensions
        .iter()
        .find_map(|&(found, body)| (found == kind).then_some(body))
        .unwrap_or_else(|| panic!("the ClientHello has no extension {kind:#06x}"))
}

/// Big-endian reader over TLS wire encoding; panics on truncated input.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> &'a [u8] {
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        head
    }

    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn u16(&mut self) -> u16 {
        let bytes = self.take(2);
        u16::from_be_bytes([bytes[0], bytes[1]])
    }

    fn u24(&mut self) -> usize {
        let bytes = self.take(3);
        usize::from(bytes[0]) << 16 | usize::from(bytes[1]) << 8 | usize::from(bytes[2])
    }

    fn vec8(&mut self) -> Reader<'a> {
        let len = usize::from(self.u8());
        Reader(self.take(len))
    }

    fn vec16(&mut self) -> Reader<'a> {
        let len = usize::from(self.u16());
        Reader(self.take(len))
    }
}
