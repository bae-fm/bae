//! A release's files as its detail and its editor list them, over rows the
//! caller gives, and which of a folder's or a release's files no track plays.

// The content types of the images bae can show: the formats a cover can be
// made from (`ContentType::SUPPORTED_COVERS`). The gallery shows these images,
// and a release's Files list leaves them out, so each image of a release is in
// exactly one of them.
sqlive_compose::fragment!(
    ShowableImageTypes { content_type: String },
    r#"
    SELECT 'image/jpeg' AS content_type
    UNION ALL SELECT 'image/png'
    UNION ALL SELECT 'image/gif'
    UNION ALL SELECT 'image/webp'
"#
);

// The files of `listed` that a Files list shows. `listed.showable_image` is
// true for each file the gallery shows as an image, and `track_reads` names
// the audio file each track's segment reads and the CUE sheet that carves a
// track, either NULL. The list leaves out the files a track plays from (a
// file a track reads and a sheet that carves a track) and the images the
// gallery shows. Every other file is listed: a sheet that carves no track, an
// audio file no track reads, and an image in a format bae cannot show. The
// import pane supplies the scan's rows and the library its own, so both list
// a release's files by this one rule.
sqlive_compose::fragment!(
    UnplayedFiles
    with listed(file_id TEXT, showable_image INTEGER),
         track_reads(audio_file_id TEXT, sheet_file_id TEXT)
    { file_id: String },
    r#"
    SELECT listed.file_id
    FROM listed
    WHERE NOT listed.showable_image
      AND NOT EXISTS (SELECT 1 FROM track_reads AS taken
                      WHERE taken.audio_file_id = listed.file_id
                         OR taken.sheet_file_id = listed.file_id)
"#
);

// Each release file `files` gives, with the columns of `release_files`: its
// id as `document_file_id`, its name, content type and size, and `file_json`,
// the JSON object every list of a release's files holds it as.
sqlive_compose::fragment!(
    FileDocuments
    with files(
        id TEXT, release_id TEXT, original_filename TEXT, content_type TEXT, created_at TEXT,
        file TEXT, role TEXT, document_kind TEXT, source_audio_layout TEXT,
        source_audio_content_type TEXT, source_audio_container TEXT,
        source_audio_duration_ms INTEGER, source_audio_sample_rate_hz INTEGER,
        source_audio_bits_per_sample INTEGER, source_audio_bitrate_kbps INTEGER,
        source_audio_channels INTEGER
    )
    {
        document_file_id: String,
        original_filename: String,
        content_type: String,
        file_size: u64,
        file_json: String,
    },
    r#"
    SELECT f.id AS document_file_id, f.original_filename, f.content_type,
           stored.size AS file_size,
           json_object(
               'id', f.id, 'release_id', f.release_id,
               'original_filename', f.original_filename, 'content_type', f.content_type,
               'created_at', f.created_at, 'file', f.file, 'role', f.role,
               'document_kind', f.document_kind,
               'source_audio_layout', f.source_audio_layout,
               'source_audio_content_type', f.source_audio_content_type,
               'source_audio_container', f.source_audio_container,
               'source_audio_duration_ms', f.source_audio_duration_ms,
               'source_audio_sample_rate_hz', f.source_audio_sample_rate_hz,
               'source_audio_bits_per_sample', f.source_audio_bits_per_sample,
               'source_audio_bitrate_kbps', f.source_audio_bitrate_kbps,
               'source_audio_channels', f.source_audio_channels,
               'file_size', stored.size, 'stored', stored.hash IS NOT NULL) AS file_json
    FROM files AS f
    JOIN coven_files AS stored ON stored.id = f.file
"#
);

// The release's files, from the rows `files` gives (the release's own, with
// the columns of `release_files`), as one JSON object: every file, and among them the images
// the gallery shows and the files no track plays from ([`UnplayedFiles`]),
// each in the release's file order (natural order of their names, case aside,
// id breaking a tie), with the number of files, the number of images, the
// files' total size and the summary of their audio. A track plays from each
// file `segments` reads and from each CUE sheet `carved` names. `sheets`
// gives each CUE sheet none of whose tracks stay the JSON of its row, and
// `carried` lists the files no track plays from in the same order as
// `others`, each file or such a sheet in its place.
sqlive_compose::fragment!(
    ReleaseFiles on releases
    with files(
        id TEXT, release_id TEXT, original_filename TEXT, content_type TEXT, created_at TEXT,
        file TEXT, role TEXT, document_kind TEXT, source_audio_layout TEXT,
        source_audio_content_type TEXT, source_audio_container TEXT,
        source_audio_duration_ms INTEGER, source_audio_sample_rate_hz INTEGER,
        source_audio_bits_per_sample INTEGER, source_audio_bitrate_kbps INTEGER,
        source_audio_channels INTEGER
    ), segments(file_id TEXT), carved(sheet_file_id TEXT), sheets(file_id TEXT, sheet TEXT)
    { release_files: String },
    r#"
    WITH unplayed AS (
        SELECT free.file_id
        FROM @crate::records::file_fragments::UnplayedFiles(
            listed: (SELECT image.id, image.content_type IN (
                         SELECT shown.content_type
                         FROM @crate::records::file_fragments::ShowableImageTypes AS shown)
                     FROM files AS image),
            track_reads: (SELECT segment.file_id, NULL FROM segments AS segment
                          UNION ALL
                          SELECT NULL, cut.sheet_file_id FROM carved AS cut)) AS free),
    listed AS (
        SELECT f.document_file_id AS id, f.original_filename,
               f.content_type IN (
                   SELECT shown.content_type
                   FROM @crate::records::file_fragments::ShowableImageTypes AS shown) AS showable,
               f.document_file_id IN (SELECT file_id FROM unplayed) AS in_files,
               f.file_size AS size, f.file_json,
               (SELECT replaced.sheet FROM sheets AS replaced
                WHERE replaced.file_id = f.document_file_id) AS sheet_without_tracks
        FROM @crate::records::file_fragments::FileDocuments(
                 files: (SELECT * FROM files)) AS f)
    SELECT json_object(
               'files', json(json_group_array(json(listed.file_json)
                    ORDER BY listed.original_filename COLLATE natural_order, listed.id)),
               'images', json(json_group_array(json(listed.file_json)
                    ORDER BY listed.original_filename COLLATE natural_order, listed.id)
                    FILTER (WHERE listed.showable)),
               'others', json(json_group_array(json(listed.file_json)
                    ORDER BY listed.original_filename COLLATE natural_order, listed.id)
                    FILTER (WHERE listed.in_files)),
               'carried', json(json_group_array(
                    CASE WHEN listed.sheet_without_tracks IS NULL
                         THEN json_object('file', json(listed.file_json))
                         ELSE json_object('sheet', json(listed.sheet_without_tracks)) END
                    ORDER BY listed.original_filename COLLATE natural_order, listed.id)
                    FILTER (WHERE listed.in_files)),
               'has_others', COALESCE(SUM(listed.in_files), 0) > 0,
               'count', COUNT(*), 'image_count', COUNT(*) FILTER (WHERE listed.showable),
               'total_size', COALESCE(SUM(listed.size), 0),
               'source_audio', json((
                    SELECT summed.summary
                    FROM @crate::source_audio::AudioSummary(
                        counted: (SELECT ROW_NUMBER() OVER (
                                          ORDER BY audio.original_filename
                                              COLLATE natural_order, audio.id),
                                         audio.source_audio_layout,
                                         audio.source_audio_content_type,
                                         audio.source_audio_sample_rate_hz,
                                         audio.source_audio_bits_per_sample,
                                         audio.source_audio_bitrate_kbps,
                                         audio.source_audio_channels,
                                         audio.source_audio_duration_ms
                                  FROM files AS audio)) AS summed))) AS release_files
    FROM listed
"#
);

// [`ShowableImageTypes`]'s rows, which a test compares with the formats a
// cover can be made from.
#[cfg(test)]
sqlive_compose::read!(
    ShownImageTypes,
    r#"
    SELECT shown.*
    FROM @crate::records::file_fragments::ShowableImageTypes AS shown
"#
);

#[cfg(test)]
#[path = "file_fragments_tests.rs"]
mod tests;
