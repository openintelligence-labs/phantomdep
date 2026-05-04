# PhantomDep — convenience targets.
# Most contributors will use cargo / pytest / vsce directly; this file just
# wraps the common combinations into one-liners.

# Make sure cargo (installed by rustup at $HOME/.cargo/bin) is on PATH even
# when invoked from a non-login shell.
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: help build test bench demo demo-render demo-preview clean

# Default goal: print the available targets so a fresh contributor isn't lost.
help:
	@echo "PhantomDep make targets:"
	@echo "  build         — cargo build --release --workspace"
	@echo "  test          — cargo test + pytest"
	@echo "  bench         — phantomdep benchmark --iterations 100"
	@echo "  demo          — preview every demo in your terminal (alias for demo-preview)"
	@echo "  demo-preview  — run every assets/demos/run-demo.sh in sequence"
	@echo "  demo-render   — render every assets/demos/tapes/*.tape into a GIF (needs vhs)"
	@echo "  clean         — cargo clean + remove generated GIFs"

# ----- Core -----------------------------------------------------------------

build:
	cargo build --release --workspace

test:
	cargo test --workspace
	cd phantom-db/pipelines && python3 -m pytest tests/

bench: build
	./target/release/phantomdep benchmark --iterations 100

# ----- Demos ----------------------------------------------------------------

# Render every .tape into the matching GIF/MP4 in assets/demos/.
# Requires VHS — install with `brew install vhs`.
demo-render: build
	@command -v vhs >/dev/null 2>&1 || { \
	    echo "error: vhs not found. Install with 'brew install vhs'." >&2; exit 1; \
	}
	@for tape in assets/demos/tapes/*.tape; do \
	    echo "→ rendering $$tape"; \
	    vhs "$$tape"; \
	done

# Preview every demo as plain text in your terminal — no VHS required.
# Use this to verify the output before recording.
demo-preview: build
	./assets/demos/run-demo.sh all

# Default `make demo` previews; render only when explicitly asked.
demo: demo-preview

# ----- Cleanup --------------------------------------------------------------

clean:
	cargo clean
	rm -rf assets/demos/*.gif assets/demos/*.mp4
	cd phantom-db/pipelines && rm -rf candidates/ build/ *.egg-info
