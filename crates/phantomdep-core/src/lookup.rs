use std::sync::Arc;

use anyhow::Result;

use crate::cache::PackageCache;
use crate::checker::PackageRecord;
use crate::crates_io::CratesClient;
use crate::go_proxy::GoProxyClient;
use crate::npm::NpmClient;
use crate::pypi::PypiClient;
use crate::verdict::Ecosystem;

/// High-level lookup that owns the cache and per-ecosystem clients.
pub struct Lookup {
    cache: Option<Arc<PackageCache>>,
    pypi: PypiClient,
    npm: NpmClient,
    crates: CratesClient,
    go: GoProxyClient,
}

impl Lookup {
    pub fn new(cache: Option<Arc<PackageCache>>) -> Result<Self> {
        Ok(Self {
            cache,
            pypi: PypiClient::new()?,
            npm: NpmClient::new()?,
            crates: CratesClient::new()?,
            go: GoProxyClient::new()?,
        })
    }

    pub async fn lookup(&self, name: &str, ecosystem: Ecosystem) -> Result<PackageRecord> {
        if let Some(cache) = &self.cache {
            if let Some(hit) = cache.get(ecosystem, name)? {
                return Ok(hit);
            }
        }
        let record = match ecosystem {
            Ecosystem::Pypi => self.pypi.lookup(name).await?,
            Ecosystem::Npm => self.npm.lookup(name).await?,
            Ecosystem::Cargo => self.crates.lookup(name).await?,
            Ecosystem::Go => self.go.lookup(name).await?,
            other => PackageRecord::missing(name, other),
        };
        if let Some(cache) = &self.cache {
            cache.put(&record).ok();
        }
        Ok(record)
    }
}
