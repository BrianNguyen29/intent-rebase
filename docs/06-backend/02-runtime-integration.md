# Runtime Integration

## Mục tiêu
IRE không thay workflow runtime. Nó cần adapters chuẩn để:
- read execution state
- read/write checkpoints
- pause
- resume
- cancel
- branch
- inject tasks/approvals

## Adapter capability contract
Mỗi adapter phải khai báo:
- supports_pause
- supports_resume
- supports_branch
- supports_checkpoint_lookup
- supports_task_injection
- supports_side_effect_intercepts
- consistency_guarantees
- max_resume_delay

## Temporal adapter
Khuyến nghị cho production v1 nếu cần:
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
Phù hợp nếu:
- agent harness đã dùng LangGraph
- cần interrupts, persistence, HITL
- logic graph-centric mạnh

## Custom adapter
Phải đáp ứng tối thiểu:
- workflow execution identity
- checkpoint semantics rõ ràng
- action preflight hook
- intent version propagation

## Required runtime hooks
- on_intent_change_detected
- on_rebase_plan_preview
- before_side_effect_dispatch
- on_approval_stale
- on_resume
- on_compensation_needed
