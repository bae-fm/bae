//! One test's body run in a process of its own: this test binary again,
//! filtered to that one test. The integration tests share one process, so a
//! test that changes something process-wide (a resource limit) or can take
//! the process down (a stack overflow aborts it) runs this way.

use std::process::Command;

/// Set in the child to the file it writes once the body has run.
const RAN: &str = "BAE_TEST_OWN_PROCESS_RAN";

/// Run `body` in a child process that runs only the test named `name`, its
/// path in this binary (`module::test`), and fail unless the child ran the
/// body and passed. The child records that the body ran because a filter
/// that matches no test passes with nothing run.
pub(crate) fn in_own_process(name: &str, body: impl FnOnce()) {
    if let Some(ran) = std::env::var_os(RAN) {
        body();
        std::fs::write(ran, name).expect("record that the body ran");
        return;
    }
    let directory = tempfile::tempdir().expect("a directory for the child's record");
    let ran = directory.path().join("ran");
    let status = Command::new(std::env::current_exe().expect("this test binary"))
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env(RAN, &ran)
        .status()
        .expect("run the test in its own process");
    assert!(
        status.success(),
        "{name} failed in its own process: {status}"
    );
    assert!(
        ran.exists(),
        "{name} ran no test in its own process: no test in this binary has that name"
    );
}
