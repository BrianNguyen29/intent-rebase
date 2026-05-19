use crate::*;
use chrono::Utc;
use uuid::Uuid;

fn make_intent_entry(i: u8) -> IntentVersionEntry {
    IntentVersionEntry {
        intent_id: Uuid::from_bytes([i, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        version: 1,
        content_hash: format!("{:032x}", i as u64),
    }
}

fn make_artifact_entry(i: u8) -> ArtifactEntry {
    ArtifactEntry {
        artifact_id: Uuid::from_bytes([
            i.wrapping_add(0x10),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]),
        content_hash: format!("{:032x}", (i as u64) + 0x100),
        collected_at: Utc::now(),
    }
}

fn make_approval_entry(i: u8) -> ApprovalEntry {
    ApprovalEntry {
        approval_id: Uuid::from_bytes([
            i.wrapping_add(0x20),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]),
        content_hash: format!("{:032x}", (i as u64) + 0x200),
    }
}

fn make_audit_entry(i: u8) -> AuditEventEntry {
    AuditEventEntry {
        event_id: Uuid::from_bytes([
            i.wrapping_add(0x30),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]),
        content_hash: format!("{:032x}", (i as u64) + 0x300),
        event_index: i as usize,
    }
}

fn make_policy_entry(i: u8) -> PolicySnapshotEntry {
    PolicySnapshotEntry {
        snapshot_id: Uuid::from_bytes([
            i.wrapping_add(0x40),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]),
        scope_hash: format!("{:032x}", (i as u64) + 0x400),
    }
}

fn create_test_bundle(tenant_id: Uuid, purpose: BundlePurpose) -> ForensicBundle {
    ForensicBundle::new(
        tenant_id,
        BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose,
        BundleContents::default(),
        "test-user",
        None,
    )
}

#[test]
fn test_replay_section_result_success() {
    let result = ReplaySectionResult::success("test_section", 5, "abc123", "abc123");
    assert!(result.verified);
    assert_eq!(result.section, "test_section");
    assert_eq!(result.item_count, 5);
}

#[test]
fn test_replay_section_result_failure() {
    let result = ReplaySectionResult::failure("test_section", 5, "abc123", "xyz789");
    assert!(!result.verified);
    assert!(result.details.contains("FAILED"));
}

#[test]
fn test_generate_summary() {
    let service = BundleReplayService::new();
    let bundle = create_test_bundle(Uuid::new_v4(), BundlePurpose::IncidentInvestigation);

    let summary = service.generate_summary(&bundle);

    assert_eq!(summary.bundle_id, bundle.bundle_id);
    assert_eq!(summary.purpose, BundlePurpose::IncidentInvestigation);
    assert!(summary.summary.contains("intent versions"));
}

#[test]
fn test_verify_bundle_clean_content() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::ComplianceAudit);

    // Create content entries
    let intent_versions = vec![make_intent_entry(1), make_intent_entry(2)];
    let artifacts = vec![make_artifact_entry(1)];
    let approvals = vec![make_approval_entry(1)];
    let audit_events = vec![make_audit_entry(1), make_audit_entry(2)];
    let policy_snapshots = vec![make_policy_entry(1)];

    // Compute expected hashes using BundleGeneratorService
    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: bundle.time_range.clone(),
        purpose: bundle.purpose,
        created_by: bundle.created_by.clone(),
        intent_versions: intent_versions.clone(),
        artifacts: artifacts.clone(),
        approvals: approvals.clone(),
        audit_events: audit_events.clone(),
        policy_snapshots: policy_snapshots.clone(),
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let integrity_hash = gen_result.integrity_hash;

    let request = VerifyBundleReplayRequest {
        bundle,
        intent_versions,
        artifacts,
        approvals,
        audit_events,
        policy_snapshots,
    };

    let response = service
        .verify_bundle_with_integrity(
            request,
            integrity_hash.intent_versions_hash,
            integrity_hash.artifacts_hash,
            integrity_hash.approvals_hash,
            integrity_hash.audit_events_hash,
            integrity_hash.policy_snapshots_hash,
        )
        .expect("verification should succeed");

    assert!(response.report.overall_verified);
    assert_eq!(response.report.sections_passed, 5);
    assert_eq!(response.report.sections_failed, 0);
    assert!(response.report.summary.contains("verified successfully"));
}

#[test]
fn test_verify_bundle_tampered_content() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);

    // Create content entries
    let intent_versions = vec![make_intent_entry(1)];
    let artifacts = vec![];
    let approvals = vec![];
    let audit_events = vec![];
    let policy_snapshots = vec![];

    // Build a bundle with DIFFERENT content
    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: bundle.time_range.clone(),
        purpose: bundle.purpose,
        created_by: bundle.created_by.clone(),
        intent_versions: vec![make_intent_entry(99)], // Different content!
        artifacts: vec![],
        approvals: vec![],
        audit_events: vec![],
        policy_snapshots: vec![],
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let integrity_hash = gen_result.integrity_hash;

    // Verify with ORIGINAL content (not the tampered content)
    let request = VerifyBundleReplayRequest {
        bundle,
        intent_versions, // This doesn't match what was in the bundle
        artifacts,
        approvals,
        audit_events,
        policy_snapshots,
    };

    let response = service
        .verify_bundle_with_integrity(
            request,
            integrity_hash.intent_versions_hash,
            integrity_hash.artifacts_hash,
            integrity_hash.approvals_hash,
            integrity_hash.audit_events_hash,
            integrity_hash.policy_snapshots_hash,
        )
        .expect("verification should complete");

    // Verification should fail because content doesn't match
    assert!(!response.report.overall_verified);
    assert!(response.report.sections_failed > 0);
    assert!(response.report.summary.contains("FAILED"));
}

#[test]
fn test_verify_empty_bundle() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::Legal);

    // Generate empty bundle
    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: bundle.time_range.clone(),
        purpose: bundle.purpose,
        created_by: bundle.created_by.clone(),
        intent_versions: vec![],
        artifacts: vec![],
        approvals: vec![],
        audit_events: vec![],
        policy_snapshots: vec![],
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let integrity_hash = gen_result.integrity_hash;

    let request = VerifyBundleReplayRequest {
        bundle,
        intent_versions: vec![],
        artifacts: vec![],
        approvals: vec![],
        audit_events: vec![],
        policy_snapshots: vec![],
    };

    let response = service
        .verify_bundle_with_integrity(
            request,
            integrity_hash.intent_versions_hash,
            integrity_hash.artifacts_hash,
            integrity_hash.approvals_hash,
            integrity_hash.audit_events_hash,
            integrity_hash.policy_snapshots_hash,
        )
        .expect("verification should succeed");

    // Empty bundle with empty content should verify
    assert!(response.report.overall_verified);
}

#[test]
fn test_replay_error_display() {
    let error = BundleReplayError::BundleNotReady {
        bundle_id: Uuid::new_v4(),
        current_status: BundleStatus::Pending,
    };
    let msg = error.to_string();
    assert!(msg.contains("not ready for replay"));
}

#[test]
fn test_verify_bundle_all_sections_populated() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();

    // Create entries once
    let intent_versions = vec![
        make_intent_entry(1),
        make_intent_entry(2),
        make_intent_entry(3),
    ];
    let artifacts = vec![make_artifact_entry(1), make_artifact_entry(2)];
    let approvals = vec![
        make_approval_entry(1),
        make_approval_entry(2),
        make_approval_entry(3),
    ];
    let audit_events = vec![make_audit_entry(1), make_audit_entry(2)];
    let policy_snapshots = vec![make_policy_entry(1), make_policy_entry(2)];

    // Create a full bundle with all section types populated
    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose: BundlePurpose::Legal,
        created_by: "tester".to_string(),
        intent_versions: intent_versions.clone(),
        artifacts: artifacts.clone(),
        approvals: approvals.clone(),
        audit_events: audit_events.clone(),
        policy_snapshots: policy_snapshots.clone(),
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let integrity_hash = gen_result.integrity_hash;
    let bundle = gen_result.bundle;

    // Verify with same content (using same variables)
    let request = VerifyBundleReplayRequest {
        bundle,
        intent_versions: intent_versions.clone(),
        artifacts: artifacts.clone(),
        approvals: approvals.clone(),
        audit_events: audit_events.clone(),
        policy_snapshots: policy_snapshots.clone(),
    };

    let response = service
        .verify_bundle_with_integrity(
            request,
            integrity_hash.intent_versions_hash,
            integrity_hash.artifacts_hash,
            integrity_hash.approvals_hash,
            integrity_hash.audit_events_hash,
            integrity_hash.policy_snapshots_hash,
        )
        .expect("verification should succeed");

    assert!(response.report.overall_verified);
    assert_eq!(response.report.sections.len(), 5);
    assert_eq!(response.report.total_items.intent_versions, 3);
    assert_eq!(response.report.total_items.artifacts, 2);
    assert_eq!(response.report.total_items.approvals, 3);
    assert_eq!(response.report.total_items.audit_events, 2);
    assert_eq!(response.report.total_items.policy_snapshots, 2);
}

#[test]
fn test_summary_contains_all_content_counts() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();

    let intent_versions = vec![make_intent_entry(1)];
    let artifacts = vec![make_artifact_entry(1), make_artifact_entry(2)];
    let audit_events = vec![
        make_audit_entry(1),
        make_audit_entry(2),
        make_audit_entry(3),
    ];

    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "analyst".to_string(),
        intent_versions,
        artifacts,
        approvals: vec![],
        audit_events,
        policy_snapshots: vec![],
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let summary = service.generate_summary(&gen_result.bundle);

    assert!(summary.summary.contains("1 intent versions"));
    assert!(summary.summary.contains("2 artifacts"));
    assert!(summary.summary.contains("3 audit events"));
}

#[test]
fn test_section_hashes_returned_in_response() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();

    let intent_versions = vec![make_intent_entry(1)];

    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose: BundlePurpose::ComplianceAudit,
        created_by: "auditor".to_string(),
        intent_versions: intent_versions.clone(),
        artifacts: vec![],
        approvals: vec![],
        audit_events: vec![],
        policy_snapshots: vec![],
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let integrity_hash = gen_result.integrity_hash;
    let bundle = gen_result.bundle;

    let request = VerifyBundleReplayRequest {
        bundle,
        intent_versions,
        artifacts: vec![],
        approvals: vec![],
        audit_events: vec![],
        policy_snapshots: vec![],
    };

    let response = service
        .verify_bundle_with_integrity(
            request,
            integrity_hash.intent_versions_hash,
            integrity_hash.artifacts_hash,
            integrity_hash.approvals_hash,
            integrity_hash.audit_events_hash,
            integrity_hash.policy_snapshots_hash,
        )
        .expect("verification should succeed");

    // All 5 section hashes should be returned
    assert_eq!(response.section_hashes.len(), 5);

    // Each section hash should have non-empty content_hash
    for section_hash in &response.section_hashes {
        assert!(!section_hash.content_hash.is_empty());
        assert!(!section_hash.section.is_empty());
    }
}

#[test]
fn test_verify_bundle_from_integrity_success() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();

    let intent_versions = vec![make_intent_entry(1), make_intent_entry(2)];
    let artifacts = vec![make_artifact_entry(1)];
    let approvals = vec![make_approval_entry(1)];
    let audit_events = vec![make_audit_entry(1)];
    let policy_snapshots = vec![make_policy_entry(1)];

    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose: BundlePurpose::ComplianceAudit,
        created_by: "tester".to_string(),
        intent_versions: intent_versions.clone(),
        artifacts: artifacts.clone(),
        approvals: approvals.clone(),
        audit_events: audit_events.clone(),
        policy_snapshots: policy_snapshots.clone(),
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let mut bundle = gen_result.bundle;
    bundle.status = BundleStatus::Ready;

    let content_sections = crate::bundle_hasher::ContentSectionsForVerification {
        intent_versions: crate::bundle_hasher::IntentVersionsForHash {
            versions: intent_versions,
        },
        artifacts: crate::bundle_hasher::ArtifactsForHash { artifacts },
        approvals: crate::bundle_hasher::ApprovalsForHash { approvals },
        audit_events: crate::bundle_hasher::AuditEventsForHash {
            events: audit_events,
        },
        policy_snapshots: crate::bundle_hasher::PolicySnapshotsForHash {
            snapshots: policy_snapshots,
        },
    };

    let response = service
        .verify_bundle_from_integrity(&bundle, &content_sections)
        .expect("verification should succeed");

    assert!(response.report.overall_verified);
    assert_eq!(response.report.sections_passed, 5);
    assert_eq!(response.report.sections_failed, 0);
    assert!(response.report.summary.contains("verified successfully"));
}

#[test]
fn test_verify_bundle_from_integrity_tampered_content() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();

    let intent_versions = vec![make_intent_entry(1)];
    let artifacts = vec![];
    let approvals = vec![];
    let audit_events = vec![];
    let policy_snapshots = vec![];

    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "tester".to_string(),
        intent_versions: intent_versions.clone(),
        artifacts: artifacts.clone(),
        approvals: approvals.clone(),
        audit_events: audit_events.clone(),
        policy_snapshots: policy_snapshots.clone(),
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let mut bundle = gen_result.bundle;
    bundle.status = BundleStatus::Ready;

    // Tamper with the content
    let mut tampered_intent_versions = intent_versions.clone();
    tampered_intent_versions[0].content_hash = "tampered".to_string();

    let content_sections = crate::bundle_hasher::ContentSectionsForVerification {
        intent_versions: crate::bundle_hasher::IntentVersionsForHash {
            versions: tampered_intent_versions,
        },
        artifacts: crate::bundle_hasher::ArtifactsForHash { artifacts },
        approvals: crate::bundle_hasher::ApprovalsForHash { approvals },
        audit_events: crate::bundle_hasher::AuditEventsForHash {
            events: audit_events,
        },
        policy_snapshots: crate::bundle_hasher::PolicySnapshotsForHash {
            snapshots: policy_snapshots,
        },
    };

    let response = service
        .verify_bundle_from_integrity(&bundle, &content_sections)
        .expect("verification should complete");

    assert!(!response.report.overall_verified);
    assert_eq!(response.report.sections_failed, 1);
    assert!(response.report.summary.contains("FAILED"));
}

#[test]
fn test_verify_bundle_from_integrity_not_ready() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::Legal);
    // status is Pending, not Ready

    let content_sections = crate::bundle_hasher::ContentSectionsForVerification::default();

    let result = service.verify_bundle_from_integrity(&bundle, &content_sections);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("not ready for replay"));
}

#[test]
fn test_verify_manifest_integrity() {
    let service = BundleReplayService::new();
    let tenant_id = Uuid::new_v4();

    let gen_request = crate::bundle_generator::GenerateBundleRequest {
        tenant_id,
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose: BundlePurpose::Legal,
        created_by: "tester".to_string(),
        intent_versions: vec![make_intent_entry(1)],
        artifacts: vec![],
        approvals: vec![],
        audit_events: vec![],
        policy_snapshots: vec![],
    };

    let gen_result = crate::bundle_generator::BundleGeneratorService::generate(gen_request);
    let bundle = gen_result.bundle;

    // Manifest integrity should pass for a freshly generated bundle
    assert!(service.verify_manifest_integrity(&bundle));

    // Tamper with the bundle and verify it fails
    let mut tampered_bundle = bundle.clone();
    tampered_bundle.created_by = "attacker".to_string();
    assert!(!service.verify_manifest_integrity(&tampered_bundle));
}
