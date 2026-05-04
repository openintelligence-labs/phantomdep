//! Install / uninstall the PhantomDep PreToolUse hook in `~/.claude/settings.json`.
//!
//! Idempotent: detects existing entries by the `phantomdep:hook` marker.
//! Backs up settings.json to `settings.json.phantomdep-bak` before mutation.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

const MARKER_PREFIX: &str = "phantomdep:hook";

pub fn settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not resolve home directory")?;
    Ok(home.join(".claude").join("settings.json"))
}

pub fn install(target_path: &Path, command: &str) -> Result<bool> {
    let mut settings = read_settings(target_path)?;
    let modified = inject_hook(&mut settings, command)?;
    if modified {
        backup_then_write(target_path, &settings)?;
    }
    Ok(modified)
}

pub fn uninstall(target_path: &Path) -> Result<bool> {
    let mut settings = read_settings(target_path)?;
    let modified = remove_hook(&mut settings);
    if modified {
        backup_then_write(target_path, &settings)?;
    }
    Ok(modified)
}

fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading {}", path.display()))?;
    if bytes.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing JSON in {}", path.display()))
}

fn backup_then_write(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    if path.exists() {
        let backup = path.with_extension("json.phantomdep-bak");
        std::fs::copy(path, &backup)
            .with_context(|| format!("backing up to {}", backup.display()))?;
    }
    let pretty = serde_json::to_string_pretty(value).context("encoding settings JSON")?;
    std::fs::write(path, pretty + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn inject_hook(settings: &mut Value, command: &str) -> Result<bool> {
    let obj = ensure_object(settings, "settings root")?;
    let hooks = obj
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("`hooks` is not an object"))?;
    let pretooluse = hooks_obj
        .entry("PreToolUse".to_string())
        .or_insert_with(|| json!([]));
    let arr = pretooluse
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("`hooks.PreToolUse` is not an array"))?;

    // Look for an existing PhantomDep block by marker.
    if arr.iter().any(entry_has_marker) {
        return Ok(false);
    }

    let phantomdep_entry = json!({
        "matcher": "Bash|Write|Edit|MultiEdit",
        "hooks": [{
            "type": "command",
            "command": command,
            "_marker": MARKER_PREFIX
        }]
    });
    arr.push(phantomdep_entry);
    Ok(true)
}

fn remove_hook(settings: &mut Value) -> bool {
    let Some(obj) = settings.as_object_mut() else {
        return false;
    };
    let Some(hooks) = obj.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return false;
    };
    let Some(pretooluse) = hooks.get_mut("PreToolUse").and_then(|p| p.as_array_mut()) else {
        return false;
    };
    let before = pretooluse.len();
    pretooluse.retain(|entry| !entry_has_marker(entry));
    pretooluse.len() != before
}

fn entry_has_marker(entry: &Value) -> bool {
    let Some(arr) = entry.get("hooks").and_then(|h| h.as_array()) else {
        return false;
    };
    arr.iter().any(|h| {
        h.get("_marker")
            .and_then(|m| m.as_str())
            .map(|s| s == MARKER_PREFIX)
            .unwrap_or(false)
    })
}

fn ensure_object<'a>(
    value: &'a mut Value,
    label: &str,
) -> Result<&'a mut serde_json::Map<String, Value>> {
    if !value.is_object() {
        anyhow::bail!("{label} is not a JSON object");
    }
    Ok(value.as_object_mut().expect("checked above"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_settings(initial: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "phantomdep-hook-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        let path = p.join("settings.json");
        if !initial.is_empty() {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(initial.as_bytes()).unwrap();
        }
        path
    }

    #[test]
    fn install_creates_file_when_missing() {
        let path = tmp_settings("");
        let modified = install(&path, "/usr/local/bin/phantomdep hook check").unwrap();
        assert!(modified);
        let bytes = std::fs::read(&path).unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["matcher"]
            .as_str()
            .unwrap()
            .contains("Bash"));
    }

    #[test]
    fn install_is_idempotent() {
        let path = tmp_settings("");
        install(&path, "/cmd").unwrap();
        let again = install(&path, "/cmd").unwrap();
        assert!(!again);
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn install_preserves_existing_unrelated_hooks() {
        let path = tmp_settings(
            r#"{
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Read", "hooks": [{ "type": "command", "command": "/other" }] }
                    ]
                }
            }"#,
        );
        install(&path, "/cmd").unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn uninstall_removes_only_phantomdep_entry() {
        let path = tmp_settings(
            r#"{
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Read", "hooks": [{ "type": "command", "command": "/other" }] }
                    ]
                }
            }"#,
        );
        install(&path, "/cmd").unwrap();
        let removed = uninstall(&path).unwrap();
        assert!(removed);
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["matcher"], "Read");
    }

    #[test]
    fn uninstall_when_not_installed_is_noop() {
        let path = tmp_settings(r#"{"hooks":{"PreToolUse":[]}}"#);
        let removed = uninstall(&path).unwrap();
        assert!(!removed);
    }
}
