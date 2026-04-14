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
//! - BundleStatus tracking, repository trait, in-memory implementation
//! - Bundle generation service with status management, content collection primitives, integrity hashing
//!
//! **Batch 0 scope (already delivered):**
//! - ForensicBundle model and BundleContents scaffold
//!
//! **Batch 3/4 scope (NOT YET IMPLEMENTED):**
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
pub mod bundle_gen;
pub mod bundle_hasher;
pub mod bundle_generator;
pub mod bundle_repo;
pub mod bundle_replay;

pub use bundle::*;
pub use bundle_contents::*;
pub use export::*;
pub use verification::*;
pub use bundle_gen::*;
pub use bundle_hasher::*;
pub use bundle_generator::*;
pub use bundle_repo::*;
pub use bundle_replay::*;
