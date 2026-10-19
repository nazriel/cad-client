use reqwest::Method;

use crate::SessionContext;

use super::RequestSpec;

const MACHINE_IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";

pub(crate) fn machine_identity() -> RequestSpec<(), SessionContext> {
    RequestSpec::new(Method::GET, MACHINE_IDENTITY_PATH, None, None)
}

#[cfg(test)]
mod tests {
    use super::{MACHINE_IDENTITY_PATH, machine_identity};

    #[test]
    fn machine_identity_request_uses_the_v2_contract() {
        let request = machine_identity();
        assert_eq!(request.method, reqwest::Method::GET);
        assert_eq!(request.path, MACHINE_IDENTITY_PATH);
        assert!(request.body.is_none());
    }
}
