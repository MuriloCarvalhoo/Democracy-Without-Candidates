# Democracy Without Candidates

## Architecture approved by the user

- Application logic is Rust: Leptos SSR + WASM hydration, integrated Axum, Tokio and TursoDB via the Rust `turso` crate.
- One independent deployment, subdomain and VPS per country; initially Brazil, with country-specific configuration for reuse elsewhere. One application process owns the local persistent database.
- A Python facial-processing service on the same VPS is the approved exception to Rust. Reuse only the auth-face facial engine; Rust owns accounts, sessions, authorization and TursoDB. Keep this service private/loopback and validate it before use.
- Registration requires email, password, name and address. The initially registered address determines city/state/country participation and cannot be changed in the current version, including through direct endpoint calls.
- Brazil uses BrasilAPI CEP lookup for address autofill. Select/enable the address provider through per-instance database feature flags; retain BrasilAPI response field names in persistence. Country comes from deployment configuration, not the CEP response. Feature flags are loaded at startup; database changes take effect only after application restart. The current address scope uses only returned fields; do not collect house number or complement.
- Login uses email + password, face and TOTP, without email confirmation or remember-me. Sessions use TursoDB and a host-only HttpOnly/Secure session cookie, expire after 15 minutes idle and retain expiry/revocation across app restart.
- TOTP recovery uses single-use hashed codes while retaining password/face; password recovery uses face + TOTP without email delivery. Detailed approved flows and agent proposals are in docs/architecture/auth-rbac-flow.md and docs/development/sdd-plan.md.
- The remaining defaults were delegated by the user for later review: follow the explicitly provisional SDD/PRD baseline, but keep real identity/private voting blocked until evidence.
- Email identifies the login account; do not require CPF or another national identity document. Email/face matching alone do not establish one-person uniqueness.
- Online-only product scope. No offline participation, physical voting or physical signature collection. PWA installation does not imply offline operation.
- Start with one application package, organized into domain, application, infrastructure and feature modules. Add crates or services only for a demonstrated need.
- Keep business rules and authorization in server-side application/domain code. Leptos server functions are network endpoints, not trusted calls.
- The previous NestJS/React/PostgreSQL architecture is superseded. Current guides must match the approved Rust architecture; Git preserves the previous documents.

## Communication and documentation

- Always use the installed `i-have-adhd` skill for user-facing responses unless the user disables it.
- Keep responses short and concise. Ask one pending question at a time with options by default. When the user delegates remaining decisions, record agent proposals for later review in the existing SDD/PRD; do not label them individually approved or implemented.
- Keep documentation and code concise and simple. README presents the project, usage, documentation, contribution and license; session history belongs in ai-memory and technical detail in the relevant docs. Do not duplicate them in README.
- Update existing documents. Do not create standalone analysis/review reports or duplicate decision records. Keep foundation requirements and tasks in `docs/architecture/foundation-setup.md` and architecture decisions in `docs/development/sdd-plan.md`.

## Spec-driven development

- Read `docs/development/sdd-plan.md` for the approved workflow, decision record and validation strategy. Product questions explicitly listed as open are not settled requirements.
- The user requested implementation on 2026-10-24 (branch `feat/implementation`). Implementation decisions taken autonomously are recorded for review in `docs/development/sdd-plan.md`; do not label them approved.
- Describe each feature with observable requirements, failure cases and acceptance criteria before implementing it. Link important criteria to tests and validation evidence.
- Resolve material contradictions explicitly. Delegated electoral defaults must be labelled agent proposals for review, not approved production rules. Never claim biometric uniqueness, ballot secrecy or cryptographic guarantees without implementation and validation.
- Keep the specification and task checklist consistent with the actual result. Record blockers and validation limits; never mark an unexecuted check as passed.
- Work in small runnable slices. Routine implementation within an approved scope does not require repeated permission.

## Local execution and validation

- Commands (Justfile): `just doctor`, `just dev-seed`, `just dev`, `just check`, `just test`, `just e2e`, `just face-test`, `just load`, `just backup-test`. Verify a command exists before reporting it.
- `cargo test` rebuilds `target/debug/dwc` with different cfg; browser tests must run through `just e2e`, which copies the cargo-leptos binary.
- TursoDB: write through `Db::write()` only (one serialized writer; concurrent writers return Busy). The serving process locks the database file, so operator commands run on a stopped instance except online snapshots (SIGUSR1). Avoid triggers with long `OR` chains.
- Server functions use fixed endpoints (`#[server(endpoint = ...)]`); a `#[server] fn foo_bar` generates a `FooBar` type.
- Migrations are append-only files registered in `src/infrastructure/migrations.rs` (checksums); never edit an applied migration.
- Compile both the SSR server and browser WASM targets. Native Rust tests alone do not validate hydration or browser behavior.
- Test critical rules with Rust tests and persistence with the actual TursoDB engine in isolated disposable databases.
- Run affected user flows in a real browser. Capture useful evidence, console/network failures and accessibility issues; add reproducible E2E regression tests for critical flows.
- Run load tests against a release build with synthetic identities and isolated test data. Check database integrity after load, not only HTTP latency.

## Development boundaries

- Bind local development to loopback by default. Keep secrets, database files, biometric material and generated private artifacts out of Git.
- Test identity providers must be enabled explicitly for development/testing and must be rejected by production configuration. A successful mock test does not validate real biometrics or ZKP.
- Use migrations for schema changes. Never reset a persistent user database as part of a test command.
- Preserve unrelated local changes. Use `/usr/bin/git` for repository operations.

<!-- CODEGRAPH_START -->
## CodeGraph

When `.codegraph/` exists at the repository root, use `codegraph_explore` or `codegraph explore` before searching or reading implementation code. If the index is empty or cannot answer, continue with `rg` and direct reads. Do not create or refresh the index unless requested.
<!-- CODEGRAPH_END -->
