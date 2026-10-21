use std::process::Command;

fn main() {
    set_version_env();
}

fn set_version_env() {
    // For local dev builds, derive version from git.
    // CI sets BAE_VERSION env var before building releases.
    let version = std::env::var("BAE_VERSION").unwrap_or_else(|_| derive_version_from_git());

    println!("cargo:rustc-env=BAE_VERSION={}", version);
    println!("cargo:rerun-if-env-changed=BAE_VERSION");

    // Rerun only when `git describe`'s inputs change: HEAD (a checkout) and the
    // tags. Without these, cargo reruns the build script every build, gets a
    // potentially different `git describe` output, and recompiles everything.
    //
    // HEAD belongs to the worktree's own git dir, the tags to the git dir every
    // worktree shares. A linked worktree's git dir (.git/worktrees/<name>) has
    // no refs/tags, and cargo reruns a build script whose rerun-if-changed path
    // is missing on every build, which rebuilt bae-core and everything above it
    // each time.
    if let (Some(git_dir), Some(common_dir)) = (git_path("--git-dir"), git_path("--git-common-dir"))
    {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={common_dir}/refs/tags");
    }
}

/// An absolute path `git rev-parse` reports, e.g. `--git-dir`.
fn git_path(which: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", which])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn derive_version_from_git() -> String {
    let output = match Command::new("git")
        .args(["describe", "--tags", "--always"])
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            println!("cargo:warning=git describe failed to spawn ({err}); BAE_VERSION=dev");
            return "dev".to_string();
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!(
            "cargo:warning=git describe exited {} ({}); BAE_VERSION=dev",
            output.status,
            stderr.trim()
        );
        return "dev".to_string();
    }

    match String::from_utf8(output.stdout) {
        Ok(s) => s.trim().to_string(),
        Err(err) => {
            println!(
                "cargo:warning=git describe produced non-utf8 output ({err}); BAE_VERSION=dev"
            );
            "dev".to_string()
        }
    }
}
