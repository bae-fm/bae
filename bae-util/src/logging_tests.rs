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
        tracing::debug!(target: "bae_import::service", "first-party debug");
        tracing::debug!(target: "bae_catalogs::search", "first-party library debug");
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
        self.0.lock().expect("kept lines mutex poisoned").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A debug sink records the debug lines of every bae crate and coven and
/// a dependency's info, never a dependency's debug.
#[test]
fn a_debug_sink_keeps_dependencies_at_info() {
    let kept = kept_lines(sink_filter(None, LevelFilter::DEBUG), || emit_probe_lines!());

    assert_eq!(
        kept,
        vec![
            "first-party debug".to_string(),
            "first-party library debug".to_string(),
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
        std::env::current_exe().expect("resolve the bae-util test executable"),
    );
    child.args(["--exact", test, "--nocapture"]).env(LOG_CRATE_CHILD, sink_level.to_string());
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

    assert_eq!(kept, vec!["dependency debug".to_string(), "dependency info".to_string()]);
}
