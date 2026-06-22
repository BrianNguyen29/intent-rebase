//! Health check route group and observability middleware.
//!
//! Phase 3 Batch 2 Slice 2 (bounded tracing foundation): this module is
//! self-contained: the health / readiness / metrics handlers live alongside
//! the request-id and trace-context middleware that support observability for
//! the intent API. This file replaces the prior top-level `health_routes.rs`
//! and follows the A-09 / S6 "domain-grouped routes under `crate::routes`"
//! pattern that other route groups (intent, propagation, compensation, graph,
//! approval, policy, forensic, webhook) already use.
//!
//! Extracted as a bounded handler-decomposition demo slice to confirm the
//! pattern is still safe for leaf modules.

use axum::{
    extract::State, http::Request, middleware::Next, response::IntoResponse, routing::get, Json,
    Router,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::collections::HashMap;
use tracing_opentelemetry::OpenTelemetrySpanExt;

// Re-export types needed from the crate for handlers
use crate::types::HealthResponse;
use crate::AppState;

/// Global Prometheus metrics handle — initialized once at startup.
///
/// The handle is installed by `init_metrics()` in `main.rs` before the server
/// starts so that metrics recorded before the first `/metrics` scrape are not
/// lost. The `/metrics` handler reads from this handle rather than lazily
/// installing the recorder.
static METRICS_HANDLE: std::sync::OnceLock<PrometheusHandle> = std::sync::OnceLock::new();

/// Install the Prometheus metrics recorder at application startup.
///
/// Must be called exactly once before any metrics are recorded. Calling this
/// after the server has already started is safe (no-op via `OnceLock`).
pub fn init_metrics() {
    METRICS_HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("Failed to install Prometheus recorder")
    });
    tracing::info!("Prometheus metrics recorder installed");
}

// ============================================================================
// Request ID Middleware (Phase 3 Batch 2 Slice 2 — bounded tracing foundation)
// ============================================================================

/// Phase 3 Batch 2 Slice 2 (bounded tracing foundation):
/// - Extracts `X-Request-ID` header if present
/// - Generates a new UUID if not present
/// - Stores the request ID in request extensions for downstream use
/// - Does NOT propagate to response headers (Phase 3 Batch 2 remainder scope)
/// - Does NOT wire to OpenTelemetry export (future OTEL integration scope)
///
/// This enables basic request correlation for log tracing across service boundaries
/// where explicit request-id propagation is implemented.
pub async fn request_id_middleware(
    mut request: Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    // Import RequestId here to avoid import conflicts with crate::
    use crate::types::RequestId;
    use uuid::Uuid;

    // Extract or generate request ID
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Store in extensions for downstream access
    request.extensions_mut().insert(RequestId(request_id));

    // Continue with the request
    next.run(request).await
}

// ============================================================================
// W3C Trace Context Middleware (Phase 3 Batch 2 Slice 2 — bounded OTEL propagation)
// ============================================================================

/// W3C trace-context propagation middleware.
///
/// Phase 3 Batch 2 Slice 2 (bounded OTEL propagation):
/// - Extracts `traceparent` header (W3C trace-context) from inbound requests
/// - Extracts `tracestate` header if present
/// - Injects the current span context into response `traceparent` and `tracestate` headers
/// - Enables distributed tracing correlation across service boundaries
///
/// This middleware is intentionally scoped:
/// - Only propagates trace context within this service process
/// - Does NOT implement cross-process propagation (future scope)
/// - Works regardless of whether OTLP export is configured (uses tracing core APIs)
pub async fn trace_context_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    use opentelemetry::trace::TraceContextExt;

    // Build span name from method and path
    let span_name = format!("{} {}", request.method(), request.uri().path());

    // Extract W3C traceparent header for parent context
    let traceparent_value = request
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Extract W3C tracestate header if present
    let tracestate_value = request
        .headers()
        .get("tracestate")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Create the HTTP handler span
    let span = tracing::info_span!(
        "HTTP handler",
        otel.name = %span_name,
        "http.traceparent" = ?traceparent_value.as_deref().unwrap_or(""),
        "tracestate" = ?tracestate_value.as_deref().unwrap_or("")
    );

    // If we have an incoming traceparent, set it as the parent context
    if let Some(tp) = &traceparent_value {
        let extracted_context = opentelemetry::global::get_text_map_propagator(|propagator| {
            let mut carrier: HashMap<String, String> = HashMap::new();
            carrier.insert("traceparent".to_string(), tp.clone());
            if let Some(ref ts) = tracestate_value {
                carrier.insert("tracestate".to_string(), ts.clone());
            }
            propagator.extract(&carrier)
        });

        // If extraction produced a valid span, use it as parent
        if extracted_context.span().span_context().is_valid() {
            span.set_parent(extracted_context);
        }
    }

    // Execute the request within the span context and capture the span
    let response = tracing::Instrument::instrument(next.run(request), span.clone()).await;

    // Get the active span context — span is still in scope since we cloned it
    let span_context = span.context();

    // Propagate trace context to response headers using the active span
    let mut response_builder = axum::response::Response::builder();

    let otel_span = span_context.span();
    let otel_span_context = otel_span.span_context();
    if otel_span_context.is_valid() {
        // Use the W3C traceparent format: version-trace_id-span_id-trace_flags
        // e.g., "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        let trace_id_hex = format!("{:x}", otel_span_context.trace_id());
        let span_id_hex = format!("{:x}", otel_span_context.span_id());
        let trace_flags = if otel_span_context.is_sampled() {
            "01"
        } else {
            "00"
        };
        let traceparent_out = format!("00-{}-{}-{}", trace_id_hex, span_id_hex, trace_flags);
        response_builder = response_builder.header("traceparent", traceparent_out);

        // Add tracestate header if trace state is not empty
        let trace_state = otel_span_context.trace_state();
        let ts_header = trace_state.header();
        if !ts_header.is_empty() {
            response_builder = response_builder.header("tracestate", ts_header);
        }
    }

    // Convert response to builder to add headers
    let (parts, body) = response.into_parts();
    let mut response_builder = response_builder.status(parts.status).version(parts.version);

    // Preserve all existing response headers
    for (name, value) in parts.headers.iter() {
        response_builder = response_builder.header(name, value);
    }

    let response = response_builder.body(body);

    // Handle potential error building the response
    match response {
        Ok(resp) => resp,
        Err(_) => {
            // If header addition fails (shouldn't happen), return a basic error response
            axum::response::Response::new(axum::body::Body::empty())
        }
    }
}

// ============================================================================
// Health Check Handlers
// ============================================================================

/// GET /health - Returns health status with uptime
pub async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();
    Json(HealthResponse {
        status: "ok".to_string(),
        uptime_seconds: uptime,
    })
}

/// GET /ready - Returns readiness status
pub async fn ready_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ready".to_string(),
        uptime_seconds: 0,
    })
}

/// GET /metrics - Returns Prometheus-formatted metrics
pub async fn metrics_handler() -> impl IntoResponse {
    let handle = METRICS_HANDLE
        .get()
        .expect("Prometheus metrics recorder not initialized — call init_metrics() at startup");
    let metrics = handle.render();
    axum::response::Response::builder()
        .header(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )
        .body(axum::body::Body::from(metrics))
        .expect("Failed to build metrics response")
}

// ============================================================================
// Route Registration
// ============================================================================

/// Register the health route group onto the provided router.
///
/// Routes preserved (path + method):
/// - `GET /health` → `health_handler`
/// - `GET /ready` → `ready_handler`
/// - `GET /metrics` → `metrics_handler`
///
/// The request-id and trace-context middleware are **not** registered here;
/// they are layered onto the top-level router in `router::build_router` so
/// they run uniformly across all route groups (not just health).
pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/metrics", get(metrics_handler))
}
