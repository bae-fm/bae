//! How far the files being sent have got, as a bar draws it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl UploadBar {
    /// The share of the bar's bytes done, from 0 to 1, which the bar fills
    /// to. A bar of no bytes has done none of them.
    pub fn fraction(self) -> f64 {
        assert!(self.bytes_done <= self.bytes_total, "an upload bar's bytes done pass its total");
        bae_types::progress::share_done(self.bytes_done, self.bytes_total)
    }
}

#[cfg(test)]
#[path = "upload_progress_tests.rs"]
mod tests;
