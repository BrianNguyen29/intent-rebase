# Agent Contracts and Prompting Contracts

## Agent contract style
Khi dùng coding agents để triển khai, mỗi task nên có:
- objective
- constraints
- inputs
- outputs
- non-goals
- acceptance tests
- files allowed to change
- docs to update

## Example task contract
```md
Objective:
Implement `/v1/intents/{intent_id}/versions`.

Constraints:
- Must preserve immutability of prior versions
- Must enforce tenant scoping
- Must use optimistic concurrency

Inputs:
- 03-spec/01-intent-model.md
- 04-api/01-rest-api.md
- 05-data/01-schema.md

Outputs:
- migration(s)
- handler/service code
- tests
- docs update

Non-goals:
- no UI changes
- no diff computation

Acceptance:
- create version works
- parent_version_id set
- current head updated atomically
```

## Review contract
Mọi PR do agent tạo phải có:
- summary
- assumptions
- changed files
- tests added
- open risks
- rollback note
