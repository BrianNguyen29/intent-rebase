//! Forensic Service — Phase 3 Batch 3b bounded slice
//!
//! This crate is responsible for forensic replay bundles
//! for incident investigation, compliance audits, and legal proceedings.
//!
//! **Phase 3 Batch 3b bounded slice (this delivery):**
//! - ForensicVerificationService: request-driven verification of bundle feasibility
//! - ForensicArchiveGenerator: in-memory archive generation for export (scaffolded data)
//! - Verification types: ForensicVerificationRequest/Response with coverage estimates
//! - Export types: ForensicExportRequest/Response with in-memory archive generation
//!
//! **Batch 0 scope (already delivered):**
//! - ForensicBundle model and BundleContents scaffold
//!
//! **Batch 3 scope (NOT YET IMPLEMENTED):**
//! - Bundle generation (actual data collection from services)
//! - Bundle storage (S3 or any persistence)
//! - Bundle retrieval (downloading stored bundles)
//! - Bundle replay (reproducing state from a bundle)
//! - Hash chain integrity verification
//! - Async job orchestration for bundle generation
//! - Real service integration for actual data collection

pub mod bundle;
pub mod bundle_contents;
pub mod export;
pub mod verification;

pub use bundle::*;
pub use bundle_contents::*;
pub use export::*;
pub use verification::*;
