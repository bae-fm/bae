use super::*;

pub(crate) const SQL_MAX_IN_VARS: usize = 900;

/// `value` as the integer SQLite stores, or the failure naming `what` when it
/// is past SQLite's signed range.
#[cfg(desktop)]
pub(super) fn to_i64(value: u64, what: &str) -> Result<i64, DbError> {
    i64::try_from(value).map_err(|error| {
        DbError::Message(format!(
            "{what} {value} exceeds SQLite's integer range: {error}"
        ))
    })
}

pub(super) fn in_clause_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(",")
}

/// Which candidates a loader reads the rows of, by content hash: every one's,
/// one candidate's, or a window's, each by its own statement. The one
/// candidate's statement names it by key, which its index reaches and which
/// coven reruns a live query for only when that candidate's rows change.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum CandidateRows<'a> {
    All,
    One(&'a str),
    Of(&'a [&'a str]),
}

#[cfg(desktop)]
impl<'a> CandidateRows<'a> {
    /// The clause keeping these candidates' rows by `column`, and what it
    /// binds; no clause for every candidate.
    pub(crate) fn filter(self, column: &str) -> CandidateFilter {
        match self {
            Self::All => CandidateFilter {
                clause: String::new(),
                binds: CandidateBinds::None,
            },
            Self::One(key) => CandidateFilter {
                clause: format!("WHERE {column} = :candidate"),
                binds: CandidateBinds::One(key.to_string()),
            },
            Self::Of(keys) => CandidateFilter {
                clause: format!("WHERE {column} IN (SELECT value FROM json_each(:candidates))"),
                binds: CandidateBinds::Of(serde_json::Value::from(keys.to_vec()).to_string()),
            },
        }
    }
}

/// A [`CandidateRows`]' clause, and the parameter it binds.
#[cfg(desktop)]
pub(crate) struct CandidateFilter {
    pub(crate) clause: String,
    binds: CandidateBinds,
}

#[cfg(desktop)]
enum CandidateBinds {
    None,
    One(String),
    Of(String),
}

#[cfg(desktop)]
impl CandidateFilter {
    /// What the clause binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match &self.binds {
            CandidateBinds::None => Vec::new(),
            CandidateBinds::One(key) => vec![(":candidate", key as &dyn rusqlite::ToSql)],
            CandidateBinds::Of(keys) => vec![(":candidates", keys as &dyn rusqlite::ToSql)],
        }
    }
}

/// One-row SELECT shared by coven's write [`SqlContext`] and its read
/// [`SqlReadContext`], used when artist resolution runs inside either context.
pub(super) trait QueryOne {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T>;
}

/// Multi-row SELECT shared by coven's read and write SQL contexts. Kept
/// separate from [`QueryOne`] so callers require only the queries they use.
pub(super) trait QueryRows {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>>;
}

/// The resolvers' unit tests run them against a bare seeded connection, which is
/// the only place bae still holds one — coven owns every production connection.
#[cfg(test)]
impl QueryOne for Connection {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T> {
        Connection::query_row(self, sql, params, f)
    }
}

#[cfg(all(test, desktop))]
impl QueryRows for Connection {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>> {
        let mut statement = self.prepare(sql)?;
        let values = statement.query_map(params, f)?.collect();
        values
    }
}

impl QueryOne for SqlReadContext<'_> {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T> {
        SqlReadContext::query_row(self, sql, params, f)
    }
}

impl QueryRows for SqlReadContext<'_> {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>> {
        SqlReadContext::query(self, sql, params, f)
    }
}

impl QueryOne for SqlContext<'_, '_> {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T> {
        SqlContext::query_row(self, sql, params, f)
    }
}

impl QueryRows for SqlContext<'_, '_> {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>> {
        SqlContext::query(self, sql, params, f)
    }
}

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
pub(super) fn vacate_album_on(conn: &SqlContext<'_, '_>, album_id: &str) -> Result<bool, DbError> {
    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM releases WHERE album_id = ?",
        params![album_id],
        |row| row.get(0),
    )?;
    Ok(remaining == 0)
}

/// The id of the artist `artist_id` (an SQL expression) shows as: the survivor
/// of its merges, or itself (`artist_shown_as`).
pub(super) fn shown_artist_id(artist_id: &str) -> String {
    format!("(SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = {artist_id})")
}

/// Whether the `artists` row aliased `alias` shows as itself — it was not
/// merged into another artist — so listings include it.
pub(super) fn artist_is_shown(alias: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM artist_shown_as shown_as \
         WHERE shown_as.artist_id = {alias}.id AND shown_as.shown_artist_id = {alias}.id)"
    )
}

/// Whether the album aliased `a` (every album listing's alias) holds a release,
/// so listings include it. See [`vacate_album_on`].
pub(super) const ALBUM_A_IS_SHOWN: &str =
    "EXISTS (SELECT 1 FROM releases shown_release WHERE shown_release.album_id = a.id)";

/// The composer list's rows (`composer_listing`), each with its artist, as
/// [`row_to_composer_summary`] reads them; `filter` and `tail` follow the join.
pub(super) fn composer_listing_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = String::from(
        "SELECT composer.id AS artist_id,
                composer.name AS artist_name,
                composer.sort_name AS artist_sort_name,
                composer.discogs_artist_id AS artist_discogs_artist_id,
                composer.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                composer.created_at AS artist_created_at,
                listing.work_count,
                listing.linked_release_count,
                listing.unlinked_credit_count
         FROM composer_listing listing
         JOIN artists composer ON composer.id = listing.artist_id
         ",
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

/// The artist list's rows (`artist_listing`), each with its artist as
/// `ar`, as [`row_to_artist_summary`] reads them; `filter` and `tail` follow
/// the join. An artist is listed for the albums whose credit lists
/// (`album_artists`) show it; track credits (`track_artists`) don't list it.
/// Various Artists gets no special case: it is a real `artists` row and lists
/// like any other artist.
pub(super) fn artist_listing_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = String::from(
        "SELECT ar.id AS artist_id,
                ar.name AS artist_name,
                ar.sort_name AS artist_sort_name,
                ar.discogs_artist_id AS artist_discogs_artist_id,
                ar.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                ar.created_at AS artist_created_at,
                listing.album_count
         FROM artist_listing listing
         JOIN artists ar ON ar.id = listing.artist_id
         ",
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

/// What an album listing's Artist sort compares.
#[derive(Clone, Copy)]
pub(super) enum ArtistOrder {
    /// The album's whole credit list (`album_listing`'s credit keys).
    CreditList,
    /// The one artist the row is listed under, `artist` (the browse grouped
    /// by artist lists an album under each artist it credits).
    GroupArtist,
}

/// The same relation and order serve paging, section ranges, and reveal.
/// Grouped rows are album appearances; flat rows are distinct albums. Both
/// read `album_listing listing` and the album as `a`.
pub(super) fn album_browse_order(
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
) -> (String, String) {
    if !group_by_artist {
        return (
            "FROM album_listing listing JOIN albums a ON a.id = listing.album_id".to_string(),
            build_order_by(sort, ArtistOrder::CreditList),
        );
    }
    let direction = sort
        .iter()
        .find(|c| c.field == AlbumSortField::Artist)
        .map_or(SortDirection::Ascending, |c| c.direction);
    let mut grouped_sort = vec![AlbumSortCriterion {
        field: AlbumSortField::Artist,
        direction,
    }];
    grouped_sort.extend(
        sort.iter()
            .filter(|c| c.field != AlbumSortField::Artist)
            .cloned(),
    );
    if sort.is_empty() {
        grouped_sort.push(AlbumSortCriterion {
            field: AlbumSortField::DateAdded,
            direction: SortDirection::Descending,
        });
    }
    let order = build_order_by(&grouped_sort, ArtistOrder::GroupArtist);
    // CROSS JOIN keeps this join order: the artists in their index's order,
    // each artist's albums after it, so a window reads about its own rows.
    (
        "FROM artist_listing artist \
         CROSS JOIN album_artist_link link ON link.artist_id = artist.artist_id \
         CROSS JOIN album_listing listing ON listing.album_id = link.album_id \
         JOIN albums a ON a.id = listing.album_id"
            .to_string(),
        order,
    )
}

/// The grid's sections when grouped by artist: each artist in the grouped
/// order with its name, where its albums start in the rows and how many
/// there are. `direction` is the grouped order's Artist direction.
pub(super) fn album_browse_sections_query(direction: SortDirection) -> String {
    let dir = sort_direction_sql(direction);
    let order = format!("artist.sort_key COLLATE NOCASE {dir}, artist.artist_id {dir}");
    format!(
        "SELECT artist.artist_id, artist.name, \
                SUM(artist.album_count) OVER (ORDER BY {order} ROWS UNBOUNDED PRECEDING) \
                    - artist.album_count, \
                artist.album_count \
         FROM artist_listing artist \
         ORDER BY {order}"
    )
}

fn sort_direction_sql(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    }
}

pub(super) fn work_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = format!(
        "SELECT w.id AS work_id,
                w.title AS work_title,
                w.disambiguation AS work_disambiguation,
                w.work_type AS work_type,
                w.musicbrainz_work_id AS work_musicbrainz_id,
                w.created_at AS work_created_at,
                (
                    SELECT wp.parent_work_id
                    FROM work_parts wp
                    WHERE wp.child_work_id = w.id
                    ORDER BY wp.position, wp.parent_work_id
                    LIMIT 1
                ) AS parent_work_id,
                (
                    SELECT tr.release_id
                    FROM track_works tw_cover
                    JOIN tracks tr ON tr.id = tw_cover.track_id
                    WHERE tw_cover.work_id = w.id
                    ORDER BY tr.position, tr.release_id
                    LIMIT 1
                ) AS representative_release_id,
                (
                    SELECT GROUP_CONCAT(composer.name, ', ' ORDER BY credit.position)
                    FROM (
                        SELECT {work_artist} AS artist_id, MIN(wa.position) AS position
                        FROM work_artists wa
                        WHERE wa.work_id = w.id
                        GROUP BY 1
                    ) credit
                    JOIN artists composer ON composer.id = credit.artist_id
                ) AS composer_names,
                COUNT(DISTINCT tr.release_id) AS linked_release_count
         FROM works w
         LEFT JOIN track_works tw ON tw.work_id = w.id
         LEFT JOIN tracks tr ON tr.id = tw.track_id
         ",
        work_artist = shown_artist_id("wa.artist_id"),
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(
        "
         GROUP BY w.id, w.title, w.disambiguation, w.work_type, w.musicbrainz_work_id, \
                  w.created_at",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(super) fn row_to_work_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkSummary> {
    Ok(DbWorkSummary {
        work: DbWork {
            id: row.get("work_id")?,
            title: row.get("work_title")?,
            disambiguation: row.get("work_disambiguation")?,
            work_type: row.get("work_type")?,
            musicbrainz_work_id: row.get("work_musicbrainz_id")?,
            created_at: rfc3339_column(row, "work_created_at")?,
        },
        parent_work_id: row.get("parent_work_id")?,
        representative_release_id: row.get("representative_release_id")?,
        composer_names: row.get("composer_names")?,
        linked_release_count: row.get("linked_release_count")?,
    })
}

pub(super) fn row_to_composer_summary(row: &Row<'_>) -> rusqlite::Result<DbComposerSummary> {
    Ok(DbComposerSummary {
        artist: row_to_artist_with_prefix(row, "artist_")?,
        work_count: row.get("work_count")?,
        linked_release_count: row.get("linked_release_count")?,
        unlinked_credit_count: row.get("unlinked_credit_count")?,
    })
}

pub(super) fn row_to_artist_summary(row: &Row<'_>) -> rusqlite::Result<DbArtistSummary> {
    Ok(DbArtistSummary {
        artist: row_to_artist_with_prefix(row, "artist_")?,
        album_count: row.get("album_count")?,
    })
}

pub(super) fn work_summary_sort_key(work: &DbWorkSummary) -> String {
    work.work.title.to_lowercase()
}

pub(super) fn row_to_track_role_summary(row: &Row<'_>) -> rusqlite::Result<DbTrackRoleSummary> {
    Ok(DbTrackRoleSummary {
        role: DbTrackArtistRole {
            id: row.get("track_artist_role_id")?,
            track_id: row.get("track_id")?,
            artist_id: row.get("artist_id")?,
            position: row.get("position")?,
            source: parsed_column(row, "role_source")?,
            source_credit: row.get("source_credit")?,
            created_at: rfc3339_column(row, "created_at")?,
        },
        track: row_to_track_with_prefix(row, "track_")?,
        album: row_to_album_with_prefix(row, "album_")?,
        artist: row_to_artist_with_prefix(row, "artist_")?,
    })
}

pub(super) fn work_release_rows(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Vec<DbWorkReleaseSummary>, DbError> {
    sql.query(
        "SELECT DISTINCT
            r.id AS release_id,
            r.album_id,
            a.title AS album_title,
            r.release_name,
            r.year AS release_year,
            r.media AS release_media,
            (
                SELECT COUNT(*)
                FROM releases indexed_release
                WHERE indexed_release.album_id = r.album_id
                  AND (
                      indexed_release.album_order < r.album_order
                      OR (
                          indexed_release.album_order = r.album_order
                          AND indexed_release.id <= r.id
                      )
                  )
            ) AS release_index
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY a.title, r.album_order, r.id",
        params![work_id],
        row_to_work_release_summary,
    )
    .map_err(DbError::from)
}

pub(super) fn row_to_work_release_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkReleaseSummary> {
    Ok(DbWorkReleaseSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        release_name: row.get("release_name")?,
        year: row.get("release_year")?,
        media: super::pressing_columns::read_media(row, "release_media")?,
        release_index: row.get("release_index")?,
    })
}

pub(super) fn row_to_release_summary(
    sql: &SqlReadContext<'_>,
    row: &Row<'_>,
) -> rusqlite::Result<DbReleaseSummary> {
    let id: String = row.get("release_id")?;
    Ok(DbReleaseSummary {
        unstored_audio: super::read::unstored_audio_in_row(sql, &id)?,
        id,
        album_id: row.get("album_id")?,
        media: super::pressing_columns::read_media(row, "release_media")?,
        remote: row.get("remote")?,
        pinned: row.get("pinned")?,
        upload: super::queued_uploads::release_upload_in_row(row)?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}

/// ORDER BY for composer-page sorts, over `composer_listing listing`. Ties
/// break by name ascending, then id — pagination needs a total order and a
/// name is not unique — and that tie-break is the whole clause when `sort` is
/// empty. A name the sort already orders by is not repeated, so an index of
/// `composer_listing` serves each one-field sort. Mirrors `build_order_by`'s
/// `a.id` album tie-break.
pub(super) fn composer_order_by(sort: &[ComposerSortCriterion]) -> String {
    let mut terms = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ComposerSortField::Name => "listing.name",
                ComposerSortField::WorkCount => "listing.work_count",
                ComposerSortField::LinkedReleaseCount => "listing.linked_release_count",
            };
            let direction = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            format!("{field} {direction}")
        })
        .collect::<Vec<_>>();
    if !sort.iter().any(|c| c.field == ComposerSortField::Name) {
        terms.push("listing.name ASC".to_string());
    }
    terms.push("listing.artist_id ASC".to_string());
    terms.join(", ")
}

/// ORDER BY for artist-page sorts, over `artist_listing listing`: each
/// criterion, then name ascending unless the sort already orders by name,
/// then id, the pagination tie-break. An index of `artist_listing` serves
/// each one-field sort.
pub(super) fn artist_order_by(sort: &[ArtistSortCriterion]) -> String {
    let mut terms = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ArtistSortField::Name => "listing.name",
                ArtistSortField::AlbumCount => "listing.album_count",
            };
            format!("{field} {}", sort_direction_sql(c.direction))
        })
        .collect::<Vec<_>>();
    if !sort.iter().any(|c| c.field == ArtistSortField::Name) {
        terms.push("listing.name ASC".to_string());
    }
    terms.push("listing.artist_id ASC".to_string());
    terms.join(", ")
}

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
        file_size: row.get("file_size")?,
        width: row.get("width")?,
        height: row.get("height")?,
        source: row.get("source")?,
        source_url: row.get("source_url")?,
        created_at: rfc3339_column(row, "created_at")?,
        revision: row.get("revision")?,
    })
}

/// The ORDER BY clause for an album listing over `album_listing listing`;
/// `artist` says what its Artist sort compares. With no criteria the newest
/// albums come first. An index of `album_listing` serves each one-field sort.
pub(super) fn build_order_by(sort: &[AlbumSortCriterion], artist: ArtistOrder) -> String {
    // Every clause ends with the album id, a total order over albums. Without
    // it rows sharing a sort value order arbitrarily — and `get_album_page`
    // (LIMIT/OFFSET) and `get_album_index` (a ROW_NUMBER window) are different
    // query shapes, so SQLite could tie-break them differently and a reveal would
    // scroll to the wrong album.
    if sort.is_empty() {
        return "listing.created_at DESC, listing.album_id".to_string();
    }
    let clause = sort
        .iter()
        .flat_map(|c| {
            let dir = sort_direction_sql(c.direction);
            match c.field {
                AlbumSortField::Title => vec![format!("listing.title COLLATE NOCASE {dir}")],
                AlbumSortField::Artist => match artist {
                    ArtistOrder::CreditList => vec![
                        format!("listing.credit_sort_names COLLATE NOCASE {dir}"),
                        format!("listing.credit_artist_ids {dir}"),
                    ],
                    ArtistOrder::GroupArtist => vec![
                        format!("artist.sort_key COLLATE NOCASE {dir}"),
                        format!("artist.artist_id {dir}"),
                    ],
                },
                // An unknown year sorts as above every year: last ascending,
                // first descending.
                AlbumSortField::Year => vec![
                    format!("listing.year IS NULL {dir}"),
                    format!("listing.year {dir}"),
                ],
                AlbumSortField::DateAdded => vec![format!("listing.created_at {dir}")],
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{clause}, listing.album_id")
}

/// ORDER BY for the Storage Manager list over `release_listing listing`.
/// Every clause ends with the album title,
/// then `created_at` and the release id, which keep pagination deterministic
/// when two releases share a title and a timestamp (bulk imports, tests). An
/// index of `release_listing` serves each, alone and after `stored`.
pub(super) fn storage_order_by(sort: &StorageSortCriterion) -> String {
    let dir = sort_direction_sql(sort.direction);
    let tie = "listing.album_title COLLATE NOCASE, listing.created_at, listing.release_id";
    match sort.field {
        StorageSortField::AlbumTitle => format!(
            "listing.album_title COLLATE NOCASE {dir}, listing.created_at, listing.release_id"
        ),
        StorageSortField::ArtistNames => format!(
            "listing.credit_sort_names COLLATE NOCASE {dir}, listing.credit_artist_ids {dir}, {tie}"
        ),
        // A release stating no medium goes last either way.
        StorageSortField::Media => {
            format!("listing.media_empty, listing.media_key {dir}, {tie}")
        }
        StorageSortField::FileCount => format!("listing.file_count {dir}, {tie}"),
        StorageSortField::TotalSize => format!("listing.total_size {dir}, {tie}"),
        StorageSortField::Storage => format!("listing.storage_order {dir}, {tie}"),
    }
}

/// Each release with files in coven's upload queue, and where its first
/// queued file stands in it, as the listing keeps them. Covers and other
/// images in the queue are no release uploading.
pub(super) const UPLOADING_RELEASES: &str =
    "SELECT uploading.release_id, uploading.first_queued_position AS position \
     FROM release_listing uploading \
     WHERE uploading.first_queued_position IS NOT NULL";

/// How many uploaded releases this device doesn't keep wholly pinned, read
/// from the listing's index of them.
pub(super) const CLOUD_ONLY_RELEASE_COUNT: &str =
    "SELECT COUNT(*) FROM release_listing AS listing \
     WHERE listing.stored = 1 AND listing.pinned_count < listing.file_count";

/// Whether this device keeps every file of the release `listing` lists
/// pinned: its pinned count, which local triggers keep from coven's
/// `coven_pins`, has reached its file count.
pub(super) const RELEASE_LISTING_PINNED: &str =
    "(listing.file_count > 0 AND listing.pinned_count = listing.file_count)";

/// What a Storage Manager filter keeps of `release_listing listing`: the
/// WHERE clause that follows its FROM.
fn storage_filter(filter: StorageFilter) -> &'static str {
    match filter {
        StorageFilter::All => "",
        StorageFilter::Remote => "WHERE listing.stored = 1",
        StorageFilter::Local => "WHERE listing.stored = 0",
    }
}

/// One Storage Manager row per release the filter keeps: the release's half
/// and its album's, the raw aggregates the resolver maps to `ReleaseSummary` /
/// `AlbumSummary`, whether it is pinned, and where it sorts by storage.
fn storage_row_select(filter: StorageFilter) -> String {
    let album_columns = album_summary_columns();
    let where_clause = storage_filter(filter);
    format!(
        "SELECT \
            r.id AS release_id, \
            r.album_id, \
            r.media AS release_media, \
            listing.stored AS remote, \
            listing.file_count, \
            listing.total_size, \
            {RELEASE_LISTING_PINNED} AS pinned, \
            listing.storage_order, \
            {upload_columns}, \
            {album_columns} \
        FROM release_listing listing \
        JOIN releases r ON r.id = listing.release_id \
        JOIN albums a ON a.id = listing.album_id \
        LEFT JOIN ({releases_sending}) sending ON sending.release_id = listing.release_id \
        {where_clause}",
        upload_columns = super::queued_uploads::RELEASE_UPLOAD_COLUMNS,
        releases_sending = super::queued_uploads::RELEASES_SENDING,
    )
}

/// One window of the Storage Manager list under `filter`, in `sort`'s order.
pub(super) fn storage_page_query(sort: &StorageSortCriterion, filter: StorageFilter) -> String {
    format!(
        "{} ORDER BY {} LIMIT ? OFFSET ?",
        storage_row_select(filter),
        storage_order_by(sort)
    )
}

/// The rows of `selected_count` bound release ids that the list under
/// `filter` holds, by release id. SQLite folds the outer id filter into the
/// list's rows, so each selected release is looked up by its key rather than
/// the list read whole.
pub(super) fn storage_selected_query(filter: StorageFilter, selected_count: usize) -> String {
    format!(
        "SELECT * FROM ({}) WHERE release_id IN ({}) ORDER BY release_id",
        storage_row_select(filter),
        in_clause_placeholders(selected_count)
    )
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
          LEFT JOIN artist_credits {prefix}_name \
              ON {prefix}_name.artist_id = {prefix}_credit.artist_id \
             AND {prefix}_name.name_key = {prefix}_credit.credit_key \
          WHERE {prefix}_credit.{owner_column} = {owner_expr} \
          HAVING COUNT(*) > 0)",
        shown = shown_artist_id(&format!("{prefix}_credit.artist_id")),
    )
}

/// The album `a`'s credits as a JSON array, `[]` for none.
pub(super) fn album_credits_json_sql() -> String {
    format!(
        "COALESCE({}, '[]')",
        credits_json_sql("album_artists", "album_id", "a.id", "album_credits")
    )
}

/// The credits the track `track_alias` shows, as a JSON array: its own, else
/// its album `a`'s.
pub(super) fn track_credits_json_sql(track_alias: &str) -> String {
    format!(
        "COALESCE({}, {})",
        credits_json_sql(
            "track_artists",
            "track_id",
            &format!("{track_alias}.id"),
            "track_credits"
        ),
        album_credits_json_sql(),
    )
}

/// The album's releases in album order (see [`album_primary_release_sql`]).
pub(super) fn album_release_ids_json_sql() -> &'static str {
    "COALESCE(( \
        SELECT json_group_array(album_release.id ORDER BY album_release.album_order, album_release.id) \
        FROM releases album_release \
        WHERE album_release.album_id = a.id \
    ), '[]')"
}

/// The album `a`'s primary release: its lowest-ordered release, ties broken
/// by release id, so every device that holds the same rows names the same
/// one. NULL only for an emptied album, which no listing shows.
pub(super) fn album_primary_release_sql() -> &'static str {
    "(SELECT album_release.id FROM releases album_release \
      WHERE album_release.album_id = a.id \
      ORDER BY album_release.album_order, album_release.id LIMIT 1)"
}

/// Shared column list for album-summary queries. Emits `album_credits` (the
/// credit list as JSON), `release_ids_json`
/// (releases in album order) and `primary_release_id`.
pub(super) fn album_summary_columns() -> String {
    format!(
        "a.id, a.title, a.year, a.is_compilation, \
            {primary} AS primary_release_id, \
            {album_credits} AS album_credits, \
            {release_ids} AS release_ids_json",
        primary = album_primary_release_sql(),
        album_credits = album_credits_json_sql(),
        release_ids = album_release_ids_json_sql()
    )
}

/// Shared SELECT list for album-summary queries. Callers append
/// `FROM albums a` and their own `ORDER BY` / `WHERE` / `LIMIT`.
pub(super) fn album_summary_select() -> String {
    format!("SELECT {}", album_summary_columns())
}

/// Owned SQL columns; JSON decoding runs after the read connection is released.
#[derive(Clone, PartialEq)]
pub(super) struct AlbumSummaryRow {
    pub(super) id: String,
    title: String,
    year: Option<i32>,
    is_compilation: bool,
    album_credits: String,
    release_ids_json: String,
    primary_release_id: String,
}

impl AlbumSummaryRow {
    pub(super) fn read(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            title: row.get("title")?,
            year: row.get("year")?,
            is_compilation: row.get("is_compilation")?,
            album_credits: row.get("album_credits")?,
            release_ids_json: row.get("release_ids_json")?,
            primary_release_id: row.get("primary_release_id")?,
        })
    }

    pub(super) fn process(self) -> Result<DbAlbumSummary, DbError> {
        let release_ids = serde_json::from_str(&self.release_ids_json)
            .map_err(|error| DbError::Message(error.to_string()))?;
        Ok(DbAlbumSummary {
            id: self.id,
            title: self.title,
            year: self.year,
            is_compilation: self.is_compilation,
            artist_names: crate::credits::library_line(&super::read::credits_from_json(
                &self.album_credits,
            )?),
            release_ids,
            primary_release_id: self.primary_release_id,
        })
    }
}

/// The cover version of each album's primary release — the cover an album
/// shows — keyed by that release's id. Only those releases' covers are read,
/// so another release's cover changing is not a change to the albums read; an
/// emptied album has none.
pub(super) fn album_cover_versions_on(
    sql: &SqlReadContext<'_>,
    album_ids: &[String],
) -> Result<HashMap<String, String>, DbError> {
    let mut primaries = Vec::new();
    for chunk in album_ids.chunks(SQL_MAX_IN_VARS) {
        let query = format!(
            "SELECT {} FROM albums a WHERE a.id IN ({})",
            album_primary_release_sql(),
            in_clause_placeholders(chunk.len())
        );
        primaries.extend(
            sql.query(&query, rusqlite::params_from_iter(chunk.iter()), |row| {
                row.get::<_, Option<String>>(0)
            })?
            .into_iter()
            .flatten(),
        );
    }
    super::files::image_versions_on(sql, LibraryImageType::Cover, &primaries)
}
