//! Screenshot generator for marketing/docs
//!
//! This binary:
//! 1. Creates a temp database with fixture data
//! 2. Launches the Bae app
//! 3. Captures screenshots of different views
//!
//! Usage: cargo run --release --bin generate_screenshots

use bae::db::Database;
use bae::fixtures::screenshots::{fixtures_dir, load_fixtures};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

const SCREENSHOT_DELAY_MS: u64 = 3000;

fn main() {
    // Set up temp directory for screenshot database
    let temp_dir = std::env::temp_dir().join("bae-screenshots");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).expect("Failed to clean temp dir");
    }
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    println!("Setting up screenshot database in {:?}", temp_dir);

    // Create database and load fixtures
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime");
    runtime.block_on(async {
        let db_path = temp_dir.join("library.db");
        let db = Database::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create database");

        let fixtures = fixtures_dir();
        println!("Loading fixtures from {:?}", fixtures);

        load_fixtures(&db, &fixtures)
            .await
            .expect("Failed to load fixtures");

        println!("Fixtures loaded successfully");
    });

    // Create output directory
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    std::fs::create_dir_all(&output_dir).expect("Failed to create output dir");

    println!("Screenshots will be saved to {:?}", output_dir);

    // Set environment to use temp library
    std::env::set_var("BAE_LIBRARY_PATH", temp_dir.to_str().unwrap());

    // Launch the app
    println!("Launching Bae app...");
    let app_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("bae");

    if !app_path.exists() {
        eprintln!("Error: Release build not found at {:?}", app_path);
        eprintln!("Run `cargo build --release` first");
        std::process::exit(1);
    }

    let mut app_process = Command::new(&app_path)
        .spawn()
        .expect("Failed to launch app");

    // Wait for app to start
    println!("Waiting for app to load...");
    thread::sleep(Duration::from_millis(SCREENSHOT_DELAY_MS));

    // Capture screenshots using macOS screencapture
    capture_screenshots(&output_dir, &mut app_process);

    // Clean up
    println!("Shutting down app...");
    let _ = app_process.kill();

    println!("\nDone! Screenshots saved to {:?}", output_dir);
}

#[cfg(target_os = "macos")]
fn capture_screenshots(output_dir: &std::path::Path, _app: &mut Child) {
    let screenshots = [
        ("library-grid.png", "Library view"),
        // Add more screenshot definitions as needed
    ];

    for (filename, description) in screenshots {
        println!("Capturing: {}", description);
        let output_path = output_dir.join(filename);

        // Use screencapture to capture the frontmost window
        // -l flag captures a specific window, -w captures interactively
        // For automation, we capture the whole screen and can crop later
        // Or use -l with window ID (would need to find it)
        let status = Command::new("screencapture")
            .args([
                "-x", // No sound
                "-o", // No shadow
                "-t",
                "png",
                output_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run screencapture");

        if status.success() {
            println!("  Saved: {:?}", output_path);
        } else {
            eprintln!("  Failed to capture screenshot");
        }

        thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_screenshots(_output_dir: &std::path::Path, _app: &mut Child) {
    eprintln!("Screenshot capture is only supported on macOS currently");
}
