# PhantomDep for VS Code / Cursor / Windsurf

Catches hallucinated, squatted, and malicious packages **the moment your AI assistant types `import phantom-pkg`** — long before you run `pip install` or open a PR.

This extension wraps the [PhantomDep](https://github.com/openintelligence-labs/phantomdep) language server. It works in **VS Code**, **Cursor**, **Windsurf**, and any other VS Code fork.

## What it does

When you open or edit a Python / JavaScript / TypeScript file, PhantomDep:

1. Extracts every `import` / `require` / `from … import …` statement.
2. Validates each package against PyPI / npm / its registry.
3. Cross-references against the public Phantom-DB of known slop-squats.
4. **Squiggles** the bad ones with an evidence-backed message.
5. Offers **one-click quick-fixes** to replace the import with the real package.

```python
import requests              # ✓ real, no diagnostic
import yaml                  # ✓ resolves to pyyaml — real
import huggingface_cli       # 🛑 PHANTOM — did you mean huggingface_hub?
import super_fake_pkg_xyz    # 🛑 PHANTOM — package not on PyPI
```

## Install

1. Install the [`phantomdep`](https://github.com/openintelligence-labs/phantomdep#install) binary (Homebrew, `cargo install`, GitHub Releases, etc.).
2. Install this extension from the VS Code Marketplace or OpenVSX.
3. Reload your editor. Diagnostics appear automatically on Python / JS / TS files.

If `phantomdep` is not on your `$PATH`, set the **PhantomDep › Binary Path** setting.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `phantomdep.binaryPath` | `phantomdep` | Path to the phantomdep binary. |
| `phantomdep.phantomDbPath` | _(empty)_ | Override the bundled Phantom-DB with a local checkout. |
| `phantomdep.trace.server` | `off` | Trace LSP communication for debugging. |

## Commands

- **PhantomDep: Restart Language Server** — pick up a new binary or settings change.
- **PhantomDep: Run Doctor** — opens a terminal and runs `phantomdep doctor` to verify the install end-to-end.

## What it does NOT do

- It does not duplicate vulnerability scanning. For CVEs, pair it with [OSV-Scanner](https://github.com/google/osv-scanner) or Dependabot.
- It does not phone home, ever. No telemetry, no signup. The wire is the wire.

## License

MIT. Source: <https://github.com/openintelligence-labs/phantomdep/tree/main/extensions/vscode>.
