//! The pinned Chrome for Testing build the boards render in.

use crate::capture::Step;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures::StreamExt;
use std::path::{Path, PathBuf};

/// The Chrome for Testing build every board hash names.
const PINNED: &str = "157.0.8103.78";

/// Where fetched builds are kept: `BAE_SHOTS_CHROME_CACHE`, else the build
/// volume, else the user's cache directory.
fn cache_dir() -> Result<PathBuf, ChromeError> {
    if let Some(dir) = std::env::var_os("BAE_SHOTS_CHROME_CACHE") {
        return Ok(PathBuf::from(dir));
    }
    let volume = PathBuf::from("/Volumes/Builds/bae");
    if volume.is_dir() {
        return Ok(volume.join("chrome"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| ChromeError::Fetch("no BAE_SHOTS_CHROME_CACHE and no HOME".to_string()))?;
    Ok(PathBuf::from(home).join(".cache/bae-shots/chrome"))
}

/// A platform Chrome for Testing is published for.
#[derive(Debug, PartialEq, Eq)]
struct Platform {
    /// The platform's name in the download URL and the archive.
    word: &'static str,
    /// The browser's executable inside the unpacked archive.
    executable: &'static str,
}

impl Platform {
    fn of(os: &str, arch: &str) -> Option<Self> {
        match (os, arch) {
            ("macos", "aarch64") => Some(Self {
                word: "mac-arm64",
                executable: "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            }),
            ("macos", "x86_64") => Some(Self {
                word: "mac-x64",
                executable: "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            }),
            ("linux", "x86_64") => {
                Some(Self { word: "linux64", executable: "chrome-linux64/chrome" })
            }
            _ => None,
        }
    }

    fn url(&self, version: &str) -> String {
        format!(
            "https://storage.googleapis.com/chrome-for-testing-public/{version}/{0}/chrome-{0}.zip",
            self.word
        )
    }

    /// The folder `version` unpacks into under the cache.
    fn folder(&self, cache: &Path, version: &str) -> PathBuf {
        cache.join(format!("{version}-{}", self.word))
    }
}

/// The pinned browser's executable, downloaded and unpacked into the cache
/// when it is not there. Each run unpacks in a folder of its own and renames
/// it into place, so the folder is whole or absent; of two runs that fetch at
/// once, the one that renames second finds the build there and uses it.
fn installed() -> Result<PathBuf, ChromeError> {
    let platform = Platform::of(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        ChromeError::Fetch(format!(
            "no Chrome for Testing for {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let cache = cache_dir()?;
    let fetch = |e: std::io::Error| ChromeError::Fetch(e.to_string());
    std::fs::create_dir_all(&cache).map_err(fetch)?;
    let folder = platform.folder(&cache, PINNED);
    let executable = folder.join(platform.executable);
    if executable.is_file() {
        return Ok(executable);
    }
    let work = tempfile::tempdir_in(&cache).map_err(fetch)?;
    let archive = work.path().join("chrome.zip");
    let unpacked = work.path().join("unpacked");
    let (archive_arg, unpacked_arg) = (archive.to_string_lossy(), unpacked.to_string_lossy());
    for step in [
        Step::new(
            "download Chrome for Testing",
            &cache,
            "curl",
            &[
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--output",
                &archive_arg,
                &platform.url(PINNED),
            ],
        ),
        Step::new(
            "unpack Chrome for Testing",
            &cache,
            "unzip",
            &["-q", &archive_arg, "-d", &unpacked_arg],
        ),
    ] {
        step.run().map_err(|e| ChromeError::Fetch(e.to_string()))?;
    }
    match std::fs::rename(&unpacked, &folder) {
        Ok(()) => Ok(executable),
        Err(_) if executable.is_file() => Ok(executable),
        Err(e) => Err(fetch(e)),
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ChromeError {
    #[error("cannot fetch Chrome for Testing: {0}")]
    Fetch(String),
    #[error("cannot start Chrome: {0}")]
    Launch(String),
    #[error("Chrome failed: {0}")]
    Protocol(String),
}

pub(crate) struct Chrome {
    browser: Browser,
    /// What the browser reports as its product, e.g. `Chrome/157.0.8103.78`.
    version: String,
}

impl Chrome {
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) async fn new_page(&self) -> Result<Page, ChromeError> {
        self.browser.new_page("about:blank").await.map_err(|e| ChromeError::Protocol(e.to_string()))
    }
}

/// Fetches the pinned build when the cache does not hold it, starts it
/// headless with sRGB colour and no scrollbars, runs `work` with it and closes
/// it. The browser's events are driven alongside `work` and end with it, so
/// no task outlives the call.
pub(crate) async fn with_chrome<T>(work: impl AsyncFnOnce(&Chrome) -> T) -> Result<T, ChromeError> {
    let executable = installed()?;
    let profile = tempfile::tempdir().map_err(|e| ChromeError::Launch(e.to_string()))?;
    let config = BrowserConfig::builder()
        .user_data_dir(profile.path())
        .chrome_executable(executable)
        .viewport(None)
        .args(["--force-color-profile=srgb", "--hide-scrollbars"])
        .build()
        .map_err(ChromeError::Launch)?;
    let (browser, mut events) =
        Browser::launch(config).await.map_err(|e| ChromeError::Launch(e.to_string()))?;
    let running = async move {
        let version =
            browser.version().await.map_err(|e| ChromeError::Protocol(e.to_string()))?.product;
        let mut chrome = Chrome { browser, version };
        let out = work(&chrome).await;
        if let Err(error) = chrome.browser.close().await {
            eprintln!("bae-shots: closing Chrome failed: {error}");
        }
        if let Err(error) = chrome.browser.wait().await {
            eprintln!("bae-shots: waiting for Chrome failed: {error}");
        }
        Ok(out)
    };
    // The events end when the browser closes, which `running` does last; a
    // browser that dies before that fails the work's requests.
    let driven = async {
        while events.next().await.is_some() {}
        std::future::pending::<std::convert::Infallible>().await
    };
    tokio::select! {
        out = running => out,
        never = driven => match never {},
    }
}

#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
