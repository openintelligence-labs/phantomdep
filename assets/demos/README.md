# Demos

Recordable, deterministic terminal demos for the README, social posts, and the v1.0 launch.

## Render the GIFs

We use [Charm VHS](https://github.com/charmbracelet/vhs) for terminal recordings — it produces pixel-identical GIFs/MP4s from declarative `.tape` scripts, which means anyone can re-render the demo from source and get the same result.

```bash
brew install vhs
make demo-render            # renders every .tape into assets/demos/*.gif
```

Each tape outputs to `assets/demos/<name>.gif` (and `headline.tape` also produces an MP4 because some platforms prefer it).

## Preview without VHS

If you just want to see what the demo *will* show — say, to verify your local changes haven't broken the headline GIF — run the bash version:

```bash
make demo-preview                          # runs every demo in sequence
./assets/demos/run-demo.sh headline        # just the README GIF content
./assets/demos/run-demo.sh doctor
./assets/demos/run-demo.sh scan-polyglot
./assets/demos/run-demo.sh hook
./assets/demos/run-demo.sh replay
```

## What each demo shows

| Tape | Length | Story |
|---|---|---|
| **headline.tape** | ~15 s | The README GIF. `phantomdep wrap pip install …` blocks a phantom + suggests the real package. |
| **doctor.tape** | ~15 s | Every verdict class (PHANTOM, SQUATTED, LOOKALIKE, REAL) in one shot. |
| **scan-polyglot.tape** | ~12 s | One scan over a Python + TypeScript + Rust + Go fixture catches phantoms in all 4 ecosystems. |
| **hook.tape** | ~12 s | Claude Code PreToolUse hook intercepts an install command and returns the block JSON. |
| **replay.tape** | ~14 s | `phantomdep replay` shows the full Phantom-DB; `phantomdep benchmark` shows the warm-cache numbers. |

## Fixtures

`fixtures/` is a tiny mixed-ecosystem project the `scan-polyglot` and `headline` demos point at. **It intentionally contains hallucinated package names** — never depend on it from real code.

## Authoring guidance

- Keep each tape under 20 seconds. Short demos get re-shared; long ones get scrolled past.
- Use the same theme (`Dracula`) across tapes for a consistent look.
- Hide setup commands (`Hide … Show`) so the recording opens on a clean shell.
- After every `Type` that runs a meaningful command, add a `Sleep` long enough for the output to land — usually 2–6 seconds for a `phantomdep wrap` invocation.
- Run `./assets/demos/run-demo.sh <name>` first; if the output is wrong, fix the script before recording.
