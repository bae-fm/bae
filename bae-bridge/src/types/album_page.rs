//! The album page's read: what it asks for, and the album with the release it
//! shows.

/// What the album page reads: the album, and the release the person chose on
/// its page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeAlbumDetailRequest {
    pub album_id: String,
    /// The release the person chose on the album's page, or none to show the
    /// primary release.
    pub chosen_release_id: Option<String>,
}

impl From<BridgeAlbumDetailRequest> for bae_views::album_detail::AlbumDetailRequest {
    fn from(
        BridgeAlbumDetailRequest { album_id, chosen_release_id }: BridgeAlbumDetailRequest,
    ) -> Self {
        Self { album_id, chosen_release_id }
    }
}

impl From<bae_views::album_detail::AlbumDetailRequest> for BridgeAlbumDetailRequest {
    fn from(request: bae_views::album_detail::AlbumDetailRequest) -> Self {
        let bae_views::album_detail::AlbumDetailRequest { album_id, chosen_release_id } = request;
        Self { album_id, chosen_release_id }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumDetail {
    pub album: super::BridgeAlbum,
    /// The album's releases in album order, the primary release first.
    pub releases: Vec<super::BridgeRelease>,
    /// The release the page shows: the one its request chose while the album
    /// holds it, else the primary release.
    pub shown_release: super::BridgeRelease,
    /// The shown release's place in `releases`, which the release picker
    /// marks.
    pub shown_release_index: u32,
    /// Whether the shown release is not the primary release, so the page
    /// offers Set as primary release.
    pub shown_release_can_become_primary: bool,
    /// Whether the album has more than one release, so its page offers a
    /// choice between them.
    pub shows_release_choices: bool,
    /// Who rated the album, and whether this person can.
    pub rating: super::BridgeAlbumRating,
}
