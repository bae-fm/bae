use std::sync::Arc;

use bae_core::app::{bootstrap, BootstrapError, RunningApp};
use bae_core::diagnostics::{
    DatadogDiagnosticsConfig, DiagnosticLevel, Diagnostics, DiagnosticsConfig, DiagnosticsError,
};

use crate::handle::AppHandle;
use crate::types::BridgeError;

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiagnosticsConfig {
    Disabled,
    Enabled {
        config: BridgeDatadogDiagnosticsConfig,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDatadogDiagnosticsConfig {
    pub datadog_site: String,
    pub client_token: String,
    pub source: String,
    pub service: String,
    pub environment: String,
    pub app_version: String,
    pub edition: String,
    pub git_commit: String,
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

#[derive(uniffi::Object)]
pub struct BridgeDiagnostics {
    diagnostics: Diagnostics,
}

#[uniffi::export]
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
) -> Result<Arc<AppHandle>, BridgeError> {
    configure_logging(Diagnostics::noop())?;

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
pub fn configure_diagnostics(
    config: BridgeDiagnosticsConfig,
) -> Result<Arc<BridgeDiagnostics>, BridgeError> {
    let diagnostics =
        Diagnostics::configure(config.into_core()).map_err(diagnostics_error_to_bridge)?;
    configure_logging(diagnostics.clone())?;
    Ok(Arc::new(BridgeDiagnostics { diagnostics }))
}

#[uniffi::export]
impl BridgeDiagnostics {
    pub fn log(
        &self,
        level: BridgeDiagnosticLevel,
        target: String,
        message: String,
        fields: Vec<BridgeDiagnosticField>,
    ) -> Result<(), BridgeError> {
        self.diagnostics
            .log(level.into_core(), target, message, bridge_fields(fields))
            .map_err(diagnostics_error_to_bridge)
    }

    pub fn event(
        &self,
        name: String,
        fields: Vec<BridgeDiagnosticField>,
    ) -> Result<(), BridgeError> {
        self.diagnostics
            .event(name, bridge_fields(fields))
            .map_err(diagnostics_error_to_bridge)
    }

    pub async fn flush(&self) -> Result<(), BridgeError> {
        self.diagnostics
            .flush()
            .await
            .map_err(diagnostics_error_to_bridge)
    }
}

impl BridgeDiagnosticsConfig {
    fn into_core(self) -> DiagnosticsConfig {
        match self {
            Self::Disabled => DiagnosticsConfig::Disabled,
            Self::Enabled { config } => config.into_core(),
        }
    }
}

impl BridgeDatadogDiagnosticsConfig {
    fn into_core(self) -> DiagnosticsConfig {
        let config = DatadogDiagnosticsConfig {
            datadog_site: self.datadog_site.trim().to_string(),
            client_token: self.client_token.trim().to_string(),
            source: self.source.trim().to_string(),
            service: self.service.trim().to_string(),
            environment: self.environment.trim().to_string(),
            app_version: self.app_version.trim().to_string(),
            edition: self.edition.trim().to_string(),
            git_commit: self.git_commit.trim().to_string(),
        };

        DiagnosticsConfig::Enabled(config)
    }
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

fn env_filter() -> Result<tracing_subscriber::EnvFilter, BridgeError> {
    match std::env::var("RUST_LOG") {
        Err(std::env::VarError::NotPresent) => Ok(tracing_subscriber::EnvFilter::new("info")),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(BridgeError::config("RUST_LOG is not valid Unicode"))
        }
        Ok(value) => tracing_subscriber::EnvFilter::try_new(&value)
            .map_err(|e| BridgeError::config(format!("RUST_LOG={value:?} is malformed: {e}"))),
    }
}

// Install the global subscriber. Ignores the "already initialized" error,
// which is the documented use-case for `try_init`.
fn install_subscriber(subscriber: impl tracing_subscriber::util::SubscriberInitExt) {
    if let Err(error) = subscriber.try_init() {
        tracing::debug!(%error, "tracing subscriber already installed");
    }
}

macro_rules! install_logging_subscriber {
    ($diagnostics:expr $(, $layer:expr)+ $(,)?) => {{
        use tracing_subscriber::prelude::*;
        let filter = env_filter()?;
        install_subscriber(
            tracing_subscriber::registry()
                .with(filter)
                .with(bae_core::diagnostics::tracing_layer($diagnostics))
                $(.with($layer))+,
        );
        Ok(())
    }};
}

fn fmt_log_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber,
    for<'a> S: tracing_subscriber::registry::LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
}

#[cfg(target_os = "macos")]
fn configure_logging(diagnostics: Diagnostics) -> Result<(), BridgeError> {
    install_logging_subscriber!(
        diagnostics,
        fmt_log_layer(),
        tracing_oslog::OsLogger::new("fm.bae.desktop", "default"),
    )
}

#[cfg(target_os = "android")]
fn configure_logging(diagnostics: Diagnostics) -> Result<(), BridgeError> {
    install_logging_subscriber!(
        diagnostics,
        tracing_android::layer("bae").expect("Android tracing layer initializes"),
    )
}

#[cfg(target_os = "ios")]
fn configure_logging(diagnostics: Diagnostics) -> Result<(), BridgeError> {
    install_logging_subscriber!(
        diagnostics,
        tracing_oslog::OsLogger::new("fm.bae.app", "default"),
    )
}

#[cfg(not(any(target_os = "macos", target_os = "android", target_os = "ios")))]
fn configure_logging(diagnostics: Diagnostics) -> Result<(), BridgeError> {
    install_logging_subscriber!(diagnostics, fmt_log_layer())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_diagnostics_config_disables_sending() {
        let config = BridgeDiagnosticsConfig::Disabled.into_core();

        assert!(!config.sends_events());
    }

    #[test]
    fn complete_diagnostics_config_sends_events() {
        let config = BridgeDiagnosticsConfig::Enabled {
            config: BridgeDatadogDiagnosticsConfig {
                datadog_site: "datadoghq.com".to_string(),
                client_token: "client-token".to_string(),
                source: "ios".to_string(),
                service: "bae".to_string(),
                environment: "test".to_string(),
                app_version: "1.2.3".to_string(),
                edition: "bae".to_string(),
                git_commit: "abc123".to_string(),
            },
        }
        .into_core();

        assert!(config.sends_events());
        let DiagnosticsConfig::Enabled(config) = config else {
            panic!("complete bridge config must enable diagnostics");
        };
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
