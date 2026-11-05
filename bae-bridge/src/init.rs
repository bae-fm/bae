use std::backtrace::Backtrace;
use std::sync::{Arc, Once};

use bae_diagnostics::{
    AppDiagnosticMetadata, DatadogDiagnosticsConfig, Diagnostics, DiagnosticsConfig, Screen,
    TelemetryEvent,
};
use bae_util::logging::LogSink;

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

/// The app's handle on the process's telemetry sink, built once at startup.
/// Wraps core's `Diagnostics`; it is the one host-facing telemetry object for
/// host events. `BridgeHost` (which `init_app` requires) takes it as a
/// parameter so telemetry is guaranteed set up before anything that could
/// fail, and takes core's sink from it when built.
#[derive(uniffi::Object)]
pub struct BridgeDiagnostics {
    inner: Diagnostics,
}

impl BridgeDiagnostics {
    /// Core's sink, which this object carries across to the app: the host the
    /// app builds on it reports through it.
    pub(crate) fn core(&self) -> &Diagnostics {
        &self.inner
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

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
