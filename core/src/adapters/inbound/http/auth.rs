use picoserve::io::Read;
use picoserve::request::RequestParts;
use picoserve::response::{IntoResponse, ResponseWriter, StatusCode};
use picoserve::routing::{Layer, Next};
use picoserve::ResponseSent;

const SSE_PATH: &str = "/api/events";

pub struct ApiTokenLayer {
    token: &'static str,
}

impl ApiTokenLayer {
    pub const fn new(token: &'static str) -> Self {
        Self { token }
    }

    fn is_authorized(&self, parts: &RequestParts<'_>) -> bool {
        match self.presented_token(parts) {
            Some(presented) => constant_time_eq(presented.as_bytes(), self.token.as_bytes()),
            None => false,
        }
    }

    fn presented_token<'r>(&self, parts: &RequestParts<'r>) -> Option<&'r str> {
        if let Some(header) = parts.headers().get("Authorization") {
            return bearer_from_header(header.as_str().ok()?);
        }

        if parts.path() == SSE_PATH {
            return token_from_query(parts.query()?.0);
        }

        None
    }
}

impl<State, PathParameters> Layer<State, PathParameters> for ApiTokenLayer {
    type NextState = State;
    type NextPathParameters = PathParameters;

    async fn call_layer<
        'a,
        R: Read + 'a,
        NextLayer: Next<'a, R, Self::NextState, Self::NextPathParameters>,
        W: ResponseWriter<Error = R::Error>,
    >(
        &self,
        next: NextLayer,
        state: &State,
        path_parameters: PathParameters,
        request_parts: RequestParts<'_>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        if self.is_authorized(&request_parts) {
            return next.run(state, path_parameters, response_writer).await;
        }

        (StatusCode::UNAUTHORIZED, "unauthorized\n")
            .write_to(next.into_connection().await?, response_writer)
            .await
    }
}

fn bearer_from_header(raw: &str) -> Option<&str> {
    raw.strip_prefix("Bearer ").map(str::trim)
}

fn token_from_query(query: &str) -> Option<&str> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0_u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }

    diff == 0
}

#[cfg(test)]
mod tests {
    use super::{bearer_from_header, constant_time_eq, token_from_query};

    #[test]
    fn equal_slices_match() {
        assert!(constant_time_eq(b"s3cret", b"s3cret"));
    }

    #[test]
    fn different_slices_do_not_match() {
        assert!(!constant_time_eq(b"s3cret", b"s3crdt"));
    }

    #[test]
    fn differing_lengths_do_not_match() {
        assert!(!constant_time_eq(b"s3cret", b"s3cre"));
        assert!(!constant_time_eq(b"", b"s3cret"));
    }

    #[test]
    fn empty_slices_match() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn first_byte_difference_is_not_short_circuited() {
        assert!(!constant_time_eq(b"Xs3cret", b"as3cret"));
        assert!(!constant_time_eq(b"s3cretX", b"s3creta"));
    }

    #[test]
    fn extracts_bearer_token() {
        assert_eq!(bearer_from_header("Bearer s3cret"), Some("s3cret"));
    }

    #[test]
    fn rejects_non_bearer_schemes() {
        assert_eq!(bearer_from_header("Basic s3cret"), None);
        assert_eq!(bearer_from_header("s3cret"), None);
        assert_eq!(bearer_from_header(""), None);
    }

    #[test]
    fn bearer_scheme_is_case_sensitive_and_requires_a_space() {
        assert_eq!(bearer_from_header("bearer s3cret"), None);
        assert_eq!(bearer_from_header("Bearers3cret"), None);
    }

    #[test]
    fn trims_whitespace_around_the_token() {
        assert_eq!(bearer_from_header("Bearer   s3cret  "), Some("s3cret"));
    }

    #[test]
    fn extracts_token_from_query() {
        assert_eq!(token_from_query("token=s3cret"), Some("s3cret"));
        assert_eq!(token_from_query("a=1&token=s3cret&b=2"), Some("s3cret"));
    }

    #[test]
    fn query_without_token_yields_none() {
        assert_eq!(token_from_query(""), None);
        assert_eq!(token_from_query("a=1&b=2"), None);
    }

    #[test]
    fn similarly_named_query_params_are_not_mistaken_for_the_token() {
        assert_eq!(token_from_query("mytoken=s3cret"), None);
        assert_eq!(token_from_query("token_id=s3cret"), None);
    }

    #[test]
    fn empty_token_value_is_returned_as_empty_not_none() {
        assert_eq!(token_from_query("token="), Some(""));
    }
}
