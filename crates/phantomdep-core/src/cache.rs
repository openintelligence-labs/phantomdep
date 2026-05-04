use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::checker::PackageRecord;
use crate::verdict::Ecosystem;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS package_cache (
    ecosystem TEXT NOT NULL,
    name      TEXT NOT NULL,
    payload   TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (ecosystem, name)
);
"#;

/// TTL for confirmed registry hits — long, because real packages don't disappear often.
const TTL_FOUND: u64 = 24 * 3600;
/// TTL for 404s — short, because freshly-registered slop-squats appear fast and we want to detect them.
const TTL_MISSING: u64 = 5 * 60;

pub struct PackageCache {
    conn: Mutex<Connection>,
}

impl PackageCache {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating cache dir {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening cache db at {}", path.display()))?;
        conn.execute_batch(SCHEMA).context("initialising cache schema")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&default_cache_path()?)
    }

    pub fn get(&self, ecosystem: Ecosystem, name: &str) -> Result<Option<PackageRecord>> {
        let key = normalize_key(ecosystem, name);
        let conn = self.conn.lock().expect("cache mutex poisoned");
        let mut stmt = conn
            .prepare_cached(
                "SELECT payload, fetched_at FROM package_cache WHERE ecosystem = ?1 AND name = ?2",
            )
            .context("preparing cache select")?;
        let row: Option<(String, i64)> = stmt
            .query_row(params![ecosystem.as_str(), key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok();
        let Some((payload, fetched_at)) = row else {
            return Ok(None);
        };
        let record: PackageRecord = serde_json::from_str(&payload).context("decoding cached record")?;
        let age = now_secs().saturating_sub(fetched_at as u64);
        let ttl = if record.exists { TTL_FOUND } else { TTL_MISSING };
        if age > ttl {
            return Ok(None);
        }
        Ok(Some(record))
    }

    pub fn put(&self, record: &PackageRecord) -> Result<()> {
        let key = normalize_key(record.ecosystem, &record.name);
        let conn = self.conn.lock().expect("cache mutex poisoned");
        let payload = serde_json::to_string(record).context("encoding record for cache")?;
        conn.execute(
            "INSERT INTO package_cache (ecosystem, name, payload, fetched_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(ecosystem, name) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
            params![record.ecosystem.as_str(), key, payload, now_secs() as i64],
        )?;
        Ok(())
    }
}

/// Normalize the cache key per ecosystem case-sensitivity rules.
///
/// - PyPI: case-insensitive per PEP 503 normalisation (also folds underscores
///   to dashes; we keep underscores for now to match what the user asked for —
///   only case-fold).
/// - npm: technically case-sensitive but the registry rejects mixed case; lower
///   for safety.
/// - crates.io: case-insensitive lookups, lowercase the canonical form.
/// - Go modules: case-sensitive in path segments — keep as-is.
/// - Maven: case-sensitive — keep as-is.
fn normalize_key(eco: Ecosystem, name: &str) -> String {
    match eco {
        Ecosystem::Pypi | Ecosystem::Npm | Ecosystem::Cargo => name.to_ascii_lowercase(),
        Ecosystem::Go | Ecosystem::Maven => name.to_string(),
    }
}

fn default_cache_path() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!("no cache dir available"))?;
    Ok(dir.join("phantomdep").join("cache.db"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::PackageRecord;
    use crate::verdict::Ecosystem;

    fn tmp_cache() -> PackageCache {
        let dir = tempdir();
        PackageCache::open(&dir.join("cache.db")).unwrap()
    }

    fn tmp_record(name: &str, exists: bool) -> PackageRecord {
        let mut r = PackageRecord::missing(name, Ecosystem::Pypi);
        r.exists = exists;
        r
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("phantomdep-test-{}", now_secs()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn round_trips_a_record() {
        let cache = tmp_cache();
        let record = tmp_record("requests", true);
        cache.put(&record).unwrap();
        let got = cache.get(Ecosystem::Pypi, "requests").unwrap().unwrap();
        assert!(got.exists);
        assert_eq!(got.name, "requests");
    }

    #[test]
    fn miss_returns_none() {
        let cache = tmp_cache();
        assert!(cache.get(Ecosystem::Pypi, "nope").unwrap().is_none());
    }

    #[test]
    fn pypi_lookups_are_case_insensitive() {
        let cache = tmp_cache();
        let mut record = tmp_record("Requests", true);
        record.exists = true;
        cache.put(&record).unwrap();
        // Lowercased key should hit.
        assert!(cache.get(Ecosystem::Pypi, "requests").unwrap().is_some());
        // Uppercased key should also hit (same normalised form).
        assert!(cache.get(Ecosystem::Pypi, "REQUESTS").unwrap().is_some());
    }

    #[test]
    fn go_lookups_are_case_sensitive() {
        let cache = tmp_cache();
        let mut record = PackageRecord::missing("github.com/AlecAivazis/survey", Ecosystem::Go);
        record.exists = true;
        cache.put(&record).unwrap();
        assert!(cache
            .get(Ecosystem::Go, "github.com/AlecAivazis/survey")
            .unwrap()
            .is_some());
        assert!(cache
            .get(Ecosystem::Go, "github.com/alecaivazis/survey")
            .unwrap()
            .is_none());
    }
}
