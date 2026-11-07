use picoserve::io::Read;
use picoserve::request::RequestParts;
use picoserve::response::{IntoResponse, NoContent, ResponseWriter, StatusCode};
use picoserve::routing::{Layer, Next};
use picoserve::ResponseSent;

use crate::adapters::inbound::http::auth::TokenCheck;
use crate::adapters::inbound::http::cors::CorsPolicy;

pub struct ApiLayer {
    token: TokenCheck,
    cors: CorsPolicy,
}

impl ApiLayer {
    pub const fn new(token: &'static str, origin: &'static str) -> Self {
        Self {
            token: TokenCheck::new(token),
            cors: CorsPolicy::new(origin),
        }
    }
}

impl<State, PathParameters> Layer<State, PathParameters> for ApiLayer {
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
        if request_parts.method() == "OPTIONS" {
            return (
                StatusCode::NO_CONTENT,
                self.cors.preflight_headers(),
                NoContent,
            )
                .write_to(next.into_connection().await?, response_writer)
                .await;
        }

        if !self.token.is_authorized(&request_parts) {
            return (StatusCode::UNAUTHORIZED, "unauthorized\n")
                .write_to(
                    next.into_connection().await?,
                    self.cors.writer(response_writer),
                )
                .await;
        }

        next.run(state, path_parameters, self.cors.writer(response_writer))
            .await
    }
}
