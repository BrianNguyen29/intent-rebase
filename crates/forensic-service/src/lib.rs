//! Forensic Service — Phase 3 Batch 3b and P4 bounded slice
//!
//! This crate is responsible for forensic replay bundles
//! for incident investigation, compliance audits, and legal proceedings.
//!
//! **Phase 3 Batch 3b bounded slice (previous delivery):**
//! - ForensicVerificationService: request-driven verification of bundle feasibility
//! - ForensicArchiveGenerator: in-memory archive generation for export (scaffolded data)
//! - Verification types: ForensicVerificationRequest/Response with coverage estimates
//! - Export types: ForensicExportRequest/Response with in-memory archive generation
//! - BundleStatus tracking, repository trait, in-memory implementation
//! - Bundle generation service with status management, content collection primitives, integrity hashing
//!
//! **P4 bounded slice (this delivery):**
//! - ForensicBundleService: real data collection, bundle generation, and S3/MinIO persistence
//! - BundleStorage trait with in-memory (tests) and S3/MinIO (production) implementations
//! - POST /forensic/bundle endpoint wired in intent-api
//!
//! **Batch 0 scope (already delivered):**
//! - ForensicBundle model and BundleContents scaffold
//!
//! **Phase 4 scope (NOT YET IMPLEMENTED):**
//! - Bundle retrieval/download API (GET /forensic/bundle/{id}/download)
//! - Bundle replay (reproducing state from a stored bundle)
//! - Hash chain integrity verification
//! - Async job orchestration for large bundle generation
//! - S3 lifecycle rules (GLACIER/DEEP_ARCHIVE tiering, automatic expiry)

pub mod bundle;
pub mod bundle_contents;
pub mod bundle_gen;
pub mod bundle_generator;
pub mod bundle_hasher;
pub mod bundle_replay;
pub mod bundle_repo;
pub mod bundle_service;
pub mod bundle_storage;
pub mod collector;
pub mod export;
pub mod real_collector;
pub mod s3_bundle_storage;
pub mod verification;

pub use bundle::*;
pub use bundle_contents::*;
pub use bundle_gen::*;
pub use bundle_generator::*;
pub use bundle_hasher::*;
pub use bundle_replay::*;
pub use bundle_repo::*;
pub use bundle_service::*;
pub use bundle_storage::*;
pub use collector::*;
pub use export::*;
pub use real_collector::*;
pub use s3_bundle_storage::*;
pub use verification::*;

#[cfg(test)]
mod bundle_repo_tests;

#[cfg(test)]
mod bundle_replay_tests;

#[cfg(test)]
mod bundle_service_tests;
