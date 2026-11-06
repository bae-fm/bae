//! The work each effect of a run asks: lookups and release reads, run until
//! the run is cancelled.

use super::*;
use bae_catalogs::search::{CatalogNumberSearch, SearchQuery, SourceLookup};
use bae_catalogs::Failure;
use bae_identify::documents::ReleaseToRead;
use bae_identify::state::{Effect, LookupOutcome, PressingKey};

/// What one provider's lookup came to: the provider's answer, or how bae
/// broke on its way there or back — a request it could not make, an answer it
/// could not read.
fn outcome_of(lookup: SourceLookup) -> Result<LookupOutcome, InternalFailure> {
    match lookup {
        Ok(results) => Ok(Ok(results)),
        Err(Failure::Lookup(failure)) => Ok(Err(failure)),
        Err(Failure::Internal(failure)) => Err(failure),
    }
}

/// Send what a lookup came to: `answered` with the provider's answer, or the
/// run's end where bae broke.
fn emit_answer(
    tx: &mpsc::UnboundedSender<IdentifyEvent>,
    answer: Result<LookupOutcome, InternalFailure>,
    answered: impl FnOnce(LookupOutcome) -> IdentifyEvent,
) {
    match answer {
        Ok(outcome) => emit_step(tx, answered(outcome)),
        Err(failure) => emit_step(tx, IdentifyEvent::Broke { failure }),
    }
}

/// Look up a disc ID on MusicBrainz, keeping each match's document as the
/// answer states it. Empty when
/// MB has no hits — which the reducer treats as a settled signal with zero
/// results, ready for combine, exactly like a barcode that matched nothing.
async fn lookup_and_resolve(
    disc_id: &str,
    catalogs: &crate::CatalogSessions,
    class: CallClass,
) -> Result<bae_identify::DiscIdAnswer, Failure> {
    let (results, documents) =
        catalogs.musicbrainz().lookup_musicbrainz_discid(disc_id, class).await?.into_iter().unzip();
    Ok(bae_identify::DiscIdAnswer { results, documents })
}

/// Run one effect's work on the runtime until it ends or the run is cancelled.
///
/// A cancelled run's lookup is dropped where it stands — mid-request, or
/// between a provider's retries while it waits out a busy server — so it
/// neither keeps asking a provider nobody is waiting on nor takes the
/// rate-limit slots the next run's requests need.
///
/// Work that panics is reported, and ends the run as broken on `event_tx`,
/// so the driver waiting on its answer goes on.
fn spawn_until_cancelled(
    runtime: &tokio::runtime::Handle,
    diagnostics: &bae_diagnostics::Diagnostics,
    token: &CancellationToken,
    event_tx: &mpsc::UnboundedSender<IdentifyEvent>,
    work: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let token = token.clone();
    let event_tx = event_tx.clone();
    diagnostics.spawn_reported(
        runtime,
        "run an identify lookup",
        async move {
            token.run_until_cancelled(work).await;
        },
        move |failed| {
            emit_step(&event_tx, IdentifyEvent::Broke { failure: failed.into() });
        },
    );
}

pub(super) fn dispatch_effect(
    inner: Arc<IdentifyServiceInner>,
    effect: Effect,
    class: CallClass,
    event_tx: mpsc::UnboundedSender<IdentifyEvent>,
    token: CancellationToken,
) {
    let runtime = inner.runtime_handle.clone();
    // Where an effect that panics says the run broke.
    let broke_tx = event_tx.clone();
    match effect {
        Effect::LookupDiscid { disc_id } => {
            let catalogs = inner.catalogs.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let event = match lookup_and_resolve(&disc_id, &catalogs, class).await {
                    Ok(answer) => {
                        IdentifyEvent::DiscidLookupAnswered { disc_id, outcome: Ok(answer) }
                    }
                    Err(Failure::Lookup(failure)) => {
                        IdentifyEvent::DiscidLookupAnswered { disc_id, outcome: Err(failure) }
                    }
                    Err(Failure::Internal(failure)) => IdentifyEvent::Broke { failure },
                };
                emit_step(&event_tx, event);
            });
        }

        Effect::LookupIsrcs { isrcs } => {
            let catalogs = inner.catalogs.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let lookup = catalogs.musicbrainz().lookup_musicbrainz_isrcs(&isrcs, class).await;
                let answer = outcome_of(lookup);
                emit_answer(&event_tx, answer, |outcome| IdentifyEvent::IsrcLookupAnswered {
                    outcome,
                });
            });
        }

        // Each provider is asked on its own task and answers for itself, so
        // one landing never waits on the other.
        Effect::LookupBarcode { source, barcode } => {
            let catalogs = inner.catalogs.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let query = SearchQuery::Barcode { barcode: barcode.clone() };
                let lookup = async {
                    let session = catalogs.search_session(source).await.map_err(|error| {
                        crate::ImportError::from(error).failure(&format!(
                            "asking {} a search for {:?}",
                            source.as_str(),
                            &query
                        ))
                    })?;
                    bae_catalogs::search::search_source(&session, source, &query, class).await
                }
                .await;
                let answer = outcome_of(lookup);
                emit_answer(&event_tx, answer, |outcome| IdentifyEvent::BarcodeLookupAnswered {
                    source,
                    for_barcode: barcode,
                    outcome,
                });
            });
        }

        // The one query every provider is asked, once the identifiers have
        // come back with nothing — the same query the Search section's General
        // tab sends.
        Effect::SearchTitle { source, query } => {
            let catalogs = inner.catalogs.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let search = SearchQuery::General {
                    artist: query.artist.clone(),
                    album: query.album.clone(),
                };
                let lookup = async {
                    let session = catalogs.search_session(source).await.map_err(|error| {
                        crate::ImportError::from(error).failure(&format!(
                            "asking {} a search for {:?}",
                            source.as_str(),
                            &search
                        ))
                    })?;
                    bae_catalogs::search::search_source(&session, source, &search, class).await
                }
                .await;
                let answer = outcome_of(lookup);
                emit_answer(&event_tx, answer, |outcome| IdentifyEvent::SearchAnswered {
                    source,
                    outcome,
                });
            });
        }

        // Each record is read through the one place a pick reads it from, as
        // far as ranking it needs; picking an offered row reads the rest.
        Effect::ReadReleases { releases, audio } => {
            let release_reads = inner.release_reads.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let mut read = Vec::with_capacity(releases.len());
                for release in releases {
                    let Some(stored) =
                        read_release(&release_reads, &release, class, &event_tx).await
                    else {
                        return;
                    };
                    let document = stored
                        .map(|stored| bae_identify::documents::ReleaseDocument::of(stored, &audio));
                    read.push(bae_identify::documents::ReleaseReading {
                        release: release.release().clone(),
                        document,
                    });
                }
                emit_step(&event_tx, IdentifyEvent::ReleasesRead { read });
            });
        }

        // A link is read the way an offered record is, and what it reads is
        // checked against the library as a search's answer is; a barcode or a
        // catalog number is searched on the one catalog.
        Effect::LookupPressing { source, key } => {
            let release_reads = inner.release_reads.clone();
            let catalogs = inner.catalogs.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let lookup = match &key {
                    PressingKey::Link { release } => {
                        let read = ReleaseToRead::ReleaseLookup(release.clone());
                        let Some(stored) =
                            read_release(&release_reads, &read, class, &event_tx).await
                        else {
                            return;
                        };
                        // Read, not held: the documents read for the
                        // rows the run offers hold what it stores.
                        stored
                            .map(|stored| {
                                vec![bae_catalogs::search::MetadataResult::of_release(
                                    &stored.release,
                                )]
                            })
                            .map_err(Failure::Lookup)
                    }
                    PressingKey::Barcode { barcode } => {
                        let query = SearchQuery::Barcode { barcode: barcode.clone() };
                        async {
                            let session =
                                catalogs.search_session(source).await.map_err(|error| {
                                    crate::ImportError::from(error).failure(&format!(
                                        "asking {} a search for {:?}",
                                        source.as_str(),
                                        &query
                                    ))
                                })?;
                            bae_catalogs::search::search_source(&session, source, &query, class)
                                .await
                        }
                        .await
                    }
                    PressingKey::CatalogNumber { number, label, artist } => {
                        let query = CatalogNumberSearch {
                            number: number.clone(),
                            artist: artist.clone(),
                            label: Some(label.clone()),
                        };
                        async {
                            let session =
                                catalogs.search_session(source).await.map_err(|error| {
                                    crate::ImportError::from(error).failure(&format!(
                                        "asking {} a search for {:?}",
                                        source.as_str(),
                                        &query
                                    ))
                                })?;
                            bae_catalogs::search::search_source(&session, source, &query, class)
                                .await
                        }
                        .await
                    }
                };
                let answer = outcome_of(lookup);
                emit_answer(&event_tx, answer, |outcome| IdentifyEvent::PressingLookupAnswered {
                    source,
                    key,
                    outcome,
                });
            });
        }

        // What was read stays true whether or not the run stays current, so
        // cancelling the run does not stop it being kept. Keeping it, so a
        // stored release of either album names the other among its records,
        // has no caller waiting: a write that fails is a bug, and the list's
        // answer stands.
        Effect::KeepAlbumLinks { kept } => {
            let tables = inner.tables.clone();
            let diagnostics = inner.diagnostics.clone();
            let keep = async move {
                if let Err(error) = tables.replace_group_album_links(kept).await {
                    diagnostics.report_bug("keep what a run read of albums", &error);
                }
            };
            #[cfg(test)]
            let keep = inner.driver_tasks.track_future(keep);
            // Nothing waits on it; a panic is reported and keeps nothing.
            inner.diagnostics.spawn_reported(
                &runtime,
                "keep what a run read of albums",
                keep,
                |_| {},
            );
        }

        // A number in effect is searched under the artist it is narrowed to,
        // and never under a label: the catalog searched may spell the label
        // otherwise than the release that carries the number.
        Effect::LookupCatalog { source, catalog, artist } => {
            let catalogs = inner.catalogs.clone();
            spawn_until_cancelled(&runtime, &inner.diagnostics, &token, &broke_tx, async move {
                let query = CatalogNumberSearch {
                    number: catalog.clone(),
                    artist: artist.clone(),
                    label: None,
                };
                let lookup = async {
                    let session = catalogs.search_session(source).await.map_err(|error| {
                        crate::ImportError::from(error).failure(&format!(
                            "asking {} a search for {:?}",
                            source.as_str(),
                            &query
                        ))
                    })?;
                    bae_catalogs::search::search_source(&session, source, &query, class).await
                }
                .await;
                let answer = outcome_of(lookup);
                emit_answer(&event_tx, answer, |outcome| IdentifyEvent::CatalogLookupAnswered {
                    source,
                    for_catalog: catalog,
                    artist,
                    outcome,
                });
            });
        }
    }
}

/// `read`'s release from storage or read where `read` says, as far as ranking
/// it needs, or why its catalog could not give it. `None` when bae broke
/// reading it: the run has then been told, and ends.
async fn read_release(
    release_reads: &crate::ReleaseReads,
    read: &ReleaseToRead,
    class: CallClass,
    event_tx: &mpsc::UnboundedSender<IdentifyEvent>,
) -> Option<Result<bae_catalogs::source_release::PreparedRelease, bae_catalogs::LookupFailure>> {
    let prepared = match read {
        ReleaseToRead::ReleaseLookup(release) => {
            release_reads
                .release(release, bae_catalogs::payloads::ReadDepth::Identification, class)
                .await
        }
        ReleaseToRead::DiscIdAnswer(answered) => {
            release_reads.disc_id_release(answered, class).await
        }
    };
    let error = match prepared {
        Ok(stored) => return Some(Ok(stored)),
        Err(error) => error,
    };
    let release = read.release();
    match error.failure(&format!("reading {} release {}", release.catalog.as_str(), release.key)) {
        Failure::Lookup(failure) => Some(Err(failure)),
        Failure::Internal(failure) => {
            emit_step(event_tx, IdentifyEvent::Broke { failure });
            None
        }
    }
}

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
