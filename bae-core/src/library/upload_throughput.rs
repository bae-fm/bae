//! Transfer rates from the bytes coven's upload watch tells each file has
//! handed to the connection.
use crate::db::DbSendingUploads;
use coven::FileId;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Duration;

const WINDOW: Duration = Duration::from_secs(10);

struct Measurement {
    samples: VecDeque<(Duration, u64)>,
    started: Duration,
    transferred: u64,
    total: u64,
}
#[derive(Default)]
pub(crate) struct UploadThroughput {
    files: HashMap<FileId, Measurement>,
}

impl UploadThroughput {
    /// Measure the files `sending` lists at `now` from how far `progress`
    /// has each. A file no longer sent, or not yet told, has no rate; a
    /// pause ends every measurement.
    pub(crate) fn observe(
        &mut self,
        sending: &DbSendingUploads,
        progress: &BTreeMap<FileId, coven::TransferProgress>,
        now: Duration,
    ) {
        if sending.paused {
            self.files.clear();
            return;
        }
        let measured = || {
            sending.files.iter().filter_map(|file| {
                progress
                    .get(&file.file)
                    .map(|moved| (file.file, moved.bytes_transferred, moved.bytes_total))
            })
        };
        self.files
            .retain(|id, _| measured().any(|(file, _, _)| file == *id));
        for (file, transferred, total) in measured() {
            self.record(file, transferred, total, now);
        }
    }
    fn record(&mut self, id: FileId, transferred: u64, total: u64, now: Duration) {
        let measurement = self.files.entry(id).or_insert_with(|| Measurement {
            samples: VecDeque::new(),
            started: now,
            transferred,
            total,
        });
        if transferred < measurement.transferred || total != measurement.total {
            *measurement = Measurement {
                samples: VecDeque::new(),
                started: now,
                transferred,
                total,
            };
        }
        let delta = transferred - measurement.transferred;
        if delta > 0 {
            measurement.samples.push_back((now, delta));
        }
        measurement.transferred = transferred;
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
