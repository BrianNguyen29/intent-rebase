//! Intent Rebase Engine CLI — binary entry point.
//!
//! Phase 3 Batch 1: Bounded single-shot sync CLI for compensation action
//! orchestration. The binary is intentionally minimal: argument parsing and
//! command dispatch live in the `intent_cli` library crate (`src/lib.rs`) so
//! that command logic can be unit-tested without going through this entry
//! point. See `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §7
//! (P2 Intent CLI Decoupling) for the bounded-slice rationale.

use anyhow::Result;
use clap::Parser;
use intent_cli::{run, Cli};

fn main() -> Result<()> {
    run(Cli::parse())
}
