# Dataflow

## Flow 1 — Intent creation
1. Source sends payload
2. Ingestion validates + normalizes
3. Intent version v1 is recorded
4. Event `intent.created`
5. Initial trace anchors are created

## Flow 2 — Intent change + rebase preview
1. Source changes spec/request
2. New intent version is created
3. Diff worker computes semantic diff
4. Impact engine queries graph
5. Rebase planner generates preview
6. Console / webhook notifies review

## Flow 3 — Rebase apply
1. Operator or rule engine approves
2. Runtime adapter pauses execution
3. Rebase plan is applied
4. Artifacts invalidated/quarantined
5. Compensation tasks inserted if needed
6. Workflow resumes from checkpoint
7. Event `workflow.rebased`

## Flow 4 — Audit export
1. Incident is selected
2. Replay service collects event timeline + artifacts + provenance
3. Export bundle is generated to object store
4. Operator downloads bundle

## Flow 5 — Approval stale detection
1. Policy update or intent change occurs
2. Approval evaluator runs rules
3. Old approval is marked stale
4. Side-effect step is blocked
