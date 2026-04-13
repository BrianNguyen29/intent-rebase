//! Forensic Service — Phase 3 Batch 3b (P4 bounded slice)
//!
//! This crate is responsible for generating forensic replay bundles
//! for incident investigation, compliance audits, and legal proceedings.
//!
//! **This bounded slice scope (P4):** BundleStatus tracking, repository trait, in-memory implementation,
//! bundle generation service with status management, content collection primitives, integrity hashing,
//! and bounded replay verification.
//! **Batch 4 scope:** S3 storage, HTTP API, actual content collection from services, full replay runtime.

pub mod bundle;
pub mod bundle_contents;
pub mod bundle_gen;
pub mod bundle_hasher;
pub mod bundle_generator;
pub mod bundle_repo;
pub mod bundle_replay;

pub use bundle::*;
pub use bundle_contents::*;
pub use bundle_gen::*;
pub use bundle_hasher::*;
pub use bundle_generator::*;
pub use bundle_repo::*;
pub use bundle_replay::*;
