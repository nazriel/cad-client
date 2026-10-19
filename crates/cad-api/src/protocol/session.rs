use reqwest::Method;

use crate::Company;

use super::RequestSpec;

const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

pub(crate) fn companies() -> RequestSpec<(), Vec<Company>> {
    RequestSpec::new(Method::GET, COMPANIES_PATH, None, None)
}

#[cfg(test)]
mod tests {
    use super::{COMPANIES_PATH, companies};

    #[test]
    fn companies_recipe_preserves_path() {
        let list = companies();
        assert_eq!(list.method, reqwest::Method::GET);
        assert_eq!(list.path, COMPANIES_PATH);
        assert!(list.body.is_none());
    }
}
