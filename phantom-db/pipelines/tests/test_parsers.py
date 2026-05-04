from phantomdep_pipelines.parsers import (
    extract_js_packages,
    extract_python_packages,
    normalize_pypi_name,
)


def test_pep503_normalisation_collapses_separators():
    assert normalize_pypi_name("Super_Fast_Parser") == "super-fast-parser"
    assert normalize_pypi_name("super-fast-parser") == "super-fast-parser"
    assert normalize_pypi_name("super.fast.parser") == "super-fast-parser"
    assert normalize_pypi_name("super___fast--parser") == "super-fast-parser"
    assert normalize_pypi_name("requests") == "requests"
    assert normalize_pypi_name("") == ""


def test_python_collapses_underscore_and_hyphen_variants():
    text = (
        "```bash\npip install super-fast-json-parser\n```\n"
        "```python\nimport super_fast_json_parser\n```"
    )
    pkgs = extract_python_packages(text)
    assert pkgs == {"super-fast-json-parser"}



# ----- Python --------------------------------------------------------------


def test_python_finds_pip_install_args():
    text = "Use this:\n```bash\npip install requests fastapi\n```"
    pkgs = extract_python_packages(text)
    assert pkgs == {"requests", "fastapi"}


def test_python_strips_pep508_versions():
    text = "```bash\npip install requests==2.31.0 fastapi[all]>=0.110\n```"
    pkgs = extract_python_packages(text)
    assert pkgs == {"requests", "fastapi"}


def test_python_finds_uv_add():
    text = "Run `uv add httpx pydantic` to install."
    pkgs = extract_python_packages(text)
    assert pkgs == {"httpx", "pydantic"}


def test_python_finds_imports():
    text = "```python\nimport requests\nfrom fastapi import FastAPI\n```"
    pkgs = extract_python_packages(text)
    assert pkgs == {"requests", "fastapi"}


def test_python_skips_stdlib():
    text = "```python\nimport os\nimport sys\nimport requests\n```"
    pkgs = extract_python_packages(text)
    assert pkgs == {"requests"}


def test_python_remaps_yaml_to_pyyaml():
    text = "```python\nimport yaml\n```"
    pkgs = extract_python_packages(text)
    assert pkgs == {"pyyaml"}


def test_python_skips_paths_and_urls():
    text = "```bash\npip install ./local-pkg https://x.com/y.whl git+https://gh.com/x/y\n```"
    pkgs = extract_python_packages(text)
    assert pkgs == set()


# ----- JavaScript ---------------------------------------------------------


def test_js_finds_npm_install():
    text = "```bash\nnpm install react @anthropic-ai/sdk\n```"
    pkgs = extract_js_packages(text)
    assert pkgs == {"react", "@anthropic-ai/sdk"}


def test_js_strips_npm_versions():
    text = "```bash\nnpm i react@^18 zod@latest @types/node@20\n```"
    pkgs = extract_js_packages(text)
    assert pkgs == {"react", "zod", "@types/node"}


def test_js_finds_imports():
    text = "```typescript\nimport React from 'react';\nimport { z } from 'zod';\n```"
    pkgs = extract_js_packages(text)
    assert pkgs == {"react", "zod"}


def test_js_finds_require():
    text = "```javascript\nconst _ = require('lodash');\n```"
    pkgs = extract_js_packages(text)
    assert pkgs == {"lodash"}


def test_js_skips_node_builtins():
    text = "```javascript\nconst fs = require('fs');\nconst ax = require('axios');\n```"
    pkgs = extract_js_packages(text)
    assert pkgs == {"axios"}


def test_js_collapses_subpath():
    text = "```typescript\nimport { x } from '@anthropic-ai/sdk/resources/messages';\n```"
    pkgs = extract_js_packages(text)
    assert pkgs == {"@anthropic-ai/sdk"}
