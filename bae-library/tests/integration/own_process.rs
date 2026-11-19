//! One test's body run in a process of its own: this test binary again,
//! filtered to that one test. The integration tests share one process, so a
//! test that changes something process-wide (a resource limit) or can take
//! the process down (a stack overflow aborts it) runs this way.

use std::process::Command;

/// Set in the child, which prints it with the test's name once the body has
/// run.
const RAN: &str = "BAE_TEST_OWN_PROCESS_RAN";

/// Run `body` in a child process that runs only the test named `name`, its
/// path in this binary (`module::test`), and fail unless the child ran the
/// body and passed. The child records that the body ran because a filter
/// that matches no test passes with nothing run.
pub(crate) fn in_own_process(name: &str, body: impl FnOnce()) {
    if std::env::var_os(RAN).is_some() {
        body();
        println!("{RAN}={name}");
        return;
    }
    let output = Command::new(std::env::current_exe().expect("this test binary"))
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env(RAN, "1")
        .output()
        .expect("run the test in its own process");
    let printed = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{name} failed in its own process: {}\n{printed}\n{stderr}",
        output.status
    );
    assert!(
        printed.contains(&format!("{RAN}={name}")),
        "{name} ran no test in its own process: no test in this binary has that name"
    );
}
