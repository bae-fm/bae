//! One library artist or label merged into another, as a write does on
//! finding one catalog id on several: the survivor takes the absorbed one's
//! catalog ids, and the absorbed one stays, recorded as merged into the
//! survivor (`artist_merges`, `label_merges`). Neither row is deleted and no
//! credit or release label is rewritten: credits and release labels may name
//! the absorbed one, and every read shows the survivor.

use super::*;
use bae_store::WriteStatements as _;

/// An artist merged into another, with the catalog ids the survivor takes
/// where it holds none of that catalog.
pub(crate) struct ArtistMerge<'a> {
    pub survivor: &'a str,
    pub absorbed: &'a str,
    pub discogs_artist_id: Option<&'a str>,
    pub musicbrainz_artist_id: Option<&'a str>,
    /// The sort name the survivor takes where it has none.
    pub sort_name: Option<&'a str>,
}

// Give the artist `to` an image row that names the same coven file as the
// artist `from`'s.
coven::write!(
    ShareArtistImage(to: &'p str, from: &'p str),
    r#"
    INSERT INTO artist_images (id, file, content_type)
    SELECT :to, file, content_type FROM artist_images WHERE id = :from
"#
);

coven::write!(
    DeleteArtistImage(artist_id: &'p str),
    "DELETE FROM artist_images WHERE id = :artist_id"
);

coven::write!(
    MoveArtistMerges(survivor: &'p str, absorbed: &'p str),
    "UPDATE artist_merges SET into_artist_id = :survivor WHERE into_artist_id = :absorbed"
);

coven::write!(
    InsertArtistMerge(absorbed: &'p str, survivor: &'p str),
    "INSERT INTO artist_merges (id, into_artist_id) VALUES (:absorbed, :survivor)"
);

/// Merge `merge.absorbed` into `merge.survivor` inside the caller's write.
/// A survivor without an image of its own takes the absorbed artist's, its
/// row naming the same coven file, so nothing is uploaded again; the
/// absorbed artist's image row goes.
pub(crate) fn merge_artist_on(
    conn: &CovenWrite<'_>,
    merge: &ArtistMerge<'_>,
) -> Result<(), DbError> {
    let image =
        |artist_id: &str| super::files::image_file_on(conn, &LibraryImageType::Artist, artist_id);
    let (surviving_image, absorbed_image) = (image(merge.survivor)?, image(merge.absorbed)?);
    if surviving_image.is_none() && absorbed_image.is_some() {
        conn.run(&ShareArtistImageParams { to: merge.survivor, from: merge.absorbed })?;
    }
    if absorbed_image.is_some() {
        conn.run(&DeleteArtistImageParams { artist_id: merge.absorbed })?;
    }
    let changed = super::write::update_artist_external_ids_row(
        conn,
        merge.survivor,
        merge.discogs_artist_id,
        merge.musicbrainz_artist_id,
        merge.sort_name,
    )?;
    // The survivor is an artist this write read, and its id names one row.
    assert_eq!(changed, 1, "an artist merge updates exactly its surviving artist");
    // A chain that ended at the absorbed artist now ends at the survivor.
    conn.run(&MoveArtistMergesParams { survivor: merge.survivor, absorbed: merge.absorbed })?;
    conn.run(&InsertArtistMergeParams { absorbed: merge.absorbed, survivor: merge.survivor })?;
    Ok(())
}

/// A label merged into another, with the catalog ids the survivor takes
/// where it holds none of that catalog.
pub(crate) struct LabelMerge<'a> {
    pub survivor: &'a str,
    pub absorbed: &'a str,
    pub discogs_label_id: Option<&'a str>,
    pub musicbrainz_label_id: Option<&'a str>,
}

coven::write!(
    FillLabelIds(
        id: &'p str,
        musicbrainz_label_id: Option<&'p str>,
        discogs_label_id: Option<&'p str>,
    ),
    r#"
    UPDATE labels SET
        musicbrainz_label_id = COALESCE(musicbrainz_label_id, :musicbrainz_label_id),
        discogs_label_id = COALESCE(discogs_label_id, :discogs_label_id)
    WHERE id = :id
"#
);

coven::write!(
    MoveLabelMerges(survivor: &'p str, absorbed: &'p str),
    "UPDATE label_merges SET into_label_id = :survivor WHERE into_label_id = :absorbed"
);

coven::write!(
    InsertLabelMerge(absorbed: &'p str, survivor: &'p str),
    "INSERT INTO label_merges (id, into_label_id) VALUES (:absorbed, :survivor)"
);

/// Merge `merge.absorbed` into `merge.survivor` inside the caller's write.
pub(crate) fn merge_label_on(conn: &CovenWrite<'_>, merge: &LabelMerge<'_>) -> Result<(), DbError> {
    let changed = conn.run(&FillLabelIdsParams {
        id: merge.survivor,
        musicbrainz_label_id: merge.musicbrainz_label_id,
        discogs_label_id: merge.discogs_label_id,
    })?;
    // The survivor is a label this write read, and its id names one row.
    assert_eq!(changed, 1, "a label merge updates exactly its surviving label");
    // A chain that ended at the absorbed label now ends at the survivor.
    conn.run(&MoveLabelMergesParams { survivor: merge.survivor, absorbed: merge.absorbed })?;
    conn.run(&InsertLabelMergeParams { absorbed: merge.absorbed, survivor: merge.survivor })?;
    Ok(())
}

// Each label merge row: the absorbed label and the label it was merged into,
// by absorbed id.
#[cfg(test)]
coven::read!(
    writer LabelMergeRows,
    "SELECT merge.id, merge.into_label_id FROM label_merges AS merge ORDER BY merge.id"
);

// Each merged artist and the survivor of its merges, by merged id.
#[cfg(test)]
coven::read!(
    writer MergedArtistSurvivors { artist_id: String, survivor_id: String },
    r#"
    SELECT survivor.artist_id, survivor.survivor_id
    FROM merged_artist_survivors AS survivor ORDER BY survivor.artist_id
"#
);

// Each merged label and the survivor of its merges, by merged id.
#[cfg(test)]
coven::read!(
    writer MergedLabelSurvivors { label_id: String, survivor_id: String },
    r#"
    SELECT survivor.label_id, survivor.survivor_id
    FROM merged_label_survivors AS survivor ORDER BY survivor.label_id
"#
);

// Each label and the label it shows as, by label id.
#[cfg(test)]
coven::read!(
    writer LabelsShownAs,
    r#"
    SELECT shown.label_id, shown.shown_label_id
    FROM label_shown_as AS shown ORDER BY shown.label_id
"#
);

// How many artist merge rows the library holds.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer ArtistMergeCount { merges: i64 },
    "SELECT COUNT(*) AS merges FROM artist_merges AS merge"
);

// How many artists hold the Discogs id `:discogs`.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer ArtistsHoldingDiscogsId(discogs: String) { holding: i64 },
    r#"
    SELECT COUNT(*) AS holding FROM artists AS artist
    WHERE artist.discogs_artist_id = :discogs
"#
);

#[cfg(test)]
#[path = "identity_merges_tests.rs"]
mod tests;
