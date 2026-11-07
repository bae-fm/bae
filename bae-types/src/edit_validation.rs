//! Why what a person typed into an edit can't be saved.

/// Why an edit can't be shaped into a savable one. The editor surfaces this
/// to disable Save, and a commit that reaches it writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditValidationError {
    #[error("Album title is required")]
    EmptyAlbumTitle,
    #[error("Album must have at least one artist")]
    NoAlbumArtist,
    #[error("Artist name is required")]
    EmptyArtistName,
    #[error("Year must be a number")]
    InvalidYear,
}
