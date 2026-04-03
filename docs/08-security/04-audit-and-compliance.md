# Audit and Compliance

## Audit requirements
Ghi lại:
- ai tạo intent/change
- diff nào được tính
- rebase plan nào được tạo
- ai approve/reject/apply
- side effect/compensation nào xảy ra
- policy snapshot nào có hiệu lực

## Audit event properties
- immutable id
- tenant scoped
- actor identity
- resource refs
- before/after states when appropriate
- rationale
- trace id

## Compliance readiness targets
Tùy thị trường:
- SOC 2 controls mapping
- ISO 27001 operational controls
- internal change management evidence
- customer audit export support

## Tamper resistance
- append-only write path
- periodic digesting/hashing
- optional external audit sink
