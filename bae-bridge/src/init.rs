use std::sync::Arc;

use bae_core::app::{bootstrap, BootstrapError, RunningApp};
use bae_core::diagnostics::{DiagnosticLevel, Diagnostics, DiagnosticsConfig, DiagnosticsError};

use crate::handle::AppHandle;
use crate::types::BridgeError;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDiagnosticsConfig {
    pub enabled: bool,
    pub datadog_site: Option<String>,
    pub client_token: Option<String>,
    pub source: Option<String>,
    pub service: Option<String>,
    pub environment: Option<String>,
    pub app_version: Option<String>,
    pub edition: Option<String>,
    pub git_commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDiagnosticLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDiagnosticField {
    pub key: String,
    pub value: String,
}

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

#[uniffi::export]
pub fn configure_diagnostics(config: BridgeDiagnosticsConfig) -> Result<(), BridgeError> {
    configure_logging();
    Diagnostics::install_global(config.into_core()).map_err(diagnostics_error_to_bridge)?;
    Ok(())
}

#[uniffi::export]
pub fn diagnostics_log(
    level: BridgeDiagnosticLevel,
    target: String,
    message: String,
    fields: Vec<BridgeDiagnosticField>,
) {
    Diagnostics::global().log(level.into_core(), target, message, bridge_fields(fields));
}

#[uniffi::export]
pub fn diagnostics_event(name: String, fields: Vec<BridgeDiagnosticField>) {
    Diagnostics::global().event(name, bridge_fields(fields));
}

#[uniffi::export]
pub async fn flush_diagnostics() -> bool {
    Diagnostics::global().flush().await
}

impl BridgeDiagnosticsConfig {
    fn into_core(self) -> DiagnosticsConfig {
        let Some(datadog_site) = diagnostic_config_value(self.datadog_site) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(client_token) = diagnostic_config_value(self.client_token) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(source) = diagnostic_config_value(self.source) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(service) = diagnostic_config_value(self.service) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(environment) = diagnostic_config_value(self.environment) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(app_version) = diagnostic_config_value(self.app_version) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(edition) = diagnostic_config_value(self.edition) else {
            return DiagnosticsConfig::disabled();
        };
        let Some(git_commit) = diagnostic_config_value(self.git_commit) else {
            return DiagnosticsConfig::disabled();
        };

        if !self.enabled {
            return DiagnosticsConfig::disabled();
        }

        DiagnosticsConfig {
            enabled: true,
            datadog_site,
            client_token,
            source,
            service,
            environment,
            app_version,
            edition,
            git_commit,
        }
    }
}

fn diagnostic_config_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl BridgeDiagnosticLevel {
    fn into_core(self) -> DiagnosticLevel {
        match self {
            Self::Trace => DiagnosticLevel::Trace,
            Self::Debug => DiagnosticLevel::Debug,
            Self::Info => DiagnosticLevel::Info,
            Self::Warn => DiagnosticLevel::Warn,
            Self::Error => DiagnosticLevel::Error,
        }
    }
}

fn bridge_fields(fields: Vec<BridgeDiagnosticField>) -> impl Iterator<Item = (String, String)> {
    fields.into_iter().map(|field| (field.key, field.value))
}

fn diagnostics_error_to_bridge(e: DiagnosticsError) -> BridgeError {
    BridgeError::internal(format!("diagnostics setup failed: {e}"))
}

fn bootstrap_error_to_bridge(e: BootstrapError) -> BridgeError {
    match e {
        BootstrapError::LibraryNotFound(id) => BridgeError::NotFound {
            entity: crate::types::BridgeEntityKind::Library,
            id,
        },
        BootstrapError::Config(msg) => BridgeError::config(msg),
        BootstrapError::Database(msg) => BridgeError::database(msg),
        BootstrapError::Internal(msg) => BridgeError::internal(msg),
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
    if let Err(_already_installed) = subscriber.try_init() {}
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
            .with(bae_core::diagnostics::tracing_layer())
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
            .with(bae_core::diagnostics::tracing_layer())
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
            .with(bae_core::diagnostics::tracing_layer())
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
            .with(bae_core::diagnostics::tracing_layer())
            .with(fmt_layer),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_diagnostics_config_disables_sending() {
        let config = BridgeDiagnosticsConfig {
            enabled: true,
            datadog_site: Some("datadoghq.com".to_string()),
            client_token: None,
            source: Some("ios".to_string()),
            service: Some("bae".to_string()),
            environment: Some("test".to_string()),
            app_version: Some("1.2.3".to_string()),
            edition: Some("bae".to_string()),
            git_commit: Some("abc123".to_string()),
        }
        .into_core();

        assert!(!config.sends_events());
    }

    #[test]
    fn complete_diagnostics_config_sends_events() {
        let config = BridgeDiagnosticsConfig {
            enabled: true,
            datadog_site: Some("datadoghq.com".to_string()),
            client_token: Some("client-token".to_string()),
            source: Some("ios".to_string()),
            service: Some("bae".to_string()),
            environment: Some("test".to_string()),
            app_version: Some("1.2.3".to_string()),
            edition: Some("bae".to_string()),
            git_commit: Some("abc123".to_string()),
        }
        .into_core();

        assert!(config.sends_events());
        assert_eq!(config.source, "ios");
        assert_eq!(config.edition, "bae");
    }

    #[test]
    fn bridge_diagnostic_level_maps_to_core() {
        assert_eq!(
            BridgeDiagnosticLevel::Trace.into_core(),
            DiagnosticLevel::Trace
        );
        assert_eq!(
            BridgeDiagnosticLevel::Debug.into_core(),
            DiagnosticLevel::Debug
        );
        assert_eq!(
            BridgeDiagnosticLevel::Info.into_core(),
            DiagnosticLevel::Info
        );
        assert_eq!(
            BridgeDiagnosticLevel::Warn.into_core(),
            DiagnosticLevel::Warn
        );
        assert_eq!(
            BridgeDiagnosticLevel::Error.into_core(),
            DiagnosticLevel::Error
        );
    }
}
