# AI Engineering Quality Contract

An AI coding agent must supply real outputs for its checks, add meaningful regression tests for changed behavior, and must not skip/delete tests or weaken CI or quality thresholds to get a green build. Human review is required for any changes to tests or policies.

## Automated checks
- `git diff --check` verifies patch hygiene.
- Changed Python/JSON/TOML/shell files are parsed or checked for syntax errors.
- Existing repository CI remains authoritative for build, domain-specific tests and coverage (when configured).

This foundational gate **does not certify builds, coverage or security**. Expand it with native build/test jobs after measuring a reliable baseline. Make the `quality` status required in branch protection.
