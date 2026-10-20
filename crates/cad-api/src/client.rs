use std::{fmt, sync::Arc};

use crate::{
    ApiKeyV2Credentials, Company, Error, Result, ServerConfig, SessionContext, ids::CompanyId,
    protocol::companies, transport::Transport,
};

pub(crate) use crate::transport::ClientCore;

/// Diagnostics shared by every client state.
///
/// The trait is private, so the set of states is closed outside this crate and
/// `Client` cannot be instantiated with a caller-supplied state.
trait ClientState {
    const NAME: &'static str;

    fn company_id(&self) -> Option<CompanyId> {
        None
    }
}

/// The states that already hold an authenticated core.
///
/// Session operations are written once against this trait instead of once per
/// state. It is private and implemented only below, so `Disconnected` cannot
/// acquire those operations and no downstream type can join the set.
trait Authenticated: ClientState {
    fn core(&self) -> &ClientCore;
}

/// The initial client state. It can validate credentials but cannot perform
/// authenticated operations.
#[derive(Clone)]
pub struct Disconnected {
    transport: Arc<Transport>,
}

impl ClientState for Disconnected {
    const NAME: &'static str = "Disconnected";
}

/// An authenticated client state which has not yet made company scope explicit.
#[derive(Clone)]
pub struct Connected {
    core: ClientCore,
}

impl ClientState for Connected {
    const NAME: &'static str = "Connected";
}

impl Authenticated for Connected {
    fn core(&self) -> &ClientCore {
        &self.core
    }
}

/// An authenticated client state bound to one company.
#[derive(Clone)]
pub struct CompanyScoped {
    core: ClientCore,
    company_id: CompanyId,
}

impl ClientState for CompanyScoped {
    const NAME: &'static str = "CompanyScoped";

    fn company_id(&self) -> Option<CompanyId> {
        Some(self.company_id)
    }
}

impl Authenticated for CompanyScoped {
    fn core(&self) -> &ClientCore {
        &self.core
    }
}

/// An asynchronous, typestate-driven CAD API client.
///
/// The state markers cannot be constructed by callers. Resource capabilities
/// exist only for [`Client<CompanyScoped>`].
///
/// ```compile_fail
/// use cad_api::{Client, Disconnected};
///
/// async fn invalid(client: Client<Disconnected>) {
///     client.companies().await.unwrap();
/// }
/// ```
///
/// ```compile_fail
/// use cad_api::{Client, Disconnected};
///
/// fn invalid(client: &Client<Disconnected>) {
///     let _ = client.reports();
/// }
/// ```
///
/// ```compile_fail
/// use cad_api::{Client, Connected};
///
/// fn invalid(client: &Client<Connected>) {
///     let _ = client.reports();
/// }
/// ```
///
/// ```compile_fail
/// use cad_api::{Client, Connected};
///
/// fn forge(state: Connected) -> Client<Connected> {
///     Client { state }
/// }
/// ```
///
/// ```no_run
/// use cad_api::{Client, Connected, Error};
///
/// async fn valid(client: Client<Connected>) -> Result<(), Error> {
///     let client = client.scope_current_company()?;
///     let _reports = client.reports();
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Client<State> {
    state: State,
}

impl<State: ClientState> fmt::Debug for Client<State> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("state", &State::NAME)
            .field("company_id", &self.state.company_id())
            .finish_non_exhaustive()
    }
}

impl Client<Disconnected> {
    /// Creates an asynchronous client without authenticating it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the HTTP transports cannot be initialized.
    pub fn new(config: ServerConfig) -> Result<Self> {
        Ok(Self {
            state: Disconnected {
                transport: Arc::new(Transport::new(config)?),
            },
        })
    }

    /// Validates machine API-key credentials and consumes them with the disconnected client.
    ///
    /// # Errors
    ///
    /// Returns an authentication, transport, or protocol error when the identity
    /// request fails or the server returns an invalid response.
    pub async fn connect(self, credentials: ApiKeyV2Credentials) -> Result<Client<Connected>> {
        let transport = self.state.transport;
        let context = transport.identity(&credentials).await?;
        Ok(Client {
            state: Connected {
                core: ClientCore::new(transport, credentials, None, context),
            },
        })
    }

    #[must_use]
    pub fn server_config(&self) -> &ServerConfig {
        self.state.transport.config()
    }
}

/// Session operations available in every authenticated state.
///
/// One definition serves `Connected` and `CompanyScoped`; the state itself only
/// supplies its core. Method names and signatures are the same for both, and
/// `Client<Disconnected>` still has none of them.
// The private bound is the point: callers cannot name `Authenticated`, so they
// cannot add a state or write code generic over the set. `private_bounds` warns
// about exactly that reachability gap, which here is the sealing mechanism.
#[allow(
    private_bounds,
    reason = "the bound seals the authenticated state set; it is deliberately not nameable"
)]
impl<State: Authenticated> Client<State> {
    /// Returns companies visible to the current user.
    ///
    /// # Errors
    ///
    /// Returns an authentication, transport, protocol, or API error.
    pub async fn companies(&self) -> Result<Vec<Company>> {
        self.state.core().execute(companies()).await
    }

    /// Returns the machine identity context established during connection.
    pub fn context(&self) -> SessionContext {
        self.state.core().session_context()
    }

    /// Creates a new independent client scoped with `X-CAD-Company-Id`.
    ///
    /// # Errors
    ///
    /// Returns an authentication, transport, protocol, or API error if the company
    /// selection is rejected.
    pub async fn for_company(&self, company_id: CompanyId) -> Result<Client<CompanyScoped>> {
        switch_company(self.state.core(), company_id).await
    }

    /// Drops credentials and returns to the disconnected state.
    #[must_use]
    pub fn disconnect(self) -> Client<Disconnected> {
        Client {
            state: Disconnected {
                transport: Arc::clone(self.state.core().transport()),
            },
        }
    }

    #[must_use]
    pub fn server_config(&self) -> &ServerConfig {
        self.state.core().server_config()
    }
}

impl Client<Connected> {
    /// Consumes this client and binds it to the company resolved for its machine key.
    ///
    /// # Errors
    ///
    /// Returns an authentication error if the identity response has no resolved company.
    pub fn scope_current_company(self) -> Result<Client<CompanyScoped>> {
        let core = self.state.core;
        let company_id = core
            .session_context()
            .company_selection
            .resolved_company_id
            .ok_or_else(|| {
                Error::authentication(
                    None,
                    "the machine identity response has no resolved company context",
                )
            })?;

        Ok(Client {
            state: CompanyScoped { core, company_id },
        })
    }
}

impl Client<CompanyScoped> {
    #[must_use]
    pub const fn company_id(&self) -> CompanyId {
        self.state.company_id
    }

    pub(crate) const fn core(&self) -> &ClientCore {
        &self.state.core
    }
}

async fn switch_company(core: &ClientCore, company_id: CompanyId) -> Result<Client<CompanyScoped>> {
    let context = core.identity_for_company(company_id).await?;
    let resolved = context
        .company_selection
        .resolved_company_id
        .ok_or_else(|| {
            Error::authentication(
                None,
                "the machine identity response has no resolved company context",
            )
        })?;
    if resolved != company_id {
        return Err(Error::protocol(format!(
            "machine identity resolved company {resolved}, expected {company_id}"
        )));
    }
    let switched_core = core.with_company(company_id, context);

    Ok(Client {
        state: CompanyScoped {
            core: switched_core,
            company_id,
        },
    })
}
