//! Resource limits the daemon raises for itself when it starts.
//!
//! The most relevant one for a proxy that opens many concurrent connections is
//! the number of open file descriptors (RLIMIT_NOFILE, a.k.a. `ulimit -n`).
//! Optionally it can also raise RLIMIT_NPROC (`ulimit -u`).
//!
//! These are applied at the beginning of `run_daemon`, so they take effect in
//! every launch mode: foreground `run`, detached `start --detached`, and the
//! LaunchAgent / systemd service (which all exec `run`).

use std::mem;

use anyhow::{Context, Result};
use tracing::{info, warn};

/// Default target used when the hard limit is "unlimited", so we don't request
/// an absurd number of descriptors.
const DEFAULT_NOFILE_TARGET: u64 = 65_536;

/// Raises the process soft limit for the number of open file descriptors
/// (RLIMIT_NOFILE) up to `target`, capped at the hard limit.
///
/// Returns the resulting soft limit. If the kernel cannot reach the requested
/// value, the best achievable value is returned and a warning is logged.
#[cfg(unix)]
pub fn apply_nofile(target: u64) -> Result<u64> {
    apply_soft_limit(
        libc::RLIMIT_NOFILE as libc::c_int,
        target,
        DEFAULT_NOFILE_TARGET,
        "file descriptors",
    )
}

/// Raises the process soft limit for the number of processes/threads
/// (RLIMIT_NPROC) up to `target`, capped at the hard limit.
#[cfg(unix)]
pub fn apply_nproc(target: u64) -> Result<u64> {
    apply_soft_limit(
        libc::RLIMIT_NPROC as libc::c_int,
        target,
        DEFAULT_NOFILE_TARGET,
        "processes/threads",
    )
}

/// No-op on non-unix platforms.
#[cfg(not(unix))]
pub fn apply_nofile(_target: u64) -> Result<u64> {
    Ok(0)
}

/// No-op on non-unix platforms.
#[cfg(not(unix))]
pub fn apply_nproc(_target: u64) -> Result<u64> {
    Ok(0)
}

/// Computes the new soft limit to request, or `None` if no change is needed.
///
/// - The hard limit caps the request: a non-root process cannot raise the soft
///   limit above the hard limit.
/// - When the hard limit is "unlimited", the request is capped at
///   [`DEFAULT_NOFILE_TARGET`] so we don't ask for an absurd number of
///   descriptors.
/// - If the current soft limit is already at or above the target, returns `None`.
#[cfg(all(unix, test))]
fn resolve_nofile_target(soft: u64, hard: u64, target: u64) -> Option<u64> {
    rlim_t_to_u64(resolve_soft_limit_target(
        rlim_t_from_u64(soft),
        rlim_t_from_u64(hard),
        target,
        DEFAULT_NOFILE_TARGET,
    ))
}

#[cfg(unix)]
fn resolve_soft_limit_target(
    soft: libc::rlim_t,
    hard: libc::rlim_t,
    target: u64,
    unlimited_cap: u64,
) -> Option<libc::rlim_t> {
    let hard_cap = if hard == libc::RLIM_INFINITY as libc::rlim_t {
        rlim_t_from_u64(unlimited_cap)
    } else {
        hard
    };

    let new_soft = rlim_t_from_u64(target).min(hard_cap);
    if soft >= new_soft {
        None
    } else {
        Some(new_soft)
    }
}

#[cfg(unix)]
fn rlim_t_from_u64(value: u64) -> libc::rlim_t {
    #[cfg(target_pointer_width = "32")]
    {
        value.min(u64::from(libc::rlim_t::MAX)) as libc::rlim_t
    }

    #[cfg(not(target_pointer_width = "32"))]
    {
        value as libc::rlim_t
    }
}

#[cfg(all(unix, test))]
fn rlim_t_to_u64(value: Option<libc::rlim_t>) -> Option<u64> {
    #[cfg(target_pointer_width = "32")]
    {
        value.map(u64::from)
    }

    #[cfg(not(target_pointer_width = "32"))]
    {
        value
    }
}

#[cfg(unix)]
fn rlim_t_value_to_u64(value: libc::rlim_t) -> u64 {
    #[cfg(target_pointer_width = "32")]
    {
        u64::from(value)
    }

    #[cfg(not(target_pointer_width = "32"))]
    {
        value
    }
}

#[cfg(unix)]
fn apply_soft_limit(
    resource: libc::c_int,
    target: u64,
    unlimited_cap: u64,
    name: &str,
) -> Result<u64> {
    // SAFETY: getrlimit/setrlimit only touch the `lim` struct we own and use a
    // constant resource id. There are no preconditions beyond a valid resource.
    unsafe {
        let mut lim: libc::rlimit = mem::zeroed();
        if libc::getrlimit(resource as _, &mut lim) != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("no se pudo leer el límite actual de {name}"));
        }

        let soft = lim.rlim_cur;
        let hard = lim.rlim_max;

        let Some(new_soft) = resolve_soft_limit_target(soft, hard, target, unlimited_cap) else {
            return Ok(rlim_t_value_to_u64(soft));
        };

        lim.rlim_cur = new_soft;
        if libc::setrlimit(resource as _, &lim) != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("no se pudo subir el límite de {name}"));
        }

        let mut applied: libc::rlimit = mem::zeroed();
        if libc::getrlimit(resource as _, &mut applied) != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("no se pudo leer el límite de {name} tras el ajuste"));
        }

        if applied.rlim_cur < new_soft {
            warn!(
                requested = rlim_t_value_to_u64(new_soft),
                applied = rlim_t_value_to_u64(applied.rlim_cur),
                limit = name,
                "el kernel no pudo aplicar el límite solicitado"
            );
        }

        info!(
            soft = rlim_t_value_to_u64(applied.rlim_cur),
            limit = name,
            "límite ajustado"
        );
        Ok(rlim_t_value_to_u64(applied.rlim_cur))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raises_soft_up_to_target_when_below_hard() {
        // soft 1024, hard 65536, target 32768 -> request 32768.
        assert_eq!(resolve_nofile_target(1024, 65_536, 32_768), Some(32_768));
    }

    #[test]
    fn caps_request_at_hard_limit() {
        // soft 1024, hard 8192, target 65536 -> can only reach the hard limit.
        assert_eq!(resolve_nofile_target(1024, 8192, 65_536), Some(8192));
    }

    #[test]
    fn caps_unlimited_hard_at_default_target() {
        // soft 1024, hard unlimited, target 1_000_000 -> capped at default.
        assert_eq!(
            resolve_nofile_target(1024, libc::RLIM_INFINITY, 1_000_000),
            Some(DEFAULT_NOFILE_TARGET)
        );
    }

    #[test]
    fn no_change_when_soft_already_at_or_above_target() {
        assert_eq!(resolve_nofile_target(65_536, 65_536, 32_768), None);
        assert_eq!(resolve_nofile_target(65_536, 65_536, 65_536), None);
    }

    #[test]
    fn no_change_when_target_is_zero() {
        // A target of 0 never raises anything.
        assert_eq!(resolve_nofile_target(1024, 65_536, 0), None);
    }

    #[test]
    fn generic_target_resolution_matches_nofile_behavior() {
        assert_eq!(
            resolve_soft_limit_target(
                1024 as libc::rlim_t,
                libc::RLIM_INFINITY as libc::rlim_t,
                100_000,
                65_536,
            ),
            Some(65_536 as libc::rlim_t)
        );
        assert_eq!(
            resolve_soft_limit_target(8_192 as libc::rlim_t, 8_192 as libc::rlim_t, 65_536, 65_536),
            None
        );
    }
}
