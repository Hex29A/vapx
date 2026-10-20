//! Bearer-token check for the HTTP transport.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Whether an `Authorization` header value carries exactly `expected` as its
/// bearer token. Both sides are hashed first so the comparison takes the same
/// time whatever the length of the supplied token.
pub fn token_matches(expected: &str, header_value: Option<&str>) -> bool {
    let supplied = header_value
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim())
        .unwrap_or("");
    let a = Sha256::digest(expected.as_bytes());
    let b = Sha256::digest(supplied.as_bytes());
    // An absent token must never match, not even an empty expected one.
    let present = !supplied.is_empty();
    bool::from(a.ct_eq(&b)) && present
}

/// axum middleware: 401 unless the request carries the configured token.
pub async fn require_token(State(token): State<Arc<String>>, req: Request, next: Next) -> Response {
    let supplied = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if token_matches(&token, supplied) {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "Unauthorized",
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_header_is_rejected() {
        assert!(!token_matches("secret", None));
    }

    #[test]
    fn wrong_token_is_rejected() {
        assert!(!token_matches("secret", Some("Bearer nope")));
        assert!(!token_matches("secret", Some("Bearer secre")));
        assert!(!token_matches("secret", Some("Bearer secrets")));
    }

    #[test]
    fn right_token_is_accepted() {
        assert!(token_matches("secret", Some("Bearer secret")));
        assert!(token_matches("secret", Some("bearer secret")));
    }

    #[test]
    fn other_schemes_and_empty_tokens_are_rejected() {
        assert!(!token_matches("secret", Some("Basic secret")));
        assert!(!token_matches("secret", Some("secret")));
        assert!(!token_matches("", Some("Bearer ")));
        assert!(!token_matches("", None));
    }
}
