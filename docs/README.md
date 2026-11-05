# Upscalee documentation

Start with the [project README](../README.md) for a quick overview and setup.

| Document | Purpose |
|----------|---------|
| [PRD.md](PRD.md) | What we build and why: personas, goals, requirements FR-001…FR-023 with acceptance criteria, NFRs, open questions |
| [ARCHITECTURE.md](ARCHITECTURE.md) | How it is built: components, flows, IPC and command contracts, storage, errors, deviations from the brief |
| [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md) | Tokens (colour, type, spacing, motion), components and states, screens, accessibility |
| [SECURITY.md](SECURITY.md) | Threat model, mitigations, secrets and privacy rules, vulnerability reporting |
| [TESTING_STRATEGY.md](TESTING_STRATEGY.md) | Test pyramid, how to run suites, manual release checklist, quality gates |
| [DEPLOYMENT.md](DEPLOYMENT.md) | Build prerequisites, release build, installer behaviour, signing, versioning, CI |
| [ROADMAP.md](ROADMAP.md) | Milestones M1–M6 status and the prioritised task backlog |
| [adr/](adr/) | Architecture decision records 0001–0006 |
| [Upscalee-Architecture.md](Upscalee-Architecture.md) | The original design brief (kept verbatim; superseded where ADRs say so) |

Repository-level guides: [AGENTS.md](../AGENTS.md) (rules for coding agents) ·
[CONTRIBUTING.md](../CONTRIBUTING.md) · [CONTRIBUTORS.md](../CONTRIBUTORS.md) ·
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).

## Status legend

| Label | Meaning |
|-------|---------|
| **Verified** / **Done** | Implemented and covered by automated tests |
| **Implemented** / **Done – verify** | Implemented; needs the on-device checks in TESTING_STRATEGY.md |
| **Planned** / **Todo** / *(proposed)* | Not built yet, or a target that hasn't been measured |
| **Open question** / **TBD** | Needs a decision from the maintainer |
