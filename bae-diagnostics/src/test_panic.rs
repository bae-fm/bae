//! A panic at one known place, for a test that checks where a panic's report
//! says it began.

use bae_util::task_failure::Bug;

/// The line of the `panic!` in [`panics`].
pub const PANIC_LINE: u32 = line!() + 4;

/// Panics with `message`, on line [`PANIC_LINE`] of this file, at column 5.
pub fn panics(message: &str) -> ! {
    panic!("{message}")
}

/// The bug `operation` gives once [`panics`] panics in it with `message`.
pub fn bug(operation: &'static str, message: &str) -> Bug {
    Bug {
        operation,
        message: message.to_string(),
        location: Some(coven::task::SourceLocation {
            file: file!().to_string(),
            line: PANIC_LINE,
            column: 5,
        }),
    }
}

/// The crash report of `operation`'s panic in [`panics`] with `message`: the
/// operation, and the bug's text followed by where it began.
pub fn report(operation: &str, message: &str) -> (String, String) {
    let at = format!("{}:{PANIC_LINE}:5", file!());
    (operation.to_string(), format!("{operation} panicked: {message} (at {at})"))
}
