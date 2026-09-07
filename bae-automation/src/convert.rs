use super::*;

mod candidates;
mod identify;

pub(crate) use candidates::*;
pub(crate) use identify::*;

pub(super) fn expect_no_args(args: Value, tool_name: &str) -> Result<(), AutomationError> {
    match args {
        Value::Null => Ok(()),
        Value::Object(map) if map.is_empty() => Ok(()),
        other => Err(AutomationError::validation(format!(
            "tool '{tool_name}' does not accept arguments, got {other}"
        ))),
    }
}

pub(super) fn from_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AutomationError> {
    serde_json::from_value(value).map_err(|e| AutomationError::validation(e.to_string()))
}

pub(super) fn to_value<T: Serialize>(value: T) -> Result<Value, AutomationError> {
    serde_json::to_value(value).map_err(|e| AutomationError::internal(e.to_string()))
}

/// Wrap a list result under a named key. MCP `structuredContent` must be a JSON
/// object, so a tool returning a bare `Vec` has to nest it under a field.
pub(super) fn to_list_value<T: Serialize>(
    key: &str,
    values: Vec<T>,
) -> Result<Value, AutomationError> {
    let mut map = Map::new();
    map.insert(key.to_string(), to_value(values)?);
    Ok(Value::Object(map))
}

pub(super) fn schema_object<T: JsonSchema>() -> Map<String, Value> {
    let value = serde_json::to_value(schemars::schema_for!(T)).expect("serialize JSON schema");
    let mut map = match value {
        Value::Object(map) => map,
        _ => unreachable!("JSON schema is an object"),
    };
    // MCP requires the root inputSchema to declare `type: "object"`. Struct
    // schemas already do; internally-tagged enum schemas emit a root `oneOf`
    // with no root type. Every automation tool input is an object in all
    // variants, so assert it at the root.
    map.entry("type".to_string())
        .or_insert_with(|| Value::String("object".to_string()));
    map
}

pub(super) fn empty_input_schema() -> Map<String, Value> {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("properties".to_string(), Value::Object(Map::new()));
    schema
}

pub(super) fn automation_output_snapshot(
    snapshot: bae_core::library::OutputSnapshot,
) -> AutomationOutputSnapshot {
    use bae_core::library::{OutputKind, OutputState};
    let outputs = snapshot
        .ops
        .into_iter()
        .map(|op| {
            let state = match op.state {
                OutputState::Queued => AutomationOutputState::Queued,
                OutputState::Active { progress } => {
                    AutomationOutputState::Active { percent: progress }
                }
                OutputState::Failed { error } => AutomationOutputState::Failed { error },
            };
            let kind = match op.payload.kind {
                OutputKind::Export => AutomationOutputKind::Export,
                OutputKind::Save { preset } => AutomationOutputKind::Save {
                    preset_name: preset.name,
                },
            };
            AutomationOutputOp {
                release_id: op.release_id,
                target_dir: op.payload.target_dir.to_string_lossy().to_string(),
                title: op.title,
                file_count: op.file_count,
                total_size: op.total_size,
                created_at: op.created_at,
                state,
                kind,
            }
        })
        .collect();
    AutomationOutputSnapshot {
        outputs,
        total: AutomationOutputProgress {
            queued: snapshot.total.queued,
            active: snapshot.total.active,
            failed: snapshot.total.failed,
        },
        paused: snapshot.paused,
    }
}

/// Refuse a transition the release does not currently offer, naming what core
/// says it does offer. The available set is core's — read off the release — so
/// this decides nothing, it only declines to call a transfer the desktop would
/// not have offered either.
pub(super) fn require_action(
    summary: &AutomationReleaseSummary,
    action: AutomationReleaseStorageAction,
    requested: &str,
) -> Result<(), AutomationError> {
    if summary.storage_actions.contains(&action) {
        return Ok(());
    }
    let available = summary
        .storage_actions
        .iter()
        .map(storage_action_name)
        .collect::<Vec<_>>();
    let available = if available.is_empty() {
        "none (this library has no cloud home)".to_string()
    } else {
        available.join(", ")
    };
    Err(AutomationError::validation(format!(
        "release '{}' cannot {requested} right now; available: {available}",
        summary.id
    )))
}

/// The request name for a transition core reports as available, so a refusal
/// lists what a caller may actually ask for.
fn storage_action_name(action: &AutomationReleaseStorageAction) -> &'static str {
    match action {
        AutomationReleaseStorageAction::MakeRemote => "move_to_cloud",
        AutomationReleaseStorageAction::Pin => "pin",
        AutomationReleaseStorageAction::Unpin => "unpin",
        AutomationReleaseStorageAction::MakeLocal => "make_local",
    }
}

/// The typed query and the one source to ask it of. The query itself no longer
/// names a source — every configured provider answers a person's search — so an
/// automation client's single-source request splits into the two arguments the
/// one-shot search takes.
pub(super) fn search_query(query: AutomationSearchQuery) -> (SearchQuery, MetadataSource) {
    match query {
        AutomationSearchQuery::General {
            artist,
            album,
            source,
        } => (SearchQuery::General { artist, album }, source.into()),
        AutomationSearchQuery::CatalogNumber {
            catalog_number,
            source,
        } => (SearchQuery::CatalogNumber { catalog_number }, source.into()),
        AutomationSearchQuery::Barcode { barcode, source } => {
            (SearchQuery::Barcode { barcode }, source.into())
        }
    }
}

/// Not a copy: core's `ExternalRelease` carries the source and release id as
/// one `MetadataRef`, which the automation shape spells as two fields.
pub(super) fn release_reseed(choice: AutomationReleaseReseed) -> ReleaseReseed {
    match choice {
        AutomationReleaseReseed::ExternalRelease {
            source,
            release_id,
            partners,
        } => ReleaseReseed::ExternalRelease {
            release_ref: MetadataRef::new(release_id, source.into()),
            partners: partners
                .into_iter()
                .map(AutomationMetadataRef::into_core)
                .collect(),
        },
        AutomationReleaseReseed::FileTags => ReleaseReseed::FileTags,
    }
}

impl AutomationMetadataRef {
    /// Not a copy: core names the release id `id`.
    pub(crate) fn from_core(release_ref: MetadataRef) -> Self {
        Self {
            source: release_ref.source.into(),
            release_id: release_ref.id,
        }
    }

    pub(crate) fn into_core(self) -> MetadataRef {
        MetadataRef::new(self.release_id, self.source.into())
    }
}

mirror_enum! {
    AutomationMetadataProvenance = MetadataProvenance,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        ExternalRelease {
            source: (into),
            release_id,
            partners: (each AutomationMetadataRef),
        },
        FileTags,
    },
}

mirror_enum! {
    AutomationCandidateEditField = CandidateEditField,
    into_core: pub(crate) fn,
    variants: {
        AlbumTitle,
        AlbumYear,
        PressingYear,
        Format,
        Label,
        CatalogNumber,
        Country,
        Barcode,
    },
}

mirror_enum! {
    AutomationEvidenceSignal = bae_core::import::EvidenceSignal,
    from_core: pub(crate) fn,
    variants: { Barcode, DiscId },
}

mirror_struct! {
    AutomationFileEvidence = bae_core::import::FileEvidence,
    from_core: pub(crate) fn,
    fields: {
        signal: (AutomationEvidenceSignal),
        value,
        file_id,
    },
}

mirror_struct! {
    AutomationSearchResults = GroupedSearchResults,
    from_core: pub(crate) fn,
    fields: {
        groups: (each AutomationReleaseGroup),
        statuses: (each AutomationLibraryStatus),
    },
}

mirror_struct! {
    AutomationReleaseGroupSource = bae_core::import::release_group::ReleaseGroupSource,
    from_core: pub(crate) fn,
    fields: { source: (into), group_url },
}

impl AutomationPressing {
    /// Not a copy: `pick` is what core derives from the row's releases — what
    /// picking the row claims — rather than a field it stores.
    pub(crate) fn from_core(pressing: bae_core::import::release_group::Pressing) -> Self {
        Self {
            pick: AutomationMetadataProvenance::from_core(pressing.pick()),
            releases: pressing
                .releases
                .into_iter()
                .map(AutomationMetadataResult::from_core)
                .collect(),
        }
    }
}

mirror_struct! {
    AutomationReleaseGroup = ReleaseGroup,
    from_core: pub(crate) fn,
    fields: {
        id,
        title,
        artist,
        label,
        cover_art: (opt AutomationRemoteCover),
        sources: (each AutomationReleaseGroupSource),
        year_min,
        year_max,
        pressings: (each AutomationPressing),
    },
}

impl AutomationMetadataResult {
    /// Not a copy: core's `source_tracks` is the settle marker for a stored
    /// verdict, not something an MCP client reads.
    pub(crate) fn from_core(result: MetadataResult) -> Self {
        Self {
            source: result.source.into(),
            release_id: result.release_id,
            title: result.title,
            artist: result.artist,
            year: result.year,
            format: result.format,
            label: result.label,
            catalog_number: result.catalog_number,
            country: result.country,
            barcode: result.barcode,
            cover_art: result.cover_art.map(AutomationRemoteCover::from_core),
            source_group_id: result.source_group_id,
        }
    }
}

mirror_struct! {
    AutomationLibraryStatus = LibraryStatus,
    from_core: pub(crate) fn,
    fields: {
        release_id,
        release_in_library,
        album_in_library,
        album_title,
        album_id,
    },
}

mirror_struct! {
    AutomationRemoteCover = RemoteCover,
    from_core: pub(crate) fn,
    fields: { url, thumbnail_url, label, source: (into) },
}

mirror_struct! {
    AutomationReleaseTrack = bae_core::import::search::ReleaseTrack,
    from_core: pub(crate) fn,
    fields: { title, artist, duration_ms, position, side },
}

mirror_struct! {
    AutomationReleaseDetail = ImportSearchReleaseDetail,
    from_core: pub(crate) fn,
    fields: {
        release_id,
        source: (into),
        source_group_id,
        title,
        artist,
        year,
        format,
        label,
        catalog_number,
        country,
        barcode,
        track_count,
        tracks: (each AutomationReleaseTrack),
        cover_art: (each AutomationRemoteCover),
    },
}

mirror_struct! {
    AutomationPressingEdit = PressingEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { year, format, label, catalog_number, country, barcode },
}

mirror_struct! {
    AutomationReleaseUserEdit = bae_core::import::ReleaseUserEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        album_title,
        album_artist_assignments: (each AutomationArtistAssignment),
        album_year,
        pressing: (AutomationPressingEdit),
        tracks: (each AutomationTrackUserEdit),
    },
}

impl AutomationTrackUserEdit {
    /// Not a copy: core's `file` says which of the folder's audio backs the
    /// track, which automation neither reads nor sets.
    pub(crate) fn from_core(track: TrackUserEdit) -> Self {
        Self {
            title: track.title,
            side: track.side,
            track_number: track.track_number,
            artist_assignments: AutomationTrackArtistAssignments::from_core(
                track.artist_assignments,
            ),
        }
    }

    pub(crate) fn into_core(self) -> TrackUserEdit {
        TrackUserEdit {
            title: self.title,
            side: self.side,
            track_number: self.track_number,
            artist_assignments: self.artist_assignments.into_core(),
            // Automation edits a release's metadata, never which of the
            // folder's audio backs each track; an import it starts gets the
            // track slots the folder and the tracklist produce.
            file: None,
        }
    }
}

mirror_struct! {
    AutomationExistingArtist = bae_core::import::ExistingArtist,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        artist_id,
        name,
        sort_name,
        musicbrainz_artist_id,
        discogs_artist_id,
    },
}

mirror_struct! {
    AutomationNewArtistSeed = bae_core::import::NewArtistSeed,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { name, sort_name, musicbrainz_artist_id, discogs_artist_id },
}

mirror_enum! {
    AutomationArtistAssignment = bae_core::import::ArtistAssignment,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Existing { artist: (AutomationExistingArtist) },
        New { seed: (AutomationNewArtistSeed) },
    },
}

mirror_enum! {
    AutomationTrackArtistAssignments = bae_core::import::TrackArtistAssignments,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        AlbumArtists,
        Explicit(assignments: (each AutomationArtistAssignment)),
    },
}

/// Not a copy: core's `Remote` carries the URL and the source as two unnamed
/// payloads, which the automation shape names.
pub(super) fn cover_selection(selection: AutomationCoverSelection) -> CoverSelection {
    match selection {
        AutomationCoverSelection::Remote { url, source } => {
            CoverSelection::Remote(url, source.into())
        }
        AutomationCoverSelection::Local { path } => CoverSelection::Local(path),
    }
}

mirror_enum! {
    AutomationStorageMode = StorageMode,
    into_core: pub(crate) fn,
    variants: { Local, Remote },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationLookupFailure = bae_core::signals::LookupFailure,
    from_core: pub(crate) fn,
    variants: {
        Network,
        Provider { status },
        Timeout,
        ArtworkAnalysis,
        Diagnostic { detail },
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSignalOrigin = bae_core::signals::SignalOrigin,
    from_core: pub(crate) fn,
    variants: { DiscToc, CueSheet, Artwork, FolderName, Filename, TextFile },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationImageRegion = bae_core::signals::ImageRegion,
    from_core: pub(crate) fn,
    fields: { x, y, width, height },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSourcedValue = bae_core::signals::SourcedValue,
    from_core: pub(crate) fn,
    fields: {
        value,
        origin: (AutomationSignalOrigin),
        origin_path,
        region: (opt AutomationImageRegion),
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationBarcodeSignal = bae_core::signals::BarcodeSignal,
    from_core: pub(crate) fn,
    variants: {
        Scanning { codes: (each AutomationSourcedValue) },
        Settled { codes: (each AutomationSourcedValue) },
        Failed {
            failure: (AutomationLookupFailure),
            codes: (each AutomationSourcedValue),
        },
        Absent,
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationTextSignal = bae_core::signals::TextSignal,
    from_core: pub(crate) fn,
    variants: {
        Scanning { catalogs: (each AutomationSourcedValue), free_text },
        Settled { catalogs: (each AutomationSourcedValue), free_text },
        Failed {
            failure: (AutomationLookupFailure),
            catalogs: (each AutomationSourcedValue),
            free_text,
        },
    },
}

impl AutomationDiscIdSignal {
    /// Not a copy: core's `Computed` names the LOG or CUE the disc ID came
    /// from, which the automation shape does not carry.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signal: bae_core::signals::DiscIdSignal) -> Self {
        use bae_core::signals::DiscIdSignal;
        match signal {
            DiscIdSignal::Computed {
                disc_id,
                track_count,
                ..
            } => Self::Computed {
                disc_id,
                track_count,
            },
            DiscIdSignal::Absent { track_count } => Self::Absent { track_count },
            DiscIdSignal::Failed {
                failure,
                track_count,
            } => Self::Failed {
                failure: AutomationLookupFailure::from_core(failure),
                track_count,
            },
        }
    }
}

impl AutomationSignals {
    /// Not a copy: core's `durations` are what the Ready rule narrows with, not
    /// a lookup input a client reads.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signals: bae_core::signals::Signals) -> Self {
        Self {
            disc_id: AutomationDiscIdSignal::from_core(signals.disc_id),
            barcode: AutomationBarcodeSignal::from_core(signals.barcode),
            text: AutomationTextSignal::from_core(signals.text),
        }
    }
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSignalKind = bae_core::identify::SignalKind,
    from_core: pub(crate) fn,
    variants: { DiscId, Barcode, Catalog },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSignalState = bae_core::identify::SignalState,
    from_core: pub(crate) fn,
    variants: {
        LookingUp,
        Found { count },
        NoMatch,
        Skipped,
        Failed { failure: (AutomationLookupFailure) },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSignalOption = bae_core::identify::SignalOption,
    from_core: pub(crate) fn,
    fields: { value, origin: (AutomationSignalOrigin), chosen },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationToolbarSignal = bae_core::identify::ToolbarSignal,
    from_core: pub(crate) fn,
    fields: {
        kind: (AutomationSignalKind),
        value,
        origin: (AutomationSignalOrigin),
        state: (AutomationSignalState),
        excluded,
        options: (each AutomationSignalOption),
    },
}

mirror_enum! {
    AutomationPrepareStep = PrepareStep,
    from_core: pub(crate) fn,
    variants: { Queued, ValidatingSourceFiles },
}

mirror_enum! {
    AutomationImportPhase = ImportPhase,
    from_core: pub(crate) fn,
    variants: { ReadingFiles, MeasuringLoudness, Finalizing },
}

impl AutomationRelease {
    /// Not a copy: core's `source_audio` summary belongs to the candidate view,
    /// which reads it off the folder rather than off a stored release.
    pub(crate) fn from_core(release: ReleaseDetail) -> Self {
        Self {
            summary: AutomationReleaseSummary::from_core(release.summary),
            display_name: release.display_name,
            year: release.year,
            label: release.label,
            catalog_number: release.catalog_number,
            country: release.country,
            total_duration_ms: release.total_duration_ms,
            tracks: release
                .tracks
                .into_iter()
                .map(AutomationTrackDetail::from_core)
                .collect(),
            track_groups: release
                .track_groups
                .into_iter()
                .map(AutomationTrackGroup::from_core)
                .collect(),
            files: release
                .files
                .into_iter()
                .map(AutomationFileDetail::from_core)
                .collect(),
            image_files: release
                .image_files
                .into_iter()
                .map(AutomationFileDetail::from_core)
                .collect(),
            gallery_items: release
                .gallery_items
                .into_iter()
                .map(AutomationGalleryItem::from_core)
                .collect(),
        }
    }
}

impl AutomationTrackGroup {
    /// Not a copy: core's per-group `total_duration_ms` is a display total the
    /// automation shape leaves to the client.
    pub(crate) fn from_core(group: bae_core::album_detail::TrackGroup) -> Self {
        Self {
            side: AutomationTrackSide::from_core(group.side),
            tracks: group
                .tracks
                .into_iter()
                .map(AutomationTrackDetail::from_core)
                .collect(),
        }
    }
}

mirror_struct! {
    AutomationReleaseSummary = bae_core::album_detail::ReleaseSummary,
    from_core: pub(crate) fn,
    fields: {
        id,
        album_id,
        format,
        storage_state: (into),
        pinned,
        storage_actions: (each into),
        transfer_action: (opt into),
        file_count,
        total_size,
        cover: (opt AutomationImageRef),
    },
}

impl AutomationImageRef {
    /// Not a copy: core's `image_type` says which image table the id lives in,
    /// which the automation fetch does not take.
    pub(crate) fn from_core(image: ImageRef) -> Self {
        Self {
            id: image.id,
            version: image.version,
        }
    }
}

impl AutomationTrackDetail {
    /// Not a copy: core's `display_artist` is the row-label decision for a
    /// compilation, which is a rendering call rather than a fact.
    pub(crate) fn from_core(track: TrackDetail) -> Self {
        Self {
            id: track.id,
            title: track.title,
            side: track.side,
            track_number: track.track_number,
            duration_ms: track.duration_ms,
            artist_names: track.artist_names,
            position_text: track.position_text,
            position: AutomationTrackPosition::from_core(track.position),
        }
    }
}

mirror_enum! {
    AutomationTrackPosition = TrackPosition,
    from_core: pub(crate) fn,
    variants: {
        Sided { side_letter, number },
        SidedUnnumbered { side_letter },
        Disc { disc, number },
        DiscUnnumbered { disc },
        Flat { number },
        Unnumbered,
    },
}

mirror_enum! {
    AutomationTrackSide = TrackSide,
    from_core: pub(crate) fn,
    variants: {
        Sided { side_letter },
        Disc { disc },
        Flat,
    },
}

impl AutomationFileDetail {
    /// Not a copy: the automation shape carries the source file's format
    /// directly, where core nests it under the whole scan record.
    pub(crate) fn from_core(file: FileDetail) -> Self {
        Self {
            id: file.id,
            original_filename: file.original_filename,
            file_size: file.file_size,
            is_image: file.is_image,
            content_type: file.content_type,
            audio_format: file
                .source_audio
                .map(|source_audio| AutomationAudioFormat::from_core(source_audio.format)),
        }
    }
}

mirror_struct! {
    AutomationAudioFormat = AudioFormat,
    from_core: pub(crate) fn,
    fields: {
        codec,
        sample_rate_hz,
        bits_per_sample,
        bitrate_kbps,
        channels,
    },
}

mirror_enum! {
    AutomationGallerySource = GallerySource,
    from_core: pub(crate) fn,
    variants: {
        Cover(image: (AutomationImageRef)),
        ReleaseFile { file_id },
    },
}

mirror_struct! {
    AutomationGalleryItem = GalleryItem,
    from_core: pub(crate) fn,
    fields: { id, label, source: (AutomationGallerySource) },
}

impl AutomationLibrarySearchResults {
    /// Not a copy: core's search also answers with artists, composers and
    /// works, which the automation surface does not expose.
    pub(crate) fn from_core(results: SearchResults) -> Self {
        Self {
            albums: results
                .albums
                .into_iter()
                .map(AutomationAlbumSearchResult::from_core)
                .collect(),
            tracks: results
                .tracks
                .into_iter()
                .map(AutomationTrackSearchResult::from_core)
                .collect(),
        }
    }
}

mirror_struct! {
    AutomationAlbumSearchResult = bae_core::album_detail::AlbumSearchResult,
    from_core: pub(crate) fn,
    fields: { id, title, year, artist_name, cover: (opt AutomationImageRef) },
}

impl AutomationTrackSearchResult {
    /// Not a copy: a track hit's cover is its release's, which the album hit
    /// beside it already carries.
    pub(crate) fn from_core(track: bae_core::album_detail::TrackSearchResult) -> Self {
        Self {
            id: track.id,
            title: track.title,
            duration_ms: track.duration_ms,
            album_id: track.album_id,
            album_title: track.album_title,
            artist_name: track.artist_name,
        }
    }
}
