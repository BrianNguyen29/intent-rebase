# CI/CD

## Pipeline stages
1. lint / format
2. unit tests
3. contract tests
4. integration tests
5. replay tests
6. security scans
7. image signing
8. deploy to preview/staging
9. smoke tests
10. controlled production rollout

## Special requirements for IRE
- rule pack versioning
- diff classifier versioning
- replay tests against historical rebase scenarios
- adapter compatibility tests

## Release strategies
- canary
- blue/green where possible
- feature flags for classifier behavior
- tenant allowlists for risky features
