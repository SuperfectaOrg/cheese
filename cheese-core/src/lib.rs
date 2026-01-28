pub mod error;
pub mod fs;
pub mod cache;
pub mod security;
pub mod plugins;
pub mod config;
pub mod trash;
pub mod mounts;

pub use error::{Error, Result};

use std::sync::Arc;
use parking_lot::RwLock;
use tokio::runtime::Runtime;

pub struct CheeseCore {
    runtime: Arc<Runtime>,
    config: Arc<RwLock<config::Config>>,
}

impl CheeseCore {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("cheese-worker")
            .enable_all()
            .build()?;

        let config = config::Config::load()?;

        Ok(Self {
            runtime: Arc::new(runtime),
            config: Arc::new(RwLock::new(config)),
        })
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn config(&self) -> Arc<RwLock<config::Config>> {
        Arc::clone(&self.config)
    }
}

impl Default for CheeseCore {
    fn default() -> Self {
        Self::new().expect("Failed to initialize CheeseCore")
    }
}
