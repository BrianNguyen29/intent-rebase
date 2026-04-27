//! Tenant domain model
//!
//! See [../../../../docs/14-governance/08-tenant-isolation.md] for full specification
//! and [../../../../docs/10-delivery/checklists/checklist-phase-3.md] for P3-S5 bounded slice scope.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tenant status within the platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    /// Tenant is being provisioned but not yet active
    #[default]
    Provisioning,
    /// Tenant is active and can perform operations
    Active,
    /// Tenant is suspended (can be reactivated)
    Suspended,
    /// Tenant is being offboarded (data deletion in progress)
    Offboarding,
    /// Tenant has been offboarded (archived, read-only billing records remain)
    Offboarded,
}

/// Target data residency region for a tenant
///
/// **P3-S5 scope:** Region field is recorded but residency enforcement/routing
/// is out of scope for this bounded slice.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantRegion {
    #[default]
    UsEast1,
    UsWest2,
    EuWest1,
    ApSoutheast1,
}

/// A tenant record within the Intent Rebase Engine platform.
///
/// **P3-S5 scope:** Minimal persistent fields and construction helpers.
/// Residency routing, quota enforcement, and offboarding deletion orchestration
/// are future scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    /// Unique identifier for this tenant
    pub id: Uuid,
    /// Human-readable tenant name
    pub name: String,
    /// Slug identifier used in API paths and S3 bucket prefixes
    pub slug: String,
    /// Current tenant status
    pub status: TenantStatus,
    /// Target data residency region
    pub region: TenantRegion,
    /// When this tenant record was created
    pub created_at: DateTime<Utc>,
    /// When this tenant was last updated
    pub updated_at: DateTime<Utc>,
}

impl Tenant {
    /// Create a new tenant in Provisioning status.
    ///
    /// **P3-S5 scope:** New tenant starts in Provisioning. Activation and subsequent
    /// lifecycle steps are future scope.
    pub fn new(name: String, slug: String, region: TenantRegion) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            slug,
            status: TenantStatus::Provisioning,
            region,
            created_at: now,
            updated_at: now,
        }
    }

    /// Returns true if this tenant is active and can perform operations.
    pub fn is_active(&self) -> bool {
        self.status == TenantStatus::Active
    }

    /// Returns true if this tenant is in a state that allows read operations.
    ///
    /// **P3-S5 scope:** Active and Suspended tenants allow reads. Offboarding and
    /// Offboarded tenants do not.
    pub fn allows_read(&self) -> bool {
        matches!(self.status, TenantStatus::Active | TenantStatus::Suspended)
    }

    /// Returns true if this tenant is in a state that allows write operations.
    ///
    /// **P3-S5 scope:** Only Active tenants allow writes. Provisioning, Suspended,
    /// Offboarding, and Offboarded do not.
    pub fn allows_write(&self) -> bool {
        self.status == TenantStatus::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_new_provisioning() {
        let tenant = Tenant::new(
            "Acme Corp".to_string(),
            "acme-corp".to_string(),
            TenantRegion::UsEast1,
        );

        assert_eq!(tenant.name, "Acme Corp");
        assert_eq!(tenant.slug, "acme-corp");
        assert_eq!(tenant.status, TenantStatus::Provisioning);
        assert_eq!(tenant.region, TenantRegion::UsEast1);
    }

    #[test]
    fn test_tenant_is_active() {
        let mut tenant = Tenant::new(
            "Acme Corp".to_string(),
            "acme-corp".to_string(),
            TenantRegion::UsEast1,
        );

        // Provisioning: not active, not readable, not writable
        assert!(!tenant.is_active());
        assert!(!tenant.allows_read());
        assert!(!tenant.allows_write());

        tenant.status = TenantStatus::Active;
        assert!(tenant.is_active());
        assert!(tenant.allows_read());
        assert!(tenant.allows_write());
    }

    #[test]
    fn test_tenant_suspended_allows_read_not_write() {
        let mut tenant = Tenant::new(
            "Acme Corp".to_string(),
            "acme-corp".to_string(),
            TenantRegion::UsEast1,
        );
        tenant.status = TenantStatus::Suspended;

        assert!(!tenant.is_active());
        assert!(tenant.allows_read());
        assert!(!tenant.allows_write());
    }

    #[test]
    fn test_tenant_offboarding_no_operations() {
        let mut tenant = Tenant::new(
            "Acme Corp".to_string(),
            "acme-corp".to_string(),
            TenantRegion::UsEast1,
        );
        tenant.status = TenantStatus::Offboarding;

        assert!(!tenant.is_active());
        assert!(!tenant.allows_read());
        assert!(!tenant.allows_write());
    }

    #[test]
    fn test_tenant_serialization_round_trip() {
        let tenant = Tenant::new(
            "Acme Corp".to_string(),
            "acme-corp".to_string(),
            TenantRegion::EuWest1,
        );

        let json = serde_json::to_string(&tenant).unwrap();
        let deserialized: Tenant = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, tenant.id);
        assert_eq!(deserialized.name, tenant.name);
        assert_eq!(deserialized.slug, tenant.slug);
        assert_eq!(deserialized.status, tenant.status);
        assert_eq!(deserialized.region, tenant.region);
    }
}
