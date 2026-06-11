# Rationale and External Patterns

Intent Rebase Engine is built on patterns proven in adjacent domains, then recomposed for agent systems.

## 1. Workflow versioning and replay compatibility
Temporal requires workflow code to be deterministic on replay, and provides versioning so old executions continue with old logic while new executions use new logic. This directly suggests that **changes in a running system need a version-aware compatibility layer**, rather than deploying new logic and hoping everything lines up.
Source:
- https://docs.temporal.io/develop/go/versioning
- https://docs.temporal.io/develop/safe-deployments

## 2. Durable execution, checkpoints, interrupts
LangGraph emphasizes persistence, checkpoints, durable execution, and interrupts for human-in-the-loop. This shows that to rebase mid-flight, there must be a substrate that stores durable state and resumes safely.
Source:
- https://docs.langchain.com/oss/python/langgraph/interrupts
- https://docs.langchain.com/oss/python/langgraph/persistence
- https://docs.langchain.com/oss/python/langgraph/durable-execution

## 3. Long-running agent harnesses
Anthropic has repeatedly emphasized harnesses, context engineering, structured artifacts, planner/generator/evaluator, and the need for long-running agents to split work into small pieces and hand off structured context. This reinforces the need for an intent-change management layer at runtime.
Source:
- https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents
- https://www.anthropic.com/engineering/harness-design-long-running-apps
- https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents

## 4. Spec-driven development
GitHub Spec Kit and related blog posts treat the spec as a shared source of truth and standardize the spec -> plan -> tasks chain. Intent Rebase Engine extends that logic at runtime: when the source of truth changes, execution must be systematically rebased.
Source:
- https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/
- https://github.blog/developer-skills/application-development/context-windows-plan-agent-and-tdd-what-i-learned-building-a-countdown-app-with-github-copilot/
- https://github.blog/ai-and-ml/github-copilot/how-to-build-reliable-ai-workflows-with-agentic-primitives-and-context-engineering/

## 5. Change impact analysis and requirements traceability
Software engineering has long studied impact analysis: a requirement change pulls along artifacts that must be updated. IRE applies this thinking to agent workflows.
Source:
- https://orbilu.uni.lu/bitstream/10993/12555/1/Goknil_paper.pdf
- https://arxiv.org/pdf/1608.02757

## 6. Plan repair vs replanning
Planning literature distinguishes plan repair from replanning: when possible, patching the plan locally is better than discarding everything and starting over. This is the default principle of IRE.
Source:
- https://gki.informatik.uni-freiburg.de/papers/hoeller-etal-hplan18.pdf

## 7. Event sourcing and compensation
Event sourcing allows timeline reconstruction and retrospective handling; saga/compensation handles side effects through distributed step chains.
Source:
- https://martinfowler.com/eaaDev/EventSourcing.html
- https://microservices.io/patterns/data/saga.html
