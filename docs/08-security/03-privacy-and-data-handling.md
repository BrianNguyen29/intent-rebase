# Privacy and Data Handling

## Data categories
- operational metadata
- user-generated intent content
- references to external docs/tickets
- artifacts
- audit logs
- approval records

## Privacy principles
- minimize copied source content
- store references where possible
- separate metadata from payload
- configurable retention
- encryption at rest and in transit

## Redaction
Các trường có thể cần redaction/masking:
- secrets
- tokens
- customer PII
- payment data
- legal or HR content

## Export controls
Forensic export phải hỗ trợ:
- redacted mode
- full mode with elevated approval
- short-lived signed URLs
