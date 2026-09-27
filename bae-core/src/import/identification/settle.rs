//! Turning one run's terminal answer into a stored row: the documents of the
//! pressing it matched, the draft they project into, and the write.

use super::*;

/// What became of one run's answer.
#[derive(Debug)]
pub(super) enum Settled {
    /// The row landed, classified by the Ready rule.
    Stored {
        classification: crate::identify::QueueClassification,
    },
    /// The write ran and refused the answer: the candidate can no longer be
    /// answered, or its files are not the ones the run read.
    Refused,
    /// The write ran and did not land.
    WriteFailed { error: String },
    /// No write was asked for: the answer was given up.
    Abandoned,
    /// No write was asked for: the answer could not be turned into a row.
    Unwritable { error: String },
}

/// What one settled answer reports back to the driver loop.
pub(super) struct Finished {
    /// The identity of the job the answer settles.
    pub(super) identity: CandidateIdentity,
    pub(super) representative_key: String,
    /// The run whose answer this is.
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

/// Settle one run's terminal answer and say what became of it, ending the
/// runtime's pending save for an answer that never reached a write.
pub(super) async fn settle_answer(
    context: Context,
    identity: CandidateIdentity,
    candidate: FolderCandidate,
    run: IdentifyRunId,
    state: IdentifyState,
    priority: CallPriority,
    token: CancellationToken,
) -> Finished {
    let representative_key = candidate.key();
    let settled = settle_verdict(
        &context,
        &candidate,
        run,
        state,
        priority,
        &token,
    )
    .await;
    match &settled {
        // A write that ran ended its own pending save.
        Settled::Stored { .. } | Settled::Refused | Settled::WriteFailed { .. } => {}
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

/// Turn one candidate's terminal state into a write, or say why none ran.
async fn settle_verdict(
    context: &Context,
    candidate: &FolderCandidate,
    run: IdentifyRunId,
    state: IdentifyState,
    priority: CallPriority,
    token: &CancellationToken,
) -> Settled {
    let text = state.candidate_text();
    let durations = state
        .audio()
        .expect("a terminal state carries the audio it was identified over")
        .durations
        .clone();
    let mut verdict = TerminalVerdict::try_from(state)
        .expect("the queue settles only terminal identify states");

    // This run's own snapshot, not another run's of the same candidate.
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
        &durations,
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
        &durations,
        settled_lead,
        &mut verdict,
    )
    .await;
    save(context, token, run, candidate, &verdict, signals, metadata).await
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
            // The failure keeps what the lookups found and their ledger.
            verdict.fail(crate::identify::IdentifyFailure::ReleaseDetails(
                crate::signals::LookupFailure::Diagnostic {
                    detail: error.to_string(),
                },
            ));
            None
        }
    }
}

/// Write one row, unless the answer was given up right before the write.
pub(super) async fn save(
    context: &Context,
    token: &CancellationToken,
    run: IdentifyRunId,
    candidate: &FolderCandidate,
    verdict: &TerminalVerdict,
    signals: crate::signals::Signals,
    metadata: Option<crate::import::CandidateMetadataDraft>,
) -> Settled {
    if token.is_cancelled() {
        return Settled::Abandoned;
    }
    let candidate_key = candidate.key();
    let row = NewImportCandidateVerdict {
        content_hash: candidate.files.content_hash(),
        file_edit_revision: candidate.file_edit_revision,
        folder_path: candidate_key.clone(),
        verdict: verdict.clone(),
        signals,
        metadata,
    };
    let wrote = match context
        .import
        .save_candidate_verdict_if_current(&candidate_key, run, &row)
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
        info!(
            "identification: refused the verdict for {candidate_key}: it can no longer be \
             answered, or its files are not the ones the run read"
        );
        return Settled::Refused;
    }
    Settled::Stored {
        classification: crate::identify::classify(verdict),
    }
}

/// The one pressing a verdict's matches describe, or `None` when they describe
/// several. Rows come from the run's own `pressings`, so two catalogs' records
/// of one pressing are one row, and the row's lead is judged against the
/// candidate's text as the pane judges it.
fn sole_pressing(
    findings: &crate::identify::Findings,
    text: &crate::identify::CandidateText,
) -> Option<crate::import::release_group::Pressing> {
    let judged =
        crate::identify::judged_results(findings.matches.clone(), &findings.provenance, text);
    let mut rows =
        crate::import::release_group::group_formed_rows(judged, &findings.pressings, Vec::new(), &[])
            .into_iter()
            .flat_map(crate::import::release_group::ReleaseGroup::into_pressings);
    let only = rows.next()?;
    rows.next().is_none().then_some(only)
}

/// Settle a candidate's lead: fetch and store the releases of the one pressing
/// it matched, primary and partners, and read the primary's tracklist.
///
/// The releases land before the verdict, so a stored lead opens with no
/// network; a partner that cannot be fetched fails the lead like the primary.
/// Only a `Found` that groups into one pressing has a lead. A release already
/// stored is read back rather than fetched, and `priority` is the run's own.
async fn settle_lead(
    context: &Context,
    verdict: &mut TerminalVerdict,
    text: &crate::identify::CandidateText,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    priority: CallPriority,
    token: &CancellationToken,
) -> Result<SettledLead, FinalizationError> {
    let TerminalVerdict::Found { findings, .. } = verdict else {
        return Ok(SettledLead::NoExternalRelease);
    };
    // The folder's own files rule the releases out: none is applied to the
    // draft, and the person picks from the list or does not.
    if findings.medium_conflict.is_some() {
        return Ok(SettledLead::NoExternalRelease);
    }
    let Some(pressing) = sole_pressing(findings, text) else {
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
            verdict.fail(crate::identify::IdentifyFailure::ReleaseDetails(
                crate::import::search::import_error_to_lookup_failure(&error),
            ));
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
    // `SourceTracks::Nothing` is an answer: the release states no tracklist,
    // which the Ready rule sends to Needs you. The primary's row carries it.
    findings
        .matches
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
