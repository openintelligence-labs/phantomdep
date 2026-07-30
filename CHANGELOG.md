# Changelog

All notable changes to PhantomDep are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows [SemVer](https://semver.org/).

## [1.0.1] — 2026-07-30

### Fixed
- `phantomdep scan` now treats dependency manifests as first-class scan inputs: directory scans (and single-file scans) pick up `requirements.txt`, `constraints.txt`, `requirements-*.txt` / `*-requirements.txt`, and `pyproject.toml` in addition to `Cargo.toml` and `go.mod`. A directory containing only a manifest previously reported "Scanned 0 files" and missed phantom packages declared there. Closes [#2](https://github.com/openintelligence-labs/phantomdep/issues/2).

## [1.0.0] — 2026-05-04

First public release.

### Added
- `phantomdep replay` — iterates the loaded Phantom-DB and shows what's currently caught against the live registry.
- `phantomdep benchmark` — microbenchmark harness with p50/p95/max for the four canonical scenarios.
- `assets/badges/protected-by-phantomdep.svg` — embeddable badge for downstream READMEs.
- `docs/BENCHMARK.md` — public, reproducible benchmark page with fair head-to-head.
- `assets/demos/` — 5 deterministic VHS-rendered demo GIFs (headline, doctor, scan-polyglot, hook, replay).
- VS Code extension icon (`extensions/vscode/icon.png`).
- Distribution channel skeletons: `dist/homebrew/phantomdep.rb`, `dist/npm/`, `dist/pypi/phantomdep/`.
- `Makefile` with `build`, `test`, `bench`, `demo`, `demo-render`, `clean` targets.
- `CONTRIBUTING.md`, `SECURITY.md`.

### Changed
- All workspace versions bumped from 0.x → **1.0.0**.

## [0.8.0] — 2026-05-04

### Added
- LSP server (`phantomdep lsp`) — full Content-Length-framed JSON-RPC over stdio.
  - `textDocument/publishDiagnostics` for python / javascript / typescript files.
  - `textDocument/codeAction` returning "Did you mean ..." quick-fixes.
  - UTF-16 character offsets per LSP spec.
  - Reverse resolver table so `pyyaml` verdicts land on `import yaml` ranges.
- VS Code / Cursor / Windsurf extension at `extensions/vscode/` — published-shape `.vsix`, vendored language-client deps.

## [0.7.0] — 2026-05-03

### Added
- MCP server (`phantomdep mcp`) — stdio JSON-RPC 2.0, MCP protocol version 2025-06-18, four read-only tools.
- Claude Code PreToolUse hook (`phantomdep hook check`) — gates Bash install commands and Write/Edit/MultiEdit events touching dependency manifests.
- `phantomdep hook install` / `uninstall` — idempotent settings.json wiring with marker-based detection and backup.
- `pyproject.toml` parser handling PEP 621 + Poetry tables.

### Fixed
- MultiEdit hook events now collect `new_string` from every `edits` entry (was silently no-oping).
- Hook + MCP `validate_imports` paths now use bounded-concurrency parallel lookups (was sequential).
- Wrap subcommand uses bounded concurrency (was unbounded `FuturesUnordered`).
- JS `strip_comments` now respects string literals — `"// not a comment"` no longer corrupted.
- Cache key normalised per-ecosystem (PyPI/npm/crates lowercase, Go/Maven preserve case).

## [0.6.0] — 2026-05-03

### Added
- `phantomdep wrap` install-time firewall for **8 package managers**: pip, uv, poetry, npm, pnpm, yarn, cargo, go.
- Per-manager argument parsers with PEP 508 + npm-version-spec stripping.
- pip requirements file resolution (`pip install -r req.txt`).
- Confirmation prompt on WARN verdicts (TTY only); `--yes` / `--no-prompt` / `--dry-run` flags.
- Launch-quality README.

## [0.5.0] — 2026-05-04

### Added
- Phantom-DB feeder pipeline (`phantom-db/pipelines/`): Python tooling that runs probe prompts against Ollama or a deterministic mock provider, extracts package names, and emits new candidate JSON files for PR review.
- Verifier bot: re-checks every Phantom-DB entry against the live registry; promotes phantom→squatted (and vice versa); never demotes malicious entries.
- Hourly verifier + nightly feeder GitHub Actions workflows.
- PEP 503 name normalisation so underscore/hyphen variants collapse to one canonical form.

## [0.3.0] — 2026-05-03

### Added
- crates.io support (existence + age + downloads + repository URL).
- Cargo.toml dependency parser (handles `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, target-specific tables, `package = "..."` rename, inline-table form).
- Go modules support via `proxy.golang.org` (with case-encoding for uppercase paths).
- go.mod + .go file parsers.
- deps.dev v3 client (unified metadata fallback).
- Release pipeline GitHub Action — matrix builds for 5 targets (linux x86_64/aarch64, macOS x86_64/aarch64, windows x86_64).

## [0.2.0] — 2026-05-03

### Added
- npm registry checker (handles scoped packages with URL-encoded slashes).
- JS/TS import parser (ES modules + CommonJS + dynamic + side-effect imports, Node builtins, scoped subpaths).
- Multi-ecosystem scan (`scan_path`).
- SARIF 2.1.0 output for GitHub Code Scanning.
- Markdown PR comment output.
- GitHub Action wrapper (`actions/action.yml`) with auto binary download.

## [0.1.5] — 2026-05-03

### Added
- JSON-on-disk Phantom-DB loader (`phantom-db/<eco>/<letter>/<name>.json`).
- SQLite cache layer with split TTLs (24h for found, 5min for 404s).
- Python import parser + 22-entry `import → pypi-name` resolver table.
- `phantomdep scan PATH`, `phantomdep doctor`, `phantomdep explain` commands.

## [0.1.0] — 2026-05-02

### Added
- Initial release. Verdict engine, evidence bundle model, PyPI checker, lookalike detection, bootstrap Phantom-DB with four canonical entries.
- `phantomdep check NAME --ecosystem pypi` end-to-end.
