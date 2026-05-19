use serde::{Deserialize, Serialize};

// =============================================================================
// Health and Request ID Types (Phase 2 bounded extraction)
// =============================================================================

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// Request ID stored in request extensions by the request_id_middleware.
#[derive(Clone)]
pub struct RequestId(pub String);
