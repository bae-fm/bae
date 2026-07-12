use std::sync::Arc;

use bae_core::app::BootstrapError;
#[cfg(not(feature = "desktop"))]
use bae_core::app::{bootstrap, RunningApp};
use bae_core::diagnostics::{
    AppDiagnosticMetadata, DatadogDiagnosticsConfig, Diagnostics, DiagnosticsConfig,
    DiagnosticsError, Screen, TelemetryEvent,
};

use crate::get_cloudkit_ops;
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
    pub app: BridgeAppDiagnosticMetadata,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAppDiagnosticMetadata {
    pub service: String,
    pub environment: String,
    pub app_version: String,
    pub edition: String,
    pub git_commit: String,
}

/// The host-emittable subset of the telemetry catalog. Hosts can only report
/// events they own (a screen open); core owns playback/import/sync, so a host
/// can't fabricate those. Mirrors the core catalog across the FFI boundary, the
/// same as every other `Bridge*` type.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTelemetryEvent {
    ScreenOpened { screen: BridgeScreen },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeScreen {
    Library,
    Settings,
}

#[uniffi::export]
/// `restore_playback` is the platform's "Restore on launch" preference: `true`
/// restores the saved queue/current track/position at startup, `false` starts
/// with nothing in playback. Platforms without the preference pass `true`
/// (mobile always resumes where playback left off).
///
/// `diagnostics` carries the Datadog config the host built (or `Disabled`);
/// telemetry is constructed inside bootstrap from it, so there is no separate
/// entry point to call first.
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: BridgeDiagnosticsConfig,
) -> Result<Arc<AppHandle>, BridgeError> {
    configure_logging()?;

    let diagnostics = diagnostics.into_core();

    #[cfg(feature = "desktop")]
    {
        let app = bae_desktop::bootstrap(
            library_id,
            position_update_interval_ms,
            restore_playback,
            diagnostics,
            get_cloudkit_ops(),
        )
        .map_err(bootstrap_error_to_bridge)?;
        Ok(Arc::new(AppHandle { app }))
    }

    #[cfg(not(feature = "desktop"))]
    let RunningApp {
        runtime,
        services,
        ui_event_bus,
        diagnostics,
    } = bootstrap(
        library_id,
        position_update_interval_ms,
        restore_playback,
        diagnostics,
        get_cloudkit_ops(),
    )
    .map_err(bootstrap_error_to_bridge)?;

    #[cfg(not(feature = "desktop"))]
    Ok(Arc::new(AppHandle {
        runtime,
        services,
        ui_event_bus,
        diagnostics,
    }))
}

impl AppHandle {
    #[cfg(feature = "desktop")]
    fn diagnostics_ref(&self) -> &Diagnostics {
        &self.app.diagnostics
    }

    #[cfg(not(feature = "desktop"))]
    fn diagnostics_ref(&self) -> &Diagnostics {
        &self.diagnostics
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Ship a host-originated telemetry event. Infallible — telemetry must
    /// never break the host UI; a stopped worker drops the event.
    pub fn telemetry(&self, event: BridgeTelemetryEvent) {
        self.diagnostics_ref().event(event.into_core());
    }

    /// Flush any buffered telemetry now. Hosts call this at exit so the last
    /// events reach Datadog before the process ends.
    pub async fn flush_diagnostics(&self) -> Result<(), BridgeError> {
        self.diagnostics_ref()
            .flush()
            .await
            .map_err(diagnostics_error_to_bridge)
    }
}

impl BridgeTelemetryEvent {
    fn into_core(self) -> TelemetryEvent {
        match self {
            Self::ScreenOpened { screen } => TelemetryEvent::ScreenOpened {
                screen: screen.into_core(),
            },
        }
    }
}

impl BridgeScreen {
    fn into_core(self) -> Screen {
        match self {
            Self::Library => Screen::Library,
            Self::Settings => Screen::Settings,
        }
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
        let BridgeDatadogDiagnosticsConfig {
            datadog_site,
            client_token,
            source,
            app,
        } = self;
        let BridgeAppDiagnosticMetadata {
            service,
            environment,
            app_version,
            edition,
            git_commit,
        } = app;
        let config = DatadogDiagnosticsConfig {
            datadog_site,
            client_token,
            source,
            app: AppDiagnosticMetadata {
                service,
                environment,
                app_version,
                edition,
                git_commit,
            },
        };

        DiagnosticsConfig::Enabled(config)
    }
}

fn diagnostics_error_to_bridge(e: DiagnosticsError) -> BridgeError {
    BridgeError::internal(format!("diagnostics failed: {e}"))
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
    ($($layer:expr),+ $(,)?) => {{
        use tracing_subscriber::prelude::*;
        let filter = env_filter()?;
        install_subscriber(
            tracing_subscriber::registry()
                .with(filter)
                $(.with($layer))+,
        );
        Ok(())
    }};
}

#[cfg(any(
    target_os = "macos",
    not(any(target_os = "macos", target_os = "android", target_os = "ios"))
))]
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
fn configure_logging() -> Result<(), BridgeError> {
    install_logging_subscriber!(
        fmt_log_layer(),
        tracing_oslog::OsLogger::new("fm.bae.desktop", "default"),
    )
}

#[cfg(target_os = "android")]
fn configure_logging() -> Result<(), BridgeError> {
    let android_layer = tracing_android::layer("bae").map_err(|error| {
        BridgeError::internal(format!(
            "Android tracing layer initialization failed: {error}"
        ))
    })?;
    install_logging_subscriber!(android_layer)
}

#[cfg(target_os = "ios")]
fn configure_logging() -> Result<(), BridgeError> {
    install_logging_subscriber!(tracing_oslog::OsLogger::new("fm.bae.app", "default"))
}

#[cfg(not(any(target_os = "macos", target_os = "android", target_os = "ios")))]
fn configure_logging() -> Result<(), BridgeError> {
    install_logging_subscriber!(fmt_log_layer())
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
                app: BridgeAppDiagnosticMetadata {
                    service: "bae".to_string(),
                    environment: "test".to_string(),
                    app_version: "1.2.3".to_string(),
                    edition: "bae".to_string(),
                    git_commit: "abc123".to_string(),
                },
            },
        }
        .into_core();

        assert!(config.sends_events());
        let DiagnosticsConfig::Enabled(config) = config else {
            panic!("complete bridge config must enable diagnostics");
        };
        assert_eq!(config.source, "ios");
        assert_eq!(config.app.edition, "bae");
    }

    #[test]
    fn host_telemetry_event_maps_to_the_core_catalog() {
        let core = BridgeTelemetryEvent::ScreenOpened {
            screen: BridgeScreen::Settings,
        }
        .into_core();

        assert_eq!(core.name(), "screen_opened");
        assert_eq!(
            core.fields()["screen"],
            serde_json::Value::String("settings".to_string())
        );
    }
}
