// Cargo runs `<wrapper> <rustc> <args...>`. When that command line is longer
// than Windows' 32K limit (ffmpeg-next's is about 94K on macOS: one `--cfg` per
// FFmpeg API its build script detects), cargo retries with the arguments in an
// `@argfile`. sccache expands the argfile and spawns rustc with the full
// command line again, which fails with os error 206
// (https://github.com/mozilla/sccache/issues/2397). So a compile cargo already
// moved into an argfile runs rustc directly, uncached, and every other compile
// goes through sccache.
use std::env;
use std::process::{exit, Command};

fn main() {
    let mut args = env::args_os().skip(1);
    let rustc = args.next().expect("cargo passes the rustc path first");
    let rest: Vec<_> = args.collect();
    let in_argfile = rest
        .iter()
        .any(|arg| arg.to_str().is_some_and(|arg| arg.starts_with('@')));
    let mut command = if in_argfile {
        Command::new(&rustc)
    } else {
        let mut command = Command::new("sccache");
        command.arg(&rustc);
        command
    };
    match command.args(&rest).status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("rustc-wrapper: failed to run {command:?}: {error}");
            exit(1);
        }
    }
}
