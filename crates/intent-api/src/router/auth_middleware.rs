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
    use jsonwebtoken::{decode, DecodingKey, Validation};

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

            match decode::<crate::auth::Claims>(
                token,
                &DecodingKey::from_secret(auth_config.jwt_secret.as_bytes()),
                &Validation::new(auth_config.algorithm),
            ) {
                Ok(token_data) => {
                    let mut request = request;
                    request.extensions_mut().insert(token_data.claims);
                    next.run(request).await
                }
                Err(_) => axum::response::Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body("Invalid or expired token".into())
                    .unwrap(),
            }
        }
        _ => axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body("Missing or invalid Authorization header".into())
            .unwrap(),
    }
}
