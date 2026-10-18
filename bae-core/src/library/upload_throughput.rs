//! Transfer rates from provider-confirmed upload counters.
use coven::{FileId, UploadPhase, UploadQueue};
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
    pub(crate) fn observe(&mut self, queue: &UploadQueue, now: Duration) {
        if queue.paused {
            self.files.clear();
            return;
        }
        self.files.retain(|id, _| {
            queue.files.iter().any(|upload| {
                upload.file.id() == *id && matches!(upload.phase, UploadPhase::Uploading { .. })
            })
        });
        for upload in &queue.files {
            let UploadPhase::Uploading {
                bytes_confirmed,
                bytes_total,
            } = upload.phase
            else {
                continue;
            };
            self.record(upload.file.id(), bytes_confirmed, bytes_total, now);
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
