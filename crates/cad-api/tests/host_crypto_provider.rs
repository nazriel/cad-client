//! The SDK never replaces a rustls provider its host application installed.
//!
//! The rustls provider is process-wide state, so this file is its own test
//! process and holds a single test.

use std::sync::Arc;

use cad_api::{Client, ServerConfig};
use rustls::{
    NamedGroup,
    crypto::{CryptoProvider, aws_lc_rs},
};

#[test]
fn a_host_provider_is_kept() {
    // A host's own choice, made recognisable by offering classical X25519 only.
    let host = CryptoProvider {
        kx_groups: vec![aws_lc_rs::kx_group::X25519],
        ..aws_lc_rs::default_provider()
    };
    host.install_default()
        .expect("nothing installed a provider before the host");
    let installed = Arc::clone(CryptoProvider::get_default().expect("the host provider"));

    Client::new(ServerConfig::production()).expect("an SDK client");
    cad_api::tls::install_crypto_provider();

    let current = CryptoProvider::get_default().expect("a process default provider");
    assert!(Arc::ptr_eq(&installed, current), "the SDK replaced it");
    let groups: Vec<NamedGroup> = current.kx_groups.iter().map(|group| group.name()).collect();
    assert_eq!(groups, [NamedGroup::X25519]);
}
