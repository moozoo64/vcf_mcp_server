//! Graceful shutdown on Ctrl+C (SIGINT) and, on Unix, SIGTERM.
//!
//! [`install`] registers the signal handlers and returns a token that is
//! cancelled when the first signal arrives. Each transport watches that token
//! and stops cleanly. After the first signal, a second Ctrl+C exits at once,
//! and so does a shutdown that takes longer than [`SHUTDOWN_TIMEOUT`].

use std::io::Write;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Longest time in-flight HTTP requests get to finish after a shutdown signal.
/// After this, every MCP stream still open is closed. Shutdown finishes sooner
/// if all requests complete first.
pub const REQUEST_DRAIN_PERIOD: Duration = Duration::from_secs(5);

/// Longest a graceful shutdown may take before the process exits anyway.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Exit code after a second Ctrl+C (128 + SIGINT, the shell convention).
const EXIT_INTERRUPTED: i32 = 130;

/// Write a line to stderr, ignoring errors.
///
/// `eprintln!` panics when stderr is closed, which happens when the client that
/// launched the server has exited. A panic here would kill the shutdown task
/// and leave a server that ignores Ctrl+C and SIGTERM.
pub fn log(message: impl std::fmt::Display) {
    let _ = writeln!(std::io::stderr(), "{message}");
}

/// Register the shutdown signal handlers and return a token that is cancelled
/// when the first signal arrives.
///
/// On Unix the handlers are registered before this returns, so no signal can
/// slip through between startup and the first poll.
pub fn install() -> std::io::Result<CancellationToken> {
    let shutdown = CancellationToken::new();

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            let name = tokio::select! {
                _ = sigint.recv() => "Ctrl+C",
                _ = sigterm.recv() => "SIGTERM",
            };
            begin(&shutdown, name);
            tokio::select! {
                _ = sigint.recv() => force_exit("second Ctrl+C"),
                _ = sigterm.recv() => force_exit("second SIGTERM"),
                _ = tokio::time::sleep(SHUTDOWN_TIMEOUT) => timed_out(),
            }
        });
    }

    #[cfg(not(unix))]
    {
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::signal::ctrl_c().await {
                log(format_args!("Warning: cannot listen for Ctrl+C: {e}"));
                return;
            }
            begin(&shutdown, "Ctrl+C");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => force_exit("second Ctrl+C"),
                _ = tokio::time::sleep(SHUTDOWN_TIMEOUT) => timed_out(),
            }
        });
    }

    Ok(shutdown)
}

fn begin(shutdown: &CancellationToken, signal_name: &str) {
    log(format_args!(
        "Received {signal_name}, shutting down (press Ctrl+C again to exit immediately)..."
    ));
    shutdown.cancel();
}

fn force_exit(reason: &str) -> ! {
    log(format_args!("Received {reason}, exiting immediately."));
    std::process::exit(EXIT_INTERRUPTED);
}

fn timed_out() -> ! {
    log(format_args!(
        "Shutdown did not finish within {}s, exiting.",
        SHUTDOWN_TIMEOUT.as_secs()
    ));
    std::process::exit(1);
}
