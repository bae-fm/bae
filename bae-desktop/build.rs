use std::path::Path;
use std::process::Command;

fn main() {
    set_version_env();
    copy_shared_assets();
    generate_tailwind();
}

fn set_version_env() {
    // For local dev builds, derive version from git.
    // CI sets BAE_VERSION env var before building releases.
    let version = std::env::var("BAE_VERSION").unwrap_or_else(|_| {
        Command::new("git")
            .args(["describe", "--tags", "--always"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "dev".to_string())
    });

    println!("cargo:rustc-env=BAE_VERSION={}", version);
    println!("cargo:rerun-if-env-changed=BAE_VERSION");
}

fn copy_shared_assets() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let shared_main_css = Path::new(manifest_dir).join("../bae-ui/assets/main.css");
    let local_main_css = Path::new(manifest_dir).join("assets/main.css");

    println!("cargo:rerun-if-changed={}", shared_main_css.display());

    if shared_main_css.exists() {
        std::fs::copy(&shared_main_css, &local_main_css)
            .expect("Failed to copy main.css from bae-ui");
    }
}

fn generate_tailwind() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let tailwind_input = Path::new(manifest_dir).join("tailwind.css");
    let tailwind_output = Path::new(manifest_dir).join("assets/tailwind.css");
    println!("cargo:rerun-if-changed={}", tailwind_input.display());
    println!(
        "cargo:rerun-if-changed={}",
        Path::new(manifest_dir).join("tailwind.config.js").display(),
    );
    // Use node_modules/.bin/tailwindcss directly - more reliable than npx in CI
    let tailwind_bin = Path::new(manifest_dir).join("node_modules/.bin/tailwindcss");
    let output = Command::new(&tailwind_bin)
        .args([
            "-i",
            tailwind_input.to_str().unwrap(),
            "-o",
            tailwind_output.to_str().unwrap(),
        ])
        .current_dir(manifest_dir)
        .output();
    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Failed to generate Tailwind CSS");
                eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
                eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
                panic!("Tailwind CSS generation failed");
            } else {
                println!("cargo:warning=Tailwind CSS generated successfully");
            }
        }
        Err(e) => {
            panic!("Failed to run tailwindcss: {}", e);
        }
    }
}
