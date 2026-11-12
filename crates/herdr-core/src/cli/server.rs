//! `hc server …`: starting, stopping and restarting the core.

use super::*;

#[derive(Subcommand)]
pub(super) enum ServerCmd {
    /// Start the core with this shell's environment for new terminals.
    ///
    /// A core that any other command, the app or a plugin starts takes the service manager's environment instead: launchd's on macOS, the systemd user manager's on Linux, the user's settings on Windows. Where no service manager starts it (an ssh login on macOS, Linux without a systemd user manager), it takes its starter's.
    Start,
    Stop,
    Status,
    /// Restart the core (e.g. after an update) with its own environment; programs keep running.
    Restart {
        /// Take this shell's environment for new terminals; run from a terminal outside Herdr.
        #[arg(long)]
        fresh: bool,
    },
}

fn caller_environment() -> std::collections::BTreeMap<String, String> {
    handed_over(crate::platform::utf8_vars())
}

/// A login profile's `hc` runs inside `linux_login`'s capture, whose mark is not the shell's own.
pub(super) fn handed_over(vars: impl Iterator<Item = (String, String)>) -> std::collections::BTreeMap<String, String> {
    vars.filter(|(k, _)| !k.starts_with('=') && k != "HC_LOGIN_CAPTURE").collect()
}

/// Only an explicit start gives a new core this shell's environment; a core a
/// client starts on its own takes the service manager's.
pub(super) fn start(paths: &Paths) -> Result<(), RpcError> {
    if client::connect_existing(paths).is_some() {
        return Ok(());
    }
    if std::env::var_os("HC_TERMINAL_ID").is_some() {
        eprintln!(
            "hc: this shell carries Herdr's environment; start the core from a terminal outside Herdr or unset unwanted variables first"
        );
    }
    match crate::restart::prepare_start(paths, caller_environment()) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            eprintln!(
                "hc: a core is already starting with its own environment; hc server restart --fresh gives it this shell's"
            );
            Ok(())
        }
        result => result.map_err(|e| {
            RpcError::new("unavailable", e.to_string(), "set PATH, or let any hc command start the core on its own")
        }),
    }
}

pub(super) fn restart(paths: &Paths, fresh: bool) -> Result<(), RpcError> {
    if fresh && std::env::var_os("HC_TERMINAL_ID").is_some() {
        eprintln!(
            "hc: this shell carries Herdr's environment; run --fresh from a terminal outside Herdr or unset unwanted variables first"
        );
    }
    let environment = caller_environment();
    let Some(stream) = client::connect_existing(paths) else {
        if fresh {
            crate::restart::prepare_start(paths, environment).map_err(|e| {
                RpcError::new("unavailable", e.to_string(), "retry hc server restart --fresh once the core is running")
            })?;
        }
        return Ok(());
    };
    let params = if fresh {
        let hello = client::call(&stream, "hello", json!({}))?;
        if !hello["features"].as_array().is_some_and(|f| f.contains(&json!("restart_environment"))) {
            return Err(RpcError::new(
                "refused",
                "--fresh needs a newer running core",
                "run hc update, then retry hc server restart --fresh",
            ));
        }
        json!({"environment": environment})
    } else {
        json!({})
    };
    client::call(&stream, "server.restart", params)?;
    wait_closed(&stream);
    Ok(())
}

/// Stops the core and returns once it has exited (its connection closes).
pub(super) fn stop(paths: &Paths) -> Result<Out, RpcError> {
    let Some(stream) = client::connect_existing(paths) else {
        crate::restart::discard_if_stopped(paths);
        return Ok(Out { json: json!({"stopped": false}), human: "no core running".into() });
    };
    let r = client::call(&stream, "server.stop", json!({}))?;
    wait_closed(&stream);
    Ok(Out { json: r, human: "core stopped".into() })
}

/// The core closes every connection when its process exits.
pub(super) fn wait_closed(stream: &crate::platform::Stream) {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let _ = (&*stream).read_to_end(&mut Vec::new());
}
