//! Intent Rebase Engine — Shared type definitions
//!
//! This crate contains core domain types, traits, and enums shared across
//! all IRE service crates.

pub mod artifact;
pub mod audit;
pub mod error;
pub mod graph;
pub mod intent;

pub use artifact::*;
pub use audit::*;
pub use error::*;
pub use graph::*;
pub use intent::*;
