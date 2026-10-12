# Approved SDD decisions and implementation plan

Date: 2026-10-12. Status: architecture and workflow approved by the user. Current scope: reconciliation of existing documentation only, as requested by the user. Do not scaffold the application, install more tools or run implementation tasks until the user requests execution. No application or test harness has been implemented.

## Decision record

| Topic | Approved decision | Consequence |
| --- | --- | --- |
| Language | Rust for application logic in frontend, backend and test scenarios | Browser output still includes HTML/CSS and generated JavaScript/WASM glue |
| Web framework | Leptos fullstack, SSR and WASM hydration | Server functions are the initial interface; no parallel complete REST API |
| HTTP runtime | Axum integrated through Leptos, with Tokio | One application process and deployment; no separate backend service |
| Architecture | Modular monolith, initially one application Cargo package | Domain and application rules remain separate from UI and persistence |
| Database | Local TursoDB using the Rust `turso` crate | One process owns the persistent database; no PostgreSQL schema tenancy |
| Countries | Independent subdomain, VPS, database and configuration per country; initially one | Same application reused across deployments; no global runtime database required for foundation |
| SDD | Small Markdown specs inspired by OpenSpec, without mandatory CLI | One feature/change at a time; specification, tasks and evidence |
| Agent instructions | One short root AGENTS.md | Add local instruction files only when a real exception appears |
| Harness | Thin Justfile with dev/check/test/e2e/load commands | Same reproducible checks locally and in CI; commands are future deliverables |
| Browser testing | Chrome DevTools MCP for inspection; Fantoccini/WebDriver for Rust regressions | Inspect a real browser and preserve repeatable critical flow tests |
| Load testing | Goose scenarios in Rust against release builds | Early bounded smoke test; final campaign after functional/browser acceptance |
| Current delivery | Correct existing documentation; no separate analysis reports | Approval of the plan is not authorization to start implementation in this stage |

All following implementation descriptions are future work. The illustrative tree in the repository guide and the command contracts below do not describe existing application files. The previous NestJS/React/PostgreSQL architecture is superseded; current guides have been reconciled with these decisions. Git preserves the previous documents. Unresolved product requirements remain open, as listed in the PRD.

## Product decisions still open

1. First country and its production domain, locale and deployment configuration. Brazil may be used as a clearly labelled development fixture, not an implicit production decision.
2. Real identity/biometric provider, identity proofing, recovery and protected biometric template handling. A hash alone does not settle biometric matching or uniqueness.
3. Private voting protocol, key custody, unlinkability, public/private double-vote prevention and what a receipt proves. Select and validate an established protocol; do not invent a cryptographic scheme.
4. Electoral semantics: electorate snapshot date, exact majority/quorum formulas, abstentions, ties and changes of residence. Existing PRD examples are input, not resolutions of missing edge cases.
5. VPS resources, measured target load, backup/recovery objectives and final performance thresholds. The p95 limits below are the working acceptance interpretation; validate on representative hardware before claiming capacity.

These questions do not block a future technical foundation. They must be resolved before implementing the affected product guarantees. A development demonstration must be identified as such. Before resolving an open question or making a new decision, ask the user with short options; do not infer electoral or security rules. The expanded product decision list is in [PRD section 13](../prd/prd.md#13-decisões-de-produto-pendentes).

## Chosen workflow

Use a small spec-driven workflow based on OpenSpec's distinction between current specifications and proposed changes. Start with Markdown in the repository; the CLI is optional. Do not install two competing SDD systems or build an agent orchestration service.

OpenSpec supports exploration, a reviewable proposal, implementation tasks and archival that updates the current specifications. Spec Kit is a valid alternative, including a shorter feature workflow, but its constitution and additional quality gates introduce more artifacts than this first version needs. This preference is an assessment for this project, not a universal ranking of tools.

The harness means the reproducible environment and commands used to build, start, inspect and verify the application. It is not another production service.

## Existing documents and future artifacts

Architecture decisions stay in this document; the foundation requirements, task checklist and evidence status are in [foundation-setup.md](../architecture/foundation-setup.md). Update these existing documents instead of creating another ADR, review report or foundation dossier that duplicates them.

[repository_structure.md](../architecture/repository_structure.md) describes the current document map and future code structure. There is no application Cargo package, Justfile, source tree, migrations, CI, browser tests or load package yet.

For later features, keep observable requirements, tasks and evidence together in the relevant existing document where practical. Create a separate feature spec only when the work needs it; never create empty `specs/current` entries claiming implemented behavior. Current behavior must be backed by execution evidence. Archival updates must reflect the actual implementation, not just a plan.

The active PRD and guides now describe the Rust stack. API capabilities, environment acceptance and setup order have been reconciled; identity, electoral rules, cryptographic protocol, global administration and offline behavior remain explicitly open. Preserving a proposed product goal does not establish its feasibility or implementation.

## Feature loop

1. Specify observable behavior, boundaries and failure cases; identify unresolved product/security decisions.
2. Review the spec and create a short dependency-ordered task list. Existing approval covers routine implementation within its scope. Ask the user with concise options whenever an unresolved requirement or a new decision needs resolution.
3. Implement one runnable slice and add tests that verify behavior, including critical negative/concurrent cases.
4. Execute the relevant checks, start the application and inspect affected flows in the browser.
5. Record evidence and remaining limitations, update current specs and close only criteria actually satisfied.

Example acceptance criterion: given an eligible synthetic voter and an open synthetic election, two concurrent submissions cannot produce two accepted ballots; a retried accepted submission has defined idempotent behavior. Test distinct concurrent attempts and replay of the same attempt separately. Do not infer this guarantee from the UI disabling a button.

## AGENTS.md and harness

Keep one concise root AGENTS.md with stack, code boundaries, spec workflow, commands and verification requirements. Preserve CodeGraph navigation instructions. Add directory-specific files only if a genuine local exception appears.

Approved command interface, to be implemented and verified during foundation:

| Command | Contract |
| --- | --- |
| `just dev` | Apply development migrations, start Leptos locally and print the actual URL; no implicit destructive reset |
| `just check` | Format/lint checks and compilation of server + hydration WASM targets |
| `just test` | Domain and integration tests against disposable actual TursoDB databases |
| `just e2e` | Start an isolated app/browser environment, execute critical browser flows, clean up owned processes, fail on unmet assertions |
| `just load` | Run bounded Goose scenarios against a test release build, produce report and fail on violated acceptance limits |

Use `just` only as a thin command runner. Keep the same commands locally and in CI. Bootstrap checks must report missing tools, not silently substitute another database or skip the browser. Pin the Rust toolchain and dependency lockfile. Production code and test scenarios remain Rust; command/config files and generated browser glue are infrastructure artifacts.

Each test run has its own database path and synthetic fixtures. App readiness uses a bounded health check, not a fixed sleep. Process cleanup must only stop processes created by that run. Generated reports exclude secrets and real personal data.

## Test strategy

### Domain and persistence

Use ordinary Rust tests for electoral arithmetic, state transitions and authorization. Add property-based tests only where they improve confidence, such as arithmetic boundaries and invalid state transitions. Use the actual TursoDB crate for migrations, uniqueness, atomicity, rollback, persistence after restart and concurrent submissions.

Test authorization through the network interface as well as application functions. Pin setup version and electorate snapshot for an election; specify exact quorum, majority, abstention and tie semantics before implementing them. Test the race between vote acceptance and election closure.

### Browser

Use the available Chrome DevTools MCP for interactive inspection: rendered screens, hydration, keyboard navigation, mobile viewport, session expiry and console/network errors. This inspection supplements reproducible tests; it does not replace them.

For repeatable E2E scenarios written in Rust, use Fantoccini with WebDriver and a compatible browser/driver. The first scaffold must prove one real interaction after hydration. Grow coverage around login, proposal lifecycle, signatures, voting and results as those features become available. Browser assertions should use stable semantic labels/selectors and bounded waits, not timing sleeps.

Biometrics and election cryptography require dedicated real-provider/protocol validation. Development mock providers enable deterministic UI/backend tests but must be clearly labelled and blocked in production. Do not describe a development ballot model as implementing the PRD's promise of unlinkability.

### Load, at the final acceptance stage

Use Goose: scenarios are Rust code, including realistic session state, CSRF requirements and the actual server-function transport. HTTP success alone is not a vote accepted; parse and validate the application response. Fixtures must include enough distinct eligible voters for the scenario. Do not use one identity repeatedly as the normal throughput workload.

Final scenarios: normal mixed browsing/writing load, ramp to saturation, concentrated voting on one proposal, simultaneous duplicates/replays, and sustained execution with restart/recovery checks. A small early smoke run verifies the load harness; the final performance campaign happens after functional/browser acceptance.

Working acceptance limits derived from the PRD: p95 listing duration under 2 seconds and p95 vote acceptance under 3 seconds at a documented offered load, with under 1% unexpected request failures. The plan adopts these percentiles as a working interpretation; the original PRD did not specify percentiles or offered load. Duplicate ballots, lost acknowledged ballots or inconsistent results are never acceptable. Expected denials are reported separately from unexpected failures. Final target load and measurement conditions remain to be defined.

Report achieved throughput, attempted load, p50/p95/p99 latency, errors, CPU/RAM, disk behavior, transaction conflicts/retries and final database invariants. Capture commit, build mode, machine specs, dataset, scenario duration and provider mode. Do not make latency assertions on empty samples or zero accepted votes.

Compile in release mode. Initial bounded experiments can use 10, 50 and 100 simulated users, increasing only while there is capacity. These are starting probes, not a capacity promise. Local client and server share hardware; measure generator saturation and repeat on an equivalent staging VPS from a separate generator before asserting deployment capacity.

## Implementation order

Execution is deferred by the user's current instruction. The next implementation delivery, when requested, is foundation only; approval does not imply that the entire identity/voting application can be built with unresolved security/product decisions.

1. Foundation: execute the requirements/checklist in the existing foundation guide: Leptos/Axum/TursoDB scaffold, health check, migrations, command runner, CI, one browser E2E and a bounded load smoke. Estimated 2–4 hours of execution if dependencies/browser work; 4–8 hours if toolchain integration needs diagnosis.
2. Identity and setup: sessions, RBAC, test identity provider and setup lifecycle, including bootstrap and law-creation blocking before adoption; specify real identity integration separately.
3. Proposal flow: drafts, publication, signatures, debate and persisted scheduled transitions.
4. Voting and audit: settle cryptographic protocol/privacy requirements, enforce acceptance/closure invariants, receipts and verifiable results. Security-sensitive design and validation have no reliable estimate yet.
5. Acceptance: release build, full browser regressions, backup restoration, final load campaign and evidence-based capacity report. A baseline campaign can take 1–2 hours after all scenarios and fixtures are ready; fixes are additional.

The first foundation slice is complete only when a clean local workflow starts the application, TursoDB data survives restart, server/WASM builds pass, and a browser interaction plus its repeatable E2E test succeed. The whole application remains incomplete until its product and security acceptance criteria are met.

## Current state and next executable step

- Completed: initial review/research, architecture choice, SDD/harness plan, root AGENTS.md, and reconciliation of existing README/PRD/architecture/wireframes with the Rust architecture. Foundation requirements and a pending checklist now live in the existing foundation guide.
- Not implemented: Cargo application, migrations, Justfile, CI, browser tests and load scenarios. No application tests or load tests have run.
- Environment inspection found Rust and Chrome. Initial tool downloads were moved out of the repository to a temporary directory when execution was deferred; environment prerequisites must be checked again when work resumes.
- First future implementation: execute the existing foundation checklist for the smallest local Leptos/Axum/TursoDB slice. Its acceptance is build + persistent database + real hydrated browser interaction, not voting or biometric readiness. No checklist item has been executed.
- User-facing next step: request execution of the foundation when ready. Until then, work remains documentation only.

## Research sources from the architecture planning session

- OpenSpec workflow: https://openspec.dev/docs/quickstart
- GitHub Spec Kit workflow: https://github.github.io/spec-kit/quickstart.html
- Official AGENTS.md guidance: https://learn.chatgpt.com/docs/agent-configuration/agents-md
- Leptos testing guidance: https://book.leptos.dev/testing.html
- Fantoccini Rust WebDriver client: https://github.com/jonhoo/fantoccini
- Goose Rust load tests: https://book.goose.rs/
