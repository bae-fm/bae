use super::*;

mod candidates;

pub(crate) use candidates::*;

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

pub(super) fn search_query(query: AutomationSearchQuery) -> SearchQuery {
    match query {
        AutomationSearchQuery::General {
            artist,
            album,
            source,
        } => SearchQuery::General {
            artist,
            album,
            sources: bae_core::import::SearchSources::One(source.into()),
        },
        AutomationSearchQuery::CatalogNumber {
            catalog_number,
            source,
        } => SearchQuery::CatalogNumber {
            catalog_number,
            sources: bae_core::import::SearchSources::One(source.into()),
        },
        AutomationSearchQuery::Barcode { barcode, source } => SearchQuery::Barcode {
            barcode,
            sources: bae_core::import::SearchSources::One(source.into()),
        },
    }
}

pub(super) fn release_reseed(choice: AutomationReleaseReseed) -> ReleaseReseed {
    match choice {
        AutomationReleaseReseed::ExternalRelease { source, release_id } => {
            ReleaseReseed::ExternalRelease {
                release_ref: MetadataRef::new(release_id, source.into()),
            }
        }
        AutomationReleaseReseed::FileTags => ReleaseReseed::FileTags,
    }
}

pub(super) fn metadata_provenance(provenance: AutomationMetadataProvenance) -> MetadataProvenance {
    match provenance {
        AutomationMetadataProvenance::ExternalRelease { source, release_id } => {
            MetadataProvenance::ExternalRelease {
                source: source.into(),
                release_id,
            }
        }
        AutomationMetadataProvenance::FileTags => MetadataProvenance::FileTags,
    }
}

pub(super) fn candidate_edit_field(field: AutomationCandidateEditField) -> CandidateEditField {
    match field {
        AutomationCandidateEditField::AlbumTitle => CandidateEditField::AlbumTitle,
        AutomationCandidateEditField::Year => CandidateEditField::Year,
        AutomationCandidateEditField::Format => CandidateEditField::Format,
        AutomationCandidateEditField::Label => CandidateEditField::Label,
        AutomationCandidateEditField::CatalogNumber => CandidateEditField::CatalogNumber,
        AutomationCandidateEditField::Country => CandidateEditField::Country,
        AutomationCandidateEditField::Barcode => CandidateEditField::Barcode,
    }
}

pub(super) fn automation_file_evidence(
    evidence: bae_core::import::FileEvidence,
) -> AutomationFileEvidence {
    let bae_core::import::FileEvidence {
        signal,
        value,
        file_id,
    } = evidence;
    AutomationFileEvidence {
        signal: match signal {
            bae_core::import::EvidenceSignal::Barcode => AutomationEvidenceSignal::Barcode,
            bae_core::import::EvidenceSignal::DiscId => AutomationEvidenceSignal::DiscId,
        },
        value,
        file_id,
    }
}

pub(super) fn automation_search_results(results: GroupedSearchResults) -> AutomationSearchResults {
    AutomationSearchResults {
        groups: results
            .groups
            .into_iter()
            .map(automation_release_group)
            .collect(),
        statuses: results
            .statuses
            .into_iter()
            .map(automation_library_status)
            .collect(),
    }
}

pub(super) fn automation_release_group(group: ReleaseGroup) -> AutomationReleaseGroup {
    AutomationReleaseGroup {
        id: group.id,
        title: group.title,
        artist: group.artist,
        cover_art: group.cover_art.map(automation_remote_cover),
        source_label: group.source_label,
        group_url: group.group_url,
        year_min: group.year_min,
        year_max: group.year_max,
        pressings: group
            .pressings
            .into_iter()
            .map(automation_metadata_result)
            .collect(),
    }
}

pub(super) fn automation_metadata_result(result: MetadataResult) -> AutomationMetadataResult {
    AutomationMetadataResult {
        source: result.source.into(),
        release_id: result.release_id,
        title: result.title,
        artist: result.artist,
        year: result.year,
        format: result.format,
        label: result.label,
        catalog_number: result.catalog_number,
        country: result.country,
        cover_art: result.cover_art.map(automation_remote_cover),
        source_group_id: result.source_group_id,
    }
}

pub(super) fn automation_library_status(status: LibraryStatus) -> AutomationLibraryStatus {
    AutomationLibraryStatus {
        release_id: status.release_id,
        release_in_library: status.release_in_library,
        album_in_library: status.album_in_library,
        album_title: status.album_title,
        album_id: status.album_id,
    }
}

pub(super) fn automation_remote_cover(cover: RemoteCover) -> AutomationRemoteCover {
    AutomationRemoteCover {
        url: cover.url,
        thumbnail_url: cover.thumbnail_url,
        label: cover.label,
        source: cover.source.into(),
    }
}

pub(super) fn automation_release_detail(
    detail: ImportSearchReleaseDetail,
) -> AutomationReleaseDetail {
    AutomationReleaseDetail {
        release_id: detail.release_id,
        source: detail.source.into(),
        source_group_id: detail.source_group_id,
        title: detail.title,
        artist: detail.artist,
        year: detail.year,
        format: detail.format,
        label: detail.label,
        catalog_number: detail.catalog_number,
        country: detail.country,
        barcode: detail.barcode,
        track_count: detail.track_count,
        tracks: detail
            .tracks
            .into_iter()
            .map(|track| AutomationReleaseTrack {
                title: track.title,
                artist: track.artist,
                duration_ms: track.duration_ms,
                position: track.position,
                side: track.side,
            })
            .collect(),
        cover_art: detail
            .cover_art
            .into_iter()
            .map(automation_remote_cover)
            .collect(),
    }
}

pub(super) fn automation_release_user_edit(
    edit: bae_core::import::ReleaseUserEdit,
) -> AutomationReleaseUserEdit {
    AutomationReleaseUserEdit {
        album_title: edit.album_title,
        album_artist_assignments: edit
            .album_artist_assignments
            .into_iter()
            .map(automation_artist_assignment)
            .collect(),
        pressing: AutomationPressingEdit {
            year: edit.pressing.year,
            format: edit.pressing.format,
            label: edit.pressing.label,
            catalog_number: edit.pressing.catalog_number,
            country: edit.pressing.country,
            barcode: edit.pressing.barcode,
        },
        tracks: edit
            .tracks
            .into_iter()
            .map(|track| AutomationTrackUserEdit {
                title: track.title,
                side: track.side,
                track_number: track.track_number,
                artist_assignments: automation_track_artist_assignments(track.artist_assignments),
            })
            .collect(),
    }
}

pub(super) fn release_user_edit(
    edit: AutomationReleaseUserEdit,
) -> bae_core::import::ReleaseUserEdit {
    bae_core::import::ReleaseUserEdit {
        album_title: edit.album_title,
        album_artist_assignments: edit
            .album_artist_assignments
            .into_iter()
            .map(artist_assignment)
            .collect(),
        pressing: PressingEdit {
            year: edit.pressing.year,
            format: edit.pressing.format,
            label: edit.pressing.label,
            catalog_number: edit.pressing.catalog_number,
            country: edit.pressing.country,
            barcode: edit.pressing.barcode,
        },
        tracks: edit
            .tracks
            .into_iter()
            .map(|track| TrackUserEdit {
                title: track.title,
                side: track.side,
                track_number: track.track_number,
                artist_assignments: track_artist_assignments(track.artist_assignments),
                // Automation edits a release's metadata, never which of the
                // folder's audio backs each track; an import it starts gets the
                // track slots the folder and the tracklist produce.
                file: None,
            })
            .collect(),
    }
}

fn automation_artist_assignment(
    assignment: bae_core::import::ArtistAssignment,
) -> AutomationArtistAssignment {
    match assignment {
        bae_core::import::ArtistAssignment::Existing { artist } => {
            AutomationArtistAssignment::Existing {
                artist: AutomationExistingArtist {
                    artist_id: artist.artist_id,
                    name: artist.name,
                    sort_name: artist.sort_name,
                    musicbrainz_artist_id: artist.musicbrainz_artist_id,
                    discogs_artist_id: artist.discogs_artist_id,
                },
            }
        }
        bae_core::import::ArtistAssignment::New { seed } => AutomationArtistAssignment::New {
            seed: AutomationNewArtistSeed {
                name: seed.name,
                sort_name: seed.sort_name,
                musicbrainz_artist_id: seed.musicbrainz_artist_id,
                discogs_artist_id: seed.discogs_artist_id,
            },
        },
    }
}

fn artist_assignment(assignment: AutomationArtistAssignment) -> bae_core::import::ArtistAssignment {
    match assignment {
        AutomationArtistAssignment::Existing { artist } => {
            bae_core::import::ArtistAssignment::Existing {
                artist: bae_core::import::ExistingArtist {
                    artist_id: artist.artist_id,
                    name: artist.name,
                    sort_name: artist.sort_name,
                    musicbrainz_artist_id: artist.musicbrainz_artist_id,
                    discogs_artist_id: artist.discogs_artist_id,
                },
            }
        }
        AutomationArtistAssignment::New { seed } => bae_core::import::ArtistAssignment::New {
            seed: bae_core::import::NewArtistSeed {
                name: seed.name,
                sort_name: seed.sort_name,
                musicbrainz_artist_id: seed.musicbrainz_artist_id,
                discogs_artist_id: seed.discogs_artist_id,
            },
        },
    }
}

fn automation_track_artist_assignments(
    assignments: bae_core::import::TrackArtistAssignments,
) -> AutomationTrackArtistAssignments {
    match assignments {
        bae_core::import::TrackArtistAssignments::AlbumArtists => {
            AutomationTrackArtistAssignments::AlbumArtists
        }
        bae_core::import::TrackArtistAssignments::Explicit(assignments) => {
            AutomationTrackArtistAssignments::Explicit {
                assignments: assignments
                    .into_iter()
                    .map(automation_artist_assignment)
                    .collect(),
            }
        }
    }
}

fn track_artist_assignments(
    assignments: AutomationTrackArtistAssignments,
) -> bae_core::import::TrackArtistAssignments {
    match assignments {
        AutomationTrackArtistAssignments::AlbumArtists => {
            bae_core::import::TrackArtistAssignments::AlbumArtists
        }
        AutomationTrackArtistAssignments::Explicit { assignments } => {
            bae_core::import::TrackArtistAssignments::Explicit(
                assignments.into_iter().map(artist_assignment).collect(),
            )
        }
    }
}

pub(super) fn cover_selection(selection: AutomationCoverSelection) -> CoverSelection {
    match selection {
        AutomationCoverSelection::Remote { url, source } => {
            CoverSelection::Remote(url, source.into())
        }
        AutomationCoverSelection::Local { path } => CoverSelection::Local(path),
    }
}

pub(super) fn storage_mode(mode: AutomationStorageMode) -> StorageMode {
    match mode {
        AutomationStorageMode::Local => StorageMode::Local,
        AutomationStorageMode::Remote => StorageMode::Remote,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_lookup_failure(
    failure: bae_core::signals::LookupFailure,
) -> AutomationLookupFailure {
    use bae_core::signals::LookupFailure;
    match failure {
        LookupFailure::Network => AutomationLookupFailure::Network,
        LookupFailure::Provider { status } => AutomationLookupFailure::Provider { status },
        LookupFailure::Timeout => AutomationLookupFailure::Timeout,
        LookupFailure::ArtworkAnalysis => AutomationLookupFailure::ArtworkAnalysis,
        LookupFailure::Diagnostic { detail } => AutomationLookupFailure::Diagnostic { detail },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_signal_origin(
    origin: bae_core::signals::SignalOrigin,
) -> AutomationSignalOrigin {
    use bae_core::signals::SignalOrigin;
    match origin {
        SignalOrigin::DiscToc => AutomationSignalOrigin::DiscToc,
        SignalOrigin::CueSheet => AutomationSignalOrigin::CueSheet,
        SignalOrigin::Artwork => AutomationSignalOrigin::Artwork,
        SignalOrigin::FolderName => AutomationSignalOrigin::FolderName,
        SignalOrigin::Filename => AutomationSignalOrigin::Filename,
        SignalOrigin::TextFile => AutomationSignalOrigin::TextFile,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_sourced_value(
    value: bae_core::signals::SourcedValue,
) -> AutomationSourcedValue {
    AutomationSourcedValue {
        value: value.value,
        origin: automation_signal_origin(value.origin),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_sourced_values(
    values: Vec<bae_core::signals::SourcedValue>,
) -> Vec<AutomationSourcedValue> {
    values.into_iter().map(automation_sourced_value).collect()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_disc_id_signal(
    signal: bae_core::signals::DiscIdSignal,
) -> AutomationDiscIdSignal {
    use bae_core::signals::DiscIdSignal;
    match signal {
        DiscIdSignal::Computed {
            disc_id,
            track_count,
            ..
        } => AutomationDiscIdSignal::Computed {
            disc_id,
            track_count,
        },
        DiscIdSignal::Absent { track_count } => AutomationDiscIdSignal::Absent { track_count },
        DiscIdSignal::Failed {
            failure,
            track_count,
        } => AutomationDiscIdSignal::Failed {
            failure: automation_lookup_failure(failure),
            track_count,
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_barcode_signal(
    signal: bae_core::signals::BarcodeSignal,
) -> AutomationBarcodeSignal {
    use bae_core::signals::BarcodeSignal;
    match signal {
        BarcodeSignal::Scanning { codes } => AutomationBarcodeSignal::Scanning {
            codes: automation_sourced_values(codes),
        },
        BarcodeSignal::Settled { codes } => AutomationBarcodeSignal::Settled {
            codes: automation_sourced_values(codes),
        },
        BarcodeSignal::Failed { failure, codes } => AutomationBarcodeSignal::Failed {
            failure: automation_lookup_failure(failure),
            codes: automation_sourced_values(codes),
        },
        BarcodeSignal::Absent => AutomationBarcodeSignal::Absent,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_text_signal(
    signal: bae_core::signals::TextSignal,
) -> AutomationTextSignal {
    use bae_core::signals::TextSignal;
    match signal {
        TextSignal::Scanning {
            catalogs,
            free_text,
        } => AutomationTextSignal::Scanning {
            catalogs: automation_sourced_values(catalogs),
            free_text,
        },
        TextSignal::Settled {
            catalogs,
            free_text,
        } => AutomationTextSignal::Settled {
            catalogs: automation_sourced_values(catalogs),
            free_text,
        },
        TextSignal::Failed {
            failure,
            catalogs,
            free_text,
        } => AutomationTextSignal::Failed {
            failure: automation_lookup_failure(failure),
            catalogs: automation_sourced_values(catalogs),
            free_text,
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_signals(signals: bae_core::signals::Signals) -> AutomationSignals {
    AutomationSignals {
        disc_id: automation_disc_id_signal(signals.disc_id),
        barcode: automation_barcode_signal(signals.barcode),
        text: automation_text_signal(signals.text),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_signal_kind(kind: bae_core::identify::SignalKind) -> AutomationSignalKind {
    use bae_core::identify::SignalKind;
    match kind {
        SignalKind::DiscId => AutomationSignalKind::DiscId,
        SignalKind::Barcode => AutomationSignalKind::Barcode,
        SignalKind::Catalog => AutomationSignalKind::Catalog,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_signal_state(
    state: bae_core::identify::SignalState,
) -> AutomationSignalState {
    use bae_core::identify::SignalState;
    match state {
        SignalState::LookingUp => AutomationSignalState::LookingUp,
        SignalState::Found { count } => AutomationSignalState::Found { count },
        SignalState::NoMatch => AutomationSignalState::NoMatch,
        SignalState::Skipped => AutomationSignalState::Skipped,
        SignalState::Failed { failure } => AutomationSignalState::Failed {
            failure: automation_lookup_failure(failure),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_signal_option(
    option: bae_core::identify::SignalOption,
) -> AutomationSignalOption {
    AutomationSignalOption {
        value: option.value,
        origin: automation_signal_origin(option.origin),
        chosen: option.chosen,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_toolbar_signal(
    signal: bae_core::identify::ToolbarSignal,
) -> AutomationToolbarSignal {
    AutomationToolbarSignal {
        kind: automation_signal_kind(signal.kind),
        value: signal.value,
        origin: automation_signal_origin(signal.origin),
        state: automation_signal_state(signal.state),
        excluded: signal.excluded,
        options: signal
            .options
            .into_iter()
            .map(automation_signal_option)
            .collect(),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_discid_progress(
    progress: bae_core::identify::DiscidProgressView,
) -> AutomationDiscidProgress {
    use bae_core::identify::DiscidProgressView;
    match progress {
        DiscidProgressView::Computing => AutomationDiscidProgress::Computing,
        DiscidProgressView::LookingUp => AutomationDiscidProgress::LookingUp,
        DiscidProgressView::Done { n_results } => AutomationDiscidProgress::Done { n_results },
        DiscidProgressView::Skipped => AutomationDiscidProgress::Skipped,
        DiscidProgressView::Failed { failure } => AutomationDiscidProgress::Failed {
            failure: automation_lookup_failure(failure),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_barcode_progress(
    progress: bae_core::identify::BarcodeProgressView,
) -> AutomationBarcodeProgress {
    use bae_core::identify::BarcodeProgressView;
    match progress {
        BarcodeProgressView::Scanning => AutomationBarcodeProgress::Scanning,
        BarcodeProgressView::LookingUp {
            current,
            position,
            total,
        } => AutomationBarcodeProgress::LookingUp {
            current,
            position,
            total,
        },
        BarcodeProgressView::Done { n_results } => AutomationBarcodeProgress::Done { n_results },
        BarcodeProgressView::Failed { failure } => AutomationBarcodeProgress::Failed {
            failure: automation_lookup_failure(failure),
        },
        BarcodeProgressView::Skipped => AutomationBarcodeProgress::Skipped,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_result_provenance(
    release_id: String,
    provenance: bae_core::identify::ResultProvenance,
) -> AutomationResultProvenance {
    let bae_core::identify::ResultProvenance {
        by_disc_id,
        by_barcode,
        by_catalog,
    } = provenance;
    AutomationResultProvenance {
        release_id,
        by_disc_id,
        by_barcode,
        by_catalog,
    }
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the JSON enum. Core has
/// already folded the matches into their group cards, keyed the provenance,
/// reduced the in-flight payloads to counts, and dropped what must not cross —
/// this is a field copy per variant and nothing else.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_identify_state(
    state: bae_core::identify::IdentifyState,
) -> AutomationIdentifyState {
    use bae_core::identify::IdentifyStateView;
    match IdentifyStateView::from(state) {
        IdentifyStateView::Idle => AutomationIdentifyState::Idle,
        IdentifyStateView::Triangulating { discid, barcode } => {
            AutomationIdentifyState::Triangulating {
                discid: automation_discid_progress(discid),
                barcode: automation_barcode_progress(barcode),
            }
        }
        IdentifyStateView::Found {
            groups,
            library_statuses,
            track_count,
            provenance,
        } => AutomationIdentifyState::Found {
            groups: groups.into_iter().map(automation_release_group).collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(automation_library_status)
                .collect(),
            track_count,
            provenance: provenance
                .into_iter()
                .map(|(release_id, p)| automation_result_provenance(release_id, p))
                .collect(),
        },
        IdentifyStateView::NotFoundAnywhere => AutomationIdentifyState::NotFoundAnywhere,
        IdentifyStateView::ManualOnly { track_count } => {
            AutomationIdentifyState::ManualOnly { track_count }
        }
    }
}

pub(super) fn automation_prepare_step(step: PrepareStep) -> AutomationPrepareStep {
    match step {
        PrepareStep::Queued => AutomationPrepareStep::Queued,
        PrepareStep::ReadingFolder => AutomationPrepareStep::ReadingFolder,
        PrepareStep::ParsingMetadata => AutomationPrepareStep::ParsingMetadata,
        PrepareStep::WritingCoverArt => AutomationPrepareStep::WritingCoverArt,
        PrepareStep::DiscoveringFiles => AutomationPrepareStep::DiscoveringFiles,
        PrepareStep::ValidatingTracks => AutomationPrepareStep::ValidatingTracks,
    }
}

pub(super) fn automation_import_phase(phase: ImportPhase) -> AutomationImportPhase {
    match phase {
        ImportPhase::ReadingFiles => AutomationImportPhase::ReadingFiles,
        ImportPhase::MeasuringLoudness => AutomationImportPhase::MeasuringLoudness,
        ImportPhase::Finalizing => AutomationImportPhase::Finalizing,
    }
}

pub(super) fn automation_release(release: ReleaseDetail) -> AutomationRelease {
    AutomationRelease {
        summary: automation_release_summary(release.summary),
        display_name: release.display_name,
        year: release.year,
        label: release.label,
        catalog_number: release.catalog_number,
        country: release.country,
        total_duration_ms: release.total_duration_ms,
        tracks: release
            .tracks
            .into_iter()
            .map(automation_track_detail)
            .collect(),
        track_groups: release
            .track_groups
            .into_iter()
            .map(|group| AutomationTrackGroup {
                side: automation_track_side(group.side),
                tracks: group
                    .tracks
                    .into_iter()
                    .map(automation_track_detail)
                    .collect(),
            })
            .collect(),
        files: release
            .files
            .into_iter()
            .map(automation_file_detail)
            .collect(),
        image_files: release
            .image_files
            .into_iter()
            .map(automation_file_detail)
            .collect(),
        gallery_items: release
            .gallery_items
            .into_iter()
            .map(automation_gallery_item)
            .collect(),
    }
}

pub(super) fn automation_release_summary(
    summary: bae_core::album_detail::ReleaseSummary,
) -> AutomationReleaseSummary {
    AutomationReleaseSummary {
        id: summary.id,
        album_id: summary.album_id,
        format: summary.format,
        storage_state: summary.storage_state.into(),
        pinned: summary.pinned,
        storage_actions: summary
            .storage_actions
            .into_iter()
            .map(Into::into)
            .collect(),
        transfer_action: summary.transfer_action.map(Into::into),
        file_count: summary.file_count,
        total_size: summary.total_size,
        cover: summary.cover.map(automation_image_ref),
    }
}

pub(super) fn automation_image_ref(image: ImageRef) -> AutomationImageRef {
    AutomationImageRef {
        id: image.id,
        version: image.version,
    }
}

pub(super) fn automation_track_detail(track: TrackDetail) -> AutomationTrackDetail {
    AutomationTrackDetail {
        id: track.id,
        title: track.title,
        side: track.side,
        track_number: track.track_number,
        duration_ms: track.duration_ms,
        artist_names: track.artist_names,
        position_text: track.position_text,
        position: automation_track_position(track.position),
    }
}

pub(super) fn automation_track_position(position: TrackPosition) -> AutomationTrackPosition {
    match position {
        TrackPosition::Sided {
            side_letter,
            number,
        } => AutomationTrackPosition::Sided {
            side_letter,
            number,
        },
        TrackPosition::SidedUnnumbered { side_letter } => {
            AutomationTrackPosition::SidedUnnumbered { side_letter }
        }
        TrackPosition::Disc { disc, number } => AutomationTrackPosition::Disc { disc, number },
        TrackPosition::DiscUnnumbered { disc } => AutomationTrackPosition::DiscUnnumbered { disc },
        TrackPosition::Flat { number } => AutomationTrackPosition::Flat { number },
        TrackPosition::Unnumbered => AutomationTrackPosition::Unnumbered,
    }
}

pub(super) fn automation_track_side(side: TrackSide) -> AutomationTrackSide {
    match side {
        TrackSide::Sided { side_letter } => AutomationTrackSide::Sided { side_letter },
        TrackSide::Disc { disc } => AutomationTrackSide::Disc { disc },
        TrackSide::Flat => AutomationTrackSide::Flat,
    }
}

pub(super) fn automation_file_detail(file: FileDetail) -> AutomationFileDetail {
    AutomationFileDetail {
        id: file.id,
        original_filename: file.original_filename,
        file_size: file.file_size,
        is_image: file.is_image,
        content_type: file.content_type,
        audio_format: file
            .source_audio
            .map(|source_audio| automation_audio_format(source_audio.format)),
    }
}

pub(super) fn automation_audio_format(format: AudioFormat) -> AutomationAudioFormat {
    AutomationAudioFormat {
        codec: format.codec,
        sample_rate_hz: format.sample_rate_hz,
        bits_per_sample: format.bits_per_sample,
        bitrate_kbps: format.bitrate_kbps,
        channels: format.channels,
    }
}

pub(super) fn automation_gallery_item(item: GalleryItem) -> AutomationGalleryItem {
    AutomationGalleryItem {
        id: item.id,
        label: item.label,
        source: match item.source {
            GallerySource::Cover(image) => AutomationGallerySource::Cover {
                image: automation_image_ref(image),
            },
            GallerySource::ReleaseFile { file_id } => {
                AutomationGallerySource::ReleaseFile { file_id }
            }
        },
    }
}

pub(super) fn automation_library_search_results(
    results: SearchResults,
) -> AutomationLibrarySearchResults {
    AutomationLibrarySearchResults {
        albums: results
            .albums
            .into_iter()
            .map(|album| AutomationAlbumSearchResult {
                id: album.id,
                title: album.title,
                year: album.year,
                artist_name: album.artist_name,
                cover: album.cover.map(automation_image_ref),
            })
            .collect(),
        tracks: results
            .tracks
            .into_iter()
            .map(|track| AutomationTrackSearchResult {
                id: track.id,
                title: track.title,
                duration_ms: track.duration_ms,
                album_id: track.album_id,
                album_title: track.album_title,
                artist_name: track.artist_name,
            })
            .collect(),
    }
}
