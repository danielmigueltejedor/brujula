# Agent instructions — brujula

## Repository-specific deterministic quality contract (2026-10-26)

**Project:** GNOME Web / Epiphany C, Meson and GNOME integration fork.

**Engineering requirements:** Keep Meson conventions and upstream contribution rules. Do not claim GUI or WebKit integration tests passed without all required GNOME dependencies.

**Verification commands:** meson setup build (where GNOME/WebKit dependencies are available), meson compile -C build, meson test -C build.

Before completion, run relevant checks and report exact pass/fail results. Add regression tests for changed behavior, bounds and failure paths. Never weaken/delete/skip tests, suppress warnings, or change CI thresholds to fabricate green results. Changes to `QUALITY_GATES.md`, this contract, and workflows require human review. The `quality` status must be required in branch protection to make the gate enforceable. If native dependencies prevent a test, explicitly report the blocker rather than claiming success. Existing project instructions have precedence for build setup; this section augments them.
