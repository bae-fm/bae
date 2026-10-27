//! Transfer rates from provider-confirmed upload counters.
use crate::db::DbSendingUploads;
use coven::FileId;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

const WINDOW: Duration = Duration::from_secs(10);

struct Measurement {
    samples: VecDeque<(Duration, u64)>,
    started: Duration,
    confirmed: u64,
    total: u64,
}
#[derive(Default)]
pub(crate) struct UploadThroughput {
    files: HashMap<FileId, Measurement>,
}

impl UploadThroughput {
    /// Measure the files being sent at `now` from the bytes confirmed of
    /// them. A file no longer sent, or sent in one request, has no rate; a
    /// pause ends every measurement.
    pub(crate) fn observe(&mut self, sending: &DbSendingUploads, now: Duration) {
        if sending.paused {
            self.files.clear();
            return;
        }
        let bytes = |value: i64| u64::try_from(value).expect("an upload session's bytes");
        let measured = || {
            sending.files.iter().filter_map(|file| {
                file.bytes_confirmed
                    .zip(file.bytes_total)
                    .map(|(confirmed, total)| (file.file, bytes(confirmed), bytes(total)))
            })
        };
        self.files
            .retain(|id, _| measured().any(|(file, _, _)| file == *id));
        for (file, confirmed, total) in measured() {
            self.record(file, confirmed, total, now);
        }
    }
    fn record(&mut self, id: FileId, confirmed: u64, total: u64, now: Duration) {
        let measurement = self.files.entry(id).or_insert_with(|| Measurement {
            samples: VecDeque::new(),
            started: now,
            confirmed,
            total,
        });
        if confirmed < measurement.confirmed || total != measurement.total {
            *measurement = Measurement {
                samples: VecDeque::new(),
                started: now,
                confirmed,
                total,
            };
        }
        let delta = confirmed - measurement.confirmed;
        if delta > 0 {
            measurement.samples.push_back((now, delta));
        }
        measurement.confirmed = confirmed;
    }
    pub(crate) fn rates(&mut self, now: Duration) -> HashMap<FileId, u64> {
        self.files
            .iter_mut()
            .map(|(id, measurement)| {
                while measurement.samples.front().is_some_and(|(time, _)| {
                    now.checked_sub(*time).expect("upload clock regressed") > WINDOW
                }) {
                    measurement.samples.pop_front();
                }
                let elapsed = now
                    .checked_sub(measurement.started)
                    .expect("upload clock regressed")
                    .min(WINDOW);
                let bytes = measurement
                    .samples
                    .iter()
                    .try_fold(0u64, |total, (_, bytes)| total.checked_add(*bytes))
                    .expect("upload bytes overflow");
                let rate = if elapsed.is_zero() {
                    0
                } else {
                    (bytes as f64 / elapsed.as_secs_f64()) as u64
                };
                (*id, rate)
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "upload_throughput_tests.rs"]
mod tests;
