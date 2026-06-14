use std::sync::Arc;

use bae_core::app::{bootstrap, BootstrapError, RunningApp};

use crate::handle::AppHandle;
use crate::types::BridgeError;

#[uniffi::export]
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
) -> Result<Arc<AppHandle>, BridgeError> {
    configure_logging();

    let RunningApp {
        runtime,
        services,
        ui_event_bus,
    } = bootstrap(library_id, position_update_interval_ms).map_err(bootstrap_error_to_bridge)?;

    Ok(Arc::new(AppHandle {
        runtime,
        app_services: services,
        ui_event_bus,
    }))
}

fn bootstrap_error_to_bridge(e: BootstrapError) -> BridgeError {
    match e {
        BootstrapError::LibraryNotFound(id) => BridgeError::NotFound {
            msg: format!("Library '{id}' not found"),
        },
        BootstrapError::Config(msg) => BridgeError::Config { msg },
        BootstrapError::Database(msg) => BridgeError::Database { msg },
        BootstrapError::Internal(msg) => BridgeError::Internal { msg },
    }
}

/// Build an `EnvFilter` from `RUST_LOG`. If the variable is unset, defaults to "info".
/// If set but malformed, warns on stderr and falls back to "info".
fn env_filter() -> tracing_subscriber::EnvFilter {
    match std::env::var("RUST_LOG") {
        Err(_) => tracing_subscriber::EnvFilter::new("info"),
        Ok(val) => tracing_subscriber::EnvFilter::try_new(&val).unwrap_or_else(|e| {
            eprintln!("warning: RUST_LOG={val:?} is malformed ({e}), falling back to \"info\"");
            tracing_subscriber::EnvFilter::new("info")
        }),
    }
}

// Install the global subscriber. Ignores the "already initialized" error,
// which is the documented use-case for `try_init`.
fn install_subscriber(subscriber: impl tracing_subscriber::util::SubscriberInitExt) {
    let _ = subscriber.try_init();
}

#[cfg(target_os = "macos")]
fn configure_logging() {
    use tracing_subscriber::prelude::*;

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true);

    let oslog_layer = tracing_oslog::OsLogger::new("fm.bae.desktop", "default");

    install_subscriber(
        tracing_subscriber::registry()
            .with(env_filter())
            .with(fmt_layer)
            .with(oslog_layer),
    );
}

#[cfg(target_os = "android")]
fn configure_logging() {
    use tracing_subscriber::prelude::*;

    let android_layer = tracing_android::layer("bae").unwrap();

    install_subscriber(
        tracing_subscriber::registry()
            .with(env_filter())
            .with(android_layer),
    );
}

#[cfg(target_os = "ios")]
fn configure_logging() {
    use tracing_subscriber::prelude::*;

    let oslog_layer = tracing_oslog::OsLogger::new("fm.bae.app", "default");

    install_subscriber(
        tracing_subscriber::registry()
            .with(env_filter())
            .with(oslog_layer),
    );
}

#[cfg(not(any(target_os = "macos", target_os = "android", target_os = "ios")))]
fn configure_logging() {
    use tracing_subscriber::prelude::*;

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true);

    install_subscriber(
        tracing_subscriber::registry()
            .with(env_filter())
            .with(fmt_layer),
    );
}
