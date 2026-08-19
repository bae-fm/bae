use std::backtrace::Backtrace;
use std::sync::{Arc, Once};

use bae_core::app::{bootstrap, BootstrapError};
use bae_core::diagnostics::{
    AppDiagnosticMetadata, AppStartFailureKind, DatadogDiagnosticsConfig, Diagnostics,
    DiagnosticsConfig, DiagnosticsError, Screen, TelemetryEvent,
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

/// The process-lifetime telemetry sink, built once at startup and held by the
/// host for the whole app run. Wraps core's `Diagnostics`; it is the one
/// host-facing telemetry object (host events, exit flush), and `init_app` /
/// `init_keyring` require it as a parameter so telemetry is guaranteed set up
/// before anything that could fail.
#[derive(uniffi::Object)]
pub struct BridgeDiagnostics {
    inner: Diagnostics,
}

impl BridgeDiagnostics {
    pub(crate) fn init_keyring(&self) -> Result<(), BridgeError> {
        bae_core::config::init_keyring(&self.inner).map_err(|error| BridgeError::Diagnostic {
            category: crate::types::BridgeErrorCategory::Keyring,
            detail: error.to_string(),
        })
    }

    fn emit_app_start_failed(&self, error: &BootstrapError) {
        self.inner.event(TelemetryEvent::AppStartFailed {
            kind: app_start_failure_kind(error),
        });
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeDiagnostics {
    /// Ship a host-originated telemetry event. Infallible — telemetry must never
    /// break the host UI; a stopped worker drops the event.
    pub fn event(&self, event: BridgeTelemetryEvent) {
        self.inner.event(event.into_core());
    }

    /// Flush any buffered telemetry now. Hosts call this at exit so the last
    /// events reach Datadog before the process ends.
    pub async fn flush(self: Arc<Self>) -> Result<(), BridgeError> {
        crate::operation_runtime::run(
            crate::setup::onboarding_runtime_handle()?,
            move || async move {
                self.inner
                    .flush()
                    .await
                    .map_err(diagnostics_error_to_bridge)
            },
        )
        .await
    }
}

/// Build the telemetry sink and install the tracing subscriber. Called once at
/// process start, before `init_keyring` / `init_app` (both require the returned
/// handle), so the sink exists for every launch step that could fail.
///
/// Infallible by contract: telemetry setup must never block a launch. Sink
/// construction from an `Enabled` config can fail (incomplete config, worker
/// spawn); that is logged at `error` and the app continues with the no-op sink
/// — the one failure telemetry cannot report about itself, accepted. Handling
/// it here keeps the bailout in one place instead of a catch-and-retry in every
/// host language.
#[uniffi::export]
pub fn configure_diagnostics(config: BridgeDiagnosticsConfig) -> Arc<BridgeDiagnostics> {
    configure_logging();
    install_panic_logging();
    let clock = Arc::new(coven::SystemClock);
    let ids = Arc::new(coven::UuidProvider);
    let inner = match Diagnostics::configure(config.into_core(), clock, ids) {
        Ok(diagnostics) => diagnostics,
        Err(error) => {
            tracing::error!("telemetry init failed: {error}; continuing without telemetry");
            Diagnostics::noop()
        }
    };
    Arc::new(BridgeDiagnostics { inner })
}

fn install_panic_logging() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            let message = if let Some(message) = panic.payload().downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = panic.payload().downcast_ref::<String>() {
                message.clone()
            } else {
                "non-string panic payload".to_string()
            };
            let location = panic.location().map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            });
            let backtrace = Backtrace::force_capture();
            tracing::error!(%message, ?location, %backtrace, "process panicked");
            previous(panic);
        }));
    });
}

#[uniffi::export]
/// `restore_playback` is the platform's "Restore on launch" preference: `true`
/// restores the saved queue/current track/position at startup, `false` starts
/// with nothing in playback. Platforms without the preference pass `true`
/// (mobile always resumes where playback left off).
///
/// `diagnostics` is the sink from `configure_diagnostics`, built at process
/// start. Requiring it here makes "telemetry set up first" unrepresentable to
/// skip. A bootstrap failure ships `app_start_failed` through it before the
/// error returns.
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Arc<BridgeDiagnostics>,
) -> Result<Arc<AppHandle>, BridgeError> {
    let core = diagnostics.inner.clone();

    bootstrap(
        library_id,
        position_update_interval_ms,
        restore_playback,
        core,
        get_cloudkit_ops(),
        AppHandle::start,
    )
    .map(Arc::new)
    .map_err(|error| {
        diagnostics.emit_app_start_failed(&error);
        bootstrap_error_to_bridge(error)
    })
}

fn app_start_failure_kind(error: &BootstrapError) -> AppStartFailureKind {
    match error {
        BootstrapError::LibraryNotFound(_) => AppStartFailureKind::LibraryNotFound,
        BootstrapError::Config(_) => AppStartFailureKind::Config,
        BootstrapError::Database(_) => AppStartFailureKind::Database,
        BootstrapError::Internal(_) => AppStartFailureKind::Internal,
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

/// The log filter from `RUST_LOG`, plus a complaint to emit once the subscriber
/// is installed. `RUST_LOG` is a local debugging knob; a bad value degrades to
/// the default level instead of failing, because every host's telemetry
/// fallback relies on subscriber installation never failing — a launch must
/// never die over a malformed env var.
fn env_filter() -> (tracing_subscriber::EnvFilter, Option<String>) {
    let default = || tracing_subscriber::EnvFilter::new("info");
    match std::env::var("RUST_LOG") {
        Err(std::env::VarError::NotPresent) => (default(), None),
        Err(std::env::VarError::NotUnicode(raw)) => (
            default(),
            Some(format!(
                "RUST_LOG {raw:?} is not valid Unicode; logging at the default level"
            )),
        ),
        Ok(value) => match tracing_subscriber::EnvFilter::try_new(&value) {
            Ok(filter) => (filter, None),
            Err(e) => (
                default(),
                Some(format!(
                    "RUST_LOG={value:?} is malformed: {e}; logging at the default level"
                )),
            ),
        },
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
        let (filter, filter_complaint) = env_filter();
        install_subscriber(
            tracing_subscriber::registry()
                .with(filter)
                $(.with($layer))+,
        );
        // Emitted after install so it lands in the just-installed sinks.
        if let Some(complaint) = filter_complaint {
            tracing::warn!("{complaint}");
        }
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

/// Rolling file log under `~/.bae/logs/` (daily files, `bae.log.YYYY-MM-DD`),
/// kept alongside the console and system sinks so a desktop app launched from
/// the Finder or Dock — where stdout and stderr go nowhere — still leaves a
/// readable trace. `None` (with a stderr complaint) when the bae directory
/// can't be resolved: logging must never kill a launch.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn file_log_layer<S>() -> Option<impl tracing_subscriber::Layer<S>>
where
    S: tracing::Subscriber,
    for<'a> S: tracing_subscriber::registry::LookupSpan<'a>,
{
    // The non-blocking writer flushes from a worker thread that lives as long
    // as this guard; process-lifetime static, dropped never — the OS closes
    // the file at exit.
    static GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
        std::sync::OnceLock::new();
    let dir = match bae_core::config::bae_dir() {
        Ok(dir) => dir.join("logs"),
        Err(error) => {
            eprintln!("file log disabled: cannot resolve the bae directory: {error}");
            return None;
        }
    };
    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(dir, "bae.log"));
    let _ = GUARD.set(guard);
    Some(
        tracing_subscriber::fmt::layer()
            .with_line_number(true)
            .with_target(false)
            .with_file(true)
            .with_ansi(false)
            .with_writer(writer),
    )
}

#[cfg(target_os = "macos")]
fn configure_logging() {
    install_logging_subscriber!(
        fmt_log_layer(),
        file_log_layer(),
        tracing_oslog::OsLogger::new("fm.bae.desktop", "default"),
    )
}

#[cfg(target_os = "android")]
fn configure_logging() {
    let android_layer = match tracing_android::layer("bae") {
        Ok(layer) => layer,
        Err(error) => {
            // Nowhere structured to report this: logcat is the local sink and
            // building its layer is what failed. Local logs are lost; telemetry
            // (constructed by this function's caller) is unaffected, and launch
            // must not die over it.
            eprintln!("Android tracing layer initialization failed: {error}");
            return;
        }
    };
    install_logging_subscriber!(android_layer)
}

#[cfg(target_os = "ios")]
fn configure_logging() {
    install_logging_subscriber!(tracing_oslog::OsLogger::new("fm.bae.app", "default"))
}

#[cfg(target_os = "windows")]
fn configure_logging() {
    // ETW is Windows' unified logging: a TraceLogging provider named
    // "bae-core" (GUID derived from the name), captured with
    // `logman start ... -p "*bae-core"` — the `log stream` equivalent. The
    // fmt layer stays for console-attached runs.
    match tracing_etw::LayerBuilder::new("bae-core").build() {
        Ok(etw_layer) => {
            install_logging_subscriber!(fmt_log_layer(), file_log_layer(), etw_layer)
        }
        Err(error) => {
            // Nowhere structured to report this: ETW is the local sink and
            // building its layer is what failed. Console logging still works;
            // launch must not die over it.
            eprintln!("ETW tracing layer initialization failed: {error}");
            install_logging_subscriber!(fmt_log_layer(), file_log_layer())
        }
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "android",
    target_os = "ios",
    target_os = "windows",
)))]
fn configure_logging() {
    install_logging_subscriber!(fmt_log_layer(), file_log_layer())
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
    fn bootstrap_error_maps_to_failure_kind() {
        // Every BootstrapError variant maps to its app_start_failed kind; this is
        // the seam init_app emits through before returning the bridge error.
        assert_eq!(
            app_start_failure_kind(&BootstrapError::LibraryNotFound("x".to_string())),
            AppStartFailureKind::LibraryNotFound
        );
        assert_eq!(
            app_start_failure_kind(&BootstrapError::Config("x".to_string())),
            AppStartFailureKind::Config
        );
        assert_eq!(
            app_start_failure_kind(&BootstrapError::Database("x".to_string())),
            AppStartFailureKind::Database
        );
        assert_eq!(
            app_start_failure_kind(&BootstrapError::Internal("x".to_string())),
            AppStartFailureKind::Internal
        );
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

    #[test]
    fn detached_panics_are_emitted_through_tracing() {
        const CHILD_PROCESS: &str = "BAE_TEST_DETACHED_PANIC";
        const TEST_NAME: &str = "init::tests::detached_panics_are_emitted_through_tracing";

        if std::env::var_os(CHILD_PROCESS).is_some() {
            let subscriber = tracing_subscriber::fmt()
                .with_ansi(false)
                .without_time()
                .with_writer(std::io::stdout)
                .finish();
            tracing::subscriber::with_default(subscriber, || {
                configure_diagnostics(BridgeDiagnosticsConfig::Disabled);
                panic!("detached panic test");
            });
        }

        let output = std::process::Command::new(
            std::env::current_exe().expect("resolve the bridge test executable"),
        )
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_PROCESS, "1")
        .output()
        .expect("run the detached-panic child process");

        assert!(!output.status.success(), "the child process must panic");
        let logs = String::from_utf8(output.stdout).expect("panic logs are UTF-8");
        assert!(
            logs.contains("process panicked")
                && logs.contains("detached panic test")
                && logs.contains("bae-bridge/src/init.rs")
                && logs.contains("backtrace="),
            "the panic and its diagnostics must reach tracing, got: {logs}"
        );
    }
}
