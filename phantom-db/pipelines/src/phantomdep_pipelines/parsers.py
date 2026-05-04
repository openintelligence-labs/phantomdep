"""Extract candidate package names from raw LLM output.

The LLM responses we get back are markdown + code blocks + prose. We don't
re-implement the full Rust parser here — we just need to surface the *package
names* the model wants you to install. We use registry-shaped heuristics:

- Python: `pip install foo`, `import foo`, `from foo import …`.
- JavaScript: `npm install foo`, `import x from 'foo'`, `require('foo')`.

The output is the union of distinct names per language.
"""

from __future__ import annotations

import re
from collections.abc import Iterable

# ----------------------------------------------------------------------------
# Python-specific patterns
# ----------------------------------------------------------------------------

PIP_INSTALL_RE = re.compile(
    r"\bpip(?:3)?\s+install\s+(?P<args>[^\n`]+)", re.IGNORECASE
)
UV_PIP_RE = re.compile(r"\buv\s+pip\s+install\s+(?P<args>[^\n`]+)", re.IGNORECASE)
UV_ADD_RE = re.compile(r"\buv\s+add\s+(?P<args>[^\n`]+)", re.IGNORECASE)
POETRY_ADD_RE = re.compile(r"\bpoetry\s+add\s+(?P<args>[^\n`]+)", re.IGNORECASE)

# `import foo` / `from foo import …` (top-level module only).
PY_IMPORT_RE = re.compile(
    r"(?m)^[\t ]*"
    r"(?:from\s+(?P<from_>[A-Za-z_][A-Za-z0-9_.]*)\s+import"
    r"|import\s+(?P<imp>[A-Za-z_][A-Za-z0-9_.]*(?:\s*,\s*[A-Za-z_][A-Za-z0-9_.]*)*))"
)

PYTHON_STDLIB = frozenset(
    {
        "abc", "argparse", "array", "asyncio", "base64", "collections", "concurrent",
        "contextlib", "copy", "csv", "dataclasses", "datetime", "decimal", "enum",
        "functools", "glob", "hashlib", "http", "io", "ipaddress", "itertools",
        "json", "logging", "math", "multiprocessing", "operator", "os", "pathlib",
        "pickle", "queue", "random", "re", "secrets", "shutil", "signal", "socket",
        "sqlite3", "ssl", "statistics", "string", "struct", "subprocess", "sys",
        "tempfile", "textwrap", "threading", "time", "tomllib", "traceback", "typing",
        "unittest", "urllib", "uuid", "warnings", "weakref", "xml", "zipfile", "zlib",
        "__future__", "_thread",
    }
)

IMPORT_TO_PYPI: dict[str, str] = {
    "yaml": "pyyaml",
    "PIL": "pillow",
    "cv2": "opencv-python",
    "sklearn": "scikit-learn",
    "skimage": "scikit-image",
    "bs4": "beautifulsoup4",
    "dateutil": "python-dateutil",
    "dotenv": "python-dotenv",
    "magic": "python-magic",
    "jose": "python-jose",
    "OpenSSL": "pyopenssl",
    "Crypto": "pycryptodome",
    "MySQLdb": "mysqlclient",
    "psycopg2": "psycopg2-binary",
    "ldap": "python-ldap",
    "serial": "pyserial",
    "git": "gitpython",
    "speech_recognition": "SpeechRecognition",
    "attr": "attrs",
}


def normalize_pypi_name(name: str) -> str:
    """PEP 503: lowercase + collapse runs of `-_.` to a single `-`.

    PyPI treats `Super_Fast_Parser` and `super-fast-parser` as the same
    project, so we normalise to the canonical form before any lookup or diff.
    """
    if not name:
        return name
    out = []
    last_dash = False
    for c in name.lower():
        if c in "-_.":
            if not last_dash:
                out.append("-")
                last_dash = True
        else:
            out.append(c)
            last_dash = False
    return "".join(out).strip("-")


def extract_python_packages(text: str) -> set[str]:
    """Return distinct PyPI distribution names suggested in LLM `text`.

    Names are normalised per PEP 503 so `Super-Fast-Parser` and
    `super_fast_parser` collapse to a single canonical form.
    """
    names: set[str] = set()

    for regex in (PIP_INSTALL_RE, UV_PIP_RE, UV_ADD_RE, POETRY_ADD_RE):
        for m in regex.finditer(text):
            for tok in _tokenise_install_args(m.group("args")):
                pep_name = _strip_pep508(tok)
                if pep_name:
                    names.add(normalize_pypi_name(pep_name))

    for m in PY_IMPORT_RE.finditer(text):
        if m.group("from_"):
            top = m.group("from_").split(".")[0]
            if top and top not in PYTHON_STDLIB:
                names.add(normalize_pypi_name(IMPORT_TO_PYPI.get(top, top)))
        if m.group("imp"):
            for piece in m.group("imp").split(","):
                top = piece.strip().split()[0].split(".")[0]
                if top and top not in PYTHON_STDLIB:
                    names.add(normalize_pypi_name(IMPORT_TO_PYPI.get(top, top)))

    return names


def _tokenise_install_args(args: str) -> Iterable[str]:
    for tok in args.split():
        tok = tok.strip().rstrip(",;.\"'`)")
        if not tok or tok.startswith("-"):
            continue
        if tok in {"install", "add"}:
            continue
        # Skip path/URL/VCS specifiers, but be careful: `httpx` and `httplib`
        # are real package names that start with `http`. We only reject the
        # `http://` and `https://` URL forms (which contain `://`), plus the
        # explicit prefixes used by package managers for non-registry installs.
        if tok.startswith(("./", "../", "/", "git+")):
            continue
        if "://" in tok:
            continue
        yield tok


def _strip_pep508(spec: str) -> str | None:
    s = spec.strip()
    if not s:
        return None
    # Cut at PEP 508 separators.
    cut_idx = len(s)
    for ch in "=><!~;@ ":
        i = s.find(ch)
        if i != -1 and i < cut_idx:
            cut_idx = i
    head = s[:cut_idx]
    name = head.split("[", 1)[0].strip()
    return name or None


# ----------------------------------------------------------------------------
# JavaScript-specific patterns
# ----------------------------------------------------------------------------

NPM_INSTALL_RE = re.compile(
    r"\b(?:npm\s+(?:install|i|add)|pnpm\s+(?:install|i|add)|yarn\s+(?:add|install))\s+(?P<args>[^\n`]+)",
    re.IGNORECASE,
)
JS_IMPORT_FROM_RE = re.compile(
    r"""\bimport\b[^'"`;\n]*?\bfrom\s*['"`]([^'"`\n]+)['"`]"""
)
JS_SIDE_EFFECT_RE = re.compile(r"""(?m)^\s*import\s*['"`]([^'"`\n]+)['"`]""")
JS_REQUIRE_RE = re.compile(r"""\brequire\s*\(\s*['"`]([^'"`\n]+)['"`]""")

NODE_BUILTINS = frozenset(
    {
        "assert", "buffer", "child_process", "cluster", "crypto", "dgram", "dns",
        "events", "fs", "http", "http2", "https", "net", "os", "path", "process",
        "querystring", "readline", "stream", "string_decoder", "tls", "tty", "url",
        "util", "v8", "vm", "wasi", "worker_threads", "zlib",
    }
)


def extract_js_packages(text: str) -> set[str]:
    """Return distinct npm package names suggested in LLM `text`."""
    names: set[str] = set()

    for m in NPM_INSTALL_RE.finditer(text):
        for tok in _tokenise_install_args(m.group("args")):
            pkg = _strip_npm_version(tok)
            if pkg:
                names.add(pkg)

    for regex in (JS_IMPORT_FROM_RE, JS_SIDE_EFFECT_RE, JS_REQUIRE_RE):
        for m in regex.finditer(text):
            spec = m.group(1).strip()
            pkg = _specifier_to_package(spec)
            if pkg:
                names.add(pkg)

    return {n for n in names if n not in NODE_BUILTINS}


def _strip_npm_version(spec: str) -> str | None:
    s = spec.strip()
    if not s:
        return None
    if s.startswith("@"):
        rest = s[1:]
        parts = rest.split("/", 1)
        if len(parts) < 2 or not parts[0] or not parts[1]:
            return None
        scope, pkg_with_ver = parts
        pkg = pkg_with_ver.split("@", 1)[0]
        if not pkg:
            return None
        return f"@{scope}/{pkg}"
    head = s.split("@", 1)[0]
    return head or None


def _specifier_to_package(spec: str) -> str | None:
    s = spec.strip()
    if not s or s.startswith((".", "/")) or s.startswith("node:") or "://" in s:
        return None
    if s.startswith("@"):
        rest = s[1:]
        parts = rest.split("/", 2)
        if len(parts) < 2 or not parts[0] or not parts[1]:
            return None
        return f"@{parts[0]}/{parts[1]}"
    return s.split("/", 1)[0] or None


# ----------------------------------------------------------------------------
# Public dispatcher
# ----------------------------------------------------------------------------


def extract_packages(text: str, language: str) -> set[str]:
    """Dispatch to the right extractor based on language."""
    if language == "python":
        return extract_python_packages(text)
    if language in {"javascript", "typescript"}:
        return extract_js_packages(text)
    raise ValueError(f"unsupported language: {language}")
