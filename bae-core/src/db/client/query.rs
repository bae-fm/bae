use super::*;

pub(crate) const SQL_MAX_IN_VARS: usize = 900;

pub(super) fn in_clause_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(",")
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

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
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
/// of its merges, or itself.
pub(super) fn shown_artist_id(artist_id: &str) -> String {
    format!(
        "COALESCE((SELECT survivor_id FROM merged_artist_survivors \
                   WHERE artist_id = {artist_id}), {artist_id})"
    )
}

/// Whether the `artists` row aliased `alias` shows as itself — it was not
/// merged into another artist — so listings include it.
pub(super) fn artist_is_shown(alias: &str) -> String {
    format!("{alias}.id NOT IN (SELECT artist_id FROM merged_artist_survivors)")
}

/// Whether the album aliased `a` (every album listing's alias) holds a release,
/// so listings include it. See [`vacate_album_on`].
pub(super) const ALBUM_A_IS_SHOWN: &str =
    "EXISTS (SELECT 1 FROM releases shown_release WHERE shown_release.album_id = a.id)";

pub(super) fn composer_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let release_unlinked = unlinked_release_composer_role_predicate("rar");
    let track_unlinked = unlinked_track_composer_role_predicate("tar");
    let mut query = format!(
        "SELECT composer.id AS artist_id,
                composer.name AS artist_name,
                composer.sort_name AS artist_sort_name,
                composer.discogs_artist_id AS artist_discogs_artist_id,
                composer.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                composer.created_at AS artist_created_at,
                COUNT(DISTINCT wa.work_id) AS work_count,
                COUNT(DISTINCT linked.id) AS linked_release_count,
                (
                    SELECT COUNT(*) FROM release_artist_roles rar
                    WHERE {release_role_artist} = composer.id
                      AND {release_unlinked}
                ) + (
                    SELECT COUNT(*) FROM track_artist_roles tar
                    WHERE {track_role_artist} = composer.id
                      AND {track_unlinked}
                ) AS unlinked_credit_count
         FROM artists composer
         LEFT JOIN work_artists wa ON {work_artist} = composer.id
         LEFT JOIN track_works tw ON tw.work_id = wa.work_id
         LEFT JOIN tracks linked_track ON linked_track.id = tw.track_id
         LEFT JOIN releases linked ON linked.id = linked_track.release_id
         ",
        release_role_artist = shown_artist_id("rar.artist_id"),
        track_role_artist = shown_artist_id("tar.artist_id"),
        work_artist = shown_artist_id("wa.artist_id"),
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(&format!(
        "
         GROUP BY composer.id, composer.name, composer.sort_name, composer.discogs_artist_id, composer.musicbrainz_artist_id, composer.created_at
         HAVING {} AND (work_count > 0 OR unlinked_credit_count > 0)",
        artist_is_shown("composer")
    ));
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

/// The artist-browser summary query: every artist with at least one album link,
/// plus its distinct album count. Membership comes from the albums' credit
/// lists (`album_artists`) only; track-level credits (`track_artists`) don't
/// confer it. Various Artists gets no
/// special case: it is a real `artists` row and lists like any other artist.
pub(super) fn artist_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = format!(
        "SELECT ar.id AS artist_id,
                ar.name AS artist_name,
                ar.sort_name AS artist_sort_name,
                ar.discogs_artist_id AS artist_discogs_artist_id,
                ar.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                ar.created_at AS artist_created_at,
                COUNT(DISTINCT link.album_id) AS album_count
         FROM artists ar
         JOIN ({links}) link ON link.artist_id = ar.id
         ",
        links = album_artist_links_sql(),
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(
        "
         GROUP BY ar.id, ar.name, ar.sort_name, ar.discogs_artist_id, ar.musicbrainz_artist_id, ar.created_at",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

/// One membership per album and credited artist, including merged identities:
/// an artist the list names twice is one membership.
pub(super) fn album_artist_links_sql() -> String {
    format!(
        "SELECT DISTINCT {shown} AS artist_id, aa.album_id FROM album_artists aa \
         JOIN albums a ON a.id = aa.album_id WHERE {ALBUM_A_IS_SHOWN}",
        shown = shown_artist_id("aa.artist_id"),
    )
}

/// What sorts the album `a` by artist, as `{column} COLLATE NOCASE {dir}`
/// terms: every credit's sort name (else its name) in list order, joined by a
/// separator below every printable character, so "A" sorts before "A & B",
/// and "A & B" before "A & C"; then the credited artists' ids in the same
/// order, so two artists sharing a sort name keep their albums apart, as the
/// browse grouped by artist does.
pub(super) fn album_credit_sort_terms(dir: &str) -> Vec<String> {
    let key = |column: &str| {
        format!(
            "(SELECT GROUP_CONCAT({column}, char(31) ORDER BY sort_credit.position) \
              FROM album_artists sort_credit \
              JOIN artists sort_artist ON sort_artist.id = {shown} \
              WHERE sort_credit.album_id = a.id)",
            shown = shown_artist_id("sort_credit.artist_id"),
        )
    };
    vec![
        format!(
            "{} COLLATE NOCASE {dir}",
            key("COALESCE(sort_artist.sort_name, sort_artist.name)")
        ),
        format!("{} {dir}", key("sort_artist.id")),
    ]
}

/// What an album listing's Artist sort compares.
#[derive(Clone, Copy)]
pub(super) enum ArtistOrder {
    /// The album's whole credit list ([`album_credit_sort_terms`]).
    CreditList,
    /// The one artist the row is listed under, `art_sort` (the browse grouped
    /// by artist lists an album under each artist it credits).
    GroupArtist,
}

/// The same relation and order serve paging, section ranges, and reveal.
/// Grouped rows are album appearances; flat rows are distinct albums.
pub(super) fn album_browse_order(
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
) -> (String, String) {
    if !group_by_artist {
        return (
            format!("FROM albums a WHERE {ALBUM_A_IS_SHOWN}"),
            build_order_by(sort, "a.created_at DESC", ArtistOrder::CreditList),
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
    let order = build_order_by(&grouped_sort, "a.created_at DESC", ArtistOrder::GroupArtist);
    (
        format!(
            "FROM albums a JOIN ({}) link ON link.album_id = a.id \
        JOIN artists art_sort ON art_sort.id = link.artist_id",
            album_artist_links_sql()
        ),
        order,
    )
}

pub(super) fn unlinked_release_composer_role_predicate(role_alias: &str) -> String {
    unlinked_composer_role_predicate(
        role_alias,
        "release",
        "JOIN tracks t_unlinked_release ON t_unlinked_release.id = tw_unlinked_release.track_id",
        "t_unlinked_release.release_id",
        "release_id",
    )
}

pub(super) fn unlinked_track_composer_role_predicate(role_alias: &str) -> String {
    unlinked_composer_role_predicate(
        role_alias,
        "track",
        "",
        "tw_unlinked_track.track_id",
        "track_id",
    )
}

pub(super) fn unlinked_composer_role_predicate(
    role_alias: &str,
    scope: &str,
    track_join: &str,
    linked_target_column: &str,
    role_target_column: &str,
) -> String {
    format!(
        "NOT EXISTS (
            SELECT 1 FROM work_artists wa_unlinked_{scope}
            JOIN track_works tw_unlinked_{scope} ON tw_unlinked_{scope}.work_id = wa_unlinked_{scope}.work_id
            {track_join}
            WHERE {work_artist} = {role_artist}
              AND {linked_target_column} = {role_alias}.{role_target_column}
        )",
        work_artist = shown_artist_id(&format!("wa_unlinked_{scope}.artist_id")),
        role_artist = shown_artist_id(&format!("{role_alias}.artist_id")),
    )
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
        any_file_id: row.get("any_file_id")?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}

/// ORDER BY for composer-page sorts. `composer.name ASC, composer.id ASC` is
/// always appended — pagination needs a total order and `composer.name` is not
/// unique — and is the whole clause when `sort` is empty. Mirrors
/// `build_order_by`'s `a.id` album tie-break.
pub(super) fn composer_order_by(sort: &[ComposerSortCriterion]) -> String {
    if sort.is_empty() {
        return "composer.name ASC, composer.id ASC".to_string();
    }
    let clause = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ComposerSortField::Name => "composer.name",
                ComposerSortField::WorkCount => "work_count",
                ComposerSortField::LinkedReleaseCount => "linked_release_count",
            };
            let direction = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            format!("{field} {direction}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{clause}, composer.name ASC, composer.id ASC")
}

/// ORDER BY for artist-page sorts. Same shape as `composer_order_by`, with
/// `ar.name ASC, ar.id ASC` as the always-appended pagination tie-break.
pub(super) fn artist_order_by(sort: &[ArtistSortCriterion]) -> String {
    if sort.is_empty() {
        return "ar.name ASC, ar.id ASC".to_string();
    }
    let clause = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ArtistSortField::Name => "ar.name",
                ArtistSortField::AlbumCount => "album_count",
            };
            let direction = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            format!("{field} {direction}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{clause}, ar.name ASC, ar.id ASC")
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

/// The ORDER BY clause for an album listing; `artist` says what its Artist
/// sort compares.
pub(super) fn build_order_by(
    sort: &[AlbumSortCriterion],
    default: &str,
    artist: ArtistOrder,
) -> String {
    // Every clause ends with `a.id`, a total order over the album PK. Without it
    // rows sharing a sort value order arbitrarily — and `get_album_page`
    // (LIMIT/OFFSET) and `get_album_index` (a ROW_NUMBER window) are different
    // query shapes, so SQLite could tie-break them differently and a reveal would
    // scroll to the wrong album.
    if sort.is_empty() {
        return format!("{default}, a.id");
    }
    let clause = sort
        .iter()
        .flat_map(|c| {
            let dir = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            match c.field {
                AlbumSortField::Title => {
                    vec![format!("a.title COLLATE NOCASE {dir}")]
                }
                AlbumSortField::Artist => match artist {
                    ArtistOrder::CreditList => album_credit_sort_terms(dir),
                    ArtistOrder::GroupArtist => vec![
                        format!("COALESCE(art_sort.sort_name, art_sort.name) COLLATE NOCASE {dir}"),
                        format!("art_sort.id {dir}"),
                    ],
                },
                AlbumSortField::Year => {
                    let nulls_order = match c.direction {
                        SortDirection::Ascending => "ASC",
                        SortDirection::Descending => "DESC",
                    };
                    vec![
                        format!("CASE WHEN a.year IS NULL THEN 1 ELSE 0 END {nulls_order}"),
                        format!("a.year {dir}"),
                    ]
                }
                AlbumSortField::DateAdded => {
                    vec![format!("a.created_at {dir}")]
                }
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{clause}, a.id")
}

/// ORDER BY for `get_storage_page`.
pub(super) fn storage_order_by(sort: &StorageSortCriterion) -> String {
    let dir = match sort.direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
    // Every clause ends with `r.created_at, r.id`; the id keeps pagination
    // deterministic when two releases share a timestamp (bulk imports, tests).
    match sort.field {
        StorageSortField::AlbumTitle => {
            format!("a.title COLLATE NOCASE {dir}, r.created_at, r.id")
        }
        StorageSortField::ArtistNames => format!(
            "{}, a.title COLLATE NOCASE, r.created_at, r.id",
            album_credit_sort_terms(dir).join(", ")
        ),
        StorageSortField::Media => {
            format!(
                // Every stated medium in order, each with its count: the media
                // the column shows, compared entry by entry.
                "CASE WHEN json_array_length(r.media) = 0 THEN 1 ELSE 0 END, \
                 (SELECT group_concat(printf('%s:%05d', json_extract(entry.value, '$.medium'), \
                     json_extract(entry.value, '$.count')), ',') \
                  FROM json_each(r.media) AS entry) {dir}, \
                 a.title COLLATE NOCASE, r.created_at, r.id"
            )
        }
        StorageSortField::FileCount => {
            format!("file_count {dir}, a.title COLLATE NOCASE, r.created_at, r.id")
        }
        StorageSortField::TotalSize => {
            format!("total_size {dir}, a.title COLLATE NOCASE, r.created_at, r.id")
        }
    }
}

/// Every file of a release is in storage: each row names its file, which a
/// row does only once the file's upload completed. A release this device is
/// still uploading isn't; coven's upload queue reports it.
pub(super) const RELEASE_R_STORED: &str =
    "EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id) AND \
     NOT EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id AND f.hash IS NULL)";

/// One Storage Manager row per release: releases × albums, both halves the
/// raw aggregates the resolver maps to `ReleaseSummary` / `AlbumSummary`.
/// `joins` and `where_clause` follow the FROM.
fn storage_row_select(joins: &str, where_clause: &str) -> String {
    let album_columns = album_summary_columns();
    format!(
        "SELECT \
            r.id AS release_id, \
            r.album_id, \
            r.media AS release_media, \
            ({RELEASE_R_STORED}) AS remote, \
            (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
            COALESCE(( \
                SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS file_count, \
            COALESCE(( \
                SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS total_size, \
            {album_columns} \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        {joins} \
        {where_clause}"
    )
}

/// Paginated storage-page query: the rows `where_clause` keeps, in
/// `order_by` order, one window at a time.
pub(super) fn storage_page_query(
    order_by: &str,
    where_clause: &str,
    uploading_count: usize,
) -> String {
    let (queue_cte, queue_join) = if uploading_count == 0 {
        (String::new(), String::new())
    } else {
        let values = (0..uploading_count)
            .map(|position| format!("(?, {position})"))
            .collect::<Vec<_>>()
            .join(", ");
        (
            format!("WITH upload_queue(release_id, position) AS (VALUES {values}) "),
            "JOIN upload_queue ON upload_queue.release_id = r.id".to_string(),
        )
    };
    format!(
        "{queue_cte}{} ORDER BY {order_by} LIMIT ? OFFSET ?",
        storage_row_select(&queue_join, where_clause)
    )
}

/// The rows of `selected_count` bound release ids that the list
/// `list_where` filters to holds, by release id. `list_where` binds first,
/// as for the list's count. SQLite folds the outer id filter into the list's
/// rows, so each selected release is looked up by its key rather than the
/// list read whole.
pub(super) fn storage_selected_query(list_where: &str, selected_count: usize) -> String {
    format!(
        "SELECT * FROM ({}) WHERE release_id IN ({}) ORDER BY release_id",
        storage_row_select("", list_where),
        in_clause_placeholders(selected_count)
    )
}

/// Build a WHERE clause fragment for storage-page filtering. Returns the
/// empty string for `StorageFilter::All`.
/// The WHERE clause for a storage-page filter.
///
/// `Uploading` is the odd one: which releases are mid-upload is coven's queue
/// state, not a bae column, so the caller asks coven for those release ids first
/// and this renders a bound `IN` list of `uploading_count` placeholders. An empty
/// set is `WHERE 0` — nothing is uploading, so the page is empty — rather than an
/// `IN ()`, which SQLite rejects.
pub(super) fn storage_filter_where(filter: StorageFilter, uploading_count: usize) -> String {
    match filter {
        StorageFilter::All => String::new(),
        StorageFilter::Remote => format!("WHERE ({RELEASE_R_STORED})"),
        StorageFilter::Local => format!("WHERE NOT ({RELEASE_R_STORED})"),
        StorageFilter::Uploading if uploading_count == 0 => "WHERE 0".to_string(),
        StorageFilter::Uploading => {
            format!(
                "WHERE r.id IN ({})",
                in_clause_placeholders(uploading_count)
            )
        }
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
