# Distribution channels

This directory contains the source artefacts that PhantomDep ships through each distribution channel. Each subdirectory is the canonical template the release pipeline renders into.

| Channel | Path | What it is |
|---|---|---|
| Homebrew | `dist/homebrew/phantomdep.rb` | Formula pointing at the GitHub Release tarballs. Lives at `openintelligence-labs/homebrew-tap/Formula/phantomdep.rb` once published. |
| npm | `dist/npm/` | Thin wrapper package whose `postinstall` script downloads the right binary on `npm install -g phantomdep`. |
| PyPI | `dist/pypi/phantomdep/` | Per-platform wheels with the binary bundled at `phantomdep/bin/`. |
| crates.io | `crates/phantomdep-cli/` | The actual Rust source crate (crate is published as `phantomdep` (dir `phantomdep-cli`), depends on `phantomdep-core`). |
| GitHub Releases | `.github/workflows/release.yml` | Tar.gz / zip per (os, arch); driven by `git tag v*`. |
