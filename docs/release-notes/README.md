# Release notes

Every version has its notes here, written by hand: `docs/release-notes/vX.Y.Z.md`. They are the
text of the GitHub release, and Brújula shows the current version's file on its news page
(`crates/brujula-core/src/news.rs` embeds it), so people see exactly this file in both places.

## Rules, enforced by CI

- Bumping `version` in `Cargo.toml` needs `docs/release-notes/vX.Y.Z.md` in the same pull
  request, and `news.rs` pointing at it, or the **Release notes are written** check fails
  (`scripts/check-release-notes.sh`, also run by `release-auto.yml` before it tags).
- Merging the version bump releases it: **Release from main** (`release-auto.yml`) tags the
  merge, creates the release with this file as its text, builds the AppImage and publishes the
  release. Nobody pushes a tag by hand.
- When a file here changes later on `main`, the **Release notes** workflow copies it into the
  release of the same tag.
- No generated lists ("What's Changed", "Full Changelog"), no placeholders, no mentions of forks,
  upstream or other products' names.

## Format

The news page reads this shape, so Brújula's notes keep it:

1. `# Brújula X.Y.Z`, then a line on the platform and package.
2. `## Novedades` and/or `## Arreglos`, with bullets.
3. `## Instalar`, naming this version's `Brujula-X.Y.Z-x86_64.AppImage`.

Copy the latest file as a starting point.
