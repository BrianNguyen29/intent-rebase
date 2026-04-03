# Webhooks

## Mục tiêu
Cho phép tích hợp IRE với:
- Git providers
- ticketing systems
- internal workflow engines
- policy engines
- approval tools
- chatops systems

## Outbound webhook events
- `rebase.plan_created`
- `rebase.manual_review_required`
- `approval.stale_detected`
- `workflow.restart_required`
- `compensation.manual_required`
- `audit.export_ready`

## Webhook payload requirements
- signed secret
- delivery id
- event id
- retries with exponential backoff
- replay endpoint support

## Inbound webhook sources
- spec file changed
- issue updated
- policy updated
- approval revoked
- deployment freeze triggered

## Safety requirements
- verify signatures
- dedupe by delivery id
- source trust tier
- origin allowlist
