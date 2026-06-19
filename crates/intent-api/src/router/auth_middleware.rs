//! JWT authentication middleware.
//!
//! Bounded router decomposition slice: validates Bearer tokens on protected
//! routes and bypasses public paths (/health, /ready, /metrics).
//! Gated by the `jwt-auth` feature.

use axum::http::StatusCode;

/// JWT authentication middleware for protected routes.
///
/// Public paths (/health, /ready, /metrics) bypass JWT validation.
#[cfg(feature = "jwt-auth")]
pub(crate) async fn jwt_auth_async(
    auth_config: crate::auth::AuthConfig,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::header;

    const PUBLIC_PATHS: &[&str] = &["/health", "/ready", "/metrics"];
    let path = request.uri().path();

    // Skip JWT check for public paths
    if PUBLIC_PATHS.contains(&path) {
        return next.run(request).await;
    }

    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok());

    match auth_header {
        Some(auth_value) if auth_value.starts_with("Bearer ") => {
            let token = &auth_value[7..];

            match auth_config.verify_token(token) {
                Ok(claims) => {
                    let mut request = request;
                    request.extensions_mut().insert(claims);
                    next.run(request).await
                }
                Err(_) => axum::response::Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(axum::body::Body::from(
                        r#"{"error":{"code":"UNAUTHORIZED","message":"Invalid or expired token"}}"#,
                    ))
                    .unwrap(),
            }
        }
        _ => axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CACHE_CONTROL, "no-store")
            .body(axum::body::Body::from(
                r#"{"error":{"code":"UNAUTHORIZED","message":"Missing or invalid Authorization header"}}"#,
            ))
            .unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;

    #[test]
    fn test_unauthorized_response_builder_has_headers() {
        let response = axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CACHE_CONTROL, "no-store")
            .body(axum::body::Body::from(
                r#"{"error":{"code":"UNAUTHORIZED","message":"test"}}"#,
            ))
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
    }
}
