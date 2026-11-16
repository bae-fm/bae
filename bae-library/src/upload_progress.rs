//! How far the files bae is uploading have got, as a bar draws it.

/// The bytes an upload has sent of its total. The outbox watch builds every
/// bar and keeps its bytes done within its total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl UploadBar {
    /// The share of the bar's bytes done, from 0 to 1, which the bar fills
    /// to. When the total is zero bytes, the share is 0.
    pub fn fraction(self) -> f64 {
        bae_types::progress::share_done(self.bytes_done, self.bytes_total)
    }
}

#[cfg(test)]
#[path = "upload_progress_tests.rs"]
mod tests;
