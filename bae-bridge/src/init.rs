use std::backtrace::Backtrace;
use std::sync::{Arc, Once};

use bae_app::{bootstrap, BootstrapError};
use bae_diagnostics::{
    AppDiagnosticMetadata, AppStartFailureKind, DatadogDiagnosticsConfig, Diagnostics,
    DiagnosticsConfig, DiagnosticsError, Screen, TelemetryEvent,
};
use bae_util::logging::LogSink;
use bae_util::task_failure::TaskFailed;

use crate::handle::AppHandle;
use crate::types::BridgeError;
use tracing_subscriber::filter::LevelFilter;

/// The platform's crash reporter, which the app hands in when it builds the
/// telemetry sink. Core calls it for each bug it finds — code that broke its
/// own guarantee — from whatever thread found it; the platform records one
/// non-fatal event with the operation and the error's full text.
#[uniffi::export(callback_interface)]
pub trait BridgeBugReporter: Send + Sync {
    fn report(&self, operation: String, detail: String);
}

/// Core's view of the platform's [`BridgeBugReporter`].
struct PlatformBugReporter(Box<dyn BridgeBugReporter>);

/// A throw from the platform's reporter, which uniffi raises in Rust as a
/// panic, is logged: there is nowhere else to report it, and a report made
/// from a `Drop` or a teardown must not panic.
impl bae_diagnostics::BugReporter for PlatformBugReporter {
    fn report(&self, operation: &str, detail: &str) {
        if let Err(failed) = bae_util::task_failure::caught_sync("report a bug", || {
            self.0.report(operation.to_string(), detail.to_string())
        }) {
            tracing::error!(operation, "the platform's crash reporter threw on a bug: {failed}");
        }
    }
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiagnosticsConfig {
    Disabled,
    Enabled { config: BridgeDatadogDiagnosticsConfig },
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
/// host-facing telemetry object for host events. `BridgeHost` (which
/// `init_app` requires) takes it as a parameter so telemetry is guaranteed
/// set up before anything that could fail.
#[derive(uniffi::Object)]
pub struct BridgeDiagnostics {
    inner: Diagnostics,
}

impl BridgeDiagnostics {
    /// This device's libraries as coven keeps them under `app_dir`, their
    /// keys in the OS keychain, which building it opens. A keychain coven
    /// can't reach ships `keyring_init_failed` through this sink and fails
    /// as a keyring error.
    pub(crate) fn installation(
        &self,
        app_dir: &bae_app_dir::AppDir,
    ) -> Result<coven::Coven, BridgeError> {
        app_dir.coven().map_err(|error| {
            self.inner.event(TelemetryEvent::KeyringInitFailed {});
            BridgeError::Diagnostic {
                category: crate::types::BridgeErrorCategory::Keyring,
                detail: error.to_string(),
            }
        })
    }

    /// Flush any buffered telemetry now.
    pub(crate) async fn flush(&self) -> Result<(), BridgeError> {
        self.inner.flush().await.map_err(diagnostics_error_to_bridge)
    }

    /// Bootstrap the library `library_id` with this sink, its open's steps
    /// written into `opening`'s stream. A bootstrap failure ships
    /// `app_start_failed` through it before the error returns.
    pub(crate) fn open_app(
        &self,
        app_dir: bae_app_dir::AppDir,
        library_id: String,
        position_update_interval_ms: u32,
        restore_playback: bool,
        builder: coven::CovenBuilder,
        opening: &crate::BridgeLibraryOpening,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bootstrap(
            app_dir,
            library_id,
            position_update_interval_ms,
            restore_playback,
            bae_app::PlaybackOutput::System(host.audio_focus()),
            self.inner.clone(),
            builder,
            opening.report(),
            start_handle(host, self.inner.clone()),
        ))
    }

    /// [`Self::open_app`] with the fixture a debug UI test left at `fixture`
    /// written into the library before its services start.
    #[cfg(all(debug_assertions, desktop))]
    pub(crate) fn open_app_on_fixture(
        &self,
        fixture: std::path::PathBuf,
        app_dir: bae_app_dir::AppDir,
        library_id: String,
        position_update_interval_ms: u32,
        restore_playback: bool,
        builder: coven::CovenBuilder,
        opening: &crate::BridgeLibraryOpening,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bae_app::bootstrap_on_fixture(
            fixture,
            app_dir,
            library_id,
            position_update_interval_ms,
            restore_playback,
            bae_app::PlaybackOutput::System(host.audio_focus()),
            self.inner.clone(),
            builder,
            opening.report(),
            start_handle(host, self.inner.clone()),
        ))
    }

    pub(crate) fn open_prepared(
        &self,
        app_dir: bae_app_dir::AppDir,
        handle: coven::CovenHandle,
        host: Arc<crate::BridgeHost>,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bae_app::bootstrap_prepared(
            app_dir,
            handle,
            position_update_interval_ms,
            restore_playback,
            bae_app::PlaybackOutput::System(host.audio_focus()),
            self.inner.clone(),
            start_handle(host, self.inner.clone()),
        ))
    }

    /// The recorded removals that never finished. A panic in reading their
    /// records is reported through this sink.
    pub(crate) async fn unfinished_removal(
        &self,
        app_dir: &bae_app_dir::AppDir,
        coven: &coven::Coven,
    ) -> Result<Option<bae_library::UnfinishedRemoval>, bae_library::LibraryError> {
        bae_library::UnfinishedRemoval::find(app_dir, coven, &self.inner).await
    }

    /// Remove the library `store`, which can't open, without opening it,
    /// telling `steps` as leaving and deleting start. A panic in its file
    /// work is reported through this sink.
    pub(crate) async fn remove_unopenable_library(
        &self,
        app_dir: &bae_app_dir::AppDir,
        coven: &coven::Coven,
        store: coven::StoreId,
        steps: &(dyn Fn(bae_library::ClosingStep) + Send + Sync),
    ) -> Result<(), bae_library::LibraryRemovalError> {
        bae_library::remove_unopenable_library(app_dir, coven, &self.inner, store, steps).await
    }

    /// Complete every recorded removal, telling `finished` how many are done
    /// of how many there are. A panic in its file work is reported through
    /// this sink.
    pub(crate) async fn finish_removals(
        &self,
        app_dir: &bae_app_dir::AppDir,
        coven: &coven::Coven,
        finished: &(dyn Fn(u64, u64) + Send + Sync),
    ) -> Result<(), bae_library::LibraryError> {
        bae_library::finish_removals(app_dir, coven, &self.inner, finished).await
    }

    /// The handle a bootstrap built, or its failure shipped as
    /// `app_start_failed` and returned.
    fn opened(
        &self,
        bootstrapped: Result<AppHandle, BootstrapError>,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        bootstrapped.map(Arc::new).map_err(|error| {
            self.emit_app_start_failed(&error);
            bootstrap_error_to_bridge(error)
        })
    }

    /// Report `failed` when it is a panic, and hand it back for the caller's
    /// own error.
    pub(crate) fn bug(&self, failed: TaskFailed) -> TaskFailed {
        self.inner.bug(failed)
    }

    /// Report `bug`, a host call that threw, and hand it back for the
    /// caller's own error: a phone host's calls only.
    #[cfg(any(test, not(desktop)))]
    pub(crate) fn panicked(&self, bug: bae_util::task_failure::Bug) -> bae_util::task_failure::Bug {
        self.inner.panicked(bug)
    }

    /// Report a failure of `operation` that no caller is left to hear as a
    /// bug, to the log and the platform's crash reporter.
    pub(crate) fn report_bug(&self, operation: &str, error: &dyn std::fmt::Display) {
        self.inner.report_bug(operation, error);
    }

    /// Where the bridge's calls run on `runtime`, reporting their panics to
    /// this sink.
    pub(crate) fn operations(
        &self,
        runtime: tokio::runtime::Handle,
    ) -> crate::operation_runtime::OperationRuntime {
        crate::operation_runtime::OperationRuntime::new(runtime, self.inner.clone())
    }

    fn emit_app_start_failed(&self, error: &BootstrapError) {
        self.inner.event(TelemetryEvent::AppStartFailed { kind: app_start_failure_kind(error) });
    }

    #[cfg(test)]
    pub(crate) fn noop() -> Arc<Self> {
        Arc::new(Self { inner: Diagnostics::noop() })
    }

    /// A sink that sends nothing and reports each bug to `bugs`.
    #[cfg(test)]
    pub(crate) fn reporting_bugs_to(bugs: Arc<dyn bae_diagnostics::BugReporter>) -> Arc<Self> {
        Arc::new(Self { inner: Diagnostics::noop().reporting_bugs_to(bugs) })
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
/// process start, before `BridgeHost` (which requires the returned handle),
/// so the sink exists for every launch step that could fail. The
/// sink reports the bugs core finds to `bug_reporter`, the platform's crash
/// reporter. Local logs go to the platform's native log, and on the desktop
/// also to stdout when it is a terminal.
///
/// Infallible by contract: telemetry setup must never block a launch. Sink
/// construction from an `Enabled` config can fail (incomplete config, worker
/// spawn); that is logged at `error` and the app continues with the no-op sink
/// — the one failure telemetry cannot report about itself, accepted. Handling
/// it here keeps the bailout in one place instead of a catch-and-retry in every
/// host language.
#[uniffi::export]
pub fn configure_diagnostics(
    config: BridgeDiagnosticsConfig,
    bug_reporter: Box<dyn BridgeBugReporter>,
) -> Arc<BridgeDiagnostics> {
    configure_logging();
    install_panic_logging();
    let clock = Arc::new(coven::SystemClock);
    let ids = Arc::new(coven::UuidIds);
    let bugs: Arc<dyn bae_diagnostics::BugReporter> = Arc::new(PlatformBugReporter(bug_reporter));
    let inner = match Diagnostics::configure(config.into_core(), clock, ids, bugs.clone()) {
        Ok(diagnostics) => diagnostics,
        Err(error) => {
            tracing::error!("telemetry init failed: {error}; continuing without telemetry");
            Diagnostics::noop().reporting_bugs_to(bugs)
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
                format!("{}:{}:{}", location.file(), location.line(), location.column())
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
        BootstrapError::Database(_) | BootstrapError::DeviceResetRequired(_) => {
            AppStartFailureKind::Database
        }
        BootstrapError::Internal(_) => AppStartFailureKind::Internal,
        BootstrapError::KeyringUnavailable => AppStartFailureKind::KeyringUnavailable,
        BootstrapError::KeychainEntitlementMissing(_) => {
            AppStartFailureKind::KeychainEntitlementMissing
        }
        BootstrapError::ReplacementUnfinished(_) => AppStartFailureKind::ReplacementUnfinished,
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
        let BridgeDatadogDiagnosticsConfig { datadog_site, client_token, source, app } = self;
        let BridgeAppDiagnosticMetadata { service, environment, app_version, edition, git_commit } =
            app;
        let config = DatadogDiagnosticsConfig {
            datadog_site,
            client_token,
            source,
            app: AppDiagnosticMetadata { service, environment, app_version, edition, git_commit },
        };

        DiagnosticsConfig::Enabled(config)
    }
}

fn diagnostics_error_to_bridge(e: DiagnosticsError) -> BridgeError {
    BridgeError::internal(format!("diagnostics failed: {e}"))
}

fn bootstrap_error_to_bridge(e: BootstrapError) -> BridgeError {
    match e {
        BootstrapError::LibraryNotFound(id) => {
            BridgeError::NotFound { entity: crate::types::BridgeEntityKind::Library, id }
        }
        BootstrapError::Config(msg) => BridgeError::config(msg),
        BootstrapError::Database(msg) => BridgeError::database(msg),
        BootstrapError::DeviceResetRequired(msg) => {
            BridgeError::diagnostic(crate::BridgeErrorCategory::DeviceResetRequired, msg)
        }
        // The category names the one call that finishes the replacement, so
        // the host offers that call and never the other.
        error @ BootstrapError::ReplacementUnfinished(by) => BridgeError::diagnostic(
            crate::BridgeErrorCategory::ReplacementUnfinished {
                by: crate::BridgeReplacedBy::from_core(by),
            },
            error,
        ),
        BootstrapError::Internal(msg) => BridgeError::internal(msg),
        // Its own category, not `Keyring`: that one reads as "the keyring
        // broke", and this one is "it is locked this second, try again after
        // you unlock". The host branches on the category to keep the handle
        // path open and retry, so the distinction has to survive the bridge.
        BootstrapError::KeyringUnavailable => BridgeError::diagnostic(
            crate::types::BridgeErrorCategory::KeyringLocked,
            "the OS keychain refused the read while the session is locked",
        ),
        error @ BootstrapError::KeychainEntitlementMissing(_) => BridgeError::diagnostic(
            crate::types::BridgeErrorCategory::KeychainEntitlementMissing,
            error,
        ),
    }
}

/// The terminal sink of the desktop apps: every line printed to stdout, down
/// to debug, when stdout is a terminal — a terminal run is someone watching
/// everything, and `RUST_LOG` quiets it. Launched any other way (Finder, the
/// Dock, a desktop launcher) stdout goes nowhere anyone reads, and there is
/// no sink, so no line is formatted for it. Mobile has no terminal.
#[cfg(desktop)]
fn terminal_log_sink() -> Option<LogSink> {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal().then(|| {
        LogSink::new(
            tracing_subscriber::fmt::layer()
                .with_line_number(true)
                .with_target(false)
                .with_file(true),
            LevelFilter::DEBUG,
        )
    })
}

/// The unified log, read back from a Debug build with
/// `log show --last 1h --debug --predicate 'subsystem == "fm.bae.desktop"'`,
/// or followed with
/// `log stream --level debug --predicate 'subsystem == "fm.bae.desktop"'`.
#[cfg(target_os = "macos")]
fn configure_logging() {
    bae_util::logging::install(
        terminal_log_sink()
            .into_iter()
            .chain([LogSink::new(
                tracing_oslog::OsLogger::new("fm.bae.desktop", "default"),
                LevelFilter::DEBUG,
            )])
            .collect(),
    )
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
    bae_util::logging::install(vec![LogSink::new(android_layer, LevelFilter::INFO)])
}

/// The unified log, read back from a Debug build with
/// `log show --last 1h --debug --predicate 'subsystem == "fm.bae.app"'`, or
/// followed with
/// `log stream --level debug --predicate 'subsystem == "fm.bae.app"'`.
#[cfg(target_os = "ios")]
fn configure_logging() {
    bae_util::logging::install(vec![LogSink::new(
        tracing_oslog::OsLogger::new("fm.bae.app", "default"),
        LevelFilter::DEBUG,
    )])
}

#[cfg(target_os = "windows")]
fn configure_logging() {
    // ETW is Windows' unified logging: a TraceLogging provider named
    // "bae-bridge" (GUID derived from the name), captured with
    // `logman start ... -p "*bae-bridge"` — the `log stream` equivalent.
    match tracing_etw::LayerBuilder::new("bae-bridge").build() {
        Ok(etw_layer) => bae_util::logging::install(
            terminal_log_sink()
                .into_iter()
                .chain([LogSink::new(etw_layer, LevelFilter::DEBUG)])
                .collect(),
        ),
        Err(error) => {
            // The terminal, when there is one, is the only sink left to report
            // this in; launch must not die over it.
            bae_util::logging::install(terminal_log_sink().into_iter().collect());
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
        Ok(journald_layer) => bae_util::logging::install(
            terminal_log_sink()
                .into_iter()
                .chain([LogSink::new(
                    journald_layer.with_syslog_identifier("bae".to_string()),
                    LevelFilter::INFO,
                )])
                .collect(),
        ),
        Err(error) => {
            // No journald socket (a system without systemd, or a container):
            // the terminal, when there is one, is the only sink left to report
            // this in; launch must not die over it.
            bae_util::logging::install(terminal_log_sink().into_iter().collect());
            tracing::error!("journald tracing layer initialization failed: {error}");
        }
    }
}

/// What a bootstrap runs once the library is open: start the handle for
/// `host` over it, reporting a panic in its work through `diagnostics`.
fn start_handle(
    host: Arc<crate::BridgeHost>,
    diagnostics: Diagnostics,
) -> impl FnOnce(
    bae_app::AppServices,
    tokio::runtime::Runtime,
) -> Result<AppHandle, bae_app::BootstrapError>
       + Send
       + 'static {
    move |services, runtime| AppHandle::start(services, runtime, diagnostics, host)
}

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
