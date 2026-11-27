//! The streamed pass over a candidate's artwork: a snapshot before the first
//! image and after each image but the last, then the settled one. Images are
//! read several at a time, bounded by the service's image-read permits, and
//! taken in their order.

use super::{
    emit_signals, Artwork, ArtworkPass, ExtractionInputs, ExtractionServiceInner, Gathered,
    RunningExtraction, SignalsSnapshot,
};
use crate::extraction::analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::extraction::candidate_text::{Source, SourcedLine};
use crate::extraction::fast_pass::ArtworkImage;
use crate::extraction::release::read_library_image;
use crate::left_out::{FolderInputs, InputRead, LeftOut};
use bae_identify::signals::{
    ArtworkScan, BarcodeSignal, PaneSignals, Signals, SourcedValue, TextSignal,
};
use coven::task::CompletingTask;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, TryAcquireError};
use tokio_util::sync::CancellationToken;

/// One image's reading, as its failure names it.
const READ_IMAGE: &str = "read an image's text";

/// One image being read on the blocking pool, holding one of the service's
/// image-read permits until the analyzer returns. A read its pass stops
/// waiting for runs to its end, and its panic goes to the crash reporter.
struct ImageRead<'a> {
    image: &'a ArtworkImage,
    task: CompletingTask<Result<ArtworkAnalysis, LeftOut>>,
}

impl<'a> ImageRead<'a> {
    /// Take the image's bytes and hand them to the analyzer on the blocking
    /// pool. A library image's bytes come from coven here, in one place of
    /// the open-files budget; a folder's image opens in its own place on the
    /// pool.
    async fn start(
        inner: &ExtractionServiceInner,
        analyzer: &Arc<dyn ArtworkAnalyzer>,
        image: &'a ArtworkImage,
        permit: OwnedSemaphorePermit,
    ) -> Self {
        let analyzer = analyzer.clone();
        let analyze = move |bytes: Vec<u8>| {
            analyzer
                .analyze(&bytes)
                .map_err(|unreadable| LeftOut::unreadable_image(unreadable.detail))
        };
        let task = match image {
            ArtworkImage::Library { file, .. } => {
                let bytes = read_library_image(&inner.store, &inner.records, &inner.files, file)
                    .await
                    .map_err(LeftOut::from);
                coven::task::blocking_completing(
                    READ_IMAGE,
                    inner.diagnostics.bug_sink(),
                    move || {
                        let analysis = bytes.and_then(analyze);
                        drop(permit);
                        analysis
                    },
                )
            }
            ArtworkImage::OnDisk { path, .. } => {
                let path = path.clone();
                let lease = bae_store::lease_one(&inner.files).await;
                coven::task::blocking_completing(
                    READ_IMAGE,
                    inner.diagnostics.bug_sink(),
                    move || {
                        let analysis = read_image_file(&path, &lease)
                            .map_err(|error| LeftOut::io(&error))
                            .and_then(analyze);
                        drop(permit);
                        analysis
                    },
                )
            }
        };
        ImageRead { image, task }
    }

    /// What the analyzer read off the image, or `None` where it could not be
    /// read or its read panicked, which leaves the image out through
    /// `inputs`.
    async fn finish(
        self,
        inputs: &FolderInputs,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Option<ArtworkAnalysis> {
        match self.task.await {
            Ok(Ok(analysis)) => Some(analysis),
            Ok(Err(why)) => {
                inputs.leave_out(InputRead::ArtworkImage, self.image.name(), why);
                None
            }
            Err(failed) => {
                let failed = diagnostics.bug(failed);
                inputs.leave_out_unfinished(InputRead::ArtworkImage, self.image.name(), &failed);
                None
            }
        }
    }
}

/// The bytes of the image file at `path`, opened in a place of `lease`.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: reads an image of the person's folder before coven records it"
)]
fn read_image_file(path: &std::path::Path, lease: &coven::FileLease) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let file = lease.place_blocking().open(path)?;
    let mut bytes = Vec::new();
    (&*file).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Wait out the reads a pass that is stopping no longer wants, so none
/// outlives it: the files a pass reads (a library release's staged cover)
/// last only until it returns. A read that panicked is reported to
/// `diagnostics`.
async fn abandon(reads: VecDeque<ImageRead<'_>>, diagnostics: &bae_diagnostics::Diagnostics) {
    for read in reads {
        if let Err(failed) = read.task.await {
            diagnostics.bug(failed);
        }
    }
}

/// Stream snapshots over the artwork pass — one before the first image and
/// one after each image but the last — then send the settled one, kept first
/// under `settled_key`, where every input was read, so a run started by
/// anything that heard it reuses it.
pub(super) async fn stream_extraction(
    inner: &Arc<ExtractionServiceInner>,
    extraction: &RunningExtraction,
    token: &CancellationToken,
    settled_key: Option<String>,
    extraction_inputs: ExtractionInputs,
) {
    let ExtractionInputs { mut gathered, artwork, inputs: reads } = extraction_inputs;
    let finished = match &artwork {
        Artwork::Absent => ArtworkScan::Absent,
        Artwork::Read(pass) => ArtworkScan::Done { total: pass.images.len() as u32 },
    };
    let artwork = match artwork {
        Artwork::Read(pass) => Some(pass),
        Artwork::Absent => None,
    };
    let total = artwork.as_ref().map_or(0, |pass| pass.images.len() as u32);
    let has_artwork = artwork.is_some();
    let position_of = |images: &[ArtworkImage], index: usize| ArtworkScan::Reading {
        current: images[index].file_id().map(str::to_string),
        position: index as u32 + 1,
        total,
    };

    if token.is_cancelled() {
        return;
    }

    // Without artwork the settled snapshot is the only one, so `Scanning`
    // always means images are being read.
    if let Some(pass) = &artwork {
        let classification = gathered.pool.classify();
        emit_signals(
            inner,
            extraction,
            SignalsSnapshot {
                signals: scanning_signals(
                    &gathered,
                    classification.catalogs,
                    classification.free_text,
                ),
                audio: gathered.audio.clone(),
                artwork: position_of(&pass.images, 0),
            },
        );
    }

    // The pass reads as many images at once as the service's permits allow,
    // and takes their readings in the images' order, so each snapshot holds
    // exactly the images before it whatever order the reads finish in.
    if let Some(ArtworkPass { analyzer, images }) = artwork {
        let mut unstarted = images.iter().peekable();
        let mut in_flight: VecDeque<ImageRead<'_>> = VecDeque::new();
        let mut index = 0;
        loop {
            // Start every image a permit is free for. A pass waits for a
            // permit only while none of its reads are under way, so a reading
            // that has come back never waits behind other passes' reads. A
            // cancelled pass starts no image, even when a permit frees in the
            // same moment it is cancelled.
            while let Some(&image) = unstarted.peek() {
                if token.is_cancelled() {
                    abandon(in_flight, &inner.diagnostics).await;
                    return;
                }
                let permit = if in_flight.is_empty() {
                    tokio::select! {
                        biased;
                        () = token.cancelled() => return,
                        permit = inner.image_reads.clone().acquire_owned() => {
                            permit.expect("the image-read permits are never closed")
                        }
                    }
                } else {
                    match inner.image_reads.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(TryAcquireError::NoPermits) => break,
                        Err(TryAcquireError::Closed) => {
                            unreachable!("the image-read permits are never closed")
                        }
                    }
                };
                unstarted.next();
                in_flight.push_back(ImageRead::start(inner, &analyzer, image, permit).await);
            }
            let Some(read) = in_flight.pop_front() else {
                break;
            };
            let image = read.image;
            let path = image.name();
            let file_id: Option<String> = image.file_id().map(str::to_string);
            let analysis = read.finish(&reads, &inner.diagnostics).await;
            if token.is_cancelled() {
                abandon(in_flight, &inner.diagnostics).await;
                return;
            }
            // An image left out adds nothing, and the pass goes on.
            if let Some(analysis) = analysis {
                // One sighting per image a code was read off.
                for code in analysis.codes() {
                    let seen_here = gathered
                        .barcodes
                        .iter()
                        .any(|b| b.value == code.as_str() && b.origin_path == file_id);
                    if !seen_here {
                        gathered.barcodes.push(SourcedValue {
                            value: code.into_string(),
                            origin_path: file_id.clone(),
                        });
                    }
                }
                for text in analysis.text_lines {
                    gathered
                        .pool
                        .push(SourcedLine::new(Source::Artwork { path: path.to_path_buf() }, text));
                }
            }
            index += 1;

            // The last image's snapshot is the settled one below. Every other
            // image read is a snapshot, whether or not it added anything.
            if index < images.len() {
                let classification = gathered.pool.classify();
                emit_signals(
                    inner,
                    extraction,
                    SignalsSnapshot {
                        signals: scanning_signals(
                            &gathered,
                            classification.catalogs,
                            classification.free_text,
                        ),
                        audio: gathered.audio.clone(),
                        artwork: position_of(&images, index),
                    },
                );
            }
        }
    }

    if token.is_cancelled() {
        return;
    }

    let settled_key = settled_key.filter(|_| reads.all_read());
    let classification = gathered.pool.classify();
    let barcode = if has_artwork || !gathered.barcodes.is_empty() {
        BarcodeSignal::Settled { codes: gathered.barcodes }
    } else {
        BarcodeSignal::Absent
    };
    let settled = SignalsSnapshot {
        signals: Signals {
            pane: PaneSignals {
                origin: gathered.origin,
                disc_id: gathered.disc_id,
                barcode,
                text: TextSignal::Settled {
                    catalogs: classification.catalogs,
                    free_text: classification.free_text,
                },
                isrcs: gathered.isrcs,
                track_titles: gathered.track_titles,
            },
            text_pool: gathered.pool.text_lines(),
        },
        audio: gathered.audio,
        artwork: finished,
    };
    if let Some(key) = settled_key {
        inner.settled.put(key, settled.clone());
    }
    emit_signals(inner, extraction, settled);
}

/// What has been read so far while the artwork pass is still going.
fn scanning_signals(gathered: &Gathered, catalogs: Vec<String>, free_text: Vec<String>) -> Signals {
    let text_pool = gathered.pool.text_lines();
    Signals {
        pane: PaneSignals {
            origin: gathered.origin.clone(),
            disc_id: gathered.disc_id.clone(),
            barcode: BarcodeSignal::Scanning { codes: gathered.barcodes.clone() },
            text: TextSignal::Scanning { catalogs, free_text },
            isrcs: gathered.isrcs.clone(),
            track_titles: gathered.track_titles.clone(),
        },
        text_pool,
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
