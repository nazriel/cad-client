use std::{borrow::Cow, marker::PhantomData};

use reqwest::Method;

/// Runtime-independent description of one authenticated CAD API request.
pub(crate) struct RequestSpec<Body, Response> {
    pub(crate) method: Method,
    pub(crate) path: Cow<'static, str>,
    pub(crate) query: Option<Vec<(String, String)>>,
    pub(crate) body: Option<Body>,
    response: PhantomData<fn() -> Response>,
}

impl<Body, Response> RequestSpec<Body, Response> {
    pub(crate) fn new(
        method: Method,
        path: impl Into<Cow<'static, str>>,
        query: Option<Vec<(String, String)>>,
        body: Option<Body>,
    ) -> Self {
        Self {
            method,
            path: path.into(),
            query,
            body,
            response: PhantomData,
        }
    }
}
