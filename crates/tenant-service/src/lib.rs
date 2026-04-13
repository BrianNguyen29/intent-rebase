//! Tenant Service — Phase 3 P3-S5 tenant onboarding scaffold
//!
//! This crate is responsible for tenant lifecycle management including
//! onboarding procedures and tenant record storage.
//!
//! **P3-S5 scope (this slice):** Tenant model + repository trait + in-memory implementation.
//! **Future scope:** SQL persistence, residency enforcement, offboarding deletion orchestration,
//!   public API endpoints for tenant management.

pub mod tenant;
pub mod tenant_repo;

pub use tenant::*;
pub use tenant_repo::{InMemoryTenantRepository, TenantRepository};
