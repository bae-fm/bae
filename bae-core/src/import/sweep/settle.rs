use super::*;

#[derive(Debug)]
pub(super) enum FinishCandidateOutcome {
    Stored,
    Superseded,
    Failed { error: String },
}

enum FinalizationError {
    Superseded,
    Failed(String),
}

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

/// Turn one candidate's terminal state into a stored row, a superseded result,
/// or an explicit commit failure.
pub(super) async fn finish_candidate(
    context: &SweepContext,
    entry: &InFlight,
    state: IdentifyState,
    token: &CancellationToken,
) -> FinishCandidateOutcome {
    let mut verdict = TerminalVerdict::try_from(state)
        .expect("the sweep finalizes only terminal identify states");

    let Some(signals) = entry.signals.as_ref() else {
        return FinishCandidateOutcome::Failed {
            error: format!(
                "{} reached a verdict with no settled signals",
                entry.job.representative().path.display()
            ),
        };
    };
    let candidate = entry.job.representative();
    let settled_lead = match settle_lead(
        context,
        &mut verdict,
        candidate,
        &signals.durations,
        CallPriority::Background,
        token,
    )
    .await
    {
        Ok(settled) => settled,
        Err(FinalizationError::Superseded) => return FinishCandidateOutcome::Superseded,
        Err(FinalizationError::Failed(error)) => {
            return FinishCandidateOutcome::Failed { error };
        }
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
        return FinishCandidateOutcome::Failed {
            error: error.to_string(),
        };
    }
    save(
        context,
        token,
        &candidate.path.to_string_lossy(),
        &candidate.files.content_hash(),
        &candidate.path.to_string_lossy(),
        &verdict,
        signals.clone(),
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
    signals: crate::signals::Signals,
    expected_edit_revision: u64,
    expected_metadata_revision: u64,
    metadata: crate::import::CandidateMetadataDraft,
) -> FinishCandidateOutcome {
    if token.is_cancelled() {
        return FinishCandidateOutcome::Superseded;
    }
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
            return FinishCandidateOutcome::Failed {
                error: e.to_string(),
            };
        }
    };
    if !wrote {
        debug!(
            "sweep: discarded stale verdict for {} at file-edit revision {} and metadata revision {}",
            row.folder_path, expected_edit_revision, expected_metadata_revision
        );
        return FinishCandidateOutcome::Superseded;
    }
    context
        .import
        .announce_candidate_verdict_stored(candidate_key.to_string());
    FinishCandidateOutcome::Stored
}

/// The one pressing a verdict's matches describe, or `None` when they describe
/// several.
///
/// Find online groups the same results into album cards and pairs two sources'
/// records of one physical pressing into a single row, so "how many pressings
/// did this candidate match" is that grouping's question, not a count of
/// result rows. A MusicBrainz release and a Discogs release agreeing on a
/// barcode are one row a person picks whole — an answer, not a question.
fn sole_pressing(matches: &[MetadataResult]) -> Option<crate::import::release_group::Pressing> {
    let mut pressings = crate::import::release_group::group_results(matches.to_vec())
        .into_iter()
        .flat_map(|group| group.pressings);
    let only = pressings.next()?;
    pressings.next().is_none().then_some(only)
}

/// Settle a candidate's lead: buy the documents that describe the pressing it
/// matched — the primary's and every partner's — store them, and read the
/// primary's own tracklist out of what came back. Returns whether the verdict
/// may now be stored.
///
/// **The documents land before the verdict does.** A stored verdict whose lead
/// carries a tracklist is the queue's promise that opening that candidate needs
/// no network, and that promise covers every source the pick claims, so a
/// partner that will not prepare fails the lead exactly as the primary does:
/// the candidate stores an explicit failure and no verdict names the pressing.
///
/// Only a `Found` that groups into one pressing has a lead. Several pressings
/// and a conflict are questions for a person, answered from the result rows the
/// verdict already carries, and a full fetch of every pressing on the list would
/// buy a classification that cannot change.
///
/// A release some other candidate already settled costs nothing: its documents
/// are read back and the tracklist re-derived from them.
///
/// `priority` is the run's own: a person's explicit lookup fetches its lead
/// ahead of the sweep's queued calls, so the verdict they are watching for
/// does not wait behind a queue nobody is watching.
async fn settle_lead(
    context: &SweepContext,
    verdict: &mut TerminalVerdict,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    priority: CallPriority,
    token: &CancellationToken,
) -> Result<SettledLead, FinalizationError> {
    let TerminalVerdict::Found {
        matches,
        track_count,
        ..
    } = verdict
    else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let Some(pressing) = sole_pressing(matches) else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let (primary, partners) = pressing.claims();

    let settle = async {
        let payloads =
            crate::import::service::prepare_release(&context.library_manager, &primary, priority)
                .await?;
        crate::import::service::prepare_partners(
            &context.library_manager,
            &primary,
            &partners,
            priority,
        )
        .await?;
        Ok::<_, crate::import::ImportError>(payloads)
    };
    let payloads = tokio::select! {
        biased;
        // Shutdown is not a provider answer and writes nothing.
        _ = token.cancelled() => return Err(FinalizationError::Superseded),
        payloads = settle => payloads,
    };
    let payloads = match payloads {
        Ok(payloads) => payloads,
        Err(error) => {
            debug!(
                "sweep: could not settle {} ({error}); storing the failure",
                primary.id
            );
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::import::search::import_error_to_lookup_failure(&error),
                )],
                track_count: *track_count,
            };
            return Ok(SettledLead::NoExternalRelease);
        }
    };
    let audio_durations =
        match crate::import::track_slots::audio_durations(&candidate.files, durations) {
            Ok(durations) => durations,
            Err(error) => {
                return Err(FinalizationError::Failed(error.to_string()));
            }
        };
    match payloads.source_tracks_for_audio(&audio_durations) {
        Ok(source_tracks) => {
            // `SourceTracks::Nothing` is an answer — this release states no
            // tracklist — so the verdict stores with the match unverifiable, and
            // the Ready rule lands it in Needs you rather than admitting it.
            //
            // The tracklist belongs to the primary's own match row: it is read
            // from the primary's document, and a partner states its own.
            matches
                .iter_mut()
                .find(|result| result.source == primary.source && result.release_id == primary.id)
                .expect("the pressing's primary is one of the verdict's matches")
                .source_tracks = Some(source_tracks);
            Ok(SettledLead::ExternalRelease {
                provenance: pressing.pick(),
                payloads,
            })
        }
        Err(error) => {
            debug!(
                "sweep: {} states no readable tracklist ({error}); storing the failure",
                primary.id
            );
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::signals::LookupFailure::Diagnostic {
                        detail: error.to_string(),
                    },
                )],
                track_count: *track_count,
            };
            Ok(SettledLead::NoExternalRelease)
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
    candidate: FolderCandidate,
    token: &CancellationToken,
) {
    // Subscribe before reading the revision, so no state change can land in
    // between.
    let mut bus = context.import.subscribe_events();
    let expected_metadata_revision =
        match super::candidate_metadata_revision(context, &candidate).await {
            Ok(revision) => revision,
            Err(error) => {
                context
                    .import
                    .fail_identification(&candidate_key, error.to_string());
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
                    context.import.fail_identification(
                        &candidate_key,
                        format!("{candidate_key} reached a verdict with no settled signals"),
                    );
                    return;
                };
                let settled_lead = match settle_lead(
                    context,
                    &mut verdict,
                    &entry.candidate,
                    &signals.durations,
                    CallPriority::Interactive,
                    token,
                )
                .await
                {
                    Ok(settled) => settled,
                    Err(FinalizationError::Superseded) => {
                        context.import.discard_identification(&candidate_key);
                        return;
                    }
                    Err(FinalizationError::Failed(error)) => {
                        context.import.fail_identification(&candidate_key, error);
                        return;
                    }
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
                    context
                        .import
                        .fail_identification(&candidate_key, error.to_string());
                    return;
                }
                match save(
                    context,
                    token,
                    &candidate_key,
                    &entry.candidate.files.content_hash(),
                    &entry.candidate.path.to_string_lossy(),
                    &verdict,
                    signals.clone(),
                    entry.candidate.file_edit_revision,
                    entry.expected_metadata_revision,
                    metadata,
                )
                .await
                {
                    FinishCandidateOutcome::Stored => return,
                    FinishCandidateOutcome::Superseded => {
                        info!(
                            "lookup: {candidate_key} changed while its answer was being \
                             stored; the answer is discarded"
                        );
                        context.import.discard_identification(&candidate_key);
                        return;
                    }
                    FinishCandidateOutcome::Failed { error } => {
                        context.import.fail_identification(&candidate_key, error);
                        return;
                    }
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
                context.import.discard_identification(&candidate_key);
                return;
            }
            Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))
                if candidate.path.to_string_lossy() == candidate_key.as_str() =>
            {
                context.import.discard_identification(&candidate_key);
                return;
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => {
                context.import.fail_identification(
                    &candidate_key,
                    format!(
                        "the identification event stream dropped {n} events before the result could be stored"
                    ),
                );
                return;
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}
