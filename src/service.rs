use std::{fs, path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

use crate::config;

#[cfg(target_os = "linux")]
const SERVICE_UNIT: &str = "zproxy.service";

pub fn install(paths: &config::AppPaths) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        install_macos(paths)
    }

    #[cfg(target_os = "linux")]
    {
        install_linux(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        bail!("install no está soportado en esta plataforma")
    }
}

pub fn start(paths: &config::AppPaths) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        start_macos(paths)
    }

    #[cfg(target_os = "linux")]
    {
        start_linux(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        bail!("start no está soportado en esta plataforma")
    }
}

pub fn restart(paths: &config::AppPaths) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        restart_macos(paths)
    }

    #[cfg(target_os = "linux")]
    {
        restart_linux(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        bail!("restart no está soportado en esta plataforma")
    }
}

pub fn stop(paths: &config::AppPaths) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        stop_macos(paths)
    }

    #[cfg(target_os = "linux")]
    {
        stop_linux(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        bail!("stop no está soportado en esta plataforma")
    }
}

pub fn status(paths: &config::AppPaths) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        status_macos(paths)
    }

    #[cfg(target_os = "linux")]
    {
        status_linux(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        Ok("unsupported".to_string())
    }
}

pub fn logs(paths: &config::AppPaths, lines: usize, follow: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        logs_macos(paths, lines, follow)
    }

    #[cfg(target_os = "linux")]
    {
        logs_linux(paths, lines, follow)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (paths, lines, follow);
        bail!("logs no está soportado en esta plataforma")
    }
}

pub fn tail_file(path: &std::path::Path, lines: usize, follow: bool) -> Result<()> {
    if !path.exists() {
        bail!("no existe el fichero de log {}", path.display());
    }

    let mut args: Vec<String> = vec!["-n".to_string(), lines.to_string()];
    if follow {
        args.push("-f".to_string());
    }
    args.push(path.to_string_lossy().to_string());

    run_cmd_stream("tail", &args)
}

pub fn uninstall(paths: &config::AppPaths) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        uninstall_macos(paths)
    }

    #[cfg(target_os = "linux")]
    {
        uninstall_linux(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        bail!("uninstall no está soportado en esta plataforma")
    }
}

pub fn is_installed(paths: &config::AppPaths) -> Result<bool> {
    #[cfg(target_os = "macos")]
    {
        Ok(launch_agent_plist(paths)?.exists())
    }

    #[cfg(target_os = "linux")]
    {
        Ok(systemd_user_unit(paths)?.exists())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = paths;
        Ok(false)
    }
}

#[cfg(target_os = "macos")]
fn install_macos(paths: &config::AppPaths) -> Result<()> {
    let plist_path = launch_agent_plist(paths)?;
    let exe = std::env::current_exe().context("no se pudo resolver la ruta del ejecutable")?;
    let home = dirs::home_dir().context("no se pudo resolver HOME")?;

    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)?;
    }
    paths.ensure_dirs()?;

    let out_log = paths.state_dir.join("launchd.out.log");
    let err_log = paths.state_dir.join("launchd.err.log");
    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{label}</string>

    <key>ProgramArguments</key>
    <array>
      <string>{exe}</string>
      <string>daemon</string>
    </array>

    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>

    <key>WorkingDirectory</key>
    <string>{home}</string>

    <key>StandardOutPath</key>
    <string>{out_log}</string>
    <key>StandardErrorPath</key>
    <string>{err_log}</string>
  </dict>
</plist>
"#,
        label = service_label(),
        exe = xml_escape(&exe.to_string_lossy()),
        home = xml_escape(&home.to_string_lossy()),
        out_log = xml_escape(&out_log.to_string_lossy()),
        err_log = xml_escape(&err_log.to_string_lossy()),
    );

    fs::write(&plist_path, plist_content)
        .with_context(|| format!("no se pudo escribir {}", plist_path.display()))?;

    let domain = launchctl_domain();
    let target = format!("{}/{}", domain, service_label());
    let plist = plist_path.to_string_lossy().to_string();

    let _ = Command::new("launchctl")
        .args(["bootout", &domain, &plist])
        .status();

    run_cmd("launchctl", &["bootstrap", &domain, &plist])?;
    run_cmd("launchctl", &["enable", &target])?;

    println!("servicio instalado en {}", plist_path.display());
    println!(
        "ejecutable configurado: {} (recomendado usar un path estable, no temporal de build)",
        exe.display()
    );

    Ok(())
}

#[cfg(target_os = "macos")]
fn start_macos(paths: &config::AppPaths) -> Result<()> {
    let plist_path = launch_agent_plist(paths)?;
    if !plist_path.exists() {
        bail!("no existe LaunchAgent en {}", plist_path.display());
    }

    let target = format!("{}/{}", launchctl_domain(), service_label());
    run_cmd("launchctl", &["kickstart", "-k", &target])
}

#[cfg(target_os = "macos")]
fn restart_macos(paths: &config::AppPaths) -> Result<()> {
    start_macos(paths)
}

#[cfg(target_os = "macos")]
fn stop_macos(paths: &config::AppPaths) -> Result<()> {
    let plist_path = launch_agent_plist(paths)?;
    if !plist_path.exists() {
        bail!("no existe LaunchAgent en {}", plist_path.display());
    }

    let domain = launchctl_domain();
    let target = format!("{}/{}", domain, service_label());
    run_cmd("launchctl", &["kill", "SIGTERM", &target])
}

#[cfg(target_os = "macos")]
fn status_macos(paths: &config::AppPaths) -> Result<String> {
    let installed = is_installed(paths)?;
    if !installed {
        return Ok("installed=false running=false".to_string());
    }

    let target = format!("{}/{}", launchctl_domain(), service_label());
    let output = Command::new("launchctl")
        .args(["print", &target])
        .output()
        .with_context(|| "falló launchctl print")?;

    if !output.status.success() {
        return Ok("installed=true running=false".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let state = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("state = "))
        .unwrap_or("unknown");
    let running = state == "running";
    Ok(format!(
        "installed=true running={} state={}",
        running, state
    ))
}

#[cfg(target_os = "macos")]
fn logs_macos(paths: &config::AppPaths, lines: usize, follow: bool) -> Result<()> {
    let out_log = paths.state_dir.join("launchd.out.log");
    let err_log = paths.state_dir.join("launchd.err.log");

    if !out_log.exists() && !err_log.exists() {
        bail!(
            "no se encontraron logs en {}",
            paths.state_dir.to_string_lossy()
        );
    }

    let mut args: Vec<String> = vec!["-n".to_string(), lines.to_string()];
    if follow {
        args.push("-f".to_string());
    }
    if out_log.exists() {
        args.push(out_log.to_string_lossy().to_string());
    }
    if err_log.exists() {
        args.push(err_log.to_string_lossy().to_string());
    }

    run_cmd_stream("tail", &args)
}

#[cfg(target_os = "macos")]
fn uninstall_macos(paths: &config::AppPaths) -> Result<()> {
    let plist_path = launch_agent_plist(paths)?;
    let plist = plist_path.to_string_lossy().to_string();
    let domain = launchctl_domain();

    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["bootout", &domain, &plist])
            .status();
        fs::remove_file(&plist_path)
            .with_context(|| format!("no se pudo borrar {}", plist_path.display()))?;
    }

    let target = format!("{}/{}", domain, service_label());
    let _ = Command::new("launchctl")
        .args(["disable", &target])
        .status();

    println!("servicio desinstalado ({})", plist_path.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_agent_plist(_paths: &config::AppPaths) -> Result<PathBuf> {
    let home = dirs::home_dir().context("no se pudo resolver HOME")?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", service_label())))
}

#[cfg(target_os = "macos")]
fn launchctl_domain() -> String {
    format!("gui/{}", nix_uid())
}

#[cfg(target_os = "macos")]
fn nix_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and returns current effective uid.
    unsafe { libc::geteuid() as u32 }
}

#[cfg(target_os = "linux")]
fn install_linux(paths: &config::AppPaths) -> Result<()> {
    let unit_path = systemd_user_unit(paths)?;
    let exe = std::env::current_exe().context("no se pudo resolver la ruta del ejecutable")?;
    let home = dirs::home_dir().context("no se pudo resolver HOME")?;

    if let Some(parent) = unit_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let unit = format!(
        "[Unit]\nDescription=zproxy local proxy daemon\nAfter=network-online.target\n\n[Service]\nType=simple\nExecStart={} daemon\nWorkingDirectory={}\nRestart=always\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n",
        exe.display(),
        home.display(),
    );
    fs::write(&unit_path, unit)
        .with_context(|| format!("no se pudo escribir {}", unit_path.display()))?;

    run_cmd("systemctl", &["--user", "daemon-reload"])?;
    run_cmd("systemctl", &["--user", "enable", SERVICE_UNIT])?;

    println!("servicio instalado en {}", unit_path.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn start_linux(_paths: &config::AppPaths) -> Result<()> {
    run_cmd("systemctl", &["--user", "start", SERVICE_UNIT])
}

#[cfg(target_os = "linux")]
fn restart_linux(_paths: &config::AppPaths) -> Result<()> {
    run_cmd("systemctl", &["--user", "restart", SERVICE_UNIT])
}

#[cfg(target_os = "linux")]
fn stop_linux(_paths: &config::AppPaths) -> Result<()> {
    run_cmd("systemctl", &["--user", "stop", SERVICE_UNIT])
}

#[cfg(target_os = "linux")]
fn status_linux(paths: &config::AppPaths) -> Result<String> {
    let installed = is_installed(paths)?;
    if !installed {
        return Ok("installed=false running=false".to_string());
    }

    let output = Command::new("systemctl")
        .args(["--user", "is-active", SERVICE_UNIT])
        .output()
        .with_context(|| "falló systemctl --user is-active")?;

    let active = output.status.success();
    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(format!("installed=true running={} state={}", active, state))
}

#[cfg(target_os = "linux")]
fn logs_linux(_paths: &config::AppPaths, lines: usize, follow: bool) -> Result<()> {
    let mut args: Vec<String> = vec![
        "--user".to_string(),
        "-u".to_string(),
        SERVICE_UNIT.to_string(),
        "-n".to_string(),
        lines.to_string(),
    ];
    if follow {
        args.push("-f".to_string());
    }

    run_cmd_stream("journalctl", &args)
}

#[cfg(target_os = "linux")]
fn uninstall_linux(paths: &config::AppPaths) -> Result<()> {
    let unit_path = systemd_user_unit(paths)?;

    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", SERVICE_UNIT])
        .status();

    if unit_path.exists() {
        fs::remove_file(&unit_path)
            .with_context(|| format!("no se pudo borrar {}", unit_path.display()))?;
    }

    run_cmd("systemctl", &["--user", "daemon-reload"])?;
    println!("servicio desinstalado ({})", unit_path.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_user_unit(_paths: &config::AppPaths) -> Result<PathBuf> {
    let home = dirs::home_dir().context("no se pudo resolver HOME")?;
    Ok(home
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SERVICE_UNIT))
}

fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("falló al ejecutar {program}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    bail!(
        "{} {:?} devolvió error. stdout='{}' stderr='{}'",
        program,
        args,
        stdout.trim(),
        stderr.trim()
    )
}

fn run_cmd_stream(program: &str, args: &[String]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("falló al ejecutar {program}"))?;

    if status.success() {
        return Ok(());
    }

    bail!("{} {:?} devolvió código {}", program, args, status)
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn service_label() -> &'static str {
    "dev.zproxy"
}
