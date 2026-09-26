use super::*;
use crate::open_files_peak::OpenFilesPeak;
use crate::playback::data_source::LocalReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const FILE_BYTES: usize = 64 * 1024;

/// A track of the fixture: its name and the files it reads.
#[derive(Clone)]
struct Track {
    name: String,
    files: Vec<PathBuf>,
}

fn write_file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, vec![name.len() as u8; FILE_BYTES]).unwrap();
    path.canonicalize().unwrap()
}

/// `count` tracks with a file each.
fn file_per_track(dir: &Path, count: usize) -> Vec<Track> {
    (0..count)
        .map(|n| {
            let name = format!("{n:02} Track.flac");
            Track {
                files: vec![write_file(dir, &name)],
                name,
            }
        })
        .collect()
}

/// `count` tracks all reading one image.
fn cue_image(dir: &Path, image: &str, count: usize) -> Vec<Track> {
    let path = write_file(dir, image);
    (0..count)
        .map(|n| Track {
            name: format!("{image} track {n}"),
            files: vec![path.clone()],
        })
        .collect()
}

/// What running a fixture observed.
struct Run {
    names: Vec<String>,
    opens: usize,
    peak_open: usize,
    peak_running: usize,
}

/// Run `tracks` at `parallelism`, each reading every one of its files through
/// its own reader and holding a moment so tracks overlap. The first
/// `parallelism` tracks wait for one another before going on, so the run is
/// seen at its full width however the tasks are scheduled.
async fn run(dir: &Path, tracks: Vec<Track>, parallelism: usize) -> Run {
    let opens = Arc::new(AtomicUsize::new(0));
    let first_batch = Arc::new(tokio::sync::Barrier::new(parallelism));
    let starts = Arc::new(AtomicUsize::new(0));
    let running = Arc::new(AtomicUsize::new(0));
    let peak_running = Arc::new(AtomicUsize::new(0));
    let peak_seen_by_tracks = Arc::new(AtomicUsize::new(0));
    let sampler = OpenFilesPeak::start(dir);

    let names = run_tracks_over_sources(
        tracks,
        NonZeroUsize::new(parallelism).unwrap(),
        |track: &Track| track.files.clone(),
        |path: &PathBuf| {
            opens.fetch_add(1, Ordering::Relaxed);
            SourceStream::start(
                Box::new(LocalReader::new(path)),
                FILE_BYTES as u64,
                Box::new(|_| {}),
            )
        },
        |track, streams| {
            let running = running.clone();
            let peak_running = peak_running.clone();
            let peak_seen_by_tracks = peak_seen_by_tracks.clone();
            let first_batch = first_batch.clone();
            let starts = starts.clone();
            let dir = dir.to_path_buf();
            async move {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak_running.fetch_max(now, Ordering::SeqCst);
                if starts.fetch_add(1, Ordering::SeqCst) < parallelism {
                    first_batch.wait().await;
                }
                tokio::task::spawn_blocking(move || {
                    for path in &track.files {
                        let mut reader = streams[path].new_reader();
                        let mut bytes = vec![0u8; 1024];
                        reader.read(&mut bytes).expect("the stream serves its file");
                        assert_eq!(bytes[0], path.file_name().unwrap().len() as u8);
                    }
                    peak_seen_by_tracks
                        .fetch_max(coven::open_files_under(&dir).len(), Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(15));
                })
                .await
                .unwrap();
                running.fetch_sub(1, Ordering::SeqCst);
                Ok::<_, String>(track.name)
            }
        },
    )
    .await
    .expect("every track runs");

    // Every file is closed by the time the run returns.
    coven::assert_no_open_files_under(dir);
    let peak_open = sampler
        .finish()
        .max(peak_seen_by_tracks.load(Ordering::SeqCst));
    Run {
        names,
        opens: opens.load(Ordering::Relaxed),
        peak_open,
        peak_running: peak_running.load(Ordering::SeqCst),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn file_per_track_holds_at_most_one_file_open_at_parallelism_one() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = file_per_track(dir.path(), 30);
    let expected: Vec<String> = tracks.iter().map(|track| track.name.clone()).collect();

    let run = run(dir.path(), tracks, 1).await;

    assert_eq!(run.names, expected, "results come back in track order");
    assert_eq!(run.peak_running, 1);
    assert_eq!(run.peak_open, 1);
    assert_eq!(run.opens, 30);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn file_per_track_holds_at_most_parallelism_files_open() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = file_per_track(dir.path(), 30);
    let expected: Vec<String> = tracks.iter().map(|track| track.name.clone()).collect();

    let run = run(dir.path(), tracks, 4).await;

    assert_eq!(run.names, expected, "results come back in track order");
    assert_eq!(run.peak_running, 4, "four tracks run at once");
    assert!(run.peak_open <= 4, "{} files open at once", run.peak_open);
}

/// Tracks of one image run together over its one open file, which opens
/// once for the whole run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tracks_of_one_image_run_together_over_one_open_file() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = cue_image(dir.path(), "Album.flac", 20);

    let run = run(dir.path(), tracks, 4).await;

    assert_eq!(run.names.len(), 20);
    assert_eq!(run.peak_running, 4, "the image's tracks run concurrently");
    assert_eq!(run.peak_open, 1);
    assert_eq!(run.opens, 1, "the image stays open between its tracks");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn images_and_files_per_track_together_stay_within_parallelism() {
    let dir = tempfile::tempdir().unwrap();
    let mut tracks = cue_image(dir.path(), "Disc 1.flac", 12);
    tracks.extend(file_per_track(dir.path(), 12));
    tracks.extend(cue_image(dir.path(), "Disc 3.flac", 12));
    let expected: Vec<String> = tracks.iter().map(|track| track.name.clone()).collect();

    for parallelism in [1, 3] {
        let run = run(dir.path(), tracks.clone(), parallelism).await;

        assert_eq!(run.names, expected);
        assert!(
            run.peak_open <= parallelism,
            "{} files open at once at parallelism {parallelism}",
            run.peak_open
        );
        assert_eq!(run.opens, 14, "each image opens once, each track file once");
    }
}

/// A track reading two files (an audio pregap in the file before) opens
/// both, even when that is more files than the parallelism allows at once:
/// it runs alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_track_reading_more_files_than_the_parallelism_runs_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut tracks = file_per_track(dir.path(), 3);
    let pregap_track = Track {
        name: "pregap".to_string(),
        files: vec![tracks[1].files[0].clone(), tracks[2].files[0].clone()],
    };
    tracks.push(pregap_track);

    let run = run(dir.path(), tracks, 1).await;

    assert_eq!(run.names.len(), 4);
    assert_eq!(run.peak_open, 2);
}

/// The first failure stops admitting tracks, lets the running ones finish,
/// closes every file, and is what the run returns.
///
/// Tracks before the failing one finish at once; tracks after it wait until
/// it has failed, so at most the one track admitted beside it can have
/// started, whatever order the tasks are scheduled in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_track_stops_the_run_and_closes_every_file() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = file_per_track(dir.path(), 20);
    let started = Arc::new(Mutex::new(Vec::new()));
    let (failed_tx, failed_rx) = tokio::sync::watch::channel(false);

    let result: Result<Vec<()>, String> = run_tracks_over_sources(
        tracks,
        NonZeroUsize::new(2).unwrap(),
        |track: &Track| track.files.clone(),
        |path: &PathBuf| {
            SourceStream::start(
                Box::new(LocalReader::new(path)),
                FILE_BYTES as u64,
                Box::new(|_| {}),
            )
        },
        |track, _streams| {
            let started = started.clone();
            let failed_tx = failed_tx.clone();
            let mut failed_rx = failed_rx.clone();
            async move {
                started.lock().unwrap().push(track.name.clone());
                let number: usize = track.name[..2].parse().unwrap();
                match number {
                    0..=2 => Ok(()),
                    3 => {
                        failed_tx.send_replace(true);
                        Err(format!("{} failed", track.name))
                    }
                    _ => {
                        failed_rx.wait_for(|failed| *failed).await.unwrap();
                        Ok(())
                    }
                }
            }
        },
    )
    .await;

    assert_eq!(result, Err("03 Track.flac failed".to_string()));
    let started = started.lock().unwrap().clone();
    assert!(
        started.len() <= 5 && !started.iter().any(|name| name.starts_with("05")),
        "tracks started past the one beside the failure: {started:?}"
    );
    coven::assert_no_open_files_under(dir.path());
}
