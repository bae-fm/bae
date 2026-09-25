//! Turning one run's terminal answer into a stored row: the documents of the
//! pressing it matched, the draft they project into, and the write.
//!
//! One shape for both admissions. What differs between a candidate a person
//! asked for and one the automatic admission picked up is the priority its
//! lookups are dispatched at, and that is a parameter.

use super::*;

/// What became of one run's answer.
#[derive(Debug)]
pub(super) enum Settled {
    /// The write ran and the row landed.
    Stored,
    /// The write ran and refused the answer: the candidate has moved on from
    /// the shape the answer describes.
    Refused,
    /// The write ran and did not land.
    WriteFailed { error: String },
    /// No write was asked for: the queue gave the answer up before one could
    /// be.
    Abandoned,
    /// No write was asked for: the answer could not be turned into a row.
    Unwritable { error: String },
}

/// What one settled answer reports back to the driver loop.
pub(super) struct Finished {
    /// The identity the answer covers — every member of the job it settles.
    pub(super) identity: CandidateIdentity,
    pub(super) representative_key: String,
    /// The run whose answer this is. What the write it asked for leaves in the
    /// candidate runtime is recorded against it.
    pub(super) run: IdentifyRunId,
    pub(super) settled: Settled,
}

enum FinalizationError {
    Superseded,
    Failed(String),
}

enum SettledLead {
    NoExternalRelease,
    ExternalRelease {
        provenance: crate::import::MetadataProvenance,
        release: crate::import::source_release::SourceRelease,
        partners: Vec<crate::import::source_release::SourceRelease>,
    },
}

/// Settle one run's terminal answer and say what became of it.
///
/// The one place an answer that never reached a write ends: the write ends the
/// ones it ran for, and a run left saying a commit is still coming would say it
/// for good.
#[allow(clippy::too_many_arguments)]
pub(super) async fn settle_answer(
    context: Context,
    identity: CandidateIdentity,
    candidate: FolderCandidate,
    run: IdentifyRunId,
    expected_metadata_revision: u64,
    state: IdentifyState,
    priority: CallPriority,
    token: CancellationToken,
) -> Finished {
    let representative_key = candidate.key();
    let settled = settle_verdict(
        &context,
        &candidate,
        run,
        expected_metadata_revision,
        state,
        priority,
        &token,
    )
    .await;
    match &settled {
        // The write ran, and what it left in the runtime is its own to end.
        Settled::Stored | Settled::Refused | Settled::WriteFailed { .. } => {}
        Settled::Abandoned => context
            .import
            .end_identification_answer(&representative_key, run),
        Settled::Unwritable { error } => {
            context
                .import
                .fail_identification(&representative_key, run, error.clone())
        }
    }
    Finished {
        identity,
        representative_key,
        run,
        settled,
    }
}

/// Turn one candidate's terminal state into a stored row, a refused answer, or
/// an answer that never reached a write.
#[allow(clippy::too_many_arguments)]
async fn settle_verdict(
    context: &Context,
    candidate: &FolderCandidate,
    run: IdentifyRunId,
    expected_metadata_revision: u64,
    state: IdentifyState,
    priority: CallPriority,
    token: &CancellationToken,
) -> Settled {
    let text = state.candidate_text();
    let mut verdict = TerminalVerdict::try_from(state)
        .expect("the queue settles only terminal identify states");

    // The snapshot the run was judged against, taken by run rather than by key:
    // a snapshot of another run of the same candidate answers a different
    // question.
    let Some(signals) = context
        .import
        .candidate_run_signals(&candidate.key(), run)
    else {
        return Settled::Unwritable {
            error: format!(
                "{} reached a verdict with no settled signals",
                candidate.key()
            ),
        };
    };
    let settled_lead = match settle_lead(
        context,
        &mut verdict,
        &text,
        candidate,
        &signals.durations,
        priority,
        token,
    )
    .await
    {
        Ok(settled) => settled,
        Err(FinalizationError::Superseded) => return Settled::Abandoned,
        Err(FinalizationError::Failed(error)) => return Settled::Unwritable { error },
    };

    let metadata = metadata_or_failed_verdict(
        context,
        candidate,
        &signals.durations,
        settled_lead,
        &mut verdict,
    )
    .await;
    save(
        context,
        token,
        &candidate.key(),
        run,
        crate::import::CandidateAsRead {
            content_hash: candidate.files.content_hash(),
            file_edit_revision: candidate.file_edit_revision,
            metadata_revision: expected_metadata_revision,
        },
        &candidate.key(),
        &verdict,
        signals,
        metadata,
    )
    .await
}

async fn metadata_for_settled_lead(
    context: &Context,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    settled_lead: SettledLead,
) -> Result<Option<crate::import::CandidateMetadataDraft>, crate::import::ImportError> {
    match settled_lead {
        SettledLead::NoExternalRelease => Ok(None),
        SettledLead::ExternalRelease {
            provenance,
            release,
            partners,
        } => {
            let current = context
                .library_manager
                .load_import_candidate_preparation(&candidate.files.content_hash())
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!("{} has no stored draft", candidate.key()),
                })?;
            Ok(Some(
                context
                    .import
                    .external_candidate_metadata(&release, partners, durations, provenance, &current.draft)
                    .await?,
            ))
        }
    }
}

async fn metadata_or_failed_verdict(
    context: &Context,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    settled_lead: SettledLead,
    verdict: &mut TerminalVerdict,
) -> Option<crate::import::CandidateMetadataDraft> {
    match metadata_for_settled_lead(context, candidate, durations, settled_lead).await {
        Ok(metadata) => metadata,
        Err(error) => {
            warn!(
                "identification: could not project metadata for {} ({error}); storing the failure",
                candidate.key()
            );
            // The lookups ran and showed what they showed; what could not be
            // fetched is the release detail behind the match they settled on.
            // So the run's ledger carries onto the failure that replaces its
            // verdict, rather than the pane losing the run it just watched.
            let (track_count, ledger) = match verdict {
                TerminalVerdict::Found {
                    track_count,
                    ledger,
                    ..
                }
                | TerminalVerdict::Failed {
                    track_count,
                    ledger,
                    ..
                }
                | TerminalVerdict::ManualOnly {
                    track_count,
                    ledger,
                } => (*track_count, ledger.take()),
                TerminalVerdict::NotFoundAnywhere { ledger } => (0, ledger.take()),
            };
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::signals::LookupFailure::Diagnostic {
                        detail: error.to_string(),
                    },
                )],
                track_count,
                ledger,
            };
            None
        }
    }
}

/// Write one row. Cancellation is re-checked immediately before the write, not
/// only before the lookup that precedes it: teardown during that lookup must
/// leave nothing behind, and "a cancelled candidate writes no row" is only true
/// if the last thing checked before writing is the token.
#[allow(clippy::too_many_arguments)]
pub(super) async fn save(
    context: &Context,
    token: &CancellationToken,
    candidate_key: &str,
    run: IdentifyRunId,
    candidate: crate::import::CandidateAsRead,
    folder_path: &str,
    verdict: &TerminalVerdict,
    signals: crate::signals::Signals,
    metadata: Option<crate::import::CandidateMetadataDraft>,
) -> Settled {
    if token.is_cancelled() {
        return Settled::Abandoned;
    }
    let row = NewImportCandidateVerdict {
        candidate,
        folder_path: folder_path.to_string(),
        verdict: verdict.clone(),
        signals,
        metadata,
    };
    let wrote = match context
        .import
        .save_candidate_verdict_if_current(candidate_key, run, &row)
        .await
    {
        Ok(wrote) => wrote,
        Err(e) => {
            return Settled::WriteFailed {
                error: e.to_string(),
            };
        }
    };
    if !wrote {
        // Info rather than debug: a candidate whose answer is refused is run
        // again, so a queue that never finishes reads as this line repeating.
        info!(
            "identification: discarded stale verdict for {} at file-edit revision {} and metadata revision {}",
            row.folder_path, row.candidate.file_edit_revision, row.candidate.metadata_revision
        );
        return Settled::Refused;
    }
    Settled::Stored
}

/// The one pressing a verdict's matches describe, or `None` when they describe
/// several.
///
/// The rows are the run's own — `pressings` says which row each match belongs
/// to — so "how many pressings did this candidate match" is what that run
/// answered rather than a count of result rows, and rather than a grouping of
/// this list alone. A MusicBrainz release and a Discogs release agreeing on a
/// barcode are one row a person picks whole — an answer, not a question.
///
/// The matches are judged against the candidate's own text here, exactly as
/// the pane judges them, because which record of the row leads it is decided
/// by that evidence: the record whose document fills the draft has to be the
/// one a person sees leading the row.
fn sole_pressing(
    matches: &[MetadataResult],
    provenance: &[crate::identify::LookupProvenance],
    pressings: &[u32],
    text: &crate::identify::CandidateText,
) -> Option<crate::import::release_group::Pressing> {
    let judged = crate::identify::judged_results(matches.to_vec(), provenance, text);
    let mut rows = crate::import::release_group::group_formed_rows(judged, pressings, Vec::new(), &[])
        .into_iter()
        .flat_map(crate::import::release_group::ReleaseGroup::into_pressings);
    let only = rows.next()?;
    rows.next().is_none().then_some(only)
}

/// Settle a candidate's lead: fetch the releases that describe the pressing it
/// matched — the primary and every partner — store them, and read the
/// primary's own tracklist out of what came back. Returns whether the verdict
/// may now be stored.
///
/// **The releases land before the verdict does.** A stored verdict whose lead
/// carries a tracklist is the queue's promise that opening that candidate needs
/// no network, and that promise covers every source the pick claims, so a
/// partner that will not prepare fails the lead exactly as the primary does:
/// the candidate stores an explicit failure and no verdict names the pressing.
///
/// Only a `Found` that groups into one pressing has a lead, whichever lookup
/// found it: a lone row from the title search is applied like any other.
/// Several pressings and a conflict
/// are questions for a person, answered from the result rows the verdict
/// already carries, and a full fetch of every pressing on the list would buy a
/// classification that cannot change.
///
/// A release some other candidate already settled costs nothing: its stored
/// release is read back and the tracklist re-derived from it.
///
/// `priority` is the run's own: a candidate a person asked for fetches its lead
/// ahead of the queue's background calls, so the verdict they are watching for
/// does not wait behind a queue nobody is watching.
async fn settle_lead(
    context: &Context,
    verdict: &mut TerminalVerdict,
    text: &crate::identify::CandidateText,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    priority: CallPriority,
    token: &CancellationToken,
) -> Result<SettledLead, FinalizationError> {
    let TerminalVerdict::Found {
        matches,
        provenance,
        pressings,
        track_count,
        ledger,
        ..
    } = verdict
    else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let Some(pressing) = sole_pressing(matches, provenance, pressings, text) else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let (primary, partners) = pressing.claims();

    let settle = async {
        let release =
            crate::import::service::prepare_release(&context.library_manager, &primary, priority)
                .await?;
        let prepared_partners = crate::import::service::prepare_partners(
            &context.library_manager,
            &primary,
            &partners,
            priority,
        )
        .await?;
        Ok::<_, crate::import::ImportError>((release, prepared_partners))
    };
    let prepared = tokio::select! {
        biased;
        // Shutdown is not a provider answer and writes nothing.
        _ = token.cancelled() => return Err(FinalizationError::Superseded),
        prepared = settle => prepared,
    };
    let (release, prepared_partners) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            debug!(
                "identification: could not settle {} ({error}); storing the failure",
                primary.key
            );
            *verdict = TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::ReleaseDetails(
                    crate::import::search::import_error_to_lookup_failure(&error),
                )],
                track_count: *track_count,
                ledger: ledger.take(),
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
    // `SourceTracks::Nothing` is an answer — this release states no
    // tracklist — so the verdict stores with the match unverifiable, and the
    // Ready rule lands it in Needs you rather than admitting it.
    //
    // The tracklist belongs to the primary's own match row: it is read from
    // the primary's release, and a partner states its own.
    matches
        .iter_mut()
        .find(|result| result.source == primary.catalog && result.release_id == primary.key)
        .expect("the pressing's primary is one of the verdict's matches")
        .source_tracks = Some(release.source_tracks_for_audio(&audio_durations));
    Ok(SettledLead::ExternalRelease {
        provenance: pressing.pick(),
        release,
        partners: prepared_partners,
    })
}
