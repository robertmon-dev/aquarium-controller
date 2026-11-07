use super::CorsPolicy;

const POLICY: CorsPolicy = CorsPolicy::new("http://localhost:8081");

fn header(name: &str) -> &'static str {
    POLICY
        .preflight_headers()
        .into_iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("preflight is missing {name}"))
}

#[test]
fn preflight_advertises_the_configured_origin() {
    assert_eq!(
        header("Access-Control-Allow-Origin"),
        "http://localhost:8081"
    );
}

#[test]
fn preflight_allows_every_method_the_api_serves() {
    let methods = header("Access-Control-Allow-Methods");

    for method in ["GET", "PUT", "DELETE", "OPTIONS"] {
        assert!(
            methods.contains(method),
            "a browser drops a {method} request the preflight did not allow, got: {methods}"
        );
    }
}

#[test]
fn preflight_allows_the_authorization_header() {
    let headers = header("Access-Control-Allow-Headers");

    assert!(
        headers.contains("authorization"),
        "without this the browser never sends the bearer token, got: {headers}"
    );
    assert!(
        headers.contains("content-type"),
        "a json PUT sends content-type, got: {headers}"
    );
}

#[test]
fn preflight_is_cacheable() {
    let max_age = header("Access-Control-Max-Age");

    assert!(
        max_age.parse::<u32>().is_ok_and(|seconds| seconds > 0),
        "max-age must be a positive number of seconds, got: {max_age}"
    );
}
