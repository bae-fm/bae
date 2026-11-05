//! What one settled answer does to the job it answered, and the import it
//! starts.

use super::*;

/// What one settled answer does to the job it answered.
pub(super) async fn finish(context: &Context, queue: &mut Queue, done: Finished) {
    let Some(index) = queue.settling_job(&done.identity, &done.representative_key, done.run) else {
        debug!(
            "identification: {} settled for a job the queue has moved off",
            done.representative_key
        );
        // The Import was asked of this run, which another has replaced.
        if matches!(done.settled, Settled::Stored { import_asked: true, .. }) {
            info!(
                "identification: {}'s run was replaced; the Import asked of it is dropped",
                done.representative_key
            );
        }
        return;
    };
    let job = queue.jobs.remove(index).expect("the located job exists");
    let marks = job.marks();
    for (key, _) in &marks {
        super::withdraw(context, key).await;
    }
    match done.settled {
        Settled::Stored { picked_unattended, import_asked } => {
            info!("identification: stored the verdict for {}", done.representative_key);
            // A stored row with no pick, its read having failed, imports
            // nothing, whoever asked.
            if picked_unattended {
                import_when_identified(context, &done.representative_key, import_asked).await;
            } else if import_asked {
                info!(
                    "identification: {} stored no pick; the Import asked of it is dropped",
                    done.representative_key
                );
            }
        }
        // Nothing was stored, and nothing runs again on its own.
        Settled::Refused | Settled::Abandoned => {
            info!("identification: {} stored no answer; its job is over", done.representative_key);
        }
        Settled::WriteFailed { error } | Settled::Unwritable { error } => {
            for (key, files) in &marks {
                super::record(
                    context,
                    &format!("record that {key}'s answer was not written"),
                    context
                        .import
                        .fail_identification(
                            key,
                            done.run,
                            files,
                            crate::SaveFailure::NotWritten { error: error.clone() },
                        )
                        .await,
                );
            }
        }
        Settled::Inapplicable { error } => {
            for (key, files) in &marks {
                super::record(
                    context,
                    &format!("record that {key}'s answer could not be read into its draft"),
                    context
                        .import
                        .fail_identification(
                            key,
                            done.run,
                            files,
                            crate::SaveFailure::Inapplicable { error: error.clone() },
                        )
                        .await,
                );
            }
        }
    }
}

/// Import `key`, whose run just stored an auto-importable verdict, when
/// "Import automatically when identified" is on or a person asked for the
/// import while the release was read. Whether the run started on its own or a
/// person asked for it does not matter.
async fn import_when_identified(context: &Context, key: &crate::CandidateKey, import_asked: bool) {
    if !import_asked {
        match bae_preferences::preferences(&context.store).await {
            Ok(prefs) if prefs.identification.import_when_identified => {}
            Ok(_) => return,
            Err(error) => {
                warn!(
                    "identification: {key} is not imported once identified: the setting \
                     could not be read ({error})"
                );
                return;
            }
        }
    }
    match context.import.import_identified(key).await {
        Ok(crate::ImportStart::Queued { import_id }) => {
            info!("identification: importing {key} as {import_id} now that it is identified")
        }
        Ok(crate::ImportStart::WhenPickStored) => {
            info!("identification: {key} has another pick being read, which it imports once stored")
        }
        Err(error) => {
            warn!("identification: the import of {key} once identified could not start: {error}")
        }
    }
}

#[cfg(test)]
#[path = "finish_tests.rs"]
mod tests;
