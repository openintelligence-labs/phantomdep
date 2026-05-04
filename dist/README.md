# Distribution channels

Skeletons for the distribution channels PhantomDep ships through. None of these are *published* yet; they're committed so the v1.0 release pipeline can wire them in directly.

| Channel | Path | What it is | Publish step |
|---|---|---|---|
| Homebrew | `dist/homebrew/phantomdep.rb` | Bottle-style formula pointing at the GitHub release tarballs. | Copy to `openintelligence-labs/homebrew-tap/Formula/phantomdep.rb`, regenerate SHAs, `brew bump-formula-pr`. |
| npm | `dist/npm/` | Postinstall wrapper that downloads the right binary on `npm install -g phantomdep`. | `npm publish` from `dist/npm/` after bumping `version` to match the GitHub release. |
| PyPI | `dist/pypi/phantomdep/` | Per-platform wheels with the binary bundled at `phantomdep/bin/`. | Build N wheels with `cibuildwheel` against the GitHub release artefacts, then `twine upload`. |
| crates.io | `crates/phantomdep-cli/` | The actual Rust source crate. | `cargo publish -p phantomdep-core && cargo publish -p phantomdep-cli` from the workspace root. |
| GitHub Releases | `.github/workflows/release.yml` | Tar.gz / zip per (os, arch). | `git tag v1.0.0 && git push --tags`. |

## v1.0 release sequence

See [LAUNCH.md](../LAUNCH.md) at the repo root for the maintainer-only steps. Short form:

```text
1. Bump versions (already at 1.0.0 throughout the workspace).
2. cargo test --workspace && pytest && npx tsc -p extensions/vscode
3. git commit -m "release v1.0.0" && git tag v1.0.0
4. git push --tags  ⟶ release.yml builds & publishes GitHub Release
5. cargo publish -p phantomdep-core
6. cargo publish -p phantomdep-cli
7. Update dist/homebrew/phantomdep.rb with the published SHAs, open PR to homebrew-tap
8. cd dist/npm && npm publish
9. Build wheels with cibuildwheel against the GitHub Release artefacts, then twine upload
10. cd extensions/vscode && vsce publish && ovsx publish
```

The optional steps (Homebrew tap / PyPI wheels / npm wrapper) can be deferred to v1.1 if you want to ship the GitHub Release binary first and add packaged channels post-launch.
