# Core Principles

## 1. Intent-first, not prompt-first
Prompt is a means of conveying command; intent is the object that needs long-term management.

## 2. Repair before restart
Only restart the entire workflow when repair or compensation is unsafe / infeasible.

## 3. Explicit provenance
Every artifact must know what it is based on:
- which intent version
- which inputs
- which policy snapshot
- which agent/runtime

## 4. Explainable invalidation
When marking an artifact as invalid or review-required, the system must give a clear reason.

## 5. Side-effect awareness
Read/write/approval/external call are behaviors with different risk levels; rebase cannot look at text diff alone.

## 6. Human override by design
The operator must always have a path to:
- approve a repair plan
- force restart
- force manual handoff
- suppress low-risk invalidations
- quarantine a risky branch

## 7. Event-sourced control history
Every important change must be recorded as events for replay and forensic analysis.

## 8. Multi-tenant and policy-safe
Do not let tenant A see the graph or internal artifacts of tenant B.
