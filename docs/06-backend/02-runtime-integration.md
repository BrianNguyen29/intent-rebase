# Runtime Integration

## Goal
IRE does not replace the workflow runtime. It needs standard adapters to:
- read execution state
- read/write checkpoints
- pause
- resume
- cancel
- branch
- inject tasks/approvals

## Adapter capability contract
Each adapter must declare:
- supports_pause
- supports_resume
- supports_branch
- supports_checkpoint_lookup
- supports_task_injection
- supports_side_effect_intercepts
- consistency_guarantees
- max_resume_delay

## Temporal adapter
Recommended for production v1 if you need:
- durable execution
- workflow histories
- versioning
- replay testing
- signals/queries

Use cases:
- long-running coding flows
- approval-aware workflows
- compensation orchestration

## LangGraph adapter
Suitable if:
- agent harness already uses LangGraph
- needs interrupts, persistence, HITL
- strong graph-centric logic

## Custom adapter
Must meet the minimum requirements:
- workflow execution identity
- clear checkpoint semantics
- action preflight hook
- intent version propagation

## Required runtime hooks
- on_intent_change_detected
- on_rebase_plan_preview
- before_side_effect_dispatch
- on_approval_stale
- on_resume
- on_compensation_needed
