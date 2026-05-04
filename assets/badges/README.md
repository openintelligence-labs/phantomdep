# Badges

Hot-link these from your README to show your project gates dependencies through PhantomDep.

## Static SVG (recommended for the README)

```markdown
[![Protected by PhantomDep](https://raw.githubusercontent.com/openintelligence-labs/phantomdep/main/assets/badges/protected-by-phantomdep.svg)](https://github.com/openintelligence-labs/phantomdep)
```

Renders as: ![Protected by PhantomDep](./protected-by-phantomdep.svg)

## Shields.io equivalent (recolours dynamically)

```markdown
[![protected by PhantomDep](https://img.shields.io/badge/protected%20by-PhantomDep-7c3aed?logoColor=white)](https://github.com/openintelligence-labs/phantomdep)
```

## Use the badge if

- You run `phantomdep wrap` or the GitHub Action in CI on every PR.
- You install the Claude Code hook locally so AI-suggested installs are gated.
- You use the VS Code extension so hallucinations get caught at edit time.

## Don't use the badge if

- PhantomDep is installed but doesn't actually fail your CI on `BLOCK` verdicts.
- The Phantom-DB sync workflow is disabled.

The badge is a trust signal. Keep it honest.
