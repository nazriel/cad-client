use reqwest::Method;

use super::{COMPANY_TEMPLATES_PATH, requests};

#[test]
fn list_recipe_preserves_the_normalized_template_path() {
    let list = requests::list();
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, COMPANY_TEMPLATES_PATH);
    assert!(list.body.is_none());
}
