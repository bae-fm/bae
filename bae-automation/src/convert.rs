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

pub(super) fn search_query(query: AutomationSearchQuery) -> SearchQuery {
    match query {
        AutomationSearchQuery::General {
            artist,
            album,
            source,
        } => SearchQuery::General {
            artist,
            album,
            source: source.into(),
        },
        AutomationSearchQuery::CatalogNumber {
            catalog_number,
            source,
        } => SearchQuery::CatalogNumber {
            catalog_number,
            source: source.into(),
        },
        AutomationSearchQuery::Barcode { barcode, source } => SearchQuery::Barcode {
            barcode,
            source: source.into(),
        },
    }
}

pub(super) fn identity_choice(choice: AutomationIdentityChoice) -> IdentityChoice {
    match choice {
        AutomationIdentityChoice::Exact { source, release_id } => IdentityChoice::Exact {
            release_ref: MetadataRef::new(release_id, source.into()),
        },
        AutomationIdentityChoice::Approximate { source, release_id } => {
            IdentityChoice::Approximate {
                release_ref: MetadataRef::new(release_id, source.into()),
            }
        }
        AutomationIdentityChoice::Unknown => IdentityChoice::Unknown,
    }
}

pub(super) fn automation_claim_line(claim: bae_core::import::ClaimLine) -> AutomationClaimLine {
    AutomationClaimLine {
        choice: automation_identity_choice(claim.choice),
        evidence: match claim.evidence {
            bae_core::import::ClaimEvidence::DiscIdAlone => AutomationClaimEvidence::DiscIdAlone,
            bae_core::import::ClaimEvidence::DiscIdShared { match_count } => {
                AutomationClaimEvidence::DiscIdShared { match_count }
            }
            bae_core::import::ClaimEvidence::Barcode => AutomationClaimEvidence::Barcode,
            bae_core::import::ClaimEvidence::Search => AutomationClaimEvidence::Search,
        },
        release: claim.release,
        track_count: claim.track_count,
    }
}

pub(super) fn automation_identity_choice(choice: IdentityChoice) -> AutomationIdentityChoice {
    match choice {
        IdentityChoice::Exact { release_ref } => AutomationIdentityChoice::Exact {
            source: release_ref.source.into(),
            release_id: release_ref.id,
        },
        IdentityChoice::Approximate { release_ref } => AutomationIdentityChoice::Approximate {
            source: release_ref.source.into(),
            release_id: release_ref.id,
        },
        IdentityChoice::Unknown => AutomationIdentityChoice::Unknown,
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
        album_artist_names: edit.album_artist_names,
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
                artist_names: track.artist_names,
            })
            .collect(),
    }
}

pub(super) fn release_user_edit(
    edit: AutomationReleaseUserEdit,
) -> bae_core::import::ReleaseUserEdit {
    bae_core::import::ReleaseUserEdit {
        album_title: edit.album_title,
        album_artist_names: edit.album_artist_names,
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
                artist_names: track.artist_names,
                // Automation edits a release's metadata, never which of the
                // folder's audio backs each track; an import it starts gets the
                // track slots the folder and the tracklist produce.
                file: None,
            })
            .collect(),
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
pub(super) fn automation_signal_role(role: bae_core::identify::SignalRole) -> AutomationSignalRole {
    use bae_core::identify::SignalRole;
    match role {
        SignalRole::Identity => AutomationSignalRole::Identity,
        SignalRole::Filter => AutomationSignalRole::Filter,
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
        SignalState::Confirms { count } => AutomationSignalState::Confirms { count },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_toolbar_signal(
    signal: bae_core::identify::ToolbarSignal,
) -> AutomationToolbarSignal {
    AutomationToolbarSignal {
        kind: automation_signal_kind(signal.kind),
        role: automation_signal_role(signal.role),
        value: signal.value,
        origin: automation_signal_origin(signal.origin),
        state: automation_signal_state(signal.state),
        excluded: signal.excluded,
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
        matches_catalog,
    } = provenance;
    AutomationResultProvenance {
        release_id,
        by_disc_id,
        by_barcode,
        matches_catalog,
    }
}

/// Unzip core's paired rows into the two parallel lists the JSON surface carries,
/// each record keeping its own `release_id`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn automation_results_and_statuses(
    rows: Vec<bae_core::identify::ResultRow>,
) -> (Vec<AutomationMetadataResult>, Vec<AutomationLibraryStatus>) {
    rows.into_iter()
        .map(|bae_core::identify::ResultRow { result, status }| {
            (
                automation_metadata_result(result),
                automation_library_status(status),
            )
        })
        .unzip()
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the JSON enum. Core has
/// already folded the matches into their group, keyed the provenance, reduced the
/// in-flight payloads to counts, and dropped what must not cross — this is a field
/// copy per variant and nothing else.
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
            group,
            library_statuses,
            track_count,
            provenance,
        } => AutomationIdentifyState::Found {
            group: automation_release_group(group),
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
        IdentifyStateView::Conflict {
            discid_results,
            barcode_results,
            // The disc-id section header names its source on the desktop UI. The
            // JSON surface doesn't carry a header, and each result already states
            // its own `source`, so a client that wants the label reads it there.
            matched_barcode,
            track_count,
        } => {
            let (discid_results, discid_library_statuses) =
                automation_results_and_statuses(discid_results);
            let (barcode_results, barcode_library_statuses) =
                automation_results_and_statuses(barcode_results);
            AutomationIdentifyState::Conflict {
                discid_results,
                discid_library_statuses,
                barcode_results,
                barcode_library_statuses,
                matched_barcode,
                track_count,
            }
        }
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
        audio_format: file.audio_format.map(automation_audio_format),
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
