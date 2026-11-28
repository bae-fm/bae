//! Writing a catalog release's rows: its `source_release` row, and in place
//! of every row under it, the rows of the release being written.

use super::check_derived_facts;
use super::tracklist::{insert_entry, insert_release_role, Numbering};
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::records::ReleaseRecord;
use bae_catalogs::source_release::{CatalogFacts, SourceRelease, UnfetchedReason};
use bae_store::WriteContext;
use bae_store::{DbError, WriteStatements};
use bae_types::pressing::{Medium, StatedFormat, StatedMedia};

/// The `album_links` column a release's facts are stored with: a MusicBrainz
/// release's reading, none for a Discogs release. Fetching a MusicBrainz
/// release reads its album links, or fails to, so a stored one never holds
/// links nothing asked for.
fn album_links_column(facts: &CatalogFacts) -> Option<&'static str> {
    match facts {
        CatalogFacts::MusicBrainz { album_links: AlbumLinks::NotAsked, .. } => {
            panic!("a fetched MusicBrainz release carries album links that were never read")
        }
        CatalogFacts::MusicBrainz { album_links, .. } => {
            Some(crate::tables::album_link_rows::album_links_column(album_links))
        }
        CatalogFacts::Discogs { .. } => None,
    }
}

/// Write `release`, replacing whatever was stored for it.
pub(super) fn replace_source_release_on(
    sql: &WriteContext<'_>,
    release: &SourceRelease,
) -> Result<(), DbError> {
    check_derived_facts(release);
    let catalog = release.release.catalog.as_str();
    let key = release.release.key.as_str();
    let metadata = &release.metadata;
    let pressing = &metadata.pressing;
    let facts = bae_records::records::pressing_columns::FactColumns::of(&pressing.facts);
    sql.run(&UpsertSourceReleaseParams {
        catalog,
        key,
        source_group_id: release.source_group_id.as_deref(),
        album_title: &metadata.album.title,
        album_first_year: metadata.album.first_year,
        year: pressing.typed.year,
        country: facts.country,
        region: facts.region,
        status: facts.status,
        packaging: facts.packaging,
        discogs_details: &facts.discogs_details,
        notes: &serde_json::to_string(&release.notes).expect("notes serialize"),
        archive_release_id: release
            .archive_release
            .as_ref()
            .map(|archive| archive.release_id.as_str()),
        archive_group_id: release
            .archive_release
            .as_ref()
            .and_then(|archive| archive.group_id.as_deref()),
        album_links: album_links_column(&release.catalog),
    })?;
    delete_child_rows(sql, catalog, key)?;
    for (position, label) in pressing.typed.labels.iter().enumerate() {
        sql.run(&InsertSourceReleaseLabelParams {
            catalog,
            key,
            position: position as i64,
            name: label.name(),
            catalog_number: label.catalog_number(),
            musicbrainz_label_id: label.musicbrainz_label_id(),
            discogs_label_id: label.discogs_label_id(),
        })?;
    }
    for (position, barcode) in pressing.typed.barcodes.iter().enumerate() {
        sql.run(&InsertSourceReleaseBarcodeParams {
            catalog,
            key,
            position: position as i64,
            barcode,
        })?;
    }
    for (position, stated) in metadata.album.artists.iter().enumerate() {
        let artist = &stated.credit;
        sql.run(&InsertSourceReleaseAlbumArtistParams {
            catalog,
            key,
            position: position as i64,
            name: &artist.name,
            sort_name: artist.sort_name.as_deref(),
            musicbrainz_artist_id: artist.musicbrainz_artist_id.as_deref(),
            discogs_artist_id: artist.discogs_artist_id.as_deref(),
            credited_as: artist.credited_as.as_deref(),
            join_phrase: stated.join.as_deref(),
        })?;
    }
    match &release.catalog {
        CatalogFacts::MusicBrainz { links, album_links } => {
            for (position, link) in links.iter().enumerate() {
                sql.run(&InsertSourceReleaseLinkParams {
                    catalog,
                    key,
                    position: position as i64,
                    link_catalog: link.catalog.as_str(),
                    link_key: &link.key,
                })?;
            }
            for (ordinal, album) in album_links.read().iter().enumerate() {
                sql.run(&InsertSourceReleaseAlbumLinkParams {
                    catalog,
                    key,
                    ordinal: ordinal as i64,
                    album_catalog: album.catalog.as_str(),
                    album_key: &album.key,
                })?;
            }
        }
        CatalogFacts::Discogs { media } => {
            let formats: &[StatedFormat] = match media {
                StatedMedia::Formats(formats) => formats,
                StatedMedia::Undescribed => &[],
                StatedMedia::PerMedium(_) => {
                    unreachable!("a Discogs release states its media as format entries")
                }
            };
            for (position, format) in formats.iter().enumerate() {
                sql.run(&InsertSourceReleaseFormatParams {
                    catalog,
                    key,
                    position: position as i64,
                    medium: format.medium.map(Medium::key),
                    quantity: format.quantity,
                })?;
            }
        }
    }
    for role in &release.release_roles {
        insert_release_role(sql, catalog, key, role)?;
    }
    for record in &release.other_records {
        let (kind, album_key) = match record {
            ReleaseRecord::Pressing { album_key, .. } => ("pressing", album_key.as_deref()),
            ReleaseRecord::Album { .. } => ("album", None),
        };
        sql.run(&InsertSourceReleaseRecordParams {
            catalog,
            key,
            record_catalog: record.catalog().as_str(),
            kind,
            record_key: record.key(),
            album_key,
        })?;
    }
    for (scope, covers) in [("release", &release.covers.release), ("album", &release.covers.album)]
    {
        for (position, cover) in covers.iter().enumerate() {
            sql.run(&InsertSourceReleaseCoverParams {
                catalog,
                key,
                scope,
                position: position as i64,
                url: &cover.image.url,
                label: &crate::tables::cover_labels::label_json(&cover.label)?,
                source: cover.source.as_str(),
                standing: cover.standing.as_str(),
            })?;
            for copy in &cover.image.downscaled {
                sql.run(&InsertSourceReleaseCoverCopyParams {
                    catalog,
                    key,
                    scope,
                    position: position as i64,
                    max_edge: copy.max_edge,
                    url: &copy.url,
                })?;
            }
        }
    }
    for (position, group) in release.archive_groups.iter().enumerate() {
        sql.run(&InsertSourceReleaseArchiveGroupParams {
            catalog,
            key,
            position: position as i64,
            group_id: group,
        })?;
    }
    for unfetched in &release.unfetched {
        sql.run(&InsertSourceReleaseUnfetchedParams {
            catalog,
            key,
            document: unfetched.document.as_str(),
            document_key: &unfetched.key,
            reason: match unfetched.reason {
                UnfetchedReason::Failed => "failed",
                UnfetchedReason::NotAsked => "not_asked",
                UnfetchedReason::StoppedShort => "stopped_short",
            },
        })?;
    }
    let mut numbering = Numbering::default();
    for (position, medium) in release.mediums.iter().enumerate() {
        sql.run(&InsertSourceReleaseMediumParams {
            catalog,
            key,
            position: position as i64,
            medium: medium.medium.map(Medium::key),
        })?;
        for (disc, disc_id) in medium.disc_ids.iter().enumerate() {
            sql.run(&InsertSourceReleaseMediumDiscParams {
                catalog,
                key,
                position: position as i64,
                disc: disc as i64,
                disc_id,
            })?;
        }
        for entry in &medium.entries {
            insert_entry(sql, catalog, key, position as i64, None, entry, &mut numbering)?;
        }
    }
    Ok(())
}

/// Delete the rows under the release, children before the tables they hang
/// off. Deleting a release's mediums takes its entries, and with them their
/// credits, roles and works.
fn delete_child_rows(sql: &WriteContext<'_>, catalog: &str, key: &str) -> Result<(), DbError> {
    sql.run(&DeleteSourceReleaseLabelRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseBarcodeRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseAlbumArtistRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseLinkRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseAlbumLinkRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseFormatRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseRecordRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseCoverCopyRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseCoverRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseArchiveGroupRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseRoleRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseMediumRowsParams { catalog, key })?;
    sql.run(&DeleteSourceReleaseUnfetchedRowsParams { catalog, key })?;
    Ok(())
}

coven::write!(
    UpsertSourceRelease(
        catalog: &'p str,
        key: &'p str,
        source_group_id: Option<&'p str>,
        album_title: &'p str,
        album_first_year: Option<i32>,
        year: Option<i32>,
        country: Option<&'static str>,
        region: Option<&'static str>,
        status: Option<&'static str>,
        packaging: Option<&'static str>,
        discogs_details: &'p str,
        notes: &'p str,
        archive_release_id: Option<&'p str>,
        archive_group_id: Option<&'p str>,
        album_links: Option<&'static str>,
    ),
    r#"
    INSERT INTO source_release (
        catalog, release_id, source_group_id, album_title, album_first_year, year, country,
        region, status, packaging, discogs_details, notes, archive_release_id, archive_group_id,
        album_links)
    VALUES (
        :catalog, :key, :source_group_id, :album_title, :album_first_year, :year, :country,
        :region, :status, :packaging, :discogs_details, :notes, :archive_release_id,
        :archive_group_id, :album_links)
    ON CONFLICT (catalog, release_id) DO UPDATE SET
        source_group_id = excluded.source_group_id,
        album_title = excluded.album_title,
        album_first_year = excluded.album_first_year,
        year = excluded.year,
        country = excluded.country, region = excluded.region,
        status = excluded.status, packaging = excluded.packaging,
        discogs_details = excluded.discogs_details, notes = excluded.notes,
        archive_release_id = excluded.archive_release_id,
        archive_group_id = excluded.archive_group_id,
        album_links = excluded.album_links
"#
);

coven::write!(
    DeleteSourceReleaseLabelRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_label WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseBarcodeRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_barcode WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseAlbumArtistRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_album_artist WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseLinkRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_link WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseAlbumLinkRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_album_link WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseFormatRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_format WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseRecordRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_record WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseCoverCopyRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_cover_copy WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseCoverRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_cover WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseArchiveGroupRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_archive_group WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseRoleRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_role WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseMediumRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_medium WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    DeleteSourceReleaseUnfetchedRows(catalog: &'p str, key: &'p str),
    r#"DELETE FROM source_release_unfetched WHERE catalog = :catalog AND release_id = :key"#
);

coven::write!(
    InsertSourceReleaseLabel(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        name: Option<&'p str>,
        catalog_number: Option<&'p str>,
        musicbrainz_label_id: Option<&'p str>,
        discogs_label_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO source_release_label (
        catalog, release_id, position, name, catalog_number, musicbrainz_label_id,
        discogs_label_id)
    VALUES (
        :catalog, :key, :position, :name, :catalog_number, :musicbrainz_label_id,
        :discogs_label_id)
"#
);

coven::write!(
    InsertSourceReleaseBarcode(catalog: &'p str, key: &'p str, position: i64, barcode: &'p str),
    r#"
    INSERT INTO source_release_barcode (
        catalog, release_id, position, barcode)
    VALUES (
        :catalog, :key, :position, :barcode)
"#
);

coven::write!(
    InsertSourceReleaseAlbumArtist(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        name: &'p str,
        sort_name: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        credited_as: Option<&'p str>,
        join_phrase: Option<&'p str>,
    ),
    r#"
    INSERT INTO source_release_album_artist (
        catalog, release_id, position, name, sort_name, musicbrainz_artist_id, discogs_artist_id,
        credited_as, join_phrase)
    VALUES (
        :catalog, :key, :position, :name, :sort_name, :musicbrainz_artist_id, :discogs_artist_id,
        :credited_as, :join_phrase)
"#
);

coven::write!(
    InsertSourceReleaseLink(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        link_catalog: &'p str,
        link_key: &'p str,
    ),
    r#"
    INSERT INTO source_release_link (
        catalog, release_id, position, link_catalog, link_key)
    VALUES (
        :catalog, :key, :position, :link_catalog, :link_key)
"#
);

coven::write!(
    InsertSourceReleaseAlbumLink(
        catalog: &'p str,
        key: &'p str,
        ordinal: i64,
        album_catalog: &'p str,
        album_key: &'p str,
    ),
    r#"
    INSERT INTO source_release_album_link (
        catalog, release_id, ordinal, album_catalog, album_key)
    VALUES (
        :catalog, :key, :ordinal, :album_catalog, :album_key)
"#
);

coven::write!(
    InsertSourceReleaseFormat(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        medium: Option<&'static str>,
        quantity: u32,
    ),
    r#"
    INSERT INTO source_release_format (
        catalog, release_id, position, medium, quantity)
    VALUES (
        :catalog, :key, :position, :medium, :quantity)
"#
);

coven::write!(
    InsertSourceReleaseRecord(
        catalog: &'p str,
        key: &'p str,
        record_catalog: &'p str,
        kind: &'static str,
        record_key: &'p str,
        album_key: Option<&'p str>,
    ),
    r#"
    INSERT INTO source_release_record (
        catalog, release_id, record_catalog, kind, key, album_key)
    VALUES (
        :catalog, :key, :record_catalog, :kind, :record_key, :album_key)
"#
);

coven::write!(
    InsertSourceReleaseCover(
        catalog: &'p str,
        key: &'p str,
        scope: &'static str,
        position: i64,
        url: &'p str,
        label: &'p str,
        source: &'static str,
        standing: &'static str,
    ),
    r#"
    INSERT INTO source_release_cover (
        catalog, release_id, scope, position, url, label, source, standing)
    VALUES (
        :catalog, :key, :scope, :position, :url, :label, :source, :standing)
"#
);

coven::write!(
    InsertSourceReleaseCoverCopy(
        catalog: &'p str,
        key: &'p str,
        scope: &'static str,
        position: i64,
        max_edge: u32,
        url: &'p str,
    ),
    r#"
    INSERT INTO source_release_cover_copy (
        catalog, release_id, scope, position, max_edge, url)
    VALUES (
        :catalog, :key, :scope, :position, :max_edge, :url)
"#
);

coven::write!(
    InsertSourceReleaseArchiveGroup(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        group_id: &'p str,
    ),
    r#"
    INSERT INTO source_release_archive_group (
        catalog, release_id, position, group_id)
    VALUES (
        :catalog, :key, :position, :group_id)
"#
);

coven::write!(
    InsertSourceReleaseUnfetched(
        catalog: &'p str,
        key: &'p str,
        document: &'static str,
        document_key: &'p str,
        reason: &'static str,
    ),
    r#"
    INSERT INTO source_release_unfetched (
        catalog, release_id, document, key, reason)
    VALUES (
        :catalog, :key, :document, :document_key, :reason)
"#
);

coven::write!(
    InsertSourceReleaseMedium(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        medium: Option<&'static str>,
    ),
    r#"
    INSERT INTO source_release_medium (
        catalog, release_id, position, medium)
    VALUES (
        :catalog, :key, :position, :medium)
"#
);

coven::write!(
    InsertSourceReleaseMediumDisc(
        catalog: &'p str,
        key: &'p str,
        position: i64,
        disc: i64,
        disc_id: &'p str,
    ),
    r#"
    INSERT INTO source_release_medium_disc (
        catalog, release_id, position, disc, disc_id)
    VALUES (
        :catalog, :key, :position, :disc, :disc_id)
"#
);
