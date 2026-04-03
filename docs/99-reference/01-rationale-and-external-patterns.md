# Rationale and External Patterns

Intent Rebase Engine được xây trên các pattern đã được chứng minh trong các miền gần kề, rồi tái tổ hợp lại cho agent systems.

## 1. Workflow versioning and replay compatibility
Temporal yêu cầu workflow code phải deterministic khi replay, và cung cấp versioning để các execution cũ tiếp tục chạy logic cũ trong khi execution mới dùng logic mới. Điều này gợi ý trực tiếp rằng **thay đổi ở hệ đang chạy cần có version-aware compatibility layer**, thay vì deploy logic mới rồi hy vọng mọi thứ tự đúng.
Source:
- https://docs.temporal.io/develop/go/versioning
- https://docs.temporal.io/develop/safe-deployments

## 2. Durable execution, checkpoints, interrupts
LangGraph nhấn mạnh persistence, checkpoints, durable execution và interrupts cho human-in-the-loop. Điều này cho thấy muốn rebase giữa chừng thì phải có substrate lưu trạng thái bền vững và resume an toàn.
Source:
- https://docs.langchain.com/oss/python/langgraph/interrupts
- https://docs.langchain.com/oss/python/langgraph/persistence
- https://docs.langchain.com/oss/python/langgraph/durable-execution

## 3. Long-running agent harnesses
Anthropic nhiều lần nhấn mạnh harness, context engineering, structured artifacts, planner/generator/evaluator, và việc long-running agents phải chia nhỏ công việc, handoff ngữ cảnh có cấu trúc. Điều này củng cố nhu cầu về một lớp quản lý thay đổi intent ở runtime.
Source:
- https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents
- https://www.anthropic.com/engineering/harness-design-long-running-apps
- https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents

## 4. Spec-driven development
GitHub Spec Kit và các bài blog liên quan xem spec là shared source of truth và chuẩn hóa chuỗi spec -> plan -> tasks. Intent Rebase Engine tiếp tục logic đó ở runtime: khi source of truth đổi, execution phải được rebase có hệ thống.
Source:
- https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/
- https://github.blog/developer-skills/application-development/context-windows-plan-agent-and-tdd-what-i-learned-building-a-countdown-app-with-github-copilot/
- https://github.blog/ai-and-ml/github-copilot/how-to-build-reliable-ai-workflows-with-agentic-primitives-and-context-engineering/

## 5. Change impact analysis and requirements traceability
Software engineering từ lâu đã nghiên cứu impact analysis: thay đổi requirement kéo theo artifact nào phải sửa. IRE áp dụng tư duy này vào agent workflows.
Source:
- https://orbilu.uni.lu/bitstream/10993/12555/1/Goknil_paper.pdf
- https://arxiv.org/pdf/1608.02757

## 6. Plan repair vs replanning
Literature về planning phân biệt plan repair với replanning: nếu có thể, sửa cục bộ plan sẽ tốt hơn bỏ hết và làm lại. Đây là nguyên lý mặc định của IRE.
Source:
- https://gki.informatik.uni-freiburg.de/papers/hoeller-etal-hplan18.pdf

## 7. Event sourcing and compensation
Event sourcing cho phép reconstruct timeline và xử lý hồi tố; saga/compensation giúp xử lý side effects qua chuỗi bước phân tán.
Source:
- https://martinfowler.com/eaaDev/EventSourcing.html
- https://microservices.io/patterns/data/saga.html
