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
        /// Whether a person asked for the candidate to be imported while the
        /// release was read.
        import_asked: bool,
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
    pub(super) identity: CandidateFiles,
    pub(super) representative_key: crate::CandidateKey,
    /// The run whose answer this is.
    pub(super) run: IdentifyRunId,
    pub(super) settled: Settled,
}

enum SettledLead {
    NoExternalRelease,
    ExternalRelease {
        link: bae_identify::PressingLink,
        release: bae_catalogs::source_release::SourceRelease,
        partners: Vec<bae_catalogs::source_release::SourceRelease>,
        /// The primary and the partners as their read stores them, which the
        /// verdict's write stores beside the link naming them.
        stored: Vec<bae_catalogs::source_release::SourceRelease>,
        /// The year the draft's pressing takes where the release states none.
        pressing_year: Option<i32>,
    },
}

/// One settle of an answer, as its failure names it.
pub(super) const SETTLE_ANSWER: &str = "settle an identification answer";

/// What `settle` says became of the answer of `run` for the job `identity`,
/// whose representative is `representative_key`. A settle that panics is
/// reported, and fails the job's identification as a write that did not land
/// does.
pub(super) async fn settled_or_failed(
    context: &Context,
    identity: CandidateFiles,
    representative_key: crate::CandidateKey,
    run: IdentifyRunId,
    settle: impl std::future::Future<Output = Finished>,
) -> Finished {
    bae_util::task_failure::caught(SETTLE_ANSWER, settle).await.unwrap_or_else(|failed| Finished {
        identity,
        representative_key,
        run,
        settled: Settled::WriteFailed { error: context.diagnostics.panicked(failed).to_string() },
    })
}

/// Settle one run's terminal answer and say what became of it, ending the
/// runtime's pending save for an answer that never reached a write.
pub(super) async fn settle_answer(
    context: Context,
    identity: CandidateFiles,
    candidate: FolderCandidate,
    run: IdentifyRunId,
    state: IdentifyState,
    class: CallClass,
    token: CancellationToken,
) -> Finished {
    let representative_key = candidate.key();
    let settled = settle_verdict(&context, &candidate, run, state, class, &token).await;
    let files = CandidateFiles::of(&candidate);
    let ended = match &settled {
        // A write that ran ended its own pending save.
        Settled::Stored { .. } | Settled::Refused | Settled::WriteFailed { .. } => Ok(()),
        Settled::Abandoned => {
            context.import.end_identification_answer(&representative_key, run).await
        }
        Settled::Unwritable { error } => {
            context
                .import
                .fail_identification(
                    &representative_key,
                    run,
                    &files,
                    crate::SaveFailure::NotWritten { error: error.clone() },
                )
                .await
        }
        Settled::Inapplicable { error } => {
            context
                .import
                .fail_identification(
                    &representative_key,
                    run,
                    &files,
                    crate::SaveFailure::Inapplicable { error: error.clone() },
                )
                .await
        }
    };
    super::queue::record(
        &context,
        &format!("end {representative_key}'s answer, which no verdict write ended"),
        ended,
    );
    Finished { identity, representative_key, run, settled }
}

/// Turn one candidate's terminal state into a write, or say why none ran.
async fn settle_verdict(
    context: &Context,
    candidate: &FolderCandidate,
    run: IdentifyRunId,
    state: IdentifyState,
    class: CallClass,
    token: &CancellationToken,
) -> Settled {
    let audio = state.read_audio();
    // The text the run judged its rows against, which the verdict does not
    // keep: what the pick's year falls back on.
    let text = state.candidate_text();
    // The releases the run read, which its verdict's rows name.
    let mut releases = state.read_releases();
    let mut verdict =
        TerminalVerdict::try_from(state).expect("the queue settles only terminal identify states");

    // This run's own signals, not another run's of the same candidate.
    let signals = match context.import.candidate_run_signals(&candidate.key(), run).await {
        Ok(Some(signals)) => signals,
        Ok(None) => {
            return Settled::Unwritable {
                error: format!("{} reached a verdict with no settled signals", candidate.key()),
            }
        }
        Err(error) => return Settled::Unwritable { error: error.to_string() },
    };
    let settled_lead = match settle_lead(context, &mut verdict, &text, class, token).await {
        Ok(settled_lead) => settled_lead,
        Err(settled) => return settled,
    };

    let pick = match settled_lead {
        SettledLead::NoExternalRelease => None,
        SettledLead::ExternalRelease { link, release, partners, stored, pressing_year } => {
            match lead_metadata(
                context,
                candidate,
                &mut verdict,
                &audio.expect(
                    "a terminal state that offers a release read it against measured audio",
                ),
                &release,
                partners,
                pressing_year,
            )
            .await
            {
                Ok(metadata) => {
                    // Read in full, so stored over the run's copies where those
                    // stopped short.
                    releases.extend(stored);
                    metadata.map(|metadata| crate::tables::VerdictPick { link, metadata })
                }
                Err(settled) => return settled,
            }
        }
    };
    save(context, token, run, candidate, &verdict, signals, pick, releases).await
}

/// The draft the picked release reads into, with the cover it is read with,
/// or what becomes of the answer when there is none to write.
///
/// A catalog that fails to give the cover fails `verdict` as a lookup, naming
/// the catalog, and there is no draft: the verdict is stored with the failure
/// and without the pick, so the row stands as a lookup error to retry rather
/// than an applied release with its cover missing.
async fn lead_metadata(
    context: &Context,
    candidate: &FolderCandidate,
    verdict: &mut TerminalVerdict,
    audio: &bae_catalogs::reading::ReadAudio,
    release: &bae_catalogs::source_release::SourceRelease,
    partners: Vec<bae_catalogs::source_release::SourceRelease>,
    pressing_year: Option<i32>,
) -> Result<Option<crate::CandidateMetadataDraft>, Settled> {
    let current = match crate::preparations::stored_preparation(
        &context.tables,
        &candidate.files.content_hash(),
    )
    .await
    {
        Ok(Some(current)) => current,
        Ok(None) => {
            return Err(Settled::Unwritable {
                error: format!("{} has no stored draft", candidate.key()),
            })
        }
        Err(error) => return Err(Settled::Unwritable { error: error.to_string() }),
    };
    // Files decided otherwise since the run read them make this an answer
    // for files the candidate no longer has.
    if !current.file_edits.is_at_revision(candidate.file_edit_revision) {
        return Err(Settled::Abandoned);
    }
    let source_draft = release
        .read(audio)
        .map_err(crate::ImportError::from)
        .and_then(|values| {
            crate::pane::external_candidate_draft(values, &current.metadata.draft, pressing_year)
        })
        .map_err(|error| {
            tracing::error!(
                "identification: {} matched a release its draft cannot be read from: {error}",
                candidate.key()
            );
            Settled::Inapplicable { error: error.to_string() }
        })?;
    let cover = match context
        .import
        .fetch_release_cover(release, &partners, &crate::CatalogApplySteps::unread())
        .await
    {
        Ok(cover) => cover,
        Err(failure) => {
            let Some(lookup) = bae_catalogs::search::remote_image_failure(&failure.error) else {
                return Err(Settled::Unwritable {
                    error: format!(
                        "could not fetch the {} cover of {}: {}",
                        failure.source.as_str(),
                        release.release().key,
                        failure.error
                    ),
                });
            };
            warn!(
                "identification: could not fetch the {} cover of {} ({}); storing the failure",
                failure.source.as_str(),
                release.release().key,
                failure.error
            );
            verdict.fail(bae_identify::IdentifyFailure::Cover(
                bae_catalogs::search::SourceFailure { source: failure.source, failure: lookup },
            ));
            return Ok(None);
        }
    };
    Ok(Some(crate::external_metadata_with_cover(source_draft, release, cover)))
}

/// Write one row, with the catalog releases it may name, unless the answer
/// was given up right before the write.
#[expect(
    clippy::too_many_arguments,
    reason = "the parts of one verdict write, each from a different step of the settle"
)]
pub(super) async fn save(
    context: &Context,
    token: &CancellationToken,
    run: IdentifyRunId,
    candidate: &FolderCandidate,
    verdict: &TerminalVerdict,
    signals: bae_identify::signals::Signals,
    pick: Option<crate::tables::VerdictPick>,
    releases: Vec<bae_catalogs::source_release::SourceRelease>,
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
        candidate_key: candidate_key.clone(),
        verdict: verdict.clone(),
        signals,
        pick,
        releases,
    };
    match context.import.save_candidate_verdict_if_current(&candidate_key, run, &row).await {
        Ok(crate::VerdictWrite::Stored { import_asked }) => {
            Settled::Stored { picked_unattended, import_asked }
        }
        Ok(crate::VerdictWrite::Refused) => {
            info!(
                "identification: refused the verdict for {candidate_key}: it can no longer be \
                 answered, or its files are not the ones the run read"
            );
            Settled::Refused
        }
        Err(e) => Settled::WriteFailed { error: e.to_string() },
    }
}

/// Settle a candidate's lead: the releases of the pressing the verdict picks
/// unattended, primary and partners, read in full. A verdict that picks none stands as the run found it, for the
/// person to pick from or not. A provider that fails to answer for the lead
/// fails the verdict as a lookup; anything else that stops the settle is what
/// becomes of the answer.
async fn settle_lead(
    context: &Context,
    verdict: &mut TerminalVerdict,
    text: &bae_identify::CandidateText,
    class: CallClass,
    token: &CancellationToken,
) -> Result<SettledLead, Settled> {
    let TerminalVerdict::Found { findings, track_count, .. } = &*verdict else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let Ok(pick) = bae_identify::unattended_pick(
        &findings.matches,
        &findings.pressings,
        findings.medium_conflict,
        *track_count,
    ) else {
        return Ok(SettledLead::NoExternalRelease);
    };
    let pressing = pick.pressing();
    let (primary, partners) = pressing.claims();
    let pressing_year = bae_identify::row_facts::folder_pressing_year(text, &pressing.releases);

    let settle = context.import.read_pick(&primary, &partners, class);
    let prepared = tokio::select! {
        biased;
        // Shutdown is not a provider answer and writes nothing.
        _ = token.cancelled() => return Err(Settled::Abandoned),
        prepared = settle => prepared,
    };
    let (release, prepared_partners) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let Some(failure) = error.provider_failure() else {
                return Err(Settled::Unwritable {
                    error: format!("could not settle {}: {error}", primary.key),
                });
            };
            warn!(
                "identification: could not settle {} ({error}); storing the failure",
                primary.key
            );
            verdict.fail(bae_identify::IdentifyFailure::ReleaseDetails(failure));
            return Ok(SettledLead::NoExternalRelease);
        }
    };
    let stored = std::iter::once(release.stored)
        .chain(prepared_partners.iter().map(|partner| partner.stored.clone()))
        .collect();
    Ok(SettledLead::ExternalRelease {
        link: pressing.pick(),
        release: release.release,
        partners: prepared_partners.into_iter().map(|partner| partner.release).collect(),
        stored,
        pressing_year,
    })
}

#[cfg(test)]
#[path = "settle_tests.rs"]
mod tests;
