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

-- Per-source identity rows. A release has 0+ rows: 0 = Unknown, 1+ = identified
-- (each row is independently Exact when source_release_id is set, Approximate
-- when NULL). Cross-source equivalences are encoded by a release having rows
-- in multiple sources.
CREATE TABLE IF NOT EXISTS release_identities (
    id                TEXT PRIMARY KEY,
    release_id        TEXT NOT NULL,
    source            TEXT NOT NULL,
    source_group_id   TEXT NOT NULL,
    source_release_id TEXT,
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
    --   'cover_art_archive_release'       the archive's answer for a release
    --   'cover_art_archive_release_group' the archive's answer for a group
    source TEXT NOT NULL,
    source_release_id TEXT NOT NULL,
    -- The document as the provider returned it, except for the two cover-art
    -- sources: the archive's ordinary "no cover" is a 404 with no body to keep,
    -- so those rows hold the selected cover, or JSON `null` when it has none.
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
    ON release_identities (source, source_release_id)
    WHERE source_release_id IS NOT NULL;
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
-- Either can be present without the other — a binding can be set before
-- anything has identified the candidate, and identification writes rows for
-- candidates nobody has touched.
CREATE TABLE IF NOT EXISTS import_candidate_state (
    content_hash             TEXT PRIMARY KEY,
    -- Where the candidate was last seen. Not identity (the hash is), and not
    -- authoritative for anything; retained so diagnostics can name the folder
    -- whose derived state failed to decode or match its revision.
    folder_path              TEXT NOT NULL,
    -- The identify result: `verdict` / `probed_total_duration_ms` /
    -- `identified_at` are written and cleared as one group, so they are all
    -- NULL together or all set together.
    --
    -- They are set only once identification reached a terminal verdict; a
    -- transport failure (network down, a provider error, cancellation) writes
    -- nothing, so the candidate is retried on the next sweep rather than
    -- remembered as failed. They are cleared when the user changes a sheet
    -- binding or takes a file out of the tracklist: either changes what the
    -- folder is — a one-track image becomes a twelve-track disc, and its disc
    -- ID becomes computable — so the stored verdict describes a shape that no
    -- longer applies, and clearing it is what makes the queue identify the
    -- candidate again.
    --
    -- `verdict` is `identify::TerminalVerdict`, JSON-encoded by the identify
    -- module that owns the type. Not normalized into columns: the whole set is
    -- a few hundred rows, read in full and classified in Rust.
    --
    -- A verdict naming a single match whose `source_tracks` is set is one whose
    -- lead was settled: the writer buys that release's documents into
    -- `source_release_payloads` and reads the tracklist out of them before it
    -- writes this row, so the two land in that order or neither lands. That is
    -- what lets the confirmation pane open such a candidate with no network.
    verdict                  TEXT CHECK (verdict IS NULL OR json_valid(verdict)),
    -- Sum of the probed durations of the candidate's audio files, in
    -- milliseconds. Per-file durations are not kept here — the Ready gate
    -- only ever compares totals.
    probed_total_duration_ms INTEGER,
    identified_at            TEXT,
    -- `folder_scanner::SheetBindingEdits`, JSON-encoded: which audio file the
    -- user bound each track sheet to, and which sheets they cleared. `{}` when
    -- they have decided nothing, which is not the same as clearing — a sheet
    -- absent from the map keeps the scan's proposal.
    sheet_bindings           TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(sheet_bindings)),
    -- `folder_scanner::FileRoleEdits`, JSON-encoded: which of the candidate's
    -- audio files the user took out of the tracklist, and which they put back.
    -- `{}` when they have decided nothing — a file absent from the map keeps
    -- the scan's proposal. Only files the scan read as audio can appear, so a
    -- decision never survives the file it names ceasing to be audio.
    file_roles               TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(file_roles)),
    -- `folder_scanner::SheetDiscEdits`, JSON-encoded: which disc of the release
    -- each track sheet's entries become, and which sheets the user took out of
    -- the tracklist entirely. `{}` when they have decided nothing — a sheet
    -- absent from the map takes its position among the folder's bound sheets.
    -- Cue filenames are arbitrary, so this is the only thing that says which
    -- cue is which disc.
    sheet_discs              TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(sheet_discs)),
    -- `import::IdentityPick`, JSON-encoded: the identity decided for this
    -- candidate — the pressing picked, the decision to read the folder's own
    -- tags, or the single match identification settled (which fills a blank
    -- and never overwrites a person's choice). What lets an answered pane
    -- reopen answered after a restart. Survives file decisions: the choice
    -- names a release, not a shape, and the mapping re-derives against the
    -- reshaped folder.
    identity_pick            TEXT CHECK (identity_pick IS NULL OR json_valid(identity_pick)),
    -- Monotonic compare-and-set revision for file decisions. Identification
    -- writes carry the revision they observed and cannot overwrite a verdict
    -- cleared by a later edit.
    edit_revision            INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0),
    CHECK (
        (
            verdict IS NULL
            AND probed_total_duration_ms IS NULL
            AND identified_at IS NULL
        )
        OR
        (
            verdict IS NOT NULL
            AND probed_total_duration_ms IS NOT NULL
            AND probed_total_duration_ms >= 0
            AND identified_at IS NOT NULL
        )
    )
);

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

CREATE TABLE IF NOT EXISTS folder_scan_entries (
    watched_folder_path TEXT NOT NULL,
    entry_key           TEXT NOT NULL,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    item                TEXT NOT NULL CHECK (json_valid(item)),
    PRIMARY KEY (watched_folder_path, entry_key),
    FOREIGN KEY (watched_folder_path)
        REFERENCES folder_scan_roots (watched_folder_path)
        ON DELETE CASCADE
) STRICT;
