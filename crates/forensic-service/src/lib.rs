//! Forensic Service — Phase 3 Batch 0 scaffold
//!
//! This crate is responsible for generating forensic replay bundles
//! for incident investigation, compliance audits, and legal proceedings.
//!
//! **Batch 0 scope (this slice):** type/model scaffolding and minimal module structure only.
//! **Batch 3 scope:** bundle generation, integrity verification, replay, retention (not yet implemented).

pub mod bundle;
pub mod bundle_contents;

pub use bundle::*;
pub use bundle_contents::*;
