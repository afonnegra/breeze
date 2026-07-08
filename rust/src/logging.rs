use tracing_appender::rolling;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use std::path::PathBuf;
use std::sync::Once;

use crate::paths::breeze_root;

static INIT: Once = Once::new();

pub fn init_logging() {
    INIT.call_once(|| {
        let log_dir = logs_dir();
        std::fs::create_dir_all(&log_dir).ok();
        let file_appender = rolling::daily(&log_dir, "inputvoice.log");

        let filter = EnvFilter::try_from_env("INPUTVOICE_LOG")
            .unwrap_or_else(|_| EnvFilter::new("info"));

        let pretty = std::env::var("INPUTVOICE_LOG_FORMAT")
            .map(|v| v != "json")
            .unwrap_or(true);

        let registry = tracing_subscriber::registry().with(filter);

        if pretty {
            registry
                .with(fmt::layer().with_ansi(true))
                .with(fmt::layer().with_writer(file_appender).json())
                .init();
        } else {
            registry
                .with(fmt::layer().json())
                .with(fmt::layer().with_writer(file_appender).json())
                .init();
        }
    });
}

/// Launcher-independent log directory (TD-010): the shared Breeze data
/// root (see [`crate::paths`]) plus a `logs` subfolder. Config resolves
/// its own file through the very same [`crate::paths::breeze_root`]
/// chain, so the two can never land under different roots.
fn logs_dir() -> PathBuf {
    breeze_root().join("logs")
}
