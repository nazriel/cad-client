//! A consumer that sets nothing up gets post-quantum key exchange.
//!
//! The rustls provider is process-wide state, so this file is its own test
//! process and holds a single test.

use cad_api::{Client, ServerConfig};
use rustls::{NamedGroup, crypto::CryptoProvider};

#[test]
fn building_an_sdk_client_installs_aws_lc_rs() {
    assert!(
        CryptoProvider::get_default().is_none(),
        "nothing installs a provider before the SDK"
    );

    Client::new(ServerConfig::production()).expect("an SDK client");

    let provider = CryptoProvider::get_default().expect("the SDK installed a provider");
    assert_eq!(
        provider.kx_groups.first().map(|group| group.name()),
        Some(NamedGroup::X25519MLKEM768)
    );
}
