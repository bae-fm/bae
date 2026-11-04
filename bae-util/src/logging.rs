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
        Self { layer: Box::new(layer), default_level }
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

/// The log targets that are bae's own: every workspace crate, all named
/// `bae_*`, and coven, whose target also covers its `coven_*` crates (a
/// directive matches every target it prefixes).
const FIRST_PARTY_TARGETS: &[&str] = &["bae_", "coven"];

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
        .chain(FIRST_PARTY_TARGETS.iter().map(|target| format!("{target}={level}")))
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
            Some(format!("RUST_LOG {raw:?} is not valid Unicode; logging at the default levels")),
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
/// target: `debug`, for bae's own crates. Admitted at `debug`, every
/// dependency's debug lines would build their arguments even when every sink
/// drops them.
pub(crate) fn install_global(
    subscriber: impl tracing::Subscriber + Send + Sync + 'static,
    log_crate_level: LevelFilter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tracing_log::AsLog;
    tracing::subscriber::set_global_default(subscriber)?;
    tracing_log::LogTracer::builder().with_max_level(log_crate_level.as_log()).init()?;
    Ok(())
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
