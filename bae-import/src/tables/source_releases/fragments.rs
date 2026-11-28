//! A stored catalog release as one row: its own columns, and one fragment for
//! each table under it, whose rows are one JSON array.

coven::fragment!(
    Labels on source_release {
        struct StoredLabel {
            name: Option<String>,
            catalog_number: Option<String>,
            musicbrainz_label_id: Option<String>,
            discogs_label_id: Option<String>
        }
        labels: json<Vec<StoredLabel>>
    },
    r#"
    SELECT json_group_array(json_object('name', label.name,
               'catalog_number', label.catalog_number,
               'musicbrainz_label_id', label.musicbrainz_label_id,
               'discogs_label_id', label.discogs_label_id) ORDER BY label.position) AS labels
    FROM source_release_label AS label
    WHERE label.catalog = @parent.catalog AND label.release_id = @parent.release_id
"#
);

coven::fragment!(
    Barcodes on source_release { barcodes: json<Vec<String>> },
    r#"
    SELECT json_group_array(barcode.barcode ORDER BY barcode.position) AS barcodes
    FROM source_release_barcode AS barcode
    WHERE barcode.catalog = @parent.catalog AND barcode.release_id = @parent.release_id
"#
);

coven::fragment!(
    AlbumArtists on source_release {
        struct StoredAlbumArtist {
            artist: crate::tables::source_releases::tracklist::StoredArtist,
            join_phrase: Option<String>
        }
        album_artists: json<Vec<StoredAlbumArtist>>
    },
    r#"
    SELECT json_group_array(json_object(
               'artist', json_object('name', credit.name, 'sort_name', credit.sort_name,
                   'musicbrainz_artist_id', credit.musicbrainz_artist_id,
                   'discogs_artist_id', credit.discogs_artist_id,
                   'credited_as', credit.credited_as),
               'join_phrase', credit.join_phrase) ORDER BY credit.position) AS album_artists
    FROM source_release_album_artist AS credit
    WHERE credit.catalog = @parent.catalog AND credit.release_id = @parent.release_id
"#
);

coven::fragment!(
    Links on source_release {
        struct StoredLink { catalog: String, key: String }
        links: json<Vec<StoredLink>>
    },
    r#"
    SELECT json_group_array(json_object('catalog', link.link_catalog, 'key', link.link_key)
               ORDER BY link.position) AS links
    FROM source_release_link AS link
    WHERE link.catalog = @parent.catalog AND link.release_id = @parent.release_id
"#
);

coven::fragment!(
    AlbumLinkRows on source_release {
        album_link_rows: json<Vec<crate::tables::album_link_rows::AlbumLinkRow>>
    },
    r#"
    SELECT json_group_array(json_object('catalog', link.album_catalog, 'key', link.album_key)
               ORDER BY link.ordinal) AS album_link_rows
    FROM source_release_album_link AS link
    WHERE link.catalog = @parent.catalog AND link.release_id = @parent.release_id
"#
);

coven::fragment!(
    Formats on source_release {
        struct StoredFormat { medium: Option<String>, quantity: u32 }
        formats: json<Vec<StoredFormat>>
    },
    r#"
    SELECT json_group_array(json_object('medium', format.medium, 'quantity', format.quantity)
               ORDER BY format.position) AS formats
    FROM source_release_format AS format
    WHERE format.catalog = @parent.catalog AND format.release_id = @parent.release_id
"#
);

// The records, in the order surfaces list their catalogs.
coven::fragment!(
    Records on source_release {
        struct StoredReleaseRecord {
            catalog: String,
            kind: String,
            key: String,
            album_key: Option<String>
        }
        records: json<Vec<StoredReleaseRecord>>
    },
    r#"
    SELECT json_group_array(json_object('catalog', record.record_catalog, 'kind', record.kind,
               'key', record.key, 'album_key', record.album_key)
               ORDER BY bae_catalog_rank(record.record_catalog)) AS records
    FROM source_release_record AS record
    WHERE record.catalog = @parent.catalog AND record.release_id = @parent.release_id
"#
);

coven::fragment!(
    Covers on source_release {
        struct StoredReleaseCover {
            scope: String,
            url: String,
            label: String,
            source: String,
            standing: String,
            copies: Vec<bae_catalogs::remote_image::DownscaledCopy>
        }
        covers: json<Vec<StoredReleaseCover>>
    },
    r#"
    SELECT json_group_array(json_object('scope', cover.scope, 'url', cover.url,
               'label', cover.label, 'source', cover.source, 'standing', cover.standing,
               'copies', json((
                   SELECT json_group_array(json_object('url', copy.url,
                              'max_edge', copy.max_edge) ORDER BY copy.max_edge)
                   FROM source_release_cover_copy AS copy
                   WHERE copy.catalog = cover.catalog AND copy.release_id = cover.release_id
                     AND copy.scope = cover.scope AND copy.position = cover.position)))
               ORDER BY cover.scope, cover.position) AS covers
    FROM source_release_cover AS cover
    WHERE cover.catalog = @parent.catalog AND cover.release_id = @parent.release_id
"#
);

coven::fragment!(
    ArchiveGroups on source_release { archive_groups: json<Vec<String>> },
    r#"
    SELECT json_group_array(archive.group_id ORDER BY archive.position) AS archive_groups
    FROM source_release_archive_group AS archive
    WHERE archive.catalog = @parent.catalog AND archive.release_id = @parent.release_id
"#
);

coven::fragment!(
    ReleaseRoles on source_release {
        release_roles: json<Vec<crate::tables::source_releases::tracklist::StoredRole>>
    },
    r#"
    SELECT json_group_array(json_object('position', role.position,
               'artist', json_object('name', role.name, 'sort_name', role.sort_name,
                   'musicbrainz_artist_id', role.musicbrainz_artist_id,
                   'discogs_artist_id', role.discogs_artist_id,
                   'credited_as', role.credited_as),
               'role', role.role, 'role_words', role.role_words,
               'qualifiers', role.qualifiers) ORDER BY role.position) AS release_roles
    FROM source_release_role AS role
    WHERE role.catalog = @parent.catalog AND role.release_id = @parent.release_id
"#
);

coven::fragment!(
    Mediums on source_release {
        struct StoredReleaseMedium { medium: Option<String>, disc_ids: Vec<String> }
        mediums: json<Vec<StoredReleaseMedium>>
    },
    r#"
    SELECT json_group_array(json_object('medium', medium.medium,
               'disc_ids', json((
                   SELECT json_group_array(disc.disc_id ORDER BY disc.disc)
                   FROM source_release_medium_disc AS disc
                   WHERE disc.catalog = medium.catalog AND disc.release_id = medium.release_id
                     AND disc.position = medium.position)))
               ORDER BY medium.position) AS mediums
    FROM source_release_medium AS medium
    WHERE medium.catalog = @parent.catalog AND medium.release_id = @parent.release_id
"#
);

// A track's printed name is its credit's own, so a display credit's artist
// states no `credited_as`.
coven::fragment!(
    Entries on source_release {
        struct StoredEntry {
            entry: i64,
            medium: i64,
            parent: Option<i64>,
            kind: String,
            position: Option<String>,
            title: Option<String>,
            duration_ms: Option<i64>,
            credits: Vec<crate::tables::source_releases::tracklist::StoredCredit>,
            roles: Vec<crate::tables::source_releases::tracklist::StoredRole>
        }
        entries: json<Vec<StoredEntry>>
    },
    r#"
    SELECT json_group_array(json_object('entry', entry.entry, 'medium', entry.medium,
               'parent', entry.parent, 'kind', entry.kind, 'position', entry.position,
               'title', entry.title, 'duration_ms', entry.duration_ms,
               'credits', json((
                   SELECT json_group_array(json_object('position', credit.position,
                              'credited_name', credit.credited_name,
                              'artist', CASE WHEN credit.name IS NOT NULL THEN
                                  json_object('name', credit.name,
                                      'sort_name', credit.sort_name,
                                      'musicbrainz_artist_id', credit.musicbrainz_artist_id,
                                      'discogs_artist_id', credit.discogs_artist_id) END,
                              'join_phrase', credit.join_phrase) ORDER BY credit.position)
                   FROM source_release_entry_credit AS credit
                   WHERE credit.catalog = entry.catalog AND credit.release_id = entry.release_id
                     AND credit.entry = entry.entry)),
               'roles', json((
                   SELECT json_group_array(json_object('position', role.position,
                              'artist', json_object('name', role.name,
                                  'sort_name', role.sort_name,
                                  'musicbrainz_artist_id', role.musicbrainz_artist_id,
                                  'discogs_artist_id', role.discogs_artist_id,
                                  'credited_as', role.credited_as),
                              'role', role.role, 'role_words', role.role_words,
                              'qualifiers', role.qualifiers) ORDER BY role.position)
                   FROM source_release_entry_role AS role
                   WHERE role.catalog = entry.catalog AND role.release_id = entry.release_id
                     AND role.entry = entry.entry)))
               ORDER BY entry.entry) AS entries
    FROM source_release_entry AS entry
    WHERE entry.catalog = @parent.catalog AND entry.release_id = @parent.release_id
"#
);

// Each work node with its relations in the order of their positions: a
// credit, or the node of a part.
coven::fragment!(
    Works on source_release {
        struct StoredWork {
            node: i64,
            entry: Option<i64>,
            parent: Option<i64>,
            position: i32,
            direction: Option<String>,
            musicbrainz_work_id: String,
            title: String,
            disambiguation: Option<String>,
            work_type: Option<String>,
            events: Vec<crate::tables::source_releases::tracklist::StoredEvent>
        }
        works: json<Vec<StoredWork>>
    },
    r#"
    SELECT json_group_array(json_object('node', work.node, 'entry', work.entry,
               'parent', work.parent, 'position', work.position,
               'direction', work.direction,
               'musicbrainz_work_id', work.musicbrainz_work_id, 'title', work.title,
               'disambiguation', work.disambiguation, 'work_type', work.work_type,
               'events', json((
                   SELECT json_group_array(json(event.value) ORDER BY event.position)
                   FROM (SELECT credit.position AS position,
                             json_object('credit', json_object(
                                 'artist', json_object('name', credit.name,
                                     'sort_name', credit.sort_name,
                                     'musicbrainz_artist_id', credit.musicbrainz_artist_id,
                                     'discogs_artist_id', credit.discogs_artist_id,
                                     'credited_as', credit.credited_as),
                                 'role', credit.role, 'role_words', credit.role_words,
                                 'qualifiers', credit.qualifiers)) AS value
                         FROM source_release_work_credit AS credit
                         WHERE credit.catalog = work.catalog
                           AND credit.release_id = work.release_id
                           AND credit.node = work.node
                         UNION ALL
                         SELECT part.position, json_object('part', part.node)
                         FROM source_release_work AS part
                         WHERE part.catalog = work.catalog
                           AND part.release_id = work.release_id
                           AND part.parent = work.node) AS event)))
               ORDER BY work.node) AS works
    FROM source_release_work AS work
    WHERE work.catalog = @parent.catalog AND work.release_id = @parent.release_id
"#
);

coven::fragment!(
    Unfetched on source_release {
        struct StoredUnfetched { document: String, key: String, reason: String }
        unfetched: json<Vec<StoredUnfetched>>
    },
    r#"
    SELECT json_group_array(json_object('document', missing.document, 'key', missing.key,
               'reason', missing.reason)
               ORDER BY missing.document, missing.key) AS unfetched
    FROM source_release_unfetched AS missing
    WHERE missing.catalog = @parent.catalog AND missing.release_id = @parent.release_id
"#
);

// One stored release with every row under it, as its own documents state it.
coven::fragment!(
    StoredRelease {
        album_first_year: Option<i32>,
        year: Option<i32>,
        discogs_details: json<Vec<bae_types::pressing::DiscogsDetail>>,
        notes: json<Vec<String>>
    },
    r#"
    SELECT release.catalog, release.release_id, release.source_group_id, release.album_title,
           release.album_first_year, release.year, release.country, release.region,
           release.status, release.packaging, release.discogs_details, release.notes,
           release.album_links, release.archive_release_id, release.archive_group_id,
           @crate::tables::source_releases::fragments::Labels(parent: release) AS labels,
           @crate::tables::source_releases::fragments::Barcodes(parent: release) AS barcodes,
           @crate::tables::source_releases::fragments::AlbumArtists(parent: release)
               AS album_artists,
           @crate::tables::source_releases::fragments::Links(parent: release) AS links,
           @crate::tables::source_releases::fragments::AlbumLinkRows(parent: release)
               AS album_link_rows,
           @crate::tables::source_releases::fragments::Formats(parent: release) AS formats,
           @crate::tables::source_releases::fragments::Records(parent: release) AS records,
           @crate::tables::source_releases::fragments::Covers(parent: release) AS covers,
           @crate::tables::source_releases::fragments::ArchiveGroups(parent: release)
               AS archive_groups,
           @crate::tables::source_releases::fragments::ReleaseRoles(parent: release)
               AS release_roles,
           @crate::tables::source_releases::fragments::Mediums(parent: release) AS mediums,
           @crate::tables::source_releases::fragments::Entries(parent: release) AS entries,
           @crate::tables::source_releases::fragments::Works(parent: release) AS works,
           @crate::tables::source_releases::fragments::Unfetched(parent: release) AS unfetched
    FROM source_release AS release
"#
);
