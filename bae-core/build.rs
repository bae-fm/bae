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

    // Tell cargo to only rerun when the git HEAD changes (new commit, checkout, etc.)
    // Without this, cargo reruns the build script every build, gets a potentially
    // different `git describe` output, and recompiles everything.
    if let Ok(git_dir) = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    {
        println!("cargo:rerun-if-changed={}/HEAD", git_dir);
        println!("cargo:rerun-if-changed={}/refs/tags", git_dir);
    }
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
