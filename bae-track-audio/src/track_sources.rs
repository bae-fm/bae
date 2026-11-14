//! Work over a release's tracks that holds each source file open only while
//! the tracks reading it are running.
//!
//! A track reads one or more source files: a file of its own, or the CUE
//! image it shares with the rest of its disc (plus, for an audio pregap, the
//! file before). [`run_tracks_over_sources`] takes its places of the
//! open-files budget in one wait, runs up to `parallelism` tracks at once,
//! admitting them in track order, and keeps open exactly the files those
//! tracks read. A file opens when the first track reading it starts and
//! closes once no running track reads it and the next track waiting does not
//! either. Tracks of one CUE image running together share its one open stream,
//! each decoding through its own reader over it, so an image counts once
//! however many of its tracks are in flight. A track is admitted only while
//! the files open stay within the pass's places, which are at least as many
//! as the most files one track reads, so a pass over a whole box set holds at
//! most its places' worth of files open whatever its track count.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::num::NonZeroUsize;

use crate::data_source::{AudioDataReader, FillErrorHandler, FillTask};
use bae_audio::sparse_buffer::create_sparse_buffer;
use bae_audio::sparse_buffer::SharedSparseBuffer;
use bae_store::FileLeases;
use bae_util::task_failure::{caught, TaskFailed};
use coven::{FileLease, LeaseTooLarge};

/// One track's work, as its failure names it.
const TRACK_WORK: &str = "work on one track";

/// One source file open for the tracks reading it: the buffer its bytes
/// stream through and the fill task that holds the file.
///
/// Dropped without being closed — the pass it belongs to was itself dropped,
/// as a cancelled import's is — it stops the stream all the same. The tracks
/// decoding it run on blocking threads that nothing can abort, and what stops
/// them is their next read finding the stream stopped; left running, they
/// would go on decoding, and reporting, for a pass that no longer exists.
pub struct SourceStream {
    buffer: StopOnDrop,
    fill: FillTask,
}

/// A stream's buffer, stopped when the stream lets go of it.
struct StopOnDrop(SharedSparseBuffer);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl SourceStream {
    /// Start streaming a source of `size` bytes through `reader`.
    pub fn start(reader: Box<dyn AudioDataReader>, size: u64, on_error: FillErrorHandler) -> Self {
        let buffer = create_sparse_buffer(size);
        let fill = reader.start_reading(buffer.clone(), on_error);
        Self { buffer: StopOnDrop(buffer), fill }
    }

    /// Stop the stream and wait for its fill to end, so the file is closed
    /// when this returns.
    async fn close(self) {
        let Self { buffer, fill } = self;
        drop(buffer);
        fill.ended().await;
    }
}

/// Run `work` over each of `tracks`, up to `parallelism` at once, handing each
/// the open streams of the files `files_of` says it reads, keyed as
/// `files_of` names them. `open` opens a file the first time a running track
/// needs it, in a place of the lease it is given.
///
/// The pass takes its places from `budget` in one wait before it opens
/// anything: `parallelism` of them, or the budget's size when that is less,
/// and at least as many as the most files one track reads. A track that reads
/// more files than the budget has fails the pass with the error `too_wide`
/// makes of it, before any file opens. The pass runs no more tracks than it
/// has places and keeps no more files open. Results come back in track order.
///
/// The first error stops admitting tracks; the tracks already running finish,
/// every file is closed, and that error is returned. Every file is closed
/// before this returns either way. A track whose work panics fails the pass
/// with the error `panicked` makes of it, which is where the caller reports
/// it.
pub async fn run_tracks_over_sources<K, T, R, E, Fut>(
    tracks: Vec<T>,
    budget: &FileLeases,
    parallelism: NonZeroUsize,
    files_of: impl Fn(&T) -> Vec<K>,
    open: impl Fn(&K, &FileLease) -> SourceStream,
    work: impl Fn(T, HashMap<K, SharedSparseBuffer>) -> Fut,
    panicked: impl Fn(TaskFailed) -> E,
    too_wide: impl Fn(LeaseTooLarge) -> E,
) -> Result<Vec<R>, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Fut: Future<Output = Result<R, E>> + Send + 'static,
{
    let track_count = tracks.len();
    if track_count == 0 {
        return Ok(Vec::new());
    }
    let waiting = tracks
        .into_iter()
        .map(|track| {
            let mut files: Vec<K> = Vec::new();
            for file in files_of(&track) {
                if !files.contains(&file) {
                    files.push(file);
                }
            }
            (track, files)
        })
        .collect::<Vec<_>>();
    let widest = waiting.iter().map(|(_, files)| files.len()).max().unwrap_or(0);
    let places = parallelism.get().min(budget.places()).max(widest);
    let lease = budget.lease(places).await.map_err(&too_wide)?;
    let parallelism = parallelism.get().min(places);
    let mut waiting = waiting.into_iter().enumerate().peekable();
    // Each open file with the number of running tracks reading it.
    let mut open_files: HashMap<K, (SourceStream, usize)> = HashMap::new();
    let mut running = tokio::task::JoinSet::new();
    let mut results: Vec<Option<R>> = (0..track_count).map(|_| None).collect();
    let mut failure: Option<E> = None;

    loop {
        while failure.is_none() && running.len() < parallelism {
            let Some((_, (_, files))) = waiting.peek() else {
                break;
            };
            let opening = files.iter().filter(|file| !open_files.contains_key(*file)).count();
            if !running.is_empty() && open_files.len() + opening > places {
                break;
            }
            let (index, (track, files)) = waiting.next().expect("peeked above");
            let mut streams = HashMap::with_capacity(files.len());
            for file in &files {
                let (stream, readers) =
                    open_files.entry(file.clone()).or_insert_with(|| (open(file, &lease), 0));
                *readers += 1;
                streams.insert(file.clone(), stream.buffer.0.clone());
            }
            let task = work(track, streams);
            #[expect(
                clippy::disallowed_methods,
                reason = "each task catches its work's panic, and the loop joins every task in the set"
            )]
            running.spawn(async move { (index, files, caught(TRACK_WORK, task).await) });
        }

        let Some(joined) = running.join_next().await else {
            break;
        };
        // Each task catches its work's panic, and nothing aborts the set
        // while it is joined, so only its runtime ending can fail the join.
        let (index, files, outcome) = match joined {
            Ok(joined) => joined,
            Err(error) => {
                failure.get_or_insert(panicked(TaskFailed::of_join(TRACK_WORK, error)));
                continue;
            }
        };
        match outcome {
            Ok(Ok(result)) => results[index] = Some(result),
            Ok(Err(error)) => {
                failure.get_or_insert(error);
            }
            Err(bug) => {
                failure.get_or_insert(panicked(TaskFailed::Panicked(bug)));
            }
        }
        for file in &files {
            open_files.get_mut(file).expect("a running track's files are open").1 -= 1;
        }
        // A file the next waiting track reads stays open for it, so a CUE
        // image is not closed and reopened between its tracks.
        let next_files: &[K] = match (&failure, waiting.peek()) {
            (None, Some((_, (_, files)))) => files,
            _ => &[],
        };
        let idle: Vec<K> = open_files
            .iter()
            .filter(|(file, (_, readers))| *readers == 0 && !next_files.contains(file))
            .map(|(file, _)| file.clone())
            .collect();
        for file in idle {
            let (stream, _) = open_files.remove(&file).expect("listed above");
            stream.close().await;
        }
    }

    for (_, (stream, _)) in open_files.drain() {
        stream.close().await;
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(results
            .into_iter()
            .map(|result| result.expect("every admitted track finished"))
            .collect()),
    }
}

#[cfg(test)]
#[path = "track_sources_tests.rs"]
mod tests;
