//! The pinned Chrome for Testing build the boards render in.

use crate::capture::Step;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures::StreamExt;
use std::path::{Path, PathBuf};
use tokio::task::JoinHandle;

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

/// Holds an exclusive lock on `path` until the returned file is dropped.
fn lock(path: &Path) -> Result<std::fs::File, ChromeError> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| ChromeError::Fetch(format!("{}: {e}", path.display())))?;
    file.lock().map_err(|e| ChromeError::Fetch(format!("{}: {e}", path.display())))?;
    Ok(file)
}

/// The pinned browser's executable, downloaded and unpacked into the cache
/// when it is not there. The unpacking happens beside the final folder and is
/// renamed into place, so the folder is either whole or absent.
fn installed() -> Result<PathBuf, ChromeError> {
    let platform = Platform::of(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        ChromeError::Fetch(format!(
            "no Chrome for Testing for {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let cache = cache_dir()?;
    std::fs::create_dir_all(&cache).map_err(|e| ChromeError::Fetch(e.to_string()))?;
    // Two fetches at once, in this process or another, would unpack twice.
    let _fetching = lock(&cache.join("fetch.lock"))?;
    let folder = platform.folder(&cache, PINNED);
    let executable = folder.join(platform.executable);
    if executable.is_file() {
        return Ok(executable);
    }
    let archive = cache.join(format!("{PINNED}-{}.zip", platform.word));
    let partial = cache.join(format!("{PINNED}-{}.partial", platform.word));
    let fetch = |e: std::io::Error| ChromeError::Fetch(e.to_string());
    if partial.exists() {
        std::fs::remove_dir_all(&partial).map_err(fetch)?;
    }
    let (archive_arg, partial_arg) = (archive.to_string_lossy(), partial.to_string_lossy());
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
            &["-q", &archive_arg, "-d", &partial_arg],
        ),
    ] {
        step.run().map_err(|e| ChromeError::Fetch(e.to_string()))?;
    }
    std::fs::rename(&partial, &folder).map_err(fetch)?;
    std::fs::remove_file(&archive).map_err(fetch)?;
    Ok(executable)
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
    handler: JoinHandle<()>,
    /// The browser's profile; one browser per directory.
    _profile: tempfile::TempDir,
    /// What the browser reports as its product, e.g. `Chrome/157.0.8103.78`.
    version: String,
}

impl Chrome {
    /// Fetches the pinned build when the cache does not hold it, and starts it
    /// headless with sRGB colour and no scrollbars.
    pub(crate) async fn launch() -> Result<Self, ChromeError> {
        let executable = installed()?;
        let profile = tempfile::tempdir().map_err(|e| ChromeError::Launch(e.to_string()))?;
        let config = BrowserConfig::builder()
            .user_data_dir(profile.path())
            .chrome_executable(executable)
            .viewport(None)
            .args(["--force-color-profile=srgb", "--hide-scrollbars"])
            .build()
            .map_err(ChromeError::Launch)?;
        let (browser, mut handler) =
            Browser::launch(config).await.map_err(|e| ChromeError::Launch(e.to_string()))?;
        let handler = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let version =
            browser.version().await.map_err(|e| ChromeError::Protocol(e.to_string()))?.product;
        Ok(Self { browser, handler, _profile: profile, version })
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) async fn new_page(&self) -> Result<Page, ChromeError> {
        self.browser.new_page("about:blank").await.map_err(|e| ChromeError::Protocol(e.to_string()))
    }

    pub(crate) async fn close(mut self) {
        let _ = self.browser.close().await;
        let _ = self.browser.wait().await;
        self.handler.abort();
    }
}

#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
