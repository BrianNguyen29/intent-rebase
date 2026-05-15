use crate::error_response::ApiErrorResponse;
use crate::types::{ApiError, ErrorDetails};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use intent_rebase_types::IntentRebaseError;

#[test]
fn test_api_error_serialization() {
    let api_error = ApiError {
        error: ErrorDetails {
            code: "TEST_ERROR".to_string(),
            message: "Test message".to_string(),
            retryable: false,
            details: None,
        },
    };
    let json = serde_json::to_string(&api_error).unwrap();
    assert!(json.contains("TEST_ERROR"));
    assert!(json.contains("Test message"));
}

#[test]
fn test_api_error_response_for_invalid_header() {
    let err = IntentRebaseError::InvalidHeader("X-Expected-Version must be an integer".to_string());
    let api_err_response = ApiErrorResponse(err).into_response();
    assert_eq!(api_err_response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_api_error_response_for_serialization_error() {
    // SerializationError represents internal data corruption during SQL read/write,
    // not client input errors, so it should return 500 Internal Server Error
    let err = IntentRebaseError::SerializationError("payload corrupted in database".to_string());
    let api_err_response = ApiErrorResponse(err).into_response();
    assert_eq!(api_err_response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_api_error_response_for_unauthorized() {
    let err = IntentRebaseError::Unauthorized("Missing credentials".to_string());
    let api_err_response = ApiErrorResponse(err).into_response();
    assert_eq!(api_err_response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_api_error_response_for_invalid_api_key() {
    let err = IntentRebaseError::InvalidApiKey("Invalid key format".to_string());
    let api_err_response = ApiErrorResponse(err).into_response();
    assert_eq!(api_err_response.status(), StatusCode::UNAUTHORIZED);
}
