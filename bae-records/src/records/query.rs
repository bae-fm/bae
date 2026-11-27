use super::*;

/// The table a host-provided image file's row lives in. The image type IS the
/// table (`covers` / `artist_images`), so there is no `type` column. A fixed
/// match over the enum, so the interpolated name is always a trusted literal.
pub(super) fn image_table(image_type: &LibraryImageType) -> &'static str {
    match image_type {
        LibraryImageType::Cover => "covers",
        LibraryImageType::Artist => "artist_images",
    }
}

/// The id of the artist `artist_id` (an SQL expression) shows as: the survivor
/// of its merges, or itself (`artist_shown_as`).
pub fn shown_artist_id(artist_id: &str) -> String {
    format!("(SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = {artist_id})")
}

/// The column of an image's table that names the file the image is read
/// from: a cover's browse copy, or the artist image's own file.
pub(super) fn image_file_column(image_type: &LibraryImageType) -> &'static str {
    match image_type {
        LibraryImageType::Cover => "browse_file",
        LibraryImageType::Artist => "file",
    }
}

/// The credits of `owner_expr` in `table` (`album_artists` or
/// `track_artists`), in list order, as a JSON array (see
/// [`super::read::credits_from_json`]): each the artist it shows after merges,
/// the other printed name it names, and its join. `prefix` keeps the
/// subquery's aliases apart from the query it sits in. An owner with no
/// credits gives NULL.
fn credits_json_sql(table: &str, owner_column: &str, owner_expr: &str, prefix: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'id', {prefix}_artist.id, 'name', {prefix}_artist.name, \
             'sort_name', {prefix}_artist.sort_name, \
             'discogs_artist_id', {prefix}_artist.discogs_artist_id, \
             'musicbrainz_artist_id', {prefix}_artist.musicbrainz_artist_id, \
             'created_at', {prefix}_artist.created_at, \
             'credit_key', {prefix}_name.name_key, 'credit_name', {prefix}_name.name, \
             'join', {prefix}_credit.join_phrase) ORDER BY {prefix}_credit.position) \
          FROM {table} {prefix}_credit \
          JOIN artists {prefix}_artist ON {prefix}_artist.id = {shown} \
          {credit_name} \
          WHERE {prefix}_credit.{owner_column} = {owner_expr} \
          HAVING COUNT(*) > 0)",
        shown = shown_artist_id(&format!("{prefix}_credit.artist_id")),
        credit_name =
            super::read::credit_name_join(&format!("{prefix}_credit"), &format!("{prefix}_name")),
    )
}

/// The album `a`'s credits as a JSON array, `[]` for none.
pub fn album_credits_json_sql() -> String {
    format!(
        "COALESCE({}, '[]')",
        credits_json_sql("album_artists", "album_id", "a.id", "album_credits")
    )
}

/// The credits the track `track_alias` shows, as a JSON array: its own, else
/// its album `a`'s. The one place that rule is written: every read of a
/// track's shown credits goes through it.
pub fn track_credits_json_sql(track_alias: &str) -> String {
    format!("COALESCE({}, {})", track_own_credits_sql(track_alias), album_credits_json_sql())
}

/// The track `track_alias`'s own credits as a JSON array, NULL for none.
fn track_own_credits_sql(track_alias: &str) -> String {
    credits_json_sql("track_artists", "track_id", &format!("{track_alias}.id"), "track_credits")
}

/// The album `a`'s primary release: its lowest-ordered release, ties broken
/// by release id, so every device that holds the same rows names the same
/// one.
pub fn album_primary_release_sql() -> &'static str {
    "(SELECT album_release.id FROM releases album_release \
      WHERE album_release.album_id = a.id \
      ORDER BY album_release.album_order, album_release.id LIMIT 1)"
}

/// Owned SQL columns; JSON decoding runs after the read connection is released.
#[derive(Clone, PartialEq)]
pub struct AlbumSummaryRow {
    pub(crate) id: String,
    pub(super) title: String,
    pub(super) year: Option<i32>,
    pub(super) is_compilation: bool,
    pub(super) album_credits: String,
    pub(super) release_ids_json: String,
    pub(super) primary_release_id: String,
    pub(super) primary_cover_version: Option<String>,
}

impl AlbumSummaryRow {
    pub fn process(self) -> Result<DbAlbumSummary, DbError> {
        let credits = self.credits()?;
        self.process_with(&credits)
    }

    /// The album's credits, in list order.
    pub fn credits(&self) -> Result<Vec<crate::credits::LibraryCredit>, DbError> {
        super::read::credits_from_json(&self.album_credits)
    }

    /// The summary, its artist line printed from `credits`, the album's own.
    pub fn process_with(
        self,
        credits: &[crate::credits::LibraryCredit],
    ) -> Result<DbAlbumSummary, DbError> {
        let release_ids = serde_json::from_str(&self.release_ids_json).map_err(|error| {
            DbError::unreadable_json("release_ids_json", &self.release_ids_json, error)
        })?;
        Ok(DbAlbumSummary {
            id: self.id,
            title: self.title,
            year: self.year,
            is_compilation: self.is_compilation,
            artist_line: crate::credits::library_line(credits),
            release_ids,
            primary_release_id: self.primary_release_id,
            cover_version: self.primary_cover_version,
        })
    }
}
