# Agent Implementation Guide

## Mục tiêu
Cho AI agents một bản đồ rõ ràng để triển khai mà không mơ hồ.

## Quy tắc làm việc cho agents
1. Không sửa intent schema nếu chưa cập nhật ADR.
2. Mọi thay đổi API phải cập nhật OpenAPI và event contract.
3. Mọi thay đổi graph rule phải đi kèm tests.
4. Mọi thay đổi risky ở apply path phải có replay tests.
5. Không triển khai side effect auto-compensation cho S3/S4 nếu chưa có explicit approval.

## Workstreams đề xuất

### Stream A — Core Data and APIs
- schema migrations
- intent CRUD
- diff endpoints
- rebase endpoints

### Stream B — Control Logic
- semantic diff rules
- graph propagation engine
- rebase planner

### Stream C — Runtime Integration
- adapter capability contract
- primary adapter
- checkpoint mapping
- apply pipeline

### Stream D — Console
- intent detail
- rebase preview
- workflow timeline
- approvals/stale indicators

### Stream E — Security and Audit
- authz
- audit append
- export
- permissions matrix

## Definition of done cho mỗi task
- code
- tests
- docs
- metrics/logging
- security notes
- rollback note
