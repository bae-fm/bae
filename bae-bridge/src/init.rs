use std::backtrace::Backtrace;
use std::sync::{Arc, Once};

use bae_core::app::{bootstrap, BootstrapError};
use bae_core::diagnostics::{
    AppDiagnosticMetadata, AppStartFailureKind, DatadogDiagnosticsConfig, Diagnostics,
    DiagnosticsConfig, DiagnosticsError, Screen, TelemetryEvent,
};

use crate::handle::AppHandle;
use crate::types::BridgeError;
use tracing_subscriber::filter::LevelFilter;

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
/// host-facing telemetry object for host events. `init_keyring` and
/// `BridgeHost` (which `init_app` requires) take it as a parameter so telemetry
/// is guaranteed set up before anything that could fail.
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

    /// Flush any buffered telemetry now.
    pub(crate) async fn flush(&self) -> Result<(), BridgeError> {
        self.inner
            .flush()
            .await
            .map_err(diagnostics_error_to_bridge)
    }

    /// Bootstrap the library `library_id` with this sink. A bootstrap failure
    /// ships `app_start_failed` through it before the error returns.
    pub(crate) fn open_app(
        &self,
        app_dir: bae_core::config::AppDir,
        library_id: String,
        position_update_interval_ms: u32,
        restore_playback: bool,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        oauth_clients: coven::OAuthClients,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        bootstrap(
            app_dir,
            library_id,
            position_update_interval_ms,
            restore_playback,
            self.inner.clone(),
            cloudkit_ops,
            oauth_clients,
            AppHandle::start,
        )
        .map(Arc::new)
        .map_err(|error| {
            self.emit_app_start_failed(&error);
            bootstrap_error_to_bridge(error)
        })
    }

    fn emit_app_start_failed(&self, error: &BootstrapError) {
        self.inner.event(TelemetryEvent::AppStartFailed {
            kind: app_start_failure_kind(error),
        });
    }

    #[cfg(test)]
    pub(crate) fn noop() -> Arc<Self> {
        Arc::new(Self {
            inner: Diagnostics::noop(),
        })
    }
}

#[uniffi::export]
impl BridgeDiagnostics {
    /// Ship a host-originated telemetry event. Infallible — telemetry must never
    /// break the host UI; a stopped worker drops the event.
    pub fn event(&self, event: BridgeTelemetryEvent) {
        self.inner.event(event.into_core());
    }
}

/// Build the telemetry sink and install the tracing subscriber. Called once at
/// process start, before `init_keyring` / `BridgeHost` (both require the returned
/// handle), so the sink exists for every launch step that could fail. Local
/// logs go to the platform's native log (plus the terminal on the desktop).
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

fn app_start_failure_kind(error: &BootstrapError) -> AppStartFailureKind {
    match error {
        BootstrapError::LibraryNotFound(_) => AppStartFailureKind::LibraryNotFound,
        BootstrapError::Config(_) => AppStartFailureKind::Config,
        BootstrapError::Database(_) => AppStartFailureKind::Database,
        BootstrapError::Internal(_) => AppStartFailureKind::Internal,
        BootstrapError::KeyringUnavailable => AppStartFailureKind::KeyringUnavailable,
    }
}

mirror_enum! {
    BridgeTelemetryEvent = TelemetryEvent,
    into_core: fn,
    variants: { ScreenOpened { screen: (BridgeScreen) } },
}

mirror_enum! {
    BridgeScreen = Screen,
    into_core: fn,
    variants: { Library, Settings },
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
        // Its own category, not `Keyring`: that one reads as "the keyring
        // broke", and this one is "it is locked this second, try again after
        // you unlock". The host branches on the category to keep the handle
        // path open and retry, so the distinction has to survive the bridge.
        BootstrapError::KeyringUnavailable => BridgeError::diagnostic(
            crate::types::BridgeErrorCategory::KeyringLocked,
            "the OS keychain refused the read while the session is locked",
        ),
    }
}

/// One local log sink and the level bae's own crates record at when
/// `RUST_LOG` is unset. Levels are per sink because the sinks keep different
/// amounts: the unified log and ETW discard what no one is capturing, and a
/// terminal is someone watching, so they take `debug`; logcat and the journal
/// keep every line on the device, so they take `info`.
struct LogSink {
    layer: Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
    default_level: LevelFilter,
}

impl LogSink {
    fn new(
        layer: impl tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
        default_level: LevelFilter,
    ) -> Self {
        Self {
            layer: Box::new(layer),
            default_level,
        }
    }
}

/// The log targets that are bae's own: every workspace crate the apps link,
/// and coven, whose target also covers its `coven_*` crates (a directive
/// matches every target it prefixes).
const FIRST_PARTY_TARGETS: &[&str] = &[
    "bae_automation",
    "bae_bridge",
    "bae_cast",
    "bae_core",
    "bae_desktop",
    "bae_loc",
    "bae_mcp",
    "bae_mirror",
    "bae_subsonic",
    "coven",
];

/// A sink's filter directives when `RUST_LOG` is unset: bae's own crates at
/// the sink's level, everything else no finer than `info`. A dependency's
/// debug output is its internals — a SQL lexer reports every token it
/// consumes — and at `debug` it buries bae's lines.
fn default_directives(level: LevelFilter) -> String {
    std::iter::once(std::cmp::min(level, LevelFilter::INFO).to_string())
        .chain(
            FIRST_PARTY_TARGETS
                .iter()
                .map(|target| format!("{target}={level}")),
        )
        .collect::<Vec<_>>()
        .join(",")
}

/// The filter directives `RUST_LOG` sets for every sink, plus a complaint to
/// emit once the subscriber is installed. `None` leaves each sink at its own
/// defaults. `RUST_LOG` is a local debugging knob; a bad value degrades to
/// the default levels instead of failing, because every host's telemetry
/// fallback relies on subscriber installation never failing — a launch must
/// never die over a malformed env var.
fn rust_log() -> (Option<String>, Option<String>) {
    match std::env::var("RUST_LOG") {
        Err(std::env::VarError::NotPresent) => (None, None),
        Err(std::env::VarError::NotUnicode(raw)) => (
            None,
            Some(format!(
                "RUST_LOG {raw:?} is not valid Unicode; logging at the default levels"
            )),
        ),
        Ok(value) => match tracing_subscriber::EnvFilter::try_new(&value) {
            Ok(_) => (Some(value), None),
            Err(e) => (
                None,
                Some(format!(
                    "RUST_LOG={value:?} is malformed: {e}; logging at the default levels"
                )),
            ),
        },
    }
}

/// One sink's filter: `RUST_LOG` when it is set, the sink's defaults when not.
/// `rust_log` has already been parsed, so `new` sees only valid directives.
fn sink_filter(
    rust_log: Option<&str>,
    default_level: LevelFilter,
) -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::new(match rust_log {
        Some(rust_log) => rust_log.to_string(),
        None => default_directives(default_level),
    })
}

/// Install the global subscriber over `sinks`, each behind its own level
/// filter. Ignores the "already initialized" error, which is the documented
/// use-case for `try_init`.
fn install_logging(sinks: Vec<LogSink>) {
    use tracing_subscriber::prelude::*;
    let (rust_log, complaint) = rust_log();
    let layers: Vec<_> = sinks
        .into_iter()
        .map(|sink| {
            let filter = sink_filter(rust_log.as_deref(), sink.default_level);
            sink.layer.with_filter(filter).boxed()
        })
        .collect();
    if let Err(error) = tracing_subscriber::registry().with(layers).try_init() {
        tracing::debug!(%error, "tracing subscriber already installed");
    }
    // Emitted after install so it lands in the just-installed sinks.
    if let Some(complaint) = complaint {
        tracing::warn!("{complaint}");
    }
}

/// The terminal sink of the desktop apps: every line printed to stdout when
/// bae runs from a terminal, down to debug — a terminal run is someone
/// watching everything, and `RUST_LOG` quiets it. Mobile has no terminal.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn terminal_log_sink() -> LogSink {
    LogSink::new(
        tracing_subscriber::fmt::layer()
            .with_line_number(true)
            .with_target(false)
            .with_file(true),
        LevelFilter::DEBUG,
    )
}

/// The unified log, read with
/// `log stream --level debug --predicate 'subsystem == "fm.bae.desktop"'`.
#[cfg(target_os = "macos")]
fn configure_logging() {
    install_logging(vec![
        terminal_log_sink(),
        LogSink::new(
            tracing_oslog::OsLogger::new("fm.bae.desktop", "default"),
            LevelFilter::DEBUG,
        ),
    ])
}

/// logcat, read with `adb logcat -s bae`.
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
    install_logging(vec![LogSink::new(android_layer, LevelFilter::INFO)])
}

/// The unified log, read with
/// `log stream --level debug --predicate 'subsystem == "fm.bae.app"'`.
#[cfg(target_os = "ios")]
fn configure_logging() {
    install_logging(vec![LogSink::new(
        tracing_oslog::OsLogger::new("fm.bae.app", "default"),
        LevelFilter::DEBUG,
    )])
}

#[cfg(target_os = "windows")]
fn configure_logging() {
    // ETW is Windows' unified logging: a TraceLogging provider named
    // "bae-core" (GUID derived from the name), captured with
    // `logman start ... -p "*bae-core"` — the `log stream` equivalent.
    match tracing_etw::LayerBuilder::new("bae-core").build() {
        Ok(etw_layer) => install_logging(vec![
            terminal_log_sink(),
            LogSink::new(etw_layer, LevelFilter::DEBUG),
        ]),
        Err(error) => {
            // The terminal is the only sink left to report this in; launch
            // must not die over it.
            install_logging(vec![terminal_log_sink()]);
            tracing::error!("ETW tracing layer initialization failed: {error}");
        }
    }
}

/// The systemd journal, read with `journalctl --user -t bae`.
#[cfg(not(any(
    target_os = "macos",
    target_os = "android",
    target_os = "ios",
    target_os = "windows",
)))]
fn configure_logging() {
    match tracing_journald::layer() {
        Ok(journald_layer) => install_logging(vec![
            terminal_log_sink(),
            LogSink::new(
                journald_layer.with_syslog_identifier("bae".to_string()),
                LevelFilter::INFO,
            ),
        ]),
        Err(error) => {
            // No journald socket (a system without systemd, or a container):
            // the terminal is the only sink left to report this in; launch
            // must not die over it.
            install_logging(vec![terminal_log_sink()]);
            tracing::error!("journald tracing layer initialization failed: {error}");
        }
    }
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
        assert_eq!(
            app_start_failure_kind(&BootstrapError::KeyringUnavailable),
            AppStartFailureKind::KeyringUnavailable
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

    /// The messages a subscriber behind `filter` keeps, in emission order.
    fn kept_lines(filter: tracing_subscriber::EnvFilter) -> Vec<String> {
        use tracing_subscriber::prelude::*;
        let kept = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = {
            let kept = kept.clone();
            move || KeptLine(kept.clone())
        };
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .without_time()
                .with_writer(writer)
                .with_filter(filter),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "bae_core::import", "first-party debug");
            tracing::debug!(target: "coven_database::live_query", "coven debug");
            tracing::debug!(target: "sqlite3_parser::lexer", "dependency debug");
            tracing::info!(target: "sqlite3_parser::lexer", "dependency info");
        });
        let kept = kept.lock().expect("kept lines mutex poisoned");
        String::from_utf8(kept.clone())
            .expect("log lines are UTF-8")
            .lines()
            .map(|line| {
                line.split_once(": ")
                    .expect("a formatted line puts the message after the target")
                    .1
                    .to_string()
            })
            .collect()
    }

    struct KeptLine(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for KeptLine {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("kept lines mutex poisoned")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A debug sink records bae's own debug lines and a dependency's info,
    /// never a dependency's debug.
    #[test]
    fn a_debug_sink_keeps_dependencies_at_info() {
        let kept = kept_lines(sink_filter(None, LevelFilter::DEBUG));

        assert_eq!(
            kept,
            vec![
                "first-party debug".to_string(),
                "coven debug".to_string(),
                "dependency info".to_string(),
            ]
        );
    }

    /// `RUST_LOG` replaces the defaults whole.
    #[test]
    fn rust_log_overrides_the_default_levels() {
        let kept = kept_lines(sink_filter(
            Some("sqlite3_parser=debug"),
            LevelFilter::DEBUG,
        ));

        assert_eq!(
            kept,
            vec![
                "dependency debug".to_string(),
                "dependency info".to_string()
            ]
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
