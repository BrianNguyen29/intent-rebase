//! Tracing initialization module (Phase 3 Batch 2 Slice 2 bounded slice)
//!
//! Extracts OTLP-aware tracing init from lib.rs for modular decomposition.

use opentelemetry::trace::TracerProvider;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing with optional OTLP export.
///
/// When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, this initializes the OpenTelemetry
/// SDK with an OTLP exporter and a tokio runtime extension for background export.
/// When the env var is absent, only JSON logging to stdout is active (existing behavior).
///
/// Phase 3 Batch 2 Slice 2 OTEL extension (bounded slice):
/// - Optional OTLP export when endpoint is configured via env var
/// - Retains existing JSON logging fallback when OTEL is not configured
/// - Does NOT implement cross-process trace context propagation (future scope)
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json());

    // Optionally wire in OTLP export if endpoint is configured
    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        // Use the pipeline pattern to set up OTLP with batch export
        let tracer_provider = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(opentelemetry_otlp::new_exporter().tonic())
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .expect("Failed to create OTLP tracer provider — check OTEL_EXPORTER_OTLP_ENDPOINT");

        // Set as global provider so tracing-opentelemetry layer can use it
        let _ = opentelemetry::global::set_tracer_provider(tracer_provider.clone());

        // Set global W3C trace-context propagator so extraction/injection work
        let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
        opentelemetry::global::set_text_map_propagator(propagator);

        // Create tracing-opentelemetry layer with the tracer
        let tracer = tracer_provider.tracer("intent-api");
        let tracer_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        registry.with(tracer_layer).init();
        tracing::info!("OTLP tracing enabled via OTEL_EXPORTER_OTLP_ENDPOINT");
    } else {
        // Set global W3C trace-context propagator even without OTLP
        // so trace_context_middleware extraction/injection works
        let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
        opentelemetry::global::set_text_map_propagator(propagator);

        registry.init();
        tracing::info!("OTLP tracing disabled (OTEL_EXPORTER_OTLP_ENDPOINT not set)");
    }
}
