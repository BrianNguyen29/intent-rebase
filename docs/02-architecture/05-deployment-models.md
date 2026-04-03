# Deployment Models

## Model A — Single-tenant self-hosted
Phù hợp:
- enterprise regulated
- air-gapped hoặc semi-isolated
- custom integrations nhiều

Ưu điểm:
- kiểm soát dữ liệu tối đa
- dễ bán enterprise

Nhược điểm:
- vận hành phức tạp
- upgrade management khó

## Model B — Multi-tenant SaaS
Phù hợp:
- startup / teams vừa
- cần onboarding nhanh

Ưu điểm:
- triển khai nhanh
- telemetry tập trung
- data network effects tốt

Nhược điểm:
- tenant isolation yêu cầu cao
- compliance phức tạp hơn

## Model C — Hybrid control plane
Phù hợp:
- metadata control plane hosted
- artifact payload / secrets self-hosted

Ưu điểm:
- cân bằng tốc độ và compliance
- giảm chi phí self-host toàn bộ

## Khuyến nghị
Bắt đầu với:
- bản dev/staging đơn tenant
- production architecture hỗ trợ nâng dần lên hybrid hoặc multi-tenant

## Mô hình môi trường
- local dev
- ephemeral review env
- shared staging
- pre-prod
- prod
