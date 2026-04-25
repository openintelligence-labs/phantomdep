# PhantomDep

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> **Catch hallucinated dependencies before they hit your codebase.** AI coding tools frequently suggest packages that don't exist on PyPI/npm — or worse, *did* exist until a slop-squatter registered the name last week. PhantomDep scans your code and flags them in 10 seconds.

⭐ **Star us on GitHub** if you've ever `pip install`-ed a package that ChatGPT made up.

## Why this exists

Snyk and Dependabot check for *known vulnerabilities* in *real* packages. PhantomDep catches the opposite problem: *fake* packages that AI hallucinated, or recently-created packages squatting on commonly-hallucinated names. Zero existing tools address this.

## Quick start

```bash
pip install phantomdep
phantomdep scan ./
```

## Features

| Feature | What it does |
|---|---|
| Existence check | Verifies every imported package exists on its registry |
| Age check | Flags packages registered in the last 7 days |
| Popularity check | Flags packages with <100 downloads |
| Multi-ecosystem | PyPI + npm (more coming) |
| CI integration | SARIF output for GitHub code scanning |
| VS Code extension | Real-time warnings as you write code (planned) |

## Roadmap

- [x] Risk scoring engine
- [x] PyPI existence checks
- [ ] npm registry client
- [ ] Python import parser
- [ ] JS/TS import parser
- [ ] Crowdsourced database of known hallucinated names
- [ ] GitHub Action

## Part of the Open Intelligence Labs ecosystem

- [agentic-kit](https://github.com/openintelligence-labs/agentic-kit) — shared SDK
- [SlopShield](https://github.com/openintelligence-labs/slop-shield) — blocks AI-slop PRs
- [VibeLint](https://github.com/openintelligence-labs/vibelint) — lints AI code smells

## License

MIT
