# PhantomDep GitHub Action

Catches hallucinated, squatted, and malicious packages in pull requests, before they merge.

## Usage

```yaml
# .github/workflows/phantomdep.yml
name: PhantomDep
on:
  pull_request:
  push:
    branches: [main]

jobs:
  scan:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write   # required to upload SARIF
      pull-requests: write     # required to comment on PRs
    steps:
      - uses: actions/checkout@v4
      - uses: openintelligence-labs/phantomdep@v1
        with:
          fail-on: block
```

## Inputs

| Input | Default | Description |
|---|---|---|
| `path` | `.` | Path to scan (defaults to repository root). |
| `fail-on` | `block` | Fail the job at this level: `block`, `warn`, `never`. |
| `format` | `sarif` | Primary output format. The action also always produces markdown for PR comments. |
| `upload-sarif` | `true` | Upload SARIF to GitHub Code Scanning. |
| `comment-on-pr` | `true` | Post a sticky markdown comment on PRs. |
| `version` | `latest` | Pin a specific phantomdep release. |
| `concurrency` | `16` | Maximum concurrent registry lookups. |

## Outputs

| Output | Description |
|---|---|
| `sarif-path` | Path to the generated SARIF file. |
| `worst-action` | `allow`, `warn`, or `block`. |

## What it catches

| Verdict | Meaning | Default action |
|---|---|---|
| `PHANTOM` | Package not on registry — likely an LLM hallucination | block |
| `SQUATTED` | Hallucinated name registered to capture installs | block |
| `KNOWN_MALICIOUS` | Listed in OpenSSF malicious-packages | block |
| `INTERNAL_COLLISION` | Public name collides with a known internal name | block |
| `LOOKALIKE` | Edit distance ≤1 from a popular package | warn |
| `API_MISMATCH` | Package exists but doesn't export the symbols used | warn |

PhantomDep does not duplicate vulnerability scanning. For CVEs, pair it with [OSV-Scanner](https://github.com/google/osv-scanner) or Dependabot.
