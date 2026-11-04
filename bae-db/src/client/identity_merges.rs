//! One library artist or label merged into another, as a person's answer to
//! an import's identity conflict records it, or as a write does on finding
//! one catalog id on several: the survivor takes the absorbed one's catalog
//! ids, and the absorbed one stays, recorded as merged into the
//! survivor (`artist_merges`, `label_merges`). Neither row is deleted and no
//! credit or release label is rewritten: both are shared with devices that
//! may be naming the absorbed one while apart, and every read shows the
//! survivor.

use super::*;

/// An artist merged into another, with the catalog ids the survivor takes
/// where it holds none of that catalog.
pub struct ArtistMerge<'a> {
    pub survivor: &'a str,
    pub absorbed: &'a str,
    pub discogs_artist_id: Option<&'a str>,
    pub musicbrainz_artist_id: Option<&'a str>,
    /// The sort name the survivor takes where it has none.
    pub sort_name: Option<&'a str>,
    /// When the merge is recorded, as RFC 3339.
    pub at: &'a str,
}

/// Merge `merge.absorbed` into `merge.survivor` inside the caller's write.
/// A survivor without an image of its own takes the absorbed artist's, its
/// row naming the same coven file, so nothing is uploaded again; the
/// absorbed artist's image row goes.
pub fn merge_artist_on(conn: &CovenWrite<'_>, merge: &ArtistMerge<'_>) -> Result<(), DbError> {
    let image =
        |artist_id: &str| super::files::image_file_on(conn, &LibraryImageType::Artist, artist_id);
    let (surviving_image, absorbed_image) = (image(merge.survivor)?, image(merge.absorbed)?);
    if surviving_image.is_none() && absorbed_image.is_some() {
        conn.execute(
            "INSERT INTO artist_images (id, file, content_type) \
             SELECT ?, file, content_type FROM artist_images WHERE id = ?",
            params![merge.survivor, merge.absorbed],
        )?;
    }
    if absorbed_image.is_some() {
        conn.execute("DELETE FROM artist_images WHERE id = ?", [merge.absorbed])?;
    }
    let changed = super::write::update_artist_external_ids_row(
        conn,
        merge.survivor,
        merge.discogs_artist_id,
        merge.musicbrainz_artist_id,
        merge.sort_name,
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "artist merge updated {changed} surviving artists; expected exactly one"
        )));
    }
    // A chain that ended at the absorbed artist now ends at the survivor.
    conn.execute(
        "UPDATE artist_merges SET into_artist_id = ?1 WHERE into_artist_id = ?2",
        params![merge.survivor, merge.absorbed],
    )?;
    conn.execute(
        "INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (id) DO UPDATE SET into_artist_id = excluded.into_artist_id",
        params![merge.absorbed, merge.survivor, merge.at],
    )?;
    Ok(())
}

/// A label merged into another, with the catalog ids the survivor takes
/// where it holds none of that catalog.
pub struct LabelMerge<'a> {
    pub survivor: &'a str,
    pub absorbed: &'a str,
    pub discogs_label_id: Option<&'a str>,
    pub musicbrainz_label_id: Option<&'a str>,
    /// When the merge is recorded, as RFC 3339.
    pub at: &'a str,
}

/// Merge `merge.absorbed` into `merge.survivor` inside the caller's write.
pub fn merge_label_on(conn: &CovenWrite<'_>, merge: &LabelMerge<'_>) -> Result<(), DbError> {
    let changed = conn.execute(
        "UPDATE labels SET \
             musicbrainz_label_id = COALESCE(musicbrainz_label_id, ?2), \
             discogs_label_id = COALESCE(discogs_label_id, ?3) \
         WHERE id = ?1",
        params![merge.survivor, merge.musicbrainz_label_id, merge.discogs_label_id],
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "label merge updated {changed} surviving labels; expected exactly one"
        )));
    }
    // A chain that ended at the absorbed label now ends at the survivor.
    conn.execute(
        "UPDATE label_merges SET into_label_id = ?1 WHERE into_label_id = ?2",
        params![merge.survivor, merge.absorbed],
    )?;
    conn.execute(
        "INSERT INTO label_merges (id, into_label_id, created_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (id) DO UPDATE SET into_label_id = excluded.into_label_id",
        params![merge.absorbed, merge.survivor, merge.at],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "identity_merges_tests.rs"]
mod tests;
