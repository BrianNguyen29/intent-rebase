//! Forensic Service — Phase 3 Batch 3b (P4 bounded slice)
//!
//! This crate is responsible for generating forensic replay bundles
//! for incident investigation, compliance audits, and legal proceedings.
//!
//! **This bounded slice scope (P4):** BundleStatus tracking, repository trait, in-memory implementation,
//! bundle generation service with status management.
//! **Batch 4 scope:** S3 storage, HTTP API, actual content collection, integrity verification, replay.

pub mod bundle;
pub mod bundle_contents;
pub mod bundle_gen;
pub mod bundle_repo;

pub use bundle::*;
pub use bundle_contents::*;
pub use bundle_gen::*;
pub use bundle_repo::*;
