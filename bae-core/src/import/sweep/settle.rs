use super::*;

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

    if !settle_lead(context, &mut verdict, token).await {
        return false;
    }

    let candidate = entry.job.representative();
    save(
        context,
        token,
        &candidate.path.to_string_lossy(),
        &candidate.files.content_hash(),
        &candidate.path.to_string_lossy(),
        &verdict,
        entry.probed_total_duration_ms,
        candidate.file_edit_revision,
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
    probed_total_duration_ms: u64,
    expected_edit_revision: u64,
) -> bool {
    if token.is_cancelled() {
        return false;
    }
    // A single settled match IS the identity pick: identification made the
    // decision a click makes on a several-match row, so it lands the same way,
    // at the same claim a click lands, and the pane reopens on it after a
    // restart. Anything else leaves the question open, and takes with it
    // whatever pick a superseded verdict of this run's own had made — a pick a
    // person made stands either way.
    let identity_pick = match verdict {
        TerminalVerdict::Found { matches, .. } if matches.len() == 1 => {
            let only = &matches[0];
            Some(crate::import::IdentityPick::Release {
                source: only.source,
                release_id: only.release_id.clone(),
                claim: crate::import::ClaimLevel::Exact,
            })
        }
        _ => None,
    };
    let row = NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
        folder_path: folder_path.to_string(),
        verdict: verdict.clone(),
        probed_total_duration_ms,
        expected_edit_revision,
        identity_pick,
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
            "sweep: discarded stale verdict for {} at file-edit revision {}",
            row.folder_path, expected_edit_revision
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
    token: &CancellationToken,
) -> bool {
    let TerminalVerdict::Found { matches, .. } = verdict else {
        return true;
    };
    let [only_match] = matches.as_mut_slice() else {
        return true;
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
        _ = token.cancelled() => return false,
        payloads = settle => payloads,
    };
    let payloads = match payloads {
        Ok(payloads) => payloads,
        Err(error) => {
            debug!(
                "sweep: could not settle {} ({error}); writing no row",
                only_match.release_id
            );
            return false;
        }
    };
    match payloads.source_tracks() {
        Ok(source_tracks) => {
            // `SourceTracks::Nothing` is an answer — this release states no
            // tracklist — so the verdict stores with the match unverifiable, and
            // the Ready rule lands it in Needs you rather than admitting it.
            only_match.source_tracks = Some(source_tracks);
            true
        }
        Err(error) => {
            debug!(
                "sweep: {} states no readable tracklist ({error}); writing no row",
                only_match.release_id
            );
            false
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
pub(super) async fn record_selection_verdict(
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
    let mut entry = SelectionInFlight {
        candidate,
        probed_total_duration_ms: 0,
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
                entry.probed_total_duration_ms = signals.probed_total_duration_ms;
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
                if !settle_lead(context, &mut verdict, token).await {
                    continue;
                }
                if save(
                    context,
                    token,
                    &candidate_key,
                    &entry.candidate.files.content_hash(),
                    &entry.candidate.path.to_string_lossy(),
                    &verdict,
                    entry.probed_total_duration_ms,
                    entry.candidate.file_edit_revision,
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
                    "sweep: selection recorder for {candidate_key} lagged by {n} events; writing no verdict"
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
