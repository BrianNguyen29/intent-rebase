# UX Flows

## Flow A — Operator reviews rebase preview
1. Receive notification
2. Open rebase plan
3. View semantic diff
4. View impact graph
5. View affected artifacts + approvals
6. Choose:
   - apply
   - edit repair policy
   - escalate
   - force restart

## Flow B — Approval stale during execution
1. Banner shows stale approval
2. Side effect step is blocked
3. Operator views stale reason
4. Request revalidation or alternative route

## Flow C — Incident forensic
1. Select workflow
2. Open timeline
3. Filter by intent version / side effect / actor
4. Export forensic bundle

## UX principles
- show reasons, not only statuses
- prefer diff + rationale + next action
- separate low-risk from high-risk changes visually
- avoid hidden auto-decisions
