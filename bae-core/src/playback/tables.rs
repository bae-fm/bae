//! Playback's own tables in the library's store: the queue's lanes and its
//! row, the device-local resume row, and the `playback_now` and
//! `playback_timeline` rows the screens read what playback is doing from,
//! with the reads that show a queued or playing track. Their SQL is
//! functions over coven's write and read contexts; [`PlaybackQueueTables`],
//! [`PlaybackNowRow`] and [`PlaybackTimelineRow`] are what the playback
//! service writes them through, and the functions here read them for
//! everything else.

mod now;
mod queue;
mod queue_reads;
mod resume;
mod timeline;
mod track_display;

#[cfg(any(test, feature = "test-utils"))]
pub use now::now_playing;
pub(crate) use now::subscribe_now_playing;
pub use now::{playback_level, PlaybackNowRow};
pub use queue::{
    settle_playback_queue, ContextFill, ContextFilled, ManualPlace, PlaybackQueueTables,
    ReleaseStart, ShuffleChange,
};
pub use queue_reads::queue_value;
#[cfg(any(test, feature = "test-utils"))]
pub use queue_reads::{queue_addition, queue_upcoming};
pub(crate) use queue_reads::{
    subscribe_queue_addition, subscribe_queue_upcoming, subscribe_queue_value, QueueUpcomingRead,
};
pub use resume::{
    clear_playback_state, load_playback_state, save_playback_state, PlaybackResumeRow,
};
#[cfg(any(test, feature = "test-utils"))]
pub use timeline::playback_timeline;
pub(crate) use timeline::subscribe_playback_timeline;
pub use timeline::PlaybackTimelineRow;
pub(crate) use track_display::subscribe_playback_track_infos;
pub use track_display::track_display;
#[cfg(test)]
pub(crate) mod tests;

use crate::db::client::query::*;
use crate::db::client::read::*;
use crate::db::*;
use coven::{CovenError, CovenWrite, SqlReadContext};
use rusqlite::{params, OptionalExtension, Row};
use tracing::warn;
