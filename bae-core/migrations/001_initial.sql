-- bae's application schema. coven runs this (idempotently) after its own
-- bookkeeping migration when it opens the connection it owns, so every
-- `CREATE TABLE`/`CREATE INDEX` is `IF NOT EXISTS`: re-running over a
-- snapshot-bootstrapped database that already carries the schema is a no-op.
--
-- The `sync_cursors`, `sync_state`, and `cloud_outbox` bookkeeping tables are
-- owned by coven (created by coven's MIGRATION_SQL), not declared here.

CREATE TABLE IF NOT EXISTS artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_name TEXT,
    discogs_artist_id TEXT,
    musicbrainz_artist_id TEXT,

    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

-- Albums are aggregates over releases; identity lives on `release_identities`.
CREATE TABLE IF NOT EXISTS albums (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    -- Primary artist FK. Additional artists live in album_artists with position > 0.
    -- Nullable in SQLite because NOT NULL cannot be added to an existing column
    -- without recreating the table; the application layer treats it as required.
    artist_id TEXT REFERENCES artists(id),
    year INTEGER,
    -- The release that supplies the album's cover art and is shown by default.
    -- When NULL, callers fall back to the first release.
    primary_release_id TEXT,
    is_compilation INTEGER NOT NULL DEFAULT 0,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS album_artists (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(album_id, artist_id)
) STRICT;

CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    release_name TEXT,
    year INTEGER,
    disc_id TEXT,
    -- 'musicbrainz' | 'discogs' | 'file_tags'.
    metadata_source TEXT NOT NULL,
    -- Specific MB/Discogs release the metadata was seeded from. NULL
    -- when `metadata_source = 'file_tags'`.
    metadata_source_release_id TEXT,
    format TEXT,
    label TEXT,
    catalog_number TEXT,
    country TEXT,
    barcode TEXT,
    -- Shared, synced fact (the coven gate column): is this release's audio in
    -- the cloud home (remote) or local to one device (local). A local release's
    -- in-place files are tracked by coven as external blob refs
    -- (`local_blob_refs`, coven's own device-local table), NOT here — they must
    -- not sync. A remote release's bytes live in coven's blob cache.
    remote INTEGER NOT NULL,
    source_folder_name TEXT,
    -- SHA-256 over the imported folder's categorized file structure (sorted
    -- relative paths + sizes). Location-independent content fingerprint: the
    -- same rip in any parent folder hashes the same. Used to recognize an
    -- already-imported folder and to pick the overwrite target on re-import.
    content_hash TEXT,
    -- Album-level loudness measured at import (EBU R128 integrated loudness over
    -- all tracks combined), in LUFS. NULL = not measured (a measurement failure,
    -- or imported before measurement existed). Playback derives a gain from this
    -- and a constant target; the stored value is the raw measurement, never a gain.
    album_loudness_lufs REAL,
    -- Album-level true peak as a LINEAR ratio (1.0 = 0 dBTP), the max across all
    -- tracks. NULL = not measured. Playback caps the album gain at 1.0/peak to
    -- prevent clipping.
    album_peak_linear REAL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;

-- The device-local truth that a local release's files are in place is owned by
-- coven, not bae: a local release file is a coven *user-provided* blob, tracked
-- in coven's own device-local `local_blob_refs` table (blob_id = file id, the
-- path = `folder/original_filename`). bae registers/clears those refs through
-- coven's `register_external_blob` / `clear_external_blob`; coven flips them as
-- part of the make-Remote / make-Local transitions. There is no bae table here.

-- Per-source identity rows. A release has 0+ rows: 0 = Unknown, 1+ = identified,
-- each naming the pressing it claims. Cross-source equivalences are encoded by
-- a release having rows in multiple sources.
CREATE TABLE IF NOT EXISTS release_identities (
    id                TEXT PRIMARY KEY,
    release_id        TEXT NOT NULL,
    source            TEXT NOT NULL,
    source_group_id   TEXT NOT NULL,
    source_release_id TEXT NOT NULL,
    _updated_at       TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    UNIQUE (release_id, source),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS tracks (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    title TEXT NOT NULL,
    side INTEGER NOT NULL,
    track_number INTEGER,
    duration_ms INTEGER,
    discogs_position TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS track_artists (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- `id` is minted, never the source's own work id: a MusicBrainz work MBID is
-- often a name-based (version 3) UUID, which the sync layer refuses on a synced
-- row. `musicbrainz_work_id` carries the source identity and is what an import
-- dedups a work on. MusicBrainz is the only source of works, so every row has
-- one. It is indexed, not UNIQUE: two devices can mint separate rows for the
-- same work, and a synced row must never fail to land.
CREATE TABLE IF NOT EXISTS works (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    disambiguation TEXT,
    work_type TEXT,
    musicbrainz_work_id TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS work_artists (
    id TEXT PRIMARY KEY,
    work_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (work_id) REFERENCES works (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(work_id, artist_id, position)
) STRICT;

CREATE TABLE IF NOT EXISTS work_parts (
    id TEXT PRIMARY KEY,
    parent_work_id TEXT NOT NULL,
    child_work_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (parent_work_id) REFERENCES works (id) ON DELETE CASCADE,
    FOREIGN KEY (child_work_id) REFERENCES works (id) ON DELETE CASCADE,
    UNIQUE(parent_work_id, child_work_id)
) STRICT;

CREATE TABLE IF NOT EXISTS track_works (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (work_id) REFERENCES works (id) ON DELETE CASCADE,
    UNIQUE(track_id, work_id)
) STRICT;

CREATE TABLE IF NOT EXISTS release_artist_roles (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    source_credit TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(release_id, artist_id, position, source)
) STRICT;

CREATE TABLE IF NOT EXISTS track_artist_roles (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    source_credit TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(track_id, artist_id, position, source)
) STRICT;

CREATE TABLE IF NOT EXISTS release_files (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    -- Cloud object key for this file's remote blob, mirroring coven's
    -- BlobRef.cloud_path. NULL = the hashed-by-id layout (opaque homes); a
    -- value = the explicit readable key set when the file entered a browsable
    -- home (`{artist}/{album}/{filename}`). Synced, so every device addresses
    -- the blob the same way; computed once at upload time and never re-derived,
    -- so a metadata rename never moves the blob.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Lowercase-hex SHA-256 of the blob's plaintext (coven's
    -- `BlobDecl::hash_column`), signed on the row alongside the declared size
    -- and verified against the decrypted bytes on a Remote fetch. NOT NULL:
    -- coven reads it off every blob-bearing row and refuses a row without one,
    -- so a hashless blob is not a state this schema can hold.
    hash TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- bits_per_sample is nullable: lossy codecs (MP3, AAC, etc.) don't expose a
-- bit depth via FFmpeg, and substituting a default would store a fabricated
-- value. NULL surfaces the absence to consumers.
-- Audio-format rows hold track-level codec/display metadata. File-backed sample
-- windows live in audio_format_segments so a CUE track can be assembled from
-- ordered windows across one or more source files.
CREATE TABLE IF NOT EXISTS audio_formats (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    pregap_ms INTEGER,
    generated_pregap_ms INTEGER,
    pregap_samples INTEGER,
    generated_pregap_samples INTEGER,
    sample_rate INTEGER NOT NULL,
    bits_per_sample INTEGER,
    channels INTEGER NOT NULL,
    -- Per-track loudness measured at import (EBU R128 integrated loudness over
    -- this track's sample window), in LUFS. NULL = not measured (decode/measure
    -- failure, or a near-silent track that has no usable loudness). Playback
    -- derives a gain from this and a constant target; the stored value is the
    -- raw measurement, never a gain.
    track_loudness_lufs REAL,
    -- Per-track true peak as a LINEAR ratio (1.0 = 0 dBTP), the max across
    -- channels. NULL = not measured. Playback caps the track gain at 1.0/peak
    -- to prevent clipping.
    track_peak_linear REAL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS audio_format_segments (
    id TEXT PRIMARY KEY,
    audio_format_id TEXT NOT NULL,
    segment_index INTEGER NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('audio_pregap', 'main')),
    file_id TEXT NOT NULL,
    start_sample INTEGER NOT NULL,
    end_sample INTEGER,
    start_byte INTEGER,
    end_byte INTEGER,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (audio_format_id, segment_index),
    FOREIGN KEY (audio_format_id) REFERENCES audio_formats (id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES release_files(id) ON DELETE CASCADE
) STRICT;

-- Album covers — the one small grid image bae produces per release, 1:1 with a
-- release (`id` IS the release id). A coven host-provided · CacheEager *asset*:
-- bae hands coven the bytes (`local_files::store("covers", id, …)`), coven owns
-- the on-device copy (its local store while Local, its cache while Remote). The
-- FK on `id` makes coven gate the cover as a child of its release and ride the
-- release's gate without keeping it alive (declared `.asset()`).
CREATE TABLE IF NOT EXISTS covers (
    -- The release id this cover belongs to (1:1).
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key for this cover's blob (relative to the `covers`
    -- namespace coven prepends), mirroring coven's BlobRef.cloud_path. NULL =
    -- the hashed-by-id layout (opaque homes); a value = the explicit readable
    -- key (`{album}/{release}/cover.{ext}`) on a browsable home.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Content hash, as on release_files.hash.
    hash TEXT NOT NULL,
    -- The id of the coven blob holding this cover's bytes. Distinct from the
    -- row id (which is the release id and cannot move): coven names one
    -- immutable byte-string per (namespace, blob id), so replacing a cover
    -- repoints the row at a NEW blob id rather than writing new bytes under
    -- the old one — which coven refuses (`BlobAlreadyReferenced`).
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- Artist images — bae-produced, 1:1 with an artist (`id` IS the artist id). A
-- coven host-provided · CacheEager *asset* of `artists`, same shape as `covers`.
CREATE TABLE IF NOT EXISTS artist_images (
    -- The artist id this image belongs to (1:1).
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key for this image's blob (relative to the `artist_images`
    -- namespace coven prepends). NULL = hashed-by-id (opaque homes); a value =
    -- the readable `{artist}/artist.{ext}` key on a browsable home.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Content hash, as on release_files.hash.
    hash TEXT NOT NULL,
    -- The id of the coven blob holding this image's bytes — a new one per
    -- stored image, as on covers.blob_id.
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- The documents a metadata lookup returned, keyed by the *source* entity each
-- one describes rather than by any local release. One store serves both halves
-- of a release's life: identification writes the rows before it stores the
-- verdict that names the release, so opening a candidate replays what
-- identification fetched with no network at all; a committed release reads the
-- same rows back through its `metadata_source` / `metadata_source_release_id`
-- pointer, which is how a metadata reset re-projects the seed.
--
-- Two candidates that match one release share its row, and a re-fetch upserts
-- it. NOT synced: no `_updated_at` and absent from `synced_tables()`, the same
-- device-local convention as `playback_state` and `import_candidate_state` —
-- these are re-fetchable provider documents, not the user's library.
--
-- No foreign key and no cascade: a row outlives whichever candidate or release
-- caused it to be fetched, and is shared between them. Pruning, if the volume
-- ever justifies it, attaches to candidate deletion.
CREATE TABLE IF NOT EXISTS source_release_payloads (
    -- Which lookup produced this document, and therefore what
    -- `source_release_id` names:
    --   'musicbrainz'                     the release itself
    --   'musicbrainz_release_group'       its release group, by group id
    --   'discogs'                         a Discogs release
    --   'discogs_master'                  a Discogs master, by master id
    --   'musicbrainz_discogs_xref'        the MusicBrainz release cross-linked
    --                                     to a Discogs one, by the *Discogs*
    --                                     release id — MusicBrainz's URL lookup
    --                                     found it, so nothing in the Discogs
    --                                     document names it back
    source TEXT NOT NULL,
    source_release_id TEXT NOT NULL,
    -- The document as the provider returned it.
    json TEXT NOT NULL CHECK (json_valid(json)),
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (source, source_release_id)
) STRICT;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_artists_discogs_id ON artists (discogs_artist_id);
CREATE INDEX IF NOT EXISTS idx_artists_mb_id ON artists (musicbrainz_artist_id);
CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_albums_artist_id ON albums (artist_id);
CREATE INDEX IF NOT EXISTS idx_album_artists_album_id ON album_artists (album_id);
CREATE INDEX IF NOT EXISTS idx_album_artists_artist_id ON album_artists (artist_id);
CREATE INDEX IF NOT EXISTS idx_track_artists_track_id ON track_artists (track_id);
CREATE INDEX IF NOT EXISTS idx_track_artists_artist_id ON track_artists (artist_id);
CREATE INDEX IF NOT EXISTS idx_releases_album_id ON releases (album_id);
CREATE INDEX IF NOT EXISTS idx_releases_content_hash
    ON releases (content_hash)
    WHERE content_hash IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_release_identities_group
    ON release_identities (source, source_group_id);
CREATE INDEX IF NOT EXISTS idx_release_identities_release
    ON release_identities (source, source_release_id);
CREATE INDEX IF NOT EXISTS idx_tracks_release_id ON tracks (release_id);
CREATE INDEX IF NOT EXISTS idx_release_files_release_id ON release_files (release_id);
CREATE INDEX IF NOT EXISTS idx_audio_formats_track_id ON audio_formats (track_id);
CREATE INDEX IF NOT EXISTS idx_audio_format_segments_format_id ON audio_format_segments (audio_format_id);

-- Device-local playback queue, restored after a restart on the same device.
-- NOT synced: no `_updated_at` and absent from `synced_tables()`, the same
-- device-local convention as coven's own bookkeeping tables. A single row
-- (id = 'current'). The row is the recipe to refill the queue, not its rows:
-- `source` is what the context lane played from (NULL for a single track),
-- `shuffled` whether that lane was shuffled, `manual` the Up Next track ids,
-- and `current_track_id` the resume point. Session edits (removals, reorders,
-- the shuffled order) are not stored — restore rebuilds a pristine lane.

CREATE INDEX IF NOT EXISTS idx_works_mb_id ON works (musicbrainz_work_id);
CREATE INDEX IF NOT EXISTS idx_work_artists_artist ON work_artists(artist_id);
CREATE INDEX IF NOT EXISTS idx_work_artists_work ON work_artists(work_id);
CREATE INDEX IF NOT EXISTS idx_work_parts_parent ON work_parts(parent_work_id);
CREATE INDEX IF NOT EXISTS idx_work_parts_child ON work_parts(child_work_id);
CREATE INDEX IF NOT EXISTS idx_track_works_track ON track_works(track_id);
CREATE INDEX IF NOT EXISTS idx_track_works_work ON track_works(work_id);
CREATE INDEX IF NOT EXISTS idx_release_artist_roles_artist ON release_artist_roles(artist_id);
CREATE INDEX IF NOT EXISTS idx_release_artist_roles_release ON release_artist_roles(release_id);
CREATE INDEX IF NOT EXISTS idx_track_artist_roles_artist ON track_artist_roles(artist_id);
CREATE INDEX IF NOT EXISTS idx_track_artist_roles_track ON track_artist_roles(track_id);

CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    source           TEXT,
    -- Whether the context lane was shuffled. Restore refills the lane from
    -- `source` and permutes it afresh; the session's shuffled order is not
    -- stored. NULL exactly when `source` is (no context playing).
    shuffled         INTEGER,
    manual           TEXT NOT NULL,
    repeat           TEXT NOT NULL,
    current_track_id TEXT,
    position_ms      INTEGER,
    volume           REAL NOT NULL,
    is_muted         INTEGER NOT NULL
);

-- Device-local import triage state, keyed by an import candidate's content
-- hash (`CategorizedFiles::content_hash` — sorted (relative_path, size) over
-- every file the release carries). NOT synced: no `_updated_at` and absent from
-- `synced_tables()`, the same device-local convention as `playback_state`
-- above. Adding, removing, or resizing a file changes the hash, which is the
-- invalidation — nothing deletes the orphaned row under the old hash.
--
-- Two independent things share the row, because both are derived state about
-- one set of bytes: what identification concluded, and what the user decided.
-- Either can be present without the other.
CREATE TABLE IF NOT EXISTS import_candidate_state (
    content_hash             TEXT PRIMARY KEY,
    -- Where the candidate was last seen. Not identity, not authoritative.
    folder_path              TEXT NOT NULL,
    -- The identify result. Set and cleared as one group: all NULL together or
    -- all set together. Set only once identification reached a terminal
    -- verdict; cleared when a file decision changes what the folder is.
    verdict_kind             TEXT CHECK (verdict_kind IN ('found', 'not_found', 'manual_only')),
    verdict_track_count      INTEGER CHECK (verdict_track_count IS NULL OR verdict_track_count >= 0),
    probed_total_duration_ms INTEGER,
    identified_at            TEXT,
    -- The identity decided for this candidate: a pressing, or the folder's own
    -- tags. `identity_pick_author` says who decided it.
    pick_kind                TEXT CHECK (pick_kind IS NULL OR pick_kind IN ('release', 'unknown')),
    pick_source              TEXT CHECK (pick_source IS NULL OR pick_source IN ('musicbrainz', 'discogs')),
    pick_release_id          TEXT,
    identity_pick_author     TEXT CHECK (identity_pick_author IS NULL OR identity_pick_author IN ('user', 'identification')),
    -- Advances with every file decision, so a verdict derived from an older
    -- shape is refused.
    edit_revision            INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0),
    CHECK (
        (verdict_kind IS NULL AND verdict_track_count IS NULL
            AND probed_total_duration_ms IS NULL AND identified_at IS NULL)
        OR
        (verdict_kind IS NOT NULL AND probed_total_duration_ms IS NOT NULL
            AND probed_total_duration_ms >= 0 AND identified_at IS NOT NULL)
    ),
    CHECK (verdict_kind IN ('found', 'manual_only') = (verdict_track_count IS NOT NULL)),
    CHECK ((pick_kind IS NULL) = (identity_pick_author IS NULL)),
    CHECK ((pick_kind = 'release') = (pick_source IS NOT NULL AND pick_release_id IS NOT NULL))
) STRICT;

-- One matched release of a found verdict, in match order, with the provenance
-- saying which signal produced it.
CREATE TABLE IF NOT EXISTS import_candidate_match (
    content_hash           TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    source                 TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id             TEXT NOT NULL,
    title                  TEXT NOT NULL,
    artist                 TEXT,
    year                   INTEGER,
    format                 TEXT,
    label                  TEXT,
    catalog_number         TEXT,
    country                TEXT,
    cover_url              TEXT,
    cover_thumbnail_url    TEXT,
    cover_label            TEXT,
    cover_source           TEXT CHECK (cover_source IS NULL OR cover_source IN ('musicbrainz', 'discogs')),
    source_group_id        TEXT,
    -- NULL: nobody asked the source for its tracklist yet. 'listed' /
    -- 'nothing': asked. The total is NULL when any listed track has no length.
    source_tracks_kind     TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing')),
    source_tracks_count    INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    source_tracks_total_ms INTEGER CHECK (source_tracks_total_ms IS NULL OR source_tracks_total_ms >= 0),
    by_disc_id             INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode             INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog        INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_thumbnail_url IS NULL) AND (cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (source_tracks_total_ms IS NULL OR source_tracks_kind = 'listed')
) STRICT;

-- One file's user decisions: its role, which audio a sheet describes, which
-- disc a sheet is. A column is NULL where the user decided nothing about
-- that aspect; an absent row is no decision at all. A cleared sheet binding
-- is stored ('cleared'), not removed: the scan's proposal is not restored.
CREATE TABLE IF NOT EXISTS import_candidate_file_edit (
    content_hash          TEXT NOT NULL,
    relative_path         TEXT NOT NULL,
    role_choice           TEXT CHECK (role_choice IS NULL OR role_choice IN ('audio', 'not_a_track')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('describes', 'cleared')),
    sheet_binding_file_id TEXT,
    sheet_disc            TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number     INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (content_hash, relative_path),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((sheet_binding = 'describes') = (sheet_binding_file_id IS NOT NULL)),
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (role_choice IS NOT NULL OR sheet_binding IS NOT NULL OR sheet_disc IS NOT NULL)
) STRICT;


-- What one of the candidate's audio units plays for, as reading it off the
-- disk found. Written by identification and by the pane when it opens a
-- candidate identification never measured; read by the mapping table, so
-- opening a candidate never opens its files again.
--
-- `duration_ms` NULL means the unit was read and states no length — a file
-- that would not probe, or a sheet entry the sheet gives no timing. The
-- absence of a row is the different fact that nothing has read it yet.
CREATE TABLE IF NOT EXISTS import_candidate_file_duration (
    content_hash        TEXT NOT NULL,
    -- 'file': relative_path is the audio file. 'slice': relative_path is
    -- the container; sheet_relative_path and slice_index name the entry.
    kind                TEXT NOT NULL CHECK (kind IN ('file', 'slice')),
    relative_path       TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL DEFAULT '',
    -- The sheet's playable tracks counted from zero — the number the binding
    -- carries, not the number the sheet prints.
    slice_index         INTEGER NOT NULL DEFAULT -1 CHECK (slice_index >= -1),
    duration_ms         INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    PRIMARY KEY (content_hash, kind, relative_path, sheet_relative_path, slice_index),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    -- A STRICT primary key column cannot be NULL, so the two sentinels stand
    -- in for "this kind names no sheet entry"; these keep them and the kind
    -- agreeing.
    CHECK ((kind = 'file') = (sheet_relative_path = '' AND slice_index = -1)),
    CHECK ((kind = 'slice') = (sheet_relative_path <> '' AND slice_index >= 0))
) STRICT;

-- The signals identification settled on for one candidate: the disc ID, the
-- barcode verdict, and the classified text, with their list values in
-- `import_candidate_signal_value`. Stored so the pane and the search sheet
-- read them back after a restart instead of re-extracting them.
--
-- Only a settled value is storable: a `Scanning` barcode or text signal is
-- artwork OCR still running, and a verdict is written only after it finishes.
CREATE TABLE IF NOT EXISTS import_candidate_signals (
    content_hash        TEXT PRIMARY KEY,
    disc_id_state       TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'failed')),
    disc_id             TEXT,
    track_count         INTEGER NOT NULL CHECK (track_count >= 0),
    disc_id_failure     TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    disc_id_failure_status INTEGER,
    disc_id_failure_detail TEXT,
    barcode_state       TEXT NOT NULL CHECK (barcode_state IN ('settled', 'failed', 'absent')),
    barcode_failure     TEXT CHECK (barcode_failure IS NULL OR barcode_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    barcode_failure_status INTEGER,
    barcode_failure_detail TEXT,
    text_state          TEXT NOT NULL CHECK (text_state IN ('settled', 'failed')),
    text_failure        TEXT CHECK (text_failure IS NULL OR text_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    text_failure_status INTEGER,
    text_failure_detail TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((disc_id_state = 'computed') = (disc_id IS NOT NULL)),
    CHECK ((disc_id_state = 'failed') = (disc_id_failure IS NOT NULL)),
    CHECK ((barcode_state = 'failed') = (barcode_failure IS NOT NULL)),
    CHECK ((text_state = 'failed') = (text_failure IS NOT NULL)),
    CHECK (disc_id_failure_status IS NULL OR disc_id_failure = 'provider'),
    CHECK ((disc_id_failure = 'diagnostic') = (disc_id_failure_detail IS NOT NULL)),
    CHECK (barcode_failure_status IS NULL OR barcode_failure = 'provider'),
    CHECK ((barcode_failure = 'diagnostic') = (barcode_failure_detail IS NOT NULL)),
    CHECK (text_failure_status IS NULL OR text_failure = 'provider'),
    CHECK ((text_failure = 'diagnostic') = (text_failure_detail IS NOT NULL))
) STRICT;

-- One value of one of the three lists a settled signal carries, in the order
-- extraction found it. Free text carries no origin; barcodes and catalog
-- numbers do, because their badges say where each came from.
CREATE TABLE IF NOT EXISTS import_candidate_signal_value (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('barcode', 'catalog', 'free_text')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    origin       TEXT CHECK (origin IS NULL OR origin IN ('disc_toc', 'cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file')),
    PRIMARY KEY (content_hash, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    CHECK ((list = 'free_text') = (origin IS NULL))
) STRICT;

-- The last import of this candidate that failed, so the pane still offers
-- Retry after a relaunch. Cleared when an import is queued for the hash;
-- written by the worker when one fails.
CREATE TABLE IF NOT EXISTS import_candidate_failure (
    content_hash TEXT PRIMARY KEY,
    error        TEXT NOT NULL,
    failed_at    TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The cover the user chose for this candidate: one of the folder's own images,
-- or one of the picked release's remote covers. No row means the picked
-- release's default cover stands; a row is written only on an explicit choice.
CREATE TABLE IF NOT EXISTS import_candidate_cover (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local', 'remote')),
    file_id      TEXT,
    url          TEXT,
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'local') = (file_id IS NOT NULL)),
    CHECK ((kind = 'remote') = (url IS NOT NULL AND source IS NOT NULL))
) STRICT;

-- The album-level metadata fields the user typed over the picked release's
-- own. Every column is nullable and NULL means the seed's value stands, so a
-- field nobody touched still commits as untouched; a stored string, including
-- the empty one, is the user's value. `year` is text because the form is text
-- — the commit parses it.
CREATE TABLE IF NOT EXISTS import_candidate_edit (
    content_hash      TEXT PRIMARY KEY,
    album_title       TEXT,
    album_artist_text TEXT,
    year              TEXT,
    format            TEXT,
    label             TEXT,
    catalog_number    TEXT,
    country           TEXT,
    barcode           TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (album_title IS NOT NULL OR album_artist_text IS NOT NULL OR year IS NOT NULL
        OR format IS NOT NULL OR label IS NOT NULL OR catalog_number IS NOT NULL
        OR country IS NOT NULL OR barcode IS NOT NULL)
) STRICT;

-- One mapping-table track row the user changed, whole: a track row is edited
-- as a unit, and `track_number` has no NULL left over to mean "untouched".
-- `dropped = 1` is a row taken out of the import, which then holds nothing
-- else.
CREATE TABLE IF NOT EXISTS import_candidate_track_edit (
    content_hash TEXT NOT NULL,
    -- The row identity the mapping table addresses this track by.
    track_id     TEXT NOT NULL,
    dropped      INTEGER NOT NULL DEFAULT 0 CHECK (dropped IN (0, 1)),
    title        TEXT,
    artist_text  TEXT,
    side         INTEGER,
    track_number INTEGER,
    -- NULL kind: the row has no audio behind it — a track the folder has
    -- nothing for.
    file_kind    TEXT CHECK (file_kind IS NULL OR file_kind IN ('standalone', 'sheet_slice')),
    file_id      TEXT,
    sheet_id     TEXT,
    slice_index  INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((dropped = 1) = (title IS NULL AND artist_text IS NULL AND side IS NULL
        AND track_number IS NULL AND file_kind IS NULL)),
    CHECK ((file_kind IS NOT NULL) = (file_id IS NOT NULL)),
    CHECK ((file_kind = 'sheet_slice') = (sheet_id IS NOT NULL AND slice_index IS NOT NULL))
) STRICT;


-- Device-local watched-root intent. These tables deliberately have no
-- `_updated_at` and are absent from `synced_tables()`.
CREATE TABLE IF NOT EXISTS watched_import_folders (
    path      TEXT PRIMARY KEY,
    position  INTEGER NOT NULL UNIQUE CHECK (position >= 0)
) STRICT;

CREATE TABLE IF NOT EXISTS skipped_import_candidates (
    watched_folder_path    TEXT NOT NULL,
    relative_candidate_path TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, relative_candidate_path),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- Device-local folder interpretation. Paths identify filesystem locations on
-- this device and never enter the synced membership graph.
CREATE TABLE IF NOT EXISTS folder_release_decisions (
    watched_folder_path  TEXT NOT NULL,
    relative_folder_path TEXT NOT NULL,
    decision             TEXT NOT NULL CHECK (
        decision IN ('combine_as_one_release', 'keep_as_separate_releases')
    ),
    -- Who decided. The scan reads a folder its own way when nothing is stored
    -- and records that as 'heuristic'; the user's own answer replaces it as
    -- 'user' and is never read over again.
    author               TEXT NOT NULL CHECK (author IN ('user', 'heuristic')),
    PRIMARY KEY (watched_folder_path, relative_folder_path),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- Device-local cache of the last observed folder scan. A scan generation is
-- durable before traversal begins. Entries are written as they are discovered;
-- successful completion removes entries not seen in that generation in the
-- same transaction that marks the root complete. A failed or interrupted scan
-- keeps both previously known entries and newly discovered entries.
CREATE TABLE IF NOT EXISTS folder_scan_generation_sequence (
    singleton       INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_generation INTEGER NOT NULL CHECK (last_generation >= 0)
) STRICT;

INSERT OR IGNORE INTO folder_scan_generation_sequence (singleton, last_generation)
VALUES (1, 0);

CREATE TABLE IF NOT EXISTS folder_scan_roots (
    watched_folder_path TEXT PRIMARY KEY,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    status              TEXT NOT NULL CHECK (status IN ('scanning', 'complete', 'failed')),
    error               TEXT,
    CHECK (
        (status = 'failed' AND error IS NOT NULL)
        OR
        (status != 'failed' AND error IS NULL)
    ),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;


-- One scanned folder under a watched root. `kind` is what the scan made of
-- it: a release approximation seen before its enclosing boundary was known
-- (tentative), a release (valid), or a folder that failed validation
-- (invalid — carries the reason, no files). Entries are written as they are
-- discovered; successful completion removes entries not seen in that
-- generation in the same transaction that marks the root complete.
CREATE TABLE IF NOT EXISTS scan_candidate (
    watched_folder_path            TEXT NOT NULL,
    path                           TEXT NOT NULL,
    generation                     INTEGER NOT NULL CHECK (generation >= 0),
    kind                           TEXT NOT NULL CHECK (kind IN ('tentative', 'valid', 'invalid')),
    name                           TEXT NOT NULL,
    display_path                   TEXT NOT NULL,
    file_root                      TEXT,
    scope                          TEXT CHECK (scope IS NULL OR scope IN ('direct', 'recursive')),
    content_hash                   TEXT,
    file_edit_revision             INTEGER NOT NULL DEFAULT 0 CHECK (file_edit_revision >= 0),
    format_label                   TEXT,
    combine_ancestor_relative_path TEXT,
    invalid_reason                 TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path            TEXT,
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((kind = 'invalid') = (file_root IS NULL AND scope IS NULL AND content_hash IS NULL AND format_label IS NULL)),
    CHECK ((invalid_reason IN ('corrupt_audio', 'corrupt_image')) = (invalid_reason_path IS NOT NULL))
) STRICT;

-- Every file under a candidate's root, in relative_path order, with the
-- role the scan proposed (and the user's decisions applied). A track sheet
-- carries what its FILE directive resolved to and which disc it is.
CREATE TABLE IF NOT EXISTS scan_candidate_file (
    watched_folder_path   TEXT NOT NULL,
    candidate_path        TEXT NOT NULL,
    relative_path         TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    absolute_path         TEXT NOT NULL,
    size                  INTEGER NOT NULL CHECK (size >= 0),
    file_name             TEXT NOT NULL,
    dir_prefix            TEXT,
    proposed_audio        INTEGER NOT NULL CHECK (proposed_audio IN (0, 1)),
    role                  TEXT NOT NULL CHECK (role IN ('audio', 'track_sheet', 'artwork', 'document', 'other')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('describes', 'unresolved', 'refused_codec')),
    sheet_binding_file_id TEXT,
    sheet_binding_codec   TEXT,
    sheet_disc            TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number     INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path) REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK ((role = 'track_sheet') = (sheet_binding IS NOT NULL AND sheet_disc IS NOT NULL)),
    CHECK ((sheet_binding IN ('describes', 'refused_codec')) = (sheet_binding_file_id IS NOT NULL)),
    CHECK ((sheet_binding = 'refused_codec') = (sheet_binding_codec IS NOT NULL)),
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL))
) STRICT;

-- The parsed sheet behind a track-sheet file.
CREATE TABLE IF NOT EXISTS scan_cue_sheet (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    title               TEXT,
    performer           TEXT,
    catalog             TEXT,
    date                TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS scan_cue_track (
    watched_folder_path         TEXT NOT NULL,
    candidate_path              TEXT NOT NULL,
    sheet_relative_path         TEXT NOT NULL,
    position                    INTEGER NOT NULL CHECK (position >= 0),
    number                      INTEGER NOT NULL,
    mode                        TEXT NOT NULL CHECK (mode IN ('audio', 'other')),
    mode_other                  TEXT,
    title                       TEXT,
    performer                   TEXT,
    file_reference              TEXT NOT NULL,
    start_cue_frames            INTEGER NOT NULL CHECK (start_cue_frames >= 0),
    end_cue_frames              INTEGER CHECK (end_cue_frames IS NULL OR end_cue_frames >= 0),
    pregap_kind                 TEXT NOT NULL CHECK (pregap_kind IN ('none', 'audio', 'silence')),
    pregap_frames               INTEGER CHECK (pregap_frames IS NULL OR pregap_frames >= 0),
    pregap_index_number         INTEGER,
    pregap_index_file_reference TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_cue_sheet (watched_folder_path, candidate_path, sheet_relative_path) ON DELETE CASCADE,
    CHECK ((mode = 'other') = (mode_other IS NOT NULL)),
    CHECK ((pregap_kind = 'none') = (pregap_frames IS NULL)),
    CHECK ((pregap_kind = 'audio') = (pregap_index_number IS NOT NULL AND pregap_index_file_reference IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS scan_cue_index (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    track_position      INTEGER NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    number              INTEGER NOT NULL,
    frames              INTEGER NOT NULL CHECK (frames >= 0),
    file_reference      TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, track_position, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path, track_position)
        REFERENCES scan_cue_track (watched_folder_path, candidate_path, sheet_relative_path, position) ON DELETE CASCADE
) STRICT;

-- The explicit boundary decisions that exposed a candidate, retained so its
-- context menu can set the opposite interpretation.
CREATE TABLE IF NOT EXISTS scan_candidate_resolved_boundary (
    watched_folder_path  TEXT NOT NULL,
    candidate_path       TEXT NOT NULL,
    position             INTEGER NOT NULL CHECK (position >= 0),
    relative_folder_path TEXT NOT NULL,
    decision             TEXT NOT NULL CHECK (decision IN ('combine_as_one_release', 'keep_as_separate_releases')),
    name                 TEXT NOT NULL,
    display_path         TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path) REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

-- A folder whose structure admits both one recursive release and several
-- direct ones: a question for the user, with the compact tree the pane shows
-- and the tentative candidates it hides.
CREATE TABLE IF NOT EXISTS scan_boundary (
    watched_folder_path  TEXT NOT NULL,
    relative_folder_path TEXT NOT NULL,
    generation           INTEGER NOT NULL CHECK (generation >= 0),
    name                 TEXT NOT NULL,
    display_path         TEXT NOT NULL,
    shared_file_count    INTEGER NOT NULL CHECK (shared_file_count >= 0),
    PRIMARY KEY (watched_folder_path, relative_folder_path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS scan_boundary_tree_row (
    watched_folder_path           TEXT NOT NULL,
    boundary_relative_folder_path TEXT NOT NULL,
    position                      INTEGER NOT NULL CHECK (position >= 0),
    name                          TEXT NOT NULL,
    display_path                  TEXT NOT NULL,
    depth                         INTEGER NOT NULL CHECK (depth >= 0),
    kind                          TEXT NOT NULL CHECK (kind IN ('folder', 'candidate', 'invalid')),
    track_count                   INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    format_label                  TEXT,
    invalid_reason                TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path           TEXT,
    decision_relative_folder_path TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, boundary_relative_folder_path, position),
    FOREIGN KEY (watched_folder_path, boundary_relative_folder_path)
        REFERENCES scan_boundary (watched_folder_path, relative_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'candidate') = (track_count IS NOT NULL AND format_label IS NOT NULL)),
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((invalid_reason IN ('corrupt_audio', 'corrupt_image')) = (invalid_reason_path IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS scan_boundary_tree_row_ancestor (
    watched_folder_path           TEXT NOT NULL,
    boundary_relative_folder_path TEXT NOT NULL,
    row_position                  INTEGER NOT NULL,
    position                      INTEGER NOT NULL CHECK (position >= 0),
    ancestor_relative_folder_path TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, boundary_relative_folder_path, row_position, position),
    FOREIGN KEY (watched_folder_path, boundary_relative_folder_path, row_position)
        REFERENCES scan_boundary_tree_row (watched_folder_path, boundary_relative_folder_path, position) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS scan_boundary_hidden_candidate (
    watched_folder_path           TEXT NOT NULL,
    boundary_relative_folder_path TEXT NOT NULL,
    position                      INTEGER NOT NULL CHECK (position >= 0),
    candidate_path                TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, boundary_relative_folder_path, position),
    FOREIGN KEY (watched_folder_path, boundary_relative_folder_path)
        REFERENCES scan_boundary (watched_folder_path, relative_folder_path) ON DELETE CASCADE
) STRICT;

-- A candidate is addressed by its path alone (the key a selection carries)
-- and gathered by its content hash (the file decision that reshapes every
-- copy of one release). Neither is the leading column of the primary key.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_path ON scan_candidate (path);
CREATE INDEX IF NOT EXISTS idx_scan_candidate_content_hash
    ON scan_candidate (content_hash) WHERE content_hash IS NOT NULL;
