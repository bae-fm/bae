//! Which library label a release's label names, as the rule every write of a
//! release's labels decides by reads it (see `client::label_resolution`):
//! a library label as a write names it, and the conflict a person settles.

use super::*;

/// A library label as a write that decides labels names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingLabel {
    pub label_id: String,
    pub name: String,
    pub musicbrainz_label_id: Option<String>,
    pub discogs_label_id: Option<String>,
}

/// A MusicBrainz and a Discogs id one incoming label carries that two
/// library labels hold. Import stops until the person chooses how this label
/// is used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("label '{incoming_label_name}' has catalog ids belonging to different library labels")]
pub struct LabelIdentityConflict {
    pub incoming_label_name: String,
    pub discogs_label_id: String,
    pub musicbrainz_label_id: String,
    pub discogs_label: ExistingLabel,
    pub musicbrainz_label: ExistingLabel,
}

/// Whether every catalog id two statements of one label carry agrees. An
/// absent id makes no claim; two present ids of one catalog must be equal.
pub fn label_ids_are_compatible(
    label: &ExistingLabel,
    musicbrainz_label_id: Option<&str>,
    discogs_label_id: Option<&str>,
) -> bool {
    source_ids_are_compatible(label.musicbrainz_label_id.as_deref(), musicbrainz_label_id)
        && source_ids_are_compatible(label.discogs_label_id.as_deref(), discogs_label_id)
}
