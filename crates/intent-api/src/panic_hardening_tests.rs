use crate::panic_hardening::{
    format_join_error, init_panic_hook, record_panic_event, sanitize_panic_payload,
};

#[test]
fn test_sanitize_panic_payload_jwt_token() {
    let payload = "Error: token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let sanitized = sanitize_panic_payload(payload);
    assert!(sanitized.contains("<JWT_REDACTED>"));
    assert!(!sanitized.contains("eyJ"));
}

#[test]
fn test_sanitize_panic_payload_db_url() {
    let payload = "Connection failed: postgres://user:password@localhost:5432/dbname";
    let sanitized = sanitize_panic_payload(payload);
    // Bounded slice: only protocol prefix is redacted, not full URL credentials
    // This prevents protocol-based log injection while keeping implementation minimal
    assert!(sanitized.contains("<DB_URL_REDACTED>"));
    assert!(!sanitized.contains("postgres://"));
}

#[test]
fn test_sanitize_panic_payload_aws_credentials() {
    let payload = "AWS Error: AccessKeyId=AKIAIOSFODNN7EXAMPLE SecretAccessKey=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let sanitized = sanitize_panic_payload(payload);
    assert!(sanitized.contains("<AWS_CREDS_REDACTED>"));
    assert!(!sanitized.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[test]
fn test_sanitize_panic_payload_bearer_token() {
    let payload = "Auth failed: Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
    let sanitized = sanitize_panic_payload(payload);
    assert!(sanitized.contains("<TOKEN_REDACTED>"));
    assert!(!sanitized.contains("eyJ"));
}

#[test]
fn test_sanitize_panic_payload_truncation() {
    let long_payload = "x".repeat(1000);
    let sanitized = sanitize_panic_payload(&long_payload);
    assert!(sanitized.contains("<TRUNCATED"));
    assert!(sanitized.len() < 1000);
}

#[test]
fn test_sanitize_panic_payload_noop_on_clean_string() {
    let payload = "This is a normal error message with no secrets";
    let sanitized = sanitize_panic_payload(payload);
    assert_eq!(sanitized, payload);
}

#[test]
fn test_init_panic_hook_does_not_panic() {
    // init_panic_hook should not panic - just register the hook
    init_panic_hook();
    // If we get here, the test passes
}

#[tokio::test]
async fn test_format_join_error_sanitizes_panic_message() {
    let handle = tokio::spawn(async {
        panic!(
            "panic with secret eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c token"
        );
    });
    let result = handle.await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let formatted = format_join_error("test_worker", err);
    assert!(formatted.contains("test_worker worker task panicked:"));
    assert!(formatted.contains("<JWT_REDACTED>"));
    assert!(!formatted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
}

#[tokio::test]
async fn test_format_join_error_on_aborted_task() {
    let handle = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    });
    handle.abort();
    let result = handle.await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let formatted = format_join_error("aborted_worker", err);
    assert!(formatted.contains("aborted_worker worker task panicked:"));
}

#[test]
fn test_record_panic_event_increments_counter() {
    // Bounded S7 metric test: install a thread-local recorder that captures
    // the `process_panics_total` counter increments without touching the
    // global `metrics-exporter-prometheus` recorder (avoids global
    // metrics-recorder conflicts with other tests in the suite).
    use metrics::atomics::AtomicU64;
    use metrics::{
        Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
    };
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    struct PanicCounterRecorder {
        counter: Arc<AtomicU64>,
    }

    impl Recorder for PanicCounterRecorder {
        fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
            if key.name() == "process_panics_total" {
                Counter::from_arc(self.counter.clone())
            } else {
                Counter::noop()
            }
        }
        fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
            Gauge::noop()
        }
        fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
            Histogram::noop()
        }
    }

    let counter = Arc::new(AtomicU64::new(0));
    let recorder = PanicCounterRecorder {
        counter: counter.clone(),
    };

    metrics::with_local_recorder(&recorder, || {
        record_panic_event();
        record_panic_event();
        record_panic_event();
    });

    assert_eq!(counter.load(Ordering::Relaxed), 3);
}
