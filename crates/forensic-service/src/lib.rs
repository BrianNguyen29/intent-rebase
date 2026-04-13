//! Forensic Service — Phase 3 Batch 3b bounded slice
//!
//! This crate is responsible for forensic replay bundles
//! for incident investigation, compliance audits, and legal proceedings.
//!
//! **Phase 3 Batch 3b bounded slice (this delivery):**
//! - ForensicVerificationService: request-driven verification of bundle feasibility
//! - Verification types: ForensicVerificationRequest/Response with coverage estimates
//!
//! **Batch 0 scope (already delivered):**
//! - ForensicBundle model and BundleContents scaffold
//!
//! **Batch 3 scope (NOT YET IMPLEMENTED):**
//! - Bundle generation (actual data collection)
//! - Bundle storage (S3 or any persistence)
//! - Bundle retrieval (downloading stored bundles)
//! - Bundle replay (reproducing state from a bundle)
//! - Hash chain integrity verification

pub mod bundle;
pub mod bundle_contents;
pub mod verification;

pub use bundle::*;
pub use bundle_contents::*;
pub use verification::*;
