//! The statements only the records' tests run: reads of the rows a test
//! checks, and writes of rows the records' own writes never write.
//! coven's compiler walks no `#[cfg(test)]` module, so they are here rather
//! than beside their tests.

// Each artist merge row: the absorbed artist and the artist it was merged
// into, by absorbed id.
#[cfg(test)]
coven::read!(
    writer ArtistMergeRows,
    "SELECT merge.id, merge.into_artist_id FROM artist_merges AS merge ORDER BY merge.id"
);

// Each label merge row: the absorbed label and the label it was merged into,
// by absorbed id.
#[cfg(test)]
coven::read!(
    writer LabelMergeRows,
    "SELECT merge.id, merge.into_label_id FROM label_merges AS merge ORDER BY merge.id"
);

// Each merged artist and the survivor of its merges, by merged id.
#[cfg(test)]
coven::read!(
    writer MergedArtistSurvivors { artist_id: String, survivor_id: String },
    r#"
    SELECT survivor.artist_id, survivor.survivor_id
    FROM merged_artist_survivors AS survivor ORDER BY survivor.artist_id
"#
);

// Each merged label and the survivor of its merges, by merged id.
#[cfg(test)]
coven::read!(
    writer MergedLabelSurvivors { label_id: String, survivor_id: String },
    r#"
    SELECT survivor.label_id, survivor.survivor_id
    FROM merged_label_survivors AS survivor ORDER BY survivor.label_id
"#
);

// Each label and the label it shows as, by label id.
#[cfg(test)]
coven::read!(
    writer LabelsShownAs,
    r#"
    SELECT shown.label_id, shown.shown_label_id
    FROM label_shown_as AS shown ORDER BY shown.label_id
"#
);

// The artists the album `:album` credits, as written, in list order.
#[cfg(test)]
coven::read!(
    writer AlbumCreditArtistIds(album: String),
    r#"
    SELECT credit.artist_id FROM album_artists AS credit
    WHERE credit.album_id = :album ORDER BY credit.position
"#
);

// The id of each artist.
#[cfg(test)]
coven::read!(writer ArtistIds, "SELECT artist.id FROM artists AS artist");

// The id of each label, with its Discogs id.
#[cfg(test)]
coven::read!(
    writer LabelIds,
    "SELECT label.id, label.discogs_label_id FROM labels AS label"
);

// The id of each release.
#[cfg(test)]
coven::read!(writer ReleaseIds, "SELECT release.id FROM releases AS release");

// The label each release label row places, if any.
#[cfg(test)]
coven::read!(
    writer PlacedLabels,
    "SELECT placed.id, placed.label_id FROM release_labels AS placed"
);

// Each release file, with the release and the coven file it names.
#[cfg(test)]
coven::read!(
    writer ReleaseFileRows,
    "SELECT file.id, file.release_id, file.file FROM release_files AS file"
);

// Each coven file this device knows.
#[cfg(test)]
coven::read!(writer CovenFileIds, "SELECT stored.id FROM coven_files AS stored");

// Each device coven registers, with its name and state.
#[cfg(test)]
coven::read!(
    writer RegisteredDevices { device: coven::DeviceId, state: coven::DeviceState },
    "SELECT registered.device, registered.name, registered.state FROM coven_devices AS registered"
);

// The device coven registers as this one when `:is_self`, else the other,
// with its name.
#[cfg(test)]
coven::read!(
    writer DeviceWhereSelf(is_self: bool) { device: coven::DeviceId },
    r#"
    SELECT registered.device, registered.name FROM coven_devices AS registered
    WHERE registered.is_self = :is_self
"#
);

// Each track with its medium, and each medium.
#[cfg(test)]
coven::read!(
    writer TracksAndMediums { id: Option<String>, medium_id: Option<String>, medium: Option<String> },
    r#"
    SELECT track.id, track.medium_id, NULL AS medium FROM tracks AS track
    UNION ALL
    SELECT NULL AS id, NULL AS medium_id, medium.id AS medium FROM release_mediums AS medium
"#
);

// The tracks that sit on no medium row the library holds.
#[cfg(test)]
coven::read!(
    writer TracksOnMissingMediums,
    r#"
    SELECT track.id FROM tracks AS track
    LEFT JOIN release_mediums AS medium ON medium.id = track.medium_id
    WHERE medium.id IS NULL
"#
);

// Move every track on the medium `:from` to the medium `:to`, as a layout
// edit that empties a medium does.
#[cfg(test)]
coven::write!(
    MoveTracksBetweenMediums(from: &'p str, to: &'p str),
    "UPDATE tracks SET medium_id = :to WHERE medium_id = :from"
);

// The ids of the release `:release`'s media rows, in the order of their
// places.
#[cfg(test)]
coven::read!(
    writer MediaRowsOfRelease(release: String) { position: i64 },
    r#"
    SELECT media.id, media.position FROM release_media AS media
    WHERE media.release_id = :release ORDER BY media.position
"#
);

// Each media row: its place, medium and count.
#[cfg(test)]
coven::read!(
    writer MediaRows,
    "SELECT media.id, media.position, media.medium, media.count FROM release_media AS media"
);

// The ids of the rows of the release `:release`'s barcodes, in the order of
// their places.
#[cfg(test)]
coven::read!(
    writer BarcodeRowsOfRelease(release: String),
    r#"
    SELECT barcode.id FROM release_barcodes AS barcode
    WHERE barcode.release_id = :release ORDER BY barcode.position
"#
);

// The ids of the rows of the release `:release`'s Discogs details, in the
// order of their places.
#[cfg(test)]
coven::read!(
    writer DiscogsDetailRowsOfRelease(release: String),
    r#"
    SELECT detail.id FROM release_discogs_details AS detail
    WHERE detail.release_id = :release ORDER BY detail.position
"#
);

// Each track of the release `:release` with its number, in play order.
#[cfg(test)]
coven::read!(
    writer TrackNumbersOfRelease(release: String) { track_number: u32 },
    r#"
    SELECT track.id, track.track_number FROM tracks AS track
    WHERE track.release_id = :release ORDER BY track.position
"#
);

// How many printed names the artists hold.
#[cfg(test)]
coven::read!(
    writer CreditNameCount { names: i64 },
    "SELECT COUNT(*) AS names FROM artist_credits AS credit_name"
);

// The printed name each work, release role and track role credit names.
#[cfg(test)]
coven::read!(
    writer RoleCreditKeys { place: i64, credit_key: Option<String> },
    r#"
    SELECT 1 AS place, credit.credit_key FROM work_artists AS credit
    UNION ALL
    SELECT 2 AS place, credit.credit_key FROM release_artist_roles AS credit
    UNION ALL
    SELECT 3 AS place, credit.credit_key FROM track_artist_roles AS credit
    ORDER BY place
"#
);

// The role, the catalog's words for it and the qualifiers of each work
// credit, in list order.
#[cfg(test)]
coven::read!(
    writer WorkCreditRoles,
    r#"
    SELECT credit.role, credit.role_words, credit.qualifiers
    FROM work_artists AS credit ORDER BY credit.position
"#
);

// The roles the writing roles view names.
#[cfg(test)]
coven::read!(writer WritingRoles { role: String }, "SELECT writing.role FROM writing_credit_roles AS writing");

// Four disc names, in the natural order bae's schema names.
#[cfg(test)]
coven::read!(
    writer NaturallyOrderedNames { name: String },
    r#"
    SELECT listed.column1 AS name
    FROM (VALUES ('Disc 10'), ('disc 2'), ('Disc 2'), ('Disc 1')) AS listed
    ORDER BY listed.column1 COLLATE natural_order
"#
);

// The id of each release record.
#[cfg(test)]
coven::read!(writer ReleaseRecordIds, "SELECT record.id FROM release_records AS record");

// Each album credit's album, artist and place.
#[cfg(test)]
coven::read!(
    writer AlbumCreditPlaces,
    "SELECT credit.album_id, credit.artist_id, credit.position FROM album_artists AS credit"
);

// The label the release `:release`'s label row names.
coven::read!(
    writer LabelOfRelease(release: String),
    r#"
    SELECT placed.label_id FROM release_labels AS placed
    WHERE placed.release_id = :release
"#
);

// The label each of the release `:release`'s label rows names, as stored,
// with its number, in list order.
coven::read!(
    writer PlacedLabelsOfRelease(release: String),
    r#"
    SELECT placed.label_id, placed.catalog_number FROM release_labels AS placed
    WHERE placed.release_id = :release ORDER BY placed.position
"#
);

// How many labels the library holds.
#[cfg(test)]
coven::read!(writer LabelCount { labels: i64 }, "SELECT COUNT(*) AS labels FROM labels AS label");

// How many artists hold the Discogs id `:discogs`.
coven::read!(
    writer ArtistsHoldingDiscogsId(discogs: String) { holding: i64 },
    r#"
    SELECT COUNT(*) AS holding FROM artists AS artist
    WHERE artist.discogs_artist_id = :discogs
"#
);

// How many artist merge rows the library holds.
coven::read!(
    writer ArtistMergeCount { merges: i64 },
    "SELECT COUNT(*) AS merges FROM artist_merges AS merge"
);

// An album row, as a bare library's test plants it.
#[cfg(test)]
coven::write!(
    PlantAlbum(id: &'p str, title: &'p str, created_at: &'p str),
    r#"
    INSERT INTO albums (id, title, is_compilation, created_at)
    VALUES (:id, :title, 0, :created_at)
"#
);

// A release row of the album `:album_id`, first in its album.
#[cfg(test)]
coven::write!(
    PlantRelease(id: &'p str, album_id: &'p str, created_at: &'p str),
    r#"
    INSERT INTO releases (id, album_id, created_at, album_order)
    VALUES (:id, :album_id, :created_at, 0)
"#
);

// A release's creation time as `:created_at` holds it, which may be no
// timestamp at all.
#[cfg(test)]
coven::write!(
    DamageReleaseCreatedAt(id: &'p str, created_at: &'p str),
    "UPDATE releases SET created_at = :created_at WHERE id = :id"
);

// The name of the folder the release `:id` was imported from.
#[cfg(test)]
coven::write!(
    SetSourceFolderName(id: &'p str, name: &'p str),
    "UPDATE releases SET source_folder_name = :name WHERE id = :id"
);

// coven's record of the file `:id`, one byte long, that only device 1 holds,
// as coven writes it.
#[cfg(test)]
coven::write!(
    PlantCovenFile(id: &'p str),
    r#"
    INSERT OR IGNORE INTO coven_files (id, audience, size, coordinates, namespace)
    VALUES (:id, '', 1, 'device 1', 'release_files')
"#
);

// Every stored file's recorded length as `:size`, which may be a length no
// write stores.
#[cfg(test)]
coven::write!(DamageCovenFileSizes(size: i64), "UPDATE coven_files SET size = :size");

// A release file row with any role, kind and source-audio facts, as another
// device's write could store it.
#[cfg(test)]
coven::write!(
    PlantReleaseFile(
        id: &'p str,
        release_id: &'p str,
        content_type: &'p str,
        file: &'p str,
        role: &'p str,
        document_kind: Option<&'p str>,
        source_audio_layout: Option<&'p str>,
        source_audio_content_type: Option<&'p str>,
        source_audio_container: Option<&'p str>,
        source_audio_duration_ms: Option<i64>,
        source_audio_sample_rate_hz: Option<i64>,
        source_audio_bits_per_sample: Option<i64>,
        source_audio_channels: Option<i64>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO release_files (
        id, release_id, original_filename, content_type, file, role, document_kind,
        source_audio_layout, source_audio_content_type, source_audio_container,
        source_audio_duration_ms, source_audio_sample_rate_hz, source_audio_bits_per_sample,
        source_audio_channels, created_at
    ) VALUES (
        :id, :release_id, :id, :content_type, :file, :role, :document_kind,
        :source_audio_layout, :source_audio_content_type, :source_audio_container,
        :source_audio_duration_ms, :source_audio_sample_rate_hz, :source_audio_bits_per_sample,
        :source_audio_channels, :created_at
    )
"#
);

// A one-sided CD medium of the release `:release_id` at place 1.
#[cfg(test)]
coven::write!(
    PlantMedium(id: &'p str, release_id: &'p str, created_at: &'p str),
    r#"
    INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at)
    VALUES (:id, :release_id, 1, 'cd', 1, :created_at)
"#
);

// A three-minute track, number 1 on side 1 of the medium `:medium_id`.
#[cfg(test)]
coven::write!(
    PlantTrack(id: &'p str, release_id: &'p str, medium_id: &'p str, created_at: &'p str),
    r#"
    INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, duration_ms, created_at)
    VALUES (:id, :release_id, 'Track Title', :medium_id, 1, 1, 180000, :created_at)
"#
);

// A FLAC audio format of the track `:track_id`, at 44.1 kHz in two channels.
#[cfg(test)]
coven::write!(
    PlantAudioFormat(id: &'p str, track_id: &'p str, created_at: &'p str),
    r#"
    INSERT INTO audio_formats (id, track_id, content_type, sample_rate, channels, created_at)
    VALUES (:id, :track_id, 'audio/flac', 44100, 2, :created_at)
"#
);

// A segment of the audio format `:audio_format_id` over the file `:file_id`,
// in any role and with any positions, as another device's write could store
// it.
#[cfg(test)]
coven::write!(
    PlantSegment(
        id: &'p str,
        audio_format_id: &'p str,
        segment_index: i64,
        role: &'p str,
        file_id: &'p str,
        start_sample: i64,
        end_sample: Option<i64>,
        start_byte: Option<i64>,
        end_byte: Option<i64>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO audio_format_segments (
        id, audio_format_id, segment_index, role, file_id,
        start_sample, end_sample, start_byte, end_byte, created_at
    ) VALUES (
        :id, :audio_format_id, :segment_index, :role, :file_id,
        :start_sample, :end_sample, :start_byte, :end_byte, :created_at
    )
"#
);

// The segment `:id`'s position `:column` (its start or end sample or byte) as
// `:value`, which may be a position no write stores.
#[cfg(test)]
coven::write!(
    DamageSegmentPosition(id: &'p str, column: &'p str, value: i64),
    r#"
    UPDATE audio_format_segments SET
        start_sample = CASE WHEN :column = 'start_sample' THEN :value ELSE start_sample END,
        end_sample = CASE WHEN :column = 'end_sample' THEN :value ELSE end_sample END,
        start_byte = CASE WHEN :column = 'start_byte' THEN :value ELSE start_byte END,
        end_byte = CASE WHEN :column = 'end_byte' THEN :value ELSE end_byte END
    WHERE id = :id
"#
);

// The audio format `:id`'s value `:column` (a pregap length, its sample rate,
// channels or bit depth) as `:value`, which may be a value no write stores.
#[cfg(test)]
coven::write!(
    DamageAudioFormatValue(id: &'p str, column: &'p str, value: Option<i64>),
    r#"
    UPDATE audio_formats SET
        pregap_samples = CASE WHEN :column = 'pregap_samples' THEN :value ELSE pregap_samples END,
        generated_pregap_samples = CASE WHEN :column = 'generated_pregap_samples'
            THEN :value ELSE generated_pregap_samples END,
        sample_rate = CASE WHEN :column = 'sample_rate' THEN :value ELSE sample_rate END,
        channels = CASE WHEN :column = 'channels' THEN :value ELSE channels END,
        bits_per_sample = CASE WHEN :column = 'bits_per_sample' THEN :value ELSE bits_per_sample END
    WHERE id = :id
"#
);

// The scrub points of the segment `:segment` as the blob `:points`, which may
// be one no write stores.
#[cfg(test)]
coven::write!(
    PlantScrubPoints(segment: &'p str, points: &'p [u8]),
    r#"
    INSERT OR REPLACE INTO audio_segment_scrub_points (audio_format_segment_id, points)
    VALUES (:segment, :points)
"#
);
