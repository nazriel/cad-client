use std::{sync::Arc, time::Duration};

use reqwest::redirect::Policy;
use serde::{Serialize, de::DeserializeOwned};

use crate::{ApiKeyV1Credentials, Error, Result, ServerConfig, SessionContext, ids::CompanyId};
use crate::{
    config::{STORAGE_STALL_TIMEOUT, storage_redirect_policy},
    protocol::{RequestSpec, decode_json_bytes, machine_identity},
};

#[derive(Clone)]
pub(crate) struct Transport {
    config: ServerConfig,
    api: reqwest::Client,
    storage: reqwest::Client,
}

impl Transport {
    pub(crate) fn new(config: ServerConfig) -> Result<Self> {
        let api = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(config.request_timeout())
            .redirect(Policy::none())
            .user_agent(concat!("cad-api/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(Error::transport)?;
        let storage = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(STORAGE_STALL_TIMEOUT)
            .redirect(storage_redirect_policy())
            .user_agent(concat!("cad-api-storage/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(Error::transport)?;

        Ok(Self {
            config,
            api,
            storage,
        })
    }

    pub(crate) fn config(&self) -> &ServerConfig {
        &self.config
    }

    pub(crate) async fn identity(
        &self,
        credentials: &ApiKeyV1Credentials,
    ) -> Result<SessionContext> {
        let response = self
            .request_from_spec(machine_identity(), credentials, None)?
            .send()
            .await
            .map_err(Error::transport)?;
        let context = decode_json_response(response)
            .await
            .map_err(Error::classify_auth)?;
        log_resolved_company(&context);
        Ok(context)
    }

    fn request_from_spec<B, T>(
        &self,
        request: RequestSpec<B, T>,
        credentials: &ApiKeyV1Credentials,
        company_id: Option<CompanyId>,
    ) -> Result<reqwest::RequestBuilder>
    where
        B: Serialize,
    {
        let url = self.config.endpoint(&request.path)?;
        log_request(&request.method, &url, company_id);
        let mut builder = self
            .api
            .request(request.method, url)
            .bearer_auth(credentials.bearer_token());
        if let Some(company_id) = company_id {
            builder = builder.header("X-CAD-Company-Id", company_id.to_string());
        }
        if let Some(query) = request.query.as_deref() {
            builder = builder.query(query);
        }
        if let Some(body) = request.body.as_ref() {
            builder = builder.json(body);
        }
        Ok(builder)
    }
}

fn log_request(method: &reqwest::Method, url: &url::Url, company_id: Option<CompanyId>) {
    if std::env::var_os("CAD_DEBUG_HTTP").is_some() {
        let mut safe_url = url.clone();
        safe_url.set_query(None);
        safe_url.set_fragment(None);
        if let Some(company_id) = company_id {
            eprintln!("cad-cli HTTP {method} {safe_url} X-CAD-Company-Id: {company_id}");
        } else {
            eprintln!("cad-cli HTTP {method} {safe_url}");
        }
    }
}

fn log_resolved_company(context: &SessionContext) {
    if std::env::var_os("CAD_DEBUG_HTTP").is_some() {
        let selection = &context.company_selection;
        eprintln!(
            "cad-cli resolved company: {} (source: {:?})",
            selection
                .resolved_company_id
                .map(|company_id| company_id.to_string())
                .as_deref()
                .unwrap_or("none"),
            selection.source
        );
    }
}

#[derive(Clone)]
pub(crate) struct ClientCore {
    transport: Arc<Transport>,
    credentials: Arc<ApiKeyV1Credentials>,
    company_id: Option<CompanyId>,
    context: SessionContext,
}

impl ClientCore {
    pub(crate) fn new(
        transport: Arc<Transport>,
        credentials: ApiKeyV1Credentials,
        company_id: Option<CompanyId>,
        context: SessionContext,
    ) -> Self {
        Self {
            transport,
            credentials: Arc::new(credentials),
            company_id,
            context,
        }
    }

    pub(crate) fn transport(&self) -> &Arc<Transport> {
        &self.transport
    }

    pub(crate) fn session_context(&self) -> SessionContext {
        self.context.clone()
    }

    pub(crate) fn with_company(&self, company_id: CompanyId, context: SessionContext) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            credentials: Arc::clone(&self.credentials),
            company_id: Some(company_id),
            context,
        }
    }

    pub(crate) fn server_config(&self) -> &ServerConfig {
        self.transport.config()
    }

    pub(crate) async fn execute<B, T>(&self, request: RequestSpec<B, T>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        let response = self
            .transport
            .request_from_spec(request, &self.credentials, self.company_id)?
            .send()
            .await
            .map_err(Error::transport)?;
        decode_json_response(response).await
    }

    pub(crate) async fn identity_for_company(
        &self,
        company_id: CompanyId,
    ) -> Result<SessionContext> {
        let response = self
            .transport
            .request_from_spec(machine_identity(), &self.credentials, Some(company_id))?
            .send()
            .await
            .map_err(Error::transport)?;
        decode_json_response(response).await
    }

    pub(crate) fn storage_client(&self) -> &reqwest::Client {
        &self.transport.storage
    }
}

pub(crate) async fn decode_json_response<T>(response: reqwest::Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let bytes = response.bytes().await.map_err(Error::transport)?;
    decode_json_bytes(status.as_u16(), &bytes)
}
