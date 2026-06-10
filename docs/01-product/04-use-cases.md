# Primary Use Cases

## UC1. Coding Agent Rebase
### Scenario
An agent is refactoring an auth module. Mid-flight, the user changes requirements:
- keep backward compatibility
- do not modify public API
- must increase test coverage

### Desired Behavior
- keep analysis of the current codebase
- invalidate patches related to public API
- mark old test plan as incomplete
- spawn additional task for compatibility tests
- revoke old approval if approval scope changed

## UC2. Support Workflow Rebase
### Scenario
An agent is preparing a customer response based on an old policy. The policy team just changed escalation criteria.

### Desired Behavior
- draft response is marked review-required
- email send step is blocked
- approval path is updated
- operator sees clear policy diff -> impact

## UC3. Research Workflow Rebase
### Scenario
A lead agent is coordinating 5 sub-agents researching vendor options. Later, budget is cut and security requirements increase.

### Desired Behavior
- discovery summaries remain usable
- ranking/recommendation outputs are marked invalid
- re-run vendor scoring under new constraints
- external RFQ steps not yet sent are cancelled

## UC4. Internal Ops / DevOps Rebase
### Scenario
An agent has planned a canary rollout. Later, SRE changes error budget policy and freeze window.

### Desired Behavior
- plan is rebased, not auto-deployed
- change request needs new approval
- rollback strategy is added as mandatory
