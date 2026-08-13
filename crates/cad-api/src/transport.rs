use std::{sync::Arc, time::Duration};

use reqwest::{Method, redirect::Policy};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;

use crate::{
    ApiKeyV1Credentials, Error, Result, ServerConfig, SessionContext,
    auth::TokenSet,
    config::{STORAGE_STALL_TIMEOUT, storage_redirect_policy},
    protocol::{
        AccessTokenResponse, ExchangeTokenResponse, RequestSpec, access_token_refresh,
        api_key_exchange, decode_json_bytes,
    },
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
        // Storage transfers are unbounded in total duration; only a stalled
        // peer must fail, so this client uses a read timeout instead.
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

    pub(crate) async fn exchange(&self, credentials: &ApiKeyV1Credentials) -> Result<TokenSet> {
        let response = self
            .request_from_spec(api_key_exchange(credentials))?
            .send()
            .await
            .map_err(Error::transport)?;
        let response: ExchangeTokenResponse = decode_json_response(response)
            .await
            .map_err(Error::classify_auth)?;
        response.validate()?;

        TokenSet::from_pair(response.access_token, response.refresh_token, None)
    }

    async fn refresh(&self, refresh_token: &str) -> Result<String> {
        let response = self
            .request_from_spec(access_token_refresh())?
            .bearer_auth(refresh_token)
            .send()
            .await
            .map_err(Error::transport)?;
        let response: AccessTokenResponse = decode_json_response(response)
            .await
            .map_err(Error::classify_auth)?;
        response.validate()?;
        Ok(response.access_token)
    }

    fn request_from_spec<B, T>(&self, request: RequestSpec<B, T>) -> Result<reqwest::RequestBuilder>
    where
        B: Serialize,
    {
        let url = self.config.endpoint(&request.path)?;
        let mut builder = self.api.request(request.method, url);
        if let Some(query) = request.query.as_deref() {
            builder = builder.query(query);
        }
        if let Some(body) = request.body.as_ref() {
            builder = builder.json(body);
        }
        Ok(builder)
    }
}

pub(crate) struct Session {
    tokens: Mutex<TokenSet>,
}

impl Session {
    pub(crate) fn new(tokens: TokenSet) -> Self {
        Self {
            tokens: Mutex::new(tokens),
        }
    }

    pub(crate) async fn context(&self) -> SessionContext {
        self.tokens.lock().await.context()
    }

    pub(crate) async fn bind_company(&self, company_id: crate::ids::CompanyId) -> Result<()> {
        self.tokens.lock().await.bind_company(company_id)
    }

    async fn bearer(&self, transport: &Transport) -> Result<String> {
        let mut tokens = self.tokens.lock().await;
        if tokens.should_refresh() {
            refresh_locked(transport, &mut tokens).await?;
        }
        Ok(tokens.access_token().to_owned())
    }

    pub(crate) async fn force_refresh(&self, transport: &Transport) -> Result<SessionContext> {
        let mut tokens = self.tokens.lock().await;
        refresh_locked(transport, &mut tokens).await?;
        Ok(tokens.context())
    }
}

async fn refresh_locked(transport: &Transport, tokens: &mut TokenSet) -> Result<()> {
    let access_token = transport.refresh(tokens.refresh_token()).await?;
    tokens.replace_access(access_token)
}

#[derive(Clone)]
pub(crate) struct ClientCore {
    transport: Arc<Transport>,
    session: Arc<Session>,
}

impl ClientCore {
    pub(crate) fn new(transport: Arc<Transport>, session: Arc<Session>) -> Self {
        Self { transport, session }
    }

    pub(crate) fn transport(&self) -> &Arc<Transport> {
        &self.transport
    }

    pub(crate) fn session(&self) -> &Arc<Session> {
        &self.session
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
            .request_builder(
                request.method,
                &request.path,
                request.query.as_deref(),
                request.body.as_ref(),
            )
            .await?
            .send()
            .await
            .map_err(Error::transport)?;
        decode_json_response(response).await
    }

    pub(crate) fn storage_client(&self) -> &reqwest::Client {
        &self.transport.storage
    }

    async fn request_builder<B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&[(String, String)]>,
        body: Option<&B>,
    ) -> Result<reqwest::RequestBuilder>
    where
        B: Serialize,
    {
        let access_token = self.session.bearer(&self.transport).await?;
        let url = self.transport.config.endpoint(path)?;
        let mut request = self
            .transport
            .api
            .request(method, url)
            .bearer_auth(access_token);
        if let Some(query) = query {
            request = request.query(query);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        Ok(request)
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
