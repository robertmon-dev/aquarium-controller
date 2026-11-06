use picoserve::io::Read;
use picoserve::request::RequestParts;
use picoserve::response::{
    Body, Connection, HeadersIter, IntoResponse, NoContent, Response, ResponseWriter, StatusCode,
};
use picoserve::routing::{Layer, Next};
use picoserve::ResponseSent;

const ORIGIN_HEADER: &str = "Access-Control-Allow-Origin";
const ALLOW_METHODS: &str = "GET, PUT, DELETE, OPTIONS";
const ALLOW_HEADERS: &str = "authorization, content-type";
const MAX_AGE_SECONDS: &str = "600";

pub struct CorsLayer {
    origin: &'static str,
}

impl CorsLayer {
    pub const fn new(origin: &'static str) -> Self {
        Self { origin }
    }
}

struct OriginWriter<W> {
    inner: W,
    origin: &'static str,
}

impl<W: ResponseWriter> ResponseWriter for OriginWriter<W> {
    type Error = W::Error;

    async fn write_response<R: Read<Error = Self::Error>, H: HeadersIter, B: Body>(
        self,
        connection: Connection<'_, R>,
        response: Response<H, B>,
    ) -> Result<ResponseSent, Self::Error> {
        self.inner
            .write_response(connection, response.with_header(ORIGIN_HEADER, self.origin))
            .await
    }
}

impl<State, PathParameters> Layer<State, PathParameters> for CorsLayer {
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
                [
                    (ORIGIN_HEADER, self.origin),
                    ("Access-Control-Allow-Methods", ALLOW_METHODS),
                    ("Access-Control-Allow-Headers", ALLOW_HEADERS),
                    ("Access-Control-Max-Age", MAX_AGE_SECONDS),
                ],
                NoContent,
            )
                .write_to(next.into_connection().await?, response_writer)
                .await;
        }

        next.run(
            state,
            path_parameters,
            OriginWriter {
                inner: response_writer,
                origin: self.origin,
            },
        )
        .await
    }
}
