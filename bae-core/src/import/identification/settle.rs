//! Turning one run's terminal answer into a stored row: the documents of the
//! pressing it matched, the draft they project into, and the write.

use super::*;

/// What became of one run's answer.
#[derive(Debug)]
pub(super) enum Settled {
    /// The row landed.
    Stored {
        /// Whether the verdict picked its release unattended, which
        /// automatic import takes.
        picked_unattended: bool,
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
    /// No write was asked for: the release the verdict picks could not be
    /// read into the folder's draft, which another run would fail the same
    /// way.
    Inapplicable { error: String },
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

enum SettledLead {
    NoExternalRelease,
    ExternalRelease {
        link: crate::import::ReleaseLink,
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
        Settled::Unwritable { error } => context.import.fail_identification(
            &representative_key,
            run,
            crate::import::SaveFailure::NotWritten {
                error: error.clone(),
            },
        ),
        Settled::Inapplicable { error } => context.import.fail_identification(
            &representative_key,
            run,
            crate::import::SaveFailure::Inapplicable {
                error: error.clone(),
            },
        ),
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
    let settled_lead = match settle_lead(context, &mut verdict, priority, token).await {
        Ok(settled_lead) => settled_lead,
        Err(settled) => return settled,
    };

    let pick = match settled_lead {
        SettledLead::NoExternalRelease => None,
        SettledLead::ExternalRelease {
            link,
            release,
            partners,
        } => match lead_metadata(context, candidate, &durations, &release, partners).await {
            Ok(metadata) => Some(crate::db::VerdictPick { link, metadata }),
            Err(settled) => return settled,
        },
    };
    save(context, token, run, candidate, &verdict, signals, pick).await
}

/// The draft the picked release reads into, with the artist images and cover
/// it needs, or what becomes of the answer when there is none to write.
async fn lead_metadata(
    context: &Context,
    candidate: &FolderCandidate,
    durations: &crate::import::probe::SourceDurations,
    release: &crate::import::source_release::SourceRelease,
    partners: Vec<crate::import::source_release::SourceRelease>,
) -> Result<crate::import::CandidateMetadataDraft, Settled> {
    let current = match context
        .library_manager
        .load_import_candidate_preparation(&candidate.files.content_hash())
        .await
    {
        Ok(Some(current)) => current,
        Ok(None) => {
            return Err(Settled::Unwritable {
                error: format!("{} has no stored draft", candidate.key()),
            })
        }
        Err(error) => {
            return Err(Settled::Unwritable {
                error: error.to_string(),
            })
        }
    };
    // Files decided otherwise since the run read them make this an answer
    // for files the candidate no longer has.
    if current.file_edit_revision != candidate.file_edit_revision {
        return Err(Settled::Abandoned);
    }
    let source_draft = context
        .import
        .external_candidate_draft(release, durations, &current.draft)
        .map_err(|error| {
            tracing::error!(
                "identification: {} matched a release its draft cannot be read from: {error}",
                candidate.key()
            );
            Settled::Inapplicable {
                error: error.to_string(),
            }
        })?;
    context
        .import
        .external_candidate_assets(source_draft, release, partners, durations, &current.draft)
        .await
        .map_err(|error| Settled::Unwritable {
            error: error.to_string(),
        })
}

/// Write one row, unless the answer was given up right before the write.
pub(super) async fn save(
    context: &Context,
    token: &CancellationToken,
    run: IdentifyRunId,
    candidate: &FolderCandidate,
    verdict: &TerminalVerdict,
    signals: crate::signals::Signals,
    pick: Option<crate::db::VerdictPick>,
) -> Settled {
    if token.is_cancelled() {
        return Settled::Abandoned;
    }
    let candidate_key = candidate.key();
    // Only the verdict's unattended pick is ever applied, so an applied
    // release is that pick, which automatic import then takes.
    let picked_unattended = pick.is_some();
    let row = NewImportCandidateVerdict {
        content_hash: candidate.files.content_hash(),
        file_edit_revision: candidate.file_edit_revision,
        folder_path: candidate_key.clone(),
        verdict: verdict.clone(),
        signals,
        pick,
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
    Settled::Stored { picked_unattended }
}

/// Settle a candidate's lead: the stored releases of the pressing the verdict
/// picks unattended, primary and partners, which the run already fetched and
/// stored. A verdict that picks none stands as the run found it, for the
/// person to pick from or not. A provider that fails to answer for the lead
/// fails the verdict as a lookup; anything else that stops the settle is what
/// becomes of the answer.
async fn settle_lead(
    context: &Context,
    verdict: &mut TerminalVerdict,
    priority: CallPriority,
    token: &CancellationToken,
) -> Result<SettledLead, Settled> {
    let TerminalVerdict::Found {
        findings,
        track_count,
        ..
    } = &*verdict
    else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let Ok(pick) = crate::identify::unattended_pick(
        &findings.matches,
        &findings.pressings,
        findings.medium_conflict,
        *track_count,
    ) else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let pressing = pick.pressing();
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
        _ = token.cancelled() => return Err(Settled::Abandoned),
        prepared = settle => prepared,
    };
    let (release, prepared_partners) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let Some(failure) = crate::import::search::provider_failure(&error) else {
                return Err(Settled::Unwritable {
                    error: format!("could not settle {}: {error}", primary.key),
                });
            };
            debug!(
                "identification: could not settle {} ({error}); storing the failure",
                primary.key
            );
            verdict.fail(crate::identify::IdentifyFailure::ReleaseDetails(failure));
            return Ok(SettledLead::NoExternalRelease);
        }
    };
    Ok(SettledLead::ExternalRelease {
        link: pressing.pick(),
        release,
        partners: prepared_partners,
    })
}
