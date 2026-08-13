use serde::de::DeserializeOwned;

use crate::{Error, Result};

pub(crate) fn decode_json_bytes<T>(status: u16, bytes: &[u8]) -> Result<T>
where
    T: DeserializeOwned,
{
    if !(200..300).contains(&status) {
        return Err(Error::from_api_response(status, bytes));
    }
    if bytes.is_empty() {
        // A body-less success (204, or 200 with an empty body) is only valid
        // for a response type that can be built from nothing, such as `()`.
        return serde_json::from_str::<T>("null").map_err(|_| {
            Error::protocol(format!("HTTP {status} response unexpectedly had no body"))
        });
    }

    serde_json::from_slice(bytes)
        .map_err(|_| Error::protocol("successful response contained invalid JSON"))
}

#[cfg(test)]
mod tests {
    use super::decode_json_bytes;

    #[test]
    fn body_less_success_decodes_into_a_unit_response() {
        assert!(decode_json_bytes::<()>(204, b"").is_ok());
        assert!(decode_json_bytes::<Option<u8>>(200, b"").unwrap().is_none());
    }

    #[test]
    fn body_less_success_still_fails_a_type_that_needs_data() {
        let error = decode_json_bytes::<Vec<u8>>(204, b"").expect_err("a list needs a body");
        assert!(error.to_string().contains("no body"));
    }

    #[test]
    fn error_status_is_classified_before_the_empty_body_check() {
        let error = decode_json_bytes::<()>(500, b"").expect_err("5xx must not decode as success");
        assert!(error.to_string().contains("HTTP 500"));
    }
}
