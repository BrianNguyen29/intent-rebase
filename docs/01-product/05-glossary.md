# Glossary

- **Intent**: biểu diễn có cấu trúc của mục tiêu, ràng buộc và tiêu chí hoàn thành.
- **Intent Version**: phiên bản của intent sau mỗi thay đổi đáng kể.
- **Semantic Diff**: khác biệt có ý nghĩa giữa hai intent versions.
- **Trace Edge**: liên kết giữa một clause/field của intent với artifact hoặc hành động thực thi.
- **Artifact**: bất kỳ đầu ra hoặc trạng thái trung gian nào của workflow: plan, patch, test, summary, approval, report.
- **Invalidation**: đánh dấu một artifact hoặc task không còn đáng tin dưới intent mới.
- **Review Required**: artifact chưa chắc sai, nhưng cần con người hoặc hệ rules xác minh lại.
- **Compensation**: hành động bù/undo/mitigate cho side effect đã xảy ra.
- **Repair Plan**: kế hoạch sửa cục bộ workflow sau khi intent đổi.
- **Rebase Plan**: kết quả cuối cùng mô tả cách chuyển execution từ intent cũ sang intent mới.
- **Side Effect**: hành động tác động ra ngoài hệ, ví dụ ghi DB, gửi mail, merge PR, gọi API thay đổi trạng thái.
- **Policy Snapshot**: ảnh chụp chính sách hiệu lực tại thời điểm một artifact hoặc action được tạo.
- **Checkpoint**: trạng thái bền vững cho phép resume workflow.

---

## Agent Safety Vocabulary (Formalized)

The following terms are used in the Agent Safety Rebase roadmap and related docs. Each term includes its source reference and current implementation status.

- **IntentVersion** — A versioned snapshot of an intent after a significant change.
  *Source refs:* `crates/intent-rebase-types/src/intent.rs`; `docs/03-spec/01-intent-model.md`
  *Status/scope:* Implemented (Phase 1).

- **RebasePlan** — The final plan describing how to transition execution from an old intent version to a new one.
  *Source refs:* `crates/rebase-engine/src/planner.rs`; `docs/03-spec/04-rebase-engine.md`
  *Status/scope:* Implemented (preview-only in Phase 1; apply path delivered in Phase 2b).

- **ImpactReport** — An on-demand read-only projection across pillars (intent, graph, audit, compensation) that summarizes the effect of a policy or intent change.
  *Source refs:* `docs/13-adrs/10-impact-report.md`; `docs/13-adrs/11-policy-snapshot-impact-report.md`
  *Status/scope:* Bounded MVP delivered (ADR-10 / ADR-11); no persistence for MVP.

- **SafetyGate** — A control point that blocks or allows rebase apply based on risk classification, approval state, and policy snapshot.
  *Source refs:* `docs/13-adrs/09-rebase-apply-rls-transaction-boundary.md`
  *Status/scope:* Bounded implemented (Phase 3); full enforcement pending RLS completion.

- **PropagationStatus** — The status of downstream signal propagation for an intent change, including webhook delivery and event streaming state.
  *Source refs:* `docs/10-delivery/19-propagation-status-implementation-plan.md`
  *Status/scope:* Slice 1 bounded MVP delivered locally; full downstream tracking deferred to Phase 4+.

- **PolicySnapshot** — A capture of the effective policy at the time an artifact or action was created.
  *Source refs:* `docs/14-governance/05-policy-snapshot-spec.md`
  *Status/scope:* Implemented (Phase 2b); S3 lifecycle enforcement deferred to Phase 4+.

- **CompensationAction** — An action taken to mitigate or undo a side effect, with types including Rollback, CounterAction, FollowupNotice, and Escalation.
  *Source refs:* `crates/compensation-service/src/lib.rs`; `docs/03-spec/05-compensation.md`
  *Status/scope:* Bounded implemented (four executors delivered in Phase 3 Batch 1).

- **ForensicBundle** — A collected set of intent versions, artifacts, audit events, and graph state packaged for integrity verification and replay.
  *Source refs:* `docs/10-delivery/17-production-readiness-backlog.md` (P4); `crates/forensic-service/src/lib.rs`
  *Status/scope:* Bounded generation/export/download delivered; full replay and S3 lifecycle Phase 4+.

- **IntentFamily** — A group of related intents that share lineage across multiple workflows or adapters.
  *Source refs:* `docs/10-delivery/18-agent-safety-rebase-roadmap.md` (Phase 4)
  *Status/scope:* Design-only; deferred to Phase 4+.
