use std::backtrace::Backtrace;
use std::sync::{Arc, Once};

use bae_core::app::{bootstrap, BootstrapError};
use bae_core::diagnostics::{
    AppDiagnosticMetadata, AppStartFailureKind, DatadogDiagnosticsConfig, Diagnostics,
    DiagnosticsConfig, DiagnosticsError, Screen, TelemetryEvent,
};
use bae_core::logging::LogSink;
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
impl bae_core::diagnostics::BugReporter for PlatformBugReporter {
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
        self.inner.flush().await.map_err(diagnostics_error_to_bridge)
    }

    /// Bootstrap the library `library_id` with this sink, its open's steps
    /// written into `opening`'s stream. A bootstrap failure ships
    /// `app_start_failed` through it before the error returns.
    pub(crate) fn open_app(
        &self,
        app_dir: bae_core::config::AppDir,
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
            host.audio_focus(),
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
        app_dir: bae_core::config::AppDir,
        library_id: String,
        position_update_interval_ms: u32,
        restore_playback: bool,
        builder: coven::CovenBuilder,
        opening: &crate::BridgeLibraryOpening,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bae_core::app::bootstrap_on_fixture(
            fixture,
            app_dir,
            library_id,
            position_update_interval_ms,
            restore_playback,
            host.audio_focus(),
            self.inner.clone(),
            builder,
            opening.report(),
            start_handle(host, self.inner.clone()),
        ))
    }

    pub(crate) fn open_prepared(
        &self,
        app_dir: bae_core::config::AppDir,
        handle: coven::CovenHandle,
        host: Arc<crate::BridgeHost>,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bae_core::app::bootstrap_prepared(
            app_dir,
            handle,
            position_update_interval_ms,
            restore_playback,
            host.audio_focus(),
            self.inner.clone(),
            start_handle(host, self.inner.clone()),
        ))
    }

    /// The recorded removals that never finished. A panic in reading their
    /// records is reported through this sink.
    pub(crate) async fn unfinished_removal(
        &self,
        app_dir: &bae_core::config::AppDir,
    ) -> Result<Option<bae_core::library::UnfinishedRemoval>, bae_core::library::LibraryError> {
        bae_core::library::UnfinishedRemoval::find(app_dir, &self.inner).await
    }

    /// Remove the library `store`, which can't open, without opening it,
    /// telling `steps` as leaving and deleting start. A panic in its file
    /// work is reported through this sink.
    pub(crate) async fn remove_unopenable_library(
        &self,
        app_dir: &bae_core::config::AppDir,
        keychain: &bae_core::library::Keychain,
        store: coven::StoreId,
        steps: &(dyn Fn(bae_core::library::ClosingStep) + Send + Sync),
    ) -> Result<(), bae_core::library::LibraryRemovalError> {
        bae_core::library::remove_unopenable_library(app_dir, &self.inner, keychain, store, steps)
            .await
    }

    /// Complete every recorded removal, telling `finished` how many are done
    /// of how many there are. A panic in its file work is reported through
    /// this sink.
    pub(crate) async fn finish_removals(
        &self,
        app_dir: &bae_core::config::AppDir,
        finished: &(dyn Fn(u64, u64) + Send + Sync),
    ) -> Result<(), bae_core::library::LibraryError> {
        bae_core::library::finish_removals(app_dir, &self.inner, finished).await
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
    pub(crate) fn reporting_bugs_to(
        bugs: Arc<dyn bae_core::diagnostics::BugReporter>,
    ) -> Arc<Self> {
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
/// process start, before `init_keyring` / `BridgeHost` (both require the returned
/// handle), so the sink exists for every launch step that could fail. The
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
    let bugs: Arc<dyn bae_core::diagnostics::BugReporter> =
        Arc::new(PlatformBugReporter(bug_reporter));
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
    bae_core::logging::install(
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
    bae_core::logging::install(vec![LogSink::new(android_layer, LevelFilter::INFO)])
}

/// The unified log, read back from a Debug build with
/// `log show --last 1h --debug --predicate 'subsystem == "fm.bae.app"'`, or
/// followed with
/// `log stream --level debug --predicate 'subsystem == "fm.bae.app"'`.
#[cfg(target_os = "ios")]
fn configure_logging() {
    bae_core::logging::install(vec![LogSink::new(
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
        Ok(etw_layer) => bae_core::logging::install(
            terminal_log_sink()
                .into_iter()
                .chain([LogSink::new(etw_layer, LevelFilter::DEBUG)])
                .collect(),
        ),
        Err(error) => {
            // The terminal, when there is one, is the only sink left to report
            // this in; launch must not die over it.
            bae_core::logging::install(terminal_log_sink().into_iter().collect());
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
        Ok(journald_layer) => bae_core::logging::install(
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
            bae_core::logging::install(terminal_log_sink().into_iter().collect());
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
    bae_core::library::AppServices,
    tokio::runtime::Runtime,
) -> Result<AppHandle, bae_core::app::BootstrapError>
       + Send
       + 'static {
    move |services, runtime| AppHandle::start_for_host(services, runtime, diagnostics, host)
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
        assert_eq!(
            app_start_failure_kind(&BootstrapError::KeychainEntitlementMissing(
                keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into()
            )),
            AppStartFailureKind::KeychainEntitlementMissing
        );
        assert_eq!(
            app_start_failure_kind(&BootstrapError::ReplacementUnfinished(
                coven::ReplacedBy::Restore
            )),
            AppStartFailureKind::ReplacementUnfinished
        );
    }

    /// A build without its keychain entitlement crosses as its own line, not
    /// as a broken keyring or a locked one.
    #[test]
    fn a_missing_keychain_entitlement_crosses_as_its_own_category() {
        let error = bootstrap_error_to_bridge(BootstrapError::KeychainEntitlementMissing(
            keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into(),
        ));
        let BridgeError::Diagnostic { category, .. } = &error else {
            panic!("a missing entitlement is a diagnostic: {error:?}");
        };
        assert_eq!(*category, crate::BridgeErrorCategory::KeychainEntitlementMissing);
        assert_eq!(
            crate::bridge_error_category_key(category.clone()),
            "core.error.keyring.entitlement_missing"
        );
    }

    /// An open that stopped on an unfinished replacement crosses as the call
    /// that finishes it, each to its own line.
    #[test]
    fn an_unfinished_replacement_crosses_as_the_call_that_finishes_it() {
        for (core, bridge, key) in [
            (
                coven::ReplacedBy::Restore,
                crate::BridgeReplacedBy::Restore,
                "core.error.replacement_unfinished.restore",
            ),
            (
                coven::ReplacedBy::Reset,
                crate::BridgeReplacedBy::Reset,
                "core.error.replacement_unfinished.reset",
            ),
        ] {
            let error = bootstrap_error_to_bridge(BootstrapError::ReplacementUnfinished(core));
            let BridgeError::Diagnostic { category, .. } = &error else {
                panic!("an unfinished replacement is a diagnostic: {error:?}");
            };
            assert_eq!(*category, crate::BridgeErrorCategory::ReplacementUnfinished { by: bridge });
            assert_eq!(crate::types::bridge_error_line_key(&error).as_deref(), Some(key));
        }
    }

    #[test]
    fn host_telemetry_event_maps_to_the_core_catalog() {
        let core =
            BridgeTelemetryEvent::ScreenOpened { screen: BridgeScreen::Settings }.into_core();

        assert_eq!(core.name(), "screen_opened");
        assert_eq!(core.fields()["screen"], serde_json::Value::String("settings".to_string()));
    }

    /// A desktop app whose stdout is no terminal — launched from Finder, the
    /// Dock or a desktop launcher, where stdout goes nowhere anyone reads —
    /// prints none of its lines there.
    #[cfg(desktop)]
    #[test]
    fn no_line_is_printed_to_a_stdout_that_is_no_terminal() {
        const CHILD_PROCESS: &str = "BAE_TEST_STDOUT_NO_TERMINAL";
        const TEST_NAME: &str = "init::tests::no_line_is_printed_to_a_stdout_that_is_no_terminal";
        const PROBE: &str = "terminal sink probe";

        if std::env::var_os(CHILD_PROCESS).is_some() {
            configure_logging();
            tracing::debug!(target: "bae_bridge", "{PROBE}");
            return;
        }

        // `output` hands the child a pipe for its stdout, not a terminal.
        let output = std::process::Command::new(
            std::env::current_exe().expect("resolve the bridge test executable"),
        )
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_PROCESS, "1")
        .env_remove("RUST_LOG")
        .output()
        .expect("run the logging child process");

        assert!(output.status.success(), "the child process logs and exits");
        let printed = String::from_utf8(output.stdout).expect("the child prints UTF-8");
        assert!(
            !printed.contains(PROBE),
            "nothing is printed to a stdout that is no terminal, got: {printed}"
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
                struct NoCrashReporter;
                impl BridgeBugReporter for NoCrashReporter {
                    fn report(&self, _operation: String, _detail: String) {}
                }
                configure_diagnostics(BridgeDiagnosticsConfig::Disabled, Box::new(NoCrashReporter));
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
