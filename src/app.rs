use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
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

#[derive(Debug)]
pub struct PidGuard {
    lock_file: std::fs::File,
    pid_path: PathBuf,
}

impl PidGuard {
    pub fn acquire(paths: &config::AppPaths) -> Result<Self> {
        let lock_path = paths.lock_file();
        let mut lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("no se pudo abrir {}", lock_path.display()))?;

        lock_file
            .try_lock_exclusive()
            .map_err(|_| anyhow!("localproxy ya está corriendo o el lockfile está ocupado"))?;

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

pub fn start_detached(paths: &config::AppPaths) -> Result<u32> {
    paths.ensure_dirs()?;

    let exe = std::env::current_exe().context("no se pudo resolver la ruta del ejecutable")?;
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.log_file())?;
    let err_file = log_file.try_clone()?;

    let mut command = Command::new(exe);
    command
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(err_file));

    let child = command
        .spawn()
        .context("no se pudo arrancar localproxy daemon en background")?;

    Ok(child.id())
}
