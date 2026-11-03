/// What the context lane plays from: a single release, several releases one
/// after another in the order they were chosen, a list of tracks in the order
/// it was sent, or the whole library. The queue screens label the lane by it;
/// the lane's tracks are in the queue tables, filled once when it starts
/// playing.
///
/// `Releases` always stands for at least two releases: a single-release play
/// collapses to `Release` (see [`ContextSource::releases`]). Which releases
/// they were, or which tracks, is the lane's tracks, not the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextSource {
    Release(String),
    Releases,
    Tracks,
    Library,
}

impl ContextSource {
    /// The source of a play of an ordered, non-empty set of release ids,
    /// collapsing a single id to [`ContextSource::Release`] so a one-album play
    /// behaves exactly like playing that release. The caller guarantees a
    /// non-empty list.
    pub fn releases(mut ids: Vec<String>) -> ContextSource {
        debug_assert!(!ids.is_empty(), "a release source needs at least one id");
        if ids.len() == 1 {
            ContextSource::Release(ids.remove(0))
        } else {
            ContextSource::Releases
        }
    }

    /// The source as the `playback_queue.source_kind` and
    /// `source_release_id` columns hold it.
    pub(crate) fn to_columns(&self) -> (&'static str, Option<&str>) {
        match self {
            ContextSource::Release(id) => ("release", Some(id)),
            ContextSource::Releases => ("releases", None),
            ContextSource::Tracks => ("tracks", None),
            ContextSource::Library => ("library", None),
        }
    }

    /// The source the `playback_queue.source_kind` and `source_release_id`
    /// columns hold. A pair [`Self::to_columns`] never writes is refused.
    pub(crate) fn from_columns(
        kind: &str,
        release_id: Option<String>,
    ) -> Result<ContextSource, String> {
        match (kind, release_id) {
            ("release", Some(id)) => Ok(ContextSource::Release(id)),
            ("releases", None) => Ok(ContextSource::Releases),
            ("tracks", None) => Ok(ContextSource::Tracks),
            ("library", None) => Ok(ContextSource::Library),
            (kind, release_id) => Err(format!(
                "the queue's source {kind:?} with release {release_id:?} is not one bae writes"
            )),
        }
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
