# UX Flows

## Flow A — Operator reviews rebase preview
1. Nhận notification
2. Mở rebase plan
3. Xem semantic diff
4. Xem impact graph
5. Xem affected artifacts + approvals
6. Chọn:
   - apply
   - edit repair policy
   - escalate
   - force restart

## Flow B — Approval stale during execution
1. Banner hiển thị stale approval
2. Side effect step bị block
3. Operator xem lý do stale
4. Yêu cầu revalidation hoặc alternative route

## Flow C — Incident forensic
1. Chọn workflow
2. Mở timeline
3. Filter theo intent version / side effect / actor
4. Export forensic bundle

## UX principles
- show reasons, not only statuses
- prefer diff + rationale + next action
- separate low-risk from high-risk changes visually
- avoid hidden auto-decisions
