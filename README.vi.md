# Intent Rebase Engine

> **Khi intent thay đổi, hãy rebase phần việc đang chạy — thay vì để agent tiếp tục đi trên một lời hứa đã lỗi thời.**

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust: stable](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Docs](https://img.shields.io/badge/Docs-docs%2FREADME.md-blueviolet.svg)](docs/README.md)
[![Status: not production-ready](https://img.shields.io/badge/Status-not%20production--ready-critical.svg)](#an-toàn)

[Tiếng Việt](README.vi.md) · [English](README.md)

[Quickstart](#quickstart) ·
[Tài liệu](#tài-liệu) ·
[Kiến trúc](#kiến-trúc) ·
[API](#api) ·
[Cấu hình](#cấu-hình) ·
[Đóng góp](#đóng-góp-bảo-mật--hỗ-trợ)

---

## Intent Rebase Engine là gì?

**Intent Rebase Engine (IRE)** là một control-plane bằng Rust cho *sự thay đổi intent* trong các workflow có agent. IRE phiên bản hóa intent của người dùng, tính semantic diff giữa các phiên bản, mô hình hóa tác động lan truyền (downstream impact), và **rebase** các phần execution, approval và side effect đang chạy dở sang intent mới — thay vì reset tiến độ từ đầu, phớt lờ sự thay đổi, hay để agent tiếp tục sinh ra output dưới một intent đã bị bác bỏ.

IRE hướng tới những hệ thống mà con người và agent cùng lặp lại trên một mục tiêu chung: coding copilot, support automation, research workflow, và các agent chính-sách-driven mà sản phẩm đầu ra phải luôn nhất quán với intent mới nhất.

## Điểm nổi bật

- **Intent phiên bản hóa** — mỗi thay đổi có ý nghĩa tạo ra một intent version bất biến mới, kèm đầy đủ lineage.
- **Semantic diff** — nắm bắt *điều gì* đã đổi và *điều đó có nghĩa gì*, thay vì chỉ một patch văn bản.
- **Dependency graph** — liên kết mỗi clause của intent với artifact, approval và side effect tương ứng.
- **Rebase nhận-biết-tác-động** — tự động phân loại invalidation, review bắt buộc và compensation.
- **Repair planning** — sinh một execution plan đã được rebase, không phải khởi động lại từ zero.
- **Provenance mặc định** — mọi output đều truy ngược được về intent version đã sinh ra nó.
- **Multi-tenant, audit-first** — truy vấn/ghi có tenant scoping, RLS wiring, và forensic bundle để replay.
- **Bề mặt vận hành có kiểm soát** — REST + OpenAPI, operator CLI, và một runtime-adapter seam để gắn vào workflow engine bạn chọn.

## Cách hoạt động

1. **Normalize** intent thành một cấu trúc đã được phiên bản hóa và validate.
2. **Diff** intent mới với phiên bản trước đó một cách semantic.
3. **Graph** mối quan hệ phụ thuộc giữa intent và các artifact, execution, side effect.
4. **Phân loại** tác động — invalidation, review bắt buộc, compensation.
5. **Plan & trace** — sinh execution plan đã rebase và ghi lại provenance cho mọi output.

## Khi nào IRE hữu ích

| Tình huống | IRE làm gì |
| --- | --- |
| Coding copilot bị đổi spec giữa chừng | Replay lại plan, đánh dấu các patch cũ là stale, và revalidate approval dưới intent mới. |
| Policy của một support workflow được cập nhật | Dựng lại dependency graph, đánh dấu các case bị ảnh hưởng, và đề xuất compensation. |
| Research workflow bị cắt giảm budget | Phân loại lại các task downstream, làm nổi các artifact cần review, và đề xuất một plan nhỏ hơn. |
| Một batch đang chạy thì lệnh đóng băng deploy được phát | Chụp lại side effect, đóng băng apply, và sinh forensic bundle để review sau. |

---

## Quickstart

> **Yêu cầu môi trường.** Một **Rust** toolchain stable gần đây (đã pin qua `rust-toolchain.toml`, cài qua [rustup](https://rustup.rs)), **Git**, và tùy chọn **Docker** + **Docker Compose v2** cho local stack Postgres / NATS / MinIO. **Node.js 20+** chỉ cần khi bạn chạy OpenAPI spectral lint.

### 1. Clone và cấu hình

```bash
git clone https://github.com/BrianNguyen29/intent-rebase.git
cd intent-rebase
cp .env.example .env       # chỉ chứa giá trị mặc định cho local dev — xem phần Cấu hình bên dưới
```

`.env.example` chứa sẵn các **placeholder cho local dev** cho `DATABASE_URL`, `JWT_SECRET` và các key S3/MinIO. Hãy thay bằng giá trị thật trước khi dùng ngoài localhost; xem [Cấu hình](docs/getting-started/configuration.md) để biết đầy đủ danh sách biến.

### 2. Fast verify (không cần dịch vụ ngoài)

```bash
bash scripts/verify-fast.sh
# tương đương với:
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --lib --all-features
```

Vòng lặp nhanh chạy hoàn toàn trong bộ nhớ và là **nguồn chân lý cục bộ chính** của dự án.

### 3. Tùy chọn — local stack cho live-integration test

```bash
docker compose -f infrastructure/local/docker-compose.yml up -d
```

Lệnh này đưa lên **Postgres 16**, **NATS 2.10 với JetStream**, và **MinIO**. Sau đó set các env var mà suite cần (xem [`.env.example`](.env.example) và [Cấu hình](docs/getting-started/configuration.md)) và chạy các suite `#[ignore]` một cách tường minh bằng `cargo test … -- --ignored`.

### 4. Chạy API

```bash
cargo run -p intent-api
```

Smoke-test server đang chạy:

```bash
curl -s http://localhost:8080/health
```

Cấu hình mặc định dùng in-memory repository ở những nơi có thể. Set `DATABASE_URL` (và, nếu muốn, các env var của `NATS_URL` / S3) để chạy đường SQL-backed, NATS-backed hoặc S3-backed.

> **Lưu ý.** Một lần `verify-fast.sh` xanh **không phải** tín hiệu production-ready. Xem [An toàn](#an-toàn).

---

## Kiến trúc

IRE là một Cargo workspace gồm **11 crate** được tổ chức thành bốn **plane**.

### Bốn plane

| Plane | Trách nhiệm |
| --- | --- |
| **Control** | Ingest intent, versioning, semantic diff, phân tích tác động, repair planning, quyết định theo policy, audit. |
| **Execution** | Runtime adapter tới workflow engine, agent runtime, task scheduler và side-effect dispatcher. |
| **Data** | OLTP metadata, event log, object store, graph store / relational edges, analytics store. |
| **Operator** | Console, approval UI, forensic replay, policy simulation, rebase preview. |

### Workspace crates

| Crate | Mục đích |
| --- | --- |
| `intent-rebase-types` | Định nghĩa type lõi và các domain model dùng chung. |
| `intent-service` | Lưu trữ intent, semantic diff, và vòng đời (Postgres). |
| `intent-api` | HTTP API server (Axum) và middleware stack. |
| `rebase-engine` | Rebase decision engine — diff, impact, sinh plan. |
| `graph-service` | Dependency graph service — node, edge, traversal. |
| `runtime-adapter` | Runtime execution adapter (mặc định mock; Temporal ở mức bounded). |
| `rebase-orchestrator` | Điều phối orchestration, dry-run, single-shot runtime. |
| `compensation-service` | Vòng đời compensation action, executor, batch operation. |
| `forensic-service` | Sinh, xác minh và xuất forensic bundle. |
| `tenant-service` | Onboarding đa tenant, quota, và rule-pack isolation. |
| `intent-cli` | Operator CLI để chạy orchestration và inspect. |

Bản đồ đầy đủ từng thành phần nằm trong [System Overview](docs/02-architecture/01-system-overview.md) và [Component Catalog](docs/02-architecture/02-components.md).

## API

- **REST + OpenAPI** — [`docs/04-api/openapi.yaml`](docs/04-api/openapi.yaml) là tham chiếu canonical cho endpoint, kèm theo [ghi chú REST API](docs/04-api/01-rest-api.md).
- **Events** — xem [event contracts](docs/04-api/02-events.md) để biết topic và payload.
- **Webhooks** — xem [webhook contract](docs/04-api/03-webhooks.md) để biết delivery, chữ ký và cơ chế retry.

## Cấu hình

IRE được cấu hình hoàn toàn qua biến môi trường; `.env.example` chứa giá trị mặc định cho local dev. Xem [Configuration](docs/getting-started/configuration.md) để có đầy đủ tham chiếu — database, JWT, runtime adapter, NATS, S3/MinIO, forensic bundle, OpenTelemetry, CORS và các suite `#[ignore]`.

## Tài liệu

Trung tâm tài liệu công khai đầy đủ nằm ở **[docs/README.md](docs/README.md)**.

| Chủ đề | Tài liệu |
| --- | --- |
| Quickstart | [Quickstart](docs/getting-started/quickstart.md) |
| Cấu hình | [Configuration](docs/getting-started/configuration.md) |
| Phát triển & verification | [Development & Verification](docs/getting-started/development.md) |
| Tổng quan hệ thống | [System Overview](docs/02-architecture/01-system-overview.md) |
| Thành phần | [Component Catalog](docs/02-architecture/02-components.md) |
| OpenAPI (canonical) | [openapi.yaml](docs/04-api/openapi.yaml) |
| Ghi chú REST API | [REST API](docs/04-api/01-rest-api.md) |
| Events | [Events](docs/04-api/02-events.md) |
| Webhooks | [Webhooks](docs/04-api/03-webhooks.md) |
| Intent model | [Intent Model](docs/03-spec/01-intent-model.md) |
| Semantic diff | [Semantic Diff](docs/03-spec/02-semantic-diff.md) |
| Dependency graph | [Dependency Graph](docs/03-spec/03-dependency-graph.md) |
| Rebase engine | [Rebase Engine](docs/03-spec/04-rebase-engine.md) |
| Test strategy | [Test Strategy](docs/11-quality/01-test-strategy.md) |
| ADR pack | [ADR Index](docs/13-adrs/README.md) |
| Glossary | [Glossary](docs/01-product/05-glossary.md) |
| Lý do & pattern tham khảo | [Rationale & External Patterns](docs/99-reference/01-rationale-and-external-patterns.md) |

---

## Đóng góp, Bảo mật & Hỗ trợ

- **Đóng góp** — xem [CONTRIBUTING.md](CONTRIBUTING.md). Bắt đầu từ vòng local verification, tuân thủ chính sách no-overclaim và đọc các quy tắc riêng của repo trong đó.
- **Bảo mật** — xem [SECURITY.md](SECURITY.md). IRE chưa có sign-off SRE, bảo mật hay penetration-test bên ngoài; hãy báo cáo sự cố qua kênh riêng.
- **Hỗ trợ** — xem [`.github/SUPPORT.md`](.github/SUPPORT.md). Hỗ trợ ở mức best-effort, bị giới hạn bởi phạm vi dự án; **không có SLA**.
- **Quy tắc ứng xử** — xem [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- **Issues & PRs** — dùng các template [bug report](.github/ISSUE_TEMPLATE/bug_report.md) và [feature request](.github/ISSUE_TEMPLATE/feature_request.md), kèm theo [PR template](.github/PULL_REQUEST_TEMPLATE.md).

## An toàn

IRE **chưa sẵn sàng cho production** và chưa được validate cho các workload production, nhạy cảm hay khách hàng. Chỉ sử dụng cho local development, integration experiment, và nghiên cứu bounded về thiết kế. Đừng xem một lần local verification xanh là tín hiệu production-ready, và đừng dựa vào bất kỳ thiết lập, lệnh hay ví dụ nào trên site này như hướng dẫn cứng hóa cho production.

## Giấy phép

Copyright © Intent Rebase Engine Team.

Bản quyền theo **Apache License, Version 2.0** (the "License"); bạn không được sử dụng file này trừ khi tuân thủ License. Bạn có thể lấy bản sao của License tại <https://www.apache.org/licenses/LICENSE-2.0>.

Trừ khi luật pháp yêu cầu hoặc được thỏa thuận bằng văn bản, phần mềm phân phối theo License được phân phối "NGUYÊN TRẠNG" ("AS IS"), KHÔNG CÓ BẤT KỲ BẢO ĐẢM NÀO, dù rõ ràng hay ngụ ý. Xem file [LICENSE](LICENSE) để có văn bản đầy đủ.
