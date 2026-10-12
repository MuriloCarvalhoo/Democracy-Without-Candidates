# Democracy Without Candidates

## Architecture approved by the user

- Application logic is Rust: Leptos SSR + WASM hydration, integrated Axum, Tokio and TursoDB via the Rust `turso` crate.
- One independent deployment, subdomain and VPS per country; initially Brazil, with country-specific configuration for reuse elsewhere. One application process owns the local persistent database.
- Online-only product scope. No offline participation, physical voting or physical signature collection. PWA installation does not imply offline operation.
- Start with one application package, organized into domain, application, infrastructure and feature modules. Add crates or services only for a demonstrated need.
- Keep business rules and authorization in server-side application/domain code. Leptos server functions are network endpoints, not trusted calls.
- The previous NestJS/React/PostgreSQL architecture is superseded. Current guides must match the approved Rust architecture; Git preserves the previous documents.

## Communication and documentation

- Always use the installed `i-have-adhd` skill for user-facing responses unless the user disables it.
- Keep responses short and concise. Ask one pending question at a time, with explicit options; do not settle open product questions by assumption.
- Keep documentation and code concise and simple. README presents the project, usage, documentation, contribution and license; session history belongs in ai-memory and technical detail in the relevant docs. Do not duplicate them in README.
- Update existing documents. Do not create standalone analysis/review reports or duplicate decision records. Keep foundation requirements and tasks in `docs/architecture/foundation-setup.md` and architecture decisions in `docs/development/sdd-plan.md`.

## Spec-driven development

- Read `docs/development/sdd-plan.md` for the approved workflow, decision record and validation strategy. Product questions explicitly listed as open are not settled requirements.
- Current user scope is documentation only. Do not create application code, install further tools or execute the implementation plan until the user requests implementation.
- Describe each feature with observable requirements, failure cases and acceptance criteria before implementing it. Link important criteria to tests and validation evidence.
- Resolve material contradictions explicitly. Do not invent electoral rules or claim biometric uniqueness, ballot secrecy or cryptographic guarantees that have not been implemented and verified.
- Keep the specification and task checklist consistent with the actual result. Record blockers and validation limits; never mark an unexecuted check as passed.
- Work in small runnable slices. Routine implementation within an approved scope does not require repeated permission.

## Local execution and validation

- There is currently no Cargo application or runnable test harness. Add and verify actual commands when scaffolding the application; do not report proposed commands as existing tools.
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
