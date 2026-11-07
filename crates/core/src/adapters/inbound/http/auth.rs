use picoserve::request::RequestParts;

const SSE_PATH: &str = "/api/events";

#[derive(Debug, Clone, Copy)]
pub struct TokenCheck {
    token: &'static str,
}

impl TokenCheck {
    pub const fn new(token: &'static str) -> Self {
        Self { token }
    }

    pub fn is_authorized(&self, parts: &RequestParts<'_>) -> bool {
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
mod tests;
