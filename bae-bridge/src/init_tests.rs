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
    let core = BridgeTelemetryEvent::ScreenOpened { screen: BridgeScreen::Settings }.into_core();

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
            && logs.contains("bae-bridge/src/init_tests.rs")
            && logs.contains("backtrace="),
        "the panic and its diagnostics must reach tracing, got: {logs}"
    );
}
