//! Reading and writing a release's records — every catalog's description of
//! it.

use super::*;

impl Database {
    /// Test seam: write a release's records outside the atomic import path,
    /// replacing any it had.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release_records(
        &self,
        release_id: &str,
        records: &[crate::import::ReleaseRecord],
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let records = records.to_vec();
        let now = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now()).to_rfc3339();
        self.call_sql(move |sql| write_release_records(&sql, &release_id, &records, &now))
            .await
    }

    /// Every record for a release. Empty if no catalog describes it.
    pub async fn get_release_records(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseRecord>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| get_release_records_on(&sql, &release_id))
            .await
    }

    /// The library album `records` file a release under, leaving out the
    /// releases `excluding`. The applied record — the list's first — decides,
    /// then each other record in order: the first that names an
    /// album the library holds names it, by a copy of its pressing, else by
    /// its album (MusicBrainz release group, Discogs master). Where one record
    /// names several, its group's own album is it if the library holds it,
    /// else the oldest.
    pub async fn album_records_name(
        &self,
        records: &[crate::import::ReleaseRecord],
        excluding: &[String],
    ) -> Result<Option<String>, DbError> {
        let records = records.to_vec();
        let excluding = excluding.to_vec();
        self.read(move |sql| album_records_name_on(&sql, &records, &excluding))
            .await
    }

    /// Replace a release's record rows and move the release between albums
    /// when the target differs from the source, in one transaction (see
    /// [`set_records_on`]).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn set_records_atomic(&self, change: RecordsChange) -> Result<(), DbError> {
        let now = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now());
        self.call_sql(move |sql| set_records_on(&sql, &change, now))
            .await
    }

    /// Check, for each candidate in `checks`, whether the library already
    /// holds the same pressing or the same album (group). Drives the
    /// "in library" badges shown in the identify-pipeline result lists.
    ///
    /// Per check, every album holding a release whose record matches
    /// `(check.catalog, check.release_id)` — a copy of this pressing — or
    /// `(check.catalog, check.source_group_id)` — another release of its
    /// group — each once, those holding the pressing first, then by age.
    pub async fn check_releases_in_library(
        &self,
        checks: &[LibraryCheck],
    ) -> Result<Vec<LibraryStatus>, DbError> {
        let checks = checks.to_vec();
        self.read(move |sql| check_releases_in_library_on(&sql, &checks))
            .await
    }

    /// Follow the library membership of every release `initial` checks, in
    /// one read per change. The releases on offer change by pointing the same
    /// query at new checks through its request handle, not by opening another.
    pub(crate) fn subscribe_library_statuses(
        &self,
        initial: BTreeSet<LibraryCheck>,
    ) -> coven::ReconfigurableLiveQuery<BTreeSet<LibraryCheck>, Vec<LibraryStatus>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |checks, sql| {
                let checks: Vec<LibraryCheck> = checks.iter().cloned().collect();
                check_releases_in_library_on(&sql, &checks).map_err(CovenError::from)
            })
            .process(Ok)
    }
}

/// A release's new records and the album they file it under: what
/// [`set_records_on`] writes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) struct RecordsChange {
    pub release_id: String,
    pub records: Vec<crate::import::ReleaseRecord>,
    /// The album the release is in now.
    pub current_album_id: String,
    /// The album the release lands in.
    pub target_album_id: String,
    /// The target album's row, when the release is the first to land in it.
    pub new_album: Option<DbAlbum>,
}

/// Write `change`:
///
/// 1. Write the destination album when `new_album` is `Some` and that album
///    holds no release, with a copy of `current_album_id`'s credit list (so it
///    lands credited as the source was, not a bare row with no artist).
/// 2. Replace the release's records.
/// 3. UPDATE the release's `album_id`; a release that moves is ordered after
///    the destination album's others.
/// 4. If the release vacated `current_album_id` (the source), the source's
///    primary release is whichever release left there is ordered lowest. An
///    emptied source stays.
///
/// Metadata columns (pressing fields, album fields, tracks) are deliberately
/// untouched. Caller decides whether to reseed the metadata.
///
/// Nothing is done to the stored catalog releases: they are keyed by the
/// *catalog's* release, so re-pointing the records at a different one already
/// reads a different row. There is nothing stale to wipe, and the rows this
/// release used may be another candidate's.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn set_records_on(
    tx: &SqlContext<'_, '_>,
    change: &RecordsChange,
    now_dt: chrono::DateTime<chrono::Utc>,
) -> Result<(), DbError> {
    let now = now_dt.to_rfc3339();
    let RecordsChange {
        release_id,
        records,
        current_album_id,
        target_album_id,
        new_album,
    } = change;

    // 1. Write the destination album when it is new to this release's
    //    history: absent, or here but empty (a group's album another release
    //    vacated). Must come before the release UPDATE so the FK on
    //    `releases.album_id` points at an existing row. The album takes the
    //    source's fields and credits; one that holds releases by the time this
    //    transaction runs is left as it is.
    if let Some(album) = new_album {
        let holds_releases: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM releases WHERE album_id = ?)",
            params![album.id],
            |row| row.get(0),
        )?;
        if !holds_releases {
            upsert_album_row(tx, album)?;
            let source_artists: Vec<DbAlbumArtist> = tx.query(
                "SELECT aa.artist_id, aa.position, aa.join_phrase, ac.name_key, ac.name \
                 FROM album_artists aa \
                 LEFT JOIN artist_credits ac \
                     ON ac.artist_id = aa.artist_id AND ac.name_key = aa.credit_key \
                 WHERE aa.album_id = ? ORDER BY aa.position",
                params![current_album_id],
                |row| {
                    Ok(DbAlbumArtist {
                        credit: match row.get::<_, Option<String>>("name_key")? {
                            Some(key) => Some(crate::credits::CreditName {
                                key,
                                name: row.get("name")?,
                            }),
                            None => None,
                        },
                        join_phrase: row.get("join_phrase")?,
                        ..DbAlbumArtist::new(
                            &album.id,
                            &row.get::<_, String>("artist_id")?,
                            row.get::<_, u32>("position")?,
                            now_dt,
                        )
                    })
                },
            )?;
            replace_album_artists(tx, &album.id, &source_artists, &now)?;
        }
    }

    // 2. Replace the records and the catalog the draft was read from.
    write_release_records(tx, release_id, records, &now)?;

    // 3. Update release: album and where its draft was read. A release
    //    joining another album goes after that album's others.
    if target_album_id != current_album_id {
        tx.execute(
            "UPDATE releases SET album_order = \
                 (SELECT COALESCE(MAX(album_order) + 1, 0) FROM releases WHERE album_id = ?1) \
             WHERE id = ?2",
            params![target_album_id, release_id],
        )?;
    }
    tx.execute(
        "UPDATE releases SET album_id = ? WHERE id = ?",
        params![target_album_id, release_id],
    )?;

    // 4. Source-album cleanup. Only runs when the release actually moved;
    //    same-album updates don't vacate anything. Recheck inside the
    //    transaction (TOCTOU: a writer may have added a release to the source
    //    since the manager's pre-flight read).
    if target_album_id != current_album_id {
        vacate_album_on(tx, current_album_id)?;
    }

    Ok(())
}

/// The library album `records` file a release under, leaving out the
/// releases `excluding`. The applied record — the list's first — decides,
/// then each other record in order: the first that names an album
/// the library holds names it, by a copy of its pressing, else by its album
/// (MusicBrainz release group, Discogs master). Library albums are the
/// curator's: two albums the records name stay two, and the applied record's
/// wins over a partner's. Where one record names several, its group's own
/// album is it if the library holds it, else the oldest.
pub(crate) fn album_records_name_on(
    sql: &SqlReadContext<'_>,
    records: &[crate::import::ReleaseRecord],
    excluding: &[String],
) -> Result<Option<String>, DbError> {
    for record in records {
        let mut albums = match record.release_ref() {
            Some(release) => albums_holding_record(
                sql,
                RecordMatch::Pressing,
                release.catalog.as_str(),
                &release.key,
                excluding,
            )?,
            None => Vec::new(),
        };
        if albums.is_empty() {
            if let Some(album) = record.album_ref() {
                albums = albums_holding_record(
                    sql,
                    RecordMatch::Album,
                    album.catalog.as_str(),
                    &album.key,
                    excluding,
                )?;
            }
        }
        let own = crate::db::identity::album_id_for_records(std::slice::from_ref(record));
        if let Some(album) = own
            .filter(|own| albums.contains(own))
            .or_else(|| albums.into_iter().next())
        {
            return Ok(Some(album));
        }
    }
    Ok(None)
}

/// Which identity a record lookup matches a library record by.
#[derive(Clone, Copy)]
enum RecordMatch {
    /// A copy of the pressing: a pressing record naming the same release.
    Pressing,
    /// Another release of the album: a record naming the same album (release
    /// group, master), as an album record does by its key and a pressing
    /// record by its album key.
    Album,
}

impl RecordMatch {
    /// The predicate on `rr` with `?1` the catalog and `?2` the key: plain
    /// equality on the index that kind of record has —
    /// `idx_release_records_catalog_key` for a pressing,
    /// `idx_release_records_catalog_album`, whose expression this is, for an
    /// album.
    fn predicate(self) -> &'static str {
        match self {
            RecordMatch::Pressing => "rr.catalog = ?1 AND rr.kind = 'pressing' AND rr.key = ?2",
            RecordMatch::Album => {
                "rr.catalog = ?1 \
                 AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) = ?2"
            }
        }
    }
}

/// The lookup [`album_records_name_on`] reads each record by: every album
/// holding a release with a record matching `(catalog, key)` the way `kind`
/// says, skipping records of `exclude_release_ids`, each once, oldest first
/// (ties by id): a stated order, so every device reads one list.
fn albums_holding_record(
    sql: &SqlReadContext<'_>,
    kind: RecordMatch,
    catalog: &str,
    key: &str,
    exclude_release_ids: &[String],
) -> Result<Vec<String>, DbError> {
    let exclude_predicate = if exclude_release_ids.is_empty() {
        String::new()
    } else {
        format!(
            "AND rr.release_id NOT IN ({})",
            (0..exclude_release_ids.len())
                .map(|index| format!("?{}", index + 3))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let query = format!(
        "SELECT a.id FROM albums a
         WHERE a.id IN (
             SELECT r.album_id FROM release_records rr
             JOIN releases r ON r.id = rr.release_id
             WHERE {predicate}
               {exclude_predicate})
         ORDER BY a.created_at, a.id",
        predicate = kind.predicate()
    );
    let binds = [catalog, key]
        .into_iter()
        .chain(exclude_release_ids.iter().map(String::as_str));
    sql.query(&query, rusqlite::params_from_iter(binds), |row| {
        row.get::<_, String>(0)
    })
    .map_err(DbError::from)
}

/// One statement answers every check: the checks go in as one JSON array,
/// each matched by plain equality on the index its kind of record has — its
/// pressing on `idx_release_records_catalog_key`, its album on
/// `idx_release_records_catalog_album` — and the rows come back in check
/// order, each check's albums holding its pressing first, then by age.
pub(super) fn check_releases_in_library_on(
    sql: &(impl super::query::QueryOne + super::query::QueryRows),
    checks: &[LibraryCheck],
) -> Result<Vec<LibraryStatus>, DbError> {
    let mut statuses = checks
        .iter()
        .map(|check| LibraryStatus {
            release_id: check.release_id.clone(),
            albums: Vec::new(),
        })
        .collect::<Vec<_>>();
    if checks.is_empty() {
        return Ok(statuses);
    }
    let checks_json = serde_json::Value::Array(
        checks
            .iter()
            .map(|check| {
                serde_json::json!([
                    check.release_id,
                    check.source.as_str(),
                    check.source_group_id
                ])
            })
            .collect(),
    )
    .to_string();
    let rows = sql.query(
        r#"
            WITH checks(position, release_id, catalog, group_id) AS (
                SELECT key, value ->> 0, value ->> 1, value ->> 2 FROM json_each(?1)
            ),
            matches(position, album_id, holds_release) AS (
                SELECT checks.position, r.album_id, 1
                FROM checks
                JOIN release_records rr
                  ON rr.catalog = checks.catalog
                 AND rr.kind = 'pressing'
                 AND rr.key = checks.release_id
                JOIN releases r ON r.id = rr.release_id
                UNION ALL
                SELECT checks.position, r.album_id, 0
                FROM checks
                JOIN release_records rr
                  ON rr.catalog = checks.catalog
                 AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) = checks.group_id
                JOIN releases r ON r.id = rr.release_id
                WHERE checks.group_id IS NOT NULL
            )
            SELECT
                matches.position,
                a.id AS album_id,
                a.title AS album_title,
                MAX(matches.holds_release) AS holds_release
            FROM matches
            JOIN albums a ON a.id = matches.album_id
            GROUP BY matches.position, a.id
            ORDER BY matches.position, holds_release DESC, a.created_at, a.id
        "#,
        params![checks_json],
        |row| {
            Ok((
                row.get::<_, i64>("position")?,
                crate::db::LibraryAlbumMatch {
                    album_id: row.get("album_id")?,
                    album_title: row.get("album_title")?,
                    holds_release: row.get::<_, i64>("holds_release")? != 0,
                },
            ))
        },
    )?;
    for (position, album) in rows {
        let position = usize::try_from(position).expect("a check's position is an index");
        statuses[position].albums.push(album);
    }
    Ok(statuses)
}
