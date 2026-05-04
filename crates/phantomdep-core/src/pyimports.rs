use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;

/// Top-level Python imports that are part of the standard library and should
/// never be flagged. Sourced from the CPython 3.13 module index.
static STDLIB: &[&str] = &[
    "__future__", "_thread", "abc", "aifc", "argparse", "array", "ast", "asynchat",
    "asyncio", "asyncore", "atexit", "audioop", "base64", "bdb", "binascii", "bisect",
    "builtins", "bz2", "calendar", "cgi", "cgitb", "chunk", "cmath", "cmd", "code",
    "codecs", "codeop", "collections", "colorsys", "compileall", "concurrent", "configparser",
    "contextlib", "contextvars", "copy", "copyreg", "crypt", "csv", "ctypes", "curses",
    "dataclasses", "datetime", "dbm", "decimal", "difflib", "dis", "distutils", "doctest",
    "email", "encodings", "ensurepip", "enum", "errno", "faulthandler", "fcntl", "filecmp",
    "fileinput", "fnmatch", "fractions", "ftplib", "functools", "gc", "genericpath",
    "getopt", "getpass", "gettext", "glob", "graphlib", "grp", "gzip", "hashlib", "heapq",
    "hmac", "html", "http", "idlelib", "imaplib", "imghdr", "imp", "importlib", "inspect",
    "io", "ipaddress", "itertools", "json", "keyword", "lib2to3", "linecache", "locale",
    "logging", "lzma", "mailbox", "mailcap", "marshal", "math", "mimetypes", "mmap",
    "modulefinder", "msilib", "msvcrt", "multiprocessing", "netrc", "nis", "nntplib",
    "ntpath", "numbers", "opcode", "operator", "optparse", "os", "ossaudiodev", "parser",
    "pathlib", "pdb", "pickle", "pickletools", "pipes", "pkgutil", "platform", "plistlib",
    "poplib", "posix", "posixpath", "pprint", "profile", "pstats", "pty", "pwd", "py_compile",
    "pyclbr", "pydoc", "pydoc_data", "pyexpat", "queue", "quopri", "random", "re", "readline",
    "reprlib", "resource", "rlcompleter", "runpy", "sched", "secrets", "select", "selectors",
    "shelve", "shlex", "shutil", "signal", "site", "smtpd", "smtplib", "sndhdr", "socket",
    "socketserver", "spwd", "sqlite3", "ssl", "stat", "statistics", "string", "stringprep",
    "struct", "subprocess", "sunau", "symbol", "symtable", "sys", "sysconfig", "syslog",
    "tabnanny", "tarfile", "telnetlib", "tempfile", "termios", "test", "textwrap", "threading",
    "time", "timeit", "tkinter", "token", "tokenize", "tomllib", "trace", "traceback",
    "tracemalloc", "tty", "turtle", "turtledemo", "types", "typing", "unicodedata",
    "unittest", "urllib", "uu", "uuid", "venv", "warnings", "wave", "weakref", "webbrowser",
    "winreg", "winsound", "wsgiref", "xdrlib", "xml", "xmlrpc", "zipapp", "zipfile",
    "zipimport", "zlib", "zoneinfo",
];

/// Common import-name → PyPI-name remappings. These are the cases where
/// `import x` does not map directly to `pip install x`.
static IMPORT_TO_PYPI: &[(&str, &str)] = &[
    ("yaml", "pyyaml"),
    ("PIL", "pillow"),
    ("cv2", "opencv-python"),
    ("sklearn", "scikit-learn"),
    ("skimage", "scikit-image"),
    ("bs4", "beautifulsoup4"),
    ("dateutil", "python-dateutil"),
    ("dotenv", "python-dotenv"),
    ("magic", "python-magic"),
    ("jose", "python-jose"),
    ("levenshtein", "python-levenshtein"),
    ("OpenSSL", "pyopenssl"),
    ("Crypto", "pycryptodome"),
    ("MySQLdb", "mysqlclient"),
    ("psycopg2", "psycopg2-binary"),
    ("ldap", "python-ldap"),
    ("serial", "pyserial"),
    ("git", "gitpython"),
    ("speech_recognition", "SpeechRecognition"),
    ("attr", "attrs"),
    ("google", "google-api-python-client"),
    ("tensorflow_hub", "tensorflow-hub"),
];

static IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches:
    //   import foo
    //   import foo as bar
    //   import foo, baz
    //   import foo.bar
    //   from foo import x
    //   from foo.bar import x
    // Skips relative imports (`from .x import y`, `from ..x import y`).
    Regex::new(
        r"(?m)^[\t ]*(?:from[\t ]+(?P<from>[A-Za-z_][A-Za-z0-9_.]*)[\t ]+import|import[\t ]+(?P<imp>[A-Za-z_][A-Za-z0-9_.]*(?:[\t ]*,[\t ]*[A-Za-z_][A-Za-z0-9_.]*)*))",
    )
    .expect("import regex compiles")
});

/// Extract third-party top-level package names from Python source.
/// Returns the set of *PyPI package names* (after import → pypi remapping).
pub fn extract_pypi_packages(source: &str) -> BTreeSet<String> {
    let mut top_modules: BTreeSet<String> = BTreeSet::new();

    let stripped = strip_strings_and_comments(source);

    for cap in IMPORT_RE.captures_iter(&stripped) {
        if let Some(from_match) = cap.name("from") {
            let module = from_match.as_str();
            if let Some(top) = top_module(module) {
                top_modules.insert(top.to_string());
            }
        }
        if let Some(imp_match) = cap.name("imp") {
            for piece in imp_match.as_str().split(',') {
                let piece = piece.trim();
                let name = piece.split(|c: char| c.is_whitespace()).next().unwrap_or("");
                if let Some(top) = top_module(name) {
                    top_modules.insert(top.to_string());
                }
            }
        }
    }

    top_modules
        .into_iter()
        .filter(|m| !STDLIB.iter().any(|s| s == m))
        .map(|m| import_to_pypi(&m))
        .collect()
}

fn top_module(module: &str) -> Option<&str> {
    let trimmed = module.trim();
    if trimmed.is_empty() || trimmed.starts_with('.') {
        return None;
    }
    Some(trimmed.split('.').next().unwrap_or(trimmed))
}

fn import_to_pypi(name: &str) -> String {
    for (k, v) in IMPORT_TO_PYPI {
        if *k == name {
            return v.to_string();
        }
    }
    name.to_string()
}

/// Strip Python string literals and comments so they don't produce false-positive imports.
/// Conservative: handles `# comment`, `"..."`, `'...'`, triple-quoted strings.
fn strip_strings_and_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // line comment
        if c == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // triple-quoted strings
        if (c == b'"' || c == b'\'') && i + 2 < bytes.len() && bytes[i + 1] == c && bytes[i + 2] == c {
            let quote = c;
            i += 3;
            while i + 2 < bytes.len() {
                if bytes[i] == quote && bytes[i + 1] == quote && bytes[i + 2] == quote {
                    i += 3;
                    break;
                }
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }
        // single-line string
        if c == b'"' || c == b'\'' {
            let quote = c;
            i += 1;
            while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &str) -> Vec<String> {
        extract_pypi_packages(s).into_iter().collect()
    }

    #[test]
    fn finds_simple_imports() {
        let pkgs = extract("import requests\nimport numpy\n");
        assert_eq!(pkgs, vec!["numpy", "requests"]);
    }

    #[test]
    fn handles_from_imports() {
        let pkgs = extract("from fastapi import FastAPI\nfrom pydantic import BaseModel\n");
        assert_eq!(pkgs, vec!["fastapi", "pydantic"]);
    }

    #[test]
    fn collapses_dotted_to_top_module() {
        let pkgs = extract("from sklearn.ensemble import RandomForestClassifier\n");
        assert_eq!(pkgs, vec!["scikit-learn"]);
    }

    #[test]
    fn skips_stdlib() {
        let pkgs = extract("import os\nimport sys\nimport json\nimport requests\n");
        assert_eq!(pkgs, vec!["requests"]);
    }

    #[test]
    fn skips_relative_imports() {
        let pkgs = extract("from . import foo\nfrom ..bar import baz\nimport requests\n");
        assert_eq!(pkgs, vec!["requests"]);
    }

    #[test]
    fn applies_import_to_pypi_remap() {
        let pkgs = extract("import yaml\nfrom PIL import Image\nimport cv2\n");
        let mut sorted = pkgs;
        sorted.sort();
        assert_eq!(sorted, vec!["opencv-python", "pillow", "pyyaml"]);
    }

    #[test]
    fn handles_multi_import_on_one_line() {
        let pkgs = extract("import requests, numpy\n");
        assert_eq!(pkgs, vec!["numpy", "requests"]);
    }

    #[test]
    fn handles_import_as() {
        let pkgs = extract("import numpy as np\nimport pandas as pd\n");
        assert_eq!(pkgs, vec!["numpy", "pandas"]);
    }

    #[test]
    fn ignores_imports_inside_strings_and_comments() {
        let pkgs = extract(
            r#"
# import requests
"""
import torch
"""
import numpy
"#,
        );
        assert_eq!(pkgs, vec!["numpy"]);
    }

    #[test]
    fn handles_indented_imports() {
        let pkgs = extract(
            r#"
def f():
    import requests
    from pandas import DataFrame
"#,
        );
        let mut s = pkgs;
        s.sort();
        assert_eq!(s, vec!["pandas", "requests"]);
    }
}
