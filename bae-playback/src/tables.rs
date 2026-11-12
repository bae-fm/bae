//! Playback's own tables in the library's store: the queue's lanes and its
//! row, the device-local resume row, and the `playback_now` and
//! `playback_timeline` rows the screens read what playback is doing from,
//! with the reads that show a queued or playing track. Their SQL is
//! functions over coven's write and read contexts; [`PlaybackQueueTables`],
//! [`PlaybackNowRow`] and [`PlaybackTimelineRow`] are what the playback
//! service writes them through, and the functions here read them for
//! everything else.

mod now;
mod preview_target;
mod queue;
mod queue_fill;
mod queue_reads;
mod resume;
mod timeline;
mod track_display;

#[cfg(any(test, feature = "test-utils"))]
pub use now::now_playing;
pub(crate) use now::remote_receiver_from_row;
pub use now::subscribe_now_playing;
pub(super) use now::SCHEMA as NOW_SCHEMA;
pub use now::{playback_level, PlaybackNowRow};
#[cfg(any(test, feature = "test-utils"))]
pub use queue::fail_queue_writes_for_test;
pub(super) use queue::SCHEMA as QUEUE_SCHEMA;
pub use queue::{settle_playback_queue, ManualPlace, PlaybackQueueTables, ShuffleChange};
pub use queue_fill::{ContextFill, ContextFilled, LeftOut, ReleaseStart};
pub use queue_reads::queue_value;
pub use queue_reads::subscribe_queue_addition;
#[cfg(any(test, feature = "test-utils"))]
pub use queue_reads::{queue_addition, queue_context_version, queue_upcoming};
pub use queue_reads::{subscribe_queue_upcoming, subscribe_queue_value, QueueUpcomingRead};
#[cfg(any(test, feature = "test-utils"))]
pub use resume::subscribe_playback_state;
pub use resume::{load_playback_state, PlaybackResumeRow};
#[cfg(any(test, feature = "test-utils"))]
pub use timeline::playback_timeline;
pub use timeline::subscribe_playback_timeline;
pub use timeline::PlaybackTimelineRow;
pub(super) use timeline::SCHEMA as TIMELINE_SCHEMA;
pub(crate) use track_display::subscribe_playback_track_infos;
pub use track_display::track_display;
#[cfg(test)]
#[path = "tables_tests.rs"]
mod tests;
#[cfg(any(test, feature = "test-utils"))]
pub mod window_cost;

use bae_records::records::query::*;
use bae_records::records::read::*;
use bae_store::read::*;
use bae_store::*;
use coven::{CovenWrite, SqlReadContext};
use rusqlite::{params, OptionalExtension, Row};
use tracing::warn;
