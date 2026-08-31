use super::*;

enum SettledLead {
    NoExternalRelease,
    ExternalRelease {
        provenance: crate::import::MetadataProvenance,
        payloads: crate::import::payloads::ReleasePayloads,
    },
}

async fn metadata_for_settled_lead(
    context: &SweepContext,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    settled_lead: SettledLead,
) -> Result<crate::import::CandidateMetadataDraft, crate::import::ImportError> {
    match settled_lead {
        SettledLead::NoExternalRelease => {
            let source_draft = crate::import::pane::blank_candidate_source(&candidate.files);
            Ok(crate::import::CandidateMetadataDraft {
                edit: source_draft.edit,
                track_mappings: source_draft.track_mappings,
                source_discogs_artist_ids: Default::default(),
                provenance: None,
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            })
        }
        SettledLead::ExternalRelease {
            provenance,
            payloads,
        } => {
            context
                .import
                .external_candidate_metadata(&payloads, candidate, durations, provenance, None)
                .await
        }
    }
}

async fn metadata_or_failed_verdict(
    context: &SweepContext,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    settled_lead: SettledLead,
    verdict: &mut TerminalVerdict,
) -> crate::import::CandidateMetadataDraft {
    match metadata_for_settled_lead(context, candidate, durations, settled_lead).await {
        Ok(metadata) => metadata,
        Err(error) => {
            warn!(
                "sweep: could not project metadata for {} ({error}); storing the failure",
                candidate.path.display()
            );
            let track_count = match verdict {
                TerminalVerdict::Found { track_count, .. }
                | TerminalVerdict::Failed { track_count, .. }
                | TerminalVerdict::ManualOnly { track_count } => *track_count,
                TerminalVerdict::NotFoundAnywhere => 0,
            };
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::signals::LookupFailure::Diagnostic {
                        detail: error.to_string(),
                    },
                )],
                track_count,
            };
            let source_draft = crate::import::pane::blank_candidate_source(&candidate.files);
            crate::import::CandidateMetadataDraft {
                edit: source_draft.edit,
                track_mappings: source_draft.track_mappings,
                source_discogs_artist_ids: Default::default(),
                provenance: None,
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            }
        }
    }
}

/// Turn one candidate's terminal state into a stored row, or into nothing.
/// Returns whether a row was written.
pub(super) async fn finish_candidate(
    context: &SweepContext,
    entry: &InFlight,
    state: IdentifyState,
    token: &CancellationToken,
) -> bool {
    // Only in-flight states fail conversion. Provider failures convert to an
    // explicit failed verdict and are stored below.
    let Ok(mut verdict) = TerminalVerdict::try_from(state) else {
        return false;
    };

    let Some(signals) = entry.signals.as_ref() else {
        warn!(
            "sweep: {} reached a verdict with no signals; writing no row",
            entry.job.representative().path.display()
        );
        return false;
    };
    let candidate = entry.job.representative();
    let Some(settled_lead) =
        settle_lead(context, &mut verdict, candidate, &signals.durations, token).await
    else {
        return false;
    };

    let mut metadata = metadata_or_failed_verdict(
        context,
        candidate,
        &signals.durations,
        settled_lead,
        &mut verdict,
    )
    .await;
    if let Err(error) = preserve_current_mapping_decisions(context, candidate, &mut metadata).await
    {
        warn!(
            "sweep: could not preserve the current mappings for {} ({error}); writing no row",
            candidate.path.display()
        );
        return false;
    }
    save(
        context,
        token,
        &candidate.path.to_string_lossy(),
        &candidate.files.content_hash(),
        &candidate.path.to_string_lossy(),
        &verdict,
        Some(signals.clone()),
        candidate.file_edit_revision,
        entry.expected_metadata_revision,
        metadata,
    )
    .await
}

async fn preserve_current_mapping_decisions(
    context: &SweepContext,
    candidate: &FolderCandidate,
    metadata: &mut crate::import::CandidateMetadataDraft,
) -> Result<(), crate::library::LibraryError> {
    let current = context
        .library_manager
        .load_import_candidate_preparation(&candidate.files.content_hash())
        .await?
        .ok_or_else(|| {
            crate::library::LibraryError::Internal(format!(
                "{} has no stored import preparation",
                candidate.path.display()
            ))
        })?;
    metadata.track_mappings = crate::import::edits::preserve_track_mapping_decisions(
        std::mem::take(&mut metadata.track_mappings),
        &current.track_mappings,
    );
    Ok(())
}

/// Write one row. Cancellation is re-checked immediately before the write, not
/// only before the lookup that precedes it: teardown during that lookup must
/// leave nothing behind, and "a cancelled candidate writes no row" is only true
/// if the last thing checked before writing is the token.
#[allow(clippy::too_many_arguments)]
pub(super) async fn save(
    context: &SweepContext,
    token: &CancellationToken,
    candidate_key: &str,
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
    signals: Option<crate::signals::Signals>,
    expected_edit_revision: u64,
    expected_metadata_revision: u64,
    metadata: crate::import::CandidateMetadataDraft,
) -> bool {
    if token.is_cancelled() {
        return false;
    }
    // A verdict with no signals behind it is not a verdict: the state machine
    // reaches a terminal state only from a settled snapshot, so this cannot
    // happen once it has passed `Triangulating` — and if it somehow does, the
    // row is refused and the next pass asks again rather than storing a
    // candidate whose signals nothing recorded.
    let Some(signals) = signals else {
        warn!("sweep: {folder_path} reached a verdict with no signals; writing no row");
        return false;
    };
    let row = NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
        folder_path: folder_path.to_string(),
        verdict: verdict.clone(),
        signals,
        expected_edit_revision,
        expected_metadata_revision,
        metadata,
    };
    let wrote = match context
        .import
        .save_candidate_verdict_if_current(candidate_key, &row)
        .await
    {
        Ok(wrote) => wrote,
        Err(e) => {
            warn!(
                "sweep: could not store the verdict for {} ({e}); it is retried next pass",
                row.folder_path
            );
            return false;
        }
    };
    if !wrote {
        debug!(
            "sweep: discarded stale verdict for {} at file-edit revision {} and metadata revision {}",
            row.folder_path, expected_edit_revision, expected_metadata_revision
        );
        return false;
    }
    context
        .import
        .announce_candidate_verdict_stored(candidate_key.to_string());
    true
}

/// Settle a candidate's lead: buy the documents that describe the release it
/// matched, store them, and read the source's own tracklist out of what came
/// back. Returns whether the verdict may now be stored.
///
/// **The documents land before the verdict does.** A stored verdict whose lead
/// carries a tracklist is the queue's promise that opening that candidate needs
/// no network, so the two are written in this order and a failure here writes
/// neither. The candidate stores an explicit failure, while the release
/// document archive remains absent.
///
/// Only a single-match `Found` has a lead. Several matches and a conflict are
/// questions for a person, answered from the result rows the verdict already
/// carries, and a full fetch of every pressing on the list would buy a
/// classification that cannot change.
///
/// A release some other candidate already settled costs nothing: its documents
/// are read back and the tracklist re-derived from them.
async fn settle_lead(
    context: &SweepContext,
    verdict: &mut TerminalVerdict,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    token: &CancellationToken,
) -> Option<SettledLead> {
    let TerminalVerdict::Found {
        matches,
        track_count,
        ..
    } = verdict
    else {
        return Some(SettledLead::NoExternalRelease);
    };
    let [only_match] = matches.as_mut_slice() else {
        return Some(SettledLead::NoExternalRelease);
    };
    let release = MetadataRef::new(&only_match.release_id, only_match.source);

    let settle = crate::import::service::prepare_release(
        &context.library_manager,
        &release,
        CallPriority::Background,
    );
    let payloads = tokio::select! {
        biased;
        // Shutdown is not a provider answer and writes nothing.
        _ = token.cancelled() => return None,
        payloads = settle => payloads,
    };
    let payloads = match payloads {
        Ok(payloads) => payloads,
        Err(error) => {
            debug!(
                "sweep: could not settle {} ({error}); storing the failure",
                only_match.release_id
            );
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::import::search::import_error_to_lookup_failure(&error),
                )],
                track_count: *track_count,
            };
            return Some(SettledLead::NoExternalRelease);
        }
    };
    let audio_durations =
        match crate::import::track_slots::audio_durations(&candidate.files, durations) {
            Ok(durations) => durations,
            Err(error) => {
                warn!(
                    "sweep: {} has incomplete audio timing ({error}); writing no row",
                    candidate.path.display()
                );
                return None;
            }
        };
    match payloads.source_tracks_for_audio(&audio_durations) {
        Ok(source_tracks) => {
            // `SourceTracks::Nothing` is an answer — this release states no
            // tracklist — so the verdict stores with the match unverifiable, and
            // the Ready rule lands it in Needs you rather than admitting it.
            only_match.source_tracks = Some(source_tracks);
            Some(SettledLead::ExternalRelease {
                provenance: crate::import::MetadataProvenance::ExternalRelease {
                    source: only_match.source,
                    release_id: only_match.release_id.clone(),
                },
                payloads,
            })
        }
        Err(error) => {
            debug!(
                "sweep: {} states no readable tracklist ({error}); storing the failure",
                only_match.release_id
            );
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::signals::LookupFailure::Diagnostic {
                        detail: error.to_string(),
                    },
                )],
                track_count: *track_count,
            };
            Some(SettledLead::NoExternalRelease)
        }
    }
}

/// Whether `candidate_key` holds a stored verdict for its current file shape
/// — see [`ImportServiceHandle::stored_verdict`], which owns the read. A
/// read failure is returned to the caller; it must not be converted into
/// permission to overwrite state that could not be inspected.
pub(super) async fn has_stored_verdict(
    context: &SweepContext,
    candidate_key: &str,
) -> Result<bool, crate::import::ImportError> {
    Ok(context
        .import
        .stored_verdict(candidate_key)
        .await?
        .is_some())
}

/// Watch a run a person started and store the first verdict it reaches.
///
/// **The first that stores.** Once one verdict is stored the watch ends: a
/// signal the user then toggles off is them filtering their
/// own view, not a durable fact about the folder, and persisting it would leave
/// the next launch showing a queue narrowed by exclusions nobody remembers
/// making.
pub(super) async fn record_explicit_lookup_verdict(
    context: &SweepContext,
    run: IdentifyRunId,
    candidate_key: String,
    token: &CancellationToken,
) {
    // Subscribe before reading the candidate, so no state change can land in
    // between.
    let mut bus = context.import.subscribe_events();
    let Some(candidate) = folder_candidate(context, &candidate_key).await else {
        // Not a scanned folder candidate — a library release being
        // re-identified. It has no content hash to key a row by.
        return;
    };
    let expected_metadata_revision = match super::candidate_metadata_revision(context, &candidate)
        .await
    {
        Ok(revision) => revision,
        Err(error) => {
            warn!(
                    "sweep: cannot read the metadata revision for {candidate_key} ({error}); recording no verdict"
                );
            return;
        }
    };
    let mut entry = ExplicitLookupInFlight {
        candidate,
        signals: None,
        expected_metadata_revision,
    };

    loop {
        let event = tokio::select! {
            biased;
            _ = token.cancelled() => return,
            event = bus.recv() => event,
        };
        match event {
            Ok(ImportEvent::SignalsUpdated {
                candidate_key: key,
                signals,
                ..
            }) if key == candidate_key => {
                entry.signals = Some(signals);
            }
            Ok(ImportEvent::IdentifyStateChanged {
                candidate_key: key,
                run: event_run,
                state,
                ..
            }) if key == candidate_key && event_run == run => {
                if matches!(state, IdentifyState::Idle) {
                    // The run was cancelled — the user dismissed the candidate,
                    // or the sweep took it over. Either way this watch is done.
                    return;
                }
                if !state.is_terminal() {
                    continue;
                }
                let mut verdict = TerminalVerdict::try_from(state)
                    .expect("a terminal identify state always has a verdict");
                // Settles here too: a row a person's own run wrote is a row the
                // queue treats as answered, and the promise that an answered
                // lead opens offline holds however the answer was reached.
                let Some(signals) = entry.signals.as_ref() else {
                    warn!(
                        "sweep: {candidate_key} reached a verdict with no signals; writing no row"
                    );
                    continue;
                };
                let Some(settled_lead) = settle_lead(
                    context,
                    &mut verdict,
                    &entry.candidate,
                    &signals.durations,
                    token,
                )
                .await
                else {
                    continue;
                };
                let mut metadata = metadata_or_failed_verdict(
                    context,
                    &entry.candidate,
                    &signals.durations,
                    settled_lead,
                    &mut verdict,
                )
                .await;
                if let Err(error) =
                    preserve_current_mapping_decisions(context, &entry.candidate, &mut metadata)
                        .await
                {
                    warn!(
                        "sweep: could not preserve the current mappings for {candidate_key} ({error}); writing no verdict"
                    );
                    continue;
                }
                if save(
                    context,
                    token,
                    &candidate_key,
                    &entry.candidate.files.content_hash(),
                    &entry.candidate.path.to_string_lossy(),
                    &verdict,
                    Some(signals.clone()),
                    entry.candidate.file_edit_revision,
                    entry.expected_metadata_revision,
                    metadata,
                )
                .await
                {
                    return;
                }
            }
            // The candidate is gone, or is a different shape than the run was
            // answering. Either way this watch has nothing left to store: a
            // verdict written now would describe the folder as it was before
            // the binding changed, which is exactly the stale answer the change
            // cleared.
            Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key }))
                if key == candidate_key =>
            {
                return;
            }
            Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))
                if candidate.path.to_string_lossy() == candidate_key.as_str() =>
            {
                return;
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(
                    "sweep: explicit Lookup recorder for {candidate_key} lagged by {n} events; writing no verdict"
                );
                return;
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// The scanned folder candidate behind a key, or `None` when the key names
/// something else (a library release being re-identified) or nothing.
async fn folder_candidate(context: &SweepContext, key: &str) -> Option<FolderCandidate> {
    super::actionable_candidate(context, key).await
}
