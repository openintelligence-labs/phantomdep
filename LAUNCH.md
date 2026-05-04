# Launch checklist

Everything in this repo is **release-ready** as of v1.0.0. The remaining work is the publishing dance — steps that require credentials, GitHub org access, or registry accounts that the maintainer has and the build pipeline doesn't.

This is a one-pager so you can run the launch in an afternoon. **Read it once before starting** — there's an order to it (the GitHub Release must exist before npm/Homebrew/PyPI can fetch the binaries).

---

## 0. Pre-flight (~10 min)

```bash
# Verify the workspace is in the state this doc assumes.
make build
make test
cargo clippy --workspace --all-targets -- -D warnings
make demo-preview     # confirms every demo runs end-to-end against live registries
```

Expected: 128 Rust + 22 Python tests pass, clippy exits 0, every demo prints the expected output.

**Also confirm you have credentials for everything in [§ "What I cannot delegate"](#what-i-the-maintainer-cannot-delegate). Easier to find out now than at step 7.**

---

## 1. Create the public GitHub repo (~5 min)

The codebase currently lives inside the OilLabs monorepo at `openintelligence-labs/<monorepo>/phantomdep/`. For the public launch we want a standalone repo at `github.com/openintelligence-labs/phantomdep`.

> **Foot-gun:** do NOT `git init` inside the monorepo's `phantomdep/` subdirectory — that creates a nested repo inside the parent's `.git/` and confuses every git tool. Copy the directory out first.

```bash
# Copy the project out of the monorepo to a fresh location.
cp -R path/to/openintelligence-labs-monorepo/phantomdep ~/projects/phantomdep
cd ~/projects/phantomdep

# Create the public repo on GitHub.
gh repo create openintelligence-labs/phantomdep \
    --public \
    --description "Local-first dependency firewall for AI coding agents. Catches hallucinated, squatted, and malicious packages before they reach your repo." \
    --homepage "https://github.com/openintelligence-labs/phantomdep"

# Initialise + push.
git init
git add .
git commit -m "chore: initial public release v1.0.0"
git remote add origin git@github.com:openintelligence-labs/phantomdep.git
git branch -M main
git push -u origin main
```

**Don't squash** the v1.0.0 commit — the CHANGELOG references the version-by-version evolution.

---

## 2. Cut the v1.0.0 git tag (~2 min, then ~10 min waiting for CI)

```bash
git tag -a v1.0.0 -m "PhantomDep v1.0.0 — first public release"
git push --tags
```

This triggers `.github/workflows/release.yml`, which builds 5 binaries (linux x86_64/aarch64, macOS x86_64/aarch64, windows x86_64) with sha256 checksums and publishes them to GitHub Releases.

**Wait until the release shows up at https://github.com/openintelligence-labs/phantomdep/releases/tag/v1.0.0 before continuing.** Steps 4, 5, and 6 download from this URL.

```bash
# Sanity-check the published release before depending on it.
gh release view v1.0.0 --repo openintelligence-labs/phantomdep
```

---

## 3. Publish to crates.io (~3 min)

```bash
cargo login                                  # paste your crates.io token
cargo publish -p phantomdep-core
sleep 60                                     # let the index update
cargo publish -p phantomdep-cli
```

`phantomdep-core` must be live on crates.io before `phantomdep-cli` will publish, since the CLI crate depends on the published version.

---

## 4. Cold-install verification (~10 min)

**Before publishing to npm / PyPI / Homebrew, prove the install path actually works for a fresh user.**

```bash
# In a clean Docker container with no PhantomDep state:
docker run --rm -it ubuntu:24.04 bash
# inside the container:
apt-get update && apt-get install -y curl ca-certificates
curl -sSfL https://github.com/openintelligence-labs/phantomdep/releases/latest/download/phantomdep-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xz
./phantomdep --version           # expect: phantomdep 1.0.0
./phantomdep doctor              # expect: 4 verdicts, exit 2

# In a clean macOS shell with cargo installed:
cargo install --locked phantomdep-cli --version 1.0.0
phantomdep doctor
```

If either of these fails, **stop and fix before continuing** — npm/PyPI/Homebrew users will hit the same problem at scale.

---

## 5. Publish to PyPI (~30–60 min, real work)

The `dist/pypi/phantomdep/` skeleton vendors the binary inside `phantomdep/bin/`. We need one wheel per (os, arch).

**Honest assessment**: there is no clean one-liner for this. The reliable path is a GitHub Actions workflow that downloads the GitHub Release artefacts, bundles each into a per-platform wheel, and uploads with `twine`. Until that workflow exists:

- **Option A (recommended for launch day): defer PyPI publishing to v1.1.** `pip install phantomdep` won't work on day one, but `cargo install` and `curl | tar` do. The README already gates the `pip install` line behind a "Coming with v1.0 launch" note (move the gate to "v1.1" if you defer).
- **Option B (do it manually before launch):** for each of the 4 supported platforms, download the binary, drop it into `dist/pypi/phantomdep/src/phantomdep/bin/`, build the wheel with the right platform tag using `setuptools` directly, then `twine upload`. This is fiddly and easy to mess up; only do it if you have an hour and a Python packaging-savvy day.
- **Open follow-up issue:** "Automate PyPI wheel publishing via cibuildwheel".

---

## 6. Publish to npm (~3 min)

```bash
cd dist/npm
npm login
npm publish --access public
```

The `postinstall` script in `dist/npm/install.js` downloads the right binary from the GitHub Release on user install. **Make sure the GitHub Release exists first** (step 2).

---

## 7. Create the Homebrew tap (~10 min)

```bash
# Get the SHAs from the published release.
for target in x86_64-apple-darwin aarch64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
    echo "$target:"
    curl -sSfL "https://github.com/openintelligence-labs/phantomdep/releases/download/v1.0.0/phantomdep-${target}.tar.gz.sha256"
done

# Create the tap repo (one-time).
gh repo create openintelligence-labs/homebrew-tap --public

# Clone the tap, copy the formula, paste in the SHAs.
git clone git@github.com:openintelligence-labs/homebrew-tap.git
cd homebrew-tap
mkdir -p Formula
cp /path/to/phantomdep/dist/homebrew/phantomdep.rb Formula/phantomdep.rb
$EDITOR Formula/phantomdep.rb       # paste the SHAs into the REPLACE_WITH_*_SHA256 placeholders

git add . && git commit -m "phantomdep: 1.0.0"
git push origin main

# Verify the install on a fresh shell:
brew install openintelligence-labs/tap/phantomdep
phantomdep --version    # should print: phantomdep 1.0.0
```

---

## 8. Publish the VS Code extension (~10 min)

```bash
cd extensions/vscode

# Marketplace publisher account: https://aka.ms/vscode-create-publisher
# OpenVSX publisher account: https://open-vsx.org/

vsce login openintelligence-labs           # one-time, prompts for PAT
vsce publish

ovsx login                                  # one-time
ovsx publish phantomdep-1.0.0.vsix
```

The extension is already fully packaged at `extensions/vscode/phantomdep-1.0.0.vsix` (439 KB with vendored deps + icon).

---

## 9. Enable the Phantom-DB sync workflows (~5 min, maybe a 24h wait)

The verifier (hourly) and feeder (nightly) workflows already exist:

- `.github/workflows/phantomdb-verifier.yml`
- `.github/workflows/phantomdb-feeder.yml`

They become eligible to run as soon as the repo is public. **Caveat:** GitHub sometimes delays first-time runs of cron workflows on brand-new public repos by up to 24 hours. Force a manual run to confirm both are working:

```bash
gh workflow run phantomdb-verifier --repo openintelligence-labs/phantomdep
gh workflow run phantomdb-feeder --repo openintelligence-labs/phantomdep --field provider=mock

# Watch for completion.
gh run list --repo openintelligence-labs/phantomdep --limit 5
```

---

## 10. Announce (~30 min on the day, ~1 week of pitching beforehand)

The [architecture's launch playbook](./docs/ARCHITECTURE.md#14-distribution-and-launch) has the full channel list. Suggested order for launch day:

1. **Show HN**: `Show HN: PhantomDep — catches packages your AI made up, before you install them` with the headline GIF inline.
2. **r/LocalLLaMA**: angle on "I built a tool that catches packages your local LLM made up."
3. **r/programming**, **r/Python**, **r/javascript**, **r/rust**, **r/golang**, **r/selfhosted**.
4. **TLDR Sec**, **TLDR AI**, **Last Week in AI**, **Pointer.io**, **Console.dev** — pitch a week ahead, not on launch day.
5. **dev.to** + **Lobste.rs**.
6. **Bluesky** + **X** with the headline GIF.

The first quarterly **"State of LLM Package Hallucination"** report can come 30 days after launch — by then the feeder pipeline has enough Phantom-DB candidate volume to make the report substantive.

---

## What I (the maintainer) cannot delegate

Every step above requires one of:

- A GitHub org account that owns `openintelligence-labs/`.
- A crates.io account with publish rights for `phantomdep-core` and `phantomdep-cli`.
- A PyPI account with publish rights for `phantomdep` *(only if doing step 5)*.
- An npm account with publish rights for `phantomdep`.
- A VS Code Marketplace publisher account (`openintelligence-labs`).
- An OpenVSX publisher account.

If any of these don't exist yet, **create them first**. The ones that take the longest:

- **VS Code Marketplace publisher account** — needs an Azure DevOps PAT scoped to "Marketplace > Manage". Allow a day if it's your first time.
- **PyPI name reservation** — file a name-reservation request on PyPI **before launch day** so a slop-squatter doesn't take `phantomdep` while you're building wheels. (Yes, the irony of this tool needing slop-squat protection for its own name is on the nose.)
- **Homebrew tap** — needs a separate GitHub repo (`openintelligence-labs/homebrew-tap`) created via step 7.

---

## Rollback plan

If something goes catastrophically wrong on launch day:

1. **Yank the bad release**: `cargo yank --vers 1.0.0 phantomdep-cli` and `npm unpublish phantomdep@1.0.0` (within 72 hours of publish — npm refuses unpublish after that).
2. **Mark the GitHub Release as a pre-release** so the `/latest` redirect doesn't resolve to it.
3. **Revert the Homebrew formula** by pushing a commit to the tap that removes the `phantomdep.rb` file (or pins to a known-good prior version once one exists).
4. **Pin all README / asset links to a known-good commit SHA** so external embedders don't see anything broken while you fix forward.

---

## After launch

- **First 72 hours:** watch GitHub Issues. Most "this didn't work" reports are install-path bugs that take a 1-line PR.
- **Thank early stargazers publicly.** Don't tag individuals without their consent — a single "thanks for the early stars" post in your launch thread is plenty.
- **First weekly Phantom-DB additions post:** day 7.
- **First quarterly "State of LLM Package Hallucination" report:** day 30.
- **Star count is not a goal.** Per architecture §13, the metrics that matter are slopsquat captures, replacement events, and Phantom-DB integration. Don't optimise for what you can't trust.

---

> **What this checklist proves:** the engineering is done. The launch is an operational task, not a build task.
