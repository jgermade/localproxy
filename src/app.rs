use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use fs2::FileExt;
use tokio::{signal, sync::RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{config, control, gateway, proxy};

#[derive(Clone)]
pub struct SharedState {
    pub paths: config::AppPaths,
    pub config: Arc<RwLock<config::AppConfig>>,
    pub gateway_ip: Arc<RwLock<Option<std::net::IpAddr>>>,
    pub shutdown: CancellationToken,
}

pub async fn run_daemon(paths: config::AppPaths) -> Result<()> {
    paths.ensure_dirs()?;
    let config = config::load_or_create(&paths)?;
    let pid_guard = PidGuard::acquire(&paths)?;

    let state = SharedState {
        paths: paths.clone(),
        config: Arc::new(RwLock::new(config)),
        gateway_ip: Arc::new(RwLock::new(None)),
        shutdown: CancellationToken::new(),
    };

    let gateway_state = state.clone();
    let gateway_task = tokio::spawn(async move {
        if let Err(error) = gateway::run(gateway_state).await {
            error!(%error, "gateway detector detenido con error");
        }
    });

    let control_state = state.clone();
    let control_task = tokio::spawn(async move {
        if let Err(error) = control::serve(control_state).await {
            error!(%error, "control socket detenido con error");
        }
    });

    let signal_shutdown = state.shutdown.clone();
    let signal_task = tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            signal_shutdown.cancel();
        }
    });

    let proxy_result = proxy::serve(state.clone()).await;
    state.shutdown.cancel();

    gateway_task.await?;
    control_task.await?;
    signal_task.abort();

    drop(pid_guard);
    proxy_result
}

pub struct PidGuard {
    lock_file: std::fs::File,
    pid_path: PathBuf,
}

impl PidGuard {
    pub fn acquire(paths: &config::AppPaths) -> Result<Self> {
        let lock_path = paths.lock_file();
        let mut lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("no se pudo abrir {}", lock_path.display()))?;

        lock_file
            .try_lock_exclusive()
            .map_err(|_| anyhow!("zproxy ya está corriendo o el lockfile está ocupado"))?;

        let pid = std::process::id();
        lock_file.set_len(0)?;
        writeln!(lock_file, "{pid}")?;
        fs::write(paths.pid_file(), pid.to_string())?;

        info!(pid, "daemon inicializado");

        Ok(Self {
            lock_file,
            pid_path: paths.pid_file(),
        })
    }
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = self.lock_file.unlock();
        let _ = fs::remove_file(&self.pid_path);
    }
}
