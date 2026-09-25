//! Work over a release's tracks that holds each source file open only while
//! the tracks reading it are running.
//!
//! A track reads one or more source files: a file of its own, or the CUE
//! image it shares with the rest of its disc (plus, for an audio pregap, the
//! file before). [`run_tracks_over_sources`] runs up to `parallelism` tracks
//! at once, admitting them in track order, and keeps open exactly the files
//! those tracks read. A file opens when the first track reading it starts and
//! closes once no running track reads it and the next track waiting does not
//! either. Tracks of one CUE image running together share its one open stream,
//! each decoding through its own reader over it, so an image counts once
//! however many of its tracks are in flight. A track is admitted only while
//! the files open stay within `parallelism` — a track reading more files than
//! that runs alone — so a pass over a whole box set holds at most
//! `parallelism` files open whatever its track count.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::num::NonZeroUsize;

use crate::playback::data_source::{AudioDataReader, FillErrorHandler, FillTask};
use crate::playback::sparse_buffer::create_sparse_buffer;
use crate::playback::SharedSparseBuffer;

/// One source file open for the tracks reading it: the buffer its bytes
/// stream through and the fill task that holds the file.
pub(crate) struct SourceStream {
    buffer: SharedSparseBuffer,
    fill: FillTask,
}

impl SourceStream {
    /// Start streaming a source of `size` bytes through `reader`.
    pub(crate) fn start(
        reader: Box<dyn AudioDataReader>,
        size: u64,
        on_error: FillErrorHandler,
    ) -> Self {
        let buffer = create_sparse_buffer(size);
        let fill = reader.start_reading(buffer.clone(), on_error);
        Self { buffer, fill }
    }

    /// Stop the stream and wait for its fill to end, so the file is closed
    /// when this returns.
    async fn close(self) {
        let Self { buffer, fill } = self;
        buffer.cancel();
        drop(buffer);
        fill.ended().await;
    }
}

/// Run `work` over each of `tracks`, up to `parallelism` at once, handing each
/// the open streams of the files `files_of` says it reads, keyed as
/// `files_of` names them. `open` opens a file the first time a running track
/// needs it. Results come back in track order.
///
/// The first error stops admitting tracks; the tracks already running finish,
/// every file is closed, and that error is returned. Every file is closed
/// before this returns either way.
pub(crate) async fn run_tracks_over_sources<K, T, R, E, Fut>(
    tracks: Vec<T>,
    parallelism: NonZeroUsize,
    files_of: impl Fn(&T) -> Vec<K>,
    open: impl Fn(&K) -> SourceStream,
    work: impl Fn(T, HashMap<K, SharedSparseBuffer>) -> Fut,
) -> Result<Vec<R>, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Fut: Future<Output = Result<R, E>> + Send + 'static,
{
    let parallelism = parallelism.get();
    let track_count = tracks.len();
    let mut waiting = tracks
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
        .enumerate()
        .peekable();
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
            let opening = files
                .iter()
                .filter(|file| !open_files.contains_key(*file))
                .count();
            if !running.is_empty() && open_files.len() + opening > parallelism {
                break;
            }
            let (index, (track, files)) = waiting.next().expect("peeked above");
            let mut streams = HashMap::with_capacity(files.len());
            for file in &files {
                let (stream, readers) = open_files
                    .entry(file.clone())
                    .or_insert_with(|| (open(file), 0));
                *readers += 1;
                streams.insert(file.clone(), stream.buffer.clone());
            }
            let task = work(track, streams);
            running.spawn(async move { (index, files, task.await) });
        }

        let Some(joined) = running.join_next().await else {
            break;
        };
        let (index, files, outcome) = match joined {
            Ok(joined) => joined,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        };
        match outcome {
            Ok(result) => results[index] = Some(result),
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
        for file in &files {
            open_files
                .get_mut(file)
                .expect("a running track's files are open")
                .1 -= 1;
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
