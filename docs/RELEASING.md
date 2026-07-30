# Releasing PhantomDep

How a version goes from `main` to customers. Written against **v1.0.0** (2026-07-29).
Steps marked **BLOCKED** need credentials that do not exist on the release machine yet —
the exact commands are ready to paste once tokens arrive.

## 0. Preconditions

```sh
git fetch --tags
git status                      # clean tree, main == origin/main
cargo test --workspace          # must be green (128 tests as of v1.0.0)
cargo build --release
./target/release/phantomdep --version
```

Version lives in one place: `[workspace.package] version` in `Cargo.toml`
(inherited by both crates and mirrored in `dist/npm/package.json`,
`dist/pypi/phantomdep/pyproject.toml`, and `dist/homebrew/phantomdep.rb`).
Bump all four together, update `CHANGELOG.md`, merge to `main`.

## 1. Tag + GitHub Release (DONE for v1.0.0)

```sh
git tag -a v1.0.0 -m "PhantomDep v1.0.0 — first public release"
git push origin v1.0.0
```

Pushing a `v*.*.*` tag triggers `.github/workflows/release.yml`, which
matrix-builds 5 targets (linux x86_64/aarch64, macOS x86_64/aarch64,
windows x86_64), generates `.sha256` files, and publishes/updates the GitHub
Release with all assets. CI archives contain only the `phantomdep` binary.
Re-dispatch manually with `gh workflow run release.yml -f tag=vX.Y.Z`.
If the workflow ever fails, the manual fallback is:

```sh
mkdir -p /tmp/stage && cp target/release/phantomdep /tmp/stage/
tar -C /tmp/stage -czf phantomdep-aarch64-apple-darwin.tar.gz phantomdep
gh release create v1.0.0 --title "PhantomDep v1.0.0" \
  --notes-file <(sed -n '/^## \[1.0.0\]/,/^## \[/p' CHANGELOG.md) \
  phantomdep-aarch64-apple-darwin.tar.gz
# or: gh release upload v1.0.0 <asset> --clobber
```

Verify end-to-end like a customer would:

```sh
gh release download v1.0.0 -p 'phantomdep-aarch64-apple-darwin.tar.gz' -D /tmp/e2e
tar -xzf /tmp/e2e/phantomdep-aarch64-apple-darwin.tar.gz -C /tmp/e2e
/tmp/e2e/phantomdep --version
# real scan: a hallucinated import must come back PHANTOM / exit 2
printf 'import langchain_vectorstore_utils_pro\n' > /tmp/e2e/app.py
/tmp/e2e/phantomdep scan /tmp/e2e        # expect: ✗ PHANTOM, exit code 2
```

## 2. BLOCKED — repo visibility

The repo is currently **private**. Every binary-download install path below
(Homebrew formula URL, npm `install.js`, PyPI wheel bootstrap, GitHub Action)
fetches from `https://github.com/openintelligence-labs/phantomdep/releases/download/...`,
which returns 404 to unauthenticated customers until the repo is public.
Making it public is a deliberate launch decision — not part of this checklist.

## 3. BLOCKED — npm (needs `NPM_TOKEN`)

State as of 2026-07-29:

- The bare name `phantomdep` is **unclaimed** — `npm view phantomdep` → E404,
  `https://registry.npmjs.org/phantomdep` → `{"error":"Not found"}`. First
  publish wins; claim it early.
- The **`@phantomdep` scope is owned by a third party** — npm user
  `sibobbbbbb` (raditya0814@gmail.com) published `@phantomdep/blocklist`,
  `@phantomdep/cli`, `@phantomdep/core` v0.1.0 on 2026-06-18, backed by the
  unrelated TypeScript repo `github.com/sibobbbbbb/phantomdep`. We cannot use
  the `@phantomdep/...` scope. Do not squat back; if it matters, file an npm
  support / trademark dispute.

Once a token exists:

```sh
cd dist/npm
npm whoami                                  # sanity: correct account
npm publish --access public --dry-run       # inspect the file list first
npm publish --access public
# smoke test (requires public repo, see §2):
npm install -g phantomdep && phantomdep --version
```

`dist/npm/package.json` version must equal the workspace version — the
postinstall `install.js` downloads
`releases/download/v<version>/phantomdep-<target>.<tar.gz|zip>`.

## 4. BLOCKED — Homebrew tap (needs push access to `openintelligence-labs/homebrew-tap`)

`dist/homebrew/phantomdep.rb` is the canonical template. Before pushing,
fill in every `sha256` from the **final** release assets:

```sh
gh release download v1.0.0 -p '*.sha256' -D /tmp/shas && cat /tmp/shas/*.sha256
```

Then:

```sh
gh repo create openintelligence-labs/homebrew-tap --public   # once, if missing
git clone https://github.com/openintelligence-labs/homebrew-tap
mkdir -p homebrew-tap/Formula
cp dist/homebrew/phantomdep.rb homebrew-tap/Formula/phantomdep.rb
cd homebrew-tap && git add Formula/phantomdep.rb \
  && git commit -m "phantomdep 1.0.0" && git push
# customer path:
brew install openintelligence-labs/tap/phantomdep
```

## 5. BLOCKED — PyPI (needs `~/.pypirc` or `TWINE_PASSWORD` API token)

Nothing exists on PyPI yet (`pip index versions phantomdep` → not found).
`dist/pypi/phantomdep/` is a wrapper package that bundles the binary at
`phantomdep/bin/` per-platform.

```sh
cd dist/pypi/phantomdep
# stage the platform binary into the package before building:
mkdir -p src/phantomdep/bin && cp ../../../target/release/phantomdep src/phantomdep/bin/
python -m pip install --upgrade build twine
python -m build                              # per-platform wheel + sdist
python -m twine upload dist/*                # token: __token__ / pypi-...
# smoke test:
pip install phantomdep && phantomdep --version
```

For real per-platform wheels, run the build inside the release matrix (one
wheel per target with the right `--plat-name`) rather than on one machine.

## 6. crates.io — AUTOMATED via trusted publishing

Both `phantomdep-core` and `phantomdep` publish automatically on tag push:
the `publish-crates` job in release.yml uses crates.io trusted publishing
(configured on both crates: this repo / release.yml / `crates` environment —
no tokens). v1.0.1 was the manual bootstrap publish.

Manual fallback (requires `cargo login` as an owner):

```sh
cargo publish -p phantomdep-core --dry-run
cargo publish -p phantomdep-core
cargo publish -p phantomdep --dry-run
cargo publish -p phantomdep                  # after core is indexed
```

## 7. Post-release checklist

- [ ] All 5 release assets + `.sha256` files attached to the GitHub Release.
- [ ] Homebrew formula shas match the final assets (see §4).
- [ ] `phantomdep --version` from a freshly downloaded asset prints the new version.
- [ ] Real-scan e2e (§1) flags a hallucinated package with exit code 2.
- [ ] CHANGELOG has the version + date.
