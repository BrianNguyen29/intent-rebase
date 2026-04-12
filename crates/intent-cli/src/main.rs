//! Intent Rebase Engine CLI
//!
//! Phase 3 Batch 1: Bounded single-shot sync CLI for compensation action orchestration.
//!
//! **Bounded scope:**
//! - Single-shot: sync run over explicit action IDs, auto-decide approve|reapprove|execute|skip
//! - No queue polling, no distributed claiming/locking, no scheduler
//! - HTTP transport: talks to intent-api HTTP endpoints
//! - Uses existing CompensationActionService methods via HTTP

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;
use uuid::Uuid;

/// CLI for Intent Rebase Engine - Compensation Action Orchestration
///
/// Phase 3 Batch 1: Bounded single-shot sync CLI for compensation action orchestration.
#[derive(Parser)]
#[command(name = "intent-cli")]
#[command(about = "Intent Rebase Engine CLI - Compensation Action Orchestration", long_about = None)]
struct Cli {
    /// API base URL (default: http://localhost:8080)
    #[arg(short, long, default_value = "http://localhost:8080")]
    api_url: String,

    /// Tenant ID (required for all commands)
    #[arg(short, long)]
    tenant_id: Uuid,

    /// Optional authentication API key
    #[arg(short, long)]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run orchestration for explicit compensation action IDs (single-shot sync).
    ///
    /// Phase 3 Batch 1: Auto-decides approve|reapprove|execute|skip per action
    /// and returns the persisted run handle with per-item results.
    Run {
        /// Comma-separated list of compensation action IDs to process.
        #[arg(short, long, value_delimiter = ',')]
        action_ids: Vec<Uuid>,

        /// Optional intent scope for this run.
        #[arg(short, long)]
        intent_id: Option<Uuid>,

        /// Actor who initiated this run (for audit purposes).
        #[arg(short, long)]
        initiated_by: Option<String>,
    },

    /// Get an existing orchestration run by ID.
    GetRun {
        /// The run ID to retrieve.
        #[arg(short, long)]
        run_id: Uuid,
    },
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Run {
            action_ids,
            intent_id,
            initiated_by,
        } => {
            run_orchestration(
                &cli.api_url,
                cli.tenant_id,
                action_ids.clone(),
                *intent_id,
                initiated_by.clone(),
                cli.api_key.as_deref(),
            )?;
        }
        Commands::GetRun { run_id } => {
            get_run(&cli.api_url, cli.tenant_id, *run_id, cli.api_key.as_deref())?;
        }
    }

    Ok(())
}

/// Execute a single-shot orchestration run via POST /compensation-actions/runs.
///
/// Phase 3 Batch 1: Creates a run (HTTP 202) and polls GET /compensation-actions/runs/{run_id}
/// until the run reaches a terminal status (completed, completed_with_errors, failed).
fn run_orchestration(
    api_url: &str,
    tenant_id: Uuid,
    action_ids: Vec<Uuid>,
    intent_id: Option<Uuid>,
    initiated_by: Option<String>,
    api_key: Option<&str>,
) -> Result<()> {
    let request = serde_json::json!({
        "action_ids": action_ids,
        "intent_id": intent_id,
        "initiated_by": initiated_by
    });

    let url = format!(
        "{}/compensation-actions/runs?tenant_id={}",
        api_url.trim_end_matches('/'),
        tenant_id
    );

    info!("Creating orchestration run: POST {}", url);

    let mut req = ureq::post(&url);

    if let Some(key) = api_key {
        req = req.set("X-API-Key", key);
    }

    let response = req
        .send_json(&request)
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if response.status() >= 400 {
        let status = response.status();
        let text = response.into_string().unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, text);
    }

    let run: intent_api::OrchestrationRunResponse = response
        .into_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    let run_id = run.id;
    println!("\n=== Orchestration Run Started ===");
    println!("Run ID: {}", run_id);
    println!("Status: {} (polling for completion...)", run.status);
    println!("Created: {}", run.created_at);

    // Poll until terminal status
    let terminal_statuses = ["completed", "completed_with_errors", "failed"];
    let poll_url = format!(
        "{}/compensation-actions/runs/{}?tenant_id={}",
        api_url.trim_end_matches('/'),
        run_id,
        tenant_id
    );

    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut req = ureq::get(&poll_url);
        if let Some(key) = api_key {
            req = req.set("X-API-Key", key);
        }

        let response = match req.call() {
            Ok(r) => r,
            Err(e) => {
                anyhow::bail!("Poll request failed: {}", e);
            }
        };

        if response.status() >= 400 {
            let status = response.status();
            let text = response.into_string().unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let run: intent_api::OrchestrationRunResponse = response
            .into_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse poll response: {}", e))?;

        if terminal_statuses.contains(&run.status.as_str()) {
            // Print final results
            println!("\n=== Orchestration Run Results ===");
            println!("Run ID: {}", run.id);
            println!("Status: {}", run.status);
            println!("Created: {}", run.created_at);
            if let Some(started) = run.started_at {
                println!("Started: {}", started);
            }
            if let Some(completed) = run.completed_at {
                println!("Completed: {}", completed);
            }
            println!("\nCounts:");
            println!("  Succeeded: {}", run.succeeded_count);
            println!("  Failed: {}", run.failed_count);
            println!("  Skipped: {}", run.skipped_count);
            println!("  Not Found: {}", run.not_found_count);
            println!("  Total: {}", run.total_count);

            if !run.item_results.is_empty() {
                println!("\nPer-Action Results:");
                for item in &run.item_results {
                    let status = if item.success { "✓" } else { "✗" };
                    println!(
                        "  {} Action {} ({}) -> {} [{}]",
                        status,
                        item.action_id,
                        item.action_taken,
                        item.resulting_status,
                        item.reason
                    );
                }
            }
            return Ok(());
        }

        print!(".");
    }
}

/// Get an existing orchestration run via GET /compensation-actions/runs/{run_id}.
fn get_run(api_url: &str, tenant_id: Uuid, run_id: Uuid, api_key: Option<&str>) -> Result<()> {
    let url = format!(
        "{}/compensation-actions/runs/{}?tenant_id={}",
        api_url.trim_end_matches('/'),
        run_id,
        tenant_id
    );

    info!("Getting orchestration run: GET {}", url);

    let mut req = ureq::get(&url);

    if let Some(key) = api_key {
        req = req.set("X-API-Key", key);
    }

    let response = req
        .call()
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if response.status() >= 400 {
        let status = response.status();
        let text = response.into_string().unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, text);
    }

    let run: intent_api::OrchestrationRunResponse = response
        .into_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    println!("\n=== Orchestration Run ===");
    println!("Run ID: {}", run.id);
    println!("Tenant ID: {}", run.tenant_id);
    if let Some(intent_id) = run.intent_id {
        println!("Intent ID: {}", intent_id);
    }
    println!("Status: {}", run.status);
    println!("Initiated By: {:?}", run.initiated_by);
    println!("Created: {}", run.created_at);
    if let Some(started) = run.started_at {
        println!("Started: {}", started);
    }
    if let Some(completed) = run.completed_at {
        println!("Completed: {}", completed);
    }
    println!("\nCounts:");
    println!("  Succeeded: {}", run.succeeded_count);
    println!("  Failed: {}", run.failed_count);
    println!("  Skipped: {}", run.skipped_count);
    println!("  Not Found: {}", run.not_found_count);
    println!("  Total: {}", run.total_count);

    if !run.item_results.is_empty() {
        println!("\nPer-Action Results:");
        for item in &run.item_results {
            let status = if item.success { "✓" } else { "✗" };
            println!(
                "  {} Action {} ({}) -> {} [{}]",
                status, item.action_id, item.action_taken, item.resulting_status, item.reason
            );
        }
    }

    Ok(())
}
