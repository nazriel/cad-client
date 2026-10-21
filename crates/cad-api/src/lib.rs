#![doc = "Typed client for the Carbon+Alt+Delete HTTP API."]
#![forbid(unsafe_code)]

// TODO(blocking): a blocking client will come back behind a `blocking` feature
// gate. The first implementation mirrored the whole async surface by hand —
// client, core, session, transport and one wrapper per domain operation — which
// doubled every change and let the two surfaces drift. It was removed rather
// than kept in that shape; it returns once there is a way to derive it from a
// single definition (generated facades, or an executor abstraction over
// `RequestSpec`), so async and blocking cannot diverge. See
// `docs/architecture.md`.

mod auth;
mod client;
mod config;
pub mod domains;
mod error;
pub mod ids;
mod protocol;
pub mod tls;
mod transport;

pub use auth::{
    ApiKeyV2Credentials, CompanySelectionSource, MachineCompanySelection, MachinePrincipal,
    SessionContext,
};
pub use client::{Client, CompanyScoped, Connected, Disconnected};
pub use config::{Environment, ServerConfig};
pub use domains::companies::Company;
pub use error::{Error, Result};
