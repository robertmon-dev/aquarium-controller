use picoserve::io::Read;
use picoserve::response::{Body, Connection, HeadersIter, Response, ResponseWriter};
use picoserve::ResponseSent;

pub const ORIGIN_HEADER: &str = "Access-Control-Allow-Origin";

const ALLOW_METHODS: &str = "GET, PUT, DELETE, OPTIONS";
const ALLOW_HEADERS: &str = "authorization, content-type";
const MAX_AGE_SECONDS: &str = "600";

#[derive(Debug, Clone, Copy)]
pub struct CorsPolicy {
    origin: &'static str,
}

impl CorsPolicy {
    pub const fn new(origin: &'static str) -> Self {
        Self { origin }
    }

    pub const fn preflight_headers(&self) -> [(&'static str, &'static str); 4] {
        [
            (ORIGIN_HEADER, self.origin),
            ("Access-Control-Allow-Methods", ALLOW_METHODS),
            ("Access-Control-Allow-Headers", ALLOW_HEADERS),
            ("Access-Control-Max-Age", MAX_AGE_SECONDS),
        ]
    }

    pub fn writer<W>(&self, inner: W) -> OriginWriter<W> {
        OriginWriter {
            inner,
            origin: self.origin,
        }
    }
}

pub struct OriginWriter<W> {
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

#[cfg(test)]
mod tests;
