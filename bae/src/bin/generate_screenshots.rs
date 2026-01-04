//! Screenshot generator for marketing/docs
//!
//! This binary:
//! 1. Creates a temp database with fixture data
//! 2. Launches the bae app
//! 3. Captures screenshots of different views
//!
//! Usage: cargo run --release --bin generate_screenshots

use bae::db::{Database, DbAlbum, DbAlbumArtist, DbArtist};
use bae::ui::local_file_url::local_file_url;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct FixtureData {
    albums: Vec<FixtureAlbum>,
}

#[derive(Debug, Deserialize)]
struct FixtureAlbum {
    artist: String,
    title: String,
    year: i32,
}

/// Load screenshot fixtures into the database
async fn load_fixtures(
    db: &Database,
    fixtures_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_path = fixtures_dir.join("data.json");
    let data: FixtureData = serde_json::from_str(&std::fs::read_to_string(&data_path)?)?;

    let covers_dir = fixtures_dir.join("covers");
    let mut artist_ids: HashMap<String, String> = HashMap::new();
    let now = Utc::now();

    for album_data in &data.albums {
        let artist_id = if let Some(id) = artist_ids.get(&album_data.artist) {
            id.clone()
        } else {
            let artist = DbArtist {
                id: Uuid::new_v4().to_string(),
                name: album_data.artist.clone(),
                sort_name: None,
                discogs_artist_id: None,
                bandcamp_artist_id: None,
                created_at: now,
                updated_at: now,
            };
            db.insert_artist(&artist).await?;
            artist_ids.insert(album_data.artist.clone(), artist.id.clone());
            artist.id
        };

        let cover_filename = format!(
            "{}_{}.png",
            album_data
                .artist
                .to_lowercase()
                .replace(' ', "-")
                .replace('\'', ""),
            album_data
                .title
                .to_lowercase()
                .replace(' ', "-")
                .replace('\'', "")
        );
        let cover_path = covers_dir.join(&cover_filename);
        let cover_url = if cover_path.exists() {
            Some(local_file_url(cover_path.to_str().unwrap()))
        } else {
            None
        };

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: album_data.title.clone(),
            year: Some(album_data.year),
            discogs_release: None,
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_image_id: None,
            cover_art_url: cover_url,
            is_compilation: false,
            created_at: now,
            updated_at: now,
        };
        db.insert_album(&album).await?;

        let album_artist = DbAlbumArtist {
            id: Uuid::new_v4().to_string(),
            album_id: album.id.clone(),
            artist_id,
            position: 0,
        };
        db.insert_album_artist(&album_artist).await?;
    }

    println!("Loaded {} fixture albums", data.albums.len());
    Ok(())
}

fn fixtures_dir() -> PathBuf {
    // Allow override via environment variable for website screenshot generation
    if let Ok(path) = std::env::var("BAE_SCREENSHOT_FIXTURES_DIR") {
        PathBuf::from(path)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("screenshots")
    }
}

const SCREENSHOT_DELAY_MS: u64 = 8000; // Give app time to fully load library

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
    // Allow override via environment variable for website screenshot generation
    let output_dir = if let Ok(path) = std::env::var("BAE_SCREENSHOT_OUTPUT_DIR") {
        PathBuf::from(path)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("screenshots")
    };
    std::fs::create_dir_all(&output_dir).expect("Failed to create output dir");

    println!("Screenshots will be saved to {:?}", output_dir);

    // Set environment for the app
    // Use dev mode so it reads from env vars instead of keyring
    std::env::set_var("BAE_DEV_MODE", "1");
    std::env::set_var("BAE_LIBRARY_PATH", temp_dir.to_str().unwrap());
    // Screenshot mode disables network features to avoid permission dialogs
    std::env::set_var("BAE_SCREENSHOT_MODE", "1");

    // Set dummy credentials - we don't need real ones for screenshots
    // The app handles unavailable cloud storage gracefully
    std::env::set_var("BAE_DISCOGS_API_KEY", "dummy-for-screenshots");
    std::env::set_var("BAE_S3_BUCKET", "dummy");
    std::env::set_var("BAE_S3_REGION", "us-east-1");
    std::env::set_var("BAE_S3_ACCESS_KEY", "dummy");
    std::env::set_var("BAE_S3_SECRET_KEY", "dummy");

    // Launch the app - check multiple possible locations
    println!("Launching bae app...");
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Possible binary locations (in order of preference)
    let possible_paths = [
        // After dx bundle: binary inside .app bundle
        manifest_dir.join("target/dx/bae/bundle/macos/bundle/macos/bae.app/Contents/MacOS/bae"),
        // After cargo build --release
        manifest_dir.join("target/release/bae"),
    ];

    let app_path = possible_paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("Error: Release build not found. Checked:");
            for p in &possible_paths {
                eprintln!("  - {:?}", p);
            }
            eprintln!("Run `dx bundle --release` or `cargo build --release` first");
            std::process::exit(1);
        });

    println!("Using binary at: {:?}", app_path);

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
    // Resize the app window to a good screenshot size
    resize_bae_window(1920, 1080);
    thread::sleep(Duration::from_millis(1000)); // Let UI re-render

    // Get the CGWindowID for proper window capture with rounded corners
    let window_id = get_bae_cg_window_id();

    let screenshots = [
        ("library-grid.png", "Library view"),
        // Add more screenshot definitions as needed
    ];

    for (filename, description) in screenshots {
        println!("Capturing: {}", description);
        let output_path = output_dir.join(filename);

        let status = if let Some(wid) = &window_id {
            // Capture window by CGWindowID - preserves rounded corners with alpha
            Command::new("screencapture")
                .args([
                    "-x", // No sound
                    "-o", // No shadow
                    "-l",
                    wid, // CGWindowID
                    "-t",
                    "png",
                    output_path.to_str().unwrap(),
                ])
                .status()
                .expect("Failed to run screencapture")
        } else {
            // Fallback: capture entire screen
            println!("  Warning: Could not find bae window, capturing full screen");
            Command::new("screencapture")
                .args(["-x", "-t", "png", output_path.to_str().unwrap()])
                .status()
                .expect("Failed to run screencapture")
        };

        if status.success() {
            println!("  Saved: {:?}", output_path);
        } else {
            eprintln!("  Failed to capture screenshot");
        }

        thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(target_os = "macos")]
fn get_bae_cg_window_id() -> Option<String> {
    // Use Swift with Quartz to get the actual CGWindowID
    let script = r#"
import Quartz
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
if let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] {
    for window in windowList {
        if let name = window[kCGWindowOwnerName as String] as? String,
           name.contains("bae"),
           let num = window[kCGWindowNumber as String] as? Int {
            print(num)
            break
        }
    }
}
"#;

    let output = Command::new("swift").args(["-e", script]).output().ok()?;

    if output.status.success() {
        let wid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !wid.is_empty() {
            println!("Found bae CGWindowID: {}", wid);
            return Some(wid);
        }
    }

    eprintln!(
        "Warning: Could not get bae CGWindowID: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    None
}

#[cfg(target_os = "macos")]
fn resize_bae_window(width: u32, height: u32) {
    let script = format!(
        r#"
        tell application "System Events"
            set baeProcess to first process whose name is "bae"
            tell baeProcess
                set frontmost to true
                if (count of windows) > 0 then
                    set position of window 1 to {{0, 0}}
                    set size of window 1 to {{{}, {}}}
                end if
            end tell
        end tell
    "#,
        width, height
    );

    let output = Command::new("osascript").args(["-e", &script]).output();

    match output {
        Ok(out) if out.status.success() => {
            println!("Resized bae window to {}x{}", width, height);
        }
        Ok(out) => {
            eprintln!(
                "Warning: Could not resize window: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => {
            eprintln!("Warning: Failed to run resize script: {}", e);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_screenshots(_output_dir: &std::path::Path, _app: &mut Child) {
    eprintln!("Screenshot capture is only supported on macOS currently");
}
