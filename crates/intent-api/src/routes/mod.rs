//! Domain-grouped route registration modules.
//!
//! Each submodule provides an `add_routes` helper that accepts an axum `Router`
//! and returns it with the relevant routes registered. This keeps `router.rs`
//! focused on high-level router construction while preserving exact route
//! ordering and behaviour.

pub mod approval;
pub mod compensation;
pub mod forensic;
pub mod graph;
pub mod health;
pub mod intent;
pub mod policy;
pub mod propagation;
pub mod webhook;
