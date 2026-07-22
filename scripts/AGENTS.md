# Mova Scripts AGENTS

These instructions apply to helper tooling under `scripts`. Repository-wide rules live in the root `AGENTS.md`; this file defines script and media-analysis constraints.

## Responsibilities

- Own Python and other helper scripts, offline jobs, media analysis, and intro or outro detection.
- Treat script inputs, outputs, exit codes, and timeouts used by Rust as stable machine-readable contracts.
- Do not use human-readable logs as the only integration contract.
- Prefer existing repository tooling and the lowest-friction runtime. Add Python packages or system dependencies only when necessary and document the reason.

## Verification

- Run the narrowest useful script command or fixture for a script change.
- Run the relevant Rust checks when a script change affects a Rust caller.

## Documentation

- Update the relevant README when script dependencies, execution, output contracts, or media-analysis behavior changes.
