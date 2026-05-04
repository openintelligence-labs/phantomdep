//! Parse package-manager install command lines into (Ecosystem, package names).
//!
//! Each parser is a pure function taking `args` (everything after the program name)
//! and returning a `ParsedInstall`. We intentionally keep these regex-light and
//! conservative: when in doubt we *include* a name so PhantomDep checks it,
//! rather than miss a slop-squat.

use crate::verdict::Ecosystem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedInstall {
    pub ecosystem: Ecosystem,
    pub manager: Manager,
    /// Package names to validate (already version-stripped, normalized).
    pub packages: Vec<String>,
    /// Requirements files referenced by the command (`-r req.txt`, etc.).
    /// Caller is expected to read these and re-parse.
    pub requirement_files: Vec<String>,
    /// True if this command line did not actually install anything we recognize
    /// (e.g. `pip install --upgrade pip` with no positional args, or a
    /// `pip install ./local/path`).
    pub no_packages: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Pip,
    Uv,
    Poetry,
    Npm,
    Pnpm,
    Yarn,
    Cargo,
    Go,
}

impl Manager {
    pub fn ecosystem(self) -> Ecosystem {
        match self {
            Self::Pip | Self::Uv | Self::Poetry => Ecosystem::Pypi,
            Self::Npm | Self::Pnpm | Self::Yarn => Ecosystem::Npm,
            Self::Cargo => Ecosystem::Cargo,
            Self::Go => Ecosystem::Go,
        }
    }

    pub fn from_program(program: &str) -> Option<Self> {
        let basename = std::path::Path::new(program)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(program);
        match basename {
            "pip" | "pip3" => Some(Self::Pip),
            "uv" => Some(Self::Uv),
            "poetry" => Some(Self::Poetry),
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "yarn" => Some(Self::Yarn),
            "cargo" => Some(Self::Cargo),
            "go" => Some(Self::Go),
            _ => None,
        }
    }
}

pub fn parse(manager: Manager, args: &[String]) -> ParsedInstall {
    match manager {
        Manager::Pip => parse_pip(args),
        Manager::Uv => parse_uv(args),
        Manager::Poetry => parse_poetry(args),
        Manager::Npm => parse_npm_like(Manager::Npm, args, &["install", "i", "add"]),
        Manager::Pnpm => parse_npm_like(Manager::Pnpm, args, &["install", "i", "add"]),
        Manager::Yarn => parse_npm_like(Manager::Yarn, args, &["add", "install"]),
        Manager::Cargo => parse_cargo(args),
        Manager::Go => parse_go(args),
    }
}

// ----------------------------------------------------------------------------
// Pip
// ----------------------------------------------------------------------------
//
// pip install [options] <pkg>...
// pip install -r req.txt
// flags that take a value: -r/--requirement, -c/--constraint, -e/--editable,
//                          --index-url, --extra-index-url, --find-links, -t/--target,
//                          --root, --prefix, -d/--dest, --src, --upgrade-strategy,
//                          --report, --log, --proxy, --cert, --client-cert
fn parse_pip(args: &[String]) -> ParsedInstall {
    let value_flags = &[
        "-r", "--requirement", "-c", "--constraint", "-e", "--editable",
        "--index-url", "--extra-index-url", "--find-links", "-t", "--target",
        "--root", "--prefix", "-d", "--dest", "--src", "--upgrade-strategy",
        "--report", "--log", "--proxy", "--cert", "--client-cert", "--python-version",
        "--platform", "--implementation", "--abi", "--no-binary", "--only-binary",
        "--config-settings",
    ];
    
    parse_with_subcommand(
        Manager::Pip,
        args,
        &["install"],
        value_flags,
        &["-r", "--requirement"],
        strip_pep508_version,
        is_pip_specifier_special,
    )
}

// ----------------------------------------------------------------------------
// uv
// ----------------------------------------------------------------------------
//
// uv pip install <pkg>...
// uv add <pkg>...
fn parse_uv(args: &[String]) -> ParsedInstall {
    // Recognise both `uv pip install ...` and `uv add ...`.
    if args.first().map(|s| s.as_str()) == Some("pip") {
        return parse_with_subcommand(
            Manager::Uv,
            &args[1..],
            &["install"],
            &["-r", "--requirement", "-c", "--constraint", "--index-url", "--extra-index-url"],
            &["-r", "--requirement"],
            strip_pep508_version,
            is_pip_specifier_special,
        );
    }
    parse_with_subcommand(
        Manager::Uv,
        args,
        &["add", "sync", "install"],
        &["--index-url", "--extra-index-url", "--python", "--package"],
        &[],
        strip_pep508_version,
        is_pip_specifier_special,
    )
}

// ----------------------------------------------------------------------------
// poetry
// ----------------------------------------------------------------------------
//
// poetry add <pkg>...
fn parse_poetry(args: &[String]) -> ParsedInstall {
    parse_with_subcommand(
        Manager::Poetry,
        args,
        &["add"],
        &["--source", "--python", "-E", "--extras", "-D", "--group"],
        &[],
        strip_pep508_version,
        is_pip_specifier_special,
    )
}

// ----------------------------------------------------------------------------
// npm / pnpm / yarn
// ----------------------------------------------------------------------------
fn parse_npm_like(manager: Manager, args: &[String], install_verbs: &[&str]) -> ParsedInstall {
    // value_flags are flags that take a *separate value*. The npm save flags
    // (`--save-dev`, `-D`, `--save-prod`, `-P`, `--save-optional`, `-O`,
    // `--save-peer`) are boolean — they do NOT consume the next token.
    parse_with_subcommand(
        manager,
        args,
        install_verbs,
        &[
            "--registry", "--prefix", "--workspace", "-w",
            "--filter", "-F", "--reporter", "--cwd", "--tag",
        ],
        &[],
        strip_npm_version,
        is_npm_specifier_special,
    )
}

// ----------------------------------------------------------------------------
// cargo
// ----------------------------------------------------------------------------
//
// cargo add <crate>[@<version>] [--features ...]
// cargo install <crate>[@<version>]
fn parse_cargo(args: &[String]) -> ParsedInstall {
    parse_with_subcommand(
        Manager::Cargo,
        args,
        &["add", "install"],
        &[
            "--features", "--default-features", "--no-default-features",
            "--registry", "--git", "--path", "--branch", "--tag", "--rev",
            "--profile", "--target", "--manifest-path", "--package", "-p",
            "--root", "--locked",
        ],
        &[],
        strip_cargo_version,
        is_cargo_specifier_special,
    )
}

// ----------------------------------------------------------------------------
// go
// ----------------------------------------------------------------------------
//
// go get <module>[@<version>]
// go install <module>[@<version>]
fn parse_go(args: &[String]) -> ParsedInstall {
    
    parse_with_subcommand(
        Manager::Go,
        args,
        &["get", "install"],
        &[],
        &[],
        strip_go_version,
        is_go_specifier_special,
    )
}

// ----------------------------------------------------------------------------
// Generic walker
// ----------------------------------------------------------------------------
fn parse_with_subcommand(
    manager: Manager,
    args: &[String],
    install_verbs: &[&str],
    value_flags: &[&str],
    requirement_flags: &[&str],
    normalize: fn(&str) -> Option<String>,
    is_special: fn(&str) -> bool,
) -> ParsedInstall {
    let ecosystem = manager.ecosystem();

    // Skip past the install verb, if present.
    let mut iter = args.iter();
    let mut found_verb = false;
    for a in iter.by_ref() {
        if install_verbs.iter().any(|v| v == a) {
            found_verb = true;
            break;
        }
        // If the first non-flag token is not an install verb, this is not an
        // install command we recognise.
        if !a.starts_with('-') {
            break;
        }
    }

    if !found_verb {
        return ParsedInstall {
            ecosystem,
            manager,
            packages: vec![],
            requirement_files: vec![],
            no_packages: true,
        };
    }

    let mut packages: Vec<String> = Vec::new();
    let mut requirement_files: Vec<String> = Vec::new();

    while let Some(a) = iter.next() {
        if a == "--" {
            // Everything after `--` is positional.
            for rest in iter.by_ref() {
                if let Some(p) = normalize(rest) {
                    if !is_special(rest) {
                        packages.push(p);
                    }
                }
            }
            break;
        }
        if a.starts_with('-') {
            // Handle `--flag=value` form: skip without consuming next.
            let stripped = a.split('=').next().unwrap_or(a);
            if requirement_flags.iter().any(|f| f == &stripped) && !a.contains('=') {
                if let Some(path) = iter.next() {
                    requirement_files.push(path.clone());
                }
                continue;
            }
            if requirement_flags.iter().any(|f| f == &stripped) && a.contains('=') {
                let path = a.split_once('=').map(|x| x.1).unwrap_or("");
                if !path.is_empty() {
                    requirement_files.push(path.to_string());
                }
                continue;
            }
            if value_flags.iter().any(|f| f == &stripped) && !a.contains('=') {
                let _ = iter.next();
                continue;
            }
            // Bare flag (`--upgrade`, `-U`); nothing to consume.
            continue;
        }
        if is_special(a) {
            continue;
        }
        if let Some(p) = normalize(a) {
            packages.push(p);
        }
    }

    let no_packages = packages.is_empty() && requirement_files.is_empty();
    ParsedInstall {
        ecosystem,
        manager,
        packages,
        requirement_files,
        no_packages,
    }
}

// ----------------------------------------------------------------------------
// Per-ecosystem normalisers
// ----------------------------------------------------------------------------

/// Strip PEP 508 version specifiers and extras: `pkg[extra]==1.0`, `pkg>=1`, etc.
fn strip_pep508_version(spec: &str) -> Option<String> {
    let s = spec.trim();
    if s.is_empty() {
        return None;
    }
    // PEP 508 separators: `==`, `>=`, `<=`, `!=`, `~=`, `>`, `<`, `;` (env marker).
    let cut = s
        .find(['=', '>', '<', '!', '~', ';', ' ', '@'])
        .unwrap_or(s.len());
    let head = &s[..cut];
    // Strip optional [extras]
    let name = head.split('[').next().unwrap_or(head).trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_lowercase())
    }
}

/// pip-style "this is not a registry name" specifier:
/// path-like (`./foo`, `/abs`), URL, VCS url, wheel/sdist file.
fn is_pip_specifier_special(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    s.starts_with('.')
        || s.starts_with('/')
        || s.contains("://")
        || s.starts_with("git+")
        || l.ends_with(".whl")
        || l.ends_with(".tar.gz")
        || l.ends_with(".zip")
}

/// Strip npm version: `pkg@1.2.3`, `@scope/pkg@^1`, but preserve scope on bare `@scope/pkg`.
fn strip_npm_version(spec: &str) -> Option<String> {
    let s = spec.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('@') {
        // Scoped: keep up to next '@' boundary if any.
        let mut parts = rest.splitn(2, '/');
        let scope = parts.next().unwrap_or("");
        let pkg_with_ver = parts.next().unwrap_or("");
        if pkg_with_ver.is_empty() {
            return None;
        }
        let pkg = pkg_with_ver.split('@').next().unwrap_or(pkg_with_ver);
        if scope.is_empty() || pkg.is_empty() {
            return None;
        }
        return Some(format!("@{scope}/{pkg}"));
    }
    let head = s.split('@').next().unwrap_or(s);
    if head.is_empty() {
        None
    } else {
        Some(head.to_string())
    }
}

fn is_npm_specifier_special(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    s.starts_with('.')
        || s.starts_with('/')
        || s.starts_with("file:")
        || s.contains("://")
        || s.starts_with("git+")
        || l.ends_with(".tgz")
        || l.ends_with(".tar.gz")
        || l.contains('#') && s.contains(':') // git#commit form
}

fn strip_cargo_version(spec: &str) -> Option<String> {
    let head = spec.split('@').next().unwrap_or(spec).trim();
    if head.is_empty() {
        None
    } else {
        Some(head.to_string())
    }
}

fn is_cargo_specifier_special(s: &str) -> bool {
    s.starts_with('.') || s.starts_with('/') || s.contains("://")
}

fn strip_go_version(spec: &str) -> Option<String> {
    let head = spec.split('@').next().unwrap_or(spec).trim();
    if head.is_empty() {
        None
    } else {
        Some(head.to_string())
    }
}

fn is_go_specifier_special(s: &str) -> bool {
    s.starts_with('.') || s.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    // ---------- pip ----------

    #[test]
    fn pip_install_simple() {
        let p = parse(Manager::Pip, &s(&["install", "requests", "fastapi"]));
        assert_eq!(p.packages, vec!["requests", "fastapi"]);
        assert!(p.requirement_files.is_empty());
    }

    #[test]
    fn pip_strips_version_specifiers_and_extras() {
        let p = parse(
            Manager::Pip,
            &s(&["install", "requests==2.31.0", "fastapi[all]>=0.110", "uvicorn~=0.30"]),
        );
        assert_eq!(p.packages, vec!["requests", "fastapi", "uvicorn"]);
    }

    #[test]
    fn pip_handles_requirements_file() {
        let p = parse(Manager::Pip, &s(&["install", "-r", "req.txt"]));
        assert!(p.packages.is_empty());
        assert_eq!(p.requirement_files, vec!["req.txt"]);
    }

    #[test]
    fn pip_handles_requirements_equals_form() {
        let p = parse(Manager::Pip, &s(&["install", "--requirement=req.txt"]));
        assert_eq!(p.requirement_files, vec!["req.txt"]);
    }

    #[test]
    fn pip_skips_path_specifier() {
        let p = parse(Manager::Pip, &s(&["install", "./mylib", "requests"]));
        assert_eq!(p.packages, vec!["requests"]);
    }

    #[test]
    fn pip_skips_git_specifier() {
        let p = parse(Manager::Pip, &s(&["install", "git+https://github.com/x/y"]));
        assert!(p.packages.is_empty());
    }

    #[test]
    fn pip_drops_value_flag_and_value() {
        let p = parse(
            Manager::Pip,
            &s(&["install", "--index-url", "https://x", "requests"]),
        );
        assert_eq!(p.packages, vec!["requests"]);
    }

    #[test]
    fn pip_no_install_verb_yields_no_packages() {
        let p = parse(Manager::Pip, &s(&["list"]));
        assert!(p.no_packages);
        assert!(p.packages.is_empty());
    }

    // ---------- uv ----------

    #[test]
    fn uv_pip_install_works() {
        let p = parse(Manager::Uv, &s(&["pip", "install", "requests"]));
        assert_eq!(p.packages, vec!["requests"]);
    }

    #[test]
    fn uv_add_works() {
        let p = parse(Manager::Uv, &s(&["add", "requests", "fastapi==0.110"]));
        assert_eq!(p.packages, vec!["requests", "fastapi"]);
    }

    // ---------- npm / pnpm / yarn ----------

    #[test]
    fn npm_install_simple() {
        let p = parse(Manager::Npm, &s(&["install", "react", "zod"]));
        assert_eq!(p.packages, vec!["react", "zod"]);
    }

    #[test]
    fn npm_i_alias_works() {
        let p = parse(Manager::Npm, &s(&["i", "react"]));
        assert_eq!(p.packages, vec!["react"]);
    }

    #[test]
    fn npm_handles_scoped_packages() {
        let p = parse(
            Manager::Npm,
            &s(&["install", "@anthropic-ai/sdk", "@types/node@latest"]),
        );
        assert_eq!(p.packages, vec!["@anthropic-ai/sdk", "@types/node"]);
    }

    #[test]
    fn npm_strips_versions() {
        let p = parse(
            Manager::Npm,
            &s(&["i", "react@^18", "zod@3.22.0", "lodash@latest"]),
        );
        assert_eq!(p.packages, vec!["react", "zod", "lodash"]);
    }

    #[test]
    fn yarn_add_works() {
        let p = parse(Manager::Yarn, &s(&["add", "react", "@types/node"]));
        assert_eq!(p.packages, vec!["react", "@types/node"]);
    }

    #[test]
    fn pnpm_skips_save_dev_flags() {
        let p = parse(
            Manager::Pnpm,
            &s(&["add", "-D", "vitest", "--save-prod", "react"]),
        );
        assert_eq!(p.packages, vec!["vitest", "react"]);
    }

    #[test]
    fn npm_skips_path_specifier() {
        let p = parse(Manager::Npm, &s(&["i", "./local", "react"]));
        assert_eq!(p.packages, vec!["react"]);
    }

    // ---------- cargo ----------

    #[test]
    fn cargo_add_simple() {
        let p = parse(Manager::Cargo, &s(&["add", "serde", "tokio@1.40"]));
        assert_eq!(p.packages, vec!["serde", "tokio"]);
    }

    #[test]
    fn cargo_install_works() {
        let p = parse(Manager::Cargo, &s(&["install", "ripgrep"]));
        assert_eq!(p.packages, vec!["ripgrep"]);
    }

    #[test]
    fn cargo_strips_features_value() {
        let p = parse(
            Manager::Cargo,
            &s(&["add", "tokio", "--features", "full,macros", "anyhow"]),
        );
        assert_eq!(p.packages, vec!["tokio", "anyhow"]);
    }

    // ---------- go ----------

    #[test]
    fn go_get_strips_version() {
        let p = parse(
            Manager::Go,
            &s(&["get", "github.com/spf13/cobra@v1.8.0", "github.com/aws/aws-sdk-go-v2"]),
        );
        assert_eq!(
            p.packages,
            vec!["github.com/spf13/cobra", "github.com/aws/aws-sdk-go-v2"]
        );
    }

    #[test]
    fn go_install_works() {
        let p = parse(Manager::Go, &s(&["install", "github.com/foo/bar@latest"]));
        assert_eq!(p.packages, vec!["github.com/foo/bar"]);
    }

    // ---------- manager detection ----------

    #[test]
    fn detects_managers_from_program() {
        assert_eq!(Manager::from_program("pip"), Some(Manager::Pip));
        assert_eq!(Manager::from_program("pip3"), Some(Manager::Pip));
        assert_eq!(Manager::from_program("/usr/bin/uv"), Some(Manager::Uv));
        assert_eq!(Manager::from_program("npm"), Some(Manager::Npm));
        assert_eq!(Manager::from_program("yarn.exe"), Some(Manager::Yarn));
        assert_eq!(Manager::from_program("unrecognized"), None);
    }
}
