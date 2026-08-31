use super::*;

enum SettledLead {
    NoExternalRelease,
    ExternalRelease {
        provenance: crate::import::MetadataProvenance,
        payloads: crate::import::payloads::ReleasePayloads,
    },
}

fn metadata_for_settled_lead(
    context: &SweepContext,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    settled_lead: SettledLead,
) -> Result<crate::import::CandidateMetadataDraft, crate::import::ImportError> {
    match settled_lead {
        SettledLead::NoExternalRelease => Ok(crate::import::CandidateMetadataDraft {
            edit: crate::import::pane::blank_candidate_draft(&candidate.files),
            provenance: None,
            cover: None,
        }),
        SettledLead::ExternalRelease {
            provenance,
            payloads,
        } => Ok(crate::import::CandidateMetadataDraft {
            edit: context
                .import
                .external_candidate_draft(&payloads, candidate, durations)?,
            provenance: Some(provenance),
            cover: None,
        }),
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
    // The one gate on storability. "Terminal" only means nothing is in flight;
    // a `Found` built from the one signal whose lookup survived, or a
    // `NotFoundAnywhere` where neither lookup ever answered, is terminal and is
    // not an answer. The conversion refuses those, and refusing means no row,
    // which means the next pass asks again.
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

    let metadata =
        match metadata_for_settled_lead(context, candidate, &signals.durations, settled_lead) {
            Ok(metadata) => metadata,
            Err(error) => {
                warn!(
                    "sweep: could not project metadata for {} ({error}); writing no row",
                    candidate.path.display()
                );
                return false;
            }
        };
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
/// neither — the next pass asks again, exactly as a failed lookup does.
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
    let TerminalVerdict::Found { matches, .. } = verdict else {
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
        // Shutdown mid-lookup is a transport failure by another name: nothing
        // was learned, so nothing is written and the next launch asks again.
        _ = token.cancelled() => return None,
        payloads = settle => payloads,
    };
    let payloads = match payloads {
        Ok(payloads) => payloads,
        Err(error) => {
            debug!(
                "sweep: could not settle {} ({error}); writing no row",
                only_match.release_id
            );
            return None;
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
                "sweep: {} states no readable tracklist ({error}); writing no row",
                only_match.release_id
            );
            None
        }
    }
}

/// Whether `candidate_key` holds a stored verdict for its current file shape
/// — see [`ImportServiceHandle::stored_verdict`], which owns the read. A
/// failure resolves to `false` after a `warn!`: the caller's fallback is a
/// full identification run, which re-answers the candidate and re-stores the
/// row, so nothing is served from — or left depending on — the unreadable
/// one.
pub(super) async fn has_stored_verdict(context: &SweepContext, candidate_key: &str) -> bool {
    match context.import.stored_verdict(candidate_key).await {
        Ok(verdict) => verdict.is_some(),
        Err(error) => {
            warn!(
                "reading the stored verdict for {candidate_key} failed ({error}); \
                 re-running identification"
            );
            false
        }
    }
}

/// Watch a run a person started and store the first verdict it reaches.
///
/// **The first that stores, not the first that settles.** A terminal state
/// shaped by a lookup that never answered does not convert, so a re-run from the
/// toolbar after a network blip is still captured. Once one verdict is stored
/// the watch ends: a signal the user then toggles off is them filtering their
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
                let Ok(mut verdict) = TerminalVerdict::try_from(state) else {
                    // Terminal but shaped by a failed lookup. Keep watching: a
                    // re-run from the toolbar may still answer it.
                    continue;
                };
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
                let metadata = match metadata_for_settled_lead(
                    context,
                    &entry.candidate,
                    &signals.durations,
                    settled_lead,
                ) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        warn!(
                            "sweep: could not project metadata for {candidate_key} ({error}); writing no row"
                        );
                        continue;
                    }
                };
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
