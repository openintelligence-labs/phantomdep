use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, ExitCode};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use phantomdep_core::{
    Action, Ecosystem, Evidence, EvidenceBundle, HookEvent, Lookup, LspServer, Manager,
    McpServer, PackageCache, PhantomDb, Resolver, ScanReport, Verdict, evaluate_hook,
    extract_requirements, hook_install, parse_install, report_to_markdown, report_to_sarif,
    scan_path,
};

#[derive(Debug, Parser)]
#[command(
    name = "phantomdep",
    version,
    about = "Local-first dependency firewall for AI coding agents",
    long_about = "PhantomDep validates every new dependency at the moment it is introduced — \
                  AI tool call, editor, shell, manifest, diff, PR, or CI — and produces an \
                  evidence-backed verdict before phantom, squatted, or malicious packages \
                  reach your machine or repo."
)]
struct Cli {
    /// Path to a Phantom-DB directory to layer on top of the bootstrap data.
    #[arg(long, global = true, env = "PHANTOMDEP_DB")]
    db: Option<PathBuf>,

    /// Disable the local SQLite cache.
    #[arg(long, global = true)]
    no_cache: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a single package name against the registry and Phantom-DB.
    Check {
        name: String,
        #[arg(short, long, default_value = "pypi")]
        ecosystem: String,
        #[arg(short, long, value_enum, default_value_t = Format::Terminal)]
        format: Format,
        /// Skip network calls; use only the local Phantom-DB and offline data.
        #[arg(long)]
        offline: bool,
    },
    /// Scan a directory of source code for risky dependencies.
    Scan {
        /// Path to scan (defaults to current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long, value_enum, default_value_t = Format::Terminal)]
        format: Format,
        /// Maximum concurrent registry lookups.
        #[arg(long, default_value_t = 16)]
        concurrency: usize,
    },
    /// Explain the evidence behind a verdict for a single package.
    Explain {
        name: String,
        #[arg(short, long, default_value = "pypi")]
        ecosystem: String,
        #[arg(long)]
        offline: bool,
    },
    /// Run the built-in launch demo against four canonical cases.
    Doctor,
    /// Replay every Phantom-DB entry against the live registry.
    ///
    /// Useful for "what does PhantomDep catch right now?" demos and for
    /// validating that the loaded DB matches today's registry state.
    Replay {
        /// Skip network calls; show DB-only verdicts.
        #[arg(long)]
        offline: bool,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Limit to the first N entries (handy for quick demos).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Run a microbenchmark suite (cold, warm, scan, hook).
    Benchmark {
        /// Number of iterations per measurement.
        #[arg(long, default_value_t = 10)]
        iterations: usize,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run the MCP (Model Context Protocol) server on stdio.
    ///
    /// Read-only tools, deterministic outputs, no shell exec, no network unless
    /// the underlying ecosystem checker needs it. See architecture §5.5.
    Mcp,
    /// Run the LSP (Language Server Protocol) server on stdio.
    ///
    /// Emits diagnostics for hallucinated/squatted/lookalike imports in
    /// .py/.js/.ts files and offers "Did you mean ..." quick-fix code actions.
    /// Wire it into VS Code, Cursor, Windsurf, Neovim, Helix, or any LSP client.
    Lsp,
    /// Claude Code PreToolUse hook helpers.
    Hook(HookCmd),
    /// Validate every package in an install command, then exec the install if clear.
    ///
    /// Examples:
    ///   phantomdep wrap pip install requests fastapi
    ///   phantomdep wrap npm install react @anthropic-ai/sdk
    ///   phantomdep wrap cargo add serde tokio
    ///   phantomdep wrap go get github.com/spf13/cobra
    Wrap {
        /// Auto-allow WARN-level findings (e.g. lookalikes).
        #[arg(long)]
        yes: bool,
        /// Auto-deny WARN-level findings without prompting.
        #[arg(long, conflicts_with = "yes")]
        no_prompt: bool,
        /// Skip the underlying command — print the verdicts and exit.
        #[arg(long)]
        dry_run: bool,
        /// The install command to wrap (e.g. `pip install requests`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Terminal,
    Json,
    Sarif,
    Markdown,
}

#[derive(Debug, clap::Args)]
struct HookCmd {
    #[command(subcommand)]
    action: HookAction,
}

#[derive(Debug, Subcommand)]
enum HookAction {
    /// Read a Claude Code PreToolUse event JSON on stdin and emit a decision JSON on stdout.
    /// Exit codes: 0 = allow, 2 = block (Claude reads stderr as the block reason).
    Check {
        /// Print the full evidence bundle to stderr alongside the decision.
        #[arg(long)]
        verbose: bool,
    },
    /// Install the PhantomDep PreToolUse hook into ~/.claude/settings.json.
    Install {
        /// Override the path to settings.json (default: ~/.claude/settings.json).
        #[arg(long)]
        path: Option<PathBuf>,
        /// Override the command to register (default: `<this-binary> hook check`).
        #[arg(long)]
        command: Option<String>,
    },
    /// Remove the PhantomDep PreToolUse hook from ~/.claude/settings.json.
    Uninstall {
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("phantomdep: {err:#}");
            ExitCode::from(3)
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let db = load_phantom_db(cli.db.as_deref())?;
    let cache = if cli.no_cache {
        None
    } else {
        Some(Arc::new(PackageCache::open_default().context("opening cache")?))
    };

    match cli.command {
        Command::Check {
            name,
            ecosystem,
            format,
            offline,
        } => check(&name, &ecosystem, format, offline, &db, cache).await,
        Command::Scan {
            path,
            format,
            concurrency,
        } => scan(&path, format, concurrency, &db, cache).await,
        Command::Explain {
            name,
            ecosystem,
            offline,
        } => explain(&name, &ecosystem, offline, &db, cache).await,
        Command::Doctor => doctor(&db, cache).await,
        Command::Replay { offline, json, limit } => replay(offline, json, limit, &db, cache).await,
        Command::Benchmark { iterations, json } => benchmark(iterations, json, &db, cache).await,
        Command::Wrap {
            yes,
            no_prompt,
            dry_run,
            command,
        } => wrap(command, yes, no_prompt, dry_run, &db, cache).await,
        Command::Mcp => mcp(db, cache).await,
        Command::Lsp => lsp(db, cache).await,
        Command::Hook(hook_cmd) => match hook_cmd.action {
            HookAction::Check { verbose } => hook_check(verbose, db, cache).await,
            HookAction::Install { path, command } => hook_install_cmd(path, command),
            HookAction::Uninstall { path } => hook_uninstall_cmd(path),
        },
    }
}

async fn mcp(db: PhantomDb, cache: Option<Arc<PackageCache>>) -> Result<ExitCode> {
    let lookup = Arc::new(Lookup::new(cache).context("constructing lookup")?);
    let server = McpServer::new(lookup, Arc::new(db));
    server.serve_stdio().await.context("running MCP server")?;
    Ok(ExitCode::from(0))
}

async fn lsp(db: PhantomDb, cache: Option<Arc<PackageCache>>) -> Result<ExitCode> {
    let lookup = Arc::new(Lookup::new(cache).context("constructing lookup")?);
    let server = LspServer::new(lookup, Arc::new(db));
    server.serve_stdio().await.context("running LSP server")?;
    Ok(ExitCode::from(0))
}

async fn hook_check(
    verbose: bool,
    db: PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading hook event from stdin")?;
    if input.trim().is_empty() {
        eprintln!("phantomdep hook check: empty stdin; nothing to evaluate.");
        return Ok(ExitCode::from(0));
    }
    let event: HookEvent =
        serde_json::from_str(&input).context("parsing hook event JSON")?;
    let lookup = Arc::new(Lookup::new(cache).context("constructing lookup")?);
    let evaluation = evaluate_hook(event, lookup, Arc::new(db))
        .await
        .context("evaluating hook")?;

    if verbose {
        for bundle in &evaluation.bundles {
            eprintln!(
                "  {:?}/{:?} {} ({:?})",
                bundle.action, bundle.verdict, bundle.name, bundle.ecosystem
            );
        }
    }

    let json = serde_json::to_string(&evaluation.decision)?;
    println!("{json}");

    if matches!(evaluation.worst_action, Action::Block) {
        // Per Claude Code hooks spec, exit code 2 + stderr → reason fed back to Claude.
        eprintln!("{}", evaluation.decision.reason);
        Ok(ExitCode::from(2))
    } else {
        Ok(ExitCode::from(0))
    }
}

fn hook_install_cmd(path: Option<PathBuf>, command: Option<String>) -> Result<ExitCode> {
    let target = match path {
        Some(p) => p,
        None => hook_install::settings_path()?,
    };
    let cmd = match command {
        Some(c) => c,
        None => {
            let exe = std::env::current_exe().context("resolving current binary path")?;
            format!("{} hook check", exe.display())
        }
    };
    let modified = hook_install::install(&target, &cmd)?;
    if modified {
        println!(
            "phantomdep: installed PreToolUse hook in {} (command: `{}`)",
            target.display(),
            cmd
        );
    } else {
        println!("phantomdep: hook already installed in {}", target.display());
    }
    Ok(ExitCode::from(0))
}

fn hook_uninstall_cmd(path: Option<PathBuf>) -> Result<ExitCode> {
    let target = match path {
        Some(p) => p,
        None => hook_install::settings_path()?,
    };
    let removed = hook_install::uninstall(&target)?;
    if removed {
        println!("phantomdep: removed PreToolUse hook from {}", target.display());
    } else {
        println!(
            "phantomdep: no phantomdep hook found in {}",
            target.display()
        );
    }
    Ok(ExitCode::from(0))
}

fn load_phantom_db(db_path: Option<&std::path::Path>) -> Result<PhantomDb> {
    let candidate = db_path
        .map(PathBuf::from)
        .or_else(|| {
            // Try ./phantom-db relative to cwd
            let p = PathBuf::from("phantom-db");
            if p.exists() { Some(p) } else { None }
        });

    if let Some(path) = candidate {
        PhantomDb::from_dir(&path).with_context(|| format!("loading phantom-db at {}", path.display()))
    } else {
        Ok(PhantomDb::bootstrap())
    }
}

async fn check(
    name: &str,
    ecosystem: &str,
    format: Format,
    offline: bool,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    let ecosystem: Ecosystem = ecosystem.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let bundle = single_lookup(name, ecosystem, offline, db, cache).await?;
    match format {
        Format::Json => print_json(&bundle)?,
        Format::Terminal => print_terminal(&bundle)?,
        Format::Sarif | Format::Markdown => {
            anyhow::bail!("--format sarif/markdown is only valid for `scan`");
        }
    }
    Ok(ExitCode::from(bundle.action.exit_code() as u8))
}

async fn explain(
    name: &str,
    ecosystem: &str,
    offline: bool,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    let ecosystem: Ecosystem = ecosystem.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let bundle = single_lookup(name, ecosystem, offline, db, cache).await?;
    print_explain(&bundle)?;
    Ok(ExitCode::from(bundle.action.exit_code() as u8))
}

async fn single_lookup(
    name: &str,
    ecosystem: Ecosystem,
    offline: bool,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<EvidenceBundle> {
    let record = if offline {
        let mut r = phantomdep_core::PackageRecord::missing(name, ecosystem);
        r.exists = db.lookup(name, ecosystem).is_some();
        r
    } else {
        let lookup = Lookup::new(cache).context("constructing lookup")?;
        lookup.lookup(name, ecosystem).await.context("registry lookup")?
    };
    Ok(Resolver::new(db).resolve(name, ecosystem, record))
}

async fn scan(
    path: &std::path::Path,
    format: Format,
    concurrency: usize,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    let lookup = Arc::new(Lookup::new(cache).context("constructing lookup")?);
    let report = scan_path(path, lookup, db, concurrency)
        .await
        .context("scanning project")?;
    let mut stdout = io::stdout().lock();
    match format {
        Format::Json => {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&report)?)?;
        }
        Format::Sarif => {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&report_to_sarif(&report))?)?;
        }
        Format::Markdown => {
            write!(stdout, "{}", report_to_markdown(&report))?;
        }
        Format::Terminal => {
            drop(stdout);
            print_scan_report(&report)?;
        }
    }
    Ok(ExitCode::from(report.worst_action().exit_code() as u8))
}

async fn wrap(
    command: Vec<String>,
    auto_yes: bool,
    no_prompt: bool,
    dry_run: bool,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    if command.is_empty() {
        anyhow::bail!("wrap: missing install command (e.g. `phantomdep wrap pip install ...`)");
    }
    let program = &command[0];
    let manager = Manager::from_program(program).ok_or_else(|| {
        anyhow::anyhow!(
            "wrap: don't know how to wrap `{program}`. Supported: pip, uv, poetry, npm, pnpm, yarn, cargo, go"
        )
    })?;

    let parsed = parse_install(manager, &command[1..]);

    // Resolve requirements files (pip -r req.txt) into more package names.
    let mut all_packages: Vec<String> = parsed.packages.clone();
    for req_path in &parsed.requirement_files {
        match std::fs::read_to_string(req_path) {
            Ok(text) => {
                for name in extract_requirements(&text) {
                    if !all_packages.contains(&name) {
                        all_packages.push(name);
                    }
                }
            }
            Err(err) => {
                eprintln!("phantomdep wrap: could not read {req_path}: {err}");
            }
        }
    }

    if all_packages.is_empty() {
        if parsed.no_packages {
            // Nothing recognisably to install — pass through verbatim.
            return passthrough(&command, dry_run);
        }
        eprintln!("phantomdep wrap: no package names extracted; running command unchanged.");
        return passthrough(&command, dry_run);
    }

    // Validate each package with bounded parallelism (matches scan / hook /
    // MCP). Caps simultaneous registry calls so `pip install -r huge.txt`
    // doesn't open hundreds of HTTP connections at once.
    const WRAP_CONCURRENCY: usize = 16;
    let lookup = Arc::new(Lookup::new(cache).context("constructing lookup")?);
    let resolver = Resolver::new(db);
    let ecosystem = parsed.ecosystem;
    use futures::stream::{self, StreamExt};
    let bundles: Vec<(String, EvidenceBundle)> = stream::iter(all_packages.iter().cloned())
        .map(|pkg| {
            let lookup = Arc::clone(&lookup);
            async move {
                let record = lookup.lookup(&pkg, ecosystem).await;
                (pkg, record)
            }
        })
        .buffer_unordered(WRAP_CONCURRENCY)
        .map(|(pkg, record)| {
            let bundle = match record {
                Ok(r) => resolver.resolve(&pkg, ecosystem, r),
                Err(err) => {
                    let mut b = EvidenceBundle::new(pkg.clone(), ecosystem);
                    b.verdict = Verdict::Unknown;
                    b.action = Action::Warn;
                    b.evidence.push(Evidence::Note {
                        source: "lookup".into(),
                        message: format!("registry lookup failed: {err}"),
                    });
                    b
                }
            };
            (pkg, bundle)
        })
        .collect()
        .await;
    let mut bundles = bundles;

    // Sort bundles by worst action descending so the user sees the bad ones first.
    bundles.sort_by_key(|(name, b)| (-(action_rank(b.action) as i32), name.clone()));

    let mut blocks = 0usize;
    let mut warns = 0usize;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "phantomdep wrap: validating {} package(s) for `{} {}`",
        bundles.len(),
        program,
        parsed_summary(manager),
    )?;
    for (_, bundle) in &bundles {
        writeln!(out, "  {}  {}", verdict_badge(bundle.verdict), bundle.name)?;
        if let Some(fix) = bundle.fixes.first() {
            writeln!(
                out,
                "      did you mean: {} (confidence {:.2})",
                fix.replacement, fix.confidence
            )?;
        }
        match bundle.action {
            Action::Block => blocks += 1,
            Action::Warn => warns += 1,
            Action::Allow => {}
        }
    }
    writeln!(out)?;
    drop(out);

    if blocks > 0 {
        eprintln!(
            "phantomdep wrap: blocking install. {blocks} package(s) returned BLOCK verdict."
        );
        return Ok(ExitCode::from(2));
    }

    if warns > 0 {
        let proceed = if auto_yes {
            true
        } else if no_prompt || !std::io::stdin().is_terminal() {
            false
        } else {
            prompt_proceed(warns)?
        };
        if !proceed {
            eprintln!("phantomdep wrap: blocking install on warn (user denied or non-interactive).");
            return Ok(ExitCode::from(2));
        }
    }

    passthrough(&command, dry_run)
}

fn parsed_summary(manager: Manager) -> &'static str {
    match manager {
        Manager::Pip | Manager::Uv => "install ...",
        Manager::Poetry => "add ...",
        Manager::Npm | Manager::Pnpm => "install ...",
        Manager::Yarn => "add ...",
        Manager::Cargo => "add ...",
        Manager::Go => "get ...",
    }
}

fn action_rank(a: Action) -> u8 {
    match a {
        Action::Block => 2,
        Action::Warn => 1,
        Action::Allow => 0,
    }
}

fn prompt_proceed(warns: usize) -> Result<bool> {
    eprint!(
        "phantomdep wrap: {warns} warning(s). Proceed with install anyway? [y/N] "
    );
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    handle.read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(matches!(answer.as_str(), "y" | "yes"))
}

fn passthrough(command: &[String], dry_run: bool) -> Result<ExitCode> {
    if dry_run {
        eprintln!("phantomdep wrap: --dry-run set; would have executed: {}", command.join(" "));
        return Ok(ExitCode::from(0));
    }
    let status = ProcessCommand::new(&command[0])
        .args(&command[1..])
        .status()
        .with_context(|| format!("could not exec {}", command[0]))?;
    let code = status.code().unwrap_or(1);
    Ok(ExitCode::from((code & 0xff) as u8))
}

async fn replay(
    offline: bool,
    as_json: bool,
    limit: Option<usize>,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    let lookup = if offline {
        None
    } else {
        Some(Arc::new(Lookup::new(cache).context("constructing lookup")?))
    };

    let resolver = Resolver::new(db);
    let mut entries: Vec<&phantomdep_core::PhantomEntry> = db.entries().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let total = entries.len();
    if let Some(n) = limit {
        entries.truncate(n);
    }

    let mut results: Vec<EvidenceBundle> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let record = if let Some(ref lk) = lookup {
            lk.lookup(&entry.name, entry.ecosystem)
                .await
                .unwrap_or_else(|_| phantomdep_core::PackageRecord::missing(&entry.name, entry.ecosystem))
        } else {
            // DB-only path: pretend the package exists so the Phantom-DB hit fires.
            let mut r = phantomdep_core::PackageRecord::missing(&entry.name, entry.ecosystem);
            r.exists = true;
            r
        };
        results.push(resolver.resolve(&entry.name, entry.ecosystem, record));
    }

    if as_json {
        let payload = serde_json::json!({
            "snapshot": db.snapshot(),
            "total_in_db": total,
            "evaluated": results.len(),
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::from(0));
    }

    let mut out = io::stdout().lock();
    writeln!(out)?;
    writeln!(
        out,
        "  PhantomDep replay — {} entries from snapshot {}",
        results.len(),
        db.snapshot().unwrap_or("(none)")
    )?;
    writeln!(out)?;
    let mut blocks = 0usize;
    let mut warns = 0usize;
    for bundle in &results {
        let badge = verdict_badge(bundle.verdict);
        writeln!(out, "  {} {}  ({})", badge, bundle.name, bundle.ecosystem.as_str())?;
        if let Some(fix) = bundle.fixes.first() {
            writeln!(out, "      → suggested: {}", fix.replacement)?;
        }
        match bundle.action {
            Action::Block => blocks += 1,
            Action::Warn => warns += 1,
            Action::Allow => {}
        }
    }
    writeln!(out)?;
    writeln!(out, "  summary: {} block, {} warn, {} allow", blocks, warns, results.len() - blocks - warns)?;
    writeln!(out)?;
    Ok(ExitCode::from(0))
}

async fn benchmark(
    iterations: usize,
    as_json: bool,
    db: &PhantomDb,
    cache: Option<Arc<PackageCache>>,
) -> Result<ExitCode> {
    use std::time::Instant;
    let lookup = Arc::new(Lookup::new(cache).context("constructing lookup")?);

    // Warm caches.
    let _ = lookup.lookup("requests", Ecosystem::Pypi).await;
    let _ = lookup.lookup("react", Ecosystem::Npm).await;
    let _ = lookup.lookup("phantomdep-totally-fake-12345", Ecosystem::Pypi).await;

    let mut warm_real_pypi = Vec::with_capacity(iterations);
    let mut warm_real_npm = Vec::with_capacity(iterations);
    let mut warm_phantom = Vec::with_capacity(iterations);
    let mut db_only = Vec::with_capacity(iterations);
    let resolver = Resolver::new(db);

    for _ in 0..iterations {
        let t = Instant::now();
        let r = lookup.lookup("requests", Ecosystem::Pypi).await?;
        let _ = resolver.resolve("requests", Ecosystem::Pypi, r);
        warm_real_pypi.push(t.elapsed().as_micros() as u64);

        let t = Instant::now();
        let r = lookup.lookup("react", Ecosystem::Npm).await?;
        let _ = resolver.resolve("react", Ecosystem::Npm, r);
        warm_real_npm.push(t.elapsed().as_micros() as u64);

        let t = Instant::now();
        let r = lookup
            .lookup("phantomdep-totally-fake-12345", Ecosystem::Pypi)
            .await?;
        let _ = resolver.resolve("phantomdep-totally-fake-12345", Ecosystem::Pypi, r);
        warm_phantom.push(t.elapsed().as_micros() as u64);

        let t = Instant::now();
        let mut record = phantomdep_core::PackageRecord::missing("offline-bench", Ecosystem::Pypi);
        record.exists = true;
        let _ = resolver.resolve("offline-bench", Ecosystem::Pypi, record);
        db_only.push(t.elapsed().as_micros() as u64);
    }

    fn stats(samples: &[u64]) -> (u64, u64, u64) {
        let mut s = samples.to_vec();
        s.sort_unstable();
        let p50 = s[s.len() / 2];
        let p95 = s[s.len() * 95 / 100];
        let max = *s.last().unwrap();
        (p50, p95, max)
    }

    let (p50_real_pypi, p95_real_pypi, max_real_pypi) = stats(&warm_real_pypi);
    let (p50_real_npm, p95_real_npm, max_real_npm) = stats(&warm_real_npm);
    let (p50_phantom, p95_phantom, max_phantom) = stats(&warm_phantom);
    let (p50_offline, p95_offline, max_offline) = stats(&db_only);

    if as_json {
        let payload = serde_json::json!({
            "iterations": iterations,
            "warm_real_pypi_us": { "p50": p50_real_pypi, "p95": p95_real_pypi, "max": max_real_pypi },
            "warm_real_npm_us":  { "p50": p50_real_npm,  "p95": p95_real_npm,  "max": max_real_npm  },
            "warm_phantom_us":   { "p50": p50_phantom,   "p95": p95_phantom,   "max": max_phantom   },
            "offline_resolve_us": { "p50": p50_offline,  "p95": p95_offline,  "max": max_offline   },
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::from(0));
    }

    let mut out = io::stdout().lock();
    writeln!(out)?;
    writeln!(out, "  PhantomDep benchmark ({} iterations, warm caches)", iterations)?;
    writeln!(out)?;
    writeln!(
        out,
        "  {:<28} {:>10} {:>10} {:>10}",
        "scenario", "p50 (μs)", "p95 (μs)", "max (μs)"
    )?;
    writeln!(out, "  {:-<60}", "")?;
    writeln!(
        out,
        "  {:<28} {:>10} {:>10} {:>10}",
        "real PyPI (cached)", p50_real_pypi, p95_real_pypi, max_real_pypi
    )?;
    writeln!(
        out,
        "  {:<28} {:>10} {:>10} {:>10}",
        "real npm (cached)", p50_real_npm, p95_real_npm, max_real_npm
    )?;
    writeln!(
        out,
        "  {:<28} {:>10} {:>10} {:>10}",
        "phantom PyPI (cached)", p50_phantom, p95_phantom, max_phantom
    )?;
    writeln!(
        out,
        "  {:<28} {:>10} {:>10} {:>10}",
        "offline resolve only", p50_offline, p95_offline, max_offline
    )?;
    writeln!(out)?;
    Ok(ExitCode::from(0))
}

async fn doctor(db: &PhantomDb, cache: Option<Arc<PackageCache>>) -> Result<ExitCode> {
    let cases: &[(&str, Ecosystem, &str)] = &[
        (
            "phantomdep-totally-fake-pkg-12345",
            Ecosystem::Pypi,
            "PHANTOM — a name no attacker has registered yet",
        ),
        (
            "huggingface-cli",
            Ecosystem::Pypi,
            "SQUATTED — Lasso's canonical proof-of-exploit case",
        ),
        (
            "reqests",
            Ecosystem::Pypi,
            "LOOKALIKE — typosquat of `requests` (edit distance 1)",
        ),
        (
            "requests",
            Ecosystem::Pypi,
            "REAL — established package, allowed",
        ),
    ];

    let mut stdout = io::stdout().lock();
    writeln!(stdout)?;
    writeln!(stdout, "  PhantomDep doctor — running 4 canonical demos")?;
    writeln!(stdout, "  Phantom-DB snapshot: {}", db.snapshot().unwrap_or("none"))?;
    writeln!(stdout)?;
    drop(stdout);

    let mut worst = 0u8;
    for (name, ecosystem, blurb) in cases {
        let lookup = Lookup::new(cache.clone()).context("constructing lookup")?;
        let record = lookup
            .lookup(name, *ecosystem)
            .await
            .with_context(|| format!("looking up {name}"))?;
        let bundle = Resolver::new(db).resolve(name, *ecosystem, record);
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "  {}", blurb)?;
        drop(stdout);
        print_terminal(&bundle)?;
        let mut stdout = io::stdout().lock();
        writeln!(stdout)?;
        worst = worst.max(bundle.action.exit_code() as u8);
    }

    let mut stdout = io::stdout().lock();
    writeln!(stdout, "  doctor done.")?;
    Ok(ExitCode::from(worst))
}

fn print_json(bundle: &EvidenceBundle) -> Result<()> {
    let out = serde_json::to_string_pretty(bundle)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{out}")?;
    Ok(())
}

fn print_terminal(bundle: &EvidenceBundle) -> Result<()> {
    let mut out = io::stdout().lock();
    let badge = verdict_badge(bundle.verdict);
    writeln!(
        out,
        "{badge} {name}  ({ecosystem})",
        name = bundle.name,
        ecosystem = bundle.ecosystem.as_str(),
    )?;
    writeln!(
        out,
        "  verdict:    {:?}    action: {:?}    confidence: {:.2}",
        bundle.verdict, bundle.action, bundle.confidence
    )?;
    if matches!(bundle.verdict, Verdict::Real) {
        writeln!(out, "  risk score: {} / 100", bundle.risk_score)?;
    }
    if let Some(snap) = &bundle.phantom_db_snapshot {
        writeln!(out, "  phantom-db: {snap}")?;
    }
    if !bundle.evidence.is_empty() {
        writeln!(out, "  evidence:")?;
        for ev in &bundle.evidence {
            writeln!(out, "    - {}", format_evidence(ev))?;
        }
    }
    if !bundle.fixes.is_empty() {
        writeln!(out, "  did you mean:")?;
        for fix in &bundle.fixes {
            writeln!(
                out,
                "    → {} (confidence {:.2})",
                fix.replacement, fix.confidence
            )?;
        }
    }
    Ok(())
}

fn print_explain(bundle: &EvidenceBundle) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out)?;
    writeln!(
        out,
        "  Package : {} ({})",
        bundle.name,
        bundle.ecosystem.as_str()
    )?;
    writeln!(out, "  Verdict : {:?}", bundle.verdict)?;
    writeln!(out, "  Action  : {:?}", bundle.action)?;
    writeln!(out, "  Confidence : {:.2}", bundle.confidence)?;
    if let Some(snap) = &bundle.phantom_db_snapshot {
        writeln!(out, "  Phantom-DB : {}", snap)?;
    }
    writeln!(out)?;
    writeln!(out, "  Why we think so")?;
    writeln!(out, "  ───────────────")?;
    if bundle.evidence.is_empty() {
        writeln!(out, "  (no signals collected)")?;
    } else {
        for ev in &bundle.evidence {
            writeln!(out, "  • {}", format_evidence(ev))?;
        }
    }
    if !bundle.fixes.is_empty() {
        writeln!(out)?;
        writeln!(out, "  What you probably meant")?;
        writeln!(out, "  ───────────────────────")?;
        for fix in &bundle.fixes {
            writeln!(
                out,
                "  → {}    (confidence {:.2})",
                fix.replacement, fix.confidence
            )?;
        }
    }
    writeln!(out)?;
    Ok(())
}

fn print_scan_report(report: &ScanReport) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out)?;
    writeln!(
        out,
        "  Scanned {} files in {}",
        report.files_scanned,
        report.root.display()
    )?;
    writeln!(out, "  Resolved {} unique packages", report.packages_seen)?;

    let counts = report.count_by_verdict();
    if !counts.is_empty() {
        writeln!(out)?;
        let mut buf = String::from("  ");
        for (k, v) in &counts {
            buf.push_str(&format!("{k}={v}  "));
        }
        writeln!(out, "{}", buf.trim_end())?;
    }
    writeln!(out)?;

    for finding in &report.findings {
        let badge = verdict_badge(finding.bundle.verdict);
        writeln!(out, "  {} {}", badge, finding.package)?;
        let files: Vec<String> = finding
            .files
            .iter()
            .take(3)
            .map(|p| p.display().to_string())
            .collect();
        if !files.is_empty() {
            let suffix = if finding.files.len() > 3 {
                format!("  (+{} more)", finding.files.len() - 3)
            } else {
                String::new()
            };
            writeln!(out, "      seen in: {}{}", files.join(", "), suffix)?;
        }
        if !finding.bundle.fixes.is_empty() {
            let fix = &finding.bundle.fixes[0];
            writeln!(
                out,
                "      did you mean: {} (confidence {:.2})",
                fix.replacement, fix.confidence
            )?;
        }
    }
    writeln!(out)?;
    Ok(())
}

fn verdict_badge(v: Verdict) -> &'static str {
    match v {
        Verdict::Phantom => "✗ PHANTOM     ",
        Verdict::KnownMalicious => "✗ MALICIOUS   ",
        Verdict::Squatted => "✗ SQUATTED    ",
        Verdict::InternalCollision => "✗ COLLISION   ",
        Verdict::ApiMismatch => "! API MISMATCH",
        Verdict::Lookalike => "! LOOKALIKE   ",
        Verdict::Real => "✓ REAL        ",
        Verdict::Unknown => "? UNKNOWN     ",
    }
}

fn format_evidence(ev: &Evidence) -> String {
    use Evidence::*;
    match ev {
        RegistryExistence { source, exists, .. } => {
            format!("registry_existence: {} reports exists={}", source, exists)
        }
        RegistryAge {
            source, value_days, ..
        } => format!("registry_age: {source} package is {value_days} days old"),
        Downloads30d { source, value, .. } => {
            format!("downloads_30d: {source} reports {value}")
        }
        PhantomDbHit {
            status,
            first_observed,
            intended_target,
            ..
        } => {
            let mut s = format!("phantom_db_hit: status={status}");
            if let Some(when) = first_observed {
                s.push_str(&format!(", first_observed={when}"));
            }
            if let Some(target) = intended_target {
                s.push_str(&format!(", intended_target={target}"));
            }
            s
        }
        Lookalike {
            edit_distance,
            compared_to,
            ..
        } => format!("lookalike: edit_distance={edit_distance} from {compared_to}"),
        KnownMalicious {
            source,
            advisory_id,
        } => match advisory_id {
            Some(id) => format!("known_malicious: {source} advisory {id}"),
            None => format!("known_malicious: {source}"),
        },
        GithubLink {
            url, starjacked, ..
        } => format!(
            "github_link: url={} starjacked={}",
            url.as_deref().unwrap_or("none"),
            starjacked
        ),
        Provenance { source, verified } => {
            format!("provenance: {source} verified={verified}")
        }
        Note { source, message } => format!("note ({source}): {message}"),
    }
}
