//! A setup call's steps, as the app reads them off the call's stream.

use super::*;
use crate::types::{BridgeBootstrapStatus, BridgeSetupProgress, BridgeSetupStep};

/// How many times the stream said a key started or stopped.
#[derive(Default)]
struct Heard(Mutex<usize>);

struct Activity(Arc<Heard>);

impl KeyActivityCallback for Activity {
    fn on_keys_changed(&self, _keys: Vec<Vec<u8>>) {
        *self.0 .0.lock().unwrap() += 1;
    }
}

/// The stream's one key holds where the call is while it runs, and nothing
/// once the call's writing ends.
#[test]
fn the_stream_holds_the_running_step_until_the_call_ends() {
    let setup = BridgeLibrarySetup::new(crate::BridgeDiagnostics::noop());
    let heard = Arc::new(Heard::default());
    let stream = setup.feed_setup_steps(Box::new(Activity(heard.clone())));
    let steps = setup.writer();

    steps.step(BridgeSetupStep::RemovingLibraries, 1, 3);
    assert_eq!(
        stream.latest(Vec::new()),
        Some(BridgeFastValue::SetupProgress {
            progress: BridgeSetupProgress {
                step: BridgeSetupStep::RemovingLibraries,
                done: 1,
                total: 3,
            },
        })
    );
    steps.bootstrap(coven::BootstrapStatus::Connecting);
    assert_eq!(
        stream.latest(Vec::new()),
        Some(BridgeFastValue::BootstrapStatus {
            status: BridgeBootstrapStatus::Connecting,
        })
    );

    drop(steps);
    assert_eq!(stream.latest(Vec::new()), None);
    assert_eq!(*heard.0.lock().unwrap(), 2, "the key started, then stopped");
}

/// Creating a library runs one step, which ends with the call.
#[test]
fn creating_a_library_reports_its_step() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let setup = BridgeLibrarySetup::new(crate::BridgeDiagnostics::noop());
    let caller = tokio::runtime::Runtime::new().unwrap();
    caller
        .block_on(host.create_library(Some("Created".into()), setup.clone()))
        .unwrap();
    assert_eq!(
        setup.written(),
        vec![
            Some(BridgeFastValue::SetupProgress {
                progress: BridgeSetupProgress {
                    step: BridgeSetupStep::CreatingLibrary,
                    done: 0,
                    total: 0,
                },
            }),
            None,
        ]
    );
}
