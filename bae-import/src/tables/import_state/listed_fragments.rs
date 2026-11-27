//! What the candidate state offers the import list's rows: for each folder's
//! content, whether its state describes its current files, its last import's
//! failure, its draft's provenance, its link and its cover; and what is
//! running for each candidate.

// Each content hash whose state is stored, at the file edit revision the
// state describes.
sqlive_compose::fragment!(
    ListedState,
    r#"
    SELECT state.content_hash AS state_hash, state.edit_revision AS state_revision
    FROM import_candidate_state AS state
"#
);

// The failure each content's last import left.
sqlive_compose::fragment!(
    ListedFailure,
    r#"
    SELECT failure.content_hash AS failed_hash, failure.error AS failed_error
    FROM import_candidate_failure AS failure
"#
);

// What is running for each candidate.
sqlive_compose::fragment!(
    ListedStanding,
    r#"
    SELECT standing.candidate_key AS standing_candidate, standing.identification,
        standing.import_asked, standing.save_failure_error, standing.import_step
    FROM import_candidate_standing AS standing
"#
);

// Each draft's provenance.
sqlive_compose::fragment!(
    ListedProvenance,
    r#"
    SELECT provenance.content_hash AS provenance_hash, provenance.kind AS provenance_kind,
        provenance.source AS provenance_source,
        provenance.release_id AS provenance_release_id
    FROM import_candidate_draft_provenance AS provenance
"#
);

// Each candidate's release link: a pressing's partners in the other catalogs,
// by catalog, an album's album in each catalog, in catalog order, and the
// stored records it claims, in the order surfaces list them, as JSON arrays.
sqlive_compose::fragment!(
    ListedLink { link_partners: String, link_albums: String, link_records: String },
    r#"
    SELECT link.content_hash AS link_hash, link.kind AS link_kind, link.source AS link_source,
        link.release_id AS link_release_id,
        (SELECT json_group_array(json_array(partner.source, partner.release_id)
                    ORDER BY partner.source)
         FROM import_candidate_release_link_partner AS partner
         WHERE partner.content_hash = link.content_hash) AS link_partners,
        (SELECT json_group_array(json_array(album.catalog, album.album_id)
                    ORDER BY bae_catalog_rank(album.catalog), album.catalog)
         FROM import_candidate_release_link_album AS album
         WHERE album.content_hash = link.content_hash) AS link_albums,
        (SELECT json_group_array(json_object('catalog', claimed.catalog, 'kind', claimed.kind,
                    'key', claimed.key, 'album', claimed.album) ORDER BY claimed.position)
         FROM import_candidate_claimed_record AS claimed
         WHERE claimed.content_hash = link.content_hash) AS link_records
    FROM import_candidate_release_link AS link
"#
);

// Each candidate's chosen cover, with the copies the catalog serves of a
// remote one as a JSON array.
sqlive_compose::fragment!(
    ListedCover { cover_copies: String },
    r#"
    SELECT cover.content_hash AS cover_hash, cover.kind AS cover_kind,
        cover.file_id AS cover_file_id, cover.url AS cover_url, cover.source AS cover_source,
        (SELECT json_group_array(json_object('url', copy.url, 'max_edge', copy.max_edge)
                    ORDER BY copy.max_edge)
         FROM import_candidate_cover_copy AS copy
         WHERE copy.content_hash = cover.content_hash AND copy.image_url = cover.url)
            AS cover_copies
    FROM import_candidate_cover AS cover
"#
);
