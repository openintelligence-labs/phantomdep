"""Entry point: exec the bundled phantomdep binary, forwarding all args."""

from __future__ import annotations

import os
import sys
from importlib import resources


def main() -> None:
    """Locate the platform binary and exec it with the same argv."""
    binary = _resolve_binary()
    if binary is None:
        sys.stderr.write(
            "phantomdep: precompiled binary not bundled for this platform.\n"
            "Install from https://github.com/openintelligence-labs/phantomdep/releases\n"
            "or use Homebrew / cargo / npm.\n"
        )
        sys.exit(1)
    os.execv(str(binary), [str(binary), *sys.argv[1:]])


def _resolve_binary():
    name = "phantomdep.exe" if sys.platform == "win32" else "phantomdep"
    try:
        ref = resources.files("phantomdep") / "bin" / name
    except (ModuleNotFoundError, FileNotFoundError):
        return None
    if not ref.is_file():
        return None
    # Use as_file so we get a real path even if installed from a wheel.
    with resources.as_file(ref) as path:
        return path


if __name__ == "__main__":
    main()
