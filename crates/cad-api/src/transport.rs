use std::{sync::Arc, time::Duration};

use reqwest::redirect::Policy;
use serde::{Serialize, de::DeserializeOwned};

use crate::{ApiKeyV2Credentials, Error, Result, ServerConfig, SessionContext, ids::CompanyId};
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
        credentials: &ApiKeyV2Credentials,
    ) -> Result<SessionContext> {
        let response = self
            .request_from_spec(machine_identity(), credentials, None)?
            .send()
            .await
            .map_err(Error::transport)?;
        let context: SessionContext = decode_json_response(response)
            .await
            .map_err(Error::classify_auth)?;
        tracing::debug!(
            resolved_company_id = ?context.company_selection.resolved_company_id,
            company_selection_source = ?context.company_selection.source,
            "resolved machine company selection"
        );
        Ok(context)
    }

    fn request_from_spec<B, T>(
        &self,
        request: RequestSpec<B, T>,
        credentials: &ApiKeyV2Credentials,
        company_id: Option<CompanyId>,
    ) -> Result<reqwest::RequestBuilder>
    where
        B: Serialize,
    {
        let url = self.config.endpoint(&request.path)?;
        let mut safe_url = url.clone();
        safe_url.set_query(None);
        safe_url.set_fragment(None);
        tracing::debug!(
            method = %request.method,
            url = %safe_url,
            company_id = ?company_id,
            "sending API request"
        );
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

#[derive(Clone)]
pub(crate) struct ClientCore {
    transport: Arc<Transport>,
    credentials: Arc<ApiKeyV2Credentials>,
    company_id: Option<CompanyId>,
    context: SessionContext,
}

impl ClientCore {
    pub(crate) fn new(
        transport: Arc<Transport>,
        credentials: ApiKeyV2Credentials,
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
