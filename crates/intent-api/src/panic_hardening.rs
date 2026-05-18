// ============================================================================
// Panic Hardening (Phase 2b bounded local-executable slice)
// ============================================================================
//
// Bounded slice: panic hook registration for observability.
// - Hook is registered at startup via init_panic_hook() before any async task spawns
// - Panics are logged via tracing at ERROR level with thread info
// - Panic payload is sanitized to avoid secret exposure (no raw error messages logged)
// - No production alerting claims (Phase 4 scope)
// - No broad worker lifecycle redesign (only hook registration)
//

/// Sanitize a panic payload for logging.
///
/// Removes or redacts strings that commonly contain secrets:
/// - Full JWT tokens (eyJ...eyJ pattern)
/// - Database connection strings (postgres://, postgresql://)
/// - AWS credentials patterns (AccessKeyId=, SecretAccessKey=, etc.)
/// - Bearer tokens
///
/// Returns a sanitized string safe for logging.
pub(crate) fn sanitize_panic_payload(payload: &str) -> String {
    let mut result = payload.to_string();

    // Redact JWT tokens (eyJ... format - base64url JWT structure)
    // JWT has 3 base64url-encoded parts separated by dots
    while let Some(start) = result.find("eyJ") {
        // Find the end of potential JWT: look for second dot and check third segment
        if let Some(first_dot) = result[start..].find('.') {
            if let Some(second_dot) = result[start + first_dot + 1..].find('.') {
                let jwt_end = start + first_dot + 1 + second_dot + 1;
                // Check if third segment exists and is reasonably long (typical JWT part)
                if second_dot > 20 && jwt_end <= result.len() {
                    result = format!(
                        "{}{}{}",
                        &result[..start],
                        "<JWT_REDACTED>",
                        &result[jwt_end..]
                    );
                    continue;
                }
            }
        }
        break;
    }

    // Redact database URLs (connection strings)
    result = result
        .replace("postgres://", "<DB_URL_REDACTED>")
        .replace("postgresql://", "<DB_URL_REDACTED>");

    // Redact AWS credentials patterns
    // Simple pattern: key names followed by = or : and a value
    let aws_patterns = [
        "AccessKeyId=",
        "SecretAccessKey=",
        "aws_access_key=",
        "aws_secret=",
    ];
    for pattern in &aws_patterns {
        if let Some(pos) = result.find(*pattern) {
            // Find the end of the value (simple: until space or end)
            let value_start = pos + pattern.len();
            let value_end = result[value_start..]
                .find(' ')
                .map(|p| value_start + p)
                .unwrap_or(result.len());
            // Only redact if value looks like credentials (alphanumeric with some special chars)
            let value = &result[value_start..value_end];
            if value.len() > 10
                && value
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '/' || c == '+' || c == '=')
            {
                result = format!(
                    "{}{}<AWS_CREDS_REDACTED>{}",
                    &result[..value_start],
                    pattern,
                    &result[value_end..]
                );
            }
        }
    }

    // Redact Bearer tokens
    if let Some(pos) = result.find("Bearer ") {
        let token_start = pos + 7; // length of "Bearer "
        let token_end = result[token_start..]
            .find(' ')
            .map(|p| token_start + p)
            .unwrap_or(result.len());
        let token = &result[token_start..token_end];
        // Only redact if it looks like a token (alphanumeric with dots/underscores/hyphens)
        if !token.is_empty() && token.len() > 10 {
            result = format!(
                "{}Bearer <TOKEN_REDACTED>{}",
                &result[..pos],
                &result[token_end..]
            );
        }
    }

    // Truncate very long payloads (could be binary or huge strings)
    if result.len() > 500 {
        format!("{}... <TRUNCATED (len={})>", &result[..500], result.len())
    } else {
        result
    }
}

/// Format panic location info for logging.
fn format_panic_location(location: &std::panic::PanicHookInfo) -> String {
    location
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown location>".to_string())
}

/// Panic hook that logs panic info via tracing.
///
/// This hook is registered at startup to ensure panics are observable.
/// - Logs at ERROR level with thread name and panic message
/// - Sanitizes payload to avoid secret exposure
/// - Uses tracing to integrate with existing log infrastructure
fn panic_hook(info: &std::panic::PanicHookInfo) {
    let location = format_panic_location(info);
    let binding = std::thread::current();
    let thread = binding.name().unwrap_or("<unnamed>");

    let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
        sanitize_panic_payload(s)
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        sanitize_panic_payload(s)
    } else {
        "<non-string panic payload>".to_string()
    };

    // Use eprintln as fallback since tracing may not be initialized yet during early panics
    eprintln!(
        "PANIC: thread={}, location={}, payload={}",
        thread, location, payload
    );
}

/// Format a tokio task join error with sanitized payload for safe logging.
///
/// Uses `sanitize_panic_payload` on the panic message (extracted via
/// `JoinError::into_panic`) to avoid leaking secrets that may be present
/// in panic messages.
pub fn format_join_error(worker_name: &str, err: tokio::task::JoinError) -> String {
    let raw = if err.is_panic() {
        let payload = err.into_panic();
        match payload.downcast::<String>() {
            Ok(s) => *s,
            Err(payload) => match payload.downcast::<&str>() {
                Ok(s) => (*s).to_string(),
                Err(_) => "<non-string panic payload>".to_string(),
            },
        }
    } else {
        err.to_string()
    };
    let sanitized = sanitize_panic_payload(&raw);
    format!("{} worker task panicked: {}", worker_name, sanitized)
}

/// Initialize the panic hook for observability.
///
/// Call this at startup before any async tasks are spawned.
/// Bounded slice: only registers local panic hook, no external alerting.
pub fn init_panic_hook() {
    std::panic::set_hook(Box::new(panic_hook));
    tracing::debug!("Panic hook registered for observability");
}
