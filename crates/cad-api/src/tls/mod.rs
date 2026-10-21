//! The rustls crypto provider behind every HTTPS client.
//!
//! `reqwest` is built without a provider of its own: it uses rustls' process
//! default. [`install_crypto_provider`] makes that default aws-lc-rs, whose key
//! exchange offers the hybrid post-quantum `X25519MLKEM768` group first and
//! falls back to classical X25519 or the NIST curves for servers that do not
//! support it. Trust anchors stay Mozilla's root store, which reqwest adds
//! itself from `webpki-roots`.

use rustls::crypto::{CryptoProvider, aws_lc_rs};

/// Installs aws-lc-rs as rustls' process-wide crypto provider, unless one is
/// already installed.
///
/// The SDK calls this before it builds an HTTP client, so a consumer needs no
/// setup; an application's other `reqwest` clients get the same provider once
/// it has run. Calling it again, from any thread, is harmless.
///
/// It never replaces a provider the host application installed first. The
/// trade-off: a host that chose another provider (ring, say, which has no
/// ML-KEM) keeps it, and its connections, the SDK's included, then negotiate
/// without post-quantum key exchange.
pub fn install_crypto_provider() {
    if CryptoProvider::get_default().is_none() {
        // Fails only when another thread installed a provider in the
        // meantime, which is then the one to keep.
        let _ = aws_lc_rs::default_provider().install_default();
    }
}

#[cfg(test)]
mod tests;
