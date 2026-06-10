# Goals and Non-Goals

## Goals

### G1. Standardize intent into a versioned object
Intent is not just a chat transcript. IRE must support:
- objective
- constraints
- acceptance criteria
- trust / approval boundaries
- budget / latency / cost caps
- prohibited actions
- preferred trade-offs
- external references

### G2. Meaningful semantic diff
The system must recognize different kinds of changes such as:
- adding detail
- narrowing or expanding scope
- constraint changes
- quality/definition of done changes
- authority scope changes
- budget or urgency changes
- risk appetite changes

### G3. Build a dependency graph between intent and execution artifacts
The graph must connect:
- intent clauses
- plans / tasks
- agent runs
- tool invocations
- outputs
- tests
- approvals
- side effects
- memory items

### G4. Rebase instead of restart by default
The system must prioritize:
- salvage what is still correct
- invalidate selectively
- request review/approval again when needed
- compensation for side effects
- resume from a valid checkpoint

### G5. Full audit and replay
Must be able to answer:
- what change occurred
- who/what created the change
- which artifact was affected
- why the system chose repair/restart/compensate
- which output was produced under which intent version

### G6. Production readiness
The system must have:
- clear authn/authz
- audit logs
- multi-tenant isolation
- SLA / SLO
- observability
- runbooks
- backpressure / retry / idempotency

## Non-Goals

### NG1. Not an LLM model platform
IRE does not train its own model and does not replace the inference provider.

### NG2. Not a workflow engine replacement
IRE should integrate with Temporal/LangGraph/custom runtime rather than reinvent durable execution from scratch.

### NG3. Not the sole source-of-truth for business specs
IRE consumes specs from multiple sources and standardizes them into Intent Objects; the original spec may still live in Git, issue tracker, ticketing, or docs.

### NG4. Not automatic undo of every side effect
Many real-world side effects cannot be perfectly reversed; the system must classify and escalate.

### NG5. Not tied exclusively to a single agent system
The design must support multiple runtimes/protocols via adapters.
