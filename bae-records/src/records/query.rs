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

/// After a release has left `album_id` inside the current transaction: whether
/// the album is now empty. Its primary release is whichever of the releases
/// left is ordered lowest, so nothing else follows the one that left.
///
/// An emptied album is never deleted. Another device may have added a release
/// to it while apart, and a delete wins over that release in the merge and
/// leaves it without a parent, so the pull that carries it could never apply.
/// An empty album is inert instead: every listing shows only albums with a
/// release.
/// A release that later lands in it, from any device, fills it again.
///
/// Precondition: the release row must already be deleted or repointed away from
/// `album_id`, since this counts what remains.
pub(super) fn vacate_album_on(conn: &CovenWrite<'_>, album_id: &str) -> Result<bool, DbError> {
    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM releases WHERE album_id = ?",
        params![album_id],
        |row| row.get(0),
    )?;
    Ok(remaining == 0)
}

/// The id of the artist `artist_id` (an SQL expression) shows as: the survivor
/// of its merges, or itself (`artist_shown_as`).
pub fn shown_artist_id(artist_id: &str) -> String {
    format!("(SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = {artist_id})")
}

/// The names of the artists credited on the work `work_id` (an SQL
/// expression) in a writing role, each once as the artist it shows after merges, joined by ", "
/// in credit order; NULL when the work has no artists.
pub fn work_composer_names_sql(work_id: &str) -> String {
    format!(
        "(
                    SELECT GROUP_CONCAT(composer.name, ', ' ORDER BY credit.position)
                    FROM (
                        SELECT {work_artist} AS artist_id, MIN(wa.position) AS position
                        FROM work_artists wa
                        WHERE wa.work_id = {work_id}
                          AND wa.role IN (SELECT role FROM writing_credit_roles)
                        GROUP BY 1
                    ) credit
                    JOIN artists composer ON composer.id = credit.artist_id
                )",
        work_artist = shown_artist_id("wa.artist_id"),
    )
}

/// Whether the `artists` row aliased `alias` shows as itself — it was not
/// merged into another artist — so listings include it.
pub fn artist_is_shown(alias: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM artist_shown_as shown_as \
         WHERE shown_as.artist_id = {alias}.id AND shown_as.shown_artist_id = {alias}.id)"
    )
}

/// Whether the album aliased `a` (every album listing's alias) holds a release,
/// so listings include it. See `vacate_album_on`.
pub const ALBUM_A_IS_SHOWN: &str =
    "EXISTS (SELECT 1 FROM releases shown_release WHERE shown_release.album_id = a.id)";

/// Build a `DbLibraryImage` from a `covers`/`artist_images` row. The table is the
/// type, so `image_type` is supplied by the caller rather than read from a column.
pub(super) fn row_to_library_image(
    row: &Row,
    image_type: LibraryImageType,
) -> rusqlite::Result<DbLibraryImage> {
    Ok(DbLibraryImage {
        id: row.get("id")?,
        image_type,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
    })
}

/// Each release with files in coven's upload queue, and where its first
/// queued file stands in it, as each release's storage row keeps them.
/// Covers and other images in the queue are no release uploading.
pub const UPLOADING_RELEASES: &str =
    "SELECT uploading.release_id, uploading.first_queued_position AS position \
     FROM release_storage uploading \
     WHERE uploading.first_queued_position IS NOT NULL";

/// Whether this device keeps every file of the release whose storage row is
/// `release_storage storage` pinned and whole here: its pinned count, which
/// local triggers keep from the pins coven's `coven_pins` lists as kept, has
/// reached its file count.
pub const RELEASE_STORAGE_PINNED: &str =
    "(storage.file_count > 0 AND storage.pinned_count = storage.file_count)";

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

/// The track `track_alias`'s own credits as a JSON array, `[]` for none: a
/// track with none takes its album's ([`track_credits_json_sql`]).
pub fn track_own_credits_json_sql(track_alias: &str) -> String {
    format!("COALESCE({}, '[]')", track_own_credits_sql(track_alias))
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

/// The album's releases in album order (see [`album_primary_release_sql`]).
pub(crate) fn album_release_ids_json_sql() -> &'static str {
    "COALESCE(( \
        SELECT json_group_array(album_release.id ORDER BY album_release.album_order, album_release.id) \
        FROM releases album_release \
        WHERE album_release.album_id = a.id \
    ), '[]')"
}

/// The album `a`'s primary release: its lowest-ordered release, ties broken
/// by release id, so every device that holds the same rows names the same
/// one. NULL only for an emptied album, which no listing shows.
pub fn album_primary_release_sql() -> &'static str {
    "(SELECT album_release.id FROM releases album_release \
      WHERE album_release.album_id = a.id \
      ORDER BY album_release.album_order, album_release.id LIMIT 1)"
}

/// Shared column list for album-summary queries. Emits `album_credits` (the
/// credit list as JSON), `release_ids_json`
/// (releases in album order), `primary_release_id` and the version of that
/// release's cover, `primary_cover_version`.
pub fn album_summary_columns() -> String {
    format!(
        "a.id, a.title, a.year, a.is_compilation, \
            {primary} AS primary_release_id, \
            {cover} AS primary_cover_version, \
            {album_credits} AS album_credits, \
            {release_ids} AS release_ids_json",
        primary = album_primary_release_sql(),
        cover = super::files::cover_version_sql(album_primary_release_sql()),
        album_credits = album_credits_json_sql(),
        release_ids = album_release_ids_json_sql()
    )
}

/// Shared SELECT list for album-summary queries. Callers append
/// `FROM albums a` and their own `ORDER BY` / `WHERE` / `LIMIT`.
pub fn album_summary_select() -> String {
    format!("SELECT {}", album_summary_columns())
}

/// Owned SQL columns; JSON decoding runs after the read connection is released.
#[derive(Clone, PartialEq)]
pub struct AlbumSummaryRow {
    pub(crate) id: String,
    title: String,
    year: Option<i32>,
    is_compilation: bool,
    album_credits: String,
    release_ids_json: String,
    primary_release_id: String,
    primary_cover_version: Option<String>,
}

impl AlbumSummaryRow {
    pub fn read(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            title: row.get("title")?,
            year: row.get("year")?,
            is_compilation: row.get("is_compilation")?,
            album_credits: row.get("album_credits")?,
            release_ids_json: row.get("release_ids_json")?,
            primary_release_id: row.get("primary_release_id")?,
            primary_cover_version: row.get("primary_cover_version")?,
        })
    }

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
