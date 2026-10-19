//! Fixtures shared by the integration tests.
//!
//! Integration tests see only the crate's public API, so the `cfg(test)` helper
//! in `src/auth.rs` is out of reach and is reproduced here instead.

/// Header value of `name` on `request`, lowercased for case-insensitive checks.
pub fn header(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase)
}
