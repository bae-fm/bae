//! The originals a read shows, as this device's disk has answered for them.
//! The read's rows go out as soon as the database has them; the disk is asked
//! on a task of its own, since the volume the originals sit on can be slow to
//! answer, and its answer reaches the read like any other change.

use crate::album_detail::OriginalsSeen;
use crate::db::RecordedOriginal;
use crate::library::{LibraryError, LibraryManager};

/// What the disk last said of each original a read shows, and the one look
/// at the disk in flight for them.
pub(super) struct ShownOriginals {
    manager: LibraryManager,
    shown: Vec<RecordedOriginal>,
    seen: OriginalsSeen,
    look: Option<tokio::task::JoinHandle<Result<OriginalsSeen, LibraryError>>>,
    /// Whether the read showed its originals again since the look in flight
    /// began, so the disk is asked again once that look answers.
    shown_since: bool,
}

impl ShownOriginals {
    pub(super) fn new(manager: LibraryManager) -> Self {
        Self {
            manager,
            shown: Vec::new(),
            seen: OriginalsSeen::default(),
            look: None,
            shown_since: false,
        }
    }

    /// Show `originals` from now on and ask the disk about them again. What
    /// the disk last said of them stays shown until it answers. A look already
    /// in flight finishes first, so a read showing its originals faster than
    /// the disk answers still gets each answer.
    pub(super) fn show(&mut self, originals: Vec<RecordedOriginal>) {
        self.seen.retain(&originals);
        self.shown = originals;
        if self.look.is_some() {
            self.shown_since = true;
        } else {
            self.ask();
        }
    }

    /// What the disk has said of the originals shown.
    pub(super) fn seen(&self) -> &OriginalsSeen {
        &self.seen
    }

    /// Wait until the disk answers; whether the answer changes what is shown.
    /// Cancel-safe: an answer stays owed until a call returns it.
    pub(super) async fn answered(&mut self) -> Result<bool, LibraryError> {
        let Some(look) = self.look.as_mut() else {
            return std::future::pending().await;
        };
        let answer = look.await;
        self.look = None;
        let answer = match answer {
            Ok(answer) => answer?,
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(error) => {
                return Err(LibraryError::Internal(format!(
                    "the look at this device's originals ended: {error}"
                )))
            }
        };
        let before = self.seen.clone();
        self.seen.extend(answer);
        self.seen.retain(&self.shown);
        if std::mem::take(&mut self.shown_since) {
            self.ask();
        }
        Ok(self.seen != before)
    }

    fn ask(&mut self) {
        if self.shown.is_empty() {
            return;
        }
        let manager = self.manager.clone();
        let originals = self.shown.clone();
        self.look = Some(tokio::spawn(async move {
            manager.look_at_originals(originals).await
        }));
    }
}

impl Drop for ShownOriginals {
    fn drop(&mut self) {
        if let Some(look) = &self.look {
            look.abort();
        }
    }
}
