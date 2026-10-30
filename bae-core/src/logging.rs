//! The process's log subscriber: which lines each local sink keeps, and the
//! one global install every app makes at launch over its platform's sinks.

use tracing_subscriber::filter::LevelFilter;

/// One local log sink and the level bae's own crates record at when
/// `RUST_LOG` is unset. Levels are per sink because the sinks keep different
/// amounts: the unified log and ETW discard what no one is capturing, and a
/// terminal is someone watching, so they take `debug`; logcat and the journal
/// keep every line on the device, so they take `info`. A Debug build of the
/// macOS or iOS app has the unified log save every level of its lines
/// (`OSLogPreferences` in its Info.plist), so a development build is read after
/// the fact; a Release build keeps the system's defaults.
pub struct LogSink {
    layer: Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
    default_level: LevelFilter,
}

impl LogSink {
    pub fn new(
        layer: impl tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
        default_level: LevelFilter,
    ) -> Self {
        Self {
            layer: Box::new(layer),
            default_level,
        }
    }

    /// The finest level this sink keeps of a `log` crate record. Only
    /// dependencies log through the `log` crate — bae's crates and coven's
    /// log through tracing — so with `RUST_LOG` unset it is the sink's level
    /// for dependencies. `RUST_LOG` may name any target, so under it the finest
    /// level any of its directives names.
    fn log_crate_level(&self, rust_log: Option<&str>) -> LevelFilter {
        match rust_log {
            // An `EnvFilter` without a hint admits every level.
            Some(rust_log) => tracing_subscriber::EnvFilter::new(rust_log)
                .max_level_hint()
                .unwrap_or(LevelFilter::TRACE),
            None => dependency_level(self.default_level),
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

/// The finest level a sink at `level` keeps of a dependency's lines when
/// `RUST_LOG` is unset: no finer than `info`. A dependency's debug output is
/// its internals — a SQL lexer reports every token it consumes — and at
/// `debug` it buries bae's lines.
fn dependency_level(level: LevelFilter) -> LevelFilter {
    std::cmp::min(level, LevelFilter::INFO)
}

/// A sink's filter directives when `RUST_LOG` is unset: bae's own crates at
/// the sink's level, everything else at its dependency level.
fn default_directives(level: LevelFilter) -> String {
    std::iter::once(dependency_level(level).to_string())
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
/// filter. A subscriber already installed in the process stays, and this
/// install is logged and dropped.
pub fn install(sinks: Vec<LogSink>) {
    use tracing_subscriber::prelude::*;
    let (rust_log, complaint) = rust_log();
    let log_crate_level = sinks
        .iter()
        .map(|sink| sink.log_crate_level(rust_log.as_deref()))
        .fold(LevelFilter::OFF, std::cmp::max);
    let layers: Vec<_> = sinks
        .into_iter()
        .map(|sink| {
            let filter = sink_filter(rust_log.as_deref(), sink.default_level);
            sink.layer.with_filter(filter).boxed()
        })
        .collect();
    let subscriber = tracing_subscriber::registry().with(layers);
    if let Err(error) = install_global(subscriber, log_crate_level) {
        tracing::debug!(%error, "log subscriber already installed");
    }
    // Emitted after install so it lands in the just-installed sinks.
    if let Some(complaint) = complaint {
        tracing::warn!("{complaint}");
    }
}

/// Install `subscriber` as the process's, and carry the `log` crate's records
/// into it at `log_crate_level` and coarser.
///
/// The `log` crate's max level is set here, not from the subscriber's own
/// level hint as tracing-subscriber's `init` sets it. The `log` macros build
/// their arguments whenever that level admits the record, before any filter
/// can drop it, and the hint is the finest level any sink keeps of any
/// target: `debug`, for bae's own crates. Admitted at `debug`, sqlite3-parser
/// computes a line and column for each token it scans by walking the SQL from
/// its start, which makes tokenizing SQL quadratic in its length; coven
/// tokenizes the whole schema when it creates a library.
pub(crate) fn install_global(
    subscriber: impl tracing::Subscriber + Send + Sync + 'static,
    log_crate_level: LevelFilter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tracing_log::AsLog;
    tracing::subscriber::set_global_default(subscriber)?;
    tracing_log::LogTracer::builder()
        .with_max_level(log_crate_level.as_log())
        .init()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Emits one line for each target and level the filter tests probe.
    ///
    /// A macro, not a function, so each test that expands it owns its
    /// callsites. tracing caches a callsite's interest process-wide, computed
    /// from the subscribers alive when the callsite is first hit. Each filter
    /// test runs on its own thread behind its own subscriber; a callsite two of
    /// them share can be first hit by both at once, and the interest one
    /// computes from its own subscriber alone then overwrites the other's, so
    /// the other test's events are dropped or kept by the wrong filter.
    macro_rules! emit_probe_lines {
        () => {{
            tracing::debug!(target: "bae_core::import", "first-party debug");
            tracing::debug!(target: "coven_database::live_query", "coven debug");
            tracing::debug!(target: "sqlite3_parser::lexer", "dependency debug");
            tracing::info!(target: "sqlite3_parser::lexer", "dependency info");
        }};
    }

    /// The messages a subscriber behind `filter` keeps of those `emit` logs,
    /// in emission order.
    fn kept_lines(filter: tracing_subscriber::EnvFilter, emit: impl FnOnce()) -> Vec<String> {
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
        tracing::subscriber::with_default(subscriber, emit);
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
        let kept = kept_lines(
            sink_filter(None, LevelFilter::DEBUG),
            || emit_probe_lines!(),
        );

        assert_eq!(
            kept,
            vec![
                "first-party debug".to_string(),
                "coven debug".to_string(),
                "dependency info".to_string(),
            ]
        );
    }

    /// Set in the child process of the `log` crate level tests: the level of
    /// the one sink it installs.
    const LOG_CRATE_CHILD: &str = "BAE_TEST_LOG_CRATE_LEVEL";

    /// In the child process, install one sink at the level the parent named,
    /// print the `log` crate's max level, and say so; in the parent, do
    /// nothing.
    fn report_log_crate_level_in_child() -> bool {
        let Some(sink_level) = std::env::var_os(LOG_CRATE_CHILD) else {
            return false;
        };
        let sink_level: LevelFilter = sink_level
            .to_str()
            .and_then(|level| level.parse().ok())
            .expect("the parent names a level filter");
        install(vec![LogSink::new(
            tracing_subscriber::fmt::layer().with_writer(std::io::sink),
            sink_level,
        )]);
        println!("log crate max level: {}", tracing_log::log::max_level());
        true
    }

    /// The `log` crate's max level once `test`, rerun as a child process,
    /// installs a sink at `sink_level` under `rust_log`. A child, because the
    /// subscriber and the `log` logger are process-wide and set once.
    fn log_crate_level_after_install(
        test: &str,
        sink_level: LevelFilter,
        rust_log: Option<&str>,
    ) -> String {
        let mut child = std::process::Command::new(
            std::env::current_exe().expect("resolve the bae-core test executable"),
        );
        child
            .args(["--exact", test, "--nocapture"])
            .env(LOG_CRATE_CHILD, sink_level.to_string());
        match rust_log {
            Some(rust_log) => child.env("RUST_LOG", rust_log),
            None => child.env_remove("RUST_LOG"),
        };
        let output = child.output().expect("run the logging child process");
        assert!(
            output.status.success(),
            "the child installs logging and reports the level: {output:?}"
        );
        let printed = String::from_utf8(output.stdout).expect("the child prints UTF-8");
        printed
            .lines()
            .find_map(|line| line.strip_prefix("log crate max level: "))
            .unwrap_or_else(|| panic!("the child reports the level, got: {printed}"))
            .to_string()
    }

    /// Only dependencies log through the `log` crate, and a debug sink keeps
    /// their lines at info, so the `log` crate admits no record finer than
    /// info. Its macros evaluate their arguments whenever the max level
    /// admits the record, before any filter can drop it.
    #[test]
    fn a_debug_sink_opens_the_log_crate_to_info() {
        if report_log_crate_level_in_child() {
            return;
        }
        assert_eq!(
            log_crate_level_after_install(
                "logging::tests::a_debug_sink_opens_the_log_crate_to_info",
                LevelFilter::DEBUG,
                None,
            ),
            "INFO"
        );
    }

    /// `RUST_LOG`'s finest level is the finest a `log` record can be kept at.
    #[test]
    fn rust_log_sets_the_log_crate_level() {
        if report_log_crate_level_in_child() {
            return;
        }
        assert_eq!(
            log_crate_level_after_install(
                "logging::tests::rust_log_sets_the_log_crate_level",
                LevelFilter::DEBUG,
                Some("warn"),
            ),
            "WARN"
        );
    }

    /// `RUST_LOG` replaces the defaults whole.
    #[test]
    fn rust_log_overrides_the_default_levels() {
        let kept = kept_lines(
            sink_filter(Some("sqlite3_parser=debug"), LevelFilter::DEBUG),
            || emit_probe_lines!(),
        );

        assert_eq!(
            kept,
            vec![
                "dependency debug".to_string(),
                "dependency info".to_string()
            ]
        );
    }
}
