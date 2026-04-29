//! Metrics module for Intent Rebase Engine
//!
//! Phase 3 Batch 2 (P2-S2): Defines Prometheus metrics for SLO tracking.
//!
//! ## Metrics Naming Convention
//!
//! Metric names follow the pattern: `<service>_<operation>_<type>`
//! - `intent_api_*` - Intent API service metrics
//! - `compensation_*` - Compensation service metrics
//!
//! ## SLO Targets
//!
//! - 99.9% successful intent version creation
//! - 99.5% rebase preview availability
//! - 99.0% rebase apply path availability
//! - 99.9% audit append success
//! - p95 diff compute < 2s
//! - p95 rebase preview < 10s
//! - p95 rebase apply < 60s
//! - p95 approval wait alert threshold: 30 minutes

use metrics::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts, Registry,
};

/// Intent API metrics definitions
pub mod intent_api {
    use metrics::{
        Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts,
    };

    /// Counter for intent version creation attempts
    pub fn intent_version_created_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_version_created_total",
                "Total intent version creation attempts",
            )
            .const_label("service", "intent-api"),
            &["status"],
        )
        .expect("Failed to create intent_version_created_total counter")
    }

    /// Counter for rebase preview requests
    pub fn rebase_preview_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_rebase_preview_total",
                "Total rebase preview requests",
            )
            .const_label("service", "intent-api"),
            &["status"],
        )
        .expect("Failed to create rebase_preview_total counter")
    }

    /// Counter for rebase apply requests
    pub fn rebase_apply_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_rebase_apply_total",
                "Total rebase apply requests",
            )
            .const_label("service", "intent-api"),
            &["status"],
        )
        .expect("Failed to create rebase_apply_total counter")
    }

    /// Counter for audit append operations
    pub fn audit_append_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_audit_append_total",
                "Total audit append operations",
            )
            .const_label("service", "intent-api"),
            &["status"],
        )
        .expect("Failed to create audit_append_total counter")
    }

    /// Histogram for diff computation duration
    pub fn diff_duration_seconds() -> HistogramVec {
        HistogramVec::new(
            HistogramOpts::new(
                "intent_api_diff_duration_seconds",
                "Diff computation duration in seconds",
            )
            .const_label("service", "intent-api")
            .buckets(vec![0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 10.0]),
            &[],
        )
        .expect("Failed to create diff_duration_seconds histogram")
    }

    /// Histogram for rebase preview duration
    pub fn rebase_preview_duration_seconds() -> HistogramVec {
        HistogramVec::new(
            HistogramOpts::new(
                "intent_api_rebase_preview_duration_seconds",
                "Rebase preview computation duration in seconds",
            )
            .const_label("service", "intent-api")
            .buckets(vec![0.5, 1.0, 2.5, 5.0, 10.0, 20.0, 60.0]),
            &[],
        )
        .expect("Failed to create rebase_preview_duration_seconds histogram")
    }

    /// Histogram for rebase apply duration
    pub fn rebase_apply_duration_seconds() -> HistogramVec {
        HistogramVec::new(
            HistogramOpts::new(
                "intent_api_rebase_apply_duration_seconds",
                "Rebase apply computation duration in seconds",
            )
            .const_label("service", "intent-api")
            .buckets(vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0]),
            &[],
        )
        .expect("Failed to create rebase_apply_duration_seconds histogram")
    }

    /// Histogram for approval wait duration
    pub fn approval_wait_duration_seconds() -> HistogramVec {
        HistogramVec::new(
            HistogramOpts::new(
                "intent_api_approval_wait_duration_seconds",
                "Time spent waiting for approval in seconds",
            )
            .const_label("service", "intent-api")
            .buckets(vec![60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0]), // 1min to 2hr
            &[],
        )
        .expect("Failed to create approval_wait_duration_seconds histogram")
    }

    /// Gauge for error budget remaining (per SLO)
    pub fn error_budget_remaining() -> GaugeVec {
        GaugeVec::new(
            Opts::new(
                "intent_api_error_budget_remaining",
                "Error budget remaining (0.0 to 1.0) per SLO",
            )
            .const_label("service", "intent-api"),
            &["slo"],
        )
        .expect("Failed to create error_budget_remaining gauge")
    }

    /// Counter for total API requests
    pub fn requests_total() -> CounterVec {
        CounterVec::new(
            Opts::new("intent_api_requests_total", "Total API requests")
                .const_label("service", "intent-api"),
            &["endpoint", "method", "status"],
        )
        .expect("Failed to create requests_total counter")
    }

    /// Counter for errors
    pub fn errors_total() -> CounterVec {
        CounterVec::new(
            Opts::new("intent_api_errors_total", "Total errors")
                .const_label("service", "intent-api"),
            &["slo", "error_type"],
        )
        .expect("Failed to create errors_total counter")
    }

    // =========================================================================
    // DLQ Metrics (Phase 3 DLQ design stubs — G3 evidence)
    // =========================================================================
    // NOTE: These are metric STUBS for DLQ monitoring design.
    // Actual instrumentation requires DLQ worker implementation (Phase 4).
    // Gauge/counter labels follow Prometheus best practices.

    /// Gauge for current DLQ depth (number of messages in DLQ)
    ///
    /// Alert threshold: > 10 messages
    /// Usage: `metrics::gauge!("intent_api_dlq_messages_current").set(value)`
    pub fn dlq_messages_current() -> GaugeVec {
        GaugeVec::new(
            Opts::new(
                "intent_api_dlq_messages_current",
                "Current depth of DLQ (number of messages in dead-letter queue)",
            )
            .const_label("service", "intent-api"),
            &[],
        )
        .expect("Failed to create dlq_messages_current gauge")
    }

    /// Gauge for age of oldest message in DLQ (seconds)
    ///
    /// Alert threshold: > 3600s (1 hour)
    /// Usage: `metrics::gauge!("intent_api_dlq_message_age_seconds").set(value)`
    pub fn dlq_message_age_seconds() -> GaugeVec {
        GaugeVec::new(
            Opts::new(
                "intent_api_dlq_message_age_seconds",
                "Age of oldest message in DLQ in seconds",
            )
            .const_label("service", "intent-api"),
            &[],
        )
        .expect("Failed to create dlq_message_age_seconds gauge")
    }

    /// Counter for total replay operations
    ///
    /// Usage: `metrics::counter!("intent_api_dlq_replay_total", "status" => status).increment(1)`
    pub fn dlq_replay_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_dlq_replay_total",
                "Total DLQ replay operations",
            )
            .const_label("service", "intent-api"),
            &["status"],
        )
        .expect("Failed to create dlq_replay_total counter")
    }

    /// Counter for failed replay attempts
    ///
    /// Alert threshold: > 0
    /// Usage: `metrics::counter!("intent_api_dlq_replay_failures_total").increment(1)`
    pub fn dlq_replay_failures_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_dlq_replay_failures_total",
                "Total failed DLQ replay attempts",
            )
            .const_label("service", "intent-api"),
            &[],
        )
        .expect("Failed to create dlq_replay_failures_total counter")
    }

    /// Counter for total messages ever sent to DLQ
    ///
    /// N/A (counter — monotonic)
    /// Usage: `metrics::counter!("intent_api_dlq_messages_total").increment(1)`
    pub fn dlq_messages_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "intent_api_dlq_messages_total",
                "Total messages ever sent to DLQ",
            )
            .const_label("service", "intent-api"),
            &[],
        )
        .expect("Failed to create dlq_messages_total counter")
    }
}

/// Compensation service metrics definitions
pub mod compensation {
    use super::*;

    /// Counter for compensation action executions
    pub fn action_executed_total() -> CounterVec {
        CounterVec::new(
            Opts::new(
                "compensation_action_executed_total",
                "Total compensation action executions",
            )
            .const_label("service", "compensation-service"),
            &["status", "strategy", "feasibility"],
        )
        .expect("Failed to create compensation_action_executed_total counter")
    }

    /// Histogram for compensation execution duration
    pub fn execution_duration_seconds() -> HistogramVec {
        HistogramVec::new(
            HistogramOpts::new(
                "compensation_execution_duration_seconds",
                "Compensation execution duration in seconds",
            )
            .const_label("service", "compensation-service")
            .buckets(vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0]),
            &[],
        )
        .expect("Failed to create execution_duration_seconds histogram")
    }
}

/// Register all metrics with the global registry
pub fn register_metrics(registry: &Registry) {
    // Intent API metrics
    registry
        .register(intent_api::intent_version_created_total())
        .expect("Failed to register intent_version_created_total");
    registry
        .register(intent_api::rebase_preview_total())
        .expect("Failed to register rebase_preview_total");
    registry
        .register(intent_api::rebase_apply_total())
        .expect("Failed to register rebase_apply_total");
    registry
        .register(intent_api::audit_append_total())
        .expect("Failed to register audit_append_total");
    registry
        .register(intent_api::diff_duration_seconds())
        .expect("Failed to register diff_duration_seconds");
    registry
        .register(intent_api::rebase_preview_duration_seconds())
        .expect("Failed to register rebase_preview_duration_seconds");
    registry
        .register(intent_api::rebase_apply_duration_seconds())
        .expect("Failed to register rebase_apply_duration_seconds");
    registry
        .register(intent_api::approval_wait_duration_seconds())
        .expect("Failed to register approval_wait_duration_seconds");
    registry
        .register(intent_api::error_budget_remaining())
        .expect("Failed to register error_budget_remaining");
    registry
        .register(intent_api::requests_total())
        .expect("Failed to register requests_total");
    registry
        .register(intent_api::errors_total())
        .expect("Failed to register errors_total");

    // DLQ metrics (Phase 3 DLQ design stubs — G3 evidence)
    // NOTE: These are metric STUBS for DLQ monitoring design.
    // Actual instrumentation requires DLQ worker implementation (Phase 4).
    registry
        .register(intent_api::dlq_messages_current())
        .expect("Failed to register dlq_messages_current");
    registry
        .register(intent_api::dlq_message_age_seconds())
        .expect("Failed to register dlq_message_age_seconds");
    registry
        .register(intent_api::dlq_replay_total())
        .expect("Failed to register dlq_replay_total");
    registry
        .register(intent_api::dlq_replay_failures_total())
        .expect("Failed to register dlq_replay_failures_total");
    registry
        .register(intent_api::dlq_messages_total())
        .expect("Failed to register dlq_messages_total");

    // Compensation service metrics
    registry
        .register(compensation::action_executed_total())
        .expect("Failed to register compensation_action_executed_total");
    registry
        .register(compensation::execution_duration_seconds())
        .expect("Failed to register execution_duration_seconds");
}
